use super::*;
use crate::essais::racine;
use std::fs;

#[test]
fn kraken_remarques_lit_le_fichier_et_sort_en_0_ou_2() {
    let dossier = racine("remarques-cmd");
    let ecrire = |nom: &str, octets: &[u8]| {
        let chemin = dossier.join(nom);
        fs::write(&chemin, octets).unwrap();
        chemin
    };
    // Juste, en forme 2 (celle du relecteur), avec ou sans remarque : le code 0, et Kraken n'a rien fait d'autre.
    let blocs = "rejoue\n    Rien de rejoué.\n\nnon rejoue\n    Rien.\n";
    let juste = ecrire(
        "juste.lydite",
        format!("remarques 2\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    A reprendre.\n\n{blocs}").as_bytes(),
    );
    assert_eq!(remarques_cmd(&juste), Ok(ExitCode::SUCCESS));
    assert_eq!(
        remarques_cmd(&ecrire("vide.lydite", format!("remarques 2\n\n{blocs}").as_bytes())),
        Ok(ExitCode::SUCCESS)
    );
    // La forme 1 est refusée en 7015, comme le juge du relecteur la refuse.
    let un = ecrire("un.lydite", b"remarques 1\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    A reprendre.\n");
    let refus = remarques_cmd(&un).unwrap_err();
    assert_eq!(refus.code, Code::RemarquesIllisibles, "{refus}");
    assert_eq!(remarques_cmd(&ecrire("un-vide.lydite", b"remarques 1\n")).unwrap_err().code, Code::RemarquesIllisibles);
    // Faux, ou plus de 1 Mo : le refus 7015, qui sort en 2 comme tout refus.
    for (nom, octets) in [("faux.lydite", b"plan 1\n".to_vec()), ("lourd.lydite", vec![b'x'; 1_000_001])] {
        let refus = remarques_cmd(&ecrire(nom, &octets)).unwrap_err();
        assert_eq!(refus.code, Code::RemarquesIllisibles, "{nom} : {refus}");
        assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    }
    // Un fichier qu'on ne lit pas, absent ou dossier : le refus 6006, qui nomme son chemin.
    for chemin in [dossier.join("absent.lydite"), dossier.clone()] {
        let refus = remarques_cmd(&chemin).unwrap_err();
        assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
        assert!(refus.preuve.starts_with(&format!("reading the remarks {}: ", chemin.display())), "{refus}");
    }
    let _ = fs::remove_dir_all(&dossier);
}

/// Une leçon juste, nommée `nom`.
fn lecon_juste(nom: &str) -> String {
    format!(
        "---\nname: {nom}\ndescription: Une règle.\n---\n\n## What happened\nA.\n\n## Why\nB.\n\n\
         ## How it is checked\nC.\n\n## Pitfall\nD.\n\n## Files\n- src/a.rs\n\n## Falls when\nE.\n"
    )
}

