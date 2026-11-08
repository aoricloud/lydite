"""La garde de ce que le dépôt publie, jouée par le workflow `garde` à chaque PR et à chaque push sur main.

Elle cherche les mots de la liste `GARDE` (un par ligne, sans casse ; BOM retiré, lignes vides et `#` ignorées) et
quelques motifs génériques (adresses et noms d'un réseau privé, chemins personnels, adresses électroniques) dans les
noms neufs des fichiers changés, les lignes ajoutées par le diff net et par chaque commit, les commits (messages,
auteurs et committers), le nom de la branche, le titre et le corps. Elle nomme le lieu, jamais le mot ni ce qu'un motif
a trouvé : les noms passent d'abord, et un fichier n'est nommé que si aucun nom neuf ne porte rien.

La PR d'un fork ne lit pas la liste (`LISTE=fork`) : sa sortie est publique, et rougir sur un mot deviné le
confirmerait. Ailleurs, une liste vide ou absente est une faute : la garde ne passe pas sans elle.

Entrées : en argument, le dossier que l'étape d'avant remplit par l'API (`diff`, `commits.jsonl`, et `erreur` quand une
lecture a échoué) ; dans l'environnement, `GARDE`, `LISTE`, `TITRE`, `CORPS` et `BRANCHE` (les trois derniers vides
sur un push). Sortie : une annotation par trouvaille, en anglais ; code 1 s'il y en a une, 0 sinon.
"""

import json
import os
import re
import sys
from pathlib import Path

BOM = chr(0xFEFF)

# Les motifs génériques, repris du relevé d'arbre.sh et resserrés pour ne rougir que sur une fuite probable : les
# valeurs d'essai du dépôt (127.0.0.1, exemple.invalid, /home/x, C:\Users\é t, %USERPROFILE%, ~/.local,
# settings.local.json) passent. Ils sont écrits pour que leur source ne tombe pas sous eux (« t[a]ilscale ») : la
# garde relit ce fichier quand il change.
RESEAU_PRIVE = re.compile(
    r"\b(?:10(?:\.\d{1,3}){3}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2}|192\.168(?:\.\d{1,3}){2}"
    r"|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7])(?:\.\d{1,3}){2})\b"
    r"|\bf[cd][0-9a-f]{2}:[0-9a-f]{1,4}:[0-9a-f]{1,4}:",
    re.IGNORECASE,
)
NOM_DE_RESEAU = re.compile(
    r"\.ts\.n[e]t\b|\.l[a]n\b|\.home\.[a]rpa\b|t[a]ilscale|(?<=\w)\.loc[a]l\b(?![.-])"
    r"|(?<!host\.docker)(?<!metadata\.google)\.intern[a]l\b",
    re.IGNORECASE,
)
GABARITS = r"(?i:you|user|x|me|é|compte|example|name|runner|runneradmin)\b"
# Le chemin Windows sans casse ; /home/ et /Users/ avec la leur, pour que /users/<login> (l'API) passe.
CHEMIN_PERSONNEL = re.compile(
    rf"(?i:[A-Za-z]:[\\/]+Users[\\/]+)(?!{GABARITS})[^\\/\s'\"`<>]+"
    rf"|/home/(?!{GABARITS})[\w.-]+|/Users/(?!{GABARITS})[\w.-]+"
)
COURRIEL = re.compile(r"[\w.%+-]+@([\w-]+(?:\.[\w-]+)*\.[A-Za-z]{2,})")
COURRIELS_PERMIS = {"noreply@anthropic.com"}
DOMAINES_PERMIS = {"github.com", "users.noreply.github.com", "example.com", "example.net", "example.org"}
SUFFIXES_PERMIS = (".invalid", ".test", ".example", ".localhost")

# Les échappements de git dans un nom cité (quote_c_style), hors l'octal.
ECHAPPEMENTS = {b"a": 7, b"b": 8, b"t": 9, b"n": 10, b"v": 11, b"f": 12, b"r": 13, b'"': 34, b"\\": 92}

# Au-delà, les trouvailles se comptent sans se dire une à une.
TROUVAILLES_DITES = 50


