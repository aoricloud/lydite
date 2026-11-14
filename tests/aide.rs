//! `lydite --help` et `lydite -h`, joués sur le binaire lui-même : ils écrivent l'usage, une forme par ligne, sans rien
//! lire ni écrire d'autre ; ailleurs sur la ligne, ces mots sont le refus 1006, et rien ne part.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-aide-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le binaire, dont le dossier de Lydite est `dossier` et dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des
/// leurres sous le dossier temporaire, jamais créés : le vrai dossier personnel de l'humain n'est jamais lu.
fn kraken(dossier: &Path) -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-aide-leurres-{}", std::process::id()));
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande
        .env("LYDITE_DOSSIER", dossier)
        .env("USERPROFILE", leurres.join("personnel"))
        .env("HOME", leurres.join("personnel"))
        .env_remove("XDG_STATE_HOME")
        .env("CLAUDE_CONFIG_DIR", leurres.join("config"));
    commande
}

fn jouer(commande: &mut Command) -> Output {
    commande.output().unwrap()
}

/// Les noms que contient un dossier.
fn noms(dossier: &Path) -> Vec<String> {
    let entrees = fs::read_dir(dossier).unwrap();
    let mut noms: Vec<String> =
        entrees.map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

/// La sortie sur stdout, sans les `\r` qu'une plateforme aurait pu y mettre.
fn sortie_de(sortie: &Output) -> String {
    String::from_utf8(sortie.stdout.clone()).unwrap().replace("\r\n", "\n")
}

#[test]
fn l_aide_ecrit_l_usage_une_forme_par_ligne_et_sort_en_0() {
    let dossier = neuf("usage");
    let longue = jouer(kraken(&dossier).arg("--help"));
    let courte = jouer(kraken(&dossier).arg("-h"));
    for sortie in [&longue, &courte] {
        assert_eq!(sortie.status.code(), Some(0), "{}", String::from_utf8_lossy(&sortie.stderr));
        assert!(sortie.stderr.is_empty(), "{}", String::from_utf8_lossy(&sortie.stderr));
    }
    assert_eq!(longue.stdout, courte.stdout);
    let texte = sortie_de(&longue);
    let lignes: Vec<&str> = texte.lines().collect();
    assert_eq!(lignes[0], "Usage:", "{texte}");
    // Chaque forme de l'usage se lit sur sa ligne, précédée de deux espaces.
    for forme in [
        "lydite launch <profile> \"<objective>\" [--n N] [--verify \"<command>\"] [--repo <path>]",
        "lydite launch <profile> --plan <file> [--repo <path>] [--pr <branch>]",
        concat!(
            "lydite launch <profile> \"<objective>\" --split --verify \"<proof>\" [--dry-run] [--repo <path>]",
            " [--pr <branch>]"
        ),
        concat!(
            "lydite launch <profile> --issue <n> --split --verify \"<proof>\" [--dry-run] [--repo <path>]",
            " [--pr <branch>]"
        ),
        "lydite plan [--split] <file>",
        "lydite remarks <file>",
        "lydite findings <report> [--plan <id> --verify \"<proof>\"]",
        "lydite lessons <folder>",
        "lydite accept <profile>",
        "lydite profile <profile>",
        "lydite doctor [<profile>]",
        "lydite status",
        "lydite wait",
        "lydite --version",
        "lydite memory",
        "lydite memory recall <address>",
        "lydite skill",
    ] {
        let ligne = format!("  {forme}");
        assert!(lignes.contains(&ligne.as_str()), "pas de ligne « {ligne} » :\n{texte}");
    }
    assert_eq!(lignes.len(), 18, "une ligne par forme, après `Usage:` :\n{texte}");
}

#[test]
fn l_aide_ne_cree_rien_dans_le_dossier_de_lydite() {
    // Un dossier neuf et vide : tout ce que la commande y écrirait s'y verrait.
    let dossier = neuf("rien");
    for mot in ["--help", "-h"] {
        assert!(jouer(kraken(&dossier).arg(mot)).status.success());
    }
    assert_eq!(noms(&dossier), Vec::<String>::new());
}

#[test]
fn l_aide_avec_un_mot_de_plus_ou_ailleurs_est_le_refus_1006_et_ne_lance_rien() {
    let dossier = neuf("refus");
    for mots in [&["--help", "x"][..], &["launch", "claude", "--help"], &["accept", "-h"]] {
        let sortie = jouer(kraken(&dossier).args(mots));
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(2), "{mots:?}: {erreur}");
        assert!(erreur.starts_with("lydite : 1006 contrat.commande_mal_formee\n"), "{mots:?}: {erreur}");
        assert!(sortie.stdout.is_empty(), "{mots:?}: l'usage ne s'écrit pas avec un refus");
        assert_eq!(noms(&dossier), Vec::<String>::new(), "{mots:?}");
    }
}
