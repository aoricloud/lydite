//! Kraken : ce que l'humain tape et lit ; l'usage est au README. Pour `lancer`, le code de sortie vaut 0 seulement si
//! chaque main est prise et sa prise constatée (Kraken a rejoué sa vérification), ou si chaque tâche d'une vague à plan
//! est assemblée ; 1 sinon : une prise déclarée (sans vérification, la main seule a dit `PREUVE: vert`) donne 1, comme
//! une main classée, et un refus de la forge pendant la vague aussi. Il vaut 2 pour un refus d'avant la vague, et si
//! le fil ne se lit pas. `lydite etat` y ajoute ses règles : découpage, relecture et CI verte, et le code 0 quand il
//! n'y a aucune vague, rien n'étant alors affirmé. Il vaut 3 pour un lancement interrompu : la dernière vague du fil
//! porte son `vague.lancement` et aucun `vague.fin`, et aucun lancement ne tient plus le dossier (le verrou se sonde un
//! instant) ; ce que le texte compte alors est partiel, et ses mains qui couraient se disent interrompues. Un fil
//! illisible et un sceau rompu (2) se jugent avant ; une dernière vague sans `vague.lancement` (un fil d'avant) se
//! lit comme avant. `lydite lancer` ne rend jamais 3.
//!
//! `lydite attendre` attend la fin du lancement qui tient le dossier, puis, si le fil a au moins une note de plus
//! qu'à son départ, dit la dernière vague comme `lydite etat` et en rend le code : 0 si elle est menée à bout et
//! verte ; 1 si elle est rouge, ou finie sans son sceau de fin (un lancement tué, même sans note de plus) ; 2 si le fil
//! ne se lit pas ou si un sceau est rompu, si le lancement attendu n'a ouvert aucune vague ni fait avancer aucune
//! (aucune note de plus), et si, faute de dossier tenu comme de note neuve en 60 s, il n'y a rien à attendre ; 3 si le
//! lancement attendu est mort sans son `vague.fin`, avec ou sans note de plus, ce qui l'emporte sur le 1 du sceau de
//! fin.
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
use crate::coeur::constats::{self, Rapport};
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::json;
use crate::coeur::lecons;
use crate::coeur::plan;
use crate::coeur::plan::Plan;
use crate::coeur::remarques;
use crate::conduite::lancer::{Liens, Posee, base_du_depot};
use crate::conduite::mener::{Lieu, decouper_cmd, mener_sur, preparer};
use crate::conduite::publication;
use crate::conduite::relecture;
use crate::conduite::relecture::Origine;

pub(crate) mod doctor;
pub(crate) mod etat;
pub(crate) mod ligne;
pub(crate) mod memoire;

pub fn principal() -> ExitCode {
    let sortie = executer().unwrap_or_else(|refus| {
        eprintln!("{refus}");
        ExitCode::from(2)
    });
    avec_sa_fin(sortie)
}

/// À la sortie, pour toutes les sorties en un seul endroit : si ce processus a ouvert au moins une vague, il note au
/// fil le `vague.fin` de la dernière, avec le code qu'il rend (0, 1, ou 2 pour un refus). `ExitCode` ne dit pas son
/// nombre : il se compare à `SUCCESS` et à `FAILURE`, et tout autre est le 2 d'un refus. Une commande qui n'ouvre
/// aucune vague ne note rien. Une fin qui ne s'écrit pas se dit sur stderr et fait rendre 1 à la place de 0.
fn avec_sa_fin(sortie: ExitCode) -> ExitCode {
    let code: u8 = if sortie == ExitCode::SUCCESS {
        0
    } else if sortie == ExitCode::FAILURE {
        1
    } else {
        2
    };
    match dossier::noter_la_fin(code) {
        Ok(()) => sortie,
        Err(e) => {
            eprintln!("the end of the launch is not written to the thread: {e}");
            if code == 0 { ExitCode::FAILURE } else { sortie }
        }
    }
}

