use super::*;

/// Ce que `lire` rend d'une ligne écrite à partir de ces champs : chaque valeur est plate, donc en `Some`.
fn lus(champs: &[(&str, Valeur)]) -> BTreeMap<String, Option<Valeur>> {
    champs.iter().map(|(cle, valeur)| (cle.to_string(), Some(valeur.clone()))).collect()
}

fn texte(valeur: &str) -> Option<Valeur> {
    Some(Valeur::Texte(valeur.into()))
}

/// Deux lignes de `niveaux` niveaux d'imbrication en tout, l'objet de la ligne compris : en tableaux, puis en objets.
fn profondes(niveaux: usize) -> [String; 2] {
    let dedans = niveaux - 1;
    [
        format!(r#"{{"v":2,"p":{}{}}}"#, "[".repeat(dedans), "]".repeat(dedans)),
        format!(r#"{{"v":2,"p":{}1{}}}"#, r#"{"p":"#.repeat(dedans), "}".repeat(dedans)),
    ]
}

#[test]
fn un_objet_plat_fait_l_aller_retour() {
    // La forme exacte : compacte, les clés dans l'ordre donné, un texte échappé selon la RFC 8259.
    let plat = [
        ("v", Valeur::Entier(2)),
        ("t", Valeur::Texte("é\u{1F600} \"q\" \\ / \t\r\n \u{0}\u{1f}".into())),
        ("n", Valeur::Nul),
        ("b", Valeur::Booleen(true)),
        ("f", Valeur::Booleen(false)),
        ("u", Valeur::Entier(u64::MAX)),
    ];
    let attendu =
        r#"{"v":2,"t":"é😀 \"q\" \\ / \t\r\n \u0000\u001f","n":null,"b":true,"f":false,"u":18446744073709551615}"#;
    assert_eq!(ecrire(&plat), attendu);
    assert_eq!(lire(attendu).unwrap(), lus(&plat));
    assert_eq!(ecrire(&[]), "{}");
    assert!(lire("{}").unwrap().is_empty());

    // Tous ces caractères d'une traite : les contrôles, l'ASCII, les accents, les bords des substituts, U+1F600 et
    // les derniers du dernier plan. Puis guillemets, barres, `\n` restés écrits, tabulations et retours.
    let tous: String = (0u32..0x300)
        .chain(0xD7F0..0xE010)
        .chain(0x1_F600..0x1_F610)
        .chain(0x10_FFF0..0x11_0000)
        .filter_map(char::from_u32)
        .collect();
    let champs = [
        ("seance", Valeur::Texte("1761901200000_7_0".into())),
        ("tous", Valeur::Texte(tous)),
        ("texte", Valeur::Texte("« \"q\" », \\n resté écrit, \\\\ \\\"\ttab\r\nretour\n\u{0}".into())),
        ("vide", Valeur::Texte(String::new())),
        ("zero", Valeur::Entier(0)),
        ("é\"\\\n", Valeur::Nul),
    ];
    let ligne = ecrire(&champs);
    assert!(ligne.chars().all(|c| c >= ' '), "une ligne sans contrôle brut : {ligne:?}");
    assert_eq!(lire(&ligne).unwrap(), lus(&champs));
}

#[test]
fn une_ligne_d_un_autre_ecrivain_se_lit() {
    // Des blancs de toute sorte entre les éléments, un `\r` final (une ligne CRLF), les clés dans le désordre,
    // `\u00e9`, deux paires de substitution (U+1F600, U+1D11E) en majuscules ou minuscules, `\/` ; puis un objet,
    // un tableau et des nombres qui ne sont pas des entiers.
    let ligne = [
        "\t {  ",
        r#""seq" : 3 ,"#,
        "\r\n",
        r#""v":2 , "texte":"caf\u00e9 \ud83d\ude00 \uD834\uDD1E \uD83d\uDE00 a\/b \"q\" "#,
        r#"\\ \u0022 \u005C fin\n\b\f\t\r","#,
        r#"  "nul" : null,"vrai":true ,"faux":false,"#,
        "\n",
        r#""é":"é","vide":"","zero":0,"max":18446744073709551615,"nulle":"a\u0000b","brut":"\u007f","#,
        r#""bornes":"\ud800\udc00 \udbff\udfff \ud7ff \ue000 \uffff","#,
        r#""objet":{"a":[1,2,{"b":null,"b":[]}],"a":"x","":{}},"#,
        r#""liste":[ 1 , "x" ,[true,false,null],{},[]],"#,
        r#""flottant":1.0,"negatif":-1,"exposant":1e3,"majuscule":2E+2,"moins_zero":-0,"decimal":0.5e-2,"#,
        r#""trop_grand":18446744073709551616,"zero_decimal":0.0"#,
        " } \r",
    ]
    .concat();
    let attendu: BTreeMap<String, Option<Valeur>> = [
        ("seq", Some(Valeur::Entier(3))),
        ("v", Some(Valeur::Entier(2))),
        ("texte", texte("caf\u{e9} \u{1F600} \u{1D11E} \u{1F600} a/b \"q\" \\ \" \\ fin\n\u{8}\u{c}\t\r")),
        ("nul", Some(Valeur::Nul)),
        ("vrai", Some(Valeur::Booleen(true))),
        ("faux", Some(Valeur::Booleen(false))),
        ("é", texte("é")),
        ("vide", texte("")),
        ("zero", Some(Valeur::Entier(0))),
        ("max", Some(Valeur::Entier(u64::MAX))),
        ("nulle", texte("a\u{0}b")),
        ("brut", texte("\u{7f}")),
        ("bornes", texte("\u{10000} \u{10FFFF} \u{D7FF} \u{E000} \u{FFFF}")),
        ("objet", None),
        ("liste", None),
        ("flottant", None),
        ("negatif", None),
        ("exposant", None),
        ("majuscule", None),
        ("moins_zero", None),
        ("decimal", None),
        ("trop_grand", None),
        ("zero_decimal", None),
    ]
    .into_iter()
    .map(|(cle, valeur)| (cle.to_string(), valeur))
    .collect();
    assert_eq!(lire(&ligne).unwrap(), attendu);

    // Ce que la RFC laisse passer tel quel dans une chaîne : U+007F, un contrôle C1, un séparateur de ligne Unicode.
    let brut = lire("{\"a\":\"\u{7f}\u{85}\u{2028}\u{1F600}\"}").unwrap();
    assert_eq!(brut["a"], texte("\u{7f}\u{85}\u{2028}\u{1F600}"));
    // Des blancs seuls autour d'un objet vide.
    assert!(lire(" \t\r\n{ \r\n} \r\n").unwrap().is_empty());
    // `v`, `seance` et `seq` se lisent à côté de valeurs imbriquées, quel que soit leur ordre ; ceux qui y sont
    // imbriqués ne comptent pas.
    let imbrique = lire(r#"{"x":{"v":9,"seq":[1]},"seance":"s","v":2,"seq":7,"y":[{"v":1}]}"#).unwrap();
    assert_eq!(imbrique["v"], Some(Valeur::Entier(2)));
    assert_eq!(imbrique["seq"], Some(Valeur::Entier(7)));
    assert_eq!(imbrique["seance"], texte("s"));
    assert_eq!((&imbrique["x"], &imbrique["y"]), (&None, &None));
    // 64 niveaux, l'objet de la ligne compris : lus jusqu'au bout, puis oubliés.
    for ligne in profondes(64) {
        assert_eq!(lire(&ligne).unwrap()["p"], None, "{ligne}");
    }
}

#[test]
fn chaque_ligne_hors_grammaire_est_refusee() {
    let [tableaux, objets] = profondes(65);
    // Chaque ligne avec un bout de son refus : elle l'est pour la raison que la table donne.
    let cas = [
        // Une clé redite au premier niveau, avant comme après décodage.
        (r#"{"a":1,"a":2}"#, "clé redite : \"a\""),
        (r#"{"a":1,"b":3,"a":"x"}"#, "clé redite : \"a\""),
        (r#"{"a":1,"\u0061":2}"#, "clé redite : \"a\""),
        // Un nombre hors de la grammaire.
        (r#"{"a":01}"#, "attendu `,` ou `}`, vu '1'"),
        (r#"{"a":00}"#, "vu '0'"),
        (r#"{"a":-01}"#, "vu '1'"),
        (r#"{"a":1.}"#, "attendu un chiffre, vu '}'"),
        (r#"{"a":1.e3}"#, "attendu un chiffre, vu 'e'"),
        (r#"{"a":.5}"#, "attendu une valeur, vu '.'"),
        (r#"{"a":+1}"#, "attendu une valeur, vu '+'"),
        (r#"{"a":1e}"#, "attendu un chiffre"),
        (r#"{"a":1e+}"#, "attendu un chiffre"),
        (r#"{"a":-}"#, "attendu un chiffre"),
        (r#"{"a":- 1}"#, "attendu un chiffre"),
        (r#"{"a":0x10}"#, "vu 'x'"),
        (r#"{"a":1_000}"#, "vu '_'"),
        (r#"{"a":NaN}"#, "attendu une valeur"),
        (r#"{"a":Infinity}"#, "attendu une valeur"),
        (r#"{"a":[01]}"#, "attendu `,` ou `]`, vu '1'"),
        // Un substitut seul.
        (r#"{"a":"\ud83d"}"#, "substitut seul \\ud83d"),
        (r#"{"a":"\ude00"}"#, "substitut seul \\ude00"),
        (r#"{"a":"\ud83dx"}"#, "substitut seul"),
        (r#"{"a":"\ud83d\u0041"}"#, "substitut seul"),
        (r#"{"a":"\ud83d\ud83d"}"#, "substitut seul"),
        (r#"{"a":"\ude00\ud83d"}"#, "substitut seul \\ude00"),
        (r#"{"a":"\ud83d\n"}"#, "substitut seul"),
        (r#"{"a":"\ud83d\\ude00"}"#, "substitut seul"),
        (r#"{"a":"\uDBFF"}"#, "substitut seul \\udbff"),
        (r#"{"a":"\uDC00"}"#, "substitut seul \\udc00"),
        (r#"{"a":"\ud83d\ue000"}"#, "substitut seul"),
        (r#"{"a":"\ud83d\udbff"}"#, "substitut seul"),
        (r#"{"a":"\ud800"}"#, "substitut seul \\ud800"),
        (r#"{"a":"\udfff"}"#, "substitut seul \\udfff"),
        // Un contrôle brut, dans une chaîne, une clé ou un tableau.
        ("{\"a\":\"x\ty\"}", "contrôle brut '\\t'"),
        ("{\"a\":\"x\ny\"}", "contrôle brut '\\n'"),
        ("{\"a\":\"x\ry\"}", "contrôle brut '\\r'"),
        ("{\"a\":\"x\u{0}y\"}", "contrôle brut '\\0'"),
        ("{\"a\":\"x\u{1f}y\"}", "contrôle brut '\\u{1f}'"),
        ("{\"\t\":1}", "contrôle brut"),
        ("{\"a\":[\"\u{1}\"]}", "contrôle brut"),
        // Une suite après `}`, hors blancs.
        (r#"{"a":1}x"#, "après `}`, vu 'x'"),
        (r#"{"a":1} x"#, "après `}`, vu 'x'"),
        (r#"{"a":1}}"#, "après `}`, vu '}'"),
        (r#"{"a":1}{"b":2}"#, "après `}`, vu '{'"),
        ("{\"a\":1}\n{\"b\":2}", "après `}`, vu '{'"),
        ("{\"a\":1}\u{0}", "après `}`"),
        ("{\"a\":1}\u{a0}", "après `}`"),
        ("{\"a\":1}\u{b}", "après `}`"),
        (r#"{"a":1}//"#, "après `}`"),
        (r#"{"a":1} /* */"#, "après `}`"),
        // Une virgule en trop ou en moins.
        (r#"{"a":1,}"#, "attendu une clé (une chaîne), vu '}'"),
        (r#"{"a":1 ,}"#, "attendu une clé"),
        (r#"{,}"#, "attendu une clé"),
        (r#"{"a":1,,"b":2}"#, "attendu une clé"),
        (r#"{"a":[1,]}"#, "attendu une valeur, vu ']'"),
        (r#"{"a":[,1]}"#, "attendu une valeur, vu ','"),
        (r#"{"a":{"b":1,}}"#, "attendu une clé"),
        (r#"{"a":1 "b":2}"#, "attendu `,` ou `}`, vu '\"'"),
        (r#"{"a":[1 2]}"#, "attendu `,` ou `]`, vu '2'"),
        // Un guillemet qui manque.
        (r#"{"a:1}"#, "vu la fin de la ligne"),
        (r#"{"a":"x}"#, "attendu `\"` fermant, vu la fin de la ligne"),
        (r#"{"a":"x\"}"#, "vu la fin de la ligne"),
        (r#"{a":1}"#, "attendu une clé"),
        (r#"{'a':1}"#, "attendu une clé"),
        (r#"{"a":x"}"#, "attendu une valeur"),
        (r#"{"a":'x'}"#, "attendu une valeur"),
        // Un échappement faux.
        (r#"{"a":"\x"}"#, "attendu un échappement"),
        (r#"{"a":"\'"}"#, "attendu un échappement"),
        (r#"{"a":"\U0041"}"#, "attendu un échappement"),
        (r#"{"a":"\"#, "ou `u`), vu la fin de la ligne"),
        (r#"{"a":"\u12"}"#, "attendu un chiffre hexadécimal, vu '\"'"),
        (r#"{"a":"\u12G4"}"#, "vu 'G'"),
        (r#"{"a":"\u+123"}"#, "vu '+'"),
        (r#"{"a":"\u-123"}"#, "vu '-'"),
        (r#"{"a":"\u00é9"}"#, "vu 'é'"),
        (r#"{"a":"\u"#, "vu la fin de la ligne"),
        (r#"{"a":"\ud83d\uZZZZ"}"#, "attendu un chiffre hexadécimal, vu 'Z'"),
        // Des mots qui ne sont pas des valeurs.
        (r#"{"a":tru}"#, "attendu `true`"),
        (r#"{"a":True}"#, "attendu une valeur"),
        (r#"{"a":fals}"#, "attendu `false`"),
        (r#"{"a":nul}"#, "attendu `null`"),
        (r#"{"a":nulll}"#, "vu 'l'"),
        (r#"{"a":truefalse}"#, "vu 'f'"),
        (r#"{"a":undefined}"#, "attendu une valeur"),
        // Des valeurs imbriquées mal formées : lues jusqu'au bout, donc refusées.
        (r#"{"a":[}"#, "attendu une valeur, vu '}'"),
        (r#"{"a":[1}"#, "attendu `,` ou `]`, vu '}'"),
        (r#"{"a":{"b":1]}"#, "attendu `,` ou `}`, vu ']'"),
        (r#"{"a":{"b"}}"#, "attendu `:`, vu '}'"),
        (r#"{"a":{"b":}}"#, "attendu une valeur"),
        (r#"{"a":{1:2}}"#, "attendu une clé"),
        (r#"{"a":["\ud83d"]}"#, "substitut seul"),
        (r#"{"a":[[[[1}"#, "attendu `,` ou `]`"),
        // Pas un objet, ou pas fini.
        ("", "attendu un objet `{`, vu la fin de la ligne"),
        (" \t\r\n", "attendu un objet `{`, vu la fin de la ligne"),
        ("[]", "attendu un objet `{`, vu '['"),
        (r#"[{"a":1}]"#, "attendu un objet `{`, vu '['"),
        ("\"a\"", "attendu un objet `{`, vu '\"'"),
        ("1", "attendu un objet `{`, vu '1'"),
        ("null", "attendu un objet `{`, vu 'n'"),
        ("\u{feff}{\"a\":1}", "attendu un objet `{`, vu '\\u{feff}'"),
        ("\u{a0}{\"a\":1}", "attendu un objet `{`"),
        ("\u{b}{\"a\":1}", "attendu un objet `{`"),
        ("{", "attendu une clé (une chaîne), vu la fin de la ligne"),
        ("{ ", "attendu une clé"),
        (r#"{"a""#, "attendu `:`, vu la fin de la ligne"),
        (r#"{"a" 1}"#, "attendu `:`, vu '1'"),
        (r#"{"a":"#, "attendu une valeur, vu la fin de la ligne"),
        (r#"{"a":}"#, "attendu une valeur, vu '}'"),
        (r#"{"a":1"#, "attendu `,` ou `}`, vu la fin de la ligne"),
        (r#"{"a":1,"#, "attendu une clé"),
        ("{\u{b}\"a\":1}", "attendu une clé"),
        // Plus de 64 niveaux : l'objet de la ligne est le premier, le 65e est de trop.
        (tableaux.as_str(), "plus de 64 niveaux d'imbrication"),
        (objets.as_str(), "plus de 64 niveaux d'imbrication"),
    ];
    for (ligne, bout) in cas {
        let erreur = lire(ligne).expect_err(ligne);
        assert!(erreur.contains(bout), "{ligne:?} devait être refusée par « {bout} », et l'est par : {erreur}");
    }
    // Le refus entier, avec le rang du caractère fautif, compté en caractères et non en octets.
    let refus = |ligne: &str| lire(ligne).unwrap_err();
    assert_eq!(refus(r#"{"é":1,}"#), "au caractère 8 : attendu une clé (une chaîne), vu '}'");
    assert_eq!(
        refus("{\"a\":\"😀\u{1}\"}"),
        "contrôle brut '\\u{1}' dans une chaîne (à écrire \\u00xx), au caractère 8"
    );
    assert_eq!(refus(r#"{"a":"😀\ud83d"}"#), "substitut seul \\ud83d, au caractère 8");
    assert_eq!(refus(&tableaux), "plus de 64 niveaux d'imbrication, au caractère 75");
    assert_eq!(refus(r#"{"a":1}x"#), "au caractère 8 : attendu la fin de la ligne après `}`, vu 'x'");
    assert_eq!(refus(""), "au caractère 1 : attendu un objet `{`, vu la fin de la ligne");
}

#[test]
fn aucune_coupe_ne_fait_paniquer() {
    let ligne = concat!(
        r#"{"v":2,"texte":"caf\u00e9 \ud83d\ude00 \uD834\uDD1E a\/b \"q\" \\ fin\n","é😀":"é😀 𝄞 ","#,
        r#""nombre":-1.5e+3,"objet":{"a":[1,{"b":null}],"c":"d"},"liste":[true,false,null,"x",0],"#,
        r#""max":18446744073709551615,"vide":{},"rien":[]}"#
    );
    assert_eq!(lire(ligne).unwrap().len(), 9);
    // Chaque préfixe, coupé sur une frontière de caractère : une ligne qui ne finit pas est refusée.
    for (i, _) in ligne.char_indices() {
        let coupe = &ligne[..i];
        assert!(lire(coupe).is_err(), "un préfixe n'est pas une ligne : {coupe:?}");
    }
    // Chaque caractère remplacé à son tour, par ce qui trouble le plus un lecteur : lu ou refusé, sans paniquer.
    let remplacants = ['"', '\\', '{', '[', ']', '}', ',', ':', '-', '0', 'e', 'u', '\u{1}', 'é', '\u{1F600}'];
    for (i, c) in ligne.char_indices() {
        for remplacant in remplacants {
            let coupe = format!("{}{remplacant}{}", &ligne[..i], &ligne[i + c.len_utf8()..]);
            let _ = lire(&coupe);
        }
    }
    // Une pile de 100 000 `[`, ou d'objets et de tableaux : refusée au 65e niveau, sans descendre jusqu'au fond.
    assert!(lire(&"[".repeat(100_000)).is_err());
    for ouvrant in [r#"{"p":"#, r#"{"p":["#, "["] {
        let ligne = format!(r#"{{"v":2,"p":{}"#, ouvrant.repeat(100_000));
        let erreur = lire(&ligne).unwrap_err();
        assert!(erreur.contains("plus de 64 niveaux d'imbrication"), "{erreur}");
    }
    let fermees = format!(r#"{{"p":{}{}}}"#, "[".repeat(100_000), "]".repeat(100_000));
    assert!(lire(&fermees).is_err());
}

#[test]
fn chaine_echappe_selon_la_rfc() {
    assert_eq!(chaine(""), r#""""#);
    assert_eq!(chaine("abc"), r#""abc""#);
    assert_eq!(chaine(r#"a"b"#), r#""a\"b""#);
    assert_eq!(chaine(r"a\b"), r#""a\\b""#);
    // La barre oblique ne s'échappe pas ; ni U+007F, ni un contrôle C1, ni un séparateur de ligne Unicode.
    assert_eq!(chaine("a/b"), r#""a/b""#);
    assert_eq!(chaine("é😀𝄞\u{7f}\u{85}\u{2028}"), "\"é😀𝄞\u{7f}\u{85}\u{2028}\"");
    // Les contrôles : cinq en deux caractères, les autres en `\u00xx`, en minuscules.
    assert_eq!(chaine("\n\r\t\u{8}\u{c}"), r#""\n\r\t\b\f""#);
    assert_eq!(chaine("\u{0}\u{1}\u{b}\u{e}\u{1f}"), r#""\u0000\u0001\u000b\u000e\u001f""#);
    // Chaque caractère de l'ASCII : seuls `"`, `\` et U+0000 à U+001F changent, et la chaîne se relit.
    for code in 0u8..0x80 {
        let c = char::from(code);
        let json = chaine(&c.to_string());
        assert!(json.chars().all(|c| c >= ' '), "aucun contrôle brut : {json:?}");
        if c < ' ' || c == '"' || c == '\\' {
            assert!(json.len() > 3 && json[1..].starts_with('\\'), "{c:?} s'échappe : {json}");
        } else {
            assert_eq!(json, format!("\"{c}\""), "{c:?} passe tel quel");
        }
        assert_eq!(lire(&format!("{{\"k\":{json}}}")).unwrap()["k"], texte(&c.to_string()), "{json}");
    }
}

/// L'échappement JSON `\u` de ce code à quatre chiffres hexadécimaux.
fn u(code: &str) -> String {
    format!("\\u{code}")
}

#[test]
fn un_document_garde_ses_valeurs_imbriquees() {
    let texte = |t: &str| Json::Texte(t.into());
    let nombre = |n: &str| Json::Nombre(n.into());
    let objet = |membres: Vec<(&str, Json)>| {
        Json::Objet(membres.into_iter().map(|(cle, valeur)| (cle.to_string(), valeur)).collect())
    };
    // Des objets et des tableaux imbriqués, des nombres gardés tels qu'écrits, des blancs de toute sorte, des
    // accents bruts ou échappés, une paire de substituts (U+1F600) et le même caractère brut.
    let ligne = r#" {"a" : [1, -0, -2.50, 1E+3, 0.5e-2, 18446744073709551616, {"b":null,"c":[true,false,[]]}],
        "é":"caf<e> <rire> 😀 \"q\" \\ \/","o":{},"":""}
        "#
    .replace("<e>", &u("00e9"))
    .replace("<rire>", &(u("d83d") + &u("de00")));
    let attendu = objet(vec![
        (
            "a",
            Json::Liste(vec![
                nombre("1"),
                nombre("-0"),
                nombre("-2.50"),
                nombre("1E+3"),
                nombre("0.5e-2"),
                nombre("18446744073709551616"),
                objet(vec![
                    ("b", Json::Nul),
                    ("c", Json::Liste(vec![Json::Booleen(true), Json::Booleen(false), Json::Liste(vec![])])),
                ]),
            ]),
        ),
        ("é", texte("café 😀 😀 \"q\" \\ /")),
        ("o", objet(vec![])),
        ("", texte("")),
    ]);
    assert_eq!(document(&ligne), Ok(attendu));
    // Un document est une valeur, quelle qu'elle soit, entourée de blancs.
    assert_eq!(document(" [ ] \r\n"), Ok(Json::Liste(vec![])));
    assert_eq!(document(r#""x""#), Ok(texte("x")));
    assert_eq!(document("-1.5e+3"), Ok(nombre("-1.5e+3")));
    assert_eq!(document("null"), Ok(Json::Nul));
    // Une clé redite est refusée à tout niveau, comparée une fois décodée. `lire` ne la refuse qu'au premier, et
    // oublie l'imbriqué.
    let redites = [
        r#"{"x":{"b":1,"b":2}}"#.to_string(),
        r#"[{"c":[{"d":1,"d":[]}]}]"#.to_string(),
        format!(r#"{{"x":{{"é":1,"{}":2}}}}"#, u("00e9")),
    ];
    for (redite, cle) in redites.iter().zip(["b", "d", "é"]) {
        assert_eq!(document(redite), Err(format!("clé redite : {cle:?}")), "{redite}");
    }
    for redite in [&redites[0], &redites[2]] {
        assert!(lire(redite).is_ok(), "{redite}");
    }
    // Ce que la RFC 8259 n'écrit pas est refusé comme par `lire`, avec le caractère fautif.
    let refus = |texte: &str| document(texte).unwrap_err();
    assert_eq!(refus(""), "au caractère 1 : attendu une valeur, vu la fin de la ligne");
    assert_eq!(refus(r#"{"a":1}x"#), "au caractère 8 : attendu la fin du document, vu 'x'");
    assert_eq!(refus("[01]"), "au caractère 3 : attendu `,` ou `]`, vu '1'");
    assert_eq!(refus(r#"{"a":[1,]}"#), "au caractère 9 : attendu une valeur, vu ']'");
    assert_eq!(refus(&format!(r#"["{}"]"#, u("d83d"))), format!("substitut seul {}, au caractère 3", u("d83d")));
    assert_eq!(refus("[\"\t\"]"), "contrôle brut '\\t' dans une chaîne (à écrire \\u00xx), au caractère 3");
    // 64 niveaux se lisent et le 65e est de trop, en objets comme en tableaux, que le premier soit l'objet d'une
    // ligne ou un tableau.
    let tableaux = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    for ligne in profondes(64).into_iter().chain([tableaux(64)]) {
        assert!(document(&ligne).is_ok(), "{ligne}");
    }
    for ligne in profondes(65).into_iter().chain([tableaux(65)]) {
        let erreur = document(&ligne).unwrap_err();
        assert!(erreur.starts_with("plus de 64 niveaux d'imbrication, au caractère "), "{erreur}");
    }
}

#[test]
fn aucune_coupe_d_un_document_ne_fait_paniquer() {
    let ligne = concat!(
        r#"{"v":2,"texte":"caf<e> <rire> a\/b \"q\" \\ fin\n","é😀":"é😀 𝄞 ","nombre":-1.5e+3,"#,
        r#""objet":{"a":[1,{"b":null}],"c":"d"},"liste":[true,false,null,"x",0,[[{}]]],"#,
        r#""max":18446744073709551616,"vide":{},"rien":[]}"#
    )
    .replace("<e>", &u("00e9"))
    .replace("<rire>", &(u("d83d") + &u("de00")));
    assert!(matches!(document(&ligne), Ok(Json::Objet(objet)) if objet.len() == 9), "{ligne}");
    // Chaque préfixe, coupé sur une frontière de caractère : un document qui ne finit pas est refusé.
    for (i, _) in ligne.char_indices() {
        let coupe = &ligne[..i];
        assert!(document(coupe).is_err(), "un préfixe n'est pas un document : {coupe:?}");
    }
    // Chaque caractère remplacé à son tour, par ce qui trouble le plus un lecteur : lu ou refusé, sans paniquer.
    let remplacants = ['"', '\\', '{', '[', ']', '}', ',', ':', '-', '0', 'e', 'u', '\u{1}', 'é', '\u{1F600}'];
    for (i, c) in ligne.char_indices() {
        for remplacant in remplacants {
            let coupe = format!("{}{remplacant}{}", &ligne[..i], &ligne[i + c.len_utf8()..]);
            let _ = document(&coupe);
        }
    }
    // Une pile de 100 000 ouvrants, tableaux, objets ou les deux : refusée au 65e niveau, sans descendre au fond.
    for ouvrant in ["[", r#"{"p":"#, r#"[{"p":"#] {
        let erreur = document(&ouvrant.repeat(100_000)).unwrap_err();
        assert!(erreur.starts_with("plus de 64 niveaux d'imbrication"), "{erreur}");
    }
}
