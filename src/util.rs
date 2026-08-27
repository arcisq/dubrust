use std::process::Command;

/// Создать команду без всплывающего консольного окна на Windows.
/// Без этого каждый вызов ffmpeg моргает чёрным окном поверх приложения.
pub fn hidden_command(program: &str) -> Command {
    let mut cmd = Command::new(program);
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
