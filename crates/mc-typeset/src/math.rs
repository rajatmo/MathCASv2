//! The notebook's one look (round nb-ui, decision NBUI.5): linear text in the cell's font with exactly three
//! two-dimensional boxes, the square root, the fraction and the power. [`math_text`] reads a text (an output's printed
//! form) and keeps every character as it is, except that `sqrt(x)`, `a/b` and `x^n` become the three boxes, grouped
//! as the text parses (`a/b*c` is (a/b)·c, `2^3^4` is 2^(3^4), `-x^2` is −(x²)) and without the parentheses that only
//! group a slot (`(x+1)/(x-1)` draws no parentheses around its numerator and denominator, `x^(1/2)` none around its
//! exponent); a power's base keeps its parentheses, which still group it.
//!
//! The drawing reads as written mathematics (NBUI.7): a power `^(1/2)` is a square root (`2^(1/2)` is √2), `^(-1/2)`
//! one over a root; a product's factors with negative integer powers go below a fraction bar (`3*x^-1*y` is 3y over
//! x, `y^-1` is 1/y); and `*` between factors is a thin space where juxtaposition reads unambiguously (before a name
//! or a box: `3*x^2` is 3x²), kept between numbers (`2*3`) and before a parenthesis. Every item still maps back to
//! the bytes it stands for ([`SourceMap`]), so carets, selections, copying and Find work on the printed text.
//!
//! A value shown with a label (round 13, gap G80): an output that is a `labeled(x, l)` or `legended(x, l)` call, or
//! a list of such calls among other elements, draws each call as a [`Node::Labeled`]: `x` typeset as above, the
//! label as its characters (a string without its quotes) below it or at the side the call names, a legend at the
//! right with a coloured swatch before each entry of `line_legend` / `swatch_legend` / `point_legend`. The box
//! stands for the whole call's bytes, its value slot for the value's, the label's characters for their own (a swatch
//! for its colour's), so selecting and copying work as for any output. Forms it does not know stay text.
//!
//! [`mc_boxes::restrict`] turns any box tree into one with only those three boxes (older typeset notebooks): every
//! other box becomes its linear text.
use crate::printed::{SourceMap, Typeset};
#[cfg(test)]
use mc_boxes::linear_text;
use mc_boxes::{Item, Node, Row, RowPath, Side, Step};
use mc_syntax::ast::{Expr, ExprKind, MulOp, Stmt, StmtKind};
use std::collections::HashMap;
use std::ops::Range;

/// A two-dimensional construct found in the text: its whole byte range and its slots.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Construct {
    kind: Kind,
    whole: Range<usize>,
    slots: Vec<Vec<Piece>>,
    /// the label of a [`Kind::Labeled`] (its value is the one slot)
    label: Option<Box<LabelSpec>>,
}

/// The label of a `labeled` / `legended` call as drawn: its characters with the bytes each stands for, its side,
/// and which characters are swatches of which colour.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LabelSpec {
    side: Side,
    chars: Vec<(char, Range<usize>)>,
    swatches: Vec<(usize, [u8; 3])>,
}

/// A part of a slot: a stretch of the text (built with the constructs inside it), a character the drawing adds (a
/// `1` over a root, a `*` between factors moved into a numerator) standing for `span`, or a box of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(Range<usize>),
    Sym(char, Range<usize>),
    Box(Construct),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Sqrt,
    Fraction,
    Power,
    Labeled,
}

impl Construct {
    fn plain(kind: Kind, whole: Range<usize>, slots: Vec<Range<usize>>) -> Construct {
        Construct { kind, whole, slots: slots.into_iter().map(|s| vec![Piece::Text(s)]).collect(), label: None }
    }

    /// A construct of `kind` with these slots.
    fn of(kind: Kind, whole: Range<usize>, slots: Vec<Vec<Piece>>) -> Construct {
        Construct { kind, whole, slots, label: None }
    }

    /// Slots that would draw nothing (a construct is kept only when every slot has something).
    fn has_empty_slot(&self) -> bool {
        self.slots.iter().any(|s| s.iter().all(|p| matches!(p, Piece::Text(r) if r.is_empty())))
    }
}

/// `text` with its square roots, fractions and powers as boxes and every other character as itself.
pub fn math_text(text: &str) -> Typeset {
    let parsed = mc_syntax::parse(text);
    let mut found = Vec::new();
    if parsed.errors.is_empty() {
        let mut stack: Vec<&Expr> = Vec::new();
        for s in &parsed.program.stmts {
            stmt_exprs(s, &mut stack);
        }
        // an explicit stack: expressions nest as deep as the parser allows
        while let Some(e) = stack.pop() {
            constructs(text, e, &mut found);
            children(e, &mut stack);
        }
        found.extend(labeled_calls(text, &parsed.program.stmts));
        absorb_coefficient_parens(text, &mut found);
    }
    // the constructs by their start, longest first (an enclosing fraction before the one in its numerator)
    let mut by_start: HashMap<usize, Vec<Construct>> = HashMap::new();
    for c in found {
        // a box never spans lines, and its slots are never empty
        if text[c.whole.clone()].contains('\n') || c.has_empty_slot() {
            continue;
        }
        by_start.entry(c.whole.start).or_default().push(c);
    }
    for v in by_start.values_mut() {
        v.sort_by_key(|c| std::cmp::Reverse(c.whole.len()));
    }
    let (row, map) = build(text, 0..text.len(), &by_start, 0);
    Typeset { row, message: false, map }
}

/// The row of `range` of `text` and its map: characters as themselves, constructs starting inside as boxes.
fn build(text: &str, range: Range<usize>, at: &HashMap<usize, Vec<Construct>>, depth: usize) -> (Row, SourceMap) {
    let mut items = Vec::new();
    let mut map = SourceMap::default();
    let mut i = range.start;
    while i < range.end {
        let fits = at.get(&i).and_then(|v| v.iter().find(|c| c.whole.end <= range.end));
        if let (Some(c), true) = (fits, depth < mc_boxes::MAX_NESTING) {
            push_box(text, c, at, depth, &mut items, &mut map);
            i = c.whole.end;
            continue;
        }
        let Some(ch) = text[i..].chars().next() else { break };
        map.spans.push(i..i + ch.len_utf8());
        items.push(Item::Char(ch));
        i += ch.len_utf8();
    }
    let mut row = Row(items);
    thin_products(&mut row);
    (row, map)
}

