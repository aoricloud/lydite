//! Le rejeu de la preuve d'une main sous le mur natif : `bwrap` lancé par Lydite, sans srt. Même entrée et même sortie
//! que [`crate::branchements::srt::rejouer`] : dossier courant le bac, entrée nulle, sortie et erreurs dans le
//! `journal`, code de sortie rendu, délai, arbre tué au délai.
//!
//! Les règles sont celles du rejeu par srt ([`Profil::listes_du_mur`]) ; [`crate::coeur::natif::arguments`] les
//! traduit en arguments du `bwrap` extérieur, après que ce fichier a lu sur le disque ce qu'elle demande (chemins
//! réels, existence). Dans le bac, la commande part sous un second `bwrap` qui pose le verrou
//! ([`crate::coeur::verrou`]) :
//!
//! ```text
//! <bwrap> <arguments du mur> -- sh -c '<POSE_LE_VERROU>' sh <filtre> <bwrap> <commande>
//!   → exec 9<<filtre> && exec <bwrap> --bind / / --dev-bind /dev /dev --unshare-pid --proc /proc --die-with-parent
//!                                     --seccomp 9 -- sh -c <commande>
//! ```
//!
//! Le fichier du filtre s'écrit dans le dossier temporaire du bac (`verrou-<lot>.bpf`) et se retire quelle que soit la
//! fin. La commande part après que le filtre est ouvert, jamais avant. `TMPDIR` y vaut le dossier temporaire du bac, le
//! seul endroit hors du bac où la preuve écrit, comme sous srt. Les chemins que le mur a sautés parce qu'ils manquent
//! s'écrivent en tête du journal, une ligne chacun.
//!
//! # Le réseau
//!
//! Sans domaine permis par le profil, le réseau est coupé en entier (`--unshare-net` des arguments du mur) et rien
//! d'autre ne part. Avec des domaines, le bac n'a toujours pas de réseau : il sort par `lydite-reseau`
//! (le dossier `reseau/` de ce dépôt), en deux morceaux.
//!
//! 1. Avant le bac, Lydite crée un dossier neuf en 0700 à côté du temporaire du bac et lance, hors du bac,
//!    `lydite-reseau filtre --socket <dossier>/s --domaines <a,b,c> --journal <Lydite>/reseau.journal`, le dossier
//!    s'appelant `<dix hexadécimaux>.net`. Un chemin de socket de plus de 107 octets est refusé avant de lancer. Son
//!    entrée est un tuyau que Lydite tient ouvert : le filtre s'arrête quand elle se ferme. Lydite attend sa ligne
//!    `ready` jusqu'à une échéance ; un filtre qui meurt avant, ou qui se tait, est un refus : rien n'est lancé,
//!    rien ne reste.
//! 2. Dans le bac, le dossier du socket et `lydite-reseau` sont montés en lecture seule, après les montages du mur,
//!    et la chaîne devient :
//!
//! ```text
//! <bwrap> <arguments du mur> --ro-bind <dossier> <dossier> --ro-bind <lydite-reseau> <lydite-reseau>
//!   -- <lydite-reseau> relais --ecoute 127.0.0.1:3128 --vers <socket> -- sh -c '<POSE_LE_VERROU>' sh …
//! ```
//!
//! Le relais écoute avant de lancer la commande, donc avant le verrou ; la commande, elle, reste sous le verrou, et ne
//! sort que par `127.0.0.1:3128`, où les variables de proxy la mènent. Le verrou ne refuse que la création d'un socket
//! Unix : la commande pourrait sinon changer ce que le chemin du socket désigne, et le relais, hors du verrou, se
//! connecterait à autre chose. Le dossier est donc monté en lecture seule (rien ne s'y crée, ne s'y retire ni ne s'y
//! renomme), et le relais épingle sa cible avant de lancer la commande : il refuse de partir si le dossier est un lien
//! ou si l'entrée n'est pas un socket, ouvre le dossier une fois, et ne se connecte plus que par ce descripteur, jamais
//! par le chemin d'origine. À la fin, quelle qu'elle soit (succès, échec,
//! délai dépassé), Lydite ferme l'entrée du filtre, l'attend jusqu'à une échéance, tue son arbre s'il ne sort pas,
//! puis retire le dossier du socket. Le relais ne transmet pas les signaux : la mise à mort vise l'arbre entier.
//!
//! Le programme se lit dans `LYDITE_RESEAU`, sinon `lydite-reseau` à côté de l'exécutable de Lydite.

