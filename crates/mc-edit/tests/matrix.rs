//! The typeset-math test matrix (round 19): every construct the cell editor makes (power, fraction, square
//! root, n-th root, and nested combinations) against every editing operation: entering it with the box keys,
//! walking the caret through every slot and back, Home/End in every slot, selecting whole boxes and every slot,
//! Backspace into a box from outside, Backspace/Delete at every slot edge (unwrapping keeps the term for fractions
//! and powers; a root loses its index, then its sign), erasing everything, typing an operator after the box, copying
//! the box and pasting it into plain text and into a slot of another box, and the round trip box -> linear text ->
//! `typeset_input`. Terms are compared as read by the language's reader (`mc_syntax::read`), so "the same term" is
//! the same parsed program, hence the same value. Subscripts are not a cell construct (decision NBUI.5: `x_1` has no
//! meaning in the language), so they are not in the matrix.
//!
//! The test prints the matrix (construct x operation, pass counts) and fails if any cell fails.
use mc_api::RefStore;
mod common;
use common::Old;
use mc_boxes::{from_text, linear_text, restrict, to_text, Item, Row, RowPath, Step};
use mc_typeset::typeset_input;
use mc_edit::{Editor, Structure};

/// What a construct is made of: how to type it, its canonical text, its linear text, and the canonical texts that
/// Backspace at the start of its first slot and Delete at the end of its last slot leave (`None`: the term stays).
struct Construct {
    name: &'static str,
    keys: &'static [K],
    canonical: &'static str,
    linear: &'static str,
    backspace_first: Option<&'static str>,
}

