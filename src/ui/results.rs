use std::collections::HashMap;
use std::path::PathBuf;

use eframe::egui;
use egui::{Color32, CursorIcon, RichText, ScrollArea, Sense, TextFormat};

use crate::app::RgaGuiApp;
use crate::types::SubMatch;

pub fn show(ctx: &egui::Context, app: &mut RgaGuiApp) {
    if app.show_find_bar {
        egui::TopBottomPanel::bottom("find_bar").show(ctx, |ui| {
            render_find_bar(ui, app);
        });
    }
    egui::CentralPanel::default().show(ctx, |ui| {
        // ── Status bar ───────────────────────────────────────────────────
        ui.horizontal(|ui| {
            match app.search_status {
                crate::types::SearchStatus::Idle => {
                    ui.label("Ready — type a pattern and press Search");
                }
                crate::types::SearchStatus::Searching => {
                    ui.label(format!(
                        "Searching… {} matches so far",
                        app.results.len()
                    ));
                }
                crate::types::SearchStatus::Done => {
                    let total = app.results.len();
                    let files: usize = app
                        .results
                        .iter()
                        .map(|m| m.path.as_path())
                        .collect::<std::collections::BTreeSet<_>>()
                        .len();
                    ui.label(format!(
                        "Done — {} match{} in {} file{}",
                        total,
                        if total == 1 { "" } else { "es" },
                        files,
                        if files == 1 { "" } else { "s" },
                    ));
                    let all_collapsed = app.collapsed_files.values().all(|&c| c);
                    if ui
                        .button(if all_collapsed { "Expand all" } else { "Collapse all" })
                        .clicked()
                    {
                        let new_state = !all_collapsed;
                        let mut seen = std::collections::HashSet::new();
                        for m in &app.results {
                            let key = m.path.to_string_lossy().to_string();
                            if seen.insert(key.clone()) {
                                app.collapsed_files.insert(key, new_state);
                            }
                        }
                    }
                }
            }
        });

        // ── Messages ─────────────────────────────────────────────────────
        if let Some(ref err) = app.error_message {
            egui::CollapsingHeader::new(egui::RichText::new("Error").color(Color32::RED))
                .default_open(true)
                .show(ui, |ui| {
                    ui.colored_label(Color32::RED, err);
                });
        }
        if let Some(ref info) = app.info_message {
            egui::CollapsingHeader::new(
                egui::RichText::new("Info").color(ui.visuals().weak_text_color()),
            )
            .default_open(true)
            .show(ui, |ui| {
                ui.colored_label(ui.visuals().weak_text_color(), info);
            });
        }

        ui.separator();

        // ── Results ──────────────────────────────────────────────────────
        let char_width = ui.style().text_styles
            .get(&egui::TextStyle::Body)
            .map(|f| f.size * 0.6)
            .unwrap_or(8.0);
        let max_width = app.max_width_chars * char_width;
        render_flat(ui, app, max_width);
    });
}

// ── Flat view ─────────────────────────────────────────────────────────────

fn render_flat(ui: &mut egui::Ui, app: &mut RgaGuiApp, max_width: f32) {
    let mut open_path: Option<PathBuf> = None;
    let word_wrap = app.word_wrap;

    // ── Find-in-results: build per-row range buckets ──
    let highlight_all = app.find_highlight_all;
    let current_match = app.find_matches.get(app.find_current).copied();
    let current_row = current_match.map(|(ri, _, _)| ri);
    let (find_all_buckets, find_cur) = {
        let mut buckets: Vec<Vec<(usize, usize)>> = vec![Vec::new(); app.results.len()];
        for &(ri, s, e) in &app.find_matches {
            if ri < buckets.len() {
                buckets[ri].push((s, e));
            }
        }
        (buckets, current_match.map(|(_, s, e)| (s, e)))
    };
    let find_need_scroll = app.find_need_scroll;
    let mut did_scroll = false;

    ScrollArea::both()
        .auto_shrink([false; 2])
        .show(ui, |ui| {
            ui.set_max_width(max_width);
            let limit = 20_000;
            let results = &app.results;
            let count = results.len().min(limit);

            // Split by page number (from text) if available.
            // Otherwise no separators.
            let mut starts_entry = vec![false; count];
            let has_pages = results.iter().any(|m| page_number(&m.line_text).is_some());

            if has_pages {
                if count > 0 {
                    starts_entry[0] = true;
                    let mut prev_page = page_number(&results[0].line_text);
                    for i in 1..count {
                        let cur_page = page_number(&results[i].line_text);
                        let page_changed = cur_page.is_some()
                            && prev_page.is_some()
                            && cur_page != prev_page;
                        if page_changed {
                            starts_entry[i] = true;
                            prev_page = cur_page;
                        } else if cur_page.is_some() {
                            prev_page = cur_page;
                        }
                    }
                }
            }

            let mut current_path: Option<PathBuf> = None;
            let mut skip_until_path: Option<PathBuf> = None;

            for i in 0..count {
                let m = &results[i];

                // Skip if current file is collapsed
                if let Some(ref skip_path) = skip_until_path {
                    if m.path == *skip_path {
                        continue;
                    } else {
                        skip_until_path = None;
                    }
                }

                let is_new_file = current_path.as_ref() != Some(&m.path);
                if is_new_file {
                    current_path = Some(m.path.clone());
                    let name = file_name(&m.path);
                    let full = m.path.display().to_string();
                    let key = full.clone();
                    let collapsed = *app.collapsed_files.get(&key).unwrap_or(&false);

                    ui.add_space(4.0);
                    if word_wrap {
                        ui.horizontal_wrapped(|ui| {
                            render_file_header(
                                ui,
                                &mut app.collapsed_files,
                                &name,
                                &full,
                                &key,
                                collapsed,
                                &m.path,
                                &mut open_path,
                                &mut app.glob_filter,
                            );
                        });
                    } else {
                        ui.horizontal(|ui| {
                            render_file_header(
                                ui,
                                &mut app.collapsed_files,
                                &name,
                                &full,
                                &key,
                                collapsed,
                                &m.path,
                                &mut open_path,
                                &mut app.glob_filter,
                            );
                        });
                    }

                    if collapsed {
                        skip_until_path = Some(m.path.clone());
                        continue;
                    }
                }

                if starts_entry[i] {
                    ui.separator();
                }

                let row_resp = render_match_row(
                    ui,
                    m,
                    word_wrap,
                    if highlight_all {
                        &find_all_buckets[i]
                    } else {
                        &[]
                    },
                    if current_row == Some(i) { find_cur } else { None },
                );
                if find_need_scroll && current_row == Some(i) {
                    row_resp.scroll_to_me(Some(egui::Align::TOP));
                    did_scroll = true;
                }
            }
            if app.results.len() > limit {
                ui.colored_label(
                    Color32::YELLOW,
                    format!(
                        "… and {} more (limit reached to keep UI responsive)",
                        app.results.len() - limit
                    ),
                );
            }
        });

    if find_need_scroll && did_scroll {
        app.find_need_scroll = false;
    }

    if let Some(p) = open_path {
        open_file(app, p);
    }
}

