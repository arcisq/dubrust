//! 100% чистый Rust инференс модели FireRedVAD (SOTA 2026, Apache-2.0, 2.3 МБ).
//! Работает полностью автономно внутри бинарника без необходимости в Python или внешних DLL.

use anyhow::{anyhow, Context, Result};
use std::f32::consts::PI;
use tract_onnx::prelude::*;

/// Сколько кадров отдаём сети за один запуск (3000 кадров = 30 секунд).
/// Раньше весь фильм шёл одним тензором: на часовом видео это гигабайты
/// памяти и минуты зависания.
const CHUNK_FRAMES: usize = 3000;
/// Перехлёст на разогрев рекуррентных слоёв; эти кадры отбрасываются.
const CHUNK_WARMUP_FRAMES: usize = 50;

const EMBEDDED_MODEL: &[u8] = include_bytes!("../../models/firered_vad/firered_vad.onnx");
const EMBEDDED_CMVN: &str = include_str!("../../models/firered_vad/cmvn.json");

const SAMPLE_RATE: usize = 16000;
const FRAME_LENGTH_SAMPLES: usize = 400; // 25ms @ 16kHz
const FRAME_SHIFT_SAMPLES: usize = 160;  // 10ms @ 16kHz
const FFT_SIZE: usize = 512;
const NUM_MEL_BINS: usize = 80;
const LOW_FREQ: f32 = 20.0;
const HIGH_FREQ: f32 = 8000.0;

#[derive(Debug, Clone, serde::Deserialize)]
struct CmvnData {
    means: Vec<f32>,
    inv_stddevs: Vec<f32>,
}

/// Вычисление Mel-частоты по формуле Kaldi / O'Shaughnessy
#[inline]
fn hz_to_mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

#[inline]
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * ((mel / 1127.0).exp() - 1.0)
}

/// Построение треугольных Mel-фильтров (Kaldi style, 80 bins)
fn get_mel_filterbank() -> Vec<Vec<f32>> {
    let num_bins = NUM_MEL_BINS;
    let num_fft_bins = FFT_SIZE / 2 + 1;

    let mel_low = hz_to_mel(LOW_FREQ);
    let mel_high = hz_to_mel(HIGH_FREQ);
    let mel_delta = (mel_high - mel_low) / (num_bins + 1) as f32;

    let mut filterbank = vec![vec![0.0f32; num_fft_bins]; num_bins];

    for i in 0..num_bins {
        let left_mel = mel_low + i as f32 * mel_delta;
        let center_mel = mel_low + (i + 1) as f32 * mel_delta;
        let right_mel = mel_low + (i + 2) as f32 * mel_delta;

        let left_hz = mel_to_hz(left_mel);
        let center_hz = mel_to_hz(center_mel);
        let right_hz = mel_to_hz(right_mel);

        for (k, filter_val) in filterbank[i].iter_mut().enumerate().take(num_fft_bins) {
            let freq = k as f32 * (SAMPLE_RATE as f32 / FFT_SIZE as f32);
            if freq >= left_hz && freq <= center_hz && center_hz > left_hz {
                *filter_val = (freq - left_hz) / (center_hz - left_hz);
            } else if freq > center_hz && freq <= right_hz && right_hz > center_hz {
                *filter_val = (right_hz - freq) / (right_hz - center_hz);
            }
        }
    }

    filterbank
}

/// Простое и быстрое БПФ (Radix-2 FFT на 512 точек)
fn fft_512(real: &mut [f32; FFT_SIZE], imag: &mut [f32; FFT_SIZE]) {
    let n = FFT_SIZE;
    let mut j = 0;
    for i in 0..n - 1 {
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
        let mut k = n / 2;
        while k <= j {
            j -= k;
            k /= 2;
        }
        j += k;
    }

    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = -2.0 * PI / len as f32;
        let w_step_r = angle.cos();
        let w_step_i = angle.sin();

        let mut i = 0;
        while i < n {
            let mut wr = 1.0f32;
            let mut wi = 0.0f32;

            for k in 0..half {
                let u_r = real[i + k];
                let u_i = imag[i + k];

                let v_r = real[i + k + half] * wr - imag[i + k + half] * wi;
                let v_i = real[i + k + half] * wi + imag[i + k + half] * wr;

                real[i + k] = u_r + v_r;
                imag[i + k] = u_i + v_i;
                real[i + k + half] = u_r - v_r;
                imag[i + k + half] = u_i - v_i;

                let next_wr = wr * w_step_r - wi * w_step_i;
                let next_wi = wr * w_step_i + wi * w_step_r;
                wr = next_wr;
                wi = next_wi;
            }
            i += len;
        }
        len *= 2;
    }
}

