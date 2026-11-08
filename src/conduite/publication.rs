//! La forge : la PR d'une vague à `--pr`, avant comme après la vague. Chaque geste est une fonction, testable seule,
//! dans l'ordre de publication, et aucun n'écrit au fil. [`avant_la_vague`] et [`Proposition::apres_la_vague`]
//! les enchaînent, et la seconde note chacun dans une séance à elle : `forge.poussee` avant le push, `forge.pr`,
//! `forge.ci` et `forge.refus` après, `forge.rangement` avant de retirer.
//!
//! - Avant la vague, rien ne part tant que tout n'est pas lu : le distant `origin` ([`distant`], 8001), la base qu'il
//!   porte ([`Forge::base_distante`], 8002), `gh` connecté à son hôte ([`Forge::connecte`], 8003).
//! - Après la vague : quelque chose à pousser ([`a_pousser`], 8004), rien qui revienne à l'humain
//!   ([`Forge::publiable`], 8005), le push sans force ([`Forge::pousser`], 8006), la PR ouverte et relue
//!   ([`Forge::ouvrir`], 8007, son corps par [`corps`]), sa CI ([`Forge::lire_ci`] : 8008, 8009 en conflit, 8010) et le
//!   journal d'une CI rouge ([`Forge::journal_rouge`]).
//! - Une CI rouge vaut une seule relance ([`Proposition::relancer`], 8011), aucune si une tâche de la vague est
//!   critique (8011 d'emblée) : une base qui n'a pas bougé
//!   ([`Forge::base_a_bouge`], 8009), une vague neuve d'une tâche `ci` partie de la tête poussée ([`plan_de_relance`]),
//!   la même règle de publication sur toute la PR, le push en avance rapide dans la branche de la PR, la CI relue ;
//!   tout cela se note sous la relance, ou sous la vague relancée si la relance ne s'ouvre pas.
//! - Enfin le rangement de ce qui est poussé ([`Proposition::ranger`]), relance comprise ; les deux gestes de git d'un
//!   retrait ([`retirer`]) passent par la forge comme les autres.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::time::Duration;

use crate::branchements::bac::{retirer, temporaire_de};
use crate::branchements::dossier::{self, Fil};
use crate::branchements::git::faire_borne;
use crate::branchements::github::{Ci, Forge, GIT_FORGE, PrOuverte, a_pousser, distant, en_echec};
use crate::coeur::erreur::{Code, Refus, queue};
use crate::coeur::fil::{
    self, Base, Couleur, Evenement, Foi, Note, SceauVague, Vague, correction_de, etat, relecture_de,
};
use crate::coeur::plan::{Plan, TAILLE_MAX, Tache};
use crate::conduite::alarmes;
use crate::conduite::lancer::Menee;

/// La correction du refus 8011.
const ROUGE: &str =
    "La PR reste ouverte et rouge, et Lydite ne la relance pas deux fois : lis sa CI, puis corrige-la toi-même.";

/// La première ligne de l'objectif de la tâche de relance ; le contexte la suit.
const CORRIGER: &str = "Corrige au plus petit ce que le contexte nomme, et rien d'autre, sans toucher à .github/";

/// Le plan de la relance d'une CI rouge : une vague qui rend verte la CI de la PR `numero`, d'une seule tâche `ci` sans
/// dépendance. Elle joue la preuve de la vague relancée, et son objectif est [`CORRIGER`], puis le `contexte` : les
/// checks en échec et la fin du journal de la CI. Ses `fichiers` sont l'union de ceux des tâches de la vague relancée,
/// dans leur ordre et sans doublon, quand chacune en a ; aucun sinon : une tâche que rien ne bornait a pu tout changer.
fn plan_de_relance(plan: &Plan, numero: u64, contexte: &str) -> Plan {
    let contexte = contexte.trim_end();
    let objectif = if contexte.is_empty() { CORRIGER.to_string() } else { format!("{CORRIGER}\n{contexte}") };
    let fichiers = plan.taches.iter().try_fold(Vec::new(), |mut unis: Vec<String>, tache| {
        for chemin in tache.fichiers.as_ref()? {
            if !unis.contains(chemin) {
                unis.push(chemin.clone());
            }
        }
        Some(unis)
    });
    let tache = Tache { id: "ci".into(), depend: Vec::new(), preuve: None, objectif, fichiers, ..Tache::default() };
    Plan { objectif: format!("Rendre verte la CI de la PR {numero}"), preuve: plan.preuve.clone(), taches: vec![tache] }
}

