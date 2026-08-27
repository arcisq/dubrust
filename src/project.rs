use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::models::{DubMode, MixConfig, PhraseSegment, SlicerConfig};

pub const PROJECT_VERSION: u32 = 1;

/// Содержимое project.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectData {
    #[serde(default)]
    pub version: u32,
    pub video_path: PathBuf,
    #[serde(default)]
    pub segments: Vec<PhraseSegment>,
    #[serde(default)]
    pub dub_mode: DubMode,
    #[serde(default)]
    pub slicer_config: SlicerConfig,
    #[serde(default)]
    pub mix: MixConfig,
}

impl ProjectData {
    pub fn new(video_path: &Path) -> Self {
        Self {
            version: PROJECT_VERSION,
            video_path: video_path.to_path_buf(),
            segments: Vec::new(),
            dub_mode: DubMode::default(),
            slicer_config: SlicerConfig::default(),
            mix: MixConfig::default(),
        }
    }
}

/// Раскладка файлов проекта.
/// Дубли лежат рядом с видео, а не в %TEMP%: раньше их вытирала система
/// и вся работа пропадала между запусками.
#[derive(Debug, Clone)]
pub struct ProjectPaths {
    pub root: PathBuf,
    pub takes_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub audio_path: PathBuf,
    pub background_path: PathBuf,
    pub project_file: PathBuf,
}

fn is_writable_dir(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }
    let probe = path.join(".dubrust_write_test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

impl ProjectPaths {
    pub fn for_video(video_path: &Path) -> Self {
        let abs_video = if video_path.is_absolute() {
            video_path.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(video_path)
        } else {
            video_path.to_path_buf()
        };

        let stem = abs_video
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("project");
        let folder = format!("{stem}.dubrust");

        let near_video = abs_video.parent().map(|parent| parent.join(&folder));
        let root = match near_video {
            Some(candidate) if is_writable_dir(&candidate) => candidate,
            _ => std::env::temp_dir().join("dubrust").join(&folder),
        };

        let takes_dir = root.join("takes");

        Self {
            trash_dir: takes_dir.join(".trash"),
            audio_path: root.join("audio.wav"),
            background_path: root.join("background.wav"),
            project_file: root.join("project.json"),
            takes_dir,
            root,
        }
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.takes_dir)
            .with_context(|| format!("Не удалось создать папку проекта {:?}", self.takes_dir))?;
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.project_file.is_file()
    }

    /// Уникальное имя для нового дубля.
    /// Раньше имя зависело только от номера фразы, и перезапись ломала уже
    /// загруженный в плеер файл.
    pub fn unique_take_path(&self, segment_id: usize) -> PathBuf {
        for attempt in 1..=9999 {
            let candidate = self
                .takes_dir
                .join(format!("take_{segment_id:03}_{attempt:02}.wav"));
            if !candidate.exists() {
                return candidate;
            }
        }
        self.takes_dir.join(format!("take_{segment_id:03}_last.wav"))
    }

    /// Переместить дубль в корзину и вернуть новый путь — для отмены удаления
    pub fn move_to_trash(&self, take_path: &Path) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.trash_dir)?;

        let name = take_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("take.wav")
            .to_string();

        let mut target = self.trash_dir.join(&name);
        let mut attempt = 1;
        while target.exists() {
            target = self.trash_dir.join(format!("{attempt:02}_{name}"));
            attempt += 1;
        }

        std::fs::rename(take_path, &target)
            .with_context(|| format!("Не удалось удалить дубль {take_path:?}"))?;

        Ok(target)
    }

    /// Вернуть дубль из корзины
    pub fn restore_from_trash(&self, trashed: &Path, segment_id: usize) -> Result<PathBuf> {
        self.ensure_dirs()?;
        let target = self.unique_take_path(segment_id);
        std::fs::rename(trashed, &target)
            .with_context(|| format!("Не удалось восстановить дубль {trashed:?}"))?;
        Ok(target)
    }
}

/// Атомарное сохранение: сначала во временный файл, потом подмена.
/// Иначе падение в момент записи оставляло битый проект.
pub fn save(paths: &ProjectPaths, data: &ProjectData) -> Result<()> {
    paths.ensure_dirs()?;

    let json = serde_json::to_string_pretty(data).context("Не удалось сериализовать проект")?;
    let temp = paths.root.join("project.json.tmp");

    std::fs::write(&temp, json)
        .with_context(|| format!("Не удалось записать {temp:?}"))?;

    if paths.project_file.exists() {
        let _ = std::fs::remove_file(&paths.project_file);
    }

    std::fs::rename(&temp, &paths.project_file)
        .with_context(|| format!("Не удалось сохранить {:?}", paths.project_file))?;

    Ok(())
}

pub fn load(paths: &ProjectPaths) -> Result<ProjectData> {
    let raw = std::fs::read_to_string(&paths.project_file)
        .with_context(|| format!("Не удалось прочитать {:?}", paths.project_file))?;

    let mut data: ProjectData =
        serde_json::from_str(&raw).context("Файл проекта повреждён или несовместим")?;

    // Потерянные файлы дублей не должны выглядеть как записанные
    for segment in data.segments.iter_mut() {
        segment.sync_duration();
        if let Some(file) = segment.recording_file.as_ref() {
            if !file.exists() {
                let in_takes = if let Some(name) = file.file_name() {
                    paths.takes_dir.join(name)
                } else {
                    paths.root.join(file)
                };
                if in_takes.exists() {
                    segment.recording_file = Some(in_takes);
                } else {
                    segment.recording_file = None;
                }
            }
        }
    }

    Ok(data)
}

/// Перенос записанных дублей на новую нарезку по максимальному перекрытию.
/// Раньше повторная нарезка теряла все записи.
pub fn remap_takes(previous: &[PhraseSegment], fresh: &mut [PhraseSegment]) -> usize {
    let mut used = vec![false; previous.len()];
    let mut restored = 0usize;

    for segment in fresh.iter_mut() {
        let mut best: Option<(usize, f32)> = None;

        for (index, old) in previous.iter().enumerate() {
            if used[index] || old.recording_file.is_none() {
                continue;
            }

            let overlap = old.overlap(segment.start_sec, segment.end_sec);
            if overlap <= 0.0 {
                continue;
            }

            if best.map_or(true, |(_, best_overlap)| overlap > best_overlap) {
                best = Some((index, overlap));
            }
        }

        if let Some((index, _)) = best {
            used[index] = true;
            segment.recording_file = previous[index].recording_file.clone();
            if segment.text_note.is_empty() {
                segment.text_note = previous[index].text_note.clone();
            }
            restored += 1;
        }
    }

    restored
}
