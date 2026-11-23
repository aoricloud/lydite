# The Hermes catalog entry

The entry for `NousResearch/hermes-agent`, as one file `plugin-catalog/lydite.yaml`. `<sha>` stands for the
40-character commit of the release the entry pins, here and in the image addresses; `version` is that release's.
Rules are those of the
[catalog submission page](https://hermes-agent.nousresearch.com/docs/developer-guide/plugins/catalog-submission).

## The entry

```yaml
name: lydite
title: Lydite
repo: https://github.com/aoricloud/lydite
sha: <sha>
subdir: hermes
description: "Hand a GitHub issue and the command that proves it done to Lydite from the chat. Lydite runs the job in Claude Code sessions, on the models its plan and its profile name, at the profile's provider (a profile can send its reviewer to Anthropic instead), replays your proof itself and opens a pull request, with a receipt, for the work that passed. Also reads the lessons kept for a repository, checks a plan file, and turns a hermes-security findings export into one plan per finding you choose: prove it by a failing test, then fix it. Disclosure: every launch goes to Hermes' human approval (it shows the repository, the work or, for a plan, its file name, size, task count, origin and SHA-256, the proof, and the profile with the target branch, `plan only` in a dry run; long values are cut with an ellipsis, the repository and a plan's proof line included; \"always\" covers only the same request: repository, profile, issue number, objective, target branch, dry run or launch, and proof or plan bytes; Hermes itself skips the prompt in yolo mode, with `approvals.mode: off`, in a `hermes -z` one-shot, in an unattended run set to approve, and for a request you already allowed), then the plugin starts the installed `lydite` binary (argument list, no shell) as a detached background process, with a closed environment list plus the provider key you set; Lydite runs git and gh on the host with your gh login and your git credentials, and Claude Code under your Claude Code login or, when the profile names a key, with that provider key; it replays your proof with `sh -c`, reaches GitHub and its profile's provider, plus whatever the accepted profile opens (`reseau` domains, tools such as WebSearch, MCP servers), pushes a branch and opens the pull request (a dry run only shows the plan), then waits up to 30 minutes for its CI and may run one more wave, not reviewed, on that branch if it is red, unless a task of the wave is critical; under a required wall (`mur = exige`, the default, or `natif`) the sessions' commands run in Claude Code's sandbox and your proof is replayed inside the wall; under a profile you accepted with `mur = demande` (an objective or a hand-written plan only) they may run outside it and see the provider key if there is one, and the proof is replayed on the host. The status, lessons, plan-check and findings tools start no wave and ask no approval: they run `lydite` read-only, and the findings tool writes the plans of the findings you choose. Reads the plan file, the repository's `.agents/skills` lessons and the findings report you name (and, for existence only, the repository's `.git` and the files a report cites). The plugin writes only under its data folder (the one Hermes gives it, or `<temporary folder>/lydite` without one): plans, launch logs, copies of launched plans; it keeps its last launches in its Hermes plugin state. The plugin and Lydite send no telemetry; Claude Code, gh and git are separate programs with their own settings. The full list is in the README."
maintainer: aoricloud
tier: community
category: automation
requires_hermes: ">=0.23.0"
docs_url: https://github.com/aoricloud/lydite/tree/<sha>/hermes
version: "0.5.1"
image: https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/banner.png
screenshots:
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-approval.png
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-receipt.png
platforms: [linux]
known_issues:
  - "Linux only: the plugin's tools appear on Linux only, and Lydite's wall has been run on Linux only."
  - "Needs the lydite binary of the same release as this plugin (attested release of github.com/aoricloud/lydite; for lydite_findings, a binary that has `lydite findings`, newer than 0.4.0), Claude Code, gh and srt installed (bubblewrap and lydite-reseau instead of srt under mur = natif), and a Lydite profile accepted by a human."
  - "A plan that places files (`place` lines) cannot be launched through the plugin: lydite_lancer and lydite_plan refuse it before anything starts (the plugin launches a copy of the plan without its `.files` folder). Launch such a plan with `lydite launch` from a shell."
  - "Some names are still French: three tool names (lydite_lancer, lydite_etat, lydite_lecons), the plugin settings (binaire, profil, cle_nom, cle), the keys of profiles and plans, and a few lines Lydite writes. Lydite's commands have English words, and its usage and the lines about the pull request and CI are in English; pull request receipts are in English. The progress lines of a launch are still in French."
capabilities:
  provides_tools: [lydite_lancer, lydite_etat, lydite_lecons, lydite_plan, lydite_findings]
  provides_hooks: [pre_tool_call]
  provides_middleware: []
  requires_env: []
```

## The rules, one by one

| Rule | Where it stands |
| --- | --- |
| 2. Exact SHA pin | `sha` is the commit of a release tag, the one `hermes plugins validate` passed on. |
| 3. No self-updating code | The plugin fetches nothing and replaces none of its own files (it only rewrites, under its data folder, a plan file it wrote itself). |
| 5. Owner submission | Submitted by the owner of the repository. |
| 6. Capabilities match | `provides_tools` and `provides_hooks` are the registered ones; `tests/test_manifeste.py` checks it in CI, on every pull request or push to `main` that touches `hermes/`, and `hermes plugins validate` says "matches registrations". No `capabilities:` consent entry is needed (no override). |
| 7. Install scanner | `security scan — safe` on 0.23.0. |
| 9. No core override | `no core override — no runtime rebinds of Hermes core`. |
| 10. Dependencies | None (standard library only). |
| 11. Credentials through Hermes | The key is read only through `get_secret("LYDITE_CLE_FOURNISSEUR")` from `agent.secret_scope`; the plugin also reads a closed list of variables that are not secrets (`ENV_CONSERVE`: `PATH`, `HOME`, the `XDG_*` folders and so on), to pass them to Lydite; no `.env`, no `auth.json`; a test forbids those strings and `os.getenv` / `os.environ[`. |
| 12. Approvals respected | `pre_tool_call` returns `approve` for every launch, showing the repository, the work, the proof, the profile and the target branch in at most nine rows (a value too long for its rows is cut with a `…`); a plan is shown by its file name, its size, its number of tasks, its proof line, its origin and the SHA-256 of its bytes, not its text. A plan that places files (`place` lines) is refused before the prompt, so the human is never asked about a launch that cannot go. Nothing bypasses a guard. Hermes itself skips the prompt in yolo mode, with `approvals.mode: off`, in a `hermes -z` one-shot, in an unattended run set to approve, and for a request already allowed (read in the code of Hermes 0.23.3, outside this repository). No Hermes agent or subagent is spawned (the plugin starts only `lydite`, and Lydite starts the workers); no shell. `lydite_lecons` and `lydite_plan` start no wave, write nothing and run the binary read-only (`lydite lecons`, `lydite plan`), so they ask no approval. `lydite_findings` runs the Lydite binary (`lydite findings`, read-only) and nothing else: it starts no wave and asks no approval; a plan it wrote carries its origin (a `report` line) and is launched like any plan, after the human's approval; Lydite itself refuses a plan that carries that line unless the profile requires the wall (6037). A plan whose line was removed, or written by hand from a report, goes as a plan written by hand, and the human's reading of the plan before approving is what holds; the approval message says the origin (`report <finding id>` or `none (written by hand)`); for an issue or an objective with `observe` (refused with a plan), its branch row says `plan only`, and the hand that splits the work still runs. |
| 13. Risky behavior disclosed | See "Disclosures" below and the README. |
| 14. Truthful metadata | `requires_hermes: ">=0.23.0"` (the release it was validated on), same in `plugin.yaml`; `version` is the pinned code's. |
| 17. No unscannable executables | No compiled binary, no `.pyc` in the plugin (a test checks the file headers); its only non-text files are the PNG images under `docs/`. The Lydite binary is **not** shipped nor downloaded: it must already be on the machine. Lydite's release binaries are built by the repository's own GitHub Actions and carry a build attestation. |

## Disclosures (for the PR description)

- **What it is.** A launcher: five tools that run the already-installed `lydite` binary, read its state, check a plan
  file, read a repository's lessons and write plans from a findings report. The coding workers are Claude Code sessions that Lydite starts itself, on the models that the plan and a Lydite profile a human accepted name, at the profile's provider (a profile can send its reviewer to Anthropic). Each session runs on the machine; only its commands are in Claude Code's sandbox, under a required wall (`exige`, the default, or `natif`; see "A requested wall" below). They are not Hermes agents. The plugin also registers one skill, `lydite:driving-lydite` (Linux only), whose text is `hermes/skills/driving-lydite/SKILL.md` and which Hermes gives to its model.
- **Shell commands.** None through a shell. One external program (`lydite`) is started with an argument list:
  `lydite lancer ...` (detached, long-running), `lydite etat` (read-only), `lydite lecons <folder>` (read-only) and
  `lydite plan <file>` (read-only) and `lydite findings <report>` (read-only, and with `--plan <id> --verify <proof>`
  to write a plan on its standard output; it needs a binary newer than 0.4.0). Lydite itself runs `git`, `gh` and the
  `claude` process on the host, with the human's gh login and git credentials; only the commands of a hand run in Claude Code's
  sandbox, and the user's proof command is replayed in the wall (by srt under `mur = exige`, by Lydite's native wall
  under `mur = natif`).