/// Le corps de la PR, public : d'abord le bloc « Reçu », écrit à l'ouverture de la PR : le coût jusque-là, les mains,
/// relances et classées (`recu`, voir `Recu`), et les fichiers de réglage de la preuve que le diff de la base à la tête
/// change (`reglages`, voir [`reglages_changes`] : chaque chemin écrit en code, ou `chemin au nom illisible` s'il porte
/// un caractère de contrôle ou une apostrophe inverse ; vingt au plus, puis « … et <k> autres »), ou « aucun » ; puis
/// l'objectif du plan ; `Preuve : <preuve de la vague> → vert` ; les tâches assemblées,
/// `<id> : <première ligne de son objectif>` ; les tâches dehors, `<id> : classée` ou `<id> : refusée (<code>) :
/// <règle>` ; puis la ligne de la relecture, `relecture`, si la vague a été relue, et sous elle la ligne du commit
/// relu et rejoué ([`controler_la_tete`]), que `relecture` porte aussi, dont `rejouee` fait un texte de deux lignes.
/// La fusion suit la règle du dépôt, et
/// le corps n'en dit rien. L'issue de la tâche `k` du plan est celle que le fil rejoue pour la main `k` de `vague`
/// (`fil::etat`) : le numéro d'un refus vient de sa note `vague.refus`, et sa règle de la table des codes, rien qu'un
/// numéro pour un code inconnu. Jamais la sortie d'une main, qui peut commencer par « refusée (», ni la preuve d'un
/// refus, ni la règle que porte une note : un autre écrivain du fil peut l'avoir écrite.
///
/// Dans une vague découpée (le fil note qu'elle joue le plan d'un découpage), les objectifs des tâches sont écrits
/// par une main, et le corps, qui est la voix de Kraken, ne les reprend pas : une tâche assemblée s'y nomme par son
/// id seul.
///
/// Une PR d'issue (`issue`) porte en plus, en dernier, `Closes #<n>` : sa fusion ferme l'issue. Sans issue, rien ne
/// change. Hors les chemins des réglages, écrits en code (voir ci-dessus), le corps ne reprend jamais un texte écrit
/// par une main.
pub(crate) fn corps(
    plan: &Plan,
    vague: &Vague,
    relecture: Option<&str>,
    issue: Option<u64>,
    recu: &Recu,
    reglages: &[String],
    points: &[String],
) -> String {
    let decoupee = vague.decoupage.is_some();
    let (mut assemblees, mut dehors) = (Vec::new(), Vec::new());
    for (k, tache) in (1..).zip(&plan.taches) {
        let id = &tache.id;
        match vague.mains.get(&k).and_then(|main| main.issue.as_ref()) {
            Some(fil::Issue::Assemblee) if decoupee => assemblees.push(id.clone()),
            Some(fil::Issue::Assemblee) => {
                let premiere = tache.objectif.lines().map(str::trim).find(|ligne| !ligne.is_empty());
                assemblees.push(format!("{id} : {}", premiere.unwrap_or_default()));
            }
            Some(fil::Issue::Refusee { code, .. }) => dehors.push(match Code::de_numero(*code) {
                Some(code) => format!("{id} : refusée ({}) : {}", code.numero(), code.regle()),
                None => format!("{id} : refusée ({code})"),
            }),
            None => dehors.push(format!("{id} : classée")),
        }
    }
    let preuve = format!("Preuve : {} → vert", plan.preuve.as_deref().unwrap_or_default());
    let relue = relecture.unwrap_or_default().to_string();
    let changes = if reglages.is_empty() {
        "aucun".to_string()
    } else {
        let mut noms: Vec<String> = reglages.iter().take(REGLAGES_NOMMES).map(|chemin| en_code(chemin)).collect();
        if reglages.len() > REGLAGES_NOMMES {
            noms.push(format!("… et {} autres", reglages.len() - REGLAGES_NOMMES));
        }
        noms.join(", ")
    };
    let lignes = recu.lignes_de_pr().join("\n");
    let recu = format!("Reçu\n{lignes}\n- Réglages de la preuve changés : {changes}");
    let a_justifier = if points.is_empty() {
        String::new()
    } else {
        let mut lignes = vec!["Points to justify (found by Lydite without a model):".to_string()];
        lignes.extend(points.iter().take(POINTS_NOMMES).map(|point| format!("- {point}")));
        if points.len() > POINTS_NOMMES {
            lignes.push(format!("- … and {} more", points.len() - POINTS_NOMMES));
        }
        lignes.join("\n")
    };
    let paragraphes =
        [recu, plan.objectif.clone(), preuve, assemblees.join("\n"), dehors.join("\n"), a_justifier, relue];
    let mut corps: Vec<String> = paragraphes.into_iter().filter(|paragraphe| !paragraphe.is_empty()).collect();
    corps.extend(issue.map(|numero| format!("Closes #{numero}")));
    corps.join("\n\n") + "\n"
}

/// Le nombre de fichiers de réglage que le corps nomme : au-delà, il compte les autres, pour que le corps reste sous la
/// borne de la forge.
const REGLAGES_NOMMES: usize = 20;

/// Le nombre de points à justifier ([`alarmes::points`]) que le corps nomme : au-delà, il compte les autres.
const POINTS_NOMMES: usize = 20;

/// Un chemin que le diff nomme, écrit en code : une main l'a écrit, et ni une mention, ni un saut de ligne n'y a
/// d'effet. Un chemin qui porte un caractère de contrôle ou une apostrophe inverse y est remplacé par un nom neutre.
pub(crate) fn en_code(chemin: &str) -> String {
    if chemin.chars().any(|c| c.is_control() || c == '`') {
        "`chemin au nom illisible`".to_string()
    } else {
        format!("`{chemin}`")
    }
}

/// Les fichiers de réglage de la preuve (voir [`corps`]) parmi les `chemins` qu'un diff nomme, dans leur ordre : le nom
/// d'un fichier de réglage de Cargo, de rustfmt, de clippy ou de la chaîne d'outils, à toute profondeur, ou tout
/// chemin sous un dossier `.cargo`.
fn reglages_changes<'a>(chemins: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    const NOMS: [&str; 9] = [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "rustfmt.toml",
        ".rustfmt.toml",
        "clippy.toml",
        ".clippy.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
    ];
    let reglage = |chemin: &str| {
        let mut parties = chemin.split('/').rev();
        parties.next().is_some_and(|nom| NOMS.contains(&nom)) || parties.any(|dossier| dossier == ".cargo")
    };
    chemins.into_iter().filter(|chemin| reglage(chemin)).map(String::from).collect()
}

/// La ligne du corps de la PR qui dit la relecture de la vague `id`, d'après les vagues que le fil rejoue :
/// `Relecture : <n> remarques, dont <b> bloquantes, corrigées.` si le fil note une correction de la vague, sans
/// `, corrigées` sinon ; `None` si la vague n'a pas été relue. Rien que des nombres : le texte des remarques n'est
/// jamais publié.
fn ligne_de_relecture(vagues: &[Vague], id: &str) -> Option<String> {
    let relecture = relecture_de(vagues, id)?;
    let corrigees = if correction_de(vagues, id).is_some() { ", corrigées" } else { "" };
    Some(format!("Relecture : {} remarques, dont {} bloquantes{corrigees}.", relecture.n, relecture.bloquantes))
}

/// La correction du refus 7016 d'une tête qui n'est pas celle que la relecture a jugée.
const SANS_TETE_JUGEE: &str = "Rien ne se pousse : seule se pousse la tête que la relecture a jugée, ou la correction \
                               qui vise cette relecture. Relance la vague, ou relis toi-même la tête avant de la \
                               pousser.";

/// Vrai si un `vague.fusion` de l'une des vagues `ids` porte le commit `sha` : la preuve y a été rejouée verte.
fn rejouee_sur(evenements: &[Evenement], ids: &[&str], sha: &str) -> bool {
    evenements.iter().any(|e| match e {
        Evenement::Fusion { vague, sha: fusionne, .. } => ids.contains(&vague.as_str()) && fusionne == sha,
        _ => false,
    })
}

