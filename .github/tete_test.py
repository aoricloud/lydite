"""Les essais du contrôle de fusion (`.github/tete.py`), joués par la CI : les deux formes de la ligne qui valent,
celles qui ne valent rien, l'arbre exact, les corrections fusionnées dans la branche et dans les leurs, et les PR d'une
autre branche. L'API est fausse : rien ne sort sur le réseau.
"""

import importlib.util
import os
import subprocess
import sys
import unittest
from pathlib import Path

ICI = Path(__file__).resolve().parent
# Sous `python3 -I`, le dossier du script n'est pas dans le chemin des modules : on charge tete.py par son chemin.
_SPEC = importlib.util.spec_from_file_location("tete", ICI / "tete.py")
tete = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(tete)

TETE, RELUE, AUTRE, CORRIGEE, PLUS_LOIN = "a" * 40, "b" * 40, "c" * 40, "d" * 40, "e" * 40
ARBRE, AUTRE_ARBRE = "1" * 40, "2" * 40
BRANCHE = "lydite/vague-1"
DEPOT = "proprietaire/lydite"


def relu(sha):
    return f"Reviewed and replayed on commit {sha}."


def corrige(relue, sha, rejoue=None):
    return (
        f"Reviewed on commit {relue}, corrected in {sha} without a second review,"
        f" and replayed on commit {rejoue or sha}."
    )


class FausseApi:
    """Une API d'essai : la PR 1, l'arbre de chaque commit, et les PR fermées dont la base est chaque branche."""

    def __init__(self, corps, arbres, fermees=None, branche=BRANCHE, depot_tete=DEPOT):
        self.depot = DEPOT
        pr = {"branche": branche, "depot": depot_tete, "meme": depot_tete == DEPOT, "tete": TETE, "corps": corps}
        self.prs = {"1": pr}
        self.arbres = arbres
        self.par_base = fermees or {}

    def pr(self, numero):
        return self.prs[numero]

    def arbre(self, sha):
        return self.arbres[sha]

    def fermees(self, branche):
        return self.par_base.get(branche, [])


def fermee(numero, branche, corps, fusionnee=True, meme=True):
    return {"numero": numero, "branche": branche, "meme": meme, "fusionnee": fusionnee, "corps": corps}


