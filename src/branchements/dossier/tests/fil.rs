//! Les tests du fil dans le dossier de Kraken : la séance, ses notes, son compteur, et le fichier écrit et lu.
use crate::branchements::dossier::*;
use crate::coeur::fil::{Couleur, Foi, Verdict, etat};
use crate::essais::{codes, dossier, sans_instant, seqs};
use std::collections::HashSet;
use std::thread;

#[test]
fn un_champ_fait_l_aller_retour() {
    let dir = dossier("aller-retour");
    let fil = Fil::seance(&dir, "humain");
    let texte =
        "ligne 1\nligne 2\r\n\tindentée, une barre \\, un \\n resté écrit, des \"guillemets\", des accents : éàçœÉ\\";
    let fin = Evenement::Fin {
        vague: "v\t1".into(),
        main: 1,
        tentative: 2,
        couleur: Couleur::Vert,
        foi: Foi::Constate,
        texte: texte.into(),
        verif: Some("\\".into()),
    };
    fil.noter(&fin).unwrap();
    let brut = fs::read_to_string(dir.join(FICHIER)).unwrap();
    assert_eq!(brut.matches('\n').count(), 1, "une seule ligne : {brut}");
    let attendu = format!(
        concat!(
            r#"{{"v":2,"seance":"{}","seq":1,"instant":"<instant>","origine":"humain","origine_foi":"declare","#,
            r#""type":"main.fin","vague":"v\t1","main":1,"tentative":2,"couleur":"vert","foi":"constate","texte""#,
            r#":"ligne 1\nligne 2\r\n\tindentée, une barre \\, un \\n resté écrit, des \"guillemets\","#,
            r#" des accents : éàçœÉ\\","verif":"\\"}}"#,
        ),
        fil.seance
    );
    assert_eq!(sans_instant(brut.trim_end_matches('\n')), attendu);
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let [note] = notes.as_slice() else { panic!("une note : {notes:?}") };
    assert_eq!((note.seance.as_str(), note.seq, note.origine.as_str()), (fil.seance.as_str(), 1, "humain"));
    assert_eq!((note.origine_foi, &note.evenement), (Foi::Declare, &fin));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn huit_fils_d_execution_notent_1600_lignes_lisibles() {
    let dir = dossier("huit");
    let fil = Fil::seance(&dir, "humain");
    // Plus d'une page par ligne, avec des tabulations et des retours à échapper.
    let texte = "une ligne\tet sa tabulation\n".repeat(200);
    thread::scope(|s| {
        for main in 1..=8 {
            let (fil, texte) = (&fil, &texte);
            s.spawn(move || {
                for tentative in 1..=200 {
                    let (couleur, foi, texte, verif) = (Couleur::Rouge, Foi::Constate, texte.clone(), None);
                    let fin = Evenement::Fin { vague: "v".into(), main, tentative, couleur, foi, texte, verif };
                    fil.noter(&fin).unwrap();
                }
            });
        }
    });
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert_eq!((notes.len(), ecarts.len()), (1600, 0), "{ecarts:?}");
    // Le compteur suit l'ordre du fichier : 1 à 1600, dans l'ordre des lignes.
    assert_eq!(seqs(&notes), (1..=1600).collect::<Vec<_>>());
    let mut lues: Vec<_> = notes
        .iter()
        .map(|note| match &note.evenement {
            Evenement::Fin { main, tentative, texte: lu, .. } if *lu == texte => (*main, *tentative),
            evenement => panic!("ligne abîmée : {evenement:?}"),
        })
        .collect();
    lues.sort_unstable();
    assert_eq!(lues, (1..=8).flat_map(|main| (1..=200).map(move |t| (main, t))).collect::<Vec<_>>());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deux_seances_entrelacees_numerotent_chacune_sans_trou() {
    let dir = dossier("entrelacees");
    let (a, b) = (Fil::seance(&dir, "humain"), Fil::seance(&dir, "humain"));
    assert_ne!(a.seance, b.seance);
    let debut = Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 };
    // Mêlées ligne à ligne, puis depuis deux fils d'exécution à la fois, comme deux processus.
    for fil in [&a, &b, &b, &a, &b, &a, &a] {
        fil.noter(&debut).unwrap();
    }
    thread::scope(|s| {
        for fil in [&a, &b] {
            let debut = &debut;
            s.spawn(move || (0..100).for_each(|_| fil.noter(debut).unwrap()));
        }
    });
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let ordre: Vec<&str> = notes.iter().take(7).map(|note| note.seance.as_str()).collect();
    let (sa, sb) = (a.seance.as_str(), b.seance.as_str());
    assert_eq!(ordre, [sa, sb, sb, sa, sb, sa, sa], "les deux séances se mêlent dans le fichier");
    for (fil, n) in [(&a, 104), (&b, 103)] {
        let leurs: Vec<u64> = notes.iter().filter(|note| note.seance == fil.seance).map(|note| note.seq).collect();
        assert_eq!(leurs, (1..=n).collect::<Vec<_>>(), "séance {}", fil.seance);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn deux_seances_sur_une_vague_ont_deux_ids_sans_ecart() {
    let dir = dossier("une-vague");
    // La séance du lancement ouvre la vague ; une autre pièce y écrit depuis la sienne.
    let lancement = Fil::seance(&dir, "humain");
    let ouverture =
        Evenement::Vague { vague: "v".into(), n: 1, depot: "/srv/depot".into(), verifier: None, objectif: "o".into() };
    lancement.noter(&ouverture).unwrap();
    lancement.noter(&Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 }).unwrap();
    let autre = Fil::seance(&dir, "humain");
    let verdict = Evenement::Verdict { vague: "v".into(), main: 1, verdict: Verdict::Classee, foi: Foi::Constate };
    autre.noter(&verdict).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "{ecarts:?}");
    let (sl, sa) = (lancement.seance.as_str(), autre.seance.as_str());
    assert_ne!(sl, sa);
    let lues: Vec<(&str, u64)> = notes.iter().map(|note| (note.seance.as_str(), note.seq)).collect();
    assert_eq!(lues, [(sl, 1), (sl, 2), (sa, 1)]);
    let vagues = etat(&notes.into_iter().map(|note| note.evenement).collect::<Vec<_>>());
    assert_eq!(vagues.len(), 1, "une vague, que deux séances racontent");
    assert_eq!(vagues[0].mains[&1].verdict, Some((Verdict::Classee, Foi::Constate)));
    // Des séances ouvertes à la chaîne, depuis plusieurs fils d'exécution : jamais deux fois le même id.
    let ids: Vec<String> = thread::scope(|s| {
        let fils: Vec<_> = (0..4)
            .map(|_| s.spawn(|| (0..250).map(|_| Fil::seance(&dir, "humain").seance).collect::<Vec<_>>()))
            .collect();
        fils.into_iter().flat_map(|fil| fil.join().unwrap()).collect()
    });
    assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_note_ratee_ne_prend_pas_de_numero() {
    let dir = dossier("ratee");
    let fil = Fil::seance(&dir, "humain");
    let debut = |tentative| Evenement::Debut { vague: "v".into(), main: 1, tentative };
    fil.noter(&debut(1)).unwrap();
    // Le fil mis de côté, un dossier à sa place : la note rate.
    let (chemin, cote) = (dir.join(FICHIER), dir.join("cote"));
    fs::rename(&chemin, &cote).unwrap();
    fs::create_dir(&chemin).unwrap();
    assert!(fil.noter(&debut(2)).is_err());
    fs::remove_dir(&chemin).unwrap();
    fs::rename(&cote, &chemin).unwrap();
    fil.noter(&debut(3)).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "aucun trou : {ecarts:?}");
    let lues: Vec<_> = notes.iter().map(|note| (note.seq, note.evenement.clone())).collect();
    assert_eq!(lues, [(1, debut(1)), (2, debut(3))]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_verrou_empoisonne_se_reprend_et_le_compteur_reste_juste() {
    let dir = dossier("poison");
    let fil = Fil::seance(&dir, "humain");
    let debut = |tentative| Evenement::Debut { vague: "v".into(), main: 1, tentative };
    fil.noter(&debut(1)).unwrap();
    // Une main qui panique en tenant le verrou : il est empoisonné pour toutes les autres.
    let panique = thread::scope(|s| {
        s.spawn(|| {
            let _garde = fil.seq.lock().unwrap();
            panic!("une main a paniqué");
        })
        .join()
    });
    assert!(panique.is_err() && fil.seq.is_poisoned());
    // Les mains d'après notent encore : le verrou se reprend, et le compteur, qui n'a avancé qu'après des écritures
    // entières, dit juste.
    fil.noter(&debut(2)).unwrap();
    fil.noter(&debut(3)).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "aucun trou, aucune redite : {ecarts:?}");
    assert_eq!(seqs(&notes), [1, 2, 3]);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn une_erreur_d_es_du_fil_nomme_son_chemin_et_garde_son_kind() {
    let dir = dossier("es");
    let fil = Fil::seance(&dir, "humain");
    let chemin = dir.join(FICHIER);
    // Un dossier à la place du fichier : le fil ne s'ouvre ni pour noter, ni pour lire.
    fs::create_dir(&chemin).unwrap();
    let nom = chemin.display().to_string();
    let direct = OpenOptions::new().create(true).read(true).append(true).open(&chemin).unwrap_err().kind();
    let erreur = fil.noter(&Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 }).unwrap_err();
    assert_eq!(erreur.kind(), direct, "le kind() d'une ouverture directe : {erreur}");
    assert!(erreur.to_string().contains(&nom), "l'erreur de la note nomme le fil {nom} : {erreur}");
    let refus = lire(&dir).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie);
    assert!(refus.to_string().contains(&nom), "l'erreur de la lecture nomme le fil {nom} : {refus}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_bout_de_ligne_en_fin_de_fil_ne_se_colle_pas_a_la_ligne_suivante() {
    let dir = dossier("bout");
    // Ce qu'une écriture coupée laisse en fin de fil : le début d'une ligne sans `\n` (Kraken tué, disque plein,
    // coupure de courant), de l'UTF-8 coupé, ou des zéros (la queue d'un fichier NTFS après une coupure).
    let bouts: [&[u8]; 4] = [
        r#"{"v":2,"seance":"1761901236000_9_1","seq":2,"instant":"2026-10-31T21:03:00.1"#.as_bytes(),
        concat!(
            r#"{"v":2,"seance":"s","seq":2,"instant":"2026-10-31T21:03:00.123Z","origine":"humain","origine_foi""#,
            r#":"declare","type":"main.fin","vague":"a","main":1,"tentative":1,"couleur":"vert","foi":"constate","#,
            r#""texte":"bonjour.txt écr"#,
        )
        .as_bytes(),
        b"{\"v\":2,\"seance\":\"s\",\"seq\":2,\"type\":\"main.verdict\",\"verdict\":\"class\xC3",
        &[0; 64],
    ];
    let debut = Evenement::Debut { vague: "A".into(), main: 1, tentative: 1 };
    // Le lancement d'après, dans sa séance : une vague entière, prise sur la déclaration de sa main.
    let suite = [
        Evenement::Vague {
            vague: "b".into(),
            n: 1,
            depot: "/srv/depot".into(),
            verifier: None,
            objectif: "Écrire salut.txt".into(),
        },
        Evenement::Debut { vague: "b".into(), main: 1, tentative: 1 },
        Evenement::Fin {
            vague: "b".into(),
            main: 1,
            tentative: 1,
            couleur: Couleur::Vert,
            foi: Foi::Declare,
            texte: "salut.txt écrit\nPREUVE: vert".into(),
            verif: None,
        },
        Evenement::Verdict { vague: "b".into(), main: 1, verdict: Verdict::Prise, foi: Foi::Declare },
    ];
    for (i, bout) in bouts.into_iter().enumerate() {
        let kraken = dir.join(i.to_string());
        fs::create_dir(&kraken).unwrap();
        Fil::seance(&kraken, "humain").noter(&debut).unwrap();
        OpenOptions::new().append(true).open(kraken.join(FICHIER)).unwrap().write_all(bout).unwrap();
        let lancement = Fil::seance(&kraken, "humain");
        for evenement in &suite {
            lancement.noter(evenement).unwrap();
        }
        let Lecture { notes, ecarts } = lire(&kraken).unwrap();
        assert_eq!(codes(&ecarts), [(2, 1001)], "seul le bout est illisible : {bout:?}");
        let evenements: Vec<Evenement> = notes.into_iter().map(|note| note.evenement).collect();
        assert_eq!(evenements[0], debut);
        assert_eq!(evenements[1..], suite);
        let vague = etat(&evenements).pop().unwrap();
        let rejoue = (vague.n, vague.objectif.as_str(), vague.mains[&1].verdict);
        assert_eq!(rejoue, (1, "Écrire salut.txt", Some((Verdict::Prise, Foi::Declare))), "{bout:?}");
        // Le bout est clos une fois, et les lignes entières d'après n'ajoutent rien.
        let retours = fs::read(kraken.join(FICHIER)).unwrap().into_iter().filter(|&octet| octet == b'\n').count();
        assert_eq!(retours, 6, "{bout:?}");
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn un_fil_absent_est_un_fil_vide() {
    // Ni dossier, ni fichier, ni ligne : un fil vide, sans écart.
    let absent = env::temp_dir().join(format!("kraken-fil-absent-{}", lot()));
    assert_eq!(lire(&absent).unwrap(), Lecture::default());
    let dir = dossier("sans-fil");
    assert_eq!(lire(&dir).unwrap(), Lecture::default());
    fs::write(dir.join(FICHIER), "").unwrap();
    assert_eq!(lire(&dir).unwrap(), Lecture::default());
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(windows)]
#[test]
fn sous_windows_un_lecteur_partage_lecture_ecriture_effacement() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = dossier("partage");
    let fil = Fil::seance(&dir, "humain");
    let debut = Evenement::Debut { vague: "v".into(), main: 1, tentative: 1 };
    fil.noter(&debut).unwrap();
    let chemin = dir.join(FICHIER);
    // Lecture, écriture et effacement partagés (7) : une note passe pendant qu'un lecteur tient le fil.
    let lecteur = OpenOptions::new().read(true).share_mode(7).open(&chemin).unwrap();
    fil.noter(&debut).unwrap();
    drop(lecteur);
    // Lecture seule partagée (1) : la note échoue, os error 32.
    let lecteur = OpenOptions::new().read(true).share_mode(1).open(&chemin).unwrap();
    let erreur = fil.noter(&debut).unwrap_err();
    assert!(erreur.to_string().contains("(os error 32)"), "{erreur}");
    drop(lecteur);
    fil.noter(&debut).unwrap();
    let Lecture { notes, ecarts } = lire(&dir).unwrap();
    assert!(ecarts.is_empty(), "la note ratée n'a pas pris de numéro : {ecarts:?}");
    assert_eq!(seqs(&notes), [1, 2, 3]);
    let _ = fs::remove_dir_all(&dir);
}
