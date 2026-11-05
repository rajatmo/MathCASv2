//! Text to boxes through the parser (design 08, sections 4.5 and 4.6):
//!
//! - [`typeset_output`]: printed output (`value.printed`) to boxes, with the output substitutions of 4.5
//!   (juxtaposition for `*`, `π ∞ ⅇ ⅈ`, `→ ≤ ≥ ≠ ⩵`, strings without quotes, primes).
//! - [`typeset_input`]: linear input text to boxes (`Ctrl+Shift+N`): every construct with a box form becomes the
//!   box; all other text is kept exactly as typed.
//!
//! Both walk the syntax tree and copy the source text between the children they convert, so text without a box
//! form passes through unchanged (spacing, comments, operators as written).
use crate::canonical::from_text;
use crate::tree::{BigOp, Item, Node, Row, MAX_NESTING};
use mc_syntax::ast::{BinOp, Expr, ExprKind, MulOp, Stmt, StmtKind};
use mc_syntax::Diagnostic;

/// Typeset output: the boxes and whether the value is a failure (drawn as linear text in the message colour).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typeset {
    /// the content
    pub row: Row,
    /// the printed value is `failure(...)`: `row` is its plain text
    pub message: bool,
}

/// Printed output to boxes. Text that does not parse (elisions, opaque objects) stays linear text.
pub fn typeset_output(printed: &str) -> Typeset {
    on_big_stack(|| {
        let parsed = mc_syntax::parse(printed);
        if !parsed.errors.is_empty() {
            return Typeset { row: plain(printed), message: false };
        }
        if let [Stmt { kind: StmtKind::Expr(e), .. }] = parsed.program.stmts.as_slice() {
            if let ExprKind::Call(f, _) = &e.kind {
                if matches!(&f.kind, ExprKind::Name(n) if n == "failure") {
                    return Typeset { row: plain(printed), message: true };
                }
            }
        }
        let c = Conv { src: printed, output: true };
        let mut items = Vec::new();
        c.program(&parsed.program.stmts, &mut items, 0);
        Typeset { row: Row(items), message: false }
    })
}

/// Linear input text to boxes; on a syntax error nothing is converted and the errors are returned.
pub fn typeset_input(text: &str) -> Result<Row, Vec<Diagnostic>> {
    on_big_stack(|| {
        // canonical box text already in the input is read first, so converting twice changes nothing
        let parsed = mc_syntax::parse(text);
        if !parsed.errors.is_empty() {
            return Err(parsed.errors);
        }
        let c = Conv { src: text, output: false };
        let mut items = Vec::new();
        c.program(&parsed.program.stmts, &mut items, 0);
        Ok(Row(items))
    })
}

/// A row of the characters of `s`, with no placeholders (output text is never a template).
fn plain(s: &str) -> Row {
    Row(s.chars().map(Item::Char).collect())
}

/// Stack for the conversion: the syntax tree is up to [`mc_syntax::MAX_DEPTH`] deep and its frames are large in
/// unoptimised builds, so the walk runs on a thread whose stack is reserved, not committed.
const STACK_BYTES: usize = 32 << 20;

fn on_big_stack<R: Send, F: FnOnce() -> R + Send>(f: F) -> R {
    std::thread::scope(|s| {
        match std::thread::Builder::new().name("mc-boxes".into()).stack_size(STACK_BYTES).spawn_scoped(s, f) {
            Ok(h) => h.join().unwrap_or_else(|p| std::panic::resume_unwind(p)),
            Err(e) => panic!("cannot start the typesetting thread: {e}"),
        }
    })
}

/// A child of a node, for copying the source text around it.
enum Child<'a> {
    E(&'a Expr),
    S(&'a Stmt),
}

impl Child<'_> {
    fn span(&self) -> (usize, usize) {
        let s = match self {
            Child::E(e) => e.span,
            Child::S(s) => s.span,
        };
        (s.start as usize, s.end as usize)
    }
}

struct Conv<'a> {
    src: &'a str,
    output: bool,
}

/// Output spellings of operators (design 08, section 4.5), longest first; `===` and `=!=` stay as they are.
const OUTPUT_OPS: &[(&str, &str)] =
    &[("===", "==="), ("=!=", "=!="), ("<->", "↔"), ("->", "→"), (":>", "⧴"), ("<=", "≤"), (">=", "≥"), ("!=", "≠"), ("==", "⩵"), ("=>", "⇒")];

