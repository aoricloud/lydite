"""Les outils : la demande vérifiée, le verrou d'approbation, le lancement détaché (avec un faux Lydite, sous
Linux), l'environnement fermé et la clé du fournisseur."""

import hashlib
import json
import os
import signal
import sys
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

from .chargeur import LINUX, AvecDossier, FauxContexte, plugin, sous_module

PREUVE = "cargo test && cargo clippy --all-targets -- -D warnings"
SECRET = "sk-le-secret-du-fournisseur"
PLAN = (
    "plan 1\nobjectif : Un LISEZ-MOI pour apercus/\npreuve : cargo test\n\ntache lisezmoi\nmodele : deepseek-flash\n"
    "relance : deepseek-flash\nfichiers : apercus/LISEZ-MOI.md\ncritique : oui\n    Écris apercus/LISEZ-MOI.md.\n"
)


def faux_secrets(valeur=SECRET):
    """Le module `agent.secret_scope` de Hermes, réduit à `get_secret`."""
    paquet = mock.MagicMock()
    paquet.secret_scope.get_secret = lambda nom, defaut=None: valeur if nom == sous_module("nom").SECRET_ENV else defaut
    return mock.patch.dict(sys.modules, {"agent": paquet, "agent.secret_scope": paquet.secret_scope})


class AvecOutils(AvecDossier):
    def setUp(self):
        super().setUp()
        self.sortie = self.dossier / "sortie"
        self.sortie.mkdir()
        self.depot = self.dossier / "depot"
        (self.depot / ".git").mkdir(parents=True)
        self.environ = {
            "PATH": "/usr/bin:/bin",
            "HOME": str(self.dossier),
            "LANG": "C.UTF-8",
            "LYDITE_DOSSIER": str(self.dossier / "dossier-lydite"),
            "OPENAI_API_KEY": "sk-une-autre-cle",
            "HERMES_YOLO_MODE": "1",
        }
        self.reglages = {}
        self.addCleanup(self.tuer)

    def outils(self, attente=0.3, **reglages):
        self.reglages.update(reglages)
        self.ctx = FauxContexte(self.dossier / "donnees", self.reglages)
        return sous_module("outils").Outils(self.ctx, environ=self.environ, attente=attente)

    def demande(self, **champs):
        return {"depot": str(self.depot), "issue": 15, "preuve": PREUVE, **champs}

    def demande_de_plan(self, texte=PLAN, **champs):
        """Une demande par plan : le fichier est écrit dans le dossier d'essai, sans issue ni preuve."""
        chemin = self.dossier / "plan.txt"
        chemin.write_text(texte, encoding="utf-8", newline="\n")
        return self.demande(issue=None, preuve=None, plan=str(chemin), **champs)

    def faux_lydite(self, suite="", code_etat=0, texte_etat="aucune vague", code_lecons=0, texte_lecons=""):
        """Un faux Lydite : il note ses arguments et son environnement, répond à `etat` et à `lecons`, puis fait
        `suite`."""
        dossier = self.dossier / "bin"
        dossier.mkdir(exist_ok=True)
        chemin = dossier / "lydite"
        chemin.write_text(
            f"#!/bin/sh\n"
            f"if [ \"$1\" = etat ]; then echo '{texte_etat}'; exit {code_etat}; fi\n"
            f"for a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{self.sortie}/argv'\n"
            f"env > '{self.sortie}/env'\n"
            f"if [ \"$1\" = lecons ]; then printf '%s' '{texte_lecons}' >&2; exit {code_lecons}; fi\n"
            f"{suite}\n",
            encoding="utf-8",
        )
        chemin.chmod(0o755)
        self.reglages["binaire"] = str(chemin)

    def lu(self, nom):
        deadline = time.time() + 5
        while not (self.sortie / nom).exists() and time.time() < deadline:
            time.sleep(0.05)
        return (self.sortie / nom).read_text(encoding="utf-8")

    def tuer(self):
        for pid, enfant in list(sous_module("outils")._PROCESSUS.items()):
            try:
                os.killpg(pid, signal.SIGKILL)
            except OSError:
                pass
            enfant.wait()
        sous_module("outils")._PROCESSUS.clear()


