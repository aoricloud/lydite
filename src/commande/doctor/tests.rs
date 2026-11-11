use super::*;
use crate::coeur::erreur::Code;
use crate::commande::ligne::{Commande, options};
use crate::essais::{effacer, racine, script};
use std::fs;
use std::path::Path;

/// Un faux programme dans `dir` : sous Windows un `.bat`, ailleurs un script `#!/bin/sh` exécutable. Rend son chemin.
fn faux(dir: &Path, nom: &str, unix: &str, windows: &str) -> OsString {
    if cfg!(windows) {
        let chemin = dir.join(format!("{nom}.bat"));
        fs::write(&chemin, format!("@echo off\r\n{}\r\n", windows.replace('\n', "\r\n"))).unwrap();
        chemin.into_os_string()
    } else {
        let chemin = dir.join(nom);
        script(&chemin, &format!("[ \"$1\" = --chauffe ] && exit 0\n{unix}"));
        #[cfg(unix)]
        chauffer(&chemin);
        chemin.into_os_string()
    }
}

/// Lance le script qu'on vient d'écrire, sans rien jouer (`--chauffe`), jusqu'à ce qu'il parte. Un autre test qui lance
/// un processus au même instant tient un moment le descripteur d'écriture du script, et son `exec` rend alors « Text
/// file busy » : sans cela, ce test tomberait au hasard de la charge. Le délai est un garde-fou, non une attente.
#[cfg(unix)]
fn chauffer(chemin: &Path) {
    let debut = std::time::Instant::now();
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

/// Un faux `git` qui dit sa version sur deux lignes : seule la première se montre.
fn faux_git(dir: &Path) -> OsString {
    faux(
        dir,
        "git",
        "echo 'git version 9.9'; echo 'git: seconde ligne'",
        "echo git version 9.9\necho git: seconde ligne",
    )
}

/// Un faux `gh` qui écrit le nom d'un compte et rend `code`.
fn faux_gh(dir: &Path, code: u8) -> OsString {
    // Le code est dans le nom : deux faux de codes différents ne s'écrasent pas.
    let (unix, windows) = (format!("echo 'compte-de-gh'; exit {code}"), format!("echo compte-de-gh\nexit /b {code}"));
    faux(dir, &format!("gh{code}"), &unix, &windows)
}

/// Un faux `claude` : `auth status` écrit un courriel et rend `code`, le reste dit la version.
fn faux_claude(dir: &Path, code: u8) -> OsString {
    let unix = format!("if [ \"$1\" = auth ]; then echo 'someone@example.com'; exit {code}; fi\necho 'claude 9.9'");
    let windows = format!("if \"%1\"==\"auth\" (echo someone@example.com& exit /b {code})\necho claude 9.9");
    faux(dir, &format!("claude{code}"), &unix, &windows)
}

/// Un faux `srt` qui dit sa version.
fn faux_srt(dir: &Path) -> OsString {
    faux(dir, "srt", "echo 'srt 9.9'", "echo srt 9.9")
}

/// Un faux `bwrap` qui dit sa version.
fn faux_bwrap(dir: &Path) -> OsString {
    faux(dir, "bwrap", "echo 'bubblewrap 9.9'", "echo bubblewrap 9.9")
}

/// Un faux `lydite-reseau` : un fichier, que `doctor` ne lance pas.
fn faux_reseau(dir: &Path) -> PathBuf {
    let chemin = dir.join("lydite-reseau");
    fs::write(&chemin, "x").unwrap();
    chemin
}

/// Tous les programmes présents et connectés, dans `dir`.
fn tout_present(dir: &Path) -> Programmes {
    Programmes {
        git: faux_git(dir),
        gh: faux_gh(dir, 0),
        claude: faux_claude(dir, 0),
        srt: faux_srt(dir),
        bwrap: faux_bwrap(dir),
        reseau: Ok(faux_reseau(dir)),
        delai_git: DELAI,
        delai_gh: DELAI,
        delai_claude: DELAI,
        delai_srt: DELAI,
        delai_bwrap: DELAI,
    }
}

/// Un programme qui n'existe pas.
fn absent(dir: &Path) -> OsString {
    dir.join("n-existe-pas").into_os_string()
}

/// Un dossier de Lydite court, qui n'existe pas : `doctor` n'écrit rien, il ne le crée pas.
fn dossier_court() -> Result<PathBuf, Refus> {
    Ok(PathBuf::from(if cfg!(windows) { r"C:\lydite-doctor-absent" } else { "/lydite-doctor-absent" }))
}

const PROFIL_SAUTE: &str =
    "skipped profile: name a profile, from your repository, to check it and its wall: lydite doctor <profile>";

/// Les lignes de `doctor` quand tout est en ordre et qu'aucun profil n'est nommé, hors de Windows : un `srt` présent.
fn lignes_pretes() -> Vec<String> {
    let dossier = dossier_court().unwrap();
    [
        "ok git: git version 9.9".to_string(),
        "ok gh: logged in".into(),
        "ok claude: claude 9.9".into(),
        "ok claude login: signed in".into(),
        "ok srt: srt 9.9".into(),
        format!("ok state folder: {} (created at the first launch)", dossier.display()),
        PROFIL_SAUTE.into(),
        "ready".into(),
    ]
    .to_vec()
}

#[test]
fn tout_present_donne_ready_et_le_code_0() {
    let dir = racine("doctor-pret");
    let (lignes, code) = verifier(&tout_present(&dir), &dossier_court(), None, false);
    assert_eq!(lignes, lignes_pretes());
    assert_eq!(code, 0);
    effacer(&dir);
}

/// Les lignes prêtes dont la ligne `rang` est `ligne`, et le verdict d'un seul manque.
fn avec_un_manque(rang: usize, ligne: &str) -> Vec<String> {
    let mut lignes = lignes_pretes();
    lignes[rang] = ligne.into();
    *lignes.last_mut().unwrap() = "not ready: 1 missing".into();
    lignes
}

#[test]
fn chaque_manque_donne_sa_ligne_missing_avec_son_fix() {
    let dir = racine("doctor-manques");
    let tout = || tout_present(&dir);
    let installer_srt = "install it as the \"Try it\" section of the README says, or name its path in LYDITE_SRT.";
    let connexion = "run `claude auth login`. A profile that names another provider (`api`) needs no Claude account.";
    let cas: Vec<(Programmes, usize, String)> = vec![
        // Le détail d'un programme absent nomme la commande et l'erreur : ici le début, la fin se juge plus bas.
        (Programmes { git: absent(&dir), ..tout() }, 0, "missing git: `".to_string()),
        (Programmes { gh: absent(&dir), ..tout() }, 1, "missing gh: `".into()),
        (
            Programmes { gh: faux_gh(&dir, 1), ..tout() },
            1,
            "missing gh: not logged in. Fix: run `gh auth login`.".into(),
        ),
        (
            Programmes { claude: faux_claude(&dir, 1), ..tout() },
            3,
            format!("missing claude login: not signed in. Fix: {connexion}"),
        ),
        (Programmes { srt: absent(&dir), ..tout() }, 4, "missing srt: `".into()),
    ];
    for (programmes, rang, attendue) in cas {
        let (lignes, code) = verifier(&programmes, &dossier_court(), None, false);
        assert_eq!(code, 1, "{attendue} : {lignes:?}");
        // Les autres lignes sont celles du cas prêt ; celle du manque est la seule qui change.
        let mut autres = lignes.clone();
        autres[rang] = String::new();
        let mut prete = avec_un_manque(rang, "");
        prete[rang] = String::new();
        assert_eq!(autres, prete, "{attendue}");
        assert!(lignes[rang].starts_with(&attendue), "{} ne commence pas par {attendue}", lignes[rang]);
    }
    // Un programme absent dit son erreur et quoi faire, par son propre Fix.
    let (lignes, _) = verifier(&Programmes { git: absent(&dir), ..tout() }, &dossier_court(), None, false);
    assert!(lignes[0].ends_with(" Fix: install git."), "{}", lignes[0]);
    let (lignes, _) = verifier(&Programmes { gh: absent(&dir), ..tout() }, &dossier_court(), None, false);
    let fix = " Fix: install gh (https://cli.github.com) or name its path in LYDITE_GH.";
    assert!(lignes[1].ends_with(fix), "{}", lignes[1]);
    let (lignes, _) = verifier(&Programmes { srt: absent(&dir), ..tout() }, &dossier_court(), None, false);
    assert!(lignes[4].ends_with(&format!(" Fix: {installer_srt}")), "{}", lignes[4]);
    // claude absent manque deux fois : le programme, et sa connexion, qu'on ne peut pas lire.
    let (lignes, code) = verifier(&Programmes { claude: absent(&dir), ..tout() }, &dossier_court(), None, false);
    let installer = " Fix: install Claude Code (https://claude.com/claude-code) or name its path in LYDITE_CLAUDE.";
    assert!(lignes[2].starts_with("missing claude: `") && lignes[2].ends_with(installer), "{}", lignes[2]);
    assert!(lignes[3].starts_with("missing claude login: `"), "{}", lignes[3]);
    assert!(lignes[3].ends_with(&format!(" Fix: {connexion}")), "{}", lignes[3]);
    assert_eq!((lignes.last().unwrap().as_str(), code), ("not ready: 2 missing", 1));
    effacer(&dir);
}

#[test]
fn un_dossier_refuse_est_un_manque_avec_sa_preuve_et_sa_correction() {
    let dir = racine("doctor-dossier-refuse");
    let refus = Refus::new(
        Code::DossierRelatif,
        "LYDITE_DOSSIER vaut « x », qui n'est pas un chemin absolu",
        "Pose LYDITE_DOSSIER à un chemin absolu, ou retire-la.",
    );
    let (lignes, code) = verifier(&tout_present(&dir), &Err(refus), None, false);
    let manque = "missing state folder: LYDITE_DOSSIER vaut « x », qui n'est pas un chemin absolu \
                  Fix: Pose LYDITE_DOSSIER à un chemin absolu, ou retire-la.";
    assert_eq!(lignes, avec_un_manque(5, manque));
    assert_eq!(code, 1);
    effacer(&dir);
}

#[test]
fn un_dossier_existant_ne_dit_pas_qu_il_sera_cree() {
    let dir = racine("doctor-dossier-existant");
    let (lignes, _) = verifier(&tout_present(&dir), &Ok(dir.clone()), None, true);
    assert_eq!(lignes[5], format!("ok state folder: {}", dir.display()));
    effacer(&dir);
}

#[test]
fn un_dossier_trop_long_pour_le_socket_de_pont_est_un_manque_hors_de_windows() {
    let dir = racine("doctor-socket");
    // Le socket que Claude Code ouvre dans un bac ajoute 56 octets au dossier : 51 octets de dossier, 107 de chemin.
    let dossier = |octets: usize| Ok(PathBuf::from(format!("/{}", "a".repeat(octets - 1))));
    let (lignes, code) = verifier(&tout_present(&dir), &dossier(51), None, false);
    assert_eq!((lignes[5].starts_with("ok state folder: /aaa"), code), (true, 0), "{lignes:?}");
    let trop_long = dossier(52);
    let (lignes, code) = verifier(&tout_present(&dir), &trop_long, None, false);
    let chemin = trop_long.as_ref().unwrap().display().to_string();
    let detail = format!(
        "missing state folder: {chemin} is 52 bytes long and may not exceed 51: the bridge socket Claude Code opens \
         in a sandbox below it would be 108 bytes, over the 107 a socket path holds. \
         Fix: set LYDITE_DOSSIER to a shorter absolute path."
    );
    assert_eq!(lignes, avec_un_manque(5, &detail));
    assert_eq!(code, 1);
    // Sous Windows, le chemin d'un socket n'est pas contrôlé.
    let (lignes, code) = verifier(&tout_present(&dir), &trop_long, None, true);
    assert_eq!(lignes[5], format!("ok state folder: {chemin} (created at the first launch)"));
    assert_eq!(code, 0, "{lignes:?}");
    effacer(&dir);
}

#[test]
fn sous_windows_srt_est_saute_meme_absent() {
    let dir = racine("doctor-windows");
    let programmes = Programmes { srt: absent(&dir), ..tout_present(&dir) };
    let (lignes, code) = verifier(&programmes, &dossier_court(), None, true);
    let saute = "skipped srt: srt does not hold under Windows: a profile there asks `mur = demande` \
                 (see \"Windows\" in the README).";
    assert_eq!(lignes[4], saute);
    assert_eq!((lignes.last().unwrap().as_str(), code), ("ready", 0));
    effacer(&dir);
}

#[test]
fn le_profil_nomme_se_dit_accepte_ou_refuse() {
    let dir = racine("doctor-profil");
    let programmes = tout_present(&dir);
    let vu = |fournisseur, mur| Ok(ProfilVu { fournisseur, mur });
    for (mur, fin) in [(true, ", wall checked"), (false, "")] {
        let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", false, &vu(false, mur))), false);
        let mut attendues = lignes_pretes();
        attendues[6] = format!("ok profile: p accepted, ready to launch here{fin}");
        assert_eq!((lignes, code), (attendues, 0));
    }
    // Un refus : sa ligne, son numéro, puis ses quatre lignes, chacune sous deux espaces.
    let refus = Refus::new(Code::ProfilNonAccepte, "le profil p n'est pas accepté", "Lance : lydite accepter p.");
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", false, &Err(refus.clone()))), false);
    let mut attendues = lignes_pretes();
    attendues.splice(6..7, ["missing profile: p, refusal 6011".to_string()]);
    attendues.splice(7..7, refus.to_string().lines().map(|ligne| format!("  {ligne}")));
    *attendues.last_mut().unwrap() = "not ready: 1 missing".into();
    assert_eq!(lignes, attendues);
    assert_eq!(attendues.len(), 12);
    assert_eq!(code, 1);
    effacer(&dir);
}

#[test]
fn un_profil_qui_nomme_un_fournisseur_n_a_pas_besoin_d_un_compte_claude() {
    let dir = racine("doctor-fournisseur");
    let programmes = Programmes { claude: faux_claude(&dir, 1), ..tout_present(&dir) };
    let profil = Ok(ProfilVu { fournisseur: true, mur: true });
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("api", false, &profil)), false);
    let saute = "skipped claude login: the profile names another provider, whose hands need no Claude account.";
    assert_eq!(lignes[3], saute);
    assert_eq!((lignes.last().unwrap().as_str(), code), ("ready", 0));
    // Sans fournisseur, le même `claude auth status` rouge est un manque.
    let profil = Ok(ProfilVu { fournisseur: false, mur: true });
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("claude", false, &profil)), false);
    assert!(lignes[3].starts_with("missing claude login: not signed in."), "{}", lignes[3]);
    assert_eq!(code, 1);
    effacer(&dir);
}

