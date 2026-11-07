//! Les tests du dossier de Kraken.
use super::*;
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::fil::{instant, maintenant};
use crate::essais::{kraken, racine};

#[test]
fn kraken_dossier_l_emporte_tel_quel() {
    for windows in [true, false] {
        let choisi = dossier_selon(valeur("k"), valeur("u"), valeur("x"), valeur("h"), windows).unwrap();
        assert_eq!(choisi, absolu("k"), "le dossier lui-même, sans `lydite` ajouté");
    }
    // Posé sans être absolu (relatif, vide) : le refus 6004 qui nomme la variable, pas un repli en silence.
    for faux in ["relatif", ""] {
        let refus = dossier_selon(Some(faux.into()), valeur("u"), valeur("x"), valeur("h"), false).unwrap_err();
        assert_eq!(refus.code, Code::DossierRelatif);
        assert!(refus.preuve.contains("LYDITE_DOSSIER"), "{refus}");
    }
}

#[test]
fn sous_windows_userprofile_et_rien_d_autre() {
    let choisi = dossier_selon(None, valeur("u"), valeur("x"), valeur("h"), true).unwrap();
    assert_eq!(choisi, absolu("u").join(".lydite"));
    let refus = dossier_selon(None, None, valeur("x"), valeur("h"), true).unwrap_err();
    assert_eq!(refus.code, Code::DossierIntrouvable);
    assert!(refus.preuve.contains("USERPROFILE"), "{refus}");
}

#[test]
fn ailleurs_xdg_state_home_puis_home() {
    let xdg = dossier_selon(None, valeur("u"), valeur("x"), valeur("h"), false).unwrap();
    assert_eq!(xdg, absolu("x").join("lydite"), "USERPROFILE ne compte pas hors de Windows");
    let home = dossier_selon(None, valeur("u"), None, valeur("h"), false).unwrap();
    assert_eq!(home, absolu("h").join(".local").join("state").join("lydite"));
    // Vide ou relative, XDG_STATE_HOME est ignorée (règle XDG) ; sans HOME, aucun dossier n'est possible.
    for faux in ["relatif", ""] {
        assert_eq!(dossier_selon(None, None, Some(faux.into()), valeur("h"), false).unwrap(), home);
    }
    let refus = dossier_selon(None, valeur("u"), None, None, false).unwrap_err();
    assert_eq!(refus.code, Code::DossierIntrouvable);
    assert!(refus.preuve.contains("XDG_STATE_HOME") && refus.preuve.contains("HOME"), "{refus}");
    assert!(refus.correction.contains("LYDITE_DOSSIER"), "{refus}");
}

#[test]
fn dossier_lit_les_vraies_variables() {
    // Lecture seule, rien n'est posé : absolu, ou un refus clair quand l'environnement ne dit rien de bon.
    match dossier() {
        Ok(dossier) => assert!(dossier.is_absolute(), "{}", dossier.display()),
        Err(refus) => assert!(refus.to_string().contains("LYDITE_DOSSIER"), "{refus}"),
    }
}

