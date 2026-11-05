//! Layout snapshot tests (design 08, section 9.8): box positions as text, rounded to 0.1 unit, for the fixed
//! metrics source ([`MonoMetrics`]: a monospaced text font and the bundled math font) at size 10. Set
//! `MC_BOXES_BLESS=1` to rewrite `tests/snapshots/layout.txt` after an intended change, and review the diff.
//! Further checks: every row of the tree has one caret stop per offset, hit-testing a caret gives it back, and
//! wrapped lines stay within the width.
use mc_boxes::layout::{layout, Draw, Layout, LayoutOptions, MonoMetrics};
use mc_boxes::{from_text, typeset_output, Item, Node, Row, RowPath, Step};

/// (name, canonical text, display style, wrap width)
const CASES: &[(&str, &str, bool, Option<f32>)] = &[
    ("plain", "1+x", false, None),
    ("operators", "a -> b <= c != d", false, None),
    ("fraction", "1+((x)/(y))", false, None),
    ("nested fraction", "((1)/(((1)/(x)) + 1))", false, None),
    ("small rational", "((1)/(2))x", false, None),
    ("superscript", "((x)^(2)) + ((e)^(((x)^(2))))", false, None),
    ("subscript", "((subscript(x, i))) + ((subscript(a, n + 1)))", false, None),
    ("sub and super", "((((subscript(x, i))))^(2))", false, None),
    ("radical", "((sqrt(x + 1)))", false, None),
    ("root", "((root(x, 3)))", false, None),
    ("tall radical", "((sqrt(((1)/(x)))))", false, None),
    ("over under", "((overscript(x, →)))((underscript(y, 0)))((underoverscript(S, a, b)))", false, None),
    ("grid", "(([[a, b], [c, d]]))", false, None),
    ("stretchy delimiters", "(((([[1, 2], [3, 4]]))))", false, None),
    ("sum inline", "((sum(k, [k, 1, n])))", false, None),
    ("sum display", "((sum(k, [k, 1, n])))", true, None),
    ("integral display", "((integrate(f, [x, 0, 1])))", true, None),
    ("integral inline", "((integrate(f, x)))", false, None),
    ("limit display", "((limit(f, x -> 0)))", true, None),
    ("derivative inline", "((d(f, x)))", false, None),
    ("derivative display", "((d(f, x)))", true, None),
    ("placeholder", "((□)/(□))", false, None),
    ("graphic", "((image(\"p.png\")))", false, None),
    ("empty", "", false, None),
    ("wrap", "f(alpha, beta, gamma) + g(delta, epsilon)", false, Some(150.0)),
    ("wrap newline", "f(a,\nb)", false, None),
    ("wrap fraction slots", "((aaaa + bbbb + cccc + dddd)/(2))", false, Some(100.0)),
];

fn lay(text: &str, display: bool, width: Option<f32>) -> Layout {
    let o = LayoutOptions { size: 10.0, display, max_width: width, indent_continuations: !display, ..Default::default() };
    layout(&from_text(text), &MonoMetrics, &o)
}

#[test]
fn snapshots() {
    let mut all = String::new();
    for (name, text, display, width) in CASES {
        all.push_str(&format!("== {name}: {text:?}\n"));
        all.push_str(&lay(text, *display, *width).snapshot());
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/layout.txt");
    if std::env::var("MC_BOXES_BLESS").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &all).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    if expected != all {
        let (a, b): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), all.lines().collect());
        let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        panic!("layout snapshot differs at line {}:\n expected: {:?}\n actual:   {:?}", k + 1, a.get(k), b.get(k));
    }
}

/// Every row path of a tree with its length.
fn rows(r: &Row, path: &mut RowPath, out: &mut Vec<(RowPath, usize)>) {
    out.push((path.clone(), r.len()));
    for (i, it) in r.0.iter().enumerate() {
        if let Item::Box(b) = it {
            for k in 0..b.slot_count() {
                path.push(Step { item: i, slot: k });
                rows(b.slot(k).unwrap(), path, out);
                path.pop();
            }
        }
    }
}

#[test]
fn every_row_has_its_stops_and_hits_come_back() {
    for (name, text, display, width) in CASES {
        let tree = from_text(text);
        let l = lay(text, *display, *width);
        let mut all = Vec::new();
        rows(&tree, &mut Vec::new(), &mut all);
        for (path, len) in all {
            let g = l.rows.iter().find(|r| r.path == path).unwrap_or_else(|| panic!("{name}: no row {path:?}"));
            assert_eq!(g.stops.len(), len + 1, "{name}: stops of {path:?}");
            for s in &g.stops {
                assert!(s.top <= s.baseline && s.baseline <= s.bottom, "{name}: {s:?}");
                assert!(s.x >= 0.0 && s.x <= l.width + 1e-3, "{name}: {s:?}");
            }
        }
        // a click exactly on a caret position finds a position with the same geometry
        for r in &l.rows {
            for (k, s) in r.stops.iter().enumerate() {
                let (p, o) = l.hit(s.x, (s.top + s.bottom) / 2.0);
                let back = l.caret(&p, o).unwrap();
                assert!((back.x - s.x).abs() < 0.05 && (back.baseline - s.baseline).abs() < 2.0, "{name}: {:?} {k} -> {p:?} {o}", r.path);
            }
        }
    }
}

#[test]
fn wrapped_lines_fit() {
    let text = "f(alpha, beta, gamma, delta) + g(epsilon, zeta) - h(eta, theta, iota, kappa, lambda) = mu";
    for w in [80.0, 120.0, 200.0, 400.0] {
        let l = lay(text, false, Some(w));
        assert!(l.lines.len() > 1 || w >= 400.0, "{w}: {} lines", l.lines.len());
        for line in &l.lines {
            let right = l
                .items
                .iter()
                .filter_map(|d| match d {
                    Draw::Char { x, y, .. } if *y >= line.top && *y <= line.bottom => Some(*x + 6.0),
                    _ => None,
                })
                .fold(0.0, f32::max);
            assert!(right <= w + 0.1, "width {w}: line {line:?} ends at {right}");
        }
        // lines follow one another without overlapping, items in order
        for p in l.lines.windows(2) {
            assert!(p[0].bottom <= p[1].top + 0.01 && p[0].end == p[1].start, "{p:?}");
        }
        // inputs indent continuation lines inside brackets
        assert_eq!(l.lines[0].indent, 0.0);
    }
}

#[test]
fn outputs_typeset_and_lay_out() {
    for printed in ["x^2 + 2*x + 1", "sum(1/k^2, [k, 1, inf])", "[[1, 2], [3, 4]]", "sqrt(2)/2", "derivative(1)(f)(x)"] {
        let t = typeset_output(printed);
        let l = layout(&t.row, &MonoMetrics, &LayoutOptions::output(10.0, Some(300.0)));
        assert!(l.width > 0.0 && l.height > 0.0 && !l.items.is_empty(), "{printed}");
    }
    // a big operator in display style is larger than inline
    let row = Row::boxed(Node::BigOp {
        op: mc_boxes::BigOp::Sum,
        body: Row::text("k"),
        var: Row::text("k"),
        lower: Some(Row::text("1")),
        upper: Some(Row::text("n")),
    });
    let inline = layout(&row, &MonoMetrics, &LayoutOptions::input(10.0, None));
    let display = layout(&row, &MonoMetrics, &LayoutOptions::output(10.0, None));
    assert!(display.height > inline.height);
}
