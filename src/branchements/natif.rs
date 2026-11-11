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
//! fin. La commande part après que le filtre est ouvert, jamais avant. Le réseau est coupé en entier (`--unshare-net`
//! des arguments du mur) : un profil qui permet des domaines est refusé avant tout lancement. `TMPDIR` y vaut le
//! dossier temporaire du bac, le seul endroit hors du bac où la preuve écrit, comme sous srt. Les chemins que le mur a
//! sautés parce qu'ils manquent s'écrivent en tête du journal, une ligne chacun.

#![cfg_attr(not(all(test, target_os = "linux")), expect(dead_code))]
// raccourci : rien dans le profil ne mène encore ici, seule la grille (des tests, sous Linux) appelle ce rejeu ;
// l'appelant qui choisit le mur natif à la place de srt le prendra, et rustc dira alors cette attente inutile

use std::cell::RefCell;
use std::env;
use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::{self, ErrorKind, Write};
use std::path::{self, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use crate::branchements::bac::temporaire_de;
use crate::branchements::claude::temporaire;
use crate::branchements::processus::{attendre, echeance, retire_de_la_preuve, sans_session, tuer_et_attendre};
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

/// Le programme `bwrap` : `LYDITE_BWRAP`, sinon `bwrap`.
pub(crate) fn programme() -> OsString {
    programme_selon(env::var_os("LYDITE_BWRAP"))
}

/// Le programme qu'une valeur de `LYDITE_BWRAP` désigne : une valeur absente ou vide laisse `bwrap`.
fn programme_selon(valeur: Option<OsString>) -> OsString {
    valeur.filter(|valeur| !valeur.is_empty()).unwrap_or_else(|| "bwrap".into())
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

/// Rejoue la commande de vérification dans le bac sous le mur natif, `programme` étant le `bwrap` et `maison` le
/// dossier où `~` se déplie. Rend son `verif` : `None` si elle n'a pas pu partir, sinon la fin de sa sortie (les
/// chemins sautés d'abord, puis la sortie de la commande) ; et Ok si elle réussit avant le délai, sinon Err(erreur),
/// dont le texte est celui du rejeu par srt (`rouge (<statut>)`, `temps dépassé (<s> s)`). Un profil qui n'exige pas
/// le mur, ou qui permet des domaines, ne part pas : rien n'est créé. Une commande blanche non plus. Le fichier du
/// filtre est retiré dans tous les cas, et le journal lu puis effacé.
// raccourci : pas de réseau sous le mur natif ; monter le relais avec la vague suivante
pub(crate) fn rejouer(
    programme: &OsStr,
    maison: &Path,
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
) -> (Option<String>, Result<(), String>) {
    if commande.trim().is_empty() {
        return (None, Err(format!("vérification « {commande} » blanche : rien ne se rejoue, rien n'est prouvé")));
    }
    let delai = profil.delai_preuve();
    let mur = match preparer(maison, profil, bac) {
        Ok(mur) => mur,
        Err(erreur) => return (None, Err(format!("vérification « {commande} » impossible : {erreur}"))),
    };
    let issue = executer(programme, commande, bac, delai, journal, profil.variable_de_la_cle(), &mur);
    let _ = fs::remove_file(&mur.filtre);
    // Même quand la vérification n'a pas pu partir : le journal que le mur a commencé se lit et s'efface.
    let sortie = lire_journal(journal);
    let issue = match issue {
        Ok(issue) => issue,
        Err(erreur) => return (None, Err(erreur)),
    };
    let rejeu = match issue {
        Err(erreur) => Err(erreur),
        Ok(Some(statut)) if statut.success() => Ok(()),
        Ok(Some(statut)) => Err(queue(&format!("vérification « {commande} » rouge ({statut}) :\n{sortie}"))),
        Ok(None) => {
            Err(queue(&format!("vérification « {commande} » : temps dépassé ({} s) :\n{sortie}", delai.as_secs_f32())))
        }
    };
    (Some(queue(&sortie)), rejeu)
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
/// recréé vide d'abord, comme sous srt. Rien n'est créé si le profil n'exige pas le mur ou permet des domaines, ni, du
/// filtre, si le mur ne se forme pas.
fn preparer(maison: &Path, profil: &Profil, bac: &Path) -> Result<Mur, String> {
    let absolu = path::absolute(bac).map_err(|e| format!("chemin absolu du bac {} : {e}", bac.display()))?;
    let Some(dossier) = temporaire_de(&absolu) else {
        return Err(format!("le bac {} n'a pas de nom, son dossier temporaire ne se forme pas", bac.display()));
    };
    let Some(listes) = profil.listes_du_mur(&absolu, &dossier)? else {
        return Err("le profil n'exige pas le mur : le mur natif ne se joue que sous un mur exigé".into());
    };
    if !listes.domaines.is_empty() {
        return Err(format!(
            "le mur natif coupe le réseau en entier et ne relaie pas encore de domaine, or le profil en permet ({}) : \
             retire `reseau` du profil pour jouer sous le mur natif",
            listes.domaines.join(", ")
        ));
    }
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
    Ok(Mur { arguments: natif.arguments, filtre: fichier, temporaire: dossier, notes })
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

/// Joue la commande dans le bac sous `mur`, sa sortie dans `journal`, sans les variables que [`retire_de_la_preuve`]
/// désigne, et sans `cle`, la variable qui tient la clé du fournisseur du profil s'il en nomme un : la clé ne va qu'à
/// la main. `TMPDIR` vaut le dossier temporaire du mur. Err si elle n'a pas pu partir (journal impossible à écrire,
/// programme qui ne part pas), sinon son issue. Au délai, son arbre entier meurt.
fn executer(
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
    verification.args(&mur.arguments).args(["--", "sh", "-c", POSE_LE_VERROU, "sh"]);
    verification.arg(&mur.filtre).arg(programme).arg(commande);
    if let Some(cle) = cle {
        verification.env_remove(cle);
    }
    sans_session(&mut verification, retire_de_la_preuve);
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
