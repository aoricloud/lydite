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

use crate::branchements::github::GIT_FORGE;
use crate::coeur::empreinte::{hex, sha256};
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

/// Les variables de git qui changent la lecture des pathspecs : `geste` les ôte de l'enfant. Les refus 7006 (un
/// `.claude/` ajouté par une main) et 7021 (un fichier qui s'exécute à l'ouverture de la branche) lisent les chemins
/// par des pathspecs à magie (`:(icase,glob)…`, `:(exclude,icase,glob)…`, `:(icase,literal)…`) : avec
/// `GIT_LITERAL_PATHSPECS=1` dans l'environnement de Kraken, git les lirait mot pour mot, ne trouverait rien, et ces
/// deux refus se tairaient. Elles ne nomment pas de dépôt : elles ne vont pas dans `VARIABLES_DE_DEPOT`, que
/// `retire_de_l_env` (processus.rs) retire aussi pour la main.
#[rustfmt::skip]
pub(crate) const VARIABLES_DE_PATHSPEC: [&str; 4] = [
    "GIT_LITERAL_PATHSPECS",
    "GIT_GLOB_PATHSPECS",
    "GIT_NOGLOB_PATHSPECS",
    "GIT_ICASE_PATHSPECS",
];

/// Un geste git de Kraken dans `dir`, le dépôt visé ou un de ses worktrees : la seule façon dont Kraken y lance git
/// (ses racines, le bac, `sans_claude`, l'assemblage, la branche de la vague). Chaque commande porte les `REGLAGES` et
/// l'`IDENTITE`, qui ne servent qu'aux commits et aux fusions mais, posés partout, laissent une seule façon de faire et
/// nomment Kraken dans les reflogs. Aucune ne reçoit les `VARIABLES_DE_DEPOT` ni les `VARIABLES_DE_PATHSPEC` de
/// l'environnement de Kraken (un appelant qui veut une lecture des chemins mot pour mot la pose après coup). Rien ne
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
    for nom in VARIABLES_DE_PATHSPEC {
        git.env_remove(nom);
    }
    git
}

/// Joue un geste : la sortie standard de git s'il réussit ; sinon ce qui a été vu, qui commence par la commande et son
/// dossier (la sortie de git peut être vide), puis ce que git a dit, ou pourquoi il n'a pas pu partir.
pub(crate) fn faire(mut geste: Command) -> Result<String, String> {
    let sortie = geste.output().map_err(|e| format!("cannot start {}: {e}", decrire(&geste)))?;
    if !sortie.status.success() {
        return Err(echec(&geste, &sortie));
    }
    Ok(String::from_utf8_lossy(&sortie.stdout).into_owned())
}

