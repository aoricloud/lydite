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
        (String::new(), "ligne 1 «  » : la première ligne d'un fichier de remarques est `remarques 1`"),
        ("remarques 2\n".into(), "ligne 1 « remarques 2 » : la première ligne"),
        ("plan 1\n".into(), "ligne 1 « plan 1 » : la première ligne"),
        ("remarques 1 \n".into(), "ligne 1 « remarques 1  » : la première ligne"),
        ("\nremarques 1\n".into(), "ligne 1 «  » : la première ligne"),
        // Une ligne hors de la forme.
        (debut("bonjour\n"), "ligne 2 « bonjour » : elle n'est ni vide, ni indentée, ni `remarque <n>`"),
        (debut("remarques 1\n"), "ligne 2 « remarques 1 » : elle n'est ni vide"),
        (debut("remarque1\n"), "ligne 2 « remarque1 » : elle n'est ni vide"),
        (debut("    texte\n"), "ligne 2 «     texte » : cette ligne est indentée, et aucune ligne `remarque <n>`"),
        (debut("fichier : a\n"), "la clé « fichier » ne suit aucune ligne `remarque <n>`"),
        (debut("auteur : moi\n"), "la clé « auteur » n'est pas une clé de remarque (fichier, ligne, bloquante)"),
        // La numérotation : 1, 2, 3… dans l'ordre.
        (debut("remarque\n"), "la remarque porte le numéro «  », et la suivante est la 1"),
        (debut("remarque deux\n"), "la remarque porte le numéro « deux », et la suivante est la 1"),
        (debut("remarque 2\n"), "la remarque porte le numéro « 2 », et la suivante est la 1"),
        (debut("remarque 0\n"), "la remarque porte le numéro « 0 »"),
        (debut("remarque -1\n"), "la remarque porte le numéro « -1 »"),
        (debut("remarque +1\n"), "la remarque porte le numéro « +1 »"),
        (debut("remarque 1 2\n"), "la remarque porte le numéro « 1 2 »"),
        (debut("remarque 99999999999999999999999\n"), "le numéro « 99999999999999999999999 »"),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n x\nremarque 3\n"),
            "ligne 6 « remarque 3 » : la remarque porte le numéro « 3 », et la suivante est la 2",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n x\nremarque 1\n"),
            "la remarque porte le numéro « 1 », et la suivante est la 2",
        ),
        // L'ordre des clés : fichier, ligne si elle paraît, bloquante, une fois chacune.
        (debut("remarque 1\nbloquante : oui\n"), "la clé « bloquante » vient avant `fichier`"),
        (debut("remarque 1\nligne : 1\n"), "la clé « ligne » vient avant `fichier`"),
        (debut("remarque 1\nfichier : a\nfichier : b\n"), "la clé « fichier » est déjà donnée dans cette remarque"),
        (debut("remarque 1\nfichier : a\nligne : 1\nligne : 2\n"), "la clé « ligne » est déjà donnée"),
        (debut("remarque 1\nfichier : a\nbloquante : non\nbloquante : oui\n"), "la clé « bloquante » est déjà donnée"),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\nligne : 1\n"),
            "la clé « ligne » vient après `bloquante`, la dernière clé",
        ),
        (
            debut("remarque 1\nfichier : a\nligne : 1\nfichier : b\n"),
            "la clé « fichier » est déjà donnée dans cette remarque",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : non\n    x\nbloquante : oui\n"),
            "la clé « bloquante » vient après les lignes indentées",
        ),
        // Ce qui manque à une remarque.
        (debut("remarque 1\n"), "ligne 2 « remarque 1 » : la remarque n'a pas de ligne `fichier : …`"),
        (debut("remarque 1\nfichier : a\n"), "ligne 2 « remarque 1 » : la remarque n'a pas de ligne `bloquante : …`"),
        (
            debut("remarque 1\nfichier : a\nbloquante : oui\n"),
            "la remarque n'a pas de texte : son constat et sa correction, en lignes indentées",
        ),
        (
            debut("remarque 1\nfichier : a\nbloquante : oui\n   \n\t\nremarque 2\n"),
            "ligne 2 « remarque 1 » : la remarque n'a pas de texte",
        ),
        (
            debut("remarque 1\nfichier : a\n    texte avant bloquante\n"),
            "le texte d'une remarque vient après ses clés, la dernière étant `bloquante`",
        ),
        // Les valeurs.
        (debut("remarque 1\nfichier :\n"), "la clé « fichier » est blanche"),
        (debut("remarque 1\nfichier : a\nligne :   \n"), "la clé « ligne » est blanche"),
        (debut("remarque 1\nfichier : a\nbloquante :\n"), "la clé « bloquante » est blanche"),
        (
            debut("remarque 1\nfichier : a\nligne : 0\n"),
            "la clé « ligne » vaut « 0 », et sa valeur est un entier positif",
        ),
        (debut("remarque 1\nfichier : a\nligne : -3\n"), "la clé « ligne » vaut « -3 »"),
        (debut("remarque 1\nfichier : a\nligne : 1.5\n"), "la clé « ligne » vaut « 1.5 »"),
        (debut("remarque 1\nfichier : a\nligne : douze\n"), "la clé « ligne » vaut « douze »"),
        (debut("remarque 1\nfichier : a\nligne : ٣\n"), "la clé « ligne » vaut « ٣ »"),
        (debut("remarque 1\nfichier : a\nligne : 4294967296\n"), "la clé « ligne » vaut « 4294967296 »"),
        (
            debut("remarque 1\nfichier : a\nbloquante : Oui\n"),
            "la clé « bloquante » vaut « Oui », et ses seules valeurs sont `oui` et `non`",
        ),
        (debut("remarque 1\nfichier : a\nbloquante : peut-être\n"), "la clé « bloquante » vaut « peut-être »"),
        (debut("remarque 1\nfichier : a\nbloquante : oui non\n"), "la clé « bloquante » vaut « oui non »"),
        // Un chemin relatif à la racine du dépôt, écrit comme git l'écrit.
        (debut("remarque 1\nfichier : C:\\x\\a.rs\n"), "le chemin « C:\\x\\a.rs » s'écrit avec une barre inverse"),
        (debut("remarque 1\nfichier : src\\a.rs\n"), "s'écrit avec une barre inverse, et git sépare les noms par `/`"),
        (debut("remarque 1\nfichier : /etc/passwd\n"), "le chemin « /etc/passwd » est absolu"),
        (debut("remarque 1\nfichier : C:/x\n"), "le chemin « C:/x » est absolu"),
        (debut("remarque 1\nfichier : ../a\n"), "le chemin « ../a » a un nom vide, `.` ou `..`"),
        (debut("remarque 1\nfichier : a/../b\n"), "a un nom vide, `.` ou `..`"),
        (debut("remarque 1\nfichier : a//b\n"), "a un nom vide, `.` ou `..`"),
        (debut("remarque 1\nfichier : ./a\n"), "a un nom vide, `.` ou `..`"),
        (debut("remarque 1\nfichier : a\tb\n"), "le chemin « a\\tb » porte un caractère de contrôle"),
        (debut("remarque 1\nfichier : a\u{1b}[31m\n"), "le chemin « a\\u{1b}[31m » porte un caractère de contrôle"),
    ];
    for (brut, bout) in cas {
        let refus = refuse(&brut);
        assert!(refus.preuve.contains(bout), "« {bout} » dans la preuve de {brut:?} : {refus}");
    }
}

