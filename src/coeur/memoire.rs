//! La mémoire de Lydite commence par ce qui est déjà écrit : le fil. Ce module en tient l'index, une ligne par ligne
//! lue du fil, et rappelle une ligne par son adresse ou par son empreinte, en lecture seule. Il est pur : il ne lit ni
//! n'écrit aucun fichier, il prend les octets du fil et les [`LigneLue`] que `fil::lire_octets_avec_lignes` en tire.
//!
//! - L'adresse d'une ligne est `<seance>/<seq>`, le `seq` en chiffres décimaux. Une séance qui porte un blanc, un
//!   caractère de contrôle, un caractère hors de l'ASCII ou un `/` n'a pas d'adresse : un autre écrivain du fil peut en
//!   écrire une, et elle casserait la forme de l'index.
//! - L'empreinte d'une ligne est le SHA-256 de ses octets exacts, sans le `\n` qui la termine, en 64 hexadécimaux
//!   minuscules.
//! - L'index est une ligne par ligne lue, dans l'ordre du fil, six champs séparés par un espace :
//!   `<adresse> <empreinte> <genre> <instant> <foi> <taille>`. L'adresse d'une ligne qui n'en a pas s'écrit `-`. `foi`
//!   vaut `observed` (le `constate` du fil), `declared` (son `declare`) ou `-` quand la ligne n'en affirme aucune ;
//!   `taille` compte les octets de la ligne sans son `\n`. Un fil sans ligne lue donne un texte vide.
//! - Le rappel rend les octets exacts de la ligne nommée : la ligne du fil elle-même, jamais une copie refaite. Si deux
//!   lignes portent la même adresse (un `seq` redit), l'adresse rend la première dans l'ordre du fil : une ligne
//!   ajoutée plus tard ne change jamais ce qu'une adresse rappelle. L'index les montre toutes les deux, et leur
//!   empreinte les distingue.

// raccourci : rien n'appelle encore `index` ni `rappeler` ; la commande de la tâche suivante les prendra
#![cfg_attr(not(test), expect(dead_code))]

use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::erreur::{Code, Refus};
use crate::coeur::fil::{Foi, LigneLue};

/// L'adresse de la ligne, `<seance>/<seq>` : `None` si sa séance porte un blanc, un caractère de contrôle, un
/// caractère hors de l'ASCII ou un `/`, ou si elle est vide.
pub(crate) fn adresse(ligne: &LigneLue) -> Option<String> {
    let nette = !ligne.seance.is_empty() && ligne.seance.chars().all(|c| c.is_ascii_graphic() && c != '/');
    nette.then(|| format!("{}/{}", ligne.seance, ligne.seq))
}

/// Les octets exacts de la ligne dans ceux du fil, sans son `\n`. Vide si `ligne` ne vient pas de ces octets.
fn octets_de<'a>(octets: &'a [u8], ligne: &LigneLue) -> &'a [u8] {
    octets.get(ligne.debut..ligne.fin).unwrap_or_default()
}

/// L'empreinte de la ligne : le SHA-256 de ses octets exacts sans le `\n`, en 64 hexadécimaux minuscules.
fn empreinte_de(octets: &[u8], ligne: &LigneLue) -> String {
    hex(&sha256(octets_de(octets, ligne)))
}

/// L'index des `lignes` lues dans `octets`, dans l'ordre du fil : une ligne de texte par ligne lue, finie par `\n`,
/// `<adresse> <empreinte> <genre> <instant> <foi> <taille>`. Un fil sans ligne lue donne un texte vide.
pub(crate) fn index(octets: &[u8], lignes: &[LigneLue]) -> String {
    let mut texte = String::new();
    for ligne in lignes {
        let foi = match ligne.foi {
            Some(Foi::Constate) => "observed",
            Some(Foi::Declare) => "declared",
            None => "-",
        };
        texte.push_str(&format!(
            "{} {} {} {} {foi} {}\n",
            adresse(ligne).as_deref().unwrap_or("-"),
            empreinte_de(octets, ligne),
            ligne.genre,
            ligne.instant,
            octets_de(octets, ligne).len(),
        ));
    }
    texte
}

/// 64 hexadécimaux minuscules : la forme d'une empreinte.
fn est_une_empreinte(texte: &str) -> bool {
    texte.len() == 64 && texte.bytes().all(|octet| matches!(octet, b'0'..=b'9' | b'a'..=b'f'))
}

/// Les octets exacts de la ligne que `texte` nomme, dans `octets` : sa ligne du fil elle-même, sans copie.
///
/// - Un texte avec `/` est une adresse, `<seance>/<seq>`, coupée sur son dernier `/` ; son `seq` s'écrit en chiffres
///   décimaux ASCII, sans signe ni zéro de tête. Elle rend la première ligne de cette adresse dans l'ordre du fil.
///   Une adresse mal formée, ou que le fil ne porte pas, est le refus 6034.
/// - Un texte sans `/` est une empreinte, 64 hexadécimaux minuscules ; un autre texte est le refus 6035, avant toute
///   lecture du fil. Une empreinte bien formée que le fil ne porte pas est le refus 6034.
pub(crate) fn rappeler<'a>(octets: &'a [u8], lignes: &[LigneLue], texte: &str) -> Result<&'a [u8], Refus> {
    let donner = "Give an address `<session>/<seq>` or a SHA-256 that `lydite memory` shows.";
    let trouvee = if texte.contains('/') {
        // L'adresse de la ligne est écrite sous sa forme seule : `x/01` et `x/+1` ne sont l'adresse d'aucune ligne.
        lignes.iter().find(|ligne| adresse(ligne).as_deref() == Some(texte)).ok_or_else(|| {
            Refus::new(
                Code::MemoireTexteAbsent,
                format!(
                    "{texte:?} is not the address of a line read from the thread; an address is `<session>/<seq>`, \
                     the seq in decimal digits without sign or leading zero"
                ),
                donner,
            )
        })?
    } else if est_une_empreinte(texte) {
        lignes.iter().find(|ligne| empreinte_de(octets, ligne) == texte).ok_or_else(|| {
            Refus::new(
                Code::MemoireTexteAbsent,
                format!("no line read from the thread has the SHA-256 {texte:?}"),
                donner,
            )
        })?
    } else {
        return Err(Refus::new(
            Code::MemoireEmpreinteFausse,
            format!("{texte:?} has no `/` and is not 64 lowercase hexadecimal digits"),
            "Give an address `<session>/<seq>`, or a SHA-256 of 64 lowercase hexadecimal digits, that `lydite \
             memory` shows.",
        ));
    };
    Ok(octets_de(octets, trouvee))
}

#[cfg(test)]
mod tests;
