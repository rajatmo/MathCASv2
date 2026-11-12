//! Math input traps found by the round-32 self-tests (`tests/selftest/input-*.txt`) and fixed in the model crate
//! `mc-nbmodel` (NOTES.md, "Round 32", R32.1 to R32.4): each test is the trap through the model's commands.
use mc_nbmodel::Structure;
use mc_nbmodel::keys::{Chord, Context, KeyTable};
use mc_nbmodel::{Command, FakeEnv, Notebook};

fn notation(nb: &Notebook) -> String {
    match nb.input_editor() {
        Some((_, e)) if mc_boxes::has_boxes(e.root()) => e.notation(),
        _ => "none".to_string(),
    }
}

fn typed(nb: &mut Notebook, env: &mut FakeEnv, cmds: Vec<Command>) {
    for c in cmds {
        nb.apply(c, env);
    }
}

/// R32.1: Redo of the box key that made a plain cell's first box puts the caret back into the new slot. Today the
/// caret lands after the box, so the next character typed goes outside it (`(u^□)*2`).
#[test]
fn redo_of_a_first_box_restores_the_caret_in_its_slot() {
    let mut nb = Notebook::new();
    let mut env = FakeEnv::default();
    typed(&mut nb, &mut env, vec![Command::Insert("u".into()), Command::Structure(Structure::Superscript)]);
    assert_eq!(notation(&nb), "(u^▮□)");
    typed(&mut nb, &mut env, vec![Command::Undo, Command::Redo]);
    assert_eq!(notation(&nb), "(u^▮□)");
}

/// R32.1: Undo of a Backspace that removed the cell's last box (the cell became plain text) puts the caret back where
/// it was inside the box. Today it lands after the box.
#[test]
fn undo_of_an_unwrapping_backspace_restores_the_caret() {
    let mut nb = Notebook::new();
    let mut env = FakeEnv::default();
    typed(&mut nb, &mut env, vec![Command::Insert("x".into()), Command::Structure(Structure::Fraction)]);
    typed(&mut nb, &mut env, vec![Command::Backspace]);
    assert_eq!(notation(&nb), "none");
    typed(&mut nb, &mut env, vec![Command::Undo]);
    assert_eq!(notation(&nb), "(x/▮□)");
}

/// R32.2: An alias finished between automatically closed brackets keeps the closer automatic: the `)` typed next
/// steps over it. Today it is inserted, so `sin(` Esc th Esc `)` gives `sin(θ))`, a syntax error.
#[test]
fn an_alias_keeps_the_automatic_closer() {
    let mut nb = Notebook::new();
    nb.set_auto_close(true);
    let mut env = FakeEnv::default();
    typed(
        &mut nb,
        &mut env,
        vec![
            Command::Insert("n".into()),
            Command::Insert("(".into()),
            Command::Alias,
            Command::Insert("p".into()),
            Command::Alias,
            Command::Insert(")".into()),
        ],
    );
    assert_eq!(nb.cells()[0].text, "n(π)");
}

/// R32.3: `Ctrl+5` at an insertion point makes an Input cell holding an n-th root, as `Ctrl+2` makes a square root.
/// Today it switches to tab 5 and the following digits go into a new plain cell.
#[test]
fn ctrl5_at_an_insertion_point_makes_a_root() {
    let keys = KeyTable::default();
    let chord = Chord::parse("Ctrl+5").expect("a chord");
    assert_eq!(keys.lookup(chord, Context::Gap), Some(&Command::Structure(Structure::Root)));
}

/// Undo after typing over a selection in boxes gives the selection back, Redo the caret after the typed text.
#[test]
fn undo_of_typing_over_a_selection_restores_the_selection() {
    let mut nb = Notebook::new();
    let mut env = FakeEnv::default();
    typed(
        &mut nb,
        &mut env,
        vec![
            Command::Insert("a".into()),
            Command::Structure(Structure::Fraction),
            Command::Insert("b".into()),
            Command::ExitStructure,
            Command::Insert(" + c".into()),
            Command::Move(mc_nbmodel::Motion::Home, false),
            Command::Move(mc_nbmodel::Motion::Right, true),
        ],
    );
    assert_eq!(notation(&nb), "⟦(a/b)⟧ + c");
    typed(&mut nb, &mut env, vec![Command::Insert("q".into())]);
    // the last box went: the cell is plain text
    assert_eq!(notation(&nb), "none");
    assert_eq!(nb.cells()[0].text, "q + c");
    typed(&mut nb, &mut env, vec![Command::Undo]);
    assert_eq!(notation(&nb), "⟦(a/b)⟧ + c");
    typed(&mut nb, &mut env, vec![Command::Redo]);
    assert_eq!(nb.cells()[0].text, "q + c");
}

/// R32.2 in a cell with boxes: the alias between automatic brackets keeps the closer automatic there too.
#[test]
fn an_alias_in_boxes_keeps_the_automatic_closer() {
    let mut nb = Notebook::new();
    nb.set_auto_close(true);
    let mut env = FakeEnv::default();
    let mut cmds = vec![Command::Insert("x".into()), Command::Structure(Structure::Superscript), Command::Insert("2".into())];
    cmds.push(Command::ExitStructure);
    for t in [" ", "+", " ", "s", "i", "n", "("] {
        cmds.push(Command::Insert(t.into()));
    }
    cmds.extend([Command::Alias, Command::Insert("t".into()), Command::Insert("h".into()), Command::Alias]);
    cmds.push(Command::Insert(")".into()));
    typed(&mut nb, &mut env, cmds);
    assert_eq!(notation(&nb), "(x^2) + sin(θ)▮");
}

/// R32.4: Home or End in a slot goes to the slot's edge; pressed again there, to the line's edge (design 3.1).
#[test]
fn home_and_end_twice_reach_the_line_edges() {
    let mut nb = Notebook::new();
    let mut env = FakeEnv::default();
    typed(
        &mut nb,
        &mut env,
        vec![
            Command::Insert("a".into()),
            Command::Insert(" ".into()),
            Command::Insert("+".into()),
            Command::Insert(" ".into()),
            Command::Insert("x".into()),
            Command::Structure(Structure::Superscript),
            Command::Insert("2".into()),
            Command::Insert("n".into()),
        ],
    );
    assert_eq!(notation(&nb), "a + (x^2n▮)");
    typed(&mut nb, &mut env, vec![Command::Move(mc_nbmodel::Motion::Home, false)]);
    assert_eq!(notation(&nb), "a + (x^▮2n)");
    typed(&mut nb, &mut env, vec![Command::Move(mc_nbmodel::Motion::Home, false)]);
    assert_eq!(notation(&nb), "▮a + (x^2n)");
    typed(&mut nb, &mut env, vec![Command::Move(mc_nbmodel::Motion::Right, false)]);
    typed(&mut nb, &mut env, vec![Command::Move(mc_nbmodel::Motion::End, false)]);
    assert_eq!(notation(&nb), "a + (x^2n)▮");
}
