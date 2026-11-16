pub mod console;
pub mod diagnostics;
pub mod document;
pub mod edit;
pub mod find;
pub mod lexer;

use std::fs;
use std::path::{Path, PathBuf};

use console::{Console, Level};
use document::Document;
use find::{to_char_ranges, FindState};

const DEFAULT_SCRIPT: &str = r#"// LoadEngine Built-in Scripting
// Ctrl+S save | F5 run | Ctrl+F find | Ctrl+/ comment | Tab indent

struct Player {
    health: f32,
    speed: f32,
}

fn on_start() {
    let player = Player { health: 100.0, speed: 5.5 };
    println!("Player ready: {}", player.health);
}

fn on_update(dt: f32) {
    // game loop
}
"#;

pub struct EditorState {
    pub docs: Vec<Document>,
    pub active: usize,
    next_id: u64,
    pub project_dir: PathBuf,
    pub files: Vec<PathBuf>,
    pub console: Console,
    pub find: FindState,
    pub font_size: f32,
    pub show_explorer: bool,
    pub show_console: bool,
    /// انتخاب/جابجایی کرسر که UI در فریم بعد اعمال می‌کند (char index)
    pub pending_select: Option<(usize, usize)>,
}

impl EditorState {
    pub fn new() -> Self {
        let project_dir = PathBuf::from("scripts");
        let _ = fs::create_dir_all(&project_dir);

        let main_path = project_dir.join("main.les");
        let mut files = Vec::new();
        scan(&project_dir, 3, &mut files);
        if files.is_empty() {
            let _ = fs::write(&main_path, DEFAULT_SCRIPT);
        }

        let mut ed = Self {
            docs: Vec::new(),
            active: 0,
            next_id: 1,
            project_dir,
            files: Vec::new(),
            console: Console::new(),
            find: FindState::default(),
            font_size: 15.0,
            show_explorer: true,
            show_console: true,
            pending_select: None,
        };
        ed.refresh_files();
        ed.console.info("Editor ready. Press F5 to build the active script.");
        if main_path.exists() {
            ed.open_file(main_path);
        } else if let Some(first) = ed.files.first().cloned() {
            ed.open_file(first);
        }
        ed
    }

    pub fn refresh_files(&mut self) {
        let mut files = Vec::new();
        scan(&self.project_dir, 3, &mut files);
        self.files = files;
    }

    pub fn open_file(&mut self, path: PathBuf) {
        if let Some(i) = self.docs.iter().position(|d| d.path == path) {
            self.active = i;
            return;
        }
        match Document::open(self.next_id, path.clone()) {
            Ok(doc) => {
                self.next_id += 1;
                self.docs.push(doc);
                self.active = self.docs.len() - 1;
            }
            Err(e) => self.console.error(format!("Cannot open {}: {e}", path.display())),
        }
    }

    pub fn new_file(&mut self) {
        let mut n = 1;
        let path = loop {
            let p = self.project_dir.join(format!("script_{n}.les"));
            if !p.exists() && !self.docs.iter().any(|d| d.path == p) {
                break p;
            }
            n += 1;
        };
        let doc = Document::new(self.next_id, path, "// New script\n".to_string(), true);
        self.next_id += 1;
        self.docs.push(doc);
        self.active = self.docs.len() - 1;
    }

    pub fn close_tab(&mut self, idx: usize) {
        if idx >= self.docs.len() {
            return;
        }
        // موقتاً به‌جای دیالوگ تأیید، تب تغییر‌کرده قبل از بسته شدن ذخیره می‌شود
        if self.docs[idx].dirty {
            match self.docs[idx].save() {
                Ok(()) => {
                    let name = self.docs[idx].name();
                    self.console.info(format!("Auto-saved {name}"));
                }
                Err(e) => self.console.error(format!("Auto-save failed: {e}")),
            }
        }
        self.docs.remove(idx);
        if self.active > idx {
            self.active -= 1;
        }
        self.active = self.active.min(self.docs.len().saturating_sub(1));
        self.refresh_files();
    }

