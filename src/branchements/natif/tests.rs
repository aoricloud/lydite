//! Les tests du rejeu sous le mur natif. Sans vrai bwrap : un faux (`essais::faux_bwrap`) note son appel et joue ce qui
//! suit son premier `--`, sans mur. Ce que le vrai mur tient se constate par la grille (`LYDITE_GRILLE=natif`).
// Le faux bwrap est un `sh` : ses tests, et ce qui ne sert qu'à eux, ne sont que sous Unix, où le mur existe ; le reste
// (les refus d'avant le lancement) se joue partout.
#![cfg_attr(not(unix), allow(dead_code))]
use super::*;
use crate::essais::mains::{dossier, profil_borne};
use std::collections::BTreeSet;

/// Un décor : un dossier neuf (chemin réel), son faux dossier personnel, le dossier de Kraken du profil, le git commun
/// du dépôt visé, le bac et le dossier temporaire du bac (qui n'existe qu'une fois le rejeu préparé).
struct Decor {
    dir: PathBuf,
    maison: PathBuf,
    kraken: PathBuf,
    commun: PathBuf,
    bac: PathBuf,
    temporaire: PathBuf,
}

fn decor(nom: &str) -> Decor {
    // Sous Unix, le chemin réel (`mur_attendu` compare des chemins réels) ; sous Windows, tel que le runner l'écrit :
    // `canonicalize` y rend un chemin verbatim (`\\?\C:\...`), que le profil borné refuse.
    #[cfg(unix)]
    let dir = fs::canonicalize(dossier(nom)).unwrap();
    #[cfg(not(unix))]
    let dir = dossier(nom);
    let (maison, kraken, commun, bac) =
        (dir.join("personnel"), dir.join("lydite"), dir.join("depot").join(".git"), dir.join("bac"));
    for dossier in [&maison, &kraken, &commun, &bac] {
        fs::create_dir_all(dossier).unwrap();
    }
    let temporaire = temporaire_de(&bac).unwrap();
    Decor { dir, maison, kraken, commun, bac, temporaire }
}

impl Decor {
    /// Le profil au mur exigé, borné par ce décor, avec ces lignes de plus.
    fn profil(&self, lignes: &str) -> Profil {
        profil_borne(&format!("sorte = claude\nmur = exige\n{lignes}"), &self.dir)
    }

    /// Le faux bwrap de ce décor.
    #[cfg(unix)]
    fn faux(&self) -> OsString {
        crate::essais::faux_bwrap(&self.dir)
    }

    /// Ce qui est directement dans `dossier`, par nom.
    fn contenu(dossier: &Path) -> BTreeSet<String> {
        let entrees = fs::read_dir(dossier).unwrap();
        entrees.map(|entree| entree.unwrap().file_name().to_string_lossy().into_owned()).collect()
    }

    /// Le rejeu de `commande` sous le programme `programme`, le journal du décor.
    fn rejouer(&self, programme: &OsStr, profil: &Profil, commande: &str) -> (Option<String>, Result<(), String>) {
        rejouer(programme, &self.maison, profil, commande, &self.bac, &self.dir.join("journal.tmp"))
    }
}

/// Les arguments d'un mur monté par ce décor, sans domaine : les fermés (le dossier personnel, le dossier de Kraken),
/// les lus (le bac, le git commun, puis `lus`), les écrits (le bac, son temporaire), le git commun refusé en écriture.
fn mur_attendu(d: &Decor, lus: &[&Path]) -> Vec<String> {
    let texte = |chemin: &Path| chemin.to_string_lossy().into_owned();
    let mut args: Vec<String> = [
        "--new-session",
        "--die-with-parent",
        "--unshare-user",
        "--unshare-pid",
        "--unshare-net",
        "--unshare-ipc",
        "--unshare-uts",
        "--cap-drop",
        "ALL",
        "--ro-bind",
        "/",
        "/",
        "--tmpfs",
    ]
    .map(String::from)
    .into();
    args.extend([texte(&d.maison), "--tmpfs".into(), texte(&d.kraken)]);
    let lus = [d.bac.as_path(), d.commun.as_path()].into_iter().chain(lus.iter().copied());
    for lu in lus {
        args.extend(["--ro-bind".into(), texte(lu), texte(lu)]);
    }
    for ecrit in [&d.bac, &d.temporaire] {
        args.extend(["--bind".into(), texte(ecrit), texte(ecrit)]);
    }
    args.extend(["--ro-bind".into(), texte(&d.commun), texte(&d.commun)]);
    args.extend(["--dev", "/dev", "--proc", "/proc"].map(String::from));
    args
}

