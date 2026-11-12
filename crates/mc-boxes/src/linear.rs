//! Linear input text (design 08, section 4.6): the canonical text with every box written in its plain form and
//! only the parentheses the meaning needs: `((a + b)/(2c))` becomes `(a + b)/(2c)`, `((x)^(2))` becomes `x^2`,
//! `((sqrt(x)))` becomes `sqrt(x)`. The result reads to the same terms as the canonical text (tested), so "copy as
//! plain text" and screen readers see what the kernel evaluates.
use crate::canonical::Wrap;
use crate::tree::{BigOp, Item, Node, Row};
use mc_syntax::ast::{ExprKind, StmtKind};

/// The linear input text of a row.
pub fn linear_text(row: &Row) -> String {
    let mut out = String::new();
    write_row(row, &mut out);
    out
}

/// How a box's plain form binds, which decides when it needs parentheses around it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Form {
    /// a call, list or string: binds like a name
    Atom,
    /// `b^e`
    Pow,
    /// `n/d`
    Div,
}

/// The syntactic class of a slot's text, from the parser.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    /// a name, number, string, call, index, list, prime or anything in parentheses
    Atom,
    /// a power
    Pow,
    /// a product or quotient chain
    Mul,
    /// anything else (sums, comparisons, several statements, errors)
    Other,
}

fn class(text: &str) -> Class {
    let t = text.trim();
    if t.is_empty() {
        return Class::Other;
    }
    let parsed = mc_syntax::parse(t);
    if !parsed.errors.is_empty() || parsed.program.stmts.len() != 1 {
        return Class::Other;
    }
    let StmtKind::Expr(e) = &parsed.program.stmts[0].kind else { return Class::Other };
    // the expression must cover the whole text (a parenthesised expression's span includes its parentheses)
    if e.span.start != 0 || e.span.end as usize != t.len() {
        return Class::Other;
    }
    let scan = crate::scan::Scan::new(t);
    if scan.is_group(0, scan.chars.len(), '(') {
        return Class::Atom;
    }
    match e.kind {
        ExprKind::Name(_)
        | ExprKind::Str(_)
        | ExprKind::Call(..)
        | ExprKind::Index(..)
        | ExprKind::List(_)
        | ExprKind::Prime(..)
        | ExprKind::Slot(_) => Class::Atom,
        // a number literal (a sign is a separate prefix operator)
        ExprKind::Num(_) => Class::Atom,
        ExprKind::Pow(..) => Class::Pow,
        ExprKind::Mul(_) => Class::Mul,
        _ => Class::Other,
    }
}

fn paren_unless(t: String, ok: bool) -> String {
    if ok {
        t
    } else {
        format!("({t})")
    }
}

fn slot(r: &Row, w: Wrap) -> String {
    w.apply(linear_text(r))
}