/// Append construct `c` as a box item.
fn push_box(text: &str, c: &Construct, at: &HashMap<usize, Vec<Construct>>, depth: usize, items: &mut Vec<Item>, map: &mut SourceMap) {
    let mut slots: Vec<(Row, SourceMap)> = c.slots.iter().map(|s| build_pieces(text, s, at, depth + 1)).collect();
    if let Some(l) = &c.label {
        // the label's characters as they are (no products made thin, no boxes)
        let row = Row(l.chars.iter().map(|(ch, _)| Item::Char(*ch)).collect());
        let spans = l.chars.iter().map(|(_, r)| r.clone()).collect();
        slots.push((row, SourceMap { spans, boxes: HashMap::new() }));
    }
    let mut rows = slots.iter().map(|(r, _)| r.clone());
    let mut next = || rows.next().unwrap_or_default();
    let node = match (c.kind, &c.label) {
        (Kind::Sqrt, _) => Node::Radical { radicand: next(), index: None },
        (Kind::Fraction, _) => Node::Fraction { num: next(), den: next() },
        (Kind::Labeled, Some(l)) => Node::Labeled {
            value: next(),
            label: next(),
            side: l.side,
            swatches: l.swatches.clone(),
            text: text.get(c.whole.clone()).unwrap_or("").to_string(),
        },
        (Kind::Power | Kind::Labeled, _) => Node::Superscript { base: next(), exp: next() },
    };
    map.boxes.insert(items.len(), slots.into_iter().map(|(_, m)| m).collect());
    map.spans.push(c.whole.clone());
    items.push(Item::Box(Box::new(node)));
}

/// A slot's row from its pieces.
fn build_pieces(text: &str, pieces: &[Piece], at: &HashMap<usize, Vec<Construct>>, depth: usize) -> (Row, SourceMap) {
    let mut items = Vec::new();
    let mut map = SourceMap::default();
    for p in pieces {
        match p {
            Piece::Text(r) => {
                let (row, m) = build(text, r.clone(), at, depth);
                let base = items.len();
                for (k, b) in m.boxes {
                    map.boxes.insert(base + k, b);
                }
                map.spans.extend(m.spans);
                items.extend(row.0);
            }
            Piece::Sym(c, span) => {
                map.spans.push(span.clone());
                items.push(Item::Char(*c));
            }
            Piece::Box(c) => push_box(text, c, at, depth, &mut items, &mut map),
        }
    }
    let mut row = Row(items);
    thin_products(&mut row);
    (row, map)
}

/// `*` between factors becomes a thin space where juxtaposition reads unambiguously: before a name or a box (`3x²`,
/// `2√2`, `x y`); it stays before a number (`2*3`, `x*2`), a parenthesis or anything else.
fn thin_products(row: &mut Row) {
    let n = row.0.len();
    for k in 0..n {
        if !matches!(row.0[k], Item::Char('*')) || k == 0 || k + 1 >= n {
            continue;
        }
        let prev_ok = match &row.0[k - 1] {
            Item::Char(c) => c.is_alphanumeric() || *c == ')' || *c == '_' || *c == '.',
            Item::Box(_) => true,
            Item::Placeholder => false,
        };
        let next_ok = starts_like_a_name(&row.0[k + 1]);
        if prev_ok && next_ok {
            row.0[k] = Item::Char('\u{2009}');
        }
    }
}

/// Whether an item begins with a letter (a name, or a box that is drawn starting with one or with a root or a bar).
fn starts_like_a_name(it: &Item) -> bool {
    match it {
        Item::Char(c) => c.is_alphabetic() || *c == '√',
        Item::Box(b) => match &**b {
            Node::Radical { .. } | Node::Fraction { .. } => true,
            Node::Superscript { base, .. } => base.0.first().is_some_and(starts_like_a_name),
            _ => false,
        },
        Item::Placeholder => false,
    }
}

/// The text of `r`, trimmed.
fn text_of<'t>(text: &'t str, r: &Range<usize>) -> &'t str {
    text.get(r.clone()).unwrap_or("").trim()
}

/// An exponent's text without parentheses around it.
fn bare(s: &str) -> &str {
    let mut s = s.trim();
    while s.len() >= 2 && s.starts_with('(') && s.ends_with(')') && matching(s, 0) == Some(s.len() - 1) {
        s = s[1..s.len() - 1].trim();
    }
    s
}

/// A negative integer exponent's magnitude: `-1` gives `Some("1")`'s range in the text.
fn negative_integer(text: &str, exp: &Range<usize>) -> Option<Range<usize>> {
    let t = text.get(exp.clone())?;
    let lead = t.len() - t.trim_start().len();
    let s = t.trim();
    let digits = s.strip_prefix('-')?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let start = exp.start + lead + 1;
    Some(start..start + digits.len())
}

/// The parts of a power `base^exp` (ranges in the text): the base's contents and where it starts with any
/// parentheses, the exponent's contents, and the end of the whole power.
struct PowParts {
    start: usize,
    end: usize,
    /// the base with its parentheses (as it is drawn in a power)
    base: Range<usize>,
    /// the base without them (as it is drawn under a root)
    inner: Range<usize>,
    exp: Range<usize>,
}

fn pow_parts(text: &str, b: &Expr, x: &Expr) -> Option<PowParts> {
    let caret = find_op(text, b.span.end as usize, x.span.start as usize, b'^')?;
    let (inner, start) = slot_before(text, b.span.start as usize, caret);
    let base = if inner.start > start { start..trim(text, start..caret).end } else { inner.clone() };
    let (exp, end) = slot_after(text, caret + 1, x.span.end as usize);
    Some(PowParts { start, end: end.max(start), base, inner, exp })
}

/// A power as drawn: `^(1/2)` a square root, `^(-1/2)` one over a root, a negative integer power one over the
/// positive power, else a superscript.
fn power(text: &str, p: &PowParts) -> Construct {
    let whole = p.start..p.end;
    let e = bare(text.get(p.exp.clone()).unwrap_or(""));
    let root = || Construct::plain(Kind::Sqrt, whole.clone(), vec![p.inner.clone()]);
    match e {
        "1/2" => root(),
        "-1/2" => Construct::of(
            Kind::Fraction,
            whole.clone(),
            vec![vec![Piece::Sym('1', whole.clone())], vec![Piece::Box(root())]],
        ),
        _ => match negative_integer(text, &p.exp) {
            Some(mag) => Construct::of(
                Kind::Fraction,
                whole.clone(),
                vec![vec![Piece::Sym('1', whole.clone())], vec![positive_power(text, p, &mag)]],
            ),
            None => Construct::plain(Kind::Power, whole, vec![p.base.clone(), p.exp.clone()]),
        },
    }
}

/// The denominator piece of a factor `base^-mag`: the base alone for `-1`, else `base^mag`.
fn positive_power(text: &str, p: &PowParts, mag: &Range<usize>) -> Piece {
    if text_of(text, mag) == "1" {
        // a lone base needs no parentheses below a bar
        Piece::Text(p.inner.clone())
    } else {
        Piece::Box(Construct::plain(Kind::Power, p.start..p.end, vec![p.base.clone(), mag.clone()]))
    }
}