class Demande(AvecOutils):
    def refus(self, **champs):
        with self.assertRaises(sous_module("outils").Refus) as contexte:
            self.outils()._demande(self.demande(**champs))
        return str(contexte.exception)

    def test_une_issue_donne_la_demande_complete(self):
        demande = self.outils()._demande(self.demande())
        self.assertEqual(demande["issue"], 15)
        self.assertIsNone(demande["objectif"])
        self.assertEqual(demande["profil"], "claude")
        self.assertEqual(demande["pr"], "main")
        self.assertFalse(demande["observer"])

    def test_le_profil_par_defaut_est_le_reglage(self):
        demande = self.outils(profil="nautile")._demande(self.demande())
        self.assertEqual(demande["profil"], "nautile")

    def test_un_numero_d_issue_en_texte_est_accepte(self):
        self.assertEqual(self.outils()._demande(self.demande(issue="15"))["issue"], 15)

    def test_le_depot_doit_etre_un_depot_git_absolu(self):
        self.assertIn("required", self.refus(depot=""))
        self.assertIn("absolute", self.refus(depot="depot"))
        self.assertIn("not a git checkout", self.refus(depot=str(self.dossier)))

    def test_une_issue_ou_un_objectif_mais_pas_les_deux_ni_aucun(self):
        self.assertIn("exactly one", self.refus(objectif="faire x"))
        self.assertIn("exactly one", self.refus(issue=None))

    def test_une_issue_est_un_entier_positif(self):
        for mauvaise in (0, -3, True, "abc", 1.5):
            self.assertIn("positive", self.refus(issue=mauvaise), mauvaise)

    def test_un_objectif_tient_sur_une_ligne_et_ne_commence_pas_par_un_tiret(self):
        self.assertIn("one line", self.refus(issue=None, objectif="a\nb"))
        self.assertIn("one line", self.refus(issue=None, objectif="--n"))
        demande = self.outils()._demande(self.demande(issue=None, objectif="  ajouter x  "))
        self.assertEqual(demande["objectif"], "ajouter x")

    def test_la_preuve_est_obligatoire(self):
        self.assertIn("preuve", self.refus(preuve="   "))
        self.assertIn("preuve", self.refus(preuve=None))

    def test_une_preuve_ou_un_objectif_trop_longs_sont_refuses(self):
        outils = sous_module("outils")
        self.assertIn("script", self.refus(preuve="x" * (outils.LIMITE_DE_LA_PREUVE + 1)))
        self.assertIn("under", self.refus(issue=None, objectif="x" * (outils.LIMITE_DE_L_OBJECTIF + 1)))
        self.outils()._demande(self.demande(preuve="x" * outils.LIMITE_DE_LA_PREUVE))

    def test_un_plan_donne_la_demande_sans_issue_ni_preuve(self):
        demande = self.outils()._demande(self.demande_de_plan())
        self.assertEqual(demande["plan"], str(self.dossier / "plan.txt"))
        self.assertEqual(demande["plan_octets"], PLAN.encode("utf-8"))
        self.assertIsNone(demande["issue"])
        self.assertEqual(demande["preuve"], "")

    def test_un_plan_exclut_l_issue_l_objectif_la_preuve_et_observer(self):
        plan = self.demande_de_plan()["plan"]
        self.assertIn("exactly one", self.refus(plan=plan, issue=15, preuve=None))
        self.assertIn("exactly one", self.refus(plan=plan, issue=None, objectif="x", preuve=None))
        self.assertIn("own `preuve", self.refus(plan=plan, issue=None))
        self.assertIn("observer", self.refus(plan=plan, issue=None, preuve=None, observer=True))

    def test_un_plan_illisible_ou_trop_long_est_refuse(self):
        sans = {"issue": None, "preuve": None}
        self.assertIn("absolute", self.refus(plan="plan.txt", **sans))
        self.assertIn("reasonable size", self.refus(plan=str(self.dossier / "absent.txt"), **sans))
        chemin = self.dossier / "mauvais.txt"
        chemin.write_bytes(b"\xff\xfe plan")
        self.assertIn("UTF-8", self.refus(plan=str(chemin), **sans))
        chemin.write_text("   \n", encoding="utf-8")
        self.assertIn("between 1 and", self.refus(plan=str(chemin), **sans))
        chemin.write_text("x" * (sous_module("outils").LIMITE_DU_PLAN + 1), encoding="utf-8")
        self.assertIn("between 1 and", self.refus(plan=str(chemin), **sans))

    def test_un_profil_ou_une_branche_mal_formes_sont_refuses(self):
        self.assertIn("profile name", self.refus(profil="--decouper"))
        self.assertIn("profile name", self.refus(profil="a b"))
        self.assertIn("branch", self.refus(pr="-f"))
        self.assertIn("branch", self.refus(pr="a..b"))


