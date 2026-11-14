//! L'ordonnancement d'une vague : `lancer` mène les tâches d'un plan, chacune par une main dans un bac à elle, et rend
//! l'issue de chacune. Les indépendantes partent ensemble, chacune dans son bac posé sur la base ; une dépendante part
//! de la branche de la vague dès que les siennes y sont assemblées. Des copies (`Plan::copies`, le `--n` de `lancer`)
//! sont un plan sans preuve de vague : rien ne s'y assemble.
//!
//! Un seul fil d'exécution, la vague, note au fil, pose les bacs et assemble. Chaque main joue sa tentative dans un fil
//! d'exécution à elle et rend un seul message, que la vague reçoit dans l'ordre d'arrivée. Trois règles tiennent chaque
//! main, relance comprise, par ses baux (`bail.rs`) : un résultat périmé est refusé, une tâche n'a jamais deux mains
//! actives, une main jamais conclue expire seule. Une main tuée à son délai n'est pas relancée.
//!
//! Une coupure de l'API (`Echec::Coupure`) n'est pas un échec de la main : la tâche repart une fois, après [`PAUSE`],
//! sans que cette tentative compte comme une relance (la relance d'une main rouge lui reste). La pause se passe dans
//! le fil d'exécution de la main qui repart, pas dans la vague, et le bail de cette tentative la couvre. Une seconde
//! coupure de la même tâche la classe, la cause dite (« API outage (<statut>) »). Le fil la note après la fin de
//! la tentative coupée (`main.coupure`).
//!
//! Au plus [`MAINS_A_LA_FOIS`] mains tournent ensemble, copies de `--n` comprises : les tâches prêtes qui restent
//! attendent qu'une main finisse, et partent dans l'ordre du plan. Chaque tâche a son profil, celui de la vague aux
//! modèles de la tâche (`modele` et `relance` du plan, à défaut ceux du profil) et à son effort (`effort` du plan, à
//! défaut ceux du profil), que reçoivent sa main et son `main.lancement`. Avant toute main, un `effort` de tâche que le
//! profil ne permet pas (`efforts_permis`) refuse la vague en 6033. Une tâche qui a des `fichiers` ne reste prise que
//! si sa main n'a changé que ces chemins (7013), à la fin de sa tentative comme au gel. Un fichier que le plan épingle
//! par `sha256` n'est tenu que si git le garde sous cette empreinte, à la fin de la tentative (elle est rouge sinon)
//! puis au gel (7022) ; la consigne de la main les lui dit.
//!
//! Une vague de découpage, de relecture ou de correction porte de plus ses [`Liens`] : le juge de sa main, ce que
//! Kraken pose dans le bac de sa main avant qu'elle parte (le `DIFF.lydite` du relecteur), le découpage dont elle joue
//! le plan, la vague qu'elle corrige, et celle que relit son relecteur.

use std::collections::HashMap;
use std::iter;
use std::mem;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, Scope};
use std::time::{Duration, Instant};

use crate::branchements::bac::{bac, exclusions_de_l_etat, poser, reglages_du_bac, retirer_de_force, sans_claude};
use crate::branchements::dossier::{Fil, depense_du_jour, ecrire_consigne, lire_quota, retenir_la_vague, tenir};
use crate::branchements::git::{Arbre, BORNE_EMPREINTE, Gardee, empreinte_gardee, entree_de_l_index};
use crate::branchements::git::{faire, geste, lecons_du_commit};
use crate::branchements::srt::{FinDuRejeu, rejouer_et_finir};
use crate::coeur::bail::Baux;
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::{Base, Couleur, Evenement, Foi, IssueVague, LancementVague, Verdict};
use crate::coeur::fil::{Role, instant, lot, maintenant, refus_en_clair};
use crate::coeur::lecons::{self, Lecon};
use crate::coeur::plan::{Plan, Rouge, Tache, couvre, hors_forme};
use crate::coeur::profil::{Prix, Profil, empreinte};
use crate::coeur::protocole::{Constat, Jetons, Usage, UsageModele};
use crate::coeur::texte::en_ligne;
use crate::conduite::assemblage::{self, Assemblage};
use crate::conduite::tentative::{Echec, IMPOSSIBLE, Juge, Pris, consigne_de, declare_rouge, tenter};

/// Ce que devient chaque tâche d'une vague, dans l'ordre du plan : l'id de sa main, `main-<vague>-<k>`, qui nomme aussi
/// son bac, et son issue.
pub(crate) type Issues = Vec<(String, Result<Pris, String>)>;

/// Ce que Kraken pose dans le bac d'une main avant qu'elle parte, une fois le bac fait : le `DIFF.lydite` du relecteur
/// (`relecture.rs`). Un refus retient la main : sa tâche est refusée, sans bail ni départ.
pub(crate) type Preparation<'a> = &'a dyn Fn(&Path) -> Result<(), Refus>;

/// Ce que le plan dit pour un fichier qu'il pose, et le contenu que Lydite en a lu : les octets du fichier de même
/// chemin sous `<fichier du plan>.files/`, tels quels, dont l'empreinte est celle du plan.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Posee {
    /// L'id de la tâche dont une ligne `place` dit ce fichier.
    pub(crate) tache: String,
    /// Le chemin du fichier, tel que le plan l'écrit : relatif à la racine du dépôt, des noms séparés par `/`.
    pub(crate) fichier: String,
    pub(crate) octets: Vec<u8>,
}

/// Ce qu'une vague de découpage, de relecture ou de correction porte de plus qu'une vague à plan ; par défaut, rien.
#[derive(Clone, Copy, Default)]
pub(crate) struct Liens<'a> {
    /// Le contenu des fichiers que le plan pose (`lire_plan`), tel que la ligne de commande l'a lu : rien pour toute
    /// vague que Lydite bâtit (correction, relance, découpage, relecteur), dont le plan ne pose aucun fichier.
    pub(crate) poses: &'a [Posee],
    /// Le juge de ses mains : celui du découpeur ou du relecteur, dont la preuve prêtée est une commande de Kraken
    /// ([`Juge`]). Sa vague, des copies, ne note pas cette commande en `verifier` : Kraken ne la rejoue pas.
    pub(crate) juge: Option<Juge<'a>>,
    /// Le découpage dont elle joue le plan : son `vague.joue` se note à son ouverture, avant tout départ.
    pub(crate) decoupage: Option<&'a str>,
    /// La vague qu'elle corrige : son `vague.correction` se note à son ouverture, avant tout départ.
    pub(crate) corrige: Option<&'a str>,
    /// La vague que relit son relecteur : son `vague.relecteur` se note à son ouverture, avant tout départ.
    pub(crate) relit: Option<&'a str>,
    /// Ce qui se pose dans le bac de chacune de ses mains avant sa première tentative ([`Preparation`]).
    pub(crate) preparation: Option<Preparation<'a>>,
    /// Le numéro de l'issue dont elle joue le plan : son `vague.issue` se note à son ouverture, après ses autres notes
    /// d'ouverture et avant tout départ.
    pub(crate) issue: Option<u64>,
    /// L'empreinte (SHA-256, 64 hexadécimaux minuscules) des octets du fichier que `--plan` a donné à cette vague : son
    /// `vague.lancement` la porte ; `None` pour toute autre vague.
    pub(crate) plan: Option<&'a str>,
    /// Vrai pour la vague de relance de la CI, qui a son propre contrôle (« n'a rien changé à la tête poussée ») :
    /// ses mains ne passent pas par `main_qui_change`.
    pub(crate) relance_ci: bool,
    /// Les leçons du commit de base de la vague dont elle est la suite (relecture, correction, relance de la CI) : elle
    /// les reçoit telles quelles et ne lit rien au commit d'où elle part, qu'un travail de mains a pu faire.
    pub(crate) lecons: Option<&'a [Lecon]>,
    /// La pause avant qu'une tâche reparte sur une coupure de l'API ; [`PAUSE`] sans elle. Les tests la mettent à zéro.
    pub(crate) pause: Option<Duration>,
}

/// Ce que rend une vague partie d'une base donnée ([`lancer_sur`]) : ce que rend [`lancer`], et de quoi la pousser.
pub(crate) struct Menee {
    /// L'id de la vague, le lot dont sont faits les ids de ses mains et de ses bacs.
    pub(crate) id: String,
    /// La branche de la vague, `None` pour des copies.
    pub(crate) branche: Option<String>,
    /// La tête de son assemblage quand la vague finit : sa base, ou sa dernière fusion notée ; `None` pour des copies.
    pub(crate) tete: Option<String>,
    /// La vague s'est arrêtée sur le refus 7012.
    pub(crate) arretee: bool,
    pub(crate) issues: Issues,
    /// Les leçons lues une seule fois, au commit de base de la vague ; ses suites les reçoivent par [`Liens::lecons`].
    pub(crate) lecons: Vec<Lecon>,
}

/// Ce qu'une tentative rend, tel que `tenter` le rend : le constat de la main (`None` pour une main brute, qui ne
/// déclare rien), la fin de la sortie de sa preuve (`None` si elle n'a pas tourné), et son verdict ; puis le
/// `main.rouge` que Lydite a constaté à la fin d'une tentative verte de tâche à `red` (`None` sans constat). Seule la
/// vague note.
type Rendu = (Option<Constat>, Option<String>, Result<Pris, Echec>, Option<Evenement>);

/// Les mains qui tournent en même temps, au plus, copies de `--n` comprises ; les autres attendent leur tour, dans
/// l'ordre du plan. Une relance garde la place de la tentative qu'elle suit : elle part quand le message de celle-ci
/// est reçu, et ne compte jamais en plus.
// raccourci : quatre mains à la fois pour la mémoire d'un PC, monter vers ce que le nœud sait de sa machine
const MAINS_A_LA_FOIS: usize = 4;

/// Le plus long que la vague attend un message avant de regarder les échéances de ses baux.
const ATTENTE: Duration = Duration::from_millis(50);

/// Ce que le bail d'une main laisse en plus du délai de la main et de celui de sa preuve.
const MARGE: Duration = Duration::from_secs(60);

