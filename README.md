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
  is red, reruns it once. The pull request body states the proof result and the review outcome. Merging stays a human
  gesture.
- **One thread of facts.** Everything that happens is appended to a journal outside the repository, and `lydite etat`
  is read from it. A fact only declared by a hand is never shown as a fact Lydite observed.
- **Plans, reviews.** An objective (or an issue) is split into tasks by a hand, and Lydite reads the plan itself;
  critical tasks are reviewed by another hand, and its blocking remarks trigger a correction.

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

The optional `proteges` key lists repository paths (files, or folders ending in `/`) that no hand may change, whatever the `fichiers` of its task, and the hand never runs `git commit`: Lydite commits its work itself.

```bash
lydite accepter <name>        # shows the profile and accepts its fingerprint; a changed profile must be accepted again
```

**Run a wave** on an issue of the repository's `origin`, with your proof, from the repository (or with `--depot`):

```bash
lydite lancer <name> --issue 15 --decouper --verifier "cargo test && cargo clippy -- -D warnings" --pr main
lydite lancer <name> "an objective on one line" --decouper --verifier "<proof>" --observer   # show the plan only
lydite etat                                                                                    # where the last wave stands
```

`lancer` exits 0 only if everything was taken and observed, 1 otherwise, and 2 for a refusal (nothing started). A
refusal always says its code, its rule, what was seen and what to do. The full format of profiles, plans and the
journal is at the top of `src/coeur/profil.rs`, `src/coeur/plan.rs` and `src/coeur/fil.rs`.

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
  on: an agent is never lent a right to act as you. Sign in with `claude` itself, or use an API key.
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
