use super::*;
use crate::essais::{remarque, texte};

/// Le refus de ce fichier : il doit être refusé, en 7015, et son affichage tient en quatre lignes.
fn refuse(brut: impl AsRef<[u8]>) -> Refus {
    let refus = lire_remarques(brut.as_ref()).unwrap_err();
    assert_eq!(refus.code, Code::RemarquesIllisibles, "{refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    assert!(!refus.correction.trim().is_empty());
    refus
}

#[test]
fn un_fichier_sans_remarque_ne_porte_que_son_en_tete() {
    let sans = ["remarques 1", "remarques 1\n", "remarques 1\r\n", "\u{feff}remarques 1\n", "remarques 1\n\n \t\n\n"];
    for brut in sans {
        assert_eq!(lire_remarques(brut.as_bytes()), Ok(Vec::new()), "{brut:?}");
    }
}

#[test]
fn les_remarques_se_lisent_dans_l_ordre_avec_leurs_champs() {
    let brut = texte(&[
        "remarques 1",
        "",
        "remarque 1",
        "fichier : src/a.rs",
        "ligne : 12",
        "bloquante : oui",
        "    Le test ne peut pas echouer.",
        "    Ajouter une assertion.",
        "",
        "remarque 2",
        "fichier   :   docs/",
        "bloquante : non",
        "\tUne ligne, puis",
        "",
        "\t  un blanc dedans.",
        "",
        "",
        "remarque 3",
        "fichier : Lisez-moi.md",
        "ligne : 1",
        "bloquante : non",
        "  Un accent : é, et un deux-points : sans effet.",
    ]);
    let attendu = vec![
        remarque("src/a.rs", Some(12), true, "Le test ne peut pas echouer.\nAjouter une assertion."),
        remarque("docs/", None, false, "Une ligne, puis\n\n  un blanc dedans."),
        remarque("Lisez-moi.md", Some(1), false, "Un accent : é, et un deux-points : sans effet."),
    ];
    assert_eq!(lire_remarques(brut.as_bytes()), Ok(attendu.clone()));
    // Les fins de ligne `\r\n` et un BOM de tête n'y changent rien.
    assert_eq!(lire_remarques(brut.replace('\n', "\r\n").as_bytes()), Ok(attendu.clone()));
    assert_eq!(lire_remarques(format!("\u{feff}{brut}").as_bytes()), Ok(attendu));
}

#[test]
fn chaque_faute_du_fichier_est_le_refus_7015_qui_nomme_sa_ligne() {
    let debut = |reste: &str| format!("remarques 1\n{reste}");
    let cas: Vec<(String, &str)> = vec![
        // La première ligne : `remarques 1`, seule.
        (String::new(), "line 1 «  » : the first line of a remarks file is `remarques 1`"),
        ("remarques 3\n".into(), "line 1 « remarques 3 » : the first line"),
        ("plan 1\n".into(), "line 1 « plan 1 » : the first line"),
        ("remarques 1 \n".into(), "line 1 « remarques 1  » : the first line"),
        ("\nremarques 1\n".into(), "line 1 «  » : the first line"),
        // Une ligne hors de la forme.
        (debut("bonjour\n"), "line 2 « bonjour » : it is neither empty, nor indented, nor `remarque <n>`"),
        (debut("remarques 1\n"), "line 2 « remarques 1 » : it is neither empty"),
        (debut("remarque1\n"), "line 2 « remarque1 » : it is neither empty"),
        (debut("    texte\n"), "line 2 «     texte » : this line is indented, and no `remarque <n>` line"),
        (debut("fichier : a\n"), "the key « fichier » follows no `remarque <n>` line"),
        (debut("auteur : moi\n"), "the key « auteur » is not a remark key (fichier, ligne, bloquante)"),
        // La numérotation : 1, 2, 3… dans l'ordre.
        (debut("remarque\n"), "the remark carries the number «  », and the next one is 1"),
        (debut("remarque deux\n"), "the remark carries the number « deux », and the next one is 1"),
        (debut("remarque 2\n"), "the remark carries the number « 2 », and the next one is 1"),
        (debut("remarque 0\n"), "the remark carries the number « 0 »"),
        (debut("remarque -1\n"), "the remark carries the number « -1 »"),
        (debut("remarque +1\n"), "the remark carries the number « +1 »"),
        (debut("remarque 1 2\n"), "the remark carries the number « 1 2 »"),
        (debut("remarque 99999999999999999999999\n"), "the number « 99999999999999999999999 »"),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n x\nremarque 3\n"),
            "line 6 « remarque 3 » : the remark carries the number « 3 », and the next one is 2",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n x\nremarque 1\n"),
            "the remark carries the number « 1 », and the next one is 2",
        ),
        // L'ordre des clés : fichier, ligne si elle paraît, bloquante, une fois chacune.
        (debut("remarque 1\nbloquante : oui\n"), "the key « bloquante » comes before `fichier`"),
        (debut("remarque 1\nligne : 1\n"), "the key « ligne » comes before `fichier`"),
        (debut("remarque 1\nfichier : a\nfichier : b\n"), "the key « fichier » is already given in this remark"),
        (debut("remarque 1\nfichier : a\nligne : 1\nligne : 2\n"), "the key « ligne » is already given"),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\nbloquante : oui\n"),
            "the key « bloquante » is already given",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\nligne : 1\n"),
            "the key « ligne » comes after `bloquante`, the last key",
        ),
        (
            debut("remarque 1\nfichier : a\nligne : 1\nfichier : b\n"),
            "the key « fichier » is already given in this remark",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n    x\nbloquante : oui\n"),
            "the key « bloquante » comes after the indented lines",
        ),
        // Ce qui manque à une remarque.
        (debut("remarque 1\n"), "line 2 « remarque 1 » : the remark has no `fichier : …` line"),
        (debut("remarque 1\nfichier : a\n"), "line 2 « remarque 1 » : the remark has no `bloquante : …` line"),
        (
            debut("remarque 1\nfichier : a\nbloquante : oui\n"),
            "the remark has no text: its finding and its fix, as indented lines",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : oui\n   \n\t\nremarque 2\n"),
            "line 2 « remarque 1 » : the remark has no text",
        ),
        (
            debut("remarque 1\nfichier : a\n    texte avant bloquante\n"),
            "the text of a remark comes after its keys, the last one being `bloquante`",
        ),
        // Les valeurs.
        (debut("remarque 1\nfichier :\n"), "the key « fichier » is blank"),
        (debut("remarque 1\nfichier : a\nligne :   \n"), "the key « ligne » is blank"),
        (debut("remarque 1\nfichier : a\nbloquante :\n"), "the key « bloquante » is blank"),
        (
            debut("remarque 1\nfichier : a\nligne : 0\n"),
            "the key « ligne » is set to « 0 », and its value is a positive integer",
        ),
        (debut("remarque 1\nfichier : a\nligne : -3\n"), "the key « ligne » is set to « -3 »"),
        (debut("remarque 1\nfichier : a\nligne : 1.5\n"), "the key « ligne » is set to « 1.5 »"),
        (debut("remarque 1\nfichier : a\nligne : douze\n"), "the key « ligne » is set to « douze »"),
        (debut("remarque 1\nfichier : a\nligne : ٣\n"), "the key « ligne » is set to « ٣ »"),
        (debut("remarque 1\nfichier : a\nligne : 4294967296\n"), "the key « ligne » is set to « 4294967296 »"),
        (
            debut("remarque 1\nfichier : a\nbloquante : Oui\n"),
            "the key « bloquante » is set to « Oui », and its only values are `oui` and `non`",
        ),
        (debut("remarque 1\nfichier : a\nbloquante : peut-être\n"), "the key « bloquante » is set to « peut-être »"),
        (debut("remarque 1\nfichier : a\nbloquante : oui non\n"), "the key « bloquante » is set to « oui non »"),
        // Un chemin relatif à la racine du dépôt, écrit comme git l'écrit.
        (debut("remarque 1\nfichier : C:\\x\\a.rs\n"), "the path « C:\\x\\a.rs » is written with a backslash"),
        (debut("remarque 1\nfichier : src\\a.rs\n"), "is written with a backslash, and git separates names with `/`"),
        (debut("remarque 1\nfichier : /etc/passwd\n"), "the path « /etc/passwd » is absolute"),
        (debut("remarque 1\nfichier : C:/x\n"), "the path « C:/x » is absolute"),
        (debut("remarque 1\nfichier : ../a\n"), "the path « ../a » has an empty name, `.` or `..`"),
        (debut("remarque 1\nfichier : a/../b\n"), "has an empty name, `.` or `..`"),
        (debut("remarque 1\nfichier : a//b\n"), "has an empty name, `.` or `..`"),
        (debut("remarque 1\nfichier : ./a\n"), "has an empty name, `.` or `..`"),
        (debut("remarque 1\nfichier : a\tb\n"), "the path « a\\tb » carries a control character"),
        (debut("remarque 1\nfichier : a\u{1b}[31m\n"), "the path « a\\u{1b}[31m » carries a control character"),
    ];
    for (brut, bout) in cas {
        let refus = refuse(&brut);
        assert!(refus.preuve.contains(bout), "« {bout} » dans la preuve de {brut:?} : {refus}");
    }
}