/// A key of the matrix scripts.
#[derive(Clone, Copy)]
enum K {
    T(&'static str),
    S(Structure),
    Out,
    Right,
}

fn constructs() -> Vec<Construct> {
    use Structure::*;
    use K::*;
    vec![
        Construct { name: "power", keys: &[T("x"), S(Superscript), T("2")], canonical: "((x)^(2))", linear: "x^2", backspace_first: None },
        Construct { name: "fraction", keys: &[T("a"), S(Fraction), T("b")], canonical: "((a)/(b))", linear: "a/b", backspace_first: None },
        Construct { name: "square root", keys: &[S(Radical), T("x")], canonical: "((sqrt(x)))", linear: "sqrt(x)", backspace_first: Some("x") },
        Construct {
            name: "n-th root",
            keys: &[S(Root), T("3"), S(Root), T("x")],
            canonical: "((surd(x, 3)))",
            linear: "surd(x, 3)",
            backspace_first: None,
        },
        Construct {
            name: "power of a sum",
            keys: &[T("(a+b)"), S(Superscript), T("n")],
            canonical: "(((a+b))^(n))",
            linear: "(a+b)^n",
            backspace_first: None,
        },
        Construct {
            name: "fraction of power over root",
            keys: &[S(Fraction), T("x"), S(Superscript), T("2"), Out, Right, S(Radical), T("y")],
            canonical: "((((x)^(2)))/(((sqrt(y)))))",
            linear: "x^2/sqrt(y)",
            backspace_first: None,
        },
        Construct {
            name: "root in an exponent",
            keys: &[T("e"), S(Superscript), S(Radical), T("x")],
            canonical: "((e)^(((sqrt(x)))))",
            linear: "e^sqrt(x)",
            backspace_first: None,
        },
        Construct {
            name: "power of a root",
            keys: &[S(Radical), T("x"), Out, S(Superscript), T("3")],
            canonical: "((((sqrt(x))))^(3))",
            linear: "sqrt(x)^3",
            backspace_first: None,
        },
        Construct {
            name: "n-th root of a fraction",
            keys: &[S(Root), T("3"), S(Root), T("a"), S(Fraction), T("b")],
            canonical: "((surd(((a)/(b)), 3)))",
            linear: "surd(a/b, 3)",
            backspace_first: None,
        },
        Construct {
            name: "fraction in a fraction",
            keys: &[T("1"), S(Fraction), T("1"), S(Fraction), T("x")],
            canonical: "((1)/(((1)/(x))))",
            linear: "1/(1/x)",
            backspace_first: None,
        },
        // round 21: deeper nestings and the root-in-index cases
        Construct {
            name: "n-th root with a root index",
            keys: &[S(Root), S(Radical), T("y"), Out, S(Root), T("x")],
            canonical: "((surd(x, ((sqrt(y))))))",
            linear: "surd(x, sqrt(y))",
            backspace_first: None,
        },
        Construct {
            name: "power in a power",
            keys: &[T("x"), S(Superscript), T("2"), S(Superscript), T("n")],
            canonical: "((x)^(((2)^(n))))",
            linear: "x^2^n",
            backspace_first: None,
        },
        Construct {
            name: "fraction in an exponent",
            keys: &[T("e"), S(Superscript), T("a"), S(Fraction), T("b")],
            canonical: "((e)^(((a)/(b))))",
            linear: "e^(a/b)",
            backspace_first: None,
        },
        Construct {
            name: "power of an n-th root",
            keys: &[S(Root), T("3"), S(Root), T("x"), Out, S(Superscript), T("2")],
            canonical: "((((surd(x, 3))))^(2))",
            linear: "surd(x, 3)^2",
            backspace_first: None,
        },
        Construct {
            name: "fraction of fractions",
            keys: &[S(Fraction), T("a"), S(Fraction), T("b"), Out, Right, T("c"), S(Fraction), T("d")],
            canonical: "((((a)/(b)))/(((c)/(d))))",
            linear: "(a/b)/(c/d)",
            backspace_first: None,
        },
        Construct {
            name: "root of a root of a root",
            keys: &[S(Radical), S(Radical), S(Radical), T("x")],
            canonical: "((sqrt(((sqrt(((sqrt(x)))))))))",
            linear: "sqrt(sqrt(sqrt(x)))",
            backspace_first: Some("((sqrt(((sqrt(x))))))"),
        },
        Construct {
            name: "n-th root of a power over a root",
            keys: &[S(Root), T("3"), S(Root), T("x"), S(Superscript), T("2"), Out, S(Fraction), S(Radical), T("y")],
            canonical: "((surd(((((x)^(2)))/(((sqrt(y))))), 3)))",
            linear: "surd(x^2/sqrt(y), 3)",
            backspace_first: None,
        },
    ]
}

fn type_keys(ed: &mut Editor, keys: &[K]) {
    for k in keys {
        match *k {
            K::T(t) => ed.insert_text(t),
            K::S(s) => assert!(ed.structure(s)),
            K::Out => assert!(ed.exit_structure()),
            K::Right => {
                ed.move_right(false);
            }
        }
    }
}

/// Every caret position of a tree: (row path, offset), rows in reading order.
fn positions(row: &Row, path: &mut RowPath, out: &mut Vec<(RowPath, usize)>) {
    for k in 0..=row.len() {
        out.push((path.clone(), k));
        if let Some(Item::Box(b)) = row.0.get(k) {
            for (s, row) in b.slots().into_iter().enumerate() {
                path.push(Step { item: k, slot: s });
                positions(row, path, out);
                path.pop();
            }
        }
    }
}

/// The slots of a tree: (path, row).
fn slots(row: &Row) -> Vec<(RowPath, Row)> {
    let mut ps = vec![];
    positions(row, &mut vec![], &mut ps);
    let mut out: Vec<(RowPath, Row)> = vec![];
    for (p, _) in ps {
        if !p.is_empty() && !out.iter().any(|(q, _)| *q == p) {
            out.push((p.clone(), row.at(&p).cloned().unwrap_or_default()));
        }
    }
    out
}

/// The characters typed into a tree (its slots' characters, not the boxes' names).
fn typed_chars(r: &Row) -> Vec<char> {
    let mut out = vec![];
    for i in &r.0 {
        match i {
            Item::Char(c) if c.is_alphanumeric() => out.push(*c),
            Item::Box(b) => b.slots().into_iter().for_each(|s| out.extend(typed_chars(s))),
            _ => {}
        }
    }
    out
}

fn filled(r: &Row) -> bool {
    r.0.iter().all(|i| match i {
        Item::Box(b) => b.slots().iter().all(|s| !s.is_empty() && filled(s)),
        _ => true,
    })
}

struct Terms {
    store: RefStore,
}

impl Terms {
    /// The term `text` reads as (printed), or the error.
    fn of(&self, text: &str) -> Result<String, String> {
        let printer = mc_syntax::Printer::new(&self.store);
        mc_syntax::read(&self.store, text).map(|p| printer.program(&p)).map_err(|e| format!("{e:?}"))
    }
}

/// One operation of the matrix on a construct: `Ok` or why it failed.
type Check = fn(&Construct, &Terms) -> Result<(), String>;

fn made(c: &Construct) -> Editor {
    let mut ed = Editor::new(Row::default());
    type_keys(&mut ed, c.keys);
    ed
}

fn eq<T: PartialEq + std::fmt::Debug>(what: &str, got: T, want: T) -> Result<(), String> {
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got:?}, want {want:?}"))
    }
}