class Verrou(AvecOutils):
    def compact(self, message):
        """Le message sans les pliures de l'approbation, pour y retrouver une valeur entière."""
        return "".join(message.replace("\n" + " " * 9, "").split())

    def bornes(self, message):
        """Ce que Hermes montre : neuf lignes de 58 caractères au plus."""
        lignes = message.splitlines()
        self.assertLessEqual(len(lignes), sous_module("outils").LIGNES_DE_L_APPROBATION, lignes)
        for ligne in lignes:
            self.assertLessEqual(len(ligne), sous_module("outils").LARGEUR_DE_L_APPROBATION, ligne)

    def depot_long(self, longueur):
        """Un dépôt git dont le chemin absolu fait exactement `longueur` caractères."""
        chemin = self.dossier / ("d" * (longueur - len(str(self.dossier)) - 1))
        (chemin / ".git").mkdir(parents=True)
        self.assertEqual(len(str(chemin)), longueur)
        return chemin

    def test_un_autre_outil_passe_sans_question(self):
        self.assertIsNone(self.outils().verrou(tool_name="terminal", args={"command": "ls"}))
        self.assertIsNone(self.outils().verrou(tool_name=sous_module("nom").ETAT, args={}))

    def test_un_lancement_demande_l_approbation_avec_ce_qui_va_partir(self):
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=self.demande(profil="nautile"))
        self.assertEqual(decision["action"], "approve")
        message = self.compact(decision["message"])
        for attendu in (str(self.depot), "issue #15", PREUVE, "nautile", "main"):
            self.assertIn(self.compact(attendu), message)

    def test_les_messages_d_approbation_tiennent_dans_les_bornes(self):
        cas = (
            self.demande(),
            self.demande(issue=None, objectif="ajouter x"),
            self.demande(issue=None, objectif="ajouter x", observer=True),
            self.demande_de_plan(),
        )
        for args in cas:
            decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=args)
            self.assertEqual(decision["action"], "approve", decision)
            self.bornes(decision["message"])

    def test_un_depot_de_150_caracteres_tient_dans_les_bornes(self):
        depot = str(self.depot_long(150))
        cas = (
            self.demande(depot=depot),
            self.demande(issue=None, objectif="ajouter x", depot=depot),
            self.demande(issue=None, objectif="ajouter x", depot=depot, observer=True),
            self.demande_de_plan(depot=depot),
        )
        for args in cas:
            decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=args)
            self.assertEqual(decision["action"], "approve", decision)
            self.bornes(decision["message"])

    def test_une_ligne_de_preuve_de_300_caracteres_tient_dans_les_bornes(self):
        plan = "plan 1\nobjectif : o\npreuve : " + "p" * 300 + "\n\ntache a\n    faire.\n"
        args = self.demande_de_plan(texte=plan)
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=args)
        self.assertEqual(decision["action"], "approve")
        self.bornes(decision["message"])

    def test_un_plan_est_montre_par_son_nom_ses_taches_et_son_empreinte(self):
        args = self.demande_de_plan()
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=args)
        self.assertEqual(decision["action"], "approve")
        message = decision["message"]
        lignes = message.splitlines()
        debut = next(rang for rang, ligne in enumerate(lignes) if ligne.startswith("sha256 : "))
        empreinte = lignes[debut][len("sha256 : ") :] + lignes[debut + 1].strip()
        self.assertEqual(empreinte, hashlib.sha256(PLAN.encode("utf-8")).hexdigest())
        self.assertIn(Path(args["plan"]).name, message)
        self.assertIn(f"{len(PLAN)} chars", message)
        self.assertIn("1 tasks", message)
        self.assertIn("cargo test", message)
        self.assertNotIn("Écris apercus/LISEZ-MOI.md.", message)

    def test_la_preuve_donnee_se_lit_en_entier_dans_les_bornes(self):
        preuve = "cargo test " + "a" * 80
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=self.demande(preuve=preuve))
        self.assertEqual(decision["action"], "approve")
        self.assertIn(self.compact(preuve), self.compact(decision["message"]))
        self.bornes(decision["message"])

    def test_111_caracteres_sont_refuses_et_110_acceptes(self):
        lancer = sous_module("nom").LANCER
        outils = self.outils()
        self.assertEqual(outils.verrou(tool_name=lancer, args=self.demande(preuve="p" * 110))["action"], "approve")
        self.assertEqual(outils.verrou(tool_name=lancer, args=self.demande(preuve="p" * 111))["action"], "block")
        self.assertEqual(
            outils.verrou(tool_name=lancer, args=self.demande(issue=None, objectif="o" * 110))["action"],
            "approve",
        )
        self.assertEqual(
            outils.verrou(tool_name=lancer, args=self.demande(issue=None, objectif="o" * 111))["action"],
            "block",
        )

    def test_la_preuve_et_l_objectif_de_110_caracteres_paraissent_en_entier(self):
        lancer = sous_module("nom").LANCER
        preuve = "cargo test " + "a" * 99
        objectif = "o" * 110
        self.assertEqual(len(preuve), 110)
        self.assertEqual(len(objectif), 110)
        for args in (
            self.demande(preuve=preuve),
            self.demande(issue=None, objectif=objectif),
            self.demande(issue=None, objectif=objectif, preuve=preuve),
        ):
            decision = self.outils().verrou(tool_name=lancer, args=args)
            self.assertEqual(decision["action"], "approve", decision)
            message = self.compact(decision["message"])
            self.assertIn(self.compact(args["preuve"]), message)
            if args.get("objectif"):
                self.assertIn(self.compact(f"objective: {objectif}"), message)
            self.bornes(decision["message"])

    def test_l_accord_toujours_d_un_plan_ne_vaut_que_pour_ce_texte(self):
        lancer = sous_module("nom").LANCER
        un = self.outils().verrou(tool_name=lancer, args=self.demande_de_plan())["rule_key"]
        meme = self.outils().verrou(tool_name=lancer, args=self.demande_de_plan())["rule_key"]
        autre = self.outils().verrou(tool_name=lancer, args=self.demande_de_plan(PLAN + "de plus\n"))["rule_key"]
        self.assertEqual(un, meme)
        self.assertNotEqual(un, autre)

    def test_l_accord_toujours_ne_vaut_que_pour_la_meme_preuve(self):
        lancer = sous_module("nom").LANCER

        def clef(**champs):
            return self.outils().verrou(tool_name=lancer, args=self.demande(**champs))["rule_key"]

        self.assertEqual(clef(), clef(issue=16, objectif=None))
        self.assertNotEqual(clef(), clef(preuve="true"))
        self.assertNotEqual(clef(), clef(profil="autre"))

    def test_une_demande_qui_sera_refusee_ne_derange_pas_l_humain(self):
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=self.demande(preuve=""))
        self.assertEqual(decision["action"], "block")
        self.assertIn("preuve", json.loads(decision["message"])["erreur"])

    def test_des_arguments_absents_ne_font_pas_planter_le_crochet(self):
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=None)
        self.assertEqual(decision["action"], "block")

    def test_le_crochet_est_celui_qui_est_enregistre(self):
        ctx = FauxContexte(self.dossier)
        plugin().register(ctx)
        decision = ctx.crochets["pre_tool_call"](tool_name=sous_module("nom").LANCER, args=self.demande(), task_id="t")
        self.assertEqual(decision["action"], "approve")


