use eframe::egui;

use crate::app::{playhead_label, DubApp};
use crate::i18n::{self, Lang};
use crate::models::DubMode;
use crate::ui;

/// Верхняя панель с основными действиями
pub fn top_bar(app: &mut DubApp, ctx: &egui::Context) {
    let t = app.t();
    let lang = app.lang;

    egui::TopBottomPanel::top("dubrust_top_bar").show(ctx, |ui| {
        ui.add_space(6.0);

        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!app.busy, egui::Button::new(t.open_video))
                .clicked()
            {
                app.open_dialog();
            }

            let can_export = !app.busy && app.has_media() && app.recorded_count() > 0;
            let export = ui.add_enabled(can_export, egui::Button::new(t.export_video));
            if export.clicked() {
                app.export_dialog();
            }
            if !can_export && app.has_media() {
                export.on_hover_text(t.export_hint);
            }

            if ui
                .add_enabled(
                    !app.busy && app.has_media(),
                    egui::Button::new(t.reslice),
                )
                .clicked()
            {
                app.reslice();
            }

            ui.toggle_value(&mut app.focus_mode, t.focus)
                .on_hover_text(t.focus_hint);

            ui.toggle_value(&mut app.show_settings, t.settings);

            ui.separator();

            let mut mode = app.dub_mode;
            egui::ComboBox::from_id_salt("dubrust_mode")
                .selected_text(mode.label(lang))
                .show_ui(ui, |ui| {
                    for candidate in DubMode::ALL {
                        ui.selectable_value(&mut mode, candidate, candidate.label(lang))
                            .on_hover_text(candidate.hint(lang));
                    }
                });
            if mode != app.dub_mode {
                app.dub_mode = mode;
                app.mark_dirty();
            }

            // Переключатель языка: всё перерисовывается сразу, без перезапуска
            let mut selected_lang = app.lang;
            egui::ComboBox::from_id_salt("dubrust_lang")
                .selected_text(selected_lang.native_name())
                .show_ui(ui, |ui| {
                    for candidate in Lang::ALL {
                        ui.selectable_value(
                            &mut selected_lang,
                            candidate,
                            candidate.native_name(),
                        );
                    }
                })
                .response
                .on_hover_text(t.language_hint);
            if selected_lang != app.lang {
                app.lang = selected_lang;
            }

            ui.separator();

            let mut volume = app.player.volume();
            if ui
                .add(
                    egui::Slider::new(&mut volume, 0.0..=2.0)
                        .text(t.volume)
                        .show_value(false),
                )
                .changed()
            {
                app.player.set_volume(volume);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(playhead_label(app));
            });
        });

        ui.add_space(6.0);
    });
}

/// Нижняя строка: статус, прогресс и предупреждения.
/// Раньше ошибки уходили только в консоль и пользователь их не видел.
pub fn status_bar(app: &mut DubApp, ctx: &egui::Context) {
    let t = app.t();
    let lang = app.lang;

    egui::TopBottomPanel::bottom("dubrust_status_bar").show(ctx, |ui| {
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if let Some((stage, fraction)) = app.progress.clone() {
                ui.add(
                    egui::ProgressBar::new(fraction.clamp(0.0, 1.0))
                        .desired_width(240.0)
                        .text(stage),
                );
            } else if app.busy {
                ui.spinner();
            }

            if !app.status.is_empty() {
                ui.label(app.status.as_str());
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(i18n::counters(
                        lang,
                        app.segments.len(),
                        app.recorded_count(),
                    ))
                    .color(ui::MUTED),
                );
            });
        });

        if let Some(warning) = app.warning.clone() {
            ui.horizontal(|ui| {
                ui.colored_label(ui::WARNING, format!("⚠ {warning}"));
                if ui.small_button(t.hide).clicked() {
                    app.warning = None;
                }
            });
        }

        ui.add_space(4.0);
    });
}

