use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use crate::audio::wav::read_wav_mono;

/// Источник, который считает реально проигранные сэмплы.
/// Благодаря этому позиция воспроизведения берётся из аудиопотока,
/// а не из настенных часов, и курсор не уезжает от звука.
struct CountingSource {
    data: Vec<f32>,
    pos: usize,
    sample_rate: u32,
    played: Arc<AtomicUsize>,
}

impl Iterator for CountingSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        let sample = *self.data.get(self.pos)?;
        self.pos += 1;
        if self.pos % 32 == 0 || self.pos == self.data.len() {
            self.played.store(self.pos, Ordering::Relaxed);
        }
        Some(sample)
    }
}

impl Source for CountingSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> u16 {
        1
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(
            self.data.len() as f64 / self.sample_rate as f64,
        ))
    }
}

pub struct AudioPlayer {
    _stream: Option<OutputStream>,
    handle: Option<OutputStreamHandle>,
    sink: Option<Sink>,
    played: Arc<AtomicUsize>,
    sample_rate: u32,
    start_sec: f32,
    length_sec: f32,
    volume: f32,
    paused: bool,
    init_error: Option<String>,
}

impl Default for AudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer {
    pub fn new() -> Self {
        let (stream, handle, init_error) = match OutputStream::try_default() {
            Ok((stream, handle)) => (Some(stream), Some(handle), None),
            Err(err) => (
                None,
                None,
                Some(format!("Нет доступного аудиовыхода: {err}")),
            ),
        };

        Self {
            _stream: stream,
            handle,
            sink: None,
            played: Arc::new(AtomicUsize::new(0)),
            sample_rate: 44_100,
            start_sec: 0.0,
            length_sec: 0.0,
            volume: 1.0,
            paused: false,
            init_error,
        }
    }

    pub fn is_available(&self) -> bool {
        self.handle.is_some()
    }

    pub fn init_error(&self) -> Option<&str> {
        self.init_error.as_deref()
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 2.0);
        if let Some(sink) = &self.sink {
            sink.set_volume(self.volume);
        }
    }

    /// Воспроизвести готовый буфер сэмплов.
    /// `start_sec` — время на таймлайне, соответствующее началу буфера.
    pub fn play_samples(
        &mut self,
        samples: Vec<f32>,
        sample_rate: u32,
        start_sec: f32,
    ) -> Result<()> {
        self.stop();

        if samples.is_empty() {
            return Ok(());
        }

        let handle = self
            .handle
            .as_ref()
            .ok_or_else(|| anyhow!("Аудиовыход недоступен"))?;

        let sample_rate = sample_rate.max(1);
        let length_sec = samples.len() as f32 / sample_rate as f32;
        let played = Arc::new(AtomicUsize::new(0));

        let sink = Sink::try_new(handle).context("Не удалось создать аудио-вывод")?;
        sink.set_volume(self.volume);
        sink.append(CountingSource {
            data: samples,
            pos: 0,
            sample_rate,
            played: Arc::clone(&played),
        });
        sink.play();

        self.played = played;
        self.sink = Some(sink);
        self.sample_rate = sample_rate;
        self.start_sec = start_sec;
        self.length_sec = length_sec;
        self.paused = false;

        Ok(())
    }

    /// Воспроизвести WAV-файл целиком (для прослушивания дублей)
    pub fn play_file(&mut self, path: &Path) -> Result<()> {
        let buffer = read_wav_mono(path)?;
        if buffer.is_empty() {
            return Err(anyhow!("Файл пуст: {:?}", path));
        }
        self.play_samples(buffer.samples, buffer.sample_rate, 0.0)
    }

    pub fn stop(&mut self) {
        if let Some(sink) = self.sink.take() {
            sink.stop();
        }
        self.played.store(0, Ordering::Relaxed);
        self.paused = false;
        self.length_sec = 0.0;
    }

    pub fn pause(&mut self) {
        if let Some(sink) = &self.sink {
            sink.pause();
            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if let Some(sink) = &self.sink {
            sink.play();
            self.paused = false;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused && self.sink.is_some()
    }

    /// Идёт ли воспроизведение прямо сейчас
    pub fn is_playing(&self) -> bool {
        match &self.sink {
            Some(sink) => !sink.empty() && !self.paused,
            None => false,
        }
    }

    /// Загружен ли буфер (воспроизведение идёт или на паузе)
    pub fn is_active(&self) -> bool {
        match &self.sink {
            Some(sink) => !sink.empty(),
            None => false,
        }
    }

    /// Текущая позиция на таймлайне в секундах
    pub fn position_sec(&self) -> f32 {
        let played = self.played.load(Ordering::Relaxed) as f32 / self.sample_rate as f32;
        self.start_sec + played.min(self.length_sec)
    }

    /// Сколько всего длится текущий буфер
    pub fn length_sec(&self) -> f32 {
        self.length_sec
    }

    /// Время конца текущего буфера на таймлайне
    pub fn end_sec(&self) -> f32 {
        self.start_sec + self.length_sec
    }
}
