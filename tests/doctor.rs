//! `lydite doctor`, joué sur le binaire lui-même, avec de faux programmes que rien de l'humain ne remplace.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// Le binaire, dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des leurres sous le dossier temporaire, jamais
/// créés : posés sur l'enfant seul, ils ne touchent pas le processus du test, et le vrai dossier personnel de l'humain
/// n'est jamais lu.
fn kraken() -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-doctor-leurres-{}", std::process::id()));
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande
        .env("USERPROFILE", leurres.join("personnel"))
        .env("HOME", leurres.join("personnel"))
        .env("CLAUDE_CONFIG_DIR", leurres.join("config"));
    commande
}

/// Un faux programme dans `dir` : sous Windows un `.bat`, ailleurs un script `#!/bin/sh` exécutable. Il dit une ligne
/// et rend le code qu'on lui donne. Rend son chemin.
fn faux(dir: &Path, nom: &str, code: u8) -> OsString {
    if cfg!(windows) {
        let chemin = dir.join(format!("{nom}.bat"));
        fs::write(&chemin, format!("@echo off\r\necho {nom} 9.9\r\nexit /b {code}\r\n")).unwrap();
        chemin.into_os_string()
    } else {
        let chemin = dir.join(nom);
        fs::write(&chemin, format!("#!/bin/sh\n[ \"$1\" = --chauffe ] && exit 0\necho '{nom} 9.9'\nexit {code}\n"))
            .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&chemin, fs::Permissions::from_mode(0o755)).unwrap();
            chauffer(&chemin);
        }
        chemin.into_os_string()
    }
}

/// Lance le script qu'on vient d'écrire, sans rien jouer (`--chauffe`), jusqu'à ce qu'il parte : un autre test qui
/// lance un processus au même instant tient un moment le descripteur d'écriture du script, et son `exec` rend alors
/// « Text file busy ». Le délai est un garde-fou, non une attente.
#[cfg(unix)]
fn chauffer(chemin: &Path) {
    let debut = Instant::now();
    loop {
        let lancement = Command::new(chemin).arg("--chauffe").stdin(Stdio::null()).stdout(Stdio::null()).status();
        match lancement {
            Err(e)
                if e.kind() == std::io::ErrorKind::ExecutableFileBusy && debut.elapsed() < Duration::from_secs(30) =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            autre => return assert!(autre.unwrap().success(), "{} ne chauffe pas", chemin.display()),
        }
    }
}

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-doctor-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Un dossier de Lydite neuf, et s'il est assez court : son chemin tient dans les 51 octets que le socket de pont d'un
/// bac laisse (`doctor` le refuse au-delà, hors de Windows). Le dossier temporaire du système, s'il est assez court,
/// sinon `/tmp`, sinon un dossier long, dans le dossier temporaire : un système dont le dossier temporaire est long, et
/// où `/tmp` ne s'écrit pas, n'a aucun dossier court à donner.
fn dossier_de_lydite(nom: &str) -> (PathBuf, bool) {
    let nom = format!("ld-{nom}-{}", std::process::id());
    for candidat in [std::env::temp_dir(), PathBuf::from("/tmp")] {
        let dir = candidat.join(&nom);
        let _ = fs::remove_dir_all(&dir);
        let court = cfg!(windows) || dir.as_os_str().as_encoded_bytes().len() <= 51;
        if court && fs::create_dir_all(&dir).is_ok() {
            return (dir, true);
        }
    }
    let dir = std::env::temp_dir().join(nom);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    (dir, false)
}

/// Joue git dans ce dossier, avec une identité de passage : le dépôt jetable n'a pas de configuration.
fn git(dir: &Path, args: &[&str]) {
    let identite = ["-c", "user.name=t", "-c", "user.email=t@t"];
    let sortie = Command::new("git").current_dir(dir).args(identite).args(args).output().unwrap();
    assert!(sortie.status.success(), "git {args:?} : {}", String::from_utf8_lossy(&sortie.stderr));
}

/// Les trois faux programmes, tous à 0, dans `dir`, posés sur la commande : `LYDITE_GH`, `LYDITE_CLAUDE`, `LYDITE_SRT`.
fn avec_les_faux<'a>(commande: &'a mut Command, dir: &Path) -> &'a mut Command {
    commande
        .env("LYDITE_GH", faux(dir, "gh", 0))
        .env("LYDITE_CLAUDE", faux(dir, "claude", 0))
        .env("LYDITE_SRT", faux(dir, "srt", 0))
}

/// Les lignes que la commande a écrites sur stdout, et son code de sortie. Rien sur stderr : `doctor` n'est jamais un
/// refus (code 2) tant que la ligne se lit.
fn jouer(commande: &mut Command) -> (Vec<String>, i32) {
    let Output { status, stdout, stderr } = commande.output().unwrap();
    let erreur = String::from_utf8_lossy(&stderr);
    assert!(erreur.is_empty(), "{commande:?} écrit sur stderr : {erreur}");
    let lignes = String::from_utf8_lossy(&stdout).lines().map(str::to_string).collect();
    (lignes, status.code().unwrap())
}

