//! La grille d'essais réels du mur : chaque cas joue une commande `sh` dans le vrai mur, par le vrai chemin du rejeu de
//! la preuve ([`rejouer`], sous un profil au mur exigé et borné comme `lancer` le borne), et constate ce qu'un mur doit
//! tenir. Le constat est celui de la grille, non celui du mur essayé : un cas que le mur ne tient pas est un constat de
//! la grille, pas une faute du cas.
//!
//! Elle est éteinte par défaut (`#[ignore]`) : elle demande bwrap, srt et le réseau. Pour la jouer :
//! `LYDITE_GRILLE=srt LYDITE_SRT=<srt> cargo test grille -- --ignored`. Jouée sans cela, chaque test échoue en le
//! disant.
//!
//! La table des cas s'écrit une fois ([`TABLE`]) ; le mur joué est un paramètre ([`Mur`]), pour qu'un second mur la
//! rejoue sans la recopier. Chaque essai monte son décor dans un dossier neuf ([`Decor`]) : rien du vrai dossier
//! personnel n'est lu ni écrit, car `HOME` y vaut le faux dossier personnel le temps du rejeu. Chaque commande écrit
//! d'abord une marque à elle (`TOUR`) : une marque absente de la sortie ne prouve rien si la commande n'a pas tourné.
//! Linux seulement : le mur est bwrap.
#![cfg(target_os = "linux")]

use super::*;
use crate::essais::mains::profil_borne;
use crate::essais::{depot, git};
use std::env;
use std::ffi::OsString;
use std::io::{self, ErrorKind};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Instant;

/// Une requête HTTP par le proxy du mur : `argv` = marque de départ, URL, marque de réussite, en-tête `Host` (ou vide).
const REQUETE: &str = r#"import sys, urllib.request
print(sys.argv[1], flush=True)
demande = urllib.request.Request(sys.argv[2], headers={"Host": sys.argv[4]} if sys.argv[4] else {})
with urllib.request.urlopen(demande, timeout=20) as reponse:
    if reponse.status == 200:
        print(sys.argv[3], flush=True)"#;

/// Une connexion directe, sans le proxy du mur : `argv` = marque de départ, marque de réussite.
const SORTIE: &str = r#"import socket, sys
print(sys.argv[1], flush=True)
socket.create_connection(("example.com", 80), timeout=10)
print(sys.argv[2], flush=True)"#;

/// Une résolution de nom faite par le bac lui-même : `argv` = marque de départ, marque de réussite.
const RESOLUTION: &str = r#"import socket, sys
print(sys.argv[1], flush=True)
socket.getaddrinfo("example.com", 80)
print(sys.argv[2], flush=True)"#;

/// Une connexion à un socket Unix : `argv` = marque de départ, marque de réussite, chemin du socket.
const SOCKET: &str = r#"import socket, sys
print(sys.argv[1], flush=True)
s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect(sys.argv[3])
print(sys.argv[2], flush=True)"#;

/// Un appel système brut, par `syscall` : `socket` avec la moitié haute de l'argument `domaine` remplie, ou
/// `io_uring_setup`. `argv` = marque de départ, marque de réussite, `socket` ou `io_uring`. Une architecture inconnue
/// sort avant la marque de départ.
const APPEL: &str = r#"import ctypes, platform, sys
tente, cree, quoi = sys.argv[1], sys.argv[2], sys.argv[3]
libc = ctypes.CDLL(None, use_errno=True)
libc.syscall.restype = ctypes.c_long
libc.syscall.argtypes = [ctypes.c_long, ctypes.c_ulong, ctypes.c_ulong, ctypes.c_ulong]
params = ctypes.create_string_buffer(120)
if quoi == "socket":
    numero = {"x86_64": 41, "aarch64": 198}[platform.machine()]
    arguments = ((1 << 32) | 1, 1, 0)
else:
    numero = 425
    arguments = (4, ctypes.addressof(params), 0)
print(tente, flush=True)
if libc.syscall(numero, *arguments) < 0:
    sys.exit(1)
print(cree, flush=True)"#;

/// Ce que `/proc` du bac montre : `argv` = les deux moitiés du témoin de dehors, les deux moitiés du témoin de dedans,
/// la marque du premier, la marque du second. Les moitiés ne se recollent qu'ici : la ligne de commande de la commande
/// ne porte jamais un témoin en entier, et ne se voit donc pas elle-même.
const PROC: &str = r#"import os, sys
vus = ""
for pid in os.listdir("/proc"):
    if pid.isdigit():
        try:
            with open("/proc/" + pid + "/cmdline", errors="replace") as f:
                vus += f.read().replace("\0", " ") + "\n"
        except OSError:
            pass
for debut, fin, marque in ((sys.argv[1], sys.argv[2], sys.argv[5]), (sys.argv[3], sys.argv[4], sys.argv[6])):
    if debut + fin in vus:
        print(marque)"#;

/// Le mur que la grille joue. La table des cas ne le nomme pas.
#[derive(Clone, Copy)]
enum Mur {
    Srt,
}

impl Mur {
    /// Le programme qui tient ce mur, ou ce qui manque pour jouer la grille.
    fn programme(self) -> Result<OsString, String> {
        match self {
            Mur::Srt => programme_de_srt(env::var("LYDITE_GRILLE").ok().as_deref(), env::var_os("LYDITE_SRT")),
        }
    }

    /// La ligne du profil qui exige ce mur.
    fn ligne(self) -> &'static str {
        match self {
            Mur::Srt => "mur = exige",
        }
    }

    /// Le profil rejoue la preuve par ce programme.
    fn brancher(self, profil: &mut Profil, programme: OsString) {
        match self {
            Mur::Srt => profil.avec_srt(programme),
        }
    }
}

