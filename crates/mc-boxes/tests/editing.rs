//! The structure-command vectors of design 08, section 4.2, the placeholder vectors of 4.3, and the editor's
//! other commands. The notation is [`mc_boxes::notation`]'s: `(N/D)`, `(B^E)`, `(B_S)`, `√(X)`, `(B over O)`,
//! `grid[[a,b],[c,d]]`, `□`; a superscript box is `(x^2)` (the design's tables write it `x^2`). The editor's own
//! notation adds `▮` for the caret and `⟦ ⟧` around a selection.
use mc_boxes::{AliasOutcome, Editor, Item, Node, Row, Structure};

/// The start of each table row: its text, or a superscript box `x^2`.
fn start(text: &str) -> Editor {
    if text == "x^2 (box)" {
        return Editor::new(Row::boxed(Node::Superscript { base: Row::text("x"), exp: Row::text("2") }));
    }
    Editor::new(Row::text(text))
}

/// A structure command at the end of `text`, then `Q`, `Ctrl+Space`; `Z` marks where the caret is then (the
/// design's tables type a `Z` there; typing it would now put an implicit `*` before it, see `implicit_times`).
fn run(text: &str, s: Structure) -> String {
    let mut ed = start(text);
    assert!(ed.structure(s));
    ed.insert_char('Q');
    assert!(ed.exit_structure());
    ed.notation().replace('▮', "Z")
}

#[test]
fn structure_table() {
    use Structure::*;
    let cols = [Fraction, Superscript, Subscript, Radical, GridRow, GridColumn];
    let table: &[(&str, [&str; 6])] = &[
        ("1+x", ["1+(x/Q)Z", "1+(x^Q)Z", "1+(x_Q)Z", "1+x*√(Q)Z", "1+grid[[x],[Q]]Z", "1+grid[[x,Q]]Z"]),
        ("f(x)", ["(f(x)/Q)Z", "(f(x)^Q)Z", "(f(x)_Q)Z", "f(x)*√(Q)Z", "grid[[f(x)],[Q]]Z", "grid[[f(x),Q]]Z"]),
        ("a*b", ["a*(b/Q)Z", "a*(b^Q)Z", "a*(b_Q)Z", "a*b*√(Q)Z", "a*grid[[b],[Q]]Z", "a*grid[[b,Q]]Z"]),
        ("(a+b)", ["((a+b)/Q)Z", "((a+b)^Q)Z", "((a+b)_Q)Z", "(a+b)*√(Q)Z", "grid[[(a+b)],[Q]]Z", "grid[[(a+b),Q]]Z"]),
        ("[a, b]", ["([a, b]/Q)Z", "([a, b]^Q)Z", "([a, b]_Q)Z", "[a, b]*√(Q)Z", "grid[[[a, b]],[Q]]Z", "grid[[[a, b],Q]]Z"]),
        // a subscript on a superscript box wraps the base in parentheses; a superscript nests
        ("x^2 (box)", ["((x^2)/Q)Z", "((x^2)^Q)Z", "(((x^2))_Q)Z", "(x^2)*√(Q)Z", "grid[[(x^2)],[Q]]Z", "grid[[(x^2),Q]]Z"]),
        ("", ["(Q/□)Z", "(Q^□)Z", "(Q_□)Z", "√(Q)Z", "grid[[□],[Q]]Z", "grid[[□,Q]]Z"]),
        ("x+", ["x+(Q/□)Z", "x*(+^Q)Z", "x*(+_Q)Z", "x+√(Q)Z", "grid[[x+],[Q]]Z", "grid[[x+,Q]]Z"]),
    ];
    let mut n = 0;
    for (text, expected) in table {
        for (s, want) in cols.iter().zip(expected) {
            assert_eq!(run(text, *s), *want, "{text:?} {s:?}");
            n += 1;
        }
    }
    assert_eq!(n, 48);
}

#[test]
fn over_and_under_take_what_superscript_takes() {
    for (s, word) in [(Structure::Overscript, "over"), (Structure::Underscript, "under")] {
        assert_eq!(run("1+x", s), format!("1+(x {word} Q)Z"));
        assert_eq!(run("f(x)", s), format!("(f(x) {word} Q)Z"));
        assert_eq!(run("", s), format!("(Q {word} □)Z"));
        assert_eq!(run("x+", s), format!("x*(+ {word} Q)Z"));
    }
}

