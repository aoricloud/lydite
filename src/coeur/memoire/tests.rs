use super::*;
use crate::coeur::fil::{
    Evenement, IssueVague, SceauVague, ligne_d_issue, ligne_de_note, ligne_de_sceau, lire_octets_avec_lignes,
};
use crate::essais::codes;
use std::path::Path;

const INSTANT: &str = "2026-10-31T21:03:00.123Z";

/// Une ligne `main.debut` bien formée, écrite à la main, de la séance `seance` (sans caractère à échapper), au `seq`
/// donné, de la main `main`.
fn debut(seance: &str, seq: u64, main: usize) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"{instant}","origine":"humain","#,
            r#""origine_foi":"declare","type":"main.debut","vague":"v","main":{main},"tentative":1}}"#,
        ),
        seance = seance,
        seq = seq,
        instant = INSTANT,
        main = main,
    )
}

/// Une ligne `main.constat`, à `foi: constate`.
fn constat(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"{instant}","origine":"humain","#,
            r#""origine_foi":"declare","type":"main.constat","vague":"v","main":1,"tentative":1,"#,
            r#""version":"2.1.312","mur_actif":true,"foi":"constate"}}"#,
        ),
        seance = seance,
        seq = seq,
        instant = INSTANT,
    )
}

/// Une ligne `main.fournisseur`, à `foi: declare`.
fn fournisseur(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"{instant}","origine":"humain","#,
            r#""origine_foi":"declare","type":"main.fournisseur","vague":"v","main":1,"tentative":1,"#,
            r#""fournisseur":"api.example.com","foi":"declare"}}"#,
        ),
        seance = seance,
        seq = seq,
        instant = INSTANT,
    )
}

/// La ligne `main.debut` de `seq` 1 que Kraken écrit pour la séance `seance`, quelle qu'elle soit : le rédacteur du
/// fil échappe ce qu'il faut.
fn ecrite_par_kraken(seance: &str) -> String {
    ligne_de_note(seance, 1, "humain", &Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 })
}

/// Les octets d'un fil : chaque ligne finie par `\n`.
fn fil_de<S: AsRef<str>>(lignes: &[S]) -> Vec<u8> {
    lignes.iter().map(|ligne| format!("{}\n", ligne.as_ref())).collect::<String>().into_bytes()
}

/// Les lignes que le lecteur tire de ces octets.
fn lues(octets: &[u8]) -> Vec<LigneLue> {
    lire_octets_avec_lignes(Path::new("fil.jsonl"), octets).unwrap().1
}

/// L'empreinte attendue d'une ligne écrite en toutes lettres : le SHA-256 de ses octets, sans `\n`.
fn empreinte(ligne: &str) -> String {
    hex(&sha256(ligne.as_bytes()))
}

#[test]
fn l_index_d_un_fil_de_deux_notes() {
    let (premiere, seconde) = (constat("s", 1), fournisseur("s", 2));
    let octets = fil_de(&[&premiere, &seconde]);
    let attendu = format!(
        "s/1 {} main.constat {INSTANT} observed {}\ns/2 {} main.fournisseur {INSTANT} declared {}\n",
        empreinte(&premiere),
        premiere.len(),
        empreinte(&seconde),
        seconde.len(),
    );
    assert_eq!(index(&octets, &lues(&octets)), attendu);
}

#[test]
fn un_fil_sans_ligne_lue_a_un_index_vide() {
    assert_eq!(index(b"", &lues(b"")), "");
    // Ce qui suit le dernier `\n` attend la lecture suivante, et une ligne vide ne dit rien.
    let octets = format!("\n{}", debut("s", 1, 1)).into_bytes();
    assert_eq!(index(&octets, &lues(&octets)), "");
}

