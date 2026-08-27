# Contributing to DubRust

Thanks for taking the time! DubRust is a hobby project, so the bar is simple:
**it must build, tests must pass, and the UI must stay responsive.**

## Getting started

```powershell
git clone https://github.com/arcisq/dubrust.git
cd dubrust
cargo test
cargo run --release
```

You need a recent stable Rust toolchain and `ffmpeg` + `ffprobe` in `PATH`.

## Ground rules

- **Never block the UI thread.** All heavy work (ffmpeg, VAD, HT-Demucs, mixing) goes into
  `src/tasks.rs` worker tasks and reports back through `Event` channel messages.
- **`src/ui/` renders, it does not decide.** UI code reads state and returns actions;
  state mutation lives in `src/app.rs`.
- **No hardcoded user-facing strings.** Every visible string goes through `src/i18n.rs`
  and must be added to both `EN` and `RU`.
- **Do not bundle model weights.** Anything large is downloaded on demand into the app data
  directory (see `src/audio/demucs.rs`).
- **Keep the public API of `src/lib.rs` stable** or update `tests/` in the same commit.

## Before opening a PR

```powershell
cargo test
cargo build --release
```

Describe what you changed and how you verified it. Screenshots or a short clip are gold for
UI changes. If your change affects behaviour users can notice, add a line to `CHANGELOG.md`.

## Licensing

DubRust is licensed under **AGPL-3.0-or-later**. By contributing you agree that your
contribution is licensed under the same terms. Do not paste code from incompatible licenses,
and do not link GPL-incompatible libraries: external tools (like ffmpeg) are invoked as
separate processes on purpose.
