# Contributing to Lydite

Thank you for your interest. Lydite is young and moves fast; a short issue before a large change saves everyone time.

## Build and test

Linux on x86-64 is the main platform; Windows builds and tests too, but its wall does not hold. The Rust toolchain is
pinned by `rust-toolchain.toml`, and `rustup` installs it on first use. Lydite has no crate dependency, and keeps it
that way.

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
python3 -m unittest discover -s hermes/tests -t hermes     # the Hermes plugin
ruff check hermes .github && ruff format --check hermes .github   # if Python changed (ruff 0.17.2, as in CI)
```

The tests never start a real Claude Code or a real `gh`: they use stand-ins. You do not need an account to run them.

## Before you open a pull request

- **Open an issue first** for anything larger than a small fix, so that we agree on the change before you write it.
  For a security problem, do not open an issue: see [SECURITY.md](SECURITY.md).
- **Keep it small**: one change per pull request, with a test that fails without it.
- **Language**: the code, its comments and its tests are written in French. What a user reads is moving to English:
  the README, the Hermes plugin and the pull request receipts already are; the commands and their flags are too;
  the keys of profiles and plans are still in French. Keep the language you find in the file you change.
- **Layers**: the code is in three layers (`coeur`, `branchements`, `conduite`, under `commande`); `tests/couches.rs`
  keeps each one from naming the layers above it.

## What checks a pull request

- **CI**: the tests on Linux and Windows, `clippy`, `rustfmt`, `ruff` and the plugin's tests.
- **`garde`**: looks for private names, private network addresses and personal paths in what a pull request adds
  (its files, commits, title and body). Write example values, never your own paths or addresses.
- **`tete`**: applies only to branches named `lydite/…`, which Lydite itself opens; it checks that the head is the
  commit Lydite reviewed and replayed.

A maintainer reviews and merges. Lydite never merges, and neither does any bot on `main`.

## Where it is going

The README's [Where it stands](README.md#where-it-stands) says what works, what is partly there and what is not there
yet. Issues labelled `good first issue` are a good place to start.

## Conduct

Everyone taking part follows the [Code of Conduct](CODE_OF_CONDUCT.md).
