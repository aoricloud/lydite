use super::*;
use crate::branchements::dossier::{FICHIER, Fil, lire, lire_tout};
use crate::coeur::empreinte::{hex, sha256};
use crate::essais::{codes, dossier, sans_instant, seqs};
use std::collections::HashSet;
use std::fs;
use std::thread;

/// Pose ces lignes, chacune finie par `\n`, en fil du dossier.
fn poser(dir: &Path, lignes: &[&str]) {
    fs::write(dir.join(FICHIER), lignes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>()).unwrap();
}

/// Une ligne `main.debut` bien formée, écrite à la main, de la séance `seance` au `seq` donné : un test la change
/// par `replace`.
fn debut(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"main.debut","vague":"v","main":1"#,
            r#","tentative":1}}"#,
        ),
        seance = seance,
        seq = seq,
    )
}

/// La ligne, son id de séance et son instant remplacés : ils changent d'une écriture à l'autre.
fn sans_seance_ni_instant(ligne: &str) -> String {
    let ligne = sans_instant(ligne);
    let (avant, apres) = ligne.split_once(r#""seance":""#).expect("une ligne porte sa séance");
    let (_, apres) = apres.split_once('"').expect("une séance entre guillemets");
    format!(r#"{avant}"seance":"<seance>"{apres}"#)
}

/// Une empreinte de profil : le SHA-256 d'un fichier vide, en hexadécimal.
const EMPREINTE: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// Une ligne `main.lancement` bien formée, écrite à la main, de la séance `seance` au `seq` donné : un test la
/// change par `replace`.
fn lancement(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"main.lancement","vague":"v","mai"#,
            r#"n":1,"tentative":1,"profil":"claude","empreinte":"{EMPREINTE}","modele":"sonnet""#,
            r#","mur":"demande","reglages":"{{}}","reglages_depot":"","foi":"declare"}}"#,
        ),
        seance = seance,
        seq = seq,
        EMPREINTE = EMPREINTE,
    )
}

/// Une ligne `main.constat` bien formée, écrite à la main, de la séance `seance` au `seq` donné.
fn constat(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"main.constat","vague":"v","main""#,
            r#":1,"tentative":1,"version":"2.1.312","mur_actif":true,"foi":"constate"}}"#,
        ),
        seance = seance,
        seq = seq,
    )
}

/// Les vagues de l'exemple et leurs séances.
const A: &str = "1761901200000_7_0";
const B: &str = "1761901200500_8_0";

/// Deux vagues mêlées, chacune dans sa séance. Dans la première, la main 1 est prise du premier coup, la 2 à la
/// relance, la 3 classée ; dans la seconde, sans vérification, la main 1 est prise sur sa déclaration.
const EXEMPLE: &[&str] = &[
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":1,"instant":"2025-10-31T09:00:00.000Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"vague.ouverte","vague":"1761"#,
        r#"901200000_7_0","n":3,"depot":"/srv/depot","verifier":"cargo test","objectif":"Éc"#,
        r#"rire bonjour.txt"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":2,"instant":"2025-10-31T09:00:00.010Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"1761901"#,
        r#"200000_7_0","main":1,"tentative":1}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":3,"instant":"2025-10-31T09:00:00.011Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"1761901"#,
        r#"200000_7_0","main":2,"tentative":1}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":4,"instant":"2025-10-31T09:00:00.012Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"1761901"#,
        r#"200000_7_0","main":3,"tentative":1}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200500_8_1","seq":1,"instant":"2025-10-31T09:00:00.500Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"vague.ouverte","vague":"1761"#,
        r#"901200500_8_0","n":1,"depot":"/srv/autre","verifier":null,"objectif":"Écrire sal"#,
        r#"ut.txt"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200500_8_1","seq":2,"instant":"2025-10-31T09:00:00.510Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"1761901"#,
        r#"200500_8_0","main":1,"tentative":1}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":5,"instant":"2025-10-31T09:00:30.000Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"176190120"#,
        r#"0000_7_0","main":3,"tentative":1,"couleur":"rouge","foi":"constate","texte":"lan"#,
        r#"cement impossible de claude : introuvable","verif":null}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":6,"instant":"2025-10-31T09:00:30.001Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"1761901"#,
        r#"200000_7_0","main":3,"tentative":2}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":7,"instant":"2025-10-31T09:00:36.000Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"176190120"#,
        r#"0000_7_0","main":1,"tentative":1,"couleur":"vert","foi":"constate","texte":"bonj"#,
        r#"our.txt écrit","verif":"test result: ok. 1 passed"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":8,"instant":"2025-10-31T09:00:36.001Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.verdict","vague":"17619"#,
        r#"01200000_7_0","main":1,"verdict":"prise","foi":"constate"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200500_8_1","seq":3,"instant":"2025-10-31T09:00:40.000Z""#,
        r#","origine":"agent:main-1761901200500_8_0-1","origine_foi":"declare","type":"main"#,
        r#".fin","vague":"1761901200500_8_0","main":1,"tentative":1,"couleur":"vert","foi":"#,
        r#""declare","texte":"salut.txt écrit\nPREUVE: vert","verif":null}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200500_8_1","seq":4,"instant":"2025-10-31T09:00:40.001Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.verdict","vague":"17619"#,
        r#"01200500_8_0","main":1,"verdict":"prise","foi":"declare"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":9,"instant":"2025-10-31T09:00:50.000Z""#,
        r#","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"176190120"#,
        r#"0000_7_0","main":2,"tentative":1,"couleur":"rouge","foi":"constate","texte":"vér"#,
        r#"ification « cargo test » rouge (exit code: 101) :\ntest bonjour ... FAILED","ver"#,
        r#"if":"test bonjour ... FAILED"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":10,"instant":"2025-10-31T09:00:50.001Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.debut","vague":"176190"#,
        r#"1200000_7_0","main":2,"tentative":2}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":11,"instant":"2025-10-31T09:01:00.000Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"17619012"#,
        r#"00000_7_0","main":3,"tentative":2,"couleur":"rouge","foi":"constate","texte":"la"#,
        r#"ncement impossible de claude : introuvable","verif":null}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":12,"instant":"2025-10-31T09:01:00.001Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.verdict","vague":"1761"#,
        r#"901200000_7_0","main":3,"verdict":"classee","foi":"constate"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":13,"instant":"2025-10-31T09:01:29.000Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"17619012"#,
        r#"00000_7_0","main":2,"tentative":2,"couleur":"vert","foi":"constate","texte":"bon"#,
        r#"jour.txt corrigé\ten CRLF","verif":"test result: ok. 1 passed"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901200000_7_1","seq":14,"instant":"2025-10-31T09:01:29.001Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.verdict","vague":"1761"#,
        r#"901200000_7_0","main":2,"verdict":"prise","foi":"constate"}"#,
    ),
];

/// Ce que `lancer` rend pour une vague : l'id de chaque main, et sa sortie (constatée ou non) ou son erreur.
type Rendu<'a> = Vec<(String, Result<(&'a str, bool), &'a str>)>;

fn rendu(vague: &Vague) -> Rendu<'_> {
    let rendu = vague.mains.iter().map(|(n, main)| {
        let issue = match main.verdict.expect("une main sans verdict") {
            (Verdict::Prise, foi) => Ok((main.texte(), foi == Foi::Constate)),
            (Verdict::Classee | Verdict::Impossible, _) => Err(main.texte()),
        };
        (format!("main-{}-{n}", vague.id), issue)
    });
    rendu.collect()
}

#[test]
fn un_fil_exemple_se_rejoue() {
    let dir = dossier("rejeu");
    poser(&dir, EXEMPLE);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(notes.len(), EXEMPLE.len());
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let vagues = etat(&evenements);
    assert_eq!(vagues.len(), 2);
    let attendu: Rendu = vec![
        (format!("main-{A}-1"), Ok(("bonjour.txt écrit", true))),
        (format!("main-{A}-2"), Ok(("bonjour.txt corrigé\ten CRLF", true))),
        (format!("main-{A}-3"), Err("lancement impossible de claude : introuvable")),
    ];
    assert_eq!(rendu(&vagues[0]), attendu);
    let attendu: Rendu = vec![(format!("main-{B}-1"), Ok(("salut.txt écrit\nPREUVE: vert", false)))];
    assert_eq!(rendu(&vagues[1]), attendu);
    let couleurs = |vague: &Vague, n| vague.mains[&n].tentatives.values().map(|t| t.couleur).collect::<Vec<_>>();
    let (vert, rouge) = (Some((Couleur::Vert, Foi::Constate)), Some((Couleur::Rouge, Foi::Constate)));
    assert_eq!(couleurs(&vagues[0], 1), [vert]);
    assert_eq!(couleurs(&vagues[0], 2), [rouge, vert]);
    assert_eq!(couleurs(&vagues[0], 3), [rouge, rouge]);
    assert_eq!(couleurs(&vagues[1], 1), [Some((Couleur::Vert, Foi::Declare))]);
    let relancee = &vagues[0].mains[&2].tentatives[&1];
    assert!(relancee.texte.starts_with("vérification « cargo test » rouge"), "{}", relancee.texte);
    assert_eq!(relancee.verif.as_deref(), Some("test bonjour ... FAILED"));
    assert_eq!(vagues[0].mains[&3].tentatives[&1].verif, None, "une main rouge avant la vérification");
    let ouvertures = vagues.iter().map(|v| (v.n, v.depot.as_str(), v.verifier.as_deref(), v.objectif.as_str()));
    assert_eq!(
        ouvertures.collect::<Vec<_>>(),
        [(3, "/srv/depot", Some("cargo test"), "Écrire bonjour.txt"), (1, "/srv/autre", None, "Écrire salut.txt")]
    );
    // La fin déclarée porte l'origine de sa main ; les autres lignes, celle de leur séance.
    let origines: Vec<&str> = notes.iter().map(|note| note.origine.as_str()).collect();
    let main = format!("agent:main-{B}-1");
    assert_eq!(origines.iter().filter(|origine| **origine != "humain").collect::<Vec<_>>(), [&main.as_str()]);
    // Noter ce qu'on a lu, une séance neuve par séance lue, redonne les mêmes lignes, aux ids de séance et aux
    // instants près.
    let copie = dir.join("copie");
    fs::create_dir(&copie).unwrap();
    let mut seances: BTreeMap<&str, Fil> = BTreeMap::new();
    for note in &notes {
        let fil = seances.entry(&note.seance).or_insert_with(|| Fil::seance(&copie, "humain"));
        fil.noter(&note.evenement).unwrap();
    }
    let relu = fs::read_to_string(copie.join(FICHIER)).unwrap();
    let relu: Vec<String> = relu.lines().map(sans_seance_ni_instant).collect();
    assert_eq!(relu, EXEMPLE.iter().map(|ligne| sans_seance_ni_instant(ligne)).collect::<Vec<_>>());
    let _ = fs::remove_dir_all(&dir);
}

/// La vague C et les séances de trois programmes qui ne sont pas Kraken, mêlées dans le fichier :
/// - Python, `json.dumps` : un espace après `:` et `,`, tout l'accentué échappé (`\u00e9`, une paire de substituts
///   pour l'émoji), une fois les clés triées (`sort_keys`), des instants sans décimale ou à une seule, une ligne
///   finie par `\r\n` (fichier ouvert en mode texte sous Windows) ;
/// - TypeScript, `JSON.stringify` : compact, UTF-8 brut, dont U+0085, U+2028 et U+2029 (que `splitlines()` de
///   Python couperait en lignes), les clés dans un autre ordre, des instants plus anciens que ceux de Python ;
/// - passerelle, dans la forme du jour de la fusion : `bascule.publiee` et `bascule.retour` aplatis, `de_modele` ou
///   tous les `de_…` à `null`, sa séance et son `seq` à elle, l'origine `humain`, `origine_foi: declare`.
const C: &str = "1761901300000_9_0";
const SEANCE_PYTHON: &str = "1761901300100_41_1";
const SEANCE_TYPESCRIPT: &str = "1761901300200_52_1";
const SEANCE_PASSERELLE: &str = "1761901300300_63_1";
const AUTRE_PROGRAMME: &[&str] = &[
    // Python, seq 1 : la vague s'ouvre.
    concat!(
        r#"{"v": 2, "seance": "1761901300100_41_1", "seq": 1, "instant": "2026-10-31T21:03:"#,
        r#"00.123Z", "origine": "humain", "origine_foi": "declare", "type": "vague.ouverte""#,
        r#", "vague": "1761901300000_9_0", "n": 2, "depot": "/srv/d\u00e9p\u00f4t", "verifi"#,
        r#"er": "cargo test", "objectif": "\u00c9crire bonjour.txt \ud83d\ude00"}"#,
    ),
    // Python, seq 2, les clés triées : `v` n'est pas la première.
    concat!(
        r#"{"instant": "2026-10-31T21:03:00.150Z", "main": 1, "origine": "humain", "origine"#,
        r#"_foi": "declare", "seance": "1761901300100_41_1", "seq": 2, "tentative": 1, "typ"#,
        r#"e": "main.debut", "v": 2, "vague": "1761901300000_9_0"}"#,
    ),
    // Passerelle, seq 1 de sa séance : `bascule.publiee`, `de_modele` à null.
    concat!(
        r#"{"v":2,"seance":"1761901300300_63_1","seq":1,"instant":"2026-10-31T21:03:00.200Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"bascule.publiee","de_compte"#,
        r#"":"compte-a","de_fournisseur":"anthropic","de_modele":null,"vers_compte":"compte"#,
        r#"-b","vers_fournisseur":"anthropic","vers_modele":"claude-opus"}"#,
    ),
    // TypeScript, seq 1, le corps d'abord et l'enveloppe à l'envers ; son instant précède celui de Python.
    concat!(
        r#"{"vague":"1761901300000_9_0","main":2,"tentative":1,"type":"main.debut","origine"#,
        r#"_foi":"declare","origine":"humain","instant":"2026-10-31T21:02:59.987Z","seq":1,"#,
        r#""seance":"1761901300200_52_1","v":2}"#,
    ),
    // TypeScript, seq 2 : UTF-8 brut, avec ses séparateurs de ligne Unicode.
    concat!(
        concat!(
            r#"{"v":2,"seance":"1761901300200_52_1","seq":2,"instant":"2026-10-31T21:03:36.000Z"#,
            r#"","origine":"humain","origine_foi":"declare","type":"main.fin","vague":"17619013"#,
            r#"00000_9_0","main":1,"tentative":1,"couleur":"vert","foi":"constate","texte":"bon"#,
            r#"jour.txt écrit"#,
        ),
        "\u{85}\u{2028}\u{2029}",
        r#" ✓ 😀","verif":"test result: ok. 1 passed"}"#,
    ),
    concat!(
        r#"{"v":2,"seance":"1761901300200_52_1","seq":3,"instant":"2026-10-31T21:03:36.001Z"#,
        r#"","origine":"humain","origine_foi":"declare","type":"main.verdict","vague":"1761"#,
        r#"901300000_9_0","main":1,"verdict":"prise","foi":"constate"}"#,
    ),
    // Python, seq 3 : la main 2 est rouge ; accents échappés, ligne en CRLF.
    concat!(
        concat!(
            r#"{"v": 2, "seance": "1761901300100_41_1", "seq": 3, "instant": "2026-10-31T21:03:"#,
            r#"50.000Z", "origine": "humain", "origine_foi": "declare", "type": "main.fin", "va"#,
            r#"gue": "1761901300000_9_0", "main": 2, "tentative": 1, "couleur": "rouge", "foi":"#,
            r#" "constate", "texte": "v\u00e9rification \u00ab cargo test \u00bb rouge (exit co"#,
            r#"de: 101) :\ntest salut ... FAILED", "verif": "test salut ... FAILED"}"#,
        ),
        "\r",
    ),
    // Passerelle, seq 2 : `bascule.retour`, les clés triées, tous les `de_…` à null (`de` manque).
    concat!(
        r#"{"de_compte":null,"de_fournisseur":null,"de_modele":null,"instant":"2026-10-31T2"#,
        r#"1:03:51Z","origine":"humain","origine_foi":"declare","seance":"1761901300300_63_"#,
        r#"1","seq":2,"type":"bascule.retour","v":2,"vers_compte":"compte-a","vers_fourniss"#,
        r#"eur":"anthropic","vers_modele":"claude-sonnet"}"#,
    ),
    // Python, seq 4 à 6 : la relance de la main 2, dont la fin déclarée porte l'origine de sa main.
    concat!(
        r#"{"v": 2, "seance": "1761901300100_41_1", "seq": 4, "instant": "2026-10-31T21:03:"#,
        r#"52Z", "origine": "humain", "origine_foi": "declare", "type": "main.debut", "vagu"#,
        r#"e": "1761901300000_9_0", "main": 2, "tentative": 2}"#,
    ),
    concat!(
        r#"{"v": 2, "seance": "1761901300100_41_1", "seq": 5, "instant": "2026-10-31T21:04:"#,
        r#"20.5Z", "origine": "agent:main-1761901300000_9_0-2", "origine_foi": "declare", ""#,
        r#"type": "main.fin", "vague": "1761901300000_9_0", "main": 2, "tentative": 2, "cou"#,
        r#"leur": "vert", "foi": "declare", "texte": "salut.txt \u00e9crit\nPREUVE: vert", "#,
        r#""verif": null}"#,
    ),
    concat!(
        r#"{"v": 2, "seance": "1761901300100_41_1", "seq": 6, "instant": "2026-10-31T21:04:"#,
        r#"20.501Z", "origine": "humain", "origine_foi": "declare", "type": "main.verdict","#,
        r#" "vague": "1761901300000_9_0", "main": 2, "verdict": "prise", "foi": "declare"}"#,
    ),
];

