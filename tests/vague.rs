//! La vague d'un plan, jouée sur le binaire lui-même : `lydite lancer <profil> --plan <fichier>`. Chaque test lance
//! `kraken` sur un dépôt git jetable qui a un commit, avec un dossier de Kraken jetable hors de ce dépôt et un `claude`
//! qui n'existe pas : un plan accepté par erreur ne lancerait aucune vraie main. Aucun nom de branche n'est supposé.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Une racine neuve et vide, à ce test, dans le dossier temporaire du système : le dépôt visé (`depot`), le dossier de
/// Kraken (`kraken`, que rien ne crée avant un plan juste) et les plans y vivent côte à côte, hors de ce dépôt-ci.
fn racine(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-vague-{nom}-{}", std::process::id()));
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

/// Un dépôt git neuf avec un commit, dans `<racine>/depot`. L'identité du commit passe par `-c`.
fn depot(racine: &Path) -> PathBuf {
    let dir = racine.join("depot");
    fs::create_dir_all(&dir).unwrap();
    git(&dir, &["init", "-q"]);
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"];
    git(&dir, &[&identite[..], &["commit", "-q", "--allow-empty", "-m", "init"]].concat());
    dir
}

/// Écrit le plan `nom` dans la racine, ses lignes finies par `\n`, et rend son chemin.
fn plan(racine: &Path, nom: &str, lignes: &[&str]) -> PathBuf {
    let chemin = racine.join(nom);
    fs::write(&chemin, lignes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>()).unwrap();
    chemin
}

/// `lydite lancer claude --plan <plan> --depot <racine>/depot`, joué dans la racine.
fn lancer_le_plan(racine: &Path, plan: &Path) -> Output {
    commande_du_plan(racine, plan).output().unwrap()
}

/// La commande de `lancer_le_plan`, avant qu'elle ne parte : un test y pose ce qu'il veut sur l'enfant seul. Le dossier
/// de Kraken est `<racine>/lydite`, `LYDITE_CLAUDE` vise un programme absent, et `USERPROFILE`, `HOME` et
/// `CLAUDE_CONFIG_DIR` vont à des leurres sous la racine, jamais créés, d'où Kraken tire les dossiers de connexion de
/// Claude : posés sur l'enfant seul, ils ne touchent pas le processus du test, et le vrai dossier personnel de l'humain
/// n'est jamais lu. Le test ne retire jamais `LYDITE_CLAUDE` : sans elle, Kraken prendrait `claude` dans le PATH.
fn commande_du_plan(racine: &Path, plan: &Path) -> Command {
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande
        .args(["lancer", "claude", "--plan"])
        .arg(plan)
        .arg("--depot")
        .arg(racine.join("depot"))
        .current_dir(racine)
        .env("LYDITE_DOSSIER", racine.join("lydite"))
        .env("LYDITE_CLAUDE", racine.join("claude-absent"))
        .env("LYDITE_SRT", faux_srt(racine));
    avec_leurres(&mut commande, racine);
    commande
}

/// Le faux srt des essais, `<racine>/faux-srt.sh` (`.cmd` sous Windows), que `LYDITE_SRT` nomme : le profil intégré
/// exige le mur, et ces essais ne portent pas sur lui. Il saute `--settings <fichier>`, puis joue la preuve ; `true`,
/// l'épreuve du mur, sort en 0. Rend son chemin ; l'écrire deux fois donne le même fichier.
fn faux_srt(racine: &Path) -> PathBuf {
    let (chemin, lignes) = if cfg!(windows) {
        let lignes = ["@echo off", r#"if "%~5"=="true" exit /b 0"#, "%~5", "exit /b %errorlevel%"];
        (racine.join("faux-srt.cmd"), lignes.map(String::from).to_vec())
    } else {
        let lignes = ["#!/bin/sh", "shift 2", r#"exec "$@""#];
        (racine.join("faux-srt.sh"), lignes.map(String::from).to_vec())
    };
    script(&chemin, &lignes);
    chemin
}

/// Les leurres du dossier personnel et de `CLAUDE_CONFIG_DIR` que `commande` reçoit, sous la racine du test, sans le
/// dossier de configuration de gh de l'humain ou de la CI, qui changerait les réglages attendus.
fn avec_leurres<'a>(commande: &'a mut Command, racine: &Path) -> &'a mut Command {
    let personnel = racine.join("personnel");
    commande.env("USERPROFILE", &personnel).env("HOME", personnel).env("CLAUDE_CONFIG_DIR", racine.join("config"));
    commande.env_remove("GH_CONFIG_DIR").env_remove("XDG_CONFIG_HOME")
}

/// Un refus lu sur stderr : sa première ligne, `lydite : <numéro> <nom>`, puis la preuve et la correction, sans leur
/// étiquette.
struct Refus {
    code: String,
    preuve: String,
    correction: String,
}

/// Le refus que Kraken a écrit : le code 2, sans panique et sans rien sur stdout, et quatre lignes sur stderr.
fn lire_refus(sortie: &Output) -> Refus {
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{erreur}");
    assert!(!erreur.contains("panicked"), "{erreur}");
    assert!(sortie.stdout.is_empty(), "stdout : {}", String::from_utf8_lossy(&sortie.stdout));
    let lignes: Vec<&str> = erreur.lines().collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    assert!(lignes[1].starts_with("  rule: "), "{erreur}");
    let apres = |i: usize, etiquette: &str| {
        lignes[i].strip_prefix(etiquette).unwrap_or_else(|| panic!("{etiquette}attendu : {erreur}")).to_string()
    };
    Refus { code: lignes[0].into(), preuve: apres(2, "  seen: "), correction: apres(3, "  fix: ") }
}

/// Rien n'est parti : le dossier de Kraken n'a ni fil ni bac (le dossier `bacs/` vide que laisse l'épreuve du mur
/// est toléré), le dépôt visé n'a ni branche de Kraken ni autre worktree que le sien, et son arbre est propre.
fn rien_n_est_parti(racine: &Path) {
    let depot = racine.join("depot");
    let restes: Vec<_> = fs::read_dir(racine.join("lydite"))
        .map(|entrees| entrees.map(|entree| entree.unwrap().file_name()).collect())
        .unwrap_or_default();
    assert!(restes.is_empty() || restes == ["bacs"], "ni fil, ni bac : {restes:?}");
    let bacs = fs::read_dir(racine.join("lydite").join("bacs")).map_or(0, Iterator::count);
    assert_eq!(bacs, 0, "aucun bac ne reste");
    assert_eq!(git(&depot, &["branch", "--list", "lydite/*"]), "", "ni branche");
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
    assert_eq!(git(&depot, &["status", "--porcelain"]), "");
}

/// Une preuve qui tient un lancement en vie sans rien attendre du temps : elle pose la marque `marques/demarre`, puis
/// attend que le test pose `marques/libre`. Elle ne joue qu'après que le lancement a pris son dossier et avant qu'il
/// n'écrive au fil. Son plafond (une minute) n'est qu'un garde-fou contre une preuve orpheline : le test pose `libre`
/// à sa fin. Rend la commande de la preuve.
fn preuve_qui_attend(racine: &Path) -> String {
    let marques = racine.join("marques");
    fs::create_dir_all(&marques).unwrap();
    let (demarre, libre) = (marques.join("demarre"), marques.join("libre"));
    if cfg!(windows) {
        let bat = racine.join("attendre.bat");
        let lignes = [
            "@echo off".to_string(),
            format!("echo x> \"{}\"", demarre.display()),
            "set /a n=0".to_string(),
            ":boucle".to_string(),
            format!("if exist \"{}\" exit /b 0", libre.display()),
            "set /a n+=1".to_string(),
            "if %n% geq 60 exit /b 1".to_string(),
            "ping -n 2 127.0.0.1 >nul".to_string(),
            "goto boucle".to_string(),
        ];
        script(&bat, &lignes);
        bat.display().to_string()
    } else {
        let sh = racine.join("attendre.sh");
        let lignes = [
            format!("touch '{}'", demarre.display()),
            "n=0".to_string(),
            format!("while [ ! -f '{}' ]; do", libre.display()),
            "  n=$((n + 1))".to_string(),
            "  [ \"$n\" -ge 600 ] && exit 1".to_string(),
            "  sleep 0.1".to_string(),
            "done".to_string(),
        ];
        script(&sh, &lignes);
        format!("sh {}", sh.display())
    }
}

/// Un plan d'une tâche, de preuve `preuve`.
fn plan_de_preuve(racine: &Path, nom: &str, preuve: &str) -> PathBuf {
    plan(racine, nom, &["plan 1", "objectif : o", &format!("preuve : {preuve}"), "", "tache a", "    Écrire a.txt"])
}

/// Lance le plan et rend l'enfant dès que sa preuve a posé `marques/demarre` : le lancement tient alors son dossier,
/// et n'a encore rien écrit au fil. L'attente est celle d'une marque, jamais d'un délai ; son plafond n'est qu'un
/// garde-fou, et un enfant déjà sorti est dit avec sa sortie.
fn lancer_et_attendre_sa_preuve(racine: &Path, plan: &Path) -> Child {
    let mut enfant = commande_du_plan(racine, plan).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    let (marque, debut) = (racine.join("marques").join("demarre"), Instant::now());
    while !marque.exists() {
        if let Some(statut) = enfant.try_wait().unwrap() {
            let sortie = enfant.wait_with_output().unwrap();
            panic!("le lancement est sorti ({statut}) avant sa preuve : {}", String::from_utf8_lossy(&sortie.stderr));
        }
        assert!(debut.elapsed() < Duration::from_secs(60), "la preuve n'a jamais démarré");
        thread::sleep(Duration::from_millis(20));
    }
    enfant
}

/// Libère la preuve qui attend.
fn liberer(racine: &Path) {
    fs::write(racine.join("marques").join("libre"), "").unwrap();
}

#[test]
fn un_second_lancement_pendant_un_premier_est_refuse_en_6032_sans_rien_ecrire_au_fil() {
    let racine = racine("pris");
    depot(&racine);
    let attend = plan_de_preuve(&racine, "attend.txt", &preuve_qui_attend(&racine));
    let rapide = plan_de_preuve(&racine, "rapide.txt", "git status");
    let premier = lancer_et_attendre_sa_preuve(&racine, &attend);
    // Le premier tient le dossier et n'a pas encore ouvert sa vague : le fil n'existe pas.
    let fil = racine.join("lydite").join("fil.jsonl");
    assert!(!fil.exists());
    let refus = lire_refus(&lancer_le_plan(&racine, &rapide));
    assert_eq!(refus.code, "lydite : 6032 lancer.dossier_pris");
    assert!(refus.preuve.contains(&racine.join("lydite").display().to_string()), "{}", refus.preuve);
    assert!(refus.correction.contains("Wait for the other launch to finish"), "{}", refus.correction);
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{}", refus.correction);
    assert!(!fil.exists(), "le lancement refusé n'a rien écrit au fil");
    // Le premier finit sans être gêné : sa vague est la seule du fil, et il n'a pas été refusé.
    liberer(&racine);
    let fin = premier.wait_with_output().unwrap();
    let erreur = String::from_utf8_lossy(&fin.stderr);
    assert_eq!(fin.status.code(), Some(1), "la tâche, sans claude, est classée : {erreur}");
    // Le refus par son code et son nom : un « 6032 » nu se trouve aussi dans un id de vague (un pid comme 26032).
    assert!(!erreur.contains("6032 lancer.dossier_pris"), "{erreur}");
    let fil = fs::read_to_string(&fil).unwrap();
    assert_eq!(fil.matches(r#""type":"vague.ouverte""#).count(), 1, "{fil}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn deux_lancements_sur_deux_dossiers_passent() {
    let racine = racine("deux-dossiers");
    depot(&racine);
    let attend = plan_de_preuve(&racine, "attend.txt", &preuve_qui_attend(&racine));
    let rapide = plan_de_preuve(&racine, "rapide.txt", "git status");
    let premier = lancer_et_attendre_sa_preuve(&racine, &attend);
    // Le second prend un autre dossier pendant que le premier tient le sien : il va jusqu'au bout.
    let autre = racine.join("autre");
    let second = commande_du_plan(&racine, &rapide).env("LYDITE_DOSSIER", &autre).output().unwrap();
    let erreur = String::from_utf8_lossy(&second.stderr);
    assert_eq!(second.status.code(), Some(1), "la tâche, sans claude, est classée : {erreur}");
    assert!(!erreur.contains("6032 lancer.dossier_pris"), "{erreur}");
    assert!(autre.join("fil.jsonl").is_file(), "le second a écrit son fil : {erreur}");
    assert!(!racine.join("lydite").join("fil.jsonl").exists(), "le premier n'a rien reçu du second");
    liberer(&racine);
    let fin = premier.wait_with_output().unwrap();
    assert_eq!(fin.status.code(), Some(1), "{}", String::from_utf8_lossy(&fin.stderr));
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_lancement_tue_laisse_le_dossier_libre_et_le_suivant_part() {
    let racine = racine("tue");
    depot(&racine);
    let attend = plan_de_preuve(&racine, "attend.txt", &preuve_qui_attend(&racine));
    let rapide = plan_de_preuve(&racine, "rapide.txt", "git status");
    let mut premier = lancer_et_attendre_sa_preuve(&racine, &attend);
    premier.kill().unwrap();
    premier.wait().unwrap();
    // La preuve orpheline, si elle survit à son lancement, finit d'elle-même.
    liberer(&racine);
    let suivant = lancer_le_plan(&racine, &rapide);
    let erreur = String::from_utf8_lossy(&suivant.stderr);
    assert_eq!(suivant.status.code(), Some(1), "la tâche, sans claude, est classée : {erreur}");
    assert!(!erreur.contains("6032 lancer.dossier_pris"), "{erreur}");
    assert!(racine.join("lydite").join("fil.jsonl").is_file(), "le suivant a écrit son fil : {erreur}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_plan_faux_est_refuse_avant_tout_bac() {
    let racine = racine("faux");
    depot(&racine);
    let entete = ["plan 1", "objectif : o", "preuve : git status"];
    // Chaque plan faux : ses lignes, le code du refus que Kraken écrit, et le début de sa preuve, qui nomme le numéro
    // et le texte de la ligne fautive (hors un plan sans tâche, qui n'en vise aucune).
    let cas: [(Vec<&str>, &str, &str); 7] = [
        (
            vec!["plan 2", "objectif : o", "preuve : git status", "tache a", "    x"],
            "lydite : 6016 lancer.plan_version",
            "ligne 1 « plan 2 » : la première ligne d'un plan est `plan 1`",
        ),
        (
            [&entete[..], &["", "blabla"]].concat(),
            "lydite : 6018 lancer.plan_ligne",
            "ligne 5 « blabla » : elle n'est ni vide, ni indentée",
        ),
        (
            [&entete[..], &["tache a b", "    x"]].concat(),
            "lydite : 6019 lancer.plan_identifiant",
            "ligne 4 « tache a b » : l'id « a b » n'est pas un mot",
        ),
        (
            [&entete[..], &["tache a", "depend : zz", "    x"]].concat(),
            "lydite : 6020 lancer.plan_dependance",
            "ligne 5 « depend : zz » : la dépendance « zz » n'est l'id d'aucune tâche du plan",
        ),
        (
            [&entete[..], &["tache a", "depend : b", "    x", "tache b", "depend : a", "    y"]].concat(),
            "lydite : 6020 lancer.plan_dependance",
            "ligne 5 « depend : b » : cycle de dépendances : a → b → a",
        ),
        (
            vec!["plan 1", "objectif : o", "tache a", "    x"],
            "lydite : 6021 lancer.plan_blanc",
            "l'en-tête n'a pas de ligne `preuve : …`",
        ),
        (entete.to_vec(), "lydite : 6021 lancer.plan_blanc", "le plan ne contient aucune tâche"),
    ];
    for (i, (lignes, code, debut)) in cas.into_iter().enumerate() {
        let sortie = lancer_le_plan(&racine, &plan(&racine, &format!("plan-{i}.txt"), &lignes));
        let refus = lire_refus(&sortie);
        assert_eq!(refus.code, code, "plan {i} : {lignes:?}");
        assert!(
            refus.preuve.starts_with(debut),
            "plan {i} : la preuve devait commencer par « {debut} » : {}",
            refus.preuve
        );
        assert!(!refus.correction.trim().is_empty(), "plan {i} : la correction enseigne");
        rien_n_est_parti(&racine);
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_plan_en_utf16_est_refuse_par_le_binaire() {
    let racine = racine("utf16");
    depot(&racine);
    // Ce qu'écrit `>` sous PowerShell 5.1 : la marque FF FE, puis le texte en UTF-16 petit-boutiste, fins de ligne
    // CRLF.
    let texte = "plan 1\r\nobjectif : o\r\npreuve : git status\r\n\r\ntache a\r\n    Écrire a\r\n";
    let octets: Vec<u8> = [0xFF, 0xFE].into_iter().chain(texte.encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let chemin = racine.join("plan-utf16.txt");
    fs::write(&chemin, &octets).unwrap();
    // Les octets relus sont ceux qu'on a voulus : la marque, puis le texte, qui se décode.
    let ecrit = fs::read(&chemin).unwrap();
    assert_eq!(ecrit[..2], [0xFF, 0xFE]);
    let unites: Vec<u16> = ecrit[2..].chunks(2).map(|paire| u16::from_le_bytes([paire[0], paire[1]])).collect();
    assert_eq!(String::from_utf16(&unites).unwrap(), texte);

    let refus = lire_refus(&lancer_le_plan(&racine, &chemin));
    assert_eq!(refus.code, "lydite : 6017 lancer.plan_encodage");
    assert!(refus.preuve.contains("FF FE") && refus.preuve.contains("UTF-16"), "{}", refus.preuve);
    let commande = "(Get-Content <fichier>) | Set-Content -Encoding utf8 <fichier>";
    assert!(
        refus.correction.contains(commande),
        "la correction donne la commande qui réécrit le fichier : {}",
        refus.correction
    );
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_plan_absent_nomme_son_chemin() {
    let racine = racine("absent");
    depot(&racine);
    let absent = racine.join("absent.txt");
    let refus = lire_refus(&lancer_le_plan(&racine, &absent));
    assert_eq!(refus.code, "lydite : 6006 lancer.entree_sortie");
    assert!(
        refus.preuve.contains(&absent.display().to_string()),
        "la preuve nomme le chemin du plan : {}",
        refus.preuve
    );
    assert!(refus.correction.contains("--plan"), "la correction enseigne : {}", refus.correction);
    rien_n_est_parti(&racine);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_vague_a_plan_dit_chaque_tache_puis_sa_branche() {
    let racine = racine("vague");
    let depot = depot(&racine);
    let base = git(&depot, &["rev-parse", "HEAD"]);
    // a, puis b qui dépend d'elle ; la preuve de la vague passe sur la base. Le `claude` n'existe pas : a est classée
    // après sa relance, et b, qui dépend d'elle, est sautée.
    let chemin = plan(
        &racine,
        "plan.txt",
        &[
            "plan 1",
            "objectif : Écrire a, puis b d'après a",
            "preuve : git status",
            "",
            "tache a",
            "    Écrire a.txt",
            "",
            "tache b",
            "depend : a",
            "    Écrire b.txt d'après a.txt",
        ],
    );
    let sortie = lancer_le_plan(&racine, &chemin);
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(1), "une tâche non assemblée donne 1 : {erreur}");
    assert!(sortie.stdout.is_empty(), "aucune sortie prise : {}", String::from_utf8_lossy(&sortie.stdout));
    assert!(!erreur.contains("panicked"), "{erreur}");

    // La dernière ligne dit la branche de la vague, d'où se tire le lot `<v>` de ses mains.
    let lignes: Vec<&str> = erreur.lines().collect();
    let derniere = lignes.last().copied().unwrap_or_default();
    let Some(v) = derniere.strip_prefix("branche de la vague : lydite/vague-") else { panic!("{erreur}") };
    // a : classée, puis son erreur, celle d'un `claude` qui ne part pas ; b : refusée, puis son refus en clair (7011).
    assert_eq!(lignes[0], format!("classée : main-{v}-1 (a)"), "{erreur}");
    let Some(b) = lignes.iter().position(|ligne| *ligne == format!("refusée : main-{v}-2 (b)")) else {
        panic!("{erreur}")
    };
    let de_a = lignes[1..b].join("\n");
    assert!(de_a.contains("claude-absent"), "l'erreur de a nomme la main qui ne part pas : {erreur}");
    assert_eq!(lignes.len(), b + 5, "b : sa ligne, son refus en trois lignes, puis la branche : {erreur}");
    assert!(lignes[b + 1].starts_with("refusée (7011) : "), "{erreur}");
    assert!(lignes[b + 2].contains("it depends on a,"), "la preuve nomme la dépendance perdue : {erreur}");

    // Ce que Kraken a posé : la branche de la vague, restée à la base, le bac de a, et rien pour b ; rien ne bouge dans
    // l'arbre du dépôt visé.
    assert_eq!(git(&depot, &["rev-parse", &format!("lydite/vague-{v}")]), base, "rien n'est assemblé");
    let bacs = racine.join("lydite").join("bacs");
    assert!(bacs.join(format!("main-{v}-1")).is_dir(), "a a son bac");
    assert!(!bacs.join(format!("main-{v}-2")).exists(), "b n'a pas de bac");
    assert!(bacs.join(format!("vague-{v}")).is_dir(), "le worktree de la vague reste en place");
    assert_eq!(git(&depot, &["status", "--porcelain"]), "");
    // Le fil a noté le refus de b.
    let fil = fs::read_to_string(racine.join("lydite").join("fil.jsonl")).unwrap();
    assert!(fil.contains(r#""type":"vague.refus""#) && fil.contains(r#""code":7011"#), "{fil}");
    // Le profil intégré déclare : chacune des deux tentatives de a est précédée de son lancement et suivie de son
    // constat, vide, car sa main ne part pas ; b, sautée, n'en a aucun.
    assert_eq!(fil.matches(r#""type":"main.lancement""#).count(), 2, "{fil}");
    assert_eq!(fil.matches(r#""type":"main.constat""#).count(), 2, "{fil}");
    // `lydite etat` finit la ligne de a par le mot du mur de sa dernière tentative, déclaré faute de constat ; celle de
    // b, qui n'a pas de tentative, n'en a pas.
    let dossier = racine.join("lydite");
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande.arg("etat").env("LYDITE_DOSSIER", dossier);
    let etat = avec_leurres(&mut commande, &racine).output().unwrap();
    let dit = String::from_utf8_lossy(&etat.stdout);
    assert_eq!(etat.status.code(), Some(1), "{dit}");
    let ligne = |tache: &str| dit.lines().find(|ligne| ligne.starts_with(&format!("main-{v}-{tache} "))).unwrap_or("");
    assert!(ligne("1 a").ends_with(" · wall required (declared)"), "{dit}");
    assert!(
        ligne("2 b").starts_with(&format!("main-{v}-2 b : refusée (7011) : ")) && !ligne("2 b").contains("wall"),
        "{dit}"
    );
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_git_dir_herite_ne_detourne_aucun_geste_de_la_vague() {
    let racine = racine("leurre");
    // Le leurre : un autre dépôt, que `GIT_DIR` nomme à Kraken sur l'enfant seul, comme le ferait un crochet ou un
    // alias `!` de git qui lance Kraken. Hérité, il mènerait chaque geste de la vague (`worktree add`, `reset --hard`,
    // `merge`) dans le leurre, quel que soit le dossier du geste.
    let leurre = depot(&racine.join("autre"));
    let depot = depot(&racine);
    let lignes = ["plan 1", "objectif : o", "preuve : exit 0", "", "tache a", "    Écrire a.txt"];
    let chemin = plan(&racine, "plan.txt", &lignes);
    let sortie = commande_du_plan(&racine, &chemin).env("GIT_DIR", leurre.join(".git")).output().unwrap();
    // La vague va au bout (sa tâche est classée : le `claude` n'existe pas), et sa branche est dans le dépôt visé.
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(1), "{erreur}");
    let Some(branche) = erreur.lines().last().and_then(|ligne| ligne.strip_prefix("branche de la vague : ")) else {
        panic!("{erreur}")
    };
    let branches = git(&depot, &["branch", "--list", "lydite/*"]);
    assert!(branches.contains(branche), "la branche de la vague est dans le dépôt visé : {branches:?}\n{erreur}");
    // Le leurre n'a rien vu : ni branche de Kraken, ni autre worktree que le sien.
    assert_eq!(git(&leurre, &["branch", "--list", "lydite/*"]), "", "ni branche");
    assert_eq!(git(&leurre, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
    let _ = fs::remove_dir_all(&racine);
}

/// Les sortants que le profil ferme par défaut, dans l'ordre du contrat (comme dans les essais du découpage).
const FERMES: [&str; 19] = [
    "CronCreate",
    "CronDelete",
    "CronList",
    "ScheduleWakeup",
    "RemoteTrigger",
    "SendMessage",
    "ListAgents",
    "PushNotification",
    "SendUserFile",
    "Artifact",
    "ArtifactComments",
    "ArtifactData",
    "ArtifactCheck",
    "DesignSync",
    "SendToProject",
    "SendToProjectContext",
    "ShareOnboardingGuide",
    "AskUserQuestion",
    "WebSearch",
];

// Les réponses de Claude Code des essais du découpage : un mur exigé et actif, sans réseau, au modèle `haiku`.
const GET_SETTINGS: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_settings","response":{"applied":"#,
    r#"{"model":"haiku"},"effective":{"disableAllHooks":true,"autoMemoryEnabled":false,"sandbox":{"enabled":true,"fa"#,
    r#"ilIfUnavailable":true,"allowUnsandboxedCommands":false,"filesystem":{"denyRead":["~/",@DOSSIER@]},"#,
    r#""network":{"allowedDomains":[],"strictAllowlist":true}"#,
    r#","credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},"#,
    r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}}},"#,
    r#""sources":[{"source":"userSettings","settings":{}},{"source":"flagSettings","settings":{}}]}}}"#,
);
const GET_HOOKS_LISTING: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_hooks_listing","response":{"poli"#,
    r#"cy":{"disabledByPolicy":false,"managedOnly":false,"pluginOnly":false,"allDisabled":true,"policyHookCount":0},"#,
    r#""hooks":[],"events":[],"eventCatalog":[]}}}"#,
);
const GET_SANDBOX_DIALOG: &str = concat!(
    r#"{"type":"control_response","response":{"subtype":"success","request_id":"get_sandbox_dialog","response":{"sup"#,
    r#"ported":true,"enabled":true,"enabled_in_settings":true,"mode":"regular"}}}"#,
);
const OUVERTURE: &str = concat!(
    r#"{"type":"system","subtype":"init","cwd":"bac","tools":["Task","Bash","Edit","Read","Skill","WebFetch","Workfl"#,
    r#"ow","Write"],"mcp_servers":[],"model":"haiku","permissionMode":"acceptEdits","claude_code_version":"9.9.9-fau"#,
    r#"sse"}"#,
);
const RESULTAT: &str =
    r#"{"type":"result","subtype":"success","is_error":false,"num_turns":1,"result":"fait\nPREUVE: vert"}"#;

/// Écrit ces lignes en script à ce chemin, dans les fins de ligne du système, puis le rend exécutable.
fn script(chemin: &Path, lignes: &[String]) {
    let fin = if cfg!(windows) { "\r\n" } else { "\n" };
    fs::write(chemin, lignes.iter().map(|ligne| format!("{ligne}{fin}")).collect::<String>()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// Le dossier en règle de permission de Claude Code : `//` puis le chemin absolu en forme POSIX.
fn regle_du_dossier(dossier: &Path) -> String {
    let chemin = dossier.to_str().expect("un dossier d'essai en UTF-8");
    if cfg!(windows) {
        format!("//{}{}", chemin[..1].to_ascii_lowercase(), chemin[2..].replace('\\', "/"))
    } else {
        format!("/{chemin}")
    }
}

/// Un texte en chaîne JSON : entre guillemets, l'antislash et le guillemet échappés.
fn json_chaine(texte: &str) -> String {
    format!("\"{}\"", texte.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Les quatre réponses de contrôle que la fausse claude rend : les règles que Kraken déclare à la main d'une tâche de
/// preuve `preuve`, sous le dossier de Kraken `dossier` et les leurres de la racine, sur le dépôt visé `depot`.
fn controle(racine: &Path, dossier: &Path, depot: &Path, preuve: &str) -> String {
    let mut allow: Vec<String> = ["Skill", "Workflow", "ExitPlanMode"].map(String::from).into();
    allow.extend([format!("Bash({preuve})"), format!("PowerShell({preuve})")]);
    let regle = regle_du_dossier(dossier);
    let mut deny: Vec<String> = FERMES.map(String::from).into();
    deny.extend([format!("Edit({regle}/fil.jsonl)"), format!("Edit({regle}/profils/**)")]);
    deny.extend(["Read", "Edit"].map(|outil| format!("{outil}({regle}/srt-*.json)")));
    for leurre in [racine.join("config"), racine.join("personnel").join(".claude")] {
        let regle = regle_du_dossier(&leurre);
        deny.extend(["Read", "Edit"].map(|outil| format!("{outil}({regle}/.credentials.json)")));
    }
    let commun = git(depot, &["rev-parse", "--path-format=absolute", "--git-common-dir"]);
    deny.push(format!("Edit({}/**)", regle_du_dossier(Path::new(&commun))));
    deny.extend(
        ["Bash(git commit:*)", "Bash(git commit *)", "PowerShell(git commit:*)", "PowerShell(git commit *)"]
            .map(String::from),
    );
    // Les leçons, protégées de toute main : la fausse claude met le bac de la main à la place de `@BAC@`.
    deny.push("Edit(@BAC@/.agents/skills/**)".into());
    let vues: Vec<String> = [("allow", &allow), ("deny", &deny)]
        .iter()
        .flat_map(|(comportement, regles)| {
            regles.iter().map(move |regle| {
                let regle = regle.replace('\\', "\\\\").replace('"', "\\\"");
                format!(r#"{{"behavior":"{comportement}","source":"flagSettings","rule":"{regle}"}}"#)
            })
        })
        .collect();
    let regles = format!(
        concat!(
            r#"{{"type":"control_response","response":{{"subtype":"success","request_id":"list_permission_rules","re"#,
            r#"sponse":{{"state":{{"rules":[{}]}}}}}}}}"#,
        ),
        vues.join(",")
    );
    let reglages = GET_SETTINGS.replace("@DOSSIER@", &json_chaine(&dossier.to_string_lossy()));
    [reglages.as_str(), &regles, GET_HOOKS_LISTING, GET_SANDBOX_DIALOG].join("\n") + "\n"
}

#[test]
fn une_main_qui_reecrit_le_git_de_son_bac_ne_detourne_aucun_geste_de_la_vague() {
    let racine = racine("detour");
    // L'autre dépôt, que la main nomme dans le `.git` de son bac : suivi, il recevrait le gel et la fusion.
    let autre = depot(&racine.join("autre"));
    let tete_de_l_autre = git(&autre, &["rev-parse", "HEAD"]);
    let depot = depot(&racine);
    let (dossier, main) = (racine.join("lydite"), racine.join("main"));
    fs::create_dir_all(dossier.join("profils")).unwrap();
    fs::create_dir_all(&main).unwrap();
    // La fausse claude : elle répond aux contrôles, lit sa consigne, réécrit le `.git` de son bac vers l'autre dépôt,
    // écrit a.txt, puis finit vert. Le faux srt saute `--settings <fichier>` et joue la preuve.
    let gitdir = format!("gitdir: {}", autre.join(".git").display());
    let (controle_jsonl, fin_jsonl) = (main.join("controle.jsonl"), main.join("fin.jsonl"));
    let (fausse, srt, lignes, faux_srt) = if cfg!(windows) {
        let lignes = vec![
            "@echo off".to_string(),
            // La règle des leçons nomme le bac de la main : `@BAC@` y est remplacé par sa règle.
            // PowerShell rend le dossier courant en nom long, la règle de Lydite le garde tel quel : `%CD%` aussi.
            "set \"bac_cd=%CD%\"".to_string(),
            format!(
                "powershell -NoProfile -Command \"$p=$env:bac_cd; $r='//'+$p.Substring(0,1).ToLower()\
                 +$p.Substring(2).Replace('\\','/'); \
                 [Console]::Out.Write((Get-Content -Raw '{}').Replace('@BAC@',$r))\"",
                controle_jsonl.display()
            ),
            r#"findstr "^" > nul"#.to_string(),
            // git pose le `.git` du bac caché sous Windows, et cmd ne réécrit pas un fichier caché.
            "attrib -h .git".to_string(),
            format!(">.git echo {gitdir}"),
            ">a.txt echo a".to_string(),
            format!(r#"type "{}""#, fin_jsonl.display()),
            "exit /b 0".to_string(),
        ];
        let faux_srt = vec!["@echo off", r#"if "%~5"=="true" exit /b 0"#, "%~5", "exit /b %errorlevel%"];
        (main.join("fausse-claude.cmd"), racine.join("faux-srt.cmd"), lignes, faux_srt)
    } else {
        let lignes = vec![
            "#!/bin/sh".to_string(),
            // La règle des leçons nomme le bac de la main : `@BAC@` y est remplacé par sa règle.
            format!(r#"sed "s|@BAC@|/$(pwd)|g" '{}'"#, controle_jsonl.display()),
            "cat > /dev/null".to_string(),
            format!("printf '%s\\n' '{gitdir}' > .git"),
            "echo a > a.txt".to_string(),
            format!("cat '{}'", fin_jsonl.display()),
        ];
        (
            main.join("fausse-claude.sh"),
            racine.join("faux-srt.sh"),
            lignes,
            vec!["#!/bin/sh", "shift 2", r#"exec "$@""#],
        )
    };
    script(&fausse, &lignes);
    script(&srt, &faux_srt.into_iter().map(String::from).collect::<Vec<_>>());
    let preuve = "exit 0";
    fs::write(&controle_jsonl, controle(&racine, &dossier, &depot, preuve)).unwrap();
    fs::write(&fin_jsonl, format!("{OUVERTURE}\n{RESULTAT}\n")).unwrap();
    // Le profil `essai`, qui nomme la fausse claude et exige le mur, puis son acceptation.
    let profil = format!(
        "sorte = claude\nprogramme = {}\nmodele = haiku\nrelance = haiku\nmur = exige\ndelai_main = 120\n\
         delai_preuve = 60\n",
        fausse.display()
    );
    fs::write(dossier.join("profils").join("essai.profil"), profil).unwrap();
    let kraken = || {
        let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
        commande
            .current_dir(&racine)
            .env("LYDITE_DOSSIER", &dossier)
            .env("LYDITE_CLAUDE", racine.join("claude-absent"))
            .env("LYDITE_SRT", &srt);
        avec_leurres(&mut commande, &racine);
        commande
    };
    let accepte = kraken().args(["accepter", "essai"]).output().unwrap();
    assert_eq!(accepte.status.code(), Some(0), "{}", String::from_utf8_lossy(&accepte.stderr));

    let lignes = ["plan 1", "objectif : o", &format!("preuve : {preuve}"), "", "tache a", "    Écrire a.txt"];
    let chemin = plan(&racine, "plan.txt", &lignes);
    let sortie = kraken().args(["lancer", "essai", "--plan"]).arg(&chemin).arg("--depot").arg(&depot).output().unwrap();
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(0), "la tâche est assemblée : {erreur}");
    let Some(branche) = erreur.lines().last().and_then(|ligne| ligne.strip_prefix("branche de la vague : ")) else {
        panic!("{erreur}")
    };
    // La main a bien réécrit le `.git` de son bac.
    let v = branche.strip_prefix("lydite/vague-").unwrap();
    let bac = dossier.join("bacs").join(format!("main-{v}-1"));
    assert_eq!(fs::read_to_string(bac.join(".git")).unwrap().trim(), gitdir);
    // Le gel et la fusion sont allés dans le dépôt visé : la branche de la vague y porte a.txt.
    assert_eq!(git(&depot, &["ls-tree", "-r", "--name-only", branche]), "a.txt", "{erreur}");
    // L'autre dépôt est intact : sa tête, un seul commit, ni branche de Kraken, ni index ni arbre touchés.
    assert_eq!(git(&autre, &["rev-parse", "HEAD"]), tete_de_l_autre);
    assert_eq!(git(&autre, &["rev-list", "--all", "--count"]), "1");
    assert_eq!(git(&autre, &["branch", "--list", "lydite/*"]), "");
    assert_eq!(git(&autre, &["status", "--porcelain"]), "");
    let _ = fs::remove_dir_all(&racine);
}
