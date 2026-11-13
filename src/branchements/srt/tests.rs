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

/// Une commande qui pose une ligne dans `jeux` du bac à chaque jeu, puis joue `suite` ; le second jeu sort en 0 si
/// `reussit_au_second`.
#[cfg(unix)]
fn commande_comptee(suite: &str, reussit_au_second: bool) -> String {
    let second = if reussit_au_second { r#"if [ "$(wc -l < jeux)" -ge 2 ]; then exit 0; fi; "# } else { "" };
    format!("echo j >> jeux; {second}{suite}")
}

/// Rejoue `commande` dans un bac jetable : son rejeu, le nombre de jeux qu'elle a comptés, et le dossier à effacer.
#[cfg(unix)]
fn rejeu_compte(nom: &str, commande: &str, delai: Duration) -> (Rejeu, usize, PathBuf) {
    let (dir, bac) = bac_jetable(nom);
    let profil = Profil::faux(vec!["x".into()], delai);
    let rejeu = rejouer(&profil, commande, &bac, &dir.join("journal.tmp"));
    let jeux = fs::read_to_string(bac.join("jeux")).unwrap_or_default().lines().count();
    (rejeu, jeux, dir)
}

#[cfg(unix)]
#[test]
fn une_preuve_tuee_au_premier_jeu_se_rejoue_une_fois_et_le_fil_le_dit() {
    // Tuée du signal 9 puis du code 137 (une coquille dont l'enfant est tué) : verte au second jeu.
    for (nom, suite) in [("tuee-9", "kill -9 $$"), ("tuee-137", "sh -c 'kill -9 $$'; exit $?")] {
        let ((verif, rejeu), jeux, dir) = rejeu_compte(nom, &commande_comptee(suite, true), DELAI);
        assert_eq!(rejeu, Ok(()), "{nom}");
        assert_eq!(jeux, 2, "{nom} : la commande a joué deux fois");
        let verif = verif.unwrap();
        assert!(verif.starts_with("proof killed ("), "{nom} : {verif}");
        assert!(verif.lines().next().unwrap().ends_with("), replayed once"), "{nom} : {verif}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(unix)]
#[test]
fn le_statut_du_premier_jeu_est_dans_la_ligne_et_la_sortie_du_second_la_suit() {
    // Le premier jeu écrit PREMIER-JEU puis se tue ; le second écrit SECOND-JEU et sort en 0.
    let commande =
        r#"echo j >> jeux; if [ "$(wc -l < jeux)" -lt 2 ]; then echo PREMIER-JEU; kill -9 $$; fi; echo SECOND-JEU"#;
    let ((verif, rejeu), jeux, dir) = rejeu_compte("tuee-ligne", commande, DELAI);
    assert_eq!(rejeu, Ok(()));
    assert_eq!(jeux, 2);
    let verif = verif.unwrap();
    let (premiere, reste) = verif.split_once('\n').unwrap();
    assert!(premiere.starts_with("proof killed (signal: 9"), "{premiere}");
    assert!(premiere.ends_with("), replayed once"), "{premiere}");
    assert_eq!(reste.trim_end(), "SECOND-JEU", "la fin de la sortie du second jeu seulement");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn une_preuve_tuee_deux_fois_est_rouge_jouee_deux_fois_et_pas_trois() {
    let ((verif, rejeu), jeux, dir) = rejeu_compte("tuee-deux-fois", &commande_comptee("kill -9 $$", false), DELAI);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("rouge (signal: 9"), "{erreur}");
    assert_eq!(jeux, 2, "jouée deux fois, pas trois");
    assert!(verif.unwrap().starts_with("proof killed ("), "la ligne dit le premier jeu");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn une_preuve_rouge_ou_plantee_ou_trop_lente_n_est_jouee_qu_une_fois() {
    // `exit 3` : rouge, et la ligne n'y est pas. Le signal 11 : un plantage, rouge. Le délai : « temps dépassé ».
    let cas = [
        ("rouge-3", "exit 3", Duration::from_secs(30), "rouge (exit status: 3)"),
        ("rouge-11", "kill -11 $$", Duration::from_secs(30), "rouge (signal: 11"),
        ("rouge-lente", "sleep 30", Duration::from_secs(3), "temps dépassé"),
    ];
    for (nom, suite, delai, dit) in cas {
        let ((verif, rejeu), jeux, dir) = rejeu_compte(nom, &commande_comptee(suite, false), delai);
        let erreur = rejeu.unwrap_err();
        assert!(erreur.contains(dit), "{nom} : {erreur}");
        assert_eq!(jeux, 1, "{nom} : jouée une seule fois");
        assert!(!verif.unwrap().contains("proof killed"), "{nom} : pas de ligne");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn une_commande_blanche_ou_qui_ne_part_pas_finit_autre_et_une_verte_ou_rouge_finit_de_son_statut() {
    let dir = dossier("fin-autre");
    let profil = Profil::faux(vec!["x".into()], DELAI);
    // Blanche : rien ne part, pas de journal, la fin est `Autre`.
    for commande in ["", " ", "\t \r\n"] {
        let journal = dir.join("journal.tmp");
        let ((verif, rejeu), fin) = rejouer_et_finir(&profil, commande, &dir, &journal);
        assert_eq!((verif, fin), (None, FinDuRejeu::Autre), "{commande:?}");
        assert!(rejeu.unwrap_err().contains("blanche"), "{commande:?}");
        assert!(!journal.exists(), "{commande:?} n'a pas de journal");
    }
    // Qui ne part pas : un bac absent, ou un journal posé sous un fichier.
    let fichier = dir.join("fichier");
    fs::write(&fichier, "x").unwrap();
    for (bac, journal) in [(dir.join("absent"), dir.join("journal.tmp")), (dir.clone(), fichier.join("journal.tmp"))] {
        let ((verif, rejeu), fin) = rejouer_et_finir(&profil, "exit 0", &bac, &journal);
        assert_eq!((verif, fin), (None, FinDuRejeu::Autre));
        assert!(rejeu.unwrap_err().contains("impossible"));
    }
    // Qui a fini d'elle-même : verte sur 0, rouge sinon, sous tous les systèmes.
    let journal = dir.join("journal.tmp");
    let (rejeu, fin) = rejouer_et_finir(&profil, "exit 0", &dir, &journal);
    assert_eq!((rejeu, fin), ((Some(String::new()), Ok(())), FinDuRejeu::Verte));
    let ((verif, rejeu), fin) = rejouer_et_finir(&profil, "exit 3", &dir, &journal);
    assert_eq!((verif, fin), (Some(String::new()), FinDuRejeu::Rouge));
    assert!(rejeu.unwrap_err().starts_with("vérification « exit 3 » rouge ("));
    let _ = fs::remove_dir_all(&dir);
}

/// Rejoue `commande` par `rejouer_et_finir` dans un bac jetable : son rejeu, sa fin, le nombre de jeux qu'elle a
/// comptés, et le dossier à effacer.
#[cfg(unix)]
fn fin_comptee(nom: &str, commande: &str, delai: Duration) -> (Rejeu, FinDuRejeu, usize, PathBuf) {
    let (dir, bac) = bac_jetable(nom);
    let profil = Profil::faux(vec!["x".into()], delai);
    let (rejeu, fin) = rejouer_et_finir(&profil, commande, &bac, &dir.join("journal.tmp"));
    let jeux = fs::read_to_string(bac.join("jeux")).unwrap_or_default().lines().count();
    (rejeu, fin, jeux, dir)
}

#[cfg(unix)]
#[test]
fn le_rejeu_dit_sa_fin_par_un_type_et_rejouer_rend_ce_qu_il_rendait() {
    // Plus de 5000 caractères : `queue` n'en garde que 4000, la fin ne se lit pas dans le texte coupé.
    let longue = "i=0; while [ $i -lt 600 ]; do echo 0123456789; i=$((i+1)); done; exit 3";
    let lent = Duration::from_secs(3);
    let cas = [
        ("fin-verte", "exit 0", DELAI, FinDuRejeu::Verte, 1),
        ("fin-rouge", "exit 3", DELAI, FinDuRejeu::Rouge, 1),
        ("fin-longue", longue, DELAI, FinDuRejeu::Rouge, 1),
        ("fin-delai", "sleep 30", lent, FinDuRejeu::Delai, 1),
    ];
    for (nom, suite, delai, attendue, jeux_attendus) in cas {
        let commande = commande_comptee(suite, false);
        let (rejeu, fin, jeux, dir) = fin_comptee(nom, &commande, delai);
        assert_eq!(fin, attendue, "{nom} : {rejeu:?}");
        assert_eq!(jeux, jeux_attendus, "{nom}");
        // `rejouer` rend le même rejeu que sa voisine, au texte près.
        let (le_meme, _, autre_dir) = rejeu_compte(&format!("{nom}-rejouer"), &commande, delai);
        assert_eq!(le_meme, rejeu, "{nom}");
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&autre_dir);
    }
    // Le rouge long : son texte est coupé aux 4000 derniers caractères, et sa fin reste `Rouge`.
    let (rejeu, fin, _, dir) = fin_comptee("fin-longue-texte", longue, DELAI);
    let erreur = rejeu.1.unwrap_err();
    assert_eq!(erreur.chars().count(), 4000, "le texte est coupé");
    assert!(!erreur.starts_with("vérification"), "l'en-tête est coupé : {}", &erreur[..40]);
    assert_eq!(fin, FinDuRejeu::Rouge);
    let _ = fs::remove_dir_all(&dir);
    // Tuée deux fois : jouée deux fois, et la fin est celle du second jeu, `Tuee`, jamais `Rouge`.
    let (rejeu, fin, jeux, dir) = fin_comptee("fin-tuee", &commande_comptee("kill -9 $$", false), DELAI);
    assert!(matches!(fin, FinDuRejeu::Tuee(statut) if est_tuee(&statut)), "{fin:?}");
    assert_eq!(jeux, 2, "jouée deux fois");
    assert!(rejeu.1.unwrap_err().contains("rouge (signal: 9"));
    let _ = fs::remove_dir_all(&dir);
    // Tuée puis verte au second jeu : `Verte`, la fin du second jeu.
    let (rejeu, fin, jeux, dir) = fin_comptee("fin-tuee-verte", &commande_comptee("kill -9 $$", true), DELAI);
    assert_eq!((rejeu.1, fin, jeux), (Ok(()), FinDuRejeu::Verte, 2));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_code_137_n_est_dit_tue_que_sous_unix() {
    let statut = coquille("exit 137").status().unwrap();
    assert_eq!(est_tuee(&statut), cfg!(unix));
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
        assert!(refus.preuve.starts_with("the wall is required, and srt does not play `true`: "), "{refus}");
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

/// Un bac jetable où le mur natif a de quoi se monter : le dossier personnel, le dossier de Kraken, le dossier de
/// connexion de Claude et le git commun, que `profil_borne` ferme ou lit, existent sous le dossier rendu.
fn decor_natif(nom: &str) -> (PathBuf, PathBuf) {
    let (dir, bac) = bac_jetable(nom);
    for sous in ["personnel", "lydite", "config", "depot/.git"] {
        fs::create_dir_all(dir.join(sous)).unwrap();
    }
    (dir, bac)
}

/// Un `bwrap` qui part (`--version`), puis rend `code` après avoir écrit `dit` sur ses erreurs : un faux mur qui ne
/// tient pas. Rend son chemin.
#[cfg(unix)]
fn bwrap_qui_echoue(dir: &Path, dit: &str, code: u8) -> OsString {
    let chemin = dir.join("bwrap-qui-echoue.sh");
    crate::essais::script(&chemin, &format!("[ \"$1\" = --version ] && exit 0\necho '{dit}' >&2\nexit {code}"));
    crate::essais::chauffer(&chemin);
    chemin.into_os_string()
}

#[cfg(unix)]
#[test]
fn sous_un_mur_natif_la_preuve_se_rejoue_par_le_mur_natif_et_srt_n_est_pas_lance() {
    let (dir, bac) = decor_natif("srt-natif");
    let natif = MurNatif { programme: crate::essais::faux_bwrap(&dir), maison: Ok(dir.join("personnel")) };
    let journal = dir.join("journal.tmp");
    let mut profil = profil_borne("sorte = claude\nmur = natif\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    let (verif, rejeu) = rejouer_avec(&profil, "echo MARQUE-NATIF", &bac, &journal, &natif);
    assert_eq!(rejeu, Ok(()));
    assert!(verif.as_deref().is_some_and(|sortie| sortie.ends_with("MARQUE-NATIF\n")), "{verif:?}");
    // Un `bwrap` extérieur et le second, qui joue la commande ; le journal est effacé.
    let appels = crate::essais::appels_du_faux_bwrap(&dir);
    assert_eq!(appels.len(), 2, "{appels:?}");
    assert_eq!(appels[1].last().map(String::as_str), Some("echo MARQUE-NATIF"), "{appels:?}");
    assert!(!journal.exists());
    assert!(!dir.join("srt").join("forme.txt").exists(), "srt n'est pas lancé sous `mur = natif`");
    // Le même regard voit srt quand le profil l'exige, et `bwrap` alors ne part pas : l'assertion qui précède a pu
    // échouer.
    let mut exige = profil_borne("sorte = claude\nmur = exige\n", &dir);
    exige.avec_srt(faux_srt(&dir, true));
    rejouer_avec(&exige, "echo MARQUE-SRT", &bac, &journal, &natif).1.unwrap();
    assert_eq!(note_de_srt(&dir, "forme.txt"), "--settings sh -c");
    assert_eq!(crate::essais::appels_du_faux_bwrap(&dir).len(), 2, "sous exige, bwrap ne part pas");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sous_un_mur_natif_une_preuve_qui_ne_se_rejoue_pas_est_une_erreur_et_jamais_une_coquille_nue() {
    let (dir, bac) = decor_natif("natif-sans-maison");
    let (journal, marque) = (dir.join("journal.tmp"), bac.join("marque.txt"));
    let natif = MurNatif { programme: "bwrap-introuvable".into(), maison: Err("pas de HOME".into()) };
    let mut profil = profil_borne("sorte = claude\nmur = natif\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    let commande = if cfg!(windows) { "echo x> marque.txt" } else { "echo x > marque.txt" };
    // Sans dossier personnel, le mur ne sait pas où fermer `~` : la preuve ne part pas, ni par la coquille ni par srt.
    let (verif, rejeu) = rejouer_avec(&profil, commande, &bac, &journal, &natif);
    assert_eq!(verif, None);
    assert_eq!(rejeu, Err(format!("vérification « {commande} » impossible : pas de HOME")));
    // Un `bwrap` qui ne part pas : l'erreur de son lancement, et la commande ne s'est pas jouée hors du mur.
    let natif = MurNatif { programme: dir.join("bwrap-absent").into(), maison: Ok(dir.join("personnel")) };
    let (verif, rejeu) = rejouer_avec(&profil, commande, &bac, &journal, &natif);
    assert_eq!(verif, None);
    assert!(rejeu.unwrap_err().starts_with(&format!("vérification « {commande} » impossible : ")));
    assert!(!marque.exists(), "la commande ne s'est pas jouée");
    assert!(!dir.join("srt").join("forme.txt").exists(), "srt n'est pas lancé non plus");
    // Le même regard voit la marque quand la commande se joue (hors mur, sous un profil sans mur).
    let sans_mur = Profil::faux(vec!["x".into()], DELAI);
    rejouer_avec(&sans_mur, commande, &bac, &journal, &natif).1.unwrap();
    assert!(marque.exists());
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn sous_un_mur_natif_l_epreuve_joue_true_dans_le_mur_natif_et_ne_laisse_rien() {
    let (dir, _) = decor_natif("epreuve-natif");
    let kraken = dir.join("lydite");
    let natif = MurNatif { programme: crate::essais::faux_bwrap(&dir), maison: Ok(dir.join("personnel")) };
    let mut profil = profil_borne("sorte = claude\nmur = natif\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    assert_eq!(eprouver_le_mur_avec(&profil, &kraken, &natif), Ok(()));
    // La sonde de `bwrap`, puis le mur extérieur et le second, qui joue `true`.
    let appels = crate::essais::appels_du_faux_bwrap(&dir);
    assert_eq!(appels.len(), 3, "{appels:?}");
    assert_eq!(appels[0], ["--version"]);
    assert_eq!(appels[2].last().map(String::as_str), Some("true"), "{appels:?}");
    assert!(!dir.join("srt").join("forme.txt").exists(), "srt n'est pas lancé sous `mur = natif`");
    // Le dossier de `bacs/` que l'épreuve a créé est là, vide, et rien d'autre : ni worktree, ni journal, ni filtre.
    let restes: Vec<_> = fs::read_dir(&kraken).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(restes, ["bacs"]);
    assert_eq!(fs::read_dir(kraken.join("bacs")).unwrap().count(), 0);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_mur_natif_dont_bwrap_ne_part_pas_est_refuse_en_6028_avant_que_rien_soit_cree() {
    let (dir, _) = decor_natif("epreuve-sans-bwrap");
    let kraken = dir.join("lydite");
    let absent = dir.join("bwrap-absent");
    let natif = MurNatif { programme: absent.clone().into(), maison: Ok(dir.join("personnel")) };
    let mut profil = profil_borne("sorte = claude\nmur = natif\n", &dir);
    profil.avec_srt(faux_srt(&dir, true));
    let refus = eprouver_le_mur_avec(&profil, &kraken, &natif).unwrap_err();
    assert_eq!(refus.code, Code::MurRefuse, "{refus}");
    let debut =
        format!("{PREUVE_DU_MUR_NATIF} needs bubblewrap (LYDITE_BWRAP), and `{}` cannot start: ", absent.display());
    assert!(refus.preuve.starts_with(&debut), "{refus}");
    assert!(refus.correction.contains("install it") && refus.correction.contains("set LYDITE_BWRAP"), "{refus}");
    assert!(refus.correction.contains("`mur = exige`") && refus.correction.contains("through srt"), "{refus}");
    // Rien n'est créé : le dossier de Kraken est vide (l'épreuve qui réussit y laisse `bacs/`, comme le voit le test
    // voisin), et srt n'est pas lancé à la place.
    assert_eq!(fs::read_dir(&kraken).unwrap().count(), 0);
    assert!(!dir.join("srt").join("forme.txt").exists(), "srt n'est pas lancé à la place");
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn un_mur_natif_que_le_systeme_ferme_ou_qui_ne_joue_pas_true_est_refuse_en_6028_et_rien_ne_reste() {
    let (dir, _) = decor_natif("epreuve-ferme");
    let kraken = dir.join("lydite");
    let profil = profil_borne("sorte = claude\nmur = natif\n", &dir);
    let eprouver = |programme: OsString| {
        let natif = MurNatif { programme, maison: Ok(dir.join("personnel")) };
        let refus = eprouver_le_mur_avec(&profil, &kraken, &natif).unwrap_err();
        assert_eq!(refus.code, Code::MurRefuse, "{refus}");
        assert!(refus.preuve.starts_with(PREUVE_DU_MUR_NATIF), "{refus}");
        assert!(refus.correction.contains("`mur = exige`"), "{refus}");
        // Le dossier de `bacs/` que l'épreuve crée reste vide : ni worktree, ni temporaire, ni filtre, ni journal.
        assert_eq!(fs::read_dir(kraken.join("bacs")).unwrap().count(), 0);
        assert_eq!(fs::read_dir(&kraken).unwrap().count(), 1, "seul `bacs/` est dans le dossier de Kraken");
        refus
    };
    // `bwrap` part et dit ce qu'il dit quand le système ferme les espaces d'utilisateur.
    let dit = "bwrap: No permissions to create new namespace, likely because the kernel does not allow non-privileged \
               user namespaces";
    let refus = eprouver(bwrap_qui_echoue(&dir, dit, 1));
    assert!(refus.preuve.contains("needs the user namespaces") && refus.preuve.contains(dit), "{refus}");
    assert!(refus.correction.contains("kernel.unprivileged_userns_clone=1"), "{refus}");
    // Un autre échec : l'erreur du rejeu, telle quelle, sans accuser les espaces d'utilisateur.
    let refus = eprouver(bwrap_qui_echoue(&dir, "boum", 1));
    assert!(refus.preuve.starts_with(&format!("{PREUVE_DU_MUR_NATIF} does not play `true`: ")), "{refus}");
    assert!(refus.preuve.contains("rouge (") && refus.preuve.contains("boum"), "{refus}");
    assert!(!refus.preuve.contains("namespaces") && !refus.correction.contains("userns"), "{refus}");
    let _ = fs::remove_dir_all(&dir);
}

/// Le refus 6028 que rend le mur natif pour l'erreur de rejeu `erreur`, diagnostiquée comme `eprouver_le_mur` le fait.
fn refus_pour(erreur: &str) -> Refus {
    refus_du_mur_natif(OsStr::new("bwrap"), &Cause::Rejeu(natif::manque(erreur), erreur.to_string()))
}

const CONSTRUIRE: &str = "Build lydite-reseau";

#[test]
fn un_lydite_reseau_introuvable_se_corrige_en_le_construisant() {
    let erreur = "vérification « true » impossible : the profile allows domains, so the native wall needs \
                  lydite-reseau, and /srv/outils/lydite-reseau cannot be read (No such file or directory (os error \
                  2)): set LYDITE_RESEAU to the path of a built lydite-reseau, or put lydite-reseau next to the \
                  Lydite executable";
    let refus = refus_pour(erreur);
    assert_eq!(refus.code, Code::MurRefuse, "{refus}");
    assert!(refus.preuve.starts_with(&format!("{PREUVE_DU_MUR_NATIF} needs lydite-reseau")), "{refus}");
    assert!(refus.correction.starts_with(CONSTRUIRE), "{refus}");
}

#[test]
fn un_socket_trop_long_se_corrige_par_un_dossier_d_etat_plus_court() {
    let erreur = "vérification « true » impossible : the network filter socket path /srv/k/bacs/x.net/s is 130 bytes \
                  long, and a Unix socket path holds 107 at most: use a shorter sandbox folder";
    let refus = refus_pour(erreur);
    assert_eq!(refus.code, Code::MurRefuse, "{refus}");
    assert!(refus.preuve.starts_with(PREUVE_DU_MUR_NATIF) && refus.preuve.contains("130 bytes long"), "{refus}");
    assert!(refus.correction.starts_with("Use a shorter Lydite state folder"), "{refus}");
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
    assert!(!refus.correction.contains(CONSTRUIRE), "{refus}");
}

#[test]
fn un_lydite_reseau_present_qui_meurt_se_corrige_en_lisant_ce_qu_il_a_dit() {
    let erreur = "vérification « true » impossible : the network filter /srv/outils/lydite-reseau exited before it \
                  said ready (exit status: 3): boom";
    let refus = refus_pour(erreur);
    assert_eq!(refus.code, Code::MurRefuse, "{refus}");
    assert!(refus.preuve.contains("exited before it said ready (exit status: 3): boom"), "{refus}");
    assert!(refus.correction.contains("read what it said"), "{refus}");
    assert!(!refus.correction.contains(CONSTRUIRE), "{refus}");
}

#[test]
fn la_sortie_de_la_commande_ne_choisit_pas_la_correction_du_filtre() {
    // `bwrap` part, mais la sortie de la commande porte les mots du filtre : le mur n'a rien dit de lui-même.
    let erreur = "vérification « true » rouge (exit status: 1) :\nlydite-reseau: not found; the network filter socket \
                  path is too long";
    let refus = refus_pour(erreur);
    assert!(refus.preuve.starts_with(&format!("{PREUVE_DU_MUR_NATIF} does not play `true`: ")), "{refus}");
    assert!(!refus.correction.contains(CONSTRUIRE) && !refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
}

#[test]
fn toute_correction_du_mur_natif_dit_lydite_accept_et_jamais_accepter() {
    let erreurs = [
        "vérification « true » impossible : the profile allows domains, so the native wall needs lydite-reseau, and x",
        "vérification « true » impossible : the network filter socket path /s is 130 bytes long",
        "vérification « true » impossible : the network filter /srv/l exited before it said ready",
        "vérification « true » rouge (exit status: 1) :\nbwrap: setting up uid map: Permission denied",
        "vérification « true » rouge (exit status: 1) :\nboum",
    ];
    for erreur in erreurs {
        let refus = refus_pour(erreur);
        assert!(refus.correction.contains("`lydite accept <profile>`"), "{refus}");
        assert!(!refus.correction.contains("accepter"), "{refus}");
    }
    let bwrap = refus_du_mur_natif(OsStr::new("bwrap"), &Cause::Bwrap("x".into()));
    assert!(
        bwrap.correction.contains("`lydite accept <profile>`") && !bwrap.correction.contains("accepter"),
        "{bwrap}"
    );
}
