//! Le test des couches : un fichier de production ne nomme `crate::` que d'une couche à lui ou en dessous. Le cœur est
//! en bas, puis les branchements, la conduite et la commande ; les fichiers de tests et `essais` peuvent appeler toutes
//! les couches.

use std::fs;
use std::path::{Path, PathBuf};

/// Les couches, de la plus basse à la plus haute : chacune peut nommer la sienne et celles qui la précèdent.
const COUCHES: [&str; 4] = ["coeur", "branchements", "conduite", "commande"];

#[test]
fn une_couche_ne_nomme_pas_une_couche_du_dessus() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut fautes = Vec::new();
    for (rang, couche) in COUCHES.iter().enumerate() {
        let mut fichiers = Vec::new();
        fichiers_de_production(&src.join(couche), &mut fichiers);
        for fichier in fichiers {
            let source = fs::read_to_string(&fichier).unwrap();
            for (ligne, texte) in source.lines().enumerate() {
                for dessus in &COUCHES[rang + 1..] {
                    let nomme = format!("crate::{dessus}");
                    if texte.contains(&nomme) {
                        let chemin = fichier.strip_prefix(&src).unwrap().display();
                        fautes.push(format!("src/{chemin}:{} nomme `{nomme}`", ligne + 1));
                    }
                }
            }
        }
    }
    assert!(fautes.is_empty(), "une couche nomme une couche du dessus :\n{}", fautes.join("\n"));
}

/// Les fichiers de production sous `dossier` : les `.rs`, sauf `tests.rs` et ce que porte un dossier `tests/`, qui sont
/// des tests.
fn fichiers_de_production(dossier: &Path, trouves: &mut Vec<PathBuf>) {
    for entree in fs::read_dir(dossier).unwrap() {
        let entree = entree.unwrap();
        let nom = entree.file_name();
        let chemin = entree.path();
        if entree.file_type().unwrap().is_dir() {
            if nom != "tests" {
                fichiers_de_production(&chemin, trouves);
            }
        } else if entree.file_type().unwrap().is_file()
            && chemin.extension().is_some_and(|extension| extension == "rs")
            && nom != "tests.rs"
        {
            trouves.push(chemin);
        }
    }
}
