// DubRust — студия дубляжа и переозвучки видео.
// Copyright (C) 2026 Arcis (arcisq)

//! Локализация: английский и русский.
//!
//! Строки — `&'static str` в двух константах, поэтому переключение языка
//! не стоит ни аллокаций, ни поиска по словарю. Компилятор сам напомнит,
//! если в одном из языков забыта фраза.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Lang {
    #[default]
    En,
    Ru,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Ru];

    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ru => "Русский",
        }
    }

    /// Язык системы: сначала DUBRUST_LANG, потом обычные переменные локали.
    pub fn detect() -> Lang {
        let raw = std::env::var("DUBRUST_LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default()
            .to_ascii_lowercase();

        if raw.starts_with("ru") {
            Lang::Ru
        } else {
            Lang::En
        }
    }

    pub fn strings(self) -> &'static Strings {
        match self {
            Lang::En => &EN,
            Lang::Ru => &RU,
        }
    }
}

/// Все подписи интерфейса.
pub struct Strings {
    // верхняя панель
    pub open_video: &'static str,
    pub export_video: &'static str,
    pub export_hint: &'static str,
    pub reslice: &'static str,
    pub focus: &'static str,
    pub focus_hint: &'static str,
    pub settings: &'static str,
    pub volume: &'static str,
    pub language_hint: &'static str,

    // строка состояния
    pub hide: &'static str,

    // настройки — нарезка
    pub slicing: &'static str,
    pub slicing_engine: &'static str,
    pub neural_vad: &'static str,
    pub neural_vad_hint: &'static str,
    pub neural_threshold: &'static str,
    pub sensitivity: &'static str,
    pub sensitivity_hint: &'static str,
    pub min_pause: &'static str,
    pub min_pause_hint: &'static str,
    pub min_phrase: &'static str,
    pub max_phrase: &'static str,
    pub padding: &'static str,
    pub padding_hint: &'static str,
    pub apply_reslice: &'static str,

    // настройки — сведение
    pub mixing: &'static str,
    pub take_gain: &'static str,
    pub original_gain: &'static str,
    pub duck_level: &'static str,
    pub fit_takes: &'static str,
    pub fit_takes_hint: &'static str,
    pub max_stretch: &'static str,
    pub normalize_takes: &'static str,
    pub cleanup: &'static str,
    pub highpass: &'static str,
    pub highpass_hint: &'static str,
    pub gate: &'static str,
    pub gate_hint: &'static str,
    pub hotkeys: &'static str,

    // список фраз
    pub phrases: &'static str,
    pub no_phrases: &'static str,
    pub play_original: &'static str,
    pub record: &'static str,
    pub record_again: &'static str,
    pub stop: &'static str,
    pub play_take: &'static str,
    pub delete_take: &'static str,
    pub note_hint: &'static str,
    pub recording: &'static str,
    pub level_silence: &'static str,
    pub level_overload: &'static str,
    pub level: &'static str,
    pub finish_key: &'static str,
    pub cancel: &'static str,
    pub record_phrase_key: &'static str,
    pub undo_take: &'static str,
    pub monitor: &'static str,
    pub monitor_hint: &'static str,

    // таймлайн и видео
    pub timeline_placeholder: &'static str,
    pub zoom: &'static str,
    pub zoom_fit: &'static str,
    pub zoom_follow: &'static str,
    pub drop_video: &'static str,
    pub drop_video_hint: &'static str,
    pub video_fallback: &'static str,
    pub frame_loading: &'static str,
    pub prev_phrase: &'static str,
    pub next_phrase: &'static str,
    pub pause: &'static str,
    pub play_original_key: &'static str,
    pub record_key: &'static str,
    pub stop_key: &'static str,
    pub play_take_key: &'static str,
    pub no_selection: &'static str,

