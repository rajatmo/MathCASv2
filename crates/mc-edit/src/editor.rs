//! The editor (design 11, section 3.2): a tree, a selection and the marks that belong to positions (automatic
//! closers, a pending alias), changed only by [`Editor::apply`]. Every command is data ([`Command`]) and is written
//! once, over the tree; text behaviour is the behaviour of a row of characters, so a cell without boxes and every
//! slot of a box behave the same. Each command that changes the tree returns one [`Transaction`].
use crate::geom::{Geometry, Logical};
use crate::pos::{Position, Selection};
use crate::rules::{self, Lexical, INDENT};
use crate::tx::{Kind, Splice, Transaction};
use mc_boxes::aliases::{self, Alias};
use mc_boxes::ops::{closer_of, matching_open, primary_before};
use mc_boxes::{from_text, linear_text, to_text, to_text_mapped, write_notation, Item, Node, Row, RowPath, Step, MAX_NESTING};
use std::ops::Range;

/// The box keys (decision NBUI.5: a cell has exactly these three boxes; round 19 gave the root an index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Structure {
    /// `Ctrl+/`: a fraction of the term before the caret (or the selection) over a new denominator
    Fraction,
    /// `Ctrl+6` (`Ctrl+^`): a power of the term before the caret
    Superscript,
    /// `Ctrl+2`: a square root of the selection (or an empty one)
    Radical,
    /// `Ctrl+5` (`Ctrl+%`): in a root, to its index (adding one: an n-th root) and back to the radicand; elsewhere
    /// an n-th root of the selection (or an empty one) with the caret in its index
    Root,
}

/// A caret motion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Motion {
    /// previous item; out of a slot to the previous one or before the box
    Left,
    /// next item; into boxes in reading order
    Right,
    /// previous word of the row
    WordLeft,
    /// next word of the row
    WordRight,
    /// inside a structure the slot above, else the visual line above
    Up,
    /// inside a structure the slot below, else the visual line below
    Down,
    /// start of the slot's line, again: start of the visual line
    Home,
    /// end of the slot's line, again: end of the visual line
    End,
}

/// An editing command (design 11, section 3.2): key bindings, menus, the self-test and tests all drive these.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// typed text: with automatic closing, an opener brings its closer, wraps a selection, and a typed closer steps
    /// over one inserted automatically
    Type(String),
    /// text inserted as it is (pasted text, a completion), replacing the selection
    Insert(String),
    /// Return: a line break keeping the line's indentation (one level more after an opening bracket)
    Newline,
    /// delete the selection or the item before the caret
    Backspace,
    /// delete the selection or the item after the caret
    Delete,
    /// delete the word before the caret
    DeleteWordBack,
    /// delete the word after the caret
    DeleteWordForward,
    /// move the caret; `true` extends the selection
    Move(Motion, bool),
    /// a box key
    Structure(Structure),
    /// `Ctrl+Space`: the caret just after the innermost enclosing box
    ExitStructure,
    /// next placeholder; without placeholders indent
    Tab,
    /// previous placeholder; without placeholders outdent the selected lines
    Outdent,
    /// `Esc`: start or finish an alias
    Alias,
    /// canonical box text pasted at the caret (boxes become boxes)
    Paste(String),
    /// a template: canonical box text where `▮` marks the placeholder that gets the caret
    Template(String),
    /// replace the name before the caret (a completion accepted)
    ReplaceWord(String),
    /// select the whole tree
    SelectAll,
    /// `Ctrl+.`: the next larger syntactic unit
    ExpandSelection,
    /// `Ctrl+Shift+B`: the smallest bracketed expression around the selection
    CheckBalance,
    /// `Alt+/`: comment or uncomment the selected lines
    ToggleComment,
    /// a click: the caret there
    SetCaret(Position),
    /// a Shift+click or a drag: the head there, the anchor kept
    ExtendTo(Position),
    /// a selection made elsewhere (Find, a triple-click)
    Select(Selection),
    /// a double-click: the word at the position
    SelectWord(Position),
}

/// What a command may consult: the layout and the window's setting for automatic closing.
#[derive(Clone, Copy, Default)]
pub struct Context<'a> {
    /// the layout of the tree (Up, Down, Home and End on wrapped lines); without one, the root row's line breaks
    pub geometry: Option<&'a dyn Geometry>,
    /// typing an opening bracket or a quote inserts its closer too
    pub auto_close: bool,
}

/// How a command ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// done (it may have changed nothing)
    Done,
    /// refused: nothing to act on
    Beep,
    /// a motion went past the start of the content
    Before,
    /// a motion went past the end of the content
    After,
    /// not handled by the editor (Ctrl+A on everything selected, Ctrl+. past the whole tree): the notebook acts
    Unhandled,
}

/// The result of [`Editor::apply`].
#[derive(Clone, Debug, PartialEq)]
pub struct Outcome {
    /// the change, if the tree changed
    pub tx: Option<Transaction>,
    /// how it ended
    pub status: Status,
}

/// A pending or unknown alias, for drawing: the start marker goes at `start`, the name is `start..end` of the row at
/// `path`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AliasMark {
    /// the row
    pub path: RowPath,
    /// where the marker is drawn
    pub start: usize,
    /// end of the name typed so far
    pub end: usize,
    /// the name was looked up and not found (error colour)
    pub unknown: bool,
}

/// A box tree being edited.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Editor {
    root: Row,
    sel: Selection,
    /// closers inserted automatically and not typed over yet: the position of the closer item
    closers: Vec<Position>,
    /// a pending alias: where its name starts
    alias: Option<Position>,
    /// the last alias looked up without success
    unknown: Option<AliasMark>,
    /// the x Up and Down keep
    goal_x: Option<f32>,
    /// the splices of the command being run
    pending: Vec<Splice>,
    /// the command being run changed the structure (a box went): its own undo step, not merged into deleting
    structural: bool,
}

impl Editor {
    /// An editor over `root`, the caret at its end.
    pub fn new(root: Row) -> Editor {
        let n = root.len();
        Editor { root, sel: Selection::caret(Position::root(n)), ..Default::default() }
    }

    /// An editor over canonical box text.
    pub fn from_text(text: &str) -> Editor {
        Editor::new(from_text(text))
    }

    /// The tree.
    pub fn root(&self) -> &Row {
        &self.root
    }

    /// The selection (the caret is its head).
    pub fn selection(&self) -> &Selection {
        &self.sel
    }

    /// The caret.
    pub fn caret(&self) -> &Position {
        &self.sel.head
    }

    /// The selected row and items; `None` when nothing is selected.
    pub fn range(&self) -> Option<(RowPath, Range<usize>)> {
        self.sel.range(&self.root)
    }

    /// The selected items as a row of their own (empty without a selection).
    pub fn selected(&self) -> Row {
        match self.range() {
            Some((p, r)) => self.root.at(&p).map_or_else(Row::new, |row| Row(row.0[r].to_vec())),
            None => Row::new(),
        }
    }

    /// Sets the selection (made valid), keeping the marks.
    pub fn set_selection(&mut self, sel: Selection) {
        self.sel = sel.clamped(&self.root);
    }

    /// Replaces the tree and the selection; the marks go (undo and redo of the notebook, a text set from outside).
    pub fn reset(&mut self, root: Row, sel: Selection) {
        self.root = root;
        self.sel = sel.clamped(&self.root);
        self.closers.clear();
        self.alias = None;
        self.unknown = None;
        self.goal_x = None;
    }

