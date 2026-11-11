//! `lydite skill`, joué sur le binaire lui-même : il écrit la skill de pilotage, telle quelle, sans rien lire ni écrire
//! d'autre ; et cette skill ne peut pas vieillir : chaque commande qu'elle cite est un mot que le binaire connaît.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Le fichier de la skill, seule source : le binaire le lie, le plugin Hermes le lit à ce chemin.
fn fichier_de_la_skill() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("hermes").join("skills").join("driving-lydite").join("SKILL.md")
}

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-skill-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le binaire, dont le dossier de Lydite est `dossier` et dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des
/// leurres sous le dossier temporaire, jamais créés : posés sur l'enfant seul, ils ne touchent pas le processus du
/// test, et le vrai dossier personnel de l'humain n'est jamais lu.
fn kraken(dossier: &Path) -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-skill-leurres-{}", std::process::id()));
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

#[test]
fn skill_ecrit_exactement_les_octets_du_fichier_et_sort_en_0() {
    let dossier = neuf("octets");
    let sortie = jouer(kraken(&dossier).arg("skill"));
    assert_eq!(sortie.status.code(), Some(0), "{}", String::from_utf8_lossy(&sortie.stderr));
    assert_eq!(sortie.stdout, fs::read(fichier_de_la_skill()).unwrap());
    assert!(!sortie.stdout.is_empty());
    assert!(sortie.stderr.is_empty(), "{}", String::from_utf8_lossy(&sortie.stderr));
}

#[test]
fn skill_ne_cree_rien_dans_le_dossier_de_lydite() {
    // Un dossier neuf et vide : tout ce que la commande y écrirait s'y verrait.
    let dossier = neuf("rien");
    assert!(jouer(kraken(&dossier).arg("skill")).status.success());
    assert_eq!(noms(&dossier), Vec::<String>::new());
    // Un dossier qui n'existe pas encore reste absent.
    let absent = neuf("absent").join("pas-encore");
    assert!(jouer(kraken(&absent).arg("skill")).status.success());
    assert!(!absent.exists(), "{} a été créé", absent.display());
}

#[test]
fn skill_avec_un_argument_de_trop_est_le_refus_1006() {
    let dossier = neuf("refus");
    for mots in [&["skill", "x"][..], &["skill", "--n", "1"], &["skill", "x", "y"]] {
        let sortie = jouer(kraken(&dossier).args(mots));
        let erreur = String::from_utf8_lossy(&sortie.stderr);
        assert_eq!(sortie.status.code(), Some(2), "{mots:?}: {erreur}");
        assert!(erreur.starts_with("lydite : 1006 contrat.commande_mal_formee\n"), "{mots:?}: {erreur}");
        assert!(erreur.contains(&format!("« {} » follows it", mots[1])), "{mots:?}: {erreur}");
        assert!(sortie.stdout.is_empty(), "{mots:?}: la skill ne s'écrit pas avec un refus");
    }
}

/// Les mots que le texte cite après `lydite ` : une commande par mot, qu'elle soit en code ou dans la prose. Le nom
/// d'un outil du plugin (`lydite_lancer`) n'a pas d'espace après `lydite` : il n'en est pas un. `lydite binary` et
/// `lydite plugin` disent le programme et son plugin, non une commande.
fn commandes_citees(texte: &str) -> BTreeSet<String> {
    const PROSE: [&str; 2] = ["binary", "plugin"];
    let mut mots = BTreeSet::new();
    let mut reste = texte;
    while let Some(i) = reste.find("lydite ") {
        let precedent = reste[..i].chars().next_back();
        reste = &reste[i + "lydite ".len()..];
        // Un `lydite` collé à la fin d'un autre mot n'est pas le programme.
        if precedent.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            continue;
        }
        let mot: String = reste.chars().take_while(|c| c.is_ascii_lowercase() || *c == '-').collect();
        if !mot.is_empty() && !PROSE.contains(&mot.as_str()) {
            mots.insert(mot);
        }
    }
    mots
}

/// Le binaire connaît ce mot : lancé avec lui, il ne répond jamais par le refus 1005. Quelques mots feraient du travail
/// ou attendraient (60 s) s'ils étaient lancés seuls : ceux-là prennent un drapeau de trop, que le binaire refuse en
/// 1006 avant tout ; un mot inconnu y est refusé en 1005 de même.
fn est_connu(mot: &str, dossier: &Path) -> Result<(), String> {
    let mut commande = kraken(dossier);
    commande.arg(mot);
    if ["wait", "doctor"].contains(&mot) {
        commande.arg("--x");
    }
    let sortie = jouer(&mut commande);
    let erreur = String::from_utf8_lossy(&sortie.stderr).into_owned();
    if erreur.starts_with("lydite : 1005 ") {
        return Err(format!("« lydite {mot} » : le binaire ne connaît pas ce mot : {erreur}"));
    }
    Ok(())
}

/// Le texte servi par le binaire : ce que `lydite skill` écrit.
fn texte_servi() -> String {
    let dossier = neuf("servi");
    String::from_utf8(jouer(kraken(&dossier).arg("skill")).stdout).unwrap()
}

#[test]
fn la_skill_servie_ne_cite_que_des_commandes_que_le_binaire_connait() {
    let texte = texte_servi();
    let citees = commandes_citees(&texte);
    // Le relevé voit bien les commandes que la skill doit citer : un relevé vide ne ferait rien rougir.
    for attendu in ["launch", "status", "lessons", "plan", "accept", "doctor", "wait"] {
        assert!(citees.contains(attendu), "la skill ne cite pas `lydite {attendu}` : {citees:?}");
    }
    let dossier = neuf("connus");
    let inconnus: Vec<String> = citees.iter().filter_map(|mot| est_connu(mot, &dossier).err()).collect();
    assert!(inconnus.is_empty(), "la skill cite des commandes inconnues du binaire :\n{}", inconnus.join("\n"));
}

#[test]
fn le_releve_voit_un_mot_inconnu_et_le_binaire_le_refuse_en_1005() {
    // Le mot que la mutation ajouterait à la skill : le relevé le trouve, et le binaire le refuse en 1005.
    let citees = commandes_citees("Run `lydite frobnicate` now, with the lydite plugin and lydite_lancer.");
    assert_eq!(citees, BTreeSet::from(["frobnicate".to_string()]));
    let dossier = neuf("inconnu");
    let erreur = est_connu("frobnicate", &dossier).unwrap_err();
    assert!(erreur.contains("1005 contrat.commande_inconnue"), "{erreur}");
    assert!(est_connu("skill", &dossier).is_ok());
}
