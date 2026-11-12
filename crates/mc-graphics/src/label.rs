//! Labels: text and expressions drawn in graphics (tick labels, axis and frame labels, legends, `text` primitives).
//!
//! A label is a box row ([`mc_boxes::Row`]) laid out by `mc-boxes` with the math font's metrics, so every glyph of
//! the bundled STIX Two Math is available and scripts, fractions and radicals are set by the font's MATH rules.
//!
//! - A **string** is text; between `$` signs it is a small subset of TeX math: `^` and `_` with a single character,
//!   a command or a `{group}`; commands from the alias table (`\alpha`, `\infty`, `\le`, `\partial`, ...);
//!   `\frac{a}{b}`, `\sqrt{x}`, `\mathrm{text}` and `\text{text}`; `\,` `\;` `\ ` for spaces. Latin letters in
//!   math are italic, digits upright, `-` is a minus sign. `\$` is a dollar sign. Unknown commands are kept as
//!   written. Nesting deeper than [`MAX_DEPTH`] is kept as plain text.
//! - `subscript(a, b)` and `superscript(a, b)` set `b` as an index or exponent of `a` (each a label itself).
//! - Any other **expression** is printed and typeset like notebook output, in traditional form (single-letter
//!   names italic): `x^2/2` gets a raised exponent and a built-up fraction.
use mc_boxes::layout::{layout, Draw, LayoutOptions, MathFontMetrics};
use mc_boxes::tree::{Item, Node, Row};
use mc_api::{Store, View, T};

/// Groups and scripts nested deeper than this are not interpreted.
pub const MAX_DEPTH: usize = 16;

/// An integer of more limbs than this (about 20,000 digits) is labelled by its leading digits and exponent.
const HUGE_LIMBS: usize = 1000;

/// A label ready to typeset.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    /// the content
    pub row: Row,
    /// single-letter names in italics, operators spaced (labels from expressions)
    pub traditional: bool,
    /// operators spaced: the label has math in it
    pub math: bool,
    /// the text as written (a string label's source, an expression's printed form), for text extraction and
    /// accessibility
    pub text: String,
    /// its own font size in points (`style(x, font_size(n))`), over the size it is typeset at
    pub size: Option<f32>,
}

/// A label string (with `$...$` math) as plain text for places that cannot draw it, such as a tooltip: TeX commands
/// become their characters (`$\sin\theta$` gives `sinθ`), scripts `^` / `_` (in parentheses when longer than one
/// character), fractions `a/b`, roots with `√`.
pub fn plain_text(s: &str) -> String {
    if !s.contains('$') {
        return s.to_string();
    }
    let mut out = String::new();
    row_text(&Label::text(s).row, &mut out);
    out
}

/// A mathematical italic letter (how math letters are set) as its plain letter.
fn plain_char(c: char) -> char {
    let u = c as u32;
    let from = |base: u32, to: u32| char::from_u32(to + (u - base)).unwrap_or(c);
    match u {
        0x1D434..=0x1D44D => from(0x1D434, 'A' as u32),
        0x1D44E..=0x1D467 => from(0x1D44E, 'a' as u32),
        0x210E => 'h',
        0x1D6E2..=0x1D6FA => from(0x1D6E2, 0x391),
        0x1D6FC..=0x1D714 => from(0x1D6FC, 0x3B1),
        _ => c,
    }
}

fn row_text(r: &Row, out: &mut String) {
    for item in &r.0 {
        match item {
            Item::Char(c) => out.push(plain_char(*c)),
            Item::Placeholder => {}
            Item::Box(node) => node_text(node, out),
        }
    }
}

/// A script, numerator or radicand: in parentheses when it is longer than one character.
fn part(r: &Row, out: &mut String) {
    let mut t = String::new();
    row_text(r, &mut t);
    if t.chars().count() > 1 {
        out.push('(');
        out.push_str(&t);
        out.push(')');
    } else {
        out.push_str(&t);
    }
}

fn node_text(n: &Node, out: &mut String) {
    match n {
        Node::Fraction { num, den } => {
            part(num, out);
            out.push('/');
            part(den, out);
        }
        Node::Superscript { base, exp } => {
            row_text(base, out);
            out.push('^');
            part(exp, out);
        }
        Node::Subscript { base, index } => {
            row_text(base, out);
            out.push('_');
            part(index, out);
        }
        Node::Radical { radicand, .. } => {
            out.push('√');
            part(radicand, out);
        }
        other => {
            for r in other.slots() {
                row_text(r, out);
            }
        }
    }
}

