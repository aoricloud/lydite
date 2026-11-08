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
//! chemin est refusé sans relance, et la fusion n'a pas lieu. Le découpeur et le relecteur, qui n'écrivent que leur
//! fichier, sont tenus par `seul_change` (7014).
//!
//! La base s'éprouve par la preuve de la vague (6022) à l'ouverture de l'assemblage, et, pour un découpage, avant le
//! découpeur (`eprouver_la_base`), par la même épreuve mise à part (`eprouver`).
//!
//! Ce module ne connaît pas les événements du fil : la note d'une fusion lui vient par une fermeture.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::branchements::bac::{CLAUDE, butees_du_bac, exclusions_de_l_etat, poser, sans_claude, temporaire_de};
use crate::branchements::git::{faire, geste};
use crate::branchements::srt::rejouer;
use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::fil::lot;
use crate::coeur::plan::couvre;
use crate::coeur::profil::Profil;

/// Les chemins qu'un refus 7013 cite au plus, pour ce que la main a changé comme pour les `fichiers` de sa tâche :
/// au-delà, il dit le nombre des autres. Le refus tient dans l'erreur de la tentative, que le fil note et que la
/// relance lit.
const CITES_MAX: usize = 20;

/// La correction d'un chemin changé hors des `fichiers` de la tâche (7013), que la relance reçoit en contexte.
const HORS_FICHIERS: &str = "Remets chacun de ces chemins comme dans la base, ou retire-le, dans ce même bac : un \
                             travail n'est pris que s'il ne change que les chemins de `fichiers`, un dossier (fini par \
                             `/`) couvrant ce qu'il contient.";

/// La correction d'un chemin changé hors des `fichiers` de la tâche, vu au gel (7013) : la fusion n'a pas eu lieu, et
/// la tâche est refusée sans relance automatique.
const HORS_FICHIERS_GEL: &str = "La tâche est refusée, et il n'y a pas de relance automatique ; la fusion n'a pas eu \
                                 lieu, et la branche comme le worktree de la vague restent à leur tête. Un nouveau \
                                 travail qui ne change que les chemins de `fichiers`, un dossier (fini par `/`) \
                                 couvrant ce qu'il contient, repartira de cette tête.";

/// La correction d'un chemin protégé changé pendant une tentative (7018), que la relance reçoit en contexte.
const PROTEGE: &str = "Restore these paths as in the base, in this same bac, then run your proof again: a protected \
                       path changes only through its profile, which the human accepts. Do not change them by an edit, \
                       nor by a command.";

/// La correction d'un chemin protégé que le gel porterait (7018) : la fusion n'a pas eu lieu.
const PROTEGE_GEL: &str = "The task is refused and nothing of it is merged. Restore these paths as in the base, then \
                           run the task again: a protected path changes only through its profile, which the human \
                           accepts.";

/// La correction d'un `.claude` que le gel prendrait (7006), vu après `sans_claude` : la fusion n'a pas eu lieu, et la
/// tâche est refusée sans relance automatique.
const CLAUDE_GELE: &str = "La tâche est refusée, et il n'y a pas de relance automatique ; la fusion n'a pas eu \
                           lieu, et la branche comme le worktree de la vague restent à leur tête. Un réglage, un \
                           crochet, une commande, une compétence, un agent ou une mémoire de projet se pose par \
                           l'humain, hors de la vague ; un nouveau travail qui n'écrit dans aucun dossier .claude \
                           repartira de cette tête.";

/// La correction d'un git qui ne dit pas ce que la main a changé (7009).
const SANS_LISTE: &str = "Vérifie que git se lance et que le bac est un worktree du dépôt visé : faute de savoir ce \
                          que la main a changé, sa tentative ne peut pas être prise.";

/// La correction d'un bac du découpeur ou du relecteur qui porte autre chose que son fichier (7014), que la relance
/// reçoit en contexte.
const SEUL: &str = "Écris ton fichier, et rien d'autre, à la racine du bac, en fichier ordinaire que le dépôt visé \
                    n'ignore pas ; remets comme dans la base tout autre chemin, ou retire-le, et ne commite rien.";

/// La correction d'une base rouge (6022).
const BASE_ROUGE: &str = "Donne à la vague une preuve qui passe sur la base ; pour réparer la base, donne à la tâche \
                          de réparation sa propre preuve, puis relance.";

/// Tout dossier `.claude` d'un bac, à toute profondeur et en toute casse, ce qui est sous lui comme l'entrée elle-même,
/// ôtés d'un geste de git : ce que la main y change est à 7006, qui ne fige ni ne relance ce bac.
const SOUS_CLAUDE: [&str; 2] = [":(exclude,icase,glob)**/.claude/**", ":(exclude,icase,glob)**/.claude"];

