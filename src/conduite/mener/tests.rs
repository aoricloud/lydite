use super::*;
use crate::coeur::json;
use crate::coeur::profil::Demande;
use crate::essais::{depot, racine};
use std::fs;

#[test]
fn chaque_tache_d_un_plan_se_dit_assemblee_classee_ou_refusee() {
    let pris = || -> Result<Pris, String> { Ok(Pris { sortie: "fait".into(), prouve: true }) };
    assert_eq!(annonce_tache("main-v-1", "a", &pris()), "assemblée : main-v-1 (a)");
    let erreur = Err("vérification « git status » rouge (exit code: 1) :\nfatal".to_string());
    assert_eq!(
        annonce_tache("main-v-1", "a", &erreur),
        "classée : main-v-1 (a)\nvérification « git status » rouge (exit code: 1) :\nfatal"
    );
    // Un refus se dit en clair, tel que `lancer` le rend : ses trois lignes suivent la première.
    let refus =
        crate::coeur::fil::refus_en_clair(7011, Code::Sautee.regle(), "main-v-2 (b) ne part pas", "Relance-la.");
    let attendu = format!("refusée : main-v-2 (b)\n{refus}");
    assert_eq!(annonce_tache("main-v-2", "b", &Err(refus)), attendu);
    assert!(attendu.lines().count() == 4 && attendu.lines().nth(1).unwrap().starts_with("refusée (7011) : "));
    // Une erreur de main qui commence comme un refus s'affiche refusée : la limite connue du raccourci.
    let limite = Err("refusée (1) : une main qui le dit".to_string());
    assert!(annonce_tache("main-v-3", "c", &limite).starts_with("refusée : main-v-3 (c)\n"));
}