use std::cell::RefCell;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{self, Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::branchements::bac::temporaire_de;
use crate::branchements::claude::temporaire;
use crate::branchements::processus::{attendre, echeance, retire_de_la_preuve, sans_session, tuer_et_attendre};
use crate::branchements::srt::{Jeu, est_tuee, rejouer_si_tuee};
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{nommee, queue};
use crate::coeur::fil::lot;
use crate::coeur::natif::{Disque, Regles, TOUJOURS_REFUSES, arguments};
use crate::coeur::profil::Profil;
use crate::coeur::verrou::{Architecture, filtre};

/// Le `sh` d'une ligne qui ouvre le filtre (`$1`) sur le descripteur 9, puis lance le second `bwrap` (`$2`) qui le lit
/// et joue la commande (`$3`). Les trois arrivent en paramètres : aucun d'eux ne passe par une citation.
// `--dev-bind /dev /dev` : sans lui, le `--bind / /` de ce second bwrap rend `/dev/null` inutilisable (Permission
// denied), fait constaté par la grille du mur natif.
const POSE_LE_VERROU: &str = concat!(
    r#"exec 9<"$1" && exec "$2" --bind / / --dev-bind /dev /dev --unshare-pid --proc /proc --die-with-parent "#,
    r#"--seccomp 9 -- sh -c "$3""#,
);

/// L'adresse que le relais écoute dans le bac, et que les variables de proxy de la commande désignent.
const ECOUTE: &str = "127.0.0.1:3128";

/// Les variables de la commande qui mènent ses clients HTTP au relais.
const VARIABLES_DU_PROXY: [&str; 6] =
    ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy"];

/// Les variables de la commande qui gardent l'hôte local hors du proxy, et leur valeur.
const VARIABLES_SANS_PROXY: [&str; 2] = ["NO_PROXY", "no_proxy"];
const SANS_PROXY: &str = "localhost,127.0.0.1,::1";

/// La ligne que le filtre écrit sur sa sortie quand il écoute.
const PRET: &str = "ready";

/// Ce qu'un chemin de socket Unix tient d'octets au plus (`sun_path` de 108, moins l'octet nul).
const SOCKET_MAX: usize = 107;

/// Le temps que le filtre a pour dire `ready`, et pour sortir une fois son entrée fermée, avant que son arbre soit tué.
const ATTENTE_DU_FILTRE: Duration = Duration::from_secs(10);

/// Pour qui n'a pas de filtre : comment en donner un.
const DONNER_LE_FILTRE: &str =
    "set LYDITE_RESEAU to the path of a built lydite-reseau, or put lydite-reseau next to the Lydite executable";

/// Le programme `bwrap` : `LYDITE_BWRAP`, sinon `bwrap`.
pub(crate) fn programme() -> OsString {
    programme_selon(env::var_os("LYDITE_BWRAP"))
}

/// Le programme qu'une valeur de `LYDITE_BWRAP` désigne : une valeur absente ou vide laisse `bwrap`.
fn programme_selon(valeur: Option<OsString>) -> OsString {
    valeur.filter(|valeur| !valeur.is_empty()).unwrap_or_else(|| "bwrap".into())
}

/// Le programme `bwrap` part-il ? Il se lance en `--version`, hors de tout mur, sans rien créer : seul compte qu'il
/// parte, son code de sortie n'est pas jugé. Le texte de l'erreur dit pourquoi il ne part pas. Un `bwrap` qui ne se
/// termine pas avant l'échéance est tué : il est parti.
pub(crate) fn bwrap_part(programme: &OsStr) -> Result<(), String> {
    let mut enfant = Command::new(programme)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if !matches!(attendre(&mut enfant, echeance(ATTENTE_DU_FILTRE)), Ok(Some(_))) {
        let _ = tuer_et_attendre(&mut enfant);
    }
    Ok(())
}

/// Ce qui manque au mur natif quand il ne joue pas `true`, d'après l'erreur de son rejeu ([`manque`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Manque {
    /// `lydite-reseau`, le filtre du réseau : introuvable, ou il ne se lance pas.
    Reseau,
    /// Le chemin du socket du filtre est trop long pour un socket Unix : le dossier d'état de Lydite est trop long.
    SocketLong,
    /// `lydite-reseau` part, mais meurt ou se tait avant de dire `ready` : ce qu'il a dit se lit dans l'erreur.
    FiltreMort,
    /// Les espaces d'utilisateur (ou les montages) que le système ferme : `bwrap` part, mais ne monte pas son mur.
    Espaces,
    /// Une autre cause : l'erreur du rejeu la dit.
    Autre,
}

