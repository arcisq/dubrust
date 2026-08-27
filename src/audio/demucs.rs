//! Модуль нейросетевого разделения вокала и фона (HT-Demucs).
//! Архитектура динамической загрузки (Load-Dynamic):
//! 1. Базовый dubrust.exe весит 30-40 МБ и не зависит от каких-либо DLL при запуске.
//! 2. При включении опции HT-Demucs, модель и onnxruntime.dll скачиваются по запросу в %APPDATA%\dubrust\.
//! 3. Библиотека onnxruntime.dll подгружается динамически через libloading в момент запуска разделения.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub const DEMUCS_HF_URL: &str =
    "https://huggingface.co/StemSplitio/htdemucs-onnx/resolve/main/htdemucs_fp16weights.onnx";

pub const ONNXRUNTIME_DLL_URL: &str =
    "https://github.com/microsoft/onnxruntime/releases/download/v1.19.2/onnxruntime-win-x64-1.19.2.zip";

pub const DEMUCS_MODEL_FILENAME: &str = "htdemucs_fp16weights.onnx";
pub const ONNXRUNTIME_DLL_FILENAME: &str = "onnxruntime.dll";

static ORT_INIT: OnceLock<Result<(), String>> = OnceLock::new();

pub fn get_app_dir() -> PathBuf {
    // Портативная сборка держит веса и DLL рядом с собой, а не в %APPDATA%.
    if let Some(dir) = crate::util::portable_data_dir() {
        return dir;
    }

    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join("dubrust");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }

    if let Ok(home) = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")) {
        let p = PathBuf::from(home).join(".cache").join("dubrust");
        let _ = std::fs::create_dir_all(&p);
        return p;
    }

    PathBuf::from("data")
}

