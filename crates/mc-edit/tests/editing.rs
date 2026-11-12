//! The structure-command vectors of design 08, section 4.2, the placeholder vectors of 4.3, and the editor's
//! other commands. The notation is [`mc_boxes::notation`]'s: `(N/D)`, `(B^E)`, `(B_S)`, `√(X)`, `(B over O)`,
//! `grid[[a,b],[c,d]]`, `□`; a superscript box is `(x^2)` (the design's tables write it `x^2`). The editor's own
//! notation adds `▮` for the caret and `⟦ ⟧` around a selection.
mod common;
use common::{AliasOutcome, Old};
use mc_boxes::{Item, Node, Row, Step};
use mc_edit::{Editor, Structure};

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
    let cols = [Fraction, Superscript, Radical];
    let table: &[(&str, [&str; 3])] = &[
        ("1+x", ["1+(x/Q)Z", "1+(x^Q)Z", "1+x*√(Q)Z"]),
        ("f(x)", ["(f(x)/Q)Z", "(f(x)^Q)Z", "f(x)*√(Q)Z"]),
        ("a*b", ["a*(b/Q)Z", "a*(b^Q)Z", "a*b*√(Q)Z"]),
        ("(a+b)", ["((a+b)/Q)Z", "((a+b)^Q)Z", "(a+b)*√(Q)Z"]),
        ("[a, b]", ["([a, b]/Q)Z", "([a, b]^Q)Z", "[a, b]*√(Q)Z"]),
        // a power nests
        ("x^2 (box)", ["((x^2)/Q)Z", "((x^2)^Q)Z", "(x^2)*√(Q)Z"]),
        ("", ["(Q/□)Z", "(Q^□)Z", "√(Q)Z"]),
        ("x+", ["x+(Q/□)Z", "x+(Q^□)Z", "x+√(Q)Z"]),
    ];
    let mut n = 0;
    for (text, expected) in table {
        for (s, want) in cols.iter().zip(expected) {
            assert_eq!(run(text, *s), *want, "{text:?} {s:?}");
            n += 1;
        }
    }
    assert_eq!(n, 24);
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
        // round 35: after a space the box needs no `*` (the space multiplies)
        ("x ", "x (Q^□)Z"),
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
    // spaces at the edges of the selection stay outside the box (round 32)
    let mut ed = Editor::from_text("a + b + c");
    ed.select(&[], 3..9);
    ed.structure(Structure::Radical);
    assert_eq!(ed.notation(), "a + √(b + c▮)");
    let mut ed = Editor::from_text("a +  b ");
    ed.select(&[], 3..7);
    ed.structure(Structure::Fraction);
    assert_eq!(ed.notation(), "a +  (b/▮□) ");
    // a selection of spaces only is taken as it is
    let mut ed = Editor::from_text("a  b");
    ed.select(&[], 1..3);
    ed.structure(Structure::Radical);
    assert_eq!(ed.notation(), "a*√(  ▮)*b");
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
    // at the start of the exponent the box goes, its contents stay as linear text
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "x^▮");
    // a box whose slots are all empty goes as a whole
    let mut ed = Editor::new(Row::new());
    ed.structure(Structure::Fraction);
    assert_eq!(ed.notation(), "(▮□/□)");
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "▮");
    assert!(!ed.backspace());
    // Backspace at the start of a denominator unwraps the fraction into `num/den`, the caret after the `/`
    let mut ed = Editor::from_text("((a+1)/(b))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 1 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "(a+1)/▮b");
    // at the start of a numerator: the caret where the box began
    let mut ed = Editor::from_text("2*((a)/(b+c))");
    ed.set_caret(&[mc_boxes::Step { item: 2, slot: 0 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "2*▮a/(b+c)");
    // Delete at the end of a numerator: the caret before the `/`
    let mut ed = Editor::from_text("((a)/(b))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 0 }], 1);
    assert!(ed.delete());
    assert_eq!(ed.notation(), "a▮/b");
    // a square root leaves its radicand; a power with a compound base keeps it in parentheses
    let mut ed = Editor::from_text("((sqrt(x+1)))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 0 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "▮x+1");
    let mut ed = Editor::from_text("((x+1)^(2))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 1 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "(x+1)^▮2");
    // a fraction in a denominator is parenthesised when unwrapped
    let mut ed = Editor::from_text("((a)/(((b)/(c))))");
    ed.set_caret(&[mc_boxes::Step { item: 0, slot: 1 }], 0);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "a/▮((b/c))");
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
    // operator pairs are drawn as their two characters in cells (NBUI.3), so the caret steps over each and
    // Backspace removes one (round 34: the same in every row, as in cells without boxes)
    let mut ed = Editor::from_text("a->b");
    ed.set_caret(&[], 4);
    ed.move_left(false);
    ed.move_left(false);
    assert_eq!(ed.notation(), "a-▮>b");
    ed.set_caret(&[], 3);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "a-▮b");
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
    // a template of one of the three boxes puts the caret in its marked placeholder
    assert_eq!(ed.escape(), AliasOutcome::Started);
    ed.insert_text("fract");
    assert_eq!(ed.escape(), AliasOutcome::Replaced);
    assert_eq!(ed.notation(), "x = α*(⟦□⟧/□)");
    ed.insert_char('f');
    ed.next_placeholder();
    ed.insert_char('t');
    assert_eq!(ed.text(), "x = α*((f)/(t))");
    // other templates (integrals, sums, matrices) are unknown in a cell
    let mut ed = Editor::new(Row::new());
    ed.escape();
    ed.insert_text("intt");
    assert_eq!(ed.escape(), AliasOutcome::Unknown);
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
    // round 35: a space multiplies, so a box and an operand apart need no `*`
    assert_eq!(typed("", Structure::Radical, " y"), "√(Q) y▮");
    assert_eq!(typed("x ", Structure::Radical, ""), "x √(Q)▮");
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

#[test]
fn an_alias_after_a_box_keeps_its_name_after_the_implicit_times() {
    let mut e = Editor::from_text("((sqrt(x)))");
    assert_eq!(e.escape(), AliasOutcome::Started);
    e.insert_text("alpha");
    assert_eq!(e.escape(), AliasOutcome::Replaced);
    assert_eq!(e.notation(), "√(x)*α▮");
}

#[test]
fn extending_keeps_the_anchor() {
    let start = || {
        let mut ed = Editor::from_text("a+((xy)/(z))+b");
        ed.set_caret(&[Step { item: 2, slot: 0 }], 1);
        ed
    };
    let mut ed = start();
    assert_eq!(ed.notation(), "a+(x▮y/z)+b");
    let steps = [
        (true, "a+(x⟦y⟧/z)+b"),
        (true, "a+⟦(xy/z)⟧+b"),
        (true, "a+⟦(xy/z)+⟧b"),
        (false, "a+⟦(xy/z)⟧+b"),
        // back into the box that holds the anchor: the selection starts from the anchor again
        (false, "a+(x⟦y⟧/z)+b"),
        (false, "a+(x▮y/z)+b"),
        (false, "a+(⟦x⟧y/z)+b"),
        (false, "a+⟦(xy/z)⟧+b"),
        (false, "a⟦+(xy/z)⟧+b"),
    ];
    for (forward, want) in steps {
        assert!(if forward { ed.move_right(true) } else { ed.move_left(true) });
        assert_eq!(ed.notation(), want);
    }
    // Home and End keep the anchor too
    ed.move_end(true);
    assert_eq!(ed.notation(), "a+⟦(xy/z)+b⟧");
    // a plain arrow collapses the selection at its end
    assert!(ed.move_right(false));
    assert_eq!(ed.notation(), "a+(xy/z)+b▮");

    // Shift+click from outside into a slot selects up to the whole box; editing acts on what is shown
    let mut ed = Editor::from_text("a+((xy)/(z))+b");
    ed.set_caret(&[], 0);
    ed.extend_to(&[Step { item: 2, slot: 0 }], 1);
    assert_eq!(ed.notation(), "⟦a+(xy/z)⟧+b");
    assert_eq!(ed.selected_text(), "a+((xy)/(z))");
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "▮+b");

    // a selection across two slots of one box selects the box
    let mut ed = start();
    ed.extend_to(&[Step { item: 2, slot: 1 }], 1);
    assert_eq!(ed.notation(), "a+⟦(xy/z)⟧+b");
    // typing replaces it
    ed.insert_char('w');
    assert_eq!(ed.notation(), "a+w▮+b");
}

/// Round 33: a slot whose brackets do not match (the state between typing `(` and `)`) reads back as the same tree,
/// so what is drawn and where the caret is stay what the editor has.
#[test]
fn slots_with_unmatched_brackets_read_back() {
    let mut ed = Editor::new(Row::boxed(Node::Fraction { num: Row::text("2m"), den: Row::text("k") }));
    ed.set_caret(&[Step { item: 0, slot: 0 }], 0);
    ed.insert_char('(');
    let text = ed.text();
    assert_eq!(mc_boxes::from_text(&text), *ed.root());
    assert_eq!(ed.notation(), "((▮2m/k)");
    // the kernel reads the text without the escapes: an unmatched bracket, as typed
    assert_eq!(mc_boxes::source_text(&text), "(((2m)/(k))");
    let ph = || Row::text("y");
    for slot in ["(", ")", "(]", "a)(b", "\"x", "a # (b", "[(1, 2", "`n", "f(x", "x) + (y]"] {
        let s = || Row::text(slot);
        let nodes = [
            Node::Fraction { num: s(), den: ph() },
            Node::Fraction { num: ph(), den: s() },
            Node::Superscript { base: s(), exp: ph() },
            Node::Superscript { base: ph(), exp: s() },
            Node::Subscript { base: ph(), index: s() },
            Node::Radical { radicand: s(), index: None },
            Node::Radical { radicand: s(), index: Some(s()) },
            // nested: the inner box's escapes are part of the outer slot's text
            Node::Fraction { num: Row::boxed(Node::Radical { radicand: s(), index: None }), den: s() },
        ];
        for node in nodes {
            let mut row = Row::text("a + (");
            row.0.push(Item::Box(Box::new(node)));
            row.0.extend(Row::text(" + 1").0);
            let text = mc_boxes::to_text(&row);
            assert_eq!(mc_boxes::from_text(&text), row, "{slot:?}: {text}");
            let (mapped, map) = mc_boxes::to_text_mapped(&row);
            assert_eq!(mapped, text, "{slot:?}");
            // every character of the top row is where the map says
            for (i, it) in row.0.iter().enumerate() {
                if let Item::Char(c) = it {
                    assert!(mapped[map.offset(&[], i).unwrap()..].starts_with(*c), "{slot:?} item {i}");
                }
            }
        }
    }
    // balanced text has no escapes: files written before are read and written as they were
    let plain = "((((a + b))^(2))/((sqrt(f(x)))))";
    assert_eq!(mc_boxes::to_text(&mc_boxes::from_text(plain)), plain);
    assert!(!plain.contains(mc_boxes::ESCAPE));
}

/// Round 33: the base of a power that is not one operand is drawn in parentheses (`(2m)²`, not `2m²`).
#[test]
fn compound_bases() {
    use mc_boxes::ops::compound_base;
    for (base, compound) in [
        ("x", false),
        ("x1", false),
        ("12.5", false),
        ("f(x)", false),
        ("(a + b)", false),
        ("[1, 2]", false),
        ("f'", false),
        ("\"s\"", false),
        ("2m", true),
        ("-x", true),
        ("a + b", true),
        ("a b", true),
        ("(a)(b)", true),
        ("2(x)", true),
    ] {
        assert_eq!(compound_base(&Row::text(base)), compound, "{base}");
    }
    let frac = Row::boxed(Node::Fraction { num: Row::text("a"), den: Row::text("b") });
    let power = Row::boxed(Node::Superscript { base: Row::text("x"), exp: Row::text("2") });
    let root = Row::boxed(Node::Radical { radicand: Row::text("x"), index: None });
    let index = Row::boxed(Node::Subscript { base: Row::text("x"), index: Row::text("i") });
    assert!(compound_base(&frac) && compound_base(&power));
    assert!(!compound_base(&root) && !compound_base(&index) && !compound_base(&Row::placeholder()));
}

/// Round 33: Backspace in an empty pair of brackets removes both; the power key on the whole inside of a bracket
/// pair takes the brackets as the base.
#[test]
fn brackets_pair_up() {
    let mut ed = Editor::new(Row::text("g + ()"));
    ed.set_caret(&[], 5);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "g + ▮");
    let mut ed = Editor::new(Row::text("(a)"));
    ed.set_caret(&[], 1);
    assert!(ed.backspace());
    assert_eq!(ed.notation(), "▮a)", "a bracket with something in it goes alone");
    let mut ed = Editor::new(Row::text("y = (a + b)"));
    ed.select(&[], 5..10);
    assert!(ed.structure(Structure::Superscript));
    ed.insert_char('2');
    assert_eq!(ed.linear(), "y = (a + b)^2");
    assert_eq!(ed.notation(), "y = ((a + b)^2▮)");
    // a fraction keeps what was selected
    let mut ed = Editor::new(Row::text("(a + b)"));
    ed.select(&[], 1..6);
    assert!(ed.structure(Structure::Fraction));
    assert_eq!(ed.notation(), "((a + b/▮□))");
}