/// Ce que l'erreur d'un rejeu raté dit qui manque au mur natif.
///
/// Le diagnostic se fait d'abord sur ce que le mur écrit lui-même, `vérification « <commande> » impossible : <erreur
/// du mur>`, jamais sur la sortie de la commande, qui peut porter n'importe quel mot : le chemin d'un socket trop
/// long, un filtre qui meurt (ou se tait) avant `ready`, `lydite-reseau` introuvable ou qui ne part pas. Les espaces
/// d'utilisateur, eux, se lisent dans ce que `bwrap` écrit quand le système les ferme (`namespace`, `uid map`, `not
/// permitted`), sa sortie après le statut rouge, et seulement si le mur n'a rien dit de lui-même. Une erreur qui ne
/// commence pas par `impossible :` (`rouge`, `temps dépassé`, ou queue d'une sortie longue) est toute sortie.
pub(crate) fn manque(erreur: &str) -> Manque {
    let du_mur = erreur.lines().next().is_some_and(|ligne| ligne.contains("» impossible : "));
    let (mur, sortie) = if du_mur { (erreur, "") } else { ("", erreur) };
    let (mur, sortie) = (mur.to_lowercase(), sortie.to_lowercase());
    let dit = |texte: &str, mots: &[&str]| mots.iter().any(|mot| texte.contains(mot));
    if dit(&mur, &["network filter socket path"]) {
        Manque::SocketLong
    } else if dit(&mur, &["exited before it said", "did not say ready"]) {
        Manque::FiltreMort
    } else if dit(&mur, &["needs lydite-reseau", "lydite_reseau is not set", "cannot start the network filter"]) {
        Manque::Reseau
    } else if dit(&sortie, &["namespace", "uid map", "gid map", "not permitted"]) {
        Manque::Espaces
    } else {
        Manque::Autre
    }
}

/// Le dossier personnel, où se déplie `~` : `HOME`. Absent ou vide, ou pas absolu, il ne se déplie pas, et le mur ne
/// saurait pas quoi fermer.
pub(crate) fn maison() -> Result<PathBuf, String> {
    maison_selon(env::var_os("HOME"))
}

/// Le dossier personnel qu'une valeur de `HOME` désigne.
fn maison_selon(valeur: Option<OsString>) -> Result<PathBuf, String> {
    let maison = PathBuf::from(valeur.unwrap_or_default());
    if maison.has_root() {
        Ok(maison)
    } else {
        let dit = maison.display();
        Err(format!("HOME ne donne pas de dossier personnel absolu ({dit}) : le mur ne sait pas où ferme `~`"))
    }
}

/// L'architecture du filtre : celle de ce binaire. Une autre n'a pas de filtre.
fn architecture() -> Result<Architecture, String> {
    if cfg!(target_arch = "x86_64") {
        Ok(Architecture::X86_64)
    } else if cfg!(target_arch = "aarch64") {
        Ok(Architecture::Aarch64)
    } else {
        Err("le filtre d'appels système du mur n'existe que pour x86_64 et aarch64".into())
    }
}

/// Le programme `lydite-reseau` : `LYDITE_RESEAU`, sinon `lydite-reseau` à côté de l'exécutable de Lydite. C'est le
/// chemin que ces deux sources donnent, pas encore vu sur le disque : [`preparer`] le lit, et seulement quand le profil
/// permet des domaines. Err si ni l'une ni l'autre ne donne de chemin, et le texte dit les deux façons d'en donner un.
pub(crate) fn reseau() -> Result<PathBuf, String> {
    reseau_selon(env::var_os("LYDITE_RESEAU"), env::current_exe())
}

/// Le programme `lydite-reseau` qu'une valeur de `LYDITE_RESEAU` et l'exécutable de Lydite désignent : une valeur
/// absente ou vide laisse le programme voisin de l'exécutable.
fn reseau_selon(valeur: Option<OsString>, exe: io::Result<PathBuf>) -> Result<PathBuf, String> {
    if let Some(valeur) = valeur.filter(|valeur| !valeur.is_empty()) {
        return Ok(PathBuf::from(valeur));
    }
    let nom = format!("lydite-reseau{}", env::consts::EXE_SUFFIX);
    match exe {
        Ok(exe) => exe.parent().map(|dossier| dossier.join(nom)).ok_or_else(|| {
            format!(
                "LYDITE_RESEAU is not set, and the Lydite executable {} has no folder: {DONNER_LE_FILTRE}",
                exe.display()
            )
        }),
        Err(e) => {
            Err(format!("LYDITE_RESEAU is not set, and the Lydite executable is unknown ({e}): {DONNER_LE_FILTRE}"))
        }
    }
}

/// Ce que le rejeu règle hors du profil : où est `lydite-reseau`, et les deux échéances du filtre (le temps de dire
/// `ready`, puis le temps de sortir une fois son entrée fermée). [`rejouer`] le tire de l'environnement ; un test le
/// donne.
struct Reglage {
    filtre: Result<PathBuf, String>,
    pret: Duration,
    arret: Duration,
}

