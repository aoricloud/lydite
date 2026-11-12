//! Les tests des gestes git.
use super::*;
#[cfg(unix)]
use crate::essais::neuf;
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

#[test]
fn un_geste_ote_de_l_enfant_les_variables_qui_changent_la_lecture_des_pathspecs() {
    // Sans elles ôtées, `GIT_LITERAL_PATHSPECS=1` ferait lire mot pour mot les pathspecs à magie des refus 7006 et
    // 7021, qui ne trouveraient plus rien. Le test lit ce que la `Command` ôte à l'enfant, sans rien poser dans son
    // processus.
    let git = geste(Path::new("."), &["status"]);
    let otees: Vec<&OsStr> = git.get_envs().filter(|(_, valeur)| valeur.is_none()).map(|(nom, _)| nom).collect();
    let noms = ["GIT_LITERAL_PATHSPECS", "GIT_GLOB_PATHSPECS", "GIT_NOGLOB_PATHSPECS", "GIT_ICASE_PATHSPECS"];
    for nom in noms {
        assert!(otees.contains(&OsStr::new(nom)), "{nom} passerait à git : {otees:?}");
    }
}

/// Le SHA-256 de ces octets, en hexadécimaux.
fn sha(octets: &str) -> String {
    hex(&sha256(octets.as_bytes()))
}

/// Commite l'index du dépôt et rend le sha du commit.
fn commiter(dir: &Path) -> String {
    git(dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "x"]);
    git(dir, &["rev-parse", "HEAD"])
}

#[test]
fn la_lecture_d_un_fichier_epingle_dit_ce_que_git_garde_dans_l_index_et_dans_un_commit() {
    let dir = depot("empreinte-lue");
    ecrire(&dir, "a.txt", "a\n");
    ecrire(&dir, "src/b.txt", "b\n");
    git(&dir, &["add", "."]);
    // Dans l'index, avant tout commit.
    let lu = |arbre, chemin: &str| empreinte_gardee(&dir, arbre, chemin, BORNE_EMPREINTE);
    assert_eq!(lu(Arbre::Index, "a.txt"), Ok(Gardee::Empreinte(sha("a\n"))));
    assert_eq!(lu(Arbre::Index, "src/b.txt"), Ok(Gardee::Empreinte(sha("b\n"))));
    assert_eq!(lu(Arbre::Index, "absent.txt"), Ok(Gardee::Absent));
    // Un dossier n'est pas un fichier : l'index n'a pas d'entrée à son nom.
    assert_eq!(lu(Arbre::Index, "src"), Ok(Gardee::Absent));
    // Dans un commit, les mêmes réponses ; le fichier de l'index que le commit n'a pas encore est absent.
    let commit = commiter(&dir);
    assert_eq!(lu(Arbre::Commit(&commit), "a.txt"), Ok(Gardee::Empreinte(sha("a\n"))));
    assert_eq!(lu(Arbre::Commit(&commit), "src/b.txt"), Ok(Gardee::Empreinte(sha("b\n"))));
    assert_eq!(lu(Arbre::Commit(&commit), "absent.txt"), Ok(Gardee::Absent));
    assert_eq!(lu(Arbre::Commit(&commit), "src"), Ok(Gardee::PasOrdinaire("040000".into())));
    // Ce que git garde, pas le disque : le fichier du disque change, l'index et le commit gardent leurs octets.
    ecrire(&dir, "a.txt", "changé\n");
    assert_eq!(lu(Arbre::Index, "a.txt"), Ok(Gardee::Empreinte(sha("a\n"))));
    assert_eq!(lu(Arbre::Commit(&commit), "a.txt"), Ok(Gardee::Empreinte(sha("a\n"))));
    // Un fichier exécutable est ordinaire.
    git(&dir, &["update-index", "--chmod=+x", "src/b.txt"]);
    assert_eq!(lu(Arbre::Index, "src/b.txt"), Ok(Gardee::Empreinte(sha("b\n"))));
    // Un sous-dépôt n'est pas un fichier ordinaire, et rien n'est lu de lui.
    let objet = "1111111111111111111111111111111111111111";
    git(&dir, &["update-index", "--add", "--cacheinfo", &format!("160000,{objet},sous")]);
    assert_eq!(lu(Arbre::Index, "sous"), Ok(Gardee::PasOrdinaire("160000".into())));
    let _ = fs::remove_dir_all(&dir);
}