/// Le profil `nom`, dit au mur natif ou non, avec ce que `preparer` en a dit.
fn nomme<'a>(nom: &'a str, natif: bool, vu: &'a Result<ProfilVu, Refus>) -> ProfilNomme<'a> {
    ProfilNomme { nom, natif, vu }
}

/// Un profil que `preparer` a accepté, au mur exigé.
fn accepte() -> Result<ProfilVu, Refus> {
    Ok(ProfilVu { fournisseur: false, mur: true })
}

/// La ligne de `doctor` qui parle du programme `nom` (`ok <nom>:`, `skipped <nom>:` ou `missing <nom>:`), ou `None`.
fn ligne_de<'a>(lignes: &'a [String], nom: &str) -> Option<&'a String> {
    lignes
        .iter()
        .find(|ligne| ["ok", "skipped", "missing"].iter().any(|etat| ligne.starts_with(&format!("{etat} {nom}:"))))
}

#[test]
fn un_profil_natif_verifie_bwrap_et_lydite_reseau_a_la_place_de_srt() {
    let dir = racine("doctor-natif");
    // srt manque, et le profil n'en a pas besoin : rien ne manque.
    let programmes = Programmes { srt: absent(&dir), ..tout_present(&dir) };
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &accepte())), false);
    let reseau = faux_reseau(&dir);
    let attendues = [
        "ok git: git version 9.9".to_string(),
        "ok gh: logged in".into(),
        "ok claude: claude 9.9".into(),
        "ok claude login: signed in".into(),
        "ok bwrap: bubblewrap 9.9".into(),
        format!("ok lydite-reseau: {}", reseau.display()),
        "skipped srt: the profile uses the native wall (`mur = natif`), which runs bubblewrap itself: it needs no srt."
            .into(),
        format!("ok state folder: {} (created at the first launch)", dossier_court().unwrap().display()),
        "ok profile: p accepted, ready to launch here, wall checked".into(),
        "ready".into(),
    ];
    assert_eq!(lignes, attendues);
    assert_eq!(code, 0);
    // Le même regard, sur le même profil qui n'est pas natif : srt se vérifie, bwrap et lydite-reseau non. Il voit
    // donc bien ce qu'il cherche.
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", false, &accepte())), false);
    assert!(ligne_de(&lignes, "srt").is_some_and(|ligne| ligne.starts_with("missing srt: `")), "{lignes:?}");
    assert_eq!((ligne_de(&lignes, "bwrap"), ligne_de(&lignes, "lydite-reseau")), (None, None), "{lignes:?}");
    assert_eq!(code, 1);
    // Sans profil, la sortie ne change pas : ni bwrap ni lydite-reseau, même absents.
    let sans = Programmes { bwrap: absent(&dir), reseau: Err("rien".into()), ..tout_present(&dir) };
    let (lignes, code) = verifier(&sans, &dossier_court(), None, false);
    assert_eq!((lignes, code), (lignes_pretes(), 0));
    effacer(&dir);
}

