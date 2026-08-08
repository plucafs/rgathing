use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};

use eframe::egui;

use crate::search;
use crate::types::{
    config_path, load_config, DirTree, FocusTarget, PersistedConfig, SearchDir, SearchMatch,
    SearchOptions, SearchStatus,
};

pub struct RgaGuiApp {
    // ── Directories ──────────────────────────────────────────────────────
    pub dir_tree: DirTree,
    pub new_dir_path: String,
    pub new_group_name: String,

    // ── Search parameters ────────────────────────────────────────────────
    pub pattern: String,
    pub case_insensitive: bool,
    pub show_hidden: bool,
    pub respect_gitignore: bool,
    pub glob_filter: String,
    pub context_lines: u32,
    pub exact_match: bool,

    // ── Results presentation ─────────────────────────────────────────────
    pub word_wrap: bool,
    pub max_width_chars: f32,

    // ── Search history ───────────────────────────────────────────────────
    pub search_history: Vec<String>,
    pub history_index: Option<usize>,

    // ── Find-in-results bar ──────────────────────────────────────────────
    pub show_find_bar: bool,
    pub find_text: String,
    pub find_match_case: bool,
    pub find_whole_words: bool,
    pub find_highlight_all: bool,
    pub find_current: usize,
    pub find_need_scroll: bool,
    /// (result_index, start_byte, end_byte) of each find match, cached.
    pub find_matches: Vec<(usize, usize, usize)>,
    pub find_cache_key: u64,

    // ── Appearance ───────────────────────────────────────────────────────
    pub ui_scale: f32,
    pub light_mode: bool,

    // ── Search runtime state ─────────────────────────────────────────────
    pub results: Vec<SearchMatch>,
    pub search_status: SearchStatus,
    pub cancel_flag: Option<Arc<AtomicBool>>,
    result_receiver: Option<mpsc::Receiver<SearchMatch>>,
    pub error_message: Option<String>,
    pub info_message: Option<String>,

    // ── UI state ─────────────────────────────────────────────────────────
    pub focus_after_search: Option<FocusTarget>,
    pub pending_enable: Vec<(u64, bool)>,
    pub show_about: bool,
    pub collapsed_files: HashMap<String, bool>,
}

impl RgaGuiApp {
    pub fn new() -> Self {
        let cwd = dirs::download_dir().unwrap_or_else(|| {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        });
        let mut app = Self {
            dir_tree: DirTree::default(),
            new_dir_path: String::new(),
            new_group_name: String::new(),
            pattern: String::new(),
            case_insensitive: false,
            show_hidden: false,
            respect_gitignore: true,
            glob_filter: String::new(),
            context_lines: 0,
            exact_match: false,
            word_wrap: false,
            max_width_chars: 80.0,

            // ── Search history ──
            search_history: Vec::new(),
            history_index: None,

            // ── Find-in-results bar ──
            show_find_bar: false,
            find_text: String::new(),
            find_match_case: false,
            find_whole_words: false,
            find_highlight_all: false,
            find_current: 0,
            find_need_scroll: false,
            find_matches: Vec::new(),
            find_cache_key: 0,
            ui_scale: 1.40,
            light_mode: false,
            results: Vec::new(),
            search_status: SearchStatus::Idle,
            cancel_flag: None,
            result_receiver: None,
            error_message: None,
            info_message: None,
            focus_after_search: Some(FocusTarget::Pattern),
            pending_enable: Vec::new(),
            show_about: false,
            collapsed_files: HashMap::new(),
        };

        if let Some(cfg) = load_config() {
            app.dir_tree = cfg.dir_tree;
            app.pattern = cfg.pattern;
            app.ui_scale = cfg.ui_scale;
            app.light_mode = cfg.light_mode;
            app.case_insensitive = cfg.case_insensitive;
            app.show_hidden = cfg.show_hidden;
            app.respect_gitignore = cfg.respect_gitignore;
            app.glob_filter = cfg.glob_filter;
            app.context_lines = cfg.context_lines;
            app.word_wrap = cfg.word_wrap;
            app.max_width_chars = cfg.max_width_chars;
            app.search_history = cfg.search_history;
            app.find_match_case = cfg.find_match_case;
            app.find_whole_words = cfg.find_whole_words;
            app.find_highlight_all = cfg.find_highlight_all;
        }

        if app.dir_tree.total_dirs() == 0 {
            let id = app.dir_tree.alloc_id();
            app.dir_tree.free.push(SearchDir {
                id,
                path: cwd,
                enabled: true,
            });
        }

        app
    }

