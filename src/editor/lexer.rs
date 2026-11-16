#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Plain,
    Keyword,
    Type,
    Func,
    Macro,
    Str,
    Num,
    Comment,
}

pub struct Token {
    pub start: usize,
    pub end: usize,
    pub kind: Tok,
}

const KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "if", "else", "match", "for", "while", "loop", "return", "break",
    "continue", "struct", "enum", "impl", "trait", "pub", "use", "mod", "const", "static",
    "as", "in", "self", "true", "false", "where", "type", "move", "ref", "async", "await",
];

const TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "usize", "isize", "f32", "f64",
    "bool", "char", "str",
];

fn push(out: &mut Vec<Token>, start: usize, end: usize, kind: Tok) {
    if let Some(last) = out.last_mut() {
        if last.kind == kind && last.end == start {
            last.end = end;
            return;
        }
    }
    out.push(Token { start, end, kind });
}

/// توکن‌ها کل متن را بدون سوراخ پوشش می‌دهند (start..end همیشه روی مرز UTF-8 است)
pub fn tokenize(src: &str) -> Vec<Token> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;

    while i < n {
        let start = i;
        let c = b[i];

        if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            push(&mut out, start, i, Tok::Comment);
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            i += 2;
            while i < n && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                i += 1;
            }
            i = (i + 2).min(n);
            push(&mut out, start, i, Tok::Comment);
        } else if c == b'"' {
            i += 1;
            while i < n && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(n);
            push(&mut out, start, i, Tok::Str);
        } else if c.is_ascii_digit() {
            while i < n
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'_'
                    || (b[i] == b'.' && b.get(i + 1).map_or(false, |d| d.is_ascii_digit())))
            {
                i += 1;
            }
            push(&mut out, start, i, Tok::Num);
        } else if c.is_ascii_alphabetic() || c == b'_' {
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let word = &src[start..i];
            let kind = if KEYWORDS.contains(&word) {
                Tok::Keyword
            } else if b.get(i) == Some(&b'!') && b.get(i + 1) != Some(&b'=') {
                Tok::Macro
            } else if TYPES.contains(&word) || word.starts_with(|ch: char| ch.is_ascii_uppercase()) {
                Tok::Type
            } else if b.get(i) == Some(&b'(') {
                Tok::Func
            } else {
                Tok::Plain
            };
            push(&mut out, start, i, kind);
        } else {
            i += 1;
            while i < n && !src.is_char_boundary(i) {
                i += 1;
            }
            push(&mut out, start, i, Tok::Plain);
        }
    }
    out
}