use super::*;

/// Le texte d'une leçon : ces lignes, chacune finie par `\n`.
fn texte(lignes: &[&str]) -> String {
    lignes.iter().map(|ligne| format!("{ligne}\n")).collect()
}

const ENTETE: [&str; 4] = ["---", "name: ma-lecon", "description: Une règle.", "---"];

/// Une leçon juste, dont `## Files` porte ces lignes.
fn lecon_avec(fichiers: &[&str]) -> Vec<String> {
    let mut lignes: Vec<String> = ENTETE.iter().map(|ligne| ligne.to_string()).collect();
    for (rang, titre) in SECTIONS.iter().enumerate() {
        lignes.push(String::new());
        lignes.push(format!("## {titre}"));
        if rang == FICHIERS {
            lignes.extend(fichiers.iter().map(|ligne| ligne.to_string()));
        } else {
            lignes.push(format!("Du texte pour {titre}."));
        }
    }
    lignes
}

fn en_texte(lignes: &[String]) -> String {
    texte(&lignes.iter().map(String::as_str).collect::<Vec<_>>())
}

/// Le refus de la leçon, avec le numéro de sa ligne.
fn refusee(texte: &str) -> Refus {
    let refus = lire("ma-lecon", texte.as_bytes()).unwrap_err();
    assert_eq!(refus.code, Code::LeconMalFormee, "{refus}");
    refus
}

fn refusee_ligne(texte: &str, ligne: usize) -> Refus {
    let refus = refusee(texte);
    assert!(refus.preuve.starts_with(&format!("ligne {ligne} : ")), "ligne {ligne} attendue : {refus}");
    refus
}

#[test]
fn une_lecon_juste_est_lue() {
    let lecon = lire("ma-lecon", en_texte(&lecon_avec(&["- src/a.rs", "- docs/"])).as_bytes()).unwrap();
    assert_eq!(lecon.nom, "ma-lecon");
    assert_eq!(lecon.regle, "Une règle.");
    assert_eq!(lecon.fichiers, ["src/a.rs", "docs/"]);
    assert!(lecon.texte.starts_with("## What happened"), "{}", lecon.texte);
    assert!(lecon.texte.ends_with("Du texte pour Falls when."), "{}", lecon.texte);
    // `\r\n` est admis, et une leçon sans retour final aussi.
    let crlf = en_texte(&lecon_avec(&["- src/a.rs"])).replace('\n', "\r\n");
    assert_eq!(lire("ma-lecon", crlf.as_bytes()).unwrap().fichiers, ["src/a.rs"]);
    let sans_fin = en_texte(&lecon_avec(&[]));
    assert!(lire("ma-lecon", sans_fin.trim_end().as_bytes()).is_ok());
}

#[test]
fn une_lecon_sans_chemin_est_generale() {
    let lecon = lire("ma-lecon", en_texte(&lecon_avec(&[])).as_bytes()).unwrap();
    assert!(lecon.fichiers.is_empty());
}

#[test]
fn une_lecon_sans_en_tete_est_refusee() {
    refusee_ligne("## What happened\ntexte\n", 1);
    refusee_ligne("", 1);
    refusee_ligne("---\ndescription: x\nname: ma-lecon\n---\n", 2);
}

#[test]
fn une_cle_de_plus_est_refusee() {
    let mut lignes = lecon_avec(&[]);
    lignes.insert(3, "license: MIT".into());
    refusee_ligne(&en_texte(&lignes), 4);
    // Une clé de moins : la ligne 3 n'est plus la description.
    refusee_ligne("---\nname: ma-lecon\n---\n", 3);
}

#[test]
fn un_nom_qui_n_est_pas_celui_du_dossier_est_refuse() {
    let refus = lire("autre", en_texte(&lecon_avec(&[])).as_bytes()).unwrap_err();
    assert!(refus.preuve.starts_with("ligne 2 : "), "{refus}");
    for mauvais in ["Ma-Lecon", "ma_lecon", "", &"a".repeat(65)] {
        let mut lignes = lecon_avec(&[]);
        lignes[1] = format!("name: {mauvais}");
        let refus = lire(mauvais, en_texte(&lignes).as_bytes()).unwrap_err();
        assert!(refus.preuve.starts_with("ligne 2 : "), "{mauvais:?} : {refus}");
    }
    let long = "a".repeat(64);
    let mut lignes = lecon_avec(&[]);
    lignes[1] = format!("name: {long}");
    assert!(lire(&long, en_texte(&lignes).as_bytes()).is_ok());
}

#[test]
fn une_regle_trop_longue_ou_sur_deux_lignes_est_refusee() {
    let mut lignes = lecon_avec(&[]);
    lignes[2] = format!("description: {}", "é".repeat(200));
    assert!(lire("ma-lecon", en_texte(&lignes).as_bytes()).is_ok());
    lignes[2] = format!("description: {}", "é".repeat(201));
    refusee_ligne(&en_texte(&lignes), 3);
    lignes[2] = "description: ".into();
    refusee_ligne(&en_texte(&lignes), 3);
    // Sur deux lignes : la seconde tient la place de `---`.
    let mut lignes = lecon_avec(&[]);
    lignes.insert(3, "suite de la règle".into());
    refusee_ligne(&en_texte(&lignes), 4);
}

