// DubRust — студия дубляжа и переозвучки видео.
// Copyright (C) 2026 brawrel228

//! Детектор речи и нарезка на фразы.
//!
//! Первая версия сравнивала громкость каждого кадра с одним порогом
//! `max(max_rms * 0.08, настройка)` — и ошибалась четырьмя способами:
//!
//! 1. Порог зависел от самого громкого места дорожки: один крик или взрыв
//!    в саундтреке — и вся обычная речь оказывалась «тишиной».
//! 2. Один и тот же порог на вход и на выход. Конец фразы всегда тише
//!    середины, поэтому фраза закрывалась раньше времени.
//! 3. Учитывалась только громкость. Шипящие и глухие (с, ш, т, ф, х) по
//!    энергии почти не отличаются от шума — именно окончания слов терялись.
//! 4. Слишком длинная реплика резалась на равные части — ровно посередине слова.
//!
//! Сейчас: шумовой пол оценивается по перцентилю, порогов два (гистерезис),
//! глухие согласные ловятся по частоте переходов через ноль, у начала есть
//! откат назад, у конца — хвост, а длинные фразы делятся в самой тихой точке.

use std::path::Path;

use anyhow::{anyhow, Result};

use super::firered::detect_speech_firered;
use crate::audio::dsp;
use crate::audio::wav::{read_wav_mono, AudioBuffer};
use crate::i18n::Lang;
use crate::models::{PhraseSegment, SlicerConfig};

/// Длина анализируемого окна и шаг между окнами
const FRAME_SEC: f32 = 0.020;
const HOP_SEC: f32 = 0.010;

/// Сколько держать хвост после последнего речевого кадра
const TAIL_SEC: f32 = 0.08;
/// Насколько можно откатить начало назад, чтобы забрать мягкую атаку
const LEAD_SEC: f32 = 0.14;

/// Какой движок реально дал результат
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEngine {
    FireRed,
    Dsp,
}

impl VadEngine {
    pub fn label(self, lang: Lang) -> &'static str {
        let strings = lang.strings();
        match self {
            VadEngine::FireRed => strings.engine_firered,
            VadEngine::Dsp => strings.engine_dsp,
        }
    }
}

/// Отчёт о нарезке: важно видеть, почему нейросеть не сработала,
/// а не молча получать худший результат.
#[derive(Debug, Clone)]
pub struct SliceReport {
    pub engine: VadEngine,
    pub warning: Option<String>,
}

type Range = (f32, f32);

/// Нарезка WAV на фразы.
pub fn detect_phrases_from_wav(path: &Path, config: &SlicerConfig) -> Result<Vec<PhraseSegment>> {
    Ok(detect_phrases_from_wav_verbose(path, config)?.0)
}

/// Нарезка с отчётом о использованном движке.
pub fn detect_phrases_from_wav_verbose(
    path: &Path,
    config: &SlicerConfig,
) -> Result<(Vec<PhraseSegment>, SliceReport)> {
    let buffer = read_wav_mono(path)?;
    let total_duration = buffer.duration_sec();

    if total_duration <= 0.0 {
        return Err(anyhow!("Аудиодорожка пуста: {:?}", path));
    }

    // Признаки считаем один раз: они нужны и для DSP-поиска, и для
    // аккуратного деления длинных фраз после нейросети.
    let features = Features::new(&buffer);
    let mut warning = None;

    match config.engine {
        crate::models::SlicerEngine::FireRedVad => {
            let samples_16k = if buffer.sample_rate == 16000 {
                buffer.samples.clone()
            } else {
                dsp::resample(&buffer.samples, buffer.sample_rate, 16000)
            };

            match detect_speech_firered(
                &samples_16k,
                config.neural_threshold,
                config.min_phrase_duration_sec,
                config.min_silence_duration_sec,
                config.max_phrase_duration_sec,
                config.padding_sec,
            ) {
                Ok(ranges) => {
                    let segments = finalize_segments(ranges, total_duration, config, Some(&features));
                    if !segments.is_empty() {
                        return Ok((
                            segments,
                            SliceReport {
                                engine: VadEngine::FireRed,
                                warning: None,
                            },
                        ));
                    }
                    warning = Some("FireRedVAD не нашёл речи — включён встроенный DSP-детектор".to_string());
                }
                Err(err) => {
                    warning = Some(format!(
                        "FireRedVAD ошибка: {err}. Включён встроенный DSP-детектор"
                    ));
                }
            }
        }
        crate::models::SlicerEngine::Dsp => {}
    }

    let ranges = detect_ranges_dsp(&features, config);
    let segments = finalize_segments(ranges, total_duration, config, Some(&features));

    Ok((
        segments,
        SliceReport {
            engine: VadEngine::Dsp,
            warning,
        },
    ))
}