class Commande(AvecOutils):
    def commande(self, **champs):
        outils = self.outils()
        return outils._commande("/k/lydite", outils._demande(self.demande(**champs)))

    def test_une_issue(self):
        self.assertEqual(
            self.commande(profil="nautile"),
            [
                "/k/lydite",
                "lancer",
                "nautile",
                "--issue",
                "15",
                "--decouper",
                "--verifier",
                PREUVE,
                "--depot",
                str(self.depot),
                "--pr",
                "main",
            ],
        )

    def test_un_objectif_et_le_mode_observer(self):
        argv = self.commande(issue=None, objectif="ajouter x", observer=True, pr="dev")
        self.assertEqual(argv[:4], ["/k/lydite", "lancer", "claude", "ajouter x"])
        self.assertEqual(argv[-3:], ["--pr", "dev", "--observer"])

    def test_un_plan_se_lance_par_sa_copie_sans_verifier_ni_decouper(self):
        outils = self.outils()
        demande = outils._demande(self.demande_de_plan(profil="nautile"))
        demande["plan_copie"] = "/donnees/20261107.plan"
        self.assertEqual(
            outils._commande("/k/lydite", demande),
            [
                "/k/lydite",
                "lancer",
                "nautile",
                "--plan",
                "/donnees/20261107.plan",
                "--depot",
                str(self.depot),
                "--pr",
                "main",
            ],
        )

    def test_la_preuve_reste_un_seul_argument(self):
        argv = self.commande(preuve="true; touch /tmp/x && echo $(id)")
        self.assertEqual(argv[argv.index("--verifier") + 1], "true; touch /tmp/x && echo $(id)")