pub fn get_models_dir() -> PathBuf {
    let p = get_app_dir().join("models");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn get_bin_dir() -> PathBuf {
    let p = get_app_dir().join("bin");
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn find_demucs_model() -> Option<PathBuf> {
    let appdata_path = get_models_dir().join(DEMUCS_MODEL_FILENAME);
    if appdata_path.is_file() {
        if let Ok(meta) = std::fs::metadata(&appdata_path) {
            if meta.len() > 10_000_000 {
                return Some(appdata_path);
            }
        }
    }

    let local_path = PathBuf::from("models").join(DEMUCS_MODEL_FILENAME);
    if local_path.is_file() {
        if let Ok(meta) = std::fs::metadata(&local_path) {
            if meta.len() > 10_000_000 {
                return Some(local_path);
            }
        }
    }

    None
}

pub fn find_onnxruntime_dll() -> Option<PathBuf> {
    let appdata_dll = get_bin_dir().join(ONNXRUNTIME_DLL_FILENAME);
    if appdata_dll.is_file() {
        return Some(appdata_dll);
    }

    let local_dll = PathBuf::from(ONNXRUNTIME_DLL_FILENAME);
    if local_dll.is_file() {
        return Some(local_dll);
    }

    let bin_dll = PathBuf::from("bin").join(ONNXRUNTIME_DLL_FILENAME);
    if bin_dll.is_file() {
        return Some(bin_dll);
    }

    None
}

pub fn is_demucs_ready() -> bool {
    find_demucs_model().is_some() && find_onnxruntime_dll().is_some()
}

/// Динамическая инициализация среды ONNX Runtime из скачанной DLL.
///
/// Раньше здесь был `Once`: ошибка первой попытки терялась, а все
/// следующие вызовы молча возвращали Ok при неинициализированном ORT —
/// и программа падала уже внутри инференса. Теперь результат запоминается.
pub fn ensure_ort_initialized() -> Result<()> {
    let result = ORT_INIT.get_or_init(|| match find_onnxruntime_dll() {
        Some(dll_path) => match ort::init_from(&dll_path) {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("Ошибка инициализации ort из {:?}: {e}", dll_path)),
        },
        None => Err("onnxruntime.dll не найден".to_string()),
    });

    match result {
        Ok(()) => Ok(()),
        Err(err) => Err(anyhow!(err.clone())),
    }
}

/// Скачивает веса HT-Demucs и onnxruntime.dll с официальных источников
pub fn download_demucs_model<F>(mut on_progress: F) -> Result<PathBuf>
where
    F: FnMut(f32, u64, u64),
{
    let target_dir = get_models_dir();
    let target_model = target_dir.join(DEMUCS_MODEL_FILENAME);
    let bin_dir = get_bin_dir();
    let target_dll = bin_dir.join(ONNXRUNTIME_DLL_FILENAME);

    // 1. Скачиваем onnxruntime.dll если его ещё нет
    if !target_dll.exists() {
        on_progress(0.02, 1, 100);
        let temp_zip = bin_dir.join("ort_temp.zip");
        let resp = ureq::get(ONNXRUNTIME_DLL_URL)
            .set("User-Agent", "DubRust/0.1.0")
            .call()
            .context("Не удалось подключиться к GitHub Releases для скачивания onnxruntime.dll")?;

        let mut reader = resp.into_reader();
        let mut zip_file = File::create(&temp_zip)?;
        std::io::copy(&mut reader, &mut zip_file)?;
        drop(zip_file);

        // Распаковываем onnxruntime.dll из архива
        let file = File::open(&temp_zip)?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut extracted = false;
        for i in 0..archive.len() {
            let mut item = archive.by_index(i)?;
            if item.name().ends_with("onnxruntime.dll") {
                let mut out = File::create(&target_dll)?;
                std::io::copy(&mut item, &mut out)?;
                extracted = true;
                break;
            }
        }
        let _ = std::fs::remove_file(&temp_zip);
        if !extracted {
            return Err(anyhow!("onnxruntime.dll не найден внутри скачанного архива"));
        }
    }

    // 2. Скачиваем веса HT-Demucs с Hugging Face
    let temp_model = target_dir.join(format!("{}.download", DEMUCS_MODEL_FILENAME));
    let response = ureq::get(DEMUCS_HF_URL)
        .set("User-Agent", "DubRust/0.1.0")
        .call()
        .context("Не удалось подключиться к Hugging Face")?;

    let total_bytes = response
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(165_612_636);

    let mut reader = response.into_reader();
    let mut file = File::create(&temp_model)?;

    let mut downloaded: u64 = 0;
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buffer[..bytes_read])?;
        downloaded += bytes_read as u64;

        let progress = (downloaded as f32 / total_bytes as f32).clamp(0.0, 1.0);
        on_progress(progress, downloaded, total_bytes);
    }

    file.sync_all()?;
    drop(file);

    std::fs::rename(&temp_model, &target_model)?;
    Ok(target_model)
}

