# Catalog submission (draft, nothing submitted)

Prepared for `NousResearch/hermes-agent`, as one file `plugin-catalog/lydite.yaml`. **Not submitted**: it waits
for the public repository `aoricloud/lydite` to exist, and for the human's go. Rules are those of the
[catalog submission page](https://hermes-agent.nousresearch.com/docs/developer-guide/plugins/catalog-submission).

## The entry (the commit is the only placeholder)

```yaml
name: lydite
repo: https://github.com/aoricloud/lydite
sha: <the 40-character commit of the public repository that was validated>
subdir: hermes
description: Hand an issue and its proof to Lydite from the chat; the proof is replayed by Lydite and a pull request comes back stating what was proved.
maintainer: aoricloud
category: automation
requires_hermes: ">=0.23.0"
version: "0.3.1"
image: https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/banner.png
screenshots:
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-approval.png
  - https://raw.githubusercontent.com/aoricloud/lydite/<sha>/hermes/docs/screenshot-receipt.png
platforms: [linux]
capabilities:
  provides_tools: [lydite_lancer, lydite_etat, lydite_lecons, lydite_plan]
  provides_hooks: [pre_tool_call]
  provides_middleware: []
  requires_env: []
```

## The rules, one by one

| Rule | Where it stands |
| --- | --- |
| 2. Exact SHA pin | Filled in at submission, from the commit that `hermes plugins validate` passed on. |
| 3. No self-updating code | The plugin fetches nothing and replaces no file. |
| 5. Owner submission | Submitted by the owner of the repository. |
| 6. Capabilities match | `provides_tools` and `provides_hooks` are the registered ones; `tests/test_manifeste.py` checks it on every push, and `hermes plugins validate` says "matches registrations". No `capabilities:` consent entry is needed (no override). |
| 7. Install scanner | `security scan — safe` on 0.23.0. |
| 9. No core override | `no core override — no runtime rebinds of Hermes core`. |
| 10. Dependencies | None (standard library only). |
| 11. Credentials through Hermes | `get_secret("LYDITE_CLE_FOURNISSEUR")` from `agent.secret_scope`; no `.env`, no `auth.json`; a test forbids those strings and `os.getenv` / `os.environ[`. |
| 12. Approvals respected | `pre_tool_call` returns `approve` for every launch, showing the repository, the work, the proof, the profile and the target branch in at most nine rows; a plan is shown by its file name, its size, its number of tasks, its proof line and the SHA-256 of its bytes, not its text. Nothing bypasses a guard; no Hermes child process is spawned; no shell. `lydite_lecons` and `lydite_plan` start nothing and write nothing, so they ask no approval; |
| 13. Risky behavior disclosed | See "Disclosures" below and the README. |
| 14. Truthful metadata | `requires_hermes: ">=0.23.0"` (the release it was validated on), same in `plugin.yaml`; `version` is the pinned code's. |
| 17. No unscannable executables | No binary, no `.pyc` in the repository (a test checks the file headers). The Lydite binary is **not** shipped nor downloaded: it must already be on the machine. Lydite's own release binaries, when published, are built by the repository's GitHub Actions with a build attestation. |

## Disclosures (for the PR description)

- **What it is.** A launcher: four tools that run the already-installed `lydite` binary, read its state, check a plan
  file and read a repository's lessons. The coding workers are Claude Code sessions that Lydite starts itself behind its sandbox wall,
  on the model of a Lydite profile a human accepted. They are not Hermes agents.
- **Shell commands.** None through a shell. One external program (`lydite`) is started with an argument list:
  `lydite lancer ...` (detached, long-running), `lydite etat` (read-only), `lydite lecons <folder>` (read-only) and `lydite plan <file>` (read-only).
  Lydite itself runs `git`, `gh`, `claude` and the user's proof command inside its sandbox.
- **Network.** None from the plugin. Lydite and its workers reach the model provider named in the profile and GitHub.
- **Background process.** One detached `lydite lancer` per launch (minutes to hours), at most one per Lydite folder,
  its log under the plugin's data folder; the last 20 launches are kept.
- **Stored credentials.** None stored by the plugin. The optional provider key lives in Hermes' own `.env`
  (`hermes config set`) and is handed to the Lydite process through one environment variable.
- **File reads.** The plan file named in a launch call, to fingerprint its bytes, show its name, size, task count,
  proof line and SHA-256 for approval, and copy it (mode 0600, under the plugin's data folder; Lydite reads the copy),
  the plan file is also read when named in a `lydite_plan` call, to fingerprint its bytes and count its tasks, and the
  lesson files of the repository named in a `lydite_lecons` call (`.agents/skills/*/SKILL.md`, 4096 bytes each
  at most, never following a link out of that folder). Nothing else outside the plugin's own folder.

  Read the plan file before you approve: Hermes shows its name and its SHA-256, not its text. A human can check it
  with `sha256sum <file>`.
- **Telemetry.** None.
- **Platforms.** Linux only; the tools hide themselves elsewhere.
- **Honest limit.** A worker runs with `--permission-mode acceptEdits --permission-prompts none` inside Lydite's wall, so what it may do is what the
  accepted profile lends it and the wall allows, not what a per-command prompt would ask. The launch itself is
  approved by the human in Hermes before anything starts.

## What a reviewer can run

```bash
hermes plugins validate hermes --install-deps   # at the pinned commit
python3 -m unittest discover -s hermes/tests -t hermes
```

Checked on Hermes 0.23.0 (stable): all checks green; also `hermes plugins doctor hermes --ci`.
