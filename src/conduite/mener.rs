//! La conduite de la vague : ce que `lancer` lit avant de partir, la vague, sa relecture et sa forge.

use std::env;
use std::ffi::OsStr;
use std::path::{self, Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use crate::branchements::dossier;
use crate::branchements::git;
use crate::branchements::github;
use crate::branchements::srt;
use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::plan;
use crate::coeur::plan::Plan;
use crate::coeur::profil::Profil;
use crate::conduite::decoupage;
use crate::conduite::decoupage::Decoupage;
use crate::conduite::lancer::{Liens, Menee, base_du_depot, duree_du_bail, lancer_avec};
use crate::conduite::publication;
use crate::conduite::publication::Proposition;
use crate::conduite::relecture::{Origine, Prise, Relue, relire, relire_issue};
use crate::conduite::tentative::Pris;

/// Ce que `lancer` demande.
#[derive(Debug, PartialEq)]
pub(crate) struct Options {
    pub(crate) profil: String,
    pub(crate) objectif: String,
    pub(crate) n: usize,
    pub(crate) verifier: Option<String>,
    pub(crate) depot: Option<PathBuf>,
    /// `--decouper` : une main découpe l'objectif en un plan, que la vague joue.
    pub(crate) decouper: bool,
    /// `--observer`, avec `--decouper` : le plan se montre sans que rien se construise.
    pub(crate) observer: bool,
    /// `--pr`, avec `--decouper` : la branche du distant, base de la PR de la vague.
    pub(crate) pr: Option<String>,
    /// `--issue <n>`, avec `--decouper` : l'issue du distant `origin` que la main découpe, à la place d'un objectif
    /// tapé (`objectif` est alors vide).
    pub(crate) issue: Option<u64>,
}

/// Ce qu'une vague de `lancer` tient de `preparer` et de son environnement : le profil, le dépôt visé, le dossier de
/// Kraken, et, pour sa forge, `gh`, le dossier personnel que la règle de publication cherche, et la lecture de sa CI.
pub(crate) struct Lieu<'a> {
    pub(crate) profil: &'a Profil,
    pub(crate) depot: &'a Path,
    pub(crate) dossier: &'a Path,
    pub(crate) gh: &'a OsStr,
    pub(crate) personnel: Option<&'a Path>,
    pub(crate) ci: (Duration, Duration),
}

/// Ce que `lancer` lit avant de partir : le dossier de Kraken, le dépôt visé (le dossier courant par défaut) et le
/// profil, rendus bornés et prêts à lancer. Tout refus sort avant tout bac et tout fil, le premier qui s'applique, dans
/// cet ordre :
///
/// 1. le dossier : `KRAKEN_DOSSIER` relatif (6004), ou aucun dossier à tirer de l'environnement (6005) ;
/// 2. le dépôt : son chemin absolu (6006), un dossier qui existe (6002), avant tout geste git ;
/// 3. git : il connaît le dépôt (6003, sa sortie en preuve) et se lance (6006) ;
/// 4. le dossier est hors du dépôt visé (6015) ;
/// 5. le profil : son nom (6007), son fichier (6008, 6009, 6010) ou le profil intégré du même nom, sinon inconnu
///    (6001) ;
/// 6. son empreinte est celle qu'a acceptée `kraken accepter` (6011, 6012) ;
/// 7. le dossier et les dossiers de connexion de Claude (`CLAUDE_CONFIG_DIR` s'il est posé, puis le `.claude` du
///    dossier personnel) s'écrivent dans une règle de permission (6023), et aucune `lecture` ne recouvre l'un d'eux
///    (6009) ;
/// 8. si le profil nomme un fournisseur, la variable d'environnement que `cle` nomme est posée et non vide (6027) : sa
///    valeur est la clé de ses mains, et n'est lue qu'ici ;
/// 9. sous un mur exigé, avant toute main et avant l'épreuve de la base (6028, voir `srt::eprouver_le_mur`) :
///    srt (`KRAKEN_SRT` s'il est posé, lu ici) joue `true` sous des réglages de rejeu.
///
/// Le profil se lit après le dépôt : il vit dans le dossier de Kraken, qu'on sait alors hors du dépôt visé.
pub(crate) fn preparer(profil: &str, depot: Option<&Path>) -> Result<(Profil, PathBuf, PathBuf), Refus> {
    let config = env::var_os("CLAUDE_CONFIG_DIR").map(PathBuf::from);
    let (mut profil, depot, dossier) =
        preparer_dans(profil, depot, dossier::dossier()?, config.as_deref(), env::home_dir().as_deref())?;
    profil.fournir(|variable| env::var_os(variable))?;
    if let Some(srt) = env::var_os("KRAKEN_SRT") {
        profil.avec_srt(srt);
    }
    srt::eprouver_le_mur(&profil, &dossier)?;
    Ok((profil, depot, dossier))
}

