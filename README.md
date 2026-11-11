# Lydite

**Lydite runs cheap coding agents (called *hands*) on your repository, replays your proof command itself, and opens a
pull request only for what passed, with a receipt.** The agents' word is never the proof.

<p>
  <a href="https://aoricloud.github.io/givre-lab/"><img src="https://raw.githubusercontent.com/aoricloud/givre-lab/main/apercus/etape-3-planche.png" alt="Nine simulated snow crystals, from Givre Lab" height="240"></a>
  <a href="https://aoricloud.github.io/mycelium-lab/"><img src="https://raw.githubusercontent.com/aoricloud/mycelium-lab/main/apercus/etape-3-colonie.png" alt="A simulated mycelium colony, from Mycelium Lab" height="240"></a>
</p>

*Two simulators whose code was written by Lydite's hands, pull request after pull request:
[Givre Lab](https://github.com/aoricloud/givre-lab) (crystal growth) and
[Mycelium Lab](https://github.com/aoricloud/mycelium-lab). Click an image to run it in your browser.*

Lydite is the foreman that proves: it hands coding work to cheap agents, each in its own sandbox behind the same wall,
replays your proof itself, and only returns verified pull requests, with their receipt. Today its workers run in Claude
Code, on any model that speaks Anthropic's API format (Claude, DeepSeek…), with no Claude account needed for another
provider. You launch it from Claude Code or from Hermes, whatever model your Hermes runs.

The *wall* is Claude Code's sandbox (`srt`): it confines what each hand's commands can read, write and reach on the
network. A *wave* is one launch: an objective, its tasks, one pull request.

> [!NOTE]
> **Status: early.** Lydite builds itself and two public test repositories (the *benches*); nobody else has run it
> yet. It is a small Rust program with no crate dependencies. Its commands and messages are still in French (`lancer`,
> `etat`; a table below translates the words you need); English is in progress. The wall only works on Linux. A wave
> runs Claude Code sessions on your own account or provider key. Expect rough edges.

## What a Lydite pull request says

This is the body of [mycelium-lab#55](https://github.com/aoricloud/mycelium-lab/pull/55), abridged (`[…]`). Lydite
wrote it. The reviewer's lines at the end are still in French:

```text
Receipt
- Cost: $0.12 estimated
- 2 hands, 0 retries, 0 set aside
- Fil sealed at 280615 bytes: sha256 0056f995[…].
- Proof settings changed: none

mycelium lancer refuses a box too large to simulate instead of aborting (issue #53)

Proof: cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check → green

refus-boite-geante: Fix issue #53 of this repository […]

Review: 1 remark, 0 blocking.
Reviewed and replayed on commit 375275bd4ed8ae6e92cb4cac84bedec09842b385.
Not replayed (from the review):
[…]                                                    ten lines, in French; the first says that nothing replayed
                                                       shows the new test failing on the code before the fix
```

The receipt counts the hands, their retries and the tasks set aside, gives the estimated cost of the hands, and seals
the journal of the run (the *thread*; this receipt still calls it `Fil`). Three more things to notice. The proof line
is your command, run again by Lydite after the hands have finished, not by them. The commit line names the exact
commit that was replayed (and reviewed, when the plan marked a task critical): on the benches, GitHub only merges a
pull request whose head is that commit. And the last block is what the review could **not** see replayed. Lydite
prints it.

As of 9 November 2026, three days after the first one was merged, on our own repositories:

| Repository | Pull requests by Lydite, merged | Hands | Retries | Estimated cost of the hands, from the receipts |
|---|---|---|---|---|
| [lydite](https://github.com/aoricloud/lydite/pulls?q=is%3Apr+is%3Amerged) (it builds itself) | 27 | 82 | 3 | ≈ $55 on 26 receipts: an API-price equivalent, run on a Claude subscription, nothing billed per token |
| [mycelium-lab](https://github.com/aoricloud/mycelium-lab/pulls?q=is%3Apr+is%3Amerged) | 34, of which 16 carry a receipt | 92 (on those 16) | 1 (on those 16) | $4.67 (on those 16) |
| [givre-lab](https://github.com/aoricloud/givre-lab/pulls?q=is%3Apr+is%3Amerged) | 26 | 148 | 6 | $7.95 |

These figures are read from the pull request bodies. The costs are estimates (tokens counted, at a listed price), and
they count the hands only: not the session that wrote each plan, nor the CI. The benches' hands run on a low-cost model
(DeepSeek).

## What Lydite refuses to do

Each refusal has a number, says what it saw, and says what to do.

- **Lydite itself never merges.** It opens the pull request and reads its CI. Merging is your rule. (On our two
  benches, the repository's own workflow asks GitHub to auto-merge a head Lydite replayed, once the CI is green on
  Linux and Windows. On Lydite itself, a pull request is merged by hand.)
- **It never takes a hand's word.** A fact a hand only declared is shown as declared, never as observed.
- **It starts nothing if your proof is already red** on the base commit (6022): a red proof proves nothing afterwards.
- **It starts nothing under a wall that does not hold** (6028), and takes no issue as input without the wall (6029): an
  issue is text from outside.
- **It keeps a hand inside its task**: a change outside the task's list of files (7013), to a protected path or a
  lesson (7018), or to a `.claude/` folder (7006) is refused; a hand does not run `git commit` (Lydite commits).
- **It does not push a hand to cheat.** A hand that finds a test no honest code can pass says `IMPOSSIBLE: <reason>`;
  Lydite sets the task aside, replays nothing, retries nothing and merges nothing from it.
- **It does not publish a change under `.github/`** (8005), nor take a pull request whose CI stays silent (8008).
- **It never opens your Claude login file**, and puts no provider key in a profile or a pull request (see below).

What it does not do for you: a green proof is worth what your proof is worth. Lydite replays it and flags, without a
model, a test of the base that lost lines and an assertion or a tolerance that was removed or changed ("Points to
justify" in the pull request). It does not judge whether your tests are good.

## How a wave goes

1. You give an objective (or an issue) and **the command that proves the work done**.
2. A hand splits it into tasks; Lydite reads the plan itself. With `--observer` it stops here and shows the plan.
3. Each task goes to a hand in its own git worktree and branch, behind the wall, with a filtered network exit.
4. When a hand says it is done, Lydite replays your proof itself, in a new sandboxed process. Green: the work is
   merged into the wave's branch, and the proof is replayed after every merge. Red: one retry, with the error given to
   the hand, then the task is set aside.
5. The tasks a plan marks critical are reviewed by another hand, which gets the diff and the replayed proof, and must
   say what was replayed and what was not.
6. Lydite pushes the branch, opens the pull request with its receipt, and reads the CI. If the CI is red and no task
   was marked critical, one hand corrects what the CI reported, once; otherwise the pull request stays open and red
   (8011).

Everything that happens is appended to a journal outside the repository (the *thread*), sealed by its SHA-256 before
the pull request is written. `lydite etat` reads it back and recomputes every seal.

## Try it

**What it takes.** Linux on x86-64 (or WSL2 on Windows), about an hour the first time, and Claude Code sessions on
your own account or provider key: even looking at a plan runs one hand.

| | Why | How |
|---|---|---|
| `git`, and `gh` logged in | the pull request | `gh auth status`. To verify the download you need `gh` 2.67 or later; Debian 13's packaged 2.46 has no `gh attestation` (use the `.sha256` file instead) |
| [Claude Code](https://claude.com/claude-code), signed in | the hands | Lydite reads Claude Code's output in a precise form, checked on 2.1.312 to 2.1.317; another version may break it |
| Node and npm; `srt` | the wall | `sudo apt install bubblewrap socat ripgrep`, then `sudo npm install -g @anthropic-ai/sandbox-runtime`. Its current version (0.0.81) needs Node 22.12 or later; with Debian 13's Node 20, install `@anthropic-ai/sandbox-runtime@0.0.78`, the version we run. Install it outside your home folder: the wall closes your home, and an `srt` under it fails with 6028 unless a `lecture` line of the profile opens its folder. `LYDITE_SRT` names an `srt` that is not on your `PATH`. On Ubuntu 24.04 and later, see the sandbox runtime's note on AppArmor and user namespaces |
| a target repository | the work | on GitHub, with a branch you may push to, a CI that runs on pull requests, and a proof command that is green today |

**Install.** Download the archive, verify it was built by this repository's GitHub Actions, and put the binary on your
`PATH`:

```bash
gh release download --repo aoricloud/lydite --pattern 'lydite-*-linux-x86_64.tar.gz*'
gh attestation verify lydite-*-linux-x86_64.tar.gz --repo aoricloud/lydite    # or: sha256sum -c lydite-*.tar.gz.sha256
tar -xzf lydite-*-linux-x86_64.tar.gz
mkdir -p ~/.local/bin && install -m 755 lydite-*-linux-x86_64/lydite ~/.local/bin/lydite
```

Or build it: `cargo build --release` (the toolchain is pinned by `rust-toolchain.toml`).

**Check your setup.** Run `lydite doctor`: it launches no hand, writes nothing, and says for `git`, `gh`, Claude Code,
`srt` and the state folder whether they are in order. Then, from your repository once the profile is accepted, run
`lydite doctor <profile>`: it also reads the profile and plays the wall check (the only thing it writes). Every
`missing` line says what to do.

**A profile** says what the hands are lent. Write `~/.local/state/lydite/profils/first.profil` (the default state
folder; `LYDITE_DOSSIER` moves it). This one is for a Rust project:

```text
sorte = claude
modele = sonnet
relance = sonnet
# the hands that split and review use opus unless you say otherwise
decoupeur = sonnet
relecteur = sonnet
mur = exige
# the wall closes your home folder: open what your proof's tools need to read there
lecture = ~/.cargo, ~/.rustup
delai_main = 3600
delai_preuve = 300
```

A proof that downloads something (dependencies, say) also needs a `reseau` line naming the domains it may reach.

```bash
lydite accepter first     # shows the profile and accepts its fingerprint; a changed profile must be accepted again
lydite profil first       # read-only: what Lydite understood (models, efforts, wall, ceiling, timeouts)
```

**Look at a plan first.** Run this from your repository: it checks the wall, checks that your proof passes on the
base, runs one hand that splits the objective into tasks, prints the plan, and stops.

```bash
lydite lancer first "Add a test for the empty-input case of parse()" --decouper --verifier "cargo test" --observer
```

**Then run the wave** and get a pull request against `main`:

```bash
lydite lancer first "Add a test for the empty-input case of parse()" --decouper --verifier "cargo test" --pr main
lydite etat               # where the last wave stands; `lydite attendre` waits for a running one
```

Progress and the pull request's address are printed as the wave goes. `lancer` exits 0 only if every task was taken
and its proof replayed green, 1 otherwise, and 2 for a refusal before anything started.

**What it costs.** A pull request is the work of several hands. On our own repositories their estimated cost for one
pull request is $0.31 on average on DeepSeek (26 receipts on Givre Lab) and about $2.12 at Claude's API prices (26
receipts here). Each hand has a ceiling, `plafond` (20 dollars by default), enforced by Claude Code; with another
provider (`api`) it may never trigger.

**The French words you will type:**

| Word | Meaning |
|---|---|
| `lancer` | launch a wave |
| `etat` / `attendre` | status of the last wave / wait for the running one |
| `accepter` / `profil` | accept a profile / show how Lydite reads it |
| `--verifier` | your proof command |
| `--decouper` / `--observer` | let a hand split the objective into tasks / show the plan and stop |
| `--depot` | path of the repository, if you are not in it |
| `sorte`, `modele`, `relance` | profile: kind of hand, its model, the model of the retry |
| `mur = exige` / `demande` | the wall is required / only requested |
| `lecture`, `reseau` | paths the hands' commands may read, domains they may reach |
| `plafond` | the most one hand may spend, in dollars |

**Another provider.** Any API in Anthropic's format works, with no Claude account. The profile names the URL and the
*name* of the environment variable that holds your key, never the key:

```text
sorte = claude
api = https://api.deepseek.com/anthropic
cle = DEEPSEEK_API_KEY
modele = deepseek-flash
relance = deepseek-flash
decoupeur = deepseek-flash
relecteur = deepseek-flash
mur = exige
```

Set `decoupeur` and `relecteur` too: they default to `opus`, a name your provider may not know. An optional `prix`
line gives the provider's prices, so that receipts carry a cost.

**Windows.** The wall (the sandbox) is required everywhere, Windows included: a profile with no `mur` line, and the built-in `claude` profile, require it. But Claude Code's sandbox and srt do not hold under Windows, so the wall check fails there (6028) before any hand. To launch anyway, knowingly, write `mur = demande` in the profile (for the built-in profile, a file `profils/claude.profil` that repeats its text with `mur = demande`), then `lydite accepter <name>`. Under a requested wall the hand's commands may run outside Claude Code's sandbox and see the provider's key if there is one, and the proof is replayed outside the wall. Lydite also refuses `--issue` there (6029), and replays your proof through `cmd /C`. WSL2 gives you the Linux path, wall included.

## Where it stands

**Works today**, each seen in a public pull request:

- A wave from a plan, and its pull request with a receipt
  ([givre-lab#29](https://github.com/aoricloud/givre-lab/pull/29)).
- A review that judges on the replayed proof and names the commit; a merge check that refuses a head Lydite did not
  replay ([#21](https://github.com/aoricloud/lydite/pull/21), [#22](https://github.com/aoricloud/lydite/pull/22),
  [mycelium-lab#47](https://github.com/aoricloud/mycelium-lab/pull/47)).
- The honest way out, `IMPOSSIBLE` ([#6](https://github.com/aoricloud/lydite/pull/6)); protected paths and the exact
  text of a plan ([#14](https://github.com/aoricloud/lydite/pull/14)).
- The sealed thread ([#21](https://github.com/aoricloud/lydite/pull/21)); written lessons given to each hand
  ([#26](https://github.com/aoricloud/lydite/pull/26)).
- Linux and Windows builds, with binaries attested by GitHub Actions; the tests run on both in CI, against a stand-in
  for Claude Code.

**Partly there:**

- Windows: only with `mur = demande`, knowingly. The wall does not hold there: the hands' commands run outside the
  sandbox and see a provider key if there is one. No `--issue`.
- English: this README, the Hermes plugin and the labels of the pull request body. Commands, refusals and
  `lydite etat` are in French, and so are the reviewers' lines in our own pull requests.
- The Hermes plugin: Linux only.
- Providers: we run Claude and DeepSeek. The format allows others; we have not run them.
- Anyone but us: Lydite has only run on our own three repositories.

**Not there yet:** a command that checks your installation; a start guide replayed on a fresh machine; reference pages
for profiles, plans, the thread and the refusals; binaries for macOS and ARM; a judge for tests that prove nothing.

Lydite is designed so that a human decides in five cases only: merging into `main`, putting something on a real system,
spending beyond a quota, giving root, and publishing.

## Claude Code and your accounts

Lydite runs the Claude Code you installed (`claude`), as Anthropic publishes it, with its documented options: it
patches nothing and has no sign-in of its own. Each person signs in to Claude Code themselves, with their own Claude
account, their own API key or their provider's key, as they would without Lydite.

- **No Claude credential is read or kept.** Lydite only checks that Claude's login file exists, to close it to the
  agents; it never opens it. Nothing of the kind is written to a profile, to the journal or to a pull request.
- **A provider key stays yours.** A profile with `api` and `cle` names an environment variable and never holds the key.
  Lydite gives that value to the Claude Code sessions of that profile; it never reaches your proof, and Lydite never
  writes it in a profile or a pull request.
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
your proof replayed by Lydite itself, a pull request with its receipt. The plugin adds four tools and one skill:

- `lydite_lancer` starts a wave from an issue, an objective or a plan file, with your proof command. Hermes shows you
  the repository, the work, the proof and the profile, and waits for your approval before anything starts.
- `lydite_etat` reads back what Lydite recorded: tasks, proofs, pull request, CI, and the receipt (cost, hands,
  retries). A fact a worker only declared is shown as declared.
- `lydite_lecons` returns the lessons Lydite keeps for a repository, the written rules it gives to its
  workers (`.agents/skills/<name>/SKILL.md`): the list, or one lesson in full. It is read-only, and Lydite
  checks the lessons first.
- `lydite_plan` checks a plan file with Lydite before a launch (`lydite plan <file>`): whether Lydite
  accepts it, or its refusal and the fix, so that the plan is corrected with the human before the launch
  is approved. It starts nothing and costs nothing.
- The skill `lydite:driving-lydite` tells the model when to hand a job to Lydite, how to read what `lydite_etat`
  returns, and to quote a refusal and stop instead of redoing the work itself. It is in the open `SKILL.md` format, so
  Claude Code can read it too.

It needs Linux (the wall is held there only), the `lydite` binary already installed, and a profile that a human accepted
with `lydite accepter`. The plugin never accepts one and never merges. The workers are Lydite's Claude Code sessions,
never Hermes agents. Install and settings: [`hermes/README.md`](hermes/README.md).

## Reference

Until the reference pages exist, the full format of profiles, plans and the thread is at the top of
`src/coeur/profil.rs`, `src/coeur/plan.rs` and `src/coeur/fil.rs` (in French).

### One launch per state folder

The state folder (below) holds the thread, `fil.jsonl`, and the day's quota. One
`lydite lancer` (with or without `--decouper`) holds the folder alone until its process ends, CI included: a second
launch on the same folder is refused with 6032 (`lancer.dossier_pris`), and its correction says to wait for the other
one or to take another folder (`LYDITE_DOSSIER`). The waves of one launch do not refuse each other. The hold is an
operating-system file lock on `lancer.verrou` in the folder, never on the thread: a process that dies or is killed
loses it and leaves nothing to clean. `lydite etat` and `lydite accepter` do not take it. A launch that meets the
lock for an instant (the probe of `lydite attendre`, below) retries for about 200 ms before it refuses.

### Waiting for a wave

`lydite etat` returns 1 for a wave still running as well as for a red one. `lydite attendre`
(no argument) waits while a launch holds the state folder (it probes the lock for an instant, every 2 seconds, and
never keeps it), then, if the thread gained at least one note since it started, says the last wave like `lydite etat`
and exits 0 if it is carried through and green, 1 if it is red or finished without its closing seal (a killed launch,
even with no new note), 2 if the journal cannot be read or a seal is broken. If the folder was held and then freed
with no new note (a launch refused before it opened its wave), it says so on stderr and exits 2. Run just before the
launch takes the folder, it waits at most 60 seconds for a note to come; with no new note and no held folder it says
so and exits 2.

### A profile

A profile is a file, `profils/<name>.profil`, in Lydite's state folder (`$LYDITE_DOSSIER`, else
`$XDG_STATE_HOME/lydite` or `~/.local/state/lydite`; `%USERPROFILE%\.lydite` on Windows). A human writes it, then
accepts it.

The optional `proteges` key lists repository paths (files, or folders ending in `/`) that no hand may change, whatever the `fichiers` of its task, and the hand never runs `git commit`: Lydite commits its work itself. The lessons folder `.agents/skills/` is always protected, with or without the key: a hand never changes a lesson (a change there turns its attempt red, and at the freeze the task is refused with 7018), only the human does. A neighbouring path such as `.agents/other` stays free.

The optional `plafond` key caps what each hand may spend, in dollars (20 by default; a positive number with two decimals at most, for example `plafond = 7.5`). Every Claude hand is started with `--max-budget-usd <plafond>`; a hand that reaches it is stopped and not retried: its task is refused with 7019, the rest of the wave goes on, and the decision comes back to the human (raise `plafond` in the profile, accept it with `lydite accepter`, then rerun the task in a new wave, or leave it). It is a ceiling, not a budget: high enough not to hold back a normal hand, it only stops a hand that runs away, sub-agents included. For a profile that names another provider (`api`), Claude Code counts the spend at the prices it knows, possibly zero, so the ceiling may never trigger.

The optional `effort`, `effort_relance`, `effort_decoupeur` and `effort_relecteur` keys set Claude Code's effort (`low`, `medium`, `high`, `xhigh` or `max`) for the hand, its retry (also the hand that corrects a review), the splitter and the reviewer; a key becomes `--effort <value>` for that hand, and Lydite checks that Claude Code applied it. Without them Claude Code takes its own default (`medium` on 2.1.317), which `lydite profil` shows on its `effort:` row. Under `api`, Claude Code passes the effort along (measured on 2.1.317), but what the provider does with it is not measured.

The optional `efforts_permis` key lists, separated by commas, the efforts a plan may ask for its tasks (`efforts_permis = low, medium`; values among `low`, `medium`, `high`, `xhigh` and `max`). Without it any effort is allowed. A plan task whose `effort :` line (see Plans) is not in the list is refused with 6033 (`lancer.effort_non_permis`) before any hand starts and before anything is written: its proof names the task and the effort, and its correction says to pick an allowed effort in the plan, or to widen `efforts_permis` and accept the profile again. The profile's own `effort*` keys are not judged by it, and `lydite profil` shows it on its `allowed efforts:` row.

The optional `mesure` key is made for a measurement bench only, never for a real wave. It lists one or two settings, `sans-lecture` and `sans-a1`, to measure the effect of a lesson: `sans-lecture` keeps the lessons block in the system prompt but drops the reading instruction; `sans-a1` drops the two fixed sentences (honesty and perimeter) from a task hand's instruction, and keeps its list of files and everything else. Any other value is refused like any unknown profile value. The thread notes the active settings in `main.consigne.mesure`, and the receipt (the PR body and `lydite etat`) says so on a line of its own, `Measurement: <settings>.`, so a real wave is never weakened without saying it.

```bash
lydite accepter <name>        # shows the profile and accepts its fingerprint; a changed profile must be accepted again
lydite profil <name>          # read-only: loads the profile as `lancer` does and says its source, fingerprint, models, efforts, wall (required or requested), where the proof is replayed, the per-hand ceiling and the two timeouts
```

### Plans

In a plan, the repeatable task key `exact : <file> : <text>` makes Lydite check itself, byte for byte and without a
model, that the text appears in that file of the task (the file must be covered by the task's `fichiers`); a hand that
splits it or leaves it out gets a red attempt.

The optional task key `effort : <level>` (`low`, `medium`, `high`, `xhigh` or `max`; any other value refuses the plan
line) sets Claude Code's effort for both attempts of the task, in place of the profile's `effort` and `effort_relance`,
the way `modele` and `relance` replace its models. The profile must allow it (`efforts_permis`, otherwise 6033 at launch).
`lydite etat` and the Receipt of the pull request say the effort of each role of the wave on an `Effort:` line, for
example `- Effort: hand: low, high; reviewer: Claude Code's default` (a role with no declared effort shows
`Claude Code's default`).

### Lessons

A lesson is a file of the target repository, `.agents/skills/<name>/SKILL.md`, in the open skills format.
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

### The thread and its seal

- **One thread of facts.** Everything that happens is appended to a journal outside the repository, and `lydite etat`
  is read from it. A fact only declared by a hand is never shown as a fact Lydite observed. Before a hand leaves, a
  `main.consigne` line says what it received: its role, the SHA-256 of the exact text sent, the SHA-256 of its system
  prompt file, and the lessons given and set aside. Both texts are kept in `consignes/<SHA-256>.txt`.
- **A sealed thread.** Before it builds the body of its PR, and again when it ends (on every exit, published or not),
  a wave writes a `vague.sceau` line: the journal's length in bytes and the SHA-256 of those bytes. The PR receipt
  repeats the first one (`Thread sealed at <bytes> bytes: sha256 <hash>.`). `lydite etat` recomputes every seal, names
  the first one that no longer matches (its wave, its bytes) on stderr, and exits 2. A seal shows a line changed or
  removed before it. It does not show a line appended after it, nor a journal rewritten with its seals redone (the
  receipt in the PR, off the machine, keeps its own).

### Reviews

- **Plans, reviews.** An objective (or an issue) is split into tasks by a hand, and Lydite reads the plan itself;
  critical tasks are reviewed by another hand, and its blocking remarks trigger a correction. The reviewer gets, next
  to the diff, `PREUVE.lydite`: the wave's proof command, the commit where Lydite replayed it green, and its output
  as the journal keeps it. Its file (`remarques 2`) ends with two mandatory blocks, `rejoue` (what the replayed proof
  shows) and `non rejoue` (what the work claims and nothing replayed here shows: another platform, the network, a path
  no test plays); the pull request body repeats the lines of the second under `Not replayed (from the review):`.

## Develop

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
python3 -m unittest discover -s hermes/tests -t hermes     # the Hermes plugin
```

The code is in three layers (`coeur`, `branchements`, `conduite`, under `commande`); `tests/couches.rs` keeps each one
from naming the layers above it. The code and its comments are in French, like the interface.

## Why the name

Lydite is the touchstone: the black stone on which gold was rubbed to tell it from what only shines.

## License

MIT. See [LICENSE](LICENSE).
