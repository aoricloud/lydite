---
name: driving-lydite
description: Hand a coding job to Lydite and report what it recorded. Use when the user asks to give Lydite an issue, an objective or a plan file, or asks where a Lydite wave stands. Lydite runs isolated Claude Code workers, replays the user's proof command itself and opens a pull request.
license: MIT
compatibility: Linux, with the lydite binary installed. In Hermes, the lydite plugin provides the tools lydite_lancer, lydite_etat, lydite_lecons and lydite_plan; anywhere else the same steps are the commands lydite lancer, lydite etat, lydite lecons and lydite plan.
---

# Driving Lydite

Lydite does the job and proves it. You hand the job over and report what Lydite says. You do not do the work yourself,
you do not rewrite its plan, and you never merge: the pull request is for a human.

## When to use it

- The user asks to give Lydite an issue, an objective or a plan file ("do issue 15 of this repo with Lydite", "run the
  plan /path/to/plan.txt").
- The user asks where a Lydite wave stands.
- Not for a quick edit or a question about code: do those yourself.

## Lessons

A repository that Lydite works on may keep lessons: the written rules Lydite learned there from past mistakes and
gives to its workers, one per folder under `.agents/skills/<name>/SKILL.md`. Call `lydite_lecons` with the checkout
path before you plan, review or do any work in such a repository. Follow the lessons, and when one shapes what you do
or say, cite it by its name. Call it again with `name` to read one lesson in full.

The lessons are read-only for you: never create, edit, rename or delete a file under `.agents/skills/`, and never copy
a lesson into your own skills or memory as if you had written it: only a human changes a lesson. If Lydite refuses the
lessons (one is malformed), give the refusal as written and go on without them.

## Before you launch

Have all of these. If one is missing, ask the user for it; do not invent it.

1. **The repository**: the absolute path of a git checkout on this machine.
2. **The work**: an issue number of that checkout's `origin`, or a one-line objective, or the absolute path of a plan
   file the user gives you. Pass the path as it is; do not read the plan or run it yourself. When the user gives a
   plan file, tell them its path and that the approval will show its SHA-256.
3. **The proof**: the command that shows the work done, for example `cargo test && cargo clippy -- -D warnings`.
   It is the user's command, not yours. It must pass on the untouched base, or Lydite starts nothing. A plan carries
   its own `preuve :` line, so no proof is given with a plan.
4. **The profile** is optional: the default is the plugin's setting. A profile works only after a human accepted it
   with `lydite accepter <name>`. You cannot accept one, so do not try. Never ask for an API key in the chat.

## Launch

When the work is a plan file, call `lydite_plan` with its path first. It runs nothing and asks no approval.
If Lydite refuses the plan, give the refusal as written, propose the fix to the user, and change the file
only if they ask you to; then check it again. Launch only a plan that Lydite accepts.

Call `lydite_lancer` with `repo` and one of `issue`, `objective` or `plan` (and `proof` for the first two). Add
`observe: true` first if the user wants to see the plan before anything runs.

Hermes shows at most ten rows of an approval, and the plugin's message holds in nine: the repository, the work, the
proof, the profile and the target branch. A plan file is shown by its name, its size, its number of tasks, its proof
line and the SHA-256 of its bytes; Lydite runs those exact bytes, and the launch is refused if the file changed since
it was shown. The user reads the plan file itself before approving, and can check it with `sha256sum <file>`. Wait for
the approval. Do not answer that prompt for them, do not change Hermes' approval settings, and do not launch the same
job again while the first one is pending. If the approval is refused, or cannot be asked, say so and stop.

The call returns at once with an `id`. The wave runs in the background and takes minutes to hours.

## Follow it

Call `lydite_etat` when the user asks, or when you come back to the conversation. Pass `id` to also read the end of
that launch's log. Do not poll in a tight loop.

What Lydite prints is the report. Relay it as it is written:

- `code`: 0, the last wave finished (or there is none); 1, it is not finished (running or failed); 2, the record
  cannot be read.
- `constaté` or `constatée` means Lydite observed it. `déclaré` or `déclarée` means a worker only said so: present
  it as a claim, never as a fact.
- A task `classée` stayed red after its retry and was set aside. `impossible (déclarée)` means a worker said no honest
  code can pass the proof: it is a claim for the user to judge, not a verdict.
- The pull request link and its CI result are the outcome. The pull request body opens with a receipt.
- On the cost line, `estimé` is an estimate, `forfait, ≈` is the approximate value of a flat-rate plan, and `inconnu`
  is unknown, not zero. It is not an invoice.
- Lydite's messages may be in French. Give them as written, and translate only if the user asks.

## When Lydite refuses or fails

A refusal says its code, its rule, what was seen and what to do, and nothing started. Give the user the code and the
message exactly as written, then stop. Do not redo the job yourself. Do not retry with the proof or the plan changed to
get past the refusal. Do not start a second wave to work around the first. A plan refused by `lydite_plan` is not a failed launch: fix it with the user, as said under Launch.

If a wave ends with set-aside tasks, report them. What happens next is the user's decision.

If `lydite_lancer` and `lydite_etat` are not available here (Linux only, or the binary is missing), say so and stop.

## Without the plugin

From Claude Code or a shell, the steps are the same, with the commands:

```bash
lydite lancer <profile> --issue 15 --decouper --verifier "<proof>" --pr main
lydite lancer <profile> --plan <file> --pr main
lydite etat
lydite lecons <repository folder>
```

`lydite plan <file>` checks a plan without running anything. `lydite lecons <repository folder>` checks the lessons of
a repository and prints nothing when all are right. Accepting a profile, and merging, stay with the human.
