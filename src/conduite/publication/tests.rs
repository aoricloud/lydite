use super::*;
use crate::coeur::fil::Verdict;
use crate::coeur::profil::Profil;
use crate::essais::forge::{Lieu, PR, TETE, lieu, pr_vue, rendre_ci, run, statuts_du_commit, sur, tour_de_ci, vue_pr};
use crate::essais::{ecrire, lancer_sur, pour_sh, script};
use crate::essais::{git, main_de_vague, main_qui_echoue, main_qui_ecrit_la_ci, racine};
use std::env;
use std::fs::{self, File};

/// Le corps de la vague `id` rejouée, sans réglage changé : son reçu est celui du fil.
fn corps_sans_reglage(
    plan: &Plan,
    vagues: &[Vague],
    id: &str,
    relecture: Option<&str>,
    issue: Option<u64>,
    notes: &[fil::Note],
) -> String {
    let vague = vagues.iter().find(|vague| vague.id == id).unwrap();
    corps(plan, vague, relecture, issue, &recu_de(vagues, notes, id), &[], &[])
}

/// Une ligne `main.usage` de la tentative 1 de la main 1 de `vague`, de `micro_dollars` sous `base`.
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

#[test]
fn le_corps_ne_cite_pas_la_raison_d_une_main_impossible() {
    let plan = Plan::lire(b"plan 1\nobjectif : Un test\npreuve : cargo test\n\ntache a\n    Ecrire a.txt\n").unwrap();
    let v = "1761901500000_11_0";
    let evenements = [
        Evenement::Vague { vague: v.into(), n: 1, depot: "depot".into(), verifier: None, objectif: "Un test".into() },
        Evenement::Debut { vague: v.into(), main: 1, tentative: 1 },
        Evenement::Fin {
            vague: v.into(),
            main: 1,
            tentative: 1,
            couleur: Couleur::Rouge,
            foi: Foi::Declare,
            texte: "IMPOSSIBLE: le test se contredit".into(),
            verif: None,
        },
        Evenement::Verdict { vague: v.into(), main: 1, verdict: Verdict::Impossible, foi: Foi::Declare },
    ];
    let vagues = etat(&evenements);
    let notes: Vec<fil::Note> = evenements
        .iter()
        .zip(1..)
        .map(|(evenement, seq)| fil::Note {
            seance: "s".into(),
            seq,
            instant: "2026-10-31T21:00:00.000Z".into(),
            origine: "humain".into(),
            origine_foi: Foi::Declare,
            evenement: evenement.clone(),
        })
        .collect();
    let texte = corps_sans_reglage(&plan, &vagues, v, None, None, &notes);
    assert!(!texte.contains("le test se contredit"), "{texte}");
    assert!(texte.contains("\na : classée\n"), "{texte}");
    assert!(texte.contains("1 impossible"), "{texte}");
}

#[test]
fn le_corps_de_la_pr_dit_la_mesure_sur_une_ligne_a_part_quand_un_reglage_est_pose() {
    let plan = Plan::lire(b"plan 1\nobjectif : Un test\npreuve : cargo test\n\ntache a\n    Ecrire a.txt\n").unwrap();
    let v = "1761901500000_11_0";
    let corps_de = |mesure: &str| {
        let evenements = [
            Evenement::Vague { vague: v.into(), n: 1, depot: "d".into(), verifier: None, objectif: "Un test".into() },
            Evenement::Consigne {
                vague: v.into(),
                main: 1,
                tentative: 1,
                role: fil::Role::Main,
                consigne: "c".into(),
                systeme: None,
                lecons: String::new(),
                ecartees: String::new(),
                mesure: mesure.into(),
            },
        ];
        let notes: Vec<fil::Note> = evenements
            .iter()
            .zip(1..)
            .map(|(evenement, seq)| fil::Note {
                seance: "s".into(),
                seq,
                instant: "2026-10-31T21:00:00.000Z".into(),
                origine: "humain".into(),
                origine_foi: Foi::Declare,
                evenement: evenement.clone(),
            })
            .collect();
        corps_sans_reglage(&plan, &etat(&evenements), v, None, None, &notes)
    };
    assert!(!corps_de("").contains("Measurement"), "une vraie vague ne dit rien");
    let texte = corps_de("sans-lecture");
    assert!(texte.lines().any(|ligne| ligne == "- Measurement: sans-lecture."), "{texte}");
    let texte = corps_de("sans-lecture\nsans-a1");
    assert!(texte.lines().any(|ligne| ligne == "- Measurement: sans-lecture, sans-a1."), "{texte}");
    // La ligne est dans le bloc « Reçu », avant l'objectif du plan.
    assert!(texte.find("Measurement").unwrap() < texte.find("Un test").unwrap(), "{texte}");
}

#[test]
fn le_recu_separe_les_mains_refusees() {
    let v = "1761901500000_11_0";
    let refus = |main| Evenement::Refus {
        vague: v.into(),
        main,
        code: 7007,
        regle: "règle".into(),
        preuve: "preuve".into(),
        correction: "correction".into(),
        verif: None,
    };
    let recu = |evenements: &[Evenement]| {
        let notes: Vec<fil::Note> = evenements
            .iter()
            .zip(1..)
            .map(|(evenement, seq)| fil::Note {
                seance: "s".into(),
                seq,
                instant: "2026-10-31T21:00:00.000Z".into(),
                origine: "humain".into(),
                origine_foi: Foi::Declare,
                evenement: evenement.clone(),
            })
            .collect();
        recu_de(&etat(evenements), &notes, v).mains()
    };
    let debut = |main| Evenement::Debut { vague: v.into(), main, tentative: 1 };
    // Sans refus, la ligne ne change pas.
    assert_eq!(recu(&[debut(1)]), "1 main, 0 relance, 0 classée");
    // Une main refusée, puis deux.
    assert_eq!(recu(&[debut(1), refus(1)]), "1 main, 0 relance, 0 classée, 1 refusée");
    assert_eq!(recu(&[debut(1), debut(2), refus(1), refus(2)]), "2 mains, 0 relance, 0 classée, 2 refusées");
    // Une main refusée sans tentative compte aussi : son refus est son issue.
    assert_eq!(recu(&[refus(1)]), "0 main, 0 relance, 0 classée, 1 refusée");
}

#[test]
fn le_recu_compte_les_coupures_sans_en_faire_des_relances() {
    let v = "1761901500000_11_0";
    let recu = |evenements: &[Evenement]| {
        let notes: Vec<fil::Note> = evenements
            .iter()
            .zip(1..)
            .map(|(evenement, seq)| fil::Note {
                seance: "s".into(),
                seq,
                instant: "2026-10-31T21:00:00.000Z".into(),
                origine: "humain".into(),
                origine_foi: Foi::Declare,
                evenement: evenement.clone(),
            })
            .collect();
        recu_de(&etat(evenements), &notes, v).mains()
    };
    let debut = |main, tentative| Evenement::Debut { vague: v.into(), main, tentative };
    let coupure = |main, tentative| Evenement::Coupure { vague: v.into(), main, tentative, statut: 529 };
    let refus = |main| Evenement::Refus {
        vague: v.into(),
        main,
        code: 7007,
        regle: "règle".into(),
        preuve: "preuve".into(),
        correction: "correction".into(),
        verif: None,
    };
    // Sans coupure, la ligne ne change pas : la seconde tentative est une relance.
    assert_eq!(recu(&[debut(1, 1), debut(1, 2)]), "2 mains, 1 relance, 0 classée");
    // La tentative qui suit une coupure n'est pas une relance ; une coupure se dit au singulier, puis au pluriel.
    assert_eq!(recu(&[debut(1, 1), coupure(1, 1), debut(1, 2)]), "2 mains, 0 relance, 0 classée, 1 coupure");
    let deux = [debut(1, 1), coupure(1, 1), debut(1, 2), coupure(1, 2)];
    assert_eq!(recu(&deux), "2 mains, 0 relance, 0 classée, 2 coupures");
    // Une coupure, puis une relance : la tentative 2, qui suit la coupure, n'en est pas une ; la 3 en est une.
    let relancee = [debut(1, 1), coupure(1, 1), debut(1, 2), debut(1, 3)];
    assert_eq!(recu(&relancee), "3 mains, 1 relance, 0 classée, 1 coupure");
    // Une coupure qui n'a pas été suivie d'une tentative ne retire aucune relance à une autre main.
    let autre = [debut(1, 1), debut(1, 2), debut(2, 1), coupure(2, 1)];
    assert_eq!(recu(&autre), "3 mains, 1 relance, 0 classée, 1 coupure");
    // Après les refusées.
    assert_eq!(recu(&[debut(1, 1), coupure(1, 1), refus(1)]), "1 main, 0 relance, 0 classée, 1 refusée, 1 coupure");
}

