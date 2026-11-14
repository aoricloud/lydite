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

/// Les douze rapports d'exemple du lecteur : leur nom (`00-juste`) et le chemin de leur `.json`, dans l'ordre des noms.
fn les_exemples_du_lecteur() -> Vec<(String, std::path::PathBuf)> {
    let dossier = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/coeur/constats/exemples");
    let mut exemples: Vec<(String, std::path::PathBuf)> = fs::read_dir(dossier)
        .unwrap()
        .map(|entree| entree.unwrap().path())
        .filter(|chemin| chemin.extension().is_some_and(|extension| extension == "json"))
        .map(|chemin| (chemin.file_stem().unwrap().to_string_lossy().into_owned(), chemin))
        .collect();
    exemples.sort();
    assert_eq!(exemples.len(), 12, "les douze exemples : {exemples:?}");
    exemples
}

/// Le membre `nom` d'un objet JSON.
fn membre<'a>(valeur: &'a json::Json, nom: &str) -> &'a json::Json {
    let json::Json::Objet(membres) = valeur else { panic!("pas un objet : {valeur:?}") };
    membres.get(nom).unwrap_or_else(|| panic!("pas de membre {nom} : {membres:?}"))
}

/// Le texte d'une valeur JSON.
fn texte_json(valeur: &json::Json) -> &str {
    let json::Json::Texte(texte) = valeur else { panic!("pas un texte : {valeur:?}") };
    texte
}

/// Les éléments d'une liste JSON.
fn liste_json(valeur: &json::Json) -> &[json::Json] {
    let json::Json::Liste(liste) = valeur else { panic!("pas une liste : {valeur:?}") };
    liste
}

/// Les clés d'un objet JSON, dans l'ordre.
fn cles_json(valeur: &json::Json) -> Vec<&str> {
    let json::Json::Objet(membres) = valeur else { panic!("pas un objet : {valeur:?}") };
    membres.keys().map(String::as_str).collect()
}

#[test]
fn la_liste_des_constats_se_relit_comme_le_lecteur_la_donne_pour_les_douze_exemples() {
    for (nom, chemin) in les_exemples_du_lecteur() {
        let texte = texte_des_constats(&chemin, None).unwrap();
        // Un objet sur une ligne, fini par `\n`, dont aucun saut de ligne n'est brut.
        assert_eq!(texte.matches('\n').count(), 1, "{nom} : {texte}");
        assert!(texte.ends_with('\n') && !texte.contains('\r'), "{nom} : {texte}");
        let lu = constats::lire(&fs::read(&chemin).unwrap()).unwrap();
        let doc = json::document(&texte).unwrap_or_else(|e| panic!("{nom} : {e} : {texte}"));
        assert_eq!(cles_json(&doc), ["findings", "scan", "set_aside"], "{nom}");
        assert_eq!(texte_json(membre(&doc, "scan")), lu.scan, "{nom}");
        let gardes = liste_json(membre(&doc, "findings"));
        assert_eq!(gardes.len(), lu.constats.len(), "{nom}");
        for (vu, attendu) in gardes.iter().zip(&lu.constats) {
            assert_eq!(cles_json(vu), ["id", "kind", "severity", "state", "title", "where"], "{nom}");
            assert_eq!(texte_json(membre(vu, "id")), attendu.id, "{nom}");
            assert_eq!(texte_json(membre(vu, "kind")), attendu.sorte.nom(), "{nom}");
            assert_eq!(texte_json(membre(vu, "state")), attendu.etat, "{nom}");
            assert_eq!(texte_json(membre(vu, "severity")), attendu.gravite, "{nom}");
            assert_eq!(texte_json(membre(vu, "title")), attendu.titre, "{nom}");
            let lieux: Vec<&str> = liste_json(membre(vu, "where")).iter().map(texte_json).collect();
            assert_eq!(lieux, attendu.lieux, "{nom}");
        }
        let ecartes = liste_json(membre(&doc, "set_aside"));
        assert_eq!(ecartes.len(), lu.ecartes.len(), "{nom}");
        for (vu, attendu) in ecartes.iter().zip(&lu.ecartes) {
            assert_eq!(cles_json(vu), ["ref", "why"], "{nom}");
            assert_eq!(texte_json(membre(vu, "ref")), attendu.reference, "{nom}");
            assert_eq!(texte_json(membre(vu, "why")), attendu.pourquoi, "{nom}");
        }
    }
}