def mots_de_la_liste(brut):
    """Les mots de la liste avec leur numéro de ligne, en minuscules : BOM retiré, blancs du bord retirés, lignes vides
    et `#` ignorées."""
    mots = []
    for numero, ligne in enumerate(brut.removeprefix(BOM).splitlines(), start=1):
        mot = ligne.strip()
        if mot and not mot.startswith("#"):
            mots.append((numero, mot.lower()))
    return mots


def courriel_permis(trouve):
    """Une adresse d'essai, sans boîte réelle derrière, ou une adresse qui ne répond pas (noreply)."""
    adresse, domaine = trouve.group(0).lower(), trouve.group(1).lower()
    return adresse in COURRIELS_PERMIS or domaine in DOMAINES_PERMIS or domaine.endswith(SUFFIXES_PERMIS)


def trouvailles(texte, mots):
    """Ce que porte le texte, nommé sans ce qui a été trouvé : les lignes de la liste, puis les motifs."""
    bas = texte.lower()
    trouve = [f"Line {numero} of the guard list" for numero, mot in mots if mot in bas]
    if RESEAU_PRIVE.search(texte):
        trouve.append("A private network address")
    if NOM_DE_RESEAU.search(texte):
        trouve.append("A private network name")
    if CHEMIN_PERSONNEL.search(texte):
        trouve.append("A personal path")
    if any(not courriel_permis(c) for c in COURRIEL.finditer(texte)):
        trouve.append("An e-mail address")
    return trouve


def sans_guillemets(nom):
    """Un nom que git a cité (`"b/\\303\\251t\\303\\251"`), rendu en clair ; un nom non cité, tel quel."""
    if not (len(nom) >= 2 and nom[0] == nom[-1] == '"'):
        return nom

    def octet(trouve):
        echappe = trouve.group(1)
        if len(echappe) == 3:
            return bytes([int(echappe, 8) & 0xFF])
        return bytes([ECHAPPEMENTS.get(echappe, echappe[0])])

    brut = nom[1:-1].encode("utf-8", "surrogateescape")
    return re.sub(rb"\\([0-7]{3}|.)", octet, brut, flags=re.DOTALL).decode("utf-8", "replace")


def cote_b(reste):
    """Le nom `b/` d'une ligne `diff --git a/… b/…` (sans son début), en clair ; None s'il ne se lit pas sans doute."""
    if reste.endswith('"'):
        debut = reste.rfind(' "b/')
        return sans_guillemets(reste[debut + 1 :])[2:] if debut >= 0 else None
    return reste.split(" b/", 1)[1] if reste.count(" b/") == 1 else None


def chemin_d_en_tete(ligne, prefixe):
    """Le nom d'une ligne `+++ b/…` ou `--- a/…`, en clair et sans préfixe ; None pour `/dev/null`."""
    nom = sans_guillemets(ligne[4:].rstrip("\t\r"))
    return nom[len(prefixe) :] if nom.startswith(prefixe) else None


def lire_diff(texte):
    """D'un diff de git, net ou commit par commit : les noms neufs (un fichier supprimé n'en a pas), les lignes
    ajoutées par fichier, et les fichiers binaires publiés."""
    noms, ajouts, binaires = [], {}, []
    en_tete, dans_l_en_tete, fichier = None, False, None

    def clore():
        nonlocal fichier
        nom = None if en_tete["supprime"] else (en_tete["plus"] or en_tete["vers"] or en_tete["b"])
        if nom is not None:
            noms.append(nom)
            if en_tete["binaire"]:
                binaires.append(nom)
        fichier = nom or en_tete["b"]

    for ligne in texte.split("\n"):
        if ligne.startswith("diff --git "):
            if dans_l_en_tete:
                clore()  # un bloc sans `@@` : binaire, renommage seul, mode
            en_tete = {"b": cote_b(ligne[len("diff --git ") :]), "plus": None, "vers": None}
            en_tete.update(supprime=False, binaire=False)
            dans_l_en_tete = True
        elif dans_l_en_tete:
            if ligne.startswith("@@"):
                clore()
                dans_l_en_tete = False
            elif ligne.startswith("+++ "):
                en_tete["plus"] = chemin_d_en_tete(ligne, "b/")
            elif ligne.startswith(("rename to ", "copy to ")):
                en_tete["vers"] = sans_guillemets(ligne.split(" to ", 1)[1])
            elif ligne.startswith("deleted file mode"):
                en_tete["supprime"] = True
            elif ligne.startswith(("Binary files ", "GIT binary patch")):
                en_tete["binaire"] = True
        elif ligne.startswith("+") and en_tete is not None:
            ajouts.setdefault(fichier, []).append(ligne[1:])
    if dans_l_en_tete:
        clore()
    return noms, ajouts, binaires