#[test]
fn une_ligne_sautee_n_est_pas_dans_l_index_et_ses_voisines_y_sont() {
    let trop_plat = debut("s", 2, 1).replace(r#""main":1"#, r#""main":[1]"#);
    let mal_forme = debut("s", 4, 1).replace(r#""main":1"#, r#""main":"x""#);
    let inconnue = debut("s", 5, 1).replace("main.debut", "autre.chose");
    let lignes = [
        debut("s", 1, 1),
        trop_plat,
        debut("s", 3, 1),
        mal_forme,
        inconnue,
        "ceci n'est pas du JSON".to_string(),
        debut("s", 6, 1),
    ];
    let mut octets = fil_de(&lignes);
    // Une ligne en cours d'écriture, sans `\n`, n'y est pas non plus.
    octets.extend_from_slice(debut("s", 7, 1).as_bytes());
    let (lu, lignes_lues) = lire_octets_avec_lignes(Path::new("fil.jsonl"), &octets).unwrap();
    // Le lecteur saute bien les lignes 2, 4 et 6 (1009, 1009, 1001) : sans cela l'absence ne prouverait rien.
    assert_eq!(codes(&lu.0.ecarts), [(2, 1009), (4, 1009), (6, 1001)]);
    let texte = index(&octets, &lignes_lues);
    let adresses: Vec<&str> = texte.lines().map(|ligne| ligne.split(' ').next().unwrap()).collect();
    assert_eq!(adresses, ["s/1", "s/3", "s/6"]);
    assert_eq!(rappeler(&octets, &lignes_lues, "s/3").unwrap(), debut("s", 3, 1).as_bytes());
    for sautee in ["s/2", "s/4", "s/5", "s/7"] {
        assert_eq!(rappeler(&octets, &lignes_lues, sautee).unwrap_err().code, Code::MemoireTexteAbsent, "{sautee}");
    }
}

#[test]
fn une_issue_et_un_sceau_sont_dans_l_index_sans_foi() {
    let issue = ligne_d_issue("s", 1, "humain", &IssueVague { vague: "v".into(), issue: 12 });
    let sceau = ligne_de_sceau("s", 2, "humain", &SceauVague::sur("v", b"abc"));
    let note = ligne_de_note("s", 3, "humain", &Evenement::Correction { vague: "v".into(), corrige: "u".into() });
    let octets = fil_de(&[&issue, &sceau, &note]);
    let texte = index(&octets, &lues(&octets));
    let champs: Vec<Vec<&str>> = texte.lines().map(|ligne| ligne.split(' ').collect()).collect();
    assert_eq!(champs.len(), 3, "{texte}");
    for (rang, (ligne, genre)) in
        [(&issue, "vague.issue"), (&sceau, "vague.sceau"), (&note, "vague.correction")].into_iter().enumerate()
    {
        let champs = &champs[rang];
        assert_eq!(champs.len(), 6, "{texte}");
        assert_eq!(champs[0], format!("s/{}", rang + 1));
        assert_eq!(champs[1], empreinte(ligne));
        assert_eq!(champs[2], genre);
        assert_eq!(champs[4], "-", "{genre} n'affirme aucune foi");
        assert_eq!(champs[5], ligne.len().to_string());
    }
}

#[test]
fn une_seance_qui_n_a_pas_d_adresse_s_ecrit_tiret_et_se_rappelle_par_son_empreinte() {
    // Le témoin : une séance sans rien de tout cela a son adresse.
    let temoin = ecrite_par_kraken("lot_1.2-x");
    let octets = fil_de(&[&temoin]);
    let lue = lues(&octets);
    assert_eq!(adresse(&lue[0]).as_deref(), Some("lot_1.2-x/1"));
    assert!(index(&octets, &lue).starts_with("lot_1.2-x/1 "));
    for seance in ["deux mots", "tab\there", "s\u{1}c", "del\u{7f}", "séance", "a/b", " ", "\u{a0}nbsp"] {
        let ligne = ecrite_par_kraken(seance);
        let octets = fil_de(&[&ligne]);
        let lue = lues(&octets);
        assert_eq!(lue.len(), 1, "{seance:?} se lit");
        assert_eq!(adresse(&lue[0]), None, "{seance:?}");
        let texte = index(&octets, &lue);
        assert!(texte.starts_with(&format!("- {} main.debut ", empreinte(&ligne))), "{seance:?} : {texte}");
        // Son empreinte la rappelle, octet pour octet ; l'adresse qu'on lui supposerait ne la rappelle pas.
        assert_eq!(rappeler(&octets, &lue, &empreinte(&ligne)).unwrap(), ligne.as_bytes(), "{seance:?}");
        let supposee = rappeler(&octets, &lue, &format!("{seance}/1")).unwrap_err();
        assert_eq!(supposee.code, Code::MemoireTexteAbsent, "{seance:?}");
    }
}

#[test]
fn l_adresse_et_l_empreinte_rappellent_la_meme_ligne_a_l_octet() {
    let lignes = [constat("s", 1), debut("t", 1, 1), fournisseur("s", 2)];
    let octets = fil_de(&lignes);
    let lue = lues(&octets);
    assert_eq!(lue.len(), 3);
    for (ligne, lue_ici) in lignes.iter().zip(&lue) {
        let adresse = adresse(lue_ici).unwrap();
        let par_adresse = rappeler(&octets, &lue, &adresse).unwrap();
        let par_empreinte = rappeler(&octets, &lue, &empreinte(ligne)).unwrap();
        assert_eq!(par_adresse, ligne.as_bytes(), "{adresse}");
        assert_eq!(par_empreinte, ligne.as_bytes(), "{adresse}");
        // La ligne du fil elle-même, non une copie : le rappel est une tranche des octets du fil.
        assert!(std::ptr::eq(par_adresse.as_ptr(), octets[lue_ici.debut..].as_ptr()), "{adresse}");
        assert!(std::ptr::eq(par_empreinte.as_ptr(), octets[lue_ici.debut..].as_ptr()), "{adresse}");
    }
}

#[test]
fn le_rappel_garde_les_octets_exacts_d_une_ligne_a_retour_chariot() {
    // Le fil se coupe sur `\n` seul : un `\r` de fin fait partie de la ligne, de son empreinte et de sa taille.
    let ligne = format!("{}\r", debut("s", 1, 1));
    let octets = fil_de(&[&ligne]);
    let lue = lues(&octets);
    assert_eq!(lue.len(), 1, "la ligne à `\\r` se lit");
    assert_eq!(rappeler(&octets, &lue, "s/1").unwrap(), ligne.as_bytes());
    let attendu = format!("s/1 {} main.debut {INSTANT} - {}\n", empreinte(&ligne), ligne.len());
    assert_eq!(index(&octets, &lue), attendu);
    assert!(ligne.ends_with('\r') && !attendu.contains('\r'));
}

#[test]
fn une_ligne_de_seq_zero_se_rappelle_par_son_adresse() {
    let (zero, un) = (debut("x", 0, 1), debut("x", 1, 2));
    let octets = fil_de(&[&zero, &un]);
    let (lu, lue) = lire_octets_avec_lignes(Path::new("fil.jsonl"), &octets).unwrap();
    // Le lecteur lit la ligne de seq 0 avec un écart : 1 est attendu en tête de la séance.
    assert_eq!(codes(&lu.0.ecarts), [(1, 1010)]);
    assert_eq!(lue.len(), 2);
    assert_eq!(rappeler(&octets, &lue, "x/0").unwrap(), zero.as_bytes());
    assert_eq!(rappeler(&octets, &lue, "x/1").unwrap(), un.as_bytes());
    assert!(index(&octets, &lue).starts_with("x/0 "));
}

#[test]
fn une_adresse_mal_formee_ou_absente_est_6034() {
    let octets = fil_de(&[debut("x", 0, 1), debut("x", 1, 2)]);
    let lue = lues(&octets);
    // Le témoin : les deux adresses bien formées rappellent leur ligne.
    for bien in ["x/0", "x/1"] {
        assert!(rappeler(&octets, &lue, bien).is_ok(), "{bien}");
    }
    let absent = [
        "x/01",
        "x/+1",
        "x/abc",
        "/1",
        "x/",
        "x/-1",
        "x/00",
        "x/ 1",
        "x/1/",
        "x/1 ",
        "x/99999999999999999999999",
        "x/2",
        "y/1",
        "/",
        "//",
    ];
    for texte in absent {
        let refus = rappeler(&octets, &lue, texte).unwrap_err();
        assert_eq!(refus.code.numero(), 6034, "{texte:?}");
        assert_eq!(refus.code.nom(), "lancer.memoire_texte_absent");
        assert!(refus.preuve.contains(&format!("{texte:?}")), "la preuve cite le texte : {}", refus.preuve);
        assert!(refus.correction.contains("lydite memory"), "{}", refus.correction);
    }
}

#[test]
fn un_texte_sans_barre_qui_n_est_pas_une_empreinte_est_6035() {
    let ligne = debut("x", 1, 1);
    let octets = fil_de(&[&ligne]);
    let lue = lues(&octets);
    let bonne = empreinte(&ligne);
    // Le témoin : l'empreinte bien écrite rappelle la ligne.
    assert_eq!(rappeler(&octets, &lue, &bonne).unwrap(), ligne.as_bytes());
    let fausses = [
        "mot".to_string(),
        String::new(),
        bonne[..63].to_string(),
        format!("{bonne}0"),
        bonne.to_uppercase(),
        format!("{bonne} "),
        "g".repeat(64),
    ];
    for texte in &fausses {
        let refus = rappeler(&octets, &lue, texte).unwrap_err();
        assert_eq!(refus.code.numero(), 6035, "{texte:?}");
        assert_eq!(refus.code.nom(), "lancer.memoire_empreinte_fausse");
        assert!(refus.preuve.contains(&format!("{texte:?}")), "la preuve cite le texte : {}", refus.preuve);
        assert!(refus.correction.contains("lydite memory"), "{}", refus.correction);
    }
    // Refusé avant la lecture du fil : même sans aucune ligne, un mot reste 6035 et non 6034.
    assert_eq!(rappeler(b"", &[], "mot").unwrap_err().code, Code::MemoireEmpreinteFausse);
    // Bien formée mais que le fil ne porte pas : 6034.
    let autre = rappeler(&octets, &lue, &"0".repeat(64)).unwrap_err();
    assert_eq!(autre.code, Code::MemoireTexteAbsent);
    assert!(autre.preuve.contains(&"0".repeat(64)), "{}", autre.preuve);
}

#[test]
fn de_deux_lignes_de_meme_adresse_l_adresse_rend_la_premiere() {
    let (premiere, seconde) = (debut("s", 1, 1), debut("s", 1, 2));
    assert_ne!(premiere, seconde);
    let octets = fil_de(&[&premiere, &seconde]);
    let (lu, lue) = lire_octets_avec_lignes(Path::new("fil.jsonl"), &octets).unwrap();
    // Le `seq` redit est l'écart 1010, et la ligne est lue.
    assert_eq!(codes(&lu.0.ecarts), [(2, 1010)]);
    // L'index les montre toutes les deux, sous la même adresse, et leur empreinte les distingue.
    let texte = index(&octets, &lue);
    let champs: Vec<Vec<&str>> = texte.lines().map(|ligne| ligne.split(' ').collect()).collect();
    assert_eq!((champs[0][0], champs[1][0]), ("s/1", "s/1"));
    assert_eq!((champs[0][1], champs[1][1]), (empreinte(&premiere).as_str(), empreinte(&seconde).as_str()));
    // L'adresse rend la première ; chaque empreinte rend la sienne.
    assert_eq!(rappeler(&octets, &lue, "s/1").unwrap(), premiere.as_bytes());
    assert_eq!(rappeler(&octets, &lue, &empreinte(&seconde)).unwrap(), seconde.as_bytes());
    // Une ligne ajoutée plus tard sous la même adresse ne change pas ce qu'elle rappelle.
    let troisieme = debut("s", 1, 3);
    let plus_tard = fil_de(&[&premiere, &seconde, &troisieme]);
    let lue_plus_tard = lues(&plus_tard);
    assert_eq!(lue_plus_tard.len(), 3);
    assert_eq!(rappeler(&plus_tard, &lue_plus_tard, "s/1").unwrap(), premiere.as_bytes());
}
