//! Les leçons : le fichier `.agents/skills/<nom>/SKILL.md` d'un dépôt, au format ouvert des skills, qui garde la
//! règle qu'une erreur passée a enseignée. Une leçon s'écrit par la session ou la pilote, dans la PR qui corrige
//! l'erreur, jamais par une main. Ce module la lit, sans bibliothèque, et la lit strictement.
//!
//! ```text
//! ---
//! name: <nom du dossier>
//! description: <la règle, sur une ligne>
//! ---
//!
//! ## What happened
//! ## Why
//! ## How it is checked
//! ## Pitfall
//! ## Files
//! - <chemin>
//! ## Falls when
//! ```
//!
//! - l'en-tête tient `name` puis `description`, rien d'autre ; le nom est celui du dossier de la leçon, en `a-z`,
//!   chiffres et `-`, de 1 à 64 caractères ; la règle tient sur une ligne, de 1 à 200 caractères ;
//! - puis les six sections, chacune une fois et dans cet ordre, chacune du texte, sauf `## Files`, qui porte zéro ou
//!   plusieurs lignes `- <chemin>`, des chemins que `plan::hors_forme` ne reprend pas. Une leçon sans chemin est
//!   générale ;
//! - UTF-8, 4096 octets au plus, `\r\n` admis.
//!
//! Toute faute est le refus 6031, qui nomme la ligne qu'il vise.

use std::str;

use crate::coeur::erreur::{Code, Refus};
use crate::coeur::plan;

/// Ce que pèse une leçon au plus, en octets.
pub(crate) const TAILLE_MAX: usize = 4096;

/// Les sections, dans l'ordre où elles se suivent.
const SECTIONS: [&str; 6] = ["What happened", "Why", "How it is checked", "Pitfall", "Files", "Falls when"];

/// Le rang de `## Files` dans [`SECTIONS`].
const FICHIERS: usize = 4;

/// Une leçon lue.
// Les mains ne la reçoivent qu'avec la tâche qui suit : d'ici là, seuls les tests lisent ses champs.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Lecon {
    /// Son nom, celui de son dossier.
    pub(crate) nom: String,
    /// Sa règle, en une ligne.
    pub(crate) regle: String,
    /// Les chemins qu'elle vise ; aucun, elle est générale.
    pub(crate) fichiers: Vec<String>,
    /// Son texte : tout ce qui suit l'en-tête.
    pub(crate) texte: String,
}

/// Le refus de la ligne `ligne` (à partir de 1).
fn refus(ligne: usize, quoi: impl std::fmt::Display, correction: &str) -> Refus {
    Refus::new(Code::LeconMalFormee, format!("ligne {ligne} : {quoi}"), correction)
}

const CORRECTION_ENTETE: &str = "Écris l'en-tête `---`, `name: <nom du dossier>`, `description: <la règle, sur une \
                                 ligne>`, `---`, sans autre clé : la forme est au README.";
const CORRECTION_SECTIONS: &str = "Écris chaque section une fois, avec du texte, dans l'ordre : `## What happened`, \
                                   `## Why`, `## How it is checked`, `## Pitfall`, `## Files`, `## Falls when`.";
const CORRECTION_CHEMIN: &str = "Sous `## Files`, écris une ligne `- <chemin>` par fichier visé, relatif à la racine \
                                 du dépôt et écrit avec `/` ; sans chemin, laisse la section sans ligne.";