    pub fn save_config(&self) {
        let Some(path) = config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let cfg = PersistedConfig {
            dir_tree: self.dir_tree.clone(),
            pattern: self.pattern.clone(),
            ui_scale: self.ui_scale,
            light_mode: self.light_mode,
            case_insensitive: self.case_insensitive,
            show_hidden: self.show_hidden,
            respect_gitignore: self.respect_gitignore,
            glob_filter: self.glob_filter.clone(),
            context_lines: self.context_lines,
            exact_match: self.exact_match,
            word_wrap: self.word_wrap,
            max_width_chars: self.max_width_chars,
            search_history: self.search_history.clone(),
            find_match_case: self.find_match_case,
            find_whole_words: self.find_whole_words,
            find_highlight_all: self.find_highlight_all,
        };
        if let Ok(json) = serde_json::to_string_pretty(&cfg) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn start_search(&mut self) {
        if let Some(ref flag) = self.cancel_flag {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        // Record pattern into search history (no consecutive duplicates).
        let pattern = self.pattern.trim();
        if !pattern.is_empty() {
            if self
                .search_history
                .last()
                .map(|p| p.as_str() != pattern)
                .unwrap_or(true)
            {
                self.search_history.push(pattern.to_string());
            }
            const MAX_HISTORY: usize = 50;
            if self.search_history.len() > MAX_HISTORY {
                let drop_n = self.search_history.len() - MAX_HISTORY;
                self.search_history.drain(0..drop_n);
            }
        }
        self.history_index = None;

        self.results.clear();
        self.error_message = None;
        self.info_message = None;

        let opts = SearchOptions {
            case_insensitive: self.case_insensitive,
            show_hidden: self.show_hidden,
            respect_gitignore: self.respect_gitignore,
            glob_filter: self.glob_filter.clone(),
            context_lines: self.context_lines,
            exact_match: self.exact_match,
        };

        match search::start_search(&self.dir_tree, &self.pattern, &opts) {
            Ok((status, cancel, rx)) => {
                self.search_status = status;
                self.cancel_flag = Some(cancel);
                self.result_receiver = Some(rx);
                self.save_config();
            }
            Err(msg) => {
                self.error_message = Some(msg);
                self.search_status = SearchStatus::Idle;
            }
        }
    }

    fn collect_and_update(&mut self) {
        self.search_status = search::collect_results(
            &mut self.result_receiver,
            &mut self.results,
            &mut self.cancel_flag,
        );
    }

    /// Recompute find-in-results matches when inputs change. Cached by a key
    /// derived from the needle text, options and result count.
    pub fn update_find_matches(&mut self) {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.find_text.hash(&mut h);
        self.find_match_case.hash(&mut h);
        self.find_whole_words.hash(&mut h);
        self.results.len().hash(&mut h);
        let key = h.finish();
        if key == self.find_cache_key && !self.results.is_empty() {
            return;
        }
        self.find_cache_key = key;
        self.find_matches.clear();

        if self.find_text.is_empty() {
            return;
        }

        let needle = self.find_text.as_str();
        let needle_lower = if self.find_match_case {
            None
        } else {
            Some(needle.to_lowercase())
        };

        for (ri, m) in self.results.iter().enumerate() {
            let hay = &m.line_text;
            let (search_hay, search_ndl) = match needle_lower.as_ref() {
                None => (hay.as_bytes(), needle.as_bytes()),
                Some(low) => {
                    // Lowercase may change byte length; fall back to char scan.
                    find_ranges_ci(hay, low, self.find_whole_words, ri, &mut self.find_matches);
                    continue;
                }
            };
            find_ranges_cs(search_hay, search_ndl, self.find_whole_words, ri, hay, &mut self.find_matches);
        }

        if self.find_current >= self.find_matches.len() && !self.find_matches.is_empty() {
            self.find_current = 0;
            self.find_need_scroll = true;
        } else if self.find_matches.is_empty() {
            self.find_current = 0;
        }
    }
}

/// Case-sensitive substring ranges (whole-words optional).
fn find_ranges_cs(
    hay_bytes: &[u8],
    ndl_bytes: &[u8],
    whole_words: bool,
    ri: usize,
    hay: &str,
    out: &mut Vec<(usize, usize, usize)>,
) {
    if ndl_bytes.is_empty() || ndl_bytes.len() > hay_bytes.len() {
        return;
    }
    let mut start = 0;
    while let Some(found) = memchr_find(hay_bytes, ndl_bytes, start) {
        let end = found + ndl_bytes.len();
        if whole_words && !is_word_boundary(hay, found, end) {
            start = found + 1;
            continue;
        }
        out.push((ri, found, end));
        start = end;
    }
}

/// Case-insensitive substring ranges using char-level scanning.
fn find_ranges_ci(
    hay: &str,
    needle_lower: &str,
    whole_words: bool,
    ri: usize,
    out: &mut Vec<(usize, usize, usize)>,
) {
    if needle_lower.is_empty() {
        return;
    }
    let hay_lower = hay.to_lowercase();
    let ndl = needle_lower.as_bytes();
    let hay_b = hay_lower.as_bytes();
    let mut start = 0;
    while let Some(found) = memchr_find(hay_b, ndl, start) {
        let end = found + ndl.len();
        // Map back to the original string byte positions by char index.
        // Because lowercasing preserves the number of characters (not bytes),
        // we recompute byte offsets in the original using char iteration.
        let (orig_start, orig_end) = map_ci_offsets(hay, &hay_lower, found, end);
        if whole_words && !is_word_boundary(hay, orig_start, orig_end) {
            start = end;
            continue;
        }
        out.push((ri, orig_start, orig_end));
        start = end;
    }
}

fn memchr_find(hay: &[u8], ndl: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    hay[from..]
        .windows(ndl.len())
        .position(|w| w == ndl)
        .map(|p| p + from)
}

fn map_ci_offsets(orig: &str, lower: &str, lo_start: usize, lo_end: usize) -> (usize, usize) {
    // Align the lowercased substring [lo_start, lo_end) back to the original
    // string by counting leading characters up to each char boundary.
    let lo_start = lower.floor_char_boundary(lo_start);
    let lo_end = lower.ceil_char_boundary(lo_end.min(lower.len()));
    let n_start = lower[..lo_start].chars().count();
    let n_end = lower[..lo_end].chars().count();
    let o_start = orig
        .char_indices()
        .nth(n_start)
        .map(|(i, _)| i)
        .unwrap_or(orig.len());
    let o_end = orig
        .char_indices()
        .nth(n_end)
        .map(|(i, _)| i)
        .unwrap_or(orig.len());
    (o_start, o_end)
}

fn is_word_boundary(text: &str, start: usize, end: usize) -> bool {
    let before = text.floor_char_boundary(start);
    // Character immediately before the match
    let pre_is_word = before > 0 && is_word_char(text[..before].chars().next_back().unwrap_or(' '));
    let post_is_word = text[end..].chars().next().map(is_word_char).unwrap_or(false);
    !pre_is_word && !post_is_word
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

// ── eframe App ────────────────────────────────────────────────────────────

impl eframe::App for RgaGuiApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_config();
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui_scale = ctx.zoom_factor();
        ctx.set_visuals(if self.light_mode {
            egui::Visuals::light()
        } else {
            egui::Visuals::dark()
        });

        self.collect_and_update();

        if matches!(self.search_status, SearchStatus::Searching) {
            ctx.request_repaint();
        }
        if ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Q)) {
            ctx.send_viewport_cmd(egui::viewport::ViewportCommand::Close);
        }

