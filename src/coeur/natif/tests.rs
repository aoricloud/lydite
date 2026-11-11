use super::*;

/// Un chemin du faux disque : le chemin, son chemin réel (le sien, sauf pour un lien), et s'il est un dossier.
type Entree = (String, String, bool);

fn dossier(chemin: &str) -> Entree {
    (chemin.into(), chemin.into(), true)
}

fn fichier(chemin: &str) -> Entree {
    (chemin.into(), chemin.into(), false)
}

fn lien(chemin: &str, reel: &str, dossier: bool) -> Entree {
    (chemin.into(), reel.into(), dossier)
}

/// Le disque des tests : ce qu'il ne tient pas est absent. Rien n'est lu ni créé sur le vrai disque.
struct Faux(Vec<Entree>);

impl Faux {
    fn sonder(&self, chemin: &Path) -> Disque {
        match self.0.iter().find(|(connu, _, _)| Path::new(connu) == chemin) {
            None => Disque::Absent,
            Some((_, reel, true)) => Disque::Dossier(PathBuf::from(reel)),
            Some((_, reel, false)) => Disque::Fichier(PathBuf::from(reel)),
        }
    }
}

fn chemins(textes: &[&str]) -> Vec<PathBuf> {
    textes.iter().map(PathBuf::from).collect()
}

/// Les règles d'un cas, que `jouer` passe à `arguments`.
#[derive(Default, Clone)]
struct Cas {
    fermes: Vec<PathBuf>,
    lus: Vec<PathBuf>,
    ecrits: Vec<PathBuf>,
    refuses: Vec<PathBuf>,
    bac: PathBuf,
    toujours: Vec<&'static str>,
}

impl Cas {
    fn jouer(&self, faux: &Faux) -> Result<Natif, String> {
        let regles = Regles {
            maison: Path::new("/srv/maison"),
            fermes: &self.fermes,
            lus: &self.lus,
            ecrits: &self.ecrits,
            refuses: &self.refuses,
            bac: &self.bac,
            toujours: &self.toujours,
        };
        arguments(&regles, |chemin| faux.sonder(chemin))
    }

    /// Les arguments rendus, ou l'échec du test si les règles sont refusées.
    fn args(&self, faux: &Faux) -> Vec<String> {
        lignes(&self.jouer(faux).expect("des règles que le mur accepte"))
    }
}

/// Les arguments en texte, la barre inverse de Windows écrite en `/` : les attendus s'écrivent à la Unix.
fn lignes(natif: &Natif) -> Vec<String> {
    natif.arguments.iter().map(|argument| argument.to_string_lossy().replace('\\', "/")).collect()
}

/// Où la suite `motif` figure, d'un seul tenant, dans `args`.
fn place(args: &[String], motif: &[&str]) -> Option<usize> {
    args.windows(motif.len()).position(|fenetre| fenetre.iter().zip(motif).all(|(a, m)| a == m))
}

/// Les destinations des montages de `option` (`--ro-bind`, `--bind`), dans l'ordre, sans le montage de la racine.
fn destinations(args: &[String], option: &str) -> Vec<String> {
    let montages = (0..args.len()).filter(|&i| args[i] == option && i + 2 < args.len());
    montages.map(|i| args[i + 2].clone()).filter(|dest| dest != "/").collect()
}

/// Les arguments fixes qui ouvrent la liste, avant tout chemin.
const OUVERTURE: [&str; 12] = [
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
];