/// Извлечение 80-dim fbank признаков с CMVN-нормализацией на чистом Rust
fn extract_fbank_cmvn(samples_16k: &[f32]) -> Result<Vec<f32>> {
    let cmvn: CmvnData = serde_json::from_str(EMBEDDED_CMVN)
        .context("Ошибка парсинга встроенного cmvn.json")?;

    // Без этой проверки короткий cmvn.json давал panic по индексу
    // прямо в цикле признаков — то есть приложение просто падало.
    if cmvn.means.len() < NUM_MEL_BINS || cmvn.inv_stddevs.len() < NUM_MEL_BINS {
        return Err(anyhow!(
            "cmvn.json повреждён: ожидалось {NUM_MEL_BINS} коэффициентов"
        ));
    }

    if samples_16k.len() < FRAME_LENGTH_SAMPLES {
        return Ok(Vec::new());
    }

    let num_frames = (samples_16k.len() - FRAME_LENGTH_SAMPLES) / FRAME_SHIFT_SAMPLES + 1;
    let filterbank = get_mel_filterbank();

    // Окно Хэмминга / Povey
    let mut window = [0.0f32; FRAME_LENGTH_SAMPLES];
    for (i, w) in window.iter_mut().enumerate() {
        *w = 0.54 - 0.46 * ((2.0 * PI * i as f32) / (FRAME_LENGTH_SAMPLES - 1) as f32).cos();
    }

    let mut fbank_all = Vec::with_capacity(num_frames * NUM_MEL_BINS);
    let mut real = [0.0f32; FFT_SIZE];
    let mut imag = [0.0f32; FFT_SIZE];

    for frame_idx in 0..num_frames {
        let start_sample = frame_idx * FRAME_SHIFT_SAMPLES;
        let frame_slice = &samples_16k[start_sample..start_sample + FRAME_LENGTH_SAMPLES];

        // Масштабирование до 16-bit PCM как ожидает Kaldi/torchaudio: [-32768, 32767]
        for i in 0..FRAME_LENGTH_SAMPLES {
            real[i] = frame_slice[i] * 32768.0 * window[i];
            imag[i] = 0.0;
        }
        for i in FRAME_LENGTH_SAMPLES..FFT_SIZE {
            real[i] = 0.0;
            imag[i] = 0.0;
        }

        fft_512(&mut real, &mut imag);

        // Спектр мощности
        let mut power_spec = [0.0f32; FFT_SIZE / 2 + 1];
        for i in 0..=FFT_SIZE / 2 {
            power_spec[i] = real[i] * real[i] + imag[i] * imag[i];
        }

        // Применение 80 Mel-фильтров + логарифм + CMVN
        for bin in 0..NUM_MEL_BINS {
            let mut mel_energy = 0.0f32;
            let filter = &filterbank[bin];
            for k in 0..=FFT_SIZE / 2 {
                mel_energy += power_spec[k] * filter[k];
            }
            let log_energy = (mel_energy.max(1e-10)).ln();
            let norm_val = (log_energy - cmvn.means[bin]) * cmvn.inv_stddevs[bin];
            fbank_all.push(norm_val);
        }
    }

    Ok(fbank_all)
}

