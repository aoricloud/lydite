//! L'assemblage d'une vague à plan. Sa branche, `lydite/vague-<v>`, part de la base et seul Kraken l'avance ; son
//! worktree, `<dossier>/bacs/vague-<v>`, détaché, reçoit les prises une à une, dans l'ordre d'arrivée, et y rejoue la
//! preuve de la vague après chacune.
//!
//! L'invariant : la branche vaut la base, ou un commit dont la preuve est verte et la fusion notée ; elle ne porte
//! jamais l'état de Claude Code. Un retour à la tête raté ferme l'assemblage, pour qu'aucune fusion ne parte d'un état
//! mêlé. Kraken n'écrit dans le dépôt visé que des branches et des worktrees sous `.git`, et aucun `verif-*.tmp` ne
//! reste : les journaux de la preuve vivent dans le dossier de Kraken et s'effacent. Chaque geste git passe par
//! `git::geste`, qui ne joue ni crochet ni moniteur de fichiers du dépôt visé, et ne signe pas ce qu'il commite.
//!
//! Sous un mur exigé seulement, les pilotes de filtre et de fusion que nomment les `.gitattributes` d'un bac ne
//! tournent, hors du mur, que si une configuration les définit : celle du git commun du dépôt visé, que la main
//! n'écrit pas, celle du git-dir que Kraken a lu à la pose du worktree, avant toute main (`bac::poser`), et que
//! chacun de ses gestes porte au lieu de suivre le `.git` du bac, ou la configuration globale de git (celle de
//! l'utilisateur, et celle du système), dont les pilotes tournent aussi. Une main ne peut donc ni définir un pilote
//! ni détourner un geste vers un dépôt dont la configuration en définit un : ce ne sont que ceux que l'humain a posés
//! dans le dépôt visé ou dans la configuration globale de git.
//!
//! Les chemins que le profil protège (`proteges`, 7018) se jugent aux mêmes deux endroits, avant les `fichiers` : à la
//! fin de chaque tentative (`proteges_tenus`), un chemin protégé changé la rougit ; au gel, la tâche est refusée et
//! rien d'elle n'est fusionné, qu'elle ait des `fichiers` ou non. Les leçons (`.agents/skills/`) sont toujours de ces
//! chemins, que le profil ait ou non une clé `proteges` : une main n'en change aucune.
//!
//! Avant l'assemblage, `fichiers_tenus` juge ce que la main a changé dans son bac contre les `fichiers` de sa tâche
//! (7013) : la vague le joue à la fin de chaque tentative prise, avant qu'elle ne soit notée, et un chemin de trop la
//! rougit, pour que la relance le reçoive. Le gel est jugé de même, contre les mêmes `fichiers` : un chemin de trop
//! qu'il porte est refusé sans relance, et la fusion n'a pas lieu. Le gel est jugé aussi contre tout dossier `.claude`
//! (7006), que la tâche ait des `fichiers` ou non : `sans_claude` juge le bac avant le gel, et un `.claude` écrit ou
//! commité dans le bac après lui, même ôté avant le gel, entrerait dans la branche sans ce second jugement ; un tel
//! chemin est refusé sans relance, et la fusion n'a pas lieu. Il l'est de même contre les noms de `TOUJOURS_REFUSES`
//! (`.mcp.json`, `.vscode`...), que le mur ne ferme que s'ils existent déjà : un tel fichier neuf est le refus 7021.
//! Le découpeur et le relecteur, qui n'écrivent que leur
//! fichier, sont tenus par `seul_change` (7014).
//!
//! Un fichier que le plan épingle par `sha256` se juge de même aux deux endroits, sur ce que git garde et non sur le
//! disque (`git::empreinte_gardee`) : à la fin de chaque tentative (`epingles_tenues`), sur l'index du bac, pour les
//! épingles de la tâche et celles d'une autre tâche dont la main a changé le fichier ; au gel (`epingles_gelees`),
//! sur le commit gelé, pour les épingles de la tâche et celles d'une autre dont le gel a changé le fichier, quels que
//! soient `fichiers` et `proteges`, juste avant la fusion. Au gel, un écart est le refus 7022.
//!
//! La base s'éprouve par la preuve de la vague (6022) à l'ouverture de l'assemblage, et, pour un découpage, avant le
//! découpeur (`eprouver_la_base`), par la même épreuve mise à part (`eprouver`).
//!
//! Ce module ne connaît pas les événements du fil : la note d'une fusion lui vient par une fermeture.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::branchements::bac::{
    CLAUDE, butees_du_bac, butees_protegees, exclusions_de_l_etat, poser, sans_claude, temporaire_de,
};
use crate::branchements::git::{Arbre, BORNE_EMPREINTE, Gardee, empreinte_gardee, faire, geste};
use crate::branchements::srt::rejouer;
use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::fil::lot;
use crate::coeur::natif::TOUJOURS_REFUSES;
use crate::coeur::plan::{Plan, couvre};
use crate::coeur::profil::Profil;
use crate::coeur::texte::en_ligne;

/// Les chemins qu'un refus 7013 cite au plus, pour ce que la main a changé comme pour les `fichiers` de sa tâche :
/// au-delà, il dit le nombre des autres. Le refus tient dans l'erreur de la tentative, que le fil note et que la
/// relance lit.
const CITES_MAX: usize = 20;

/// La correction d'un chemin changé hors des `fichiers` de la tâche (7013), que la relance reçoit en contexte.
const HORS_FICHIERS: &str = "Put each of these paths back as in the base, or remove it, in this same sandbox: work is \
                             taken only if it changes only the paths of `fichiers`, a folder (ending in `/`) covering \
                             what it contains.";

/// La correction d'un chemin changé hors des `fichiers` de la tâche, vu au gel (7013) : la fusion n'a pas eu lieu, et
/// la tâche est refusée sans relance automatique.
const HORS_FICHIERS_GEL: &str = "The task is refused, and there is no automatic retry; the merge did not happen, and \
                                 the wave branch and the wave's worktree stay at their head. New work that changes \
                                 only the paths of `fichiers`, a folder (ending in `/`) covering what it contains, \
                                 will start again from this head.";

/// La correction d'un chemin protégé changé pendant une tentative (7018), que la relance reçoit en contexte.
const PROTEGE: &str = "Restore these paths as in the base, in this same sandbox, then run your proof again: a \
                       protected path changes only through its profile, which the human accepts. Do not change them \
                       by an edit, nor by a command.";

/// La correction d'un chemin protégé que le gel porterait (7018) : la fusion n'a pas eu lieu.
const PROTEGE_GEL: &str = "The task is refused and nothing of it is merged. Restore these paths as in the base, then \
                           run the task again: a protected path changes only through its profile, which the human \
                           accepts.";