/// Un cas qui a un dossier fermé et un fichier fermé, des chemins lus sous le dossier fermé et hors de lui, le bac et
/// son temporaire écrits sous le dossier fermé, le git commun lu et refusé en écriture, et deux des fichiers toujours
/// refusés dans le bac.
fn cas_complet() -> (Cas, Faux) {
    let cas = Cas {
        fermes: chemins(&["/srv/maison", "/srv/secret.env"]),
        lus: chemins(&["/srv/maison/.cargo", "/srv/depot/.git", "/srv/maison/bac"]),
        ecrits: chemins(&["/srv/maison/bac", "/srv/maison/tmp"]),
        refuses: chemins(&["/srv/depot/.git"]),
        bac: PathBuf::from("/srv/maison/bac"),
        toujours: TOUJOURS_REFUSES.to_vec(),
    };
    let faux = Faux(vec![
        dossier("/srv/maison"),
        fichier("/srv/secret.env"),
        dossier("/srv/maison/.cargo"),
        dossier("/srv/depot/.git"),
        dossier("/srv/maison/bac"),
        dossier("/srv/maison/tmp"),
        fichier("/srv/maison/bac/.gitconfig"),
        dossier("/srv/maison/bac/.claude"),
        dossier("/srv/maison/bac/.claude/commands"),
    ]);
    (cas, faux)
}

#[test]
fn l_ordre_des_arguments_est_celui_du_mur() {
    let (cas, faux) = cas_complet();
    let mut attendus: Vec<&str> = OUVERTURE.to_vec();
    attendus.extend([
        // fermé : un dossier en tmpfs, un fichier par /dev/null
        "--tmpfs",
        "/srv/maison",
        "--ro-bind",
        "/dev/null",
        "/srv/secret.env",
        // lus, dans l'ordre des règles
        "--ro-bind",
        "/srv/maison/.cargo",
        "/srv/maison/.cargo",
        "--ro-bind",
        "/srv/depot/.git",
        "/srv/depot/.git",
        "--ro-bind",
        "/srv/maison/bac",
        "/srv/maison/bac",
        // écrits
        "--bind",
        "/srv/maison/bac",
        "/srv/maison/bac",
        "--bind",
        "/srv/maison/tmp",
        "/srv/maison/tmp",
        // refusés en écriture : la règle, puis ce qui est toujours refusé dans le bac
        "--ro-bind",
        "/srv/depot/.git",
        "/srv/depot/.git",
        "--ro-bind",
        "/srv/maison/bac/.gitconfig",
        "/srv/maison/bac/.gitconfig",
        "--ro-bind",
        "/srv/maison/bac/.claude/commands",
        "/srv/maison/bac/.claude/commands",
        // l'ancêtre épinglé
        "--ro-bind",
        "/srv/maison/bac/.claude",
        "/srv/maison/bac/.claude",
        "--dev",
        "/dev",
        "--proc",
        "/proc",
    ]);
    assert_eq!(cas.args(&faux), attendus);
    assert!(cas.jouer(&faux).unwrap().sautes.is_empty());
}

#[test]
fn un_tmpfs_vient_toujours_avant_le_montage_de_ce_quil_couvre() {
    let (cas, faux) = cas_complet();
    let args = cas.args(&faux);
    let tmpfs: Vec<(usize, &str)> =
        (0..args.len()).filter(|&i| args[i] == "--tmpfs").map(|i| (i, args[i + 1].as_str())).collect();
    assert_eq!(tmpfs.len(), 1, "le cas a un dossier fermé : {args:?}");
    let mut verifies = 0;
    for i in (0..args.len()).filter(|&i| args[i] == "--ro-bind" || args[i] == "--bind") {
        for (position, ferme) in &tmpfs {
            if Path::new(&args[i + 2]).starts_with(ferme) {
                assert!(position < &i, "{} se monte avant le --tmpfs {ferme} : {args:?}", args[i + 2]);
                verifies += 1;
            }
        }
    }
    // le .cargo lu, le git du bac lu puis écrit, le temporaire écrit, et ce qu'on refuse dans le bac
    assert!(verifies >= 7, "trop peu de montages sous le dossier fermé ({verifies}) : {args:?}");
}