/// Настоящий нейросетевой инференс HT-Demucs на фрагментах аудио
fn separate_with_neural_htdemucs(
    original_wav_path: &Path,
    output_bg_wav_path: &Path,
    model_path: &Path,
) -> Result<()> {
    ensure_ort_initialized()?;

    let mut session = ort::session::Session::builder()
        .map_err(|e| anyhow!("ort error: {e}"))?
        .with_intra_threads(4)
        .map_err(|e| anyhow!("ort error: {e}"))?
        .with_inter_threads(1)
        .map_err(|e| anyhow!("ort error: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow!("Не удалось создать сессию HT-Demucs: {e}"))?;

    let audio = crate::audio::wav::read_wav_mono(original_wav_path)?;
    let sample_rate = audio.sample_rate;
    let total_samples = audio.samples.len();

    if total_samples == 0 {
        return Ok(());
    }

    // Окно инференса HT-Demucs: 343980 сэмплов (~7.8 секунд)
    let chunk_size = 343980;
    let step_size = (chunk_size as f32 * 0.75) as usize; // 25% перекрытие для гладкого сшивания

    let mut background_accum = vec![0.0f32; total_samples];
    let mut weight_accum = vec![0.0f32; total_samples];

    let mut cursor = 0;
    while cursor < total_samples {
        let end = (cursor + chunk_size).min(total_samples);
        let actual_len = end - cursor;

        let mut input_chunk = vec![0.0f32; 2 * chunk_size];
        for i in 0..actual_len {
            let sample = audio.samples[cursor + i];
            input_chunk[i] = sample; // Left
            input_chunk[chunk_size + i] = sample; // Right
        }

        let input_tensor = ort::value::Tensor::from_array(([1, 2, chunk_size], input_chunk))?;
        let outputs = session.run(ort::inputs![input_tensor])?;

        // Извлекаем stems: [1, 4, 2, chunk_size]
        // Stems: 0=drums, 1=bass, 2=other, 3=vocals
        if let Some((_, tensor_val)) = outputs.iter().next() {
            let (_shape, data) = tensor_val.try_extract_tensor::<f32>()?;
            // Background = drums + bass + other
            let stem_stride = 2 * chunk_size;

            if data.len() >= 3 * stem_stride {
                // Трапеция вместо косинуса на всю длину окна: склоны ровно по
                // зоне перехлёста, в середине ровно 1.0. Косинусное окно при шаге 75%
                // не суммировалось в единицу — фон «дышал» громкостью, а самое начало
                // и самый конец дорожки уходили в ноль.
                let fade = chunk_size.saturating_sub(step_size).max(1);
                let at_start = cursor == 0;
                let at_end = end == total_samples;

                for i in 0..actual_len {
                    let head = if at_start {
                        1.0
                    } else {
                        ((i as f32 + 0.5) / fade as f32).min(1.0)
                    };
                    let tail = if at_end {
                        1.0
                    } else {
                        ((actual_len - i) as f32 / fade as f32).min(1.0)
                    };
                    let w = head.min(tail).max(1e-3);

                    // drums + bass + other (моно сумма левого и правого)
                    let drums = (data[i] + data[chunk_size + i]) * 0.5;
                    let bass = (data[1 * stem_stride + i] + data[1 * stem_stride + chunk_size + i]) * 0.5;
                    let other = (data[2 * stem_stride + i] + data[2 * stem_stride + chunk_size + i]) * 0.5;

                    let bg_sample = drums + bass + other;

                    background_accum[cursor + i] += bg_sample * w;
                    weight_accum[cursor + i] += w;
                }
            }
        }

        if end == total_samples {
            break;
        }
        cursor += step_size;
    }

    // Нормализация по весам перекрытия
    for i in 0..total_samples {
        if weight_accum[i] > 1e-4 {
            background_accum[i] /= weight_accum[i];
        }
    }

    crate::audio::wav::write_wav_mono(output_bg_wav_path, &background_accum, sample_rate)?;
    Ok(())
}

/// Выполняет разделение оригинального аудио на вокал и фоновый шум/музыку (BGM).
/// Если скачаны веса HT-Demucs и DLL — запускает полный нейросетевой инференс.
/// Если нет — использует быстрое противофазное подавление.
pub fn extract_background_track(
    original_wav_path: &Path,
    output_bg_wav_path: &Path,
) -> Result<()> {
    if let (Some(model), Some(_dll)) = (find_demucs_model(), find_onnxruntime_dll()) {
        if let Ok(()) = separate_with_neural_htdemucs(original_wav_path, output_bg_wav_path, &model) {
            return Ok(());
        }
    }

    // Быстрый DSP фоллбэк: противофазное вычитание речевого центра + Notch
    let audio = crate::audio::wav::read_wav_mono(original_wav_path)?;
    let rate = audio.sample_rate as f32;
    let mut bg_samples = Vec::with_capacity(audio.samples.len());

    let w_lp = 2.0 * std::f32::consts::PI * 180.0 / rate;
    let alpha_lp = w_lp / (1.0 + w_lp);
    let mut lp_state = 0.0f32;

    let w_hp = 2.0 * std::f32::consts::PI * 4200.0 / rate;
    let alpha_hp = 1.0 / (1.0 + w_hp);
    let mut hp_prev_in = 0.0f32;
    let mut hp_state = 0.0f32;

    for &s in &audio.samples {
        lp_state += alpha_lp * (s - lp_state);
        hp_state = alpha_hp * (hp_state + s - hp_prev_in);
        hp_prev_in = s;
        let speech_band = (s - lp_state - hp_state) * 0.02;
        let bg = (lp_state * 1.1 + hp_state * 1.1 + speech_band).clamp(-1.0, 1.0);
        bg_samples.push(bg);
    }

    crate::audio::wav::write_wav_mono(output_bg_wav_path, &bg_samples, audio.sample_rate)?;
    Ok(())
}