/// Joue un geste dont la sortie standard peut être immense : en garde `garde` octets et compte le reste sans le garder.
/// La sortie d'erreur se lit à part, en même temps, pour qu'un git qui la remplit pendant qu'on lit sa sortie ne s'y
/// bloque pas. Un lancement impossible rend `cannot start …` ; un git qui refuse rend le texte d'`echec` :
/// sa sortie d'erreur entière, puis sa sortie standard gardée jusqu'à `garde` octets, et le nombre d'octets non gardés
/// quand il en reste.
#[expect(clippy::expect_used, reason = "tuyaux demandés au lancement de l'enfant")]
pub(crate) fn faire_borne(mut geste: Command, garde: usize) -> Result<(Vec<u8>, u64), String> {
    let mut enfant = geste
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", decrire(&geste)))?;
    let mut sortie = enfant.stdout.take().expect("stdout en tuyau");
    let mut erreur = enfant.stderr.take().expect("stderr en tuyau");
    let lecteur = std::thread::spawn(move || {
        let mut texte = Vec::new();
        erreur.read_to_end(&mut texte).map(|_| texte)
    });
    let mut gardes = Vec::with_capacity(garde);
    let mut borne = (&mut sortie).take(garde as u64);
    borne.read_to_end(&mut gardes).map_err(|e| format!("cannot read {}: {e}", decrire(&geste)))?;
    let reste = io::copy(&mut sortie, &mut io::sink()).map_err(|e| format!("cannot read {}: {e}", decrire(&geste)))?;
    let statut = enfant.wait().map_err(|e| format!("cannot wait for {}: {e}", decrire(&geste)))?;
    let erreur = lecteur
        .join()
        .map_err(|_| format!("cannot read the error output of {}: the thread died", decrire(&geste)))?
        .map_err(|e| format!("cannot read the error output of {}: {e}", decrire(&geste)))?;
    if !statut.success() {
        let mut vu = echec(&geste, &Output { status: statut, stdout: gardes, stderr: erreur });
        if reste > 0 {
            vu.push_str(&format!("; {reste} more bytes on the standard output, not kept"));
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
    format!("git {} in {dir}", args.join(" "))
}

/// Un geste que git a refusé : la commande et son dossier, son statut, puis ce que git a écrit.
fn echec(geste: &Command, sortie: &Output) -> String {
    let (erreur, texte) = (String::from_utf8_lossy(&sortie.stderr), String::from_utf8_lossy(&sortie.stdout));
    format!("{} failed ({}): {}", decrire(geste), sortie.status, format!("{erreur}{texte}").trim())
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
            format!("{} is not a folder that exists", depot.display()),
            "Run Lydite from the target repository, or give it with --repo <path>.",
        ));
    }
    let git = geste(depot, &["rev-parse", "--path-format=absolute", "--show-toplevel", "--git-common-dir"])
        .output()
        .map_err(|e| nommee(e, format!("cannot start git in {}", depot.display())))?;
    let sortie = String::from_utf8_lossy(&git.stdout);
    match sortie.lines().collect::<Vec<_>>()[..] {
        [tete, commun] if git.status.success() => Ok((tete.into(), commun.into())),
        _ => Err(Refus::new(
            Code::BacImpossible,
            format!(
                "git rev-parse in {} failed ({}): {}",
                depot.display(),
                git.status,
                String::from_utf8_lossy(&git.stderr).trim()
            ),
            "Target a git repository, from its folder or with --repo <path>, then run again.",
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
            "Set LYDITE_DOSSIER to an absolute path outside the target repository, then run again.",
        )
    };
    if dossier.components().any(|composant| composant == Component::ParentDir) {
        return Err(refus(format!("the state folder {} contains \"..\"", dossier.display())));
    }
    let depot = fs::canonicalize(racine)
        .map_err(|e| nommee(e, format!("canonical path of the repository {}", racine.display())))?;
    if canonique(dossier)?.starts_with(&depot) {
        return Err(refus(format!(
            "the state folder {} is under the target repository {}",
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
            "Check that git starts in the target repository and reads the base there, then run again.",
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
            .map_err(|refus| Refus::new(refus.code, format!("{chemin}: {}", refus.preuve), refus.correction))?;
        lecons.push(lecon);
    }
    Ok(lecons)
}

/// La borne de la lecture d'un fichier épinglé (`sha256` du plan) : 64 Mio. Au-delà, le fichier est trop lourd, et
/// n'est pas lu en entier.
pub(crate) const BORNE_EMPREINTE: u64 = 64 * 1024 * 1024;

/// Où git garde le fichier qu'on lui demande : l'index du bac, ou un commit.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Arbre<'a> {
    Index,
    Commit(&'a str),
}

/// Ce que git garde d'un fichier ([`empreinte_gardee`]).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Gardee {
    /// Un fichier ordinaire : le SHA-256 (64 hexadécimaux minuscules) des octets que git en garde.
    Empreinte(String),
    /// Git n'a aucune entrée à ce chemin.
    Absent,
    /// Une entrée qui n'est pas un fichier ordinaire (un lien symbolique, un sous-dépôt, un dossier) : son mode.
    PasOrdinaire(String),
    /// Un fichier de plus que la borne : il n'est pas lu en entier.
    TropLourd,
}

/// Ce que git garde du fichier `chemin`, dans l'index du bac ou dans un commit : la seule lecture d'un fichier épinglé,
/// la même à la fin d'une tentative et au gel. Elle n'ouvre jamais un fichier du bac, qu'une main a écrit : elle
/// demande à git l'entrée du chemin (`ls-files -s` pour l'index, `ls-tree` pour un commit), sans entrée le fichier est
/// absent, et un mode autre que 100644 ou 100755 n'est pas un fichier ordinaire ; puis elle lit l'objet par
/// `cat-file blob`, borné à `borne` octets (au-delà, le fichier est trop lourd), et en prend le SHA-256.
///
/// Le chemin se donne à git à la lettre (`GIT_LITERAL_PATHSPECS`, que `geste` ôte), et les objets se lisent sans les
/// remplacements que le dépôt déclare (`GIT_NO_REPLACE_OBJECTS`, comme les gestes de la forge). Un git qui échoue est
/// une erreur, jamais un fichier absent.
pub(crate) fn empreinte_gardee(bac: &Path, arbre: Arbre<'_>, chemin: &str, borne: u64) -> Result<Gardee, String> {
    let mut entrees = match arbre {
        Arbre::Index => geste(bac, &["ls-files", "-s", "-z", "--", chemin]),
        Arbre::Commit(commit) => geste(bac, &["ls-tree", "-z", commit, "--", chemin]),
    };
    entrees.envs(GIT_FORGE).env("GIT_LITERAL_PATHSPECS", "1");
    let sortie = faire(entrees)?;
    let mut trouvees = entrees_du_chemin(&sortie, chemin, matches!(arbre, Arbre::Index));
    let Some((mode, objet)) = trouvees.next() else { return Ok(Gardee::Absent) };
    if trouvees.next().is_some() || (mode != "100644" && mode != "100755") {
        return Ok(Gardee::PasOrdinaire(mode));
    }
    let mut lire = geste(bac, &["cat-file", "blob", &objet]);
    lire.envs(GIT_FORGE);
    let garde = usize::try_from(borne.saturating_add(1)).unwrap_or(usize::MAX);
    let (octets, total) = faire_borne(lire, garde)?;
    if total > borne {
        return Ok(Gardee::TropLourd);
    }
    Ok(Gardee::Empreinte(hex(&sha256(&octets))))
}

/// Les entrées que `ls-files -s -z` (`index` vrai : `<mode> <objet> <étape>\t<chemin>`) ou `ls-tree -z`
/// (`<mode> <type> <objet>\t<chemin>`) a rendues pour `chemin`, chacune `(mode, objet)`. Git rend aussi ce qui est sous
/// un dossier que `chemin` nomme, et rien d'autre que ce chemin ne compte.
fn entrees_du_chemin<'a>(sortie: &'a str, chemin: &'a str, index: bool) -> impl Iterator<Item = (String, String)> + 'a {
    sortie.split('\0').filter_map(move |entree| {
        let (champs, nom) = entree.split_once('\t')?;
        let [mode, deuxieme, troisieme] = champs.split(' ').collect::<Vec<_>>()[..] else { return None };
        let objet = if index { deuxieme } else { troisieme };
        (nom == chemin).then(|| (mode.to_string(), objet.to_string()))
    })
}

/// L'entrée que l'index du bac a au fichier `chemin` : `Ok(Some((mode, objet)))`, `Ok(None)` sans entrée, une erreur si
/// git échoue ou si le chemin a plus d'une entrée (un conflit en porte plusieurs, une par étape). Elle lit comme
/// [`empreinte_gardee`] : `ls-files -s -z` par [`geste`], sans les remplacements d'objets que le dépôt déclare
/// (`GIT_FORGE`), le chemin donné à git à la lettre (`GIT_LITERAL_PATHSPECS`), jamais un fichier du bac ouvert par
/// Lydite. Elle ne juge pas le mode, que l'appelant juge.
pub(crate) fn entree_de_l_index(bac: &Path, chemin: &str) -> Result<Option<(String, String)>, String> {
    let mut entrees = geste(bac, &["ls-files", "-s", "-z", "--", chemin]);
    entrees.envs(GIT_FORGE).env("GIT_LITERAL_PATHSPECS", "1");
    let sortie = faire(entrees)?;
    let mut trouvees = entrees_du_chemin(&sortie, chemin, true);
    let premiere = trouvees.next();
    if trouvees.next().is_some() {
        return Err(format!("the index of {} has more than one entry for {chemin:?}", bac.display()));
    }
    Ok(premiere)
}

/// Le chemin canonique d'un chemin qui n'existe peut-être pas : son plus proche ancêtre qui existe, canonisé, puis le
/// reste tel quel. Le chemin ne porte pas `..`.
fn canonique(chemin: &Path) -> io::Result<PathBuf> {
    let mut reste = Vec::new();
    for ancetre in chemin.ancestors() {
        match fs::canonicalize(ancetre) {
            Ok(canon) => return Ok(reste.iter().rev().fold(canon, |canon, nom| canon.join(nom))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => reste.extend(ancetre.file_name()),
            Err(e) => return Err(nommee(e, format!("canonical path of {}", ancetre.display()))),
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, format!("canonical path of {}: no ancestor exists", chemin.display())))
}

#[cfg(test)]
mod tests;
