//! The key table (design 3.1): one list of (chord, context, command) used by the window, the key scripts and the
//! generated keyboard help. `Alt` is the command modifier for styles and evaluation control.
use crate::cell::OutputForm;
use crate::command::{AppAction, Command, Motion};
use mc_boxes::Structure;
use crate::style::Style;
use std::fmt;

/// A key, without modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// a character key, by the character it types without Shift (`'a'`, `'1'`, `'/'`, `'['`, `'.'`)
    Char(char),
    /// Return
    Enter,
    /// the keypad's Enter
    KeypadEnter,
    /// the keypad's `.`
    KeypadPeriod,
    /// Tab
    Tab,
    /// Backspace
    Backspace,
    /// Delete
    Delete,
    /// Insert
    Insert,
    /// Escape
    Escape,
    /// Space
    Space,
    /// arrow left
    Left,
    /// arrow right
    Right,
    /// arrow up
    Up,
    /// arrow down
    Down,
    /// Home
    Home,
    /// End
    End,
    /// Page Up
    PageUp,
    /// Page Down
    PageDown,
    /// a function key F1 ... F12
    F(u8),
}

/// Modifier keys held with a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Mods {
    /// Ctrl
    pub ctrl: bool,
    /// Shift
    pub shift: bool,
    /// Alt
    pub alt: bool,
}

/// A key with modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    /// the modifiers
    pub mods: Mods,
    /// the key
    pub key: Key,
}

/// Where the selection is, which decides the bindings that apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    /// a caret or text selection in a cell
    Text,
    /// whole cells selected by their brackets
    Cells,
    /// an insertion point between cells
    Gap,
    /// any of these
    Any,
}

/// The character typed with Shift on a US layout, for chords written with the shifted character (`Ctrl+{`).
fn unshift(c: char) -> Option<char> {
    let pairs = [
        ('{', '['),
        ('}', ']'),
        ('^', '6'),
        ('_', '-'),
        ('@', '2'),
        ('&', '7'),
        ('%', '5'),
        ('$', '4'),
        ('<', ','),
        ('>', '.'),
        ('?', '/'),
        ('"', '\''),
        ('+', '='),
        ('(', '9'),
        (')', '0'),
        ('*', '8'),
        ('!', '1'),
        ('#', '3'),
        (':', ';'),
        ('|', '\\'),
        ('~', '`'),
    ];
    pairs.iter().find(|p| p.0 == c).map(|p| p.1)
}

impl Chord {
    /// Read a chord written as `Ctrl+Shift+D`, `Alt+1`, `Shift+Enter`, `Ctrl+{` (a shifted character adds Shift),
    /// `Keypad Enter`, `F3`.
    pub fn parse(text: &str) -> Result<Chord, String> {
        let mut mods = Mods::default();
        let mut rest = text;
        loop {
            if let Some(r) = rest.strip_prefix("Ctrl+") {
                mods.ctrl = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Shift+") {
                mods.shift = true;
                rest = r;
            } else if let Some(r) = rest.strip_prefix("Alt+") {
                mods.alt = true;
                rest = r;
            } else {
                break;
            }
        }
        let key = match rest {
            "Enter" | "Return" => Key::Enter,
            "Keypad Enter" | "KeypadEnter" => Key::KeypadEnter,
            "Keypad ." | "KeypadPeriod" => Key::KeypadPeriod,
            "Tab" => Key::Tab,
            "Backspace" => Key::Backspace,
            "Delete" | "Del" => Key::Delete,
            "Insert" | "Ins" => Key::Insert,
            "Esc" | "Escape" => Key::Escape,
            "Space" => Key::Space,
            "Left" => Key::Left,
            "Right" => Key::Right,
            "Up" => Key::Up,
            "Down" => Key::Down,
            "Home" => Key::Home,
            "End" => Key::End,
            "PageUp" => Key::PageUp,
            "PageDown" => Key::PageDown,
            f if f.len() >= 2 && f.starts_with('F') && f[1..].parse::<u8>().is_ok() => {
                Key::F(f[1..].parse().expect("checked"))
            }
            c if c.chars().count() == 1 => {
                let c = c.chars().next().expect("one char");
                match unshift(c) {
                    Some(base) => {
                        mods.shift = true;
                        Key::Char(base)
                    }
                    None => Key::Char(c.to_ascii_lowercase()),
                }
            }
            _ => return Err(format!("unknown key `{text}`")),
        };
        Ok(Chord { mods, key })
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.mods.ctrl {
            f.write_str("Ctrl+")?;
        }
        if self.mods.alt {
            f.write_str("Alt+")?;
        }
        if self.mods.shift {
            f.write_str("Shift+")?;
        }
        match self.key {
            Key::Char(c) => write!(f, "{}", c.to_ascii_uppercase()),
            Key::F(n) => write!(f, "F{n}"),
            Key::KeypadEnter => f.write_str("Keypad Enter"),
            Key::KeypadPeriod => f.write_str("Keypad ."),
            k => write!(f, "{k:?}"),
        }
    }
}

