//! Le filtre d'appels système du mur : un programme seccomp-BPF classique, rendu en octets, sans appeler le système.
//! Celui qui lance la main l'installe avant de lui laisser la main ; ce fichier ne fait que l'écrire.
//!
//! Le format est celui du noyau : des instructions de 8 octets (`struct sock_filter` : code sur 16 bits, deux sauts
//! sur 8 bits, constante sur 32 bits), dans l'ordre des octets de la machine. Il ne lit que `struct seccomp_data` :
//! le numéro d'appel (offset 0), l'architecture (offset 4) et le premier argument (offset 16).
//!
//! Ce que le filtre décide, dans cet ordre :
//!
//! - une architecture autre que celle attendue : le processus est tué ;
//! - sur x86_64, un numéro d'appel de l'ABI x32 (`0x40000000` et plus) : le processus est tué, car ces appels portent
//!   d'autres numéros que ceux que le filtre regarde ;
//! - `io_uring_setup`, `io_uring_enter`, `io_uring_register` (425, 426, 427) : refus `EPERM` ; `io_uring` fait des
//!   opérations sans passer par les appels que ce filtre regarde ;
//! - `socket` dont la moitié basse du premier argument vaut `AF_UNIX` : refus `EPERM`, quelle que soit la moitié
//!   haute (le noyau ne lit que la moitié basse : c'est un `int`) ;
//! - tout le reste passe.

#![cfg_attr(not(test), expect(dead_code))]
// raccourci : rien n'appelle encore cette fonction ; l'appelant qui installe le filtre avant la main la prendra, et
// rustc dira alors cette attente inutile

/// L'architecture que le filtre garde : celle du processus de la main.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Architecture {
    X86_64,
    Aarch64,
}

impl Architecture {
    /// Le `AUDIT_ARCH_*` du noyau : le numéro de machine ELF, 64 bits, petit-boutiste.
    fn audit(self) -> u32 {
        match self {
            Architecture::X86_64 => 0xC000_003E,
            Architecture::Aarch64 => 0xC000_00B7,
        }
    }

    /// Le numéro de `socket`.
    fn socket(self) -> u32 {
        match self {
            Architecture::X86_64 => 41,
            Architecture::Aarch64 => 198,
        }
    }
}

// Les instructions de BPF classique que le filtre emploie (`linux/filter.h`).
/// `BPF_LD | BPF_W | BPF_ABS` : charge un mot de 32 bits de `seccomp_data`.
const LD_W_ABS: u16 = 0x20;
/// `BPF_JMP | BPF_JEQ | BPF_K` : saute selon que le mot est égal à la constante.
const JEQ_K: u16 = 0x15;
/// `BPF_JMP | BPF_JGE | BPF_K` : saute selon que le mot est au moins la constante, sans signe.
const JGE_K: u16 = 0x35;
/// `BPF_RET | BPF_K` : rend la constante, le verdict.
const RET_K: u16 = 0x06;

// Les offsets de `struct seccomp_data { int nr; __u32 arch; __u64 instruction_pointer; __u64 args[6]; }`.
const OFFSET_NUMERO: u32 = 0;
const OFFSET_ARCHITECTURE: u32 = 4;
/// La moitié basse de `args[0]` : au début de l'argument sur une machine petit-boutiste, à la fin sur une
/// grand-boutiste.
const OFFSET_ARGUMENT_BAS: u32 = if cfg!(target_endian = "big") { 20 } else { 16 };

// Les verdicts (`linux/seccomp.h`).
const TUE_LE_PROCESSUS: u32 = 0x8000_0000;
/// `SECCOMP_RET_ERRNO` avec `EPERM` (1) dans les 16 bits bas.
const REFUSE_EPERM: u32 = 0x0005_0001;
const PERMET: u32 = 0x7FFF_0000;

/// Le premier numéro de l'ABI x32 : le bit `__X32_SYSCALL_BIT`.
const X32: u32 = 0x4000_0000;
/// `io_uring_setup`, `io_uring_enter`, `io_uring_register`, les mêmes sur x86_64 et aarch64.
const IO_URING: [u32; 3] = [425, 426, 427];
/// `AF_UNIX`, valeur de la famille de `socket`.
const AF_UNIX: u32 = 1;

/// Où mène un saut : à l'étape suivante, ou à l'un des trois verdicts que le programme porte à sa fin.
#[derive(Clone, Copy)]
enum Cible {
    Suite,
    Tue,
    Refuse,
    Permet,
}

/// Une étape du programme, avant que les sauts n'aient des offsets.
enum Etape {
    /// Charge le mot de `seccomp_data` à cet offset.
    Charge(u32),
    /// Si le mot est égal à la constante, va à la première cible, sinon à la seconde.
    Egal(u32, Cible, Cible),
    /// Si le mot est au moins la constante, va à la première cible, sinon à la seconde.
    AuMoins(u32, Cible, Cible),
}

/// Le filtre de `architecture`, en instructions de 8 octets dans l'ordre des octets de la machine.
pub(crate) fn filtre(architecture: Architecture) -> Vec<u8> {
    let mut etapes = vec![
        Etape::Charge(OFFSET_ARCHITECTURE),
        Etape::Egal(architecture.audit(), Cible::Suite, Cible::Tue),
        Etape::Charge(OFFSET_NUMERO),
    ];
    if architecture == Architecture::X86_64 {
        etapes.push(Etape::AuMoins(X32, Cible::Tue, Cible::Suite));
    }
    for numero in IO_URING {
        etapes.push(Etape::Egal(numero, Cible::Refuse, Cible::Suite));
    }
    etapes.push(Etape::Egal(architecture.socket(), Cible::Suite, Cible::Permet));
    etapes.push(Etape::Charge(OFFSET_ARGUMENT_BAS));
    etapes.push(Etape::Egal(AF_UNIX, Cible::Refuse, Cible::Permet));

    // Les trois verdicts suivent les étapes, dans cet ordre.
    let fin = etapes.len();
    let saut = |cible: Cible, depuis: usize| {
        let arrivee = match cible {
            Cible::Suite => depuis + 1,
            Cible::Tue => fin,
            Cible::Refuse => fin + 1,
            Cible::Permet => fin + 2,
        };
        // un programme de quelques instructions n'approche pas 255 ; si cela changeait, le saut sortirait du programme
        // et le noyau refuserait de le charger, plutôt que de laisser passer ce qu'il devait tenir
        u8::try_from(arrivee - (depuis + 1)).unwrap_or(u8::MAX)
    };

    let mut octets = Vec::with_capacity((fin + 3) * 8);
    let mut instruction = |code: u16, jt: u8, jf: u8, k: u32| {
        octets.extend(code.to_ne_bytes());
        octets.extend([jt, jf]);
        octets.extend(k.to_ne_bytes());
    };
    for (position, etape) in etapes.iter().enumerate() {
        match *etape {
            Etape::Charge(offset) => instruction(LD_W_ABS, 0, 0, offset),
            Etape::Egal(k, alors, sinon) => instruction(JEQ_K, saut(alors, position), saut(sinon, position), k),
            Etape::AuMoins(k, alors, sinon) => instruction(JGE_K, saut(alors, position), saut(sinon, position), k),
        }
    }
    instruction(RET_K, 0, 0, TUE_LE_PROCESSUS);
    instruction(RET_K, 0, 0, REFUSE_EPERM);
    instruction(RET_K, 0, 0, PERMET);
    octets
}

#[cfg(test)]
mod tests;
