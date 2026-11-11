use super::*;
use crate::branchements::dossier::charger_dans;
use crate::coeur::fil::lot;
use crate::essais::{avec_mur_demande, kraken, racine};
use std::env;
use std::ffi::OsString;
use std::fs;

/// Un chemin absolu pour le système qui joue le test ; il n'est jamais lu, ni créé.
fn absolu(chemin: &str) -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" }).join(chemin)
}

/// Le profil de ce texte, lu comme le fichier `essai.profil` d'un dossier de Kraken fictif, sans `LYDITE_CLAUDE`.
fn lu(texte: &str) -> Result<Profil, Refus> {
    lire("essai", &avec_mur_demande(texte), Some(&absolu("k").join("profils").join("essai.profil")), None)
}

/// Les leurres des dossiers de connexion de Claude, fictifs, jamais lus ni créés : la valeur de `CLAUDE_CONFIG_DIR`,
/// puis le dossier personnel, dont le `.claude` est le second.
fn leurres() -> (PathBuf, PathBuf) {
    (absolu("config"), absolu("personnel"))
}

/// Le profil de ce texte, borné par le dossier de Kraken fictif, le git commun d'un dépôt fictif et les leurres.
fn borne(texte: &str) -> Profil {
    let mut profil = lu(texte).unwrap();
    let (config, personnel) = leurres();
    profil.borner(&absolu("k"), &absolu("depot").join(".git"), Some(&config), Some(&personnel)).unwrap();
    profil
}

/// Ce qu'un profil dit d'une main Claude.
fn claude(profil: &Profil) -> &Claude {
    match &profil.sorte {
        Sorte::Claude(claude) => claude,
        Sorte::Brute(argv) => panic!("une main brute : {argv:?}"),
    }
}

/// Le JSON de `--settings` d'une première tentative dans un bac fictif.
fn reglages(profil: &Profil, verifier: Option<&str>) -> String {
    profil.demande(&absolu("bac"), verifier, false).declaration.expect("une main Claude bornée").reglages
}

/// Le dossier de Kraken fictif en règle, sur le système qui joue le test.
fn regle_k() -> &'static str {
    if cfg!(windows) { "//c/k" } else { "//k" }
}

/// Le git commun fictif de `borne` en règle, sur le système qui joue le test.
fn regle_depot() -> &'static str {
    if cfg!(windows) { "//c/depot/.git" } else { "//depot/.git" }
}

