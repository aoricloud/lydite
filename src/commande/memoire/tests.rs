//! Les tests de `lydite memory` et de `lydite memory recall` : ce que chaque commande écrit sur sa sortie et sur sa
//! sortie d'erreur, lu dans des tampons, sur des fils écrits par le test.
use super::*;
use crate::branchements::dossier::{Fil, lire_sceaux};
use crate::coeur::empreinte::{hex, sha256};
use crate::coeur::fil::Evenement;
use crate::essais::{V1, ouverture, racine};
use std::fs;

/// Un fil de quatre lignes : deux notes (« premier »), le sceau de `V1` qui les couvre, puis une note d'après le sceau
/// (« dernier »). Rend le dossier.
fn fil_scelle(nom: &str) -> std::path::PathBuf {
    let dossier = racine(nom);
    let fil = Fil::seance(&dossier, "humain");
    fil.noter(&ouverture(V1, 1, "premier")).unwrap();
    fil.noter(&Evenement::Debut { vague: V1.into(), main: 1, tentative: 1 }).unwrap();
    fil.sceller(V1).unwrap();
    fil.noter(&ouverture(V1, 1, "dernier")).unwrap();
    dossier
}

/// Le texte du fil du dossier.
fn fil_de(dossier: &Path) -> String {
    fs::read_to_string(dossier.join("fil.jsonl")).unwrap()
}

/// Joue l'index sur des tampons : son résultat, ce qu'il a écrit sur la sortie, puis sur la sortie d'erreur.
fn jouer_l_index(dossier: &Path) -> (Result<ExitCode, Refus>, String, String) {
    let (mut sortie, mut erreur) = (Vec::new(), Vec::new());
    let rendu = memoire_ecrivant(dossier, &mut sortie, &mut erreur);
    (rendu, String::from_utf8(sortie).unwrap(), String::from_utf8(erreur).unwrap())
}

/// Joue le rappel de `texte` sur des tampons, comme [`jouer_l_index`].
fn jouer_le_rappel(dossier: &Path, texte: &str) -> (Result<ExitCode, Refus>, String, String) {
    let (mut sortie, mut erreur) = (Vec::new(), Vec::new());
    let rendu = rappel_ecrivant(dossier, texte, &mut sortie, &mut erreur);
    (rendu, String::from_utf8(sortie).unwrap(), String::from_utf8(erreur).unwrap())
}

/// L'adresse `<séance>/<seq>` de la première ligne du fil, lue de l'index.
fn premiere_adresse(dossier: &Path) -> String {
    let (_, index, _) = jouer_l_index(dossier);
    index.split(' ').next().unwrap().to_string()
}

/// Change, dans le fil du dossier, `avant` en `apres` (de même longueur) : une ligne changée sur place.
fn changer_dans_le_fil(dossier: &Path, avant: &str, apres: &str) {
    assert_eq!(avant.len(), apres.len());
    let texte = fil_de(dossier);
    assert_eq!(texte.matches(avant).count(), 1, "{texte}");
    fs::write(dossier.join("fil.jsonl"), texte.replace(avant, apres)).unwrap();
}

/// Les noms que contient un dossier.
fn noms(dossier: &Path) -> Vec<String> {
    let entrees = fs::read_dir(dossier).unwrap();
    let mut noms: Vec<String> =
        entrees.map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect();
    noms.sort();
    noms
}