#[test]
fn la_forme_2_se_lit_avec_ses_deux_blocs_et_la_forme_1_se_lit_encore() {
    let brut = texte(&[
        "remarques 2",
        "",
        "remarque 1",
        "fichier : src/a.rs",
        "ligne : 3",
        "bloquante : oui",
        "    Le test ne peut pas echouer.",
        "",
        "rejoue",
        "    cargo test vert sur la tête.",
        "  Le test a est joué.",
        "",
        "non rejoue",
        "\tLe réseau n'est pas joué.",
    ]);
    let lu = lire_fichier(brut.as_bytes()).unwrap();
    assert_eq!(lu.forme, 2);
    assert_eq!(lu.remarques, vec![remarque("src/a.rs", Some(3), true, "Le test ne peut pas echouer.")]);
    assert_eq!(lu.rejoue, vec!["cargo test vert sur la tête.", "Le test a est joué."]);
    assert_eq!(lu.non_rejoue, vec!["Le réseau n'est pas joué."]);
    assert_eq!(lire_remarques(brut.as_bytes()).unwrap().len(), 1);
    assert_eq!(lu.exiger_la_forme_2().map(|f| f.forme), Ok(2));
    // Seuls, les deux blocs suffisent ; CRLF et BOM n'y changent rien.
    let seuls = "remarques 2\n\nrejoue\n    a\nnon rejoue\n    b\n";
    for brut in [seuls.to_string(), seuls.replace('\n', "\r\n"), format!("\u{feff}{seuls}")] {
        let lu = lire_fichier(brut.as_bytes()).unwrap();
        assert_eq!((lu.remarques.len(), lu.rejoue, lu.non_rejoue), (0, vec!["a".to_string()], vec!["b".to_string()]));
    }
    // La forme 1 se lit encore, sans bloc ; mais la forme 2 est exigée du relecteur.
    let un = lire_fichier(b"remarques 1\n").unwrap();
    assert_eq!((un.forme, un.rejoue.is_empty(), un.non_rejoue.is_empty()), (1, true, true));
    let refus = un.exiger_la_forme_2().unwrap_err();
    assert_eq!(refus.code, Code::RemarquesIllisibles);
    assert!(refus.preuve.contains("line 1 « remarques 1 » : the reviewer writes form 2"), "{refus}");
}

