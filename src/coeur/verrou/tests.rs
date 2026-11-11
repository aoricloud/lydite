use super::{Architecture, filtre};

// Les valeurs que les tests écrivent en dur, d'après les en-têtes du noyau : elles ne viennent pas du code qu'ils
// jugent, sans quoi une faute de copie le laisserait juger juste.
const AUDIT_X86_64: u32 = 0xC000_003E;
const AUDIT_AARCH64: u32 = 0xC000_00B7;
const AUDIT_I386: u32 = 0x4000_0003;
const AUDIT_ARM: u32 = 0x4000_0028;
const TUE: u32 = 0x8000_0000;
const EPERM: u32 = 0x0005_0001;
const PERMIS: u32 = 0x7FFF_0000;

// Les seules instructions que l'interprète connaît : celles que le filtre emploie, pas plus.
const LD_W_ABS: u16 = 0x20;
const JEQ_K: u16 = 0x15;
const JGE_K: u16 = 0x35;
const RET_K: u16 = 0x06;
/// `sizeof(struct seccomp_data)`.
const TAILLE_DONNEES: usize = 64;

/// Ce que l'interprète refuse, au chargement comme à l'exécution.
#[derive(Debug, PartialEq, Eq)]
enum Defaut {
    /// Aucune instruction, ou des octets qui ne font pas un nombre entier d'instructions de 8 octets.
    Taille(usize),
    /// L'instruction de cette position saute hors du programme, par l'une ou l'autre de ses branches.
    SautHorsProgramme(usize),
    /// La dernière instruction ne rend pas de verdict : le programme pourrait finir sans verdict.
    SansVerdictFinal,
    /// Un code d'instruction que le filtre n'emploie pas.
    Inconnue(u16),
    /// Un chargement non aligné, ou hors de `seccomp_data`.
    Chargement(u32),
}

struct Instruction {
    code: u16,
    jt: u8,
    jf: u8,
    k: u32,
}

/// Un programme chargé. Le noyau vérifie les sauts de tout le programme au chargement, qu'une branche soit prise ou
/// non : l'interprète aussi.
struct Programme(Vec<Instruction>);

impl Programme {
    /// Lit les instructions, dans l'ordre des octets de la machine, et les vérifie.
    fn charger(octets: &[u8]) -> Result<Programme, Defaut> {
        let (morceaux, reste) = octets.as_chunks::<8>();
        if morceaux.is_empty() || !reste.is_empty() {
            return Err(Defaut::Taille(octets.len()));
        }
        let instructions: Vec<Instruction> = morceaux
            .iter()
            .map(|i| Instruction {
                code: u16::from_ne_bytes([i[0], i[1]]),
                jt: i[2],
                jf: i[3],
                k: u32::from_ne_bytes([i[4], i[5], i[6], i[7]]),
            })
            .collect();
        for (position, instruction) in instructions.iter().enumerate() {
            match instruction.code {
                LD_W_ABS => {
                    let debut = instruction.k as usize;
                    if !debut.is_multiple_of(4) || debut + 4 > TAILLE_DONNEES {
                        return Err(Defaut::Chargement(instruction.k));
                    }
                }
                JEQ_K | JGE_K => {
                    for saut in [instruction.jt, instruction.jf] {
                        if position + 1 + usize::from(saut) >= instructions.len() {
                            return Err(Defaut::SautHorsProgramme(position));
                        }
                    }
                }
                RET_K => {}
                autre => return Err(Defaut::Inconnue(autre)),
            }
        }
        match instructions.last() {
            Some(derniere) if derniere.code == RET_K => Ok(Programme(instructions)),
            _ => Err(Defaut::SansVerdictFinal),
        }
    }

    /// Le verdict du programme sur ces `donnees`, la forme d'une `seccomp_data` dans l'ordre des octets de la machine.
    fn jouer(&self, donnees: &[u8; TAILLE_DONNEES]) -> Result<u32, Defaut> {
        let (mut mot, mut position) = (0u32, 0usize);
        // chaque saut va en avant : la boucle finit sur un verdict, ou sur la fin du programme que `charger` exclut
        while let Some(instruction) = self.0.get(position) {
            position += 1;
            let saut = |vrai: bool| usize::from(if vrai { instruction.jt } else { instruction.jf });
            match instruction.code {
                LD_W_ABS => {
                    let debut = instruction.k as usize;
                    mot = u32::from_ne_bytes(donnees[debut..debut + 4].try_into().unwrap());
                }
                JEQ_K => position += saut(mot == instruction.k),
                JGE_K => position += saut(mot >= instruction.k),
                RET_K => return Ok(instruction.k),
                autre => return Err(Defaut::Inconnue(autre)),
            }
        }
        Err(Defaut::SansVerdictFinal)
    }
}

