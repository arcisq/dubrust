//! Базовый DSP: ресемплинг, растяжение по времени, гейны, фейды и лимитер.

/// Ресемплинг кубической интерполяцией Эрмита.
/// Микрофон почти всегда пишет 48000 Гц, а дорожка видео — 44100 Гц:
/// без этого шага голос в финале звучит ниже и медленнее примерно на 9%.
pub fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if samples.is_empty() || from_rate == 0 || to_rate == 0 || from_rate == to_rate {
        return samples.to_vec();
    }

    let ratio = to_rate as f64 / from_rate as f64;
    let out_len = ((samples.len() as f64) * ratio).round() as usize;
    let mut out = Vec::with_capacity(out_len);

    let last = samples.len() as isize - 1;
    let get = |i: isize| -> f32 { samples[i.clamp(0, last) as usize] };

    for n in 0..out_len {
        let pos = n as f64 / ratio;
        let i = pos.floor() as isize;
        let t = (pos - i as f64) as f32;

        let p0 = get(i - 1);
        let p1 = get(i);
        let p2 = get(i + 1);
        let p3 = get(i + 2);

        let c0 = p1;
        let c1 = 0.5 * (p2 - p0);
        let c2 = p0 - 2.5 * p1 + 2.0 * p2 - 0.5 * p3;
        let c3 = 0.5 * (p3 - p0) + 1.5 * (p1 - p2);

        out.push((((c3 * t + c2) * t + c1) * t + c0).clamp(-1.0, 1.0));
    }

    out
}