/// Нарезка уже декодированного буфера без внешних процессов.
pub fn detect_phrases_from_samples(
    buffer: &AudioBuffer,
    config: &SlicerConfig,
) -> Vec<PhraseSegment> {
    let total_duration = buffer.duration_sec();
    let features = Features::new(buffer);

    if config.engine == crate::models::SlicerEngine::FireRedVad {
        let samples_16k = if buffer.sample_rate == 16000 {
            buffer.samples.clone()
        } else {
            dsp::resample(&buffer.samples, buffer.sample_rate, 16000)
        };

        if let Ok(ranges) = detect_speech_firered(
            &samples_16k,
            config.neural_threshold,
            config.min_phrase_duration_sec,
            config.min_silence_duration_sec,
            config.max_phrase_duration_sec,
            config.padding_sec,
        ) {
            let segments = finalize_segments(ranges, total_duration, config, Some(&features));
            if !segments.is_empty() {
                return segments;
            }
        }
    }

    let ranges = detect_ranges_dsp(&features, config);
    finalize_segments(ranges, total_duration, config, Some(&features))
}

/// Покадровые признаки дорожки
struct Features {
    sample_rate: u32,
    hop: usize,
    /// Громкость кадра в dBFS
    rms_db: Vec<f32>,
    /// Частота переходов через ноль: у шипящих она высокая
    zcr: Vec<f32>,
    /// Шумовой пол и типичный уровень речи, dBFS
    noise_db: f32,
    speech_db: f32,
}

impl Features {
    fn new(buffer: &AudioBuffer) -> Self {
        let sample_rate = buffer.sample_rate.max(1);
        let frame = ((sample_rate as f32 * FRAME_SEC) as usize).max(1);
        let hop = ((sample_rate as f32 * HOP_SEC) as usize).max(1);
        let samples = &buffer.samples;

        let capacity = samples.len() / hop + 1;
        let mut rms_db = Vec::with_capacity(capacity);
        let mut zcr = Vec::with_capacity(capacity);

        let mut pos = 0usize;
        while pos < samples.len() {
            let end = (pos + frame).min(samples.len());
            let window = &samples[pos..end];
            let level = dsp::rms(window).max(1e-7);
            rms_db.push(20.0 * level.log10());
            zcr.push(zero_crossing_rate(window));
            pos += hop;
        }

        // Перцентили вместо минимума и максимума: один щёлчок или одна
        // абсолютная тишина больше не сбивают всю настройку.
        let noise_db = percentile(&rms_db, 0.15);
        let speech_db = percentile(&rms_db, 0.92);

        Self {
            sample_rate,
            hop,
            rms_db,
            zcr,
            noise_db,
            speech_db,
        }
    }

    fn len(&self) -> usize {
        self.rms_db.len()
    }

    fn hop_sec(&self) -> f32 {
        self.hop as f32 / self.sample_rate as f32
    }

    fn time(&self, index: usize) -> f32 {
        index as f32 * self.hop_sec()
    }

    fn index(&self, time: f32) -> usize {
        if self.len() == 0 {
            return 0;
        }
        let raw = (time / self.hop_sec().max(1e-6)).round();
        (raw.max(0.0) as usize).min(self.len() - 1)
    }
}

fn zero_crossing_rate(window: &[f32]) -> f32 {
    if window.len() < 2 {
        return 0.0;
    }

    let mut crossings = 0usize;
    for pair in window.windows(2) {
        if (pair[0] >= 0.0) != (pair[1] >= 0.0) {
            crossings += 1;
        }
    }

    crossings as f32 / (window.len() - 1) as f32
}

fn percentile(values: &[f32], quantile: f32) -> f32 {
    if values.is_empty() {
        return -70.0;
    }

    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let last = sorted.len() - 1;
    let index = (last as f32 * quantile.clamp(0.0, 1.0)).round() as usize;
    sorted[index.min(last)]
}

/// Два порога вместо одного: открываем фразу строго, закрываем мягко.
struct Thresholds {
    open_db: f32,
    close_db: f32,
    floor_db: f32,
}

fn thresholds(features: &Features, config: &SlicerConfig) -> Thresholds {
    let noise = features.noise_db;
    let speech = features.speech_db.max(noise + 6.0);

    // Старый слайдер абсолютного порога (-60..-10 dB) теперь задаёт запас
    // над измеренным шумом: так настройка одинаково работает и на тихой,
    // и на громкой дорожке.
    let sensitivity = ((config.silence_threshold_db + 60.0) / 50.0).clamp(0.0, 1.0);
    let margin = 4.0 + sensitivity * 12.0;

    // Порог никогда не выше самой речи — иначе не найдётся ничего.
    let open_db = (noise + margin).min(speech - 3.0).max(noise + 2.5);
    let close_db = (open_db - 6.0).max(noise + 1.0);

    Thresholds {
        open_db,
        close_db,
        floor_db: noise,
    }
}

