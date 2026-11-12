//! LaTeX of a box tree ("Copy as LaTeX", design 10.1, milestone M7): math-mode LaTeX for typeset input and output.
//!
//! Boxes map to the usual constructions (`\frac`, `^{}`, `_{}`, `\sqrt[n]{}`, `\overset`, `\underset`, `matrix`
//! environments, `\sum_{}^{}`, `\int_{}^{} \, dx`, `\lim_{x \to a}`, `\frac{\partial}{\partial x}`); characters map
//! to their LaTeX commands (Greek letters, operators, arrows, relations); names of two or more letters are upright
//! (`\sin` for the functions LaTeX knows, `\operatorname{name}` before an argument list, `\text{name}` otherwise);
//! `*` and the thin space of output products become a space, `×` `\times`. A grid between square brackets is a
//! `bmatrix` (parentheses: `pmatrix`). The conventions are the common ones of mathematical writing in LaTeX (the
//! amsmath user's guide for the matrix environments and `\operatorname`).
use crate::tree::{BigOp, Item, Node, Row, Side};

/// The LaTeX (math mode, without delimiters) of a row.
pub fn to_latex(row: &Row) -> String {
    let mut out = String::new();
    write_row(row, &mut out, 0);
    tidy(&out)
}

/// Functions LaTeX has a command for (upright, with operator spacing).
const FUNCTIONS: &[&str] = &[
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "coth", "exp", "log",
    "ln", "lg", "det", "dim", "gcd", "max", "min", "sup", "inf", "lim", "deg", "arg", "ker", "Pr",
];

/// Names drawn as symbols.
fn named_symbol(name: &str) -> Option<&'static str> {
    Some(match name {
        "pi" => "\\pi",
        "inf" => "\\infty",
        "im" => "i",
        "e" => "e",
        _ => greek(name)?,
    })
}

fn greek(name: &str) -> Option<&'static str> {
    Some(match name {
        "alpha" => "\\alpha",
        "beta" => "\\beta",
        "gamma" => "\\gamma",
        "delta" => "\\delta",
        "epsilon" => "\\epsilon",
        "theta" => "\\theta",
        "lambda" => "\\lambda",
        "mu" => "\\mu",
        "sigma" => "\\sigma",
        "phi" => "\\phi",
        "omega" => "\\omega",
        _ => return None,
    })
}

/// The LaTeX of a single character.
fn char_latex(c: char) -> Option<&'static str> {
    Some(match c {
        'α' => "\\alpha ",
        'β' => "\\beta ",
        'γ' => "\\gamma ",
        'δ' => "\\delta ",
        'ε' => "\\varepsilon ",
        'ϵ' => "\\epsilon ",
        'ζ' => "\\zeta ",
        'η' => "\\eta ",
        'θ' => "\\theta ",
        'ϑ' => "\\vartheta ",
        'ι' => "\\iota ",
        'κ' => "\\kappa ",
        'λ' => "\\lambda ",
        'μ' => "\\mu ",
        'ν' => "\\nu ",
        'ξ' => "\\xi ",
        'π' => "\\pi ",
        'ρ' => "\\rho ",
        'σ' => "\\sigma ",
        'ς' => "\\varsigma ",
        'τ' => "\\tau ",
        'υ' => "\\upsilon ",
        'φ' => "\\varphi ",
        'ϕ' => "\\phi ",
        'χ' => "\\chi ",
        'ψ' => "\\psi ",
        'ω' => "\\omega ",
        'Γ' => "\\Gamma ",
        'Δ' => "\\Delta ",
        'Θ' => "\\Theta ",
        'Λ' => "\\Lambda ",
        'Ξ' => "\\Xi ",
        'Π' => "\\Pi ",
        'Σ' => "\\Sigma ",
        'Υ' => "\\Upsilon ",
        'Φ' => "\\Phi ",
        'Ψ' => "\\Psi ",
        'Ω' => "\\Omega ",
        '∞' => "\\infty ",
        'ⅇ' => "e",
        'ⅈ' => "i",
        'ⅆ' => "d",
        '→' => "\\to ",
        '⧴' => "\\mapsto ",
        '↔' => "\\leftrightarrow ",
        '⇒' => "\\Rightarrow ",
        '≤' => "\\le ",
        '≥' => "\\ge ",
        '≠' => "\\ne ",
        '⩵' => "=",
        '≈' => "\\approx ",
        '±' => "\\pm ",
        '∓' => "\\mp ",
        '×' => "\\times ",
        '·' => "\\cdot ",
        '÷' => "\\div ",
        '−' => "-",
        '∂' => "\\partial ",
        '∇' => "\\nabla ",
        '∈' => "\\in ",
        '∉' => "\\notin ",
        '⊂' => "\\subset ",
        '⊆' => "\\subseteq ",
        '∪' => "\\cup ",
        '∩' => "\\cap ",
        '∧' => "\\land ",
        '∨' => "\\lor ",
        '¬' => "\\lnot ",
        '∀' => "\\forall ",
        '∃' => "\\exists ",
        '∅' => "\\emptyset ",
        '′' => "'",
        '″' => "''",
        '‴' => "'''",
        '°' => "^{\\circ}",
        '\u{2009}' | '*' => " ",
        '{' => "\\{",
        '}' => "\\}",
        '#' => "\\#",
        '$' => "\\$",
        '%' => "\\%",
        '&' => "\\&",
        '~' => "\\sim ",
        '\\' => "\\backslash ",
        '\n' => " \\\\\n",
        '□' => "\\square ",
        _ => return None,
    })
}