/// Names drawn as symbols in output.
fn output_name(n: &str) -> Option<char> {
    match n {
        "pi" => Some('π'),
        "inf" => Some('∞'),
        "e" => Some('ⅇ'),
        "im" => Some('ⅈ'),
        _ => None,
    }
}

fn primes(n: u64) -> Option<&'static str> {
    match n {
        1 => Some("′"),
        2 => Some("″"),
        3 => Some("‴"),
        4 => Some("⁗"),
        _ => None,
    }
}

impl Conv<'_> {
    fn text(&self, lo: usize, hi: usize, out: &mut Vec<Item>) {
        let t = &self.src[lo..hi.max(lo)];
        if !self.output {
            out.extend(t.chars().map(|c| if c == '□' { Item::Placeholder } else { Item::Char(c) }));
            return;
        }
        let mut rest = t;
        'outer: while let Some(c) = rest.chars().next() {
            for (from, to) in OUTPUT_OPS {
                if let Some(r) = rest.strip_prefix(from) {
                    out.extend(to.chars().map(Item::Char));
                    rest = r;
                    continue 'outer;
                }
            }
            out.push(Item::Char(c));
            rest = &rest[c.len_utf8()..];
        }
    }

    /// Copies `lo..hi`, converting the children (sorted, non-overlapping) and copying the text between them.
    fn splice(&self, lo: usize, hi: usize, children: Vec<Child<'_>>, out: &mut Vec<Item>, depth: usize) {
        let mut pos = lo;
        for ch in children {
            let (a, b) = ch.span();
            if a < pos || b > hi {
                continue;
            }
            self.text(pos, a, out);
            match ch {
                Child::E(e) => self.expr(e, out, depth),
                Child::S(s) => self.stmt(s, out, depth),
            }
            pos = b;
        }
        self.text(pos, hi, out);
    }

    fn program(&self, stmts: &[Stmt], out: &mut Vec<Item>, depth: usize) {
        self.splice(0, self.src.len(), stmts.iter().map(Child::S).collect(), out, depth);
    }

    fn stmt(&self, s: &Stmt, out: &mut Vec<Item>, depth: usize) {
        let (lo, hi) = (s.span.start as usize, s.span.end as usize);
        let children: Vec<Child> = match &s.kind {
            StmtKind::Expr(e) => vec![Child::E(e)],
            StmtKind::Let { value, .. } => vec![Child::E(value)],
            StmtKind::Fn { body, .. } => vec![Child::E(body)],
            StmtKind::Rule { lhs, rhs, .. } => vec![Child::E(lhs), Child::E(rhs)],
            StmtKind::Module { body, .. } => body.iter().map(Child::S).collect(),
            StmtKind::Pub(inner) => vec![Child::S(inner)],
            _ => vec![],
        };
        self.splice(lo, hi, children, out, depth);
    }

    /// A slot's row from an expression, without the parentheses around it if it is parenthesised.
    fn slot(&self, e: &Expr, depth: usize) -> Row {
        let mut items = Vec::new();
        let (lo, hi) = (e.span.start as usize, e.span.end as usize);
        if self.parenthesised(e) {
            // the same expression with its span shrunk inside the parentheses
            let inner = Expr { kind: e.kind.clone(), span: mc_syntax::Span::new(lo + 1, hi - 1) };
            self.expr(&inner, &mut items, depth);
        } else {
            self.expr(e, &mut items, depth);
        }
        if items.is_empty() {
            items.push(Item::Placeholder);
        }
        Row(items)
    }

    fn parenthesised(&self, e: &Expr) -> bool {
        let t = &self.src[e.span.start as usize..e.span.end as usize];
        let s = crate::scan::Scan::new(t);
        s.is_group(0, s.chars.len(), '(')
    }

    fn push_box(&self, n: Node, out: &mut Vec<Item>) {
        out.push(Item::Box(Box::new(n)));
    }

    fn expr(&self, e: &Expr, out: &mut Vec<Item>, depth: usize) {
        let (lo, hi) = (e.span.start as usize, e.span.end as usize);
        if depth >= MAX_NESTING {
            self.text(lo, hi, out);
            return;
        }
        let d = depth + 1;
        if let Some(n) = self.as_box(e, d) {
            // a parenthesised box keeps its parentheses as characters around it
            if self.parenthesised(e) {
                out.push(Item::Char('('));
                self.push_box(n, out);
                out.push(Item::Char(')'));
            } else {
                self.push_box(n, out);
            }
            return;
        }
        match &e.kind {
            ExprKind::Name(n) if self.output => match output_name(n) {
                Some(c) if !self.parenthesised(e) => out.push(Item::Char(c)),
                _ => self.text(lo, hi, out),
            },
            ExprKind::Str(s) if self.output && !self.parenthesised(e) => out.extend(s.chars().map(Item::Char)),
            ExprKind::Num(n) if self.output && n.imag => {
                let t = &self.src[lo..hi];
                match t.strip_suffix("im") {
                    Some(digits) => {
                        out.extend(digits.chars().map(Item::Char));
                        out.push(Item::Char('ⅈ'));
                    }
                    None => self.text(lo, hi, out),
                }
            }
            ExprKind::Prime(f, n) if self.output => match primes(u64::from(*n)) {
                Some(p) => {
                    self.splice(lo, f.span.end as usize, vec![Child::E(f)], out, depth);
                    out.extend(p.chars().map(Item::Char));
                }
                None => self.splice(lo, hi, vec![Child::E(f)], out, depth),
            },
            ExprKind::Mul(fs) if self.output => {
                if self.parenthesised(e) {
                    out.push(Item::Char('('));
                }
                self.juxtapose(fs.iter().map(|(_, f)| f), out, depth);
                if self.parenthesised(e) {
                    out.push(Item::Char(')'));
                }
            }
            ExprKind::Call(f, args) => {
                // derivative(n)(f)(x) and derivative(n)(f): f with primes
                if let Some((fun, n)) = self.derivative_of(e) {
                    if let Some(p) = primes(n) {
                        self.expr(fun, out, d);
                        out.extend(p.chars().map(Item::Char));
                    } else {
                        let base = self.slot(fun, d);
                        let exp = Row::text(&format!("({n})"));
                        self.push_box(Node::Superscript { base, exp }, out);
                    }
                    if !matches!(&f.kind, ExprKind::Call(g, _) if matches!(&g.kind, ExprKind::Name(_))) {
                        // applied: copy the argument list
                        let start = args.first().map_or(hi, |a| a.value.span.start as usize);
                        let open = self.src[..start].rfind('(').unwrap_or(start);
                        self.splice(open, hi, args.iter().map(|a| Child::E(&a.value)).collect(), out, depth);
                    }
                    return;
                }
                let mut ch = vec![Child::E(f)];
                ch.extend(args.iter().map(|a| Child::E(&a.value)));
                self.splice(lo, hi, ch, out, depth);
            }
            _ => self.splice(lo, hi, children(e), out, depth),
        }
    }

    /// Output products: factors side by side with a thin space, `×` between two numbers.
    fn juxtapose<'e>(&self, factors: impl Iterator<Item = &'e Expr>, out: &mut Vec<Item>, depth: usize) {
        let mut prev_num = false;
        for (k, f) in factors.enumerate() {
            let num = matches!(f.kind, ExprKind::Num(_));
            if k > 0 {
                out.push(Item::Char(if prev_num && num { '×' } else { '\u{2009}' }));
            }
            self.expr(f, out, depth);
            prev_num = num;
        }
    }

    /// The factors of a product as a row: output juxtaposes them, input joins them with `*`.
    fn factors_row(&self, fs: &[&Expr], depth: usize) -> Row {
        if let [one] = fs {
            return self.slot(one, depth);
        }
        let mut items = Vec::new();
        if self.output {
            self.juxtapose(fs.iter().copied(), &mut items, depth);
        } else {
            for (k, f) in fs.iter().enumerate() {
                if k > 0 {
                    // keep the source's own operator between neighbours (`2x`, `a * b`), `*` otherwise
                    let (pe, s) = (fs[k - 1].span.end as usize, f.span.start as usize);
                    let gap = self.src.get(pe..s).filter(|g| g.trim() == "*" || g.is_empty());
                    match gap {
                        Some(g) => self.text(pe, pe + g.len(), &mut items),
                        None => items.push(Item::Char('*')),
                    }
                }
                self.expr(f, &mut items, depth);
            }
        }
        Row(items)
    }

    /// The box an expression is drawn as, if it has one.
    fn as_box(&self, e: &Expr, depth: usize) -> Option<Node> {
        match &e.kind {
            ExprKind::Mul(fs) if fs.iter().any(|(op, _)| *op == MulOp::Div) => {
                let num: Vec<&Expr> = fs.iter().filter(|(op, _)| *op == MulOp::Times).map(|(_, f)| f).collect();
                let den: Vec<&Expr> = fs.iter().filter(|(op, _)| *op == MulOp::Div).map(|(_, f)| f).collect();
                let num = if num.is_empty() { Row::text("1") } else { self.factors_row(&num, depth) };
                Some(Node::Fraction { num, den: self.factors_row(&den, depth) })
            }
            ExprKind::Pow(b, x) => {
                let base = self.slot(b, depth);
                // inputs keep their powers (a radical would read as a different term); outputs show radicals
                let root = if self.output { self.reciprocal(x) } else { None };
                Some(match root {
                    Some(n) if self.src_of(n) == "2" => Node::Radical { radicand: base, index: None },
                    Some(n) => Node::Radical { radicand: base, index: Some(self.slot(n, depth)) },
                    None => Node::Superscript { base: self.script_base(b, depth), exp: self.slot(x, depth) },
                })
            }
            ExprKind::Call(f, args) if args.iter().all(|a| a.name.is_none()) => {
                let ExprKind::Name(name) = &f.kind else { return None };
                let a: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
                let s = |k: usize| self.slot(a[k], depth);
                Some(match (name.as_str(), a.len()) {
                    ("sqrt", 1) => Node::Radical { radicand: s(0), index: None },
                    ("root", 2) => Node::Radical { radicand: s(0), index: Some(s(1)) },
                    ("subscript", 2) => Node::Subscript { base: self.script_base(a[0], depth), index: s(1) },
                    ("overscript", 2) => Node::Overscript { base: s(0), over: s(1) },
                    ("underscript", 2) => Node::Underscript { base: s(0), under: s(1) },
                    ("underoverscript", 3) => Node::Underoverscript { base: s(0), under: s(1), over: s(2) },
                    ("image", 1) => match &a[0].kind {
                        ExprKind::Str(p) => Node::Graphic { path: p.clone() },
                        _ => return None,
                    },
                    ("limit", 2) => match &a[1].kind {
                        ExprKind::Binary(BinOp::Rule, v, p) => Node::BigOp {
                            op: BigOp::Limit,
                            body: s(0),
                            var: self.slot(v, depth),
                            lower: Some(self.slot(p, depth)),
                            upper: None,
                        },
                        _ => return None,
                    },
                    ("d", 2) => match &a[1].kind {
                        ExprKind::Name(_) => Node::BigOp { op: BigOp::Derivative, body: s(0), var: s(1), lower: None, upper: None },
                        _ => return None,
                    },
                    ("sum" | "product" | "integrate", 2) => {
                        let op = match name.as_str() {
                            "sum" => BigOp::Sum,
                            "product" => BigOp::Product,
                            _ => BigOp::Integral,
                        };
                        match &a[1].kind {
                            ExprKind::List(l) if l.len() == 3 => Node::BigOp {
                                op,
                                body: s(0),
                                var: self.slot(&l[0], depth),
                                lower: Some(self.slot(&l[1], depth)),
                                upper: Some(self.slot(&l[2], depth)),
                            },
                            ExprKind::Name(_) => Node::BigOp { op, body: s(0), var: s(1), lower: None, upper: None },
                            _ => return None,
                        }
                    }
                    _ => return None,
                })
            }
            _ => None,
        }
    }

    /// A script's base keeps its parentheses when it needs them to read right (`(a + b)²`), drops them around a
    /// name or call.
    fn script_base(&self, b: &Expr, depth: usize) -> Row {
        let keep = self.parenthesised(b)
            && !matches!(b.kind, ExprKind::Name(_) | ExprKind::Num(_) | ExprKind::Call(..) | ExprKind::Index(..) | ExprKind::List(_));
        if keep {
            let mut items = vec![Item::Char('(')];
            items.extend(self.slot(b, depth).0);
            items.push(Item::Char(')'));
            Row(items)
        } else {
            self.slot(b, depth)
        }
    }

    fn src_of(&self, e: &Expr) -> &str {
        &self.src[e.span.start as usize..e.span.end as usize]
    }

    /// For an exponent `1/n` (possibly parenthesised): `n`.
    fn reciprocal<'e>(&self, x: &'e Expr) -> Option<&'e Expr> {
        match &x.kind {
            ExprKind::Mul(fs) => match fs.as_slice() {
                [(MulOp::Times, one), (MulOp::Div, n)] if self.src_of(one) == "1" => Some(n),
                _ => None,
            },
            _ => None,
        }
    }

    /// `derivative(n)(f)` or `derivative(n)(f)(x)`: the function and the order.
    fn derivative_of<'e>(&self, e: &'e Expr) -> Option<(&'e Expr, u64)> {
        let ExprKind::Call(f, args) = &e.kind else { return None };
        let (inner, fargs) = match &f.kind {
            // derivative(n)(f)(x): f is derivative(n)(f)
            ExprKind::Call(g, gargs) if matches!(&g.kind, ExprKind::Call(..)) => (&**g, gargs),
            // derivative(n)(f): f is derivative(n)
            ExprKind::Call(g, _) if matches!(&g.kind, ExprKind::Name(_)) => (&**f, args),
            _ => return None,
        };
        let ExprKind::Call(d, n) = &inner.kind else { return None };
        let (ExprKind::Name(dn), [n], [fun]) = (&d.kind, n.as_slice(), fargs.as_slice()) else { return None };
        if dn != "derivative" || n.name.is_some() || fun.name.is_some() || !matches!(n.value.kind, ExprKind::Num(_)) {
            return None;
        }
        Some((&fun.value, self.src_of(&n.value).parse().ok()?))
    }
}