    /// Undoes `tx` (the last change of the tree), restoring its selection.
    pub fn revert(&mut self, tx: &Transaction) {
        tx.revert(&mut self.root);
        let root = std::mem::take(&mut self.root);
        self.reset(root, tx.before.clone());
    }

    /// Redoes `tx`, restoring the selection after it.
    pub fn redo(&mut self, tx: &Transaction) {
        tx.apply(&mut self.root);
        let root = std::mem::take(&mut self.root);
        self.reset(root, tx.after.clone());
    }

    /// The canonical text of the tree (design 08, section 4.4).
    pub fn text(&self) -> String {
        to_text(&self.root)
    }

    /// The linear input text of the tree (design 08, section 4.6).
    pub fn linear(&self) -> String {
        linear_text(&self.root)
    }

    /// The closers inserted automatically, by the position of the closer.
    pub fn closers(&self) -> &[Position] {
        &self.closers
    }

    /// The notation of the tree with the caret as `▮` or the selection in `⟦ ⟧` (the key scripts' notation).
    pub fn notation(&self) -> String {
        let sel = self.range();
        let caret = &self.sel.head;
        let mut s = String::new();
        write_notation(&self.root, &mut s, &mut |out, pos| {
            let row = sel.as_ref().map_or(caret.path.as_slice(), |(p, _)| p.as_slice());
            if pos.path != row {
                return;
            }
            match &sel {
                Some((_, r)) => {
                    if pos.offset == r.end && r.end > r.start {
                        out.push('⟧');
                    }
                    if pos.offset == r.start {
                        out.push('⟦');
                    }
                }
                None if pos.offset == caret.offset => out.push('▮'),
                None => {}
            }
        });
        s
    }

    /// The pending alias (while the caret stays in its row after its start, on characters only) or the last unknown
    /// one, for drawing the marker and colouring the name.
    pub fn alias_mark(&self) -> Option<AliasMark> {
        if let Some(a) = &self.alias {
            let row = self.root.at(&a.path)?;
            let c = &self.sel.head;
            let ok = a.path == c.path
                && c.offset >= a.offset
                && c.offset <= row.len()
                && row.0[a.offset..c.offset].iter().all(|i| matches!(i, Item::Char(ch) if !ch.is_whitespace() || *ch == ' '));
            if ok {
                return Some(AliasMark { path: a.path.clone(), start: a.offset, end: c.offset, unknown: false });
            }
        }
        self.unknown.clone()
    }

    /// The name before the caret (letters, digits, `_` and `.`): its row, where it starts, and its text. `None` with
    /// a selection.
    pub fn word_before_caret(&self) -> Option<(RowPath, usize, String)> {
        if self.range().is_some() {
            return None;
        }
        let c = &self.sel.head;
        let row = self.root.at(&c.path)?;
        let start = name_start(row, c.offset);
        let word = row.0[start..c.offset.min(row.len())].iter().filter_map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect();
        Some((c.path.clone(), start, word))
    }

    // ---- applying commands ----

    /// Runs a command: a pure function of the tree, the selection, the marks and the command. The tree changes only
    /// here, and every change is in the returned transaction.
    pub fn apply(&mut self, cmd: &Command, cx: &Context<'_>) -> Outcome {
        use Command as C;
        let before = self.sel.clone();
        self.pending.clear();
        self.structural = false;
        if !matches!(cmd, C::Move(Motion::Up | Motion::Down, _)) {
            self.goal_x = None;
        }
        // a caret motion keeps the automatic closers the caret stays inside of (round 38: Down into a fraction's
        // denominator inside `(` forgot its `)`, and the `)` typed after the fraction made a second one)
        let inside = matches!(cmd, C::Move(_, false));
        if !inside
            && matches!(
                cmd,
                C::Move(..)
                    | C::SetCaret(_)
                    | C::ExtendTo(_)
                    | C::Select(_)
                    | C::SelectWord(_)
                    | C::SelectAll
                    | C::ExpandSelection
                    | C::CheckBalance
                    | C::ToggleComment
            )
        {
            self.closers.clear();
        }
        let status = self.run(cmd, cx);
        if inside {
            self.keep_closers_around_caret();
        }
        let kind = match cmd {
            C::Type(t) | C::Insert(t) => Kind::Typing { space: t.ends_with(char::is_whitespace) },
            C::Newline => Kind::Typing { space: true },
            C::Backspace | C::Delete | C::DeleteWordBack | C::DeleteWordForward if !self.structural => Kind::Deleting,
            _ => Kind::Other,
        };
        let splices = std::mem::take(&mut self.pending);
        let tx = (!splices.is_empty()).then(|| Transaction { splices, before, after: self.sel.clone(), kind });
        Outcome { tx, status }
    }

    fn run(&mut self, cmd: &Command, cx: &Context<'_>) -> Status {
        use Command as C;
        match cmd {
            C::Type(t) => {
                if t.is_empty() {
                    return Status::Done;
                }
                if !(cx.auto_close && self.auto_close(t)) {
                    self.insert_items(Row::text(t).0);
                }
                Status::Done
            }
            C::Insert(t) => {
                self.insert_items(Row::text(t).0);
                Status::Done
            }
            C::Newline => {
                self.newline();
                Status::Done
            }
            C::Backspace => self.backspace(false),
            C::DeleteWordBack => self.backspace(true),
            C::Delete => self.delete(false),
            C::DeleteWordForward => self.delete(true),
            C::Move(m, extend) => self.motion(*m, *extend, cx),
            C::Structure(s) => {
                if self.structure(*s) {
                    Status::Done
                } else {
                    Status::Beep
                }
            }
            C::ExitStructure => {
                self.collapse();
                match self.sel.head.path.pop() {
                    Some(last) => {
                        self.sel = Selection::caret(Position::new(&self.sel.head.path, last.item + 1));
                        Status::Done
                    }
                    None => Status::Beep,
                }
            }
            C::Tab => {
                if self.root.has_placeholder() {
                    return if self.next_placeholder() { Status::Done } else { Status::Beep };
                }
                self.indent();
                Status::Done
            }
            C::Outdent => {
                if self.root.has_placeholder() {
                    return if self.prev_placeholder() { Status::Done } else { Status::Beep };
                }
                self.outdent();
                Status::Done
            }
            C::Alias => self.escape(),
            C::Paste(canonical) => {
                let row = from_text(canonical);
                if self.depth() + row.nesting() > MAX_NESTING {
                    return Status::Beep;
                }
                self.insert_items(row.0);
                Status::Done
            }
            C::Template(t) => {
                if self.insert_template(t) {
                    Status::Done
                } else {
                    Status::Beep
                }
            }
            C::ReplaceWord(name) => {
                if self.range().is_none() {
                    let c = self.sel.head.clone();
                    let start = name_start(self.row(&c.path), c.offset);
                    if start < c.offset {
                        self.sel = Selection::items(&c.path, start..c.offset);
                    }
                }
                self.insert_items(Row::text(name).0);
                Status::Done
            }
            C::SelectAll => {
                let n = self.root.len();
                if n == 0 || self.range() == Some((vec![], 0..n)) {
                    return Status::Unhandled;
                }
                self.sel = Selection::items(&[], 0..n);
                Status::Done
            }
            C::ExpandSelection => self.expand_selection(),
            C::CheckBalance => self.check_balance(),
            C::ToggleComment => {
                self.toggle_comment();
                Status::Done
            }
            C::SetCaret(p) => {
                self.sel = Selection::caret(p.clamped(&self.root));
                Status::Done
            }
            C::ExtendTo(p) => {
                self.sel.head = p.clamped(&self.root);
                Status::Done
            }
            C::Select(s) => {
                self.sel = s.clamped(&self.root);
                Status::Done
            }
            C::SelectWord(p) => {
                let p = p.clamped(&self.root);
                let (a, b) = rules::word_at(self.row(&p.path), p.offset);
                self.sel = Selection::items(&p.path, a..b);
                Status::Done
            }
        }
    }

