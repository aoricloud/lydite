//! La vague d'un plan, jouée sur le binaire lui-même : `lydite lancer <profil> --plan <fichier>`. Chaque test lance
//! `kraken` sur un dépôt git jetable qui a un commit, avec un dossier de Kraken jetable hors de ce dépôt et un `claude`
//! qui n'existe pas : un plan accepté par erreur ne lancerait aucune vraie main. Aucun nom de branche n'est supposé.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

// raccourci : create_dir refuse un dossier laissé là, sans vérifier son propriétaire ; monter (un nom au hasard) si
// deux tests du même pid prennent le même nom
/// Une racine neuve et vide, à ce test, dans le dossier temporaire du système : le dépôt visé (`depot`), le dossier de
/// Kraken (`kraken`, que rien ne crée avant un plan juste) et les plans y vivent côte à côte, hors de ce dépôt-ci.
fn racine(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-vague-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
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
    // Le profil intégré ne part qu'accepté : ces essais ne portent pas sur son accord, que l'humain donne d'abord.
    accepter_l_integre(racine, &racine.join("lydite"));
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

/// Le geste de l'humain : `lydite accept claude` sur le dossier de Kraken `dossier`, qui y écrit l'empreinte du profil
/// intégré. Il sort en 0, ou l'essai s'arrête.
fn accepter_l_integre(racine: &Path, dossier: &Path) {
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande.args(["accept", "claude"]).current_dir(racine).env("LYDITE_DOSSIER", dossier);
    let sortie = avec_leurres(&mut commande, racine).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", String::from_utf8_lossy(&sortie.stderr));
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
    // `profils` est ce que l'accord du profil intégré y a posé, avant le lancement.
    assert!(restes.iter().all(|reste| reste == "bacs" || reste == "profils"), "ni fil, ni bac : {restes:?}");
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
    // Le lancement refusé n'a ouvert aucune vague : le fil n'a que la fin du premier.
    assert_eq!(fil.matches(r#""type":"vague.lancement""#).count(), 1, "{fil}");
    assert_eq!(fil.matches(r#""type":"vague.fin""#).count(), 1, "{fil}");
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
    accepter_l_integre(&racine, &autre);
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

/// La valeur du champ texte `champ` de la ligne `ligne` du fil, ou `None` si le champ n'y est pas en texte.
fn champ_texte<'a>(ligne: &'a str, champ: &str) -> Option<&'a str> {
    let (_, apres) = ligne.split_once(&format!(r#""{champ}":""#))?;
    apres.split('"').next()
}

/// Les lignes du fil du dossier de Kraken de ce test, dont le `type` est `genre`.
fn lignes_du_fil(racine: &Path, genre: &str) -> Vec<String> {
    let fil = fs::read_to_string(racine.join("lydite").join("fil.jsonl")).unwrap();
    fil.lines().filter(|ligne| ligne.contains(&format!(r#""type":"{genre}""#))).map(String::from).collect()
}

#[test]
fn un_lancement_d_un_plan_mene_a_sa_fin_laisse_son_lancement_et_sa_fin_au_fil() {
    let racine = racine("lancement-fin");
    depot(&racine);
    // Le SHA-256 des octets du plan, tel que `sha256sum <fichier>` le donne : le plan, puis le même d'un octet de plus.
    const PLAN: &str = "6583aabda19b77577974f2f545649817a8a99343c678ce6c6bd4db83b3edf2e9";
    const PLAN_PLUS_UN_OCTET: &str = "efea8c8dd2d0e7e26aebec8201c10c9a49efb205ff99d813bcdedb71c5c01182";
    let rapide = plan_de_preuve(&racine, "rapide.txt", "git status");
    let entete = ["plan 1", "objectif : o", "preuve : git status", "", "tache a", "    Écrire a.txt", ""];
    let plus_long = plan(&racine, "plus-long.txt", &entete);
    assert_eq!(fs::metadata(&plus_long).unwrap().len(), fs::metadata(&rapide).unwrap().len() + 1);
    for (fichier, attendu) in [(&rapide, PLAN), (&rapide, PLAN), (&plus_long, PLAN_PLUS_UN_OCTET)] {
        let sortie = lancer_le_plan(&racine, fichier);
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(1), "la tâche, sans claude, est classée : {erreur}");
        let fil = fs::read_to_string(racine.join("lydite").join("fil.jsonl")).unwrap();
        let lignes: Vec<&str> = fil.lines().collect();
        // Le lancement de cette vague, le dernier du fil : la version de ce binaire et l'empreinte du fichier.
        let lancements = lignes_du_fil(&racine, "vague.lancement");
        let lancement = lancements.last().expect("un vague.lancement");
        let vague = champ_texte(lancement, "vague").expect("sa vague");
        let version = format!(r#""version":"{}""#, env!("CARGO_PKG_VERSION"));
        assert!(lancement.contains(&version), "{lancement}");
        assert_eq!(champ_texte(lancement, "plan"), Some(attendu), "{lancement}");
        // Le fil ne garde qu'un seul `vague.lancement` pour cette vague, et qu'un seul `vague.fin`, sa dernière ligne :
        // le code de sa sortie, après le sceau de fin.
        let marque = format!(r#""vague":"{vague}""#);
        let notes = |genre: &str| lignes.iter().filter(|l| l.contains(&marque) && l.contains(genre)).count();
        assert_eq!(notes(r#""type":"vague.lancement""#), 1, "{fil}");
        assert_eq!(notes(r#""type":"vague.fin""#), 1, "{fil}");
        let derniere = lignes.last().unwrap();
        assert!(derniere.contains(&format!(r#""type":"vague.fin","vague":"{vague}","code":1}}"#)), "{derniere}");
        assert!(lignes[lignes.len() - 2].contains(r#""type":"vague.sceau""#), "{fil}");
    }
    // Trois lancements, trois fins ; le même fichier a donné la même empreinte, et l'octet de plus une autre.
    let plans: Vec<String> = lignes_du_fil(&racine, "vague.lancement")
        .iter()
        .map(|ligne| champ_texte(ligne, "plan").unwrap().to_string())
        .collect();
    assert_eq!(plans, [PLAN, PLAN, PLAN_PLUS_UN_OCTET]);
    assert_eq!(lignes_du_fil(&racine, "vague.fin").len(), 3);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn un_lancement_refuse_avant_sa_vague_et_lydite_etat_n_ajoutent_aucune_ligne_au_fil() {
    let racine = racine("lancement-refuse");
    depot(&racine);
    let rapide = plan_de_preuve(&racine, "rapide.txt", "git status");
    let sortie = lancer_le_plan(&racine, &rapide);
    assert_eq!(sortie.status.code(), Some(1), "{}", String::from_utf8_lossy(&sortie.stderr));
    let fil = racine.join("lydite").join("fil.jsonl");
    let avant = fs::read(&fil).unwrap();
    assert!(String::from_utf8_lossy(&avant).contains(r#""type":"vague.fin""#));
    // Un plan faux est refusé avant sa vague (code 2) : rien n'est ouvert, rien ne se note.
    let faux = plan(&racine, "faux.txt", &["plan 2", "objectif : o", "preuve : git status", "tache a", "    x"]);
    lire_refus(&lancer_le_plan(&racine, &faux));
    assert_eq!(fs::read(&fil).unwrap(), avant, "un plan refusé n'a rien écrit au fil");
    // `lydite etat` lit le dossier, sans y écrire.
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande.arg("etat").env("LYDITE_DOSSIER", racine.join("lydite"));
    let etat = avec_leurres(&mut commande, &racine).output().unwrap();
    assert_eq!(etat.status.code(), Some(1), "{}", String::from_utf8_lossy(&etat.stdout));
    assert_eq!(fs::read(&fil).unwrap(), avant, "lydite etat n'a rien écrit au fil");
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
    r#"ilIfUnavailable":true,"allowUnsandboxedCommands":false,"filesystem":{"denyRead":["~/",@DOSSIER@,@CONFIG@]},"#,
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
    let reglages = GET_SETTINGS
        .replace("@DOSSIER@", &json_chaine(&dossier.to_string_lossy()))
        .replace("@CONFIG@", &json_chaine(&racine.join("config").to_string_lossy()));
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

// Les essais du mur natif (`mur = natif`) : un faux bwrap, un faux `lydite-reseau` et un faux srt qui note ses appels,
// que le profil ne doit jamais lancer. Unix seulement : le mur natif est bubblewrap. Leurs chemins sont courts : le
// socket du filtre réseau vit sous le dossier de Kraken, et un chemin de socket Unix tient 107 octets au plus.

/// Une racine neuve au nom court, sous le dossier temporaire du système : `n<pid><suffixe>`.
#[cfg(unix)]
fn racine_courte(suffixe: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("n{}{suffixe}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir(&dir).unwrap();
    dir
}

/// Écrit un faux programme dans `dir` et le lance une fois (`--version`), jusqu'à ce que son `exec` parte : un autre
/// test qui lance un processus au même instant tient un moment le descripteur d'écriture du script, et son `exec` rend
/// alors « Text file busy ». Le délai est un garde-fou, non une attente. Chaque faux sort en 0 sur `--version`.
#[cfg(unix)]
fn faux_programme(dir: &Path, nom: &str, lignes: &[String]) -> PathBuf {
    let chemin = dir.join(nom);
    script(&chemin, lignes);
    let debut = Instant::now();
    loop {
        let lancement = Command::new(&chemin).arg("--version").stdin(Stdio::null()).output();
        match lancement {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy && debut.elapsed() < Duration::from_secs(30) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            autre => break assert!(autre.is_ok(), "{} ne part pas", chemin.display()),
        }
    }
    chemin
}

/// Les trois faux du mur natif, sous `<racine>/faux`, et leurs notes (`bwrap.log`, `reseau.log`, `srt.log`, une ligne
/// par appel, ses arguments). Le faux bwrap joue ce qui suit son premier `--`, sans mur ; s'il reçoit `echec`, il dit
/// ce texte sur ses erreurs et rend 1 à tout appel autre que `--version`. Le faux `lydite-reseau` dit `ready` en
/// `filtre` puis lit son entrée jusqu'à sa fermeture, et joue ce qui suit son premier `--` en `relais`. Le faux srt
/// saute `--settings <fichier>` et joue la commande. Aucun ne note `--version`, avec lequel le test et Lydite les
/// sondent.
#[cfg(unix)]
struct FauxNatif {
    bwrap: PathBuf,
    reseau: PathBuf,
    srt: PathBuf,
    notes: PathBuf,
}

#[cfg(unix)]
impl FauxNatif {
    fn nouveau(racine: &Path, echec: Option<&str>) -> FauxNatif {
        let dir = racine.join("faux");
        fs::create_dir_all(&dir).unwrap();
        let notes = dir.join("notes");
        fs::create_dir_all(&notes).unwrap();
        // Le début de chaque faux : il sort en 0 sur `--version`, puis note son appel.
        let debut = |nom: &str| {
            [
                "#!/bin/sh".to_string(),
                r#"[ "$1" = --version ] && exit 0"#.to_string(),
                format!(r#"echo "$*" >> '{}'"#, notes.join(nom).display()),
            ]
        };
        // Jouer ce qui suit le premier `--`.
        let suite = [
            r#"while [ $# -gt 0 ] && [ "$1" != "--" ]; do shift; done"#.to_string(),
            "[ $# -gt 0 ] && shift".to_string(),
            r#"exec "$@""#.to_string(),
        ];
        let mut bwrap = debut("bwrap.log").to_vec();
        if let Some(dit) = echec {
            bwrap.push(format!("echo '{dit}' >&2; exit 1"));
        }
        bwrap.extend(suite.clone());
        let mut reseau = debut("reseau.log").to_vec();
        reseau.extend([
            r#"case "$1" in"#.to_string(),
            "  filtre) echo ready; cat > /dev/null ;;".to_string(),
            format!("  relais) {}; {}; {} ;;", suite[0], suite[1], suite[2]),
            "esac".to_string(),
        ]);
        let mut srt = debut("srt.log").to_vec();
        srt.extend(["shift 2".to_string(), r#"exec "$@""#.to_string()]);
        FauxNatif {
            bwrap: faux_programme(&dir, "bwrap", &bwrap),
            reseau: faux_programme(&dir, "lydite-reseau", &reseau),
            srt: faux_programme(&dir, "srt", &srt),
            notes,
        }
    }

    /// Les lignes que ce faux a notées, vides s'il n'a jamais été appelé.
    fn appels(&self, nom: &str) -> Vec<String> {
        fs::read_to_string(self.notes.join(nom)).unwrap_or_default().lines().map(String::from).collect()
    }
}

/// Le profil `essai` du mur natif, qui permet `exemple.test` et nomme `programme`, écrit dans `dossier`, puis la
/// commande `lydite <args>` qui le prend avec ces faux, dans la racine : le dossier de Kraken est `dossier`,
/// `LYDITE_CLAUDE` vise un programme absent, et `LYDITE_BWRAP`, `LYDITE_RESEAU` et `LYDITE_SRT` visent les faux. Le
/// profil n'est pas accepté.
#[cfg(unix)]
fn kraken_natif(
    racine: &Path,
    dossier: &Path,
    faux: &FauxNatif,
    programme: &Path,
) -> impl Fn(&[&str]) -> Command + use<> {
    fs::create_dir_all(dossier.join("profils")).unwrap();
    let profil = format!(
        "sorte = claude\nprogramme = {}\nmodele = haiku\nrelance = haiku\nmur = natif\nreseau = exemple.test\n\
         delai_main = 120\ndelai_preuve = 60\n",
        programme.display()
    );
    fs::write(dossier.join("profils").join("essai.profil"), profil).unwrap();
    let (racine, dossier) = (racine.to_path_buf(), dossier.to_path_buf());
    let (bwrap, reseau, srt) = (faux.bwrap.clone(), faux.reseau.clone(), faux.srt.clone());
    move |args: &[&str]| {
        let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
        commande
            .args(args)
            .current_dir(&racine)
            .env("LYDITE_DOSSIER", &dossier)
            .env("LYDITE_CLAUDE", racine.join("claude-absent"))
            .env("LYDITE_BWRAP", &bwrap)
            .env("LYDITE_RESEAU", &reseau)
            .env("LYDITE_SRT", &srt);
        avec_leurres(&mut commande, &racine);
        commande
    }
}

/// Tout ce que `dossier` tient, chemins relatifs, dossiers comme fichiers, à tous les niveaux.
#[cfg(unix)]
fn arbre(dossier: &Path) -> std::collections::BTreeSet<String> {
    fn parcourir(racine: &Path, dir: &Path, vus: &mut std::collections::BTreeSet<String>) {
        for entree in fs::read_dir(dir).unwrap() {
            let chemin = entree.unwrap().path();
            vus.insert(chemin.strip_prefix(racine).unwrap().display().to_string());
            if chemin.is_dir() {
                parcourir(racine, &chemin, vus);
            }
        }
    }
    let mut vus = std::collections::BTreeSet::new();
    parcourir(dossier, dossier, &mut vus);
    vus
}

#[cfg(unix)]
#[test]
fn une_vague_sous_un_mur_natif_rejoue_la_preuve_par_le_mur_natif_et_ne_lance_jamais_srt() {
    let racine = racine_courte("a");
    let depot = depot(&racine);
    let (dossier, main) = (racine.join("l"), racine.join("main"));
    fs::create_dir_all(&main).unwrap();
    let faux = FauxNatif::nouveau(&racine, None);
    // La fausse claude : elle répond aux contrôles, lit sa consigne, écrit a.txt, puis finit vert.
    let (controle_jsonl, fin_jsonl) = (main.join("controle.jsonl"), main.join("fin.jsonl"));
    let lignes = [
        "#!/bin/sh".to_string(),
        // La règle des leçons nomme le bac de la main : `@BAC@` y est remplacé par sa règle.
        format!(r#"sed "s|@BAC@|/$(pwd)|g" '{}'"#, controle_jsonl.display()),
        "cat > /dev/null".to_string(),
        "echo a > a.txt".to_string(),
        format!("cat '{}'", fin_jsonl.display()),
    ];
    let fausse = main.join("fausse-claude.sh");
    script(&fausse, &lignes);
    let preuve = "exit 0";
    // Le profil permet `exemple.test` : la main le déclare, et le filtre du mur natif le laisse passer.
    let web = r#"{"behavior":"allow","source":"flagSettings","rule":"WebFetch(domain:exemple.test)"}"#;
    let reponses = controle(&racine, &dossier, &depot, preuve)
        .replace(r#""allowedDomains":[]"#, r#""allowedDomains":["exemple.test"]"#)
        .replace(r#""rule":"ExitPlanMode"}"#, &format!(r#""rule":"ExitPlanMode"}},{web}"#));
    fs::write(&controle_jsonl, reponses).unwrap();
    fs::write(&fin_jsonl, format!("{OUVERTURE}\n{RESULTAT}\n")).unwrap();
    let kraken = kraken_natif(&racine, &dossier, &faux, &fausse);
    let accepte = kraken(&["accepter", "essai"]).output().unwrap();
    assert_eq!(accepte.status.code(), Some(0), "{}", String::from_utf8_lossy(&accepte.stderr));

    let chemin =
        plan(&racine, "plan.txt", &["plan 1", "objectif : o", "preuve : exit 0", "", "tache a", "    Écrire a.txt"]);
    let sortie = kraken(&["lancer", "essai", "--plan"]).arg(&chemin).arg("--depot").arg(&depot).output().unwrap();
    let erreur = String::from_utf8_lossy(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(0), "la tâche est assemblée : {erreur}");
    let Some(branche) = erreur.lines().last().and_then(|ligne| ligne.strip_prefix("branche de la vague : ")) else {
        panic!("{erreur}")
    };
    assert_eq!(git(&depot, &["ls-tree", "-r", "--name-only", branche]), "a.txt", "{erreur}");

    // srt n'a jamais été appelé, ni pour l'épreuve du mur ni pour une preuve : son faux note tout appel, et il note
    // bien le sien quand on l'appelle.
    assert_eq!(faux.appels("srt.log"), Vec::<String>::new(), "srt n'est lancé ni pour un mur ni pour une preuve");
    let essai = Command::new(&faux.srt).args(["--settings", "x", "true"]).status().unwrap();
    assert!(essai.success());
    assert_eq!(faux.appels("srt.log"), ["--settings x true"], "le faux srt note ses appels");
    // L'épreuve du mur, `true`, puis la preuve rejouée trois fois, `exit 0` (sur la base de la vague, dans le bac de la
    // tâche, sur l'assemblage), sont jouées dans le mur natif : chaque rejeu est un `bwrap` extérieur et le second, qui
    // pose le verrou (`--seccomp 9`) et joue la commande.
    let bwrap = faux.appels("bwrap.log");
    let jouees = |commande: &str| {
        let fin = format!("--seccomp 9 -- sh -c {commande}");
        bwrap.iter().filter(|ligne| ligne.ends_with(&fin)).count()
    };
    assert_eq!(jouees("true"), 1, "l'épreuve du mur : {bwrap:?}");
    assert_eq!(jouees("exit 0"), 3, "les trois rejeux de la preuve : {bwrap:?}");
    // Les `bwrap` extérieurs disent où chaque rejeu monte son bac : l'épreuve, deux fois la vague, une fois la tâche.
    let exterieurs: Vec<&String> = bwrap.iter().filter(|ligne| ligne.starts_with("--new-session")).collect();
    // Le motif part du dossier de Kraken de l'essai : le fichier de Lydite, que le mur lit, peut vivre sous un dossier
    // `bacs/main-…` d'ailleurs (le worktree d'une main), qu'un simple `/bacs/main-` compterait.
    let bacs = format!("{}/bacs/", dossier.display());
    let dans = |bac: &str| exterieurs.iter().filter(|ligne| ligne.contains(&format!("{bacs}{bac}"))).count();
    assert_eq!((exterieurs.len(), dans("mur-"), dans("vague-"), dans("main-")), (4, 1, 2, 1), "{exterieurs:?}");
    // Chaque rejeu sort par le filtre du réseau : un `filtre` et un `relais` pour chacun, quatre rejeux en tout.
    let reseau = faux.appels("reseau.log");
    let fois = |mot: &str| reseau.iter().filter(|ligne| ligne.starts_with(mot)).count();
    assert_eq!((fois("filtre"), fois("relais")), (4, 4), "{reseau:?}");
    // Rien du filtre ne reste dans le dossier de Kraken : ni dossier de socket, ni fichier du filtre d'appels système.
    let restes: Vec<String> =
        arbre(&dossier).into_iter().filter(|chemin| chemin.ends_with(".net") || chemin.ends_with(".bpf")).collect();
    assert!(restes.is_empty(), "{restes:?}");
    assert!(arbre(&dossier).iter().any(|chemin| chemin.starts_with("bacs/main-")), "le bac de la tâche est là");
    let _ = fs::remove_dir_all(&racine);
}

#[cfg(unix)]
#[test]
fn un_mur_natif_qui_ne_tient_pas_refuse_en_6028_avant_toute_main_et_ne_laisse_rien_dans_le_dossier_de_lydite() {
    let racine = racine_courte("b");
    let depot = depot(&racine);
    let dossier = racine.join("l");
    // Le `bwrap` part, et dit ce qu'il dit quand le système ferme les espaces d'utilisateur : il ne joue rien.
    let dit = "bwrap: No permissions to create new namespace, likely because the kernel does not allow non-privileged \
               user namespaces";
    let faux = FauxNatif::nouveau(&racine, Some(dit));
    let kraken = kraken_natif(&racine, &dossier, &faux, &racine.join("claude-absent"));
    let accepte = kraken(&["accepter", "essai"]).output().unwrap();
    assert_eq!(accepte.status.code(), Some(0), "{}", String::from_utf8_lossy(&accepte.stderr));
    // Ce que le dossier de Kraken tient une fois le profil accepté, avant tout lancement.
    let avant = arbre(&dossier);
    assert!(avant.iter().any(|chemin| chemin.starts_with("profils/")), "le même regard voit le profil : {avant:?}");
    let chemin =
        plan(&racine, "plan.txt", &["plan 1", "objectif : o", "preuve : exit 0", "", "tache a", "    Écrire a.txt"]);
    let sortie = kraken(&["lancer", "essai", "--plan"]).arg(&chemin).arg("--depot").arg(&depot).output().unwrap();
    let refus = lire_refus(&sortie);
    assert_eq!(refus.code, "lydite : 6028 lancer.mur_refuse");
    assert!(refus.preuve.starts_with("the native wall (`mur = natif`) needs the user namespaces"), "{}", refus.preuve);
    assert!(refus.preuve.contains(dit), "la preuve dit ce que bwrap a dit : {}", refus.preuve);
    let correction = &refus.correction;
    assert!(correction.contains("`mur = exige`") && correction.contains("through srt"), "{correction}");
    // Le mur a été essayé (le `true` de l'épreuve est parti sous le faux, qui n'a joué que le `bwrap` extérieur et son
    // mur), le filtre du réseau a été lancé puis arrêté, et srt n'a pas pris la place du mur.
    let bwrap = faux.appels("bwrap.log");
    assert_eq!(bwrap.len(), 1, "{bwrap:?}");
    assert!(bwrap[0].starts_with("--new-session") && bwrap[0].ends_with(" true"), "{bwrap:?}");
    assert_eq!(faux.appels("reseau.log").iter().filter(|ligne| ligne.starts_with("filtre")).count(), 1);
    assert_eq!(faux.appels("srt.log"), Vec::<String>::new(), "srt n'est pas lancé à la place");
    // Rien n'est resté dans le dossier de Kraken que le dossier `bacs/` vide de l'épreuve, ni dans le dépôt visé.
    let apres = arbre(&dossier);
    let nouveaux: Vec<&String> = apres.difference(&avant).collect();
    assert_eq!(nouveaux, ["bacs"], "ni fil, ni bac, ni journal, ni filtre : {apres:?}");
    assert_eq!(git(&depot, &["branch", "--list", "lydite/*"]), "", "ni branche");
    assert_eq!(git(&depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1, "ni worktree");
    assert_eq!(git(&depot, &["status", "--porcelain"]), "");
    let _ = fs::remove_dir_all(&racine);
}

/// Joue `lydite etat` sur le dossier de Kraken de la racine : son code et sa sortie.
#[cfg(unix)]
fn etat_de_la_racine(racine: &Path) -> (Option<i32>, String) {
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande.arg("etat").current_dir(racine).env("LYDITE_DOSSIER", racine.join("lydite"));
    let sortie = avec_leurres(&mut commande, racine).output().unwrap();
    (sortie.status.code(), String::from_utf8_lossy(&sortie.stdout).into_owned())
}

#[cfg(unix)]
#[test]
fn un_lancement_tue_pendant_que_sa_main_court_est_dit_interrompu_par_etat_et_pas_avant() {
    let racine = racine("interrompu");
    let depot = depot(&racine);
    let (dossier, main) = (racine.join("lydite"), racine.join("main"));
    fs::create_dir_all(dossier.join("profils")).unwrap();
    fs::create_dir_all(&main).unwrap();
    let (controle_jsonl, marques) = (main.join("controle.jsonl"), racine.join("marques"));
    fs::create_dir_all(&marques).unwrap();
    let (demarre, libre, sortie) = (marques.join("demarre"), marques.join("libre"), marques.join("sortie"));
    // La fausse main : elle répond aux contrôles, pose `demarre`, puis attend que le test pose `libre` sans lire son
    // entrée, pose `sortie` et sort sans rien rendre. Son plafond (une minute) n'est qu'un garde-fou contre une main
    // orpheline.
    let lignes = [
        "#!/bin/sh".to_string(),
        r#"[ "$1" = --version ] && exit 0"#.to_string(),
        format!(r#"sed "s|@BAC@|/$(pwd)|g" '{}'"#, controle_jsonl.display()),
        format!("touch '{}'", demarre.display()),
        "n=0".to_string(),
        format!("while [ ! -f '{}' ]; do", libre.display()),
        "  n=$((n + 1))".to_string(),
        "  [ \"$n\" -ge 600 ] && exit 1".to_string(),
        "  sleep 0.1".to_string(),
        "done".to_string(),
        format!("touch '{}'", sortie.display()),
    ];
    let fausse = faux_programme(&main, "fausse-claude.sh", &lignes);
    let srt = faux_srt(&racine);
    let preuve = "exit 0";
    fs::write(&controle_jsonl, controle(&racine, &dossier, &depot, preuve)).unwrap();
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
    let lignes_du_plan = ["plan 1", "objectif : o", "preuve : exit 0", "", "tache a", "    Écrire a.txt"];
    let chemin = plan(&racine, "plan.txt", &lignes_du_plan);
    let mut lancement = kraken()
        .args(["lancer", "essai", "--plan"])
        .arg(&chemin)
        .arg("--depot")
        .arg(&depot)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // La main court : la marque que sa fausse pose est la seule attente, son plafond n'est qu'un garde-fou.
    let debut = Instant::now();
    while !demarre.exists() {
        if let Some(statut) = lancement.try_wait().unwrap() {
            panic!("le lancement est sorti ({statut}) avant que sa main ne démarre");
        }
        assert!(debut.elapsed() < Duration::from_secs(60), "la main n'a jamais démarré");
        thread::sleep(Duration::from_millis(20));
    }
    // Le lancement tient le dossier : sa vague court, rien n'est interrompu.
    let (code, dit) = etat_de_la_racine(&racine);
    assert_eq!(code, Some(1), "{dit}");
    assert!(dit.contains("running, attempt 1") && !dit.contains("Interrupted:"), "{dit}");
    // Le lancement est tué, sa main survit. Un enfant du lancement peut tenir le verrou un instant après sa mort :
    // `etat` se rejoue jusqu'au 3, sans attente fixe.
    lancement.kill().unwrap();
    lancement.wait().unwrap();
    let debut = Instant::now();
    let (code, dit) = loop {
        let (code, dit) = etat_de_la_racine(&racine);
        if code == Some(3) || debut.elapsed() > Duration::from_secs(30) {
            break (code, dit);
        }
    };
    assert_eq!(code, Some(3), "{dit}");
    assert!(dit.contains("interrupted, attempt 1") && !dit.contains("running"), "{dit}");
    assert_eq!(dit.lines().last().map(|ligne| ligne.starts_with("Interrupted:")), Some(true), "{dit}");
    // La fausse main survit à son lancement : elle finit quand le test la libère.
    assert!(!libre.exists());
    fs::write(&libre, "").unwrap();
    // On efface la racine une fois la main sortie : sa fausse pose `sortie` juste avant de sortir.
    let debut = Instant::now();
    while !sortie.exists() && debut.elapsed() < Duration::from_secs(10) {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(sortie.exists(), "la fausse main n'a pas posé `sortie` dans les dix secondes après `libre`");
    let _ = fs::remove_dir_all(&racine);
}