/// « replayed » si la preuve a été rejouée sur le commit, « not replayed » sinon.
fn rejoue(rejouee: bool) -> &'static str {
    if rejouee { "replayed" } else { "not replayed" }
}

/// Le dernier `vague.fusion` de la vague `id` : sa tête, `None` sans.
fn derniere_tete(evenements: &[Evenement], id: &str) -> Option<String> {
    evenements.iter().rev().find_map(|e| match e {
        Evenement::Fusion { vague, sha, .. } if vague == id => Some(sha.clone()),
        _ => None,
    })
}

/// Ce que le fil dit de la relecture de la vague `id` face à `tete`, la tête que la forge va pousser au premier envoi,
/// et, s'il la laisse partir, la ligne du corps de la PR qui nomme le commit relu et rejoué.
///
/// - Sans relecture (aucune tâche critique), la tête part comme elle est : « Replayed on commit <tête>, not reviewed:
///   no task is critical. »
/// - Relue sans remarque bloquante, seule part la tête que le verdict (`vague.verdict`) dit relue : « Reviewed and
///   replayed on commit <sha>. »
/// - Relue avec des bloquantes, seule part la tête de la correction qui vise cette relecture (le dernier
///   `vague.fusion` de sa vague), jamais la tête relue : « Reviewed on commit <relue>, corrected in <tête> without a
///   second review, and replayed on commit <tête>. »
/// - Sinon, le refus 7016, dont la preuve nomme la tête à pousser et celle qu'il fallait.
///
/// « replayed » ne s'écrit que si un `vague.fusion` de la vague ou de sa correction porte le commit.
fn controler_la_tete(vagues: &[Vague], evenements: &[Evenement], id: &str, tete: &str) -> Result<String, Refus> {
    let refus = |preuve: String| Refus::new(Code::RelectureManquee, preuve, SANS_TETE_JUGEE);
    let Some(relecture) = relecture_de(vagues, id) else {
        let rejoue = rejoue(rejouee_sur(evenements, &[id], tete));
        return Ok(format!("{} on commit {tete}, not reviewed: no task is critical.", capitaliser(rejoue)));
    };
    // Le verdict est celui du relecteur qui a rendu la dernière relecture de la vague.
    let relecteur = vagues.iter().rev().find(|v| v.relecture.as_ref().is_some_and(|r| r.relue == id));
    let jugement = vagues.iter().find(|vague| vague.id == id).and_then(|vague| vague.jugement.as_ref());
    let Some(jugement) = jugement.filter(|j| relecteur.is_some_and(|r| r.id == j.relecteur)) else {
        return Err(refus(format!(
            "la relecture de la vague {id} n'a pas de verdict au fil, et la tête à pousser est {tete}"
        )));
    };
    let jugee = jugement.sha.as_str();
    if relecture.bloquantes == 0 {
        if tete != jugee {
            return Err(refus(format!("la tête à pousser {tete} n'est pas la tête relue {jugee} de la vague {id}")));
        }
        return Ok(if rejouee_sur(evenements, &[id], jugee) {
            format!("Reviewed and replayed on commit {jugee}.")
        } else {
            format!("Reviewed and not replayed on commit {jugee}.")
        });
    }
    let Some(correction) = correction_de(vagues, id) else {
        return Err(refus(format!(
            "la relecture de la vague {id} a des remarques bloquantes sur la tête relue {jugee}, aucune correction ne \
             les vise, et la tête à pousser est {tete}"
        )));
    };
    let corrigee = derniere_tete(evenements, &correction.id).filter(|corrigee| corrigee != jugee);
    let Some(corrigee) = corrigee else {
        return Err(refus(format!(
            "la correction {} de la vague {id} n'a pas de tête autre que la tête relue {jugee}, et la tête à pousser \
             est {tete}",
            correction.id
        )));
    };
    if tete != corrigee {
        return Err(refus(format!(
            "la tête à pousser {tete} n'est pas la tête {corrigee} de la correction {} de la vague {id} (relue sur \
             {jugee})",
            correction.id
        )));
    }
    let rejoue = rejoue(rejouee_sur(evenements, &[id, &correction.id], tete));
    Ok(format!(
        "Reviewed on commit {jugee}, corrected in {tete} without a second review, and {rejoue} on commit {tete}."
    ))
}

/// Le mot avec sa première lettre en capitale.
fn capitaliser(mot: &str) -> String {
    let mut lettres = mot.chars();
    lettres.next().map_or_else(String::new, |premiere| premiere.to_uppercase().chain(lettres).collect())
}

/// La PR d'une vague à `--pr`, préparée avant la vague ([`avant_la_vague`]) : la forge du dépôt visé, la branche que
/// nomme `--pr` et sa tête distante, d'où part la vague.
pub(crate) struct Proposition<'a> {
    forge: Forge<'a>,
    /// La branche du distant que nomme `--pr`, base de la PR.
    branche: String,
    /// Sa tête distante, lue avant la vague : la base de la vague et de sa PR.
    pub(crate) base: String,
    /// L'issue que la PR ferme à sa fusion (`Closes #<n>` dans son corps), `None` hors de la source « issue ».
    issue: Option<u64>,
}

/// Avant la vague, à `--pr` : le distant `origin` du dépôt visé (8001), la tête distante de la base `branche` (8002),
/// puis `gh` connecté à l'hôte du distant (8003). Chaque faute est un refus, et rien n'est noté : rien ne part.
pub(crate) fn avant_la_vague<'a>(
    gh: &'a OsStr,
    dossier: &'a Path,
    depot: &'a Path,
    branche: &str,
) -> Result<Proposition<'a>, Refus> {
    let forge = Forge { gh, dossier, depot, distant: distant(dossier, depot)? };
    let base = forge.base_distante(branche)?;
    forge.connecte()?;
    Ok(Proposition { forge, branche: branche.into(), base, issue: None })
}

/// Ce qui arrête la forge avant la fin de sa publication.
enum Arret {
    /// Un refus de 8004 à 8011, qui s'écrit sur stderr et se note.
    Refus(Refus),
    /// Ce qui l'arrête net, sans rien noter ni ranger de plus : une vague arrêtée (7012), une note ratée, un fil qui ne
    /// se relit pas.
    Net(String),
}

