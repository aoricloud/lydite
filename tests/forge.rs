//! La forge de `kraken lancer <profil> --plan <fichier> --pr <branche>`, jouée sur le binaire lui-même : sa ligne de
//! commande, et les refus d'avant la vague (8001 à 8003), chacun en code 2, sans bac ni note. Chaque test lance
//! `kraken` sur un dépôt git jetable qui a un commit, avec un dossier de Kraken jetable hors de ce dépôt, un `claude`
//! qui n'existe pas et un faux `gh`, que `KRAKEN_GH` nomme sur l'enfant seul : aucun vrai `claude` ni `gh` ne part, et
//! le seul distant est un dépôt nu local. Le dossier personnel et `CLAUDE_CONFIG_DIR` vont, sur l'enfant seul aussi, à
//! des leurres : rien de l'humain n'est lu, ce que le dernier essai éprouve en rejouant les refus sous un environnement
//! hostile. Le chemin d'après la vague se joue au niveau du crate (`src/conduite/publication.rs`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// L'URL d'`origin` dans ces essais : `forge.test` ne se résout jamais, et `insteadOf` la renvoie au dépôt nu.
const URL: &str = "https://forge.test/p/d";

/// Une racine neuve et vide, à ce test, dans le dossier temporaire du système : le dépôt visé (`depot`), son plan, le
/// dossier de Kraken (`kraken`), le faux `gh` et le distant nu y vivent côte à côte, hors de ce dépôt-ci.
fn racine(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kraken-forge-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Joue git dans `dir` et rend sa sortie standard, rognée.
fn git(dir: &Path, args: &[&str]) -> String {
    let sortie = Command::new("git").current_dir(dir).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
    String::from_utf8_lossy(&sortie.stdout).trim().to_string()
}

/// Un dépôt git neuf avec un commit, dans `<racine>/depot`, et le plan d'une tâche, `<racine>/plan.txt`.
fn depot(racine: &Path) -> PathBuf {
    let dir = racine.join("depot");
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    git(&dir, &[&identite[..], &["commit", "-q", "--allow-empty", "-m", "init"]].concat());
    fs::write(racine.join("plan.txt"), "plan 1\nobjectif : o\npreuve : exit 0\n\ntache a\n    Ecrire a\n").unwrap();
    dir
}

/// Le distant `origin` du dépôt : [`URL`], qu'`insteadOf` renvoie au dépôt nu `<racine>/distant.git`, où le `HEAD` du
/// dépôt est poussé dans `main`.
fn distant(racine: &Path, depot: &Path) {
    let nu = racine.join("distant.git");
    git(racine, &["init", "-q", "--bare", &nu.to_string_lossy()]);
    git(depot, &["remote", "add", "origin", URL]);
    // Avec des `/` : git lit `C:/…` comme un chemin du poste sous Windows.
    git(depot, &["config", &format!("url.{}.insteadOf", nu.to_string_lossy().replace('\\', "/")), URL]);
    git(depot, &["push", "-q", "origin", "HEAD:refs/heads/main"]);
}

/// Un faux `gh` qui ne se dit connecté à rien : il ajoute ses arguments à `<racine>/gh-appels.txt`, écrit « You are not
/// logged into any GitHub hosts. » sur ses erreurs, et sort en 1. Un `.cmd` sous Windows, un script `sh` ailleurs.
fn faux_gh(racine: &Path) -> PathBuf {
    let appels = racine.join("gh-appels.txt");
    let (chemin, texte) = if cfg!(windows) {
        let texte = format!(
            "@echo off\r\n>>\"{}\" echo %*\r\necho You are not logged into any GitHub hosts. 1>&2\r\nexit 1\r\n",
            appels.display()
        );
        (racine.join("gh.cmd"), texte)
    } else {
        let texte = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\necho 'You are not logged into any GitHub hosts.' >&2\nexit 1\n",
            appels.display()
        );
        (racine.join("gh"), texte)
    };
    fs::write(&chemin, texte).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    chemin
}

