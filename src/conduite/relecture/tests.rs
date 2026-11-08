use super::*;
use crate::branchements::bac::bac;
use crate::coeur::fil::{Couleur, Lecture, Verdict, etat};
use crate::coeur::remarques::lire_remarques;
use crate::commande::etat::{etat_dans, rendu};
use crate::conduite::lancer::base_du_depot;
use crate::conduite::mener::{Lieu, mener_sur};
use crate::conduite::publication;
use crate::conduite::tentative::Pris;
use crate::essais::ecrire;
use crate::essais::{PR, depot, distant_nu, faux_relecteur, gh_qui_suit, git, path_garde, racine};
use crate::essais::{remarque, texte};
use std::env;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

/// Un plan de trois tâches : a et c sont critiques et partagent `sortie/` (c attend a), b ne l'est pas.
const PLAN: &str = "plan 1\nobjectif : Ecrire deux fichiers\npreuve : exit 0\n\n\
                    tache a\nfichiers : sortie/\ncritique : oui\n    Ecrire le fichier critique\n    en deux lignes\n\n\
                    tache b\nfichiers : autre/\n    Ecrire l'autre fichier\n\n\
                    tache c\nfichiers : sortie/ doc/lisez.md\ncritique : oui\n    Ecrire c\n";

/// Un profil Claude de ces essais.
fn claude(texte: &str) -> Profil {
    crate::coeur::profil::lire("essai", texte, None, None).unwrap()
}

#[test]
fn la_consigne_dit_l_objectif_les_taches_critiques_la_regle_le_format_et_la_preuve() {
    let plan = Plan::lire(PLAN.as_bytes()).unwrap();
    let critiques: Vec<&Tache> = plan.taches.iter().filter(|tache| tache.critique).collect();
    assert_eq!(critiques.len(), 2);
    let consigne = consigne(&plan.objectif, &critiques, &[]);
    for bout in [
        "Objectif de la vague : Ecrire deux fichiers\n",
        "- tâche a, fichiers : sortie/\n    Ecrire le fichier critique\n    en deux lignes\n",
        "- tâche c, fichiers : sortie/ doc/lisez.md\n    Ecrire c\n",
        "Une remarque est bloquante (`bloquante : oui`) si le travail manque son objectif, perd des données, publie \
         ce qu'il ne doit pas, affaiblit un contrôle, ou laisse un test qui ne peut pas échouer. Toute autre remarque \
         est à noter",
        "Écris tes remarques dans le fichier REMARQUES.lydite, à la racine de ton dossier, et ne touche à rien d'autre",
        "ni à un autre fichier, ni à l'histoire du dépôt (aucun commit)",
        "le fichier DIFF.lydite, à la racine de ton dossier",
        "seule et telle qu'elle est écrite, sans rien y ajouter : lydite remarques REMARQUES.lydite.",
        FORME,
    ] {
        assert!(consigne.contains(bout), "« {bout} » : {consigne}");
    }
    assert!(consigne.ends_with("PREUVE: vert si elle passe, sinon PREUVE: rouge."), "{consigne}");
    // La tâche qui n'est pas critique n'y est pas ; une tâche critique sans `fichiers` le dit.
    assert!(!consigne.contains("autre/") && !consigne.contains("l'autre fichier"), "{consigne}");
    let libre = Tache { id: "d".into(), objectif: "Libre".into(), critique: true, ..Tache::default() };
    assert!(super::consigne("o", &[&libre], &[]).contains("- tâche d, fichiers : tous, rien ne la borne\n    Libre\n"));
    // Le format de la consigne est celui que lit `lire_remarques` : l'exemple qu'elle donne se lit, et sa clé est
    // juste.
    let exemple = FORME.replace("<chemin>", "a.txt").replace("<entier>", "3").replace("<le constat", "Le constat");
    let exemple = exemple.replace("en lignes indentées>", "en lignes indentées.");
    assert_eq!(
        lire_remarques(exemple.as_bytes()).unwrap(),
        [remarque("a.txt", Some(3), true, "Le constat puis la correction, en lignes indentées.")]
    );
}

#[test]
fn le_relecteur_et_la_correction_partent_sur_leurs_cles_du_profil() {
    let plan = Plan::lire(PLAN.as_bytes()).unwrap();
    let critiques: Vec<&Tache> = plan.taches.iter().filter(|tache| tache.critique).collect();
    let remarques = [
        remarque("sortie/a.txt", Some(3), true, "Le test ne peut pas echouer.\nAjouter une assertion."),
        remarque("a", None, false, "Un detail."),
        remarque("sortie/c.txt", None, true, "Perdu."),
    ];
    let objectif = "Corrige au plus petit les remarques bloquantes de la relecture, et rien d'autre\n\
                    sortie/a.txt, ligne 3 : Le test ne peut pas echouer.\n    Ajouter une assertion.\n\
                    sortie/c.txt : Perdu.";
    // Sans clé écrite : opus au relecteur, sonnet à la correction ; deux fois, aux deux tentatives.
    let defaut = claude("sorte = claude\n");
    let relecteur = plan_du_relecteur(&defaut, &plan, &critiques, &[]);
    assert_eq!((relecteur.objectif.as_str(), relecteur.preuve.as_deref()), ("Relire : Ecrire deux fichiers", None));
    let [tache] = &relecteur.taches[..] else { panic!("{relecteur:?}") };
    assert_eq!((tache.id.as_str(), tache.preuve.as_deref()), ("relecteur", Some(PREUVE)));
    assert_eq!((tache.modele.as_deref(), tache.relance.as_deref()), (Some("opus"), Some("opus")));
    assert!(tache.depend.is_empty() && tache.fichiers.is_none() && !tache.critique);
    assert_eq!(tache.objectif, consigne(&plan.objectif, &critiques, &[]));
    let correction = plan_de_la_correction(&defaut, &plan, &critiques, &remarques);
    let [tache] = &correction.taches[..] else { panic!("{correction:?}") };
    assert_eq!((tache.modele.as_deref(), tache.relance.as_deref()), (Some("sonnet"), Some("sonnet")));
    // Les clés du profil : `relecteur` au relecteur, `relance` à la correction, et `modele` à aucun des deux.
    let profil = claude("sorte = claude\nmodele = haiku\nrelance = claude-sonnet-5-5\nrelecteur = claude-opus-5-5\n");
    let relecteur = plan_du_relecteur(&profil, &plan, &critiques, &[]);
    let modeles = |plan: &Plan| (plan.taches[0].modele.clone(), plan.taches[0].relance.clone());
    let pair = |modele: &str| (Some(modele.to_string()), Some(modele.to_string()));
    assert_eq!(modeles(&relecteur), pair("claude-opus-5-5"));
    let correction = plan_de_la_correction(&profil, &plan, &critiques, &remarques);
    assert_eq!(modeles(&correction), pair("claude-sonnet-5-5"));
    // La correction : l'objectif fixe puis les seules remarques bloquantes, la preuve de la vague, les fichiers des
    // tâches critiques réunis sans doublon, et pas critique elle-même.
    assert_eq!((correction.objectif.as_str(), correction.preuve.as_deref()), (objectif, Some("exit 0")));
    let [tache] = &correction.taches[..] else { panic!("{correction:?}") };
    assert_eq!((tache.id.as_str(), tache.objectif.as_str(), tache.preuve.as_deref()), ("correction", objectif, None));
    assert_eq!(tache.fichiers, Some(vec!["sortie/".to_string(), "doc/lisez.md".to_string()]));
    assert!(tache.depend.is_empty() && !tache.critique);
    // Une main brute n'a pas de modèle ; une tâche critique sans `fichiers` ne borne ni le diff ni la correction.
    let brute = Profil::faux(vec!["x".into()], Duration::from_secs(1));
    assert_eq!(modeles(&plan_du_relecteur(&brute, &plan, &critiques, &[])), (None, None));
    assert_eq!(modeles(&plan_de_la_correction(&brute, &plan, &critiques, &remarques)), (None, None));
    let libre = Tache { fichiers: None, ..critiques[0].clone() };
    assert_eq!(fichiers_de(&[&libre, critiques[1]]), None);
    assert_eq!(plan_de_la_correction(&brute, &plan, &[&libre], &remarques).taches[0].fichiers, None);
    assert_eq!(fichiers_de(&[]), None);
}

