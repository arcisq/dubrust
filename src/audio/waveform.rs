use std::path::Path;

use anyhow::Result;

use crate::audio::wav::{read_wav_mono, AudioBuffer};
use crate::models::WaveformData;

/// Построение волноформы из уже декодированных моно-сэмплов.
pub fn waveform_from_samples(
    samples: &[f32],
    sample_rate: u32,
    target_points_per_sec: f32,
) -> WaveformData {
    let sample_rate = sample_rate.max(1);
    let duration_sec = samples.len() as f32 / sample_rate as f32;

    if samples.is_empty() {
        return WaveformData {
            duration_sec: 0.0,
            peaks: Vec::new(),
        };
    }

    let points_per_sec = target_points_per_sec.max(1.0);
    let samples_per_point = ((sample_rate as f32 / points_per_sec) as usize).max(1);
    let mut peaks = Vec::with_capacity(samples.len() / samples_per_point + 1);

    for chunk in samples.chunks(samples_per_point) {
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        for &s in chunk {
            if s < min {
                min = s;
            }
            if s > max {
                max = s;
            }
        }
        peaks.push((min.clamp(-1.0, 1.0), max.clamp(-1.0, 1.0)));
    }

    WaveformData {
        duration_sec,
        peaks,
    }
}

/// Построение волноформы из WAV-файла.
pub fn extract_waveform_from_wav(path: &Path, target_points_per_sec: f32) -> Result<WaveformData> {
    let AudioBuffer {
        samples,
        sample_rate,
    } = read_wav_mono(path)?;
    Ok(waveform_from_samples(
        &samples,
        sample_rate,
        target_points_per_sec,
    ))
}
