//! Hotkey strings like `"Ctrl+Shift+S"` ↔ structured form.

/// A parsed global hotkey. `vk` uses **Windows virtual-key code values** (plain numbers, so
/// this stays platform-independent): letters `A`–`Z` = 0x41–0x5A, digits `0`–`9` = 0x30–0x39,
/// `F1`–`F24` = 0x70–0x87, `PrintScreen` = 0x2C, `Space` = 0x20, `Insert` = 0x2D,
/// `Delete` = 0x2E, `Home` = 0x24, `End` = 0x23, `PageUp` = 0x21, `PageDown` = 0x22.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Hotkey {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
    pub vk: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyParseError(pub String);

impl std::fmt::Display for HotkeyParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid hotkey: {}", self.0)
    }
}

impl std::error::Error for HotkeyParseError {}

impl Hotkey {
    /// Case-insensitive, whitespace-tolerant. Modifier aliases: Ctrl/Control, Shift, Alt,
    /// Win/Super/Meta. Key aliases: PrintScreen/PrtSc/PrtScn/Print, Space, F1–F24, A–Z, 0–9,
    /// Insert/Ins, Delete/Del, Home, End, PageUp/PgUp, PageDown/PgDn.
    /// Exactly one non-modifier key is required. `PrintScreen` alone (no modifiers) is allowed.
    pub fn parse(s: &str) -> Result<Hotkey, HotkeyParseError> {
        let mut h = Hotkey {
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
            vk: 0,
        };
        let mut key: Option<u16> = None;
        let err = |m: &str| HotkeyParseError(format!("{m} in {s:?}"));
        if s.trim().is_empty() {
            return Err(err("empty hotkey"));
        }
        for part in s.split('+') {
            let p = part.trim().to_ascii_lowercase();
            if p.is_empty() {
                return Err(err("empty component"));
            }
            match p.as_str() {
                "ctrl" | "control" => h.ctrl = true,
                "shift" => h.shift = true,
                "alt" => h.alt = true,
                "win" | "super" | "meta" => h.win = true,
                _ => {
                    let Some(vk) = key_from_name(&p) else {
                        return Err(err(&format!("unknown key {part:?}")));
                    };
                    if key.replace(vk).is_some() {
                        return Err(err("more than one non-modifier key"));
                    }
                }
            }
        }
        h.vk = key.ok_or_else(|| err("missing key"))?;
        Ok(h)
    }
    /// Canonical display form: modifiers in the fixed order `Ctrl+Shift+Alt+Win`, then the key
    /// (`parse(display(h)) == h` always holds for parsed hotkeys), e.g. `"Ctrl+Shift+S"`, `"PrintScreen"`.
    pub fn display(&self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.ctrl, "Ctrl"),
            (self.shift, "Shift"),
            (self.alt, "Alt"),
            (self.win, "Win"),
        ] {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(&key_name(self.vk));
        out
    }
}

/// Named (non letter/digit/F-key) keys: (canonical name, vk).
const NAMED: [(&str, u16); 8] = [
    ("PrintScreen", 0x2C),
    ("Space", 0x20),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
];

/// `name` must be lowercase.
fn key_from_name(name: &str) -> Option<u16> {
    let alias = match name {
        "prtsc" | "prtscn" | "print" => "printscreen",
        "ins" => "insert",
        "del" => "delete",
        "pgup" => "pageup",
        "pgdn" => "pagedown",
        n => n,
    };
    if let Some((_, vk)) = NAMED.iter().find(|(n, _)| n.eq_ignore_ascii_case(alias)) {
        return Some(*vk);
    }
    let b = alias.as_bytes();
    if b.len() == 1 && b[0].is_ascii_lowercase() {
        return Some(b[0].to_ascii_uppercase() as u16);
    }
    if b.len() == 1 && b[0].is_ascii_digit() {
        return Some(b[0] as u16);
    }
    if let Some(n) = alias.strip_prefix('f').and_then(|n| n.parse::<u16>().ok())
        && (1..=24).contains(&n)
        && !alias[1..].starts_with('0')
        && !alias[1..].starts_with('+')
    {
        return Some(0x70 + n - 1);
    }
    None
}

