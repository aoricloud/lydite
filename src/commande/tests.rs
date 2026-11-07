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
    // Juste, avec ou sans remarque : le code 0, et Kraken n'a rien fait d'autre.
    let juste =
        ecrire("juste.lydite", b"remarques 1\n\nremarque 1\nfichier : a.txt\nbloquante : oui\n    A reprendre.\n");
    assert_eq!(remarques_cmd(&juste), Ok(ExitCode::SUCCESS));
    assert_eq!(remarques_cmd(&ecrire("vide.lydite", b"remarques 1\n")), Ok(ExitCode::SUCCESS));
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
