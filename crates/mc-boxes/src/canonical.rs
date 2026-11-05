//! Canonical box text (design 08, section 4.4): ordinary MathCAS source in which each box is a
//! double-parenthesised expression of a fixed shape, and its reading back; plus the linear input text (4.6), the
//! same source with the box markers reduced to the parentheses the meaning needs.
//!
//! | Box | Canonical text |
//! |---|---|
//! | fraction | `((N)/(D))` |
//! | superscript | `((B)^(E))` |
//! | subscript | `((subscript(B, S)))` |
//! | radical | `((sqrt(X)))`, `((root(X, n)))` |
//! | over/under | `((overscript(B, O)))`, `((underscript(B, U)))`, `((underoverscript(B, U, O)))` |
//! | grid | `(([[a, b], [c, d]]))` |
//! | big operators | `((sum(f, [k, a, b])))`, `((sum(f, k)))`, `((product(...)))`, `((integrate(f, x)))`, `((integrate(f, [x, a, b])))`, `((limit(f, x -> a)))`, `((d(f, x)))` |
//! | graphic | `((image("path")))` |
//! | placeholder | `□` |
//!
//! Slots of the call shapes are separated by `, `. A slot whose text would not read back as one slot (it has a
//! top-level `,`, or `->` in a limit's variable, or is a bracket list where a list means limits) is written in one
//! more pair of parentheses, and that pair is removed on reading; the rule is applied recursively so that a slot
//! which itself looks like such a wrapped slot is wrapped again (see [`Wrap`]). With it, `from_text(to_text(b)) == b`
//! for every well-formed tree whose rows have balanced brackets outside strings and comments, and
//! `to_text(from_text(t)) == t` for every text.
use crate::scan::Scan;
use std::cell::RefCell;
use std::collections::HashMap;
use crate::tree::{BigOp, Item, Node, Row, MAX_NESTING};

/// The canonical text of a row.
pub fn to_text(row: &Row) -> String {
    let mut s = String::new();
    write_row(row, &mut s);
    s
}

/// Reads canonical text: every double-parenthesised box shape becomes its box (recursively), `□` outside
/// literals becomes a placeholder, and everything else stays characters. Never fails.
pub fn from_text(text: &str) -> Row {
    let scan = Scan::new(text);
    Reader { s: &scan, memo: RefCell::new(HashMap::new()) }.row(0, scan.chars.len(), 0)
}

/// What makes a slot's text need one more pair of parentheses in a call shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Wrap {
    /// a top-level `,`
    Comma,
    /// a top-level `,` or `->` (a limit's variable)
    CommaArrow,
    /// a top-level `,`, or a bracket list (the variable of a sum, product or integral without limits)
    CommaList,
}

impl Wrap {
    /// Whether the text `lo..hi` of `s` must be wrapped: its own reason, or it is one parenthesised group whose
    /// inside would have to be wrapped (so that unwrapping on reading is unambiguous).
    pub(crate) fn needs(self, s: &Scan, lo: usize, hi: usize) -> bool {
        let own = match self {
            Wrap::Comma => s.has_top(lo, hi, ","),
            Wrap::CommaArrow => s.has_top(lo, hi, ",") || s.has_top(lo, hi, "->"),
            Wrap::CommaList => s.has_top(lo, hi, ",") || s.is_group(lo, hi, '['),
        };
        own || (s.is_group(lo, hi, '(') && self.needs(s, lo + 1, hi - 1))
    }

    /// The text to write for a slot whose text is `t`.
    pub(crate) fn apply(self, t: String) -> String {
        let s = Scan::new(&t);
        if self.needs(&s, 0, s.chars.len()) {
            format!("({t})")
        } else {
            t
        }
    }
}

fn write_row(row: &Row, out: &mut String) {
    for it in &row.0 {
        match it {
            Item::Char(c) => out.push(*c),
            Item::Placeholder => out.push('□'),
            Item::Box(b) => write_node(b, out),
        }
    }
}

