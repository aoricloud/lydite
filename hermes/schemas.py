"""Ce que le modèle lit de chaque outil : son nom, quand l'employer, ses paramètres."""

from .nom import ETAT, LANCER

SCHEMA_LANCER = {
    "name": LANCER,
    "description": (
        "Hand a coding job to Lydite. Use this tool, and do not do the work yourself, whenever the user asks to give "
        "Lydite an issue, an objective or a plan file, or to run a Lydite plan. It starts a wave in the background: "
        "Lydite splits one GitHub issue (or one objective) into tasks (or reads the plan), runs an "
        "isolated Claude Code worker per task, replays YOUR proof command itself on every result, keeps only what "
        "passes, and opens a pull request that states the proof result and the review outcome (it never merges). Use "
        "it to hand over a well-specified coding job together with the command that proves it done, e.g. 'do issue "
        "15 of this repo, proof: cargo test'. The workers use the model of the Lydite profile, not Hermes. A wave "
        "takes minutes to hours: this call returns at once with an id; follow it with " + ETAT + ". Linux only. The "
        "human approves each launch."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "depot": {
                "type": "string",
                "description": "Absolute path of the git checkout on this machine that Lydite works on.",
            },
            "issue": {
                "type": "integer",
                "description": (
                    "Number of the open issue of the checkout's `origin` to do. "
                    "Give one of `issue`, `objectif`, `plan`."
                ),
            },
            "objectif": {
                "type": "string",
                "description": "One-line objective to do instead of an issue. Give one of `issue`, `objectif`, `plan`.",
            },
            "plan": {
                "type": "string",
                "description": (
                    "Absolute path of a plan file written by hand (tasks, models, files, and its own `preuve :` line) "
                    "instead of letting Lydite split an issue or objective. Use it when the user gives you the path "
                    "of a plan file to run: pass the path, do not read or execute the plan yourself. Give one of "
                    "`issue`, `objectif`, `plan`; no `preuve`, no `observer` with it. The human reads the whole plan "
                    "before approving."
                ),
            },
            "preuve": {
                "type": "string",
                "description": (
                    "The command that proves the work done (e.g. 'cargo test && cargo clippy -- -D warnings'). "
                    "Required with `issue` or `objectif`, not with `plan`. Lydite runs it itself, in each worker's "
                    "box. It must pass on the untouched base first, or nothing starts."
                ),
            },
            "profil": {
                "type": "string",
                "description": "Lydite profile to use (accepted beforehand by a human). Default: the plugin setting.",
            },
            "pr": {
                "type": "string",
                "description": "Branch the pull request targets. Default: main. Lydite never merges it.",
            },
            "observer": {
                "type": "boolean",
                "description": "Only show the plan Lydite would run (no worker, no pull request). Default: false.",
            },
        },
        "required": ["depot"],
    },
}

SCHEMA_ETAT = {
    "name": ETAT,
    "description": (
        "Report where Lydite stands: the last wave's tasks, proofs, pull request and CI as Lydite itself recorded "
        "them (a fact a worker only declared is shown as declared), plus the waves started from this plugin and "
        "whether their process is still running. Pass `id` (returned by " + LANCER + ") to also read the end of "
        "that launch's log."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "Launch id returned by " + LANCER + ". Omit for the overall state.",
            },
        },
        "required": [],
    },
}