impl Label {
    /// Plain text, nothing interpreted (tick labels).
    pub fn plain(s: &str) -> Label {
        Label { row: Row(s.chars().map(Item::Char).collect()), traditional: false, math: false, text: s.to_string(), size: None }
    }

    /// A string label: text with `$...$` math.
    pub fn text(s: &str) -> Label {
        let mut p = TexParser { chars: s.chars().collect(), pos: 0, math: false, upright: false };
        let row = p.text_row();
        Label { row, traditional: false, math: s.contains('$'), text: s.to_string(), size: None }
    }

    /// The label of an expression (see the module documentation).
    pub fn of<S: Store + ?Sized>(h: &S, t: T<'_>) -> Label {
        if let View::Str(s) = h.view(t) {
            return Label::text(s);
        }
        // an integer of tens of thousands of digits: its leading digits and exponent (printing all of it takes
        // seconds and no drawing shows it; X26-13)
        if let View::BigInt { neg, mag } = h.view(t) {
            if mag.len() > HUGE_LIMBS {
                return Label::huge_integer(neg, mag);
            }
        }
        // a styled label: its content at its font size (other styles are not used here)
        if let View::App { head, args } = h.view(t) {
            if !args.is_empty() && matches!(h.view(head), View::Sym(f) if h.sym_name(f) == "style") {
                let mut st = crate::model::Style::default();
                let _ = crate::model::styled(h, &args[1..], &mut st);
                let mut l = Label::of(h, args[0]);
                l.size = st.font_size.map(|v| v as f32).or(l.size);
                return l;
            }
        }
        let row = term_row(h, t, 0);
        Label { row, traditional: true, math: true, text: mc_syntax::Printer::new(h).expr(t), size: None }
    }

    /// The label of an integer too long to print: `≈ d.dddd×10^k` from its top limbs (the value to six digits).
    fn huge_integer(neg: bool, mag: &[u64]) -> Label {
        let n = mag.len();
        let top = mag[n - 1];
        let bits = top.leading_zeros() as usize;
        // the top 128 bits as a float, and the number of bits below them
        let hi = ((top as u128) << 64 | mag[n - 2] as u128) << bits;
        let lead = hi as f64;
        let shift = (n - 2) as f64 * 64.0 - bits as f64;
        let log10 = lead.log10() + shift * std::f64::consts::LOG10_2;
        let k = log10.floor();
        let m = 10f64.powf(log10 - k);
        let mantissa = format!("{}{m:.5}", if neg { "\u{2212}" } else { "" });
        let mut l = Label::scientific(&mantissa, k as i64);
        l.text = format!("\u{2248} {}", l.text);
        l.row.0.insert(0, Item::Char('\u{2248}'));
        l
    }

    /// A number written `m × 10^k` (the exponent raised), or plainly when `k` is 0.
    pub fn scientific(mantissa: &str, k: i64) -> Label {
        let ten = Row(vec![Item::Box(Box::new(Node::Superscript {
            base: Row::text("10"),
            exp: Row::text(&k.to_string().replace('-', "\u{2212}")),
        }))]);
        let mut items: Vec<Item> = Vec::new();
        if mantissa != "1" {
            items.extend(mantissa.chars().map(Item::Char));
            items.push(Item::Char('\u{d7}'));
        }
        items.extend(ten.0);
        Label { row: Row(items), traditional: false, math: true, text: format!("{mantissa}\u{d7}10^{k}"), size: None }
    }
}

/// The row of an expression label.
fn term_row<S: Store + ?Sized>(h: &S, t: T<'_>, depth: usize) -> Row {
    match h.view(t) {
        View::Str(s) => return Label::text(s).row,
        View::App { head, args: &[a, b] } if depth < MAX_DEPTH => {
            if let View::Sym(f) = h.view(head) {
                let (a, b) = (term_row(h, a, depth + 1), term_row(h, b, depth + 1));
                match h.sym_name(f).as_str() {
                    "superscript" => return Row::boxed(Node::Superscript { base: a, exp: b }),
                    "subscript" => return Row::boxed(Node::Subscript { base: a, index: b }),
                    _ => {}
                }
            }
        }
        _ => {}
    }
    let printed = mc_syntax::Printer::new(h).expr(t);
    mc_typeset::typeset_output(&printed).row
}

