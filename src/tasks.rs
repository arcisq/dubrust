use std::path::PathBuf;
use std::thread;

use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::audio::extract_waveform_from_wav;
use crate::i18n::{self, Lang};
use crate::models::{DubMode, MediaInfo, MixConfig, PhraseSegment, SlicerConfig, WaveformData};
use crate::project::{self, ProjectData, ProjectPaths};
use crate::slicer::{self, VadEngine};
use crate::video;

const WAVEFORM_POINTS_PER_SEC: f32 = 60.0;

/// Задание для фонового потока.
/// Раньше всё это выполнялось в UI-потоке, и окно висло на минуты.
/// Язык передаётся вместе с заданием: сообщения о прогрессе собираются в воркере.
pub enum Task {
    Open {
        video_path: PathBuf,
        lang: Lang,
    },
    ReSlice {
        audio_path: PathBuf,
        config: SlicerConfig,
        previous: Vec<PhraseSegment>,
        lang: Lang,
    },
    Export {
        video_path: PathBuf,
        audio_path: PathBuf,
        segments: Vec<PhraseSegment>,
        mode: DubMode,
        mix: MixConfig,
        output_path: PathBuf,
        lang: Lang,
    },
    DownloadDemucs {
        lang: Lang,
    },
}

/// Результат открытия медиафайла
pub struct OpenedMedia {
    pub video_path: PathBuf,
    pub info: MediaInfo,
    pub paths: ProjectPaths,
    pub project: ProjectData,
    pub waveform: WaveformData,
    pub restored: bool,
    pub warning: Option<String>,
}

/// Событие из фонового потока
pub enum Event {
    Status(String),
    Progress {
        stage: String,
        fraction: f32,
    },
    Opened(Box<OpenedMedia>),
    Sliced {
        segments: Vec<PhraseSegment>,
        engine: VadEngine,
        warning: Option<String>,
        restored: usize,
    },
    Exported(PathBuf),
    DemucsProgress {
        fraction: f32,
        downloaded: u64,
        total: u64,
    },
    DemucsFinished(Result<PathBuf, String>),
    Failed(String),
}

fn handle_open(video_path: PathBuf, lang: Lang, events: &Sender<Event>) -> Result<Box<OpenedMedia>> {
    let _ = events.send(Event::Status(i18n::stage_metadata(lang).to_string()));
    video::check_tools()?;

    let info = video::probe_media(&video_path)?;
    let paths = ProjectPaths::for_video(&video_path);
    paths.ensure_dirs()?;

    let restored = paths.exists();
    let mut project = if restored {
        match project::load(&paths) {
            Ok(data) => data,
            Err(_) => ProjectData::new(&video_path),
        }
    } else {
        ProjectData::new(&video_path)
    };

    // Путь видео мог измениться — перенос папки больше не ломает проект
    project.video_path = video_path.clone();

    if !paths.audio_path.is_file() {
        let _ = events.send(Event::Status(i18n::stage_extract_audio(lang).to_string()));
        video::extract_audio_from_video(&video_path, &paths.audio_path)?;
    }

    let _ = events.send(Event::Status(i18n::stage_waveform(lang).to_string()));
    let waveform = extract_waveform_from_wav(&paths.audio_path, WAVEFORM_POINTS_PER_SEC)?;

    let mut warning = None;

    if project.segments.is_empty() {
        let _ = events.send(Event::Status(i18n::stage_slicing(lang).to_string()));
        match slicer::detect_phrases_from_wav_verbose(&paths.audio_path, &project.slicer_config) {
            Ok((segments, report)) => {
                project.segments = segments;
                warning = report.warning;
            }
            Err(error) => {
                warning = Some(i18n::status_slice_failed(lang, &format!("{error}")));
            }
        }
    }

    let _ = project::save(&paths, &project);

    Ok(Box::new(OpenedMedia {
        video_path,
        info,
        paths,
        project,
        waveform,
        restored,
        warning,
    }))
}

fn worker(tasks: Receiver<Task>, events: Sender<Event>) {
    while let Ok(task) = tasks.recv() {
        match task {
            Task::Open { video_path, lang } => match handle_open(video_path, lang, &events) {
                Ok(opened) => {
                    let _ = events.send(Event::Opened(opened));
                }
                Err(error) => {
                    let _ = events.send(Event::Failed(format!("{error:#}")));
                }
            },

            Task::ReSlice {
                audio_path,
                config,
                previous,
                lang,
            } => {
                let _ = events.send(Event::Status(i18n::status_reslicing(lang).to_string()));

                match slicer::detect_phrases_from_wav_verbose(&audio_path, &config) {
                    Ok((mut segments, report)) => {
                        let restored = project::remap_takes(&previous, &mut segments);
                        let _ = events.send(Event::Sliced {
                            segments,
                            engine: report.engine,
                            warning: report.warning,
                            restored,
                        });
                    }
                    Err(error) => {
                        let _ = events.send(Event::Failed(format!("{error:#}")));
                    }
                }
            }

            Task::Export {
                video_path,
                audio_path,
                segments,
                mode,
                mix,
                output_path,
                lang,
            } => {
                let reporter = events.clone();
                let mut progress = move |update: video::ExportProgress| {
                    let _ = reporter.send(Event::Progress {
                        stage: update.stage,
                        fraction: update.fraction,
                    });
                };

                let result = video::export_dubbed_video_with(
                    &video_path,
                    &audio_path,
                    &segments,
                    mode,
                    &mix,
                    &output_path,
                    lang,
                    &mut progress,
                );

                match result {
                    Ok(()) => {
                        let _ = events.send(Event::Exported(output_path));
                    }
                    Err(error) => {
                        let _ = events.send(Event::Failed(format!("{error:#}")));
                    }
                }
            }
            Task::DownloadDemucs { lang } => {
                let sender = events.clone();
                let _ = events.send(Event::Status(lang.strings().demucs_downloading.to_string()));
                let result = crate::audio::download_demucs_model(move |fraction, downloaded, total| {
                    let _ = sender.send(Event::DemucsProgress {
                        fraction,
                        downloaded,
                        total,
                    });
                });
                match result {
                    Ok(path) => {
                        let _ = events.send(Event::DemucsFinished(Ok(path)));
                    }
                    Err(err) => {
                        let _ = events.send(Event::DemucsFinished(Err(format!("{err:#}"))));
                    }
                }
            }
        }
    }
}

/// Фоновый исполнитель тяжёлых операций
pub struct Worker {
    tasks: Sender<Task>,
    events: Receiver<Event>,
}

impl Default for Worker {
    fn default() -> Self {
        Self::spawn()
    }
}

impl Worker {
    pub fn spawn() -> Self {
        let (task_tx, task_rx) = unbounded::<Task>();
        let (event_tx, event_rx) = unbounded::<Event>();

        let _ = thread::Builder::new()
            .name("dubrust-worker".to_string())
            .spawn(move || worker(task_rx, event_tx));

        Self {
            tasks: task_tx,
            events: event_rx,
        }
    }

    pub fn send(&self, task: Task) -> bool {
        self.tasks.send(task).is_ok()
    }

    /// Все накопившиеся события без блокировки UI
    pub fn poll(&self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}