/// Rejoue la commande de vérification dans le bac sous le mur natif, `programme` étant le `bwrap` et `maison` le
/// dossier où `~` se déplie. Rend son `verif` : `None` si elle n'a pas pu partir, sinon la fin de sa sortie (les
/// chemins sautés d'abord, puis la sortie de la commande) ; et Ok si elle réussit avant le délai, sinon Err(erreur),
/// dont le texte est celui du rejeu par srt (`rouge (<statut>)`, `temps dépassé (<s> s)`). Un profil qui n'exige pas
/// le mur ne part pas : rien n'est créé. Une commande blanche non plus. Un profil qui permet des domaines part avec
/// le filtre réseau (voir l'en-tête) ; sans `lydite-reseau`, ou avec un chemin de socket trop long, il ne part pas non
/// plus, et rien n'est créé. Le fichier du filtre d'appels système est retiré dans tous les cas, le filtre réseau
/// arrêté et son dossier retiré, et le journal lu puis effacé.
pub(crate) fn rejouer(
    programme: &OsStr,
    maison: &Path,
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
) -> (Option<String>, Result<(), String>) {
    let reglage = Reglage { filtre: reseau(), pret: ATTENTE_DU_FILTRE, arret: ATTENTE_DU_FILTRE };
    rejouer_avec(programme, maison, profil, commande, bac, journal, &reglage)
}

/// [`rejouer`], avec les `reglage`s donnés.
fn rejouer_avec(
    programme: &OsStr,
    maison: &Path,
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
    reglage: &Reglage,
) -> (Option<String>, Result<(), String>) {
    if commande.trim().is_empty() {
        return (None, Err(format!("vérification « {commande} » blanche : rien ne se rejoue, rien n'est prouvé")));
    }
    rejouer_si_tuee(|| jouer_sous_le_mur(programme, maison, profil, commande, bac, journal, reglage))
}

/// Un jeu de la preuve sous le mur natif : le mur préparé de nouveau, et le journal repris à neuf. Rend son rejeu, et
/// le statut de la commande si elle a fini tuée ([`est_tuee`]).
fn jouer_sous_le_mur(
    programme: &OsStr,
    maison: &Path,
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
    reglage: &Reglage,
) -> Jeu {
    let delai = profil.delai_preuve();
    let mur = match preparer(maison, profil, bac, reglage) {
        Ok(mur) => mur,
        Err(erreur) => return ((None, Err(format!("vérification « {commande} » impossible : {erreur}"))), None),
    };
    let jouee = executer(programme, commande, bac, delai, journal, profil.variable_de_la_cle(), &mur);
    let _ = fs::remove_file(&mur.filtre);
    // Même quand la vérification n'a pas pu partir : le journal que le mur a commencé se lit et s'efface.
    let sortie = lire_journal(journal);
    let (issue, arret) = match jouee {
        Ok(jouee) => jouee,
        Err(erreur) => return ((None, Err(erreur)), None),
    };
    let tuee = match &issue {
        Ok(Some(statut)) if est_tuee(statut) => Some(*statut),
        _ => None,
    };
    let rejeu = match issue {
        Err(erreur) => Err(erreur),
        Ok(Some(statut)) if statut.success() => Ok(()),
        Ok(Some(statut)) => Err(queue(&format!("vérification « {commande} » rouge ({statut}) :\n{sortie}"))),
        Ok(None) => {
            Err(queue(&format!("vérification « {commande} » : temps dépassé ({} s) :\n{sortie}", delai.as_secs_f32())))
        }
    };
    // Un filtre qui ne s'arrête pas n'est jamais tu : même une preuve verte le dit.
    let rejeu = match (rejeu, arret) {
        (rejeu, None) => rejeu,
        (Ok(()), Some(echec)) => Err(format!("vérification « {commande} » : {echec}")),
        (Err(erreur), Some(echec)) => Err(format!("{erreur}\n{echec}")),
    };
    ((Some(queue(&sortie)), rejeu), tuee)
}

/// Ce que [`preparer`] a posé pour un rejeu.
struct Mur {
    /// Les arguments du `bwrap` extérieur, sans la commande.
    arguments: Vec<OsString>,
    /// Le fichier du filtre, écrit : à retirer après le rejeu.
    filtre: PathBuf,
    /// Le dossier temporaire du bac, recréé vide.
    temporaire: PathBuf,
    /// Les chemins que le mur a sautés, une ligne chacun, pour le journal.
    notes: Vec<String>,
    /// Le filtre réseau que le profil demande, `None` sans domaine.
    reseau: Option<Reseau>,
}

/// Le filtre réseau d'un rejeu, prévu par [`preparer`] et pas encore lancé : rien de lui n'existe sur le disque.
struct Reseau {
    /// Les domaines que le profil permet.
    domaines: Vec<String>,
    /// `lydite-reseau`, par son chemin réel : le même chemin se monte dans le bac.
    programme: PathBuf,
    /// Le dossier neuf du socket, à côté du temporaire du bac.
    dossier: PathBuf,
    /// Le socket du filtre, dans `dossier`.
    socket: PathBuf,
    /// Le journal du filtre, dans le dossier de Lydite.
    journal: PathBuf,
    /// Le temps qu'a le filtre pour dire `ready`, puis pour sortir une fois son entrée fermée.
    pret: Duration,
    arret: Duration,
}

