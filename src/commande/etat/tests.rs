use super::*;
use crate::branchements::dossier::Fil;
use crate::coeur::erreur::Code;
use crate::coeur::fil::{self, Couleur, lot};
use crate::conduite::mener::{annonce, toutes_constatees};
use crate::conduite::tentative::Pris;
use crate::essais::{MAINTENANT, V1, lacher, ouverture};
use std::env;
use std::fs;

const V2: &str = "1761901200000_42_0";

fn debut(vague: &str, main: usize, tentative: usize) -> Evenement {
    Evenement::Debut { vague: vague.into(), main, tentative }
}

fn fin(vague: &str, main: usize, tentative: usize, (couleur, foi): (Couleur, Foi), texte: &str) -> Evenement {
    Evenement::Fin { vague: vague.into(), main, tentative, couleur, foi, texte: texte.into(), verif: None }
}

fn verdict(vague: &str, main: usize, (verdict, foi): (Verdict, Foi)) -> Evenement {
    Evenement::Verdict { vague: vague.into(), main, verdict, foi }
}

/// Le lancement d'une tentative, d'un profil qui exige ou demande le mur.
fn lancement(vague: &str, main: usize, tentative: usize, mur: fil::Mur) -> Evenement {
    Evenement::Lancement {
        vague: vague.into(),
        main,
        tentative,
        profil: "claude".into(),
        empreinte: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
        modele: "sonnet".into(),
        effort: None,
        mur,
        reglages: "{}".into(),
        reglages_depot: String::new(),
    }
}

/// Le constat d'une tentative : le mur actif ou non, `None` si Claude Code ne l'a pas dit.
fn constat(vague: &str, main: usize, tentative: usize, mur_actif: Option<bool>) -> Evenement {
    Evenement::Constat { vague: vague.into(), main, tentative, version: Some("2.1.312".into()), mur_actif }
}

/// Ce que Kraken écrit : rouge et classée sont constatées ; verte et prise, constatées ou déclarées.
const ROUGE: (Couleur, Foi) = (Couleur::Rouge, Foi::Constate);
const VERTE: (Couleur, Foi) = (Couleur::Vert, Foi::Constate);
const VERTE_DECLAREE: (Couleur, Foi) = (Couleur::Vert, Foi::Declare);
const PRISE: (Verdict, Foi) = (Verdict::Prise, Foi::Constate);
const PRISE_DECLAREE: (Verdict, Foi) = (Verdict::Prise, Foi::Declare);
const CLASSEE: (Verdict, Foi) = (Verdict::Classee, Foi::Constate);

/// Deux vagues. L'ancienne a une main classée ; la dernière en a cinq : une prise, une prise déclarée, une classée
/// après deux échecs (son erreur tient sur plusieurs lignes), une en cours à sa tentative 2, et une pas commencée.
fn exemple() -> Vec<Evenement> {
    vec![
        ouverture(V1, 1, "Ancienne vague"),
        debut(V1, 1, 1),
        fin(V1, 1, 1, ROUGE, "lancement impossible de claude : introuvable"),
        verdict(V1, 1, CLASSEE),
        ouverture(V2, 5, "Écrire bonjour.txt"),
        debut(V2, 1, 1),
        fin(V2, 1, 1, VERTE, "bonjour.txt écrit"),
        verdict(V2, 1, PRISE),
        debut(V2, 2, 1),
        fin(V2, 2, 1, VERTE_DECLAREE, "bonjour.txt écrit\nPREUVE: vert"),
        verdict(V2, 2, PRISE_DECLAREE),
        debut(V2, 3, 1),
        fin(V2, 3, 1, ROUGE, "premier échec"),
        debut(V2, 3, 2),
        fin(
            V2,
            3,
            2,
            ROUGE,
            concat!(
                "vérification « cargo test » rouge (exit code: 101) :\n",
                "test bonjour ... ok\n",
                "test salut ... FAILED\n\n",
            ),
        ),
        verdict(V2, 3, CLASSEE),
        debut(V2, 4, 1),
        fin(V2, 4, 1, ROUGE, "premier échec"),
        debut(V2, 4, 2),
    ]
}

#[test]
fn kraken_etat_affiche_la_derniere_vague() {
    let attendu = format!(
        "Écrire bonjour.txt\n\
         main-{V2}-1 : prise\n\
         main-{V2}-2 : prise déclarée, non prouvée\n\
         main-{V2}-3 : classée : test salut ... FAILED\n\
         main-{V2}-4 : en cours, tentative 2\n\
         main-{V2}-5 : en attente\n\
         Coût : inconnu · 6 mains, 2 relances, 1 classée\n\
         Effort: Claude Code's default"
    );
    assert_eq!(rendu(&etat(&exemple()), MAINTENANT), attendu);
}

/// Deux échéances de bail, l'une passée à cette heure (sans décimale : le rejeu la range à la milliseconde), l'autre à
/// venir.
const PASSEE: &str = "2026-10-31T21:59:00Z";
const A_VENIR: &str = "2026-10-31T22:30:00.5Z";

/// Le sha d'où partent les vagues à plan de ces tests, et que leurs fusions notent.
const BASE: &str = "9f2c41a7d3b85e60c1a49d27b8e3f5a60d1c2b34";

fn assemblage(vague: &str) -> Evenement {
    let (branche, base, preuve) = (format!("lydite/vague-{vague}"), BASE.into(), "cargo test".into());
    Evenement::Assemblage { vague: vague.into(), branche, base, preuve }
}

fn tache(vague: &str, main: usize, id: &str, depend: &[&str]) -> Evenement {
    let depend = depend.iter().map(|id| id.to_string()).collect();
    let (preuve, objectif) = ("cargo test".into(), format!("Tâche {id}"));
    Evenement::Tache { vague: vague.into(), main, id: id.into(), depend, preuve, objectif }
}

fn bail(vague: &str, main: usize, tentative: usize, echeance: &str) -> Evenement {
    Evenement::Bail { vague: vague.into(), main, tentative, echeance: echeance.into() }
}

fn fusion(vague: &str, main: usize) -> Evenement {
    Evenement::Fusion { vague: vague.into(), main, sha: BASE.into(), verif: "vert".into() }
}

/// Le refus de la tâche par ce code, avec la règle du code.
fn refus(vague: &str, main: usize, code: Code) -> Evenement {
    let (regle, preuve, correction) = (code.regle().into(), format!("preuve {main}"), format!("correction {main}"));
    Evenement::Refus { vague: vague.into(), main, code: code.numero(), regle, preuve, correction, verif: None }
}

/// Ce que le fil note d'une main prise du premier coup : son bail, son début, sa fin verte et son verdict.
fn prise_de(vague: &str, main: usize) -> Vec<Evenement> {
    let verte = fin(vague, main, 1, VERTE, "fait");
    vec![bail(vague, main, 1, A_VENIR), debut(vague, main, 1), verte, verdict(vague, main, PRISE)]
}

/// Les notes d'une vague de deux mains, sous l'id de cette vague : ce qu'un test écrit au fil.
type Notes = fn(&str) -> Vec<Evenement>;

/// Ce que `lydite etat` dit de la tâche `t`, seule de la vague à plan V1, après ces événements.
fn etat_de_t(evenements: &[Evenement]) -> String {
    let tete = [ouverture(V1, 1, "o"), assemblage(V1), tache(V1, 1, "t", &[])];
    let texte = rendu(&etat(&[&tete[..], evenements].concat()), MAINTENANT);
    let prefixe = format!("main-{V1}-1 t : ");
    let ligne = texte.lines().find_map(|ligne| ligne.strip_prefix(&prefixe));
    ligne.unwrap_or_else(|| panic!("{texte}")).to_string()
}

#[test]
fn kraken_etat_affiche_une_vague_a_plan() {
    let rouge = concat!(
        "vérification « cargo test » rouge (exit code: 101) :\n",
        "test bonjour ... ok\n",
        "test salut ... FAILED\n\n",
    );
    let tache_rouge_deux_fois = vec![
        bail(V2, 3, 1, A_VENIR),
        debut(V2, 3, 1),
        fin(V2, 3, 1, ROUGE, "premier échec"),
        bail(V2, 3, 2, A_VENIR),
        debut(V2, 3, 2),
        fin(V2, 3, 2, ROUGE, rouge),
        verdict(V2, 3, CLASSEE),
    ];
    let tache_relancee = vec![
        bail(V2, 7, 1, A_VENIR),
        debut(V2, 7, 1),
        fin(V2, 7, 1, ROUGE, "premier échec"),
        bail(V2, 7, 2, A_VENIR),
        debut(V2, 7, 2),
    ];
    let evenements = [
        vec![
            ouverture(V2, 10, "Dix tâches"),
            assemblage(V2),
            tache(V2, 1, "a", &[]),
            tache(V2, 2, "b", &["a"]),
            tache(V2, 3, "c", &[]),
            tache(V2, 4, "d", &["c"]),
            tache(V2, 5, "e", &[]),
            tache(V2, 6, "f", &[]),
            tache(V2, 7, "g", &[]),
            tache(V2, 8, "h", &["a", "c"]),
            tache(V2, 9, "i", &["a"]),
            tache(V2, 10, "j", &["a", "f", "e"]),
        ],
        // a : prise, puis assemblée.
        prise_de(V2, 1),
        vec![fusion(V2, 1)],
        // b : prise, puis refusée à l'assemblage (un conflit) : le refus l'emporte sur son verdict.
        prise_de(V2, 2),
        vec![refus(V2, 2, Code::Conflit)],
        // c : rouge deux fois, classée.
        tache_rouge_deux_fois,
        // d : sautée, sans bac ni main, car c n'est pas assemblée.
        vec![refus(V2, 4, Code::Sautee)],
        // e : prise, elle attend son assemblage.
        prise_de(V2, 5),
        // f : sa main n'a rien rendu, et l'échéance de son bail est passée.
        vec![bail(V2, 6, 1, PASSEE), debut(V2, 6, 1)],
        // g : relancée, sa seconde tentative court dans son bail.
        tache_relancee,
        // h, i et j n'ont pas commencé : h attend c, les dépendances de i sont assemblées, j attend f et e.
    ]
    .concat();
    let attendu = [
        "Dix tâches".to_string(),
        format!("branche : lydite/vague-{V2} · preuve : cargo test"),
        format!("main-{V2}-1 a : assemblée"),
        format!("main-{V2}-2 b : refusée (7007) : {}", Code::Conflit.regle()),
        format!("main-{V2}-3 c : classée : test salut ... FAILED"),
        format!("main-{V2}-4 d : refusée (7011) : {}", Code::Sautee.regle()),
        format!("main-{V2}-5 e : prise, en attente d'assemblage"),
        format!("main-{V2}-6 f : sans nouvelles depuis 2026-10-31T21:59:00.000Z (déduit)"),
        format!("main-{V2}-7 g : en cours, tentative 2"),
        format!("main-{V2}-8 h : en attente de c"),
        format!("main-{V2}-9 i : en attente"),
        format!("main-{V2}-10 j : en attente de f, e"),
        "Coût : inconnu · 8 mains, 2 relances, 1 classée, 2 refusées".to_string(),
        "Effort: Claude Code's default".to_string(),
    ];
    assert_eq!(rendu(&etat(&evenements), MAINTENANT), attendu.join("\n"));
    // Une même tâche remplit parfois plusieurs états à la fois : le premier de la liste l'emporte, et de la
    // fusion et du refus, le dernier noté.
    let refusee = |code: Code| format!("refusée ({}) : {}", code.numero(), code.regle());
    let classee =
        vec![bail(V1, 1, 1, A_VENIR), debut(V1, 1, 1), fin(V1, 1, 1, ROUGE, "rouge"), verdict(V1, 1, CLASSEE)];
    let sans_fin_et_passee = vec![bail(V1, 1, 1, PASSEE), debut(V1, 1, 1)];
    let cas: Vec<(&str, Vec<Evenement>, String)> = vec![
        ("la fusion l'emporte sur la prise", [prise_de(V1, 1), vec![fusion(V1, 1)]].concat(), "assemblée".into()),
        (
            "le refus l'emporte sur la prise",
            [prise_de(V1, 1), vec![refus(V1, 1, Code::Conflit)]].concat(),
            refusee(Code::Conflit),
        ),
        (
            "le refus l'emporte sur la classée",
            [classee, vec![refus(V1, 1, Code::PreuveRouge)]].concat(),
            refusee(Code::PreuveRouge),
        ),
        (
            "le refus l'emporte sur le silence",
            [sans_fin_et_passee.clone(), vec![refus(V1, 1, Code::MainExpiree)]].concat(),
            refusee(Code::MainExpiree),
        ),
        (
            "le verdict l'emporte sur le silence",
            [sans_fin_et_passee, vec![verdict(V1, 1, PRISE)]].concat(),
            "prise, en attente d'assemblage".into(),
        ),
        (
            "le dernier mot est au refus",
            [prise_de(V1, 1), vec![fusion(V1, 1), refus(V1, 1, Code::GitImpossible)]].concat(),
            refusee(Code::GitImpossible),
        ),
        (
            "le dernier mot est à la fusion",
            [prise_de(V1, 1), vec![refus(V1, 1, Code::GitImpossible), fusion(V1, 1)]].concat(),
            "assemblée".into(),
        ),
        (
            "un verdict déclaré porte son mot",
            vec![
                bail(V1, 1, 1, A_VENIR),
                debut(V1, 1, 1),
                fin(V1, 1, 1, VERTE_DECLAREE, "fait\nPREUVE: vert"),
                verdict(V1, 1, PRISE_DECLAREE),
            ],
            "prise déclarée, non prouvée, en attente d'assemblage".into(),
        ),
        ("rien n'a commencé", vec![], "en attente".into()),
    ];
    for (quoi, evenements, attendu) in cas {
        assert_eq!(etat_de_t(&evenements), attendu, "{quoi}");
    }
}

