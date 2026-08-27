use eframe::egui;

use crate::app::DubApp;
use crate::i18n;
use crate::ui;
use crate::util::{format_duration, format_timecode};

/// Снимок строки списка: рисуем по копии, чтобы можно было менять состояние
struct Row {
    index: usize,
    id: usize,
    start_sec: f32,
    end_sec: f32,
    has_take: bool,
    note: String,
}

enum Action {
    Select(usize),
    PlayOriginal(usize),
    PlayTake(usize),
    Record(usize),
    Delete(usize),
    Note(usize, String),
}

pub fn side_panel(app: &mut DubApp, ctx: &egui::Context) {
    let t = app.t();
    let lang = app.lang;

    egui::SidePanel::right("dubrust_phrases")
        .default_width(330.0)
        .min_width(280.0)
        .show(ctx, |ui| {
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.heading(t.phrases);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(i18n::of_total(
                            lang,
                            app.recorded_count(),
                            app.segments.len(),
                        ))
                        .color(ui::MUTED),
                    );
                });
            });

            recorder_block(app, ui);
            ui.separator();

            if app.segments.is_empty() {
                ui.add_space(8.0);
                ui.label(egui::RichText::new(t.no_phrases).color(ui::MUTED));
                return;
            }

            let rows: Vec<Row> = app
                .segments
                .iter()
                .enumerate()
                .map(|(index, segment)| Row {
                    index,
                    id: segment.id,
                    start_sec: segment.start_sec,
                    end_sec: segment.end_sec,
                    has_take: segment.has_recording(),
                    note: segment.text_note.clone(),
                })
                .collect();

            let selected = app.selected;
            let recording = app.recording_index();
            let busy = app.busy;
            let mut actions: Vec<Action> = Vec::new();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for row in &rows {
                        let is_selected = selected == Some(row.index);
                        let is_recording = recording == Some(row.index);

                        let frame = egui::Frame::group(ui.style())
                            .fill(if is_selected {
                                ui::SEGMENT_FILL
                            } else {
                                ui::PANEL_DARK
                            })
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                if is_recording {
                                    ui::RECORD
                                } else if is_selected {
                                    ui::ACCENT
                                } else {
                                    egui::Color32::TRANSPARENT
                                },
                            ));

                        frame.show(ui, |ui| {
                            ui.set_width(ui.available_width());

                            let header = ui.horizontal(|ui| {
                                let marker = if row.has_take { "●" } else { "○" };
                                let color = if row.has_take { ui::DONE } else { ui::MUTED };
                                ui.label(egui::RichText::new(marker).color(color));
                                ui.label(
                                    egui::RichText::new(i18n::phrase_number(lang, row.id)).strong(),
                                );

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format_duration(
                                                row.end_sec - row.start_sec,
                                            ))
                                            .color(ui::MUTED)
                                            .small(),
                                        );
                                    },
                                );
                            });

                            if header.response.interact(egui::Sense::click()).clicked() {
                                actions.push(Action::Select(row.index));
                            }

                            ui.label(
                                egui::RichText::new(format!(
                                    "{} → {}",
                                    format_timecode(row.start_sec),
                                    format_timecode(row.end_sec)
                                ))
                                .small()
                                .color(ui::MUTED),
                            );

                            ui.horizontal(|ui| {
                                if ui.small_button(t.play_original).clicked() {
                                    actions.push(Action::PlayOriginal(row.index));
                                }

                                let record_label = if is_recording {
                                    t.stop
                                } else if row.has_take {
                                    t.record_again
                                } else {
                                    t.record
                                };

                                if ui
                                    .add_enabled(!busy, egui::Button::new(record_label).small())
                                    .clicked()
                                {
                                    actions.push(Action::Record(row.index));
                                }

                                if row.has_take {
                                    if ui.small_button(t.play_take).clicked() {
                                        actions.push(Action::PlayTake(row.index));
                                    }
                                    if ui
                                        .small_button("✕")
                                        .on_hover_text(t.delete_take)
                                        .clicked()
                                    {
                                        actions.push(Action::Delete(row.index));
                                    }
                                }
                            });

                            if is_selected {
                                let mut note = row.note.clone();
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut note)
                                        .hint_text(t.note_hint)
                                        .desired_width(f32::INFINITY),
                                );
                                if response.changed() {
                                    actions.push(Action::Note(row.index, note));
                                }
                            }
                        });

                        ui.add_space(4.0);
                    }
                });

            for action in actions {
                match action {
                    Action::Select(index) => app.select(index),
                    Action::PlayOriginal(index) => {
                        app.select(index);
                        if let Some(segment) = app.segments.get(index) {
                            let (start, end) = (segment.start_sec, segment.end_sec);
                            app.play_original(start, Some(end));
                        }
                    }
                    Action::PlayTake(index) => {
                        app.selected = Some(index);
                        app.play_take();
                    }
                    Action::Record(index) => {
                        if app.recording_index().is_some() {
                            app.toggle_recording();
                        } else {
                            app.select(index);
                            app.toggle_recording();
                        }
                    }
                    Action::Delete(index) => {
                        app.selected = Some(index);
                        app.delete_take();
                    }
                    Action::Note(index, note) => {
                        if let Some(segment) = app.segments.get_mut(index) {
                            segment.text_note = note;
                        }
                        app.mark_dirty();
                    }
                }
            }
        });
}

/// Блок записи: индикатор уровня и отмена.
/// Без индикатора было непонятно, слышит ли микрофон голос.
fn recorder_block(app: &mut DubApp, ui: &mut egui::Ui) {
    let t = app.t();
    ui.add_space(4.0);

    if app.recorder.is_recording() {
        let level = app.mic_level.clamp(0.0, 1.0);
        let duration = app.recorder.recorded_duration_sec();

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(t.recording).color(ui::RECORD).strong());
            ui.label(egui::RichText::new(format_duration(duration)).color(ui::MUTED));
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

        ui.horizontal(|ui| {
            if ui.button(t.finish_key).clicked() {
                app.toggle_recording();
            }
            if ui.button(t.cancel).clicked() {
                app.cancel_recording();
            }
        });
    } else {
        ui.horizontal(|ui| {
            let can_record = app.has_media() && app.selected.is_some() && !app.busy;
            if ui
                .add_enabled(can_record, egui::Button::new(t.record_phrase_key))
                .clicked()
            {
                app.toggle_recording();
            }

            if app.can_undo_delete() && ui.button(t.undo_take).clicked() {
                app.undo_delete();
            }
        });

        ui.checkbox(&mut app.monitor_original, t.monitor)
            .on_hover_text(t.monitor_hint);
    }

    ui.add_space(4.0);
}