/// A recursive-descent reader of the TeX subset; depth is bounded by [`MAX_DEPTH`].
struct TexParser {
    chars: Vec<char>,
    pos: usize,
    /// inside `$...$`
    math: bool,
    /// letters upright to the end of the group (`\rm`)
    upright: bool,
}

impl TexParser {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    /// The whole string: text and math segments.
    fn text_row(&mut self) -> Row {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                '\\' if self.chars.get(self.pos + 1) == Some(&'$') => {
                    out.push(Item::Char('$'));
                    self.pos += 2;
                }
                '$' => {
                    self.pos += 1;
                    self.math = true;
                    let r = self.math_row(0, false);
                    out.extend(r.0);
                    self.math = false;
                    if self.peek() == Some('$') {
                        self.pos += 1;
                    }
                }
                _ => {
                    out.push(Item::Char(c));
                    self.pos += 1;
                }
            }
        }
        Row(out)
    }

    /// Math up to the closing `$` (or `}` in a group, when `group`).
    fn math_row(&mut self, depth: usize, group: bool) -> Row {
        // a font switch (`\rm`) lasts to the end of its group
        let upright = self.upright;
        let mut out: Vec<Item> = Vec::new();
        while let Some(c) = self.peek() {
            match c {
                '$' => break,
                '}' if group => {
                    self.pos += 1;
                    self.upright = upright;
                    return Row(out);
                }
                '^' | '_' => {
                    self.pos += 1;
                    let script = self.atom(depth + 1);
                    let base = out.pop().map_or_else(Row::new, |i| Row(vec![i]));
                    let node = match (c, base.0.as_slice()) {
                        // `x_1^2`: both scripts stacked on the right of x
                        ('^', [Item::Box(b)]) if matches!(**b, Node::Subscript { .. }) => {
                            Node::Superscript { base, exp: script }
                        }
                        ('^', _) => Node::Superscript { base, exp: script },
                        _ => Node::Subscript { base, index: script },
                    };
                    out.push(Item::Box(Box::new(node)));
                }
                _ => out.extend(self.atom(depth).0),
            }
        }
        self.upright = upright;
        Row(out)
    }

    /// One atom: a group, a command or a character.
    fn atom(&mut self, depth: usize) -> Row {
        let Some(c) = self.peek() else { return Row::new() };
        if depth >= MAX_DEPTH {
            // too deep: the rest as written
            let rest: String = self.chars[self.pos..].iter().take_while(|&&c| c != '$').collect();
            self.pos += rest.chars().count();
            return Row::text(&rest);
        }
        self.pos += 1;
        match c {
            '{' => self.math_row(depth + 1, true),
            '\\' => self.command(depth),
            ' ' if self.math => Row::new(),
            // a tie: a space that does not break
            '~' if self.math => Row::text(" "),
            c if self.math && self.upright && c.is_ascii_alphabetic() => Row(vec![Item::Char(c)]),
            c => Row(vec![Item::Char(if self.math { math_letter(c) } else { c })]),
        }
    }

    /// The command after a backslash.
    fn command(&mut self, depth: usize) -> Row {
        let start = self.pos;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.pos += 1;
        }
        if self.pos == start {
            // a one-character command: spaces, or an escaped character
            let Some(c) = self.peek() else { return Row::text("\\") };
            self.pos += 1;
            return match c {
                ',' | ';' | ' ' | ':' => Row::text("\u{2009}"),
                c => Row(vec![Item::Char(c)]),
            };
        }
        let name: String = self.chars[start..self.pos].iter().collect();
        match name.as_str() {
            "frac" => {
                let num = self.atom(depth + 1);
                let den = self.atom(depth + 1);
                Row::boxed(Node::Fraction { num, den })
            }
            "sqrt" => Row::boxed(Node::Radical { radicand: self.atom(depth + 1), index: None }),
            "mathrm" | "text" | "textrm" | "operatorname" => {
                let was = self.math;
                self.math = false;
                let r = self.atom(depth + 1);
                self.math = was;
                r
            }
            "mathit" => self.atom(depth + 1),
            // font switches to the end of the group: upright (`\rm`, `\rmfamily`, `\upshape`), italic
            // (`\it`), bold (drawn upright)
            "rm" | "rmfamily" | "upshape" | "bf" | "bfseries" | "it" | "itshape" | "mit" => {
                self.upright = !matches!(name.as_str(), "it" | "itshape" | "mit");
                while self.peek() == Some(' ') {
                    self.pos += 1;
                }
                Row::new()
            }
            // letter styles: the letters of the argument in the math alphabets
            "mathcal" | "mathscr" | "mathbb" | "mathbf" | "boldsymbol" | "mathfrak" | "mathsf" | "mathtt" => {
                let was = self.math;
                self.math = false;
                let mut r = self.atom(depth + 1);
                self.math = was;
                for it in r.0.iter_mut() {
                    if let Item::Char(c) = it {
                        *c = styled_letter(&name, *c);
                    }
                }
                r
            }
            // accents: a combining mark after the (last character of the) argument
            "bar" | "overline" | "hat" | "widehat" | "tilde" | "widetilde" | "vec" | "dot" | "ddot" | "check" | "breve"
            | "acute" | "grave" => {
                let mark = match name.as_str() {
                    "bar" => '\u{0304}',
                    "overline" => '\u{0305}',
                    "hat" | "widehat" => '\u{0302}',
                    "tilde" | "widetilde" => '\u{0303}',
                    "vec" => '\u{20D7}',
                    "dot" => '\u{0307}',
                    "ddot" => '\u{0308}',
                    "check" => '\u{030C}',
                    "breve" => '\u{0306}',
                    "acute" => '\u{0301}',
                    _ => '\u{0300}',
                };
                let mut r = self.atom(depth + 1);
                r.0.push(Item::Char(mark));
                r
            }
            // sizes of delimiters: the delimiter itself follows
            "left" | "right" | "big" | "Big" | "bigg" | "Bigg" | "bigl" | "bigr" | "Bigl" | "Bigr" => Row::new(),
            // operator names: upright
            n if OPERATOR_NAMES.contains(&n) => Row::text(n),
            _ => match mc_boxes::aliases::lookup(&format!("\\{name}")) {
                Some(mc_boxes::aliases::Alias::Text(s)) => Row::text(s),
                _ => Row::text(&format!("\\{name}")),
            },
        }
    }
}