fn is_letter(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn write_row(row: &Row, out: &mut String, depth: usize) {
    let items = &row.0;
    let ch = |i: usize| match items.get(i) {
        Some(Item::Char(c)) => Some(*c),
        _ => None,
    };
    let mut i = 0;
    while i < items.len() {
        match &items[i] {
            Item::Placeholder => out.push_str("\\square "),
            Item::Box(b) => write_node(b, out, depth),
            Item::Char('"') => {
                // a string: its text upright
                let mut j = i + 1;
                let mut s = String::new();
                while let Some(c) = ch(j) {
                    if c == '"' {
                        break;
                    }
                    s.push(c);
                    j += 1;
                }
                out.push_str("\\text{");
                out.push_str(&text_escape(&s));
                out.push('}');
                i = j + 1;
                continue;
            }
            Item::Char(c) if is_letter(*c) => {
                // a name (letters, digits after the first, underscores)
                let mut j = i;
                let mut name = String::new();
                while let Some(c) = ch(j).filter(|c| is_letter(*c) || c.is_ascii_digit()) {
                    name.push(c);
                    j += 1;
                }
                write_name(&name, ch(j) == Some('('), out);
                i = j;
                continue;
            }
            Item::Char('[') if matches!(items.get(i + 1), Some(Item::Box(b)) if matches!(**b, Node::Grid { .. })) && ch(i + 2) == Some(']') => {
                if let Some(Item::Box(b)) = items.get(i + 1) {
                    if let Node::Grid { rows } = &**b {
                        write_matrix("bmatrix", rows, out, depth);
                    }
                }
                i += 3;
                continue;
            }
            Item::Char('(') if matches!(items.get(i + 1), Some(Item::Box(b)) if matches!(**b, Node::Grid { .. })) && ch(i + 2) == Some(')') => {
                if let Some(Item::Box(b)) = items.get(i + 1) {
                    if let Node::Grid { rows } = &**b {
                        write_matrix("pmatrix", rows, out, depth);
                    }
                }
                i += 3;
                continue;
            }
            Item::Char(c) => match char_latex(*c) {
                Some(s) => out.push_str(s),
                None if ch(i + 1) == Some('>') && *c == '-' => {
                    out.push_str("\\to ");
                    i += 2;
                    continue;
                }
                None if ch(i + 1) == Some('=') && matches!(c, '<' | '>' | '!' | '=') => {
                    out.push_str(match c {
                        '<' => "\\le ",
                        '>' => "\\ge ",
                        '!' => "\\ne ",
                        _ => "=",
                    });
                    i += 2;
                    continue;
                }
                None => out.push(*c),
            },
        }
        i += 1;
    }
}

fn write_name(name: &str, called: bool, out: &mut String) {
    if let Some(s) = named_symbol(name) {
        out.push_str(s);
        out.push(' ');
    } else if name.chars().count() == 1 || name.chars().all(|c| c.is_ascii_digit()) {
        out.push_str(name);
    } else if FUNCTIONS.contains(&name) {
        out.push('\\');
        out.push_str(name);
        out.push(' ');
    } else if called {
        out.push_str("\\operatorname{");
        out.push_str(&text_escape(name));
        out.push('}');
    } else {
        // a letter with digits is a subscripted symbol in writing (`x1` is x₁); longer names are words
        let letters: String = name.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        let rest = &name[letters.len()..];
        if letters.len() == 1 && !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            out.push_str(&format!("{letters}_{{{rest}}}"));
        } else {
            out.push_str("\\text{");
            out.push_str(&text_escape(name));
            out.push('}');
        }
    }
}

