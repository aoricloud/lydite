//! Les tests du dépôt visé : ses racines, et le dossier de Kraken hors de lui.
use crate::branchements::git::*;
use crate::essais::{depot, racine};

#[test]
fn racines_refuse_un_depot_absent_puis_un_dossier_hors_git() {
    let racine = racine("profil-racines");
    // Absent, ou un fichier : le refus 6002, avant tout geste git, et rien n'est créé.
    let (absent, fichier) = (racine.join("absent"), racine.join("fichier"));
    fs::write(&fichier, "x").unwrap();
    for depot in [&absent, &fichier] {
        let refus = racines(depot).unwrap_err();
        assert_eq!(refus.code, Code::DepotIntrouvable, "{}", depot.display());
        assert!(refus.preuve.contains(&depot.display().to_string()), "{refus}");
    }
    assert!(!absent.exists());
    // Un dossier hors git : le refus 6003, qui porte la sortie de git, et le dossier reste vide.
    let nu = racine.join("nu");
    fs::create_dir(&nu).unwrap();
    let refus = racines(&nu).unwrap_err();
    assert_eq!(refus.code, Code::BacImpossible);
    assert!(refus.preuve.starts_with(&format!("git rev-parse dans {} a échoué (", nu.display())), "{refus}");
    assert!(refus.preuve.len() > format!("git rev-parse dans {} a échoué () : ", nu.display()).len() + 10, "{refus}");
    assert_eq!(fs::read_dir(&nu).unwrap().count(), 0);
    // Un dépôt, vu d'un de ses dossiers : sa tête et son git commun, en chemins absolus.
    let depot = depot(&racine);
    fs::create_dir(depot.join("sous")).unwrap();
    let (tete, commun) = racines(&depot.join("sous")).unwrap();
    assert!(tete.is_absolute() && commun.is_absolute(), "{} {}", tete.display(), commun.display());
    assert_eq!(fs::canonicalize(tete).unwrap(), fs::canonicalize(&depot).unwrap());
    assert_eq!(fs::canonicalize(commun).unwrap(), fs::canonicalize(depot.join(".git")).unwrap());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_dossier_de_kraken_sous_le_depot_est_refuse() {
    let racine = racine("profil-sous");
    let depot = depot(&racine);
    // La tête telle que git l'écrit (sous Windows, avec des barres obliques), et des dossiers écrits autrement.
    let (tete, _) = racines(&depot).unwrap();
    for dossier in [depot.join("lydite"), depot.join(".git").join("lydite"), depot.clone(), depot.join("a").join("b")] {
        let refus = hors_du_depot(&tete, &dossier).unwrap_err();
        assert_eq!(refus.code, Code::DossierSousLeDepot, "{}", dossier.display());
        assert!(refus.preuve.contains(&dossier.display().to_string()), "{refus}");
        assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
    }
    // Le même dépôt écrit autrement, d'une autre casse sous Windows ou par un lien ailleurs : la canonisation le
    // voit.
    let autrement = racine.join(if cfg!(windows) { "DEPOT" } else { "lien" });
    #[cfg(unix)]
    std::os::unix::fs::symlink(&depot, &autrement).unwrap();
    let refus = hors_du_depot(&tete, &autrement.join("lydite")).unwrap_err();
    assert_eq!(refus.code, Code::DossierSousLeDepot, "{}", autrement.display());
    // Un `..` est refusé, où qu'il mène.
    let detour = racine.join("lydite").join("..").join("ailleurs");
    let refus = hors_du_depot(&tete, &detour).unwrap_err();
    assert_eq!(refus.code, Code::DossierSousLeDepot);
    assert!(refus.preuve.ends_with("porte « .. »"), "{refus}");
    // Rien n'est créé.
    assert!(!depot.join("lydite").exists() && !depot.join("a").exists() && !racine.join("lydite").exists());
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_dossier_de_kraken_a_cote_du_depot_est_permis() {
    let racine = racine("profil-a-cote");
    let depot = depot(&racine);
    let (tete, _) = racines(&depot).unwrap();
    // À côté, sous un nom qui commence comme le dépôt, plus haut que lui, ou loin sous un dossier absent.
    for dossier in [racine.join("lydite"), racine.join("depot-lydite"), racine.clone(), racine.join("k").join("l")] {
        assert_eq!(hors_du_depot(&tete, &dossier), Ok(()), "{}", dossier.display());
    }
    assert!(!racine.join("lydite").exists() && !racine.join("k").exists(), "rien n'est créé");
    let _ = fs::remove_dir_all(&racine);
}
