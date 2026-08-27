use std::path::PathBuf;
use std::time::Duration;

use eframe::egui;

use crate::audio::wav::{read_wav_mono, AudioBuffer};
use crate::audio::{dsp, AudioPlayer, AudioRecorder};
use crate::i18n::{self, Lang, Strings};
use crate::models::{DubMode, MediaInfo, MixConfig, PhraseSegment, SlicerConfig, WaveformData};
use crate::project::{self, ProjectData, ProjectPaths};
use crate::tasks::{Event, Task, Worker};
use crate::ui;
use crate::util::{format_duration, format_timecode};
use crate::video::{FramePump, VideoFrame};

/// Ширина декодируемого кадра: больше не нужно для превью
const PREVIEW_WIDTH: u32 = 720;
const AUTOSAVE_INTERVAL: f64 = 2.0;
const STATUS_LIFETIME: f64 = 6.0;

pub struct DubApp {
    worker: Worker,
    pub frames: FramePump,
    pub player: AudioPlayer,
    pub recorder: AudioRecorder,

    pub video_path: Option<PathBuf>,
    pub paths: Option<ProjectPaths>,
    pub info: MediaInfo,
    pub waveform: WaveformData,
    pub audio: Option<AudioBuffer>,
    pub segments: Vec<PhraseSegment>,
    pub selected: Option<usize>,
    pub dub_mode: DubMode,
    pub slicer_config: SlicerConfig,
    pub mix: MixConfig,

    pub playhead: f32,
    pub playing: bool,
    play_end: Option<f32>,

    pending_frame: Option<VideoFrame>,
    texture: Option<egui::TextureHandle>,
    texture_size: [usize; 2],
    still_request: f32,

    recording_for: Option<usize>,
    pub mic_level: f32,
    /// Пускать оригинальный звук в наушники во время записи
    pub monitor_original: bool,

    pub busy: bool,
    pub progress: Option<(String, f32)>,
    pub status: String,
    status_at: f64,
    pub warning: Option<String>,
    pub show_settings: bool,
    /// Режим «одна фраза за раз»: крупная карточка вместо студийной раскладки
    pub focus_mode: bool,
    /// Масштаб таймлайна (1.0 = 100% вписать в ширину, > 1.0 = приближение)
    pub timeline_zoom: f32,
    /// Автоматически прокручивать таймлайн вслед за курсором воспроизведения
    pub timeline_follow: bool,
    /// Язык интерфейса
    pub lang: Lang,

    pub demucs_downloading: bool,
    pub demucs_progress: f32,
    pub demucs_downloaded: u64,
    pub demucs_total: u64,

    undo_take: Option<(usize, PathBuf)>,
    dirty: bool,
    last_save: f64,
}