#[test]
fn un_profil_natif_dit_pour_chaque_manque_quoi_faire() {
    let dir = racine("doctor-natif-manques");
    let installer = "install bubblewrap (Linux only; on Debian and Ubuntu, `apt install bubblewrap`) or name its path \
                     in LYDITE_BWRAP.";
    let construire = "build it (`cargo build --release --manifest-path reseau/Cargo.toml`) and put it next to the \
                      `lydite` executable, or name its path in LYDITE_RESEAU.";
    // `bwrap` ne part pas, `lydite-reseau` n'est pas là où on le cherche.
    let absent_reseau = dir.join("lydite-reseau-absent");
    let programmes = Programmes { bwrap: absent(&dir), reseau: Ok(absent_reseau.clone()), ..tout_present(&dir) };
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &accepte())), false);
    let bwrap = ligne_de(&lignes, "bwrap").unwrap();
    let commande = format!("{} --version", absent(&dir).to_string_lossy());
    assert!(bwrap.starts_with(&format!("missing bwrap: `{commande}` could not start: ")), "{bwrap}");
    assert!(bwrap.ends_with(&format!(" Fix: {installer}")), "{bwrap}");
    let reseau = format!("missing lydite-reseau: {} is not a file. Fix: {construire}", absent_reseau.display());
    assert_eq!(ligne_de(&lignes, "lydite-reseau"), Some(&reseau));
    assert_eq!(lignes.last().map(String::as_str), Some("not ready: 2 missing"));
    assert_eq!(code, 1);
    // Un dossier n'est pas `lydite-reseau`.
    let programmes = Programmes { reseau: Ok(dir.clone()), ..tout_present(&dir) };
    let (lignes, _) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &accepte())), false);
    let attendue = format!("missing lydite-reseau: {} is not a file. Fix: {construire}", dir.display());
    assert_eq!(ligne_de(&lignes, "lydite-reseau"), Some(&attendue));
    // Aucun chemin à chercher : ni `LYDITE_RESEAU`, ni dossier de l'exécutable.
    let programmes = Programmes { reseau: Err("inconnu".into()), ..tout_present(&dir) };
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &accepte())), false);
    let attendue = format!(
        "missing lydite-reseau: there is no path to it: LYDITE_RESEAU is not set, and the folder of the Lydite \
         executable is unknown. Fix: {construire}"
    );
    assert_eq!(ligne_de(&lignes, "lydite-reseau"), Some(&attendue));
    assert_eq!((lignes.last().map(String::as_str), code), (Some("not ready: 1 missing"), 1));
    effacer(&dir);
}

