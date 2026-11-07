//! Les aides des tests d'une main, de sa preuve et de leurs processus : leur `dossier` n'est pas celui du fil.
use super::*;

/// Un dossier neuf, à ce test.
pub(crate) fn dossier(nom: &str) -> PathBuf {
    let dir = env::temp_dir().join(format!("lydite-test-tentative-{nom}-{}", lot()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le délai d'une main qui ne doit pas l'atteindre.
pub(crate) const DELAI: Duration = Duration::from_secs(30);

/// Un bac jetable : le sous-dossier `bac` d'un dossier neuf à ce test, que le test efface en entier à la fin,
/// son dossier temporaire compris. Rend le dossier, puis le bac.
pub(crate) fn bac_jetable(nom: &str) -> (PathBuf, PathBuf) {
    let dir = dossier(nom);
    let bac = dir.join("bac");
    fs::create_dir(&bac).unwrap();
    (dir, bac)
}

/// Une main qui joue [`LENTE`], écrite dans `dir` : son argv.
pub(crate) fn arbre(dir: &Path) -> Vec<String> {
    if cfg!(windows) {
        let script = dir.join("arbre.bat");
        fs::write(&script, format!("@echo off\r\n{LENTE}\r\n")).unwrap();
        vec![script.to_string_lossy().into_owned()]
    } else {
        let script = dir.join("arbre.sh");
        fs::write(&script, format!("{LENTE}\n")).unwrap();
        vec!["sh".into(), script.to_string_lossy().into_owned()]
    }
}

/// Un profil de fichier `claude` lu de ce texte, borné par un dossier de Kraken, un dépôt et deux leurres de dossiers
/// de connexion de Claude, fictifs, de `dir` : `config` pour `CLAUDE_CONFIG_DIR`, `personnel` pour le dossier
/// personnel.
pub(crate) fn profil_borne(texte: &str, dir: &Path) -> Profil {
    let mut profil = crate::coeur::profil::lire("essai", texte, None, None).unwrap();
    let (config, personnel) = (dir.join("config"), dir.join("personnel"));
    profil.borner(&dir.join("lydite"), &dir.join("depot").join(".git"), Some(&config), Some(&personnel)).unwrap();
    profil
}