fn slot(r: &Row, w: Wrap) -> String {
    w.apply(to_text(r))
}

fn write_node(n: &Node, out: &mut String) {
    let c = Wrap::Comma;
    let body = match n {
        Node::Fraction { num, den } => format!("({})/({})", to_text(num), to_text(den)),
        Node::Superscript { base, exp } => format!("({})^({})", to_text(base), to_text(exp)),
        Node::Subscript { base, index } => format!("(subscript({}, {}))", slot(base, c), slot(index, c)),
        Node::Radical { radicand, index: None } => format!("(sqrt({}))", slot(radicand, c)),
        Node::Radical { radicand, index: Some(i) } => format!("(root({}, {}))", slot(radicand, c), slot(i, c)),
        Node::Overscript { base, over } => format!("(overscript({}, {}))", slot(base, c), slot(over, c)),
        Node::Underscript { base, under } => format!("(underscript({}, {}))", slot(base, c), slot(under, c)),
        Node::Underoverscript { base, under, over } => {
            format!("(underoverscript({}, {}, {}))", slot(base, c), slot(under, c), slot(over, c))
        }
        Node::Grid { rows } => {
            let rows: Vec<String> =
                rows.iter().map(|r| format!("[{}]", r.iter().map(|x| slot(x, c)).collect::<Vec<_>>().join(", "))).collect();
            format!("([{}])", rows.join(", "))
        }
        Node::BigOp { op, body, var, lower, upper } => {
            let f = op.function();
            let b = slot(body, c);
            match (op, lower, upper) {
                (BigOp::Limit, Some(p), _) => format!("({f}({b}, {} -> {}))", slot(var, Wrap::CommaArrow), slot(p, c)),
                (_, Some(lo), Some(hi)) => format!("({f}({b}, [{}, {}, {}]))", slot(var, c), slot(lo, c), slot(hi, c)),
                _ => format!("({f}({b}, {}))", slot(var, Wrap::CommaList)),
            }
        }
        Node::Graphic { path } => {
            let mut lit = String::new();
            mc_syntax::string_literal(path, &mut lit);
            format!("(image({lit}))")
        }
    };
    out.push('(');
    out.push_str(&body);
    out.push(')');
}

struct Reader<'a> {
    s: &'a Scan,
    /// the box read at each (opening parenthesis, depth): the fraction rule looks at inner groups again
    memo: RefCell<HashMap<(usize, usize), Option<Node>>>,
}

/// The heads of the call shapes.
const CALLS: &[&str] =
    &["subscript", "sqrt", "root", "overscript", "underscript", "underoverscript", "sum", "product", "integrate", "limit", "d", "image"];

