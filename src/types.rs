use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ── Search matches ────────────────────────────────────────────────────────

/// A single submatch (highlighted span within a line).
#[derive(Clone, Debug)]
pub struct SubMatch {
    #[allow(dead_code)]
    pub text: String,
    pub start: usize,
    pub end: usize,
}

/// One match found by rga inside a file.
#[derive(Clone, Debug)]
pub struct SearchMatch {
    pub path: PathBuf,
    pub line_number: Option<u64>,
    pub line_text: String,
    pub submatches: Vec<SubMatch>,
    pub is_context: bool,
}

// ── Search status ─────────────────────────────────────────────────────────

pub enum SearchStatus {
    Idle,
    Searching,
    Done,
}

// ── Focus target ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum FocusTarget {
    Pattern,
}

// ── Search options ────────────────────────────────────────────────────────

pub struct SearchOptions {
    pub case_insensitive: bool,
    pub show_hidden: bool,
    pub respect_gitignore: bool,
    pub glob_filter: String,
    pub exact_match: bool,
    pub context_lines: u32,
}

// ── Directory tree ────────────────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize)]
pub struct SearchDir {
    pub id: u64,
    pub path: PathBuf,
    pub enabled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DirGroup {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub collapsed: bool,
    pub dirs: Vec<SearchDir>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DirTree {
    pub free: Vec<SearchDir>,
    pub groups: Vec<DirGroup>,
    pub next_id: u64,
}

impl DirTree {
    pub fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn enabled_paths(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        for d in &self.free {
            if d.enabled {
                out.push(d.path.clone());
            }
        }
        for g in &self.groups {
            if g.enabled {
                for d in &g.dirs {
                    if d.enabled {
                        out.push(d.path.clone());
                    }
                }
            }
        }
        out
    }

    pub fn total_dirs(&self) -> usize {
        self.free.len() + self.groups.iter().map(|g| g.dirs.len()).sum::<usize>()
    }

    pub fn group_mut(&mut self, group_id: u64) -> Option<&mut DirGroup> {
        self.groups.iter_mut().find(|g| g.id == group_id)
    }

    pub fn remove_dir(&mut self, dir_id: u64) -> Option<SearchDir> {
        if let Some(idx) = self.free.iter().position(|d| d.id == dir_id) {
            Some(self.free.remove(idx))
        } else {
            for g in &mut self.groups {
                if let Some(idx) = g.dirs.iter().position(|d| d.id == dir_id) {
                    return Some(g.dirs.remove(idx));
                }
            }
            None
        }
    }
}

// ── Persistence ───────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct PersistedConfig {
    #[serde(default)]
    pub dir_tree: DirTree,
    #[serde(default)]
    pub pattern: String,
    #[serde(default = "default_ui_scale")]
    pub ui_scale: f32,
    #[serde(default)]
    pub light_mode: bool,
    #[serde(default)]
    pub case_insensitive: bool,
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default = "default_true")]
    pub respect_gitignore: bool,
    #[serde(default)]
    pub glob_filter: String,
    #[serde(default)]
    pub context_lines: u32,
    #[serde(default)]
    pub exact_match: bool,
    #[serde(default)]
    pub word_wrap: bool,
    #[serde(default = "default_max_width")]
    pub max_width_chars: f32,
    #[serde(default)]
    pub search_history: Vec<String>,
    #[serde(default)]
    pub find_match_case: bool,
    #[serde(default)]
    pub find_whole_words: bool,
    #[serde(default)]
    pub find_highlight_all: bool,
}

fn default_max_width() -> f32 {
    80.0
}

fn default_ui_scale() -> f32 {
    1.40
}

fn default_true() -> bool {
    true
}

pub fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("rgathing").join("config.json"))
}

pub fn load_config() -> Option<PersistedConfig> {
    let text = std::fs::read_to_string(config_path()?).ok()?;
    let mut cfg: PersistedConfig = serde_json::from_str(&text).ok()?;
    if cfg.dir_tree.next_id == 0 {
        cfg.dir_tree.next_id = cfg.dir_tree.total_dirs() as u64 + 10;
    }
    let mut max_id = cfg.dir_tree.next_id;
    for d in &cfg.dir_tree.free {
        max_id = max_id.max(d.id);
    }
    for g in &cfg.dir_tree.groups {
        for d in &g.dirs {
            max_id = max_id.max(d.id);
        }
    }
    cfg.dir_tree.next_id = max_id + 1;
    Some(cfg)
}