/// La pause d'une tâche avant qu'elle reparte sur une coupure de l'API (une seule fois par tâche). Elle se passe dans
/// le fil d'exécution de la main qui repart, pas dans la vague, et le bail de cette tentative la couvre.
const PAUSE: Duration = Duration::from_secs(60);

/// La correction du refus 7012, qui arrête la vague.
const ARRET: &str = "The wave stops: no more notes, starts or merges, and its running hands finish. Make the thread \
                     writable, or the settings that the proof names readable, then run again in a fresh wave the \
                     tasks that are not assembled.";

/// La durée du bail de chaque main de la vague : le délai de la main, quatre délais de preuve (la preuve et le rouge,
/// chacun rejoué une fois s'il est tué), et une minute de marge. Une tentative conclut donc avant son échéance, qui ne
/// tombe que sur une main qui n'a rien rendu.
// raccourci : toute tâche reçoit le bail du rouge, même sans clé red ; monter (un bail par tâche) si une vague longue
// en souffre
pub(crate) fn duree_du_bail(profil: &Profil) -> Duration {
    profil.delai_main() + 4 * profil.delai_preuve() + MARGE
}

/// La base d'une vague sans `--pr` : le `HEAD` du dépôt visé, en sha. Un dépôt qui n'est pas un dossier est le refus
/// 6002 ; un dépôt où git ne lit pas de commit, d'où partiraient les bacs, le refus 6003.
pub(crate) fn base_du_depot(depot: &Path) -> Result<String, Refus> {
    if !depot.is_dir() {
        return Err(Refus::new(
            Code::DepotIntrouvable,
            format!("{} is not a folder that exists", depot.display()),
            "Run Lydite from the target repository, or give it with --repo <path>.",
        ));
    }
    let base = faire(geste(depot, &["rev-parse", "--verify", "HEAD"])).map_err(|vu| {
        Refus::new(
            Code::BacImpossible,
            vu,
            "Target a git repository that has at least one commit (--repo <path>), then run again.",
        )
    })?;
    Ok(base.trim().to_string())
}

/// Ce que la base garde à chaque chemin que le plan pose, lu par git depuis la racine du dépôt visé (`empreinte_gardee`
/// sur le commit `base`) : rien, ou un fichier ordinaire, que la pose remplace. Un lien symbolique, un dossier, un
/// sous-dépôt, un fichier trop lourd, ou une lecture de git qui rate sont le refus 6036, avant tout bac et tout fil.
pub(crate) fn poses_posables(depot: &Path, base: &str, poses: &[Posee]) -> Result<(), Refus> {
    for posee in poses {
        let fichier = en_ligne(&posee.fichier);
        let vu = match empreinte_gardee(depot, Arbre::Commit(base), &posee.fichier, BORNE_EMPREINTE) {
            Ok(Gardee::Absent | Gardee::Empreinte(_)) => continue,
            Ok(Gardee::PasOrdinaire(mode)) => format!("it keeps mode {mode} there, which is not an ordinary file"),
            Ok(Gardee::TropLourd) => format!("it keeps a file of more than {BORNE_EMPREINTE} bytes there"),
            Err(vu) => format!("git does not say what it keeps there: {}", en_ligne(&vu)),
        };
        let preuve = format!("the file `{fichier}` that the plan places: the base commit {base}: {vu}");
        let correction = "Place no file where the base commit keeps a symbolic link, a folder, a submodule or a very \
                          large file (an ordinary file there is replaced by the placed one), or run the plan on \
                          another base.";
        return Err(Refus::new(Code::FichierPoseRefuse, preuve, correction));
    }
    Ok(())
}

/// Pose `octets` au chemin `fichier` du bac, tels quels : d'abord dans le fichier temporaire `temporaire`, hors du
/// bac ; puis `git hash-object -w --no-filters` en fait l'objet de git (sans conversion de fins de ligne ni filtre du
/// dépôt), `update-index --add --cacheinfo` l'inscrit en 100644 et `checkout-index -f` l'écrit : git n'écrit jamais à
/// travers un lien symbolique. Le temporaire est effacé dans tous les cas. Rend ce qui a raté.
fn poser_un_fichier(bac: &Path, temporaire: &Path, fichier: &str, octets: &[u8]) -> Result<(), String> {
    let pose = std::fs::write(temporaire, octets)
        .map_err(|e| format!("its bytes are not written to {}: {e}", temporaire.display()))
        .and_then(|()| {
            let mut hacher = geste(bac, &["hash-object", "-w", "--no-filters"]);
            hacher.arg(temporaire);
            let objet = faire(hacher)?;
            let inscrire = ["update-index", "--add", "--cacheinfo", &format!("100644,{},{fichier}", objet.trim())];
            faire(geste(bac, &inscrire))?;
            faire(geste(bac, &["checkout-index", "-f", "--", fichier])).map(|_| ())
        });
    if let Err(vu) = std::fs::remove_file(temporaire)
        && vu.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!("lydite: cannot remove {}: {vu}", temporaire.display());
    }
    pose
}

/// [`lancer_sur`], avec les [`Liens`] de la vague : son `vague.joue` suit son `vague.assemblage` s'il nomme un
/// découpage, et le juge, s'il y en a un, rend le verdict de chaque tentative ([`tenter`]).
pub(crate) fn lancer_avec(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    plan: &Plan,
    bail: Duration,
    base: &str,
    liens: Liens<'_>,
) -> Result<Menee, Refus> {
    // Un effort de tâche que le profil ne permet pas : le refus 6033, avant tout le reste, et rien ne part.
    efforts_permis(profil, plan)?;
    // Les leçons du commit de base, lues par git avant toute main : une leçon fausse refuse la vague. Une suite de
    // vague reçoit celles de la vague première, et ne lit pas son commit de départ.
    let lecons = match liens.lecons {
        Some(donnees) => donnees.to_vec(),
        None => lecons_du_commit(depot, base)?,
    };
    // Le dossier se crée ici, et le lancement le tient pour lui seul (6032) ; ses autres vagues le trouvent tenu.
    tenir(dossier)?;
    // Une vague par lancement, nommée par son lot : les ids de ses mains et de ses bacs en sont faits.
    let id = lot();
    let assemblage = match &plan.preuve {
        Some(preuve) => Some(assemblage::ouvrir(depot, dossier, profil, &id, base, preuve)?),
        None => None,
    };
    let fil = Fil::seance(dossier, "humain");
    let notes = ouverture(&id, depot, plan, base, assemblage.as_ref(), liens);
    let notees = notes
        .iter()
        .try_for_each(|note| fil.noter(note))
        .and_then(|()| match liens.issue {
            Some(issue) => fil.noter_issue(&IssueVague { vague: id.clone(), issue }),
            None => Ok(()),
        })
        .and_then(|()| {
            let version = env!("CARGO_PKG_VERSION").to_string();
            fil.noter_lancement(&LancementVague { vague: id.clone(), version, plan: liens.plan.map(str::to_string) })
        });
    if notees.is_ok() {
        // Le processus retient sa dernière vague ouverte : sa sortie en note la fin.
        retenir_la_vague(dossier, &id);
    }
    if let Err(erreur) = notees {
        let mut refus = Refus::from(erreur);
        if let Some(Err(echec)) = assemblage.map(Assemblage::retirer) {
            refus.preuve = format!("{}\nthen {echec}", refus.preuve);
        }
        return Err(refus);
    }
    let branche = assemblage.as_ref().map(|assemblage| assemblage.branche().to_string());
    let mut vague = Vague {
        id,
        base: base.into(),
        depot,
        profil,
        plan,
        duree: bail,
        fil,
        assemblage,
        juge: liens.juge,
        lecons,
        corrige: liens.corrige.is_some() || liens.relance_ci,
        role: match (liens.relance_ci, liens.corrige, liens.juge, liens.relit) {
            (true, ..) => Role::Relance,
            (_, Some(_), ..) => Role::Correction,
            (_, _, Some(_), Some(_)) => Role::Relecteur,
            (_, _, Some(_), None) => Role::Decoupeur,
            _ => Role::Main,
        },
        preparation: liens.preparation,
        poses: liens.poses,
        baux: Baux::new(),
        rangs: (0..).zip(&plan.taches).map(|(rang, tache)| (tache.id.as_str(), rang)).collect(),
        taches: plan.taches.iter().map(|_| Etat::Attente).collect(),
        pause: liens.pause.unwrap_or(PAUSE),
        coupures: plan.taches.iter().map(|_| 0).collect(),
        precedentes: plan.taches.iter().map(|_| None).collect(),
        en_vol: Vec::new(),
        arret: None,
    };
    let issues = thread::scope(|s| vague.mener(s));
    let tete = vague.assemblage.as_ref().map(|assemblage| assemblage.tete().to_string());
    Ok(Menee { id: vague.id, branche, tete, arretee: vague.arret.is_some(), issues, lecons: vague.lecons })
}

/// Chaque `effort` de tâche du plan est un effort que le profil permet (`efforts_permis`) ; sinon le refus 6033, qui
/// nomme la première tâche et son effort.
fn efforts_permis(profil: &Profil, plan: &Plan) -> Result<(), Refus> {
    for tache in &plan.taches {
        let Some(effort) = tache.effort.as_deref().filter(|effort| !profil.permet_l_effort(effort)) else { continue };
        return Err(Refus::new(
            Code::EffortNonPermis,
            format!("task {} asks for effort {effort}, which the profile's `efforts_permis` does not allow", tache.id),
            "Choose, in the plan, on the `effort` line, an effort that the profile allows, or widen `efforts_permis` \
             in the profile and accept it again: lydite accept <profile>.",
        ));
    }
    Ok(())
}

