use super::lexer::{tokenize, Tok};

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub line: usize, // 0-based
    pub col: usize,  // 0-based
    pub message: String,
}

/// چک ساده‌ی تعادل براکت‌ها (کامنت و رشته نادیده گرفته می‌شوند)
pub fn check(src: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut stack: Vec<(char, usize, usize)> = Vec::new();
    let (mut line, mut col) = (0usize, 0usize);

    for tok in tokenize(src) {
        let skip = matches!(tok.kind, Tok::Comment | Tok::Str);
        for ch in src[tok.start..tok.end].chars() {
            if !skip {
                match ch {
                    '(' | '[' | '{' => stack.push((ch, line, col)),
                    ')' | ']' | '}' => {
                        let expected = match ch {
                            ')' => '(',
                            ']' => '[',
                            _ => '{',
                        };
                        let top = stack.last().copied();
                        match top {
                            Some((open, ..)) if open == expected => {
                                stack.pop();
                            }
                            Some((open, l, c)) => {
                                out.push(Diagnostic {
                                    line,
                                    col,
                                    message: format!(
                                        "Mismatched '{ch}' (opened '{open}' at {}:{})",
                                        l + 1,
                                        c + 1
                                    ),
                                });
                                stack.pop();
                            }
                            None => out.push(Diagnostic {
                                line,
                                col,
                                message: format!("Unexpected '{ch}'"),
                            }),
                        }
                    }
                    _ => {}
                }
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
    }

    for (open, l, c) in stack {
        out.push(Diagnostic { line: l, col: c, message: format!("Unclosed '{open}'") });
    }
    out
}