- **A requested wall.** A profile accepted with `mur = demande` leaves this: the commands of a hand may run outside
  Claude Code's sandbox and see the provider's key if there is one, and the proof is replayed on the host, outside the
  wall. A required wall (`exige`, the default, or `natif`) is required for `--issue` and, by Lydite's refusal 6037,
  for a plan that carries the `report` line that `lydite_findings` writes; a plan without that line is not held to it.
- **Network.** None from the plugin. Lydite and its workers reach GitHub, the model provider named in the profile (and
  Anthropic, for a reviewer under `api_relecteur = anthropic`), and whatever the accepted profile opens (the domains of
  `reseau`, tools such as WebSearch, MCP servers).
- **Background process.** One detached `lydite lancer` per launch (minutes to hours), at most one per Lydite folder,
  its log under the plugin's data folder (the one Hermes gives the plugin, or, without it, `<temporary folder>/lydite`);
  the last 20 launches are kept. Unless it is a dry run, once the branch is pushed and the pull request opened, that
  process waits for its CI, 30 minutes at most. If the CI is red it runs one more wave, one only, not reviewed, on that branch; it runs
  none when a task of the wave is critical.
- **Stored credentials.** None stored by the plugin. The optional provider key lives in Hermes' own `.env`
  (created, set to mode 600, then filled in an editor, never on a command line, or through `hermes secrets`)
  and is handed to the Lydite process through one environment variable.
