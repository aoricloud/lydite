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

// Les deux moitiés d'un en-tête PEM de clé privée, chacune sur sa ligne : aucune ligne ne porte les deux.
const ENTETE_DEBUT: &str = "-----BEGIN ";
const ENTETE_FIN: &str = "PRIVATE KEY-----";

// Chaque exemple de clé se fabrique à l'exécution : un préfixe, puis un corps répété.
fn anthropique(corps: usize) -> String {
    format!("sk-{}{}", "ant-", "a".repeat(corps))
}

fn sk(corps: usize) -> String {
    format!("sk-{}", "a".repeat(corps))
}

fn github(prefixe: &str, corps: usize) -> String {
    format!("{prefixe}{}", "a".repeat(corps))
}

fn aws(prefixe: &str, corps: usize) -> String {
    format!("{prefixe}{}", "B".repeat(corps))
}

fn backblaze(corps: usize) -> String {
    format!("K00{}", "a".repeat(corps))
}

fn pem(genre: &str) -> String {
    format!("{ENTETE_DEBUT}{genre}{ENTETE_FIN}")
}

fn cle(chemin: &str, ligne: u64, nom: &str) -> String {
    format!("`{chemin}`, line {ligne}: an added line has the shape of a key (`{nom}`)")
}

fn fichier(chemin: &str, nom: &str) -> String {
    format!("`{chemin}`: a file that usually holds secrets is added or changed (`{nom}`)")
}

/// Les points d'une seule ligne ajoutée (la ligne 2 de la tête) dans un fichier `notes.txt`.
fn points_de_la_ligne(ligne: &str) -> Vec<String> {
    points_des_lignes_ajoutees("notes.txt", &[ligne])
}

#[test]
fn une_cle_ajoutee_est_relevee_avec_le_numero_de_la_tete_et_sans_etre_recopiee() {
    let ligne = anthropique(24);
    let diff = format!("--- a/src/x.rs\n+++ b/src/x.rs\n@@ -10,2 +12,3 @@\n a\n+{ligne}\n b\n");
    let points = points(&diff);
    assert_eq!(points, [cle("src/x.rs", 13, "Anthropic key")]);
    assert!(!points[0].contains(&"a".repeat(8)));
}

#[test]
fn chaque_forme_de_cle_rend_son_point_et_son_nom() {
    let formes = [
        (anthropique(20), "Anthropic key"),
        (anthropique(40), "Anthropic key"),
        (format!("sk-ant-{}", "a_-".repeat(7)), "Anthropic key"),
        (sk(30), "sk- key"),
        (format!("sk-{}", "a_-".repeat(10)), "sk- key"),
        (github("ghp_", 30), "GitHub token"),
        (github("gho_", 30), "GitHub token"),
        (github("ghu_", 30), "GitHub token"),
        (github("ghs_", 30), "GitHub token"),
        (github("ghr_", 30), "GitHub token"),
        (github("ghp_", 45), "GitHub token"),
        (format!("github_pat_{}", "a_".repeat(15)), "GitHub token"),
        (aws("AKIA", 16), "AWS access key ID"),
        (aws("ASIA", 16), "AWS access key ID"),
        (format!("ASIA{}", "7".repeat(16)), "AWS access key ID"),
        (format!("{}\"", aws("AKIA", 16)), "AWS access key ID"),
        (format!("{}_x", aws("AKIA", 16)), "AWS access key ID"),
        (backblaze(20), "Backblaze key"),
        (format!("K00{}", "a/+b".repeat(5)), "Backblaze key"),
        (pem("RSA "), "PEM private key"),
        (pem("OPENSSH "), "PEM private key"),
        (pem("ENCRYPTED "), "PEM private key"),
        (pem(""), "PEM private key"),
        (format!("x{}", pem("EC ")), "PEM private key"),
    ];
    for (ligne, nom) in formes {
        assert_eq!(points_de_la_ligne(&ligne), [cle("notes.txt", 2, nom)], "{nom}");
    }
}

