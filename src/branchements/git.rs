//! Les gestes git de Kraken : les réglages qu'il pose sur chacun, l'identité qu'il donne aux commits, les variables de
//! dépôt qu'il ôte de l'enfant, et les façons de le jouer. [`geste`] est la seule façon dont Kraken lance git dans le
//! dépôt visé ou un de ses worktrees ; [`faire`] et [`faire_borne`] le jouent, et disent ce qui a été vu quand git
//! refuse.

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

use crate::coeur::erreur::{Code, Refus, nommee};
use crate::coeur::lecons::{self, Lecon};

/// Le git-dir et le work-tree de chaque worktree que Kraken a posé, lus à sa pose, avant toute main, et rangés sous le
/// chemin où il l'a posé : [`geste`] les passe à git au lieu de suivre le fichier `.git` du worktree, que la main peut
/// réécrire vers un autre dépôt.
static LIEUX: Mutex<Vec<(PathBuf, PathBuf, PathBuf)>> = Mutex::new(Vec::new());

/// Les lieux rangés, même après une panique qui les tenait : chaque écriture y est entière.
pub(crate) fn lieux() -> MutexGuard<'static, Vec<(PathBuf, PathBuf, PathBuf)>> {
    LIEUX.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Les réglages que Kraken pose en tête de chacun de ses gestes git : la ligne de commande l'emporte sur toute la
/// configuration de git, celle que le dépôt visé porte comme celle qu'un bac lui ajouterait.
///
/// - `core.hooksPath=/dev/null` : aucun crochet ne se trouve, donc aucun ne joue. `git worktree add` joue
///   `post-checkout` et `reference-transaction`, `update-ref` joue `reference-transaction`, et un crochet se lit dans
///   le dépôt, ou dans un bac sous un `core.hooksPath` relatif : Kraken le jouerait hors du mur, avec les droits de
///   l'humain. C'est le principe de `disableAllHooks`, étendu à tous les gestes de Kraken.
/// - `core.fsmonitor=false` : le programme que nomme `core.fsmonitor` tournerait de même hors du mur, lancé par
///   `status`, `diff`, `ls-files`, `add`, `commit`, `reset` et `clean`, crochets coupés ou non. Le couper ne coûte que
///   de la vitesse.
/// - `commit.gpgsign=false` : un commit ou une fusion de Kraken n'est pas signé, et ne lance pas le programme de
///   signature du dépôt.
/// - `core.longpaths=true` : sous Windows, git refuse sans lui un chemin de plus de 260 caractères (« Filename too
///   long »), et un bac en porte vite, sous le `target` d'un projet par exemple, que `git worktree remove` efface avec
///   lui. Ailleurs, il ne change rien.
const REGLAGES: [&str; 8] = [
    "-c",
    "core.hooksPath=/dev/null",
    "-c",
    "core.fsmonitor=false",
    "-c",
    "commit.gpgsign=false",
    "-c",
    "core.longpaths=true",
];

/// L'identité des commits et des fusions de Kraken, posée sur l'enfant git de chaque geste et sur le processus de
/// chaque main (claude.rs) : elle l'emporte sur `user.name` et `user.email`, `-c` compris, et sur un environnement
/// hérité. Les commits de la main ne portent ni le nom ni l'adresse de l'humain.
pub(crate) const IDENTITE: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "lydite"),
    ("GIT_AUTHOR_EMAIL", "lydite@invalid"),
    ("GIT_COMMITTER_NAME", "lydite"),
    ("GIT_COMMITTER_EMAIL", "lydite@invalid"),
];

