"""La garde de ce que le dépôt publie, jouée par le workflow `garde` à chaque PR et à chaque push sur main.

Elle cherche les mots de la liste `GARDE` (un par ligne, sans casse ; BOM retiré, lignes vides et `#` ignorées) et
quelques motifs génériques (adresses et noms d'un réseau privé, chemins personnels, adresses électroniques) dans les
noms neufs des fichiers changés, les lignes ajoutées par le diff net et par chaque commit, les commits (messages,
auteurs et committers), le nom de la branche, le titre et le corps. Elle nomme le lieu, jamais le mot ni ce qu'un motif
a trouvé : les noms passent d'abord, et un fichier n'est nommé que si aucun nom neuf ne porte rien. Une ligne de la
liste qui porte plusieurs mots se cherche aussi repliée (voir `porte`) : coupée en fin de ligne ou séparée par d'autres
signes, elle se trouve encore. L'adresse de l'auteur et du committer d'un commit ne passe que par la liste, jamais par
le motif des adresses électroniques : c'est celle qu'un contributeur publie lui-même avec son commit, et la garde
rejouée sur `main` après la fusion de sa PR la reverrait.

La PR d'un fork ne lit pas la liste (`LISTE=fork`) : sa sortie est publique, et rougir sur un mot deviné le
confirmerait. Ailleurs, une liste vide ou absente est une faute : la garde ne passe pas sans elle.

Un fichier binaire ne se lit pas comme du texte : il passe si, et seulement si, la liste des binaires approuvés
(`binaires.txt`, à côté de ce script, donc celle de la branche de base) porte son chemin et l'empreinte SHA-256 de
chacune des versions que les commits lui donnent, et que sa version finale est l'une d'elles (voir `juge_du_binaire`).

Entrées : en argument, le dossier que l'étape d'avant remplit par l'API (`diff`, `commits.jsonl`, `pr.json` avec le
titre, le corps et la branche d'une PR, `fichiers.jsonl` avec l'id de blob de chaque fichier changé, et `erreur` quand
une lecture a échoué) ; dans l'environnement, `GARDE` et `LISTE`. Le texte de la PR ne passe jamais par
l'environnement, que le journal d'Actions affiche.
Sortie : une annotation par trouvaille, en anglais ; code 1 s'il y en a une, 0 sinon.
"""

import base64
import hashlib
import json
import os
import re
import string
import sys
import zlib
from pathlib import Path

BOM = chr(0xFEFF)

# La liste des binaires approuvés : celle qui est à côté de ce script, c'est-à-dire, sous `pull_request_target`, celle de
# la branche par défaut, qu'une PR ne change pas pour elle-même.
BINAIRES = Path(__file__).resolve().parent / "binaires.txt"
# La taille d'un fichier au-delà de laquelle un `GIT binary patch` ne se lit pas (la borne d'un fichier sur GitHub).
TAILLE_MAX = 100 * 1024 * 1024
# La lettre qui ouvre une ligne de données d'un `GIT binary patch` dit combien d'octets elle porte : A-Z 1 à 26, a-z
# 27 à 52.
LONGUEURS = string.ascii_uppercase + string.ascii_lowercase

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


def replier(texte):
    """La forme repliée d'un texte, comme `publiable` la fait dans Lydite : en minuscules, chaque suite de signes qui ne
    sont ni lettres ni chiffres réduite à un blanc, sans blanc aux bords."""
    return " ".join(re.findall(r"[^\W_]+", texte.lower()))


def porte(bas, replie, mot):
    """Le texte, en minuscules (`bas`) et replié (`replie`), porte la ligne `mot` de la liste : à la lettre ; ou, pour
    une ligne d'au moins deux mots que le repli garde, sous sa forme repliée. Un mot seul ne se compare qu'à la
    lettre."""
    if mot in bas:
        return True
    forme = replier(mot)
    return len(mot.split()) >= 2 and len(forme.split()) >= 2 and forme in replie


def trouvailles(texte, mots, *, courriels=True):
    """Ce que porte le texte, nommé sans ce qui a été trouvé : les lignes de la liste, puis les motifs ; sans celui des
    adresses électroniques si `courriels` est faux."""
    bas, replie = texte.lower(), replier(texte)
    trouve = [f"Line {numero} of the guard list" for numero, mot in mots if porte(bas, replie, mot)]
    if RESEAU_PRIVE.search(texte):
        trouve.append("A private network address")
    if NOM_DE_RESEAU.search(texte):
        trouve.append("A private network name")
    if CHEMIN_PERSONNEL.search(texte):
        trouve.append("A personal path")
    if courriels and any(not courriel_permis(c) for c in COURRIEL.finditer(texte)):
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


