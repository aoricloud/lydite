# Security

Lydite starts coding workers on your machine and lends them limited rights behind a sandbox wall. A flaw in what it
lends, closes, refuses or replays can hurt the people who run it. Please report such a flaw privately.

## How to report

Use GitHub's private vulnerability reporting: the **Security** tab of this repository, then **Report a
vulnerability**. Only the maintainers see the report, and we answer there.

Never report a security problem in a public issue, pull request or discussion, and never paste a key, a token or a
private path into one.

Please include the release or commit you ran, your system, the profile (without any key), and the steps, plan or
command that show the problem. A short reproduction helps more than a long explanation.

## What is in scope

- A worker that gets out of its wall: reads or writes outside what its profile lends, reaches a network address its
  profile does not allow, or runs a command outside the sandbox when the wall is required (`mur = exige` or `natif`).
- A key or a token that reaches somewhere it should not: a worker's commands, the proof, a log, the thread, a pull
  request, or the Hermes plugin's answers.
- A proof, a review or a merge check that can be made to say green when it is not, or to name a commit that was not
  replayed.
- A plan, a lesson, a remark file or a pull request body that makes Lydite do something its rules forbid.
- In the Hermes plugin: a launch that starts without the human's approval, or anything that bypasses Hermes' guards.

## What is not

- Known limits written in the README, among them: on Windows the wall does not hold (`mur = demande` only,
  knowingly); outside a required wall, the workers' commands see the provider key.
- What a profile lends on purpose: a human accepts each profile, and the worker may use what it lends.
- Problems in Claude Code, `srt`, Hermes or GitHub themselves: please report those to their own projects.

## What happens next

We confirm the report, work on a fix in private, and publish it in a release with a note that credits you, unless you
prefer not to be named. This is a small project: please allow a few days for a first answer.