    // фокус-режим
    pub select_phrase_hint: &'static str,
    pub has_take: &'static str,
    pub no_take: &'static str,
    pub scene_take: &'static str,
    pub scene_hint: &'static str,
    pub solo_take: &'static str,
    pub solo_hint: &'static str,
    pub original_big: &'static str,
    pub undo_short: &'static str,
    pub back: &'static str,
    pub forward: &'static str,
    pub next_empty: &'static str,
    pub note_label: &'static str,
    pub note_hint_focus: &'static str,
    pub done_key: &'static str,
    pub open_video_hint: &'static str,

    // режимы дубляжа
    pub mode_replace: &'static str,
    pub mode_replace_hint: &'static str,
    pub mode_only_dub: &'static str,
    pub mode_only_dub_hint: &'static str,
    pub mode_voiceover: &'static str,
    pub mode_voiceover_hint: &'static str,
    pub mode_dub_with_bg: &'static str,
    pub mode_dub_with_bg_hint: &'static str,

    // разделение HT-Demucs
    pub demucs_title: &'static str,
    pub demucs_download_btn: &'static str,
    pub demucs_downloading: &'static str,
    pub demucs_installed: &'static str,
    pub demucs_bg_volume: &'static str,

    // движки нарезки
    pub engine_firered: &'static str,
    pub engine_dsp: &'static str,
}

pub static EN: Strings = Strings {
    open_video: "Open video",
    export_video: "Export video",
    export_hint: "Record at least one take",
    reslice: "Re-split",
    focus: "Focus",
    focus_hint: "Tab — one phrase at a time instead of the studio layout",
    settings: "Settings",
    volume: "Volume",
    language_hint: "Interface language",

    hide: "hide",

    slicing: "Phrase detection",
    slicing_engine: "Engine",
    neural_vad: "FireRedVAD (neural)",
    neural_vad_hint: "Uses FireRedVAD ONNX. Falls back to the built-in detector when unavailable",
    neural_threshold: "Speech probability",
    sensitivity: "Sensitivity",
    sensitivity_hint: "How far above the measured noise floor speech must rise. Lower = catches quiet speech",
    min_pause: "Min pause, s",
    min_pause_hint: "Shorter silences stay inside one phrase",
    min_phrase: "Min phrase, s",
    max_phrase: "Max phrase, s",
    padding: "Padding, s",
    padding_hint: "Extra room around speech. Never eats into the neighbouring phrase",
    apply_reslice: "Apply and re-split",

    mixing: "Mixing",
    take_gain: "Take volume",
    original_gain: "Original volume",
    duck_level: "Original ducking",
    fit_takes: "Fit take to phrase length",
    fit_takes_hint: "Time stretch without changing pitch",
    max_stretch: "Max stretch",
    normalize_takes: "Normalize takes",
    cleanup: "Voice cleanup",
    highpass: "Low cut, Hz",
    highpass_hint: "85 Hz removes fan hum and desk rumble without touching the voice",
    gate: "Noise gate",
    gate_hint: "Pushes background down between words. 0 — off",
    hotkeys: "Shortcuts: Space — original, R — record, T — take in scene, Shift+T — take only, Enter — next without take, Tab — focus mode, ←/→ — phrases, Delete — drop take, Ctrl+Z — undo",

    phrases: "Phrases",
    no_phrases: "No phrases yet. Open a video or change the detection settings",
    play_original: "▶ Original",
    record: "● Record",
    record_again: "● Re-record",
    stop: "■ Stop",
    play_take: "▶ Take",
    delete_take: "✕ Delete take",
    note_hint: "Note or line text",
    recording: "● RECORDING",
    level_silence: "silence — check the microphone",
    level_overload: "clipping",
    level: "level",
    finish_key: "■ Finish (R)",
    cancel: "Cancel",
    record_phrase_key: "● Record phrase (R)",
    undo_take: "↶ Restore take",
    monitor: "🎧 Original while recording",
    monitor_hint: "Headphones only: speakers would leak the original into the microphone",

    timeline_placeholder: "The waveform and phrases will appear here",
    zoom: "Zoom",
    zoom_fit: "Fit",
    zoom_follow: "Follow playhead",
    drop_video: "Drop a video here",
    drop_video_hint: "or press “Open video” — phrases are detected automatically",
    video_fallback: "video",
    frame_loading: "preparing frame…",
    prev_phrase: "Previous phrase (←)",
    next_phrase: "Next phrase (→)",
    pause: "⏸ Pause",
    play_original_key: "↻ Original (Space)",
    record_key: "● Record (R)",
    stop_key: "■ Stop (R)",
    play_take_key: "▶ Take (T)",
    no_selection: "No phrase selected",

    select_phrase_hint: "Pick a phrase on the timeline or press →",
    has_take: "✓ take ready",
    no_take: "no take",
    scene_take: "▶ Take in scene (T)",
    scene_hint: "Your voice over the video together with the original actors",
    solo_take: "♪ Take only",
    solo_hint: "Shift+T — the bare recording without the scene",
    original_big: "↻ Original (Space)",
    undo_short: "↶ Restore",
    back: "← Back",
    forward: "Next →",
    next_empty: "⇥ Next without take (Enter)",
    note_label: "Note / line text",
    note_hint_focus: "what the actor says and what you should say",
    done_key: "■ Done (R)",
    open_video_hint: "Or press “Open video” in the top bar",

    mode_replace: "Replace speech",
    mode_replace_hint: "The original goes silent under each phrase and your take takes its place",
    mode_only_dub: "Dub voice only",
    mode_only_dub_hint: "The original track is dropped entirely, only your takes remain",
    mode_voiceover: "Voice-over with ducking",
    mode_voiceover_hint: "The original stays audible but quieter under your voice",
    mode_dub_with_bg: "Dubbing + clean BGM (HT-Demucs)",
    mode_dub_with_bg_hint: "Original voice is removed, keeping clean background music and effects under your takes",

    demucs_title: "Voice/Background separation (HT-Demucs, ~166 MB)",
    demucs_download_btn: "Download HT-Demucs weights from Hugging Face (Meta Research, ~166 MB)",
    demucs_downloading: "Downloading HT-Demucs from Hugging Face...",
    demucs_installed: "HT-Demucs weights installed (ready)",
    demucs_bg_volume: "Background volume",

    engine_firered: "FireRedVAD (neural SOTA)",
    engine_dsp: "built-in detector",
};