#[test]
fn un_bloc_manquant_vide_hors_d_ordre_ou_double_est_refuse_en_7015_a_sa_ligne() {
    let remarque_1 = "remarque 1\nfichier : a\nbloquante : non\n    x\n";
    let cas: Vec<(String, &str)> = vec![
        ("remarques 2\n".into(), "line 2 «  » : the file has no block `rejoue`"),
        ("remarques 2\nrejoue\n    a\n".into(), "line 4 «  » : the file has no block `non rejoue`"),
        ("remarques 2\nnon rejoue\n    a\n".into(), "line 2 « non rejoue » : the block `non rejoue` comes after"),
        (
            "remarques 2\nrejoue\n    a\nrejoue\n    b\n".into(),
            "line 4 « rejoue » : the block `rejoue` is already given",
        ),
        (
            "remarques 2\nrejoue\n    a\nnon rejoue\n    b\nnon rejoue\n    c\n".into(),
            "line 6 « non rejoue » : the block `non rejoue` is already given",
        ),
        (
            "remarques 2\nrejoue\nnon rejoue\n    b\n".into(),
            "line 2 « rejoue » : the block `rejoue` has no indented line",
        ),
        ("remarques 2\nrejoue\n  \n\nnon rejoue\n    b\n".into(), "line 2 « rejoue » : the block `rejoue` has no"),
        ("remarques 2\nrejoue\n    a\nnon rejoue\n".into(), "line 4 « non rejoue » : the block `non rejoue` has no"),
        (
            format!("remarques 2\nrejoue\n    a\nnon rejoue\n    b\n{remarque_1}"),
            "line 6 « remarque 1 » : a remark comes before the blocks",
        ),
        (
            "remarques 2\nrejoue\n    a\nfichier : a\n".into(),
            "line 4 « fichier : a » : the key « fichier » follows no `remarque <n>` line",
        ),
        // En forme 1, les blocs ne se lisent pas : la ligne du bloc dit pourquoi.
        (
            "remarques 1\nrejoue\n    a\n".into(),
            "line 2 « rejoue » : the blocks `rejoue` and `non rejoue` are read only under `remarques 2`",
        ),
    ];
    for (brut, bout) in cas {
        let refus = refuse(&brut);
        assert!(refus.preuve.contains(bout), "« {bout} » dans la preuve de {brut:?} : {refus}");
    }
}

