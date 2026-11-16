pub const TAB_WIDTH: usize = 4;

pub fn ordered(sel: (usize, usize)) -> (usize, usize) {
    (sel.0.min(sel.1), sel.0.max(sel.1))
}

pub fn byte_idx(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map(|(b, _)| b).unwrap_or(text.len())
}

pub fn line_col(text: &str, char_idx: usize) -> (usize, usize) {
    let (mut line, mut col) = (0, 0);
    for (i, c) in text.chars().enumerate() {
        if i >= char_idx {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

pub fn line_start(text: &str, char_idx: usize) -> usize {
    let mut start = 0;
    for (i, c) in text.chars().enumerate() {
        if i >= char_idx {
            break;
        }
        if c == '\n' {
            start = i + 1;
        }
    }
    start
}

pub fn line_to_char(text: &str, line: usize) -> usize {
    if line == 0 {
        return 0;
    }
    let mut l = 0;
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            l += 1;
            if l == line {
                return i + 1;
            }
        }
    }
    text.chars().count()
}

/// بازه‌ی [a, b) را با s عوض می‌کند و ایندکس کرسر بعد از s را برمی‌گرداند
pub fn replace_range_chars(text: &mut String, a: usize, b: usize, s: &str) -> usize {
    let (ba, bb) = (byte_idx(text, a), byte_idx(text, b));
    text.replace_range(ba..bb, s);
    a + s.chars().count()
}

/// Enter هوشمند: حفظ indent، افزودن یک سطح بعد از `{ ( [`، و باز کردن `{|}`
pub fn newline(text: &mut String, sel: (usize, usize)) -> usize {
    let (a, b) = ordered(sel);
    let ls = line_start(text, a);
    let prefix: String = text.chars().skip(ls).take(a - ls).collect();
    let indent: String = prefix.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
    let last = prefix.trim_end().chars().last();
    let next = text.chars().nth(b);
    let unit = " ".repeat(TAB_WIDTH);

    if matches!(last, Some('{' | '(' | '[')) {
        let closes = matches!(
            (last, next),
            (Some('{'), Some('}')) | (Some('('), Some(')')) | (Some('['), Some(']'))
        );
        if closes {
            let ins = format!("\n{indent}{unit}\n{indent}");
            let caret = a + 1 + indent.chars().count() + TAB_WIDTH;
            replace_range_chars(text, a, b, &ins);
            return caret;
        }
        return replace_range_chars(text, a, b, &format!("\n{indent}{unit}"));
    }
    replace_range_chars(text, a, b, &format!("\n{indent}"))
}

/// تایپ کاراکتر با auto-close / skip-over / wrap انتخاب
pub fn type_char(text: &mut String, sel: (usize, usize), c: char) -> (usize, usize) {
    let (a, b) = ordered(sel);
    let next = text.chars().nth(b);
    let prev = if a > 0 { text.chars().nth(a - 1) } else { None };
    let close = match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        _ => None,
    };

    if a == b && matches!(c, ')' | ']' | '}' | '"') && next == Some(c) {
        return (a + 1, a + 1);
    }

    if let Some(close) = close {
        if a != b {
            let (ba, bb) = (byte_idx(text, a), byte_idx(text, b));
            text.insert(bb, close);
            text.insert(ba, c);
            return (a + 1, b + 1);
        }
        let word_before = prev.map_or(false, |p| p.is_alphanumeric() || p == '_');
        let next_ok = next
            .map_or(true, |n| n.is_whitespace() || matches!(n, ')' | ']' | '}' | ',' | ';'));
        if !(c == '"' && word_before) && next_ok {
            replace_range_chars(text, a, a, &format!("{c}{close}"));
            return (a + 1, a + 1);
        }
    }

    let caret = replace_range_chars(text, a, b, &c.to_string());
    (caret, caret)
}

struct Span {
    sl: usize,
    sc: usize,
    el: usize,
    ec: usize,
    last: usize,
}

fn span(text: &str, a: usize, b: usize) -> Span {
    let (sl, sc) = line_col(text, a);
    let (el, ec) = line_col(text, b);
    // اگر انتخاب دقیقاً اول یک خط تمام شود، آن خط جزو انتخاب حساب نمی‌شود
    let last = if b > a && ec == 0 && el > sl { el - 1 } else { el };
    Span { sl, sc, el, ec, last }
}