    // ---- the tree, changed through one primitive ----

    fn row(&self, path: &[Step]) -> &Row {
        self.root.at(path).unwrap_or(&self.root)
    }

    fn caret_row(&self) -> &Row {
        self.row(&self.sel.head.path)
    }

    /// Nesting depth of the caret's row.
    fn depth(&self) -> usize {
        self.sel.head.path.len()
    }

    fn set_head(&mut self, path: &[Step], offset: usize) {
        self.sel = Selection::caret(Position::new(path, offset));
    }

    /// Drops the selection, keeping the caret.
    fn collapse(&mut self) {
        self.sel.anchor = self.sel.head.clone();
    }

    /// The one primitive change: items `range` of the row at `path` become `items` (a slot left empty gets a
    /// placeholder, so slots are never empty). The marks after it move with their items.
    fn edit(&mut self, path: &[Step], range: Range<usize>, mut items: Vec<Item>) {
        let Some(row) = self.root.at_mut(path) else { return };
        let end = range.end.min(row.len());
        let at = range.start.min(end);
        if !path.is_empty() && row.len() - (end - at) + items.len() == 0 {
            items = vec![Item::Placeholder];
        }
        if at == end && items.is_empty() {
            return;
        }
        let removed: Vec<Item> = row.0.splice(at..end, items.iter().cloned()).collect();
        // a box removed is a change of structure: an undo step of its own
        self.structural |= removed.iter().any(|i| matches!(i, Item::Box(_)));
        let s = Splice { path: path.to_vec(), at, removed, inserted: items };
        self.closers.retain_mut(|p| shift(p, &s, true));
        if let Some(a) = &mut self.alias {
            if !shift(a, &s, false) {
                self.alias = None;
            }
        }
        self.unknown = None;
        self.pending.push(s);
    }

    /// After a caret motion: the automatic closers the caret is still inside of (between the closer and its opener,
    /// in their row or in a box between them) stay; the others are forgotten.
    fn keep_closers_around_caret(&mut self) {
        let head = self.sel.head.clone();
        let root = &self.root;
        self.closers.retain(|c| {
            let (Some(row), true) = (root.at(&c.path), head.path.starts_with(&c.path)) else { return false };
            let Some(open) = matching_open(row, c.offset) else { return false };
            match head.path.get(c.path.len()) {
                None => open < head.offset && head.offset <= c.offset,
                Some(step) => open < step.item && step.item < c.offset,
            }
        });
    }

    /// Brings a selection whose ends are in different rows into one row (the caret at the end it was on), before a
    /// command acts on it.
    fn settle(&mut self) {
        if self.sel.anchor.path == self.sel.head.path {
            return;
        }
        let Some((p, r)) = self.range() else {
            self.collapse();
            return;
        };
        let forward = self.sel.forward();
        self.sel = Selection {
            anchor: Position::new(&p, if forward { r.start } else { r.end }),
            head: Position::new(&p, if forward { r.end } else { r.start }),
        };
    }

    // ---- typing ----

    /// Deletes the selection; true if there was one.
    fn delete_selection(&mut self) -> bool {
        self.settle();
        let Some((p, r)) = self.range() else {
            self.collapse();
            return false;
        };
        self.edit(&p, r.clone(), vec![]);
        self.set_head(&p, r.start);
        true
    }

    /// Inserts items at the caret, replacing the selection or a slot's lone placeholder; the caret goes after them.
    /// Returns where the inserted items start (after a `*` put in front of them).
    fn insert_items(&mut self, items: Vec<Item>) -> usize {
        self.delete_selection();
        self.unknown = None;
        let c = self.sel.head.clone();
        let n = items.len();
        let row = self.row(&c.path);
        if row.is_placeholder() && n > 0 {
            self.edit(&c.path, 0..1, items);
            self.set_head(&c.path, n);
            return 0;
        }
        let at = c.offset.min(row.len());
        self.edit(&c.path, at..at, items);
        self.set_head(&c.path, at + n);
        self.implicit_times(&c.path, at, at + n)
    }

    /// Implicit multiplication where newly inserted items `start..end` of the row at `path` make a box touch an
    /// operand. The caret keeps its place relative to what was typed. Returns the start of the inserted items.
    fn implicit_times(&mut self, path: &[Step], start: usize, end: usize) -> usize {
        let Some(row) = self.root.at(path) else { return start };
        let (at_end, at_start) = (rules::needs_times(row, end), rules::needs_times(row, start));
        if at_end {
            self.insert_times(path, end, false);
        }
        if at_start {
            self.insert_times(path, start, true);
            return start + 1;
        }
        start
    }

    /// Inserts a `*` at `i` of the row at `path`, the caret moving along when it is after `i` (or at `i` and
    /// `shift_at` holds) or inside a box at or after `i`.
    fn insert_times(&mut self, path: &[Step], i: usize, shift_at: bool) {
        let alias_here = self.alias.as_ref().is_some_and(|a| a.path == path && a.offset == i);
        self.edit(path, i..i, vec![Item::Char('*')]);
        if alias_here {
            if let Some(a) = &mut self.alias {
                a.offset += 1;
            }
        }
        let d = path.len();
        for p in [&mut self.sel.head, &mut self.sel.anchor] {
            if p.path.len() > d && p.path[..d] == path[..] {
                if p.path[d].item >= i {
                    p.path[d].item += 1;
                }
            } else if p.path == path && (p.offset > i || (shift_at && p.offset == i)) {
                p.offset += 1;
            }
        }
    }

