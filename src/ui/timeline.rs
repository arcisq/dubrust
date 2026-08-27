use eframe::egui;

use crate::app::DubApp;
use crate::ui;
use crate::util::{format_duration, format_timecode};

const PANEL_HEIGHT: f32 = 145.0;

pub fn panel(app: &mut DubApp, ctx: &egui::Context) {
    egui::TopBottomPanel::bottom("dubrust_timeline")
        .exact_height(PANEL_HEIGHT)
        .show(ctx, |ui| {
            if !app.has_media() {
                let placeholder = app.t().timeline_placeholder;
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new(placeholder).color(ui::MUTED));
                });
                return;
            }

            timeline_header(app, ui);
            draw_timeline(app, ui);
        });
}

/// Верхняя панель управления таймлайном (зум, следование, таймкод)
fn timeline_header(app: &mut DubApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);

        // Текущее время / Длительность
        let duration = app.duration();
        let tc_text = format!(
            "{} / {}",
            format_timecode(app.playhead),
            format_timecode(duration)
        );
        ui.label(egui::RichText::new(tc_text).color(ui::ACCENT).strong().size(12.0));

        // Инфо о выбранной фразе
        if let Some((idx, seg)) = app.selected.and_then(|i| app.segments.get(i).map(|s| (i + 1, s))) {
            ui.separator();
            let seg_info = format!("#{} [{:.2} с]", idx, seg.duration);
            ui.label(
                egui::RichText::new(seg_info)
                    .color(if seg.has_recording() { ui::DONE } else { ui::MUTED })
                    .size(12.0),
            );
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Чекбокс следования за курсором
            let follow_label = app.t().zoom_follow;
            ui.checkbox(
                &mut app.timeline_follow,
                egui::RichText::new(follow_label).size(11.0).color(ui::MUTED),
            );

            ui.separator();

            // Кнопка сброса зума (100% / Вписать)
            let fit_label = app.t().zoom_fit;
            if ui.button(egui::RichText::new(fit_label).size(11.0)).clicked() {
                app.timeline_zoom = 1.0;
            }

            // Кнопка приблизить (+)
            if ui.button(egui::RichText::new("+").strong().size(12.0)).clicked() {
                app.timeline_zoom = (app.timeline_zoom * 1.35).min(50.0);
            }

            // Индикатор зума
            let zoom_text = format!("{:.1}x", app.timeline_zoom);
            ui.label(egui::RichText::new(zoom_text).color(ui::MUTED).size(11.0));

            // Слайдер масштаба
            ui.add(
                egui::Slider::new(&mut app.timeline_zoom, 1.0..=30.0)
                    .logarithmic(true)
                    .show_value(false),
            );

            // Кнопка отдалить (−)
            if ui.button(egui::RichText::new("−").strong().size(12.0)).clicked() {
                app.timeline_zoom = (app.timeline_zoom / 1.35).max(1.0);
            }

            ui.label(egui::RichText::new(app.t().zoom).color(ui::MUTED).size(11.0));
        });
    });

    ui.add_space(2.0);
}

/// Шаг сетки времени с поддержкой мелких долей секунды при сильном приближении
fn grid_step(duration: f32, width: f32) -> f32 {
    let target_px = 90.0;
    let rough = duration * target_px / width.max(1.0);
    for candidate in [
        0.05, 0.1, 0.2, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0,
    ] {
        if candidate >= rough {
            return candidate;
        }
    }
    600.0
}

