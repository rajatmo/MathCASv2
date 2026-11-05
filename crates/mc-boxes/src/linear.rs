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
        Node::Radical { radicand, index: Some(i) } => call("root", vec![slot(radicand, c), slot(i, c)]),
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
                let wrap = match form {
                    Form::Atom => attaches_left(prev),
                    Form::Pow => {
                        attaches_left(prev)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::from_text;

    fn lin(t: &str) -> String {
        linear_text(&from_text(t))
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
        assert_eq!(lin("2((x)^(2))"), "2(x^2)");
        assert_eq!(lin("y((sqrt(x)))"), "y(sqrt(x))");
        assert_eq!(lin("1 + ((sqrt(x)))"), "1 + sqrt(x)");
        assert_eq!(lin("((subscript(x, i)))"), "subscript(x, i)");
        assert_eq!(lin("(([[1, ((a)/(b))], [3, 4]]))"), "[[1, a/b], [3, 4]]");
        assert_eq!(lin("((sum(((1)/(k)), [k, 1, n])))"), "sum(1/k, [k, 1, n])");
        assert_eq!(lin("((limit(f, x -> 0)))"), "limit(f, x -> 0)");
    }
}