#[test]
fn un_profil_que_le_mur_natif_a_refuse_se_lit_natif_et_ne_demande_pas_srt() {
    let dir = racine("doctor-natif-refuse");
    // srt manque : un profil natif refusé n'en a pas besoin, mais `bwrap` et `lydite-reseau` se disent.
    let programmes = Programmes { srt: absent(&dir), bwrap: absent(&dir), ..tout_present(&dir) };
    let natif = Refus::new(
        Code::MurRefuse,
        "the native wall (`mur = natif`) needs bubblewrap (LYDITE_BWRAP), and `bwrap` cannot start: x",
        "Install bubblewrap.",
    );
    let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &Err(natif.clone()))), false);
    assert!(ligne_de(&lignes, "bwrap").is_some_and(|ligne| ligne.starts_with("missing bwrap: `")), "{lignes:?}");
    assert!(ligne_de(&lignes, "srt").is_some_and(|ligne| ligne.starts_with("skipped srt: ")), "{lignes:?}");
    assert!(lignes.contains(&"missing profile: p, refusal 6028".to_string()), "{lignes:?}");
    assert_eq!(lignes.last().map(String::as_str), Some("not ready: 2 missing"));
    assert_eq!(code, 1);
    // Un profil qui dit `mur = exige` est vérifié par srt, quel que soit son refus, même 6028 au texte du mur natif.
    let de_srt = Refus::new(Code::MurRefuse, "le mur est exigé, et srt ne joue pas `true` : x", "Installe srt.");
    for refus in [de_srt, natif] {
        let (lignes, _) = verifier(&programmes, &dossier_court(), Some(nomme("p", false, &Err(refus))), false);
        assert!(ligne_de(&lignes, "srt").is_some_and(|ligne| ligne.starts_with("missing srt: `")), "{lignes:?}");
        assert_eq!((ligne_de(&lignes, "bwrap"), ligne_de(&lignes, "lydite-reseau")), (None, None), "{lignes:?}");
    }
    effacer(&dir);
}