#[test]
fn un_chemin_lu_ou_ecrit_sous_un_dossier_ferme_est_rouvert() {
    let (cas, faux) = cas_complet();
    let args = cas.args(&faux);
    let ferme = place(&args, &["--tmpfs", "/srv/maison"]).expect("le dossier fermé");
    let lu = place(&args, &["--ro-bind", "/srv/maison/.cargo", "/srv/maison/.cargo"]).expect("le chemin lu");
    let ecrit = place(&args, &["--bind", "/srv/maison/tmp", "/srv/maison/tmp"]).expect("le chemin écrit");
    assert!(ferme < lu, "le chemin lu est rouvert après le tmpfs : {args:?}");
    assert!(ferme < ecrit, "le chemin écrit est rouvert après le tmpfs : {args:?}");
}

#[test]
fn le_git_commun_sous_le_bac_est_en_lecture_seule_apres_le_bind_du_bac() {
    let cas = Cas { ecrits: chemins(&["/srv/depot"]), refuses: chemins(&["/srv/depot/.git"]), ..Cas::default() };
    let faux = Faux(vec![dossier("/srv/depot"), dossier("/srv/depot/.git")]);
    let args = cas.args(&faux);
    let ecriture = place(&args, &["--bind", "/srv/depot", "/srv/depot"]).expect("le bac écrit");
    let lecture = place(&args, &["--ro-bind", "/srv/depot/.git", "/srv/depot/.git"]).expect("le git commun refusé");
    assert!(ecriture < lecture, "le git commun reprend la lecture seule après le --bind du bac : {args:?}");
}

#[test]
fn un_chemin_lu_ou_ecrit_absent_est_saute_et_liste() {
    let cas = Cas {
        lus: chemins(&["/srv/lu-absent", "/srv/lu"]),
        ecrits: chemins(&["/srv/ecrit-absent", "/srv/ecrit"]),
        ..Cas::default()
    };
    let faux = Faux(vec![dossier("/srv/lu"), dossier("/srv/ecrit")]);
    let natif = cas.jouer(&faux).unwrap();
    let args = lignes(&natif);
    assert_eq!(natif.sautes, chemins(&["/srv/lu-absent", "/srv/ecrit-absent"]));
    assert_eq!(destinations(&args, "--ro-bind"), ["/srv/lu"]);
    assert_eq!(destinations(&args, "--bind"), ["/srv/ecrit"]);

    // la même règle, quand le chemin existe : rien n'est sauté, tout est monté
    let faux =
        Faux(vec![dossier("/srv/lu-absent"), dossier("/srv/lu"), dossier("/srv/ecrit-absent"), dossier("/srv/ecrit")]);
    let natif = cas.jouer(&faux).unwrap();
    let args = lignes(&natif);
    assert!(natif.sautes.is_empty());
    assert_eq!(destinations(&args, "--ro-bind"), ["/srv/lu-absent", "/srv/lu"]);
    assert_eq!(destinations(&args, "--bind"), ["/srv/ecrit-absent", "/srv/ecrit"]);
}

#[test]
fn un_chemin_ferme_ou_refuse_absent_est_saute_sans_bruit() {
    let cas = Cas {
        fermes: chemins(&["/srv/ferme"]),
        ecrits: chemins(&["/srv/ecrit"]),
        refuses: chemins(&["/srv/ecrit/refuse"]),
        ..Cas::default()
    };
    let absent = Faux(vec![dossier("/srv/ecrit")]);
    let natif = cas.jouer(&absent).unwrap();
    let args = lignes(&natif);
    assert!(natif.sautes.is_empty());
    assert!(!args.contains(&"--tmpfs".to_string()), "rien à fermer : {args:?}");
    assert_eq!(destinations(&args, "--ro-bind"), Vec::<String>::new());

    // les mêmes règles, les chemins présents : le dossier est fermé, le refusé est monté
    let present = Faux(vec![dossier("/srv/ferme"), dossier("/srv/ecrit"), dossier("/srv/ecrit/refuse")]);
    let args = cas.args(&present);
    assert!(place(&args, &["--tmpfs", "/srv/ferme"]).is_some(), "{args:?}");
    assert_eq!(destinations(&args, "--ro-bind"), ["/srv/ecrit/refuse"]);
}