    /// Automatic closing (decision NB5.1), the same in every row: `(`, `[`, `{` insert their closer, the caret
    /// between, except inside strings and comments and right before a letter, digit, `_` or box (a bracket typed
    /// before existing text is usually meant to wrap it); `"` pairs outside strings and comments before white space,
    /// a closer, `,`, `;` or the end; an opener or `"` typed over a selection wraps it, the selection staying on the
    /// wrapped items; a typed closer right before a closer inserted automatically steps over it. True when handled.
    fn auto_close(&mut self, t: &str) -> bool {
        let mut it = t.chars();
        let (Some(ch), None) = (it.next(), it.next()) else { return false };
        let closer = if ch == '"' { Some('"') } else { closer_of(ch) };
        // step over an automatic closer
        if self.range().is_none() {
            let c = self.sel.head.clone();
            if self.row(&c.path).0.get(c.offset) == Some(&Item::Char(ch)) && self.closers.contains(&c) {
                self.closers.retain(|p| *p != c);
                self.set_head(&c.path, c.offset + 1);
                return true;
            }
        }
        let Some(close) = closer else { return false };
        self.settle();
        // a selected placeholder is typed over, not wrapped (design 08, section 4.3)
        let placeholder = self.range().is_some_and(|(p, r)| self.row(&p).0[r].iter().all(|i| *i == Item::Placeholder));
        if placeholder {
            self.delete_selection();
        }
        if let Some((path, r)) = self.range() {
            self.edit(&path, r.end..r.end, vec![Item::Char(close)]);
            self.edit(&path, r.start..r.start, vec![Item::Char(ch)]);
            self.sel = Selection::items(&path, r.start + 1..r.end + 1);
            return true;
        }
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        let next = if row.is_placeholder() { None } else { row.0.get(c.offset) };
        let pairs = rules::lexical_at(row, c.offset) == Lexical::Code
            && match (ch, next) {
                (_, None) => true,
                (_, Some(Item::Box(_))) => false,
                ('"', Some(Item::Char(n))) => n.is_whitespace() || ")]},;".contains(*n),
                (_, Some(Item::Char(n))) => !(n.is_alphanumeric() || *n == '_'),
                (_, Some(Item::Placeholder)) => false,
            };
        if !pairs {
            return false;
        }
        self.insert_items(vec![Item::Char(ch), Item::Char(close)]);
        let h = self.sel.head.clone();
        let at = Position::new(&h.path, h.offset - 1);
        self.closers.push(at.clone());
        self.sel = Selection::caret(at);
        true
    }

    /// Return: a line break keeping the indentation of the caret's line, one level more after an opening bracket;
    /// between a bracket and its closer (`f(|)`) the closer goes onto a line of its own at the old indentation and the
    /// caret onto the indented line between. Inside a string or comment, and with a selection, a plain line break.
    fn newline(&mut self) {
        self.settle();
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        if self.range().is_some() || rules::lexical_at(row, c.offset) != Lexical::Code || row.is_placeholder() {
            self.insert_items(vec![Item::Char('\n')]);
            return;
        }
        let k = c.offset.min(row.len());
        let line = rules::chars(row, rules::line_start(row, k)..k);
        let indent: String = line.chars().take_while(|ch| *ch == ' ' || *ch == '\t').collect();
        let opener = line.trim_end().chars().next_back().filter(|ch| "([{".contains(*ch));
        let closer = match row.0.get(k) {
            Some(Item::Char(ch)) => Some(*ch),
            _ => None,
        };
        let split = matches!((opener, closer), (Some('('), Some(')')) | (Some('['), Some(']')) | (Some('{'), Some('}')));
        let inner = if opener.is_some() { format!("{indent}{INDENT}") } else { indent.clone() };
        let t = if split { format!("\n{inner}\n{indent}") } else { format!("\n{inner}") };
        self.insert_items(Row::text(&t).0);
        if split {
            self.set_head(&c.path, k + 1 + inner.chars().count());
        }
    }

    /// Tab without placeholders: the indentation at the caret, or every selected line indented.
    fn indent(&mut self) {
        let (a, b) = self.root_span();
        let multi = self.root.0[a..b].contains(&Item::Char('\n'));
        if !multi {
            self.insert_items(Row::text(INDENT).0);
            return;
        }
        let starts = self.selected_lines();
        for &s in starts.iter().rev() {
            self.edit(&[], s..s, Row::text(INDENT).0);
        }
        let n = INDENT.len();
        self.sel = Selection::items(&[], a + n..b + n * starts.len());
    }

    /// Shift+Tab without placeholders: every selected line outdented by up to one level.
    fn outdent(&mut self) {
        let (mut a, mut b) = self.root_span();
        for &s in self.selected_lines().iter().rev() {
            let n = (s..self.root.len()).take(INDENT.len()).take_while(|&j| self.root.0[j] == Item::Char(' ')).count();
            self.edit(&[], s..s + n, vec![]);
            let fix = |p: usize| if p > s { p - n.min(p - s) } else { p };
            (a, b) = (fix(a), fix(b));
        }
        self.sel = Selection::items(&[], a..b);
    }

    /// `Alt+/`: comments the selected lines with `# `, or uncomments them when every non-blank one is a comment.
    fn toggle_comment(&mut self) {
        let (mut a, mut b) = self.root_span();
        let starts = self.selected_lines();
        let line_of = |root: &Row, s: usize| rules::chars(root, s..rules::line_end(root, s));
        let commented = starts
            .iter()
            .map(|&s| line_of(&self.root, s))
            .filter(|l| !l.trim().is_empty())
            .all(|l| l.trim_start().starts_with('#'));
        for &s in starts.iter().rev() {
            let line = line_of(&self.root, s);
            if commented {
                let indent = line.chars().count() - line.trim_start().chars().count();
                let rest = line.trim_start();
                let n = if rest.starts_with("# ") {
                    2
                } else {
                    usize::from(rest.starts_with('#'))
                };
                let at = s + indent;
                self.edit(&[], at..at + n, vec![]);
                let fix = |p: usize| if p > at { p - n.min(p - at) } else { p };
                (a, b) = (fix(a), fix(b));
            } else if !line.trim().is_empty() {
                self.edit(&[], s..s, Row::text("# ").0);
                let (oa, ob) = (a, b);
                let fix = |p: usize| if p >= s && !(p == s && p == oa && oa != ob) { p + 2 } else { p };
                (a, b) = (fix(a), fix(b));
            }
        }
        self.sel = Selection::items(&[], a..b);
    }

    /// The selection's ends in the root row (a position inside a box counts as the box's place).
    fn root_span(&self) -> (usize, usize) {
        let at = |p: &Position| p.path.first().map_or(p.offset, |s| s.item);
        let (x, y) = (at(&self.sel.anchor), at(&self.sel.head));
        let (a, b) = (x.min(y), x.max(y));
        let inside = |p: &Position| !p.path.is_empty();
        // a selection ending inside a box covers the box
        let b = if (inside(&self.sel.anchor) && at(&self.sel.anchor) == b) || (inside(&self.sel.head) && at(&self.sel.head) == b) {
            (b + 1).min(self.root.len())
        } else {
            b
        };
        (a, b)
    }

    /// The starts of the root lines the selection touches.
    fn selected_lines(&self) -> Vec<usize> {
        let (a, b) = self.root_span();
        rules::lines(&self.root)
            .into_iter()
            .filter(|r| r.start == rules::line_start(&self.root, a) || (r.start > a && r.start < b.max(a + 1)))
            .map(|r| r.start)
            .collect()
    }

    fn insert_template(&mut self, template: &str) -> bool {
        let mut row = from_text(template);
        if self.depth() + row.nesting() > MAX_NESTING {
            return false;
        }
        let mark = find_char(&row, '▮');
        if let Some((p, k)) = &mark {
            if let Some(r) = row.at_mut(p) {
                r.0[*k] = Item::Placeholder;
            }
        }
        self.delete_selection();
        let base = self.sel.head.path.clone();
        let n = row.len();
        let start = self.insert_items(row.0);
        match mark {
            Some((mut p, k)) => {
                if let Some(first) = p.first_mut() {
                    first.item += start;
                    let mut full = base;
                    full.extend(p);
                    self.sel = Selection::items(&full, k..k + 1);
                } else {
                    self.sel = Selection::items(&base, start + k..start + k + 1);
                }
            }
            None => self.set_head(&base, start + n),
        }
        true
    }

