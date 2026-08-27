//! Фокус-режим: одна фраза за раз, крупное видео и большие кнопки.
//! Студийная раскладка удобна для обзора, но при самой переозвучке мешает:
//! глаза бегают по спискам вместо того, чтобы смотреть на актёра.

use eframe::egui;

use crate::app::DubApp;
use crate::i18n::{self, Lang, Strings};
use crate::ui;
use crate::util::format_duration;

/// Действие, выбранное в карточке.
/// Собираем его в замыкании и применяем после: внутри `&mut app` недоступен.
#[derive(Clone, PartialEq)]
enum Action {
    None,
    Record,
    Cancel,
    PlayScene,
    PlaySolo,
    PlayOriginal,
    Delete,
    Undo,
    Prev,
    Next,
    NextEmpty,
    Note(String),
}

pub fn central(app: &mut DubApp, ctx: &egui::Context) {
    let mut action = Action::None;
    let t = app.t();
    let lang = app.lang;

    egui::CentralPanel::default().show(ctx, |ui| {
        if !app.has_media() {
            empty_state(ui, t);
            return;
        }

        let Some(index) = app.selected else {
            ui.vertical_centered(|ui| {
                ui.add_space(40.0);
                ui.label(t.select_phrase_hint);
            });
            return;
        };
        let Some(segment) = app.segments.get(index) else {
            return;
        };

        // Снимок данных фразы — дальше рисуем только по нему
        let total = app.segments.len();
        let recorded = app.recorded_count();
        let start = segment.start_sec;
        let end = segment.end_sec;
        let duration = segment.duration;
        let has_take = segment.has_recording();
        let mut note = segment.text_note.clone();

        header(
            ui, lang, t, index, total, recorded, start, end, duration, has_take,
        );
        ui.add_space(6.0);

        // Кадр крупно: видно губы актёра, а это главное при дубляже
        let reserved = 190.0;
        let height = (ui.available_height() - reserved).max(120.0);
        preview(app, ui, height);

        ui.add_space(8.0);

        if app.recorder.is_recording() {
            recording_panel(app, ui, &mut action);
        } else {
            idle_panel(app, ui, has_take, &mut action);
        }

        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t.note_label).color(ui::MUTED));
            if ui
                .add(
                    egui::TextEdit::singleline(&mut note)
                        .desired_width(ui.available_width() - 8.0)
                        .hint_text(t.note_hint_focus),
                )
                .changed()
            {
                action = Action::Note(note.clone());
            }
        });
    });

    apply(app, action);
}

fn empty_state(ui: &mut egui::Ui, t: &'static Strings) {
    ui.vertical_centered(|ui| {
        ui.add_space(60.0);
        ui.heading(t.drop_video);
        ui.add_space(6.0);
        ui.label(egui::RichText::new(t.open_video_hint).color(ui::MUTED));
    });
}

#[allow(clippy::too_many_arguments)]
fn header(
    ui: &mut egui::Ui,
    lang: Lang,
    t: &'static Strings,
    index: usize,
    total: usize,
    recorded: usize,
    start: f32,
    end: f32,
    duration: f32,
    has_take: bool,
) {
    ui.horizontal(|ui| {
        ui.heading(i18n::phrase_of_total(lang, index + 1, total));

        ui.label(
            egui::RichText::new(format!(
                "{} → {}  ·  {}",
                format_duration(start),
                format_duration(end),
                format_duration(duration)
            ))
            .color(ui::MUTED),
        );

        if has_take {
            ui.colored_label(ui::DONE, t.has_take);
        } else {
            ui.colored_label(ui::WARNING, t.no_take);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let fraction = if total == 0 {
                0.0
            } else {
                recorded as f32 / total as f32
            };
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width(160.0)
                    .text(format!("{recorded}/{total}")),
            );
        });
    });
}

fn preview(app: &mut DubApp, ui: &mut egui::Ui, height: f32) {
    let loading = app.t().frame_loading;
    let frame = egui::Frame::none()
        .fill(ui::PANEL_DARK)
        .rounding(egui::Rounding::same(8.0));

    frame.show(ui, |ui| {
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());

        match app.texture() {
            Some(texture) => {
                let size = texture.size_vec2();
                // Вписываем кадр целиком, без обрезки и растяжек
                let scale = (rect.width() / size.x).min(rect.height() / size.y);
                let target = egui::vec2(size.x * scale, size.y * scale);
                let image_rect = egui::Rect::from_center_size(rect.center(), target);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(texture.id(), image_rect, uv, egui::Color32::WHITE);
            }
            None => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    loading,
                    egui::FontId::proportional(14.0),
                    ui::MUTED,
                );
            }
        }
    });
}

