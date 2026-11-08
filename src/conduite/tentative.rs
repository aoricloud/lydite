use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::branchements::claude::{jouer, temporaire};
use crate::branchements::srt::rejouer;
use crate::coeur::erreur::queue;
use crate::coeur::profil::{Demande, Profil};
use crate::coeur::protocole::{Attendu, Constat};
use crate::conduite::socle::socle;

const PREUVE: &str = "\n\nQuand tu as fini, joue toi-même le test qui prouve ton travail. La dernière ligne \
de ta réponse est exactement PREUVE: vert si le test passe, sinon PREUVE: rouge.";

/// Le plafond, en caractères, de la couche fixe d'une main de tâche ([`fixe`]) : sa taille d'aujourd'hui arrondie au
/// 500 supérieur.
#[cfg(test)]
pub(crate) const PLAFOND: usize = 1000;

/// La couche fixe d'une main de tâche : la sortie honnête, le périmètre et sa ligne de fichiers, puis la preuve que la
/// main se donne sans commande.
#[cfg(test)]
pub(crate) fn fixe() -> String {
    format!("{HONNETETE}\n\n{PERIMETRE} Your files: {PREUVE}")
}

/// Les lignes fixes que la consigne d'une main de tâche porte après l'objectif : la sortie honnête, quand aucun code
/// honnête ne passe le test, et le périmètre des fichiers.
const HONNETETE: &str = "If the request and a test contradict each other, so that no honest code can pass the \
test, do not bend the code to the test (no forced case, no branch for a test, no constant tuned to its seeds, no \
weakened test): write `IMPOSSIBLE: <reason>` as the last line of your answer and stop. A reasoned refusal is a valid \
result.";

const PERIMETRE: &str = "Change only the files of your task; the other tasks do the rest.";

/// Le début de l'erreur d'une main qui a déclaré la tâche impossible : Kraken l'écrit lui-même en tête, et la raison
/// de la main suit. La vague le reconnaît à ce début ([`impossible_declaree`]) et ne relance pas.
const IMPOSSIBLE: &str = "impossible (déclarée) : ";

/// La raison d'une main qui a déclaré sa tâche impossible, si l'erreur `erreur` est celle que `tenter` écrit alors.
pub(crate) fn impossible_declaree(erreur: &str) -> Option<&str> {
    erreur.strip_prefix(IMPOSSIBLE)
}

/// La raison de la dernière ligne non vide du texte d'une main si elle commence par `IMPOSSIBLE`, suivi de blancs
/// éventuels puis de `:`. Une ligne `IMPOSSIBLE` ailleurs ne compte pas.
fn declare_impossible(texte: &str) -> Option<&str> {
    let derniere = texte.lines().rev().find(|ligne| !ligne.trim().is_empty())?;
    let reste = derniere.trim().strip_prefix("IMPOSSIBLE")?;
    Some(reste.trim_start().strip_prefix(':')?.trim())
}

/// Un résultat pris ; `prouve` est faux quand aucune commande de vérification ne l'a rejoué.
#[derive(Debug)]
pub(crate) struct Pris {
    pub(crate) sortie: String,
    pub(crate) prouve: bool,
}

/// Le juge d'une main dont la preuve prêtée est une commande de Kraken : le découpeur (`decoupage.rs`), puis le
/// relecteur. Kraken ne relance pas cette commande par une coquille : il lit lui-même ce que la main a laissé dans son
/// bac, et rend `Ok`, ou l'erreur qui rougit la tentative et que la relance reçoit. La ligne `PREUVE:` de la main ne
/// compte pas.
pub(crate) type Juge<'a> = &'a (dyn Fn(&Path) -> Result<(), String> + Sync);

/// Ce que la main reçoit : l'objectif, l'erreur de la tentative précédente s'il y en a une, puis la preuve qu'on attend
/// d'elle. `pretee` : la commande de vérification est prêtée à la main (voir `Profil::prete`), qui la joue elle-même
/// jusqu'à ce qu'elle passe, avant que Kraken la rejoue. La main en lance chaque morceau seul et tel quel, car sous
/// Windows, sans bac à sable, Claude Code ne laisse passer un morceau prêté que seul et tel quel (`excludedCommands`),
/// et refuse une commande qui l'enrobe.
fn consigne(
    objectif: &str,
    fichiers: Option<&[String]>,
    verifier: Option<&str>,
    precedente: Option<&str>,
    pretee: bool,
    socle: &str,
) -> String {
    let perimetre = match fichiers {
        Some(fichiers) => format!("{PERIMETRE} Your files: {}", fichiers.join(" ")),
        None => PERIMETRE.to_string(),
    };
    // Avec une commande, c'est Kraken qui la rejoue ; la main la joue d'abord si on la lui prête, car en `acceptEdits`
    // elle ne peut souvent pas la lancer sans ce prêt.
    let preuve = match (verifier, pretee) {
        (Some(commande), true) => format!(
            "\n\nQuand tu auras fini, joue toi-même dans ton dossier : {commande}. Lance chaque morceau seul et tel \
             qu'il est écrit, dans l'ordre (les morceaux sont ceux que joint ` && ` ; une commande sans ` && ` en a \
             un seul), sans rien y ajouter avant ni après, ni `;`, ni `|`, ni redirection : c'est seul ainsi qu'un \
             morceau t'est permis. Reprends ton travail jusqu'à ce qu'elle \
             réussisse : Lydite la rejouera ensuite, et ton travail est pris si elle réussit."
        ),
        (Some(commande), false) => format!(
            "\n\nQuand tu auras fini, Lydite jouera dans ton dossier : {commande}. Ton travail est pris si elle \
             réussit."
        ),
        (None, _) => PREUVE.into(),
    };
    format!("{socle}\n\n{}\n\n{HONNETETE}\n\n{perimetre}{preuve}", avec_precedente(objectif, precedente))
}

