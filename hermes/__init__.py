"""Lydite pour Hermes : deux outils (`<préfixe>_lancer`, `<préfixe>_etat`) et le verrou d'approbation du lancement.

Le plugin ne fait que lancer le binaire de Lydite déjà présent sur la machine et lire ce que Lydite a constaté. Les
mains sont des sessions Claude Code que Lydite lance lui-même derrière son mur ; ce ne sont jamais des agents Hermes.
Voir README.md.
"""

from .nom import ETAT, LANCER, PREFIXE
from .outils import Outils
from .schemas import SCHEMA_ETAT, SCHEMA_LANCER


def register(ctx):
    """Enregistre exactement ce que `plugin.yaml` déclare : deux outils et un crochet `pre_tool_call`."""
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
    ctx.register_hook("pre_tool_call", outils.verrou)
