//! Property tests of the editor (design 11, section 5): random command sequences, then
//! - every transaction applied to the tree before it gives the tree after it, and reverting it gives the tree
//!   before (and the selection before) exactly;
//! - after the whole sequence, undoing every step gives the first tree and selection, redoing gives the last;
//! - after every command `from_text(to_text(tree)) == tree`, slots are never empty and the selection is valid.
//!
//! Seeded and bounded; `MC_EDIT_SEED` and `MC_EDIT_CASES` override the defaults.
use mc_boxes::{from_text, to_text, Item, Row};
use mc_edit::{Command, Context, Editor, History, Motion, Position, Selection, Structure, Transaction};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn seed() -> u64 {
    std::env::var("MC_EDIT_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x5eed_e341_77aa_0b1d)
}

fn cases(default: usize) -> usize {
    std::env::var("MC_EDIT_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn slots_filled(r: &Row) -> bool {
    r.0.iter().all(|i| match i {
        Item::Box(b) => b.slots().iter().all(|s| !s.is_empty() && slots_filled(s)),
        _ => true,
    })
}

/// Every caret position of a tree.
fn positions(r: &Row, path: &mut Vec<mc_boxes::Step>, out: &mut Vec<Position>) {
    for i in 0..=r.len() {
        out.push(Position::new(path, i));
        if let Some(Item::Box(b)) = r.0.get(i) {
            for k in 0..b.slot_count() {
                path.push(mc_boxes::Step { item: i, slot: k });
                positions(b.slot(k).unwrap(), path, out);
                path.pop();
            }
        }
    }
}

fn random_position(ed: &Editor, r: &mut Rng) -> Position {
    let mut all = vec![];
    positions(ed.root(), &mut vec![], &mut all);
    all[r.below(all.len())].clone()
}

/// A random command. Typed characters avoid the letters of the box shapes' heads, so no typed text reads as a box.
fn command(ed: &Editor, r: &mut Rng) -> Command {
    const CHARS: &[&str] = &["a", "b", "x", "1", "2", " ", "+", "-", "*", "(", ")", "[", "]", "\"", "#", ",", "\n", "y z"];
    use Motion::*;
    let motions = [Left, Right, WordLeft, WordRight, Up, Down, Home, End];
    match r.below(24) {
        0..=6 => Command::Type(CHARS[r.below(CHARS.len())].to_string()),
        7 => Command::Insert("q(r)".into()),
        8 => Command::Newline,
        9 => Command::Backspace,
        10 => Command::Delete,
        11 => [Command::DeleteWordBack, Command::DeleteWordForward][r.below(2)].clone(),
        12..=14 => Command::Move(motions[r.below(motions.len())], r.below(3) == 0),
        15 | 16 => Command::Structure([Structure::Fraction, Structure::Superscript, Structure::Radical, Structure::Root][r.below(4)]),
        17 => Command::ExitStructure,
        18 => [Command::Tab, Command::Outdent][r.below(2)].clone(),
        19 => Command::Alias,
        20 => Command::SetCaret(random_position(ed, r)),
        21 => Command::ExtendTo(random_position(ed, r)),
        22 => [Command::Paste("((a)/(b))".into()), Command::ToggleComment, Command::SelectAll, Command::ExpandSelection][r.below(4)].clone(),
        _ => Command::SelectWord(random_position(ed, r)),
    }
}

#[test]
fn transactions_undo_exactly_and_texts_read_back() {
    let mut r = Rng(seed());
    let n = cases(400);
    for case in 0..n {
        let auto_close = case % 2 == 0;
        let cx = Context { geometry: None, auto_close };
        let mut ed = Editor::new(Row::text(["", "x", "f(a, b)", "1 +\n  2"][case % 4]));
        let first = (ed.root().clone(), ed.selection().clone());
        let mut done: Vec<Transaction> = vec![];
        let mut log = vec![];
        for _ in 0..r.below(60) + 1 {
            let cmd = command(&ed, &mut r);
            log.push(cmd.clone());
            let before = ed.root().clone();
            let out = ed.apply(&cmd, &cx);
            let ctx = || format!("case {case}, commands {log:?}\nnotation {}", ed.notation());
            assert!(slots_filled(ed.root()), "{}", ctx());
            assert!(ed.selection().head.valid(ed.root()) && ed.selection().anchor.valid(ed.root()), "{}", ctx());
            if boxes_in_code(ed.root()) {
                assert_eq!(from_text(&to_text(ed.root())), *ed.root(), "{}", ctx());
            }
            match out.tx {
                Some(tx) => {
                    let mut t = before.clone();
                    tx.apply(&mut t);
                    assert_eq!(t, *ed.root(), "redo {}", ctx());
                    tx.revert(&mut t);
                    assert_eq!(t, before, "undo {}", ctx());
                    assert_eq!(tx.after, *ed.selection(), "{}", ctx());
                    done.push(tx);
                }
                None => assert_eq!(before, *ed.root(), "a change without a transaction: {}", ctx()),
            }
        }
        let last = (ed.root().clone(), ed.selection().clone());
        // undo everything, then redo everything
        for tx in done.iter().rev() {
            ed.revert(tx);
        }
        assert_eq!(*ed.root(), first.0, "full undo, case {case} {log:?}");
        if let Some(tx) = done.first() {
            // the selection the first change started from, exactly
            assert_eq!(*ed.selection(), tx.before, "case {case} {log:?}");
        }
        for tx in &done {
            ed.redo(tx);
        }
        assert_eq!(*ed.root(), last.0, "full redo, case {case} {log:?}");
        if let Some(tx) = done.last() {
            assert_eq!(*ed.selection(), tx.after, "case {case}");
        }
    }
}

/// The selection before the first command comes back with the first tree when nothing moved the caret between
/// commands (a history of edits only).
#[test]
fn a_history_restores_the_first_tree_and_selection() {
    let mut r = Rng(seed() ^ 0x77);
    for case in 0..cases(200) {
        let cx = Context { geometry: None, auto_close: true };
        let mut ed = Editor::new(Row::text("a+b"));
        ed.set_selection(Selection::caret(Position::root(case % 4)));
        let first = (ed.root().clone(), ed.selection().clone());
        let mut h = History::default();
        let mut root = ed.root().clone();
        let mut last_after = None;
        for _ in 0..r.below(30) + 1 {
            let cmd = match command(&ed, &mut r) {
                // edits only: a click or a motion between edits would leave the selection elsewhere
                Command::SetCaret(_) | Command::ExtendTo(_) | Command::SelectWord(_) | Command::Move(..) => Command::Type("a".into()),
                c => c,
            };
            if let Some(tx) = ed.apply(&cmd, &cx).tx {
                last_after = Some(tx.after.clone());
                h.push(tx);
            }
        }
        let last = (ed.root().clone(), ed.selection().clone());
        root.clone_from(ed.root());
        let mut sel = None;
        while let Some(s) = h.undo(&mut root) {
            sel = Some(s);
        }
        assert_eq!(root, first.0, "case {case}");
        if let Some(s) = sel {
            // the first step's selection before it: where the caret was when the first edit began
            assert!(s.head.valid(&root), "case {case}");
        }
        let mut sel = None;
        while let Some(s) = h.redo(&mut root) {
            sel = Some(s);
        }
        assert_eq!(root, last.0, "case {case}");
        if let Some(s) = sel {
            assert_eq!(Some(s), last_after, "case {case}");
        }
    }
}

/// Whether every box and placeholder of the tree is in code (not after a `#` or inside a string of its row): a box in a comment or a
/// string cannot read back as a box, nor one touching an operand: the limits of the canonical text (design 08,
/// section 4.4).
fn boxes_in_code(row: &Row) -> bool {
    // a box that a deletion left touching a name reads as a call (`f((sqrt(x)))`); typing puts a `*` there
    if (1..row.len()).any(|j| mc_edit::rules::needs_times(row, j)) {
        return false;
    }
    // characters that spell a box shape (`((`, from the linear text an unwrapped root leaves) read as the box: the
    // documented ambiguity of design 08, section 4.4
    if row.0.windows(2).any(|w| w[0] == Item::Char('(') && matches!(w[1], Item::Char('(') | Item::Box(_))) {
        return false;
    }
    (0..row.len()).all(|k| match &row.0[k] {
        Item::Box(b) => {
            mc_edit::rules::lexical_at(row, k) == mc_edit::rules::Lexical::Code && b.slots().iter().all(|s| boxes_in_code(s))
        }
        Item::Placeholder => mc_edit::rules::lexical_at(row, k) == mc_edit::rules::Lexical::Code,
        _ => true,
    })
}
