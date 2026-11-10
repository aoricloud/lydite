## What this changes

<!-- One or two sentences. Link the issue it answers, if there is one. -->

## How it is proved

<!-- The test that fails without this change and passes with it, and the commands you ran. -->

- [ ] `cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
- [ ] `python3 -m unittest discover -s hermes/tests -t hermes`, if `hermes/` changed
- [ ] No private name, path, address or key in the diff, the commits or this text