#[test]
fn le_diff_est_borne_a_1_mo_et_une_ligne_le_dit() {
    // En deçà, ou pile à 1 Mo : tel quel.
    let petit = "diff --git a/x b/x\n".repeat(10);
    assert_eq!(borner(petit.clone(), petit.len() as u64), petit);
    let pile = "x".repeat(TAILLE_MAX);
    assert_eq!(borner(pile.clone(), pile.len() as u64), pile);
    // Au-delà : la lecture bornée n'en a gardé que les premiers octets, et le total est à part ; des lignes
    // entières, puis la mention, le tout sous 1 Mo.
    let gros = format!("{}\n", "y".repeat(99)).repeat(20_000);
    let coupe = borner(gros[..TAILLE_MAX + 1].to_string(), gros.len() as u64);
    assert!(coupe.len() <= TAILLE_MAX && coupe.len() > TAILLE_MAX - 300, "{} octets", coupe.len());
    let derniere = coupe.lines().last().unwrap();
    let mention = format!(
        "[diff coupé par Lydite : {} octets gardés sur {}, le reste n'est pas montré]",
        coupe.len() - derniere.len() - 1,
        gros.len()
    );
    assert_eq!(derniere, mention);
    let gardee = &coupe[..coupe.len() - derniere.len() - 1];
    assert!(gros.starts_with(gardee) && gardee.ends_with('\n'), "des lignes entières");
    // Une seule ligne énorme, de caractères de deux octets, coupée en plein caractère : la lecture bornée en garde
    // un préfixe d'octets, la conversion le remplace, et la coupe tombe à une frontière de caractère, la ligne close.
    let enorme = "é".repeat(TAILLE_MAX);
    let gardes = String::from_utf8_lossy(&enorme.as_bytes()[..TAILLE_MAX + 1]).into_owned();
    let coupe = borner(gardes, enorme.len() as u64);
    assert!(coupe.len() <= TAILLE_MAX && coupe.ends_with("n'est pas montré]\n"));
    let (gardee, _) = coupe.rsplit_once("\n[diff coupé").unwrap();
    assert!(!gardee.is_empty() && gardee.chars().all(|c| c == 'é'));
    // Un diff de la taille de la limite, plus un octet.
    let un_de_trop = "z".repeat(TAILLE_MAX + 1);
    assert!(borner(un_de_trop.clone(), un_de_trop.len() as u64).len() <= TAILLE_MAX);
}

#[test]
fn la_lecture_du_diff_garde_les_premiers_octets_et_compte_le_reste() {
    let racine = racine("relecture-diff-borne");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    let base = commiter(&depot, &[("a.txt", "a\n")]);
    let gros = "une ligne de texte assez longue pour que le diff pèse\n".repeat(40_000);
    let tete = commiter(&depot, &[("gros/g.txt", &gros)]);
    let bac = bac(&depot, &kraken, "borne", &tete).unwrap();
    let diff = || {
        let mut commande = geste(&bac, &["diff", "--no-color", "--no-ext-diff", "--no-textconv", &base, &tete, "--"]);
        commande.env("GIT_LITERAL_PATHSPECS", "1").args(["gros/".to_string()]);
        commande
    };
    let attendu = faire(diff()).unwrap();
    let (gardes, total) = faire_borne(diff(), TAILLE_MAX + 1).unwrap();
    assert_eq!(gardes.len(), TAILLE_MAX + 1, "la lecture en garde un de plus que la borne");
    assert_eq!(total, attendu.len() as u64, "le total compte aussi le reste, non gardé");
    assert_eq!(&gardes[..], &attendu.as_bytes()[..TAILLE_MAX + 1]);
    let _ = fs::remove_dir_all(&racine);
}

/// Commite ces fichiers (chemin, contenu) dans le dépôt, et rend le nouveau `HEAD`.
fn commiter(depot: &Path, fichiers: &[(&str, &str)]) -> String {
    for (chemin, contenu) in fichiers {
        ecrire(depot, chemin, contenu);
    }
    git(depot, &["add", "-A"]);
    git(depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "fichiers"]);
    git(depot, &["rev-parse", "HEAD"])
}

#[test]
fn le_diff_est_limite_aux_fichiers_critiques_sans_joker() {
    let racine = racine("relecture-diff");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    let base = commiter(&depot, &[("src/a.txt", "a\n"), ("doc/b.txt", "b\n"), ("x[1].txt", "x\n"), ("x1.txt", "y\n")]);
    let fichiers = [("src/a.txt", "a2\n"), ("doc/b.txt", "b2\n"), ("x[1].txt", "x2\n"), ("x1.txt", "y2\n")];
    let tete = commiter(&depot, &[fichiers.as_slice(), &[("src/neuf.txt", "neuf\n")]].concat());
    // Les fichiers des tâches critiques : un dossier, et un chemin à crochets, pris tels quels et non comme un joker.
    let (bac_a, bac_b) = (bac(&depot, &kraken, "a", &tete).unwrap(), bac(&depot, &kraken, "b", &tete).unwrap());
    ecrire_le_diff(&bac_a, &base, &tete, Some(&["src/".to_string(), "x[1].txt".to_string()])).unwrap();
    let diff = fs::read_to_string(bac_a.join(DIFF)).unwrap().replace("\r\n", "\n");
    for dedans in [
        "diff --git a/src/a.txt b/src/a.txt",
        "+a2\n",
        "diff --git a/src/neuf.txt",
        "+neuf\n",
        "diff --git a/x[1].txt",
        "+x2\n",
    ] {
        assert!(diff.contains(dedans), "« {dedans} » : {diff}");
    }
    for dehors in ["doc/b.txt", "x1.txt", "+b2", "+y2"] {
        assert!(!diff.contains(dehors), "« {dehors} » ne se relit pas : {diff}");
    }
    // Sans `fichiers` (une tâche critique que rien ne borne), tout le diff de la base à la tête.
    ecrire_le_diff(&bac_b, &base, &tete, None).unwrap();
    let tout = fs::read_to_string(bac_b.join(DIFF)).unwrap();
    for dedans in ["src/a.txt", "doc/b.txt", "x[1].txt", "x1.txt", "src/neuf.txt"] {
        assert!(tout.contains(dedans), "« {dedans} » : {tout}");
    }
    // Un diff qui pèse plus de 1 Mo est coupé, une ligne le dit, et il garde les premières lignes.
    let gros = "une ligne de texte assez longue pour que le diff pèse\n".repeat(40_000);
    let lourd = commiter(&depot, &[("gros/g.txt", &gros), ("petit/p.txt", "p\n")]);
    let bac_c = bac(&depot, &kraken, "c", &lourd).unwrap();
    ecrire_le_diff(&bac_c, &tete, &lourd, Some(&["gros/".to_string()])).unwrap();
    let ecrit = fs::read_to_string(bac_c.join(DIFF)).unwrap();
    assert!(ecrit.len() <= TAILLE_MAX && ecrit.len() > TAILLE_MAX - 300, "{} octets", ecrit.len());
    assert!(ecrit.starts_with("diff --git a/gros/g.txt b/gros/g.txt\n"), "{}", &ecrit[..100]);
    assert!(ecrit.lines().last().unwrap().starts_with("[diff coupé par Lydite : "), "la dernière ligne le dit");
    assert!(!ecrit.contains("petit/p.txt"));
    // Un git qui ne dit pas le diff (une tête qui n'existe pas) est le refus 7009 ; une base inconnue aussi.
    let refus = ecrire_le_diff(&bac_c, &"0".repeat(40), &lourd, None).unwrap_err();
    assert_eq!(refus.code, Code::GitImpossible, "{refus}");
    // Le refus est le texte d'`echec` : la commande, son dossier, son statut, puis ce que git a dit.
    let mut meme =
        geste(&bac_c, &["diff", "--no-color", "--no-ext-diff", "--no-textconv", &"0".repeat(40), &lourd, "--"]);
    meme.env("GIT_LITERAL_PATHSPECS", "1");
    assert_eq!(refus.preuve, faire(meme).unwrap_err());
    let _ = fs::remove_dir_all(&racine);
}

/// Un bac neuf du dépôt sous `kraken`, posé sur le `HEAD` du dépôt : son chemin, et son départ.
fn bac_neuf(depot: &Path, kraken: &Path, id: &str) -> (PathBuf, String) {
    let depart = git(depot, &["rev-parse", "HEAD"]);
    (bac(depot, kraken, id, &depart).unwrap(), depart)
}

/// Des remarques justes, dont une bloquante : le texte que pose le relecteur de ces essais.
fn remarques_justes() -> String {
    texte(&[
        "remarques 2",
        "",
        "remarque 1",
        "fichier : sortie/a.txt",
        "ligne : 3",
        "bloquante : oui",
        "    Le test ne peut pas echouer. SECRET-DE-LA-RELECTURE",
        "    Ajouter une assertion.",
        "",
        "remarque 2",
        "fichier : sortie/a.txt",
        "bloquante : non",
        "    Un nom peu clair.",
        "",
        "rejoue",
        "    La preuve de la vague est verte.",
        "",
        "non rejoue",
        "    Le réseau n'est pas joué.",
    ])
}

/// Un fichier de remarques de la forme 2 sans remarque, ses deux blocs seuls.
const SANS_REMARQUE: &str = "remarques 2\n\nrejoue\n    La preuve de la vague est verte.\n\nnon rejoue\n    Rien.\n";