/// Les variables de git qui désignent, à la place de ceux du dossier d'un geste, un dépôt, un arbre de travail, un
/// index, un espace de noms ou des objets (`GIT_ALTERNATE_OBJECT_DIRECTORIES` : un commit fait sous elle n'écrit pas
/// les objets que l'autre dépôt a déjà, et le dépôt visé les perd quand l'autre disparaît), ou qui portent la
/// configuration d'un git parent (`GIT_CONFIG_PARAMETERS` et `GIT_CONFIG_COUNT`, dont `GIT_CONFIG_KEY_<n>` et
/// `GIT_CONFIG_VALUE_<n>` sont inertes sans le second) ou le dossier de ses programmes (`GIT_EXEC_PATH`) : `geste` les
/// ôte de l'enfant, et `retire_de_l_env` (processus.rs) de la main et de sa preuve. Lancé d'un crochet, d'un alias `!`
/// de git ou d'un shell qui les exporte, Kraken les hériterait, et chaque geste (`reset --hard`, `clean -fd`, `merge`,
/// `add -A`) jouerait dans le dépôt qu'elles visent, quel que soit son dossier, avec la configuration et les programmes
/// qu'elles portent : la branche, l'index ou l'arbre d'un autre dépôt perdus. Les fichiers de l'humain,
/// `GIT_CONFIG_GLOBAL` et `GIT_CONFIG_SYSTEM`, n'en sont pas : ils passent partout.
pub(crate) const VARIABLES_DE_DEPOT: [&str; 10] = [
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_NAMESPACE",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_EXEC_PATH",
];

/// Un geste git de Kraken dans `dir`, le dépôt visé ou un de ses worktrees : la seule façon dont Kraken y lance git
/// (ses racines, le bac, `sans_claude`, l'assemblage, la branche de la vague). Chaque commande porte les `REGLAGES` et
/// l'`IDENTITE`, qui ne servent qu'aux commits et aux fusions mais, posés partout, laissent une seule façon de faire et
/// nomment Kraken dans les reflogs. Aucune ne reçoit les `VARIABLES_DE_DEPOT` de l'environnement de Kraken. Rien ne
/// s'écrit dans la configuration de git. Dans un worktree que Kraken a posé ([`crate::branchements::bac::poser`]), le
/// geste porte `--git-dir` et `--work-tree` lus à la pose, et ne suit pas le `.git` du worktree, qu'une main peut
/// réécrire vers un autre dépôt ; un geste dans le dépôt visé lui-même suit son dossier.
pub(crate) fn geste(dir: &Path, args: &[&str]) -> Command {
    let mut git = Command::new("git");
    git.current_dir(dir).args(REGLAGES);
    if let Some((_, git_dir, work_tree)) = lieux().iter().find(|(chemin, ..)| chemin == dir) {
        git.arg("--git-dir").arg(git_dir).arg("--work-tree").arg(work_tree);
    }
    git.args(args).envs(IDENTITE);
    for nom in VARIABLES_DE_DEPOT {
        git.env_remove(nom);
    }
    git
}

/// Joue un geste : la sortie standard de git s'il réussit ; sinon ce qui a été vu, qui commence par la commande et son
/// dossier (la sortie de git peut être vide), puis ce que git a dit, ou pourquoi il n'a pas pu partir.
pub(crate) fn faire(mut geste: Command) -> Result<String, String> {
    let sortie = geste.output().map_err(|e| format!("lancement impossible de {} : {e}", decrire(&geste)))?;
    if !sortie.status.success() {
        return Err(echec(&geste, &sortie));
    }
    Ok(String::from_utf8_lossy(&sortie.stdout).into_owned())
}