// ── Find-in-results bar ───────────────────────────────────────────────────

fn render_find_bar(ui: &mut egui::Ui, app: &mut RgaGuiApp) {
    ui.horizontal(|ui| {
        ui.label("🔍");
        let find_id = egui::Id::new("find_input");
        let _resp = ui.add(
            egui::TextEdit::singleline(&mut app.find_text)
                .id(find_id)
                .hint_text("Find in results…")
                .desired_width(200.0),
        );

        let total = app.find_matches.len();
        let cur = if total == 0 { 0 } else { app.find_current + 1 };
        ui.label(format!("{cur} / {total}"));

        ui.separator();
        if ui
            .checkbox(&mut app.find_match_case, "Match case")
            .changed()
        {
            app.save_config();
        }
        if ui
            .checkbox(&mut app.find_whole_words, "Whole words")
            .changed()
        {
            app.save_config();
        }
        if ui
            .checkbox(&mut app.find_highlight_all, "Highlight all")
            .changed()
        {
            app.save_config();
        }

        ui.separator();
        if ui.button("Close").clicked() || (ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            app.show_find_bar = false;
        }
    });
}

// ── Match row ─────────────────────────────────────────────────────────────

fn render_match_row(
    ui: &mut egui::Ui,
    m: &crate::types::SearchMatch,
    word_wrap: bool,
    find_ranges: &[(usize, usize)],
    find_current_range: Option<(usize, usize)>,
) -> egui::Response {
    let ln_str = format_line_number(m.line_number);
    let visuals = ui.visuals().clone();
    let text_color = if m.is_context {
        visuals.weak_text_color()
    } else {
        visuals.text_color()
    };
    let highlight_color = if m.is_context {
        visuals.weak_text_color()
    } else {
        visuals.warn_fg_color
    };
    // find hit (cyan) vs current find hit (orange)
    let find_color = Color32::from_rgb(90, 180, 255);
    let find_current_color = Color32::from_rgb(100, 255, 100);

    let line_text = if m.is_context && m.line_text.trim().is_empty() {
        "(empty)".to_string()
    } else {
        m.line_text.clone()
    };

    let job = build_layout_job(
        &line_text,
        &m.submatches,
        find_ranges,
        find_current_range,
        text_color,
        highlight_color,
        find_color,
        find_current_color,
    );

    let resp = ui.horizontal(|ui| {
        ui.add(
            egui::Label::new(
                RichText::new(&ln_str)
                    .color(visuals.weak_text_color())
                    .monospace(),
            ),
        );
        ui.add(egui::Label::new(job.clone()).wrap_mode(if word_wrap {
            egui::TextWrapMode::Wrap
        } else {
            egui::TextWrapMode::Extend
        }));
    });
    resp.response
}