/// Lit la leçon du dossier `dossier` (son nom, pas son chemin) dont voici les octets.
pub(crate) fn lire(dossier: &str, octets: &[u8]) -> Result<Lecon, Refus> {
    if octets.len() > TAILLE_MAX {
        let preuve = format!("la leçon pèse {} octets, plus de {TAILLE_MAX}", octets.len());
        return Err(refus(1, preuve, "Raccourcis la leçon sous 4096 octets : garde la règle et ce qui la prouve."));
    }
    let texte = str::from_utf8(octets).map_err(|e| {
        let ligne = 1 + octets[..e.valid_up_to()].iter().filter(|&&octet| octet == b'\n').count();
        refus(ligne, "la leçon n'est pas de l'UTF-8", "Réécris la leçon en UTF-8.")
    })?;
    let mut lignes: Vec<&str> = texte.split('\n').map(|ligne| ligne.strip_suffix('\r').unwrap_or(ligne)).collect();
    if lignes.last() == Some(&"") {
        lignes.pop();
    }
    let ligne = |rang: usize| lignes.get(rang).copied().unwrap_or("");

    if ligne(0) != "---" {
        return Err(refus(1, "la leçon commence par une ligne `---`", CORRECTION_ENTETE));
    }
    let Some(nom) = ligne(1).strip_prefix("name: ") else {
        return Err(refus(2, "la deuxième ligne est `name: <nom>`", CORRECTION_ENTETE));
    };
    let bon_nom = (1..=64).contains(&nom.len())
        && nom.bytes().all(|octet| octet.is_ascii_lowercase() || octet.is_ascii_digit() || octet == b'-');
    if !bon_nom {
        let preuve = format!("le nom {nom:?} n'est pas fait de 1 à 64 caractères parmi `a-z`, chiffres et `-`");
        return Err(refus(2, preuve, CORRECTION_ENTETE));
    }
    if nom != dossier {
        let preuve = format!("le nom {nom:?} n'est pas celui du dossier de la leçon, {dossier:?}");
        return Err(refus(2, preuve, "Mets dans `name` le nom du dossier de la leçon, ou renomme le dossier."));
    }
    let Some(regle) = ligne(2).strip_prefix("description: ") else {
        return Err(refus(3, "la troisième ligne est `description: <règle>`", CORRECTION_ENTETE));
    };
    let longueur = regle.chars().count();
    if regle.trim().is_empty() || longueur > 200 || regle.chars().any(char::is_control) {
        let preuve = format!("la règle fait {longueur} caractères : de 1 à 200, sur une ligne, non blanche");
        return Err(refus(3, preuve, CORRECTION_ENTETE));
    }
    if ligne(3) != "---" {
        let preuve = "l'en-tête ne tient que `name` et `description`, chacun sur une ligne, puis `---`";
        return Err(refus(4, preuve, CORRECTION_ENTETE));
    }

    let mut fichiers = Vec::new();
    // Les sections lues, et la courante : la ligne de son titre, et si elle porte du texte.
    let mut lues = 0;
    let mut courante: Option<(usize, bool)> = None;
    for (rang, texte) in lignes.iter().enumerate().skip(4) {
        let numero = rang + 1;
        if let Some(titre) = texte.strip_prefix("## ") {
            fermer(courante, lues)?;
            match SECTIONS.iter().position(|section| *section == titre) {
                Some(place) if place == lues => {
                    lues += 1;
                    courante = Some((numero, false));
                }
                Some(place) if place < lues => {
                    let preuve = format!("`## {titre}` est en double ou hors d'ordre");
                    return Err(refus(numero, preuve, CORRECTION_SECTIONS));
                }
                Some(_) => {
                    let preuve = format!("il manque `## {}` avant `## {titre}`", SECTIONS[lues]);
                    return Err(refus(numero, preuve, CORRECTION_SECTIONS));
                }
                None => {
                    let preuve = format!("`## {titre}` n'est pas une section de la leçon");
                    return Err(refus(numero, preuve, CORRECTION_SECTIONS));
                }
            }
            continue;
        }
        if texte.trim().is_empty() {
            continue;
        }
        let Some((_, porte)) = courante.as_mut() else {
            return Err(refus(numero, "du texte avant la première section", CORRECTION_SECTIONS));
        };
        *porte = true;
        if lues - 1 == FICHIERS {
            let chemin = texte.strip_prefix("- ").filter(|chemin| *chemin == chemin.trim());
            let Some(chemin) = chemin else {
                return Err(refus(numero, "sous `## Files`, une ligne est `- <chemin>`", CORRECTION_CHEMIN));
            };
            if let Some(raison) = plan::hors_forme(chemin) {
                return Err(refus(numero, format!("le chemin {chemin:?} {raison}"), CORRECTION_CHEMIN));
            }
            fichiers.push(chemin.to_string());
        }
    }
    fermer(courante, lues)?;
    if lues < SECTIONS.len() {
        let preuve = format!("il manque `## {}`", SECTIONS[lues]);
        return Err(refus(lignes.len().max(1), preuve, CORRECTION_SECTIONS));
    }
    let texte = lignes[4..].join("\n").trim().to_string();
    Ok(Lecon { nom: nom.to_string(), regle: regle.to_string(), fichiers, texte })
}

/// Ferme la section courante, la `lues`-ième : sauf `## Files`, elle porte du texte.
fn fermer(courante: Option<(usize, bool)>, lues: usize) -> Result<(), Refus> {
    match courante {
        Some((numero, false)) if lues - 1 != FICHIERS => {
            let preuve = format!("`## {}` est vide", SECTIONS[lues - 1]);
            Err(refus(numero, preuve, CORRECTION_SECTIONS))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
