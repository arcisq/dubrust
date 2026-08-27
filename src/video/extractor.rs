use std::path::Path;

use anyhow::{anyhow, Context, Result};
use image::RgbImage;

use crate::models::MediaInfo;
use crate::util::{hidden_command, tail_lines};

/// Есть ли внешний инструмент в PATH
pub fn tool_available(tool: &str) -> bool {
    hidden_command(tool)
        .arg("-version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Проверка окружения при старте — без ffmpeg приложение молча ничего не делало
pub fn check_tools() -> Result<()> {
    let ffmpeg = tool_available("ffmpeg");
    let ffprobe = tool_available("ffprobe");

    if ffmpeg && ffprobe {
        return Ok(());
    }

    let mut missing = Vec::new();
    if !ffmpeg {
        missing.push("ffmpeg");
    }
    if !ffprobe {
        missing.push("ffprobe");
    }

    Err(anyhow!(
        "Не найдено в PATH: {}. Установите ffmpeg и перезапустите приложение",
        missing.join(", ")
    ))
}

fn parse_frame_rate(value: &str) -> Option<f32> {
    let mut parts = value.split('/');
    let numerator: f32 = parts.next()?.trim().parse().ok()?;
    match parts.next() {
        Some(denominator) => {
            let denominator: f32 = denominator.trim().parse().ok()?;
            if denominator.abs() < f32::EPSILON {
                None
            } else {
                Some(numerator / denominator)
            }
        }
        None => Some(numerator),
    }
}

/// Метаданные файла через ffprobe в JSON.
/// Старый вариант брал width/height из плоского вывода и попадал на последний поток.
pub fn probe_media(path: &Path) -> Result<MediaInfo> {
    let output = hidden_command("ffprobe")
        .args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"])
        .arg(path)
        .output()
        .context("Не удалось запустить ffprobe. Установлен ли ffmpeg?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("ffprobe: {}", tail_lines(&stderr, 2)));
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)
        .context("Не удалось разобрать ответ ffprobe")?;

    let mut info = MediaInfo::default();

    if let Some(duration) = json
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse::<f32>().ok())
    {
        info.duration_sec = duration.max(0.0);
    }

    if let Some(streams) = json.get("streams").and_then(|s| s.as_array()) {
        for stream in streams {
            let codec_type = stream.get("codec_type").and_then(|v| v.as_str()).unwrap_or("");

            match codec_type {
                "video" if !info.has_video => {
                    info.has_video = true;
                    if let Some(width) = stream.get("width").and_then(|v| v.as_u64()) {
                        info.width = width.max(1) as u32;
                    }
                    if let Some(height) = stream.get("height").and_then(|v| v.as_u64()) {
                        info.height = height.max(1) as u32;
                    }
                    let rate = stream
                        .get("avg_frame_rate")
                        .and_then(|v| v.as_str())
                        .and_then(parse_frame_rate)
                        .filter(|fps| *fps > 0.1)
                        .or_else(|| {
                            stream
                                .get("r_frame_rate")
                                .and_then(|v| v.as_str())
                                .and_then(parse_frame_rate)
                        });
                    if let Some(fps) = rate {
                        info.fps = fps.clamp(1.0, 240.0);
                    }
                }
                "audio" => info.has_audio = true,
                _ => {}
            }

            if info.duration_sec <= 0.0 {
                if let Some(duration) = stream
                    .get("duration")
                    .and_then(|v| v.as_str())
                    .and_then(|d| d.parse::<f32>().ok())
                {
                    info.duration_sec = duration.max(0.0);
                }
            }
        }
    }

    Ok(info)
}

/// Высота кадра для заданной ширины с сохранением пропорций (чётная)
pub fn scaled_height(info: &MediaInfo, target_width: u32) -> u32 {
    let width = info.width.max(1);
    let height = info.height.max(1);
    let scaled = (target_width as f32 * height as f32 / width as f32).round() as u32;
    let even = scaled.max(2) & !1;
    even.max(2)
}

/// Извлечение аудиодорожки в WAV 44100 Моно.
/// Если звуковой дорожки нет, создаётся тишина нужной длины,
/// иначе вся дальнейшая работа падала без объяснения.
pub fn extract_audio_from_video(video_path: &Path, output_wav: &Path) -> Result<()> {
    if let Some(parent) = output_wav.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let output = hidden_command("ffmpeg")
        .args(["-y", "-v", "error"])
        .arg("-i")
        .arg(video_path)
        .args([
            "-vn", "-sn", "-dn", "-map", "0:a:0", "-acodec", "pcm_s16le", "-ar", "44100", "-ac",
            "1",
        ])
        .arg(output_wav)
        .output()
        .context("Не удалось запустить ffmpeg. Установлен ли ffmpeg?")?;

    if output.status.success() && output_wav.exists() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Вторая попытка: возможно, у файла вообще нет звука
    let duration = probe_media(video_path).map(|info| info.duration_sec).unwrap_or(0.0);
    if duration > 0.0 {
        let silence = hidden_command("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=44100:cl=mono",
                "-t",
            ])
            .arg(format!("{duration:.3}"))
            .args(["-acodec", "pcm_s16le"])
            .arg(output_wav)
            .output();

        if let Ok(silence) = silence {
            if silence.status.success() && output_wav.exists() {
                return Ok(());
            }
        }
    }

    Err(anyhow!(
        "Не удалось извлечь аудио: {}",
        tail_lines(&stderr, 3)
    ))
}

/// Один кадр в RGB для скраба/паузы.
/// PNG вместо MJPEG — без артефактов сжатия и сдвига цвета.
pub fn extract_frame_rgb(video_path: &Path, time_sec: f32, target_width: u32) -> Result<RgbImage> {
    let target_width = target_width.clamp(64, 1920) & !1;

    let output = hidden_command("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(format!("{:.3}", time_sec.max(0.0)))
        .arg("-i")
        .arg(video_path)
        .args(["-frames:v", "1", "-an", "-sn", "-dn", "-vf"])
        .arg(format!("scale={target_width}:-2"))
        .args(["-f", "image2pipe", "-vcodec", "png", "pipe:1"])
        .output()
        .context("Не удалось запустить ffmpeg для кадра")?;

    if !output.status.success() || output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Кадр не получен: {}", tail_lines(&stderr, 2)));
    }

    let image = image::load_from_memory_with_format(&output.stdout, image::ImageFormat::Png)
        .context("Не удалось декодировать кадр")?;

    Ok(image.to_rgb8())
}
