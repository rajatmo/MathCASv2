//! The method-per-command view of the editor that the box tests of earlier rounds were written against, so those
//! tests run unchanged on the new editor: each method is one [`Command`] through [`Editor::apply`].
#![allow(dead_code)]
use mc_edit::{Command, Context, Editor, Motion, Position, Selection, Status, Structure};
use mc_boxes::{Row, Step};

/// What `Esc` did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AliasOutcome {
    Started,
    Replaced,
    Unknown,
    Cancelled,
}

fn cx() -> Context<'static> {
    Context::default()
}

pub trait Old {
    fn run(&mut self, c: Command) -> (bool, Status);
    fn structure(&mut self, s: Structure) -> bool {
        self.run(Command::Structure(s)).1 == Status::Done
    }
    fn insert_char(&mut self, c: char) {
        self.run(Command::Insert(c.to_string()));
    }
    fn insert_text(&mut self, s: &str) {
        self.run(Command::Insert(s.to_string()));
    }
    fn exit_structure(&mut self) -> bool {
        self.run(Command::ExitStructure).1 == Status::Done
    }
    fn select(&mut self, path: &[Step], r: std::ops::Range<usize>) {
        self.run(Command::Select(Selection::items(path, r)));
    }
    fn set_caret(&mut self, path: &[Step], offset: usize) {
        self.run(Command::SetCaret(Position::new(path, offset)));
    }
    fn extend_to(&mut self, path: &[Step], offset: usize) {
        self.run(Command::ExtendTo(Position::new(path, offset)));
    }
    fn next_placeholder(&mut self) -> bool;
    fn prev_placeholder(&mut self) -> bool;
    fn backspace(&mut self) -> bool;
    fn delete(&mut self) -> bool;
    fn move_left(&mut self, extend: bool) -> bool {
        self.run(Command::Move(Motion::Left, extend)).1 == Status::Done
    }
    fn move_right(&mut self, extend: bool) -> bool {
        self.run(Command::Move(Motion::Right, extend)).1 == Status::Done
    }
    fn move_up(&mut self) -> bool {
        self.run(Command::Move(Motion::Up, false)).1 == Status::Done
    }
    fn move_down(&mut self) -> bool {
        self.run(Command::Move(Motion::Down, false)).1 == Status::Done
    }
    fn move_home(&mut self, extend: bool) {
        self.run(Command::Move(Motion::Home, extend));
    }
    fn move_end(&mut self, extend: bool) {
        self.run(Command::Move(Motion::End, extend));
    }
    fn insert_template(&mut self, t: &str) -> bool {
        self.run(Command::Template(t.to_string())).1 == Status::Done
    }
    fn paste(&mut self, t: &str) -> bool {
        self.run(Command::Paste(t.to_string())).1 == Status::Done
    }
    fn expand_selection(&mut self) -> bool {
        self.run(Command::ExpandSelection).1 == Status::Done
    }
    fn select_all(&mut self) {
        self.run(Command::SelectAll);
    }
    fn escape(&mut self) -> AliasOutcome;
    fn selected_text(&self) -> String;
    fn cut(&mut self) -> String;
}

impl Old for Editor {
    fn run(&mut self, c: Command) -> (bool, Status) {
        let before = (self.root().clone(), self.selection().clone(), self.alias_mark());
        let out = self.apply(&c, &cx());
        let changed = out.tx.is_some() || before != (self.root().clone(), self.selection().clone(), self.alias_mark());
        (changed, out.status)
    }
    fn next_placeholder(&mut self) -> bool {
        self.root().has_placeholder() && self.run(Command::Tab).1 == Status::Done
    }
    fn prev_placeholder(&mut self) -> bool {
        self.root().has_placeholder() && self.run(Command::Outdent).1 == Status::Done
    }
    fn backspace(&mut self) -> bool {
        self.run(Command::Backspace).0
    }
    fn delete(&mut self) -> bool {
        self.run(Command::Delete).0
    }
    fn escape(&mut self) -> AliasOutcome {
        let before = self.alias_mark().filter(|m| !m.unknown);
        let (_, status) = self.run(Command::Alias);
        match (status, before) {
            (Status::Beep, _) => AliasOutcome::Unknown,
            (_, Some(m)) if m.start == m.end => AliasOutcome::Cancelled,
            (_, Some(_)) => AliasOutcome::Replaced,
            _ => AliasOutcome::Started,
        }
    }
    fn selected_text(&self) -> String {
        mc_boxes::to_text(&self.selected())
    }
    fn cut(&mut self) -> String {
        let t = self.selected_text();
        if !t.is_empty() {
            self.run(Command::Delete);
        }
        t
    }
}

/// An editor over a row, the caret at its end.
pub fn editor(r: Row) -> Editor {
    Editor::new(r)
}