// Sous Unix seulement : git pour Windows refuse d'inscrire dans l'index une entrée nommée `:(icase)A.txt`
// (« error: Invalid path »), car un `:` n'est pas un nom de fichier sous Windows.
#[cfg(unix)]
#[test]
fn la_lecture_d_un_fichier_epingle_lit_le_chemin_a_la_lettre_meme_quand_il_ressemble_a_une_magie() {
    let dir = depot("empreinte-lue-magie");
    ecrire(&dir, "a.txt", "a\n");
    ecrire(&dir, "motif.txt", "le motif\n");
    git(&dir, &["add", "a.txt"]);
    // Une entrée dont le nom est une magie de pathspec, posée dans l'index sans fichier du disque : lu comme magie,
    // `:(icase)A.txt` désignerait `a.txt`.
    let motif = git(&dir, &["hash-object", "-w", "motif.txt"]);
    git(&dir, &["update-index", "--add", "--cacheinfo", &format!("100644,{motif},:(icase)A.txt")]);
    let lu = |arbre, chemin: &str| empreinte_gardee(&dir, arbre, chemin, BORNE_EMPREINTE);
    // Le chemin se lit à la lettre : `:(icase)A.txt` est le nom d'une entrée, pas une magie.
    assert_eq!(lu(Arbre::Index, ":(icase)A.txt"), Ok(Gardee::Empreinte(sha("le motif\n"))));
    assert_eq!(lu(Arbre::Index, ":(icase)a.txt"), Ok(Gardee::Absent));
    // Dans un commit, les mêmes réponses.
    let commit = commiter(&dir);
    assert_eq!(lu(Arbre::Commit(&commit), ":(icase)A.txt"), Ok(Gardee::Empreinte(sha("le motif\n"))));
    assert_eq!(lu(Arbre::Commit(&commit), ":(icase)a.txt"), Ok(Gardee::Absent));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_lecture_d_un_fichier_epingle_rend_trop_lourd_un_fichier_de_plus_que_la_borne() {
    let dir = depot("empreinte-borne");
    ecrire(&dir, "a.txt", "abcd\n");
    git(&dir, &["add", "."]);
    let commit = commiter(&dir);
    for arbre in [Arbre::Index, Arbre::Commit(&commit)] {
        // Cinq octets : la borne de quatre ne les prend pas, celle de cinq oui.
        assert_eq!(empreinte_gardee(&dir, arbre, "a.txt", 4), Ok(Gardee::TropLourd), "{arbre:?}");
        assert_eq!(empreinte_gardee(&dir, arbre, "a.txt", 5), Ok(Gardee::Empreinte(sha("abcd\n"))), "{arbre:?}");
    }
    assert_eq!(BORNE_EMPREINTE, 67_108_864);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_git_qui_echoue_n_est_pas_un_fichier_absent_pour_la_lecture_d_un_fichier_epingle() {
    let dir = depot("empreinte-git-echoue");
    ecrire(&dir, "a.txt", "a\n");
    git(&dir, &["add", "."]);
    commiter(&dir);
    let inconnu = "0123456789abcdef0123456789abcdef01234567";
    let vu = empreinte_gardee(&dir, Arbre::Commit(inconnu), "a.txt", BORNE_EMPREINTE);
    assert!(vu.as_ref().is_err_and(|erreur| erreur.contains("ls-tree")), "{vu:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// Un lien symbolique du bac vers `cible` : `false` si le système ne le permet pas (Windows sans le droit).
#[cfg(unix)]
fn lier(cible: &Path, lien: &Path) -> bool {
    std::os::unix::fs::symlink(cible, lien).is_ok()
}

#[cfg(unix)]
#[test]
fn la_lecture_d_un_fichier_epingle_ne_lit_rien_hors_du_bac_par_un_lien_symbolique() {
    let dir = depot("empreinte-liens");
    // Hors du bac : un fichier dont l'empreinte ne doit paraître nulle part.
    let dehors = neuf("empreinte-liens-dehors");
    fs::create_dir_all(&dehors).unwrap();
    ecrire(&dehors, "secret.txt", "secret\n");
    let secret = sha("secret\n");
    // Un lien symbolique à la place du fichier : git l'a en mode 120000, et rien n'est lu de ce qu'il vise.
    assert!(lier(&dehors.join("secret.txt"), &dir.join("lien.txt")));
    // Un fichier derrière un dossier qui est un lien vers un dossier hors du bac : git n'a aucune entrée à ce chemin.
    assert!(lier(&dehors, &dir.join("dossier")));
    git(&dir, &["add", "."]);
    let commit = commiter(&dir);
    for arbre in [Arbre::Index, Arbre::Commit(&commit)] {
        let lien = empreinte_gardee(&dir, arbre, "lien.txt", BORNE_EMPREINTE);
        assert_eq!(lien, Ok(Gardee::PasOrdinaire("120000".into())), "{arbre:?}");
        let derriere = empreinte_gardee(&dir, arbre, "dossier/secret.txt", BORNE_EMPREINTE);
        assert_eq!(derriere, Ok(Gardee::Absent), "{arbre:?}");
        let dit = format!("{lien:?} {derriere:?}");
        assert!(!dit.contains(&secret), "{dit}");
    }
    // Le fichier visé existe bien sur le disque, derrière le lien : la lecture par le disque l'aurait trouvé.
    assert_eq!(fs::read_to_string(dir.join("dossier/secret.txt")).unwrap(), "secret\n");
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&dehors);
}

// Les tests du dépôt visé (ses racines, le dossier de Kraken hors de lui), à part : `depot` y nomme l'aide des tests
// qui pose un dépôt sous une racine, pas `depot_neuf`.
mod racines;
