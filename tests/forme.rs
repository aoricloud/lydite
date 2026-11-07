//! Le test de forme : dans `src/` et `tests/`, aucune ligne d'un fichier `.rs` ne dépasse 120 caractères. Les exemples
//! du fil, plus longs, vivent dans `src/coeur/fil/exemples.jsonl`, que ce test ne lit pas.

use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn aucune_ligne_de_plus_de_120_caracteres() {
    let racine = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut fichiers = Vec::new();
    for dossier in ["src", "tests"] {
        fichiers_rust(&racine.join(dossier), &mut fichiers);
    }
    let mut fautes = Vec::new();
    for fichier in fichiers {
        let source = fs::read_to_string(&fichier).unwrap();
        let nom = fichier.strip_prefix(racine).unwrap().display().to_string().replace('\\', "/");
        for (rang, ligne) in source.lines().enumerate() {
            let longueur = ligne.chars().count();
            if longueur > 120 {
                fautes.push(format!("{nom}:{} : {longueur} caractères", rang + 1));
            }
        }
    }
    assert!(fautes.is_empty(), "des lignes de plus de 120 caractères :\n{}", fautes.join("\n"));
}

/// Les fichiers `.rs` sous `dossier`, ses sous-dossiers compris.
fn fichiers_rust(dossier: &Path, trouves: &mut Vec<PathBuf>) {
    for entree in fs::read_dir(dossier).unwrap() {
        let entree = entree.unwrap();
        let chemin = entree.path();
        let genre = entree.file_type().unwrap();
        if genre.is_dir() {
            fichiers_rust(&chemin, trouves);
        } else if genre.is_file() && chemin.extension().is_some_and(|extension| extension == "rs") {
            trouves.push(chemin);
        }
    }
}
