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
    ("root", "((surd(x, 3)))", false, None),
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

/// Typeset outputs (design 4.5, M5): one printed value per row of the table, laid out in display style at a width
/// that makes the long sum wrap. Re-bless with `MC_BOXES_BLESS=1`.
const OUTPUTS: &[&str] = &[
    "11/15",
    "(x + 1)^5/(x - 1)^2",
    "sqrt(2)*sqrt(x) + surd(x, 3)",
    "2*x*y^2 - 3*x",
    "subscript(a, n)^2 + overscript(x, \"^\")",
    "sum(1/k^2, [k, 1, n])",
    "product(f(k), [k, 1, n])",
    "integrate(e^(-x^2), [x, 0, inf])",
    "integrate(f(x), x)",
    "limit(sin(x)/x, x -> 0)",
    "d(f(x), x)",
    "[derivative(1)(f)(x), derivative(2)(f)(x), derivative(5)(f)(x)]",
    "[x -> pi/2, y != im, z <= inf]",
    "\"text\"",
    "[[1, 2], [3, 4]]",
    "(2/3)*2^(1/2) - (1/2)*x",
    "x^12 + 12*x^11*y + 66*x^10*y^2 + 220*x^9*y^3 + 495*x^8*y^4 + 792*x^7*y^5 + 924*x^6*y^6",
];