/// Ce que la forge a fait après la vague : sa séance du fil, le dernier sha qu'elle a poussé, la couleur de la dernière
/// CI qu'elle a lue, et la vague de relance une fois ouverte.
struct Suite {
    fil: Fil,
    poussee: Option<String>,
    ci: Option<Couleur>,
    /// La vague de relance, ouverte par une CI rouge : dès lors, la forge note sous son id.
    relance: Option<Menee>,
}

/// Ce que la forge a publié de la vague, que sa relance reprend : la branche distante et la tête qu'elle y a poussée,
/// la PR qu'elle y a ouverte, le titre et le corps qu'elle a publiés, les chemins du poste que la règle de publication
/// cherche, et la lecture de la CI (`ATTENTE_DE_LA_CI`).
struct Publication<'a> {
    branche: &'a str,
    tete: &'a str,
    pr: PrOuverte,
    titre: String,
    corps: String,
    chemins: Vec<&'a Path>,
    ci: (Duration, Duration),
}

/// Ce que dit une CI rouge : ses checks en échec, puis la fin du journal de ses runs en échec, vide sans journal.
struct Rouge {
    echecs: String,
    journal: String,
}

impl Rouge {
    /// Le contexte d'une relance : les checks en échec, puis la fin du journal s'il y en a une.
    fn contexte(&self) -> String {
        if self.journal.is_empty() { self.echecs.clone() } else { format!("{}\n{}", self.echecs, self.journal) }
    }
}

/// Note l'événement dans la séance de la forge. Ratée, la note arrête la forge, et l'erreur nomme le fil.
fn noter(fil: &Fil, evenement: &Evenement) -> Result<(), String> {
    fil.noter(evenement).map_err(|e| format!("une note de la forge ne s'écrit pas : {e}"))
}