#[test]
fn operands_are_primaries() {
    // juxtaposed number and name: the name; a number with a fraction part; primes; strings; indexing
    for (text, want) in [
        ("2x", "2(x^Q)Z"),
        ("x1", "(x1^Q)Z"),
        ("1.5", "(1.5^Q)Z"),
        ("f''", "(f''^Q)Z"),
        ("a + \"s\"", "a + (\"s\"^Q)Z"),
        ("m[1]", "(m[1]^Q)Z"),
        ("x ", "x *(Q^□)Z"),
    ] {
        assert_eq!(run(text, Structure::Superscript), want, "{text}");
    }
}

#[test]
fn selection_becomes_the_first_slot() {
    let mut ed = Editor::from_text("1+x");
    ed.select(&[], 0..3);
    assert_eq!(ed.notation(), "⟦1+x⟧");
    ed.structure(Structure::Fraction);
    ed.insert_char('Q');
    assert_eq!(ed.notation(), "(1+x/Q▮)");
    // a radical over a selection keeps the caret at the end of the radicand
    let mut ed = Editor::from_text("ab");
    ed.select(&[], 0..2);
    ed.structure(Structure::Radical);
    assert_eq!(ed.notation(), "√(ab▮)");
}

#[test]
fn tab_moves_through_placeholders() {
    // Ctrl+/, A, Tab, B gives A/B
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::Fraction);
    ed.insert_char('A');
    assert!(ed.next_placeholder());
    assert_eq!(ed.notation(), "(A/⟦□⟧)");
    ed.insert_char('B');
    assert_eq!(ed.notation(), "(A/B▮)");
    assert!(!ed.next_placeholder());
    // Ctrl+/, Tab, B gives □/B
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::Fraction);
    assert!(ed.next_placeholder());
    ed.insert_char('B');
    assert_eq!(ed.notation(), "(□/B▮)");
    // Shift+Tab goes back
    assert!(ed.prev_placeholder());
    assert_eq!(ed.notation(), "(⟦□⟧/B)");
    assert!(!ed.prev_placeholder());
    // a template's placeholders in reading order
    let mut ed = Editor::new(Row::new());
    assert!(ed.insert_template("((sum(□, [▮, □, □])))"));
    assert_eq!(ed.notation(), "∑[⟦□⟧=□..□](□)");
    for _ in 0..3 {
        assert!(ed.next_placeholder());
    }
    assert_eq!(ed.notation(), "∑[□=□..□](⟦□⟧)");
    assert!(!ed.next_placeholder());
}

#[test]
fn ctrl_space_and_ctrl_5() {
    let mut ed = Editor::from_text("x");
    assert!(!ed.exit_structure());
    ed.structure(Structure::Subscript);
    ed.insert_char('i');
    // Ctrl+5 from a subscript makes the superscript position
    assert!(ed.switch_script());
    ed.insert_char('2');
    assert_eq!(ed.notation(), "((x_i)^2▮)");
    // and back to the subscript
    assert!(ed.switch_script());
    assert_eq!(ed.notation(), "((x_i▮)^2)");
    assert_eq!(ed.text(), "((((subscript(x, i))))^(2))");
    // from a plain superscript, Ctrl+5 creates the subscript
    let mut ed = Editor::from_text("y");
    ed.structure(Structure::Superscript);
    ed.insert_char('n');
    assert!(ed.switch_script());
    assert_eq!(ed.notation(), "((y_⟦□⟧)^n)");
    // with no script slot to switch to, nothing happens
    let mut ed = Editor::from_text("a");
    ed.structure(Structure::Fraction);
    let before = ed.notation();
    assert!(!ed.switch_script());
    assert_eq!(ed.notation(), before);
    // under to over, radical index
    let mut ed = Editor::from_text("S");
    ed.structure(Structure::Underscript);
    ed.insert_char('a');
    assert!(ed.switch_script());
    ed.insert_char('b');
    assert_eq!(ed.notation(), "(S under a over b▮)");
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::Radical);
    ed.insert_char('x');
    assert!(ed.switch_script());
    ed.insert_char('3');
    assert_eq!(ed.notation(), "√[3▮](x)");
}

