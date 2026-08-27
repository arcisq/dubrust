use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Stdio};
use std::thread;
use std::time::Duration;

use crossbeam_channel::{bounded, Receiver, SendTimeoutError, Sender, TryRecvError};

use crate::models::MediaInfo;
use crate::util::hidden_command;
use crate::video::extractor::{extract_frame_rgb, scaled_height};

/// Готовый кадр в RGB
pub struct VideoFrame {
    pub time_sec: f32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

enum Command {
    Open {
        path: PathBuf,
        info: MediaInfo,
        width: u32,
    },
    /// Один кадр — для паузы и перетаскивания курсора
    Still(f32),
    /// Непрерывный поток с указанной позиции
    Play(f32),
    Stop,
    Quit,
}

struct Stream {
    child: Child,
    stdout: ChildStdout,
    frame_bytes: usize,
    width: u32,
    height: u32,
    fps: f32,
    start_sec: f32,
    index: u64,
}

fn stop_stream(stream: &mut Option<Stream>) {
    if let Some(mut active) = stream.take() {
        let _ = active.child.kill();
        let _ = active.child.wait();
    }
}

fn start_stream(path: &Path, info: &MediaInfo, width: u32, start_sec: f32) -> Option<Stream> {
    let width = (width.clamp(64, 1920)) & !1;
    let height = scaled_height(info, width);
    let fps = info.fps.clamp(5.0, 30.0);
    let start_sec = start_sec.max(0.0);

    let mut child = hidden_command("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(format!("{start_sec:.3}"))
        .arg("-i")
        .arg(path)
        .args(["-an", "-sn", "-dn", "-vf"])
        .arg(format!("scale={width}:{height},fps={fps:.3}"))
        .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let stdout = child.stdout.take()?;

    Some(Stream {
        child,
        stdout,
        frame_bytes: (width as usize) * (height as usize) * 3,
        width,
        height,
        fps,
        start_sec,
        index: 0,
    })
}

fn worker(commands: Receiver<Command>, frames: Sender<VideoFrame>) {
    let mut media: Option<(PathBuf, MediaInfo, u32)> = None;
    let mut stream: Option<Stream> = None;

    loop {
        // Пока поток активен — не блокируемся на командах, иначе ждём их.
        let next = if stream.is_some() {
            match commands.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            match commands.recv() {
                Ok(command) => Some(command),
                Err(_) => break,
            }
        };

        if let Some(command) = next {
            match command {
                Command::Quit => {
                    stop_stream(&mut stream);
                    break;
                }
                Command::Open { path, info, width } => {
                    stop_stream(&mut stream);
                    media = Some((path, info, width));
                }
                Command::Stop => stop_stream(&mut stream),
                Command::Still(time_sec) => {
                    stop_stream(&mut stream);
                    if let Some((path, _, width)) = media.as_ref() {
                        if let Ok(image) = extract_frame_rgb(path, time_sec, *width) {
                            let width = image.width();
                            let height = image.height();
                            let _ = frames.try_send(VideoFrame {
                                time_sec,
                                width,
                                height,
                                pixels: image.into_raw(),
                            });
                        }
                    }
                }
                Command::Play(time_sec) => {
                    stop_stream(&mut stream);
                    if let Some((path, info, width)) = media.as_ref() {
                        stream = start_stream(path, info, *width, time_sec);
                    }
                }
            }
            continue;
        }

        let Some(active) = stream.as_mut() else {
            continue;
        };

        let mut buffer = vec![0u8; active.frame_bytes];
        match active.stdout.read_exact(&mut buffer) {
            Ok(()) => {
                let time_sec = active.start_sec + active.index as f32 / active.fps;
                active.index += 1;

                let frame = VideoFrame {
                    time_sec,
                    width: active.width,
                    height: active.height,
                    pixels: buffer,
                };

                // Очередь небольшая: когда она заполнена, ffmpeg ждёт,
                // и темп воспроизведения задаёт сам UI.
                match frames.send_timeout(frame, Duration::from_millis(250)) {
                    Ok(()) => {}
                    Err(SendTimeoutError::Timeout(_)) => {}
                    Err(SendTimeoutError::Disconnected(_)) => {
                        stop_stream(&mut stream);
                        break;
                    }
                }
            }
            Err(_) => stop_stream(&mut stream),
        }
    }

    stop_stream(&mut stream);
}

/// Поставщик кадров видео.
/// Раньше на каждый кадр запускался новый процесс ffmpeg прямо в UI-потоке.
pub struct FramePump {
    commands: Sender<Command>,
    frames: Receiver<VideoFrame>,
    open: bool,
}

impl Default for FramePump {
    fn default() -> Self {
        Self::new()
    }
}

impl FramePump {
    pub fn new() -> Self {
        let (command_tx, command_rx) = bounded::<Command>(16);
        let (frame_tx, frame_rx) = bounded::<VideoFrame>(3);

        let _ = thread::Builder::new()
            .name("dubrust-frames".to_string())
            .spawn(move || worker(command_rx, frame_tx));

        Self {
            commands: command_tx,
            frames: frame_rx,
            open: false,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self, path: &Path, info: &MediaInfo, width: u32) {
        self.drain();
        let _ = self.commands.try_send(Command::Open {
            path: path.to_path_buf(),
            info: info.clone(),
            width,
        });
        self.open = true;
    }

    /// Запросить один кадр (пауза, скраб)
    pub fn request_still(&self, time_sec: f32) {
        let _ = self.commands.try_send(Command::Still(time_sec));
    }

    /// Начать потоковое воспроизведение с позиции
    pub fn play(&self, time_sec: f32) {
        let _ = self.commands.try_send(Command::Play(time_sec));
    }

    pub fn stop(&self) {
        let _ = self.commands.try_send(Command::Stop);
    }

    /// Выбросить устаревшие кадры (после перемотки)
    pub fn drain(&self) {
        while self.frames.try_recv().is_ok() {}
    }

    pub fn try_recv(&self) -> Option<VideoFrame> {
        self.frames.try_recv().ok()
    }
}

impl Drop for FramePump {
    fn drop(&mut self) {
        let _ = self.commands.try_send(Command::Quit);
    }
}
