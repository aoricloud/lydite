use super::*;
use crate::essais::{depot, ecrire, git, racine};
use std::fs;

/// Un diff : un fichier de test où une assertion et une tolérance changent, un fichier source où une assertion saute.
const DIFF: &str = "diff --git a/tests/a.rs b/tests/a.rs
--- a/tests/a.rs
+++ b/tests/a.rs
@@ -10,4 +10,4 @@ fn x() {
 un
-    assert_eq!(a, b);
+    assert_eq!(a, c);
 deux
-    let tolerance = 1;
+    let tolerance = 100;
diff --git a/src/b.rs b/src/b.rs
--- a/src/b.rs
+++ b/src/b.rs
@@ -3 +3,2 @@
-    assert!(ok);
\\ No newline at end of file
+    // sans
+    let epsilon = 0.5;
";

#[test]
fn les_points_sont_les_tests_retires_les_assertions_et_les_tolerances_triees_par_chemin_puis_ligne() {
    assert_eq!(
        points(DIFF),
        [
            "`src/b.rs`, line 3: an assertion removed or changed",
            "`tests/a.rs`: a test of the base changed (2 lines removed)",
            "`tests/a.rs`, line 11: an assertion removed or changed",
            "`tests/a.rs`, line 13: a tolerance removed or changed",
        ]
    );
}

#[test]
fn une_tolerance_elargie_est_relevee() {
    let diff = concat!(
        "--- a/src/s.rs\n+++ b/src/s.rs\n@@ -7 +7 @@\n",
        "-    if (a - b).abs() < 1e-6 {\n",
        "+    if (a - b).abs() < 1.0 {\n"
    );
    assert_eq!(points(diff), ["`src/s.rs`, line 7: a tolerance removed or changed"]);
}

#[test]
fn les_signes_d_une_tolerance() {
    let signes = ["EPSILON", "tolerance", "Tolerant", "approx_eq", "a.abs_diff(b)", "relative_eq!", "4 ulps", "x<1e-6"];
    for ligne in signes {
        let diff = format!("--- a/s.rs\n+++ b/s.rs\n@@ -1 +0,0 @@\n-{ligne}\n");
        assert_eq!(points(&diff), ["`s.rs`, line 1: a tolerance removed or changed"], "{ligne}");
    }
    for ligne in ["x.abs()", "a < b", "1e5", "e-3", "1e-"] {
        let diff = format!("--- a/s.rs\n+++ b/s.rs\n@@ -1 +0,0 @@\n-{ligne}\n");
        assert_eq!(points(&diff), Vec::<String>::new(), "{ligne}");
    }
    let diff = "--- a/s.rs\n+++ b/s.rs\n@@ -1 +0,0 @@\n-if x.abs() < y {}\n";
    assert_eq!(points(diff), ["`s.rs`, line 1: a tolerance removed or changed"]);
}

#[test]
fn un_test_neuf_seulement_ajoute_ne_rend_rien() {
    let diff = concat!(
        "--- /dev/null\n+++ b/tests/neuf.rs\n@@ -0,0 +1,3 @@\n",
        "+#[test]\n+fn t() { assert!(true); }\n+let epsilon = 1;\n"
    );
    assert_eq!(points(diff), Vec::<String>::new());
    let diff = "--- a/src/s.rs\n+++ b/src/s.rs\n@@ -1 +1,2 @@\n a\n+let epsilon = 1; assert!(a);\n";
    assert_eq!(points(diff), Vec::<String>::new());
}

#[test]
fn un_fichier_de_test_efface_rend_son_point_avec_son_nombre() {
    let diff = "--- a/tests/t.rs\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-un\n-deux\n-trois\n";
    assert_eq!(points(diff), ["`tests/t.rs`: a test of the base changed (3 lines removed)"]);
    // Une ligne retirée sans assertion rend le point du fichier, une fois, pour deux hunks.
    let diff = "--- a/tests/t.rs\n+++ b/tests/t.rs\n@@ -1 +1 @@\n-a\n+b\n@@ -9 +9 @@\n-c\n+d\n";
    assert_eq!(points(diff), ["`tests/t.rs`: a test of the base changed (2 lines removed)"]);
}

#[test]
fn une_ligne_d_assertion_et_de_tolerance_rend_le_seul_point_de_tolerance() {
    let diff = "--- a/src/s.rs\n+++ b/src/s.rs\n@@ -4 +4 @@\n-assert!((a - b).abs() < 1e-9);\n+assert!(a == b);\n";
    assert_eq!(points(diff), ["`src/s.rs`, line 4: a tolerance removed or changed"]);
}

#[test]
fn les_noms_de_fichiers_de_test() {
    for chemin in [
        "tests/a.rs",
        "src/test/a.rs",
        "a/Tests/b.py",
        "src/tests.rs",
        "src/Tests.rs",
        "test_a.py",
        "a_test.go",
        "a_tests.rs",
        "A_TEST.RS",
        "a.test.ts",
        "a.spec.js",
        "a.SPEC.js",
    ] {
        let diff = format!("--- a/{chemin}\n+++ b/{chemin}\n@@ -1 +1 @@\n-x\n+y\n");
        assert_eq!(points(&diff), [format!("`{chemin}`: a test of the base changed (1 lines removed)")], "{chemin}");
    }
    for chemin in ["src/contests.rs", "src/attest.rs", "src/a_test", "src/latest.rs", "src/testing/a.rs", "a_test"] {
        let diff = format!("--- a/{chemin}\n+++ b/{chemin}\n@@ -1 +1 @@\n-x\n+y\n");
        assert_eq!(points(&diff), Vec::<String>::new(), "{chemin}");
    }
}

#[test]
fn les_points_sont_sans_doublon_et_un_diff_sans_signe_n_en_a_pas() {
    // Un doublon de ligne retirée (le même diff deux fois) n'apparaît qu'une fois.
    let diff = "--- a/s.rs\n+++ b/s.rs\n@@ -5 +5 @@\n-let tol = TOLERANCE;\n+let tol = TOLERANCE * 2;\n";
    assert_eq!(points(diff), ["`s.rs`, line 5: a tolerance removed or changed"]);
    let deux = format!("{diff}{diff}");
    assert_eq!(points(&deux), ["`s.rs`, line 5: a tolerance removed or changed"]);
    assert_eq!(points("--- a/s.rs\n+++ b/s.rs\n@@ -1 +1 @@\n-a\n+b\n"), Vec::<String>::new());
    assert_eq!(points(""), Vec::<String>::new());
    // Une ligne de contenu qui ressemble à un en-tête (`--- `) reste du contenu, et un fichier supprimé garde son nom.
    let diff = "--- a/tests/t.rs\n+++ /dev/null\n@@ -1,2 +0,0 @@\n--- assert\n-fin\n";
    assert_eq!(
        points(diff),
        [
            "`tests/t.rs`: a test of the base changed (2 lines removed)",
            "`tests/t.rs`, line 1: an assertion removed or changed"
        ]
    );
}

#[test]
fn un_chemin_hostile_reste_en_code() {
    let diff = "--- a/tests/@x`y.rs\n+++ b/tests/@x`y.rs\n@@ -1 +1 @@\n-assert!(a);\n+b\n";
    assert_eq!(
        points(diff),
        [
            "`path with an unreadable name`: a test of the base changed (1 lines removed)",
            "`path with an unreadable name`, line 1: an assertion removed or changed"
        ]
    );
}

/// Le diff d'un fichier `chemin` qui garde une ligne de contexte et reçoit les `lignes` ajoutées, puis ses points.
fn points_des_lignes_ajoutees(chemin: &str, lignes: &[&str]) -> Vec<String> {
    let ajoutees: String = lignes.iter().map(|l| format!("+{l}\n")).collect();
    let diff = format!("--- a/{chemin}\n+++ b/{chemin}\n@@ -1 +1,{} @@\n ctx\n{ajoutees}", lignes.len() + 1);
    points(&diff)
}

fn marqueur(chemin: &str, ligne: u64, montre: &str) -> String {
    format!("`{chemin}`, line {ligne}: a marker added that skips or isolates a test (`{montre}`)")
}

#[test]
fn un_ignore_ajoute_est_releve_avec_le_numero_de_la_tete() {
    let diff = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -10,2 +12,3 @@\n a\n+#[ignore]\n #[test]\n";
    assert_eq!(points(diff), ["`src/x.rs`, line 13: a marker added that skips or isolates a test (`#[ignore]`)"]);
}

#[test]
fn les_marqueurs_rust_de_la_liste_sont_releves_et_les_autres_lignes_non() {
    assert_eq!(
        points_des_lignes_ajoutees("src/x.rs", &["    #[ignore = \"lent\"]"]),
        [marqueur("src/x.rs", 2, "#[ignore]")]
    );
    assert_eq!(points_des_lignes_ajoutees("src/x.rs", &["#[ignore]"]), [marqueur("src/x.rs", 2, "#[ignore]")]);
    assert_eq!(
        points_des_lignes_ajoutees("src/x.rs", &["#[cfg_attr(windows, ignore)]"]),
        [marqueur("src/x.rs", 2, "#[cfg_attr(…, ignore)]")]
    );
    assert_eq!(
        points_des_lignes_ajoutees("src/x.rs", &["#[cfg_attr(windows, ignore = \"x\")]"]),
        [marqueur("src/x.rs", 2, "#[cfg_attr(…, ignore)]")]
    );
    // `=` suit `ignore` sans blanc : seule la branche `=` du premier cas le voit.
    assert_eq!(points_des_lignes_ajoutees("src/x.rs", &["#[ignore=\"x\"]"]), [marqueur("src/x.rs", 2, "#[ignore]")]);
    // Dans `cfg_attr`, `ignore` suivi de `=`, d'un blanc, de `,` ; chaque suite a sa branche.
    for ligne in [
        "#[cfg_attr(windows, ignore=\"x\")]",
        "#[cfg_attr(windows, ignore )]",
        "#[cfg_attr(feature = \"noignore\", ignore, foo)]",
        // Un faux point voulu : le mot `ignore` seul, dans une chaîne, compte.
        "#[cfg_attr(docsrs, doc = \"Set this to ignore it\")]",
    ] {
        let attendu = [marqueur("src/x.rs", 2, "#[cfg_attr(…, ignore)]")];
        assert_eq!(points_des_lignes_ajoutees("src/x.rs", &[ligne]), attendu, "{ligne}");
    }
    for ligne in [
        "let s = \"#[ignore]\";",
        "// #[ignore]",
        "#[ignored]",
        "#[cfg_attr(not(test), expect(dead_code))]",
        "#[cfg_attr(windows, notignore)]",
        "#[cfg_attr(feature = \"noignore\")]",
        "#[test] #[ignore]",
    ] {
        assert_eq!(points_des_lignes_ajoutees("src/x.rs", &[ligne]), Vec::<String>::new(), "{ligne}");
    }
    // Une ligne retirée ou de contexte ne compte pas, même si elle porte le marqueur.
    let retiree = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1 @@\n-#[ignore]\n #[test]\n";
    assert_eq!(points(retiree), Vec::<String>::new());
    let contexte = "--- a/src/x.rs\n+++ b/src/x.rs\n@@ -1,2 +1,2 @@\n #[ignore]\n #[test]\n";
    assert_eq!(points(contexte), Vec::<String>::new());
}

#[test]
fn les_marqueurs_python_de_la_liste_sont_releves_avec_leur_nom() {
    let marqueurs = [
        ("@pytest.mark.skipif(x, reason=\"y\")", "pytest.mark.skipif"),
        ("@pytest.mark.skip(reason=\"y\")", "pytest.mark.skip"),
        ("@PyTest.Mark.Skip", "pytest.mark.skip"),
        ("@pytest.mark.xfail", "pytest.mark.xfail"),
        ("    pytest.skip(\"x\")", "pytest.skip"),
        ("    pytest.xfail(\"x\")", "pytest.xfail"),
        ("@unittest.skipUnless(LINUX, \"x\")", "unittest.skip"),
        ("@unittest.skipIf(WIN, \"x\")", "unittest.skip"),
        ("@unittest.skip(\"x\")", "unittest.skip"),
        ("@unittest.expectedFailure", "expectedFailure"),
        ("        self.skipTest(\"x\")", "skipTest"),
        ("raise SkipTest()", "skipTest"),
        // Les faux points voulus : un nom qui contient le marqueur, ou un commentaire, compte.
        ("unittest.skipped_total += 1", "unittest.skip"),
        ("# @pytest.mark.skip", "pytest.mark.skip"),
    ];
    for (ligne, montre) in marqueurs {
        let attendu = [marqueur("tests/test_a.py", 2, montre)];
        assert_eq!(points_des_lignes_ajoutees("tests/test_a.py", &[ligne]), attendu, "{ligne}");
    }
    // Le premier de la liste l'emporte, pas le premier de la ligne.
    let ligne = "@unittest.skip(\"x\") # pytest.mark.skipif";
    let attendu = [marqueur("tests/test_a.py", 2, "pytest.mark.skipif")];
    assert_eq!(points_des_lignes_ajoutees("tests/test_a.py", &[ligne]), attendu);
    let neutres = [
        "def test_a(): pass",
        "pytest.importorskip(\"x\")",
        "#[ignore]",
        "assert x",
        "@mark.skip",
        "@skipIf(x, \"y\")",
    ];
    for ligne in neutres {
        assert_eq!(points_des_lignes_ajoutees("tests/test_a.py", &[ligne]), Vec::<String>::new(), "{ligne}");
    }
}

#[test]
fn les_marqueurs_javascript_de_la_liste_sont_releves_et_les_autres_lignes_non() {
    let marqueurs = [
        ("it.only(\"a\", () => {});", ".only"),
        ("  describe.skip('a', () => {", ".skip"),
        ("test.skip.each([1])('a', f);", ".skip"),
        ("}); context.only.each(", ".only"),
        ("suite.only('a', f);", ".only"),
        ("suite.skip('a', f);", ".skip"),
        ("// it.only(", ".only"),
        ("xit(\"a\", f);", "xit"),
        ("xdescribe('a', f);", "xdescribe"),
        ("xtest('a', f);", "xtest"),
    ];
    for chemin in ["a.test.ts", "b.spec.JS", "src/c.tsx", "d.mjs", "e.jsx", "f.cjs", "g.mts", "h.cts", "i.js"] {
        for (ligne, montre) in marqueurs {
            assert_eq!(points_des_lignes_ajoutees(chemin, &[ligne]), [marqueur(chemin, 2, montre)], "{chemin} {ligne}");
        }
    }
    let neutres = [
        "submit.only(",
        "a.it.only(",
        "fixit(",
        "$it.only(",
        "IT.only(",
        "it.only",
        "it.skipped(",
        "it('a', f);",
        "fit(",
        "xit",
        "test.describe.only(",
        "it.skipIf(c)(",
        "this.skip();",
    ];
    for ligne in neutres {
        assert_eq!(points_des_lignes_ajoutees("a.test.ts", &[ligne]), Vec::<String>::new(), "{ligne}");
    }
    // Ni l'extension d'un autre langage, ni un nom sans extension de la liste.
    assert_eq!(points_des_lignes_ajoutees("src/x.rs", &["it.only("]), Vec::<String>::new());
    assert_eq!(points_des_lignes_ajoutees("a.test.json", &["it.only("]), Vec::<String>::new());
    assert_eq!(points_des_lignes_ajoutees("Makefile", &["#[ignore]"]), Vec::<String>::new());
}

#[test]
fn la_ligne_de_la_tete_et_celle_de_la_base_ne_se_melangent_pas_entre_deux_hunks() {
    let diff = concat!(
        "--- a/src/e.rs\n+++ b/src/e.rs\n",
        "@@ -3,2 +3,4 @@\n a\n+b\n+c\n d\n",
        "@@ -20,3 +22,3 @@\n e\n-    assert!(x);\n\\ No newline at end of file\n+#[ignore]\n f\n"
    );
    assert_eq!(
        points(diff),
        ["`src/e.rs`, line 21: an assertion removed or changed", &marqueur("src/e.rs", 23, "#[ignore]")[..]]
    );
    // Un fichier d'essai qui ne reçoit que des lignes ajoutées rend le seul point du marqueur.
    let neuf = "--- /dev/null\n+++ b/tests/neuf.rs\n@@ -0,0 +1,3 @@\n+#[test]\n+#[ignore]\n+fn t() {}\n";
    assert_eq!(points(neuf), [marqueur("tests/neuf.rs", 2, "#[ignore]")]);
    // Un marqueur ajouté dans un fichier d'essai dont une ligne est retirée garde les deux points.
    let mixte = "--- a/tests/t.rs\n+++ b/tests/t.rs\n@@ -1 +1,2 @@\n-a\n+#[ignore]\n+b\n";
    assert_eq!(
        points(mixte),
        ["`tests/t.rs`: a test of the base changed (1 lines removed)", &marqueur("tests/t.rs", 1, "#[ignore]")[..]]
    );
}

#[test]
fn un_chemin_hostile_reste_en_code_pour_un_marqueur_ajoute() {
    let diff = "--- a/tests/@x`y.rs\n+++ b/tests/@x`y.rs\n@@ -1 +1 @@\n-a\n+#[ignore]\n";
    assert_eq!(
        points(diff),
        [
            "`path with an unreadable name`: a test of the base changed (1 lines removed)",
            "`path with an unreadable name`, line 1: a marker added that skips or isolates a test (`#[ignore]`)"
        ]
    );
}

#[test]
fn lire_ignore_les_reglages_de_prefixe_du_depot() {
    let racine = racine("alarmes-prefixes");
    let depot = depot(&racine);
    let commiter = |fichiers: &[(&str, &str)]| {
        for (chemin, contenu) in fichiers {
            ecrire(&depot, chemin, contenu);
        }
        git(&depot, &["add", "-A"]);
        git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "f"]);
        git(&depot, &["rev-parse", "HEAD"])
    };
    let base = commiter(&[("tests/a.rs", "un\nassert!(a);\n"), ("src/b.rs", "assert!(b);\n")]);
    let tete = commiter(&[("tests/a.rs", "un\n"), ("src/b.rs", "rien\n")]);
    let attendu = [
        "`src/b.rs`, line 1: an assertion removed or changed",
        "`tests/a.rs`: a test of the base changed (1 lines removed)",
        "`tests/a.rs`, line 2: an assertion removed or changed",
    ];
    assert_eq!(lire(&depot, &base, &tete, None).unwrap(), attendu);
    git(&depot, &["config", "diff.noprefix", "true"]);
    assert_eq!(lire(&depot, &base, &tete, None).unwrap(), attendu);
    git(&depot, &["config", "--unset", "diff.noprefix"]);
    // Not two characters: a two-character prefix is cut like `a/` and `b/`, so the test could not fail.
    git(&depot, &["config", "diff.srcPrefix", "avant/"]);
    git(&depot, &["config", "diff.dstPrefix", "apres/"]);
    assert_eq!(lire(&depot, &base, &tete, None).unwrap(), attendu);
    let _ = fs::remove_dir_all(&racine);
}