impl DubApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let player = AudioPlayer::new();
        // Язык берём из окружения: русская система — русский интерфейс, иначе English
        let lang = Lang::detect();
        let mut status = i18n::status_welcome(lang).to_string();
        let mut warning = None;

        if let Some(error) = player.init_error() {
            warning = Some(error.to_string());
            status = error.to_string();
        }

        if let Err(error) = crate::video::check_tools() {
            warning = Some(format!("{error:#}"));
        }

        Self {
            worker: Worker::spawn(),
            frames: FramePump::new(),
            player,
            recorder: AudioRecorder::new(),

            video_path: None,
            paths: None,
            info: MediaInfo::default(),
            waveform: WaveformData::default(),
            audio: None,
            segments: Vec::new(),
            selected: None,
            dub_mode: DubMode::default(),
            slicer_config: SlicerConfig::default(),
            mix: MixConfig::default(),

            playhead: 0.0,
            playing: false,
            play_end: None,

            pending_frame: None,
            texture: None,
            texture_size: [0, 0],
            still_request: -1.0,

            recording_for: None,
            mic_level: 0.0,
            monitor_original: false,

            busy: false,
            progress: None,
            status,
            status_at: 0.0,
            warning,
            show_settings: false,
            focus_mode: true,
            timeline_zoom: 1.0,
            timeline_follow: true,
            lang,

            demucs_downloading: false,
            demucs_progress: 0.0,
            demucs_downloaded: 0,
            demucs_total: 166_000_000,

            undo_take: None,
            dirty: false,
            last_save: 0.0,
        }
    }

    // ——— состояние ———

    pub fn has_media(&self) -> bool {
        self.video_path.is_some()
    }

    pub fn duration(&self) -> f32 {
        self.info.duration_sec.max(self.waveform.duration_sec)
    }

    pub fn texture(&self) -> Option<&egui::TextureHandle> {
        self.texture.as_ref()
    }

    pub fn selected_segment(&self) -> Option<&PhraseSegment> {
        self.selected.and_then(|index| self.segments.get(index))
    }

    pub fn recorded_count(&self) -> usize {
        self.segments.iter().filter(|s| s.has_recording()).count()
    }

    pub fn recording_index(&self) -> Option<usize> {
        self.recording_for
    }

    pub fn can_undo_delete(&self) -> bool {
        self.undo_take.is_some()
    }

    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
        self.status_at = 0.0;
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Строки текущего языка — короткий доступ для всего интерфейса
    pub fn t(&self) -> &'static Strings {
        self.lang.strings()
    }

    // ——— открытие и экспорт ———

    pub fn open_dialog(&mut self) {
        let picked = rfd::FileDialog::new()
            .add_filter(
                i18n::dialog_video(self.lang),
                &["mp4", "mkv", "mov", "avi", "webm", "m4v"],
            )
            .add_filter(i18n::dialog_all_files(self.lang), &["*"])
            .pick_file();

        if let Some(path) = picked {
            self.open_video(path);
        }
    }

    pub fn open_video(&mut self, path: PathBuf) {
        if self.busy {
            self.set_status(i18n::status_busy(self.lang));
            return;
        }

        self.save_project();
        self.stop_playback();
        self.recorder.cancel();
        self.recording_for = None;

        self.busy = true;
        self.warning = None;
        self.set_status(i18n::status_opening(self.lang));
        self.worker.send(Task::Open {
            video_path: path,
            lang: self.lang,
        });
    }

    pub fn export_dialog(&mut self) {
        if self.busy {
            return;
        }

        let (Some(video_path), Some(paths)) = (self.video_path.clone(), self.paths.clone()) else {
            self.set_status(i18n::status_open_first(self.lang));
            return;
        };

        if self.recorded_count() == 0 {
            self.set_status(i18n::status_no_takes(self.lang));
            return;
        }

        let stem = video_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("video");

        let picked = rfd::FileDialog::new()
            .set_file_name(format!("{stem}_dubbed.mp4"))
            .add_filter("MP4", &["mp4"])
            .save_file();

        let Some(output_path) = picked else {
            return;
        };

        self.stop_playback();
        self.save_project();
        self.busy = true;
        self.progress = Some((i18n::status_preparing(self.lang).to_string(), 0.0));

        self.worker.send(Task::Export {
            video_path,
            audio_path: paths.audio_path.clone(),
            segments: self.segments.clone(),
            mode: self.dub_mode,
            mix: self.mix,
            output_path,
            lang: self.lang,
        });
    }

    pub fn reslice(&mut self) {
        if self.busy {
            return;
        }

        let Some(paths) = self.paths.clone() else {
            self.set_status(i18n::status_open_first(self.lang));
            return;
        };

        self.stop_playback();
        self.busy = true;
        self.set_status(i18n::status_reslicing(self.lang));

        self.worker.send(Task::ReSlice {
            audio_path: paths.audio_path.clone(),
            config: self.slicer_config.clone(),
            previous: self.segments.clone(),
            lang: self.lang,
        });
    }

    pub fn save_project(&mut self) {
        let (Some(paths), Some(video_path)) = (self.paths.clone(), self.video_path.clone()) else {
            return;
        };

        let data = ProjectData {
            version: project::PROJECT_VERSION,
            video_path,
            segments: self.segments.clone(),
            dub_mode: self.dub_mode,
            slicer_config: self.slicer_config.clone(),
            mix: self.mix,
        };

        match project::save(&paths, &data) {
            Ok(()) => self.dirty = false,
            Err(error) => {
                let text = i18n::status_save_failed(self.lang, &format!("{error:#}"));
                self.set_status(text);
            }
        }
    }

    // ——— навигация и воспроизведение ———

    pub fn select(&mut self, index: usize) {
        if index >= self.segments.len() {
            return;
        }
        self.selected = Some(index);
        let start = self.segments[index].start_sec;
        self.seek(start);
    }

    /// К следующей фразе без дубля — основной путь, когда озвучиваешь подряд
    pub fn select_next_empty(&mut self) {
        if self.segments.is_empty() {
            return;
        }

        let count = self.segments.len();
        let from = self.selected.map(|index| index + 1).unwrap_or(0);

        // Ищем по кругу: после конца возвращаемся к пропущенным в начале
        for offset in 0..count {
            let index = (from + offset) % count;
            if !self.segments[index].has_recording() {
                self.select(index);
                let text = i18n::status_phrase_without_take(self.lang, index + 1);
                self.set_status(text);
                return;
            }
        }

        self.set_status(i18n::status_all_dubbed(self.lang));
    }

    pub fn step_segment(&mut self, delta: i32) {
        if self.segments.is_empty() {
            return;
        }
        let current = self.selected.unwrap_or(0) as i32;
        let next = (current + delta).clamp(0, self.segments.len() as i32 - 1);
        self.select(next as usize);
    }

    pub fn seek(&mut self, time_sec: f32) {
        let time = time_sec.clamp(0.0, self.duration());
        self.stop_playback();
        self.playhead = time;
        self.request_still(time);
    }

    fn request_still(&mut self, time_sec: f32) {
        if !self.has_media() || (self.still_request - time_sec).abs() < 0.01 {
            return;
        }
        self.still_request = time_sec;
        self.pending_frame = None;
        self.frames.drain();
        self.frames.request_still(time_sec);
    }

    pub fn stop_playback(&mut self) {
        self.player.stop();
        self.frames.stop();
        self.playing = false;
        self.play_end = None;
    }

    /// Воспроизвести оригинал с позиции до указанного момента
    pub fn play_original(&mut self, start_sec: f32, end_sec: Option<f32>) {
        if self.recorder.is_recording() {
            return;
        }

        if self.audio.is_none() {
            self.set_status(i18n::status_audio_not_ready(self.lang));
            return;
        }
        let Some(audio) = self.audio.as_ref() else {
            return;
        };

        let duration = self.duration();
        let start = start_sec.clamp(0.0, duration);
        let end = end_sec.unwrap_or(duration).clamp(start, duration);

        if end - start < 0.02 {
            return;
        }

        let samples = audio.slice_sec(start, end);
        let sample_rate = audio.sample_rate;

        self.stop_playback();

        if let Err(error) = self.player.play_samples(samples, sample_rate, start) {
            self.set_status(format!("{error:#}"));
            return;
        }

        self.playhead = start;
        self.playing = true;
        self.play_end = Some(end);
        self.pending_frame = None;
        self.frames.drain();
        self.frames.play(start);
        self.still_request = -1.0;
    }

    /// Space: воспроизвести выбранную фразу, иначе с курсора
    pub fn toggle_play(&mut self) {
        if self.playing {
            let position = self.playhead;
            self.stop_playback();
            self.playhead = position;
            self.request_still(position);
            return;
        }

        match self.selected_segment() {
            Some(segment) => {
                let (start, end) = (segment.start_sec, segment.end_sec);
                self.play_original(start, Some(end));
            }
            None => {
                let start = self.playhead;
                self.play_original(start, None);
            }
        }
    }

    /// Прослушать дубль так, как он будет в фильме: свой голос поверх сцены,
    /// с разгоном до фразы и видео. Голый дубль ничего не говорит о попадании в диалог.
    pub fn play_take(&mut self) {
        const PREROLL_SEC: f32 = 0.8;
        const TAIL_SEC: f32 = 0.4;

        if self.recorder.is_recording() {
            return;
        }

        let Some(segment) = self.selected_segment() else {
            self.set_status(i18n::status_select_phrase(self.lang));
            return;
        };
        let (seg_start, seg_end) = (segment.start_sec, segment.end_sec);
        let Some(path) = segment.recording_file.clone() else {
            self.set_status(i18n::status_no_take(self.lang));
            return;
        };

        // Без оригинальной дорожки смешивать не с чем
        if self.audio.is_none() {
            self.play_take_solo();
            return;
        }

        let take = match read_wav_mono(&path) {
            Ok(buffer) => buffer,
            Err(error) => {
                self.set_status(format!("{error:#}"));
                return;
            }
        };

        let duration = self.duration();
        let start = (seg_start - PREROLL_SEC).clamp(0.0, duration);
        let end = (seg_end + TAIL_SEC).clamp(start, duration);

        let (mixed, sample_rate) = {
            let Some(audio) = self.audio.as_ref() else {
                return;
            };
            let sample_rate = audio.sample_rate;
            let mut bed = audio.slice_sec(start, end);

            let mut voice = if take.sample_rate == sample_rate {
                take.samples
            } else {
                dsp::resample(&take.samples, take.sample_rate, sample_rate)
            };

            dsp::cleanup_voice(
                &mut voice,
                sample_rate,
                self.mix.highpass_hz,
                self.mix.gate_strength,
            );
            if self.mix.normalize_takes {
                dsp::normalize_peak(&mut voice, 0.89);
            }
            dsp::apply_gain(&mut voice, self.mix.take_gain.max(0.0));

            // Оригинал ведём так же, как на экспорте: что услышал — то и получишь
            let bed_gain = match self.dub_mode {
                DubMode::OnlyDubVoice => 0.0,
                DubMode::DubWithBackground => self.mix.bg_gain.max(0.0),
                _ => self.mix.original_gain.max(0.0),
            };
            let inside = match self.dub_mode {
                DubMode::ReplaceSpeech | DubMode::OnlyDubVoice => 0.0,
                DubMode::VoiceOverDucking => self.mix.duck_level.clamp(0.0, 1.0),
                // На прослушке подложка — сырой оригинал, а не разделённый фон,
                // поэтому в зоне фразы его надо глушить: иначе слышно два голоса
                // сразу, чего в готовом файле не будет.
                DubMode::DubWithBackground => 0.0,
            };

            let bed_len = bed.len();
            let to_index = |time: f32| {
                (((time - start).max(0.0) * sample_rate as f32) as usize).min(bed_len)
            };
            let phrase_from = to_index(seg_start);
            let phrase_to = to_index(seg_end).max(phrase_from);
            let ramp = ((sample_rate as f32 * 0.05) as usize).max(1);

            for (index, sample) in bed.iter_mut().enumerate() {
                let level = if index >= phrase_from && index < phrase_to {
                    // Плавный вход и выход приглушения — без щёлчков на границах
                    let from_start = index - phrase_from;
                    let to_end = phrase_to - index;
                    let t = (from_start.min(to_end) as f32 / ramp as f32).min(1.0);
                    inside + (1.0 - inside) * (1.0 - t)
                } else {
                    1.0
                };
                *sample *= bed_gain * level;
            }

            let mut mixed = bed;
            for (offset, sample) in voice.into_iter().enumerate() {
                let index = phrase_from + offset;
                if index >= mixed.len() {
                    break;
                }
                mixed[index] += sample;
            }
            for sample in mixed.iter_mut() {
                *sample = dsp::soft_clip(*sample);
            }

            (mixed, sample_rate)
        };

        self.stop_playback();

        if let Err(error) = self.player.play_samples(mixed, sample_rate, start) {
            self.set_status(format!("{error:#}"));
            return;
        }

        self.playhead = start;
        self.playing = true;
        self.play_end = Some(end);
        self.pending_frame = None;
        self.frames.drain();
        self.frames.play(start);
        self.still_request = -1.0;
        self.set_status(i18n::status_take_in_scene(self.lang));
    }

    /// Только запись, без сцены (Shift+T): проверить дикцию и шум
    pub fn play_take_solo(&mut self) {
        let Some(path) = self
            .selected_segment()
            .and_then(|segment| segment.recording_file.clone())
        else {
            self.set_status(i18n::status_no_take(self.lang));
            return;
        };

        self.stop_playback();

        if let Err(error) = self.player.play_file(&path) {
            self.set_status(format!("{error:#}"));
        }
    }

    // ——— запись ———

    pub fn toggle_recording(&mut self) {
        if self.recorder.is_recording() {
            self.finish_recording();
            return;
        }

        let Some(index) = self.selected else {
            self.set_status(i18n::status_select_phrase(self.lang));
            return;
        };

        if self.paths.is_none() {
            self.set_status(i18n::status_open_first(self.lang));
            return;
        }

        self.stop_playback();

        match self.recorder.start() {
            Ok(()) => {
                self.recording_for = Some(index);
                self.start_recording_preview(index);
                let device = self.recorder.device_name().to_string();
                let text = i18n::status_recording_phrase(self.lang, index + 1, &device);
                self.set_status(text);
            }
            Err(error) => self.set_status(format!("{error:#}")),
        }
    }

    /// Во время записи видео продолжает идти: без картинки не попасть в артикуляцию
    fn start_recording_preview(&mut self, index: usize) {
        let Some(segment) = self.segments.get(index) else {
            return;
        };
        let (start, end) = (segment.start_sec, segment.end_sec);

        self.playhead = start;
        self.pending_frame = None;
        self.still_request = -1.0;
        self.frames.drain();
        self.frames.play(start);

        // Оригинальная дорожка — только по желанию: через колонки она попадёт в микрофон
        if self.monitor_original {
            if let Some(audio) = self.audio.as_ref() {
                let samples = audio.slice_sec(start, end);
                let sample_rate = audio.sample_rate;
                if let Err(error) = self.player.play_samples(samples, sample_rate, start) {
                    self.set_status(format!("{error:#}"));
                }
            }
        }
    }

    /// После записи глушим видео и монитор, курсор возвращаем на начало фразы
    fn stop_recording_preview(&mut self, index: Option<usize>) {
        self.player.stop();
        self.frames.stop();
        self.playing = false;
        self.play_end = None;
        self.pending_frame = None;

        let start = index
            .and_then(|index| self.segments.get(index))
            .map(|segment| segment.start_sec)
            .unwrap_or(self.playhead);
        self.playhead = start;
        self.still_request = -1.0;
        self.request_still(start);
    }

    /// Отмена записи без сохранения дубля
    pub fn cancel_recording(&mut self) {
        self.recorder.cancel();
        let index = self.recording_for.take();
        self.stop_recording_preview(index);
        self.set_status(i18n::status_record_cancelled(self.lang));
    }

    fn finish_recording(&mut self) {
        let Some(index) = self.recording_for.take() else {
            self.recorder.cancel();
            return;
        };

        let Some(paths) = self.paths.clone() else {
            self.recorder.cancel();
            return;
        };

        let segment_id = self.segments.get(index).map(|s| s.id).unwrap_or(index + 1);
        let target = paths.unique_take_path(segment_id);

        match self.recorder.stop_and_save(&target) {
            Ok(result) => {
                if let Some(segment) = self.segments.get_mut(index) {
                    segment.recording_file = Some(result.path);
                }
                self.mark_dirty();
                let dropped = self.recorder.dropped_blocks();
                if dropped > 0 {
                    self.warning = Some(i18n::status_dropped_blocks(self.lang, dropped));
                }
                let text =
                    i18n::status_take_saved(self.lang, &format_duration(result.duration_sec));
                self.set_status(text);
            }
            Err(error) => self.set_status(format!("{error:#}")),
        }

        self.stop_recording_preview(Some(index));
    }

    pub fn delete_take(&mut self) {
        let Some(index) = self.selected else { return };
        let Some(paths) = self.paths.clone() else { return };

        let Some(path) = self
            .segments
            .get(index)
            .and_then(|segment| segment.recording_file.clone())
        else {
            self.set_status(i18n::status_no_take(self.lang));
            return;
        };

        self.player.stop();

        match paths.move_to_trash(&path) {
            Ok(trashed) => {
                if let Some(segment) = self.segments.get_mut(index) {
                    segment.recording_file = None;
                }
                self.undo_take = Some((index, trashed));
                self.mark_dirty();
                self.set_status(i18n::status_take_deleted(self.lang));
            }
            Err(error) => self.set_status(format!("{error:#}")),
        }
    }

    pub fn undo_delete(&mut self) {
        let Some((index, trashed)) = self.undo_take.take() else {
            return;
        };
        let Some(paths) = self.paths.clone() else { return };

        let segment_id = self.segments.get(index).map(|s| s.id).unwrap_or(index + 1);

        match paths.restore_from_trash(&trashed, segment_id) {
            Ok(restored) => {
                if let Some(segment) = self.segments.get_mut(index) {
                    segment.recording_file = Some(restored);
                }
                self.mark_dirty();
                self.set_status(i18n::status_take_restored(self.lang));
            }
            Err(error) => self.set_status(format!("{error:#}")),
        }
    }

    // ——— события фонового потока ———

    fn handle_events(&mut self) {
        for event in self.worker.poll() {
            match event {
                Event::Status(text) => self.set_status(text),

                Event::Progress { stage, fraction } => {
                    self.progress = Some((stage, fraction));
                }

                Event::Opened(opened) => {
                    self.busy = false;
                    self.progress = None;

                    let opened = *opened;
                    self.info = opened.info;
                    self.waveform = opened.waveform;
                    self.segments = opened.project.segments;
                    self.dub_mode = opened.project.dub_mode;
                    self.slicer_config = opened.project.slicer_config;
                    self.mix = opened.project.mix;
                    self.paths = Some(opened.paths);
                    self.video_path = Some(opened.video_path.clone());
                    self.warning = opened.warning;

                    self.audio = self
                        .paths
                        .as_ref()
                        .and_then(|paths| read_wav_mono(&paths.audio_path).ok());

                    self.playhead = 0.0;
                    self.selected = if self.segments.is_empty() { None } else { Some(0) };
                    self.undo_take = None;
                    self.dirty = false;

                    self.frames.open(&opened.video_path, &self.info, PREVIEW_WIDTH);
                    self.still_request = -1.0;
                    self.request_still(0.0);

                    let name = opened
                        .video_path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or(self.t().video_fallback)
                        .to_string();

                    let text = if opened.restored {
                        i18n::status_project_restored(
                            self.lang,
                            &name,
                            self.segments.len(),
                            self.recorded_count(),
                        )
                    } else {
                        i18n::status_phrases_found(self.lang, &name, self.segments.len())
                    };
                    self.set_status(text);
                }

                Event::Sliced {
                    segments,
                    engine,
                    warning,
                    restored,
                } => {
                    self.busy = false;
                    self.segments = segments;
                    self.selected = if self.segments.is_empty() { None } else { Some(0) };
                    self.warning = warning;
                    self.mark_dirty();
                    let text = i18n::status_sliced(
                        self.lang,
                        engine.label(self.lang),
                        self.segments.len(),
                        restored,
                    );
                    self.set_status(text);
                }

                Event::Exported(path) => {
                    self.busy = false;
                    self.progress = None;
                    let text = i18n::status_exported(self.lang, &path.display().to_string());
                    self.set_status(text);
                }

                Event::DemucsProgress {
                    fraction,
                    downloaded,
                    total,
                } => {
                    self.demucs_downloading = true;
                    self.demucs_progress = fraction;
                    self.demucs_downloaded = downloaded;
                    self.demucs_total = total;
                    self.progress = Some((
                        format!(
                            "HT-Demucs ({:.1} MB / {:.1} MB)",
                            downloaded as f64 / 1_048_576.0,
                            total as f64 / 1_048_576.0
                        ),
                        fraction,
                    ));
                }

                Event::DemucsFinished(res) => {
                    self.demucs_downloading = false;
                    self.progress = None;
                    match res {
                        Ok(_) => {
                            self.mix.enable_bg_separation = true;
                            self.mark_dirty();
                            let text = self.t().demucs_installed.to_string();
                            self.set_status(text);
                        }
                        Err(err) => {
                            let text = format!("HT-Demucs: {err}");
                            self.warning = Some(text.clone());
                            self.set_status(text);
                        }
                    }
                }

                Event::Failed(message) => {
                    self.busy = false;
                    self.progress = None;
                    self.warning = Some(message.clone());
                    self.set_status(message);
                }
            }
        }
    }

    pub fn download_demucs(&mut self) {
        if self.demucs_downloading {
            return;
        }
        self.demucs_downloading = true;
        self.demucs_progress = 0.0;
        self.worker.send(crate::tasks::Task::DownloadDemucs { lang: self.lang });
    }

    // ——— кадры ———

    fn pump_frames(&mut self, ctx: &egui::Context) {
        loop {
            if self.pending_frame.is_none() {
                self.pending_frame = self.frames.try_recv();
            }

            let Some(frame) = self.pending_frame.as_ref() else {
                break;
            };

            // Кадры текут и при воспроизведении, и во время записи дубля
            let streaming = self.playing || self.recorder.is_recording();

            // Кадр ждёт своё время по текущей позиции
            if streaming && frame.time_sec > self.playhead + 0.005 {
                break;
            }

            if let Some(frame) = self.pending_frame.take() {
                self.upload_frame(ctx, frame);
            }

            if !streaming {
                break;
            }
        }
    }

    fn upload_frame(&mut self, ctx: &egui::Context, frame: VideoFrame) {
        let width = frame.width as usize;
        let height = frame.height as usize;

        if width == 0 || height == 0 || frame.pixels.len() != width * height * 3 {
            return;
        }

        let image = egui::ColorImage::from_rgb([width, height], &frame.pixels);
        let options = egui::TextureOptions::LINEAR;

        match self.texture.as_mut() {
            Some(texture) if self.texture_size == [width, height] => texture.set(image, options),
            _ => {
                self.texture = Some(ctx.load_texture("dubrust-frame", image, options));
                self.texture_size = [width, height];
            }
        }
    }

    // ——— ввод ———

    fn handle_hotkeys(&mut self, ctx: &egui::Context) {
        // Раньше хоткеи срабатывали при вводе текста в поля
        if ctx.wants_keyboard_input() {
            return;
        }

        let (space, record, take, shift, prev, next, delete, undo, focus, next_empty) =
            ctx.input(|input| {
                (
                    input.key_pressed(egui::Key::Space),
                    input.key_pressed(egui::Key::R),
                    input.key_pressed(egui::Key::T),
                    input.modifiers.shift,
                    input.key_pressed(egui::Key::ArrowLeft),
                    input.key_pressed(egui::Key::ArrowRight),
                    input.key_pressed(egui::Key::Delete)
                        || input.key_pressed(egui::Key::Backspace),
                    input.modifiers.command && input.key_pressed(egui::Key::Z),
                    input.key_pressed(egui::Key::Tab),
                    input.key_pressed(egui::Key::Enter),
                )
            });

        if space {
            self.toggle_play();
        }
        if record {
            self.toggle_recording();
        }
        if take {
            // Shift+T — чистая запись, без сцены
            if shift {
                self.play_take_solo();
            } else {
                self.play_take();
            }
        }
        if prev {
            self.step_segment(-1);
        }
        if next {
            self.step_segment(1);
        }
        if delete {
            self.delete_take();
        }
        if undo {
            self.undo_delete();
        }
        if focus {
            self.focus_mode = !self.focus_mode;
        }
        if next_empty {
            self.select_next_empty();
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        // Сначала забираем данные, потом работаем — внутри ctx.input была блокировка
        let dropped = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect::<Vec<_>>()
        });

        if let Some(path) = dropped.into_iter().next() {
            self.open_video(path);
        }
    }

    fn tick_playback(&mut self) {
        // При записи позицию задаёт сам рекордер: видео и дубль идут одним временем
        if self.recorder.is_recording() {
            if let Some(index) = self.recording_for {
                let start = self
                    .segments
                    .get(index)
                    .map(|segment| segment.start_sec)
                    .unwrap_or(0.0);
                let position = start + self.recorder.recorded_duration_sec();
                self.playhead = position.min(self.duration());
            }
            return;
        }

        if !self.playing {
            return;
        }

        if self.player.is_active() {
            self.playhead = self.player.position_sec();
        }

        let finished = !self.player.is_active();
        let reached_end = self
            .play_end
            .map(|end| self.playhead >= end - 0.005)
            .unwrap_or(false);

        if finished || reached_end {
            let stop_at = self.play_end.unwrap_or(self.playhead).min(self.duration());
            self.stop_playback();
            self.playhead = stop_at;
            self.request_still(stop_at);
        }
    }

    fn autosave(&mut self, ctx: &egui::Context) {
        if !self.dirty || self.paths.is_none() {
            return;
        }

        let now = ctx.input(|input| input.time);
        if now - self.last_save < AUTOSAVE_INTERVAL {
            return;
        }

        self.last_save = now;
        self.save_project();
    }
}

