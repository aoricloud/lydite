use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::branchements::claude::{ErreurMain, jouer, temporaire};
use crate::branchements::srt::rejouer;
use crate::coeur::erreur::queue;
use crate::coeur::plan::Rouge;
use crate::coeur::profil::{Demande, Profil};
use crate::coeur::protocole::{Attendu, Constat};
use crate::conduite::socle::socle;

const PREUVE: &str = "\n\nQuand tu as fini, joue toi-même le test qui prouve ton travail. La dernière ligne \
de ta réponse est exactement PREUVE: vert si le test passe, sinon PREUVE: rouge.";

/// Le plafond, en caractères, de la couche fixe d'une main de tâche, mesurée sur [`consigne`] aux pièces vides, pour
/// chaque variante de la preuve et du rouge d'abord (sans `red`, fichiers nommés, `declared`) : la plus grande,
/// arrondie au 500 supérieur.
#[cfg(test)]
pub(crate) const PLAFOND: usize = 1500;

/// Les lignes fixes que la consigne d'une main de tâche porte après l'objectif : la sortie honnête, quand aucun code
/// honnête ne passe le test, et le périmètre des fichiers.
const HONNETETE: &str = "If the request and a test contradict each other, so that no honest code can pass the \
test, do not bend the code to the test (no forced case, no branch for a test, no constant tuned to its seeds, no \
weakened test): write `IMPOSSIBLE: <reason>` as the last line of your answer and stop. A reasoned refusal is a valid \
result.";

const PERIMETRE: &str = "Change only the files of your task; the other tasks do the rest.";

/// La phrase fixe qui suit la ligne des fichiers : le gel est de Lydite, et le réglage de la main refuse `git commit`.
const COMMIT: &str = "Lydite commits your work itself: do not run git commit.";

/// Le début du texte de l'issue d'une tâche classée impossible : Kraken l'écrit lui-même en tête, et la raison de la
/// main suit. Il ne sert qu'à dire l'issue ; aucune décision ne le lit.
pub(crate) const IMPOSSIBLE: &str = "impossible (declared): ";

/// L'échec d'une tentative, du point où elle le rend jusqu'à celui où la vague décide.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Echec {
    /// La main a déclaré sa tâche impossible (`IMPOSSIBLE: <raison>`) ; la raison est la sienne. Classée sans relance.
    Impossible(String),
    /// Une coupure de l'API (`protocole::coupure`) : son statut HTTP, puis l'erreur de la main en texte. Elle repart
    /// une fois, sans compter comme une relance.
    Coupure(u16, String),
    /// Kraken a tué la main à son délai (`ErreurMain::Tuee`) ; le texte de son erreur. Refusée sans relance à la
    /// première tentative.
    Tuee(String),
    /// La main a atteint son plafond de dépense (`protocole::plafond_atteint`) ; le texte de son erreur. Refusée sans
    /// relance, à toute tentative : la suite revient à l'humain.
    Plafond(String),
    /// Toute autre erreur, dite en texte. Suit la relance.
    Erreur(String),
}

/// La raison de la dernière ligne non vide du texte d'une main si elle commence par `IMPOSSIBLE`, suivi de blancs
/// éventuels puis de `:`. Une ligne `IMPOSSIBLE` ailleurs ne compte pas.
fn declare_impossible(texte: &str) -> Option<&str> {
    let derniere = texte.lines().rev().find(|ligne| !ligne.trim().is_empty())?;
    let reste = derniere.trim().strip_prefix("IMPOSSIBLE")?;
    Some(reste.trim_start().strip_prefix(':')?.trim())
}

/// Les fichiers de test que la main nomme dans sa réponse, dans l'ordre, chacun une fois : une ligne par fichier,
/// dont le début, blancs ôtés, est `RED:` ; le chemin est le reste de la ligne, blancs ôtés. Une ligne `RED:` sans
/// chemin nomme le chemin vide, que l'appelant refuse comme tout chemin hors de la forme de `fichiers`. Une ligne
/// qui n'est pas ainsi, ou qui a `RED:` ailleurs que dans son début, ne compte pas.
pub(crate) fn declare_rouge(texte: &str) -> Vec<&str> {
    let mut fichiers: Vec<&str> = Vec::new();
    for fichier in texte.lines().filter_map(|ligne| ligne.trim().strip_prefix("RED:")).map(str::trim) {
        if !fichiers.contains(&fichier) {
            fichiers.push(fichier);
        }
    }
    fichiers
}

