use std::process::Command;

#[test]
fn test_vad_speech_detection() {
    let temp_dir = std::env::temp_dir().join("dubrust_test_vad");
    let _ = std::fs::create_dir_all(&temp_dir);
    let wav_path = temp_dir.join("test_speech.wav");

    // Создаем тестовый WAV файл: 1 сек тишина, 2 сек речь (синус 440Hz 0.5), 1 сек тишина, 1.5 сек речь (синус 880Hz 0.7), 1 сек тишина
    let sample_rate = 44100u32;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut writer = hound::WavWriter::create(&wav_path, spec).unwrap();

    // 0.0 - 1.0s: тишина
    for _ in 0..(sample_rate as usize) {
        writer.write_sample(0i16).unwrap();
    }

    // 1.0 - 3.0s: речь 1 (громкая)
    for i in 0..(sample_rate as usize * 2) {
        let t = i as f32 / sample_rate as f32;
        let val = (t * 440.0 * 2.0 * std::f32::consts::PI).sin() * 0.5;
        writer.write_sample((val * i16::MAX as f32) as i16).unwrap();
    }

    // 3.0 - 4.0s: пауза/тишина
    for _ in 0..(sample_rate as usize) {
        writer.write_sample(0i16).unwrap();
    }

    // 4.0 - 5.5s: речь 2
    for i in 0..((sample_rate as f32 * 1.5) as usize) {
        let t = i as f32 / sample_rate as f32;
        let val = (t * 880.0 * 2.0 * std::f32::consts::PI).sin() * 0.7;
        writer.write_sample((val * i16::MAX as f32) as i16).unwrap();
    }

    // 5.5 - 6.5s: тишина
    for _ in 0..(sample_rate as usize) {
        writer.write_sample(0i16).unwrap();
    }

    writer.finalize().unwrap();

    // Проверяем работу VAD в режиме DSP
    let config = dubrust::models::SlicerConfig {
        engine: dubrust::models::SlicerEngine::Dsp,
        use_neural_vad: false,
        neural_threshold: 0.40,
        silence_threshold_db: -25.0,
        min_silence_duration_sec: 0.3,
        min_phrase_duration_sec: 0.3,
        max_phrase_duration_sec: 5.0,
        padding_sec: 0.05,
    };

    let segments = dubrust::slicer::detect_phrases_from_wav(&wav_path, &config).unwrap();
    println!("Обнаружено сегментов: {:?}", segments);

    assert_eq!(segments.len(), 2, "Должно быть найдено ровно 2 фразы");

    // Фраза 1: ~ 1.0s .. 3.0s
    assert!((segments[0].start_sec - 1.0).abs() < 0.25);
    assert!((segments[0].end_sec - 3.0).abs() < 0.25);

    // Фраза 2: ~ 4.0s .. 5.5s
    assert!((segments[1].start_sec - 4.0).abs() < 0.25);
    assert!((segments[1].end_sec - 5.5).abs() < 0.25);

    // Проверяем генерацию волноформы
    let wf = dubrust::audio::extract_waveform_from_wav(&wav_path, 50.0).unwrap();
    assert!(wf.duration_sec >= 6.4);
    assert!(!wf.peaks.is_empty());

    let _ = std::fs::remove_file(wav_path);
}

#[test]
fn test_video_extract_and_dub_export() {
    let temp_dir = std::env::temp_dir().join("dubrust_test_export");
    let _ = std::fs::create_dir_all(&temp_dir);

    let test_video_path = temp_dir.join("synthetic_input.mp4");
    let extracted_audio_path = temp_dir.join("extracted_audio.wav");
    let test_output_path = temp_dir.join("final_dubbed_output.mp4");

    // 1. Создаем синтетическое видео через ffmpeg (3 секунды)
    let gen_res = Command::new("ffmpeg")
        .args([
            "-y",
            "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=15",
            "-f", "lavfi", "-i", "sine=frequency=440:duration=3",
            "-c:v", "libx264",
            "-c:a", "aac",
            test_video_path.to_str().unwrap(),
        ])
        .output();

    if let Ok(out) = gen_res {
        if !out.status.success() {
            eprintln!("Пропуск теста видео (ffmpeg encoder недоступен)");
            return;
        }
    } else {
        return;
    }

    // 2. Тестируем извлечение звука
    dubrust::video::extract_audio_from_video(&test_video_path, &extracted_audio_path).unwrap();
    assert!(extracted_audio_path.exists());

    // 3. Создаем тестовый дубль (WAV файл записи пользователя)
    let take_path = temp_dir.join("take_01.wav");
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&take_path, spec).unwrap();
    for i in 0..44100 {
        let t = i as f32 / 44100.0;
        let val = (t * 220.0 * 2.0 * std::f32::consts::PI).sin() * 0.9;
        writer.write_sample((val * i16::MAX as f32) as i16).unwrap();
    }
    writer.finalize().unwrap();

    let mut seg = dubrust::models::PhraseSegment::new(1, 0.5, 2.0);
    seg.recording_file = Some(take_path.clone());

    let segments = vec![seg];

    // 4. Тестируем экспорт финального видео
    dubrust::video::export_dubbed_video(
        &test_video_path,
        &extracted_audio_path,
        &segments,
        dubrust::models::DubMode::ReplaceSpeech,
        &test_output_path,
    )
    .unwrap();

    assert!(test_output_path.exists());
    let metadata = std::fs::metadata(&test_output_path).unwrap();
    assert!(metadata.len() > 1000, "Финальное видео должно быть создано и не пусто");

    // Очистка
    let _ = std::fs::remove_file(test_video_path);
    let _ = std::fs::remove_file(extracted_audio_path);
    let _ = std::fs::remove_file(take_path);
    let _ = std::fs::remove_file(test_output_path);
}

#[test]
fn test_firered_vad_slicing() {
    let wav_path = std::path::PathBuf::from("test_ai.wav");
    if !wav_path.exists() {
        return;
    }

    let config = dubrust::models::SlicerConfig {
        engine: dubrust::models::SlicerEngine::FireRedVad,
        use_neural_vad: true,
        neural_threshold: 0.40,
        silence_threshold_db: -28.0,
        min_silence_duration_sec: 0.25,
        min_phrase_duration_sec: 0.25,
        max_phrase_duration_sec: 5.0,
        padding_sec: 0.08,
    };

    let result = dubrust::slicer::detect_phrases_from_wav_verbose(&wav_path, &config);
    assert!(result.is_ok(), "FireRedVAD нарезка должна отработать успешно");
    let (segments, report) = result.unwrap();
    println!("FireRedVAD Report Engine: {:?}, сегментов: {}", report.engine, segments.len());
}
