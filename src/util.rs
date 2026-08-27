use std::path::PathBuf;
use std::process::Command;

/// Каталог, в котором лежит сам исполняемый файл.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()?
        .parent()
        .map(|dir| dir.to_path_buf())
}

/// Портативный режим: рядом с exe лежит файл-маркер `portable.txt`.
/// В этом режиме приложение не трогает %APPDATA% и реестр — всё своё
/// хранит внутри собственной папки, поэтому его можно носить на флешке.
pub fn is_portable() -> bool {
    exe_dir()
        .map(|dir| dir.join("portable.txt").is_file())
        .unwrap_or(false)
}

/// Каталог данных для портативной сборки: `./data` рядом с exe.
/// Возвращает `None`, если сборка обычная (установленная).
pub fn portable_data_dir() -> Option<PathBuf> {
    if !is_portable() {
        return None;
    }
    let dir = exe_dir()?.join("data");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Найти внешнюю утилиту (ffmpeg/ffprobe).
///
/// Сначала смотрим рядом с exe и в `./ffmpeg/bin` — туда их кладут инсталлер
/// и портативный архив, чтобы пользователю не пришлось ничего настраивать.
/// Если не нашли — отдаём просто имя, и его подхватит системный PATH.
pub fn resolve_tool(program: &str) -> PathBuf {
    if program.contains('/') || program.contains('\\') {
        return PathBuf::from(program);
    }

    let file_name = if cfg!(windows) && !program.ends_with(".exe") {
        format!("{program}.exe")
    } else {
        program.to_string()
    };

    if let Some(dir) = exe_dir() {
        let candidates = [
            dir.join(&file_name),
            dir.join("ffmpeg").join(&file_name),
            dir.join("ffmpeg").join("bin").join(&file_name),
        ];
        for candidate in candidates {
            if candidate.is_file() {
                return candidate;
            }
        }
    }

    PathBuf::from(program)
}

/// Создать команду без всплывающего консольного окна на Windows.
/// Без этого каждый вызов ffmpeg моргает чёрным окном поверх приложения.
pub fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(resolve_tool(program));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Формат времени мм:сс.дд для интерфейса
pub fn format_timecode(seconds: f32) -> String {
    let total = seconds.max(0.0);
    let minutes = (total / 60.0).floor() as u32;
    let secs = total - minutes as f32 * 60.0;
    format!("{:02}:{:05.2}", minutes, secs)
}

/// Короткий формат длительности: 1.25 с
pub fn format_duration(seconds: f32) -> String {
    format!("{:.2} с", seconds.max(0.0))
}

/// Обрезать строку до указанной длины с многоточием (безопасно для UTF-8)
pub fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Последние строки вывода процесса — для понятных сообщений об ошибках ffmpeg
pub fn tail_lines(text: &str, lines: usize) -> String {
    let collected: Vec<&str> = text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    let start = collected.len().saturating_sub(lines);
    collected[start..].join("; ")
}
