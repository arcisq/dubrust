# Лицензии третьих сторон

Сам DubRust распространяется под **AGPL-3.0-or-later** (текст — в файле `LICENSE`).
Здесь перечислено, что используется внутри и рядом, и почему это совместимо.

Сводка получена из `cargo metadata` по всему дереву зависимостей: 479 пакетов,
включая зависимости для других платформ (Linux/macOS/Android/WASM).

## Rust-зависимости

| Лицензия | Пакетов | Примеры |
| --- | --- | --- |
| MIT OR Apache-2.0 (и варианты записи) | ~275 | eframe, egui, epaint, serde, anyhow, image, png, rodio, tempfile, wgpu, windows-sys |
| MIT | 103 | rfd, nix, objc2-*, wayland-*, zbus, tracing |
| Apache-2.0 (только) | 20 | cpal, hound, winit, ab_glyph, glutin, oboe, claxon |
| Unicode-3.0 | 18 | icu_* (приходят через url/idna) |
| Zlib / BSD-2 / BSD-3 / ISC / BSL-1.0 / 0BSD / CC0 / Unlicense | ~30 | glow, bytemuck, slotmap, libloading, tiny-skia, ogg, walkdir |
| **MPL-2.0** | 4 | symphonia, symphonia-core, symphonia-bundle-mp3, symphonia-metadata |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | 2 | r-efi (выбираем MIT/Apache) |
| (MIT OR Apache-2.0) AND OFL-1.1 AND LicenseRef-UFL-1.0 | 1 | epaint_default_fonts (шрифты egui) |
| (Apache-2.0 OR MIT) AND BSD-3-Clause | 1 | encoding_rs |

GPL-, AGPL- и LGPL-only крейтов в дереве нет.

### Почему это совместимо с AGPL-3.0

- MIT, BSD, ISC, Zlib, BSL-1.0, CC0, Unlicense, Unicode-3.0 — permissive, включаются в AGPL-работу
  без ограничений, требуют лишь сохранения текстов лицензий и атрибуции.
- Apache-2.0 односторонне совместима с GPLv3/AGPLv3 (именно поэтому нужна версия 3,
  а не GPLv2: с GPLv2-only cpal и hound были бы несовместимы).
- MPL-2.0 (symphonia) — файловый copyleft с прямой оговоркой о совместимости с GPL/AGPL
  («Secondary License», раздел 3.3). Обязательства касаются только файлов symphonia,
  если их менять. Приходит через фичи rodio `mp3`/`vorbis`/`flac`; если их отключить,
  зависимость уйдёт совсем (приложению достаточно WAV).
- OFL-1.1 и Ubuntu Font License (шрифты в `epaint_default_fonts`) — требуют сохранения
  уведомлений о лицензиях шрифтов в дистрибутиве.

## Внешние утилиты (не входят в состав приложения)

### ffmpeg и ffprobe

Вызываются только как отдельные процессы операционной системы через `std::process::Command`
(см. `src/util.rs`, `src/video/extractor.rs`, `src/video/frames.rs`, `src/video/exporter.rs`).
Обмен идёт аргументами командной строки и потоками stdin/stdout (кадры — `image2pipe`).

- Никакой линковки с libav*: нет ни `ffmpeg-sys`, ни `ffmpeg-next`, ни статической,
  ни динамической. Вызов CLI-утилиты — агрегация, а не производное произведение.
- Сборки ffmpeg бывают LGPL-2.1-or-later или GPL-2.0-or-later (типовые Windows-сборки
  с libx264/x265 — GPL). На лицензию DubRust это не влияет.
- Если бинарник ffmpeg будет класться в дистрибутив, нужно приложить его лицензию и
  обеспечить доступ к исходникам именно той сборки. Проще — требовать установку
  ffmpeg пользователем (текущее поведение: проверка наличия в PATH при старте).
- Файлы, полученные на выходе (смонтированное видео), лицензией ffmpeg не обременены.

### Silero VAD и PyTorch

`scripts/silero_vad_segmenter.py` запускается отдельным процессом Python, результат
читается из stdout как JSON.

- Silero VAD — MIT, начиная с v4.0. **Важно:** версии до 3.1 включительно были GPL-3.0,
  поэтому при пиннинге старой ревизии условия меняются (для AGPL-3.0-проекта
  GPL-3.0-модель в отдельном процессе не создаёт проблемы, но следить стоит).
- PyTorch — BSD-3-Clause, устанавливается пользователем самостоятельно.

## Как пересобрать сводку

```powershell
cargo install cargo-about   # генерация полного списка текстов лицензий
cargo install cargo-deny    # проверка политики лицензий в CI
cargo deny check licenses
```

## Binaries redistributed in the Windows installer and portable archive

| Component | Version | License | Notes |
|---|---|---|---|
| `ffmpeg.exe`, `ffprobe.exe` | static "essentials" build from [gyan.dev](https://www.gyan.dev/ffmpeg/builds/) | GPL-3.0 | Unmodified upstream binaries. DubRust never links them: they are launched as separate processes over the command line, which is mere aggregation, not a combined work. The full license text ships as `LICENSE-ffmpeg.txt` next to the executable, and the build sources are published at the link above. |

Nothing else is bundled. HT-Demucs weights and ONNX Runtime are downloaded by the user on demand
and stay subject to their own licenses; the FireRedVAD model committed in `models/` is embedded at
compile time and covered by the entry above in this file.