#[test]
fn l_empreinte_garde_le_profil() {
    let dossier = kraken("empreinte");
    let (chemin, garde) =
        (dossier.join("profils").join("veille.profil"), dossier.join("profils").join("veille.empreinte"));
    fs::write(&chemin, "sorte = claude\nmodele = haiku\n").unwrap();
    // Absente : le profil n'est pas lancé (6011), et la correction dit comment l'accepter.
    let refus = charger_dans("veille", &dossier).unwrap().garder().unwrap_err();
    assert_eq!(refus.code, Code::ProfilNonAccepte);
    assert!(refus.preuve.contains(&garde.display().to_string()), "{refus}");
    assert!(refus.correction.contains("lydite accepter veille"), "{refus}");
    // Acceptée : le texte et l'empreinte rendus, l'empreinte écrite en 64 hexadécimaux minuscules, et le profil
    // passe.
    let (texte, empreinte) = accepter("veille", &dossier).unwrap();
    assert_eq!(texte, "sorte = claude\nmodele = haiku\n");
    assert_eq!(empreinte, hex(&sha256(texte.as_bytes())));
    assert_eq!(fs::read_to_string(&garde).unwrap(), empreinte);
    assert_eq!(charger_dans("veille", &dossier).unwrap().garder(), Ok(()));
    // Changée, d'un retour de ligne même : le refus 6012, avec l'empreinte acceptée et celle lue.
    fs::write(&chemin, "sorte = claude\nmodele = haiku\n\n").unwrap();
    let refus = charger_dans("veille", &dossier).unwrap().garder().unwrap_err();
    assert_eq!(refus.code, Code::ProfilChange);
    let lue = hex(&sha256(b"sorte = claude\nmodele = haiku\n\n"));
    assert!(refus.preuve.contains(&format!("« {empreinte} »")) && refus.preuve.contains(&format!("« {lue} »")));
    assert!(refus.correction.contains("lydite accepter veille"), "{refus}");
    // Réacceptée, elle passe de nouveau ; une empreinte écrite à la main, finie par un retour, aussi.
    assert_eq!(accepter("veille", &dossier).unwrap().1, lue);
    assert_eq!(charger_dans("veille", &dossier).unwrap().garder(), Ok(()));
    fs::write(&garde, format!("{lue}\n")).unwrap();
    assert_eq!(charger_dans("veille", &dossier).unwrap().garder(), Ok(()));
    // L'intégré n'en demande pas, et ne s'accepte pas (6013) ; un nom sans fichier ni intégré est inconnu (6001).
    assert_eq!(charger_dans("claude", &dossier).unwrap().garder(), Ok(()));
    let refus = accepter("claude", &dossier).unwrap_err();
    assert_eq!(refus.code, Code::ProfilSansFichier);
    assert!(refus.preuve.contains("claude.profil"), "{refus}");
    assert_eq!(accepter("absent", &dossier).unwrap_err().code, Code::ProfilInconnu);
    assert!(!dossier.join("profils").join("claude.empreinte").exists());
    // Un profil faux, ou un nom faux, ne s'accepte pas, et rien n'est écrit.
    fs::write(dossier.join("profils").join("faux.profil"), "sorte = claude\nbudget = 5\n").unwrap();
    assert_eq!(accepter("faux", &dossier).unwrap_err().code, Code::ProfilLigneRefusee);
    fs::write(dossier.join("profils").join("vide.profil"), "").unwrap();
    assert_eq!(accepter("vide", &dossier).unwrap_err().code, Code::ProfilIncomplet);
    assert!(!dossier.join("profils").join("faux.empreinte").exists());
    assert_eq!(accepter("../veille", &dossier).unwrap_err().code, Code::ProfilNomRefuse);
    // Une empreinte qui ne s'écrit pas, ou ne se lit pas (un dossier à sa place) : le refus 6006, qui la nomme.
    fs::remove_file(&garde).unwrap();
    fs::create_dir(&garde).unwrap();
    for refus in
        [accepter("veille", &dossier).unwrap_err(), charger_dans("veille", &dossier).unwrap().garder().unwrap_err()]
    {
        assert_eq!(refus.code, Code::EntreeSortie);
        assert!(refus.preuve.contains(&garde.display().to_string()), "{refus}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn le_fichier_de_connexion_est_la_ou_il_existe() {
    let racine = racine("profil-presente");
    let claude = racine.join(".claude");
    assert!(!connexion_presente(&claude), "ni le dossier ni le fichier");
    fs::create_dir_all(&claude).unwrap();
    assert!(!connexion_presente(&claude), "le dossier sans le fichier");
    fs::write(claude.join(".credentials.json"), [0xFF, 0xFE, 0x00]).unwrap();
    assert!(connexion_presente(&claude), "le fichier, quel que soit son contenu");
    // Un fichier à la place du dossier : rien dessous.
    let fichier = racine.join("fichier");
    fs::write(&fichier, "x").unwrap();
    assert!(!connexion_presente(&fichier));
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_empreinte_acceptee_est_celle_d_un_texte_entier_en_minuscules() {
    let dossier = kraken("garde-stricte");
    let garde = dossier.join("profils").join("veille.empreinte");
    fs::write(dossier.join("profils").join("veille.profil"), "sorte = claude\n").unwrap();
    let (_, empreinte) = accepter("veille", &dossier).unwrap();
    assert_eq!(empreinte, hex(&sha256(b"sorte = claude\n")));
    // En majuscules, tronquée, vide ou rallongée : ce n'est pas l'empreinte lue, le refus 6012 (le fichier est là, ce
    // n'est pas le 6011).
    for fausse in [empreinte.to_uppercase(), empreinte[..63].to_string(), String::new(), format!("{empreinte}0")] {
        fs::write(&garde, &fausse).unwrap();
        let refus = charger_dans("veille", &dossier).unwrap().garder().unwrap_err();
        assert_eq!(refus.code, Code::ProfilChange, "{fausse:?}");
    }
    // Les blancs et la fin de ligne autour de la bonne ne la changent pas.
    fs::write(&garde, format!("  {empreinte} \r\n")).unwrap();
    assert_eq!(charger_dans("veille", &dossier).unwrap().garder(), Ok(()));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn le_quota_se_lit_en_micro_dollars() {
    for (texte, attendu) in [
        ("5", Some(5_000_000)),
        ("5\n", Some(5_000_000)),
        ("12.50\r\n", Some(12_500_000)),
        ("\u{feff}0.01\n", Some(10_000)),
        ("  3.5  \n", Some(3_500_000)),
        ("0", None),
        ("0.00", None),
        ("", None),
        ("\n", None),
        ("-1", None),
        ("+1", None),
        ("1,5", None),
        ("1e2", None),
        ("1.234", None),
        (".5", None),
        ("5.", None),
        ("abc", None),
        ("5 $", None),
        ("5\n6\n", None),
        ("99999999999999999999", None),
    ] {
        assert_eq!(quota_de(texte), attendu, "{texte:?}");
    }
}

#[test]
fn un_quota_absent_ne_borne_rien_et_un_quota_mal_forme_est_refuse() {
    let dossier = kraken("quota");
    let chemin = dossier.join("profils").join("quota");
    assert_eq!(lire_quota(&dossier), Ok(None), "absent : rien ne se vérifie");
    fs::write(&chemin, "2.50\n").unwrap();
    assert_eq!(lire_quota(&dossier), Ok(Some(2_500_000)));
    for mauvais in [&b"beaucoup\n"[..], b"0\n", b"1.001\n", b"\xff\xfe", b""] {
        fs::write(&chemin, mauvais).unwrap();
        let refus = lire_quota(&dossier).unwrap_err();
        assert_eq!(refus.code, Code::Quota, "{mauvais:?}");
        assert!(refus.preuve.contains(&chemin.display().to_string()), "la preuve nomme le fichier : {refus}");
    }
    // Un dossier à sa place : il ne se lit pas, et le refus nomme le quota.
    fs::remove_file(&chemin).unwrap();
    fs::create_dir(&chemin).unwrap();
    assert_eq!(lire_quota(&dossier).unwrap_err().code, Code::Quota);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn la_depense_du_jour_somme_les_usages_estimes_de_ce_jour_seulement() {
    let dossier = kraken("depense");
    let usage = |modele: &str, micro_dollars: u64, base: Base| Evenement::Usage {
        vague: "v".into(),
        main: 1,
        tentative: 1,
        modele: modele.into(),
        entree: 1,
        sortie: 1,
        cache_ecrit: 0,
        cache_lu: 0,
        reflexion: 0,
        micro_dollars,
        base,
    };
    let seance = Fil::seance(&dossier, "humain");
    for evenement in [
        usage("a", 100, Base::Estime),
        usage("b", 20, Base::Estime),
        usage("c", 5_000, Base::Forfait),
        usage("d", 0, Base::Inconnu),
    ] {
        seance.noter(&evenement).unwrap();
    }
    let jour = &instant(maintenant())[..10];
    assert_eq!(depense_du_jour(&dossier, jour), Ok(120), "le forfait et l'inconnu ne comptent pas");
    assert_eq!(depense_du_jour(&dossier, "2001-01-01"), Ok(0), "un autre jour ne compte pas");
    let _ = fs::remove_dir_all(&dossier);
}

/// Un chemin absolu pour le système qui joue le test ; il n'est jamais lu, ni créé.
fn absolu(chemin: &str) -> PathBuf {
    PathBuf::from(if cfg!(windows) { r"C:\" } else { "/" }).join(chemin)
}

fn valeur(chemin: &str) -> Option<OsString> {
    Some(absolu(chemin).into_os_string())
}

// Les tests du fil de ce dossier, à part : `dossier` y nomme l'aide des tests (un dossier neuf), pas `dossier()`.
mod fil;
