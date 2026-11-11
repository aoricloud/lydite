//! Le rejeu de la preuve d'une main, par la coquille, sous srt ou sous le mur natif, et l'épreuve du mur.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io;
use std::path::{self, Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use crate::branchements::bac::temporaire_de;
use crate::branchements::claude::temporaire;
use crate::branchements::natif::{self, Manque};
use crate::branchements::processus::{
    attendre, coquille, echeance, retire_de_la_preuve, sans_session, tuer_et_attendre,
};
use crate::coeur::erreur::{Code, Refus, nommee, queue};
use crate::coeur::fil::lot;
use crate::coeur::profil::{MurDuRejeu, Profil, Srt};

/// Le mur natif tel que l'environnement le donne : le `bwrap` (`LYDITE_BWRAP`) et le dossier personnel (`HOME`) que
/// [`natif::rejouer`] prend. Lu une fois par [`rejouer`] et [`eprouver_le_mur`] ; un test le donne.
struct MurNatif {
    programme: OsString,
    maison: Result<PathBuf, String>,
}

impl MurNatif {
    fn de_l_environnement() -> MurNatif {
        MurNatif { programme: natif::programme(), maison: natif::maison() }
    }
}

/// Rejoue la commande de vérification dans le bac. Rend son `verif` : `None` si elle n'a pas pu partir, sinon la fin de
/// sa sortie, « journal illisible : … » si son journal ne se lit pas ; et Ok si elle réussit avant le délai, sinon
/// Err(erreur). Sa sortie passe par `journal`, un fichier temporaire effacé dans tous les cas. Une commande blanche ne
/// part pas : elle ferait prendre toute main pour prouvée. Le profil choisit le mur : sous `mur = exige`, elle se
/// rejoue par srt ([`sous_le_mur`]) ; sous `mur = natif`, par le mur natif ([`natif::rejouer`]), sans chercher ni
/// lancer srt, et une preuve qui ne s'y rejoue pas est une erreur, jamais une coquille nue.
// raccourci : sous un mur demandé, la preuve se rejoue hors du mur, avec les droits de l'humain ; le code que la main
// laisse peut y écrire le fil ou une empreinte ; monter avec le compte séparé du nœud
pub(crate) fn rejouer(
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
) -> (Option<String>, Result<(), String>) {
    rejouer_avec(profil, commande, bac, journal, &MurNatif::de_l_environnement())
}

/// [`rejouer`], avec le mur natif donné.
fn rejouer_avec(
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
    natif: &MurNatif,
) -> (Option<String>, Result<(), String>) {
    if commande.trim().is_empty() {
        return (None, Err(format!("vérification « {commande} » blanche : rien ne se rejoue, rien n'est prouvé")));
    }
    if profil.mur_du_rejeu() == MurDuRejeu::Natif {
        return match &natif.maison {
            Ok(maison) => natif::rejouer(&natif.programme, maison, profil, commande, bac, journal),
            Err(erreur) => (None, Err(format!("vérification « {commande} » impossible : {erreur}"))),
        };
    }
    let delai = profil.delai_preuve();
    let srt = match sous_le_mur(profil, bac) {
        Ok(srt) => srt,
        Err(erreur) => return (None, Err(format!("vérification « {commande} » impossible : {erreur}"))),
    };
    let issue = executer(commande, bac, delai, journal, profil.variable_de_la_cle(), srt.as_ref());
    if let Some(srt) = &srt {
        let _ = fs::remove_file(&srt.fichier);
    }
    // Même quand la vérification n'a pas pu partir : la coquille qui ne part pas laisse un journal vide.
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

/// La correction d'un mur exigé que srt ne tient pas (6028).
const MUR_REFUSE: &str = if cfg!(windows) {
    "Sous Windows, le bac à sable de Claude Code et srt ne tiennent pas, donc un mur exigé n'y tient pas. Pour lancer \
     quand même, en connaissance de cause : écris `mur = demande` dans le profil (pour le profil intégré, un fichier \
     `profils/claude.profil` qui reprend son texte avec `mur = demande`), puis lydite accepter <profil>. Sous un mur \
     demandé, les commandes de la main tournent hors du bac à sable et y voient la clé du fournisseur s'il y en a un, \
     et la preuve se rejoue hors du mur. Puis relance."
} else {
    "La cause la plus probable : srt, ou le programme d'isolation qu'il lance, est installé sous un dossier que le mur \
     ferme (le dossier personnel), et rend 127 ; installe-le hors de ce dossier, ou pose LYDITE_SRT au chemin d'un srt \
     qui y joue `srt --settings <fichier> sh -c true`. Puis relance."
};

/// L'épreuve du mur : sous un mur exigé, avant toute main et avant l'épreuve de la base, `true` se rejoue par srt
/// comme une preuve ([`rejouer`]), dans un dossier neuf du dossier de Kraken, `bacs/mur-<lot>`, retiré ensuite avec son
/// dossier temporaire (journal `verif-<lot>-mur.tmp`, effacé). Si srt ne rend pas 0, ou ne se lance pas, c'est le refus
/// 6028, et rien n'est lancé. Sous un mur demandé, rien ne se joue ni ne se crée. Un dossier qui ne se crée pas est le
/// refus 6006.
///
/// Sous `mur = natif`, `true` se joue dans le mur natif ([`rejouer`]), avec le filtre du réseau si le profil demande
/// des domaines, et srt n'est ni cherché ni lancé. Le refus 6028 nomme ce qui manque ([`refus_du_mur_natif`]) : un
/// `bwrap` qui ne part pas est refusé avant que rien soit créé.
pub(crate) fn eprouver_le_mur(profil: &Profil, dossier: &Path) -> Result<(), Refus> {
    eprouver_le_mur_avec(profil, dossier, &MurNatif::de_l_environnement())
}

/// [`eprouver_le_mur`], avec le mur natif donné.
fn eprouver_le_mur_avec(profil: &Profil, dossier: &Path, natif: &MurNatif) -> Result<(), Refus> {
    if !profil.exige() {
        return Ok(());
    }
    let sous_natif = profil.mur_du_rejeu() == MurDuRejeu::Natif;
    if sous_natif && let Err(e) = natif::bwrap_part(&natif.programme) {
        return Err(refus_du_mur_natif(&natif.programme, &Cause::Bwrap(e)));
    }
    let lot = lot();
    let worktree = dossier.join("bacs").join(format!("mur-{lot}"));
    fs::create_dir_all(&worktree).map_err(|e| nommee(e, format!("création du dossier {}", worktree.display())))?;
    let journal = dossier.join(format!("verif-{lot}-mur.tmp"));
    let (_, rejeu) = rejouer_avec(profil, "true", &worktree, &journal, natif);
    let _ = fs::remove_dir_all(&worktree);
    if let Some(temporaire) = temporaire_de(&worktree) {
        let _ = fs::remove_dir_all(temporaire);
    }
    rejeu.map_err(|erreur| {
        if sous_natif {
            refus_du_mur_natif(&natif.programme, &Cause::Rejeu(natif::manque(&erreur), erreur))
        } else {
            let preuve = format!("the wall is required, and srt does not play `true`: {erreur}");
            Refus::new(Code::MurRefuse, preuve, MUR_REFUSE)
        }
    })
}

/// Le début de la preuve de tout refus 6028 du mur natif ([`refus_du_mur_natif`]).
const PREUVE_DU_MUR_NATIF: &str = "the native wall (`mur = natif`)";

/// Le repli que la correction de tout refus du mur natif propose : srt rejoue l'épreuve et la preuve.
const REPLI_SRT: &str = "To use srt instead, write `mur = exige` in the profile, accept it again with `lydite \
                         accept <profile>`, and launch again: the wall check and the replay of your proof then go \
                         through srt.";

/// Pourquoi le mur natif est refusé.
enum Cause {
    /// `bwrap` ne part pas : le texte de l'erreur.
    Bwrap(String),
    /// `true` ne s'est pas joué : ce qui manque d'après son erreur, et l'erreur.
    Rejeu(Manque, String),
}

/// Le refus 6028 d'un mur natif qui ne tient pas, `programme` étant son `bwrap`. Sa preuve commence par
/// [`PREUVE_DU_MUR_NATIF`] et nomme ce qui manque : bubblewrap (`LYDITE_BWRAP`), `lydite-reseau` (`LYDITE_RESEAU`), ou
/// les espaces d'utilisateur que le système ferme. Sa correction dit quoi installer ou poser, et que `mur = exige`
/// rejoue par srt.
fn refus_du_mur_natif(programme: &OsStr, cause: &Cause) -> Refus {
    let programme = programme.to_string_lossy();
    let (preuve, correction) = match cause {
        Cause::Bwrap(e) => (
            format!("needs bubblewrap (LYDITE_BWRAP), and `{programme}` cannot start: {e}"),
            "The native wall runs on Linux and needs bubblewrap: install it (`bwrap`; on Debian and Ubuntu, `apt \
             install bubblewrap`), or set LYDITE_BWRAP to the path of a bubblewrap that runs."
                .to_string(),
        ),
        Cause::Rejeu(Manque::Reseau, erreur) => (
            format!("needs lydite-reseau (LYDITE_RESEAU), its network filter, and does not play `true`: {erreur}"),
            "Build lydite-reseau (`cargo build --release --manifest-path reseau/Cargo.toml`) and put it next to the \
             `lydite` executable, or set LYDITE_RESEAU to the path of a built lydite-reseau that runs."
                .to_string(),
        ),
        Cause::Rejeu(Manque::SocketLong, erreur) => (
            format!(
                "needs a socket path for lydite-reseau, its network filter, that a Unix socket holds, and the Lydite \
                 state folder is too long for it: {erreur}"
            ),
            "Use a shorter Lydite state folder: set LYDITE_DOSSIER to a shorter absolute path (`lydite doctor` says \
             how long it may be)."
                .to_string(),
        ),
        Cause::Rejeu(Manque::FiltreMort, erreur) => (
            format!("needs lydite-reseau (LYDITE_RESEAU), its network filter, to say ready, and it does not: {erreur}"),
            "lydite-reseau starts, but stops or stays silent before it says ready: read what it said, in the proof \
             above, and fix what it names. LYDITE_RESEAU names the lydite-reseau that runs."
                .to_string(),
        ),
        Cause::Rejeu(Manque::Espaces, erreur) => (
            format!(
                "needs the user namespaces that bubblewrap sets its sandbox up with, and this system closes them, so \
                 `true` does not play: {erreur}"
            ),
            "Open unprivileged user namespaces on this system (on Debian, `sysctl kernel.unprivileged_userns_clone=1`; \
             on Ubuntu 24.04 and later, `sysctl kernel.apparmor_restrict_unprivileged_userns=0`; in a container, \
             allow them to the container), or run Lydite on a system that has them."
                .to_string(),
        ),
        Cause::Rejeu(Manque::Autre, erreur) => (
            format!("does not play `true`: {erreur}"),
            format!(
                "Read what the proof says, and fix what it names. LYDITE_BWRAP names the bubblewrap that runs (here \
                 `{programme}`) and LYDITE_RESEAU the lydite-reseau."
            ),
        ),
    };
    Refus::new(Code::MurRefuse, format!("{PREUVE_DU_MUR_NATIF} {preuve}"), format!("{correction} {REPLI_SRT}"))
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

/// L'issue d'une vérification partie : son statut, None si elle dépasse le délai, ou l'erreur de son attente.
type Issue = Result<Option<ExitStatus>, String>;

/// Le rejeu de la preuve dans `bac` sous un mur exigé (voir [`Profil::rejeu`]), son dossier temporaire
/// ([`temporaire_de`]) recréé vide comme pour une main ([`temporaire`]) ; `None` sous un mur demandé, et rien n'est
/// créé.
fn sous_le_mur(profil: &Profil, bac: &Path) -> Result<Option<Srt>, String> {
    if !profil.exige() {
        return Ok(None);
    }
    let absolu = path::absolute(bac).map_err(|e| format!("chemin absolu du bac {} : {e}", bac.display()))?;
    let Some(dossier) = temporaire_de(&absolu) else {
        return Err(format!("le bac {} n'a pas de nom, son dossier temporaire ne se forme pas", bac.display()));
    };
    let srt = profil.rejeu(&absolu, &dossier)?;
    temporaire(bac, true)?;
    Ok(srt)
}

/// Joue la commande dans le bac, sa sortie dans `journal`, sans les variables que [`retire_de_la_preuve`] désigne, et
/// sans `cle`, la variable qui tient la clé du fournisseur du profil s'il en nomme un : la clé ne va qu'à la main.
/// Avec `srt`, elle part en `<srt> --settings <fichier> sh -c <commande>`, son fichier de réglages écrit d'abord ; srt
/// garde le `TMPDIR` de Kraken, et `CLAUDE_CODE_TMPDIR` vaut son dossier temporaire, que srt donne en `TMPDIR` à la
/// preuve ; le code de srt est celui de la preuve. Err si
/// elle n'a pas pu partir (journal ou réglages impossibles à écrire, programme qui ne part pas), sinon son issue. Au
/// délai, son arbre entier meurt.
fn executer(
    commande: &str,
    bac: &Path,
    delai: Duration,
    journal: &Path,
    cle: Option<&str>,
    srt: Option<&Srt>,
) -> Result<Issue, String> {
    let echec = |e: io::Error| format!("vérification « {commande} » impossible : {e}");
    // La sortie va dans un fichier, pas dans un tuyau : un petit-enfant qui survit ne bloque pas la lecture.
    let fichier = File::create(journal).map_err(|e| echec(nommee(e, format!("journal {}", journal.display()))))?;
    let mut verification = match srt {
        None => coquille(commande),
        Some(srt) => {
            fs::write(&srt.fichier, &srt.reglages)
                .map_err(|e| echec(nommee(e, format!("réglages de srt {}", srt.fichier.display()))))?;
            let mut muree = Command::new(&srt.programme);
            muree.arg("--settings").arg(&srt.fichier).args(["sh", "-c", commande]);
            muree
        }
    };
    if let Some(cle) = cle {
        verification.env_remove(cle);
    }
    sans_session(&mut verification, retire_de_la_preuve);
    // Après `sans_session`, qui retire les `CLAUDE*` de Kraken : sans `CLAUDE_CODE_TMPDIR`, srt donne `/tmp/claude` à
    // la preuve. srt garde le `TMPDIR` de Kraken, où il pose ses propres sockets de pont.
    if let Some(srt) = srt {
        verification.env("CLAUDE_CODE_TMPDIR", &srt.temporaire);
    }
    let mut enfant = verification
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
mod grille;
#[cfg(test)]
mod tests;