const RAPPEL: &str = "The number on the first line is the form of the file, never a number of remarks";
const ECRIS_2: &str = "the reviewer writes `remarques 2` whatever the number of its remarks, even none";

#[test]
fn une_premiere_ligne_remarques_et_des_chiffres_autres_que_1_et_2_dit_que_le_chiffre_est_la_forme() {
    for premiere in ["remarques 0", "remarques 3", "remarques 14"] {
        let refus = refuse(format!("{premiere}\n"));
        assert!(refus.preuve.contains(&format!("line 1 « {premiere} » : the first line")), "{refus}");
        assert!(refus.correction.contains(RAPPEL), "{refus}");
        assert!(refus.correction.contains(ECRIS_2), "{refus}");
        assert!(refus.correction.contains("Write `remarques 2` alone on the first line (or `remarques 1`)"), "{refus}");
    }
    // Toute autre première ligne garde son refus d'avant, sans le rappel.
    for premiere in ["remarques deux", "remarques", "remarques ", "remarques 2a", "remarques -1", "Remarques 3"] {
        let refus = refuse(format!("{premiere}\n"));
        assert!(!refus.correction.contains(RAPPEL), "{premiere:?} : {refus}");
        assert_eq!(
            refus.correction,
            "Write `remarques 2` alone on the first line (or `remarques 1`): Lydite reads only these forms, before \
             anything else.",
            "{premiere:?}"
        );
    }
}

