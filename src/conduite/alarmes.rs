//! Les points à justifier d'un diff, relevés sans modèle : ce que Lydite voit seul dans le travail d'une vague et que
//! son auteur doit pouvoir justifier. Une fonction pure ([`points`]) lit le diff de git ; [`lire`] et [`ARGUMENTS`]
//! disent comment git le donne. Seules les lignes retirées comptent : un fichier de test de la base dont le diff en
//! retire (`<chemin>: a test of the base changed (<N> lines removed)`, une fois par chemin), puis, par ligne retirée,
//! une tolérance ou, sinon, une assertion (`<chemin>, line <n>: …`). Le numéro est celui de la ligne de la base. Les
//! points sont triés par chemin puis par ligne, et sans doublon.

use std::path::Path;

use crate::branchements::git::{faire_borne, geste};
use crate::coeur::plan::TAILLE_MAX;
use crate::conduite::publication::en_code;

/// Les arguments de `git diff` qui donnent le diff que lit [`points`], avant la base et la tête. Les préfixes `a/` et
/// `b/` sont fixés : la configuration de l'humain ou du dépôt (`diff.noprefix`, `diff.srcPrefix`, `diff.dstPrefix`)
/// ne change pas la forme des noms que lit `chemin_de`.
pub(crate) const ARGUMENTS: [&str; 7] =
    ["diff", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", "--src-prefix=a/", "--dst-prefix=b/"];

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

/// Un fichier de test : un de ses dossiers s'appelle `tests` ou `test`, ou son nom est `tests.rs`, commence par
/// `test_`, finit par `_test.<ext>` ou `_tests.<ext>`, ou contient `.test.` ou `.spec.`, sans casse.
fn est_un_test(chemin: &str) -> bool {
    let chemin = chemin.to_lowercase();
    let mut parties = chemin.rsplit('/');
    let nom = parties.next().unwrap_or_default();
    let radical = nom.rsplit_once('.').map_or("", |(radical, _)| radical);
    nom == "tests.rs"
        || nom.starts_with("test_")
        || radical.ends_with("_test")
        || radical.ends_with("_tests")
        || nom.contains(".test.")
        || nom.contains(".spec.")
        || parties.any(|d| d == "tests" || d == "test")
}

/// Une ligne, en minuscules, qui porte une tolérance.
fn porte_une_tolerance(bas: &str) -> bool {
    let mots = ["epsilon", "toleran", "approx", "abs_diff", "relative_eq", "ulps"];
    let octets = bas.as_bytes();
    let exposant =
        octets.windows(4).any(|q| q[0].is_ascii_digit() && q[1] == b'e' && q[2] == b'-' && q[3].is_ascii_digit());
    mots.iter().any(|mot| bas.contains(mot)) || (bas.contains(".abs()") && bas.contains('<')) || exposant
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

/// Les points à justifier de `diff`, triés par chemin puis par ligne, et sans doublon.
pub(crate) fn points(diff: &str) -> Vec<String> {
    // (chemin, ligne de la base, raison)
    let mut trouves: Vec<(String, Option<u64>, String)> = Vec::new();
    // Les lignes retirées de chaque chemin, dans l'ordre où on les voit.
    let mut retirees: Vec<(String, u64)> = Vec::new();
    let (mut ancien, mut nouveau) = (None, None);
    // Dans un hunk : les lignes qui restent de l'ancien et du nouveau, et la ligne courante de la base.
    let (mut restent, mut courante) = ((0u64, 0u64), 0u64);
    for ligne in diff.lines() {
        let chemin: &str = nouveau.or(ancien).unwrap_or_default();
        if restent != (0, 0) {
            let (signe, texte) = ligne.split_at(ligne.chars().next().map_or(0, char::len_utf8));
            if signe == "\\" {
                continue;
            }
            if signe == "-" {
                let bas = texte.to_lowercase();
                // raccourci : un test écrit dans un fichier de code n'est vu que par ses assertions, monter avec A3,
                // qui lira les tests un par un
                let raison = if porte_une_tolerance(&bas) {
                    Some("a tolerance removed or changed")
                } else {
                    bas.contains("assert").then_some("an assertion removed or changed")
                };
                if let Some(raison) = raison {
                    trouves.push((chemin.into(), Some(courante), raison.into()));
                }
                match retirees.iter_mut().find(|(vu, _)| vu == chemin) {
                    Some((_, n)) => *n += 1,
                    None => retirees.push((chemin.into(), 1)),
                }
            }
            if signe != "+" {
                restent.0 = restent.0.saturating_sub(1);
                courante += 1;
            }
            if signe != "-" {
                restent.1 = restent.1.saturating_sub(1);
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
        }
    }
    for (chemin, n) in retirees {
        if est_un_test(&chemin) {
            trouves.push((chemin, None, format!("a test of the base changed ({n} lines removed)")));
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
