"""Les essais de la garde (`.github/garde.py`), joués par la CI : chaque lieu, la liste et ses lignes, les motifs, les
forks, les noms que git cite ou supprime, et qu'aucun mot ni rien de ce qu'un motif trouve ne paraisse dans la sortie.

Les valeurs qui tomberaient sous un motif sont écrites en morceaux, pour que la garde ne les trouve pas dans ce fichier
quand il change ; les mots de la liste d'essai sont inventés.
"""

import base64
import hashlib
import json
import os
import shutil
import string
import subprocess
import sys
import tempfile
import unittest
import zlib
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

# Un contenu binaire d'essai, et le diff net qui l'ajoute à docs/a.png : le diff net ne porte jamais son contenu.
IMAGE = bytes(range(256)) * 3
NET_IMAGE = (
    "diff --git a/docs/a.png b/docs/a.png\nnew file mode 100644\nindex 0000000..1111111\n"
    "Binary files /dev/null and b/docs/a.png differ\n"
)

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


def blob(contenu):
    """L'id de blob git de `contenu`."""
    return hashlib.sha1(b"blob %d\0" % len(contenu) + contenu).hexdigest()


def approuve(chemin, contenu):
    """La ligne de la liste des binaires qui approuve `contenu` au `chemin`."""
    return f"{hashlib.sha256(contenu).hexdigest()}  {chemin}\n"


def binaire(chemin, contenu, sorte="literal"):
    """Le diff d'un commit qui ajoute le binaire `chemin`, comme l'API le rend (`.patch`) : l'id de blob entier, puis un
    `GIT binary patch` dont la première partie (`sorte`) porte le contenu compressé, en base 85, 52 octets par ligne."""
    compresse = zlib.compress(contenu)
    lignes = []
    for debut in range(0, len(compresse), 52):
        morceau = compresse[debut : debut + 52]
        lettre = (string.ascii_uppercase + string.ascii_lowercase)[len(morceau) - 1]
        lignes.append(lettre + base64.b85encode(morceau, pad=True).decode("ascii"))
    donnees = "\n".join(lignes)
    return (
        f"diff --git a/{chemin} b/{chemin}\nnew file mode 100644\nindex {'0' * 40}..{blob(contenu)}\n"
        f"GIT binary patch\n{sorte} {len(contenu)}\n{donnees}\n\nliteral 0\nHcmV?d00001\n\n"
    )


