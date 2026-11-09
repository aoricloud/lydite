"""Le manifeste dit ce que le code enregistre (règle 6 du catalogue), et le dépôt ne porte ni binaire ni secret lu
à la main (règles 11 et 17)."""

import re
import subprocess
import tempfile
import unittest

from .chargeur import DOSSIER, FauxContexte, plugin, sous_module


def manifeste():
    """`plugin.yaml` lu sans bibliothèque : ses champs de tête, ses listes, ses `env`."""
    texte = (DOSSIER / "plugin.yaml").read_text(encoding="utf-8")
    champs = dict(re.findall(r"^([a-z_]+): *(.+)$", texte, re.MULTILINE))
    listes = {}
    for cle, bloc in re.findall(r"^([a-z_]+):\n((?:  - .+\n?)+)", texte, re.MULTILINE):
        listes[cle] = re.findall(r"^  - (.+)$", bloc, re.MULTILINE)
    return {"champs": champs, "listes": listes, "texte": texte}


class Manifeste(unittest.TestCase):
    def test_le_nom_et_le_prefixe_sont_ceux_du_manifeste(self):
        nom = sous_module("nom")
        lu = manifeste()
        self.assertEqual(lu["champs"]["name"], nom.NOM)
        self.assertEqual(lu["listes"]["provides_tools"], [nom.LANCER, nom.ETAT, nom.LECONS])
        self.assertIn(f"env: {nom.SECRET_ENV}", lu["texte"])

    def test_les_capacites_declarees_sont_les_enregistrees(self):
        with tempfile.TemporaryDirectory() as dossier:
            ctx = FauxContexte(dossier)
            plugin().register(ctx)
        lu = manifeste()
        self.assertEqual(sorted(ctx.outils), sorted(lu["listes"]["provides_tools"]))
        self.assertEqual(sorted(ctx.crochets), sorted(lu["listes"]["provides_hooks"]))

    def test_chaque_schema_porte_le_nom_de_son_outil(self):
        with tempfile.TemporaryDirectory() as dossier:
            ctx = FauxContexte(dossier)
            plugin().register(ctx)
        for nom, outil in ctx.outils.items():
            self.assertEqual(outil["schema"]["name"], nom)
            self.assertTrue(outil["schema"]["description"])
            self.assertEqual(outil["schema"]["parameters"]["type"], "object")

    def test_register_ne_lit_ni_etat_ni_reglage(self):
        """Le contexte de `hermes plugins validate` n'a pas d'état : `register` ne doit pas en avoir besoin."""

        class Nu:
            def __init__(self):
                self.outils = []

            def register_tool(self, name, *args, **options):
                self.outils.append(name)

            def register_hook(self, nom, rappel):
                self.outils.append(nom)

            def register_skill(self, name, path, description="", frontmatter=None):
                pass

        nu = Nu()
        plugin().register(nu)
        self.assertEqual(len(nu.outils), 4)

    def test_les_reglages_lus_par_le_code_sont_ceux_du_manifeste(self):
        code = (DOSSIER / "outils.py").read_text(encoding="utf-8")
        lus = set(re.findall(r'_reglage\("([a-z_]+)"', code))
        declares = set(re.findall(r"^  ([a-z_]+):\n    type:", manifeste()["texte"], re.MULTILINE))
        self.assertTrue(lus)
        self.assertLessEqual(lus, declares)


class Regles(unittest.TestCase):
    def test_aucun_binaire_ni_pyc_dans_le_plugin(self):
        magiques = (b"\x7fELF", b"MZ", b"\xca\xfe\xba\xbe", b"\xcf\xfa\xed\xfe", b"\xfe\xed\xfa\xcf")
        # Ce que git suit, et non ce qui traîne sur le disque : un test joué laisse des `.pyc`, que .gitignore écarte.
        try:
            sortie = subprocess.run(["git", "ls-files", "-z", "--", "."], cwd=DOSSIER, capture_output=True, check=True)
        except (OSError, subprocess.CalledProcessError):
            self.skipTest("pas de git ici, ou ce dossier n'est pas dans un dépôt git")
        suivis = [DOSSIER / nom for nom in sortie.stdout.decode("utf-8").split(chr(0)) if nom]
        self.assertTrue(suivis)
        for chemin in suivis:
            self.assertNotIn("__pycache__", chemin.parts, chemin)
            self.assertNotEqual(chemin.suffix, ".pyc", chemin)
            self.assertFalse(chemin.read_bytes().startswith(magiques), f"{chemin} ressemble à un binaire")

    def test_gitignore_garde_dehors_les_fichiers_compiles(self):
        lignes = (DOSSIER.parent / ".gitignore").read_text(encoding="utf-8").splitlines()
        self.assertIn("__pycache__/", lignes)
        self.assertIn("*.pyc", lignes)

    def test_le_code_ne_lit_pas_de_fichier_de_cle_et_n_appelle_pas_de_shell(self):
        interdits = (
            r"\.env\b",
            r"auth\.json",
            r"shell\s*=\s*True",
            r"os\.system",
            r"os\.popen",
            r"os\.getenv",
            r"os\.environ\[",
        )
        for chemin in DOSSIER.glob("*.py"):
            code = chemin.read_text(encoding="utf-8")
            for motif in interdits:
                self.assertIsNone(re.search(motif, code), f"{chemin.name} contient {motif!r}")

    def test_les_lignes_font_120_caracteres_au_plus(self):
        for chemin in [*DOSSIER.glob("*.py"), *DOSSIER.glob("tests/*.py")]:
            for rang, ligne in enumerate(chemin.read_text(encoding="utf-8").splitlines(), 1):
                self.assertLessEqual(len(ligne), 120, f"{chemin.name}:{rang}")


if __name__ == "__main__":
    unittest.main()
