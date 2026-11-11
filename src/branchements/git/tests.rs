//! Les tests des gestes git.
use super::*;
use crate::essais::{depot_neuf as depot, ecrire, git};
use std::ffi::OsStr;
use std::fs;

#[test]
fn un_echec_de_faire_borne_dit_les_octets_non_gardes() {
    let depot = depot("borne-echec");
    ecrire(&depot, "a.txt", "a\n");
    git(&depot, &["add", "a.txt"]);
    // `a.txt` sur la sortie standard (6 octets), puis `absent` ne correspond à rien : git sort en 1, et 4 octets
    // de `a.txt` restent hors d'une garde de 2.
    let commande = || geste(&depot, &["ls-files", "--error-unmatch", "a.txt", "absent"]);
    let coupee = faire_borne(commande(), 2).unwrap_err();
    assert!(coupee.contains("4 more bytes on the standard output, not kept"), "{coupee}");
    // Gardé en entier, le même échec ne dit rien des octets non gardés.
    let entiere = faire_borne(commande(), 6).unwrap_err();
    assert!(!entiere.contains("not kept"), "{entiere}");
    let _ = fs::remove_dir_all(&depot);
}

#[test]
fn un_geste_ote_de_l_enfant_les_variables_qui_nomment_un_depot() {
    // Le test ne pose rien dans son processus : il lit ce que la `Command` ôte à l'enfant (`None`), que le
    // processus qui le joue porte ces variables ou non.
    let git = geste(Path::new("."), &["status"]);
    let otees: Vec<&OsStr> = git.get_envs().filter(|(_, valeur)| valeur.is_none()).map(|(nom, _)| nom).collect();
    let noms = [
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
    for nom in noms {
        assert!(otees.contains(&OsStr::new(nom)), "{nom} passerait à git : {otees:?}");
    }
    // Ce que le geste laisse passer, sans l'ôter ni le poser : les moyens de s'authentifier, que Kraken garde pour ses
    // propres gestes (le push de la forge), et les fichiers de configuration de l'humain.
    let gardees = [
        "GIT_SSH_COMMAND",
        "GIT_SSH",
        "GIT_ASKPASS",
        "SSH_ASKPASS",
        "SSH_AUTH_SOCK",
        "GIT_CONFIG_GLOBAL",
        "GIT_CONFIG_SYSTEM",
    ];
    for nom in gardees {
        assert!(git.get_envs().all(|(vue, _)| vue != OsStr::new(nom)), "{nom} serait touchée par le geste");
    }
}

// Les tests du dépôt visé (ses racines, le dossier de Kraken hors de lui), à part : `depot` y nomme l'aide des tests
// qui pose un dépôt sous une racine, pas `depot_neuf`.
mod racines;
