//! OCR result post-processing.

/// Join recognized lines with `\r\n` (Windows clipboard convention). Each line's trailing
/// whitespace is stripped when `trim_trailing`; internal runs of spaces collapse to one.
/// Leading/trailing empty lines are dropped.
pub fn join_lines(lines: &[String], trim_trailing: bool) -> String {
    todo!()
}

/// Short single-line preview for toasts: first `max_chars` chars, newlines → ` ⏎ `,
/// with `…` appended if truncated.
pub fn preview(text: &str, max_chars: usize) -> String {
    todo!()
}

/// `(chars, lines)` counts for the toast subtitle (chars excludes line breaks).
pub fn stats(text: &str) -> (usize, usize) {
    todo!()
}
