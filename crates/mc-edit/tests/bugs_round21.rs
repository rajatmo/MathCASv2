//! Round 21: tests for the typeset-editor bugs found in this round; a test that still fails is kept ignored until
//! its bug is fixed.
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