#[test]
fn le_juge_ne_prend_que_remarques_kraken_seul_ordinaire_et_lisible() {
    let racine = racine("relecture-juge");
    let (depot, kraken) = (depot(&racine), racine.join("lydite"));
    commiter(&depot, &[("suivi.txt", "1\n"), (".gitignore", "ignore/\n")]);
    let juge = |bac: &Path, depart: &str| juger(bac, depart);
    // Absent : 7014.
    let (bac_a, depart) = bac_neuf(&depot, &kraken, "absent");
    let refus = juge(&bac_a, &depart).unwrap_err();
    assert!(
        refus.code == Code::BacTouche && refus.preuve.contains("REMARQUES.lydite n'est pas à la racine du bac"),
        "{refus}"
    );
    // Seul, avec le DIFF.lydite de Kraken ou sans lui, et un fichier que le dépôt ignore : il se lit, et ses octets
    // sont ceux du fichier.
    let (bac_b, depart) = bac_neuf(&depot, &kraken, "juste");
    ecrire(&bac_b, REMARQUES, &remarques_justes());
    ecrire(&bac_b, "ignore/x.o", "ignoré");
    let (remarques, brut) = juge(&bac_b, &depart).unwrap();
    assert_eq!((remarques.len(), brut), (2, remarques_justes()));
    ecrire(&bac_b, DIFF, "diff --git a/x b/x\n");
    assert_eq!(juge(&bac_b, &depart).map(|(r, _)| r.len()), Ok(2));
    // Un autre fichier de plus, suivi ou non : 7014, qui le cite, mais pas le DIFF.lydite.
    ecrire(&bac_b, "suivi.txt", "2\n");
    ecrire(&bac_b, "autre.txt", "x");
    let refus = juge(&bac_b, &depart).unwrap_err();
    assert_eq!(refus.code, Code::BacTouche, "{refus}");
    assert!(refus.preuve.ends_with(r#"en plus de REMARQUES.lydite : "autre.txt", "suivi.txt""#), "{refus}");
    // Un commit, même du seul fichier : 7014.
    let (bac_c, depart) = bac_neuf(&depot, &kraken, "commite");
    ecrire(&bac_c, REMARQUES, &remarques_justes());
    git(&bac_c, &["add", REMARQUES]);
    git(&bac_c, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "r"]);
    let refus = juge(&bac_c, &depart).unwrap_err();
    assert!(refus.code == Code::BacTouche && refus.preuve.contains("la main a commité dans son bac"), "{refus}");
    // Un fichier qui ne se lit pas : 7015, après qu'il a passé le contrôle du bac.
    let (bac_d, depart) = bac_neuf(&depot, &kraken, "illisible");
    ecrire(&bac_d, REMARQUES, "pas des remarques\n");
    let refus = juge(&bac_d, &depart).unwrap_err();
    assert_eq!(refus.code, Code::RemarquesIllisibles, "{refus}");
    // La forme 1 se lit, mais le juge du relecteur exige la forme 2 : 7015, à la première ligne ; la forme 2 sans
    // l'un de ses blocs est refusée de même.
    for (nom, brut, bout) in [
        ("forme-1", "remarques 1\n", "ligne 1 « remarques 1 » : le relecteur écrit la forme 2"),
        ("sans-non-rejoue", "remarques 2\n\nrejoue\n    a\n", "le fichier n'a pas de bloc `non rejoue`"),
    ] {
        assert!(lire_remarques(b"remarques 1\n").is_ok());
        let (bac_f, depart) = bac_neuf(&depot, &kraken, nom);
        ecrire(&bac_f, REMARQUES, brut);
        let refus = juge(&bac_f, &depart).unwrap_err();
        assert_eq!(refus.code, Code::RemarquesIllisibles, "{refus}");
        assert!(refus.preuve.contains(bout), "{refus}");
    }
    // Un fichier que le dépôt ignore : 7014, bien qu'il soit seul et se lise.
    commiter(&depot, &[(".gitignore", "ignore/\nREMARQUES.lydite\n")]);
    let (bac_e, depart) = bac_neuf(&depot, &kraken, "ignore");
    ecrire(&bac_e, REMARQUES, &remarques_justes());
    let refus = juge(&bac_e, &depart).unwrap_err();
    assert!(refus.code == Code::BacTouche && refus.preuve.contains("le dépôt visé ignore REMARQUES.lydite"), "{refus}");
    let _ = fs::remove_dir_all(&racine);
}

/// Le délai des fausses mains, et l'instant où ces essais lisent le fil, bien après eux.
const DELAI: Duration = Duration::from_secs(30);
const PLUS_TARD: &str = "2099-01-01T00:00:00.000Z";

/// Le plan des essais de vague : a, critique, écrit sous `sortie/` ; b, qui ne l'est pas, sous `autre/`. Elles ne
/// partagent rien : elles tournent ensemble.
const PLAN_DE_VAGUE: &str = "plan 1\nobjectif : Ecrire deux fichiers\npreuve : exit 0\n\n\
                             tache a\nfichiers : sortie/\ncritique : oui\n    Ecrire le fichier critique\n\n\
                             tache b\nfichiers : autre/\n    Ecrire l'autre fichier\n";

/// Le plan sans tâche critique : les mêmes tâches, a ne l'étant pas (son objectif garde son mot, que la fausse main
/// lit pour savoir sous quel dossier écrire).
fn plan_sans_critique() -> Plan {
    Plan::lire(PLAN_DE_VAGUE.replace("critique : oui\n", "").as_bytes()).unwrap()
}

/// Un essai : un dépôt visé jetable, un dossier de Kraken que le premier geste crée, et le faux relecteur, qui est
/// aussi la main des tâches et celle de la correction, avec son dossier.
struct Essai {
    racine: PathBuf,
    depot: PathBuf,
    kraken: PathBuf,
    faux: PathBuf,
    profil: Profil,
}

impl Essai {
    fn nouveau(nom: &str) -> Essai {
        let racine = racine(nom);
        let depot = depot(&racine);
        let faux = racine.join("relecteur");
        let profil = Profil::faux(faux_relecteur(&faux), DELAI);
        Essai { kraken: racine.join("lydite"), racine, depot, faux, profil }
    }

    /// Les remarques que le faux relecteur écrit à sa tentative `n`.
    fn remarques(&self, n: usize, texte: &str) {
        fs::write(self.faux.join(format!("remarques-{n}.lydite")), texte).unwrap();
    }

    /// Pose une marque dans le dossier du faux relecteur : `toucher-1`, `hors-fichiers`, `ne-rien-faire`.
    fn marque(&self, nom: &str) {
        fs::write(self.faux.join(nom), "").unwrap();
    }

    /// Ce que le faux relecteur a gardé dans son dossier, en fins de ligne `\n` ; `None` s'il n'a rien gardé.
    fn garde(&self, nom: &str) -> Option<String> {
        let octets = fs::read(self.faux.join(nom)).ok()?;
        Some(String::from_utf8_lossy(&octets).replace("\r\n", "\n"))
    }

    /// Le `HEAD` du dépôt visé.
    fn base(&self) -> String {
        git(&self.depot, &["rev-parse", "HEAD"])
    }

    /// La vague de ce plan comme `lancer` la mène, sa relecture comprise : avec ce `gh`, sa forge aussi, sur un
    /// distant nu (`distant_nu`) ; la CI se lit toutes les 10 ms, une minute au plus. Rend le code de sortie.
    fn mener(&self, plan: &Plan, gh: Option<&Path>) -> ExitCode {
        let programme: OsString = gh.map_or_else(|| "gh-absent".into(), |gh| gh.as_os_str().to_os_string());
        let lieu = Lieu {
            profil: &self.profil,
            depot: &self.depot,
            dossier: &self.kraken,
            gh: &programme,
            personnel: Some(&self.racine),
            ci: (Duration::from_millis(10), Duration::from_secs(60)),
        };
        let issue = match gh {
            None => {
                let base = base_du_depot(&self.depot).unwrap();
                mener_sur(&lieu, plan, &base, Liens::default(), None, Origine::Plan)
            }
            Some(_) => {
                let proposition = publication::avant_la_vague(&programme, &self.kraken, &self.depot, "main").unwrap();
                mener_sur(&lieu, plan, &proposition.base, Liens::default(), Some(&proposition), Origine::Plan)
            }
        };
        issue.unwrap()
    }

    /// Le fil, qui se lit sans écart.
    fn fil(&self) -> Vec<Evenement> {
        let Lecture { notes, ecarts } = crate::branchements::dossier::lire(&self.kraken).unwrap();
        assert!(ecarts.is_empty(), "{ecarts:?}");
        notes.into_iter().map(|note| note.evenement).collect()
    }

    /// Ce que le faux `gh` a reçu, un appel par ligne.
    fn appels_de_gh(&self) -> Vec<String> {
        let appels = fs::read_to_string(self.racine.join("gh").join("appels.txt")).unwrap_or_default();
        appels.lines().map(|ligne| ligne.replace('"', "").trim().to_string()).collect()
    }

    /// Le corps de la PR que le faux `gh` a reçu, en fins de ligne `\n`.
    fn corps(&self) -> String {
        fs::read_to_string(self.racine.join("gh").join("corps.txt")).unwrap().replace("\r\n", "\n")
    }

    /// La tête que le distant nu garde pour la branche `branche`, `None` s'il ne la connaît pas.
    fn distante(&self, branche: &str) -> Option<String> {
        let nu = self.racine.join("distant.git");
        let lue = std::process::Command::new("git")
            .current_dir(&nu)
            .args(["rev-parse", "--verify", "-q", &format!("refs/heads/{branche}")])
            .output()
            .unwrap();
        lue.status.success().then(|| String::from_utf8_lossy(&lue.stdout).trim().to_string())
    }

    /// Les noms de ce que contient `bacs`, triés.
    fn bacs(&self) -> Vec<String> {
        let Ok(bacs) = fs::read_dir(self.kraken.join("bacs")) else { return Vec::new() };
        let mut noms: Vec<String> = bacs.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        noms.sort();
        noms
    }
}

/// Les ids des vagues que le fil ouvre, dans l'ordre.
fn ouvertes(fil: &[Evenement]) -> Vec<String> {
    let ouverte = |evenement: &Evenement| match evenement {
        Evenement::Vague { vague, .. } => Some(vague.clone()),
        _ => None,
    };
    fil.iter().filter_map(ouverte).collect()
}

/// La tête que la dernière fusion de la vague `vague` a notée.
fn tete_de(fil: &[Evenement], vague: &str) -> String {
    let fusion = |evenement: &Evenement| match evenement {
        Evenement::Fusion { vague: v, sha, .. } if v == vague => Some(sha.clone()),
        _ => None,
    };
    fil.iter().rev().find_map(fusion).unwrap_or_else(|| panic!("la vague {vague} n'a aucune fusion"))
}

/// Les refus de la forge que le fil note sur la vague `vague`, par leur code.
fn refus_de_forge(fil: &[Evenement], vague: &str) -> Vec<(u16, String)> {
    let refus = |evenement: &Evenement| match evenement {
        Evenement::RefusForge { vague: v, code, preuve, .. } if v == vague => Some((*code, preuve.clone())),
        _ => None,
    };
    fil.iter().filter_map(refus).collect()
}

/// Les fins de la main 1 de la vague `vague` : la couleur et le texte de chacune.
fn fins_de(fil: &[Evenement], vague: &str) -> Vec<(Couleur, String)> {
    let fin = |evenement: &Evenement| match evenement {
        Evenement::Fin { vague: v, main: 1, couleur, texte, .. } if v == vague => Some((*couleur, texte.clone())),
        _ => None,
    };
    fil.iter().filter_map(fin).collect()
}

/// Aucun de ces dossiers d'essai ne demeure dans le dossier temporaire du système : un test qui ouvre plusieurs
/// essais sous le même nom (`let essai = …`) efface chacun avant que le suivant ne le masque.
fn rien_ne_demeure(racines: &[PathBuf]) {
    let restes: Vec<&PathBuf> = racines.iter().filter(|racine| racine.exists()).collect();
    assert!(restes.is_empty(), "des essais ont laissé leur dossier derrière eux : {restes:?}");
}

#[test]
fn sans_tache_critique_assemblee_il_n_y_a_pas_de_relecture() {
    // Un plan sans tâche critique : aucun relecteur ne part.
    let essai = Essai::nouveau("relecture-aucune");
    let mut racines = vec![essai.racine.clone()];
    assert_eq!(essai.mener(&plan_sans_critique(), None), ExitCode::SUCCESS);
    assert!(essai.garde("relecture-1").is_none(), "aucun relecteur n'est parti");
    let fil = essai.fil();
    assert_eq!(ouvertes(&fil).len(), 1, "la seule vague du plan");
    assert!(!fil.iter().any(|e| matches!(e, Evenement::Relecture { .. } | Evenement::Correction { .. })));
    let _ = fs::remove_dir_all(&essai.racine);
    // Une tâche critique qui n'est pas assemblée (sa preuve est rouge, elle est classée) n'est pas relue non plus.
    let essai = Essai::nouveau("relecture-critique-dehors");
    racines.push(essai.racine.clone());
    let plan = "plan 1\nobjectif : o\npreuve : exit 0\n\ntache a\npreuve : exit 1\ncritique : oui\n    Ecrire a\n\n\
                tache b\n    Ecrire b\n";
    assert_eq!(essai.mener(&Plan::lire(plan.as_bytes()).unwrap(), None), ExitCode::FAILURE);
    assert!(essai.garde("relecture-1").is_none(), "aucun relecteur n'est parti");
    assert_eq!(ouvertes(&essai.fil()).len(), 1);
    let _ = fs::remove_dir_all(&essai.racine);
    // Des copies n'ont ni tête ni tâche critique.
    let essai = Essai::nouveau("relecture-copies");
    racines.push(essai.racine.clone());
    let copies = Plan::copies("Ecrire", Some("exit 0"), 2);
    assert_eq!(essai.mener(&copies, None), ExitCode::SUCCESS);
    assert!(essai.garde("relecture-1").is_none());
    let _ = fs::remove_dir_all(&essai.racine);
    rien_ne_demeure(&racines);
}

#[test]
fn seules_les_taches_critiques_assemblees_se_relisent() {
    // a et c sont critiques ; c est classée (sa preuve est rouge) et reste dehors : la consigne, le diff et la
    // correction ne parlent que de a.
    let essai = Essai::nouveau("relecture-assemblees");
    essai.remarques(1, SANS_REMARQUE);
    let plan = "plan 1\nobjectif : Ecrire trois fichiers\npreuve : exit 0\n\n\
                tache a\nfichiers : sortie/\ncritique : oui\n    Ecrire a, critique\n\n\
                tache b\nfichiers : autre/\n    Ecrire b\n\n\
                tache c\nfichiers : doc/\npreuve : exit 1\ncritique : oui\n    Ecrire c, critique\n";
    assert_eq!(essai.mener(&Plan::lire(plan.as_bytes()).unwrap(), None), ExitCode::FAILURE, "c est classée");
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r] = &ouvertes[..] else { panic!("la vague et le relecteur : {ouvertes:?}") };
    let consigne = essai.garde("consigne-relecteur-1.txt").unwrap();
    assert!(consigne.contains("- tâche a, fichiers : sortie/\n    Ecrire a, critique\n"), "{consigne}");
    assert!(!consigne.contains("tâche c") && !consigne.contains("doc/") && !consigne.contains("tâche b"), "{consigne}");
    for fixe in ["IMPOSSIBLE: <reason>", "Change only the files of your task"] {
        assert!(!consigne.contains(fixe), "« {fixe} » : {consigne}");
    }
    let diff = essai.garde("diff-1.txt").unwrap();
    assert!(diff.contains(&format!("sortie/main-{v}-1.txt")) && !diff.contains("autre/"), "{diff}");
    // Relue sans remarque : prise, sans correction.
    assert!(fil.iter().any(
        |e| matches!(e, Evenement::Relecture { vague, relue, n: 0, bloquantes: 0, .. } if vague == r && relue == v)
    ));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_relecture_sans_bloquante_laisse_la_forge_pousser_la_tete_de_la_vague() {
    let essai = Essai::nouveau("relecture-sans-bloquante");
    distant_nu(&essai.racine, &essai.depot);
    let gh = gh_qui_suit(&essai.racine, &essai.depot);
    let brut = texte(&[
        "remarques 2",
        "",
        "remarque 1",
        "fichier : sortie/a.txt",
        "bloquante : non",
        "    Un nom peu clair. SECRET-DE-LA-RELECTURE",
        "",
        "rejoue",
        "    La preuve de la vague est verte.",
        "",
        "non rejoue",
        "    Le réseau n'est pas joué.",
        "    Windows n'est pas joué.",
    ]);
    essai.remarques(1, &brut);
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, Some(&gh)), ExitCode::SUCCESS);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r] = &ouvertes[..] else { panic!("la vague, puis le relecteur : {ouvertes:?}") };
    // La relecture est notée au fil de la vague relue, avec ses nombres et son texte ; aucune correction ne part.
    let notee = fil.iter().find_map(|evenement| match evenement {
        Evenement::Relecture { vague, relue, n, bloquantes, remarques } => {
            Some((vague.clone(), relue.clone(), *n, *bloquantes, remarques.clone()))
        }
        _ => None,
    });
    assert_eq!(notee, Some((r.clone(), v.clone(), 1, 0, brut)));
    assert!(!fil.iter().any(|evenement| matches!(evenement, Evenement::Correction { .. })));
    assert_eq!(fins_de(&fil, r).iter().map(|(couleur, _)| *couleur).collect::<Vec<_>>(), [Couleur::Vert]);
    // La forge a poussé la tête de la vague : la branche locale, la distante et la dernière fusion s'accordent.
    let (t, branche) = (tete_de(&fil, v), format!("lydite/vague-{v}"));
    assert_eq!(git(&essai.depot, &["rev-parse", &branche]), t);
    assert_eq!(essai.distante(&branche), Some(t.clone()));
    // Le verdict suit aussitôt la relecture : la tête relue, et le lancement du relecteur (null pour une main nue).
    let rang = fil.iter().position(|e| matches!(e, Evenement::Relecture { .. })).unwrap();
    let lancement = fil.iter().rev().find_map(|e| match e {
        Evenement::Lancement { vague, profil, empreinte, modele, .. } if vague == r => {
            Some((profil.clone(), empreinte.clone(), modele.clone()))
        }
        _ => None,
    });
    let (profil, empreinte, modele) = match lancement {
        Some((profil, empreinte, modele)) => (Some(profil), Some(empreinte), Some(modele)),
        None => (None, None, None),
    };
    let verdict =
        Evenement::VerdictRelecture { vague: r.clone(), relue: v.clone(), sha: t.clone(), profil, empreinte, modele };
    assert_eq!(fil.get(rang + 1), Some(&verdict), "{fil:?}");
    // Son corps dit la relecture en nombres, jamais en texte.
    let sceaux = crate::branchements::dossier::lire_sceaux(&essai.kraken);
    let (octets, empreinte) = (sceaux[0].octets, &sceaux[0].empreinte);
    let attendu = format!(
        "Reçu\n- Coût : inconnu\n- 3 mains, 0 relance, 0 classée\n- Fil sealed at {octets} bytes: sha256 {empreinte}.\n\
         - Réglages de la preuve changés : aucun\n\n\
         Ecrire deux fichiers\n\nPreuve : exit 0 → vert\n\na : Ecrire le fichier critique\nb : Ecrire \
         l'autre fichier\n\nRelecture : 1 remarques, dont 0 bloquantes.\nReviewed and replayed on commit {t}.\n\
         Not replayed (from the review):\n- `Le réseau n'est pas joué.`\n- `Windows n'est pas joué.`\n"
    );
    assert_eq!(essai.corps(), attendu);
    // Le relecteur a reçu sa consigne : l'objectif, la tâche critique seule avec ses fichiers, la preuve prêtée.
    let consigne = essai.garde("consigne-relecteur-1.txt").unwrap();
    assert!(consigne.contains("Objectif de la vague : Ecrire deux fichiers\n"), "{consigne}");
    assert!(consigne.contains("- tâche a, fichiers : sortie/\n    Ecrire le fichier critique\n"), "{consigne}");
    assert!(!consigne.contains("autre/") && !consigne.contains("l'autre fichier"), "{consigne}");
    assert!(consigne.contains("lydite remarques REMARQUES.lydite"), "{consigne}");
    // Son bac portait le diff du seul travail critique, et le dossier de l'exécutable de Kraken tête son PATH.
    let diff = essai.garde("diff-1.txt").expect("le relecteur a vu DIFF.lydite");
    assert!(diff.contains(&format!("diff --git a/sortie/main-{v}-1.txt b/sortie/main-{v}-1.txt")), "{diff}");
    assert!(!diff.contains("autre/"), "le travail de b n'est pas critique : {diff}");
    let preuve = essai.garde("preuve-1.txt").expect("le relecteur a vu PREUVE.lydite");
    let entete = format!("Proof command: exit 0\nReplayed green by Lydite on commit: {t}\n");
    assert!(preuve.starts_with(&entete), "{preuve}");
    assert!(consigne.contains("PREUVE.lydite holds the proof that Lydite replayed on this commit: judge the work"));
    let exe = env::current_exe().unwrap();
    let (separateur, ancien) = (if cfg!(windows) { ';' } else { ':' }, env::var("PATH").unwrap());
    let attendu = format!("{}{separateur}{ancien}", exe.parent().unwrap().display());
    assert_eq!(path_garde(&essai.faux.join("path-relecteur-1.txt")), attendu);
    // Rien de la relecture ne reste : son bac et sa branche sont retirés, et le dépôt visé n'a rien vu.
    assert!(!essai.bacs().iter().any(|nom| nom.starts_with(&format!("main-{r}"))), "{:?}", essai.bacs());
    assert_eq!(git(&essai.depot, &["branch", "--list", &format!("lydite/main-{r}-1")]), "");
    assert_eq!(git(&essai.depot, &["status", "--porcelain", "--untracked-files=all"]), "");
    // La PR est partie de la tête de la vague : `lydite etat` dit la relecture, sa PR verte, et sort en 0.
    let dit = rendu(&etat(&fil), PLUS_TARD);
    // La ligne de reçu a ses propres essais : ici, le reste du texte.
    let dit = dit.lines().filter(|ligne| !ligne.starts_with("Coût : ")).collect::<Vec<_>>().join("\n");
    assert!(dit.contains("Relecture : 1 remarques, dont 0 bloquantes"), "{dit}");
    assert!(dit.ends_with(&format!("PR 12 · {PR} · CI verte (constatée)")), "{dit}");
    assert_eq!(etat_dans(&essai.kraken), Ok(ExitCode::SUCCESS));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn des_remarques_bloquantes_valent_une_correction_poussee_de_t2() {
    let essai = Essai::nouveau("relecture-corrigee");
    distant_nu(&essai.racine, &essai.depot);
    let gh = gh_qui_suit(&essai.racine, &essai.depot);
    essai.remarques(1, &remarques_justes());
    let base = essai.base();
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, Some(&gh)), ExitCode::SUCCESS);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r, c] = &ouvertes[..] else { panic!("la vague, le relecteur, la correction : {ouvertes:?}") };
    // La relecture a rendu deux remarques, dont une bloquante, et la correction est liée à la vague relue, avant
    // tout départ de sa tâche.
    assert!(fil.iter().any(
        |e| matches!(e, Evenement::Relecture { vague, relue, n: 2, bloquantes: 1, .. } if vague == r && relue == v)
    ));
    let rang = |cherche: &dyn Fn(&Evenement) -> bool| fil.iter().position(cherche).unwrap();
    let assemblage = rang(&|e| matches!(e, Evenement::Assemblage { vague, .. } if vague == c));
    let lien = rang(&|e| *e == Evenement::Correction { vague: c.clone(), corrige: v.clone() });
    let tache = rang(&|e| matches!(e, Evenement::Tache { vague, .. } if vague == c));
    assert!(assemblage < lien && lien < tache, "{assemblage} {lien} {tache}");
    // La correction part de `t`, la tête de la vague, et la branche de la vague avance à `t2`, que la forge pousse.
    let (t, t2, branche) = (tete_de(&fil, v), tete_de(&fil, c), format!("lydite/vague-{v}"));
    assert_ne!(t, t2);
    assert!(fil.iter().any(|e| matches!(e, Evenement::Assemblage { vague, base, .. } if vague == c && *base == t)));
    assert_eq!(git(&essai.depot, &["rev-parse", &branche]), t2);
    assert_eq!(essai.distante(&branche), Some(t2.clone()));
    git(&essai.depot, &["merge-base", "--is-ancestor", &t, &t2]);
    let arbre = git(&essai.depot, &["ls-tree", "-r", "--name-only", &t2]);
    assert_eq!(arbre, format!("autre/main-{v}-2.txt\nsortie/correction.txt\nsortie/main-{v}-1.txt"));
    // L'objectif de la correction : la règle fixe, puis la seule remarque bloquante, sa ligne et son texte.
    let consigne = essai.garde("consigne-correction.txt").unwrap();
    let objectif = "Corrige au plus petit les remarques bloquantes de la relecture, et rien d'autre\n\
                    sortie/a.txt, ligne 3 : Le test ne peut pas echouer. SECRET-DE-LA-RELECTURE\n    Ajouter une \
                    assertion.\n";
    let (socle, consigne) = consigne.split_once("\n\n").unwrap();
    assert!(socle.starts_with("A refused action is not retried another way.\n"), "{socle}");
    assert!(consigne.starts_with(objectif), "{consigne}");
    assert!(!consigne.contains("Un nom peu clair"), "une remarque à noter n'est pas à corriger : {consigne}");
    // Le texte des remarques n'est ni dans le corps de la PR, ni dans un commit poussé : il n'y reste que les nombres.
    let corps = essai.corps();
    let ligne = format!(
        "Relecture : 2 remarques, dont 1 bloquantes, corrigées.\nReviewed on commit {t}, corrected in {t2} without a \
         second review, and replayed on commit {t2}.\nNot replayed (from the review):\n- `Le réseau n'est pas joué.`\n"
    );
    assert!(corps.ends_with(&format!("\n\n{ligne}")), "{corps}");
    let messages = git(&essai.depot, &["log", "--format=%B", &format!("{base}..{t2}")]);
    for (ou, texte) in [("le corps", corps.as_str()), ("l'histoire", messages.as_str())] {
        assert!(!texte.contains("SECRET-DE-LA-RELECTURE") && !texte.contains("Le test ne peut pas"), "{ou} : {texte}");
    }
    // `git grep` sort en 1 quand rien ne correspond : aucun fichier de la tête poussée ne porte le texte.
    let grep = std::process::Command::new("git").current_dir(&essai.depot).args(["grep", "-q", "SECRET", &t2]).status();
    assert_eq!(grep.unwrap().code(), Some(1), "l'arbre poussé ne porte pas le texte des remarques");
    // Tout est rangé : les bacs de la vague, du relecteur et de la correction, leurs branches, le worktree de la
    // vague et celui de la correction. La branche de la vague reste, qui porte la PR.
    assert!(essai.bacs().is_empty(), "{:?}", essai.bacs());
    let branches = git(&essai.depot, &["for-each-ref", "--format=%(refname:short)", "refs/heads/lydite"]);
    assert_eq!(branches, branche);
    let rangements =
        |id: &String| fil.iter().filter(|e| matches!(e, Evenement::Rangement { vague, .. } if vague == id)).count();
    assert_eq!((rangements(v), rangements(c)), (1, 1), "la forge range la vague et sa correction");
    // `lydite etat` montre la chaîne entière, la correction assemblée, et sort en 0.
    let dit = rendu(&etat(&fil), PLUS_TARD);
    // La ligne de reçu a ses propres essais : ici, le reste du texte.
    let dit = dit.lines().filter(|ligne| !ligne.starts_with("Coût : ")).collect::<Vec<_>>().join("\n");
    let ligne = format!("Correction : main-{c}-1 correction : assemblée");
    assert!(dit.contains("Relecture : 2 remarques, dont 1 bloquantes") && dit.contains(&ligne), "{dit}");
    assert!(dit.ends_with(&format!("PR 12 · {PR} · CI verte (constatée)")), "{dit}");
    assert_eq!(etat_dans(&essai.kraken), Ok(ExitCode::SUCCESS));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn le_relecteur_voit_la_preuve_rejouee_sur_sa_tete_avec_sa_sortie() {
    let essai = Essai::nouveau("relecture-preuve-rejouee");
    essai.remarques(1, SANS_REMARQUE);
    let preuve = "echo PREUVE-REJOUEE-SUR-T";
    let plan = Plan::lire(PLAN_DE_VAGUE.replace("preuve : exit 0", &format!("preuve : {preuve}")).as_bytes()).unwrap();
    let _ = essai.mener(&plan, None);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, _] = &ouvertes[..] else { panic!("la vague, puis le relecteur : {ouvertes:?}") };
    let t = tete_de(&fil, v);
    // La sortie que le dernier `vague.fusion` qui porte `t` garde est celle de la preuve rejouée.
    let verif = fil.iter().rev().find_map(|e| match e {
        Evenement::Fusion { vague, sha, verif, .. } if vague == v && *sha == t => Some(verif.clone()),
        _ => None,
    });
    let verif = verif.unwrap();
    assert!(verif.contains("PREUVE-REJOUEE-SUR-T"), "{verif:?}");
    let vu = essai.garde("preuve-1.txt").expect("le relecteur a vu PREUVE.lydite");
    // `echo` imprime `\r\n` sous Windows : `garde` rend le fichier en `\n`, l'attendu se compare de même.
    let attendu = texte_de_la_preuve(Some(preuve), &fil, v, &t).replace("\r\n", "\n");
    assert_eq!(vu, attendu);
    assert!(vu.starts_with(&format!("Proof command: {preuve}\nReplayed green by Lydite on commit: {t}\n")), "{vu}");
    assert!(vu.contains(verif.replace("\r\n", "\n").trim_end()), "la sortie de la preuve rejouée : {vu}");
    assert!(essai.garde("sans-preuve-1").is_none());
    // Le fichier est du bac, retiré avec lui : le dépôt visé n'a rien vu.
    assert_eq!(git(&essai.depot, &["status", "--porcelain", "--untracked-files=all"]), "");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn le_fichier_de_preuve_dit_en_une_ligne_ce_qui_manque_et_ne_garde_que_la_fin_de_la_sortie() {
    let fusion = |vague: &str, sha: &str, verif: &str| Evenement::Fusion {
        vague: vague.into(),
        main: 1,
        sha: sha.into(),
        verif: verif.into(),
    };
    let fil = [fusion("v", "aaa", "premiere\n"), fusion("w", "bbb", "autre vague\n"), fusion("v", "aaa", "derniere\n")];
    // Le dernier `vague.fusion` de la vague qui porte la tête.
    assert_eq!(
        texte_de_la_preuve(Some("cargo test"), &fil, "v", "aaa"),
        "Proof command: cargo test\nReplayed green by Lydite on commit: aaa\nOutput (last 4000 characters), as Lydite \
         kept it:\nderniere\n"
    );
    // Aucun `vague.fusion` ne porte la tête : une ligne, sans sortie.
    let sans = texte_de_la_preuve(Some("cargo test"), &fil, "v", "ccc");
    assert_eq!(sans.lines().count(), 1, "{sans}");
    assert!(sans.contains("ccc") && !sans.contains("derniere"), "{sans}");
    let autre_vague = texte_de_la_preuve(Some("cargo test"), &fil, "v", "bbb");
    assert!(autre_vague.contains("bbb") && !autre_vague.contains("autre vague"), "{autre_vague}");
    // Pas de commande de preuve : une ligne, sans sortie.
    for preuve in [None, Some(""), Some("  ")] {
        let sans = texte_de_la_preuve(preuve, &fil, "v", "aaa");
        assert_eq!(sans, "This wave has no proof command: nothing was replayed.\n");
    }
    // Les 4000 derniers caractères de la sortie, comptés en caractères.
    let longue = format!("{}{}", "début".repeat(1000), "é".repeat(SORTIE_MAX));
    let fil = [fusion("v", "aaa", &longue)];
    let texte = texte_de_la_preuve(Some("p"), &fil, "v", "aaa");
    let (_, sortie) = texte.split_once("kept it:\n").unwrap();
    assert_eq!(sortie, format!("{}\n", "é".repeat(SORTIE_MAX)));
}

