//! Filename templating and `%ENV%` expansion.

/// Local wall-clock time (supplied by the platform layer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DateTime {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millis: u16,
}

/// Render a filename (without extension). Inside each `{...}` group, the tokens
/// `YYYY`, `YY`, `MM`, `DD`, `HH`, `mm`, `ss`, `ms` are replaced by zero-padded values and other
/// characters are kept (so `{YYYY-MM-DD}` → `2026-11-23`). Text outside braces is literal.
/// Unknown/unterminated groups are kept literally. The result is sanitized: Windows-invalid
/// filename characters `<>:"/\|?*` and control chars become `_`, trailing dots/spaces are
/// trimmed, and an empty result becomes `"Kapix"`.
pub fn render_filename(template: &str, now: &DateTime) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // Unterminated group: keep literally.
            out.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let group = &after[..close];
        match expand_tokens(group, now) {
            Some(s) => out.push_str(&s),
            None => {
                out.push('{');
                out.push_str(group);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    let sanitized: String = out
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = sanitized.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "Kapix".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Expand `%NAME%` variables using `lookup` (case-insensitive name). Unknown variables and a
/// lone `%` are kept literally.
pub fn expand_env(s: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(open) = rest.find('%') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('%').and_then(|close| {
            let name = &after[..close];
            if name.is_empty() {
                return None;
            }
            let v = lookup(name)
                .or_else(|| lookup(&name.to_uppercase()))
                .or_else(|| lookup(&name.to_lowercase()))?;
            Some((v, close))
        });
        match value {
            Some((v, close)) => {
                out.push_str(&v);
                rest = &after[close + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// If `name.ext` exists according to `exists`, try `name (2).ext`, `name (3).ext`, ...
pub fn unique_name(name: &str, ext: &str, exists: impl Fn(&str) -> bool) -> String {
    let join = |n: &str| {
        if ext.is_empty() {
            n.to_string()
        } else {
            format!("{n}.{ext}")
        }
    };
    let first = join(name);
    if !exists(&first) {
        return first;
    }
    (2u32..)
        .map(|i| join(&format!("{name} ({i})")))
        .find(|c| !exists(c))
        .expect("unbounded range")
}

/// Replace date/time tokens in one `{...}` group. `None` if the group contains no token
/// (so the caller keeps it literally, braces included).
fn expand_tokens(group: &str, now: &DateTime) -> Option<String> {
    // Longest-first; case-sensitive (`MM` month, `mm` minutes).
    let tokens: [(&str, String); 8] = [
        ("YYYY", format!("{:04}", now.year)),
        ("YY", format!("{:02}", now.year % 100)),
        ("MM", format!("{:02}", now.month)),
        ("DD", format!("{:02}", now.day)),
        ("HH", format!("{:02}", now.hour)),
        ("mm", format!("{:02}", now.minute)),
        ("ss", format!("{:02}", now.second)),
        ("ms", format!("{:03}", now.millis)),
    ];
    let mut out = String::new();
    let mut found = false;
    let mut rest = group;
    'outer: while !rest.is_empty() {
        for (tok, val) in &tokens {
            if let Some(r) = rest.strip_prefix(tok) {
                out.push_str(val);
                rest = r;
                found = true;
                continue 'outer;
            }
        }
        let c = rest.chars().next()?;
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: DateTime = DateTime {
        year: 2026,
        month: 11,
        day: 3,
        hour: 4,
        minute: 5,
        second: 6,
        millis: 7,
    };

    #[test]
    fn renders_pattern() {
        assert_eq!(
            render_filename("Kapix_{YYYY-MM-DD}_{HH-mm-ss}", &NOW),
            "Kapix_2026-11-03_04-05-06"
        );
        assert_eq!(
            render_filename("{YYYY-MM-DD}_{HH-mm-ss}", &NOW),
            "2026-11-03_04-05-06"
        );
        assert_eq!(
            render_filename("{YY}{MM}{DD}-{HH}{mm}{ss}.{ms}", &NOW),
            "261103-040506.007"
        );
        assert_eq!(render_filename("{YYYYMMDD}", &NOW), "20261103");
        assert_eq!(
            render_filename("{ms}", &DateTime { millis: 123, ..NOW }),
            "123"
        );
        assert_eq!(
            render_filename("{YYYY}", &DateTime { year: 99, ..NOW }),
            "0099"
        );
    }

    #[test]
    fn literal_text_and_odd_groups() {
        assert_eq!(render_filename("plain name", &NOW), "plain name");
        assert_eq!(render_filename("YYYY literal", &NOW), "YYYY literal");
        assert_eq!(render_filename("a{}b", &NOW), "a{}b");
        assert_eq!(render_filename("a{foo}b", &NOW), "a{foo}b");
        assert_eq!(render_filename("shot_{YYYY", &NOW), "shot_{YYYY");
        assert_eq!(render_filename("{YYYY}}{", &NOW), "2026}{");
        assert_eq!(
            render_filename("{YYYY}\u{e9}\u{1F600}", &NOW),
            "2026\u{e9}\u{1F600}"
        );
    }

    #[test]
    fn sanitizes() {
        assert_eq!(
            render_filename("a<b>c:d\"e/f\\g|h?i*j", &NOW),
            "a_b_c_d_e_f_g_h_i_j"
        );
        assert_eq!(render_filename("a\tb\nc", &NOW), "a_b_c");
        assert_eq!(render_filename("name. . ", &NOW), "name");
        assert_eq!(render_filename("", &NOW), "Kapix");
        assert_eq!(render_filename("...", &NOW), "Kapix");
        assert_eq!(render_filename("{HH:mm}", &NOW), "04_05");
    }

    fn env(name: &str) -> Option<String> {
        match name.to_ascii_uppercase().as_str() {
            "USERPROFILE" => Some("C:\\Users\\k".into()),
            "X" => Some("1".into()),
            _ => None,
        }
    }

    #[test]
    fn env_expansion() {
        assert_eq!(
            expand_env("%USERPROFILE%\\Pictures", env),
            "C:\\Users\\k\\Pictures"
        );
        assert_eq!(expand_env("%userprofile%\\p", env), "C:\\Users\\k\\p");
        assert_eq!(expand_env("%UserProfile%", env), "C:\\Users\\k");
        assert_eq!(expand_env("%NOPE%\\x", env), "%NOPE%\\x");
        assert_eq!(expand_env("100%", env), "100%");
        assert_eq!(expand_env("%", env), "%");
        assert_eq!(expand_env("%%", env), "%%");
        assert_eq!(expand_env("50% of %X%", env), "50% of 1");
        assert_eq!(expand_env("%X%%X%", env), "11");
        assert_eq!(expand_env("no vars", env), "no vars");
        // lookup that is strictly case-sensitive on the upper-case name still works
        let strict = |n: &str| (n == "HOME").then(|| "/h".to_string());
        assert_eq!(expand_env("%home%", strict), "/h");
    }

    #[test]
    fn unique_names() {
        let none = |_: &str| false;
        assert_eq!(unique_name("shot", "png", none), "shot.png");
        let taken = ["shot.png", "shot (2).png"];
        let exists = |n: &str| taken.contains(&n);
        assert_eq!(unique_name("shot", "png", exists), "shot (3).png");
        assert_eq!(unique_name("other", "png", exists), "other.png");
        assert_eq!(unique_name("a", "", |n| n == "a"), "a (2)");
    }
}
