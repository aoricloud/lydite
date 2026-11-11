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
    decor_dans(dossier(nom))
}

/// Un décor dont le dossier a un nom court (neuf, `n` et huit hexadécimaux du lot) : le socket du filtre réseau, qui
/// est à côté du temporaire du bac, doit tenir dans 107 octets sous le dossier temporaire de la machine.
fn decor_court() -> Decor {
    let dir = env::temp_dir().join(format!("n{}", &hex(&sha256(lot().as_bytes()))[..8]));
    fs::create_dir_all(&dir).unwrap();
    decor_dans(dir)
}

/// Le décor monté dans `dir`, un dossier neuf.
fn decor_dans(dir: PathBuf) -> Decor {
    // Sous Unix, le chemin réel (`mur_attendu` compare des chemins réels) ; sous Windows, tel que le runner l'écrit :
    // `canonicalize` y rend un chemin verbatim (`\\?\C:\...`), que le profil borné refuse.
    #[cfg(unix)]
    let dir = fs::canonicalize(dir).unwrap();
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

    /// Le rejeu de `commande` sous le programme `programme`, le journal du décor, et ces `reglage`s du filtre réseau.
    fn rejouer_avec(
        &self,
        programme: &OsStr,
        profil: &Profil,
        commande: &str,
        reglage: &Reglage,
    ) -> (Option<String>, Result<(), String>) {
        let journal = self.dir.join("journal.tmp");
        rejouer_avec(programme, &self.maison, profil, commande, &self.bac, &journal, reglage)
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

/// Une échéance du filtre que les tests ne font jamais tomber : ils n'attendent que le filtre, qui répond aussitôt.
const ATTENTE: Duration = Duration::from_secs(60);

/// Le texte de la commande qui écrit les huit variables de proxy, une ligne chacune : `absente` pour une variable que
/// l'environnement ne pose pas.
const AFFICHE_LES_PROXY: &str = concat!(
    "for v in HTTP_PROXY HTTPS_PROXY http_proxy https_proxy ALL_PROXY all_proxy NO_PROXY no_proxy; ",
    r#"do eval "echo $v=\${$v-absente}"; done"#,
);

/// Ce que [`AFFICHE_LES_PROXY`] écrit quand `valeur` donne celle de chaque variable.
fn proxy_attendus(valeur: impl Fn(&str) -> String) -> String {
    let noms =
        ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy", "ALL_PROXY", "all_proxy", "NO_PROXY", "no_proxy"];
    noms.iter().map(|nom| format!("{nom}={}\n", valeur(nom))).collect()
}

#[test]
fn un_filtre_absent_est_refuse_avant_tout_lancement() {
    // Le profil permet des domaines, `lydite-reseau` manque : refus net, qui nomme les deux façons de le donner.
    let d = decor("natif-filtre-absent");
    let avant = Decor::contenu(&d.dir);
    assert!(avant.contains("bac") && !avant.contains(&temporaire_nom(&d)), "{avant:?}");
    let profil = d.profil("reseau = exemple.test, autre.test\n");
    let programme = d.dir.join("pas-de-bwrap");
    let absent = d.dir.join("pas-de-lydite-reseau");
    let reglage = Reglage { filtre: Ok(absent.clone()), pret: ATTENTE, arret: ATTENTE };
    let (verif, rejeu) = d.rejouer_avec(programme.as_os_str(), &profil, "true", &reglage);
    let erreur = rejeu.unwrap_err();
    let debut = "vérification « true » impossible : the profile allows domains, so the native wall needs lydite-reseau";
    assert!(erreur.starts_with(debut), "{erreur}");
    assert!(erreur.contains(&absent.display().to_string()), "{erreur}");
    assert!(erreur.contains("LYDITE_RESEAU") && erreur.contains("next to the Lydite executable"), "{erreur}");
    assert_eq!(verif, None);
    // Rien n'est créé : ni dossier temporaire, ni dossier de socket, ni filtre, ni journal ; le bac est intact.
    assert_eq!(Decor::contenu(&d.dir), avant);
    assert!(Decor::contenu(&d.bac).is_empty());
    // Un programme que ni la variable ni l'exécutable ne donnent est refusé de même, avant de toucher le disque.
    let reglage = Reglage { filtre: Err("LYDITE_RESEAU is not set".into()), ..reglage };
    let (_, rejeu) = d.rejouer_avec(programme.as_os_str(), &profil, "true", &reglage);
    assert_eq!(rejeu, Err("vérification « true » impossible : LYDITE_RESEAU is not set".to_string()));
    assert_eq!(Decor::contenu(&d.dir), avant);
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn le_programme_du_filtre_se_lit_de_l_environnement_puis_de_l_executable() {
    let exe = || Ok(PathBuf::from("/srv/lydite/lydite"));
    let voisin = Path::new("/srv/lydite").join(format!("lydite-reseau{}", env::consts::EXE_SUFFIX));
    // La variable l'emporte, sans que le disque ou l'exécutable soient lus ; vide, elle ne compte pas.
    let inconnu = || Err(io::Error::other("inconnu"));
    assert_eq!(
        reseau_selon(Some("/srv/outils/lydite-reseau".into()), inconnu()),
        Ok("/srv/outils/lydite-reseau".into())
    );
    assert_eq!(reseau_selon(Some("/srv/outils/lydite-reseau".into()), exe()), Ok("/srv/outils/lydite-reseau".into()));
    assert_eq!(reseau_selon(None, exe()), Ok(voisin.clone()));
    assert_eq!(reseau_selon(Some(OsString::new()), exe()), Ok(voisin));
    // Ni l'une ni l'autre : le refus nomme les deux façons de le donner.
    let erreur = reseau_selon(None, inconnu()).unwrap_err();
    assert!(erreur.contains("inconnu"), "{erreur}");
    assert!(erreur.contains("LYDITE_RESEAU") && erreur.contains("next to the Lydite executable"), "{erreur}");
}

#[test]
fn un_chemin_de_socket_de_plus_de_107_octets_est_refuse() {
    // La limite est exacte : 107 octets passent, 108 non.
    let chemin = |octets: usize| PathBuf::from(format!("/{}", "a".repeat(octets - 1)));
    assert_eq!(socket_assez_court(&chemin(107)), Ok(()));
    let erreur = socket_assez_court(&chemin(108)).unwrap_err();
    assert!(erreur.contains("is 108 bytes long") && erreur.contains("holds 107 at most"), "{erreur}");
    assert!(erreur.contains("use a shorter sandbox folder"), "{erreur}");
}

/// Le faux `lydite-reseau` du décor, avec des échéances larges : celui des tests dont le filtre doit dire `ready`.
#[cfg(unix)]
fn reglage_du_faux(d: &Decor) -> Reglage {
    Reglage { filtre: Ok(crate::essais::faux_reseau(&d.dir)), pret: ATTENTE, arret: ATTENTE }
}

/// Les notes du faux `lydite-reseau` du décor, dans son fichier `nom`, une par ligne.
#[cfg(unix)]
fn notes_du_faux_reseau(d: &Decor, nom: &str) -> Vec<String> {
    let texte = fs::read_to_string(d.dir.join("reseau").join(nom)).unwrap_or_default();
    texte.lines().map(String::from).collect()
}

/// Le filtre du faux, lancé une fois : ses arguments, d'où le dossier de son socket.
#[cfg(unix)]
fn filtre_lance(d: &Decor) -> Vec<String> {
    let appels = crate::essais::appels_du_faux_reseau(&d.dir, "filtres");
    assert_eq!(appels.len(), 1, "un filtre lancé : {appels:?}");
    appels[0].clone()
}

/// Rien ne reste d'un filtre lancé : le dossier de son socket est retiré, et ni lui ni ses enfants ne vivent.
#[cfg(unix)]
fn plus_de_filtre(d: &Decor, filtre: &[String]) {
    let dossier = Path::new(&filtre[2]).parent().unwrap();
    // Le faux a vu ce dossier à son lancement (le même chemin, par `[ -d ]`) : son absence ensuite n'est pas un regard
    // sur le mauvais chemin.
    assert_eq!(notes_du_faux_reseau(d, "dossiers.txt").len(), 1, "le dossier existait au lancement du filtre");
    assert!(!dossier.exists(), "le dossier du socket reste : {}", dossier.display());
    assert!(!notes_du_faux_reseau(d, "pids.txt").is_empty(), "le faux note son pid");
    rien_ne_reste_du_filtre(d);
}

/// Aucun dossier de socket ne reste à côté du temporaire du bac, et chaque pid que le faux a noté est mort : ne dépend
/// pas de l'avancée du faux, qui a pu noter tous ses pids, ou aucun.
#[cfg(unix)]
fn rien_ne_reste_du_filtre(d: &Decor) {
    let reste: Vec<String> = Decor::contenu(&d.dir).into_iter().filter(|nom| nom.ends_with(".net")).collect();
    assert!(reste.is_empty(), "{reste:?}");
    for pid in notes_du_faux_reseau(d, "pids.txt") {
        assert!(crate::essais::meurt(pid.parse().unwrap()), "le processus {pid} du filtre vit encore");
    }
}

#[cfg(unix)]
#[test]
fn avec_des_domaines_le_filtre_est_pret_avant_le_bac_et_les_deux_appels_ont_leur_forme() {
    let d = decor_court();
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    let profil = d.profil("reseau = exemple.test, autre.test\n");
    let ordre = d.dir.join("reseau").join("ordre.txt");
    let commande = format!(r#"echo bac >> "{}"; {AFFICHE_LES_PROXY}"#, ordre.display());
    let (verif, rejeu) = d.rejouer_avec(&faux, &profil, &commande, &reglage);
    assert_eq!(rejeu, Ok(()));
    // Les variables de la commande : proxy sur le relais, et l'hôte local hors du proxy.
    let valeur = |nom: &str| match nom.to_ascii_uppercase().as_str() {
        "NO_PROXY" => "localhost,127.0.0.1,::1".to_string(),
        _ => "http://127.0.0.1:3128".to_string(),
    };
    assert_eq!(verif, Some(proxy_attendus(valeur)));
    // Le filtre : lancé hors du bac avec le socket dans un dossier neuf à côté du temporaire, les domaines des règles
    // et son journal dans le dossier de Lydite. Son dossier, créé avant lui, est en 0700 ; son entrée est un tuyau.
    let filtre = filtre_lance(&d);
    let socket = PathBuf::from(&filtre[2]);
    assert_eq!(socket.file_name().unwrap(), "s");
    let dossier = socket.parent().unwrap();
    assert!(dossier.file_name().unwrap().to_string_lossy().ends_with(".net"), "{}", dossier.display());
    assert_eq!(dossier.parent(), d.temporaire.parent(), "à côté du temporaire du bac");
    let journal = d.kraken.join("reseau.journal").to_string_lossy().into_owned();
    let attendu = ["filtre", "--socket", &socket.to_string_lossy(), "--domaines", "exemple.test,autre.test"];
    assert_eq!(filtre, [&attendu[..], &["--journal", &journal]].concat());
    assert_eq!(notes_du_faux_reseau(&d, "dossiers.txt"), ["drwx------"]);
    assert_eq!(notes_du_faux_reseau(&d, "entrees.txt"), ["tuyau"]);
    // Le bwrap extérieur : les arguments du mur, le dossier du socket et `lydite-reseau` montés, puis le relais, qui
    // lance la ligne du verrou.
    let programme = fs::canonicalize(reglage.filtre.as_ref().unwrap()).unwrap().to_string_lossy().into_owned();
    let dossier_texte = dossier.to_string_lossy().into_owned();
    let appels = crate::essais::appels_du_faux_bwrap(&d.dir);
    assert_eq!(appels.len(), 2, "un bwrap extérieur, un second dedans : {appels:?}");
    let exterieur = &appels[0];
    let mur = mur_attendu(&d, &[]);
    assert_eq!(exterieur[..mur.len()], mur[..]);
    let bpf = exterieur[exterieur.len() - 3].clone();
    assert!(Path::new(&bpf).starts_with(&d.temporaire), "{bpf}");
    let relais = ["relais", "--ecoute", "127.0.0.1:3128", "--vers", &socket.to_string_lossy(), "--"].map(String::from);
    let ligne = ["sh", "-c", POSE_LE_VERROU, "sh", &bpf, &faux.to_string_lossy(), &commande].map(String::from);
    let montages = ["--bind", &dossier_texte, &dossier_texte, "--ro-bind", &programme, &programme, "--", &programme];
    let relayee: Vec<String> = relais.iter().chain(&ligne).cloned().collect();
    let attendu: Vec<String> = montages.map(String::from).into_iter().chain(relayee.clone()).collect();
    assert_eq!(exterieur[mur.len()..], attendu[..]);
    // Le relais reçoit ce qui le suit : l'écoute, le socket, puis la même ligne du verrou.
    assert_eq!(crate::essais::appels_du_faux_reseau(&d.dir, "relais"), [relayee]);
    let verrou = "--bind / / --dev-bind /dev /dev --unshare-pid --proc /proc --die-with-parent --seccomp 9 -- sh -c";
    let second: Vec<String> = verrou.split(' ').map(String::from).chain([commande.clone()]).collect();
    assert_eq!(appels[1], second);
    // L'ordre : filtre prêt, relais, bac ; et l'entrée du filtre ne se ferme qu'après la fin du bac.
    assert_eq!(notes_du_faux_reseau(&d, "ordre.txt"), ["pret", "relais", "bac", "entree-fermee"]);
    plus_de_filtre(&d, &filtre);
    assert!(Decor::contenu(&d.temporaire).is_empty(), "le fichier du filtre d'appels système est retiré");
    assert!(!d.dir.join("journal.tmp").exists());
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn apres_un_echec_le_filtre_est_arrete_et_son_dossier_retire() {
    let d = decor_court();
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = d.rejouer_avec(&faux, &profil, "echo AVANT-LE-ROUGE; exit 3", &reglage);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("rouge (exit status: 3) :\nAVANT-LE-ROUGE"), "{erreur}");
    assert_eq!(verif, Some("AVANT-LE-ROUGE\n".to_string()));
    // Le filtre a fini par l'entrée fermée, comme après un succès : Lydite l'a arrêté, il n'est pas mort de lui-même.
    assert_eq!(notes_du_faux_reseau(&d, "ordre.txt"), ["pret", "relais", "entree-fermee"]);
    plus_de_filtre(&d, &filtre_lance(&d));
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn apres_un_delai_depasse_le_filtre_est_arrete_et_son_dossier_retire() {
    use crate::essais::{ARBRE, LENTE, effacer, marque, meurt};
    let d = decor_court();
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    let profil = d.profil(&format!("reseau = exemple.test\ndelai_preuve = {}\n", ARBRE.as_secs()));
    let (verif, rejeu) = d.rejouer_avec(&faux, &profil, LENTE, &reglage);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.contains("temps dépassé") && verif.is_some(), "elle a tourné, puis dépassé : {erreur}");
    // La preuve tuée, son arbre meurt, le relais compris ; puis le filtre s'arrête, entrée fermée.
    let pid = marque(&d.bac, Duration::ZERO);
    assert!(meurt(pid), "le petit-enfant {pid} survit à la preuve tuée");
    assert_eq!(notes_du_faux_reseau(&d, "ordre.txt"), ["pret", "relais", "entree-fermee"]);
    plus_de_filtre(&d, &filtre_lance(&d));
    effacer(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_filtre_qui_ne_dit_jamais_ready_est_refuse_a_l_echeance_sans_rien_laisser() {
    let d = decor_court();
    let faux = d.faux();
    let reglage = Reglage { pret: Duration::from_secs(1), arret: Duration::from_millis(500), ..reglage_du_faux(&d) };
    fs::write(d.dir.join("reseau").join("muet"), "").unwrap();
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = d.rejouer_avec(&faux, &profil, "echo MARQUE-NATIF", &reglage);
    let erreur = rejeu.unwrap_err();
    let debut = "vérification « echo MARQUE-NATIF » impossible : the network filter ";
    assert!(erreur.starts_with(debut) && erreur.ends_with(" did not say ready within 1 s"), "{erreur}");
    assert_eq!(verif, None);
    // Seul ce qui ne dépend pas de l'avancée du faux (il a pu écrire toutes ses notes, ou aucune) : le bac n'est pas
    // parti, le dossier du socket est retiré, et chaque processus que le faux a noté est mort, l'arbre entier tué. Le
    // délai lui-même se montre sur un canal préparé : `attendre_pret_rend_muet_a_l_echeance_et_mort_a_la_fermeture`.
    assert!(crate::essais::appels_du_faux_bwrap(&d.dir).is_empty(), "rien ne part");
    rien_ne_reste_du_filtre(&d);
    assert!(Decor::contenu(&d.temporaire).is_empty(), "le fichier du filtre d'appels système est retiré");
    assert!(!d.dir.join("journal.tmp").exists());
    let _ = fs::remove_dir_all(&d.dir);
}

#[test]
fn attendre_pret_rend_muet_a_l_echeance_et_mort_a_la_fermeture() {
    let (envoi, lignes) = mpsc::channel::<String>();
    // L'envoi reste vivant : une ligne qui n'est pas `ready` ne compte pas, puis l'échéance tombe.
    envoi.send("autre chose".into()).unwrap();
    assert!(matches!(attendre_pret(&lignes, Duration::from_millis(50)), Err(Silence::Muet)));
    // La ligne `ready` arrive : le filtre est prêt.
    envoi.send(PRET.into()).unwrap();
    assert!(attendre_pret(&lignes, ATTENTE).is_ok());
    // L'envoi lâché : la sortie du filtre s'est fermée, c'est qu'il est mort, sans attendre l'échéance.
    drop(envoi);
    assert!(matches!(attendre_pret(&lignes, ATTENTE), Err(Silence::Mort)));
}

#[cfg(unix)]
#[test]
fn une_commande_qui_ne_part_pas_et_un_filtre_qui_ne_s_arrete_pas_sont_dits_tous_les_deux() {
    let d = decor_court();
    let reglage = reglage_du_faux(&d);
    // Le faux remplace le dossier de son socket par un fichier : il ne se retire pas, et l'arrêt du filtre échoue.
    fs::write(d.dir.join("reseau").join("fichier"), "").unwrap();
    let absent = d.dir.join("pas-de-bwrap");
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = d.rejouer_avec(absent.as_os_str(), &profil, "true", &reglage);
    let erreur = rejeu.unwrap_err();
    assert_eq!(verif, None, "elle n'a pas pu partir");
    // La commande d'abord, sur sa ligne ; puis l'échec de l'arrêt, sur la sienne, qui nomme le dossier.
    let (commande, arret) = erreur.split_once('\n').unwrap_or((&erreur, ""));
    assert!(commande.starts_with("vérification « true » impossible : "), "{erreur}");
    assert!(!commande.contains("network filter"), "{erreur}");
    let dossier = Path::new(&filtre_lance(&d)[2]).parent().unwrap().to_path_buf();
    let attendu = format!("cannot remove the network filter folder {}: ", dossier.display());
    assert!(arret.starts_with(&attendu), "{erreur}");
    // Le dossier est bien devenu un fichier, que l'arrêt n'a pas pu retirer : l'échec est réel, pas dit pour rien.
    assert!(dossier.is_file(), "{}", dossier.display());
    // Le filtre, lui, est arrêté : son entrée est fermée, il est sorti.
    for pid in notes_du_faux_reseau(&d, "pids.txt") {
        assert!(crate::essais::meurt(pid.parse().unwrap()), "le processus {pid} du filtre vit encore");
    }
    assert!(Decor::contenu(&d.temporaire).is_empty());
    assert!(!d.dir.join("journal.tmp").exists());
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn une_commande_qui_ne_part_pas_sans_echec_d_arret_ne_dit_que_son_erreur() {
    // Le pendant : le même faux, sans le fichier, s'arrête bien ; l'erreur est celle de la commande, seule.
    let d = decor_court();
    let reglage = reglage_du_faux(&d);
    let absent = d.dir.join("pas-de-bwrap");
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = d.rejouer_avec(absent.as_os_str(), &profil, "true", &reglage);
    let erreur = rejeu.unwrap_err();
    assert_eq!(verif, None);
    assert!(erreur.starts_with("vérification « true » impossible : ") && !erreur.contains('\n'), "{erreur}");
    plus_de_filtre(&d, &filtre_lance(&d));
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_filtre_qui_meurt_avant_ready_est_refuse_avec_ce_qu_il_dit() {
    let d = decor_court();
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    fs::write(d.dir.join("reseau").join("mort"), "").unwrap();
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = d.rejouer_avec(&faux, &profil, "echo MARQUE-NATIF", &reglage);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.starts_with("vérification « echo MARQUE-NATIF » impossible : the network filter "), "{erreur}");
    assert!(erreur.ends_with(" exited before it said ready (exit status: 3): boom"), "{erreur}");
    assert_eq!(verif, None);
    assert!(crate::essais::appels_du_faux_bwrap(&d.dir).is_empty(), "rien ne part");
    plus_de_filtre(&d, &filtre_lance(&d));
    assert!(Decor::contenu(&d.temporaire).is_empty());
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn un_socket_trop_long_est_refuse_avant_de_lancer() {
    let d = decor("natif-reseau-long");
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    // Un bac dont le dossier a un nom de 110 octets : son socket, à côté de son temporaire, dépasse 107 octets.
    let parent = d.dir.join("l".repeat(110));
    let bac = parent.join("bac");
    fs::create_dir_all(&bac).unwrap();
    let avant = Decor::contenu(&parent);
    let profil = d.profil("reseau = exemple.test\n");
    let (verif, rejeu) = rejouer_avec(&faux, &d.maison, &profil, "true", &bac, &d.dir.join("journal.tmp"), &reglage);
    let erreur = rejeu.unwrap_err();
    assert!(erreur.starts_with("vérification « true » impossible : the network filter socket path "), "{erreur}");
    assert!(erreur.contains(" bytes long") && erreur.contains("use a shorter sandbox folder"), "{erreur}");
    assert_eq!(verif, None);
    // Ni filtre, ni bwrap, ni dossier : rien n'est créé.
    assert!(crate::essais::appels_du_faux_reseau(&d.dir, "filtres").is_empty());
    assert!(crate::essais::appels_du_faux_bwrap(&d.dir).is_empty());
    assert_eq!(Decor::contenu(&parent), avant);
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn sans_domaine_aucun_filtre_ne_part_et_aucune_variable_n_est_posee() {
    let d = decor_court();
    let (faux, reglage) = (d.faux(), reglage_du_faux(&d));
    let (verif, rejeu) = d.rejouer_avec(&faux, &d.profil(""), AFFICHE_LES_PROXY, &reglage);
    assert_eq!(rejeu, Ok(()));
    // Les variables de la commande sont celles de Lydite, sans une de plus.
    let de_lydite = |nom: &str| env::var(nom).unwrap_or_else(|_| "absente".into());
    assert_eq!(verif, Some(proxy_attendus(de_lydite)));
    // Pas de filtre, pas de relais, pas de montage, la chaîne d'avant : le réseau reste coupé en entier.
    assert!(crate::essais::appels_du_faux_reseau(&d.dir, "filtres").is_empty());
    assert!(crate::essais::appels_du_faux_reseau(&d.dir, "relais").is_empty());
    let appels = crate::essais::appels_du_faux_bwrap(&d.dir);
    let mur = mur_attendu(&d, &[]);
    assert_eq!(appels[0][..mur.len()], mur[..]);
    assert_eq!(appels[0][mur.len()..mur.len() + 5], ["--", "sh", "-c", POSE_LE_VERROU, "sh"].map(String::from));
    assert!(appels[0].contains(&"--unshare-net".to_string()));
    let dossiers: Vec<String> = Decor::contenu(&d.dir).into_iter().filter(|nom| nom.ends_with(".net")).collect();
    assert!(dossiers.is_empty(), "{dossiers:?}");
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
    let reglage = Reglage { filtre: Err("sans domaine, le filtre n'est pas lu".into()), pret: ATTENTE, arret: ATTENTE };
    let mur = preparer(&d.maison, &d.profil(""), &d.bac, &reglage).unwrap();
    assert!(mur.filtre.is_file() && mur.filtre.parent() == Some(d.temporaire.as_path()), "{:?}", mur.filtre);
    assert_eq!(mur.temporaire, d.temporaire);
    assert!(mur.notes.is_empty(), "{:?}", mur.notes);
    assert!(mur.reseau.is_none(), "sans domaine, pas de filtre réseau");
    assert!(Decor::contenu(&d.dir).contains(&temporaire_nom(&d)));
    let _ = fs::remove_dir_all(&d.dir);
}

#[cfg(unix)]
#[test]
fn avec_des_domaines_le_filtre_reseau_se_prevoit_sans_rien_creer() {
    // Le pendant du test précédent : le même profil, avec des domaines, prévoit le filtre à côté du temporaire du bac,
    // le journal dans le dossier de Lydite, et ne crée encore ni le dossier du socket ni le socket.
    let d = decor_court();
    let reglage = reglage_du_faux(&d);
    let mur = preparer(&d.maison, &d.profil("reseau = exemple.test, autre.test\n"), &d.bac, &reglage).unwrap();
    let reseau = mur.reseau.unwrap();
    assert_eq!(reseau.domaines, ["exemple.test", "autre.test"]);
    assert_eq!(reseau.programme, fs::canonicalize(reglage.filtre.unwrap()).unwrap());
    assert_eq!(reseau.dossier.parent(), d.temporaire.parent());
    assert_eq!(reseau.socket, reseau.dossier.join("s"));
    assert_eq!(reseau.journal, d.kraken.join("reseau.journal"));
    assert!(!reseau.dossier.exists() && !reseau.socket.exists());
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