/// Un rapport juste : un constat gardé, un autre que le scanner a rejeté, et un candidat gardé.
const RAPPORT_JUSTE: &str = concat!(
    r#"{"documentType":"hermes-security.findings","schemaVersion":"1.0","scanId":"scan_0123456789abcdef01234567","#,
    r#""findings":[{"findingId":"hsf_a1a1a1a1a1a1a1a1a1a1a1a1","evidenceState":"source_supported","#,
    r#""title":"Guillemet \" et barre \\ et saut\nde ligne","locations":[{"path":"src/a.rs","startLine":3},"#,
    r#"{"path":"src/b.rs"}]},{"findingId":"hsf_c3c3c3c3c3c3c3c3c3c3c3c3","evidenceState":"rejected","#,
    r#""title":"u","locations":[{"path":"src/a.rs"}]}],"retained":[{"candidateId":"cand_b2b2b2b2b2b2b2b2b2b2b2b2","#,
    r#""evidenceState":"inconclusive","title":"v","locations":[{"path":"src/c.rs"}]}]}"#
);

#[test]
fn le_titre_d_un_rapport_s_echappe_dans_la_liste() {
    let dossier = racine("constats-echappement");
    let rapport = dossier.join("rapport.json");
    fs::write(&rapport, RAPPORT_JUSTE).unwrap();
    let texte = texte_des_constats(&rapport, None).unwrap();
    // Le guillemet et la barre inverse du titre sont échappés dans le texte écrit, et le saut de ligne, que le lecteur
    // a replié en espace, n'y est pas.
    assert!(texte.contains(r#""title":"Guillemet \" et barre \\ et saut de ligne""#), "{texte}");
    let doc = json::document(&texte).unwrap();
    let [premier, candidat] = liste_json(membre(&doc, "findings")) else { panic!("{texte}") };
    assert_eq!(texte_json(membre(premier, "title")), "Guillemet \" et barre \\ et saut de ligne");
    assert_eq!(texte_json(membre(candidat, "kind")), "candidate");
    let lieux: Vec<&str> = liste_json(membre(premier, "where")).iter().map(texte_json).collect();
    assert_eq!(lieux, ["src/a.rs:3", "src/b.rs"]);
    let [ecarte] = liste_json(membre(&doc, "set_aside")) else { panic!("{texte}") };
    assert_eq!(texte_json(membre(ecarte, "ref")), "hsf_c3c3c3c3c3c3c3c3c3c3c3c3");
    assert_eq!(texte_json(membre(ecarte, "why")), "rejected by the scanner");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn le_plan_d_un_constat_est_le_texte_du_lecteur_a_l_octet_pour_les_douze_exemples() {
    // Les fins de ligne d'une copie Windows des fichiers d'exemple reviennent à `\n`, des deux côtés.
    let normal = |texte: &str| texte.replace("\r\n", "\n");
    for (nom, chemin) in les_exemples_du_lecteur() {
        let lu = constats::lire(&fs::read(&chemin).unwrap()).unwrap();
        let premier = lu.constats.first().unwrap_or_else(|| panic!("{nom} : aucun constat gardé"));
        let plan = texte_des_constats(&chemin, Some((&premier.id, "cargo test"))).unwrap();
        let attendu = fs::read_to_string(chemin.with_extension("plan")).unwrap();
        assert_eq!(normal(&plan), normal(&attendu), "{nom}");
    }
}

/// Le refus 6038 de cette lecture : sa preuve dit `dit`, et il tient en quatre lignes.
fn refus_6038(chemin: &Path, plan: Option<(&str, &str)>, dit: &str) -> Refus {
    let refus = texte_des_constats(chemin, plan).unwrap_err();
    assert_eq!(refus.code, Code::RapportRefuse, "{refus}");
    assert_eq!(refus.code.numero(), 6038);
    assert!(refus.preuve.contains(dit), "la preuve devait dire « {dit} » : {refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    refus
}

#[test]
fn un_constat_a_planifier_mal_forme_absent_ou_ecarte_est_refuse_en_6038_en_disant_lequel() {
    let dossier = racine("constats-identite");
    let rapport = dossier.join("rapport.json");
    fs::write(&rapport, RAPPORT_JUSTE).unwrap();
    let plan = |identite: &'static str| Some((identite, "cargo test"));
    // Gardés : un constat, et un candidat.
    for juste in ["hsf_a1a1a1a1a1a1a1a1a1a1a1a1", "cand_b2b2b2b2b2b2b2b2b2b2b2b2"] {
        let texte = texte_des_constats(&rapport, plan(juste)).unwrap();
        assert!(texte.starts_with("plan 1\n") && texte.contains(&format!("tache {juste}\n")), "{texte}");
    }
    let majuscules = "hsf_A1A1A1A1A1A1A1A1A1A1A1A1";
    let mal_formes = ["", "x", "hsf_a1", majuscules, "scan_a1a1a1a1a1a1a1a1a1a1a1a1", "../etc/passwd"];
    for mal_forme in mal_formes {
        refus_6038(&rapport, plan(mal_forme), "is not well-formed");
    }
    // Un id d'un genre qui n'est pas celui de l'entrée : `cand_` à la place d'un `hsf_` n'est pas dans le rapport.
    refus_6038(&rapport, plan("cand_a1a1a1a1a1a1a1a1a1a1a1a1"), "cand_a1a1a1a1a1a1a1a1a1a1a1a1 is not in the report");
    refus_6038(&rapport, plan("hsf_d4d4d4d4d4d4d4d4d4d4d4d4"), "hsf_d4d4d4d4d4d4d4d4d4d4d4d4 is not in the report");
    let refus = refus_6038(&rapport, plan("hsf_c3c3c3c3c3c3c3c3c3c3c3c3"), "is set aside by the reader");
    assert!(refus.preuve.contains("rejected by the scanner"), "{refus}");
    // Une identité mal formée longue, ou avec un contrôle, ne se cite pas en entier ni crue.
    let longue = format!("hsf_{}\u{1b}[31m", "a".repeat(500));
    let refus = texte_des_constats(&rapport, Some((&longue, "cargo test"))).unwrap_err();
    assert!(refus.preuve.len() < 400 && !refus.preuve.contains('\u{1b}'), "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn une_preuve_refusee_par_le_lecteur_est_le_refus_6038_de_son_texte() {
    let dossier = racine("constats-preuve");
    let rapport = dossier.join("rapport.json");
    fs::write(&rapport, RAPPORT_JUSTE).unwrap();
    let id = "hsf_a1a1a1a1a1a1a1a1a1a1a1a1";
    refus_6038(&rapport, Some((id, "  ")), "The proof is blank.");
    refus_6038(&rapport, Some((id, "cargo test\nrm -rf x")), "The proof holds a control or line-break character.");
    refus_6038(&rapport, Some((id, &"x".repeat(111))), "The proof is over 110 characters.");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_rapport_refuse_par_le_lecteur_est_le_refus_6038_de_son_texte_et_rien_n_est_ecrit() {
    let dossier = racine("constats-refuse");
    let ecrire = |nom: &str, octets: &[u8]| {
        let chemin = dossier.join(nom);
        fs::write(&chemin, octets).unwrap();
        chemin
    };
    // Ce que le lecteur dit, mot pour mot : la preuve du refus est son texte entier.
    let faux = [("vide.json", b"".to_vec()), ("faux.json", b"[1]".to_vec()), ("plan.json", b"plan 1\n".to_vec())];
    for (nom, octets) in faux {
        let chemin = ecrire(nom, &octets);
        let attendu = constats::lire(&octets).unwrap_err();
        for plan in [None, Some(("hsf_a1a1a1a1a1a1a1a1a1a1a1a1", "cargo test"))] {
            let refus = refus_6038(&chemin, plan, &attendu);
            assert_eq!(refus.preuve, attendu, "{nom}");
        }
    }
    // Le plan de la ligne de commande n'est jamais lu avant le rapport : un rapport faux l'emporte sur un id mal formé.
    let sans_type = ecrire("sans-type.json", b"{}");
    refus_6038(&sans_type, Some(("x", "")), &constats::lire(b"{}").unwrap_err());
    // Plus de 4 194 304 octets : refusé du texte du lecteur ; à la borne, le lecteur dit « not JSON ».
    let borne = constats::OCTETS_MAXIMUM;
    let trop = ecrire("trop.json", &vec![b' '; borne + 1]);
    assert_eq!(refus_6038(&trop, None, "The report is over 4194304 bytes.").preuve, constats::rapport_trop_gros());
    let juste = ecrire("borne.json", &vec![b' '; borne]);
    refus_6038(&juste, None, "The report is not JSON.");
    let _ = fs::remove_dir_all(&dossier);
}

#[test]
fn un_rapport_qui_n_est_pas_un_fichier_ordinaire_ou_qu_on_ne_lit_pas_est_refuse() {
    let dossier = racine("constats-fichier");
    // Un dossier n'est pas un fichier ordinaire : 6038, qui le nomme.
    refus_6038(&dossier, None, "is not a regular file");
    // Un fichier absent : 6006, qui nomme son chemin.
    let absent = dossier.join("absent.json");
    let refus = texte_des_constats(&absent, None).unwrap_err();
    assert_eq!(refus.code, Code::EntreeSortie, "{refus}");
    assert!(refus.preuve.starts_with(&format!("reading the report {}: ", absent.display())), "{refus}");
    let _ = fs::remove_dir_all(&dossier);
}

#[cfg(unix)]
#[test]
fn un_rapport_trop_gros_est_pese_avant_d_etre_ouvert() {
    use std::os::unix::fs::PermissionsExt;
    // Sans droit de lecture, ouvert, ce fichier serait le refus 6006 : le 6038 vient donc de la pesée. (Un processus
    // qui lit tout, comme root, le lit : le lecteur refuse alors le même texte.)
    let dossier = racine("constats-pese");
    let trop = dossier.join("trop.json");
    fs::write(&trop, vec![b' '; constats::OCTETS_MAXIMUM + 1]).unwrap();
    fs::set_permissions(&trop, fs::Permissions::from_mode(0o000)).unwrap();
    assert_eq!(refus_6038(&trop, None, "The report is over 4194304 bytes.").preuve, constats::rapport_trop_gros());
    let _ = fs::remove_dir_all(&dossier);
}

#[cfg(unix)]
#[test]
fn un_lien_a_la_place_du_rapport_est_refuse_sans_etre_suivi() {
    // La cible est un rapport juste : suivi, le lien passerait. Seul le refus de le suivre le fait voir.
    let dossier = racine("constats-lien");
    let vrai = dossier.join("vrai.json");
    fs::write(&vrai, RAPPORT_JUSTE).unwrap();
    let lien = dossier.join("lien.json");
    std::os::unix::fs::symlink(&vrai, &lien).unwrap();
    assert!(texte_des_constats(&vrai, None).is_ok());
    refus_6038(&lien, None, "is a symbolic link, which is not followed");
    let _ = fs::remove_dir_all(&dossier);
}
