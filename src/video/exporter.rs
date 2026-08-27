use std::path::Path;

use anyhow::{anyhow, Context, Result};

use crate::audio::dsp;
use crate::audio::wav::{read_wav_mono, write_wav_mono, AudioBuffer};
use crate::i18n::{self, Lang};
use crate::models::{DubMode, MixConfig, PhraseSegment};
use crate::util::{hidden_command, tail_lines};

/// Всё сведение идёт в одной частоте дискретизации
pub const MASTER_RATE: u32 = 44100;

const EDGE_FADE_SEC: f32 = 0.012;
const DUCK_RAMP_SEC: f32 = 0.03;
const NORMALIZE_TARGET: f32 = 0.89;

/// Состояние экспорта для прогресс-бара
#[derive(Debug, Clone)]
pub struct ExportProgress {
    pub stage: String,
    pub fraction: f32,
}

/// Совместимая обёртка: настройки сведения по умолчанию, без прогресса
pub fn export_dubbed_video(
    video_path: &Path,
    original_audio: &Path,
    segments: &[PhraseSegment],
    mode: DubMode,
    output_path: &Path,
) -> Result<()> {
    export_dubbed_video_with(
        video_path,
        original_audio,
        segments,
        mode,
        &MixConfig::default(),
        output_path,
        Lang::default(),
        &mut |_| {},
    )
}

