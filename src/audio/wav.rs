use std::path::Path;

use anyhow::{Context, Result};

/// Декодированный моно-буфер в f32 [-1.0 .. 1.0]
#[derive(Debug, Clone, Default)]
pub struct AudioBuffer {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl AudioBuffer {
    pub fn duration_sec(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / self.sample_rate as f32
        }
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Индекс сэмпла по времени с обрезкой по границам буфера
    pub fn sample_index(&self, time_sec: f32) -> usize {
        let idx = (time_sec.max(0.0) * self.sample_rate as f32) as usize;
        idx.min(self.samples.len())
    }

    /// Копия участка по времени
    pub fn slice_sec(&self, start_sec: f32, end_sec: f32) -> Vec<f32> {
        let a = self.sample_index(start_sec);
        let b = self.sample_index(end_sec).max(a);
        self.samples[a..b].to_vec()
    }
}

fn int_scale(bits_per_sample: u16) -> f32 {
    match bits_per_sample {
        8 => i8::MAX as f32,
        16 => i16::MAX as f32,
        24 => 8_388_607.0,
        32 => i32::MAX as f32,
        _ => i16::MAX as f32,
    }
}

/// Чтение WAV в моно f32. Каналы корректно микшуются, битность учитывается.
pub fn read_wav_mono(path: &Path) -> Result<AudioBuffer> {
    let reader = hound::WavReader::open(path)
        .with_context(|| format!("Не удалось открыть WAV: {:?}", path))?;

    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .map(|s| s.clamp(-1.0, 1.0))
            .collect(),
        hound::SampleFormat::Int => {
            let scale = int_scale(spec.bits_per_sample);
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|v| (v as f32 / scale).clamp(-1.0, 1.0))
                .collect()
        }
    };

    let samples = if channels > 1 {
        interleaved
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / chunk.len() as f32)
            .collect()
    } else {
        interleaved
    };

    Ok(AudioBuffer {
        samples,
        sample_rate: spec.sample_rate.max(1),
    })
}

/// Запись моно WAV 16 бит
pub fn write_wav_mono(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Не удалось создать папку {:?}", parent))?;
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: sample_rate.max(1),
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(path, spec)
        .with_context(|| format!("Не удалось создать WAV: {:?}", path))?;

    for &sample in samples {
        writer.write_sample((sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)?;
    }

    writer
        .finalize()
        .with_context(|| format!("Не удалось завершить запись WAV: {:?}", path))?;

    Ok(())
}
