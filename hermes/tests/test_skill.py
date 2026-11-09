"""La skill de pilotage suit le format ouvert `SKILL.md` avec ses seuls champs communs (de quoi la lire aussi dans
Claude Code), le plugin l'enregistre, et elle dit ce que font les outils."""

import re
import tempfile
import unittest

from .chargeur import DOSSIER, FauxContexte, plugin, sous_module

FICHIER = DOSSIER / "skills" / "driving-lydite" / "SKILL.md"
# Les champs du format ouvert : le nom et la description (obligatoires), puis ceux que le format nomme en option.
CHAMPS_COMMUNS = {"name", "description", "license", "compatibility", "metadata", "allowed-tools"}
# Ce que Hermes signale, dans un texte de skill, comme une possible injection de consignes.
MOTIFS_SUSPECTS = (
    "ignore previous instructions",
    "ignore all previous",
    "you are now",
    "disregard your",
    "forget your instructions",
    "new instructions:",
    "system prompt:",
    "<system>",
    "]]>",
)


def lire():
    """(champs de tête, corps) de la skill : les champs sont des lignes `clé: valeur`, sans bibliothèque."""
    texte = FICHIER.read_text(encoding="utf-8")
    tete, corps = re.fullmatch(r"---\n(.*?)\n---\n(.*)", texte, re.DOTALL).groups()
    champs = {}
    for ligne in tete.splitlines():
        trouve = re.fullmatch(r"([a-z-]+): (.+)", ligne)
        assert trouve, f"une ligne de tête n'est pas `clé: valeur` sur une seule ligne : {ligne!r}"
        champs[trouve.group(1)] = trouve.group(2)
    return champs, corps


class Skill(unittest.TestCase):
    def test_la_tete_n_a_que_les_champs_communs_du_format_ouvert(self):
        champs, _ = lire()
        self.assertLessEqual(set(champs), CHAMPS_COMMUNS)
        self.assertLessEqual({"name", "description"}, set(champs))

    def test_le_nom_est_celui_du_dossier_et_les_longueurs_sont_tenues(self):
        champs, _ = lire()
        self.assertEqual(champs["name"], FICHIER.parent.name)
        self.assertRegex(champs["name"], r"[a-z0-9]+(-[a-z0-9]+)*")
        self.assertLessEqual(len(champs["name"]), 64)
        self.assertLessEqual(len(champs["description"]), 1024)
        # Hermes ne met dans l'index du modèle que les 60 premiers caractères de la description : la première
        # phrase doit dire à elle seule quand l'employer.
        self.assertLessEqual(len(champs["description"].split(". ")[0]) + 1, 60)
        self.assertLessEqual(len(champs.get("compatibility", "")), 500)

    def test_le_plugin_l_enregistre_avec_la_description_de_son_fichier(self):
        with tempfile.TemporaryDirectory() as dossier:
            ctx = FauxContexte(dossier)
            plugin().register(ctx)
        champs, _ = lire()
        self.assertEqual(list(ctx.skills), [champs["name"]])
        inscrite = ctx.skills[champs["name"]]
        self.assertEqual(inscrite["path"].resolve(), FICHIER.resolve())
        self.assertEqual(inscrite["description"], champs["description"])
        self.assertEqual(inscrite["frontmatter"], {"platforms": ["linux"]})

    def test_elle_nomme_les_outils_et_les_commandes_qu_elle_decrit(self):
        nom = sous_module("nom")
        _, corps = lire()
        for attendu in (nom.LANCER, nom.ETAT, nom.LECONS, "lydite lancer", "lydite etat", "lydite lecons",
                        "lydite accepter"):
            self.assertIn(attendu, corps)

    def test_elle_ne_porte_aucun_motif_que_hermes_tient_pour_suspect(self):
        champs, corps = lire()
        texte = (champs["description"] + "\n" + corps).lower()
        for motif in MOTIFS_SUSPECTS:
            self.assertNotIn(motif, texte)
        # Hermes remplace `${...}` et les commandes en ligne d'une skill avant de la rendre : rien de tel ici.
        self.assertNotIn("${", corps)
        self.assertNotIn("!`", corps)


if __name__ == "__main__":
    unittest.main()
