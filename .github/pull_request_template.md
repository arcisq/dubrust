## What does this change?

<!-- Short description. Link an issue if there is one. -->

## How was it verified?

- [ ] `cargo test` passes
- [ ] `cargo build --release` passes
- [ ] Tried it on a real video (describe below)

## Checklist

- [ ] No blocking work on the UI thread (heavy work goes to `src/tasks.rs`)
- [ ] New user-facing strings added to both `EN` and `RU` in `src/i18n.rs`
- [ ] No model weights or binaries committed
- [ ] `CHANGELOG.md` updated if users can notice the change
- [ ] Contribution is licensed under AGPL-3.0-or-later
