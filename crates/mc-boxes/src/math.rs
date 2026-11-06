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
//! [`restrict`] turns any box tree into one with only those three boxes (older typeset notebooks): every other box
//! becomes its linear text.
use crate::linear::linear_text;
use crate::printed::{SourceMap, Typeset};
use crate::tree::{Item, Node, Row};
use mc_syntax::ast::{Expr, ExprKind, MulOp, Stmt, StmtKind};
use std::collections::HashMap;
use std::ops::Range;

/// A two-dimensional construct found in the text: its whole byte range and its slots.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Construct {
    kind: Kind,
    whole: Range<usize>,
    slots: Vec<Vec<Piece>>,
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
}

impl Construct {
    fn plain(kind: Kind, whole: Range<usize>, slots: Vec<Range<usize>>) -> Construct {
        Construct { kind, whole, slots: slots.into_iter().map(|s| vec![Piece::Text(s)]).collect() }
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
        if let (Some(c), true) = (fits, depth < crate::tree::MAX_NESTING) {
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
    let slots: Vec<(Row, SourceMap)> = c.slots.iter().map(|s| build_pieces(text, s, at, depth + 1)).collect();
    let mut rows = slots.iter().map(|(r, _)| r.clone());
    let node = match c.kind {
        Kind::Sqrt => Node::Radical { radicand: rows.next().unwrap_or_default(), index: None },
        Kind::Fraction => Node::Fraction { num: rows.next().unwrap_or_default(), den: rows.next().unwrap_or_default() },
        Kind::Power => Node::Superscript { base: rows.next().unwrap_or_default(), exp: rows.next().unwrap_or_default() },
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
        "-1/2" => Construct {
            kind: Kind::Fraction,
            whole: whole.clone(),
            slots: vec![vec![Piece::Sym('1', whole.clone())], vec![Piece::Box(root())]],
        },
        _ => match negative_integer(text, &p.exp) {
            Some(mag) => Construct {
                kind: Kind::Fraction,
                whole: whole.clone(),
                slots: vec![vec![Piece::Sym('1', whole.clone())], vec![positive_power(text, p, &mag)]],
            },
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
    Some(Construct { kind: Kind::Fraction, whole, slots: vec![num, den] })
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

/// `row` with only square roots, fractions and powers as boxes: every other box (a subscript, an n-th root, a grid,
/// a big operator, an over- or underscript, a graphic) becomes its linear text, its slots' boxes kept where they are
/// of the three kinds.
pub fn restrict(row: &Row) -> Row {
    let mut out = Vec::with_capacity(row.0.len());
    for it in &row.0 {
        match it {
            Item::Box(b) => match &**b {
                Node::Fraction { num, den } => {
                    out.push(Item::Box(Box::new(Node::Fraction { num: restrict(num), den: restrict(den) })))
                }
                Node::Superscript { base, exp } => {
                    out.push(Item::Box(Box::new(Node::Superscript { base: restrict(base), exp: restrict(exp) })))
                }
                Node::Radical { radicand, index: None } => {
                    out.push(Item::Box(Box::new(Node::Radical { radicand: restrict(radicand), index: None })))
                }
                _ => out.extend(Row::text(&linear_text(&Row(vec![it.clone()]))).0),
            },
            other => out.push(other.clone()),
        }
    }
    Row(out)
}

/// A stretch of a cell's linear text and the canonical text it stands for.
pub type Segment = (Range<usize>, Range<usize>);

/// The linear text of a cell's canonical text, for Find (NBUI.7: `x^2` finds a power box), with the canonical byte
/// range each stretch of it stands for: one stretch per item of the top row (a box's linear text stands for the whole
/// box).
pub fn linear_segments(canonical: &str) -> (String, Vec<Segment>) {
    let row = crate::from_text(canonical);
    let (_, map) = crate::to_text_mapped(&row);
    let mut lin = String::new();
    let mut segs = Vec::with_capacity(row.0.len());
    for (k, it) in row.0.iter().enumerate() {
        let a = map.offset(&[], k).unwrap_or(canonical.len());
        let b = map.offset(&[], k + 1).unwrap_or(canonical.len());
        let piece = match it {
            Item::Char(c) => c.to_string(),
            Item::Placeholder => "□".to_string(),
            Item::Box(_) => linear_text(&Row(vec![it.clone()])),
        };
        let start = lin.len();
        lin.push_str(&piece);
        segs.push((start..lin.len(), a..b));
    }
    (lin, segs)
}

/// Whether `row` holds a box at any depth (a cell whose text is plain needs no box marks in the file).
pub fn has_boxes(row: &Row) -> bool {
    row.0.iter().any(|i| matches!(i, Item::Box(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::notation;

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

    #[test]
    fn find_reads_the_linear_text() {
        let t = "a+((x)^(2))+((sqrt(y)))";
        let (lin, segs) = linear_segments(t);
        assert_eq!(lin, "a+x^2+sqrt(y)");
        let k = lin.find("x^2").unwrap();
        let seg = segs.iter().find(|(l, _)| l.contains(&k)).unwrap();
        assert_eq!(&t[seg.1.clone()], "((x)^(2))");
    }

    #[test]
    fn old_boxes_become_linear_except_the_three() {
        let row = crate::from_text("((subscript(x, i)))+((sqrt(((a)/(b)))))+((root(y, 3)))");
        let r = restrict(&row);
        assert_eq!(notation(&r), "subscript(x, i)+√((a/b))+root(y, 3)");
        assert!(has_boxes(&r) && !has_boxes(&Row::text("x")));
    }
}