impl eframe::App for DubApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dt = ctx.input(|input| input.unstable_dt).clamp(0.001, 0.2);

        self.handle_events();
        self.handle_dropped_files(ctx);
        self.handle_hotkeys(ctx);

        if self.recorder.is_recording() {
            self.recorder.pump();
            self.mic_level = self.recorder.level(dt);
        } else {
            self.mic_level = (self.mic_level - dt * 2.0).max(0.0);
        }

        self.tick_playback();
        self.pump_frames(ctx);

        ui::controls::top_bar(self, ctx);
        ui::controls::status_bar(self, ctx);
        ui::timeline::panel(self, ctx);

        // В фокус-режиме одна крупная карточка вместо списка фраз и студийного превью
        if self.focus_mode {
            ui::focus::central(self, ctx);
        } else {
            ui::phrases::side_panel(self, ctx);
            ui::video_view::central(self, ctx);
        }

        if self.show_settings {
            ui::controls::settings_window(self, ctx);
        }

        // Статус гаснет сам, чтобы не висеть вечно
        let now = ctx.input(|input| input.time);
        if self.status_at <= 0.0 {
            self.status_at = now;
        } else if now - self.status_at > STATUS_LIFETIME && !self.busy {
            self.status.clear();
        }

        self.autosave(ctx);

        if self.playing || self.recorder.is_recording() || self.busy {
            ctx.request_repaint_after(Duration::from_millis(16));
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.recorder.is_recording() {
            self.finish_recording();
        }
        self.stop_playback();
        self.save_project();
    }
}

/// Формат подписи позиции для интерфейса
pub fn playhead_label(app: &DubApp) -> String {
    format!(
        "{} / {}",
        format_timecode(app.playhead),
        format_timecode(app.duration())
    )
}
