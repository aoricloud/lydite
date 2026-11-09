# Lydite

Lydite is the foreman that proves: it hands coding work to cheap agents, each in its own sandbox behind the same wall,
replays your proof itself, and only returns verified pull requests, with their receipt. Today its workers run in Claude
Code, on any model that speaks Anthropic's API format (Claude, DeepSeek…), with no Claude account needed for another
provider. You launch it from Claude Code or from Hermes, whatever model your Hermes runs.

You give it an issue (or an objective) and **the command that proves the work done**: the agents' word is never the
proof. It is a small harness written in Rust, with no dependencies, for Linux and Windows; its workers are called
*hands*. On Windows the wall is asked for, not held (see **Windows** below).

> **Status: early.** Lydite builds itself, and runs on a public test bench, [Mycelium Lab](https://github.com/aoricloud/mycelium-lab)
> (a simulator whose code its hands write from reviewed issues). The interface is French (commands, messages, docs)
> and the source code and its comments still use the project's internal name, **Kraken**, in places. Expect
> rough edges.

## What it does

- **A box per hand.** Each task runs in its own git worktree and branch, with the rights a *profile* lends it and a
  filtered network exit. Nothing runs outside Lydite.
- **The proof is replayed by Lydite.** A hand runs your proof when it thinks it is done; Lydite runs it again, in a
  fresh box. Green: the work is taken and merged into the wave's branch, and the proof is replayed after every merge,
  so the branch only advances on green. Red: one retry with the error in context, then the task is set aside.
- **An honest way out.** A hand is told not to bend the code to a test that no honest code can pass (no forced case,
  no branch for a test, no weakened test). It says so instead, with `IMPOSSIBLE: <reason>` as the last line of its
  answer. Lydite then sets the task aside without replaying the proof or retrying (a retry would push the hand to
  bend), merges nothing from its box, and records the reason as declared, never observed. `lydite etat` shows it
  (`impossible (déclarée)`), the receipt counts such tasks next to the set-aside ones, and the public pull request
  body never quotes the hand's words.
- **A pull request, never a merge.** Lydite pushes the wave's branch, opens the pull request, reads its CI and, if it
  is red, reruns it once. The pull request body states the proof result and the review outcome, and names the commit
  that was reviewed and replayed: a reviewed wave only pushes the head its review judged, or the correction of that
  review. Merging stays a human gesture.
- **One thread of facts.** Everything that happens is appended to a journal outside the repository, and `lydite etat`
  is read from it. A fact only declared by a hand is never shown as a fact Lydite observed. Before a hand leaves, a
  `main.consigne` line says what it received: its role, the SHA-256 of the exact text sent, the SHA-256 of its system
  prompt file, and the lessons given and set aside. Both texts are kept in `consignes/<SHA-256>.txt`.
- **A sealed thread.** Before it builds the body of its PR, and again when it ends (on every exit, published or not),
  a wave writes a `vague.sceau` line: the journal's length in bytes and the SHA-256 of those bytes. The PR receipt
  repeats the first one (`Fil sealed at <bytes> bytes: sha256 <hash>.`). `lydite etat` recomputes every seal, names
  the first one that no longer matches (its wave, its bytes) on stderr, and exits 2. A seal shows a line changed or
  removed before it. It does not show a line appended after it, nor a journal rewritten with its seals redone (the
  receipt in the PR, off the machine, keeps its own).
- **Plans, reviews.** An objective (or an issue) is split into tasks by a hand, and Lydite reads the plan itself;
  critical tasks are reviewed by another hand, and its blocking remarks trigger a correction. The reviewer gets, next
  to the diff, `PREUVE.lydite`: the wave's proof command, the commit where Lydite replayed it green, and its output
  as the journal keeps it. Its file (`remarques 2`) ends with two mandatory blocks, `rejoue` (what the replayed proof
  shows) and `non rejoue` (what the work claims and nothing replayed here shows: another platform, the network, a path
  no test plays); the pull request body repeats the lines of the second under `Not replayed (from the review):`.

A human decides in five cases only: merging into `main`, putting something on a real system, spending beyond a quota,
giving root, and publishing.

## Use it

You need Git, [Claude Code](https://claude.com/claude-code) (`claude`), `gh` logged in to the host of the repository
(for pull requests), and, on Linux, `srt` (the sandbox that Claude Code uses) for a profile that requires the wall.

**Install.** Download the archive for your system from the [Releases](https://github.com/aoricloud/lydite/releases)
page, verify it was built by this repository's GitHub Actions, and put the binary on your `PATH`:

```bash
gh release download --repo aoricloud/lydite --pattern 'lydite-*-linux-x86_64.tar.gz'   # the latest release
gh attestation verify lydite-*-linux-x86_64.tar.gz --repo aoricloud/lydite
tar -xzf lydite-*-linux-x86_64.tar.gz
install -m 755 lydite-*-linux-x86_64/lydite ~/.local/bin/lydite
```

On Windows the archive is `lydite-*-windows-x86_64.zip`, with `lydite.exe` inside.

Or build it: `cargo build --release` (the toolchain is pinned by `rust-toolchain.toml`); the binary is
`target/release/lydite`.

**One launch per state folder.** The state folder (below) holds the thread, `fil.jsonl`, and the day's quota. One
`lydite lancer` (with or without `--decouper`) holds the folder alone until its process ends, CI included: a second
launch on the same folder is refused with 6032 (`lancer.dossier_pris`), and its correction says to wait for the other
one or to take another folder (`LYDITE_DOSSIER`). The waves of one launch do not refuse each other. The hold is an
operating-system file lock on `lancer.verrou` in the folder, never on the thread: a process that dies or is killed
loses it and leaves nothing to clean. `lydite etat` and `lydite accepter` do not take it. A launch that meets the
lock for an instant (the probe of `lydite attendre`, below) retries for about 200 ms before it refuses.

**Waiting for a wave.** `lydite etat` returns 1 for a wave still running as well as for a red one. `lydite attendre`
(no argument) waits while a launch holds the state folder (it probes the lock for an instant, every 2 seconds, and
never keeps it), then says the last wave like `lydite etat` and exits 0 if it is carried through and green, 1 if it is
red or finished without its closing seal (a killed launch), 2 if the journal cannot be read or a seal is broken. Run
just before the launch takes the folder, it waits at most 60 seconds for a wave to open; with no new wave and no held
folder it says so and exits 2.

**A profile** is a file, `profils/<name>.profil`, in Lydite's state folder (`$LYDITE_DOSSIER`, else
`$XDG_STATE_HOME/lydite` or `~/.local/state/lydite`; `%USERPROFILE%\.lydite` on Windows). A human writes it, then
accepts it:

```text
sorte = claude
modele = sonnet
relance = sonnet
mur = exige
delai_main = 3600
delai_preuve = 300
```

The optional `proteges` key lists repository paths (files, or folders ending in `/`) that no hand may change, whatever the `fichiers` of its task, and the hand never runs `git commit`: Lydite commits its work itself. The lessons folder `.agents/skills/` is always protected, with or without the key: a hand never changes a lesson (a change there turns its attempt red, and at the freeze the task is refused with 7018), only the human does. A neighbouring path such as `.agents/other` stays free.

The optional `mesure` key is made for a measurement bench only, never for a real wave. It lists one or two settings, `sans-lecture` and `sans-a1`, to measure the effect of a lesson: `sans-lecture` keeps the lessons block in the system prompt but drops the reading instruction; `sans-a1` drops the two fixed sentences (honesty and perimeter) from a task hand's instruction, and keeps its list of files and everything else. Any other value is refused like any unknown profile value. The thread notes the active settings in `main.consigne.mesure`, and the receipt (the PR body and `lydite etat`) says so on a line of its own, `Measurement: <settings>.`, so a real wave is never weakened without saying it.

```bash
lydite accepter <name>        # shows the profile and accepts its fingerprint; a changed profile must be accepted again
```

**Run a wave** on an issue of the repository's `origin`, with your proof, from the repository (or with `--depot`):

```bash
lydite lancer <name> --issue 15 --decouper --verifier "cargo test && cargo clippy -- -D warnings" --pr main
lydite lancer <name> "an objective on one line" --decouper --verifier "<proof>" --observer   # show the plan only
lydite etat                                                                                    # where the last wave stands
lydite attendre                                                                                # wait for the running launch, then say it
```

`lancer` exits 0 only if everything was taken and observed, 1 otherwise, and 2 for a refusal (nothing started). A
refusal always says its code, its rule, what was seen and what to do. The full format of profiles, plans and the
journal is at the top of `src/coeur/profil.rs`, `src/coeur/plan.rs` and `src/coeur/fil.rs`.

In a plan, the repeatable task key `exact : <file> : <text>` makes Lydite check itself, byte for byte and without a
model, that the text appears in that file of the task (the file must be covered by the task's `fichiers`); a hand that
splits it or leaves it out gets a red attempt.

**Lessons.** A lesson is a file of the target repository, `.agents/skills/<name>/SKILL.md`, in the open skills format.
It starts with a header of exactly `name: <name>` (the folder's name: `a-z`, digits and `-`, 1 to 64 characters) and
`description: <the rule>` (one line, 1 to 200 characters), between two `---` lines. Then six sections, each once and in
this order, each with text: `## What happened`, `## Why`, `## How it is checked`, `## Pitfall`, `## Files` (zero or
more `- <path>` lines, written like a plan's `fichiers`; none means a general lesson) and `## Falls when`. UTF-8, 4096
bytes at most. `lydite lecons <repository folder>` reads every lesson of a repository: it says nothing and exits 0 if
all are right, otherwise it refuses (6031) with the file and the line of the first wrong one. A lesson is written by the
session or the pilot, in the pull request that fixes the mistake, never by a hand.

At the start of a wave Lydite reads the lessons of the base commit with git (a wrong one refuses the wave before any
hand), and gives each task hand (a plan task, the correction, the CI retry) one to three of them: first those whose
`## Files` and the task's `fichiers` cover each other, then the general ones, each group by name; a hand without
`fichiers` gets only the general ones, and a bare hand none. They go into the system prompt, in the file
`<Lydite folder>/consignes/<SHA-256 of the text>.txt` passed by `--append-system-prompt-file`; the splitter and the
reviewer get the list of all lessons, one line each, the same way.

**Windows.** The wall (the sandbox) is not held under Windows: a profile asks for it there, it does not require it.

## Claude Code and your accounts

Lydite runs the Claude Code you installed (`claude`), as Anthropic publishes it, with its documented options: it
patches nothing and has no sign-in of its own. Each person signs in to Claude Code themselves, with their own Claude
account, their own API key or their provider's key, as they would without Lydite.

- **No Claude credential is read or kept.** Lydite only checks that Claude's login file exists, to close it to the
  agents; it never opens it. Nothing of the kind is written to a profile, to the journal or to a pull request.
- **A provider key stays yours.** A profile with `api` and `cle` names an environment variable and never holds the key.
  Lydite gives that value to the Claude Code sessions of that profile; it never reaches your proof, and Lydite never
  writes it down.
- **What the agents inherit.** Your `ANTHROPIC_*` variables and `CLAUDE_CONFIG_DIR` reach them (with a provider
  profile, its URL and key replace `ANTHROPIC_BASE_URL` and `ANTHROPIC_API_KEY`, and `ANTHROPIC_AUTH_TOKEN` is
  dropped). Other `CLAUDE*` variables (a `CLAUDE_CODE_OAUTH_TOKEN`, say), GitHub tokens and SSH agents are not passed
  on: an agent is never lent a right to act as you. Sign in with `claude` itself, or use an API key. The agent keeps
  the key to reach its model, but the commands it runs in the sandbox do not see `ANTHROPIC_API_KEY` or
  `ANTHROPIC_AUTH_TOKEN`.
- **Subscriptions.** Anthropic's advertised limits for the Pro, Max and Ultra plans assume ordinary, individual use of
  Claude Code, and a wave runs many sessions: for large waves, prefer an API key or a provider profile.

Lydite is an independent project, not made or endorsed by Anthropic. "Claude Code" and "Anthropic" only name what
Lydite runs; see Anthropic's [legal and compliance page](https://code.claude.com/docs/en/legal-and-compliance).

## From Hermes

[`hermes/`](hermes/README.md) is a [Hermes Agent](https://hermes-agent.nousresearch.com) plugin. You talk to Hermes,
Hermes hands the job to Lydite, and Lydite does what it does anywhere else: its own Claude Code workers behind the wall,
your proof replayed by Lydite itself, a pull request with its receipt. The plugin adds two tools and one skill:

- `lydite_lancer` starts a wave from an issue, an objective or a plan file, with your proof command. Hermes shows you
  the repository, the work, the proof and the profile, and waits for your approval before anything starts.
- `lydite_etat` reads back what Lydite recorded: tasks, proofs, pull request, CI, and the receipt (cost, hands,
  retries). A fact a worker only declared is shown as declared.
- The skill `lydite:driving-lydite` tells the model when to hand a job to Lydite, how to read what `lydite_etat`
  returns, and to quote a refusal and stop instead of redoing the work itself. It is in the open `SKILL.md` format, so
  Claude Code can read it too.

It needs Linux (the wall is held there only), the `lydite` binary already installed, and a profile that a human accepted
with `lydite accepter`. The plugin never accepts one and never merges. The workers are Lydite's Claude Code sessions,
never Hermes agents. Install and settings: [`hermes/README.md`](hermes/README.md).

## Develop

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
python3 -m unittest discover -s hermes/tests -t hermes     # the Hermes plugin
```

The code is in three layers (`coeur`, `branchements`, `conduite`, under `commande`); `tests/couches.rs` keeps each one
from naming the layers above it. The code and its comments are in French, like the interface.

## License

MIT. See [LICENSE](LICENSE).