#[test]
fn un_chemin_ferme_absent_est_rendu_dans_fermes_absents() {
    let cas = Cas { fermes: chemins(&["/srv/ferme-absent", "/srv/ferme"]), ..Cas::default() };
    let natif = cas.jouer(&Faux(vec![dossier("/srv/ferme")])).unwrap();
    assert_eq!(natif.fermes_absents, chemins(&["/srv/ferme-absent"]));
    assert!(natif.sautes.is_empty());

    let natif = cas.jouer(&Faux(vec![dossier("/srv/ferme"), dossier("/srv/ferme-absent")])).unwrap();
    assert!(natif.fermes_absents.is_empty());
}

#[test]
fn le_tilde_se_deplie_en_maison_et_ferme_le_dossier_personnel() {
    // la maison des tests est /srv/maison ; `~/` et `~` en tête se déplient, `~x` n'est pas un tilde
    let faux = Faux(vec![dossier("/srv/maison"), dossier("/srv/maison/.cargo"), dossier("/srv/maison/bac")]);
    let cas =
        Cas { fermes: chemins(&["~/"]), lus: chemins(&["~/.cargo"]), ecrits: chemins(&["~/bac"]), ..Cas::default() };
    let args = cas.args(&faux);
    assert!(place(&args, &["--tmpfs", "/srv/maison"]).is_some(), "{args:?}");
    assert!(place(&args, &["--ro-bind", "/srv/maison/.cargo", "/srv/maison/.cargo"]).is_some(), "{args:?}");
    assert!(place(&args, &["--bind", "/srv/maison/bac", "/srv/maison/bac"]).is_some(), "{args:?}");

    let cas = Cas { fermes: chemins(&["~"]), ..Cas::default() };
    assert!(place(&cas.args(&faux), &["--tmpfs", "/srv/maison"]).is_some());

    // le dossier personnel absent se dit, il ne se tait pas
    let natif = Cas { fermes: chemins(&["~/"]), ..Cas::default() }.jouer(&Faux(Vec::new())).unwrap();
    assert_eq!(natif.fermes_absents, chemins(&["/srv/maison"]));
}

#[test]
fn un_chemin_qui_n_est_pas_absolu_est_refuse() {
    let faux = Faux(vec![dossier("/srv/maison"), dossier("relatif"), dossier("~x")]);
    let base = Cas { bac: PathBuf::from("/srv/bac"), ..Cas::default() };
    let cas = [
        Cas { fermes: chemins(&["relatif"]), ..base.clone() },
        Cas { fermes: chemins(&["~x"]), ..base.clone() },
        Cas { lus: chemins(&["relatif"]), ..base.clone() },
        Cas { ecrits: chemins(&["relatif"]), ..base.clone() },
        Cas { refuses: chemins(&["relatif"]), ..base.clone() },
        Cas { bac: PathBuf::from("relatif"), toujours: vec![".gitconfig"], ..Cas::default() },
    ];
    for cas in cas {
        let refus = cas.jouer(&faux).expect_err("un chemin relatif est refusé");
        assert!(refus.contains("n'est pas absolu"), "{refus}");
    }
    // les mêmes règles en absolu passent
    let absolu = Cas { fermes: chemins(&["/srv/maison"]), bac: PathBuf::from("/srv/bac"), ..Cas::default() };
    assert!(absolu.jouer(&faux).is_ok());
}