/// La correction d'un `.claude` que le gel prendrait (7006), vu après `sans_claude` : la fusion n'a pas eu lieu, et la
/// tâche est refusée sans relance automatique.
const CLAUDE_GELE: &str = "The task is refused, and there is no automatic retry; the merge did not happen, and the \
                           wave branch and the wave's worktree stay at their head. A setting, a hook, a command, a \
                           skill, an agent or a project memory is set by the human, outside the wave; new work that \
                           writes in no .claude folder will start again from this head.";

/// La correction d'un fichier d'ouverture que le gel porterait (7021) : la fusion n'a pas eu lieu, et la tâche est
/// refusée sans relance automatique.
const OUVERTURE_GELEE: &str = "The task is refused, and there is no automatic retry; the merge did not happen, and \
                               the wave branch and the wave's worktree stay at their head. A file that the editor, \
                               the agent or the shell runs when the branch is next opened (.mcp.json, .vscode, \
                               .idea, a shell or git configuration file) is set by the human, outside the wave; new \
                               work that writes none of them will start again from this head.";

/// La correction d'un fichier épinglé par `sha256` que le gel ne porte pas sous l'empreinte du plan (7022) : la fusion
/// n'a pas eu lieu, et la tâche est refusée sans relance automatique.
const EMPREINTE_GELEE: &str = "The task is refused, and there is no automatic retry; the merge did not happen, and \
                               nothing of it is merged. Run the task again in a fresh wave. The SHA-256 to pin is \
                               that of the bytes git stores, which in a repository that normalizes line endings are \
                               not those on the disk.";

/// La correction d'un git qui ne dit pas ce que la main a changé (7009).
const SANS_LISTE: &str = "Check that git runs and that the sandbox is a worktree of the target repository: without \
                          knowing what the hand changed, its attempt cannot be taken.";

/// La correction d'un bac du découpeur ou du relecteur qui porte autre chose que son fichier (7014), que la relance
/// reçoit en contexte.
const SEUL: &str = "Write your file, and nothing else, at the root of the sandbox, as an ordinary file that the \
                    target repository does not ignore; put every other path back as in the base, or remove it, and \
                    commit nothing.";

/// La correction d'une base rouge (6022).
const BASE_ROUGE: &str = "Give the wave a proof that passes on the base; to repair the base, give the repair task its \
                          own proof, then run again.";

/// Tout dossier `.claude` d'un bac, à toute profondeur et en toute casse, ce qui est sous lui comme l'entrée elle-même,
/// ôtés d'un geste de git : ce que la main y change est à 7006, qui ne fige ni ne relance ce bac.
const SOUS_CLAUDE: [&str; 2] = [":(exclude,icase,glob)**/.claude/**", ":(exclude,icase,glob)**/.claude"];

/// La correction d'un assemblage fermé (7010).
const FERME: &str = "Nothing more is merged in this wave, and its branch stays at its last head: look at the wave's \
                     worktree and its locks (index.lock), then run the unassembled tasks again in a fresh wave.";

/// La correction d'un geste git raté dans la vague (7009).
const RATE: &str = "The wave branch did not advance for this task: check that git runs and that the sandbox and the \
                    wave's worktree are worktrees of the target repository.";

/// L'assemblage ouvert d'une vague : son worktree, sa branche et sa tête, et la preuve qu'il rejoue après chaque
/// fusion.
pub(crate) struct Assemblage<'a> {
    depot: PathBuf,
    /// Le dossier de Kraken, où vivent les journaux de la preuve.
    dossier: PathBuf,
    profil: &'a Profil,
    vague: String,
    preuve: String,
    worktree: PathBuf,
    branche: String,
    /// Le sha de la branche : la base, puis la dernière fusion où la branche a avancé.
    tete: String,
    /// Ce qui a fermé l'assemblage, un retour à la tête raté : plus rien ne s'y fusionne.
    fermeture: Option<String>,
}

/// Ouvre l'assemblage de la vague `vague` sur la base (un sha) : la branche `lydite/vague-<v>` sur elle, puis le
/// worktree `<dossier>/bacs/vague-<v>`, détaché sur elle, où la preuve de la vague se joue (journal
/// `verif-<v>-base.tmp`, effacé ; voir [`eprouver`]). Rouge, le worktree et la branche se retirent, et c'est le refus
/// 6022 : éprouver la base protège l'imputation, car un vert d'assemblage ne se doit alors qu'aux tâches. Un geste git
/// raté avant la preuve est le refus 6006, qui porte la commande et la sortie de git ; ce que ce geste a posé se
/// retire. Rien n'est noté ici.
pub(crate) fn ouvrir<'a>(
    depot: &Path,
    dossier: &Path,
    profil: &'a Profil,
    vague: &str,
    base: &str,
    preuve: &str,
) -> Result<Assemblage<'a>, Refus> {
    let assemblage = Assemblage {
        depot: depot.into(),
        dossier: dossier.into(),
        profil,
        vague: vague.into(),
        preuve: preuve.into(),
        worktree: dossier.join("bacs").join(format!("vague-{vague}")),
        branche: format!("lydite/vague-{vague}"),
        tete: base.into(),
        fermeture: None,
    };
    let rate = |vu: String| {
        let correction = format!(
            "Check that git runs in the target repository, and that neither the branch {} nor the folder {} exists, \
             then run again.",
            assemblage.branche,
            assemblage.worktree.display()
        );
        Refus::new(Code::EntreeSortie, vu, correction)
    };
    // La branche d'abord : si elle existe déjà, rien n'est posé, et rien de ce qui n'est pas à cette vague ne se
    // retire.
    faire(geste(depot, &["branch", &assemblage.branche, base])).map_err(rate)?;
    if let Err(vu) = poser(depot, &assemblage.worktree, &["--detach"], base) {
        let retrait = assemblage.effacer_branche().err().map_or_else(String::new, |echec| format!("\nthen {echec}"));
        return Err(rate(format!("{vu}{retrait}")));
    }
    let journal = dossier.join(format!("verif-{vague}-base.tmp"));
    if let Err(mut refus) = eprouver(profil, preuve, &assemblage.worktree, &journal, base) {
        if let Err(echec) = assemblage.retirer() {
            refus.preuve = format!("{}\nthen {echec}", refus.preuve);
        }
        return Err(refus);
    }
    Ok(assemblage)
}

/// L'épreuve de la base : la preuve de la vague, jouée dans `worktree`, posé sur `base`, sa sortie par `journal`,
/// effacé. Rouge, c'est le refus 6022.
fn eprouver(profil: &Profil, preuve: &str, worktree: &Path, journal: &Path, base: &str) -> Result<(), Refus> {
    let (_, rejeu) = rejouer(profil, preuve, worktree, journal);
    rejeu.map_err(|erreur| {
        Refus::new(Code::BaseRouge, format!("the wave's proof, played on the base {base}: {erreur}"), BASE_ROUGE)
    })
}