/// The expression children of a node, in source order.
fn children(e: &Expr) -> Vec<Child<'_>> {
    use Child::{E, S};
    let mut v: Vec<Child> = match &e.kind {
        ExprKind::Typed(x, _) | ExprKind::Named(_, x) | ExprKind::Neg(x) | ExprKind::Not(x) | ExprKind::Prime(x, _) => vec![E(x)],
        ExprKind::SlotFn(x) | ExprKind::Lambda(_, x) => vec![E(x)],
        ExprKind::Add(xs) => xs.iter().map(|(_, x)| E(x)).collect(),
        ExprKind::Mul(xs) => xs.iter().map(|(_, x)| E(x)).collect(),
        ExprKind::Pow(a, b) | ExprKind::Binary(_, a, b) | ExprKind::Span(a, b) | ExprKind::While(a, b) => vec![E(a), E(b)],
        ExprKind::Nary(_, xs) | ExprKind::List(xs) => xs.iter().map(E).collect(),
        ExprKind::Call(f, args) => std::iter::once(E(f)).chain(args.iter().map(|a| E(&a.value))).collect(),
        ExprKind::Index(x, idx) => std::iter::once(E(x)).chain(idx.iter().map(E)).collect(),
        ExprKind::Block(stmts) => stmts.iter().map(S).collect(),
        ExprKind::If(c, t, f) => [Some(&**c), Some(&**t), f.as_deref()].into_iter().flatten().map(E).collect(),
        ExprKind::For(_, it, body) => vec![E(it), E(body)],
        ExprKind::Match(x, arms) => std::iter::once(E(x)).chain(arms.iter().flat_map(|a| [E(&a.pattern), E(&a.body)])).collect(),
        _ => vec![],
    };
    v.sort_by_key(|c| c.span().0);
    v
}