/// Commands that set a function's name upright (`\sin x`).
const OPERATOR_NAMES: [&str; 30] = [
    "sin", "cos", "tan", "cot", "sec", "csc", "arcsin", "arccos", "arctan", "sinh", "cosh", "tanh", "coth", "log",
    "ln", "lg", "exp", "lim", "max", "min", "sup", "inf", "det", "dim", "ker", "deg", "gcd", "arg", "Pr", "mod",
];

/// A character in math: Latin letters italic, `-` a minus sign, others as they are.
fn math_letter(c: char) -> char {
    let shifted = match c {
        'h' => return '\u{210e}',
        'a'..='z' => 0x1d44e + (c as u32 - 'a' as u32),
        'A'..='Z' => 0x1d434 + (c as u32 - 'A' as u32),
        '-' => return '\u{2212}',
        _ => return c,
    };
    char::from_u32(shifted).unwrap_or(c)
}

/// A letter or digit in a math alphabet (`\mathcal`, `\mathscr`: script; `\mathbb`: double-struck; `\mathbf`,
/// `\boldsymbol`: bold; `\mathfrak`: fraktur; `\mathsf`: sans-serif; `\mathtt`: monospace), with the letters that
/// Unicode keeps in the Letterlike Symbols block; other characters as they are.
fn styled_letter(style: &str, c: char) -> char {
    let (upper, lower, digits): (u32, Option<u32>, Option<u32>) = match style {
        "mathcal" | "mathscr" => (0x1d49c, Some(0x1d4b6), None),
        "mathbb" => (0x1d538, Some(0x1d552), Some(0x1d7d8)),
        "mathbf" | "boldsymbol" => (0x1d400, Some(0x1d41a), Some(0x1d7ce)),
        "mathfrak" => (0x1d504, Some(0x1d51e), None),
        "mathsf" => (0x1d5a0, Some(0x1d5ba), Some(0x1d7e2)),
        "mathtt" => (0x1d670, Some(0x1d68a), Some(0x1d7f6)),
        _ => return c,
    };
    // holes of the math alphabets, filled by older characters
    let hole = match (style, c) {
        ("mathcal" | "mathscr", 'B') => Some('\u{212c}'),
        ("mathcal" | "mathscr", 'E') => Some('\u{2130}'),
        ("mathcal" | "mathscr", 'F') => Some('\u{2131}'),
        ("mathcal" | "mathscr", 'H') => Some('\u{210b}'),
        ("mathcal" | "mathscr", 'I') => Some('\u{2110}'),
        ("mathcal" | "mathscr", 'L') => Some('\u{2112}'),
        ("mathcal" | "mathscr", 'M') => Some('\u{2133}'),
        ("mathcal" | "mathscr", 'R') => Some('\u{211b}'),
        ("mathcal" | "mathscr", 'e') => Some('\u{212f}'),
        ("mathcal" | "mathscr", 'g') => Some('\u{210a}'),
        ("mathcal" | "mathscr", 'o') => Some('\u{2134}'),
        ("mathbb", 'C') => Some('\u{2102}'),
        ("mathbb", 'H') => Some('\u{210d}'),
        ("mathbb", 'N') => Some('\u{2115}'),
        ("mathbb", 'P') => Some('\u{2119}'),
        ("mathbb", 'Q') => Some('\u{211a}'),
        ("mathbb", 'R') => Some('\u{211d}'),
        ("mathbb", 'Z') => Some('\u{2124}'),
        ("mathfrak", 'C') => Some('\u{212d}'),
        ("mathfrak", 'H') => Some('\u{210c}'),
        ("mathfrak", 'I') => Some('\u{2111}'),
        ("mathfrak", 'R') => Some('\u{211c}'),
        ("mathfrak", 'Z') => Some('\u{2128}'),
        _ => None,
    };
    if let Some(h) = hole {
        return h;
    }
    let code = match c {
        'A'..='Z' => upper + (c as u32 - 'A' as u32),
        'a'..='z' => match lower {
            Some(l) => l + (c as u32 - 'a' as u32),
            None => return c,
        },
        '0'..='9' => match digits {
            Some(d) => d + (c as u32 - '0' as u32),
            None => return c,
        },
        _ => return c,
    };
    char::from_u32(code).unwrap_or(c)
}