/// Les lignes que le faux bwrap a notées, dans son fichier `nom`.
#[cfg(unix)]
fn notes_du_faux(d: &Decor, nom: &str) -> Vec<String> {
    let texte = fs::read_to_string(d.dir.join("bwrap").join(nom)).unwrap_or_default();
    texte.lines().map(String::from).collect()
}

#[cfg(unix)]
#[test]
fn la_commande_part_sous_deux_bwrap_dont_le_second_pose_le_verrou() {
    let d = decor("natif-forme");
    let faux = d.faux();
    let profil = d.profil("");
    let commande = "echo MARQUE-NATIF; pwd -P";
    let (verif, rejeu) = d.rejouer(&faux, &profil, commande);
    assert_eq!(rejeu, Ok(()));
    // La sortie de la commande, rien d'autre : le décor est complet, le mur ne saute aucun chemin.
    assert_eq!(verif, Some(format!("MARQUE-NATIF\n{}\n", d.bac.display())));
    let appels = crate::essais::appels_du_faux_bwrap(&d.dir);
    assert_eq!(appels.len(), 2, "un bwrap extérieur, un second dedans : {appels:?}");
    // Le bwrap extérieur : les arguments du mur, puis la ligne qui ouvre le filtre et lance le second.
    let (exterieur, interieur) = (&appels[0], &appels[1]);
    let mur = mur_attendu(&d, &[]);
    assert_eq!(exterieur[..mur.len()], mur[..]);
    let filtre = Path::new(&exterieur[mur.len() + 5]);
    let suite = ["--", "sh", "-c", POSE_LE_VERROU, "sh"].map(String::from);
    assert_eq!(exterieur[mur.len()..mur.len() + 5], suite);
    let fin = [filtre.to_string_lossy().into_owned(), faux.to_string_lossy().into_owned(), commande.into()];
    assert_eq!(exterieur[mur.len() + 5..], fin);
    assert_eq!(filtre.parent(), Some(d.temporaire.as_path()), "le filtre est dans le dossier temporaire du bac");
    // Le second : le verrou, par le descripteur 9, puis la commande.
    let verrou = "--bind / / --dev-bind /dev /dev --unshare-pid --proc /proc --die-with-parent --seccomp 9 -- sh -c";
    let attendu: Vec<String> = verrou.split(' ').map(String::from).chain([commande.to_string()]).collect();
    assert_eq!(*interieur, attendu);
    // Le fichier du filtre existait au moment de l'appel, le second bwrap en a lu les octets par le descripteur 9, et
    // il n'en reste rien après : le dossier temporaire est vide (le même regard l'a vu plein, par le faux).
    assert_eq!(notes_du_faux(&d, "filtres.txt"), [format!("present {}", filtre.display())]);
    assert_eq!(fs::read(d.dir.join("bwrap").join("filtre.bin")).unwrap(), filtre_attendu());
    assert!(!filtre.exists());
    assert!(Decor::contenu(&d.temporaire).is_empty(), "{:?}", Decor::contenu(&d.temporaire));
    // La commande part dans le bac, avec le temporaire du bac en `TMPDIR` ; le journal est effacé.
    let env = notes_du_faux(&d, "env.txt");
    assert_eq!(env[..2], [format!("tmpdir={}", d.temporaire.display()), format!("cwd={}", d.bac.display())]);
    assert!(!d.dir.join("journal.tmp").exists());
    let _ = fs::remove_dir_all(&d.dir);
}

