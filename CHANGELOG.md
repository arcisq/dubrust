# Changelog

All notable changes to DubRust are documented here.
The format loosely follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the project uses [Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-08-28

First public release.

### Added

- Video dubbing workflow: open a video, auto-slice speech into phrases, record a take per
  phrase, mix and export the result with the video stream copied (no re-encode).
- FireRedVAD phrase slicer (2.3 MB, embedded, pure Rust) with an automatic DSP fallback,
  snapping, merging of fragments and smart splitting of long phrases.
- Four mix modes: dub voice only, dubbing + clean BGM (HT-Demucs), replace speech,
  voice-over with ducking.
- Optional HT-Demucs stem separation with load-dynamic ONNX Runtime: weights (~166 MB) and
  runtime are downloaded on demand with progress reporting, never bundled.
- Timeline with waveform, phrase zones, take markers, zoom (1x-50x) and playhead following.
- Focus mode: one phrase at a time with large record/listen controls.
- Original-audio monitoring while recording, so you hear the actor you are dubbing.
- Voice cleanup chain: high-pass, noise gate, normalization, optional time-fitting
  (WSOLA time-stretch without pitch shift).
- Full English and Russian UI localization with automatic system-language detection.
- Autosave, project restore, take trash with `Ctrl+Z` undo, take remapping after re-slicing.
- Windows installer (Inno Setup, EN/RU wizard): per-user install without admin rights,
  Start menu and desktop shortcuts, uninstaller entry, bundled `ffmpeg`/`ffprobe`.
- Portable build: a `portable.txt` marker keeps model weights, ONNX Runtime and settings in
  `./data` next to the executable and writes nothing to `%APPDATA%` or the registry.
- External tools are now resolved next to the executable (and in `./ffmpeg/bin`) before
  falling back to `PATH`, so shipped builds work without any system configuration.
- Static MSVC runtime linking: the binary runs on a clean Windows without
  Visual C++ Redistributable.
- `scripts/package.ps1` builds every release artifact in one go (portable archive,
  installer, `SHA256SUMS.txt`) and caches the static ffmpeg download.

### Fixed

- FireRedVAD no longer allocates the whole file as a single tensor; inference runs in
  30-second windows with warm-up overlap (no more OOM/hangs on long videos).
- Corrupted `cmvn.json` returns an error instead of panicking.
- Phrase padding is applied once (double padding used to make neighbouring phrases overlap).
- HT-Demucs overlap-add uses a COLA-correct trapezoid window: no more "breathing" background
  and no fade-out at the very start and end of the track.
- `onnxruntime.dll` initialization errors are surfaced instead of being silently cached as success.
- Take preview in "dubbing + clean BGM" mode no longer plays the original voice under your take.
- Timeline draws only the visible viewport, so zooming no longer tanks the frame rate;
  dragging the playhead no longer spawns an ffmpeg frame request per frame.

[0.1.0]: https://github.com/arcisq/dubrust/releases/tag/v0.1.0