pub static RU: Strings = Strings {
    open_video: "Открыть видео",
    export_video: "Экспорт видео",
    export_hint: "Запишите хотя бы один дубль",
    reslice: "Перенарезать",
    focus: "Фокус",
    focus_hint: "Tab — одна фраза крупно вместо студийной раскладки",
    settings: "Настройки",
    volume: "Громкость",
    language_hint: "Язык интерфейса",

    hide: "скрыть",

    slicing: "Нарезка на фразы",
    slicing_engine: "Движок",
    neural_vad: "FireRedVAD (нейросеть)",
    neural_vad_hint: "Использует FireRedVAD ONNX. Если недоступен — работает встроенный детектор",
    neural_threshold: "Вероятность речи",
    sensitivity: "Чувствительность",
    sensitivity_hint: "Насколько речь должна быть громче измеренного шумового пола. Ниже — ловит тихую речь",
    min_pause: "Мин. пауза, с",
    min_pause_hint: "Паузы короче остаются внутри одной фразы",
    min_phrase: "Мин. фраза, с",
    max_phrase: "Макс. фраза, с",
    padding: "Отступы, с",
    padding_hint: "Запас вокруг речи. Никогда не залезает в соседнюю фразу",
    apply_reslice: "Применить и перенарезать",

    mixing: "Сведение",
    take_gain: "Громкость дублей",
    original_gain: "Громкость оригинала",
    duck_level: "Приглушение оригинала",
    fit_takes: "Подгонять дубль под длину фразы",
    fit_takes_hint: "Растяжение по времени без изменения высоты голоса",
    max_stretch: "Макс. растяжение",
    normalize_takes: "Нормализовать дубли",
    cleanup: "Чистка голоса",
    highpass: "Срез низких, Гц",
    highpass_hint: "85 Гц убирает гул кулера и рокот стола, голоса не касается",
    gate: "Шумодав",
    gate_hint: "Придавливает фон в паузах. 0 — выключен",
    hotkeys: "Горячие клавиши: Space — оригинал, R — запись, T — дубль в сцене, Shift+T — только дубль, Enter — следующая без дубля, Tab — фокус-режим, ←/→ — фразы, Delete — удалить дубль, Ctrl+Z — отмена",

    phrases: "Фразы",
    no_phrases: "Фраз пока нет. Откройте видео или измените настройки нарезки",
    play_original: "▶ Оригинал",
    record: "● Запись",
    record_again: "● Записать заново",
    stop: "■ Стоп",
    play_take: "▶ Дубль",
    delete_take: "✕ Удалить дубль",
    note_hint: "Заметка или текст реплики",
    recording: "● ЗАПИСЬ",
    level_silence: "тишина — проверьте микрофон",
    level_overload: "перегрузка",
    level: "уровень",
    finish_key: "■ Завершить (R)",
    cancel: "Отменить",
    record_phrase_key: "● Записать фразу (R)",
    undo_take: "↶ Вернуть дубль",
    monitor: "🎧 Звук оригинала при записи",
    monitor_hint: "Только в наушниках: через колонки оригинал попадёт в микрофон",

    timeline_placeholder: "Здесь появится волновая форма и фразы",
    zoom: "Масштаб",
    zoom_fit: "Вписать",
    zoom_follow: "Следить за курсором",
    drop_video: "Перетащите видео в окно",
    drop_video_hint: "или нажмите «Открыть видео» — фразы нарежутся автоматически",
    video_fallback: "видео",
    frame_loading: "кадр готовится…",
    prev_phrase: "Предыдущая фраза (←)",
    next_phrase: "Следующая фраза (→)",
    pause: "⏸ Пауза",
    play_original_key: "↻ Оригинал (Space)",
    record_key: "● Запись (R)",
    stop_key: "■ Стоп (R)",
    play_take_key: "▶ Дубль (T)",
    no_selection: "Фраза не выбрана",

    select_phrase_hint: "Выберите фразу на таймлайне или нажмите →",
    has_take: "✓ дубль есть",
    no_take: "нет дубля",
    scene_take: "▶ Дубль в сцене (T)",
    scene_hint: "Свой голос поверх видео и голосов актёров",
    solo_take: "♪ Только дубль",
    solo_hint: "Shift+T — чистая запись без сцены",
    original_big: "↻ Оригинал (Space)",
    undo_short: "↶ Вернуть",
    back: "← Назад",
    forward: "Дальше →",
    next_empty: "⇥ К следующей без дубля (Enter)",
    note_label: "Заметка / текст реплики",
    note_hint_focus: "что говорит актёр и что говорить вам",
    done_key: "■ Готово (R)",
    open_video_hint: "Или нажмите «Открыть видео» в верхней панели",

    mode_replace: "Замена речи",
    mode_replace_hint: "Под каждой фразой оригинал глушится, вместо него звучит ваш дубль",
    mode_only_dub: "Только дубляж",
    mode_only_dub_hint: "Оригинальная дорожка убирается совсем, остаются только ваши дубли",
    mode_voiceover: "Закадр с приглушением",
    mode_voiceover_hint: "Оригинал слышно, но тише под вашим голосом",
    mode_dub_with_bg: "Дубляж + чистый фон (HT-Demucs)",
    mode_dub_with_bg_hint: "Голос оригинала вырезан, под вашим дублем звучит чистая музыка и эффекты",

    demucs_title: "Разделение голос/фон (HT-Demucs, ~166 МБ)",
    demucs_download_btn: "Скачать веса HT-Demucs с Hugging Face (Meta Research, ~166 МБ)",
    demucs_downloading: "Загрузка HT-Demucs с Hugging Face...",
    demucs_installed: "Веса HT-Demucs установлены (готово)",
    demucs_bg_volume: "Громкость фона",

    engine_firered: "FireRedVAD (нейросеть SOTA)",
    engine_dsp: "встроенный детектор",
};