/// Les notes qui ouvrent la vague : `vague.ouverte`, puis, pour un plan, `vague.assemblage`, son `vague.joue` s'il joue
/// le plan d'un découpage, son `vague.correction` s'il corrige une vague, et un `vague.tache` par tâche, avant tout
/// départ. Son `vague.relecteur`, si son relecteur relit une vague, suit l'assemblage d'un plan, ou l'ouverture de
/// copies. Des copies portent dans `verifier` leur preuve commune, sauf celles d'un juge, dont la commande prêtée n'est
/// pas rejouée ; un plan, `null`.
fn ouverture(
    vague: &str,
    depot: &Path,
    plan: &Plan,
    base: &str,
    assemblage: Option<&Assemblage<'_>>,
    liens: Liens<'_>,
) -> Vec<Evenement> {
    let verifier = match (&plan.preuve, liens.juge) {
        (None, None) => plan.taches.first().and_then(|tache| tache.preuve.clone()),
        _ => None,
    };
    let mut notes = vec![Evenement::Vague {
        vague: vague.into(),
        n: plan.taches.len(),
        depot: depot.to_string_lossy().into_owned(),
        verifier,
        objectif: plan.objectif.clone(),
    }];
    let relit = |relue: &str| Evenement::Relecteur { vague: vague.into(), relue: relue.into() };
    if let (Some(assemblage), Some(preuve)) = (assemblage, &plan.preuve) {
        let branche = assemblage.branche().into();
        notes.push(Evenement::Assemblage { vague: vague.into(), branche, base: base.into(), preuve: preuve.clone() });
        let joue = |decoupage: &str| Evenement::Joue { vague: vague.into(), decoupage: decoupage.into() };
        notes.extend(liens.decoupage.map(joue));
        let corrige = |corrigee: &str| Evenement::Correction { vague: vague.into(), corrige: corrigee.into() };
        notes.extend(liens.corrige.map(corrige));
        notes.extend(liens.relit.map(relit));
        notes.extend((1..).zip(&plan.taches).map(|(main, tache)| Evenement::Tache {
            vague: vague.into(),
            main,
            id: tache.id.clone(),
            depend: tache.depend.clone(),
            preuve: tache.preuve.clone().unwrap_or_else(|| preuve.clone()),
            objectif: tache.objectif.clone(),
        }));
    } else {
        notes.extend(liens.relit.map(relit));
    }
    notes
}

/// Joue la main et rend ce qu'elle rend ; une main qui panique rend une fin rouge qui le dit. La panique ne fait pas
/// tomber la vague, et le message de la main arrive quand même.
fn en_message(main: impl FnOnce() -> Rendu) -> Rendu {
    panic::catch_unwind(AssertUnwindSafe(main)).unwrap_or_else(|panique| {
        let quoi = panique
            .downcast_ref::<&str>()
            .map(|quoi| (*quoi).to_string())
            .or_else(|| panique.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "no message".into());
        (None, None, Err(Echec::Erreur(format!("the hand panicked: {quoi}"))), None)
    })
}

/// Le refus dit en clair, le texte que `lydite etat` tire de sa note `vague.refus`.
fn en_clair(refus: &Refus) -> String {
    refus_en_clair(refus.code.numero(), refus.code.regle(), &refus.preuve, &refus.correction)
}

/// Le refus dit comme l'erreur d'une tentative qu'il rougit : son code et sa règle, puis sa preuve, puis sa correction,
/// chacune sur sa ligne. C'est ce que la relance reçoit en contexte. Elle ne commence pas par `refusée (`, le début du
/// refus d'une tâche (`en_clair`) : une tâche classée sur cette erreur reste une classée, au fil comme à l'écran. Le
/// juge du découpeur rend ainsi ses refus.
pub(crate) fn en_erreur(refus: &Refus) -> String {
    format!("{} : {}\n{}\n{}", refus.code, refus.code.regle(), refus.preuve, refus.correction)
}

/// Le travail pris d'une tâche qui a des `fichiers` n'est pris que si sa main n'a changé que ces chemins ; sinon sa
/// tentative est rouge (7013), et son erreur dit les chemins en trop et la règle, que la relance reçoit. Un git qui ne
/// dit pas ce que la main a changé la rougit de même (7009) : rien ne prouve alors qu'elle est restée dans ses
/// fichiers. Une tâche sans `fichiers` n'est bornée que par `proteges` : un chemin protégé changé, `fichiers` ou non,
/// rougit la tentative (7018), avant le jugement des `fichiers`.
fn dans_ses_fichiers(bac: &Path, depart: &str, tache: &Tache, proteges: &[String], pris: Pris) -> Result<Pris, String> {
    assemblage::proteges_tenus(bac, depart, &tache.id, proteges).map_err(|refus| en_erreur(&refus))?;
    let Some(fichiers) = &tache.fichiers else { return Ok(pris) };
    assemblage::fichiers_tenus(bac, depart, &tache.id, fichiers, proteges)
        .map(|()| pris)
        .map_err(|refus| en_erreur(&refus))
}

/// Une main de plan verte qui n'a changé aucun chemin depuis `depart` n'a rien fait : sa tentative est rouge, et son
/// erreur le dit, que la relance reçoit. Un git qui ne dit pas ce qu'elle a changé ne la rougit pas ici : son refus
/// 7009 reste celui d'aujourd'hui.
fn main_qui_change(bac: &Path, depart: &str, proteges: &[String]) -> Result<(), String> {
    match assemblage::chemins_changes(bac, depart, proteges) {
        Ok(chemins) if chemins.is_empty() => Err("the hand changed nothing".to_string()),
        Ok(_) => Ok(()),
        // Le refus 7009 vient plus loin, à l'assemblage : ici, la main n'est pas jugée sur ce que git ne dit pas.
        Err(_) => Ok(()),
    }
}

/// Chaque texte que le plan exige (`exact`) paraît, octet pour octet, dans son fichier du bac ; sinon la tentative est
/// rouge, et son erreur nomme le fichier et le texte, que la relance reçoit. Un fichier absent, ou illisible, ne le
/// contient pas.
fn textes_exacts(bac: &Path, tache: &Tache, pris: Pris) -> Result<Pris, String> {
    for (fichier, texte) in &tache.exacts {
        let present = std::fs::read(bac.join(fichier)).is_ok_and(|octets| {
            let aiguille = texte.as_bytes();
            octets.windows(aiguille.len().max(1)).any(|fenetre| fenetre == aiguille)
        });
        if !present {
            return Err(format!("the text that the plan requires is missing from {fichier}: {texte}"));
        }
    }
    Ok(pris)
}

/// Ce que le constat du rouge d'une tentative prend de sa vague et de son bac.
struct Constatation<'a> {
    profil: &'a Profil,
    /// Le dépôt visé, où se pose le bac du rouge.
    depot: &'a Path,
    /// Le dossier de Lydite, où vivent les bacs.
    dossier: &'a Path,
    /// L'id de la main, `main-<vague>-<k>`, que le nom du bac du rouge reprend.
    main: &'a str,
    tentative: usize,
    /// Le bac de la main, parti de `depart`.
    bac: &'a Path,
    depart: &'a str,
    tache: &'a Tache,
    /// La preuve de la tâche, que le rouge rejoue.
    commande: &'a str,
    /// Le journal de la sortie du rejeu sur la base.
    journal: &'a Path,
}

/// Les fichiers du test d'une tâche à `red`, dans l'ordre : ceux du plan, ou ceux que la main nomme dans sa `reponse`
/// (`RED: <file>`, voir [`declare_rouge`]). Sans aucune ligne `RED:`, la tentative rougit ; un fichier déclaré qui n'a
/// pas la forme d'un chemin de `fichiers` ([`hors_forme`]), que les `fichiers` de la tâche ne couvrent pas, ou que la
/// main n'a pas changé depuis le départ ([`assemblage::chemins_changes`]) la rougit, en le nommant.
fn fichiers_du_test(jeu: &Constatation<'_>, rouge: &Rouge, reponse: &str) -> Result<Vec<String>, String> {
    let declares = match rouge {
        Rouge::Nommes(fichiers) => return Ok(fichiers.clone()),
        Rouge::Declare => declare_rouge(reponse),
    };
    if declares.is_empty() {
        return Err("the plan asks red first, and the hand named no test file (`RED: <file>`)".to_string());
    }
    let changes = assemblage::chemins_changes(jeu.bac, jeu.depart, jeu.profil.proteges()).map_err(|refus| {
        format!("git does not say what the hand changed, to check its test files: {}", refus.preuve)
    })?;
    for fichier in &declares {
        let nom = en_ligne(fichier);
        if hors_forme(fichier).is_some() {
            return Err(format!(
                "the test file {nom} that the hand names (`RED: <file>`) is not a file path as `fichiers` writes it"
            ));
        }
        let couvert = jeu.tache.fichiers.as_ref().is_none_or(|fichiers| fichiers.iter().any(|e| couvre(e, fichier)));
        if !couvert {
            return Err(format!(
                "the test file {nom} that the hand names (`RED: <file>`) is outside the `fichiers` of the task"
            ));
        }
        if !changes.iter().any(|change| change == fichier) {
            return Err(format!(
                "the test file {nom} that the hand names (`RED: <file>`) is not a file that the hand changed"
            ));
        }
    }
    Ok(declares.into_iter().map(str::to_string).collect())
}

