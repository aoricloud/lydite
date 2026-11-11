use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::branchements::git::{faire, geste, lieux};
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{Code, Refus, nommee};

/// Un bac neuf : un worktree du dépôt sous `<dossier>/bacs/<id>`, sur la branche `lydite/<id>`, posée sur `depart` (un
/// sha, ou `HEAD` du dépôt). Le dossier est un chemin absolu (voir `dossier::dossier`) : git le lit depuis le dépôt.
/// Tout échec, que git ne parte pas ou qu'il refuse le bac, est le refus 7001, qui nomme la commande et le dépôt, puis
/// ce que git a dit : la tâche n'a pas de bac, et les autres continuent.
pub(crate) fn bac(depot: &Path, dossier: &Path, id: &str, depart: &str) -> Result<PathBuf, Refus> {
    let bac = dossier.join("bacs").join(id);
    poser(depot, &bac, &["-b", &format!("lydite/{id}")], depart).map_err(|vu| {
        let correction = format!(
            "The task is refused, and the others continue: check that git starts in the target repository, and that \
             neither the branch lydite/{id} nor the folder {} exists, then run it again in a new wave.",
            bac.display()
        );
        Refus::new(Code::VagueBacImpossible, vu, correction)
    })?;
    Ok(bac)
}

/// Pose `worktree`, un worktree du dépôt visé sur `depart`, avec ces `options` de `git worktree add -q`, puis lit son
/// git-dir et son work-tree, que tout geste de Kraken dans `worktree` portera ([`crate::branchements::git::lieux`]).
/// Rend ce que git a refusé.
pub(crate) fn poser(depot: &Path, worktree: &Path, options: &[&str], depart: &str) -> Result<(), String> {
    let mut pose = geste(depot, &["worktree", "add", "-q"]);
    pose.args(options).arg(worktree).arg(depart);
    faire(pose)?;
    // Le lieu d'un worktree retiré au même chemin ne vaut plus : la lecture suit le `.git` tout juste posé.
    lieux().retain(|(chemin, ..)| chemin != worktree);
    let lu = faire(geste(worktree, &["rev-parse", "--path-format=absolute", "--git-dir", "--show-toplevel"]))?;
    let mut lignes = lu.lines();
    let (Some(git_dir), Some(work_tree)) = (lignes.next(), lignes.next()) else {
        return Err(format!("git rev-parse gives no git-dir and work-tree for {}: {lu}", worktree.display()));
    };
    lieux().push((worktree.to_path_buf(), git_dir.into(), work_tree.into()));
    Ok(())
}

/// Retire un worktree de Kraken du dépôt visé, le bac d'une tâche ou celui d'une vague, chaque pas une fois le
/// précédent réussi : le worktree, par `git worktree remove` sans forcer, car git refuse un worktree qui garde un
/// changement ou un fichier non suivi, un travail qui ne serait nulle part ailleurs ; puis son dossier temporaire
/// ([`temporaire_de`]), s'il existe ; puis la branche `(nom, valeur)`, si elle est donnée, par `update-ref -d` sur sa
/// valeur : une branche qui a bougé reste. Rend le premier pas raté, qui se nomme. Un retrait efface le dossier du bac
/// tout entier, fichiers ignorés compris : git ne les compte pas pour du travail à perdre, et le `target` d'un projet
/// ou un fichier de réglages local que le dépôt ignore partent avec le bac, sans demande. Les deux gestes de git se
/// jouent par `faire`, que l'appelant donne : la forge y met ses variables et son délai, comme à chacun de ses gestes,
/// sans quoi un git qui attend une réponse ou s'éternise la retiendrait sans fin.
pub(crate) fn retirer(
    depot: &Path,
    worktree: &Path,
    branche: Option<(&str, &str)>,
    faire: impl Fn(Command) -> Result<String, String>,
) -> Result<(), String> {
    let mut retrait = geste(depot, &["worktree", "remove"]);
    retrait.arg(worktree);
    faire(retrait)?;
    if let Some(temporaire) = temporaire_de(worktree).filter(|temporaire| temporaire.exists()) {
        fs::remove_dir_all(&temporaire).map_err(|e| format!("erasing {}: {e}", temporaire.display()))?;
    }
    if let Some((nom, valeur)) = branche {
        faire(geste(depot, &["update-ref", "-d", &format!("refs/heads/{nom}"), valeur]))?;
    }
    Ok(())
}