/// Le srt de la grille : `LYDITE_GRILLE=srt` la allume, `LYDITE_SRT` nomme le srt à essayer. Sans l'un ou l'autre, le
/// texte de l'erreur le dit ; la grille ne passe jamais en sautant en silence.
fn programme_de_srt(grille: Option<&str>, srt: Option<OsString>) -> Result<OsString, String> {
    if grille != Some("srt") {
        return Err("la grille du mur est éteinte : pose LYDITE_GRILLE=srt, et LYDITE_SRT au srt à essayer (elle \
                    demande bwrap, srt et le réseau) : LYDITE_GRILLE=srt LYDITE_SRT=<srt> cargo test grille -- \
                    --ignored"
            .into());
    }
    match srt {
        Some(srt) if !srt.is_empty() => Ok(srt),
        _ => Err("LYDITE_SRT ne nomme pas de srt : pose-le au chemin du srt à essayer, la grille ne joue pas un mur \
                  qu'elle ne connaît pas"
            .into()),
    }
}

/// Ce que le rejeu rend : vert, rouge (avec ou sans le code de sortie de la preuve, tel qu'il est rendu), ou le délai
/// dépassé.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Fin {
    Vert,
    Rouge,
    Code(i32),
    Delai,
}

impl Fin {
    /// Ce que le rejeu a rendu, lu de son résultat.
    fn vue(rejeu: &Result<(), String>) -> Fin {
        let Err(erreur) = rejeu else { return Fin::Vert };
        if erreur.contains("temps dépassé (") && !erreur.contains("; mise à mort") {
            return Fin::Delai;
        }
        let code = erreur.split_once("rouge (exit status: ").and_then(|(_, reste)| reste.split_once(')'));
        code.and_then(|(code, _)| code.parse().ok()).map_or(Fin::Rouge, Fin::Code)
    }

    /// `vu` est ce que `self` attend : un rouge attendu sans code accepte tout rouge.
    fn attend(self, vu: Fin) -> bool {
        match (self, vu) {
            (Fin::Rouge, Fin::Code(_)) => true,
            (attendu, vu) => attendu == vu,
        }
    }
}

/// L'état d'un chemin de l'hôte.
#[derive(Debug, Clone, PartialEq)]
enum Etat {
    Absent,
    Dossier,
    /// Un fichier, son texte sans la fin de ligne.
    Contient(String),
}

fn etat_de(chemin: &Path) -> Etat {
    match fs::read_to_string(chemin) {
        Ok(texte) => Etat::Contient(texte.trim_end().to_string()),
        Err(e) if e.kind() == ErrorKind::NotFound => Etat::Absent,
        Err(_) if chemin.is_dir() => Etat::Dossier,
        Err(e) => panic!("{} ne se lit pas : {e}", chemin.display()),
    }
}

/// Un processus `sleep` de dehors, tué avec le témoin.
struct Dormeur(Child);

impl Drop for Dormeur {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Ce que le cas ouvre sur l'hôte, hors du mur, et que le mur ne doit pas atteindre.
enum Temoin {
    Aucun,
    /// Un socket Unix de ce chemin, ouvert dehors par [`Temoin::avant`] : il compte ses connexions.
    Socket(PathBuf, Option<UnixListener>),
    /// Des écouteurs TCP de la machine, sur ce port : ils comptent leurs connexions.
    Tcp(Vec<TcpListener>, u16),
    /// Un `sleep <durée>` lancé dehors, tué avec le témoin.
    Dehors {
        _processus: Dormeur,
        duree: String,
    },
    /// Des `sleep <durée>` que le mur lance : aucun ne reste après.
    Restes(Vec<String>),
    /// La machine joint `example.com` en direct : sans cela, un bac qui n'y arrive pas ne prouve rien.
    Sortie,
}

impl Temoin {
    /// Ce que l'hôte doit être, avant le rejeu, pour que le cas prouve quelque chose.
    fn avant(&mut self) {
        match self {
            Temoin::Socket(chemin, ecouteur) => {
                let ouvert = UnixListener::bind(&*chemin).unwrap();
                ouvert.set_nonblocking(true).unwrap();
                *ecouteur = Some(ouvert);
            }
            Temoin::Tcp(_, port) => {
                let adresses = ("localtest.me", *port).to_socket_addrs();
                let adresses = adresses.unwrap_or_else(|e| panic!("localtest.me ne se résout pas : {e}"));
                let adresses: Vec<_> = adresses.collect();
                let locale = adresses.iter().any(|adresse| adresse.ip().is_loopback());
                assert!(locale, "localtest.me ne résout pas vers la machine ({adresses:?}) : R6 ne prouverait rien");
            }
            Temoin::Sortie => {
                let adresses = ("example.com", 80).to_socket_addrs();
                let mut adresses = adresses.unwrap_or_else(|e| panic!("example.com ne se résout pas : {e}"));
                let adresse = adresses.next().expect("example.com n'a pas d'adresse");
                let joint = TcpStream::connect_timeout(&adresse, Duration::from_secs(10));
                assert!(joint.is_ok(), "la machine ne joint pas example.com : R3 ne prouverait rien ({joint:?})");
            }
            Temoin::Dehors { duree, .. } => {
                let la = !dormeurs(duree).is_empty();
                assert!(la, "le processus témoin sleep {duree} ne tourne pas : F8 ne prouverait rien");
            }
            Temoin::Aucun | Temoin::Restes(_) => {}
        }
    }