#[test]
fn l_index_dit_chaque_ligne_du_fil_scelle_intact_et_sort_en_0() {
    let dossier = fil_scelle("memoire-index");
    let (rendu, sortie, erreur) = jouer_l_index(&dossier);
    assert_eq!(rendu, Ok(ExitCode::SUCCESS));
    assert_eq!(erreur, "");
    let texte = fil_de(&dossier);
    let lignes: Vec<&str> = sortie.lines().collect();
    assert_eq!(lignes.len(), 4, "{sortie}");
    assert!(sortie.ends_with('\n'));
    // L'empreinte et la taille se refont ici, sur les lignes du fichier, sans passer par le module de la mémoire.
    for (dite, ecrite) in lignes.iter().zip(texte.lines()) {
        let champs: Vec<&str> = dite.split(' ').collect();
        assert_eq!(champs.len(), 6, "{dite}");
        assert_eq!(champs[1], hex(&sha256(ecrite.as_bytes())), "{dite}");
        assert_eq!(champs[5], ecrite.len().to_string(), "{dite}");
    }
    let genres: Vec<&str> = lignes.iter().map(|ligne| ligne.split(' ').nth(2).unwrap()).collect();
    assert_eq!(genres, ["vague.ouverte", "main.debut", "vague.sceau", "vague.ouverte"]);
    let seqs = lignes.iter().map(|ligne| ligne.split(' ').next().unwrap().split('/').nth(1).unwrap());
    let adresses: Vec<&str> = seqs.collect();
    assert_eq!(adresses, ["1", "2", "3", "4"]);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_fil_absent_rend_un_index_vide_en_0_sans_rien_creer() {
    let dossier = racine("memoire-absent");
    let absent = dossier.join("pas-encore");
    assert_eq!(jouer_l_index(&absent), (Ok(ExitCode::SUCCESS), String::new(), String::new()));
    assert!(!absent.exists(), "{} a été créé", absent.display());
    // Un dossier sans fil : de même, le fil ne se crée pas.
    assert_eq!(jouer_l_index(&dossier), (Ok(ExitCode::SUCCESS), String::new(), String::new()));
    assert_eq!(noms(&dossier), Vec::<String>::new());
    // Le rappel d'une empreinte bien formée dans un fil absent : une ligne absente, 6034.
    let refus = jouer_le_rappel(&dossier, &"0".repeat(64)).0.unwrap_err();
    assert_eq!(refus.code, Code::MemoireTexteAbsent, "{refus}");
    assert_eq!(noms(&dossier), Vec::<String>::new());
    let _ = fs::remove_dir_all(&dossier);
}

/// Un écrivain qui compte ses appels à `write`, et garde ce qu'il reçoit.
#[derive(Default)]
struct Compteur {
    appels: usize,
    recu: Vec<u8>,
}

impl Write for Compteur {
    fn write(&mut self, octets: &[u8]) -> io::Result<usize> {
        self.appels += 1;
        self.recu.extend_from_slice(octets);
        Ok(octets.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn l_index_et_le_rappel_s_ecrivent_d_un_seul_bloc() {
    let dossier = fil_scelle("memoire-bloc");
    let mut sortie = Compteur::default();
    assert_eq!(memoire_ecrivant(&dossier, &mut sortie, &mut Vec::new()), Ok(ExitCode::SUCCESS));
    assert_eq!(sortie.recu.iter().filter(|&&octet| octet == b'\n').count(), 4);
    assert_eq!(sortie.appels, 1, "une écriture par ligne ferait quatre appels");
    let mut sortie = Compteur::default();
    let adresse = premiere_adresse(&dossier);
    assert_eq!(rappel_ecrivant(&dossier, &adresse, &mut sortie, &mut Vec::new()), Ok(ExitCode::SUCCESS));
    assert_eq!(sortie.appels, 1, "la ligne et son retour partent ensemble");
    let _ = fs::remove_dir_all(&dossier);
}

/// Un écrivain dont toute écriture échoue de cette façon.
struct Casse(io::ErrorKind);

impl Write for Casse {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(self.0.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(self.0.into())
    }
}

#[test]
fn un_tube_ferme_n_est_ni_un_refus_ni_une_panique_mais_une_autre_ecriture_ratee_l_est() {
    let dossier = fil_scelle("memoire-tube");
    let adresse = premiere_adresse(&dossier);
    let mut erreur = Vec::new();
    // Le lecteur a fermé le tube : la commande finit en silence, en 0.
    let ferme = || Casse(io::ErrorKind::BrokenPipe);
    assert_eq!(memoire_ecrivant(&dossier, &mut ferme(), &mut erreur), Ok(ExitCode::SUCCESS));
    assert_eq!(rappel_ecrivant(&dossier, &adresse, &mut ferme(), &mut erreur), Ok(ExitCode::SUCCESS));
    assert!(erreur.is_empty(), "{}", String::from_utf8_lossy(&erreur));
    // Toute autre écriture ratée est le refus 6006.
    let plein = || Casse(io::ErrorKind::StorageFull);
    let refus = memoire_ecrivant(&dossier, &mut plein(), &mut erreur).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
    let refus = rappel_ecrivant(&dossier, &adresse, &mut plein(), &mut erreur).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn le_rappel_rend_la_ligne_exacte_suivie_d_un_retour_par_son_adresse_et_par_son_empreinte() {
    let dossier = fil_scelle("memoire-rappel");
    let texte = fil_de(&dossier);
    let seance = premiere_adresse(&dossier).split('/').next().unwrap().to_string();
    for (i, ecrite) in texte.lines().enumerate() {
        let attendu = (Ok(ExitCode::SUCCESS), format!("{ecrite}\n"), String::new());
        assert_eq!(jouer_le_rappel(&dossier, &format!("{seance}/{}", i + 1)), attendu);
        assert_eq!(jouer_le_rappel(&dossier, &hex(&sha256(ecrite.as_bytes()))), attendu);
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_rappel_d_une_ligne_absente_est_le_refus_6034_qui_la_cite() {
    let dossier = fil_scelle("memoire-absente");
    let seance = premiere_adresse(&dossier).split('/').next().unwrap().to_string();
    let inconnue = "0".repeat(64);
    for texte in [format!("{seance}/9"), "autre/1".into(), "x/abc".into(), inconnue] {
        let (rendu, sortie, erreur) = jouer_le_rappel(&dossier, &texte);
        let refus = rendu.unwrap_err();
        assert_eq!(refus.code, Code::MemoireTexteAbsent, "{texte}: {refus}");
        assert!(refus.preuve.contains(&texte), "{refus}");
        assert_eq!((sortie.as_str(), erreur.as_str()), ("", ""), "{texte}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_texte_mal_forme_est_le_refus_6035_avant_toute_lecture_du_fil() {
    // Un fil d'une autre version, que seule une lecture refuserait (1002) : le texte mal formé le devance.
    let dossier = racine("memoire-empreinte");
    fs::write(dossier.join("fil.jsonl"), "{\"v\":3}\n").unwrap();
    for texte in ["a".repeat(63), "A".repeat(64), "mot".into(), String::new(), "g".repeat(64)] {
        let refus = jouer_le_rappel(&dossier, &texte).0.unwrap_err();
        assert_eq!(refus.code, Code::MemoireEmpreinteFausse, "{texte:?}: {refus}");
    }
    // Un texte bien formé, lui, lit le fil, et le fil d'une autre version est son refus.
    for texte in ["s/1".to_string(), "a".repeat(64)] {
        let refus = jouer_le_rappel(&dossier, &texte).0.unwrap_err();
        assert_eq!(refus.code, Code::VersionDifferente, "{texte}: {refus}");
    }
    let refus = jouer_l_index(&dossier).0.unwrap_err();
    assert_eq!(refus.code, Code::VersionDifferente, "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_sceau_rompu_arrete_l_index_et_le_rappel_sans_rien_ecrire_sur_la_sortie() {
    let dossier = fil_scelle("memoire-rompu");
    let adresse = premiere_adresse(&dossier);
    let sceau = lire_sceaux(&dossier)[0].clone();
    // Le fil intact : les deux commandes rendent leur ligne, le même fil que celui qu'on va rompre.
    assert!(!jouer_l_index(&dossier).1.is_empty());
    assert!(!jouer_le_rappel(&dossier, &adresse).1.is_empty());
    // Un octet change avant le sceau : le sceau est nommé, comme `lydite status` le nomme, et rien ne sort.
    changer_dans_le_fil(&dossier, "premier", "premiex");
    let dit = format!(
        "the thread no longer matches the seal of wave {V1}, noted at {} bytes: a line before it changed or \
         disappeared\n",
        sceau.octets
    );
    let attendu = (Ok(ExitCode::from(2)), String::new(), dit);
    assert_eq!(jouer_l_index(&dossier), attendu);
    assert_eq!(jouer_le_rappel(&dossier, &adresse), attendu);
    // Même une ligne que le changement n'a pas touchée n'est pas rendue : aucune ligne d'un fil rompu.
    let derniere = adresse.replace("/1", "/4");
    assert_eq!(jouer_le_rappel(&dossier, &derniere), attendu);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_ligne_ajoutee_apres_le_dernier_sceau_ne_rompt_rien() {
    let dossier = fil_scelle("memoire-ajoutee");
    let mut texte = fil_de(&dossier);
    texte.push_str("{\"ajoutee\":\"apres\"}\n");
    fs::write(dossier.join("fil.jsonl"), texte).unwrap();
    let (rendu, sortie, _) = jouer_l_index(&dossier);
    assert_eq!(rendu, Ok(ExitCode::SUCCESS));
    assert_eq!(sortie.lines().count(), 4, "la ligne ajoutée n'est pas lue : son enveloppe manque");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn l_index_dit_les_ecarts_du_lecteur_et_le_rappel_ne_les_redit_pas() {
    let dossier = racine("memoire-ecarts");
    let fil = Fil::seance(&dossier, "humain");
    fil.noter(&ouverture(V1, 1, "premier")).unwrap();
    // Une ligne cassée entre deux notes : le lecteur la saute, et son numéro est 2.
    let chemin = dossier.join("fil.jsonl");
    let mut texte = fs::read_to_string(&chemin).unwrap();
    texte.push_str("pas du json\n");
    fs::write(&chemin, texte).unwrap();
    fil.noter(&ouverture(V1, 1, "dernier")).unwrap();
    let (rendu, sortie, erreur) = jouer_l_index(&dossier);
    assert_eq!(rendu, Ok(ExitCode::SUCCESS));
    assert_eq!(sortie.lines().count(), 2, "{sortie}");
    assert_eq!(erreur.lines().count(), 4, "un refus tient en quatre lignes : {erreur}");
    assert!(erreur.starts_with("lydite : 1001 "), "{erreur}");
    assert!(erreur.contains("ligne 2 :"), "{erreur}");
    // Le rappel ne redit pas l'écart.
    let adresse = premiere_adresse(&dossier);
    let (rendu, _, erreur) = jouer_le_rappel(&dossier, &adresse);
    assert_eq!(rendu, Ok(ExitCode::SUCCESS));
    assert_eq!(erreur, "");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn les_deux_commandes_n_ecrivent_ni_le_fil_ni_rien_dans_le_dossier() {
    let dossier = fil_scelle("memoire-lecture-seule");
    let (avant, noms_avant) = (fil_de(&dossier), noms(&dossier));
    let adresse = premiere_adresse(&dossier);
    jouer_l_index(&dossier).0.unwrap();
    jouer_le_rappel(&dossier, &adresse).0.unwrap();
    jouer_le_rappel(&dossier, "x/9").0.unwrap_err();
    assert_eq!(fil_de(&dossier), avant);
    assert_eq!(noms(&dossier), noms_avant);
    assert_eq!(noms_avant, ["fil.jsonl"], "aucun verrou ne se prend");
    let _ = fs::remove_dir_all(&dossier);
}