#[test]
fn des_blocs_sous_remarques_1_sont_refuses_a_leur_ligne_en_disant_que_le_chiffre_est_la_forme() {
    for (brut, ligne) in [
        ("remarques 1\nrejoue\n    a\n", "line 2 « rejoue »"),
        ("remarques 1\n\nnon rejoue\n    a\n", "line 3 « non rejoue »"),
        ("remarques 1\n\nremarque 1\nfichier : a\nbloquante : non\n    x\nrejoue\n    a\n", "line 7 « rejoue »"),
    ] {
        let refus = refuse(brut);
        assert!(refus.preuve.contains(ligne), "{refus}");
        assert!(refus.preuve.contains("are read only under `remarques 2`"), "{refus}");
        assert!(refus.preuve.contains("the first line of this file is `remarques 1`"), "{refus}");
        assert!(refus.correction.contains("Write `remarques 2` on the first line"), "{refus}");
        assert!(refus.correction.contains(RAPPEL) && refus.correction.contains(ECRIS_2), "{refus}");
    }
    // Un fichier de forme 1 sans bloc et un fichier de forme 2 valide se lisent comme avant.
    let un = lire_fichier(b"remarques 1\n\nremarque 1\nfichier : a\nbloquante : non\n    x\n").unwrap();
    assert_eq!((un.forme, un.remarques.len(), un.rejoue.is_empty()), (1, 1, true));
    let deux = lire_fichier(b"remarques 2\nrejoue\n    a\nnon rejoue\n    b\n").unwrap();
    assert_eq!((deux.forme, deux.rejoue, deux.non_rejoue), (2, vec!["a".to_string()], vec!["b".to_string()]));
    // Le refus de la forme 1 écartée par le juge du relecteur redit que le chiffre est la forme.
    let refus = lire_fichier(b"remarques 1\n").unwrap().exiger_la_forme_2().unwrap_err();
    assert!(refus.correction.contains(RAPPEL) && refus.correction.contains(ECRIS_2), "{refus}");
}

#[test]
fn un_fichier_qui_n_est_pas_de_l_utf_8_est_refuse_en_7015_avec_son_octet() {
    let refus = refuse(b"\xFF\xFEr\x00e\x00");
    assert!(refus.preuve.contains("starts with the bytes FF FE, the mark of a UTF-16 text"), "{refus}");
    assert!(refus.correction.contains("Set-Content -Encoding utf8"), "{refus}");
    assert!(refuse(b"\xFE\xFFr").preuve.contains("FE FF"));
    let refus = refuse(b"remarques 1\n\nremarque 1\nfichier : \xC3(\n");
    assert!(refus.preuve.contains("the byte at position 35 (0xC3), line 4, is not UTF-8"), "{refus}");
    // Le rang compte le BOM.
    let refus = refuse(b"\xEF\xBB\xBFremarques 1\n\xC3");
    assert!(refus.preuve.contains("the byte at position 16 (0xC3), line 2"), "{refus}");
    // Un fichier coupé au milieu d'un caractère.
    let coupe = &"remarques 1\né".as_bytes()[..13];
    assert!(refuse(coupe).preuve.contains("is not UTF-8"));
}

#[test]
fn un_fichier_de_plus_de_1_mo_ou_de_plus_de_10_000_remarques_est_refuse() {
    // Plus de 1 Mo : refusé sans être lu, que le contenu soit juste ou non.
    let trop = vec![b'a'; TAILLE_MAX + 1];
    assert_eq!(refuse(&trop).preuve, "the remarks file is over 1 MB: more than 1000000 bytes");
    // Pile 1 Mo, il se lit : un texte de remarque long d'un million d'octets moins son en-tête.
    let tete = "remarques 1\n\nremarque 1\nfichier : a\nbloquante : non\n    ";
    let pile = format!("{tete}{}", "x".repeat(TAILLE_MAX - tete.len()));
    assert_eq!(pile.len(), TAILLE_MAX);
    assert_eq!(lire_remarques(pile.as_bytes()).unwrap().len(), 1);
    // 10 000 remarques se lisent, la 10 001e est refusée.
    let n_remarques = |n: usize| {
        (1..=n).fold(String::from("remarques 1\n"), |brut, k| {
            brut + &format!("remarque {k}\nfichier : a\nbloquante : non\n    x\n")
        })
    };
    let dix_mille = n_remarques(NOMBRE_MAX);
    assert!(dix_mille.len() < TAILLE_MAX);
    let lues = lire_remarques(dix_mille.as_bytes()).unwrap();
    assert_eq!((lues.len(), lues[9_999].texte.as_str()), (NOMBRE_MAX, "x"));
    let refus = refuse(n_remarques(NOMBRE_MAX + 1));
    let trop = "this is remark 10001, and a remarks file has 10000 at most";
    assert!(refus.preuve.contains(trop), "{refus}");
}