#[test]
fn le_corps_dit_le_plan_sans_sortie_de_main_ni_chemin() {
    let racine = racine("forge-corps");
    let plan = Plan::lire(
        b"plan 1\nobjectif : Cinq fichiers\npreuve : cargo test\n\n\
          tache a\n    Ecrire a.txt,\n    en deux lignes\n\
          tache b\n    Ecrire b.txt\n\
          tache c\ndepend : b\n    Ecrire c.txt\n\
          tache d\n    Ecrire d.txt\n\
          tache e\n    Ecrire e.txt\n",
    )
    .unwrap();
    let v = "1761901500000_11_0";
    let cache = racine.display().to_string();
    let refus = |main: usize, code: u16| Evenement::Refus {
        vague: v.into(),
        main,
        code,
        regle: "Une règle qu'un autre écrivain a notée.".into(),
        preuve: format!("preuve qui nomme {cache}"),
        correction: "Corrige.".into(),
        verif: None,
    };
    // La vague nomme son dépôt, sous le dossier temporaire du test. a assemblée ; b classée, sa sortie commence comme
    // un refus et nomme un chemin ; c sautée (7011) ; d refusée en conflit (7007) ; e refusée sous un code que Kraken
    // ne connaît pas.
    let sortie = format!("refusée (7007) : une main qui le dit\n{cache}");
    let depot = racine.join("depot").to_string_lossy().into_owned();
    let debut = |main, tentative| Evenement::Debut { vague: v.into(), main, tentative };
    let evenements = [
        Evenement::Vague { vague: v.into(), n: 5, depot, verifier: None, objectif: "Cinq fichiers".into() },
        debut(1, 1),
        usage(v, 600_000, fil::Base::Estime),
        usage(v, 240_000, fil::Base::Forfait),
        usage(v, 0, fil::Base::Inconnu),
        Evenement::Fusion { vague: v.into(), main: 1, sha: TETE.into(), verif: "vert".into() },
        debut(2, 1),
        debut(2, 2),
        Evenement::Fin {
            vague: v.into(),
            main: 2,
            tentative: 2,
            couleur: Couleur::Rouge,
            foi: Foi::Constate,
            texte: sortie,
            verif: None,
        },
        Evenement::Verdict { vague: v.into(), main: 2, verdict: Verdict::Classee, foi: Foi::Constate },
        refus(3, 7011),
        refus(4, 7007),
        refus(5, 8999),
    ];
    let vagues = etat(&evenements);
    let notes: Vec<fil::Note> = evenements
        .iter()
        .zip(1..)
        .map(|(evenement, seq)| fil::Note {
            seance: "s".into(),
            seq,
            instant: "2026-10-31T21:00:00.000Z".into(),
            origine: "humain".into(),
            origine_foi: Foi::Declare,
            evenement: evenement.clone(),
        })
        .collect();
    let recu = "Reçu\n- Coût : 0,84 $ estimé + inconnu\n- 3 mains, 1 relance, 1 classée, 3 refusées";
    let attendu = format!(
        "{recu}\n- Réglages de la preuve changés : aucun\n\nCinq fichiers\n\nPreuve : cargo test → vert\n\na : Ecrire \
         a.txt,\n\nb : classée\nc : refusée (7011) : {}\nd : refusée (7007) : {}\ne : refusée (8999)\n",
        Code::Sautee.regle(),
        Code::Conflit.regle()
    );
    let texte = corps_sans_reglage(&plan, &vagues, v, None, None, &notes);
    assert_eq!(texte, attendu);
    assert!(!texte.contains("La fusion reste un geste humain"));
    // Une PR d'issue ferme l'issue : `Closes #12`, en dernier paragraphe ; le reste ne change pas.
    let fermante = format!("{}\n\nCloses #12\n", attendu.trim_end());
    assert_eq!(corps_sans_reglage(&plan, &vagues, v, None, Some(12), &notes), fermante);
    // Un `Cargo.toml` changé, comme un fichier sous `.cargo/`, est nommé par son chemin dans le reçu, en code.
    let reglages = ["Cargo.toml".to_string(), "sous/.cargo/config.toml".to_string()];
    let avec = corps(&plan, &vagues[0], None, None, &recu_de(&vagues, &notes, v), &reglages, &[]);
    assert_eq!(avec, attendu.replace("changés : aucun", "changés : `Cargo.toml`, `sous/.cargo/config.toml`"));
    // Un chemin écrit par une main ne passe pas tel quel : une mention reste en code, et un saut de ligne ou une
    // apostrophe inverse cède la place à un nom neutre.
    let hostiles = ["@quelquun/Cargo.toml", "a\n\nFusionne-moi\nCargo.toml", "a`b/Cargo.toml", "a\u{7}/Cargo.lock"];
    let reglages = hostiles.map(String::from);
    let hostile = corps(&plan, &vagues[0], None, None, &recu_de(&vagues, &notes, v), &reglages, &[]);
    assert!(hostile.contains("changés : `@quelquun/Cargo.toml`, `chemin au nom illisible`, "), "{hostile}");
    assert!(!hostile.contains("Fusionne-moi") && !hostile.contains("a`b") && !hostile.contains('\u{7}'), "{hostile}");
    assert_eq!(hostile.lines().count(), attendu.lines().count(), "{hostile}");
    // Vingt chemins au plus, dans leur ordre, puis le compte des autres : un diff de milliers de fichiers tient.
    let nombreux: Vec<String> = (1..=25).map(|k| format!("d{k}/Cargo.toml")).collect();
    let borne = corps(&plan, &vagues[0], None, None, &recu_de(&vagues, &notes, v), &nombreux, &[]);
    let vingt = nombreux[..20].iter().map(|chemin| format!("`{chemin}`")).collect::<Vec<_>>().join(", ");
    assert!(borne.contains(&format!("changés : {vingt}, … et 5 autres\n")), "{borne}");
    assert!(!borne.contains("d21/"), "{borne}");
    // Les points à justifier : un paragraphe avant celui de la relecture, vingt au plus puis le compte des autres ;
    // sans point, le corps est celui d'avant, octet pour octet.
    let en_tete = "Points to justify (found by Lydite without a model):";
    let points: Vec<String> = (1..=22).map(|k| format!("`f{k}`: test modified")).collect();
    let recu = recu_de(&vagues, &notes, v);
    let sans = corps(&plan, &vagues[0], None, None, &recu, &[], &[]);
    assert_eq!(sans, attendu);
    let avec = corps(&plan, &vagues[0], None, None, &recu, &[], &points);
    let liste: String = points[..20].iter().map(|point| format!("\n- {point}")).collect();
    assert_eq!(avec, format!("{}\n\n{en_tete}{liste}\n- … and 2 more\n", attendu.trim_end()));
    let relue = "Relecture : 1 remarques, dont 0 bloquantes.";
    let sans = corps(&plan, &vagues[0], Some(relue), None, &recu, &[], &[]);
    assert_eq!(sans, format!("{}\n\n{relue}\n", attendu.trim_end()));
    let avec = corps(&plan, &vagues[0], Some(relue), None, &recu, &[], &points[..1]);
    let un = format!("{}\n\n{en_tete}\n- {}\n\n{relue}\n", attendu.trim_end(), points[0]);
    assert_eq!(avec, un);
    let temporaire = env::temp_dir().to_string_lossy().into_owned();
    let absents = [cache.as_str(), temporaire.as_str(), "une main qui le dit", "autre écrivain", "preuve qui nomme"];
    for absent in absents {
        assert!(!texte.contains(absent), "le corps est public, sans {absent:?}");
    }
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn le_corps_d_une_vague_decoupee_nomme_ses_taches_par_leur_id_et_dit_la_relecture_en_nombres() {
    // Trois tâches : a et b assemblées, c classée. Les objectifs des tâches d'un plan découpé sont écrits par une
    // main :
    // le corps ne les reprend pas.
    let plan = Plan::lire(
        b"plan 1\nobjectif : Trois fichiers\npreuve : cargo test\n\n\
          tache a\n    Ecrire a.txt, SECRET-DE-LA-MAIN\ntache b\n    Ecrire b.txt\ntache c\n    Ecrire c.txt\n",
    )
    .unwrap();
    let (v, d, r, c) = ("1761901500000_11_0", "1761901400000_11_0", "1761901600000_11_0", "1761901700000_11_0");
    let fusion = |main| Evenement::Fusion { vague: v.into(), main, sha: TETE.into(), verif: "vert".into() };
    let ouverte = Evenement::Vague {
        vague: v.into(),
        n: 3,
        depot: "/srv/depot".into(),
        verifier: None,
        objectif: "Trois fichiers".into(),
    };
    let classee = Evenement::Verdict { vague: v.into(), main: 3, verdict: Verdict::Classee, foi: Foi::Constate };
    let joue = Evenement::Joue { vague: v.into(), decoupage: d.into() };
    let relecture = |n, bloquantes| Evenement::Relecture {
        vague: r.into(),
        relue: v.into(),
        n,
        bloquantes,
        remarques: "remarques 1\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    SECRET-DE-LA-RELECTURE".into(),
    };
    let correction = Evenement::Correction { vague: c.into(), corrige: v.into() };
    let corps_de = |notes: &[Evenement]| {
        let vagues = etat(notes);
        corps_sans_reglage(&plan, &vagues, v, ligne_de_relecture(&vagues, v).as_deref(), None, &[])
    };
    let fin = "\n";
    let entete = "Reçu\n- Coût : inconnu\n- 0 main, 0 relance, 1 classée\n- Réglages de la preuve changés : aucun\n\n\
                  Trois fichiers\n\nPreuve : cargo test → vert\n\n";
    // Découpée, relue, corrigée : les ids seuls, ce qui est dehors dit son état, puis la relecture en nombres.
    let notes = [ouverte.clone(), joue.clone(), fusion(1), fusion(2), classee.clone(), relecture(3, 1), correction];
    let corrigee =
        format!("{entete}a\nb\n\nc : classée\n\nRelecture : 3 remarques, dont 1 bloquantes, corrigées.{fin}");
    assert_eq!(corps_de(&notes), corrigee);
    // Relue sans bloquante : la ligne ne dit pas « corrigées ».
    let notes = [ouverte.clone(), joue.clone(), fusion(1), fusion(2), classee.clone(), relecture(3, 0)];
    assert_eq!(
        corps_de(&notes),
        format!("{entete}a\nb\n\nc : classée\n\nRelecture : 3 remarques, dont 0 bloquantes.{fin}")
    );
    // Découpée et jamais relue : pas de ligne de relecture.
    let notes = [ouverte.clone(), joue, fusion(1), fusion(2), classee.clone()];
    assert_eq!(corps_de(&notes), format!("{entete}a\nb\n\nc : classée{fin}"));
    // Écrite à la main, relue : les objectifs des tâches sont ceux de l'humain, et restent dans le corps, avec la
    // ligne.
    let notes = [ouverte, fusion(1), fusion(2), classee, relecture(2, 0)];
    let a_la_main = format!(
        "{entete}a : Ecrire a.txt, SECRET-DE-LA-MAIN\nb : Ecrire b.txt\n\nc : classée\n\nRelecture : 2 remarques, \
         dont 0 bloquantes.{fin}"
    );
    assert_eq!(corps_de(&notes), a_la_main);
    // Jamais le texte des remarques, ni celui d'une main d'un plan découpé.
    for texte in [corrigee, a_la_main] {
        assert!(!texte.contains("SECRET-DE-LA-RELECTURE"));
    }
    assert!(
        !corps_de(&[Evenement::Joue { vague: v.into(), decoupage: d.into() }, fusion(1)]).contains("SECRET-DE-LA-MAIN")
    );
}

#[test]
fn le_recu_ferme_les_liens_de_proche_en_proche_et_s_arrete_sur_une_boucle() {
    let (v, c1, c2) = ("1761901100000_7_0", "1761901200000_7_0", "1761901300000_7_0");
    let ouvertes = [v, c1, c2].map(|id| crate::essais::ouverture(id, 1, "o"));
    let corrige = |vague: &str, corrige: &str| Evenement::Correction { vague: vague.into(), corrige: corrige.into() };
    let usages = [usage(v, 1_000_000, fil::Base::Estime), usage(c1, 2_000_000, fil::Base::Estime)];
    let notes_de = |evenements: &[Evenement]| -> Vec<fil::Note> {
        let notes = evenements.iter().zip(1..);
        let note = |(evenement, seq): (&Evenement, u64)| fil::Note {
            seance: "s".into(),
            seq,
            instant: "2026-10-31T21:00:00.000Z".into(),
            origine: "humain".into(),
            origine_foi: Foi::Declare,
            evenement: evenement.clone(),
        };
        notes.map(note).collect()
    };
    // C1 corrige V, C2 corrige C1 : deux degrés, et le reçu de V compte les trois vagues.
    let chaine = [
        ouvertes.to_vec(),
        vec![corrige(c1, v), corrige(c2, c1)],
        usages.to_vec(),
        vec![usage(c2, 3_000_000, fil::Base::Estime)],
    ]
    .concat();
    let (vagues, notes) = (etat(&chaine), notes_de(&chaine));
    assert_eq!(recu_de(&vagues, &notes, v).cout(), "6,00 $ estimé");
    // Le lien va de la vague vers ses corrections : la correction C1 ne compte que C2 de plus qu'elle.
    assert_eq!(recu_de(&vagues, &notes, c1).cout(), "5,00 $ estimé");
    // Une boucle (V corrige C2) s'arrête : chaque vague est comptée une fois.
    let boucle = [chaine, vec![corrige(v, c2)]].concat();
    let (vagues, notes) = (etat(&boucle), notes_de(&boucle));
    for id in [v, c1, c2] {
        assert_eq!(recu_de(&vagues, &notes, id).cout(), "6,00 $ estimé", "{id}");
    }
}

#[test]
fn les_reglages_de_la_preuve_se_reconnaissent_a_toute_profondeur() {
    let changes = [
        "Cargo.toml",
        "crates/a/Cargo.toml",
        "Cargo.lock",
        "src/build.rs",
        ".cargo/config.toml",
        "a/.cargo/sous/config",
        "rustfmt.toml",
        ".rustfmt.toml",
        "x/clippy.toml",
        ".clippy.toml",
        "rust-toolchain",
        "rust-toolchain.toml",
    ];
    assert_eq!(reglages_changes(changes), changes.map(String::from));
    // Ni un autre fichier, ni un nom qui n'est que voisin, ni un fichier `.cargo` : seul un dossier du même nom compte.
    let autres =
        ["src/main.rs", "README.md", "Cargo.toml.bak", "mon-Cargo.toml", "src/cargo/x.rs", "a/.cargo", "build.rs.md"];
    assert_eq!(reglages_changes(autres), Vec::<String>::new());
}

/// Le délai de la preuve des fausses mains, et le bail de leurs tentatives : bien au-delà de ce qu'elles prennent.
const PREUVE: Duration = Duration::from_secs(30);
const BAIL: Duration = Duration::from_secs(600);

/// Une preuve de tâche qui écrit `.github/ci.yml` dans son bac, où la vague le gèle avec le travail de la main.
const ECRIT_LA_CI: &str =
    if cfg!(windows) { r"mkdir .github && echo x> .github\ci.yml" } else { "mkdir .github && echo x > .github/ci.yml" };

/// Une preuve de tâche qui commite dans son bac sous une autre identité que celle de Kraken, posée sur la seule
/// commande de git : l'environnement du test n'y change rien.
const COMMITE_EN_ETRANGER: &str = if cfg!(windows) {
    "set GIT_AUTHOR_NAME=autre&& set GIT_AUTHOR_EMAIL=autre@exemple.invalid&& set GIT_COMMITTER_NAME=autre&& \
     set GIT_COMMITTER_EMAIL=autre@exemple.invalid&& git -c commit.gpgsign=false commit -q --allow-empty -m etranger"
} else {
    "GIT_AUTHOR_NAME=autre GIT_AUTHOR_EMAIL=autre@exemple.invalid GIT_COMMITTER_NAME=autre \
     GIT_COMMITTER_EMAIL=autre@exemple.invalid git -c commit.gpgsign=false commit -q --allow-empty -m etranger"
};

/// Un plan, lu dans ses lignes.
fn plan(lignes: &[&str]) -> Plan {
    Plan::lire(lignes.join("\n").as_bytes()).unwrap()
}

/// Les mots de ces notes de forge, dans l'ordre : `poussée`, `pr 12`, `ci Vert`, `refus 8004`, `rangement`.
fn mots(notes: &[Evenement]) -> Vec<String> {
    let mot = |note: &Evenement| match note {
        Evenement::Poussee { .. } => "poussée".to_string(),
        Evenement::Pr { numero, .. } => format!("pr {numero}"),
        Evenement::Ci { couleur, .. } => format!("ci {couleur:?}"),
        Evenement::RefusForge { code, .. } => format!("refus {code}"),
        Evenement::Rangement { .. } => "rangement".into(),
        autre => format!("{autre:?}"),
    };
    notes.iter().map(mot).collect()
}

impl Lieu {
    /// La proposition de `--pr main`, lue comme avant une vague : `gh auth status`, l'appel 1 du faux `gh`, réussit
    /// faute d'autre consigne.
    fn proposition(&self) -> Proposition<'_> {
        avant_la_vague(&self.gh.programme, &self.kraken, &self.depot, "main").unwrap()
    }

    /// La vague de ce plan, partie de la base de la proposition : chaque main écrit sa consigne dans son bac.
    fn vague(&self, proposition: &Proposition, plan: &Plan) -> Menee {
        let profil = Profil::faux(main_de_vague(&self.racine), PREUVE);
        lancer_sur(&self.depot, &self.kraken, &profil, plan, BAIL, &proposition.base).unwrap()
    }

    /// La forge après cette vague, sans relance attendue : une CI qui n'est pas rouge, ou un refus avant la relance.
    fn apres(&self, proposition: &Proposition, plan: &Plan, menee: &Menee, delai: Duration) -> ExitCode {
        let aucune = |_: &Plan, _: &str| -> Result<Menee, Refus> { panic!("aucune relance n'est attendue") };
        self.apres_avec(proposition, plan, menee, &aucune, delai)
    }

    /// La forge après cette vague, qui relance par `lancer` : la racine du test y tient lieu de dossier personnel, et
    /// la CI se lit toutes les 10 ms, `delai` au plus.
    fn apres_avec(
        &self,
        proposition: &Proposition,
        plan: &Plan,
        menee: &Menee,
        lancer: &dyn Fn(&Plan, &str) -> Result<Menee, Refus>,
        delai: Duration,
    ) -> ExitCode {
        let ci = (Duration::from_millis(10), delai);
        proposition.apres_la_vague(plan, menee, None, lancer, Some(&self.racine), ci)
    }

    /// La relance, comme `lancer_sur` la mène avec cette fausse main, sur la tête qu'on lui donne. La tête qu'elle
    /// assemble n'est connue qu'alors : le faux `gh` rend sa CI, à ses appels `n` à `n + 2`, pour ces `runs`, une
    /// fois la relance assemblée.
    fn relance(&self, main: Vec<String>, n: usize, runs: Vec<String>) -> impl Fn(&Plan, &str) -> Result<Menee, Refus> {
        let profil = Profil::faux(main, PREUVE);
        move |plan, base| {
            let menee = lancer_sur(&self.depot, &self.kraken, &profil, plan, BAIL, base)?;
            if let Some(tete) = &menee.tete {
                rendre_ci(&self.gh, n, tete, &runs, &[]);
            }
            Ok(menee)
        }
    }

    /// Le faux `gh` ouvre la PR 12 à son appel `n`, la relit sur `tete` au suivant, puis rend sa CI aux trois
    /// suivants : ouverte, sans conflit, ses runs dans `runs`, sans statut.
    fn ouvre(&self, n: usize, tete: &str, runs: &[String]) {
        self.gh.rendre(n, 0, &format!("{PR}\n"), "");
        self.gh.rendre(n + 1, 0, &pr_vue(tete), "");
        rendre_ci(&self.gh, n + 2, tete, runs, &[]);
    }

    /// La vague de ce plan, et sa PR 12 (appels 2 à 6 du faux `gh`) dont la CI est rouge : un run passe, un échoue,
    /// et le journal de son run en échec (appels 7 et 8) finit par `test b ... FAILED`.
    fn vague_rouge(&self, proposition: &Proposition, plan: &Plan) -> Menee {
        let menee = self.vague(proposition, plan);
        let tete = menee.tete.clone().unwrap();
        self.ouvre(
            2,
            &tete,
            &[run("test (linux)", "completed", "success"), run("test (windows)", "completed", "failure")],
        );
        self.gh.rendre(7, 0, r#"[{"conclusion":"failure","databaseId":8}]"#, "");
        self.gh.rendre(8, 0, "test b ... FAILED\n", "");
        menee
    }

    /// Tous les événements du fil, dans son ordre, qui se lit sans écart.
    fn fil(&self) -> Vec<Evenement> {
        let fil::Lecture { notes, ecarts } = dossier::lire(&self.kraken).unwrap();
        assert!(ecarts.is_empty(), "{ecarts:?}");
        notes.into_iter().map(|note| note.evenement).collect()
    }

    /// Les notes de forge de la vague `vague`, dans l'ordre du fil, qui se lit sans écart.
    fn forge_du_fil(&self, vague: &str) -> Vec<Evenement> {
        let de_forge = |note: &Evenement| {
            matches!(
                note,
                Evenement::Poussee { .. }
                    | Evenement::Pr { .. }
                    | Evenement::Ci { .. }
                    | Evenement::RefusForge { .. }
                    | Evenement::Rangement { .. }
            )
        };
        self.fil().into_iter().filter(|note| note.vague() == vague && de_forge(note)).collect()
    }

    /// Les id des vagues que le fil ouvre, dans l'ordre : la vague, puis sa relance s'il y en a une.
    fn vagues(&self) -> Vec<String> {
        let ouvertes = self.fil().into_iter().filter_map(|note| match note {
            Evenement::Vague { vague, .. } => Some(vague),
            _ => None,
        });
        ouvertes.collect()
    }

    /// La tête que la dernière fusion de la vague `vague` a notée.
    fn tete_de(&self, vague: &str) -> String {
        let mut fusions = self.fil().into_iter().filter_map(|note| match note {
            Evenement::Fusion { vague: v, sha, .. } if v == vague => Some(sha),
            _ => None,
        });
        fusions.next_back().expect("la vague a une fusion")
    }

    /// Ce que `lydite etat` dit de ce fil, et son code de sortie.
    fn etat(&self) -> (String, ExitCode) {
        let dit = crate::commande::etat::rendu(&etat(&self.fil()), "2099-01-01T00:00:00.000Z");
        // La ligne de reçu a ses propres essais : ici, le reste du texte.
        let dit = dit.lines().filter(|ligne| !ligne.starts_with("Coût : ")).collect::<Vec<_>>().join("\n");
        (dit, crate::commande::etat::etat_dans(&self.kraken).unwrap())
    }

    /// Les branches de Kraken du distant nu.
    fn poussees(&self) -> String {
        git(&self.nu, &["branch", "--list", "lydite/*"])
    }
}

#[test]
fn une_pr_verte_part_de_la_base_distante_puis_range_ses_bacs() {
    let lieu = lieu("forge-verte");
    let base = lieu.base();
    // Un commit local, jamais poussé : HEAD n'est plus la base distante, et une ligne le dit.
    let local = sur(&lieu.depot, &base, &[("local.txt", "non poussé")], "local");
    let proposition = lieu.proposition();
    assert_eq!(proposition.base, base);
    let ecart = proposition.ecart_de_head().expect("HEAD n'est pas la base distante");
    assert!(ecart.contains(&local) && ecart.contains(&base) && ecart.contains(" main "), "{ecart}");
    let plan = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : exit 0",
        "tache a",
        "    Ecrire a",
        "tache b",
        "depend : a",
        "    Ecrire b",
    ]);
    let menee = lieu.vague(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    lieu.ouvre(2, &tete, &[run("test", "completed", "success")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    // La branche de la vague est poussée sur sa tête, et la PR ouverte contre main, avec son corps.
    let branche = format!("lydite/vague-{}", menee.id);
    assert_eq!((lieu.distante(&branche), git(&lieu.depot, &["rev-parse", &branche])), (tete.clone(), tete.clone()));
    let mut attendus = vec![
        "auth status --hostname forge.test".to_string(),
        format!("pr create --base main --head {branche} --title Ecrire a et b --body-file - --repo forge.test/p/d"),
        format!("pr view {branche} --json number,url,headRefOid --repo forge.test/p/d"),
    ];
    attendus.extend(tour_de_ci(&tete));
    assert_eq!(lieu.gh.appels(), attendus);
    assert_eq!(lieu.appels_bien_partis(), 6);
    // La séance de la forge a scellé le fil juste avant le corps, et le reçu cite ce sceau.
    let sceaux = crate::branchements::dossier::lire_sceaux(&lieu.kraken);
    assert_eq!(sceaux.len(), 1, "{sceaux:?}");
    let (octets, empreinte) = (sceaux[0].octets, &sceaux[0].empreinte);
    let corps = format!(
        "Reçu\n- Coût : inconnu\n- 2 mains, 0 relance, 0 classée\n- Fil sealed at {octets} bytes: sha256 {empreinte}.\n\
         - Réglages de la preuve changés : aucun\n\n\
         Ecrire a et b\n\nPreuve : exit 0 → vert\n\na : Ecrire a\nb : Ecrire b\n\n\
         Replayed on commit {tete}, not reviewed: no critical task was assembled.\n"
    );
    assert_eq!(lieu.gh.entree(2), corps);
    // Le commit local n'est pas dans la PR : ni dans l'histoire de la tête poussée, ni dans son arbre.
    let mut dedans = Command::new("git");
    dedans.current_dir(&lieu.depot).args(["merge-base", "--is-ancestor", &local, &tete]);
    assert!(!dedans.status().unwrap().success(), "le commit local n'entre pas dans la PR");
    let arbre = git(&lieu.depot, &["ls-tree", "-r", "--name-only", &tete]);
    assert!(!arbre.contains("local.txt"), "{arbre}");
    // Au fil, dans l'ordre : la poussée, la PR, la CI, le rangement, qui nomme ce qu'il retire.
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["poussée", "pr 12", "ci Vert", "rangement"]);
    let bacs = lieu.kraken.join("bacs");
    let mut retires = Vec::new();
    for (id, _) in &menee.issues {
        let bac = bacs.join(id);
        let temporaire = temporaire_de(&bac).unwrap();
        retires.extend([bac.display().to_string(), temporaire.display().to_string(), format!("lydite/{id}")]);
    }
    retires.push(bacs.join(format!("vague-{}", menee.id)).display().to_string());
    assert!(matches!(&notes[3], Evenement::Rangement { retires: vus, .. } if *vus == retires), "{notes:?}");
    // Tout est retiré : les bacs, leurs dossiers temporaires, leurs branches et le worktree de la vague. Sa branche
    // reste, qui porte la PR.
    assert_eq!(fs::read_dir(&bacs).unwrap().count(), 0, "plus rien sous bacs");
    let branches = git(&lieu.depot, &["for-each-ref", "--format=%(refname)", "refs/heads/lydite"]);
    assert_eq!(branches, format!("refs/heads/{branche}"));
    assert_eq!(git(&lieu.depot, &["worktree", "list", "--porcelain"]).matches("worktree ").count(), 1);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_vague_qui_change_un_cargo_toml_le_nomme_en_code_dans_le_corps_de_la_pr() {
    let lieu = lieu("forge-reglage");
    let proposition = lieu.proposition();
    let lignes = ["plan 1", "objectif : Changer Cargo.toml", "preuve : exit 0", "tache a", "    Ecrire Cargo.toml"];
    let plan = plan(&lignes);
    // Une fausse main qui écrit `Cargo.toml` à la racine de son bac, de sa consigne.
    let main = if cfg!(windows) {
        let bat = lieu.racine.join("reglage.bat");
        fs::write(&bat, "@echo off\r\nfindstr \"^\" > Cargo.toml\r\necho PREUVE: vert\r\n").unwrap();
        vec![bat.to_string_lossy().into_owned()]
    } else {
        let sh = lieu.racine.join("reglage.sh");
        fs::write(&sh, "cat > Cargo.toml\necho 'PREUVE: vert'\n").unwrap();
        vec!["sh".into(), sh.to_string_lossy().into_owned()]
    };
    let profil = Profil::faux(main, PREUVE);
    let menee = lancer_sur(&lieu.depot, &lieu.kraken, &profil, &plan, BAIL, &proposition.base).unwrap();
    lieu.ouvre(2, &menee.tete.clone().unwrap(), &[run("test", "completed", "success")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    // Le corps transmis à `gh` nomme le réglage changé, en code.
    let corps = lieu.gh.entree(2).replace("\r\n", "\n");
    assert!(corps.contains("\n- Réglages de la preuve changés : `Cargo.toml`\n"), "{corps}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn le_titre_d_une_pr_trop_longue_finit_par_une_ellipse() {
    let lieu = lieu("forge-titre");
    let proposition = lieu.proposition();
    // 250 a, puis des é : une coupe au 256e octet tomberait au milieu d'un é. Le 255e caractère, le dernier gardé
    // (b), et le 256e, le premier ôté (c), sont ASCII : sous Windows, le faux gh les note tels quels, et le « … »
    // final (0x85 en page 1252) ne se confond pas avec un é.
    let objectif = format!("{}{}bc{}fin", "a".repeat(250), "é".repeat(4), "é".repeat(10));
    let entete = format!("objectif : {objectif}");
    let plan = plan(&["plan 1", &entete, "preuve : exit 0", "tache a", "    Ecrire a"]);
    let menee = lieu.vague(&proposition, &plan);
    lieu.ouvre(2, &menee.tete.clone().unwrap(), &[run("test", "completed", "success")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    // Le titre : les 255 premiers caractères de l'objectif, puis « … », 256 caractères en tout.
    let titre = format!("{}{}b…", "a".repeat(250), "é".repeat(4));
    assert_eq!(titre.chars().count(), 256);
    assert!(!objectif.starts_with(&titre), "le titre n'est pas l'objectif entier");
    // Sous Windows, le faux gh note ses arguments dans la page de code de la console : chaque caractère hors ASCII
    // y est lu comme un é ; sous Linux, il les note en UTF-8. La comparaison passe donc par la même conversion des
    // deux côtés, et par les caractères tels quels hors Windows.
    let appels = String::from_utf8_lossy(&fs::read(lieu.racine.join("gh").join("appels.txt")).unwrap()).into_owned();
    let create = appels.lines().nth(1).unwrap().replace('"', "");
    let lu = create.split_once(" --title ").and_then(|(_, reste)| reste.split_once(" --body-file ")).unwrap().0;
    let comme_gh_de_windows =
        |texte: &str| -> String { texte.chars().map(|c| if c.is_ascii() { c } else { 'é' }).collect() };
    assert_eq!(comme_gh_de_windows(lu), comme_gh_de_windows(&titre));
    if !cfg!(windows) {
        assert_eq!(lu, titre);
    }
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Le plan d'une seule tâche, a, que le faux gh de ces tests laisse sans autre histoire.
fn plan_de_a() -> Plan {
    plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"])
}

/// Le contexte que la CI rouge de `vague_rouge` donne à la relance : son check en échec, puis la fin de son journal.
const CONTEXTE: &str = "test (windows) : FAILURE\ntest b ... FAILED";

#[test]
fn une_ci_rouge_vaut_une_relance_qui_avance_la_branche_de_la_pr() {
    let lieu = lieu("forge-relance");
    let proposition = lieu.proposition();
    let plan = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : exit 0",
        "tache a",
        "    Ecrire a",
        "tache b",
        "depend : a",
        "    Ecrire b",
    ]);
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    // La relance rend la CI verte : le faux gh la rend à ses appels 9 à 11, sur la tête que la relance assemble.
    let verte = lieu.relance(main_de_vague(&lieu.racine), 9, vec![run("test (windows)", "completed", "success")]);
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &verte, Duration::from_secs(60)), ExitCode::SUCCESS);
    // Une seule relance : une vague neuve, partie de la tête poussée, d'une tâche ci dont l'objectif porte le
    // contexte, le check en échec puis la fin du journal, et non le check qui passe.
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let (fil, relance) = (lieu.fil(), &vagues[1]);
    let objectif = fil.iter().find_map(|note| match note {
        Evenement::Vague { vague, objectif, .. } if vague == relance => Some(objectif.as_str()),
        _ => None,
    });
    assert_eq!(objectif, Some("Rendre verte la CI de la PR 12"));
    let partie_de_la_tete = fil.iter().any(|note| {
        matches!(note, Evenement::Assemblage { vague, base, preuve, .. }
            if vague == relance && *base == tete && preuve == "exit 0")
    });
    assert!(partie_de_la_tete, "la relance part de la tête poussée {tete}");
    let tache = fil.iter().find_map(|note| match note {
        Evenement::Tache { vague, id, objectif, .. } if vague == relance => Some((id.as_str(), objectif)),
        _ => None,
    });
    let attendu = format!("{CORRIGER}\n{CONTEXTE}");
    assert_eq!(tache.map(|(id, objectif)| (id, objectif.replace("\r\n", "\n"))), Some(("ci", attendu)));
    // Elle pousse sa tête dans la branche de la PR, en avance rapide : la tête poussée en est un ancêtre. La branche
    // locale de la vague avance avec elle, et la main a reçu le contexte.
    let t2 = lieu.tete_de(relance);
    assert_ne!(t2, tete);
    git(&lieu.nu, &["merge-base", "--is-ancestor", &tete, &t2]);
    assert_eq!((lieu.distante(&branche), git(&lieu.depot, &["rev-parse", &branche])), (t2.clone(), t2.clone()));
    let consigne = git(&lieu.nu, &["show", &format!("{t2}:main-{relance}-1.txt")]).replace("\r\n", "\n");
    let (socle, consigne) = consigne.split_once("\n\n").unwrap();
    let debut_du_socle = "A refusal from the wall or from your profile is not worked around another way.\n";
    assert!(socle.starts_with(debut_du_socle), "{socle}");
    assert!(consigne.starts_with(CORRIGER) && consigne.contains(CONTEXTE), "{consigne}");
    // Le faux gh : la CI rouge et son journal, puis la CI de la relance, sur la même PR, lue aux mêmes appels.
    let appels = lieu.gh.appels();
    assert_eq!(appels.len(), 11, "{appels:?}");
    assert_eq!(appels[3..6], tour_de_ci(&tete));
    assert_eq!(appels[6], format!("run list --commit {tete} --json databaseId,conclusion --repo forge.test/p/d"));
    assert_eq!(appels[7], "run view 8 --log-failed --repo forge.test/p/d");
    assert_eq!(appels[8..], tour_de_ci(&t2));
    assert_eq!(lieu.appels_bien_partis(), 11);
    // Au fil : la vague note sa poussée, sa PR et sa CI rouge ; la relance, la sienne sous son id, avec la branche et
    // le numéro de la PR qu'elle relance.
    let (de_la_vague, de_la_relance) = (lieu.forge_du_fil(&menee.id), lieu.forge_du_fil(relance));
    assert_eq!(mots(&de_la_vague), ["poussée", "pr 12", "ci Rouge", "rangement"]);
    assert_eq!(mots(&de_la_relance), ["poussée", "pr 12", "ci Vert", "rangement"]);
    let Evenement::Ci { texte, .. } = &de_la_vague[2] else { panic!("{de_la_vague:?}") };
    assert_eq!(texte.replace("\r\n", "\n"), "test (linux) : SUCCESS\ntest (windows) : FAILURE\ntest b ... FAILED\n");
    let [poussee, pr, ci, Evenement::Rangement { retires, .. }] = &de_la_relance[..] else {
        panic!("{de_la_relance:?}")
    };
    assert_eq!(poussee, &Evenement::Poussee { vague: relance.clone(), branche: branche.clone(), sha: t2.clone() });
    let (url, foi) = (PR.to_string(), Foi::Constate);
    let base = "main".to_string();
    assert_eq!(
        pr,
        &Evenement::Pr {
            vague: relance.clone(),
            branche: branche.clone(),
            base,
            sha: t2.clone(),
            numero: 12,
            url,
            foi
        }
    );
    assert!(matches!(ci, Evenement::Ci { sha, numero: 12, couleur: Couleur::Vert, .. } if *sha == t2), "{ci:?}");
    // Tout se range, la relance comprise ; seule la branche de la PR reste. Le rangement de la relance nomme son
    // bac, son worktree et sa branche.
    let bacs = lieu.kraken.join("bacs");
    let bac = bacs.join(format!("main-{relance}-1"));
    let attendus = [
        bac.display().to_string(),
        temporaire_de(&bac).unwrap().display().to_string(),
        format!("lydite/main-{relance}-1"),
        bacs.join(format!("vague-{relance}")).display().to_string(),
        format!("lydite/vague-{relance}"),
    ];
    assert_eq!(*retires, attendus);
    assert_eq!(fs::read_dir(&bacs).unwrap().count(), 0, "plus rien sous bacs");
    let branches = git(&lieu.depot, &["for-each-ref", "--format=%(refname)", "refs/heads/lydite"]);
    assert_eq!(branches, format!("refs/heads/{branche}"));
    // `lydite etat` montre la vague relancée, puis sa relance : la CI rouge, puis la verte, et le code 0.
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::SUCCESS, "{dit}");
    let rouge = format!("PR 12 · {PR} · CI rouge (constatée)");
    let verte = format!("PR 12 · {PR} · CI verte (constatée)");
    let (de_rouge, de_verte) = (dit.find(&rouge), dit.find(&verte));
    assert!(dit.starts_with("Ecrire a et b\n") && de_rouge < de_verte && de_rouge.is_some(), "{dit}");
    assert!(dit.contains("\n\nRendre verte la CI de la PR 12\n"), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_tache_dehors_garde_le_code_1_meme_quand_la_relance_rend_la_ci_verte() {
    let lieu = lieu("forge-dehors");
    let proposition = lieu.proposition();
    assert_eq!(proposition.ecart_de_head(), None, "HEAD est la base distante");
    // a est assemblée ; b, dont la preuve est rouge, est classée, et reste dehors.
    let plan = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : exit 0",
        "tache a",
        "    Ecrire a",
        "tache b",
        "preuve : exit 3",
        "    Ecrire b",
    ]);
    let menee = lieu.vague_rouge(&proposition, &plan);
    let verte = lieu.relance(main_de_vague(&lieu.racine), 9, vec![run("test (windows)", "completed", "success")]);
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &verte, Duration::from_secs(60)), ExitCode::FAILURE);
    // La CI rouge se relance, puis la verte se note : ni refus, mais la tâche dehors garde le code à 1.
    let relance = lieu.vagues()[1].clone();
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["poussée", "pr 12", "ci Rouge", "rangement"]);
    assert_eq!(mots(&lieu.forge_du_fil(&relance)), ["poussée", "pr 12", "ci Vert", "rangement"]);
    assert!(lieu.gh.entree(2).contains("\n\nb : classée\n"), "{}", lieu.gh.entree(2));
    // Le reçu de la PR compte les mains de la vague, sa relance, sa tâche classée.
    assert!(lieu.gh.entree(2).starts_with("Reçu\n- Coût : inconnu\n- 3 mains, 1 relance, 1 classée\n"));
    // a est rangée, et rien de b ne se range : il garde son bac avec son travail, son dossier temporaire et sa
    // branche.
    let bacs = lieu.kraken.join("bacs");
    let (a, b) = (&menee.issues[0].0, &menee.issues[1].0);
    let (bac_de_a, vague) = (bacs.join(a), bacs.join(format!("vague-{}", menee.id)));
    let temporaire_de_a = temporaire_de(&bac_de_a).unwrap();
    let retires = [
        bac_de_a.display().to_string(),
        temporaire_de_a.display().to_string(),
        format!("lydite/{a}"),
        vague.display().to_string(),
    ];
    assert!(matches!(&notes[3], Evenement::Rangement { retires: vus, .. } if *vus == retires), "{notes:?}");
    assert!(!bac_de_a.exists() && !temporaire_de_a.exists() && !vague.exists());
    assert!(bacs.join(b).join(format!("{b}.txt")).is_file(), "le travail de b reste dans son bac");
    assert!(temporaire_de(&bacs.join(b)).unwrap().is_dir());
    assert_eq!(git(&lieu.depot, &["branch", "--list", &format!("lydite/{a}")]), "");
    assert_ne!(git(&lieu.depot, &["branch", "--list", &format!("lydite/{b}")]), "");
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    assert!(dit.contains(&format!("main-{}-2 b : classée", menee.id)), "{dit}");
    assert!(dit.ends_with(&format!("PR 12 · {PR} · CI verte (constatée)")), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn la_garde_rejouee_sur_la_poussee_d_une_relance_de_ci_refuse_8005() {
    let lieu = lieu("forge-relance-garde");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    // La garde naît après la première poussée : le contexte de la CI rouge, que la consigne de la relance porte dans
    // son bac, n'était pas encore écrit.
    let garde = lieu.kraken.join("garde");
    fs::write(&garde, "# la relance\ntest b ... FAILED\n").unwrap();
    let verte = lieu.relance(main_de_vague(&lieu.racine), 9, vec![run("test (windows)", "completed", "success")]);
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &verte, Duration::from_secs(60)), ExitCode::FAILURE);
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let notes = lieu.forge_du_fil(&vagues[1]);
    assert_eq!(mots(&notes), ["refus 8005"]);
    let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
    assert!(preuve.starts_with("la ligne 2 de la garde paraît dans les lignes ajoutées de b/"), "{preuve}");
    assert!(!preuve.contains(&garde.display().to_string()), "{preuve}");
    // Rien de la relance n'est poussé : la branche de la PR reste à la tête de la vague.
    assert_eq!(lieu.distante(&branche), tete);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_ci_rouge_deux_fois_est_8011_et_la_pr_reste_ouverte_et_rouge() {
    let lieu = lieu("forge-rouge-deux-fois");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague_rouge(&proposition, &plan);
    let branche = format!("lydite/vague-{}", menee.id);
    // La CI de la relance est rouge encore (appels 9 à 11), avec un journal à elle (appels 12 et 13).
    lieu.gh.rendre(12, 0, r#"[{"conclusion":"failure","databaseId":9}]"#, "");
    lieu.gh.rendre(13, 0, "test a ... FAILED encore\n", "");
    let rouge = lieu.relance(main_de_vague(&lieu.racine), 9, vec![run("test (windows)", "completed", "failure")]);
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &rouge, Duration::from_secs(60)), ExitCode::FAILURE);
    // Une seule relance, dont le refus 8011 se note sous elle, après sa CI rouge, avec son journal à elle.
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let (de_la_vague, de_la_relance) = (lieu.forge_du_fil(&menee.id), lieu.forge_du_fil(&vagues[1]));
    assert_eq!(mots(&de_la_vague), ["poussée", "pr 12", "ci Rouge", "rangement"]);
    assert_eq!(mots(&de_la_relance), ["poussée", "pr 12", "ci Rouge", "refus 8011", "rangement"]);
    let Evenement::Ci { texte, .. } = &de_la_relance[2] else { panic!("{de_la_relance:?}") };
    assert_eq!(texte.replace("\r\n", "\n"), "test (windows) : FAILURE\ntest a ... FAILED encore\n");
    let Evenement::RefusForge { code, regle, preuve, correction, .. } = &de_la_relance[3] else {
        panic!("{de_la_relance:?}")
    };
    assert_eq!((*code, regle.as_str()), (8011, Code::CiRouge.regle()));
    assert!(preuve.contains("rouge encore") && preuve.contains("test (windows) : FAILURE"), "{preuve}");
    assert!(correction.contains("ne la relance pas deux fois"), "{correction}");
    // Ce que la relance a poussé reste poussé, la forge range, et il n'y a pas de troisième tour.
    assert_eq!(lieu.distante(&branche), lieu.tete_de(&vagues[1]));
    assert_eq!(fs::read_dir(lieu.kraken.join("bacs")).unwrap().count(), 0, "plus rien sous bacs");
    assert_eq!(lieu.gh.appels().len(), 13);
    // `lydite etat` montre la vague relancée, puis sa relance, rouge encore et refusée.
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    let rouge = format!("PR 12 · {PR} · CI rouge (constatée)");
    assert_eq!(dit.matches(&rouge).count(), 2, "{dit}");
    assert!(dit.ends_with(&format!("{rouge}\nrefusée (8011) : {}", Code::CiRouge.regle())), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_relance_qui_touche_github_n_est_pas_poussee_8005() {
    let lieu = lieu("forge-relance-github");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    let touche = lieu.relance(main_qui_ecrit_la_ci(&lieu.racine), 9, Vec::new());
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &touche, Duration::from_secs(60)), ExitCode::FAILURE);
    // La relance s'ouvre et s'assemble, mais sa règle de publication la refuse : le refus 8005 se note sous elle,
    // avant tout `forge.poussee`, et rien ne part.
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let (relance, notes) = (&vagues[1], lieu.forge_du_fil(&vagues[1]));
    assert_eq!(mots(&notes), ["refus 8005"]);
    let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
    assert!(preuve.starts_with(".github/ci.yml change entre la base"), "{preuve}");
    assert_eq!((lieu.distante(&branche), git(&lieu.depot, &["rev-parse", &branche])), (tete.clone(), tete));
    assert_eq!(lieu.gh.appels().len(), 8, "la CI de la relance n'est pas lue");
    // La vague relancée se range ; la relance garde son bac avec son travail, son worktree et sa branche.
    assert_eq!(mots(&lieu.forge_du_fil(&menee.id)), ["poussée", "pr 12", "ci Rouge", "rangement"]);
    let bacs = lieu.kraken.join("bacs");
    assert!(bacs.join(format!("main-{relance}-1")).join(".github").join("ci.yml").is_file(), "son bac reste");
    assert!(bacs.join(format!("vague-{relance}")).is_dir(), "son worktree reste");
    assert_ne!(git(&lieu.depot, &["branch", "--list", &format!("lydite/vague-{relance}")]), "");
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    assert!(dit.ends_with(&format!("\nrefusée (8005) : {}", Code::PublicationRefusee.regle())), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_base_qui_a_bouge_pendant_la_ci_n_est_pas_relancee_8009() {
    let lieu = lieu("forge-base-bougee");
    let proposition = lieu.proposition();
    let (plan, base) = (plan_de_a(), lieu.base());
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    // La base avance sur le distant pendant que la CI tourne : sa tête n'est plus un ancêtre de la tête poussée.
    let avancee = sur(&lieu.depot, &base, &[("b.txt", "b\n")], "b");
    git(&lieu.depot, &["push", "-q", "origin", &format!("{avancee}:refs/heads/main")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    // Aucune relance ne part : le refus 8009 se note sous la vague, qui se range, et la PR reste telle qu'elle est.
    assert_eq!(lieu.vagues(), std::slice::from_ref(&menee.id));
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["poussée", "pr 12", "ci Rouge", "refus 8009", "rangement"]);
    let Evenement::RefusForge { preuve, .. } = &notes[3] else { panic!("{notes:?}") };
    assert!(preuve.contains(&format!("sa tête distante {avancee}")), "{preuve}");
    assert_eq!(lieu.distante(&branche), tete);
    assert_eq!(lieu.gh.appels().len(), 8);
    assert_eq!(fs::read_dir(lieu.kraken.join("bacs")).unwrap().count(), 0, "la vague se range");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_vague_critique_rouge_est_8011_sans_relance() {
    let lieu = lieu("forge-critique-rouge");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "critique : oui", "    Ecrire a"]);
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    // `apres` panique si une relance part : aucune ne part. Le refus 8011 se note sous la vague, comme les autres,
    // après sa CI rouge ; la PR reste ouverte et rouge, à la tête poussée, et la vague se range.
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    assert_eq!(lieu.vagues(), std::slice::from_ref(&menee.id));
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["poussée", "pr 12", "ci Rouge", "refus 8011", "rangement"]);
    let Evenement::RefusForge { code, regle, preuve, correction, .. } = &notes[3] else { panic!("{notes:?}") };
    assert_eq!((*code, regle.as_str(), correction.as_str()), (8011, Code::CiRouge.regle(), ROUGE));
    assert!(preuve.contains("tâche a est critique") && preuve.contains("test (windows) : FAILURE"), "{preuve}");
    assert_eq!(lieu.distante(&branche), tete);
    assert_eq!(lieu.gh.appels().len(), 8, "rien après la CI rouge et son journal");
    assert_eq!(fs::read_dir(lieu.kraken.join("bacs")).unwrap().count(), 0, "la vague se range");
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    let rouge = format!("PR 12 · {PR} · CI rouge (constatée)");
    assert!(dit.ends_with(&format!("{rouge}\nrefusée (8011) : {}", Code::CiRouge.regle())), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Une preuve verte tant que `<racine>/rouge` n'existe pas : une vague la joue dans `<racine>/lydite/bacs/<x>`.
const PREUVE_A_MARQUE: &str =
    if cfg!(windows) { r"if exist ..\..\..\rouge (exit 1)" } else { "test ! -f ../../../rouge" };

#[test]
fn lancer_qui_refuse_la_relance_est_8011_sous_la_vague() {
    let lieu = lieu("forge-relance-refusee");
    let proposition = lieu.proposition();
    let preuve = format!("preuve : {PREUVE_A_MARQUE}");
    let plan = plan(&["plan 1", "objectif : Ecrire a", &preuve, "tache a", "    Ecrire a"]);
    let menee = lieu.vague_rouge(&proposition, &plan);
    // La preuve de la vague, verte sur sa base et à chaque fusion, passe au rouge : la base de la relance, la tête
    // poussée, l'éprouve avant toute main, et `lancer_sur` refuse en 6022.
    fs::write(lieu.racine.join("rouge"), "").unwrap();
    let relance = lieu.relance(main_de_vague(&lieu.racine), 9, Vec::new());
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &relance, Duration::from_secs(60)), ExitCode::FAILURE);
    // La relance ne s'ouvre pas, et ce n'est pas une sortie en 2 : le refus 8011 se note sous la vague relancée, qui
    // se range ; rien de la relance ne reste.
    assert_eq!(lieu.vagues(), std::slice::from_ref(&menee.id));
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["poussée", "pr 12", "ci Rouge", "refus 8011", "rangement"]);
    let Evenement::RefusForge { code, preuve, .. } = &notes[3] else { panic!("{notes:?}") };
    assert_eq!(*code, 8011);
    assert!(preuve.contains("ne s'ouvre pas, 6022 lancer.base_rouge"), "{preuve}");
    assert_eq!(fs::read_dir(lieu.kraken.join("bacs")).unwrap().count(), 0, "plus rien sous bacs");
    let branches = git(&lieu.depot, &["for-each-ref", "--format=%(refname)", "refs/heads/lydite"]);
    assert_eq!(branches, format!("refs/heads/lydite/vague-{}", menee.id));
    // `lydite etat` le montre sous la vague relancée, après sa PR et sa CI rouge.
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    let rouge = format!("PR 12 · {PR} · CI rouge (constatée)");
    assert!(dit.ends_with(&format!("{rouge}\nrefusée (8011) : {}", Code::CiRouge.regle())), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[cfg(unix)]
#[test]
fn une_main_de_relance_de_ci_qui_ne_change_rien_est_refusee_par_le_controle_de_la_relance() {
    let lieu = lieu("forge-relance-vide");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague_rouge(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    // Une main qui ne change rien, verte, et qui note chacun de ses départs.
    let departs = lieu.racine.join("departs");
    let script_vide = lieu.racine.join("vide.sh");
    let corps = format!("cat > /dev/null\necho x >> '{}'\necho 'PREUVE: vert'\n", pour_sh(&departs));
    fs::write(&script_vide, corps).unwrap();
    let profil = Profil::faux(vec!["sh".into(), script_vide.to_string_lossy().into_owned()], PREUVE);
    let relancer = |plan: &Plan, base: &str| -> Result<Menee, Refus> {
        let liens = crate::conduite::lancer::Liens { relance_ci: true, ..Default::default() };
        crate::conduite::lancer::lancer_avec(&lieu.depot, &lieu.kraken, &profil, plan, BAIL, base, liens)
    };
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &relancer, Duration::from_secs(60)), ExitCode::FAILURE);
    // Le contrôle propre à la relance refuse : la tête poussée n'a pas changé ; la main n'est pas partie deux fois.
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let notes = lieu.forge_du_fil(&vagues[1]);
    let Some(Evenement::RefusForge { preuve, .. }) = notes.first() else { panic!("{notes:?}") };
    assert_eq!(preuve, &format!("la vague de relance {} n'a rien changé à la tête poussée {tete}", vagues[1]));
    let departs = fs::read_to_string(&departs).unwrap();
    assert_eq!(departs.lines().count(), 1, "pas de relance de main : {departs}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_relance_non_assemblee_est_8011_sous_la_relance() {
    let lieu = lieu("forge-relance-classee");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague_rouge(&proposition, &plan);
    let (tete, branche) = (menee.tete.clone().unwrap(), format!("lydite/vague-{}", menee.id));
    let echoue = lieu.relance(main_qui_echoue(&lieu.racine), 9, Vec::new());
    assert_eq!(lieu.apres_avec(&proposition, &plan, &menee, &echoue, Duration::from_secs(60)), ExitCode::FAILURE);
    // La relance s'ouvre mais sa tâche est classée : le refus 8011 se note sous elle, rien ne se pousse.
    let vagues = lieu.vagues();
    assert_eq!(vagues.len(), 2, "{vagues:?}");
    let (relance, notes) = (&vagues[1], lieu.forge_du_fil(&vagues[1]));
    assert_eq!(mots(&notes), ["refus 8011", "rangement"]);
    let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
    assert!(preuve.starts_with(&format!("la vague de relance {relance} n'a pas assemblé sa tâche ci : ")), "{preuve}");
    assert_eq!(lieu.distante(&branche), tete);
    assert_eq!(lieu.gh.appels().len(), 8, "la CI de la relance n'est pas lue");
    // Le bac de sa tâche reste, qui n'a rien de poussé ; son worktree et sa branche, à la tête poussée, se rangent.
    let bacs = lieu.kraken.join("bacs");
    let retires = [bacs.join(format!("vague-{relance}")).display().to_string(), format!("lydite/vague-{relance}")];
    assert!(matches!(&notes[1], Evenement::Rangement { retires: vus, .. } if *vus == retires), "{notes:?}");
    assert!(bacs.join(format!("main-{relance}-1")).is_dir() && !bacs.join(format!("vague-{relance}")).exists());
    assert_eq!(git(&lieu.depot, &["branch", "--list", &format!("lydite/vague-{relance}")]), "");
    // `lydite etat` montre la relance, qui n'a pas de PR à elle : son objectif, sa tâche classée, puis le refus.
    let (dit, code) = lieu.etat();
    assert_eq!(code, ExitCode::FAILURE, "{dit}");
    assert!(dit.starts_with("Rendre verte la CI de la PR 12\n"), "{dit}");
    assert!(dit.contains(&format!("main-{relance}-1 ci : classée")), "{dit}");
    assert!(dit.ends_with(&format!("\nrefusée (8011) : {}", Code::CiRouge.regle())), "{dit}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn la_relance_est_un_plan_d_une_tache_ci_dont_l_objectif_porte_le_contexte() {
    let plan = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : cargo test",
        "tache a",
        "    Ecrire a",
        "tache b",
        "depend : a",
        "preuve : exit 3",
        "    Ecrire b",
    ]);
    let relance = plan_de_relance(&plan, 12, "test (windows) : FAILURE\ntest b ... FAILED\n");
    // Une vague qui rend verte la CI de la PR, de la preuve de la vague relancée, et sa seule tâche, sans dépendance
    // ni preuve à elle.
    assert_eq!(relance.objectif, "Rendre verte la CI de la PR 12");
    assert_eq!(relance.preuve.as_deref(), Some("cargo test"));
    let [tache] = &relance.taches[..] else { panic!("{relance:?}") };
    assert_eq!((tache.id.as_str(), tache.depend.is_empty(), tache.preuve.as_deref()), ("ci", true, None));
    assert_eq!(tache.objectif, format!("{CORRIGER}\ntest (windows) : FAILURE\ntest b ... FAILED"));
    assert!(tache.objectif.starts_with("Corrige au plus petit") && tache.objectif.contains("sans toucher à .github/"));
    // Sans contexte, l'objectif est sa première ligne seule.
    assert_eq!(plan_de_relance(&plan, 3, "").taches[0].objectif, CORRIGER);
}

#[test]
fn la_tache_ci_prend_l_union_des_fichiers_quand_chaque_tache_en_a() {
    // Chaque tâche a ses fichiers : la tâche ci a leur union, dans leur ordre, sans doublon.
    let bornees = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : exit 0",
        "tache a",
        "fichiers : src/ a.txt",
        "    Ecrire a",
        "tache b",
        "fichiers : a.txt b.txt",
        "    Ecrire b",
    ]);
    let unis = ["src/", "a.txt", "b.txt"].map(String::from).to_vec();
    assert_eq!(plan_de_relance(&bornees, 12, "").taches[0].fichiers, Some(unis));
    // Une tâche sans fichiers, que rien ne bornait : la tâche ci n'en a aucun.
    let libre = plan(&[
        "plan 1",
        "objectif : Ecrire a et b",
        "preuve : exit 0",
        "tache a",
        "fichiers : a.txt",
        "    Ecrire a",
        "tache b",
        "    Ecrire b",
    ]);
    assert_eq!(plan_de_relance(&libre, 12, "").taches[0].fichiers, None);
}