/// Tab / Shift+Tab
pub fn tab(text: &mut String, sel: (usize, usize), shift: bool) -> (usize, usize) {
    let (a, b) = ordered(sel);
    if shift {
        return shift_lines(text, sel, true);
    }
    if a == b {
        let (_, col) = line_col(text, a);
        let n = TAB_WIDTH - col % TAB_WIDTH;
        let c = replace_range_chars(text, a, a, &" ".repeat(n));
        return (c, c);
    }
    shift_lines(text, sel, false)
}

pub fn shift_lines(text: &mut String, sel: (usize, usize), dedent: bool) -> (usize, usize) {
    let (a, b) = ordered(sel);
    let sp = span(text, a, b);
    let mut out = String::with_capacity(text.len() + 64);
    let (mut na, mut nb) = (a, b);

    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i < sp.sl || i > sp.last {
            out.push_str(line);
            continue;
        }
        if dedent {
            let mut n = line.chars().take(TAB_WIDTH).take_while(|c| *c == ' ').count();
            if n == 0 && line.starts_with('\t') {
                n = 1;
            }
            out.extend(line.chars().skip(n));
            if i == sp.sl {
                na = na.saturating_sub(n.min(sp.sc));
            }
            if i < sp.el {
                nb = nb.saturating_sub(n);
            } else if i == sp.el {
                nb = nb.saturating_sub(n.min(sp.ec));
            }
        } else {
            if line.is_empty() {
                continue;
            }
            out.push_str(&" ".repeat(TAB_WIDTH));
            out.push_str(line);
            if i == sp.sl && sp.sc > 0 {
                na += TAB_WIDTH;
            }
            if i < sp.el {
                nb += TAB_WIDTH;
            } else if i == sp.el && sp.ec > 0 {
                nb += TAB_WIDTH;
            }
        }
    }
    *text = out;
    (na, nb)
}

/// Ctrl+/
pub fn toggle_comment(text: &mut String, sel: (usize, usize)) -> (usize, usize) {
    let (a, b) = ordered(sel);
    let sp = span(text, a, b);
    let lines: Vec<&str> = text.split('\n').collect();
    let total_lines = lines.len();

    let non_blank: Vec<&str> = lines[sp.sl..=sp.last]
        .iter()
        .copied()
        .filter(|l| !l.trim().is_empty())
        .collect();
    if non_blank.is_empty() {
        return (a, b);
    }
    let uncomment = non_blank.iter().all(|l| l.trim_start().starts_with("//"));
    let min_indent = non_blank
        .iter()
        .map(|l| l.chars().take_while(|c| c.is_whitespace()).count())
        .min()
        .unwrap_or(0);

    let mut out = String::with_capacity(text.len() + 16);
    let mut caret_delta: isize = 0;

    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if i < sp.sl || i > sp.last || line.trim().is_empty() {
            out.push_str(line);
            continue;
        }
        let d: isize;
        if uncomment {
            let lead = line.chars().take_while(|c| c.is_whitespace()).count();
            let (ws, body) = line.split_at(byte_idx(line, lead));
            let rest = body.strip_prefix("//").unwrap_or(body);
            let rest2 = rest.strip_prefix(' ').unwrap_or(rest);
            out.push_str(ws);
            out.push_str(rest2);
            d = -((body.chars().count() - rest2.chars().count()) as isize);
        } else {
            let bi = byte_idx(line, min_indent);
            out.push_str(&line[..bi]);
            out.push_str("// ");
            out.push_str(&line[bi..]);
            d = 3;
        }
        if i == sp.sl {
            caret_delta = d;
        }
    }
    *text = out;

    if a == b {
        let ls = line_to_char(text, sp.sl) as isize;
        let c = ((a as isize) + caret_delta).max(ls) as usize;
        (c, c)
    } else {
        let start = line_to_char(text, sp.sl);
        let end = if sp.last + 1 < total_lines {
            line_to_char(text, sp.last + 1) - 1
        } else {
            text.chars().count()
        };
        (start, end)
    }
}