/// Éprouve la base avant le découpeur, par la même épreuve que `ouvrir` ([`eprouver`]) : un worktree détaché sur elle,
/// `<dossier>/bacs/base-<lot>`, où la preuve de la vague se joue (journal `verif-<lot>-base.tmp`, effacé), puis qui se
/// retire de force, avec ce que la preuve y a laissé : il n'est qu'à Kraken. Rouge, c'est le refus 6022, et aucune main
/// ne part. Un dossier de Kraken qui ne se crée pas, un geste git raté, ou un worktree qui ne se retire pas après une
/// épreuve verte, est le refus 6006, qui porte sa commande et la sortie de git. Rien n'est noté ici.
// raccourci : la base s'éprouve deux fois, ici puis à l'ouverture de la vague du plan, ce qui coûte des secondes de
// CPU et non des jetons ; monter vers une épreuve gardée par sha si elle pèse
pub(crate) fn eprouver_la_base(
    depot: &Path,
    dossier: &Path,
    profil: &Profil,
    base: &str,
    preuve: &str,
) -> Result<(), Refus> {
    fs::create_dir_all(dossier).map_err(|e| nommee(e, format!("creating the state folder {}", dossier.display())))?;
    let lot = lot();
    let worktree = dossier.join("bacs").join(format!("base-{lot}"));
    let rate = |vu: String| {
        let correction = format!(
            "Check that git runs in the target repository, and that the folder {} does not exist, then run again.",
            worktree.display()
        );
        Refus::new(Code::EntreeSortie, vu, correction)
    };
    poser(depot, &worktree, &["--detach"], base).map_err(rate)?;
    let epreuve = eprouver(profil, preuve, &worktree, &dossier.join(format!("verif-{lot}-base.tmp")), base);
    let mut retrait = geste(depot, &["worktree", "remove", "--force"]);
    retrait.arg(&worktree);
    let echecs: Vec<String> =
        [faire(retrait).map(drop), effacer_le_temporaire(&worktree)].into_iter().filter_map(Result::err).collect();
    let retrait = if echecs.is_empty() { Ok(()) } else { Err(echecs.join("\n")) };
    match (epreuve, retrait) {
        (Ok(()), Ok(())) => Ok(()),
        (Ok(()), Err(echec)) => Err(rate(echec)),
        (Err(refus), Ok(())) => Err(refus),
        (Err(mut refus), Err(echec)) => {
            refus.preuve = format!("{}\nthen {echec}", refus.preuve);
            Err(refus)
        }
    }
}

fn effacer_le_temporaire(worktree: &Path) -> Result<(), String> {
    let Some(temporaire) = temporaire_de(worktree).filter(|t| t.exists()) else {
        return Ok(());
    };
    fs::remove_dir_all(&temporaire).map_err(|e| format!("removing {}: {e}", temporaire.display()))
}