#[test]
fn grids_grow_inside() {
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::GridRow);
    ed.insert_char('a');
    ed.structure(Structure::GridRow);
    assert_eq!(ed.notation(), "grid[[□],[a],[⟦□⟧]]");
    ed.insert_char('b');
    ed.structure(Structure::GridColumn);
    ed.insert_char('c');
    assert_eq!(ed.notation(), "grid[[□,□],[a,□],[b,c▮]]");
    assert_eq!(ed.text(), "(([[□, □], [a, □], [b, c]]))");
    // up and down move between grid rows
    assert!(ed.move_up());
    assert_eq!(ed.notation(), "grid[[□,□],[a,□▮],[b,c]]");
    assert!(ed.move_down());
    assert!(!ed.move_down());
}

#[test]
fn typing_replaces_placeholders_and_backspace_unwinds() {
    let mut ed = Editor::from_text("x");
    ed.structure(Structure::Superscript);
    ed.insert_text("12");
    assert_eq!(ed.notation(), "(x^12▮)");
    assert!(ed.exit_structure());
    // Backspace after a box enters it, then deletes inside
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "(x^12▮)");
    assert!(ed.backspace());
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "(x^▮□)");
    // at the start of the exponent the structure goes, its contents stay
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "x▮");
    // a box whose slots are all empty goes as a whole
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::Fraction);
    assert_eq!(ed.notation(), "(▮□/□)");
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "▮");
    assert!(!ed.backspace());
    // dissolving a fraction from its denominator keeps both parts
    let mut ed = Editor::from_text("((a)/(b))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 1 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "a▮b");
    // Delete at the end of a numerator does the same
    let mut ed = Editor::from_text("((a)/(b))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 0 }], 1);
    assert!(ed.delete());
    assert_eq!(ed.notation(), "a▮b");
}

#[test]
fn arrows_walk_through_boxes() {
    let mut ed = Editor::from_text("1+((x)/(y))");
    ed.set_caret(&[], 0);
    let mut seen = vec![ed.notation()];
    while ed.move_right(false) {
        seen.push(ed.notation());
    }
    assert_eq!(seen, ["▮1+(x/y)", "1▮+(x/y)", "1+▮(x/y)", "1+(▮x/y)", "1+(x▮/y)", "1+(x/▮y)", "1+(x/y▮)", "1+(x/y)▮"]);
    let mut back = vec![ed.notation()];
    while ed.move_left(false) {
        back.push(ed.notation());
    }
    seen.reverse();
    assert_eq!(back, seen);
    // operator pairs are one step and one Backspace
    let mut ed = Editor::from_text("a->b");
    ed.set_caret(&[], 4);
    ed.move_left(false);
    ed.move_left(false);
    assert_eq!(ed.notation(), "a▮->b");
    ed.set_caret(&[], 3);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "a▮b");
    // extending out of a slot selects the whole box
    let mut ed = Editor::from_text("1+((x)/(y))");
    ed.set_caret(&[mc_boxes::Step { item: 2, slot: 0 }], 1);
    assert!(ed.move_right(true));
    assert_eq!(ed.notation(), "1+⟦(x/y)⟧");
    assert_eq!(ed.selected_text(), "((x)/(y))");
    // Ctrl+. grows outwards
    let mut ed = Editor::from_text("1+((xy)/(z))");
    ed.set_caret(&[mc_boxes::Step { item: 2, slot: 0 }], 1);
    assert!(ed.expand_selection());
    assert_eq!(ed.notation(), "1+(⟦xy⟧/z)");
    assert!(ed.expand_selection());
    assert_eq!(ed.notation(), "1+⟦(xy/z)⟧");
    assert!(ed.expand_selection());
    assert_eq!(ed.notation(), "⟦1+(xy/z)⟧");
    assert!(!ed.expand_selection());
}