#[test]
fn output_snapshots() {
    let mut all = String::new();
    for printed in OUTPUTS {
        let t = typeset_output(printed);
        all.push_str(&format!("== {printed:?}\n"));
        all.push_str(&layout(&t.row, &MonoMetrics, &LayoutOptions::output(10.0, Some(240.0))).snapshot());
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/output.txt");
    if std::env::var("MC_BOXES_BLESS").is_ok_and(|v| v == "1") {
        std::fs::write(&path, &all).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default().replace("\r\n", "\n");
    if expected != all {
        let (a, b): (Vec<&str>, Vec<&str>) = (expected.lines().collect(), all.lines().collect());
        let k = a.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(a.len().min(b.len()));
        panic!("output snapshot differs at line {}:\n expected: {:?}\n actual:   {:?}", k + 1, a.get(k), b.get(k));
    }
}

/// A painter's text font: monospaced, with its own axis and line pitch, every letter its own (no math italics).
struct TextMetrics;

impl mc_boxes::layout::Metrics for TextMetrics {
    fn char(&self, c: char, role: mc_boxes::layout::FontRole) -> mc_boxes::layout::CharMetrics {
        MonoMetrics.char(c, role)
    }
    fn role(&self, c: char) -> mc_boxes::layout::FontRole {
        if c == '∑' {
            mc_boxes::layout::FontRole::Math
        } else {
            mc_boxes::layout::FontRole::Text
        }
    }
    fn axis_height(&self) -> Option<f32> {
        Some(0.3)
    }
    fn line_pitch(&self) -> Option<f32> {
        Some(1.25)
    }
    fn italic_letters(&self) -> bool {
        false
    }
}

#[test]
fn painter_metrics_choose_fonts_axis_and_line_pitch() {
    let o = LayoutOptions { size: 10.0, traditional: true, ..Default::default() };
    // plain lines are exactly one pitch apart, as the painter's text lines
    let l = layout(&from_text("a\nb\nc"), &TextMetrics, &o);
    assert_eq!(l.lines.len(), 3);
    for w in l.lines.windows(2) {
        assert!((w[1].baseline - w[0].baseline - 12.5).abs() < 1e-3, "{:?}", l.lines);
    }
    // traditional form keeps the letters (no math italics) and draws them in the text font
    let chars: Vec<(char, mc_boxes::layout::FontRole)> =
        l.items.iter().filter_map(|d| if let Draw::Char { c, role, .. } = d { Some((*c, *role)) } else { None }).collect();
    assert_eq!(chars.iter().map(|c| c.0).collect::<String>(), "abc");
    assert!(chars.iter().all(|c| c.1 == mc_boxes::layout::FontRole::Text));
    // the fraction bar sits on the painter's axis (0.3 em above the baseline)
    let f = layout(&from_text("((a)/(b))"), &TextMetrics, &LayoutOptions { size: 10.0, ..Default::default() });
    let rule = f.items.iter().find_map(|d| if let Draw::Rule { y, h, .. } = d { Some(y + h / 2.0) } else { None }).unwrap();
    assert!((f.baseline - rule - 3.0).abs() < 1e-3, "{} {rule}", f.baseline);
    // the default metrics keep the old behaviour (pictures use them)
    let m = layout(&from_text("x"), &MonoMetrics, &o);
    assert!(m.items.iter().any(|d| matches!(d, Draw::Char { c: '𝑥', .. })));
}

#[test]
fn lines_right_justify_and_everything_follows() {
    let text = "f(alpha, beta, gamma) + g(delta, epsilon) + ((x)/(y))";
    let mut l = lay(text, true, Some(150.0));
    assert!(l.lines.len() >= 2);
    let before = l.clone();
    l.align_right(200.0);
    for (a, b) in before.lines.iter().zip(&l.lines) {
        assert!((b.right - 200.0).abs() < 1e-3, "{:?}", l.lines);
        assert!((b.indent - a.indent - (200.0 - a.right)).abs() < 1e-3);
    }
    assert!(l.width >= 200.0);
    // every item moved by its line's shift; caret stops too, so hit-testing gives the same positions back
    for (a, b) in before.items.iter().zip(&l.items) {
        let (x0, x1) = match (a, b) {
            (Draw::Char { x: x0, .. }, Draw::Char { x: x1, .. }) | (Draw::Rule { x: x0, .. }, Draw::Rule { x: x1, .. }) => (*x0, *x1),
            _ => continue,
        };
        assert!(x1 >= x0);
    }
    for r in &l.rows {
        for (k, s) in r.stops.iter().enumerate() {
            assert_eq!(l.hit(s.x, s.baseline), (r.path.clone(), k), "{:?} {k}", r.path);
        }
    }
    // translating keeps hit-testing consistent too
    let mut t = l.clone();
    t.translate(3.0, -2.0);
    let s = t.rows[0].stops[1];
    assert_eq!(t.hit(s.x, s.baseline), (vec![], 1));
    assert!((t.baseline - l.baseline + 2.0).abs() < 1e-4);
}

/// Tall brackets drawn as strokes stay inside their box, closing brackets mirror opening ones, and a tall bracket
/// around a fraction is a `Draw::Delim` (with vector delimiters on) or the font's glyph (off).
#[test]
fn stroked_delimiters() {
    use mc_boxes::layout::{delimiter_paths, is_vector_delimiter, Draw};
    let (x, y, w, h, t) = (10.0, 20.0, 6.0, 40.0, 0.8);
    for (open, close) in [('(', ')'), ('[', ']'), ('{', '}'), ('⟨', '⟩'), ('⌈', '⌉'), ('⌊', '⌋')] {
        assert!(is_vector_delimiter(open) && is_vector_delimiter(close));
        let a = delimiter_paths(open, x, y, w, h, t);
        let b = delimiter_paths(close, x, y, w, h, t);
        assert!(!a.is_empty(), "{open}");
        for p in a.iter().chain(&b).flatten() {
            assert!(p[0] >= x - 1e-3 && p[0] <= x + w + 1e-3 && p[1] >= y - 1e-3 && p[1] <= y + h + 1e-3, "{open} {p:?}");
        }
        for (la, lb) in a.iter().zip(&b) {
            for (pa, pb) in la.iter().zip(lb) {
                assert!((pa[0] - (2.0 * x + w - pb[0])).abs() < 1e-3 && (pa[1] - pb[1]).abs() < 1e-3, "{open}{close}");
            }
        }
    }
    assert!(!is_vector_delimiter('a') && delimiter_paths('a', x, y, w, h, t).is_empty());
    let tall = "(((([[1, 2], [3, 4]]))))";
    let on = layout(&from_text(tall), &MonoMetrics, &LayoutOptions::output(10.0, None));
    assert!(on.items.iter().any(|d| matches!(d, Draw::Delim { c: '(', .. })));
    let off = layout(&from_text(tall), &MonoMetrics, &LayoutOptions { vector_delimiters: false, ..LayoutOptions::output(10.0, None) });
    assert!(!off.items.iter().any(|d| matches!(d, Draw::Delim { .. })));
}

/// A rational coefficient's parentheses go into its fraction box (`(1/2)*pi` is ½ pi, not (½) pi); a fraction that
/// is not a coefficient keeps them.
#[test]
fn coefficient_fractions_lose_their_parentheses() {
    use mc_boxes::{math_text, Item};
    let row = |s: &str| math_text(s).row;
    let parens = |r: &mc_boxes::Row| r.0.iter().filter(|i| matches!(i, Item::Char('(') | Item::Char(')'))).count();
    assert_eq!(parens(&row("(1/2)*pi")), 0);
    assert_eq!(parens(&row("(1/2)*x + (3/4)*y")), 0);
    assert_eq!(parens(&row("f((1/2))")), 4);
}

/// Round 33: a power whose base is not one operand is drawn with parentheses (they are no items: the caret stops
/// are those of the base), a simple base and a base typed in brackets without more; outputs keep their own.
#[test]
fn compound_bases_are_drawn_in_parentheses() {
    let opens = |l: &Layout| l.items.iter().filter(|d| matches!(d, Draw::Char { c: '(', .. } | Draw::Delim { c: '(', .. })).count();
    let power = |base: Row| Row::boxed(Node::Superscript { base, exp: Row::text("2") });
    let o = LayoutOptions::input(10.0, None);
    let lay_row = |r: &Row| layout(r, &MonoMetrics, &o);
    assert_eq!(opens(&lay_row(&power(Row::text("x")))), 0);
    assert_eq!(opens(&lay_row(&power(Row::text("2m")))), 1);
    assert_eq!(opens(&lay_row(&power(Row::text("(2m)")))), 1);
    assert_eq!(opens(&lay_row(&power(Row::text("-x")))), 1);
    assert_eq!(opens(&lay_row(&power(Row::text("f(x)")))), 1);
    let frac = Row::boxed(Node::Fraction { num: Row::text("a"), den: Row::text("b") });
    assert_eq!(opens(&lay_row(&power(frac))), 1);
    // the base's stops: before `2`, between, after `m`; nothing for the parentheses
    let l = lay_row(&power(Row::text("2m")));
    let base = l.rows.iter().find(|r| r.path == [Step { item: 0, slot: 0 }]).unwrap();
    assert_eq!(base.stops.len(), 3);
    assert!(base.stops[0].x > 0.0, "the first stop is after the opening parenthesis");
    for (text, n) in [("(a + b)^2", 1), ("x^2", 0), ("(-2)^n", 1), ("(a/b)^n", 1), ("f(x)^2", 1), ("(x^2)^3", 1), ("2^(n + 1)", 0)] {
        let out = layout(&typeset_output(text).row, &MonoMetrics, &LayoutOptions::output(10.0, None));
        assert_eq!(opens(&out), n, "{text}");
    }
}

/// Caret geometry (design 11, section 5): every stop is one text line tall at its row's scale (text, script or
/// script-script size), never the height of the line around it; on the root row every stop has the full text line,
/// also beside a fraction. Every item has a box, and the boxes of a row's items lie left to right.
#[test]
fn every_stop_is_one_text_line_tall() {
    let f = mc_mathfont::font();
    let size = 10.0;
    let (a, d) = (0.75, 0.25);
    let scales = [1.0, f.constants.script_percent / 100.0, f.constants.script_script_percent / 100.0];
    for (name, text, display, width) in CASES {
        let l = lay(text, *display, *width);
        let row = from_text(text);
        for r in &l.rows {
            for (k, s) in r.stops.iter().enumerate() {
                let h = s.bottom - s.top;
                let ok = scales.iter().any(|sc| (h - (a + d) * size * sc).abs() < 0.01);
                assert!(ok, "{name}: row {:?} stop {k} is {h} tall", r.path);
                assert!((s.ascent() / (s.ascent() + s.descent()) - a).abs() < 0.01, "{name}: {:?} {k}", r.path);
                if r.path.is_empty() {
                    assert!((h - (a + d) * size).abs() < 0.01, "{name}: root stop {k} is {h} tall");
                }
            }
            let len = row.at(&r.path).map_or(0, |x| x.len());
            assert_eq!(r.ink.len(), len, "{name}: row {:?} has a box per item", r.path);
            for b in &r.ink {
                assert!(b.w >= 0.0 && b.bottom > b.top, "{name}: {:?} {b:?}", r.path);
            }
        }
    }
    // beside a fraction the caret is a text line, not the fraction's height
    let l = lay("a + ((x)/(y))", false, None);
    let s = l.caret(&[], 1).unwrap();
    let frac = l.item_box(&[], 4).unwrap();
    assert!(s.bottom - s.top < (frac.bottom - frac.top) * 0.8, "{s:?} {frac:?}");
    // a click on the numerator lands in it, on the bar's level beside it in the root row
    let num = l.item_box(&[Step { item: 4, slot: 0 }], 0).unwrap();
    assert_eq!(l.hit(num.x + num.w * 0.4, (num.top + num.bottom) / 2.0), (vec![Step { item: 4, slot: 0 }], 0));
    assert_eq!(l.hit(0.1, s.baseline), (vec![], 0));
    // selection boxes: one per line, the union of the items'
    let sel = l.selection_boxes(&[], 0..5);
    assert_eq!(sel.len(), 1);
    assert!((sel[0].x - l.item_box(&[], 0).unwrap().x).abs() < 1e-3 && (sel[0].x + sel[0].w - frac.x - frac.w).abs() < 1e-3);
}

/// A row laid out line by line (each logical line on its own, unchanged lines from the cache) is the row laid out
/// whole: the same lines, caret stops, item boxes and painted items, also with brackets open across lines, wrapped
/// lines and boxes; and after an edit of one line only that line is laid out again.
#[test]
fn line_by_line_layout_is_the_whole_layout() {
    use mc_boxes::layout::{layout_by_lines, LineCache};
    let texts = [
        "a = 1\nb = 2\n",
        "f(x,\n  y) + ((sqrt(z)))\ng(((1)/(2)))",
        "alpha + beta + gamma + delta + epsilon\nf(zeta, eta,\ntheta + iota + kappa + lambda + mu + nu)\n\nx",
        "((x)^(2))\n((a)/(b)) + 1\n",
    ];
    let opts = LayoutOptions::input(10.0, Some(160.0));
    for t in texts {
        let row = from_text(t);
        let whole = mc_boxes::layout::layout(&row, &MonoMetrics, &opts);
        let mut cache = LineCache::default();
        let parts = layout_by_lines(&row, &MonoMetrics, &opts, &mut cache);
        let close = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert_eq!(whole.lines.len(), parts.lines.len(), "{t:?}");
        for (a, b) in whole.lines.iter().zip(&parts.lines) {
            assert!(a.start == b.start && a.end == b.end && close(a.baseline, b.baseline) && close(a.indent, b.indent), "{t:?}\n{a:?}\n{b:?}");
        }
        assert_eq!(whole.items.len(), parts.items.len(), "{t:?}");
        for r in &whole.rows {
            let q = parts.rows.iter().find(|q| q.path == r.path).unwrap_or_else(|| panic!("{t:?}: no row {:?}", r.path));
            assert_eq!(r.stops.len(), q.stops.len(), "{t:?} {:?}", r.path);
            for (a, b) in r.stops.iter().zip(&q.stops) {
                assert!(close(a.x, b.x) && close(a.baseline, b.baseline) && close(a.top, b.top), "{t:?} {:?}: {a:?} {b:?}", r.path);
            }
            assert_eq!(r.ink.len(), q.ink.len(), "{t:?} {:?}", r.path);
        }
        assert!(close(whole.height, parts.height) || parts.height >= whole.height, "{t:?}");
    }
}

/// Round 37: the layout composed in place where an edit changed the row (`layout_by_lines_shared` keeping its last
/// layout) is the layout composed from the start, number for number, after every edit of a random sequence: typing in
/// a line, deleting, new lines and joined lines (lines move), brackets (the depth of the lines after changes),
/// boxes (rows and heights change), pastes of several lines, and no change at all.
#[test]
fn layouts_changed_in_place_are_layouts_composed_from_the_start() {
    use mc_boxes::layout::{layout_by_lines, layout_by_lines_shared, LineCache};
    let seed: u64 = std::env::var("MC_BOXES_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(37);
    let mut x = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = |n: usize| {
        x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((x >> 33) as usize) % n.max(1)
    };
    let boxes = [from_text("((1)/(2))"), from_text("((x)^(2))"), from_text("((sqrt(y + 1)))")];
    for opts in [LayoutOptions::input(10.0, Some(160.0)), LayoutOptions::input(13.0, None)] {
        let mut row = from_text("a = 1\nf(x,\n  y) + ((sqrt(z)))\ng(((1)/(2)))\nalpha + beta + gamma + delta + epsilon + zeta\n\nlast");
        let mut cache = LineCache::default();
        for step in 0..400 {
            let len = row.0.len();
            match next(9) {
                0..=2 => {
                    let c = ['q', ' ', '+', '1', 'z'][next(5)];
                    row.0.insert(next(len + 1), Item::Char(c));
                }
                3 if len > 0 => {
                    row.0.remove(next(len));
                }
                4 => row.0.insert(next(len + 1), Item::Char('\n')),
                5 => {
                    let c = ['(', ')', '[', ']'][next(4)];
                    row.0.insert(next(len + 1), Item::Char(c));
                }
                6 => {
                    let b = boxes[next(3)].0.clone();
                    let at = next(len + 1);
                    row.0.splice(at..at, b);
                }
                7 => {
                    let at = next(len + 1);
                    row.0.splice(at..at, Row::text("u = v\nw(\n").0);
                }
                _ => {}
            }
            // the composed layout is let go of before the next call (as the notebook does), so it is changed in place
            let shared = layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache);
            let whole = layout_by_lines(&row, &MonoMetrics, &opts, &mut LineCache::default());
            assert!(*shared == whole, "seed {seed}, step {step}: {:?}", mc_boxes::to_text(&row));
            drop(shared);
            // a layout still held elsewhere is copied, not changed under its holder
            if step % 50 == 0 {
                let held = layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache);
                row.0.push(Item::Char('k'));
                let again = layout_by_lines_shared(&row, &MonoMetrics, &opts, &mut cache);
                row.0.pop();
                assert!(*held != *again);
                assert!(*held == layout_by_lines(&row, &MonoMetrics, &opts, &mut LineCache::default()));
            }
        }
    }
}