impl Assemblage<'_> {
    pub(crate) fn branche(&self) -> &str {
        &self.branche
    }

    /// Le sha de la tête de la branche : la base, puis la dernière fusion où la branche a avancé. Une tâche dépendante
    /// part de là.
    pub(crate) fn tete(&self) -> &str {
        &self.tete
    }

    /// Assemble la prise de la tâche `id`, numéro `k`, dont le bac est parti de `depart` et dont les `fichiers` bornent
    /// le travail (`None` : rien ne borne). Dans l'ordre :
    ///
    /// 1. `sans_claude` sur le bac (7006) ;
    /// 2. le retour du worktree de la vague à la tête ; raté, l'assemblage se ferme (7010) ;
    /// 3. le gel, dans le bac : tout son travail, hors de l'état de Claude Code, dans un commit `<id>` s'il y en a ;
    /// 4. ce que le gel prend ou porte dans son histoire sous un dossier `.claude`, que la tâche ait des `fichiers` ou
    ///    non (7006) : un chemin vu refuse sans relance, et la fusion n'a pas lieu ;
    /// 5. les chemins changés entre `depart` et le gel, jugés contre les `fichiers` (7013) : un chemin de trop refuse
    ///    sans relance, et la fusion n'a pas lieu ;
    /// 6. `merge --no-ff --no-log` du `HEAD` du bac, et non du nom de sa branche : une main qui a changé de branche est
    ///    assemblée avec ce qu'elle a laissé ; `--no-log` parce que `merge.log` ajouterait la liste des commits
    ///    fusionnés au message. En conflit, c'est le refus 7007, qui nomme les fichiers ;
    /// 7. la preuve de la vague, rejouée dans le worktree (journal `verif-<v>-fusion-<k>.tmp`, effacé) ; rouge, 7008 ;
    /// 8. `noter(sha, verif)`, qui note la fusion au fil ; ratée, 7012 ; puis la branche avance par `update-ref` sur
    ///    son ancienne tête ; raté après la note, 7009, dont le `vague.refus` suit la note.
    ///
    /// Sur un refus, le worktree revient à la tête, et la branche n'a pas bougé ; si ce retour rate, l'assemblage se
    /// ferme, et c'est le refus 7010. Fermé, il refuse toute prise en 7010. Un autre échec de git est le refus 7009.
    /// Rend le `verif` de la preuve de la vague (`None` si elle n'a pas tourné), et le sha de la nouvelle tête ou le
    /// refus.
    // raccourci : une tâche en conflit (7007) ou à preuve rouge (7008) est refusée sans relance ; monter vers une
    // relance sur la tête de la vague, l'erreur en contexte, si le besoin s'en montre
    #[cfg(test)]
    pub(crate) fn fusionner(
        &mut self,
        bac: &Path,
        depart: &str,
        id: &str,
        fichiers: Option<&[String]>,
        k: usize,
        noter: impl FnOnce(&str, &str) -> io::Result<()>,
    ) -> (Option<String>, Result<String, Refus>) {
        self.fusionner_et_noter_le_gel(bac, depart, id, fichiers, k, &[], |_| Ok(()), |_, _| Ok(()), noter)
    }

    /// `fusionner`, avec `noter_gel(gel)` appelé juste après le gel du bac (étape 3), avant tout jugement du gel et
    /// avant la fusion : le commit gelé est celui sur lequel porte le verdict de la main. Raté, 7012, comme la note
    /// de la fusion. `epingles` sont celles de tout le plan (voir [`epingles_du_plan`]) : celles de la tâche `id` et
    /// celles des autres, que le gel contrôle aussi (étape 5 bis). Tous les jugements passés, et seulement si la tâche
    /// a des épingles, `noter_empreintes(gel, fichiers)` se joue juste avant la fusion, `fichiers` étant un
    /// `<fichier> <empreinte>` par épingle de la tâche ; raté, 7012.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fusionner_et_noter_le_gel(
        &mut self,
        bac: &Path,
        depart: &str,
        id: &str,
        fichiers: Option<&[String]>,
        k: usize,
        epingles: &[Epingle<'_>],
        noter_gel: impl FnOnce(&str) -> io::Result<()>,
        noter_empreintes: impl FnOnce(&str, Vec<String>) -> io::Result<()>,
        noter: impl FnOnce(&str, &str) -> io::Result<()>,
    ) -> (Option<String>, Result<String, Refus>) {
        if let Some(fermeture) = &self.fermeture {
            let preuve =
                format!("the assembly of wave {} is closed, and {id} does not enter it: {fermeture}", self.vague);
            return (None, Err(Refus::new(Code::AssemblageFerme, preuve, FERME)));
        }
        if let Err(refus) = sans_claude(bac, depart).and_then(|()| self.revenir(&format!("before the merge of {id}"))) {
            return (None, Err(refus));
        }
        let (verif, issue) = self.assembler(bac, depart, id, fichiers, k, epingles, noter_gel, noter_empreintes, noter);
        let issue = match issue {
            Ok(sha) => {
                self.tete.clone_from(&sha);
                Ok(sha)
            }
            Err(refus) => {
                let pourquoi = format!("after refusal {} of {id} ({})", refus.code, refus.preuve);
                Err(self.revenir(&pourquoi).err().unwrap_or(refus))
            }
        };
        (verif, issue)
    }

    /// Les étapes 3 à 8 de `fusionner`, depuis le worktree de la vague revenu à sa tête.
    #[allow(clippy::too_many_arguments)]
    fn assembler(
        &self,
        bac: &Path,
        depart: &str,
        id: &str,
        fichiers: Option<&[String]>,
        k: usize,
        epingles: &[Epingle<'_>],
        noter_gel: impl FnOnce(&str) -> io::Result<()>,
        noter_empreintes: impl FnOnce(&str, Vec<String>) -> io::Result<()>,
        noter: impl FnOnce(&str, &str) -> io::Result<()>,
    ) -> (Option<String>, Result<String, Refus>) {
        let rate = |vu: String| Refus::new(Code::GitImpossible, vu, RATE);
        let proteges = self.profil.proteges();
        let gel = match geler(bac, depart, proteges, id) {
            Ok(gel) => gel,
            Err(vu) => return (None, Err(rate(vu))),
        };
        if let Err(erreur) = noter_gel(&gel) {
            let preuve = format!("the freeze of {id} ({gel}) is not noted: {erreur}");
            let correction = "Nothing is merged and the wave stops: make the thread writable, then run the \
                              unassembled tasks again.";
            return (None, Err(Refus::new(Code::FilInjoignable, preuve, correction)));
        }
        // `sans_claude` a jugé le bac avant le gel : ce qu'un processus resté derrière la main y écrit ou y commite
        // après lui, sous un dossier `.claude`, entrerait dans la branche sans ce second jugement, que la tâche ait des
        // `fichiers` ou non (une vague sans plan reste ouverte à 7013).
        if let Err(refus) = claude_gele(bac, depart, id, &gel).and_then(|()| ouverture_gelee(bac, depart, id, &gel)) {
            return (None, Err(refus));
        }
        // Le gel est ce que la branche recevra : ses chemins sont jugés contre les `fichiers` de la tâche, même si le
        // contrôle de fin de tentative ne les a pas vus.
        // Un chemin protégé l'emporte sur les `fichiers`, et borne aussi une tâche qui n'en a pas.
        if fichiers.is_some() || !proteges.is_empty() {
            let changes = match chemins_geles(bac, depart, &gel) {
                Ok(changes) => changes,
                Err(refus) => return (None, Err(refus)),
            };
            if let Err(refus) = protegees_changees(&changes, id, proteges, PROTEGE_GEL) {
                return (None, Err(refus));
            }
            if let Some(fichiers) = fichiers
                && let Err(refus) = hors_des_fichiers(changes, id, fichiers, HORS_FICHIERS_GEL)
            {
                return (None, Err(refus));
            }
        }
        // Les épingles `sha256` se jugent sur le commit gelé, que la tâche ait des `fichiers` ou non : le dernier
        // jugement du gel, avant la fusion.
        let lues = match epingles_gelees(bac, depart, id, &gel, epingles) {
            Ok(lues) => lues,
            Err(refus) => return (None, Err(refus)),
        };
        if !lues.is_empty()
            && let Err(erreur) = noter_empreintes(&gel, lues)
        {
            let preuve = format!("the pins of {id} ({gel}) are not noted: {erreur}");
            let correction = "Nothing is merged and the wave stops: make the thread writable, then run the \
                              unassembled tasks again.";
            return (None, Err(Refus::new(Code::FilInjoignable, preuve, correction)));
        }
        if let Err(vu) = faire(geste(&self.worktree, &["merge", "-q", "--no-ff", "--no-log", "-m", id, &gel])) {
            let en_conflit = faire(geste(&self.worktree, &["diff", "--name-only", "--diff-filter=U"]));
            let fichiers: Vec<String> = en_conflit.unwrap_or_default().lines().map(str::to_string).collect();
            if fichiers.is_empty() {
                return (None, Err(rate(vu)));
            }
            let preuve = format!("the merge of {id} on head {} is in conflict: {}", self.tete, fichiers.join(", "));
            let correction = "The task is refused without a retry, and the wave branch did not move: run it again on \
                              the head of the wave, or make the tasks that touch these files depend on one another.";
            return (None, Err(Refus::new(Code::Conflit, preuve, correction)));
        }
        let sha = match faire(geste(&self.worktree, &["rev-parse", "HEAD"])) {
            Ok(sha) => sha.trim().to_string(),
            Err(vu) => return (None, Err(rate(vu))),
        };
        let journal = self.dossier.join(format!("verif-{}-fusion-{k}.tmp", self.vague));
        let (verif, rejeu) = rejouer(self.profil, &self.preuve, &self.worktree, &journal);
        if let Err(erreur) = rejeu {
            let preuve = format!("the wave's proof, replayed after the merge of {id}: {erreur}");
            let correction = "The task is refused without a retry, and the wave branch did not move: read the end of \
                              the proof's output, then run the task again on the head of the wave.";
            return (verif, Err(Refus::new(Code::PreuveRouge, preuve, correction)));
        }
        if let Err(erreur) = noter(&sha, verif.as_deref().unwrap_or_default()) {
            let preuve = format!("the merge of {id} ({sha}) is not noted: {erreur}");
            let correction = "The merge is undone and the wave stops: make the thread writable, then run the \
                              unassembled tasks again.";
            return (verif, Err(Refus::new(Code::FilInjoignable, preuve, correction)));
        }
        let reference = format!("refs/heads/{}", self.branche);
        if let Err(vu) = faire(geste(&self.depot, &["update-ref", &reference, &sha, &self.tete])) {
            return (verif, Err(rate(format!("the merge of {id} is noted, but {vu}"))));
        }
        (verif, Ok(sha))
    }

    /// Ramène le worktree de la vague à la tête de la branche, `pourquoi` disant à quel moment : `reset --hard`, puis
    /// `clean -fd`. S'il ne le peut, l'assemblage se ferme, et c'est le refus 7010.
    fn revenir(&mut self, pourquoi: &str) -> Result<(), Refus> {
        let reset = faire(geste(&self.worktree, &["reset", "-q", "--hard", &self.tete]));
        let Err(vu) = reset.and_then(|_| faire(geste(&self.worktree, &["clean", "-q", "-fd"]))) else {
            return Ok(());
        };
        let preuve = format!("the wave's worktree does not return to its head {}, {pourquoi}: {vu}", self.tete);
        self.fermeture = Some(preuve.clone());
        Err(Refus::new(Code::AssemblageFerme, preuve, FERME))
    }

    /// Retire le worktree de la vague, son dossier temporaire et sa branche, que `ouvrir` a posés : rien n'en reste
    /// sans trace quand la vague ne s'ouvre pas. Rend ce que git a refusé.
    pub(crate) fn retirer(self) -> Result<(), String> {
        let mut enleve = geste(&self.depot, &["worktree", "remove", "--force"]);
        enleve.arg(&self.worktree);
        let echecs: Vec<String> =
            [faire(enleve).map(drop), effacer_le_temporaire(&self.worktree), self.effacer_branche().map(drop)]
                .into_iter()
                .filter_map(Result::err)
                .collect();
        if echecs.is_empty() { Ok(()) } else { Err(echecs.join("\n")) }
    }

    /// Efface la branche de la vague, si elle est encore à sa tête : Kraken ne retire que ce qu'il a posé.
    fn effacer_branche(&self) -> Result<String, String> {
        faire(geste(&self.depot, &["update-ref", "-d", &format!("refs/heads/{}", self.branche), &self.tete]))
    }
}