/// Lit la ligne de commande et la joue. Tout refus sort par `Err` : `main` l'écrit sur stderr et sort en 2. Le plan
/// d'un `--plan` se lit après `preparer` et avant tout bac ([`lire_plan`]) : un refus, et rien n'est parti.
/// `--decouper` passe par `decouper_cmd` ; sinon `mener_sur` mène la vague, sa relecture et sa forge.
fn executer() -> Result<ExitCode, Refus> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let (profil, depot, dossier, plan, pr, decoupe, empreinte, posees) = match options(&args)? {
        Commande::Lancer(o) => {
            let (profil, depot, dossier) = preparer(&o.profil, o.depot.as_deref())?;
            let plan = Plan::copies(&o.objectif, o.verifier.as_deref(), o.n);
            (profil, depot, dossier, plan, None, o.decouper.then_some(o), None, Vec::new())
        }
        Commande::Plan { profil, plan, depot, pr } => {
            let (profil, depot, dossier) = preparer(&profil, depot.as_deref())?;
            let (plan, _, empreinte, posees) = lire_plan(&plan, plan::Mode::Main)?;
            (profil, depot, dossier, plan, pr, None, Some(empreinte), posees)
        }
        Commande::Accepter(nom) => return accepter_cmd(&nom),
        Commande::Profil(nom) => return profil_cmd(&nom),
        Commande::Doctor(nom) => return doctor::doctor_cmd(nom.as_deref()),
        Commande::Etat => return etat_cmd(),
        Commande::Attendre => return attendre_cmd(),
        Commande::Memoire => return memoire::memoire_cmd(),
        Commande::Rappel(texte) => return memoire::rappel_cmd(&texte),
        Commande::LirePlan { fichier, decoupe } => return plan_cmd(&fichier, decoupe),
        Commande::LireRemarques { fichier } => return remarques_cmd(&fichier),
        Commande::LireConstats { rapport, plan } => {
            return constats_cmd(
                &rapport,
                plan.as_ref().map(|(identite, preuve)| (identite.as_str(), preuve.as_str())),
            );
        }
        Commande::LireLecons { dossier } => return lecons_cmd(&dossier),
        Commande::Version => return version_cmd(),
        Commande::Skill => return skill_cmd(),
        Commande::Aide => return aide_cmd(),
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
    // L'empreinte des octets du fichier de `--plan` : la vague du plan la porte à son `vague.lancement`. Le contenu des
    // fichiers que le plan pose arrive tel que `lire_plan` l'a lu, sans rien relire du disque.
    let liens = Liens { plan: empreinte.as_deref(), poses: &posees, ..Liens::default() };
    let Some(base) = pr else {
        return mener_sur(&lieu, &plan, &base_du_depot(&depot)?, liens, None, Origine::Plan);
    };
    let proposition = publication::avant_la_vague(&gh, &dossier, &depot, &base)?;
    if let Some(ecart) = proposition.ecart_de_head() {
        eprintln!("{ecart}");
    }
    mener_sur(&lieu, &plan, &proposition.base, liens, Some(&proposition), Origine::Plan)
}

/// Les fichiers qu'un plan pose, au plus.
const POSES_MAX: usize = 64;

/// Un fichier que le plan pose pèse 4 Mio au plus.
const POSE_TAILLE_MAX: u64 = 4 * 1024 * 1024;

/// Le refus 6036 : ce qui est refusé (un fichier que le plan pose, ou le plan) et ce qui a été vu.
fn pose_refusee(sujet: impl std::fmt::Display, vu: impl std::fmt::Display) -> Refus {
    let preuve = format!("{sujet}: {vu}");
    let correction = "Put each file that the plan places, with the bytes whose SHA-256 the plan writes, at the same \
                      path under the folder `<plan file>.files/` next to the plan: a regular file, never a symbolic \
                      link, of 4 MiB at most; a plan places 64 files at most.";
    Refus::new(Code::FichierPoseRefuse, preuve, correction)
}

