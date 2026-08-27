use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::i18n::Lang;

/// Режим сведения финального аудио
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DubMode {
    /// Полностью убрать оригинальную дорожку (только ваш дубляж)
    #[default]
    OnlyDubVoice,
    /// Дубляж + чистый фон (HT-Demucs): голос оригинала удалён нейросетью, звучат ваши дубли + чистый фон
    DubWithBackground,
    /// Глушить исходную речь в зонах фраз, фон оставлять
    ReplaceSpeech,
    /// Закадровый перевод: приглушение оригинала под голосом
    VoiceOverDucking,
}

impl DubMode {
    pub const ALL: [DubMode; 4] = [
        DubMode::OnlyDubVoice,
        DubMode::DubWithBackground,
        DubMode::ReplaceSpeech,
        DubMode::VoiceOverDucking,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let strings = lang.strings();
        match self {
            DubMode::ReplaceSpeech => strings.mode_replace,
            DubMode::OnlyDubVoice => strings.mode_only_dub,
            DubMode::VoiceOverDucking => strings.mode_voiceover,
            DubMode::DubWithBackground => strings.mode_dub_with_bg,
        }
    }

    pub fn hint(self, lang: Lang) -> &'static str {
        let strings = lang.strings();
        match self {
            DubMode::ReplaceSpeech => strings.mode_replace_hint,
            DubMode::OnlyDubVoice => strings.mode_only_dub_hint,
            DubMode::VoiceOverDucking => strings.mode_voiceover_hint,
            DubMode::DubWithBackground => strings.mode_dub_with_bg_hint,
        }
    }
}

/// Фрагмент речи / фраза
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhraseSegment {
    pub id: usize,
    pub start_sec: f32,
    pub end_sec: f32,
    /// Путь к файлу с записанным дублем (WAV)
    pub recording_file: Option<PathBuf>,
    pub duration: f32,
    #[serde(default)]
    pub text_note: String,
}

impl PhraseSegment {
    pub fn new(id: usize, start_sec: f32, end_sec: f32) -> Self {
        let (start_sec, end_sec) = if end_sec < start_sec {
            (end_sec, start_sec)
        } else {
            (start_sec, end_sec)
        };
        let start_sec = start_sec.max(0.0);
        let end_sec = end_sec.max(start_sec);
        Self {
            id,
            start_sec,
            end_sec,
            duration: end_sec - start_sec,
            recording_file: None,
            text_note: String::new(),
        }
    }

    pub fn has_recording(&self) -> bool {
        self.recording_file.as_ref().map_or(false, |p| p.exists())
    }

    /// Изменить границы и сразу синхронизировать длительность
    pub fn set_range(&mut self, start_sec: f32, end_sec: f32) {
        self.start_sec = start_sec.max(0.0);
        self.end_sec = end_sec.max(self.start_sec);
        self.duration = self.end_sec - self.start_sec;
    }

    /// Пересчитать длительность (например, после загрузки проекта)
    pub fn sync_duration(&mut self) {
        self.duration = (self.end_sec - self.start_sec).max(0.0);
    }

    pub fn contains(&self, time_sec: f32) -> bool {
        time_sec >= self.start_sec && time_sec <= self.end_sec
    }

    /// Длина пересечения с интервалом в секундах
    pub fn overlap(&self, start_sec: f32, end_sec: f32) -> f32 {
        (self.end_sec.min(end_sec) - self.start_sec.max(start_sec)).max(0.0)
    }
}

/// Движок сегментации речи
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SlicerEngine {
    /// FireRedVAD: SOTA нейросетевой детектор активности голоса (~2.3 МБ, Apache-2.0)
    #[default]
    FireRedVad,
    /// Встроенный DSP: быстрый поиск по уровню громкости
    Dsp,
}

impl SlicerEngine {
    pub const ALL: [SlicerEngine; 2] = [
        SlicerEngine::FireRedVad,
        SlicerEngine::Dsp,
    ];

    pub fn label(self, lang: Lang) -> &'static str {
        let strings = lang.strings();
        match self {
            SlicerEngine::FireRedVad => strings.engine_firered,
            SlicerEngine::Dsp => strings.engine_dsp,
        }
    }
}