/// Un dossier de Lydite court qui n'existe pas : `doctor` n'écrit rien, il ne le crée pas.
fn dossier_absent(nom: &str, dir: &Path) -> PathBuf {
    if cfg!(windows) { dir.join("absent") } else { PathBuf::from(format!("/ld-{nom}-{}", std::process::id())) }
}

#[test]
fn doctor_sort_en_0_et_finit_sur_ready_quand_tout_est_la() {
    let dir = neuf("pret");
    let dossier = dossier_absent("pret", &dir);
    let (lignes, code) = jouer(avec_les_faux(kraken().arg("doctor").env("LYDITE_DOSSIER", &dossier), &dir));
    assert_eq!(code, 0, "{lignes:?}");
    assert_eq!(lignes.last().map(String::as_str), Some("ready"), "{lignes:?}");
    assert!(lignes.iter().any(|ligne| ligne == "ok gh: logged in"), "{lignes:?}");
    assert!(lignes.iter().any(|ligne| ligne == "ok claude login: signed in"), "{lignes:?}");
    let creation = format!("ok state folder: {} (created at the first launch)", dossier.display());
    assert!(lignes.contains(&creation), "{lignes:?}");
    assert!(!dossier.exists(), "doctor a créé {}", dossier.display());
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn un_gh_qui_n_existe_pas_est_un_manque_et_le_code_1() {
    let dir = neuf("sans-gh");
    let dossier = dossier_absent("sans-gh", &dir);
    let mut commande = kraken();
    avec_les_faux(commande.arg("doctor").env("LYDITE_DOSSIER", &dossier), &dir);
    commande.env("LYDITE_GH", dir.join("gh-n-existe-pas"));
    let (lignes, code) = jouer(&mut commande);
    assert_eq!(code, 1, "{lignes:?}");
    let manque = lignes.iter().find(|ligne| ligne.starts_with("missing gh: ")).expect("une ligne `missing gh`");
    assert!(manque.contains(" Fix: install gh "), "{manque}");
    assert_eq!(lignes.last().map(String::as_str), Some("not ready: 1 missing"), "{lignes:?}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn doctor_d_un_profil_dit_le_refus_avant_accepter_et_sort_en_0_apres() {
    let (dir, (dossier, court)) = (neuf("profil"), dossier_de_lydite("profil"));
    let depot = dir.join("depot");
    fs::create_dir_all(&depot).unwrap();
    git(&depot, &["init", "-q"]);
    git(&depot, &["commit", "-q", "--allow-empty", "-m", "init"]);
    fs::create_dir_all(dossier.join("profils")).unwrap();
    fs::write(dossier.join("profils").join("p.profil"), "sorte = claude\nmur = demande\n").unwrap();
    let doctor = || {
        let mut commande = kraken();
        avec_les_faux(commande.current_dir(&depot).env("LYDITE_DOSSIER", &dossier), &dir);
        commande
    };
    // Avant `accepter` : le profil se lit, mais son empreinte n'est pas acceptée.
    let (lignes, code) = jouer(doctor().args(["doctor", "p"]));
    assert_eq!(code, 1, "{lignes:?}");
    assert!(lignes.contains(&"missing profile: p, refusal 6011".to_string()), "{lignes:?}");
    // Le dossier trop long d'un système au dossier temporaire long est un manque de plus, sans rapport avec le profil.
    let verdict = format!("not ready: {} missing", 1 + usize::from(!court));
    assert_eq!(lignes.last(), Some(&verdict), "{lignes:?}");
    // Après : le profil est accepté, et `doctor` dit prêt.
    let accepter = doctor().args(["accepter", "p"]).output().unwrap();
    assert!(accepter.status.success(), "{}", String::from_utf8_lossy(&accepter.stderr));
    let (lignes, code) = jouer(doctor().args(["doctor", "p"]));
    assert!(lignes.contains(&"ok profile: p accepted, ready to launch here".to_string()), "{lignes:?}");
    if court {
        assert_eq!(code, 0, "{lignes:?}");
        assert_eq!(lignes.last().map(String::as_str), Some("ready"), "{lignes:?}");
    } else {
        // Le dossier de ce système est trop long pour le socket de pont : c'est le seul manque.
        let manques: Vec<&String> = lignes.iter().filter(|ligne| ligne.starts_with("missing ")).collect();
        assert!(matches!(manques[..], [m] if m.starts_with("missing state folder: ")), "{lignes:?}");
        assert_eq!((code, lignes.last().map(String::as_str)), (1, Some("not ready: 1 missing")), "{lignes:?}");
    }
    for dir in [&dir, &dossier] {
        fs::remove_dir_all(dir).unwrap();
    }
}
