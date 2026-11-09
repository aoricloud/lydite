---
name: tests-sans-course
description: A test's result never depends on timing or on the order of two threads (sleep, polling, a watcher thread): split out the step that reacts and test it on a prepared state.
---

## What happened
A test written by a correction, `une_vague_ouverte_puis_refusee_finit_par_un_sceau`, ran two threads: one made the log read-only while the wave wrote a 16 MB opening, so that a later note would fail. It passed in the CI. It failed when another wave loaded the same machine: the proof replayed for the next wave failed on it (pull request 26), the merge of one task was refused and two more tasks were skipped. The test was replaced by a test of the isolated step on a prepared log, with no thread and no wait (pull request 27).

## Why
A proof that depends on timing or on the scheduler passes or fails by chance. Lydite replays the proof after every merge, often beside other waves on the same machine, so one chance failure stops a whole wave.

## How it is checked
Before you finish, read each test you added for `sleep`, polling loops, threads that race the code under test, and timeouts used as assertions; the reviewer reads them too. The proof of the next wave replays the test under load.

## Pitfall
A generous timeout does not make a race safe, it only makes it rarer. If the behaviour needs a failure at a precise moment, split out the step that reacts to the failure and call it on a prepared state. A test whose subject is a timeout may wait: keep the wait short and the margin large.

## Files
- src/
- tests/

## Falls when
The tests run on a deterministic scheduler and clock, so that no test can depend on the machine's load.