/// Les lignes que la consigne d'une tâche à `red` ajoute après les épingles : ce que Lydite rejoue, sur la base plus
/// les seuls fichiers du test, et où écrire le test pour qu'il puisse rougir.
fn lignes_du_rouge(rouge: &Rouge) -> Vec<String> {
    let ou_ecrire = "Write the test in its own file, apart from the fix, in a file that the base already compiles or \
                     collects (in Rust: a module that the base declares, or a file under `tests/`): a test in the \
                     file of the fix, or in a file that the base does not run, cannot fail on the base.";
    let mut lignes = Vec::new();
    match rouge {
        Rouge::Nommes(fichiers) => {
            lignes.push(
                "Red first: Lydite replays the proof on the base plus only these files, and it must fail there; \
                 then it must pass with your whole change:"
                    .to_string(),
            );
            lignes.extend(fichiers.iter().cloned());
        }
        Rouge::Declare => {
            lignes.push(
                "Red first: Lydite replays the proof on the base plus only the test files you name, and it must \
                 fail there; then it must pass with your whole change."
                    .to_string(),
            );
            lignes.push("Name each test file at the end of your answer, one line `RED: <file>` each.".to_string());
        }
    }
    lignes.push(ou_ecrire.to_string());
    lignes
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
/// d'elle. Les `epingles` (fichier et empreinte) viennent après les textes exacts, sous une ligne qui dit à la main que
/// ces fichiers doivent avoir cette empreinte, telle que git les garde ; sans épingle, rien ne s'ajoute. `pretee` :
/// la commande de vérification est prêtée à la main (voir `Profil::prete`), qui la joue elle-même jusqu'à ce qu'elle
/// passe, avant que Kraken la rejoue. La main en lance chaque morceau seul et tel quel, car sous
/// Windows, sans bac à sable, Claude Code ne laisse passer un morceau prêté que seul et tel quel (`excludedCommands`),
/// et refuse une commande qui l'enrobe.
///
/// `rouge` (la clé `red` de la tâche) ajoute, après les épingles, les lignes du rouge d'abord ([`lignes_du_rouge`]).
///
/// `sans_a1` (le réglage `sans-a1` de la clé `mesure`, d'un banc de mesure) retire `HONNETETE` et `PERIMETRE`, et rien
/// d'autre : la liste des fichiers, les textes exacts, le rouge, `COMMIT` et la preuve restent.
#[allow(clippy::too_many_arguments)]
fn consigne(
    objectif: &str,
    fichiers: Option<&[String]>,
    exacts: &[(String, String)],
    epingles: &[(String, String)],
    rouge: Option<&Rouge>,
    verifier: Option<&str>,
    precedente: Option<&str>,
    pretee: bool,
    socle: &str,
    sans_a1: bool,
) -> String {
    let debut = match (sans_a1, fichiers) {
        (false, Some(fichiers)) => format!("{PERIMETRE} Your files: {}", fichiers.join(" ")),
        (false, None) => PERIMETRE.to_string(),
        (true, Some(fichiers)) => format!("Your files: {}", fichiers.join(" ")),
        (true, None) => String::new(),
    };
    let mut lignes = Vec::new();
    if !debut.is_empty() {
        lignes.push(debut);
    }
    if !exacts.is_empty() {
        lignes.push("These texts must appear in your files byte for byte, as written here:".to_string());
        lignes.extend(exacts.iter().map(|(fichier, texte)| format!("{fichier}: {texte}")));
    }
    if !epingles.is_empty() {
        lignes.push("These files must have exactly this SHA-256, as git stores them:".to_string());
        lignes.extend(epingles.iter().map(|(fichier, empreinte)| format!("{fichier}: {empreinte}")));
    }
    lignes.extend(rouge.into_iter().flat_map(lignes_du_rouge));
    lignes.push(COMMIT.to_string());
    let perimetre = lignes.join("\n");
    let honnetete = if sans_a1 { String::new() } else { format!("{HONNETETE}\n\n") };
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
    format!("{socle}\n\n{}\n\n{honnetete}{perimetre}{preuve}", avec_precedente(objectif, precedente))
}

/// L'objectif, puis l'erreur de la tentative d'avant s'il y en a une. C'est toute la consigne d'une main jugée par
/// Kraken ([`Juge`]), dont l'objectif dit déjà la preuve.
fn avec_precedente(objectif: &str, precedente: Option<&str>) -> String {
    match precedente {
        None => objectif.to_string(),
        Some(erreur) => format!("{objectif}\n\nTentative précédente échouée :\n{erreur}"),
    }
}

/// Le texte exact que reçoit la main d'une tentative, calculé une seule fois : la vague en garde les octets, en note
/// l'empreinte (`main.consigne`) et les envoie à [`tenter`]. Le socle porte l'heure où la main s'arrête : son départ,
/// pris ici, et son délai. Avec `juge`, la main est le découpeur ou le relecteur : son objectif, puis l'erreur
/// d'avant ; sinon c'est une main de tâche, avec ses fichiers, ses textes exacts, ses épingles `sha256` (un couple
/// fichier et empreinte chacune : celles de sa tâche, puis celles des autres tâches que ses `fichiers` couvrent), son
/// `rouge` (les lignes du rouge d'abord, après les épingles ; sans `red`, la consigne ne change pas d'un octet) et sa
/// preuve.
#[allow(clippy::too_many_arguments)]
pub(crate) fn consigne_de(
    profil: &Profil,
    objectif: &str,
    fichiers: Option<&[String]>,
    exacts: &[(String, String)],
    epingles: &[(String, String)],
    rouge: Option<&Rouge>,
    verifier: Option<&str>,
    precedente: Option<&str>,
    juge: bool,
) -> String {
    let depart = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or(Duration::ZERO).as_secs();
    let socle = socle(depart, profil.delai_main());
    if juge {
        format!("{socle}\n\n{}", avec_precedente(objectif, precedente))
    } else {
        let sans_a1 = profil.mesure().sans_a1;
        let pretee = profil.prete(verifier);
        consigne(objectif, fichiers, exacts, epingles, rouge, verifier, precedente, pretee, &socle, sans_a1)
    }
}

/// Une tentative : joue la main une fois dans le bac, puis sa preuve, la vérification que Kraken rejoue ou, sans
/// commande, la ligne `PREUVE: vert` de la main. `consigne` est le texte exact que la main reçoit ([`consigne_de`]).
/// La ligne de commande de la main et ce qu'elle déclare viennent du
/// profil (`Profil::demande`, pure : la vague en tire déjà le `main.lancement`) ; la main joue sous `delai_main`. Rend
/// le constat de la main (`None` pour une main brute, qui ne déclare rien), le `verif` de la tentative, la fin de la
/// sortie de la vérification (`None` sans commande, après une main rouge, ou si elle n'a pas pu partir), et le
/// verdict. `precedente` est l'erreur de la tentative d'avant, et ouvre la relance ; la sortie de la vérification passe
/// par `journal`.
///
/// Une main de tâche (sans `juge`) dont la dernière ligne non vide est `IMPOSSIBLE: <raison>` est déclarée impossible :
/// sa preuve n'est pas rejouée, et son verdict est [`Echec::Impossible`]. `fichiers` est le
/// périmètre de sa tâche, que sa consigne nomme.
///
/// Avec `juge`, la main est le découpeur ou le relecteur, dont la preuve prêtée, `verifier`, est une commande de
/// Kraken : sa consigne est son objectif, qui dit déjà sa preuve, puis l'erreur d'avant ; le dossier de l'exécutable
/// de Kraken passe en tête de son `PATH`, pour que la commande se trouve ; et son verdict est celui du juge, une prise
/// constatée, sans coquille ni `verif`. Sans juge, la main a le `PATH` de Kraken tel quel.
///
/// `lecons` est le fichier des leçons que la main reçoit en prompt système (`--append-system-prompt-file`), s'il y en
/// a ; une main brute n'en reçoit pas.
///
/// Si le profil nomme un fournisseur ([`Profil::fournisseur`]), la main, le découpeur et le relecteur compris, reçoit
/// l'URL de son API et sa clé, et perd la variable que `cle` nomme (voir `partir`) ; la preuve que Kraken rejoue ne
/// reçoit rien du fournisseur ([`rejouer`]).
#[allow(clippy::too_many_arguments)]
pub(crate) fn tenter(
    profil: &Profil,
    bac: &Path,
    consigne: &str,
    verifier: Option<&str>,
    precedente: Option<&str>,
    journal: &Path,
    juge: Option<Juge<'_>>,
    lecons: Option<&Path>,
) -> (Option<Constat>, Option<String>, Result<Pris, Echec>) {
    let mut verif = None;
    let Demande { argv, declaration } = profil.demande_avec(bac, verifier, precedente.is_some(), lecons);
    let attendu = declaration.as_ref().map(|d| Attendu {
        reglages: &d.reglages,
        modele: &d.modele,
        effort: d.effort.as_deref(),
        mcp: d.mcp.as_deref(),
    });
    // raccourci : ce que la main écrit entre au fil tel quel, et une main qui y recopierait sa clé l'y écrirait ;
    // monter (masquer la valeur de la clé dans ce qu'elle rend) si une main d'un fournisseur la répète
    let fournisseur = profil.fournisseur();
    // Sous un mur exigé seul, le dossier temporaire est recréé vide avant la main ([`temporaire`]).
    let (constat, jeu) = match profil.exige().then(|| temporaire(bac, true)).transpose() {
        Err(erreur) => (attendu.as_ref().map(|_| Constat::default()), Err(ErreurMain::Autre(erreur))),
        Ok(_) => jouer(&argv, bac, consigne, profil.delai_main(), attendu.as_ref(), juge.is_some(), fournisseur),
    };
    let coupure = constat.as_ref().and_then(|constat| constat.coupure);
    let plafond = constat.as_ref().is_some_and(|constat| constat.plafond);
    let verdict = jeu.map_err(|erreur| echec_de(erreur, coupure, plafond)).and_then(|sortie| {
        // Une main de tâche qui déclare l'impossible : rien n'est rejoué, et l'échec le dit pour que la vague classe.
        let declaree = if juge.is_none() { declare_impossible(&sortie) } else { None };
        if let Some(raison) = declaree {
            return Err(Echec::Impossible(queue(raison)));
        }
        match (juge, verifier) {
            (Some(juge), _) => juge(bac).map(|()| Pris { sortie, prouve: true }).map_err(Echec::Erreur),
            (None, Some(commande)) => {
                let (fin, rejeu) = rejouer(profil, commande, bac, journal);
                verif = fin;
                rejeu.map(|()| Pris { sortie, prouve: true }).map_err(Echec::Erreur)
            }
            (None, None) => annonce_vert(sortie).map(|sortie| Pris { sortie, prouve: false }).map_err(Echec::Erreur),
        }
    });
    (constat, verif, verdict)
}

/// L'échec d'une main qui a rendu `erreur` : une coupure de l'API si son constat en a lu une (`statut`), sinon une
/// erreur ordinaire. Une main qui a atteint son plafond de dépense (`plafond`) est un `Echec::Plafond`, avant toute
/// autre lecture. Une main tuée à son délai (`ErreurMain::Tuee`) est un `Echec::Tuee` si aucune coupure n'a été lue.
fn echec_de(erreur: ErreurMain, statut: Option<u16>, plafond: bool) -> Echec {
    match (plafond, statut, erreur) {
        (true, _, erreur) => Echec::Plafond(erreur.texte()),
        (false, Some(statut), erreur) => Echec::Coupure(statut, erreur.texte()),
        (false, None, ErreurMain::Tuee(texte)) => Echec::Tuee(texte),
        (false, None, ErreurMain::Autre(texte)) => Echec::Erreur(texte),
    }
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