/// Le contenu des fichiers que le plan pose, dans l'ordre du plan : un plan sans `place` n'en lit aucun, n'ouvre rien.
/// Le dossier est `<fichier du plan>.files`, voisin du plan, et chaque fichier y a son chemin. Aucun lien symbolique
/// n'est suivi (le dossier, ses sous-dossiers, le fichier : `symlink_metadata`), le fichier ne passe pas 4 Mio (lu
/// borné, un octet de plus pour voir qu'il dépasse) et ses octets ont l'empreinte du plan. Le refus est 6036.
fn lire_poses(fichier_du_plan: &Path, plan: &Plan) -> Result<Vec<Posee>, Refus> {
    use std::io::Read;
    let total: usize = plan.taches.iter().map(|tache| tache.poses.len()).sum();
    if total == 0 {
        return Ok(Vec::new());
    }
    if total > POSES_MAX {
        let quoi = format!("it places {total} files, and {POSES_MAX} at most");
        return Err(pose_refusee("the plan", quoi));
    }
    let mut nom = fichier_du_plan.as_os_str().to_os_string();
    nom.push(".files");
    let racine = std::path::PathBuf::from(nom);
    // Le chemin est de la forme que `symlink_metadata` ne suit pas : un dossier ou un fichier ordinaire, sinon dit.
    let verifier = |chemin: &Path, dossier: bool| -> Result<(), String> {
        let ici = chemin.display();
        match std::fs::symlink_metadata(chemin) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!("{ici} is missing")),
            Err(e) => Err(format!("{ici} cannot be read: {e}")),
            Ok(vu) if vu.file_type().is_symlink() => Err(format!("{ici} is a symbolic link, which is not followed")),
            Ok(vu) if dossier && !vu.is_dir() => Err(format!("{ici} is not a folder")),
            Ok(vu) if !dossier && !vu.is_file() => Err(format!("{ici} is not a regular file")),
            Ok(_) => Ok(()),
        }
    };
    let mut posees = Vec::with_capacity(total);
    for tache in &plan.taches {
        for (fichier, empreinte) in &tache.poses {
            let refus = |vu: String| pose_refusee(format!("the file `{fichier}` that the plan places"), vu);
            let mut chemin = racine.clone();
            verifier(&chemin, true).map_err(refus)?;
            let noms: Vec<&str> = fichier.split('/').collect();
            for (rang, nom) in noms.iter().enumerate() {
                chemin.push(nom);
                verifier(&chemin, rang + 1 < noms.len()).map_err(refus)?;
            }
            let mut octets = Vec::new();
            let lu = std::fs::File::open(&chemin)
                .and_then(|ouvert| ouvert.take(POSE_TAILLE_MAX + 1).read_to_end(&mut octets));
            lu.map_err(|e| refus(format!("{} cannot be read: {e}", chemin.display())))?;
            if octets.len() as u64 > POSE_TAILLE_MAX {
                return Err(refus(format!("{} weighs more than 4 MiB", chemin.display())));
            }
            let vue = hex(&sha256(&octets));
            if vue != *empreinte {
                let quoi = format!("the SHA-256 of {} is {vue}, and the plan writes {empreinte}", chemin.display());
                return Err(refus(quoi));
            }
            posees.push(Posee { tache: tache.id.clone(), fichier: fichier.clone(), octets });
        }
    }
    Ok(posees)
}

