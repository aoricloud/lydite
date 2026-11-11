//! Les tests des outils de processus : l'arbre et sa mise à mort, et l'environnement ôté à une main et à sa preuve.
use super::*;
use crate::essais::mains::{arbre, bac_jetable};
use crate::essais::{ATTENTE_DE_LA_MARQUE, marque, meurt};
use std::process::Stdio;

#[test]
fn l_arbre_se_lit_dans_les_stats() {
    // Le nom du programme peut porter blancs et parenthèses : le parent se lit après la dernière `)`.
    assert_eq!(parent("1234 (sh) S 1000 1234 1234 0 -1 4194560"), Some((1234, 1000)));
    assert_eq!(parent("77 (a b) (c)) R 5 77 77 0"), Some((77, 5)));
    assert_eq!(parent("9 (zombie) Z 1 9 9\n"), Some((9, 1)));
    for illisible in ["", "1234 sh S 1000", "x (sh) S 1000", "1234 (sh) S", "1234 (sh) S x"] {
        assert_eq!(parent(illisible), None, "{illisible:?}");
    }
    // 10 a deux enfants, 11 et 12, et un petit-enfant, 13 ; 20 et 21 sont d'un autre arbre.
    let liens = [(1, 0), (10, 1), (11, 10), (12, 10), (13, 11), (20, 1), (21, 20)];
    let mut sous_10 = descendants(10, &liens);
    sous_10.sort_unstable();
    assert_eq!(sous_10, [11, 12, 13]);
    assert_eq!(descendants(13, &liens), Vec::<u32>::new());
    // Un cycle, qu'un relevé pris pendant que des pid resservent pourrait montrer, ne fait pas boucler.
    assert_eq!(descendants(30, &[(30, 31), (31, 30)]), [31]);
}

#[cfg(target_os = "linux")]
#[test]
fn un_code_non_nul_de_kill_n_est_un_echec_que_si_un_pid_vit_encore() {
    let pids = ["10".to_string(), "11".to_string(), "12".to_string()];
    let loin = Instant::now() + SORTIE;
    let maintenant = Instant::now();
    // Code nul : jamais un échec, et rien ne se lit.
    let lectures = std::cell::Cell::new(0_u32);
    let compte = |_: &str| {
        lectures.set(lectures.get() + 1);
        true
    };
    assert!(!coup_en_echec(true, &pids, compte, maintenant));
    assert_eq!(lectures.get(), 0, "un code nul ne lit rien");
    // Code non nul, tous les pids sont morts : un descendant est sorti entre le relevé et le coup.
    assert!(!coup_en_echec(false, &pids, |_| false, maintenant));
    // Code non nul, un seul pid vit encore à l'échéance : un échec.
    assert!(coup_en_echec(false, &pids, |pid| pid == "11", maintenant));
    // Un pid qui vit toujours à l'échéance (nulle ici) est un échec.
    assert!(coup_en_echec(false, &pids, |_| true, maintenant));
    // La racine frappée se lit vivante deux fois, puis sort : la décision attend, et n'y voit pas d'échec.
    let relectures = std::cell::Cell::new(0_u32);
    let sort_apres_deux_lectures = |_: &str| {
        relectures.set(relectures.get() + 1);
        relectures.get() <= 2
    };
    assert!(!coup_en_echec(false, &pids[..1], sort_apres_deux_lectures, loin));
    assert_eq!(relectures.get(), 3, "relue jusqu'à ce que plus rien ne vive");
    // Le pid de la fonction `vivant` : le test lui-même vit, un pid hors de portée des pid ne vit pas.
    let moi = std::process::id().to_string();
    assert!(vivant(&moi));
    assert!(!vivant("4294967295"));
}

