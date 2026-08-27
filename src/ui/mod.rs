pub mod controls;
pub mod focus;
pub mod phrases;
pub mod timeline;
pub mod video_view;

use eframe::egui::Color32;

/// Общая палитра интерфейса
pub const ACCENT: Color32 = Color32::from_rgb(94, 154, 255);
pub const RECORD: Color32 = Color32::from_rgb(232, 84, 92);
pub const DONE: Color32 = Color32::from_rgb(88, 196, 140);
pub const MUTED: Color32 = Color32::from_rgb(138, 146, 163);
pub const WAVE: Color32 = Color32::from_rgb(104, 118, 148);
pub const PANEL_DARK: Color32 = Color32::from_rgb(24, 28, 38);
pub const SEGMENT_FILL: Color32 = Color32::from_rgb(46, 60, 88);
pub const WARNING: Color32 = Color32::from_rgb(233, 181, 88);