#[test]
fn un_profil_natif_pas_encore_accepte_ne_demande_pas_srt() {
    let dir = racine("doctor-natif-non-accepte");
    let programmes = Programmes { srt: absent(&dir), ..tout_present(&dir) };
    // Les refus d'avant le mur (6011 : pas d'empreinte, 6012 : changé) et un refus d'une autre cause : le profil dit
    // `mur = natif`, srt n'est ni vérifié ni conseillé, bwrap et lydite-reseau le sont.
    let refus = [
        Refus::new(Code::ProfilNonAccepte, "le profil p n'a pas d'empreinte acceptée", "Accepte-le."),
        Refus::new(Code::ProfilChange, "le profil p a changé", "Réaccepte-le."),
        Refus::new(Code::ProfilNonAccepte, "autre chose", "Autre."),
    ];
    for refus in refus {
        let numero = refus.code.numero();
        let (lignes, code) = verifier(&programmes, &dossier_court(), Some(nomme("p", true, &Err(refus))), false);
        assert!(ligne_de(&lignes, "srt").is_some_and(|ligne| ligne.starts_with("skipped srt: ")), "{lignes:?}");
        assert!(ligne_de(&lignes, "bwrap").is_some_and(|ligne| ligne.starts_with("ok bwrap: ")), "{lignes:?}");
        assert!(ligne_de(&lignes, "lydite-reseau").is_some_and(|ligne| ligne.starts_with("ok ")), "{lignes:?}");
        assert!(lignes.contains(&format!("missing profile: p, refusal {numero}")), "{lignes:?}");
        assert_eq!((lignes.last().map(String::as_str), code), (Some("not ready: 1 missing"), 1));
        assert!(!lignes.iter().any(|ligne| ligne.contains("missing srt")), "{lignes:?}");
    }
    effacer(&dir);
}