/// Le plan du fichier, lu dans son mode, et les dépendances que Kraken y ajoute : ses octets, 1 Mo au plus et un de
/// plus pour voir qu'il dépasse (un fichier plus gros ne se lit pas en entier), puis `Plan::lire_en`. Le troisième
/// rendu est l'empreinte de ces octets, SHA-256 en 64 hexadécimaux minuscules : celle que `sha256sum <fichier>` donne.
/// Le quatrième est le contenu des fichiers que le plan pose ([`lire_poses`]), lu après le plan : rien pour un plan
/// sans `place`.
fn lire_plan(fichier: &Path, mode: plan::Mode<'_>) -> Result<(Plan, Vec<plan::Attente>, String, Vec<Posee>), Refus> {
    use std::io::Read;
    let mut octets = Vec::new();
    let borne = plan::TAILLE_MAX as u64 + 1;
    let lu = std::fs::File::open(fichier).and_then(|ouvert| ouvert.take(borne).read_to_end(&mut octets));
    lu.map_err(|e| {
        let preuve = format!("reading the plan {}: {e}", fichier.display());
        let correction = "Give --plan, or lydite plan, the path of a file that Lydite can read (a relative path \
                          starts from the current folder), then run again.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    })?;
    let empreinte = hex(&sha256(&octets));
    let (plan, attentes) = Plan::lire_en(&octets, mode)?;
    let posees = lire_poses(fichier, &plan)?;
    Ok((plan, attentes, empreinte, posees))
}

/// `lydite plan [--decoupe] <fichier>` : lit le plan et écrit sur stdout chaque dépendance que Kraken ajoute. Le mode
/// découpé ne juge ni l'en-tête ni la casse, que seul le rejeu juge.
fn plan_cmd(fichier: &Path, decoupe: bool) -> Result<ExitCode, Refus> {
    let mode = if decoupe { plan::Mode::Decoupe(None) } else { plan::Mode::Main };
    let (plan, attentes, ..) = lire_plan(fichier, mode)?;
    // Permis, et voulu : un plan sans tâche critique n'est pas relu. L'auteur doit le savoir ; rien sur stdout, où le
    // plugin Hermes lit une dépendance par ligne, et rien pour le découpeur.
    if !decoupe && !plan.taches.iter().any(|tache| tache.critique) {
        eprintln!("No task is critical: nothing in this plan is reviewed.");
    }
    for attente in attentes {
        println!("{attente}");
    }
    Ok(ExitCode::SUCCESS)
}

/// `lydite remarques <fichier>` : lit le fichier comme Kraken lira celui du relecteur
/// (`remarques::lire_fichier`, puis la forme 2 que le juge exige), et ne fait rien d'autre : rien sur stdout.
fn remarques_cmd(fichier: &Path) -> Result<ExitCode, Refus> {
    let octets = relecture::lire_borne(fichier).map_err(|e| {
        let preuve = format!("reading the remarks {}: {e}", fichier.display());
        let correction = "Give lydite remarks the path of a file that Lydite can read (a relative path starts \
                          from the current folder), then run again.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    })?;
    remarques::lire_fichier(&octets)?.exiger_la_forme_2().map(|_| ExitCode::SUCCESS)
}

/// Le refus 6038 : le rapport, ou le plan d'un constat, que le lecteur refuse. La preuve est ce que le lecteur a vu.
fn rapport_refuse(preuve: impl Into<String>) -> Refus {
    let correction = "Give lydite findings the report that `hermes security-review export` wrote (a regular file of \
                      4 MiB at most), and, for a plan, the id of a finding that its list keeps with --verify \
                      \"<proof>\", then run again.";
    Refus::new(Code::RapportRefuse, preuve, correction)
}

/// Les octets du rapport : un fichier ordinaire, jamais un lien suivi, de [`constats::OCTETS_MAXIMUM`] octets au
/// plus, pesé avant d'être lu. Un fichier qu'on ne lit pas est le refus 6006 ; un lien, ce qui n'est pas un fichier
/// ordinaire, et un rapport trop gros sont le refus 6038.
fn lire_rapport(fichier: &Path) -> Result<Vec<u8>, Refus> {
    use std::io::Read;
    let illisible = |e: std::io::Error| {
        let preuve = format!("reading the report {}: {e}", fichier.display());
        let correction = "Give lydite findings the path of a file that Lydite can read (a relative path starts from \
                          the current folder), then run again.";
        Refus::new(Code::EntreeSortie, preuve, correction)
    };
    let ici = fichier.display();
    let vu = std::fs::symlink_metadata(fichier).map_err(illisible)?;
    if vu.file_type().is_symlink() {
        return Err(rapport_refuse(format!("the report {ici} is a symbolic link, which is not followed")));
    }
    if !vu.is_file() {
        return Err(rapport_refuse(format!("the report {ici} is not a regular file")));
    }
    if vu.len() > constats::OCTETS_MAXIMUM as u64 {
        return Err(rapport_refuse(constats::rapport_trop_gros()));
    }
    // Lu borné, un octet de plus pour voir qu'il dépasse : le lecteur refuse ce qui a grossi depuis la pesée.
    let mut octets = Vec::new();
    let borne = constats::OCTETS_MAXIMUM as u64 + 1;
    std::fs::File::open(fichier).and_then(|ouvert| ouvert.take(borne).read_to_end(&mut octets)).map_err(illisible)?;
    Ok(octets)
}

/// La liste d'un rapport lu, en un objet JSON sur une ligne, dans l'ordre du lecteur : les constats gardés (`id`,
/// `kind`, `state`, `severity`, `title`, `where`) et ceux qu'il écarte (`ref`, `why`). `json::ecrire` n'écrit que des
/// objets plats : chaque chaîne, un titre venu du rapport compris, passe par `json::chaine`.
fn liste_des_constats(rapport: &Rapport) -> String {
    let constats: Vec<String> = rapport
        .constats
        .iter()
        .map(|constat| {
            let lieux: Vec<String> = constat.lieux.iter().map(|lieu| json::chaine(lieu)).collect();
            format!(
                r#"{{"id":{},"kind":{},"state":{},"severity":{},"title":{},"where":[{}]}}"#,
                json::chaine(&constat.id),
                json::chaine(constat.sorte.nom()),
                json::chaine(&constat.etat),
                json::chaine(&constat.gravite),
                json::chaine(&constat.titre),
                lieux.join(",")
            )
        })
        .collect();
    let ecartes: Vec<String> = rapport
        .ecartes
        .iter()
        .map(|ecarte| {
            format!(r#"{{"ref":{},"why":{}}}"#, json::chaine(&ecarte.reference), json::chaine(&ecarte.pourquoi))
        })
        .collect();
    format!(
        r#"{{"scan":{},"findings":[{}],"set_aside":[{}]}}"#,
        json::chaine(&rapport.scan),
        constats.join(","),
        ecartes.join(",")
    )
}

/// Ce que `lydite findings` écrit sur stdout : la liste du rapport et sa fin de ligne, ou, avec l'identité d'un constat
/// et la preuve, le texte de `constats::plan_de`, tel quel. Une identité mal formée, absente du rapport ou écartée est
/// refusée en disant laquelle des trois.
fn texte_des_constats(fichier: &Path, plan: Option<(&str, &str)>) -> Result<String, Refus> {
    let rapport = constats::lire(&lire_rapport(fichier)?).map_err(rapport_refuse)?;
    let Some((identite, preuve)) = plan else {
        return Ok(liste_des_constats(&rapport) + "\n");
    };
    if !["hsf_", "cand_"].iter().any(|prefixe| constats::identite_de_forme(identite, prefixe)) {
        let vu: String = identite.chars().take(80).collect();
        return Err(rapport_refuse(format!(
            "the finding id {vu:?} is not well-formed: it is `hsf_` or `cand_` and 24 lowercase hexadecimal digits"
        )));
    }
    if let Some(constat) = rapport.constats.iter().find(|constat| constat.id == identite) {
        return constats::plan_de(constat, &rapport.scan, preuve).map_err(rapport_refuse);
    }
    Err(rapport_refuse(match rapport.ecartes.iter().find(|ecarte| ecarte.reference == identite) {
        Some(ecarte) => format!("the finding {identite} is set aside by the reader: {}", ecarte.pourquoi),
        None => format!("the finding {identite} is not in the report"),
    }))
}

/// `lydite findings <report> [--plan <id> --verify "<proof>"]` : lit le rapport comme le lecteur le lit et écrit sur
/// stdout sa liste, ou le plan du constat demandé, et rien d'autre.
fn constats_cmd(fichier: &Path, plan: Option<(&str, &str)>) -> Result<ExitCode, Refus> {
    use std::io::Write;
    let texte = texte_des_constats(fichier, plan)?;
    let mut sortie = std::io::stdout().lock();
    sortie.write_all(texte.as_bytes()).and_then(|()| sortie.flush())?;
    Ok(ExitCode::SUCCESS)
}

/// `lydite lecons <dossier>` : lit chaque `.agents/skills/*/SKILL.md` du dépôt de ce dossier, dans l'ordre des noms, et
/// ne dit rien si toutes sont justes. Sinon, le refus de la première fausse, avec son fichier et sa ligne. Un dépôt
/// sans `.agents/skills`, ou un sous-dossier sans `SKILL.md`, n'a aucune leçon à lire.
fn lecons_cmd(depot: &Path) -> Result<ExitCode, Refus> {
    use std::io::Read;
    let illisible = |chemin: &Path, e: std::io::Error| {
        let preuve = format!("reading the lessons {}: {e}", chemin.display());
        let correction = "Give lydite lessons the folder of a repository whose `.agents/skills` can be read (a \
                          relative path starts from the current folder), then run again.";
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

/// La skill de pilotage, liée au binaire : le fichier du plugin Hermes reste sa seule source, lu à ce chemin.
const SKILL: &str = include_str!("../hermes/skills/driving-lydite/SKILL.md");

/// `lydite skill` : écrit sur stdout la skill de ce binaire, telle quelle. Elle ne lit ni profil, ni dossier de
/// Lydite, ni dépôt, et n'écrit rien d'autre : elle marche partout.
fn skill_cmd() -> Result<ExitCode, Refus> {
    use std::io::Write;
    let mut sortie = std::io::stdout().lock();
    sortie.write_all(SKILL.as_bytes()).and_then(|()| sortie.flush())?;
    Ok(ExitCode::SUCCESS)
}

/// `lydite --help` : écrit sur stdout l'usage, une forme par ligne. Elle ne lit ni profil, ni dossier de Lydite, ni
/// dépôt, et n'écrit rien d'autre.
fn aide_cmd() -> Result<ExitCode, Refus> {
    use std::io::Write;
    let mut sortie = std::io::stdout().lock();
    sortie.write_all(ligne::aide().as_bytes()).and_then(|()| sortie.flush())?;
    Ok(ExitCode::SUCCESS)
}

/// `lydite accepter <nom>` : lit et valide le profil du dossier de Kraken, écrit l'empreinte qu'il accepte, puis dit à
/// l'humain ce qu'il vient d'accepter : le texte du profil et son empreinte.
fn accepter_cmd(nom: &str) -> Result<ExitCode, Refus> {
    let (texte, empreinte) = dossier::accepter(nom, &dossier::dossier()?)?;
    println!("{}\nfingerprint: {empreinte}", texte.trim_end());
    Ok(ExitCode::SUCCESS)
}

/// `lydite profil <nom>` : charge le profil comme `lancer` (le fichier ou l'intégré, puis l'empreinte acceptée) et
/// écrit sur stdout ce que la main recevra, une rangée par ligne. Rien d'autre n'est lu ni écrit.
fn profil_cmd(nom: &str) -> Result<ExitCode, Refus> {
    let profil = dossier::charger_dans(nom, &dossier::dossier()?)?;
    profil.garder_selon(dossier::lire_empreinte)?;
    for rangee in profil.rangees() {
        println!("{rangee}");
    }
    Ok(ExitCode::SUCCESS)
}

fn etat_cmd() -> Result<ExitCode, Refus> {
    etat::etat_dans(&dossier::dossier()?)
}

/// `lydite attendre` : attend la fin du lancement qui tient le dossier, puis dit la dernière vague comme `etat`.
fn attendre_cmd() -> Result<ExitCode, Refus> {
    etat::attendre_dans(&dossier::dossier()?, etat::DELAI_D_OUVERTURE, etat::PAUSE_DE_L_ATTENTE)
}

/// `lydite --version` : dit la version du binaire, lue de `CARGO_PKG_VERSION` du `Cargo.toml`, sur stdout.
fn version_cmd() -> Result<ExitCode, Refus> {
    println!("lydite {}", env!("CARGO_PKG_VERSION"));
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests;