- **File reads.** The plan file named in a launch call, to fingerprint its bytes, show its name, size, task count,
  proof line and SHA-256 for approval, and copy it (mode 0600, under the plugin's data folder; Lydite reads the copy);
  the plan file named in a `lydite_plan` call, to fingerprint its bytes and count its tasks; and the lesson files of
  the repository named in a `lydite_lecons` call (`.agents/skills/*/SKILL.md`, 4096 bytes each at most, never
  following a link out of that folder); the report file named in a `lydite_findings` call (4 MiB at most) and the
  files of the repository that the report cites, of which only the existence is looked at (a finding that cites a file
  which is not in the repository is set aside); and `/proc/<pid>/cmdline` of a launch it made, to tell whether that
  launch still runs. It also looks, for existence only, at the `.git` of the repository named in a call, and searches
  for the `lydite` binary (the `binaire` setting, or the `PATH`). Nothing else outside the plugin's own folder.

  Read the plan file before you approve: Hermes shows its name and its SHA-256, not its text. A human can check it
  with `sha256sum <file>`.
- **File writes.** Only under the plugin's data folder (the one Hermes gives the plugin, or `<temporary folder>/lydite`),
  in mode 0600: the plans written by `lydite_findings`, the log of each launch and the copy of each plan launched; and,
  in its Hermes plugin state, its last launches. Nothing anywhere else.
- **Third-party text.** The titles and the claims of a report enter a worker's instruction, quoted as data, bounded
  and without a line break. Lydite refuses, unless the wall is required (6037), a plan that carries the `report` line
  that `lydite_findings` writes; a plan whose line was removed, or written by hand from a report, goes as a plan
  written by hand, and the human's reading of the plan before approving is what holds.
- **Telemetry.** None from the plugin or Lydite; Claude Code, gh and git are separate programs with their own settings.
- **Platforms.** Linux only; the tools hide themselves elsewhere.
- **Honest limit.** A Claude Code worker runs with `--permission-mode acceptEdits --permission-prompts none`; its Claude Code
  session runs on the machine, and when the wall is required (`exige`, the default, or `natif`; see "A requested wall" above) its commands run in Claude Code's sandbox. So what it may do is what the accepted profile lends it and the wall allows, not what a per-command prompt
  would ask. The launch itself is approved by the human in Hermes before anything starts, unless Hermes skips the prompt (see rule 12).

## What a reviewer can run

```bash
hermes plugins validate hermes --install-deps   # at the pinned commit
python3 -m unittest discover -s hermes/tests -t hermes
```

The validation is replayed at the pinned commit, before the entry is proposed: `hermes plugins validate` and `hermes plugins doctor hermes --ci`, on Hermes 0.23.0 (stable).