        // ── Find-in-results shortcuts ─────────────────────────────────────
        // Keep find matches fresh while the bar is open.
        self.update_find_matches();
        let total = self.find_matches.len();

        let ctrl_f = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::F));
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape)) && self.show_find_bar;
        let f3 = ctx.input(|i| i.key_pressed(egui::Key::F3));
        let ctrl_g = ctx.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::G));
        let enter_find = self.show_find_bar
            && ctx.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.ctrl && !i.modifiers.shift);
        let shift_enter = self.show_find_bar
            && ctx.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.shift);

        if ctrl_f {
            self.show_find_bar = !self.show_find_bar;
            if self.show_find_bar {
                self.find_current = 0;
                self.find_need_scroll = true;
                ctx.memory_mut(|m| m.request_focus(egui::Id::new("find_input")));
            }
        }
        if esc && self.show_find_bar {
            self.show_find_bar = false;
        }
        if total > 0 && (enter_find || f3 || ctrl_g) {
            self.find_current = (self.find_current + 1) % total;
            self.find_need_scroll = true;
        }
        if total > 0 && shift_enter {
            self.find_current = (self.find_current + total - 1) % total;
            self.find_need_scroll = true;
        }

        // Apply pending enables
        let pending = std::mem::take(&mut self.pending_enable);
        for (dir_id, on) in pending {
            if let Some(idx) = self.dir_tree.free.iter().position(|d| d.id == dir_id) {
                self.dir_tree.free[idx].enabled = on;
            } else {
                for g in &mut self.dir_tree.groups {
                    if let Some(idx) = g.dirs.iter().position(|d| d.id == dir_id) {
                        g.dirs[idx].enabled = on;
                        break;
                    }
                }
            }
        }

        // UI
        crate::ui::menu::show(ctx, self);
        crate::ui::top_bar::show(ctx, self);
        crate::ui::dir_tree::show(ctx, self);
        crate::ui::results::show(ctx, self);
    }
}