/// `preparer` à partir du dossier de Kraken, de `CLAUDE_CONFIG_DIR` (`config`) et du dossier personnel (`personnel`),
/// lus une fois par `preparer` : les tests n'ont pas à toucher l'environnement de l'humain.
fn preparer_dans(
    nom: &str,
    depot: Option<&Path>,
    dossier: PathBuf,
    config: Option<&Path>,
    personnel: Option<&Path>,
) -> Result<(Profil, PathBuf, PathBuf), Refus> {
    let depot = match depot {
        Some(depot) => path::absolute(depot).map_err(|e| nommee(e, format!("chemin absolu de {}", depot.display())))?,
        None => env::current_dir().map_err(|e| nommee(e, "lecture du dossier courant"))?,
    };
    let (racine, git_commun) = git::racines(&depot)?;
    git::hors_du_depot(&racine, &dossier)?;
    let mut profil = dossier::charger_dans(nom, &dossier)?;
    profil.garder_selon(dossier::lire_empreinte)?;
    profil.borner_selon(&dossier, &git_commun, config, personnel, dossier::connexion_presente)?;
    Ok((profil, depot, dossier))
}

/// La vague du plan, partie de `base` avec ses liens de découpage, de relecture et de correction, dite comme toute
/// vague (`dire`) ; puis la relecture du travail de ses tâches critiques (`relecture::relire`), dite sur stderr : une
/// relecture classée (7016) ou une correction qui ne s'assemble pas (7017) s'écrit comme refus, rien ne se pousse, et
/// le code est 1. Corrigée, la vague finit à la tête de la correction, d'où part la forge. Puis, avec `proposition`,
/// sa forge jusqu'à sa PR (`publication.rs`), qui rend le code, et dont une CI rouge se relance par une vague de plus,
/// sans lien, dite de même. Sans forge, le code est 0 si chaque tâche est prise et sa prise constatée, 1 sinon. Une
/// vague née d'une issue (`origine`) se relit par `relecture::relire_issue`.
pub(crate) fn mener_sur(
    lieu: &Lieu<'_>,
    plan: &Plan,
    base: &str,
    liens: Liens<'_>,
    proposition: Option<&Proposition<'_>>,
    origine: Origine,
) -> Result<ExitCode, Refus> {
    let bail = duree_du_bail(lieu.profil);
    let lancer_la_vague = |plan: &Plan, base: &str, liens: Liens<'_>| -> Result<Menee, Refus> {
        let menee = lancer_avec(lieu.depot, lieu.dossier, lieu.profil, plan, bail, base, liens)?;
        dire(plan, menee.branche.as_deref(), &menee.issues);
        Ok(menee)
    };
    let mut menee = lancer_la_vague(plan, base, liens)?;
    let relire = if origine == Origine::Issue { relire_issue } else { relire };
    let corrigee = match relire(lieu.depot, lieu.dossier, lieu.profil, plan, &menee, base, bail) {
        Relue::Aucune => None,
        Relue::Prise(Prise { main, n, bloquantes, correction }) => {
            eprintln!("relecture prise : {main} · {n} remarques, dont {bloquantes} bloquantes");
            if let Some(corrigee) = &correction {
                let main = corrigee.issues.first().map_or("", |(main, _)| main.as_str());
                let tete = corrigee.tete.as_deref().unwrap_or_default();
                eprintln!("correction assemblée : {main} · la vague finit à {tete}");
            }
            correction
        }
        Relue::Dehors(refus) => {
            eprintln!("{refus}");
            return Ok(ExitCode::FAILURE);
        }
    };
    if let Some(tete) = corrigee.as_ref().and_then(|corrigee| corrigee.tete.clone()) {
        menee.tete = Some(tete);
    }
    let Some(proposition) = proposition else {
        return Ok(if toutes_constatees(&menee.issues) { ExitCode::SUCCESS } else { ExitCode::FAILURE });
    };
    let relancer = |plan: &Plan, base: &str| lancer_la_vague(plan, base, Liens::default());
    Ok(proposition.apres_la_vague(plan, &menee, corrigee.as_ref(), &relancer, lieu.personnel, lieu.ci))
}