fn recording_panel(app: &mut DubApp, ui: &mut egui::Ui, action: &mut Action) {
    let t = app.t();
    let level = app.mic_level.clamp(0.0, 1.0);
    let elapsed = app.recorder.recorded_duration_sec();

    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(t.recording)
                .color(ui::RECORD)
                .size(20.0)
                .strong(),
        );
        ui.label(egui::RichText::new(format_duration(elapsed)).size(18.0));
    });

    ui.add(
        egui::ProgressBar::new(level)
            .desired_width(ui.available_width())
            .fill(if level > 0.97 { ui::RECORD } else { ui::DONE })
            .text(if level < 0.01 {
                t.level_silence
            } else if level > 0.97 {
                t.level_overload
            } else {
                t.level
            }),
    );

    ui.add_space(6.0);

    ui.horizontal(|ui| {
        if big_button(ui, t.done_key, ui::DONE).clicked() {
            *action = Action::Record;
        }
        if big_button(ui, t.cancel, ui::MUTED).clicked() {
            *action = Action::Cancel;
        }
    });
}

fn idle_panel(app: &mut DubApp, ui: &mut egui::Ui, has_take: bool, action: &mut Action) {
    let t = app.t();
    let can_record = app.has_media() && !app.busy;

    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                can_record,
                big(egui::RichText::new(t.record_key).color(ui::RECORD)),
            )
            .clicked()
        {
            *action = Action::Record;
        }

        if ui
            .add_enabled(has_take, big(egui::RichText::new(t.scene_take)))
            .on_hover_text(t.scene_hint)
            .clicked()
        {
            *action = Action::PlayScene;
        }

        if ui
            .add_enabled(has_take, big(egui::RichText::new(t.solo_take)))
            .on_hover_text(t.solo_hint)
            .clicked()
        {
            *action = Action::PlaySolo;
        }

        if ui.add(big(egui::RichText::new(t.original_big))).clicked() {
            *action = Action::PlayOriginal;
        }
    });

    ui.add_space(4.0);

    ui.horizontal(|ui| {
        if ui
            .add_enabled(has_take, egui::Button::new(t.delete_take))
            .clicked()
        {
            *action = Action::Delete;
        }
        if app.can_undo_delete() && ui.button(t.undo_short).clicked() {
            *action = Action::Undo;
        }

        ui.separator();

        if ui.button(t.back).clicked() {
            *action = Action::Prev;
        }
        if ui.button(t.forward).clicked() {
            *action = Action::Next;
        }
        if ui.button(t.next_empty).clicked() {
            *action = Action::NextEmpty;
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut app.monitor_original, t.monitor)
                .on_hover_text(t.monitor_hint);
        });
    });
}

fn big(text: egui::RichText) -> egui::Button<'static> {
    egui::Button::new(text.size(16.0)).min_size(egui::vec2(150.0, 36.0))
}

fn big_button(ui: &mut egui::Ui, label: &str, color: egui::Color32) -> egui::Response {
    ui.add(big(egui::RichText::new(label).color(color)))
}

fn apply(app: &mut DubApp, action: Action) {
    match action {
        Action::None => {}
        Action::Record => app.toggle_recording(),
        Action::Cancel => app.cancel_recording(),
        Action::PlayScene => app.play_take(),
        Action::PlaySolo => app.play_take_solo(),
        Action::PlayOriginal => app.toggle_play(),
        Action::Delete => app.delete_take(),
        Action::Undo => app.undo_delete(),
        Action::Prev => app.step_segment(-1),
        Action::Next => app.step_segment(1),
        Action::NextEmpty => app.select_next_empty(),
        Action::Note(text) => {
            if let Some(index) = app.selected {
                if let Some(segment) = app.segments.get_mut(index) {
                    segment.text_note = text;
                    app.mark_dirty();
                }
            }
        }
    }
}