/// A glyph of a typeset label: its id in the math font, the character it shows (when it is one), its baseline
/// origin (y down from the label's top) and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph {
    /// glyph id in [`mc_mathfont::font`]
    pub id: u16,
    /// the character, for text extraction (`None` for pieces of stretched delimiters)
    pub ch: Option<char>,
    /// origin x
    pub x: f32,
    /// baseline y
    pub y: f32,
    /// font size
    pub size: f32,
}

/// A typeset label: glyphs and rules in a box with its top-left corner at the origin, y down.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Typeset {
    /// width
    pub width: f32,
    /// height
    pub height: f32,
    /// baseline of the first line
    pub baseline: f32,
    /// the glyphs
    pub glyphs: Vec<Glyph>,
    /// filled rectangles (fraction bars, radical overbars): x, y, w, h
    pub rules: Vec<[f32; 4]>,
    /// the characters in drawing order, spaces included
    pub text: String,
}

/// Lay a label out at font size `size`.
pub fn typeset(label: &Label, size: f32) -> Typeset {
    let size = label.size.unwrap_or(size);
    let opts = LayoutOptions {
        size,
        display: false,
        max_width: None,
        indent_continuations: false,
        operator_spacing: label.math,
        traditional: label.traditional,
        small_inline_rationals: true,
        // labels carry glyphs and rules only: tall brackets come from the math font
        vector_delimiters: false,
    };
    let lay = layout(&label.row, &MathFontMetrics, &opts);
    let font = mc_mathfont::font();
    let glyph_of = |c: char| font.glyph(c).or_else(|| font.glyph('?')).unwrap_or(0);
    let mut out = Typeset {
        width: lay.width,
        height: lay.height,
        baseline: lay.baseline,
        text: label.text.clone(),
        ..Default::default()
    };
    for d in &lay.items {
        match *d {
            Draw::Char { c, size, x, y, .. } => {
                if !c.is_whitespace() {
                    out.glyphs.push(Glyph { id: glyph_of(c), ch: Some(c), x, y, size });
                }
            }
            Draw::Placeholder { x, y, size, .. } => {
                out.glyphs.push(Glyph { id: glyph_of('\u{25a1}'), ch: Some('\u{25a1}'), x, y, size })
            }
            Draw::Glyph { glyph, size, x, y } => out.glyphs.push(Glyph { id: glyph, ch: None, x, y, size }),
            Draw::Rule { x, y, w, h } => out.rules.push([x, y, w, h]),
            // not produced: vector delimiters are off for labels
            Draw::Delim { .. } | Draw::Graphic { .. } => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_boxes::tree::notation;

    #[test]
    fn plain_text_of_math_labels() {
        assert_eq!(plain_text("time (s)"), "time (s)");
        assert_eq!(plain_text(r"$\sin\theta$"), "sin\u{3b8}");
        assert_eq!(plain_text("$x_1^2$ data"), "x_1^2 data");
        assert_eq!(plain_text(r"$\frac{a}{bc}$"), "a/(bc)");
    }

    fn tex(s: &str) -> String {
        notation(&Label::text(s).row)
    }

    /// Gap queue G84: a tie is a space, `\rm` sets the letters upright to the end of the group.
    #[test]
    fn ties_and_font_switches() {
        let t = tex(r"$\tau~T_n$");
        assert!(t.starts_with("\u{3c4} ") && !t.contains('~'), "{t}");
        let t = tex(r"$\rho~\rm(GeV^4)$");
        assert!(t.starts_with("\u{3c1} (Ge(V^4))") && !t.contains("rm"), "{t}");
        // to the end of the group only
        assert_eq!(tex(r"${\rm d}x$"), "d\u{1d465}");
        assert_eq!(tex(r"$\rm a \it b$"), "a\u{1d44f}");
        // outside math a tilde is a tilde
        assert_eq!(tex("~5 s"), "~5 s");
    }

    /// Gap queue G43: letter styles.
    #[test]
    fn letter_styles() {
        assert_eq!(tex(r"$\mathcal{A}\mathcal{B}$"), "\u{1d49c}\u{212c}");
        assert_eq!(tex(r"$\mathbb{R}\mathbb{x}$"), "\u{211d}\u{1d569}");
        assert_eq!(tex(r"$\mathbf{v}2$"), "\u{1d42f}2");
        assert!(!tex(r"$\ln \mathcal{A}$").contains("mathcal"));
    }

    #[test]
    fn tex_subset() {
        assert_eq!(tex("time (s)"), "time (s)");
        assert_eq!(tex("$x^2$"), "(\u{1d465}^2)");
        assert_eq!(tex("$\\alpha_1^2$"), "((\u{3b1}_1)^2)");
        assert_eq!(tex("$e^{-x}$"), "(\u{1d452}^\u{2212}\u{1d465})");
        assert_eq!(tex("cost \\$5"), "cost $5");
        assert_eq!(tex("$\\mathrm{d}x$"), "d\u{1d465}");
        assert_eq!(tex("$\\nosuch$"), "\\nosuch");
        assert_eq!(tex("$\\sin x$"), "sin\u{1d465}");
        assert_eq!(tex("$\\frac{1}{2}$"), "(1/2)");
        // unclosed math and groups end at the end of the string
        assert_eq!(tex("$x_{1"), "(\u{1d465}_1)");
        assert_eq!(tex("$^2$"), "(^2)");
    }

    #[test]
    fn deep_nesting_is_bounded() {
        let deep = format!("${}x{}$", "{".repeat(100), "}".repeat(100));
        let l = Label::text(&deep);
        assert!(!l.row.0.is_empty());
        let t = typeset(&l, 10.0);
        assert!(t.width > 0.0);
    }

    #[test]
    fn typesetting_gives_glyphs() {
        let t = typeset(&Label::text("$\\alpha_1^2$"), 20.0);
        let at = |c: char| *t.glyphs.iter().find(|g| g.ch == Some(c)).expect("the glyph is there");
        assert_eq!(t.glyphs.len(), 3);
        let (base, sub, sup) = (at('\u{3b1}'), at('1'), at('2'));
        assert!(sub.size < 20.0 && sub.y > base.y, "{:?}", t.glyphs);
        assert!(sup.y < base.y && sub.x == sup.x);
        let f = typeset(&Label::text("$\\frac{a}{b}$"), 20.0);
        assert_eq!(f.rules.len(), 1);
        let sci = typeset(&Label::scientific("2", -3), 10.0);
        assert_eq!(sci.glyphs.iter().filter_map(|g| g.ch).collect::<String>(), "2\u{d7}10\u{2212}3");
    }
}