/// The constructs `e` makes: a square root call, a power, a product's fractions (one per division: `a*b/c*d/e` is
/// ((a·b)/c·d)/e, each numerator running from the product's start to its division; or, for a product without
/// divisions that has factors with negative integer powers, one fraction with those factors below the bar), or a
/// rational literal.
fn constructs(text: &str, e: &Expr, out: &mut Vec<Construct>) {
    let (lo, hi) = (e.span.start as usize, e.span.end as usize);
    match &e.kind {
        ExprKind::Call(f, args) => {
            let is_sqrt = matches!(&f.kind, ExprKind::Name(n) if n == "sqrt");
            if !is_sqrt || args.len() != 1 || args[0].name.is_some() {
                return;
            }
            let open = skip_space(text, f.span.end as usize);
            if text.as_bytes().get(open) != Some(&b'(') {
                return;
            }
            if let Some(close) = matching(text, open) {
                out.push(Construct::plain(Kind::Sqrt, f.span.start as usize..close + 1, vec![trim(text, open + 1..close)]));
            }
        }
        ExprKind::Pow(b, x) => {
            if let Some(p) = pow_parts(text, b, x) {
                out.push(power(text, &p));
            }
        }
        ExprKind::Mul(factors) => {
            if factors.iter().all(|(op, _)| *op == MulOp::Times) {
                if let Some(c) = ratio(text, factors) {
                    out.push(c);
                }
                return;
            }
            let first = factors.first().map_or(lo, |f| f.1.span.start as usize);
            for k in 1..factors.len() {
                if factors[k].0 != MulOp::Div {
                    continue;
                }
                let prev = &factors[k - 1].1;
                let den = &factors[k].1;
                let Some(slash) = find_op(text, prev.span.end as usize, den.span.start as usize, b'/') else { continue };
                let (num, start) = slot_before(text, first, slash);
                let (d, end) = slot_after(text, slash + 1, den.span.end as usize);
                out.push(Construct::plain(Kind::Fraction, start..end, vec![num, d]));
            }
        }
        ExprKind::Num(_) => {
            // a rational literal (`1/2`, `-3/4`): a fraction of its two integers
            // (its span may include the parentheses around it)
            let (mut lo, mut hi) = (lo, hi.min(text.len()));
            while hi > lo + 1 && text.as_bytes()[lo] == b'(' && matching(text, lo) == Some(hi - 1) {
                lo += 1;
                hi -= 1;
            }
            if let Some(k) = text[lo..hi].find('/') {
                let slash = lo + k;
                out.push(Construct::plain(Kind::Fraction, lo..hi, vec![trim(text, lo..slash), trim(text, slash + 1..hi)]));
            }
        }
        _ => {}
    }
}

/// A product whose factors include negative integer powers, as one fraction: the other factors above the bar in
/// their order (`1` when there are none), the negative powers below it with positive exponents, each `*` between
/// two factors on the same side kept from the text (it becomes a thin space where it reads so).
fn ratio(text: &str, factors: &[(MulOp, Expr)]) -> Option<Construct> {
    // each factor's range and, for a negative integer power, its parts and magnitude
    let mut parts: Vec<(Range<usize>, Option<NegPower>)> = Vec::new();
    for (_, f) in factors {
        let neg = match &f.kind {
            ExprKind::Pow(b, x) => pow_parts(text, b, x).and_then(|p| negative_integer(text, &p.exp).map(|m| (p, m))),
            _ => None,
        };
        let range = match &neg {
            Some((p, _)) => p.start..p.end,
            None => {
                let (lo, hi) = (f.span.start as usize, f.span.end as usize);
                widen_parens(text, lo..hi)
            }
        };
        parts.push((range, neg));
    }
    if parts.iter().all(|(_, n)| n.is_none()) || parts.len() < 2 && parts.iter().all(|(_, n)| n.is_some()) {
        // no negative power, or a lone power (drawn by `power`)
        return None;
    }
    let whole = parts.first()?.0.start..parts.last()?.0.end;
    // the `*` (or space) before factor k, standing for itself when the factor moves
    let sep = |k: usize| -> Piece {
        let from = parts[k - 1].0.end;
        let to = parts[k].0.start;
        match text.get(from..to).and_then(|s| s.find('*')) {
            Some(i) => Piece::Sym('*', from + i..from + i + 1),
            None => Piece::Sym('*', from..to),
        }
    };
    let (mut num, mut den): (Vec<Piece>, Vec<Piece>) = (Vec::new(), Vec::new());
    for (k, (range, neg)) in parts.iter().enumerate() {
        let side = if neg.is_some() { &mut den } else { &mut num };
        if !side.is_empty() {
            side.push(sep(k));
        }
        match neg {
            Some((p, mag)) => side.push(positive_power(text, p, mag)),
            None => side.push(Piece::Text(range.clone())),
        }
    }
    if num.is_empty() {
        num.push(Piece::Sym('1', whole.clone()));
    }
    Some(Construct::of(Kind::Fraction, whole, vec![num, den]))
}

/// A factor `base^-n`: its parts and the range of `n`.
type NegPower = (PowParts, Range<usize>);

/// A range grown over parentheses that enclose it exactly (a factor whose span leaves them out).
fn widen_parens(text: &str, r: Range<usize>) -> Range<usize> {
    let b = text.as_bytes();
    let (mut lo, mut hi) = (r.start, r.end);
    while lo > 0 && hi < b.len() && b[lo - 1] == b'(' && matching(text, lo - 1) == Some(hi) {
        lo -= 1;
        hi += 1;
    }
    lo..hi
}

/// The byte of operator `op` between `from` and `to` (spaces and closing parentheses may come before it).
fn find_op(text: &str, from: usize, to: usize, op: u8) -> Option<usize> {
    let b = text.as_bytes();
    let to = to.min(b.len());
    let mut i = from.min(to);
    // the operand may end in parentheses outside its span
    while i < to {
        match b[i] {
            c if c == op => return Some(i),
            b' ' | b')' | b'\t' => i += 1,
            _ => return None,
        }
    }
    // a span that already covers the operator's left side: search backwards is not needed; the operand before it
    // ended at `from`
    None
}

/// The slot before an operator at `op`, starting no earlier than `lo`: its contents (inside its parentheses when it
/// is parenthesised) and where the construct starts.
fn slot_before(text: &str, lo: usize, op: usize) -> (Range<usize>, usize) {
    let r = trim(text, lo..op);
    let b = text.as_bytes();
    if r.end > r.start && b[r.end - 1] == b')' {
        if let Some(open) = matching_back(text, r.end - 1) {
            if open <= r.start {
                return (trim(text, open + 1..r.end - 1), open.min(r.start));
            }
        }
    }
    // an operand whose span left out its opening parenthesis
    (r.clone(), r.start)
}

