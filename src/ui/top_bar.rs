use eframe::egui;

use crate::app::RgaGuiApp;
use crate::types::FocusTarget;

pub fn show(ctx: &egui::Context, app: &mut RgaGuiApp) {
    let pattern_id = egui::Id::new("pattern_input");

    egui::TopBottomPanel::top("search_bar").show(ctx, |ui| {
        ui.vertical(|ui| {
            // ── Row 0: pattern + action buttons ──────────────────────────
            ui.horizontal(|ui| {
                ui.label("🔍");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut app.pattern)
                        .id(pattern_id)
                        .hint_text("Search pattern (regex, inside files)…")
                        .desired_width(300.0),
                );

                let enter_pressed =
                    response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                // ── Search history navigation (Shift+Up / Shift+Down) ──
                let hist_len = app.search_history.len();
                let shift_up = ui.input(|i| i.key_pressed(egui::Key::ArrowUp) && i.modifiers.shift);
                let shift_down = ui.input(|i| i.key_pressed(egui::Key::ArrowDown) && i.modifiers.shift);
                if response.has_focus() && (shift_up || shift_down) {
                    if hist_len > 0 {
                        let new_idx = match app.history_index {
                            None => {
                                // Remember current typing so we can restore it.
                                if shift_up { Some(hist_len - 1) } else { None }
                            }
                            Some(i) => {
                                if shift_up {
                                    if i == 0 { None } else { Some(i - 1) }
                                } else {
                                    if i + 1 >= hist_len { None } else { Some(i + 1) }
                                }
                            }
                        };
                        match new_idx {
                            Some(idx) => {
                                if idx < hist_len {
                                    app.pattern = app.search_history[idx].clone();
                                    app.history_index = Some(idx);
                                }
                            }
                            None => {
                                app.pattern.clear();
                                app.history_index = None;
                            }
                        }
                    }
                }

                if ui.button("Search").clicked() || enter_pressed {
                    app.focus_after_search = Some(FocusTarget::Pattern);
                    app.start_search();
                }

                if matches!(app.search_status, crate::types::SearchStatus::Searching)
                    && ui.button("Stop").clicked()
                {
                    if let Some(ref flag) = app.cancel_flag {
                        flag.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                    app.search_status = crate::types::SearchStatus::Done;
                }

                if ui.button("Clear").clicked() {
                    app.pattern.clear();
                    app.results.clear();
                    app.collapsed_files.clear();
                    app.search_status = crate::types::SearchStatus::Idle;
                    app.error_message = None;
                    app.info_message = None;
                }
            });

            // ── Row 1: rga flags ─────────────────────────────────────────
            ui.horizontal(|ui| {
                let mut opts_changed = false;
                opts_changed |= ui
                    .checkbox(&mut app.case_insensitive, "Case insensitive")
                    .changed();
                opts_changed |=
                    ui.checkbox(&mut app.show_hidden, "Hidden").changed();
                opts_changed |= ui
                    .checkbox(&mut app.respect_gitignore, ".gitignore")
                    .changed();
                opts_changed |= ui
                    .checkbox(&mut app.exact_match, "Exact match")
                    .changed();
                if opts_changed {
                    app.save_config();
                }

                ui.separator();
                ui.label("Glob:");
                ui.add(
                    egui::TextEdit::singleline(&mut app.glob_filter)
                        .hint_text("*.pdf,*.md…")
                        .desired_width(100.0),
                );

                ui.separator();
                ui.label("Context:");
                ui.add(
                    egui::DragValue::new(&mut app.context_lines)
                        .speed(1)
                        .range(0..=20)
                        .suffix(" lines"),
                );
            });

            // ── Row 2: wrap + theme + scale ──────────────────────────────
            ui.horizontal(|ui| {
                if ui.checkbox(&mut app.word_wrap, "Wrap lines").changed() {
                    app.save_config();
                }

                ui.separator();
                ui.label("Max width:");
                if ui
                    .add(
                        egui::DragValue::new(&mut app.max_width_chars)
                            .speed(1)
                            .range(20.0..=200.0)
                            .max_decimals(0)
                            .suffix(" ch"),
                    )
                    .changed()
                {
                    app.save_config();
                }

                ui.separator();
                ui.label("UI Scale:");
                if ui
                    .add(
                        egui::DragValue::new(&mut app.ui_scale)
                            .speed(0.01)
                            .range(0.5..=3.0)
                            .max_decimals(2)
                            .suffix("×"),
                    )
                    .changed()
                {
                    ctx.set_zoom_factor(app.ui_scale);
                    app.save_config();
                }

                ui.separator();
                let theme_label = if app.light_mode { "Light" } else { "Dark" };
                egui::ComboBox::from_id_salt("theme_combo")
                    .selected_text(format!("Theme: {theme_label}"))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(&mut app.light_mode, false, "Dark")
                            .clicked()
                            || ui
                                .selectable_value(&mut app.light_mode, true, "Light")
                                .clicked()
                        {
                            app.save_config();
                        }
                    });
            });

            // ── Deferred focus ───────────────────────────────────────────
            if let Some(FocusTarget::Pattern) = app.focus_after_search {
                ui.memory_mut(|mem| mem.request_focus(pattern_id));
                app.focus_after_search = None;
            }
        });
    });
}