/// Le filtre réseau, prévu pour un profil qui permet `domaines`, dont le temporaire du bac est `temporaire` : le
/// programme est vu sur le disque et le chemin du socket est mesuré, rien n'est créé.
fn prevoir_le_reseau(
    profil: &Profil,
    temporaire: &Path,
    domaines: Vec<String>,
    reglage: &Reglage,
) -> Result<Reseau, String> {
    let programme = programme_reel(&reglage.filtre)?;
    let Some(lydite) = profil.dossier_de_lydite() else {
        return Err("the profile has no Lydite folder to keep the network filter's log in".into());
    };
    // Un nom court (dix hexadécimaux du lot, comme le temporaire du bac en a douze) : le chemin du socket tient dans
    // 107 octets, et le dossier du bac en prend déjà une bonne part.
    let dossier = temporaire.with_file_name(format!("{}.net", &hex(&sha256(lot().as_bytes()))[..10]));
    let socket = dossier.join("s");
    socket_assez_court(&socket)?;
    let journal = lydite.join("reseau.journal");
    Ok(Reseau { domaines, programme, dossier, socket, journal, pret: reglage.pret, arret: reglage.arret })
}

/// Le chemin réel de `lydite-reseau` : un fichier que le disque tient. Le refus nomme les deux façons de le donner.
fn programme_reel(filtre: &Result<PathBuf, String>) -> Result<PathBuf, String> {
    let chemin = filtre.as_ref().map_err(String::clone)?;
    let manque = |pourquoi: String| {
        format!(
            "the profile allows domains, so the native wall needs lydite-reseau, and {pourquoi}: {DONNER_LE_FILTRE}"
        )
    };
    match fs::canonicalize(chemin) {
        Ok(reel) if reel.is_file() => Ok(reel),
        Ok(reel) => Err(manque(format!("{} is not a file", reel.display()))),
        Err(e) => Err(manque(format!("{} cannot be read ({e})", chemin.display()))),
    }
}

/// Un chemin de socket Unix de plus de [`SOCKET_MAX`] octets ne se lie pas : il est refusé avant de lancer.
fn socket_assez_court(socket: &Path) -> Result<(), String> {
    let octets = socket.as_os_str().len();
    if octets > SOCKET_MAX {
        return Err(format!(
            "the network filter socket path {} is {octets} bytes long, and a Unix socket path holds {SOCKET_MAX} at \
             most: use a shorter sandbox folder",
            socket.display()
        ));
    }
    Ok(())
}

/// L'issue d'une vérification partie : son statut, None si elle dépasse le délai, ou l'erreur de son attente.
type Issue = Result<Option<ExitStatus>, String>;

/// Ce que le disque dit d'un chemin, par son chemin réel : absent (introuvable, ou sous un fichier), fichier ou
/// dossier. Toute autre erreur est gardée dans `echec` : un chemin dont on ignore l'état ne se tient pas pour absent,
/// ce que le mur ne fermerait pas.
fn lire_le_disque(chemin: &Path, echec: &RefCell<Option<String>>) -> Disque {
    match fs::canonicalize(chemin) {
        Ok(reel) if reel.is_dir() => Disque::Dossier(reel),
        Ok(reel) => Disque::Fichier(reel),
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) => Disque::Absent,
        Err(e) => {
            echec.borrow_mut().get_or_insert_with(|| nommee(e, format!("chemin {}", chemin.display())).to_string());
            Disque::Absent
        }
    }
}

/// Les arguments du mur pour un rejeu dans `bac`, et son fichier du filtre écrit : le dossier temporaire du bac est
/// recréé vide d'abord, comme sous srt. Rien n'est créé si le profil n'exige pas le mur, ni si le filtre réseau qu'il
/// demande ne se prévoit pas ([`prevoir_le_reseau`]), ni, du filtre d'appels système, si le mur ne se forme pas.
fn preparer(maison: &Path, profil: &Profil, bac: &Path, reglage: &Reglage) -> Result<Mur, String> {
    let absolu = path::absolute(bac).map_err(|e| format!("chemin absolu du bac {} : {e}", bac.display()))?;
    let Some(dossier) = temporaire_de(&absolu) else {
        return Err(format!("le bac {} n'a pas de nom, son dossier temporaire ne se forme pas", bac.display()));
    };
    let Some(listes) = profil.listes_du_mur(&absolu, &dossier)? else {
        return Err("le profil n'exige pas le mur : le mur natif ne se joue que sous un mur exigé".into());
    };
    let reseau = if listes.domaines.is_empty() {
        None
    } else {
        Some(prevoir_le_reseau(profil, &dossier, listes.domaines.clone(), reglage)?)
    };
    let architecture = architecture()?;
    if temporaire(bac, true)?.is_none() {
        return Err(format!("le bac {} n'est pas un dossier : le mur n'y monte rien", bac.display()));
    }

    let chemins = |liste: &[String]| liste.iter().map(PathBuf::from).collect::<Vec<_>>();
    let (fermes, lus, ecrits, refuses) =
        (chemins(&listes.fermes), chemins(&listes.lus), chemins(&listes.ecrits), chemins(&listes.refuses));
    let regles = Regles {
        maison,
        fermes: &fermes,
        lus: &lus,
        ecrits: &ecrits,
        refuses: &refuses,
        bac: &absolu,
        toujours: &TOUJOURS_REFUSES,
    };
    let echec = RefCell::new(None);
    let natif = arguments(&regles, |chemin| lire_le_disque(chemin, &echec))?;
    if let Some(erreur) = echec.into_inner() {
        return Err(erreur);
    }
    let sautes = natif.sautes.iter().map(|chemin| format!("lu ou écrit par le profil, absent : {}", chemin.display()));
    let fermes_absents =
        natif.fermes_absents.iter().map(|chemin| format!("fermé par le profil, absent : {}", chemin.display()));
    let notes = sautes.chain(fermes_absents).map(|note| format!("mur natif : {note}")).collect();

    let fichier = dossier.join(format!("verrou-{}.bpf", lot()));
    if let Err(e) = fs::write(&fichier, filtre(architecture)) {
        let _ = fs::remove_file(&fichier);
        return Err(nommee(e, format!("filtre {}", fichier.display())).to_string());
    }
    Ok(Mur { arguments: natif.arguments, filtre: fichier, temporaire: dossier, notes, reseau })
}