    // ---- deleting ----

    /// Backspace (or Ctrl+Backspace, `word`): the selection; the alias just started; the item before the caret (an
    /// empty bracket pair, or a pair whose closer came automatically, goes whole; in a line's leading spaces back to
    /// the previous indentation stop); a box before the caret is entered (deleted when all its slots are empty); at
    /// the start of a slot the box goes, its contents kept as linear text.
    fn backspace(&mut self, word: bool) -> Status {
        if self.delete_selection() {
            return Status::Done;
        }
        if self.alias.as_ref() == Some(&self.sel.head) {
            self.alias = None;
            return Status::Done;
        }
        self.unknown = None;
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        let k = c.offset.min(row.len());
        if k == 0 || row.is_placeholder() {
            self.at_slot_edge(false);
            return Status::Done;
        }
        match &row.0[k - 1] {
            Item::Box(b) if b.slot_count() > 0 && !b.is_blank() => self.enter_box(k - 1, false),
            Item::Char(o) if self.pair_after(&c, *o) => self.remove(k - 1..k + 1),
            Item::Char(_) if word => {
                let j = rules::word_left(row, k);
                // a word that reaches back to a box stops after it (the box is entered by the next press)
                let j = (j..k).rev().find(|&i| matches!(row.0[i], Item::Box(_))).map_or(j, |i| i + 1);
                self.remove(j..k);
            }
            Item::Char(' ') if c.path.is_empty() => {
                let ls = rules::line_start(row, k);
                let lead = k - ls;
                if lead >= 2 && row.0[ls..k].iter().all(|i| *i == Item::Char(' ')) {
                    let stop = (lead - 1) / INDENT.len() * INDENT.len();
                    self.remove(ls + stop..k);
                } else {
                    self.remove(k - 1..k);
                }
            }
            _ => self.remove(k - 1..k),
        }
        Status::Done
    }

    /// Whether the opener `o` before the caret and the item at the caret are a pair that Backspace removes whole: an
    /// empty bracket pair, or any pair whose closer was inserted automatically.
    fn pair_after(&self, c: &Position, o: char) -> bool {
        let close = if o == '"' { Some('"') } else { closer_of(o) };
        let Some(close) = close else { return false };
        let row = self.row(&c.path);
        row.0.get(c.offset) == Some(&Item::Char(close)) && (o != '"' || self.closers.contains(c))
    }

    /// Delete (or Ctrl+Delete, `word`): the mirror of [`Editor::backspace`].
    fn delete(&mut self, word: bool) -> Status {
        if self.delete_selection() {
            return Status::Done;
        }
        self.unknown = None;
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        let k = c.offset;
        if k >= row.len() || row.is_placeholder() {
            self.at_slot_edge(true);
            return Status::Done;
        }
        match &row.0[k] {
            Item::Box(b) if b.slot_count() > 0 && !b.is_blank() => self.enter_box(k, true),
            Item::Char(_) if word => {
                let j = rules::word_right(row, k);
                let j = (k..j).find(|&i| matches!(row.0[i], Item::Box(_))).unwrap_or(j).max(k + 1);
                self.remove(k..j);
            }
            _ => self.remove(k..k + 1),
        }
        Status::Done
    }

    fn remove(&mut self, r: Range<usize>) {
        let path = self.sel.head.path.clone();
        let start = r.start;
        self.edit(&path, r, vec![]);
        self.set_head(&path, start);
    }

    /// Backspace at the start (or Delete at the end) of a slot.
    fn at_slot_edge(&mut self, forward: bool) {
        let Some(last) = self.sel.head.path.last().copied() else { return };
        self.structural = true;
        let parent: RowPath = self.sel.head.path[..self.sel.head.path.len() - 1].to_vec();
        let Some(Item::Box(b)) = self.root.at(&parent).and_then(|r| r.0.get(last.item)) else { return };
        let b = (**b).clone();
        if b.is_blank() {
            self.edit(&parent, last.item..last.item + 1, vec![]);
            let len = self.row(&parent).len();
            let at = if self.row(&parent).is_placeholder() { 0 } else { last.item.min(len) };
            self.set_head(&parent, at);
            return;
        }
        // an n-th root (slots: index 0, radicand 1): between its slots Backspace and Delete step across, deleting
        // nothing; an empty index goes (a square root is left); at the outer edges a filled root becomes its linear
        // text like a fraction or a power, so no character typed is lost and the term stays
        if let Node::Radical { radicand, index: Some(index) } = &b {
            let item = last.item;
            let inner = (last.slot == 1) != forward;
            if inner || index.is_placeholder() {
                let (rlen, ilen, empty) = (radicand.len(), index.len(), index.is_placeholder());
                let (slot, offset) = if empty {
                    let square = Node::Radical { radicand: radicand.clone(), index: None };
                    self.edit(&parent, item..item + 1, vec![Item::Box(Box::new(square))]);
                    (0, if forward && last.slot == 1 { rlen } else { 0 })
                } else if forward {
                    (1, 0)
                } else {
                    (0, ilen)
                };
                let mut p = parent;
                p.push(Step { item, slot });
                self.set_head(&p, offset);
                return;
            }
            let lin = linear_text(&Row(vec![Item::Box(Box::new(b.clone()))]));
            let items = Row::text(&lin).0;
            let caret_at = if forward { items.len() } else { 0 };
            self.edit(&parent, item..item + 1, items);
            self.set_head(&parent, item + caret_at);
            return;
        }
        // the box goes and its contents stay as linear text (decision NBUI.6)
        let (items, caret_at) = rules::unwrap_box(&b, last.slot, forward);
        let caret_at = caret_at.min(items.len());
        self.edit(&parent, last.item..last.item + 1, items);
        self.set_head(&parent, last.item + caret_at);
    }

    /// Puts the caret inside the box at item `k` of the caret's row: at the start of its first slot or the end of its
    /// last.
    fn enter_box(&mut self, k: usize, first: bool) {
        let c = self.sel.head.clone();
        let Some(Item::Box(b)) = self.row(&c.path).0.get(k) else { return };
        let n = b.slot_count();
        if n == 0 {
            self.set_head(&c.path, if first { k + 1 } else { k });
            return;
        }
        let slot = if first { 0 } else { n - 1 };
        let len = b.slot(slot).map_or(0, |r| r.len());
        let mut p = c.path;
        p.push(Step { item: k, slot });
        self.set_head(&p, if first { 0 } else { len });
    }

    // ---- moving ----

    fn motion(&mut self, m: Motion, extend: bool, cx: &Context<'_>) -> Status {
        match m {
            Motion::Left | Motion::Right => self.horizontal(m == Motion::Right, extend),
            Motion::WordLeft | Motion::WordRight => self.word_motion(m == Motion::WordRight, extend),
            Motion::Home | Motion::End => {
                self.home_end(m == Motion::End, extend, cx);
                Status::Done
            }
            Motion::Up | Motion::Down => self.vertical(m == Motion::Up, extend, cx),
        }
    }

