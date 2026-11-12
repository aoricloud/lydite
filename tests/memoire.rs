//! `lydite memory` et `lydite memory recall <address>`, joués sur le binaire lui-même, sur un fil que le test écrit
//! dans un dossier de Lydite neuf : ils lisent le fil, n'écrivent rien et ne lancent aucune main.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-memoire-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le binaire, dont le dossier de Lydite est `dossier` et dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des
/// leurres sous le dossier temporaire, jamais créés : posés sur l'enfant seul, ils ne touchent pas le processus du
/// test, et le vrai dossier personnel de l'humain n'est jamais lu.
fn kraken(dossier: &Path) -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-memoire-leurres-{}", std::process::id()));
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

/// La ligne d'ouverture d'une vague, à la séance et au `seq` donnés, sans son `\n`.
fn ligne(seance: &str, seq: u64, objectif: &str) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","origine":"humain","#,
            r#""origine_foi":"declare","type":"vague.ouverte","vague":"v","n":1,"depot":"/srv/depot","#,
            r#""verifier":null,"objectif":"{objectif}"}}"#,
        ),
        seance = seance,
        seq = seq,
        objectif = objectif,
    )
}

/// Écrit le fil du dossier : ces lignes, chacune finie par `\n`.
fn ecrire_le_fil(dossier: &Path, lignes: &[String]) {
    fs::write(dossier.join("fil.jsonl"), lignes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>()).unwrap();
}

/// Le texte d'une sortie, sans les `\r` qu'une plateforme aurait pu y mettre.
fn texte(octets: &[u8]) -> String {
    String::from_utf8(octets.to_vec()).unwrap().replace("\r\n", "\n")
}