/// Constate le rouge d'une tentative verte de tâche à `red` : la preuve de la tâche, rejouée sur la base plus les seuls
/// fichiers du test, finit rouge. Pour chaque fichier du test : `git add` dans le bac de la main, son entrée de
/// l'index (un mode 100644 ou 100755), son empreinte. Puis un bac neuf, `<main>-rouge-<tentative>`, posé détaché sur
/// `depart`, où chaque fichier entre par `git update-index --add --cacheinfo` et `git checkout-index -f` : seuls les
/// octets que git garde du bac de la main. La preuve s'y rejoue ([`rejouer_et_finir`], le même mur et le même délai),
/// puis le bac du rouge se retire de force, que le rejeu ait réussi ou non ; un retrait raté se dit sur la sortie
/// d'erreur et ne change pas le verdict.
///
/// Rend les `<fichier> <empreinte>` du test et la fin de la sortie du rejeu rouge. Toute autre issue est une erreur,
/// dite en anglais, que la relance reçoit : un test qui passe sur la base (`FinDuRejeu::Verte`), ou un rejeu qui n'a
/// pas fini rouge de lui-même (délai, preuve tuée, rejeu impossible), qui n'est pas le rouge d'un test.
fn constater_le_rouge(jeu: &Constatation<'_>, rouge: &Rouge, reponse: &str) -> Result<(Vec<String>, String), String> {
    let fichiers = fichiers_du_test(jeu, rouge, reponse)?;
    let mut entrees = Vec::new();
    for fichier in &fichiers {
        let nom = en_ligne(fichier);
        let mut ajout = geste(jeu.bac, &["add", "--", &format!(":(literal){fichier}")]);
        ajout.args(exclusions_de_l_etat());
        faire(ajout).map_err(|vu| format!("red first: the test file {nom} is not added by git: {}", en_ligne(&vu)))?;
        let entree = entree_de_l_index(jeu.bac, fichier).map_err(|vu| {
            format!("red first: git does not say what it stores of the test file {nom}: {}", en_ligne(&vu))
        })?;
        let Some((mode, objet)) = entree else {
            return Err(format!("red first: git keeps no entry for the test file {nom}"));
        };
        if mode != "100644" && mode != "100755" {
            return Err(format!("red first: the test file {nom} is not an ordinary file for git (mode {mode})"));
        }
        let empreinte = match empreinte_gardee(jeu.bac, Arbre::Index, fichier, BORNE_EMPREINTE) {
            Ok(Gardee::Empreinte(empreinte)) => empreinte,
            Ok(Gardee::TropLourd) => {
                return Err(format!("red first: the test file {nom} is larger than {BORNE_EMPREINTE} bytes"));
            }
            Ok(Gardee::Absent | Gardee::PasOrdinaire(_)) => {
                return Err(format!("red first: git keeps no ordinary file at the test file {nom}"));
            }
            Err(vu) => {
                return Err(format!(
                    "red first: git does not say what it stores of the test file {nom}: {}",
                    en_ligne(&vu)
                ));
            }
        };
        entrees.push((fichier.clone(), mode, objet, empreinte));
    }
    let nommes = || fichiers.iter().map(|fichier| en_ligne(fichier)).collect::<Vec<_>>().join(", ");
    let bac_du_rouge = jeu.dossier.join("bacs").join(format!("{}-rouge-{}", jeu.main, jeu.tentative));
    let pose = poser(jeu.depot, &bac_du_rouge, &["--detach"], jeu.depart)
        .map_err(|vu| format!("red first: the sandbox of the red replay is not made: {}", en_ligne(&vu)));
    let jeu_du_rouge = pose.and_then(|()| {
        for (fichier, mode, objet, _) in &entrees {
            let nom = en_ligne(fichier);
            let inscrire =
                geste(&bac_du_rouge, &["update-index", "--add", "--cacheinfo", &format!("{mode},{objet},{fichier}")]);
            faire(inscrire).map_err(|vu| {
                format!("red first: the test file {nom} does not enter the red sandbox: {}", en_ligne(&vu))
            })?;
            faire(geste(&bac_du_rouge, &["checkout-index", "-f", "--", fichier])).map_err(|vu| {
                format!("red first: the test file {nom} is not written in the red sandbox: {}", en_ligne(&vu))
            })?;
        }
        Ok(rejouer_et_finir(jeu.profil, jeu.commande, &bac_du_rouge, jeu.journal))
    });
    // Le bac du rouge se retire dans tous les cas, de force ; un bac qui n'a pas pu se poser n'a rien à retirer.
    if bac_du_rouge.exists()
        && let Err(vu) = retirer_de_force(jeu.depot, &bac_du_rouge)
    {
        eprintln!("lydite: cannot remove the red sandbox {}: {vu}", bac_du_rouge.display());
    }
    let ((verif, rejeu), fin) = jeu_du_rouge?;
    match fin {
        FinDuRejeu::Rouge => {
            let fichiers = entrees.into_iter().map(|(fichier, _, _, empreinte)| format!("{fichier} {empreinte}"));
            Ok((fichiers.collect(), verif.unwrap_or_default()))
        }
        FinDuRejeu::Verte => Err(format!(
            "red first: the proof passes on the base plus only {}: this test does not fail there (a test in the file \
             of the fix, or in a file that the base does not compile or collect, cannot fail on the base)",
            nommes()
        )),
        FinDuRejeu::Delai | FinDuRejeu::Tuee(_) | FinDuRejeu::Autre => Err(format!(
            "red first: the proof replayed on the base plus only {} did not end red by itself, which is not the red \
             of a test: {}",
            nommes(),
            rejeu.err().unwrap_or_default()
        )),
    }
}

/// Le seul message qu'une main envoie à la vague : sa tâche, la tentative de son bail (le jeton que son résultat
/// porte), et ce qu'elle a rendu, le `main.rouge` constaté compris.
struct Message {
    main: usize,
    tentative: usize,
    rendu: Rendu,
}

/// Où en est une tâche.
enum Etat {
    /// Elle attend ses dépendances.
    Attente,
    /// Sa main court, dans son bac parti de `depart`, jusqu'à l'échéance de son bail.
    Partie { bac: PathBuf, depart: String, echeance: String },
    /// Son issue, que plus rien ne change.
    Finie(Result<Pris, String>),
}

/// Ce que la main reçoit en prompt système et ce que le fil en dit (`main.consigne`) : le fichier des leçons données,
/// s'il y en a, son empreinte, les leçons données (`<nom> <SHA-256 de son SKILL.md>`, une par ligne) et le nom des
/// autres, une par ligne.
struct Gardees {
    fichier: Option<PathBuf>,
    systeme: Option<String>,
    donnees: String,
    ecartees: String,
}

/// Une vague qui tourne : ce qu'elle mène, et où en est chaque tâche.
struct Vague<'a> {
    id: String,
    /// Le sha d'où partent les bacs des indépendantes, et la branche de la vague.
    base: String,
    depot: &'a Path,
    profil: &'a Profil,
    plan: &'a Plan,
    /// La durée du bail de chaque tentative.
    duree: Duration,
    /// La séance de la vague, sur le fil du dossier de Kraken, où vivent aussi les bacs et les journaux des preuves.
    fil: Fil,
    /// L'assemblage d'un plan ; `None` pour des copies.
    assemblage: Option<Assemblage<'a>>,
    /// Le juge de ses mains, celui du découpeur ou du relecteur ([`Liens`]).
    juge: Option<Juge<'a>>,
    /// Les leçons du commit de base, lues au départ de la vague.
    lecons: Vec<Lecon>,
    /// Vrai pour la vague d'une correction (7017) ou d'une relance de la CI, qui ont leur propre contrôle.
    corrige: bool,
    /// Le rôle de ses mains, que dit leur `main.consigne`.
    role: Role,
    /// Ce qui se pose dans le bac de chaque main avant son premier départ ([`Preparation`]).
    preparation: Option<Preparation<'a>>,
    /// Le contenu des fichiers que le plan pose, que chaque tentative pose dans le bac de sa tâche ([`Liens::poses`]).
    poses: &'a [Posee],
    baux: Baux,
    /// Le rang de chaque tâche dans le plan, par son id.
    rangs: HashMap<&'a str, usize>,
    taches: Vec<Etat>,
    /// La pause d'une tâche qui repart sur une coupure de l'API ([`PAUSE`]).
    pause: Duration,
    /// Le nombre de coupures de l'API de chaque tâche : la première repart, la seconde classe la tâche. Une tentative
    /// qui suit une coupure n'est pas une relance.
    coupures: Vec<usize>,
    /// L'erreur de la tentative d'avant que porte la dernière tentative partie de chaque tâche (`None` à la première) :
    /// une tentative qui porte une erreur est une relance.
    precedentes: Vec<Option<String>>,
    /// Les tentatives parties dont le message n'est pas encore arrivé, en (main, tentative).
    en_vol: Vec<(usize, usize)>,
    /// Le refus 7012 qui a arrêté la vague : plus aucune note, ni départ, ni fusion.
    arret: Option<Refus>,
}

impl<'a> Vague<'a> {
    fn main(&self, k: usize) -> String {
        format!("main-{}-{k}", self.id)
    }

