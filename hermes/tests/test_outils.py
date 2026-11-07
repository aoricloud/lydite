"""Les outils : la demande vérifiée, le verrou d'approbation, le lancement détaché (avec un faux Lydite, sous
Linux), l'environnement fermé et la clé du fournisseur."""

import json
import os
import signal
import sys
import threading
import time
import unittest
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
            "PATH": "/usr/bin:/bin", "HOME": str(self.dossier), "LANG": "C.UTF-8",
            "LYDITE_DOSSIER": str(self.dossier / "dossier-lydite"),
            "OPENAI_API_KEY": "sk-une-autre-cle", "HERMES_YOLO_MODE": "1",
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

    def faux_lydite(self, suite="", code_etat=0, texte_etat="aucune vague"):
        """Un faux Lydite : il note ses arguments et son environnement, répond à `etat`, puis fait `suite`."""
        dossier = self.dossier / "bin"
        dossier.mkdir(exist_ok=True)
        chemin = dossier / "lydite"
        chemin.write_text(
            f"#!/bin/sh\nif [ \"$1\" = etat ]; then echo '{texte_etat}'; exit {code_etat}; fi\n"
            f"for a in \"$@\"; do printf '%s\\n' \"$a\"; done > '{self.sortie}/argv'\n"
            f"env > '{self.sortie}/env'\n{suite}\n",
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
    def test_un_autre_outil_passe_sans_question(self):
        self.assertIsNone(self.outils().verrou(tool_name="terminal", args={"command": "ls"}))
        self.assertIsNone(self.outils().verrou(tool_name=sous_module("nom").ETAT, args={}))

    def test_un_lancement_demande_l_approbation_avec_ce_qui_va_partir(self):
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=self.demande(profil="nautile"))
        self.assertEqual(decision["action"], "approve")
        for attendu in (str(self.depot), "issue #15", PREUVE, "nautile", "main"):
            self.assertIn(attendu, decision["message"])

    def test_l_humain_lit_la_preuve_en_entier_sans_retouche(self):
        preuve = "cargo test\n  && echo " + "a" * 1500 + "\ntrue"
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=self.demande(preuve=preuve))
        self.assertIn(preuve, decision["message"])

    def test_l_humain_lit_le_plan_en_entier_avec_son_chemin(self):
        args = self.demande_de_plan()
        decision = self.outils().verrou(tool_name=sous_module("nom").LANCER, args=args)
        self.assertEqual(decision["action"], "approve")
        self.assertIn(PLAN.rstrip(), decision["message"])
        self.assertIn(args["plan"], decision["message"])
        self.assertIn(str(self.depot), decision["message"])

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
            ["/k/lydite", "lancer", "nautile", "--issue", "15", "--decouper", "--verifier", PREUVE,
             "--depot", str(self.depot), "--pr", "main"],
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
            ["/k/lydite", "lancer", "nautile", "--plan", "/donnees/20261107.plan",
             "--depot", str(self.depot), "--pr", "main"],
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
            self.lu("argv").split("\n")[:5], ["lancer", "claude", "--issue", "15", "--decouper"],
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
        outils._noter({"id": "id000", "debut": "2026-11-07T00:00:00Z", "journal": str(journaux / "0.log"),
                       "plan_copie": str(copie)})
        for rang in range(1, sous_module("outils").LANCEMENTS_GARDES + 1):
            outils._noter({"id": f"id{rang:03}", "debut": f"2026-11-07T00:00:{rang:02}Z",
                           "journal": str(journaux / f"{rang}.log")})
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


if __name__ == "__main__":
    unittest.main()