/// Des instructions `(code, jt, jf, k)`, en octets dans l'ordre de la machine.
fn octets(instructions: &[(u16, u8, u8, u32)]) -> Vec<u8> {
    let mut octets = Vec::new();
    for &(code, jt, jf, k) in instructions {
        octets.extend(code.to_ne_bytes());
        octets.extend([jt, jf]);
        octets.extend(k.to_ne_bytes());
    }
    octets
}

/// Une `struct seccomp_data { int nr; __u32 arch; __u64 instruction_pointer; __u64 args[6]; }`.
fn donnees(architecture: u32, numero: u32, arguments: [u64; 6]) -> [u8; TAILLE_DONNEES] {
    let mut donnees = [0u8; TAILLE_DONNEES];
    donnees[0..4].copy_from_slice(&numero.to_ne_bytes());
    donnees[4..8].copy_from_slice(&architecture.to_ne_bytes());
    for (i, argument) in arguments.iter().enumerate() {
        donnees[16 + 8 * i..24 + 8 * i].copy_from_slice(&argument.to_ne_bytes());
    }
    donnees
}

/// Un appel de la table : ce que le filtre voit, et le verdict que le mur doit en tirer.
struct Appel {
    nom: String,
    architecture: u32,
    numero: u32,
    arguments: [u64; 6],
    verdict: u32,
}

/// Une machine que le filtre garde, avec les numéros d'appel qui changent de l'une à l'autre.
struct Machine {
    nom: &'static str,
    architecture: Architecture,
    audit: u32,
    socket: u32,
    socketpair: u32,
    /// Le numéro de `socket` de l'autre machine : sur celle-ci, un autre appel.
    socket_de_l_autre: u32,
    /// `read`, `write`, `openat`, `execve`.
    ordinaires: [u32; 4],
    /// Si l'ABI x32 existe sur cette machine.
    x32: bool,
}

const MACHINES: [Machine; 2] = [
    Machine {
        nom: "x86_64",
        architecture: Architecture::X86_64,
        audit: AUDIT_X86_64,
        socket: 41,
        socketpair: 53,
        socket_de_l_autre: 198,
        ordinaires: [0, 1, 257, 59],
        x32: true,
    },
    Machine {
        nom: "aarch64",
        architecture: Architecture::Aarch64,
        audit: AUDIT_AARCH64,
        socket: 198,
        socketpair: 199,
        socket_de_l_autre: 41,
        ordinaires: [63, 64, 56, 221],
        x32: false,
    },
];