def lire_commits(chemin):
    """Les commits que l'étape d'avant a écrits, un objet JSON par ligne (`sha`, `m`, `a`, `c`)."""
    if not chemin.exists():
        return []
    return [json.loads(ligne) for ligne in chemin.read_text(encoding="utf-8").splitlines() if ligne.strip()]


def identites(commit):
    """L'auteur et le committer d'un commit, nom et adresse."""
    personnes = (commit.get("a") or {}, commit.get("c") or {})
    return "\n".join(f"{p.get('name') or ''} <{p.get('email') or ''}>" for p in personnes)


def sans_commande(texte):
    """Un texte sans de quoi ouvrir une commande de workflow : `%`, retours et caractères de contrôle échappés."""
    return "".join(c if c.isprintable() and c != "%" else f"%{ord(c):02X}" for c in texte)


def garder(dossier, environ):
    """Les mots lus, les fautes (une phrase en anglais chacune, sans doublon) et les avis de ce que le dossier et
    l'environnement donnent à lire."""
    fork = environ.get("LISTE") == "fork"
    mots = [] if fork else mots_de_la_liste(environ.get("GARDE") or "")
    fautes, avis = [], []
    if fork:
        avis.append("A pull request from a fork: its output is public, so only the generic patterns are checked.")
    elif not mots:
        fautes.append("The GARDE secret is empty or missing: the guard list could not be checked.")
    if (dossier / "erreur").exists():
        fautes.append("Part of the change could not be read from the API (it may be too large): check it by hand.")
    diff = (dossier / "diff").read_text(encoding="utf-8", errors="replace") if (dossier / "diff").exists() else ""
    noms, ajouts, binaires = lire_diff(diff)
    dans_les_noms = trouvailles("\n".join(noms), mots)
    fautes += [f"{t} appears in a changed file name." for t in dans_les_noms]
    noms_surs = not dans_les_noms
    for fichier, lignes in ajouts.items():
        lieu = f"the added lines of {fichier}" if noms_surs and fichier else "the added lines of a changed file"
        fautes += [f"{t} appears in {lieu}." for t in trouvailles("\n".join(lignes), mots)]
    for fichier in binaires:
        nomme = f": {fichier}" if noms_surs else ""
        fautes.append(f"A binary file is changed{nomme}; its content cannot be checked.")
    for commit in lire_commits(dossier / "commits.jsonl"):
        court = str(commit.get("sha") or "")[:7]
        message, personnes = trouvailles(commit.get("m") or "", mots), trouvailles(identites(commit), mots)
        fautes += [f"{t} appears in the message of commit {court}." for t in message]
        fautes += [f"{t} appears in the author or committer of commit {court}." for t in personnes]
    for variable, lieu in (("BRANCHE", "the branch name"), ("TITRE", "the title"), ("CORPS", "the body")):
        fautes += [f"{t} appears in {lieu}." for t in trouvailles(environ.get(variable) or "", mots)]
    return mots, list(dict.fromkeys(fautes)), avis


def main():
    mots, fautes, avis = garder(Path(sys.argv[1]), os.environ)
    for un_avis in avis:
        print(f"::notice title=garde::{sans_commande(un_avis)}")
    for faute in fautes[:TROUVAILLES_DITES]:
        print(f"::error title=garde::{sans_commande(faute)}")
    if len(fautes) > TROUVAILLES_DITES:
        print(f"::error title=garde::{len(fautes) - TROUVAILLES_DITES} more findings.")
    if fautes:
        return 1
    print(f"garde: nothing found ({len(mots)} words of the list and the generic patterns).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
