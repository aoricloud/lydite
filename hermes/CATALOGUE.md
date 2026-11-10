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
  Hand a GitHub issue and the command that proves it done to Lydite from the chat. Sandboxed Claude Code workers do
  the job, Lydite replays your proof itself, and a pull request comes back with a receipt (cost, workers, retries,
  the commit that was reviewed and replayed, and what was not replayed). Also hands Hermes the lessons Lydite keeps
  for a repository, read-only, and has Lydite check a plan file before a launch. Disclosure: after your approval of
  each launch, starts the already-installed `lydite` binary (argument list, no shell) as a detached background
  process; Lydite runs git, gh, Claude Code workers and your proof inside its sandbox and reaches GitHub and the model
  provider of its profile; the child environment is a closed list (no Hermes key, no YOLO setting); reads the plan
  file and the lesson files you name; no telemetry.
maintainer: aoricloud
tier: community
category: automation
requires_hermes: ">=0.23.0"
docs_url: https://github.com/aoricloud/lydite/tree/main/hermes
version: "0.3.2"
image: https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/banner.png
screenshots:
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-approval.png
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-receipt.png
platforms: [linux]
known_issues:
  - "Linux only: Lydite's sandbox wall holds there."
  - "Needs the lydite binary (attested release of github.com/aoricloud/lydite), Claude Code, gh and srt installed, and a Lydite profile accepted by a human."
  - "Lydite's own commands and status lines are still in French; pull request receipts are in English."
capabilities:
  provides_tools: [lydite_lancer, lydite_etat, lydite_lecons, lydite_plan]
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
| 12. Approvals respected | `pre_tool_call` returns `approve` for every launch, showing the repository, the work, the proof, the profile and the target branch in at most nine rows; a plan is shown by its file name, its size, its number of tasks, its proof line and the SHA-256 of its bytes, not its text. Nothing bypasses a guard; no Hermes child process is spawned; no shell. `lydite_lecons` and `lydite_plan` start nothing and write nothing, so they ask no approval. |
| 13. Risky behavior disclosed | See "Disclosures" below and the README. |
| 14. Truthful metadata | `requires_hermes: ">=0.23.0"` (the release it was validated on), same in `plugin.yaml`; `version` is the pinned code's. |
| 17. No unscannable executables | No compiled binary, no `.pyc` in the plugin (a test checks the file headers); its only non-text files are the PNG images under `docs/`. The Lydite binary is **not** shipped nor downloaded: it must already be on the machine. Lydite's release binaries are built by the repository's own GitHub Actions and carry a build attestation. |

## Disclosures (for the PR description)

- **What it is.** A launcher: four tools that run the already-installed `lydite` binary, read its state, check a plan
  file and read a repository's lessons. The coding workers are Claude Code sessions that Lydite starts itself behind
  its sandbox wall, on the model of a Lydite profile a human accepted. They are not Hermes agents.
- **Shell commands.** None through a shell. One external program (`lydite`) is started with an argument list:
  `lydite lancer ...` (detached, long-running), `lydite etat` (read-only), `lydite lecons <folder>` (read-only) and
  `lydite plan <file>` (read-only). Lydite itself runs `git`, `gh`, `claude` and the user's proof command inside its
  sandbox.
- **Network.** None from the plugin. Lydite and its workers reach the model provider named in the profile and GitHub.
- **Background process.** One detached `lydite lancer` per launch (minutes to hours), at most one per Lydite folder,
  its log under the plugin's data folder; the last 20 launches are kept.
- **Stored credentials.** None stored by the plugin. The optional provider key lives in Hermes' own `.env`
  (`hermes config set`) and is handed to the Lydite process through one environment variable.
- **File reads.** The plan file named in a launch call, to fingerprint its bytes, show its name, size, task count,
  proof line and SHA-256 for approval, and copy it (mode 0600, under the plugin's data folder; Lydite reads the copy);
  the plan file named in a `lydite_plan` call, to fingerprint its bytes and count its tasks; and the lesson files of
  the repository named in a `lydite_lecons` call (`.agents/skills/*/SKILL.md`, 4096 bytes each at most, never
  following a link out of that folder). Nothing else outside the plugin's own folder.

  Read the plan file before you approve: Hermes shows its name and its SHA-256, not its text. A human can check it
  with `sha256sum <file>`.
- **Telemetry.** None.
- **Platforms.** Linux only; the tools hide themselves elsewhere.
- **Honest limit.** A worker runs with `--permission-mode acceptEdits --permission-prompts none` inside Lydite's
  wall, so what it may do is what the accepted profile lends it and the wall allows, not what a per-command prompt
  would ask. The launch itself is approved by the human in Hermes before anything starts.

## What a reviewer can run

```bash
hermes plugins validate hermes --install-deps   # at the pinned commit
python3 -m unittest discover -s hermes/tests -t hermes
```

Checked on Hermes 0.23.0 (stable): all checks green; also `hermes plugins doctor hermes --ci`.
