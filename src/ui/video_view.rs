use eframe::egui;

use crate::app::DubApp;
use crate::i18n::{self, Strings};
use crate::ui;
use crate::util::{ellipsize, format_duration, format_timecode};

pub fn central(app: &mut DubApp, ctx: &egui::Context) {
    let t = app.t();

    egui::CentralPanel::default().show(ctx, |ui| {
        if !app.has_media() {
            placeholder(ui, t);
            return;
        }

        media_header(app, ui);
        ui.add_space(4.0);

        let transport_height = 42.0;
        let area = egui::vec2(
            ui.available_width(),
            (ui.available_height() - transport_height).max(90.0),
        );
        video_area(app, ui, area);

        ui.add_space(6.0);
        transport(app, ui);
    });
}

fn placeholder(ui: &mut egui::Ui, t: &'static Strings) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.35);
            ui.label(egui::RichText::new(t.drop_video).heading());
            ui.add_space(6.0);
            ui.label(egui::RichText::new(t.drop_video_hint).color(ui::MUTED));
        });
    });
}

fn media_header(app: &mut DubApp, ui: &mut egui::Ui) {
    let name = app
        .video_path
        .as_ref()
        .and_then(|path| path.file_name())
        .and_then(|value| value.to_str())
        .unwrap_or(app.t().video_fallback)
        .to_string();

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(ellipsize(&name, 56)).strong())
            .on_hover_text(name.as_str());

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(format!(
                    "{}×{} · {:.0} fps · {}",
                    app.info.width,
                    app.info.height,
                    app.info.fps,
                    format_duration(app.info.duration_sec)
                ))
                .small()
                .color(ui::MUTED),
            );
        });
    });
}

fn video_area(app: &mut DubApp, ui: &mut egui::Ui, area: egui::Vec2) {
    let t = app.t();
    let (rect, response) = ui.allocate_exact_size(area, egui::Sense::click());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, egui::Rounding::same(4.0), egui::Color32::BLACK);

    match app.texture().map(|texture| (texture.id(), texture.size_vec2())) {
        Some((texture_id, texture_size)) if texture_size.x > 0.0 && texture_size.y > 0.0 => {
            // Пропорции сохраняются: раньше картинка растягивалась на всю панель
            let scale = (rect.width() / texture_size.x).min(rect.height() / texture_size.y);
            let size = texture_size * scale;
            let image_rect = egui::Rect::from_center_size(rect.center(), size);

            painter.image(
                texture_id,
                image_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
        _ => {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                t.frame_loading,
                egui::FontId::proportional(14.0),
                ui::MUTED,
            );
        }
    }

    if app.recorder.is_recording() {
        let level = app.mic_level.clamp(0.0, 1.0);

        painter.rect_stroke(
            rect,
            egui::Rounding::same(4.0),
            egui::Stroke::new(2.0_f32, ui::RECORD),
        );
        painter.text(
            rect.left_top() + egui::vec2(12.0, 10.0),
            egui::Align2::LEFT_TOP,
            "● REC",
            egui::FontId::proportional(15.0),
            ui::RECORD,
        );

        // Вертикальный индикатор уровня справа
        let meter = egui::Rect::from_min_max(
            egui::pos2(rect.right() - 26.0, rect.top() + 12.0),
            egui::pos2(rect.right() - 12.0, rect.bottom() - 12.0),
        );
        painter.rect_filled(
            meter,
            egui::Rounding::same(3.0),
            egui::Color32::from_black_alpha(140),
        );

        let filled = egui::Rect::from_min_max(
            egui::pos2(meter.left(), meter.bottom() - meter.height() * level),
            meter.max,
        );
        let color = if level > 0.97 { ui::RECORD } else { ui::DONE };
        painter.rect_filled(filled, egui::Rounding::same(3.0), color);
    }

    if response.clicked() {
        app.toggle_play();
    }
}

fn transport(app: &mut DubApp, ui: &mut egui::Ui) {
    let t = app.t();
    let lang = app.lang;

    let has_segments = !app.segments.is_empty();
    let has_take = app
        .selected_segment()
        .map(|segment| segment.has_recording())
        .unwrap_or(false);
    let recording = app.recorder.is_recording();
    let playing = app.playing;
    let busy = app.busy;

    let selected_label = match app.selected_segment() {
        Some(segment) => {
            let note = if segment.text_note.is_empty() {
                String::new()
            } else {
                format!(" · {}", ellipsize(&segment.text_note, 40))
            };
            format!(
                "{} · {} → {}{}",
                i18n::phrase_number(lang, segment.id),
                format_timecode(segment.start_sec),
                format_timecode(segment.end_sec),
                note
            )
        }
        None => t.no_selection.to_string(),
    };

    ui.horizontal(|ui| {
        if ui
            .add_enabled(has_segments, egui::Button::new("⏮"))
            .on_hover_text(t.prev_phrase)
            .clicked()
        {
            app.step_segment(-1);
        }

        let play_label = if playing { t.pause } else { t.play_original };
        if ui
            .add_enabled(!recording, egui::Button::new(play_label))
            .on_hover_text("Space")
            .clicked()
        {
            app.toggle_play();
        }

        if ui
            .add_enabled(has_segments, egui::Button::new("⏭"))
            .on_hover_text(t.next_phrase)
            .clicked()
        {
            app.step_segment(1);
        }

        ui.separator();

        let record_label = if recording { t.stop_key } else { t.record_key };
        let record_button = egui::Button::new(
            egui::RichText::new(record_label).color(if recording {
                egui::Color32::WHITE
            } else {
                ui::RECORD
            }),
        );
        if ui
            .add_enabled(has_segments && !busy, record_button)
            .clicked()
        {
            app.toggle_recording();
        }

        if ui
            .add_enabled(has_take && !recording, egui::Button::new(t.play_take_key))
            .clicked()
        {
            app.play_take();
        }

        ui.separator();
        ui.label(egui::RichText::new(selected_label).color(ui::MUTED));
    });
}