/// Joue un geste dont la sortie standard peut être immense : en garde `garde` octets et compte le reste sans le garder.
/// La sortie d'erreur se lit à part, en même temps, pour qu'un git qui la remplit pendant qu'on lit sa sortie ne s'y
/// bloque pas. Un lancement impossible rend `lancement impossible de …` ; un git qui refuse rend le texte d'`echec` :
/// sa sortie d'erreur entière, puis sa sortie standard gardée jusqu'à `garde` octets, et le nombre d'octets non gardés
/// quand il en reste.
#[expect(clippy::expect_used, reason = "tuyaux demandés au lancement de l'enfant")]
pub(crate) fn faire_borne(mut geste: Command, garde: usize) -> Result<(Vec<u8>, u64), String> {
    let mut enfant = geste
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("lancement impossible de {} : {e}", decrire(&geste)))?;
    let mut sortie = enfant.stdout.take().expect("stdout en tuyau");
    let mut erreur = enfant.stderr.take().expect("stderr en tuyau");
    let lecteur = std::thread::spawn(move || {
        let mut texte = Vec::new();
        erreur.read_to_end(&mut texte).map(|_| texte)
    });
    let mut gardes = Vec::with_capacity(garde);
    let mut borne = (&mut sortie).take(garde as u64);
    borne.read_to_end(&mut gardes).map_err(|e| format!("lecture impossible de {} : {e}", decrire(&geste)))?;
    let reste = io::copy(&mut sortie, &mut io::sink())
        .map_err(|e| format!("lecture impossible de {} : {e}", decrire(&geste)))?;
    let statut = enfant.wait().map_err(|e| format!("attente impossible de {} : {e}", decrire(&geste)))?;
    let erreur = lecteur
        .join()
        .map_err(|_| format!("lecture impossible de l'erreur de {} : le fil a péri", decrire(&geste)))?
        .map_err(|e| format!("lecture impossible de l'erreur de {} : {e}", decrire(&geste)))?;
    if !statut.success() {
        let mut vu = echec(&geste, &Output { status: statut, stdout: gardes, stderr: erreur });
        if reste > 0 {
            vu.push_str(&format!(" ; {reste} octets de plus sur la sortie standard, non gardés"));
        }
        return Err(vu);
    }
    let total = gardes.len() as u64 + reste;
    Ok((gardes, total))
}

/// Le geste, pour une preuve : `git <arguments> dans <dossier>`, sans les `REGLAGES` que porte chacun.
pub(crate) fn decrire(geste: &Command) -> String {
    let mut args: Vec<_> = geste.get_args().skip(REGLAGES.len()).map(OsStr::to_string_lossy).collect();
    if args.first().is_some_and(|arg| arg == "--git-dir") {
        args.drain(..4);
    }
    let dir = geste.get_current_dir().map_or_else(String::new, |dir| dir.display().to_string());
    format!("git {} dans {dir}", args.join(" "))
}

/// Un geste que git a refusé : la commande et son dossier, son statut, puis ce que git a écrit.
fn echec(geste: &Command, sortie: &Output) -> String {
    let (erreur, texte) = (String::from_utf8_lossy(&sortie.stderr), String::from_utf8_lossy(&sortie.stdout));
    format!("{} a échoué ({}) : {}", decrire(geste), sortie.status, format!("{erreur}{texte}").trim())
}

/// Les racines du dépôt visé : son dossier de tête et son git commun, en chemins absolus, que dit
/// `git rev-parse --path-format=absolute --show-toplevel --git-common-dir`, lancé par [`geste`] : sous un `GIT_DIR`
/// hérité, git rendrait le git commun d'un autre dépôt, que `borner` donnerait en lecture à la main. Un dépôt qui n'est
/// pas un dossier qui existe est le refus 6002, avant tout geste git ; git qui ne part pas, le refus 6006, qui le
/// nomme ; git qui ne connaît pas le dépôt, le refus 6003, qui porte sa sortie. Rien n'est créé.
pub(crate) fn racines(depot: &Path) -> Result<(PathBuf, PathBuf), Refus> {
    if !depot.is_dir() {
        return Err(Refus::new(
            Code::DepotIntrouvable,
            format!("{} n'est pas un dossier qui existe", depot.display()),
            "Run Lydite from the target repository, or give it with --depot <path>.",
        ));
    }
    let git = geste(depot, &["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"])
        .output()
        .map_err(|e| nommee(e, format!("lancement impossible de git dans {}", depot.display())))?;
    let sortie = String::from_utf8_lossy(&git.stdout);
    match sortie.lines().collect::<Vec<_>>()[..] {
        [tete, commun] if git.status.success() => Ok((tete.into(), commun.into())),
        _ => Err(Refus::new(
            Code::BacImpossible,
            format!(
                "git rev-parse dans {} a échoué ({}) : {}",
                depot.display(),
                git.status,
                String::from_utf8_lossy(&git.stderr).trim()
            ),
            "Target a git repository, from its folder or with --depot <path>, then run again.",
        )),
    }
}

