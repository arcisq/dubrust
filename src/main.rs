// DubRust — студия дубляжа и переозвучки видео.
// Copyright (C) 2026 brawrel228
//
// Эта программа — свободное ПО: вы можете распространять и/или изменять её на
// условиях GNU Affero General Public License версии 3 или (по вашему выбору)
// любой более поздней версии, опубликованной Free Software Foundation.
//
// Программа распространяется в надежде, что она будет полезной, но БЕЗ КАКИХ-ЛИБО
// ГАРАНТИЙ. Подробности см. в файле LICENSE.

// Консоль скрывается только в release: в debug она нужна для логов
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;

use dubrust::app::DubApp;

fn load_icon() -> Option<egui::IconData> {
    let icon_bytes = include_bytes!("../assets/icon-256.png");
    if let Ok(image) = image::load_from_memory(icon_bytes) {
        let rgba = image.into_rgba8();
        let (width, height) = rgba.dimensions();
        Some(egui::IconData {
            rgba: rgba.into_raw(),
            width,
            height,
        })
    } else {
        None
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 760.0])
        .with_min_inner_size([800.0, 550.0])
        .with_active(true)
        .with_title("DubRust — Dubbing Studio");

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "DubRust — Dubbing Studio",
        options,
        Box::new(|cc| {
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = egui::Color32::from_rgb(15, 17, 23);
            visuals.window_fill = egui::Color32::from_rgb(20, 24, 33);
            visuals.extreme_bg_color = egui::Color32::from_rgb(12, 14, 19);
            cc.egui_ctx.set_visuals(visuals);

            Ok(Box::new(DubApp::new(cc)))
        }),
    )
}