    /// Mène la vague jusqu'au bout : chaque tâche part dès que ses dépendances sont assemblées, ou est sautée ; chaque
    /// message se reçoit dans l'ordre d'arrivée ; un bail échu expire. La vague finit quand plus aucune main n'est en
    /// vol, mains expirées comprises : Kraken ne part pas en laissant tourner une main sans trace.
    // raccourci : un fil d'exécution qui ne rend rien retient la fin de lancer ; monter vers des fils détachés avec le
    // nœud
    fn mener<'s>(&mut self, s: &'s Scope<'s, '_>) -> Issues
    where
        'a: 's,
    {
        let (envoi, reception) = mpsc::channel();
        loop {
            self.partir(s, &envoi);
            if self.en_vol.is_empty() {
                break;
            }
            let premier = reception.recv_timeout(ATTENTE).ok();
            // L'instant d'expiration se prend avant de vider le canal : un résultat arrivé avant lui conclut son bail.
            let maintenant = Instant::now();
            for message in premier.into_iter().chain(iter::from_fn(|| reception.try_recv().ok())) {
                self.recevoir(s, &envoi, message);
            }
            // Arrêtée, la vague ne décide plus rien : elle attend ses mains.
            if self.arret.is_none() {
                for (k, tentative) in self.baux.expirer(maintenant) {
                    self.expirer(k, tentative);
                }
            }
        }
        // Plus rien ne court. Dans une vague qui ne s'est pas arrêtée, une tâche qui attend encore a une dépendance qui
        // ne s'assemblera jamais (un cycle, que `Plan::lire` refuse) : elle est sautée, comme les autres.
        if self.arret.is_none() {
            for k in 1..=self.taches.len() {
                if matches!(self.taches[k - 1], Etat::Attente) {
                    self.sauter(k);
                }
            }
        }
        // Dans une vague arrêtée, une tâche qui n'a pas pu partir porte le refus qui l'a arrêtée.
        let arretee = self.hors_du_fil("");
        let taches = mem::take(&mut self.taches);
        (1..)
            .zip(taches)
            .map(|(k, etat)| {
                let issue = match etat {
                    Etat::Finie(issue) => issue,
                    Etat::Attente | Etat::Partie { .. } => Err(arretee.clone()),
                };
                (self.main(k), issue)
            })
            .collect()
    }

    /// Fait partir, dans l'ordre du plan, chaque tâche en attente dont toutes les dépendances sont assemblées, tant que
    /// moins de [`MAINS_A_LA_FOIS`] mains sont en vol ; saute sans bac, de proche en proche, celles dont une dépendance
    /// ne sera jamais assemblée. Une vague arrêtée ne fait plus rien partir.
    fn partir<'s>(&mut self, s: &'s Scope<'s, '_>, envoi: &Sender<Message>)
    where
        'a: 's,
    {
        let plan = self.plan;
        // Un saut peut en entraîner un autre, plus haut dans le plan : un tour de plus, jusqu'à ce que rien ne change.
        let mut encore = true;
        while encore {
            encore = false;
            for (k, tache) in (1..).zip(&plan.taches) {
                if self.arret.is_some() {
                    return;
                }
                if !matches!(self.taches[k - 1], Etat::Attente) {
                    continue;
                }
                if tache.depend.iter().any(|id| self.perdue(id)) {
                    self.sauter(k);
                    encore = true;
                } else if self.en_vol.len() < MAINS_A_LA_FOIS && tache.depend.iter().all(|id| self.assemblee(id)) {
                    self.depart(s, envoi, k, None, false);
                }
            }
        }
    }

    fn assemblee(&self, id: &str) -> bool {
        self.rangs.get(id).is_some_and(|&rang| matches!(self.taches[rang], Etat::Finie(Ok(_))))
    }

    fn perdue(&self, id: &str) -> bool {
        self.rangs.get(id).is_none_or(|&rang| matches!(self.taches[rang], Etat::Finie(Err(_))))
    }

    /// Le profil d'une tâche : celui de la vague aux modèles de la tâche (`Profil::avec_modeles`), puis, si elle donne
    /// un `effort`, à cet effort pour ses deux tentatives (`Profil::avec_efforts`), à la place des clés `effort` et
    /// `effort_relance` du profil. Sans `effort`, ceux du profil restent, ceux de son rôle compris.
    fn profil_de(&self, tache: &Tache) -> Profil {
        let profil = self.profil.avec_modeles(tache.modele.as_deref(), tache.relance.as_deref());
        match tache.effort.as_deref() {
            Some(effort) => profil.avec_efforts(Some(effort), Some(effort)),
            None => profil,
        }
    }

    /// Fait partir une tentative de la tâche `k`. À la première, son bac se pose : celui d'une indépendante sur la
    /// base, celui d'une dépendante sur la tête de la vague, où les siennes sont assemblées ; puis la préparation de
    /// la vague, s'il y en a une, y pose ce qu'elle doit, et son refus refuse la tâche, sans bail. Puis son bail
    /// s'ouvre et se note (`main.bail`), son début aussi (`main.debut`), son lancement enfin (`main.lancement`, voir
    /// `noter_lancement`), et sa main part dans un fil d'exécution à elle, qui rend un seul message. La main et son
    /// lancement reçoivent le profil de la tâche, celui de la vague à ses modèles et à son effort (`profil_de`).
    /// Une note ratée, ou des réglages du bac illisibles, arrêtent la vague, et la main ne part pas. `precedente` est
    /// l'erreur de la tentative d'avant, que la relance porte en contexte. `repartie` : la tentative repart sur une
    /// coupure de l'API ; sa main attend alors [`PAUSE`] dans son fil d'exécution avant de jouer, et son bail la
    /// couvre. Une main verte dont la tâche a des
    /// `fichiers` rend une tentative rouge si elle a changé un chemin hors d'eux (`dans_ses_fichiers`). Le juge de la
    /// vague, s'il y en a un, rend le verdict de la main (`tenter`).
    fn depart<'s>(
        &mut self,
        s: &'s Scope<'s, '_>,
        envoi: &Sender<Message>,
        k: usize,
        precedente: Option<String>,
        repartie: bool,
    ) where
        'a: 's,
    {
        let plan = self.plan;
        let tache = &plan.taches[k - 1];
        let (bac, depart) = match &self.taches[k - 1] {
            Etat::Partie { bac, depart, .. } => (bac.clone(), depart.clone()),
            Etat::Attente | Etat::Finie(_) => {
                let depart = match &self.assemblage {
                    Some(assemblage) if !tache.depend.is_empty() => assemblage.tete().to_string(),
                    _ => self.base.clone(),
                };
                match bac(self.depot, self.fil.dossier(), &self.main(k), &depart) {
                    Ok(bac) => {
                        // Ce que Kraken pose dans le bac se pose une fois, avant tout bail : un refus retient la main.
                        if let Some(Err(refus)) = self.preparation.map(|preparer| preparer(&bac)) {
                            self.refuser(k, refus, None);
                            return;
                        }
                        (bac, depart)
                    }
                    Err(refus) => {
                        self.refuser(k, refus, None);
                        return;
                    }
                }
            }
        };
        // Les fichiers que le plan pose se posent avant chaque tentative, la relance comprise (une main d'avant a pu
        // les changer), et avant tout bail : un refus retient la main.
        if let Err(refus) = self.poser_les_fichiers(k, &bac) {
            self.refuser(k, refus, None);
            return;
        }
        let verifier = tache.preuve.as_deref().or(plan.preuve.as_deref());
        let profil = self.profil_de(tache);
        // Les leçons de la main, écrites avant tout bail : une écriture ratée arrête la vague.
        // Puis le texte de sa consigne, calculé une fois et gardé de même.
        let gardees = self.fichier_des_lecons(&profil, tache).and_then(|gardees| {
            let consigne = self.garder_la_consigne(tache, &profil, verifier, precedente.as_deref())?;
            Ok((gardees, consigne))
        });
        let (gardees, consigne) = match gardees {
            Ok(prises) => prises,
            Err(refus) => {
                self.arret.get_or_insert(refus);
                let issue = self.hors_du_fil(precedente.as_deref().unwrap_or_default());
                self.taches[k - 1] = Etat::Finie(Err(issue));
                return;
            }
        };
        // Le quota du jour, avant tout bail : une main refusée ici n'a ni bail ni début ; les mains parties finissent.
        if let Err(refus) = self.sous_le_quota(&profil, &bac, verifier, precedente.is_some()) {
            self.refuser(k, refus, None);
            return;
        }
        let (depuis, ms) = (Instant::now(), maintenant());
        let pause = if repartie { self.pause } else { Duration::ZERO };
        let duree = self.duree + pause;
        let tentative = match self.baux.ouvrir(k, depuis, duree) {
            Ok(tentative) => tentative,
            Err(refus) => {
                self.refuser(k, refus, None);
                return;
            }
        };
        let echeance = instant(ms.saturating_add(duree.as_millis()));
        let bail = Evenement::Bail { vague: self.id.clone(), main: k, tentative, echeance: echeance.clone() };
        let debut = Evenement::Debut { vague: self.id.clone(), main: k, tentative };
        // La consigne se note après le lancement (et le fournisseur), avant que la main parte.
        let note_consigne = Evenement::Consigne {
            vague: self.id.clone(),
            main: k,
            tentative,
            role: self.role,
            consigne: empreinte(&consigne),
            systeme: gardees.systeme.clone(),
            lecons: gardees.donnees.clone(),
            ecartees: gardees.ecartees.clone(),
            mesure: profil.mesure().texte(),
        };
        if !(self.noter(&bail)
            && self.noter(&debut)
            && self.noter_lancement(&profil, &bac, k, tentative, verifier, precedente.is_some())
            && self.noter(&note_consigne))
        {
            // La main ne part pas ; son issue garde l'erreur de sa tentative d'avant, s'il y en a une.
            let issue = self.hors_du_fil(precedente.as_deref().unwrap_or_default());
            self.taches[k - 1] = Etat::Finie(Err(issue));
            return;
        }
        self.taches[k - 1] = Etat::Partie { bac: bac.clone(), depart: depart.clone(), echeance };
        self.en_vol.push((k, tentative));
        self.precedentes[k - 1] = precedente.clone();
        let envoi = envoi.clone();
        // Hors du dépôt visé et du bac : la fin de la sortie entre au fil, le fichier s'efface.
        // raccourci : tué en pleine vérification, Kraken laisse le fichier ; monter avec le nœud, qui rangera.
        let journal = self.fil.dossier().join(format!("verif-{}-{k}-{tentative}.tmp", self.id));
        let juge = self.juge;
        // Une tâche de plan sans juge : sa main doit avoir changé un chemin. Les mains qui jugent ont leur contrôle.
        let plan_sans_juge = self.assemblage.is_some() && juge.is_none() && !self.corrige;
        // Les épingles `sha256` de tout le plan : celles de la tâche, et celles d'une autre dont sa main change le
        // fichier.
        let epingles = assemblage::epingles_du_plan(plan);
        // Le rouge d'abord : une tâche à `red` qui a une preuve, hors d'un juge. Le dossier de Lydite et le dépôt se
        // prennent ici, comme `journal` : le fil d'exécution de la main ne touche pas à la vague.
        let rouge = if juge.is_none() { verifier.zip(tache.rouge.as_ref()) } else { None };
        let (depot, dossier, main_id, vague) =
            (self.depot, self.fil.dossier().to_path_buf(), self.main(k), self.id.clone());
        let journal_du_rouge = self.fil.dossier().join(format!("verif-{}-{k}-{tentative}-rouge.tmp", self.id));
        s.spawn(move || {
            // La pause d'une tentative qui repart sur une coupure de l'API se passe ici, hors de la vague.
            if !pause.is_zero() {
                thread::sleep(pause);
            }
            let rendu = en_message(|| {
                let (constat, verif, verdict) = tenter(
                    &profil,
                    &bac,
                    &consigne,
                    verifier,
                    precedente.as_deref(),
                    &journal,
                    juge,
                    gardees.fichier.as_deref(),
                );
                let verdict = verdict.and_then(|pris| {
                    dans_ses_fichiers(&bac, &depart, tache, profil.proteges(), pris).map_err(Echec::Erreur)
                });
                let verdict = verdict.and_then(|pris| textes_exacts(&bac, tache, pris).map_err(Echec::Erreur));
                let verdict = verdict.and_then(|pris| {
                    assemblage::epingles_tenues(&bac, &depart, &tache.id, &epingles, profil.proteges())
                        .map(|()| pris)
                        .map_err(Echec::Erreur)
                });
                let verdict = match (plan_sans_juge, verdict) {
                    (true, Ok(pris)) => {
                        main_qui_change(&bac, &depart, profil.proteges()).map(|()| pris).map_err(Echec::Erreur)
                    }
                    (_, verdict) => verdict,
                };
                // Au bout de la chaîne, quand tout le reste est vert : le rouge constaté ; la note est celle de la
                // vague.
                let (verdict, rouge) = match (rouge, verdict) {
                    (Some((commande, demande)), Ok(pris)) => {
                        let jeu = Constatation {
                            profil: &profil,
                            depot,
                            dossier: &dossier,
                            main: &main_id,
                            tentative,
                            bac: &bac,
                            depart: &depart,
                            tache,
                            commande,
                            journal: &journal_du_rouge,
                        };
                        match constater_le_rouge(&jeu, demande, &pris.sortie) {
                            Ok((fichiers, verif)) => {
                                let base = depart.clone();
                                (Ok(pris), Some(Evenement::Rouge { vague, main: k, tentative, base, fichiers, verif }))
                            }
                            Err(erreur) => (Err(Echec::Erreur(erreur)), None),
                        }
                    }
                    (_, verdict) => (verdict, None),
                };
                (constat, verif, verdict, rouge)
            });
            // La vague lit le canal jusqu'au dernier message qu'elle attend : celui-ci ne se perd pas.
            let _ = envoi.send(Message { main: k, tentative, rendu });
        });
    }

    /// Reçoit le message d'une main. Son bail se conclut, ou son résultat est périmé ; puis son constat, s'il y en a
    /// un, et sa fin se notent, et la suite dépend d'elle : rouge à la première tentative, la relance, si rien ne
    /// l'interdit ; rouge à la seconde, la tâche classée ; verte, la tâche prise, puis assemblée pour un plan.
    fn recevoir<'s>(&mut self, s: &'s Scope<'s, '_>, envoi: &Sender<Message>, message: Message)
    where
        'a: 's,
    {
        let Message { main: k, tentative, rendu: (constat, verif, verdict, rouge) } = message;
        self.en_vol.retain(|&vol| vol != (k, tentative));
        let (couleur, foi, texte) = match &verdict {
            Ok(Pris { sortie, prouve: true }) => (Couleur::Vert, Foi::Constate, sortie.clone()),
            Ok(Pris { sortie, prouve: false }) => (Couleur::Vert, Foi::Declare, sortie.clone()),
            // La raison d'une main qui déclare l'impossible : rouge, et déclarée, car Kraken n'a rien vu.
            Err(Echec::Impossible(raison)) => (Couleur::Rouge, Foi::Declare, raison.clone()),
            Err(Echec::Erreur(erreur) | Echec::Coupure(_, erreur) | Echec::Tuee(erreur) | Echec::Plafond(erreur)) => {
                (Couleur::Rouge, Foi::Constate, erreur.clone())
            }
        };
        let vague = self.id.clone();
        // Le jeton : seul se prend un résultat qui porte la tentative du bail ouvert de sa tâche. Un résultat périmé
        // n'est ni pris, ni figé, ni assemblé : il se note, et c'est tout.
        if self.baux.conclure(k, tentative).is_err() {
            let perimee = Evenement::Perimee { vague, main: k, tentative, couleur, foi, texte: texte.clone(), verif };
            // Une main qui dépasse son bail a dépensé : son usage se note juste avant sa péremption, et compte pour
            // le quota des départs qui suivent.
            let bac = self.fil.dossier().join("bacs").join(self.main(k));
            let usages = constat.as_ref().map(|constat| self.usages(k, tentative, &bac, constat)).unwrap_or_default();
            if !usages.iter().chain(iter::once(&perimee)).all(|note| self.noter(note)) {
                let hors = self.hors_du_fil(&texte);
                if let Etat::Finie(Err(issue)) = &mut self.taches[k - 1] {
                    issue.push('\n');
                    issue.push_str(&hors);
                }
            }
            return;
        }
        // Un bail ouvert est celui d'une tâche partie.
        let Etat::Partie { bac, depart, .. } = &self.taches[k - 1] else { return };
        let (bac, depart) = (bac.clone(), depart.clone());
        // Son constat, quand Kraken a lu quelque chose d'elle, puis son usage, une ligne par modèle, puis sa fin : le
        // constat précède l'usage, et l'usage la fin, au fil.
        let usages = constat.as_ref().map(|constat| self.usages(k, tentative, &bac, constat)).unwrap_or_default();
        let constat = constat.map(|Constat { version, mur_actif, .. }| Evenement::Constat {
            vague: vague.clone(),
            main: k,
            tentative,
            version,
            mur_actif,
        });
        let fin =
            Evenement::Fin { vague: vague.clone(), main: k, tentative, couleur, foi, texte: texte.clone(), verif };
        // Une coupure de l'API se note après la fin de la tentative coupée.
        let coupure = match &verdict {
            Err(Echec::Coupure(statut, _)) => {
                Some(Evenement::Coupure { vague: vague.clone(), main: k, tentative, statut: *statut })
            }
            _ => None,
        };
        // Le `main.rouge` suit la fin dans la même suite : seule la vague note, et un résultat périmé n'en note pas.
        let notes: Vec<Evenement> =
            constat.into_iter().chain(usages).chain(iter::once(fin)).chain(rouge).chain(coupure).collect();
        if !notes.iter().all(|note| self.noter(note)) {
            let issue = self.hors_du_fil(&texte);
            self.taches[k - 1] = Etat::Finie(Err(issue));
            return;
        }
        let bac = (bac.as_path(), depart.as_str());
        match verdict {
            // Impossible, déclarée : classée sans relance (l'erreur en contexte la pousserait à plier), rien ne
            // s'assemble.
            Err(Echec::Impossible(raison)) => {
                let impossible = Evenement::Verdict { vague, main: k, verdict: Verdict::Impossible, foi: Foi::Declare };
                let erreur = format!("{IMPOSSIBLE}{raison}");
                let issue = if self.noter(&impossible) { erreur } else { self.hors_du_fil(&erreur) };
                self.taches[k - 1] = Etat::Finie(Err(issue));
            }
            // Une main qui atteint son plafond de dépense, à toute tentative : refusée sans relance (7019), la vague
            // continue pour ses autres tâches.
            Err(Echec::Plafond(erreur)) => {
                let preuve = format!(
                    "{} ({}) reached its spending ceiling, {} dollars: {}",
                    self.main(k),
                    self.plan.taches[k - 1].id,
                    self.profil.plafond(),
                    erreur.lines().next().unwrap_or_default()
                );
                let correction = "The task is refused without a retry, because the hand reached its spending ceiling: \
                                  raise `plafond` in the profile, accept it (`lydite accept`), then run the task \
                                  again in a fresh wave, or leave it.";
                self.refuser(k, Refus::new(Code::PlafondAtteint, preuve, correction), None);
            }
            // La première coupure de la tâche repart, après une pause, sans compter comme une relance, avec l'erreur
            // que portait la tentative coupée ; la seconde la classe, la cause dite.
            Err(Echec::Coupure(..)) if self.coupures[k - 1] == 0 => {
                self.coupures[k - 1] = 1;
                let precedente = self.precedentes[k - 1].clone();
                self.repartir(s, envoi, k, bac, precedente);
            }
            Err(Echec::Coupure(statut, _)) => {
                let classee = Evenement::Verdict { vague, main: k, verdict: Verdict::Classee, foi: Foi::Constate };
                let cause = format!("API outage ({statut})");
                let issue = if self.noter(&classee) { cause } else { self.hors_du_fil(&cause) };
                self.taches[k - 1] = Etat::Finie(Err(issue));
            }
            // Une relance suit la première tentative de la tâche que ne coupe aucune coupure de l'API.
            Err(Echec::Erreur(erreur)) if tentative - self.coupures[k - 1] == 1 => {
                self.relancer(s, envoi, k, bac, erreur, false);
            }
            Err(Echec::Tuee(erreur)) if tentative - self.coupures[k - 1] == 1 => {
                self.relancer(s, envoi, k, bac, erreur, true);
            }
            Err(Echec::Erreur(erreur) | Echec::Tuee(erreur)) => {
                let classee = Evenement::Verdict { vague, main: k, verdict: Verdict::Classee, foi: Foi::Constate };
                let issue = if self.noter(&classee) { erreur } else { self.hors_du_fil(&erreur) };
                self.taches[k - 1] = Etat::Finie(Err(issue));
            }
            Ok(pris) => {
                let foi = if pris.prouve { Foi::Constate } else { Foi::Declare };
                if self.noter(&Evenement::Verdict { vague, main: k, verdict: Verdict::Prise, foi }) {
                    self.assembler(k, bac, pris);
                } else {
                    let issue = self.hors_du_fil(&pris.sortie);
                    self.taches[k - 1] = Etat::Finie(Err(issue));
                }
            }
        }
    }

    /// Après une première coupure de l'API : la garde du `.claude/` du bac (7006, ou 7009 si git rate), comme avant une
    /// relance, puis la tentative repart avec `precedente`, l'erreur que portait la tentative coupée, après la pause.
    fn repartir<'s>(
        &mut self,
        s: &'s Scope<'s, '_>,
        envoi: &Sender<Message>,
        k: usize,
        (bac, depart): (&Path, &str),
        precedente: Option<String>,
    ) where
        'a: 's,
    {
        match sans_claude(bac, depart) {
            Ok(()) => self.depart(s, envoi, k, precedente, true),
            Err(refus) => self.refuser(k, refus, None),
        }
    }

    /// Après une première tentative rouge, dans cet ordre : la garde du `.claude/` du bac, parti de `depart`, avant
    /// toute relance dans ce bac (7006, ou 7009 si git rate) ; la main tuée à son délai (7005) ; puis la relance,
    /// l'erreur en contexte.
    fn relancer<'s>(
        &mut self,
        s: &'s Scope<'s, '_>,
        envoi: &Sender<Message>,
        k: usize,
        (bac, depart): (&Path, &str),
        erreur: String,
        tuee: bool,
    ) where
        'a: 's,
    {
        if let Err(refus) = sans_claude(bac, depart) {
            self.refuser(k, refus, None);
        } else if tuee {
            // raccourci : une main tuée à son délai est refusée sans relance ; monter vers une relance quand le nœud
            // prouvera la mort de l'arbre
            let preuve = format!(
                "{} ({}) : {}",
                self.main(k),
                self.plan.taches[k - 1].id,
                erreur.lines().next().unwrap_or_default()
            );
            let correction = "The task is refused without a retry, because its hand's tree may still be running: \
                              raise the hand's timeout in its profile, or give it a smaller task, in a fresh wave.";
            self.refuser(k, Refus::new(Code::MainTuee, preuve, correction), None);
        } else {
            self.depart(s, envoi, k, Some(erreur), false);
        }
    }

    /// Assemble la prise verte de la tâche `k`, dont le bac est parti de `depart`, dans la branche de la vague ; des
    /// copies n'assemblent rien, et leur prise est leur issue. Assemblée, l'issue est la prise ; refusée, son refus,
    /// noté. Une fusion qui ne se note pas est défaite, et arrête la vague (7012).
    fn assembler(&mut self, k: usize, (bac, depart): (&Path, &str), pris: Pris) {
        let plan = self.plan;
        let tache = &plan.taches[k - 1];
        let (vague, fil) = (self.id.as_str(), &self.fil);
        let epingles = assemblage::epingles_du_plan(plan);
        let Some(assemblage) = &mut self.assemblage else {
            self.taches[k - 1] = Etat::Finie(Ok(pris));
            return;
        };
        let (verif, issue) = assemblage.fusionner_et_noter_le_gel(
            bac,
            depart,
            &tache.id,
            tache.fichiers.as_deref(),
            k,
            &epingles,
            |sha| fil.noter(&Evenement::Gel { vague: vague.into(), main: k, sha: sha.into() }),
            |sha, fichiers| {
                fil.noter(&Evenement::Empreintes { vague: vague.into(), main: k, sha: sha.into(), fichiers })
            },
            |sha, verif| {
                fil.noter(&Evenement::Fusion { vague: vague.into(), main: k, sha: sha.into(), verif: verif.into() })
            },
        );
        match issue {
            Ok(_) => self.taches[k - 1] = Etat::Finie(Ok(pris)),
            Err(refus) if refus.code == Code::FilInjoignable => {
                self.arret = Some(refus);
                let issue = self.hors_du_fil(&pris.sortie);
                self.taches[k - 1] = Etat::Finie(Err(issue));
            }
            Err(refus) => self.refuser(k, refus, verif),
        }
    }

    /// La tentative `tentative` de la tâche `k` n'a rien rendu à l'échéance de son bail : elle expire (7004), et sa
    /// tâche est refusée aussitôt, sans relance, car sa main peut tourner encore. Son résultat, s'il vient, est périmé.
    // raccourci : une tâche expirée est refusée sans relance ; monter vers une relance après le retour de sa main si le
    // besoin s'en montre
    fn expirer(&mut self, k: usize, tentative: usize) {
        let Etat::Partie { echeance, .. } = &self.taches[k - 1] else { return };
        let preuve = format!(
            "{} ({}) returned nothing before {echeance}, the deadline of the lease of its attempt {tentative}",
            self.main(k),
            self.plan.taches[k - 1].id
        );
        let correction = "The task is refused without a retry, because its hand may still be running: run it again \
                          in a fresh wave, with a smaller task if the hand was too slow.";
        self.refuser(k, Refus::new(Code::MainExpiree, preuve, correction), None);
    }

    fn sauter(&mut self, k: usize) {
        let plan = self.plan;
        let tache = &plan.taches[k - 1];
        let ids = || tache.depend.iter().map(String::as_str);
        let mut perdues: Vec<&str> = ids().filter(|id| self.perdue(id)).collect();
        if perdues.is_empty() {
            perdues = ids().filter(|id| !self.assemblee(id)).collect();
        }
        let preuve = format!(
            "{} ({}) does not start, and has no sandbox: it depends on {}, which the wave will not assemble",
            self.main(k),
            tache.id,
            perdues.join(", ")
        );
        let correction = "Run it again in a fresh wave, once the tasks it depends on are assembled.";
        self.refuser(k, Refus::new(Code::Sautee, preuve, correction), None);
    }

    /// Refuse la tâche `k` : le refus se note au fil (`vague.refus`), avec `verif`, la fin de la sortie de la preuve de
    /// la vague si elle a tourné, et l'issue est ce refus en clair. Si la note ne s'écrit pas, l'issue garde ce refus
    /// après celui qui a arrêté la vague.
    fn refuser(&mut self, k: usize, refus: Refus, verif: Option<String>) {
        let clair = en_clair(&refus);
        let Refus { code, preuve, correction } = refus;
        let regle = code.regle().into();
        let note =
            Evenement::Refus { vague: self.id.clone(), main: k, code: code.numero(), regle, preuve, correction, verif };
        let issue = if self.noter(&note) { clair } else { self.hors_du_fil(&clair) };
        self.taches[k - 1] = Etat::Finie(Err(issue));
    }

    /// Pose dans `bac`, le bac de la tâche `k`, chaque fichier que le plan pose pour elle ([`poser_un_fichier`]), dans
    /// l'ordre du plan ; le temporaire de chacun est dans le dossier de Lydite, hors du bac. Un geste raté est le refus
    /// 7009, qui nomme le fichier : rien ne se pose ailleurs que dans ce bac.
    fn poser_les_fichiers(&self, k: usize, bac: &Path) -> Result<(), Refus> {
        let tache = &self.plan.taches[k - 1];
        for (rang, posee) in self.poses.iter().filter(|posee| posee.tache == tache.id).enumerate() {
            let temporaire = self.fil.dossier().join(format!("pose-{}-{k}-{rang}.tmp", self.id));
            poser_un_fichier(bac, &temporaire, &posee.fichier, &posee.octets).map_err(|vu| {
                let preuve = format!(
                    "{} ({}) : the file {} that the plan places is not placed in its sandbox: {}",
                    self.main(k),
                    tache.id,
                    en_ligne(&posee.fichier),
                    en_ligne(&vu)
                );
                let correction = "The task is refused without a retry, because Lydite could not place a file in its \
                                  sandbox: check that git writes there and that nothing blocks the path, then run it \
                                  again in a fresh wave.";
                Refus::new(Code::GitImpossible, preuve, correction)
            })?;
        }
        Ok(())
    }

    /// Le fichier des leçons de la main de `tache` (`dossier::ecrire_consigne`), s'il y en a : les leçons de la tâche
    /// (`lecons::donnees`) pour une main de tâche, la liste de toutes pour le découpeur et le relecteur (la vague a un
    /// juge). Aucun pour une main nue, ou sans leçon qui lui aille. Une écriture ratée est le refus 7012.
    fn fichier_des_lecons(&self, profil: &Profil, tache: &Tache) -> Result<Gardees, Refus> {
        let mut triees: Vec<&Lecon> = self.lecons.iter().collect();
        triees.sort_by(|a, b| a.nom.cmp(&b.nom));
        let donnees: Vec<&Lecon> = if profil.nue() {
            Vec::new()
        } else if self.juge.is_some() {
            triees.clone()
        } else {
            lecons::donnees(&self.lecons, tache.fichiers.as_deref())
        };
        let ecartees: Vec<&str> =
            triees.iter().filter(|lecon| !donnees.contains(lecon)).map(|lecon| lecon.nom.as_str()).collect();
        // L'empreinte notée est celle du texte donné à la main (`\r\n` ramenés à `\n` à la lecture), non celle des
        // octets du SKILL.md au commit de base quand il est en `\r\n`.
        let donnees_dites: Vec<String> =
            donnees.iter().map(|lecon| format!("{} {}", lecon.nom, empreinte(&lecon.texte))).collect();
        let texte = if profil.nue() {
            String::new()
        } else if self.juge.is_some() {
            lecons::texte_liste(&self.lecons)
        } else {
            let texte = lecons::texte_donne(&donnees);
            // Le réglage `sans-lecture` (clé `mesure`) retire la consigne de lecture, et garde le bloc des leçons.
            match texte.strip_prefix(&format!("{}\n", lecons::CONSIGNE_DE_LECTURE)) {
                Some(bloc) if profil.mesure().sans_lecture => bloc.to_string(),
                _ => texte,
            }
        };
        let (fichier, systeme) = if texte.is_empty() {
            (None, None)
        } else {
            (Some(ecrire_consigne(self.fil.dossier(), &texte, ARRET)?), Some(empreinte(&texte)))
        };
        Ok(Gardees { fichier, systeme, donnees: donnees_dites.join("\n"), ecartees: ecartees.join("\n") })
    }

    /// Le texte exact de la consigne de la tentative `precedente` de la tâche `tache`, calculé une fois, et gardé dans
    /// le dossier de Kraken (`dossier::ecrire_consigne`) : ses octets sont ceux qu'on note et qu'on envoie à la main.
    /// Une écriture ratée est le refus 7012.
    fn garder_la_consigne(
        &self,
        tache: &Tache,
        profil: &Profil,
        verifier: Option<&str>,
        precedente: Option<&str>,
    ) -> Result<String, Refus> {
        let juge = self.juge.is_some();
        let fichiers = tache.fichiers.as_deref();
        // Ses épingles, puis celles des autres tâches que ses `fichiers` couvrent (toutes, sans `fichiers`).
        let mut epingles = tache.empreintes.clone();
        for autre in self.plan.taches.iter().filter(|autre| autre.id != tache.id) {
            for (fichier, empreinte) in &autre.empreintes {
                let couvert = fichiers.is_none_or(|fichiers| fichiers.iter().any(|entree| couvre(entree, fichier)));
                if couvert && !epingles.iter().any(|(deja, _)| deja == fichier) {
                    epingles.push((fichier.clone(), empreinte.clone()));
                }
            }
        }
        let objectif = &tache.objectif;
        let rouge = tache.rouge.as_ref();
        // Les fichiers que Lydite pose dans son bac, dans l'ordre du plan : une ligne de plus, avant les épingles.
        let poses: Vec<String> = tache.poses.iter().map(|(fichier, _)| fichier.clone()).collect();
        let texte = consigne_de(
            profil,
            objectif,
            fichiers,
            &tache.exacts,
            &poses,
            &epingles,
            rouge,
            verifier,
            precedente,
            juge,
        );
        ecrire_consigne(self.fil.dossier(), &texte, ARRET)?;
        Ok(texte)
    }

    /// Le quota du jour (`profils/quota`, voir `lire_quota`), avant le départ d'une tentative de la tâche `profil` :
    /// sans fichier, rien ne se vérifie. Sinon la main ne part pas si son profil a une `api` sans `prix` pour son
    /// modèle de cette tentative, ni si la dépense estimée du jour UTC (`depense_du_jour`) atteint le quota, ni si le
    /// fichier ou le fil ne se lisent pas : un refus 6030 ou celui du fil. Le forfait et l'inconnu ne comptent pas.
    fn sous_le_quota(&self, profil: &Profil, bac: &Path, verifier: Option<&str>, relance: bool) -> Result<(), Refus> {
        let kraken = self.fil.dossier();
        let Some(quota) = lire_quota(kraken)? else { return Ok(()) };
        if let Some(declaration) = profil.demande(bac, verifier, relance).declaration
            && declaration.fournisseur.is_some()
            && profil.prix(&declaration.modele).is_none()
        {
            let preuve = format!(
                "the profile names the provider {}, with no `prix` for the hand's model {}, under a set quota",
                declaration.fournisseur.unwrap_or_default(),
                declaration.modele
            );
            let correction = "Add this model and its prices to the profile's `prix` line, then accept the profile \
                              (lydite accept <profile>); or remove the quota file.";
            return Err(Refus::new(Code::Quota, preuve, correction));
        }
        let jour = &instant(maintenant())[..10];
        let depense = depense_du_jour(kraken, jour)?;
        if depense >= quota {
            let dollars =
                |micro_dollars: u64| format!("{}.{:06}", micro_dollars / 1_000_000, micro_dollars % 1_000_000);
            let preuve = format!(
                "the estimated spend of {jour} (UTC), {} $, reaches the day's quota, {} $",
                dollars(depense),
                dollars(quota)
            );
            let correction = "No hand starts before tomorrow (UTC): the running hands finish. Raise the quota in \
                              profils/quota, or remove the file, if the human decides to spend more.";
            return Err(Refus::new(Code::Quota, preuve, correction));
        }
        Ok(())
    }

    /// Les `main.usage` de la tentative `tentative` de la tâche `k`, dans `bac` : voir [`usages_de`], sous le modèle et
    /// le profil de la tâche. Aucun si le profil ne déclare rien (une main brute) : elle n'a ni modèle ni usage.
    fn usages(&self, k: usize, tentative: usize, bac: &Path, constat: &Constat) -> Vec<Evenement> {
        let tache = &self.plan.taches[k - 1];
        let verifier = tache.preuve.as_deref().or(self.plan.preuve.as_deref());
        let profil = self.profil_de(tache);
        // Une relance porte l'erreur d'avant ; une tentative qui repart sur une coupure n'en est pas une.
        let relance = self.precedentes[k - 1].is_some();
        match profil.demande(bac, verifier, relance).declaration {
            Some(declaration) => usages_de(&self.id, k, tentative, &declaration.modele, &profil, constat),
            None => Vec::new(),
        }
    }

    /// Note l'événement au fil. Une note ratée arrête la vague, sur le refus 7012, dont la preuve nomme le fil :
    /// ensuite, plus rien ne se note, ce qui protège ce que le fil prouve (le compteur, les jetons et les échéances).
    /// Rend vrai si la note est écrite.
    fn noter(&mut self, evenement: &Evenement) -> bool {
        if self.arret.is_some() {
            return false;
        }
        let Err(erreur) = self.fil.noter(evenement) else { return true };
        self.arret = Some(Refus::new(Code::FilInjoignable, erreur.to_string(), ARRET));
        false
    }

    /// Note le `main.lancement` de la tentative `tentative` de la tâche `k`, dans `bac` : ce que `profil`, celui de la
    /// tâche, déclare pour elle, et les réglages du dépôt que le bac porte à cet instant. Si le profil nomme un
    /// fournisseur, le `main.fournisseur` de la même tentative suit, à chaque tentative et pour chaque main, copies
    /// et mains du découpeur, du relecteur et de la correction comprises. Une main brute (`Profil::faux`) ne déclare
    /// rien : rien n'est noté, et elle peut partir. Des réglages qui ne se lisent pas arrêtent la vague sur le même
    /// refus 7012 qu'une note ratée, car la note ne peut pas s'écrire. Rend vrai si la main peut partir.
    fn noter_lancement(
        &mut self,
        profil: &Profil,
        bac: &Path,
        k: usize,
        tentative: usize,
        verifier: Option<&str>,
        relance: bool,
    ) -> bool {
        let Some(declaration) = profil.demande(bac, verifier, relance).declaration else { return true };
        match reglages_du_bac(bac).map(|reglages| declaration.lancement_selon(&self.id, k, tentative, reglages)) {
            Ok(lancement) => {
                let fournisseur = declaration.fournisseur(&self.id, k, tentative);
                self.noter(&lancement) && fournisseur.iter().all(|note| self.noter(note))
            }
            Err(erreur) => {
                self.arret.get_or_insert_with(|| Refus::new(Code::FilInjoignable, erreur.to_string(), ARRET));
                false
            }
        }
    }

    /// Ce que le fil n'a pas reçu, gardé dans l'issue de sa tâche : le refus qui a arrêté la vague, puis `texte`, la
    /// sortie de sa main ou un refus qui ne s'est pas noté. Rien ne se perd.
    fn hors_du_fil(&self, texte: &str) -> String {
        match (&self.arret, texte) {
            (Some(arret), "") => en_clair(arret),
            (Some(arret), texte) => format!("{}\n{texte}", en_clair(arret)),
            (None, texte) => texte.to_string(),
        }
    }
}