impl Reader<'_> {
    fn row(&self, lo: usize, hi: usize, depth: usize) -> Row {
        let s = self.s;
        let mut items = Vec::new();
        let mut i = lo;
        while i < hi {
            if s.is(i, '(') && depth < MAX_NESTING {
                if let Some(j) = s.close[i].filter(|&j| j < hi) {
                    if let Some(n) = self.node(i, j, depth + 1) {
                        items.push(Item::Box(Box::new(n)));
                        i = j + 1;
                        continue;
                    }
                }
            }
            let c = s.chars[i];
            items.push(if c == '□' && !s.literal[i] { Item::Placeholder } else { Item::Char(c) });
            i += 1;
        }
        Row(items)
    }

    /// A slot row: never empty.
    fn slot(&self, lo: usize, hi: usize, depth: usize) -> Option<Row> {
        (hi > lo).then(|| self.row(lo, hi, depth))
    }

    /// A call argument's slot: for all but the first argument one leading space (after `, `) dropped, then the
    /// wrapping pair if [`Wrap`] put one.
    fn arg(&self, (lo, hi): (usize, usize), w: Wrap, depth: usize, after_sep: bool) -> Option<Row> {
        let range = if after_sep { strip_lead(self.s, lo, hi) } else { (lo, hi) };
        let (lo, hi) = self.unwrap(range, w);
        self.slot(lo, hi, depth)
    }

    fn unwrap(&self, (lo, hi): (usize, usize), w: Wrap) -> (usize, usize) {
        if self.s.is_group(lo, hi, '(') && w.needs(self.s, lo + 1, hi - 1) {
            (lo + 1, hi - 1)
        } else {
            (lo, hi)
        }
    }

    /// The box whose text is the group `i..=j`, if it has one of the shapes.
    fn node(&self, i: usize, j: usize, depth: usize) -> Option<Node> {
        if let Some(n) = self.memo.borrow().get(&(i, depth)) {
            return n.clone();
        }
        let n = self.node_uncached(i, j, depth);
        self.memo.borrow_mut().insert((i, depth), n.clone());
        n
    }

    fn node_uncached(&self, i: usize, j: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        if !s.is(i + 1, '(') {
            return None;
        }
        let k = s.close[i + 1]?;
        if k == j - 1 {
            // `((inner))`: a call shape or a grid
            return self.call(i + 2, j - 1, depth);
        }
        // `((N)op(D))`
        let op = s.chars.get(k + 1).copied().filter(|_| !s.literal[k + 1])?;
        if !(op == '/' || op == '^') || !s.is_group(k + 2, j, '(') {
            return None;
        }
        // The slot delimiters are plain parentheses: if either is itself a box's text, this is a parenthesised
        // `box op box` the user wrote (`(x²/√y)`), not a fraction or power box.
        if depth < MAX_NESTING && (self.node(i + 1, k, depth + 1).is_some() || self.node(k + 2, j - 1, depth + 1).is_some()) {
            return None;
        }
        let a = self.slot(i + 2, k, depth)?;
        let b = self.slot(k + 3, j - 1, depth)?;
        Some(if op == '/' { Node::Fraction { num: a, den: b } } else { Node::Superscript { base: a, exp: b } })
    }

    fn call(&self, lo: usize, hi: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        if s.is_group(lo, hi, '[') {
            return self.grid(lo, hi, depth);
        }
        let open = (lo..hi).find(|&p| !(s.chars[p].is_ascii_alphanumeric() || s.chars[p] == '_'))?;
        if !s.is_group(open, hi, '(') {
            return None;
        }
        let name = s.string(lo, open);
        if !CALLS.contains(&name.as_str()) {
            return None;
        }
        let args = s.split(open + 1, hi - 1, ",", false);
        let c = Wrap::Comma;
        let a = |k: usize, w: Wrap| self.arg(args[k], w, depth, k > 0);
        Some(match (name.as_str(), args.len()) {
            ("subscript", 2) => Node::Subscript { base: a(0, c)?, index: a(1, c)? },
            ("sqrt", 1) => Node::Radical { radicand: a(0, c)?, index: None },
            ("root", 2) => Node::Radical { radicand: a(0, c)?, index: Some(a(1, c)?) },
            ("overscript", 2) => Node::Overscript { base: a(0, c)?, over: a(1, c)? },
            ("underscript", 2) => Node::Underscript { base: a(0, c)?, under: a(1, c)? },
            ("underoverscript", 3) => Node::Underoverscript { base: a(0, c)?, under: a(1, c)?, over: a(2, c)? },
            ("image", 1) => {
                Node::Graphic { path: string_value(s, args[0].0, args[0].1)? }
            }
            ("limit", 2) => {
                let (l, h) = strip_lead(s, args[1].0, args[1].1);
                let parts = s.split(l, h, "->", true);
                let [(vl, vh), (pl, ph)] = parts[..] else { return None };
                let vh = if vh > vl && s.chars[vh - 1] == ' ' { vh - 1 } else { vh };
                let (vl, vh) = self.unwrap((vl, vh), Wrap::CommaArrow);
                Node::BigOp {
                    op: BigOp::Limit,
                    body: a(0, c)?,
                    var: self.slot(vl, vh, depth)?,
                    lower: Some(self.arg((pl, ph), c, depth, true)?),
                    upper: None,
                }
            }
            ("sum" | "product" | "integrate" | "d", 2) => {
                let op = match name.as_str() {
                    "sum" => BigOp::Sum,
                    "product" => BigOp::Product,
                    "integrate" => BigOp::Integral,
                    _ => BigOp::Derivative,
                };
                let body = a(0, c)?;
                let (l, h) = strip_lead(s, args[1].0, args[1].1);
                if op != BigOp::Derivative && s.is_group(l, h, '[') {
                    let items = s.split(l + 1, h - 1, ",", false);
                    let [v, lo_, hi_] = items[..] else { return None };
                    Node::BigOp {
                        op,
                        body,
                        var: self.arg(v, c, depth, false)?,
                        lower: Some(self.arg(lo_, c, depth, true)?),
                        upper: Some(self.arg(hi_, c, depth, true)?),
                    }
                } else {
                    Node::BigOp { op, body, var: a(1, Wrap::CommaList)?, lower: None, upper: None }
                }
            }
            _ => return None,
        })
    }

    fn grid(&self, lo: usize, hi: usize, depth: usize) -> Option<Node> {
        let s = self.s;
        let mut rows = Vec::new();
        for (n, r) in s.split(lo + 1, hi - 1, ",", false).into_iter().enumerate() {
            let (l, h) = if n > 0 { strip_lead(s, r.0, r.1) } else { r };
            if !s.is_group(l, h, '[') {
                return None;
            }
            let cells = s.split(l + 1, h - 1, ",", false);
            rows.push(
                cells.into_iter().enumerate().map(|(m, c)| self.arg(c, Wrap::Comma, depth, m > 0)).collect::<Option<Vec<_>>>()?,
            );
        }
        let n = Node::Grid { rows };
        n.is_well_formed().then_some(n)
    }
}