// Строки с числами: порядок слов в языках разный, поэтому собираем их функциями,
// а не склейкой кусков на месте вызова.

/// «3 of 40» / «3 из 40»
pub fn of_total(lang: Lang, value: usize, total: usize) -> String {
    match lang {
        Lang::En => format!("{value} of {total}"),
        Lang::Ru => format!("{value} из {total}"),
    }
}

/// «Phrase 7» / «Фраза 7»
pub fn phrase_number(lang: Lang, id: usize) -> String {
    match lang {
        Lang::En => format!("Phrase {id}"),
        Lang::Ru => format!("Фраза {id}"),
    }
}

/// Заголовок фокус-режима: «Phrase 7 of 40» / «Фраза 7 из 40»
pub fn phrase_of_total(lang: Lang, number: usize, total: usize) -> String {
    match lang {
        Lang::En => format!("Phrase {number} of {total}"),
        Lang::Ru => format!("Фраза {number} из {total}"),
    }
}

/// Счётчики в строке состояния
pub fn counters(lang: Lang, phrases: usize, takes: usize) -> String {
    match lang {
        Lang::En => format!("Phrases: {phrases} · takes: {takes}"),
        Lang::Ru => format!("Фраз: {phrases} · дублей: {takes}"),
    }
}

// ——— Сообщения строки состояния и диалогов ———
// Здесь функции, а не поля структуры: часть сообщений с подстановками,
// и порядок слов в двух языках разный.