/// Les chemins fictifs de `borne` et de `reglages` en chaînes JSON, écrites à la main : le dossier de Kraken, le
/// bac, puis le git commun.
fn chemins_json() -> [&'static str; 3] {
    if cfg!(windows) {
        [r#""C:\\k""#, r#""C:\\bac""#, r#""C:\\depot\\.git""#]
    } else {
        [r#""/k""#, r#""/bac""#, r#""/depot/.git""#]
    }
}

/// Les règles de `deny` sans aucun sortant déclaré, dans l'ordre : les dix-neuf sortants, les deux `Edit` du dossier
/// `regle`, `Read` et `Edit` de ses réglages de srt, `Read` et `Edit` du fichier de connexion de chaque leurre, puis
/// l'`Edit` du git commun `commun`, en règles écrites à la main.
fn deny_sans_declare(regle: &str, commun: &str, bac: &str) -> String {
    let (config, personnel) =
        if cfg!(windows) { ("//c/config", "//c/personnel/.claude") } else { ("//config", "//personnel/.claude") };
    format!(
        concat!(
            r#""deny":["CronCreate","CronDelete","CronList","ScheduleWakeup","RemoteTrigger","#,
            r#""SendMessage","ListAgents","PushNotification","#,
            r#""SendUserFile","Artifact","ArtifactComments","ArtifactData","ArtifactCheck","DesignSync","#,
            r#""SendToProject","SendToProjectContext","ShareOnboardingGuide","#,
            r#""AskUserQuestion","WebSearch","Edit({regle}/fil.jsonl)","Edit({regle}/profils/**)","#,
            r#""Read({regle}/srt-*.json)","Edit({regle}/srt-*.json)","#,
            r#""Read({config}/.credentials.json)","Edit({config}/.credentials.json)","#,
            r#""Read({personnel}/.credentials.json)","Edit({personnel}/.credentials.json)","#,
            r#""Edit({commun}/**)","Bash(git commit:*)","Bash(git commit *)","#,
            r#""PowerShell(git commit:*)","PowerShell(git commit *)","Edit({bac}/.agents/skills/**)"]"#,
        ),
        bac = bac,
        regle = regle,
        commun = commun,
        config = config,
        personnel = personnel
    )
}

#[test]
fn charger_claude_ou_un_inconnu() {
    // Un dossier de Kraken jetable, sans le moindre profil : `claude` est le profil intégré.
    let dossier = kraken("charger");
    let profil = charger_dans("claude", &dossier).unwrap();
    // Le programme dépend de LYDITE_CLAUDE, que ce test ne touche pas : le reste est fixe.
    assert_eq!(profil.delai_preuve(), Duration::from_secs(300));
    assert_eq!(profil.delai_main(), Duration::from_secs(3600));
    assert_eq!(claude(&profil).modele, "sonnet");
    // Un profil inconnu est le refus 6001 : ce qui a été vu, le fichier qui manque, et les profils qu'on peut nommer.
    let refus = charger_dans("x", &dossier).err().expect("« x » n'est pas un profil");
    assert_eq!(refus.code, Code::ProfilInconnu);
    assert!(refus.preuve.starts_with("profil « x » inconnu : "), "{refus}");
    assert!(refus.preuve.contains("x.profil est absent"), "{refus}");
    assert!(refus.correction.contains("claude"), "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_profil_se_lit_d_un_fichier() {
    let dossier = kraken("fichier");
    let programme = absolu("outils").join("claude");
    let texte = concat!(
        "# La main des corrections.\n",
        "sorte = claude\n",
        "programme = <programme>\n",
        "modele = opus\n",
        "  relance=claude-sonnet-5-5  \n",
        "\n",
        "\t# outils et serveurs\n",
        "outils = SendMessage , mcp__docs\n",
        r#"mcp = {"mcpServers":{"docs":{"command":"docs-mcp","args":["a=b"]}}}"#,
        "\n",
        "mur = exige\n",
        "reseau = docs.exemple.test,paquets.exemple.test\n",
        "lecture = ~/.cargo\n",
        "delai_main = 1800\n",
        "delai_preuve = 60",
    )
    .replace("<programme>", &programme.display().to_string());
    let chemin = dossier.join("profils").join("correcteur.profil");
    fs::write(&chemin, &texte).unwrap();
    let profil = charger_dans("correcteur", &dossier).unwrap();
    let lu = claude(&profil);
    assert_eq!((lu.nom.as_str(), lu.fichier.as_deref()), ("correcteur", Some(chemin.as_path())));
    assert_eq!(lu.empreinte, hex(&sha256(&fs::read(&chemin).unwrap())), "l'empreinte des octets du fichier");
    assert_eq!(lu.programme, programme.display().to_string());
    assert_eq!((lu.modele.as_str(), lu.relance.as_str(), lu.exige), ("opus", "claude-sonnet-5-5", true));
    assert_eq!(lu.outils, ["SendMessage", "mcp__docs"]);
    assert_eq!(lu.mcp.as_deref(), Some(r#"{"mcpServers":{"docs":{"command":"docs-mcp","args":["a=b"]}}}"#));
    assert_eq!(lu.reseau, ["docs.exemple.test", "paquets.exemple.test"]);
    assert_eq!(lu.lecture, ["~/.cargo"]);
    assert_eq!(lu.ligne_lecture, (12, "lecture = ~/.cargo".to_string()));
    assert_eq!((profil.delai_main(), profil.delai_preuve()), (Duration::from_secs(1800), Duration::from_secs(60)));
    assert!(lu.bornes.is_none(), "rien n'est borné avant borner");
    // Un nom nu est un programme aussi ; sans programme écrit, c'est LYDITE_CLAUDE, sinon claude.
    let nu = lu_programme("sorte = claude\nprogramme = claude-essai\n", None);
    assert_eq!(nu, "claude-essai");
    assert_eq!(lu_programme("sorte = claude\n", None), "claude");
    assert_eq!(lu_programme("sorte = claude\n", Some("/opt/claude")), "/opt/claude");
    // Charger ne crée rien, ni empreinte, ni autre chose.
    assert_eq!(fs::read_dir(dossier.join("profils")).unwrap().count(), 1);
    // Un nom hors de [A-Za-z0-9_-] est le refus 6007, avant toute lecture.
    for nom in ["", "../correcteur", "correcteur.profil", "a b", "é", "a/b", r"a\b"] {
        let refus = charger_dans(nom, &dossier).err().expect(nom);
        assert_eq!(refus.code, Code::ProfilNomRefuse, "{nom:?}");
        assert!(refus.preuve.contains(&format!("« {nom} »")), "{refus}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

/// Le programme d'un profil lu de ce texte, avec cette valeur de `LYDITE_CLAUDE`.
fn lu_programme(texte: &str, kraken_claude: Option<&str>) -> String {
    let profil = lire("essai", texte, None, kraken_claude).unwrap();
    claude(&profil).programme.clone()
}

#[test]
fn un_profil_sans_ligne_mur_exige_le_mur_et_demande_ne_vaut_que_ecrit() {
    // `lire` direct : `lu` ajouterait `mur = demande`.
    let sans = lire("essai", "sorte = claude\n", None, None).unwrap();
    assert!(claude(&sans).exige && sans.exige());
    let demande = lire("essai", "sorte = claude\nmur = demande\n", None, None).unwrap();
    assert!(!claude(&demande).exige && !demande.exige());
    let exige = lire("essai", "sorte = claude\nmur = exige\n", None, None).unwrap();
    assert!(exige.exige());
}

#[test]
fn sans_fichier_claude_est_le_profil_integre() {
    let dossier = kraken("integre");
    let profil = charger_dans("claude", &dossier).unwrap();
    let integre = claude(&profil);
    assert_eq!((integre.nom.as_str(), integre.fichier.as_ref()), ("claude", None));
    // Le texte intégré, au format du fichier, et son empreinte : sonnet aux deux tentatives, le mur exigé, les
    // délais par défaut, sans outils, mcp, reseau ni lecture.
    assert_eq!(
        INTEGRE,
        "sorte = claude\nmodele = sonnet\nrelance = sonnet\nmur = exige\ndelai_main = 3600\ndelai_preuve = 300\n"
    );
    assert_eq!(integre.empreinte, hex(&sha256(INTEGRE.as_bytes())));
    assert_eq!((integre.modele.as_str(), integre.relance.as_str(), integre.exige), ("sonnet", "sonnet", true));
    assert!(integre.outils.is_empty() && integre.mcp.is_none() && integre.reseau.is_empty());
    assert!(integre.lecture.is_empty());
    assert_eq!((profil.delai_main(), profil.delai_preuve()), (Duration::from_secs(3600), Duration::from_secs(300)));
    // Un autre nom sans fichier est inconnu (6001) : la preuve nomme le fichier attendu, la correction l'intégré.
    let refus = charger_dans("hermes", &dossier).err().unwrap();
    assert_eq!(refus.code, Code::ProfilInconnu);
    assert!(refus.preuve.contains("« hermes »") && refus.preuve.contains("hermes.profil"), "{refus}");
    assert!(refus.correction.contains("claude"), "{refus}");
    // Un fichier claude.profil l'emporte sur l'intégré.
    fs::write(dossier.join("profils").join("claude.profil"), "sorte = claude\nmodele = haiku\n").unwrap();
    let ecrit = charger_dans("claude", &dossier).unwrap();
    assert_eq!((claude(&ecrit).modele.as_str(), claude(&ecrit).fichier.is_some()), ("haiku", true));
    // Sans dossier de Kraken du tout, l'intégré vaut encore, et rien n'est créé.
    let absent = env::temp_dir().join(format!("lydite-test-profil-absent-{}", lot()));
    assert!(claude(&charger_dans("claude", &absent).unwrap()).fichier.is_none());
    assert!(!absent.exists(), "charger ne crée rien");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn la_cle_mesure_liste_un_ou_deux_reglages_et_aucun_par_defaut() {
    let mesure = |ligne: &str| lu(&format!("sorte = claude\n{ligne}\n")).map(|profil| profil.mesure());
    assert_eq!(lu("sorte = claude\n").unwrap().mesure(), Mesure::default());
    assert_eq!(lu("sorte = claude\n").unwrap().mesure().texte(), "");
    assert_eq!(mesure("mesure = sans-lecture").unwrap(), Mesure { sans_lecture: true, sans_a1: false });
    assert_eq!(mesure("mesure = sans-a1").unwrap(), Mesure { sans_lecture: false, sans_a1: true });
    let deux = Mesure { sans_lecture: true, sans_a1: true };
    assert_eq!(mesure("mesure = sans-lecture, sans-a1").unwrap(), deux);
    assert_eq!(mesure("mesure = sans-a1 ,sans-lecture").unwrap(), deux);
    // Les réglages actifs se disent un par ligne, dans le même ordre quel que soit celui du fichier.
    assert_eq!(deux.texte(), "sans-lecture\nsans-a1");
    assert_eq!(Mesure { sans_lecture: false, sans_a1: true }.texte(), "sans-a1");
    // Le profil intégré et une main brute n'ont aucun réglage.
    assert_eq!(lire("claude", integre("claude").unwrap(), None, None).unwrap().mesure(), Mesure::default());
    assert_eq!(Profil::faux(Vec::new(), Duration::from_secs(1)).mesure(), Mesure::default());
    // Une autre valeur, un réglage redit, une valeur vide, une clé redite : refusés comme toute valeur d'un profil.
    for ligne in [
        "mesure = autre",
        "mesure = sans-lecture, autre",
        "mesure = SANS-A1",
        "mesure = sans-a1 sans-lecture",
        "mesure = sans-a1, sans-a1",
        "mesure = sans-lecture,sans-a1,sans-lecture",
        "mesure =",
        "mesure = sans-a1,",
        "mesure = -sans-a1",
        "mesure = sans-a1\nmesure = sans-lecture",
    ] {
        let refus = mesure(ligne).expect_err(ligne);
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{ligne} : {refus}");
        assert!(refus.preuve.contains("essai.profil:"), "{ligne} : {refus}");
    }
    let refus = mesure("mesure = autre").unwrap_err();
    assert!(refus.preuve.contains("sans-lecture, sans-a1"), "{refus}");
    // Elle s'accepte avec le profil, comme toute clé : l'empreinte est celle du fichier.
    let texte = "sorte = claude\nmesure = sans-a1\n";
    assert_eq!(claude(&lu(texte).unwrap()).empreinte, empreinte(&avec_mur_demande(texte)));
}

#[test]
fn un_profil_faux_est_refuse_avec_sa_ligne() {
    let ailleurs = absolu("a").join("..").join("b").display().to_string();
    // Chaque faute en ligne 2, après un commentaire et avant la sorte, puis le bout de la règle de son cas.
    let cas = [
        ("couleur = bleu", "la clé est une de celles-ci : sorte, programme, modele"),
        ("budget = 10", "aucun budget"),
        ("sans egal", "une ligne vaut « clé = valeur »"),
        ("= claude", "la clé est une de"),
        ("modele =", "une valeur n'est ni vide ni en « - »"),
        ("modele = -x", "ni vide ni en « - »"),
        ("modele = --dangerously-skip-permissions", "ni vide ni en « - »"),
        ("reseau = a.test,,b.test", "ni vide ni en « - »"),
        ("reseau = a.test,", "ni vide ni en « - »"),
        ("outils = SendMessage, -x", "ni vide ni en « - »"),
        ("sorte = hermes", "la sorte d'une main est claude"),
        ("programme =", "ni vide ni en « - »"),
        ("programme = bin/claude", "un programme est un nom nu ou un chemin absolu"),
        ("programme = ./claude", "un nom nu ou un chemin absolu"),
        ("delai_main = 0", "un délai est un entier positif de secondes"),
        ("delai_main = 1.5", "un entier positif"),
        ("delai_preuve = +5", "un entier positif"),
        ("delai_preuve = 99999999999999999999", "un entier positif"),
        ("delai_preuve = 5 s", "un entier positif"),
        ("plafond = 0", "un plafond est un nombre positif de dollars"),
        ("plafond = 0.00", "un plafond est un nombre positif de dollars"),
        ("plafond = -5", "ni vide ni en « - »"),
        ("plafond = +5", "un plafond est un nombre positif de dollars"),
        ("plafond = 5.", "un plafond est un nombre positif de dollars"),
        ("plafond = .5", "un plafond est un nombre positif de dollars"),
        ("plafond = 1.234", "deux décimales au plus"),
        ("plafond = 1e3", "un plafond est un nombre positif de dollars"),
        ("plafond = 20 dollars", "un plafond est un nombre positif de dollars"),
        ("plafond = 99999999999999999999", "un plafond est un nombre positif de dollars"),
        ("reseau = *.exemple.test", "un domaine s'écrit en [A-Za-z0-9.-], sans joker ni port"),
        ("reseau = exemple.test:443", "sans joker ni port"),
        ("reseau = exemple.test/chemin", "sans joker ni port"),
        ("modele = Sonnet", "un modèle s'écrit en [a-z0-9.-]"),
        ("modele = sonnet[1m]", "un modèle s'écrit en [a-z0-9.-]"),
        ("relance = claude_sonnet", "un modèle s'écrit en [a-z0-9.-]"),
        ("relance = opus plan", "un modèle s'écrit en [a-z0-9.-]"),
        ("decoupeur = Opus", "un modèle s'écrit en [a-z0-9.-]"),
        ("decoupeur = opus[1m]", "un modèle s'écrit en [a-z0-9.-]"),
        ("decoupeur = -x", "ni vide ni en « - »"),
        ("decoupeur =", "ni vide ni en « - »"),
        ("relecteur = Opus", "un modèle s'écrit en [a-z0-9.-]"),
        ("relecteur = opus[1m]", "un modèle s'écrit en [a-z0-9.-]"),
        ("relecteur = -x", "ni vide ni en « - »"),
        ("relecteur =", "ni vide ni en « - »"),
        ("lecture = relatif/x", "une lecture est un chemin absolu ou en ~/…"),
        ("lecture = ~/../x", "sans .. ni joker"),
        ("lecture = ~/*", "sans .. ni joker"),
        ("lecture = ~/{a,b}", "sans .. ni joker"),
        ("lecture = ~/[ab]", "sans .. ni joker"),
        ("lecture = ~/a?", "sans .. ni joker"),
        ("lecture = ~root/x", "une lecture est un chemin absolu ou en ~/…"),
        ("proteges = /etc/passwd", "est absolu"),
        ("proteges = C:/x", "est absolu"),
        ("proteges = Cargo.lock, ../x", "sort du dépôt par `..`"),
        ("proteges = src/../x", "sort du dépôt par `..`"),
        ("proteges = .git/", "est sous `.git/`"),
        ("proteges = a/.git/hooks", "est sous `.git/`"),
        ("proteges = .claude/settings.json", "est sous `.claude/`"),
        ("proteges = a\\b", "barre inverse"),
        ("proteges = src/*.rs", "porte « * »"),
        ("proteges = src/a?", "porte « ? »"),
        ("proteges = src/[ab]", "porte « [ »"),
        ("proteges = src/{a,b}", "un chemin protégé est relatif"),
        ("proteges = src/(a)", "porte « ( »"),
        ("proteges = a\"b", "porte « \" »"),
        ("proteges = Cargo.lock,,x", "ni vide ni en « - »"),
        ("proteges = -x", "ni vide ni en « - »"),
        ("proteges =", "ni vide ni en « - »"),
        ("outils = Bash", "un outil déclaré est mcp__<serveur> ou l'un de ceux-ci : CronCreate"),
        ("outils = sendmessage", "un outil déclaré est"),
        ("outils = mcp__", "un outil déclaré est"),
        ("outils = mcp__a.b", "un outil déclaré est"),
        ("outils = mcp__docs", "un serveur mcp__<serveur> ne se déclare qu'avec sa configuration, mcp"),
        (
            r#"mcp = {"serveurs":{}}"#,
            "une configuration MCP est un objet JSON sur la ligne, sans clé redite, qui porte l'objet mcpServers",
        ),
        (r#"mcp = {"mcpServers":1}"#, "une configuration MCP"),
        (r#"mcp = {"mcpServers":"docs"}"#, "une configuration MCP"),
        (r#"mcp = {"mcpServers":{}} x"#, "une configuration MCP"),
        ("mcp = docs.json", "une configuration MCP"),
        // Les noms des serveurs sont les clés de l'objet mcpServers, chacune une fois.
        (r#"mcp = {"mcpServers":[]}"#, "une configuration MCP"),
        (r#"mcp = {"mcpServers":[{"docs":{}}]}"#, "une configuration MCP"),
        (r#"mcp = {"mcpServers":{"docs":{"command":"a"},"docs":{"command":"b"}}}"#, "sans clé redite"),
        ("mur = fort", "le mur est exige ou demande"),
        ("mur = Exige", "le mur est exige ou demande"),
    ];
    let lecture_ailleurs = format!("lecture = {ailleurs}");
    let cas = cas.into_iter().chain([(lecture_ailleurs.as_str(), "sans .. ni joker")]);
    for (faute, regle) in cas {
        let texte = format!("# une main\n{faute}\nsorte = claude\n");
        let refus = lu(&texte).err().unwrap_or_else(|| panic!("{faute:?} devait être refusée"));
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{faute:?} : {refus}");
        let lieu = absolu("k").join("profils").join("essai.profil").display().to_string();
        assert!(refus.preuve.starts_with(&format!("{lieu}:2 : « {faute} » : ")), "{faute:?} : {refus}");
        assert!(refus.preuve.contains(regle), "{faute:?} devait être refusée par « {regle} » : {refus}");
        assert!(refus.correction.contains("ligne 2") && refus.correction.contains("lydite accepter essai"));
    }
    // Une clé redite est refusée à sa seconde ligne.
    let refus = lu("sorte = claude\nmodele = opus\nmodele = sonnet\n").err().unwrap();
    assert!(refus.preuve.contains(":3 : « modele = sonnet » : chaque clé s'écrit une fois"), "{refus}");
    let refus = lu("sorte = claude\nsorte = claude\n").err().unwrap();
    assert!(refus.preuve.contains(":2 : « sorte = claude » : chaque clé"), "{refus}");
    // Sans sorte : le refus 6010, qui nomme le fichier et dit la ligne à ajouter.
    for texte in ["", "\n", "# rien\n", "modele = opus\n"] {
        let refus = lu(texte).err().unwrap();
        assert_eq!(refus.code, Code::ProfilIncomplet, "{texte:?}");
        assert!(refus.preuve.ends_with("essai.profil ne dit pas sa sorte"), "{refus}");
        assert!(refus.correction.contains("« sorte = claude »"), "{refus}");
    }
    // Un BOM, des \r et des blancs autour : lus, et les valeurs n'en gardent rien.
    let profil = lu("\u{feff}sorte = claude\r\nmodele = opus\r\n\r\n  # fin\r\nreseau = a.test , b.test\r\n").unwrap();
    assert_eq!(claude(&profil).modele, "opus");
    assert_eq!(claude(&profil).reseau, ["a.test", "b.test"]);
    let bom = "\u{feff}sorte = claude\r\n";
    let empreint = hex(&sha256(avec_mur_demande(bom).as_bytes()));
    assert_eq!(claude(&lu(bom).unwrap()).empreinte, empreint, "le BOM est empreint");
    // Un fichier qui n'est pas de l'UTF-8, ou qui ne se lit pas : le refus 6008, qui nomme son chemin.
    let dossier = kraken("illisible");
    let utf16 = dossier.join("profils").join("utf16.profil");
    fs::write(&utf16, b"\xFF\xFEs\x00o\x00").unwrap();
    let refus = charger_dans("utf16", &dossier).err().unwrap();
    assert_eq!(refus.code, Code::ProfilIllisible);
    assert_eq!(refus.preuve, format!("{} : UTF-8 invalide après l'octet 0", utf16.display()));
    assert!(refus.correction.contains("lydite accepter utf16"), "{refus}");
    let coupe = dossier.join("profils").join("coupe.profil");
    fs::write(&coupe, b"sorte = claude\n\xC3\n").unwrap();
    assert!(charger_dans("coupe", &dossier).err().unwrap().preuve.ends_with("UTF-8 invalide après l'octet 15"));
    let repertoire = dossier.join("profils").join("dossier.profil");
    fs::create_dir(&repertoire).unwrap();
    let refus = charger_dans("dossier", &dossier).err().unwrap();
    assert_eq!(refus.code, Code::ProfilIllisible);
    let systeme = fs::read(&repertoire).unwrap_err().to_string();
    assert_eq!(refus.preuve, format!("lecture du profil {} : {systeme}", repertoire.display()));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn la_relance_prend_son_modele() {
    let profil = borne("sorte = claude\nmodele = opus\nrelance = haiku\n");
    let modele = |demande: &Demande| {
        let i = demande.argv.iter().position(|arg| arg == "--model").expect("--model s'écrit toujours");
        (demande.argv[i + 1].clone(), demande.declaration.as_ref().unwrap().modele.clone())
    };
    let (premiere, seconde) = (profil.demande(&absolu("bac"), None, false), profil.demande(&absolu("bac"), None, true));
    assert_eq!(modele(&premiere), ("opus".into(), "opus".into()));
    assert_eq!(modele(&seconde), ("haiku".into(), "haiku".into()));
    // Seul le modèle change d'une tentative à l'autre.
    let sans_modele =
        |demande: Demande| demande.argv.into_iter().filter(|arg| arg != "opus" && arg != "haiku").collect::<Vec<_>>();
    assert_eq!(sans_modele(premiere), sans_modele(seconde));
    // Sans relance écrite, la relance est sonnet, comme le modèle sans modele écrit.
    let defaut = borne("sorte = claude\nmodele = opus\n");
    assert_eq!(modele(&defaut.demande(&absolu("bac"), None, true)).0, "sonnet");
    assert_eq!(modele(&borne("sorte = claude\n").demande(&absolu("bac"), None, false)).0, "sonnet");
    // Pure : deux appels rendent la même demande.
    let verifier = Some("git status");
    assert_eq!(profil.demande(&absolu("bac"), verifier, true), profil.demande(&absolu("bac"), verifier, true));
}

#[test]
fn l_argv_d_une_main_qui_a_des_lecons_porte_le_fichier_de_consigne() {
    let profil = borne("sorte = claude\n");
    let bac = absolu("bac");
    let fichier = absolu("k").join("consignes").join("abc.txt");
    let sans = profil.demande(&bac, None, false);
    assert!(!sans.argv.iter().any(|arg| arg == "--append-system-prompt-file"), "{:?}", sans.argv);
    let avec = profil.demande_avec(&bac, None, false, Some(&fichier));
    let n = avec.argv.len();
    assert_eq!(avec.argv[n - 2..], ["--append-system-prompt-file".to_string(), fichier.to_string_lossy().into_owned()]);
    // Le reste de l'argv et la déclaration sont ceux d'une main sans leçon.
    assert_eq!(avec.argv[..n - 2], sans.argv[..]);
    assert_eq!(avec.declaration, sans.declaration);
    // Une main nue ne reçoit pas de leçon.
    let nue = Profil::faux(vec!["x".into()], Duration::from_secs(1));
    assert!(nue.nue() && !profil.nue());
    assert_eq!(nue.demande_avec(&bac, None, false, Some(&fichier)).argv, ["x"]);
}

#[test]
fn le_decoupeur_a_son_modele_opus_par_defaut() {
    // Sans la clé, opus, profil intégré compris ; avec, la valeur lue, dans le domaine de `modele`.
    assert_eq!(lu("sorte = claude\n").unwrap().decoupeur(), Some("opus"));
    let absent = env::temp_dir().join(format!("lydite-test-profil-decoupeur-{}", lot()));
    assert_eq!(charger_dans("claude", &absent).unwrap().decoupeur(), Some("opus"));
    let profil = borne("sorte = claude\nmodele = haiku\nrelance = sonnet\ndecoupeur = claude-sonnet-5-5\n");
    assert_eq!(profil.decoupeur(), Some("claude-sonnet-5-5"));
    // Elle ne change ni `modele` ni `relance` ; le découpeur prend la sienne à ses deux tentatives, par la copie de
    // sa tâche, et le profil lui-même ne bouge pas.
    assert_eq!((claude(&profil).modele.as_str(), claude(&profil).relance.as_str()), ("haiku", "sonnet"));
    let decoupeur = profil.avec_modeles(profil.decoupeur(), profil.decoupeur());
    for relance in [false, true] {
        let declaration = decoupeur.demande(&absolu("bac"), None, relance).declaration.expect("une main bornée");
        assert_eq!(declaration.modele, "claude-sonnet-5-5", "relance {relance}");
    }
    // Une valeur hors du domaine est la ligne refusée (6009), comme une clé redite ; une main brute n'a pas de modèle.
    let refus = lu("sorte = claude\ndecoupeur = Opus\n").err().unwrap();
    assert_eq!(refus.code, Code::ProfilLigneRefusee);
    assert!(refus.preuve.contains(":2 : « decoupeur = Opus » : un modèle s'écrit en [a-z0-9.-]"), "{refus}");
    let refus = lu("sorte = claude\ndecoupeur = opus\ndecoupeur = haiku\n").err().unwrap();
    assert!(refus.preuve.contains(":3 : « decoupeur = haiku » : chaque clé s'écrit une fois"), "{refus}");
    assert_eq!(Profil::faux(vec!["x".into()], Duration::from_secs(1)).decoupeur(), None);
}

#[test]
fn le_relecteur_a_son_modele_opus_par_defaut() {
    // Sans la clé, opus, profil intégré compris ; avec, la valeur lue, dans le domaine de `modele`.
    assert_eq!(lu("sorte = claude\n").unwrap().relecteur(), Some("opus"));
    let absent = env::temp_dir().join(format!("lydite-test-profil-relecteur-{}", lot()));
    assert_eq!(charger_dans("claude", &absent).unwrap().relecteur(), Some("opus"));
    let profil = borne("sorte = claude\nmodele = haiku\nrelance = sonnet\nrelecteur = claude-opus-5-5\n");
    assert_eq!(profil.relecteur(), Some("claude-opus-5-5"));
    // Elle ne change ni `modele`, ni `relance`, ni `decoupeur` ; le relecteur prend la sienne à ses deux tentatives,
    // par la copie de sa tâche, et le profil lui-même ne bouge pas.
    assert_eq!((claude(&profil).modele.as_str(), claude(&profil).relance.as_str()), ("haiku", "sonnet"));
    assert_eq!(profil.decoupeur(), Some("opus"));
    let relecteur = profil.avec_modeles(profil.relecteur(), profil.relecteur());
    for relance in [false, true] {
        let declaration = relecteur.demande(&absolu("bac"), None, relance).declaration.expect("une main bornée");
        assert_eq!(declaration.modele, "claude-opus-5-5", "relance {relance}");
    }
    // Une valeur hors du domaine est la ligne refusée (6009), comme une clé redite ; une main brute n'a pas de modèle.
    let refus = lu("sorte = claude\nrelecteur = Opus\n").err().unwrap();
    assert_eq!(refus.code, Code::ProfilLigneRefusee);
    assert!(refus.preuve.contains(":2 : « relecteur = Opus » : un modèle s'écrit en [a-z0-9.-]"), "{refus}");
    let refus = lu("sorte = claude\nrelecteur = opus\nrelecteur = haiku\n").err().unwrap();
    assert!(refus.preuve.contains(":3 : « relecteur = haiku » : chaque clé s'écrit une fois"), "{refus}");
    assert_eq!(Profil::faux(vec!["x".into()], Duration::from_secs(1)).relecteur(), None);
}

#[test]
fn la_relance_est_le_modele_de_la_main_qui_corrige() {
    // La clé `relance` : sonnet par défaut, la valeur lue sinon ; une main brute n'a pas de modèle.
    assert_eq!(lu("sorte = claude\n").unwrap().relance(), Some("sonnet"));
    assert_eq!(lu("sorte = claude\nrelance = haiku\n").unwrap().relance(), Some("haiku"));
    assert_eq!(Profil::faux(vec!["x".into()], Duration::from_secs(1)).relance(), None);
}

/// Les valeurs d'effort que Claude Code accepte, et les quatre clés qui les portent avec ce que chacune lit.
const VALEURS_D_EFFORT: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];
const CLES_D_EFFORT: [&str; 4] = ["effort", "effort_relance", "effort_decoupeur", "effort_relecteur"];

/// L'effort que le profil lit pour la clé `cle`.
fn effort_lu<'a>(profil: &'a Profil, cle: &str) -> Option<&'a str> {
    match cle {
        "effort" => claude(profil).effort.as_deref(),
        "effort_relance" => profil.effort_relance(),
        "effort_decoupeur" => profil.effort_decoupeur(),
        "effort_relecteur" => profil.effort_relecteur(),
        autre => panic!("{autre} n'est pas une clé d'effort"),
    }
}

/// Le drapeau `--effort` d'un argv : sa valeur, et ce qui le suit, `None` s'il manque. Il en part un seul au plus.
fn effort_de(argv: &[String]) -> Option<(&str, Option<&str>)> {
    assert!(argv.iter().filter(|arg| *arg == "--effort").count() <= 1, "{argv:?}");
    let i = argv.iter().position(|arg| arg == "--effort")?;
    Some((argv[i + 1].as_str(), argv.get(i + 2).map(String::as_str)))
}

#[test]
fn chaque_cle_d_effort_se_lit_et_se_refuse_hors_de_son_domaine() {
    for cle in CLES_D_EFFORT {
        // Sans la clé, rien n'est posé : le défaut de Claude Code.
        assert_eq!(effort_lu(&lu("sorte = claude\n").unwrap(), cle), None, "{cle}");
        // Chaque valeur du domaine se lit, et ne change que sa clé.
        for valeur in VALEURS_D_EFFORT {
            let profil = lu(&format!("sorte = claude\n{cle} = {valeur}\n")).unwrap_or_else(|refus| panic!("{refus}"));
            for autre in CLES_D_EFFORT {
                assert_eq!(effort_lu(&profil, autre), (autre == cle).then_some(valeur), "{cle} = {valeur} : {autre}");
            }
        }
        // Une autre valeur est la ligne refusée (6009), avec la règle de l'effort.
        for valeur in ["Low", "HIGH", "extreme", "none", "xhigh2", "high medium", "1", "auto"] {
            let refus = lu(&format!("sorte = claude\n{cle} = {valeur}\n")).err().unwrap_or_else(|| panic!("{valeur}"));
            assert_eq!(refus.code, Code::ProfilLigneRefusee, "{cle} = {valeur} : {refus}");
            let ligne = format!(":2 : « {cle} = {valeur} » : un effort vaut low, medium, high, xhigh ou max");
            assert!(refus.preuve.contains(&ligne), "{refus}");
        }
        // Vide, en `-`, redite : refusée comme toute clé.
        for ligne in [format!("{cle} ="), format!("{cle} = -high"), format!("{cle} = low\n{cle} = max")] {
            let refus = lu(&format!("sorte = claude\n{ligne}\n")).err().unwrap_or_else(|| panic!("{ligne}"));
            assert_eq!(refus.code, Code::ProfilLigneRefusee, "{ligne} : {refus}");
        }
        // Une clé du domaine de `CLES`.
        assert!(CLES.contains(&cle), "{cle}");
    }
    // Les quatre se lisent ensemble, chacune à sa valeur.
    let profil =
        lu("sorte = claude\neffort = low\neffort_relance = medium\neffort_decoupeur = xhigh\neffort_relecteur = max\n")
            .unwrap();
    let lus: Vec<_> = CLES_D_EFFORT.iter().map(|cle| effort_lu(&profil, cle)).collect();
    assert_eq!(lus, [Some("low"), Some("medium"), Some("xhigh"), Some("max")]);
    // Une main brute n'a pas d'effort.
    let brute = Profil::faux(vec!["x".into()], Duration::from_secs(1));
    assert_eq!((brute.effort_decoupeur(), brute.effort_relecteur(), brute.effort_relance()), (None, None, None));
}

#[test]
fn le_profil_integre_ne_pose_aucun_effort_et_garde_son_texte() {
    let integre = integre("claude").unwrap();
    assert!(!integre.contains("effort"), "{integre}");
    // Le texte, donc l'empreinte, est celui d'avant les clés d'effort.
    assert_eq!(empreinte(integre), "7f53bd77a8516da1e12af702da88b79f0caa1807316b0a5352e29b190b5eb3b5");
    let profil = lire("claude", integre, None, None).unwrap();
    assert!(CLES_D_EFFORT.iter().all(|cle| effort_lu(&profil, cle).is_none()));
}

#[test]
fn l_effort_d_une_cle_posee_part_apres_le_plafond_et_avant_les_lecons() {
    let tous = "sorte = claude\nplafond = 7.5\neffort = low\neffort_relance = high\neffort_decoupeur = xhigh\n\
                effort_relecteur = max\n";
    let profil = borne(tous);
    let sans = borne("sorte = claude\nplafond = 7.5\n");
    let lecons = absolu("lecons.md");
    // La main : `effort` à sa première tentative, `effort_relance` à sa relance, juste après le plafond et avant les
    // leçons, jamais avant `--model`.
    for (relance, effort) in [(false, "low"), (true, "high")] {
        let Demande { argv, declaration } = profil.demande_avec(&absolu("bac"), None, relance, Some(&lecons));
        let i = argv.iter().position(|arg| arg == "--max-budget-usd").expect("le plafond");
        let attendu = ["--max-budget-usd", "7.5", "--effort", effort, "--append-system-prompt-file"];
        assert_eq!(argv[i..i + 5], attendu, "relance {relance}");
        assert_eq!(declaration.unwrap().effort.as_deref(), Some(effort), "relance {relance}");
        // `--model` reste où il est sans la clé : des fausses claude le lisent à une position fixe.
        let nu = sans.demande_avec(&absolu("bac"), None, relance, Some(&lecons)).argv;
        let place = |argv: &[String]| argv.iter().position(|arg| arg == "--model").expect("--model");
        assert_eq!(place(&argv), place(&nu), "relance {relance}");
        assert_eq!(argv[..=place(&argv) + 1], nu[..=place(&nu) + 1], "relance {relance}");
    }
    // Sans clé, l'argv ne change pas : ni drapeau, ni effort déclaré.
    for relance in [false, true] {
        let Demande { argv, declaration } = sans.demande_avec(&absolu("bac"), None, relance, Some(&lecons));
        assert_eq!(effort_de(&argv), None, "{argv:?}");
        assert_eq!(declaration.unwrap().effort, None);
        let fin = ["--max-budget-usd", "7.5", "--append-system-prompt-file", &lecons.to_string_lossy()];
        assert_eq!(argv[argv.len() - 4..], fin, "relance {relance}");
    }
    // Le découpeur : `effort_decoupeur` à ses deux tentatives, et seulement lui.
    for relance in [false, true] {
        let Demande { argv, declaration } = profil.pour_le_decoupeur().demande(&absolu("bac"), None, relance);
        assert_eq!(effort_de(&argv), Some(("xhigh", None)), "relance {relance}");
        assert_eq!(declaration.unwrap().effort.as_deref(), Some("xhigh"));
    }
    // Le relecteur : `effort_relecteur` à ses deux tentatives.
    for relance in [false, true] {
        let argv = profil.pour_le_relecteur().demande(&absolu("bac"), None, relance).argv;
        assert_eq!(effort_de(&argv), Some(("max", None)), "relance {relance}");
    }
    // La main de correction de la relecture : `effort_relance` à ses deux tentatives.
    for relance in [false, true] {
        let argv = profil.pour_la_correction().demande(&absolu("bac"), None, relance).argv;
        assert_eq!(effort_de(&argv), Some(("high", None)), "relance {relance}");
    }
    // Le profil lui-même ne bouge pas : sa main garde `effort` puis `effort_relance`.
    assert_eq!(effort_de(&profil.demande(&absolu("bac"), None, false).argv), Some(("low", None)));
    assert_eq!(effort_de(&profil.demande(&absolu("bac"), None, true).argv), Some(("high", None)));
    // Sans la clé du rôle, le rôle part sans `--effort`, même si la main en a un, et son argv est celui du profil sans
    // clé aux modèles près.
    let main_seule = borne("sorte = claude\nplafond = 7.5\neffort = low\neffort_relance = high\n");
    for relance in [false, true] {
        for role in [main_seule.pour_le_decoupeur(), main_seule.pour_le_relecteur()] {
            let argv = role.demande(&absolu("bac"), None, relance).argv;
            assert_eq!(argv, sans.demande(&absolu("bac"), None, relance).argv, "relance {relance}");
        }
    }
    // La copie aux modèles d'une tâche garde l'effort.
    let copie = profil.avec_modeles(Some("opus"), Some("haiku"));
    assert_eq!(effort_de(&copie.demande(&absolu("bac"), None, false).argv), Some(("low", None)));
    assert_eq!(effort_de(&copie.demande(&absolu("bac"), None, true).argv), Some(("high", None)));
}

#[test]
fn lydite_profil_dit_l_effort_de_chaque_role_ou_le_defaut_de_claude_code() {
    let rangee = |profil: &Profil| profil.rangees().into_iter().find(|rangee| rangee.starts_with("effort: ")).unwrap();
    let defaut = "Claude Code's default";
    assert_eq!(
        rangee(&lu("sorte = claude\n").unwrap()),
        format!("effort: {defaut}; retry: {defaut}; splitter: {defaut}; reviewer: {defaut}")
    );
    let profil = lu("sorte = claude\neffort_relecteur = max\neffort = low\n").unwrap();
    assert_eq!(rangee(&profil), format!("effort: low; retry: {defaut}; splitter: {defaut}; reviewer: max"));
    // Après la rangée du modèle.
    let rangees = profil.rangees();
    let place = |debut: &str| rangees.iter().position(|rangee| rangee.starts_with(debut)).unwrap();
    assert_eq!(place("effort: "), place("model: ") + 1);
}

/// Un profil qui nomme un fournisseur : une API en https avec un port et un chemin, et la variable de sa clé.
const FOURNISSEUR: &str = "sorte = claude\napi = https://api.exemple.test:8443/anthropic\ncle = CLE_ESSAI\n";

/// Une clé qui n'est celle de personne, un leurre que rien ne doit écrire.
const LEURRE: &str = "leurre-de-la-cle-4f9a";

/// L'environnement fictif de ces tests : `CLE_ESSAI` y vaut `valeur`, et rien d'autre n'y est posé.
fn environnement(valeur: &str) -> impl Fn(&str) -> Option<OsString> {
    let valeur = valeur.to_string();
    move |nom| (nom == "CLE_ESSAI").then(|| OsString::from(&valeur))
}

/// Le profil de ce texte, borné, dont la clé est lue dans l'environnement fictif sous la valeur `valeur`.
fn fourni(texte: &str, valeur: &str) -> Profil {
    let mut profil = borne(texte);
    profil.fournir(environnement(valeur)).unwrap();
    profil
}

#[test]
fn un_profil_nomme_son_fournisseur_et_sa_main_attend_sa_cle() {
    let mut profil = lu(FOURNISSEUR).unwrap();
    let fournisseur = claude(&profil).fournisseur.as_ref().expect("le profil nomme un fournisseur");
    assert_eq!(fournisseur.api, "https://api.exemple.test:8443/anthropic");
    assert_eq!((fournisseur.hote.as_str(), fournisseur.cle.as_str()), ("api.exemple.test:8443", "CLE_ESSAI"));
    assert_eq!(fournisseur.valeur, None, "lire ne touche pas à l'environnement");
    assert_eq!((profil.variable_de_la_cle(), profil.fournisseur()), (Some("CLE_ESSAI"), None));
    // Borné mais sans sa clé, la main ne part pas : elle parlerait à l'URL du fournisseur avec les identifiants
    // de l'humain.
    let (config, personnel) = leurres();
    profil.borner(&absolu("k"), &absolu("depot").join(".git"), Some(&config), Some(&personnel)).unwrap();
    assert_eq!(profil.demande(&absolu("bac"), None, false), Demande { argv: Vec::new(), declaration: None });
    // Sa clé lue, elle part : l'URL et la clé vont à la main, l'hôte à sa déclaration, et une copie aux modèles
    // d'une tâche garde le fournisseur.
    profil.fournir(environnement(LEURRE)).unwrap();
    assert_eq!(
        profil.fournisseur(),
        Some(Fournisseur { api: "https://api.exemple.test:8443/anthropic", cle: LEURRE, variable: "CLE_ESSAI" })
    );
    let declaration = profil.demande(&absolu("bac"), None, false).declaration.expect("la main part");
    assert_eq!(declaration.fournisseur.as_deref(), Some("api.exemple.test:8443"));
    let copie = profil.avec_modeles(Some("haiku"), None);
    assert_eq!((copie.fournisseur(), copie.variable_de_la_cle()), (profil.fournisseur(), Some("CLE_ESSAI")));
    // Sa déclaration fait le `main.fournisseur` de la tentative, juste après son lancement.
    let note = declaration.fournisseur("v", 2, 1).expect("un fournisseur déclaré");
    let attendu = Evenement::Fournisseur {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        fournisseur: "api.exemple.test:8443".into(),
    };
    assert_eq!(note, attendu);
    // Sans fournisseur : rien à lire, rien à recevoir, rien à noter.
    let mut libre = borne("sorte = claude\n");
    assert_eq!(libre.fournir(|nom| panic!("rien à lire : {nom}")), Ok(()));
    assert_eq!((libre.fournisseur(), libre.variable_de_la_cle()), (None, None));
    let declaration = libre.demande(&absolu("bac"), None, false).declaration.expect("la main part");
    assert_eq!((declaration.fournisseur.clone(), declaration.fournisseur("v", 1, 1)), (None, None));
    let mut faux = Profil::faux(vec!["x".into()], Duration::from_secs(1));
    assert_eq!(faux.fournir(|nom| panic!("rien à lire : {nom}")), Ok(()));
    assert_eq!((faux.fournisseur(), faux.variable_de_la_cle()), (None, None));
}

#[test]
fn le_fournisseur_ne_change_ni_la_ligne_de_commande_ni_les_reglages_et_la_cle_n_y_est_pas() {
    // Le même profil, avec et sans fournisseur : mêmes drapeaux, mêmes réglages, mêmes règles, à chaque tentative ;
    // seuls l'empreinte (le texte du fichier) et l'hôte déclaré changent. La clé et l'URL ne sont dans aucun
    // argument de la main : elle les reçoit par son environnement.
    let sans = borne("sorte = claude\nmodele = opus\nreseau = exemple.test\n");
    let avec = fourni(&format!("{FOURNISSEUR}modele = opus\nreseau = exemple.test\n"), LEURRE);
    for (verifier, relance) in [(None, false), (Some("cargo test && git status"), true)] {
        let (sans, avec) =
            (sans.demande(&absolu("bac"), verifier, relance), avec.demande(&absolu("bac"), verifier, relance));
        assert_eq!(sans.argv, avec.argv, "{verifier:?} {relance}");
        for arg in &avec.argv {
            assert!(!arg.contains(LEURRE) && !arg.contains("api.exemple.test"), "{arg}");
        }
        let (sans, mut avec) = (sans.declaration.unwrap(), avec.declaration.unwrap());
        assert_eq!(avec.fournisseur.take().as_deref(), Some("api.exemple.test:8443"));
        avec.empreinte = sans.empreinte.clone();
        assert_eq!(avec, sans);
    }
}

#[test]
fn l_empreinte_porte_le_nom_de_la_variable_jamais_sa_valeur() {
    // L'empreinte est celle des octets du fichier : `api` et le nom de la variable y sont, la clé n'y est pas, et
    // deux clés différentes sous le même nom donnent la même empreinte, dans le profil comme dans sa déclaration.
    let attendue = hex(&sha256(avec_mur_demande(FOURNISSEUR).as_bytes()));
    for valeur in [LEURRE, "une-autre-cle"] {
        let profil = fourni(FOURNISSEUR, valeur);
        assert_eq!(claude(&profil).empreinte, attendue, "{valeur}");
        let declaration = profil.demande(&absolu("bac"), None, false).declaration.unwrap();
        assert_eq!(declaration.empreinte, attendue, "{valeur}");
    }
    // Un autre nom de variable, ou une autre URL, est un autre fichier, donc une autre empreinte, à reaccepter.
    for autre in [FOURNISSEUR.replace("CLE_ESSAI", "AUTRE_CLE"), FOURNISSEUR.replace("8443", "9443")] {
        assert_ne!(claude(&lu(&autre).unwrap()).empreinte, attendue, "{autre}");
    }
}

#[test]
fn un_fournisseur_mal_donne_est_le_refus_6027_qui_ne_reprend_pas_la_valeur() {
    let (api, cle) = ("api = https://api.exemple.test/anthropic", "cle = CLE_ESSAI");
    // Chaque faute en ligne 2, après un commentaire, avec la ligne juste de l'autre clé : la ligne fautive est
    // celle que le refus nomme, par sa clé, sans reprendre ce qu'elle disait (une clé collée dans `cle`, des
    // identifiants dans `api`).
    let fautes_de_api = [
        "api = http://hote-secret.exemple.test/x",
        "api = ftp://hote-secret.exemple.test/x",
        "api = hote-secret.exemple.test/x",
        "api = HTTPS://hote-secret.exemple.test/x",
        "api = https://:8443/x-secret",
        "api = https:///hote-secret",
        "api = https://utilisateur:mot-de-passe-secret@hote.exemple.test/x",
        "api = https://hote.exemple.test/x?cle=secret-requete",
        "api = https://hote.exemple.test/x#secret-fragment",
        "api = https://hote.exemple.test:/x",
        "api = https://hote.exemple.test:84a3/x",
        "api = https://hote.exemple.test:123456/x",
        "api = https://hote-secret exemple.test/x",
        "api = https://hote.exemple.test/un secret",
        "api = https://hôte-secret.exemple.test/x",
        "api = https://hote_secret.exemple.test/x",
        "api = https://[::1]:8443/secret",
    ];
    let fautes_de_cle = [
        "cle = cle_secrete_minuscule",
        "cle = CLE-SECRETE",
        "cle = CLE SECRETE",
        "cle = $CLE_SECRETE",
        "cle = sk-ant-api03-0123456789-secrete",
        "cle = CLÉ_SECRÈTE",
        "cle = CLE.SECRETE",
        "cle = %CLE_SECRETE%",
    ];
    let lieu = absolu("k").join("profils").join("essai.profil").display().to_string();
    let fautes = fautes_de_api.iter().map(|faute| (*faute, "api", cle));
    for (faute, nom, autre) in fautes.chain(fautes_de_cle.iter().map(|faute| (*faute, "cle", api))) {
        let refus = lu(&format!("# un fournisseur\n{faute}\n{autre}\nsorte = claude\n")).err().expect(faute);
        assert_eq!(refus.code, Code::Fournisseur, "{faute} : {refus}");
        assert!(refus.preuve.starts_with(&format!("{lieu}:2 : « {nom} » : ")), "{faute} : {refus}");
        assert!(refus.correction.contains("ligne 2") && refus.correction.contains("lydite accepter essai"), "{refus}");
        let valeur = faute.split_once('=').unwrap().1.trim();
        assert!(!refus.to_string().contains(valeur), "le refus reprend {valeur} : {refus}");
    }
    // Un hôte vide est refusé aussi. La règle de leur cas est dite en clair.
    let refus = lu("sorte = claude\napi = https://\ncle = X\n").err().unwrap();
    assert_eq!(refus.code, Code::Fournisseur);
    let refus = lu("sorte = claude\napi = http://hote.exemple.test\ncle = X\n").err().unwrap();
    assert!(refus.preuve.ends_with("ni http, ni identifiants, ni requête"), "{refus}");
    let refus = lu("sorte = claude\napi = https://hote.exemple.test\ncle = x\n").err().unwrap();
    assert!(refus.preuve.ends_with("en [A-Z0-9_], jamais la clé elle-même"), "{refus}");
    // Le reste de la grammaire d'une ligne est celui des autres clés : une valeur vide ou en « - » est la ligne
    // refusée (6009), et une clé redite aussi.
    for faute in ["api =", "cle =", "api = -x", "cle = -X"] {
        let refus = lu(&format!("sorte = claude\n{faute}\n")).err().expect(faute);
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{faute}");
    }
    let refus = lu(&format!("{FOURNISSEUR}cle = AUTRE\n")).err().unwrap();
    assert!(refus.preuve.contains(":4 : « cle = AUTRE » : chaque clé s'écrit une fois"), "{refus}");
}

#[test]
fn une_cle_du_fournisseur_seule_est_le_refus_6027() {
    let lieu = absolu("k").join("profils").join("essai.profil").display().to_string();
    let seules = [("api = https://api.exemple.test/anthropic", "api", "cle"), ("cle = CLE_ESSAI", "cle", "api")];
    for (ligne, cle, manque) in seules {
        let refus = lu(&format!("# un fournisseur\n{ligne}\nsorte = claude\n")).err().expect(ligne);
        assert_eq!(refus.code, Code::Fournisseur, "{ligne}");
        let dit = format!("{lieu}:2 : « {cle} » est seule : un fournisseur se donne par api et cle ensemble");
        assert_eq!(refus.preuve, dit);
        let ajoute = format!("Ajoute la ligne « {manque} = ");
        assert!(refus.correction.starts_with(&ajoute) && refus.correction.contains("ou retire la ligne 2"), "{refus}");
        assert!(refus.correction.ends_with("lydite accepter essai."), "{refus}");
    }
    // Les deux ensemble, dans l'ordre qu'on veut, et séparées d'autres lignes : le profil se lit.
    for texte in ["api = https://a.exemple.test\ncle = X\n", "cle = X\nmodele = haiku\napi = https://a.exemple.test\n"]
    {
        let profil = lu(&format!("sorte = claude\n{texte}")).unwrap();
        assert_eq!(profil.variable_de_la_cle(), Some("X"));
    }
}

#[test]
fn le_prix_se_lit_et_se_rend_en_micro_dollars() {
    let fournisseur = "api = https://a.exemple.test\ncle = X\n";
    let texte =
        format!("sorte = claude\n{fournisseur}prix = deepseek-flash 0.15 0.60, deepseek-v4-pro 0.44 0.870001\n");
    let profil = lu(&texte).unwrap();
    let prix = |entree, sortie, cache_lu, cache_ecrit| Some(Prix { entree, sortie, cache_lu, cache_ecrit });
    // Sans prix de cache, les deux caches se comptent au prix d'entrée.
    assert_eq!(profil.prix("deepseek-flash"), prix(150_000, 600_000, 150_000, 150_000));
    assert_eq!(profil.prix("deepseek-v4-pro"), prix(440_000, 870_001, 440_000, 440_000));
    assert_eq!(profil.prix("sonnet"), None);
    let entiers = lu(&format!("sorte = claude\n{fournisseur}prix = m 2 10\n")).unwrap();
    assert_eq!(entiers.prix("m"), prix(2_000_000, 10_000_000, 2_000_000, 2_000_000));
    let trois = lu(&format!("sorte = claude\n{fournisseur}prix = m 1 2 0.5\n")).unwrap();
    assert_eq!(trois.prix("m"), prix(1_000_000, 2_000_000, 500_000, 1_000_000), "le cache écrit vaut l'entrée");
    let quatre = lu(&format!("sorte = claude\n{fournisseur}prix = m 1 2 0.5 1.25\n")).unwrap();
    assert_eq!(quatre.prix("m"), prix(1_000_000, 2_000_000, 500_000, 1_250_000));
    let cinq = lu(&format!("sorte = claude\n{fournisseur}prix = m 1 2 0 0, n 3 4 5\n")).unwrap();
    assert_eq!(cinq.prix("m"), prix(1_000_000, 2_000_000, 0, 0));
    assert_eq!(cinq.prix("n"), prix(3_000_000, 4_000_000, 5_000_000, 3_000_000));
    // Le prix est une ligne du profil comme une autre : l'empreinte le couvre.
    let sans = lu(&format!("sorte = claude\n{fournisseur}")).unwrap();
    assert_ne!(claude(&profil).empreinte, claude(&sans).empreinte);
    assert_eq!(sans.prix("deepseek-flash"), None);
}

#[test]
fn un_prix_sans_api_ou_mal_forme_est_refuse_avec_sa_ligne() {
    let fournisseur = "api = https://a.exemple.test\ncle = X\n";
    let lieu = absolu("k").join("profils").join("essai.profil").display().to_string();
    let refus = lu("sorte = claude\nprix = m 1 2\n").err().unwrap();
    assert_eq!(refus.code, Code::Fournisseur);
    assert!(refus.preuve.starts_with(&format!("{lieu}:2 : « prix »")), "{refus}");
    for faute in [
        "prix = m",
        "prix = m 1",
        "prix = m 1 2 3 4 5",
        "prix = m 1 2 x",
        "prix = m 1 2 3 -4",
        "prix = m -1 2",
        "prix = m 1 -2",
        "prix = m +1 2",
        "prix = m 1. 2",
        "prix = m .5 2",
        "prix = m 0.1234567 2",
        "prix = m 1e3 2",
        "prix = m un 2",
        "prix = M 1 2",
        "prix = m 1 2,",
        "prix = m 1 2, n 3",
        "prix = m 1 2, m 3 4",
        "prix = m 99999999999999 2",
    ] {
        let refus = lu(&format!("sorte = claude\n{fournisseur}{faute}\n")).err().expect(faute);
        assert!(refus.preuve.contains(&format!(":4 : « {faute} »")), "{faute} : {refus}");
    }
    let refus = lu(&format!("sorte = claude\n{fournisseur}prix = m 1 2\nprix = m 1 2\n")).err().unwrap();
    assert!(refus.preuve.contains("chaque clé s'écrit une fois"), "{refus}");
}

#[test]
fn les_urls_d_api_et_les_noms_de_variable_ont_leur_grammaire() {
    // L'hôte, port compris, que le fil note.
    let justes = [
        ("https://api.deepseek.com/anthropic", "api.deepseek.com"),
        ("https://api.deepseek.com", "api.deepseek.com"),
        ("https://api.deepseek.com/", "api.deepseek.com"),
        ("https://localhost:8443", "localhost:8443"),
        ("https://127.0.0.1:8443/a/b-c_d.e~f", "127.0.0.1:8443"),
        ("https://API.Exemple.Test/x", "API.Exemple.Test"),
    ];
    for (api, hote) in justes {
        assert_eq!(hote_de(api), Some(hote), "{api}");
    }
    for faux in
        ["", "https:", "https://", "http://a.test", "a.test", "https://a.test:", "https://a:b:c/", "https://a.test?x"]
    {
        assert_eq!(hote_de(faux), None, "{faux:?}");
    }
    for juste in ["X", "CLE_ESSAI", "DEEPSEEK_API_KEY", "ANTHROPIC_AUTH_TOKEN", "K2", "_"] {
        assert!(nom_de_variable(juste), "{juste}");
    }
    for faux in ["", "x", "Cle", "CLE-ESSAI", "CLE ESSAI", "CLE.ESSAI", "É", "CLE=1"] {
        assert!(!nom_de_variable(faux), "{faux:?}");
    }
}

#[test]
fn la_cle_du_fournisseur_se_lit_au_lancement_ou_c_est_le_refus_6027() {
    // Absente, vide, blanche, ou qui n'est pas de l'UTF-8 : le refus 6027, dont la preuve nomme le profil et la
    // variable, jamais une valeur, et la main ne part pas.
    let lieu = absolu("k").join("profils").join("essai.profil").display().to_string();
    let cas = [
        (None, "n'est pas posée"),
        (Some(OsString::new()), "est vide"),
        (Some(OsString::from(" \t\r\n")), "est vide"),
        (Some(non_utf8().into_os_string()), "n'est pas de l'UTF-8"),
    ];
    for (valeur, motif) in cas {
        let mut profil = borne(FOURNISSEUR);
        let refus = profil
            .fournir(|nom| {
                assert_eq!(nom, "CLE_ESSAI", "elle ne lit que la variable que cle nomme");
                valeur.clone()
            })
            .unwrap_err();
        assert_eq!(refus.code, Code::Fournisseur, "{motif}");
        assert_eq!(refus.preuve, format!("{lieu} : la variable d'environnement CLE_ESSAI, que cle nomme, {motif}"));
        assert!(refus.correction.starts_with("Pose CLE_ESSAI dans l'environnement de Lydite"), "{refus}");
        assert!(refus.correction.contains("api.exemple.test:8443"), "{refus}");
        assert_eq!(profil.fournisseur(), None, "{motif}");
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new(), "la main ne part pas");
    }
    // Une clé posée la remplit ; relue, elle suit l'environnement du moment.
    let mut profil = borne(FOURNISSEUR);
    profil.fournir(environnement(LEURRE)).unwrap();
    assert_eq!(profil.fournisseur().map(|f| f.cle), Some(LEURRE));
    profil.fournir(environnement("une-autre-cle")).unwrap();
    assert_eq!(profil.fournisseur().map(|f| f.cle), Some("une-autre-cle"));
}

#[test]
fn une_copie_prend_les_modeles_de_sa_tache_et_garde_le_reste() {
    let profil = borne("sorte = claude\nmodele = opus\nrelance = haiku\nmur = exige\nreseau = exemple.test\n");
    // Le modèle de chaque tentative, première puis relance : dans l'argv, et dans la déclaration qui en tire le
    // constat et le `main.lancement`.
    let modeles = |profil: &Profil| {
        [false, true].map(|relance| {
            let Demande { argv, declaration } = profil.demande(&absolu("bac"), None, relance);
            let i = argv.iter().position(|arg| arg == "--model").expect("--model s'écrit toujours");
            assert_eq!(argv[i + 1], declaration.expect("une main Claude bornée").modele, "relance {relance}");
            argv[i + 1].clone()
        })
    };
    assert_eq!(modeles(&profil), ["opus", "haiku"]);
    // Les deux modèles de la tâche ; l'un des deux seulement, l'autre restant celui du profil ; aucun.
    assert_eq!(modeles(&profil.avec_modeles(Some("tache"), Some("reprise"))), ["tache", "reprise"]);
    assert_eq!(modeles(&profil.avec_modeles(Some("tache"), None)), ["tache", "haiku"]);
    assert_eq!(modeles(&profil.avec_modeles(None, Some("reprise"))), ["opus", "reprise"]);
    assert_eq!(modeles(&profil.avec_modeles(None, None)), ["opus", "haiku"]);
    assert_eq!(modeles(&profil), ["opus", "haiku"], "le profil lui-même ne bouge pas");
    // Seul le modèle diffère : remis, la demande est celle du profil, argv, réglages, mur et empreinte compris.
    let copie = profil.avec_modeles(Some("tache"), Some("reprise"));
    for (relance, modele) in [(false, "opus"), (true, "haiku")] {
        let mut demande = copie.demande(&absolu("bac"), Some("cargo test"), relance);
        let i = demande.argv.iter().position(|arg| arg == "--model").expect("--model s'écrit toujours");
        demande.argv[i + 1] = modele.into();
        demande.declaration.as_mut().expect("une main Claude bornée").modele = modele.into();
        assert_eq!(demande, profil.demande(&absolu("bac"), Some("cargo test"), relance), "relance {relance}");
    }
    assert_eq!((copie.delai_main(), copie.delai_preuve()), (profil.delai_main(), profil.delai_preuve()));
    // Une main que `borner` n'a pas bornée ne part pas, et sa copie non plus ; une main brute n'a pas de modèle, et sa
    // copie est la même.
    let non_borne = lu("sorte = claude\nmodele = opus\n").unwrap();
    let copie = non_borne.avec_modeles(Some("tache"), None);
    assert_eq!(copie.demande(&absolu("bac"), None, false), Demande { argv: Vec::new(), declaration: None });
    let faux = Profil::faux(vec!["fausse".into(), "--x".into()], Duration::from_secs(7));
    let copie = faux.avec_modeles(Some("tache"), Some("reprise"));
    for relance in [false, true] {
        assert_eq!(copie.demande(&absolu("bac"), None, relance), faux.demande(&absolu("bac"), None, relance));
    }
    assert_eq!((copie.delai_main(), copie.delai_preuve()), (faux.delai_main(), faux.delai_preuve()));
}

#[test]
fn le_plafond_se_lit_avec_son_defaut_et_part_sur_la_ligne_de_commande() {
    // Vingt dollars par défaut, au profil intégré comme à un profil qui ne le dit pas.
    assert_eq!(lire("essai", "sorte = claude\n", None, None).unwrap().plafond(), "20");
    // Une valeur écrite, sans zéro de trop : entière, à une décimale, à deux.
    for (ecrit, porte) in [("7.5", "7.5"), ("7.50", "7.5"), ("100", "100"), ("0.25", "0.25"), ("3.05", "3.05")] {
        let profil = lire("essai", &format!("sorte = claude\nplafond = {ecrit}\n"), None, None).unwrap();
        assert_eq!(profil.plafond(), porte, "{ecrit}");
    }
    // Chaque main part avec le plafond du profil, à la première tentative comme à la relance.
    let profil = borne("sorte = claude\nplafond = 7.5\n");
    for relance in [false, true] {
        let argv = profil.demande(&absolu("bac"), None, relance).argv;
        let i = argv.iter().position(|arg| arg == "--max-budget-usd").expect("le drapeau");
        assert_eq!(argv[i + 1], "7.5", "relance {relance}");
        assert_eq!(argv.iter().filter(|arg| *arg == "--max-budget-usd").count(), 1);
    }
    // La copie aux modèles de la tâche garde le plafond.
    let copie = profil.avec_modeles(Some("opus"), None);
    let argv = copie.demande(&absolu("bac"), None, false).argv;
    assert!(argv.ends_with(&["--max-budget-usd".into(), "7.5".into()]), "{argv:?}");
    // Une clé écrite deux fois est refusée comme les autres.
    assert!(lire("essai", "sorte = claude\nplafond = 5\nplafond = 6\n", None, None).is_err());
}

#[test]
fn l_argv_parle_en_stream_json() {
    let profil = borne("sorte = claude\n");
    let Demande { argv, declaration } = profil.demande(&absolu("bac"), None, false);
    let declaration = declaration.unwrap();
    let attendu = [
        "claude",
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--permission-mode",
        "acceptEdits",
        "--permission-prompts",
        "none",
        "--strict-mcp-config",
        "--setting-sources",
        "user",
        "--model",
        "sonnet",
        "--settings",
    ];
    assert_eq!(argv[..attendu.len()], attendu);
    assert_eq!(
        argv[attendu.len()..],
        [declaration.reglages.as_str(), "--max-budget-usd", "20"],
        "le JSON exact, en chaîne, puis le plafond de dépense, à la fin"
    );
    assert!(!argv.iter().any(|arg| arg == "--tools" || arg == "--mcp-config"), "{argv:?}");
    // La déclaration : le profil, son empreinte, le modèle, le mur demandé, sans mcp.
    assert_eq!((declaration.profil.as_str(), declaration.mur, declaration.mcp), ("essai", false, None));
    assert_eq!(declaration.empreinte, hex(&sha256(avec_mur_demande("sorte = claude\n").as_bytes())));
    // Le programme du profil en tête.
    let programme = absolu("outils").join("claude").display().to_string();
    let nomme = borne(&format!("sorte = claude\nprogramme = {programme}\n"));
    assert_eq!(nomme.demande(&absolu("bac"), None, false).argv[0], programme);
    // Une main Claude que borner n'a pas bornée ne part pas : rien à lancer, rien à déclarer.
    let non_borne = lu("sorte = claude\n").unwrap();
    assert_eq!(non_borne.demande(&absolu("bac"), None, false), Demande { argv: Vec::new(), declaration: None });
}

#[test]
fn la_declaration_dit_son_lancement() {
    let dossier = kraken("lancement");
    let bac = dossier.join("bac");
    fs::create_dir(&bac).unwrap();
    let texte = "sorte = claude\nmodele = opus\nrelance = haiku\nmur = exige\n";
    let exige = borne(texte);
    let lancement = |profil: &Profil, relance: bool, bac: &Path| {
        let declaration = profil.demande(bac, None, relance).declaration.unwrap();
        let tentative = if relance { 2 } else { 1 };
        (declaration.lancement("v", 3, tentative, bac), declaration)
    };
    // Le profil, son empreinte, le modèle de la tentative, le mur et le JSON de --settings : la déclaration, telle
    // quelle. Un bac sans réglages du dépôt : la chaîne vide.
    let (evenement, declaration) = lancement(&exige, false, &bac);
    let attendu = Evenement::Lancement {
        vague: "v".into(),
        main: 3,
        tentative: 1,
        profil: "essai".into(),
        empreinte: hex(&sha256(texte.as_bytes())),
        modele: "opus".into(),
        mur: Mur::Exige,
        reglages: declaration.reglages.clone(),
        reglages_depot: String::new(),
    };
    assert_eq!(evenement.unwrap(), attendu);
    // La relance prend son modèle ; le mur demandé, non exigé, devient `Demande`.
    let (relance, _) = lancement(&exige, true, &bac);
    let Ok(Evenement::Lancement { modele, tentative, mur, .. }) = relance else { panic!("{relance:?}") };
    assert_eq!((modele.as_str(), tentative, mur), ("haiku", 2, Mur::Exige));
    let demande = borne("sorte = claude\nmur = demande\n");
    let Ok(Evenement::Lancement { mur, modele, .. }) = lancement(&demande, false, &bac).0 else { panic!() };
    assert_eq!((mur, modele.as_str()), (Mur::Demande, "sonnet"));
    // Les réglages du dépôt que le bac porte à cet instant, empreints : lus à chaque lancement.
    fs::create_dir(bac.join(".claude")).unwrap();
    fs::write(bac.join(".claude").join("settings.json"), "{}").unwrap();
    let Ok(Evenement::Lancement { reglages_depot, .. }) = lancement(&exige, false, &bac).0 else { panic!() };
    assert_eq!(reglages_depot, format!(".claude/settings.json {}", hex(&sha256(b"{}"))));
    // La note est lisible par le fil : une ligne bien formée, sans écart, que le lecteur rend telle qu'écrite.
    let fil = crate::branchements::dossier::Fil::seance(&dossier, "humain");
    let evenement = lancement(&exige, false, &bac).0.unwrap();
    fil.noter(&evenement).unwrap();
    let lecture = crate::branchements::dossier::lire(&dossier).unwrap();
    assert!(lecture.ecarts.is_empty(), "{:?}", lecture.ecarts);
    assert_eq!(lecture.notes.iter().map(|note| &note.evenement).collect::<Vec<_>>(), [&evenement]);
    // Un fichier de réglages présent qui ne se lit pas (un dossier à sa place) : une erreur qui le nomme, et la main
    // ne part pas.
    fs::remove_file(bac.join(".claude").join("settings.json")).unwrap();
    fs::create_dir(bac.join(".claude").join("settings.json")).unwrap();
    let erreur = lancement(&exige, false, &bac).0.unwrap_err();
    assert!(erreur.to_string().contains(&bac.join(".claude").join("settings.json").display().to_string()));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn les_reglages_disent_le_mur_du_profil() {
    // Un dossier de Kraken et un git commun avec un espace et un accent, qu'une règle écrit tels quels ; un bac
    // avec guillemet et barre inverse, qui ne va qu'à allowRead. Chaque chemin, puis sa chaîne JSON écrite à la
    // main.
    let (dossier, dossier_json, regle) = if cfg!(windows) {
        (r"C:\Users\é t\.lydite", r#""C:\\Users\\é t\\.lydite""#, "//c/Users/é t/.lydite")
    } else {
        ("/home/é t/.lydite", r#""/home/é t/.lydite""#, "//home/é t/.lydite")
    };
    let (lecture, lecture_json) = if cfg!(windows) {
        (r"C:\Program Files\outil", r#""C:\\Program Files\\outil""#)
    } else {
        ("/opt/outil x", r#""/opt/outil x""#)
    };
    // Le bac est en règle (la règle `Edit` des leçons le dit) : ni guillemet ni barre inverse dans ses noms.
    let (bac, bac_json, regle_du_bac) = if cfg!(windows) {
        (r"C:\bac é 1", r#""C:\\bac é 1""#, "//c/bac é 1")
    } else {
        ("/bac é 1", r#""/bac é 1""#, "//bac é 1")
    };
    let (git_commun, git_json, regle_git) = if cfg!(windows) {
        (r"C:\dépôt x\.git", r#""C:\\dépôt x\\.git""#, "//c/dépôt x/.git")
    } else {
        ("/dépôt x/.git", r#""/dépôt x/.git""#, "//dépôt x/.git")
    };
    let texte = format!(
        "sorte = claude\nmur = exige\nreseau = docs.exemple.test, paquets.exemple.test\n\
         lecture = {lecture}, ~/.cargo\n"
    );
    let (config, personnel) = leurres();
    let mut exige = lu(&texte).unwrap();
    exige.borner(Path::new(dossier), Path::new(git_commun), Some(&config), Some(&personnel)).unwrap();
    let attendu = format!(
        concat!(
            r#"{{"sandbox":{{"enabled":true,"failIfUnavailable":true,"allowUnsandboxedCommands":false,"#,
            r#""filesystem":{{"denyRead":["~/",{dossier_json}],"#,
            r#""allowRead":[{bac_json},{git_json},{lecture_json},"~/.cargo"]}},"#,
            r#""network":{{"allowedDomains":["docs.exemple.test","paquets.exemple.test"],"#,
            r#""strictAllowlist":true}},"#,
            r#""credentials":{{"envVars":[{{"name":"ANTHROPIC_API_KEY","mode":"deny"}},"#,
            r#"{{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}}]}}}},"#,
            r#""permissions":{{"allow":["Skill","Workflow","ExitPlanMode","#,
            r#""WebFetch(domain:docs.exemple.test)","WebFetch(domain:paquets.exemple.test)"],{deny}}},"#,
            r#""disableAllHooks":true,"autoMemoryEnabled":false}}"#,
        ),
        dossier_json = dossier_json,
        bac_json = bac_json,
        git_json = git_json,
        lecture_json = lecture_json,
        deny = deny_sans_declare(regle, regle_git, regle_du_bac),
    );
    let Demande { argv, declaration } = exige.demande(Path::new(bac), None, false);
    let declaration = declaration.unwrap();
    assert_eq!(declaration.reglages, attendu);
    assert_eq!(argv[argv.len() - 3..], [attendu.as_str(), "--max-budget-usd", "20"]);
    assert!(declaration.mur, "le mur exigé");
    // Le même profil, le mur demandé : seul failIfUnavailable change.
    let mut demande = lu(&texte.replace("mur = exige", "mur = demande")).unwrap();
    demande.borner(Path::new(dossier), Path::new(git_commun), Some(&config), Some(&personnel)).unwrap();
    let declaration = demande.demande(Path::new(bac), None, false).declaration.unwrap();
    let attendu = attendu.replace(r#""failIfUnavailable":true"#, r#""failIfUnavailable":false"#);
    assert_eq!(declaration.reglages, attendu);
    assert!(!declaration.mur, "le mur demandé, non exigé");
    // Le JSON se relit : un objet, et ses quatre clés.
    let objet = json::lire(&attendu).unwrap();
    assert_eq!(objet.keys().collect::<Vec<_>>(), ["autoMemoryEnabled", "disableAllHooks", "permissions", "sandbox"]);
}

#[test]
fn les_reglages_de_srt_portent_toutes_leurs_cles_et_les_regles_de_la_main() {
    // Sous le mur exigé, le rejeu par srt prend les règles de fichiers et de réseau d'une main posée dans le même
    // worktree, écrit dans le worktree et dans son dossier temporaire, et porte toutes ses clés, même vides : srt
    // refuse un fichier incomplet.
    let mut exige = borne("sorte = claude\nmur = exige\nreseau = exemple.test\nlecture = ~/.cargo\n");
    let worktree = absolu("k").join("bacs").join("main-1");
    let temporaire = crate::branchements::bac::temporaire_de(&worktree).unwrap();
    let srt = exige.rejeu(&worktree, &temporaire).unwrap().expect("le mur est exigé");
    let chaine = |chemin: &Path| json::chaine(&chemin.to_string_lossy());
    let attendu = format!(
        concat!(
            r#"{{"filesystem":{{"denyRead":["~/",{k}],"allowRead":[{w},{git},"~/.cargo"],"#,
            r#""allowWrite":[{w},{t}],"denyWrite":[{git}]}},"#,
            r#""network":{{"allowedDomains":["exemple.test"],"deniedDomains":[]}}}}"#,
        ),
        k = chaine(&absolu("k")),
        w = chaine(&worktree),
        git = chaine(&absolu("depot").join(".git")),
        t = chaine(&temporaire),
    );
    assert_eq!(srt.reglages, attendu);
    // Les règles sont celles du `--settings` de la main, à la lettre.
    let champ = |json: &Json, chemin: &str| {
        chemin.split('.').try_fold(json, |json, cle| if let Json::Objet(o) = json { o.get(cle) } else { None }).cloned()
    };
    let main = json::document(&exige.demande(&worktree, None, false).declaration.unwrap().reglages).unwrap();
    let rejeu = json::document(&srt.reglages).unwrap();
    for (de_la_main, du_rejeu) in [
        ("sandbox.filesystem.denyRead", "filesystem.denyRead"),
        ("sandbox.filesystem.allowRead", "filesystem.allowRead"),
        ("sandbox.network.allowedDomains", "network.allowedDomains"),
    ] {
        assert_eq!(champ(&main, de_la_main), champ(&rejeu, du_rejeu), "{du_rejeu}");
    }
    // Le fichier de réglages est neuf, dans le dossier de Kraken, hors du worktree et de son dossier temporaire.
    assert_eq!(srt.fichier.parent(), Some(absolu("k").as_path()));
    let nom = srt.fichier.file_name().unwrap().to_string_lossy().into_owned();
    assert!(nom.starts_with("srt-") && nom.ends_with(".json"), "{nom}");
    assert_ne!(exige.rejeu(&worktree, &temporaire).unwrap().unwrap().fichier, srt.fichier, "un fichier par rejeu");
    assert_eq!(srt.temporaire, temporaire);
    // Le programme est srt, ou celui qu'on pose.
    assert_eq!(srt.programme, "srt");
    exige.avec_srt(OsString::from("faux-srt"));
    assert_eq!(exige.rejeu(&worktree, &temporaire).unwrap().unwrap().programme, "faux-srt");
    // Sous un mur demandé, pour une main brute : rien ne passe par srt. Un mur exigé non borné ne se rejoue pas.
    assert!(borne("sorte = claude\n").rejeu(&worktree, &temporaire).unwrap().is_none());
    assert!(Profil::faux(vec!["x".into()], DELAI_PREUVE).rejeu(&worktree, &temporaire).unwrap().is_none());
    let erreur = lu("sorte = claude\nmur = exige\n").unwrap().rejeu(&worktree, &temporaire).unwrap_err();
    assert!(erreur.contains("pas borné"), "{erreur}");
}

#[test]
fn les_regles_de_chemin_suivent_le_systeme() {
    // Sous Windows, la lettre de lecteur en minuscule et des barres obliques ; ailleurs, `//` devant le chemin.
    assert_eq!(regle_de_chemin(r"C:\Users\x\.lydite", true), Ok("//c/Users/x/.lydite".into()));
    assert_eq!(regle_de_chemin(r"d:/k\é t", true), Ok("//d/k/é t".into()));
    assert_eq!(regle_de_chemin(r"C:\", true), Ok("//c".into()));
    assert_eq!(regle_de_chemin("/home/x/.lydite", false), Ok("//home/x/.lydite".into()));
    assert_eq!(regle_de_chemin("/home//é t/", false), Ok("//home/é t".into()));
    // Ce qu'une règle ne sait pas écrire : sa raison, dont `borner` fait le refus 6023 qui nomme le chemin.
    let refuses = [
        (r"C:\Users\compte (perso)\.lydite", true),
        (r"C:\a)\k", true),
        (r"\\serveur\partage\lydite", true),
        (r"\\?\C:\lydite", true),
        ("C:lydite", true),
        ("/home/x/.lydite", true),
        ("1:/k", true),
        ("/a(b)/k", false),
        (r"/a\b/k", false),
        ("/a/[x]/k", false),
        ("/a/\"q\"/k", false),
        ("/a/{x}", false),
        ("/a/*", false),
        ("/a/?", false),
        ("relatif/k", false),
        (r"C:\k", false),
    ];
    for (chemin, windows) in refuses {
        let raison = regle_de_chemin(chemin, windows).unwrap_err();
        let lecteur = "sous Windows, il ne commence pas par une lettre de lecteur";
        let dite = raison == lecteur || raison == "il n'est pas absolu" || raison.starts_with("le nom « ");
        assert!(dite, "{chemin} : {raison}");
    }
    // borner suit le système qui joue le test : les règles du dossier de Kraken s'écrivent dans sa forme.
    let reglages = reglages(&borne("sorte = claude\n"), None);
    let edit = format!(
        r#""Edit({regle}/fil.jsonl)","Edit({regle}/profils/**)","Read({regle}/srt-*.json)","Edit({regle}/srt-*.json)""#,
        regle = regle_k()
    );
    assert!(reglages.contains(&edit), "{reglages}");
}

#[test]
fn les_regles_qui_ferment_le_dossier_de_lydite_portent_les_noms_du_coeur() {
    let regle = regle_k();
    let reglages = reglages(&borne("sorte = claude\n"), None);
    let attendues = [
        format!("Edit({regle}/{FIL})"),
        format!("Edit({regle}/{PROFILS}/**)"),
        format!("Read({regle}/{DEBUT_SRT}*.json)"),
        format!("Edit({regle}/{DEBUT_SRT}*.json)"),
    ];
    for attendue in attendues {
        assert!(reglages.contains(&format!("\"{attendue}\"")), "{attendue} : {reglages}");
    }
    // Les réglages de srt du rejeu portent le même début de nom, que la règle `srt-*.json` ferme.
    let exige = borne("sorte = claude\nmur = exige\n");
    let srt = exige.rejeu(&absolu("w"), &absolu("t")).unwrap().unwrap();
    let nom = srt.fichier.file_name().unwrap().to_string_lossy().into_owned();
    assert!(nom.starts_with(DEBUT_SRT), "{nom}");
    // Le mode des éditions est celui de la ligne de commande.
    let argv = borne("sorte = claude\n").demande(&absolu("w"), None, false).argv;
    let place = argv.iter().position(|mot| mot == "--permission-mode").unwrap();
    assert_eq!(argv[place + 1], crate::coeur::protocole::MODE_DES_EDITIONS);
}

#[test]
fn les_outils_ouvrent_le_bac_et_ferment_le_reste() {
    // Les permis d'office, puis WebFetch par domaine de reseau ; les dix-neuf sortants fermés, dans l'ordre (ce qui
    // réveille, ce qui parle, ce qui publie, ce qui demande, ce qui fait entrer du contenu extérieur), les deux
    // règles Edit du dossier de Kraken, les deux de ses réglages de srt, les deux règles du fichier de connexion de
    // chaque leurre, puis l'Edit du git commun.
    let ouvert = reglages(&borne("sorte = claude\nreseau = exemple.test, autre.test\n"), None);
    let permissions = format!(
        concat!(
            r#""permissions":{{"allow":["Skill","Workflow","ExitPlanMode","#,
            r#""WebFetch(domain:exemple.test)","WebFetch(domain:autre.test)"],{}}},"#,
        ),
        deny_sans_declare(regle_k(), regle_depot(), regle_bac())
    );
    assert!(ouvert.contains(&permissions), "{ouvert}");
    assert!(ouvert.contains(r#""allowedDomains":["exemple.test","autre.test"]"#), "{ouvert}");
    // Sans reseau, aucun WebFetch, et la liste des domaines est vide : le réseau est fermé.
    let ferme = reglages(&borne("sorte = claude\n"), None);
    let permissions = format!(
        r#""permissions":{{"allow":["Skill","Workflow","ExitPlanMode"],{}}},"#,
        deny_sans_declare(regle_k(), regle_depot(), regle_bac())
    );
    assert!(ferme.contains(&permissions), "{ferme}");
    assert!(ferme.contains(r#""allowedDomains":[],"strictAllowlist":true"#), "{ferme}");
    assert!(!ferme.contains("WebFetch"), "{ferme}");
}

#[test]
fn un_profil_declare_ses_sortants_et_ses_serveurs_mcp() {
    let mcp = r#"{"mcpServers":{"docs":{"command":"docs-mcp"}}}"#;
    let profil = borne(&format!("sorte = claude\noutils = ListAgents, SendMessage, mcp__docs\nmcp = {mcp}\n"));
    let Demande { argv, declaration } = profil.demande(&absolu("bac"), None, false);
    let declaration = declaration.unwrap();
    // --mcp-config <mcp> part juste après --strict-mcp-config, et la déclaration en dit les serveurs, par leur
    // nom : ceux que le constat attend à l'ouverture.
    let i = argv.iter().position(|arg| arg == "--strict-mcp-config").unwrap();
    assert_eq!(argv[i + 1..i + 3], ["--mcp-config", mcp]);
    assert_eq!(declaration.mcp, Some(vec!["docs".to_string()]));
    // SendMessage et ListAgents passent de deny à allow, dans l'ordre des sortants, puis mcp__docs.
    let permissions = concat!(
        r#""allow":["Skill","Workflow","ExitPlanMode","SendMessage","ListAgents","mcp__docs"],"#,
        r#""deny":["CronCreate","CronDelete","CronList","ScheduleWakeup","RemoteTrigger","PushNotification","#,
        r#""SendUserFile","#,
    );
    assert!(declaration.reglages.contains(permissions), "{}", declaration.reglages);
    // Sans mcp : ni --mcp-config, ni mcp dans la déclaration, et le sortant déclaré passe seul.
    let sans = borne("sorte = claude\noutils = AskUserQuestion\n").demande(&absolu("bac"), None, false);
    assert!(!sans.argv.iter().any(|arg| arg == "--mcp-config"), "{:?}", sans.argv);
    let declaration = sans.declaration.unwrap();
    assert_eq!(declaration.mcp, None);
    assert!(declaration.reglages.contains(r#""ExitPlanMode","AskUserQuestion"],"deny""#), "{}", declaration.reglages);
    assert!(declaration.reglages.contains(r#""ShareOnboardingGuide","WebSearch","Edit("#), "{}", declaration.reglages);
    // mcp sans mcp__ déclaré : --mcp-config part, et aucun de ses serveurs n'est permis d'office.
    let seul = borne(&format!("sorte = claude\nmcp = {mcp}\n")).demande(&absolu("bac"), None, false);
    assert!(seul.argv.iter().any(|arg| arg == "--mcp-config"));
    assert!(!seul.declaration.unwrap().reglages.contains("mcp__"));
    // Les noms sont les clés de mcpServers, triées, à chaque tentative ; un mcpServers vide n'en déclare aucun.
    let deux =
        r#"{"mcpServers":{"lore":{"command":"lore-mcp","env":{"A":"1"}},"docs":{"url":"https://exemple.test"}}}"#;
    for (config, noms) in [(deux, vec!["docs", "lore"]), (r#"{"mcpServers":{}}"#, vec![])] {
        let profil = borne(&format!("sorte = claude\nmcp = {config}\n"));
        for relance in [false, true] {
            let declaration = profil.demande(&absolu("bac"), None, relance).declaration.unwrap();
            assert_eq!(declaration.mcp, Some(noms.iter().map(|nom| nom.to_string()).collect()), "{config}");
        }
    }
}

#[test]
fn la_recherche_web_est_fermee_d_office_et_s_ouvre_par_outils() {
    // Fermée d'office : WebSearch n'est pas un permis, et son nom nu suit les autres sortants dans deny.
    let ferme = reglages(&borne("sorte = claude\n"), None);
    assert!(ferme.contains(r#""allow":["Skill","Workflow","ExitPlanMode"],"#), "{ferme}");
    assert!(ferme.contains(r#""AskUserQuestion","WebSearch","Edit("#), "{ferme}");
    // Ouverte par `outils = WebSearch` : elle passe de deny à allow, à sa place parmi les sortants déclarés.
    let ouvert = reglages(&borne("sorte = claude\nreseau = exemple.test\noutils = WebSearch, SendMessage\n"), None);
    let permis =
        r#""allow":["Skill","Workflow","ExitPlanMode","WebFetch(domain:exemple.test)","SendMessage","WebSearch"],"#;
    assert!(ouvert.contains(permis), "{ouvert}");
    assert!(ouvert.contains(r#""AskUserQuestion","Edit("#), "{ouvert}");
    assert_eq!(ouvert.matches("WebSearch").count(), 1, "{ouvert}");
}

/// Le bac fictif de `reglages` en règle, sur le système qui joue le test.
fn regle_bac() -> &'static str {
    if cfg!(windows) { "//c/bac" } else { "//bac" }
}

#[test]
fn la_cle_proteges_se_lit_et_sans_elle_rien_ne_change() {
    let profil = lu("sorte = claude\nproteges = Cargo.lock, tests/juge/ ,ci/echecs.txt\n").unwrap();
    assert_eq!(profil.proteges(), [".agents/skills/", "Cargo.lock", "tests/juge/", "ci/echecs.txt"]);
    // La clé ne s'écrit qu'une fois.
    let refus = lu("sorte = claude\nproteges = a\nproteges = b\n").err().unwrap();
    assert_eq!(refus.code, Code::ProfilLigneRefusee);
    // Sans elle, seules les leçons sont protégées, et les réglages ne portent que leur règle `Edit` du bac.
    let sans = borne("sorte = claude\n");
    assert_eq!(sans.proteges(), [".agents/skills/"]);
    let regles = reglages(&sans, None);
    assert_eq!(regles.matches(&format!("Edit({}/", regle_bac())).count(), 1, "{regles}");
    assert!(regles.contains(&format!("Edit({}/.agents/skills/**)", regle_bac())), "{regles}");
    assert_eq!(lu("sorte = claude\n").unwrap().avec_modeles(Some("opus"), None).proteges(), [".agents/skills/"]);
    // Une copie aux modèles d'une tâche garde les chemins protégés.
    assert_eq!(profil.avec_modeles(Some("opus"), None).proteges().len(), 4);
    // Un fichier voisin des leçons reste libre : seul `.agents/skills/` est protégé.
    assert!(!regles.contains(".agents/**") && !regles.contains(".agents/autre"), "{regles}");
}

#[test]
fn les_reglages_ferment_l_edition_d_un_fichier_et_d_un_dossier_proteges() {
    let profil = borne("sorte = claude\nproteges = Cargo.lock, tests/juge/, ci/echecs.txt\n");
    let reglages = reglages(&profil, None);
    let regle = regle_bac();
    // Les trois règles sont dans `deny`, après celles de `git commit`, un fichier tel quel, un dossier en `/**`.
    let attendu = format!(
        concat!(
            r#""PowerShell(git commit *)","Edit({regle}/.agents/skills/**)","#,
            r#""Edit({regle}/Cargo.lock)","Edit({regle}/tests/juge/**)","#,
            r#""Edit({regle}/ci/echecs.txt)"]"#,
        ),
        regle = regle
    );
    assert!(reglages.contains(&attendu), "{reglages}");
    assert!(!reglages.contains(r#"/juge/)"#) && !reglages.contains("juge//"), "{reglages}");
    // Les deux copies d'une relance disent la même chose.
    let relance = profil.demande(&absolu("bac"), None, true).declaration.unwrap().reglages;
    assert!(relance.contains(&attendu), "{relance}");
}

#[test]
fn le_reglage_de_tout_profil_refuse_git_commit_sous_les_deux_formes_de_prefixe() {
    let refus = concat!(
        r#""Bash(git commit:*)","Bash(git commit *)","PowerShell(git commit:*)","PowerShell(git commit *)""#,
        r#","Edit("#,
    );
    for texte in ["sorte = claude\n", "sorte = claude\nmur = exige\nreseau = exemple.test\n"] {
        for verifier in [None, Some("cargo test && cargo clippy")] {
            let reglages = reglages(&borne(texte), verifier);
            assert!(reglages.contains(refus), "{texte:?} {verifier:?} : {reglages}");
            // Dans `deny`, jamais dans `allow` : le prêt de la preuve ne les ouvre pas.
            let (allow, deny) = reglages.split_once(r#""deny":"#).unwrap();
            assert!(!allow.contains("git commit") && deny.contains("git commit"), "{reglages}");
        }
    }
}

#[test]
fn un_bac_sans_regle_ferme_le_depart_d_une_main_aux_chemins_proteges() {
    // Un chemin protégé ne se ferme que par une règle : un bac qu'une règle ne sait pas écrire ne part pas.
    let profil = borne("sorte = claude\nproteges = Cargo.lock\n");
    let demande = profil.demande(Path::new("relatif"), None, false);
    assert!(demande.argv.is_empty() && demande.declaration.is_none());
    // Les leçons sont toujours protégées : même sans la clé, un bac sans règle ne part pas ; un bac en règle, si.
    let libre = borne("sorte = claude\n");
    assert!(libre.demande(Path::new("relatif"), None, false).argv.is_empty());
    assert!(!libre.demande(&absolu("bac"), None, false).argv.is_empty());
}

#[test]
fn le_fichier_de_connexion_de_claude_est_ferme_aux_outils() {
    // Ce qui suit les gardes du dossier de Kraken dans deny, jusqu'à l'Edit du git commun, borné par ces dossiers de
    // connexion.
    let apres_les_gardes = |config: Option<&Path>, personnel: Option<&Path>| {
        let mut profil = lu("sorte = claude\n").unwrap();
        profil.borner(&absolu("k"), &absolu("depot").join(".git"), config, personnel).unwrap();
        let reglages = reglages(&profil, None);
        let garde = format!(r#""Edit({}/srt-*.json)""#, regle_k());
        let (_, fin) = reglages.split_once(&garde).unwrap_or_else(|| panic!("{reglages}"));
        let commun = format!(r#","Edit({}/**)","Bash(git commit:*)""#, regle_depot());
        fin.split_once(&commun).map(|(regles, _)| regles.to_string()).unwrap_or_else(|| panic!("{reglages}"))
    };
    // `Read` puis `Edit` du fichier de connexion, en règle écrite à la main, pour chaque dossier de connexion :
    // `CLAUDE_CONFIG_DIR` d'abord, puis le `.claude` du dossier personnel.
    let paire = |chemin: &str| {
        let regle = if cfg!(windows) { format!("//c/{chemin}") } else { format!("//{chemin}") };
        format!(r#","Read({regle}/.credentials.json)","Edit({regle}/.credentials.json)""#)
    };
    let (config, personnel) = leurres();
    let (de_config, du_personnel) = (paire("config"), paire("personnel/.claude"));
    // Le `.claude` d'un dossier personnel que la règle écrit se ferme, `CLAUDE_CONFIG_DIR` posé ou non, qu'il garde
    // un fichier de connexion ou non (celui des leurres n'existe pas).
    assert_eq!(apres_les_gardes(Some(&config), Some(&personnel)), format!("{de_config}{du_personnel}"));
    assert_eq!(apres_les_gardes(None, Some(&personnel)), du_personnel);
    assert_eq!(apres_les_gardes(Some(&config), None), de_config);
    assert_eq!(apres_les_gardes(None, None), "");
    // Deux chemins égaux, même écrits autrement, ne donnent qu'une paire.
    let autrement = PathBuf::from(if cfg!(windows) { "c:/personnel/.claude/" } else { "/personnel//.claude/" });
    for meme in [personnel.join(".claude"), autrement] {
        assert_eq!(apres_les_gardes(Some(&meme), Some(&personnel)), du_personnel, "{}", meme.display());
    }
}

/// Pose un fichier de connexion de Claude dans le `.claude` de ce dossier personnel : son contenu n'est pas du JSON,
/// car seule son existence se regarde.
fn connecter(personnel: &Path) {
    let claude = personnel.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    fs::write(claude.join(".credentials.json"), "pas du json, jamais lu").unwrap();
}

#[test]
fn claude_config_dir_pose_un_dossier_personnel_sans_regle_n_est_refuse_que_s_il_garde_sa_connexion() {
    let racine = racine("profil-connexion");
    let (git, config) = (absolu("depot").join(".git"), absolu("config"));
    // Des dossiers personnels réels, dont le `.claude` est vide : un chemin qu'une règle écrit, et deux qu'elle
    // n'écrit pas (une parenthèse).
    let (ecrit, refuse, autre) = (racine.join("personnel"), racine.join("compte (perso)"), racine.join("a (b)"));
    let borne = |config: Option<&Path>, personnel: &Path| {
        let mut profil = lu("sorte = claude\n").unwrap();
        let issue = profil.borner(&absolu("k"), &git, config, Some(personnel));
        (issue, profil)
    };
    let regle = |dossier: &Path| regle_de_chemin(&dossier.to_string_lossy(), cfg!(windows)).unwrap();
    let paire = |dossier: &Path| {
        let regle = regle(dossier);
        format!(r#","Read({regle}/.credentials.json)","Edit({regle}/.credentials.json)""#)
    };
    let de_config = paire(&config);
    // Sans fichier de connexion, `CLAUDE_CONFIG_DIR` posé : un dossier personnel que la règle écrit se ferme encore,
    // et un que la règle n'écrit pas n'a ni règle ni refus : la main part avec la seule paire de `CLAUDE_CONFIG_DIR`.
    let (issue, profil) = borne(Some(&config), &ecrit);
    assert_eq!(issue, Ok(()));
    let attendu = format!("{de_config}{}", paire(&ecrit.join(".claude")));
    assert!(reglages(&profil, None).contains(&attendu), "{}", reglages(&profil, None));
    let (issue, profil) = borne(Some(&config), &refuse);
    assert_eq!(issue, Ok(()));
    let json = reglages(&profil, None);
    assert!(json.contains(&de_config), "{json}");
    assert_eq!(json.matches(".credentials.json").count(), 2, "{json}");
    // Sans `CLAUDE_CONFIG_DIR`, rien ne change : le `.claude` du dossier personnel se ferme, ou s'il ne s'écrit pas
    // en règle, le refus 6023 dit de poser `CLAUDE_CONFIG_DIR`, d'y connecter claude et de retirer l'ancien fichier.
    let (issue, profil) = borne(None, &ecrit);
    assert_eq!(issue, Ok(()));
    assert!(reglages(&profil, None).contains(&paire(&ecrit.join(".claude"))));
    let (issue, profil) = borne(None, &refuse);
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::DossierCheminRefuse);
    for etape in ["Pose CLAUDE_CONFIG_DIR", "connecte claude", "retire l'ancien fichier de connexion"] {
        assert!(refus.correction.contains(etape), "{etape} : {refus}");
    }
    assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new(), "la main ne part pas");
    // Avec un fichier de connexion, `CLAUDE_CONFIG_DIR` posé : le refus 6023 d'un chemin qu'une règle ne sait pas
    // écrire est revenu, et sa correction dit de retirer le fichier ou de le déplacer, puisque la connexion vit dans
    // `CLAUDE_CONFIG_DIR`.
    connecter(&refuse);
    let (issue, profil) = borne(Some(&config), &refuse);
    let refus = issue.unwrap_err();
    assert_eq!(refus.code, Code::DossierCheminRefuse);
    let nomme =
        format!("le dossier de connexion de Claude (dossier personnel) {} : ", refuse.join(".claude").display());
    assert!(refus.preuve.starts_with(&nomme), "{refus}");
    assert!(refus.correction.contains("Retire du .claude du dossier personnel son fichier de connexion"), "{refus}");
    assert!(refus.correction.contains("ou déplace-le") && refus.correction.contains("CLAUDE_CONFIG_DIR"), "{refus}");
    assert!(!refus.correction.contains("Pose CLAUDE_CONFIG_DIR"), "{refus}");
    assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new(), "la main ne part pas");
    // Une lecture qui recouvre le `.claude` du dossier personnel reste refusée en 6009, fermé ou non.
    let mut profil = lu("sorte = claude\nlecture = ~/.claude\n").unwrap();
    let refus = profil.borner(&absolu("k"), &git, Some(&config), Some(&autre)).unwrap_err();
    assert_eq!(refus.code, Code::ProfilLigneRefusee, "{refus}");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn la_preuve_est_pretee_morceau_par_morceau() {
    let profil = borne("sorte = claude\n");
    let verifier = concat!(
        "cargo fmt --check && cargo test && cargo clippy --all-targets -- -D warnings",
        " && findstr /x bonjour bonjour.txt",
    );
    assert!(profil.prete(Some(verifier)));
    let pret = concat!(
        r#""ExitPlanMode","Bash(cargo fmt --check)","PowerShell(cargo fmt --check)","#,
        r#""Bash(cargo test)","PowerShell(cargo test)","#,
        r#""Bash(cargo clippy --all-targets -- -D warnings)","#,
        r#""PowerShell(cargo clippy --all-targets -- -D warnings)","#,
        r#""Bash(findstr /x bonjour bonjour.txt)","PowerShell(findstr /x bonjour bonjour.txt)"],"deny""#,
    );
    for relance in [false, true] {
        let reglages = profil.demande(&absolu("bac"), Some(verifier), relance).declaration.unwrap().reglages;
        assert!(reglages.contains(pret), "{reglages}");
    }
    // Un morceau seul, avec `=`, `.`, `_` et `/` ; puis sans commande, rien n'est prêté.
    let seul = reglages(&profil, Some("git -c core.quotepath=off status ./src_a"));
    let pret = concat!(
        r#""Bash(git -c core.quotepath=off status ./src_a)","#,
        r#""PowerShell(git -c core.quotepath=off status ./src_a)"]"#,
    );
    assert!(seul.contains(pret), "{seul}");
    assert!(!profil.prete(None));
    // Aucun `Bash(...)` permis sans commande prêtée : seuls les refus de `git commit` en portent.
    assert!(!reglages(&profil, None).contains(r#""allow":["Skill","Workflow","ExitPlanMode","Bash("#));
    assert!(!reglages(&profil, None).replace("Bash(git commit", "").contains("Bash("));
}

#[test]
fn sous_le_mur_demande_la_preuve_pretee_sort_du_bac_a_sable() {
    // Les réglages entiers, écrits à la main : le mur exigé ou non, ce qui suit allowUnsandboxedCommands, puis ce
    // qui finit allow.
    let [dossier, bac, git] = chemins_json();
    let forme = |mur_exige: bool, exclues: &str, pret: &str| {
        format!(
            concat!(
                r#"{{"sandbox":{{"enabled":true,"failIfUnavailable":{mur_exige},"allowUnsandboxedCommands":false,"#,
                r#"{exclues}"filesystem":{{"denyRead":["~/",{dossier}],"allowRead":[{bac},{git}]}},"#,
                r#""network":{{"allowedDomains":[],"strictAllowlist":true}},"#,
                r#""credentials":{{"envVars":[{{"name":"ANTHROPIC_API_KEY","mode":"deny"}},"#,
                r#"{{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}}]}}}},"#,
                r#""permissions":{{"allow":["Skill","Workflow","ExitPlanMode"{pret}],{deny}}},"#,
                r#""disableAllHooks":true,"autoMemoryEnabled":false}}"#,
            ),
            mur_exige = mur_exige,
            exclues = exclues,
            dossier = dossier,
            bac = bac,
            git = git,
            pret = pret,
            deny = deny_sans_declare(regle_k(), regle_depot(), regle_bac()),
        )
    };
    let verifier = Some("cargo test && findstr /x bonjour bonjour.txt");
    let pret = concat!(
        r#","Bash(cargo test)","PowerShell(cargo test)","#,
        r#""Bash(findstr /x bonjour bonjour.txt)","PowerShell(findstr /x bonjour bonjour.txt)""#,
    );
    let (exige, demande) = (borne("sorte = claude\nmur = exige\n"), borne("sorte = claude\n"));
    // Sous le mur exigé, la preuve prêtée reste dans le bac à sable : le JSON d'avant, octet pour octet, ses règles
    // au bout de allow. Sous le mur demandé sans rien de prêté, sans commande ou avec une qui ne se prête pas, de
    // même.
    assert_eq!(reglages(&exige, verifier), forme(true, "", pret));
    assert_eq!(reglages(&demande, None), forme(false, "", ""));
    assert_eq!(reglages(&demande, Some("echo $HOME")), forme(false, "", ""));
    // Sous le mur demandé, les morceaux prêtés sortent du bac à sable, juste après allowUnsandboxedCommands, qui
    // reste faux : les morceaux des règles, dans leur ordre, à chaque tentative.
    let exclues = r#""excludedCommands":["cargo test","findstr /x bonjour bonjour.txt"],"#;
    for relance in [false, true] {
        let declaration = demande.demande(&absolu("bac"), verifier, relance).declaration.unwrap();
        assert_eq!(declaration.reglages, forme(false, exclues, pret), "relance {relance}");
    }
}

#[test]
fn une_preuve_qui_preterait_plus_n_est_pas_pretee() {
    let profil = borne("sorte = claude\n");
    let sans_pret = |relance| profil.demande(&absolu("bac"), None, relance);
    // Une commande qui voudrait passer un réglage à true dans le JSON de --settings.
    let vrai = r#"x"],"allowUnsandboxedCommands":true,"y":["z"#;
    let cas = [
        vrai,
        "cargo test *",
        "cargo test(x)",
        "cargo test )",
        "cargo test:*",
        "cargo test, rm x",
        "cargo test \"x\"",
        r"cargo test \x",
        "cargo test & del x",
        "cargo test | tee x",
        "cargo test; rm x",
        "echo $HOME",
        "echo %PATH%",
        "cargo test > x",
        "cargo test\nrm x",
        "cargo test\r\nrm x",
        "cargo test\trm x",
        "Bash(*)",
        "",
        " ",
        "cargo test && ",
        " && cargo test",
        "cargo test &&  && x",
        "cargo test&&rm x",
        "cargo test && rm x; y",
    ];
    for verifier in cas {
        assert!(!profil.prete(Some(verifier)), "{verifier:?}");
        for relance in [false, true] {
            // Rien de prêté : la demande est celle d'une preuve que la main ne joue pas.
            assert_eq!(profil.demande(&absolu("bac"), Some(verifier), relance), sans_pret(relance), "{verifier:?}");
        }
    }
    // Rien n'en sort, sous le mur demandé : allowUnsandboxedCommands reste faux, et seul, et rien n'entre dans
    // excludedCommands.
    let reglages = reglages(&profil, Some(vrai));
    assert_eq!(reglages.matches("allowUnsandboxedCommands").count(), 1, "{reglages}");
    assert!(reglages.contains(r#""allowUnsandboxedCommands":false"#), "{reglages}");
    assert!(!reglages.contains("excludedCommands"), "{reglages}");
}

#[test]
fn le_programme_du_fichier_l_emporte_sur_kraken_claude() {
    // `LYDITE_CLAUDE` n'est que le programme d'un profil qui n'en nomme pas, nu ou en chemin absolu.
    let absolu = absolu("outils").join("claude").display().to_string();
    assert_eq!(lu_programme(&format!("sorte = claude\nprogramme = {absolu}\n"), Some("/opt/claude")), absolu);
    assert_eq!(lu_programme("sorte = claude\nprogramme = claude-essai\n", Some("/opt/claude")), "claude-essai");
    assert_eq!(lu_programme("sorte = claude\n", Some("/opt/claude")), "/opt/claude");
}

#[test]
fn l_empreinte_du_profil_integre_est_celle_de_son_texte_ecrite_a_la_main() {
    // Le SHA-256 du texte intégré, calculé hors du code : printf 'sorte = claude\nmodele = sonnet\nrelance = sonnet\n
    // mur = exige\ndelai_main = 3600\ndelai_preuve = 300\n' | sha256sum. Elle s'inscrit au fil, jamais gardée.
    let dossier = kraken("integre-empreinte");
    let profil = charger_dans("claude", &dossier).unwrap();
    let attendue = "7f53bd77a8516da1e12af702da88b79f0caa1807316b0a5352e29b190b5eb3b5";
    assert_eq!(claude(&profil).empreinte, attendue);
    assert_eq!(profil.garder(), Ok(()), "le profil intégré n'a rien à garder");
    assert!(!dossier.join("profils").join("claude.empreinte").exists());
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_lecture_qui_n_est_ni_absolue_ni_du_dossier_personnel_est_refusee() {
    // `~` seul, `~\` et un chemin relatif (avec ou sans lecteur) ne sont pas des chemins que `lecture` accepte ; la
    // ligne refusée est nommée par son numéro, avec la règle de son cas.
    for faute in ["~", r"~\x", "./x", "x", "C:x", "~root/x"] {
        let refus = lu(&format!("sorte = claude\nlecture = {faute}\n")).err().expect(faute);
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{faute}");
        assert!(refus.preuve.contains(":2 : « lecture = "), "{refus}");
        assert!(refus.preuve.contains("une lecture est un chemin absolu ou en ~/…"), "{refus}");
    }
}

#[test]
fn une_entree_lecture_qui_recouvre_le_dossier_de_kraken_est_refusee() {
    let (dossier, git, (config, personnel)) = (absolu("k").join("lydite"), absolu("depot").join(".git"), leurres());
    // Lui, un de ses ancêtres, la racine, un chemin sous lui : le refus 6009, qui nomme la ligne lecture.
    let cas = [
        (dossier.clone(), true),
        (absolu("k"), true),
        (absolu(""), true),
        (dossier.join("profils"), true),
        (dossier.join("bacs").join("main-1"), true),
        (absolu("k").join("autre"), false),
        (absolu("k").join("lyditex"), false),
        (absolu("ailleurs"), false),
    ];
    for (entree, recouvre) in cas {
        let ligne = format!("lecture = {}, ~/.cargo", entree.display());
        let mut profil = lu(&format!("sorte = claude\n# ce que les commandes lisent\n{ligne}\n")).unwrap();
        let issue = profil.borner(&dossier, &git, Some(&config), Some(&personnel));
        if !recouvre {
            assert_eq!(issue, Ok(()), "{}", entree.display());
            continue;
        }
        let refus = issue.unwrap_err();
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{}", entree.display());
        assert!(refus.preuve.contains(&format!(":3 : « {ligne} » : une lecture ne recouvre pas")), "{refus}");
        assert!(refus.preuve.ends_with(&format!("et {} le recouvre", entree.display())), "{refus}");
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new(), "la main ne part pas");
    }
    // `~/` mène au dossier personnel que `borner` reçoit, que Claude Code lit aussi pour `~`.
    let sous = personnel.join(".lydite-essai");
    for (entree, recouvre) in
        [("~/", true), ("~/.lydite-essai", true), ("~/.lydite-essai/profils", true), ("~/.cargo", false)]
    {
        let mut profil = lu(&format!("sorte = claude\nlecture = {entree}\n")).unwrap();
        let issue = profil.borner(&sous, &git, Some(&config), Some(&personnel));
        assert_eq!(
            issue.map_err(|refus| refus.code),
            if recouvre { Err(Code::ProfilLigneRefusee) } else { Ok(()) },
            "{entree}"
        );
    }
}

#[test]
fn un_dossier_de_kraken_au_chemin_refuse_est_refuse() {
    let git = absolu("depot").join(".git");
    let mut cas = vec![absolu("a(b)").join("lydite"), absolu("k").join("[x]"), absolu("k").join("{x}"), absolu("k*")];
    if cfg!(windows) {
        cas.extend([
            PathBuf::from(r"\\serveur\partage\lydite"),
            PathBuf::from(r"\\?\C:\lydite"),
            PathBuf::from("C:lydite"),
        ]);
    } else {
        cas.extend([PathBuf::from(r"/a\b/lydite"), PathBuf::from("/a\"b/lydite"), PathBuf::from("lydite")]);
    }
    cas.push(non_utf8());
    let (config, personnel) = leurres();
    for dossier in cas {
        let mut profil = lu("sorte = claude\n").unwrap();
        let refus = profil.borner(&dossier, &git, Some(&config), Some(&personnel)).unwrap_err();
        assert_eq!(refus.code, Code::DossierCheminRefuse, "{}", dossier.display());
        assert!(refus.preuve.starts_with(&format!("le dossier de Lydite {}", dossier.display())), "{refus}");
        assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
        // Rien n'est posé : la main ne part pas, et ses règles ne tombent pas en silence.
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new());
    }
    // Un git commun qu'une règle ne sait pas écrire : son Edit tomberait en silence, et le bac à sable de Claude Code
    // l'ouvrirait en écriture. Le refus 6023, qui le nomme ; rien n'est posé.
    for commun in [absolu("a(b)").join(".git"), absolu("a").join("[x]")] {
        let mut profil = lu("sorte = claude\n").unwrap();
        let refus = profil.borner(&absolu("k"), &commun, Some(&config), Some(&personnel)).unwrap_err();
        assert_eq!(refus.code, Code::DossierCheminRefuse, "{}", commun.display());
        let nomme = format!("le git commun du dépôt visé {}", commun.display());
        assert!(refus.preuve.starts_with(&nomme), "{refus}");
        assert!(refus.correction.starts_with("Pose le dépôt visé"), "{refus}");
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new());
    }
    // Une main brute n'a rien à borner.
    let mut faux = Profil::faux(vec!["x".into()], Duration::from_secs(1));
    assert_eq!(faux.borner(&absolu("a(b)"), &git, Some(Path::new("relatif")), Some(&absolu("b(c)"))), Ok(()));
}

#[test]
fn un_dossier_de_connexion_au_chemin_refuse_est_refuse() {
    let (git, (config, personnel)) = (absolu("depot").join(".git"), leurres());
    // Le refus de `borner`, sur le dossier de Kraken fictif : rien n'est posé, la main ne part pas, et ses règles ne
    // tombent pas en silence.
    let refus_de = |config: Option<&Path>, personnel: Option<&Path>| {
        let mut profil = lu("sorte = claude\n").unwrap();
        let refus = profil.borner(&absolu("k"), &git, config, personnel).unwrap_err();
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new());
        refus
    };
    // Un CLAUDE_CONFIG_DIR relatif, vide, qu'une règle ne sait pas écrire ou qui n'est pas de l'UTF-8 : le refus
    // 6023, qui le nomme et dit de le poser autrement.
    let configs = [PathBuf::from("config"), PathBuf::new(), absolu("c(1)"), absolu("c").join("[x]"), non_utf8()];
    for config in configs {
        let refus = refus_de(Some(&config), Some(&personnel));
        assert_eq!(refus.code, Code::DossierCheminRefuse, "{}", config.display());
        let nomme = format!("le dossier de connexion de Claude (CLAUDE_CONFIG_DIR) {}", config.display());
        assert!(refus.preuve.starts_with(&nomme), "{refus}");
        assert!(refus.correction.starts_with("Pose CLAUDE_CONFIG_DIR à un chemin absolu"), "{refus}");
    }
    // Le `.claude` d'un dossier personnel qu'une règle ne sait pas écrire, sans CLAUDE_CONFIG_DIR : la correction dit
    // de poser CLAUDE_CONFIG_DIR, d'y connecter claude, puis de retirer l'ancien fichier de connexion.
    let refuse = absolu("compte (perso)");
    let refus = refus_de(None, Some(&refuse));
    assert_eq!(refus.code, Code::DossierCheminRefuse);
    let nomme =
        format!("le dossier de connexion de Claude (dossier personnel) {} : ", refuse.join(".claude").display());
    assert!(refus.preuve.starts_with(&nomme), "{refus}");
    for etape in ["Pose CLAUDE_CONFIG_DIR", "connecte claude", "retire l'ancien fichier de connexion"] {
        assert!(refus.correction.contains(etape), "{etape} : {refus}");
    }
    // CLAUDE_CONFIG_DIR posé, ce dossier, qui n'a pas de fichier de connexion, n'est plus un refus (voir
    // `avec_claude_config_dir_le_dossier_personnel_ne_se_ferme_que_s_il_garde_un_fichier_de_connexion`).
    let mut profil = lu("sorte = claude\n").unwrap();
    assert_eq!(profil.borner(&absolu("k"), &git, Some(&config), Some(&refuse)), Ok(()));
    // Le dossier de Kraken se juge d'abord, avec sa correction.
    let mut profil = lu("sorte = claude\n").unwrap();
    let refus = profil.borner(&absolu("k(1)"), &git, Some(Path::new("config")), Some(&refuse)).unwrap_err();
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
}

#[test]
fn une_lecture_qui_recouvre_un_dossier_de_connexion_est_refusee() {
    let (dossier, git, (config, personnel)) = (absolu("k"), absolu("depot").join(".git"), leurres());
    let claude = personnel.join(".claude");
    // Un dossier de connexion, un de ses ancêtres ou un chemin sous lui, écrit en absolu ou par `~/` : le refus 6009,
    // qui nomme la ligne et le dossier recouvert, comme pour le dossier de Kraken. À côté, rien n'est recouvert.
    let cas = [
        (config.display().to_string(), Some(&config)),
        (config.join("x").display().to_string(), Some(&config)),
        (claude.display().to_string(), Some(&claude)),
        (personnel.display().to_string(), Some(&claude)),
        ("~/.claude".to_string(), Some(&claude)),
        ("~/.claude/skills".to_string(), Some(&claude)),
        (absolu("configx").display().to_string(), None),
        ("~/.claude-x".to_string(), None),
        ("~/.cargo".to_string(), None),
    ];
    for (entree, recouvert) in cas {
        let ligne = format!("lecture = {entree}");
        let mut profil = lu(&format!("sorte = claude\n{ligne}\n")).unwrap();
        let issue = profil.borner(&dossier, &git, Some(&config), Some(&personnel));
        let Some(recouvert) = recouvert else {
            assert_eq!(issue, Ok(()), "{entree}");
            continue;
        };
        let refus = issue.unwrap_err();
        assert_eq!(refus.code, Code::ProfilLigneRefusee, "{entree}");
        let faute = format!(":2 : « {ligne} » : une lecture ne recouvre pas le dossier de connexion de Claude (");
        assert!(refus.preuve.contains(&faute), "{refus}");
        assert!(refus.preuve.contains(&format!(" {} (ni lui, ", recouvert.display())), "{refus}");
        assert!(refus.preuve.ends_with(&format!("et {entree} le recouvre")), "{refus}");
        assert_eq!(profil.demande(&absolu("bac"), None, false).argv, Vec::<String>::new(), "la main ne part pas");
    }
}

/// Un dossier dont le chemin n'est pas de l'UTF-8 : un substitut seul sous Windows, l'octet 0xFF ailleurs.
fn non_utf8() -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(OsString::from_wide(&[u16::from(b'C'), u16::from(b':'), u16::from(b'\\'), 0xD800]))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(b"/k\xFF".to_vec()))
    }
}

#[test]
fn faux_ne_prete_ni_ne_declare_rien() {
    let argv = vec!["fausse".to_string(), "--x".into()];
    let mut faux = Profil::faux(argv.clone(), Duration::from_secs(7));
    let (config, personnel) = leurres();
    assert_eq!(faux.borner(&absolu("k"), &absolu("depot"), Some(&config), Some(&personnel)), Ok(()));
    for (verifier, relance) in [(None, false), (Some("git status"), false), (Some("git status"), true)] {
        assert_eq!(faux.demande(&absolu("bac"), verifier, relance), Demande { argv: argv.clone(), declaration: None });
        assert!(!faux.prete(verifier));
    }
    assert_eq!(faux.garder(), Ok(()));
    assert_eq!((faux.delai_main(), faux.delai_preuve()), (Duration::from_secs(3600), Duration::from_secs(7)));
}

#[test]
fn toute_main_ferme_la_cle_du_fournisseur_et_le_jeton_herite_aux_commandes() {
    // Les deux règles `deny` de `sandbox.credentials.envVars`, juste après `network`, sous un mur exigé comme sous un
    // mur demandé, avec ou sans fournisseur.
    let regles = concat!(
        r#""strictAllowlist":true},"credentials":{"envVars":[{"name":"ANTHROPIC_API_KEY","mode":"deny"},"#,
        r#"{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}]}},"permissions""#,
    );
    for mur in ["", "mur = exige\n"] {
        let sans = borne(&format!("sorte = claude\n{mur}"));
        let avec = fourni(&format!("{FOURNISSEUR}{mur}"), LEURRE);
        for profil in [sans, avec] {
            let reglages = reglages(&profil, None);
            assert!(reglages.contains(regles), "{mur:?} : {reglages}");
            assert!(!reglages.contains(LEURRE), "{reglages}");
        }
    }
}