/// `lancer --decouper`, après `preparer` : avec `--issue`, avant tout bac, un profil dont le mur n'est pas exigé est le
/// refus 6029, puis l'issue se lit sur le distant `origin` du dépôt visé (`github::lire_issue`). Avec `--pr`, la forge
/// lit ensuite ce qu'il lui faut et la base est la tête distante, sinon le `HEAD` du dépôt visé ; puis le découpage
/// (`decoupage::decouper`, ou `decoupage::decouper_issue`), qui éprouve la base (6022) avant son découpeur. Dehors, la
/// ligne de sa main va sur stderr, aucune vague ne part, et le code est 1. Pris, le découpage se dit sur stderr ; avec
/// `--observer`, le plan va sur stdout, et le code est 0, sans vague ; sinon la vague du plan part de la même base,
/// liée à son découpage (et à son issue, `vague.issue`), et `mener_sur` la mène comme celle d'un `--plan`, avec `--pr`
/// jusqu'à sa PR, dont le corps ferme l'issue (`Closes #<n>`).
pub(crate) fn decouper_cmd(lieu: &Lieu<'_>, o: &Options) -> Result<ExitCode, Refus> {
    let issue = match o.issue {
        Some(numero) => Some(lire_l_issue(lieu, &o.profil, numero)?),
        None => None,
    };
    let mut proposition = match &o.pr {
        Some(branche) => Some(publication::avant_la_vague(lieu.gh, lieu.dossier, lieu.depot, branche)?),
        None => None,
    };
    if let Some(ecart) = proposition.as_ref().and_then(Proposition::ecart_de_head) {
        eprintln!("{ecart}");
    }
    if let (Some(proposition), Some(issue)) = (proposition.as_mut(), &issue) {
        proposition.fermer_l_issue(issue.numero);
    }
    let base = match &proposition {
        Some(proposition) => proposition.base.clone(),
        None => base_du_depot(lieu.depot)?,
    };
    // `options` ne lit pas `--decouper` sans `--verifier`.
    let preuve = o.verifier.as_deref().unwrap_or_default();
    let bail = duree_du_bail(lieu.profil);
    let (depot, dossier, profil) = (lieu.depot, lieu.dossier, lieu.profil);
    let Decoupage { main, issue: sortie } = match &issue {
        Some(lue) => {
            let source = decoupage::Issue { numero: lue.numero, titre: &lue.titre, corps: &lue.corps };
            decoupage::decouper_issue(depot, dossier, profil, &source, preuve, &base, bail)?
        }
        None => decoupage::decouper(depot, dossier, profil, &o.objectif, preuve, &base, bail)?,
    };
    let decoupe = match sortie {
        Ok(decoupe) => decoupe,
        Err(dehors) => {
            eprintln!("{}\nle découpage n'est pas pris : aucune vague ne part", annonce(&main, &Err(dehors)));
            return Ok(ExitCode::FAILURE);
        }
    };
    eprintln!("découpage pris : {main} · {} tâches", decoupe.plan.taches.len());
    if o.observer {
        println!("{}", plan::sans_controle_en_lignes(decoupe.texte.trim_end()));
        return Ok(ExitCode::SUCCESS);
    }
    let numero = issue.as_ref().map(|issue| issue.numero);
    let liens = Liens { decoupage: Some(&decoupe.vague), issue: numero, ..Liens::default() };
    let origine = if numero.is_some() { Origine::Issue } else { Origine::Plan };
    mener_sur(lieu, &decoupe.plan, &base, liens, proposition.as_ref(), origine)
}