/// Tout dossier `.claude` d'un bac, à toute profondeur et en toute casse : Claude Code charge les compétences d'un
/// `<dossier>/.claude/skills` imbriqué, et sous Windows `.Claude/` est le dossier qu'il lit. Deux pathspecs, car sous
/// `glob` un motif ne couvre pas ce qui est sous lui, ni son contraire : ce qui est sous le dossier, et l'entrée
/// `.claude` elle-même, fichier ou lien symbolique. Git ne liste un lien que comme cette entrée, et Claude Code lit à
/// travers lui les réglages du dossier qu'il vise.
pub(crate) const CLAUDE: [&str; 2] = [":(icase,glob)**/.claude/**", ":(icase,glob)**/.claude"];

/// Les noms que Claude Code écrit lui-même sous `.claude/` et tient hors de git : sa liste `# claude-code-runtime`, la
/// même dans 2.1.312 et 2.1.316. `worktrees` reçoit ses sous-agents isolés, qu'une main qui délègue y écrit sans
/// l'avoir voulu. Aucun n'est un réglage, un crochet, une commande, une compétence ni un agent, et la mémoire de projet
/// (`.claude/agent-memory/`) n'en est pas. L'exemption ne vaut que hors de git : suivi ou commité, cet état est refusé
/// comme le reste.
// raccourci : la liste de Claude Code 2.1.312 à 2.1.316 ; monter quand une version en ajoute, le refus 7006 nommant le
// fichier
pub(crate) const ETAT_DE_CLAUDE: [&str; 10] = [
    "scheduled_tasks.lock",
    "scheduled_tasks.json",
    "routines/.state",
    "worktrees",
    "checkpoints",
    "mailbox",
    "agent-registry.json",
    "agent-memory-local",
    "first-run",
    "assistant-daemon-state.json",
];

/// Les pathspecs qui ôtent l'état de Claude Code d'un geste, à toute profondeur et en toute casse : chaque nom, et ce
/// qui est sous lui, car sous `glob` un motif ne couvre pas ce qui est sous lui.
pub(crate) fn exclusions_de_l_etat() -> Vec<String> {
    let exclure = |nom: &&str| {
        [format!(":(exclude,icase,glob)**/.claude/{nom}"), format!(":(exclude,icase,glob)**/.claude/{nom}/**")]
    };
    ETAT_DE_CLAUDE.iter().flat_map(exclure).collect()
}

