//! Random editing of typeset input through the model (round 19): box keys (power, fraction, square and n-th
//! root), typing, Backspace and Delete, arrows with and without Shift, Home/End, leaving a box, cut, copy and paste.
//! After every command: the cell's text is canonical box text that reads back to itself, the selection is valid,
//! no character typed into the cell is lost except by a deletion, and when the command changed the cell, Undo gives
//! back exactly the text before it and Redo exactly the text after it. At the end, undoing everything gives the
//! empty cell. `MC_FUZZ_CASES` sets the number of sequences (default 300). Sequences that leave the cell are put back in it.
use mc_nbmodel::Structure;
use mc_nbmodel::{Cell, CellId, Command, FakeEnv, Motion, Notebook, Selection, Style};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn command(r: &mut Rng) -> Command {
    use Structure::*;
    match r.below(20) {
        0..=4 => Command::Insert(["x", "2", "+", "a", "(", ")", "n", " ", "y", "*"][r.below(10)].to_string()),
        5..=7 => Command::Structure([Fraction, Superscript, Radical, Root][r.below(4)]),
        8 => Command::Backspace,
        9 => Command::Delete,
        10 => Command::Move(Motion::Left, r.below(3) == 0),
        11 => Command::Move(Motion::Right, r.below(3) == 0),
        12 => Command::Move(if r.below(2) == 0 { Motion::Home } else { Motion::End }, r.below(4) == 0),
        13 => Command::Move(if r.below(2) == 0 { Motion::Up } else { Motion::Down }, false),
        14 => Command::ExitStructure,
        15 => Command::Copy,
        16 => Command::Cut,
        17 => Command::Paste,
        18 => Command::ExtendSelection,
        _ => Command::Insert("+".to_string()),
    }
}

fn text(nb: &Notebook) -> String {
    nb.cells().first().map_or(String::new(), |c| c.text.clone())
}

#[test]
fn random_typeset_editing_keeps_text_and_undo_exact() {
    let cases: usize = std::env::var("MC_FUZZ_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(300);
    let mut r = Rng(0x5eed_1919_abcd_0001);
    let (mut commands, mut undone) = (0, 0);
    for case in 0..cases {
        let mut nb = Notebook::with_cells(vec![Cell::new(CellId(0), Style::Input, "")]);
        let id = nb.cells()[0].id;
        let mut env = FakeEnv::default();
        nb.apply(Command::SelectText(id, 0, 0), &mut env);
        let mut log: Vec<String> = vec![];
        for step in 0..30 {
            // every command its own undo unit (a typing pause between them)
            env.time += 10.0;
            let cmd = command(&mut r);
            log.push(format!("{cmd:?}"));
            let before = text(&nb);
            nb.apply(cmd.clone(), &mut env);
            commands += 1;
            let after = text(&nb);
            let ctx = || format!("case {case} step {step}: {}\n{before:?} -> {after:?}", log.join(" "));
            // the cell's text reads back to itself
            if nb.cells()[0].props.typeset {
                assert_eq!(mc_boxes::to_text(&mc_boxes::from_text(&after)), after, "{}", ctx());
            }
            // the selection names the cell, on character boundaries
            match nb.selection() {
                Selection::Caret { cell, anchor, pos } => {
                    assert_eq!(*cell, id, "{}", ctx());
                    assert!(after.is_char_boundary(*anchor) && after.is_char_boundary(*pos), "{}", ctx());
                }
                // arrows past the cell's edges go to the insertion points around it, Ctrl+. grows to the cell: back in
                Selection::Gap(_) | Selection::Cells { .. } => {
                    nb.apply(Command::SelectText(id, after.len(), after.len()), &mut env);
                }
            }
            // nothing typed goes missing unless the command deletes
            let deletes = matches!(cmd, Command::Backspace | Command::Delete | Command::Cut)
                || matches!(cmd, Command::Insert(_) | Command::Paste | Command::Structure(_));
            if !deletes {
                let count = |s: &str| s.chars().filter(|c| c.is_alphanumeric()).count();
                let letters = |s: &str| {
                    let row = mc_boxes::from_text(s);
                    count(&mc_boxes::linear_text(&row).replace("sqrt", "").replace("surd", ""))
                };
                assert!(letters(&after) >= letters(&before), "{}: characters lost", ctx());
            }
            if after != before {
                let sel = nb.selection().clone();
                env.time += 10.0;
                nb.apply(Command::Undo, &mut env);
                assert_eq!(text(&nb), before, "undo of {cmd:?}: {}", ctx());
                env.time += 10.0;
                nb.apply(Command::Redo, &mut env);
                assert_eq!(text(&nb), after, "redo of {cmd:?}: {}", ctx());
                // undo and redo keep working from where the command left off
                nb.apply(Command::SelectText(id, 0, 0), &mut env);
                if let Selection::Caret { anchor, pos, .. } = sel {
                    nb.apply(Command::SelectText(id, anchor.min(after.len()), pos.min(after.len())), &mut env);
                }
                undone += 1;
            }
        }
        // undoing everything gives the empty cell back
        let mut n = 0;
        while !nb.apply(Command::Undo, &mut env).beep {
            n += 1;
            assert!(n < 1000, "case {case}: undo never ends");
        }
        assert_eq!(text(&nb), "", "case {case}: undo all: {}", log.join(" "));
    }
    println!("typeset editing: {cases} sequences, {commands} commands, {undone} undone and redone exactly");
}
