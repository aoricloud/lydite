use super::*;
use crate::branchements::processus::coquille;
use crate::essais::forge::{
    KRAKEN, Lieu, PR, TETE, lieu, pr_vue, rendre_ci, run, run_le, sur, sur_par, tour_de_ci, vue_pr,
};
use crate::essais::{ARBRE, LENTE, effacer, marque, meurt};
use crate::essais::{FauxGh, URL_DU_DISTANT, depot, distant_nu, git, racine};

impl Lieu {
    /// La forge de ce lieu, son distant lu comme avant une vague.
    fn forge(&self) -> Forge<'_> {
        let distant = distant(&self.kraken, &self.depot).unwrap();
        Forge { gh: &self.gh.programme, dossier: &self.kraken, depot: &self.depot, distant }
    }
}

/// Un autre que Kraken.
const AUTRE: (&str, &str) = ("autre", "autre@exemple.invalid");

#[test]
fn kraken_gh_nomme_le_programme_de_la_forge() {
    // Lecture seule, rien n'est posé : `LYDITE_GH` s'il l'est, sinon `gh`.
    assert_eq!(programme_gh(), env::var_os("LYDITE_GH").unwrap_or_else(|| "gh".into()));
}

#[test]
fn l_url_d_un_distant_se_lit_en_trois_formes() {
    let lu = |url: &str| analyser(url).map(|distant| (distant.url, distant.hote, distant.depot));
    let formes = [
        ("https://github.com/exemple/depot", "github.com", "github.com/exemple/depot"),
        ("https://github.com/exemple/depot.git", "github.com", "github.com/exemple/depot"),
        (
            "https://ghe.exemple.test/equipe-1/mon_depot.v2",
            "ghe.exemple.test",
            "ghe.exemple.test/equipe-1/mon_depot.v2",
        ),
        ("ssh://git@github.com/exemple/depot.git", "github.com", "github.com/exemple/depot"),
        ("ssh://forge.test:2222/p/d", "forge.test", "forge.test/p/d"),
        ("git@github.com:exemple/depot.git", "github.com", "github.com/exemple/depot"),
        ("forge.test:p/d", "forge.test", "forge.test/p/d"),
    ];
    for (url, hote, depot) in formes {
        assert_eq!(lu(url), Some((url.to_string(), hote.to_string(), depot.to_string())), "{url}");
    }
    // Une URL locale, un autre schéma, des identifiants, un chemin qui n'a pas deux noms, un hôte ou un utilisateur
    // qui serait une option de ssh : rien ne se lit.
    let refusees = [
        "/srv/depot.git",
        r"C:\depots\depot",
        "C:/depots/depot",
        "c:p/d",
        "file:///srv/depot.git",
        "../depot",
        r"\\serveur\depot",
        "./a:b/c",
        "http://github.com/p/d",
        "git://github.com/p/d",
        "https://jeton@github.com/p/d",
        "https://u:secret@github.com/p/d",
        "ssh://u:secret@github.com/p/d",
        "https://github.com:443/p/d",
        "ssh://github.com:x/p/d",
        "https://github.com/p",
        "https://github.com/p/d/e",
        "https://github.com/p/",
        "https://github.com//d",
        "https://github.com/p/.git",
        "https://github.com/p/d ",
        "ssh://-oProxyCommand=x/p/d",
        "-oProxyCommand=x:p/d",
        "ssh://-u@github.com/p/d",
        "github.com",
        "",
    ];
    for url in refusees {
        assert_eq!(lu(url), None, "{url}");
    }
}