class Controle(unittest.TestCase):
    def passe(self, api):
        passe, message = tete.controler(api, "1")
        self.assertTrue(passe, message)
        return message

    def rougit(self, api):
        passe, message = tete.controler(api, "1")
        self.assertFalse(passe, message)
        return message

    def test_la_tete_relue_et_rejouee_passe(self):
        self.passe(FausseApi(f"Reçu\n\n{relu(TETE)}\n", {TETE: ARBRE}))

    def test_un_autre_commit_du_meme_arbre_passe(self):
        self.passe(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: ARBRE}))

    def test_un_arbre_qui_differe_rougit(self):
        self.rougit(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE}))

    def test_la_forme_d_une_correction_dans_la_vague_passe(self):
        self.passe(FausseApi(corrige(RELUE, TETE), {TETE: ARBRE, RELUE: AUTRE_ARBRE}))

    def test_une_correction_rejouee_sur_un_autre_commit_ne_vaut_rien(self):
        self.rougit(FausseApi(corrige(RELUE, TETE, rejoue=AUTRE), {TETE: ARBRE, RELUE: ARBRE, AUTRE: ARBRE}))

    def test_les_formes_sans_rejeu_ou_sans_relecture_ne_valent_rien(self):
        for corps in (
            f"Reviewed and not replayed on commit {TETE}.",
            f"Replayed on commit {TETE}, not reviewed: no critical task was assembled.",
            corrige(RELUE, TETE).replace("and replayed", "and not replayed"),
        ):
            with self.subTest(corps=corps):
                self.rougit(FausseApi(corps, {TETE: ARBRE, RELUE: AUTRE_ARBRE}))

    def test_un_corps_sans_ligne_rougit_et_le_dit(self):
        message = self.rougit(FausseApi("Reçu\n", {TETE: ARBRE}))
        self.assertIn("names no commit", message)

    def test_une_ligne_dans_un_texte_ne_vaut_rien(self):
        self.rougit(FausseApi(f"See: {relu(TETE)}", {TETE: ARBRE}))

    def test_les_blancs_des_bords_et_les_fins_de_ligne_windows_passent(self):
        self.passe(FausseApi(f"Reçu\r\n  {relu(TETE)}  \r\n", {TETE: ARBRE}))

    def test_une_correction_fusionnee_dans_la_branche_passe(self):
        fermees = {BRANCHE: [fermee(2, "lydite/vague-2", relu(CORRIGEE))]}
        message = self.passe(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE, CORRIGEE: ARBRE}, fermees))
        self.assertIn("pull request 2", message)

    def test_la_correction_d_une_correction_passe(self):
        fermees = {
            BRANCHE: [fermee(2, "lydite/vague-2", relu(CORRIGEE))],
            "lydite/vague-2": [fermee(3, "lydite/vague-3", relu(PLUS_LOIN))],
        }
        arbres = {TETE: ARBRE, RELUE: AUTRE_ARBRE, CORRIGEE: AUTRE_ARBRE, PLUS_LOIN: ARBRE}
        self.assertIn("pull request 3", self.passe(FausseApi(relu(RELUE), arbres, fermees)))

    def test_une_correction_fermee_sans_fusion_ne_compte_pas(self):
        fermees = {BRANCHE: [fermee(2, "lydite/vague-2", relu(CORRIGEE), fusionnee=False)]}
        self.rougit(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE, CORRIGEE: ARBRE}, fermees))

    def test_une_correction_d_un_autre_arbre_ne_compte_pas(self):
        fermees = {BRANCHE: [fermee(2, "lydite/vague-2", relu(CORRIGEE))]}
        self.rougit(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE, CORRIGEE: AUTRE_ARBRE}, fermees))

    def test_des_corrections_qui_se_citent_en_boucle_finissent(self):
        fermees = {
            BRANCHE: [fermee(2, "lydite/vague-2", "")],
            "lydite/vague-2": [fermee(3, BRANCHE, "")],
        }
        self.rougit(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE}, fermees))

    def test_une_pr_d_une_autre_branche_passe_sans_controle(self):
        message = self.passe(FausseApi("", {}, branche="tete-relue-ci"))
        self.assertIn("Not a Lydite pull request", message)

    def test_une_branche_lydite_venue_d_un_fork_rougit_meme_avec_un_corps_juste(self):
        for depot in ("quelquun/lydite", ""):
            with self.subTest(depot=depot):
                message = self.rougit(FausseApi(relu(TETE), {TETE: ARBRE}, depot_tete=depot))
                self.assertIn("from a fork is not Lydite's", message)

    def test_une_branche_d_un_fork_sans_le_prefixe_passe_sans_controle(self):
        self.passe(FausseApi("", {}, branche="usage-en-anglais", depot_tete="quelquun/lydite"))

    def test_une_correction_venue_d_un_fork_ne_compte_pas(self):
        fermees = {BRANCHE: [fermee(2, "lydite/vague-2", relu(CORRIGEE), meme=False)]}
        self.rougit(FausseApi(relu(RELUE), {TETE: ARBRE, RELUE: AUTRE_ARBRE, CORRIGEE: ARBRE}, fermees))


class Lancement(unittest.TestCase):
    def test_sans_son_environnement_le_controle_rougit(self):
        env = {cle: valeur for cle, valeur in os.environ.items() if cle not in ("DEPOT", "NUMERO")}
        fait = subprocess.run(
            [sys.executable, "-I", str(ICI / "tete.py")], capture_output=True, text=True, env=env, check=False
        )
        self.assertEqual(fait.returncode, 1)
        self.assertIn("could not check the head", fait.stderr)


if __name__ == "__main__":
    unittest.main()
