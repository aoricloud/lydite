"""Le contrôle de fusion d'une PR de Lydite, joué par la CI (`.github/workflows/tete.yml`).

Une PR dont la branche commence par `lydite/` ne passe que si l'arbre de sa tête est exactement celui d'un commit que
Lydite dit relu et rejoué, par la ligne que sa publication écrit dans le corps de la PR : « Reviewed and replayed on
commit <sha>. », ou, après une correction faite dans la vague, « Reviewed on commit <relue>, corrected in <sha>
without a second review, and replayed on commit <sha>. ». Le commit peut aussi venir d'une PR de correction fusionnée
dans la branche, ou dans la branche d'une telle PR, et ainsi de suite : après la fusion de ses corrections, la tête
d'une PR mère n'a été relue sous aucun nom, mais son arbre est celui de la dernière correction relue (choix du 08/11).
Les autres formes de la ligne (« not replayed », « not reviewed ») ne valent rien ici. Une PR d'une autre branche
passe sans contrôle. Une branche `lydite/` venue d'un autre dépôt, un fork, rougit : son corps n'est pas celui que la
forge de Lydite écrit, et le contrôle n'atteste que la ligne qu'il lit.

Entrées, dans l'environnement : `DEPOT` (propriétaire/nom), `NUMERO` (la PR), et `GH_TOKEN` pour `gh api`. Le code de
la PR n'est jamais extrait : tout se lit par l'API, comme du texte.
"""

import json
import os
import re
import subprocess
import sys
from urllib.parse import quote

# Les deux formes de la ligne qui disent un commit relu et rejoué ; le groupe `sha` est ce commit.
RELU = re.compile(r"Reviewed and replayed on commit (?P<sha>[0-9a-f]{40})\.")
CORRIGE = re.compile(
    r"Reviewed on commit [0-9a-f]{40}, corrected in (?P<sha>[0-9a-f]{40}) without a second review,"
    r" and replayed on commit (?P=sha)\."
)
PREFIXE = "lydite/"
# Les bornes de la remontée : les PR fusionnées dans la branche, puis dans les leurs, et ainsi de suite.
PROFONDEUR_MAX = 10
PR_MAX = 200


def relus(corps):
    """Les commits qu'un corps de PR dit relus et rejoués : chacun sur une ligne entière, aux blancs des bords près."""
    trouves = []
    for ligne in (corps or "").splitlines():
        trouve = RELU.fullmatch(ligne.strip()) or CORRIGE.fullmatch(ligne.strip())
        if trouve:
            trouves.append(trouve["sha"])
    return trouves


def gh(chemin, filtre):
    """Une lecture de l'API par `gh api`, filtrée par jq. Une erreur arrête le contrôle, qui rougit."""
    fait = subprocess.run(
        ["gh", "api", "--paginate", chemin, "--jq", filtre], check=False, capture_output=True, text=True
    )
    if fait.returncode != 0:
        raise RuntimeError(f"gh api {chemin}: {fait.stderr.strip() or fait.returncode}")
    return fait.stdout


class Api:
    """Ce que le contrôle lit de GitHub. Les essais en donnent une fausse, de mêmes méthodes."""

    def __init__(self, depot):
        self.depot = depot

    def pr(self, numero):
        """La branche, son dépôt (`meme` : celui de la base, comparé par son id ; faux s'il a été effacé), la tête et le
        corps d'une PR."""
        champs = (
            '{branche: .head.ref, depot: (.head.repo.full_name // ""), meme: (.head.repo.id == .base.repo.id),'
            ' tete: .head.sha, corps: (.body // "")}'
        )
        filtre = f"{champs} | @json"
        return json.loads(gh(f"repos/{self.depot}/pulls/{numero}", filtre))

    def arbre(self, sha):
        """L'arbre d'un commit."""
        return gh(f"repos/{self.depot}/git/commits/{sha}", ".tree.sha").strip()

    def fermees(self, branche):
        """Les PR fermées dont la base est cette branche : numéro, branche, venue du dépôt même ou non, fusionnée ou non,
        corps."""
        chemin = f"repos/{self.depot}/pulls?state=closed&base={quote(branche, safe='')}&per_page=100"
        champs = (
            "{numero: .number, branche: .head.ref, meme: (.head.repo.id == .base.repo.id),"
            ' fusionnee: (.merged_at != null), corps: (.body // "")}'
        )
        return [json.loads(ligne) for ligne in gh(chemin, f".[] | {champs} | @json").splitlines() if ligne.strip()]


def controler(api, numero):
    """Le verdict du contrôle pour une PR : (passe, message)."""
    # raccourci : le contrôle croit le corps de la PR, écrit par le jeton de publication, et non une relecture
    # attestée ; monter avec une relecture attestée (hsf_abda9e93)
    pr = api.pr(numero)
    tete = pr["tete"]
    if not pr["branche"].startswith(PREFIXE):
        return True, f"Not a Lydite pull request (branch {pr['branche']}): nothing to check."
    if not pr["meme"]:
        return False, (
            f"The branch {pr['branche']} comes from {pr['depot'] or 'a deleted repository'}, not from {api.depot}:"
            " a lydite/ branch from a fork is not Lydite's."
        )
    arbre = api.arbre(tete)
    for sha in relus(pr["corps"]):
        if sha == tete or api.arbre(sha) == arbre:
            return True, f"The head {tete} has the tree of {sha}, reviewed and replayed by Lydite."
    vues, branches = set(), [pr["branche"]]
    for _ in range(PROFONDEUR_MAX):
        suivantes = []
        for branche in branches:
            for correction in api.fermees(branche):
                # Une correction venue d'un fork n'atteste rien : son auteur écrit son corps, même après la fusion.
                if not correction["fusionnee"] or not correction["meme"] or correction["numero"] in vues:
                    continue
                vues.add(correction["numero"])
                if len(vues) > PR_MAX:
                    return False, f"More than {PR_MAX} pull requests are merged into this branch: not checked."
                for sha in relus(correction["corps"]):
                    if api.arbre(sha) == arbre:
                        return True, (
                            f"The head {tete} has the tree of {sha}, reviewed and replayed by Lydite in pull request"
                            f" {correction['numero']}, merged into this branch."
                        )
                suivantes.append(correction["branche"])
        branches = suivantes
    if relus(pr["corps"]):
        return False, (
            f"The head {tete} is not the commit that this pull request says reviewed and replayed, and no merged"
            " correction has its tree."
        )
    return False, (
        f"This pull request names no commit as reviewed and replayed by Lydite, and no merged correction has the tree"
        f" of its head {tete}."
    )


def main():
    try:
        passe, message = controler(Api(os.environ["DEPOT"]), os.environ["NUMERO"])
    except (KeyError, RuntimeError, ValueError) as erreur:
        print(f"tete: could not check the head: {erreur}", file=sys.stderr)
        return 1
    print(f"tete: {message}", file=sys.stdout if passe else sys.stderr)
    return 0 if passe else 1


if __name__ == "__main__":
    sys.exit(main())