impl Proposition<'_> {
    /// La PR fermera l'issue `numero` à sa fusion : son corps porte `Closes #<numero>` ([`corps`]).
    pub(crate) fn fermer_l_issue(&mut self, numero: u64) {
        self.issue = Some(numero);
    }

    /// La ligne que stderr reçoit si le `HEAD` du dépôt visé n'est pas la base distante : la vague part de celle-ci, et
    /// les commits locaux non poussés n'entrent pas dans la PR. `None` s'il l'est.
    pub(crate) fn ecart_de_head(&self) -> Option<String> {
        let head = self.forge.faire(self.forge.git(&["rev-parse", "--verify", "HEAD"]), None).ok();
        let head = head.as_deref().map(str::trim);
        (head != Some(self.base.as_str())).then(|| {
            format!(
                "HEAD du dépôt visé ({}) n'est pas la tête distante de {} ({}) : la vague part de celle-ci, sans les \
                 commits locaux non poussés",
                head.unwrap_or("illisible"),
                self.branche,
                self.base
            )
        })
    }

    /// Après la vague `menee`, partie de la base : les étapes de l'en-tête du module, chaque geste noté dans une séance
    /// de la forge. Une vague arrêtée (7012), ou une note ratée, arrête la forge net : plus rien ne se pousse,
    /// ne s'ouvre ni ne se range. Un refus s'écrit sur stderr et se note (`forge.refus`), sous la relance si elle s'est
    /// ouverte, sinon sous la vague. Après le dernier push réussi, quelle que soit la CI, la forge range, la relance
    /// comprise ([`Proposition::ranger`]). `personnel` est le dossier personnel, que 8005 cherche avec le dossier de
    /// Kraken et le dépôt visé ; `ci`, l'intervalle et le délai de la lecture de la CI. Rend le code de sortie : 0 si
    /// toutes les tâches du plan sont assemblées et la dernière CI lue est verte, 1 sinon.
    ///
    /// `corrigee` est la vague de la correction des remarques bloquantes de la relecture (`relecture.rs`), assemblée
    /// : la tête de `menee` est alors la sienne, `t2`, d'où la forge part, et la forge range aussi ce qu'elle a posé,
    /// comme celui d'une relance.
    pub(crate) fn apres_la_vague(
        &self,
        plan: &Plan,
        menee: &Menee,
        corrigee: Option<&Menee>,
        lancer: &dyn Fn(&Plan, &str) -> Result<Menee, Refus>,
        personnel: Option<&Path>,
        ci: (Duration, Duration),
    ) -> ExitCode {
        let fil = Fil::seance(self.forge.dossier, "humain");
        let mut suite = Suite { fil, poussee: None, ci: None, relance: None };
        let fin = match self.publier(&mut suite, plan, menee, lancer, personnel, ci) {
            Ok(()) => Ok(true),
            Err(Arret::Net(pourquoi)) => Err(pourquoi),
            Err(Arret::Refus(refus)) => {
                eprintln!("{refus}");
                let vague = suite.relance.as_ref().map_or(&menee.id, |relance| &relance.id).clone();
                let Refus { code, preuve, correction } = refus;
                let regle = code.regle().into();
                let note = Evenement::RefusForge { vague, code: code.numero(), regle, preuve, correction };
                noter(&suite.fil, &note).map(|()| false)
            }
        };
        // Après le dernier push réussi, quelle que soit la CI ; une note ratée ne laisse rien ranger.
        let fin = fin.and_then(|sans_refus| {
            let Some(poussee) = &suite.poussee else { return Ok(sans_refus) };
            self.ranger(&suite.fil, menee, poussee, false)?;
            if let Some(corrigee) = corrigee {
                self.ranger(&suite.fil, corrigee, poussee, true)?;
            }
            if let Some(relance) = &suite.relance {
                self.ranger(&suite.fil, relance, poussee, true)?;
            }
            Ok(sans_refus)
        });
        let assemblees = menee.issues.iter().all(|(_, issue)| issue.is_ok());
        match fin {
            Ok(true) if assemblees && suite.ci == Some(Couleur::Vert) => ExitCode::SUCCESS,
            Ok(_) => ExitCode::FAILURE,
            Err(pourquoi) => {
                eprintln!("la forge s'arrête : {pourquoi}");
                ExitCode::FAILURE
            }
        }
    }

    /// Le push, la PR, sa CI, puis, rouge, la relance, chaque geste noté.
    fn publier(
        &self,
        suite: &mut Suite,
        plan: &Plan,
        menee: &Menee,
        lancer: &dyn Fn(&Plan, &str) -> Result<Menee, Refus>,
        personnel: Option<&Path>,
        ci: (Duration, Duration),
    ) -> Result<(), Arret> {
        if menee.arretee {
            return Err(Arret::Net("la vague s'est arrêtée (7012), et rien ne se pousse".into()));
        }
        let (Some(branche), Some(tete)) = (&menee.branche, &menee.tete) else {
            return Err(Arret::Net("la vague n'a pas d'assemblage, et rien ne se pousse".into()));
        };
        a_pousser(&self.base, tete).map_err(Arret::Refus)?;
        let (vague, relecture, mut recu) = self.rejouee(menee, tete)?;
        let reglages = self.reglages_changes(tete).map_err(Arret::Net)?;
        let mut caracteres = plan.objectif.char_indices();
        let titre = match (caracteres.nth(255), caracteres.next()) {
            (Some((coupe, _)), Some(_)) => format!("{}…", &plan.objectif[..coupe]),
            _ => plan.objectif.clone(),
        };
        let points = self.points_a_justifier(tete).map_err(Arret::Net)?;
        // Le sceau du fil, noté juste avant le corps : le reçu le reprend.
        let sceau =
            suite.fil.sceller(&menee.id).map_err(|e| Arret::Net(format!("le sceau du fil ne s'écrit pas : {e}")))?;
        recu.sceau = Some(sceau);
        let corps = corps(plan, &vague, relecture.as_deref(), self.issue, &recu, &reglages, &points);
        let chemins: Vec<&Path> = personnel.into_iter().chain([self.forge.dossier, self.forge.depot]).collect();
        self.forge.publiable(&self.base, tete, branche, &titre, &corps, &chemins).map_err(Arret::Refus)?;
        let poussee = Evenement::Poussee { vague: menee.id.clone(), branche: branche.clone(), sha: tete.clone() };
        noter(&suite.fil, &poussee).map_err(Arret::Net)?;
        self.forge.pousser(tete, branche).map_err(Arret::Refus)?;
        suite.poussee = Some(tete.clone());
        let pr = self.forge.ouvrir(&self.branche, branche, &titre, &corps, tete).map_err(Arret::Refus)?;
        let publication = Publication { branche, tete, pr, titre, corps, chemins, ci };
        self.noter_la_pr(&suite.fil, &menee.id, &publication, tete)?;
        eprintln!("PR {} · {}", publication.pr.numero, publication.pr.url);
        let Some(rouge) = self.lire_la_ci(suite, &menee.id, &publication, tete, None)? else { return Ok(()) };
        self.relancer(suite, plan, lancer, &publication, &rouge)
    }

    /// Note `forge.pr` sous `vague` : la PR de `publication` porte `sha` dans sa branche.
    fn noter_la_pr(&self, fil: &Fil, vague: &str, publication: &Publication, sha: &str) -> Result<(), Arret> {
        let PrOuverte { numero, url } = &publication.pr;
        let (vague, branche, base) = (vague.into(), publication.branche.into(), self.branche.clone());
        let pr = Evenement::Pr {
            vague,
            branche,
            base,
            sha: sha.into(),
            numero: *numero,
            url: url.clone(),
            foi: Foi::Constate,
        };
        noter(fil, &pr).map_err(Arret::Net)
    }

    /// Lit la CI de la PR de `publication` sur `sha` ([`Forge::lire_ci`], `avant` étant le sha qu'elle remplace),
    /// puis la note sous `vague` : `forge.ci`, avec, rouge, la fin du journal de ses runs en échec après ses checks.
    /// Dit sa couleur sur stderr. Rend, rouge, ce qu'elle dit de son échec.
    fn lire_la_ci(
        &self,
        suite: &mut Suite,
        vague: &str,
        publication: &Publication,
        sha: &str,
        avant: Option<&str>,
    ) -> Result<Option<Rouge>, Arret> {
        let (intervalle, delai) = publication.ci;
        let numero = publication.pr.numero;
        let Ci { couleur, mut texte } =
            self.forge.lire_ci(numero, sha, avant, intervalle, delai).map_err(Arret::Refus)?;
        let rouge = (couleur == Couleur::Rouge)
            .then(|| Rouge { echecs: en_echec(&texte), journal: self.forge.journal_rouge(sha) });
        if let Some(rouge) = rouge.as_ref().filter(|rouge| !rouge.journal.is_empty()) {
            texte = format!("{texte}\n{}", rouge.journal);
        }
        let note = Evenement::Ci { vague: vague.into(), numero, sha: sha.into(), couleur, texte, foi: Foi::Constate };
        noter(&suite.fil, &note).map_err(Arret::Net)?;
        suite.ci = Some(couleur);
        eprintln!("CI {} (constatée)", if couleur == Couleur::Vert { "verte" } else { "rouge" });
        Ok(rouge)
    }

    /// La CI de la PR est rouge sur la tête poussée, et `rouge` dit pourquoi. Elle vaut une seule relance, dans cet
    /// ordre :
    ///
    /// - aucune tâche du plan n'est critique, sinon 8011 sans relance : le travail d'une tâche critique a été relu, et
    ///   une relance le changerait sans relecture ;
    /// - la base n'a pas bougé ([`Forge::base_a_bouge`]), sinon 8009 : Kraken ne réintègre pas ;
    /// - `lancer` lance une vague de relance ([`plan_de_relance`]) partie de la tête poussée, qui passe par la variante
    ///   de `lancer` comme toute vague. Son refus (6022, 6006…) n'est pas une sortie en 2 : c'est 8011, noté sous la
    ///   vague ; dès que la relance s'ouvre, tout se note sous elle ;
    /// - la relance doit être assemblée, et avoir changé la tête, sinon 8011 ;
    /// - la règle de publication sur toute la PR, jusqu'à la nouvelle tête (8005) ; `forge.poussee` ; le push de la
    ///   nouvelle tête dans la branche de la PR, en avance rapide et sans force (8006) ; la branche locale de la vague
    ///   avance de l'ancienne tête à la nouvelle ; `forge.pr`, la même PR, puis la CI relue et notée (`forge.ci`) ;
    /// - rouge encore, 8011. Une seule relance.
    fn relancer(
        &self,
        suite: &mut Suite,
        plan: &Plan,
        lancer: &dyn Fn(&Plan, &str) -> Result<Menee, Refus>,
        publication: &Publication,
        rouge: &Rouge,
    ) -> Result<(), Arret> {
        let (numero, tete, branche) = (publication.pr.numero, publication.tete, publication.branche);
        let refus = |preuve: String| Arret::Refus(Refus::new(Code::CiRouge, preuve, ROUGE));
        if let Some(critique) = plan.taches.iter().find(|tache| tache.critique) {
            let echecs = rouge.echecs.lines().collect::<Vec<_>>().join(" ; ");
            let id = &critique.id;
            return Err(refus(format!(
                "la CI de la PR {numero} est rouge, et sa tâche {id} est critique : pas de relance : {echecs}"
            )));
        }
        self.forge.base_a_bouge(&self.branche, tete).map_err(Arret::Refus)?;
        eprintln!("CI rouge : une relance, la vague repart de {tete}");
        let relance = plan_de_relance(plan, numero, &rouge.contexte());
        let ouverte = lancer(&relance, tete).map_err(|echec| {
            refus(format!("la vague de relance de la PR {numero} ne s'ouvre pas, {} : {}", echec.code, echec.preuve))
        })?;
        let (id, nouvelle) = (ouverte.id.clone(), ouverte.tete.clone().filter(|nouvelle| nouvelle != tete));
        let assemblee = !ouverte.arretee && ouverte.issues.iter().all(|(_, issue)| issue.is_ok());
        let dehors = ouverte.issues.iter().find_map(|(_, issue)| issue.as_ref().err()).map(|erreur| queue(erreur));
        suite.relance = Some(ouverte);
        if !assemblee {
            let dehors = dehors.unwrap_or_default();
            return Err(refus(format!("la vague de relance {id} n'a pas assemblé sa tâche ci : {dehors}")));
        }
        let Some(nouvelle) = nouvelle else {
            return Err(refus(format!("la vague de relance {id} n'a rien changé à la tête poussée {tete}")));
        };
        // raccourci : la relance pousse une tête que personne n'a relue, et le corps de la PR garde sa ligne de commit
        // relu et rejoué d'origine ; monter avec le contrôle de fusion de la CI
        self.forge
            .publiable(&self.base, &nouvelle, branche, &publication.titre, &publication.corps, &publication.chemins)
            .map_err(Arret::Refus)?;
        let poussee = Evenement::Poussee { vague: id.clone(), branche: branche.into(), sha: nouvelle.clone() };
        noter(&suite.fil, &poussee).map_err(Arret::Net)?;
        self.forge.pousser(&nouvelle, branche).map_err(Arret::Refus)?;
        suite.poussee = Some(nouvelle.clone());
        let locale = format!("refs/heads/{branche}");
        if let Err(vu) = self.forge.faire(self.forge.git(&["update-ref", &locale, &nouvelle, tete]), None) {
            eprintln!("la branche locale {branche} n'avance pas : {vu}");
        }
        self.noter_la_pr(&suite.fil, &id, publication, &nouvelle)?;
        eprintln!("PR {numero} · {} : {branche} avance à {nouvelle}", publication.pr.url);
        let Some(encore) = self.lire_la_ci(suite, &id, publication, &nouvelle, Some(tete))? else { return Ok(()) };
        let echecs = encore.echecs.lines().collect::<Vec<_>>().join(" ; ");
        Err(refus(format!("la CI de la PR {numero} est rouge encore sur la tête de la relance {nouvelle} : {echecs}")))
    }

    /// La vague `menee` telle que le fil la rejoue, avec la ligne de sa relecture s'il y en a une
    /// ([`ligne_de_relecture`]) : le corps de la PR en tire l'issue de chaque tâche, et les nombres de la relecture.
    fn rejouee(&self, menee: &Menee, tete: &str) -> Result<(Vague, Option<String>, Recu), Arret> {
        let lue = dossier::lire(self.forge.dossier)
            .map_err(|refus| Arret::Net(format!("le fil ne se relit pas : {}", refus.preuve)))?;
        let evenements: Vec<Evenement> = lue.notes.iter().map(|note| note.evenement.clone()).collect();
        let mut vagues = etat(&evenements);
        // La tête à pousser est celle que la relecture a jugée, ou sa correction : sinon rien ne se pousse (7016).
        let commit = controler_la_tete(&vagues, &evenements, &menee.id, tete).map_err(Arret::Refus)?;
        // Sous la ligne de la relecture, s'il y en a une, la ligne du commit relu et rejoué.
        let relecture = [ligne_de_relecture(&vagues, &menee.id), Some(commit)].into_iter().flatten();
        let relecture = Some(relecture.collect::<Vec<_>>().join("\n"));
        let recu = recu_de(&vagues, &lue.notes, &menee.id);
        let rang = vagues.iter().position(|vague| vague.id == menee.id);
        let rang = rang.ok_or_else(|| Arret::Net(format!("le fil ne rejoue pas la vague {}", menee.id)))?;
        Ok((vagues.swap_remove(rang), relecture, recu))
    }

    /// Les fichiers de réglage de la preuve que le diff de la base à la tête `tete` change ([`reglages_changes`]),
    /// que le corps nomme chacun par son chemin, en code, vingt au plus. Un diff que git ne rend pas est une
    /// erreur : le corps ne dit pas « aucun » sans l'avoir lu.
    fn reglages_changes(&self, tete: &str) -> Result<Vec<String>, String> {
        let diff = self.forge.git(&["diff", "--name-only", "--no-renames", "-z", &self.base, tete]);
        let noms =
            self.forge.faire(diff, None).map_err(|vu| format!("le diff de la base à la tête ne se lit pas : {vu}"))?;
        Ok(reglages_changes(noms.split('\0').filter(|nom| !nom.is_empty())))
    }

    /// Les points à justifier du diff de la base à `tete` ([`alarmes::points`]). Le diff se lit avec une borne, comme
    /// [`alarmes::lire`] : au-delà de [`TAILLE_MAX`] octets il est lu coupé. Un git qui ne le lit pas est une
    /// erreur : le corps ne se tait pas sur ce qu'il n'a pas lu.
    fn points_a_justifier(&self, tete: &str) -> Result<Vec<String>, String> {
        let mut args = alarmes::ARGUMENTS.to_vec();
        args.extend([self.base.as_str(), tete]);
        let (gardes, _) = faire_borne(self.forge.git(&args), TAILLE_MAX + 1)
            .map_err(|vu| format!("le diff de la base à la tête ne se lit pas : {vu}"))?;
        Ok(alarmes::points(&String::from_utf8_lossy(&gardes)))
    }

    /// La branche locale `nom` du dépôt visé et sa valeur, si son travail est dans `poussee` : sa valeur en est un
    /// ancêtre. `None` pour une branche qui n'est plus là ou qui a avancé : Kraken ne retire que ce qu'il a vu dans
    /// `poussee`.
    fn branche_dans(&self, nom: &str, poussee: &str) -> Option<(String, String)> {
        let lue =
            self.forge.faire(self.forge.git(&["rev-parse", "--verify", "-q", &format!("refs/heads/{nom}")]), None);
        let valeur = lue.ok()?.trim().to_string();
        self.forge.dans(self.forge.depot, &valeur, poussee).then(|| (nom.to_string(), valeur))
    }

    /// Range la vague après le dernier push réussi de la forge, `poussee`. Ne se retire que ce dont le
    /// travail y est : le bac de chaque tâche assemblée dont la tête, son gel, est `poussee` ou l'un de ses ancêtres,
    /// puis son dossier temporaire ([`temporaire_de`]), puis sa branche `lydite/<main>` si elle en est un aussi, une
    /// fois son worktree retiré ([`retirer`]) ; un bac posé par l'ancien programme porte une branche `kraken/<id>`
    /// que ce rangement ne cherche pas : la migration du dossier d'état la renomme ; puis le worktree de la
    /// vague, si sa tête en est un. Les bacs des tâches dehors restent, car leur travail, jamais gelé, n'est nulle
    /// part ailleurs. La branche de la vague relancée reste aussi, qui porte la PR ; celle d'une
    /// relance (`sa_branche`) se retire avec son worktree si elle est un ancêtre de `poussee`. Ce qui va se retirer
    /// se note d'abord (`forge.rangement`, sous la vague du bac), et une note ratée arrête la forge ; un retrait raté
    /// s'écrit sur stderr, et le reste de son worktree demeure. Les gestes de git d'un retrait passent par la forge
    /// comme les autres ([`Forge::faire`] et [`GIT_FORGE`]) : un git qui attend une réponse ou s'éternise (un fichier
    /// du bac tenu par un processus, un pilote de filtre du dépôt visé) est tué à son délai, le retrait est raté, et
    /// la forge ne reste pas suspendue, PR ouverte.
    fn ranger(&self, fil: &Fil, menee: &Menee, poussee: &str, sa_branche: bool) -> Result<(), String> {
        let bacs = self.forge.dossier.join("bacs");
        let mut retraits: Vec<(PathBuf, Option<(String, String)>)> = Vec::new();
        for (id, _) in menee.issues.iter().filter(|(_, issue)| issue.is_ok()) {
            let bac = bacs.join(id);
            if !self.forge.dans(&bac, "HEAD", poussee) {
                continue;
            }
            retraits.push((bac, self.branche_dans(&format!("lydite/{id}"), poussee)));
        }
        let worktree = bacs.join(format!("vague-{}", menee.id));
        if self.forge.dans(&worktree, "HEAD", poussee) {
            let branche = menee.branche.as_deref().filter(|_| sa_branche);
            retraits.push((worktree, branche.and_then(|nom| self.branche_dans(nom, poussee))));
        }
        if retraits.is_empty() {
            return Ok(());
        }
        let mut retires = Vec::new();
        for (worktree, branche) in &retraits {
            retires.push(worktree.display().to_string());
            let temporaire = temporaire_de(worktree).filter(|temporaire| temporaire.exists());
            retires.extend(temporaire.map(|temporaire| temporaire.display().to_string()));
            retires.extend(branche.iter().map(|(nom, _)| nom.clone()));
        }
        noter(fil, &Evenement::Rangement { vague: menee.id.clone(), retires })?;
        // Les deux gestes de git d'un retrait se jouent comme tout geste de la forge : ses variables et son délai.
        let par_la_forge = |mut geste: Command| {
            geste.envs(GIT_FORGE);
            self.forge.faire(geste, None)
        };
        for (worktree, branche) in &retraits {
            let branche = branche.as_ref().map(|(nom, valeur)| (nom.as_str(), valeur.as_str()));
            if let Err(vu) = retirer(self.forge.depot, worktree, branche, par_la_forge) {
                eprintln!("retrait raté, le reste de ce worktree demeure : {vu}");
            }
        }
        Ok(())
    }
}

