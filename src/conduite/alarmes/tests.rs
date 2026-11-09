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
            "`chemin au nom illisible`: a test of the base changed (1 lines removed)",
            "`chemin au nom illisible`, line 1: an assertion removed or changed"
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
    // Pas deux caractères : un préfixe de deux se couperait comme `a/` et `b/`, et le test ne pourrait pas échouer.
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