#[test]
fn une_section_qui_manque_est_refusee() {
    // Sans `## Why` : refusée, à la ligne de la section qui la suit.
    let mut lignes = lecon_avec(&[]);
    let place = lignes.iter().position(|ligne| ligne == "## Why").unwrap();
    lignes.drain(place..place + 2);
    let refus = refusee_ligne(&en_texte(&lignes), place + 2);
    assert!(refus.preuve.contains("il manque `## Why`"), "{refus}");
    // Sans la dernière : à la fin du fichier.
    let mut lignes = lecon_avec(&[]);
    lignes.truncate(lignes.len() - 2);
    let refus = refusee_ligne(&en_texte(&lignes), lignes.len());
    assert!(refus.preuve.contains("il manque `## Falls when`"), "{refus}");
    // Sans aucune.
    let refus = refusee_ligne(&texte(&ENTETE), 4);
    assert!(refus.preuve.contains("il manque `## What happened`"), "{refus}");
}

#[test]
fn une_section_vide_est_refusee() {
    // `## Why` vide, suivie d'une autre section ou de la fin.
    let mut lignes = lecon_avec(&[]);
    let place = lignes.iter().position(|ligne| ligne == "## Why").unwrap();
    lignes.remove(place + 1);
    let refus = refusee_ligne(&en_texte(&lignes), place + 1);
    assert!(refus.preuve.contains("`## Why` est vide"), "{refus}");
    let mut lignes = lecon_avec(&[]);
    let dernier = lignes.len() - 1;
    lignes[dernier] = "   ".into();
    refusee_ligne(&en_texte(&lignes), dernier);
}

#[test]
fn une_section_en_double_ou_hors_d_ordre_est_refusee() {
    let mut lignes = lecon_avec(&[]);
    lignes.extend(["## Why".to_string(), "Encore.".to_string()]);
    let fin = lignes.len() - 1;
    let refus = refusee_ligne(&en_texte(&lignes), fin);
    assert!(refus.preuve.contains("en double ou hors d'ordre"), "{refus}");
    // `## Pitfall` avant `## How it is checked`.
    let mut lignes = lecon_avec(&[]);
    let (a, b) = (
        lignes.iter().position(|ligne| ligne == "## How it is checked").unwrap(),
        lignes.iter().position(|ligne| ligne == "## Pitfall").unwrap(),
    );
    lignes.swap(a, b);
    let refus = refusee_ligne(&en_texte(&lignes), a + 1);
    assert!(refus.preuve.contains("il manque `## How it is checked` avant `## Pitfall`"), "{refus}");
    // Une section inconnue.
    let mut lignes = lecon_avec(&[]);
    lignes.extend(["## Autre".to_string(), "Texte.".to_string()]);
    let fin = lignes.len() - 1;
    refusee_ligne(&en_texte(&lignes), fin);
    // Du texte avant la première section.
    let mut lignes = lecon_avec(&[]);
    lignes.insert(4, "Intrus.".into());
    refusee_ligne(&en_texte(&lignes), 5);
}

#[test]
fn un_chemin_hors_forme_est_refuse() {
    let cas = [
        ("- /abs", "est absolu"),
        ("- src\\a.rs", "barre inverse"),
        ("- ../a", "`..`"),
        ("- .git/config", "`.git/`"),
        ("- .claude/x", "`.claude/`"),
        ("- ", "nom vide"),
        ("src/a.rs", "`- <chemin>`"),
        ("-  src/a.rs", "`- <chemin>`"),
    ];
    for (ligne, raison) in cas {
        let lignes = lecon_avec(&["- bon/chemin", ligne]);
        let place = lignes.iter().position(|l| l == ligne).unwrap();
        let refus = refusee_ligne(&en_texte(&lignes), place + 1);
        assert!(refus.preuve.contains(raison), "{ligne:?} : {refus}");
    }
}

#[test]
fn une_lecon_de_plus_de_4096_octets_est_refusee() {
    let mut lignes = lecon_avec(&[]);
    let base = en_texte(&lignes).len();
    lignes[6] = format!("Du texte pour What happened.{}", "x".repeat(TAILLE_MAX - base));
    let pile = en_texte(&lignes);
    assert_eq!(pile.len(), TAILLE_MAX);
    assert!(lire("ma-lecon", pile.as_bytes()).is_ok());
    lignes[6].push('x');
    let refus = refusee_ligne(&en_texte(&lignes), 1);
    assert!(refus.preuve.contains("4097 octets"), "{refus}");
}

#[test]
fn une_lecon_qui_n_est_pas_de_l_utf_8_est_refusee() {
    let mut octets = en_texte(&lecon_avec(&[])).into_bytes();
    let place = octets.windows(5).position(|w| w == b"Pitfa").unwrap();
    octets[place] = 0xff;
    let refus = lire("ma-lecon", &octets).unwrap_err();
    assert_eq!(refus.code, Code::LeconMalFormee);
    assert!(refus.preuve.starts_with("ligne 15 : "), "{refus}");
}
