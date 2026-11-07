//! Les tests des outils de processus : l'arbre et sa mise à mort, et l'environnement ôté à une main et à sa preuve.
use super::*;
use crate::essais::mains::{arbre, bac_jetable};
use crate::essais::{ARBRE, marque, meurt};
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
    let pid = marque(&bac, ARBRE);
    assert_eq!(tuer_arbre(enfant.id()), Ok(()));
    assert!(meurt(pid), "le petit-enfant {pid} survit à tuer_arbre");
    assert!(attendre(&mut enfant, Instant::now() + SORTIE).unwrap().is_some(), "l'enfant meurt aussi");
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
