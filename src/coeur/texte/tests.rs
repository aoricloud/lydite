use super::*;

#[test]
fn chaque_famille_de_controle_s_ecrit_sous_sa_forme_visible() {
    // C0 : le retour chariot, la tabulation, le saut de ligne, l'échappement ; DEL ; C1 : NEL.
    assert_eq!(en_ligne("a\rb"), "a\\rb");
    assert_eq!(en_ligne("a\tb"), "a\\tb");
    assert_eq!(en_ligne("a\nb"), "a\\nb");
    assert_eq!(en_ligne("a\u{1b}[31mb"), "a\\u{1b}[31mb");
    assert_eq!(en_ligne("a\u{7f}b"), "a\\u{7f}b");
    assert_eq!(en_ligne("a\u{85}b"), "a\\u{85}b");
    assert_eq!(en_ligne("a\u{0}b"), "a\\u{0}b");
}

#[test]
fn chaque_marque_bidirectionnelle_s_ecrit_sous_sa_forme_visible() {
    assert_eq!(en_ligne("a\u{202e}b"), "a\\u{202e}b");
    for c in ('\u{202A}'..='\u{202E}').chain('\u{2066}'..='\u{2069}') {
        let ecrit = en_ligne(&format!("<{c}>"));
        assert!(!ecrit.contains(c), "{ecrit}");
        assert_eq!(ecrit, format!("<{}>", c.escape_default()));
    }
    // Les bornes voisines ne sont pas des marques.
    assert_eq!(en_ligne("\u{2029}\u{202f}\u{2065}\u{206a}"), "\u{2029}\u{202f}\u{2065}\u{206a}");
}

#[test]
fn un_texte_accentue_ne_change_pas() {
    let texte = "Écrire « bonjour » à l'été : ça va, 日本語, 🦀";
    assert_eq!(en_ligne(texte), texte);
    assert_eq!(en_lignes(texte), texte);
    assert_eq!(en_lignes_du_plan(texte), texte);
}

#[test]
fn plusieurs_lignes_gardent_le_saut_de_ligne_la_tabulation_et_le_retour_chariot_avant_lui() {
    assert_eq!(en_lignes("a\r\nb"), "a\r\nb");
    assert_eq!(en_lignes("a\n\tb\n"), "a\n\tb\n");
    assert_eq!(en_lignes("a\rb"), "a\\rb");
    assert_eq!(en_lignes("a\r"), "a\\r");
    assert_eq!(en_lignes("a\r\rb"), "a\\r\\rb");
    assert_eq!(en_lignes("a\r\r\nb"), "a\\r\r\nb");
    assert_eq!(en_lignes("a\u{1b}b\u{202e}"), "a\\u{1b}b\\u{202e}");
}

#[test]
fn le_plan_ecrit_toujours_le_retour_chariot() {
    assert_eq!(en_lignes_du_plan("tache a\n\tfichiers : x\u{1b}[31m\r\n"), "tache a\n\tfichiers : x\\u{1b}[31m\\r\n");
    assert_eq!(en_lignes_du_plan("a\u{202e}b"), "a\\u{202e}b");
}