#[test]
fn un_distant_illisible_est_refuse_en_8001() {
    let racine = racine("forge-8001");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    // Sans origin.
    let refus = distant(&kraken, &depot).unwrap_err();
    assert_eq!(refus.code, Code::DistantIllisible);
    assert!(refus.preuve.contains("n'a pas de distant origin"), "{refus}");
    // Un origin local, puis des identifiants, que la preuve ne recopie pas.
    git(&depot, &["remote", "add", "origin", &racine.join("ailleurs.git").to_string_lossy()]);
    let refus = distant(&kraken, &depot).unwrap_err();
    assert_eq!(refus.code, Code::DistantIllisible);
    assert!(refus.preuve.contains("ne nomme pas un hôte, un propriétaire et un dépôt"), "{refus}");
    git(&depot, &["remote", "set-url", "origin", "https://jeton-secret@forge.test/p/d"]);
    let refus = distant(&kraken, &depot).unwrap_err();
    assert_eq!(refus.code, Code::DistantIllisible);
    assert!(!refus.to_string().contains("jeton-secret"), "{refus}");
    // L'URL que le dépôt écrit, même quand `insteadOf` la renvoie au distant nu : Kraken nomme la forge.
    git(&depot, &["remote", "remove", "origin"]);
    distant_nu(&racine, &depot);
    let attendu = Distant { url: URL_DU_DISTANT.into(), hote: "forge.test".into(), depot: "forge.test/p/d".into() };
    assert_eq!(distant(&kraken, &depot), Ok(attendu));
    assert_eq!(fs::read_dir(&kraken).unwrap().count(), 0, "les sorties de git s'effacent une fois lues");
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_base_inconnue_est_refusee_en_8002() {
    let lieu = lieu("forge-8002");
    let forge = lieu.forge();
    let base = lieu.base();
    // La tête de la branche du distant ; un commit local non poussé n'y entre pas, et aucune ref ne bouge.
    let refs = || git(&lieu.depot, &["for-each-ref", "--format=%(refname) %(objectname)"]);
    let avant = refs();
    assert_eq!(forge.base_distante("main"), Ok(base.clone()));
    assert_eq!(refs(), avant);
    sur(&lieu.depot, &base, &[("local.txt", "non poussé")], "local");
    assert_eq!(forge.base_distante("main"), Ok(base.clone()));
    // Une branche absente du distant, ou un nom que git ne prend pas tel quel.
    for branche in ["absente", "a..b", "main.lock", "@{-1}", ""] {
        let refus = forge.base_distante(branche).unwrap_err();
        assert_eq!(refus.code, Code::BaseInconnue, "{branche:?} : {refus}");
    }
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn la_base_lue_ne_bouge_pas_quand_un_autre_fetch_ecrit_fetch_head_8002() {
    let lieu = lieu("forge-fetch-head");
    let forge = lieu.forge();
    let base = lieu.base();
    // Une autre branche du distant, que l'éditeur de l'humain va chercher pendant que Kraken lit sa base.
    let autre = sur(&lieu.depot, &base, &[("autre.txt", "autre\n")], "autre");
    git(&lieu.depot, &["push", "-q", "origin", &format!("{autre}:refs/heads/autre")]);
    let refs = || git(&lieu.depot, &["for-each-ref", "--format=%(refname)", "refs/lydite"]);
    let (mut fetch_head, mut pendant) = (String::new(), String::new());
    let entre = || {
        git(&lieu.depot, &["fetch", "-q", "origin", "autre"]);
        fetch_head = git(&lieu.depot, &["rev-parse", "FETCH_HEAD"]);
        pendant = refs();
    };
    assert_eq!(forge.tete_distante_avec("main", entre), Ok(base));
    // L'autre fetch a bien réécrit FETCH_HEAD ; la base lue est celle du fetch de Kraken, dans une ref à lui, que sa
    // lecture a retirée.
    assert_eq!(fetch_head, autre);
    assert!(pendant.starts_with("refs/lydite/base/") && !pendant.contains('\n'), "{pendant:?}");
    assert_eq!(refs(), "");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn gh_qui_ne_se_dit_pas_connecte_est_refuse_en_8003() {
    let lieu = lieu("forge-8003");
    let forge = lieu.forge();
    lieu.gh.rendre(1, 0, "Logged in to forge.test\n", "");
    assert_eq!(forge.connecte(), Ok(()));
    lieu.gh.rendre(2, 1, "", "You are not logged into any GitHub hosts.\n");
    let refus = forge.connecte().unwrap_err();
    assert_eq!(refus.code, Code::GhInjoignable);
    assert!(refus.preuve.contains("You are not logged into any GitHub hosts."), "{refus}");
    assert_eq!(lieu.appels_bien_partis(), 2);
    // Un gh qui ne se lance pas.
    let absent = lieu.racine.join("gh-absent");
    let forge = Forge { gh: absent.as_os_str(), ..lieu.forge() };
    let refus = forge.connecte().unwrap_err();
    assert_eq!(refus.code, Code::GhInjoignable);
    assert!(refus.preuve.starts_with("lancement impossible de "), "{refus}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_vague_restee_sur_sa_base_n_a_rien_a_pousser_8004() {
    let base = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let refus = a_pousser(base, base).unwrap_err();
    assert_eq!(refus.code, Code::RienAPousser);
    assert!(refus.preuve.contains(base), "{refus}");
    assert_eq!(a_pousser(base, TETE), Ok(()));
}

#[test]
fn ni_github_ni_chemin_du_poste_ne_se_publie_8005() {
    let lieu = lieu("forge-8005");
    let forge = lieu.forge();
    let base = lieu.base();
    // La racine du test tient lieu de dossier personnel : le dépôt et le dossier de Kraken sont sous elle.
    let chemins = [lieu.racine.as_path(), lieu.kraken.as_path(), lieu.depot.as_path()];
    let publier = |base: &str, tete: &str, titre: &str, corps: &str| {
        forge.publiable(base, tete, "lydite/vague-x", titre, corps, &chemins).map_err(|refus| {
            assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
            refus.preuve
        })
    };
    // Un fichier ordinaire et un `.github` qui n'est pas à la racine passent ; une racine du système ne compte pas.
    let propre = sur(&lieu.depot, &base, &[("a.txt", "bonjour\n"), ("sous/.github/x.yml", "x\n")], "a : écrire");
    assert_eq!(publier(&base, &propre, "Écrire a", "Écrire a\n"), Ok(()));
    let racine_du_systeme = env::current_dir().unwrap().ancestors().last().unwrap().to_path_buf();
    let avec_racine = [racine_du_systeme.as_path(), lieu.depot.as_path()];
    assert_eq!(forge.publiable(&base, &propre, "b", "titre", "corps", &avec_racine), Ok(()));
    // `.github/` à la racine, en toute casse.
    for chemin in [".github/workflows/ci.yml", ".GitHub/x.yml"] {
        let tete = sur(&lieu.depot, &base, &[(chemin, "x\n")], "ci");
        let preuve = publier(&base, &tete, "t", "c").unwrap_err();
        assert!(preuve.starts_with(&format!("{chemin} change entre la base")), "{preuve}");
    }
    // Un chemin du poste, écrit avec `/` ou avec `\`, dans une ligne ajoutée.
    let depot = lieu.depot.to_string_lossy();
    for ecrit in [depot.replace('\\', "/"), depot.replace('/', "\\")] {
        let tete = sur(&lieu.depot, &base, &[("notes.txt", &format!("vu dans {ecrit}\n"))], "notes");
        let preuve = publier(&base, &tete, "t", "c").unwrap_err();
        assert!(preuve.contains("paraît dans les lignes ajoutées de b/notes.txt"), "{preuve}");
    }
    // Sans casse sous Windows seulement.
    let tete = sur(&lieu.depot, &base, &[("notes.txt", &format!("vu dans {}\n", depot.to_uppercase()))], "notes");
    assert_eq!(publier(&base, &tete, "t", "c").is_err(), cfg!(windows));
    // Dans le message d'un commit poussé, le titre ou le corps.
    let message = format!("notes de {}", lieu.kraken.display());
    let tete = sur(&lieu.depot, &base, &[("b.txt", "b\n")], &message);
    let preuve = publier(&base, &tete, "t", "c").unwrap_err();
    assert!(preuve.contains("paraît dans les messages des commits poussés"), "{preuve}");
    let preuve = publier(&base, &propre, &format!("voir {}", lieu.racine.display()), "c").unwrap_err();
    assert!(preuve.contains("paraît dans le titre"), "{preuve}");
    let preuve = publier(&base, &propre, "t", &format!("voir {}\n", lieu.depot.display())).unwrap_err();
    assert!(preuve.contains("paraît dans le corps"), "{preuve}");
    // Un chemin que la base porte déjà, et que la vague retire, n'est pas publié par elle.
    let vieille = sur(&lieu.depot, &base, &[("vieux.txt", &format!("{depot}\n"))], "vieux");
    let tete = sur(&lieu.depot, &vieille, &[("vieux.txt", "propre\n")], "propre");
    assert_eq!(publier(&vieille, &tete, "t", "c"), Ok(()));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_depot_vise_depuis_un_sous_dossier_ne_cache_rien_a_8005() {
    let lieu = lieu("forge-relatif");
    // `diff.relative`, de l'humain ou du dépôt visé, et Kraken lancé depuis un sous-dossier du dépôt : sans
    // `--no-relative`, `git diff` ne dirait que ce qui change sous ce sous-dossier, et ses chemins en seraient
    // relatifs.
    git(&lieu.depot, &["config", "diff.relative", "true"]);
    let base = sur(&lieu.depot, &lieu.base(), &[("sous/a.txt", "a\n")], "sous");
    let sous = lieu.depot.join("sous");
    let forge = Forge { depot: &sous, ..lieu.forge() };
    let chemins = [lieu.depot.as_path()];
    let refuse = |quoi: &str, tete: &str| {
        let refus = forge.publiable(&base, tete, "b", "t", "c", &chemins).expect_err(quoi);
        assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
        refus.preuve
    };
    // Un `.github/` à la racine, hors du sous-dossier, que `--name-only` tairait.
    let github = sur(&lieu.depot, &base, &[(".github/ci.yml", "x\n"), ("sous/b.txt", "b\n")], "ci");
    let preuve = refuse("un .github/ hors du sous-dossier est publié", &github);
    assert!(preuve.starts_with(".github/ci.yml change entre la base"), "{preuve}");
    // Un chemin du poste dans une ligne ajoutée hors du sous-dossier, que `git diff -U0` tairait.
    let vu_dans = format!("vu dans {}\n", lieu.depot.display());
    let notes = sur(&lieu.depot, &base, &[("notes.txt", &vu_dans), ("sous/b.txt", "b\n")], "notes");
    let preuve = refuse("un chemin du poste hors du sous-dossier est publié", &notes);
    assert!(preuve.contains("paraît dans les lignes ajoutées de b/notes.txt"), "{preuve}");
    // Ce qui ne change que sous le sous-dossier passe toujours.
    let propre = sur(&lieu.depot, &base, &[("sous/b.txt", "b\n")], "b");
    assert_eq!(forge.publiable(&base, &propre, "b", "t", "c", &chemins), Ok(()));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_commit_qui_n_est_pas_de_kraken_ne_se_publie_pas_8005() {
    let lieu = lieu("forge-auteurs");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.racine.as_path(), lieu.kraken.as_path(), lieu.depot.as_path()];
    let publier = |base: &str, tete: &str| {
        forge.publiable(base, tete, "b", "t", "c", &chemins).map_err(|refus| {
            assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
            refus.preuve
        })
    };
    // Auteur et committer de Kraken : il passe.
    let de_kraken = sur(&lieu.depot, &base, &[("a.txt", "a\n")], "a");
    assert_eq!(publier(&base, &de_kraken), Ok(()));
    // L'auteur, le committer ou les deux d'un autre : refusé, et la preuve nomme le commit et ce qu'il porte.
    for (auteur, committer) in [(AUTRE, KRAKEN), (KRAKEN, AUTRE), (AUTRE, AUTRE)] {
        let tete = sur_par(&lieu.depot, &base, &[("b.txt", "b\n")], "b", (auteur, committer));
        let preuve = publier(&base, &tete).unwrap_err();
        let dit = |(nom, adresse): (&str, &str)| format!("« {nom} <{adresse}> »");
        let porte = format!("auteur {}, committer {}", dit(auteur), dit(committer));
        assert!(preuve.contains(&tete) && preuve.contains("n'est pas de Lydite"), "{preuve}");
        assert!(preuve.contains(&porte), "la preuve dit {porte} : {preuve}");
    }
    // Un commit étranger sous un commit de Kraken, au milieu de ce qui se publie, compte aussi.
    let etranger = sur_par(&lieu.depot, &base, &[("c.txt", "c\n")], "c", (AUTRE, AUTRE));
    let dessus = sur(&lieu.depot, &etranger, &[("d.txt", "d\n")], "d");
    let preuve = publier(&base, &dessus).unwrap_err();
    assert!(preuve.contains(&etranger), "{preuve}");
    // Ce que la base porte déjà n'est pas publié par la vague.
    assert_eq!(publier(&etranger, &dessus), Ok(()));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_mot_de_la_garde_ne_se_publie_pas_8005() {
    let lieu = lieu("forge-garde");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    let garde = lieu.kraken.join("garde");
    fs::create_dir_all(&lieu.kraken).unwrap();
    // Une ligne vide, une ligne commentée et des blancs de bord ne comptent pas : seule la ligne 4 garde.
    fs::write(&garde, "\n# zébulon\n   \n  Zébulon Dupré  \n").unwrap();
    let propre = sur(&lieu.depot, &base, &[("a.txt", "bonjour\n")], "a : écrire");
    let publier = |tete: &str, branche: &str, titre: &str, corps: &str| {
        forge.publiable(&base, tete, branche, titre, corps, &chemins).map_err(|refus| {
            assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
            refus.preuve
        })
    };
    // Le mot seul, sans la suite de mots de la ligne, ne la trouve pas : il passe.
    assert_eq!(publier(&propre, "lydite/vague-1", "Écrire a", "Écrire a zébulon\n"), Ok(()));
    let dit = |ou: &str| format!("la ligne 4 de la garde paraît dans {ou}");
    // Chaque lieu, en toute casse, et la preuve ne dit pas le mot.
    let notes = sur(&lieu.depot, &base, &[("notes.txt", "chez ZÉBULON DUPRÉ\n")], "notes");
    let message = sur(&lieu.depot, &base, &[("b.txt", "b\n")], "chez zébulon dupré");
    let refuse = [
        (publier(&notes, "lydite/vague-1", "t", "c"), "les lignes ajoutées de b/notes.txt"),
        (publier(&message, "b", "t", "c"), "les messages des commits poussés"),
        (publier(&propre, "b", "Pour zébulon Dupré", "c"), "le titre"),
        (publier(&propre, "b", "t", "voir ZÉBULON dupré\n"), "le corps"),
        (publier(&propre, "lydite/zÉbulon dupré", "t", "c"), "le nom de la branche"),
    ];
    for (preuve, ou) in refuse {
        let preuve = preuve.unwrap_err();
        assert_eq!(preuve, dit(ou));
        assert!(!preuve.to_lowercase().contains("zébulon"), "{preuve}");
    }
    // Un fichier dont le nom et une ligne portent le mot : la preuve ne nomme pas le fichier.
    let nomme = sur(&lieu.depot, &base, &[("Zébulon Dupré.md", "chez zébulon dupré\n")], "nomme");
    let preuve = publier(&nomme, "b", "t", "c").unwrap_err();
    assert_eq!(preuve, dit("le nom d'un fichier changé"));
    assert!(!preuve.to_lowercase().contains("zébulon"), "{preuve}");
    // Un fichier qui ne se lit pas est le refus 6006, qui nomme son chemin.
    fs::remove_file(&garde).unwrap();
    fs::create_dir(&garde).unwrap();
    let dossier = forge.publiable(&base, &propre, "b", "t", "c", &chemins).unwrap_err();
    fs::remove_dir(&garde).unwrap();
    fs::write(&garde, [0xff, 0xfe]).unwrap();
    let octets = forge.publiable(&base, &propre, "b", "t", "c", &chemins).unwrap_err();
    for refus in [dossier, octets] {
        assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
        assert!(refus.preuve.contains(&garde.display().to_string()), "{refus}");
    }
    // Sans fichier, la publication passe comme avant.
    fs::remove_file(&garde).unwrap();
    assert_eq!(publier(&propre, "lydite/zébulon dupré", "Pour zébulon dupré", "c"), Ok(()));
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Publie un fichier `notes.txt` qui porte `contenu` avec la garde `garde` : `Ok` s'il passe, sinon la preuve du
/// refus 8005.
fn garde_sur_un_contenu(nom: &str, garde: &str, contenu: &str) -> Result<(), String> {
    let lieu = lieu(nom);
    let forge = lieu.forge();
    let base = lieu.base();
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), garde).unwrap();
    let tete = sur(&lieu.depot, &base, &[("notes.txt", contenu)], "notes");
    let chemins = [lieu.depot.as_path()];
    let res = forge.publiable(&base, &tete, "b", "t", "c", &chemins).map_err(|refus| {
        assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
        refus.preuve
    });
    let _ = fs::remove_dir_all(&lieu.racine);
    res
}

#[test]
fn deux_mots_coupes_par_un_prefixe_de_commentaire_ne_se_publient_pas_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-1", "essai-un essai-deux\n", "// essai-un\n// essai-deux\n");
    assert_eq!(res.unwrap_err(), "la ligne 1 de la garde paraît dans les lignes ajoutées de b/notes.txt");
}

#[test]
fn deux_mots_separes_par_deux_espaces_ne_se_publient_pas_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-2", "essai-un essai-deux\n", "essai-un  essai-deux\n");
    assert_eq!(res.unwrap_err(), "la ligne 1 de la garde paraît dans les lignes ajoutées de b/notes.txt");
}

#[test]
fn la_forme_sur_une_ligne_de_la_garde_reste_refusee_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-3", "essai-un essai-deux\n", "essai-un essai-deux\n");
    assert_eq!(res.unwrap_err(), "la ligne 1 de la garde paraît dans les lignes ajoutées de b/notes.txt");
}

#[test]
fn un_seul_des_deux_mots_de_la_garde_passe_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-4", "essai-un essai-deux\n", "// essai-un\n");
    assert_eq!(res, Ok(()));
}

#[test]
fn un_mot_seul_de_la_garde_reste_compare_a_la_lettre_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-5", "essai-un\n", "essai un\n");
    assert_eq!(res, Ok(()));
}