def version_binaire(lignes, neuf):
    """La version qu'un `GIT binary patch` donne à un fichier, lue de sa première partie (`lignes`, de `literal
    <taille>` à la ligne vide exclue : base 85 par ligne, puis zlib) : son empreinte SHA-256 et son id de blob git. None
    si elle ne se lit pas sans doute : une partie `delta` (elle ne se lit qu'avec l'ancien contenu), des données qui ne
    se décodent pas, une taille fausse ou trop grande, ou un id de blob qui n'est pas `neuf`, celui de la ligne
    `index`."""
    entete = re.fullmatch(r"literal (\d+)", lignes[0]) if lignes else None
    if entete is None or not re.fullmatch(r"[0-9a-f]{40}", neuf or "") or int(entete.group(1)) > TAILLE_MAX:
        return None
    taille, brut = int(entete.group(1)), bytearray()
    try:
        for ligne in lignes[1:]:
            octets = LONGUEURS.find(ligne[:1]) + 1
            donnees = base64.b85decode(ligne[1:])
            if octets == 0 or len(donnees) < octets:
                return None
            brut += donnees[:octets]
        lecteur = zlib.decompressobj()
        contenu = lecteur.decompress(bytes(brut), taille + 1)
    except (ValueError, zlib.error):
        return None
    blob = hashlib.sha1(b"blob %d\0" % len(contenu) + contenu).hexdigest()
    if len(contenu) != taille or not lecteur.eof or blob != neuf:
        return None
    return hashlib.sha256(contenu).hexdigest(), blob


def lire_diff(texte):
    """D'un diff de git, net ou commit par commit : les noms neufs (un fichier supprimé n'en a pas), les lignes
    ajoutées par fichier, les fichiers binaires publiés, et, pour chacun, les versions que les commits lui donnent
    (`version_binaire`, None pour une version qui ne se lit pas). Le diff net ne porte pas le contenu d'un binaire :
    seul un `GIT binary patch` en donne une version."""
    noms, ajouts, binaires, versions = [], {}, [], {}
    en_tete, dans_l_en_tete, fichier, partie = None, False, None, None

    def clore():
        nonlocal fichier
        nom = None if en_tete["supprime"] else (en_tete["plus"] or en_tete["vers"] or en_tete["b"])
        if nom is not None:
            noms.append(nom)
            if en_tete["binaire"]:
                binaires.append(nom)
            if en_tete["patch"] is not None:
                versions.setdefault(nom, []).append(version_binaire(en_tete["patch"], en_tete["neuf"]))
        fichier = nom or en_tete["b"]

    for ligne in texte.split("\n"):
        if partie is not None:
            if ligne:
                partie.append(ligne)
                continue
            partie = None  # la ligne vide clôt la première partie ; la seconde (le retour) ne compte pas
        if re.match(r"From [0-9a-f]{40} ", ligne):
            # Un commit du `.patch` commence : son message n'est d'aucun fichier, et une de ses lignes (« deleted file
            # mode ») ne doit pas changer l'en-tête resté ouvert d'un bloc sans `@@`, comme celui d'un binaire.
            if dans_l_en_tete:
                clore()
            en_tete, dans_l_en_tete = None, False
            continue
        if ligne.startswith("diff --git "):
            if dans_l_en_tete:
                clore()  # un bloc sans `@@` : binaire, renommage seul, mode
            en_tete = {"b": cote_b(ligne[len("diff --git ") :]), "plus": None, "vers": None}
            en_tete.update(supprime=False, binaire=False, neuf=None, patch=None)
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
            elif ligne.startswith("index ") and ".." in ligne:
                en_tete["neuf"] = ligne.split("..", 1)[1].split(" ", 1)[0]
            elif ligne.startswith("Binary files "):
                en_tete["binaire"] = True
            elif ligne == "GIT binary patch" and en_tete["patch"] is None:
                en_tete["binaire"], en_tete["patch"] = True, []
                partie = en_tete["patch"]
        elif ligne.startswith("+") and en_tete is not None:
            ajouts.setdefault(fichier, []).append(ligne[1:])
    if dans_l_en_tete:
        clore()
    return noms, ajouts, binaires, versions