#[test]
fn un_fichier_qui_n_est_pas_de_l_utf_8_est_refuse_en_7015_avec_son_octet() {
    let refus = refuse(b"\xFF\xFEr\x00e\x00");
    assert!(refus.preuve.contains("commence par les octets FF FE, la marque d'un texte en UTF-16"), "{refus}");
    assert!(refus.correction.contains("Set-Content -Encoding utf8"), "{refus}");
    assert!(refuse(b"\xFE\xFFr").preuve.contains("FE FF"));
    let refus = refuse(b"remarques 1\n\nremarque 1\nfichier : \xC3(\n");
    assert!(refus.preuve.contains("l'octet de rang 35 (0xC3), ligne 4, n'est pas de l'UTF-8"), "{refus}");
    // Le rang compte le BOM.
    let refus = refuse(b"\xEF\xBB\xBFremarques 1\n\xC3");
    assert!(refus.preuve.contains("l'octet de rang 16 (0xC3), ligne 2"), "{refus}");
    // Un fichier coupé au milieu d'un caractère.
    let coupe = &"remarques 1\né".as_bytes()[..13];
    assert!(refuse(coupe).preuve.contains("n'est pas de l'UTF-8"));
}

#[test]
fn un_fichier_de_plus_de_1_mo_ou_de_plus_de_10_000_remarques_est_refuse() {
    // Plus de 1 Mo : refusé sans être lu, que le contenu soit juste ou non.
    let trop = vec![b'a'; TAILLE_MAX + 1];
    assert_eq!(refuse(&trop).preuve, "le fichier de remarques dépasse 1 Mo : plus de 1000000 octets");
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
    let trop = "c'est la remarque 10001, et un fichier de remarques en compte 10000 au plus";
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