/// Полный экспорт с настройками сведения и отчётом о прогрессе
pub fn export_dubbed_video_with(
    video_path: &Path,
    original_audio: &Path,
    segments: &[PhraseSegment],
    mode: DubMode,
    mix: &MixConfig,
    output_path: &Path,
    lang: Lang,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<()> {
    let master = build_master_mix(original_audio, segments, mode, mix, lang, progress)?;

    if master.is_empty() {
        return Err(anyhow!(i18n::error_nothing_to_export(lang)));
    }

    let temp_dir = std::env::temp_dir().join("dubrust");
    let master_path = temp_dir.join(format!("master_mix_{}.wav", std::process::id()));
    write_wav_mono(&master_path, &master, MASTER_RATE)?;

    progress(ExportProgress {
        stage: i18n::stage_build_video(lang).to_string(),
        fraction: 0.85,
    });

    let result = mux_video_with_audio(video_path, &master_path, output_path);
    let _ = std::fs::remove_file(&master_path);
    result?;

    progress(ExportProgress {
        stage: i18n::stage_done(lang).to_string(),
        fraction: 1.0,
    });

    Ok(())
}

/// Сведение итоговой дорожки.
/// Заглушение делается через общий конверт усиления, поэтому пересекающиеся
/// фразы больше не глушат оригинал дважды и не дают щелчков на стыках.
pub fn build_master_mix(
    original_audio: &Path,
    segments: &[PhraseSegment],
    mode: DubMode,
    mix: &MixConfig,
    lang: Lang,
    progress: &mut dyn FnMut(ExportProgress),
) -> Result<Vec<f32>> {
    progress(ExportProgress {
        stage: i18n::stage_read_original(lang).to_string(),
        fraction: 0.05,
    });

    let bed_audio_path = if mode == DubMode::DubWithBackground {
        let parent = original_audio.parent().unwrap_or(original_audio);
        let bg_path = parent.join("background.wav");
        if !bg_path.exists() {
            progress(ExportProgress {
                stage: i18n::stage_read_original(lang).to_string(),
                fraction: 0.06,
            });
            let _ = crate::audio::extract_background_track(original_audio, &bg_path);
        }
        if bg_path.exists() {
            bg_path
        } else {
            original_audio.to_path_buf()
        }
    } else {
        original_audio.to_path_buf()
    };

    let original = load_at_master_rate(&bed_audio_path)?;

    // Фразы с записанными дублями, по порядку на таймлайне
    let mut recorded_segments: Vec<&PhraseSegment> = segments
        .iter()
        .filter(|s| s.has_recording())
        .collect();
    recorded_segments.sort_by(|a, b| {
        a.start_sec
            .partial_cmp(&b.start_sec)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut takes: Vec<(usize, Vec<f32>)> = Vec::new();
    let total = recorded_segments.len().max(1);

    for (index, segment) in recorded_segments.iter().enumerate() {
        let Some(path) = segment.recording_file.as_ref() else {
            continue;
        };
        if !path.exists() {
            continue;
        }

        progress(ExportProgress {
            stage: i18n::stage_mixing_phrase(lang, index + 1, total),
            fraction: 0.1 + 0.7 * (index as f32 / total as f32),
        });

        let take = load_at_master_rate(path)?;
        if take.samples.is_empty() {
            continue;
        }

        let start = seconds_to_samples(segment.start_sec);
        let target_len = seconds_to_samples(segment.end_sec).saturating_sub(start);

        // Доступная длина дубля — до следующего записанного дубля (а не до пустой фразы!)
        let available = recorded_segments
            .get(index + 1)
            .map(|next| seconds_to_samples(next.start_sec).saturating_sub(start))
            .unwrap_or(usize::MAX);

        let mut samples = take.samples;

        // Сначала убираем гул и фон, потом нормализуем
        dsp::cleanup_voice(&mut samples, MASTER_RATE, mix.highpass_hz, mix.gate_strength);

        if mix.normalize_takes {
            dsp::normalize_peak(&mut samples, NORMALIZE_TARGET);
        }

        let mut samples = fit_take(samples, target_len, available, mix);
        dsp::apply_gain(&mut samples, mix.take_gain.max(0.0));

        let fade = seconds_to_samples(EDGE_FADE_SEC).min(samples.len() / 4);
        dsp::fade_in(&mut samples, fade);
        dsp::fade_out(&mut samples, fade);

        takes.push((start, samples));
    }

    progress(ExportProgress {
        stage: i18n::stage_final_mix(lang).to_string(),
        fraction: 0.8,
    });

    let take_end = takes
        .iter()
        .map(|(start, samples)| start + samples.len())
        .max()
        .unwrap_or(0);
    let total_len = original.samples.len().max(take_end);

    if total_len == 0 {
        return Ok(Vec::new());
    }

    // Подложка из оригинала с конвертом заглушения
    let mut master = vec![0.0f32; total_len];
    let bed_level = match mode {
        DubMode::OnlyDubVoice => 0.0,
        DubMode::DubWithBackground => mix.bg_gain.max(0.0),
        _ => mix.original_gain.max(0.0),
    };

    if bed_level > 0.0 && !original.samples.is_empty() {
        let envelope = build_gain_envelope(&recorded_segments, total_len, mode, mix);
        for (index, target) in master.iter_mut().enumerate() {
            let sample = original.samples.get(index).copied().unwrap_or(0.0);
            *target = sample * bed_level * envelope[index];
        }
    }

    for (start, samples) in takes {
        for (offset, sample) in samples.into_iter().enumerate() {
            let index = start + offset;
            if index >= master.len() {
                break;
            }
            master[index] += sample;
        }
    }

    for sample in master.iter_mut() {
        *sample = dsp::soft_clip(*sample);
    }

    Ok(master)
}

fn build_gain_envelope(
    segments: &[&PhraseSegment],
    total_len: usize,
    mode: DubMode,
    mix: &MixConfig,
) -> Vec<f32> {
    let mut envelope = vec![1.0f32; total_len];

    let inside_level = match mode {
        DubMode::ReplaceSpeech | DubMode::OnlyDubVoice => 0.0,
        DubMode::VoiceOverDucking => mix.duck_level.clamp(0.0, 1.0),
        DubMode::DubWithBackground => 1.0,
    };

    if (inside_level - 1.0).abs() < f32::EPSILON {
        return envelope;
    }

    let ramp = seconds_to_samples(DUCK_RAMP_SEC).max(1);

    for segment in segments {
        // Глушим только там, где есть чем заменить
        if !segment.has_recording() {
            continue;
        }

        let start = seconds_to_samples(segment.start_sec).min(total_len);
        let end = seconds_to_samples(segment.end_sec).min(total_len);
        if end <= start {
            continue;
        }

        for index in start..end {
            let from_start = index - start;
            let to_end = end - index;
            let progress = from_start.min(to_end) as f32 / ramp as f32;
            let level = inside_level + (1.0 - inside_level) * (1.0 - progress.min(1.0));
            envelope[index] = envelope[index].min(level);
        }
    }

    envelope
}

/// Подгонка дубля под длину фразы без изменения высоты голоса
fn fit_take(
    samples: Vec<f32>,
    target_len: usize,
    available_len: usize,
    mix: &MixConfig,
) -> Vec<f32> {
    if samples.is_empty() {
        return samples;
    }

    let mut samples = samples;

    if mix.fit_takes && target_len > 0 {
        let max_stretch = mix.max_stretch.max(1.0);
        let factor = (target_len as f32 / samples.len() as f32).clamp(1.0 / max_stretch, max_stretch);
        if (factor - 1.0).abs() > 0.01 {
            samples = dsp::time_stretch(&samples, factor, MASTER_RATE);
        }
    }

    if samples.len() > available_len {
        samples.truncate(available_len);
    }

    samples
}

fn load_at_master_rate(path: &Path) -> Result<AudioBuffer> {
    let buffer = read_wav_mono(path)?;
    if buffer.sample_rate == MASTER_RATE || buffer.samples.is_empty() {
        return Ok(buffer);
    }

    Ok(AudioBuffer {
        samples: dsp::resample(&buffer.samples, buffer.sample_rate, MASTER_RATE),
        sample_rate: MASTER_RATE,
    })
}

fn seconds_to_samples(seconds: f32) -> usize {
    (seconds.max(0.0) * MASTER_RATE as f32).round() as usize
}

fn is_mp4_like(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "m4v" | "mov"))
        .unwrap_or(false)
}

/// Сборка видео с новой дорожкой.
/// Сначала без перекодирования видео, при несовместимом контейнере — с перекодированием.
fn mux_video_with_audio(video_path: &Path, audio_path: &Path, output_path: &Path) -> Result<()> {
    if let Some(parent) = output_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let attempts: [&[&str]; 2] = [
        &["-c:v", "copy"],
        &[
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
        ],
    ];

    let mut last_error = String::new();

    for video_args in attempts {
        let mut command = hidden_command("ffmpeg");
        command
            .args(["-y", "-v", "error"])
            .arg("-i")
            .arg(video_path)
            .arg("-i")
            .arg(audio_path)
            .args(["-map", "0:v:0", "-map", "1:a:0"])
            .args(video_args)
            .args(["-c:a", "aac", "-b:a", "256k", "-ar", "44100", "-shortest"]);

        if is_mp4_like(output_path) {
            command.args(["-movflags", "+faststart"]);
        }

        let output = command
            .arg(output_path)
            .output()
            .context("Не удалось запустить ffmpeg. Установлен ли ffmpeg?")?;

        if output.status.success() && output_path.exists() {
            return Ok(());
        }

        last_error = tail_lines(&String::from_utf8_lossy(&output.stderr), 3);
    }

    Err(anyhow!("Не удалось собрать видео: {last_error}"))
}
