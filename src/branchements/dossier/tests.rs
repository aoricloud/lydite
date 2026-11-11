//! Les tests du dossier de Kraken.
use super::*;
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::fil::{instant, maintenant};
use crate::essais::{effacer, kraken, lacher, racine};
use std::time::{Duration, Instant};

#[test]
fn le_dossier_range_le_fil_et_les_profils_sous_les_noms_que_les_regles_de_la_main_ferment() {
    // Les règles `Edit` du profil visent ces noms (`profil::FIL`, `profil::PROFILS`) : le dossier les écrit sous eux.
    assert_eq!(FICHIER, profil::FIL);
    let dossier = Path::new("k");
    assert_eq!(fichier("veille", dossier).unwrap(), dossier.join(profil::PROFILS).join("veille.profil"));
    assert_eq!(fichier("veille", dossier).unwrap().parent(), Some(dossier.join("profils").as_path()));
}

#[test]
fn tenir_cree_le_dossier_et_son_fichier_a_part_du_fil() {
    let dir = racine("tenir").join("kraken");
    assert!(!dir.exists());
    tenir(&dir).unwrap();
    assert!(dir.join(VERROU).is_file());
    assert!(!dir.join(FICHIER).exists(), "le verrou n'est pas le fil");
    // Un lancement qui redemande son dossier (ses autres vagues) ne se refuse pas.
    tenir(&dir).unwrap();
    effacer(dir.parent().unwrap());
}

#[test]
fn un_dossier_que_tient_un_autre_est_refuse_en_6032_et_se_prend_des_qu_il_le_lache() {
    let dir = racine("pris-par-un-autre").join("kraken");
    fs::create_dir_all(&dir).unwrap();
    // L'autre lancement : un autre processus tient le même verrou du système, que ce fichier-ci fait de même.
    let autre = OpenOptions::new().create(true).write(true).truncate(false).open(dir.join(VERROU)).unwrap();
    autre.try_lock().unwrap();
    let refus = tenir(&dir).unwrap_err();
    assert_eq!(refus.code, Code::DossierPris);
    assert!(refus.preuve.contains(&dir.display().to_string()), "{refus}");
    assert!(refus.correction.contains("Attends la fin de l'autre lancement"), "{refus}");
    assert!(!dir.join(FICHIER).exists(), "le refus n'écrit pas au fil");
    // Il ne reste rien du refus : l'autre fini (ou tué), le dossier se prend.
    drop(autre);
    tenir(&dir).unwrap();
    effacer(dir.parent().unwrap());
}

/// Le verrou d'un autre lancement (ou de la sonde) sur `dir`, que le système tient tant que le fichier reste ouvert.
fn verrou_d_un_autre(dir: &Path) -> fs::File {
    let autre = OpenOptions::new().create(true).write(true).truncate(false).open(dir.join(VERROU)).unwrap();
    // La prise se réessaie : une prise exclusive qui suit une sonde peut tomber sur la copie égarée de cette sonde,
    // qu'un processus né à cet instant garde entre `fork` et `exec`.
    let debut = Instant::now();
    while autre.try_lock().is_err() {
        assert!(debut.elapsed() < Duration::from_secs(30), "le verrou de {} ne se prend pas", dir.display());
        std::thread::sleep(Duration::from_millis(1));
    }
    autre
}

#[test]
fn un_lancement_qui_tombe_sur_un_verrou_bref_reessaie_et_le_prend() {
    let dir = racine("reessai").join("kraken");
    fs::create_dir_all(&dir).unwrap();
    let mut autre = Some(verrou_d_un_autre(&dir));
    let mut pauses = 0;
    // Le verrou se lâche pendant la première pause : la sonde a fini. Le lancement l'a pris, sans refus.
    tenir_en(&dir, 30_000, &mut || {
        pauses += 1;
        autre = None;
        std::thread::sleep(Duration::from_millis(1));
    })
    .unwrap();
    // Il réessaie au moins une fois, puis prend le verrou ; « à l'essai suivant » ne se promet pas tant qu'une copie du
    // descripteur peut vivre.
    assert!(pauses >= 1);
    // Il le tient bien : une sonde le voit tenu.
    assert!(sonder(&dir).unwrap());
    effacer(dir.parent().unwrap());
}

#[test]
fn un_verrou_qui_ne_se_lache_pas_est_refuse_en_6032_apres_ses_reessais() {
    let dir = racine("reessai-vain").join("kraken");
    fs::create_dir_all(&dir).unwrap();
    let _autre = verrou_d_un_autre(&dir);
    let mut pauses = 0;
    let refus = tenir_en(&dir, 4, &mut || pauses += 1).unwrap_err();
    assert_eq!(refus.code, Code::DossierPris);
    assert_eq!(pauses, 4, "un essai de plus que de pauses, puis le refus");
    effacer(dir.parent().unwrap());
}

#[test]
fn la_sonde_dit_le_dossier_tenu_sans_le_garder_ni_rien_creer() {
    let racine = racine("sonde");
    let dir = racine.join("kraken");
    // Ni dossier ni fichier de verrou : personne ne tient, et la sonde ne crée rien.
    assert!(!sonder(&dir).unwrap());
    assert!(!dir.exists());
    fs::create_dir_all(&dir).unwrap();
    assert!(!sonder(&dir).unwrap());
    assert!(!dir.join(VERROU).exists(), "la sonde ne crée pas le fichier du verrou");
    // Un autre lancement le tient : tenu ; lâché : libre.
    let autre = verrou_d_un_autre(&dir);
    assert!(sonder(&dir).unwrap());
    lacher(autre, &dir);
    assert!(!sonder(&dir).unwrap());
    // La sonde ne garde rien : un verrou exclusif se prend après, d'un autre fichier.
    let suivant = verrou_d_un_autre(&dir);
    lacher(suivant, &dir);
    // Une sonde qui tient le verrou en partage fait réessayer `tenir_en`, non refuser : le cas des deux à la fois.
    let sonde = OpenOptions::new().read(true).open(dir.join(VERROU)).unwrap();
    sonde.try_lock_shared().unwrap();
    let mut sonde = Some(sonde);
    tenir_en(&dir, 30_000, &mut || {
        sonde = None;
        std::thread::sleep(Duration::from_millis(1));
    })
    .unwrap();
    effacer(&racine);
}

#[test]
fn deux_dossiers_se_tiennent_ensemble() {
    let racine = racine("deux-tenus");
    let (a, b) = (racine.join("a"), racine.join("b"));
    tenir(&a).unwrap();
    tenir(&b).unwrap();
    effacer(&racine);
}

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