#[test]
fn kraken_lecons_lit_les_lecons_du_depot_et_sort_en_0_ou_2() {
    let depot = racine("lecons-cmd");
    let skills = depot.join(".agents").join("skills");
    let poser = |nom: &str, texte: &str| {
        fs::create_dir_all(skills.join(nom)).unwrap();
        fs::write(skills.join(nom).join("SKILL.md"), texte).unwrap();
    };
    // Sans `.agents/skills` : aucune leçon.
    assert_eq!(lecons_cmd(&depot), Ok(ExitCode::SUCCESS));
    // Une leçon juste, et un sous-dossier sans `SKILL.md`, qui n'est pas une leçon.
    poser("bonne", &lecon_juste("bonne"));
    fs::create_dir_all(skills.join("vide")).unwrap();
    assert_eq!(lecons_cmd(&depot), Ok(ExitCode::SUCCESS));
    // Une juste et une fausse : le refus de la fausse, avec son fichier et sa ligne.
    poser("fausse", &lecon_juste("fausse").replace("## Why\nB.\n\n", ""));
    let refus = lecons_cmd(&depot).unwrap_err();
    assert_eq!(refus.code, Code::LeconMalFormee, "{refus}");
    let fichier = skills.join("fausse").join("SKILL.md");
    assert!(refus.preuve.starts_with(&format!("{} : line 9:", fichier.display())), "{refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    // La première fausse, dans l'ordre des noms.
    poser("accrochee", "pas une leçon\n");
    let refus = lecons_cmd(&depot).unwrap_err();
    assert!(refus.preuve.contains("accrochee"), "{refus}");
    let _ = fs::remove_dir_all(&depot);
}

/// Un plan écrit à la main d'une tâche `a` qui pose ces fichiers : (chemin, empreinte).
fn plan_posant(poses: &[(&str, String)]) -> String {
    let lignes: String =
        poses.iter().map(|(fichier, empreinte)| format!("place : {fichier} : {empreinte}\n")).collect();
    format!("plan 1\nobjectif : o\npreuve : p\n\ntache a\n{lignes}    x\n")
}

/// Le dossier `<plan>.files` d'un plan.
fn dossier_des_fichiers(plan: &Path) -> std::path::PathBuf {
    let mut nom = plan.as_os_str().to_os_string();
    nom.push(".files");
    nom.into()
}

/// Écrit le contenu du fichier `chemin` sous `<plan>.files/`.
fn poser(plan: &Path, chemin: &str, octets: &[u8]) {
    let cible = dossier_des_fichiers(plan).join(chemin);
    fs::create_dir_all(cible.parent().unwrap()).unwrap();
    fs::write(cible, octets).unwrap();
}

/// Le refus 6036 de ce plan, par `lire_plan` et, pareil, par `lydite plan` : la preuve, qui doit nommer le fichier.
fn refus_6036(plan: &Path, dit: &str) -> Refus {
    let refus = lire_plan(plan, plan::Mode::Main).unwrap_err();
    assert_eq!(refus.code, Code::FichierPoseRefuse, "{refus}");
    assert_eq!(refus.code.numero(), 6036);
    assert!(refus.preuve.contains(dit), "la preuve devait dire « {dit} » : {refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    assert_eq!(plan_cmd(plan, false).unwrap_err(), refus);
    refus
}

#[test]
fn lydite_plan_lit_le_contenu_des_fichiers_poses_et_sort_en_0() {
    let dossier = racine("plan-place");
    let plan = dossier.join("plan.txt");
    // Des octets tels quels : un CRLF, un octet hors de l'UTF-8, un NUL ; et un fichier de 4 Mio, la borne même.
    let (a, lourd): (&[u8], _) = (b"a\r\n\xff\x00z", vec![b'x'; 4 * 1024 * 1024]);
    poser(&plan, "src/sous/a.rs", a);
    poser(&plan, "README.md", &lourd);
    let poses = [("src/sous/a.rs", hex(&sha256(a))), ("README.md", hex(&sha256(&lourd)))];
    fs::write(&plan, plan_posant(&poses)).unwrap();
    assert_eq!(plan_cmd(&plan, false), Ok(ExitCode::SUCCESS));
    let (_, _, empreinte, posees) = lire_plan(&plan, plan::Mode::Main).unwrap();
    // L'empreinte du plan reste celle de ses octets, et le contenu est rendu dans l'ordre du plan.
    assert_eq!(empreinte, hex(&sha256(&fs::read(&plan).unwrap())));
    let attendu = [("src/sous/a.rs", a.to_vec()), ("README.md", lourd)];
    let attendu: Vec<Posee> =
        attendu.into_iter().map(|(f, octets)| Posee { tache: "a".into(), fichier: f.into(), octets }).collect();
    assert_eq!(posees, attendu);
    // Un plan sans `place` ne lit rien de plus : il n'a ni dossier `.files` ni contenu.
    let sans = dossier.join("sans.txt");
    fs::write(&sans, "plan 1\nobjectif : o\npreuve : p\n\ntache a\n    x\n").unwrap();
    assert!(!dossier_des_fichiers(&sans).exists());
    assert_eq!(plan_cmd(&sans, false), Ok(ExitCode::SUCCESS));
    assert_eq!(lire_plan(&sans, plan::Mode::Main).unwrap().3, []);
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_contenu_de_fichier_pose_absent_trop_gros_ou_d_une_autre_empreinte_est_le_refus_6036() {
    let a = (b"contenu de a\n" as &[u8], hex(&sha256(b"contenu de a\n")));
    let plan_de = |nom: &str| {
        let dossier = racine(nom);
        let plan = dossier.join("plan.txt");
        fs::write(&plan, plan_posant(&[("src/a.rs", a.1.clone())])).unwrap();
        (dossier, plan)
    };
    // Le plan est juste, et le contenu aussi : rien n'est refusé, ce qui garde les cas qui suivent.
    let (dossier, plan) = plan_de("pose-juste");
    poser(&plan, "src/a.rs", a.0);
    assert_eq!(lire_plan(&plan, plan::Mode::Main).unwrap().3.len(), 1);
    let _ = fs::remove_dir_all(&dossier);
    // Aucun dossier `.files`, puis un dossier sans le fichier.
    let (dossier, plan) = plan_de("pose-sans-dossier");
    refus_6036(&plan, "`src/a.rs`");
    refus_6036(&plan, "is missing");
    fs::create_dir_all(dossier_des_fichiers(&plan).join("src")).unwrap();
    refus_6036(&plan, "is missing");
    // Le fichier est un dossier.
    fs::create_dir_all(dossier_des_fichiers(&plan).join("src").join("a.rs")).unwrap();
    refus_6036(&plan, "is not a regular file");
    let _ = fs::remove_dir_all(&dossier);
    // `.files` est un fichier, puis `src` en est un.
    let (dossier, plan) = plan_de("pose-dossier-fichier");
    fs::write(dossier_des_fichiers(&plan), b"x").unwrap();
    refus_6036(&plan, "is not a folder");
    fs::remove_file(dossier_des_fichiers(&plan)).unwrap();
    poser(&plan, "src", b"x");
    refus_6036(&plan, "is not a folder");
    let _ = fs::remove_dir_all(&dossier);
    // Plus de 4 Mio, d'un octet : le plan écrit l'empreinte juste de ces octets, et c'est la taille qui refuse.
    let lourd = vec![b'x'; 4 * 1024 * 1024 + 1];
    let (dossier, plan) = plan_de("pose-lourde");
    fs::write(&plan, plan_posant(&[("src/a.rs", hex(&sha256(&lourd)))])).unwrap();
    poser(&plan, "src/a.rs", &lourd);
    refus_6036(&plan, "more than 4 MiB");
    let _ = fs::remove_dir_all(&dossier);
    // Une autre empreinte : les deux sont dites.
    let (dossier, plan) = plan_de("pose-autre-empreinte");
    poser(&plan, "src/a.rs", b"autre contenu\n");
    let refus = refus_6036(&plan, "the SHA-256 of");
    assert!(refus.preuve.contains(&hex(&sha256(b"autre contenu\n"))), "{refus}");
    assert!(refus.preuve.contains(&format!("the plan writes {}", a.1)), "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_plan_pose_64_fichiers_au_plus() {
    let dossier = racine("pose-64");
    let plan = dossier.join("plan.txt");
    let fichiers: Vec<(String, String)> = (0..65)
        .map(|k| {
            let octets = format!("fichier {k}\n");
            poser(&plan, &format!("d/f{k}.txt"), octets.as_bytes());
            (format!("d/f{k}.txt"), hex(&sha256(octets.as_bytes())))
        })
        .collect();
    let en_plan = |n: usize| {
        let poses: Vec<(&str, String)> = fichiers[..n].iter().map(|(f, e)| (f.as_str(), e.clone())).collect();
        fs::write(&plan, plan_posant(&poses)).unwrap();
    };
    en_plan(64);
    assert_eq!(lire_plan(&plan, plan::Mode::Main).unwrap().3.len(), 64);
    en_plan(65);
    refus_6036(&plan, "it places 65 files, and 64 at most");
    let _ = fs::remove_dir_all(&dossier);
}

#[cfg(unix)]
#[test]
fn un_lien_symbolique_a_la_place_du_fichier_ou_d_un_dossier_est_le_refus_6036() {
    use std::os::unix::fs::symlink;
    // Chaque cible porte les bons octets : suivi, le lien passerait. Seul le refus de le suivre le fait voir.
    let a = b"contenu de a\n";
    let plan_de = |nom: &str| {
        let dossier = racine(nom);
        let plan = dossier.join("plan.txt");
        fs::write(&plan, plan_posant(&[("src/a.rs", hex(&sha256(a)))])).unwrap();
        let vrai = dossier.join("vrai");
        fs::create_dir_all(vrai.join("src")).unwrap();
        fs::write(vrai.join("src").join("a.rs"), a).unwrap();
        (dossier, plan, vrai)
    };
    // Le fichier est un lien.
    let (dossier, plan, vrai) = plan_de("pose-lien-fichier");
    fs::create_dir_all(dossier_des_fichiers(&plan).join("src")).unwrap();
    symlink(vrai.join("src").join("a.rs"), dossier_des_fichiers(&plan).join("src").join("a.rs")).unwrap();
    refus_6036(&plan, "is a symbolic link");
    let _ = fs::remove_dir_all(&dossier);
    // Un dossier du chemin est un lien.
    let (dossier, plan, vrai) = plan_de("pose-lien-sous-dossier");
    fs::create_dir_all(dossier_des_fichiers(&plan)).unwrap();
    symlink(vrai.join("src"), dossier_des_fichiers(&plan).join("src")).unwrap();
    refus_6036(&plan, "is a symbolic link");
    let _ = fs::remove_dir_all(&dossier);
    // Le dossier `.files` lui-même est un lien.
    let (dossier, plan, vrai) = plan_de("pose-lien-dossier");
    symlink(&vrai, dossier_des_fichiers(&plan)).unwrap();
    refus_6036(&plan, "is a symbolic link");
    let _ = fs::remove_dir_all(&dossier);
}
