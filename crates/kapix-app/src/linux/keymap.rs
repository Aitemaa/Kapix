//! Linux evdev keycode translation for `kapix_ui::Overlay` (both shortcut keys and printable
//! text input for the inline text annotation tool).

use kapix_ui::Key;

// Modifier keycodes from <linux/input-event-codes.h>.
pub const KEY_LEFTSHIFT: u32 = 42;
pub const KEY_RIGHTSHIFT: u32 = 54;
pub const KEY_LEFTCTRL: u32 = 29;
pub const KEY_RIGHTCTRL: u32 = 97;
pub const KEY_LEFTALT: u32 = 56;
pub const KEY_RIGHTALT: u32 = 100;
pub const KEY_CAPSLOCK: u32 = 58;

/// Translate a Linux evdev keycode into `kapix_ui::Key` (uppercase letters/digits regardless of
/// Shift, matching Windows `VK_*` semantics).
pub fn evdev_to_key(code: u32) -> Key {
    match code {
        1 => Key::Escape,
        14 => Key::Backspace,
        15 => Key::Tab,
        28 | 96 => Key::Enter,
        111 => Key::Delete,
        103 => Key::Up,
        105 => Key::Left,
        106 => Key::Right,
        108 => Key::Down,
        // Number row (1..=0)
        2 => Key::Char('1'),
        3 => Key::Char('2'),
        4 => Key::Char('3'),
        5 => Key::Char('4'),
        6 => Key::Char('5'),
        7 => Key::Char('6'),
        8 => Key::Char('7'),
        9 => Key::Char('8'),
        10 => Key::Char('9'),
        11 => Key::Char('0'),
        // Letters (US QWERTY physical positions)
        16 => Key::Char('Q'),
        17 => Key::Char('W'),
        18 => Key::Char('E'),
        19 => Key::Char('R'),
        20 => Key::Char('T'),
        21 => Key::Char('Y'),
        22 => Key::Char('U'),
        23 => Key::Char('I'),
        24 => Key::Char('O'),
        25 => Key::Char('P'),
        30 => Key::Char('A'),
        31 => Key::Char('S'),
        32 => Key::Char('D'),
        33 => Key::Char('F'),
        34 => Key::Char('G'),
        35 => Key::Char('H'),
        36 => Key::Char('J'),
        37 => Key::Char('K'),
        38 => Key::Char('L'),
        44 => Key::Char('Z'),
        45 => Key::Char('X'),
        46 => Key::Char('C'),
        47 => Key::Char('V'),
        48 => Key::Char('B'),
        49 => Key::Char('N'),
        50 => Key::Char('M'),
        _ => Key::Other,
    }
}

/// Translate a Linux evdev keycode to a printable US-layout character for the inline Text tool.
pub fn evdev_to_char(code: u32, shift: bool, caps_lock: bool) -> Option<char> {
    let upper = shift ^ caps_lock;
    let letter = |lo: char, hi: char| Some(if upper { hi } else { lo });
    let pair = |lo: char, hi: char| Some(if shift { hi } else { lo });

    match code {
        // Number row
        2 => pair('1', '!'),
        3 => pair('2', '@'),
        4 => pair('3', '#'),
        5 => pair('4', '$'),
        6 => pair('5', '%'),
        7 => pair('6', '^'),
        8 => pair('7', '&'),
        9 => pair('8', '*'),
        10 => pair('9', '('),
        11 => pair('0', ')'),
        12 => pair('-', '_'),
        13 => pair('=', '+'),
        // Top row
        16 => letter('q', 'Q'),
        17 => letter('w', 'W'),
        18 => letter('e', 'E'),
        19 => letter('r', 'R'),
        20 => letter('t', 'T'),
        21 => letter('y', 'Y'),
        22 => letter('u', 'U'),
        23 => letter('i', 'I'),
        24 => letter('o', 'O'),
        25 => letter('p', 'P'),
        26 => pair('[', '{'),
        27 => pair(']', '}'),
        // Home row
        30 => letter('a', 'A'),
        31 => letter('s', 'S'),
        32 => letter('d', 'D'),
        33 => letter('f', 'F'),
        34 => letter('g', 'G'),
        35 => letter('h', 'H'),
        36 => letter('j', 'J'),
        37 => letter('k', 'K'),
        38 => letter('l', 'L'),
        39 => pair(';', ':'),
        40 => pair('\'', '"'),
        41 => pair('`', '~'),
        43 => pair('\\', '|'),
        // Bottom row
        44 => letter('z', 'Z'),
        45 => letter('x', 'X'),
        46 => letter('c', 'C'),
        47 => letter('v', 'V'),
        48 => letter('b', 'B'),
        49 => letter('n', 'N'),
        50 => letter('m', 'M'),
        51 => pair(',', '<'),
        52 => pair('.', '>'),
        53 => pair('/', '?'),
        57 => Some(' '),
        // Numpad
        55 => Some('*'),
        71 => Some('7'),
        72 => Some('8'),
        73 => Some('9'),
        74 => Some('-'),
        75 => Some('4'),
        76 => Some('5'),
        77 => Some('6'),
        78 => Some('+'),
        79 => Some('1'),
        80 => Some('2'),
        81 => Some('3'),
        82 => Some('0'),
        83 => Some('.'),
        98 => Some('/'),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_shortcuts_and_text() {
        assert_eq!(evdev_to_key(1), Key::Escape);
        assert_eq!(evdev_to_key(28), Key::Enter);
        assert_eq!(evdev_to_key(30), Key::Char('A'));
        assert_eq!(evdev_to_key(2), Key::Char('1'));
        assert_eq!(evdev_to_char(30, false, false), Some('a'));
        assert_eq!(evdev_to_char(30, true, false), Some('A'));
        assert_eq!(evdev_to_char(30, false, true), Some('A'));
        assert_eq!(evdev_to_char(30, true, true), Some('a'));
        assert_eq!(evdev_to_char(2, true, false), Some('!'));
        assert_eq!(evdev_to_char(57, false, false), Some(' '));
    }
}
