//! `lydite doctor`, joué sur le binaire lui-même, avec de faux programmes que rien de l'humain ne remplace.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
    // Ses imports vivent ici : sous Windows, où `chauffer` n'existe pas, ils seraient inutilisés.
    use std::process::Stdio;
    use std::thread;
    use std::time::{Duration, Instant};
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

// Le mur natif n'existe que sous Linux, et srt n'y tient pas sous Windows : `doctor` y dit « skipped srt » pour tout
// profil. Ce test, qui attend `missing srt` pour le profil `mur = exige`, ne se joue que là où le mur existe.
#[cfg(unix)]
#[test]
fn doctor_d_un_profil_natif_verifie_bwrap_et_lydite_reseau_a_la_place_de_srt() {
    let (dir, (dossier, _)) = (neuf("natif"), dossier_de_lydite("natif"));
    let depot = dir.join("depot");
    fs::create_dir_all(&depot).unwrap();
    git(&depot, &["init", "-q"]);
    git(&depot, &["commit", "-q", "--allow-empty", "-m", "init"]);
    fs::create_dir_all(dossier.join("profils")).unwrap();
    // Le même profil, au mur natif (`p`) puis au mur de srt (`s`).
    fs::write(dossier.join("profils").join("p.profil"), "sorte = claude\nmur = natif\n").unwrap();
    fs::write(dossier.join("profils").join("s.profil"), "sorte = claude\nmur = exige\n").unwrap();
    // `lydite-reseau` est là, `bwrap` et srt n'y sont pas : aucun mur ne tient, et `doctor` dit ce qui manque à celui
    // que le profil choisit.
    let reseau = dir.join("lydite-reseau");
    fs::write(&reseau, "x").unwrap();
    let doctor = || {
        let mut commande = kraken();
        avec_les_faux(commande.current_dir(&depot).env("LYDITE_DOSSIER", &dossier), &dir);
        commande
            .env("LYDITE_SRT", dir.join("srt-n-existe-pas"))
            .env("LYDITE_BWRAP", dir.join("bwrap-n-existe-pas"))
            .env("LYDITE_RESEAU", &reseau);
        commande
    };
    for profil in ["p", "s"] {
        let accepter = doctor().args(["accepter", profil]).output().unwrap();
        assert!(accepter.status.success(), "{}", String::from_utf8_lossy(&accepter.stderr));
    }
    let a = |lignes: &[String], debut: &str| lignes.iter().any(|ligne| ligne.starts_with(debut));
    let (lignes, code) = jouer(doctor().args(["doctor", "p"]));
    assert_eq!(code, 1, "{lignes:?}");
    assert!(a(&lignes, "missing bwrap: `") && a(&lignes, "missing profile: p, refusal 6028"), "{lignes:?}");
    let bwrap = lignes.iter().find(|ligne| ligne.starts_with("missing bwrap: ")).unwrap();
    assert!(bwrap.contains(" Fix: install bubblewrap ") && bwrap.contains("LYDITE_BWRAP"), "{bwrap}");
    // Sans ligne `reseau`, le rejeu n'a pas besoin du filtre : `lydite-reseau` n'est ni vérifié, ni compté.
    let sans_filtre =
        "skipped lydite-reseau: the profile allows no domain, so the native wall needs no network filter.";
    assert!(lignes.contains(&sans_filtre.to_string()), "{lignes:?}");
    assert!(!a(&lignes, "ok lydite-reseau") && !a(&lignes, "missing lydite-reseau"), "{lignes:?}");
    assert!(a(&lignes, "skipped srt: the profile uses the native wall"), "{lignes:?}");
    assert!(!a(&lignes, "missing srt"), "{lignes:?}");
    // Le même profil natif avec des domaines : le filtre se vérifie. Présent, il est dit `ok` ; absent, il manque, et
    // la correction dit d'abord que l'archive le livre.
    fs::write(dossier.join("profils").join("d.profil"), "sorte = claude\nmur = natif\nreseau = docs.exemple.test\n")
        .unwrap();
    let accepter = doctor().args(["accepter", "d"]).output().unwrap();
    assert!(accepter.status.success(), "{}", String::from_utf8_lossy(&accepter.stderr));
    let (lignes, code) = jouer(doctor().args(["doctor", "d"]));
    assert_eq!(code, 1, "{lignes:?}");
    assert!(lignes.contains(&format!("ok lydite-reseau: {}", reseau.display())), "{lignes:?}");
    assert!(!a(&lignes, "skipped lydite-reseau"), "{lignes:?}");
    let (lignes, _) = jouer(doctor().env("LYDITE_RESEAU", dir.join("reseau-absent")).args(["doctor", "d"]));
    let manque = lignes.iter().find(|ligne| ligne.starts_with("missing lydite-reseau: ")).unwrap_or_else(|| {
        panic!("{lignes:?}");
    });
    let archive = " Fix: the Linux archive ships it next to `lydite`: put it there, or name its path in LYDITE_RESEAU \
                   (from the source: `cargo build --release --manifest-path reseau/Cargo.toml`).";
    assert!(manque.ends_with(archive), "{manque}");
    // Le profil au mur de srt : srt se vérifie, et ni bwrap ni lydite-reseau.
    let (lignes, code) = jouer(doctor().args(["doctor", "s"]));
    assert_eq!(code, 1, "{lignes:?}");
    assert!(a(&lignes, "missing srt: `") && a(&lignes, "missing profile: s, refusal 6028"), "{lignes:?}");
    assert!(!a(&lignes, "missing bwrap") && !a(&lignes, "ok bwrap") && !a(&lignes, "ok lydite-reseau"), "{lignes:?}");
    // Un profil natif pas encore accepté (6011) : le mur se lit dans son fichier, et srt n'est pas demandé.
    fs::write(dossier.join("profils").join("n.profil"), "sorte = claude\nmur = natif\n").unwrap();
    let (lignes, code) = jouer(doctor().args(["doctor", "n"]));
    assert_eq!(code, 1, "{lignes:?}");
    assert!(a(&lignes, "missing profile: n, refusal 6011"), "{lignes:?}");
    assert!(
        a(&lignes, "missing bwrap: `") && a(&lignes, "skipped srt: the profile uses the native wall"),
        "{lignes:?}"
    );
    assert!(!a(&lignes, "missing srt"), "{lignes:?}");
    for dir in [&dir, &dossier] {
        fs::remove_dir_all(dir).unwrap();
    }
}