/// Characters escaped inside `\text{}`.
fn text_escape(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        match c {
            '_' | '#' | '$' | '%' | '&' | '{' | '}' => {
                o.push('\\');
                o.push(c);
            }
            '\\' => o.push_str("\\textbackslash{}"),
            '~' => o.push_str("\\textasciitilde{}"),
            '^' => o.push_str("\\textasciicircum{}"),
            _ => o.push(c),
        }
    }
    o
}

/// A slot in braces.
fn group(r: &Row, out: &mut String, depth: usize) {
    out.push('{');
    write_row(r, out, depth + 1);
    out.push('}');
}

fn write_matrix(env: &str, rows: &[Vec<Row>], out: &mut String, depth: usize) {
    out.push_str(&format!("\\begin{{{env}}} "));
    for (k, r) in rows.iter().enumerate() {
        if k > 0 {
            out.push_str(" \\\\ ");
        }
        for (j, cell) in r.iter().enumerate() {
            if j > 0 {
                out.push_str(" & ");
            }
            write_row(cell, out, depth + 1);
        }
    }
    out.push_str(&format!(" \\end{{{env}}}"));
}

fn write_node(n: &Node, out: &mut String, depth: usize) {
    if depth > crate::tree::MAX_NESTING {
        return;
    }
    match n {
        Node::Fraction { num, den } => {
            out.push_str("\\frac");
            group(num, out, depth);
            group(den, out, depth);
        }
        Node::Superscript { base, exp } => {
            group(base, out, depth);
            out.push('^');
            group(exp, out, depth);
        }
        Node::Subscript { base, index } => {
            group(base, out, depth);
            out.push('_');
            group(index, out, depth);
        }
        Node::Radical { radicand, index } => {
            out.push_str("\\sqrt");
            if let Some(i) = index {
                out.push('[');
                write_row(i, out, depth + 1);
                out.push(']');
            }
            group(radicand, out, depth);
        }
        Node::Overscript { base, over } => {
            out.push_str("\\overset");
            group(over, out, depth);
            group(base, out, depth);
        }
        Node::Underscript { base, under } => {
            out.push_str("\\underset");
            group(under, out, depth);
            group(base, out, depth);
        }
        Node::Underoverscript { base, under, over } => {
            out.push_str("\\underset");
            group(under, out, depth);
            out.push_str("{\\overset");
            group(over, out, depth);
            group(base, out, depth);
            out.push('}');
        }
        Node::Grid { rows } => write_matrix("matrix", rows, out, depth),
        Node::BigOp { op, body, var, lower, upper } => match op {
            BigOp::Sum | BigOp::Product => {
                out.push_str(if *op == BigOp::Sum { "\\sum" } else { "\\prod" });
                match (lower, upper) {
                    (Some(lo), Some(hi)) => {
                        out.push_str("_{");
                        write_row(var, out, depth + 1);
                        out.push('=');
                        write_row(lo, out, depth + 1);
                        out.push_str("}^");
                        group(hi, out, depth);
                    }
                    _ => {
                        out.push('_');
                        group(var, out, depth);
                    }
                }
                out.push(' ');
                write_row(body, out, depth + 1);
            }
            BigOp::Integral => {
                out.push_str("\\int");
                if let (Some(lo), Some(hi)) = (lower, upper) {
                    out.push('_');
                    group(lo, out, depth);
                    out.push('^');
                    group(hi, out, depth);
                }
                out.push(' ');
                write_row(body, out, depth + 1);
                out.push_str(" \\, d");
                write_row(var, out, depth + 1);
            }
            BigOp::Limit => {
                out.push_str("\\lim_{");
                write_row(var, out, depth + 1);
                out.push_str(" \\to ");
                if let Some(p) = lower {
                    write_row(p, out, depth + 1);
                }
                out.push_str("} ");
                write_row(body, out, depth + 1);
            }
            BigOp::Derivative => {
                out.push_str("\\frac{\\partial}{\\partial ");
                write_row(var, out, depth + 1);
                out.push_str("} ");
                write_row(body, out, depth + 1);
            }
        },
        Node::Graphic { path } => {
            out.push_str("\\text{[");
            out.push_str(&text_escape(path));
            out.push_str("]}");
        }
        Node::Labeled { value, label, side, .. } => {
            // the label as upright text (a legend's entries one after the other), its swatches left out
            let mut text = String::new();
            for it in &label.0 {
                match it {
                    Item::Char(c) if crate::math::is_swatch(*c) => {}
                    Item::Char('\n') => text.push_str("; "),
                    Item::Char(c) => text.push(*c),
                    _ => {}
                }
            }
            let lbl = format!("\\text{{{}}}", text_escape(text.trim()));
            match side {
                Side::Below | Side::Above => {
                    out.push_str(if *side == Side::Below { "\\underset{" } else { "\\overset{" });
                    out.push_str(&lbl);
                    out.push('}');
                    group(value, out, depth);
                }
                Side::Left => {
                    out.push_str(&lbl);
                    out.push_str("\\quad ");
                    write_row(value, out, depth + 1);
                }
                Side::Right => {
                    write_row(value, out, depth + 1);
                    out.push_str("\\quad ");
                    out.push_str(&lbl);
                }
            }
        }
    }
}