#[test]
fn un_fichier_ferme_se_ferme_par_dev_null_et_un_dossier_par_tmpfs() {
    let cas = Cas { fermes: chemins(&["/srv/secret.env", "/srv/secrets"]), ..Cas::default() };
    let faux = Faux(vec![fichier("/srv/secret.env"), dossier("/srv/secrets")]);
    let args = cas.args(&faux);
    assert!(place(&args, &["--ro-bind", "/dev/null", "/srv/secret.env"]).is_some(), "{args:?}");
    assert!(place(&args, &["--tmpfs", "/srv/secrets"]).is_some(), "{args:?}");
    assert!(place(&args, &["--tmpfs", "/srv/secret.env"]).is_none(), "{args:?}");
    assert!(place(&args, &["--ro-bind", "/dev/null", "/srv/secrets"]).is_none(), "{args:?}");
}

#[test]
fn un_lien_se_monte_a_son_chemin_reel_une_fois() {
    let cas = Cas { fermes: chemins(&["/srv/lien", "/srv/reel"]), lus: chemins(&["/srv/lien-lu"]), ..Cas::default() };
    let faux = Faux(vec![
        lien("/srv/lien", "/srv/reel", true),
        dossier("/srv/reel"),
        lien("/srv/lien-lu", "/srv/cible", true),
    ]);
    let args = cas.args(&faux);
    let fermetures = args.iter().filter(|a| *a == "--tmpfs").count();
    assert_eq!(fermetures, 1, "le lien et sa cible se ferment une fois : {args:?}");
    assert!(place(&args, &["--tmpfs", "/srv/reel"]).is_some(), "{args:?}");
    assert!(place(&args, &["--ro-bind", "/srv/cible", "/srv/cible"]).is_some(), "{args:?}");
    assert!(!args.iter().any(|a| a == "/srv/lien" || a == "/srv/lien-lu"), "le lien ne se monte pas : {args:?}");
}

#[test]
fn chaque_chemin_toujours_refuse_l_est_s_il_existe_et_ne_l_est_pas_sinon() {
    for nom in TOUJOURS_REFUSES {
        let chemin = format!("/srv/bac/{nom}");
        let chemin = chemin.as_str();
        let cas = Cas {
            ecrits: chemins(&["/srv/bac"]),
            bac: PathBuf::from("/srv/bac"),
            toujours: TOUJOURS_REFUSES.to_vec(),
            ..Cas::default()
        };
        let presents = Faux(vec![dossier("/srv/bac"), dossier(chemin)]);
        let args = cas.args(&presents);
        let ecrit = place(&args, &["--bind", "/srv/bac", "/srv/bac"]).expect("le bac écrit");
        let refuse = place(&args, &["--ro-bind", chemin, chemin]).unwrap_or_else(|| panic!("{nom} : {args:?}"));
        assert!(ecrit < refuse, "{nom} se refuse après le --bind du bac : {args:?}");

        let absents = Faux(vec![dossier("/srv/bac")]);
        let args = cas.args(&absents);
        assert_eq!(destinations(&args, "--ro-bind"), Vec::<String>::new(), "{nom} absent n'est pas refusé");
    }
}

#[test]
fn un_ancetre_commun_de_deux_refuses_s_epingle_une_fois() {
    let cas = Cas {
        ecrits: chemins(&["/srv/bac"]),
        bac: PathBuf::from("/srv/bac"),
        toujours: vec![".claude/commands", ".claude/agents"],
        ..Cas::default()
    };
    let faux = Faux(vec![
        dossier("/srv/bac"),
        dossier("/srv/bac/.claude"),
        dossier("/srv/bac/.claude/commands"),
        dossier("/srv/bac/.claude/agents"),
    ]);
    let args = cas.args(&faux);
    assert_eq!(
        destinations(&args, "--ro-bind"),
        ["/srv/bac/.claude/commands", "/srv/bac/.claude/agents", "/srv/bac/.claude"]
    );
}