#[cfg(unix)]
#[test]
fn une_reponse_qui_dit_rien_de_bloquant_ne_vaut_pas_moins_que_la_bloquante_du_fichier() {
    // Le relecteur dit dans sa réponse qu'il n'y a rien de bloquant ; son fichier porte une remarque bloquante. Kraken
    // compte le fichier, jamais la réponse : la bloquante compte, et la correction part.
    let essai = Essai::nouveau("relecture-resume-sans-bloquante");
    essai.remarques(1, &remarques_justes());
    essai.marque("dit-rien-de-bloquant");
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    let _ = essai.mener(&plan, None);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r, c] = &ouvertes[..] else { panic!("la vague, le relecteur, la correction : {ouvertes:?}") };
    assert!(fil.iter().any(
        |e| matches!(e, Evenement::Relecture { vague, relue, n: 2, bloquantes: 1, .. } if vague == r && relue == v)
    ));
    assert!(fil.contains(&Evenement::Correction { vague: c.clone(), corrige: v.clone() }));
    let consigne = essai.garde("consigne-correction.txt").unwrap();
    assert!(consigne.contains("Le test ne peut pas echouer."), "{consigne}");
    // La réponse gardée du relecteur porte bien la phrase qui dit qu'il n'y a rien de bloquant.
    let fins = fins_de(&fil, r);
    assert!(
        fins.iter().any(|(_, texte)| texte.contains("There is nothing blocking in my review.")),
        "la réponse du relecteur : {fins:?}"
    );
    let _ = fs::remove_dir_all(&essai.racine);
}