#[test]
fn preparer_rend_le_premier_refus_dans_son_ordre() {
    // Le dossier de Kraken, `CLAUDE_CONFIG_DIR` et le dossier personnel sont des leurres jetables : `preparer_dans`
    // les reçoit, et aucun test ne lit ceux de l'humain ni l'environnement. Chaque cas est le premier refus de sa
    // ligne, par son code.
    let racine = racine("preparer");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    let (config, personnel) = (racine.join("config"), racine.join("personnel"));
    let profils = kraken.join("profils");
    fs::create_dir_all(&profils).unwrap();
    let code = |nom: &str, visee: &Path, dossier: &Path| {
        let issue = preparer_dans(nom, Some(visee), dossier.to_path_buf(), Some(&config), Some(&personnel));
        issue.err().unwrap_or_else(|| panic!("{nom} dans {} devait être refusé", dossier.display())).code
    };
    // Le dépôt d'abord, le profil, inconnu ici, ensuite. Un chemin vide ne se rend pas absolu (6006) ; un dépôt
    // absent n'est pas un dossier (6002, avant tout geste git) ; un dossier hors git est refusé par git (6003).
    let vide = preparer_dans("inconnu", Some(Path::new("")), kraken.clone(), Some(&config), Some(&personnel));
    let refus = vide.err().expect("un dépôt vide");
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.preuve.contains("chemin absolu de"), "{refus}");
    let (absent, nu) = (racine.join("absent"), racine.join("nu"));
    fs::create_dir(&nu).unwrap();
    assert_eq!(code("inconnu", &absent, &kraken), Code::DepotIntrouvable);
    assert_eq!(code("inconnu", &nu, &kraken), Code::BacImpossible);
    // Puis le dossier de Kraken hors du dépôt (6015), avant le profil ; le dépôt, lui, passe avant le dossier.
    assert_eq!(code("inconnu", &depot, &depot.join("lydite")), Code::DossierSousLeDepot);
    assert_eq!(code("claude", &nu, &nu.join("lydite")), Code::BacImpossible);
    // Puis le profil : son nom (6007), son fichier (6008, 6009, 6010), ou le profil intégré, sinon inconnu (6001).
    assert_eq!(code("../x", &depot, &kraken), Code::ProfilNomRefuse);
    assert_eq!(code("inconnu", &depot, &kraken), Code::ProfilInconnu);
    let poser = |nom: &str, octets: &[u8]| fs::write(profils.join(format!("{nom}.profil")), octets).unwrap();
    poser("utf16", b"\xFF\xFEs\x00");
    assert_eq!(code("utf16", &depot, &kraken), Code::ProfilIllisible);
    poser("budget", b"sorte = claude\nbudget = 5\n");
    assert_eq!(code("budget", &depot, &kraken), Code::ProfilLigneRefusee);
    poser("vide", b"");
    assert_eq!(code("vide", &depot, &kraken), Code::ProfilIncomplet);
    // Puis l'empreinte (6011, 6012), avant ce que `borner` refuse.
    poser("veille", b"sorte = claude\nmodele = haiku\n");
    assert_eq!(code("veille", &depot, &kraken), Code::ProfilNonAccepte);
    dossier::accepter("veille", &kraken).unwrap();
    poser("veille", b"sorte = claude\nmodele = haiku\n\n");
    assert_eq!(code("veille", &depot, &kraken), Code::ProfilChange);
    dossier::accepter("veille", &kraken).unwrap();
    // Puis `borner` : un dossier qu'une règle de permission ne sait pas écrire (6023), et une lecture qui recouvre
    // le dossier de Kraken (6009). Le profil intégré n'a pas d'empreinte à garder, mais le chemin le refuse aussi.
    let rare = racine.join("k(1)");
    fs::create_dir_all(rare.join("profils")).unwrap();
    fs::write(rare.join("profils").join("veille.profil"), "sorte = claude\n").unwrap();
    assert_eq!(code("veille", &depot, &rare), Code::ProfilNonAccepte, "l'empreinte se garde avant le chemin");
    dossier::accepter("veille", &rare).unwrap();
    assert_eq!(code("veille", &depot, &rare), Code::DossierCheminRefuse);
    assert_eq!(code("claude", &depot, &rare), Code::DossierCheminRefuse);
    // Un dossier de connexion de Claude aussi : `CLAUDE_CONFIG_DIR` relatif ou à un chemin refusé, ou un dossier
    // personnel à un chemin refusé qui garde un fichier de connexion. Sans lui, `CLAUDE_CONFIG_DIR` posé, ce dossier
    // personnel n'est plus un refus.
    fs::create_dir_all(rare.join(".claude")).unwrap();
    assert_eq!(preparer_dans("veille", Some(&depot), kraken.clone(), Some(&config), Some(&rare)).map(drop), Ok(()));
    fs::write(rare.join(".claude").join(".credentials.json"), "").unwrap();
    let relatif = PathBuf::from("relatif");
    for (config, personnel) in [(&relatif, &personnel), (&rare, &personnel), (&config, &rare)] {
        let refus = preparer_dans("veille", Some(&depot), kraken.clone(), Some(config), Some(personnel)).err();
        assert_eq!(refus.map(|refus| refus.code), Some(Code::DossierCheminRefuse), "{}", config.display());
    }
    poser("lecteur", format!("sorte = claude\nlecture = {}\n", racine.display()).as_bytes());
    dossier::accepter("lecteur", &kraken).unwrap();
    assert_eq!(code("lecteur", &depot, &kraken), Code::ProfilLigneRefusee);
    // Aucun refus : le profil, le dépôt en chemin absolu et le dossier, tels quels ; le profil est borné, sa main part.
    for nom in ["claude", "veille"] {
        let pret = preparer_dans(nom, Some(&depot), kraken.clone(), Some(&config), Some(&personnel));
        let Ok((profil, visee, dossier)) = pret else { panic!("{nom} devait passer") };
        assert_eq!((visee, dossier), (depot.clone(), kraken.clone()));
        let Demande { argv, declaration } = profil.demande(&racine.join("bac"), None, false);
        let declaration = declaration.expect("une main Claude bornée");
        assert!(argv.contains(&"--settings".to_string()), "{argv:?}");
        let fermee = json::chaine(&kraken.to_string_lossy());
        assert!(declaration.reglages.contains(&format!(r#""denyRead":["~/",{fermee}]"#)), "{}", declaration.reglages);
        assert!(declaration.reglages.contains("depot/.git"), "le git commun du dépôt : {}", declaration.reglages);
    }
    // Rien n'a été créé hors du dossier jetable, et le dépôt ne porte que son `.git`.
    assert_eq!(fs::read_dir(&depot).unwrap().count(), 1);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn la_relance_de_la_ci_porte_son_drapeau_et_aucun_autre_lien() {
    let liens = liens_de_la_relance();
    assert!(liens.relance_ci);
    assert!(liens.decoupage.is_none() && liens.issue.is_none() && liens.corrige.is_none());
    assert!(liens.juge.is_none() && liens.relit.is_none() && liens.preparation.is_none());
}

#[test]
fn lancer_ne_rend_0_que_si_chaque_prise_est_constatee() {
    let pris = |id: &str, prouve| -> (String, Result<Pris, String>) {
        (id.into(), Ok(Pris { sortie: "fait".into(), prouve }))
    };
    assert!(toutes_constatees(&[pris("main-v-1", true), pris("main-v-2", true)]));
    // Une prise déclarée donne 1, comme une main classée.
    assert!(!toutes_constatees(&[pris("main-v-1", true), pris("main-v-2", false)]));
    assert!(!toutes_constatees(&[pris("main-v-1", true), ("main-v-2".into(), Err("classée".into()))]));
}

// Le guetteur ferme le fil par ses droits Unix (`0o444`) : le test n'a de sens que là.
#[cfg(unix)]
#[test]
fn une_vague_ouverte_puis_refusee_finit_par_un_sceau() {
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Instant;
    let racine = racine("sceau-refus");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    let profil = Profil::faux(crate::essais::main_de_vague(&racine), Duration::from_secs(60));
    // Un objectif lourd fait une ouverture longue à écrire : le guetteur voit le fil grandir et le ferme en écriture
    // (l'écriture en cours finit, elle a son descripteur) ; la note suivante, `vague.assemblage`, ne s'écrit pas. Il le
    // rouvre dès que le refus a retiré le worktree de la vague, avant le sceau.
    let mut plan = Plan::lire("plan 1\nobjectif : o\npreuve : exit 0\ntache a\n    Écrire a".as_bytes()).unwrap();
    plan.objectif = "o".repeat(16 << 20);
    let fil = kraken.join("fil.jsonl");
    let fini = AtomicBool::new(false);
    let lieu = Lieu {
        profil: &profil,
        depot: &depot,
        dossier: &kraken,
        gh: OsStr::new("gh-absent"),
        personnel: Some(&racine),
        ci: (Duration::from_millis(10), Duration::from_secs(60)),
    };
    let base = base_du_depot(&depot).unwrap();
    let issue = std::thread::scope(|s| {
        let guetteur = s.spawn(|| {
            let limite = Instant::now() + Duration::from_secs(60);
            let droits = |mode| fs::set_permissions(&fil, fs::Permissions::from_mode(mode));
            let worktree_la = || {
                fs::read_dir(kraken.join("bacs"))
                    .map(|dir| dir.flatten().any(|e| e.file_name().to_string_lossy().starts_with("vague-")))
                    .unwrap_or(false)
            };
            let en_cours = || !fini.load(Ordering::SeqCst) && Instant::now() < limite;
            while en_cours() {
                if fs::metadata(&fil).is_ok_and(|m| m.len() > 0) && droits(0o444).is_ok() {
                    while worktree_la() && en_cours() {}
                    let _ = droits(0o644);
                    return;
                }
            }
        });
        let issue = mener_sur(&lieu, &plan, &base, Liens::default(), None, Origine::Plan);
        fini.store(true, Ordering::SeqCst);
        guetteur.join().unwrap();
        issue
    });
    let refus = issue.expect_err("la note de l'assemblage ne s'écrit pas");
    assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
    assert!(refus.preuve.contains("note au fil"), "le refus remonte tel quel : {}", refus.preuve);
    let texte = fs::read_to_string(&fil).unwrap();
    let lignes: Vec<&str> = texte.lines().collect();
    assert_eq!(lignes.len(), 2, "l'ouverture puis le sceau");
    assert!(lignes[0].contains(r#""type":"vague.ouverte""#));
    let id = lignes[0].split(r#""vague":""#).nth(1).and_then(|reste| reste.split('"').next()).unwrap();
    assert!(lignes[1].contains(r#""type":"vague.sceau""#), "le fil finit par un sceau");
    assert!(lignes[1].contains(&format!(r#""vague":"{id}""#)), "le sceau est celui de la vague ouverte");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn chaque_copie_se_dit_prise_refusee_ou_classee() {
    let pris = |prouve| -> Result<Pris, String> { Ok(Pris { sortie: "fait".into(), prouve }) };
    assert_eq!(annonce("main-v-1", &pris(true)), "pris : main-v-1");
    assert_eq!(annonce("main-v-1", &pris(false)), "pris, déclaré : main-v-1");
    // Une copie refusée (sa main tuée à son délai, un `.claude/` touché, un fil injoignable) se dit refusée, avec son
    // refus en clair, tel que `lancer` le rend : ses trois lignes suivent la première. Elle n'est pas « classée ».
    let preuve = "main-v-2 : main : temps dépassé (1 s)";
    let refus = crate::coeur::fil::refus_en_clair(7005, Code::MainTuee.regle(), preuve, "Relève le délai de la main.");
    let attendu = format!("refusée : main-v-2\n{refus}");
    assert_eq!(annonce("main-v-2", &Err(refus)), attendu);
    assert!(attendu.lines().count() == 4 && attendu.lines().nth(1).unwrap().starts_with("refusée (7005) : "));
    assert!(!attendu.contains("classé"), "{attendu}");
    // « échec classé » reste aux classées, deux fois rouges, dont l'erreur se lit telle quelle.
    let rouge = Err("vérification « exit 3 » rouge (exit code: 3) :\noups".to_string());
    let attendu = "échec classé : main-v-3\nvérification « exit 3 » rouge (exit code: 3) :\noups";
    assert_eq!(annonce("main-v-3", &rouge), attendu);
    // Une erreur de main qui commence comme un refus s'affiche refusée : la limite connue du raccourci, la même que
    // celle d'`annonce_tache`.
    let limite = Err("refusée (1) : une main qui le dit".to_string());
    assert!(annonce("main-v-4", &limite).starts_with("refusée : main-v-4\n"));
}