/// La table d'appels d'une machine : (architecture, numéro, arguments) → verdict.
fn table(m: &Machine) -> Vec<Appel> {
    let mut lignes = Vec::new();
    let mut ajouter = |nom: &str, architecture: u32, numero: u32, arguments: [u64; 6], verdict: u32| {
        lignes.push(Appel { nom: format!("{} : {nom}", m.nom), architecture, numero, arguments, verdict });
    };
    let a = m.audit;
    // `socket(famille, SOCK_STREAM, 0)`
    let famille = |famille: u64| [famille, 1, 0, 0, 0, 0];

    ajouter("socket(AF_UNIX)", a, m.socket, famille(1), EPERM);
    ajouter("socket(AF_UNIX, SOCK_STREAM | SOCK_CLOEXEC)", a, m.socket, [1, 0x8_0001, 0, 0, 0, 0], EPERM);
    ajouter("socket(AF_INET)", a, m.socket, famille(2), PERMIS);
    ajouter("socket(AF_INET6)", a, m.socket, famille(10), PERMIS);
    ajouter("socket(AF_NETLINK)", a, m.socket, famille(16), PERMIS);
    ajouter("socket(AF_INET), un dans chaque autre argument", a, m.socket, [2, 1, 1, 1, 1, 1], PERMIS);
    ajouter("socket, moitié haute pleine et basse à AF_UNIX", a, m.socket, famille(0xFFFF_FFFF_0000_0001), EPERM);
    ajouter("socket, moitié haute pleine et basse à AF_INET", a, m.socket, famille(0xFFFF_FFFF_0000_0002), PERMIS);
    ajouter("socket, moitié haute à un et basse à zéro", a, m.socket, famille(0x1_0000_0000), PERMIS);
    ajouter("socketpair(AF_UNIX)", a, m.socketpair, famille(1), PERMIS);
    ajouter("le numéro de socket de l'autre machine, avec AF_UNIX", a, m.socket_de_l_autre, famille(1), PERMIS);

    for (nom, numero) in [("io_uring_setup", 425), ("io_uring_enter", 426), ("io_uring_register", 427)] {
        ajouter(nom, a, numero, [0; 6], EPERM);
    }
    ajouter("pidfd_send_signal, 424, juste avant io_uring", a, 424, [0; 6], PERMIS);
    ajouter("open_tree, 428, juste après io_uring", a, 428, [0; 6], PERMIS);

    // write(1, …) porte 1 en premier argument : seul `socket` regarde AF_UNIX
    for (nom, numero) in ["read", "write", "openat", "execve"].into_iter().zip(m.ordinaires) {
        ajouter(nom, a, numero, [1, 0, 0, 0, 0, 0], PERMIS);
    }

    let autres = [AUDIT_X86_64, AUDIT_AARCH64, AUDIT_I386, AUDIT_ARM].into_iter().filter(|&autre| autre != a);
    // la même valeur à un bit près compte pour une autre architecture
    for autre in autres.chain([a ^ 0x8000_0000]) {
        let nom = format!("architecture {autre:#010X}");
        ajouter(&format!("{nom}, appel ordinaire"), autre, m.ordinaires[0], [0; 6], TUE);
        ajouter(&format!("{nom}, io_uring_setup"), autre, 425, [0; 6], TUE);
        ajouter(&format!("{nom}, socket(AF_UNIX)"), autre, m.socket, famille(1), TUE);
        ajouter(&format!("{nom}, socket(AF_INET)"), autre, m.socket, famille(2), TUE);
    }

    if m.x32 {
        ajouter("x32, le premier numéro", a, 0x4000_0000, [0; 6], TUE);
        ajouter("x32, socket(AF_INET)", a, 0x4000_0000 | m.socket, famille(2), TUE);
        ajouter("x32, io_uring_enter", a, 0x4000_0000 | 426, [0; 6], TUE);
        ajouter("le dernier numéro sous x32", a, 0x3FFF_FFFF, [0; 6], PERMIS);
        ajouter("un numéro tout à un", a, 0xFFFF_FFFF, [0; 6], TUE);
    } else {
        // la règle x32 est celle de x86_64 : ailleurs, ce numéro est un appel comme un autre
        ajouter("0x40000000", a, 0x4000_0000, [0; 6], PERMIS);
    }
    lignes
}

/// Les noms des appels de `table` dont le verdict de `programme` n'est pas celui de la table. Un programme que
/// l'interprète ne peut pas jouer jusqu'au bout fait échouer le test.
fn ecarts(programme: &Programme, table: &[Appel]) -> Vec<String> {
    let mut ecarts = Vec::new();
    for appel in table {
        match programme.jouer(&donnees(appel.architecture, appel.numero, appel.arguments)) {
            Ok(verdict) if verdict == appel.verdict => {}
            Ok(_) => ecarts.push(appel.nom.clone()),
            Err(defaut) => panic!("{} : {defaut:?}", appel.nom),
        }
    }
    ecarts
}

fn charger_filtre(m: &Machine) -> Programme {
    Programme::charger(&filtre(m.architecture)).unwrap_or_else(|defaut| panic!("{} : {defaut:?}", m.nom))
}

#[test]
fn la_table_des_appels_passe_sur_chaque_machine() {
    for m in &MACHINES {
        assert_eq!(ecarts(&charger_filtre(m), &table(m)), Vec::<String>::new(), "{}", m.nom);
    }
}

#[test]
fn le_filtre_est_en_instructions_de_8_octets_dans_l_ordre_des_octets_de_la_machine() {
    for m in &MACHINES {
        let filtre = filtre(m.architecture);
        assert_eq!(filtre.len() % 8, 0, "{}", m.nom);
        // la première instruction lit l'architecture : mot de 32 bits à l'offset 4
        let premiere = [LD_W_ABS.to_ne_bytes().as_slice(), &[0, 0], 4u32.to_ne_bytes().as_slice()].concat();
        assert_eq!(&filtre[..8], premiere.as_slice(), "{}", m.nom);
    }
}

/// Les positions des comparaisons du programme (`JEQ`, `JGE`).
fn comparaisons(programme: &Programme) -> Vec<usize> {
    let comparaison = |i: &Instruction| matches!(i.code, JEQ_K | JGE_K);
    (0..programme.0.len()).filter(|&position| comparaison(&programme.0[position])).collect()
}

