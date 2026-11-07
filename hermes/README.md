# lydite: a Hermes plugin

A Hermes plugin that hands a job to [Lydite](https://github.com/aoricloud/lydite) from the chat: you give an issue (or
an objective), the **command that proves it done**, and a repository. Lydite splits the work, runs one isolated
Claude Code worker per task behind its sandbox wall, **replays your proof itself** on every result, keeps only what
passes, and opens a pull request that states the proof result and the review outcome. Lydite never merges: that stays a human gesture.

The program, its environment variables (`LYDITE_DOSSIER`, `LYDITE_SRT`, ...) and its state folder are all named
Lydite. The plugin's own name lives in `nom.py` and `plugin.yaml`, and `tests/test_manifeste.py` fails if the two
disagree.

## What it adds to Hermes

| Tool | What it does |
| --- | --- |
| `lydite_lancer` | Starts `lydite lancer <profile> --issue <n> --decouper --verifier "<proof>" --depot <repo> --pr main` in the background and returns at once with an id. Also takes `objectif` (instead of `issue`), `plan` (instead of both: a plan file written by hand, with its own models, files and `preuve :` line; see below), `profil`, `pr`, and `observer` (show the plan, build nothing). |
| `lydite_etat` | Returns `lydite etat` (what Lydite itself recorded: tasks, proofs, pull request, CI) and the launches made from this plugin, with the end of a launch's log when given its `id`. |

The workers are **Claude Code sessions that Lydite starts itself**, on the model and provider of the Lydite profile
(for example a DeepSeek endpoint). They are never Hermes agents or Hermes child processes.

## Requirements

- Linux (Lydite's wall holds there only; the tools hide themselves elsewhere).
- The `lydite` binary already installed on the machine, built from the Lydite repository; `git`, `claude`, and for a
  pull request `gh`; for the wall, `srt`. **This plugin ships and downloads no binary.**
- A Lydite profile **accepted beforehand by a human** (`lydite accepter <name>`). The plugin never accepts one.
- Hermes `>= 0.23.0`.

> If `lydite` is not on PATH, or if several programs could answer to that name, set the `binaire` setting to the
> absolute path of the right one.

## Install and use

Once the plugin is in the Hermes catalog: `hermes plugins install lydite`. Until then, copy this `hermes/`
folder to `~/.hermes/plugins/lydite`. Then:

```bash
hermes plugins enable lydite
hermes config set plugins.entries.lydite.settings.binaire /home/you/lydite
hermes config set plugins.entries.lydite.settings.profil  my-profile
# only if the profile names a provider key (its `cle` line); the value goes to Hermes' own .env:
hermes config set plugins.entries.lydite.settings.cle_nom MY_PROVIDER_KEY
hermes config set LYDITE_CLE_FOURNISSEUR <the provider key>
```

Then, in the chat: "do issue 15 of /home/you/project, the proof is `cargo test && cargo clippy -- -D warnings`".
Or hand it a plan you wrote: "run the plan /home/you/plan.txt on /home/you/project". A plan names the model of each
task, which an issue cannot do reliably today (Lydite's planner chooses them); `lydite plan <file>` checks one without
running anything.
Hermes shows you the repository, the work, the proof and the profile, and waits for your approval before anything
starts.

## If the model does the work itself

By default Hermes hides plugin tools behind a `tool_search` step, and a model can miss them. Name Lydite in your
request ("use Lydite to run the plan ..."), or show the tools to the model directly:

```bash
hermes config set tools.tool_search.enabled off
```

## Settings

| Setting | Meaning |
| --- | --- |
| `binaire` | Absolute path of the Lydite binary. Empty: the first `lydite` on PATH. |
| `profil` | Default Lydite profile (default `claude`, Claude Code's own login). |
| `cle_nom` | Name of the environment variable the profile's `cle` line names (e.g. a provider key). Empty: no key is passed. |
| `cle` (secret `LYDITE_CLE_FOURNISSEUR`) | The provider key itself, set through Hermes. Read with `agent.secret_scope.get_secret`, handed only to the Lydite process. |

## What it does on your machine (disclosures)

- **Commands.** One program only: the Lydite binary, started with an argument list, never through a shell. `lydite
  lancer` (in the background) and `lydite etat` (read-only, 30 s timeout). Lydite in turn runs `git`, `gh`, `claude`
  and the proof command inside its sandbox; those are Lydite's, documented in its README.
- **Network.** The plugin makes no network call itself. Lydite and its workers do: the model provider named in the
  profile, and GitHub (through `gh`) for the issue, the push and the pull request. Nothing is sent anywhere else, and
  there is no telemetry.
- **Long-running process.** `lydite_lancer` starts one detached `lydite lancer` process (its own session, so it
  survives Hermes) that can run for minutes to hours. At most one per Lydite folder from this plugin. It writes its
  output to a log file under the plugin's data folder in `HERMES_HOME` (the last 20 launches are kept).
- **Secrets.** Only through Hermes: `get_secret("LYDITE_CLE_FOURNISSEUR")`, passed in the environment of the
  Lydite process under the variable named by `cle_nom`, and in no log, argument, state or tool result. The plugin
  never reads `.env`, `auth.json` or any key file.
- **Environment.** The Lydite process gets a closed list of variables (`PATH`, `HOME`, `LANG`, the `LYDITE_*` paths of
  the binary, `GH_CONFIG_DIR`, `CLAUDE_CONFIG_DIR`, ...), not the rest of Hermes' environment.
- **Approvals.** A `pre_tool_call` hook sends every `lydite_lancer` call to Hermes' human approval, showing the exact
  repository, work, proof and profile. A request that would be refused anyway is answered without asking. "Always"
  only covers the same repository, profile and proof. Nothing here disables a guard or reads the YOLO mode; in an
  unattended run Hermes' gate fails closed.
- **Files.** Reads nothing outside its own data folder except what the Lydite binary reports, the `plan` file named in
  a call (to show it to you in full, and to copy it), and the existence of the repository's `.git`. A plan is copied
  with mode 0600 under the plugin's data folder and Lydite reads the copy: if the file changed after you saw it, the
  launch is refused.

## Limits, said plainly

- Linux only. On Windows the wall is not held by Lydite; the tools do not appear.
- Lydite's proof is only as good as your command: a test that is wrong is replayed as faithfully as one that is right.
- The pull request body today states the proof result and the review outcome; a fuller receipt (cost, models, tests
  before and after) is planned. `lydite_etat` returns what Lydite prints today.

## Test

```bash
python3 -m unittest discover -s hermes/tests -t hermes
hermes plugins validate hermes --install-deps
```

The tests use only the standard library. They load the plugin by path, with a fake `ctx` and a fake `lydite`
script; the launch tests need Linux.

## License

MIT.