/// One binding of the key table.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    /// the chord as written in the design (`Ctrl+{`)
    pub keys: &'static str,
    /// the parsed chord
    pub chord: Chord,
    /// where it applies
    pub context: Context,
    /// what it does
    pub command: Command,
}

/// The key table, in the order of design 3.1 (the keyboard help lists it in this order).
pub fn table() -> Vec<Binding> {
    use Command as C;
    use Context::*;
    let mut t: Vec<(&'static str, Context, Command)> = vec![
        // evaluation
        ("Shift+Enter", Any, C::Evaluate),
        ("Keypad Enter", Any, C::Evaluate),
        ("Shift+Keypad Enter", Any, C::EvaluateNext),
        ("Alt+.", Any, C::Abort),
        ("Alt+Shift+.", Any, C::Unqueue),
        ("Alt+,", Any, C::App(AppAction::InterruptDialog)),
    ];
    // movement and selection: Shift extends
    let motions = [
        ("Left", Motion::Left),
        ("Right", Motion::Right),
        ("Ctrl+Left", Motion::WordLeft),
        ("Ctrl+Right", Motion::WordRight),
        ("Up", Motion::Up),
        ("Down", Motion::Down),
        ("Home", Motion::Home),
        ("End", Motion::End),
        ("Ctrl+Home", Motion::DocStart),
        ("Ctrl+End", Motion::DocEnd),
    ];
    const SHIFTED: [&str; 10] = [
        "Shift+Left",
        "Shift+Right",
        "Ctrl+Shift+Left",
        "Ctrl+Shift+Right",
        "Shift+Up",
        "Shift+Down",
        "Shift+Home",
        "Shift+End",
        "Ctrl+Shift+Home",
        "Ctrl+Shift+End",
    ];
    for (i, (k, m)) in motions.into_iter().enumerate() {
        t.push((k, Any, C::Move(m, false)));
        t.push((SHIFTED[i], Any, C::Move(m, true)));
    }
    t.extend([
        ("Ctrl+Up", Any, C::App(AppAction::ScrollLines(-1))),
        ("Ctrl+Down", Any, C::App(AppAction::ScrollLines(1))),
        ("PageUp", Any, C::App(AppAction::ScrollPages(-1))),
        ("PageDown", Any, C::App(AppAction::ScrollPages(1))),
        ("Ctrl+A", Any, C::SelectAll),
        // editing
        ("Enter", Any, C::Enter),
        ("Alt+Enter", Any, C::SameStyleBelow),
        ("Tab", Text, C::Tab),
        ("Shift+Tab", Text, C::Outdent),
        ("Alt+/", Text, C::ToggleComment),
        ("Esc", Text, C::Alias),
        ("Esc", Gap, C::Alias),
        ("Ctrl+.", Any, C::ExtendSelection),
        ("Ctrl+Keypad .", Any, C::ExtendSelection),
        ("Ctrl+Shift+B", Text, C::CheckBalance),
        ("Backspace", Any, C::Backspace),
        ("Delete", Any, C::Delete),
        ("Ctrl+Backspace", Text, C::DeleteWordBack),
        ("Ctrl+Delete", Text, C::DeleteWordForward),
        ("Ctrl+K", Text, C::App(AppAction::Complete)),
        ("F2", Text, C::App(AppAction::Complete)),
        ("Ctrl+Z", Any, C::Undo),
        ("Ctrl+Y", Any, C::Redo),
        ("Ctrl+Shift+Z", Any, C::Redo),
        ("Ctrl+X", Any, C::Cut),
        ("Shift+Delete", Any, C::Cut),
        ("Ctrl+C", Any, C::Copy),
        ("Ctrl+Insert", Any, C::Copy),
        ("Ctrl+V", Any, C::Paste),
        ("Shift+Insert", Any, C::Paste),
        ("Ctrl+Shift+C", Any, C::CopyPlain),
        ("Ctrl+L", Any, C::InputFromAbove),
        ("Ctrl+Shift+L", Any, C::OutputFromAbove),
    ]);
    // cells, styles and grouping
    for d in 1..=9u8 {
        let keys: &'static str =
            ["Alt+1", "Alt+2", "Alt+3", "Alt+4", "Alt+5", "Alt+6", "Alt+7", "Alt+8", "Alt+9"][usize::from(d - 1)];
        t.push((keys, Any, C::SetStyle(Style::of_key(d).expect("Alt+1..9 have styles"))));
    }
    t.extend([
        ("Alt+0", Any, C::App(AppAction::OtherStyle)),
        ("Ctrl+Shift+D", Text, C::Divide),
        ("Ctrl+Shift+M", Any, C::Merge),
        ("Ctrl+Shift+G", Any, C::GroupTogether),
        ("Ctrl+Shift+U", Any, C::Ungroup),
        ("Ctrl+'", Any, C::ToggleGroup),
        ("Ctrl+{", Any, C::OpenAll),
        ("Ctrl+}", Any, C::CloseAll),
        ("Ctrl+8", Any, C::ToggleInitialization),
        ("Ctrl+Shift+E", Any, C::ShowExpression),
        // two-dimensional input (design 4.2, NB5.3); with a caret in a cell Ctrl+2, Ctrl+5, Ctrl+6 and Ctrl+7 are
        // the reference's typesetting keys, elsewhere they switch tabs
        ("Ctrl+/", Text, C::Structure(Structure::Fraction)),
        ("Ctrl+6", Text, C::Structure(Structure::Superscript)),
        ("Ctrl+^", Text, C::Structure(Structure::Superscript)),
        ("Ctrl+-", Text, C::Structure(Structure::Subscript)),
        ("Ctrl+_", Text, C::Structure(Structure::Subscript)),
        ("Ctrl+2", Text, C::Structure(Structure::Radical)),
        ("Ctrl+@", Text, C::Structure(Structure::Radical)),
        ("Ctrl+7", Text, C::Structure(Structure::Overscript)),
        ("Ctrl+&", Text, C::Structure(Structure::Overscript)),
        ("Ctrl+Shift+,", Text, C::Structure(Structure::Underscript)),
        ("Ctrl+Enter", Text, C::Structure(Structure::GridRow)),
        ("Ctrl+,", Text, C::Structure(Structure::GridColumn)),
        ("Ctrl+5", Text, C::SwitchScript),
        ("Ctrl+%", Text, C::SwitchScript),
        ("Ctrl+Space", Text, C::ExitStructure),
        ("Ctrl+Shift+N", Any, C::ToTypeset),
        ("Ctrl+Shift+I", Any, C::ToLinear),
        ("Ctrl+Shift+Alt+N", Any, C::OutputForm(OutputForm::Traditional)),
        // the window
        ("Ctrl+N", Any, C::App(AppAction::New)),
        ("Ctrl+O", Any, C::App(AppAction::Open)),
        ("Ctrl+S", Any, C::App(AppAction::Save)),
        ("Ctrl+Shift+S", Any, C::App(AppAction::SaveAs)),
        ("Ctrl+W", Any, C::App(AppAction::Close)),
        // tabs (the window's; design guess of the polish package: Ctrl+8 stays the initialization key of the
        // reference, so tab 8 has no number key, and the typesetting keys Ctrl+2/5/6/7 keep their shifted
        // spellings Ctrl+@ % ^ &)
        ("Ctrl+Tab", Any, C::App(AppAction::NextTab)),
        ("Ctrl+PageDown", Any, C::App(AppAction::NextTab)),
        ("Ctrl+Shift+Tab", Any, C::App(AppAction::PrevTab)),
        ("Ctrl+PageUp", Any, C::App(AppAction::PrevTab)),
        ("Ctrl+Shift+T", Any, C::App(AppAction::ReopenTab)),
        ("Ctrl+1", Any, C::App(AppAction::NthTab(1))),
        ("Ctrl+2", Any, C::App(AppAction::NthTab(2))),
        ("Ctrl+3", Any, C::App(AppAction::NthTab(3))),
        ("Ctrl+4", Any, C::App(AppAction::NthTab(4))),
        ("Ctrl+5", Any, C::App(AppAction::NthTab(5))),
        ("Ctrl+6", Any, C::App(AppAction::NthTab(6))),
        ("Ctrl+7", Any, C::App(AppAction::NthTab(7))),
        ("Ctrl+9", Any, C::App(AppAction::NthTab(9))),
        ("Ctrl+F", Any, C::App(AppAction::Find)),
        ("Ctrl+E", Any, C::App(AppAction::Find)),
        ("F3", Any, C::App(AppAction::FindNext(true))),
        ("Shift+F3", Any, C::App(AppAction::FindNext(false))),
        ("F1", Any, C::App(AppAction::Help)),
        ("F12", Any, C::App(AppAction::FullScreen)),
        ("Ctrl+Alt+=", Any, C::App(AppAction::Zoom(1))),
        ("Ctrl+Alt+-", Any, C::App(AppAction::Zoom(-1))),
    ]);
    // reserved by the reference: they do nothing rather than something else
    for k in [
        "Ctrl+0",
        "Ctrl+$",
        "Ctrl+J",
        "Alt+Left",
        "Alt+Right",
        "F7",
    ] {
        t.push((k, Any, C::Nop));
    }
    t.into_iter()
        .map(|(keys, context, command)| Binding {
            keys,
            chord: Chord::parse(keys).unwrap_or_else(|e| panic!("key table: {e}")),
            context,
            command,
        })
        .collect()
}

