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
description: >-
  Hand a GitHub issue and the command that proves it done to Lydite from the chat. Lydite runs the job in Claude Code
  sessions, on the model and provider of its profile, replays your proof itself and opens a pull request, with a
  receipt, for the work that passed. Also reads the lessons Lydite keeps for a repository, checks a plan file, and
  turns a hermes-security findings export into one plan per finding you choose: prove it by a failing test, then fix
  it. Disclosure: after your approval of each launch (it shows the repository, the work, the proof (for a plan, also
  its SHA-256), the profile and the target branch; "always" covers that exact request only), starts the installed
  `lydite` binary (argument list, no shell) as a detached background process, with a closed environment list plus the
  provider key you set; Lydite runs git, gh and Claude Code on the host with your GitHub login and reaches GitHub and
  its profile's provider; under a required wall (`mur = exige`, the default, or `natif`) the sessions' commands run in
  Claude Code's sandbox and your proof is replayed inside the wall; under a profile you accepted with `mur = demande`
  they may run outside it and see the provider key if there is one, and the proof is replayed on the host. The status,
  lessons and plan-check tools are read-only and ask no approval. Writes plans only under the plugin's data folder; no
  telemetry. The full list is in the README.
maintainer: aoricloud
tier: community
category: automation
requires_hermes: ">=0.23.0"
docs_url: https://github.com/aoricloud/lydite/tree/<sha>/hermes
version: "0.4.0"
image: https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/banner.png
screenshots:
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-approval.png
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-receipt.png
platforms: [linux]
known_issues:
  - "Linux only: Lydite's sandbox wall holds there."
  - "Needs the lydite binary of the same release as this plugin (attested release of github.com/aoricloud/lydite), Claude Code, gh and srt installed (bubblewrap and lydite-reseau instead of srt under mur = natif), and a Lydite profile accepted by a human."
  - "Some names are still French: three tool names (lydite_lancer, lydite_etat, lydite_lecons), the plugin settings (binaire, profil, cle_nom, cle) and a few lines Lydite writes. Lydite's commands have English words, and its usage and the lines about the pull request and CI are in English; pull request receipts are in English. The progress lines of a launch are still in French."
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
| 3. No self-updating code | The plugin fetches nothing and replaces no file. |
| 5. Owner submission | Submitted by the owner of the repository. |
| 6. Capabilities match | `provides_tools` and `provides_hooks` are the registered ones; `tests/test_manifeste.py` checks it on every push, and `hermes plugins validate` says "matches registrations". No `capabilities:` consent entry is needed (no override). |
| 7. Install scanner | `security scan — safe` on 0.23.0. |
| 9. No core override | `no core override — no runtime rebinds of Hermes core`. |
| 10. Dependencies | None (standard library only). |
| 11. Credentials through Hermes | `get_secret("LYDITE_CLE_FOURNISSEUR")` from `agent.secret_scope`; no `.env`, no `auth.json`; a test forbids those strings and `os.getenv` / `os.environ[`. |
| 12. Approvals respected | `pre_tool_call` returns `approve` for every launch, showing the repository, the work, the proof, the profile and the target branch in at most nine rows; a plan is shown by its file name, its size, its number of tasks, its proof line and the SHA-256 of its bytes, not its text. Nothing bypasses a guard; no Hermes child process is spawned; no shell. `lydite_lecons` and `lydite_plan` start nothing and write nothing, so they ask no approval. `lydite_findings` starts nothing and asks no approval; a plan it wrote is launched like any plan, after the human's approval, and only under a required wall, which the plugin checks with `lydite profil` (read-only). |
| 13. Risky behavior disclosed | See "Disclosures" below and the README. |
| 14. Truthful metadata | `requires_hermes: ">=0.23.0"` (the release it was validated on), same in `plugin.yaml`; `version` is the pinned code's. |
| 17. No unscannable executables | No compiled binary, no `.pyc` in the plugin (a test checks the file headers); its only non-text files are the PNG images under `docs/`. The Lydite binary is **not** shipped nor downloaded: it must already be on the machine. Lydite's release binaries are built by the repository's own GitHub Actions and carry a build attestation. |

