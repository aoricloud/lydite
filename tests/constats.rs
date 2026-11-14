//! `lydite findings <report>`, joué sur le binaire lui-même, sur les douze rapports d'exemple du lecteur
//! (`src/coeur/constats/exemples/`) : la liste sur stdout, le plan d'un constat à l'octet, et les refus (6038, code de
//! sortie 2). La commande ne lit que le rapport et n'écrit que sur stdout.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// La règle du refus 6038, telle que la deuxième ligne d'un refus la dit.
const REGLE: &str =
    "  rule: A hermes-security report is read strictly, and a plan is written only for a finding it keeps.";

/// Un dossier neuf et vide, à ce test, dans le dossier temporaire du système.
fn neuf(nom: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lydite-constats-{nom}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Le binaire, dont le dossier de Lydite est `dossier` et dont le dossier personnel et `CLAUDE_CONFIG_DIR` vont à des
/// leurres sous le dossier temporaire, jamais créés : le vrai dossier personnel de l'humain n'est jamais lu.
fn kraken(dossier: &Path) -> Command {
    let leurres = std::env::temp_dir().join(format!("lydite-constats-leurres-{}", std::process::id()));
    let mut commande = Command::new(env!("CARGO_BIN_EXE_lydite"));
    commande
        .env("LYDITE_DOSSIER", dossier)
        .env("USERPROFILE", leurres.join("personnel"))
        .env("HOME", leurres.join("personnel"))
        .env_remove("XDG_STATE_HOME")
        .env("CLAUDE_CONFIG_DIR", leurres.join("config"));
    commande
}

/// Un flux, sans les `\r` qu'une plateforme aurait pu y mettre.
fn texte_de(flux: &[u8]) -> String {
    String::from_utf8(flux.to_vec()).unwrap().replace("\r\n", "\n")
}

/// Les douze exemples : leur nom, le chemin de leur rapport et le plan attendu pour leur premier constat.
fn exemples() -> Vec<(String, PathBuf, String)> {
    let dossier = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/coeur/constats/exemples");
    let mut exemples: Vec<(String, PathBuf, String)> = fs::read_dir(&dossier)
        .unwrap()
        .map(|entree| entree.unwrap().path())
        .filter(|chemin| chemin.extension().is_some_and(|extension| extension == "json"))
        .map(|chemin| {
            let plan = texte_de(&fs::read(chemin.with_extension("plan")).unwrap());
            (chemin.file_stem().unwrap().to_string_lossy().into_owned(), chemin, plan)
        })
        .collect();
    exemples.sort();
    assert_eq!(exemples.len(), 12, "les douze exemples");
    exemples
}

/// L'identité de la tâche d'un plan, que sa ligne `tache` dit.
fn identite_du_plan(plan: &str) -> &str {
    plan.lines().find_map(|ligne| ligne.strip_prefix("tache ")).unwrap()
}

#[test]
fn la_liste_de_chaque_exemple_est_un_objet_sur_une_ligne_de_stdout_et_rien_d_autre() {
    let dossier = neuf("liste");
    for (nom, rapport, plan) in exemples() {
        let sortie = kraken(&dossier).arg("findings").arg(&rapport).output().unwrap();
        assert_eq!(sortie.status.code(), Some(0), "{nom} : {}", texte_de(&sortie.stderr));
        assert!(sortie.stderr.is_empty(), "{nom} : {}", texte_de(&sortie.stderr));
        let texte = texte_de(&sortie.stdout);
        assert_eq!(texte.matches('\n').count(), 1, "{nom} : {texte}");
        assert!(texte.ends_with("}\n"), "{nom} : {texte}");
        let debut = r#"{"scan":"scan_0123456789abcdef01234567","findings":["#;
        assert!(texte.starts_with(debut), "{nom} : {texte}");
        // Le constat que dit le plan attendu est le premier de la liste.
        let premier = format!(r#"{debut}{{"id":"{}","#, identite_du_plan(&plan));
        assert!(texte.starts_with(&premier), "{nom} : {texte}");
    }
}

#[test]
fn la_liste_de_l_exemple_juste_est_exactement_celle_du_lecteur() {
    let dossier = neuf("juste");
    let (_, rapport, _) = exemples().remove(0);
    let sortie = kraken(&dossier).arg("findings").arg(&rapport).output().unwrap();
    let attendu = concat!(
        r#"{"scan":"scan_0123456789abcdef01234567","findings":[{"id":"hsf_010101010101010101010101","#,
        r#""kind":"finding","state":"source_supported","severity":"high","title":"Path traversal in the archive "#,
        r#"reader","where":["src/archive.rs:42"]}],"set_aside":[]}"#,
        "\n"
    );
    assert_eq!(texte_de(&sortie.stdout), attendu);
}

#[test]
fn le_plan_de_chaque_exemple_est_son_fichier_a_l_octet_et_rien_d_autre() {
    let dossier = neuf("plan");
    for (nom, rapport, plan) in exemples() {
        for (commande, verifier) in [("findings", "--verify"), ("constats", "--verifier")] {
            let sortie = kraken(&dossier)
                .arg(commande)
                .arg(&rapport)
                .args(["--plan", identite_du_plan(&plan), verifier, "cargo test"])
                .output()
                .unwrap();
            assert_eq!(sortie.status.code(), Some(0), "{nom} : {}", texte_de(&sortie.stderr));
            assert!(sortie.stderr.is_empty(), "{nom} : {}", texte_de(&sortie.stderr));
            assert_eq!(texte_de(&sortie.stdout), plan, "{nom}");
        }
    }
}

/// Le refus d'une sortie : le code 2, rien sur stdout, et quatre lignes sur stderr, dont la première porte `entete` et
/// la troisième `vu`.
fn refus_de(sortie: &Output, entete: &str, vu: &str) -> Vec<String> {
    let erreur = texte_de(&sortie.stderr);
    assert_eq!(sortie.status.code(), Some(2), "{erreur}");
    assert!(sortie.stdout.is_empty(), "{}", texte_de(&sortie.stdout));
    let lignes: Vec<String> = erreur.lines().map(String::from).collect();
    assert_eq!(lignes.len(), 4, "{erreur}");
    assert_eq!(lignes[0], entete, "{erreur}");
    assert!(lignes[2].starts_with("  seen: ") && lignes[2].contains(vu), "devait dire « {vu} » : {erreur}");
    lignes
}

#[test]
fn un_rapport_ou_un_plan_refuse_est_le_refus_6038_et_sort_en_2() {
    let dossier = neuf("refus");
    let juste = exemples().remove(0).1;
    let rapport = |nom: &str, texte: &str| {
        let chemin = dossier.join(nom);
        fs::write(&chemin, texte).unwrap();
        chemin
    };
    let mal_ecrit = rapport("mal-ecrit.json", "plan 1\n");
    // Un rapport où le scanner a rejeté un constat : il est écarté.
    let ecarte = rapport(
        "ecarte.json",
        concat!(
            r#"{"documentType":"hermes-security.findings","schemaVersion":"1.0","#,
            r#""scanId":"scan_0123456789abcdef01234567","findings":[{"findingId":"hsf_c3c3c3c3c3c3c3c3c3c3c3c3","#,
            r#""evidenceState":"rejected","title":"u","locations":[{"path":"src/a.rs"}]}]}"#
        ),
    );
    let entete = "lydite : 6038 lancer.rapport_refuse";
    let id_juste = "hsf_010101010101010101010101";
    let cas: [(&Path, &[&str], &str); 6] = [
        (&mal_ecrit, &[], "The report is not JSON."),
        (&mal_ecrit, &["--plan", id_juste, "--verify", "cargo test"], "The report is not JSON."),
        (&juste, &["--plan", "x", "--verify", "cargo test"], "is not well-formed"),
        (&juste, &["--plan", "hsf_ffffffffffffffffffffffff", "--verify", "cargo test"], "is not in the report"),
        (&ecarte, &["--plan", "hsf_c3c3c3c3c3c3c3c3c3c3c3c3", "--verify", "cargo test"], "is set aside by the reader"),
        (&juste, &["--plan", id_juste, "--verify", "  "], "The proof is blank."),
    ];
    for (chemin, plus, vu) in cas {
        let sortie = kraken(&dossier).arg("findings").arg(chemin).args(plus).output().unwrap();
        let lignes = refus_de(&sortie, entete, vu);
        assert_eq!(lignes[1], REGLE);
    }
    // Un constat écarté n'est pas un refus de la liste : le rapport est lu, et la liste le dit.
    let sortie = kraken(&dossier).arg("findings").arg(&ecarte).output().unwrap();
    assert_eq!(sortie.status.code(), Some(0), "{}", texte_de(&sortie.stderr));
    let liste = texte_de(&sortie.stdout);
    assert!(liste.contains(r#""set_aside":[{"ref":"hsf_c3c3c3c3c3c3c3c3c3c3c3c3","why":"rejected by the scanner"}]"#));
}

#[test]
fn les_autres_refus_gardent_leur_code_et_rien_n_est_ecrit_ailleurs() {
    let dossier = neuf("autres");
    let juste = exemples().remove(0).1;
    // Un fichier absent : 6006. Une ligne de commande fausse : 1006, avant toute lecture.
    let absent = dossier.join("absent.json");
    let sortie = kraken(&dossier).arg("findings").arg(&absent).output().unwrap();
    refus_de(&sortie, "lydite : 6006 lancer.entree_sortie", "reading the report");
    let sortie = kraken(&dossier).arg("findings").arg(&juste).args(["--plan", "x"]).output().unwrap();
    refus_de(&sortie, "lydite : 1006 contrat.commande_mal_formee", "go together, or neither");
    let sortie = kraken(&dossier).arg("findings").output().unwrap();
    refus_de(&sortie, "lydite : 1006 contrat.commande_mal_formee", "the report file is missing");
    // Ni le dossier de Lydite ni le dossier courant ne reçoivent rien, refus ou liste.
    let (ici, depot) = (neuf("ici"), neuf("depot"));
    let copie = ici.join("rapport.json");
    fs::copy(&juste, &copie).unwrap();
    let variantes: [&[&str]; 3] =
        [&[], &["--plan", "hsf_010101010101010101010101", "--verify", "cargo test"], &["--plan", "x", "--verify", "y"]];
    for plus in variantes {
        let sortie = kraken(&depot).current_dir(&ici).arg("findings").arg("rapport.json").args(plus).output().unwrap();
        assert!(matches!(sortie.status.code(), Some(0 | 2)), "{}", texte_de(&sortie.stderr));
        let noms = |dossier: &Path| -> Vec<String> {
            fs::read_dir(dossier).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect()
        };
        assert_eq!(noms(&ici), ["rapport.json"]);
        assert_eq!(noms(&depot), Vec::<String>::new());
    }
}
