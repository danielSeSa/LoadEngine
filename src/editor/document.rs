use std::{fs, io, path::PathBuf};

use super::diagnostics::{self, Diagnostic};

pub struct Document {
    pub id: u64,
    pub path: PathBuf,
    pub text: String,
    pub dirty: bool,
    /// (anchor, caret) به‌صورت char index
    pub sel: (usize, usize),
    pub diagnostics: Vec<Diagnostic>,
}

impl Document {
    pub fn new(id: u64, path: PathBuf, text: String, dirty: bool) -> Self {
        let mut d = Self { id, path, text, dirty, sel: (0, 0), diagnostics: Vec::new() };
        d.refresh();
        d
    }

    pub fn open(id: u64, path: PathBuf) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?.replace("\r\n", "\n");
        Ok(Self::new(id, path, text, false))
    }

    pub fn save(&mut self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, &self.text)?;
        self.dirty = false;
        Ok(())
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// بعد از هر تغییر متن صدا زده می‌شود
    pub fn refresh(&mut self) {
        self.diagnostics = diagnostics::check(&self.text);
    }
}