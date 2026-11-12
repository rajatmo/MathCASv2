//! Random printed outputs through typesetting and layout: nothing panics and every row gets its caret stops (the
//! editor's random command sequences moved to `mc-edit/tests/fuzz.rs` with the editor, round 34). Seeded and
//! bounded; `MC_BOXES_SEED` and `MC_BOXES_CASES` override the defaults. The ignored test `measure` prints timings
//! (run it in release through the guard).
use mc_boxes::layout::{layout, LayoutOptions, MonoMetrics};
use mc_boxes::{from_text, to_text};
use mc_typeset::typeset_output;
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
    let root = from_text("f(x) = ((a + b)/(2c)) + ((sqrt(((x)^(2)) + 1))) - ((sum(((1)/(((k)^(2)))), [k, 1, n])))");
    let n = 2000;
    let t = Instant::now();
    for k in 0..n {
        if (k + 1) % (n / 4) == 0 {
            eprintln!("layout [{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
        }
        let _ = layout(&root, &MonoMetrics, &LayoutOptions::input(13.0, Some(600.0)));
    }
    eprintln!("input cell layout: {:.1} us each", t.elapsed().as_secs_f64() * 1e6 / n as f64);
    let t = Instant::now();
    for _ in 0..n {
        let _ = from_text(&to_text(&root));
    }
    eprintln!("canonical read: {:.1} us each", t.elapsed().as_secs_f64() * 1e6 / n as f64);
}