#[test]
fn aliases_and_templates() {
    let mut ed = Editor::from_text("x = ");
    assert_eq!(ed.escape(), AliasOutcome::Started);
    ed.insert_text("a");
    assert!(ed.alias_mark().is_some());
    assert_eq!(ed.escape(), AliasOutcome::Replaced);
    assert_eq!(ed.notation(), "x = α▮");
    assert!(ed.alias_mark().is_none());
    // a template puts the caret in its marked placeholder
    assert_eq!(ed.escape(), AliasOutcome::Started);
    ed.insert_text("intt");
    assert_eq!(ed.escape(), AliasOutcome::Replaced);
    assert_eq!(ed.notation(), "x = α*∫[□](⟦□⟧)");
    ed.insert_char('f');
    ed.next_placeholder();
    ed.insert_char('t');
    assert_eq!(ed.text(), "x = α*((integrate(f, t)))");
    // an unknown alias stays, marked, until edited
    let mut ed = Editor::new(Row::new());
    ed.escape();
    ed.insert_text("zzz");
    assert_eq!(ed.escape(), AliasOutcome::Unknown);
    assert!(ed.alias_mark().is_some_and(|m| m.unknown && m.start == 0 && m.end == 3));
    ed.insert_char('!');
    assert!(ed.alias_mark().is_none());
    assert_eq!(ed.notation(), "zzz!▮");
    // Backspace right after the start marker removes it; Esc Esc drops it
    let mut ed = Editor::from_text("y");
    ed.escape();
    assert!(ed.backspace());
    assert!(ed.alias_mark().is_none());
    assert_eq!(ed.notation(), "y▮");
    ed.escape();
    assert_eq!(ed.escape(), AliasOutcome::Cancelled);
    // moving out of an unfinished alias leaves its text plain
    let mut ed = Editor::new(Row::new());
    ed.escape();
    ed.insert_text("ab");
    ed.move_left(false);
    ed.move_left(false);
    ed.move_left(false);
    assert!(ed.alias_mark().is_none() || ed.caret().offset == 0);
    ed.set_caret(&[], 2);
    assert_eq!(ed.notation(), "ab▮");
}

#[test]
fn paste_cut_and_text_round_trip() {
    let mut ed = Editor::from_text("a + ");
    assert!(ed.paste("((sqrt(x)))"));
    assert_eq!(ed.notation(), "a + √(x)▮");
    ed.select(&[], 0..1);
    assert_eq!(ed.cut(), "a");
    assert_eq!(ed.text(), " + ((sqrt(x)))");
    assert_eq!(ed.linear(), " + sqrt(x)");
    let t = ed.text();
    assert_eq!(Editor::from_text(&t).root(), ed.root());
    assert!(matches!(ed.root().0.last(), Some(Item::Box(_))));
}

/// Implicit multiplication: a `*` goes where a box would touch an operand (a name, a closing bracket, a prime, a
/// string, a box; after a box also a number or `(`), so the text never reads as a call or fails to read; a number
/// before a box is left alone (`2√2` already multiplies), and the caret stays right after what was typed.
#[test]
fn implicit_times_between_boxes_and_operands() {
    let typed = |text: &str, s: Structure, after: &str| {
        let mut ed = Editor::from_text(text);
        assert!(ed.structure(s));
        ed.insert_char('Q');
        assert!(ed.exit_structure());
        ed.insert_text(after);
        ed.notation()
    };
    assert_eq!(typed("x", Structure::Radical, ""), "x*√(Q)▮");
    assert_eq!(typed("2", Structure::Radical, ""), "2√(Q)▮");
    assert_eq!(typed("x2", Structure::Radical, ""), "x2*√(Q)▮");
    assert_eq!(typed("f(a)", Structure::Radical, ""), "f(a)*√(Q)▮");
    assert_eq!(typed("x+", Structure::Radical, ""), "x+√(Q)▮");
    assert_eq!(typed("", Structure::Radical, "y"), "√(Q)*y▮");
    assert_eq!(typed("", Structure::Radical, "(y)"), "√(Q)*(y)▮");
    assert_eq!(typed("", Structure::Radical, "2"), "√(Q)*2▮");
    assert_eq!(typed("", Structure::Radical, "+y"), "√(Q)+y▮");
    assert_eq!(typed("", Structure::Radical, "[1]"), "√(Q)[1]▮");
    assert_eq!(typed("", Structure::Radical, " y"), "√(Q)* y▮");
    // a box after a box
    let mut ed = Editor::from_text("((sqrt(a)))");
    assert!(ed.structure(Structure::Radical));
    assert_eq!(ed.notation(), "√(a)*√(▮□)");
    // typing in front of a box: the `*` follows the caret
    let mut ed = Editor::from_text("((sqrt(a)))");
    ed.move_home(false);
    ed.insert_char('k');
    assert_eq!(ed.notation(), "k▮*√(a)");
    ed.insert_char('2');
    assert_eq!(ed.notation(), "k2▮*√(a)");
    // the text reads as a product
    assert_eq!(ed.text(), "k2*((sqrt(a)))");
}