fn frames_for(seconds: f32) -> usize {
    ((seconds / HOP_SEC).round() as usize).max(1)
}

/// Поиск речи с гистерезисом
fn detect_ranges_dsp(features: &Features, config: &SlicerConfig) -> Vec<Range> {
    if features.len() == 0 {
        return Vec::new();
    }

    let th = thresholds(features, config);
    let min_silence = frames_for(config.min_silence_duration_sec.max(0.05));
    let tail = frames_for(TAIL_SEC);
    let lead = frames_for(LEAD_SEC);
    let onset_frames = 2usize;
    let last_frame = features.len() - 1;

    let mut ranges: Vec<Range> = Vec::new();
    let mut start: Option<usize> = None;
    let mut silence = 0usize;
    let mut onset = 0usize;

    for index in 0..features.len() {
        let loud = features.rms_db[index] >= th.open_db;

        // Шипящие тише гласных, но у них много переходов через ноль —
        // без этого конец фразы («…сь», «…ть») обрубался.
        let sustained = features.rms_db[index] >= th.close_db
            || (features.zcr[index] > 0.22 && features.rms_db[index] >= th.floor_db + 2.5);

        match start {
            None => {
                if loud {
                    onset += 1;
                    if onset >= onset_frames {
                        let begin = index + 1 - onset;
                        start = Some(backtrack_start(features, &th, begin, lead));
                        silence = 0;
                    }
                } else {
                    onset = 0;
                }
            }
            Some(begin) => {
                if sustained {
                    silence = 0;
                } else {
                    silence += 1;
                    if silence >= min_silence {
                        let last_voiced = index.saturating_sub(silence);
                        let end = (last_voiced + tail).min(last_frame);
                        ranges.push((features.time(begin), features.time(end) + FRAME_SEC));
                        start = None;
                        onset = 0;
                        silence = 0;
                    }
                }
            }
        }
    }

    if let Some(begin) = start {
        ranges.push((
            features.time(begin),
            features.time(last_frame) + FRAME_SEC,
        ));
    }

    ranges
}

/// Откат начала назад: мягкая атака («в…», «л…») тише порога входа.
fn backtrack_start(
    features: &Features,
    th: &Thresholds,
    begin: usize,
    limit: usize,
) -> usize {
    let stop = begin.saturating_sub(limit);
    let mut index = begin;

    while index > stop {
        let candidate = index - 1;
        let quiet = features.rms_db[candidate] < th.floor_db + 2.0 && features.zcr[candidate] < 0.20;
        if quiet {
            break;
        }
        index = candidate;
    }

    index
}

/// Общая пост-обработка для обоих движков: склейка обрывков, деление длинных
/// фраз в тишине, притягивание границ к минимумам и отступы.
fn finalize_segments(
    ranges: Vec<Range>,
    total_duration: f32,
    config: &SlicerConfig,
    features: Option<&Features>,
) -> Vec<PhraseSegment> {
    let mut list: Vec<Range> = ranges
        .into_iter()
        .map(|(start, end)| {
            (
                start.clamp(0.0, total_duration),
                end.clamp(0.0, total_duration),
            )
        })
        .filter(|(start, end)| end > start)
        .collect();

    list.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let min_gap = config.min_silence_duration_sec.max(0.0);
    let max_len = config.max_phrase_duration_sec.max(0.5);
    let min_len = config.min_phrase_duration_sec.max(0.0);

    let list = merge_ranges(list, min_gap, max_len);
    let list = split_long(list, max_len, min_len, features);
    let list = snap_ranges(list, features);
    let list = apply_padding(list, config.padding_sec.max(0.0), total_duration);

    list.into_iter()
        .filter(|(start, end)| end - start >= min_len)
        .enumerate()
        .map(|(index, (start, end))| PhraseSegment::new(index + 1, start, end))
        .collect()
}

/// Склейка соседей. Короткие обрывки (одно слово, вдох) притягиваются
/// охотнее: именно они раньше давали ощущение «фраза разорвана посередине».
fn merge_ranges(ranges: Vec<Range>, min_gap: f32, max_len: f32) -> Vec<Range> {
    let mut merged: Vec<Range> = Vec::with_capacity(ranges.len());

    for range in ranges {
        match merged.last_mut() {
            Some(last) => {
                let gap = range.0 - last.1;
                let fragment = (range.1 - range.0) < 0.25 || (last.1 - last.0) < 0.25;
                let allowed_gap = if fragment { min_gap * 2.0 } else { min_gap };
                let combined = range.1.max(last.1) - last.0;

                if gap < allowed_gap && combined <= max_len * 1.15 {
                    last.1 = last.1.max(range.1);
                } else {
                    merged.push(range);
                }
            }
            None => merged.push(range),
        }
    }

    merged
}