fn node(n: &Node) -> (String, Form) {
    let c = Wrap::Comma;
    let call = |name: &str, args: Vec<String>| (format!("{name}({})", args.join(", ")), Form::Atom);
    match n {
        Node::Fraction { num, den } => {
            let a = linear_text(num);
            let b = linear_text(den);
            let ka = class(&a);
            let kb = class(&b);
            let a = paren_unless(a, matches!(ka, Class::Atom | Class::Pow));
            let b = paren_unless(b, matches!(kb, Class::Atom | Class::Pow));
            (format!("{a}/{b}"), Form::Div)
        }
        Node::Superscript { base, exp } => {
            let a = linear_text(base);
            let b = linear_text(exp);
            let ka = class(&a);
            let kb = class(&b);
            let a = paren_unless(a, ka == Class::Atom);
            let b = paren_unless(b, matches!(kb, Class::Atom | Class::Pow));
            (format!("{a}^{b}"), Form::Pow)
        }
        Node::Subscript { base, index } => call("subscript", vec![slot(base, c), slot(index, c)]),
        Node::Radical { radicand, index: None } => call("sqrt", vec![slot(radicand, c)]),
        Node::Radical { radicand, index: Some(i) } => call("surd", vec![slot(radicand, c), slot(i, c)]),
        Node::Overscript { base, over } => call("overscript", vec![slot(base, c), slot(over, c)]),
        Node::Underscript { base, under } => call("underscript", vec![slot(base, c), slot(under, c)]),
        Node::Underoverscript { base, under, over } => {
            call("underoverscript", vec![slot(base, c), slot(under, c), slot(over, c)])
        }
        Node::Grid { rows } => {
            let rows: Vec<String> =
                rows.iter().map(|r| format!("[{}]", r.iter().map(|x| slot(x, c)).collect::<Vec<_>>().join(", "))).collect();
            (format!("[{}]", rows.join(", ")), Form::Atom)
        }
        Node::BigOp { op, body, var, lower, upper } => {
            let b = slot(body, c);
            let second = match (op, lower, upper) {
                (BigOp::Limit, Some(p), _) => format!("{} -> {}", slot(var, Wrap::CommaArrow), slot(p, c)),
                (_, Some(lo), Some(hi)) => format!("[{}, {}, {}]", slot(var, c), slot(lo, c), slot(hi, c)),
                _ => slot(var, Wrap::CommaList),
            };
            call(op.function(), vec![b, second])
        }
        Node::Graphic { path } => {
            let mut lit = String::new();
            mc_syntax::string_literal(path, &mut lit);
            call("image", vec![lit])
        }
        // the printed call it was read from
        Node::Labeled { text, .. } => (text.clone(), Form::Atom),
    }
}

/// A neighbour of a box in its row, as far as binding goes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// start or end of the row
    Edge,
    /// a character
    Char(char),
    /// another box or a placeholder (binds like a name)
    Box,
}

/// Whether a neighbour on the left would attach to the box (a call, an index, juxtaposition, a longer name).
fn attaches_left(s: Side) -> bool {
    match s {
        Side::Edge => false,
        Side::Box => true,
        Side::Char(c) => c.is_alphanumeric() || matches!(c, '_' | ')' | ']' | '}' | '\'' | '"' | '`' | '.' | '′'),
    }
}

/// Whether `s` ends with a number literal (digits with an optional fraction), directly, not with a name such as
/// `x2`.
fn ends_with_number(s: &str) -> bool {
    let tail: String = s.chars().rev().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.').collect();
    let tail: String = tail.chars().rev().collect();
    tail.chars().next().is_some_and(|c| c.is_ascii_digit()) && tail.chars().all(|c| c.is_ascii_digit() || c == '.')
}

fn write_row(row: &Row, out: &mut String) {
    let start = out.len();
    for (i, it) in row.0.iter().enumerate() {
        match it {
            Item::Char(c) => out.push(*c),
            Item::Placeholder => out.push('□'),
            Item::Box(b) => {
                let (t, form) = node(b);
                let prev = out[start..].chars().rev().find(|c| *c != ' ').map_or(Side::Edge, Side::Char);
                let next = row.0[i + 1..]
                    .iter()
                    .find(|x| !matches!(x, Item::Char(' ')))
                    .map_or(Side::Edge, |x| if let Item::Char(c) = x { Side::Char(*c) } else { Side::Box });
                // a number right before a power or a call that starts with a name or `(` multiplies it by
                // juxtaposition, which binds like `*` (round 32: `2x^2`, not `2(x^2)`)
                let coefficient = ends_with_number(&out[start..])
                    && t.chars().next().is_some_and(|c| c.is_alphabetic() || c == '(');
                let wrap = match form {
                    Form::Atom => attaches_left(prev) && !coefficient,
                    Form::Pow => {
                        (attaches_left(prev) && !coefficient)
                            || match next {
                                Side::Edge => false,
                                Side::Box => true,
                                Side::Char(c) => c.is_alphanumeric() || matches!(c, '_' | '(' | '[' | '\'' | '.' | '^' | '"' | '`'),
                            }
                    }
                    Form::Div => {
                        let left_ok = match prev {
                            Side::Edge => true,
                            Side::Char(c) => matches!(c, '+' | '=' | ',' | ';' | '(' | '[' | '{' | '>' | '\n' | ':'),
                            Side::Box => false,
                        };
                        let right_ok = match next {
                            Side::Edge => true,
                            Side::Char(c) => matches!(c, '+' | '-' | '=' | ',' | ';' | ')' | ']' | '}' | '<' | '>' | '!' | '\n' | '#'),
                            Side::Box => false,
                        };
                        !(left_ok && right_ok)
                    }
                };
                if wrap {
                    out.push('(');
                    out.push_str(&t);
                    out.push(')');
                } else {
                    out.push_str(&t);
                }
            }
        }
    }
}

