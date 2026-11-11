use super::*;
use crate::branchements::dossier::Fil;
use crate::coeur::fil::{Evenement, Lecture, etat, lot};
use crate::commande::etat::rendu;
use crate::essais::{MAINTENANT, V1, ouverture};
use std::env;
use std::fs;

fn args(s: &[&str]) -> Vec<OsString> {
    s.iter().map(OsString::from).collect()
}

/// Le code du refus que la ligne de commande reçoit : elle doit être refusée.
fn code(s: &[&str]) -> Code {
    options(&args(s)).unwrap_err().code
}

/// Un argument qui n'est pas de l'UTF-8 : un substitut seul sous Windows, l'octet 0xFF ailleurs.
fn non_utf8() -> OsString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xD800])
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xFF])
    }
}

/// Une ligne de commande en mots séparés par des blancs, où `?` est un argument qui n'est pas de l'UTF-8.
fn ligne(mots: &str) -> Vec<OsString> {
    mots.split_whitespace().map(|mot| if mot == "?" { non_utf8() } else { mot.into() }).collect()
}

#[test]
fn options_lues() {
    let o = options(&args(&["lancer", "claude", "obj", "--n", "10", "--verifier", "cargo test", "--depot", "d"]));
    assert_eq!(
        o,
        Ok(Commande::Lancer(Options {
            profil: "claude".into(),
            objectif: "obj".into(),
            n: 10,
            verifier: Some("cargo test".into()),
            depot: Some("d".into()),
            decouper: false,
            observer: false,
            pr: None,
            issue: None,
        }))
    );
    assert!(matches!(options(&args(&["lancer", "claude", "obj"])), Ok(Commande::Lancer(Options { n: 1, .. }))));
    // Ce qui valait None est le refus 1006 : un entier nul, un drapeau sans valeur, un drapeau inconnu ou redit.
    let mal_formee = Code::CommandeMalFormee;
    assert_eq!(code(&["lancer", "claude", "obj", "--n", "0"]), mal_formee);
    assert_eq!(code(&["lancer", "claude", "obj", "--n"]), mal_formee);
    assert_eq!(code(&["lancer", "claude", "obj", "--x", "1"]), mal_formee);
    assert_eq!(code(&["lancer", "claude", "obj", "--n", "1", "--n", "2"]), mal_formee);
    // Un drapeau à la place de l'objectif, ou du profil, est le refus 1006, qui dit ce qui manque : l'objectif
    // « --verifier » n'est jamais lancé.
    let manque = |mots: &[&str]| options(&args(mots)).unwrap_err();
    for (mots, dit) in [
        (&["lancer", "claude", "--verifier", "x"][..], "`launch`: the objective is missing"),
        (&["lancer", "claude", "--n", "3"], "`launch`: the objective is missing"),
        (&["lancer", "claude", "--depot", "d", "--n", "3"], "`launch`: the objective is missing"),
        (&["lancer", "--n", "3", "obj"], "`launch`: the profile and the objective are missing"),
        (&["lancer", "--verifier", "x", "--n", "3"], "`launch`: the profile and the objective are missing"),
        (&["lancer", "--depot", "--n"], "`launch`: the profile and the objective are missing"),
        (&["lancer", "--n", "--n"], "`launch`: the profile and the objective are missing"),
        (&["launch", "claude", "--verify", "x"], "`launch`: the objective is missing"),
        (&["launch", "--repo", "--n"], "`launch`: the profile and the objective are missing"),
    ] {
        let refus = manque(mots);
        assert_eq!((refus.code, refus.preuve.as_str()), (mal_formee, dit), "{mots:?}");
        assert!(refus.correction.contains(USAGE), "{refus}");
    }
    // Seuls les drapeaux de `lancer` sont refusés à cette place : « --help » reste un objectif.
    assert!(matches!(options(&args(&["lancer", "claude", "--help"])), Ok(Commande::Lancer(_))));
    // `--verifier` vide ou blanc est le refus 6014 : rejouée, une commande blanche réussirait toujours et ferait
    // prendre toute main pour prouvée. La preuve montre ce qui a été vu.
    for (blanc, vu) in [("", r#""""#), (" ", r#"" ""#), ("\t \r\n", r#""\t \r\n""#)] {
        let refus = options(&args(&["lancer", "claude", "obj", "--verifier", blanc])).unwrap_err();
        assert_eq!(refus.code, Code::PreuveVide, "{blanc:?}");
        assert_eq!(refus.preuve, format!("--verify is {vu}, a blank command"));
        assert!(refus.correction.contains("--verify"), "{refus}");
        assert!(!refus.correction.contains("--verifier"), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Le premier refus l'emporte : le drapeau inconnu qui suit un --verifier blanc n'est pas lu.
    let refus = options(&args(&["lancer", "claude", "obj", "--n", "2", "--verifier", " ", "--x"])).unwrap_err();
    assert_eq!(refus.code, Code::PreuveVide);
    // Une commande qui n'est pas blanche passe, blancs autour compris : Kraken la rejoue telle qu'elle est.
    let lue = options(&args(&["lancer", "claude", "obj", "--verifier", " exit 0 "]));
    assert!(matches!(lue, Ok(Commande::Lancer(Options { verifier: Some(v), .. })) if v == " exit 0 "));
}

#[test]
fn options_accepter() {
    // Un nom, rien d'autre : le nom n'est lu qu'à l'acceptation (6007 s'il est faux), pas à la ligne de commande.
    assert_eq!(options(&args(&["accepter", "veille"])), Ok(Commande::Accepter("veille".into())));
    assert_eq!(options(&args(&["accepter", "../x"])), Ok(Commande::Accepter("../x".into())));
    let mal_formee = Code::CommandeMalFormee;
    for (mots, dit) in [
        (&["accepter"][..], "`accept`: the profile is missing"),
        (&["accepter", "a", "b"], "`accept` takes only the profile name, and « b » follows it"),
        (&["accepter", "a", "--n", "1"], "and « --n » follows it"),
    ] {
        let refus = options(&args(mots)).unwrap_err();
        assert_eq!(refus.code, mal_formee, "{mots:?}");
        assert!(refus.preuve.contains(dit), "{mots:?} : {refus}");
        assert!(refus.correction.contains(USAGE), "{refus}");
    }
    // Un nom qui n'est pas de l'UTF-8 est une ligne mal formée, sans panique.
    let refus = options(&ligne("accepter ?")).unwrap_err();
    assert_eq!(refus.code, mal_formee);
    assert!(refus.preuve.contains("the profile must be UTF-8 text"), "{refus}");
    // La commande est dans l'usage, que la correction du refus 1005 donne.
    assert!(USAGE.contains("lydite accept <profile>"), "{USAGE}");
    let inconnue = options(&args(&["Accepter", "veille"])).unwrap_err();
    assert_eq!(inconnue.code, Code::CommandeInconnue);
    assert!(inconnue.correction.contains("lydite accept <profile>"), "{inconnue}");
}

#[test]
fn options_profil() {
    assert_eq!(options(&args(&["profil", "veille"])), Ok(Commande::Profil("veille".into())));
    for (mots, dit) in [
        (&["profil"][..], "`profile`: the profile is missing"),
        (&["profil", "a", "b"], "`profile` takes only the profile name, and « b » follows it"),
    ] {
        let refus = options(&args(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots:?}");
        assert!(refus.preuve.contains(dit), "{mots:?} : {refus}");
        assert!(refus.correction.contains(USAGE), "{refus}");
    }
    assert!(USAGE.contains("lydite profile <profile>"), "{USAGE}");
}

#[test]
fn options_plan_seul() {
    // `plan <fichier>`, ou `plan --decoupe <fichier>` : le chemin garde son `OsString`.
    let lu = |fichier: &str, decoupe| Ok(Commande::LirePlan { fichier: fichier.into(), decoupe });
    assert_eq!(options(&args(&["plan", "p.txt"])), lu("p.txt", false));
    assert_eq!(options(&args(&["plan", "--decoupe", "PLAN.lydite"])), lu("PLAN.lydite", true));
    let mauvais = non_utf8();
    let complete = [args(&["plan", "--decoupe"]), vec![mauvais.clone()]].concat();
    assert_eq!(options(&complete), Ok(Commande::LirePlan { fichier: PathBuf::from(mauvais), decoupe: true }));
    // Chaque faute : 1006, dont la preuve dit ce qui a été vu, et la correction donne l'usage. Un drapeau n'est pas
    // un fichier, et rien ne suit le fichier, pas même `--decoupe`.
    for (mots, vu) in [
        ("plan", "`plan`: the plan file is missing"),
        ("plan --decoupe", "`plan`: the plan file is missing"),
        ("plan --n 1", "« --n » is a flag"),
        ("plan --decoupe --decoupe p", "« --decoupe » is a flag"),
        ("plan --plan p", "« --plan » is a flag"),
        ("plan p q", "`plan` takes only `[--split] <file>`, and « q » follows it"),
        ("plan p --decoupe", "and « --decoupe » follows it"),
        ("plan --decoupe p ?", "follows it"),
    ] {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // `lancer --plan` reste la vague d'un plan, et `Plan` sa commande.
    assert!(matches!(options(&args(&["lancer", "claude", "--plan", "plan"])), Ok(Commande::Plan { .. })));
}

#[test]
fn options_remarques_seul() {
    // `remarques <fichier>` : le chemin garde son `OsString`.
    let lu = |fichier: &str| Ok(Commande::LireRemarques { fichier: fichier.into() });
    assert_eq!(options(&args(&["remarques", "REMARQUES.lydite"])), lu("REMARQUES.lydite"));
    let mauvais = non_utf8();
    let complete = [args(&["remarques"]), vec![mauvais.clone()]].concat();
    assert_eq!(options(&complete), Ok(Commande::LireRemarques { fichier: PathBuf::from(mauvais) }));
    // Chaque faute : 1006, dont la preuve dit ce qui a été vu, et la correction donne l'usage. Un drapeau n'est pas
    // un fichier, et rien ne suit le fichier.
    for (mots, vu) in [
        ("remarques", "`remarks`: the remarks file is missing"),
        ("remarques --n 1", "« --n » is a flag"),
        ("remarques --decoupe p", "« --decoupe » is a flag"),
        ("remarques -", "« - » is a flag"),
        ("remarques p q", "`remarks` takes only `<file>`, and « q » follows it"),
        ("remarques p --decoupe", "and « --decoupe » follows it"),
        ("remarques p ?", "follows it"),
    ] {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Le mot est dans l'usage et dans la liste fermée que le refus 1005 donne.
    assert!(USAGE.contains("lydite remarks <file>"), "{USAGE}");
}

#[test]
fn options_lecons_seul() {
    let lu = |dossier: &str| Ok(Commande::LireLecons { dossier: dossier.into() });
    assert_eq!(options(&args(&["lecons", "depot"])), lu("depot"));
    let mauvais = non_utf8();
    let complete = [args(&["lecons"]), vec![mauvais.clone()]].concat();
    assert_eq!(options(&complete), Ok(Commande::LireLecons { dossier: PathBuf::from(mauvais) }));
    for (mots, vu) in [
        ("lecons", "`lessons`: the repository folder is missing"),
        ("lecons --n 1", "« --n » is a flag"),
        ("lecons -", "« - » is a flag"),
        ("lecons p q", "`lessons` takes only `<folder>`, and « q » follows it"),
    ] {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
    }
    assert!(USAGE.contains("lydite lessons <folder>"), "{USAGE}");
}

#[test]
fn options_attendre_seul() {
    assert_eq!(options(&args(&["attendre"])), Ok(Commande::Attendre));
    // Un mot de plus est le refus 1006, qui nomme ce mot, comme pour `etat`.
    for mots in [&["attendre", "x"][..], &["attendre", "--n", "1"], &["attendre", "x", "y"]] {
        let refus = options(&args(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots:?}");
        assert!(refus.preuve.contains(&format!("« {} » follows it", mots[1])), "{refus}");
    }
    assert!(USAGE.contains("lydite wait"), "{USAGE}");
}

#[test]
fn options_skill_seul() {
    // Le mot est le même dans les deux langues, et un mot de plus est le refus 1006, qui le nomme.
    assert_eq!(options(&args(&["skill"])), Ok(Commande::Skill));
    for mots in [&["skill", "x"][..], &["skill", "--n", "1"], &["skill", "x", "y"]] {
        let refus = options(&args(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots:?}");
        assert!(refus.preuve.contains(&format!("« {} » follows it", mots[1])), "{refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
    }
    // La dernière forme de l'usage, après toutes les autres.
    assert!(USAGE.ends_with(" or lydite skill"), "{USAGE}");
}

#[test]
fn options_etat_seul() {
    assert_eq!(options(&args(&["etat"])), Ok(Commande::Etat));
    // Ce qui valait None est le refus 1006 ; l'absence de commande aussi.
    for mots in [&["etat", "x"][..], &["etat", "--n", "1"], &["etat", "x", "y"], &["lancer"]] {
        assert_eq!(code(mots), Code::CommandeMalFormee, "{mots:?}");
    }
    assert_eq!(options(&[]).unwrap_err().code, Code::CommandeMalFormee);
    // Un mot qui n'est pas une commande est le refus 1005.
    assert_eq!(code(&["bonjour"]), Code::CommandeInconnue);
}

#[test]
fn options_plan_lu() {
    let attendu = |depot: Option<&str>, pr: Option<&str>| {
        let (depot, pr) = (depot.map(PathBuf::from), pr.map(String::from));
        Ok(Commande::Plan { profil: "claude".into(), plan: "p.txt".into(), depot, pr })
    };
    assert_eq!(options(&args(&["lancer", "claude", "--plan", "p.txt"])), attendu(None, None));
    // `--depot` vient avant ou après `--plan`.
    assert_eq!(options(&args(&["lancer", "claude", "--plan", "p.txt", "--depot", "d"])), attendu(Some("d"), None));
    assert_eq!(options(&args(&["lancer", "claude", "--depot", "d", "--plan", "p.txt"])), attendu(Some("d"), None));
    // `--pr <branche>` aussi, seul ou avec `--depot`, où qu'il vienne : la branche est lue telle quelle.
    assert_eq!(options(&args(&["lancer", "claude", "--plan", "p.txt", "--pr", "main"])), attendu(None, Some("main")));
    let pr_au_milieu = ["lancer", "claude", "--depot", "d", "--pr", "release/1.2", "--plan", "p.txt"];
    assert_eq!(options(&args(&pr_au_milieu)), attendu(Some("d"), Some("release/1.2")));
    let pr_en_tete = ["lancer", "claude", "--pr", "dev", "--plan", "p.txt", "--depot", "d"];
    assert_eq!(options(&args(&pr_en_tete)), attendu(Some("d"), Some("dev")));
    // Un tiret au milieu d'un nom de branche passe : seul un tiret en tête est refusé.
    let tiret = options(&args(&["lancer", "claude", "--plan", "p.txt", "--pr", "a-b"]));
    assert_eq!(tiret, attendu(None, Some("a-b")));
    // Les deux chemins gardent leur `OsString` : un chemin n'est pas forcément de l'UTF-8.
    let mauvais = non_utf8();
    let (plan, depot) = (vec![mauvais.clone()], vec![mauvais.clone()]);
    let complete = [args(&["lancer", "claude", "--plan"]), plan, args(&["--depot"]), depot].concat();
    let Ok(Commande::Plan { plan, depot, .. }) = options(&complete) else { panic!("{complete:?}") };
    assert_eq!((plan, depot), (PathBuf::from(&mauvais), Some(PathBuf::from(mauvais))));
    // Sans `--plan` après `lancer`, la ligne est celle de M3a, inchangée.
    assert!(matches!(options(&args(&["lancer", "claude", "obj", "--depot", "d"])), Ok(Commande::Lancer(_))));
}

#[test]
fn options_plan_refuse_ce_qui_ne_va_pas_avec_lui() {
    // Chaque faute de la ligne : le mot que la preuve montre de ce qui a été vu. Aucun drapeau n'est ignoré en
    // silence, car la ligne ferait croire à n copies, ou à une autre preuve.
    let cas = [
        ("lancer claude --plan p --n 3", "« --n »"),
        ("lancer claude --n 3 --plan p", "« --n »"),
        ("lancer claude --plan p --verifier x", "« --verifier »"),
        ("lancer claude objectif --plan p", "« objectif »"),
        ("lancer claude --plan p objectif", "« objectif »"),
        ("lancer claude --plan p --x 1", "« --x »"),
        ("lancer claude --plan p --plan q", "repeated: --plan"),
        ("lancer claude --plan p --depot d --depot e", "repeated: --repo"),
        ("lancer claude --plan p --depot d --plan", "repeated: --plan"),
        ("lancer claude --plan", "without a value: --plan"),
        ("lancer claude --plan p --depot", "without a value: --repo"),
        // Une valeur n'est jamais l'un des deux drapeaux : l'oublier se dit, au lieu de manger le suivant.
        ("lancer claude --plan --depot d", "without a value: --plan"),
        ("lancer claude --depot --plan p", "without a value: --repo"),
        ("lancer claude --depot d --plan", "without a value: --plan"),
        // Le profil est le premier mot, et n'est pas un drapeau.
        ("lancer --plan p", "« --plan »"),
        ("lancer --depot d --plan p", "« --depot »"),
        ("lancer --n 3 --plan p", "« --n »"),
        ("lancer ? --plan p", "the profile"),
        ("lancer --pr main --plan p", "« --pr »"),
        // `--pr` veut une branche : une valeur qui commence par `-` n'en est pas une (git la lirait comme une
        // option), un drapeau n'est pas une valeur, et il se donne une fois.
        ("lancer claude --plan p --pr -x", "« -x »"),
        ("lancer claude --plan p --pr --force", "« --force »"),
        ("lancer claude --pr - --plan p", "« - »"),
        ("lancer claude --plan p --pr a --pr b", "repeated: --pr"),
        ("lancer claude --plan p --pr", "without a value: --pr"),
        ("lancer claude --pr --plan p", "without a value: --pr"),
        ("lancer claude --plan p --pr --depot d", "without a value: --pr"),
        // Les mêmes fautes en anglais, et en mêlant les deux langues : le mot anglais est nommé, et les deux mots vus
        // quand l'humain a mêlé les deux langues.
        ("launch claude --plan p --verify x", "« --verify »"),
        ("launch claude --plan p --repo d --repo e", "repeated: --repo"),
        ("launch claude --plan p --repo d --depot e", "repeated: --repo, seen as « --repo » and « --depot »"),
        ("launch claude --plan p --depot d --repo e", "repeated: --repo, seen as « --depot » and « --repo »"),
        ("launch claude --repo --plan p", "without a value: --repo"),
        ("launch --repo d --plan p", "« --repo »"),
        ("lancer claude --plan p --pr ?", "the branch of --pr"),
    ];
    for (mots, vu) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Un mot qui n'est pas de l'UTF-8, là où un drapeau est attendu, est nommé sans panique.
    let refus = options(&ligne("lancer claude ? --plan p")).unwrap_err();
    assert_eq!(refus.code, Code::CommandeMalFormee);
    assert!(refus.preuve.contains(&format!("{:?}", non_utf8())), "{refus}");
    // `--plan` manque : `options` ne passe à `options_plan` que si le mot paraît, mais la lecture ne s'y fie pas.
    let refus = options_plan(&args(&["claude", "--depot", "d"])).unwrap_err();
    assert_eq!(refus.code, Code::CommandeMalFormee);
    assert!(refus.preuve.contains("--plan <file>"), "{refus}");
    let refus = options_plan(&[]).unwrap_err();
    assert!(refus.preuve.contains("the profile is missing"), "{refus}");
}

#[test]
fn un_pr_sans_plan_ni_decouper_est_refuse() {
    // `--pr` ne va qu'avec `--plan` ou `--decouper` : sans l'un d'eux, la ligne est celle de M3a, qui ne le connaît
    // pas. Où qu'il paraisse, même à la place de l'objectif (que la ligne ne prendrait pas pour un drapeau), c'est
    // le refus 1006 qui le dit.
    let cas = [
        "lancer claude obj --pr main",
        "lancer claude --pr main",
        "lancer claude --pr",
        "lancer --pr main",
        "lancer claude obj --n 2 --verifier x --depot d --pr main",
        "lancer claude obj --pr",
        "lancer ? obj --pr main",
        // `--observer` ne suffit pas : il ne va qu'avec `--decouper`, qui seul ouvre `--pr`.
        "lancer claude obj --observer --pr main",
    ];
    for mots in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert_eq!(refus.preuve, "`launch`: --pr goes only with --plan <file> or --split", "{mots}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Avec `--plan` ou `--decouper`, il passe ; sans `--pr`, rien ne change.
    assert!(matches!(options(&args(&["lancer", "claude", "--plan", "p", "--pr", "main"])), Ok(Commande::Plan { .. })));
    let decouper = ["lancer", "claude", "obj", "--decouper", "--verifier", "x", "--pr", "main"];
    assert!(matches!(options(&args(&decouper)), Ok(Commande::Lancer(Options { decouper: true, .. }))));
    assert!(matches!(options(&args(&["lancer", "claude", "obj", "--depot", "d"])), Ok(Commande::Lancer(_))));
}

#[test]
fn options_decouper_lues() {
    let attendu = |observer: bool, depot: Option<&str>, pr: Option<&str>| {
        Ok(Commande::Lancer(Options {
            profil: "claude".into(),
            objectif: "obj".into(),
            n: 1,
            verifier: Some("cargo test".into()),
            depot: depot.map(PathBuf::from),
            decouper: true,
            observer,
            pr: pr.map(String::from),
            issue: None,
        }))
    };
    // `--decouper` ne prend pas de valeur : il se lit où qu'il vienne, avant ou après `--verifier`.
    let seul = ["lancer", "claude", "obj", "--decouper", "--verifier", "cargo test"];
    assert_eq!(options(&args(&seul)), attendu(false, None, None));
    let apres = ["lancer", "claude", "obj", "--verifier", "cargo test", "--decouper"];
    assert_eq!(options(&args(&apres)), attendu(false, None, None));
    // Un objectif d'une ligne passe, quel que soit son texte.
    let une_ligne = ["lancer", "claude", "faire a, puis b", "--decouper", "--verifier", "cargo test"];
    let Ok(Commande::Lancer(o)) = options(&args(&une_ligne)) else { panic!("{une_ligne:?}") };
    assert_eq!(o.objectif, "faire a, puis b");
    // `--observer`, `--depot` et `--pr` vont avec lui, dans l'ordre qu'on veut, seuls ou ensemble.
    let observe = ["lancer", "claude", "obj", "--decouper", "--observer", "--verifier", "cargo test"];
    assert_eq!(options(&args(&observe)), attendu(true, None, None));
    let observe_apres = ["lancer", "claude", "obj", "--observer", "--verifier", "cargo test", "--decouper"];
    assert_eq!(options(&args(&observe_apres)), attendu(true, None, None));
    let pr = ["lancer", "claude", "obj", "--decouper", "--verifier", "cargo test", "--pr", "main"];
    assert_eq!(options(&args(&pr)), attendu(false, None, Some("main")));
    let tout = ["lancer", "claude", "obj", "--pr", "dev", "--depot", "d", "--verifier", "cargo test"];
    let tout = [&tout[..], &["--decouper", "--observer"]].concat();
    assert_eq!(options(&args(&tout)), attendu(true, Some("d"), Some("dev")));
    // Sans `--decouper`, la ligne de M3a n'a ni l'un ni l'autre.
    let m3a = options(&args(&["lancer", "claude", "obj", "--verifier", "cargo test"]));
    assert!(matches!(m3a, Ok(Commande::Lancer(Options { decouper: false, observer: false, pr: None, .. }))));
    // Un tiret au milieu d'une branche passe, comme avec `--plan` ; la valeur de `--pr` n'est pas `--split`.
    let tiret = ["lancer", "claude", "obj", "--decouper", "--verifier", "cargo test", "--pr", "a-b"];
    assert_eq!(options(&args(&tiret)), attendu(false, None, Some("a-b")));
    // Le chemin de `--depot` garde son `OsString`, avec `--decouper` aussi.
    let mauvais = non_utf8();
    let complete =
        [args(&["lancer", "claude", "obj", "--decouper", "--verifier", "x", "--depot"]), vec![mauvais.clone()]];
    let Ok(Commande::Lancer(o)) = options(&complete.concat()) else { panic!("{complete:?}") };
    assert_eq!(o.depot, Some(PathBuf::from(mauvais)));
}

#[test]
fn options_issue_lues() {
    // `--issue <n>` remplace l'objectif : le profil est suivi des seuls drapeaux, dans l'ordre qu'on veut.
    let attendu = |observer: bool, pr: Option<&str>| {
        Ok(Commande::Lancer(Options {
            profil: "claude".into(),
            objectif: String::new(),
            n: 1,
            verifier: Some("cargo test".into()),
            depot: Some("d".into()),
            decouper: true,
            observer,
            pr: pr.map(String::from),
            issue: Some(42),
        }))
    };
    let seule = ["lancer", "claude", "--issue", "42", "--decouper", "--verifier", "cargo test", "--depot", "d"];
    assert_eq!(options(&args(&seule)), attendu(false, None));
    let tout = ["lancer", "claude", "--decouper", "--depot", "d", "--verifier", "cargo test", "--pr", "main"];
    let tout = [&tout[..], &["--observer", "--issue", "42"]].concat();
    assert_eq!(options(&args(&tout)), attendu(true, Some("main")));
    // Le plus grand entier positif passe.
    let max = u64::MAX.to_string();
    let grande = ["lancer", "claude", "--issue", &max, "--decouper", "--verifier", "x"];
    assert!(matches!(options(&args(&grande)), Ok(Commande::Lancer(Options { issue: Some(u64::MAX), .. }))));
}

#[test]
fn options_issue_refuse_ce_qui_ne_va_pas_avec_lui() {
    let cas = [
        // Sans `--decouper`.
        ("lancer claude --issue 3", "--issue goes only with --split"),
        ("lancer claude --issue 3 --verifier x", "--issue goes only with --split"),
        ("lancer claude --issue 3 --verifier x --pr main", "--pr goes only with"),
        // Avec un objectif, avant ou après.
        ("lancer claude obj --issue 3 --decouper --verifier x", "--issue replaces the objective"),
        ("lancer claude --issue 3 obj --decouper --verifier x", "unknown flag: « obj »"),
        // Un numéro nul, négatif, non entier, ou absent.
        ("lancer claude --issue 0 --decouper --verifier x", "a positive integer, not « 0 »"),
        ("lancer claude --issue -3 --decouper --verifier x", "a positive integer, not « -3 »"),
        ("lancer claude --issue 3.5 --decouper --verifier x", "a positive integer, not « 3.5 »"),
        ("lancer claude --issue douze --decouper --verifier x", "a positive integer, not « douze »"),
        ("lancer claude --issue 18446744073709551616 --decouper --verifier x", "a positive integer"),
        ("lancer claude --issue ? --decouper --verifier x", "a positive integer"),
        ("lancer claude --decouper --verifier x --issue", "flag without a value: --issue"),
        ("lancer claude --issue 3 --issue 4 --decouper --verifier x", "flag repeated: --issue"),
        // Il exclut `--n` et `--plan`, et demande `--verifier` comme `--decouper`.
        ("lancer claude --issue 3 --decouper --verifier x --n 1", "--split excludes --n"),
        ("lancer claude --issue 3 --plan p", "is not one of them"),
        ("lancer claude --issue 3 --decouper --plan p", "--split excludes --plan"),
        ("lancer claude --issue 3 --decouper", "--split requires --verify"),
        // Pas de profil.
        ("lancer --issue 3 --decouper --verifier x", "the profile is missing"),
        // Le `--issue` que `--verifier` prend pour valeur n'en est pas un : il manque l'objectif.
        ("lancer claude --verifier --issue --decouper", "the objective, or `--issue <n>`, is missing"),
    ];
    for (mots, vu) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
    }
}

#[test]
fn options_decouper_refuse_ce_qui_ne_va_pas_avec_lui() {
    // Chaque faute de la ligne : le refus 1006, le mot que la preuve montre de ce qui a été vu, l'usage en correction.
    let cas = [
        // `--decouper` demande `--verifier`, la preuve de la vague, que seul l'humain donne.
        ("lancer claude obj --decouper", "--split requires --verify"),
        ("lancer claude obj --decouper --depot d", "--split requires --verify"),
        ("lancer claude obj --decouper --observer", "--split requires --verify"),
        // Il exclut `--n`, même à 1, et `--plan`, où qu'ils paraissent.
        ("lancer claude obj --decouper --verifier x --n 3", "--split excludes --n"),
        ("lancer claude obj --n 1 --verifier x --decouper", "--split excludes --n"),
        ("lancer claude obj --n 3 --decouper", "--split excludes --n"),
        ("lancer claude --plan p --decouper", "--split excludes --plan"),
        ("lancer claude obj --decouper --verifier x --plan p", "--split excludes --plan"),
        ("lancer claude --decouper --plan p --depot d --pr main", "--split excludes --plan"),
        // `--observer` ne va qu'avec lui.
        ("lancer claude obj --observer", "--dry-run goes only with --split"),
        ("lancer claude obj --verifier x --observer", "--dry-run goes only with --split"),
        ("lancer claude obj --observer --n 2", "--dry-run goes only with --split"),
        // Un drapeau sans valeur ne se dit qu'une fois.
        ("lancer claude obj --decouper --decouper --verifier x", "flag repeated: --split"),
        ("lancer claude obj --decouper --verifier x --observer --observer", "flag repeated: --dry-run"),
        // Un drapeau à la place du profil ou de l'objectif : il manque l'un ou l'autre.
        ("lancer claude --decouper --verifier x", "`launch`: the objective is missing"),
        ("lancer claude --observer --decouper --verifier x", "`launch`: the objective is missing"),
        ("lancer claude --pr main --decouper --verifier x", "`launch`: the objective is missing"),
        ("lancer --decouper obj --verifier x", "`launch`: the profile and the objective are missing"),
        ("lancer --observer --decouper", "`launch`: the profile and the objective are missing"),
        // `--pr` veut le nom d'une branche, comme avec `--plan`.
        ("lancer claude obj --decouper --verifier x --pr", "without a value: --pr"),
        ("lancer claude obj --decouper --verifier x --pr -x", "« -x »"),
        ("lancer claude obj --decouper --verifier x --pr a --pr b", "repeated: --pr"),
        ("lancer claude obj --decouper --verifier x --pr ?", "the branch of --pr"),
        // Un drapeau inconnu l'est avec `--decouper` aussi.
        ("lancer claude obj --decouper --verifier x --x 1", "unknown flag: « --x »"),
        // Le `--decouper` que `--verifier` prend pour valeur n'en est pas un : `--pr` n'a plus où aller.
        ("lancer claude obj --verifier --decouper --pr main", "--pr goes only with --plan <file> or --split"),
    ];
    for (mots, vu) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Une preuve blanche est le refus 6014, comme sans `--decouper` : rejouée, elle réussirait toujours.
    for blanche in ["", " ", "\t"] {
        let refus = options(&args(&["lancer", "claude", "obj", "--decouper", "--verifier", blanche])).unwrap_err();
        assert_eq!(refus.code, Code::PreuveVide, "{blanche:?}");
    }
    // Le premier refus d'une lecture gauche à droite l'emporte sur ceux d'après la ligne entière.
    let refus = options(&args(&["lancer", "claude", "obj", "--decouper", "--verifier", " ", "--n", "2"])).unwrap_err();
    assert_eq!(refus.code, Code::PreuveVide);
    let refus = options(&args(&["lancer", "claude", "obj", "--x", "--decouper"])).unwrap_err();
    assert!(refus.preuve.contains("unknown flag: « --x »"), "{refus}");
    // Un objectif sur plusieurs lignes : l'en-tête du plan n'en tient qu'une, `--decouper` lu avant ou après
    // `--verifier`.
    for (objectif, vu) in [("a\nb", r#""a\nb""#), ("a\r\nb", r#""a\r\nb""#), ("a\rb", r#""a\rb""#)] {
        let avant = ["lancer", "claude", objectif, "--decouper", "--verifier", "x"];
        let apres = ["lancer", "claude", objectif, "--verifier", "x", "--decouper"];
        for mots in [avant, apres] {
            let refus = options(&args(&mots)).unwrap_err();
            assert_eq!(refus.code, Code::CommandeMalFormee, "{mots:?}");
            assert!(refus.preuve.contains("`launch`: --split wants a one-line objective"), "{refus}");
            assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
            assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
    }
    // Sans `--decouper`, un objectif sur plusieurs lignes passe comme avant.
    assert!(options(&args(&["lancer", "claude", "a\nb", "--verifier", "x"])).is_ok());
}

#[test]
fn l_usage_donne_les_deux_formes_de_lancer() {
    // La forme de M3a, telle quelle (tests/contrat.rs la lit), et la forme de `--plan` à côté, avec ses deux
    // drapeaux facultatifs.
    let m3a = r#"lydite launch <profile> "<objective>" [--n N] [--verify "<command>"] [--repo <path>]"#;
    let plan = "lydite launch <profile> --plan <file> [--repo <path>] [--pr <branch>]";
    for forme in [m3a, plan, "lydite status"] {
        assert!(USAGE.contains(forme), "{forme}");
    }
}

#[test]
fn l_usage_dit_le_decoupage_et_les_commandes_de_m5() {
    // La forme du découpage, entière, avec `--dry-run`, `--repo` et `--pr` facultatifs, et les deux commandes que
    // d'autres pièces lisent : l'usage les dit, comme la liste fermée que donne le refus 1005.
    let decouper = concat!(
        r#"lydite launch <profile> "<objective>" --split --verify "<proof>" [--dry-run] [--repo <path>] "#,
        r#"[--pr <branch>]"#,
    );
    for forme in [decouper, "lydite plan [--split] <file>", "lydite remarks <file>"] {
        assert!(USAGE.contains(forme), "{forme}");
    }
    let inconnue = options(&args(&["Plan", "p.txt"])).unwrap_err();
    assert_eq!(inconnue.code, Code::CommandeInconnue);
    for forme in ["--split", "--dry-run", "lydite plan [--split] <file>", "lydite remarks <file>"] {
        assert!(inconnue.correction.contains(forme), "la correction du refus 1005 dit {forme} : {inconnue}");
    }
}

#[test]
fn l_usage_est_en_anglais() {
    // L'usage est la première chose que l'humain lit : il est en anglais. Les commandes et leurs drapeaux restent ce
    // qu'ils sont, et les mots qui nomment les arguments aussi.
    let francais =
        [" ou ", "<profil>", "<objectif>", "<fichier>", "<dossier>", "<chemin>", "<branche>", "<preuve>", "<commande>"];
    let anglais =
        [" or ", "<profile>", "<objective>", "<file>", "<folder>", "<path>", "<branch>", "<proof>", "<command>"];
    for (francais, anglais) in francais.iter().zip(anglais.iter()) {
        assert!(!USAGE.contains(francais), "l'usage dit encore {francais} : {USAGE}");
        assert!(USAGE.contains(anglais), "l'usage ne dit pas {anglais} : {USAGE}");
    }
}

#[test]
fn l_usage_ne_montre_aucun_mot_francais_de_la_table() {
    // Chaque mot français qui a un double anglais est absent de l'usage.
    let francais = ["lancer", "etat", "attendre", "accepter", "profil ", "remarques", "lecons"];
    let drapeaux = ["--verifier", "--depot", "--decouper", "--decoupe", "--observer"];
    for mot in francais.iter().chain(drapeaux.iter()) {
        assert!(!USAGE.contains(mot), "l'usage dit encore {mot} : {USAGE}");
    }
}

#[test]
fn chaque_mot_anglais_est_lu_comme_son_double_francais() {
    // Pour chaque paire de lignes, l'une en français, l'autre en anglais, puis des lignes qui mêlent les deux
    // langues : `options` rend la même valeur, ou le même code de refus, avec la même correction.
    let paires = [
        // Commandes sans drapeau.
        ("etat", "status"),
        ("attendre", "wait"),
        ("etat x", "status x"),
        ("attendre x y", "wait x y"),
        ("accepter veille", "accept veille"),
        ("accepter", "accept"),
        ("accepter a b", "accept a b"),
        ("accepter ?", "accept ?"),
        ("profil veille", "profile veille"),
        ("profil", "profile"),
        ("profil a b", "profile a b"),
        ("remarques r.txt", "remarks r.txt"),
        ("remarques", "remarks"),
        ("remarques -x", "remarks -x"),
        ("remarques p q", "remarks p q"),
        ("lecons depot", "lessons depot"),
        ("lecons", "lessons"),
        ("lecons -x", "lessons -x"),
        ("lecons p q", "lessons p q"),
        // `plan` : son seul drapeau.
        ("plan p.txt", "plan p.txt"),
        ("plan --decoupe p.txt", "plan --split p.txt"),
        ("plan --decoupe", "plan --split"),
        ("plan p --decoupe", "plan p --split"),
        ("plan --decoupe --decoupe p", "plan --split --split p"),
        // `launch` : chaque forme de l'usage.
        ("lancer claude obj", "launch claude obj"),
        ("lancer claude obj --n 3 --verifier cargo --depot d", "launch claude obj --n 3 --verify cargo --repo d"),
        ("lancer claude obj --verifier", "launch claude obj --verify"),
        ("lancer claude obj --verifier ?", "launch claude obj --verify ?"),
        ("lancer claude obj --verifier x --verifier y", "launch claude obj --verify x --verify y"),
        ("lancer claude obj --depot d --depot e", "launch claude obj --repo d --repo e"),
        ("lancer claude --verifier x", "launch claude --verify x"),
        ("lancer --depot d", "launch --repo d"),
        ("lancer claude --plan p --depot d --pr main", "launch claude --plan p --repo d --pr main"),
        ("lancer claude --depot d --plan p", "launch claude --repo d --plan p"),
        ("lancer claude --plan p --verifier x", "launch claude --plan p --verify x"),
        ("lancer claude --plan p --depot d --depot e", "launch claude --plan p --repo d --repo e"),
        ("lancer claude --plan p --depot", "launch claude --plan p --repo"),
        ("lancer claude --plan p --decouper", "launch claude --plan p --split"),
        ("lancer claude obj --decouper --verifier x", "launch claude obj --split --verify x"),
        ("lancer claude obj --decouper --verifier x --observer", "launch claude obj --split --verify x --dry-run"),
        ("lancer claude obj --decouper --verifier x --pr dev", "launch claude obj --split --verify x --pr dev"),
        ("lancer claude obj --decouper --verifier x --depot d", "launch claude obj --split --verify x --repo d"),
        ("lancer claude obj --decouper", "launch claude obj --split"),
        ("lancer claude obj --decouper --verifier x --n 2", "launch claude obj --split --verify x --n 2"),
        ("lancer claude obj --observer", "launch claude obj --dry-run"),
        ("lancer claude obj --decouper --decouper --verifier x", "launch claude obj --split --split --verify x"),
        (
            "lancer claude obj --decouper --verifier x --observer --observer",
            "launch claude obj --split --verify x --dry-run --dry-run",
        ),
        ("lancer claude obj --pr main", "launch claude obj --pr main"),
        ("lancer claude --issue 3 --decouper --verifier x", "launch claude --issue 3 --split --verify x"),
        ("lancer claude --issue 3 --verifier x", "launch claude --issue 3 --verify x"),
        ("lancer claude --issue 3 --decouper", "launch claude --issue 3 --split"),
        ("lancer claude obj --issue 3 --decouper --verifier x", "launch claude obj --issue 3 --split --verify x"),
        // Une valeur qui est le mot de l'autre langue est une valeur, ou un drapeau, comme chez l'autre.
        ("lancer claude obj --verifier --decouper --pr main", "launch claude obj --verify --split --pr main"),
        ("lancer claude obj --depot --n", "launch claude obj --repo --n"),
        // Des lignes qui mêlent les deux langues.
        ("lancer claude obj --verify cargo --depot d", "launch claude obj --verifier cargo --repo d"),
        ("lancer claude obj --split --verifier x --dry-run", "launch claude obj --decouper --verify x --observer"),
        ("lancer claude --plan p --repo d", "launch claude --plan p --depot d"),
        ("launch claude obj --decouper --verifier x --dry-run", "lancer claude obj --split --verify x --observer"),
        ("lancer claude obj --verify x --verifier y", "launch claude obj --verifier x --verify y"),
        ("lancer claude obj --repo d --depot e", "launch claude obj --depot d --repo e"),
        ("lancer claude obj --decouper --split --verify x", "launch claude obj --split --decouper --verify x"),
        ("lancer claude --plan p --repo d --depot e", "launch claude --plan p --depot d --repo e"),
        ("lancer claude obj --split --verify x --pr", "launch claude obj --decouper --verifier x --pr"),
    ];
    for (francais, anglais) in paires {
        let (f, a) = (options(&ligne(francais)), options(&ligne(anglais)));
        match (&f, &a) {
            (Ok(_), Ok(_)) => assert_eq!(f, a, "{francais} / {anglais}"),
            (Err(f), Err(a)) => {
                assert_eq!(f.code, a.code, "{francais} / {anglais}");
                assert_eq!(f.correction, a.correction, "{francais} / {anglais}");
            }
            _ => panic!("{francais} / {anglais} : {f:?} / {a:?}"),
        }
    }
    // Les lignes d'une paire ne se lisent pas toutes pareil : un refus et une valeur existent dans la table.
    assert!(options(&ligne("lancer claude obj --verifier x")).is_ok());
    assert!(options(&ligne("launch claude obj --split")).is_err());
}

#[test]
fn les_deux_formes_d_un_drapeau_sont_un_drapeau_redit() {
    // `--verify x --verifier y` : le refus 1006 d'un drapeau redit, dont la preuve nomme les deux mots vus.
    let cas = [
        ("launch claude obj --verify x --verifier y", "--verify", "--verifier"),
        ("launch claude obj --verifier x --verify y", "--verifier", "--verify"),
        ("launch claude obj --repo d --depot e", "--repo", "--depot"),
        ("launch claude obj --split --decouper --verify x", "--split", "--decouper"),
        ("launch claude obj --dry-run --observer", "--dry-run", "--observer"),
        ("launch claude --plan p --depot d --repo e", "--depot", "--repo"),
    ];
    for (mots, premier, second) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        let (premier, second) = (format!("« {premier} »"), format!("« {second} »"));
        assert!(refus.preuve.starts_with("flag repeated: --"), "{refus}");
        assert!(refus.preuve.contains(&premier) && refus.preuve.contains(&second), "{mots} : {refus}");
    }
    // Un même mot redit nomme le mot anglais, et lui seul, même s'il est tapé en français.
    for (mots, preuve) in [
        ("lancer claude obj --verifier x --verifier y", "flag repeated: --verify"),
        ("launch claude obj --verify x --verify y", "flag repeated: --verify"),
        ("lancer claude obj --decouper --decouper", "flag repeated: --split"),
    ] {
        assert_eq!(options(&ligne(mots)).unwrap_err().preuve, preuve, "{mots}");
    }
}

#[test]
fn la_casse_compte_pour_les_mots_anglais_comme_pour_les_francais() {
    for mot in ["Launch", "LAUNCH", "STATUS", "Status", "Wait", "Accept", "Profile", "Remarks", "Lessons", "Lancer"] {
        assert_eq!(code(&[mot]), Code::CommandeInconnue, "{mot}");
    }
    // Un drapeau en majuscules n'est pas un drapeau : il est inconnu, ou il est l'objectif manquant.
    let refus = options(&args(&["launch", "claude", "obj", "--Verify", "x"])).unwrap_err();
    assert!(refus.preuve.contains("unknown flag: « --Verify »"), "{refus}");
}

#[test]
fn les_preuves_nomment_le_mot_anglais_meme_tape_en_francais() {
    // Aucun mot français de la table ne paraît dans ce que Lydite écrit, à part ceux que l'humain a tapés et que la
    // preuve cite entre « ».
    let mots_francais = [
        "`lancer`",
        "`etat`",
        "`attendre`",
        "`accepter`",
        "`profil`",
        "`remarques`",
        "`lecons`",
        "--verifier",
        "--depot",
        "--decouper",
        "--decoupe",
        "--observer",
        "lydite lancer",
        "lydite etat",
    ];
    for ligne_ in [
        "lancer",
        "lancer claude",
        "lancer ? obj",
        "lancer claude obj --verifier",
        "lancer claude obj --depot",
        "lancer claude obj --decouper",
        "lancer claude obj --observer",
        "lancer claude obj --pr main",
        "lancer claude --issue 3",
        "lancer claude obj --decouper --verifier x --n 2",
        "lancer claude --plan p --decouper",
        "lancer claude --plan p --depot",
        "lancer claude --plan p --depot d --depot e",
        "lancer claude --plan",
        "etat x",
        "attendre x",
        "accepter",
        "accepter a b",
        "profil",
        "profil a b",
        "remarques",
        "remarques a b",
        "lecons",
        "lecons a b",
        "plan",
        "plan a b",
        "bonjour",
    ] {
        let refus = options(&ligne(ligne_)).unwrap_err();
        // Les mots tapés sont cités entre « » : on les retire avant de chercher un mot français.
        let sans_cites = refus
            .to_string()
            .split('«')
            .map(|reste| reste.split_once('»').map_or(reste, |(_, fin)| fin))
            .collect::<String>();
        for francais in mots_francais {
            assert!(!sans_cites.contains(francais), "{ligne_} : {francais} dans {refus}");
        }
    }
}

#[test]
fn les_preuves_de_mot_en_trop_sont_en_anglais() {
    for (mots, mot_anglais) in [("lecons p q", "<folder>"), ("remarques p q", "<file>"), ("plan p q", "<file>")] {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mots}");
        assert!(!refus.preuve.contains("<dossier>") && !refus.preuve.contains("<fichier>"), "{refus}");
        assert!(refus.preuve.contains(mot_anglais), "{refus}");
    }
}

#[test]
fn un_argument_non_utf8_est_refuse_sans_panique() {
    // Comme mot de commande, ce n'est pas une commande (1005) ; là où il faut un texte, la ligne est mal formée
    // (1006). Un drapeau qui n'est pas de l'UTF-8 est un drapeau inconnu.
    let cas = [
        ("?", Code::CommandeInconnue),
        ("lancer ? obj", Code::CommandeMalFormee),
        ("lancer claude ?", Code::CommandeMalFormee),
        ("lancer claude obj ?", Code::CommandeMalFormee),
        ("lancer claude obj --n ?", Code::CommandeMalFormee),
        ("lancer claude obj --verifier ?", Code::CommandeMalFormee),
        ("lancer claude obj --decouper ?", Code::CommandeMalFormee),
        ("lancer claude obj --decouper --verifier ?", Code::CommandeMalFormee),
        ("lancer claude obj --decouper --verifier x --pr ?", Code::CommandeMalFormee),
        ("etat ?", Code::CommandeMalFormee),
        ("accepter ?", Code::CommandeMalFormee),
        ("accepter claude ?", Code::CommandeMalFormee),
    ];
    for (mots, attendu) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, attendu, "{mots}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
}

#[test]
fn un_depot_non_utf8_est_garde() {
    // `--depot` est un chemin, pas un texte : il traverse sans perdre un octet.
    let depot = non_utf8();
    let complete = [args(&["lancer", "claude", "obj", "--depot"]), vec![depot.clone()]].concat();
    let Ok(Commande::Lancer(o)) = options(&complete) else { panic!("{complete:?}") };
    assert_eq!(o.depot, Some(PathBuf::from(depot)));
}

#[test]
fn chaque_refus_de_la_ligne_de_commande_enseigne() {
    // Chaque faute de la ligne : son code, et le mot que la preuve montre de ce qui a été vu.
    let (inconnue, mal_formee) = (Code::CommandeInconnue, Code::CommandeMalFormee);
    let cas = [
        ("", mal_formee, "no command"),
        ("bonjour", inconnue, "bonjour"),
        ("etat x", mal_formee, "x"),
        ("lancer", mal_formee, "the profile and the objective"),
        ("lancer claude", mal_formee, "the objective"),
        ("lancer ? obj", mal_formee, "the profile"),
        ("lancer claude obj --x 1", mal_formee, "--x"),
        ("lancer claude obj --n 1 --n 2", mal_formee, "repeated: --n"),
        ("lancer claude obj --n", mal_formee, "without a value: --n"),
        ("lancer claude obj --n 0", mal_formee, "« 0 »"),
        ("lancer claude obj --n deux", mal_formee, "« deux »"),
        ("lancer claude obj --verifier", mal_formee, "without a value: --verify"),
        ("lancer claude obj --depot", mal_formee, "without a value: --repo"),
        ("launch claude obj --verify", mal_formee, "without a value: --verify"),
        ("launch claude obj --repo", mal_formee, "without a value: --repo"),
        ("lancer claude --verifier x", mal_formee, "the objective is missing"),
        ("lancer --n 3 obj", mal_formee, "the profile and the objective are missing"),
        ("accepter", mal_formee, "the profile is missing"),
        ("accepter claude veille", mal_formee, "« veille » follows it"),
    ];
    for (mots, code, vu) in cas {
        let refus = options(&ligne(mots)).unwrap_err();
        assert_eq!(refus.code, code, "{mots}");
        assert!(refus.preuve.contains(vu), "la preuve dit ce qui a été vu ({vu}) : {refus}");
        assert!(refus.correction.contains(USAGE), "la correction donne l'usage : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
}

#[test]
fn n_va_de_1_a_la_borne_du_lecteur() {
    // La borne du contrat, écrite ici à la main : la changer change le contrat, et l'en-tête de fil.rs avec lui.
    const BORNE: usize = 10_000;
    // `--n` ne passe pas ce que le fil lit : Kraken n'écrit pas de vague que son lecteur refuserait.
    let ligne = |n: usize| options(&args(&["lancer", "claude", "obj", "--n", &n.to_string()]));
    assert!(matches!(ligne(BORNE), Ok(Commande::Lancer(Options { n: BORNE, .. }))));
    for hors in [BORNE + 1, usize::MAX] {
        let refus = ligne(hors).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee);
        assert!(refus.preuve.contains(&format!("from 1 to 10000, not « {hors} »")), "{refus}");
    }
    // La vague de BORNE mains s'écrit, se relit sans écart, et se dit en une ligne par main, entre son objectif et ses
    // deux lignes de reçu (le coût, l'effort).
    let dossier = env::temp_dir().join(format!("lydite-test-borne-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    Fil::seance(&dossier, "humain").noter(&ouverture(V1, BORNE, "o")).unwrap();
    let Lecture { notes, ecarts } = crate::branchements::dossier::lire(&dossier).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(rendu(&etat(&evenements), MAINTENANT).lines().count(), 3 + BORNE);
    let _ = fs::remove_dir_all(&dossier);
}
