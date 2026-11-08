"""Charge le plugin comme Hermes le fait (par son chemin), et fournit un faux contexte `ctx`.

Le dossier du plugin s'appelle `hermes`, comme le paquet de Hermes : on ne l'importe donc jamais par son nom.
"""

import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path

DOSSIER = Path(__file__).resolve().parents[1]
NOM_DU_MODULE = "plugin_sous_test"
LINUX = sys.platform.startswith("linux")


def plugin():
    """Le paquet du plugin, chargé une fois."""
    if NOM_DU_MODULE not in sys.modules:
        spec = importlib.util.spec_from_file_location(
            NOM_DU_MODULE, DOSSIER / "__init__.py", submodule_search_locations=[str(DOSSIER)]
        )
        module = importlib.util.module_from_spec(spec)
        sys.modules[NOM_DU_MODULE] = module
        spec.loader.exec_module(module)
    return sys.modules[NOM_DU_MODULE]


def sous_module(nom):
    plugin()
    return sys.modules[f"{NOM_DU_MODULE}.{nom}"]


class FauxEtat:
    """L'état durable d'un plugin : des valeurs JSON, comme `ctx.state`, et son dossier de données."""

    def __init__(self, dossier):
        self.data_dir = Path(dossier)
        self._valeurs = {}

    def get(self, cle, defaut=None):
        return json.loads(json.dumps(self._valeurs.get(cle, defaut)))

    def set(self, cle, valeur):
        self._valeurs[cle] = json.loads(json.dumps(valeur))


class FauxContexte:
    """Ce que `register` reçoit : il note ce qu'on enregistre, et rend les réglages qu'on lui donne."""

    def __init__(self, dossier, reglages=None):
        self.reglages = dict(reglages or {})
        self.outils = {}
        self.crochets = {}
        self.skills = {}
        self.state = FauxEtat(dossier)

    def register_tool(self, name, toolset, schema, handler, **options):
        self.outils[name] = {"toolset": toolset, "schema": schema, "handler": handler, **options}

    def register_skill(self, name, path, description="", frontmatter=None):
        self.skills[name] = {"path": Path(path), "description": description, "frontmatter": dict(frontmatter or {})}

    def register_hook(self, nom, rappel):
        self.crochets[nom] = rappel

    def get_config(self, cle, defaut=None):
        return self.reglages.get(cle, defaut)


class AvecDossier(unittest.TestCase):
    """Un dossier jetable par test."""

    def setUp(self):
        self._temporaire = tempfile.TemporaryDirectory()
        self.addCleanup(self._temporaire.cleanup)
        self.dossier = Path(self._temporaire.name)
