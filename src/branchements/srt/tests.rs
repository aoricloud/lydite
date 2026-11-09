//! Les tests du rejeu de la preuve, par la coquille ou sous srt, et de l'épreuve du mur.
use super::*;
use crate::coeur::json::{self, Json};
use crate::essais::mains::{DELAI, bac_jetable, dossier, profil_borne};
use crate::essais::{ARBRE, LENTE, faux_srt, marque, meurt};
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;

#[test]
fn une_preuve_vide_ou_blanche_est_rouge() {
    let dir = dossier("blanche");
    let profil = Profil::faux(vec!["x".into()], DELAI);
    // `cmd /C ""` comme `sh -c ""` sortent en 0 : rejouée, une commande blanche prouverait toute main.
    for commande in ["", " ", "\t \r\n"] {
        let journal = dir.join("journal.tmp");
        let (verif, rejeu) = rejouer(&profil, commande, &dir, &journal);
        assert_eq!(verif, None, "{commande:?} ne tourne pas");
        let erreur = rejeu.unwrap_err();
        assert!(erreur.contains("blanche : rien ne se rejoue, rien n'est prouvé"), "{erreur}");
        assert!(!journal.exists(), "{commande:?} n'a pas de journal");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_journal_qui_ne_se_cree_pas_est_nomme() {
    let dir = dossier("journal-nomme");
    let fichier = dir.join("fichier");
    fs::write(&fichier, "x").unwrap();
    // Un journal posé sous un fichier ne se crée pas : l'erreur le nomme.
    let journal = fichier.join("journal.tmp");
    let profil = Profil::faux(vec!["x".into()], DELAI);
    let erreur = rejouer(&profil, "exit 0", &dir, &journal).1.unwrap_err();
    let nomme = format!("vérification « exit 0 » impossible : journal {} : ", journal.display());
    assert!(erreur.starts_with(&nomme), "{erreur}");
    // Un journal qui ne se lit pas est nommé aussi.
    let absent = dir.join("absent.tmp");
    let lu = lire_journal(&absent);
    assert!(lu.starts_with(&format!("journal illisible : {} : ", absent.display())), "{lu}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_preuve_trop_lente_est_tuee_avec_ses_petits_enfants() {
    let dir = dossier("preuve-lente");
    let bac = dir.join("bac");
    fs::create_dir(&bac).unwrap();
    let profil = Profil::faux(vec!["x".into()], ARBRE);
    let (verif, rejeu) = rejouer(&profil, LENTE, &bac, &dir.join("journal.tmp"));
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("temps dépassé") && verif.is_some(), "elle a tourné, puis dépassé : {erreur}");
    let pid = marque(&bac, Duration::ZERO);
    assert!(meurt(pid), "le petit-enfant {pid} survit à la preuve tuée");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_journal_illisible_est_dit() {
    let dir = dossier("journal");
    // Un dossier à la place du journal, ou un journal effacé par la vérification elle-même : rien ne se lit.
    let dossier_journal = dir.join("dossier.tmp");
    fs::create_dir(&dossier_journal).unwrap();
    for journal in [dossier_journal, dir.join("absent.tmp")] {
        let sortie = lire_journal(&journal);
        assert!(sortie.starts_with("journal illisible : ") && sortie.len() > 20, "{}", journal.display());
    }
    // Un journal qui se lit rend sa sortie, même vide, et s'efface.
    for ecrit in ["test result: ok\n", ""] {
        let journal = dir.join("journal.tmp");
        fs::write(&journal, ecrit).unwrap();
        assert_eq!(lire_journal(&journal), ecrit);
        assert!(!journal.exists(), "le journal lu s'efface");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_verification_qui_ne_part_pas_n_a_pas_de_verif() {
    let dir = dossier("depart");
    let profil = Profil::faux(vec!["x".into()], Duration::from_secs(30));
    let fichier = dir.join("fichier");
    fs::write(&fichier, "x").unwrap();
    // Un journal posé sous un fichier ne se crée pas ; dans un bac absent, la coquille ne part pas.
    for (bac, journal) in [(dir.clone(), fichier.join("journal.tmp")), (dir.join("absent"), dir.join("journal.tmp"))] {
        let (verif, rejeu) = rejouer(&profil, "exit 0", &bac, &journal);
        let erreur = rejeu.unwrap_err();
        assert_eq!(verif, None, "elle n'a pas tourné : {erreur}");
        assert!(erreur.contains("vérification « exit 0 » impossible : "), "{erreur}");
        assert!(!journal.exists(), "aucun journal ne reste : {}", journal.display());
    }
    // Une vérification muette a tourné : sa sortie est vide, et son rouge est dit.
    let journal = dir.join("muette.tmp");
    let (verif, rejeu) = rejouer(&profil, "exit 3", &dir, &journal);
    assert_eq!(verif.as_deref(), Some(""));
    let erreur = rejeu.unwrap_err();
    assert!(erreur.starts_with("vérification « exit 3 » rouge ("), "{erreur}");
    assert!(!journal.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn la_preuve_rejouee_garde_le_dossier_temporaire_de_kraken() {
    // La main a son dossier temporaire ; la preuve que Kraken rejoue garde celui de Kraken, et n'en crée aucun.
    let (dir, bac) = bac_jetable("temp-preuve");
    let profil = Profil::faux(vec!["x".into()], DELAI);
    let commande = if cfg!(windows) { "echo %TEMP%> temp.txt" } else { r#"printf '%s' "$TMPDIR" > temp.txt"# };
    rejouer(&profil, commande, &bac, &dir.join("journal.tmp")).1.unwrap();
    let vu = fs::read_to_string(bac.join("temp.txt")).unwrap();
    let temporaire = env::var(if cfg!(windows) { "TEMP" } else { "TMPDIR" }).unwrap_or_default();
    assert_eq!(vu.trim_end(), temporaire);
    assert!(!temporaire_de(&bac).unwrap().exists(), "la preuve ne crée pas de dossier temporaire");
    let _ = fs::remove_dir_all(&dir);
}

/// Ce que le faux srt de `dir` a noté sous ce nom, rogné.
fn note_de_srt(dir: &Path, nom: &str) -> String {
    fs::read_to_string(dir.join("srt").join(nom)).unwrap().trim().to_string()
}

#[test]
fn sous_un_mur_exige_la_preuve_se_rejoue_par_srt() {
    let (dir, bac) = bac_jetable("srt");
    let kraken = dir.join("lydite");
    fs::create_dir(&kraken).unwrap();
    let mut profil = profil_borne("sorte = claude\nmur = exige\nreseau = exemple.test\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    let journal = dir.join("journal.tmp");
    // Le code de srt est celui de la preuve : vert sur 0, rouge sinon.
    let (verif, rejeu) = rejouer(&profil, "exit 0", &bac, &journal);
    assert_eq!((verif.as_deref(), rejeu), (Some(""), Ok(())));
    let erreur = rejouer(&profil, "exit 3", &bac, &journal).1.unwrap_err();
    assert!(erreur.starts_with("vérification « exit 3 » rouge ("), "{erreur}");
    // L'appel est `srt --settings <fichier> sh -c <preuve>`, le fichier neuf dans le dossier de Kraken, effacé après.
    assert_eq!(note_de_srt(&dir, "forme.txt"), "--settings sh -c");
    let fichier = PathBuf::from(note_de_srt(&dir, "fichier.txt"));
    assert_eq!(fichier.parent(), Some(kraken.as_path()));
    assert_eq!(fs::read_dir(&kraken).unwrap().count(), 0, "le fichier de réglages s'efface");
    // Il porte toutes ses clés, et ouvre en écriture le dossier temporaire du rejeu, créé à côté du bac.
    let temporaire = temporaire_de(&bac).unwrap();
    assert!(temporaire.is_dir() && temporaire.parent() == Some(dir.as_path()));
    let Ok(Json::Objet(reglages)) = json::document(&note_de_srt(&dir, "reglages.json")) else { panic!() };
    let cles = |json: Option<&Json>| match json {
        Some(Json::Objet(objet)) => objet.keys().cloned().collect::<Vec<_>>(),
        autre => panic!("{autre:?}"),
    };
    assert_eq!(cles(reglages.get("filesystem")), ["allowRead", "allowWrite", "denyRead", "denyWrite"]);
    assert_eq!(cles(reglages.get("network")), ["allowedDomains", "deniedDomains"]);
    let Some(Json::Objet(fichiers)) = reglages.get("filesystem") else { panic!() };
    let ecrits = [&bac, &temporaire].map(|chemin| Json::Texte(chemin.to_string_lossy().into_owned()));
    assert_eq!(fichiers.get("allowWrite"), Some(&Json::Liste(ecrits.into())));
    // srt garde le TMPDIR de Kraken, CLAUDE_CODE_TMPDIR vaut le dossier temporaire du bac, et les variables
    // retirées de la preuve le restent.
    let commande = if cfg!(windows) { "set > env.txt" } else { "env > env.txt" };
    rejouer(&profil, commande, &bac, &journal).1.unwrap();
    let vu = String::from_utf8_lossy(&fs::read(bac.join("env.txt")).unwrap()).into_owned();
    let vues: BTreeMap<String, String> = vu
        .lines()
        .filter_map(|ligne| ligne.split_once('='))
        .map(|(nom, valeur)| (nom.to_ascii_uppercase(), valeur.trim_end().to_string()))
        .collect();
    let attendu = temporaire.to_string_lossy().into_owned();
    assert_eq!(vues.get("CLAUDE_CODE_TMPDIR"), Some(&attendu));
    assert_eq!(vues.get("TMPDIR"), env::var("TMPDIR").ok().as_ref());
    for (nom, _) in env::vars_os() {
        let nom = nom.to_string_lossy().to_ascii_uppercase();
        if retire_de_la_preuve(&nom) && nom != "CLAUDE_CODE_TMPDIR" {
            assert!(!vues.contains_key(&nom), "{nom} part de la preuve rejouée par srt");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_mur_exige_que_srt_ne_tient_pas_est_refuse_et_rien_ne_reste() {
    let dir = env::temp_dir().join(format!("k-{}", lot()));
    fs::create_dir_all(&dir).unwrap();
    let kraken = dir.join("lydite");
    // Sous un mur demandé, rien ne se joue ni ne se crée.
    assert_eq!(eprouver_le_mur(&profil_borne("sorte = claude\n", &dir), &kraken), Ok(()));
    assert!(!kraken.exists());
    // Un srt qui joue `true` sous des réglages de rejeu : le mur est tenu, et son dossier se retire avec son `.tmp`.
    let mut profil = profil_borne("sorte = claude\nmur = exige\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    assert_eq!(eprouver_le_mur(&profil, &kraken), Ok(()));
    assert_eq!(note_de_srt(&dir, "forme.txt"), "--settings sh -c");
    assert_eq!(fs::read_dir(kraken.join("bacs")).unwrap().count(), 0);
    // Un srt qui rend 1, ou qui ne se lance pas : le refus 6028, et rien ne reste.
    for srt in [faux_srt(&dir, false), OsString::from("lydite-srt-introuvable")] {
        profil.avec_srt(srt);
        let refus = eprouver_le_mur(&profil, &kraken).unwrap_err();
        assert_eq!(refus.code, Code::MurRefuse, "{refus}");
        assert!(refus.preuve.starts_with("le mur est exigé, et srt ne joue pas `true` : "), "{refus}");
        assert_eq!(fs::read_dir(kraken.join("bacs")).unwrap().count(), 0);
        // Le texte du refus est celui du système qui joue le test : seul celui de Windows parle de Windows et du
        // moyen de lancer quand même.
        assert_eq!(refus.correction.contains("Sous Windows"), cfg!(windows), "{refus}");
        assert_eq!(refus.correction.contains("mur = demande"), cfg!(windows), "{refus}");
        if cfg!(windows) {
            assert!(refus.correction.contains("profils/claude.profil"), "{refus}");
            assert!(refus.correction.contains("lydite accepter"), "{refus}");
        } else {
            assert!(refus.correction.contains("LYDITE_SRT"), "{refus}");
        }
    }
    let restes: Vec<_> = fs::read_dir(&kraken).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(restes, ["bacs"], "ni réglages ni journal ne restent");
    let _ = fs::remove_dir_all(&dir);
}
