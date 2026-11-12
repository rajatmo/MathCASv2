//! Timings of the parts of one keystroke in a long cell (ignored; run in release with `--ignored --nocapture`).
use mc_boxes::layout::{layout, LayoutOptions, MonoMetrics};
use mc_boxes::{to_text, to_text_mapped, Row};
use mc_edit::{Command, Context, Editor};
use std::time::Instant;

fn time<T>(what: &str, n: u32, mut f: impl FnMut() -> T) -> T {
    let t = Instant::now();
    let mut out = None;
    for _ in 0..n {
        out = Some(f());
    }
    println!("{what:<28} {:>9.3} ms", t.elapsed().as_secs_f64() * 1000.0 / n as f64);
    out.unwrap()
}

#[test]
#[ignore = "timings"]
fn one_keystroke_in_a_300_line_cell() {
    let line = "result = f(alpha, beta) + 3*x^2 - y/2\n";
    let text = line.repeat(std::env::var("MC_LINES").ok().and_then(|v| v.parse().ok()).unwrap_or(300));
    let row = Row::text(&text);
    let mut ed = Editor::new(row.clone());
    let cx = Context::default();
    time("apply Type at the end", 200, || ed.apply(&Command::Type("q".into()), &cx));
    time("clone the tree", 200, || ed.root().clone());
    time("to_text", 200, || to_text(ed.root()));
    time("to_text_mapped", 200, || to_text_mapped(ed.root()));
    let opts = LayoutOptions { operator_spacing: false, small_inline_rationals: false, ..LayoutOptions::input(14.0, Some(900.0)) };
    time("layout (mono metrics)", 20, || layout(ed.root(), &MonoMetrics, &opts));
    time("colouring (mc-syntax)", 20, || mc_syntax::colour(&text, None));
    let mut cache = mc_boxes::layout::LineCache::default();
    mc_boxes::layout::layout_by_lines(ed.root(), &MonoMetrics, &opts, &mut cache);
    time("layout by lines, unchanged", 20, || mc_boxes::layout::layout_by_lines(ed.root(), &MonoMetrics, &opts, &mut cache));
    let last = Row::text(line);
    time("layout of one line", 20, || layout(&last, &MonoMetrics, &opts));
    time("layout by lines, one changed", 20, || {
        ed.apply(&Command::Type("q".into()), &cx);
        mc_boxes::layout::layout_by_lines(ed.root(), &MonoMetrics, &opts, &mut cache)
    });
    // round 37: the composed layout changed in place (the caller lets go of it before the next keystroke)
    drop(mc_boxes::layout::layout_by_lines_shared(ed.root(), &MonoMetrics, &opts, &mut cache));
    time("in place, typed at the end", 20, || {
        ed.apply(&Command::Type("q".into()), &cx);
        mc_boxes::layout::layout_by_lines_shared(ed.root(), &MonoMetrics, &opts, &mut cache).width
    });
    let mut top = Editor::new(Row::text(&format!("q{text}")));
    top.set_selection(mc_edit::Selection::caret(mc_edit::Position::root(1)));
    drop(mc_boxes::layout::layout_by_lines_shared(top.root(), &MonoMetrics, &opts, &mut cache));
    time("in place, typed at the top", 20, || {
        top.apply(&Command::Type("q".into()), &cx);
        mc_boxes::layout::layout_by_lines_shared(top.root(), &MonoMetrics, &opts, &mut cache).width
    });
}
