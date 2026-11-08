"""Les essais de la garde (`.github/garde.py`), joués par la CI : chaque lieu, la liste et ses lignes, les motifs, les
forks, les noms que git cite ou supprime, et qu'aucun mot ni rien de ce qu'un motif trouve ne paraisse dans la sortie.

Les valeurs qui tomberaient sous un motif sont écrites en morceaux, pour que la garde ne les trouve pas dans ce fichier
quand il change ; les mots de la liste d'essai sont inventés.
"""

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ICI = Path(__file__).resolve().parent
GARDE = ICI / "garde.py"
MOT = "Brindefer"
MOT_ACCENTUE = "Zébrelune"
# Un BOM, un commentaire, une ligne vide : MOT est à la ligne 4, MOT_ACCENTUE à la ligne 5.
LISTE = chr(0xFEFF) + f"# essai\n\nzinzolin\n{MOT}\n  {MOT_ACCENTUE}  \r\n"
# Une ligne de plusieurs mots, à la ligne 1.
PLUSIEURS = "essai-un essai-deux\n"

RESEAU = "192." + "168.1.20"
RESEAU_V6 = "fd" + "7a:115c:a1e0::1"
CHEMIN_WINDOWS = "C:" + "\\Users\\" + "someone\\projet"
CHEMIN_LINUX = "/ho" + "me/someone/projet"
CHEMIN_MAC = "/Us" + "ers/someone/projet"
NOM_DE_RESEAU = "nas.l" + "an"
NOM_LOCAL = "portable.lo" + "cal"
COURRIEL = "someone@" + "mailbox.org"

PROPRE = {
    "sha": "a" * 40,
    "m": "Un message propre\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
    "a": {"name": "lydite", "email": "lydite@invalid"},
    "c": {"name": "GitHub", "email": "noreply@github.com"},
}


def bloc(chemin, *lignes, cite=None):
    """Le diff d'un fichier neuf qui ajoute ces lignes ; `cite` est le nom tel que git le cite (sans guillemets)."""
    a, b = (f'"a/{cite}"', f'"b/{cite}"') if cite else (f"a/{chemin}", f"b/{chemin}")
    ajouts = "".join(f"+{ligne}\n" for ligne in lignes)
    return (
        f"diff --git {a} {b}\nnew file mode 100644\nindex 0000000..1111111\n--- /dev/null\n+++ {b}\n"
        f"@@ -0,0 +1,{len(lignes)} @@\n{ajouts}"
    )


def renomme(avant, apres):
    """Le diff d'un renommage sans changement."""
    return f"diff --git a/{avant} b/{apres}\nsimilarity index 100%\nrename from {avant}\nrename to {apres}\n"


def supprime(chemin, *lignes):
    """Le diff d'un fichier supprimé."""
    retraits = "".join(f"-{ligne}\n" for ligne in lignes)
    return (
        f"diff --git a/{chemin} b/{chemin}\ndeleted file mode 100644\nindex 1111111..0000000\n--- a/{chemin}\n"
        f"+++ /dev/null\n@@ -1,{len(lignes)} +0,0 @@\n{retraits}"
    )


def jouer(diff="", commits=(PROPRE,), erreur=False, liste=LISTE, pr=None, **variables):
    """Joue la garde sur ces entrées (`pr` : le titre, le corps et la branche, comme l'API les écrit) : son code de
    sortie et tout ce qu'elle a écrit."""
    with tempfile.TemporaryDirectory() as dossier:
        Path(dossier, "diff").write_text(diff, encoding="utf-8")
        Path(dossier, "commits.jsonl").write_text("".join(json.dumps(c) + "\n" for c in commits), encoding="utf-8")
        if pr is not None:
            Path(dossier, "pr.json").write_text(json.dumps(pr), encoding="utf-8")
        if erreur:
            Path(dossier, "erreur").write_text("diff\n", encoding="utf-8")
        ecartees = ("GARDE", "LISTE", "TITRE", "CORPS", "BRANCHE")
        environ = {k: v for k, v in os.environ.items() if k not in ecartees}
        environ.update({"GARDE": liste, **variables})
        fini = subprocess.run(
            [sys.executable, "-I", str(GARDE), dossier], check=False, env=environ, capture_output=True, encoding="utf-8"
        )
    return fini.returncode, fini.stdout + fini.stderr