/// Le reçu d'une vague et de ses vagues liées : ce que le fil dit de son coût, de ses mains et de sa durée, lu sans
/// rien écrire.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Recu {
    /// La somme des `main.usage`, en micro-dollars, toutes bases confondues (l'inconnu y est à 0).
    micro_dollars: u64,
    forfait: bool,
    estime: bool,
    inconnu: bool,
    /// Les tentatives, dont les secondes (et au-delà) sont des relances ; les mains classées.
    mains: usize,
    relances: usize,
    classees: usize,
    /// Les mains qui ont déclaré leur tâche impossible (`IMPOSSIBLE: <raison>`), comptées à part des classées.
    impossibles: usize,
    /// De la première note de la vague à la dernière de ses vagues liées, en millisecondes ; `None` sans instants.
    duree: Option<u128>,
    /// Le sceau du fil noté juste avant le corps de la PR ; `None` hors d'une PR (`recu_de` n'en lit aucun).
    sceau: Option<SceauVague>,
}

/// Le reçu de la vague `id` parmi `vagues` : elle, son découpage, ses relectures, ses corrections et ses relances, et
/// de proche en proche les vagues que celles-ci lient de même (une vague déjà liée n'est pas reprise : une boucle
/// s'arrête). Les usages et les instants viennent des `notes` du fil.
pub(crate) fn recu_de(vagues: &[Vague], notes: &[Note], id: &str) -> Recu {
    let mut ids: Vec<&str> = vec![id];
    let mut rang = 0;
    while let Some(&courant) = ids.get(rang) {
        rang += 1;
        let decoupage = vagues.iter().find(|vague| vague.id == courant).and_then(|vague| vague.decoupage.as_deref());
        for vague in vagues {
            let lie = decoupage == Some(vague.id.as_str())
                || vague.relit.as_deref() == Some(courant)
                || vague.relecture.as_ref().is_some_and(|relecture| relecture.relue == courant)
                || vague.corrige.as_deref() == Some(courant)
                || vague.relance_de.as_deref() == Some(courant);
            if lie && !ids.contains(&vague.id.as_str()) {
                ids.push(&vague.id);
            }
        }
    }
    let liees: Vec<&Vague> = vagues.iter().filter(|vague| ids.contains(&vague.id.as_str())).collect();
    let est_liee = |nom: &str| liees.iter().any(|vague| vague.id == nom);
    let mut recu = Recu::default();
    for vague in &liees {
        let mains = vague.mains.values();
        recu.mains += mains.clone().map(|main| main.tentatives.len()).sum::<usize>();
        recu.relances += mains.clone().flat_map(|main| main.tentatives.keys()).filter(|n| **n >= 2).count();
        recu.classees += mains.clone().filter(|main| matches!(main.verdict, Some((fil::Verdict::Classee, _)))).count();
        recu.impossibles += mains.filter(|main| matches!(main.verdict, Some((fil::Verdict::Impossible, _)))).count();
    }
    let (mut debut, mut fin): (Option<u128>, Option<u128>) = (None, None);
    for note in notes.iter().filter(|note| est_liee(note.evenement.vague())) {
        if let Evenement::Usage { micro_dollars, base, .. } = &note.evenement {
            recu.micro_dollars = recu.micro_dollars.saturating_add(*micro_dollars);
            match base {
                Base::Forfait => recu.forfait = true,
                Base::Estime => recu.estime = true,
                Base::Inconnu => recu.inconnu = true,
            }
        }
        let ms = millisecondes(&note.instant);
        if note.evenement.vague() == id && debut.is_none() {
            debut = ms;
        }
        fin = fin.max(ms);
    }
    recu.duree = debut.zip(fin).map(|(debut, fin)| fin.saturating_sub(debut));
    recu
}