/// Rien ne s'est poussé : le distant nu ne connaît aucune branche de Kraken, et `gh` n'a servi qu'à se dire connecté.
fn rien_de_pousse(essai: &Essai) {
    let nu = essai.racine.join("distant.git");
    assert_eq!(git(&nu, &["branch", "--list", "lydite/*"]), "", "rien n'est poussé");
    assert_eq!(essai.appels_de_gh(), ["auth status --hostname forge.test"]);
}

#[test]
fn une_correction_non_assemblee_est_le_refus_7017_et_rien_ne_se_pousse() {
    let essai = Essai::nouveau("relecture-7017");
    distant_nu(&essai.racine, &essai.depot);
    let gh = gh_qui_suit(&essai.racine, &essai.depot);
    essai.remarques(1, &remarques_justes());
    // La main de la correction change un chemin hors des `fichiers` des tâches critiques (7013), à ses deux tentatives.
    essai.marque("hors-fichiers");
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, Some(&gh)), ExitCode::FAILURE);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, _, c] = &ouvertes[..] else { panic!("la vague, le relecteur, la correction : {ouvertes:?}") };
    let fins = fins_de(&fil, c);
    assert_eq!(fins.iter().map(|(couleur, _)| *couleur).collect::<Vec<_>>(), [Couleur::Rouge, Couleur::Rouge]);
    for (_, texte) in &fins {
        assert!(texte.starts_with("7013 vague.hors_fichiers : ") && texte.contains("dehors.txt"), "{texte}");
    }
    // Le refus 7017 est noté sur la vague relue, il nomme la tâche dehors, et la branche de la vague n'a pas avancé.
    let refus = refus_de_forge(&fil, v);
    let [(7017, preuve)] = &refus[..] else { panic!("{refus:?}") };
    assert!(preuve.contains(&format!("la vague de correction {c} n'a pas assemblé sa tâche")), "{preuve}");
    assert!(preuve.contains("dehors.txt"), "{preuve}");
    assert_eq!(git(&essai.depot, &["rev-parse", &format!("lydite/vague-{v}")]), tete_de(&fil, v));
    rien_de_pousse(&essai);
    // La correction n'a pas de seconde relecture, et les bacs demeurent : la tâche classée garde le sien.
    assert_eq!(fil.iter().filter(|e| matches!(e, Evenement::Relecture { .. })).count(), 1);
    assert!(essai.bacs().contains(&format!("main-{c}-1")), "{:?}", essai.bacs());
    // `lydite etat` dit la relecture, la correction dehors, le refus 7017, et sort en 1.
    let dit = rendu(&etat(&fil), PLUS_TARD);
    assert!(dit.contains(&format!("Correction : main-{c}-1 correction : classée")), "{dit}");
    assert!(dit.contains("refusée (7017) : "), "{dit}");
    assert_eq!(etat_dans(&essai.kraken), Ok(ExitCode::FAILURE));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn une_correction_qui_ne_change_rien_n_est_pas_une_correction() {
    let essai = Essai::nouveau("relecture-sans-changement");
    distant_nu(&essai.racine, &essai.depot);
    let gh = gh_qui_suit(&essai.racine, &essai.depot);
    essai.remarques(1, &remarques_justes());
    essai.marque("ne-rien-faire");
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, Some(&gh)), ExitCode::FAILURE);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, _, c] = &ouvertes[..] else { panic!("la vague, le relecteur, la correction : {ouvertes:?}") };
    // Assemblée, mais à la tête d'avant : les remarques bloquantes restent sans correction (7017).
    assert_eq!(tete_de(&fil, c), tete_de(&fil, v));
    let refus = refus_de_forge(&fil, v);
    let [(7017, preuve)] = &refus[..] else { panic!("{refus:?}") };
    assert!(preuve.contains(&format!("la vague de correction {c} n'a rien changé à la tête")), "{preuve}");
    rien_de_pousse(&essai);
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_fichier_illisible_deux_fois_est_7015_puis_7016_et_rien_ne_se_pousse() {
    let essai = Essai::nouveau("relecture-7016");
    distant_nu(&essai.racine, &essai.depot);
    let gh = gh_qui_suit(&essai.racine, &essai.depot);
    essai.remarques(1, "pas des remarques\n");
    essai.remarques(2, "remarques 3\n");
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, Some(&gh)), ExitCode::FAILURE);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r] = &ouvertes[..] else { panic!("la vague, le relecteur : {ouvertes:?}") };
    // Deux fois rouge, du refus 7015 que la relance reçoit en contexte ; puis classée.
    let fins = fins_de(&fil, r);
    assert_eq!(fins.iter().map(|(couleur, _)| *couleur).collect::<Vec<_>>(), [Couleur::Rouge, Couleur::Rouge]);
    let regle = Code::RemarquesIllisibles.regle();
    for (_, texte) in &fins {
        assert!(texte.starts_with(&format!("7015 vague.remarques_illisibles : {regle}\nligne 1 « ")), "{texte}");
    }
    let seconde = essai.garde("consigne-relecteur-2.txt").unwrap();
    assert!(
        seconde.contains(&format!("Tentative précédente échouée :\n{}", fins[0].1.replace("\r\n", "\n"))),
        "{seconde}"
    );
    let classee =
        fil.iter().any(|e| matches!(e, Evenement::Verdict { vague, verdict: Verdict::Classee, .. } if vague == r));
    assert!(classee);
    // Le refus 7016 est noté sur la vague relue ; ni relecture ni correction au fil ; rien ne se pousse.
    let refus = refus_de_forge(&fil, v);
    let [(7016, preuve)] = &refus[..] else { panic!("{refus:?}") };
    assert!(preuve.contains(&format!("la relecture de la vague {v} est classée : main-{r}-1 : 7015 ")), "{preuve}");
    assert!(!fil.iter().any(|e| matches!(e, Evenement::Relecture { .. } | Evenement::Correction { .. })));
    // Le lien du relecteur à la vague relue est au fil, avant le départ de sa main.
    let rang = |cherche: &dyn Fn(&Evenement) -> bool| fil.iter().position(cherche);
    let lien = rang(&|e| *e == Evenement::Relecteur { vague: r.clone(), relue: v.clone() });
    let depart = rang(&|e| matches!(e, Evenement::Debut { vague, .. } if vague == r));
    assert!(matches!((lien, depart), (Some(lien), Some(depart)) if lien < depart), "{lien:?} {depart:?}");
    rien_de_pousse(&essai);
    // Le bac du relecteur reste, avec ce qu'il a écrit ; le travail critique n'est pas relu : le code est 1.
    assert!(essai.bacs().contains(&format!("main-{r}-1")), "{:?}", essai.bacs());
    assert_eq!(etat_dans(&essai.kraken), Ok(ExitCode::FAILURE));
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn un_relecteur_qui_touche_un_fichier_est_7014_et_se_reprend_a_sa_relance() {
    // Il touche autre.txt à sa première tentative (7014), puis l'ôte et écrit ses remarques à la seconde.
    let essai = Essai::nouveau("relecture-7014");
    let mut racines = vec![essai.racine.clone()];
    essai.remarques(1, &remarques_justes());
    essai.remarques(2, SANS_REMARQUE);
    essai.marque("toucher-1");
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    assert_eq!(essai.mener(&plan, None), ExitCode::SUCCESS);
    let fil = essai.fil();
    let vagues = ouvertes(&fil);
    let [_, r] = &vagues[..] else { panic!("la vague et le relecteur, sans correction : {vagues:?}") };
    let fins = fins_de(&fil, r);
    assert_eq!(fins.iter().map(|(couleur, _)| *couleur).collect::<Vec<_>>(), [Couleur::Rouge, Couleur::Vert]);
    let debut = format!("7014 vague.bac_touche : {}\n", Code::BacTouche.regle());
    assert!(
        fins[0].1.starts_with(&debut) && fins[0].1.contains(r#"en plus de REMARQUES.lydite : "autre.txt""#),
        "{}",
        fins[0].1
    );
    let seconde = essai.garde("consigne-relecteur-2.txt").unwrap();
    assert!(seconde.contains("Tentative précédente échouée :\n7014 vague.bac_touche"), "{seconde}");
    // C'est la seconde tentative que le fil note : un fichier sans remarque, donc ni correction ni autre vague.
    assert!(fil.iter().any(|e| matches!(e, Evenement::Relecture { n: 0, bloquantes: 0, .. })));
    let _ = fs::remove_dir_all(&essai.racine);
    // Qu'il le touche aux deux tentatives, et la relecture est classée (7016).
    let essai = Essai::nouveau("relecture-7014-deux-fois");
    racines.push(essai.racine.clone());
    essai.remarques(1, &remarques_justes());
    essai.remarques(2, &remarques_justes());
    essai.marque("toucher-1");
    essai.marque("toucher-2");
    assert_eq!(essai.mener(&plan, None), ExitCode::FAILURE);
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v, r] = &ouvertes[..] else { panic!("la vague et le relecteur : {ouvertes:?}") };
    for (_, texte) in fins_de(&fil, r) {
        assert!(texte.starts_with("7014 vague.bac_touche : "), "{texte}");
    }
    let refus = refus_de_forge(&fil, v);
    assert_eq!(refus.iter().map(|(code, _)| *code).collect::<Vec<_>>(), [7016]);
    let _ = fs::remove_dir_all(&essai.racine);
    rien_ne_demeure(&racines);
}