class Garde(unittest.TestCase):
    def sans_rien_de_trouve(self, sortie, *valeurs):
        for valeur in (MOT, MOT_ACCENTUE, *valeurs):
            self.assertNotIn(valeur.lower(), sortie.lower())

    def que_des_annotations(self, sortie):
        for ligne in sortie.splitlines():
            self.assertTrue(ligne.startswith(("::error title=garde::", "::notice title=garde::", "garde: ")), ligne)

    def test_ce_qui_est_propre_passe(self):
        # Les valeurs d'essai du dépôt passent : boucle locale, adresses d'essai, chemins inventés, routes de l'API.
        lignes = (
            "fn a() {}",
            "127.0.0.1",
            "/home/x/projet",
            "/home/é t/.lydite",
            "C:\\Users\\é t\\.lydite",
            "C:\\Users\\compte (perso)\\.lydite",
            "%USERPROFILE%",
            "~/.local/state/lydite",
            ".claude/settings.local.json",
            "CLAUDE.local.md",
            "git@github.com",
            "humain@exemple.test",
            "autre@exemple.invalid",
            "https://api.github.com/users/octocat",
            "169.254.169.254",
            "host.docker.internal",
        )
        pr = {"titre": "Un titre", "corps": "Un corps", "branche": "lydite/vague-1"}
        code, sortie = jouer(bloc("src/a.rs", *lignes), pr=pr)
        self.assertEqual(code, 0, sortie)
        self.assertIn("3 words of the list", sortie)
        self.que_des_annotations(sortie)

    def test_le_titre_le_corps_et_la_branche(self):
        pr = {"titre": f"Avec {MOT.upper()}", "corps": f"un {MOT_ACCENTUE.upper()}", "branche": MOT.lower()}
        code, sortie = jouer(pr=pr)
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in the title.", sortie)
        self.assertIn("Line 5 of the guard list appears in the body.", sortie)
        self.assertIn("Line 4 of the guard list appears in the branch name.", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_un_nom_neuf_tait_tous_les_chemins(self):
        code, sortie = jouer(bloc(f"src/{MOT.lower()}.rs", "fn a() {}") + bloc("src/b.rs", f"// {MOT}"))
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in a changed file name.", sortie)
        self.assertIn("Line 4 of the guard list appears in the added lines of a changed file.", sortie)
        self.assertNotIn("src/b.rs", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_un_nom_renomme_vers_un_mot_rougit(self):
        code, sortie = jouer(renomme("src/b.rs", f"src/{MOT.lower()}.rs"))
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in a changed file name.", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_les_noms_anciens_ne_comptent_pas(self):
        # Retirer un nom déjà publié, ou renommer le fichier qui le porte, ne rougit pas.
        diff = supprime(f"src/{MOT.lower()}.rs", "fn a() {}") + renomme(f"doc/{MOT.lower()}.md", "doc/b.md")
        code, sortie = jouer(diff)
        self.assertEqual(code, 0, sortie)

    def test_un_nom_cite_par_git_se_lit_en_clair(self):
        cite = "".join(f"\\{o:03o}" for o in f"{MOT_ACCENTUE.lower()}.rs".encode())
        code, sortie = jouer(bloc("", "fn a() {}", cite="src/" + cite))
        self.assertEqual(code, 1)
        self.assertIn("Line 5 of the guard list appears in a changed file name.", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_les_echappements_d_un_nom_cite(self):
        # `"`, une tabulation et un saut de ligne dans un nom : git les écrit en `\"`, `\t` et `\n`.
        code, sortie = jouer(bloc("", f"// {MOT}", cite='src/a\\"b\\tc\\nd.rs'))
        self.assertEqual(code, 1)
        self.assertIn('the added lines of src/a"b%09c%0Ad.rs.', sortie)
        self.que_des_annotations(sortie)

    def test_les_lignes_ajoutees_nomment_leur_fichier(self):
        # Une ligne ajoutée qui commence par « ++ » s'écrit « +++ » dans le bloc : elle compte comme ajoutée.
        diff = bloc("src/b.rs", "fn b() {}", f"++ {MOT}") + "\\ No newline at end of file\n"
        code, sortie = jouer(diff)
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in the added lines of src/b.rs.", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_un_mot_retire_par_un_commit_suivant_rougit(self):
        # Le diff net est vide ; celui du premier commit garde le mot, que main recevrait dans son histoire.
        premier = "From aaaa Mon Sep 17 00:00:00 2001\nSubject: [PATCH 1/2] ajout\n\n---\n" + bloc("src/a.rs", MOT)
        second = "-- \n2.43.0\n\nFrom bbbb Mon Sep 17 00:00:00 2001\nSubject: [PATCH 2/2] retrait\n\n---\n"
        code, sortie = jouer(premier + second + supprime("src/a.rs", MOT))
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in the added lines of src/a.rs.", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_les_commits(self):
        message = dict(PROPRE, sha="b" * 40, m=f"Un message\n\n{MOT}")
        auteur = dict(PROPRE, sha="c" * 40, a={"name": "x", "email": f"{MOT.lower()}@invalid"})
        code, sortie = jouer(commits=(PROPRE, message, auteur))
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in the message of commit bbbbbbb.", sortie)
        self.assertIn("Line 4 of the guard list appears in the author or committer of commit ccccccc.", sortie)
        self.assertNotIn("aaaaaaa", sortie)
        self.sans_rien_de_trouve(sortie)

    def test_les_motifs_generiques(self):
        cas = (
            (RESEAU, "A private network address"),
            (RESEAU_V6, "A private network address"),
            (CHEMIN_WINDOWS, "A personal path"),
            (CHEMIN_LINUX, "A personal path"),
            (CHEMIN_MAC, "A personal path"),
            (NOM_DE_RESEAU, "A private network name"),
            (NOM_LOCAL, "A private network name"),
            (COURRIEL, "An e-mail address"),
        )
        for valeur, trouve in cas:
            with self.subTest(trouve=trouve, valeur=valeur):
                code, sortie = jouer(bloc("src/a.rs", f"// {valeur}"), LISTE="fork")
                self.assertEqual(code, 1)
                self.assertIn(f"{trouve} appears in the added lines of src/a.rs.", sortie)
                self.sans_rien_de_trouve(sortie, valeur, "someone", "mailbox", "portable", "nas.")

    def test_le_texte_de_la_pr_ne_vient_pas_de_l_environnement(self):
        # Le journal d'Actions affiche l'environnement d'une étape : le texte de la PR n'y passe plus.
        code, sortie = jouer(TITRE=f"Avec {MOT}", CORPS=MOT, BRANCHE=MOT)
        self.assertEqual(code, 0, sortie)

    def test_un_fork_ne_lit_pas_la_liste(self):
        code, sortie = jouer(pr={"titre": f"Avec {MOT}"}, LISTE="fork")
        self.assertEqual(code, 0, sortie)
        self.assertIn("A pull request from a fork", sortie)
        self.assertNotIn("::error", sortie)

    def test_une_liste_vide_rougit(self):
        code, sortie = jouer(liste="# rien\n\n")
        self.assertEqual(code, 1)
        self.assertIn("The GARDE secret is empty or missing", sortie)

    def test_un_binaire_ne_se_lit_pas(self):
        neuf = "diff --git a/img.png b/img.png\nnew file mode 100644\nindex 0000000..1111111\n"
        neuf += "Binary files /dev/null and b/img.png differ\n"
        code, sortie = jouer(neuf)
        self.assertEqual(code, 1)
        self.assertIn("A binary file is changed: img.png; its content cannot be checked.", sortie)
        retire = "diff --git a/img.png b/img.png\ndeleted file mode 100644\nindex 1111111..0000000\n"
        retire += "Binary files a/img.png and /dev/null differ\n"
        self.assertEqual(jouer(retire)[0], 0)

    def test_une_lecture_echouee_rougit(self):
        code, sortie = jouer(erreur=True)
        self.assertEqual(code, 1)
        self.assertIn("could not be read from the API", sortie)

    def test_un_nom_ne_peut_pas_ouvrir_une_commande(self):
        code, sortie = jouer(bloc("src/a%0A::warning::x.rs", f"// {MOT}"))
        self.assertEqual(code, 1)
        self.assertIn("the added lines of src/a%250A::warning::x.rs.", sortie)
        self.que_des_annotations(sortie)

    def test_les_fichiers_de_la_garde_passent(self):
        # La garde, ses essais, ses workflows et le CLAUDE.md du dépôt, publiés tels quels, ne portent aucun motif.
        fichiers = [GARDE, ICI / "garde_test.py", ICI / "workflows" / "garde.yml", ICI / "workflows" / "ci.yml"]
        fichiers.append(ICI.parent / "CLAUDE.md")
        diff = "".join(bloc(f.name, *f.read_text(encoding="utf-8").splitlines()) for f in fichiers)
        code, sortie = jouer(diff, LISTE="fork")
        self.assertEqual(code, 0, sortie)

    # Le jeu d'essais commun avec `publiable` de Lydite (src/branchements/github/tests.rs), dans le même ordre : une
    # ligne de plusieurs mots se trouve aussi repliée, un mot seul reste comparé à la lettre.
    def test_une_ligne_de_plusieurs_mots_coupee_sur_deux_lignes_rougit(self):
        code, sortie = jouer(bloc("src/a.rs", "// essai-un", "// essai-deux"), liste=PLUSIEURS)
        self.assertEqual(code, 1, sortie)
        self.assertIn("Line 1 of the guard list appears in the added lines of src/a.rs.", sortie)
        self.sans_rien_de_trouve(sortie, "essai-un", "essai-deux")

    def test_une_ligne_de_plusieurs_mots_separee_par_d_autres_blancs_rougit(self):
        code, sortie = jouer(bloc("src/a.rs", "essai-un  essai-deux"), liste=PLUSIEURS)
        self.assertEqual(code, 1, sortie)
        self.assertIn("Line 1 of the guard list appears in the added lines of src/a.rs.", sortie)
        self.sans_rien_de_trouve(sortie, "essai-un", "essai-deux")

    def test_une_ligne_de_plusieurs_mots_ecrite_telle_quelle_rougit(self):
        code, sortie = jouer(bloc("src/a.rs", "essai-un essai-deux"), liste=PLUSIEURS)
        self.assertEqual(code, 1, sortie)
        self.assertIn("Line 1 of the guard list appears in the added lines of src/a.rs.", sortie)
        self.sans_rien_de_trouve(sortie, "essai-un", "essai-deux")

    def test_une_partie_d_une_ligne_de_plusieurs_mots_passe(self):
        code, sortie = jouer(bloc("src/a.rs", "essai-un"), liste=PLUSIEURS)
        self.assertEqual(code, 0, sortie)

    def test_un_mot_seul_reste_compare_a_la_lettre(self):
        code, sortie = jouer(bloc("src/a.rs", "essai un"), liste="essai-un\n")
        self.assertEqual(code, 0, sortie)

    def test_une_ligne_sans_lettres_ne_trouve_pas_tout(self):
        # Repliée, « !! ?? » serait vide, et tout texte la contiendrait : elle ne se compare qu'à la lettre.
        code, sortie = jouer(bloc("src/a.rs", "fn a() {}"), liste="!! ??\n")
        self.assertEqual(code, 0, sortie)


if __name__ == "__main__":
    unittest.main()
