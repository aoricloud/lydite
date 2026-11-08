//! Les points à justifier d'un diff, relevés sans modèle : ce que Lydite voit seul dans le travail d'une vague et que
//! son auteur doit pouvoir justifier. Une fonction pure ([`points`]) lit le diff de git ; [`lire`] et [`ARGUMENTS`]
//! disent comment git le donne. Trois signes, chacun une ligne `<chemin>, line <n>: <raison>` (sans `, line <n>` pour
//! un test modifié) : un fichier de test modifié, une assertion retirée, une tolérance qui change (ajoutée ou
//! retirée). Le numéro est celui de la ligne de la base ; pour une ligne ajoutée, la ligne de la base où elle
//! s'insère. Les points sont triés et sans doublon.

use std::path::Path;

use crate::branchements::git::{faire_borne, geste};
use crate::coeur::plan::TAILLE_MAX;
use crate::conduite::publication::en_code;

/// Les arguments de `git diff` qui donnent le diff que lit [`points`], avant la base et la tête.
pub(crate) const ARGUMENTS: [&str; 5] = ["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames"];

/// Le diff de `base` à `tete` limité à `fichiers` (tous s'il n'y en a pas), lu dans `dossier`, puis ses [`points`].
/// Raccourci : un diff de plus de [`TAILLE_MAX`] octets est lu coupé, et ses points avec lui. Un git qui ne le dit pas
/// rend `Err`.
pub(crate) fn lire(dossier: &Path, base: &str, tete: &str, fichiers: Option<&[String]>) -> Result<Vec<String>, String> {
    let mut args = ARGUMENTS.to_vec();
    args.extend([base, tete, "--"]);
    let mut diff = geste(dossier, &args);
    diff.env("GIT_LITERAL_PATHSPECS", "1").args(fichiers.unwrap_or_default());
    let (gardes, _) = faire_borne(diff, TAILLE_MAX + 1)?;
    Ok(points(&String::from_utf8_lossy(&gardes)))
}

/// Un fichier de test : sous un dossier `tests`, ou nommé `tests.rs`, `*_test.rs`, `*_tests.rs` ou `test_*`.
fn est_un_test(chemin: &str) -> bool {
    let mut parties = chemin.rsplit('/');
    let nom = parties.next().unwrap_or_default();
    nom.ends_with("tests.rs") || nom.ends_with("_test.rs") || nom.starts_with("test_") || parties.any(|d| d == "tests")
}

/// Le chemin d'une ligne `--- ` ou `+++ ` d'un en-tête de diff, sans le préfixe `a/` ou `b/`, `None` pour `/dev/null`.
fn chemin_de(ligne: &str) -> Option<&str> {
    let brut = ligne.get(4..)?.split('\t').next()?.trim_matches('"');
    (brut != "/dev/null").then(|| brut.get(2..).unwrap_or(brut))
}

/// Les deux nombres d'une plage de hunk `<début>[,<compte>]` : le début, et le compte (1 par défaut).
fn plage(texte: &str) -> (u64, u64) {
    let mut nombres = texte.split(',').map(|n| n.trim().parse::<u64>().unwrap_or(0));
    let debut = nombres.next().unwrap_or(0);
    (debut, nombres.next().unwrap_or(1))
}

/// Les points à justifier de `diff`, triés et sans doublon.
pub(crate) fn points(diff: &str) -> Vec<String> {
    // (chemin, ligne de la base, raison)
    let mut trouves: Vec<(String, Option<u64>, &str)> = Vec::new();
    let (mut ancien, mut nouveau) = (None, None);
    // Dans un hunk : les lignes qui restent de l'ancien et du nouveau, et la ligne courante de la base.
    // Une ligne ajoutée prend la ligne de la base que le changement retire juste avant elle, sinon celle où elle
    // s'insère.
    let (mut restent, mut courante, mut retiree) = ((0u64, 0u64), 0u64, None);
    for ligne in diff.lines() {
        let chemin: &str = nouveau.or(ancien).unwrap_or_default();
        if restent != (0, 0) {
            let (signe, texte) = ligne.split_at(ligne.chars().next().map_or(0, char::len_utf8));
            if signe == "\\" {
                continue;
            }
            let bas = texte.to_lowercase();
            let lieu = if signe == "+" { retiree.unwrap_or(courante) } else { courante };
            retiree = match signe {
                "-" => Some(courante),
                "+" => retiree,
                _ => None,
            };
            if signe == "-" && bas.contains("assert") {
                trouves.push((chemin.into(), Some(lieu), "assertion removed"));
            }
            if matches!(signe, "-" | "+") && ["toler", "tolér", "epsilon"].iter().any(|mot| bas.contains(mot)) {
                trouves.push((chemin.into(), Some(lieu), "tolerance changed"));
            }
            if signe != "+" {
                restent.0 = restent.0.saturating_sub(1);
            }
            if signe != "-" {
                restent.1 = restent.1.saturating_sub(1);
            }
            if signe != "+" {
                courante += 1;
            }
        } else if ligne.starts_with("diff ") {
            (ancien, nouveau) = (None, None);
        } else if ligne.starts_with("--- ") {
            ancien = chemin_de(ligne);
        } else if ligne.starts_with("+++ ") {
            nouveau = chemin_de(ligne);
        } else if let Some(entete) = ligne.strip_prefix("@@ -") {
            let (vieux, neuf) = entete.split_once(" +").unwrap_or_default();
            let (vieux, neuf) = (plage(vieux), plage(neuf.split(' ').next().unwrap_or_default()));
            (restent, courante) = ((vieux.1, neuf.1), vieux.0);
            if est_un_test(chemin) {
                trouves.push((chemin.into(), None, "test modified"));
            }
        }
    }
    trouves.sort();
    trouves.dedup();
    trouves
        .into_iter()
        .map(|(chemin, ligne, raison)| match ligne {
            Some(ligne) => format!("{}, line {ligne}: {raison}", en_code(&chemin)),
            None => format!("{}: {raison}", en_code(&chemin)),
        })
        .collect()
}

#[cfg(test)]
mod tests;