#[test]
fn une_main_sans_nouvelles_n_est_jamais_dite_expiree() {
    // L'heure est 22:00:00.000. Une tentative sans fin dont l'échéance est atteinte : Kraken ne sait rien de sa
    // main, il a pu s'arrêter. Il le déduit, le dit, et ne la dit pas expirée. L'échéance se dit à la milliseconde.
    let tentative = |echeance: &str| vec![bail(V1, 1, 1, echeance), debut(V1, 1, 1)];
    let sans_nouvelles = |depuis: &str| format!("sans nouvelles depuis {depuis} (déduit)");
    let cas = [
        // Passée d'une milliseconde, d'une seconde, d'un jour.
        ("2026-10-31T21:59:59.999Z", sans_nouvelles("2026-10-31T21:59:59.999Z")),
        ("2026-10-31T21:59:59Z", sans_nouvelles("2026-10-31T21:59:59.000Z")),
        ("2026-10-30T22:00:00.000Z", sans_nouvelles("2026-10-30T22:00:00.000Z")),
        // Atteinte à l'instant même, avec ou sans décimale : comme pour le bail, une échéance atteinte est échue.
        ("2026-10-31T22:00:00.000Z", sans_nouvelles("2026-10-31T22:00:00.000Z")),
        ("2026-10-31T22:00:00Z", sans_nouvelles("2026-10-31T22:00:00.000Z")),
        // À venir d'une milliseconde, d'une demi-seconde, d'une seconde.
        ("2026-10-31T22:00:00.001Z", "en cours, tentative 1".to_string()),
        ("2026-10-31T22:00:00.5Z", "en cours, tentative 1".to_string()),
        ("2026-10-31T22:00:01Z", "en cours, tentative 1".to_string()),
    ];
    for (echeance, attendu) in cas {
        let dit = etat_de_t(&tentative(echeance));
        assert_eq!(dit, attendu, "échéance {echeance}");
        assert!(!dit.contains("expir"), "une main sans nouvelles n'est pas dite expirée : {dit}");
    }
    // Seule la dernière tentative compte, avec sa propre échéance : la première a fini rouge, la seconde court.
    let relancee = |echeance: &str| {
        let premiere = [bail(V1, 1, 1, PASSEE), debut(V1, 1, 1), fin(V1, 1, 1, ROUGE, "rouge")];
        [&premiere[..], &[bail(V1, 1, 2, echeance), debut(V1, 1, 2)]].concat()
    };
    assert_eq!(etat_de_t(&relancee(A_VENIR)), "en cours, tentative 2");
    assert_eq!(etat_de_t(&relancee("2026-10-31T21:59:59.999Z")), sans_nouvelles("2026-10-31T21:59:59.999Z"));
    // Une tentative qui a fini n'est pas sans nouvelles, son échéance fût-elle passée ; sans bail, rien à déduire.
    let finie = [bail(V1, 1, 1, PASSEE), debut(V1, 1, 1), fin(V1, 1, 1, VERTE, "fait")];
    assert_eq!(etat_de_t(&finie), "en cours, tentative 1");
    assert_eq!(etat_de_t(&[debut(V1, 1, 1)]), "en cours, tentative 1");
    // Quand la vague a expiré la main elle-même, elle l'a noté : c'est son refus (7004), pas une déduction.
    let expiree = etat_de_t(&[bail(V1, 1, 1, PASSEE), debut(V1, 1, 1), refus(V1, 1, Code::MainExpiree)]);
    assert_eq!(expiree, format!("refusée (7004) : {}", Code::MainExpiree.regle()));
}