/// Le gel d'un bac : tout son travail, hors de l'état de Claude Code, dans un commit à Kraken s'il y en a ; sans ces
/// exclusions, `add -A` figerait le worktree d'un sous-agent en dépôt imbriqué (mode 160000). Rend le `HEAD` du bac,
/// que la fusion prend. Les `proteges` et `depart` disent quelles butées du mur à un chemin protégé absent ne sont
/// pas à commiter ([`butees_du_bac`]).
fn geler(bac: &Path, depart: &str, proteges: &[String], titre: &str) -> Result<String, String> {
    let mut ajout = geste(bac, &["add", "-A", "--", "."]);
    ajout.args(exclusions_de_l_etat());
    // Les butées du mur que la main a laissées derrière elle : ni écrites par elle, ni à commiter.
    ajout.args(butees_du_bac(bac, depart, proteges).into_iter().map(|chemin| format!(":(exclude,literal){chemin}")));
    faire(ajout)?;
    if !faire(geste(bac, &["diff", "--cached", "--name-only"]))?.trim().is_empty() {
        faire(geste(bac, &["commit", "-q", "-m", titre]))?;
    }
    Ok(faire(geste(bac, &["rev-parse", "HEAD"]))?.trim().to_string())
}

/// Les chemins sous un dossier `.claude` ([`CLAUDE`]) que le gel ferait entrer dans la branche de la vague : ce que
/// `sans_claude` a laissé passer, écrit ou commité dans le bac après lui. Deux gestes, comme `sans_claude` : le diff
/// des arbres entre `depart` et `gel`, qui voit un `.claude` encore là au gel, et le journal de `depart..gel`, qui voit
/// un `.claude` commité puis ôté, que l'arbre ne porte plus mais dont la fusion apporterait le commit. Un nom vu est le
/// refus 7006, qui les nomme, et la fusion n'a pas lieu ; un échec de git est le refus 7009, comme pour
/// [`chemins_geles`] : une liste vide ne vaut « rien sous `.claude` » que si git a réussi.
fn claude_gele(bac: &Path, depart: &str, id: &str, gel: &str) -> Result<(), Refus> {
    let vus = vus_au_gel(bac, depart, gel, &CLAUDE)?;
    if vus.is_empty() {
        return Ok(());
    }
    Err(Refus::new(
        Code::ClaudeTouche,
        format!(
            "the freeze of task {id} would carry, under a .claude folder, paths that the merge would bring into the \
             wave branch: {}",
            citer(&vus)
        ),
        CLAUDE_GELE,
    ))
}

/// Les chemins que le gel ferait entrer dans la branche de la vague sous un nom de [`TOUJOURS_REFUSES`] (`.mcp.json`,
/// `.vscode`, `.idea`, les fichiers de shell et de git de la racine) : ce que l'éditeur, l'agent ou le shell exécute à
/// la prochaine ouverture de la branche. Le mur de la preuve ne ferme que ceux qui existent déjà : un fichier neuf,
/// que `add -A` commite, ne passerait que par ce jugement. Les noms valent à la racine du bac, en toute casse, que la
/// tâche ait des `fichiers` ou non et même s'ils les couvrent ; comme [`claude_gele`], il voit l'arbre du gel et
/// l'histoire de `depart..gel`. Un nom vu est le refus 7021, qui les nomme, et la fusion n'a pas lieu ; un échec de
/// git est le refus 7009.
fn ouverture_gelee(bac: &Path, depart: &str, id: &str, gel: &str) -> Result<(), Refus> {
    let noms: Vec<String> = TOUJOURS_REFUSES.iter().map(|nom| format!(":(icase,literal){nom}")).collect();
    let vus = vus_au_gel(bac, depart, gel, &noms)?;
    if vus.is_empty() {
        return Ok(());
    }
    Err(Refus::new(
        Code::OuvertureExecutable,
        format!(
            "the freeze of task {id} would carry paths that run when the branch is next opened, and that the merge \
             would bring into the wave branch: {}",
            citer(&vus)
        ),
        OUVERTURE_GELEE,
    ))
}

