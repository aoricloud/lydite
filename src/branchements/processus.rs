//! Les outils de processus de la main, de sa preuve et de la forge : l'échéance, l'attente, la mise à mort d'un
//! arbre, la coquille, et l'environnement ôté à une main et à sa preuve.

use std::env;
use std::fs;
use std::io;
#[cfg(target_os = "linux")]
use std::path;
use std::path::Path;
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::git::VARIABLES_DE_DEPOT;

/// Le temps qu'a une main pour sortir après son résultat, et pour finir ses voies (sortie et erreurs) une fois sortie.
pub(crate) const SORTIE: Duration = Duration::from_secs(10);

/// Le temps qu'un arbre tué a pour finir de sortir, après sa mise à mort ([`tuer_et_attendre`]).
pub(crate) const MORT: Duration = Duration::from_secs(10);

/// L'instant où tombe ce délai, compté d'ici. Un délai démesuré se borne à cent ans : l'addition d'un `Instant` ne
/// déborde pas.
pub(crate) fn echeance(delai: Duration) -> Instant {
    Instant::now() + delai.min(Duration::from_secs(100 * 365 * 24 * 3600))
}

/// Le statut de l'enfant s'il sort avant l'échéance, `None` sinon.
pub(crate) fn attendre(enfant: &mut Child, echeance: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(statut) = enfant.try_wait()? {
            return Ok(Some(statut));
        }
        if Instant::now() >= echeance {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Tue le processus `pid` et tous ses descendants, avant tout `wait` : son pid ne peut pas encore resservir. Sous
/// Windows, `taskkill /T /F`, dont le code 0 se vérifie ; ailleurs, les descendants se lisent dans `/proc/*/stat`, puis
/// un seul `kill -KILL` les tue avec lui. Pas de groupe de processus : Ctrl+C doit atteindre les mains.
// raccourci : un descendant détaché (setsid, nohup, ou dont le parent est déjà mort) survit, et sous Windows un enfant
// né entre le relevé de `taskkill /T` et la mort de sa racine aussi (sous forte charge, un `ping` tenait encore le bac
// 2 s après la mise à mort : les tests qui tuent un arbre effacent leur dossier par `essais::effacer`) ;
// monter vers un Job Object ou un cgroup avec le nœud ; et sous Windows, `taskkill /T` relève l'arbre puis arrête
// chaque processus : un descendant qui sort entre les deux fait rendre un code non nul alors que tout l'arbre est mort.
// `tuer_et_attendre` ne tient donc ce code pour un échec que si la racine vit encore : un descendant que `taskkill`
// n'arrête pas pour une autre raison que sa sortie, la racine étant morte, n'est plus dit.
pub(crate) fn tuer_arbre(pid: u32) -> Result<(), EchecDeMiseAMort> {
    let (mut tueur, nom) = if cfg!(windows) {
        let mut taskkill = Command::new("taskkill");
        taskkill.args(["/T", "/F", "/PID", &pid.to_string()]);
        (taskkill, format!("taskkill /T /F /PID {pid}"))
    } else {
        let proc = fs::read_dir("/proc").map_err(|e| EchecDeMiseAMort::NonParti(format!("/proc illisible : {e}")))?;
        let liens: Vec<(u32, u32)> = proc
            .filter_map(|entree| fs::read_to_string(entree.ok()?.path().join("stat")).ok())
            .filter_map(|stat| parent(&stat))
            .collect();
        let pids: Vec<String> =
            std::iter::once(pid).chain(descendants(pid, &liens)).map(|pid| pid.to_string()).collect();
        let mut kill = Command::new("kill");
        kill.arg("-KILL").args(&pids);
        (kill, format!("kill -KILL {}", pids.join(" ")))
    };
    let sortie = tueur.output().map_err(|e| EchecDeMiseAMort::NonParti(format!("{nom} impossible : {e}")))?;
    if sortie.status.success() {
        return Ok(());
    }
    let (erreur, texte) = (String::from_utf8_lossy(&sortie.stderr), String::from_utf8_lossy(&sortie.stdout));
    Err(EchecDeMiseAMort::CodeNonNul(format!("{nom} ({}) : {}{}", sortie.status, erreur.trim(), texte.trim())))
}

/// L'échec de [`tuer_arbre`] : l'outil n'a pas pu partir, ou il a tourné et rendu un code non nul. Le texte le dit.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum EchecDeMiseAMort {
    NonParti(String),
    CodeNonNul(String),
}

impl EchecDeMiseAMort {
    fn texte(self) -> String {
        match self {
            Self::NonParti(texte) | Self::CodeNonNul(texte) => texte,
        }
    }
}

/// Tue l'arbre de l'enfant ([`tuer_arbre`], avant tout `wait`), tue l'enfant, attend [`MORT`] au plus qu'il sorte, et
/// rend l'échec de la mise à mort de l'arbre, s'il y en a un. Sous Windows, un code non nul de `taskkill` n'est pas un
/// échec si l'enfant, racine de l'arbre, est sorti à la fin de l'attente.
pub(crate) fn tuer_et_attendre(enfant: &mut Child) -> Result<(), String> {
    let tuee = tuer_arbre(enfant.id());
    let _ = enfant.kill();
    let sorti = matches!(attendre(enfant, Instant::now() + MORT), Ok(Some(_)));
    match tuee {
        Err(EchecDeMiseAMort::CodeNonNul(_)) if cfg!(windows) && sorti => Ok(()),
        autre => autre.map_err(EchecDeMiseAMort::texte),
    }
}

/// Tue, une fois la main sortie, tout processus du compte de Lydite dont le dossier courant est dans `bac` : ce que la
/// main a laissé derrière elle (un `cargo test` lancé en arrière-plan) survit à `tuer_arbre`, car un orphelin n'est
/// plus un descendant. Le bac est à la main seule. Lydite lui-même n'est jamais visé, et les autres mains, qui
/// travaillent dans d'autres bacs, non plus. Sous Windows, comme ailleurs que sous Linux, rien ne change : `Ok`.
// raccourci : un orphelin qui a quitté le bac (`cd`) ou né entre le relevé et la mise à mort survit ; monter vers un
// cgroup avec le nœud
pub(crate) fn tuer_les_restes(bac: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        let bac = fs::canonicalize(bac).or_else(|_| path::absolute(bac));
        let bac = bac.map_err(|e| format!("bac illisible : {e}"))?;
        // La racine du système n'est le bac de personne : tout processus y serait « dans le bac ».
        if bac.file_name().is_none() {
            return Ok(());
        }
        let proc = fs::read_dir("/proc").map_err(|e| format!("/proc illisible : {e}"))?;
        let compte = fs::metadata("/proc/self").map_err(|e| format!("/proc/self illisible : {e}"))?.uid();
        let moi = std::process::id();
        let pids: Vec<u32> = proc
            .filter_map(|entree| {
                let chemin = entree.ok()?.path();
                let pid: u32 = chemin.file_name()?.to_str()?.parse().ok()?;
                let du_compte = fs::metadata(&chemin).ok()?.uid() == compte;
                let ici = dans_le_bac(&fs::read_link(chemin.join("cwd")).ok()?, &bac);
                (pid != moi && du_compte && ici).then_some(pid)
            })
            .collect();
        if pids.is_empty() {
            return Ok(());
        }
        let noms: Vec<String> = pids.iter().map(u32::to_string).collect();
        let sortie = Command::new("kill")
            .arg("-KILL")
            .args(&noms)
            .output()
            .map_err(|e| format!("kill -KILL {} impossible : {e}", noms.join(" ")))?;
        // Un processus mort entre le relevé et le coup n'est pas un échec ; un qui vit encore en est un.
        let vivants: Vec<&String> = noms.iter().filter(|pid| vivant(pid)).collect();
        if sortie.status.success() || vivants.is_empty() {
            return Ok(());
        }
        let (erreur, texte) = (String::from_utf8_lossy(&sortie.stderr), String::from_utf8_lossy(&sortie.stdout));
        Err(format!("kill -KILL {} ({}) : {}{}", noms.join(" "), sortie.status, erreur.trim(), texte.trim()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = bac;
        Ok(())
    }
}

/// Le dossier courant `cwd` d'un processus est dans `bac`, lui compris. Le dossier d'un bac effacé sous le processus se
/// lit avec le suffixe ` (deleted)`.
#[cfg(target_os = "linux")]
fn dans_le_bac(cwd: &Path, bac: &Path) -> bool {
    let efface = cwd.to_string_lossy();
    cwd.starts_with(bac) || efface.strip_suffix(" (deleted)").is_some_and(|c| Path::new(c).starts_with(bac))
}

/// Le processus de ce pid tourne encore, sans être zombie.
#[cfg(target_os = "linux")]
fn vivant(pid: &str) -> bool {
    fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|stat| {
        let etat = stat.rsplit_once(')').and_then(|(_, reste)| reste.split_whitespace().next());
        !matches!(etat, Some("Z" | "X"))
    })
}

/// Le processus d'une ligne de `/proc/<pid>/stat` et son parent, `(pid, ppid)`. Le nom du programme, entre parenthèses,
/// peut porter des blancs et des parenthèses : l'état, puis le parent, se lisent après la dernière `)`.
fn parent(stat: &str) -> Option<(u32, u32)> {
    let (tete, reste) = stat.rsplit_once(')')?;
    let pid = tete.split_once(' ')?.0.parse().ok()?;
    let ppid = reste.split_whitespace().nth(1)?.parse().ok()?;
    Some((pid, ppid))
}

/// Les descendants de `racine` parmi ces liens `(pid, ppid)`, de proche en proche, sans `racine` ni doublon.
fn descendants(racine: u32, liens: &[(u32, u32)]) -> Vec<u32> {
    let mut trouves = Vec::new();
    let mut a_voir = vec![racine];
    while let Some(pere) = a_voir.pop() {
        for &(pid, ppid) in liens {
            if ppid == pere && pid != racine && !trouves.contains(&pid) {
                trouves.push(pid);
                a_voir.push(pid);
            }
        }
    }
    trouves
}

/// La variable de ce nom ne passe ni à la main ni à sa preuve : toute marque de session de Claude (`CLAUDE*`, sauf
/// `CLAUDE_CONFIG_DIR`) ; les jetons `GH_TOKEN`, `GITHUB_TOKEN`, `GH_ENTERPRISE_TOKEN` et `GITHUB_ENTERPRISE_TOKEN` et
/// les moyens de s'authentifier comme l'humain, `GIT_SSH_COMMAND`, `GIT_SSH`, `GIT_ASKPASS`, `SSH_ASKPASS` et
/// `SSH_AUTH_SOCK`, car un jeton ou un moyen de s'authentifier est un droit et aucun n'est prêté (Kraken les garde pour
/// ses propres gestes, le push de la forge) ; les [`VARIABLES_DE_DEPOT`] de git, sans quoi chaque git que jouent la
/// main ou sa preuve dans le bac (`add`, `commit`, `reset`) jouerait dans le dépôt qu'elles nomment. La casse ne compte
/// pas : l'environnement de Windows l'ignore. `ANTHROPIC_*`, `CLAUDE_CONFIG_DIR`, `MCP_*` et les fichiers de
/// configuration de git de l'humain (`GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`) passent à la main ; sa preuve perd en
/// plus `CLAUDE_CONFIG_DIR` et `ANTHROPIC_*` ([`retire_de_la_preuve`]). Le fournisseur d'un profil n'y change rien : sa
/// main a ses propres `ANTHROPIC_BASE_URL` et `ANTHROPIC_API_KEY`, perd un `ANTHROPIC_AUTH_TOKEN` hérité et la variable
/// que `cle` nomme (`partir`), et sa preuve perd de plus la variable qui tient la clé (`executer`).
pub(crate) fn retire_de_l_env(nom: &str) -> bool {
    let nom = nom.to_ascii_uppercase();
    let jetons = ["GH_TOKEN", "GITHUB_TOKEN", "GH_ENTERPRISE_TOKEN", "GITHUB_ENTERPRISE_TOKEN"];
    let authentification = ["GIT_SSH_COMMAND", "GIT_SSH", "GIT_ASKPASS", "SSH_ASKPASS", "SSH_AUTH_SOCK"];
    (nom.starts_with("CLAUDE") && nom != "CLAUDE_CONFIG_DIR")
        || jetons.contains(&nom.as_str())
        || authentification.contains(&nom.as_str())
        || VARIABLES_DE_DEPOT.contains(&nom.as_str())
}

/// La variable de ce nom ne passe pas à la preuve que Kraken rejoue : tout ce que [`retire_de_l_env`] ôte à la main, et
/// de plus `CLAUDE_CONFIG_DIR` et `ANTHROPIC_*`, que la main garde. La connexion de la main à Claude n'est pas celle de
/// sa preuve, qui joue le test de l'humain et non Claude.
pub(crate) fn retire_de_la_preuve(nom: &str) -> bool {
    let nom = nom.to_ascii_uppercase();
    retire_de_l_env(&nom) || nom == "CLAUDE_CONFIG_DIR" || nom.starts_with("ANTHROPIC_")
}

/// La commande, sans les variables de l'environnement de Kraken que `retire` désigne : [`retire_de_l_env`] pour la
/// main, [`retire_de_la_preuve`] pour sa preuve.
// raccourci : lancé depuis une session Claude, son ANTHROPIC_BASE_URL passe à la main ; une variable CLAUDE* de réglage
// de l'humain part, et le bloc env de ses réglages la redonne ; monter (ôter aussi ANTHROPIC_BASE_URL à la main) si une
// main lancée depuis l'app ne s'authentifie pas
pub(crate) fn sans_session(commande: &mut Command, retire: fn(&str) -> bool) -> &mut Command {
    for (nom, _) in env::vars_os() {
        if retire(&nom.to_string_lossy()) {
            commande.env_remove(nom);
        }
    }
    commande
}

pub(crate) fn coquille(commande: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut c = Command::new("cmd");
        c.arg("/C").raw_arg(commande);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.args(["-c", commande]);
        c
    }
}

#[cfg(test)]
mod tests;