/// L'objectif, puis l'erreur de la tentative d'avant s'il y en a une. C'est toute la consigne d'une main jugée par
/// Kraken ([`Juge`]), dont l'objectif dit déjà la preuve.
fn avec_precedente(objectif: &str, precedente: Option<&str>) -> String {
    match precedente {
        None => objectif.to_string(),
        Some(erreur) => format!("{objectif}\n\nTentative précédente échouée :\n{erreur}"),
    }
}

/// Une tentative : joue la main une fois dans le bac, puis sa preuve, la vérification que Kraken rejoue ou, sans
/// commande, la ligne `PREUVE: vert` de la main. La ligne de commande de la main et ce qu'elle déclare viennent du
/// profil (`Profil::demande`, pure : la vague en tire déjà le `main.lancement`) ; la main joue sous `delai_main`. Rend
/// le constat de la main (`None` pour une main brute, qui ne déclare rien), le `verif` de la tentative, la fin de la
/// sortie de la vérification (`None` sans commande, après une main rouge, ou si elle n'a pas pu partir), et le
/// verdict. `precedente` est l'erreur de la tentative d'avant, et ouvre la relance ; la sortie de la vérification passe
/// par `journal`.
///
/// Une main de tâche (sans `juge`) dont la dernière ligne non vide est `IMPOSSIBLE: <raison>` est déclarée impossible :
/// sa preuve n'est pas rejouée, et son verdict est l'erreur que [`impossible_declaree`] reconnaît. `fichiers` est le
/// périmètre de sa tâche, que sa consigne nomme.
///
/// Avec `juge`, la main est le découpeur ou le relecteur, dont la preuve prêtée, `verifier`, est une commande de
/// Kraken : sa consigne est son objectif, qui dit déjà sa preuve, puis l'erreur d'avant ; le dossier de l'exécutable
/// de Kraken passe en tête de son `PATH`, pour que la commande se trouve ; et son verdict est celui du juge, une prise
/// constatée, sans coquille ni `verif`. Sans juge, la main a le `PATH` de Kraken tel quel.
///
/// Si le profil nomme un fournisseur ([`Profil::fournisseur`]), la main, le découpeur et le relecteur compris, reçoit
/// l'URL de son API et sa clé, et perd la variable que `cle` nomme (voir `partir`) ; la preuve que Kraken rejoue ne
/// reçoit rien du fournisseur ([`rejouer`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tenter(
    profil: &Profil,
    bac: &Path,
    objectif: &str,
    fichiers: Option<&[String]>,
    verifier: Option<&str>,
    precedente: Option<&str>,
    journal: &Path,
    juge: Option<Juge<'_>>,
) -> (Option<Constat>, Option<String>, Result<Pris, String>) {
    let mut verif = None;
    // Le socle porte l'heure où la main s'arrête : son départ, pris ici, et son délai.
    let depart = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs();
    let socle = socle(depart, profil.delai_main());
    let consigne = match juge {
        Some(_) => format!("{socle}\n\n{}", avec_precedente(objectif, precedente)),
        None => consigne(objectif, fichiers, verifier, precedente, profil.prete(verifier), &socle),
    };
    let Demande { argv, declaration } = profil.demande(bac, verifier, precedente.is_some());
    let attendu =
        declaration.as_ref().map(|d| Attendu { reglages: &d.reglages, modele: &d.modele, mcp: d.mcp.as_deref() });
    // raccourci : ce que la main écrit entre au fil tel quel, et une main qui y recopierait sa clé l'y écrirait ;
    // monter (masquer la valeur de la clé dans ce qu'elle rend) si une main d'un fournisseur la répète
    let fournisseur = profil.fournisseur().zip(profil.variable_de_la_cle()).map(|((api, cle), nom)| (api, cle, nom));
    // Sous un mur exigé seul, le dossier temporaire est recréé vide avant la main ([`temporaire`]).
    let (constat, jeu) = match profil.exige().then(|| temporaire(bac, true)).transpose() {
        Err(erreur) => (attendu.as_ref().map(|_| Constat::default()), Err(erreur)),
        Ok(_) => jouer(&argv, bac, &consigne, profil.delai_main(), attendu.as_ref(), juge.is_some(), fournisseur),
    };
    let verdict = jeu.and_then(|sortie| match (juge, verifier) {
        // Une main de tâche qui déclare l'impossible : rien n'est rejoué, et l'erreur le dit pour que la vague classe.
        (None, _) if declare_impossible(&sortie).is_some() => {
            Err(format!("{IMPOSSIBLE}{}", queue(declare_impossible(&sortie).unwrap_or_default())))
        }
        (Some(juge), _) => juge(bac).map(|()| Pris { sortie, prouve: true }),
        (None, Some(commande)) => {
            let (fin, rejeu) = rejouer(profil, commande, bac, journal);
            verif = fin;
            rejeu.map(|()| Pris { sortie, prouve: true })
        }
        (None, None) => annonce_vert(sortie).map(|sortie| Pris { sortie, prouve: false }),
    });
    (constat, verif, verdict)
}

fn annonce_vert(texte: String) -> Result<String, String> {
    let derniere = texte.lines().rev().find(|ligne| !ligne.trim().is_empty());
    if derniere.map(str::trim) == Some("PREUVE: vert") {
        Ok(texte)
    } else {
        Err(queue(&format!("pas de « PREUVE: vert » en dernière ligne :\n{texte}")))
    }
}

#[cfg(test)]
mod tests;
