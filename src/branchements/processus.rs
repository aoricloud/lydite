//! Les outils de processus de la main, de sa preuve et de la forge : l'échéance, l'attente, la mise à mort d'un
//! arbre, la coquille, et l'environnement ôté à une main et à sa preuve.

use std::env;
use std::fs;
use std::io;
use std::process::{Child, Command, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use crate::branchements::git::VARIABLES_DE_DEPOT;

/// Le temps qu'a une main pour sortir après son résultat, et celui qu'un arbre tué a pour finir de sortir.
pub(crate) const SORTIE: Duration = Duration::from_secs(10);

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
// monter vers un Job Object ou un cgroup avec le nœud
pub(crate) fn tuer_arbre(pid: u32) -> Result<(), String> {
    let (mut tueur, nom) = if cfg!(windows) {
        let mut taskkill = Command::new("taskkill");
        taskkill.args(["/T", "/F", "/PID", &pid.to_string()]);
        (taskkill, format!("taskkill /T /F /PID {pid}"))
    } else {
        let proc = fs::read_dir("/proc").map_err(|e| format!("/proc illisible : {e}"))?;
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
    let sortie = tueur.output().map_err(|e| format!("{nom} impossible : {e}"))?;
    if sortie.status.success() {
        return Ok(());
    }
    let (erreur, texte) = (String::from_utf8_lossy(&sortie.stderr), String::from_utf8_lossy(&sortie.stdout));
    Err(format!("{nom} ({}) : {}{}", sortie.status, erreur.trim(), texte.trim()))
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
