---
name: assertion-qui-peut-echouer
description: Each assertion must be able to fail: look where the code writes, match a typed field rather than a whole debug dump, and check that a filter can match something.
---

## What happened
Reviews of pull requests 21, 23, 28 and 29 found six tests that show nothing about the defect they aim at. A test expected an error that began with a different text than the one the code returns. A test checked that a file did not exist in a folder where the code never writes it, so it always passed. A test searched the digits `6031` in the whole debug text of a log that holds hashes and ids, which can carry those digits by chance. A rewritten check looked for the code only in its numeric field, while a refusal would have come out under another code with `6031` only in its text, so the assertion could no longer fail. A test name promised a refusal at the freeze while the test showed only a red attempt. A requested test did not exist at all (pull request 23).

## Why
A test that cannot fail makes the proof green and hides the defect. Three of the six blocking remarks of pull requests 21 to 30 were tests that show nothing, and each cost a correction round.

## How it is checked
For each new assertion, name the change to the code that would make it fail. Make that change in a file of your task, run the test and see it red, then undo it and check `git diff`. If nothing fails, the assertion is empty. The reviewer asks the same question and cites the line.

## Pitfall
A negative assertion (`not exists`, `!contains`, `is_empty`) passes on a wrong place or a wrong spelling: check once that the same lookup finds the thing when it is there. A test name says what the test shows, no more.

## Files
- src/
- tests/

## Falls when
Lydite measures the strength of a proof by mutation and reports an empty assertion by itself.