/// Les chemins, sous `pathspecs`, que le gel apporte à la branche : le diff des arbres entre `depart` et `gel`, qui
/// voit ce qui est encore là au gel, et le journal de `depart..gel`, qui voit ce qui fut commité puis ôté, que l'arbre
/// ne porte plus mais dont la fusion apporterait le commit. Triés, chacun une fois ; un échec de git est le refus
/// 7009 : une liste vide ne vaut « rien » que si git a réussi.
fn vus_au_gel(bac: &Path, depart: &str, gel: &str, pathspecs: &[impl AsRef<str>]) -> Result<Vec<String>, Refus> {
    let diff = geste(bac, &["diff", "--name-only", "-z", "--no-renames", depart, gel, "--"]);
    // `-m` : une fusion dit ses fichiers, que `--name-only` taît sans lui. `--full-history` : git n'élague aucun côté
    // d'une fusion. `--no-show-signature` : le `log.showSignature` du dépôt visé lancerait son `gpg.program` hors du
    // mur. `-z` : les chemins finissent par NUL, comme ceux du diff.
    let plage = format!("{depart}..{gel}");
    let journal = geste(
        bac,
        &["log", "--full-history", "-m", "-z", "--format=", "--name-only", "--no-show-signature", &plage, "--"],
    );
    let mut vus = Vec::new();
    for mut commande in [diff, journal] {
        commande.args(pathspecs.iter().map(AsRef::as_ref));
        let sortie = faire(commande).map_err(|vu| Refus::new(Code::GitImpossible, vu, SANS_LISTE))?;
        vus.extend(sortie.split('\0').filter(|entree| !entree.is_empty()).map(String::from));
    }
    vus.sort();
    vus.dedup();
    Ok(vus)
}

/// Les chemins que la main a changés dans son bac depuis `depart`, triés, chacun une fois : ce que le gel et la fusion
/// feraient entrer dans la branche de la vague. Deux gestes de git, tous deux sans détection des renommés, qui se
/// lisent alors comme un effacé et un ajouté : leurs deux noms comptent.
///
/// - `git status --porcelain=v1 -z --untracked-files=all` : le suivi modifié, ajouté ou effacé, dans l'index comme
///   dans l'arbre, et le non suivi que git n'ignore pas. Un fichier ignoré (`target/`) n'y est pas, car le gel ne le
///   prend pas non plus ;
/// - `git diff --name-only -z <depart> HEAD` : ce que les commits de la main ont changé, dont `status` ne dit rien.
///
/// Rien sous un dossier `.claude` (voir [`SOUS_CLAUDE`]). Ni la butée du mur à un chemin de `proteges` absent de
/// `depart` ([`butees_protegees`]), tant qu'elle est non suivie : commitée ou ajoutée à l'index, elle compte. Un
/// échec de git, qu'il ne parte pas ou qu'il refuse, est le refus 7009 : une liste vide ne vaut « rien changé » que si
/// git a réussi.
pub(crate) fn chemins_changes(bac: &Path, depart: &str, proteges: &[String]) -> Result<Vec<String>, Refus> {
    let statut = geste(bac, &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--", "."]);
    let commites = geste(bac, &["diff", "--name-only", "-z", "--no-renames", depart, "HEAD", "--", "."]);
    let butees = butees_protegees(bac, depart, proteges);
    let mut chemins = Vec::new();
    // Chaque entrée finit par NUL, sans guillemets ; celles de `status` commencent par deux lettres d'état et un blanc.
    for (mut commande, saut) in [(statut, 3), (commites, 0)] {
        commande.args(SOUS_CLAUDE);
        let sortie = faire(commande).map_err(|vu| Refus::new(Code::GitImpossible, vu, SANS_LISTE))?;
        let entrees = sortie.split('\0').filter(|entree| !entree.is_empty());
        // Le non suivi est `?? <chemin>` : seul lui laisse passer une butée.
        let butee =
            |entree: &&str| saut == 3 && entree.strip_prefix("?? ").is_some_and(|nom| butees.iter().any(|b| b == nom));
        chemins
            .extend(entrees.filter(|entree| !butee(entree)).filter_map(|entree| entree.get(saut..)).map(String::from));
    }
    chemins.sort();
    chemins.dedup();
    Ok(chemins)
}

/// Les chemins changés entre le départ du bac, `depart`, et son gel, `gel` : ce que la fusion ferait entrer dans la
/// branche de la vague. Un seul geste de git, sans détection des renommés, qui se lisent alors comme un effacé et un
/// ajouté : leurs deux noms comptent. Rien sous un dossier `.claude` (voir [`SOUS_CLAUDE`]). Un échec de git est le
/// refus 7009, comme pour [`chemins_changes`] : une liste vide ne vaut « rien changé » que si git a réussi.
fn chemins_geles(bac: &Path, depart: &str, gel: &str) -> Result<Vec<String>, Refus> {
    let mut diff = geste(bac, &["diff", "--name-only", "-z", "--no-renames", depart, gel, "--", "."]);
    diff.args(SOUS_CLAUDE);
    let sortie = faire(diff).map_err(|vu| Refus::new(Code::GitImpossible, vu, SANS_LISTE))?;
    let mut chemins: Vec<String> = sortie.split('\0').filter(|entree| !entree.is_empty()).map(String::from).collect();
    chemins.sort();
    chemins.dedup();
    Ok(chemins)
}

/// Le refus 7013 pour les chemins de `changes` que `fichiers` ne couvre pas ([`couvre`]), avec la `correction` de
/// l'endroit qui juge.
fn hors_des_fichiers(changes: Vec<String>, id: &str, fichiers: &[String], correction: &str) -> Result<(), Refus> {
    let hors: Vec<String> =
        changes.into_iter().filter(|chemin| !fichiers.iter().any(|entree| couvre(entree, chemin))).collect();
    if hors.is_empty() {
        return Ok(());
    }
    let preuve = format!("the hand of task {id} changed, outside its files ({}): {}", citer(fichiers), citer(&hors));
    Err(Refus::new(Code::HorsFichiers, preuve, correction))
}

/// Le refus 7018 pour les chemins de `changes` que `proteges` couvre ([`couvre`]), avec la `correction` de l'endroit
/// qui juge. Un dossier protégé (`entree/`) couvre aussi son propre chemin sans la barre : un fichier écrit à la place
/// du dossier, que `couvre` ne voit pas, est un changement du chemin protégé, sauf la butée du mur qui n'est pas dans
/// `changes` ([`chemins_changes`]).
fn protegees_changees(changes: &[String], id: &str, proteges: &[String], correction: &str) -> Result<(), Refus> {
    let couvert = |entree: &String, chemin: &String| {
        couvre(entree, chemin) || entree.strip_suffix('/').is_some_and(|dossier| dossier == chemin)
    };
    let touches: Vec<String> =
        changes.iter().filter(|chemin| proteges.iter().any(|entree| couvert(entree, chemin))).cloned().collect();
    if touches.is_empty() {
        return Ok(());
    }
    let preuve = format!("the hand of task {id} changed paths that its profile protects: {}", citer(&touches));
    Err(Refus::new(Code::CheminProtege, preuve, correction))
}

/// La main de la tâche `id` n'a changé aucun chemin que `proteges` couvre ([`couvre`]), vu à la fin d'une tentative,
/// qu'elle ait des `fichiers` ou non, et avant le jugement des `fichiers`. Un chemin protégé changé est le refus 7018 ;
/// git qui ne dit pas ce que la main a changé, le refus 7009. Sans chemin protégé, git n'est pas interrogé.
pub(crate) fn proteges_tenus(bac: &Path, depart: &str, id: &str, proteges: &[String]) -> Result<(), Refus> {
    if proteges.is_empty() {
        return Ok(());
    }
    protegees_changees(&chemins_changes(bac, depart, proteges)?, id, proteges, PROTEGE)
}

/// La main de la tâche `id` n'a changé que des chemins que `fichiers` couvre ([`couvre`]). La comparaison est exacte,
/// octet à octet, comme git rend les chemins : la casse compte sous Windows comme ailleurs, et un écart de casse n'est
/// jamais couvert. Un chemin de trop est le refus 7013, qui cite ces chemins et ceux de `fichiers` ; git qui ne dit pas
/// ce que la main a changé, le refus 7009. Les `proteges` du profil ne servent qu'à ne pas compter la butée du mur à un
/// chemin protégé absent ([`chemins_changes`]).
// raccourci : un descendant détaché de la main peut encore écrire après le gel, dans le bac, sans effet sur la branche
// qui ne reçoit que le gel, ou hors du bac, ce que seul le mur tient ; monter quand le nœud prouvera la mort de l'arbre
// d'une main
pub(crate) fn fichiers_tenus(
    bac: &Path,
    depart: &str,
    id: &str,
    fichiers: &[String],
    proteges: &[String],
) -> Result<(), Refus> {
    hors_des_fichiers(chemins_changes(bac, depart, proteges)?, id, fichiers, HORS_FICHIERS)
}

/// Un fichier que le plan épingle par son SHA-256 : la tâche qui l'épingle, le fichier, et l'empreinte attendue, celle
/// des octets que git garde (`git::empreinte_gardee`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Epingle<'a> {
    pub(crate) tache: &'a str,
    pub(crate) fichier: &'a str,
    pub(crate) empreinte: &'a str,
}