## Disclosures (for the PR description)

- **What it is.** A launcher: five tools that run the already-installed `lydite` binary, read its state, check a plan
  file, read a repository's lessons and write plans from a findings report. The coding workers are Claude Code sessions that Lydite starts itself, behind
  its sandbox wall when the wall is required (`exige`, the default, or `natif`; see "A requested wall" below), on the model of a Lydite profile a human accepted. They are not Hermes agents.
- **Shell commands.** None through a shell. One external program (`lydite`) is started with an argument list:
  `lydite lancer ...` (detached, long-running), `lydite etat` (read-only), `lydite lecons <folder>` (read-only) and
  `lydite plan <file>` (read-only) and `lydite profil <name>` (read-only). Lydite itself runs `git`, `gh` and the
  `claude` process on the host, with the human's GitHub login; only the commands of a hand run in Claude Code's
  sandbox, and the user's proof command is replayed in the wall (by srt under `mur = exige`, by Lydite's native wall
  under `mur = natif`).
- **A requested wall.** A profile accepted with `mur = demande` leaves this: the commands of a hand may run outside
  Claude Code's sandbox and see the provider's key if there is one, and the proof is replayed on the host, outside the
  wall. A required wall (`exige`, the default, or `natif`) is required for `--issue` and for a plan written from a
  report.
- **Network.** None from the plugin. Lydite and its workers reach the model provider named in the profile and GitHub.
- **Background process.** One detached `lydite lancer` per launch (minutes to hours), at most one per Lydite folder,
  its log under the plugin's data folder; the last 20 launches are kept.
- **Stored credentials.** None stored by the plugin. The optional provider key lives in Hermes' own `.env`
  (created, set to mode 600, then filled in an editor, never on a command line, or through `hermes secrets`)
  and is handed to the Lydite process through one environment variable.
- **File reads.** The plan file named in a launch call, to fingerprint its bytes, show its name, size, task count,
  proof line and SHA-256 for approval, and copy it (mode 0600, under the plugin's data folder; Lydite reads the copy);
  the plan file named in a `lydite_plan` call, to fingerprint its bytes and count its tasks; and the lesson files of
  the repository named in a `lydite_lecons` call (`.agents/skills/*/SKILL.md`, 4096 bytes each at most, never
  following a link out of that folder); the report file named in a `lydite_findings` call (4 MiB at most) and the
  files of the repository that the report cites, of which only the existence is looked at. Nothing else outside the
  plugin's own folder.

  Read the plan file before you approve: Hermes shows its name and its SHA-256, not its text. A human can check it
  with `sha256sum <file>`.
- **File writes.** The plans written by `lydite_findings`, mode 0600, under the plugin's data folder, and nothing
  anywhere else.
- **Third-party text.** The titles and the claims of a report enter a worker's instruction, quoted as data, bounded
  and without a line break; and such a plan only runs under a required wall.
- **Telemetry.** None.
- **Platforms.** Linux only; the tools hide themselves elsewhere.
- **Honest limit.** A worker runs with `--permission-mode acceptEdits --permission-prompts none` inside Lydite's
  wall when the wall is required (`exige`, the default, or `natif`; see "A requested wall" above), so what it may do is what the accepted profile lends it and the wall allows, not what a per-command prompt
  would ask. The launch itself is approved by the human in Hermes before anything starts.

## What a reviewer can run

```bash
hermes plugins validate hermes --install-deps   # at the pinned commit
python3 -m unittest discover -s hermes/tests -t hermes
```

The validation is replayed at the pinned commit, before the entry is proposed: `hermes plugins validate` and `hermes plugins doctor hermes --ci`, on Hermes 0.23.0 (stable).
