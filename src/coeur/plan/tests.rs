#[test]
fn le_plan_montre_echappe_ses_controles_sauf_lignes_et_tabulations() {
    let texte = "tache a\n\tfichiers : x\u{1b}[31m\r\n";
    assert_eq!(super::sans_controle_en_lignes(texte), "tache a\n\tfichiers : x\\u{1b}[31m\\r\n");
}

use super::*;
use std::thread;

/// Le texte d'un plan : ces lignes, chacune finie par `\n`.
fn texte(lignes: &[&str]) -> String {
    lignes.iter().map(|ligne| format!("{ligne}\n")).collect()
}

/// Lit un plan écrit en lignes.
fn lire(lignes: &[&str]) -> Result<Plan, Refus> {
    Plan::lire(texte(lignes).as_bytes())
}

/// L'en-tête que les tests posent devant leurs tâches : la première ligne qui suit est la 4e.
const ENTETE: [&str; 3] = ["plan 1", "objectif : Ajouter etat", "preuve : cargo test"];

/// Lit un plan : l'en-tête, puis ces lignes.
fn lire_apres_entete(suite: &[&str]) -> Result<Plan, Refus> {
    lire(&[&ENTETE[..], suite].concat())
}

/// Une tâche attendue.
fn tache(id: &str, depend: &[&str], preuve: Option<&str>, objectif: &str) -> Tache {
    Tache {
        id: id.into(),
        depend: depend.iter().map(|id| id.to_string()).collect(),
        preuve: preuve.map(str::to_string),
        objectif: objectif.into(),
        ..Tache::default()
    }
}

#[test]
fn un_plan_se_lit() {
    let plan = lire(&[
        "plan 1",
        "objectif : Ajouter la commande etat",
        "preuve : cargo test",
        "",
        "tache types",
        "    Écrire les types.",
        "",
        // Une dépendance peut nommer une tâche plus bas, et des blancs de toute sorte séparent les ids.
        "tache etat",
        "depend : types \t fil",
        "    Écrire etat.",
        "",
        "tache fil",
        "preuve : cargo test fil",
        "depend : types",
        "    Écrire le fil.",
        "    Sur deux lignes.",
    ])
    .unwrap();
    let attendu = Plan {
        objectif: "Ajouter la commande etat".into(),
        preuve: Some("cargo test".into()),
        taches: vec![
            tache("types", &[], None, "Écrire les types."),
            tache("etat", &["types", "fil"], None, "Écrire etat."),
            tache("fil", &["types"], Some("cargo test fil"), "Écrire le fil.\nSur deux lignes."),
        ],
    };
    assert_eq!(plan, attendu);
    // Les blancs autour du `:` ne comptent pas, ni l'ordre des clés de l'en-tête, ni une ligne de blancs ; une
    // `depend` vide n'en nomme aucune ; la valeur d'une clé est tout ce qui suit le premier `:`.
    let plan = lire(&[
        "plan 1",
        "preuve:   echo a:b  ",
        "  ",
        "objectif   :Ajouter etat",
        "tache A-b_9",
        "depend :",
        "\tx",
        "tache\tz  ",
        "    y",
    ])
    .unwrap();
    assert_eq!((plan.objectif.as_str(), plan.preuve.as_deref()), ("Ajouter etat", Some("echo a:b")));
    assert_eq!(plan.taches, [tache("A-b_9", &[], None, "x"), tache("z", &[], None, "y")]);
}

#[test]
fn l_objectif_garde_ses_lignes_sans_son_indentation() {
    let plan = lire_apres_entete(&[
        "tache a",
        "",
        "    Première ligne.",
        "      Plus indentée.",
        "\t",
        "",
        "    preuve : du texte, pas une clé.",
        "",
        "   ",
        "tache b",
        "\tTabulée.",
        "\t\tDeux tabulations.",
        "tache c",
        "  \tMélange.",
        "  \t  Plus.",
        "tache d",
        "  Espaces.",
        "\tTabulation.",
    ])
    .unwrap();
    let objectifs: Vec<&str> = plan.taches.iter().map(|tache| tache.objectif.as_str()).collect();
    let attendus = [
        // Les lignes vides d'avant et de fin sont ôtées, celles du milieu gardées, de blancs ou non.
        "Première ligne.\n  Plus indentée.\n\n\npreuve : du texte, pas une clé.",
        "Tabulée.\n\tDeux tabulations.",
        "Mélange.\n  Plus.",
        // Une tabulation n'est pas des espaces : sans indentation commune, rien n'est ôté.
        "  Espaces.\n\tTabulation.",
    ];
    assert_eq!(objectifs, attendus);
}

#[test]
fn un_plan_avec_bom_et_crlf_se_lit() {
    let lignes = [
        "plan 1",
        "objectif : o",
        "preuve : p",
        "",
        "tache a",
        "depend :",
        "    Faire a.",
        "",
        "tache b",
        "depend : a",
        "    Faire b.",
    ];
    let attendu = lire(&lignes).unwrap();
    assert_eq!(attendu.taches.len(), 2);
    let (lf, crlf) = (texte(&lignes), texte(&lignes).replace('\n', "\r\n"));
    let bom: &[u8] = b"\xEF\xBB\xBF";
    let cas = [
        ("CRLF", crlf.clone().into_bytes()),
        ("BOM", [bom, lf.as_bytes()].concat()),
        ("BOM et CRLF", [bom, crlf.as_bytes()].concat()),
        ("BOM et CRLF, sans fin de ligne", [bom, crlf.trim_end().as_bytes()].concat()),
    ];
    for (nom, octets) in cas {
        assert_eq!(Plan::lire(&octets).as_ref(), Ok(&attendu), "{nom}");
    }
    // Le BOM s'ôte une fois : un second est du texte, et la première ligne n'est plus `plan 1`.
    let refus = Plan::lire(&[bom, bom, b"plan 1\n"].concat()).unwrap_err();
    assert_eq!(refus.code, Code::PlanVersion);
    // Seul le `\r` final d'une ligne s'ôte : celui du milieu fait partie de la ligne.
    let refus = Plan::lire(b"plan 1\robjectif : o\r\n").unwrap_err();
    assert_eq!(refus.code, Code::PlanVersion);
    assert!(refus.preuve.starts_with(r"ligne 1 « plan 1\robjectif : o » : "), "{refus}");
}