/// La correction d'un assemblage fermé (7010).
const FERME: &str = "Plus rien ne se fusionne dans cette vague, et sa branche reste à sa dernière tête : regarde le \
                     worktree de la vague et ses verrous (index.lock), puis relance les tâches non assemblées dans une \
                     vague neuve.";

/// La correction d'un geste git raté dans la vague (7009).
const RATE: &str = "La branche de la vague n'a pas avancé pour cette tâche : vérifie que git se lance et que le bac et \
                    le worktree de la vague sont des worktrees du dépôt visé.";

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
            "Vérifie que git se lance dans le dépôt visé, et que ni la branche {} ni le dossier {} n'existent, puis \
             relance.",
            assemblage.branche,
            assemblage.worktree.display()
        );
        Refus::new(Code::EntreeSortie, vu, correction)
    };
    // La branche d'abord : si elle existe déjà, rien n'est posé, et rien de ce qui n'est pas à cette vague ne se
    // retire.
    faire(geste(depot, &["branch", &assemblage.branche, base])).map_err(rate)?;
    if let Err(vu) = poser(depot, &assemblage.worktree, &["--detach"], base) {
        let retrait = assemblage.effacer_branche().err().map_or_else(String::new, |echec| format!("\npuis {echec}"));
        return Err(rate(format!("{vu}{retrait}")));
    }
    let journal = dossier.join(format!("verif-{vague}-base.tmp"));
    if let Err(mut refus) = eprouver(profil, preuve, &assemblage.worktree, &journal, base) {
        if let Err(echec) = assemblage.retirer() {
            refus.preuve = format!("{}\npuis {echec}", refus.preuve);
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
        Refus::new(Code::BaseRouge, format!("la preuve de la vague, jouée sur la base {base} : {erreur}"), BASE_ROUGE)
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
    fs::create_dir_all(dossier)
        .map_err(|e| nommee(e, format!("création du dossier de Lydite {}", dossier.display())))?;
    let lot = lot();
    let worktree = dossier.join("bacs").join(format!("base-{lot}"));
    let rate = |vu: String| {
        let correction = format!(
            "Vérifie que git se lance dans le dépôt visé, et que le dossier {} n'existe pas, puis relance.",
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
            refus.preuve = format!("{}\npuis {echec}", refus.preuve);
            Err(refus)
        }
    }
}

fn effacer_le_temporaire(worktree: &Path) -> Result<(), String> {
    let Some(temporaire) = temporaire_de(worktree).filter(|t| t.exists()) else {
        return Ok(());
    };
    fs::remove_dir_all(&temporaire).map_err(|e| format!("effacement de {} : {e}", temporaire.display()))
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
        self.fusionner_et_noter_le_gel(bac, depart, id, fichiers, k, |_| Ok(()), noter)
    }

    /// `fusionner`, avec `noter_gel(gel)` appelé juste après le gel du bac (étape 3), avant tout jugement du gel et
    /// avant la fusion : le commit gelé est celui sur lequel porte le verdict de la main. Raté, 7012, comme la note
    /// de la fusion.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn fusionner_et_noter_le_gel(
        &mut self,
        bac: &Path,
        depart: &str,
        id: &str,
        fichiers: Option<&[String]>,
        k: usize,
        noter_gel: impl FnOnce(&str) -> io::Result<()>,
        noter: impl FnOnce(&str, &str) -> io::Result<()>,
    ) -> (Option<String>, Result<String, Refus>) {
        if let Some(fermeture) = &self.fermeture {
            let preuve =
                format!("l'assemblage de la vague {} est fermé, et {id} n'y entre pas : {fermeture}", self.vague);
            return (None, Err(Refus::new(Code::AssemblageFerme, preuve, FERME)));
        }
        if let Err(refus) = sans_claude(bac, depart).and_then(|()| self.revenir(&format!("avant la fusion de {id}"))) {
            return (None, Err(refus));
        }
        let (verif, issue) = self.assembler(bac, depart, id, fichiers, k, noter_gel, noter);
        let issue = match issue {
            Ok(sha) => {
                self.tete.clone_from(&sha);
                Ok(sha)
            }
            Err(refus) => {
                let pourquoi = format!("après le refus {} de {id} ({})", refus.code, refus.preuve);
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
        noter_gel: impl FnOnce(&str) -> io::Result<()>,
        noter: impl FnOnce(&str, &str) -> io::Result<()>,
    ) -> (Option<String>, Result<String, Refus>) {
        let rate = |vu: String| Refus::new(Code::GitImpossible, vu, RATE);
        let gel = match geler(bac, id) {
            Ok(gel) => gel,
            Err(vu) => return (None, Err(rate(vu))),
        };
        if let Err(erreur) = noter_gel(&gel) {
            let preuve = format!("le gel de {id} ({gel}) n'est pas noté : {erreur}");
            let correction = "Rien n'est fusionné et la vague s'arrête : rends le fil inscriptible, puis relance les \
                              tâches non assemblées.";
            return (None, Err(Refus::new(Code::FilInjoignable, preuve, correction)));
        }
        // `sans_claude` a jugé le bac avant le gel : ce qu'un processus resté derrière la main y écrit ou y commite
        // après lui, sous un dossier `.claude`, entrerait dans la branche sans ce second jugement, que la tâche ait des
        // `fichiers` ou non (une vague sans plan reste ouverte à 7013).
        if let Err(refus) = claude_gele(bac, depart, id, &gel) {
            return (None, Err(refus));
        }
        // Le gel est ce que la branche recevra : ses chemins sont jugés contre les `fichiers` de la tâche, même si le
        // contrôle de fin de tentative ne les a pas vus.
        // Un chemin protégé l'emporte sur les `fichiers`, et borne aussi une tâche qui n'en a pas.
        let proteges = self.profil.proteges();
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
        if let Err(vu) = faire(geste(&self.worktree, &["merge", "-q", "--no-ff", "--no-log", "-m", id, &gel])) {
            let en_conflit = faire(geste(&self.worktree, &["diff", "--name-only", "--diff-filter=U"]));
            let fichiers: Vec<String> = en_conflit.unwrap_or_default().lines().map(str::to_string).collect();
            if fichiers.is_empty() {
                return (None, Err(rate(vu)));
            }
            let preuve =
                format!("la fusion de {id} sur la tête {} est en conflit : {}", self.tete, fichiers.join(", "));
            let correction = "La tâche est refusée sans relance, et la branche de la vague n'a pas bougé : relance-la \
                              sur la tête de la vague, ou fais dépendre l'une de l'autre les tâches qui touchent ces \
                              fichiers.";
            return (None, Err(Refus::new(Code::Conflit, preuve, correction)));
        }
        let sha = match faire(geste(&self.worktree, &["rev-parse", "HEAD"])) {
            Ok(sha) => sha.trim().to_string(),
            Err(vu) => return (None, Err(rate(vu))),
        };
        let journal = self.dossier.join(format!("verif-{}-fusion-{k}.tmp", self.vague));
        let (verif, rejeu) = rejouer(self.profil, &self.preuve, &self.worktree, &journal);
        if let Err(erreur) = rejeu {
            let preuve = format!("la preuve de la vague, rejouée après la fusion de {id} : {erreur}");
            let correction = "La tâche est refusée sans relance, et la branche de la vague n'a pas bougé : lis la fin \
                              de la sortie de la preuve, puis relance la tâche sur la tête de la vague.";
            return (verif, Err(Refus::new(Code::PreuveRouge, preuve, correction)));
        }
        if let Err(erreur) = noter(&sha, verif.as_deref().unwrap_or_default()) {
            let preuve = format!("la fusion de {id} ({sha}) n'est pas notée : {erreur}");
            let correction = "La fusion est défaite et la vague s'arrête : rends le fil inscriptible, puis relance les \
                              tâches non assemblées.";
            return (verif, Err(Refus::new(Code::FilInjoignable, preuve, correction)));
        }
        let reference = format!("refs/heads/{}", self.branche);
        if let Err(vu) = faire(geste(&self.depot, &["update-ref", &reference, &sha, &self.tete])) {
            return (verif, Err(rate(format!("la fusion de {id} est notée, mais {vu}"))));
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
        let preuve = format!("le worktree de la vague ne revient pas à sa tête {}, {pourquoi} : {vu}", self.tete);
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
/// que la fusion prend.
fn geler(bac: &Path, titre: &str) -> Result<String, String> {
    let mut ajout = geste(bac, &["add", "-A", "--", "."]);
    ajout.args(exclusions_de_l_etat());
    // Les butées du mur que la main a laissées derrière elle : ni écrites par elle, ni à commiter.
    ajout.args(butees_du_bac(bac).into_iter().map(|chemin| format!(":(exclude,literal){chemin}")));
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
        commande.args(CLAUDE);
        let sortie = faire(commande).map_err(|vu| Refus::new(Code::GitImpossible, vu, SANS_LISTE))?;
        vus.extend(sortie.split('\0').filter(|entree| !entree.is_empty()).map(String::from));
    }
    if vus.is_empty() {
        return Ok(());
    }
    vus.sort();
    vus.dedup();
    Err(Refus::new(
        Code::ClaudeTouche,
        format!(
            "le gel de la tâche {id} porterait, sous un dossier .claude, des chemins que la fusion ferait entrer dans \
             la branche de la vague : {}",
            citer(&vus)
        ),
        CLAUDE_GELE,
    ))
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
/// Rien sous un dossier `.claude` (voir [`SOUS_CLAUDE`]). Un échec de git, qu'il ne parte pas ou qu'il refuse, est le
/// refus 7009 : une liste vide ne vaut « rien changé » que si git a réussi.
pub(crate) fn chemins_changes(bac: &Path, depart: &str) -> Result<Vec<String>, Refus> {
    let statut = geste(bac, &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--", "."]);
    let commites = geste(bac, &["diff", "--name-only", "-z", "--no-renames", depart, "HEAD", "--", "."]);
    let mut chemins = Vec::new();
    // Chaque entrée finit par NUL, sans guillemets ; celles de `status` commencent par deux lettres d'état et un blanc.
    for (mut commande, saut) in [(statut, 3), (commites, 0)] {
        commande.args(SOUS_CLAUDE);
        let sortie = faire(commande).map_err(|vu| Refus::new(Code::GitImpossible, vu, SANS_LISTE))?;
        let entrees = sortie.split('\0').filter(|entree| !entree.is_empty());
        chemins.extend(entrees.filter_map(|entree| entree.get(saut..)).map(String::from));
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
    let preuve =
        format!("la main de la tâche {id} a changé, hors de ses fichiers ({}) : {}", citer(fichiers), citer(&hors));
    Err(Refus::new(Code::HorsFichiers, preuve, correction))
}

/// Le refus 7018 pour les chemins de `changes` que `proteges` couvre ([`couvre`]), avec la `correction` de l'endroit
/// qui juge.
fn protegees_changees(changes: &[String], id: &str, proteges: &[String], correction: &str) -> Result<(), Refus> {
    let touches: Vec<String> =
        changes.iter().filter(|chemin| proteges.iter().any(|entree| couvre(entree, chemin))).cloned().collect();
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
    protegees_changees(&chemins_changes(bac, depart)?, id, proteges, PROTEGE)
}

/// La main de la tâche `id` n'a changé que des chemins que `fichiers` couvre ([`couvre`]). La comparaison est exacte,
/// octet à octet, comme git rend les chemins : la casse compte sous Windows comme ailleurs, et un écart de casse n'est
/// jamais couvert. Un chemin de trop est le refus 7013, qui cite ces chemins et ceux de `fichiers` ; git qui ne dit pas
/// ce que la main a changé, le refus 7009.
// raccourci : un descendant détaché de la main peut encore écrire après le gel, dans le bac, sans effet sur la branche
// qui ne reçoit que le gel, ou hors du bac, ce que seul le mur tient ; monter quand le nœud prouvera la mort de l'arbre
// d'une main
pub(crate) fn fichiers_tenus(bac: &Path, depart: &str, id: &str, fichiers: &[String]) -> Result<(), Refus> {
    hors_des_fichiers(chemins_changes(bac, depart)?, id, fichiers, HORS_FICHIERS)
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
            let quoi = if lu.file_type().is_symlink() { "un lien symbolique" } else { "un dossier" };
            return Err(touche(format!("{fichier}, à la racine du bac, est {quoi}, et non un fichier ordinaire")));
        }
        Err(e) => return Err(touche(format!("{fichier} n'est pas à la racine du bac : {e}"))),
    }
    let ignore = geste(bac, &["check-ignore", "-q", "--", fichier]).output().map_err(|e| {
        rate(format!("lancement impossible de git check-ignore -q -- {fichier} dans {} : {e}", bac.display()))
    })?;
    match ignore.status.code() {
        Some(1) => {}
        Some(0) => {
            let preuve =
                format!("le dépôt visé ignore {fichier} (git check-ignore), qui n'est donc pas un travail du bac");
            return Err(touche(preuve));
        }
        _ => {
            let dit = String::from_utf8_lossy(&ignore.stderr);
            let vu = format!(
                "git check-ignore -q -- {fichier} dans {} a échoué ({}) : {}",
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
            "la main a commité dans son bac : sa tête est {}, et son départ {depart}",
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
    Err(touche(format!("la main a changé dans son bac, en plus de {fichier} : {}", citer(&hors))))
}

/// Ces chemins entre guillemets, `CITES_MAX` au plus, puis le nombre des autres. Chacun s'écrit comme un texte de Rust,
/// ses caractères de contrôle échappés : il vient d'une main ou d'un plan, et peut piéger un terminal.
fn citer(chemins: &[String]) -> String {
    let cites = chemins.iter().take(CITES_MAX).map(|chemin| format!("{chemin:?}")).collect::<Vec<_>>().join(", ");
    match chemins.len().checked_sub(CITES_MAX) {
        Some(reste) if reste > 0 => format!("{cites}, et {reste} de plus"),
        _ => cites,
    }
}

#[cfg(test)]
mod tests;