impl Recu {
    /// Le coût : `0,84 $ estimé` s'il a une part estimée, `forfait, ≈ 0,84 $` si tout est au forfait, `inconnu` si
    /// rien n'est connu ; ` + inconnu` s'il a de plus une part inconnue.
    pub(crate) fn cout(&self) -> String {
        let dollars = dollars(self.micro_dollars);
        let connu = if self.estime {
            format!("{dollars} $ estimé")
        } else if self.forfait {
            format!("forfait, ≈ {dollars} $")
        } else {
            return "inconnu".into();
        };
        if self.inconnu { format!("{connu} + inconnu") } else { connu }
    }

    /// `7 mains, 2 relances, 1 classée`.
    pub(crate) fn mains(&self) -> String {
        let pluriel = |n: usize, mot: &str| format!("{n} {mot}{}", if n > 1 { "s" } else { "" });
        let (m, r, c) = (self.mains, self.relances, self.classees);
        let base = format!("{}, {}, {}", pluriel(m, "main"), pluriel(r, "relance"), pluriel(c, "classée"));
        match self.impossibles {
            0 => base,
            i => format!("{base}, {}", pluriel(i, "impossible")),
        }
    }

    /// `41 min` ; `None` sans instants.
    fn duree(&self) -> Option<String> {
        let secondes = self.duree? / 1000;
        Some(match secondes {
            0..60 => format!("{secondes} s"),
            60..3600 => format!("{} min", secondes / 60),
            _ => format!("{} h {:02} min", secondes / 3600, secondes % 3600 / 60),
        })
    }