@unittest.skipUnless(LINUX, "Lydite's wall and the detached launch are Linux only")
class Lancement(AvecOutils):
    def lancer(self, outils=None, **champs):
        return json.loads((outils or self.outils()).lancer(self.demande(**champs)))

    def test_un_lancement_long_rend_la_main_avec_un_identifiant(self):
        self.faux_lydite("sleep 30")
        rendu = self.lancer()
        self.assertTrue(rendu["lance"], rendu)
        self.assertTrue(rendu["id"] and rendu["pid"])
        self.assertEqual(
            self.lu("argv").split("\n")[:5],
            ["lancer", "claude", "--issue", "15", "--decouper"],
        )
        self.assertTrue(os.path.exists(rendu["journal"]))

    def test_le_processus_est_detache_de_hermes(self):
        self.faux_lydite("sleep 30")
        rendu = self.lancer()
        self.assertEqual(os.getpgid(rendu["pid"]), rendu["pid"])
        self.assertNotEqual(os.getpgid(rendu["pid"]), os.getpgid(os.getpid()))

    def test_un_refus_de_lydite_se_rend_tout_de_suite_avec_son_texte(self):
        self.faux_lydite("echo 'refus 6022 : la base ne passe pas la preuve'; exit 2")
        rendu = self.lancer(self.outils(attente=5))
        self.assertFalse(rendu["lance"])
        self.assertEqual(rendu["sortie"], 2)
        self.assertIn("refused", rendu["signification"])
        self.assertIn("6022", rendu["fin_du_journal"])

    def test_deux_lancements_dans_le_meme_dossier_de_lydite_sont_refuses(self):
        self.faux_lydite("sleep 30")
        outils = self.outils()
        premier = self.lancer(outils)
        second = self.lancer(outils)
        self.assertIn("still running", second["erreur"])
        self.assertIn(premier["id"], second["erreur"])

    def test_un_lancement_fini_ne_bloque_plus(self):
        self.faux_lydite("exit 0")
        outils = self.outils(attente=5)
        self.assertFalse(self.lancer(outils)["lance"])
        self.assertNotIn("erreur", self.lancer(outils))

    def test_l_environnement_est_ferme(self):
        self.faux_lydite("sleep 30")
        self.lancer()
        env = self.lu("env")
        self.assertIn("LYDITE_DOSSIER=", env)
        self.assertIn("PATH=/usr/bin:/bin", env)
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("HERMES_YOLO_MODE", env)

    def test_la_cle_du_fournisseur_va_au_processus_et_nulle_part_ailleurs(self):
        self.faux_lydite("echo debut; sleep 30")
        outils = self.outils(cle_nom="CLE_DEEPSEEK")
        with faux_secrets():
            rendu = self.lancer(outils)
        self.assertTrue(rendu["lance"], rendu)
        self.assertIn(f"CLE_DEEPSEEK={SECRET}", self.lu("env"))
        for endroit in (json.dumps(rendu), json.dumps(self.ctx.state._valeurs), self.lu("argv")):
            self.assertNotIn(SECRET, endroit)
        with open(rendu["journal"], encoding="utf-8") as journal:
            self.assertNotIn(SECRET, journal.read())

    def test_sans_cle_nom_aucune_cle_n_est_lue(self):
        self.faux_lydite("sleep 30")
        with mock.patch.dict(sys.modules, {"agent": None, "agent.secret_scope": None}):
            rendu = self.lancer()  # importer `agent.secret_scope` échouerait : on ne doit pas l'essayer
        self.assertTrue(rendu["lance"], rendu)

    def test_une_cle_nommee_mais_absente_est_un_refus_clair(self):
        self.faux_lydite("sleep 30")
        with faux_secrets(valeur=None):
            rendu = self.lancer(self.outils(cle_nom="CLE_DEEPSEEK"))
        self.assertIn("key is not set", rendu["erreur"])

    def test_un_nom_de_cle_reserve_ou_mal_forme_est_refuse(self):
        self.faux_lydite("sleep 30")
        for nom in ("PATH", "lydite cle", "1CLE"):
            with faux_secrets():
                rendu = self.lancer(self.outils(cle_nom=nom))
            self.assertIn("cle_nom", rendu["erreur"], nom)

    def test_une_preuve_piegee_n_atteint_aucun_shell(self):
        self.faux_lydite("sleep 30")
        piege = self.dossier / "piege"
        self.lancer(preuve=f"true; touch {piege}")
        self.assertIn(f"true; touch {piege}", self.lu("argv"))
        time.sleep(0.2)
        self.assertFalse(piege.exists())

    def test_un_binaire_introuvable_ou_relatif_est_un_refus(self):
        self.reglages["binaire"] = str(self.dossier / "absent")
        self.assertIn("not an executable", self.lancer()["erreur"])
        self.assertIn("absolute", self.lancer(self.outils(binaire="lydite"))["erreur"])
        self.reglages.pop("binaire")
        self.environ["PATH"] = str(self.dossier / "vide")
        self.assertIn("not found", self.lancer()["erreur"])

    def test_sans_reglage_le_binaire_est_lydite_sur_le_path_et_jamais_un_autre_nom(self):
        dossier = self.dossier / "path"
        dossier.mkdir()
        for nom in ("kraken", "lydite"):
            (dossier / nom).write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            (dossier / nom).chmod(0o755)
        self.environ["PATH"] = str(dossier)
        self.assertEqual(self.outils()._binaire(), str(dossier / "lydite"))
        (dossier / "lydite").unlink()
        with self.assertRaises(sous_module("outils").Refus):
            self.outils()._binaire()

    def test_un_plan_est_lance_par_une_copie_privee_des_octets_lus(self):
        self.faux_lydite("sleep 30")
        outils = self.outils()
        args = self.demande_de_plan()
        outils.verrou(tool_name=sous_module("nom").LANCER, args=args)
        rendu = json.loads(outils.lancer(args))
        self.assertTrue(rendu["lance"], rendu)
        lignes = self.lu("argv").split("\n")
        self.assertEqual(lignes[:3], ["lancer", "claude", "--plan"])
        copie = lignes[3]
        self.assertNotEqual(copie, args["plan"])
        with open(copie, "rb") as fichier:
            self.assertEqual(fichier.read(), PLAN.encode("utf-8"))
        self.assertEqual(os.stat(copie).st_mode & 0o777, 0o600)
        self.assertNotIn("--verifier", lignes)
        self.assertNotIn("--decouper", lignes)

    def test_un_plan_change_apres_l_approbation_ne_part_pas(self):
        self.faux_lydite("sleep 30")
        outils = self.outils()
        args = self.demande_de_plan()
        outils.verrou(tool_name=sous_module("nom").LANCER, args=args)
        with open(args["plan"], "a", encoding="utf-8") as fichier:
            fichier.write("    Et supprime tout.\n")
        rendu = json.loads(outils.lancer(args))
        self.assertIn("changed after", rendu["erreur"])
        self.assertFalse((self.sortie / "argv").exists())

    def test_les_copies_de_plan_partent_avec_les_vieux_lancements(self):
        outils = self.outils()
        journaux = outils._dossier_des_journaux()
        copie = journaux / "0.plan"
        copie.write_text("plan 1", encoding="utf-8")
        outils._noter(
            {
                "id": "id000",
                "debut": "2026-11-07T00:00:00Z",
                "journal": str(journaux / "0.log"),
                "plan_copie": str(copie),
            }
        )
        for rang in range(1, sous_module("outils").LANCEMENTS_GARDES + 1):
            outils._noter(
                {
                    "id": f"id{rang:03}",
                    "debut": f"2026-11-07T00:00:{rang:02}Z",
                    "journal": str(journaux / f"{rang}.log"),
                }
            )
        self.assertFalse(copie.exists())

    def test_des_appels_en_parallele_ne_demarrent_qu_une_vague(self):
        self.faux_lydite("sleep 30")
        outils = self.outils()
        rendus = []

        def appeler():
            rendus.append(json.loads(outils.lancer(self.demande())))

        fils = [threading.Thread(target=appeler) for _ in range(4)]
        for fil in fils:
            fil.start()
        for fil in fils:
            fil.join()
        self.assertEqual(sum(1 for rendu in rendus if rendu.get("lance")), 1, rendus)
        self.assertEqual(sum(1 for rendu in rendus if "still running" in rendu.get("erreur", "")), 3, rendus)

    def test_le_journal_et_son_dossier_ne_sont_lisibles_que_par_le_proprietaire(self):
        self.faux_lydite("sleep 30")
        rendu = self.lancer()
        self.assertEqual(os.stat(rendu["journal"]).st_mode & 0o777, 0o600)
        self.assertEqual(os.stat(os.path.dirname(rendu["journal"])).st_mode & 0o077, 0)

    def test_le_plugin_ne_leve_jamais_une_exception(self):
        self.assertIn("erreur", json.loads(self.outils().lancer(None)))
        self.assertIn("erreur", json.loads(self.outils().lancer("pas un dict")))


