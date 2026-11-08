---
name: driving-lydite
description: Hand a coding job to Lydite and report what it recorded. Use when the user asks to give Lydite an issue, an objective or a plan file, or asks where a Lydite wave stands. Lydite runs isolated Claude Code workers, replays the user's proof command itself and opens a pull request.
license: MIT
compatibility: Linux, with the lydite binary installed. In Hermes, the lydite plugin provides the tools lydite_lancer and lydite_etat; anywhere else the same steps are the commands lydite lancer and lydite etat.
---

# Driving Lydite

Lydite does the job and proves it. You hand the job over and report what Lydite says. You do not do the work yourself,
you do not rewrite its plan, and you never merge: the pull request is for a human.

## When to use it

- The user asks to give Lydite an issue, an objective or a plan file ("do issue 15 of this repo with Lydite", "run the
  plan /path/to/plan.txt").
- The user asks where a Lydite wave stands.
- Not for a quick edit or a question about code: do those yourself.

## Before you launch

Have all of these. If one is missing, ask the user for it; do not invent it.

1. **The repository**: the absolute path of a git checkout on this machine.
2. **The work**: an issue number of that checkout's `origin`, or a one-line objective, or the absolute path of a plan
   file the user gives you. Pass the path as it is; do not read the plan or run it yourself.
3. **The proof**: the command that shows the work done, for example `cargo test && cargo clippy -- -D warnings`.
   It is the user's command, not yours. It must pass on the untouched base, or Lydite starts nothing. A plan carries
   its own `preuve :` line, so no proof is given with a plan.
4. **The profile** is optional: the default is the plugin's setting. A profile works only after a human accepted it
   with `lydite accepter <name>`. You cannot accept one, so do not try. Never ask for an API key in the chat.

## Launch

Call `lydite_lancer` with `depot` and one of `issue`, `objectif` or `plan` (and `preuve` for the first two). Add
`observer: true` first if the user wants to see the plan before anything runs.

Hermes shows the user the repository, the work, the proof and the profile, and waits for their approval. Wait for it.
Do not answer that prompt for them, do not change Hermes' approval settings, and do not launch the same job again
while the first one is pending. If the approval is refused, or cannot be asked, say so and stop.

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
get past the refusal. Do not start a second wave to work around the first.

If a wave ends with set-aside tasks, report them. What happens next is the user's decision.

If `lydite_lancer` and `lydite_etat` are not available here (Linux only, or the binary is missing), say so and stop.

## Without the plugin

From Claude Code or a shell, the steps are the same, with the commands:

```bash
lydite lancer <profile> --issue 15 --decouper --verifier "<proof>" --pr main
lydite lancer <profile> --plan <file> --pr main
lydite etat
```

`lydite plan <file>` checks a plan without running anything. Accepting a profile, and merging, stay with the human.