#[test]
fn une_autre_version_est_refusee_avant_le_reste() {
    // Tout le reste est faux (un objectif blanc, un id en deux mots, une ligne perdue, une dépendance inconnue) :
    // la version se lit avant, et seule elle est refusée.
    let fautif = ["objectif :", "tache a b", "    perdue", "depend : zz", "blabla"];
    for premiere in ["plan 2", "plan 0", "plan", "plan 1 ", "plan  1", "Plan 1", "plan 1.0", "plan: 1", " plan 1", ""] {
        let octets = texte(&[&[premiere], &fautif[..]].concat());
        let refus = Plan::lire(octets.as_bytes()).unwrap_err();
        assert_eq!(refus.code, Code::PlanVersion, "{premiere:?}");
        let debut = format!("ligne 1 {} : ", cite(premiere));
        assert!(refus.preuve.starts_with(&debut), "la preuve nomme la ligne 1 et son texte : {refus}");
        assert!(refus.correction.contains("`plan 1`"), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Un fichier vide, ou qui commence par un en-tête sans version, n'a pas de première ligne `plan 1`.
    for octets in [&b""[..], b"\n", b"objectif : o\npreuve : p\n"] {
        assert_eq!(Plan::lire(octets).unwrap_err().code, Code::PlanVersion, "{octets:?}");
    }
}

#[test]
fn un_plan_en_utf16_est_refuse_et_le_dit() {
    // « plan 1\n » en UTF-16 avec sa marque : petit-boutiste (FF FE), ce qu'écrit `>` sous PowerShell 5.1, puis
    // gros-boutiste (FE FF). Puis la marque seule, et celle d'un UTF-32 petit-boutiste (FF FE 00 00).
    let petit: [u8; 16] = [0xFF, 0xFE, 0x70, 0, 0x6C, 0, 0x61, 0, 0x6E, 0, 0x20, 0, 0x31, 0, 0x0A, 0];
    let gros: [u8; 16] = [0xFE, 0xFF, 0, 0x70, 0, 0x6C, 0, 0x61, 0, 0x6E, 0, 0x20, 0, 0x31, 0, 0x0A];
    let cas: [(&[u8], &str); 5] = [
        (&petit, "FF FE"),
        (&gros, "FE FF"),
        (&[0xFF, 0xFE], "FF FE"),
        (&[0xFE, 0xFF], "FE FF"),
        (&[0xFF, 0xFE, 0, 0], "FF FE"),
    ];
    for (octets, marque) in cas {
        let refus = Plan::lire(octets).unwrap_err();
        assert_eq!(refus.code, Code::PlanEncodage, "{octets:?}");
        assert!(refus.preuve.contains(marque) && refus.preuve.contains("UTF-16"), "{refus}");
        let commande = "(Get-Content <fichier>) | Set-Content -Encoding utf8 <fichier>";
        assert!(refus.correction.contains(commande), "la correction réécrit le fichier : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
}

#[test]
fn un_octet_hors_utf8_est_refuse() {
    // Les octets, leur rang attendu (à partir de 1, BOM compris), la ligne de l'octet et sa valeur.
    let cas: [(&[u8], usize, usize, &str); 8] = [
        // Un é de Latin-1 : « plan 1\n » fait 7 octets, « objectif : caf » 14.
        (b"plan 1\nobjectif : caf\xE9\n", 22, 2, "0xE9"),
        (b"\xEF\xBB\xBFplan 1\nobjectif : caf\xE9\n", 25, 2, "0xE9"),
        // Un é en UTF-8 coupé au premier octet, en fin de fichier.
        (b"plan 1\n\xC3", 8, 2, "0xC3"),
        (b"plan 1\n\n\n\xFF", 10, 4, "0xFF"),
        // Un FF ou un FE qui ne forme pas une marque UTF-16, un code long, un substitut codé en UTF-8.
        (b"\xFFA", 1, 1, "0xFF"),
        (b"\xFE", 1, 1, "0xFE"),
        (b"plan 1\n\xC0\x80", 8, 2, "0xC0"),
        (b"plan 1\n\xED\xA0\x80", 8, 2, "0xED"),
    ];
    for (octets, rang, ligne, octet) in cas {
        let refus = Plan::lire(octets).unwrap_err();
        assert_eq!(refus.code, Code::PlanEncodage, "{octets:?}");
        let attendu = format!("l'octet de rang {rang} ({octet}), ligne {ligne}, n'est pas de l'UTF-8");
        assert_eq!(refus.preuve, attendu, "{octets:?}");
        assert!(refus.correction.contains("Set-Content -Encoding utf8"), "{refus}");
    }
}

#[test]
fn chaque_refus_nomme_sa_ligne_et_enseigne() {
    let (ligne, identifiant) = (Code::PlanLigne, Code::PlanIdentifiant);
    // Après l'en-tête (lignes 1 à 3) : la suite du plan, le code, la ligne fautive, un bout de la preuve.
    let cas: &[(&[&str], Code, usize, &str)] = &[
        // 6018 : une ligne de la colonne 0 qui n'est rien de connu, dans l'en-tête ou dans une tâche.
        (&["blabla"], ligne, 4, "ni vide, ni indentée"),
        (&["tache a", "    x", "blabla"], ligne, 6, "ni vide, ni indentée"),
        // Une clé inconnue, ou d'un autre endroit : `objectif` n'est pas d'une tâche, `depend` pas de l'en-tête.
        (&["auteur : moi"], ligne, 4, "n'est pas une clé de l'en-tête"),
        (&["tache a", "budget : 5", "    x"], ligne, 5, "n'est pas une clé d'une tâche"),
        (&["tache a", "objectif : x", "    x"], ligne, 5, "n'est pas une clé d'une tâche"),
        (&["depend : a", "tache a", "    x"], ligne, 4, "n'est pas une clé de l'en-tête"),
        (&["tache: a", "    x"], ligne, 4, "« tache » n'est pas une clé de l'en-tête"),
        // Une clé redite, dans l'en-tête ou dans une tâche.
        (&["objectif : encore"], ligne, 4, "déjà donnée dans l'en-tête"),
        (&["preuve : encore"], ligne, 4, "déjà donnée dans l'en-tête"),
        (&["tache a", "depend : b", "depend : b", "    x"], ligne, 6, "déjà donnée dans cette tâche"),
        (&["tache a", "preuve : x", "preuve : y", "    z"], ligne, 6, "déjà donnée dans cette tâche"),
        // Une clé après l'objectif de sa tâche.
        (&["tache a", "    x", "preuve : y"], ligne, 6, "vient après les lignes indentées"),
        // Une ligne indentée avant toute tâche.
        (&["    perdue", "tache a", "    x"], ligne, 4, "aucune ligne `tache <id>` ne la précède"),
        (&["\tperdue"], ligne, 4, "aucune ligne `tache <id>` ne la précède"),
        // 6019 : un id absent, en deux mots, hors de l'ASCII ou de la grammaire, ou redit.
        (&["tache"], identifiant, 4, "il manque l'id"),
        (&["tache  \t"], identifiant, 4, "il manque l'id"),
        (&["tache a b", "    x"], identifiant, 4, "l'id « a b » n'est pas un mot"),
        (&["tache été", "    x"], identifiant, 4, "l'id « été » n'est pas un mot"),
        (&["tache a.b", "    x"], identifiant, 4, "l'id « a.b » n'est pas un mot"),
        (
            &["tache a", "    x", "tache b", "    y", "tache a", "    z"],
            identifiant,
            8,
            "l'id « a » est déjà celui de la tâche de la ligne 4",
        ),
    ];
    for &(suite, code, numero, bout) in cas {
        let refus = lire_apres_entete(suite).unwrap_err();
        assert_eq!(refus.code, code, "{suite:?}");
        let fautive = [&ENTETE[..], suite].concat()[numero - 1];
        let debut = format!("ligne {numero} {} : ", cite(fautive));
        assert!(refus.preuve.starts_with(&debut), "la preuve nomme la ligne {numero} et son texte : {refus}");
        assert!(refus.preuve.contains(bout), "{suite:?} devait dire « {bout} » : {refus}");
        assert!(refus.correction.contains("`tache <id>`"), "la correction enseigne la forme : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Une ligne immense est citée coupée, une ligne qui piège un terminal l'est sans son contrôle.
    assert!(!cite(&"x".repeat(CITE_MAX)).contains('…') && cite(&"x".repeat(CITE_MAX + 1)).contains('…'));
    let immense = "x".repeat(10_000);
    let refus = lire_apres_entete(&[&immense]).unwrap_err();
    assert!(refus.preuve.len() < 500 && refus.preuve.contains("… »"), "{}", refus.preuve);
    let refus = lire_apres_entete(&["\x1b[31mrouge"]).unwrap_err();
    assert!(!refus.preuve.contains('\x1b') && refus.preuve.contains("{1b}[31mrouge"), "{refus}");
}

#[test]
fn un_cycle_est_refuse_et_nomme() {
    // Les tâches après l'en-tête, le cycle nommé, la ligne de la dépendance que le refus cite (celle de la première
    // tâche du cycle).
    let cas: &[(&[&str], &str, usize)] = &[
        (&["tache a", "depend : b", "    x", "tache b", "depend : a", "    y"], "a → b → a", 5),
        // Un cycle de trois, et une tâche qui y mène, placée avant : elle n'en fait pas partie.
        (
            &[
                "tache d",
                "depend : a",
                "    x",
                "tache a",
                "depend : b",
                "    x",
                "tache b",
                "depend : c",
                "    x",
                "tache c",
                "depend : a",
                "    x",
            ],
            "a → b → c → a",
            8,
        ),
        // Un cycle plus bas que la tâche qui y mène.
        (
            &["tache x", "depend : y", "    x", "tache y", "depend : z", "    x", "tache z", "depend : y", "    x"],
            "y → z → y",
            8,
        ),
        // La première dépendance (c) est sans cycle ; la seconde (b) reboucle.
        (&["tache a", "depend : c b", "    x", "tache b", "depend : a", "    x", "tache c", "    x"], "a → b → a", 5),
    ];
    for &(suite, cycle, numero) in cas {
        let refus = lire_apres_entete(suite).unwrap_err();
        assert_eq!(refus.code, Code::PlanDependance, "{suite:?}");
        let fautive = [&ENTETE[..], suite].concat()[numero - 1];
        let attendu = format!(
            "ligne {numero} {} : cycle de dépendances : {cycle} (chaque tâche dépend de la suivante)",
            cite(fautive)
        );
        assert_eq!(refus.preuve, attendu);
        assert!(refus.correction.contains("Casse le cycle"), "{refus}");
    }
    // Ni un losange, ni deux tâches qui partagent une dépendance, ni une dépendance plus bas ne sont des cycles.
    let plan = lire_apres_entete(&[
        "tache d",
        "depend : b c",
        "    x",
        "tache b",
        "depend : a",
        "    x",
        "tache c",
        "depend : a",
        "    x",
        "tache a",
        "    x",
    ])
    .unwrap();
    assert_eq!(plan.taches.len(), 4);
}

#[test]
fn une_dependance_inconnue_ou_a_soi_est_refusee() {
    // Les tâches après l'en-tête, la ligne fautive (la ligne `depend`), un bout de la preuve.
    let cas: &[(&[&str], usize, &str)] = &[
        (&["tache a", "depend : zz", "    x"], 5, "la dépendance « zz » n'est l'id d'aucune tâche du plan"),
        (&["tache a", "depend : b zz", "    x", "tache b", "    y"], 5, "la dépendance « zz »"),
        // Un id se lit à la lettre, majuscule comprise, et les ids ne se séparent que par des blancs.
        (&["tache a", "depend : A", "    x", "tache b", "    y"], 5, "la dépendance « A »"),
        (&["tache a", "depend : b,c", "    x", "tache b", "    y"], 5, "la dépendance « b,c »"),
        // À soi : seule, ou parmi d'autres.
        (&["tache a", "depend : a", "    x"], 5, "la tâche « a » dépend d'elle-même"),
        (&["tache b", "    y", "tache a", "depend : b a", "    x"], 7, "la tâche « a » dépend d'elle-même"),
    ];
    for &(suite, numero, bout) in cas {
        let refus = lire_apres_entete(suite).unwrap_err();
        assert_eq!(refus.code, Code::PlanDependance, "{suite:?}");
        let fautive = [&ENTETE[..], suite].concat()[numero - 1];
        let debut = format!("ligne {numero} {} : ", cite(fautive));
        assert!(refus.preuve.starts_with(&debut), "la preuve nomme la ligne {numero} et son texte : {refus}");
        assert!(refus.preuve.contains(bout), "{suite:?} devait dire « {bout} » : {refus}");
        assert!(refus.correction.contains("depend"), "{refus}");
    }
}

#[test]
fn une_preuve_ou_un_objectif_blanc_est_refuse() {
    // Le plan entier, et ce que la preuve dit : « ligne N » et le texte de la ligne quand une est visée, sinon ce
    // qui manque.
    let sans_objectif =
        |id: &str, numero: usize| format!("ligne {numero} « tache {id} » : la tâche « {id} » n'a pas d'objectif");
    let cas: &[(&[&str], String)] = &[
        // Une valeur blanche, dans l'en-tête ou dans une tâche.
        (
            &["plan 1", "objectif :", "preuve : p", "tache a", "    x"],
            "ligne 2 « objectif : » : la clé « objectif » est blanche".into(),
        ),
        (&["plan 1", "objectif : o", "preuve:  \t", "tache a", "    x"], "ligne 3 « preuve:  \\t »".into()),
        (
            &["plan 1", "objectif : o", "preuve : p", "tache a", "preuve :", "    x"],
            "ligne 5 « preuve : » : la clé « preuve » est blanche".into(),
        ),
        // Une clé qui manque à l'en-tête, ou les deux : l'objectif est dit le premier.
        (&["plan 1", "preuve : p", "tache a", "    x"], "l'en-tête n'a pas de ligne `objectif : …`".into()),
        (&["plan 1", "objectif : o", "tache a", "    x"], "l'en-tête n'a pas de ligne `preuve : …`".into()),
        (&["plan 1", "tache a", "    x"], "l'en-tête n'a pas de ligne `objectif : …`".into()),
        // Une tâche sans objectif : au milieu, avec des lignes de blancs seules, ou la dernière.
        (&["plan 1", "objectif : o", "preuve : p", "tache a", "depend : b", "tache b", "    y"], sans_objectif("a", 4)),
        (&["plan 1", "objectif : o", "preuve : p", "tache a", "  ", "\t", "tache b", "    y"], sans_objectif("a", 4)),
        (&["plan 1", "objectif : o", "preuve : p", "tache a", "    x", "tache b"], sans_objectif("b", 6)),
    ];
    for (lignes, bout) in cas {
        let refus = lire(lignes).unwrap_err();
        assert_eq!(refus.code, Code::PlanBlanc, "{lignes:?}");
        assert!(refus.preuve.contains(bout.as_str()), "{lignes:?} devait dire « {bout} » : {refus}");
        assert!(!refus.correction.trim().is_empty(), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
}

#[test]
fn un_plan_sans_tache_est_refuse() {
    for suite in [&[][..], &[""], &["", "  ", ""]] {
        let refus = lire_apres_entete(suite).unwrap_err();
        assert_eq!(refus.code, Code::PlanBlanc, "{suite:?}");
        assert_eq!(refus.preuve, "le plan ne contient aucune tâche");
        assert!(refus.correction.contains("`tache <id>`"), "{refus}");
    }
}

/// Le refus d'un plan de 10 000 tâches qui doit être refusé : lu, le test dit combien de tâches, sans les imprimer.
fn refuse(lecture: Result<Plan, Refus>) -> Refus {
    match lecture {
        Err(refus) => refus,
        Ok(plan) => panic!("le plan devait être refusé, et se lit en {} tâches", plan.taches.len()),
    }
}

#[test]
fn dix_mille_taches_se_lisent_et_pas_une_de_plus() {
    // Un plan de n tâches, chacune dépendant de la suivante `(k % n) + 1` si `anneau`, de la précédente sinon.
    let plan_de = |n: usize, anneau: bool| -> Vec<String> {
        let mut lignes: Vec<String> = ENTETE.iter().map(|ligne| ligne.to_string()).collect();
        for k in 1..=n {
            lignes.push(format!("tache t{k}"));
            match (anneau, k) {
                (true, _) => lignes.push(format!("depend : t{}", k % n + 1)),
                (false, 1) => {}
                (false, _) => lignes.push(format!("depend : t{}", k - 1)),
            }
            lignes.push("    Faire.".into());
        }
        lignes
    };
    // Sur une pile de 512 Kio : une lecture qui descendrait de tâche en tâche par récursion (10 000 appels)
    // l'épuiserait, et Kraken s'arrêterait net au lieu de refuser.
    let lire = |lignes: Vec<String>| {
        let lecture = move || Plan::lire(lignes.join("\n").as_bytes());
        thread::Builder::new().stack_size(512 * 1024).spawn(lecture).unwrap().join().unwrap()
    };
    let mut lignes = plan_de(NOMBRE_MAX, false);
    let plan = lire(lignes.clone()).unwrap();
    assert_eq!(plan.taches.len(), NOMBRE_MAX);
    assert_eq!(plan.taches[NOMBRE_MAX - 1].id, format!("t{NOMBRE_MAX}"));
    assert_eq!(plan.taches[NOMBRE_MAX - 1].depend, [format!("t{}", NOMBRE_MAX - 1)]);
    // La 10 001e est refusée en 6018, à sa ligne.
    lignes.push(format!("tache t{}", NOMBRE_MAX + 1));
    lignes.push("    Faire.".into());
    let numero = lignes.len() - 1;
    let refus = refuse(lire(lignes));
    assert_eq!(refus.code, Code::PlanLigne);
    let debut = format!("ligne {numero} « tache t{} » : c'est la tâche {}", NOMBRE_MAX + 1, NOMBRE_MAX + 1);
    assert!(refus.preuve.starts_with(&debut), "{refus}");
    assert!(refus.correction.contains("plusieurs plans"), "{refus}");
    // Un anneau de 10 000 tâches est un cycle, trouvé sans empiler d'appels non plus.
    let refus = refuse(lire(plan_de(NOMBRE_MAX, true)));
    assert_eq!(refus.code, Code::PlanDependance);
    assert!(refus.preuve.contains("cycle de dépendances : t1 → t2 → t3 → t4 →"), "{:.120}", refus.preuve);
}

/// Le refus d'une lecture qui ne panique pas est l'un des six du plan, en quatre lignes.
fn sans_panique(octets: &[u8]) {
    if let Err(refus) = Plan::lire(octets) {
        let du_plan = [
            Code::PlanVersion,
            Code::PlanEncodage,
            Code::PlanLigne,
            Code::PlanIdentifiant,
            Code::PlanDependance,
            Code::PlanBlanc,
        ];
        assert!(du_plan.contains(&refus.code), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
}

#[test]
fn aucun_octet_ne_fait_paniquer_la_lecture() {
    let plan = texte(&[
        "plan 1",
        "objectif : Écrire café",
        "preuve : cargo test -- --nocapture",
        "",
        "tache a",
        "preuve : echo a:b",
        "    Première ligne.",
        "",
        "      Plus loin.",
        "tache b",
        "depend : a   a",
        "\tFaire b.",
        "tache c",
        "depend : a b",
        "    Faire c.",
    ]);
    let bom: &[u8] = b"\xEF\xBB\xBF";
    let bases = [
        ("LF", plan.clone().into_bytes()),
        ("CRLF", plan.replace('\n', "\r\n").into_bytes()),
        ("BOM", [bom, plan.as_bytes()].concat()),
    ];
    for (nom, base) in bases {
        assert!(Plan::lire(&base).is_ok(), "{nom}");
        // Chaque préfixe, y compris ceux qui coupent un caractère en deux.
        for fin in 0..=base.len() {
            sans_panique(&base[..fin]);
        }
        // Chaque octet remplacé à son tour par ce qui trouble le plus la lecture.
        for rang in 0..base.len() {
            for remplacant in [0xFF, b'\r', b':', b'\t'] {
                let mut octets = base.clone();
                octets[rang] = remplacant;
                sans_panique(&octets);
            }
        }
    }
}

#[test]
fn des_copies_sans_dependance_ni_assemblage() {
    let plan = Plan::copies("Écrire bonjour.txt", Some("git status"), 3);
    assert_eq!(plan.objectif, "Écrire bonjour.txt");
    assert_eq!(plan.preuve, None, "des copies n'ont pas de preuve de vague : rien à assembler");
    let ids: Vec<&str> = plan.taches.iter().map(|tache| tache.id.as_str()).collect();
    assert_eq!(ids, ["1", "2", "3"]);
    for tache in &plan.taches {
        assert!(tache.depend.is_empty());
        assert_eq!(tache.preuve.as_deref(), Some("git status"));
        assert_eq!(tache.objectif, "Écrire bonjour.txt");
    }
    // Sans `--verifier`, aucune tâche n'a de preuve, et la vague n'en a pas non plus.
    let nues = Plan::copies("o", None, 2);
    assert!(nues.preuve.is_none() && nues.taches.iter().all(|tache| tache.preuve.is_none()));
    // De 1 à la borne du fil.
    assert_eq!(Plan::copies("o", None, 1).taches.len(), 1);
    assert_eq!(Plan::copies("o", None, NOMBRE_MAX).taches.len(), NOMBRE_MAX);
}

/// Lit un plan écrit en lignes, dans ce mode.
fn lire_en(lignes: &[&str], mode: Mode<'_>) -> Result<(Plan, Vec<Attente>), Refus> {
    Plan::lire_en(texte(lignes).as_bytes(), mode)
}

/// Le refus d'une lecture qui devait être refusée : lue, le test dit ce qu'elle a rendu, sans tout imprimer.
fn refuse_en(lecture: Result<(Plan, Vec<Attente>), Refus>) -> Refus {
    match lecture {
        Err(refus) => refus,
        Ok((plan, attentes)) => {
            panic!("le plan devait être refusé, et se lit en {} tâches, {} ajouts", plan.taches.len(), attentes.len())
        }
    }
}

/// La demande d'un découpage dont l'en-tête est [`ENTETE`], sur une base de ces chemins.
fn demande(chemins: &[String]) -> Demande<'_> {
    Demande { objectif: "Ajouter etat", preuve: "cargo test", chemins }
}

/// Les dépendances ajoutées, comme `lydite plan` les dit.
fn dites(attentes: &[Attente]) -> Vec<String> {
    attentes.iter().map(Attente::to_string).collect()
}

/// Un plan découpé d'une tâche, `a`, à ces `fichiers`, sous l'en-tête [`ENTETE`].
fn decoupe_d_une_tache(fichiers: &str) -> String {
    texte(&[&ENTETE[..], &["tache a", &format!("fichiers : {fichiers}"), "modele : haiku", "    x"]].concat())
}

/// Un générateur à graine fixe (xorshift) : les tests tirent des plans sans dépendre du hasard.
fn tirage(graine: &mut u64) -> u64 {
    *graine ^= *graine << 13;
    *graine ^= *graine >> 7;
    *graine ^= *graine << 17;
    *graine
}

#[test]
fn les_cles_de_m5_se_lisent_et_un_plan_de_m3b_n_en_a_aucune() {
    let lignes = [
        &ENTETE[..],
        &[
            "tache a",
            "modele : opus",
            "relance : claude-sonnet-5-5",
            "fichiers : src/plan.rs \t tests/",
            "critique : oui",
            "depend :",
            "    Écrire le plan.",
            "tache b",
            "critique:oui",
            "preuve : cargo test plan",
            "depend : a",
            "    Écrire b.",
        ],
    ]
    .concat();
    let (plan, attentes) = lire_en(&lignes, Mode::Main).unwrap();
    let [a, b] = &plan.taches[..] else { panic!("{plan:?}") };
    let cles = (a.modele.as_deref(), a.relance.as_deref(), a.critique);
    assert_eq!(cles, (Some("opus"), Some("claude-sonnet-5-5"), true));
    assert_eq!(a.fichiers.as_deref(), Some(&["src/plan.rs".to_string(), "tests/".to_string()][..]));
    let cles = (b.modele.as_deref(), b.relance.as_deref(), b.fichiers.as_deref(), b.critique);
    assert_eq!(cles, (None, None, None, true));
    assert_eq!((b.depend.as_slice(), b.preuve.as_deref()), (&["a".to_string()][..], Some("cargo test plan")));
    assert!(attentes.is_empty());
    // Un plan de M3b, sans ces clés : aucune n'est donnée, rien n'est ajouté, et `lire` le rend tel quel.
    let m3b = [&ENTETE[..], &["tache a", "    x", "tache b", "depend : a", "    y"]].concat();
    let (lu, attentes) = lire_en(&m3b, Mode::Main).unwrap();
    assert!(attentes.is_empty());
    let nue = |t: &Tache| t.modele.is_none() && t.relance.is_none() && t.fichiers.is_none() && !t.critique;
    assert!(lu.taches.iter().all(nue));
    assert_eq!(lire(&m3b), Ok(lu));
}

#[test]
fn une_cle_de_m5_hors_de_son_domaine_blanche_ou_redite_est_refusee() {
    let (ligne, blanc) = (Code::PlanLigne, Code::PlanBlanc);
    // Après l'en-tête (lignes 1 à 3) : la suite du plan, le code, la ligne fautive, un bout de la preuve.
    let cas: &[(&[&str], Code, usize, &str)] = &[
        (&["tache a", "modele : Opus", "    x"], ligne, 5, "le modèle « Opus » ne s'écrit pas en [a-z0-9.-]"),
        (&["tache a", "modele : sonnet[1m]", "    x"], ligne, 5, "le modèle « sonnet[1m] »"),
        (&["tache a", "modele : opus plan", "    x"], ligne, 5, "le modèle « opus plan »"),
        (&["tache a", "relance : -opus", "    x"], ligne, 5, "« -opus » ne s'écrit pas en [a-z0-9.-] sans `-` de tête"),
        (&["tache a", "relance : claude_sonnet", "    x"], ligne, 5, "le modèle « claude_sonnet »"),
        (
            &["tache a", "critique : non", "    x"],
            ligne,
            5,
            "la clé « critique » vaut « non », et sa seule valeur est `oui`",
        ),
        (&["tache a", "critique : Oui", "    x"], ligne, 5, "vaut « Oui »"),
        (&["tache a", "modele :", "    x"], blanc, 5, "la clé « modele » est blanche"),
        (&["tache a", "relance : \t", "    x"], blanc, 5, "la clé « relance » est blanche"),
        (&["tache a", "fichiers :", "    x"], blanc, 5, "la clé « fichiers » est blanche"),
        (&["tache a", "critique :  ", "    x"], blanc, 5, "la clé « critique » est blanche"),
        (
            &["tache a", "modele : opus", "modele : opus", "    x"],
            ligne,
            6,
            "« modele » est déjà donnée dans cette tâche",
        ),
        (&["tache a", "relance : opus", "relance : haiku", "    x"], ligne, 6, "déjà donnée dans cette tâche"),
        (&["tache a", "fichiers : a", "fichiers : b", "    x"], ligne, 6, "déjà donnée dans cette tâche"),
        (&["tache a", "critique : oui", "critique : oui", "    x"], ligne, 6, "déjà donnée dans cette tâche"),
        (&["tache a", "    x", "modele : opus"], ligne, 6, "vient après les lignes indentées"),
        (&["fichiers : src/", "tache a", "    x"], ligne, 4, "« fichiers » n'est pas une clé de l'en-tête"),
        (&["critique : oui", "tache a", "    x"], ligne, 4, "« critique » n'est pas une clé de l'en-tête"),
    ];
    for &(suite, code, numero, bout) in cas {
        let lignes = [&ENTETE[..], suite].concat();
        let refus = refuse_en(lire_en(&lignes, Mode::Main));
        assert_eq!(refus.code, code, "{suite:?}");
        let debut = format!("ligne {numero} {} : ", cite(lignes[numero - 1]));
        assert!(refus.preuve.starts_with(&debut), "la preuve nomme la ligne {numero} et son texte : {refus}");
        assert!(refus.preuve.contains(bout), "{suite:?} devait dire « {bout} » : {refus}");
        assert!(!refus.correction.trim().is_empty(), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Une clé inconnue d'une tâche nomme les six qu'une tâche connaît, et la correction aussi.
    let refus = refuse_en(lire_en(&[&ENTETE[..], &["tache a", "role : critique", "    x"]].concat(), Mode::Main));
    assert!(refus.preuve.ends_with("(`depend`, `preuve`, `modele`, `relance`, `fichiers` ou `critique`)"), "{refus}");
    assert!(refus.correction.contains("`modele`, `relance`, `fichiers` et `critique`"), "{refus}");
}

#[test]
fn un_chemin_hors_de_la_forme_de_git_est_refuse() {
    // Chaque chemin refusé, et ce que la preuve en dit. Il suit un chemin juste, sur la même ligne.
    let cas = [
        ("/etc/passwd", "est absolu"),
        ("/", "est absolu"),
        ("C:/Windows/x", "est absolu"),
        ("c:x", "est absolu"),
        (r"src\plan.rs", "s'écrit avec une barre inverse, et git sépare les noms par `/`"),
        ("..", "sort du dépôt par `..`"),
        ("../voisin/x", "sort du dépôt par `..`"),
        ("src/../../x", "sort du dépôt par `..`"),
        ("./src", "a un nom vide ou fait de points"),
        ("src/./x", "a un nom vide ou fait de points"),
        ("src//plan.rs", "a un nom vide ou fait de points"),
        ("src//", "a un nom vide ou fait de points"),
        ("...", "a un nom vide ou fait de points"),
        (".git", "est sous `.git/`"),
        (".git/config", "est sous `.git/`"),
        ("sous/.git/hooks/", "est sous `.git/`"),
        (".GIT/", "est sous `.git/`"),
        (".git./config", "est sous `.git/`"),
        (".claude", "est sous `.claude/`"),
        (".claude/settings.json", "est sous `.claude/`"),
        ("sous/.Claude/skills/", "est sous `.claude/`"),
        (".claude../x", "est sous `.claude/`"),
    ];
    for (chemin, bout) in cas {
        let fichiers = format!("fichiers : src/juste.rs {chemin}");
        for mode in [Mode::Main, Mode::Decoupe(None)] {
            let suite = ["tache a", fichiers.as_str(), "modele : haiku", "    x"];
            let refus = refuse_en(lire_en(&[&ENTETE[..], &suite].concat(), mode));
            assert_eq!(refus.code, Code::PlanChemin, "{chemin}");
            assert_eq!(refus.preuve, format!("ligne 5 {} : le chemin {} {bout}", cite(&fichiers), cite(chemin)));
            assert!(refus.correction.contains("relatifs à la racine du dépôt"), "{refus}");
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
    }
    // Ce qui passe : des dossiers, des noms qui commencent par `.git` ou `.claude` sans l'être, des points au milieu.
    let justes = ".github/workflows/ci.yml .gitignore .gitattributes .claude-x/ a.git/ x..y src/ Cargo.toml ~ a/b/c/";
    let (plan, _) =
        lire_en(&[&ENTETE[..], &["tache a", &format!("fichiers : {justes}"), "    x"]].concat(), Mode::Main).unwrap();
    assert_eq!(plan.taches[0].fichiers.as_ref().map(Vec::len), Some(justes.split_whitespace().count()));
}

#[test]
fn au_rejeu_un_chemin_porte_la_casse_que_git_lui_connait() {
    let base = ["src/main.rs", "src/plan.rs", "tests/contrat.rs", "README.md", "docs/Guide/intro.md"];
    let chemins: Vec<String> = base.map(String::from).into();
    let demande = demande(&chemins);
    // Chaque chemin écrit sous une autre casse que la base, et celui qu'elle connaît, que la correction donne : le
    // fichier, l'un de ses dossiers, ou un fichier neuf sous un dossier de la base.
    let cas = [
        ("Src/plan.rs", "src/plan.rs"),
        ("src/Plan.rs", "src/plan.rs"),
        ("SRC/", "src/"),
        ("readme.md", "README.md"),
        ("docs/guide/", "docs/Guide/"),
        ("DOCS/guide/neuf.md", "docs/Guide/neuf.md"),
        ("Tests/neuf.rs", "tests/neuf.rs"),
    ];
    for (ecrit, juste) in cas {
        let plan = decoupe_d_une_tache(&format!("src/main.rs {ecrit}"));
        let refus = Plan::lire_en(plan.as_bytes(), Mode::Decoupe(Some(&demande))).unwrap_err();
        assert_eq!(refus.code, Code::PlanChemin, "{ecrit}");
        let dit = format!("le chemin {} s'écrit {} dans la base", cite(ecrit), cite(juste));
        assert!(refus.preuve.starts_with("ligne 5 ") && refus.preuve.ends_with(&dit), "{refus}");
        assert!(refus.correction.starts_with(&format!("Écris-le {},", cite(juste))), "{refus}");
        // À la main, ou sans contexte, la casse ne se juge pas.
        for mode in [Mode::Main, Mode::Decoupe(None)] {
            assert!(Plan::lire_en(plan.as_bytes(), mode).is_ok(), "{ecrit}");
        }
    }
    // Sous la casse de la base, ou neuf : rien à redire.
    let justes = "src/plan.rs src/ README.md docs/Guide/ docs/Guide/neuf.md neuf/x.rs src/neuf.rs Neuf/";
    assert!(Plan::lire_en(decoupe_d_une_tache(justes).as_bytes(), Mode::Decoupe(Some(&demande))).is_ok());
}

#[test]
fn un_plan_pese_1_mo_au_plus() {
    // Une tâche dont l'objectif est une ligne immense, qui mène le plan à 1 Mo pile, puis à un octet de plus.
    let debut = texte(&[&ENTETE[..], &["tache a"]].concat()) + "    ";
    let de_taille = |taille: usize| format!("{debut}{}\n", "x".repeat(taille - debut.len() - 1));
    let pile = de_taille(TAILLE_MAX);
    assert_eq!(pile.len(), TAILLE_MAX);
    let (plan, _) = Plan::lire_en(pile.as_bytes(), Mode::Main).unwrap();
    assert_eq!(plan.taches[0].objectif.len(), TAILLE_MAX - debut.len() - 1);
    let lourd = de_taille(TAILLE_MAX + 1);
    for mode in [Mode::Main, Mode::Decoupe(None)] {
        let refus = Plan::lire_en(lourd.as_bytes(), mode).unwrap_err();
        assert_eq!(refus.code, Code::PlanBorne);
        assert_eq!(refus.preuve, format!("le plan dépasse 1 Mo : plus de {TAILLE_MAX} octets"));
        assert!(refus.correction.contains("sous 1 Mo"), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Le poids se juge avant tout le reste : au-delà d'1 Mo, ni l'encodage ni la version ne se lisent.
    let utf16 = [&[0xFF, 0xFE][..], &vec![0; TAILLE_MAX]].concat();
    assert_eq!(Plan::lire(&utf16).unwrap_err().code, Code::PlanBorne);
}

#[test]
fn un_plan_decoupe_a_12_taches_au_plus_chacune_avec_fichiers_et_modele() {
    // Un plan découpé de n tâches, chacune à ses fichiers et à son modèle.
    let decoupe = |n: usize| -> String {
        let mut lignes: Vec<String> = ENTETE.iter().map(|ligne| ligne.to_string()).collect();
        for k in 1..=n {
            lignes.extend([format!("tache t{k}"), format!("fichiers : t{k}/"), "modele : haiku".into()]);
            lignes.push("    Faire.".into());
        }
        lignes.iter().map(|ligne| format!("{ligne}\n")).collect()
    };
    let chemins = Vec::new();
    let demande = demande(&chemins);
    let decoupes = [Mode::Decoupe(None), Mode::Decoupe(Some(&demande))];
    for mode in decoupes {
        assert_eq!(Plan::lire_en(decoupe(12).as_bytes(), mode).unwrap().0.taches.len(), 12);
        // La 13e, à sa ligne : chaque tâche en tient 4, sous l'en-tête.
        let refus = Plan::lire_en(decoupe(13).as_bytes(), mode).unwrap_err();
        assert_eq!(refus.code, Code::PlanBorne);
        let debut = "ligne 52 « tache t13 » : c'est la tâche 13, et un plan découpé en compte 12 au plus";
        assert!(refus.preuve.starts_with(debut), "{refus}");
        assert!(refus.correction.contains("Regroupe les tâches"), "{refus}");
    }
    // À la main, 13 tâches se lisent.
    assert_eq!(Plan::lire_en(decoupe(13).as_bytes(), Mode::Main).unwrap().0.taches.len(), 13);
    // Une tâche sans `fichiers`, sans `modele`, ou sans l'un ni l'autre : refusée dans un plan découpé, lue à la main.
    let cas = [
        (["tache a", "modele : haiku", "    x"], "n'a pas de clé `fichiers`"),
        (["tache a", "fichiers : src/", "    x"], "n'a pas de clé `modele`"),
        (["tache a", "relance : opus", "    x"], "n'a ni `fichiers` ni `modele`"),
    ];
    for (suite, bout) in cas {
        let lignes = [&ENTETE[..], &suite[..], &["tache b", "fichiers : b/", "modele : haiku", "    y"]].concat();
        for mode in decoupes {
            let refus = refuse_en(lire_en(&lignes, mode));
            assert_eq!(refus.code, Code::PlanBorne);
            let attendu =
                format!("ligne 4 « tache a » : la tâche « a » {bout}, que tout plan découpé donne à chaque tâche");
            assert_eq!(refus.preuve, attendu);
            assert!(refus.correction.contains("`fichiers : <chemin> …`"), "{refus}");
            assert!(refus.correction.contains("`modele : <alias>`"), "{refus}");
        }
        assert!(lire_en(&lignes, Mode::Main).is_ok(), "{suite:?}");
    }
}

#[test]
fn une_tache_d_un_plan_decoupe_n_a_pas_sa_propre_preuve() {
    // La preuve d'une tâche est une commande que Kraken rejoue avec les droits de l'humain, et que la vague prend
    // avant la sienne : écrite par une main, elle lui donnerait un droit neuf. Seul l'humain donne la preuve de la
    // vague, par l'en-tête que le rejeu juge.
    let chemins = Vec::new();
    let demande = demande(&chemins);
    let decoupes = [Mode::Decoupe(None), Mode::Decoupe(Some(&demande))];
    // Après l'en-tête (lignes 1 à 3) : la suite du plan, et la ligne de la `preuve` de tâche.
    let cas: &[(&[&str], usize)] = &[
        (&["tache a", "preuve : echo pwned", "fichiers : src/", "modele : haiku", "    x"], 5),
        (&["tache a", "fichiers : src/", "modele : haiku", "preuve : cargo test", "    x"], 7),
        // Sur la seconde tâche ; et blanche, la clé est refusée comme les autres, non comme une valeur à compléter.
        (
            &[
                "tache a",
                "fichiers : a/",
                "modele : haiku",
                "    x",
                "tache b",
                "fichiers : b/",
                "modele : haiku",
                "preuve : exit 0",
                "    y",
            ],
            11,
        ),
        (&["tache a", "fichiers : src/", "modele : haiku", "preuve :", "    x"], 7),
    ];
    for &(suite, numero) in cas {
        let lignes = [&ENTETE[..], suite].concat();
        for mode in decoupes {
            let refus = refuse_en(lire_en(&lignes, mode));
            assert_eq!(refus.code, Code::PlanBorne, "{suite:?}");
            let debut = format!("ligne {numero} {} : ", cite(lignes[numero - 1]));
            assert!(refus.preuve.starts_with(&debut), "la preuve nomme la ligne {numero} et son texte : {refus}");
            assert!(refus.preuve.contains("porte une clé `preuve`"), "{refus}");
            let regle = "une tâche d'un plan découpé joue la preuve de la vague, que seul l'humain donne";
            assert!(refus.correction.contains(regle), "{refus}");
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
    }
    // À la main, la tâche joue sa preuve, à défaut celle de la vague : le plan se lit tel quel, et une clé blanche
    // reste le refus 6021 qui dit d'écrire la commande.
    let a_la_main = [&ENTETE[..], cas[1].0].concat();
    let (plan, _) = lire_en(&a_la_main, Mode::Main).unwrap();
    assert_eq!(plan.taches[0].preuve.as_deref(), Some("cargo test"));
    let refus = refuse_en(lire_en(&[&ENTETE[..], cas[3].0].concat(), Mode::Main));
    assert_eq!(refus.code, Code::PlanBlanc);
    assert!(refus.correction.starts_with("Écris la commande après `preuve :`"), "{refus}");
    // La preuve de la vague est celle de l'en-tête, que le rejeu compare à ce que l'humain a donné : une tâche sans
    // `preuve` la joue.
    let sans = [&ENTETE[..], &["tache a", "fichiers : src/", "modele : haiku", "    x"]].concat();
    for mode in decoupes {
        let (plan, _) = lire_en(&sans, mode).unwrap();
        assert_eq!((plan.preuve.as_deref(), plan.taches[0].preuve.as_deref()), (Some("cargo test"), None));
    }
}

#[test]
fn une_ligne_fichiers_d_un_plan_decoupe_a_100_chemins_au_plus() {
    // Le passage en série compare deux à deux les chemins de deux tâches : sans borne, 2 tâches de 100 000 chemins
    // (400 Ko) faisaient attendre le juge 57 s en release, et un plan de 1 Mo des minutes. Un plan découpé en a 12
    // au plus, de 100 chemins chacune.
    let de = |lettre: &str, n: usize| (0..n).map(|k| format!("{lettre}{k}/")).collect::<Vec<_>>().join(" ");
    // Un plan de deux tâches, de `n` dossiers chacune, sans rien de commun : la ligne `fichiers` de la première est
    // la ligne 5.
    let deux_taches = |n: usize| {
        let (a, b) = (format!("fichiers : {}", de("d", n)), format!("fichiers : {}", de("e", n)));
        let taches = ["tache a", &a, "modele : haiku", "    x", "tache b", &b, "modele : haiku", "    y"];
        texte(&[&ENTETE[..], &taches].concat())
    };
    let chemins = Vec::new();
    let demande = demande(&chemins);
    for mode in [Mode::Decoupe(None), Mode::Decoupe(Some(&demande))] {
        // 100 chemins par tâche se lisent, 200 en tout.
        let (plan, _) = Plan::lire_en(deux_taches(100).as_bytes(), mode).unwrap();
        let comptes: Vec<Option<usize>> = plan.taches.iter().map(|t| t.fichiers.as_ref().map(Vec::len)).collect();
        assert_eq!(comptes, [Some(100), Some(100)]);
        // 101 sont refusés à la ligne, comme des milliers : la borne se juge avant tout chemin, et le refus est
        // immédiat.
        for n in [101, 3_000] {
            let refus = refuse_en(Plan::lire_en(deux_taches(n).as_bytes(), mode));
            assert_eq!(refus.code, Code::PlanBorne, "{n}");
            let ligne = cite(&format!("fichiers : {}", de("d", n)));
            let debut = format!("ligne 5 {ligne} : elle nomme {n} chemins, et la ligne `fichiers` d'une tâche");
            assert!(refus.preuve.starts_with(&debut), "{refus}");
            assert!(refus.preuve.ends_with("d'un plan découpé en compte 100 au plus"), "{refus}");
            assert!(refus.correction.contains("Regroupe les chemins sous leurs dossiers"), "{refus}");
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
        // Un chemin en trop après un chemin faux : la borne passe avant, qui dit le nombre.
        let faux = format!("fichiers : /absolu {}", de("d", 100));
        let plan = texte(&[&ENTETE[..], &["tache a", &faux, "modele : haiku", "    x"]].concat());
        let refus = Plan::lire_en(plan.as_bytes(), mode).unwrap_err();
        assert!(refus.code == Code::PlanBorne && refus.preuve.contains("elle nomme 101 chemins"), "{refus}");
    }
    // À la main, la ligne n'a pas de borne.
    for n in [101, 1_000] {
        assert!(Plan::lire_en(deux_taches(n).as_bytes(), Mode::Main).is_ok(), "{n}");
    }
}

#[test]
fn au_rejeu_l_en_tete_est_celui_que_kraken_a_demande() {
    let chemins = Vec::new();
    let demande = demande(&chemins);
    let mode = Mode::Decoupe(Some(&demande));
    let taches = ["tache a", "fichiers : src/", "modele : haiku", "    x"];
    // Tel quel, aux blancs du bord près, dans l'ordre qu'on veut.
    for entete in [ENTETE, ["plan 1", "preuve:cargo test  ", "objectif :\tAjouter etat"]] {
        assert!(lire_en(&[&entete[..], &taches].concat(), mode).is_ok(), "{entete:?}");
    }
    let cas = [
        (
            ["plan 1", "objectif : Ajouter état", "preuve : cargo test"],
            2,
            "la clé « objectif » vaut « Ajouter état », et Lydite a demandé « Ajouter etat »",
        ),
        (["plan 1", "objectif : ajouter etat", "preuve : cargo test"], 2, "vaut « ajouter etat »"),
        (
            ["plan 1", "objectif : Ajouter etat", "preuve : exit 0"],
            3,
            "la clé « preuve » vaut « exit 0 », et Lydite a demandé « cargo test »",
        ),
        (["plan 1", "preuve : cargo test --lib", "objectif : Ajouter etat"], 2, "vaut « cargo test --lib »"),
    ];
    for (entete, numero, bout) in cas {
        let lignes = [&entete[..], &taches].concat();
        let refus = refuse_en(lire_en(&lignes, mode));
        assert_eq!(refus.code, Code::PlanEntete, "{entete:?}");
        let debut = format!("ligne {numero} {} : ", cite(lignes[numero - 1]));
        assert!(refus.preuve.starts_with(&debut) && refus.preuve.contains(bout), "{refus}");
        assert!(refus.correction.contains("`objectif : Ajouter etat`, puis `preuve : cargo test`"), "{refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        // À la main, ou sans contexte, l'en-tête ne se juge pas.
        for autre in [Mode::Main, Mode::Decoupe(None)] {
            assert!(lire_en(&lignes, autre).is_ok(), "{entete:?}");
        }
    }
    // Les blancs du bord de ce que l'humain a donné ne comptent pas : une ligne du plan ne peut pas les porter.
    let large = Demande { objectif: "  Ajouter etat ", preuve: "\tcargo test\n", chemins: &chemins };
    assert!(lire_en(&[&ENTETE[..], &taches].concat(), Mode::Decoupe(Some(&large))).is_ok());
}

#[test]
fn les_taches_qui_partagent_un_chemin_passent_en_serie() {
    let en_serie = |suite: &[&str]| lire_en(&[&ENTETE[..], suite].concat(), Mode::Main).unwrap();
    // Aucune dépendance entre elles : la seconde du fichier attend la première.
    let (plan, attentes) =
        en_serie(&["tache a", "fichiers : src/x.rs", "    x", "tache b", "fichiers : README.md src/x.rs", "    y"]);
    assert_eq!(dites(&attentes), ["b attend a : elles partagent src/x.rs"]);
    assert_eq!((plan.taches[0].depend.len(), plan.taches[1].depend.as_slice()), (0, &["a".to_string()][..]));
    // Une dépendance directe, ou de loin : elles ne tournent pas ensemble, et rien ne s'ajoute.
    let (plan, attentes) =
        en_serie(&["tache a", "fichiers : x", "    x", "tache b", "depend : a", "fichiers : x", "    y"]);
    assert!(attentes.is_empty());
    assert_eq!(plan.taches[1].depend, ["a"]);
    let de_loin = ["tache a", "fichiers : x", "    x", "tache b", "depend : a", "    y"];
    let (_, attentes) = en_serie(&[&de_loin[..], &["tache c", "depend : b", "fichiers : x", "    z"]].concat());
    assert!(attentes.is_empty());
    // Un dossier couvre ce qu'il contient, d'un côté comme de l'autre : le chemin partagé est le plus étroit.
    for (a, b) in [("src/", "src/x.rs"), ("src/x.rs", "src/"), ("src/", "src/sous/"), ("src/", "src/")] {
        let (fa, fb) = (format!("fichiers : {a}"), format!("fichiers : {b}"));
        let (_, attentes) = en_serie(&["tache a", &fa, "    x", "tache b", &fb, "    y"]);
        let etroit = if a.len() > b.len() { a } else { b };
        assert_eq!(dites(&attentes), [format!("b attend a : elles partagent {etroit}")], "{a} et {b}");
    }
    // Ne se partagent pas : deux dossiers voisins, un nom qui n'est que le début d'un autre, un fichier et un
    // dossier de même nom, deux casses, une tâche sans `fichiers`.
    let disjoints =
        [("src/a/", "src/b/"), ("src/a", "src/ab"), ("src/a/", "src/ab"), ("src", "src/"), ("x.rs", "X.rs")];
    for (a, b) in disjoints {
        let (fa, fb) = (format!("fichiers : {a}"), format!("fichiers : {b}"));
        assert!(en_serie(&["tache a", &fa, "    x", "tache b", &fb, "    y"]).1.is_empty(), "{a} et {b}");
    }
    assert!(en_serie(&["tache a", "    x", "tache b", "fichiers : src/", "    y"]).1.is_empty());
    assert!(couvre("src/", "src/a/b.rs") && couvre("a.rs", "a.rs") && couvre("src/", "src/"));
    assert!(!couvre("src", "src/a.rs") && !couvre("src/a/", "src/ab") && !couvre("SRC/", "src/a.rs"));
    // Trois tâches au même chemin : chacune attend la précédente, et une dépendance suffit à la troisième.
    let trois =
        ["tache a", "fichiers : x", "    x", "tache b", "fichiers : x", "    y", "tache c", "fichiers : x", "    z"];
    let (plan, attentes) = en_serie(&trois);
    assert_eq!(dites(&attentes), ["b attend a : elles partagent x", "c attend b : elles partagent x"]);
    assert_eq!(plan.taches[2].depend, ["b"]);
    // a et b ne partagent rien et tournent ensemble ; c partage avec l'une et l'autre, et les attend toutes deux.
    let (plan, attentes) = en_serie(&[
        "tache a",
        "fichiers : a.rs",
        "    x",
        "tache b",
        "fichiers : b.rs",
        "    y",
        "tache c",
        "fichiers : a.rs b.rs",
        "    z",
    ]);
    assert_eq!(dites(&attentes), ["c attend b : elles partagent b.rs", "c attend a : elles partagent a.rs"]);
    assert!(plan.taches[1].depend.is_empty());
    // Les dépendances d'abord, puis l'ordre du fichier. c, la première du fichier, dépend de b, placée plus bas ; a
    // partage x avec c et y avec b. L'ordre du fichier seul ferait attendre c à a (x), puis a à b (y) : avec c qui
    // dépend de b, le cycle c → b → a → c. L'ordre topologique (a, b, c) fait attendre a à b, et c, qui dépend de
    // b, attend déjà a de loin.
    let (plan, attentes) = en_serie(&[
        "tache c",
        "depend : b",
        "fichiers : x",
        "    z",
        "tache a",
        "fichiers : x y",
        "    x",
        "tache b",
        "fichiers : y",
        "    y",
    ]);
    assert_eq!(dites(&attentes), ["b attend a : elles partagent y"]);
    assert_eq!((plan.taches[0].depend.as_slice(), plan.taches[1].depend.len()), (&["b".to_string()][..], 0));
    // Une attente se dit sans piéger un terminal : le chemin vient d'un fichier.
    let piege = ["tache a", "fichiers : x\u{1b}[31m", "    x", "tache b", "fichiers : x\u{1b}[31m", "    y"];
    assert_eq!(dites(&en_serie(&piege).1), ["b attend a : elles partagent x\\u{1b}[31m"]);
}

#[test]
fn apres_le_passage_en_serie_deux_taches_qui_partagent_un_chemin_ne_tournent_jamais_ensemble() {
    // Des plans tirés à graine fixe : des tâches dans un ordre caché, chacune dépendant de tâches d'avant dans cet
    // ordre (placées plus haut ou plus bas dans le fichier), avec des chemins tirés parmi des fichiers et des dossiers
    // qui se couvrent.
    let chemins = ["a", "a/", "a/x", "a/y", "b/", "b/z", "c", "d/e/"];
    let mut graine = 0x2545_F491_4F6C_DD1D;
    for _ in 0..500 {
        let n = 1 + (tirage(&mut graine) % 8) as usize;
        let mut cache: Vec<usize> = (0..n).collect();
        for i in (1..n).rev() {
            cache.swap(i, (tirage(&mut graine) % (i as u64 + 1)) as usize);
        }
        let mut lignes: Vec<String> = ENTETE.iter().map(|ligne| ligne.to_string()).collect();
        let mut ecrites = Vec::new();
        for k in 0..n {
            let place = cache.iter().position(|&tache| tache == k).unwrap();
            let depend: Vec<String> = cache[..place]
                .iter()
                .filter(|_| tirage(&mut graine).is_multiple_of(3))
                .map(|t| format!("t{t}"))
                .collect();
            let fichiers: Vec<&str> =
                chemins.iter().copied().filter(|_| tirage(&mut graine).is_multiple_of(4)).collect();
            lignes.extend([format!("tache t{k}"), format!("depend : {}", depend.join(" "))]);
            if !fichiers.is_empty() {
                lignes.push(format!("fichiers : {}", fichiers.join(" ")));
            }
            lignes.push("    Faire.".into());
            ecrites.push(depend);
        }
        let texte = lignes.join("\n");
        let (plan, attentes) = Plan::lire_en(texte.as_bytes(), Mode::Main).unwrap_or_else(|r| panic!("{r}\n{texte}"));
        // Les dépendances écrites restent en tête du `depend` de leur tâche ; les ajoutées, et elles seules, suivent.
        let mut ajoutees = 0;
        for (tache, ecrite) in plan.taches.iter().zip(&ecrites) {
            assert_eq!(&tache.depend[..ecrite.len()], &ecrite[..], "{texte}");
            ajoutees += tache.depend.len() - ecrite.len();
        }
        assert_eq!(attentes.len(), ajoutees, "{texte}");
        // Sans cycle, et toute paire qui partage un chemin est ordonnée : l'une dépend de l'autre, même de loin.
        let rang: HashMap<&str, usize> = (0..).zip(&plan.taches).map(|(r, t)| (t.id.as_str(), r)).collect();
        let deps: Vec<Vec<usize>> =
            plan.taches.iter().map(|t| t.depend.iter().map(|id| rang[id.as_str()]).collect()).collect();
        assert!(cycle(&deps).is_none(), "{texte}");
        let atteintes = |depart: usize| {
            let (mut vues, mut pile) = (vec![false; n], vec![depart]);
            while let Some(tache) = pile.pop() {
                for &dependance in &deps[tache] {
                    if !vues[dependance] {
                        vues[dependance] = true;
                        pile.push(dependance);
                    }
                }
            }
            vues
        };
        for i in 0..n {
            for j in 0..i {
                if partage(&plan.taches[i], &plan.taches[j]).is_some() {
                    assert!(atteintes(i)[j] || atteintes(j)[i], "t{i} et t{j} tourneraient ensemble :\n{texte}");
                }
            }
        }
    }
}

#[test]
fn dix_mille_taches_au_meme_chemin_passent_en_serie_sans_empiler() {
    let mut lignes: Vec<String> = ENTETE.iter().map(|ligne| ligne.to_string()).collect();
    for k in 1..=NOMBRE_MAX {
        lignes.extend([format!("tache t{k}"), "fichiers : x".into(), "    Faire.".into()]);
    }
    // Sur une pile de 512 Kio, comme la lecture de dix mille tâches : rien ne descend de tâche en tâche par récursion.
    let lecture = move || {
        let (plan, attentes) = Plan::lire_en(lignes.join("\n").as_bytes(), Mode::Main).unwrap();
        (plan.taches.last().map(|tache| tache.depend.clone()), attentes.len())
    };
    let (derniere, ajoutees) = thread::Builder::new().stack_size(512 * 1024).spawn(lecture).unwrap().join().unwrap();
    assert_eq!(ajoutees, NOMBRE_MAX - 1);
    assert_eq!(derniere, Some(vec![format!("t{}", NOMBRE_MAX - 1)]));
}

#[test]
fn aucun_octet_ne_fait_paniquer_une_lecture_de_m5() {
    let plan = texte(&[
        "plan 1",
        "objectif : Ajouter etat",
        "preuve : cargo test",
        "",
        "tache a",
        "fichiers : src/ Cargo.toml",
        "modele : opus",
        "relance : sonnet",
        "critique : oui",
        "    Écrire a.",
        "tache b",
        "depend : a",
        "fichiers : src/b.rs",
        "modele : haiku",
        "\tÉcrire b.",
        "tache c",
        "fichiers : src/b.rs docs/",
        "modele : haiku",
        "    Écrire c.",
    ]);
    let chemins: Vec<String> = ["src/main.rs", "Cargo.toml", "docs/a.md"].map(String::from).into();
    let demande = demande(&chemins);
    let modes = [Mode::Main, Mode::Decoupe(None), Mode::Decoupe(Some(&demande))];
    let du_plan = [
        Code::PlanVersion,
        Code::PlanEncodage,
        Code::PlanLigne,
        Code::PlanIdentifiant,
        Code::PlanDependance,
        Code::PlanBlanc,
        Code::PlanChemin,
        Code::PlanBorne,
        Code::PlanEntete,
    ];
    let sans_panique = |octets: &[u8], mode: Mode<'_>| {
        if let Err(refus) = Plan::lire_en(octets, mode) {
            assert!(du_plan.contains(&refus.code), "{refus}");
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
    };
    for mode in modes {
        assert!(Plan::lire_en(plan.as_bytes(), mode).is_ok(), "{mode:?}");
        // Chaque préfixe, puis chaque octet remplacé à son tour par ce qui trouble le plus une lecture de M5.
        for fin in 0..=plan.len() {
            sans_panique(&plan.as_bytes()[..fin], mode);
        }
        for rang in 0..plan.len() {
            for remplacant in [0xFF, b'\r', b':', b'\t', b'/', b'.', b'\\', b' ', b'C', b'-'] {
                let mut octets = plan.clone().into_bytes();
                octets[rang] = remplacant;
                sans_panique(&octets, mode);
            }
        }
    }
    // Des plans tirés à graine fixe, de morceaux de plan : des clés, des chemins, des blancs, un octet hors de l'UTF-8.
    let morceaux: [&[u8]; 17] = [
        b"tache ",
        b"t1",
        b"t2",
        b"\n",
        b"    x",
        b"fichiers : ",
        b"modele : ",
        b"relance : ",
        b"critique : ",
        b"depend : ",
        b"oui",
        b"..",
        b"/",
        b".git",
        b" ",
        b"\xFF",
        b"\r",
    ];
    let mut graine = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..3000 {
        let mut octets = b"plan 1\nobjectif : Ajouter etat\npreuve : cargo test\n".to_vec();
        for _ in 0..tirage(&mut graine) % 40 {
            octets.extend_from_slice(morceaux[(tirage(&mut graine) % morceaux.len() as u64) as usize]);
        }
        for mode in modes {
            sans_panique(&octets, mode);
        }
    }
}
