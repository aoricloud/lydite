//! Un texte venu du dehors (une main, un modèle, un plan, la forge), rendu sûr à afficher sur un terminal.
//!
//! Chaque caractère de contrôle (C0, DEL, C1) et chaque marque d'écriture bidirectionnelle (U+202A à U+202E,
//! U+2066 à U+2069) y est écrit sous la forme visible de `char::escape_default` (`\r`, `\t`, `\u{1b}`, `\u{202e}`),
//! jamais ôté en silence. Le reste du texte ne change pas, accents compris. Trois formes :
//!
//! - [`en_ligne`] : le texte tient sur une ligne, tout contrôle y est écrit, `\n` et `\t` compris ;
//! - [`en_lignes`] : le texte a plusieurs lignes ; `\n`, `\t` et un `\r` suivi aussitôt de `\n` (une sortie de Windows)
//!   restent tels quels, tout autre `\r` est écrit ;
//! - [`en_lignes_du_plan`] : comme `en_lignes`, mais tout `\r` est écrit, même devant `\n` : la règle du plan montré.
//!
//! Une sortie sans aucun de ces caractères est le texte même, à l'octet.
//!
//! // raccourci : `lydite accepter`, qui montre à l'humain son propre profil, n'y passe pas ; ni les messages de git et
//! // de la forge que impriment `src/conduite/publication.rs` et `src/branchements/github.rs` (dont l'adresse `PR …` et
//! // la ligne `Measurement: …` du reçu) ; ni la preuve du 7017, qui recopie brute l'erreur de sa correction
//! // (`src/conduite/relecture.rs`).

/// Le texte sur une ligne : tout contrôle et toute marque bidirectionnelle y sont écrits.
pub(crate) fn en_ligne(texte: &str) -> String {
    ecrire(texte, Garde::Rien)
}

/// Le texte sur plusieurs lignes : `\n`, `\t` et un `\r` suivi aussitôt de `\n` restent ; le reste est écrit.
pub(crate) fn en_lignes(texte: &str) -> String {
    ecrire(texte, Garde::LignesEtRetourChariot)
}

/// Le texte sur plusieurs lignes, tel que le plan le montre : `\n` et `\t` restent, tout `\r` est écrit.
pub(crate) fn en_lignes_du_plan(texte: &str) -> String {
    ecrire(texte, Garde::Lignes)
}

#[derive(Clone, Copy, PartialEq)]
enum Garde {
    Rien,
    Lignes,
    LignesEtRetourChariot,
}

fn a_ecrire(c: char) -> bool {
    c.is_control() || matches!(c, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

fn ecrire(texte: &str, garde: Garde) -> String {
    let mut sortie = String::with_capacity(texte.len());
    let mut suite = texte.chars().peekable();
    while let Some(c) = suite.next() {
        let garde_ce_c = match c {
            '\n' | '\t' => garde != Garde::Rien,
            '\r' => garde == Garde::LignesEtRetourChariot && suite.peek() == Some(&'\n'),
            _ => !a_ecrire(c),
        };
        if garde_ce_c {
            sortie.push(c);
        } else {
            sortie.extend(c.escape_default());
        }
    }
    sortie
}

#[cfg(test)]
mod tests;
