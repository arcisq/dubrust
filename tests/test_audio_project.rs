//! Тесты на DSP и сохранность работы пользователя

use std::path::PathBuf;

use dubrust::audio::dsp::{peak, resample, rms, time_stretch};
use dubrust::models::{DubMode, MixConfig, PhraseSegment, SlicerConfig};
use dubrust::project::{self, ProjectData, ProjectPaths, PROJECT_VERSION};

fn sine(freq: f32, seconds: f32, sample_rate: u32, amplitude: f32) -> Vec<f32> {
    let total = (seconds * sample_rate as f32) as usize;
    (0..total)
        .map(|n| {
            let t = n as f32 / sample_rate as f32;
            amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
        })
        .collect()
}

#[test]
fn test_resample_keeps_duration_and_level() {
    let source = sine(440.0, 1.0, 48_000, 0.5);
    let resampled = resample(&source, 48_000, 44_100);

    // Длительность в секундах не должна ехать: иначе голос в финале ползёт по таймингу
    let seconds = resampled.len() as f32 / 44_100.0;
    assert!(
        (seconds - 1.0).abs() < 0.01,
        "ожидали около 1 секунды, получили {seconds}"
    );

    let source_peak = peak(&source);
    let result_peak = peak(&resampled);
    assert!(
        (source_peak - result_peak).abs() < 0.05,
        "пик изменился: {source_peak} → {result_peak}"
    );

    // Одинаковые частоты — данные проходят без изменений
    let untouched = resample(&source, 44_100, 44_100);
    assert_eq!(untouched.len(), source.len());
}

#[test]
fn test_time_stretch_changes_length_not_content() {
    let sample_rate = 44_100;
    let source = sine(300.0, 1.0, sample_rate, 0.6);

    let stretched = time_stretch(&source, 1.5, sample_rate);
    let expected = (source.len() as f32 * 1.5) as usize;
    let diff = (stretched.len() as f32 - expected as f32).abs() / expected as f32;
    assert!(diff < 0.02, "длина после растяжения ушла на {diff}");

    // Громкость не должна проваливаться от перекрытия окон
    let source_rms = rms(&source);
    let result_rms = rms(&stretched);
    assert!(
        result_rms > source_rms * 0.6 && result_rms < source_rms * 1.4,
        "RMS ушёл: {source_rms} → {result_rms}"
    );

    let sqeezed = time_stretch(&source, 0.75, sample_rate);
    assert!(sqeezed.len() < source.len());

    // Коэффициент 1.0 — работы нет
    assert_eq!(time_stretch(&source, 1.0, sample_rate).len(), source.len());
}

#[test]
fn test_remap_takes_survives_reslice() {
    let mut old_first = PhraseSegment::new(1, 0.0, 2.0);
    old_first.recording_file = Some(PathBuf::from("take_001_01.wav"));
    old_first.text_note = "первая реплика".to_string();

    let mut old_second = PhraseSegment::new(2, 3.0, 5.0);
    old_second.recording_file = Some(PathBuf::from("take_002_01.wav"));

    let previous = vec![old_first, old_second];

    let mut fresh = vec![
        PhraseSegment::new(1, 0.1, 1.9),
        PhraseSegment::new(2, 3.2, 4.8),
        PhraseSegment::new(3, 6.0, 7.0),
    ];

    let restored = project::remap_takes(&previous, &mut fresh);

    assert_eq!(restored, 2, "дубли потерялись при перенарезке");
    assert_eq!(
        fresh[0].recording_file,
        Some(PathBuf::from("take_001_01.wav"))
    );
    assert_eq!(
        fresh[1].recording_file,
        Some(PathBuf::from("take_002_01.wav"))
    );
    assert!(fresh[2].recording_file.is_none());
    assert_eq!(fresh[0].text_note, "первая реплика");

    // Один старый дубль не должен попасть в две новые фразы
    let mut split = vec![
        PhraseSegment::new(1, 0.0, 1.0),
        PhraseSegment::new(2, 1.0, 2.0),
    ];
    let restored_split = project::remap_takes(&previous, &mut split);
    assert_eq!(restored_split, 1);
}

#[test]
fn test_project_save_and_load_roundtrip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let video_path = dir.path().join("clip.mp4");
    std::fs::write(&video_path, b"fake").expect("video stub");

    let paths = ProjectPaths::for_video(&video_path);
    paths.ensure_dirs().expect("dirs");

    // Один дубль реально существует, второй — потерян
    let existing_take = paths.unique_take_path(1);
    std::fs::write(&existing_take, b"wav").expect("take stub");

    let mut first = PhraseSegment::new(1, 0.5, 2.0);
    first.recording_file = Some(existing_take.clone());
    let mut second = PhraseSegment::new(2, 3.0, 4.5);
    second.recording_file = Some(paths.takes_dir.join("take_002_99.wav"));

    let mut mix = MixConfig::default();
    mix.take_gain = 1.4;

    let data = ProjectData {
        version: PROJECT_VERSION,
        video_path: video_path.clone(),
        segments: vec![first, second],
        dub_mode: DubMode::VoiceOverDucking,
        slicer_config: SlicerConfig::default(),
        mix,
    };

    project::save(&paths, &data).expect("save");
    assert!(paths.exists(), "файл проекта не создан");

    let loaded = project::load(&paths).expect("load");

    assert_eq!(loaded.segments.len(), 2);
    assert_eq!(loaded.dub_mode, DubMode::VoiceOverDucking);
    assert!((loaded.mix.take_gain - 1.4).abs() < 1e-6);
    assert_eq!(loaded.segments[0].recording_file, Some(existing_take));
    assert!(
        loaded.segments[1].recording_file.is_none(),
        "пропавший файл должен сбрасываться"
    );
    assert!((loaded.segments[0].duration - 1.5).abs() < 1e-4);

    // Повторное сохранение не падает и не оставляет временный файл
    project::save(&paths, &loaded).expect("save again");
    assert!(!paths.root.join("project.json.tmp").exists());

    // Корзина и восстановление дубля
    let take = loaded.segments[0].recording_file.clone().unwrap();
    let trashed = paths.move_to_trash(&take).expect("to trash");
    assert!(!take.exists() && trashed.exists());

    let restored = paths.restore_from_trash(&trashed, 1).expect("restore");
    assert!(restored.exists());
}