/// Les appels du faux `gh`, un par ligne, sans les guillemets que Windows ajoute à certains.
fn appels(racine: &Path) -> Vec<String> {
    let texte = fs::read_to_string(racine.join("gh-appels.txt")).unwrap_or_default();
    texte.lines().map(|ligne| ligne.replace('"', "").trim().to_string()).collect()
}

/// `kraken` joué dans la racine, avec ces arguments. Sur l'enfant seul : le dossier de Kraken `<racine>/kraken`, un
/// `claude` absent (le test ne retire jamais cette variable : sans elle, Kraken prendrait `claude` dans le PATH), le
/// faux `gh`, et `USERPROFILE`, `HOME` et `CLAUDE_CONFIG_DIR` vers des leurres sous la racine, jamais créés, d'où
/// Kraken tire les dossiers de connexion de Claude : le vrai dossier personnel de l'humain n'est jamais lu.
fn kraken(racine: &Path, args: &[&str]) -> Output {
    let personnel = racine.join("personnel");
    Command::new(env!("CARGO_BIN_EXE_kraken"))
        .args(args)
        .current_dir(racine)
        .env("KRAKEN_DOSSIER", racine.join("kraken"))
        .env("KRAKEN_CLAUDE", racine.join("claude-absent"))
        .env("KRAKEN_GH", faux_gh(racine))
        .env("USERPROFILE", &personnel)
        .env("HOME", personnel)
        .env("CLAUDE_CONFIG_DIR", racine.join("config"))
        .output()
        .unwrap()
}

/// `kraken lancer claude --plan plan.txt --depot depot --pr <branche>`, joué dans la racine.
fn lancer_la_pr(racine: &Path, branche: &str) -> Output {
    kraken(racine, &["lancer", "claude", "--plan", "plan.txt", "--depot", "depot", "--pr", branche])
}

/// Le refus que Kraken a écrit : le code 2, sans panique et sans rien sur stdout, et quatre lignes sur stderr. Rend sa
/// première ligne, `kraken : <numéro> <nom>`, et sa preuve.
fn lire_refus(sortie: &Output) -> (String, String) {
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{erreur}");
    assert!(!erreur.contains("panicked"), "{erreur}");
    assert!(sortie.stdout.is_empty(), "stdout : {}", String::from_utf8_lossy(&sortie.stdout));
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    let preuve = lignes[2].strip_prefix("  preuve : ").unwrap_or_else(|| panic!("{erreur}"));
    assert!(lignes[3].starts_with("  correction : ") && lignes[3].len() > "  correction : ".len(), "{erreur}");
    (lignes[0].to_string(), preuve.to_string())
}

/// Rien n'est parti : le dossier de Kraken n'a ni fil, ni bac, ni sortie d'une commande de la forge, et le dépôt visé
/// n'a ni branche de Kraken, ni ref de Kraken (celle où il lit la base du distant se retire avec sa lecture), ni
/// `FETCH_HEAD` écrit par lui, ni autre worktree que le sien.
fn rien_n_est_parti(racine: &Path) {
    let (depot, kraken) = (racine.join("depot"), racine.join("kraken"));
    let restes: Vec<_> = fs::read_dir(&kraken)
        .map(|entrees| entrees.map(|entree| entree.unwrap().file_name()).collect())
        .unwrap_or_default();
    assert!(restes.is_empty(), "le dossier de Kraken reste vide : {restes:?}");
    assert_eq!(git(&depot, &["branch", "--list", "kraken/*"]), "", "ni branche");
    assert_eq!(git(&depot, &["for-each-ref", "refs/kraken"]), "", "ni ref à Kraken");
    assert!(!depot.join(".git").join("FETCH_HEAD").exists(), "Kraken n'écrit pas FETCH_HEAD");
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
}