/// Un refus : le code sort en 2, rien n'est écrit sur la sortie, et la sortie d'erreur tient en quatre lignes, dont la
/// première porte le code et son nom. Rend la sortie d'erreur.
fn refus(sortie: &Output, premiere: &str) -> String {
    let erreur = texte(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{erreur}");
    assert!(sortie.stdout.is_empty(), "un refus n'écrit rien sur la sortie : {}", texte(&sortie.stdout));
    assert_eq!(erreur.lines().count(), 4, "{erreur}");
    assert_eq!(erreur.lines().next(), Some(premiere), "{erreur}");
    erreur
}

#[test]
fn a_recalled_line_is_the_line_it_cites() {
    let dossier = neuf("rappel");
    let ecrite = ligne("s", 1, "premier");
    ecrire_le_fil(&dossier, std::slice::from_ref(&ecrite));
    let index = jouer(kraken(&dossier).arg("memory"));
    assert_eq!(index.status.code(), Some(0), "{}", texte(&index.stderr));
    assert!(index.stderr.is_empty(), "{}", texte(&index.stderr));
    let index = texte(&index.stdout);
    // Une note : une ligne de six champs, l'adresse, l'empreinte, le genre, l'instant, la foi et la taille.
    assert_eq!(index.lines().count(), 1, "{index}");
    let champs: Vec<&str> = index.trim_end().split(' ').collect();
    assert_eq!(champs.len(), 6, "{index}");
    assert_eq!(champs[0], "s/1");
    assert_eq!(champs[2], "vague.ouverte");
    assert_eq!(champs[3], "2026-10-31T21:03:00.123Z");
    assert_eq!(champs[4], "-");
    assert_eq!(champs[5], ecrite.len().to_string());
    // Le rappel, par l'adresse puis par l'empreinte lue dans l'index, rend la ligne à l'octet, suivie d'un `\n`.
    let attendu = format!("{ecrite}\n").into_bytes();
    for cle in [champs[0], champs[1]] {
        let rappel = jouer(kraken(&dossier).args(["memory", "recall", cle]));
        assert_eq!(rappel.status.code(), Some(0), "{cle}: {}", texte(&rappel.stderr));
        assert_eq!(rappel.stdout, attendu, "{cle}");
        assert!(rappel.stderr.is_empty(), "{cle}: {}", texte(&rappel.stderr));
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn a_recall_of_a_missing_address_is_refused() {
    let dossier = neuf("absente");
    ecrire_le_fil(&dossier, &[ligne("s", 1, "premier")]);
    let inconnue = "0".repeat(64);
    // Une séance inconnue, un `seq` absent, une adresse sans chiffres, une empreinte bien formée que le fil n'a pas.
    for adresse in ["autre/1", "s/2", "x/abc", inconnue.as_str()] {
        let sortie = jouer(kraken(&dossier).args(["memory", "recall", adresse]));
        let erreur = refus(&sortie, "lydite : 6034 lancer.memoire_texte_absent");
        assert!(erreur.contains(adresse), "le refus cite l'adresse {adresse} : {erreur}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn a_recall_of_a_broken_hash_is_refused() {
    let dossier = neuf("empreinte");
    ecrire_le_fil(&dossier, &[ligne("s", 1, "premier")]);
    // 63 caractères, des majuscules, un mot : aucun `/`, et pas 64 hexadécimaux minuscules.
    for texte_donne in ["a".repeat(63), "A".repeat(64), "mot".to_string()] {
        let sortie = jouer(kraken(&dossier).args(["memory", "recall", &texte_donne]));
        let erreur = refus(&sortie, "lydite : 6035 lancer.memoire_empreinte_fausse");
        assert!(erreur.contains(&texte_donne), "le refus cite le texte {texte_donne} : {erreur}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn a_memory_index_does_not_write_the_thread() {
    let dossier = neuf("lecture-seule");
    ecrire_le_fil(&dossier, &[ligne("s", 1, "premier"), ligne("s", 2, "dernier")]);
    let avant = fs::read(dossier.join("fil.jsonl")).unwrap();
    assert_eq!(noms(&dossier), ["fil.jsonl"]);
    assert!(jouer(kraken(&dossier).arg("memory")).status.success());
    assert!(jouer(kraken(&dossier).args(["memory", "recall", "s/2"])).status.success());
    assert!(!jouer(kraken(&dossier).args(["memory", "recall", "s/3"])).status.success());
    assert_eq!(fs::read(dossier.join("fil.jsonl")).unwrap(), avant, "le fil n'a pas bougé");
    // Ni verrou, ni fichier, ni dossier de plus : le dossier ne contient que le fil.
    assert_eq!(noms(&dossier), ["fil.jsonl"]);
    // Un dossier qui n'existe pas encore reste absent : l'index est vide, en 0.
    let dossier_absent = neuf("absent");
    let absent = dossier_absent.join("pas-encore");
    let sortie = jouer(kraken(&absent).arg("memory"));
    assert_eq!(sortie.status.code(), Some(0), "{}", texte(&sortie.stderr));
    assert!(sortie.stdout.is_empty() && sortie.stderr.is_empty());
    assert!(!absent.exists(), "{} a été créé", absent.display());
    let _ = fs::remove_dir_all(&dossier);
    let _ = fs::remove_dir_all(&dossier_absent);
}

#[test]
fn a_memory_index_skips_a_broken_line_like_status() {
    let dossier = neuf("ligne-cassee");
    let (premiere, derniere) = (ligne("s", 1, "premier"), ligne("s", 2, "dernier"));
    ecrire_le_fil(&dossier, &[premiere, "pas du json".to_string(), derniere]);
    let index = jouer(kraken(&dossier).arg("memory"));
    assert_eq!(index.status.code(), Some(0), "{}", texte(&index.stderr));
    // Les deux notes sont dans l'index, la ligne cassée n'y est pas.
    let dit = texte(&index.stdout);
    let adresses: Vec<&str> = dit.lines().map(|ligne| ligne.split(' ').next().unwrap()).collect();
    assert_eq!(adresses, ["s/1", "s/2"]);
    // L'écart est dit sur la sortie d'erreur, au numéro de la ligne cassée, et `lydite status` dit le même.
    let ecart = texte(&index.stderr);
    assert!(ecart.starts_with("lydite : 1001 "), "{ecart}");
    assert!(ecart.contains("ligne 2 :"), "{ecart}");
    let etat = jouer(kraken(&dossier).arg("status"));
    assert_eq!(texte(&etat.stderr), ecart);
    let _ = fs::remove_dir_all(&dossier);
}

/// Un faux programme dans `dir` qui note son appel : sous Windows un `.bat`, ailleurs un script `#!/bin/sh` exécutable.
/// Sa note est le fichier `<son chemin>.note`, où chaque appel ajoute une ligne. Rend son chemin.
fn faux(dir: &Path, nom: &str) -> PathBuf {
    let chemin = dir.join(if cfg!(windows) { format!("{nom}.bat") } else { nom.to_string() });
    let script = if cfg!(windows) {
        "@echo off\r\necho appel>> \"%~f0.note\"\r\n"
    } else {
        "#!/bin/sh\nprintf 'appel\\n' >> \"$0.note\"\n"
    };
    fs::write(&chemin, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
    }
    chemin
}

/// La note du faux programme : ce que ses appels ont écrit, `None` s'il n'a jamais été appelé.
fn note_de(chemin: &Path) -> Option<String> {
    let mut note = chemin.as_os_str().to_owned();
    note.push(".note");
    fs::read_to_string(PathBuf::from(note)).ok().map(|texte| texte.replace("\r\n", "\n"))
}

/// Appelle le faux programme. Un autre test qui lance un processus au même instant tient un moment le descripteur
/// d'écriture du script, et son `exec` rend alors « Text file busy » : l'appel se reprend, et le délai est un
/// garde-fou, non une attente.
fn appeler(chemin: &Path) {
    let debut = Instant::now();
    loop {
        match Command::new(chemin).stdin(Stdio::null()).stdout(Stdio::null()).status() {
            Err(e) if e.kind() == ErrorKind::ExecutableFileBusy && debut.elapsed() < Duration::from_secs(30) => {
                thread::sleep(Duration::from_millis(10));
            }
            autre => return assert!(autre.unwrap().success(), "{} ne s'appelle pas", chemin.display()),
        }
    }
}

#[test]
fn a_read_only_memory_command_launches_nothing() {
    let dossier = neuf("rien-lance");
    let faux_dir = neuf("rien-lance-faux");
    let programmes: Vec<PathBuf> = ["claude", "gh", "git", "srt"].iter().map(|nom| faux(&faux_dir, nom)).collect();
    // Preuve que la note peut s'écrire : un des faux programmes, appelé ici, laisse sa note. Sans elle, l'absence de
    // note après les commandes ne prouverait rien.
    appeler(&programmes[1]);
    assert_eq!(note_de(&programmes[1]).as_deref(), Some("appel\n"));
    assert!(programmes.iter().enumerate().all(|(i, p)| i == 1 || note_de(p).is_none()));
    ecrire_le_fil(&dossier, &[ligne("s", 1, "premier")]);
    let reduite = |commande: &mut Command| {
        commande
            .env("PATH", &faux_dir)
            .env_remove("LYDITE_CLAUDE")
            .env_remove("LYDITE_GH")
            .env_remove("LYDITE_SRT")
            .output()
            .unwrap()
    };
    let index = reduite(kraken(&dossier).arg("memory"));
    assert_eq!(index.status.code(), Some(0), "{}", texte(&index.stderr));
    assert_eq!(texte(&index.stdout).lines().count(), 1);
    let rappel = reduite(kraken(&dossier).args(["memory", "recall", "s/1"]));
    assert_eq!(rappel.status.code(), Some(0), "{}", texte(&rappel.stderr));
    assert_eq!(rappel.stdout, format!("{}\n", ligne("s", 1, "premier")).into_bytes());
    // Aucune note de plus : le `gh` appelé ici n'a toujours que son appel, et les trois autres n'ont aucune note.
    assert_eq!(note_de(&programmes[1]).as_deref(), Some("appel\n"));
    for i in [0, 2, 3] {
        assert_eq!(note_de(&programmes[i]), None, "{} a été appelé", programmes[i].display());
    }
    let _ = fs::remove_dir_all(&dossier);
    let _ = fs::remove_dir_all(&faux_dir);
}