/// Окно настроек нарезки и сведения
pub fn settings_window(app: &mut DubApp, ctx: &egui::Context) {
    let t = app.t();
    let mut open = app.show_settings;
    let mut changed = false;
    let mut reslice = false;

    egui::Window::new(t.settings)
        .open(&mut open)
        .resizable(false)
        .default_width(360.0)
        .show(ctx, |ui| {
            ui.heading(t.slicing);

            ui.horizontal(|ui| {
                ui.label(t.slicing_engine);
                egui::ComboBox::from_id_salt("slicer_engine_select")
                    .selected_text(app.slicer_config.engine.label(app.lang))
                    .show_ui(ui, |ui| {
                        for engine in crate::models::SlicerEngine::ALL {
                            if ui
                                .selectable_value(
                                    &mut app.slicer_config.engine,
                                    engine,
                                    engine.label(app.lang),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        }
                    });
            });

            if app.slicer_config.engine == crate::models::SlicerEngine::FireRedVad {
                changed |= ui
                    .add(
                        egui::Slider::new(&mut app.slicer_config.neural_threshold, 0.1..=0.9)
                            .text(t.neural_threshold),
                    )
                    .changed();
            }

            // Раньше здесь был сырой порог в dB — теперь это чувствительность детектора
            changed |= ui
                .add(
                    egui::Slider::new(&mut app.slicer_config.silence_threshold_db, -60.0..=-10.0)
                        .text(t.sensitivity),
                )
                .on_hover_text(t.sensitivity_hint)
                .changed();

            changed |= ui
                .add(
                    egui::Slider::new(
                        &mut app.slicer_config.min_silence_duration_sec,
                        0.05..=2.0,
                    )
                    .text(t.min_pause),
                )
                .on_hover_text(t.min_pause_hint)
                .changed();

            changed |= ui
                .add(
                    egui::Slider::new(&mut app.slicer_config.min_phrase_duration_sec, 0.1..=3.0)
                        .text(t.min_phrase),
                )
                .changed();

            changed |= ui
                .add(
                    egui::Slider::new(&mut app.slicer_config.max_phrase_duration_sec, 1.0..=30.0)
                        .text(t.max_phrase),
                )
                .changed();

            changed |= ui
                .add(
                    egui::Slider::new(&mut app.slicer_config.padding_sec, 0.0..=0.5)
                        .text(t.padding),
                )
                .on_hover_text(t.padding_hint)
                .changed();

            if ui
                .add_enabled(
                    !app.busy && app.has_media(),
                    egui::Button::new(t.apply_reslice),
                )
                .clicked()
            {
                reslice = true;
            }

            ui.separator();
            ui.heading(t.mixing);

            changed |= ui
                .add(egui::Slider::new(&mut app.mix.take_gain, 0.0..=2.0).text(t.take_gain))
                .changed();

            changed |= ui
                .add(egui::Slider::new(&mut app.mix.original_gain, 0.0..=2.0).text(t.original_gain))
                .changed();

            ui.add_enabled_ui(app.dub_mode == DubMode::VoiceOverDucking, |ui| {
                changed |= ui
                    .add(egui::Slider::new(&mut app.mix.duck_level, 0.0..=1.0).text(t.duck_level))
                    .changed();
            });

            changed |= ui
                .checkbox(&mut app.mix.fit_takes, t.fit_takes)
                .on_hover_text(t.fit_takes_hint)
                .changed();

            ui.add_enabled_ui(app.mix.fit_takes, |ui| {
                changed |= ui
                    .add(egui::Slider::new(&mut app.mix.max_stretch, 1.0..=2.0).text(t.max_stretch))
                    .changed();
            });

            changed |= ui
                .checkbox(&mut app.mix.normalize_takes, t.normalize_takes)
                .changed();

            ui.separator();
            ui.heading(t.demucs_title);

            let demucs_installed = crate::audio::is_demucs_ready();
            if demucs_installed {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("✓ {}", t.demucs_installed)).color(egui::Color32::from_rgb(100, 220, 120)));
                });
                changed |= ui
                    .checkbox(&mut app.mix.enable_bg_separation, t.mode_dub_with_bg)
                    .on_hover_text(t.mode_dub_with_bg_hint)
                    .changed();
                if app.mix.enable_bg_separation {
                    changed |= ui
                        .add(egui::Slider::new(&mut app.mix.bg_gain, 0.0..=2.0).text(t.demucs_bg_volume))
                        .changed();
                }
            } else if app.demucs_downloading {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!(
                        "{} ({:.1} MB / {:.1} MB)",
                        t.demucs_downloading,
                        app.demucs_downloaded as f64 / 1_048_576.0,
                        app.demucs_total as f64 / 1_048_576.0
                    ));
                });
                ui.add(egui::ProgressBar::new(app.demucs_progress).show_percentage());
            } else {
                if ui
                    .button(egui::RichText::new(t.demucs_download_btn).strong())
                    .on_hover_text("https://huggingface.co/StemSplitio/htdemucs-onnx")
                    .clicked()
                {
                    app.download_demucs();
                }
            }

            ui.separator();
            ui.heading(t.cleanup);

            changed |= ui
                .add(egui::Slider::new(&mut app.mix.highpass_hz, 0.0..=200.0).text(t.highpass))
                .on_hover_text(t.highpass_hint)
                .changed();

            changed |= ui
                .add(egui::Slider::new(&mut app.mix.gate_strength, 0.0..=1.0).text(t.gate))
                .on_hover_text(t.gate_hint)
                .changed();

            ui.separator();
            ui.label(egui::RichText::new(t.hotkeys).small().color(ui::MUTED));
        });

    app.show_settings = open;

    if changed {
        app.mark_dirty();
    }

    if reslice {
        app.reslice();
    }
}
