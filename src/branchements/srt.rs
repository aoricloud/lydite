//! Le rejeu de la preuve d'une main, par la coquille ou sous srt, et l'épreuve du mur.

use std::fs::{self, File};
use std::io;
use std::path::{self, Path};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use crate::branchements::bac::temporaire_de;
use crate::branchements::claude::temporaire;
use crate::branchements::processus::{
    attendre, coquille, echeance, retire_de_la_preuve, sans_session, tuer_et_attendre,
};
use crate::coeur::erreur::{Code, Refus, nommee, queue};
use crate::coeur::fil::lot;
use crate::coeur::profil::{Profil, Srt};

/// Rejoue la commande de vérification dans le bac. Rend son `verif` : `None` si elle n'a pas pu partir, sinon la fin de
/// sa sortie, « journal illisible : … » si son journal ne se lit pas ; et Ok si elle réussit avant le délai, sinon
/// Err(erreur). Sa sortie passe par `journal`, un fichier temporaire effacé dans tous les cas. Une commande blanche ne
/// part pas : elle ferait prendre toute main pour prouvée. Sous un mur exigé, elle se rejoue par srt ([`sous_le_mur`]).
// raccourci : sous un mur demandé, la preuve se rejoue hors du mur, avec les droits de l'humain ; le code que la main
// laisse peut y écrire le fil ou une empreinte ; monter avec le compte séparé du nœud
pub(crate) fn rejouer(
    profil: &Profil,
    commande: &str,
    bac: &Path,
    journal: &Path,
) -> (Option<String>, Result<(), String>) {
    if commande.trim().is_empty() {
        return (None, Err(format!("vérification « {commande} » blanche : rien ne se rejoue, rien n'est prouvé")));
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
const MUR_REFUSE: &str = "La cause la plus probable : srt, ou le programme d'isolation qu'il lance, est installé sous \
                          un dossier que le mur ferme (le dossier personnel), et rend 127 ; installe-le hors de ce \
                          dossier, ou pose LYDITE_SRT au chemin d'un srt qui y joue `srt --settings <fichier> sh -c \
                          true`. Sous Windows, le mur exigé n'est pas tenu : passe le profil à « mur = demande » et \
                          accepte-le : lydite accepter <profil>. Puis relance.";

/// L'épreuve du mur : sous un mur exigé, avant toute main et avant l'épreuve de la base, `true` se rejoue par srt
/// comme une preuve ([`rejouer`]), dans un dossier neuf du dossier de Kraken, `bacs/mur-<lot>`, retiré ensuite avec son
/// dossier temporaire (journal `verif-<lot>-mur.tmp`, effacé). Si srt ne rend pas 0, ou ne se lance pas, c'est le refus
/// 6028, et rien n'est lancé. Sous un mur demandé, rien ne se joue ni ne se crée. Un dossier qui ne se crée pas est le
/// refus 6006.
pub(crate) fn eprouver_le_mur(profil: &Profil, dossier: &Path) -> Result<(), Refus> {
    if !profil.exige() {
        return Ok(());
    }
    let lot = lot();
    let worktree = dossier.join("bacs").join(format!("mur-{lot}"));
    fs::create_dir_all(&worktree).map_err(|e| nommee(e, format!("création du dossier {}", worktree.display())))?;
    let (_, rejeu) = rejouer(profil, "true", &worktree, &dossier.join(format!("verif-{lot}-mur.tmp")));
    let _ = fs::remove_dir_all(&worktree);
    if let Some(temporaire) = temporaire_de(&worktree) {
        let _ = fs::remove_dir_all(temporaire);
    }
    rejeu.map_err(|erreur| {
        Refus::new(Code::MurRefuse, format!("le mur est exigé, et srt ne joue pas `true` : {erreur}"), MUR_REFUSE)
    })
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
mod tests;