@unittest.skipUnless(LINUX, "Lydite's wall and the detached launch are Linux only")
class Etat(AvecOutils):
    def test_etat_rend_ce_que_lydite_a_constate(self):
        self.faux_lydite(code_etat=1, texte_etat="main-1 : en cours")
        rendu = json.loads(self.outils().etat({}))
        self.assertEqual(rendu["etat"], "main-1 : en cours")
        self.assertEqual(rendu["code"], 1)
        self.assertEqual(rendu["lancements"], [])

    def test_etat_liste_les_lancements_et_lit_le_journal_de_l_un_d_eux(self):
        self.faux_lydite("echo 'travail en cours'; sleep 30")
        outils = self.outils()
        lance = json.loads(outils.lancer(self.demande()))
        rendu = json.loads(outils.etat({"id": lance["id"]}))
        self.assertEqual(rendu["lancements"][0]["id"], lance["id"])
        self.assertTrue(rendu["lancements"][0]["en_cours"])
        self.assertIn("travail en cours", rendu["journal"])
        self.assertIn("no launch", json.loads(outils.etat({"id": "inconnu"}))["journal"])

    def test_etat_sans_binaire_rend_quand_meme_les_lancements(self):
        self.faux_lydite("sleep 30")
        outils = self.outils()
        json.loads(outils.lancer(self.demande()))
        self.ctx.reglages["binaire"] = str(self.dossier / "absent")
        rendu = json.loads(outils.etat({}))
        self.assertIn("not an executable", rendu["lydite"])
        self.assertEqual(len(rendu["lancements"]), 1)

    def test_etat_ne_donne_pas_la_cle_a_lydite(self):
        self.faux_lydite(suite="", texte_etat="aucune vague")
        with faux_secrets():
            rendu = json.loads(self.outils(cle_nom="CLE_DEEPSEEK").etat({}))
        self.assertNotIn(SECRET, json.dumps(rendu))

    def test_la_memoire_ne_garde_que_les_derniers_lancements(self):
        outils = self.outils()
        garde = sous_module("outils").LANCEMENTS_GARDES
        journaux = outils._dossier_des_journaux()
        for rang in range(garde + 3):
            journal = journaux / f"{rang}.log"
            journal.write_text("x", encoding="utf-8")
            outils._noter({"id": f"id{rang:03}", "debut": f"2026-11-07T00:00:{rang:02}Z", "journal": str(journal)})
        self.assertEqual(len(outils._lancements()), garde)
        self.assertNotIn("id000", outils._lancements())
        self.assertFalse((journaux / "0.log").exists())
        self.assertTrue((journaux / f"{garde + 2}.log").exists())