#[test]
fn une_forme_trop_courte_ou_incomplete_ne_rend_rien() {
    let neutres = [
        anthropique(19),
        sk(29),
        github("ghp_", 29),
        format!("{}_", github("ghp_", 29)),
        format!("github_pat_{}", "a".repeat(29)),
        aws("AKIA", 15),
        aws("AKIA", 17),
        format!("{}z", aws("AKIA", 16)),
        backblaze(19),
        format!("K00{}-", "a".repeat(19)),
        format!("mask-{}", "a".repeat(32)),
        format!("x{}", aws("AKIA", 16)),
        ENTETE_DEBUT.to_string(),
        ENTETE_FIN.to_string(),
        format!("{ENTETE_FIN}{ENTETE_DEBUT}"),
        format!("akia{}", "B".repeat(16)),
        format!("SK-{}", "a".repeat(30)),
        format!("{} {}", "sk-", "a".repeat(40)),
    ];
    for ligne in neutres {
        assert_eq!(points_de_la_ligne(&ligne), Vec::<String>::new(), "{ligne}");
    }
    // Le début d'un en-tête sur une ligne et la fin sur la suivante : aucune des deux ne porte les deux.
    let deux = points_des_lignes_ajoutees("notes.txt", &[ENTETE_DEBUT, ENTETE_FIN]);
    assert_eq!(deux, Vec::<String>::new());
    // Une ligne retirée et une ligne de contexte ne rendent jamais de point de clé.
    let jeton = github("ghp_", 30);
    let retiree = format!("--- a/notes.txt\n+++ b/notes.txt\n@@ -1,2 +1 @@\n-{jeton}\n ctx\n");
    assert_eq!(points(&retiree), Vec::<String>::new());
    let contexte = format!("--- a/notes.txt\n+++ b/notes.txt\n@@ -1,2 +1,2 @@\n {jeton}\n+rien\n");
    assert_eq!(points(&contexte), Vec::<String>::new());
}

#[test]
fn la_premiere_forme_de_la_liste_l_emporte_et_une_ligne_rend_un_point_de_cle_au_plus() {
    let jeton_puis_anthropic = format!("{} {}", github("ghp_", 30), anthropique(20));
    assert_eq!(points_de_la_ligne(&jeton_puis_anthropic), [cle("notes.txt", 2, "Anthropic key")]);
    let sk_puis_pem = format!("{} {}", sk(30), pem("RSA "));
    assert_eq!(points_de_la_ligne(&sk_puis_pem), [cle("notes.txt", 2, "sk- key")]);
    // Le préfixe d'Anthropic suivi de 40 caractères porte aussi la forme `sk-` : la liste donne `Anthropic key`.
    assert_eq!(points_de_la_ligne(&anthropique(40)), [cle("notes.txt", 2, "Anthropic key")]);
    assert_eq!(points_de_la_ligne(&sk(30)), [cle("notes.txt", 2, "sk- key")]);
}

#[test]
fn la_frontiere_et_la_casse_d_une_forme() {
    let cle_a = anthropique(24);
    for ligne in [
        format!("API_KEY=\"{cle_a}\""),
        format!("--key {cle_a}"),
        format!("x={cle_a}"),
        format!("a-{cle_a}"),
        format!("\t{cle_a}"),
        format!("é{cle_a}"),
        cle_a.clone(),
    ] {
        assert_eq!(points_de_la_ligne(&ligne), [cle("notes.txt", 2, "Anthropic key")], "{ligne}");
    }
    for ligne in [
        format!("x_{}", github("ghp_", 30)),
        format!("7{}", aws("AKIA", 16)),
        format!("ax{cle_a}"),
        format!("_{cle_a}"),
        format!("9{}", backblaze(20)),
    ] {
        assert_eq!(points_de_la_ligne(&ligne), Vec::<String>::new(), "{ligne}");
    }
}