#[test]
fn un_bac_qui_a_bouge_apres_son_gel_reste_avec_sa_branche() {
    let lieu = lieu("forge-tardif");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"]);
    let menee = lieu.vague(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    // Un commit dans le bac de a après son gel, d'un processus qui aurait survécu à sa main : ce travail n'est pas
    // dans ce qui se pousse.
    let (id, bac) = (&menee.issues[0].0, lieu.kraken.join("bacs").join(&menee.issues[0].0));
    ecrire(&bac, "tardif.txt", "après le gel");
    git(&bac, &["add", "-A"]);
    git(&bac, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "tardif"]);
    let tardif = git(&bac, &["rev-parse", "HEAD"]);
    lieu.ouvre(2, &tete, &[run("test", "completed", "success")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    // Seul le worktree de la vague se range : le bac de a reste, avec sa branche sur le commit tardif.
    let notes = lieu.forge_du_fil(&menee.id);
    let vague = lieu.kraken.join("bacs").join(format!("vague-{}", menee.id)).display().to_string();
    assert!(matches!(&notes[3], Evenement::Rangement { retires, .. } if *retires == [vague.clone()]), "{notes:?}");
    assert!(bac.join("tardif.txt").is_file() && !Path::new(&vague).exists());
    assert_eq!(git(&lieu.depot, &["rev-parse", &format!("lydite/{id}")]), tardif);
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn le_rangement_retire_comme_la_forge_lance_ses_gestes_git() {
    let lieu = lieu("forge-rangement-gestes");
    // La base porte un fichier suivi, `temoin.dat`, d'un seul caractère.
    let base = sur(&lieu.depot, &lieu.base(), &[("temoin.dat", "x")], "temoin");
    git(&lieu.depot, &["push", "-q", "origin", &format!("{base}:refs/heads/main")]);
    let proposition = lieu.proposition();
    let menee = lieu.vague(&proposition, &plan_de_a());
    let (tete, id) = (menee.tete.clone().unwrap(), menee.issues[0].0.clone());
    // Pour dire si un bac est propre, `git worktree remove` lance le pilote `clean` d'un fichier suivi que sa date ne
    // dit plus intact. Celui-ci rend ce caractère, et note les variables que git lui laisse, puis ce que le dossier de
    // Kraken contient à cet instant : une commande de la forge y a ses fichiers de sortie, qu'elle efface ensuite.
    // Rien que des commandes internes de sh, comme les autres pilotes des essais.
    let (filtre, temoin) = (lieu.racine.join("filtre.sh"), lieu.racine.join("temoin.txt"));
    let vars = "$GIT_NO_REPLACE_OBJECTS $GIT_TERMINAL_PROMPT $GCM_INTERACTIVE";
    let corps = format!("read -r _\nprintf x\necho \"{vars}\" '{}'/* >> '{}'", pour_sh(&lieu.kraken), pour_sh(&temoin));
    script(&filtre, &corps);
    git(&lieu.depot, &["config", "filter.temoin.clean", &pour_sh(&filtre)]);
    ecrire(&lieu.depot, ".git/info/attributes", "temoin.dat filter=temoin\n");
    let fichier = lieu.kraken.join("bacs").join(&id).join("temoin.dat");
    let avenir = std::time::SystemTime::now() + Duration::from_secs(60);
    File::options().write(true).open(&fichier).unwrap().set_modified(avenir).unwrap();
    let fil = Fil::seance(&lieu.kraken, "humain");
    proposition.ranger(&fil, &menee, &tete, false).unwrap();
    assert!(!lieu.kraken.join("bacs").join(&id).exists(), "le bac est retiré");
    // Le pilote a joué sous les variables de la forge (ni remplacements d'objets, ni invite, ni gestionnaire
    // interactif), et sa commande écrivait dans les fichiers de la forge, qui la borne par son délai.
    let vu = fs::read_to_string(&temoin).expect("git a lancé le pilote du bac");
    assert!(vu.lines().all(|ligne| ligne.starts_with("1 0 never ") && ligne.contains(".sortie")), "{vu:?}");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_vague_sans_tache_assemblee_n_a_rien_a_pousser_8004() {
    let lieu = lieu("forge-vide");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Echouer", "preuve : exit 0", "tache a", "preuve : exit 3", "    Echouer"]);
    let menee = lieu.vague(&proposition, &plan);
    assert_eq!(menee.tete.as_deref(), Some(proposition.base.as_str()));
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    // Rien ne part : ni branche distante, ni autre appel à gh que sa connexion, ni rangement.
    assert_eq!(mots(&lieu.forge_du_fil(&menee.id)), ["refus 8004"]);
    assert_eq!(lieu.poussees(), "");
    assert_eq!(lieu.gh.appels().len(), 1);
    assert!(lieu.kraken.join("bacs").join(&menee.issues[0].0).is_dir());
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn ni_github_ni_chemin_du_poste_ni_commit_etranger_ne_part_8005() {
    let lieu = lieu("forge-publiable");
    let proposition = lieu.proposition();
    // a écrit .github/ci.yml par sa preuve ; b nomme le dossier de Kraken dans son objectif, que sa main écrit dans
    // son bac, et dont le corps ne prend que la première ligne ; c commite dans son bac sous une autre identité.
    let la_ci = format!("preuve : {ECRIT_LA_CI}");
    let github = plan(&["plan 1", "objectif : Toucher la CI", "preuve : exit 0", "tache a", &la_ci, "    Ecrire a"]);
    let vu_dans = format!("    vu dans {}", lieu.kraken.display());
    let chemin =
        plan(&["plan 1", "objectif : Nommer le poste", "preuve : exit 0", "tache b", "    Ecrire b", &vu_dans]);
    let en_etranger = format!("preuve : {COMMITE_EN_ETRANGER}");
    let etranger =
        plan(&["plan 1", "objectif : Publier un autre", "preuve : exit 0", "tache c", &en_etranger, "    Ecrire c"]);
    let cas = [
        (github, ".github/ci.yml change entre la base"),
        (chemin, "paraît dans les lignes ajoutées de b/"),
        (etranger, "n'est pas de Lydite : auteur « autre <autre@exemple.invalid> », committer « autre <"),
    ];
    for (plan, vu) in cas {
        let menee = lieu.vague(&proposition, &plan);
        assert!(menee.issues.iter().all(|(_, issue)| issue.is_ok()), "{:?}", menee.issues);
        assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
        let notes = lieu.forge_du_fil(&menee.id);
        assert_eq!(mots(&notes), ["refus 8005"]);
        let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
        assert!(preuve.contains(vu), "{preuve}");
        // Rien n'est poussé : la branche reste locale, à sa tête, et ses bacs restent.
        assert_eq!(lieu.poussees(), "");
        let locale = git(&lieu.depot, &["rev-parse", &format!("lydite/vague-{}", menee.id)]);
        assert_eq!(Some(locale), menee.tete);
        assert!(lieu.kraken.join("bacs").join(&menee.issues[0].0).is_dir());
    }
    assert_eq!(lieu.gh.appels().len(), 1, "aucune PR ne s'ouvre");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_mot_de_la_garde_dans_le_corps_ne_part_pas_8005() {
    let lieu = lieu("forge-garde");
    let proposition = lieu.proposition();
    // Le corps de la PR dit le reçu, dont la ligne des réglages porte le mot gardé ; la fausse main n'écrit pas ce mot
    // dans son bac, et le titre ne le porte pas : le corps seul le dit.
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"]);
    let garde = lieu.kraken.join("garde");
    fs::write(&garde, "# les noms que je ne publie pas\nChangés : Aucun\n").unwrap();
    let menee = lieu.vague(&proposition, &plan);
    assert!(menee.issues.iter().all(|(_, issue)| issue.is_ok()), "{:?}", menee.issues);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["refus 8005"]);
    let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
    assert_eq!(preuve, "la ligne 2 de la garde paraît dans le corps");
    assert!(!preuve.contains(&garde.display().to_string()), "{preuve}");
    // Rien n'est poussé, et aucune PR ne s'ouvre.
    assert_eq!(lieu.poussees(), "");
    assert_eq!(lieu.gh.appels().len(), 1, "aucune PR ne s'ouvre");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_poussee_refusee_n_ouvre_rien_8006() {
    let lieu = lieu("forge-poussee");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"]);
    let menee = lieu.vague(&proposition, &plan);
    // Le distant part après la lecture de sa base : le push ne trouve plus où aller.
    fs::rename(&lieu.nu, lieu.racine.join("parti.git")).unwrap();
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    assert_eq!(mots(&lieu.forge_du_fil(&menee.id)), ["poussée", "refus 8006"]);
    // Aucune PR ne s'ouvre, et rien ne se range sans push réussi.
    assert_eq!(lieu.gh.appels().len(), 1);
    assert!(lieu.kraken.join("bacs").join(&menee.issues[0].0).is_dir());
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn un_create_rate_dont_la_pr_existe_sur_la_tete_poussee_poursuit_la_forge() {
    let lieu = lieu("forge-create-rate");
    let proposition = lieu.proposition();
    let plan = plan_de_a();
    let menee = lieu.vague(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    // `gh pr create` sort en erreur (appel 2) alors que la PR est ouverte : la relire sur la branche (appel 3) la donne
    // sur la tête poussée, et sa CI se lit dessus (appels 4 à 6), sans refus 8007 ni second create.
    lieu.gh.rendre(2, 1, "", "gh: connexion perdue\n");
    lieu.gh.rendre(3, 0, &pr_vue(&tete), "");
    rendre_ci(&lieu.gh, 4, &tete, &[run("test", "completed", "success")], &[]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    assert_eq!(mots(&lieu.forge_du_fil(&menee.id)), ["poussée", "pr 12", "ci Vert", "rangement"]);
    let branche = format!("lydite/vague-{}", menee.id);
    let appels = lieu.gh.appels();
    let creates: Vec<_> = appels.iter().filter(|appel| appel.starts_with("pr create ")).collect();
    assert_eq!(creates.len(), 1, "{appels:?}");
    assert_eq!(appels.len(), 6, "{appels:?}");
    assert_eq!(appels[2], format!("pr view {branche} --json number,url,headRefOid --repo forge.test/p/d"));
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_ci_muette_en_conflit_ou_changee_arrete_la_forge_8008_8009_8010() {
    let lieu = lieu("forge-arrets");
    let proposition = lieu.proposition();
    let cas = [
        ("OPEN", "MERGEABLE", Duration::ZERO, 8008),
        ("OPEN", "CONFLICTING", Duration::from_secs(60), 8009),
        ("CLOSED", "MERGEABLE", Duration::from_secs(60), 8010),
    ];
    // Une vague par cas, partie de la même base ; le faux gh compte ses appels d'un cas à l'autre. Une PR ouverte et
    // sans conflit fait lire ses runs et ses statuts, ici aucun ; les autres arrêtent la forge avant.
    let mut k = 2;
    for (etat_de_la_pr, mergeable, delai, code) in cas {
        let objectif = format!("objectif : Ecrire {code}");
        let plan = plan(&["plan 1", &objectif, "preuve : exit 0", "tache a", "    Ecrire a"]);
        let menee = lieu.vague(&proposition, &plan);
        let tete = menee.tete.clone().unwrap();
        lieu.gh.rendre(k, 0, &format!("{PR}\n"), "");
        lieu.gh.rendre(k + 1, 0, &pr_vue(&tete), "");
        lieu.gh.rendre(k + 2, 0, &vue_pr(&tete, etat_de_la_pr, mergeable), "");
        lieu.gh.rendre(k + 3, 0, "[]", "");
        lieu.gh.rendre(k + 4, 0, &statuts_du_commit(&[]), "");
        k += if code == 8008 { 5 } else { 3 };
        assert_eq!(lieu.apres(&proposition, &plan, &menee, delai), ExitCode::FAILURE, "{code}");
        // Le refus, puis le rangement : le push a réussi, quelle que soit la CI.
        let refus = format!("refus {code}");
        assert_eq!(mots(&lieu.forge_du_fil(&menee.id)), ["poussée", "pr 12", refus.as_str(), "rangement"]);
        assert!(!lieu.kraken.join("bacs").join(&menee.issues[0].0).exists(), "{code} : le bac est rangé");
        assert_eq!(lieu.distante(&format!("lydite/vague-{}", menee.id)), tete);
    }
    assert_eq!(lieu.appels_bien_partis(), 12);
    let _ = fs::remove_dir_all(&lieu.racine);
}

/// Les notes d'une vague `v` relue par `r`, corrigée par `c` : la relecture (`bloquantes` sur `n = 1`), son verdict sur
/// `jugee`, la fusion de `v` sur `jugee`, et la correction de `corrige` (que `c` vise) avec ses fusions `fusions`.
fn relue_notee(bloquantes: usize, jugee: &str, corrige: &str, fusions: &[&str]) -> Vec<Evenement> {
    let mut notes = vec![
        Evenement::Fusion { vague: "v".into(), main: 1, sha: jugee.into(), verif: String::new() },
        Evenement::Relecteur { vague: "r".into(), relue: "v".into() },
        Evenement::Relecture { vague: "r".into(), relue: "v".into(), n: 1, bloquantes, remarques: "r".into() },
        Evenement::VerdictRelecture {
            vague: "r".into(),
            relue: "v".into(),
            sha: jugee.into(),
            profil: Some("claude".into()),
            empreinte: Some("e".repeat(64)),
            modele: Some("sonnet".into()),
        },
        Evenement::Correction { vague: "c".into(), corrige: corrige.into() },
    ];
    for sha in fusions {
        notes.push(Evenement::Fusion { vague: "c".into(), main: 1, sha: (*sha).into(), verif: String::new() });
    }
    notes
}

/// Ce que `controler_la_tete` dit de la vague `v` face à `tete`, d'après ces notes.
fn controle(notes: &[Evenement], tete: &str) -> Result<String, Refus> {
    controler_la_tete(&etat(notes), notes, "v", tete)
}

#[test]
fn une_vague_relue_sans_bloquante_ne_pousse_que_la_tete_jugee() {
    let (t, autre) = ("a".repeat(40), "b".repeat(40));
    let notes = relue_notee(0, &t, "v", &[]);
    assert_eq!(controle(&notes, &t).unwrap(), format!("Reviewed and replayed on commit {t}."));
    // Une autre tête est refusée en 7016, et la preuve nomme les deux commits.
    let refus = controle(&notes, &autre).unwrap_err();
    assert_eq!(refus.code, Code::RelectureManquee);
    assert!(refus.preuve.contains(&t) && refus.preuve.contains(&autre), "{}", refus.preuve);
    // Même une tête qui fut relue pour une autre vague, ou une relance de la CI du même nom : seule la jugée passe.
    let sans_verdict: Vec<Evenement> =
        notes.iter().filter(|e| !matches!(e, Evenement::VerdictRelecture { .. })).cloned().collect();
    assert_eq!(controle(&sans_verdict, &t).unwrap_err().code, Code::RelectureManquee);
}

#[test]
fn le_verdict_d_un_autre_relecteur_ne_juge_pas_la_relecture() {
    let t = "a".repeat(40);
    let mut notes = relue_notee(0, &t, "v", &[]);
    // Une relecture plus récente, d'un autre relecteur, sans verdict : le verdict de `r` ne la couvre pas.
    notes.push(Evenement::Relecture {
        vague: "s".into(),
        relue: "v".into(),
        n: 0,
        bloquantes: 0,
        remarques: "".into(),
    });
    assert_eq!(controle(&notes, &t).unwrap_err().code, Code::RelectureManquee);
}

#[test]
fn des_bloquantes_ne_poussent_que_la_correction_qui_vise_la_relecture() {
    let (t, t2, autre) = ("a".repeat(40), "c".repeat(40), "d".repeat(40));
    // Sans correction : 7016, quelle que soit la tête.
    let sans = relue_notee(1, &t, "v", &[]);
    for tete in [&t, &t2] {
        let refus = controle(&sans, tete).unwrap_err();
        assert_eq!(refus.code, Code::RelectureManquee, "{tete}");
        assert!(refus.preuve.contains(tete.as_str()) && refus.preuve.contains(&t), "{}", refus.preuve);
    }
    // Seulement la correction d'une autre relecture : 7016, même sur sa tête.
    let d_une_autre = relue_notee(1, &t, "autre", &[&t2]);
    for tete in [&t, &t2] {
        assert_eq!(controle(&d_une_autre, tete).unwrap_err().code, Code::RelectureManquee, "{tete}");
    }
    // La tête relue elle-même est refusée, correction vue ou non.
    let corrigee = relue_notee(1, &t, "v", &[&t2]);
    let refus = controle(&corrigee, &t).unwrap_err();
    assert_eq!(refus.code, Code::RelectureManquee);
    assert!(refus.preuve.contains(&t) && refus.preuve.contains(&t2), "{}", refus.preuve);
    let sans_changement = relue_notee(1, &t, "v", &[&t]);
    assert_eq!(controle(&sans_changement, &t).unwrap_err().code, Code::RelectureManquee);
    // Une autre tête que celle de la correction est refusée aussi ; c'est son dernier `vague.fusion` qui compte.
    assert_eq!(controle(&corrigee, &autre).unwrap_err().code, Code::RelectureManquee);
    let deux = relue_notee(1, &t, "v", &[&autre, &t2]);
    assert!(controle(&deux, &t2).is_ok() && controle(&deux, &autre).is_err());
    // La tête de la correction passe.
    assert!(controle(&corrigee, &t2).is_ok());
}

#[test]
fn la_ligne_du_corps_dit_le_commit_relu_et_rejoue() {
    let (t, t2) = ("a".repeat(40), "c".repeat(40));
    // Sans correction.
    let notes = relue_notee(0, &t, "v", &[]);
    assert_eq!(controle(&notes, &t).unwrap(), format!("Reviewed and replayed on commit {t}."));
    // Avec une correction : le commit relu, la tête corrigée, sans seconde relecture.
    let notes = relue_notee(1, &t, "v", &[&t2]);
    assert_eq!(
        controle(&notes, &t2).unwrap(),
        format!("Reviewed on commit {t}, corrected in {t2} without a second review, and replayed on commit {t2}.")
    );
    // Sans relecture : la tête part comme elle est, et le corps le dit.
    let ouverte = [Evenement::Fusion { vague: "v".into(), main: 1, sha: t.clone(), verif: String::new() }];
    assert_eq!(
        controle(&ouverte, &t).unwrap(),
        format!("Replayed on commit {t}, not reviewed: no critical task was assembled.")
    );
    // « replayed » ne s'écrit que si un `vague.fusion` de la vague ou de sa correction porte le commit.
    let mut sans_fusion = relue_notee(0, &t, "v", &[]);
    sans_fusion.remove(0);
    assert_eq!(controle(&sans_fusion, &t).unwrap(), format!("Reviewed and not replayed on commit {t}."));
    let mut sans_fusion = relue_notee(1, &t, "v", &[&t2]);
    sans_fusion.pop();
    sans_fusion.push(Evenement::Fusion { vague: "autre".into(), main: 1, sha: t2.clone(), verif: String::new() });
    let ligne = controle(&sans_fusion, &t2);
    assert!(ligne.is_err(), "sans fusion de la correction, la tête de la correction ne se trouve pas : {ligne:?}");
    assert_eq!(
        controle(&[], &t2).unwrap(),
        format!("Not replayed on commit {t2}, not reviewed: no critical task was assembled.")
    );
}

#[test]
fn une_vague_relue_dont_la_tete_n_est_pas_jugee_ne_pousse_rien() {
    let lieu = lieu("forge-verdict");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"]);
    let menee = lieu.vague(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    // Le fil dit la vague relue sur une autre tête, sans bloquante : la tête de la vague n'est pas la jugée.
    let jugee = "e".repeat(40);
    let fil = Fil::seance(&lieu.kraken, "humain");
    let relecteur = "relecteur-1";
    for note in [
        Evenement::Relecture {
            vague: relecteur.into(),
            relue: menee.id.clone(),
            n: 0,
            bloquantes: 0,
            remarques: String::new(),
        },
        Evenement::VerdictRelecture {
            vague: relecteur.into(),
            relue: menee.id.clone(),
            sha: jugee.clone(),
            profil: None,
            empreinte: None,
            modele: None,
        },
    ] {
        fil.noter(&note).unwrap();
    }
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::FAILURE);
    // Rien ne part : pas de `forge.poussee`, ni de PR, ni de branche au distant ; le refus 7016 nomme les deux commits.
    let notes = lieu.forge_du_fil(&menee.id);
    assert_eq!(mots(&notes), ["refus 7016"]);
    let Evenement::RefusForge { preuve, .. } = &notes[0] else { panic!("{notes:?}") };
    assert!(preuve.contains(&tete) && preuve.contains(&jugee), "{preuve}");
    assert_eq!(lieu.gh.appels(), ["auth status --hostname forge.test".to_string()]);
    assert_eq!(lieu.poussees(), "");
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn une_vague_relue_sur_sa_tete_pousse_et_le_corps_nomme_le_commit() {
    let lieu = lieu("forge-verdict-ok");
    let proposition = lieu.proposition();
    let plan = plan(&["plan 1", "objectif : Ecrire a", "preuve : exit 0", "tache a", "    Ecrire a"]);
    let menee = lieu.vague(&proposition, &plan);
    let tete = menee.tete.clone().unwrap();
    let fil = Fil::seance(&lieu.kraken, "humain");
    let relecteur = "relecteur-1";
    for note in [
        Evenement::Relecture {
            vague: relecteur.into(),
            relue: menee.id.clone(),
            n: 0,
            bloquantes: 0,
            remarques: String::new(),
        },
        Evenement::VerdictRelecture {
            vague: relecteur.into(),
            relue: menee.id.clone(),
            sha: tete.clone(),
            profil: None,
            empreinte: None,
            modele: None,
        },
    ] {
        fil.noter(&note).unwrap();
    }
    lieu.ouvre(2, &tete, &[run("test", "completed", "success")]);
    assert_eq!(lieu.apres(&proposition, &plan, &menee, Duration::from_secs(60)), ExitCode::SUCCESS);
    assert_eq!(lieu.distante(&format!("lydite/vague-{}", menee.id)), tete);
    let corps = lieu.gh.entree(2);
    assert!(
        corps.ends_with(&format!(
            "Relecture : 0 remarques, dont 0 bloquantes.\nReviewed and replayed on commit {tete}.\n"
        )),
        "{corps}"
    );
    let _ = fs::remove_dir_all(&lieu.racine);
}

#[test]
fn millisecondes_est_le_contraire_de_instant() {
    for ms in [0, 1_761_901_500_123, 951_782_400_000, 4_102_444_799_999] {
        assert_eq!(millisecondes(&fil::instant(ms)), Some(ms), "{}", fil::instant(ms));
    }
    assert_eq!(millisecondes("2026-10-31T21:03:00Z"), millisecondes("2026-10-31T21:03:00.000Z"));
    assert_eq!(millisecondes("pas un instant"), None);
}

#[test]
fn les_dollars_s_arrondissent_au_centime_a_la_virgule() {
    assert_eq!(dollars(0), "0,00");
    assert_eq!(dollars(839_999), "0,84");
    assert_eq!(dollars(834_999), "0,83");
    assert_eq!(dollars(12_340_000), "12,34");
}

#[test]
fn le_corps_porte_le_bloc_non_rejoue_de_la_relecture_dix_lignes_de_200_caracteres_au_plus() {
    let relecture = |id: &str, remarques: String| Evenement::Relecture {
        vague: "r".into(),
        relue: id.into(),
        n: 0,
        bloquantes: 0,
        remarques,
    };
    let forme_2 = |non_rejoue: &str| format!("remarques 2\n\nrejoue\n    Vert.\n\nnon rejoue\n{non_rejoue}");
    // Les lignes du bloc, sous l'intitulé, chacune en code.
    let notes = [relecture("v", forme_2("    Windows n'est pas joué.\n    Le réseau non plus.\n"))];
    assert_eq!(
        non_rejoue_de_la_relecture(&notes, "v"),
        ["Not replayed (from the review):", "- `Windows n'est pas joué.`", "- `Le réseau non plus.`"]
    );
    // Dix lignes au plus, chacune coupée à 200 caractères, sans caractère de contrôle ni apostrophe inverse.
    let lignes: String = (1..=12).map(|k| format!("    ligne {k} {}\n", "é".repeat(300))).collect();
    let dites = non_rejoue_de_la_relecture(&[relecture("v", forme_2(&lignes))], "v");
    assert_eq!(dites.len(), 1 + NON_REJOUE_LIGNES);
    assert!(dites[1].starts_with("- `ligne 1 é") && !dites.iter().any(|l| l.contains("ligne 11")), "{dites:?}");
    assert!(dites[1..].iter().all(|l| l.chars().count() == "- ``".len() + 200), "{dites:?}");
    let sale = non_rejoue_de_la_relecture(&[relecture("v", forme_2("    a`b\u{1b}[2Jc @nom\n"))], "v");
    assert_eq!(sale, ["Not replayed (from the review):", "- `a'b[2Jc @nom`"]);
    // La dernière relecture de la vague compte ; celle d'une autre vague, la forme 1 et un texte illisible se taisent.
    let notes = [relecture("v", forme_2("    Ancienne.\n")), relecture("v", forme_2("    Récente.\n"))];
    assert_eq!(non_rejoue_de_la_relecture(&notes, "v"), ["Not replayed (from the review):", "- `Récente.`"]);
    assert!(non_rejoue_de_la_relecture(&notes, "autre").is_empty());
    assert!(non_rejoue_de_la_relecture(&[relecture("v", "remarques 1\n".into())], "v").is_empty());
    assert!(non_rejoue_de_la_relecture(&[relecture("v", "n'importe quoi".into())], "v").is_empty());
}