/// `row` with only roots (square and n-th), fractions and powers as boxes (the notebook's three, decision NBUI.5):
/// every other box (a subscript, a grid, a big operator, an over- or underscript, a graphic) becomes its linear text,
/// its slots' boxes kept where they are of the three kinds.
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
                Node::Radical { radicand, index } => out.push(Item::Box(Box::new(Node::Radical {
                    radicand: restrict(radicand),
                    index: index.as_ref().map(restrict),
                }))),
                _ => out.extend(Row::text(&linear_text(&Row(vec![it.clone()]))).0),
            },
            other => out.push(other.clone()),
        }
    }
    Row(out)
}

/// A stretch of a cell's linear text and the canonical text it stands for.
pub type Segment = (std::ops::Range<usize>, std::ops::Range<usize>);

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::from_text;
    use crate::tree::notation;

    fn lin(t: &str) -> String {
        linear_text(&from_text(t))
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
    fn old_boxes_become_linear_except_the_roots_fractions_and_powers() {
        let row = from_text("((subscript(x, i)))+((sqrt(((a)/(b)))))+((root(y, 3)))");
        let r = restrict(&row);
        assert_eq!(notation(&r), "subscript(x, i)+√((a/b))+√[3](y)");
        assert!(crate::has_boxes(&r) && !crate::has_boxes(&Row::text("x")));
    }

    #[test]
    fn minimal_parentheses() {
        assert_eq!(lin("x = ((a + b)/(2c))"), "x = (a + b)/(2c)");
        assert_eq!(lin("((x)^(2))"), "x^2");
        assert_eq!(lin("((x + 1)^(n - 1))"), "(x + 1)^(n - 1)");
        assert_eq!(lin("((((x)^(2)))^(3))"), "(x^2)^3");
        assert_eq!(lin("((x)^(((y)^(2))))"), "x^y^2");
        assert_eq!(lin("((a*b)/(c))"), "(a*b)/c");
        assert_eq!(lin("((a)/(b*c))"), "a/(b*c)");
        assert_eq!(lin("((1)/(2))x"), "(1/2)x");
        assert_eq!(lin("x + ((1)/(2))"), "x + 1/2");
        assert_eq!(lin("2((x)^(2))"), "2x^2");
        assert_eq!(lin("2((sqrt(x)))"), "2sqrt(x)");
        assert_eq!(lin("2(((1 + x))^(2))"), "2(1 + x)^2");
        assert_eq!(lin("x2((x)^(2))"), "x2(x^2)");
        assert_eq!(lin("2((3)^(2))"), "2(3^2)");
        assert_eq!(lin("2 ((x)^(2))"), "2 (x^2)");
        assert_eq!(lin("y((sqrt(x)))"), "y(sqrt(x))");
        assert_eq!(lin("1 + ((sqrt(x)))"), "1 + sqrt(x)");
        assert_eq!(lin("((subscript(x, i)))"), "subscript(x, i)");
        assert_eq!(lin("(([[1, ((a)/(b))], [3, 4]]))"), "[[1, a/b], [3, 4]]");
        assert_eq!(lin("((sum(((1)/(k)), [k, 1, n])))"), "sum(1/k, [k, 1, n])");
        assert_eq!(lin("((limit(f, x -> 0)))"), "limit(f, x -> 0)");
    }
}