def lire_binaires(chemin):
    """Les binaires approuvés (chemin → empreinte SHA-256) de la liste `chemin`, et une faute par ligne mal écrite.
    Une ligne s'écrit `<empreinte>  <chemin>` (la forme de sha256sum), un chemin une fois ; `#` ouvre un commentaire,
    une ligne vide ne compte pas. Une ligne mal écrite n'approuve rien : la liste entière ne vaut plus. Une liste
    absente n'approuve rien."""
    approuves, fautes = {}, []
    if not chemin.exists():
        return approuves, fautes
    for numero, ligne in enumerate(chemin.read_text(encoding="utf-8").removeprefix(BOM).splitlines(), start=1):
        if not ligne.strip() or ligne.lstrip().startswith("#"):
            continue
        lu = re.fullmatch(r"([0-9a-f]{64})  (\S(?:.*\S)?)", ligne)
        if lu is None or lu.group(2) in approuves:
            fautes.append(
                f"Line {numero} of .github/binaires.txt is not `<sha256>  <path>` for a path not listed above: "
                "no binary file is approved."
            )
        else:
            approuves[lu.group(2)] = lu.group(1)
    return ({} if fautes else approuves), fautes


def lire_finales(chemin):
    """L'id de blob de la version finale de chaque fichier changé, que l'étape d'avant a lu par l'API (`nom`, `sha`,
    un objet JSON par ligne)."""
    if not chemin.exists():
        return {}
    lus = [json.loads(ligne) for ligne in chemin.read_text(encoding="utf-8").splitlines() if ligne.strip()]
    return {lu.get("nom"): lu.get("sha") for lu in lus}


def juge_du_binaire(approuve, lues, finale):
    """Pourquoi un binaire publié ne passe pas, None s'il passe. `approuve` est l'empreinte que la liste de la base lui
    donne (None s'il n'y est pas), `lues` les versions que les commits lui donnent (`version_binaire`), `finale` l'id de
    blob de sa version finale. Il passe si chaque version se lit et porte l'empreinte approuvée, et que la version
    finale est l'une d'elles : une version qu'aucun `GIT binary patch` ne montre (celle d'un commit de fusion) ne se
    lit pas."""
    if approuve is None:
        return "its content cannot be checked"
    if not lues or None in lues or finale not in {blob for _, blob in lues}:
        return "its content could not be read from the change"
    if any(empreinte != approuve for empreinte, _ in lues):
        return "its content is not the approved one"
    return None


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
    noms, ajouts, binaires, versions = lire_diff(diff)
    dans_les_noms = trouvailles("\n".join(noms), mots)
    fautes += [f"{t} appears in a changed file name." for t in dans_les_noms]
    noms_surs = not dans_les_noms
    for fichier, lignes in ajouts.items():
        lieu = f"the added lines of {fichier}" if noms_surs and fichier else "the added lines of a changed file"
        fautes += [f"{t} appears in {lieu}." for t in trouvailles("\n".join(lignes), mots)]
    approuves, mal_ecrites = lire_binaires(BINAIRES)
    fautes += mal_ecrites
    finales = lire_finales(dossier / "fichiers.jsonl")
    for fichier in binaires:
        raison = juge_du_binaire(approuves.get(fichier), versions.get(fichier, []), finales.get(fichier))
        if raison is not None:
            nomme = f": {fichier}" if noms_surs else ""
            fautes.append(f"A binary file is changed{nomme}; {raison}.")
    for commit in lire_commits(dossier / "commits.jsonl"):
        court = str(commit.get("sha") or "")[:7]
        message = trouvailles(commit.get("m") or "", mots)
        personnes = trouvailles(identites(commit), mots, courriels=False)
        fautes += [f"{t} appears in the message of commit {court}." for t in message]
        fautes += [f"{t} appears in the author or committer of commit {court}." for t in personnes]
    pr = json.loads((dossier / "pr.json").read_text(encoding="utf-8")) if (dossier / "pr.json").exists() else {}
    for cle, lieu in (("branche", "the branch name"), ("titre", "the title"), ("corps", "the body")):
        fautes += [f"{t} appears in {lieu}." for t in trouvailles(pr.get(cle) or "", mots)]
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