#[test]
fn les_ancetres_epingles_sont_ceux_entre_le_refuse_et_la_racine_ecrite_qui_le_contient() {
    // (les racines écrites, le refusé, les ancêtres épinglés de la racine vers lui)
    let table: [(&[&str], &str, &[&str]); 5] = [
        (&["/srv/bac"], "/srv/bac/a/b/c", &["/srv/bac/a", "/srv/bac/a/b"]),
        (&["/srv/bac"], "/srv/bac/a/b", &["/srv/bac/a"]),
        (&["/srv/bac"], "/srv/bac/.git", &[]),
        (&["/srv/bac"], "/srv/depot/.git", &[]),
        // la racine la plus profonde qui le contient : /srv/bac/a est une racine, il n'est pas épinglé
        (&["/srv/bac", "/srv/bac/a"], "/srv/bac/a/b/c", &["/srv/bac/a/b"]),
    ];
    for (racines, refuse, ancetres) in table {
        let cas = Cas { ecrits: chemins(racines), refuses: chemins(&[refuse]), ..Cas::default() };
        let tout = ["/srv/bac/a", "/srv/bac/a/b", "/srv/bac/a/b/c", "/srv/depot", "/srv/depot/.git", "/srv/bac/.git"];
        let presents: Vec<Entree> = racines.iter().chain(&tout).map(|chemin| dossier(chemin)).collect();
        let args = cas.args(&Faux(presents));
        let attendus: Vec<String> = [refuse].iter().chain(ancetres).map(|c| c.to_string()).collect();
        assert_eq!(destinations(&args, "--ro-bind"), attendus, "{refuse} sous {racines:?}");
    }
}

#[test]
fn un_montage_qui_rouvrirait_un_chemin_ferme_est_refuse() {
    let faux = Faux(vec![dossier("/srv/maison"), dossier("/srv/maison/secret"), fichier("/srv/maison/cle")]);
    let ferme = Cas { fermes: chemins(&["/srv/maison/secret"]), ..Cas::default() };
    let cas = [
        // un chemin lu, écrit ou refusé qui contient le chemin fermé, ou qui est lui
        Cas { lus: chemins(&["/srv/maison"]), ..ferme.clone() },
        Cas { ecrits: chemins(&["/srv/maison"]), ..ferme.clone() },
        Cas { refuses: chemins(&["/srv/maison"]), ..ferme.clone() },
        Cas { lus: chemins(&["/srv/maison/secret"]), ..ferme.clone() },
        // un fichier fermé, que le dossier lu qui le contient rouvrirait
        Cas { fermes: chemins(&["/srv/maison/cle"]), lus: chemins(&["/srv/maison"]), ..Cas::default() },
    ];
    for cas in cas {
        let refus = cas.jouer(&faux).expect_err("le chemin fermé se rouvrirait");
        assert!(refus.contains("rouvrirait"), "{refus}");
    }
    // un chemin lu à côté du chemin fermé, ou dessous, se monte
    let a_cote = Cas { fermes: chemins(&["/srv/maison/secret"]), lus: chemins(&["/srv/maison/cle"]), ..Cas::default() };
    assert!(a_cote.jouer(&faux).is_ok());
}

#[test]
fn un_refuse_que_la_fermeture_cache_n_est_pas_monte() {
    let faux = Faux(vec![
        dossier("/srv/maison"),
        dossier("/srv/maison/cache"),
        dossier("/srv/maison/bac"),
        dossier("/srv/maison/bac/.git"),
    ]);
    // /srv/maison est fermé : le refusé hors de tout chemin rouvert est caché, le monter l'exposerait
    let cas = Cas {
        fermes: chemins(&["/srv/maison"]),
        ecrits: chemins(&["/srv/maison/bac"]),
        refuses: chemins(&["/srv/maison/cache", "/srv/maison/bac/.git"]),
        ..Cas::default()
    };
    let args = cas.args(&faux);
    assert_eq!(destinations(&args, "--ro-bind"), ["/srv/maison/bac/.git"], "{args:?}");
}
