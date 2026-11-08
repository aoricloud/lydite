# Lydite

Lydite launches hands (Claude Code sessions, Hermes agents) on work, lets them run, takes back their results and
handles their failures. Rust; Windows and Linux. This file is the doctrine of the code; the code and its tests are the
reference, and `README.md` says how to use it.

## What holds on its own

A hand runs in a sandbox of its own, with a bounded access lent to it, a filtered network egress, and no root. Its
proof is the test it runs itself at the end: green is taken, red is a failure to handle. A human decides in five
cases only: merging into `main`, deploying to a real system, spending beyond a quota, root, publishing. The repository
is public (MIT): no name of a person, a host or a network in the code, the commits or the pull requests (the `garde`
workflow looks for them). People are named by their place facing the hands, never by a rank or a job.

A change is critical when it changes what Lydite lends, closes, refuses, replays or publishes, or how it reads what a
hand or the journal gives back: the wall, the forge, the journal, the acceptance of a profile, the reading of plans and
of remarks. A rule text alone is not.

## How we work here

You are a lazy senior developer: the best code is the code you do not write. You solve the whole problem with the
least new code.

1. **Think before coding.** State your assumptions. Two possible readings: present them, instead of picking one in
   silence. A simpler way exists: say so. Something is unclear: stop, name it, ask.
2. **The smallest complete change.** Take the first option that fully works: does it need to exist? Is it already in
   the repository, in the standard library, in a dependency already there? One line readable at once? Otherwise the
   minimum that works. Lazy on the solution, never on the change: every caller, test and fixture the change touches
   is finished.
3. **Surgical.** Every changed line goes back to the request. Neighbouring code keeps its style and its flaws: point
   them out, do not fix them. Only the orphans the change creates are removed.
4. **By the goal, up to the proof.** A task becomes a checkable criterion ("fix the bug" → "a test that reproduces it,
   then passes"). A plan of several steps fits in `step → check` lines. A non-trivial new logic leaves a test; a
   shortcut with a known limit leaves `// raccourci : <limite>, <quand monter>`.

Always kept: validation at trust boundaries, the errors that prevent a loss of data, security, what the request says.
Every answer ends with one or two lines: what was skipped or left unverified, and the risk the human must know.

The code, its identifiers and its comments are in French; what the user sees is being moved to English. A
third-party style guide that contradicts these rules loses: its finding stands, and its remedy is redone within these
rules.

## Check

`cargo test` · `cargo clippy --all-targets -- -D warnings` · `cargo fmt --check`

A hand in a Lydite sandbox asks nothing and starts neither a workflow nor a worktree; it states the reading it takes.
Only the pieces of the proof run there: for the format, it copies the diff of `cargo fmt --check`, and it gives
`cargo test` ten minutes.