fn render_file_header(
    ui: &mut egui::Ui,
    collapsed_files: &mut HashMap<String, bool>,
    name: &str,
    _full: &str,
    key: &str,
    collapsed: bool,
    path: &std::path::Path,
    open_path: &mut Option<PathBuf>,
    glob_filter: &mut String,
) {
    let arrow = if collapsed { "[+]" } else { "[-]" };
    let header_resp = ui
        .add(
            egui::Label::new(
                RichText::new(format!("{arrow} 📄 {name}")).strong(),
            )
            .sense(Sense::click())
            .selectable(false),
        )
        .on_hover_cursor(CursorIcon::PointingHand);
    if header_resp.clicked() {
        collapsed_files.insert(key.to_string(), !collapsed);
    }
    if header_resp.secondary_clicked() {
        if let Some(parent) = path.parent() {
            *open_path = Some(parent.to_path_buf());
        }
    }
    if ui.button("Open").clicked() {
        *open_path = Some(path.to_path_buf());
    }
    if ui.button("🔍").on_hover_text("Filter to this file").clicked() {
        *glob_filter = format!("**/{}", name);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn page_number(text: &str) -> Option<u64> {
    // Extract "Page N:" or "Page N " prefix from PDF-extracted text
    let text = text.trim_start();
    if let Some(rest) = text.strip_prefix("Page ") {
        rest.split([':', ' ', '\n']).next()?.parse().ok()
    } else {
        None
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("???")
        .to_string()
}

fn format_line_number(ln: Option<u64>) -> String {
    match ln {
        Some(n) => format!("{:>5} │ ", n),
        None => String::new(),
    }
}

// ── Highlighting ──────────────────────────────────────────────────────────

fn build_layout_job(
    line_text: &str,
    submatches: &[SubMatch],
    find_ranges: &[(usize, usize)],
    find_current_range: Option<(usize, usize)>,
    default_color: Color32,
    highlight_color: Color32,
    find_color: Color32,
    find_current_color: Color32,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let len = line_text.len();

    // Gather spans with priority: lower number = higher priority color.
    // 0 = current find, 1 = find, 2 = rga match
    #[derive(Clone, Copy, PartialEq)]
    enum Kind { CurFind, Find, Rga }

    let mut spans: Vec<(usize, usize, Kind)> = Vec::new();

    for sm in submatches {
        let start = sm.start.min(len);
        let end = sm.end.min(len);
        if end > start {
            spans.push((start, end, Kind::Rga));
        }
    }
    for &(s, e) in find_ranges {
        let start = s.min(len);
        let end = e.min(len);
        if end > start {
            spans.push((start, end, Kind::Find));
        }
    }
    if let Some((s, e)) = find_current_range {
        let start = s.min(len);
        let end = e.min(len);
        if end > start {
            spans.push((start, end, Kind::CurFind));
        }
    }

    // Sort by start (priority handled by overlap resolution when emitting).
    spans.sort_by_key(|(s, _, _)| *s);

    let color_for = |kind: Kind| -> Color32 {
        match kind {
            Kind::CurFind => find_current_color,
            Kind::Find => find_color,
            Kind::Rga => highlight_color,
        }
    };
    fn prio(k: Kind) -> u8 {
        match k {
            Kind::CurFind => 3,
            Kind::Find => 2,
            Kind::Rga => 1,
        }
    }

    // Build sorted boundary points from all span starts/ends.
    let mut bounds: Vec<usize> = Vec::with_capacity(spans.len() * 2 + 2);
    bounds.push(0);
    bounds.push(len);
    for &(s, e, _) in &spans {
        bounds.push(s.min(len));
        bounds.push(e.min(len));
    }
    bounds.sort_unstable();
    bounds.dedup();

    // Emit segments between consecutive boundaries.
    for w in bounds.windows(2) {
        let lo = w[0];
        let hi = w[1];
        if hi <= lo {
            continue;
        }
        // Pick the highest-priority span covering [lo, hi).
        let mut chosen: Option<Kind> = None;
        for &(s, e, k) in &spans {
            if s <= lo && e >= hi {
                match chosen {
                    Some(c) if prio(k) <= prio(c) => {}
                    _ => chosen = Some(k),
                }
            }
        }
        let color = match chosen {
            Some(k) => color_for(k),
            None => default_color,
        };
        append_char_safe(&mut job, line_text, lo, hi, color);
    }

    job
}


/// Append a whole string already known to respect char boundaries.
fn append_text(job: &mut egui::text::LayoutJob, text: &str, color: Color32) {
    if text.is_empty() {
        return;
    }
    job.append(
        text,
        0.0,
        TextFormat {
            color,
            ..Default::default()
        },
    );
}

/// Slice `[start, end)` adjusting byte indices to the nearest char
/// boundaries to avoid panicking on multi-byte UTF-8.
fn append_char_safe(
    job: &mut egui::text::LayoutJob,
    text: &str,
    start: usize,
    end: usize,
    color: Color32,
) {
    let s = text.floor_char_boundary(start);
    let e = text.ceil_char_boundary(end.min(text.len()));
    if s < e {
        append_text(job, &text[s..e], color);
    }
}

// ── File opener ───────────────────────────────────────────────────────────

fn open_file(app: &mut RgaGuiApp, path: PathBuf) {
    match open::that(&path) {
        Ok(()) => {
            app.info_message = Some(format!("Opened {}", path.display()));
        }
        Err(e) => {
            app.error_message = Some(format!("Cannot open {}: {e}", path.display()));
        }
    }
}