/// Растяжение/сжатие по времени методом OLA — без изменения высоты голоса.
/// `factor` — во сколько раз результат должен быть длиннее исходника.
pub fn time_stretch(samples: &[f32], factor: f32, sample_rate: u32) -> Vec<f32> {
    if samples.is_empty() || !factor.is_finite() || (factor - 1.0).abs() < 0.01 {
        return samples.to_vec();
    }

    let factor = factor.clamp(0.25, 4.0);
    let frame = ((sample_rate as f32 * 0.05) as usize).max(128);
    let hop_out = (frame / 2).max(1);
    let hop_in = ((hop_out as f32 / factor).round() as usize).max(1);
    // Окно поиска фазы ±10 мс — хватает на период любого мужского голоса
    let search = ((sample_rate as f32 * 0.01) as usize).max(1);
    let out_len = ((samples.len() as f32 * factor).round() as usize).max(1);

    // Окно Ханна считаем один раз, а не косинус на каждый семпл
    let window: Vec<f32> = (0..frame)
        .map(|i| {
            let phase = 2.0 * std::f32::consts::PI * i as f32 / frame as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect();

    let mut out = vec![0.0f32; out_len + frame];
    let mut window_sum = vec![0.0f32; out_len + frame];

    let mut pos_in = 0usize;
    let mut pos_out = 0usize;

    while pos_in < samples.len() && pos_out < out_len {
        // WSOLA: берём кадр не строго по сетке, а там, где он совпадает по фазе
        let offset = if pos_out >= hop_out {
            best_offset(samples, &out, pos_in, pos_out, hop_out, search)
        } else {
            0
        };

        let start = (pos_in as isize + offset).clamp(0, samples.len() as isize - 1) as usize;
        let n = frame.min(samples.len() - start);

        for i in 0..n {
            out[pos_out + i] += samples[start + i] * window[i];
            window_sum[pos_out + i] += window[i];
        }

        pos_in += hop_in;
        pos_out += hop_out;
    }

    out.truncate(out_len);
    window_sum.truncate(out_len);

    for (sample, w) in out.iter_mut().zip(window_sum.into_iter()) {
        if w > 1e-4 {
            *sample /= w;
        }
        *sample = sample.clamp(-1.0, 1.0);
    }

    out
}

/// Пиковое значение по модулю
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |acc, s| acc.max(s.abs()))
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Нормализация пика до целевого уровня (тишина не разгоняется)
pub fn normalize_peak(samples: &mut [f32], target: f32) {
    let current = peak(samples);
    if current < 1e-4 || target <= 0.0 {
        return;
    }
    let gain = (target / current).clamp(0.1, 8.0);
    for s in samples.iter_mut() {
        *s *= gain;
    }
}

pub fn apply_gain(samples: &mut [f32], gain: f32) {
    if (gain - 1.0).abs() < f32::EPSILON {
        return;
    }
    for s in samples.iter_mut() {
        *s *= gain;
    }
}

/// Мягкое ограничение вместо жёсткого clamp — нет хрипа на громких стыках
pub fn soft_clip(x: f32) -> f32 {
    const KNEE: f32 = 0.7;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

/// Линейный переход усиления от `from` к `to` по всему срезу
pub fn ramp_gain(samples: &mut [f32], from: f32, to: f32) {
    let n = samples.len();
    if n == 0 {
        return;
    }
    if n == 1 {
        samples[0] *= to;
        return;
    }
    for (i, s) in samples.iter_mut().enumerate() {
        let t = i as f32 / (n - 1) as f32;
        *s *= from + (to - from) * t;
    }
}

pub fn fade_in(samples: &mut [f32], len: usize) {
    let n = len.min(samples.len());
    if n == 0 {
        return;
    }
    ramp_gain(&mut samples[..n], 0.0, 1.0);
}

pub fn fade_out(samples: &mut [f32], len: usize) {
    let total = samples.len();
    let n = len.min(total);
    if n == 0 {
        return;
    }
    ramp_gain(&mut samples[total - n..], 1.0, 0.0);
}

/// Убрать постоянную составляющую (DC-offset).
/// Дешёвые микрофоны часто дают сдвиг нуля, из-за которого падает запас до клипа.
pub fn remove_dc(samples: &mut [f32]) {
    if samples.is_empty() {
        return;
    }
    let mean = samples.iter().sum::<f32>() / samples.len() as f32;
    if mean.abs() < 1e-6 {
        return;
    }
    for s in samples.iter_mut() {
        *s -= mean;
    }
}

/// Оценка уровня шума: 10-й перцентиль по коротким окнам RMS.
/// Кулер и кондиционер дают ровный фон, и по тихим окнам он виден точнее, чем по пику.
pub fn noise_floor(samples: &[f32], sample_rate: u32) -> f32 {
    let window = ((sample_rate as f32 * 0.02) as usize).max(32);
    if samples.len() < window * 4 {
        return 0.0;
    }

    let mut levels: Vec<f32> = samples.chunks(window).map(rms).collect();
    levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let index = ((levels.len() as f32 * 0.1) as usize).min(levels.len() - 1);
    levels[index]
}

/// Однополюсный ФВЧ: срезает гул кулера, рокот стола и сетевой фон.
/// Речь ниже 80 Гц практически не живёт, а вентилятор — живёт именно там.
pub fn highpass(samples: &mut [f32], cutoff_hz: f32, sample_rate: u32) {
    if samples.is_empty() || sample_rate == 0 || cutoff_hz <= 0.0 {
        return;
    }

    let dt = 1.0 / sample_rate as f32;
    let rc = 1.0 / (2.0 * std::f32::consts::PI * cutoff_hz);
    let alpha = rc / (rc + dt);

    let mut prev_in = samples[0];
    let mut prev_out = 0.0f32;

    for s in samples.iter_mut() {
        let x = *s;
        let y = alpha * (prev_out + x - prev_in);
        prev_in = x;
        prev_out = y;
        *s = y;
    }
}

/// Плавный шумодав (экспандер) с атакой и отпусканием.
/// Жёсткий гейт как раз даёт эффект «вентилятор говорит»: фон дёргается вслед за голосом.
/// Здесь усиление ездит плавно и с гистерезисом, поэтому фон не пульсирует.
pub fn noise_gate(samples: &mut [f32], threshold: f32, strength: f32, sample_rate: u32) {
    let strength = strength.clamp(0.0, 1.0);
    if samples.is_empty() || sample_rate == 0 || threshold <= 0.0 || strength <= 0.0 {
        return;
    }

    let attack = (-1.0 / (0.005 * sample_rate as f32)).exp();
    let release = (-1.0 / (0.120 * sample_rate as f32)).exp();

    let open = threshold * 2.5;
    let close = threshold * 1.2;
    let floor_gain = 1.0 - strength;

    let mut env = 0.0f32;
    let mut gain = floor_gain;

    for s in samples.iter_mut() {
        let x = s.abs();
        let env_coeff = if x > env { attack } else { release };
        env = x + env_coeff * (env - x);

        let target = if env >= open {
            1.0
        } else if env <= close {
            floor_gain
        } else {
            // Плавная S-кривая вместо ступеньки
            let t = ((env - close) / (open - close)).clamp(0.0, 1.0);
            floor_gain + (1.0 - floor_gain) * t * t * (3.0 - 2.0 * t)
        };

        let gain_coeff = if target > gain { attack } else { release };
        gain = target + gain_coeff * (gain - target);

        *s *= gain;
    }
}

/// Полная чистка дубля перед сведением: постоянка, гул, фон.
/// Гейт включается только когда шум реально есть и он тише голоса:
/// иначе на тихой записи он съедает начала слов.
pub fn cleanup_voice(samples: &mut [f32], sample_rate: u32, highpass_hz: f32, gate_strength: f32) {
    if samples.is_empty() {
        return;
    }

    remove_dc(samples);
    highpass(samples, highpass_hz, sample_rate);

    if gate_strength <= 0.0 {
        return;
    }

    let floor = noise_floor(samples, sample_rate);
    let voice = rms(samples);

    if floor > 1e-4 && voice > floor * 2.0 {
        noise_gate(samples, floor * 1.8, gate_strength, sample_rate);
    }
}

/// Подбор сдвига кадра по максимуму корреляции с уже уложенным хвостом.
/// Это и есть отличие WSOLA от голого OLA: без поиска фазы голос металлит.
fn best_offset(
    samples: &[f32],
    out: &[f32],
    pos_in: usize,
    pos_out: usize,
    hop_out: usize,
    search: usize,
) -> isize {
    let len = hop_out.min(256);
    if len == 0 || pos_out + len > out.len() {
        return 0;
    }

    let mut best = 0isize;
    let mut best_score = f32::MIN;
    let mut offset = -(search as isize);

    while offset <= search as isize {
        let start = pos_in as isize + offset;
        if start >= 0 && (start as usize) + len <= samples.len() {
            let start = start as usize;
            let mut score = 0.0f32;
            for i in 0..len {
                score += samples[start + i] * out[pos_out + i];
            }
            if score > best_score {
                best_score = score;
                best = offset;
            }
        }
        offset += 8;
    }

    best
}