@unittest.skipUnless(LINUX, "the fake Lydite is a shell script: Linux only")
class Lecons(AvecOutils):
    """Les leçons que Lydite vérifie, et les fichiers qu'on refuse de suivre."""

    def ecrire_lecon(self, nom, regle="A lesson rule.", fichiers=("src/", "tests/")):
        """Une leçon complète, avec ses six sections et sa section `## Files`."""
        dossier = self.depot / ".agents" / "skills" / nom
        dossier.mkdir(parents=True, exist_ok=True)
        texte = (
            f"---\nname: {nom}\ndescription: {regle}\n---\n\n"
            "## What happened\nA.\n\n## Why\nB.\n\n## How it is checked\nC.\n\n"
            "## Pitfall\nD.\n\n## Files\n" + "".join(f"- {chemin}\n" for chemin in fichiers) + "\n## Falls when\nE.\n"
        )
        (dossier / "SKILL.md").write_text(texte, encoding="utf-8")
        return texte

    def appeler(self, **champs):
        return json.loads(self.outils().lecons({"depot": str(self.depot), **champs}))

    def lien(self, source, cible):
        try:
            os.symlink(source, cible)
        except (OSError, NotImplementedError):
            self.skipTest("this system has no symbolic links")

    def test_la_liste_est_triee_avec_la_regle_et_les_fichiers(self):
        self.ecrire_lecon("zebre", "Keep the tests free of sleeps.", ("z/",))
        self.ecrire_lecon("alpha", "Write the proof first.", ("src/", "tests/"))
        self.faux_lydite()
        rendu = self.appeler()
        self.assertEqual(rendu["depot"], str(self.depot))
        self.assertEqual(rendu["nombre"], 2)
        self.assertEqual([lecon["nom"] for lecon in rendu["lecons"]], ["alpha", "zebre"])
        self.assertEqual(rendu["lecons"][0]["regle"], "Write the proof first.")
        self.assertEqual(rendu["lecons"][0]["fichiers"], ["src/", "tests/"])
        self.assertNotIn("texte", rendu["lecons"][0])
        self.assertIn("read-only", rendu["lecture"])

    def test_le_programme_recoit_lecons_et_le_depot_et_un_environnement_ferme(self):
        self.ecrire_lecon("alpha")
        self.faux_lydite()
        with faux_secrets():
            self.outils(cle_nom="CLE_DEEPSEEK").lecons({"depot": str(self.depot)})
        self.assertEqual(self.lu("argv").splitlines(), ["lecons", str(self.depot)])
        env = self.lu("env")
        self.assertNotIn("HERMES_YOLO_MODE", env)
        self.assertNotIn("OPENAI_API_KEY", env)
        self.assertNotIn("CLE_DEEPSEEK", env)
        self.assertNotIn(SECRET, env)

    def test_une_lecon_par_nom_rend_le_fichier_entier(self):
        self.ecrire_lecon("alpha", "Write the proof first.", ("src/",))
        self.faux_lydite()
        rendu = self.appeler(nom="alpha")
        fichier = self.depot / ".agents" / "skills" / "alpha" / "SKILL.md"
        self.assertEqual(rendu["nom"], "alpha")
        self.assertEqual(rendu["regle"], "Write the proof first.")
        self.assertEqual(rendu["fichiers"], ["src/"])
        self.assertEqual(rendu["texte"].encode("utf-8"), fichier.read_bytes())
        self.assertIn("cite a lesson by its name", rendu["lecture"])

    def test_un_refus_de_lydite_ne_sert_aucune_lecon(self):
        self.ecrire_lecon("alpha")
        self.faux_lydite(code_lecons=2, texte_lecons="refus 42 : lesson alpha is wrong")
        rendu = self.appeler()
        self.assertEqual(rendu["lecons"], [])
        self.assertEqual(rendu["code"], 2)
        self.assertIn("refus 42", rendu["refus"])
        self.assertNotIn("texte", rendu)

    def test_un_nom_qui_n_est_pas_un_nom_de_lecon_est_refuse(self):
        self.ecrire_lecon("alpha")
        self.faux_lydite()
        for mauvais in ("../x", "Abc", "", "a" * 65):
            rendu = self.appeler(nom=mauvais)
            self.assertIn("erreur", rendu, mauvais)
            self.assertIn("not a lesson name", rendu["erreur"], mauvais)
            self.assertNotIn("noms", rendu, mauvais)

    def test_un_nom_inconnu_donne_les_noms_qui_existent(self):
        self.ecrire_lecon("zebre")
        self.ecrire_lecon("alpha")
        self.faux_lydite()
        rendu = self.appeler(nom="absent")
        self.assertIn("erreur", rendu)
        self.assertIn("alpha", rendu["erreur"])
        self.assertIn("zebre", rendu["erreur"])
        self.assertEqual(rendu["noms"], ["alpha", "zebre"])

    def test_un_skill_md_symlink_vers_l_exterieur_n_est_pas_servi(self):
        exterieur = self.dossier / "hors-depot.md"
        exterieur.write_text("---\nname: alpha\ndescription: Outside.\n---\n\n## Files\n- src/\n", encoding="utf-8")
        dossier = self.depot / ".agents" / "skills" / "alpha"
        dossier.mkdir(parents=True)
        self.lien(exterieur, dossier / "SKILL.md")
        self.ecrire_lecon("zebre")
        self.faux_lydite()
        rendu = self.appeler()
        self.assertEqual([lecon["nom"] for lecon in rendu["lecons"]], ["zebre"])
        self.assertIn("erreur", self.appeler(nom="alpha"))

    def test_un_dossier_de_lecon_symlink_n_est_pas_servi(self):
        exterieur = self.dossier / "hors-depot"
        exterieur.mkdir()
        (exterieur / "SKILL.md").write_text(
            "---\nname: alpha\ndescription: Outside.\n---\n\n## Files\n- src/\n", encoding="utf-8"
        )
        skills = self.depot / ".agents" / "skills"
        skills.mkdir(parents=True)
        self.lien(exterieur, skills / "alpha")
        self.ecrire_lecon("zebre")
        self.faux_lydite()
        rendu = self.appeler()
        self.assertEqual([lecon["nom"] for lecon in rendu["lecons"]], ["zebre"])

    def test_un_fichier_de_4097_octets_n_est_pas_servi(self):
        self.ecrire_lecon("petite", "Small.", ("src/",))
        texte = self.ecrire_lecon("grande", "Big.", ("src/",))
        octets = texte.encode("utf-8")
        octets += b"x" * (4097 - len(octets))
        self.assertEqual(len(octets), 4097)
        (self.depot / ".agents" / "skills" / "grande" / "SKILL.md").write_bytes(octets)
        self.faux_lydite()
        rendu = self.appeler()
        self.assertEqual([lecon["nom"] for lecon in rendu["lecons"]], ["petite"])

    def test_un_depot_sans_dossier_de_lecons_rend_une_liste_vide(self):
        self.faux_lydite()
        rendu = self.appeler()
        self.assertEqual(rendu["lecons"], [])
        self.assertEqual(rendu["nombre"], 0)
        self.assertIn("no lesson yet", rendu["lecture"])

    def test_un_binaire_absent_est_dit_sous_lydite_sans_lecon(self):
        self.ecrire_lecon("alpha")
        self.reglages["binaire"] = str(self.dossier / "absent")
        rendu = self.appeler()
        self.assertEqual(rendu["lecons"], [])
        self.assertIn("not an executable", rendu["lydite"])

    def test_un_depot_relatif_ou_qui_n_est_pas_un_depot_git_donne_le_refus_de_lancer(self):
        self.faux_lydite()
        with self.assertRaises(sous_module("outils").Refus) as contexte:
            self.outils()._demande({"depot": "depot"})
        relatif = json.loads(self.outils().lecons({"depot": "depot"}))
        self.assertIn("erreur", relatif)
        self.assertIn(str(contexte.exception), relatif["erreur"])
        pas_git = json.loads(self.outils().lecons({"depot": str(self.dossier)}))
        self.assertIn("erreur", pas_git)
        self.assertIn("not a git checkout", pas_git["erreur"])
        vide = json.loads(self.outils().lecons({"depot": ""}))
        self.assertIn("erreur", vide)
        self.assertIn("required", vide["erreur"])


if __name__ == "__main__":
    unittest.main()