/// Une vague du plan des essais, a et b assemblées, faite à la main : sa base est le `HEAD` du dépôt, et sa tête un
/// commit dont le diff sous `sortie/` (le travail de a, la seule critique) pèse tout juste `taille` octets. Rend la
/// vague et sa base.
fn vague_qui_pese(essai: &Essai, taille: usize) -> (Menee, String) {
    // Le dossier de Kraken, que la vague aurait créé.
    fs::create_dir_all(&essai.kraken).unwrap();
    let base = essai.base();
    let pese = |tete: &str| lire_le_diff(&essai.depot, &base, tete, Some(&["sortie/".to_string()])).unwrap().1;
    // 9 000 lignes de 100 octets, puis une ligne qui complète : l'en-tête du diff garde sa longueur.
    let lignes = format!("{}\n", "y".repeat(99)).repeat(9_000);
    let premier = commiter(&essai.depot, &[("sortie/a.txt", &lignes)]);
    // La ligne de plus pèse sa longueur, plus le `+` du diff.
    let reste = taille - pese(&premier) as usize - 1;
    let complet = format!("{lignes}{}\n", "z".repeat(reste - 1));
    let tete = commiter(&essai.depot, &[("sortie/a.txt", &complet)]);
    assert_eq!(pese(&tete), taille as u64, "le diff pèse ce que l'essai veut");
    let pris = || Ok(Pris { sortie: String::new(), prouve: true });
    let id = crate::coeur::fil::lot();
    let issues = vec![(format!("main-{id}-1"), pris()), (format!("main-{id}-2"), pris())];
    (Menee { id, branche: None, tete: Some(tete), arretee: false, issues }, base)
}