macro_rules! msg {
    ($name:ident, $en:expr, $ru:expr) => {
        pub fn $name(lang: Lang) -> &'static str {
            match lang {
                Lang::En => $en,
                Lang::Ru => $ru,
            }
        }
    };
}

msg!(
    status_welcome,
    "Open a video or drop a file into the window",
    "Откройте видео или перетащите файл в окно"
);
msg!(
    status_busy,
    "Wait until the current operation finishes",
    "Подождите завершения текущей операции"
);
msg!(status_opening, "Opening file…", "Открытие файла…");
msg!(status_open_first, "Open a video first", "Сначала откройте видео");
msg!(
    status_no_takes,
    "No takes recorded yet",
    "Нет ни одного записанного дубля"
);
msg!(status_preparing, "Preparing…", "Подготовка…");
msg!(status_reslicing, "Re-splitting…", "Повторная нарезка…");
msg!(
    status_all_dubbed,
    "Every phrase has a take",
    "Все фразы озвучены"
);
msg!(
    status_audio_not_ready,
    "The audio track is not ready yet",
    "Аудиодорожка ещё не готова"
);
msg!(
    status_select_phrase,
    "Select a phrase first",
    "Сначала выберите фразу"
);
msg!(
    status_no_take,
    "This phrase has no take",
    "У этой фразы нет дубля"
);
msg!(
    status_take_in_scene,
    "Take in the scene — exactly like the final mix",
    "Дубль в сцене — как в финале"
);
msg!(
    status_record_cancelled,
    "Recording cancelled",
    "Запись отменена"
);
msg!(
    status_take_deleted,
    "Take deleted — Ctrl+Z brings it back",
    "Дубль удалён — Ctrl+Z вернёт его"
);
msg!(
    status_take_restored,
    "Take restored",
    "Дубль восстановлен"
);
msg!(dialog_video, "Video", "Видео");
msg!(dialog_all_files, "All files", "Все файлы");
msg!(stage_extract_audio, "Extracting audio…", "Извлечение аудио…");
msg!(stage_waveform, "Building waveform…", "Построение волновой формы…");
msg!(stage_slicing, "Detecting phrases…", "Поиск фраз…");
msg!(stage_mixing, "Mixing audio…", "Сведение звука…");
msg!(stage_muxing, "Writing video…", "Запись видео…");