    pub fn save_active(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let name = doc.name();
        match doc.save() {
            Ok(()) => {
                self.console.success(format!("Saved {name}"));
                self.refresh_files();
            }
            Err(e) => self.console.error(format!("Failed to save {name}: {e}")),
        }
    }

    pub fn run_active(&mut self) {
        if self.docs.get(self.active).map_or(false, |d| d.dirty) {
            self.save_active();
        }
        let Some(doc) = self.docs.get(self.active) else { return };
        let name = doc.name();
        let diags = diagnostics::check(&doc.text);

        self.console.build(format!("Compiling {name}..."));
        if diags.is_empty() {
            self.console
                .success(format!("{name}: 0 errors (script runtime not wired yet)"));
        } else {
            for d in &diags {
                self.console.push(
                    Level::Error,
                    format!("{name}:{}:{}: {}", d.line + 1, d.col + 1, d.message),
                    Some((d.line, d.col)),
                );
            }
            self.console
                .error(format!("Build failed with {} error(s)", diags.len()));
        }
    }

    pub fn goto(&mut self, line: usize, col: usize) {
        if let Some(doc) = self.docs.get(self.active) {
            let total = doc.text.chars().count();
            let idx = (edit::line_to_char(&doc.text, line) + col).min(total);
            self.pending_select = Some((idx, idx));
        }
    }

    // ---------- Find & Replace ----------

    pub fn match_count(&self) -> usize {
        self.docs
            .get(self.active)
            .map_or(0, |d| self.find.matches(&d.text).len())
    }

    pub fn find_next(&mut self, forward: bool) {
        let Some(doc) = self.docs.get(self.active) else { return };
        let byte_matches = self.find.matches(&doc.text);
        if byte_matches.is_empty() {
            return;
        }
        let ranges = to_char_ranges(&doc.text, &byte_matches);
        let (lo, hi) = (doc.sel.0.min(doc.sel.1), doc.sel.0.max(doc.sel.1));
        let pick = if forward {
            ranges.iter().find(|r| r.0 >= hi).or(ranges.first())
        } else {
            ranges.iter().rev().find(|r| r.1 <= lo).or(ranges.last())
        };
        self.pending_select = pick.copied();
    }

    pub fn replace_current(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let (a, b) = (doc.sel.0.min(doc.sel.1), doc.sel.0.max(doc.sel.1));
        let selected: String = doc.text.chars().skip(a).take(b - a).collect();
        let eq = if self.find.case_sensitive {
            selected == self.find.query
        } else {
            selected.eq_ignore_ascii_case(&self.find.query)
        };
        if a != b && eq {
            let c = edit::replace_range_chars(&mut doc.text, a, b, &self.find.replace);
            doc.sel = (c, c);
            doc.dirty = true;
            doc.refresh();
            self.pending_select = Some((c, c));
        }
        self.find_next(true);
    }

    pub fn replace_all(&mut self) {
        let Some(doc) = self.docs.get_mut(self.active) else { return };
        let m = self.find.matches(&doc.text);
        if m.is_empty() {
            return;
        }
        let mut out = String::with_capacity(doc.text.len());
        let mut last = 0;
        for &(s, e) in &m {
            out.push_str(&doc.text[last..s]);
            out.push_str(&self.find.replace);
            last = e;
        }
        out.push_str(&doc.text[last..]);
        doc.text = out;
        doc.dirty = true;
        doc.refresh();
        let c = doc.sel.1.min(doc.text.chars().count());
        doc.sel = (c, c);
        self.pending_select = Some((c, c));
        self.console.info(format!("Replaced {} occurrence(s)", m.len()));
    }
}

fn scan(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    if let Ok(rd) = fs::read_dir(dir) {
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if p.is_dir() {
                scan(&p, depth - 1, out);
            } else {
                out.push(p);
            }
        }
    }
}