    /// Left and Right: over an item, into a box, out of a slot to the next one or past the box. With `extend` the
    /// selection grows with boxes as units, except the box holding the anchor, which is entered.
    fn horizontal(&mut self, forward: bool, extend: bool) -> Status {
        self.unknown = None;
        if !extend {
            self.settle();
            if let Some((p, r)) = self.range() {
                self.set_head(&p, if forward { r.end } else { r.start });
                return Status::Done;
            }
            self.collapse();
        }
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        let (len, k) = (row.len(), c.offset);
        let inside = if forward { k < len } else { k > 0 };
        if extend {
            let anchor = self.sel.anchor.clone();
            if inside {
                let at = if forward { k } else { k - 1 };
                let d = c.path.len();
                let holds_anchor = anchor.path.len() > d && anchor.path[..d] == c.path[..] && anchor.path[d].item == at;
                let enter = holds_anchor && matches!(&row.0[at], Item::Box(b) if b.slot_count() > 0);
                self.sel.head = if enter {
                    let mut p = c.path.clone();
                    p.push(Step { item: at, slot: anchor.path[d].slot });
                    let len = self.root.at(&p).map_or(0, |r| r.len());
                    Position { path: p, offset: if forward { 0 } else { len } }
                } else {
                    Position::new(&c.path, if forward { k + 1 } else { k - 1 })
                };
                return Status::Done;
            }
            let mut p = c.path;
            let Some(last) = p.pop() else { return if forward { Status::After } else { Status::Before } };
            self.sel.head = Position { path: p, offset: if forward { last.item + 1 } else { last.item } };
            return Status::Done;
        }
        if inside {
            let at = if forward { k } else { k - 1 };
            if let Item::Box(b) = &row.0[at] {
                if b.slot_count() > 0 {
                    self.enter_box(at, forward);
                    return Status::Done;
                }
            }
            self.set_head(&c.path, if forward { k + 1 } else { k - 1 });
            return Status::Done;
        }
        // at the edge of a slot: the next slot, or out of the box
        let Some(last) = c.path.last().copied() else { return if forward { Status::After } else { Status::Before } };
        let parent: RowPath = c.path[..c.path.len() - 1].to_vec();
        let n = self.root.box_of(&c.path).map_or(0, |b| b.slot_count());
        let next = if forward { last.slot + 1 } else { last.slot.wrapping_sub(1) };
        if next < n {
            let mut p = parent;
            p.push(Step { item: last.item, slot: next });
            let len = self.root.at(&p).map_or(0, |r| r.len());
            self.set_head(&p, if forward { 0 } else { len });
        } else {
            self.set_head(&parent, if forward { last.item + 1 } else { last.item });
        }
        Status::Done
    }

    /// Ctrl+Left and Ctrl+Right: the previous or next word of the caret's row; at the row's edge as Left and Right.
    fn word_motion(&mut self, forward: bool, extend: bool) -> Status {
        if !extend {
            self.settle();
            if let Some((p, r)) = self.range() {
                self.set_head(&p, if forward { r.end } else { r.start });
                return Status::Done;
            }
        }
        let c = self.sel.head.clone();
        let row = self.row(&c.path);
        let k = c.offset.min(row.len());
        let target = if row.is_placeholder() {
            k
        } else if forward {
            rules::word_right(row, k)
        } else {
            rules::word_left(row, k)
        };
        if target == k {
            return self.horizontal(forward, extend);
        }
        if extend {
            self.sel.head = Position::new(&c.path, target);
        } else {
            self.set_head(&c.path, target);
        }
        Status::Done
    }

    /// Home and End: in a slot the edge of its line; pressed again (or on the root row) the edge of the visual line.
    fn home_end(&mut self, end: bool, extend: bool, cx: &Context<'_>) {
        let c = self.sel.head.clone();
        let to = |ed: &mut Editor, p: Position| {
            if extend {
                ed.sel.head = p;
            } else {
                ed.sel = Selection::caret(p);
            }
        };
        let root_offset = if c.path.is_empty() {
            c.offset
        } else {
            let row = self.row(&c.path);
            let edge = if end { rules::line_end(row, c.offset) } else { rules::line_start(row, c.offset) };
            if edge != c.offset && !row.is_placeholder() {
                to(self, Position::new(&c.path, edge));
                return;
            }
            c.path[0].item + usize::from(end)
        };
        let logical = Logical(&self.root);
        let g: &dyn Geometry = cx.geometry.unwrap_or(&logical);
        let at = Position::root(root_offset);
        let edge = match (g.locate(&at), cx.geometry) {
            (Some((_, k)), _) => g.line_items(k).map(|r| {
                if end {
                    let brk = r.end > r.start && self.root.0.get(r.end - 1) == Some(&Item::Char('\n'));
                    r.end - usize::from(brk)
                } else {
                    r.start
                }
            }),
            _ => None,
        };
        let edge = edge.unwrap_or(if end { rules::line_end(&self.root, root_offset) } else { rules::line_start(&self.root, root_offset) });
        to(self, Position::root(edge.min(self.root.len())));
    }

    /// Up and Down: inside structures first (denominator to numerator, base to exponent), then by visual lines
    /// keeping the goal x; past the first or last line the content's edge (with `extend`, the selection grows to
    /// the content's start or end).
    fn vertical(&mut self, up: bool, extend: bool, cx: &Context<'_>) -> Status {
        let anchor = self.sel.anchor.clone();
        if let Some(p) = self.slot_above_below(up) {
            self.sel = Selection { anchor: if extend { anchor } else { p.clone() }, head: p };
            return Status::Done;
        }
        let logical = Logical(&self.root);
        let g: &dyn Geometry = cx.geometry.unwrap_or(&logical);
        let head = self.sel.head.clone();
        let target = g.locate(&head).and_then(|(x0, k)| {
            let x = *self.goal_x.get_or_insert(x0);
            let t = if up { k.checked_sub(1)? } else { k + 1 };
            (t < g.line_count()).then(|| g.nearest(t, x)).flatten()
        });
        match target {
            Some(p) => {
                let p = p.clamped(&self.root);
                self.sel = Selection { anchor: if extend { anchor } else { p.clone() }, head: p };
                Status::Done
            }
            None if extend => {
                self.sel.head = Position::root(if up { 0 } else { self.root.len() });
                Status::Done
            }
            None if up => Status::Before,
            None => Status::After,
        }
    }

    /// Up or Down within the caret's enclosing structures only (denominator to numerator, base to exponent), the
    /// selection dropped; false when no structure has a slot there (a read-only output then leaves by lines).
    pub fn slot_vertical(&mut self, up: bool) -> bool {
        match self.slot_above_below(up) {
            Some(p) => {
                self.sel = Selection::caret(p);
                true
            }
            None => false,
        }
    }

    /// The position in the slot above or below the caret's within its enclosing structures.
    fn slot_above_below(&self, up: bool) -> Option<Position> {
        let path = &self.sel.head.path;
        for depth in (1..=path.len()).rev() {
            let p = &path[..depth];
            let Some(b) = self.root.box_of(p) else { continue };
            let slot = p[depth - 1].slot;
            let target = match (b, up) {
                (Node::Fraction { .. }, true) if slot == 1 => Some(0),
                (Node::Fraction { .. }, false) if slot == 0 => Some(1),
                (Node::Superscript { .. }, true) if slot == 0 => Some(1),
                (Node::Superscript { .. }, false) if slot == 1 => Some(0),
                (Node::Subscript { .. }, true) if slot == 1 => Some(0),
                (Node::Subscript { .. }, false) if slot == 0 => Some(1),
                (Node::Overscript { .. }, true) if slot == 0 => Some(1),
                (Node::Overscript { .. }, false) if slot == 1 => Some(0),
                (Node::Underscript { .. }, true) if slot == 1 => Some(0),
                (Node::Underscript { .. }, false) if slot == 0 => Some(1),
                (Node::Grid { rows }, _) => {
                    let w = rows.first().map_or(1, |r| r.len().max(1));
                    let (r, c) = (slot / w, slot % w);
                    match up {
                        true if r > 0 => Some((r - 1) * w + c),
                        false if r + 1 < rows.len() => Some((r + 1) * w + c),
                        _ => None,
                    }
                }
                _ => None,
            };
            if let Some(t) = target {
                let mut q = p[..depth - 1].to_vec();
                q.push(Step { item: p[depth - 1].item, slot: t });
                let len = self.root.at(&q).map_or(0, |r| r.len());
                let offset = self.sel.head.offset.min(len);
                return Some(Position { path: q, offset });
            }
        }
        None
    }