/// Деление слишком длинных реплик. Раньше делилось на равные части и резало
/// посередине слова; теперь рез идёт в самой тихой точке ближе к середине.
fn split_long(
    ranges: Vec<Range>,
    max_len: f32,
    min_len: f32,
    features: Option<&Features>,
) -> Vec<Range> {
    let mut out: Vec<Range> = Vec::with_capacity(ranges.len());

    for range in ranges {
        let mut stack = vec![range];

        while let Some((start, end)) = stack.pop() {
            if end - start <= max_len {
                out.push((start, end));
                continue;
            }

            let cut = quiet_point(features, start, end, min_len).unwrap_or((start + end) * 0.5);

            // Правую часть кладём первой, чтобы со стека снялась левая
            // и порядок фраз не перепутался.
            stack.push((cut, end));
            stack.push((start, cut));
        }
    }

    out
}

/// Самая тихая точка внутри фразы — естественное место реза.
fn quiet_point(
    features: Option<&Features>,
    start: f32,
    end: f32,
    min_len: f32,
) -> Option<f32> {
    let features = features?;
    if features.len() == 0 {
        return None;
    }

    // У самого края резать нельзя: получится огрызок вместо фразы.
    let margin = min_len.max(0.35);
    let from = start + margin;
    let to = end - margin;
    if to - from <= 0.05 {
        return None;
    }

    let first = features.index(from);
    let last = features.index(to);
    if last <= first {
        return None;
    }

    let middle = (start + end) * 0.5;
    let mut best = first;
    let mut best_score = f32::MAX;

    for index in first..=last {
        // Штраф за удалённость от середины: из двух равно тихих пауз
        // выбираем ту, что делит фразу ровнее.
        let time = features.time(index);
        let score = features.rms_db[index] + (time - middle).abs() * 1.5;
        if score < best_score {
            best_score = score;
            best = index;
        }
    }

    Some(features.time(best))
}

/// Притягивание границ к локальному минимуму громкости (±50 мс).
/// Граница перестаёт попадать в середину звука и не даёт щёлчков.
fn snap_ranges(ranges: Vec<Range>, features: Option<&Features>) -> Vec<Range> {
    let Some(features) = features else {
        return ranges;
    };

    if features.len() == 0 {
        return ranges;
    }

    let window = 0.05;

    ranges
        .into_iter()
        .map(|(start, end)| {
            let snapped_start = quietest_near(features, start, window);
            let snapped_end = quietest_near(features, end, window);

            if snapped_end - snapped_start >= 0.08 {
                (snapped_start, snapped_end)
            } else {
                (start, end)
            }
        })
        .collect()
}

fn quietest_near(features: &Features, time: f32, window: f32) -> f32 {
    let first = features.index((time - window).max(0.0));
    let last = features.index(time + window);
    if last <= first {
        return time;
    }

    let mut best = first;
    for index in first..=last {
        if features.rms_db[index] < features.rms_db[best] {
            best = index;
        }
    }

    features.time(best)
}

/// Отступы. Ограничены долей паузы, поэтому фразы не заезжают друг на друга.
/// Хвосту даём больше: жалоба была именно на обрезанные концы фраз.
fn apply_padding(ranges: Vec<Range>, padding: f32, total_duration: f32) -> Vec<Range> {
    let count = ranges.len();
    let mut out: Vec<Range> = Vec::with_capacity(count);

    for (index, (start, end)) in ranges.iter().copied().enumerate() {
        let prev_end = if index == 0 {
            0.0
        } else {
            ranges[index - 1].1
        };
        let next_start = if index + 1 < count {
            ranges[index + 1].0
        } else {
            total_duration
        };

        let lead_room = ((start - prev_end).max(0.0)) * 0.45;
        let tail_room = ((next_start - end).max(0.0)) * 0.45;

        let mut new_start = (start - padding.min(lead_room)).max(0.0);
        let mut new_end = (end + (padding * 1.6).min(tail_room)).min(total_duration);

        // Страховка от наложений после притягивания границ.
        if let Some(last) = out.last() {
            new_start = new_start.max(last.1);
        }
        if new_end <= new_start {
            new_end = (new_start + 0.05).min(total_duration);
        }

        out.push((new_start, new_end));
    }

    out
}