/// Les octets du filtre de la machine : ceux que le fichier doit porter.
#[cfg(unix)]
fn filtre_attendu() -> Vec<u8> {
    let octets = filtre(architecture().unwrap());
    let instructions = octets.len().is_multiple_of(8);
    assert!(!octets.is_empty() && instructions, "un programme seccomp : des instructions de 8 octets");
    octets
}

#[cfg(unix)]
#[test]
fn le_code_de_sortie_est_rendu_et_le_filtre_est_retire() {
    let d = decor("natif-rouge");
    let faux = d.faux();
    let (verif, rejeu) = d.rejouer(&faux, &d.profil(""), "echo AVANT-LE-ROUGE; exit 3");
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("rouge (exit status: 3) :\nAVANT-LE-ROUGE"), "{erreur}");
    assert_eq!(verif, Some("AVANT-LE-ROUGE\n".to_string()));
    // Le faux a vu le fichier du filtre, et il n'est plus là ; rien d'autre ne reste, ni journal.
    let vus = notes_du_faux(&d, "filtres.txt");
    assert_eq!(vus.len(), 1, "{vus:?}");
    let filtre = vus[0].strip_prefix("present ").unwrap();
    assert!(filtre.starts_with(&*d.temporaire.to_string_lossy()), "{filtre}");
    assert!(!Path::new(filtre).exists());
    assert!(Decor::contenu(&d.temporaire).is_empty());
    assert!(!d.dir.join("journal.tmp").exists());
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn une_preuve_trop_lente_est_tuee_avec_ses_petits_enfants_et_le_filtre_est_retire() {
    use crate::essais::{ARBRE, LENTE, effacer, marque, meurt};
    let d = decor("natif-lente");
    let faux = d.faux();
    let profil = d.profil(&format!("delai_preuve = {}\n", ARBRE.as_secs()));
    let (verif, rejeu) = d.rejouer(&faux, &profil, LENTE);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("temps dépassé") && verif.is_some(), "elle a tourné, puis dépassé : {erreur}");
    let pid = marque(&d.bac, Duration::ZERO);
    assert!(meurt(pid), "le petit-enfant {pid} survit à la preuve tuée");
    assert_eq!(notes_du_faux(&d, "filtres.txt").len(), 1, "le filtre existait pendant le rejeu");
    assert!(Decor::contenu(&d.temporaire).is_empty());
    effacer(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_programme_qui_ne_part_pas_est_dit_et_ne_laisse_rien() {
    let d = decor("natif-introuvable");
    let absent = d.dir.join("pas-de-bwrap");
    let (verif, rejeu) = d.rejouer(absent.as_os_str(), &d.profil(""), "true");
    let erreur = rejeu.unwrap_err();
    assert!(erreur.starts_with("vérification « true » impossible : "), "{erreur}");
    assert_eq!(verif, None, "elle n'a pas pu partir");
    // Le filtre, écrit avant le lancement, est retiré ; le journal aussi.
    assert!(Decor::contenu(&d.temporaire).is_empty());
    assert!(!d.dir.join("journal.tmp").exists());
    // Le même décor, avec le faux, rend bien un dossier temporaire où le filtre se pose : le vide ci-dessus n'est pas
    // un regard sur le mauvais dossier.
    let faux = d.faux();
    assert_eq!(d.rejouer(&faux, &d.profil(""), "true").1, Ok(()));
    assert_eq!(notes_du_faux(&d, "filtres.txt").len(), 1);
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_chemin_lu_absent_est_saute_et_dit_en_tete_du_journal() {
    let d = decor("natif-absent");
    let faux = d.faux();
    let absent = d.dir.join("lecture-absente");
    let profil = d.profil(&format!("lecture = {}\n", absent.display()));
    let (verif, rejeu) = d.rejouer(&faux, &profil, "echo MARQUE-NATIF");
    assert_eq!(rejeu, Ok(()));
    let note = format!("mur natif : lu ou écrit par le profil, absent : {}\n", absent.display());
    assert_eq!(verif, Some(format!("{note}MARQUE-NATIF\n")));
    // Le mur ne monte rien à sa place.
    let appels = crate::essais::appels_du_faux_bwrap(&d.dir);
    assert_eq!(appels[0][..mur_attendu(&d, &[]).len()], mur_attendu(&d, &[])[..]);
    assert!(!appels[0].contains(&absent.to_string_lossy().into_owned()), "{:?}", appels[0]);
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_chemin_lu_present_se_monte_en_lecture_seule() {
    // Le pendant du test précédent : le même chemin, présent, est monté, et rien n'est dit.
    let d = decor("natif-present");
    let faux = d.faux();
    let present = d.dir.join("lecture");
    fs::create_dir(&present).unwrap();
    let profil = d.profil(&format!("lecture = {}\n", present.display()));
    let (verif, rejeu) = d.rejouer(&faux, &profil, "echo MARQUE-NATIF");
    assert_eq!(rejeu, Ok(()));
    assert_eq!(verif, Some("MARQUE-NATIF\n".to_string()));
    let mur = mur_attendu(&d, &[&present]);
    assert_eq!(crate::essais::appels_du_faux_bwrap(&d.dir)[0][..mur.len()], mur[..]);
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_chemin_dont_l_etat_est_inconnu_n_est_pas_pris_pour_absent() {
    // Un lien qui boucle sur lui-même : le disque répond « boucle », ni présent ni absent. Le prendre pour absent
    // laisserait peut-être ouvert un chemin que le profil ferme ou lit : le rejeu ne part pas.
    let d = decor("natif-boucle");
    let faux = d.faux();
    let (a, b) = (d.dir.join("boucle-a"), d.dir.join("boucle-b"));
    std::os::unix::fs::symlink(&b, &a).unwrap();
    std::os::unix::fs::symlink(&a, &b).unwrap();
    let (verif, rejeu) = d.rejouer(&faux, &d.profil(&format!("lecture = {}\n", a.display())), "echo MARQUE-NATIF");
    let erreur = rejeu.unwrap_err();
    assert!(erreur.starts_with("vérification « echo MARQUE-NATIF » impossible : "), "{erreur}");
    assert!(erreur.contains(&format!("chemin {}", a.display())), "{erreur}");
    assert_eq!(verif, None);
    assert!(crate::essais::appels_du_faux_bwrap(&d.dir).is_empty(), "rien ne part");
    assert!(Decor::contenu(&d.temporaire).is_empty(), "le filtre n'est pas écrit");
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn la_cle_du_fournisseur_ne_passe_pas_a_la_preuve() {
    // `HOME` tient la place de la clé du fournisseur : une variable que le rejeu a sûrement, et qui ne doit pas passer.
    // `PATH`, qui n'est pas la clé, passe : sans lui, une sortie sans la clé ne prouverait rien.
    let d = decor("natif-cle");
    let faux = d.faux();
    let profil = d.profil("api = https://example.com/v1\ncle = HOME\n");
    let commande = r#"echo "cle=${HOME-absente}"; [ -n "$PATH" ] && echo chemin-present"#;
    let (verif, rejeu) = d.rejouer(&faux, &profil, commande);
    assert_eq!(rejeu, Ok(()));
    assert_eq!(verif, Some("cle=absente\nchemin-present\n".to_string()));
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn un_profil_qui_permet_des_domaines_est_refuse_avant_tout_lancement() {
    let d = decor("natif-domaines");
    let avant = Decor::contenu(&d.dir);
    assert!(avant.contains("bac") && !avant.contains(&temporaire_nom(&d)), "{avant:?}");
    let profil = d.profil("reseau = exemple.test, autre.test\n");
    let programme = d.dir.join("pas-de-bwrap");
    let (verif, rejeu) = d.rejouer(programme.as_os_str(), &profil, "true");
    let erreur = rejeu.unwrap_err();
    // Le refus dit pourquoi, et nomme les domaines ; ce n'est pas l'erreur d'un programme qui ne part pas.
    let debut = "vérification « true » impossible : le mur natif coupe le réseau en entier";
    assert!(erreur.starts_with(debut), "{erreur}");
    assert!(erreur.contains("(exemple.test, autre.test)"), "{erreur}");
    assert_eq!(verif, None);
    // Rien n'est créé : ni dossier temporaire, ni filtre, ni journal ; le bac est intact.
    assert_eq!(Decor::contenu(&d.dir), avant);
    assert!(Decor::contenu(&d.bac).is_empty());
    let _ = fs::remove_dir_all(&d.dir);
}

/// Le nom du dossier temporaire du bac de ce décor, dans son dossier.
fn temporaire_nom(d: &Decor) -> String {
    d.temporaire.file_name().unwrap().to_string_lossy().into_owned()
}

// La préparation entière lit le disque par `canonicalize`, verbatim sous Windows : elle ne se joue que sous Unix.
#[cfg(unix)]
#[test]
fn sans_domaine_le_meme_profil_se_prepare() {
    // Le pendant du refus : sans `reseau`, la préparation aboutit, et crée le dossier temporaire et le filtre.
    let d = decor("natif-prepare");
    let mur = preparer(&d.maison, &d.profil(""), &d.bac).unwrap();
    assert!(mur.filtre.is_file() && mur.filtre.parent() == Some(d.temporaire.as_path()), "{:?}", mur.filtre);
    assert_eq!(mur.temporaire, d.temporaire);
    assert!(mur.notes.is_empty(), "{:?}", mur.notes);
    assert!(Decor::contenu(&d.dir).contains(&temporaire_nom(&d)));
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn un_profil_qui_n_exige_pas_le_mur_ne_part_pas() {
    let d = decor("natif-demande");
    let avant = Decor::contenu(&d.dir);
    let programme = d.dir.join("pas-de-bwrap");
    let demande = profil_borne("sorte = claude\nmur = demande\n", &d.dir);
    let brute = Profil::faux(vec!["x".into()], Duration::from_secs(30));
    for profil in [demande, brute] {
        let (verif, rejeu) = d.rejouer(programme.as_os_str(), &profil, "true");
        let erreur = rejeu.unwrap_err();
        assert!(erreur.contains("le profil n'exige pas le mur"), "{erreur}");
        assert_eq!(verif, None);
    }
    assert_eq!(Decor::contenu(&d.dir), avant, "rien n'est créé");
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn une_commande_blanche_ne_part_pas() {
    let d = decor("natif-blanche");
    let avant = Decor::contenu(&d.dir);
    for commande in ["", " ", "\t \r\n"] {
        let (verif, rejeu) = d.rejouer(OsStr::new("pas-de-bwrap"), &d.profil(""), commande);
        let erreur = rejeu.unwrap_err();
        assert!(erreur.contains("blanche : rien ne se rejoue, rien n'est prouvé"), "{erreur}");
        assert_eq!(verif, None, "{commande:?}");
    }
    assert_eq!(Decor::contenu(&d.dir), avant);
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn le_programme_et_la_maison_se_lisent_de_l_environnement() {
    assert_eq!(programme_selon(None), "bwrap");
    assert_eq!(programme_selon(Some(OsString::new())), "bwrap");
    assert_eq!(programme_selon(Some("/srv/outils/bwrap".into())), "/srv/outils/bwrap");
    assert_eq!(maison_selon(Some("/srv/personnel".into())), Ok(PathBuf::from("/srv/personnel")));
    for sans in [None, Some(OsString::new()), Some("personnel".into())] {
        let erreur = maison_selon(sans.clone()).unwrap_err();
        assert!(erreur.starts_with("HOME ne donne pas de dossier personnel absolu"), "{sans:?} : {erreur}");
    }
}
