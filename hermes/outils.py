"""Les deux outils du plugin et le verrou d'approbation du lancement.

`lancer` démarre `lydite lancer ...` en arrière-plan, détaché, et rend la main au bout de quelques secondes avec un
identifiant. `etat` rend `lydite etat` (ce que Lydite a lui-même constaté) et l'état des lancements faits d'ici.

Ce que le plugin fait, et ne fait pas :
- il lance un seul programme, le binaire de Lydite, par une liste d'arguments (jamais par un shell) ;
- il ne passe à ce programme qu'une liste fermée de variables (`ENV_CONSERVE`) et, s'il y en a une, la clé du
  fournisseur, lue par `agent.secret_scope.get_secret` ; rien d'autre de l'environnement de Hermes ;
- il ne lit aucun fichier de clé, n'écrit que sous le dossier de données que Hermes lui donne, et n'accepte jamais un
  profil à la place de l'humain (`lydite accepter` reste un geste humain) ;
- la preuve et la fusion restent à Lydite : le plugin ne rejoue rien et ne fusionne rien.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from pathlib import Path

from .nom import ETAT, LANCER, PREFIXE, SECRET_ENV

# Les seules variables de l'environnement de Hermes qui passent à Lydite : celles qu'il faut pour trouver ses
# programmes et ses dossiers, jamais une clé. `GH_CONFIG_DIR` et `CLAUDE_CONFIG_DIR` nomment un dossier de connexion,
# elles n'en portent pas le contenu.
ENV_CONSERVE = (
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "LANG", "LC_ALL", "TERM", "TMPDIR",
    "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_DATA_HOME",
    "LYDITE_DOSSIER", "LYDITE_CLAUDE", "LYDITE_GH", "LYDITE_SRT",
    "GH_CONFIG_DIR", "CLAUDE_CONFIG_DIR",
)

# Combien de secondes `lancer` attend pour voir si Lydite refuse aussitôt (un refus sort en 2, sans rien lancer).
ATTENTE_DU_REFUS = 3.0

# Combien de lancements le plugin garde en mémoire, et combien d'octets d'un journal il rend.
LANCEMENTS_GARDES = 20
OCTETS_DU_JOURNAL = 4000

# L'humain approuve ce qu'il lit en entier : ni la preuve ni l'objectif ne sont tronqués dans l'approbation, donc ils
# ne dépassent pas une taille qu'on lit d'un coup d'œil.
LIMITE_DE_LA_PREUVE = 2000
LIMITE_DE_L_OBJECTIF = 500
LIMITE_DU_PLAN = 8000

# Un nom de profil ne commence jamais par un tiret : Lydite y verrait un drapeau.
_PROFIL = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_-]{0,63}")
_BRANCHE = re.compile(r"[A-Za-z0-9][A-Za-z0-9._/-]{0,127}")
_VARIABLE = re.compile(r"[A-Z][A-Z0-9_]{0,63}")

# Ce que veut dire le code de sortie de `lydite lancer` (voir le README de Lydite).
SORTIES = {
    0: "finished: everything was taken and observed (or the plan was shown, with observer)",
    1: "finished, but not everything was taken: read the end of the log and the state",
    2: "refused: Lydite started nothing; the log says why and what to do",
}

# Les lancements encore enfants de ce processus (pour les compter vivants ou morts sans zombie).
_PROCESSUS: dict[int, subprocess.Popen] = {}

# Tient la garde « une vague par dossier de Lydite » entre le contrôle et la note du lancement.
_VERROU = threading.Lock()


class Refus(Exception):
    """Un refus du plugin : son texte est rendu tel quel au modèle, qui doit pouvoir y répondre."""


def _json(objet) -> str:
    return json.dumps(objet, ensure_ascii=False)


def _maintenant() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def _empreinte(octets: bytes) -> str:
    return hashlib.sha256(octets).hexdigest()


def _fin_du_journal(chemin: str, octets: int = OCTETS_DU_JOURNAL) -> str:
    """Les derniers octets d'un journal, ou rien s'il ne se lit pas."""
    try:
        with open(chemin, "rb") as fichier:
            fichier.seek(0, os.SEEK_END)
            fichier.seek(max(0, fichier.tell() - octets))
            return fichier.read().decode("utf-8", errors="replace")
    except OSError:
        return ""


def _vivant(pid: int) -> bool:
    """Le processus d'un lancement tourne-t-il encore ? Un enfant se lit par `poll` ; sinon (Hermes a redémarré), par
    /proc, pour ne pas prendre un pid réemployé pour le nôtre."""
    enfant = _PROCESSUS.get(pid)
    if enfant is not None:
        if enfant.poll() is None:
            return True
        _PROCESSUS.pop(pid, None)
        return False
    try:
        return b"lancer" in Path(f"/proc/{pid}/cmdline").read_bytes()
    except OSError:
        return False


class Outils:
    """Les outils d'un plugin chargé : `ctx` est le contexte que Hermes donne à `register`. L'état et les réglages
    se lisent à l'usage, jamais à l'enregistrement (le contexte de la validation n'a pas d'état)."""

    def __init__(self, ctx, environ=None, attente=ATTENTE_DU_REFUS):
        self._ctx = ctx
        self._environ = os.environ if environ is None else environ
        self._attente = attente
        # Ce que l'humain a vu d'un plan : l'empreinte de ses octets, par (dépôt, profil, chemin du plan).
        self._plans_montres: dict = {}

    # --- réglages, binaire, environnement -------------------------------------------------------------------------

    def disponible(self) -> bool:
        """Les outils n'existent que sous Linux : c'est là que le mur de Lydite tient."""
        return sys.platform.startswith("linux")

    def _reglage(self, cle: str, defaut: str = "") -> str:
        valeur = self._ctx.get_config(cle, defaut)
        return valeur.strip() if isinstance(valeur, str) else defaut

    def _binaire(self) -> str:
        choisi = self._reglage("binaire")
        if choisi:
            chemin = Path(choisi).expanduser()
            if not chemin.is_absolute():
                raise Refus(f"the `binaire` setting must be an absolute path, not {choisi!r}")
            if not (chemin.is_file() and os.access(chemin, os.X_OK)):
                raise Refus(f"the `binaire` setting {str(chemin)!r} is not an executable file")
            return str(chemin)
        trouve = shutil.which("lydite", path=self._environ.get("PATH"))
        if trouve is None:
            raise Refus("the Lydite binary was not found on PATH: set the `binaire` setting of the plugin to its path")
        return trouve

    def _env(self, avec_cle: bool) -> dict:
        """L'environnement de Lydite : la liste fermée, et, pour un lancement, la clé du fournisseur si le plugin en
        a une à passer."""
        env = {nom: self._environ[nom] for nom in ENV_CONSERVE if self._environ.get(nom)}
        nom = self._reglage("cle_nom")
        if not (avec_cle and nom):
            return env
        if not _VARIABLE.fullmatch(nom) or nom in ENV_CONSERVE:
            raise Refus(f"the `cle_nom` setting {nom!r} is not a usable variable name (UPPER_SNAKE_CASE, not reserved)")
        from agent.secret_scope import get_secret  # Hermes seul : la clé ne se lit que par lui

        cle = get_secret(SECRET_ENV)
        if not cle or not cle.strip():
            raise Refus(
                f"the provider key is not set: the human must give Hermes {SECRET_ENV} (a secret of this plugin), "
                f"or empty the `cle_nom` setting if the profile needs no key"
            )
        env[nom] = cle.strip()
        return env

    # --- la demande -----------------------------------------------------------------------------------------------

    def _demande(self, args: dict) -> dict:
        """Lit et vérifie les paramètres de `lancer` ; chaque refus dit ce qu'il faut corriger."""
        depot = args.get("depot")
        if not isinstance(depot, str) or not depot.strip():
            raise Refus("`depot` is required: the absolute path of the git checkout to work on")
        chemin = Path(depot.strip()).expanduser()
        if not chemin.is_absolute():
            raise Refus(f"`depot` must be an absolute path, not {depot!r}")
        if not chemin.is_dir() or not (chemin / ".git").exists():
            raise Refus(f"`depot` {str(chemin)!r} is not a git checkout on this machine")

        issue, objectif, plan = args.get("issue"), args.get("objectif"), args.get("plan")
        if isinstance(issue, str) and issue.strip().isdigit():
            issue = int(issue)
        issue_donnee = issue is not None
        objectif_donne = isinstance(objectif, str) and bool(objectif.strip())
        plan_donne = isinstance(plan, str) and bool(plan.strip())
        if issue_donnee + objectif_donne + plan_donne != 1:
            raise Refus(
                "give exactly one of `issue` (an open issue number), `objectif` (a one-line objective) or `plan` "
                "(the absolute path of a plan file)"
            )
        if issue_donnee and (isinstance(issue, bool) or not isinstance(issue, int) or issue < 1):
            raise Refus(f"`issue` must be a positive issue number, not {issue!r}")
        if objectif_donne:
            objectif = objectif.strip()
            if "\n" in objectif or "\r" in objectif or objectif.startswith("-"):
                raise Refus("`objectif` must be one line and must not start with '-'")
            if len(objectif) > LIMITE_DE_L_OBJECTIF:
                raise Refus(f"`objectif` is {len(objectif)} characters; keep it under {LIMITE_DE_L_OBJECTIF}")

        preuve = args.get("preuve")
        plan_octets = None
        if plan_donne:
            # Un plan porte sa propre ligne `preuve :` ; Lydite ne prend ni `--verifier` ni `--observer` avec lui.
            if isinstance(preuve, str) and preuve.strip():
                raise Refus("a `plan` carries its own `preuve :` line: do not give `preuve` with it")
            if args.get("observer") is True:
                raise Refus("`observer` goes with `issue` or `objectif`, not with `plan`")
            plan, plan_octets = self._lire_le_plan(plan)
            preuve = ""
        elif not isinstance(preuve, str) or not preuve.strip():
            raise Refus("`preuve` is required: the command that proves the work done (Lydite replays it itself)")
        elif len(preuve.strip()) > LIMITE_DE_LA_PREUVE:
            raise Refus(f"`preuve` is over {LIMITE_DE_LA_PREUVE} characters: put it in a script and run that")

        profil = args.get("profil")
        profil = profil.strip() if isinstance(profil, str) and profil.strip() else self._reglage("profil", "claude")
        if not _PROFIL.fullmatch(profil):
            raise Refus(f"`profil` {profil!r} is not a profile name (letters, digits, '_', '-'; no '-' first)")

        pr = args.get("pr")
        pr = pr.strip() if isinstance(pr, str) and pr.strip() else "main"
        if not _BRANCHE.fullmatch(pr) or ".." in pr:
            raise Refus(f"`pr` {pr!r} is not a branch name")

        return {
            "depot": str(chemin),
            "issue": issue if issue_donnee else None,
            "objectif": objectif if objectif_donne else None,
            "preuve": preuve.strip(),
            "plan": plan if plan_donne else None,
            "plan_octets": plan_octets,
            "profil": profil,
            "pr": pr,
            "observer": args.get("observer") is True,
        }

    @staticmethod
    def _lire_le_plan(plan: str):
        """Le chemin et les octets d'un fichier de plan, que l'humain lira en entier avant d'approuver : un fichier
        qui se lit, en UTF-8, d'une taille qu'on lit d'un coup d'œil."""
        chemin = Path(plan.strip()).expanduser()
        if not chemin.is_absolute():
            raise Refus(f"`plan` must be an absolute path, not {plan!r}")
        try:
            if not chemin.is_file() or chemin.stat().st_size > 4 * LIMITE_DU_PLAN:
                raise Refus(f"`plan` {str(chemin)!r} is not a plan file of reasonable size")
            octets = chemin.read_bytes()
            texte = octets.decode("utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            raise Refus(f"`plan` {str(chemin)!r} cannot be read as UTF-8 text: {exc}") from exc
        if not texte.strip() or len(texte) > LIMITE_DU_PLAN:
            raise Refus(f"`plan` must hold between 1 and {LIMITE_DU_PLAN} characters, so that it can be read in full")
        return str(chemin), octets

    @staticmethod
    def _commande(binaire: str, demande: dict) -> list:
        """La ligne de commande de Lydite : une liste d'arguments, qu'aucun shell ne relit."""
        if demande["plan"]:
            return [binaire, "lancer", demande["profil"], "--plan", demande["plan_copie"],
                    "--depot", demande["depot"], "--pr", demande["pr"]]
        quoi = ["--issue", str(demande["issue"])] if demande["issue"] else [demande["objectif"]]
        argv = [binaire, "lancer", demande["profil"], *quoi, "--decouper", "--verifier", demande["preuve"]]
        argv += ["--depot", demande["depot"], "--pr", demande["pr"]]
        return argv + ["--observer"] if demande["observer"] else argv

    # --- le verrou d'approbation ----------------------------------------------------------------------------------

    def verrou(self, tool_name="", args=None, **_):
        """Le crochet `pre_tool_call` : tout lancement passe par l'approbation de l'humain, qui voit le dépôt, le
        travail et la preuve exacts. Une demande qui sera refusée de toute façon est rendue au modèle, sans
        déranger l'humain."""
        if tool_name != LANCER:
            return None
        try:
            demande = self._demande(args if isinstance(args, dict) else {})
        except Refus as refus:
            return {"action": "block", "message": _json({"erreur": str(refus)})}
        if demande["plan"]:
            texte = demande["plan_octets"].decode("utf-8")
            montre = (demande["depot"], demande["profil"], demande["plan"])
            self._plans_montres[montre] = _empreinte(demande["plan_octets"])
            quoi = f"the plan file {demande['plan']}, shown in full below: Lydite runs exactly this text"
            preuve = f"the `preuve :` line of the plan\n--- plan ---\n{texte.rstrip()}\n--- end of plan ---"
        else:
            quoi = f"issue #{demande['issue']}" if demande["issue"] else f"objective: {demande['objectif']}"
            preuve = demande["preuve"]
        message = (
            f"Start a Lydite wave in the background on this machine.\n"
            f"  repository : {demande['depot']}\n"
            f"  work       : {quoi}\n"
            f"  proof      : {preuve}\n"
            f"  profile    : {demande['profil']} (its model, wall and provider decide what the workers can do)\n"
            f"  pull request against: {demande['pr']}{' (plan only, nothing is built)' if demande['observer'] else ''}"
        )
        # L'accord « toujours » ne vaut que pour ce dépôt, ce profil et cette preuve (ou ce plan) exactement.
        corps = demande["plan_octets"].decode("utf-8") if demande["plan"] else demande["preuve"]
        cle = _empreinte("\0".join([demande["depot"], demande["profil"], corps]).encode("utf-8"))
        return {"action": "approve", "message": message, "rule_key": f"{LANCER}:{cle[:16]}"}

    # --- lancer ---------------------------------------------------------------------------------------------------

    def lancer(self, args, **_):
        try:
            return _json(self._lancer(args if isinstance(args, dict) else {}))
        except Refus as refus:
            return _json({"erreur": str(refus)})
        except Exception as exc:  # noqa: BLE001 (Hermes veut du JSON, jamais une exception)
            return _json({"erreur": f"unexpected {type(exc).__name__}: {exc}"})

    def _lancer(self, args: dict) -> dict:
        if not self.disponible():
            raise Refus("Lydite's wall only holds on Linux: this tool is Linux only")
        demande = self._demande(args)
        binaire = self._binaire()
        env = self._env(avec_cle=True)
        with _VERROU:  # deux appels en parallèle ne démarrent pas deux vagues dans le même dossier de Lydite
            return self._demarrer(demande, binaire, env, env.get("LYDITE_DOSSIER", ""))

    def _demarrer(self, demande: dict, binaire: str, env: dict, dossier: str) -> dict:
        en_cours = self._en_cours(dossier)
        if en_cours:
            raise Refus(
                f"a wave started from here is still running in this Lydite folder (launch {en_cours['id']}, pid "
                f"{en_cours['pid']}): wait for it, see {ETAT}. Two waves in one folder are not allowed"
            )

        identifiant = f"{time.strftime('%Y%m%d-%H%M%S')}-{uuid.uuid4().hex[:6]}"
        journal = self._dossier_des_journaux() / f"{identifiant}.log"
        demande["plan_copie"] = self._copier_le_plan(demande, journal) if demande["plan"] else None
        argv = self._commande(binaire, demande)
        try:
            with open(journal, "wb") as sortie:
                os.fchmod(sortie.fileno(), 0o600)  # le journal garde ce que Lydite écrit : lui seul le lit
                enfant = subprocess.Popen(
                    argv, cwd=demande["depot"], env=env, stdin=subprocess.DEVNULL, stdout=sortie,
                    stderr=subprocess.STDOUT, start_new_session=True, close_fds=True,
                )
        except OSError as exc:
            raise Refus(f"could not start Lydite: {exc}") from exc

        try:
            code = enfant.wait(timeout=self._attente)
        except subprocess.TimeoutExpired:
            code = None
            _PROCESSUS[enfant.pid] = enfant
        if demande["plan"]:
            objet = f"plan {Path(demande['plan']).name}"
        else:
            objet = f"issue #{demande['issue']}" if demande["issue"] else demande["objectif"]
        self._noter({
            "id": identifiant, "pid": enfant.pid, "debut": _maintenant(), "depot": demande["depot"],
            "profil": demande["profil"], "objet": objet, "observer": demande["observer"],
            "dossier": dossier, "journal": str(journal), "plan_copie": demande["plan_copie"], "sortie": code,
        })
        if code is not None:
            return {
                "lance": False, "id": identifiant, "sortie": code, "signification": SORTIES.get(code, "unknown code"),
                "journal": str(journal), "fin_du_journal": _fin_du_journal(str(journal)),
            }
        return {
            "lance": True, "id": identifiant, "pid": enfant.pid, "journal": str(journal), "profil": demande["profil"],
            "suite": f"running in the background; follow it with {ETAT} (id {identifiant})",
        }

    def _copier_le_plan(self, demande: dict, journal: Path) -> str:
        """Le plan que Lydite lira est une copie, faite des octets mêmes qu'on vient de lire : le fichier d'origine
        peut encore changer, la copie non. Et si l'humain a vu un autre plan que celui-là, rien ne part."""
        vu = self._plans_montres.pop((demande["depot"], demande["profil"], demande["plan"]), None)
        if vu is not None and vu != _empreinte(demande["plan_octets"]):
            raise Refus(f"the plan file {demande['plan']!r} changed after it was shown for approval: ask again")
        copie = journal.with_suffix(".plan")
        descripteur = os.open(copie, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descripteur, "wb") as fichier:
            fichier.write(demande["plan_octets"])
        return str(copie)

    # --- état -----------------------------------------------------------------------------------------------------

    def etat(self, args, **_):
        try:
            return _json(self._etat(args if isinstance(args, dict) else {}))
        except Exception as exc:  # noqa: BLE001 (Hermes veut du JSON, jamais une exception)
            return _json({"erreur": f"unexpected {type(exc).__name__}: {exc}"})

    def _etat(self, args: dict) -> dict:
        rendu: dict = {}
        try:
            fin = subprocess.run(
                [self._binaire(), "etat"], check=False, env=self._env(avec_cle=False), stdin=subprocess.DEVNULL,
                capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=30,
            )
            rendu["etat"] = fin.stdout.strip()
            rendu["code"] = fin.returncode
            rendu["lecture"] = "0: last wave finished (or none); 1: it is not (running or failed); 2: thread unreadable"
            if fin.stderr.strip():
                rendu["ecarts"] = fin.stderr.strip()[-2000:]
        except Refus as refus:
            rendu["lydite"] = str(refus)
        except subprocess.TimeoutExpired:
            rendu["lydite"] = "`lydite etat` did not answer in 30 s"

        tous = sorted(self._lancements().values(), key=lambda lancement: lancement["debut"], reverse=True)
        demande = args.get("id")
        rendu["lancements"] = [
            {
                "id": lancement["id"], "debut": lancement["debut"], "objet": lancement["objet"],
                "depot": lancement["depot"], "profil": lancement["profil"], "en_cours": self._en_cours_de(lancement),
            }
            for lancement in tous[:5]
        ]
        if isinstance(demande, str) and demande:
            choisi = self._lancements().get(demande)
            if choisi is None:
                rendu["journal"] = f"no launch {demande!r} (ids come from {LANCER})"
            else:
                rendu["journal"] = _fin_du_journal(choisi["journal"])
        return rendu

    # --- la mémoire des lancements --------------------------------------------------------------------------------

    def _dossier_des_journaux(self) -> Path:
        base = getattr(self._ctx.state, "data_dir", None)
        dossier = Path(base) if base else Path(tempfile.gettempdir()) / PREFIXE
        dossier = dossier / "journaux"
        dossier.mkdir(mode=0o700, parents=True, exist_ok=True)
        return dossier

    def _lancements(self) -> dict:
        tous = self._ctx.state.get("lancements", {})
        return tous if isinstance(tous, dict) else {}

    def _noter(self, lancement: dict) -> None:
        tous = self._lancements()
        tous[lancement["id"]] = lancement
        anciens = sorted(tous, key=lambda identifiant: tous[identifiant]["debut"])[:-LANCEMENTS_GARDES]
        for identifiant in anciens:
            ancien = tous.pop(identifiant)
            Path(ancien["journal"]).unlink(missing_ok=True)
            if ancien.get("plan_copie"):
                Path(ancien["plan_copie"]).unlink(missing_ok=True)
        self._ctx.state.set("lancements", tous)

    @staticmethod
    def _en_cours_de(lancement: dict) -> bool:
        return lancement.get("sortie") is None and _vivant(lancement["pid"])

    def _en_cours(self, dossier: str):
        """Un lancement d'ici qui tourne encore dans ce dossier de Lydite, s'il y en a un."""
        for lancement in self._lancements().values():
            if lancement.get("dossier", "") == dossier and self._en_cours_de(lancement):
                return lancement
        return None
