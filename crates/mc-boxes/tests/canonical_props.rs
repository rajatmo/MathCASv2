//! Properties of the canonical box text (design 08, section 4.4) and the linear text (4.6) over random trees and
//! texts: `boxes(text(b)) == b`, `text(boxes(t)) == t`, and the canonical and linear texts read to the same terms.
//! Seeded and bounded; `MC_BOXES_SEED` and `MC_BOXES_CASES` override the defaults.
use mc_api::RefStore;
use mc_boxes::{from_text, linear_text, notation, to_text, BigOp, Item, Node, Row};

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
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[self.below(xs.len())]
    }
}

fn seed() -> u64 {
    std::env::var("MC_BOXES_SEED").ok().and_then(|s| s.parse().ok()).unwrap_or(0x5eed_b0e5_1234_5678)
}

fn cases(default: usize) -> usize {
    std::env::var("MC_BOXES_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(default)
}

fn progress(name: &str, k: usize, n: usize) {
    if n >= 10 && (k + 1).is_multiple_of(n / 10) {
        eprintln!("{name} [{}/{n} {}%]", k + 1, (k + 1) * 100 / n);
    }
}

/// A random box (`depth` bounds the nesting); slots come from `slot`.
fn random_node(r: &mut Rng, depth: usize, slot: &mut dyn FnMut(&mut Rng, usize) -> Row) -> Node {
    let d = depth - 1;
    match r.below(12) {
        0 => Node::Fraction { num: slot(r, d), den: slot(r, d) },
        1 => Node::Superscript { base: slot(r, d), exp: slot(r, d) },
        2 => Node::Subscript { base: slot(r, d), index: slot(r, d) },
        3 => Node::Radical { radicand: slot(r, d), index: if r.below(2) == 0 { None } else { Some(slot(r, d)) } },
        4 => Node::Overscript { base: slot(r, d), over: slot(r, d) },
        5 => Node::Underscript { base: slot(r, d), under: slot(r, d) },
        6 => Node::Underoverscript { base: slot(r, d), under: slot(r, d), over: slot(r, d) },
        7 => {
            let (h, w) = (1 + r.below(3), 1 + r.below(3));
            Node::Grid { rows: (0..h).map(|_| (0..w).map(|_| slot(r, d)).collect()).collect() }
        }
        8 => Node::Graphic { path: ["a.png", "dir/b \"c\".png", "\\x\u{e9}"][r.below(3)].to_string() },
        _ => {
            let op = [BigOp::Sum, BigOp::Product, BigOp::Integral, BigOp::Limit, BigOp::Derivative][r.below(5)];
            let (lower, upper) = match op {
                BigOp::Limit => (Some(slot(r, d)), None),
                BigOp::Derivative => (None, None),
                _ if r.below(2) == 0 => (None, None),
                _ => (Some(slot(r, d)), Some(slot(r, d))),
            };
            Node::BigOp { op, body: slot(r, d), var: slot(r, d), lower, upper }
        }
    }
}

/// A random row of "hostile" text: separators, brackets in balanced groups, strings, spaces, boxes.
fn hostile_row(r: &mut Rng, depth: usize) -> Row {
    let mut items = Vec::new();
    for _ in 0..1 + r.below(4) {
        match r.below(10) {
            0 if depth > 0 => items.push(Item::Box(Box::new(random_node(r, depth, &mut hostile_row)))),
            1 => items.push(Item::Placeholder),
            2 | 3 => {
                let (o, c) = [('(', ')'), ('[', ']'), ('{', '}')][r.below(3)];
                items.push(Item::Char(o));
                items.extend(hostile_row(r, depth.saturating_sub(1)).0);
                items.push(Item::Char(c));
            }
            _ => items.extend(Row::text(r.pick(&["x", ", ", ",", " ", "->", "-", ">", "\"a)\"", "\"(\"", "ab", "^", "/", "_"])).0),
        }
    }
    Row(items)
}

/// Whether some row's own characters (boxes replaced by a neutral letter) already spell a box: the documented
/// ambiguity of a user typing a canonical form by hand, which reads back as the box.
/// A row inside slot delimiters counts with them: a numerator typed as `(sqrt(x))` reads as `((sqrt(x)))`, a box.
fn typed_shape(row: &Row) -> bool {
    let own: String = row.0.iter().map(|i| if let Item::Char(c) = i { *c } else { 'X' }).collect();
    let has_box = |t: &str| from_text(t).0.iter().any(|i| matches!(i, Item::Box(_)));
    has_box(&own)
        || has_box(&format!("({own})"))
        || row.0.iter().any(|i| matches!(i, Item::Box(b) if b.slots().into_iter().any(typed_shape)))
}

#[test]
fn boxes_of_text_of_boxes_is_identity() {
    let mut r = Rng(seed());
    let n = cases(3000);
    let mut skipped = 0;
    for k in 0..n {
        progress("boxes(text(b))", k, n);
        let row = hostile_row(&mut r, 4);
        if typed_shape(&row) {
            skipped += 1;
            continue;
        }
        let t = to_text(&row);
        let back = from_text(&t);
        assert_eq!(back, row, "case {k}: {t}\n{}\n{}", notation(&row), notation(&back));
        assert_eq!(to_text(&back), t);
    }
    assert!(skipped < n / 10, "{skipped} of {n} skipped");
}

#[test]
fn text_of_boxes_of_text_is_identity() {
    let mut r = Rng(seed() ^ 0x9e37_79b9);
    let n = cases(3000);
    let bits = ["((", "))", "(", ")", "/", "^", "[", "]", ", ", ",", "\"", "#", "\n", "->", "□", "x", "sqrt", "subscript",
        "sum", "limit", "image", "integrate", "d", "root", " ", "\\", "`"];
    for k in 0..n {
        progress("text(boxes(t))", k, n);
        let t: String = (0..r.below(30)).map(|_| r.pick(&bits)).collect();
        assert_eq!(to_text(&from_text(&t)), t, "case {k}");
    }
}

/// A random valid expression row (it reads without errors whatever the boxes inside it).
fn expr_row(r: &mut Rng, depth: usize) -> Row {
    let mut items = Vec::new();
    for t in 0..1 + r.below(3) {
        if t > 0 {
            items.extend(Row::text(r.pick(&[" + ", " - ", "*", " / ", "^"])).0);
        }
        match r.below(8) {
            0 | 1 if depth > 0 => {
                let n = random_node(r, depth, &mut |r, d| expr_row(r, d));
                // graphics are not values; keep the expression evaluable as text
                if !matches!(n, Node::Graphic { .. }) {
                    items.push(Item::Box(Box::new(n)));
                    continue;
                }
                items.extend(Row::text("x").0);
            }
            2 => {
                items.extend(Row::text("f(").0);
                items.extend(expr_row(r, depth.saturating_sub(1)).0);
                items.push(Item::Char(')'));
            }
            3 => {
                items.push(Item::Char('('));
                items.extend(expr_row(r, depth.saturating_sub(1)).0);
                items.push(Item::Char(')'));
            }
            4 => items.extend(Row::text(r.pick(&["2", "3.5", "10"])).0),
            5 => {
                items.push(Item::Char('-'));
                items.extend(Row::text(r.pick(&["a", "k"])).0);
            }
            _ => items.extend(Row::text(r.pick(&["x", "y", "a", "b", "k", "n"])).0),
        }
    }
    Row(items)
}

#[test]
fn linear_text_reads_to_the_same_terms() {
    let store = RefStore::new();
    let printer = mc_syntax::Printer::new(&store);
    let mut r = Rng(seed() ^ 0x1234);
    let n = cases(400);
    let mut checked = 0;
    for k in 0..n {
        progress("linear terms", k, n);
        let row = expr_row(&mut r, 3);
        let canonical = to_text(&row);
        let linear = linear_text(&row);
        let a = mc_syntax::read(&store, &canonical);
        let Ok(a) = a else {
            // juxtaposed boxes can make invalid text (`((a)/(b))((c)/(d))` is a call on a quotient, fine, but a
            // number after a box is not); the linear text must then be invalid too
            assert!(mc_syntax::read(&store, &linear).is_err(), "case {k}: {canonical} invalid but {linear} reads");
            continue;
        };
        let b = mc_syntax::read(&store, &linear).unwrap_or_else(|e| panic!("case {k}: {canonical} -> {linear}: {e:?}"));
        assert_eq!(printer.program(&a), printer.program(&b), "case {k}: {canonical} -> {linear}");
        checked += 1;
    }
    assert!(checked > n / 2, "{checked} of {n} rows were valid");
}

#[test]
fn mapped_text_places_every_item() {
    let mut r = Rng(seed() ^ 0x5151);
    let n = cases(1500);
    for k in 0..n {
        progress("to_text_mapped", k, n);
        let row = hostile_row(&mut r, 4);
        let (t, map) = mc_boxes::to_text_mapped(&row);
        assert_eq!(t, to_text(&row), "case {k}");
        // every character item of every row sits at its offset
        let mut stack: Vec<(Vec<mc_boxes::Step>, &mc_boxes::Row)> = vec![(vec![], &row)];
        while let Some((path, rw)) = stack.pop() {
            for (i, it) in rw.0.iter().enumerate() {
                let o = map.offset(&path, i).unwrap_or_else(|| panic!("case {k}: no offset for {path:?} {i}"));
                match it {
                    Item::Char(c) => assert!(t[o..].starts_with(*c), "case {k}: {c:?} at {o} in {t:?}"),
                    Item::Placeholder => assert!(t[o..].starts_with('□')),
                    Item::Box(b) => {
                        assert!(t[o..].starts_with("(("), "case {k}");
                        for (s, slot) in b.slots().into_iter().enumerate() {
                            let mut p = path.clone();
                            p.push(mc_boxes::Step { item: i, slot: s });
                            stack.push((p, slot));
                        }
                    }
                }
            }
            assert!(map.offset(&path, rw.len()).is_some());
        }
    }
}
