//! Round 21: tests for the notebook-model bugs found in this round. A test that still fails is ignored
//! with its id; removing the `ignore` is the proof of a fix.
use mc_nbmodel::{Bracket, Command, FakeEnv, Notebook, Style};

fn nb(text: &str) -> Notebook {
    Notebook::parse(&format!("#! mathcas 2 notebook\n\n{text}")).unwrap()
}

fn texts(nb: &Notebook) -> Vec<String> {
    nb.cells().iter().map(|c| c.text.clone()).collect()
}

#[test]

fn typeset_cell_turned_into_prose_survives_save_and_reopen() {
    for style in [Style::Text, Style::Item, Style::DisplayFormula] {
        let mut n = nb("#% cell typeset\n((a)/(b))\n");
        let mut env = FakeEnv::default();
        let id = n.cells()[0].id;
        n.apply(Command::ClickText(id, 0, false), &mut env);
        n.apply(Command::SetStyle(style), &mut env);
        let saved = n.to_text();
        let reopened = Notebook::parse(&saved).unwrap();
        assert_eq!(reopened.to_text(), saved, "{style:?}: reopening changes the file");
        // the cell must not show the canonical box text `((a)/(b))` as characters (neither before nor after)
        assert!(!reopened.cells()[0].text.contains("(("), "{style:?}: raw box text {:?}", reopened.cells()[0].text);
    }
}

#[test]

fn undo_of_divide_keeps_the_evaluation_label() {
    let mut n = nb("#% cell label=1\na + b\n#% output n=1\n#= 3\n");
    let mut env = FakeEnv::default();
    let before = n.to_text();
    assert!(before.contains("label=1"), "{before}");
    let id = n.cells()[0].id;
    n.apply(Command::ClickText(id, 1, false), &mut env);
    n.apply(Command::Divide, &mut env);
    n.apply(Command::Undo, &mut env);
    assert_eq!(n.to_text(), before);
}

#[test]

fn undo_of_a_style_change_keeps_the_evaluation_label() {
    let mut n = nb("#% cell label=1\na + b\n#% output n=1\n#= 3\n");
    let mut env = FakeEnv::default();
    let before = n.to_text();
    let id = n.cells()[0].id;
    n.apply(Command::ClickText(id, 1, false), &mut env);
    n.apply(Command::SetStyle(Style::Text), &mut env);
    n.apply(Command::Undo, &mut env);
    assert_eq!(n.to_text(), before);
}

#[test]

fn undo_of_a_style_change_keeps_the_output_number() {
    let mut n = nb("#% cell label=1\na + b\n#% output n=1\n#= 3\n");
    let mut env = FakeEnv::default();
    let before = n.to_text();
    let out = n.cells()[1].id;
    n.apply(Command::ClickText(out, 0, false), &mut env);
    n.apply(Command::SetStyle(Style::Text), &mut env);
    n.apply(Command::Undo, &mut env);
    assert_eq!(n.to_text(), before);
}

#[test]

fn a_move_that_changes_nothing_is_not_an_undo_step() {
    let mut n = nb("#% cell\na\n\n#% cell\nb\n");
    let mut env = FakeEnv::default();
    let id = n.cells()[0].id;
    n.apply(Command::ClickText(id, 1, false), &mut env);
    env.time += 1.0;
    n.apply(Command::Insert("X".into()), &mut env);
    env.time += 3.0;
    n.apply(Command::ClickBracket(Bracket::Cell(id), false, false), &mut env);
    n.apply(Command::MoveCells(0, false), &mut env);
    n.apply(Command::Undo, &mut env);
    assert_eq!(texts(&n), ["a", "b"], "one Undo must undo the typing");
}

#[test]

fn a_cell_ending_in_a_line_break_reopens_unchanged() {
    let mut n = nb("#% cell\nab\n");
    let mut env = FakeEnv::default();
    let id = n.cells()[0].id;
    n.apply(Command::ClickText(id, 2, false), &mut env);
    n.apply(Command::Enter, &mut env);
    let open = texts(&n);
    let reopened = Notebook::parse(&n.to_text()).unwrap();
    assert_eq!(texts(&reopened), open);
}

#[test]
fn typeset_cell_with_unreadable_canonical_text_reopens_unchanged() {
    use mc_nbmodel::Command;
    for typed in ["((sqrt(a)", "((a)/(", "(a+((b)^(2))", "((x)^(2)"] {
        let mut n = nb("#% cell typeset\n((a)/(b))\n");
        let mut env = FakeEnv::default();
        let id = n.cells()[0].id;
        n.apply(Command::ClickText(id, 0, false), &mut env);
        n.apply(Command::Insert(typed.into()), &mut env);
        let saved = n.to_text();
        let reopened = Notebook::parse(&saved).unwrap();
        assert_eq!(reopened.to_text(), saved, "{typed:?}");
    }
}