fn enter(c: &Construct, t: &Terms) -> Result<(), String> {
    let ed = made(c);
    eq("canonical", to_text(ed.root()).as_str(), c.canonical)?;
    eq("reads back", &from_text(c.canonical), ed.root())?;
    eq("linear", linear_text(ed.root()).as_str(), c.linear)?;
    eq("same term", t.of(c.linear)?, t.of(c.canonical)?)
}

fn walk(c: &Construct, _: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    let mut all = vec![];
    positions(ed.root(), &mut vec![], &mut all);
    ed.set_caret(&[], 0);
    let mut seen = vec![(ed.caret().path.clone(), ed.caret().offset)];
    while ed.move_right(false) {
        seen.push((ed.caret().path.clone(), ed.caret().offset));
        if seen.len() > 1000 {
            return Err("Right never ends".into());
        }
    }
    eq("ends at the end", (ed.caret().path.clone(), ed.caret().offset), (vec![], ed.root().len()))?;
    for p in &all {
        if !seen.contains(p) {
            return Err(format!("Right never visits {p:?}"));
        }
    }
    let mut back = vec![(ed.caret().path.clone(), ed.caret().offset)];
    while ed.move_left(false) {
        back.push((ed.caret().path.clone(), ed.caret().offset));
        if back.len() > 1000 {
            return Err("Left never ends".into());
        }
    }
    back.reverse();
    eq("Left retraces Right", back, seen)
}

fn home_end(c: &Construct, _: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    // Home and End go to the slot's edges; pressed at an edge already, to the edges of the line (round 32, R32.4,
    // now the editor's own rule)
    for (p, row) in slots(ed.root()) {
        if row.len() < 2 {
            continue;
        }
        ed.set_caret(&p, row.len() / 2);
        ed.move_home(false);
        eq("Home in a slot", (ed.caret().path.clone(), ed.caret().offset), (p.clone(), 0))?;
        ed.move_end(false);
        eq("End in a slot", (ed.caret().path.clone(), ed.caret().offset), (p.clone(), row.len()))?;
        ed.move_end(false);
        eq("End again", (ed.caret().path.clone(), ed.caret().offset), (vec![], ed.root().len()))?;
        ed.set_caret(&p, 0);
        ed.move_home(false);
        eq("Home again", (ed.caret().path.clone(), ed.caret().offset), (vec![], 0))?;
    }
    Ok(())
}

