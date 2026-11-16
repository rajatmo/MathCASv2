//! A box typed after an operator does not take the operator as its operand.
//! A test that still fails is kept ignored with the reason in its `ignore`; removing the `ignore` is the proof of a fix.
mod common;
use common::Old;
use mc_boxes::{to_text, Row};
use mc_edit::{Editor, Structure};

#[test]

fn a_box_after_an_operator_does_not_take_the_operator_as_its_operand() {
    for (typed, structure) in [
        ("a+", Structure::Superscript),
        ("a*", Structure::Superscript),
        ("a-", Structure::Fraction),
        ("a^", Structure::Superscript),
        ("a=", Structure::Fraction),
    ] {
        let mut ed = Editor::new(Row::default());
        ed.insert_text(typed);
        assert!(ed.structure(structure));
        ed.insert_text("2");
        let text = to_text(ed.root());
        let op = typed.chars().last().unwrap();
        assert!(!text.contains(&format!("(({op})")), "{typed:?} + {structure:?}: the operator became an operand: {text}");
    }
}