fn draw_timeline(app: &mut DubApp, ui: &mut egui::Ui) {
    let duration = app.duration().max(0.001);
    let view_width = ui.available_width();
    let total_width = (view_width * app.timeline_zoom).max(view_width);
    let total_height = (ui.available_height() - 4.0).max(60.0);

    egui::ScrollArea::horizontal()
        .id_salt("dubrust_timeline_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(total_width, total_height),
                egui::Sense::click_and_drag(),
            );
            let painter = ui.painter_at(rect);

            painter.rect_filled(rect, egui::Rounding::same(4.0), ui::PANEL_DARK);

            let time_to_x = |time: f32| {
                rect.left() + (time / duration).clamp(0.0, 1.0) * rect.width()
            };

            // Масштабирование через Ctrl + Колесо мыши над таймлайном
            if response.hovered() {
                let (wheel_delta, ctrl) = ui.input(|i| {
                    (
                        i.raw_scroll_delta.y,
                        i.modifiers.ctrl || i.modifiers.command,
                    )
                });
                if ctrl && wheel_delta.abs() > 0.0 {
                    let factor = if wheel_delta > 0.0 { 1.2 } else { 1.0 / 1.2 };
                    app.timeline_zoom = (app.timeline_zoom * factor).clamp(1.0, 50.0);
                }
            }

            // Автоскролл за курсором при воспроизведении или записи
            let playhead_x = time_to_x(app.playhead);
            if app.timeline_follow && (app.playing || app.recorder.is_recording()) {
                let center_rect = egui::Rect::from_center_size(
                    egui::pos2(playhead_x, rect.center().y),
                    egui::vec2(60.0, total_height),
                );
                ui.scroll_to_rect(center_rect, Some(egui::Align::Center));
            }

            // Зоны фраз
            let wave_top = rect.top() + 2.0;
            let wave_bottom = rect.bottom() - 18.0;

            for (index, segment) in app.segments.iter().enumerate() {
                let left = time_to_x(segment.start_sec);
                let right = time_to_x(segment.end_sec).max(left + 2.0);
                let segment_rect = egui::Rect::from_min_max(
                    egui::pos2(left, wave_top),
                    egui::pos2(right, wave_bottom),
                );

                let fill = if segment.has_recording() {
                    ui::DONE.linear_multiply(0.30)
                } else {
                    ui::SEGMENT_FILL
                };
                painter.rect_filled(segment_rect, egui::Rounding::same(3.0), fill);

                if app.recording_index() == Some(index) {
                    painter.rect_stroke(
                        segment_rect,
                        egui::Rounding::same(3.0),
                        egui::Stroke::new(2.0_f32, ui::RECORD),
                    );
                } else if app.selected == Some(index) {
                    painter.rect_stroke(
                        segment_rect,
                        egui::Rounding::same(3.0),
                        egui::Stroke::new(1.5_f32, ui::ACCENT),
                    );
                }

                // Отображение номера фразы и длительности при достаточном масштабе
                if segment_rect.width() > 28.0 {
                    let label = if segment.has_recording() {
                        format!("✓ #{}", index + 1)
                    } else {
                        format!("#{}", index + 1)
                    };
                    painter.text(
                        egui::pos2(segment_rect.left() + 4.0, segment_rect.top() + 3.0),
                        egui::Align2::LEFT_TOP,
                        label,
                        egui::FontId::proportional(10.0),
                        if segment.has_recording() { ui::DONE } else { ui::MUTED },
                    );

                    if segment_rect.width() > 70.0 {
                        painter.text(
                            egui::pos2(segment_rect.left() + 4.0, segment_rect.bottom() - 4.0),
                            egui::Align2::LEFT_BOTTOM,
                            format_duration(segment.duration),
                            egui::FontId::proportional(9.0),
                            ui::MUTED.linear_multiply(0.8),
                        );
                    }
                }
            }

            // Волновая форма: одна линия на пиксель ширины
            let center_y = (wave_top + wave_bottom) * 0.5;
            let half_height = (wave_bottom - wave_top) * 0.45;
            let columns = rect.width().max(1.0);

            // Рисуем только видимую часть: при зуме 30x полная ширина давала
            // десятки тысяч линий и меток за кадр — интерфейс превращался в слайдшоу.
            let visible = ui.clip_rect().intersect(rect);
            let first_col = (visible.left() - rect.left()).max(0.0) as usize;
            let last_col = (visible.right() - rect.left() + 1.0).clamp(0.0, columns) as usize;

            if !app.waveform.peaks.is_empty() {
                for column in first_col..last_col {
                    let rel = column as f32 / columns;
                    let (min, max) = app.waveform.peak_at(rel);
                    let x = rect.left() + column as f32;
                    let top = center_y - max * half_height;
                    let bottom = center_y - min * half_height;
                    painter.line_segment(
                        [egui::pos2(x, top), egui::pos2(x, bottom.max(top + 1.0))],
                        egui::Stroke::new(1.0_f32, ui::WAVE),
                    );
                }
            }

            // Сетка времени
            let step = grid_step(duration, rect.width());
            let visible_from = (first_col as f32 / columns) * duration;
            let visible_to = (last_col as f32 / columns) * duration;
            let mut mark = ((visible_from / step).floor() * step).max(0.0);
            while mark <= duration.min(visible_to + step) {
                let x = time_to_x(mark);
                painter.line_segment(
                    [
                        egui::pos2(x, wave_bottom),
                        egui::pos2(x, rect.bottom() - 14.0),
                    ],
                    egui::Stroke::new(1.0_f32, ui::MUTED.linear_multiply(0.6)),
                );

                let mark_label = if step < 1.0 {
                    format!("{:.2}", mark)
                } else {
                    format_timecode(mark)
                };

                painter.text(
                    egui::pos2(x + 3.0, rect.bottom() - 13.0),
                    egui::Align2::LEFT_TOP,
                    mark_label,
                    egui::FontId::proportional(10.0),
                    ui::MUTED,
                );
                mark += step;
            }

            // Курсор воспроизведения
            painter.line_segment(
                [
                    egui::pos2(playhead_x, rect.top()),
                    egui::pos2(playhead_x, rect.bottom()),
                ],
                egui::Stroke::new(1.8_f32, ui::ACCENT),
            );

            // Переход по клику и перетаскиванию
            let dragging = response.dragged() && !response.drag_started();
            if response.clicked()
                || response.drag_started()
                || dragging
                || response.drag_stopped()
            {
                if let Some(pointer) = response.interact_pointer_pos() {
                    let rel = ((pointer.x - rect.left()) / rect.width().max(1.0))
                        .clamp(0.0, 1.0);
                    let time = rel * duration;

                    if let Some(index) =
                        app.segments.iter().position(|s| s.contains(time))
                    {
                        app.selected = Some(index);
                    }

                    if dragging {
                        // Во время перетаскивания только ведём курсор. Раньше seek
                        // вызывался на каждом кадре и плодил запросы кадров ffmpeg —
                        // таймлайн дёргало и картинка отставала.
                        app.playhead = time;
                    } else {
                        app.seek(time);
                    }
                }
            }

            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            }
        });
}