/// The slot after an operator, from `from` to the operand's end `hi`: its contents and where the construct ends.
fn slot_after(text: &str, from: usize, hi: usize) -> (Range<usize>, usize) {
    let start = skip_space(text, from);
    if text.as_bytes().get(start) == Some(&b'(') {
        if let Some(close) = matching(text, start) {
            if close + 1 >= hi {
                return (trim(text, start + 1..close), close + 1);
            }
        }
    }
    let end = hi.max(start);
    (trim(text, start..end), end)
}

fn skip_space(text: &str, mut i: usize) -> usize {
    let b = text.as_bytes();
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    i
}

fn trim(text: &str, r: Range<usize>) -> Range<usize> {
    let b = text.as_bytes();
    let (mut lo, mut hi) = (r.start.min(b.len()), r.end.min(b.len()));
    while lo < hi && (b[lo] == b' ' || b[lo] == b'\t') {
        lo += 1;
    }
    while hi > lo && (b[hi - 1] == b' ' || b[hi - 1] == b'\t') {
        hi -= 1;
    }
    lo..hi
}

/// The `)` matching the `(` at `open` (strings skipped).
fn matching(text: &str, open: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    let mut in_str = false;
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'(' if !in_str => depth += 1,
            b')' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\n' => return None,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The `(` matching the `)` at `close`.
fn matching_back(text: &str, close: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut i = close + 1;
    while i > 0 {
        i -= 1;
        match b[i] {
            b')' => depth += 1,
            b'(' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\n' => return None,
            _ => {}
        }
    }
    None
}

// ---- labeled and legended values ----

/// The longest value text drawn with its label (a longer value is shown as printed: a box cannot break across the
/// lines of its row, and a value this long reads better wrapped in the output's flow).
const LABELED_VALUE_MAX: usize = 20_000;
/// The longest label that is not a string (a name, a number, a short formula) drawn as a label.
const LABEL_TERM_MAX: usize = 100;

/// The swatch characters of legends: a short line (`line_legend`), a square (`swatch_legend`), a dot
/// (`point_legend`).
const LINE_SWATCH: &str = "━━";
const SQUARE_SWATCH: &str = "■";
const POINT_SWATCH: &str = "●";

/// The `labeled` / `legended` calls of an output drawn with their labels: the output itself, or the elements of an
/// output that is a list.
fn labeled_calls(text: &str, stmts: &[Stmt]) -> Vec<Construct> {
    let [s] = stmts else { return vec![] };
    let StmtKind::Expr(e) = &s.kind else { return vec![] };
    match &e.kind {
        ExprKind::List(v) => v.iter().filter_map(|x| labeled_call(text, x)).collect(),
        _ => labeled_call(text, e).into_iter().collect(),
    }
}

/// `e` as a labeled value: `labeled(x, l)`, `labeled(x, l, side)` (a side name or a list of one),
/// `legended(x, l)` with a label, a list of labels or `line_legend([colours], [labels])` (also `swatch_legend`,
/// `point_legend`). Named arguments (options) are not drawn. `None` for any other form.
fn labeled_call(text: &str, e: &Expr) -> Option<Construct> {
    let ExprKind::Call(f, args) = &e.kind else { return None };
    let ExprKind::Name(name) = &f.kind else { return None };
    let pos: Vec<&Expr> = args.iter().filter(|a| a.name.is_none()).map(|a| &a.value).collect();
    let value = *pos.first()?;
    let v = trim(text, value.span.start as usize..value.span.end as usize);
    if v.is_empty() || v.len() > LABELED_VALUE_MAX || is_picture(value) {
        return None;
    }
    let (side, chars, swatches) = match (name.as_str(), pos.as_slice()) {
        ("labeled", [_, l]) => (Side::Below, label_chars(text, l)?, vec![]),
        ("labeled", [_, l, at]) => (side_of(at)?, label_chars(text, l)?, vec![]),
        ("legended", [_, l]) => {
            let (chars, swatches) = legend(text, l)?;
            (Side::Right, chars, swatches)
        }
        _ => return None,
    };
    if chars.is_empty() {
        return None;
    }
    let whole = trim(text, e.span.start as usize..e.span.end as usize);
    let mut c = Construct::of(Kind::Labeled, whole, vec![vec![Piece::Text(v)]]);
    c.label = Some(Box::new(LabelSpec { side, chars, swatches }));
    Some(c)
}

/// Whether a value is a picture (drawn by the graphics crate, not here).
fn is_picture(e: &Expr) -> bool {
    let ExprKind::Call(f, _) = &e.kind else { return false };
    matches!(&f.kind, ExprKind::Name(n) if matches!(n.as_str(), "graphics" | "graphics3d" | "image"))
}

/// The side a `labeled` call names: `top`/`above`, `bottom`/`below`, `left`/`before`, `right`/`after`, alone or in
/// a list of one.
fn side_of(e: &Expr) -> Option<Side> {
    match &e.kind {
        ExprKind::Name(n) => match n.as_str() {
            "top" | "above" => Some(Side::Above),
            "bottom" | "below" => Some(Side::Below),
            "left" | "before" => Some(Side::Left),
            "right" | "after" => Some(Side::Right),
            _ => None,
        },
        ExprKind::List(v) if v.len() == 1 => side_of(&v[0]),
        _ => None,
    }
}

/// A label's characters with the bytes each stands for: a string's contents without its quotes (escapes read), or
/// the printed text of a short term without calls or lists (a name, a number, `x^2`).
fn label_chars(text: &str, e: &Expr) -> Option<Vec<(char, Range<usize>)>> {
    let r = trim(text, e.span.start as usize..e.span.end as usize);
    let src = text.get(r.clone())?;
    if let ExprKind::Str(_) = &e.kind {
        return string_chars(src, r.start);
    }
    if src.len() > LABEL_TERM_MAX || src.contains('\n') || has_call_or_list(e) {
        return None;
    }
    Some(src.char_indices().map(|(i, c)| (c, r.start + i..r.start + i + c.len_utf8())).collect())
}

/// Whether a term holds a call, a list, an index or a block anywhere (such labels are not drawn as text).
fn has_call_or_list(e: &Expr) -> bool {
    let mut stack = vec![e];
    while let Some(x) = stack.pop() {
        if matches!(x.kind, ExprKind::Call(..) | ExprKind::List(_) | ExprKind::Block(_) | ExprKind::Index(..)) {
            return true;
        }
        children(x, &mut stack);
    }
    false
}

/// The characters of a string literal `src` starting at byte `at`: `"..."` with `\"`, `\\`, `\n` and `\t` read
/// (a tab as a space; any other escape kept as written), or a raw `r"..."` as it is.
fn string_chars(src: &str, at: usize) -> Option<Vec<(char, Range<usize>)>> {
    let (raw, open) = if src.starts_with("r\"") {
        (true, 2)
    } else if src.starts_with('"') {
        (false, 1)
    } else {
        return None;
    };
    if src.len() < open + 1 || !src.ends_with('"') {
        return None;
    }
    let body = &src[open..src.len() - 1];
    let base = at + open;
    let mut out = Vec::with_capacity(body.len());
    let mut it = body.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c == '\\' && !raw {
            let shown = match it.peek().map(|&(_, n)| n) {
                Some('"') => Some('"'),
                Some('\\') => Some('\\'),
                Some('n') => Some('\n'),
                Some('t') => Some(' '),
                _ => None,
            };
            if let Some(s) = shown {
                it.next();
                out.push((s, base + i..base + i + 2));
                continue;
            }
        }
        out.push((c, base + i..base + i + c.len_utf8()));
    }
    Some(out)
}