#[test]
fn doctor_lit_le_mur_dans_le_fichier_du_profil_qu_il_soit_accepte_ou_non() {
    let dir = racine("doctor-lit-natif");
    let dossier = Ok(dir.join("kraken"));
    let profils = dir.join("kraken").join("profils");
    fs::create_dir_all(&profils).unwrap();
    // Aucun fichier `.empreinte` : aucun de ces profils n'est accepté.
    fs::write(profils.join("natif.profil"), "sorte = claude\nmur = natif\n").unwrap();
    fs::write(profils.join("exige.profil"), "sorte = claude\nmur = exige\n").unwrap();
    fs::write(profils.join("demande.profil"), "sorte = claude\nmur = demande\n").unwrap();
    assert!(lire_natif("natif", &dossier));
    assert!(!lire_natif("exige", &dossier));
    assert!(!lire_natif("demande", &dossier));
    // Un profil qui n'existe pas, un dossier qui ne se donne pas : rien ne se lit, ce n'est pas un mur natif.
    assert!(!lire_natif("absent", &dossier));
    assert!(!lire_natif("natif", &Err(Refus::new(Code::ProfilNonAccepte, "x", "y"))));
    effacer(&dir);
}

#[test]
fn rien_de_ce_que_rendent_les_programmes_ne_s_affiche_hors_de_la_premiere_ligne_de_version() {
    let dir = racine("doctor-secret");
    // `claude auth status` rend 0 en écrivant un courriel, `gh auth status` un nom, `git --version` deux lignes.
    let (lignes, code) = verifier(&tout_present(&dir), &dossier_court(), None, false);
    assert_eq!(code, 0);
    assert_eq!(lignes[3], "ok claude login: signed in");
    assert_eq!(lignes[1], "ok gh: logged in");
    assert_eq!(lignes[0], "ok git: git version 9.9");
    let tout = lignes.join("\n");
    for secret in ["someone@example.com", "compte-de-gh", "seconde ligne"] {
        assert!(!tout.contains(secret), "{secret} s'affiche : {tout}");
    }
    // Le même courriel est bien écrit par le faux : sans lui, le test ne prouverait rien.
    let sortie = Command::new(faux_claude(&dir, 0)).args(["auth", "status"]).output().unwrap();
    assert!(String::from_utf8_lossy(&sortie.stdout).contains("someone@example.com"));
    effacer(&dir);
}