/// The bindings, ready for lookups.
pub struct KeyTable {
    bindings: Vec<Binding>,
}

impl Default for KeyTable {
    fn default() -> Self {
        KeyTable { bindings: table() }
    }
}

impl KeyTable {
    /// The command bound to `chord` in `context` (a binding for the context itself wins over one for any context).
    pub fn lookup(&self, chord: Chord, context: Context) -> Option<&Command> {
        let exact = self.bindings.iter().find(|b| b.chord == chord && b.context == context);
        exact
            .or_else(|| self.bindings.iter().find(|b| b.chord == chord && b.context == Context::Any))
            .map(|b| &b.command)
    }

    /// Every binding, in table order.
    pub fn bindings(&self) -> &[Binding] {
        &self.bindings
    }

    /// The first chord bound to `command` (for menus).
    pub fn chord_of(&self, command: &Command) -> Option<&'static str> {
        self.bindings.iter().find(|b| &b.command == command).map(|b| b.keys)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chords() {
        let c = Chord::parse("Ctrl+{").unwrap();
        assert_eq!(c, Chord { mods: Mods { ctrl: true, shift: true, alt: false }, key: Key::Char('[') });
        assert_eq!(Chord::parse("Ctrl+Shift+D").unwrap().to_string(), "Ctrl+Shift+D");
        assert_eq!(Chord::parse("Keypad Enter").unwrap().key, Key::KeypadEnter);
        assert_eq!(Chord::parse("F12").unwrap().key, Key::F(12));
        assert!(Chord::parse("Ctrl+Nope").is_err());
    }

    #[test]
    fn no_chord_is_bound_twice_in_one_context() {
        let t = table();
        for (i, a) in t.iter().enumerate() {
            for b in &t[i + 1..] {
                assert!(!(a.chord == b.chord && a.context == b.context), "{} bound twice", a.keys);
            }
        }
        let k = KeyTable::default();
        let alt4 = Chord::parse("Alt+4").unwrap();
        assert_eq!(k.lookup(alt4, Context::Gap), Some(&Command::SetStyle(Style::Section)));
        assert_eq!(k.lookup(Chord::parse("Tab").unwrap(), Context::Cells), None);
        assert_eq!(k.chord_of(&Command::Merge), Some("Ctrl+Shift+M"));
    }
}