/// Les `main.usage` d'une tentative : une ligne par modèle du détail de l'usage du `constat`, ou, sans détail, une
/// ligne du total sous `modele`, le modèle de la tentative ; aucune si l'usage est vide ([`Usage::est_vide`]). La base
/// de chaque ligne : `Forfait` sous l'abonnement (`apiKeySource` `none`) quand Claude Code dit sa base `list`, et sa
/// dépense est celle qu'il annonce ; `Estime` si le profil donne un `prix` à ce modèle, ou, à défaut, au modèle de la
/// tentative (sous un fournisseur, Claude Code appelle aussi des modèles que la main ne nomme pas, que le fournisseur
/// sert et facture), la ligne gardant son propre nom (voir [`estimer`]) ; sinon `Inconnu`, à 0 : jamais le coût de
/// Claude Code quand sa base n'est pas `list`. Un nom de modèle que le fil ne lirait pas (vide, ou avec un blanc ou un
/// caractère hors de l'ASCII) s'écrit `?`.
fn usages_de(
    vague: &str,
    main: usize,
    tentative: usize,
    modele: &str,
    profil: &Profil,
    constat: &Constat,
) -> Vec<Evenement> {
    let Usage { jetons, micro_dollars, modeles, .. } = &constat.usage;
    if constat.usage.est_vide() {
        return Vec::new();
    }
    let total = [UsageModele { modele: modele.into(), jetons: *jetons, micro_dollars: *micro_dollars, base: None }];
    let lignes = if modeles.is_empty() { &total[..] } else { &modeles[..] };
    // Le forfait ne vaut que sous un profil sans fournisseur : sous un `api`, ce que la main dit de sa clé et de sa
    // base ne compte pas.
    let abonnement = profil.variable_de_la_cle().is_none() && constat.source_cle.as_deref() == Some("none");
    lignes
        .iter()
        .map(|ligne| {
            let (base, depense) = if abonnement && ligne.base.as_deref() == Some("list") {
                (Base::Forfait, ligne.micro_dollars)
            } else if let Some(prix) = profil.prix(&ligne.modele).or_else(|| profil.prix(modele)) {
                (Base::Estime, estimer(&ligne.jetons, &prix))
            } else {
                (Base::Inconnu, 0)
            };
            let valide = !ligne.modele.is_empty() && ligne.modele.bytes().all(|octet| octet.is_ascii_graphic());
            let Jetons { entree, sortie, cache_ecrit, cache_lu, reflexion } = ligne.jetons;
            Evenement::Usage {
                vague: vague.into(),
                main,
                tentative,
                modele: if valide { ligne.modele.clone() } else { "?".into() },
                entree,
                sortie,
                cache_ecrit,
                cache_lu,
                reflexion,
                micro_dollars: depense,
                base,
            }
        })
        .collect()
}

/// Le coût de ces jetons, en micro-dollars, au [`Prix`] d'un million de jetons : l'entrée, la sortie, le cache lu et le
/// cache écrit, chacun à son prix ; la réflexion ne se compte pas, la sortie la contient déjà. Arrondi au micro-dollar
/// supérieur (un quota ne se sous-estime pas) ; `u64::MAX` si cela ne tient pas.
fn estimer(jetons: &Jetons, prix: &Prix) -> u64 {
    let terme = |nombre: u64, prix: u64| u128::from(nombre) * u128::from(prix);
    let cout = [
        terme(jetons.entree, prix.entree),
        terme(jetons.sortie, prix.sortie),
        terme(jetons.cache_lu, prix.cache_lu),
        terme(jetons.cache_ecrit, prix.cache_ecrit),
    ]
    .into_iter()
    .fold(0u128, u128::saturating_add);
    u64::try_from(cout.div_ceil(1_000_000)).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
