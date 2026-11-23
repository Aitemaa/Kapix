//! OCR result post-processing.

/// Join recognized lines with `\r\n` (Windows clipboard convention). Each line's trailing
/// whitespace is stripped when `trim_trailing`; internal runs of spaces collapse to one.
/// Leading/trailing empty lines are dropped.
pub fn join_lines(lines: &[String], trim_trailing: bool) -> String {
    let mut out: Vec<String> = lines
        .iter()
        .map(|l| {
            let body = l.trim_start();
            let lead = &l[..l.len() - body.len()];
            let mut s = String::with_capacity(l.len());
            s.push_str(lead);
            let mut prev_space = false;
            for c in body.chars() {
                if c == ' ' {
                    if !prev_space {
                        s.push(c);
                    }
                    prev_space = true;
                } else {
                    s.push(c);
                    prev_space = false;
                }
            }
            if trim_trailing {
                s.truncate(s.trim_end().len());
            }
            s
        })
        .collect();
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    let first = out
        .iter()
        .position(|l| !l.trim().is_empty())
        .unwrap_or(out.len());
    out.drain(..first);
    out.join("\r\n")
}

/// Short single-line preview for toasts: first `max_chars` chars, newlines → ` ⏎ `,
/// with `…` appended if truncated.
pub fn preview(text: &str, max_chars: usize) -> String {
    let text = text.replace("\r\n", "\n");
    let truncated = text.chars().count() > max_chars;
    let head: String = text.chars().take(max_chars).collect();
    let mut out = head.replace(['\n', '\r'], " \u{23CE} ");
    if truncated {
        out.push('\u{2026}');
    }
    out
}

/// `(chars, lines)` counts for the toast subtitle (chars excludes line breaks).
pub fn stats(text: &str) -> (usize, usize) {
    let chars = text.chars().filter(|c| *c != '\r' && *c != '\n').count();
    (chars, text.lines().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn join_basic() {
        assert_eq!(join_lines(&v(&["a", "b"]), true), "a\r\nb");
        assert_eq!(join_lines(&v(&["a   b  ", "c"]), true), "a b\r\nc");
        assert_eq!(join_lines(&v(&["a  b  "]), false), "a b ");
        assert_eq!(
            join_lines(&v(&["", "  ", "x", "", "y", " ", ""]), true),
            "x\r\n\r\ny"
        );
        assert_eq!(join_lines(&[], true), "");
        assert_eq!(join_lines(&v(&["", " "]), true), "");
    }

    #[test]
    fn preview_truncates() {
        assert_eq!(preview("hello", 10), "hello");
        assert_eq!(preview("hello world", 5), "hello\u{2026}");
        assert_eq!(preview("a\r\nb\nc", 10), "a \u{23CE} b \u{23CE} c");
        assert_eq!(preview("h\u{e9}llo", 2), "h\u{e9}\u{2026}");
        assert_eq!(preview("", 5), "");
    }

    #[test]
    fn stats_counts() {
        assert_eq!(stats(""), (0, 0));
        assert_eq!(stats("abc"), (3, 1));
        assert_eq!(stats("ab\r\ncd\r\nef"), (6, 3));
        assert_eq!(stats("h\u{e9}\nx"), (3, 2));
    }
}
