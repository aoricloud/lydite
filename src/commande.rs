//! Kraken : ce que l'humain tape et lit ; l'usage est au README. Pour `lancer`, le code de sortie vaut 0 seulement si
//! chaque main est prise et sa prise constatée (Kraken a rejoué sa vérification), ou si chaque tâche d'une vague à plan
//! est assemblée ; 1 sinon : une prise déclarée (sans vérification, la main seule a dit `PREUVE: vert`) donne 1, comme
//! une main classée, et un refus de la forge pendant la vague aussi. Il vaut 2 pour un refus d'avant la vague, et si
//! le fil ne se lit pas. `lydite etat` y ajoute ses règles : découpage, relecture et CI verte, et le code 0 quand il
//! n'y a aucune vague, rien n'étant alors affirmé.
//!
//! Un déclaré ne s'affiche jamais comme un fait : il porte toujours son mot (« pris, déclaré : main-… » dans `lancer`,
//! « prise déclarée, non prouvée » et « mur exigé (déclaré) » dans `lydite etat`), et ne donne jamais le code 0.
//!
//! Toute sortie en 2 est un refus (`erreur.rs`) : quatre lignes sur stderr, le code et son nom, la règle, la preuve et
//! la correction. La ligne de commande se lit par `args_os` : aucun argument ne fait paniquer Kraken.

use std::env;
use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use self::ligne::{Commande, options};
use crate::branchements::dossier;
use crate::branchements::github;
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::lecons;
use crate::coeur::plan;
use crate::coeur::plan::Plan;
use crate::coeur::remarques;
use crate::conduite::lancer::{Liens, base_du_depot};
use crate::conduite::mener::{Lieu, decouper_cmd, mener_sur, preparer};
use crate::conduite::publication;
use crate::conduite::relecture;
use crate::conduite::relecture::Origine;

pub(crate) mod etat;
pub(crate) mod ligne;

pub fn principal() -> ExitCode {
    executer().unwrap_or_else(|refus| {
        eprintln!("{refus}");
        ExitCode::from(2)
    })
}

/// Lit la ligne de commande et la joue. Tout refus sort par `Err` : `main` l'écrit sur stderr et sort en 2. Le plan
/// d'un `--plan` se lit après `preparer` et avant tout bac ([`lire_plan`]) : un refus, et rien n'est parti.
/// `--decouper` passe par `decouper_cmd` ; sinon `mener_sur` mène la vague, sa relecture et sa forge.
fn executer() -> Result<ExitCode, Refus> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let (profil, depot, dossier, plan, pr, decoupe) = match options(&args)? {
        Commande::Lancer(o) => {
            let (profil, depot, dossier) = preparer(&o.profil, o.depot.as_deref())?;
            let plan = Plan::copies(&o.objectif, o.verifier.as_deref(), o.n);
            (profil, depot, dossier, plan, None, o.decouper.then_some(o))
        }
        Commande::Plan { profil, plan, depot, pr } => {
            let (profil, depot, dossier) = preparer(&profil, depot.as_deref())?;
            (profil, depot, dossier, lire_plan(&plan, plan::Mode::Main)?.0, pr, None)
        }
        Commande::Accepter(nom) => return accepter_cmd(&nom),
        Commande::Etat => return etat_cmd(),
        Commande::LirePlan { fichier, decoupe } => return plan_cmd(&fichier, decoupe),
        Commande::LireRemarques { fichier } => return remarques_cmd(&fichier),
        Commande::LireLecons { dossier } => return lecons_cmd(&dossier),
    };
    let (gh, personnel) = (github::programme_gh(), env::home_dir());
    let lieu = Lieu {
        profil: &profil,
        depot: &depot,
        dossier: &dossier,
        gh: &gh,
        personnel: personnel.as_deref(),
        ci: github::ATTENTE_DE_LA_CI,
    };
    if let Some(o) = decoupe {
        return decouper_cmd(&lieu, &o);
    }
    let Some(base) = pr else {
        return mener_sur(&lieu, &plan, &base_du_depot(&depot)?, Liens::default(), None, Origine::Plan);
    };
    let proposition = publication::avant_la_vague(&gh, &dossier, &depot, &base)?;
    if let Some(ecart) = proposition.ecart_de_head() {
        eprintln!("{ecart}");
    }
    mener_sur(&lieu, &plan, &proposition.base, Liens::default(), Some(&proposition), Origine::Plan)
}