/// La relecture de cette vague du plan des essais, sous cette origine.
fn relire_sous(essai: &Essai, menee: &Menee, base: &str, origine: Origine) -> Relue {
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    let relire = if origine == Origine::Issue { relire_issue } else { relire };
    relire(&essai.depot, &essai.kraken, &essai.profil, &plan, menee, base, DELAI)
}

#[test]
fn sous_une_issue_un_diff_de_plus_de_1_mo_est_7016_sans_relecteur() {
    let essai = Essai::nouveau("relecture-issue-trop");
    essai.remarques(1, SANS_REMARQUE);
    let (menee, base) = vague_qui_pese(&essai, TAILLE_MAX + 1);
    let Relue::Dehors(refus) = relire_sous(&essai, &menee, &base, Origine::Issue) else {
        panic!("un diff coupé ne se relit pas sous une issue")
    };
    assert_eq!(refus.code, Code::RelectureManquee, "{refus}");
    let attendu = format!("son diff pèse {} octets, plus que 1 Mo", TAILLE_MAX + 1);
    assert!(refus.preuve.contains(&attendu), "{refus}");
    // Aucun relecteur n'est parti : ni sa main, ni son bac, ni sa vague au fil ; le refus y est noté.
    assert!(essai.garde("relecture-1").is_none(), "aucun relecteur n'est parti");
    assert!(essai.bacs().is_empty(), "{:?}", essai.bacs());
    let fil = essai.fil();
    assert!(ouvertes(&fil).is_empty(), "{fil:?}");
    assert_eq!(refus_de_forge(&fil, &menee.id).iter().map(|(code, _)| *code).collect::<Vec<_>>(), [7016]);
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn sous_une_issue_un_diff_juste_sous_la_borne_se_relit_en_entier() {
    let essai = Essai::nouveau("relecture-issue-sous");
    essai.remarques(1, SANS_REMARQUE);
    let (menee, base) = vague_qui_pese(&essai, TAILLE_MAX - 1);
    let Relue::Prise(Prise { n: 0, bloquantes: 0, correction: None, .. }) =
        relire_sous(&essai, &menee, &base, Origine::Issue)
    else {
        panic!("un diff sous la borne se relit")
    };
    // Le relecteur a vu le diff entier, sans mention de coupe.
    let diff = essai.garde("diff-1.txt").expect("le relecteur a vu DIFF.lydite");
    assert_eq!(diff.len(), TAILLE_MAX - 1);
    assert!(!diff.contains("[diff coupé par Lydite"), "le diff n'est pas coupé");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn hors_issue_un_diff_de_plus_de_1_mo_se_relit_coupe_comme_avant() {
    let essai = Essai::nouveau("relecture-hors-issue");
    essai.remarques(1, SANS_REMARQUE);
    let (menee, base) = vague_qui_pese(&essai, TAILLE_MAX + 1);
    let Relue::Prise(Prise { n: 0, .. }) = relire_sous(&essai, &menee, &base, Origine::Plan) else {
        panic!("hors issue, un diff coupé se relit")
    };
    let diff = essai.garde("diff-1.txt").expect("le relecteur a vu DIFF.lydite");
    assert!(diff.len() <= TAILLE_MAX, "{} octets", diff.len());
    assert!(diff.lines().last().unwrap().starts_with("[diff coupé par Lydite : "), "la dernière ligne le dit");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn sous_une_issue_une_vague_lancee_d_un_sous_dossier_pese_son_diff_depuis_la_racine_7016() {
    let essai = Essai::nouveau("relecture-issue-sous-dossier");
    essai.remarques(1, SANS_REMARQUE);
    // La tâche critique écrit sous `sortie/` plus de 1 Mo ; Kraken part d'un sous-dossier du dépôt visé, comme
    // `lancer` sans `--depot`, d'où `sortie/` ne nommerait rien.
    fs::write(essai.faux.join("gros.txt"), format!("{}\n", "y".repeat(99)).repeat(TAILLE_MAX / 100 + 1)).unwrap();
    let sous = essai.depot.join("sous");
    fs::create_dir(&sous).unwrap();
    let programme = OsString::from("gh-absent");
    let lieu = Lieu {
        profil: &essai.profil,
        depot: &sous,
        dossier: &essai.kraken,
        gh: &programme,
        personnel: Some(&essai.racine),
        ci: (Duration::from_millis(10), Duration::from_secs(60)),
    };
    let plan = Plan::lire(PLAN_DE_VAGUE.as_bytes()).unwrap();
    let base = base_du_depot(&sous).unwrap();
    let code = mener_sur(&lieu, &plan, &base, Liens::default(), None, Origine::Issue).unwrap();
    assert_eq!(code, ExitCode::FAILURE);
    // Une sortie qui ne publie rien scelle quand même le fil, une fois, à sa fin.
    let sceaux = crate::branchements::dossier::lire_sceaux(&essai.kraken);
    assert_eq!(sceaux.len(), 1, "{sceaux:?}");
    assert_eq!(etat_dans(&essai.kraken), Ok(ExitCode::FAILURE), "le sceau tient");
    // Le diff coupé n'est pas allé au relecteur : 7016, et aucun relecteur n'est parti.
    assert!(essai.garde("relecture-1").is_none(), "aucun relecteur n'est parti");
    let fil = essai.fil();
    let ouvertes = ouvertes(&fil);
    let [v] = &ouvertes[..] else { panic!("la vague seule : {ouvertes:?}") };
    let refus = refus_de_forge(&fil, v);
    assert_eq!(refus.iter().map(|(code, _)| *code).collect::<Vec<_>>(), [7016], "{refus:?}");
    assert!(refus[0].1.contains("plus que 1 Mo"), "{refus:?}");
    let _ = fs::remove_dir_all(&essai.racine);
}

#[test]
fn la_consigne_porte_les_points_a_justifier_apres_les_taches_critiques_vingt_au_plus() {
    let libre = Tache { id: "d".into(), objectif: "Libre".into(), critique: true, ..Tache::default() };
    let sans = consigne("o", &[&libre], &[]);
    let phrase = "Lydite found these points without a model, in the diff you review. For each one, say in a remark \
                  whether it is justified:\n\n";
    assert!(!sans.contains("Lydite found"), "{sans}");
    let points: Vec<String> = (1..=22).map(|k| format!("`f{k}`: test modified")).collect();
    let avec = consigne("o", &[&libre], &points);
    let liste: String = points[..20].iter().map(|point| format!("- {point}\n")).collect();
    // Octet pour octet : la consigne sans point, avec la phrase et les points avant « Le travail à relire ».
    let (avant, apres) = sans.split_once("    Libre\n\nLe travail").unwrap();
    assert_eq!(avec, format!("{avant}    Libre\n\n{phrase}{liste}\nLe travail{apres}"));
    assert!(!avec.contains("f21") && !avec.contains("f22"), "{avec}");
}

#[test]
fn la_borne_de_vingt_points_tient_sur_ceux_d_un_vrai_diff() {
    let mut diff = String::from("--- a/src/s.rs\n+++ b/src/s.rs\n@@ -1,25 +0,0 @@\n");
    for k in 1..=25 {
        diff.push_str(&format!("-assert!(a{k});\n"));
    }
    let tous = crate::conduite::alarmes::points(&diff);
    assert_eq!(tous.len(), 25);
    let consigne = consigne("o", &[], &tous);
    assert!(consigne.contains("`src/s.rs`, line 20: an assertion removed or changed"), "{consigne}");
    assert!(!consigne.contains("line 21:"), "{consigne}");
}

#[test]
fn la_couche_fixe_du_relecteur_tient_sous_son_plafond_sans_la_sortie_honnete_ni_le_perimetre() {
    // Mesurée avec la phrase des points à justifier, qui n'entre qu'avec un point.
    let fixe = consigne("", &[], &["p".to_string()]);
    assert!(fixe.chars().count() <= PLAFOND, "{} > {PLAFOND}", fixe.chars().count());
    assert_eq!(crate::essais::codes_inconnus(&fixe), Vec::<String>::new());
    assert!(!fixe.contains("IMPOSSIBLE") && !fixe.contains("Your files") && !fixe.contains("Change only"), "{fixe}");
}

// (la suite des essais)
