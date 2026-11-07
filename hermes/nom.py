"""Le nom du plugin : le seul endroit du code qui le dit, avec `plugin.yaml`.

Lydite est le nom du programme, de ses variables `LYDITE_*` et de son dossier d'état. Pour changer le nom :
ces trois lignes et `plugin.yaml` (`name`, `provides_tools`, l'`env` du secret) ; `tests/test_manifeste.py` échoue tant
que les deux ne disent pas la même chose.
"""

# L'identifiant du plugin (le `name` de `plugin.yaml`).
NOM = "lydite"

# Le préfixe des outils : `<PREFIXE>_lancer` et `<PREFIXE>_etat`.
PREFIXE = "lydite"

# La variable où Hermes tient la clé du fournisseur (l'`env` du secret `cle` de `plugin.yaml`).
SECRET_ENV = "LYDITE_CLE_FOURNISSEUR"

LANCER = f"{PREFIXE}_lancer"
ETAT = f"{PREFIXE}_etat"