/// Выполняет нейросетевую детекцию FireRedVAD на чистом Rust
pub fn detect_speech_firered(
    samples_16k: &[f32],
    threshold: f32,
    min_phrase_duration_sec: f32,
    min_silence_duration_sec: f32,
    max_phrase_duration_sec: f32,
    // Паддинг ставит общая постобработка в vad.rs, здесь он не нужен.
    _padding_sec: f32,
) -> Result<Vec<(f32, f32)>> {
    let fbank = extract_fbank_cmvn(samples_16k)?;
    if fbank.is_empty() {
        return Ok(Vec::new());
    }

    let num_frames = fbank.len() / NUM_MEL_BINS;

    // Инициализация ONNX модели через tract
    let mut cursor = std::io::Cursor::new(EMBEDDED_MODEL);
    let model = tract_onnx::onnx()
        .model_for_read(&mut cursor)
        .context("Не удалось прочитать встроенную ONNX модель FireRedVAD")?
        .into_optimized()
        .context("Не удалось оптимизировать ONNX модель FireRedVAD")?
        .into_runnable()
        .context("Не удалось скомпилировать ONNX модель FireRedVAD")?;

    // Инференс окнами по 30 секунд, а не одним тензором на весь файл:
    // раньше десятиминутное видео давало 60 000 кадров за раз, активации
    // сети раздувались на гигабайты и программа висла или падала по OOM.
    let mut probs: Vec<f32> = Vec::with_capacity(num_frames);
    let mut chunk_start = 0usize;

    while chunk_start < num_frames {
        let warmup = if chunk_start == 0 {
            0
        } else {
            CHUNK_WARMUP_FRAMES.min(chunk_start)
        };
        let from = chunk_start - warmup;
        let to = (chunk_start + CHUNK_FRAMES).min(num_frames);
        let len = to - from;

        let slice = fbank[from * NUM_MEL_BINS..to * NUM_MEL_BINS].to_vec();
        let feat_tensor: Tensor =
            tract_ndarray::Array3::from_shape_vec((1, len, NUM_MEL_BINS), slice)
                .context("Ошибка создания тензора fbank")?
                .into();

        let mut inputs: TVec<TValue> = tvec![feat_tensor.into()];
        for _ in 0..8 {
            let cache: Tensor = tract_ndarray::Array3::<f32>::zeros((1, 128, 10)).into();
            inputs.push(cache.into());
        }

        let outputs = model.run(inputs).context("Ошибка исполнения FireRedVAD в tract")?;
        let view = outputs[0]
            .to_array_view::<f32>()
            .context("Не удалось прочитать вероятности FireRedVAD")?;

        // Форма выхода у разных сборок модели отличается, поэтому читаем
        // плоским списком: жёсткий индекс [[0, t, 0]] падал на 2D-выходе.
        let shape = view.shape();
        let channels = if shape.len() >= 3 { shape[2].max(1) } else { 1 };
        let flat: Vec<f32> = view.iter().copied().collect();
        let frames_out = flat.len() / channels;

        for t in warmup..len {
            if t >= frames_out {
                break;
            }
            let raw_p = flat[t * channels];
            let prob = if (0.0..=1.0).contains(&raw_p) {
                raw_p
            } else {
                1.0 / (1.0 + (-raw_p).exp())
            };
            probs.push(prob);
        }

        // Если сеть вернула меньше кадров, чем окно, добираем тишиной:
        // иначе все следующие фразы уехали бы по времени.
        while probs.len() < to {
            probs.push(0.0);
        }

        chunk_start = to;
    }

    // Постобработка вероятностей: поиск границ речи
    let frame_shift_sec = 0.010f32; // 10ms
    let min_speech_frames = (min_phrase_duration_sec / frame_shift_sec).round().max(1.0) as usize;
    let min_silence_frames = (min_silence_duration_sec / frame_shift_sec).round().max(1.0) as usize;
    let total_duration = samples_16k.len() as f32 / SAMPLE_RATE as f32;

    let mut raw_segments = Vec::new();
    let mut in_speech = false;
    let mut start_frame = 0;
    let mut silence_count = 0;

    for (i, &prob) in probs.iter().enumerate() {
        if prob >= threshold {
            if !in_speech {
                in_speech = true;
                start_frame = i;
            }
            silence_count = 0;
        } else if in_speech {
            silence_count += 1;
            if silence_count >= min_silence_frames {
                let end_frame = i.saturating_sub(silence_count).saturating_add(1);
                if end_frame.saturating_sub(start_frame) >= min_speech_frames {
                    raw_segments.push((
                        start_frame as f32 * frame_shift_sec,
                        end_frame as f32 * frame_shift_sec,
                    ));
                }
                in_speech = false;
                silence_count = 0;
            }
        }
    }

    if in_speech && (probs.len().saturating_sub(start_frame)) >= min_speech_frames {
        raw_segments.push((
            start_frame as f32 * frame_shift_sec,
            probs.len() as f32 * frame_shift_sec,
        ));
    }

    // Деление длинных фраз и применение отступов
    let mut final_segments = Vec::new();
    for (start, end) in raw_segments {
        let mut cur_start = start;
        while (end - cur_start) > max_phrase_duration_sec {
            let mut split_at = cur_start + max_phrase_duration_sec;
            let mid_frame = (split_at / frame_shift_sec).round() as usize;
            let search_radius = (0.8 / frame_shift_sec).round() as usize;

            let s_min = ((cur_start + 1.0) / frame_shift_sec).round() as usize;
            let s_max = ((end - 0.5) / frame_shift_sec).round() as usize;

            let search_start = s_min.max(mid_frame.saturating_sub(search_radius));
            let search_end = s_max.min(mid_frame + search_radius);

            if search_end > search_start && search_end <= probs.len() {
                let mut min_idx = search_start;
                let mut min_val = probs[search_start];
                for (k, &p) in probs.iter().enumerate().take(search_end).skip(search_start + 1) {
                    if p < min_val {
                        min_val = p;
                        min_idx = k;
                    }
                }
                split_at = min_idx as f32 * frame_shift_sec;
            }

            // Отступы здесь больше не добавляем: их аккуратно ставит единая
            // постобработка (vad::apply_padding) с оглядкой на соседей. Раньше
            // паддинг накладывался дважды, и фразы заезжали друг на друга:
            // на стыках слышался один и тот же кусок звука два раза.
            let s_adj = cur_start.max(0.0);
            let e_adj = split_at.min(total_duration);
            if e_adj > s_adj {
                final_segments.push((s_adj, e_adj));
            }
            cur_start = split_at;
        }

        let s_adj = cur_start.max(0.0);
        let e_adj = end.min(total_duration);
        if e_adj > s_adj {
            final_segments.push((s_adj, e_adj));
        }
    }

    Ok(final_segments)
}