/// Vérifie que la main n'a touché aucun dossier `.claude` de son bac depuis `depart`, à toute profondeur et en toute
/// casse, entrée comprise (un lien nommé `.claude`). Quatre gestes de git :
///
/// - `git diff` : le suivi, modifié, effacé ou commité et encore là ;
/// - `git log` : tout commit de la main depuis `depart` qui y touche, même si un autre commit ou un effacement l'a ôté
///   ensuite : l'arbre n'en dit plus rien, mais la fusion ferait entrer ce commit dans l'histoire de la branche de la
///   vague ;
/// - `git ls-files --others` : le non-suivi, ignoré compris, hors de l'état que Claude Code y tient lui-même et des
///   butées du mur (fichiers ordinaires vides en lecture seule, entrées directes de `.claude/`). Cet état, suivi ou
///   commité, est refusé comme le reste ;
/// - `git diff --cached` : l'index, que les trois autres taisent quand la main stage un `.claude` puis l'efface du
///   worktree. Le gel le commiterait, car son exclusion empêche l'ajout mais ne retire pas une entrée déjà stagée.
///
/// Un nom vu est le refus 7006, qui les nomme, une fois chacun ; un échec de git, qu'il ne parte pas ou qu'il refuse,
/// est le refus 7009, jamais un bac propre : une sortie vide ne vaut « rien sous `.claude` » que si git a réussi.
pub(crate) fn sans_claude(bac: &Path, depart: &str) -> Result<(), Refus> {
    let suivi = geste(bac, &["diff", "--name-only", depart, "--"]);
    // `-m` : une fusion dit ses fichiers, que `--name-only` taît sans lui. `--full-history` : git n'élague aucun côté
    // d'une fusion (essai joué, git 2.54 : `-m` l'implique déjà ; on le dit pour n'en pas dépendre).
    // `--no-show-signature` : le `log.showSignature` du dépôt visé lancerait son `gpg.program` hors du mur.
    let plage = format!("{depart}..HEAD");
    let commite =
        geste(bac, &["log", "--full-history", "-m", "--format=", "--name-only", "--no-show-signature", &plage, "--"]);
    let mut non_suivi = geste(bac, &["ls-files", "--others", "--"]);
    non_suivi.args(exclusions_de_l_etat());
    let index = geste(bac, &["diff", "--cached", "--name-only", depart, "--"]);
    let mut vus = Vec::new();
    for (rang, mut commande) in [suivi, commite, non_suivi, index].into_iter().enumerate() {
        commande.args(CLAUDE);
        let sortie = faire(commande).map_err(|vu| {
            let correction = "Check that git starts and that the sandbox is a worktree of the target repository: the \
                              task is refused without being retried or frozen.";
            Refus::new(Code::GitImpossible, vu, correction)
        })?;
        // Le rang 2 est le non-suivi : une butée du mur n'y compte pas.
        let nom_vu = |nom: &&str| !nom.is_empty() && !(rang == 2 && est_une_butee(bac, nom));
        vus.extend(sortie.lines().filter(nom_vu).map(str::to_string));
    }
    if vus.is_empty() {
        return Ok(());
    }
    vus.sort();
    vus.dedup();
    Err(Refus::new(
        Code::ClaudeTouche,
        format!("the hand touched a .claude folder in its sandbox {}: {}", bac.display(), vus.join(", ")),
        "Lydite neither retries nor freezes this sandbox: read what the hand wrote there, then run the task again \
         with an objective that touches no .claude folder. A setting, a hook, a command, a skill, an agent or a \
         project memory is set by the human, outside the wave.",
    ))
}

/// Une butée du mur : une entrée directe de `.claude/` qui est un fichier ordinaire (ni lien ni dossier), vide, et sans
/// droit d'écriture. Le bac à sable de Claude Code en pose aux chemins qu'il interdit d'écrire ; elle ne porte rien.
fn est_une_butee(bac: &Path, nom: &str) -> bool {
    let Some(reste) = nom.strip_prefix(".claude/") else { return false };
    if reste.is_empty() || reste.contains('/') {
        return false;
    }
    est_un_fichier_vide_en_lecture_seule(bac, nom)
}

/// Le chemin `nom` du bac est un fichier ordinaire (ni lien ni dossier), vide, et sans droit d'écriture.
fn est_un_fichier_vide_en_lecture_seule(bac: &Path, nom: &str) -> bool {
    fs::symlink_metadata(bac.join(nom))
        .is_ok_and(|meta| meta.file_type().is_file() && meta.len() == 0 && meta.permissions().readonly())
}

/// Le chemin `chemin` n'est pas dans le commit `depart` du bac, ni fichier ni dossier : `git rev-parse --verify
/// --quiet <depart>:<chemin>` sort alors avec 1, sans rien dire. Il sort de même avec 1 quand `depart` n'est pas un
/// commit : `depart` se vérifie d'abord. Tout autre résultat, git qui ne part pas ou qui échoue, n'est pas une absence
/// prouvée : faux.
fn absent_du_depart(bac: &Path, depart: &str, chemin: &str) -> bool {
    let sort = |args: &[&str]| geste(bac, args).output().ok().and_then(|sortie| sortie.status.code());
    let commit = format!("{depart}^{{commit}}");
    let objet = format!("{depart}:{chemin}");
    sort(&["rev-parse", "--verify", "--quiet", &commit]) == Some(0)
        && sort(&["rev-parse", "--verify", "--quiet", &objet]) == Some(1)
}