/// Canonical text or linear text: canonical shapes are read as boxes, everything else converted as linear text.
/// Used when a typeset cell is converted again (`Ctrl+Shift+N` on a cell that already has boxes).
pub fn typeset_any(text: &str) -> Result<Row, Vec<Diagnostic>> {
    let canonical = from_text(text);
    if canonical.0.iter().any(|i| matches!(i, Item::Box(_))) {
        return Ok(canonical);
    }
    typeset_input(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::notation;

    fn out(t: &str) -> String {
        notation(&typeset_output(t).row)
    }

    fn inp(t: &str) -> String {
        notation(&typeset_input(t).unwrap())
    }

    #[test]
    fn output_rows_of_the_table() {
        // a/b, a*b/c: numerator factors over denominator factors
        assert_eq!(out("a/b"), "(a/b)");
        assert_eq!(out("a*b/c"), "(a\u{2009}b/c)");
        assert_eq!(out("a/b*c"), "(a\u{2009}c/b)");
        assert_eq!(out("(a + b)/(2*c)"), "(a + b/2\u{2009}c)");
        // rational literals
        assert_eq!(out("1/2"), "(1/2)");
        assert_eq!(out("x/2 + 1"), "(x/2) + 1");
        // powers and radicals
        assert_eq!(out("x^2"), "(x^2)");
        assert_eq!(out("(a + b)^n"), "((a + b)^n)");
        assert_eq!(out("x^(n + 1)"), "(x^n + 1)");
        assert_eq!(out("x^(1/2)"), "√(x)");
        assert_eq!(out("sqrt(x + 1)"), "√(x + 1)");
        assert_eq!(out("x^(1/n)"), "√[n](x)");
        assert_eq!(out("root(x, 3)"), "√[3](x)");
        assert_eq!(out("x^-1"), "(x^-1)");
        // products: thin space, × between numbers
        assert_eq!(out("2*x"), "2\u{2009}x");
        assert_eq!(out("a*b"), "a\u{2009}b");
        assert_eq!(out("2*3"), "2×3");
        assert_eq!(out("2*x*y^2"), "2\u{2009}x\u{2009}(y^2)");
        // scripts
        assert_eq!(out("subscript(x, i)"), "(x_i)");
        assert_eq!(out("subscript(x, i)^2"), "((x_i)^2)");
        assert_eq!(out("overscript(x, \"^\")"), "(x over ^)");
        assert_eq!(out("underscript(lim, n)"), "(lim under n)");
        assert_eq!(out("underoverscript(x, a, b)"), "(x under a over b)");
        // big operators
        assert_eq!(out("sum(k^2, [k, 1, n])"), "∑[k=1..n]((k^2))");
        assert_eq!(out("product(f(k), [k, 1, n])"), "∏[k=1..n](f(k))");
        assert_eq!(out("integrate(f(x), x)"), "∫[x](f(x))");
        assert_eq!(out("integrate(sin(x), [x, 0, pi])"), "∫[x=0..π](sin(x))");
        assert_eq!(out("limit(sin(x)/x, x -> 0)"), "lim[x→0]((sin(x)/x))");
        assert_eq!(out("d(f(x), x)"), "∂[x](f(x))");
        // derivatives
        assert_eq!(out("derivative(1)(f)(x)"), "f′(x)");
        assert_eq!(out("derivative(2)(f)(x)"), "f″(x)");
        assert_eq!(out("derivative(7)(f)(x)"), "(f^(7))(x)");
        assert_eq!(out("derivative(1)(f)"), "f′");
        assert_eq!(out("f''(x)"), "f″(x)");
        // constants and operators
        assert_eq!(out("pi + inf + e + im"), "π + ∞ + ⅇ + ⅈ");
        assert_eq!(out("2 + 3im"), "2 + 3ⅈ");
        assert_eq!(out("x -> 1"), "x → 1");
        assert_eq!(out("[a <= b, a >= b, a != b, a == b]"), "[a ≤ b, a ≥ b, a ≠ b, a ⩵ b]");
        // strings without quotes, lists linear
        assert_eq!(out("\"hello\""), "hello");
        assert_eq!(out("[[1, 2], [3, 4]]"), "[[1, 2], [3, 4]]");
        assert_eq!(out("image(\"plot-1.png\")"), "image(plot-1.png)");
    }

    #[test]
    fn failures_and_unparsable_output_stay_linear() {
        let f = typeset_output("failure(\"div\", 1/0)");
        assert!(f.message);
        assert_eq!(notation(&f.row), "failure(\"div\", 1/0)");
        let u = typeset_output("[1, 2, «998», 1000]");
        assert!(!u.message);
        assert_eq!(notation(&u.row), "[1, 2, «998», 1000]");
    }

    #[test]
    fn input_keeps_text_and_makes_boxes() {
        assert_eq!(inp("x = (a + b)/(2c)"), "x = (a + b/2c)");
        assert_eq!(inp("f(x) := x^2 + sqrt(x) # square"), "f(x) := (x^2) + √(x) # square");
        assert_eq!(inp("2*x*y/z"), "(2*x*y/z)");
        assert_eq!(inp("pi -> \"s\""), "pi -> \"s\"");
        assert_eq!(inp("sum(1/k^2, [k, 1, inf])"), "∑[k=1..inf]((1/(k^2)))");
        assert_eq!(inp("x^(1/2) + sqrt(y)"), "(x^(1/2)) + √(y)");
        assert!(typeset_input("f(").is_err());
        assert_eq!(notation(&typeset_any("((a)/(b)) + c/d").unwrap()), "(a/b) + c/d");
        assert_eq!(notation(&typeset_any("c/d").unwrap()), "(c/d)");
    }
}