    /// La ligne de `lydite etat` : `Coût : 0,84 $ estimé · 7 mains, 2 relances, 1 classée · 41 min`, sans la durée si
    /// les instants manquent.
    pub(crate) fn ligne(&self) -> String {
        let mut parties = vec![format!("Coût : {}", self.cout()), self.mains()];
        parties.extend(self.duree());
        parties.join(" · ")
    }

    /// Les lignes du bloc « Reçu » du corps d'une PR, à la suite de son titre : le coût et les mains, jamais la durée ;
    /// puis, si le reçu porte un sceau, `- Fil sealed at <octets> bytes: sha256 <empreinte>.`
    pub(crate) fn lignes_de_pr(&self) -> Vec<String> {
        let mut lignes = vec![format!("- Coût : {}", self.cout()), format!("- {}", self.mains())];
        lignes
            .extend(self.sceau.iter().map(|s| format!("- Fil sealed at {} bytes: sha256 {}.", s.octets, s.empreinte)));
        lignes
    }
}

/// Des micro-dollars en dollars à la virgule, arrondis au centime : `0,84`.
fn dollars(micro: u64) -> String {
    let centimes = micro / 10_000 + u64::from(micro % 10_000 >= 5_000);
    format!("{},{:02}", centimes / 100, centimes % 100)
}

/// Les millisecondes de l'époque Unix d'un instant que le fil écrit (`2026-10-31T21:03:00.123Z`), `None` s'il n'est pas
/// de cette forme. Le contraire de `fil::instant`.
fn millisecondes(instant: &str) -> Option<u128> {
    let nombre = |de: usize, a: usize| instant.get(de..a)?.parse::<u128>().ok();
    let (annee, mois, jour) = (nombre(0, 4)?, nombre(5, 7)?, nombre(8, 10)?);
    let (h, m, s) = (nombre(11, 13)?, nombre(14, 16)?, nombre(17, 19)?);
    let decimales = instant.get(19..)?.strip_suffix('Z')?.strip_prefix('.').unwrap_or_default();
    let ms = format!("{decimales:0<3}").get(..3)?.parse::<u128>().ok()?;
    // Les jours civils depuis 1970-01-01, par ères de 400 ans (le contraire de `fil::civil`).
    let annee = if mois <= 2 { annee.checked_sub(1)? } else { annee };
    let (ere, annee_ere) = (annee / 400, annee % 400);
    let mois_mars = if mois > 2 { mois - 3 } else { mois + 9 };
    let jour_annee = (153 * mois_mars + 2) / 5 + jour.checked_sub(1)?;
    let jour_ere = annee_ere * 365 + annee_ere / 4 - annee_ere / 100 + jour_annee;
    let jours = (ere * 146_097 + jour_ere).checked_sub(719_468)?;
    Some(((jours * 24 + h) * 60 + m) * 60_000 + s * 1000 + ms)
}

#[cfg(test)]
mod tests;
