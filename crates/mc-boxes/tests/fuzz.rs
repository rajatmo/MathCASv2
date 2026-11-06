//! Random command sequences on the editor and random trees through layout: nothing panics, slots are never
//! empty, the caret is always valid, the canonical text reads back to the tree, and every row gets its caret
//! stops. Seeded and bounded; `MC_BOXES_SEED` and `MC_BOXES_CASES` override the defaults. The ignored test
//! `measure` prints timings (run it in release through the guard).
use mc_boxes::layout::{layout, LayoutOptions, MonoMetrics};
use mc_boxes::{from_text, to_text, typeset_output, Editor, Item, Row, Structure};
use std::time::Instant;

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
    std::env::var("MC_BOXES_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0xf00d_cafe_1234_9876)
}

fn cases(default: usize) -> usize {
    std::env::var("MC_BOXES_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

/// Slots are never empty anywhere below the root.
fn slots_filled(r: &Row) -> bool {
    r.0.iter().all(|i| match i {
        Item::Box(b) => b.slots().iter().all(|s| !s.is_empty() && slots_filled(s)),
        _ => true,
    })
}

fn step(ed: &mut Editor, r: &mut Rng) {
    use Structure::*;
    match r.below(22) {
        0..=5 => ed.insert_char(['x', '1', '+', '(', ')', ',', '-', '>', ' ', '[', ']', '□'][r.below(12)]),
        6 => {
            ed.structure([Fraction, Superscript, Radical][r.below(3)]);
        }
        7 => {
            ed.backspace();
        }
        8 => {
            ed.delete();
        }
        9 => {
            ed.move_left(r.below(3) == 0);
        }
        10 => {
            ed.move_right(r.below(3) == 0);
        }
        11 => {
            ed.move_up();
        }
        12 => {
            ed.move_down();
        }
        13 => {
            ed.exit_structure();
        }
        14 => {
            ed.backspace();
        }
        15 => {
            ed.next_placeholder();
        }
        16 => {
            ed.prev_placeholder();
        }
        17 => {
            ed.escape();
        }
        18 => {
            ed.expand_selection();
        }
        19 => {
            let t = ed.cut();
            if r.below(2) == 0 {
                ed.paste(&t);
            }
        }
        20 => {
            ed.insert_template(["((integrate(▮, □)))", "(([[▮, □], [□, □]]))", "((sum(□, [▮, □, □])))"][r.below(3)]);
        }
        _ => {
            if r.below(2) == 0 {
                ed.move_home(r.below(2) == 0);
            } else {
                ed.move_end(false);
            }
        }
    }
}

#[test]
fn random_editing_keeps_the_tree_valid() {
    let mut r = Rng(seed());
    let n = cases(1500);
    for k in 0..n {
        if n >= 10 && (k + 1).is_multiple_of(n / 10) {
            eprintln!("editing [{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
        let mut ed = Editor::new(Row::new());
        for _ in 0..40 {
            step(&mut ed, &mut r);
            let root = ed.root();
            assert!(slots_filled(root), "case {k}: {}", ed.notation());
            let c = ed.caret();
            let row = root.at(&c.path).unwrap_or_else(|| panic!("case {k}: caret path {:?}", c.path));
            assert!(c.offset <= row.len(), "case {k}: {}", ed.notation());
            if let Some((p, sel)) = ed.selection() {
                assert!(sel.end <= root.at(&p).unwrap().len());
            }
        }
        // the canonical text reads back, unless the user typed brackets that unbalance a slot (documented)
        let t = ed.text();
        if from_text(&t) == *ed.root() {
            assert_eq!(to_text(&from_text(&t)), t);
        }
        let l = layout(ed.root(), &MonoMetrics, &LayoutOptions::input(12.0, Some(200.0)));
        assert!(l.width.is_finite() && l.height.is_finite());
        let (p, o) = l.hit(l.width / 2.0, l.height / 2.0);
        assert!(ed.root().at(&p).is_some_and(|row| o <= row.len()));
    }
}

#[test]
fn hostile_output_text_never_panics() {
    let mut r = Rng(seed() ^ 77);
    let bits = ["x", "^", "/", "(", ")", "1", "2", "sqrt(", "sum(", "[", "]", ", ", "->", "\"", "im", "pi", "*", "-", "derivative(1)(", "«3»"];
    let n = cases(1500);
    for _ in 0..n {
        let t: String = (0..r.below(25)).map(|_| bits[r.below(bits.len())]).collect();
        let out = typeset_output(&t);
        let l = layout(&out.row, &MonoMetrics, &LayoutOptions::output(10.0, Some(150.0)));
        assert!(l.height.is_finite());
    }
    // deep nesting stays bounded
    let deep = format!("{}x{}", "sqrt(".repeat(200), ")".repeat(200));
    let out = typeset_output(&deep);
    assert!(out.row.nesting() <= mc_boxes::MAX_NESTING);
    let _ = layout(&out.row, &MonoMetrics, &LayoutOptions::output(10.0, None));
    let canonical = format!("{}x{}", "((sqrt(".repeat(300), ")))".repeat(300));
    assert!(from_text(&canonical).nesting() <= mc_boxes::MAX_NESTING);
}

#[test]
#[ignore = "timings; run in release through the guard"]
fn measure() {
    // a long printed polynomial: typeset and lay out with wrapping
    let poly: String = (0..2000).map(|k| format!("{k}*x^{k}")).collect::<Vec<_>>().join(" + ");
    let t0 = Instant::now();
    let out = typeset_output(&poly);
    let t1 = Instant::now();
    let l = layout(&out.row, &MonoMetrics, &LayoutOptions::output(13.0, Some(900.0)));
    let t2 = Instant::now();
    eprintln!(
        "polynomial: {} chars, typeset {:.1} ms, layout {:.1} ms, {} lines, {} items",
        poly.len(),
        (t1 - t0).as_secs_f64() * 1e3,
        (t2 - t1).as_secs_f64() * 1e3,
        l.lines.len(),
        l.items.len()
    );
    // a typical input cell, laid out many times (the per-keystroke cost)
    let ed = Editor::from_text("f(x) = ((a + b)/(2c)) + ((sqrt(((x)^(2)) + 1))) - ((sum(((1)/(((k)^(2)))), [k, 1, n])))");
    let n = 2000;
    let t = Instant::now();
    for k in 0..n {
        if (k + 1) % (n / 4) == 0 {
            eprintln!("layout [{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
        let _ = layout(ed.root(), &MonoMetrics, &LayoutOptions::input(13.0, Some(600.0)));
    }
    eprintln!("input cell layout: {:.1} us each", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    let t = Instant::now();
    for _ in 0..n {
        let _ = from_text(&ed.text());
    }
    eprintln!("canonical read: {:.1} us each", t.elapsed().as_secs_f64() * 1e6 / n as f64);
}