/// L'issue `numero` que `--issue` nomme, lue avant tout bac : sous un profil dont le mur n'est pas exigé, le refus
/// 6029, car son texte vient d'un autre ; puis sa lecture sur le distant `origin` du dépôt visé
/// (`github::lire_issue` : 8001, 8012, 8013).
fn lire_l_issue(lieu: &Lieu<'_>, nom: &str, numero: u64) -> Result<github::Issue, Refus> {
    if !lieu.profil.exige() {
        return Err(Refus::new(
            Code::IssueSansMur,
            format!("le profil {nom} n'exige pas le mur, et --issue {numero} ferait découper le texte d'un autre"),
            format!(
                "Écris `mur = exige` dans le profil {nom}, puis kraken accepter {nom}, ou tape toi-même l'objectif \
                 à la place de --issue, puis relance."
            ),
        ));
    }
    github::lire_issue(lieu.gh, lieu.dossier, lieu.depot, numero)
}

/// Ce que `lancer` dit de sa vague : sur stdout, chaque sortie prise ; sur stderr, une ligne par tâche, puis la
/// branche de la vague d'un plan.
fn dire(plan: &Plan, branche: Option<&str>, mains: &[(String, Result<Pris, String>)]) {
    for ((id, issue), tache) in mains.iter().zip(&plan.taches) {
        if let Ok(Pris { sortie, .. }) = issue {
            println!("{sortie}");
        }
        eprintln!("{}", if branche.is_some() { annonce_tache(id, &tache.id, issue) } else { annonce(id, issue) });
    }
    if let Some(branche) = branche {
        eprintln!("branche de la vague : {branche}");
    }
}

/// Ce que `lancer` écrit sur stderr pour une main : « pris : <id> » si sa prise est constatée, « pris, déclaré : <id> »
/// si elle n'est que déclarée, « refusée : <id> » et son refus en clair si son issue commence par `refusée (`, le début
/// qu'écrit `refus_en_clair`, « échec classé : <id> » et son erreur sinon.
// raccourci : le même qu'`annonce_tache` : une erreur de main coupée à ses 4000 derniers caractères peut commencer
// ainsi, et sa main s'afficher refusée ; le code reste 1, et le fil la dit classée ; monter avec lui
pub(crate) fn annonce(id: &str, issue: &Result<Pris, String>) -> String {
    match issue {
        Ok(Pris { prouve: true, .. }) => format!("pris : {id}"),
        Ok(Pris { prouve: false, .. }) => format!("pris, déclaré : {id}"),
        Err(refus) if refus.starts_with("refusée (") => format!("refusée : {id}\n{refus}"),
        Err(erreur) => format!("échec classé : {id}\n{erreur}"),
    }
}

/// Ce que `lancer --plan` écrit sur stderr pour une tâche : « assemblée : <main> (<id>) » si sa prise est dans la
/// branche de la vague ; « refusée : <main> (<id>) » et son refus en clair ; « classée : <main> (<id>) » et sa dernière
/// erreur. Une issue `Err` qui commence par `refusée (`, le début qu'écrit `refus_en_clair`, est une refusée, et toute
/// autre une classée.
// raccourci : une erreur de main coupée à ses 4000 derniers caractères peut commencer ainsi, et sa tâche s'afficher
// refusée ; le code reste 1, et le fil la dit classée ; monter vers une issue qui dit son cas si le besoin s'en montre
fn annonce_tache(main: &str, id: &str, issue: &Result<Pris, String>) -> String {
    match issue {
        Ok(_) => format!("assemblée : {main} ({id})"),
        Err(refus) if refus.starts_with("refusée (") => format!("refusée : {main} ({id})\n{refus}"),
        Err(erreur) => format!("classée : {main} ({id})\n{erreur}"),
    }
}

pub(crate) fn toutes_constatees(mains: &[(String, Result<Pris, String>)]) -> bool {
    mains.iter().all(|(_, issue)| matches!(issue, Ok(Pris { prouve: true, .. })))
}

#[cfg(test)]
mod tests;