#[test]
fn un_programme_qui_ne_repond_pas_est_un_manque_et_ne_bloque_pas() {
    let dir = racine("doctor-delai");
    let lent = faux(&dir, "lent", "sleep 30", "ping -n 30 127.0.0.1 >nul");
    // Le délai court ne vaut que pour le programme lent : les autres gardent `DELAI`, même sous charge.
    let programmes = Programmes { git: lent.clone(), delai_git: Duration::from_secs(5), ..tout_present(&dir) };
    let (lignes, code) = verifier(&programmes, &dossier_court(), None, false);
    let commande = format!("{} --version", lent.to_string_lossy());
    let manque = format!("missing git: `{commande}` did not answer in 5 s. Fix: run `{commande}` yourself to see why.");
    assert_eq!(lignes, avec_un_manque(0, &manque));
    assert_eq!(code, 1);
    effacer(&dir);
}

#[test]
fn la_ligne_de_commande_lit_doctor_avec_ou_sans_profil() {
    let args = |mots: &[&str]| mots.iter().map(OsString::from).collect::<Vec<_>>();
    assert_eq!(options(&args(&["doctor"])), Ok(Commande::Doctor(None)));
    assert_eq!(options(&args(&["doctor", "x"])), Ok(Commande::Doctor(Some("x".into()))));
    for mal_formee in [&["doctor", "x", "y"][..], &["doctor", "--x"], &["doctor", "-"]] {
        let refus = options(&args(mal_formee)).unwrap_err();
        assert_eq!(refus.code, Code::CommandeMalFormee, "{mal_formee:?} : {refus}");
        assert!(refus.preuve.contains("doctor"), "{refus}");
        assert!(refus.correction.contains(" or lydite doctor [<profile>] or lydite status"), "{refus}");
    }
}