def jouer(diff="", commits=(PROPRE,), erreur=False, liste=LISTE, pr=None, approuves=None, fichiers=None, **variables):
    """Joue la garde sur ces entrées (`pr` : le titre, le corps et la branche, comme l'API les écrit ; `fichiers` :
    chemin → id de blob de la version finale) : son code de sortie et tout ce qu'elle a écrit. Avec `approuves`, elle
    joue une copie de la garde à côté de cette liste des binaires ; sans, la garde du dépôt et sa liste."""
    with tempfile.TemporaryDirectory() as dossier:
        Path(dossier, "diff").write_text(diff, encoding="utf-8")
        Path(dossier, "commits.jsonl").write_text("".join(json.dumps(c) + "\n" for c in commits), encoding="utf-8")
        if pr is not None:
            Path(dossier, "pr.json").write_text(json.dumps(pr), encoding="utf-8")
        if erreur:
            Path(dossier, "erreur").write_text("diff\n", encoding="utf-8")
        if fichiers is not None:
            lignes = "".join(json.dumps({"nom": nom, "sha": sha}) + "\n" for nom, sha in fichiers.items())
            Path(dossier, "fichiers.jsonl").write_text(lignes, encoding="utf-8")
        garde = GARDE
        if approuves is not None:
            base = Path(dossier, "base")
            base.mkdir()
            garde = Path(shutil.copy(GARDE, base))
            Path(base, "binaires.txt").write_text(approuves, encoding="utf-8")
        ecartees = ("GARDE", "LISTE", "TITRE", "CORPS", "BRANCHE")
        environ = {k: v for k, v in os.environ.items() if k not in ecartees}
        environ.update({"GARDE": liste, **variables})
        fini = subprocess.run(
            [sys.executable, "-I", str(garde), dossier], check=False, env=environ, capture_output=True, encoding="utf-8"
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

    def test_l_adresse_d_un_auteur_ne_passe_que_par_la_liste(self):
        # L'adresse qu'un contributeur publie avec son commit passe, sur un fork comme ailleurs.
        exterieur = {"name": "someone", "email": COURRIEL}
        commit = dict(PROPRE, sha="d" * 40, a=exterieur, c=exterieur)
        for variables in ({}, {"LISTE": "fork"}):
            with self.subTest(**variables):
                code, sortie = jouer(commits=(commit,), **variables)
                self.assertEqual(code, 0, sortie)
                self.assertNotIn("::error", sortie)
        # La même adresse ailleurs que dans l'auteur et le committer reste une faute.
        message = dict(commit, m=f"Un message\n\n{COURRIEL}")
        code, sortie = jouer(commits=(message,), pr={"corps": COURRIEL}, LISTE="fork")
        self.assertEqual(code, 1)
        self.assertIn("An e-mail address appears in the message of commit ddddddd.", sortie)
        self.assertIn("An e-mail address appears in the body.", sortie)
        self.assertNotIn("author or committer", sortie)
        # Les autres motifs jugent encore l'auteur : seul celui des adresses électroniques s'en retire.
        reseau = dict(commit, a={"name": "x", "email": "x@" + "nas.l" + "an"})
        code, sortie = jouer(commits=(reseau,), LISTE="fork")
        self.assertEqual(code, 1)
        self.assertIn("A private network name appears in the author or committer of commit ddddddd.", sortie)
        # Une adresse de la liste dans l'auteur reste une faute.
        liste = dict(commit, a={"name": "x", "email": f"{MOT.lower()}@" + "mailbox.org"})
        code, sortie = jouer(commits=(liste,))
        self.assertEqual(code, 1)
        self.assertIn("Line 4 of the guard list appears in the author or committer of commit ddddddd.", sortie)
        self.assertNotIn("An e-mail address", sortie)
        self.sans_rien_de_trouve(sortie, "mailbox")

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

    def test_un_message_de_commit_ne_retire_pas_un_binaire(self):
        # Le bloc d'un binaire n'a pas de `@@` : la ligne « deleted file mode » d'un message de commit qui le suit dans le
        # `.patch` ne le dit pas supprimé, ni dans le diff net (suivi du premier commit), ni dans un commit.
        def commit(message):
            return f"From {'a' * 40} Mon Sep 17 00:00:00 2001\nFrom: x <x@exemple.invalid>\nSubject: [PATCH] x\n\n{message}\n---\n"

        message = "deleted file mode 100644"
        diff = NET_IMAGE + commit(message) + binaire("docs/a.png", IMAGE) + commit(message)
        code, sortie = jouer(diff, approuves="", fichiers={"docs/a.png": blob(IMAGE)})
        self.assertEqual(code, 1, sortie)
        self.assertIn("A binary file is changed: docs/a.png; its content cannot be checked.", sortie)

    def test_un_binaire_approuve_passe(self):
        # Listé dans la liste de la base, sa seule version de cette empreinte, et sa version finale celle-là.
        diff = NET_IMAGE + binaire("docs/a.png", IMAGE)
        code, sortie = jouer(diff, approuves=approuve("docs/a.png", IMAGE), fichiers={"docs/a.png": blob(IMAGE)})
        self.assertEqual(code, 0, sortie)

    def test_un_binaire_approuve_au_contenu_change_rougit(self):
        autre = IMAGE + b"\0"
        code, sortie = jouer(
            NET_IMAGE + binaire("docs/a.png", autre),
            approuves=approuve("docs/a.png", IMAGE),
            fichiers={"docs/a.png": blob(autre)},
        )
        self.assertEqual(code, 1)
        self.assertIn("A binary file is changed: docs/a.png; its content is not the approved one.", sortie)

    def test_une_version_de_l_histoire_non_approuvee_rougit(self):
        # Un commit publie une autre version, qu'un second remplace par l'approuvée : la première reste dans l'histoire.
        diff = NET_IMAGE + binaire("docs/a.png", IMAGE + b"\0") + binaire("docs/a.png", IMAGE)
        code, sortie = jouer(diff, approuves=approuve("docs/a.png", IMAGE), fichiers={"docs/a.png": blob(IMAGE)})
        self.assertEqual(code, 1)
        self.assertIn("A binary file is changed: docs/a.png; its content is not the approved one.", sortie)

    def test_la_liste_de_la_tete_n_approuve_rien(self):
        # La PR ajoute à sa propre liste la ligne qui approuverait son binaire : seule la liste de la base compte.
        diff = bloc(".github/binaires.txt", approuve("docs/a.png", IMAGE).strip()) + binaire("docs/a.png", IMAGE)
        code, sortie = jouer(diff, approuves="", fichiers={"docs/a.png": blob(IMAGE)})
        self.assertEqual(code, 1)
        self.assertIn("A binary file is changed: docs/a.png; its content cannot be checked.", sortie)

    def test_une_ligne_mal_ecrite_rougit_et_n_approuve_rien(self):
        juste = approuve("docs/a.png", IMAGE)
        empreinte = juste.split()[0]
        for fausse in ("pas une empreinte  docs/b.png", f"{empreinte} docs/b.png", f"{empreinte}  ", juste.strip()):
            with self.subTest(fausse=fausse):
                code, sortie = jouer(
                    binaire("docs/a.png", IMAGE), approuves=juste + fausse + "\n", fichiers={"docs/a.png": blob(IMAGE)}
                )
                self.assertEqual(code, 1)
                self.assertIn("Line 2 of .github/binaires.txt is not `<sha256>  <path>`", sortie)
                self.assertIn("A binary file is changed: docs/a.png; its content cannot be checked.", sortie)

    def test_une_version_qui_ne_se_lit_pas_rougit(self):
        # Le diff net seul (la version d'un commit de fusion), une partie delta, des données qui ne se décodent pas, une
        # version finale qu'aucun commit ne montre, ou aucune version finale lue.
        casse = binaire("docs/a.png", IMAGE).replace(f"literal {len(IMAGE)}\n", f"literal {len(IMAGE)}\n?", 1)
        cas = (
            (NET_IMAGE, {"docs/a.png": blob(IMAGE)}),
            (binaire("docs/a.png", IMAGE, sorte="delta"), {"docs/a.png": blob(IMAGE)}),
            (casse, {"docs/a.png": blob(IMAGE)}),
            (binaire("docs/a.png", IMAGE), {"docs/a.png": blob(IMAGE + b"\0")}),
            (binaire("docs/a.png", IMAGE), None),
        )
        for diff, fichiers in cas:
            with self.subTest(diff=diff[:200], fichiers=fichiers):
                code, sortie = jouer(diff, approuves=approuve("docs/a.png", IMAGE), fichiers=fichiers)
                self.assertEqual(code, 1)
                self.assertIn(
                    "A binary file is changed: docs/a.png; its content could not be read from the change.", sortie
                )

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
        fichiers = [GARDE, ICI / "garde_test.py", ICI / "binaires.txt", ICI / "workflows" / "garde.yml"]
        fichiers.append(ICI / "workflows" / "ci.yml")
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
