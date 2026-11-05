//! egui keys to the model's chords (the key table lives in `mc_nbmodel::keys`).
use eframe::egui;
use mc_nbmodel::keys::{Chord, Key, Mods};

/// The chord of an egui key press.
pub fn chord(key: egui::Key, m: egui::Modifiers) -> Option<Chord> {
    use egui::Key as E;
    let mut mods = Mods { ctrl: m.ctrl || m.command, shift: m.shift, alt: m.alt };
    let k = match key {
        E::Enter => Key::Enter,
        E::Tab => Key::Tab,
        E::Backspace => Key::Backspace,
        E::Delete => Key::Delete,
        E::Insert => Key::Insert,
        E::Escape => Key::Escape,
        E::Space => Key::Space,
        E::ArrowLeft => Key::Left,
        E::ArrowRight => Key::Right,
        E::ArrowUp => Key::Up,
        E::ArrowDown => Key::Down,
        E::Home => Key::Home,
        E::End => Key::End,
        E::PageUp => Key::PageUp,
        E::PageDown => Key::PageDown,
        E::F1 => Key::F(1),
        E::F2 => Key::F(2),
        E::F3 => Key::F(3),
        E::F4 => Key::F(4),
        E::F5 => Key::F(5),
        E::F6 => Key::F(6),
        E::F7 => Key::F(7),
        E::F8 => Key::F(8),
        E::F9 => Key::F(9),
        E::F10 => Key::F(10),
        E::F11 => Key::F(11),
        E::F12 => Key::F(12),
        E::Colon => shifted(&mut mods, ';'),
        E::Comma => Key::Char(','),
        E::Backslash => Key::Char('\\'),
        E::Slash => Key::Char('/'),
        E::Pipe => shifted(&mut mods, '\\'),
        E::Questionmark => shifted(&mut mods, '/'),
        E::Exclamationmark => shifted(&mut mods, '1'),
        E::OpenBracket => Key::Char('['),
        E::CloseBracket => Key::Char(']'),
        E::OpenCurlyBracket => shifted(&mut mods, '['),
        E::CloseCurlyBracket => shifted(&mut mods, ']'),
        E::Backtick => Key::Char('`'),
        E::Minus => Key::Char('-'),
        E::Period => Key::Char('.'),
        E::Plus => shifted(&mut mods, '='),
        E::Equals => Key::Char('='),
        E::Semicolon => Key::Char(';'),
        E::Quote => Key::Char('\''),
        k => {
            let name = k.name();
            let mut cs = name.chars();
            match (cs.next(), cs.next()) {
                (Some(c), None) if c.is_ascii_alphanumeric() => Key::Char(c.to_ascii_lowercase()),
                _ => return None,
            }
        }
    };
    Some(Chord { mods, key: k })
}

fn shifted(mods: &mut Mods, base: char) -> Key {
    mods.shift = true;
    Key::Char(base)
}