fn select(c: &Construct, _: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    // Shift+Right from the start selects the whole box; its text is the construct
    ed.set_caret(&[], 0);
    while ed.move_right(true) {}
    eq("Shift+Right over everything", ed.selected_text().as_str(), c.canonical)?;
    ed.set_caret(&[], ed.root().len());
    while ed.move_left(true) {}
    eq("Shift+Left over everything", ed.selected_text().as_str(), c.canonical)?;
    // every slot selected on its own copies exactly that slot
    for (p, row) in slots(ed.root()) {
        ed.select(&p, 0..row.len());
        eq("a slot selected", from_text(&ed.selected_text()), row.clone())?;
        // Shift+Right out of the slot selects the whole box (a selection never cuts a box)
        ed.set_caret(&p, row.len());
        ed.move_right(true);
        let (sp, r) = ed.range().ok_or("no selection after Shift+Right out of a slot")?;
        let parent = &p[..p.len() - 1];
        let last = p[p.len() - 1];
        let box_kept = sp.as_slice() == parent && r.contains(&last.item) || sp == p;
        if !box_kept {
            return Err(format!("Shift+Right out of slot {p:?} selected {sp:?} {r:?}"));
        }
    }
    // Ctrl+. grows to the whole tree
    ed.set_caret(&slots(ed.root())[0].0, 0);
    let mut n = 0;
    while ed.expand_selection() {
        n += 1;
        if n > 50 {
            return Err("Ctrl+. never ends".into());
        }
    }
    eq("Ctrl+. reaches the whole", ed.selected_text().as_str(), c.canonical)
}

fn backspace_into(c: &Construct, _: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    ed.set_caret(&[], ed.root().len());
    ed.backspace();
    eq("Backspace after a box enters it, deleting nothing", to_text(ed.root()).as_str(), c.canonical)?;
    if ed.caret().path.is_empty() {
        return Err("the caret stayed outside".into());
    }
    let mut ed = Editor::from_text(c.canonical);
    ed.set_caret(&[], 0);
    ed.delete();
    eq("Delete before a box enters it, deleting nothing", to_text(ed.root()).as_str(), c.canonical)?;
    if ed.caret().path.is_empty() {
        return Err("the caret stayed outside".into());
    }
    Ok(())
}

fn unwrap(c: &Construct, t: &Terms) -> Result<(), String> {
    let before = t.of(c.canonical)?;
    let first = slots(&from_text(c.canonical))[0].0.clone();
    let mut ed = Editor::from_text(c.canonical);
    ed.set_caret(&first, 0);
    if !ed.backspace() {
        return Err("Backspace at the first slot's start did nothing".into());
    }
    let after = to_text(ed.root());
    match c.backspace_first {
        None => eq("Backspace at a slot edge keeps the term", t.of(&after)?, before.clone())?,
        Some(want) => eq("Backspace at the first slot's start", after.as_str(), want)?,
    }
    if !filled(ed.root()) {
        return Err("an empty slot after unwrapping".into());
    }
    // every slot edge of the top box: Backspace at each slot's start and Delete at each slot's end
    let top: Vec<(RowPath, Row)> = slots(&from_text(c.canonical)).into_iter().filter(|(p, _)| p.len() == 1).collect();
    for (p, row) in top {
        for forward in [false, true] {
            let mut ed = Editor::from_text(c.canonical);
            ed.set_caret(&p, if forward { row.len() } else { 0 });
            let did = if forward { ed.delete() } else { ed.backspace() };
            let text = to_text(ed.root());
            if !did || !filled(ed.root()) {
                return Err(format!("edge {p:?} forward {forward}: {text}"));
            }
            // no character is lost: every character of the construct is still there
            let (had, has) = (typed_chars(&from_text(c.canonical)), typed_chars(ed.root()));
            if !had.iter().all(|ch| has.contains(ch)) {
                return Err(format!("edge {p:?} forward {forward} lost characters: {} -> {text}", c.canonical));
            }
            if c.backspace_first.is_none() {
                eq(&format!("edge {p:?} forward {forward} keeps the term"), t.of(&text)?, before.clone())?;
            }
        }
    }
    Ok(())
}

fn erase(c: &Construct, _: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    ed.set_caret(&[], ed.root().len());
    let mut n = 0;
    while !ed.root().is_empty() {
        ed.backspace();
        n += 1;
        if !filled(ed.root()) {
            return Err(format!("an empty slot after {n} Backspaces"));
        }
        if n > 200 {
            return Err(format!("200 Backspaces leave {}", to_text(ed.root())));
        }
    }
    Ok(())
}

