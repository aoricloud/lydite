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
        (r#"{"a":1,"a":2}"#, "repeated key: \"a\""),
        (r#"{"a":1,"b":3,"a":"x"}"#, "repeated key: \"a\""),
        (r#"{"a":1,"\u0061":2}"#, "repeated key: \"a\""),
        // Un nombre hors de la grammaire.
        (r#"{"a":01}"#, "expected `,` or `}`, saw '1'"),
        (r#"{"a":00}"#, "saw '0'"),
        (r#"{"a":-01}"#, "saw '1'"),
        (r#"{"a":1.}"#, "expected a digit, saw '}'"),
        (r#"{"a":1.e3}"#, "expected a digit, saw 'e'"),
        (r#"{"a":.5}"#, "expected a value, saw '.'"),
        (r#"{"a":+1}"#, "expected a value, saw '+'"),
        (r#"{"a":1e}"#, "expected a digit"),
        (r#"{"a":1e+}"#, "expected a digit"),
        (r#"{"a":-}"#, "expected a digit"),
        (r#"{"a":- 1}"#, "expected a digit"),
        (r#"{"a":0x10}"#, "saw 'x'"),
        (r#"{"a":1_000}"#, "saw '_'"),
        (r#"{"a":NaN}"#, "expected a value"),
        (r#"{"a":Infinity}"#, "expected a value"),
        (r#"{"a":[01]}"#, "expected `,` or `]`, saw '1'"),
        // Un substitut seul.
        (r#"{"a":"\ud83d"}"#, "lone surrogate \\ud83d"),
        (r#"{"a":"\ude00"}"#, "lone surrogate \\ude00"),
        (r#"{"a":"\ud83dx"}"#, "lone surrogate"),
        (r#"{"a":"\ud83d\u0041"}"#, "lone surrogate"),
        (r#"{"a":"\ud83d\ud83d"}"#, "lone surrogate"),
        (r#"{"a":"\ude00\ud83d"}"#, "lone surrogate \\ude00"),
        (r#"{"a":"\ud83d\n"}"#, "lone surrogate"),
        (r#"{"a":"\ud83d\\ude00"}"#, "lone surrogate"),
        (r#"{"a":"\uDBFF"}"#, "lone surrogate \\udbff"),
        (r#"{"a":"\uDC00"}"#, "lone surrogate \\udc00"),
        (r#"{"a":"\ud83d\ue000"}"#, "lone surrogate"),
        (r#"{"a":"\ud83d\udbff"}"#, "lone surrogate"),
        (r#"{"a":"\ud800"}"#, "lone surrogate \\ud800"),
        (r#"{"a":"\udfff"}"#, "lone surrogate \\udfff"),
        // Un contrôle brut, dans une chaîne, une clé ou un tableau.
        ("{\"a\":\"x\ty\"}", "raw control '\\t'"),
        ("{\"a\":\"x\ny\"}", "raw control '\\n'"),
        ("{\"a\":\"x\ry\"}", "raw control '\\r'"),
        ("{\"a\":\"x\u{0}y\"}", "raw control '\\0'"),
        ("{\"a\":\"x\u{1f}y\"}", "raw control '\\u{1f}'"),
        ("{\"\t\":1}", "raw control"),
        ("{\"a\":[\"\u{1}\"]}", "raw control"),
        // Une suite après `}`, hors blancs.
        (r#"{"a":1}x"#, "after `}`, saw 'x'"),
        (r#"{"a":1} x"#, "after `}`, saw 'x'"),
        (r#"{"a":1}}"#, "after `}`, saw '}'"),
        (r#"{"a":1}{"b":2}"#, "after `}`, saw '{'"),
        ("{\"a\":1}\n{\"b\":2}", "after `}`, saw '{'"),
        ("{\"a\":1}\u{0}", "after `}`"),
        ("{\"a\":1}\u{a0}", "after `}`"),
        ("{\"a\":1}\u{b}", "after `}`"),
        (r#"{"a":1}//"#, "after `}`"),
        (r#"{"a":1} /* */"#, "after `}`"),
        // Une virgule en trop ou en moins.
        (r#"{"a":1,}"#, "expected a key (a string), saw '}'"),
        (r#"{"a":1 ,}"#, "expected a key"),
        (r#"{,}"#, "expected a key"),
        (r#"{"a":1,,"b":2}"#, "expected a key"),
        (r#"{"a":[1,]}"#, "expected a value, saw ']'"),
        (r#"{"a":[,1]}"#, "expected a value, saw ','"),
        (r#"{"a":{"b":1,}}"#, "expected a key"),
        (r#"{"a":1 "b":2}"#, "expected `,` or `}`, saw '\"'"),
        (r#"{"a":[1 2]}"#, "expected `,` or `]`, saw '2'"),
        // Un guillemet qui manque.
        (r#"{"a:1}"#, "saw the end of the line"),
        (r#"{"a":"x}"#, "expected a closing `\"`, saw the end of the line"),
        (r#"{"a":"x\"}"#, "saw the end of the line"),
        (r#"{a":1}"#, "expected a key"),
        (r#"{'a':1}"#, "expected a key"),
        (r#"{"a":x"}"#, "expected a value"),
        (r#"{"a":'x'}"#, "expected a value"),
        // Un échappement faux.
        (r#"{"a":"\x"}"#, "expected an escape"),
        (r#"{"a":"\'"}"#, "expected an escape"),
        (r#"{"a":"\U0041"}"#, "expected an escape"),
        (r#"{"a":"\"#, "or `u`), saw the end of the line"),
        (r#"{"a":"\u12"}"#, "expected a hexadecimal digit, saw '\"'"),
        (r#"{"a":"\u12G4"}"#, "saw 'G'"),
        (r#"{"a":"\u+123"}"#, "saw '+'"),
        (r#"{"a":"\u-123"}"#, "saw '-'"),
        (r#"{"a":"\u00é9"}"#, "saw 'é'"),
        (r#"{"a":"\u"#, "saw the end of the line"),
        (r#"{"a":"\ud83d\uZZZZ"}"#, "expected a hexadecimal digit, saw 'Z'"),
        // Des mots qui ne sont pas des valeurs.
        (r#"{"a":tru}"#, "expected `true`"),
        (r#"{"a":True}"#, "expected a value"),
        (r#"{"a":fals}"#, "expected `false`"),
        (r#"{"a":nul}"#, "expected `null`"),
        (r#"{"a":nulll}"#, "saw 'l'"),
        (r#"{"a":truefalse}"#, "saw 'f'"),
        (r#"{"a":undefined}"#, "expected a value"),
        // Des valeurs imbriquées mal formées : lues jusqu'au bout, donc refusées.
        (r#"{"a":[}"#, "expected a value, saw '}'"),
        (r#"{"a":[1}"#, "expected `,` or `]`, saw '}'"),
        (r#"{"a":{"b":1]}"#, "expected `,` or `}`, saw ']'"),
        (r#"{"a":{"b"}}"#, "expected `:`, saw '}'"),
        (r#"{"a":{"b":}}"#, "expected a value"),
        (r#"{"a":{1:2}}"#, "expected a key"),
        (r#"{"a":["\ud83d"]}"#, "lone surrogate"),
        (r#"{"a":[[[[1}"#, "expected `,` or `]`"),
        // Pas un objet, ou pas fini.
        ("", "expected an object `{`, saw the end of the line"),
        (" \t\r\n", "expected an object `{`, saw the end of the line"),
        ("[]", "expected an object `{`, saw '['"),
        (r#"[{"a":1}]"#, "expected an object `{`, saw '['"),
        ("\"a\"", "expected an object `{`, saw '\"'"),
        ("1", "expected an object `{`, saw '1'"),
        ("null", "expected an object `{`, saw 'n'"),
        ("\u{feff}{\"a\":1}", "expected an object `{`, saw '\\u{feff}'"),
        ("\u{a0}{\"a\":1}", "expected an object `{`"),
        ("\u{b}{\"a\":1}", "expected an object `{`"),
        ("{", "expected a key (a string), saw the end of the line"),
        ("{ ", "expected a key"),
        (r#"{"a""#, "expected `:`, saw the end of the line"),
        (r#"{"a" 1}"#, "expected `:`, saw '1'"),
        (r#"{"a":"#, "expected a value, saw the end of the line"),
        (r#"{"a":}"#, "expected a value, saw '}'"),
        (r#"{"a":1"#, "expected `,` or `}`, saw the end of the line"),
        (r#"{"a":1,"#, "expected a key"),
        ("{\u{b}\"a\":1}", "expected a key"),
        // Plus de 64 niveaux : l'objet de la ligne est le premier, le 65e est de trop.
        (tableaux.as_str(), "more than 64 levels of nesting"),
        (objets.as_str(), "more than 64 levels of nesting"),
    ];
    for (ligne, bout) in cas {
        let erreur = lire(ligne).expect_err(ligne);
        assert!(erreur.contains(bout), "{ligne:?} devait être refusée par « {bout} », et l'est par : {erreur}");
    }
    // Le refus entier, avec le rang du caractère fautif, compté en caractères et non en octets.
    let refus = |ligne: &str| lire(ligne).unwrap_err();
    assert_eq!(refus(r#"{"é":1,}"#), "at character 8: expected a key (a string), saw '}'");
    assert_eq!(refus("{\"a\":\"😀\u{1}\"}"), "raw control '\\u{1}' in a string (to write as \\u00xx), at character 8");
    assert_eq!(refus(r#"{"a":"😀\ud83d"}"#), "lone surrogate \\ud83d, at character 8");
    assert_eq!(refus(&tableaux), "more than 64 levels of nesting, at character 75");
    assert_eq!(refus(r#"{"a":1}x"#), "at character 8: expected the end of the line after `}`, saw 'x'");
    assert_eq!(refus(""), "at character 1: expected an object `{`, saw the end of the line");
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
        assert!(erreur.contains("more than 64 levels of nesting"), "{erreur}");
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
        assert_eq!(document(redite), Err(format!("repeated key: {cle:?}")), "{redite}");
    }
    for redite in [&redites[0], &redites[2]] {
        assert!(lire(redite).is_ok(), "{redite}");
    }
    // Ce que la RFC 8259 n'écrit pas est refusé comme par `lire`, avec le caractère fautif.
    let refus = |texte: &str| document(texte).unwrap_err();
    assert_eq!(refus(""), "at character 1: expected a value, saw the end of the line");
    assert_eq!(refus(r#"{"a":1}x"#), "at character 8: expected the end of the document, saw 'x'");
    assert_eq!(refus("[01]"), "at character 3: expected `,` or `]`, saw '1'");
    assert_eq!(refus(r#"{"a":[1,]}"#), "at character 9: expected a value, saw ']'");
    assert_eq!(refus(&format!(r#"["{}"]"#, u("d83d"))), format!("lone surrogate {}, at character 3", u("d83d")));
    assert_eq!(refus("[\"\t\"]"), "raw control '\\t' in a string (to write as \\u00xx), at character 3");
    // 64 niveaux se lisent et le 65e est de trop, en objets comme en tableaux, que le premier soit l'objet d'une
    // ligne ou un tableau.
    let tableaux = |n: usize| format!("{}{}", "[".repeat(n), "]".repeat(n));
    for ligne in profondes(64).into_iter().chain([tableaux(64)]) {
        assert!(document(&ligne).is_ok(), "{ligne}");
    }
    for ligne in profondes(65).into_iter().chain([tableaux(65)]) {
        let erreur = document(&ligne).unwrap_err();
        assert!(erreur.starts_with("more than 64 levels of nesting, at character "), "{erreur}");
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
        assert!(erreur.starts_with("more than 64 levels of nesting"), "{erreur}");
    }
}