/// Les épingles de tout le plan, dans l'ordre des tâches puis des lignes `sha256`.
pub(crate) fn epingles_du_plan(plan: &Plan) -> Vec<Epingle<'_>> {
    let mut epingles = Vec::new();
    for tache in &plan.taches {
        let (id, pins) = (tache.id.as_str(), &tache.empreintes);
        epingles.extend(pins.iter().map(|(fichier, empreinte)| Epingle { tache: id, fichier, empreinte }));
    }
    epingles
}

/// Ce que git garde de l'épingle dans `arbre`, dit pour une erreur : `None` si c'est l'empreinte attendue, `Some(ce que
/// git garde)` sinon ; un git qui échoue est l'erreur. L'empreinte vue n'est jamais celle d'un fichier du bac.
fn ecart_d_epingle(bac: &Path, arbre: Arbre<'_>, epingle: &Epingle<'_>) -> Result<Option<String>, String> {
    Ok(match empreinte_gardee(bac, arbre, epingle.fichier, BORNE_EMPREINTE)? {
        Gardee::Empreinte(vue) if vue == epingle.empreinte => None,
        Gardee::Empreinte(vue) => Some(format!("git stores the SHA-256 {vue}")),
        Gardee::Absent => Some("git stores no such file".to_string()),
        Gardee::PasOrdinaire(mode) => Some(format!("git stores it as mode {mode}, which is not an ordinary file")),
        Gardee::TropLourd => Some(format!("git stores a file of more than {BORNE_EMPREINTE} bytes")),
    })
}

/// Les épingles que le contrôle d'un bac juge : celles de la tâche `id`, puis celles d'une autre tâche du plan dont le
/// fichier est dans `changes`, sans le fichier d'une épingle de `id` déjà prise. L'appelant n'a `changes` que s'il y a
/// une épingle d'une autre tâche.
fn epingles_a_juger<'a>(epingles: &[Epingle<'a>], id: &str, changes: &[String]) -> Vec<Epingle<'a>> {
    let mut jugees: Vec<Epingle<'a>> = epingles.iter().filter(|e| e.tache == id).copied().collect();
    for autre in epingles.iter().filter(|e| e.tache != id && changes.iter().any(|chemin| chemin == e.fichier)) {
        if !jugees.iter().any(|vue| vue.fichier == autre.fichier) {
            jugees.push(*autre);
        }
    }
    jugees
}

