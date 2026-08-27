use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{bounded, Receiver, Sender, TrySendError};

use crate::audio::wav::write_wav_mono;

/// Результат сохранённого дубля
#[derive(Debug, Clone)]
pub struct RecordingResult {
    pub path: PathBuf,
    pub sample_rate: u32,
    pub duration_sec: f32,
}

/// Атомарное обновление пика (без блокировок в realtime-колбэке)
fn store_peak(peak: &AtomicU32, value: f32) {
    let mut current = peak.load(Ordering::Relaxed);
    loop {
        if value <= f32::from_bits(current) {
            return;
        }
        match peak.compare_exchange_weak(
            current,
            value.to_bits(),
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

macro_rules! build_input_stream {
    ($device:expr, $config:expr, $ty:ty, $conv:expr, $tx:expr, $peak:expr, $dropped:expr, $channels:expr) => {{
        let tx: Sender<Vec<f32>> = $tx;
        let peak: Arc<AtomicU32> = $peak;
        let dropped: Arc<AtomicUsize> = $dropped;
        let channels: usize = $channels;
        let convert = $conv;

        $device.build_input_stream(
            $config,
            move |data: &[$ty], _: &cpal::InputCallbackInfo| {
                let mut block = Vec::with_capacity(data.len() / channels + 1);
                let mut local_peak = 0.0f32;

                for frame in data.chunks(channels) {
                    let mut sum = 0.0f32;
                    for &raw in frame {
                        sum += convert(raw);
                    }
                    let value = (sum / frame.len() as f32).clamp(-1.0, 1.0);
                    local_peak = local_peak.max(value.abs());
                    block.push(value);
                }

                store_peak(&peak, local_peak);

                if let Err(TrySendError::Full(_)) = tx.try_send(block) {
                    dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            move |err| eprintln!("Ошибка входного аудиопотока: {err}"),
            None,
        )
    }};
}

pub struct AudioRecorder {
    stream: Option<cpal::Stream>,
    receiver: Option<Receiver<Vec<f32>>>,
    buffer: Vec<f32>,
    sample_rate: u32,
    channels: u16,
    recording: bool,
    peak: Arc<AtomicU32>,
    dropped: Arc<AtomicUsize>,
    level_display: f32,
    device_name: String,
}

impl Default for AudioRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            stream: None,
            receiver: None,
            buffer: Vec::new(),
            sample_rate: 48_000,
            channels: 1,
            recording: false,
            peak: Arc::new(AtomicU32::new(0)),
            dropped: Arc::new(AtomicUsize::new(0)),
            level_display: 0.0,
            device_name: String::new(),
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Сколько блоков потеряно из-за переполнения (для диагностики)
    pub fn dropped_blocks(&self) -> usize {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Начать запись с устройства по умолчанию
    pub fn start(&mut self) -> Result<()> {
        if self.recording {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow!("Микрофон не найден. Проверьте устройство ввода в системе"))?;

        self.device_name = device.name().unwrap_or_else(|_| "неизвестное устройство".to_string());

        let supported = device
            .default_input_config()
            .context("Не удалось получить конфигурацию микрофона")?;

        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();

        self.sample_rate = config.sample_rate.0.max(1);
        self.channels = config.channels.max(1);
        let channels = self.channels as usize;

        let (tx, rx) = bounded::<Vec<f32>>(512);
        self.peak.store(0, Ordering::Relaxed);
        self.dropped.store(0, Ordering::Relaxed);
        self.buffer.clear();

        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_input_stream!(
                device,
                &config,
                f32,
                |raw: f32| raw,
                tx,
                Arc::clone(&self.peak),
                Arc::clone(&self.dropped),
                channels
            ),
            cpal::SampleFormat::I16 => build_input_stream!(
                device,
                &config,
                i16,
                |raw: i16| raw as f32 / i16::MAX as f32,
                tx,
                Arc::clone(&self.peak),
                Arc::clone(&self.dropped),
                channels
            ),
            cpal::SampleFormat::U16 => build_input_stream!(
                device,
                &config,
                u16,
                |raw: u16| (raw as f32 - 32768.0) / 32768.0,
                tx,
                Arc::clone(&self.peak),
                Arc::clone(&self.dropped),
                channels
            ),
            other => return Err(anyhow!("Неподдерживаемый формат микрофона: {other:?}")),
        }
        .context("Не удалось открыть входной аудиопоток")?;

        stream.play().context("Не удалось запустить запись")?;

        self.stream = Some(stream);
        self.receiver = Some(rx);
        self.recording = true;

        Ok(())
    }

    /// Переложить накопленные блоки из канала в буфер.
    /// Вызывается из UI-потока каждый кадр.
    pub fn pump(&mut self) {
        if let Some(rx) = self.receiver.take() {
            while let Ok(block) = rx.try_recv() {
                self.buffer.extend_from_slice(&block);
            }
            self.receiver = Some(rx);
        }
    }

    /// Длительность уже записанного материала
    pub fn recorded_duration_sec(&self) -> f32 {
        self.buffer.len() as f32 / self.sample_rate as f32
    }

    /// Уровень для VU-индикатора с плавным затуханием
    pub fn level(&mut self, dt_sec: f32) -> f32 {
        let raw = f32::from_bits(self.peak.swap(0, Ordering::Relaxed));
        if raw > self.level_display {
            self.level_display = raw;
        } else {
            self.level_display = (self.level_display - dt_sec * 1.5).max(raw).max(0.0);
        }
        self.level_display
    }

    fn shutdown_stream(&mut self) {
        self.pump();
        if let Some(stream) = self.stream.take() {
            let _ = stream.pause();
        }
        self.pump();
        self.receiver = None;
        self.recording = false;
    }

    /// Завершить запись и сохранить в WAV на частоте микрофона
    pub fn stop_and_save(&mut self, path: &Path) -> Result<RecordingResult> {
        self.shutdown_stream();

        let samples = std::mem::take(&mut self.buffer);
        let sample_rate = self.sample_rate;

        if samples.len() < sample_rate as usize / 50 {
            return Err(anyhow!(
                "Запись слишком короткая — ничего не сохранено"
            ));
        }

        write_wav_mono(path, &samples, sample_rate)?;

        Ok(RecordingResult {
            path: path.to_path_buf(),
            sample_rate,
            duration_sec: samples.len() as f32 / sample_rate as f32,
        })
    }

    /// Отменить запись без сохранения
    pub fn cancel(&mut self) {
        self.shutdown_stream();
        self.buffer.clear();
        self.level_display = 0.0;
    }
}