#[test]
fn sans_la_regle_af_unix_la_table_rougit() {
    for m in &MACHINES {
        let octets = filtre(m.architecture);
        let programme = Programme::charger(&octets).unwrap();
        let af_unix: Vec<usize> =
            comparaisons(&programme).into_iter().filter(|&position| programme.0[position].k == 1).collect();
        assert_eq!(af_unix.len(), 1, "{} : une seule comparaison à AF_UNIX", m.nom);

        // la règle disparaît : la branche qui refuse mène où mène l'autre, au verdict qui permet
        let mut mute = octets.clone();
        mute[af_unix[0] * 8 + 2] = mute[af_unix[0] * 8 + 3];

        let table = table(m);
        let refus_de_socket: Vec<String> = table
            .iter()
            .filter(|appel| appel.architecture == m.audit && appel.numero == m.socket && appel.verdict == EPERM)
            .map(|appel| appel.nom.clone())
            .collect();
        assert!(!refus_de_socket.is_empty(), "{} : la table refuse au moins un socket", m.nom);
        assert_eq!(ecarts(&Programme::charger(&mute).unwrap(), &table), refus_de_socket, "{}", m.nom);
    }
}

#[test]
fn chaque_constante_du_filtre_est_tenue_par_la_table() {
    for m in &MACHINES {
        let octets = filtre(m.architecture);
        let programme = Programme::charger(&octets).unwrap();
        let comparaisons = comparaisons(&programme);
        // l'architecture, x32 sur x86_64, les trois io_uring, socket, AF_UNIX
        assert_eq!(comparaisons.len(), if m.x32 { 7 } else { 6 }, "{}", m.nom);
        for position in comparaisons {
            let mut mute = octets.clone();
            let k = programme.0[position].k.wrapping_add(1);
            mute[position * 8 + 4..position * 8 + 8].copy_from_slice(&k.to_ne_bytes());
            let rouge = ecarts(&Programme::charger(&mute).unwrap(), &table(m));
            assert!(!rouge.is_empty(), "{} : la table laisse passer la constante de l'instruction {position}", m.nom);
        }
    }
}

#[test]
fn l_interprete_suit_les_deux_branches_d_un_saut() {
    // charge le numéro ; si la comparaison avec 7 est vraie, rend 111, sinon 222
    for (comparaison, pour_huit_et_plus) in [(JEQ_K, 222), (JGE_K, 111)] {
        let programme = [(LD_W_ABS, 0, 0, 0), (comparaison, 0, 1, 7), (RET_K, 0, 0, 111), (RET_K, 0, 0, 222)];
        let programme = Programme::charger(&octets(&programme)).unwrap();
        let verdict = |numero| programme.jouer(&donnees(0, numero, [0; 6]));
        assert_eq!(verdict(7), Ok(111));
        assert_eq!(verdict(6), Ok(222));
        assert_eq!(verdict(8), Ok(pour_huit_et_plus));
        // sans signe : tout à un est plus grand que 7
        assert_eq!(verdict(0xFFFF_FFFF), Ok(pour_huit_et_plus));
    }
}

#[test]
fn l_interprete_refuse_un_saut_qui_sortirait_du_programme() {
    // à la position 0, un saut de n va en 1 + n : le programme n'a que 2 instructions
    let avec_sauts = |jt, jf| octets(&[(JEQ_K, jt, jf, 0), (RET_K, 0, 0, 1)]);
    assert!(Programme::charger(&avec_sauts(0, 0)).is_ok());
    assert_eq!(Programme::charger(&avec_sauts(1, 0)).err(), Some(Defaut::SautHorsProgramme(0)));
    // la branche que ces données ne prennent pas compte aussi : le noyau la vérifie au chargement
    assert_eq!(Programme::charger(&avec_sauts(0, 9)).err(), Some(Defaut::SautHorsProgramme(0)));
}

#[test]
fn l_interprete_refuse_ce_que_le_noyau_refuserait() {
    let verdict = (RET_K, 0, 0, 1);
    assert_eq!(Programme::charger(&[]).err(), Some(Defaut::Taille(0)));
    assert_eq!(Programme::charger(&octets(&[verdict])[..7]).err(), Some(Defaut::Taille(7)));
    assert_eq!(Programme::charger(&octets(&[(LD_W_ABS, 0, 0, 0)])).err(), Some(Defaut::SansVerdictFinal));
    assert_eq!(Programme::charger(&octets(&[(0x07, 0, 0, 0), verdict])).err(), Some(Defaut::Inconnue(0x07)));
    assert_eq!(Programme::charger(&octets(&[(LD_W_ABS, 0, 0, 2), verdict])).err(), Some(Defaut::Chargement(2)));
    assert_eq!(Programme::charger(&octets(&[(LD_W_ABS, 0, 0, 64), verdict])).err(), Some(Defaut::Chargement(64)));
    assert!(Programme::charger(&octets(&[(LD_W_ABS, 0, 0, 60), verdict])).is_ok());
}
