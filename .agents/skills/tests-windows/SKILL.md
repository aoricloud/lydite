---
name: tests-windows
description: A test also runs on Windows: normalize \r\n on both sides, put #[cfg(unix)] on a Unix-only API, and name a path the way Lydite wrote it (the runner's temp folder is short, RUNNER~1).
---

## What happened
The Windows job of the CI turned red four times (pull requests 23, 24, 28 and 29), each time on something the Linux job did not show. The Windows job stops at the first failing test binary, so each fix only revealed the next failure, one whole round later. First, a test used a Unix-only API (`std::os::unix`) without `#[cfg(unix)]`, and did not compile on Windows. Then, twice, a test assumed `\n` where the batch files that stand in for the hand in the Windows tests write `\r\n`; the second time, only one side of the comparison was normalized. Last, the fake Claude of the integration tests is a batch file that rebuilt a path with PowerShell: PowerShell returns the long form of the folder, while Lydite had written its rule with the short form of the runner's temp folder (`RUNNER~1`), and at least seven tests failed.

## Why
A hand works in a Linux sandbox, so it never runs these tests on Windows. The Windows CI is the only place they run, and each failure it finds costs a wave, a CI run and a review.

## How it is checked
The Windows job of the CI plays the whole suite; a hand cannot run it in its Linux sandbox. Before you finish, read each test you touched for `\n` assumed in the output of a process or a file, a Unix-only API without `#[cfg(unix)]`, and a path compared in another form than the one Lydite wrote.

## Pitfall
Normalizing only one side passes on Linux and fails on Windows. In a batch file, `%CD%` keeps the folder as the process received it, while PowerShell's `Get-Location` expands it to the long form. A `#[cfg(unix)]` test removes its coverage from Windows: keep a Windows-friendly check of the same behaviour where the behaviour exists there too.

## Files
- src/
- tests/

## Falls when
The Windows job is removed from the CI, or the tests stop using batch files to stand in for the hand.
