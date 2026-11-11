"""Lydite pour Hermes : cinq outils (`<préfixe>_lancer`, `<préfixe>_etat`, `<préfixe>_lecons`, `<préfixe>_plan`,
`<préfixe>_findings`) et le verrou d'approbation du lancement.

Le plugin ne fait que lancer le binaire de Lydite déjà présent sur la machine et lire ce que Lydite a constaté. Les
mains sont des sessions Claude Code que Lydite lance lui-même derrière son mur ; ce ne sont jamais des agents Hermes.
Voir README.md.
"""

import re
from pathlib import Path

from .nom import ETAT, FINDINGS, LANCER, LECONS, PLAN, PREFIXE
from .outils import Outils
from .schemas import SCHEMA_ETAT, SCHEMA_FINDINGS, SCHEMA_LANCER, SCHEMA_LECONS, SCHEMA_PLAN

# La skill de pilotage, au format ouvert `SKILL.md` : Hermes la rend sous `lydite:driving-lydite`. Sa description est
# celle de son fichier, lue ici (une seule ligne) pour n'être écrite qu'une fois.
SKILL = "driving-lydite"
FICHIER_SKILL = Path(__file__).resolve().parent / "skills" / SKILL / "SKILL.md"


def register(ctx):
    """Enregistre ce que `plugin.yaml` déclare (cinq outils et un crochet `pre_tool_call`) et la skill de pilotage."""
    outils = Outils(ctx)
    ctx.register_tool(
        name=LANCER,
        toolset=PREFIXE,
        schema=SCHEMA_LANCER,
        handler=outils.lancer,
        check_fn=outils.disponible,
        description="Start a Lydite wave from an issue and its proof",
        emoji="🐙",
    )
    ctx.register_tool(
        name=ETAT,
        toolset=PREFIXE,
        schema=SCHEMA_ETAT,
        handler=outils.etat,
        check_fn=outils.disponible,
        description="Where Lydite stands",
        emoji="🐙",
    )
    ctx.register_tool(
        name=LECONS,
        toolset=PREFIXE,
        schema=SCHEMA_LECONS,
        handler=outils.lecons,
        check_fn=outils.disponible,
        description="The lessons Lydite learned on this repository",
        emoji="🐙",
    )
    ctx.register_tool(
        name=PLAN,
        toolset=PREFIXE,
        schema=SCHEMA_PLAN,
        handler=outils.plan,
        check_fn=outils.disponible,
        description="Check a plan file with Lydite, run nothing",
        emoji="🐙",
    )
    ctx.register_tool(
        name=FINDINGS,
        toolset=PREFIXE,
        schema=SCHEMA_FINDINGS,
        handler=outils.findings,
        check_fn=outils.disponible,
        description="Read a hermes-security export and write one Lydite plan per finding, run nothing",
        emoji="🐙",
    )
    ctx.register_hook("pre_tool_call", outils.verrou)
    description = re.search(r"^description: (.+)$", FICHIER_SKILL.read_text(encoding="utf-8"), re.MULTILINE).group(1)
    # `platforms` : comme les outils, qui se cachent hors de Linux. Pas de `requires_tools` : derrière `tool_search`
    # les outils ne sont pas offerts au modèle, et la skill est justement ce qui le met sur leur piste.
    ctx.register_skill(SKILL, FICHIER_SKILL, description=description, frontmatter={"platforms": ["linux"]})