/// Collapse runs of spaces and trim (commands keep the space that ends them).
fn tidy(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' {
            space = true;
            continue;
        }
        if space && !o.is_empty() && !o.ends_with('\n') && !matches!(c, '}' | ')' | ']' | '^' | '_' | ',') {
            o.push(' ');
        }
        space = false;
        o.push(c);
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::from_text;
    use crate::printed::typeset_output;

    fn inp(t: &str) -> String {
        to_latex(&from_text(t))
    }

    fn out(t: &str) -> String {
        to_latex(&typeset_output(t).row)
    }

    #[test]
    fn boxes() {
        assert_eq!(inp("((a + b)/(2c))"), "\\frac{a + b}{2c}");
        assert_eq!(inp("((x)^(2)) + ((subscript(y, i)))"), "{x}^{2} + {y}_{i}");
        assert_eq!(inp("((sqrt(x))) + ((root(x, 3)))"), "\\sqrt{x} + \\sqrt[3]{x}");
        assert_eq!(inp("(([[a, b], [c, d]]))"), "\\begin{matrix} a & b \\\\ c & d \\end{matrix}");
        assert_eq!(inp("((sum(f(k), [k, 1, n])))"), "\\sum_{k=1}^{n} f(k)");
        assert_eq!(inp("((integrate(f(x), [x, 0, 1])))"), "\\int_{0}^{1} f(x) \\, dx");
        assert_eq!(inp("((limit(sin(x)/x, x -> 0)))"), "\\lim_{x \\to 0} \\sin (x)/x");
        assert_eq!(inp("((overscript(x, \"^\")))"), "\\overset{\\text{\\textasciicircum{}}}{x}");
    }

    #[test]
    fn characters_and_names() {
        assert_eq!(inp("2*pi*r"), "2 \\pi r");
        assert_eq!(inp("alpha + β ≤ x1"), "\\alpha + \\beta \\le x_{1}");
        assert_eq!(inp("x -> 1, y != 2, z >= 3"), "x \\to 1, y \\ne 2, z \\ge 3");
        assert_eq!(inp("n_integrate(f, x)"), "\\operatorname{n\\_integrate}(f, x)");
        assert_eq!(inp("total + 1"), "\\text{total} + 1");
        assert_eq!(inp("\"a b\""), "\\text{a b}");
        assert_eq!(inp("{x}"), "\\{x\\}");
    }

    #[test]
    fn outputs() {
        assert_eq!(out("(2/3)*2^(1/2)"), "\\frac{2 \\sqrt{2}}{3}");
        assert_eq!(out("[[1, 2], [3, 4]]"), "\\begin{bmatrix} 1 & 2 \\\\ 3 & 4 \\end{bmatrix}");
        assert_eq!(out("x^2 - 2*x*y"), "{x}^{2} - 2 x y");
        assert_eq!(out("2*3"), "2\\times 3");
        assert_eq!(out("pi + inf"), "\\pi + \\infty");
    }
}