#[test]
fn une_occurrence_valide_compte_apres_une_occurrence_trop_courte_ou_sans_frontiere() {
    let apres_mask = format!("mask-{} {}", "a".repeat(32), sk(30));
    assert_eq!(points_de_la_ligne(&apres_mask), [cle("notes.txt", 2, "sk- key")]);
    let apres_court = format!("{} {}", github("ghp_", 5), github("ghp_", 30));
    assert_eq!(points_de_la_ligne(&apres_court), [cle("notes.txt", 2, "GitHub token")]);
    let apres_colle = format!("x{} {}", github("ghp_", 30), github("ghs_", 30));
    assert_eq!(points_de_la_ligne(&apres_colle), [cle("notes.txt", 2, "GitHub token")]);
    let aws_apres_court = format!("{} {}", aws("AKIA", 15), aws("ASIA", 16));
    assert_eq!(points_de_la_ligne(&aws_apres_court), [cle("notes.txt", 2, "AWS access key ID")]);
    let anthropic_apres_court = format!("{} {}", anthropique(5), anthropique(20));
    assert_eq!(points_de_la_ligne(&anthropic_apres_court), [cle("notes.txt", 2, "Anthropic key")]);
}

fn diff_neuf(chemin: &str) -> String {
    format!("--- /dev/null\n+++ b/{chemin}\n@@ -0,0 +1 @@\n+A=1\n")
}

#[test]
fn un_fichier_qui_porte_d_ordinaire_des_secrets_rend_son_point() {
    for (chemin, nom) in [
        (".env", ".env"),
        ("config/.env.staging", ".env"),
        ("APP/.ENV", ".env"),
        (".env.production", ".env"),
        ("deploy/id_ed25519", "id_ed25519"),
        ("deploy/ID_RSA", "id_rsa"),
        ("id_dsa", "id_dsa"),
        ("a/b/id_ecdsa", "id_ecdsa"),
    ] {
        assert_eq!(points(&diff_neuf(chemin)), [fichier(chemin, nom)], "{chemin}");
    }
    for chemin in [
        ".env.example",
        ".ENV.Example",
        ".env.sample",
        ".env.template",
        "id_rsa.pub",
        "src/env.rs",
        ".envrc",
        "id_rsa_old",
        "my.env",
    ] {
        assert_eq!(points(&diff_neuf(chemin)), Vec::<String>::new(), "{chemin}");
    }
    // Un fichier retiré n'en rend pas.
    let retire = "--- a/.env\n+++ /dev/null\n@@ -1 +0,0 @@\n-A=1\n";
    assert_eq!(points(retire), Vec::<String>::new());
    // Un fichier changé, en deux hunks, rend son point une fois.
    let deux = "--- a/.env\n+++ b/.env\n@@ -1 +1 @@\n-A=1\n+A=2\n@@ -9 +9 @@\n-B=1\n+B=2\n";
    assert_eq!(points(deux), [fichier(".env", ".env")]);
}

#[test]
fn un_env_avec_une_cle_rend_son_point_de_fichier_puis_son_point_de_ligne() {
    let diff = format!("--- /dev/null\n+++ b/.env\n@@ -0,0 +1,2 @@\n+A=1\n+TOKEN={}\n", sk(30));
    assert_eq!(points(&diff), [fichier(".env", ".env"), cle(".env", 2, "sk- key")]);
}

#[test]
fn un_marqueur_et_une_cle_sur_la_meme_ligne_rendent_deux_points_au_meme_numero() {
    let ligne = format!("#[ignore] // {}", sk(30));
    assert_eq!(
        points_des_lignes_ajoutees("src/y.rs", &[&ligne]),
        [marqueur("src/y.rs", 2, "#[ignore]"), cle("src/y.rs", 2, "sk- key")]
    );
}

#[test]
fn un_chemin_hostile_reste_en_code_pour_une_cle_et_pour_un_fichier() {
    let diff = format!("--- a/tests/@x`y.rs\n+++ b/tests/@x`y.rs\n@@ -1 +1,2 @@\n ctx\n+{}\n", anthropique(24));
    assert_eq!(
        points(&diff),
        ["`path with an unreadable name`, line 2: an added line has the shape of a key (`Anthropic key`)"]
    );
    assert_eq!(
        points(&diff_neuf("tests/@x`y/.env")),
        ["`path with an unreadable name`: a file that usually holds secrets is added or changed (`.env`)"]
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