/// Le filtre réseau lancé : le processus, son entrée que Lydite tient ouverte, et le dossier de son socket. Il s'arrête
/// par [`Filtre::arreter`], et à défaut, en tombant : aucune sortie de [`executer`] ne le laisse vivant.
struct Filtre {
    enfant: Child,
    /// Son entrée standard : la fermer l'arrête.
    entree: Option<ChildStdin>,
    /// Le lecteur de sa sortie, qui finit quand elle se ferme.
    sortie: Option<JoinHandle<()>>,
    /// Le lecteur de ses erreurs, qui rend ce qu'il a lu (8 Kio au plus).
    erreurs: Option<JoinHandle<String>>,
    /// Le dossier de son socket, à retirer.
    dossier: PathBuf,
    /// Le temps qu'il a pour sortir une fois son entrée fermée.
    arret: Duration,
    /// Il est sorti, tué ou de lui-même : ses lecteurs ne bloquent plus.
    sorti: bool,
}

impl Filtre {
    /// Ferme son entrée, attend qu'il sorte jusqu'à l'échéance, tue son arbre s'il ne sort pas, puis retire le dossier
    /// du socket, même quand il ne s'est pas arrêté. Rend ce qui a échoué. Se rejoue sans dommage.
    fn arreter(&mut self) -> Result<(), String> {
        drop(self.entree.take());
        let sorti = match attendre(&mut self.enfant, echeance(self.arret)) {
            Ok(Some(_)) => Ok(()),
            Ok(None) | Err(_) => tuer_et_attendre(&mut self.enfant),
        };
        self.sorti = sorti.is_ok();
        if self.sorti
            && let Some(lecteur) = self.sortie.take()
        {
            let _ = lecteur.join();
        }
        let retire = match fs::remove_dir_all(&self.dossier) {
            Err(e) if e.kind() != ErrorKind::NotFound => {
                Err(format!("cannot remove the network filter folder {}: {e}", self.dossier.display()))
            }
            _ => Ok(()),
        };
        let sorti = sorti.map_err(|e| format!("the network filter does not stop: {e}"));
        match (sorti, retire) {
            (Ok(()), retire) => retire,
            (sorti, Ok(())) => sorti,
            (Err(sorti), Err(retire)) => Err(format!("{sorti}; {retire}")),
        }
    }

    /// Ce que le filtre a écrit sur ses erreurs, une fois sorti ; vide s'il ne l'est pas, ou n'a rien écrit.
    fn erreurs(&mut self) -> String {
        match self.erreurs.take() {
            Some(lecteur) if self.sorti => lecteur.join().unwrap_or_default().trim().to_string(),
            _ => String::new(),
        }
    }
}

impl Drop for Filtre {
    fn drop(&mut self) {
        let _ = self.arreter();
    }
}

/// Crée `dossier`, neuf, en 0700 : lui seul, et ce que le bac y monte, le tient.
#[cfg(unix)]
fn dossier_prive(dossier: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(dossier)
}

/// Sans Unix, il n'y a pas de socket Unix pour le filtre ; le dossier se crée tel quel.
#[cfg(not(unix))]
fn dossier_prive(dossier: &Path) -> io::Result<()> {
    fs::create_dir(dossier)
}