    /// `Tab` among placeholders: selects the next one in reading order. False when there is none after the caret.
    fn next_placeholder(&mut self) -> bool {
        self.settle();
        let order = positions(&self.root);
        let here = self.ordinal(&order, true);
        let found = order.iter().filter(|(_, _, ph)| *ph).find(|(o, _, _)| *o > here).cloned();
        self.select_found(found)
    }

    /// `Shift+Tab` among placeholders: selects the previous one. False when there is none before the caret.
    fn prev_placeholder(&mut self) -> bool {
        self.settle();
        let order = positions(&self.root);
        let here = self.ordinal(&order, false);
        let found = order.iter().filter(|(_, _, ph)| *ph).rev().find(|(o, _, _)| *o < here).cloned();
        self.select_found(found)
    }

    fn select_found(&mut self, found: Option<(usize, Position, bool)>) -> bool {
        match found {
            Some((_, c, _)) => {
                self.sel = Selection::items(&c.path, c.offset..c.offset + 1);
                true
            }
            None => false,
        }
    }

    fn ordinal(&self, order: &[(usize, Position, bool)], forward: bool) -> usize {
        let offset = match self.range() {
            Some((_, r)) if forward => r.end.saturating_sub(1).max(r.start),
            Some((_, r)) => r.start,
            None => self.sel.head.offset,
        };
        order.iter().find(|(_, c, _)| c.path == self.sel.head.path && c.offset == offset).map_or(0, |(o, _, _)| *o)
    }

    // ---- structures ----

    /// A box key (design 08, section 4.2): the selection, or the operand before the caret, becomes the first slot
    /// and the caret goes to the next slot; with nothing to take, the caret goes into the first slot. False if the
    /// tree would nest too deeply.
    fn structure(&mut self, s: Structure) -> bool {
        if s == Structure::Root && self.range().is_none() && self.switch_root_slot() {
            return true;
        }
        if self.depth() + 1 >= MAX_NESTING {
            return false;
        }
        self.unknown = None;
        self.settle();
        let range = match self.range() {
            Some((_, r)) => {
                // spaces at the edges of a selection stay outside the box (round 32)
                let row = self.caret_row();
                let blank = |j: usize| matches!(row.0.get(j), Some(Item::Char(' ' | '\t')));
                let (mut a, mut b) = (r.start, r.end);
                while a < b && blank(a) {
                    a += 1;
                }
                while b > a && blank(b - 1) {
                    b -= 1;
                }
                // a power of the whole inside of a bracket pair takes the brackets as its base (round 33)
                if s == Structure::Superscript
                    && a > 0
                    && matches!((row.0.get(a - 1), row.0.get(b)), (Some(Item::Char(o)), Some(Item::Char(c))) if closer_of(*o) == Some(*c))
                    && matching_open(row, b) == Some(a - 1)
                {
                    a -= 1;
                    b += 1;
                }
                Some(if a < b { a..b } else { r })
            }
            None => self.operand(s),
        };
        self.collapse();
        let c = self.sel.head.clone();
        let k = c.offset;
        let taken: Vec<Item> = match &range {
            Some(r) => self.caret_row().0[r.clone()].to_vec(),
            None => Vec::new(),
        };
        let has = !taken.is_empty();
        let operand = if has { Row(taken) } else { Row::placeholder() };
        let ph = Row::placeholder;
        let (node, slot) = match s {
            Structure::Fraction => (Node::Fraction { num: operand, den: ph() }, usize::from(has)),
            Structure::Superscript => (Node::Superscript { base: operand, exp: ph() }, usize::from(has)),
            Structure::Radical => (Node::Radical { radicand: operand, index: None }, 0),
            Structure::Root => (Node::Radical { radicand: operand, index: Some(ph()) }, 0),
        };
        let replacing_placeholder = self.caret_row().is_placeholder() && !has;
        let (at, end) = match (&range, replacing_placeholder) {
            (_, true) => (0, 1),
            (Some(r), _) => (r.start, r.end),
            (None, _) => (k, k),
        };
        self.edit(&c.path, at..end, vec![Item::Box(Box::new(node))]);
        let mut p = c.path.clone();
        p.push(Step { item: at, slot });
        let slot_len = self.root.at(&p).map_or(0, |r| r.len());
        // a radical over a selection leaves the caret at the end of the radicand
        let offset = if s == Structure::Radical && has { slot_len } else { 0 };
        self.set_head(&p, offset);
        self.implicit_times(&c.path, at, at + 1);
        true
    }

    /// `Ctrl+5` with the caret directly in a slot of a root: from the radicand to the index (a square root gets an
    /// empty index first), from the index to the end of the radicand. False when the caret is in no root's slot.
    fn switch_root_slot(&mut self) -> bool {
        let Some(last) = self.sel.head.path.last().copied() else { return false };
        let parent: RowPath = self.sel.head.path[..self.sel.head.path.len() - 1].to_vec();
        let Some(Item::Box(b)) = self.root.at(&parent).and_then(|r| r.0.get(last.item)) else { return false };
        let Node::Radical { radicand, index } = &**b else { return false };
        let (slot, offset) = match (index.is_some(), last.slot) {
            (true, 0) => (1, radicand.len()),
            (true, _) => (0, index.as_ref().map_or(0, |i| i.len())),
            (false, _) => {
                let n = Node::Radical { radicand: radicand.clone(), index: Some(Row::placeholder()) };
                self.edit(&parent, last.item..last.item + 1, vec![Item::Box(Box::new(n))]);
                (0, 0)
            }
        };
        let mut p = parent;
        p.push(Step { item: last.item, slot });
        self.set_head(&p, offset);
        self.unknown = None;
        true
    }

    /// The operand before the caret for a box key: the last primary ending at the caret; none for roots, after an
    /// operator, or in an empty slot.
    fn operand(&self, s: Structure) -> Option<Range<usize>> {
        if matches!(s, Structure::Radical | Structure::Root) {
            return None;
        }
        let row = self.caret_row();
        if row.is_placeholder() {
            return None;
        }
        primary_before(row, self.sel.head.offset.min(row.len()))
    }

    // ---- selection growth ----

    /// The canonical text, its map and the byte range of the selection in it.
    fn selection_bytes(&self) -> (String, mc_boxes::TextMap, usize, usize) {
        let (text, map) = to_text_mapped(&self.root);
        let (a, b) = match self.range() {
            Some((p, r)) => (map.offset(&p, r.start).unwrap_or(0), map.offset(&p, r.end).unwrap_or(0)),
            None => {
                let c = &self.sel.head;
                let o = map.offset(&c.path, c.offset).unwrap_or(0);
                (o, o)
            }
        };
        (text, map, a, b)
    }