#[test]
fn la_lecture_ne_panique_sur_aucune_entree() {
    let juste = texte(&[
        "remarques 1",
        "",
        "remarque 1",
        "fichier : src/é.rs",
        "ligne : 7",
        "bloquante : oui",
        "\t  Un constat, é, \u{1f980}.",
        "",
        "\t  Sa correction.",
        "remarque 2",
        "fichier : a",
        "bloquante : non",
        "  x",
    ]);
    assert_eq!(lire_remarques(juste.as_bytes()).unwrap().len(), 2);
    let juge = |octets: &[u8]| {
        if let Err(refus) = lire_remarques(octets) {
            assert_eq!(refus.code, Code::RemarquesIllisibles);
            assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
        }
    };
    // Chaque début du fichier juste, à chaque octet, coupé en plein caractère ou non ; puis chaque octet remplacé.
    let octets = juste.as_bytes();
    for fin in 0..=octets.len() {
        juge(&octets[..fin]);
    }
    for i in 0..octets.len() {
        for remplace in [0u8, b'\n', b'\r', b' ', b'\t', b':', 0x80, 0xC3, 0xFF, b'9'] {
            let mut mute = octets.to_vec();
            mute[i] = remplace;
            juge(&mute);
        }
    }
    // Des mots tirés au hasard (un tirage fixe) parmi ceux du format et des pièges.
    let mots = [
        "remarques 1",
        "remarque",
        "remarque 1",
        "remarque 2",
        "fichier",
        "ligne",
        "bloquante",
        "oui",
        "non",
        ":",
        " : ",
        "12",
        "-1",
        "0",
        "\n",
        "\r\n",
        "\r",
        "\t",
        "  ",
        "    ",
        "é",
        "\u{feff}",
        "\u{1b}",
        "\u{0}",
        "a/b",
        "..",
        "\\",
        "/",
        "C:",
        "\u{1f600}",
    ];
    let mut etat = 0x9E37_79B9_7F4A_7C15_u64;
    let mut suivant = move || {
        etat ^= etat << 13;
        etat ^= etat >> 7;
        etat ^= etat << 17;
        etat
    };
    for _ in 0..3_000 {
        let n = 1 + (suivant() % 40) as usize;
        let brut: String = (0..n).map(|_| mots[(suivant() % mots.len() as u64) as usize]).collect();
        juge(brut.as_bytes());
        juge(format!("remarques 1\n{brut}").as_bytes());
        juge(format!("remarques 1\n\nremarque 1\nfichier : a\n{brut}").as_bytes());
    }
}

#[test]
fn la_ligne_fautive_est_citee_sans_piege_ni_demesure() {
    // La preuve cite la ligne fautive : ses caractères de contrôle écrits, et coupée à 200 caractères.
    let refus = refuse(format!("remarques 1\n\u{1b}[2J{}\n", "x".repeat(500)));
    assert!(refus.preuve.contains("\\u{1b}[2J"), "{refus}");
    assert!(!refus.preuve.contains('\u{1b}') && refus.preuve.contains('…') && refus.preuve.len() < 400, "{refus}");
}