pub fn status_save_failed(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("Could not save the project: {error}"),
        Lang::Ru => format!("Не удалось сохранить проект: {error}"),
    }
}

pub fn status_phrase_without_take(lang: Lang, number: usize) -> String {
    match lang {
        Lang::En => format!("Phrase {number} — no take yet"),
        Lang::Ru => format!("Фраза {number} — без дубля"),
    }
}

pub fn status_recording_phrase(lang: Lang, number: usize, device: &str) -> String {
    match lang {
        Lang::En => format!("Recording phrase {number} — {device}"),
        Lang::Ru => format!("Запись фразы {number} — {device}"),
    }
}

pub fn status_dropped_blocks(lang: Lang, dropped: usize) -> String {
    match lang {
        Lang::En => format!(
            "Recording had dropouts ({dropped} blocks). Close heavy applications"
        ),
        Lang::Ru => format!(
            "Запись шла с пропусками ({dropped} блоков). Закройте тяжёлые программы"
        ),
    }
}

pub fn status_take_saved(lang: Lang, duration: &str) -> String {
    match lang {
        Lang::En => format!("Take saved: {duration}"),
        Lang::Ru => format!("Дубль сохранён: {duration}"),
    }
}

pub fn status_project_restored(lang: Lang, name: &str, phrases: usize, takes: usize) -> String {
    match lang {
        Lang::En => format!("{name}: project restored, {phrases} phrases, {takes} takes"),
        Lang::Ru => format!("{name}: проект восстановлен, фраз {phrases}, дублей {takes}"),
    }
}

pub fn status_phrases_found(lang: Lang, name: &str, phrases: usize) -> String {
    match lang {
        Lang::En => format!("{name}: {phrases} phrases found"),
        Lang::Ru => format!("{name}: найдено фраз {phrases}"),
    }
}

pub fn status_sliced(lang: Lang, engine: &str, phrases: usize, restored: usize) -> String {
    match lang {
        Lang::En => format!("{engine}: {phrases} phrases, {restored} takes kept"),
        Lang::Ru => format!("{engine}: фраз {phrases}, сохранено дублей {restored}"),
    }
}

pub fn status_exported(lang: Lang, path: &str) -> String {
    match lang {
        Lang::En => format!("Done: {path}"),
        Lang::Ru => format!("Готово: {path}"),
    }
}

// ——— стадии фоновых задач и экспорта ———

pub fn stage_metadata(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Reading metadata…",
        Lang::Ru => "Чтение метаданных…",
    }
}

pub fn stage_read_original(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Reading original track",
        Lang::Ru => "Чтение оригинала",
    }
}

pub fn stage_final_mix(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Final mixdown",
        Lang::Ru => "Финальное сведение",
    }
}

pub fn stage_build_video(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Building video",
        Lang::Ru => "Сборка видео",
    }
}

pub fn stage_done(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Done",
        Lang::Ru => "Готово",
    }
}

pub fn stage_mixing_phrase(lang: Lang, index: usize, total: usize) -> String {
    match lang {
        Lang::En => format!("Mixing phrase {index}/{total}"),
        Lang::Ru => format!("Сведение фразы {index}/{total}"),
    }
}

pub fn error_nothing_to_export(lang: Lang) -> &'static str {
    match lang {
        Lang::En => "Nothing to export: the audio track is empty",
        Lang::Ru => "Нечего экспортировать: звуковая дорожка пустая",
    }
}

pub fn status_slice_failed(lang: Lang, error: &str) -> String {
    match lang {
        Lang::En => format!("Phrase detection failed: {error}"),
        Lang::Ru => format!("Не удалось нарезать фразы: {error}"),
    }
}