#[test]
fn pr_ne_va_qu_avec_un_plan_et_veut_une_branche() {
    let racine = racine("ligne");
    depot(&racine);
    // Sans `--plan`, ou avec une valeur que git lirait comme une option : la ligne est refusée, avant tout.
    let lignes: [&[&str]; 2] = [
        &["lancer", "claude", "objectif", "--depot", "depot", "--pr", "main"],
        &["lancer", "claude", "--plan", "plan.txt", "--depot", "depot", "--pr", "-x"],
    ];
    for ligne in lignes {
        let (code, preuve) = lire_refus(&kraken(&racine, ligne));
        assert_eq!(code, "kraken : 1006 contrat.commande_mal_formee", "{ligne:?}");
        assert!(preuve.contains("--pr"), "{preuve}");
    }
    assert!(appels(&racine).is_empty(), "gh n'est pas lancé");
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_depot_sans_origin_est_refuse_en_8001() {
    let racine = racine("8001");
    depot(&racine);
    let (code, preuve) = lire_refus(&lancer_la_pr(&racine, "main"));
    assert_eq!(code, "kraken : 8001 forge.distant_illisible");
    assert!(preuve.contains("n'a pas de distant origin"), "{preuve}");
    assert!(appels(&racine).is_empty(), "gh n'est pas lancé");
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_base_absente_du_distant_est_refusee_en_8002() {
    let racine = racine("8002");
    let depot = depot(&racine);
    distant(&racine, &depot);
    let (code, preuve) = lire_refus(&lancer_la_pr(&racine, "absente"));
    assert_eq!(code, "kraken : 8002 forge.base_inconnue");
    assert!(preuve.contains("refs/heads/absente"), "la preuve nomme la branche cherchée : {preuve}");
    assert!(appels(&racine).is_empty(), "gh n'est pas lancé");
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn gh_qui_ne_se_dit_pas_connecte_est_refuse_en_8003() {
    let racine = racine("8003");
    let depot = depot(&racine);
    distant(&racine, &depot);
    let (code, preuve) = lire_refus(&lancer_la_pr(&racine, "main"));
    assert_eq!(code, "kraken : 8003 forge.gh_injoignable");
    assert!(preuve.contains("You are not logged into any GitHub hosts."), "{preuve}");
    assert_eq!(appels(&racine), ["auth status --hostname forge.test"], "KRAKEN_GH nomme le gh de la forge");
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn les_refus_d_avant_la_vague_ne_lisent_rien_de_l_humain() {
    let racine = racine("hostile");
    // Un environnement hostile, posé sur un second processus de ce binaire d'essais et sur lui seul :
    // `CLAUDE_CONFIG_DIR` relatif, et un dossier personnel dont le `.claude` garde un fichier de connexion (jamais lu :
    // seule son existence compte) mais ne s'écrit pas en règle de permission, une parenthèse. Les trois essais d'avant
    // la vague, joués là, attendent leurs refus 8001 à 8003 : si `kraken` laissait Kraken hériter du dossier personnel
    // ou de `CLAUDE_CONFIG_DIR` du processus, le refus 6023 prendrait leur place.
    let personnel = racine.join("personnel (hostile)");
    fs::create_dir_all(personnel.join(".claude")).unwrap();
    fs::write(personnel.join(".claude").join(".credentials.json"), "{}").unwrap();
    let essais = [
        "un_depot_sans_origin_est_refuse_en_8001",
        "une_base_absente_du_distant_est_refusee_en_8002",
        "gh_qui_ne_se_dit_pas_connecte_est_refuse_en_8003",
    ];
    let sortie = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .args(essais)
        .env("CLAUDE_CONFIG_DIR", "relatif")
        .env("USERPROFILE", &personnel)
        .env("HOME", &personnel)
        .output()
        .unwrap();
    let dit = String::from_utf8_lossy(&sortie.stdout);
    // Trois essais passés : un nom qui change ne laisse pas l'épreuve passer à vide.
    assert!(sortie.status.success() && dit.contains("3 passed"), "{dit}");
    let _ = fs::remove_dir_all(&racine);
}