/// Le plan du fichier, lu dans son mode, et les dépendances que Kraken y ajoute : ses octets, 1 Mo au plus et un de
/// plus pour voir qu'il dépasse (un fichier plus gros ne se lit pas en entier), puis `Plan::lire_en`.
fn lire_plan(fichier: &Path, mode: plan::Mode<'_>) -> Result<(Plan, Vec<plan::Attente>), Refus> {
    use std::io::Read;
    let mut octets = Vec::new();
    let borne = plan::TAILLE_MAX as u64 + 1;
    let lu = std::fs::File::open(fichier).and_then(|ouvert| ouvert.take(borne).read_to_end(&mut octets));
    lu.map_err(|e| {
        let preuve = format!("lecture du plan {} : {e}", fichier.display());
        let correction = "Donne à --plan, ou à lydite plan, le chemin d'un fichier que Lydite peut lire (un chemin \
                          relatif part du dossier courant), puis relance.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    })?;
    Plan::lire_en(&octets, mode)
}

/// `lydite plan [--decoupe] <fichier>` : lit le plan et écrit sur stdout chaque dépendance que Kraken ajoute. Le mode
/// découpé ne juge ni l'en-tête ni la casse, que seul le rejeu juge.
fn plan_cmd(fichier: &Path, decoupe: bool) -> Result<ExitCode, Refus> {
    let mode = if decoupe { plan::Mode::Decoupe(None) } else { plan::Mode::Main };
    for attente in lire_plan(fichier, mode)?.1 {
        println!("{attente}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `lydite remarques <fichier>` : lit le fichier comme Kraken lira celui du relecteur
/// (`remarques::lire_fichier`, puis la forme 2 que le juge exige), et ne fait rien d'autre : rien sur stdout.
fn remarques_cmd(fichier: &Path) -> Result<ExitCode, Refus> {
    let octets = relecture::lire_borne(fichier).map_err(|e| {
        let preuve = format!("lecture des remarques {} : {e}", fichier.display());
        let correction = "Donne à lydite remarques le chemin d'un fichier que Lydite peut lire (un chemin relatif \
                          part du dossier courant), puis relance.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    })?;
    remarques::lire_fichier(&octets)?.exiger_la_forme_2().map(|_| ExitCode::SUCCESS)
}

/// `lydite lecons <dossier>` : lit chaque `.agents/skills/*/SKILL.md` du dépôt de ce dossier, dans l'ordre des noms, et
/// ne dit rien si toutes sont justes. Sinon, le refus de la première fausse, avec son fichier et sa ligne. Un dépôt
/// sans `.agents/skills`, ou un sous-dossier sans `SKILL.md`, n'a aucune leçon à lire.
fn lecons_cmd(depot: &Path) -> Result<ExitCode, Refus> {
    use std::io::Read;
    let illisible = |chemin: &Path, e: std::io::Error| {
        let preuve = format!("lecture des leçons {} : {e}", chemin.display());
        let correction = "Donne à lydite lecons le dossier d'un dépôt dont `.agents/skills` se lit (un chemin relatif \
                          part du dossier courant), puis relance.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    };
    let skills = depot.join(".agents").join("skills");
    let entrees = match std::fs::read_dir(&skills) {
        Ok(entrees) => entrees,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ExitCode::SUCCESS),
        Err(e) => return Err(illisible(&skills, e)),
    };
    let mut dossiers = Vec::new();
    for entree in entrees {
        let entree = entree.map_err(|e| illisible(&skills, e))?;
        if entree.file_type().map_err(|e| illisible(&entree.path(), e))?.is_dir() {
            dossiers.push(entree.file_name());
        }
    }
    dossiers.sort();
    for nom in dossiers {
        let fichier = skills.join(&nom).join("SKILL.md");
        let mut octets = Vec::new();
        let borne = lecons::TAILLE_MAX as u64 + 1;
        match std::fs::File::open(&fichier).and_then(|ouvert| ouvert.take(borne).read_to_end(&mut octets)) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(illisible(&fichier, e)),
        }
        lecons::lire(&nom.to_string_lossy(), &octets).map_err(|refus| {
            Refus::new(refus.code, format!("{} : {}", fichier.display(), refus.preuve), refus.correction)
        })?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `lydite accepter <nom>` : lit et valide le profil du dossier de Kraken, écrit l'empreinte qu'il accepte, puis dit à
/// l'humain ce qu'il vient d'accepter : le texte du profil et son empreinte.
fn accepter_cmd(nom: &str) -> Result<ExitCode, Refus> {
    let (texte, empreinte) = dossier::accepter(nom, &dossier::dossier()?)?;
    println!("{}\nempreinte : {empreinte}", texte.trim_end());
    Ok(ExitCode::SUCCESS)
}

fn etat_cmd() -> Result<ExitCode, Refus> {
    etat::etat_dans(&dossier::dossier()?)
}

#[cfg(test)]
mod tests;