#[test]
fn une_garde_faite_de_signes_ne_refuse_pas_une_plage_propre_8005() {
    let res = garde_sur_un_contenu("forge-garde-replie-6", "!! ??\n", "fn a() {}\n");
    assert_eq!(res, Ok(()));
}

#[test]
fn un_contenu_ajoute_puis_retire_ne_se_publie_pas_8005() {
    let lieu = lieu("forge-historique");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "zébulon\n").unwrap();
    let publier = |tete: &str| {
        forge.publiable(&base, tete, "b", "t", "c", &chemins).map_err(|refus| {
            assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
            refus.preuve
        })
    };
    // Ajouté par un commit, retiré par le suivant : le diff net ne l'a plus.
    let ajoute_puis_retire = |nom: &str, contenu: &str| {
        let ajout = sur(&lieu.depot, &base, &[(nom, contenu)], "ajout");
        fs::remove_file(lieu.depot.join(nom)).unwrap();
        sur(&lieu.depot, &ajout, &[], "retrait")
    };
    let net = lieu.depot.display().to_string();
    let tete = ajoute_puis_retire("notes.txt", "chez zébulon\n");
    let git_net = forge.git(&["diff", "--name-only", &base, &tete]);
    assert_eq!(forge.faire(git_net, None).unwrap(), "");
    let preuve = publier(&tete).unwrap_err();
    assert_eq!(
        preuve,
        "la ligne 1 de la garde paraît dans les lignes ajoutées de b/notes.txt dans l'historique poussé"
    );
    assert!(!preuve.contains("zébulon"), "{preuve}");
    let tete = ajoute_puis_retire("Zébulon.md", "propre\n");
    let preuve = publier(&tete).unwrap_err();
    assert_eq!(preuve, "la ligne 1 de la garde paraît dans le nom d'un fichier changé");
    let tete = ajoute_puis_retire("chemin.txt", &format!("vu dans {net}\n"));
    let preuve = publier(&tete).unwrap_err();
    assert!(preuve.contains("paraît dans les lignes ajoutées de b/chemin.txt dans l'historique poussé"), "{preuve}");
    assert!(!preuve.contains("zébulon"), "{preuve}");
    // Une plage propre de plusieurs commits reste publiable.
    let un = sur(&lieu.depot, &base, &[("a.txt", "un\n")], "un");
    let deux = sur(&lieu.depot, &un, &[("b.txt", "deux\n")], "deux");
    assert_eq!(publier(&deux), Ok(()));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_nom_hors_du_sous_dossier_ajoute_puis_retire_ne_fuit_pas_8005() {
    let lieu = lieu("forge-relatif-noms");
    git(&lieu.depot, &["config", "diff.relative", "true"]);
    let base = sur(&lieu.depot, &lieu.base(), &[("sous/a.txt", "a\n")], "sous");
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "zébulon\n").unwrap();
    let sous = lieu.depot.join("sous");
    let forge = Forge { depot: &sous, ..lieu.forge() };
    let chemins = [lieu.depot.as_path()];
    let vu_dans = format!("vu dans {}\n", lieu.depot.display());
    let ajout = sur(&lieu.depot, &base, &[("Zébulon.md", &vu_dans)], "ajout");
    fs::remove_file(lieu.depot.join("Zébulon.md")).unwrap();
    let tete = sur(&lieu.depot, &ajout, &[], "retrait");
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
    assert_eq!(refus.preuve, "la ligne 1 de la garde paraît dans le nom d'un fichier changé");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_mot_de_la_garde_ne_fuit_pas_par_le_nom_du_fichier_d_un_autre_8005() {
    let lieu = lieu("forge-garde-deux");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    let garde = lieu.kraken.join("garde");
    fs::create_dir_all(&lieu.kraken).unwrap();
    // La ligne 1 est dans une ligne ajoutée, la ligne 2 dans le nom de ce même fichier : chercher la ligne 1 d'abord
    // dans les lignes ajoutées nommerait le fichier, donc écrirait le mot de la ligne 2.
    fs::write(&garde, "alpha\nzébulon\n").unwrap();
    let tete = sur(&lieu.depot, &base, &[("Zébulon.md", "alpha\n")], "notes");
    let preuve = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(preuve.code, Code::PublicationRefusee, "{preuve}");
    let preuve = preuve.preuve;
    assert_eq!(preuve, "la ligne 2 de la garde paraît dans le nom d'un fichier changé");
    for mot in ["alpha", "zébulon"] {
        assert!(!preuve.to_lowercase().contains(mot), "{preuve}");
    }
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn le_refus_de_la_garde_ne_dit_pas_le_chemin_de_la_garde_8005() {
    let lieu = lieu("forge-garde-chemin");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    let garde = lieu.kraken.join("garde");
    fs::create_dir_all(&lieu.kraken).unwrap();
    // La ligne 1 est le nom du dossier du test, que le chemin de la garde porte.
    let dossier = lieu.racine.file_name().unwrap().to_string_lossy().to_string();
    fs::write(&garde, format!("{dossier}\n")).unwrap();
    assert!(garde.display().to_string().contains(&dossier));
    let propre = sur(&lieu.depot, &base, &[("a.txt", "bonjour\n")], "a");
    let refus = forge.publiable(&base, &propre, "b", &dossier, "c", &chemins).unwrap_err();
    assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
    assert_eq!(refus.preuve, "la ligne 1 de la garde paraît dans le titre");
    assert!(!refus.preuve.contains(&dossier), "{refus}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_git_qui_echoue_ne_dit_pas_le_chemin_de_la_garde_8005() {
    let lieu = lieu("forge-garde-echec");
    let forge = lieu.forge();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    // La ligne 1 est le nom du dossier du test, que le dépôt visé porte dans son chemin.
    let dossier = lieu.racine.file_name().unwrap().to_string_lossy().to_string();
    fs::write(lieu.kraken.join("garde"), format!("{dossier}\n")).unwrap();
    let inconnue = "0123456789abcdef0123456789abcdef01234567";
    let refus = forge.publiable(inconnue, inconnue, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
    assert_eq!(
        refus.preuve,
        "ce que Lydite publierait ne se lit pas : la ligne 1 de la garde paraît dans l'erreur de git"
    );
    assert!(!format!("{refus}").contains(&dossier), "{refus}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_bom_en_tete_de_la_garde_n_est_pas_un_mot_de_la_ligne_1() {
    let lieu = lieu("forge-garde-bom");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "\u{feff}zébulon\r\n").unwrap();
    let propre = sur(&lieu.depot, &base, &[("a.txt", "bonjour\n")], "a");
    assert_eq!(forge.publiable(&base, &propre, "b", "t", "c", &chemins), Ok(()));
    let refus = forge.publiable(&base, &propre, "b", "Pour Zébulon", "c", &chemins).unwrap_err();
    assert_eq!(refus.preuve, "la ligne 1 de la garde paraît dans le titre");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_ligne_commentee_de_la_garde_ne_garde_pas_son_mot_8005() {
    let lieu = lieu("forge-garde-commentaire");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "# zébulon\nalpha\n").unwrap();
    let propre = sur(&lieu.depot, &base, &[("a.txt", "bonjour\n")], "a");
    // Le corps qui porte le mot commenté passe ; celui du mot gardé est refusé, à la ligne 2.
    assert_eq!(forge.publiable(&base, &propre, "b", "t", "voir zébulon", &chemins), Ok(()));
    let refus = forge.publiable(&base, &propre, "b", "t", "voir alpha", &chemins).unwrap_err();
    assert_eq!(refus.preuve, "la ligne 2 de la garde paraît dans le corps");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn deux_mots_de_la_garde_avec_un_nom_de_fichier_ascii_ne_fuient_pas_8005() {
    let lieu = lieu("forge-garde-ascii");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    // La ligne 1 est dans une ligne ajoutée, la ligne 2 dans le nom de ce même fichier, en ASCII.
    fs::write(lieu.kraken.join("garde"), "alpha\nBeta\n").unwrap();
    let tete = sur(&lieu.depot, &base, &[("beta.md", "alpha\n")], "notes");
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.preuve, "la ligne 2 de la garde paraît dans le nom d'un fichier changé");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_fichier_de_github_dont_le_nom_porte_un_mot_de_la_garde_est_refuse_sans_son_nom_8005() {
    let lieu = lieu("forge-garde-github");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "zébulon\n").unwrap();
    let tete = sur(&lieu.depot, &base, &[(".github/Zébulon.yml", "a\n")], "ci");
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.preuve, "la ligne 1 de la garde paraît dans le nom d'un fichier changé");
    // Sans le mot gardé dans son nom, le refus de `.github/` reste, avec son nom.
    fs::write(lieu.kraken.join("garde"), "autre\n").unwrap();
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert!(refus.preuve.starts_with(".github/Zébulon.yml change entre la base"), "{refus}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_commit_etranger_dont_l_auteur_porte_un_mot_de_la_garde_ne_cite_pas_l_identite_8005() {
    let lieu = lieu("forge-garde-auteur");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "zébulon\n").unwrap();
    let auteur = ("Zébulon Dupré", "z@exemple.invalid");
    let tete = sur_par(&lieu.depot, &base, &[("b.txt", "b\n")], "b", (auteur, KRAKEN));
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.code, Code::PublicationRefusee, "{refus}");
    assert!(refus.preuve.contains(&tete) && refus.preuve.contains("n'est pas de Lydite"), "{refus}");
    assert!(!refus.preuve.to_lowercase().contains("zébulon") && !refus.preuve.contains("auteur"), "{refus}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn les_diffs_de_la_publication_ignorent_la_configuration_des_prefixes_de_l_humain() {
    let lieu = lieu("forge-garde-prefixes");
    let forge = lieu.forge();
    let base = lieu.base();
    let chemins = [lieu.depot.as_path()];
    fs::create_dir_all(&lieu.kraken).unwrap();
    fs::write(lieu.kraken.join("garde"), "zébulon\n").unwrap();
    git(&lieu.depot, &["config", "diff.noprefix", "true"]);
    let tete = sur(&lieu.depot, &base, &[("notes.txt", "chez zébulon\n")], "notes");
    let refus = forge.publiable(&base, &tete, "b", "t", "c", &chemins).unwrap_err();
    assert_eq!(refus.preuve, "la ligne 1 de la garde paraît dans les lignes ajoutées de b/notes.txt");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn pousser_envoie_un_sha_sans_forcer_8006() {
    let lieu = lieu("forge-8006");
    let forge = lieu.forge();
    let base = lieu.base();
    let tete = sur(&lieu.depot, &base, &[("a.txt", "a\n")], "a");
    // La branche locale du même nom porte autre chose : c'est le sha qui part, jamais un nom.
    let autre = sur(&lieu.depot, &base, &[("b.txt", "b\n")], "b");
    git(&lieu.depot, &["branch", "lydite/vague-v", &autre]);
    assert_eq!(forge.pousser(&tete, "lydite/vague-v"), Ok(()));
    assert_eq!(lieu.distante("lydite/vague-v"), tete);
    // Un nom n'est pas un sha : rien ne part.
    let refus = forge.pousser("lydite/vague-v", "lydite/vague-w").unwrap_err();
    assert_eq!(refus.code, Code::PousseeRefusee);
    assert_eq!(git(&lieu.nu, &["branch", "--list", "lydite/vague-w"]), "");
    // La tête d'une relance, qui descend de la précédente, avance la branche.
    let suite = sur(&lieu.depot, &tete, &[("c.txt", "c\n")], "c");
    assert_eq!(forge.pousser(&suite, "lydite/vague-v"), Ok(()));
    assert_eq!(lieu.distante("lydite/vague-v"), suite);
    // Un commit étranger posé sur la branche distante : le push est refusé, et le distant n'a pas bougé.
    let etranger = sur(&lieu.depot, &base, &[("d.txt", "d\n")], "étranger");
    git(&lieu.depot, &["push", "-q", "origin", &format!("{etranger}:refs/heads/lydite/vague-x")]);
    let refus = forge.pousser(&tete, "lydite/vague-x").unwrap_err();
    assert_eq!(refus.code, Code::PousseeRefusee);
    assert!(refus.preuve.contains(" push "), "{refus}");
    assert_eq!(lieu.distante("lydite/vague-x"), etranger);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn ouvrir_cree_la_pr_et_la_relit_sur_le_sha_pousse_8007() {
    let lieu = lieu("forge-8007");
    let forge = lieu.forge();
    let url = "https://forge.test/p/d/pull/12";
    let vue = |oid: &str, numero: &str| format!(r#"{{"headRefOid":"{oid}","number":{numero},"url":"{url}"}}"#);
    let corps = "Écrire bonjour.txt\n\nPreuve : cargo test → vert\n\na : Écrire bonjour.txt\n\nLa fusion reste un \
                 geste humain.\n";
    let ouvrir = || forge.ouvrir("main", "lydite/vague-v", "Ecrire bonjour", corps, TETE);
    lieu.gh.rendre(1, 0, &format!("{url}\n"), "");
    lieu.gh.rendre(2, 0, &vue(TETE, "12"), "");
    assert_eq!(ouvrir(), Ok(PrOuverte { numero: 12, url: url.into() }));
    assert_eq!(lieu.gh.entree(1), corps, "le corps part sur l'entrée de gh pr create");
    assert_eq!(
        lieu.gh.appels(),
        [
            "pr create --base main --head lydite/vague-v --title Ecrire bonjour --body-file - --repo forge.test/p/d",
            "pr view lydite/vague-v --json number,url,headRefOid --repo forge.test/p/d",
        ]
    );
    // `gh pr create` refusé, et la branche n'a pas de PR à relire (appels 3 et 4) ; la PR relue sur un autre sha, sans
    // numéro positif, ou illisible : la branche reste poussée, sans PR.
    lieu.gh.rendre(3, 1, "", "pull request create failed: HTTP 502\n");
    lieu.gh.rendre(4, 1, "", "no pull requests found for branch lydite/vague-v\n");
    let refus = ouvrir().unwrap_err();
    assert_eq!(refus.code, Code::PrImpossible);
    assert!(refus.preuve.contains("HTTP 502") && refus.preuve.contains("no pull requests found"), "{refus}");
    assert!(refus.correction.contains("lydite/vague-v reste poussée"), "{refus}");
    for (n, relue, vu) in [
        (5, vue("9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34", "12"), "pas le sha poussé"),
        (7, vue(TETE, "0"), "pas un numéro positif"),
        (9, "pas du json".into(), "pas la PR en un objet JSON"),
    ] {
        lieu.gh.rendre(n, 0, "", "");
        lieu.gh.rendre(n + 1, 0, &relue, "");
        let refus = ouvrir().unwrap_err();
        assert_eq!(refus.code, Code::PrImpossible);
        assert!(refus.preuve.contains(vu), "{refus}");
    }
    assert_eq!(lieu.appels_bien_partis(), 10);
    let restes: Vec<_> = fs::read_dir(&lieu.kraken).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(restes.iter().all(|nom| nom.to_string_lossy().starts_with("gh-")), "rien d'autre ne reste : {restes:?}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Ce que `gh issue view --json number,state,title,body` rend.
fn vue_issue(numero: u64, etat: &str, titre: &str, corps: &str) -> String {
    let (titre, corps) = (json::chaine(titre), json::chaine(corps));
    let url = format!("https://forge.test/p/d/issues/{numero}");
    format!(r#"{{"body":{corps},"number":{numero},"state":"{etat}","title":{titre},"url":"{url}"}}"#)
}

#[test]
fn une_issue_ouverte_se_lit_et_les_autres_sont_refusees_8012_8013() {
    let lieu = lieu("forge-issue");
    let lire = || lire_issue(&lieu.gh.programme, &lieu.kraken, &lieu.depot, 12);
    // Ouverte : lue, par `gh issue view` qui nomme le dépôt avec son hôte, parti du dossier de Kraken.
    lieu.gh.rendre(1, 0, &vue_issue(12, "OPEN", "Écrire a", "Le corps\n\nde l'issue"), "");
    let attendue = Issue { numero: 12, titre: "Écrire a".into(), corps: "Le corps\n\nde l'issue".into() };
    assert_eq!(lire(), Ok(attendue));
    assert_eq!(lieu.gh.appels(), ["issue view 12 --json number,state,title,body,url --repo forge.test/p/d"]);
    assert!(lieu.kraken.join("gh-1.ici").is_file() && !lieu.depot.join("gh-1.ici").exists());
    // Fermée, gh en échec, un JSON faux, un autre numéro, une PR : 8012.
    lieu.gh.rendre(2, 0, &vue_issue(12, "CLOSED", "t", "c"), "");
    lieu.gh.rendre(3, 1, "", "GraphQL: Could not resolve to an issue with the number of 12.\n");
    lieu.gh.rendre(4, 0, "pas du json", "");
    lieu.gh.rendre(5, 0, &vue_issue(13, "OPEN", "t", "c"), "");
    let pr = vue_issue(12, "OPEN", "t", "c").replace("/issues/12", "/pull/12");
    lieu.gh.rendre(6, 0, &pr, "");
    let refus = [
        "son état est CLOSED",
        "Could not resolve",
        "pas l'issue 12 en un objet JSON (11 octets)",
        "pas l'issue 12",
        "celui d'une PR, pas d'une issue",
    ];
    for vu in refus {
        let refus = lire().unwrap_err();
        assert_eq!(refus.code, Code::IssueIllisible, "{refus}");
        assert!(refus.preuve.contains(vu), "{vu} : {refus}");
    }
    // Un JSON faux ou un champ qui manque ne recopie rien de la sortie de gh, qui porte le titre et le corps : le
    // refus nomme ce qui manque, ou la taille lue. gh en échec ne dit que ses erreurs, jamais sa sortie.
    let secret = "Titre secret";
    let sans_url = format!(r#"{{"body":"{secret}","number":12,"state":"OPEN","title":"{secret}"}}"#);
    let faux = format!("{secret} {secret}");
    let echoue = vue_issue(12, "OPEN", secret, secret);
    for (k, (code, sortie, vu)) in
        [(0, sans_url.as_str(), "ces champs en leur type : url"), (0, &faux, "en un objet JSON"), (1, &echoue, "")]
            .into_iter()
            .enumerate()
    {
        lieu.gh.rendre(7 + k, code, sortie, "");
        let refus = lire().unwrap_err();
        assert_eq!(refus.code, Code::IssueIllisible, "{refus}");
        assert!(refus.preuve.contains(vu), "{vu} : {refus}");
        assert!(!refus.to_string().contains(secret), "{refus}");
    }
    lieu.gh.rendre(10, 0, r#"{"number":"12","state":"OPEN","title":1,"url":"u"}"#, "");
    let refus = lire().unwrap_err();
    assert!(refus.preuve.ends_with("ces champs en leur type : number, title, body"), "{refus}");
    // 65 536 octets d'UTF-8 au titre et au corps ensemble sont pris (é en vaut deux), 65 537 refusés.
    let corps = "a".repeat(POIDS_D_UNE_ISSUE - 2);
    lieu.gh.rendre(11, 0, &vue_issue(12, "OPEN", "é", &corps), "");
    assert_eq!(lire().map(|issue| issue.corps.len() + issue.titre.len()), Ok(POIDS_D_UNE_ISSUE));
    lieu.gh.rendre(12, 0, &vue_issue(12, "OPEN", "é", &format!("{corps}a")), "");
    let refus = lire().unwrap_err();
    assert_eq!(refus.code, Code::IssueTropLongue, "{refus}");
    assert!(refus.preuve.contains("65537 octets"), "{refus}");
    // Une sortie plus longue que la borne n'est pas lue en entier : 8013.
    let enorme = vue_issue(12, "OPEN", "t", &"a".repeat(SORTIE_D_UNE_ISSUE as usize));
    assert!(enorme.len() as u64 > SORTIE_D_UNE_ISSUE);
    lieu.gh.rendre(13, 0, &enorme, "");
    let refus = lire().unwrap_err();
    assert_eq!(refus.code, Code::IssueTropLongue, "{refus}");
    assert!(refus.preuve.contains(&format!("en plus de {SORTIE_D_UNE_ISSUE} octets")), "{refus}");
    // De l'UTF-8 invalide : 8013.
    let mut invalide = br#"{"body":"a"#.to_vec();
    invalide.extend([0xff, 0xfe]);
    invalide.extend(br#"","number":12,"state":"OPEN","title":"t"}"#);
    fs::write(lieu.racine.join("gh").join("sortie-14.txt"), invalide).unwrap();
    let refus = lire().unwrap_err();
    assert_eq!(refus.code, Code::IssueTropLongue, "{refus}");
    assert!(refus.preuve.contains("UTF-8"), "{refus}");
    let restes: Vec<_> = fs::read_dir(&lieu.kraken).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(restes.iter().all(|nom| nom.to_string_lossy().starts_with("gh-")), "rien d'autre ne reste : {restes:?}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_create_rate_dont_la_pr_existe_sur_la_tete_pousse_est_reconcilie_8007() {
    let lieu = lieu("forge-reconcilier");
    let forge = lieu.forge();
    let ouvrir = || forge.ouvrir("main", "lydite/vague-v", "Ecrire a", "corps\n", TETE);
    // `gh pr create` sort en erreur, ou meurt à son délai, alors que la PR s'est ouverte : la relire sur la branche
    // (appel 2) la donne sur la tête poussée, et le create ne se rejoue pas.
    lieu.gh.rendre(1, 1, "", "gh: connexion perdue\n");
    lieu.gh.rendre(2, 0, &pr_vue(TETE), "");
    assert_eq!(ouvrir(), Ok(PrOuverte { numero: 12, url: PR.into() }));
    let create = "pr create --base main --head lydite/vague-v --title Ecrire a --body-file - --repo forge.test/p/d";
    let view = "pr view lydite/vague-v --json number,url,headRefOid --repo forge.test/p/d";
    assert_eq!(lieu.gh.appels(), [create, view]);
    // Une PR de la branche qui porte un autre sha n'est pas celle du sha poussé : le refus 8007 dit le create raté et
    // la relecture.
    let autre = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    lieu.gh.rendre(3, 1, "", "gh: connexion perdue\n");
    lieu.gh.rendre(4, 0, &pr_vue(autre), "");
    let refus = ouvrir().unwrap_err();
    assert_eq!(refus.code, Code::PrImpossible, "{refus}");
    assert!(refus.preuve.contains("connexion perdue") && refus.preuve.contains("pas le sha poussé"), "{refus}");
    assert_eq!(lieu.gh.appels().len(), 4, "un create et un view par essai");
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Un statut du commit, tel que l'api le rend dans `statuses`, en minuscules.
fn statut(contexte: &str, etat: &str) -> String {
    format!(r#"{{"context":"{contexte}","created_at":"2026-11-02T10:00:00Z","state":"{etat}","target_url":null}}"#)
}

#[test]
fn les_runs_et_les_statuts_se_classent_un_par_un() {
    let classe = |checks: &[String]| {
        let Ok(Json::Liste(checks)) = json::document(&format!("[{}]", checks.join(","))) else { panic!() };
        classer(&checks)
    };
    let (vert, rouge) = (Some(Couleur::Vert), Some(Couleur::Rouge));
    let cas = [
        // Vert : au moins un réussi, aucun en cours ni en échec ; un run neutre n'y change rien. gh écrit en
        // minuscules, Kraken lit et dit en majuscules.
        (vec![run("test (linux)", "completed", "success")], vert, "test (linux) : SUCCESS"),
        (
            vec![run("test", "completed", "success"), run("doc", "completed", "skipped")],
            vert,
            "test : SUCCESS\ndoc : SKIPPED",
        ),
        // Rouge : un échec, une fois plus rien en cours.
        (
            vec![run("test", "completed", "failure"), run("doc", "completed", "success")],
            rouge,
            "test : FAILURE\ndoc : SUCCESS",
        ),
        (vec![run("test", "completed", "timed_out")], rouge, "test : TIMED_OUT"),
        // En cours : pas de verdict, même à côté d'un échec.
        (
            vec![run("test", "completed", "failure"), run("doc", "in_progress", "")],
            None,
            "test : FAILURE\ndoc : IN_PROGRESS",
        ),
        (vec![run("test", "queued", "")], None, "test : QUEUED"),
        // Vide, ou rien qui ait réussi : pas de verdict, jamais vert.
        (vec![], None, ""),
        (vec![run("doc", "completed", "neutral")], None, "doc : NEUTRAL"),
        // Un run et un statut du commit.
        (
            vec![run("test", "completed", "success"), statut("ci/externe", "success")],
            vert,
            "test : SUCCESS\nci/externe : SUCCESS",
        ),
        (
            vec![run("test", "completed", "success"), statut("ci/externe", "error")],
            rouge,
            "test : SUCCESS\nci/externe : ERROR",
        ),
        (vec![statut("ci/externe", "failure")], rouge, "ci/externe : FAILURE"),
        (
            vec![run("test", "completed", "success"), statut("ci/externe", "pending")],
            None,
            "test : SUCCESS\nci/externe : PENDING",
        ),
    ];
    for (checks, couleur, texte) in cas {
        assert_eq!(classe(&checks), (couleur, texte.to_string()), "{checks:?}");
    }
}

/// Quatre instants qui se suivent, tels que `gh` les écrit.
const T: [&str; 4] = ["2026-11-02T10:00:00Z", "2026-11-02T10:05:00Z", "2026-11-02T10:10:00Z", "2026-11-02T10:15:00Z"];

#[test]
fn deux_checks_de_meme_nom_se_dedoublonnent_et_le_plus_recent_compte() {
    let classe = |checks: &[String]| {
        let Ok(Json::Liste(checks)) = json::document(&format!("[{}]", checks.join(","))) else { panic!() };
        classer(&checks)
    };
    let (vert, rouge) = (Some(Couleur::Vert), Some(Couleur::Rouge));
    let [t1, t2, t3, t4] = T;
    let nul = "0001-01-01T00:00:00Z";
    let (echec, succes, en_cours) = (("completed", "failure"), ("completed", "success"), ("in_progress", ""));
    let test = |issue, instants| run_le("ci", "test", issue, instants);
    let doc = |issue, instants| run_le("doc", "doc", issue, instants);
    let cas = [
        // Un workflow relancé vert après un échec : le run d'avant ne compte plus, qu'il soit listé avant ou après.
        (vec![test(echec, (t1, t2)), test(succes, (t3, t4))], vert, "test : SUCCESS"),
        (vec![test(succes, (t3, t4)), test(echec, (t1, t2))], vert, "test : SUCCESS"),
        // Un run plus récent qui échoue l'emporte sur un succès d'avant.
        (vec![test(succes, (t1, t2)), test(echec, (t3, t4))], rouge, "test : FAILURE"),
        // Relancé et pas commencé, sans `startedAt` ou à l'instant nul de gh : `createdAt` départage, pas de verdict.
        (vec![test(echec, (t1, t2)), test(en_cours, (t3, ""))], None, "test : IN_PROGRESS"),
        (vec![test(echec, (t1, t2)), test(en_cours, (t3, nul))], None, "test : IN_PROGRESS"),
        // Sans `startedAt`, `createdAt` départage ; sans aucun instant, le rang dans la liste.
        (vec![test(echec, (t2, "")), test(succes, (t3, ""))], vert, "test : SUCCESS"),
        (vec![test(succes, (t3, "")), test(echec, (t2, ""))], vert, "test : SUCCESS"),
        (vec![test(echec, ("", "")), test(succes, ("", ""))], vert, "test : SUCCESS"),
        (vec![test(succes, ("", "")), test(echec, ("", ""))], rouge, "test : FAILURE"),
        // Deux workflows restent deux ; la liste garde son ordre.
        (vec![test(echec, (t1, t2)), doc(succes, (t1, t2))], rouge, "test : FAILURE\ndoc : SUCCESS"),
        (
            vec![test(echec, (t1, t2)), doc(succes, (t1, t2)), test(succes, (t3, t4))],
            vert,
            "doc : SUCCESS\ntest : SUCCESS",
        ),
        // Le même nom dans un autre workflow est un autre run : son succès ne cache pas l'échec de l'un.
        (
            vec![test(echec, (t1, t2)), run_le("deploy", "test", succes, (t3, t4))],
            rouge,
            "test : FAILURE\ntest : SUCCESS",
        ),
        // Un run qu'on ne sait pas désigner (un champ que gh aurait renommé) ne remplace aucun autre et n'est remplacé
        // par aucun : deux runs sans workflow restent deux, et l'échec de l'un ne disparaît pas.
        (
            vec![run_le("", "test", echec, (t1, t2)), run_le("", "test", succes, (t3, t4))],
            rouge,
            "test : FAILURE\ntest : SUCCESS",
        ),
        // Un statut se dédouble par son contexte, le dernier de la liste comptant, mais ne se confond pas avec un run
        // de même nom.
        (vec![statut("ci/externe", "failure"), statut("ci/externe", "success")], vert, "ci/externe : SUCCESS"),
        (
            vec![run("ci/externe", "completed", "failure"), statut("ci/externe", "success")],
            rouge,
            "ci/externe : FAILURE\nci/externe : SUCCESS",
        ),
    ];
    for (checks, couleur, texte) in cas {
        assert_eq!(classe(&checks), (couleur, texte.to_string()), "{checks:?}");
    }
}

#[test]
fn un_commit_de_kraken_se_reconnait_a_son_auteur_et_a_son_committer() {
    let kraken = "lydite <lydite@invalid>";
    let sha = "c3a91e07d5f2486b0a7e3d915c28b4f60e1d7a39";
    let ligne = |auteur: &str, committer: &str| format!("{sha}\0{auteur}\0{committer}\n");
    assert_eq!(commit_etranger(""), None);
    assert_eq!(commit_etranger(&format!("{}{}", ligne(kraken, kraken), ligne(kraken, kraken))), None);
    // Le premier qui n'est pas de Kraken, dit en clair.
    let log = format!("{}{}{}", ligne(kraken, kraken), ligne("autre <a@b>", kraken), ligne(kraken, "autre <a@b>"));
    let dit = (sha.to_string(), "autre <a@b>".to_string(), kraken.to_string());
    assert_eq!(commit_etranger(&log), Some(dit));
    // Une ligne qui n'a pas la forme attendue n'est pas de Kraken : le doute ne laisse rien passer.
    let trop = format!("{sha}\0{kraken}\0{kraken}\0{kraken}\n");
    for malformee in [format!("{sha}\n"), format!("{sha}\0{kraken}\n"), trop, "n'importe quoi\n".to_string()] {
        assert!(commit_etranger(&malformee).is_some(), "{malformee:?}");
    }
}

#[test]
fn un_job_relance_vert_apres_un_echec_donne_une_ci_verte() {
    let lieu = lieu("forge-ci-job-relance");
    let forge = lieu.forge();
    let [t1, t2, t3, t4] = T;
    let (echec, succes) = (("completed", "failure"), ("completed", "success"));
    let runs = [
        run_le("windows", "test (windows)", echec, (t1, t2)),
        run_le("linux", "test (linux)", succes, (t1, t2)),
        run_le("windows", "test (windows)", succes, (t3, t4)),
    ];
    rendre_ci(&lieu.gh, 1, TETE, &runs, &[]);
    let ci = forge.lire_ci(12, TETE, None, Duration::from_millis(10), Duration::from_secs(60));
    assert_eq!(ci, Ok(Ci { couleur: Couleur::Vert, texte: "test (linux) : SUCCESS\ntest (windows) : SUCCESS".into() }));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn cent_checks_ne_donnent_jamais_de_vert_8008() {
    let lieu = lieu("forge-cent-checks");
    let forge = lieu.forge();
    let lire = || forge.lire_ci(12, TETE, None, Duration::from_millis(10), Duration::ZERO);
    // `n` runs réussis, le dernier en échec si `echec`, et `n` statuts réussis.
    let runs = |n: usize, echec: bool| {
        let issue = |k| if echec && k == n { "failure" } else { "success" };
        (1..=n).map(|k| run(&format!("job-{k}"), "completed", issue(k))).collect::<Vec<_>>()
    };
    let statuts = |n: usize| (1..=n).map(|k| statut(&format!("ci/{k}"), "success")).collect::<Vec<_>>();
    // gh ne rend que les 100 premiers runs ou statuts demandés : 99 de chaque réussis sont un vert, 100 ou plus de
    // l'un ou de l'autre ne le prouvent pas.
    rendre_ci(&lieu.gh, 1, TETE, &runs(99, false), &statuts(99));
    assert!(matches!(lire(), Ok(Ci { couleur: Couleur::Vert, .. })));
    for (n_runs, n_statuts, appel) in [(100, 0, 4), (101, 0, 7), (1, 100, 10)] {
        rendre_ci(&lieu.gh, appel, TETE, &runs(n_runs, false), &statuts(n_statuts));
        let refus = lire().unwrap_err();
        assert_eq!(refus.code, Code::CiMuette, "{n_runs} runs, {n_statuts} statuts : {refus}");
        let dit = format!("la PR a {n_runs} runs et {n_statuts} statuts");
        assert!(refus.preuve.contains(&dit), "la raison est dite : {refus}");
    }
    // Un échec parmi ceux qu'elle rend reste un échec, une fois plus rien en cours.
    rendre_ci(&lieu.gh, 13, TETE, &runs(100, true), &[]);
    assert!(matches!(lire(), Ok(Ci { couleur: Couleur::Rouge, .. })));
    assert_eq!(lieu.appels_bien_partis(), 15);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn la_ci_se_lit_jusqu_a_son_verdict_ou_son_refus_8008_8009_8010() {
    let lieu = lieu("forge-ci");
    let forge = lieu.forge();
    let lire = |delai: Duration| forge.lire_ci(12, TETE, None, Duration::from_millis(10), delai);
    // Vert d'emblée : la PR, ses runs, puis les statuts de son commit.
    rendre_ci(&lieu.gh, 1, TETE, &[run("test (linux)", "completed", "success")], &[]);
    let ci = Ci { couleur: Couleur::Vert, texte: "test (linux) : SUCCESS".into() };
    assert_eq!(lire(Duration::from_secs(60)), Ok(ci));
    assert_eq!(lieu.gh.appels(), tour_de_ci(TETE));
    // En cours, puis rouge au tour suivant.
    rendre_ci(&lieu.gh, 4, TETE, &[run("test", "in_progress", "")], &[]);
    rendre_ci(&lieu.gh, 7, TETE, &[run("test", "completed", "failure")], &[]);
    let ci = Ci { couleur: Couleur::Rouge, texte: "test : FAILURE".into() };
    assert_eq!(lire(Duration::from_secs(60)), Ok(ci));
    // Un statut du commit en échec rend la CI rouge, à côté d'un run qui passe.
    rendre_ci(&lieu.gh, 10, TETE, &[run("test", "completed", "success")], &[statut("ci/externe", "failure")]);
    let ci = Ci { couleur: Couleur::Rouge, texte: "test : SUCCESS\nci/externe : FAILURE".into() };
    assert_eq!(lire(Duration::from_secs(60)), Ok(ci));
    // Ni run ni statut, jusqu'au délai : 8008. De même pour gh qui ne rend pas la PR, ses runs ou ses statuts.
    rendre_ci(&lieu.gh, 13, TETE, &[], &[]);
    let refus = lire(Duration::ZERO).unwrap_err();
    assert_eq!(refus.code, Code::CiMuette, "{refus}");
    assert!(refus.preuve.contains("aucun check"), "{refus}");
    let ouverte = vue_pr(TETE, "OPEN", "MERGEABLE");
    lieu.gh.rendre(16, 1, "", "boum");
    lieu.gh.rendre(17, 0, &ouverte, "");
    lieu.gh.rendre(18, 1, "", "HTTP 403: Resource not accessible by personal access token");
    lieu.gh.rendre(19, 0, &ouverte, "");
    lieu.gh.rendre(20, 0, "[]", "");
    lieu.gh.rendre(21, 0, r#"{"message":"Not Found"}"#, "");
    for vu in ["a échoué", "Resource not accessible", "gh ne rend pas une liste"] {
        let refus = lire(Duration::ZERO).unwrap_err();
        assert_eq!(refus.code, Code::CiMuette, "{refus}");
        assert!(refus.preuve.contains(vu), "{refus}");
    }
    // La PR fermée, fusionnée, ou poussée par un autre : 8010 ; en conflit : 8009. Kraken s'arrête au premier tour,
    // sans lire sa CI.
    let autre = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    for (n, vue, code) in [
        (22, vue_pr(TETE, "CLOSED", "MERGEABLE"), Code::PrChangee),
        (23, vue_pr(autre, "MERGED", "UNKNOWN"), Code::PrChangee),
        (24, vue_pr(autre, "OPEN", "MERGEABLE"), Code::PrChangee),
        (25, vue_pr(TETE, "OPEN", "CONFLICTING"), Code::AReintegrer),
    ] {
        lieu.gh.rendre(n, 0, &vue, "");
        let refus = lire(Duration::from_secs(60)).unwrap_err();
        assert_eq!(refus.code, code, "{vue}");
        if n == 23 {
            assert!(refus.preuve.contains(&format!("est MERGED et porte {autre}")), "{refus}");
        }
    }
    assert_eq!(lieu.appels_bien_partis(), 25);
    let [pr, runs, statuts] = tour_de_ci(TETE);
    let appels = lieu.gh.appels();
    assert!(appels[21..].iter().all(|appel| *appel == pr), "{appels:?}");
    assert_eq!(appels[15..21], [pr.clone(), pr.clone(), runs.clone(), pr, runs, statuts]);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_pr_fusionnee_sur_la_tete_poussee_se_lit_comme_une_ouverte() {
    let lieu = lieu("forge-fusionnee");
    let forge = lieu.forge();
    let autre = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let lire = |avant, delai| forge.lire_ci(12, TETE, avant, Duration::from_millis(10), delai);
    let fusionnee = vue_pr(TETE, "MERGED", "UNKNOWN");
    // Vert.
    let rendre = |n, runs: &[String]| {
        rendre_ci(&lieu.gh, n, TETE, runs, &[]);
        lieu.gh.rendre(n, 0, &fusionnee, "");
    };
    rendre(1, &[run("test", "completed", "success")]);
    assert_eq!(lire(None, Duration::from_secs(60)), Ok(Ci { couleur: Couleur::Vert, texte: "test : SUCCESS".into() }));
    // Rouge : 8010, qui dit fusionnée et rouge.
    rendre(4, &[run("test", "completed", "failure")]);
    let refus = lire(None, Duration::from_secs(60)).unwrap_err();
    assert_eq!(refus.code, Code::PrChangee, "{refus}");
    assert!(refus.preuve.contains("fusionnée") && refus.preuve.contains("rouge"), "{refus}");
    // En cours : le tour suivant, avec un délai non nul ; sans verdict dans le délai, 8008.
    rendre(7, &[run("test", "in_progress", "")]);
    rendre(10, &[run("test", "completed", "success")]);
    assert_eq!(lire(None, Duration::from_secs(60)), Ok(Ci { couleur: Couleur::Vert, texte: "test : SUCCESS".into() }));
    rendre(13, &[run("test", "in_progress", "")]);
    let refus = lire(None, Duration::ZERO).unwrap_err();
    assert_eq!(refus.code, Code::CiMuette, "{refus}");
    // Fermée, ou fusionnée sur une autre tête que la poussée (`avant` compris) : 8010, tout de suite.
    lieu.gh.rendre(16, 0, &vue_pr(TETE, "CLOSED", "MERGEABLE"), "");
    lieu.gh.rendre(17, 0, &vue_pr(autre, "MERGED", "UNKNOWN"), "");
    lieu.gh.rendre(18, 0, &vue_pr(autre, "MERGED", "UNKNOWN"), "");
    for avant in [None, None, Some(autre)] {
        assert_eq!(lire(avant, Duration::from_secs(60)).unwrap_err().code, Code::PrChangee);
    }
    assert_eq!(lieu.appels_bien_partis(), 18);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_pr_fusionnee_en_conflit_sur_la_tete_poussee_a_sa_ci_verte() {
    let lieu = lieu("forge-fusionnee-conflit");
    let forge = lieu.forge();
    rendre_ci(&lieu.gh, 1, TETE, &[run("test", "completed", "success")], &[]);
    lieu.gh.rendre(1, 0, &vue_pr(TETE, "MERGED", "CONFLICTING"), "");
    let ci = forge.lire_ci(12, TETE, None, Duration::from_millis(10), Duration::from_secs(60));
    assert_eq!(ci, Ok(Ci { couleur: Couleur::Vert, texte: "test : SUCCESS".into() }));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_pr_ouverte_puis_fusionnee_sur_la_tete_poussee_rend_sa_ci_verte() {
    let lieu = lieu("forge-ouverte-puis-fusionnee");
    let forge = lieu.forge();
    // Premier tour : ouverte, CI en cours. Second tour : fusionnée sur la tête poussée, CI verte.
    rendre_ci(&lieu.gh, 1, TETE, &[run("test", "in_progress", "")], &[]);
    rendre_ci(&lieu.gh, 4, TETE, &[run("test", "completed", "success")], &[]);
    lieu.gh.rendre(4, 0, &vue_pr(TETE, "MERGED", "UNKNOWN"), "");
    let ci = forge.lire_ci(12, TETE, None, Duration::from_millis(10), Duration::from_secs(60));
    assert_eq!(ci, Ok(Ci { couleur: Couleur::Vert, texte: "test : SUCCESS".into() }));
    assert_eq!(lieu.appels_bien_partis(), 6);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn le_journal_d_une_ci_rouge_est_la_fin_de_ses_runs_en_echec() {
    let lieu = lieu("forge-journal");
    let forge = lieu.forge();
    let runs = r#"[{"conclusion":"success","databaseId":7},{"conclusion":"failure","databaseId":8}]"#;
    lieu.gh.rendre(1, 0, runs, "");
    let log = format!("{}\ntest bonjour ... FAILED\n", "x".repeat(5000));
    lieu.gh.rendre(2, 0, &log, "");
    let journal = forge.journal_rouge(TETE);
    assert!(journal.replace("\r\n", "\n").ends_with("test bonjour ... FAILED\n"), "{journal}");
    assert_eq!(journal.chars().count(), 4000, "coupé par queue");
    let attendus = [
        format!("run list --commit {TETE} --json databaseId,conclusion --repo forge.test/p/d"),
        "run view 8 --log-failed --repo forge.test/p/d".to_string(),
    ];
    assert_eq!(lieu.gh.appels(), attendus);
    // Aucun run en échec, ou gh qui ne répond pas : pas de journal.
    lieu.gh.rendre(3, 0, r#"[{"conclusion":"success","databaseId":9}]"#, "");
    assert_eq!(forge.journal_rouge(TETE), "");
    lieu.gh.rendre(4, 1, "", "boum");
    assert_eq!(forge.journal_rouge(TETE), "");
    assert_eq!(lieu.appels_bien_partis(), 4);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_base_qui_a_bouge_est_a_reintegrer_8009() {
    let lieu = lieu("forge-base");
    let forge = lieu.forge();
    let base = lieu.base();
    let tete = sur(&lieu.depot, &base, &[("a.txt", "a\n")], "a");
    assert_eq!(forge.base_a_bouge("main", &tete), Ok(()));
    // La base avance sur le distant : sa tête n'est plus un ancêtre de la tête de la vague.
    let avancee = sur(&lieu.depot, &base, &[("b.txt", "b\n")], "b");
    git(&lieu.depot, &["push", "-q", "origin", &format!("{avancee}:refs/heads/main")]);
    let refus = forge.base_a_bouge("main", &tete).unwrap_err();
    assert_eq!(refus.code, Code::AReintegrer);
    assert!(refus.preuve.contains(&format!("sa tête distante {avancee}")), "{refus}");
    // Une base qui ne se relit pas compte comme une base qui a bougé.
    assert_eq!(forge.base_a_bouge("absente", &tete).unwrap_err().code, Code::AReintegrer);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_commande_qui_depasse_son_delai_meurt_avec_son_arbre() {
    let dir = racine("forge-delai");
    let mut lente = coquille(LENTE);
    lente.current_dir(&dir);
    let debut = Instant::now();
    let erreur = jouer(lente, &dir.join("lydite"), None, ARBRE).unwrap_err();
    assert!(erreur.contains(" : temps dépassé (10 s)"), "{erreur}");
    let pid = marque(&dir, Duration::ZERO);
    assert!(meurt(pid), "le petit-enfant {pid} survit au délai");
    assert!(debut.elapsed() < Duration::from_secs(28), "tuée à son délai, pas au bout de son sommeil");
    assert_eq!(fs::read_dir(dir.join("lydite")).unwrap().count(), 0, "ses fichiers s'effacent");
    effacer(&dir);
}

#[test]
fn un_petit_enfant_qui_garde_la_sortie_ouverte_ne_retient_rien() {
    let dir = racine("forge-petit-enfant");
    // La coquille sort tout de suite ; son petit-enfant hérite de sa sortie, écrit son pid dans `marque` et dort 30 s.
    let commande = if cfg!(windows) {
        r#"start /B powershell -NoProfile -Command "$PID | Set-Content marque; Start-Sleep 30""#
    } else {
        "sleep 30 & echo $! > marque"
    };
    let mut coquille = coquille(commande);
    coquille.current_dir(&dir);
    let debut = Instant::now();
    let rendu = jouer(coquille, &dir.join("lydite"), None, ARBRE).unwrap();
    assert!(rendu.statut.success(), "{rendu:?}");
    assert!(debut.elapsed() < Duration::from_secs(20), "Lydite n'attend que l'enfant : {:?}", debut.elapsed());
    let pid = marque(&dir, ARBRE);
    let _ = tuer_arbre(pid);
    assert!(meurt(pid), "le petit-enfant {pid} meurt au ménage du test");
    effacer(&dir);
}

#[test]
fn un_appel_hors_utf8_se_lit_avec_un_caractere_de_remplacement() {
    let racine = racine("forge-appels-hors-utf8");
    let gh = FauxGh::ecrire(&racine);
    assert!(gh.appels().is_empty(), "un fichier absent reste aucun appel");
    // Un é d'un argument accentué, écrit par `cmd` en page de code 1252 : 0xE9 n'est pas de l'UTF-8.
    fs::write(racine.join("gh").join("appels.txt"), b"1 pr create --title caf\xe9\n").unwrap();
    assert_eq!(gh.appels(), ["pr create --title caf\u{fffd}".to_string()]);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn une_pr_qui_porte_encore_le_sha_d_avant_est_attendue() {
    let lieu = lieu("forge-ci-retard");
    let forge = lieu.forge();
    let avant = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";
    let lire = |delai: Duration| forge.lire_ci(12, TETE, Some(avant), Duration::from_millis(10), delai);
    // La forge met quelques secondes à porter ce qu'on vient de pousser : la PR porte encore l'ancien sha, puis le
    // nouveau, et la CI se lit au second tour, sur lui.
    lieu.gh.rendre(1, 0, &vue_pr(avant, "OPEN", "MERGEABLE"), "");
    rendre_ci(&lieu.gh, 2, TETE, &[run("test", "completed", "success")], &[]);
    assert_eq!(lire(Duration::from_secs(60)), Ok(Ci { couleur: Couleur::Vert, texte: "test : SUCCESS".into() }));
    assert_eq!(lieu.gh.appels()[1..], tour_de_ci(TETE));
    // Qui ne suit pas dans le délai : 8008, qui dit ce que la PR porte.
    lieu.gh.rendre(5, 0, &vue_pr(avant, "OPEN", "MERGEABLE"), "");
    let refus = lire(Duration::ZERO).unwrap_err();
    assert_eq!(refus.code, Code::CiMuette, "{refus}");
    assert!(refus.preuve.contains(&format!("porte encore {avant}")), "{refus}");
    // Un autre sha est un autre qui a poussé, et une PR fermée l'est avec l'ancien sha aussi : 8010, tout de suite.
    let autre = "1111111111111111111111111111111111111111";
    lieu.gh.rendre(6, 0, &vue_pr(autre, "OPEN", "MERGEABLE"), "");
    lieu.gh.rendre(7, 0, &vue_pr(avant, "CLOSED", "MERGEABLE"), "");
    for _ in 0..2 {
        assert_eq!(lire(Duration::from_secs(60)).unwrap_err().code, Code::PrChangee);
    }
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn les_checks_en_echec_se_lisent_ligne_a_ligne() {
    let texte = concat!(
        "test (linux) : SUCCESS\ntest (windows) : FAILURE\ndoc : SKIPPED\n",
        "lint : TIMED_OUT\nci/externe : ERROR\nx : y : CANCELLED\nsans issue"
    );
    assert_eq!(en_echec(texte), "test (windows) : FAILURE\nlint : TIMED_OUT\nci/externe : ERROR\nx : y : CANCELLED");
    assert_eq!(en_echec("test : SUCCESS\ndoc : NEUTRAL"), "");
    assert_eq!(en_echec(""), "");
}