/// Drops one leading space (the one after a separator).
fn strip_lead(s: &Scan, lo: usize, hi: usize) -> (usize, usize) {
    if lo < hi && s.chars[lo] == ' ' {
        (lo + 1, hi)
    } else {
        (lo, hi)
    }
}

/// The value of a string literal spanning exactly `lo..hi` (escapes `\n \t \" \\ \u{..}`).
fn string_value(s: &Scan, lo: usize, hi: usize) -> Option<String> {
    if hi < lo + 2 || s.chars[lo] != '"' || s.chars[hi - 1] != '"' {
        return None;
    }
    let mut out = String::new();
    let mut i = lo + 1;
    while i < hi - 1 {
        let c = s.chars[i];
        if c == '"' {
            return None;
        }
        if c != '\\' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        match s.chars.get(i)? {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            '"' => out.push('"'),
            '\\' => out.push('\\'),
            'u' => {
                let close = (i..hi).find(|&k| s.chars[k] == '}')?;
                let hex: String = s.chars.get(i + 2..close)?.iter().collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                i = close;
            }
            _ => return None,
        }
        i += 1;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::notation;

    fn rt(t: &str) -> String {
        let r = from_text(t);
        assert_eq!(to_text(&r), t, "text round trip of {t}");
        notation(&r)
    }

    #[test]
    fn reads_every_shape() {
        assert_eq!(rt("x = ((a + b)/(2c))"), "x = (a + b/2c)");
        assert_eq!(rt("((x)^(2)) + 1"), "(x^2) + 1");
        assert_eq!(rt("((subscript(x, i)))"), "(x_i)");
        assert_eq!(rt("((sqrt(x + 1)))"), "√(x + 1)");
        assert_eq!(rt("((root(x, 3)))"), "√[3](x)");
        assert_eq!(rt("((overscript(x, ^)))((underscript(y, _)))"), "(x over ^)(y under _)");
        assert_eq!(rt("((underoverscript(S, a, b)))"), "(S under a over b)");
        assert_eq!(rt("(([[a, b], [c, d]]))"), "grid[[a,b],[c,d]]");
        assert_eq!(rt("((sum(k^2, [k, 1, n])))"), "∑[k=1..n](k^2)");
        assert_eq!(rt("((product(f(k), k)))"), "∏[k](f(k))");
        assert_eq!(rt("((integrate(sin(x), [x, 0, pi])))"), "∫[x=0..pi](sin(x))");
        assert_eq!(rt("((integrate(f, x)))"), "∫[x](f)");
        assert_eq!(rt("((limit(sin(x)/x, x -> 0)))"), "lim[x→0](sin(x)/x)");
        assert_eq!(rt("((d(f, x)))"), "∂[x](f)");
        assert_eq!(rt(r#"((image("a \"b\".png")))"#), "image(a \"b\".png)");
        assert_eq!(rt("((□)/(□))"), "(□/□)");
    }

    #[test]
    fn nested_and_linear_text_is_kept() {
        assert_eq!(rt("((((subscript(x, i))))^(2))"), "((x_i)^2)");
        assert_eq!(rt("(((a)/(b)))"), "((a/b))");
        assert_eq!(rt("(((sqrt(x))))"), "(√(x))");
        assert_eq!(rt("((a+b))"), "((a+b))");
        assert_eq!(rt("((foo(x)))"), "((foo(x)))");
        assert_eq!(rt("((sqrt(x, y)))"), "((sqrt(x, y)))");
        assert_eq!(rt("(([[a], [b, c]]))"), "(([[a], [b, c]]))");
        assert_eq!(rt("\"((a)/(b))\" # ((a)/(b))"), "\"((a)/(b))\" # ((a)/(b))");
        assert_eq!(rt("f(((a)/(b))"), "f((a/b)");
        assert_eq!(rt("□ \"□\""), "□ \"□\"");
        assert!(matches!(from_text("□").0[..], [Item::Placeholder]));
        assert!(matches!(from_text("\"□\"").0[1], Item::Char('□')));
        assert_eq!(rt("(()/(x))"), "(()/(x))");
        // a parenthesised quotient of two boxes, not a fraction box
        assert_eq!(rt("f(((x)^(2))/((sqrt(z))))"), "f((x^2)/√(z))");
        assert_eq!(rt("(((x)^(2))^((sqrt(z))))"), "((x^2)^√(z))");
        // a fraction whose slots are boxes
        assert_eq!(rt("((((x)^(2)))/(((sqrt(z)))))"), "((x^2)/√(z))");
    }

    #[test]
    fn slots_with_separators_are_wrapped() {
        let sub = |i: &str| Row::boxed(Node::Subscript { base: Row::text("x"), index: Row::text(i) });
        for (index, text) in [
            ("i, j", "((subscript(x, (i, j))))"),
            ("(i, j)", "((subscript(x, ((i, j)))))"),
            ("(i)", "((subscript(x, (i))))"),
            (" i", "((subscript(x,  i)))"),
        ] {
            let r = sub(index);
            assert_eq!(to_text(&r), text);
            assert_eq!(from_text(text), r, "{text}");
        }
        let lim = Row::boxed(Node::BigOp {
            op: BigOp::Limit,
            body: Row::text("f"),
            var: Row::text("a -> b"),
            lower: Some(Row::text("c")),
            upper: None,
        });
        assert_eq!(to_text(&lim), "((limit(f, (a -> b) -> c)))");
        assert_eq!(from_text(&to_text(&lim)), lim);
        let int = Row::boxed(Node::BigOp { op: BigOp::Integral, body: Row::text("f"), var: Row::text("[x]"), lower: None, upper: None });
        assert_eq!(to_text(&int), "((integrate(f, ([x]))))");
        assert_eq!(from_text(&to_text(&int)), int);
    }
}