/// Le dossier de Kraken est hors du dépôt visé, dont `racine` est le dossier de tête : les deux chemins canonisés, le
/// dossier, qui peut ne pas exister encore, par son plus proche ancêtre qui existe. Un dossier sous le dépôt, ou qui
/// porte `..`, est le refus 6015 ; une canonisation ratée, le refus 6006. Rien n'est créé.
pub(crate) fn hors_du_depot(racine: &Path, dossier: &Path) -> Result<(), Refus> {
    let refus = |preuve: String| {
        Refus::new(
            Code::DossierSousLeDepot,
            preuve,
            "Pose LYDITE_DOSSIER à un chemin absolu hors du dépôt visé, puis relance.",
        )
    };
    if dossier.components().any(|composant| composant == Component::ParentDir) {
        return Err(refus(format!("le dossier de Lydite {} porte « .. »", dossier.display())));
    }
    let depot =
        fs::canonicalize(racine).map_err(|e| nommee(e, format!("chemin canonique du dépôt {}", racine.display())))?;
    if canonique(dossier)?.starts_with(&depot) {
        return Err(refus(format!(
            "le dossier de Lydite {} est sous le dépôt visé {}",
            dossier.display(),
            racine.display()
        )));
    }
    Ok(())
}

/// Les leçons du commit `base` du dépôt `depot`, lues par git (`.agents/skills/<nom>/SKILL.md`), jamais dans un bac,
/// dans l'ordre des noms. Une leçon fausse est le refus 6031, qui nomme son chemin ; un git qui ne les dit pas, le
/// refus 6006.
pub(crate) fn lecons_du_commit(depot: &Path, base: &str) -> Result<Vec<Lecon>, Refus> {
    let illisible = |vu: String| {
        Refus::new(
            Code::EntreeSortie,
            vu,
            "Vérifie que git se lance dans le dépôt visé et y lit la base, puis relance.",
        )
    };
    let sortie = faire(geste(depot, &["ls-tree", "-r", "-z", "--name-only", base, "--", ".agents/skills"]))
        .map_err(illisible)?;
    let mut fichiers: Vec<(&str, &str)> = sortie
        .split('\0')
        .filter_map(|chemin| {
            let nom = chemin.strip_prefix(".agents/skills/")?.strip_suffix("/SKILL.md")?;
            (!nom.is_empty() && !nom.contains('/')).then_some((nom, chemin))
        })
        .collect();
    fichiers.sort();
    let mut lecons = Vec::new();
    for (nom, chemin) in fichiers {
        let objet = format!("{base}:{chemin}");
        let (octets, _) =
            faire_borne(geste(depot, &["cat-file", "blob", &objet]), lecons::TAILLE_MAX + 1).map_err(illisible)?;
        let lecon = lecons::lire(nom, &octets)
            .map_err(|refus| Refus::new(refus.code, format!("{chemin} : {}", refus.preuve), refus.correction))?;
        lecons.push(lecon);
    }
    Ok(lecons)
}

/// Le chemin canonique d'un chemin qui n'existe peut-être pas : son plus proche ancêtre qui existe, canonisé, puis le
/// reste tel quel. Le chemin ne porte pas `..`.
fn canonique(chemin: &Path) -> io::Result<PathBuf> {
    let mut reste = Vec::new();
    for ancetre in chemin.ancestors() {
        match fs::canonicalize(ancetre) {
            Ok(canon) => return Ok(reste.iter().rev().fold(canon, |canon, nom| canon.join(nom))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => reste.extend(ancetre.file_name()),
            Err(e) => return Err(nommee(e, format!("chemin canonique de {}", ancetre.display()))),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        format!("chemin canonique de {} : aucun ancêtre n'existe", chemin.display()),
    ))
}

#[cfg(test)]
mod tests;