    /// Ce que le mur n'a pas dû faire, vu de l'hôte après le rejeu. Un compte de zéro n'est cru qu'une fois le même
    /// compte vu à un, par une connexion que le test fait lui-même.
    fn apres(&self) {
        match self {
            Temoin::Socket(chemin, ecouteur) => {
                let ecouteur = ecouteur.as_ref().expect("le socket témoin s'ouvre avant le rejeu");
                assert_eq!(compter(|| ecouteur.accept()), 0, "le socket témoin de dehors a reçu une connexion du mur");
                let _sonde = UnixStream::connect(chemin).unwrap();
                assert_eq!(compter(|| ecouteur.accept()), 1, "le socket témoin ne compte pas une connexion qui y est");
            }
            Temoin::Tcp(ecouteurs, port) => {
                let total = || ecouteurs.iter().map(|ecouteur| compter(|| ecouteur.accept())).sum::<usize>();
                assert_eq!(total(), 0, "la machine a reçu une connexion du proxy du mur");
                let _sonde = TcpStream::connect(("127.0.0.1", *port)).unwrap();
                assert_eq!(total(), 1, "les écouteurs ne comptent pas une connexion qui y est");
            }
            Temoin::Dehors { duree, .. } => {
                let la = !dormeurs(duree).is_empty();
                assert!(la, "le processus témoin sleep {duree} ne tourne plus : F8 ne prouve rien");
            }
            Temoin::Restes(durees) => {
                let tous_morts = || durees.iter().all(|duree| dormeurs(duree).is_empty());
                let fini = attendre_que(tous_morts, Duration::from_secs(30));
                let restes: Vec<u32> = durees.iter().flat_map(|duree| dormeurs(duree)).collect();
                assert!(fini, "des processus de la preuve tuée restent : {restes:?}");
            }
            Temoin::Aucun | Temoin::Sortie => {}
        }
    }
}

/// Les connexions en attente d'un écouteur sans attente (`set_nonblocking`), jusqu'à ce qu'il n'y en ait plus.
fn compter<T>(mut accepter: impl FnMut() -> io::Result<T>) -> usize {
    let mut compte = 0;
    loop {
        match accepter() {
            Ok(_) => compte += 1,
            Err(e) if e.kind() == ErrorKind::WouldBlock => return compte,
            Err(e) => panic!("le témoin ne compte plus : {e}"),
        }
    }
}

/// Les pid des processus `sleep <durée>` de la machine : leur ligne de commande est exactement celle-là, ni le `sh -c`
/// ni le srt qui portent le texte de la commande n'y répondent.
fn dormeurs(duree: &str) -> Vec<u32> {
    let attendue = format!("sleep\0{duree}\0");
    let processus = fs::read_dir("/proc").unwrap();
    let pids = processus.filter_map(|entree| entree.ok()?.file_name().to_str()?.parse::<u32>().ok());
    let ligne_de = |pid: &u32| fs::read(format!("/proc/{pid}/cmdline"));
    pids.filter(|pid| ligne_de(pid).is_ok_and(|ligne| ligne == attendue.as_bytes())).collect()
}

/// Vrai dès que `fait` l'est ; faux si `delai` passe d'abord. Pour ce qui meurt à son heure, non pour un résultat.
fn attendre_que(mut fait: impl FnMut() -> bool, delai: Duration) -> bool {
    let debut = Instant::now();
    while !fait() {
        if debut.elapsed() > delai {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
    true
}

/// Un cas : une commande `sh` jouée dans le mur, et ce qu'on constate après.
struct Cas {
    commande: String,
    fin: Fin,
    /// Ce que la sortie de la commande contient. La première est sa marque `TOUR`.
    visibles: Vec<String>,
    /// Ce que la sortie ne contient pas.
    invisibles: Vec<String>,
    /// L'état de ces chemins de l'hôte après coup.
    hote: Vec<(PathBuf, Etat)>,
    /// Les variables que Lydite porte pendant le rejeu.
    env: Vec<(String, String)>,
    /// Le délai de la preuve, en secondes.
    delai: u64,
    temoin: Temoin,
}

impl Cas {
    /// Un cas dont la commande écrit d'abord sa marque `TOUR` : sans elle dans la sortie, la commande n'a pas tourné.
    fn new(d: &Decor, corps: String, fin: Fin) -> Cas {
        let tour = d.marque("TOUR");
        Cas {
            commande: format!("echo {tour}; {corps}"),
            fin,
            visibles: vec![tour],
            invisibles: Vec::new(),
            hote: Vec::new(),
            env: Vec::new(),
            delai: 120,
            temoin: Temoin::Aucun,
        }
    }

    fn voit(mut self, marque: &str) -> Cas {
        self.visibles.push(marque.into());
        self
    }

    fn ne_voit_pas(mut self, marque: &str) -> Cas {
        self.invisibles.push(marque.into());
        self
    }

    fn sur_l_hote(mut self, chemin: &Path, etat: Etat) -> Cas {
        self.hote.push((chemin.into(), etat));
        self
    }

    fn avec_env(mut self, nom: &str, valeur: &str) -> Cas {
        self.env.push((nom.into(), valeur.into()));
        self
    }

    fn avec_temoin(mut self, temoin: Temoin) -> Cas {
        self.temoin = temoin;
        self
    }

    fn en_secondes(mut self, delai: u64) -> Cas {
        self.delai = delai;
        self
    }
}

/// Le décor d'un essai, dans un dossier neuf : un faux dossier personnel, un faux dossier de Lydite, un dossier de
/// `lecture`, un dépôt git avec son worktree, le bac, et le temporaire du bac. Un fichier témoin porte une marque tirée
/// pour l'essai dans le faux dossier personnel (`temoin`), dans celui de Lydite (`temoin`) et dans `lecture`
/// (`lisible`).
struct Decor {
    dir: PathBuf,
    personnel: PathBuf,
    kraken: PathBuf,
    lecture: PathBuf,
    depot: PathBuf,
    bac: PathBuf,
    temporaire: PathBuf,
    tirage: String,
    /// Une adresse IPv4 d'`example.com`, si le décor l'a cherchée (elle demande le réseau).
    ip: Option<String>,
}

impl Decor {
    /// Un décor neuf. `reseau` : y chercher l'adresse d'`example.com`, ce qui panique sans réseau. Le chemin du dossier
    /// reste court : le socket de pont de srt, dans le temporaire, tient dans 107 octets.
    fn neuf(reseau: bool) -> Decor {
        let tirage = lot();
        let dir = env::temp_dir().join(format!("lg-{tirage}"));
        let (personnel, kraken, lecture) = (dir.join("personnel"), dir.join("lydite"), dir.join("lecture"));
        for dossier in [&personnel, &kraken, &lecture] {
            fs::create_dir_all(dossier).unwrap();
        }
        let le_depot = depot(&dir);
        let bac = dir.join("bac");
        git(&le_depot, &["worktree", "add", "--quiet", "--detach", &bac.to_string_lossy()]);
        let temporaire = temporaire_de(&bac).unwrap();
        let ip = reseau.then(|| {
            let adresses = ("example.com", 80).to_socket_addrs();
            let adresses = adresses.unwrap_or_else(|e| panic!("la grille demande le réseau : example.com : {e}"));
            let ipv4 = adresses.filter(|adresse| adresse.is_ipv4()).map(|adresse| adresse.ip().to_string()).next();
            ipv4.expect("example.com n'a pas d'adresse IPv4")
        });
        let decor = Decor { dir, personnel, kraken, lecture, depot: le_depot, bac, temporaire, tirage, ip };
        fs::write(decor.personnel.join("temoin"), decor.marque("PERSO")).unwrap();
        fs::write(decor.kraken.join("temoin"), decor.marque("DOSSIER")).unwrap();
        fs::write(decor.lecture.join("lisible"), decor.marque("LECTURE")).unwrap();
        decor
    }

    /// Une marque propre à cet essai : elle ne se trouve par hasard ni dans une sortie ni dans un identifiant.
    fn marque(&self, nom: &str) -> String {
        format!("{nom}-{}", self.tirage)
    }

    /// Le tirage sans sa ponctuation, pour un nom de variable ou une durée.
    fn chiffres(&self) -> String {
        self.tirage.replace('_', "")
    }

    /// Le nom de la variable de la clé du fournisseur du profil.
    fn cle(&self) -> String {
        format!("LYDITE_GRILLE_CLE_{}", self.chiffres())
    }

    /// Le nom d'une variable témoin qui n'est pas la clé : elle passe, elle.
    fn variable_temoin(&self) -> String {
        format!("LYDITE_GRILLE_TEMOIN_{}", self.chiffres())
    }

    /// Le profil de ce cas : le mur exigé, borné par les dossiers du décor, avec un fournisseur dont la clé est la
    /// variable [`Decor::cle`], le réseau `example.com` (et `localtest.me`, qui résout vers la machine), et `lecture`.
    fn profil(&self, mur: Mur, cas: &Cas, programme: OsString) -> Profil {
        let texte = format!(
            "sorte = claude\n{}\nreseau = example.com, localtest.me\nlecture = {}\napi = https://example.com/v1\n\
             cle = {}\ndelai_preuve = {}\n",
            mur.ligne(),
            self.lecture.display(),
            self.cle(),
            cas.delai,
        );
        let mut profil = profil_borne(&texte, &self.dir);
        mur.brancher(&mut profil, programme);
        profil
    }
}

/// Les variables que le rejeu porte, posées pour lui seul et rendues après. Un verrou les sérialise : l'environnement
/// est celui du processus, et deux essais ne posent pas le leur ensemble.
struct Pose {
    avant: Vec<(String, Option<OsString>)>,
    _verrou: MutexGuard<'static, ()>,
}

static VERROU: Mutex<()> = Mutex::new(());

impl Pose {
    fn nouvelle(variables: &[(String, String)]) -> Pose {
        let verrou = VERROU.lock().unwrap_or_else(PoisonError::into_inner);
        let avant = variables
            .iter()
            .map(|(nom, valeur)| {
                let ancienne = env::var_os(nom);
                // SAFETY : seuls les tests de cette grille posent des variables, et le verrou les tient un à un ; les
                // autres threads du processus n'en lisent aucune de celles-ci.
                unsafe { env::set_var(nom, valeur) };
                (nom.clone(), ancienne)
            })
            .collect();
        Pose { avant, _verrou: verrou }
    }
}

impl Drop for Pose {
    fn drop(&mut self) {
        for (nom, ancienne) in self.avant.drain(..).rev() {
            // SAFETY : voir `Pose::nouvelle`, le verrou est encore tenu.
            unsafe {
                match ancienne {
                    Some(valeur) => env::set_var(&nom, valeur),
                    None => env::remove_var(&nom),
                }
            }
        }
    }
}

type Fabrique = fn(&Decor) -> Cas;

/// La table : un nom, et le cas que ce nom fait de son décor.
const TABLE: &[(&str, Fabrique)] = &[
    ("F1", f1),
    ("F2", f2),
    ("F3", f3),
    ("F4a", f4a),
    ("F4b", f4b),
    ("F4c", f4c),
    ("F5", f5),
    ("F6", f6),
    ("F7", f7),
    ("F8", f8),
    ("F9a", f9a),
    ("F9b", f9b),
    ("F9c", f9c),
    ("R1", r1),
    ("R2", r2),
    ("R3", r3),
    ("R4", r4),
    ("R5", r5),
    ("R6", r6),
    ("S1", s1),
    ("S2", s2),
    ("S3", s3),
    ("S4", s4),
    ("E1", e1),
    ("T1", t1),
    ("T2", t2),
];

/// F1 : un fichier témoin sous le faux `~` porte une marque ; la lecture échoue, et la marque n'est nulle part.
fn f1(d: &Decor) -> Cas {
    let temoin = d.personnel.join("temoin");
    let marque = d.marque("PERSO");
    let cas = Cas::new(d, format!("cat \"{}\"", temoin.display()), Fin::Rouge);
    cas.ne_voit_pas(&marque).sur_l_hote(&temoin, Etat::Contient(marque))
}

/// F2 : de même dans le faux dossier de Lydite.
fn f2(d: &Decor) -> Cas {
    let temoin = d.kraken.join("temoin");
    let marque = d.marque("DOSSIER");
    let cas = Cas::new(d, format!("cat \"{}\"", temoin.display()), Fin::Rouge);
    cas.ne_voit_pas(&marque).sur_l_hote(&temoin, Etat::Contient(marque))
}

/// F3 : un fichier de `lecture` se lit.
fn f3(d: &Decor) -> Cas {
    let lisible = d.lecture.join("lisible");
    let marque = d.marque("LECTURE");
    let cas = Cas::new(d, format!("cat \"{}\"", lisible.display()), Fin::Vert);
    cas.voit(&marque).sur_l_hote(&lisible, Etat::Contient(marque))
}

/// Écrire `chemin`, hors du bac : la commande échoue, sa marque d'écriture n'est pas là, et rien n'existe sur l'hôte.
fn ecriture_refusee(d: &Decor, chemin: PathBuf) -> Cas {
    let ecrit = d.marque("ECRIT");
    let cas = Cas::new(d, format!("echo x > \"{}\" && echo {ecrit}", chemin.display()), Fin::Rouge);
    cas.ne_voit_pas(&ecrit).sur_l_hote(&chemin, Etat::Absent)
}

/// F4 : écrire hors du bac échoue, et rien n'existe sur l'hôte ensuite : dans `/tmp`.
fn f4a(d: &Decor) -> Cas {
    ecriture_refusee(d, Path::new("/tmp").join(d.marque("F4a")))
}

/// F4 : dans le faux `~`.
fn f4b(d: &Decor) -> Cas {
    ecriture_refusee(d, d.personnel.join(d.marque("F4b")))
}

/// F4 : dans le dossier de `lecture`.
fn f4c(d: &Decor) -> Cas {
    ecriture_refusee(d, d.lecture.join(d.marque("F4c")))
}

/// F5 : écrire dans le bac et dans son temporaire réussit.
fn f5(d: &Decor) -> Cas {
    let ecrit = d.marque("ECRIT");
    let (dans_le_bac, dans_le_temporaire) = (d.bac.join("ecrit"), d.temporaire.join("ecrit"));
    let corps = format!(
        "echo {ecrit} > \"{}\" && echo {ecrit} > \"{}\" && echo {ecrit}",
        dans_le_bac.display(),
        dans_le_temporaire.display()
    );
    let cas = Cas::new(d, corps, Fin::Vert).voit(&ecrit);
    cas.sur_l_hote(&dans_le_bac, Etat::Contient(ecrit.clone())).sur_l_hote(&dans_le_temporaire, Etat::Contient(ecrit))
}

/// F6 : le git commun se lit, et ne s'écrit pas.
fn f6(d: &Decor) -> Cas {
    let commun = d.depot.join(".git");
    let (lu, ecrit) = (d.marque("LU"), d.marque("ECRIT"));
    let ecrit_dans = commun.join(&ecrit);
    let corps = format!(
        "cat \"{}\" > /dev/null && echo {lu}; echo x > \"{}\" && echo {ecrit}",
        commun.join("HEAD").display(),
        ecrit_dans.display()
    );
    Cas::new(d, corps, Fin::Rouge).voit(&lu).ne_voit_pas(&ecrit).sur_l_hote(&ecrit_dans, Etat::Absent)
}

/// F7 : un lien du bac vers le témoin ne le rend pas lisible.
fn f7(d: &Decor) -> Cas {
    let (temoin, lien) = (d.personnel.join("temoin"), d.bac.join("lien"));
    let (fait, perso) = (d.marque("LIEN"), d.marque("PERSO"));
    let (temoin, lien) = (temoin.display(), lien.display());
    let corps = format!("ln -s \"{temoin}\" \"{lien}\" && echo {fait}; cat \"{lien}\"");
    Cas::new(d, corps, Fin::Rouge).voit(&fait).ne_voit_pas(&perso)
}

/// F8 : un processus témoin lancé dehors n'est pas visible dans `/proc`. Un second, lancé dedans, l'est : sans cela,
/// un `/proc` vide ne prouverait rien.
fn f8(d: &Decor) -> Cas {
    let chiffres = d.chiffres();
    let dehors = format!("3000.{chiffres}");
    let processus = Command::new("sleep").arg(&dehors).stdin(Stdio::null()).spawn().unwrap();
    let (vu_dehors, vu_dedans) = (d.marque("VU-DEHORS"), d.marque("VU-DEDANS"));
    let corps = format!(
        "B=5000.; sleep \"$B\"{chiffres} & P=$!; python3 -c '{PROC}' 3000. {chiffres} 5000. {chiffres} {vu_dehors} \
         {vu_dedans}; kill $P"
    );
    let cas = Cas::new(d, corps, Fin::Vert).voit(&vu_dedans).ne_voit_pas(&vu_dehors);
    cas.avec_temoin(Temoin::Dehors { _processus: Dormeur(processus), duree: dehors })
}

/// Renommer `dossier`, épinglé par le mur : la commande échoue, la marque du renommage n'est pas là, et l'hôte a
/// toujours son dossier sous son nom.
fn renommage_refuse(d: &Decor, dossier: &Path) -> Cas {
    let renomme = PathBuf::from(format!("{}.renomme", dossier.display()));
    let fait = d.marque("RENOMME");
    let corps = format!("mv \"{}\" \"{}\" && echo {fait}", dossier.display(), renomme.display());
    let cas = Cas::new(d, corps, Fin::Rouge).ne_voit_pas(&fait);
    cas.sur_l_hote(dossier, Etat::Dossier).sur_l_hote(&renomme, Etat::Absent)
}

/// F9 : renommer un dossier épinglé échoue : le git commun.
fn f9a(d: &Decor) -> Cas {
    renommage_refuse(d, &d.depot.join(".git"))
}

/// F9 : le bac.
fn f9b(d: &Decor) -> Cas {
    renommage_refuse(d, &d.bac)
}

/// F9 : le temporaire du bac.
fn f9c(d: &Decor) -> Cas {
    renommage_refuse(d, &d.temporaire)
}

/// Une requête HTTP par le proxy du mur, vers `url`, sous l'en-tête `Host` si `hote` n'est pas vide : sa marque de
/// départ se voit toujours (le python a tourné), celle de réussite seulement si `fin` est vert.
fn requete(d: &Decor, url: &str, hote: &str, fin: Fin) -> Cas {
    let (tente, ok) = (d.marque("TENTE"), d.marque("OK"));
    let cas = Cas::new(d, format!("python3 -c '{REQUETE}' {tente} \"{url}\" {ok} \"{hote}\""), fin).voit(&tente);
    if fin == Fin::Vert { cas.voit(&ok) } else { cas.ne_voit_pas(&ok) }
}

/// R1 : un domaine non permis est refusé.
fn r1(d: &Decor) -> Cas {
    requete(d, "http://example.org/", "", Fin::Rouge)
}

/// R2 : un domaine permis passe.
fn r2(d: &Decor) -> Cas {
    requete(d, "http://example.com/", "", Fin::Vert)
}

/// Un python qui tente `code` et ne doit pas aboutir : sa marque de départ se voit (il a tourné), celle d'arrivée non.
/// `extra` est le troisième argument de `code`.
fn tentative_refusee(d: &Decor, code: &str, extra: &str, temoin: Temoin) -> Cas {
    let (tente, fait) = (d.marque("TENTE"), d.marque("FAIT"));
    let corps = format!("python3 -c '{code}' {tente} {fait} {extra}");
    Cas::new(d, corps, Fin::Rouge).voit(&tente).ne_voit_pas(&fait).avec_temoin(temoin)
}

/// R3 : sans le proxy du mur, rien ne sort.
fn r3(d: &Decor) -> Cas {
    tentative_refusee(d, SORTIE, "", Temoin::Sortie)
}

/// R4 : une adresse IP écrite en clair est refusée, même celle d'un domaine permis.
fn r4(d: &Decor) -> Cas {
    let ip = d.ip.as_deref().unwrap_or("0.0.0.0");
    requete(d, &format!("http://{ip}/"), "example.com", Fin::Rouge)
}

/// R5 : le bac ne résout aucun nom lui-même.
fn r5(d: &Decor) -> Cas {
    tentative_refusee(d, RESOLUTION, "", Temoin::Aucun)
}

/// R6 : un nom permis qui résout vers la machine elle-même est refusé : la machine, qui écoute, ne compte rien.
fn r6(d: &Decor) -> Cas {
    let v4 = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = v4.local_addr().unwrap().port();
    let mut ecouteurs = vec![v4];
    ecouteurs.extend(TcpListener::bind(("::1", port)));
    for ecouteur in &ecouteurs {
        ecouteur.set_nonblocking(true).unwrap();
    }
    requete(d, &format!("http://localtest.me:{port}/"), "", Fin::Rouge).avec_temoin(Temoin::Tcp(ecouteurs, port))
}

/// Un socket Unix témoin ouvert dehors, dans le dossier de `lecture` (lisible du mur), qui compte ses connexions.
fn socket_temoin(d: &Decor) -> (Temoin, String) {
    let chemin = d.lecture.join("t.sock");
    let extra = format!("\"{}\"", chemin.display());
    (Temoin::Socket(chemin, None), extra)
}

/// S1 : par `python3`, créer un socket Unix est refusé, et le socket témoin ouvert dehors compte zéro connexion.
fn s1(d: &Decor) -> Cas {
    let (temoin, chemin) = socket_temoin(d);
    tentative_refusee(d, SOCKET, &chemin, temoin)
}

/// S2 : la même demande, la moitié haute de l'argument remplie, est refusée aussi.
fn s2(d: &Decor) -> Cas {
    tentative_refusee(d, APPEL, "socket", Temoin::Aucun)
}

/// S3 : `io_uring` est refusé.
fn s3(d: &Decor) -> Cas {
    tentative_refusee(d, APPEL, "io_uring", Temoin::Aucun)
}

/// S4 : le réseau permis marche encore après le verrou : la même commande tente le socket Unix, refusé (sa marque de
/// refus se voit, celle de connexion non), puis joint `example.com` par le proxy.
fn s4(d: &Decor) -> Cas {
    let (temoin, chemin) = socket_temoin(d);
    let (tente, refuse, connecte) = (d.marque("TENTE-SOCKET"), d.marque("REFUSE"), d.marque("CONNECTE"));
    let (tente_http, ok) = (d.marque("TENTE"), d.marque("OK"));
    let corps = format!(
        "python3 -c '{SOCKET}' {tente} {connecte} {chemin} || echo {refuse}; python3 -c '{REQUETE}' {tente_http} \
         http://example.com/ {ok} \"\""
    );
    let cas = Cas::new(d, corps, Fin::Vert).voit(&tente).voit(&refuse).voit(&tente_http).voit(&ok);
    cas.ne_voit_pas(&connecte).avec_temoin(temoin)
}

/// E1 : une variable témoin posée à la place de la clé du fournisseur, avec une marque, est absente de `env` dans le
/// mur ; une autre variable témoin, qui n'est pas la clé, y est : sans elle, une sortie sans la clé ne prouverait rien.
fn e1(d: &Decor) -> Cas {
    let (cle, valeur) = (d.cle(), d.marque("CLE"));
    let (autre, vue) = (d.variable_temoin(), d.marque("TEMOIN"));
    let cas = Cas::new(d, "env | grep LYDITE_GRILLE_".into(), Fin::Vert).voit(&format!("{autre}={vue}"));
    cas.ne_voit_pas(&valeur).ne_voit_pas(&cle).avec_env(&cle, &valeur).avec_env(&autre, &vue)
}

/// T1 : le code de sortie est rendu tel quel.
fn t1(d: &Decor) -> Cas {
    Cas::new(d, "exit 42".into(), Fin::Code(42))
}

/// T2 : une preuve plus lente que le délai est tuée avec ses petits-enfants, et rien ne reste : la commande lance un
/// `sleep` en fond, dit l'avoir lancé, puis dort encore.
fn t2(d: &Decor) -> Cas {
    let chiffres = d.chiffres();
    let (premier, second) = (format!("300.{chiffres}"), format!("301.{chiffres}"));
    let lance = d.marque("LANCE");
    let corps = format!("sleep {premier} & echo {lance}; sleep {second}");
    Cas::new(d, corps, Fin::Delai).voit(&lance).en_secondes(20).avec_temoin(Temoin::Restes(vec![premier, second]))
}

/// Joue le cas `id` de la [`TABLE`] dans le mur : monte son décor, rejoue sa commande par [`rejouer`] sous un profil au
/// mur exigé, et constate. Panique, en le disant, si la grille est éteinte.
fn jouer(mur: Mur, id: &str) {
    let programme = mur.programme().unwrap_or_else(|erreur| panic!("{id} : {erreur}"));
    let trouve = TABLE.iter().find(|(nom, _)| *nom == id);
    let (_, fabrique) = trouve.unwrap_or_else(|| panic!("{id} n'est pas dans la table"));
    let decor = Decor::neuf(true);
    let mut cas = fabrique(&decor);
    cas.temoin.avant();
    let profil = decor.profil(mur, &cas, programme);
    let mut variables = vec![("HOME".to_string(), decor.personnel.to_string_lossy().into_owned())];
    variables.extend(cas.env.iter().cloned());
    let journal = decor.dir.join("journal.tmp");
    let (verif, rejeu) = {
        let _pose = Pose::nouvelle(&variables);
        rejouer(&profil, &cas.commande, &decor.bac, &journal)
    };
    let dossier = decor.dir.display();
    let dit = format!("{id} dans {dossier} : {}\nsortie : {verif:?}\nrejeu : {rejeu:?}", cas.commande);
    let sortie = verif.unwrap_or_else(|| panic!("la commande n'a pas pu partir (srt introuvable ?) : {dit}"));
    for marque in &cas.visibles {
        assert!(sortie.contains(marque), "{marque} manque : la commande n'a pas tourné, ou le mur l'a coupée\n{dit}");
    }
    let vu = Fin::vue(&rejeu);
    assert!(cas.fin.attend(vu), "attendu {:?}, rendu {vu:?}\n{dit}", cas.fin);
    for marque in &cas.invisibles {
        assert!(!sortie.contains(marque), "{marque} est dans la sortie : le mur ne tient pas\n{dit}");
    }
    for (chemin, attendu) in &cas.hote {
        let etat = etat_de(chemin);
        // Ce que le mur n'a pas dû créer ne reste pas, même s'il l'a fait : la grille ne laisse rien sur l'hôte.
        if *attendu == Etat::Absent {
            let _ = fs::remove_file(chemin);
        }
        assert_eq!(&etat, attendu, "{} sur l'hôte après coup\n{dit}", chemin.display());
    }
    cas.temoin.apres();
    let _ = fs::remove_dir_all(&decor.dir);
}

/// Le texte de l'`ignore` de chaque test de la grille.
const ETEINTE: &str = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored";

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f1_un_temoin_du_dossier_personnel_ne_se_lit_pas() {
    jouer(Mur::Srt, "F1");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f2_un_temoin_du_dossier_de_lydite_ne_se_lit_pas() {
    jouer(Mur::Srt, "F2");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f3_un_fichier_de_lecture_se_lit() {
    jouer(Mur::Srt, "F3");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f4a_ecrire_dans_tmp_echoue() {
    jouer(Mur::Srt, "F4a");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f4b_ecrire_dans_le_dossier_personnel_echoue() {
    jouer(Mur::Srt, "F4b");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f4c_ecrire_dans_le_dossier_de_lecture_echoue() {
    jouer(Mur::Srt, "F4c");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f5_le_bac_et_son_temporaire_s_ecrivent() {
    jouer(Mur::Srt, "F5");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f6_le_git_commun_se_lit_et_ne_s_ecrit_pas() {
    jouer(Mur::Srt, "F6");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f7_un_lien_du_bac_ne_rend_pas_le_temoin_lisible() {
    jouer(Mur::Srt, "F7");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f8_un_processus_de_dehors_n_est_pas_dans_proc() {
    jouer(Mur::Srt, "F8");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f9a_le_git_commun_ne_se_renomme_pas() {
    jouer(Mur::Srt, "F9a");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f9b_le_bac_ne_se_renomme_pas() {
    jouer(Mur::Srt, "F9b");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn f9c_le_temporaire_ne_se_renomme_pas() {
    jouer(Mur::Srt, "F9c");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r1_un_domaine_non_permis_est_refuse() {
    jouer(Mur::Srt, "R1");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r2_un_domaine_permis_passe() {
    jouer(Mur::Srt, "R2");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r3_sans_le_proxy_rien_ne_sort() {
    jouer(Mur::Srt, "R3");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r4_une_adresse_ip_en_clair_est_refusee() {
    jouer(Mur::Srt, "R4");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r5_le_bac_ne_resout_aucun_nom() {
    jouer(Mur::Srt, "R5");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn r6_un_nom_permis_qui_resout_vers_la_machine_est_refuse() {
    jouer(Mur::Srt, "R6");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn s1_un_socket_unix_ne_se_cree_pas() {
    jouer(Mur::Srt, "S1");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn s2_la_moitie_haute_de_l_argument_ne_passe_pas() {
    jouer(Mur::Srt, "S2");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn s3_io_uring_est_refuse() {
    jouer(Mur::Srt, "S3");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn s4_le_reseau_permis_marche_apres_le_verrou() {
    jouer(Mur::Srt, "S4");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn e1_la_cle_du_fournisseur_n_entre_pas_dans_le_mur() {
    jouer(Mur::Srt, "E1");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn t1_le_code_de_sortie_est_rendu_tel_quel() {
    jouer(Mur::Srt, "T1");
}

#[test]
#[ignore = "demande bwrap, srt et le réseau : LYDITE_GRILLE=srt cargo test grille -- --ignored"]
fn t2_une_preuve_trop_lente_est_tuee_avec_ses_petits_enfants() {
    jouer(Mur::Srt, "T2");
}

// Ce qui suit se joue toujours : la grille elle-même, sans mur ni réseau.

#[test]
fn chaque_cas_de_la_table_a_son_test_eteint() {
    let source = include_str!("grille.rs");
    let ignore = format!("#[ignore = {ETEINTE:?}]");
    let appel = |id: &str| format!("{}(Mur::Srt, {id:?});", "jouer");
    let mut ids: Vec<&str> = TABLE.iter().map(|(id, _)| *id).collect();
    ids.sort_unstable();
    let n = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), n, "un cas est écrit deux fois dans la table");
    for id in ids {
        assert_eq!(source.matches(&appel(id)).count(), 1, "{id} : un test et un seul le joue");
    }
    // Un test qui joue un mur porte l'`ignore` : jamais joué en passant par `cargo test` seul.
    let joue = format!("{}(Mur::Srt", "jouer");
    let mut tests = 0;
    for morceau in source.split(&format!("#[{}]", "test")).skip(1).filter(|morceau| morceau.contains(&joue)) {
        tests += 1;
        assert!(morceau.starts_with(&format!("\n{ignore}\nfn ")), "un test du mur n'est pas éteint : {morceau:.80}");
    }
    assert_eq!(tests, n, "autant de tests que de cas");
}

#[test]
fn chaque_cas_dit_sa_marque_et_ne_se_contredit_pas() {
    let decor = Decor::neuf(false);
    let tour = decor.marque("TOUR");
    for (id, fabrique) in TABLE {
        let cas = fabrique(&decor);
        let commence = cas.commande.starts_with(&format!("echo {tour}; "));
        assert!(commence, "{id} : sa commande n'écrit pas sa marque d'abord");
        assert_eq!(cas.visibles.first(), Some(&tour), "{id} : sa marque ne se cherche pas dans la sortie");
        assert!(cas.invisibles.iter().all(|marque| !cas.visibles.contains(marque)), "{id} : voit et ne voit pas");
        // Un cas rouge sans code dit ce que le mur retient : il a au moins une marque qu'il ne doit pas laisser passer.
        let rouge = matches!(cas.fin, Fin::Rouge);
        assert!(!rouge || !cas.invisibles.is_empty(), "{id} : rouge, et rien qu'il ne doive laisser passer");
        assert!(cas.delai > 0, "{id}");
    }
    let _ = fs::remove_dir_all(&decor.dir);
}

#[test]
fn le_decor_monte_ses_temoins_dans_un_dossier_neuf() {
    let decor = Decor::neuf(false);
    let dedans = |chemin: &Path| chemin.starts_with(&decor.dir);
    let (personnel, kraken, lecture) = (&decor.personnel, &decor.kraken, &decor.lecture);
    assert!([personnel, kraken, lecture, &decor.depot, &decor.bac, &decor.temporaire].into_iter().all(|c| dedans(c)));
    assert_eq!(etat_de(&personnel.join("temoin")), Etat::Contient(decor.marque("PERSO")));
    assert_eq!(etat_de(&kraken.join("temoin")), Etat::Contient(decor.marque("DOSSIER")));
    assert_eq!(etat_de(&lecture.join("lisible")), Etat::Contient(decor.marque("LECTURE")));
    // Le bac est un worktree du dépôt, dont le git commun est celui que `profil_borne` ferme en écriture.
    let administratif = decor.depot.join(".git").join("worktrees").join("bac");
    assert_eq!(etat_de(&decor.bac.join(".git")), Etat::Contient(format!("gitdir: {}", administratif.display())));
    assert_eq!(etat_de(&decor.depot.join(".git")), Etat::Dossier);
    assert_eq!(etat_de(&decor.dir.join("absent")), Etat::Absent);
    let _ = fs::remove_dir_all(&decor.dir);
}

#[test]
fn la_grille_eteinte_le_dit_au_lieu_de_passer() {
    let srt = || Some(OsString::from("srt"));
    assert_eq!(programme_de_srt(Some("srt"), srt()), Ok(OsString::from("srt")));
    for grille in [None, Some(""), Some("autre")] {
        let erreur = programme_de_srt(grille, srt()).unwrap_err();
        assert!(erreur.contains("LYDITE_GRILLE=srt"), "{erreur}");
    }
    for sans_srt in [None, Some(OsString::new())] {
        let erreur = programme_de_srt(Some("srt"), sans_srt).unwrap_err();
        assert!(erreur.contains("LYDITE_SRT ne nomme pas de srt"), "{erreur}");
    }
}

#[test]
fn les_sondes_de_la_grille_voient_ce_qui_est_la() {
    let dir = env::temp_dir().join(format!("lg-sondes-{}", lot()));
    fs::create_dir_all(&dir).unwrap();
    // Un écouteur compte une connexion qui y est, et zéro quand rien n'y est (celui du socket Unix, que la grille
    // ouvre à son `avant`, ne se joue pas ici : certains bacs refusent de créer un socket Unix).
    let ecouteur = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    ecouteur.set_nonblocking(true).unwrap();
    assert_eq!(compter(|| ecouteur.accept()), 0);
    let _sonde = TcpStream::connect(ecouteur.local_addr().unwrap()).unwrap();
    assert_eq!(compter(|| ecouteur.accept()), 1);
    // Un processus de dehors se trouve par sa ligne de commande exacte, et plus une fois tué.
    let duree = format!("200.{}", lot().replace('_', ""));
    assert_eq!(dormeurs(&duree), Vec::<u32>::new());
    let mut processus = Command::new("sleep").arg(&duree).stdin(Stdio::null()).spawn().unwrap();
    assert_eq!(dormeurs(&duree), [processus.id()]);
    processus.kill().unwrap();
    processus.wait().unwrap();
    assert!(dormeurs(&duree).is_empty());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_fin_du_rejeu_se_lit_de_son_resultat() {
    let rouge = |statut: &str| Err(format!("vérification « x » rouge ({statut}) :\nsortie"));
    assert_eq!(Fin::vue(&Ok(())), Fin::Vert);
    assert_eq!(Fin::vue(&rouge("exit status: 42")), Fin::Code(42));
    assert_eq!(Fin::vue(&rouge("signal: 9 (SIGKILL)")), Fin::Rouge);
    assert_eq!(Fin::vue(&Err("vérification « x » : temps dépassé (20 s) :\n".into())), Fin::Delai);
    assert_eq!(Fin::vue(&Err("vérification « x » : temps dépassé (20 s) ; mise à mort : e".into())), Fin::Rouge);
    assert!(Fin::Rouge.attend(Fin::Code(1)) && Fin::Code(42).attend(Fin::Code(42)));
    assert!(!Fin::Code(42).attend(Fin::Code(1)) && !Fin::Vert.attend(Fin::Rouge) && !Fin::Rouge.attend(Fin::Vert));
}
