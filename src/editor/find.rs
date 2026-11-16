#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub query: String,
    pub replace: String,
    pub case_sensitive: bool,
    pub focus_query: bool,
}

impl FindState {
    /// خروجی: بازه‌های بایتی (start, end)
    pub fn matches(&self, text: &str) -> Vec<(usize, usize)> {
        if self.query.is_empty() {
            return Vec::new();
        }
        if self.case_sensitive {
            text.match_indices(self.query.as_str())
                .map(|(i, s)| (i, i + s.len()))
                .collect()
        } else {
            // ascii lowercase طول بایت را حفظ می‌کند، پس ایندکس‌ها معتبر می‌مانند
            let t = text.to_ascii_lowercase();
            let q = self.query.to_ascii_lowercase();
            t.match_indices(q.as_str())
                .map(|(i, s)| (i, i + s.len()))
                .collect()
        }
    }
}

/// تبدیل بازه‌های بایتی (مرتب و بدون هم‌پوشانی) به بازه‌های char
pub fn to_char_ranges(text: &str, m: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(m.len());
    let (mut last_b, mut last_c) = (0usize, 0usize);
    for &(s, e) in m {
        last_c += text[last_b..s].chars().count();
        last_b = s;
        out.push((last_c, last_c + text[s..e].chars().count()));
    }
    out
}