/// The characters and swatches of a legend's label row.
type LegendRow = (Vec<(char, Range<usize>)>, Vec<(usize, [u8; 3])>);

/// A legend's characters (its entries one per line, each after its swatch when it has a colour) and its swatches.
fn legend(text: &str, e: &Expr) -> Option<LegendRow> {
    let (colours, labels, swatch): (&[Expr], &[Expr], &str) = match &e.kind {
        ExprKind::List(v) => (&[], v.as_slice(), ""),
        ExprKind::Call(f, args) => {
            let ExprKind::Name(n) = &f.kind else { return None };
            let swatch = match n.as_str() {
                "line_legend" => LINE_SWATCH,
                "swatch_legend" => SQUARE_SWATCH,
                "point_legend" => POINT_SWATCH,
                _ => return None,
            };
            let lists: Vec<&[Expr]> = args
                .iter()
                .filter(|a| a.name.is_none())
                .map(|a| match &a.value.kind {
                    ExprKind::List(v) => Some(v.as_slice()),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            match lists.as_slice() {
                [c, l] => (*c, *l, swatch),
                [l] => (&[], *l, ""),
                _ => return None,
            }
        }
        _ => return Some((label_chars(text, e)?, vec![])),
    };
    if labels.is_empty() {
        return None;
    }
    let mut chars = Vec::new();
    let mut swatches = Vec::new();
    for (k, l) in labels.iter().enumerate() {
        let entry = label_chars(text, l)?;
        if k > 0 {
            // the line break stands for the separator between the entries
            let from = labels[k - 1].span.end as usize;
            let to = (l.span.start as usize).max(from);
            chars.push(('\n', from..to));
        }
        if let Some((rgb, c)) = colours.get(k).and_then(|c| Some((colour(text, c)?, c))) {
            let cspan = trim(text, c.span.start as usize..c.span.end as usize);
            for ch in swatch.chars() {
                swatches.push((chars.len(), rgb));
                chars.push((ch, cspan.clone()));
            }
            chars.push((' ', cspan));
        }
        chars.extend(entry);
    }
    Some((chars, swatches))
}

/// The named colours (as the graphics crate reads them).
const NAMED: &[(&str, [u8; 3])] = &[
    ("black", [0, 0, 0]),
    ("white", [255, 255, 255]),
    ("gray", [128, 128, 128]),
    ("light_gray", [217, 217, 217]),
    ("dark_gray", [64, 64, 64]),
    ("red", [255, 0, 0]),
    ("green", [0, 255, 0]),
    ("blue", [0, 0, 255]),
    ("cyan", [0, 255, 255]),
    ("magenta", [255, 0, 255]),
    ("yellow", [255, 255, 0]),
    ("orange", [255, 128, 0]),
    ("purple", [128, 0, 128]),
    ("brown", [153, 102, 51]),
    ("pink", [255, 128, 128]),
];

/// A colour term's sRGB: a named colour, `rgb_color(r, g, b)`, `hue(h)` or `hue(h, s, b)`, `gray_level(g)` (each
/// with an optional opacity, which is not drawn), `"#rrggbb"`; components are number literals.
fn colour(text: &str, e: &Expr) -> Option<[u8; 3]> {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    match &e.kind {
        ExprKind::Name(n) => NAMED.iter().find(|(k, _)| k == n).map(|(_, c)| *c),
        ExprKind::Str(s) => {
            let h = s.strip_prefix('#').filter(|h| h.len() == 6)?;
            let v = u32::from_str_radix(h, 16).ok()?;
            Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
        }
        ExprKind::Call(f, args) => {
            let ExprKind::Name(n) = &f.kind else { return None };
            let v: Vec<f64> = args.iter().map(|a| number(text, &a.value)).collect::<Option<_>>()?;
            let rgb = match (n.as_str(), v.len()) {
                ("rgb_color", 3 | 4) => [v[0], v[1], v[2]],
                ("gray_level", 1 | 2) => [v[0]; 3],
                ("hue", 1) => hsb(v[0], 1.0, 1.0),
                ("hue", 3 | 4) => hsb(v[0], v[1], v[2]),
                _ => return None,
            };
            Some(rgb.map(byte))
        }
        _ => None,
    }
}

/// A colour from hue, saturation and brightness in [0, 1].
fn hsb(h: f64, s: f64, b: f64) -> [f64; 3] {
    let h = (h - h.floor()) * 6.0;
    let i = h.floor();
    let f = h - i;
    let (p, q, t) = (b * (1.0 - s), b * (1.0 - s * f), b * (1.0 - s * (1.0 - f)));
    match i as u8 {
        0 => [b, t, p],
        1 => [q, b, p],
        2 => [p, b, t],
        3 => [p, q, b],
        4 => [t, p, b],
        _ => [b, p, q],
    }
}

/// A number literal (a decimal, an integer, a rational `a/b`; negated or not) as a float.
fn number(text: &str, e: &Expr) -> Option<f64> {
    match &e.kind {
        ExprKind::Neg(x) => number(text, x).map(|v| -v),
        ExprKind::Mul(f) => match f.as_slice() {
            [(MulOp::Times, a), (MulOp::Div, b)] => Some(number(text, a)? / number(text, b)?),
            _ => None,
        },
        ExprKind::Num(_) => {
            let t = bare(text_of(text, &(e.span.start as usize..e.span.end as usize)));
            match t.split_once('/') {
                Some((a, b)) => Some(a.trim().parse::<f64>().ok()? / b.trim().parse::<f64>().ok()?),
                None => t.parse().ok(),
            }
        }
        _ => None,
    }
}

/// A label row of typeset output (the label slot of a [`Node::Labeled`]): its path, and its swatch characters by
/// item index with their sRGB colours. The painter draws the row in the label colour, the swatches in theirs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelRow {
    /// the path of the label row
    pub path: RowPath,
    /// (item index in the row, colour) of each swatch character
    pub swatches: Vec<(usize, [u8; 3])>,
}

/// The label rows of a box tree (at any depth), by path.
pub fn label_rows(row: &Row) -> Vec<LabelRow> {
    let mut out = Vec::new();
    let mut stack: Vec<(&Row, RowPath)> = vec![(row, Vec::new())];
    while let Some((r, path)) = stack.pop() {
        for (i, it) in r.0.iter().enumerate() {
            let Item::Box(b) = it else { continue };
            if let Node::Labeled { swatches, .. } = &**b {
                let mut p = path.clone();
                p.push(Step { item: i, slot: 1 });
                out.push(LabelRow { path: p, swatches: swatches.clone() });
            }
            for (k, s) in b.slots().into_iter().enumerate() {
                let mut p = path.clone();
                p.push(Step { item: i, slot: k });
                stack.push((s, p));
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn stmt_exprs<'a>(s: &'a Stmt, out: &mut Vec<&'a Expr>) {
    match &s.kind {
        StmtKind::Expr(e) => out.push(e),
        StmtKind::Let { value, .. } => out.push(value),
        StmtKind::Fn { params, options, condition, body, .. } => {
            out.extend(params.iter().chain(options).filter_map(|p| p.default.as_ref()));
            out.extend(condition.iter());
            out.push(body);
        }
        StmtKind::Rule { lhs, rhs, .. } => {
            out.push(lhs);
            out.push(rhs);
        }
        StmtKind::Module { body, .. } => {
            for s in body {
                stmt_exprs(s, out);
            }
        }
        StmtKind::Pub(s) => stmt_exprs(s, out),
        StmtKind::Sym { .. } | StmtKind::Import { .. } => {}
    }
}

/// The sub-expressions of `e`.
fn children<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    use ExprKind as K;
    match &e.kind {
        K::Num(_) | K::Str(_) | K::Name(_) | K::Pattern { .. } | K::Slot(_) => {}
        K::Typed(x, _) | K::Named(_, x) | K::Neg(x) | K::Not(x) | K::Prime(x, _) | K::SlotFn(x) => out.push(x),
        K::Add(v) => out.extend(v.iter().map(|(_, x)| x)),
        K::Mul(v) => out.extend(v.iter().map(|(_, x)| x)),
        K::Pow(a, b) | K::Binary(_, a, b) | K::Span(a, b) | K::While(a, b) => {
            out.push(a);
            out.push(b);
        }
        K::Nary(_, v) | K::List(v) => out.extend(v.iter()),
        K::Call(f, args) => {
            out.push(f);
            out.extend(args.iter().map(|a| &a.value));
        }
        K::Index(a, v) => {
            out.push(a);
            out.extend(v.iter());
        }
        K::Block(stmts) => {
            for s in stmts {
                stmt_exprs(s, out);
            }
        }
        K::Lambda(params, body) => {
            out.extend(params.iter().filter_map(|p| p.default.as_ref()));
            out.push(body);
        }
        K::If(c, t, f) => {
            out.push(c);
            out.push(t);
            out.extend(f.iter().map(|x| &**x));
        }
        K::For(_, a, b) => {
            out.push(a);
            out.push(b);
        }
        K::Match(x, arms) => {
            out.push(x);
            for a in arms {
                out.push(&a.pattern);
                out.push(&a.body);
            }
        }
    }
}

/// A rational coefficient keeps the parentheses its printed form needs (`(1/2)*pi`); drawn as a fraction it shows
/// its own extent, so the parentheses would stand around it as tall brackets. A fraction of two integers wrapped in
/// its own pair of parentheses and followed by `*` takes them into its box. Powers keep theirs (`(1/2)^n`).
fn absorb_coefficient_parens(text: &str, found: &mut [Construct]) {
    let bytes = text.as_bytes();
    let integer = |r: &[Piece]| match r {
        [Piece::Text(t)] => {
            let s = text[t.clone()].trim();
            let s = s.strip_prefix('-').unwrap_or(s);
            !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
        }
        _ => false,
    };
    for c in found.iter_mut() {
        if c.kind != Kind::Fraction || c.slots.len() != 2 || !c.slots.iter().all(|s| integer(s)) {
            continue;
        }
        let (lo, hi) = (c.whole.start, c.whole.end);
        if lo == 0 || hi >= bytes.len() || bytes[lo - 1] != b'(' || matching(text, lo - 1) != Some(hi) {
            continue;
        }
        let next = skip_space(text, hi + 1);
        if bytes.get(next) == Some(&b'*') && bytes.get(next + 1) != Some(&b'*') {
            c.whole = lo - 1..hi + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_boxes::notation;

    /// The boxes of `text` in the test notation.
    fn boxes(text: &str) -> String {
        // a thin space shows as `·`
        notation(&math_text(text).row).replace('\u{2009}', "·")
    }

    #[test]
    fn the_three_constructs_group_as_the_text_parses() {
        assert_eq!(boxes("1/2"), "(1/2)");
        assert_eq!(boxes("(x+1)/(x-1)"), "(x+1/x-1)");
        assert_eq!(boxes("a/b*c"), "(a/b)·c");
        assert_eq!(boxes("a*b/c"), "(a·b/c)");
        assert_eq!(boxes("a/b/c"), "((a/b)/c)");
        assert_eq!(boxes("-x^2"), "-(x^2)");
        assert_eq!(boxes("2^3^4"), "(2^(3^4))");
        assert_eq!(boxes("e^(i*pi)"), "(e^i·pi)");
        assert_eq!(boxes("x^-1"), "(1/x)");
        assert_eq!(boxes("(x+1)^2"), "((x+1)^2)");
        assert_eq!(boxes("(x^2 + y^-1)^(1/2)"), "√((x^2) + (1/y))");
        assert_eq!(boxes("sqrt(x^2 + 1/y)"), "√((x^2) + (1/y))");
        assert_eq!(boxes("sqrt(1/(1+x))"), "√((1/1+x))");
        assert_eq!(boxes("2*x + 3"), "2·x + 3");
    }

    /// NBUI.7: roots for half powers, negative powers below a bar, thin spaces for products.
    #[test]
    fn outputs_read_as_written_mathematics() {
        assert_eq!(boxes("2*2^(1/2)"), "2·√(2)");
        assert_eq!(boxes("(x^2 + y^-1)^(1/2)"), "√((x^2) + (1/y))");
        assert_eq!(boxes("x^(-1/2)"), "(1/√(x))");
        assert_eq!(boxes("y^-1"), "(1/y)");
        assert_eq!(boxes("a*b^-2"), "(a/(b^2))");
        assert_eq!(boxes("3*x^-1*y"), "(3·y/x)");
        assert_eq!(boxes("x^-1*y^-2"), "(1/x·(y^2))");
        assert_eq!(boxes("3*x^2 + 2*3 + 2*(a + b) + x*2"), "3·(x^2) + 2*3 + 2*(a + b) + x*2");
        // the source map stays right: the moved numerator factor and the denominator stand for their own bytes
        let t = "3*x^-1*y";
        let m = math_text(t);
        let num = &m.map.boxes[&0][0];
        let spans: Vec<&str> = num.spans.iter().map(|r| &t[r.clone()]).collect();
        assert_eq!(spans, ["3", "*", "y"]);
        let den = &m.map.boxes[&0][1];
        assert_eq!(&t[den.spans[0].clone()], "x");
        assert_eq!(&t[m.map.spans[0].clone()], t);
    }

    #[test]
    fn everything_else_stays_as_printed() {
        // no substitutions, no subscripts, grids or big operators: the characters as they are
        for t in ["sum(k, [k, 1, n])", "[[1, 2], [3, 4]]", "subscript(x, i)", "pi + x -> 2", "f(x) := x + 1", "\"a/b\""] {
            let m = math_text(t);
            assert_eq!(m.row.plain().as_deref(), Some(t), "{t}");
        }
        // a text that does not parse is all characters
        assert_eq!(math_text("1/(2").row.plain().as_deref(), Some("1/(2"));
    }

    #[test]
    fn every_item_maps_to_the_bytes_it_stands_for() {
        let t = "y = (a+b)/2 + sqrt(x)^3";
        let m = math_text(t);
        let top = &m.map;
        // the characters before the fraction map one to one
        assert_eq!(top.spans[0], 0..1);
        // the fraction stands for its whole text, its numerator's items for theirs (inside the parentheses)
        let frac = m.row.0.iter().position(|i| matches!(i, Item::Box(_))).unwrap();
        assert_eq!(&t[top.spans[frac].clone()], "(a+b)/2");
        let num = &top.boxes[&frac][0];
        assert_eq!(&t[num.spans[0].clone()], "a");
        // the power's base is the root
        let pow = m.row.0.iter().rposition(|i| matches!(i, Item::Box(_))).unwrap();
        assert_eq!(&t[top.spans[pow].clone()], "sqrt(x)^3");
        assert_eq!(boxes(t), "y = (a+b/2) + (√(x)^3)");
    }

    /// Round 13 (G80): labeled and legended values draw their labels.
    #[test]
    fn labeled_values_draw_their_labels() {
        assert_eq!(boxes(r#"labeled(x^2 + 1, "energy")"#), "((x^2) + 1 label below energy)");
        assert_eq!(boxes(r#"labeled(3/4, "a", top)"#), "((3/4) label above a)");
        assert_eq!(boxes(r#"labeled(1 + x, "lbl", [left])"#), "(1 + x label left lbl)");
        assert_eq!(boxes(r#"labeled(y, "b", after)"#), "(y label right b)");
        assert_eq!(boxes(r#"labeled(y, "b", bottom)"#), "(y label below b)");
        assert_eq!(boxes(r#"labeled(y, "b", before)"#), "(y label left b)");
        // a label that is a short term, an escaped quote, a raw string with TeX kept as written
        assert_eq!(boxes("labeled(x, a1)"), "(x label below a1)");
        assert_eq!(boxes(r#"labeled(x, "say \"hi\"")"#), "(x label below say \"hi\")");
        assert_eq!(boxes(r#"labeled(x, r"$\alpha$")"#), r"(x label below $\alpha$)");
        // legends at the right
        assert_eq!(boxes(r#"legended(sin(x), "curve")"#), "(sin(x) label right curve)");
        assert_eq!(boxes(r#"legended(x, ["a", "b"])"#), "(x label right a\nb)");
        assert_eq!(boxes(r#"legended(x, line_legend([red, blue], ["a", "b"]))"#), "(x label right ━━ a\n━━ b)");
        assert_eq!(boxes(r#"legended(x, swatch_legend([red], ["a"]))"#), "(x label right ■ a)");
        // in a list
        assert_eq!(boxes(r#"[labeled(1, "a"), labeled(2, "b"), 3]"#), "[(1 label below a), (2 label below b), 3]");
    }

    #[test]
    fn unknown_label_forms_stay_as_printed() {
        for t in [
            "labeled(x)",
            r#"labeled(x, "a", center)"#,
            r#"labeled(x, "a", top, 3)"#,
            "labeled(x, f(y))",
            "labeled(x, [a, b])",
            "legended(x, foo([red], [a]))",
            "legended(x, [])",
            r#"legended(x, "a", "b")"#,
            r#"f(labeled(x, "a"))"#,
            r#"labeled(x, "a") + 1"#,
            r#"[[labeled(x, "a")]]"#,
            r#"labeled(graphics([point([0, 0])]), "a")"#,
            r#"labeled(x, "")"#,
        ] {
            assert!(!mc_boxes::has_boxes(&math_text(t).row), "{t}");
        }
        // a value too long to be drawn with its label
        let long = format!("labeled({}, \"a\")", vec!["x"; 12_000].join("+"));
        assert!(!mc_boxes::has_boxes(&math_text(&long).row));
    }

    #[test]
    fn legend_swatches_have_their_colours() {
        let t = r#"legended(x, line_legend([red, rgb_color(0, 0.5, 1)], ["a", "b"]))"#;
        let m = math_text(t);
        let Item::Box(b) = &m.row.0[0] else { panic!("a box") };
        let Node::Labeled { swatches, side, .. } = &**b else { panic!("labeled") };
        assert_eq!(*side, Side::Right);
        assert_eq!(swatches, &[(0, [255, 0, 0]), (1, [255, 0, 0]), (5, [0, 128, 255]), (6, [0, 128, 255])]);
        // each swatch stands for its colour's text, each line break for the separator between the entries
        let label = &m.map.boxes[&0][1];
        assert_eq!(&t[label.spans[0].clone()], "red");
        assert_eq!(&t[label.spans[5].clone()], "rgb_color(0, 0.5, 1)");
        assert_eq!(&t[label.spans[4].clone()], ", ");
        assert_eq!(&t[label.spans[3].clone()], "a");
        let rows = label_rows(&m.row);
        assert_eq!(rows, vec![LabelRow { path: vec![Step { item: 0, slot: 1 }], swatches: swatches.clone() }]);
        // colours the legend cannot read draw no swatch; the entry stays
        assert_eq!(boxes(r#"legended(x, line_legend([c, hue(0.5)], ["a", "b"]))"#), "(x label right a\n━━ b)");
        assert_eq!(colour_of(r##"legended(x, line_legend(["#ff8000"], ["a"]))"##), [255, 128, 0]);
        assert_eq!(colour_of(r#"legended(x, line_legend([hue(1/3)], ["a"]))"#), [0, 255, 0]);
        assert_eq!(colour_of(r#"legended(x, line_legend([gray_level(0.5, 0.2)], ["a"]))"#), [128, 128, 128]);
    }

    /// The first swatch colour of a legend.
    fn colour_of(t: &str) -> [u8; 3] {
        let rows = label_rows(&math_text(t).row);
        rows.first().and_then(|r| r.swatches.first()).map_or_else(|| panic!("no swatch: {t}"), |s| s.1)
    }

    #[test]
    fn a_labeled_value_maps_to_its_bytes() {
        let t = r#"labeled(x^2 + 1/y, "e\"n")"#;
        let m = math_text(t);
        assert_eq!(m.row.len(), 1);
        // the box stands for the whole call: selecting all and copying gives the printed text
        assert_eq!(&t[m.map.spans[0].clone()], t);
        assert!(m.map.contiguous(&[], 0..1, t));
        // the value's items map to the value's bytes
        let value = &m.map.boxes[&0][0];
        assert_eq!(&t[value.span(&[], 0..value.spans.len()).unwrap()], "x^2 + 1/y");
        let v = Step { item: 0, slot: 0 };
        let pow = m.row.at(&[v]).unwrap().0.iter().position(|i| matches!(i, Item::Box(_))).unwrap();
        assert_eq!(&t[value.spans[pow].clone()], "x^2");
        // the label's characters map inside the quotes; an escape stands for its two bytes
        let label = &m.map.boxes[&0][1];
        let spans: Vec<&str> = label.spans.iter().map(|r| &t[r.clone()]).collect();
        assert_eq!(spans, ["e", "\\\"", "n"]);
        // a caret and Find inside the value go into its slot
        let k = t.find("1/y").unwrap();
        let (path, _) = m.map.caret_at(k + 1);
        assert_eq!(path.first(), Some(&v));
        let (path, r) = m.map.find(k..k + 3).unwrap();
        assert_eq!(path, vec![v]);
        assert_eq!(&t[m.map.span(&path, r).unwrap()], "1/y");
        // in a list, each call is one box standing for its bytes
        let t = r#"[labeled(1, "a"), 2]"#;
        let m = math_text(t);
        assert_eq!(&t[m.map.spans[1].clone()], r#"labeled(1, "a")"#);
        assert_eq!(&t[m.map.boxes[&1][0].spans[0].clone()], "1");
    }

    #[test]
    fn a_labeled_value_reads_back_as_printed() {
        let t = r#"labeled(x^2 + 1, "energy", top)"#;
        let row = math_text(t).row;
        assert_eq!(linear_text(&row), t);
        assert_eq!(mc_boxes::to_text(&row), t);
        assert_eq!(mc_boxes::to_text_mapped(&row).0, t);
        assert_eq!(notation(&mc_boxes::restrict(&row)), t);
        assert_eq!(mc_boxes::to_text(&crate::output_input_boxes(&row)), t);
        assert!(mc_boxes::to_latex(&row).contains("\\overset{\\text{energy}}"));
        // a part of the value copies as the value's part
        let value = row.at(&[Step { item: 0, slot: 0 }]).unwrap();
        assert_eq!(crate::output_input_text(value), "x^2 + 1");
    }

    /// The layout: the label at the size of the text, below, above or beside the value, the value's baseline kept.
    #[test]
    fn labels_are_placed_around_the_value() {
        use mc_boxes::layout::{layout, Draw, LayoutOptions, MonoMetrics};
        let lay = |t: &str| layout(&math_text(t).row, &MonoMetrics, &LayoutOptions::output(10.0, None));
        // (x, baseline, size) of the first character drawn in a row: the value's `v`, the label's `L`
        let at = |l: &mc_boxes::layout::Layout, ch: char| {
            l.items
                .iter()
                .find_map(|d| match d {
                    Draw::Char { c, x, y, size, .. } if *c == ch => Some((*x, *y, *size)),
                    _ => None,
                })
                .unwrap()
        };
        let plain = lay("v");
        let below = lay(r#"labeled(v, "L")"#);
        let (vx, vy, vs) = at(&below, 'v');
        let (lx, ly, ls) = at(&below, 'L');
        assert_eq!(vs, ls, "the label is as large as the value");
        assert!(ly > vy + 5.0 && (lx - vx).abs() < 0.01, "below and centred: {vx},{vy} {lx},{ly}");
        assert_eq!(below.baseline, plain.baseline, "the value keeps its baseline");
        let above = lay(r#"labeled(v, "L", top)"#);
        let (_, vy, _) = at(&above, 'v');
        let (_, ly, _) = at(&above, 'L');
        assert!(ly < vy - 5.0);
        let left = lay(r#"labeled(v, "L", left)"#);
        let (vx, vy, _) = at(&left, 'v');
        let (lx, ly, _) = at(&left, 'L');
        assert!(lx + 10.0 < vx && (ly - vy).abs() < 0.01, "before, on the baseline");
        let right = lay(r#"labeled(v, "L", right)"#);
        let (vx, _, _) = at(&right, 'v');
        let (lx, _, _) = at(&right, 'L');
        assert!(vx + 10.0 < lx);
        // a legend of two entries at the right, centred on the value; its swatches drawn as characters
        let leg = lay(r#"legended(v, line_legend([red, blue], ["A", "B"]))"#);
        let (vx, vy, _) = at(&leg, 'v');
        let (ax, ay, _) = at(&leg, 'A');
        let (bx, by, _) = at(&leg, 'B');
        assert!(ax > vx + 10.0 && (ax - bx).abs() < 0.01 && by > ay + 5.0);
        assert!(ay < vy && by > vy, "the entries around the value's line: {ay} {vy} {by}");
        assert_eq!(leg.items.iter().filter(|d| matches!(d, Draw::Char { c: '━', .. })).count(), 4);
        // the label row has its caret stops, so a click on it lands in it
        let path = vec![Step { item: 0, slot: 1 }];
        let s = below.caret(&path, 0).unwrap();
        assert_eq!(below.hit(s.x + 1.0, s.baseline - 2.0), (path, 0));
    }
}