/// Lance le filtre réseau de `reseau`, hors du bac, et attend sa ligne `ready`. Le dossier de son socket est créé neuf
/// avant lui ; si le filtre ne dit pas `ready`, il est arrêté et le dossier retiré, et l'erreur le dit : rien ne
/// reste.
fn demarrer_le_filtre(reseau: &Reseau) -> Result<Filtre, String> {
    let dossier = &reseau.dossier;
    dossier_prive(dossier).map_err(|e| format!("cannot make the network filter folder {}: {e}", dossier.display()))?;
    let lance = lancer_le_filtre(reseau);
    if lance.is_err() {
        let _ = fs::remove_dir_all(dossier);
    }
    lance
}

/// Pourquoi le filtre n'a pas dit `ready` : sa sortie s'est fermée (il est mort), ou l'échéance est tombée.
enum Silence {
    Mort,
    Muet,
}

/// Attend sur `lignes`, les lignes de la sortie du filtre, la ligne `ready`, jusqu'à `delai`.
fn attendre_pret(lignes: &Receiver<String>, delai: Duration) -> Result<(), Silence> {
    let fin = echeance(delai);
    loop {
        match lignes.recv_timeout(fin.saturating_duration_since(Instant::now())) {
            Ok(ligne) if ligne.trim() == PRET => return Ok(()),
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => return Err(Silence::Muet),
            Err(RecvTimeoutError::Disconnected) => return Err(Silence::Mort),
        }
    }
}

/// Le lancement de [`demarrer_le_filtre`], le dossier du socket étant déjà créé.
fn lancer_le_filtre(reseau: &Reseau) -> Result<Filtre, String> {
    let nom = reseau.programme.display();
    let mut enfant = Command::new(&reseau.programme)
        .arg("filtre")
        .arg("--socket")
        .arg(&reseau.socket)
        .arg("--domaines")
        .arg(reseau.domaines.join(","))
        .arg("--journal")
        .arg(&reseau.journal)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start the network filter {nom}: {e}"))?;
    let (Some(entree), Some(sortie), Some(erreurs)) = (enfant.stdin.take(), enfant.stdout.take(), enfant.stderr.take())
    else {
        let _ = tuer_et_attendre(&mut enfant);
        return Err(format!("cannot reach the pipes of the network filter {nom}"));
    };
    let (envoi, lignes) = mpsc::channel();
    let sortie = thread::spawn(move || {
        // Elle se lit jusqu'à sa fermeture, même quand plus personne n'attend de ligne.
        for ligne in BufReader::new(sortie).lines().map_while(Result::ok) {
            let _ = envoi.send(ligne);
        }
    });
    let erreurs = thread::spawn(move || {
        let mut octets = Vec::new();
        let _ = erreurs.take(8192).read_to_end(&mut octets);
        String::from_utf8_lossy(&octets).into_owned()
    });
    let (dossier, arret) = (reseau.dossier.clone(), reseau.arret);
    let mut filtre = Filtre {
        enfant,
        entree: Some(entree),
        sortie: Some(sortie),
        erreurs: Some(erreurs),
        dossier,
        arret,
        sorti: false,
    };
    let Err(silence) = attendre_pret(&lignes, reseau.pret) else { return Ok(filtre) };
    // Il n'a rien dit de ce qu'on attend : il s'arrête, et ce qu'il a écrit sur ses erreurs en dit la raison.
    let arrete = filtre.arreter();
    let statut = filtre.enfant.try_wait().ok().flatten().map(|statut| format!(" ({statut})")).unwrap_or_default();
    let erreurs = filtre.erreurs();
    let detail = if erreurs.is_empty() { String::new() } else { format!(": {erreurs}") };
    let mut texte = match silence {
        Silence::Mort => format!("the network filter {nom} exited before it said {PRET}{statut}{detail}"),
        Silence::Muet => format!("the network filter {nom} did not say {PRET} within {} s", reseau.pret.as_secs_f32()),
    };
    if let Err(echec) = arrete {
        texte = format!("{texte}; {echec}");
    }
    Err(texte)
}

/// La sortie de la vérification, lue dans son journal, qui s'efface ensuite. Un journal qui ne se lit pas le dit,
/// « journal illisible : <journal> : <erreur> », et ne rend jamais la chaîne vide.
fn lire_journal(journal: &Path) -> String {
    let sortie = fs::read(journal).map_or_else(
        |e| format!("journal illisible : {}", nommee(e, journal.display())),
        |octets| String::from_utf8_lossy(&octets).into_owned(),
    );
    let _ = fs::remove_file(journal);
    sortie
}

