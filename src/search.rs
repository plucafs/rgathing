#![allow(dead_code)]

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

use serde::Deserialize;

use crate::types::{DirTree, SearchMatch, SearchOptions, SearchStatus, SubMatch};

const MAX_LINE_LEN: usize = 2_000;

fn truncate_line(mut s: String) -> String {
    if s.len() > MAX_LINE_LEN {
        s.truncate(MAX_LINE_LEN);
        s.push_str("…");
    }
    s
}

// ── rga JSON wire types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum RgaEvent {
    #[serde(rename = "begin")]
    Begin { data: RgaBeginData },
    #[serde(rename = "match")]
    Match { data: RgaMatchData },
    #[serde(rename = "context")]
    Context { data: RgaMatchData },
    #[serde(rename = "end")]
    End { data: RgaEndData },
    #[serde(rename = "summary")]
    Summary { data: RgaSummaryData },
}

#[derive(Debug, Deserialize)]
struct RgaBeginData {
    path: RgaText,
}

#[derive(Debug, Deserialize)]
struct RgaMatchData {
    path: RgaText,
    lines: RgaText,
    line_number: Option<u64>,
    submatches: Vec<RgaSubmatch>,
}

#[derive(Debug, Deserialize)]
struct RgaEndData {
    path: RgaText,
    #[serde(default)]
    stats: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RgaSummaryData {
    #[serde(default)]
    stats: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RgaText {
    text: String,
}

#[derive(Debug, Deserialize)]
struct RgaSubmatch {
    #[serde(rename = "match")]
    match_info: RgaText,
    start: usize,
    end: usize,
}

/// Spawn rga --json [OPTIONS] PATTERN PATH... and stream results via a channel.
pub fn start_search(
    dir_tree: &DirTree,
    pattern: &str,
    opts: &SearchOptions,
) -> Result<(SearchStatus, Arc<AtomicBool>, mpsc::Receiver<SearchMatch>), String> {
    let search_paths = dir_tree.enabled_paths();

    if search_paths.is_empty() {
        return Err("No directories enabled for search.".into());
    }
    if pattern.trim().is_empty() {
        return Err("Enter a search pattern.".into());
    }

    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = cancel.clone();
    let (tx, rx) = mpsc::channel();

    let pattern = pattern.to_string();
    let glob_filter = opts.glob_filter.clone();
    let case_insensitive = opts.case_insensitive;
    let show_hidden = opts.show_hidden;
    let respect_gitignore = opts.respect_gitignore;
    let exact_match = opts.exact_match;
    let context_lines = opts.context_lines;

    thread::spawn(move || {
        let mut cmd = std::process::Command::new("rga");
        cmd.arg("--json");

        // ripgrep flags
        if case_insensitive {
            cmd.arg("-i");
        }
        if exact_match {
            cmd.arg("-F");
        }
        if show_hidden {
            cmd.arg("--hidden");
        }
        if !respect_gitignore {
            cmd.arg("--no-ignore");
        }
        if !glob_filter.is_empty() {
            cmd.arg("-g");
            cmd.arg(&glob_filter);
        }
        if context_lines > 0 {
            cmd.arg("-C");
            cmd.arg(context_lines.to_string());
        }

        cmd.arg(&pattern);
        for p in &search_paths {
            cmd.arg(p);
        }

        eprintln!("[rgathing] rga cmd: {:?}", cmd);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::null());

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(_) => return,
        };

        let stdout = match child.stdout.take() {
            Some(s) => s,
            None => return,
        };

        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines() {
            if cancel_clone.load(Ordering::Relaxed) {
                let _ = child.kill();
                return;
            }

            let line = match line {
                Ok(l) => l,
                Err(_) => break,
            };

            let event: RgaEvent = match serde_json::from_str(&line) {
                Ok(e) => e,
                Err(_) => continue,
            };

            match event {
                RgaEvent::Match { data } => {
                    let sm = SearchMatch {
                        path: std::path::PathBuf::from(data.path.text),
                        line_number: data.line_number,
                        line_text: truncate_line(data.lines.text),
                        submatches: data
                            .submatches
                            .into_iter()
                            .map(|s| SubMatch {
                                text: s.match_info.text,
                                start: s.start,
                                end: s.end,
                            })
                            .collect(),
                        is_context: false,
                    };
                    if tx.send(sm).is_err() {
                        return;
                    }
                }
                RgaEvent::Context { data } => {
                    let sm = SearchMatch {
                        path: std::path::PathBuf::from(data.path.text),
                        line_number: data.line_number,
                        line_text: truncate_line(data.lines.text),
                        submatches: Vec::new(),
                        is_context: true,
                    };
                    if tx.send(sm).is_err() {
                        return;
                    }
                }
                RgaEvent::Begin { .. } | RgaEvent::End { .. } | RgaEvent::Summary { .. } => {}
            }
        }

        let _ = child.wait();
    });

    Ok((SearchStatus::Searching, cancel, rx))
}

/// Drain all available results from the receiver. Returns the new search status.
pub fn collect_results(
    rx: &mut Option<mpsc::Receiver<SearchMatch>>,
    results: &mut Vec<SearchMatch>,
    _cancel_flag: &mut Option<Arc<AtomicBool>>,
) -> SearchStatus {
    if let Some(ref rx) = rx {
        loop {
            match rx.try_recv() {
                Ok(item) => results.push(item),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return SearchStatus::Done;
                }
            }
        }
        SearchStatus::Searching
    } else {
        SearchStatus::Idle
    }
}
