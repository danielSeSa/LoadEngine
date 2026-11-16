#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Build,
    Success,
    Warn,
    Error,
}

pub struct LogEntry {
    pub level: Level,
    pub text: String,
    /// اگر وجود داشته باشد، کلیک روی پیام به این (خط، ستون) می‌رود
    pub goto: Option<(usize, usize)>,
}

pub struct Console {
    pub entries: Vec<LogEntry>,
    pub show_info: bool,
    pub show_warn: bool,
    pub show_error: bool,
    pub auto_scroll: bool,
}

impl Console {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            show_info: true,
            show_warn: true,
            show_error: true,
            auto_scroll: true,
        }
    }

    pub fn push(&mut self, level: Level, text: impl Into<String>, goto: Option<(usize, usize)>) {
        if self.entries.len() >= 2000 {
            self.entries.drain(..500);
        }
        self.entries.push(LogEntry { level, text: text.into(), goto });
    }

    pub fn info(&mut self, t: impl Into<String>) {
        self.push(Level::Info, t, None);
    }
    pub fn build(&mut self, t: impl Into<String>) {
        self.push(Level::Build, t, None);
    }
    pub fn success(&mut self, t: impl Into<String>) {
        self.push(Level::Success, t, None);
    }
    #[allow(dead_code)]
    pub fn warn(&mut self, t: impl Into<String>) {
        self.push(Level::Warn, t, None);
    }
    pub fn error(&mut self, t: impl Into<String>) {
        self.push(Level::Error, t, None);
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn is_visible(&self, l: Level) -> bool {
        match l {
            Level::Warn => self.show_warn,
            Level::Error => self.show_error,
            _ => self.show_info,
        }
    }
}