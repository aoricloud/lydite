---
name: verrou-lache-pas-libre
description: Closing the file that holds a lock does not free the lock at once: a test that needs the folder free releases it with `essais::lacher`, and a test that takes the lock retries the take.
---

## What happened
Tests of the folder lock and of `lydite attendre` closed the lock file, then counted on the lock being free at the next line. A process that another test starts at that instant keeps a copy of the descriptor between `fork` and `exec`, and the `flock` lock lives as long as that copy. Under a load of processes, 174 rounds out of 200 failed on these tests; one of them made the first wave of V4 fail (refusal 7008). Pull request 47 added `essais::lacher`, which closes the file and waits until a probe sees the folder free, and made the tests that take the lock retry: 0 failures in 300 rounds under the same load.

## Why
The tests of one binary run together, and many of them start processes. A lock belongs to the system, not to the file value: it is released when the last copy of the descriptor is closed, which may be a few milliseconds after `drop`.

## How it is checked
A test that releases the lock and then needs the folder free calls `lacher(file, folder)`, never `drop` alone. A test that takes the lock right after a probe retries the take, with a bound. The reviewer looks for a lock file dropped and followed by an assertion that the folder is free.

## Pitfall
This wait is not polling the code under test: it waits for the system to finish a release, bounded at 30 s. Do not change Lydite's lock to fix a test, and do not promise "exactly one retry" while another test can start a process at the same instant.

## Files
- src/essais.rs
- src/branchements/dossier/tests.rs
- src/commande/etat/tests.rs

## Falls when
Lydite's lock is no longer held through a descriptor that child processes can inherit, or the tests stop starting processes.