#[test]
fn un_fil_ecrit_par_un_autre_programme_se_lit() {
    let dir = dossier("autre");
    poser(&dir, AUTRE_PROGRAMME);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    // Aucun écart, alors que trois séances se mêlent dans le fichier : ni ligne sautée, ni trou, ni redite.
    assert!(ecarts.is_empty(), "{ecarts:?}");
    // Les neuf lignes de Kraken se lisent dans l'ordre du fichier, pas des instants (celle de TypeScript est plus
    // ancienne que les deux de Python qui la précèdent), chacune avec l'instant et l'origine qu'on y a écrits.
    // Les deux lignes de passerelle, d'un type inconnu, sont ignorées au rejeu.
    let (py, ts, agent) = (SEANCE_PYTHON, SEANCE_TYPESCRIPT, format!("agent:main-{C}-2"));
    let enveloppes: Vec<_> =
        notes.iter().map(|n| (n.seance.as_str(), n.seq, n.instant.as_str(), n.origine.as_str())).collect();
    let attendu = [
        (py, 1, "2026-10-31T21:03:00.123Z", "humain"),
        (py, 2, "2026-10-31T21:03:00.150Z", "humain"),
        (ts, 1, "2026-10-31T21:02:59.987Z", "humain"),
        (ts, 2, "2026-10-31T21:03:36.000Z", "humain"),
        (ts, 3, "2026-10-31T21:03:36.001Z", "humain"),
        (py, 3, "2026-10-31T21:03:50.000Z", "humain"),
        (py, 4, "2026-10-31T21:03:52Z", "humain"),
        (py, 5, "2026-10-31T21:04:20.5Z", agent.as_str()),
        (py, 6, "2026-10-31T21:04:20.501Z", "humain"),
    ];
    assert_eq!(enveloppes, attendu);
    // Toute origine est déclarée, des autres comme de Kraken.
    assert!(notes.iter().all(|note| note.origine_foi == Foi::Declare));
    // Les événements, accents et séparateurs compris : échappés ou bruts, ils se lisent pareil.
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    let vert = "bonjour.txt écrit\u{85}\u{2028}\u{2029} ✓ 😀";
    let rouge = "vérification « cargo test » rouge (exit code: 101) :\ntest salut ... FAILED";
    let commence = |main, tentative| Evenement::Debut { vague: C.into(), main, tentative };
    let juge = |main, verdict, foi| Evenement::Verdict { vague: C.into(), main, verdict, foi };
    let fin = |main, tentative, couleur, foi, texte: &str, verif: Option<&str>| Evenement::Fin {
        vague: C.into(),
        main,
        tentative,
        couleur,
        foi,
        texte: texte.into(),
        verif: verif.map(str::to_string),
    };
    let attendus = [
        Evenement::Vague {
            vague: C.into(),
            n: 2,
            depot: "/srv/dépôt".into(),
            verifier: Some("cargo test".into()),
            objectif: "Écrire bonjour.txt 😀".into(),
        },
        commence(1, 1),
        commence(2, 1),
        fin(1, 1, Couleur::Vert, Foi::Constate, vert, Some("test result: ok. 1 passed")),
        juge(1, Verdict::Prise, Foi::Constate),
        fin(2, 1, Couleur::Rouge, Foi::Constate, rouge, Some("test salut ... FAILED")),
        commence(2, 2),
        fin(2, 2, Couleur::Vert, Foi::Declare, "salut.txt écrit\nPREUVE: vert", None),
        juge(2, Verdict::Prise, Foi::Declare),
    ];
    assert_eq!(evenements, attendus);
    // Et elles se rejouent comme celles de Kraken : une vague, la main 1 prise et constatée, la main 2 relancée
    // puis prise sur sa seule déclaration. Les champs se lisent par projections : une pièce qui ajoute un champ à
    // `Vague`, à `Main` ou à `Tentative` ne change pas ce test. Aucune de ces lignes n'est d'une vague à plan.
    let vagues = etat(&evenements);
    let [vague] = vagues.as_slice() else { panic!("une vague : {vagues:?}") };
    let ouverture =
        (vague.id.as_str(), vague.n, vague.depot.as_str(), vague.verifier.as_deref(), vague.objectif.as_str());
    assert_eq!(ouverture, (C, 2, "/srv/dépôt", Some("cargo test"), "Écrire bonjour.txt 😀"));
    assert_eq!(vague.mains.keys().collect::<Vec<_>>(), [&1, &2]);
    let projection = |n: usize| {
        let main = &vague.mains[&n];
        let tentatives = main.tentatives.iter();
        let tentatives = tentatives.map(|(t, lue)| (*t, lue.couleur, lue.texte.as_str(), lue.verif.as_deref()));
        (main.verdict, tentatives.collect::<Vec<_>>())
    };
    let (vert_constate, rouge_constate) = (Some((Couleur::Vert, Foi::Constate)), Some((Couleur::Rouge, Foi::Constate)));
    assert_eq!(
        projection(1),
        (Some((Verdict::Prise, Foi::Constate)), vec![(1, vert_constate, vert, Some("test result: ok. 1 passed"))])
    );
    assert_eq!(
        projection(2),
        (
            Some((Verdict::Prise, Foi::Declare)),
            vec![
                (1, rouge_constate, rouge, Some("test salut ... FAILED")),
                (2, Some((Couleur::Vert, Foi::Declare)), "salut.txt écrit\nPREUVE: vert", None),
            ]
        )
    );
    assert_eq!(vague.assemblage, None);
    for main in vague.mains.values() {
        assert_eq!((&main.tache, &main.echeances, &main.issue), (&None, &BTreeMap::new(), &None));
    }
    // Passerelle compte dans sa séance, ignorée ou non : sans sa première ligne, la seconde est un trou, dit à sa
    // place.
    let sans_publiee: Vec<&str> =
        AUTRE_PROGRAMME.iter().copied().filter(|ligne| !ligne.contains(r#""type":"bascule.publiee""#)).collect();
    let retour = sans_publiee.iter().position(|ligne| ligne.contains(r#""type":"bascule.retour""#)).unwrap() + 1;
    let trou = dossier("autre-trou");
    poser(&trou, &sans_publiee);
    let Lecture { notes, ecarts } = lire(&trou).unwrap();
    assert_eq!((notes.len(), codes(&ecarts)), (9, vec![(retour, 1010)]));
    assert!(ecarts[0].1.preuve.ends_with("seq 2 en tête de sa séance, où 1 est attendu"), "{}", ecarts[0].1);
    for dir in [dir, trou] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn kraken_ecrit_a_la_suite_d_un_fil_ecrit_par_un_autre_programme() {
    let dir = dossier("autre-suite");
    poser(&dir, AUTRE_PROGRAMME);
    let avant = fs::read(dir.join(FICHIER)).unwrap();
    // Le lancement d'après est une séance neuve : elle numérote depuis 1, sans rien savoir des trois autres.
    let kraken = Fil::seance(&dir, "humain");
    let classee = Evenement::Verdict { vague: C.into(), main: 2, verdict: Verdict::Classee, foi: Foi::Constate };
    kraken.noter(&classee).unwrap();
    // En ajout seul : ce que les autres ont écrit n'a pas bougé d'un octet, la ligne en CRLF comprise.
    let apres = fs::read(dir.join(FICHIER)).unwrap();
    assert!(apres.starts_with(&avant) && apres.len() > avant.len());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(notes.len(), 10);
    let derniere = notes.last().unwrap();
    assert_eq!((derniere.seq, derniere.origine.as_str(), &derniere.evenement), (1, "humain", &classee));
    let autres = [SEANCE_PYTHON, SEANCE_TYPESCRIPT, SEANCE_PASSERELLE];
    assert!(!autres.contains(&derniere.seance.as_str()), "une séance neuve : {}", derniere.seance);
    // Le fil raconte la suite : la classée, venue la dernière, remplace la prise déclarée de la main 2.
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(etat(&evenements)[0].mains[&2].verdict, Some((Verdict::Classee, Foi::Constate)));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_journal_de_passerelle_n_est_ni_lu_ni_recopie() {
    let dir = dossier("journal");
    // Les journaux d'aujourd'hui de passerelle, à côté du fil : `comptes.jsonl` porte son `v` à lui, à 1, et
    // `gestes.jsonl` imbrique. Ils se fondront dans le fil après la bascule, au choix de l'humain, pas avant.
    let comptes = r#"{"v":1,"compte":"compte-a","fournisseur":"anthropic"}"#;
    let gestes =
        r#"{"instant":"2026-10-31T21:03:00.123Z","type":"bascule.publiee","de":{"compte":"a"},"vers":{"compte":"b"}}"#;
    // Lus comme un fil, le premier l'arrêterait (1002), le second serait une ligne mal formée (1009).
    let temoin = dossier("journal-temoin");
    poser(&temoin, &[comptes]);
    assert_eq!(lire(&temoin).unwrap_err().code, Code::VersionDifferente);
    poser(&temoin, &[gestes]);
    assert_eq!(codes(&lire(&temoin).unwrap().ecarts), [(1, 1009)]);
    // Posés à côté du fil, ni la lecture ni l'écriture n'y touchent : le fil reste vide, puis ne porte que Kraken.
    for (nom, ligne) in [("comptes.jsonl", comptes), ("gestes.jsonl", gestes)] {
        fs::write(dir.join(nom), format!("{ligne}\n")).unwrap();
    }
    assert_eq!(lire(&dir).unwrap(), Lecture::default());
    Fil::seance(&dir, "humain").noter(&Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 }).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (1, 0));
    assert_eq!(fs::read_to_string(dir.join(FICHIER)).unwrap().lines().count(), 1, "rien n'est recopié au fil");
    assert_eq!(fs::read_to_string(dir.join("comptes.jsonl")).unwrap(), format!("{comptes}\n"));
    assert_eq!(fs::read_to_string(dir.join("gestes.jsonl")).unwrap(), format!("{gestes}\n"));
    for dir in [dir, temoin] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn un_type_inconnu_est_ignore() {
    let dir = dossier("inconnu");
    poser(
        &dir,
        &[
            &debut("s", 1),
            // Deux types d'autres pièces : leurs champs sont les leurs, leur enveloppe se lit, leur seq compte.
            concat!(
                r#"{"v":2,"seance":"s","seq":2,"instant":"2026-10-31T21:03:01Z","origine":"humain","#,
                r#""origine_foi":"declare","type":"ci.fin","couleur":"vert","essais":3,"rouge":fals"#,
                r#"e}"#,
            ),
            concat!(
                r#"{"v":2,"seance":"s","seq":3,"instant":"2026-10-31T21:03:02.5Z","origine":"humain"#,
                r#"","origine_foi":"declare","type":"bascule.publiee","de_compte":"a","de_modele":n"#,
                r#"ull}"#,
            ),
            // L'ancien format, où le type était un id : un type inconnu de plus, sans corps.
            &debut("s", 4).replace(
                r#""type":"main.debut","vague":"v","main":1,"tentative":1"#,
                r#""type":"main-1761901200000-1""#,
            ),
            &debut("s", 5),
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "aucun trou, aucun écart : {ecarts:?}");
    assert_eq!(seqs(&notes), [1, 5]);
    let evenements: Vec<_> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(etat(&evenements)[0].mains.len(), 1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_valeur_imbriquee_en_v2_est_mal_formee_sans_faux_trou() {
    let dir = dossier("imbriquee");
    poser(
        &dir,
        &[
            &debut("s", 1),
            // Un type connu, un champ imbriqué : mal formée, et elle compte dans sa séance.
            &debut("s", 2).replace(r#""tentative":1"#, r#""tentative":[1]"#),
            // Un type inconnu, imbriqué comme passerelle l'écrit aujourd'hui : mal formée aussi.
            concat!(
                r#"{"v":2,"seance":"s","seq":3,"instant":"2026-10-31T21:03:00Z","origine":"humain","#,
                r#""origine_foi":"declare","type":"flux.entree","entree":{"porte":{"id":1}}}"#,
            ),
            // L'enveloppe elle-même : l'origine est un objet.
            &debut("s", 4).replace(r#""origine":"humain""#, r#""origine":{"agent":"main-v-1"}"#),
            // Un nombre qui n'est pas un entier positif.
            &debut("s", 5).replace(r#""main":1"#, r#""main":1.0"#),
            &debut("s", 6).replace(r#""main":1"#, r#""main":-1"#),
            &debut("s", 7),
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, 7]);
    assert_eq!(codes(&ecarts), [(2, 1009), (3, 1009), (4, 1009), (5, 1009), (6, 1009)], "et aucun 1010");
    assert!(ecarts[1].1.preuve.contains("« entree » n'est pas une valeur plate"), "{}", ecarts[1].1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn n_main_et_tentative_vont_de_1_a_la_borne() {
    // La borne du contrat, écrite ici à la main : la changer change le contrat, et l'en-tête avec lui.
    const BORNE: usize = 10_000;
    let dir = dossier("borne");
    let ouverture = |seq: u64, n: &str| {
        format!(
            concat!(
                r#"{{"v":2,"seance":"s","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","origine":"#,
                r#""humain","origine_foi":"declare","type":"vague.ouverte","vague":"v","n":{n},"dep"#,
                r#"ot":"/srv/depot","verifier":null,"objectif":"o"}}"#,
            ),
            seq = seq,
            n = n,
        )
    };
    let ligne = |cle: &str, seq: u64, valeur: &str| match cle {
        "n" => ouverture(seq, valeur),
        "main" => debut("s", seq).replace(r#""main":1"#, &format!(r#""main":{valeur}"#)),
        _ => debut("s", seq).replace(r#""tentative":1"#, &format!(r#""tentative":{valeur}"#)),
    };
    let (borne, apres, enorme) = (BORNE.to_string(), (BORNE + 1).to_string(), u64::MAX.to_string());
    // Pour chacun des trois : 1 et la borne se lisent ; 0, la borne plus un et le plus grand u64 sont hors du domaine.
    let (mut lignes, mut mal_formees) = (Vec::new(), Vec::new());
    for cle in ["n", "main", "tentative"] {
        let valeurs =
            [("1", true), (borne.as_str(), true), ("0", false), (apres.as_str(), false), (enorme.as_str(), false)];
        for (valeur, lue) in valeurs {
            lignes.push(ligne(cle, lignes.len() as u64 + 1, valeur));
            if !lue {
                mal_formees.push((lignes.len(), 1009));
            }
        }
    }
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, 2, 6, 7, 11, 12]);
    // Une ligne refusée compte dans sa séance : aucun 1010.
    assert_eq!(codes(&ecarts), mal_formees);
    for (i, valeur) in [(0, "0"), (1, apres.as_str()), (2, enorme.as_str())] {
        let preuve = &ecarts[i].1.preuve;
        let attendue = format!("« n » vaut {valeur}, hors de son domaine : un entier de 1 à 10000");
        assert!(preuve.ends_with(&attendue), "{preuve}");
    }
    let commence = |main, tentative| Evenement::Debut { vague: "v".into(), main, tentative };
    assert!(matches!(notes[1].evenement, Evenement::Vague { n: BORNE, .. }));
    assert_eq!(notes[3].evenement, commence(BORNE, 1));
    assert_eq!(notes[5].evenement, commence(1, BORNE));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_ligne_illisible_est_sautee_et_nommee() {
    let dir = dossier("illisible");
    // De l'UTF-8 coupé : le premier octet de « é », seul.
    let mut coupe = debut("s", 2).replace(r#""vague":"v""#, r#""vague":"é""#).into_bytes();
    coupe.remove(coupe.iter().position(|&octet| octet == 0xA9).unwrap());
    let verdict = concat!(
        r#"{"v":2,"seance":"s","seq":4,"instant":"2026-10-31T21:03:00.123Z","origine":"huma"#,
        r#"in","origine_foi":"declare","type":"main.verdict","vague":"v","main":1,"verdict""#,
        r#":"bleu","foi":"constate"}"#,
    );
    let fin = concat!(
        r#"{"v":2,"seance":"s","seq":6,"instant":"2026-10-31T21:03:00.123Z","origine":"huma"#,
        r#"in","origine_foi":"declare","type":"main.fin","vague":"v","main":1,"tentative":1"#,
        r#","couleur":"non prouvé","foi":"constate","texte":"PREUVE: vert","verif":null}"#,
    );
    let lignes: [Vec<u8>; 13] = [
        debut("s", 1).into_bytes(),
        coupe,
        // Illisibles, 1001 : leur seq ne se lit pas, et ne compte pas.
        br#"{"v":2,"seance":"s","seq":2"#.to_vec(),
        debut("s", 2).replace(r#""vague":"v""#, r#""vague":"\x""#).into_bytes(),
        b"x".to_vec(),
        // Mal formées, 1009, avant `seance` et `seq` : `v` manque, ou n'est pas un entier.
        debut("s", 2).replace(r#""v":2,"#, "").into_bytes(),
        debut("s", 2).replace(r#""v":2"#, r#""v":"2""#).into_bytes(),
        // Mal formées, 1009, et comptées dans leur séance : un champ manque, un champ de trop, un verdict inconnu,
        // un entier écrit en chaîne, une couleur de M1.
        debut("s", 2).replace(r#","tentative":1"#, "").into_bytes(),
        debut("s", 3).replace(r#""tentative":1"#, r#""tentative":1,"essai":2"#).into_bytes(),
        verdict.as_bytes().to_vec(),
        debut("s", 5).replace(r#""main":1"#, r#""main":"1""#).into_bytes(),
        fin.as_bytes().to_vec(),
        debut("s", 7).into_bytes(),
    ];
    let mut octets: Vec<u8> = lignes.into_iter().flat_map(|ligne| ligne.into_iter().chain(*b"\n")).collect();
    octets.extend_from_slice(debut("s", 8).as_bytes()); // en cours d'écriture : pas encore de `\n`
    fs::write(dir.join(FICHIER), octets).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, 7]);
    let illisibles = (2..=5).map(|ligne| (ligne, 1001));
    assert_eq!(codes(&ecarts), illisibles.chain((6..=12).map(|ligne| (ligne, 1009))).collect::<Vec<_>>());
    let fil = dir.join(FICHIER).display().to_string();
    for (ligne, refus) in &ecarts {
        assert!(refus.preuve.starts_with(&format!("{fil}, ligne {ligne} : ")), "la preuve nomme sa ligne : {refus}");
    }
    assert!(ecarts[4].1.preuve.ends_with("« v » manque ou n'est pas un entier"), "{}", ecarts[4].1);
    assert!(ecarts[6].1.preuve.contains("« tentative » manque"), "{}", ecarts[6].1);
    assert!(ecarts[7].1.preuve.contains("champ en trop pour main.debut : « essai »"), "{}", ecarts[7].1);
    assert!(ecarts[8].1.preuve.contains(r#"« verdict » vaut "bleu", hors de son domaine : prise ou classee"#));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_ligne_vide_est_sautee_sans_etre_illisible() {
    let dir = dossier("vide");
    // Avant, entre et après les autres : elles comptent dans les numéros, sans être nommées.
    poser(&dir, &["", &debut("s", 1), "", "", "x", &debut("s", 2), ""]);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, 2]);
    assert_eq!(codes(&ecarts), [(5, 1001)]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_autre_version_arrete_la_lecture() {
    let dir = dossier("version");
    // `v` se lit avant tout le reste : une ligne v3 qui imbrique est une autre version, pas une ligne mal formée.
    let autres = [
        (r#"{"v":3,"seance":"s","seq":2,"type":"main.debut"}"#, 3),
        (r#"{"seq":2,"liste":[1,{"v":2}],"objet":{"a":{}},"v":3}"#, 3),
        (r#"{"v":1}"#, 1),
        (r#"{"v":0,"seance":"s"}"#, 0),
    ];
    for (autre, v) in autres {
        poser(&dir, &[&debut("s", 1), autre, &debut("s", 2), "illisible"]);
        let refus = lire(&dir).unwrap_err();
        assert_eq!(refus.code, Code::VersionDifferente, "{autre}");
        let preuve = format!("{}, ligne 2 : v vaut {v}, et ce lecteur ne lit que v 2", dir.join(FICHIER).display());
        assert_eq!(refus.preuve, preuve);
        assert!(refus.correction.contains(&format!("v {v}")), "{refus}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_sequence_rompue_est_dite() {
    let dir = dossier("sequence");
    poser(
        &dir,
        &[
            &debut("s", 1),
            &debut("s", 2),
            &debut("s", 4), // un trou : 3 manque
            &debut("s", 4), // une redite
            &debut("s", 5),
            &debut("t", 2), // une séance qui ne commence pas à 1
            "illisible",    // une ligne illisible : son seq se perd...
            &debut("t", 4), // ... et le trou qu'elle laisse est dit aussi
            &debut("u", 1),
            &debut("u", u64::MAX),
            &debut("u", u64::MAX), // une redite au bout des entiers, sans débordement
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    // Rien n'est sauté pour un seq rompu : l'écart est dit, et la ligne lue.
    assert_eq!(seqs(&notes), [1, 2, 4, 4, 5, 2, 4, 1, u64::MAX, u64::MAX]);
    assert_eq!(codes(&ecarts), [(3, 1010), (4, 1010), (6, 1010), (7, 1001), (8, 1010), (10, 1010), (11, 1010)]);
    let preuves: Vec<&str> = ecarts.iter().map(|(_, refus)| refus.preuve.rsplit(" : ").next().unwrap()).collect();
    let max = u64::MAX;
    assert_eq!(
        preuves,
        [
            "seq 4 après le seq 2",
            "seq 4 après le seq 4",
            "seq 2 en tête de sa séance, où 1 est attendu",
            "at character 1: expected an object `{`, saw 'i'",
            "seq 4 après le seq 2",
            &format!("seq {max} après le seq 1"),
            &format!("seq {max} après le seq {max}"),
        ]
    );
    assert!(ecarts[0].1.preuve.contains(r#"séance "s" : seq 4"#), "{}", ecarts[0].1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn chaque_type_fait_l_aller_retour() {
    // Chaque type de Kraken, dans chacune de ses formes, et ce qu'il écrit après son enveloppe. Une pièce qui
    // ajoute un type ajoute ici son exemple.
    let fin = |couleur, foi, texte: &str, verif: Option<&str>| Evenement::Fin {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        couleur,
        foi,
        texte: texte.into(),
        verif: verif.map(str::to_string),
    };
    let verdict = |verdict, foi| Evenement::Verdict { vague: "v".into(), main: 2, verdict, foi };
    let lance = |mur, reglages_depot: &str| Evenement::Lancement {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        profil: "claude".into(),
        empreinte: EMPREINTE.into(),
        modele: "sonnet".into(),
        effort: None,
        mur,
        reglages: r#"{"disableAllHooks":true}"#.into(),
        reglages_depot: reglages_depot.into(),
    };
    let constate = |version: Option<&str>, mur_actif| Evenement::Constat {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        version: version.map(str::to_string),
        mur_actif,
    };
    let deux = format!(".claude/settings.json {EMPREINTE}\n.claude/settings.local.json {EMPREINTE}");
    let ouverture = |verifier: Option<&str>| Evenement::Vague {
        vague: "v".into(),
        n: 3,
        depot: "/srv/depot".into(),
        verifier: verifier.map(str::to_string),
        objectif: "Écrire bonjour.txt".into(),
    };
    let sha = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let tache = |depend: &[&str]| Evenement::Tache {
        vague: "v".into(),
        main: 2,
        id: "b".into(),
        depend: depend.iter().map(|id| id.to_string()).collect(),
        preuve: "cargo test".into(),
        objectif: "Écrire b.txt".into(),
    };
    let perimee = |foi, verif: Option<&str>| Evenement::Perimee {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        couleur: Couleur::Vert,
        foi,
        texte: "arrivé trop tard".into(),
        verif: verif.map(str::to_string),
    };
    let refus = |verif: Option<&str>| Evenement::Refus {
        vague: "v".into(),
        main: 2,
        code: 7008,
        regle: "La preuve de la vague passe sur la tête après chaque fusion.".into(),
        preuve: "vérification « cargo test » rouge".into(),
        correction: "Corrige la tâche, puis relance.".into(),
        verif: verif.map(str::to_string),
    };
    let poussee = Evenement::Poussee { vague: "v".into(), branche: "kraken/vague-v".into(), sha: sha.into() };
    let gel = Evenement::Gel { vague: "v".into(), main: 2, sha: sha.into() };
    let pr = |foi, numero: u64| Evenement::Pr {
        vague: "v".into(),
        branche: "kraken/vague-v".into(),
        base: "main".into(),
        sha: sha.into(),
        numero,
        url: "https://github.com/exemple/depot/pull/12".into(),
        foi,
    };
    let ci = |couleur, foi, texte: &str| Evenement::Ci {
        vague: "v".into(),
        numero: 12,
        sha: sha.into(),
        couleur,
        texte: texte.into(),
        foi,
    };
    let refus_de_forge = Evenement::RefusForge {
        vague: "v".into(),
        code: 8005,
        regle: "Kraken ne publie ni un changement de `.github/`, ni un chemin du poste, ni un commit qu'il n'a pas \
                 fait ; publier ceux-là revient à l'humain."
            .into(),
        preuve: ".github/workflows/ci.yml change entre la base et la tête".into(),
        correction: "Pousse la branche toi-même, après relecture.".into(),
    };
    let rangement = |retires: &[&str]| Evenement::Rangement {
        vague: "v".into(),
        retires: retires.iter().map(|retire| retire.to_string()).collect(),
    };
    let decoupage = |taches, plan: &str| Evenement::Decoupage {
        vague: "d".into(),
        taches,
        empreinte: EMPREINTE.into(),
        plan: plan.into(),
    };
    let joue = Evenement::Joue { vague: "v".into(), decoupage: "d".into() };
    let relecture = |n, bloquantes, remarques: &str| Evenement::Relecture {
        vague: "r".into(),
        relue: "v".into(),
        n,
        bloquantes,
        remarques: remarques.into(),
    };
    let correction = Evenement::Correction { vague: "c".into(), corrige: "v".into() };
    let relecteur = Evenement::Relecteur { vague: "r".into(), relue: "v".into() };
    let jugement = |profil: Option<&str>| Evenement::VerdictRelecture {
        vague: "r".into(),
        relue: "v".into(),
        sha: "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34".into(),
        profil: profil.map(Into::into),
        empreinte: profil.map(|_| EMPREINTE.into()),
        modele: profil.map(|_| "sonnet".into()),
    };
    let fournisseur =
        |hote: &str| Evenement::Fournisseur { vague: "v".into(), main: 2, tentative: 1, fournisseur: hote.into() };
    let usage = |modele: &str, micro_dollars, base| Evenement::Usage {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        modele: modele.into(),
        entree: 12_000,
        sortie: 800,
        cache_ecrit: 5,
        cache_lu: 40_000,
        reflexion: 300,
        micro_dollars,
        base,
    };
    let consigne = |role, systeme: Option<&str>, lecons: &str, ecartees: &str| Evenement::Consigne {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        role,
        consigne: EMPREINTE.into(),
        systeme: systeme.map(Into::into),
        lecons: lecons.into(),
        ecartees: ecartees.into(),
        mesure: String::new(),
    };
    let exemples = [
        (
            consigne(Role::Main, Some(EMPREINTE), &format!("a {EMPREINTE}\nb {EMPREINTE}"), "c"),
            concat!(
                r#""type":"main.consigne","vague":"v","main":2,"tentative":1,"role":"main","consigne":"e3b0c44"#,
                r#"298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","systeme":"e3b0c44298fc1c149afb"#,
                r#"f4c8996fb92427ae41e4649b934ca495991b7852b855","lecons":"a e3b0c44298fc1c149afbf4c8996fb924"#,
                r#"27ae41e4649b934ca495991b7852b855\nb e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca49599"#,
                r#"1b7852b855","ecartees":"c","mesure":"""#,
            ),
        ),
        (
            consigne(Role::Relance, None, "", ""),
            concat!(
                r#""type":"main.consigne","vague":"v","main":2,"tentative":1,"role":"relance","consigne":"e3b0c"#,
                r#"44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","systeme":null,"lecons":"","e"#,
                r#"cartees":"","mesure":"""#,
            ),
        ),
        (
            usage("claude-haiku-5-5", 2500, Base::Forfait),
            concat!(
                r#""type":"main.usage","vague":"v","main":2,"tentative":1,"modele":"claude-haiku-5-5","entree":1"#,
                r#"2000,"sortie":800,"cache_ecrit":5,"cache_lu":40000,"reflexion":300,"micro_dollars":2500,"#,
                r#""base":"forfait","foi":"declare""#,
            ),
        ),
        (
            usage("deepseek-flash", 8460, Base::Estime),
            concat!(
                r#""type":"main.usage","vague":"v","main":2,"tentative":1,"modele":"deepseek-flash","entree":1"#,
                r#"2000,"sortie":800,"cache_ecrit":5,"cache_lu":40000,"reflexion":300,"micro_dollars":8460,"#,
                r#""base":"estime","foi":"declare""#,
            ),
        ),
        (
            usage("inconnu-1", 0, Base::Inconnu),
            concat!(
                r#""type":"main.usage","vague":"v","main":2,"tentative":1,"modele":"inconnu-1","entree":1"#,
                r#"2000,"sortie":800,"cache_ecrit":5,"cache_lu":40000,"reflexion":300,"micro_dollars":0,"#,
                r#""base":"inconnu","foi":"declare""#,
            ),
        ),
        (
            ouverture(Some("cargo test")),
            concat!(
                r#""type":"vague.ouverte","vague":"v","n":3,"depot":"/srv/depot","verifier":"cargo "#,
                r#"test","objectif":"Écrire bonjour.txt""#,
            ),
        ),
        (
            ouverture(None),
            concat!(
                r#""type":"vague.ouverte","vague":"v","n":3,"depot":"/srv/depot","verifier":null,"o"#,
                r#"bjectif":"Écrire bonjour.txt""#,
            ),
        ),
        (
            Evenement::Debut { vague: "v".into(), main: 2, tentative: 1 },
            r#""type":"main.debut","vague":"v","main":2,"tentative":1"#,
        ),
        (
            lance(Mur::Exige, ""),
            concat!(
                r#""type":"main.lancement","vague":"v","main":2,"tentative":1,"profil":"claude","em"#,
                r#"preinte":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","mod"#,
                r#"ele":"sonnet","mur":"exige","reglages":"{\"disableAllHooks\":true}","reglages_de"#,
                r#"pot":"","foi":"declare""#,
            ),
        ),
        (
            lance(Mur::Demande, &deux),
            concat!(
                r#""type":"main.lancement","vague":"v","main":2,"tentative":1,"profil":"claude","em"#,
                r#"preinte":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","mod"#,
                r#"ele":"sonnet","mur":"demande","reglages":"{\"disableAllHooks\":true}","reglages_"#,
                r#"depot":".claude/settings.json e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca4"#,
                r#"95991b7852b855\n.claude/settings.local.json e3b0c44298fc1c149afbf4c8996fb92427ae"#,
                r#"41e4649b934ca495991b7852b855","foi":"declare""#,
            ),
        ),
        // Un lancement qui déclare un effort : la clé suit `modele`, et manque aux deux d'avant.
        (
            Evenement::Lancement {
                vague: "v".into(),
                main: 2,
                tentative: 1,
                profil: "claude".into(),
                empreinte: EMPREINTE.into(),
                modele: "sonnet".into(),
                effort: Some("high".into()),
                mur: Mur::Exige,
                reglages: r#"{"disableAllHooks":true}"#.into(),
                reglages_depot: String::new(),
            },
            concat!(
                r#""type":"main.lancement","vague":"v","main":2,"tentative":1,"profil":"claude","em"#,
                r#"preinte":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","mod"#,
                r#"ele":"sonnet","effort":"high","mur":"exige","reglages":"{\"disableAllHooks\":tru"#,
                r#"e}","reglages_depot":"","foi":"declare""#,
            ),
        ),
        (
            constate(Some("2.1.312"), Some(true)),
            concat!(
                r#""type":"main.constat","vague":"v","main":2,"tentative":1,"version":"2.1.312","mu"#,
                r#"r_actif":true,"foi":"constate""#,
            ),
        ),
        (
            constate(None, Some(false)),
            concat!(
                r#""type":"main.constat","vague":"v","main":2,"tentative":1,"version":null,"mur_act"#,
                r#"if":false,"foi":"constate""#,
            ),
        ),
        (
            constate(Some("2.1.316"), None),
            concat!(
                r#""type":"main.constat","vague":"v","main":2,"tentative":1,"version":"2.1.316","mu"#,
                r#"r_actif":null,"foi":"constate""#,
            ),
        ),
        (
            fin(Couleur::Vert, Foi::Constate, "fait", Some("test result: ok")),
            concat!(
                r#""type":"main.fin","vague":"v","main":2,"tentative":1,"couleur":"vert","foi":"con"#,
                r#"state","texte":"fait","verif":"test result: ok""#,
            ),
        ),
        (
            fin(Couleur::Vert, Foi::Declare, "fait\nPREUVE: vert", None),
            concat!(
                r#""type":"main.fin","vague":"v","main":2,"tentative":1,"couleur":"vert","foi":"dec"#,
                r#"lare","texte":"fait\nPREUVE: vert","verif":null"#,
            ),
        ),
        (
            fin(Couleur::Rouge, Foi::Constate, "vérification « exit 3 » rouge", Some("")),
            concat!(
                r#""type":"main.fin","vague":"v","main":2,"tentative":1,"couleur":"rouge","foi":"co"#,
                r#"nstate","texte":"vérification « exit 3 » rouge","verif":"""#,
            ),
        ),
        (
            verdict(Verdict::Prise, Foi::Constate),
            r#""type":"main.verdict","vague":"v","main":2,"verdict":"prise","foi":"constate""#,
        ),
        (
            verdict(Verdict::Prise, Foi::Declare),
            r#""type":"main.verdict","vague":"v","main":2,"verdict":"prise","foi":"declare""#,
        ),
        (
            verdict(Verdict::Classee, Foi::Constate),
            r#""type":"main.verdict","vague":"v","main":2,"verdict":"classee","foi":"constate""#,
        ),
        (
            Evenement::Assemblage {
                vague: "v".into(),
                branche: "kraken/vague-v".into(),
                base: sha.into(),
                preuve: "cargo test".into(),
            },
            concat!(
                r#""type":"vague.assemblage","vague":"v","branche":"kraken/vague-v","base":"9f2c41a"#,
                r#"7d3b85e60c1a49d27b8e3f5a60d1c2b34","preuve":"cargo test""#,
            ),
        ),
        (
            tache(&[]),
            concat!(
                r#""type":"vague.tache","vague":"v","main":2,"id":"b","depend":"","preuve":"cargo t"#,
                r#"est","objectif":"Écrire b.txt""#,
            ),
        ),
        (
            tache(&["a", "c-3"]),
            concat!(
                r#""type":"vague.tache","vague":"v","main":2,"id":"b","depend":"a\nc-3","preuve":"c"#,
                r#"argo test","objectif":"Écrire b.txt""#,
            ),
        ),
        (
            Evenement::Bail { vague: "v".into(), main: 2, tentative: 1, echeance: "2026-10-31T22:09:00.123Z".into() },
            r#""type":"main.bail","vague":"v","main":2,"tentative":1,"echeance":"2026-10-31T22:09:00.123Z""#,
        ),
        (
            perimee(Foi::Declare, None),
            concat!(
                r#""type":"main.perimee","vague":"v","main":2,"tentative":1,"couleur":"vert","foi":"#,
                r#""declare","texte":"arrivé trop tard","verif":null"#,
            ),
        ),
        (
            perimee(Foi::Constate, Some("test result: ok")),
            concat!(
                r#""type":"main.perimee","vague":"v","main":2,"tentative":1,"couleur":"vert","foi":"#,
                r#""constate","texte":"arrivé trop tard","verif":"test result: ok""#,
            ),
        ),
        (
            Evenement::Fusion { vague: "v".into(), main: 2, sha: sha.into(), verif: "test result: ok".into() },
            concat!(
                r#""type":"vague.fusion","vague":"v","main":2,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5"#,
                r#"a60d1c2b34","verif":"test result: ok","foi":"constate""#,
            ),
        ),
        (
            refus(Some("test bonjour ... FAILED")),
            concat!(
                r#""type":"vague.refus","vague":"v","main":2,"code":7008,"regle":"La preuve de la v"#,
                r#"ague passe sur la tête après chaque fusion.","preuve":"vérification « cargo test"#,
                r#" » rouge","correction":"Corrige la tâche, puis relance.","verif":"test bonjour ."#,
                r#".. FAILED""#,
            ),
        ),
        (
            refus(None),
            concat!(
                r#""type":"vague.refus","vague":"v","main":2,"code":7008,"regle":"La preuve de la v"#,
                r#"ague passe sur la tête après chaque fusion.","preuve":"vérification « cargo test"#,
                r#" » rouge","correction":"Corrige la tâche, puis relance.","verif":null"#,
            ),
        ),
        (
            gel,
            concat!(
                r#""type":"main.gel","vague":"v","main":2,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a6"#,
                r#"0d1c2b34""#,
            ),
        ),
        (
            Evenement::Empreintes {
                vague: "v".into(),
                main: 2,
                sha: sha.into(),
                fichiers: vec!["src/a.rs 11".into(), "b.txt 22".into()],
            },
            concat!(
                r#""type":"main.empreintes","vague":"v","main":2,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a6"#,
                r#"0d1c2b34","fichiers":"src/a.rs 11\nb.txt 22""#,
            ),
        ),
        (
            poussee,
            concat!(
                r#""type":"forge.poussee","vague":"v","branche":"kraken/vague-v","sha":"9f2c41a7d3b"#,
                r#"85e60c1a49d27b8e3f5a60d1c2b34""#,
            ),
        ),
        (
            pr(Foi::Constate, 12),
            concat!(
                r#""type":"forge.pr","vague":"v","branche":"kraken/vague-v","base":"main","sha":"9f"#,
                r#"2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34","numero":12,"url":"https://github.com/ex"#,
                r#"emple/depot/pull/12","foi":"constate""#,
            ),
        ),
        (
            pr(Foi::Declare, 123_456),
            concat!(
                r#""type":"forge.pr","vague":"v","branche":"kraken/vague-v","base":"main","sha":"9f"#,
                r#"2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34","numero":123456,"url":"https://github.co"#,
                r#"m/exemple/depot/pull/12","foi":"declare""#,
            ),
        ),
        (
            ci(Couleur::Vert, Foi::Constate, "test (linux) : SUCCESS\ntest (windows) : SUCCESS"),
            concat!(
                r#""type":"forge.ci","vague":"v","numero":12,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a"#,
                r#"60d1c2b34","couleur":"vert","texte":"test (linux) : SUCCESS\ntest (windows) : SU"#,
                r#"CCESS","foi":"constate""#,
            ),
        ),
        (
            ci(Couleur::Rouge, Foi::Constate, "test (windows) : FAILURE\ntest bonjour ... FAILED\n"),
            concat!(
                r#""type":"forge.ci","vague":"v","numero":12,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a"#,
                r#"60d1c2b34","couleur":"rouge","texte":"test (windows) : FAILURE\ntest bonjour ..."#,
                r#" FAILED\n","foi":"constate""#,
            ),
        ),
        (
            ci(Couleur::Vert, Foi::Declare, ""),
            concat!(
                r#""type":"forge.ci","vague":"v","numero":12,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a"#,
                r#"60d1c2b34","couleur":"vert","texte":"","foi":"declare""#,
            ),
        ),
        (
            refus_de_forge,
            concat!(
                r#""type":"forge.refus","vague":"v","code":8005,"regle":"Kraken ne publie ni un cha"#,
                r#"ngement de `.github/`, ni un chemin du poste, ni un commit qu'il n'a pas fait ; "#,
                r#"publier ceux-là revient à l'humain.","preuve":".github/workflows/ci.yml change e"#,
                r#"ntre la base et la tête","correction":"Pousse la branche toi-même, après relectu"#,
                r#"re.""#,
            ),
        ),
        (rangement(&[]), r#""type":"forge.rangement","vague":"v","retires":"""#),
        (rangement(&["kraken/vague-v"]), r#""type":"forge.rangement","vague":"v","retires":"kraken/vague-v""#),
        (
            rangement(&["/srv/kraken/bacs/main-v-1", "/srv/kraken/bacs/a3abe0dfce03.tmp", "kraken/main-v-1"]),
            concat!(
                r#""type":"forge.rangement","vague":"v","retires":"/srv/kraken/bacs/main-v-1\n/srv/"#,
                r#"kraken/bacs/a3abe0dfce03.tmp\nkraken/main-v-1""#,
            ),
        ),
        (
            decoupage(2, "plan 1\nobjectif : Écrire a.txt\npreuve : cargo test\n\ntache a\n    Écrire a.txt"),
            concat!(
                r#""type":"vague.decoupage","vague":"d","taches":2,"empreinte":"e3b0c44298fc1c149af"#,
                r#"bf4c8996fb92427ae41e4649b934ca495991b7852b855","plan":"plan 1\nobjectif : Écrire"#,
                r#" a.txt\npreuve : cargo test\n\ntache a\n    Écrire a.txt""#,
            ),
        ),
        (
            decoupage(1, ""),
            concat!(
                r#""type":"vague.decoupage","vague":"d","taches":1,"empreinte":"e3b0c44298fc1c149af"#,
                r#"bf4c8996fb92427ae41e4649b934ca495991b7852b855","plan":"""#,
            ),
        ),
        (joue, r#""type":"vague.joue","vague":"v","decoupage":"d""#),
        (
            relecture(0, 0, "remarques 1"),
            r#""type":"vague.relecture","vague":"r","relue":"v","n":0,"bloquantes":0,"remarques":"remarques 1""#,
        ),
        (
            relecture(2, 1, "remarques 1\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    Reprendre « a »."),
            concat!(
                r#""type":"vague.relecture","vague":"r","relue":"v","n":2,"bloquantes":1,"remarques"#,
                r#"":"remarques 1\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    Reprendre « "#,
                r#"a ».""#,
            ),
        ),
        (correction, r#""type":"vague.correction","vague":"c","corrige":"v""#),
        (relecteur, r#""type":"vague.relecteur","vague":"r","relue":"v""#),
        (
            jugement(Some("claude")),
            concat!(
                r#""type":"vague.verdict","vague":"r","relue":"v","sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34","#,
                r#""profil":"claude","empreinte":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","#,
                r#""modele":"sonnet""#,
            ),
        ),
        (
            jugement(None),
            concat!(
                r#""type":"vague.verdict","vague":"r","relue":"v","sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34","#,
                r#""profil":null,"empreinte":null,"modele":null"#,
            ),
        ),
        (
            fournisseur("api.deepseek.com"),
            concat!(
                r#""type":"main.fournisseur","vague":"v","main":2,"tentative":1,"fournisseur":"api."#,
                r#"deepseek.com","foi":"declare""#,
            ),
        ),
        (
            fournisseur("localhost:8443"),
            concat!(
                r#""type":"main.fournisseur","vague":"v","main":2,"tentative":1,"fournisseur":"loca"#,
                r#"lhost:8443","foi":"declare""#,
            ),
        ),
        (
            Evenement::Coupure { vague: "v".into(), main: 2, tentative: 1, statut: 529 },
            r#""type":"main.coupure","vague":"v","main":2,"tentative":1,"statut":529"#,
        ),
        (
            Evenement::Coupure { vague: "v".into(), main: 2, tentative: 2, statut: 429 },
            r#""type":"main.coupure","vague":"v","main":2,"tentative":2,"statut":429"#,
        ),
    ];
    let dir = dossier("types");
    let fil = Fil::seance(&dir, "humain");
    for (evenement, _) in &exemples {
        fil.noter(evenement).unwrap();
    }
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert_eq!(brut.lines().count(), exemples.len());
    for ((_, corps), ligne) in exemples.iter().zip(brut.lines()) {
        assert!(ligne.starts_with(r#"{"v":2,"seance":""#), "`v` en premier, puis l'enveloppe : {ligne}");
        assert!(ligne.ends_with(&format!(r#","origine_foi":"declare",{corps}}}"#)), "{ligne}");
    }
    // L'en-tête dit chaque type, avec les champs de son corps dans l'ordre où il s'écrit. Ses lignes de doc se joignent
    // d'un blanc : un schéma trop long pour 120 caractères (`main.usage`) se coupe, entre deux champs.
    let en_tete: String = include_str!("../fil.rs")
        .lines()
        .map_while(|ligne| ligne.strip_prefix("//!"))
        .map(|ligne| format!("{} ", ligne.trim_start()))
        .collect();
    for (evenement, _) in &exemples {
        let cles: Vec<&str> = evenement.champs().iter().map(|(cle, _)| *cle).collect();
        let dit = format!("- `{} {{{}}}`", evenement.genre(), cles.join(", "));
        assert!(en_tete.contains(&dit), "l'en-tête dit {dit}");
    }
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(notes.into_iter().map(|note| note.evenement).collect::<Vec<_>>(), exemples.map(|(e, _)| e));
    // Clés et valeurs-jetons sont en ASCII, car un mot accentué peut s'écrire en NFC ou en NFD : seuls les textes
    // libres du corps portent des accents. Un jeton neuf qui s'accentuerait fait échouer ici, un texte libre neuf
    // s'ajoute à la liste.
    const LIBRES: [&str; 17] = [
        "vague",
        "depot",
        "verifier",
        "objectif",
        "texte",
        "verif",
        "preuve",
        "regle",
        "correction",
        "reglages",
        "reglages_depot",
        "version",
        "base",
        "url",
        "retires",
        "plan",
        "remarques",
    ];
    for ligne in brut.lines() {
        for (cle, valeur) in json::lire(ligne).unwrap() {
            assert!(cle.is_ascii(), "clé non ASCII : {cle:?}");
            if let Some(Valeur::Texte(jeton)) = valeur.filter(|_| !LIBRES.contains(&cle.as_str())) {
                assert!(jeton.is_ascii(), "jeton non ASCII : {cle} = {jeton:?}");
            }
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_lancement_et_le_constat_font_l_aller_retour() {
    // Les réglages d'une main tels que Kraken les passe en `--settings` : du JSON compact, mis dans une chaîne du
    // fil, avec un dossier de Windows accentué, ses barres obliques inverses et ses guillemets.
    let kraken = r"C:\Users\é t\.kraken";
    let reglages = format!(
        concat!(
            r#"{{"sandbox":{{"enabled":true,"failIfUnavailable":false,"allowUnsandboxedCommands"#,
            r#"":false,"filesystem":{{"denyRead":["~/",{}],"allowRead":[{}]}},"network":{{"allo"#,
            r#"wedDomains":["docs.exemple.test"],"strictAllowlist":true}},"credentials":{{"envVars":"#,
            r#"[{{"name":"ANTHROPIC_API_KEY","mode":"deny"}},{{"name":"ANTHROPIC_AUTH_TOKEN","mode":"deny"}}]}}}},"#,
            r#""permissions":{{"al"#,
            r#"low":["WebSearch","Skill"],"deny":["AskUserQuestion",{},{},{},{}]}},"disableAllH"#,
            r#"ooks":true,"autoMemoryEnabled":false}}"#,
        ),
        json::chaine(kraken),
        json::chaine(&format!(r"{kraken}\bacs\main-1")),
        json::chaine("Edit(//c/Users/é t/.kraken/fil.jsonl)"),
        json::chaine("Read(//c/Users/é t/.kraken/srt-*.json)"),
        json::chaine("Edit(//c/Users/é t/.kraken/srt-*.json)"),
        json::chaine("Edit(//c/Users/é t/dépôt/.git/**)"),
    );
    let lance = |main, mur, reglages_depot: &str| Evenement::Lancement {
        vague: "v".into(),
        main,
        tentative: 1,
        profil: "claude".into(),
        empreinte: EMPREINTE.into(),
        modele: "claude-sonnet-5-5".into(),
        // Le lancement de la main 2 déclare un effort, les autres non.
        effort: (main == 2).then(|| "xhigh".to_string()),
        mur,
        reglages: reglages.clone(),
        reglages_depot: reglages_depot.into(),
    };
    let constate = |main, version: Option<&str>, mur_actif| Evenement::Constat {
        vague: "v".into(),
        main,
        tentative: 1,
        version: version.map(str::to_string),
        mur_actif,
    };
    // Deux fichiers de réglages au dépôt, puis aucun ; une version et un mur actif, puis un mur inactif sans
    // version (la main n'a rien ouvert), puis une version et rien sur le mur.
    let deux = format!(".claude/settings.json {EMPREINTE}\n.claude/settings.local.json {EMPREINTE}");
    let evenements = [
        lance(1, Mur::Exige, &deux),
        constate(1, Some("2.1.312"), Some(true)),
        lance(2, Mur::Demande, ""),
        constate(2, None, Some(false)),
        lance(3, Mur::Exige, ""),
        constate(3, Some("2.1.316"), None),
    ];
    let dir = dossier("lancement");
    let fil = Fil::seance(&dir, "humain");
    for evenement in &evenements {
        fil.noter(evenement).unwrap();
    }
    // Les accents s'écrivent en UTF-8, tels quels.
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert!(brut.contains("é t"), "{brut}");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    // Ni l'un ni l'autre n'est la preuve d'une main : ils portent l'origine de la séance.
    assert!(notes.iter().all(|note| (note.origine.as_str(), note.origine_foi) == ("humain", Foi::Declare)));
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, evenements);
    // Les réglages reviennent tels qu'ils sont partis, et restent du JSON.
    let Evenement::Lancement { reglages: relus, reglages_depot, .. } = &lus[0] else { panic!("{:?}", lus[0]) };
    assert_eq!(relus, &reglages);
    assert_eq!(json::lire(relus).unwrap()["disableAllHooks"], Some(Valeur::Booleen(true)));
    assert_eq!(reglages_depot.lines().count(), 2);
    // `effort` ne s'écrit que déclaré : la clé manque aux lancements des mains 1 et 3, et porte celui de la main 2.
    assert_eq!(json::lire(brut.lines().nth(2).unwrap()).unwrap()["effort"], Some(Valeur::Texte("xhigh".into())));
    for i in [0, 4] {
        assert!(!brut.lines().nth(i).unwrap().contains("effort"), "{}", brut.lines().nth(i).unwrap());
    }
    // Chaque ligne est plate, et ses jetons sont de l'ASCII.
    let lignes: Vec<_> = brut.lines().map(|ligne| json::lire(ligne).unwrap()).collect();
    for objet in &lignes {
        assert!(objet.values().all(Option::is_some), "des valeurs plates : {objet:?}");
        for cle in ["type", "profil", "empreinte", "modele", "mur", "foi"] {
            if let Some(Some(Valeur::Texte(jeton))) = objet.get(cle) {
                assert!(jeton.is_ascii(), "{cle} = {jeton:?}");
            }
        }
    }
    // L'absence d'une version est null, celle d'une liste est la chaîne vide ; `mur_actif` est vrai, faux ou null.
    assert_eq!(lignes[3]["version"], Some(Valeur::Nul));
    assert_eq!(lignes[2]["reglages_depot"], Some(Valeur::Texte(String::new())));
    let mur_actif: Vec<_> = [1, 3, 5].iter().map(|&i| lignes[i]["mur_actif"].clone()).collect();
    assert_eq!(mur_actif, [Some(Valeur::Booleen(true)), Some(Valeur::Booleen(false)), Some(Valeur::Nul)]);
    // Le rejeu : le mur que le lancement déclarait, le mur actif que le constat a dit.
    let vague = etat(&lus).remove(0);
    let murs: Vec<_> =
        vague.mains.values().map(|main| (main.tentatives[&1].mur, main.tentatives[&1].mur_actif)).collect();
    assert_eq!(murs, [(Some(Mur::Exige), Some(true)), (Some(Mur::Demande), Some(false)), (Some(Mur::Exige), None)]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_fil_relit_les_trois_valeurs_du_mur_sans_changer_de_version() {
    let dir = dossier("trois-murs");
    // Trois lancements écrits à la main : `exige` et `demande`, que le fil portait déjà, et `natif`, toujours à `v` 2.
    let lignes: Vec<String> = ["exige", "demande", "natif"]
        .iter()
        .enumerate()
        .map(|(i, mur)| {
            let ligne = lancement("s", i as u64 + 1).replace(r#""mur":"demande""#, &format!(r#""mur":"{mur}""#));
            assert!(ligne.contains(&format!(r#""mur":"{mur}""#)) && ligne.contains(r#""v":2,"#), "{ligne}");
            ligne
        })
        .collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let murs: Vec<Mur> = notes
        .iter()
        .map(|note| match &note.evenement {
            Evenement::Lancement { mur, .. } => *mur,
            autre => panic!("{autre:?}"),
        })
        .collect();
    assert_eq!(murs, [Mur::Exige, Mur::Demande, Mur::Natif]);
    // Et le fil l'écrit du mot `natif`, que `exige` et `demande` ne remplacent pas.
    let evenement = |mur| Evenement::Lancement {
        vague: "v".into(),
        main: 1,
        tentative: 1,
        profil: "claude".into(),
        empreinte: EMPREINTE.into(),
        modele: "sonnet".into(),
        effort: None,
        mur,
        reglages: "{}".into(),
        reglages_depot: String::new(),
    };
    for mur in [Mur::Exige, Mur::Demande, Mur::Natif] {
        let ligne = ligne_de_note("s", 1, "humain", &evenement(mur));
        let mot = mur.mot();
        assert!(ligne.contains(&format!(r#""mur":"{mot}""#)), "{ligne}");
        assert!(ligne.contains(r#""v":2,"#), "{ligne}");
    }
    assert_eq!([Mur::Exige, Mur::Demande, Mur::Natif].map(Mur::mot), ["exige", "demande", "natif"]);
    // Le mot s'écrit en minuscules : « Natif » est hors du domaine, et sa preuve nomme les trois mots.
    let majuscule = lancement("s", 1).replace(r#""mur":"demande""#, r#""mur":"Natif""#);
    poser(&dir, &[majuscule.as_str()]);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(notes.is_empty(), "{notes:?}");
    assert!(ecarts[0].1.preuve.ends_with(r#"« mur » vaut "Natif", hors de son domaine : exige ou demande ou natif"#));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_lancement_ou_un_constat_hors_domaine_est_mal_forme() {
    let dir = dossier("hors-domaine");
    // Une ligne bien formée, changée : ce qu'on change s'y trouve.
    let change = |ligne: String, de: &str, par: &str| {
        assert!(ligne.contains(de), "{de} ne s'y trouve pas : {ligne}");
        ligne.replace(de, par)
    };
    let empreinte = format!(r#""empreinte":"{EMPREINTE}""#);
    let mauvaises = [
        // `foi` est celle du type, et ne manque pas.
        change(lancement("s", 0), r#""foi":"declare""#, r#""foi":"constate""#),
        change(constat("s", 0), r#""foi":"constate""#, r#""foi":"declare""#),
        change(lancement("s", 0), r#","foi":"declare""#, ""),
        // `mur` est un mot connu.
        change(lancement("s", 0), r#""mur":"demande""#, r#""mur":"ferme""#),
        change(lancement("s", 0), r#""mur":"demande""#, r#""mur":null"#),
        // `mur_actif` est un booléen ou null, ni une chaîne, ni un nombre, ni rien.
        change(constat("s", 0), r#""mur_actif":true"#, r#""mur_actif":"true""#),
        change(constat("s", 0), r#""mur_actif":true"#, r#""mur_actif":1"#),
        change(constat("s", 0), r#","mur_actif":true"#, ""),
        // Un champ en trop.
        change(lancement("s", 0), r#""tentative":1"#, r#""tentative":1,"essai":2"#),
        change(constat("s", 0), r#""tentative":1"#, r#""tentative":1,"essai":2"#),
        // Les textes sont des chaînes, et une liste vide est la chaîne vide, pas null.
        change(lancement("s", 0), r#""reglages_depot":"""#, r#""reglages_depot":null"#),
        change(lancement("s", 0), r#""reglages":"{}""#, r#""reglages":{}"#),
        change(constat("s", 0), r#""version":"2.1.312""#, r#""version":2"#),
        // Les jetons sont de l'ASCII sans blanc, non vides.
        change(lancement("s", 0), r#""profil":"claude""#, r#""profil":"é t""#),
        change(lancement("s", 0), &empreinte, r#""empreinte":"""#),
        change(lancement("s", 0), r#""modele":"sonnet""#, r#""modele":"claude sonnet""#),
        // `main` et `tentative` vont de 1 à la borne.
        change(lancement("s", 0), r#""main":1"#, r#""main":0"#),
        change(constat("s", 0), r#""tentative":1"#, r#""tentative":10001"#),
    ];
    // Une ligne bien formée avant et après : les mal formées comptent dans leur séance, sans faux trou.
    let lignes: Vec<String> = std::iter::once(lancement("s", 0))
        .chain(mauvaises)
        .chain([constat("s", 0)])
        .enumerate()
        .map(|(i, ligne)| ligne.replacen(r#""seq":0,"#, &format!(r#""seq":{},"#, i + 1), 1))
        .collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    let derniere = lignes.len();
    assert_eq!(seqs(&notes), [1, derniere as u64]);
    assert_eq!(codes(&ecarts), (2..derniere).map(|ligne| (ligne, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    // Chacune est refusée pour le champ qu'on a changé, et la preuve dit pourquoi.
    let champs = [
        "foi",
        "foi",
        "foi",
        "mur",
        "mur",
        "mur_actif",
        "mur_actif",
        "mur_actif",
        "essai",
        "essai",
        "reglages_depot",
        "reglages",
        "version",
        "profil",
        "empreinte",
        "modele",
        "main",
        "tentative",
    ];
    assert_eq!(ecarts.len(), champs.len());
    for ((_, refus), champ) in ecarts.iter().zip(champs) {
        assert!(refus.preuve.contains(&format!("« {champ} »")), "{champ} attendu : {}", refus.preuve);
    }
    let preuve = |i: usize| ecarts[i].1.preuve.as_str();
    assert!(preuve(0).ends_with(r#"« foi » vaut "constate", hors de son domaine : declare"#), "{}", preuve(0));
    assert!(preuve(1).ends_with(r#"« foi » vaut "declare", hors de son domaine : constate"#), "{}", preuve(1));
    assert!(preuve(3).ends_with(r#"« mur » vaut "ferme", hors de son domaine : exige ou demande ou natif"#));
    assert!(preuve(5).ends_with("« mur_actif » manque, ou n'est ni un booléen ni null"), "{}", preuve(5));
    assert!(preuve(8).ends_with("champ en trop pour main.lancement : « essai »"), "{}", preuve(8));
    assert!(preuve(9).ends_with("champ en trop pour main.constat : « essai »"), "{}", preuve(9));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_lancement_d_avant_le_champ_effort_se_relit_sans_lui_et_un_effort_hors_domaine_est_mal_forme() {
    let dir = dossier("effort-lu");
    let avec = |effort: &str| {
        let ligne = lancement("s", 0);
        assert!(ligne.contains(r#""modele":"sonnet""#), "{ligne}");
        ligne.replace(r#""modele":"sonnet""#, &format!(r#""modele":"sonnet","effort":{effort}"#))
    };
    // La ligne d'avant le champ, puis chacun des cinq efforts, puis ce qui n'en est pas un.
    let mut ecrites = vec![lancement("s", 0)];
    ecrites.extend(["low", "medium", "high", "xhigh", "max"].map(|effort| avec(&format!("\"{effort}\""))));
    ecrites.extend([avec(r#""extreme""#), avec("null"), avec("1"), avec(r#""""#)]);
    let lignes: Vec<String> = ecrites
        .iter()
        .enumerate()
        .map(|(i, ligne)| ligne.replacen(r#""seq":0,"#, &format!(r#""seq":{},"#, i + 1), 1))
        .collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    let lus: Vec<Option<String>> = notes
        .iter()
        .map(|note| match &note.evenement {
            Evenement::Lancement { effort, .. } => effort.clone(),
            autre => panic!("{autre:?}"),
        })
        .collect();
    let attendus = [None, Some("low"), Some("medium"), Some("high"), Some("xhigh"), Some("max")];
    assert_eq!(lus, attendus.map(|effort| effort.map(str::to_string)));
    assert_eq!(codes(&ecarts), (7..=10).map(|ligne| (ligne, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    let preuve = |i: usize| ecarts[i].1.preuve.as_str();
    let domaine = "hors de son domaine : low ou medium ou high ou xhigh ou max";
    assert!(preuve(0).ends_with(&format!(r#"« effort » vaut "extreme", {domaine}"#)), "{}", preuve(0));
    assert!(preuve(1).ends_with("« effort » manque ou n'est pas une chaîne"), "{}", preuve(1));
    assert!(preuve(2).ends_with("« effort » manque ou n'est pas une chaîne"), "{}", preuve(2));
    assert!(preuve(3).ends_with(&format!(r#"« effort » vaut "", {domaine}"#)), "{}", preuve(3));
    // La ligne d'avant le champ, relue puis réécrite, n'a toujours pas la clé : les mêmes clés qu'elle.
    let autre = dossier("effort-reecrit");
    Fil::seance(&autre, "humain").noter(&notes[0].evenement).unwrap();
    let reecrite = json::lire(fs::read_to_string(autre.join(FICHIER)).unwrap().lines().next().unwrap()).unwrap();
    let ancienne = json::lire(&lignes[0]).unwrap();
    assert_eq!(reecrite.keys().collect::<Vec<_>>(), ancienne.keys().collect::<Vec<_>>());
    assert!(!reecrite.contains_key("effort"));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&autre);
}

/// Une ligne `main.usage` bien formée, écrite à la main, de la séance `seance` au `seq` donné.
fn usage_ecrit(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"main.usage","vague":"v","main":1,"#,
            r#""tentative":1,"modele":"deepseek-flash","entree":1,"sortie":2,"cache_ecrit":3,"cache_lu":4,"#,
            r#""reflexion":5,"micro_dollars":6,"base":"estime","foi":"declare"}}"#,
        ),
        seance = seance,
        seq = seq,
    )
}

#[test]
fn un_usage_hors_domaine_est_mal_forme() {
    let dir = dossier("usage-hors-domaine");
    let change = |de: &str, par: &str| {
        let ligne = usage_ecrit("s", 0);
        assert!(ligne.contains(de), "{de} ne s'y trouve pas : {ligne}");
        ligne.replace(de, par)
    };
    let mauvaises = [
        (change(r#""foi":"declare""#, r#""foi":"constate""#), "foi"),
        (change(r#""base":"estime""#, r#""base":"gratuit""#), "base"),
        (change(r#""base":"estime""#, r#""base":null"#), "base"),
        (change(r#""modele":"deepseek-flash""#, r#""modele":"deepseek flash""#), "modele"),
        (change(r#""modele":"deepseek-flash""#, r#""modele":"""#), "modele"),
        (change(r#""sortie":2"#, r#""sortie":-2"#), "sortie"),
        (change(r#""sortie":2"#, r#""sortie":2.5"#), "sortie"),
        (change(r#""micro_dollars":6"#, r#""micro_dollars":"6""#), "micro_dollars"),
        (change(r#""reflexion":5,"#, ""), "reflexion"),
        (change(r#""tentative":1"#, r#""tentative":0"#), "tentative"),
        (change(r#""tentative":1"#, r#""tentative":1,"essai":2"#), "essai"),
    ];
    let lignes: Vec<String> = std::iter::once(usage_ecrit("s", 0))
        .chain(mauvaises.iter().map(|(ligne, _)| ligne.clone()))
        .chain([usage_ecrit("s", 0)])
        .enumerate()
        .map(|(i, ligne)| ligne.replacen(r#""seq":0,"#, &format!(r#""seq":{},"#, i + 1), 1))
        .collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, lignes.len() as u64]);
    assert_eq!(codes(&ecarts), (2..lignes.len()).map(|ligne| (ligne, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, champ)) in ecarts.iter().zip(&mauvaises) {
        assert!(refus.preuve.contains(&format!("« {champ} »")), "{champ} attendu : {}", refus.preuve);
    }
    let Evenement::Usage { modele, micro_dollars, base, .. } = &notes[0].evenement else { panic!("{notes:?}") };
    assert_eq!((modele.as_str(), *micro_dollars, *base), ("deepseek-flash", 6, Base::Estime));
    let _ = fs::remove_dir_all(&dir);
}

/// Une ligne `main.coupure` bien formée, écrite à la main, de la séance `seance` au `seq` donné.
fn coupure(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"main.coupure","vague":"v","main""#,
            r#":1,"tentative":1,"statut":529}}"#,
        ),
        seance = seance,
        seq = seq,
    )
}

#[test]
fn une_coupure_hors_domaine_est_mal_formee() {
    let dir = dossier("coupure-hors-domaine");
    let change = |de: &str, par: &str| {
        let ligne = coupure("s", 0);
        assert!(ligne.contains(de), "{de} ne s'y trouve pas : {ligne}");
        ligne.replace(de, par)
    };
    // Le statut d'une coupure est 408, 429, ou 500 et plus, sur 16 bits ; rien de plus ne s'écrit.
    let mauvaises = [
        (change(r#""statut":529"#, r#""statut":400"#), "statut"),
        (change(r#""statut":529"#, r#""statut":499"#), "statut"),
        (change(r#""statut":529"#, r#""statut":70000"#), "statut"),
        (change(r#""statut":529"#, r#""statut":"529""#), "statut"),
        (change(r#""statut":529"#, r#""statut":null"#), "statut"),
        (change(r#""statut":529"#, r#""statut":529.5"#), "statut"),
        (change(r#","statut":529"#, ""), "statut"),
        (change(r#""tentative":1"#, r#""tentative":0"#), "tentative"),
        (change(r#""statut":529"#, r#""statut":529,"foi":"constate""#), "foi"),
    ];
    let lignes: Vec<String> = std::iter::once(coupure("s", 0))
        .chain(mauvaises.iter().map(|(ligne, _)| ligne.clone()))
        .chain([change(r#""statut":529"#, r#""statut":408"#), change(r#""statut":529"#, r#""statut":599"#)])
        .enumerate()
        .map(|(i, ligne)| ligne.replacen(r#""seq":0,"#, &format!(r#""seq":{},"#, i + 1), 1))
        .collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [1, (lignes.len() - 1) as u64, lignes.len() as u64]);
    assert_eq!(codes(&ecarts), (2..lignes.len() - 1).map(|ligne| (ligne, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, champ)) in ecarts.iter().zip(&mauvaises) {
        assert!(refus.preuve.contains(&format!("« {champ} »")), "{champ} attendu : {}", refus.preuve);
    }
    let statuts: Vec<u16> = notes
        .iter()
        .map(|note| match note.evenement {
            Evenement::Coupure { statut, .. } => statut,
            ref autre => panic!("{autre:?}"),
        })
        .collect();
    assert_eq!(statuts, [529, 408, 599]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_rejeu_garde_les_coupures_de_chaque_tentative_et_un_fil_sans_elles_se_lit() {
    let dir = dossier("coupure-rejeu");
    // Sans `main.coupure`, un fil se lit et ne dit aucune coupure : l'exemple d'avant en est un.
    poser(&dir, EXEMPLE);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let avant: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert!(etat(&avant).iter().flat_map(|vague| vague.mains.values()).all(|main| main.coupures.is_empty()));
    // Avec, la coupure se range sous sa main et sa tentative, avec son statut.
    let fin = |tentative| Evenement::Fin {
        vague: "v".into(),
        main: 1,
        tentative,
        couleur: Couleur::Rouge,
        foi: Foi::Constate,
        texte: "hand: result in error".into(),
        verif: None,
    };
    let coupee = |tentative, statut| Evenement::Coupure { vague: "v".into(), main: 1, tentative, statut };
    let evenements = [fin(1), coupee(1, 529), fin(2), coupee(2, 429)];
    let vague = etat(&evenements).remove(0);
    assert_eq!(vague.mains[&1].coupures.iter().map(|(t, s)| (*t, *s)).collect::<Vec<_>>(), [(1, 529), (2, 429)]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_fil_d_avant_m2_se_rejoue_a_l_identique() {
    // EXEMPLE vient d'avant M2 : ni lancement ni constat. Il se lit sans écart, et aucune tentative n'y a de mur.
    let dir = dossier("avant-m2");
    poser(&dir, EXEMPLE);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let avant: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    let vagues = etat(&avant);
    let murs = |vagues: &[Vague]| -> Vec<(Option<Mur>, Option<bool>)> {
        let tentatives = vagues.iter().flat_map(|vague| vague.mains.values()).flat_map(|main| main.tentatives.values());
        tentatives.map(|tentative| (tentative.mur, tentative.mur_actif)).collect()
    };
    assert!(!murs(&vagues).is_empty() && murs(&vagues).iter().all(|mur| *mur == (None, None)));
    // Le même fil avec, après chaque début, son lancement et son constat : les mêmes vagues, au mur près.
    let mut avec = Vec::new();
    for evenement in &avant {
        avec.push(evenement.clone());
        if let Evenement::Debut { vague, main, tentative } = evenement {
            let (vague, main, tentative) = (vague.clone(), *main, *tentative);
            avec.push(Evenement::Lancement {
                vague: vague.clone(),
                main,
                tentative,
                profil: "claude".into(),
                empreinte: EMPREINTE.into(),
                modele: "sonnet".into(),
                effort: None,
                mur: Mur::Exige,
                reglages: "{}".into(),
                reglages_depot: String::new(),
            });
            let (version, mur_actif) = (Some("2.1.312".into()), Some(true));
            avec.push(Evenement::Constat { vague, main, tentative, version, mur_actif });
        }
    }
    let autre = dossier("avec-m2");
    let fil = Fil::seance(&autre, "humain");
    for evenement in &avec {
        fil.noter(evenement).unwrap();
    }
    let Lecture { notes, ecarts } = lire(&autre).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(notes.len(), avec.len());
    let mut apres = etat(&notes.into_iter().map(|note| note.evenement).collect::<Vec<_>>());
    assert!(murs(&apres).iter().all(|mur| *mur == (Some(Mur::Exige), Some(true))), "{:?}", murs(&apres));
    let tentatives =
        apres.iter_mut().flat_map(|vague| vague.mains.values_mut()).flat_map(|main| main.tentatives.values_mut());
    for tentative in tentatives {
        (tentative.mur, tentative.mur_actif) = (None, None);
    }
    assert_eq!(apres, vagues);
    // Un Kraken d'avant M2 ne connaît pas ces deux types : il les ignore, et leur `seq` compte dans leur séance.
    // Rendus inconnus, ils laissent le fil d'avant, sans trou.
    let chemin = autre.join(FICHIER);
    let ancien = fs::read_to_string(&chemin).unwrap();
    let ancien = ancien.replace(r#""type":"main.lancement""#, r#""type":"x.lancement""#);
    fs::write(&chemin, ancien.replace(r#""type":"main.constat""#, r#""type":"x.constat""#)).unwrap();
    let Lecture { notes, ecarts } = lire(&autre).unwrap();
    assert!(ecarts.is_empty(), "aucun trou : {ecarts:?}");
    assert_eq!(seqs(&notes)[..3], [1, 2, 5], "le lancement et le constat de la première main sont les seq 3 et 4");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, avant);
    assert_eq!(etat(&lus), vagues);
    for dir in [dir, autre] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn l_origine_est_declaree_et_la_preuve_d_une_main_porte_la_sienne() {
    let dir = dossier("origine");
    let fil = Fil::seance(&dir, "humain");
    let vague = lot();
    let fin = |tentative, couleur, foi| Evenement::Fin {
        vague: vague.clone(),
        main: 1,
        tentative,
        couleur,
        foi,
        texte: "t".into(),
        verif: None,
    };
    let evenements = [
        Evenement::Vague { vague: vague.clone(), n: 1, depot: "d".into(), verifier: None, objectif: "o".into() },
        Evenement::Debut { vague: vague.clone(), main: 1, tentative: 1 },
        fin(1, Couleur::Rouge, Foi::Constate),
        Evenement::Debut { vague: vague.clone(), main: 1, tentative: 2 },
        fin(2, Couleur::Vert, Foi::Declare),
        Evenement::Verdict { vague: vague.clone(), main: 1, verdict: Verdict::Prise, foi: Foi::Declare },
    ];
    for evenement in &evenements {
        fil.noter(evenement).unwrap();
    }
    // Chaque ligne écrite porte `origine_foi: declare` ; la fin déclarée porte l'origine de sa main.
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert!(brut.lines().all(|ligne| ligne.contains(r#","origine_foi":"declare","#)), "{brut}");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let main = format!("agent:main-{vague}-1");
    let origines: Vec<(&str, Foi)> = notes.iter().map(|note| (note.origine.as_str(), note.origine_foi)).collect();
    let humain = ("humain", Foi::Declare);
    assert_eq!(origines, [humain, humain, humain, humain, (main.as_str(), Foi::Declare), humain]);
    // Une fin déclarée qui porte `humain`, ou l'origine d'une autre main, est mal formée. Une fin constatée ou un
    // début peuvent porter l'origine d'une main : la règle ne vaut que pour la ligne de preuve.
    let fin = |seq: u64, origine: &str, foi: &str| {
        format!(
            concat!(
                r#"{{"v":2,"seance":"s","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","origine":"#,
                r#""{origine}","origine_foi":"declare","type":"main.fin","vague":"v","main":1,"tent"#,
                r#"ative":1,"couleur":"vert","foi":"{foi}","texte":"PREUVE: vert","verif":null}}"#,
            ),
            seq = seq,
            origine = origine,
            foi = foi,
        )
    };
    let autre = dossier("origine-lue");
    let ecrites = [
        fin(1, "agent:main-v-1", "declare"),
        fin(2, "humain", "declare"),
        fin(3, "agent:main-v-2", "declare"),
        fin(4, "agent:main-w-1", "declare"),
        fin(5, "agent:main-v-1", "constate"),
        debut("s", 6).replace(r#""origine":"humain""#, r#""origine":"agent:main-v-1""#),
    ];
    poser(&autre, &ecrites.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&autre).unwrap();
    assert_eq!(seqs(&notes), [1, 5, 6]);
    assert_eq!(codes(&ecarts), [(2, 1009), (3, 1009), (4, 1009)], "et aucun trou");
    assert!(ecarts[0].1.preuve.ends_with("une fin déclarée porte l'origine de sa main, agent:main-v-1, pas humain"));
    for dir in [dir, autre] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn une_origine_inconnue_est_mal_formee() {
    let dir = dossier("origine-inconnue");
    let origine = |seq, origine: &str| debut("s", seq).replace(r#""origine":"humain""#, origine);
    let foi = |seq, foi: &str| debut("s", seq).replace(r#","origine_foi":"declare""#, foi);
    let (long, juste) = ("a".repeat(57), "a".repeat(56)); // « main-<57 a>-1 » fait 64 caractères, un de trop
    let lignes = [
        origine(1, r#""origine":"patron""#),
        origine(2, r#""origine":"agent:verificateur""#), // une forme de passerelle, que M3a ne lit pas
        origine(3, r#""origine":"noeud:hote-a""#),
        origine(4, r#""origine":"agent:main-V-1""#),
        origine(5, &format!(r#""origine":"agent:main-{long}-1""#)),
        origine(6, r#""origine":"""#),
        origine(7, r#""origine":"agent:main--1""#),
        foi(8, ""),
        foi(9, r#","origine_foi":"prouve""#),
        foi(10, r#","origine_foi":null"#),
        foi(11, r#","origine_foi":"constate""#), // la valeur que le nœud écrira : déjà dans le domaine
        origine(12, &format!(r#""origine":"agent:main-{juste}-1""#)),
        debut("s", 13),
    ];
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(seqs(&notes), [11, 12, 13]);
    assert_eq!(codes(&ecarts), (1..=10).map(|ligne| (ligne, 1009)).collect::<Vec<_>>(), "et aucun trou");
    assert_eq!(notes[0].origine_foi, Foi::Constate);
    assert!(ecarts[0].1.preuve.ends_with(r#"« origine » n'est ni humain ni agent:main-<vague>-<main> : "patron""#));
    assert!(ecarts[7].1.preuve.ends_with("« origine_foi » manque ou n'est pas une chaîne"), "{}", ecarts[7].1);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn l_instant_s_ecrit_a_la_milliseconde_et_se_lit_de_zero_a_trois_decimales() {
    // Les dates de l'horloge de passerelle, plus les millisecondes.
    assert_eq!(instant(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(instant(951_782_400_000), "2000-02-29T00:00:00.000Z");
    assert_eq!(instant(1_792_962_180_123), "2026-10-25T21:03:00.123Z");
    assert_eq!(instant(4_102_444_799_999), "2099-12-31T23:59:59.999Z");
    assert!(instant_lu(&instant(maintenant())));
    for lu in [
        "2026-10-31T21:03:00Z",
        "2026-10-31T21:03:00.1Z",
        "2026-10-31T21:03:00.12Z",
        "2026-10-31T21:03:00.123Z",
        "2000-02-29T23:59:59.999Z",
        "2024-02-29T00:00:00Z",
        "0000-01-01T00:00:00Z",
        "9999-12-31T23:59:59.999Z",
    ] {
        assert!(instant_lu(lu), "{lu}");
    }
    for refuse in [
        "2026-10-31T21:03:00.1234Z",
        "2026-10-31T21:03:00.Z",
        "2026-10-31T21:03:00",
        "2026-10-31T21:03:00+00:00",
        "2026-10-31T21:03:00.123+00:00",
        "2026-10-31t21:03:00z",
        "2026-10-31 21:03:00Z",
        "2026-10-31T21:03Z",
        "26-10-31T21:03:00Z",
        "2026-13-01T00:00:00Z",
        "2026-00-01T00:00:00Z",
        "2026-10-00T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-10-31T24:00:00Z",
        "2026-10-31T23:60:00Z",
        "2026-12-31T23:59:60Z",
        "2026-10-31T21:03:00.12aZ",
        "2026-10-31T21:03:00Z ",
        "2026-1O-31T21:03:00Z",
        "２０２６-10-31T21:03:00Z",
        "",
    ] {
        assert!(!instant_lu(refuse), "{refuse}");
    }
    // Au fil : chaque décimale de 0 à 3 se lit, une quatrième rend la ligne mal formée.
    let dir = dossier("instant");
    let avec = |seq, instant: &str| debut("s", seq).replace("2026-10-31T21:03:00.123Z", instant);
    let lignes = [
        avec(1, "2026-10-31T21:03:00Z"),
        avec(2, "2026-10-31T21:03:00.1Z"),
        avec(3, "2026-10-31T21:03:00.12Z"),
        avec(4, "2026-10-31T21:03:00.123Z"),
        avec(5, "2026-10-31T21:03:00.1234Z"),
        debut("s", 6).replace(r#""instant":"2026-10-31T21:03:00.123Z""#, r#""instant":1793480580123"#),
    ];
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    let lus: Vec<&str> = notes.iter().map(|note| note.instant.as_str()).collect();
    assert_eq!(
        lus,
        ["2026-10-31T21:03:00Z", "2026-10-31T21:03:00.1Z", "2026-10-31T21:03:00.12Z", "2026-10-31T21:03:00.123Z"]
    );
    assert_eq!(codes(&ecarts), [(5, 1009), (6, 1009)]);
    let _ = fs::remove_dir_all(&dir);
}

/// Une ligne de la séance `s` au `seq` donné : l'enveloppe de Kraken, puis le corps (`"type":…`) tel qu'on l'écrit.
fn ligne_s(seq: u64, corps: &str) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"s","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","origine":"#,
            r#""humain","origine_foi":"declare",{corps}}}"#,
        ),
        seq = seq,
        corps = corps,
    )
}

#[test]
fn les_listes_et_les_absences_de_vague_font_l_aller_retour() {
    // Le texte libre de chaque type neuf : accents, guillemets, barre, tabulation, retour chariot, saut de ligne.
    const LIBRE: &str = "é à « ç »\t\"q\" \\ fin\r\nligne 2, 😀";
    const LIBRE_JSON: &str = r#"é à « ç »\t\"q\" \\ fin\r\nligne 2, 😀"#;
    let tache = |main: usize, depend: &[&str]| Evenement::Tache {
        vague: "v".into(),
        main,
        id: format!("t{main}"),
        depend: depend.iter().map(|id| id.to_string()).collect(),
        preuve: format!("cargo test\t{LIBRE}"),
        objectif: LIBRE.into(),
    };
    let perimee = |verif: Option<&str>| Evenement::Perimee {
        vague: "v".into(),
        main: 1,
        tentative: 1,
        couleur: Couleur::Rouge,
        foi: Foi::Constate,
        texte: LIBRE.into(),
        verif: verif.map(str::to_string),
    };
    let fusion = |verif: &str| Evenement::Fusion {
        vague: "v".into(),
        main: 1,
        sha: "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34".into(),
        verif: verif.into(),
    };
    let refus = |verif: Option<&str>| Evenement::Refus {
        vague: "v".into(),
        main: 1,
        code: 7007,
        regle: LIBRE.into(),
        preuve: LIBRE.into(),
        correction: LIBRE.into(),
        verif: verif.map(str::to_string),
    };
    // Chaque événement, et la fin de sa ligne : `depend` à zéro, un et plusieurs ids ; `verif` nul, vide et plein.
    let plein = format!(r#""verif":"{LIBRE_JSON}"}}"#);
    let exemples = [
        (tache(1, &[]), r#""depend":"","preuve":"#.to_string()),
        (tache(2, &["t1"]), r#""depend":"t1","preuve":"#.to_string()),
        (tache(3, &["t1", "t2", "a-b_c"]), r#""depend":"t1\nt2\na-b_c","preuve":"#.to_string()),
        (perimee(None), r#""verif":null}"#.to_string()),
        (perimee(Some("")), r#""verif":""}"#.to_string()),
        (perimee(Some(LIBRE)), plein.clone()),
        (fusion(""), r#""verif":"","foi":"constate"}"#.to_string()),
        (fusion(LIBRE), format!(r#""verif":"{LIBRE_JSON}","foi":"constate"}}"#)),
        (refus(None), r#""verif":null}"#.to_string()),
        (refus(Some("")), r#""verif":""}"#.to_string()),
        (refus(Some(LIBRE)), plein),
    ];
    let dir = dossier("listes");
    let fil = Fil::seance(&dir, "humain");
    for (evenement, _) in &exemples {
        fil.noter(evenement).unwrap();
    }
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert_eq!(brut.lines().count(), exemples.len());
    for ((evenement, fin), ligne) in exemples.iter().zip(brut.lines()) {
        assert!(ligne.contains(fin.as_str()), "{evenement:?} s'écrit avec {fin} : {ligne}");
    }
    // Chaque événement se relit tel quel, listes et absences comprises : null reste null, vide reste vide.
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, exemples.iter().map(|(evenement, _)| evenement.clone()).collect::<Vec<_>>());
    // Et le rejeu garde les dépendances de chaque tâche, dans leur ordre.
    let vagues = etat(&lus);
    let depend = |n: usize| vagues[0].mains[&n].tache.clone();
    assert_eq!(depend(1), Some(("t1".to_string(), vec![])));
    assert_eq!(depend(2), Some(("t2".to_string(), vec!["t1".to_string()])));
    let plusieurs = ["t1", "t2", "a-b_c"].map(str::to_string).to_vec();
    assert_eq!(depend(3), Some(("t3".to_string(), plusieurs)));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_fil_de_vague_a_plan_se_rejoue() {
    let v = "1761901400000_9_0";
    let tache = |main: usize, id: &str, depend: &[&str]| Evenement::Tache {
        vague: v.into(),
        main,
        id: id.into(),
        depend: depend.iter().map(|id| id.to_string()).collect(),
        preuve: "cargo test".into(),
        objectif: format!("Tâche {id}"),
    };
    // L'échéance de la tentative t est 22:0t:00.5Z : à la milliseconde, le rejeu la range.
    let bail = |main, tentative| Evenement::Bail {
        vague: v.into(),
        main,
        tentative,
        echeance: format!("2026-10-31T22:0{tentative}:00.5Z"),
    };
    let debut = |main, tentative| Evenement::Debut { vague: v.into(), main, tentative };
    let fin = |main, tentative, couleur| Evenement::Fin {
        vague: v.into(),
        main,
        tentative,
        couleur,
        foi: Foi::Constate,
        texte: format!("fin {main}.{tentative}"),
        verif: None,
    };
    let verdict = |main, verdict| Evenement::Verdict { vague: v.into(), main, verdict, foi: Foi::Constate };
    let fusion = |main| Evenement::Fusion { vague: v.into(), main, sha: format!("{main:040x}"), verif: "ok".into() };
    let refus = |main, code: Code| Evenement::Refus {
        vague: v.into(),
        main,
        code: code.numero(),
        regle: code.regle().into(),
        preuve: format!("preuve {main}"),
        correction: format!("correction {main}"),
        verif: None,
    };
    let evenements = vec![
        Evenement::Vague {
            vague: v.into(),
            n: 7,
            depot: "/srv/depot".into(),
            verifier: None,
            objectif: "Sept tâches".into(),
        },
        Evenement::Assemblage {
            vague: v.into(),
            branche: format!("kraken/vague-{v}"),
            base: "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34".into(),
            preuve: "cargo test".into(),
        },
        tache(1, "a", &[]),
        tache(2, "b", &["a"]),
        tache(3, "c", &[]),
        tache(4, "d", &["c"]),
        tache(5, "e", &[]),
        tache(6, "f", &[]),
        tache(7, "g", &["a"]),
        // a : prise, puis assemblée.
        bail(1, 1),
        debut(1, 1),
        fin(1, 1, Couleur::Vert),
        verdict(1, Verdict::Prise),
        fusion(1),
        // c : rouge deux fois, classée.
        bail(3, 1),
        debut(3, 1),
        fin(3, 1, Couleur::Rouge),
        bail(3, 2),
        debut(3, 2),
        fin(3, 2, Couleur::Rouge),
        verdict(3, Verdict::Classee),
        // e : prise, en attente d'assemblage.
        bail(5, 1),
        debut(5, 1),
        fin(5, 1, Couleur::Vert),
        verdict(5, Verdict::Prise),
        // f : en cours.
        bail(6, 1),
        debut(6, 1),
        // b : prise, refusée à l'assemblage (un conflit).
        bail(2, 1),
        debut(2, 1),
        fin(2, 1, Couleur::Vert),
        verdict(2, Verdict::Prise),
        refus(2, Code::Conflit),
        // g : assemblée, puis le refus 7009 d'un `update-ref` raté, venu après sa fusion, l'emporte.
        bail(7, 1),
        debut(7, 1),
        fin(7, 1, Couleur::Vert),
        verdict(7, Verdict::Prise),
        fusion(7),
        refus(7, Code::GitImpossible),
        // d : sautée, sans bac ni main.
        refus(4, Code::Sautee),
    ];
    // Écrite au fil puis relue, la vague se rejoue sur ce que le lecteur a lu.
    let dir = dossier("plan");
    let fil = Fil::seance(&dir, "humain");
    for evenement in &evenements {
        fil.noter(evenement).unwrap();
    }
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, evenements);
    let vagues = etat(&lus);
    let [vague] = vagues.as_slice() else { panic!("une vague : {vagues:?}") };
    assert_eq!((vague.id.as_str(), vague.n, vague.mains.len()), (v, 7, 7));
    let assemblage = vague.assemblage.as_ref().map(|(branche, preuve)| (branche.as_str(), preuve.as_str()));
    assert_eq!(assemblage, Some((format!("kraken/vague-{v}").as_str(), "cargo test")));
    // Chaque tâche : son id, ses dépendances, son issue, son verdict et ses tentatives.
    let issue = |issue: &Option<Issue>| match issue {
        None => "-".to_string(),
        Some(Issue::Assemblee) => "assemblée".to_string(),
        Some(Issue::Refusee { code, .. }) => format!("refusée {code}"),
    };
    let lignes: Vec<_> = vague
        .mains
        .iter()
        .map(|(n, main)| {
            let (id, depend) = main.tache.clone().expect("une tâche");
            (*n, id, depend.join("+"), issue(&main.issue), main.verdict, main.tentatives.len())
        })
        .collect();
    let (prise, classee) = (Some((Verdict::Prise, Foi::Constate)), Some((Verdict::Classee, Foi::Constate)));
    let attendu = |n: usize, id: &str, depend: &str, issue: &str, verdict, tentatives: usize| {
        (n, id.to_string(), depend.to_string(), issue.to_string(), verdict, tentatives)
    };
    assert_eq!(
        lignes,
        [
            attendu(1, "a", "", "assemblée", prise, 1),
            attendu(2, "b", "a", "refusée 7007", prise, 1),
            attendu(3, "c", "", "-", classee, 2),
            attendu(4, "d", "c", "refusée 7011", None, 0),
            attendu(5, "e", "", "-", prise, 1),
            attendu(6, "f", "", "-", None, 1),
            attendu(7, "g", "a", "refusée 7009", prise, 1),
        ]
    );
    // Un refus garde ses quatre champs, et chaque tentative son échéance, rangée à la milliseconde.
    let conflit = Code::Conflit;
    let (regle, preuve, correction) = (conflit.regle().to_string(), "preuve 2".to_string(), "correction 2".into());
    assert_eq!(vague.mains[&2].issue, Some(Issue::Refusee { code: 7007, regle, preuve, correction }));
    let echeances: Vec<_> = vague.mains[&3].echeances.iter().map(|(t, e)| (*t, e.as_str())).collect();
    assert_eq!(echeances, [(1, "2026-10-31T22:01:00.500Z"), (2, "2026-10-31T22:02:00.500Z")]);
    // L'issue est la dernière décision, dans un sens comme dans l'autre.
    let apres = |evenements: Vec<Evenement>| etat(&evenements).remove(0).mains.remove(&1).unwrap().issue;
    let refuse = Some(Issue::Refusee {
        code: 7009,
        regle: Code::GitImpossible.regle().into(),
        preuve: "preuve 1".into(),
        correction: "correction 1".into(),
    });
    assert_eq!(apres(vec![fusion(1), refus(1, Code::GitImpossible)]), refuse);
    assert_eq!(apres(vec![refus(1, Code::GitImpossible), fusion(1)]), Some(Issue::Assemblee));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn au_rejeu_rien_d_une_tentative_expiree_n_est_pris() {
    let v = "1761901400000_9_0";
    let tache = |main: usize, id: &str| Evenement::Tache {
        vague: v.into(),
        main,
        id: id.into(),
        depend: vec![],
        preuve: "cargo test".into(),
        objectif: format!("Tâche {id}"),
    };
    let bail = |main, tentative| Evenement::Bail {
        vague: v.into(),
        main,
        tentative,
        echeance: "2026-10-31T22:06:02.400Z".into(),
    };
    let debut = |main, tentative| Evenement::Debut { vague: v.into(), main, tentative };
    let fin = |main, tentative, couleur| Evenement::Fin {
        vague: v.into(),
        main,
        tentative,
        couleur,
        foi: Foi::Constate,
        texte: format!("fin {main}.{tentative}"),
        verif: Some("test result: ok".into()),
    };
    let verdict = |main| Evenement::Verdict { vague: v.into(), main, verdict: Verdict::Prise, foi: Foi::Constate };
    let perimee = |main, tentative, foi| Evenement::Perimee {
        vague: v.into(),
        main,
        tentative,
        couleur: Couleur::Vert,
        foi,
        texte: format!("périmée {main}.{tentative}"),
        verif: None,
    };
    let expiree = |main| {
        let code = Code::MainExpiree;
        let (regle, preuve, correction) = (code.regle().into(), format!("preuve {main}"), "Relance.".into());
        Evenement::Refus { vague: v.into(), main, code: code.numero(), regle, preuve, correction, verif: None }
    };
    // Jusqu'aux refus 7004 : a expire à sa première tentative, b à sa seconde (la première a fini rouge), c court.
    let avant = vec![
        Evenement::Vague { vague: v.into(), n: 3, depot: "/d".into(), verifier: None, objectif: "o".into() },
        tache(1, "a"),
        tache(2, "b"),
        tache(3, "c"),
        bail(1, 1),
        debut(1, 1),
        bail(2, 1),
        debut(2, 1),
        fin(2, 1, Couleur::Rouge),
        bail(2, 2),
        debut(2, 2),
        bail(3, 1),
        debut(3, 1),
        expiree(1),
        expiree(2),
    ];
    // Après eux : les fins et les verdicts que les mains rendent encore, leurs résultats périmés, et un résultat
    // périmé de c, qui n'a pas expiré dans le fil.
    let apres = vec![
        fin(1, 1, Couleur::Vert),
        verdict(1),
        perimee(1, 1, Foi::Declare),
        fin(2, 2, Couleur::Vert),
        verdict(2),
        perimee(2, 2, Foi::Constate),
        perimee(3, 1, Foi::Declare),
    ];
    let tout = [avant.clone(), apres].concat();
    let dir = dossier("expiree");
    let fil = Fil::seance(&dir, "humain");
    for evenement in &tout {
        fil.noter(evenement).unwrap();
    }
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, tout);
    // Rien de ce qui vient après ne change le rejeu : ni la fin, ni le verdict, ni la périmée.
    let vagues = etat(&lus);
    assert_eq!(vagues, etat(&avant));
    let main = |n: usize| {
        let main = &vagues[0].mains[&n];
        let tentatives = main.tentatives.iter().map(|(t, lue)| (*t, lue.couleur, lue.texte.as_str()));
        let issue = match &main.issue {
            Some(Issue::Refusee { code, .. }) => Some(*code),
            _ => None,
        };
        (main.verdict, issue, tentatives.collect::<Vec<_>>())
    };
    let rouge = Some((Couleur::Rouge, Foi::Constate));
    assert_eq!(main(1), (None, Some(7004), vec![(1, None, "")]), "a : expirée, rien n'est pris");
    assert_eq!(main(2), (None, Some(7004), vec![(1, rouge, "fin 2.1"), (2, None, "")]), "b : seule la fin d'avant");
    assert_eq!(main(3), (None, None, vec![(1, None, "")]), "c : une périmée ne finit pas sa tentative");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_bail_garde_son_echeance() {
    let dir = dossier("bail");
    // Un autre écrivain écrit l'échéance à 0, 1, 2 ou 3 décimales : le fil la garde telle quelle.
    let ecrites =
        ["2026-10-31T21:03:00Z", "2026-10-31T21:03:00.5Z", "2026-10-31T21:03:00.12Z", "2026-10-31T21:03:00.123Z"];
    let bail = |seq: u64, echeance: &str| {
        let corps = format!(r#""type":"main.bail","vague":"v","main":1,"tentative":{seq},"echeance":"{echeance}""#);
        ligne_s(seq, &corps)
    };
    let lignes: Vec<String> = ecrites.iter().zip(1..).map(|(echeance, seq)| bail(seq, echeance)).collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    for (evenement, ecrite) in evenements.iter().zip(ecrites) {
        let Evenement::Bail { echeance, .. } = evenement else { panic!("un bail : {evenement:?}") };
        assert_eq!(echeance, ecrite);
    }
    // Le rejeu les range à la milliseconde, `…:00Z` comprise.
    let vagues = etat(&evenements);
    let rangees: Vec<_> = vagues[0].mains[&1].echeances.iter().map(|(t, e)| (*t, e.as_str())).collect();
    let attendu = [
        (1, "2026-10-31T21:03:00.000Z"),
        (2, "2026-10-31T21:03:00.500Z"),
        (3, "2026-10-31T21:03:00.120Z"),
        (4, "2026-10-31T21:03:00.123Z"),
    ];
    assert_eq!(rangees, attendu);
    // Ainsi rangées, elles se comparent comme des textes, ce que les instants lus ne font pas : `Z` suit `.`.
    let mut textes: Vec<&str> = rangees.iter().map(|(_, e)| *e).collect();
    textes.sort_unstable();
    assert_eq!(textes, [attendu[0].1, attendu[2].1, attendu[3].1, attendu[1].1]);
    assert!(ecrites[0] > ecrites[1], "lu, …:00Z passe après …:00.5Z");
    // Ce que Kraken écrit est déjà à la milliseconde : le rangement n'y change rien. Redit, un bail remplace.
    for ms in [0, 1, 999, 1000, 1_792_962_180_123, 4_102_444_799_999] {
        assert_eq!(a_la_milliseconde(&instant(ms)), instant(ms));
    }
    let redit = [evenements[0].clone(), evenements[1].clone(), evenements[0].clone()];
    let relu = &etat(&redit)[0].mains[&1].echeances;
    assert_eq!(relu.len(), 2);
    assert_eq!(relu[&1], "2026-10-31T21:03:00.000Z");
    // Kraken écrit l'échéance telle qu'il la donne, sans décimale ajoutée.
    let ecrit = dossier("bail-ecrit");
    let bail = Evenement::Bail { vague: "v".into(), main: 1, tentative: 1, echeance: ecrites[0].into() };
    Fil::seance(&ecrit, "humain").noter(&bail).unwrap();
    let brut = fs::read_to_string(ecrit.join(FICHIER)).unwrap();
    assert!(brut.contains(r#""echeance":"2026-10-31T21:03:00Z"}"#), "{brut}");
    for dir in [dir, ecrit] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn un_type_de_vague_aux_champs_faux_est_mal_forme() {
    const ASSEMBLAGE: &str =
        r#""type":"vague.assemblage","vague":"v","branche":"kraken/vague-v","base":"9f2c41a7","preuve":"cargo test""#;
    const TACHE: &str =
        r#""type":"vague.tache","vague":"v","main":1,"id":"a","depend":"x\ny","preuve":"cargo test","objectif":"o""#;
    const BAIL: &str = r#""type":"main.bail","vague":"v","main":1,"tentative":1,"echeance":"2026-10-31T21:03:00.123Z""#;
    const PERIMEE: &str = concat!(
        r#""type":"main.perimee","vague":"v","main":1,"tentative":1,"couleur":"vert","foi":"#,
        r#""constate","texte":"t","verif":null"#,
    );
    const FUSION: &str = r#""type":"vague.fusion","vague":"v","main":1,"sha":"9f2c41a7","verif":"ok","foi":"constate""#;
    const REFUS: &str = concat!(
        r#""type":"vague.refus","vague":"v","main":1,"code":7007,"regle":"r","preuve":"p",""#,
        r#"correction":"c","verif":null"#,
    );
    let en_trop = |corps: &str, cle: &str| format!(r#"{corps},"{cle}":1"#);
    // Chaque ligne fausse, et un bout de ce que sa preuve dit d'elle.
    let cas = vec![
        // vague.assemblage
        (ASSEMBLAGE.replace(r#","base":"9f2c41a7""#, ""), "« base » manque"),
        (
            ASSEMBLAGE.replace(r#""preuve":"cargo test""#, r#""preuve":null"#),
            "« preuve » manque ou n'est pas une chaîne",
        ),
        (en_trop(ASSEMBLAGE, "essai"), "champ en trop pour vague.assemblage : « essai »"),
        // vague.tache
        (TACHE.replace(r#""main":1"#, r#""main":0"#), "« main » vaut 0, hors de son domaine : un entier de 1 à 10000"),
        (TACHE.replace(r#""main":1"#, r#""main":10001"#), "« main » vaut 10001, hors de son domaine"),
        (TACHE.replace(r#""id":"a","#, ""), "« id » manque"),
        (
            TACHE.replace(r#""depend":"x\ny""#, r#""depend":"x\n\ny""#),
            "« depend » est une liste d'un élément par ligne, sans élément vide",
        ),
        (TACHE.replace(r#""depend":"x\ny""#, r#""depend":"x\n""#), "sans élément vide"),
        (TACHE.replace(r#""depend":"x\ny""#, r#""depend":"\nx""#), "sans élément vide"),
        (TACHE.replace(r#""depend":"x\ny""#, r#""depend":"\n""#), "sans élément vide"),
        (TACHE.replace(r#""depend":"x\ny""#, r#""depend":null"#), "« depend » manque ou n'est pas une chaîne"),
        (TACHE.replace(r#""depend":"x\ny""#, r#""depend":["x","y"]"#), "« depend » n'est pas une valeur plate"),
        (TACHE.replace(r#""objectif":"o""#, r#""objectif":7"#), "« objectif » manque ou n'est pas une chaîne"),
        (en_trop(TACHE, "essai"), "champ en trop pour vague.tache : « essai »"),
        // main.bail
        (
            BAIL.replace("2026-10-31T21:03:00.123Z", "2026-10-31 21:03:00Z"),
            "« echeance » n'est pas un instant RFC 3339 en UTC",
        ),
        (BAIL.replace("2026-10-31T21:03:00.123Z", "2026-10-31T21:03:00.1234Z"), "« echeance » n'est pas un instant"),
        (BAIL.replace("2026-10-31T21:03:00.123Z", "2026-02-30T21:03:00Z"), "« echeance » n'est pas un instant"),
        (
            BAIL.replace("2026-10-31T21:03:00.123Z", "demain"),
            "« echeance » n'est pas un instant RFC 3339 en UTC, de 0 à 3 décimales : \"demain\"",
        ),
        (
            BAIL.replace(r#""echeance":"2026-10-31T21:03:00.123Z""#, r#""echeance":1793480580123"#),
            "« echeance » manque ou n'est pas une chaîne",
        ),
        (
            BAIL.replace(r#""echeance":"2026-10-31T21:03:00.123Z""#, r#""echeance":null"#),
            "« echeance » manque ou n'est pas une chaîne",
        ),
        (BAIL.replace(r#","echeance":"2026-10-31T21:03:00.123Z""#, ""), "« echeance » manque"),
        (BAIL.replace(r#""tentative":1"#, r#""tentative":0"#), "« tentative » vaut 0, hors de son domaine"),
        (en_trop(BAIL, "essai"), "champ en trop pour main.bail : « essai »"),
        // main.perimee
        (
            PERIMEE.replace(r#""couleur":"vert""#, r#""couleur":"bleu""#),
            "« couleur » vaut \"bleu\", hors de son domaine : vert ou rouge",
        ),
        (
            PERIMEE.replace(r#""foi":"constate""#, r#""foi":"prouve""#),
            "« foi » vaut \"prouve\", hors de son domaine : constate ou declare",
        ),
        (PERIMEE.replace(r#","tentative":1"#, ""), "« tentative » manque"),
        (PERIMEE.replace(r#""verif":null"#, r#""verif":7"#), "« verif » manque, ou n'est ni une chaîne ni null"),
        (en_trop(PERIMEE, "sha"), "champ en trop pour main.perimee : « sha »"),
        // vague.fusion : une fusion déclarée serait lue comme un fait
        (
            FUSION.replace(r#""foi":"constate""#, r#""foi":"declare""#),
            "« foi » vaut \"declare\", hors de son domaine : constate",
        ),
        (
            FUSION.replace(r#""foi":"constate""#, r#""foi":"prouve""#),
            "« foi » vaut \"prouve\", hors de son domaine : constate",
        ),
        (FUSION.replace(r#","foi":"constate""#, ""), "« foi » manque"),
        (FUSION.replace(r#""foi":"constate""#, r#""foi":null"#), "« foi » manque ou n'est pas une chaîne"),
        (FUSION.replace(r#""verif":"ok""#, r#""verif":null"#), "« verif » manque ou n'est pas une chaîne"),
        (FUSION.replace(r#""main":1"#, r#""main":0"#), "« main » vaut 0, hors de son domaine"),
        (en_trop(FUSION, "essai"), "champ en trop pour vague.fusion : « essai »"),
        // vague.refus : un code hors de 1000 à 9999, un champ de moins ou de plus (un refus n'a pas de `foi`)
        (REFUS.replace("7007", "999"), "« code » vaut 999, hors de son domaine : un entier de 1000 à 9999"),
        (REFUS.replace("7007", "10000"), "« code » vaut 10000, hors de son domaine : un entier de 1000 à 9999"),
        (REFUS.replace("7007", "0"), "« code » vaut 0, hors de son domaine"),
        (REFUS.replace("7007", "18446744073709551615"), "« code » vaut 18446744073709551615, hors de son domaine"),
        (REFUS.replace("7007", r#""7007""#), "« code » manque ou n'est pas un entier positif"),
        (REFUS.replace("7007", "null"), "« code » manque ou n'est pas un entier positif"),
        (REFUS.replace("7007", "7007.5"), "« code » n'est pas une valeur plate"),
        (REFUS.replace(r#","correction":"c""#, ""), "« correction » manque"),
        (REFUS.replace(r#""regle":"r""#, r#""regle":null"#), "« regle » manque ou n'est pas une chaîne"),
        (REFUS.replace(r#","verif":null"#, ""), "« verif » manque"),
        (en_trop(REFUS, "foi"), "champ en trop pour vague.refus : « foi »"),
    ];
    // Chaque ligne fausse est suivie d'une ligne juste, de l'un des six types : la séance n'a aucun trou.
    let justes = [ASSEMBLAGE, TACHE, BAIL, PERIMEE, FUSION, REFUS];
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, justes[i % justes.len()]));
    }
    let dir = dossier("champs-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout), "la preuve devait dire « {bout} » : {refus}");
    }
    let lus: Vec<&str> = notes.iter().map(|note| note.evenement.genre()).collect();
    let attendus: Vec<&str> = (0..cas.len())
        .map(|i| ["vague.assemblage", "vague.tache", "main.bail", "main.perimee", "vague.fusion", "vague.refus"][i % 6])
        .collect();
    assert_eq!(lus, attendus, "les lignes justes se lisent");
    // Les bornes se lisent : un code de 1000 et un de 9999, un bail sans décimale.
    poser(
        &dir,
        &[
            &ligne_s(1, &REFUS.replace("7007", "1000")),
            &ligne_s(2, &REFUS.replace("7007", "9999")),
            &ligne_s(3, &BAIL.replace("21:03:00.123Z", "21:03:00Z")),
            &ligne_s(4, &TACHE.replace(r#""depend":"x\ny""#, r#""depend":"""#)),
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (4, 0), "{ecarts:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_type_de_forge_aux_champs_faux_est_mal_forme() {
    const SHA: &str = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    const POUSSEE: &str = concat!(
        r#""type":"forge.poussee","vague":"v","branche":"kraken/vague-v","sha":"9f2c41a7d3b"#,
        r#"85e60c1a49d27b8e3f5a60d1c2b34""#,
    );
    const PR: &str = concat!(
        r#""type":"forge.pr","vague":"v","branche":"kraken/vague-v","base":"main","sha":"9f"#,
        r#"2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34","numero":12,"url":"https://github.com/ex"#,
        r#"emple/depot/pull/12","foi":"constate""#,
    );
    const CI: &str = concat!(
        r#""type":"forge.ci","vague":"v","numero":12,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a"#,
        r#"60d1c2b34","couleur":"rouge","texte":"test : FAILURE","foi":"constate""#,
    );
    const GEL: &str =
        concat!(r#""type":"main.gel","vague":"v","main":2,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a6"#, r#"0d1c2b34""#,);
    const EMPREINTES: &str = concat!(
        r#""type":"main.empreintes","vague":"v","main":2,"sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a6"#,
        r#"0d1c2b34","fichiers":"src/a.rs 11""#,
    );
    const REFUS: &str = r#""type":"forge.refus","vague":"v","code":8005,"regle":"r","preuve":"p","correction":"c""#;
    const RANGEMENT: &str = r#""type":"forge.rangement","vague":"v","retires":"a\nb""#;
    let en_trop = |corps: &str, cle: &str| format!(r#"{corps},"{cle}":1"#);
    let sans_sha = |corps: &str| corps.replace(&format!(r#","sha":"{SHA}""#), "");
    let avec_sha = |corps: &str, faux: &str| corps.replace(SHA, faux);
    let hors_domaine = "hors de son domaine : 40 hexadécimaux minuscules";
    let sha_en_trop = format!("{SHA}0");
    let sha_majuscule = SHA.to_uppercase();
    let sha_non_hexa = SHA.replacen('9', "g", 1);
    // Chaque ligne fausse, et un bout de ce que sa preuve dit d'elle.
    let cas = vec![
        // forge.poussee : un sha de 40 hexadécimaux minuscules
        (sans_sha(POUSSEE), "« sha » manque".to_string()),
        (avec_sha(POUSSEE, "9f2c41a7"), format!("« sha » vaut \"9f2c41a7\", {hors_domaine}")),
        (avec_sha(POUSSEE, &sha_majuscule), hors_domaine.to_string()),
        (avec_sha(POUSSEE, &sha_en_trop), hors_domaine.to_string()),
        (avec_sha(POUSSEE, &sha_non_hexa), hors_domaine.to_string()),
        (avec_sha(POUSSEE, ""), hors_domaine.to_string()),
        (POUSSEE.replace(&format!(r#""sha":"{SHA}""#), r#""sha":12"#), "« sha » manque ou n'est pas une chaîne".into()),
        (
            POUSSEE.replace(r#""branche":"kraken/vague-v""#, r#""branche":null"#),
            "« branche » manque ou n'est pas".into(),
        ),
        (POUSSEE.replace(r#""vague":"v","#, ""), "« vague » manque".to_string()),
        (en_trop(POUSSEE, "essai"), "champ en trop pour forge.poussee : « essai »".to_string()),
        // main.gel : une main et un sha de 40 hexadécimaux minuscules
        (sans_sha(GEL), "« sha » manque".to_string()),
        (avec_sha(GEL, "9f2c41a7"), format!("« sha » vaut \"9f2c41a7\", {hors_domaine}")),
        (avec_sha(GEL, &sha_majuscule), hors_domaine.to_string()),
        (avec_sha(GEL, &sha_en_trop), hors_domaine.to_string()),
        (avec_sha(GEL, &sha_non_hexa), hors_domaine.to_string()),
        (GEL.replace(r#","main":2"#, ""), "« main » manque".to_string()),
        (GEL.replace(r#""main":2"#, r#""main":"2""#), "« main » manque ou n'est pas".into()),
        (GEL.replace(r#""vague":"v","#, ""), "« vague » manque".to_string()),
        (en_trop(GEL, "foi"), "champ en trop pour main.gel : « foi »".to_string()),
        // main.empreintes : le sha du commit gelé, et une liste en chaîne sans élément vide
        (sans_sha(EMPREINTES), "« sha » manque".to_string()),
        (avec_sha(EMPREINTES, "9f2c41a7"), format!("« sha » vaut \"9f2c41a7\", {hors_domaine}")),
        (EMPREINTES.replace(r#","fichiers":"src/a.rs 11""#, ""), "« fichiers » manque".to_string()),
        (
            EMPREINTES.replace(r#""src/a.rs 11""#, r#""src/a.rs 11\n\nb.txt 22""#),
            "« fichiers » est une liste d'un élément par ligne, sans élément vide".to_string(),
        ),
        (en_trop(EMPREINTES, "foi"), "champ en trop pour main.empreintes : « foi »".to_string()),
        // forge.pr : un numéro positif, une foi dans son domaine, tous ses champs
        (
            PR.replace(r#""numero":12"#, r#""numero":0"#),
            "« numero » vaut 0, hors de son domaine : un entier positif".to_string(),
        ),
        (PR.replace(r#""numero":12"#, r#""numero":"12""#), "« numero » manque ou n'est pas un entier positif".into()),
        (PR.replace(r#""numero":12"#, r#""numero":-12"#), "« numero » n'est pas une valeur plate".into()),
        (PR.replace(r#""numero":12"#, r#""numero":1.5"#), "« numero » n'est pas une valeur plate".into()),
        (PR.replace(r#","numero":12"#, ""), "« numero » manque".to_string()),
        (
            PR.replace(r#""foi":"constate""#, r#""foi":"prouve""#),
            "« foi » vaut \"prouve\", hors de son domaine : constate ou declare".to_string(),
        ),
        (PR.replace(r#","foi":"constate""#, ""), "« foi » manque".to_string()),
        (PR.replace(r#""foi":"constate""#, r#""foi":null"#), "« foi » manque ou n'est pas une chaîne".into()),
        (PR.replace(r#""base":"main""#, r#""base":null"#), "« base » manque ou n'est pas une chaîne".into()),
        (PR.replace(r#","url":"https://github.com/exemple/depot/pull/12""#, ""), "« url » manque".to_string()),
        (avec_sha(PR, "x"), format!("« sha » vaut \"x\", {hors_domaine}")),
        (en_trop(PR, "essai"), "champ en trop pour forge.pr : « essai »".to_string()),
        // forge.ci : une couleur et une foi dans leur domaine, un numéro positif, un sha, tous ses champs
        (
            CI.replace(r#""couleur":"rouge""#, r#""couleur":"bleu""#),
            "« couleur » vaut \"bleu\", hors de son domaine : vert ou rouge".to_string(),
        ),
        (
            CI.replace(r#""foi":"constate""#, r#""foi":"prouve""#),
            "« foi » vaut \"prouve\", hors de son domaine : constate ou declare".to_string(),
        ),
        (CI.replace(r#""numero":12"#, r#""numero":0"#), "« numero » vaut 0, hors de son domaine".to_string()),
        (CI.replace(r#","texte":"test : FAILURE""#, ""), "« texte » manque".to_string()),
        (CI.replace(r#""texte":"test : FAILURE""#, r#""texte":null"#), "« texte » manque ou n'est pas".into()),
        (sans_sha(CI), "« sha » manque".to_string()),
        (avec_sha(CI, &sha_majuscule), hors_domaine.to_string()),
        (en_trop(CI, "main"), "champ en trop pour forge.ci : « main »".to_string()),
        // forge.refus : un code de 1000 à 9999, ses quatre textes, et ni `main`, ni `foi`, ni `verif`
        (REFUS.replace("8005", "999"), "« code » vaut 999, hors de son domaine : un entier de 1000 à 9999".to_string()),
        (REFUS.replace("8005", "10000"), "« code » vaut 10000, hors de son domaine".to_string()),
        (REFUS.replace("8005", r#""8005""#), "« code » manque ou n'est pas un entier positif".to_string()),
        (REFUS.replace(r#","correction":"c""#, ""), "« correction » manque".to_string()),
        (REFUS.replace(r#""regle":"r""#, r#""regle":null"#), "« regle » manque ou n'est pas une chaîne".into()),
        (en_trop(REFUS, "main"), "champ en trop pour forge.refus : « main »".to_string()),
        (en_trop(REFUS, "foi"), "champ en trop pour forge.refus : « foi »".to_string()),
        (format!(r#"{REFUS},"verif":null"#), "champ en trop pour forge.refus : « verif »".to_string()),
        // forge.rangement : une liste d'un élément par ligne, sans élément vide
        (
            RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":"a\n\nb""#),
            "« retires » est une liste d'un élément par ligne, sans élément vide".to_string(),
        ),
        (RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":"a\n""#), "sans élément vide".to_string()),
        (RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":"\na""#), "sans élément vide".to_string()),
        (RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":"\n""#), "sans élément vide".to_string()),
        (RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":null"#), "« retires » manque ou n'est pas".into()),
        (
            RANGEMENT.replace(r#""retires":"a\nb""#, r#""retires":["a","b"]"#),
            "« retires » n'est pas une valeur plate".to_string(),
        ),
        (RANGEMENT.replace(r#","retires":"a\nb""#, ""), "« retires » manque".to_string()),
        (en_trop(RANGEMENT, "essai"), "champ en trop pour forge.rangement : « essai »".to_string()),
    ];
    // Chaque ligne fausse est suivie d'une ligne juste, de l'un des cinq types : la séance n'a aucun trou.
    let justes = [POUSSEE, PR, CI, REFUS, RANGEMENT];
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, justes[i % justes.len()]));
    }
    let dir = dossier("forge-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout.as_str()), "la preuve devait dire « {bout} » : {refus}");
    }
    let lus: Vec<&str> = notes.iter().map(|note| note.evenement.genre()).collect();
    let attendus: Vec<&str> = (0..cas.len())
        .map(|i| ["forge.poussee", "forge.pr", "forge.ci", "forge.refus", "forge.rangement"][i % 5])
        .collect();
    assert_eq!(lus, attendus, "les lignes justes se lisent");
    // Les bornes se lisent : le numéro 1 et le plus grand u64 (aucune borne de 10 000), le plus petit et le plus grand
    // code, une foi déclarée, une liste vide.
    poser(
        &dir,
        &[
            &ligne_s(1, &PR.replace(r#""numero":12"#, r#""numero":1"#)),
            &ligne_s(2, &CI.replace(r#""numero":12"#, r#""numero":18446744073709551615"#)),
            &ligne_s(3, &REFUS.replace("8005", "1000")),
            &ligne_s(4, &REFUS.replace("8005", "9999")),
            &ligne_s(5, &PR.replace(r#""foi":"constate""#, r#""foi":"declare""#)),
            &ligne_s(6, &CI.replace(r#""foi":"constate""#, r#""foi":"declare""#)),
            &ligne_s(7, &RANGEMENT.replace(r#""a\nb""#, r#""""#)),
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (7, 0), "{ecarts:?}");
    let Evenement::Rangement { retires, .. } = &notes[6].evenement else { panic!("{:?}", notes[6]) };
    assert!(retires.is_empty(), "la chaîne vide est une liste sans élément : {retires:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn main_rouge_s_ecrit_et_se_relit_a_l_identique_un_fichier_a_blanc_compris() {
    const BASE: &str = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let (h1, h2) = (hex(&sha256(b"a")), hex(&sha256(b"b")));
    let verif = "test x ... FAILED\né « ç »\t\"q\"\n";
    let rouge = |fichiers: Vec<String>| Evenement::Rouge {
        vague: "v".into(),
        main: 2,
        tentative: 1,
        base: BASE.into(),
        fichiers,
        verif: verif.into(),
    };
    let evenements = [rouge(vec![format!("tests/un fichier.rs {h1}"), format!("b.txt {h2}")]), rouge(vec![])];
    let dir = dossier("rouge-aller-retour");
    let fil = Fil::seance(&dir, "humain");
    for evenement in &evenements {
        fil.noter(evenement).unwrap();
    }
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    let corps = |fichiers: &str| {
        format!(
            concat!(
                r#""type":"main.rouge","vague":"v","main":2,"tentative":1,"base":"{base}","fichiers":"{fichiers}","#,
                r#""verif":"test x ... FAILED\né « ç »\t\"q\"\n"}}"#,
            ),
            base = BASE,
            fichiers = fichiers,
        )
    };
    let attendus = [corps(&format!(r"tests/un fichier.rs {h1}\nb.txt {h2}")), corps("")];
    assert_eq!(brut.lines().count(), 2);
    for (ligne, corps) in brut.lines().zip(&attendus) {
        assert!(ligne.ends_with(&format!(r#","origine_foi":"declare",{corps}"#)), "{ligne}");
    }
    // L'en-tête du module dit le type, avec les champs de son corps dans l'ordre où il s'écrit.
    let en_tete: String = include_str!("../fil.rs")
        .lines()
        .map_while(|ligne| ligne.strip_prefix("//!"))
        .map(|ligne| format!("{} ", ligne.trim_start()))
        .collect();
    let cles: Vec<&str> = evenements[0].champs().iter().map(|(cle, _)| *cle).collect();
    assert_eq!(cles, ["vague", "main", "tentative", "base", "fichiers", "verif"]);
    assert!(en_tete.contains("- `main.rouge {vague, main, tentative, base, fichiers, verif}`"), "l'en-tête le dit");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(notes.into_iter().map(|note| note.evenement).collect::<Vec<_>>(), evenements);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn main_rouge_aux_champs_faux_est_mal_forme() {
    const SHA: &str = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let h = hex(&sha256(b"a"));
    let rouge = |base: &str, fichiers: &str| {
        format!(
            concat!(
                r#""type":"main.rouge","vague":"v","main":2,"tentative":1,"base":"{base}","fichiers":"{fichiers}","#,
                r#""verif":"x""#,
            ),
            base = base,
            fichiers = fichiers,
        )
    };
    let juste = rouge(SHA, &format!("src/a b.rs {h}"));
    let hors_domaine = "hors de son domaine : 40 hexadécimaux minuscules";
    let element = "`fichiers` has an element that is not `<file> <sha256>` (a non-empty file, then 64 lowercase \
                   hexadecimal digits)";
    let (h63, h65) = (&h[..63], format!("{h}0"));
    let majuscule = h.to_uppercase();
    let non_hexa = h.replacen('a', "g", 1);
    // Chaque ligne fausse, et un bout de ce que sa preuve dit d'elle.
    let cas: Vec<(String, String)> = vec![
        (rouge("9f2c41a7", ""), format!("« base » vaut \"9f2c41a7\", {hors_domaine}")),
        (rouge(&SHA.to_uppercase(), ""), hors_domaine.to_string()),
        (rouge(&format!("{SHA}0"), ""), hors_domaine.to_string()),
        (rouge(&SHA.replacen('9', "g", 1), ""), hors_domaine.to_string()),
        (juste.replace(&format!(r#","base":"{SHA}""#), ""), "« base » manque".to_string()),
        (rouge(SHA, "src/a.rs"), format!("{element}: \"src/a.rs\"")),
        (rouge(SHA, &format!("src/a.rs {h63}")), format!("{element}: \"src/a.rs {h63}\"")),
        (rouge(SHA, &format!("src/a.rs {h65}")), element.to_string()),
        (rouge(SHA, &format!("src/a.rs {majuscule}")), format!("{element}: \"src/a.rs {majuscule}\"")),
        (rouge(SHA, &format!("src/a.rs {non_hexa}")), element.to_string()),
        (rouge(SHA, &format!(" {h}")), element.to_string()),
        (rouge(SHA, &format!("src/a.rs {h}\\nb.rs")), format!("{element}: \"b.rs\"")),
        // Un caractère de contrôle de l'élément fautif s'écrit : la preuve ne le porte pas tel quel.
        (rouge(SHA, &format!("a\\tb\\u0001 {h63}")), format!("{element}: \"a\\tb\\u{{1}} {h63}\"")),
        (
            rouge(SHA, &format!("src/a.rs {h}\\n\\nb.rs {h}")),
            "« fichiers » est une liste d'un élément par ligne, sans élément vide".to_string(),
        ),
        (juste.replace(r#","tentative":1"#, ""), "« tentative » manque".to_string()),
        (juste.replace(r#","verif":"x""#, ""), "« verif » manque".to_string()),
        (juste.replace(r#""verif":"x""#, r#""verif":null"#), "« verif » manque ou n'est pas une chaîne".to_string()),
        (format!(r#"{juste},"foi":"constate""#), "champ en trop pour main.rouge : « foi »".to_string()),
    ];
    // Chaque ligne fausse est suivie d'une ligne juste : la séance n'a aucun trou.
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, &juste));
    }
    let dir = dossier("rouge-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout.as_str()), "la preuve devait dire « {bout} » : {refus}");
    }
    assert!(notes.iter().all(|note| note.evenement.genre() == "main.rouge"), "les lignes justes se lisent");
    assert_eq!(notes.len(), cas.len());
    // Une ligne juste se lit : un fichier à blanc compris, deux éléments.
    poser(&dir, &[&ligne_s(1, &rouge(SHA, &format!("src/a b.rs {h}\\n{h} {h}")))]);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (1, 0), "{ecarts:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_verdict_de_relecture_aux_champs_faux_est_mal_forme() {
    const SHA: &str = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let verdict = |sha: &str, profil: &str| {
        format!(
            r#""type":"vague.verdict","vague":"r","relue":"v","sha":{sha},"profil":{profil},"#,
            // les deux autres jetons sont nuls : seuls `sha` et `profil` varient ici
        ) + r#""empreinte":null,"modele":null"#
    };
    let juste = verdict(&format!("\"{SHA}\""), "\"claude\"");
    let hors_domaine = "hors de son domaine";
    let cas = vec![
        (verdict("\"9f2c41a7\"", "\"claude\""), hors_domaine.to_string()),
        (verdict(&format!("\"{SHA}0\""), "\"claude\""), hors_domaine.to_string()),
        (verdict(&format!("\"{}\"", SHA.to_uppercase()), "\"claude\""), hors_domaine.to_string()),
        (verdict(&format!("\"{}\"", SHA.replacen('9', "g", 1)), "\"claude\""), hors_domaine.to_string()),
        (verdict("12", "\"claude\""), "« sha » manque ou n'est pas une chaîne".to_string()),
        (juste.replace(r#","sha":"9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34""#, ""), "« sha » manque".to_string()),
        (verdict(&format!("\"{SHA}\""), "\"\""), "« profil » vaut \"\", hors de son domaine".to_string()),
        (verdict(&format!("\"{SHA}\""), "\"deux mots\""), "« profil » vaut \"deux mots\"".to_string()),
        (juste.replace(r#","profil":"claude""#, ""), "« profil » manque".to_string()),
        (format!(r#"{juste},"foi":"declare""#), "champ en trop pour vague.verdict : « foi »".to_string()),
        (juste.replace(r#""relue":"v""#, r#""relue":"r""#), "« relue » vaut \"r\"".to_string()),
    ];
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, &juste));
    }
    let dir = dossier("verdict-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout.as_str()), "la preuve devait dire « {bout} » : {refus}");
    }
    assert_eq!(notes.len(), cas.len(), "les lignes justes se lisent");
    // `null` est accepté pour les trois jetons, et une ligne sans `foi` est la bonne forme.
    let nul = juste.replace(r#""profil":"claude""#, r#""profil":null"#);
    poser(&dir, &[&ligne_s(1, &nul)]);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let Evenement::VerdictRelecture { profil, empreinte, modele, .. } = &notes[0].evenement else { panic!() };
    assert_eq!((profil, empreinte, modele), (&None, &None, &None));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn au_rejeu_le_verdict_se_garde_sur_la_vague_relue() {
    let sha = |c: char| c.to_string().repeat(40);
    let verdict = |sha: String, profil: Option<&str>| Evenement::VerdictRelecture {
        vague: "r".into(),
        relue: "v".into(),
        sha,
        profil: profil.map(Into::into),
        empreinte: profil.map(|_| EMPREINTE.into()),
        modele: profil.map(|_| "sonnet".into()),
    };
    let vagues = etat(&[verdict(sha('a'), Some("claude")), verdict(sha('b'), None)]);
    let ids: Vec<&str> = vagues.iter().map(|vague| vague.id.as_str()).collect();
    assert_eq!(ids, ["r", "v"], "la vague du relecteur et la vague relue sont nommées");
    assert_eq!(vagues[0].jugement, None, "le relecteur ne porte pas le verdict");
    // Le dernier verdict noté l'emporte.
    let jugement = vagues[1].jugement.as_ref().unwrap();
    assert_eq!((jugement.relecteur.as_str(), jugement.sha.as_str()), ("r", sha('b').as_str()));
    assert_eq!((&jugement.profil, &jugement.empreinte, &jugement.modele), (&None, &None, &None));
    let un = etat(&[verdict(sha('a'), Some("claude"))]);
    assert_eq!(un[1].jugement.as_ref().unwrap().profil.as_deref(), Some("claude"));
}

#[test]
fn au_rejeu_la_forge_ne_garde_que_le_constate() {
    let sha = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let ouverture = |vague: &str| Evenement::Vague {
        vague: vague.into(),
        n: 1,
        depot: "/srv/depot".into(),
        verifier: None,
        objectif: "o".into(),
    };
    let branche_de = |vague: &str| format!("kraken/vague-{vague}");
    let assemblage = |vague: &str| Evenement::Assemblage {
        vague: vague.into(),
        branche: branche_de(vague),
        base: sha.into(),
        preuve: "cargo test".into(),
    };
    let poussee = |vague: &str| Evenement::Poussee { vague: vague.into(), branche: "b".into(), sha: sha.into() };
    let pr = |vague: &str, branche: &str, numero: u64, foi| Evenement::Pr {
        vague: vague.into(),
        branche: branche.into(),
        base: "main".into(),
        sha: sha.into(),
        numero,
        url: format!("https://github.com/exemple/depot/pull/{numero}"),
        foi,
    };
    let ci = |vague: &str, couleur, foi| Evenement::Ci {
        vague: vague.into(),
        numero: 12,
        sha: sha.into(),
        couleur,
        texte: "t".into(),
        foi,
    };
    let refus = |vague: &str, code: u16| Evenement::RefusForge {
        vague: vague.into(),
        code,
        regle: format!("règle {code}"),
        preuve: format!("preuve {code}"),
        correction: format!("correction {code}"),
    };
    let rangement = |vague: &str| Evenement::Rangement { vague: vague.into(), retires: vec!["x".into()] };
    let (constate, declare) = (Foi::Constate, Foi::Declare);
    let (vert, rouge) = (Couleur::Vert, Couleur::Rouge);
    let evenements = vec![
        // a : aucune note de forge.
        ouverture("a"),
        // b : une PR et une CI déclarées, que le rejeu ne prend pas.
        ouverture("b"),
        pr("b", &branche_de("b"), 12, declare),
        ci("b", vert, declare),
        // c et d : la forge a agi, sans rien dire que le rejeu garde.
        ouverture("c"),
        poussee("c"),
        ouverture("d"),
        rangement("d"),
        // e : la dernière PR constatée, la dernière CI constatée (la déclarée qui la suit ne la remplace pas) et le
        // premier refus.
        ouverture("e"),
        assemblage("e"),
        pr("e", &branche_de("e"), 12, constate),
        pr("e", &branche_de("e"), 13, constate),
        pr("e", &branche_de("e"), 14, declare),
        ci("e", rouge, constate),
        ci("e", vert, constate),
        ci("e", rouge, declare),
        refus("e", 8004),
        refus("e", 8005),
        // f : une relance de e, dont la PR nomme la branche d'assemblage de e.
        ouverture("f"),
        assemblage("f"),
        pr("f", &branche_de("e"), 13, constate),
        ci("f", vert, constate),
        // g : la même PR, déclarée ; h : la sienne ; i : une branche qui n'est celle d'aucune vague du fil.
        ouverture("g"),
        pr("g", &branche_de("e"), 13, declare),
        ouverture("h"),
        assemblage("h"),
        pr("h", &branche_de("h"), 13, constate),
        ouverture("i"),
        pr("i", "kraken/vague-inconnue", 13, constate),
    ];
    let vagues = etat(&evenements);
    let de = |id: &str| vagues.iter().find(|vague| vague.id == id).unwrap_or_else(|| panic!("{id} : {vagues:?}"));
    assert_eq!(de("a").forge, None);
    assert_eq!(de("b").forge, None, "déclarées, la PR et la CI ne comptent pas");
    assert_eq!(de("c").forge, Some(Forge::default()), "une poussée dit seulement que la forge a agi");
    assert_eq!(de("d").forge, Some(Forge::default()), "un rangement aussi");
    let pr_de_e = Pr { numero: 13, url: "https://github.com/exemple/depot/pull/13".into(), branche: branche_de("e") };
    let premier_refus = refus_en_clair(8004, "règle 8004", "preuve 8004", "correction 8004");
    let e = Forge { pr: Some(pr_de_e), ci: Some(vert), refus: Some(premier_refus) };
    assert_eq!(de("e").forge, Some(e), "la dernière PR et la dernière CI constatées, le premier refus");
    // Le lien d'une relance : f relance e, car sa PR nomme la branche d'assemblage de e ; aucune autre ne relance.
    let relances: Vec<(&str, Option<&str>)> =
        vagues.iter().map(|vague| (vague.id.as_str(), vague.relance_de.as_deref())).collect();
    let attendu = [
        ("a", None),
        ("b", None),
        ("c", None),
        ("d", None),
        ("e", None),
        ("f", Some("e")),
        ("g", None),
        ("h", None),
        ("i", None),
    ];
    assert_eq!(relances, attendu);
    // Les vagues restent dans l'ordre où le fil les nomme ; rien ne touche aux mains.
    assert_eq!(vagues.iter().map(|vague| vague.id.as_str()).collect::<String>(), "abcdefghi");
    assert!(vagues.iter().all(|vague| vague.mains.is_empty()));
}

#[test]
fn un_type_de_m5_aux_champs_faux_est_mal_forme() {
    const DECOUPAGE: &str = concat!(
        r#""type":"vague.decoupage","vague":"d","taches":2,"empreinte":"e3b0c44298fc1c149af"#,
        r#"bf4c8996fb92427ae41e4649b934ca495991b7852b855","plan":"plan 1""#,
    );
    const JOUE: &str = r#""type":"vague.joue","vague":"v","decoupage":"d""#;
    const RELECTURE: &str =
        r#""type":"vague.relecture","vague":"r","relue":"v","n":2,"bloquantes":1,"remarques":"remarques 1""#;
    const CORRECTION: &str = r#""type":"vague.correction","vague":"c","corrige":"v""#;
    const RELECTEUR: &str = r#""type":"vague.relecteur","vague":"r","relue":"v""#;
    const FOURNISSEUR: &str = concat!(
        r#""type":"main.fournisseur","vague":"v","main":1,"tentative":1,"fournisseur":"api."#,
        r#"deepseek.com","foi":"declare""#,
    );
    let en_trop = |corps: &str, cle: &str| format!(r#"{corps},"{cle}":1"#);
    let sha_hors_domaine = "hors de son domaine : 64 hexadécimaux minuscules";
    let empreinte = |par: &str| DECOUPAGE.replace(EMPREINTE, par);
    let jeton_hors_domaine = "hors de son domaine : un jeton ASCII, non vide, sans blanc";
    let autre_vague = "la vague de la ligne, hors de son domaine : une autre vague";
    // Chaque ligne fausse, et un bout de ce que sa preuve dit d'elle.
    let cas = vec![
        // vague.decoupage : un nombre de tâches de 1 à la borne, un SHA-256, un plan en chaîne
        (
            DECOUPAGE.replace(r#""taches":2"#, r#""taches":0"#),
            "« taches » vaut 0, hors de son domaine : un entier de 1 à 10000".to_string(),
        ),
        (DECOUPAGE.replace(r#""taches":2"#, r#""taches":10001"#), "« taches » vaut 10001, hors".to_string()),
        (
            DECOUPAGE.replace(r#""taches":2"#, r#""taches":"2""#),
            "« taches » manque ou n'est pas un entier positif".to_string(),
        ),
        (DECOUPAGE.replace(r#""taches":2"#, r#""taches":-1"#), "« taches » n'est pas une valeur plate".to_string()),
        (DECOUPAGE.replace(r#""taches":2,"#, ""), "« taches » manque".to_string()),
        (empreinte("e3b0c442"), format!("« empreinte » vaut \"e3b0c442\", {sha_hors_domaine}")),
        (empreinte(&EMPREINTE.to_uppercase()), sha_hors_domaine.to_string()),
        (empreinte(&format!("{EMPREINTE}0")), sha_hors_domaine.to_string()),
        (empreinte(&EMPREINTE.replacen('e', "g", 1)), sha_hors_domaine.to_string()),
        (empreinte(""), sha_hors_domaine.to_string()),
        (
            DECOUPAGE.replace(&format!(r#""empreinte":"{EMPREINTE}""#), r#""empreinte":null"#),
            "« empreinte » manque ou n'est pas une chaîne".to_string(),
        ),
        (
            DECOUPAGE.replace(r#""plan":"plan 1""#, r#""plan":null"#),
            "« plan » manque ou n'est pas une chaîne".to_string(),
        ),
        (DECOUPAGE.replace(r#","plan":"plan 1""#, ""), "« plan » manque".to_string()),
        (DECOUPAGE.replace(r#""vague":"d","#, ""), "« vague » manque".to_string()),
        (en_trop(DECOUPAGE, "essai"), "champ en trop pour vague.decoupage : « essai »".to_string()),
        // vague.joue : un découpage qui est une autre vague
        (JOUE.replace(r#""decoupage":"d""#, r#""decoupage":"v""#), format!("« decoupage » vaut \"v\", {autre_vague}")),
        (
            JOUE.replace(r#""decoupage":"d""#, r#""decoupage":null"#),
            "« decoupage » manque ou n'est pas une chaîne".to_string(),
        ),
        (JOUE.replace(r#","decoupage":"d""#, ""), "« decoupage » manque".to_string()),
        (en_trop(JOUE, "essai"), "champ en trop pour vague.joue : « essai »".to_string()),
        // vague.relecture : une autre vague relue, des comptes de 0 à la borne, bloquantes au plus n, un texte
        (RELECTURE.replace(r#""relue":"v""#, r#""relue":"r""#), format!("« relue » vaut \"r\", {autre_vague}")),
        (
            RELECTURE.replace(r#""n":2"#, r#""n":10001"#),
            "« n » vaut 10001, hors de son domaine : un entier de 0 à 10000".to_string(),
        ),
        (RELECTURE.replace(r#""n":2"#, r#""n":"2""#), "« n » manque ou n'est pas un entier positif".to_string()),
        (RELECTURE.replace(r#""n":2"#, r#""n":-1"#), "« n » n'est pas une valeur plate".to_string()),
        (
            RELECTURE.replace(r#""bloquantes":1"#, r#""bloquantes":3"#),
            "« bloquantes » vaut 3, hors de son domaine : un entier de 0 à 2, le « n » de la ligne".to_string(),
        ),
        (
            RELECTURE.replace(r#""n":2,"bloquantes":1"#, r#""n":10000,"bloquantes":10001"#),
            "« bloquantes » vaut 10001, hors de son domaine : un entier de 0 à 10000".to_string(),
        ),
        (
            RELECTURE.replace(r#""bloquantes":1"#, r#""bloquantes":null"#),
            "« bloquantes » manque ou n'est pas un entier positif".to_string(),
        ),
        (RELECTURE.replace(r#","bloquantes":1"#, ""), "« bloquantes » manque".to_string()),
        (RELECTURE.replace(r#","remarques":"remarques 1""#, ""), "« remarques » manque".to_string()),
        (
            RELECTURE.replace(r#""remarques":"remarques 1""#, r#""remarques":null"#),
            "« remarques » manque ou n'est pas une chaîne".to_string(),
        ),
        (en_trop(RELECTURE, "essai"), "champ en trop pour vague.relecture : « essai »".to_string()),
        // vague.correction : une autre vague corrigée
        (CORRECTION.replace(r#""corrige":"v""#, r#""corrige":"c""#), format!("« corrige » vaut \"c\", {autre_vague}")),
        (CORRECTION.replace(r#","corrige":"v""#, ""), "« corrige » manque".to_string()),
        (
            CORRECTION.replace(r#""corrige":"v""#, r#""corrige":null"#),
            "« corrige » manque ou n'est pas une chaîne".to_string(),
        ),
        (en_trop(CORRECTION, "essai"), "champ en trop pour vague.correction : « essai »".to_string()),
        // vague.relecteur : une autre vague relue
        (RELECTEUR.replace(r#""relue":"v""#, r#""relue":"r""#), format!("« relue » vaut \"r\", {autre_vague}")),
        (RELECTEUR.replace(r#","relue":"v""#, ""), "« relue » manque".to_string()),
        (
            RELECTEUR.replace(r#""relue":"v""#, r#""relue":null"#),
            "« relue » manque ou n'est pas une chaîne".to_string(),
        ),
        (RELECTEUR.replace(r#""vague":"r","#, ""), "« vague » manque".to_string()),
        (en_trop(RELECTEUR, "essai"), "champ en trop pour vague.relecteur : « essai »".to_string()),
        // main.fournisseur : toujours déclaré, un jeton ASCII sans blanc, main et tentative de 1 à la borne
        (
            FOURNISSEUR.replace(r#""foi":"declare""#, r#""foi":"constate""#),
            "« foi » vaut \"constate\", hors de son domaine : declare".to_string(),
        ),
        (FOURNISSEUR.replace(r#","foi":"declare""#, ""), "« foi » manque".to_string()),
        (
            FOURNISSEUR.replace(r#""fournisseur":"api.deepseek.com""#, r#""fournisseur":"""#),
            format!("« fournisseur » vaut \"\", {jeton_hors_domaine}"),
        ),
        (
            FOURNISSEUR.replace("api.deepseek.com", "api deepseek.com"),
            format!("« fournisseur » vaut \"api deepseek.com\", {jeton_hors_domaine}"),
        ),
        (FOURNISSEUR.replace("api.deepseek.com", "api.déepseek.com"), jeton_hors_domaine.to_string()),
        (
            FOURNISSEUR.replace(r#""fournisseur":"api.deepseek.com""#, r#""fournisseur":null"#),
            "« fournisseur » manque ou n'est pas une chaîne".to_string(),
        ),
        (FOURNISSEUR.replace(r#""main":1"#, r#""main":0"#), "« main » vaut 0, hors de son domaine".to_string()),
        (
            FOURNISSEUR.replace(r#""tentative":1"#, r#""tentative":10001"#),
            "« tentative » vaut 10001, hors de son domaine".to_string(),
        ),
        (en_trop(FOURNISSEUR, "essai"), "champ en trop pour main.fournisseur : « essai »".to_string()),
    ];
    // Chaque ligne fausse est suivie d'une ligne juste, de l'un des six types : la séance n'a aucun trou.
    let justes = [DECOUPAGE, JOUE, RELECTURE, CORRECTION, RELECTEUR, FOURNISSEUR];
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, justes[i % justes.len()]));
    }
    let dir = dossier("m5-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout.as_str()), "la preuve devait dire « {bout} » : {refus}");
    }
    let lus: Vec<&str> = notes.iter().map(|note| note.evenement.genre()).collect();
    let types =
        ["vague.decoupage", "vague.joue", "vague.relecture", "vague.correction", "vague.relecteur", "main.fournisseur"];
    let attendus: Vec<&str> = (0..cas.len()).map(|i| types[i % types.len()]).collect();
    assert_eq!(lus, attendus, "les lignes justes se lisent");
    // Les bornes se lisent : une relecture sans remarque, ou avec 10 000 dont 10 000 bloquantes ; 1 et 10 000
    // tâches ; un plan et des remarques vides ; le nom d'un hôte avec un port.
    poser(
        &dir,
        &[
            &ligne_s(1, &RELECTURE.replace(r#""n":2,"bloquantes":1"#, r#""n":0,"bloquantes":0"#)),
            &ligne_s(2, &RELECTURE.replace(r#""n":2,"bloquantes":1"#, r#""n":10000,"bloquantes":10000"#)),
            &ligne_s(3, &RELECTURE.replace(r#""remarques":"remarques 1""#, r#""remarques":"""#)),
            &ligne_s(4, &DECOUPAGE.replace(r#""taches":2"#, r#""taches":1"#)),
            &ligne_s(5, &DECOUPAGE.replace(r#""taches":2"#, r#""taches":10000"#)),
            &ligne_s(6, &DECOUPAGE.replace(r#""plan":"plan 1""#, r#""plan":"""#)),
            &ligne_s(7, &FOURNISSEUR.replace("api.deepseek.com", "[::1]:8443")),
        ],
    );
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (7, 0), "{ecarts:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn au_rejeu_les_liens_de_m5_se_gardent() {
    let ouverture = |vague: &str| Evenement::Vague {
        vague: vague.into(),
        n: 1,
        depot: "/srv/depot".into(),
        verifier: None,
        objectif: "o".into(),
    };
    let decoupage =
        |taches| Evenement::Decoupage { vague: "d".into(), taches, empreinte: EMPREINTE.into(), plan: "plan 1".into() };
    let joue = Evenement::Joue { vague: "v".into(), decoupage: "d".into() };
    let relecture = |n, bloquantes| Evenement::Relecture {
        vague: "r".into(),
        relue: "v".into(),
        n,
        bloquantes,
        remarques: "remarques 1".into(),
    };
    let correction = Evenement::Correction { vague: "c".into(), corrige: "v".into() };
    let fournisseur = |tentative, hote: &str| Evenement::Fournisseur {
        vague: "v".into(),
        main: 1,
        tentative,
        fournisseur: hote.into(),
    };
    let evenements = vec![
        ouverture("d"),
        decoupage(2),
        ouverture("v"),
        joue,
        Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 },
        fournisseur(1, "api.deepseek.com"),
        Evenement::Debut { vague: "v".into(), main: 1, tentative: 2 },
        ouverture("r"),
        Evenement::Relecteur { vague: "r".into(), relue: "v".into() },
        relecture(3, 1),
        ouverture("c"),
        correction,
        ouverture("x"),
    ];
    // Écrit au fil puis relu, le fil se rejoue sur ce que le lecteur a lu.
    let dir = dossier("liens-m5");
    let fil = Fil::seance(&dir, "humain");
    for evenement in &evenements {
        fil.noter(evenement).unwrap();
    }
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, evenements);
    let vagues = etat(&lus);
    let de = |id: &str| vagues.iter().find(|vague| vague.id == id).unwrap_or_else(|| panic!("{id} : {vagues:?}"));
    assert_eq!(vagues.iter().map(|vague| vague.id.as_str()).collect::<String>(), "dvrcx");
    // Chaque lien vit sur la vague qui le note : le plan sur le découpage, le découpage sur la vague qui joue, ce que
    // la relecture a rendu sur la relecture, la vague corrigée sur la correction.
    assert_eq!(de("d").plan_pris, Some(2));
    assert_eq!(de("v").decoupage.as_deref(), Some("d"));
    assert_eq!(de("r").relecture, Some(Relecture { relue: "v".into(), n: 3, bloquantes: 1 }));
    assert_eq!(de("c").corrige.as_deref(), Some("v"));
    assert_eq!(de("r").relit.as_deref(), Some("v"));
    // Aucune autre vague n'en porte : la vague sans lien n'a que son ouverture.
    let nue = Vague { id: "x".into(), n: 1, depot: "/srv/depot".into(), objectif: "o".into(), ..Vague::default() };
    assert_eq!(de("x"), &nue);
    assert!(de("v").plan_pris.is_none() && de("v").relecture.is_none() && de("v").corrige.is_none());
    assert!(de("d").decoupage.is_none() && de("d").corrige.is_none() && de("r").decoupage.is_none());
    assert!(de("v").relit.is_none() && de("c").relit.is_none());
    // Le fournisseur est celui de la tentative qui le note ; la seconde, qui n'en note pas, n'hérite de rien.
    let tentatives = &de("v").mains[&1].tentatives;
    assert_eq!(tentatives[&1].fournisseur.as_deref(), Some("api.deepseek.com"));
    assert_eq!(tentatives[&2].fournisseur, None);
    // Un fait redit remplace le précédent.
    let redit = etat(&[ouverture("d"), decoupage(2), decoupage(5), ouverture("r"), relecture(1, 0), relecture(4, 4)]);
    assert_eq!(redit[0].plan_pris, Some(5));
    assert_eq!(redit[1].relecture, Some(Relecture { relue: "v".into(), n: 4, bloquantes: 4 }));
    let relecteur = |relue: &str| Evenement::Relecteur { vague: "r".into(), relue: relue.into() };
    let redit = etat(&[ouverture("r"), relecteur("v"), relecteur("w")]);
    assert_eq!(redit[0].relit.as_deref(), Some("w"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_lecteur_d_avant_m5_ignore_les_types_de_m5_sans_trou() {
    // Les cinq types de M5 ne changent pas `v` : un lecteur qui ne les connaît pas les ignore, et leur `seq` compte
    // dans leur séance. Rendus inconnus, ils laissent le fil d'avant, sans trou.
    let dir = dossier("avant-m5");
    let fil = Fil::seance(&dir, "humain");
    let debut = Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 };
    let m5 = [
        Evenement::Decoupage { vague: "d".into(), taches: 1, empreinte: EMPREINTE.into(), plan: "plan 1".into() },
        Evenement::Joue { vague: "v".into(), decoupage: "d".into() },
        Evenement::Relecture { vague: "r".into(), relue: "v".into(), n: 0, bloquantes: 0, remarques: String::new() },
        Evenement::Correction { vague: "c".into(), corrige: "v".into() },
        Evenement::Fournisseur { vague: "v".into(), main: 1, tentative: 1, fournisseur: "api.deepseek.com".into() },
    ];
    fil.noter(&debut).unwrap();
    for evenement in &m5 {
        fil.noter(evenement).unwrap();
    }
    fil.noter(&debut).unwrap();
    let chemin = dir.join(FICHIER);
    let mut ancien = fs::read_to_string(&chemin).unwrap();
    for evenement in &m5 {
        let genre = evenement.genre();
        ancien = ancien.replace(&format!(r#""type":"{genre}""#), &format!(r#""type":"x.{genre}""#));
    }
    fs::write(&chemin, ancien).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "aucun trou : {ecarts:?}");
    assert_eq!(seqs(&notes), [1, 7]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn l_empreinte_du_plan_du_fil_est_celle_de_son_texte() {
    // Le fil ne recalcule pas l'empreinte d'un plan ; l'exemple de `exemples.jsonl`, lui, la porte juste.
    let lignes: Vec<&str> =
        include_str!("exemples.jsonl").lines().filter(|ligne| ligne.contains(r#""type":"vague.decoupage""#)).collect();
    assert_eq!(lignes.len(), 1, "un seul découpage dans les exemples");
    let objet = json::lire(lignes[0]).unwrap();
    let (Some(Some(Valeur::Texte(plan))), Some(Some(Valeur::Texte(empreinte)))) =
        (objet.get("plan"), objet.get("empreinte"))
    else {
        panic!("{objet:?}")
    };
    assert_eq!(&hex(&sha256(plan.as_bytes())), empreinte);
    // Le plan de l'exemple a les deux tâches que le découpage dit.
    assert_eq!(plan.lines().filter(|ligne| ligne.starts_with("tache ")).count(), 2);
    assert_eq!(objet.get("taches"), Some(&Some(Valeur::Entier(2))));
}

#[test]
fn les_exemples_du_fil_se_relisent_sans_trou_ni_doublon() {
    // Les exemples vivent dans `exemples.jsonl`, à côté de ce fichier : on les lit là, une ligne de fil par ligne.
    // Le sceau d'un exemple n'est pas une note : il a son propre essai.
    let toutes: Vec<&str> =
        include_str!("exemples.jsonl").lines().filter(|ligne| ligne.starts_with(r#"{"v":2,"#)).collect();
    let (sceaux, lignes): (Vec<&str>, Vec<&str>) =
        toutes.iter().partition(|ligne| ligne.contains(r#""type":"vague.sceau""#));
    let dir = dossier("entete");
    let octets = toutes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>().into_bytes();
    let (_, _, lus) = lire_octets(Path::new("exemples.jsonl"), &octets).unwrap();
    assert_eq!(lus.len(), sceaux.len(), "chaque sceau d'exemple se lit : {sceaux:?}");
    poser(&dir, &lignes);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    // Chaque ligne se lit, d'un type que Kraken connaît ; le lecteur ne relève que les trous des lignes omises.
    assert_eq!(notes.len(), lignes.len(), "chaque ligne d'exemple se lit : {ecarts:?}");
    assert!(ecarts.iter().all(|(_, refus)| refus.code == Code::SequenceRompue), "{ecarts:?}");
    // Les `seq` d'une séance vont croissant, sans doublon.
    let mut vus: BTreeMap<&str, Vec<u64>> = BTreeMap::new();
    for note in &notes {
        vus.entry(&note.seance).or_default().push(note.seq);
    }
    for (seance, seqs) in &vus {
        assert!(seqs.windows(2).all(|deux| deux[0] < deux[1]), "séance {seance} : des seq qui se suivent : {seqs:?}");
    }
    // Les lignes omises laissent leur place : le type de chaque ligne montrée est celui du rang que son `seq` dit dans
    // la séance entière, et aucune séance ne manque à cette table, ni n'y est de trop.
    let tentative = ["main.bail", "main.debut", "main.lancement", "main.constat", "main.fin", "main.verdict"];
    let vague = ["vague.ouverte", "vague.assemblage", "vague.tache"];
    let forge = ["forge.poussee", "forge.pr", "forge.ci", "forge.poussee", "forge.pr", "forge.ci"];
    let entieres: [(&str, Vec<&str>); 13] = [
        // Le gel d'une main prise, dans une séance à lui.
        ("1761902100000_14_1", vec!["main.gel"]),
        // Les copies : la première séance va jusqu'au verdict de sa main, la seconde jusqu'à sa fin déclarée, la
        // troisième jusqu'à l'usage de sa tentative, qui suit le constat et précède la fin.
        ("1761901200000_7_1", [&["vague.ouverte"][..], &tentative].concat()),
        ("1761901200500_8_1", [&["vague.ouverte"][..], &tentative[..5], &["main.fin", "main.verdict"]].concat()),
        ("1761901300000_10_1", [&["vague.ouverte"][..], &tentative[..4], &["main.usage"]].concat()),
        // La vague à plan : deux tâches ; la première s'assemble, la seconde expire à son lancement, et son résultat
        // tardif est périmé.
        (
            "1761901400000_9_1",
            [
                &[vague[0], vague[1], vague[2], vague[2]][..],
                &tentative,
                &["vague.fusion", "main.bail", "main.debut", "main.lancement", "vague.refus", "main.perimee"],
            ]
            .concat(),
        ),
        // La vague à PR relancée : deux vagues d'une tâche assemblée, chacune dans sa séance, et la forge dans la
        // sienne.
        ("1761901500000_11_1", [&vague[..], &tentative, &["vague.fusion"]].concat()),
        ("1761901891000_11_4", [&vague[..], &tentative, &["vague.fusion"]].concat()),
        ("1761901570500_11_2", [&forge[..], &["forge.rangement", "forge.rangement"]].concat()),
        // La vague découpée, relue et corrigée : la vague de copies du découpeur, une main ; le découpage et la
        // relecture, dans la séance de l'objectif ; la vague qui joue le plan, de deux tâches assemblées ; la vague de
        // copies du relecteur, une main ; la correction, d'une tâche assemblée.
        ("1761901700000_13_1", [&["vague.ouverte"][..], &tentative].concat()),
        ("1761901780000_13_2", vec!["vague.decoupage", "vague.relecture", "vague.verdict"]),
        (
            "1761901790000_13_4",
            [
                &["vague.ouverte", "vague.assemblage", "vague.joue", "vague.tache", "vague.tache"][..],
                &tentative,
                &["vague.fusion"],
                &tentative,
                &["vague.fusion"],
            ]
            .concat(),
        ),
        ("1761901900000_13_6", [&["vague.ouverte", "vague.relecteur"][..], &tentative].concat()),
        (
            "1761902000000_13_8",
            [
                &["vague.ouverte", "vague.assemblage", "vague.correction", "vague.tache"][..],
                &tentative,
                &["vague.fusion"],
            ]
            .concat(),
        ),
    ];
    for note in &notes {
        let Some((_, entiere)) = entieres.iter().find(|(seance, _)| *seance == note.seance) else {
            panic!("la séance {} n'a pas de rang dans la table", note.seance)
        };
        let rang = usize::try_from(note.seq).unwrap() - 1;
        assert_eq!(entiere.get(rang), Some(&note.evenement.genre()), "séance {}, seq {}", note.seance, note.seq);
    }
    for (seance, _) in &entieres {
        assert!(vus.contains_key(seance), "la table nomme la séance {seance}, que l'en-tête ne montre plus");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_perimee_declaree_porte_l_origine_de_sa_main() {
    let dir = dossier("perimee");
    let fil = Fil::seance(&dir, "humain");
    let vague = lot();
    let perimee = |main, foi| Evenement::Perimee {
        vague: vague.clone(),
        main,
        tentative: 1,
        couleur: Couleur::Vert,
        foi,
        texte: "tard".into(),
        verif: None,
    };
    // `noter` pose l'origine de la main sur la périmée déclarée, comme sur une fin déclarée, et celle de la séance
    // sur la constatée.
    fil.noter(&perimee(1, Foi::Declare)).unwrap();
    fil.noter(&perimee(2, Foi::Constate)).unwrap();
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert!(brut.lines().all(|ligne| ligne.contains(r#","origine_foi":"declare","#)), "{brut}");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let main = format!("agent:main-{vague}-1");
    let origines: Vec<&str> = notes.iter().map(|note| note.origine.as_str()).collect();
    assert_eq!(origines, [main.as_str(), "humain"]);
    // Lue, une périmée déclarée qui porte `humain`, ou l'origine d'une autre main ou d'une autre vague, est mal
    // formée. Une constatée peut porter l'origine d'une main : la règle ne vaut que pour la périmée déclarée.
    let ecrite = |seq: u64, origine: &str, foi: &str| {
        let corps = format!(
            concat!(
                r#""type":"main.perimee","vague":"v","main":1,"tentative":1,"couleur":"vert","foi":"#,
                r#""{foi}","texte":"tard","verif":null"#,
            ),
            foi = foi,
        );
        ligne_s(seq, &corps).replace(r#""origine":"humain""#, &format!(r#""origine":"{origine}""#))
    };
    let lignes = [
        ecrite(1, "agent:main-v-1", "declare"),
        ecrite(2, "humain", "declare"),
        ecrite(3, "agent:main-v-2", "declare"),
        ecrite(4, "agent:main-w-1", "declare"),
        ecrite(5, "humain", "constate"),
        ecrite(6, "agent:main-v-1", "constate"),
    ];
    let autre = dossier("perimee-lue");
    poser(&autre, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&autre).unwrap();
    assert_eq!(seqs(&notes), [1, 5, 6]);
    assert_eq!(codes(&ecarts), [(2, 1009), (3, 1009), (4, 1009)], "et aucun trou");
    let dite = "une périmée déclarée porte l'origine de sa main, agent:main-v-1, pas humain";
    assert!(ecarts[0].1.preuve.ends_with(dite), "{}", ecarts[0].1);
    for dir in [dir, autre] {
        let _ = fs::remove_dir_all(dir);
    }
}

#[test]
fn le_refus_en_clair_ecrit_les_controles_de_sa_preuve() {
    let clair = refus_en_clair(7007, "r", "a\u{1b}[31m\u{202e}\nb", "c");
    assert_eq!(clair, "refusée (7007) : r\na\\u{1b}[31m\\u{202e} ; b\nc");
}

#[test]
fn le_refus_en_clair_tient_en_trois_lignes() {
    let code = Code::Conflit;
    let refus = Refus::new(code, "a.txt\nb.txt en conflit\r\n", "Reprends la tâche.\n\nPuis relance.");
    let clair = refus_en_clair(code.numero(), code.regle(), &refus.preuve, &refus.correction);
    let regle = code.regle();
    assert_eq!(
        clair,
        format!("refusée (7007) : {regle}\na.txt ; b.txt en conflit\nReprends la tâche. ; Puis relance.")
    );
    assert_eq!(clair.lines().count(), 3, "{clair}");
    // La même mise bout à bout que le refus de `main`, dont il reprend la preuve et la correction.
    let affiche = refus.to_string();
    let lignes: Vec<&str> = affiche.lines().collect();
    assert_eq!(lignes[2].strip_prefix("  seen: "), clair.lines().nth(1));
    assert_eq!(lignes[3].strip_prefix("  fix: "), clair.lines().nth(2));
    // Une preuve ou une correction blanche laisse sa ligne vide : toujours trois lignes.
    assert_eq!(refus_en_clair(7001, "r", " \n", "c").lines().count(), 3);
    // Le rejeu tire du fil le texte même que `lancer` a dit : c'est son début, `refusée (`, qui marque une refusée.
    let dir = dossier("en-clair");
    let evenement = Evenement::Refus {
        vague: "v".into(),
        main: 1,
        code: code.numero(),
        regle: regle.into(),
        preuve: refus.preuve.clone(),
        correction: refus.correction.clone(),
        verif: None,
    };
    Fil::seance(&dir, "humain").noter(&evenement).unwrap();
    let Lecture { notes, .. } = lire(&dir).unwrap();
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    let vagues = etat(&evenements);
    let Some(Issue::Refusee { code: lu, regle, preuve, correction }) = &vagues[0].mains[&1].issue else {
        panic!("un refus : {vagues:?}")
    };
    assert_eq!(refus_en_clair(*lu, regle, preuve, correction), clair);
    assert!(clair.starts_with("refusée ("));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_substitut_seul_ou_65_niveaux_sont_illisibles_quel_que_soit_v() {
    // Les échappements se composent à l'exécution : une barre oblique inverse, puis `u` et quatre chiffres.
    const BARRE: char = '\\';
    let (haut, bas) = (format!("{BARRE}ud83d"), format!("{BARRE}ude00"));
    let avec = |v: u64, texte: &str| format!(r#"{{"v":{v},"seance":"s","seq":2,"texte":"{texte}"}}"#);
    // `niveaux` niveaux d'imbrication en tout, l'objet de la ligne compris.
    let profonde = |v: u64, niveaux: usize| {
        let dedans = niveaux - 1;
        format!(r#"{{"v":{v},"seance":"s","seq":2,"p":{}{}}}"#, "[".repeat(dedans), "]".repeat(dedans))
    };
    let dir = dossier("illisibles-v");
    for v in [2, 3] {
        let illisibles = [
            avec(v, &haut),
            avec(v, &bas),
            avec(v, &format!("{haut}x")),
            avec(v, &format!("{haut}{BARRE}u0041")),
            profonde(v, 65),
        ];
        let mut lignes = vec![debut("s", 1)];
        lignes.extend(illisibles);
        lignes.push(debut("s", 2));
        poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
        // Illisibles (1001) et non d'une autre version (1002) : la lecture va au bout, les autres lignes se lisent.
        let Lecture { notes, ecarts } = lire(&dir).unwrap_or_else(|refus| panic!("v {v} : {refus}"));
        assert_eq!(seqs(&notes), [1, 2], "v {v}");
        assert_eq!(codes(&ecarts), (2..=6).map(|ligne| (ligne, 1001)).collect::<Vec<_>>(), "v {v}");
        for (ligne, refus) in &ecarts[..4] {
            assert!(refus.preuve.contains("lone surrogate"), "v {v}, ligne {ligne} : {refus}");
        }
        assert!(ecarts[4].1.preuve.contains("more than 64 levels of nesting"), "v {v} : {}", ecarts[4].1);
    }
    // Lues jusqu'au bout, les mêmes lignes en règle (une paire de substituts entière, 64 niveaux) sont d'une autre
    // version à `v` 3, qui arrête la lecture (1002), et seulement mal formées à `v` 2 (1009), faute de `type`.
    let paire = format!("{haut}{bas}");
    for (v, attendu) in [(3, Err(Code::VersionDifferente)), (2, Ok(vec![(2, 1009)]))] {
        for juste in [avec(v, &paire), profonde(v, 64)] {
            poser(&dir, &[&debut("s", 1), &juste]);
            let lu = lire(&dir).map(|lecture| codes(&lecture.ecarts)).map_err(|refus| refus.code);
            assert_eq!(lu, attendu, "v {v} : {juste}");
        }
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn le_sceau_d_une_vague_s_ecrit_et_se_relit_a_l_identique() {
    let dir = dossier("sceau-aller-retour");
    let fil = Fil::seance(&dir, "humain");
    // Un fil vide ne se scelle pas : `octets` est un entier positif.
    assert_eq!(fil.sceller("v").unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    assert!(!dir.join(FICHIER).exists(), "rien n'est écrit");
    fil.noter(&Evenement::Correction { vague: "v".into(), corrige: "u".into() }).unwrap();
    let premier = fil.sceller("v").unwrap();
    fil.noter(&Evenement::Correction { vague: "v".into(), corrige: "t".into() }).unwrap();
    let second = fil.sceller("w").unwrap();
    let brut = fs::read(dir.join(FICHIER)).unwrap();
    let lignes: Vec<&[u8]> = brut.split_inclusive(|&octet| octet == b'\n').collect();
    // Le sceau couvre ce qui précède sa ligne, la sienne exclue.
    assert_eq!(premier.octets as usize, lignes[0].len());
    assert_eq!(premier.empreinte, hex(&sha256(lignes[0])));
    assert_eq!(second.octets as usize, lignes[..3].iter().map(|l| l.len()).sum::<usize>());
    assert_eq!(second.empreinte, hex(&sha256(&brut[..second.octets as usize])));
    // Le corps, plat, dans l'ordre du contrat : la vague, les octets, l'empreinte, rien d'autre.
    let ecrit = String::from_utf8(brut.clone()).unwrap();
    let corps = format!(
        r#""type":"vague.sceau","vague":"v","octets":{},"empreinte":"{}"}}"#,
        premier.octets, premier.empreinte
    );
    assert!(ecrit.contains(&corps), "{ecrit}");
    // Elle partage le compteur de la séance, et ne se lit pas comme une note.
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(seqs(&notes), [1, 3]);
    let (_, _, lus) = lire_octets(&dir.join(FICHIER), &brut).unwrap();
    assert_eq!(lus.iter().map(|lu| lu.sceau.clone()).collect::<Vec<_>>(), [premier, second]);
    // Chaque sceau lu sait où sa ligne commence.
    let debuts = [lignes[0].len(), lignes[..3].iter().map(|l| l.len()).sum::<usize>()];
    assert_eq!(lus.iter().map(|lu| lu.debut).collect::<Vec<_>>(), debuts);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_sceau_aux_champs_faux_est_mal_forme() {
    let vide = "0".repeat(64);
    let juste = format!(r#""type":"vague.sceau","vague":"v","octets":7,"empreinte":"{vide}""#);
    let cas = vec![
        (juste.replace(r#""octets":7"#, r#""octets":0"#), "« octets » vaut 0, hors de son domaine : un entier positif"),
        (juste.replace(r#""octets":7"#, r#""octets":-7"#), "« octets » n'est pas une valeur plate"),
        (juste.replace(r#""octets":7"#, r#""octets":1.5"#), "« octets » n'est pas une valeur plate"),
        (juste.replace(r#""octets":7"#, r#""octets":"7""#), "« octets » manque ou n'est pas un entier positif"),
        (juste.replace(r#""octets":7,"#, ""), "« octets » manque"),
        (juste.replace(&vide, &"0".repeat(63)), "64 hexadécimaux minuscules"),
        (juste.replace(&vide, &"0".repeat(65)), "64 hexadécimaux minuscules"),
        (juste.replace(&vide, &"A".repeat(64)), "64 hexadécimaux minuscules"),
        (juste.replace(&vide, &"g".repeat(64)), "64 hexadécimaux minuscules"),
        (juste.replace(&format!(r#""empreinte":"{vide}""#), r#""empreinte":null"#), "« empreinte » manque"),
        (juste.replace(&format!(r#","empreinte":"{vide}""#), ""), "« empreinte » manque"),
        (juste.replace(r#""vague":"v","#, ""), "« vague » manque"),
        (format!("{juste},\"titre\":\"x\""), "champ en trop pour vague.sceau : « titre »"),
    ];
    // Chaque ligne fausse est suivie d'une juste, de la même séance : aucun trou ne s'y voit.
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, &juste));
    }
    let dir = dossier("sceau-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(notes.is_empty(), "un sceau n'est pas une note d'événement : {notes:?}");
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout), "la preuve devait dire « {bout} » : {refus}");
    }
    let brut = fs::read(dir.join(FICHIER)).unwrap();
    let (_, _, lus) = lire_octets(&dir.join(FICHIER), &brut).unwrap();
    assert_eq!(lus.len(), cas.len(), "les lignes justes se lisent");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_sceau_tient_tant_que_son_avant_ne_change_pas() {
    let avant = b"une ligne\nune autre\n";
    let sceau = SceauVague::sur("v", avant);
    let lu = |debut| [SceauLu { sceau: sceau.clone(), debut }];
    let fil = |suite: &[u8]| [&avant[..], suite].concat();
    // Intact, un octet de plus après, ou un sceau qui suit : il tient.
    assert_eq!(premier_sceau_rompu(&fil(b"suite\n"), &lu(avant.len())), None);
    assert_eq!(premier_sceau_rompu(&fil(b"suite\n"), &lu(avant.len() + 3)), None);
    // Ses octets dépassent le début de sa propre ligne : il ne correspond pas, même si le fil est assez long.
    assert_eq!(premier_sceau_rompu(&fil(b"suite\n"), &lu(avant.len() - 1)), Some(&sceau));
    // Un octet changé, une ligne retirée, un fil plus court que ses octets.
    let mut change = fil(b"suite\n");
    change[3] ^= 1;
    assert_eq!(premier_sceau_rompu(&change, &lu(avant.len())), Some(&sceau));
    assert_eq!(premier_sceau_rompu(&avant[10..], &lu(avant.len())), Some(&sceau));
    // Les octets d'un sceau trop grands pour tenir en mémoire ne font pas paniquer.
    let enorme = SceauVague { octets: u64::MAX, ..sceau.clone() };
    assert_eq!(premier_sceau_rompu(avant, &[SceauLu { sceau: enorme.clone(), debut: usize::MAX }]), Some(&enorme));
}

#[test]
fn l_issue_d_une_vague_s_ecrit_et_se_relit_a_l_identique() {
    let dir = dossier("issue-aller-retour");
    let fil = Fil::seance(&dir, "humain");
    let ecrites = [IssueVague { vague: "v".into(), issue: 42 }, IssueVague { vague: "w".into(), issue: u64::MAX }];
    for issue in &ecrites {
        fil.noter_issue(issue).unwrap();
    }
    // Elle partage le compteur de la séance : la note qui suit prend le seq d'après.
    fil.noter(&Evenement::Correction { vague: "v".into(), corrige: "u".into() }).unwrap();
    // Le corps, plat, dans l'ordre du contrat : la vague, puis le numéro, rien d'autre.
    let ecrit = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert!(ecrit.contains(r#""type":"vague.issue","vague":"v","issue":42}"#), "{ecrit}");
    assert!(ecrit.contains(r#""type":"vague.issue","vague":"w","issue":18446744073709551615}"#), "{ecrit}");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(seqs(&notes), [3], "la correction suit les deux issues");
    assert_eq!(lire_tout(&dir).unwrap().1, ecrites);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_issue_aux_champs_faux_est_mal_formee() {
    const ISSUE: &str = r#""type":"vague.issue","vague":"v","issue":7"#;
    let cas = vec![
        (ISSUE.replace(r#""issue":7"#, r#""issue":0"#), "« issue » vaut 0, hors de son domaine : un entier positif"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":-7"#), "« issue » n'est pas une valeur plate"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":1.5"#), "« issue » n'est pas une valeur plate"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":7e0"#), "« issue » n'est pas une valeur plate"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":"7""#), "« issue » manque ou n'est pas un entier positif"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":null"#), "« issue » manque ou n'est pas un entier positif"),
        (ISSUE.replace(r#""issue":7"#, r#""issue":18446744073709551616"#), "« issue »"),
        (ISSUE.replace(r#","issue":7"#, ""), "« issue » manque"),
        (ISSUE.replace(r#""vague":"v","#, ""), "« vague » manque"),
        (format!(r#"{ISSUE},"titre":"Bonjour""#), "champ en trop pour vague.issue : « titre »"),
        (format!(r#"{ISSUE},"corps":"texte""#), "champ en trop pour vague.issue : « corps »"),
    ];
    // Chaque ligne fausse est suivie d'une juste, de la même séance : aucun trou ne s'y voit.
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, ISSUE));
    }
    let dir = dossier("issue-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(notes.is_empty(), "une issue n'est pas une note d'événement : {notes:?}");
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout), "la preuve devait dire « {bout} » : {refus}");
    }
    let lues = lire_tout(&dir).unwrap().1;
    assert_eq!(lues.len(), cas.len());
    let juste = IssueVague { vague: "v".into(), issue: 7 };
    assert!(lues.iter().all(|issue| *issue == juste), "les lignes justes se lisent");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn l_etat_d_une_vague_porte_son_issue() {
    let ouverture = |vague: &str| Evenement::Vague {
        vague: vague.into(),
        n: 1,
        depot: "/srv/depot".into(),
        verifier: None,
        objectif: "o".into(),
    };
    let issue = |vague: &str, issue| IssueVague { vague: vague.into(), issue };
    let mut vagues = etat(&[ouverture("v"), ouverture("x")]);
    assert!(vagues.iter().all(|vague| vague.issue.is_none()), "etat seul n'en porte aucune");
    porter_issues(&mut vagues, &[issue("v", 3), issue("w", 5), issue("v", 12)]);
    let de = |id: &str| vagues.iter().find(|vague| vague.id == id).unwrap_or_else(|| panic!("{id} : {vagues:?}"));
    // L'issue de w, une vague que le rejeu ne connaît pas, est ignorée : aucune vague ne s'ajoute.
    assert_eq!(vagues.iter().map(|vague| vague.id.as_str()).collect::<String>(), "vx");
    // Une issue redite remplace la précédente ; une vague sans note n'en a pas.
    assert_eq!(de("v").issue, Some(12));
    assert_eq!(de("x").issue, None);
    // Rien d'autre ne bouge : la vague ouverte garde son ouverture, sans main.
    let attendue = Vague {
        id: "v".into(),
        n: 1,
        depot: "/srv/depot".into(),
        objectif: "o".into(),
        issue: Some(12),
        ..Vague::default()
    };
    assert_eq!(de("v"), &attendue);
}

#[test]
fn le_lancement_et_la_fin_d_une_vague_s_ecrivent_et_se_relisent_a_l_identique() {
    let dir = dossier("lancement-aller-retour");
    let fil = Fil::seance(&dir, "humain");
    let empreinte = "0123456789abcdef".repeat(4);
    let lancements = [
        LancementVague { vague: "v".into(), version: "0.4.0".into(), plan: Some(empreinte.clone()) },
        LancementVague { vague: "w".into(), version: "1.0.0-rc.1+b~2".into(), plan: None },
    ];
    let fins = [FinVague { vague: "w".into(), code: 0 }, FinVague { vague: "w".into(), code: 255 }];
    fil.noter_lancement(&lancements[0]).unwrap();
    fil.noter_lancement(&lancements[1]).unwrap();
    // Elles partagent le compteur de la séance : la note qui suit prend le seq d'après.
    fil.noter(&Evenement::Correction { vague: "v".into(), corrige: "u".into() }).unwrap();
    fil.noter_fin(&fins[0]).unwrap();
    fil.noter_fin(&fins[1]).unwrap();
    // Le corps, plat, dans l'ordre du contrat ; `plan` vaut `null` quand la vague n'a pas de fichier de plan.
    let brut = fs::read(dir.join(FICHIER)).unwrap();
    let ecrit = String::from_utf8(brut.clone()).unwrap();
    let corps = format!(r#""type":"vague.lancement","vague":"v","version":"0.4.0","plan":"{empreinte}"}}"#);
    assert!(ecrit.contains(&corps), "{ecrit}");
    let corps = r#""type":"vague.lancement","vague":"w","version":"1.0.0-rc.1+b~2","plan":null}"#;
    assert!(ecrit.contains(corps), "{ecrit}");
    assert!(ecrit.contains(r#""type":"vague.fin","vague":"w","code":0}"#), "{ecrit}");
    assert!(ecrit.contains(r#""type":"vague.fin","vague":"w","code":255}"#), "{ecrit}");
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    assert_eq!(seqs(&notes), [3], "la correction suit les deux lancements, et les deux fins la suivent");
    // Un seul passage rend ce que `lire_octets` rend, et de plus les lancements et les fins, dans l'ordre du fil.
    let chemin = dir.join(FICHIER);
    let (lu, lues) = lire_octets_avec_lancements(&chemin, &brut).unwrap();
    assert_eq!(lu, lire_octets(&chemin, &brut).unwrap());
    let [premier, second] = lancements;
    let [zero, tout] = fins;
    let attendues =
        [LancementLu::Ouvert(premier), LancementLu::Ouvert(second), LancementLu::Sorti(zero), LancementLu::Sorti(tout)];
    assert_eq!(lues, attendues);
    // Chacune est aussi une `LigneLue`, sans `foi`, comme une issue ou un sceau.
    let (_, lignes) = lire_octets_avec_lignes(&chemin, &brut).unwrap();
    let genres: Vec<&str> = lignes.iter().map(|ligne| ligne.genre.as_str()).collect();
    assert_eq!(genres, ["vague.lancement", "vague.lancement", "vague.correction", "vague.fin", "vague.fin"]);
    assert!(lignes.iter().all(|ligne| ligne.foi.is_none()), "{lignes:?}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_lancement_ou_une_fin_aux_champs_faux_est_mal_forme() {
    let plan = "a".repeat(64);
    let lancement = format!(r#""type":"vague.lancement","vague":"v","version":"0.4.0","plan":"{plan}""#);
    let fin = r#""type":"vague.fin","vague":"v","code":1"#.to_string();
    let cas = vec![
        (format!("{lancement},\"titre\":\"x\""), "champ en trop pour vague.lancement : « titre »"),
        (lancement.replace(r#""version":"0.4.0","#, ""), "« version » manque"),
        (lancement.replace(r#","plan":"#, r#","plan_de":"#), "« plan » manque"),
        (lancement.replace("0.4.0", "0.4 0"), "« version » vaut \"0.4 0\", hors de son domaine"),
        (lancement.replace("0.4.0", ""), "« version » vaut \"\", hors de son domaine"),
        (lancement.replace("0.4.0", "é"), "« version » vaut \"é\", hors de son domaine"),
        (lancement.replace(r#""version":"0.4.0""#, r#""version":null"#), "« version » manque"),
        (lancement.replace(&plan, &"a".repeat(63)), "« plan » vaut"),
        (lancement.replace(&plan, &"a".repeat(65)), "« plan » vaut"),
        (lancement.replace(&plan, &"A".repeat(64)), "64 hexadécimaux minuscules"),
        (lancement.replace(&plan, &"g".repeat(64)), "64 hexadécimaux minuscules"),
        (lancement.replace(&format!(r#""{plan}""#), "7"), "« plan » manque ou n'est pas une chaîne"),
        (lancement.replace(r#""vague":"v","#, ""), "« vague » manque"),
        (format!("{fin},\"titre\":\"x\""), "champ en trop pour vague.fin : « titre »"),
        (fin.replace(r#""code":1"#, r#""code":256"#), "« code » vaut 256, hors de son domaine : un entier de 0 à 255"),
        (fin.replace(r#""code":1"#, r#""code":-1"#), "« code » n'est pas une valeur plate"),
        (fin.replace(r#""code":1"#, r#""code":"1""#), "« code » manque ou n'est pas un entier positif"),
        (fin.replace(r#""code":1"#, r#""code":null"#), "« code » manque ou n'est pas un entier positif"),
        (fin.replace(r#","code":1"#, ""), "« code » manque"),
        (fin.replace(r#""vague":"v","#, ""), "« vague » manque"),
    ];
    // Chaque ligne fausse est suivie d'une juste, de la même séance : aucun trou ne s'y voit.
    let mut lignes = Vec::new();
    for (i, (faux, _)) in cas.iter().enumerate() {
        lignes.push(ligne_s(2 * i as u64 + 1, faux));
        lignes.push(ligne_s(2 * i as u64 + 2, if faux.contains("vague.fin") { &fin } else { &lancement }));
    }
    let dir = dossier("lancement-faux");
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(notes.is_empty(), "un lancement n'est pas une note d'événement : {notes:?}");
    assert_eq!(codes(&ecarts), (0..cas.len()).map(|i| (2 * i + 1, 1009)).collect::<Vec<_>>(), "et aucun 1010");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout), "la preuve devait dire « {bout} » : {refus}");
    }
    // Les lignes justes se lisent, et elles seules : une mal formée se saute.
    let brut = fs::read(dir.join(FICHIER)).unwrap();
    let (_, lues) = lire_octets_avec_lancements(&dir.join(FICHIER), &brut).unwrap();
    assert_eq!(lues.len(), cas.len());
    let bonne_fin = LancementLu::Sorti(FinVague { vague: "v".into(), code: 1 });
    let bon_lancement =
        LancementLu::Ouvert(LancementVague { vague: "v".into(), version: "0.4.0".into(), plan: Some(plan) });
    let attendues: Vec<LancementLu> = cas
        .iter()
        .map(|(faux, _)| if faux.contains("vague.fin") { bonne_fin.clone() } else { bon_lancement.clone() })
        .collect();
    assert_eq!(lues, attendues);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_fil_d_avant_sans_lancement_ni_fin_se_lit_comme_avant() {
    let exemples: String = include_str!("exemples.jsonl")
        .lines()
        .filter(|ligne| ligne.starts_with(r#"{"v":2,"#))
        .map(|ligne| format!("{ligne}\n"))
        .collect();
    let chemin = Path::new("exemples.jsonl");
    let (lu, lancements) = lire_octets_avec_lancements(chemin, exemples.as_bytes()).unwrap();
    assert!(lancements.is_empty(), "{lancements:?}");
    assert!(!lu.0.notes.is_empty(), "les exemples se lisent");
    assert_eq!(lu, lire_octets(chemin, exemples.as_bytes()).unwrap());
}

#[test]
fn des_lots_tires_a_la_chaine_different() {
    assert_ne!(lot(), lot());
    // Des centaines de tirages en quelques ms, depuis plusieurs fils : aucun doublon.
    let lots: Vec<String> = thread::scope(|s| {
        let fils: Vec<_> = (0..4).map(|_| s.spawn(|| (0..250).map(|_| lot()).collect::<Vec<_>>())).collect();
        fils.into_iter().flat_map(|fil| fil.join().unwrap()).collect()
    });
    assert_eq!(lots.iter().collect::<HashSet<_>>().len(), lots.len());
    assert!(lots.iter().all(|un| !un.contains('-')), "un lot ne porte pas de tiret");
}

#[test]
fn une_consigne_fausse_est_mal_formee_et_le_systeme_peut_etre_nul() {
    let juste = format!(
        concat!(
            r#""type":"main.consigne","vague":"v","main":1,"tentative":1,"role":"main","consigne":"{e}","#,
            r#""systeme":"{e}","lecons":"a {e}","ecartees":"b","mesure":"""#,
        ),
        e = EMPREINTE
    );
    let hors_sha = "hors de son domaine : 64 hexadécimaux minuscules";
    let roles = "hors de son domaine : main ou decoupeur ou relecteur ou correction ou relance";
    let majuscules = EMPREINTE.to_uppercase();
    let cas = [
        (juste.replace(r#""role":"main""#, r#""role":"sceau""#), roles.to_string()),
        (juste.replace(r#""role":"main""#, r#""role":"Main""#), roles.to_string()),
        (juste.replace(r#""role":"main""#, r#""role":null"#), "« role » manque ou n'est pas une chaîne".to_string()),
        (juste.replace(r#""consigne":"e3b0"#, r#""consigne":"e3b"#), hors_sha.to_string()),
        (juste.replacen(EMPREINTE, &majuscules, 1), hors_sha.to_string()),
        (juste.replacen(EMPREINTE, &EMPREINTE[1..], 1), hors_sha.to_string()),
        (juste.replacen(&format!(r#""systeme":"{EMPREINTE}""#), r#""systeme":"abc""#, 1), hors_sha.to_string()),
        (juste.replacen(&format!(r#""systeme":"{EMPREINTE}""#), r#""systeme":"""#, 1), hors_sha.to_string()),
        (juste.replacen(&format!(r#""systeme":"{EMPREINTE}","#), "", 1), "« systeme » manque".to_string()),
        (juste.replace(r#""lecons":"a "#, r#""lecons":1,"x":"a "#), "« lecons » manque".to_string()),
        (juste.replace(r#""ecartees":"b""#, r#""ecartees":null"#), "« ecartees » manque".to_string()),
        (juste.replace(r#","mesure":"""#, ""), "« mesure » manque".to_string()),
        (juste.replace(r#""main":1"#, r#""main":0"#), "« main » vaut 0, hors de son domaine".to_string()),
        (format!(r#"{juste},"essai":1"#), "champ en trop pour main.consigne : « essai »".to_string()),
    ];
    let dir = dossier("consigne-fausse");
    let lignes: Vec<String> = cas.iter().enumerate().map(|(i, (corps, _))| ligne_s(i as u64 + 1, corps)).collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(notes.is_empty(), "aucune ligne fausse ne se lit : {notes:?}");
    assert_eq!(ecarts.len(), cas.len(), "{ecarts:?}");
    assert!(ecarts.iter().all(|(_, refus)| refus.code == Code::EvenementMalForme), "{ecarts:?}");
    for ((_, refus), (_, bout)) in ecarts.iter().zip(&cas) {
        assert!(refus.preuve.contains(bout.as_str()), "la preuve devait dire « {bout} » : {refus}");
    }
    // Les lignes justes se lisent : un système nul, chacun des cinq rôles, et un texte de leçons sur plusieurs lignes.
    let nul = juste.replacen(&format!(r#""systeme":"{EMPREINTE}""#), r#""systeme":null"#, 1);
    let roles_justes = ["main", "decoupeur", "relecteur", "correction", "relance"];
    let mut justes = vec![juste.clone(), nul];
    justes.extend(roles_justes.iter().map(|role| juste.replace(r#""role":"main""#, &format!(r#""role":"{role}""#))));
    let lignes: Vec<String> = justes.iter().enumerate().map(|(i, corps)| ligne_s(i as u64 + 1, corps)).collect();
    poser(&dir, &lignes.iter().map(String::as_str).collect::<Vec<_>>());
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (justes.len(), 0), "{ecarts:?}");
    let Evenement::Consigne { systeme, role, .. } = &notes[1].evenement else { panic!("{notes:?}") };
    assert_eq!((systeme, *role), (&None, Role::Main));
    let Evenement::Consigne { role, .. } = &notes[6].evenement else { panic!("{notes:?}") };
    assert_eq!(*role, Role::Relance);
    // Un fil sans cette note se lit encore, et le rejeu ne dit rien de plus d'une consigne.
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    let vagues = etat(&evenements);
    assert_eq!(vagues.len(), 1);
    assert!(vagues[0].mains.is_empty());
    let _ = fs::remove_dir_all(&dir);
}

/// Une ligne `vague.issue` bien formée, écrite à la main, de la séance `seance` au `seq` donné.
fn issue_ecrite(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"vague.issue","vague":"v","issue":4}}"#,
        ),
        seance = seance,
        seq = seq,
    )
}

/// Une ligne `vague.sceau` bien formée, écrite à la main, de la séance `seance` au `seq` donné.
fn sceau_ecrit(seance: &str, seq: u64) -> String {
    format!(
        concat!(
            r#"{{"v":2,"seance":"{seance}","seq":{seq},"instant":"2026-10-31T21:03:00.123Z","or"#,
            r#"igine":"humain","origine_foi":"declare","type":"vague.sceau","vague":"v","octets":"#,
            r#"5,"empreinte":"{EMPREINTE}"}}"#,
        ),
        seance = seance,
        seq = seq,
        EMPREINTE = EMPREINTE,
    )
}

#[test]
fn les_lignes_lues_disent_leur_place_dans_les_octets_du_fil() {
    let gap = debut("s", 9);
    let inconnue = debut("s", 3).replace("main.debut", "autre.chose");
    let lignes = [
        debut("s", 1),
        String::new(),
        "ceci n'est pas du JSON".to_string(),
        constat("s", 2),
        inconnue,
        gap.clone(),
        lancement("t", 1),
        issue_ecrite("t", 2),
        sceau_ecrit("t", 3),
        debut("t", 4).replace(r#""main":1"#, r#""main":"x""#),
    ];
    // Le début de chaque ligne dans les octets : les longueurs des lignes avant elle, un `\n` chacune.
    let mut reste = 0;
    let debuts: Vec<usize> = lignes
        .iter()
        .map(|ligne| {
            let ici = reste;
            reste += ligne.len() + 1;
            ici
        })
        .collect();
    let mut octets: Vec<u8> = lignes.iter().map(|ligne| format!("{ligne}\n")).collect::<String>().into_bytes();
    // Ce qui suit le dernier `\n` est en cours d'écriture, et n'est pas lu.
    octets.extend_from_slice(debut("t", 5).as_bytes());
    let instant = "2026-10-31T21:03:00.123Z".to_string();
    let attendue = |rang: usize, seance: &str, seq: u64, genre: &str, foi: Option<Foi>| LigneLue {
        seance: seance.into(),
        seq,
        genre: genre.into(),
        instant: instant.clone(),
        foi,
        debut: debuts[rang],
        fin: debuts[rang] + lignes[rang].len(),
    };
    let (lu, lues) = lire_octets_avec_lignes(Path::new("fil.jsonl"), &octets).unwrap();
    // Ligne 2 : vide, sautée sans écart ; ligne 3 : illisible ; ligne 5 : un type inconnu, que `s` compte (le seq 3
    // est celui qu'on attend) ; ligne 6 : un `seq` rompu (1010), lu ; ligne 10 : mal formée.
    assert_eq!(codes(&lu.0.ecarts), [(3, 1001), (6, 1010), (10, 1009)]);
    assert_eq!(
        lues,
        [
            attendue(0, "s", 1, "main.debut", None),
            attendue(3, "s", 2, "main.constat", Some(Foi::Constate)),
            attendue(5, "s", 9, "main.debut", None),
            attendue(6, "t", 1, "main.lancement", Some(Foi::Declare)),
            attendue(7, "t", 2, "vague.issue", None),
            attendue(8, "t", 3, "vague.sceau", None),
        ]
    );
    // Chaque `debut..fin` est la ligne elle-même, sans son `\n`.
    for lue in &lues {
        let ligne = &octets[lue.debut..lue.fin];
        assert!(!ligne.ends_with(b"\n") && octets[lue.fin] == b'\n', "{lue:?}");
        let texte = str::from_utf8(ligne).unwrap();
        assert!(texte.starts_with(r#"{"v":2,"#) && texte.ends_with('}'), "{texte}");
    }
    assert_eq!(&octets[lues[2].debut..lues[2].fin], gap.as_bytes());
}

#[test]
fn la_lecture_avec_lignes_rend_celle_de_lire_octets_sur_les_exemples() {
    let toutes: Vec<&str> =
        include_str!("exemples.jsonl").lines().filter(|ligne| ligne.starts_with(r#"{"v":2,"#)).collect();
    let mut texte: Vec<String> = toutes.iter().map(|ligne| ligne.to_string()).collect();
    // Une issue de plus : les exemples n'en portent pas.
    texte.push(issue_ecrite("1761902300000_16_1", 1));
    let octets = texte.iter().map(|ligne| format!("{ligne}\n")).collect::<String>().into_bytes();
    let chemin = Path::new("exemples.jsonl");
    let seule = lire_octets(chemin, &octets).unwrap();
    let (avec, lues) = lire_octets_avec_lignes(chemin, &octets).unwrap();
    // Les mêmes notes, issues, sceaux et écarts.
    assert_eq!(seule, avec);
    assert!(!avec.0.notes.is_empty() && !avec.1.is_empty() && !avec.2.is_empty(), "des notes, une issue, des sceaux");
    assert!(avec.0.ecarts.iter().any(|(_, refus)| refus.code == Code::SequenceRompue), "les trous voulus");
    // Une ligne lue par note, issue et sceau, dans l'ordre du fil, et chacune est la ligne du texte.
    assert_eq!(lues.len(), avec.0.notes.len() + avec.1.len() + avec.2.len());
    assert_eq!(lues.len(), texte.len(), "chaque ligne d'exemple se lit");
    for (lue, ligne) in lues.iter().zip(&texte) {
        assert_eq!(&octets[lue.debut..lue.fin], ligne.as_bytes());
        assert!(ligne.contains(&format!(r#""seance":"{}","seq":{},"#, lue.seance, lue.seq)), "{lue:?} : {ligne}");
        assert!(ligne.contains(&format!(r#""type":"{}""#, lue.genre)), "{lue:?} : {ligne}");
    }
}