/// À la fin d'une tentative de la tâche `id`, les épingles `sha256` du plan tiennent dans l'index du bac : chaque
/// épingle de la tâche, puis chaque épingle d'une autre tâche dont la main a changé le fichier ([`chemins_changes`]).
/// Le fichier s'ajoute d'abord à l'index (`git add`, comme le gel le ferait), puis git dit ce qu'il en garde
/// (`git::empreinte_gardee`). Un fichier que git n'ajoute pas (absent, ignoré, derrière un lien symbolique), qui
/// n'est pas ordinaire, trop lourd ou d'une autre empreinte est une erreur qui nomme le fichier, l'empreinte attendue
/// et ce que git garde, que la relance reçoit ; un git qui échoue est une erreur de même.
pub(crate) fn epingles_tenues(
    bac: &Path,
    depart: &str,
    id: &str,
    epingles: &[Epingle<'_>],
    proteges: &[String],
) -> Result<(), String> {
    let changes = if epingles.iter().any(|e| e.tache != id) {
        chemins_changes(bac, depart, proteges)
            .map_err(|refus| format!("git does not say what the hand changed, to check the pins: {}", refus.preuve))?
    } else {
        Vec::new()
    };
    for epingle in epingles_a_juger(epingles, id, &changes) {
        let fichier = en_ligne(epingle.fichier);
        let qui = if epingle.tache == id { String::new() } else { format!(" (task {})", epingle.tache) };
        let mut ajout = geste(bac, &["add", "--", &format!(":(literal){}", epingle.fichier)]);
        ajout.args(exclusions_de_l_etat());
        faire(ajout).map_err(|vu| {
            format!("the file {fichier} that the plan pins by sha256{qui} is not added by git: {}", en_ligne(&vu))
        })?;
        let ecart = ecart_d_epingle(bac, Arbre::Index, &epingle)
            .map_err(|vu| format!("git does not say what it stores of {fichier}, pinned by sha256{qui}: {vu}"))?;
        if let Some(vu) = ecart {
            return Err(format!(
                "the file {fichier}, that the plan pins by sha256{qui} {}, is not as pinned: {vu}",
                epingle.empreinte
            ));
        }
    }
    Ok(())
}

/// Au gel de la tâche `id`, les épingles `sha256` du plan tiennent dans le commit gelé `gel` : chaque épingle de la
/// tâche, puis chaque épingle d'une autre tâche dont le gel a changé le fichier ([`chemins_geles`]). Rend les épingles
/// de la tâche lues, un `<fichier> <empreinte>` chacune, que le fil note. Un fichier absent du commit, qui n'est pas
/// ordinaire, trop lourd ou d'une autre empreinte est le refus 7022, qui nomme la tâche, le fichier et les deux
/// empreintes (ou ce qui manque), et la fusion n'a pas lieu ; un git qui échoue est le refus 7009.
fn epingles_gelees(
    bac: &Path,
    depart: &str,
    id: &str,
    gel: &str,
    epingles: &[Epingle<'_>],
) -> Result<Vec<String>, Refus> {
    let changes = if epingles.iter().any(|e| e.tache != id) { chemins_geles(bac, depart, gel)? } else { Vec::new() };
    let mut lues = Vec::new();
    for epingle in epingles_a_juger(epingles, id, &changes) {
        let fichier = en_ligne(epingle.fichier);
        let ecart = ecart_d_epingle(bac, Arbre::Commit(gel), &epingle).map_err(|vu| {
            let preuve = format!("{vu}\nthe file {fichier} is pinned by sha256, task {id}");
            Refus::new(Code::GitImpossible, preuve, SANS_LISTE)
        })?;
        if let Some(vu) = ecart {
            let qui = if epingle.tache == id { String::new() } else { format!(" of task {}", epingle.tache) };
            let preuve = format!(
                "the freeze of task {id} ({gel}) carries the file {fichier}, pinned by the plan{qui} under the \
                 SHA-256 {}, but {vu}",
                epingle.empreinte
            );
            return Err(Refus::new(Code::EmpreinteDifferente, preuve, EMPREINTE_GELEE));
        }
        if epingle.tache == id {
            lues.push(format!("{} {}", epingle.fichier, epingle.empreinte));
        }
    }
    Ok(lues)
}

/// Le bac d'une main qui n'écrit que son fichier, le découpeur ou le relecteur, parti de `depart` : `fichier` y est,
/// à sa racine, en fichier ordinaire (ni lien symbolique ni dossier), que le dépôt visé n'ignore pas
/// (`git check-ignore`) ; la main n'a rien commité ; et rien d'autre n'y a changé que lui et les chemins d'`aussi`,
/// suivi (`git diff HEAD`) comme non suivi et non ignoré (`git ls-files --others --exclude-standard`), hors de l'état
/// que Claude Code tient lui-même sous `.claude/` hors de git. Kraken regarde le fichier lui-même, car git ne dit rien
/// d'un fichier ignoré. Les chemins se comparent exactement, comme git les rend. Un écart est le refus 7014, qui cite
/// les chemins en trop ; un git qui ne répond pas, le refus 7009 : rien ne prouve alors que le bac est resté propre.
pub(crate) fn seul_change(bac: &Path, depart: &str, fichier: &str, aussi: &[&str]) -> Result<(), Refus> {
    let touche = |preuve: String| Refus::new(Code::BacTouche, preuve, SEUL);
    let rate = |vu: String| Refus::new(Code::GitImpossible, vu, SANS_LISTE);
    match fs::symlink_metadata(bac.join(fichier)) {
        Ok(lu) if lu.is_file() => {}
        Ok(lu) => {
            let quoi = if lu.file_type().is_symlink() { "a symbolic link" } else { "a folder" };
            return Err(touche(format!("{fichier}, at the root of the sandbox, is {quoi}, and not an ordinary file")));
        }
        Err(e) => return Err(touche(format!("{fichier} is not at the root of the sandbox: {e}"))),
    }
    let ignore = geste(bac, &["check-ignore", "-q", "--", fichier])
        .output()
        .map_err(|e| rate(format!("cannot launch git check-ignore -q -- {fichier} in {}: {e}", bac.display())))?;
    match ignore.status.code() {
        Some(1) => {}
        Some(0) => {
            let preuve = format!(
                "the target repository ignores {fichier} (git check-ignore), which is therefore not the sandbox's work"
            );
            return Err(touche(preuve));
        }
        _ => {
            let dit = String::from_utf8_lossy(&ignore.stderr);
            let vu = format!(
                "git check-ignore -q -- {fichier} in {} failed ({}): {}",
                bac.display(),
                ignore.status,
                dit.trim()
            );
            return Err(rate(vu));
        }
    }
    let tete = faire(geste(bac, &["rev-parse", "HEAD"])).map_err(rate)?;
    if tete.trim() != depart {
        return Err(touche(format!(
            "the hand committed in its sandbox: its head is {}, and its start {depart}",
            tete.trim()
        )));
    }
    let suivi = geste(bac, &["diff", "--name-only", "-z", "--no-renames", "HEAD", "--", "."]);
    let mut non_suivi = geste(bac, &["ls-files", "--others", "--exclude-standard", "-z", "--", "."]);
    non_suivi.args(exclusions_de_l_etat());
    let mut hors = Vec::new();
    for commande in [suivi, non_suivi] {
        let sortie = faire(commande).map_err(rate)?;
        let autres =
            sortie.split('\0').filter(|chemin| !chemin.is_empty() && *chemin != fichier && !aussi.contains(chemin));
        hors.extend(autres.map(String::from));
    }
    if hors.is_empty() {
        return Ok(());
    }
    hors.sort();
    hors.dedup();
    Err(touche(format!("the hand changed in its sandbox, besides {fichier}: {}", citer(&hors))))
}

/// Ces chemins entre guillemets, `CITES_MAX` au plus, puis le nombre des autres. Chacun s'écrit comme un texte de Rust,
/// ses caractères de contrôle échappés : il vient d'une main ou d'un plan, et peut piéger un terminal.
fn citer(chemins: &[String]) -> String {
    let cites = chemins.iter().take(CITES_MAX).map(|chemin| format!("{chemin:?}")).collect::<Vec<_>>().join(", ");
    match chemins.len().checked_sub(CITES_MAX) {
        Some(reste) if reste > 0 => format!("{cites}, and {reste} more"),
        _ => cites,
    }
}

#[cfg(test)]
mod tests;