    /// `Ctrl+.` (design 08, section 3.4): in a tree without boxes the next syntactic unit of the text (mc-syntax's
    /// grammar); with boxes the word at the caret, then the whole row, then the enclosing box, and so on outwards.
    fn expand_selection(&mut self) -> Status {
        if mc_boxes::has_boxes(&self.root) {
            return self.expand_in_boxes();
        }
        self.expand_by_grammar()
    }

    fn expand_in_boxes(&mut self) -> Status {
        self.settle();
        let path = self.sel.head.path.clone();
        let row = self.row(&path);
        let len = row.len();
        match self.range() {
            None => {
                let k = self.sel.head.offset.min(len);
                let is_w = |j: usize| matches!(row.0.get(j), Some(Item::Char(c)) if mc_boxes::ops::is_word(*c));
                let (mut a, mut b) = (k, k);
                while a > 0 && is_w(a - 1) {
                    a -= 1;
                }
                while b < len && is_w(b) {
                    b += 1;
                }
                if a == b {
                    if k > 0 {
                        a = k - 1;
                    } else if k < len {
                        b = k + 1;
                    }
                }
                if a == b {
                    return self.lift_selection();
                }
                self.sel = Selection::items(&path, a..b);
                Status::Done
            }
            Some((p, r)) if r != (0..len) => {
                self.sel = Selection::items(&p, 0..len);
                Status::Done
            }
            Some(_) => self.lift_selection(),
        }
    }

    /// The box holding the selection's row selected whole; `Unhandled` at the root.
    fn lift_selection(&mut self) -> Status {
        let mut p = self.sel.head.path.clone();
        match p.pop() {
            Some(last) => {
                self.sel = Selection::items(&p, last.item..last.item + 1);
                Status::Done
            }
            None => Status::Unhandled,
        }
    }

    fn expand_by_grammar(&mut self) -> Status {
        let (text, map, mut a, mut b) = self.selection_bytes();
        let current = self.range();
        loop {
            let Some(next) = mc_syntax::extend_selection(&text, a, b) else { return Status::Unhandled };
            if next.start == a && next.end == b {
                return Status::Unhandled;
            }
            if let Some((p, r)) = map.find(next.clone()) {
                if Some((p.clone(), r.clone())) != current {
                    self.sel = Selection::items(&p, r);
                    return Status::Done;
                }
            }
            (a, b) = (next.start, next.end);
        }
    }

    /// `Ctrl+Shift+B`: the smallest bracketed expression strictly around the selection.
    fn check_balance(&mut self) -> Status {
        let (text, map, a, b) = self.selection_bytes();
        let colouring = mc_syntax::colour(&text, None);
        match colouring.enclosing_brackets(a, b).and_then(|r| map.find(r)) {
            Some((p, r)) => {
                self.sel = Selection::items(&p, r);
                Status::Done
            }
            None => Status::Beep,
        }
    }

    // ---- aliases ----

    /// `Esc` (design 08, section 3.9): starts an alias at the caret, or ends one: the name typed since the start is
    /// looked up and replaced by its character or template. With a selection it does nothing.
    fn escape(&mut self) -> Status {
        if self.range().is_some() {
            return Status::Done;
        }
        match self.alias_mark() {
            Some(m) if !m.unknown => {
                self.alias = None;
                let row = self.row(&m.path);
                let name: String =
                    row.0[m.start..m.end].iter().filter_map(|i| if let Item::Char(c) = i { Some(*c) } else { None }).collect();
                if name.is_empty() {
                    return Status::Done;
                }
                match aliases::lookup_cell(&name) {
                    Some(Alias::Text(t)) => {
                        self.sel = Selection::items(&m.path, m.start..m.end);
                        self.insert_items(Row::text(t).0);
                        Status::Done
                    }
                    Some(Alias::Template(t)) => {
                        self.sel = Selection::items(&m.path, m.start..m.end);
                        self.delete_selection();
                        self.insert_template(t);
                        Status::Done
                    }
                    None => {
                        self.unknown = Some(AliasMark { unknown: true, ..m });
                        Status::Beep
                    }
                }
            }
            _ => {
                self.unknown = None;
                self.alias = Some(self.sel.head.clone());
                Status::Done
            }
        }
    }
}

/// Moves a position over a splice: `true` keeps it. An item position (a closer) moves when it is at or after the
/// change and goes when its item is removed; a boundary (an alias start) moves only when strictly after the change's
/// start. A position inside a removed box goes.
fn shift(p: &mut Position, s: &Splice, item: bool) -> bool {
    let d = s.path.len();
    let end = s.at + s.removed.len();
    let delta = s.inserted.len() as isize - s.removed.len() as isize;
    let moved = |v: usize| (v as isize + delta).max(0) as usize;
    if p.path.len() > d && p.path[..d] == s.path[..] {
        let i = p.path[d].item;
        if i >= end {
            p.path[d].item = moved(i);
            true
        } else {
            i < s.at
        }
    } else if p.path == s.path {
        let o = p.offset;
        if item {
            if o >= end {
                p.offset = moved(o);
                true
            } else {
                o < s.at
            }
        } else if o <= s.at {
            true
        } else if o >= end {
            p.offset = moved(o);
            true
        } else {
            false
        }
    } else {
        true
    }
}

/// Where the name before `k` starts in `row` (letters, digits, `_` and `.`).
fn name_start(row: &Row, k: usize) -> usize {
    let mut j = k.min(row.len());
    while j > 0 && matches!(row.0[j - 1], Item::Char(c) if c.is_alphanumeric() || c == '_' || c == '.') {
        j -= 1;
    }
    j
}

/// Every caret position of a tree in reading order, with whether a placeholder sits there.
fn positions(root: &Row) -> Vec<(usize, Position, bool)> {
    fn walk(r: &Row, path: &mut RowPath, out: &mut Vec<(usize, Position, bool)>) {
        for i in 0..=r.len() {
            let ph = matches!(r.0.get(i), Some(Item::Placeholder));
            let n = out.len();
            out.push((n, Position::new(path, i), ph));
            if let Some(Item::Box(b)) = r.0.get(i) {
                for k in 0..b.slot_count() {
                    path.push(Step { item: i, slot: k });
                    if let Some(s) = b.slot(k) {
                        walk(s, path, out);
                    }
                    path.pop();
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut Vec::new(), &mut out);
    out
}

/// The first occurrence of a character in a tree: its row path and index.
fn find_char(r: &Row, c: char) -> Option<(RowPath, usize)> {
    fn walk(r: &Row, c: char, path: &mut RowPath) -> Option<(RowPath, usize)> {
        for (i, it) in r.0.iter().enumerate() {
            match it {
                Item::Char(x) if *x == c => return Some((path.clone(), i)),
                Item::Box(b) => {
                    for k in 0..b.slot_count() {
                        path.push(Step { item: i, slot: k });
                        let found = b.slot(k).and_then(|s| walk(s, c, path));
                        path.pop();
                        if found.is_some() {
                            return found;
                        }
                    }
                }
                _ => {}
            }
        }
        None
    }
    walk(r, c, &mut Vec::new())
}