/// Les butées du mur à un chemin protégé absent : le bac à sable de Claude Code pose, pour refuser l'écriture d'un
/// chemin protégé qui n'existe pas, un fichier ordinaire vide et sans droit d'écriture à ce chemin, dans un dossier
/// qu'il crée lui aussi. Un chemin de `proteges` (tel qu'écrit, sans sa barre finale pour un dossier) en est une si
/// c'est un fichier ordinaire vide en lecture seule, et s'il n'est pas dans le commit `depart` du bac. Un fichier ne
/// porte rien dessous : la main n'y a rien écrit d'autre. En chemins relatifs au bac, triés ; vide si `proteges` l'est.
pub(crate) fn butees_protegees(bac: &Path, depart: &str, proteges: &[String]) -> Vec<String> {
    let mut chemins: Vec<String> = proteges
        .iter()
        .map(|entree| entree.strip_suffix('/').unwrap_or(entree))
        .filter(|chemin| !chemin.is_empty() && est_un_fichier_vide_en_lecture_seule(bac, chemin))
        .filter(|chemin| absent_du_depart(bac, depart, chemin))
        .map(str::to_string)
        .collect();
    chemins.sort();
    chemins.dedup();
    chemins
}

/// Les butées du mur du bac : les entrées directes de `.claude/` à sa racine qui sont des butées ([`est_une_butee`]),
/// puis celles à un chemin protégé absent ([`butees_protegees`]), en chemins relatifs au bac. Vide si `.claude/` n'y
/// est pas, ou ne se lit pas, et si aucune butée protégée n'y est.
pub(crate) fn butees_du_bac(bac: &Path, depart: &str, proteges: &[String]) -> Vec<String> {
    let mut noms: Vec<String> = fs::read_dir(bac.join(".claude"))
        .into_iter()
        .flatten()
        .filter_map(|entree| entree.ok()?.file_name().into_string().ok())
        .map(|nom| format!(".claude/{nom}"))
        .filter(|chemin| est_une_butee(bac, chemin))
        .collect();
    noms.extend(butees_protegees(bac, depart, proteges));
    noms.sort();
    noms.dedup();
    noms
}

/// Le dossier temporaire d'un bac ou d'un worktree : son voisin dans son dossier parent (`bacs/`), nommé des 12
/// premiers chiffres hexadécimaux du SHA-256 de son nom, suivis de `.tmp`. Le nom est court et stable, fait
/// constaté : Claude Code met ses sockets de pont dans le `TMPDIR` de son processus, le dossier temporaire même
/// (`<dossier temporaire>/claude-http-<16 hex>.sock`), et donne à ses commandes `<dossier temporaire>/claude-<uid>` en
/// `TMPDIR`. Un chemin de socket tient en 107 octets. `None` pour un chemin sans nom, une racine.
// raccourci : au-delà d'un dossier de Kraken d'environ 51 octets, sous le mur Linux le bac à sable de la main ne
// démarre pas, la main échoue sans rien jouer ; monter en refus clair quand les tests ne poseront plus leurs dossiers
// de Kraken sous TMPDIR
pub(crate) fn temporaire_de(bac: &Path) -> Option<PathBuf> {
    let nom = bac.file_name()?.to_string_lossy();
    Some(bac.with_file_name(format!("{}.tmp", &hex(&sha256(nom.as_bytes()))[..12])))
}

/// Les réglages du dépôt visé présents dans le bac, que la main ne charge plus et que le fil note encore, empreints :
/// `.claude/settings.json` puis `.claude/settings.local.json` du bac, un élément `<chemin> <empreinte>` par ligne pour
/// chacun qui est là au lancement ; la chaîne vide s'il n'y en a aucun. Un fichier présent qui ne se lit pas est une
/// erreur qui nomme son chemin et garde son `kind()`.
pub(crate) fn reglages_du_bac(bac: &Path) -> io::Result<String> {
    let mut lignes = Vec::new();
    for nom in ["settings.json", "settings.local.json"] {
        let chemin = bac.join(".claude").join(nom);
        match fs::read(&chemin) {
            Ok(octets) => lignes.push(format!(".claude/{nom} {}", hex(&sha256(&octets)))),
            Err(e) if matches!(e.kind(), io::ErrorKind::NotFound | io::ErrorKind::NotADirectory) => {}
            Err(e) => return Err(nommee(e, format!("reading the settings {}", chemin.display()))),
        }
    }
    Ok(lignes.join("\n"))
}

#[cfg(test)]
mod tests;