#[test]
fn des_copies_refusees_ou_sans_nouvelles_le_disent() {
    let evenements = [
        vec![ouverture(V1, 6, "Écrire bonjour.txt")],
        // 1 : sa main n'a rien rendu, et l'échéance de son bail est passée.
        vec![bail(V1, 1, 1, PASSEE), debut(V1, 1, 1)],
        // 2 : sa main est tuée à son délai, refusée (7005) : le refus l'emporte sur le verdict classé, que le fil
        // porte aussi.
        vec![
            bail(V1, 2, 1, A_VENIR),
            debut(V1, 2, 1),
            fin(V1, 2, 1, ROUGE, "main : temps dépassé (3600 s)"),
            refus(V1, 2, Code::MainTuee),
            verdict(V1, 2, CLASSEE),
        ],
        // 3 : en cours, dans son bail.
        vec![bail(V1, 3, 1, A_VENIR), debut(V1, 3, 1)],
        // 4 : prise et constatée.
        prise_de(V1, 4),
        // 5 : prise, qu'une fusion venue d'ailleurs ne dit pas assemblée : des copies n'ont pas d'assemblage.
        [prise_de(V1, 5), vec![fusion(V1, 5)]].concat(),
        // 6 : pas commencée.
    ]
    .concat();
    let attendu = [
        "Écrire bonjour.txt".to_string(),
        format!("main-{V1}-1 : sans nouvelles depuis 2026-10-31T21:59:00.000Z (déduit)"),
        format!("main-{V1}-2 : refusée (7005) : {}", Code::MainTuee.regle()),
        format!("main-{V1}-3 : en cours, tentative 1"),
        format!("main-{V1}-4 : prise"),
        format!("main-{V1}-5 : prise"),
        format!("main-{V1}-6 : en attente"),
        "Coût : inconnu · 5 mains, 0 relance, 1 classée, 1 refusée".to_string(),
        "Effort: Claude Code's default".to_string(),
    ];
    assert_eq!(rendu(&etat(&evenements), MAINTENANT), attendu.join("\n"));
    // Leur code reste celui des prises constatées : deux copies prises donnent 0, une copie refusée donne 1.
    let dossier = env::temp_dir().join(format!("lydite-test-copies-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    let cas: [(&str, Notes, ExitCode); 2] = [
        ("deux copies prises", |v| [prise_de(v, 1), prise_de(v, 2)].concat(), ExitCode::SUCCESS),
        ("une copie refusée", |v| [prise_de(v, 1), vec![refus(v, 2, Code::MainTuee)]].concat(), ExitCode::FAILURE),
    ];
    for (i, (quoi, notes, attendu)) in cas.into_iter().enumerate() {
        let vague = format!("17619013000{i}_7_0");
        for evenement in [vec![ouverture(&vague, 2, quoi)], notes(&vague)].concat() {
            fil.noter(&evenement).unwrap();
        }
        assert_eq!(etat_dans(&dossier), Ok(attendu), "{quoi}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn toutes_assemblees_si_chaque_tache_l_est() {
    let derniere = |evenements: Vec<Evenement>| etat(&evenements).pop().unwrap();
    let plan =
        |n: usize, suite: Vec<Evenement>| derniere([vec![ouverture(V1, n, "o"), assemblage(V1)], suite].concat());
    let assemblees = plan(2, vec![fusion(V1, 1), fusion(V1, 2)]);
    assert!(toutes_assemblees(&assemblees) && menee_a_bout(&assemblees));
    // Une prise n'est pas une assemblée : elle attend son assemblage, et le code de la vague reste à 1 alors que
    // ses mains sont toutes prises et constatées.
    let prises = plan(2, [prise_de(V1, 1), vec![fusion(V1, 1)], prise_de(V1, 2)].concat());
    assert!(toutes_prises(&prises) && !toutes_assemblees(&prises) && !menee_a_bout(&prises));
    // Une refusée, une refusée après avoir été assemblée, une tâche absente (la vague en compte trois), une tâche
    // de trop, une vague sans ouverture, une vague sans tâche.
    let refusee = plan(2, vec![fusion(V1, 1), refus(V1, 2, Code::Conflit)]);
    let defaite = plan(2, vec![fusion(V1, 1), fusion(V1, 2), refus(V1, 2, Code::GitImpossible)]);
    let absente = plan(3, vec![fusion(V1, 1), fusion(V1, 2)]);
    let de_trop = plan(2, vec![fusion(V1, 1), fusion(V1, 2), fusion(V1, 3)]);
    let sans_ouverture = derniere(vec![fusion(V1, 1)]);
    let sans_tache = derniere(vec![assemblage(V1)]);
    for (quoi, vague) in [
        ("refusée", refusee),
        ("défaite", defaite),
        ("absente", absente),
        ("de trop", de_trop),
        ("sans ouverture", sans_ouverture),
        ("sans tâche", sans_tache),
    ] {
        assert!(!toutes_assemblees(&vague) && !menee_a_bout(&vague), "{quoi}");
    }
    // Des copies gardent le code des prises constatées.
    let copies = derniere(vec![ouverture(V1, 2, "o"), verdict(V1, 1, PRISE), verdict(V1, 2, PRISE)]);
    assert!(menee_a_bout(&copies) && !toutes_assemblees(&copies));
    // `lydite etat` ne sort en 0 que sur une vague à plan toute assemblée : cinq vagues de deux tâches, ajoutées au
    // même fil, dont seule la dernière compte.
    let dossier = env::temp_dir().join(format!("lydite-test-assemblees-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    let cas: [(&str, Notes, ExitCode); 5] = [
        ("deux tâches assemblées", |v| vec![fusion(v, 1), fusion(v, 2)], ExitCode::SUCCESS),
        ("une prise non assemblée", |v| [vec![fusion(v, 1)], prise_de(v, 2)].concat(), ExitCode::FAILURE),
        ("une refusée", |v| vec![fusion(v, 1), refus(v, 2, Code::Conflit)], ExitCode::FAILURE),
        ("une classée", |v| vec![fusion(v, 1), verdict(v, 2, CLASSEE)], ExitCode::FAILURE),
        ("deux tâches assemblées, de nouveau", |v| vec![fusion(v, 1), fusion(v, 2)], ExitCode::SUCCESS),
    ];
    for (i, (quoi, notes, attendu)) in cas.into_iter().enumerate() {
        let vague = format!("17619013000{i}_7_0");
        let ouverte = vec![ouverture(&vague, 2, quoi), assemblage(&vague)];
        let taches = vec![tache(&vague, 1, "a", &[]), tache(&vague, 2, "b", &["a"])];
        for evenement in [ouverte, taches, notes(&vague)].concat() {
            fil.noter(&evenement).unwrap();
        }
        assert_eq!(etat_dans(&dossier), Ok(attendu), "{quoi}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn kraken_etat_dit_le_mur() {
    use crate::coeur::fil::Mur::{Demande, Exige};
    // Une vague de sept mains, chacune d'un cas : le mot du mur finit la ligne de la main, d'après sa dernière
    // tentative.
    let evenements = vec![
        ouverture(V2, 7, "Écrire bonjour.txt"),
        // 1 : Claude Code dit son mur actif. Constaté, quoi que le profil ait demandé.
        debut(V2, 1, 1),
        lancement(V2, 1, 1, Exige),
        constat(V2, 1, 1, Some(true)),
        fin(V2, 1, 1, VERTE, "bonjour.txt écrit"),
        verdict(V2, 1, PRISE),
        // 2 : Claude Code dit son mur inactif. Constaté aussi, même si le profil l'exigeait.
        debut(V2, 2, 1),
        lancement(V2, 2, 1, Exige),
        constat(V2, 2, 1, Some(false)),
        fin(V2, 2, 1, VERTE, "bonjour.txt écrit"),
        verdict(V2, 2, PRISE),
        // 3 : un constat qui ne dit rien du mur laisse le lancement : exigé, déclaré.
        debut(V2, 3, 1),
        lancement(V2, 3, 1, Exige),
        constat(V2, 3, 1, None),
        fin(V2, 3, 1, ROUGE, "main : réglages non pris"),
        verdict(V2, 3, CLASSEE),
        // 4 : pas de constat (la main est encore en route) : demandé, non exigé, déclaré.
        debut(V2, 4, 1),
        lancement(V2, 4, 1, Demande),
        // 5 : un fil d'avant M2, sans lancement ni constat : rien.
        debut(V2, 5, 1),
        fin(V2, 5, 1, VERTE, "bonjour.txt écrit"),
        verdict(V2, 5, PRISE),
        // 6 : la dernière tentative l'emporte. La première avait un mur actif constaté, la relance seulement le
        // lancement : demandé, non exigé.
        debut(V2, 6, 1),
        lancement(V2, 6, 1, Exige),
        constat(V2, 6, 1, Some(true)),
        fin(V2, 6, 1, ROUGE, "premier échec"),
        debut(V2, 6, 2),
        lancement(V2, 6, 2, Demande),
        // 7 : une relance sans lancement ni constat n'a pas de mot, même si sa première tentative en avait un.
        debut(V2, 7, 1),
        lancement(V2, 7, 1, Exige),
        constat(V2, 7, 1, Some(true)),
        fin(V2, 7, 1, ROUGE, "premier échec"),
        debut(V2, 7, 2),
    ];
    let attendu = format!(
        "Écrire bonjour.txt\n\
         main-{V2}-1 : prise · mur actif (constaté)\n\
         main-{V2}-2 : prise · mur inactif (constaté)\n\
         main-{V2}-3 : classée : main : réglages non pris · mur exigé (déclaré)\n\
         main-{V2}-4 : en cours, tentative 1 · mur demandé, non exigé (déclaré)\n\
         main-{V2}-5 : prise\n\
         main-{V2}-6 : en cours, tentative 2 · mur demandé, non exigé (déclaré)\n\
         main-{V2}-7 : en cours, tentative 2\n\
         Coût : inconnu · 9 mains, 2 relances, 1 classée\n\
         Effort: Claude Code's default"
    );
    let vagues = etat(&evenements);
    assert_eq!(rendu(&vagues, MAINTENANT), attendu);
    // La fonction à part, pour qui rend d'autres lignes : rien sans main, et un constat sans lancement se dit.
    let main = |n| vagues[0].mains.get(&n);
    assert_eq!(mot_du_mur(None), "");
    assert_eq!(mot_du_mur(main(1)), " · mur actif (constaté)");
    assert_eq!(mot_du_mur(main(5)), "");
    let seul = etat(&[constat(V1, 1, 1, Some(false))]);
    assert_eq!(mot_du_mur(seul[0].mains.get(&1)), " · mur inactif (constaté)");
    // La ligne d'une tâche de vague à plan finit par le même mot, par la même fonction : son état d'abord.
    let lancee = [bail(V1, 1, 1, A_VENIR), debut(V1, 1, 1), lancement(V1, 1, 1, Exige)];
    assert_eq!(etat_de_t(&lancee), "en cours, tentative 1 · mur exigé (déclaré)");
    let assemblee = [constat(V1, 1, 1, Some(true)), fin(V1, 1, 1, VERTE, "fait"), verdict(V1, 1, PRISE)];
    let assemblee = [&lancee[..], &assemblee[..], &[fusion(V1, 1)]].concat();
    assert_eq!(etat_de_t(&assemblee), "assemblée · mur actif (constaté)");
}

/// Le sha de la tête que la forge pousse dans ces tests, et l'adresse de la PR 12 qu'elle ouvre.
const TETE: &str = "c3a91e07d5f2486b0a7e3d915c28b4f60e1d7a39";
const URL_DE_LA_PR: &str = "https://github.com/exemple/depot/pull/12";

/// La branche distante où la forge pousse la vague : celle de son assemblage.
fn branche_de(vague: &str) -> String {
    format!("lydite/vague-{vague}")
}

fn poussee(vague: &str, branche: &str) -> Evenement {
    Evenement::Poussee { vague: vague.into(), branche: branche.into(), sha: TETE.into() }
}

/// La PR 12 que `vague` note, portée par `branche` : celle de la vague, ou celle d'une vague qu'elle relance.
fn pr(vague: &str, branche: &str, foi: Foi) -> Evenement {
    let (vague, branche, base) = (vague.into(), branche.into(), "main".into());
    Evenement::Pr { vague, branche, base, sha: TETE.into(), numero: 12, url: URL_DE_LA_PR.into(), foi }
}

fn ci(vague: &str, couleur: Couleur, foi: Foi) -> Evenement {
    let texte = "test (linux) : SUCCESS".into();
    Evenement::Ci { vague: vague.into(), numero: 12, sha: TETE.into(), couleur, texte, foi }
}

/// Un refus de forge de ce code, aux textes de l'essai : le fil ne tient que le numéro du code, et `lydite etat` dit
/// la règle que porte la note.
fn refus_de_forge(vague: &str, code: u16) -> Evenement {
    let (regle, preuve, correction) = ("Une règle de l'essai.".into(), format!("preuve {code}"), "Corrige.".into());
    Evenement::RefusForge { vague: vague.into(), code, regle, preuve, correction }
}

/// Les notes d'une vague à plan dont les tâches `ids`, indépendantes, sont toutes assemblées, et ce que `lydite etat`
/// en dit.
fn assemblees(vague: &str, objectif: &str, ids: &[&str]) -> (Vec<Evenement>, String) {
    let mut notes = vec![ouverture(vague, ids.len(), objectif), assemblage(vague)];
    notes.extend((1..).zip(ids).map(|(main, id)| tache(vague, main, id, &[])));
    notes.extend((1..=ids.len()).map(|main| fusion(vague, main)));
    let lignes: String = (1..).zip(ids).map(|(main, id)| format!("\nmain-{vague}-{main} {id} : assemblée")).collect();
    let texte = format!("{objectif}\nbranche : lydite/vague-{vague} · preuve : cargo test{lignes}");
    (notes, texte)
}

/// Écrit ces événements au fil d'un dossier neuf, comme Kraken, puis rend ce que `lydite etat` en dit : son texte et
/// son code de sortie.
fn etat_du_fil(evenements: &[Evenement]) -> (String, ExitCode) {
    let dossier = env::temp_dir().join(format!("lydite-test-forge-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    for evenement in evenements {
        fil.noter(evenement).unwrap();
    }
    let Lecture { notes, ecarts } = crate::branchements::dossier::lire(&dossier).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let lus: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(lus, evenements, "le fil rend ce qu'on y a écrit");
    let code = etat_dans(&dossier).unwrap();
    let _ = fs::remove_dir_all(&dossier);
    // Les lignes de reçu de chaque vague (le coût, l'effort) ont leurs propres essais : ici, le reste du texte.
    let texte = rendu(&etat(&lus), MAINTENANT);
    let sans_recu: Vec<&str> =
        texte.lines().filter(|ligne| !ligne.starts_with("Coût : ") && !ligne.starts_with("Effort: ")).collect();
    (sans_recu.join("\n"), code)
}

#[test]
fn kraken_etat_dit_la_pr_et_sa_ci() {
    let branche = branche_de(V1);
    let ligne_pr = |ci: &str| format!("PR 12 · {URL_DE_LA_PR} · {ci}");
    let refusee = |code: u16| format!("refusée ({code}) : Une règle de l'essai.");
    let ouverte = || vec![poussee(V1, &branche), pr(V1, &branche, Foi::Constate)];
    let verte = || ci(V1, Couleur::Vert, Foi::Constate);
    // Chaque cas : les notes de forge qui suivent les deux tâches assemblées, les lignes que `lydite etat` ajoute à
    // leur texte, et son code. Seule une CI verte constatée, sans refus de forge, donne 0.
    let cas = [
        ("PR verte", [ouverte(), vec![verte()]].concat(), vec![ligne_pr("CI verte (constatée)")], ExitCode::SUCCESS),
        (
            "PR rouge",
            [ouverte(), vec![ci(V1, Couleur::Rouge, Foi::Constate)]].concat(),
            vec![ligne_pr("CI rouge (constatée)")],
            ExitCode::FAILURE,
        ),
        ("CI en attente", ouverte(), vec![ligne_pr("CI en attente")], ExitCode::FAILURE),
        ("refusée en forge, sans PR", vec![refus_de_forge(V1, 8005)], vec![refusee(8005)], ExitCode::FAILURE),
        (
            "refusée en forge, la CI muette",
            [ouverte(), vec![refus_de_forge(V1, 8008)]].concat(),
            vec![ligne_pr("CI en attente"), refusee(8008)],
            ExitCode::FAILURE,
        ),
        (
            "refusée en forge, malgré sa CI verte",
            [ouverte(), vec![verte(), refus_de_forge(V1, 8010)]].concat(),
            vec![ligne_pr("CI verte (constatée)"), refusee(8010)],
            ExitCode::FAILURE,
        ),
        ("poussée seule, sans PR", vec![poussee(V1, &branche)], vec![], ExitCode::FAILURE),
    ];
    let (taches, texte) = assemblees(V1, "Deux tâches", &["a", "b"]);
    for (quoi, forge, lignes, attendu) in cas {
        let (dit, code) = etat_du_fil(&[taches.clone(), forge].concat());
        let attendu_texte = [vec![texte.clone()], lignes].concat().join("\n");
        assert_eq!((dit, code), (attendu_texte, attendu), "{quoi}");
    }
    // Une vague sans note de forge garde la règle de M3b : ses tâches assemblées donnent 0, sans ligne de PR.
    assert_eq!(etat_du_fil(&taches), (texte, ExitCode::SUCCESS));
}

#[test]
fn kraken_etat_ne_dit_pas_comme_un_fait_une_pr_declaree() {
    // Une PR et une CI déclarées, que Kraken n'a pas lues, ne s'affichent pas et ne comptent pas : la vague garde la
    // règle de M3b, comme si le fil n'en disait rien.
    let branche = branche_de(V1);
    let (taches, texte) = assemblees(V1, "Deux tâches", &["a", "b"]);
    let declarees = vec![pr(V1, &branche, Foi::Declare), ci(V1, Couleur::Vert, Foi::Declare)];
    assert_eq!(etat_du_fil(&[taches.clone(), declarees].concat()), (texte.clone(), ExitCode::SUCCESS));
    // Une CI verte déclarée ne sauve pas une CI rouge constatée : seule la constatée est prise.
    let constatee = vec![pr(V1, &branche, Foi::Constate), ci(V1, Couleur::Rouge, Foi::Constate)];
    let verte_declaree = vec![ci(V1, Couleur::Vert, Foi::Declare)];
    let (dit, code) = etat_du_fil(&[taches, constatee, verte_declaree].concat());
    assert_eq!(code, ExitCode::FAILURE);
    assert_eq!(dit, format!("{texte}\nPR 12 · {URL_DE_LA_PR} · CI rouge (constatée)"));
}

#[test]
fn kraken_etat_montre_la_vague_relancee_avant_sa_relance() {
    const V0: &str = "1761901000000_7_0";
    let (v1, v2) = (branche_de(V1), branche_de(V2));
    let (ancienne, _) = assemblees(V0, "Une vague plus ancienne", &["a"]);
    let (relancee, texte_relancee) = assemblees(V1, "Deux tâches", &["a", "b"]);
    let (relance, texte_relance) = assemblees(V2, "Rendre verte la CI de la PR 12", &["ci"]);
    let ligne_pr = |ci: &str| format!("PR 12 · {URL_DE_LA_PR} · {ci}");
    // V1 ouvre sa PR 12, dont la CI est rouge ; sa relance V2 pousse dans la même branche, sous son id, et la CI est
    // verte.
    let ouverte = vec![poussee(V1, &v1), pr(V1, &v1, Foi::Constate), ci(V1, Couleur::Rouge, Foi::Constate)];
    let suite = vec![poussee(V2, &v1), pr(V2, &v1, Foi::Constate), ci(V2, Couleur::Vert, Foi::Constate)];
    let fil = [ancienne.clone(), relancee.clone(), ouverte.clone(), relance.clone(), suite].concat();
    // La vague relancée d'abord, puis sa relance ; la vague plus ancienne n'est pas montrée. Les deux sont menées à
    // bout, et la dernière CI est verte : le code est 0.
    let (rouge, verte) = (ligne_pr("CI rouge (constatée)"), ligne_pr("CI verte (constatée)"));
    let attendu = format!("{texte_relancee}\n{rouge}\n\n{texte_relance}\n{verte}");
    assert_eq!(etat_du_fil(&fil), (attendu, ExitCode::SUCCESS));
    // Le lien vient de la branche que la PR nomme, pas de l'ordre du fil : une vague qui pousse la sienne ne relance
    // rien, et se montre seule.
    let propre = vec![poussee(V2, &v2), pr(V2, &v2, Foi::Constate), ci(V2, Couleur::Vert, Foi::Constate)];
    let fil = [relancee.clone(), ouverte.clone(), relance.clone(), propre].concat();
    assert_eq!(etat_du_fil(&fil), (format!("{texte_relance}\n{verte}"), ExitCode::SUCCESS));
    // De même, une relance dont le fil ne porte pas la vague relancée se montre seule, sa CI en attente.
    let seule = [ancienne, relance.clone(), vec![poussee(V2, &v1), pr(V2, &v1, Foi::Constate)]].concat();
    assert_eq!(etat_du_fil(&seule), (format!("{texte_relance}\n{}", ligne_pr("CI en attente")), ExitCode::FAILURE));
    // Rouge encore : la CI de la relance est rouge, et la vague relancée a le refus 8011 (une seule relance).
    let encore = vec![poussee(V2, &v1), pr(V2, &v1, Foi::Constate), ci(V2, Couleur::Rouge, Foi::Constate)];
    let fil = [relancee, ouverte.clone(), vec![refus_de_forge(V1, 8011)], relance.clone(), encore].concat();
    let (dit, code) = etat_du_fil(&fil);
    assert_eq!(code, ExitCode::FAILURE);
    let attendu =
        format!("{texte_relancee}\n{rouge}\nrefusée (8011) : Une règle de l'essai.\n\n{texte_relance}\n{rouge}");
    assert_eq!(dit, attendu);
    // La vague relancée a une tâche dehors (b, refusée à l'assemblage) : la CI de la relance est verte, et le code
    // reste 1.
    let (mut dehors, _) = assemblees(V1, "Deux tâches", &["a", "b"]);
    dehors.pop(); // la fusion de b
    dehors.push(refus(V1, 2, Code::Conflit));
    let suite = vec![poussee(V2, &v1), pr(V2, &v1, Foi::Constate), ci(V2, Couleur::Vert, Foi::Constate)];
    let (dit, code) = etat_du_fil(&[dehors, ouverte, relance, suite].concat());
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    let b = format!("main-{V1}-2 b : refusée (7007) : {}", Code::Conflit.regle());
    assert!(dit.contains(&b) && dit.contains(&verte), "{dit}");
}

/// Les vagues de M5 de ces tests, dans l'ordre où le fil les nomme : le découpage, la relecture et la correction.
const DECOUPAGE: &str = "1761900900000_7_0";
const RELECTURE: &str = "1761901050000_7_0";
const CORRECTION: &str = "1761901060000_7_0";

/// L'empreinte que porte le plan d'un découpage dans ces tests : celle d'un texte vide, que le fil ne recalcule pas.
const EMPREINTE_DU_PLAN: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

/// La vague de copies du découpeur, une main prise au premier coup, avec ce verdict.
fn decoupeur_pris(issue: (Verdict, Foi)) -> Vec<Evenement> {
    let vague = DECOUPAGE;
    vec![
        ouverture(vague, 1, "Découper l'objectif"),
        debut(vague, 1, 1),
        fin(vague, 1, 1, VERTE, "PLAN.lydite écrit"),
        verdict(vague, 1, issue),
    ]
}

/// Le découpeur classé : rouge deux fois, la seconde sur un plan que Kraken refuse.
fn decoupeur_classe() -> Vec<Evenement> {
    let vague = DECOUPAGE;
    vec![
        ouverture(vague, 1, "Découper l'objectif"),
        debut(vague, 1, 1),
        fin(vague, 1, 1, ROUGE, "PLAN.lydite manque"),
        debut(vague, 1, 2),
        fin(vague, 1, 2, ROUGE, "plan refusé\n6021 : le plan ne contient aucune tâche\n"),
        verdict(vague, 1, CLASSEE),
    ]
}

/// Le plan de `taches` tâches que prend le découpage.
fn plan_pris(taches: usize) -> Evenement {
    let (vague, empreinte, plan) = (DECOUPAGE.into(), EMPREINTE_DU_PLAN.into(), "plan 1\nobjectif : o".into());
    Evenement::Decoupage { vague, taches, empreinte, plan }
}

/// La vague `vague` joue le plan du découpage de ces tests.
fn joue(vague: &str) -> Evenement {
    Evenement::Joue { vague: vague.into(), decoupage: DECOUPAGE.into() }
}

/// La vague d'une main du relecteur, prise du premier coup, puis ce qu'elle a rendu de la vague `relue`.
fn relecture_rendue(relue: &str, n: usize, bloquantes: usize) -> Vec<Evenement> {
    let remarques = "remarques 1".into();
    let rendu = Evenement::Relecture { vague: RELECTURE.into(), relue: relue.into(), n, bloquantes, remarques };
    [vec![ouverture(RELECTURE, 1, "Relire le travail critique")], prise_de(RELECTURE, 1), vec![rendu]].concat()
}

/// La vague de correction de la vague `corrige` : une tâche `correction`, dont `issue` est la fusion ou le refus.
fn correction_de_la(corrige: &str, issue: Evenement) -> Vec<Evenement> {
    let lien = Evenement::Correction { vague: CORRECTION.into(), corrige: corrige.into() };
    let ouverte = ouverture(CORRECTION, 1, "Corrige au plus petit les remarques bloquantes de la relecture");
    vec![ouverte, assemblage(CORRECTION), lien, tache(CORRECTION, 1, "correction", &[]), issue]
}

/// La ligne que `lydite etat` dit d'un découpage qui a pris un plan de `taches` tâches.
fn ligne_du_decoupage_pris(taches: usize) -> String {
    format!("Découpage : {taches} tâches · main-{DECOUPAGE}-1 · prise (constatée)")
}

#[test]
fn kraken_etat_dit_la_chaine_d_une_vague_decoupee() {
    let (plan, texte) = assemblees(V1, "Deux tâches", &["a", "b"]);
    let decoupee = [decoupeur_pris(PRISE), vec![plan_pris(2)], plan, vec![joue(V1)]].concat();
    // Le texte attendu : la ligne du découpage, la vague comme en M3b, puis ces lignes.
    let dit =
        |lignes: &[String]| [vec![ligne_du_decoupage_pris(2), texte.clone()], lignes.to_vec()].concat().join("\n");
    // Une vague découpée et assemblée : sa première ligne est celle de son découpage, et le code est 0.
    assert_eq!(etat_du_fil(&decoupee), (dit(&[]), ExitCode::SUCCESS));
    // Un découpage observé (`--observer`) : aucune vague ne joue son plan, il ne dit que sa ligne, et le code est 0.
    let observe = [decoupeur_pris(PRISE), vec![plan_pris(3)]].concat();
    assert_eq!(etat_du_fil(&observe), (ligne_du_decoupage_pris(3), ExitCode::SUCCESS));
    // Un découpage classé : il se dit comme une vague de copies, sa main classée, et le code est 1.
    let attendu =
        format!("Découper l'objectif\nmain-{DECOUPAGE}-1 : classée : 6021 : le plan ne contient aucune tâche");
    assert_eq!(etat_du_fil(&decoupeur_classe()), (attendu, ExitCode::FAILURE));
    // Relue, sans remarque bloquante : la ligne de la relecture suit les tâches, le relecteur ne se montre pas, et le
    // code est 0.
    let sans_bloquante = [decoupee.clone(), relecture_rendue(V1, 2, 0)].concat();
    let ligne = "Relecture : 2 remarques, dont 0 bloquantes".to_string();
    assert_eq!(etat_du_fil(&sans_bloquante), (dit(&[ligne]), ExitCode::SUCCESS));
    // Relue, corrigée : la correction suit la relecture, et le code est 0.
    let corrigee = correction_de_la(V1, fusion(CORRECTION, 1));
    let corrigee = [decoupee.clone(), relecture_rendue(V1, 3, 1), corrigee].concat();
    let lignes = [
        "Relecture : 3 remarques, dont 1 bloquantes".to_string(),
        format!("Correction : main-{CORRECTION}-1 correction : assemblée"),
    ];
    assert_eq!(etat_du_fil(&corrigee), (dit(&lignes), ExitCode::SUCCESS));
    // Une correction non assemblée : sa ligne dit son refus, et le code est 1.
    let refusee = correction_de_la(V1, refus(CORRECTION, 1, Code::PreuveRouge));
    let refusee = [decoupee.clone(), relecture_rendue(V1, 3, 1), refusee].concat();
    let lignes = [
        "Relecture : 3 remarques, dont 1 bloquantes".to_string(),
        format!("Correction : main-{CORRECTION}-1 correction : refusée (7008) : {}", Code::PreuveRouge.regle()),
    ];
    assert_eq!(etat_du_fil(&refusee), (dit(&lignes), ExitCode::FAILURE));
    // Des remarques bloquantes sans correction : le code est 1.
    let sans_correction = [decoupee.clone(), relecture_rendue(V1, 1, 1)].concat();
    let ligne = "Relecture : 1 remarques, dont 1 bloquantes".to_string();
    assert_eq!(etat_du_fil(&sans_correction), (dit(&[ligne]), ExitCode::FAILURE));
    // Une relecture classée n'a rien rendu, mais le fil la lie à la vague relue (`vague.relecteur`) : la vague relue
    // dit sa relecture classée, puis le refus 7016 de sa forge, et le code est 1.
    let ouverte = ouverture(RELECTURE, 1, "Relire");
    let rouge = vec![debut(RELECTURE, 1, 1), fin(RELECTURE, 1, 1, ROUGE, "rouge"), verdict(RELECTURE, 1, CLASSEE)];
    let lien = || Evenement::Relecteur { vague: RELECTURE.into(), relue: V1.into() };
    let liee = [decoupee.clone(), vec![ouverte.clone(), lien()], rouge.clone(), vec![refus_de_forge(V1, 7016)]];
    let refus = fil::refus_en_clair(7016, "Une règle de l'essai.", "preuve 7016", "Corrige.");
    let ligne = refus.lines().next().unwrap().to_string();
    let lignes = [format!("Relecture : main-{RELECTURE}-1 · classée (7016) : rouge"), ligne];
    assert_eq!(etat_du_fil(&liee.concat()), (dit(&lignes), ExitCode::FAILURE));
    // Un relecteur lié, pris, qui n'a rendu aucune relecture, puis un refus 7012 de la forge : la relecture n'est pas
    // dite classée, mais où en est sa main, puis le refus, et le code est 1.
    let prise =
        [decoupee.clone(), vec![ouverte.clone(), lien()], prise_de(RELECTURE, 1), vec![refus_de_forge(V1, 7012)]];
    let refus = fil::refus_en_clair(7012, "Une règle de l'essai.", "preuve 7012", "Corrige.");
    let lignes = [format!("Relecture : main-{RELECTURE}-1 · prise"), refus.lines().next().unwrap().to_string()];
    assert_eq!(etat_du_fil(&prise.concat()), (dit(&lignes), ExitCode::FAILURE));
    // Sans le refus de la forge, le code reste 1 : la vague n'a pas été relue.
    let sans_refus = [decoupee.clone(), vec![ouverte.clone(), lien()], rouge.clone()].concat();
    assert_eq!(
        etat_du_fil(&sans_refus),
        (dit(&[format!("Relecture : main-{RELECTURE}-1 · classée (7016) : rouge")]), ExitCode::FAILURE)
    );
    // Un fil d'avant le lien : la relecture classée se dit comme une vague de copies, classée, et le code est 1. Le
    // refus 7016, que la forge note sur la vague relue, n'est pas montré.
    let (dit, code) = etat_du_fil(&[decoupee, vec![ouverte], rouge].concat());
    assert_eq!((dit.as_str(), code), (&*format!("Relire\nmain-{RELECTURE}-1 : classée : rouge"), ExitCode::FAILURE));
}

#[test]
fn kraken_etat_dit_la_chaine_d_une_vague_decoupee_poussee_puis_relancee_en_ci() {
    // Le découpage pris, la vague jouée, relue (une remarque, bloquante) et corrigée ; sa PR 12 a une CI rouge, que
    // sa relance (V2, une tâche `ci`) rend verte : toute la chaîne se montre, et le code est 0.
    let (plan, texte) = assemblees(V1, "Deux tâches", &["a", "b"]);
    let (relance, texte_relance) = assemblees(V2, "Rendre verte la CI de la PR 12", &["ci"]);
    let branche = branche_de(V1);
    let pousse_v1 = vec![poussee(V1, &branche), pr(V1, &branche, Foi::Constate), ci(V1, Couleur::Rouge, Foi::Constate)];
    let pousse_v2 = vec![poussee(V2, &branche), pr(V2, &branche, Foi::Constate), ci(V2, Couleur::Vert, Foi::Constate)];
    let avant = [
        decoupeur_pris(PRISE),
        vec![plan_pris(2)],
        plan,
        vec![joue(V1)],
        relecture_rendue(V1, 1, 1),
        correction_de_la(V1, fusion(CORRECTION, 1)),
        pousse_v1,
    ]
    .concat();
    let rouge = format!("PR 12 · {URL_DE_LA_PR} · CI rouge (constatée)");
    let verte = format!("PR 12 · {URL_DE_LA_PR} · CI verte (constatée)");
    let attendu = [
        ligne_du_decoupage_pris(2),
        texte,
        "Relecture : 1 remarques, dont 1 bloquantes".to_string(),
        format!("Correction : main-{CORRECTION}-1 correction : assemblée"),
        rouge.clone(),
        String::new(),
        texte_relance,
        verte,
    ]
    .join("\n");
    assert_eq!(etat_du_fil(&[avant.clone(), relance, pousse_v2].concat()), (attendu, ExitCode::SUCCESS));
    // La même chaîne, la CI de la relance rouge encore et la vague relancée refusée en 8011 : le code est 1.
    let (relance, _) = assemblees(V2, "Rendre verte la CI de la PR 12", &["ci"]);
    let rouge_encore =
        vec![poussee(V2, &branche), pr(V2, &branche, Foi::Constate), ci(V2, Couleur::Rouge, Foi::Constate)];
    let fil = [avant, vec![refus_de_forge(V1, 8011)], relance, rouge_encore].concat();
    let (dit, code) = etat_du_fil(&fil);
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    assert!(dit.contains(&format!("{rouge}\nrefusée (8011) : ")) && dit.ends_with(&rouge), "{dit}");
}

#[test]
fn kraken_etat_ne_dit_pas_comme_un_fait_un_decoupage_qu_il_n_a_pas_lu() {
    let (plan, texte) = assemblees(V1, "Deux tâches", &["a", "b"]);
    let jouee = |decoupage: Vec<Evenement>| [decoupage, plan.clone(), vec![joue(V1)]].concat();
    // Un découpage dont la main n'a qu'une prise déclarée : sa ligne porte son mot, et le code n'est pas 0.
    let declaree = jouee([decoupeur_pris(PRISE_DECLAREE), vec![plan_pris(2)]].concat());
    let ligne = format!("Découpage : 2 tâches · main-{DECOUPAGE}-1 · prise déclarée, non prouvée");
    assert_eq!(etat_du_fil(&declaree), (format!("{ligne}\n{texte}"), ExitCode::FAILURE));
    // Un découpage qui n'a pas de plan au fil, ou que le fil ne porte pas : la ligne le dit, le code n'est pas 0.
    let sans_plan = jouee(decoupeur_pris(PRISE));
    let ligne = format!("Découpage : {DECOUPAGE} · sans plan au fil");
    assert_eq!(etat_du_fil(&sans_plan), (format!("{ligne}\n{texte}"), ExitCode::FAILURE));
    let ligne = format!("Découpage : {DECOUPAGE} · absent du fil");
    assert_eq!(etat_du_fil(&jouee(vec![])), (format!("{ligne}\n{texte}"), ExitCode::FAILURE));
    // Un découpage encore en cours : sa main n'a pas fini.
    let en_cours = jouee(vec![ouverture(DECOUPAGE, 1, "Découper"), debut(DECOUPAGE, 1, 1), plan_pris(2)]);
    let ligne = format!("Découpage : 2 tâches · main-{DECOUPAGE}-1 · en cours, tentative 1");
    assert_eq!(etat_du_fil(&en_cours), (format!("{ligne}\n{texte}"), ExitCode::FAILURE));
}

#[test]
fn kraken_etat_suit_ses_liens_sans_boucler() {
    // Un autre écrivain peut lier des vagues en boucle, ou à des vagues que le fil ne porte pas : `lydite etat`
    // s'arrête, et ne dit pas 0 d'une chaîne qu'il ne peut pas suivre.
    let (a, b) = (V1, V2);
    let corrige = |vague: &str, corrige: &str| Evenement::Correction { vague: vague.into(), corrige: corrige.into() };
    let boucle = [ouverture(a, 1, "A"), ouverture(b, 1, "B"), corrige(a, b), corrige(b, a)];
    assert_eq!(etat_du_fil(&boucle).1, ExitCode::FAILURE);
    // Une correction d'une vague que le fil ne porte pas se montre seule.
    let seule = [ouverture(a, 1, "A"), assemblage(a), tache(a, 1, "x", &[]), fusion(a, 1), corrige(a, "inconnue")];
    let (dit, code) = etat_du_fil(&seule);
    assert_eq!(code, ExitCode::SUCCESS, "{dit}");
    assert!(dit.starts_with("A\nbranche : ") && dit.ends_with("x : assemblée"), "{dit}");
    // Deux relectures qui se relisent l'une l'autre : de même, sans boucle.
    let relit = |vague: &str, relue: &str| Evenement::Relecture {
        vague: vague.into(),
        relue: relue.into(),
        n: 0,
        bloquantes: 0,
        remarques: String::new(),
    };
    let relues = [ouverture(a, 1, "A"), ouverture(b, 1, "B"), relit(a, b), relit(b, a)];
    assert_eq!(etat_du_fil(&relues).1, ExitCode::FAILURE);
}

#[test]
fn kraken_etat_remonte_toutes_les_relances() {
    const V0: &str = "1761901000000_7_0";
    let (a, texte_a) = assemblees(V0, "Première vague", &["a"]);
    let (b, texte_b) = assemblees(V1, "Première relance", &["b"]);
    let (c, texte_c) = assemblees(V2, "Seconde relance", &["c"]);
    let (v0, v1) = (branche_de(V0), branche_de(V1));
    let ligne_pr = |ci: &str| format!("PR 12 · {URL_DE_LA_PR} · {ci}");
    let (rouge, verte) = (ligne_pr("CI rouge (constatée)"), ligne_pr("CI verte (constatée)"));
    let rouge_dans = |vague: &str, branche: &str| {
        vec![poussee(vague, branche), pr(vague, branche, Foi::Constate), ci(vague, Couleur::Rouge, Foi::Constate)]
    };
    // V1 pousse dans la branche de V0, V2 dans celle de V1 : la chaîne se montre entière, de la plus ancienne à la
    // dernière, et le code est celui de la dernière CI.
    let verte_dans_v1 = vec![poussee(V2, &v1), pr(V2, &v1, Foi::Constate), ci(V2, Couleur::Vert, Foi::Constate)];
    let fil = [a.clone(), rouge_dans(V0, &v0), b.clone(), rouge_dans(V1, &v0), c.clone(), verte_dans_v1].concat();
    let attendu = [texte_a.clone(), rouge.clone(), String::new(), texte_b.clone(), rouge.clone()].join("\n");
    let attendu = [attendu, String::new(), texte_c, verte].join("\n");
    assert_eq!(etat_du_fil(&fil), (attendu, ExitCode::SUCCESS));
    // Des relances qui se relancent l'une l'autre : chacune se montre une fois, et Kraken s'arrête.
    let boucle = [a, rouge_dans(V0, &v1), b, rouge_dans(V1, &v0)].concat();
    let (dit, code) = etat_du_fil(&boucle);
    let attendu = [texte_a, rouge.clone(), String::new(), texte_b, rouge].join("\n");
    assert_eq!((dit, code), (attendu, ExitCode::FAILURE));
}

#[test]
fn kraken_etat_dit_le_fournisseur_declare() {
    // Une main dont le profil nomme un fournisseur le dit à `lydite etat`, après le mur : toujours déclaré. La
    // dernière tentative l'emporte, comme pour le mur.
    use crate::coeur::fil::Mur::Exige;
    let fournisseur = |tentative: usize, hote: &str| Evenement::Fournisseur {
        vague: V1.into(),
        main: 1,
        tentative,
        fournisseur: hote.into(),
    };
    let lancee = [bail(V1, 1, 1, A_VENIR), debut(V1, 1, 1), lancement(V1, 1, 1, Exige)];
    let seul = [&lancee[..], &[fournisseur(1, "api.deepseek.com")]].concat();
    let dit = " · mur exigé (déclaré) · fournisseur api.deepseek.com (déclaré)";
    assert_eq!(etat_de_t(&seul), format!("en cours, tentative 1{dit}"));
    let fin_prise = [constat(V1, 1, 1, Some(true)), fin(V1, 1, 1, VERTE, "fait"), verdict(V1, 1, PRISE), fusion(V1, 1)];
    let assemblee = [&seul[..], &fin_prise[..]].concat();
    let dit = " · mur actif (constaté) · fournisseur api.deepseek.com (déclaré)";
    assert_eq!(etat_de_t(&assemblee), format!("assemblée{dit}"));
    // Sans `main.fournisseur`, la ligne est celle d'avant M5 ; la seconde tentative n'hérite pas de la première.
    assert_eq!(etat_de_t(&lancee), "en cours, tentative 1 · mur exigé (déclaré)");
    let relancee = [&seul[..], &[fin(V1, 1, 1, ROUGE, "rouge"), bail(V1, 1, 2, A_VENIR), debut(V1, 1, 2)]].concat();
    assert_eq!(etat_de_t(&relancee), "en cours, tentative 2");
    let autre = [&relancee[..], &[fournisseur(2, "localhost:8443")]].concat();
    assert_eq!(etat_de_t(&autre), "en cours, tentative 2 · fournisseur localhost:8443 (déclaré)");
    // La fonction à part : rien sans main.
    assert_eq!(mot_du_fournisseur(None), "");
}

#[test]
fn toutes_prises_si_chaque_main_a_pris() {
    let derniere = |evenements: Vec<Evenement>| etat(&evenements).pop().unwrap();
    let prises = derniere(vec![ouverture(V1, 2, "o"), verdict(V1, 1, PRISE), verdict(V1, 2, PRISE)]);
    assert!(toutes_prises(&prises));
    // Une prise déclarée n'y compte pas : la main seule l'a dite.
    let declaree = vec![ouverture(V1, 2, "o"), verdict(V1, 1, PRISE), verdict(V1, 2, PRISE_DECLAREE)];
    assert!(!toutes_prises(&derniere(declaree)));
    assert!(!toutes_prises(&derniere(exemple())));
    // Une main en cours, une classée, une absente (la vague en compte trois), une vague sans ouverture.
    assert!(!toutes_prises(&derniere(vec![ouverture(V1, 2, "o"), verdict(V1, 1, PRISE), debut(V1, 2, 1)])));
    let classee = vec![ouverture(V1, 2, "o"), verdict(V1, 1, PRISE), verdict(V1, 2, CLASSEE)];
    assert!(!toutes_prises(&derniere(classee)));
    let absente = vec![ouverture(V1, 3, "o"), verdict(V1, 1, PRISE), verdict(V1, 2, PRISE)];
    assert!(!toutes_prises(&derniere(absente)));
    assert!(!toutes_prises(&derniere(vec![verdict(V1, 1, PRISE)])));
}

#[test]
fn kraken_etat_ne_sort_en_0_que_sur_des_prises_constatees() {
    let dossier = env::temp_dir().join(format!("lydite-test-sortie-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    // Quatre vagues de deux mains, ajoutées au même fil : `lydite etat` ne regarde que la dernière, et son code ne
    // dépend que d'elle. Une prise déclarée donne 1, comme une main classée.
    let cas = [
        ("deux prises constatées", PRISE, PRISE, ExitCode::SUCCESS),
        ("une prise déclarée", PRISE, PRISE_DECLAREE, ExitCode::FAILURE),
        ("une main classée", CLASSEE, PRISE, ExitCode::FAILURE),
        ("deux prises constatées, de nouveau", PRISE, PRISE, ExitCode::SUCCESS),
    ];
    for (i, (quoi, premiere, seconde, attendu)) in cas.into_iter().enumerate() {
        let vague = format!("17619013000{i}_7_0");
        fil.noter(&ouverture(&vague, 2, quoi)).unwrap();
        fil.noter(&verdict(&vague, 1, premiere)).unwrap();
        fil.noter(&verdict(&vague, 2, seconde)).unwrap();
        assert_eq!(etat_dans(&dossier), Ok(attendu), "{quoi}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

/// Un fil scellé deux fois : l'ouverture de `V1` (l'objectif « premier »), le sceau S1, un début, le sceau S2, puis
/// une note (l'objectif « dernier »). Rend le dossier.
fn fil_scelle(nom: &str) -> std::path::PathBuf {
    let dossier = env::temp_dir().join(format!("lydite-test-{nom}-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    fil.noter(&ouverture(V1, 1, "premier")).unwrap();
    fil.sceller(V1).unwrap();
    fil.noter(&debut(V1, 1, 1)).unwrap();
    fil.sceller(V1).unwrap();
    fil.noter(&ouverture(V1, 1, "dernier")).unwrap();
    dossier
}

/// Change, dans le fil du dossier, `avant` en `apres` (de même longueur) : une ligne changée sur place.
fn changer_dans_le_fil(dossier: &Path, avant: &str, apres: &str) {
    assert_eq!(avant.len(), apres.len());
    let chemin = dossier.join("fil.jsonl");
    let texte = fs::read_to_string(&chemin).unwrap();
    assert_eq!(texte.matches(avant).count(), 1, "{texte}");
    fs::write(&chemin, texte.replace(avant, apres)).unwrap();
}

#[test]
fn un_fil_scelle_intact_passe_kraken_etat() {
    let scelle = fil_scelle("sceau-intact");
    let (_, _, rompu) = crate::branchements::dossier::lire_scelle(&scelle).unwrap();
    assert_eq!(rompu, None);
    // Le même fil, sans ses sceaux, rend le même code : les sceaux n'y changent rien tant qu'ils tiennent.
    let nu = env::temp_dir().join(format!("lydite-test-sans-sceau-{}", lot()));
    fs::create_dir_all(&nu).unwrap();
    let fil = Fil::seance(&nu, "humain");
    for evenement in [ouverture(V1, 1, "premier"), debut(V1, 1, 1), ouverture(V1, 1, "dernier")] {
        fil.noter(&evenement).unwrap();
    }
    let code = etat_dans(&nu).unwrap();
    assert_eq!(etat_dans(&scelle), Ok(code));
    assert_ne!(code, ExitCode::from(2));
    let _ = fs::remove_dir_all(&scelle);
    let _ = fs::remove_dir_all(&nu);
}

#[test]
fn un_octet_change_avant_un_sceau_le_fait_nommer() {
    let dossier = fil_scelle("sceau-rompu");
    // Une ligne d'avant le premier sceau change : le premier sceau est nommé, avec ses octets.
    let (_, _, intact) = crate::branchements::dossier::lire_scelle(&dossier).unwrap();
    assert_eq!(intact, None);
    let premier = crate::branchements::dossier::lire_sceaux(&dossier)[0].clone();
    changer_dans_le_fil(&dossier, "premier", "premiex");
    let (_, _, rompu) = crate::branchements::dossier::lire_scelle(&dossier).unwrap();
    assert_eq!(rompu, Some(premier.clone()));
    assert_eq!(rompu.map(|sceau| (sceau.vague, sceau.octets)), Some((V1.to_string(), premier.octets)));
    // `lydite etat` échoue, du code d'un fil qu'il ne lit pas.
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::from(2)));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn le_premier_sceau_rompu_est_nomme_pas_le_dernier() {
    let dossier = fil_scelle("sceau-second");
    // Une ligne entre les deux sceaux change : le premier tient encore, le second est nommé.
    let sceaux = crate::branchements::dossier::lire_sceaux(&dossier);
    changer_dans_le_fil(&dossier, r#""main":1,"tentative":1}"#, r#""main":2,"tentative":1}"#);
    let (_, _, rompu) = crate::branchements::dossier::lire_scelle(&dossier).unwrap();
    assert_eq!(rompu, Some(sceaux[1].clone()));
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::from(2)));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_octet_change_apres_le_dernier_sceau_ne_se_voit_pas() {
    let dossier = fil_scelle("sceau-apres");
    let avant = etat_dans(&dossier).unwrap();
    // La note d'après le dernier sceau change, et une ligne s'y ajoute : aucun sceau ne les couvre.
    changer_dans_le_fil(&dossier, "dernier", "dernieu");
    let chemin = dossier.join("fil.jsonl");
    let mut texte = fs::read_to_string(&chemin).unwrap();
    texte.push_str("{\"ajoutee\":\"apres\"}\n");
    fs::write(&chemin, texte).unwrap();
    let (_, _, rompu) = crate::branchements::dossier::lire_scelle(&dossier).unwrap();
    assert_eq!(rompu, None);
    assert_eq!(etat_dans(&dossier), Ok(avant));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_fil_reecrit_avec_ses_sceaux_refaits_ne_se_voit_pas() {
    let dossier = fil_scelle("sceau-refait");
    // Le fil change avant son premier sceau, puis on refait les sceaux sur le fil changé : ils tiennent.
    changer_dans_le_fil(&dossier, "premier", "premiex");
    let chemin = dossier.join("fil.jsonl");
    let texte = fs::read_to_string(&chemin).unwrap();
    let sans_sceaux: String =
        texte.lines().filter(|ligne| !ligne.contains("vague.sceau")).map(|ligne| format!("{ligne}\n")).collect();
    fs::write(&chemin, sans_sceaux).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    fil.sceller(V1).unwrap();
    let (_, _, rompu) = crate::branchements::dossier::lire_scelle(&dossier).unwrap();
    assert_eq!(rompu, None);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_fil_absent_est_aucune_vague() {
    let dossier = env::temp_dir().join(format!("lydite-test-etat-{}", lot()));
    assert_eq!(crate::branchements::dossier::lire(&dossier), Ok(Lecture::default()));
    assert_eq!(aucune_vague(&dossier), "aucune vague");
    assert_eq!(rendu(&[], MAINTENANT), "aucune vague");
    // Sans vague, rien n'est affirmé : le code 0, comme en M1.
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::SUCCESS));
    assert!(!dossier.exists(), "lydite etat ne crée rien");
}

#[test]
fn un_fil_de_m1_seul_est_dit() {
    let dossier = env::temp_dir().join(format!("lydite-test-m1-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let m1 = dossier.join("fil");
    let ancien = "1761901200000\tvague\t1761901200000\t1\t/srv/depot\t\tÉcrire bonjour.txt\n";
    fs::write(&m1, ancien).unwrap();
    let attendu = format!("aucune vague au fil v2 ; le fil de M1 n'est plus lu : {}", m1.display());
    assert_eq!(aucune_vague(&dossier), attendu);
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::SUCCESS));
    // Un fil v2 sans vague, le type d'une autre pièce seul : le fil de M1 est encore dit.
    let inconnu = concat!(
        r#"{"v":2,"seance":"s","seq":1,"instant":"2026-10-31T21:03:00.123Z","origine":"humain","#,
        r#""origine_foi":"declare","type":"ci.fin"}"#,
    );
    fs::write(dossier.join("fil.jsonl"), format!("{inconnu}\n")).unwrap();
    assert_eq!(crate::branchements::dossier::lire(&dossier), Ok(Lecture::default()));
    assert_eq!(aucune_vague(&dossier), attendu);
    // Il n'est ni lu, ni écrit.
    assert_eq!(fs::read_to_string(&m1).unwrap(), ancien);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_declare_ne_s_affiche_jamais_comme_un_fait() {
    // `lydite etat` : un verdict déclaré porte son mot, un constaté s'affiche seul. Kraken n'écrit pas de classée
    // déclarée, mais un autre écrivain le peut : son mot la suit aussi.
    let vague = etat(&[
        ouverture(V1, 4, "o"),
        verdict(V1, 1, PRISE),
        verdict(V1, 2, PRISE_DECLAREE),
        fin(V1, 3, 1, ROUGE, "rouge"),
        verdict(V1, 3, CLASSEE),
        fin(V1, 4, 1, ROUGE, "rouge"),
        verdict(V1, 4, (Verdict::Classee, Foi::Declare)),
    ])
    .pop()
    .unwrap();
    let lignes: Vec<String> = (1..=4).map(|n| etat_de(vague.mains.get(&n), None, MAINTENANT)).collect();
    let attendu = ["prise", "prise déclarée, non prouvée", "classée : rouge", "classée déclarée, non prouvée : rouge"];
    assert_eq!(lignes, attendu);
    // `lancer` : une prise déclarée porte son mot, une constatée s'affiche seule.
    let pris = |prouve| -> Result<Pris, String> { Ok(Pris { sortie: "fait".into(), prouve }) };
    assert_eq!(annonce("main-v-1", &pris(true)), "pris : main-v-1");
    assert_eq!(annonce("main-v-1", &pris(false)), "pris, déclaré : main-v-1");
    assert_eq!(annonce("main-v-1", &Err("rouge".into())), "échec classé : main-v-1\nrouge");
    // Le code de sortie : un déclaré ne donne jamais 0.
    assert!(!toutes_prises(&vague));
    assert!(!toutes_constatees(&[("main-v-1".into(), pris(false))]));
}

#[test]
fn kraken_etat_ne_panique_pas_sur_un_nombre_demesure() {
    // Un autre écrivain peut poser un `n` ou un `main` qui tient en u64 : `rendu` écrirait une ligne par main, jusqu'à
    // la panique ou la mémoire pleine. Le lecteur refuse ces lignes (1009), et l'état se dit sans elles.
    let dossier = env::temp_dir().join(format!("lydite-test-demesure-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    // Une vague de deux mains prises et constatées, puis, sur elle, un `n` démesuré et une main démesurée.
    let ecrites = [
        ouverture(V1, 2, "deux mains"),
        verdict(V1, 1, PRISE),
        verdict(V1, 2, PRISE),
        ouverture(V1, usize::MAX, "n démesuré"),
        debut(V1, usize::MAX, 1),
    ];
    for evenement in &ecrites {
        fil.noter(evenement).unwrap();
    }
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::SUCCESS));
    let Lecture { notes, ecarts } = crate::branchements::dossier::lire(&dossier).unwrap();
    let ecarts: Vec<_> = ecarts.iter().map(|(ligne, refus)| (*ligne, refus.code)).collect();
    assert_eq!(ecarts, [(4, Code::EvenementMalForme), (5, Code::EvenementMalForme)]);
    let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
    assert_eq!(
        rendu(&etat(&evenements), MAINTENANT),
        format!(
            "deux mains\nmain-{V1}-1 : prise\nmain-{V1}-2 : prise\nCoût : inconnu · 0 main, 0 relance, 0 classée\n\
             Effort: Claude Code's default"
        ),
    );
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_main_0_n_est_pas_une_main_de_la_vague() {
    // `rendu` part de la main 1 : une main 0, lue, serait comptée par `toutes_prises` sans jamais être dite.
    let dossier = env::temp_dir().join(format!("lydite-test-main0-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let fil = Fil::seance(&dossier, "humain");
    for evenement in
        [ouverture(V1, 2, "deux mains"), verdict(V1, 1, PRISE), verdict(V1, 0, CLASSEE), verdict(V1, 2, PRISE)]
    {
        fil.noter(&evenement).unwrap();
    }
    let ecarts = crate::branchements::dossier::lire(&dossier).unwrap().ecarts;
    assert_eq!(ecarts.len(), 1, "{ecarts:?}");
    assert_eq!((ecarts[0].0, ecarts[0].1.code), (3, Code::EvenementMalForme));
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::SUCCESS), "deux mains prises et constatées");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn kraken_etat_refuse_un_fil_d_une_autre_version() {
    let dossier = env::temp_dir().join(format!("lydite-test-version-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    let v2 = concat!(
        r#"{"v":2,"seance":"s","seq":1,"instant":"2026-10-31T21:03:00.123Z","origine":"humain","#,
        r#""origine_foi":"declare","type":"main.debut","vague":"v","main":1,"tentative":1}"#,
    );
    let v3 = r#"{"v":3,"seance":"s","seq":2,"origine":["humain"],"type":"main.debut"}"#;
    fs::write(dossier.join("fil.jsonl"), format!("{v2}\n{v3}\n")).unwrap();
    // Un refus : `main` l'écrit en quatre lignes sur stderr, puis sort en 2.
    let refus = etat_dans(&dossier).unwrap_err();
    assert_eq!(refus.code, Code::VersionDifferente);
    assert!(refus.preuve.ends_with("ligne 2 : v vaut 3, et ce lecteur ne lit que v 2"), "{refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

/// Une ligne `main.usage` de la main 1, tentative 1, de `micro_dollars` sous `base`.
fn usage(vague: &str, micro_dollars: u64, base: fil::Base) -> Evenement {
    Evenement::Usage {
        vague: vague.into(),
        main: 1,
        tentative: 1,
        modele: "sonnet".into(),
        entree: 1,
        sortie: 1,
        cache_ecrit: 0,
        cache_lu: 0,
        reflexion: 0,
        micro_dollars,
        base,
    }
}

/// Les notes de ces événements, la première à `premier` et la dernière à `dernier`.
fn notes_de(evenements: &[Evenement], premier: &str, dernier: &str) -> Vec<fil::Note> {
    let n = evenements.len();
    let note = |(i, evenement): (usize, &Evenement)| fil::Note {
        seance: "s".into(),
        seq: i as u64 + 1,
        instant: if i == 0 {
            premier
        } else if i + 1 == n {
            dernier
        } else {
            premier
        }
        .into(),
        origine: "humain".into(),
        origine_foi: Foi::Declare,
        evenement: evenement.clone(),
    };
    evenements.iter().enumerate().map(note).collect()
}

#[test]
fn le_recu_de_kraken_etat_somme_les_usages_meles_la_vague_et_ses_vagues_liees() {
    let corrige = "1761901300000_7_0";
    let evenements = [
        ouverture(V2, 2, "Deux mains"),
        debut(V2, 1, 1),
        usage(V2, 600_000, fil::Base::Estime),
        usage(V2, 240_000, fil::Base::Forfait),
        usage(V2, 0, fil::Base::Inconnu),
        fin(V2, 1, 1, VERTE, "ok"),
        verdict(V2, 1, PRISE),
        debut(V2, 2, 1),
        debut(V2, 2, 2),
        verdict(V2, 2, CLASSEE),
        // Une correction de la vague, d'une main à elle : son usage et sa tentative comptent dans le reçu.
        ouverture(corrige, 1, "Corriger"),
        Evenement::Correction { vague: corrige.into(), corrige: V2.into() },
        debut(corrige, 1, 1),
        usage(corrige, 1_000_000, fil::Base::Estime),
    ];
    // De la première note de la vague à la dernière de la correction : 41 min 30 s.
    let notes = notes_de(&evenements, "2026-10-31T21:00:00.000Z", "2026-10-31T21:41:30.000Z");
    let evenements: Vec<Evenement> = notes.iter().map(|note| note.evenement.clone()).collect();
    let vagues = etat(&evenements);
    let texte = rendu_avec(&vagues, &notes, MAINTENANT);
    let derniere = sans_la_ligne_d_effort(&texte).lines().last().unwrap();
    assert_eq!(derniere, "Coût : 1,84 $ estimé + inconnu · 4 mains, 1 relance, 1 classée · 41 min", "{texte}");
    assert_eq!(texte.lines().filter(|ligne| ligne.starts_with("Coût")).count(), 1, "{texte}");

    // Tout au forfait : « forfait, ≈ … $ », sans « estimé » ni « inconnu » ; sans notes lues, pas de durée.
    let evenements = [
        ouverture(V2, 1, "Une main"),
        debut(V2, 1, 1),
        usage(V2, 240_000, fil::Base::Forfait),
        usage(V2, 10_000, fil::Base::Forfait),
        verdict(V2, 1, PRISE),
    ];
    let notes = notes_de(&evenements, "2026-10-31T21:00:00.000Z", "2026-10-31T21:00:30.000Z");
    let texte = rendu_avec(&etat(&evenements), &notes, MAINTENANT);
    assert_eq!(
        sans_la_ligne_d_effort(&texte).lines().last().unwrap(),
        "Coût : forfait, ≈ 0,25 $ · 1 main, 0 relance, 0 classée · 30 s",
        "{texte}"
    );
    // Sans les notes, le fil rejoué ne dit ni usage ni durée : le coût est inconnu.
    let texte = rendu(&etat(&evenements), MAINTENANT);
    let cout = sans_la_ligne_d_effort(&texte).lines().last().unwrap();
    assert_eq!(cout, "Coût : inconnu · 1 main, 0 relance, 0 classée", "{texte}");

    // Rien que de l'inconnu : le coût est inconnu, sans « + inconnu ».
    let evenements = [ouverture(V2, 1, "Une main"), debut(V2, 1, 1), usage(V2, 0, fil::Base::Inconnu)];
    let notes = notes_de(&evenements, "2026-10-31T21:00:00.000Z", "2026-10-31T21:02:00.000Z");
    let texte = rendu_avec(&etat(&evenements), &notes, MAINTENANT);
    assert!(
        sans_la_ligne_d_effort(&texte).ends_with("\nCoût : inconnu · 1 main, 0 relance, 0 classée · 2 min"),
        "{texte}"
    );

    // Sans aucun usage au fil, `lydite etat` dit tout de même la ligne de reçu, au coût inconnu.
    let sans = [ouverture(V2, 1, "Une main"), debut(V2, 1, 1)];
    let notes = notes_de(&sans, "2026-10-31T21:00:00.000Z", "2026-10-31T21:00:30.000Z");
    let texte = rendu_avec(&etat(&sans), &notes, MAINTENANT);
    let cout = sans_la_ligne_d_effort(&texte).lines().last().unwrap();
    assert_eq!(cout, "Coût : inconnu · 1 main, 0 relance, 0 classée · 30 s", "{texte}");
}

/// Le texte sans sa dernière ligne : le reçu de `lydite etat` finit par le coût, puis par l'effort.
fn sans_la_ligne_d_effort(texte: &str) -> &str {
    let (reste, effort) = texte.rsplit_once('\n').expect("plusieurs lignes");
    assert!(effort.starts_with("Effort: "), "{texte}");
    reste
}

#[test]
fn le_recu_de_kraken_etat_dit_l_effort_de_chaque_role_ou_le_defaut_de_claude_code() {
    let corrige = "1761901300000_7_0";
    // Le lancement d'une tentative, à l'effort `effort` (`None` : rien de déclaré), puis sa consigne, de rôle `role`.
    let lance = |vague: &str, main, tentative, effort: Option<&str>, role| {
        let mut lancement = lancement(vague, main, tentative, fil::Mur::Exige);
        if let Evenement::Lancement { effort: declare, .. } = &mut lancement {
            *declare = effort.map(str::to_string);
        }
        let consigne = Evenement::Consigne {
            vague: vague.into(),
            main,
            tentative,
            role,
            consigne: "c".into(),
            systeme: None,
            lecons: String::new(),
            ecartees: String::new(),
            mesure: String::new(),
        };
        [debut(vague, main, tentative), lancement, consigne]
    };
    let evenements: Vec<Evenement> = [
        vec![ouverture(V2, 3, "Trois mains")],
        // La main 1 part à `low`, et sa relance à `high` ; la main 2 reprend `low` ; la 3e n'en déclare aucun.
        lance(V2, 1, 1, Some("low"), fil::Role::Main).to_vec(),
        lance(V2, 1, 2, Some("high"), fil::Role::Main).to_vec(),
        lance(V2, 2, 1, Some("low"), fil::Role::Main).to_vec(),
        lance(V2, 3, 1, None, fil::Role::Main).to_vec(),
        // Une correction de la vague : son rôle a son effort, lu dans sa vague liée.
        vec![ouverture(corrige, 1, "Corriger"), Evenement::Correction { vague: corrige.into(), corrige: V2.into() }],
        lance(corrige, 1, 1, Some("max"), fil::Role::Correction).to_vec(),
    ]
    .concat();
    let notes = notes_de(&evenements, "2026-10-31T21:00:00.000Z", "2026-10-31T21:01:00.000Z");
    let texte = rendu_avec(&etat(&evenements), &notes, MAINTENANT);
    let attendu = "Effort: hand: low, high, Claude Code's default; correction: max";
    assert_eq!(texte.lines().last().unwrap(), attendu, "{texte}");
    // Sans la correction liée, la ligne ne dit plus son rôle ; sans aucune main, elle dit le défaut.
    let sans = &evenements[..evenements.len() - 5];
    let notes = notes_de(sans, "2026-10-31T21:00:00.000Z", "2026-10-31T21:01:00.000Z");
    let texte = rendu_avec(&etat(sans), &notes, MAINTENANT);
    assert_eq!(texte.lines().last().unwrap(), "Effort: hand: low, high, Claude Code's default", "{texte}");
    let nue = [ouverture(V2, 1, "Une main")];
    let texte =
        rendu_avec(&etat(&nue), &notes_de(&nue, "2026-10-31T21:00:00.000Z", "2026-10-31T21:01:00.000Z"), MAINTENANT);
    assert_eq!(texte.lines().last().unwrap(), "Effort: Claude Code's default", "{texte}");
}

#[test]
fn le_recu_de_kraken_etat_dit_la_mesure_sur_une_ligne_a_part_quand_un_reglage_est_pose() {
    let consigne = |vague: &str, mesure: &str| Evenement::Consigne {
        vague: vague.into(),
        main: 1,
        tentative: 1,
        role: fil::Role::Main,
        consigne: "c".into(),
        systeme: None,
        lecons: String::new(),
        ecartees: String::new(),
        mesure: mesure.into(),
    };
    let rendu_de = |mesure: &str| {
        let evenements = [ouverture(V2, 1, "Une main"), debut(V2, 1, 1), consigne(V2, mesure)];
        let notes = notes_de(&evenements, "2026-10-31T21:00:00.000Z", "2026-10-31T21:00:30.000Z");
        rendu_avec(&etat(&evenements), &notes, MAINTENANT)
    };
    let cout = "Coût : inconnu · 1 main, 0 relance, 0 classée · 30 s";
    // Une vraie vague ne porte aucune ligne de mesure.
    let effort = "Effort: hand: Claude Code's default";
    let vraie = rendu_de("");
    assert!(!vraie.contains("Measurement") && vraie.ends_with(&format!("\n{cout}\n{effort}")), "{vraie}");
    // Un réglage posé, ou deux : une ligne à part, en anglais, sous le coût et l'effort.
    let un = rendu_de("sans-a1");
    assert!(un.ends_with(&format!("\n{cout}\n{effort}\nMeasurement: sans-a1.")), "{un}");
    let deux = rendu_de("sans-lecture\nsans-a1");
    assert!(deux.ends_with(&format!("\n{cout}\n{effort}\nMeasurement: sans-lecture, sans-a1.")), "{deux}");
    // Un texte qui n'est pas un réglage du fil n'entre pas dans le reçu.
    let hostile = rendu_de("sans-lecture\n@quelqu'un a dit");
    assert!(hostile.ends_with("\nMeasurement: sans-lecture."), "{hostile}");
}

// `attendre` : les tests posent le fil et le verrou à la main, et la pause est une fonction qui joue la fin du
// lancement ; aucun ne dépend d'un fil d'exécution, et la pause qui lâche le verrou attend que le système l'ait
// lâché pour de bon (`lacher`), puisque fermer le fichier ne suffit pas quand un processus né à cet instant en garde
// une copie.

/// Un dossier neuf pour `attendre`, vide.
fn dossier_d_attente(nom: &str) -> std::path::PathBuf {
    let dossier = env::temp_dir().join(format!("lydite-test-attendre-{nom}-{}", lot()));
    fs::create_dir_all(&dossier).unwrap();
    dossier
}

/// Le verrou qu'un lancement tient sur le dossier, tant que le fichier reste ouvert.
fn tenu_par_un_lancement(dossier: &Path) -> fs::File {
    let verrou = dossier.join(crate::branchements::dossier::VERROU);
    let ouvert = fs::OpenOptions::new().create(true).write(true).truncate(false).open(verrou).unwrap();
    ouvert.try_lock().unwrap();
    ouvert
}

/// Ce qu'un lancement écrit au fil : ces événements, puis, avec `scelle`, le sceau de fin de `V1`.
fn ecrire_au_fil(dossier: &Path, evenements: &[Evenement], scelle: bool) {
    let fil = Fil::seance(dossier, "humain");
    for evenement in evenements {
        fil.noter(evenement).unwrap();
    }
    if scelle {
        fil.sceller(V1).unwrap();
    }
}

/// Une vague d'une main, prise (constatée) ou classée.
fn vague_d_une_main(verdict_final: (Verdict, Foi)) -> Vec<Evenement> {
    let couleur = if verdict_final.0 == Verdict::Prise { VERTE } else { ROUGE };
    vec![
        ouverture(V1, 1, "Écrire bonjour.txt"),
        debut(V1, 1, 1),
        fin(V1, 1, 1, couleur, "fini"),
        verdict(V1, 1, verdict_final),
    ]
}

#[test]
fn une_vague_qui_finit_pendant_l_attente_fait_sortir_la_commande_avec_son_code() {
    for (verdict_final, attendu) in [(PRISE, ExitCode::SUCCESS), (CLASSEE, ExitCode::FAILURE)] {
        let dossier = dossier_d_attente("finit");
        let mut lancement = Some(tenu_par_un_lancement(&dossier));
        let mut pauses = 0;
        // Le lancement tient le dossier, le fil est vide ; il finit pendant la première pause : sa vague et son sceau,
        // puis le verrou lâché. Aucune pause de grâce n'est donnée (0) : l'attente d'un dossier tenu ne les compte pas.
        let code = attendre_avec(&dossier, 0, &mut || {
            pauses += 1;
            ecrire_au_fil(&dossier, &vague_d_une_main(verdict_final), true);
            lacher(lancement.take().unwrap(), &dossier);
        });
        assert_eq!(code, Ok(attendu), "{verdict_final:?}");
        assert_eq!(pauses, 1);
        let _ = fs::remove_dir_all(&dossier);
    }
}

#[test]
fn un_dossier_tenu_longtemps_ne_compte_pas_contre_le_delai_d_ouverture() {
    let dossier = dossier_d_attente("longtemps");
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    let mut pauses = 0;
    let code = attendre_avec(&dossier, 1, &mut || {
        pauses += 1;
        // Le lancement note sa vague pendant qu'il tient le dossier, puis le lâche à la septième pause.
        if pauses == 3 {
            ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), true);
        }
        if pauses == 7 {
            lacher(lancement.take().unwrap(), &dossier);
        }
    });
    assert_eq!(code, Ok(ExitCode::SUCCESS));
    assert_eq!(pauses, 7, "la sonde attend la fin du lancement, sans compter ses pauses");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_dossier_tenu_puis_libre_sans_note_de_plus_ne_dit_pas_la_vague_d_avant() {
    let dossier = dossier_d_attente("sans-note");
    // La dernière vague du fil est verte et scellée : ancienne, elle ne doit pas se lire « tout est vert ».
    ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), true);
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    let mut erreur = Vec::new();
    let code = attendre_ecrivant(&dossier, 0, &mut || lacher(lancement.take().unwrap(), &dossier), &mut erreur);
    assert_eq!(code, Ok(ExitCode::from(2)));
    assert_eq!(String::from_utf8(erreur).unwrap().trim_end(), SANS_VAGUE);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_dossier_tenu_puis_libre_sans_note_de_plus_sur_une_vague_sans_sceau_rend_1() {
    let dossier = dossier_d_attente("sans-note-tuee");
    // La dernière vague est menée à bout et verte, mais son lancement est mort avant d'écrire son sceau.
    ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), false);
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    let mut erreur = Vec::new();
    let code = attendre_ecrivant(&dossier, 0, &mut || lacher(lancement.take().unwrap(), &dossier), &mut erreur);
    assert_eq!(code, Ok(ExitCode::FAILURE));
    let erreur = String::from_utf8(erreur).unwrap();
    assert!(erreur.contains("sceau de fin") && !erreur.contains(SANS_VAGUE), "{erreur}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_vague_ouverte_qui_recoit_une_note_de_la_forge_pendant_l_attente_est_dite() {
    let dossier = dossier_d_attente("forge");
    // La vague est ouverte au départ ; pendant l'attente, la forge la refuse et le lancement scelle.
    ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), false);
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    let mut erreur = Vec::new();
    let code = attendre_ecrivant(
        &dossier,
        0,
        &mut || {
            ecrire_au_fil(&dossier, &[refus_de_forge(V1, 8005)], true);
            lacher(lancement.take().unwrap(), &dossier);
        },
        &mut erreur,
    );
    assert_eq!(code, Ok(ExitCode::FAILURE));
    let erreur = String::from_utf8(erreur).unwrap();
    assert!(!erreur.contains(SANS_VAGUE), "{erreur}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_lancement_tue_sans_sceau_de_fin_rend_1_la_ou_etat_rend_0() {
    let dossier = dossier_d_attente("tue");
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    // La vague est menée à bout et verte, mais le lancement est mort avant d'écrire son sceau.
    let code = attendre_avec(&dossier, 0, &mut || {
        ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), false);
        lacher(lancement.take().unwrap(), &dossier);
    });
    assert_eq!(code, Ok(ExitCode::FAILURE));
    // C'est le sceau qui manque : `etat` lit le même fil et le dit vert.
    assert_eq!(etat_dans(&dossier), Ok(ExitCode::SUCCESS));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_decoupage_observe_sans_sceau_n_est_pas_un_lancement_tue() {
    let dossier = dossier_d_attente("observe");
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    // `--observer` ne lance aucune vague et ne scelle rien : son découpage pris reste vert.
    let observe = [decoupeur_pris(PRISE), vec![plan_pris(3)]].concat();
    let code = attendre_avec(&dossier, 0, &mut || {
        ecrire_au_fil(&dossier, &observe, false);
        lacher(lancement.take().unwrap(), &dossier);
    });
    assert_eq!(code, Ok(ExitCode::SUCCESS));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_fil_au_sceau_rompu_rend_2_et_un_fil_illisible_est_un_refus() {
    let dossier = fil_scelle("attendre-rompu");
    changer_dans_le_fil(&dossier, "premier", "premiex");
    let mut lancement = Some(tenu_par_un_lancement(&dossier));
    let mut erreur = Vec::new();
    let code = attendre_ecrivant(&dossier, 0, &mut || lacher(lancement.take().unwrap(), &dossier), &mut erreur);
    assert_eq!(code, Ok(ExitCode::from(2)));
    // Le sceau rompu se dit par son propre message, non par celui d'un lancement sans vague.
    let erreur = String::from_utf8(erreur).unwrap();
    assert!(erreur.contains("ne correspond plus au sceau") && !erreur.contains(SANS_VAGUE), "{erreur}");
    let _ = fs::remove_dir_all(&dossier);
    // Un fil qu'on ne lit pas (ici un dossier à la place du fichier) : le refus, que `principal` sort en 2.
    let illisible = dossier_d_attente("illisible");
    fs::create_dir(illisible.join("fil.jsonl")).unwrap();
    let refus = attendre_avec(&illisible, 0, &mut || panic!("rien ne s'attend sur un fil illisible")).unwrap_err();
    assert!(refus.preuve.contains("fil.jsonl"), "{refus}");
    let _ = fs::remove_dir_all(&illisible);
}

#[test]
fn sans_dossier_tenu_ni_vague_neuve_attendre_le_dit_et_sort_en_2() {
    let dossier = dossier_d_attente("rien");
    // Une vague d'avant, verte et scellée : ce n'est pas une vague neuve, `attendre` ne la rend pas.
    ecrire_au_fil(&dossier, &vague_d_une_main(PRISE), true);
    let mut pauses = 0;
    let code = attendre_avec(&dossier, 3, &mut || pauses += 1);
    assert_eq!(code, Ok(ExitCode::from(2)));
    assert_eq!(pauses, 3, "tout le délai est laissé à une vague pour s'ouvrir");
    // Sans délai, une seule sonde et pas une pause.
    assert_eq!(attendre_avec(&dossier, 0, &mut || panic!("pas de pause")), Ok(ExitCode::from(2)));
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_vague_qui_s_ouvre_et_finit_pendant_le_delai_est_rendue_sans_avoir_vu_le_dossier_tenu() {
    let dossier = dossier_d_attente("ouvre");
    let mut pauses = 0;
    // Le lancement part et finit entre deux sondes : la sonde ne le voit pas, le fil le montre.
    let code = attendre_avec(&dossier, 5, &mut || {
        pauses += 1;
        if pauses == 2 {
            ecrire_au_fil(&dossier, &vague_d_une_main(CLASSEE), true);
        }
    });
    assert_eq!(code, Ok(ExitCode::FAILURE));
    assert_eq!(pauses, 2);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn kraken_etat_n_ecrit_aucun_controle_venu_d_une_main_et_garde_ses_propres_mots() {
    let piege = "rouge\r\u{1b}[2K\u{202e}impossible (déclarée)";
    let evenements = vec![
        ouverture(V1, 1, "Écrire\u{1b}[2J\nsuite\u{202e}"),
        debut(V1, 1, 1),
        fin(V1, 1, 1, ROUGE, &format!("première ligne\n{piege}")),
        verdict(V1, 1, CLASSEE),
    ];
    let (texte, _) = etat_du_fil(&evenements);
    for interdit in ['\r', '\u{1b}', '\u{202e}'] {
        assert!(!texte.contains(interdit), "{texte:?}");
    }
    let ligne = texte.lines().find(|ligne| ligne.starts_with(&format!("main-{V1}-1"))).expect(&texte);
    assert_eq!(ligne, format!("main-{V1}-1 : classée : rouge\\r\\u{{1b}}[2K\\u{{202e}}impossible (déclarée)"));
    // L'objectif garde ses lignes, et écrit ses contrôles.
    assert!(texte.starts_with("Écrire\\u{1b}[2J\nsuite\\u{202e}\n"), "{texte:?}");
}
