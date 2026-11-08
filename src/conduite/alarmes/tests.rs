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
fn les_points_sont_les_tests_modifies_les_assertions_retirees_et_les_tolerances_triees_par_ligne_de_la_base() {
    assert_eq!(
        points(DIFF),
        [
            "`src/b.rs`, line 3: assertion removed",
            "`src/b.rs`, line 3: tolerance changed",
            "`tests/a.rs`: test modified",
            "`tests/a.rs`, line 11: assertion removed",
            "`tests/a.rs`, line 13: tolerance changed",
        ]
    );
}

#[test]
fn les_points_sont_sans_doublon_et_un_diff_sans_signe_n_en_a_pas() {
    // Deux lignes de tolérance sur la même ligne de la base : un seul point.
    let diff = "--- a/s.rs\n+++ b/s.rs\n@@ -5 +5 @@\n-let tol = TOLERANCE;\n+let tol = TOLERANCE * 2;\n";
    assert_eq!(points(diff), ["`s.rs`, line 5: tolerance changed"]);
    assert_eq!(points("--- a/s.rs\n+++ b/s.rs\n@@ -1 +1 @@\n-a\n+b\n"), Vec::<String>::new());
    assert_eq!(points(""), Vec::<String>::new());
    // Une ligne de contenu qui ressemble à un en-tête (`--- `) reste du contenu, et un fichier supprimé garde son nom.
    let diff = "--- a/tests/t.rs\n+++ /dev/null\n@@ -1,2 +0,0 @@\n--- assert\n-fin\n";
    assert_eq!(points(diff), ["`tests/t.rs`: test modified", "`tests/t.rs`, line 1: assertion removed"]);
}

#[test]
fn un_chemin_hostile_reste_en_code() {
    let diff = "--- a/tests/@x`y.rs\n+++ b/tests/@x`y.rs\n@@ -1 +1 @@\n-assert!(a);\n+b\n";
    assert_eq!(
        points(diff),
        ["`chemin au nom illisible`: test modified", "`chemin au nom illisible`, line 1: assertion removed"]
    );
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
            "`src/b.rs`, line 1: assertion removed",
            "`tests/a.rs`: test modified",
            "`tests/a.rs`, line 2: assertion removed"
        ]
    );
    let seul = lire(&depot, &base, &tete, Some(&["src/".to_string()])).unwrap();
    assert_eq!(seul, ["`src/b.rs`, line 1: assertion removed"]);
    assert!(lire(&depot, &"0".repeat(40), &tete, None).is_err());
    let _ = fs::remove_dir_all(&racine);
}