#[test]
fn tuer_arbre_tue_les_petits_enfants() {
    let (dir, bac) = bac_jetable("tuer");
    let argv = arbre(&dir);
    let mut enfant = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(&bac)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = marque(&bac, ATTENTE_DE_LA_MARQUE);
    assert_eq!(tuer_arbre(enfant.id()), Ok(()));
    assert!(meurt(pid), "le petit-enfant {pid} survit à tuer_arbre");
    assert!(attendre(&mut enfant, Instant::now() + SORTIE).unwrap().is_some(), "l'enfant meurt aussi");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn tuer_et_attendre_tue_un_enfant_qui_dort_et_ne_rend_aucun_echec() {
    let (dir, bac) = bac_jetable("tuer-attendre");
    let argv = arbre(&dir);
    let mut enfant = Command::new(&argv[0])
        .args(&argv[1..])
        .current_dir(&bac)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    marque(&bac, ATTENTE_DE_LA_MARQUE);
    assert_eq!(tuer_et_attendre(&mut enfant), Ok(()));
    assert!(enfant.try_wait().unwrap().is_some(), "l'enfant est sorti");
    let _ = fs::remove_dir_all(&dir);
}

// Un enfant déjà sorti donne `Ok(())`, sous Windows comme sous Linux : `taskkill` ou `kill` ne trouve plus la racine et
// rend un code non nul, mais plus aucun processus visé ne vit, et c'est ce qui compte.
#[test]
fn tuer_et_attendre_ne_rend_pas_d_echec_pour_un_enfant_deja_sorti() {
    let mut enfant =
        coquille("exit 0").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
    assert!(attendre(&mut enfant, Instant::now() + SORTIE).unwrap().is_some(), "l'enfant sort");
    // Son pid est rendu : on ne le tue plus.
    assert_eq!(tuer_et_attendre(&mut enfant), Ok(()));
}

// Joué sous Windows seul : un `cmd` qui lance sans cesse des `cmd /c exit` fait sortir des descendants pendant que
// `taskkill /T` arrête l'arbre, et `taskkill` rend alors un code non nul alors que la racine est morte.
#[cfg(windows)]
#[test]
fn tuer_et_attendre_ne_rend_pas_d_echec_quand_un_descendant_sort_pendant_la_mise_a_mort() {
    let (dir, bac) = bac_jetable("tuer-descendants-qui-sortent");
    let script = dir.join("boucle.cmd");
    fs::write(&script, "@echo off\r\n:boucle\r\ncmd /c exit\r\ngoto boucle\r\n").unwrap();
    for tour in 0..20 {
        let mut enfant = Command::new("cmd")
            .arg("/c")
            .arg(&script)
            .current_dir(&bac)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // Laisse la boucle lancer des descendants avant de tuer.
        thread::sleep(Duration::from_millis(200));
        assert_eq!(tuer_et_attendre(&mut enfant), Ok(()), "tour {tour}");
        assert!(enfant.try_wait().unwrap().is_some(), "tour {tour} : la racine est sortie");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_preuve_rejouee_perd_la_connexion_que_la_main_garde() {
    // La connexion de la main à Claude, son dossier de réglages et ses variables `ANTHROPIC_*`, n'est pas celle de sa
    // preuve : la main les reçoit, la preuve que Kraken rejoue ne les reçoit pas, quelle que soit la casse.
    let connexion = [
        "CLAUDE_CONFIG_DIR",
        "claude_config_dir",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_BASE_URL",
        "anthropic_model",
    ];
    for nom in connexion {
        assert!(!retire_de_l_env(nom), "{nom} passe à la main");
        assert!(retire_de_la_preuve(nom), "{nom} part de la preuve");
    }
    // Un nom qui ne commence pas par ceux-là n'est pas de cette connexion : la preuve le traite comme la main.
    for nom in ["XANTHROPIC_X", "MY_CLAUDE_CONFIG_DIR"] {
        assert!(!retire_de_l_env(nom) && !retire_de_la_preuve(nom), "{nom} passe à la main et à la preuve");
    }
}
