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
        assert!(refus.preuve.starts_with(&format!("lecture des remarques {} : ", chemin.display())), "{refus}");
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
    assert!(refus.preuve.starts_with(&format!("{} : ligne 9 :", fichier.display())), "{refus}");
    assert_eq!(refus.to_string().lines().count(), 4, "{refus}");
    // La première fausse, dans l'ordre des noms.
    poser("accrochee", "pas une leçon\n");
    let refus = lecons_cmd(&depot).unwrap_err();
    assert!(refus.preuve.contains("accrochee"), "{refus}");
    let _ = fs::remove_dir_all(&depot);
}
