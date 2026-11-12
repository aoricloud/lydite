# Lydite refusals

A refusal that stops Lydite before any wave exits with code 2, on four lines: `lydite : <code> <name>`, then `rule:`, `seen:` and `fix:`. The same codes name the refusals of a wave, which `lydite status` shows. A code and its name never change; a rule may be reworded. The codes come in four blocks of a thousand:

- 1000, `contrat`: the command line and the thread.
- 6000, `lancer`: everything before a wave opens (the command line, the profile, the repository, Lydite's folder and the plan).
- 7000, `vague`: the hands and the assembly, then the files of each hand, the splitter, the reviewer and the correction that follows its review.
- 8000, `forge`: the pull request and the CI, before and after the wave.

| Code | Name | Rule |
|---|---|---|
| 1001 | `contrat.json_illisible` | A line of the thread is a JSON object in UTF-8 (RFC 8259). |
| 1002 | `contrat.version_differente` | A reader reads `v` before anything else, and reads only the thread version it knows. |
| 1005 | `contrat.commande_inconnue` | The command is a word from Lydite's closed list, which the usage gives. |
| 1006 | `contrat.commande_mal_formee` | The command line names a command and gives it the arguments of its usage, each flag once, with its value if it takes one. |
| 1009 | `contrat.evenement_mal_forme` | A v2 line carries its whole envelope and flat values; a known type carries all its fields, each in its domain, and no other. |
| 1010 | `contrat.sequence_rompue` | Within a session, `seq` runs 1, 2, 3… in thread order, with no gap and no repeat. |
| 6001 | `lancer.profil_inconnu` | A profile is named among the profiles Lydite knows. |
| 6002 | `lancer.depot_introuvable` | The target repository is a folder that exists. |
| 6003 | `lancer.bac_impossible` | The target repository is a git repository with a commit, from which the task worktrees start; without it, no wave opens. |
| 6004 | `lancer.dossier_relatif` | `LYDITE_DOSSIER`, if set, is an absolute path. |
| 6005 | `lancer.dossier_introuvable` | Without `LYDITE_DOSSIER`, Lydite's folder is taken from `USERPROFILE` on Windows, from `XDG_STATE_HOME` or `HOME` elsewhere, each absolute. |
| 6006 | `lancer.entree_sortie` | A read or a write by Lydite succeeds, or its refusal names its path or its program. |
| 6007 | `lancer.profil_nom_refuse` | A profile name is made of ASCII letters, digits, `_` and `-`. |
| 6008 | `lancer.profil_illisible` | A profile file is read as UTF-8 (BOM and carriage return tolerated); otherwise its refusal names its path. |
| 6009 | `lancer.profil_ligne_refusee` | Each line of the profile is `key = value` for a known key, its value in its domain, each key once; no budget. |
| 6010 | `lancer.profil_incomplet` | A profile says its kind. |
| 6011 | `lancer.profil_non_accepte` | A profile in a file is launched only once its fingerprint is accepted. |
| 6012 | `lancer.profil_change` | The launched profile has the accepted fingerprint; a new fingerprint is accepted again by `lydite accept`. |
| 6013 | `lancer.profil_sans_fichier` | Only a profile in a file is accepted; the built-in profile has no fingerprint to keep. |
| 6014 | `lancer.preuve_vide` | `--verify`, if given, carries a command that is not blank. |
| 6015 | `lancer.dossier_sous_le_depot` | Lydite's folder is outside the target repository. |
| 6016 | `lancer.plan_version` | A plan starts with its version, `plan 1`, which Lydite reads before anything else. |
| 6017 | `lancer.plan_encodage` | A plan is UTF-8 text, a leading BOM tolerated; a plan in UTF-16 is rewritten in UTF-8. |
| 6018 | `lancer.plan_ligne` | In column 0, a plan line is empty, `tache <id>` within the limit of 10,000 tasks, or a known key (`depend`, `preuve`, `modele`, `relance`, `fichiers`, `critique` under a task), once only and in its place, before the indented lines; an indented line follows a task. |
| 6019 | `lancer.plan_identifiant` | A task id is a word of ASCII letters and digits, `-` and `_`, unique in the plan. |
| 6020 | `lancer.plan_dependance` | A task depends only on other tasks of the plan, without a cycle. |
| 6021 | `lancer.plan_blanc` | A plan has an objective, a proof and at least one task; each task has an objective, and a written `preuve`, `modele`, `relance` or `critique` is not blank (only `depend` may be). |
| 6022 | `lancer.base_rouge` | The wave's proof passes on the base before any hand, so that a green assembly is owed only to the tasks. |
| 6023 | `lancer.dossier_chemin_refuse` | A path that Lydite writes into a permission rule, its folder or a Claude connection folder, is absolute and is written as is: on Windows it starts with a drive letter, and its names carry no parenthesis, no `*`, `?`, `[`, `]`, `{`, `}`, no quote and no backslash. |
| 6024 | `lancer.plan_chemin` | A `fichiers` path is relative to the repository root, written with `/`, without `..`, outside `.git/` and `.claude/`; in a split plan, it has the case that git knows for it in the base. |
| 6025 | `lancer.plan_borne` | A plan weighs 1 MB at most; a split plan has 12 tasks at most, and each has `fichiers` and `modele`, with no `preuve` of its own (all play the wave's); a `fichiers` line has 100 paths at most. |
| 6026 | `lancer.plan_entete` | A split plan carries the header that Lydite asked for: the human's objective and proof, as they are. |
| 6027 | `lancer.fournisseur` | A profile that names a provider gives `api`, an https URL, and `cle`, the name of an environment variable that is set and not empty, never the key itself. |
| 6028 | `lancer.mur_refuse` | Under a required wall, the wall that replays the proof plays `true` before any hand and before the base check (srt under `mur = exige`, Lydite's native wall under `mur = natif`); otherwise nothing is launched. |
| 6029 | `lancer.issue_sans_mur` | `--issue` only runs under a profile whose wall is required, because the text of an issue comes from someone else: `mur = exige` in the profile, then `lydite accept <profile>`, or an objective typed by hand. |
| 6030 | `lancer.quota` | Under a daily quota that is set (`profils/quota`, in dollars), a hand does not start when the estimated spend of the UTC day reaches it, when the file cannot be read, or when its profile with `api` has no `prix` for its model; spending beyond it is for the human. |
| 6031 | `lancer.lecon_mal_formee` | A lesson is a `.agents/skills/<name>/SKILL.md` of 4096 bytes at most, in UTF-8: a header `name` (the folder) and `description` (one line), then `## What happened`, `## Why`, `## How it is checked`, `## Pitfall`, `## Files` (lines `- <path>`) and `## Falls when`, each once, in order, with text. |
| 6032 | `lancer.dossier_pris` | A launch holds Lydite's folder for itself until its process ends: a second launch on the same folder is refused, because they would write the same thread and read the same quota. |
| 6033 | `lancer.effort_non_permis` | A plan asks only for an effort that the profile allows (`efforts_permis`). |
| 6034 | `lancer.memoire_texte_absent` | A memory recall names a line that the thread carries, by its address `<session>/<seq>` or by its SHA-256; otherwise nothing is recalled. |
| 6035 | `lancer.memoire_empreinte_fausse` | A memory recall by hash gives 64 lowercase hexadecimal digits; any other text without `/` is refused before the thread is read. |
| 7001 | `vague.bac_impossible` | Each task has its own worktree, which git creates just before its hand; a task without a worktree is refused, and the others go on. |
| 7002 | `vague.main_active` | A task never has two active hands: a hand starts only on a task with no lease, open or expired. |
| 7003 | `vague.resultat_perime` | A result is taken only if it carries the attempt of its task's open lease; any other is stale. |
| 7004 | `vague.main_expiree` | A hand that has returned nothing when its lease runs out expires, and its task is refused without a retry. |
| 7005 | `vague.main_tuee` | A hand that Lydite killed at its deadline is not retried, and its task is refused: killing it does not prove that its whole tree is dead. |
| 7006 | `vague.claude_touche` | A hand touches no `.claude/` folder of its worktree, at any depth, except the state that Claude Code keeps there itself outside git; Lydite checks it before any retry in that worktree, and at the freeze, in what it commits as in the history it brings. A wall stop, an empty read-only ordinary file laid by the sandbox, is not a write by the hand. |
| 7007 | `vague.conflit` | A hand's work merges onto the wave's head without conflict; in conflict, it is refused without a retry. |
| 7008 | `vague.preuve_rouge` | After each merge, the wave's proof passes on the head; red, the merge is undone and the task refused without a retry. |
| 7009 | `vague.git_impossible` | The wave's git steps succeed; any other git failure refuses the task without advancing the wave's branch. |
| 7010 | `vague.assemblage_ferme` | The wave's worktree returns to its head after each failure; if it cannot, the assembly closes and nothing is merged into it any more. |
| 7011 | `vague.sautee` | A task starts only if all its dependencies are assembled; otherwise it is skipped, without a worktree. |
| 7012 | `vague.fil_injoignable` | Every step of the wave is noted in the thread before it acts; a failed note stops the starts and the merges. |
| 7013 | `vague.hors_fichiers` | A hand changes only the paths of its task's `fichiers`; any other path turns its attempt red. |
| 7014 | `vague.bac_touche` | The splitter and the reviewer write only their own file, an ordinary one that the repository does not ignore, at the root of their worktree; nothing else changes there. |
| 7015 | `vague.remarques_illisibles` | The reviewer's file follows the `remarques 2` format; otherwise its attempt is red. |
| 7016 | `vague.relecture_manquee` | The work of a critical task is pushed only once reviewed; a review that is set aside, and nothing is pushed. |
| 7017 | `vague.bloquantes_restantes` | Blocking remarks are worth a single correction; not assembled, nothing is pushed. |
| 7018 | `vague.chemin_protege` | A hand changes no path that its profile protects (proteges), neither by an edit nor by a command. |
| 7019 | `vague.plafond_atteint` | A hand that reaches its spending cap is stopped and not retried; its task is refused, and what follows is for the human. |
| 7020 | `vague.bloquante_hors_de_portee` | A blocking remark targets a file that the correction cannot touch; the wave stops before any correction and nothing is pushed. |
| 7021 | `vague.ouverture_executable` | A hand adds or changes no file that runs when the branch is next opened (.mcp.json, .vscode, .idea, a shell or git configuration file); the freeze that carries one is refused before the merge. |
| 8001 | `forge.distant_illisible` | With `--pr`, the target repository has a remote `origin` whose URL names a host, an owner and a repository. |
| 8002 | `forge.base_inconnue` | The base that `--pr` names is a branch of the remote, which Lydite fetches before the wave. |
| 8003 | `forge.gh_injoignable` | `gh` starts and says it is logged in to the remote's host before the wave. |
| 8004 | `forge.rien_a_pousser` | The wave's branch has at least one more commit than its base; otherwise nothing is pushed. |
| 8005 | `forge.publication_refusee` | Lydite publishes no change under `.github/`, no path of the workstation, no commit it did not make, and no word of the user's `garde` file; publishing those is for the human. |
| 8006 | `forge.poussee_refusee` | The wave is pushed without force, within its deadline; otherwise nothing is opened. |
| 8007 | `forge.pr_impossible` | Once the branch is pushed, `gh` opens the PR against the base and reads it back on the pushed sha; otherwise the branch stays pushed, with no PR. |
| 8008 | `forge.ci_muette` | The PR's CI gives its verdict within its deadline; a PR without CI is never taken. |
| 8009 | `forge.a_reintegrer` | Lydite retries a red CI only on a PR without conflict whose base has not moved; otherwise the base is to be merged back in. |
| 8010 | `forge.pr_changee` | While Lydite waits for its CI, the PR carries the pushed sha, is not closed, and is not merged on a red CI; otherwise it stops. |
| 8011 | `forge.ci_rouge` | A red CI is worth a single retry, with the error in context; red again, or the retry not assembled, the PR stays open and red. |
| 8012 | `forge.issue_illisible` | The issue that `--issue` names is read by `gh`, exists on the remote `origin` and is open; otherwise nothing is launched. |
| 8013 | `forge.issue_trop_longue` | The issue's title and body are UTF-8 and weigh 64 KiB at most together (65,536 bytes); otherwise nothing is launched. |