/// Joue la commande dans le bac sous `mur` ([`jouer_la_commande`]), après avoir lancé le filtre réseau que `mur`
/// demande, et l'arrête après, quelle que soit l'issue : succès, échec, délai dépassé. Rend l'issue de la commande et
/// l'échec de l'arrêt du filtre, s'il y en a un ; Err si rien n'a pu partir, le filtre n'ayant pas dit `ready` compris.
/// Quand c'est la commande qui n'a pas pu partir, l'échec de l'arrêt du filtre s'ajoute à son erreur, sur sa ligne.
fn executer(
    programme: &OsStr,
    commande: &str,
    bac: &Path,
    delai: Duration,
    journal: &Path,
    cle: Option<&str>,
    mur: &Mur,
) -> Result<(Issue, Option<String>), String> {
    let mut filtre = match &mur.reseau {
        Some(reseau) => {
            Some(demarrer_le_filtre(reseau).map_err(|e| format!("vérification « {commande} » impossible : {e}"))?)
        }
        None => None,
    };
    let issue = jouer_la_commande(programme, commande, bac, delai, journal, cle, mur);
    let arret = filtre.as_mut().and_then(|filtre| filtre.arreter().err());
    match (issue, arret) {
        (Ok(issue), arret) => Ok((issue, arret)),
        (Err(erreur), None) => Err(erreur),
        // Comme `rejouer_avec` pour une commande qui a tourné : l'échec de l'arrêt s'ajoute à l'erreur.
        (Err(erreur), Some(echec)) => Err(format!("{erreur}\n{echec}")),
    }
}

/// Joue la commande dans le bac sous `mur`, sa sortie dans `journal`, sans les variables que [`retire_de_la_preuve`]
/// désigne, et sans `cle`, la variable qui tient la clé du fournisseur du profil s'il en nomme un : la clé ne va qu'à
/// la main. `TMPDIR` vaut le dossier temporaire du mur. Avec un filtre réseau (`mur.reseau`, déjà lancé), le dossier du
/// socket et `lydite-reseau` se montent dans le bac, le relais part avant le verrou, et les variables de proxy de la
/// commande le désignent. Err si elle n'a pas pu partir (journal impossible à écrire, programme qui ne part pas),
/// sinon son issue. Au délai, son arbre entier meurt.
fn jouer_la_commande(
    programme: &OsStr,
    commande: &str,
    bac: &Path,
    delai: Duration,
    journal: &Path,
    cle: Option<&str>,
    mur: &Mur,
) -> Result<Issue, String> {
    let echec = |e: io::Error| format!("vérification « {commande} » impossible : {e}");
    // La sortie va dans un fichier, pas dans un tuyau : un petit-enfant qui survit ne bloque pas la lecture.
    let mut fichier = File::create(journal).map_err(|e| echec(nommee(e, format!("journal {}", journal.display()))))?;
    for note in &mur.notes {
        writeln!(fichier, "{note}").map_err(|e| echec(nommee(e, format!("journal {}", journal.display()))))?;
    }
    let mut verification = Command::new(programme);
    verification.args(&mur.arguments);
    if let Some(reseau) = &mur.reseau {
        // Après tous les montages du mur : ni la fermeture d'un dossier ni la racine ne les recouvrent.
        // Le dossier du socket est en lecture seule : rien ne s'y crée, ne s'y retire ni ne s'y renomme.
        verification.arg("--ro-bind").arg(&reseau.dossier).arg(&reseau.dossier);
        verification.arg("--ro-bind").arg(&reseau.programme).arg(&reseau.programme);
    }
    verification.arg("--");
    if let Some(reseau) = &mur.reseau {
        // Le relais écoute, puis lance ce qui suit : le second `bwrap`, qui pose le verrou, n'est donc pas encore là.
        verification.arg(&reseau.programme).args(["relais", "--ecoute", ECOUTE, "--vers"]).arg(&reseau.socket);
        verification.arg("--");
    }
    verification.args(["sh", "-c", POSE_LE_VERROU, "sh"]);
    verification.arg(&mur.filtre).arg(programme).arg(commande);
    if let Some(cle) = cle {
        verification.env_remove(cle);
    }
    sans_session(&mut verification, retire_de_la_preuve);
    if mur.reseau.is_some() {
        let proxy = format!("http://{ECOUTE}");
        for nom in VARIABLES_DU_PROXY {
            verification.env(nom, &proxy);
        }
        for nom in VARIABLES_SANS_PROXY {
            verification.env(nom, SANS_PROXY);
        }
    }
    let mut enfant = verification
        .env("TMPDIR", &mur.temporaire)
        .current_dir(bac)
        .stdin(Stdio::null())
        .stdout(fichier.try_clone().map_err(echec)?)
        .stderr(fichier)
        .spawn()
        .map_err(echec)?;
    let statut = match attendre(&mut enfant, echeance(delai)) {
        Err(e) => return Ok(Err(format!("vérification « {commande} » : attente impossible : {e}"))),
        Ok(statut) => statut,
    };
    if statut.is_none()
        && let Err(e) = tuer_et_attendre(&mut enfant)
    {
        let secondes = delai.as_secs_f32();
        return Ok(Err(format!("vérification « {commande} » : temps dépassé ({secondes} s) ; mise à mort : {e}")));
    }
    Ok(Ok(statut))
}

#[cfg(test)]
mod tests;