#[test]
fn lire_donne_les_points_du_diff_de_git_limite_aux_fichiers() {
    let racine = racine("alarmes");
    let depot = depot(&racine);
    let commiter = |fichiers: &[(&str, &str)]| {
        for (chemin, contenu) in fichiers {
            ecrire(&depot, chemin, contenu);
        }
        git(&depot, &["add", "-A"]);
        git(&depot, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "f"]);
        git(&depot, &["rev-parse", "HEAD"])
    };
    let base = commiter(&[("tests/a.rs", "un\nassert!(a);\n"), ("src/b.rs", "assert!(b);\n")]);
    let tete = commiter(&[("tests/a.rs", "un\n"), ("src/b.rs", "rien\n")]);
    let tous = lire(&depot, &base, &tete, None).unwrap();
    assert_eq!(
        tous,
        [
            "`src/b.rs`, line 1: an assertion removed or changed",
            "`tests/a.rs`: a test of the base changed (1 lines removed)",
            "`tests/a.rs`, line 2: an assertion removed or changed"
        ]
    );
    let seul = lire(&depot, &base, &tete, Some(&["src/".to_string()])).unwrap();
    assert_eq!(seul, ["`src/b.rs`, line 1: an assertion removed or changed"]);
    assert!(lire(&depot, &"0".repeat(40), &tete, None).is_err());
    let _ = fs::remove_dir_all(&racine);
}