/// Настройки автонарезки на фразы
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicerConfig {
    /// Выбранный движок нарезки
    #[serde(default)]
    pub engine: SlicerEngine,
    /// Использовать нейросетевую модель FireRedVAD (для совместимости)
    #[serde(default = "default_true")]
    pub use_neural_vad: bool,
    /// Порог вероятности речи для нейросети (0.1 .. 0.9)
    pub neural_threshold: f32,
    /// Порог тишины для DSP-алгоритма в dB
    pub silence_threshold_db: f32,
    /// Минимальная пауза между фразами (сек)
    pub min_silence_duration_sec: f32,
    /// Минимальная длительность фразы (сек)
    pub min_phrase_duration_sec: f32,
    /// Максимальная длительность фразы (сек)
    pub max_phrase_duration_sec: f32,
    /// Отступ до и после фразы (сек)
    pub padding_sec: f32,
}

fn default_true() -> bool {
    true
}

impl Default for SlicerConfig {
    fn default() -> Self {
        Self {
            engine: SlicerEngine::FireRedVad,
            use_neural_vad: true,
            neural_threshold: 0.40,
            silence_threshold_db: -28.0,
            min_silence_duration_sec: 0.25,
            min_phrase_duration_sec: 0.25,
            max_phrase_duration_sec: 5.0,
            padding_sec: 0.08,
        }
    }
}

/// Настройки сведения финальной дорожки
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MixConfig {
    /// Усиление записанных дублей
    pub take_gain: f32,
    /// Усиление оригинальной дорожки
    pub original_gain: f32,
    /// Уровень приглушения оригинала в режиме ducking
    pub duck_level: f32,
    /// Подгонять длину дубля под длину фразы без изменения тона
    pub fit_takes: bool,
    /// Максимальный коэффициент растяжения/сжатия дубля
    pub max_stretch: f32,
    /// Нормализовать пик дубля перед сведением
    pub normalize_takes: bool,
    /// Срез низких частот в дубле, Гц (гул кулера и рокот стола)
    #[serde(default = "default_highpass_hz")]
    pub highpass_hz: f32,
    /// Сила шумодава для дублей (0.0 — выключен)
    #[serde(default = "default_gate_strength")]
    pub gate_strength: f32,
    /// Разделение голос/фон через HT-Demucs
    #[serde(default)]
    pub enable_bg_separation: bool,
    /// Громкость фоновой музыки/шума (0.0 .. 2.0)
    #[serde(default = "default_bg_gain")]
    pub bg_gain: f32,
}

fn default_highpass_hz() -> f32 {
    85.0
}

fn default_gate_strength() -> f32 {
    0.7
}

fn default_bg_gain() -> f32 {
    1.0
}

impl Default for MixConfig {
    fn default() -> Self {
        Self {
            take_gain: 1.0,
            original_gain: 1.0,
            duck_level: 0.12,
            fit_takes: true,
            max_stretch: 1.25,
            normalize_takes: true,
            highpass_hz: default_highpass_hz(),
            gate_strength: default_gate_strength(),
            enable_bg_separation: false,
            bg_gain: default_bg_gain(),
        }
    }
}

/// Данные для отрисовки волноформы
#[derive(Debug, Clone, Default)]
pub struct WaveformData {
    pub duration_sec: f32,
    /// Пики (min, max) по интервалам
    pub peaks: Vec<(f32, f32)>,
}

impl WaveformData {
    /// Пик по относительной позиции 0.0 .. 1.0
    pub fn peak_at(&self, rel: f32) -> (f32, f32) {
        if self.peaks.is_empty() {
            return (0.0, 0.0);
        }
        let idx =
            ((rel.clamp(0.0, 1.0) * self.peaks.len() as f32) as usize).min(self.peaks.len() - 1);
        self.peaks[idx]
    }
}

/// Метаданные открытого медиафайла
#[derive(Debug, Clone)]
pub struct MediaInfo {
    pub duration_sec: f32,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub has_video: bool,
    pub has_audio: bool,
}

impl Default for MediaInfo {
    fn default() -> Self {
        Self {
            duration_sec: 0.0,
            width: 640,
            height: 360,
            fps: 25.0,
            has_video: false,
            has_audio: false,
        }
    }
}