fn operator_after(c: &Construct, t: &Terms) -> Result<(), String> {
    for op in ["+", "-", "*", "/", "^", " == "] {
        let mut ed = made(c);
        // the caret leaves the box first (Ctrl+Space as often as needed), then the operator and a name
        while ed.exit_structure() {}
        ed.insert_text(op);
        ed.insert_text("y");
        let want = format!("{}{op}y", c.canonical);
        eq(&format!("`{op}` typed after the box"), to_text(ed.root()), want.clone())?;
        // a box is one operand: what follows applies to all of it
        eq("its term", t.of(&to_text(ed.root()))?, t.of(&format!("({}){op}y", c.linear))?)?;
    }
    Ok(())
}

fn copy_paste(c: &Construct, t: &Terms) -> Result<(), String> {
    let mut ed = Editor::from_text(c.canonical);
    ed.select_all();
    let copied = ed.selected_text();
    // into plain text, between characters
    let mut other = Editor::from_text("z+w");
    other.set_caret(&[], 2);
    other.paste(&copied);
    // the box editor's implicit `*` between a box and the name after it
    eq("pasted into text", to_text(other.root()), format!("z+{}*w", c.canonical))?;
    // into the numerator of a fraction: the structure nests unchanged
    let mut f = Editor::from_text("((q)/(r))");
    f.set_caret(&[Step { item: 0, slot: 0 }], 1);
    f.paste(&copied);
    let num = f.root().at(&[Step { item: 0, slot: 0 }]).cloned().unwrap_or_default();
    eq("pasted into a numerator", to_text(&num), format!("q*{}", c.canonical))
        .or_else(|_| eq("pasted into a numerator", to_text(&num), format!("q{}", c.canonical)))?;
    // cut and paste back is the identity
    let mut g = Editor::from_text(c.canonical);
    g.select_all();
    let cut = g.cut();
    g.paste(&cut);
    eq("cut then paste", to_text(g.root()).as_str(), c.canonical)?;
    // the copy's term
    eq("copied term", t.of(&copied)?, t.of(c.linear)?)
}

fn text_round_trip(c: &Construct, t: &Terms) -> Result<(), String> {
    // the linear text another program receives, read back by Ctrl+Shift+N (typeset_input), keeping the three
    // kinds of box: the identical structure
    let lin = linear_text(&from_text(c.canonical));
    let back = typeset_input(&lin).map_err(|e| format!("{lin} does not read: {e:?}"))?;
    let back = restrict(&back);
    eq("linear text read back as boxes", to_text(&back).as_str(), c.canonical)?;
    eq("its term", t.of(&to_text(&back))?, t.of(c.canonical)?)
}

#[test]
fn typeset_math_matrix() {
    let ops: &[(&str, Check)] = &[
        ("enter", enter),
        ("walk", walk),
        ("home/end", home_end),
        ("select", select),
        ("into box", backspace_into),
        ("unwrap", unwrap),
        ("erase", erase),
        ("operator", operator_after),
        ("copy/paste", copy_paste),
        ("text trip", text_round_trip),
    ];
    let t = Terms { store: RefStore::new() };
    let mut failures = vec![];
    let mut table = String::new();
    let width = 30;
    table.push_str(&format!("{:width$}", "construct"));
    for (op, _) in ops {
        table.push_str(&format!(" {op:>10}"));
    }
    table.push('\n');
    let mut passed = 0;
    let cs = constructs();
    for c in &cs {
        table.push_str(&format!("{:width$}", c.name));
        for (op, check) in ops {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(c, &t)))
                .unwrap_or_else(|_| Err("panicked".into()));
            match r {
                Ok(()) => {
                    passed += 1;
                    table.push_str(&format!(" {:>10}", "ok"));
                }
                Err(e) => {
                    table.push_str(&format!(" {:>10}", "FAIL"));
                    failures.push(format!("{} / {op}: {e}", c.name));
                }
            }
        }
        table.push('\n');
    }
    let total = cs.len() * ops.len();
    println!("{table}typeset matrix: {passed} of {total} passed");
    assert!(failures.is_empty(), "{table}\n{}", failures.join("\n"));
}