fn key_name(vk: u16) -> String {
    if let Some((n, _)) = NAMED.iter().find(|(_, v)| *v == vk) {
        return (*n).to_string();
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => (vk as u8 as char).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x70 + 1),
        _ => format!("VK_{vk:#04X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hk(c: bool, s: bool, a: bool, w: bool, vk: u16) -> Hotkey {
        Hotkey {
            ctrl: c,
            shift: s,
            alt: a,
            win: w,
            vk,
        }
    }

    #[test]
    fn parse_variants() {
        assert_eq!(
            Hotkey::parse("Ctrl+Shift+S").unwrap(),
            hk(true, true, false, false, b'S' as u16)
        );
        assert_eq!(
            Hotkey::parse("  ctrl + shift +s ").unwrap(),
            Hotkey::parse("Ctrl+Shift+S").unwrap()
        );
        assert_eq!(
            Hotkey::parse("CONTROL+alt+Del").unwrap(),
            hk(true, false, true, false, 0x2E)
        );
        assert_eq!(
            Hotkey::parse("Super+1").unwrap(),
            hk(false, false, false, true, b'1' as u16)
        );
        assert_eq!(
            Hotkey::parse("meta+F12").unwrap(),
            hk(false, false, false, true, 0x7B)
        );
        assert_eq!(Hotkey::parse("win+f24").unwrap().vk, 0x87);
        assert_eq!(Hotkey::parse("F1").unwrap().vk, 0x70);
        assert_eq!(
            Hotkey::parse("PrintScreen").unwrap(),
            hk(false, false, false, false, 0x2C)
        );
        for alias in ["prtsc", "PrtScn", "Print", "printscreen"] {
            assert_eq!(Hotkey::parse(alias).unwrap().vk, 0x2C, "{alias}");
        }
        for (n, vk) in [
            ("Space", 0x20),
            ("Ins", 0x2D),
            ("Insert", 0x2D),
            ("Home", 0x24),
            ("End", 0x23),
            ("PgUp", 0x21),
            ("PageUp", 0x21),
            ("PgDn", 0x22),
            ("PageDown", 0x22),
        ] {
            assert_eq!(Hotkey::parse(n).unwrap().vk, vk, "{n}");
        }
    }

    #[test]
    fn parse_errors() {
        for bad in [
            "",
            "   ",
            "Ctrl+Shift",
            "Ctrl+A+B",
            "Ctrl+Foo",
            "Ctrl++S",
            "+",
            "F25",
            "F0",
            "F1x",
            "Ctrl+",
        ] {
            assert!(Hotkey::parse(bad).is_err(), "{bad:?} should fail");
        }
        assert!(
            Hotkey::parse("Ctrl+Foo")
                .unwrap_err()
                .to_string()
                .contains("Foo")
        );
    }

    #[test]
    fn display_round_trip() {
        assert_eq!(
            Hotkey::parse("shift+ctrl+s").unwrap().display(),
            "Ctrl+Shift+S"
        );
        assert_eq!(Hotkey::parse("prtsc").unwrap().display(), "PrintScreen");
        assert_eq!(
            Hotkey::parse("win+alt+shift+ctrl+f5").unwrap().display(),
            "Ctrl+Shift+Alt+Win+F5"
        );
        let mut keys: Vec<u16> = (0x30..=0x39)
            .chain(0x41..=0x5A)
            .chain(0x70..=0x87)
            .collect();
        keys.extend([0x2C, 0x20, 0x2D, 0x2E, 0x24, 0x23, 0x21, 0x22]);
        for vk in keys {
            for mods in 0..16u8 {
                let h = hk(
                    mods & 1 != 0,
                    mods & 2 != 0,
                    mods & 4 != 0,
                    mods & 8 != 0,
                    vk,
                );
                assert_eq!(Hotkey::parse(&h.display()).unwrap(), h, "{}", h.display());
            }
        }
    }
}
