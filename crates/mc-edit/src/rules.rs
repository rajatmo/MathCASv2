//! The text behaviour of a row of items, written once for every row (design 11, principle 3): words, lines, where a
//! position is lexically (code, string, comment), implicit multiplication next to boxes, and what a removed box
//! leaves behind.
use mc_boxes::ops::{is_close, is_word, word_start};
use mc_boxes::{Item, Node, Row};

/// The indentation Tab inserts (decision NB-core.5: spaces, so files and the painter agree on widths).
pub const INDENT: &str = "    ";

/// The class of an item for "natural word" motion: identifiers and numbers are words, runs of operator characters
/// are words, white space separates, and every box or placeholder is a word of its own.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Class {
    Space,
    Word,
    Punct,
    Box,
}

fn class(it: &Item) -> Class {
    match it {
        Item::Char(c) if c.is_whitespace() => Class::Space,
        Item::Char(c) if c.is_alphanumeric() || *c == '_' => Class::Word,
        Item::Char(_) => Class::Punct,
        _ => Class::Box,
    }
}

/// The start of the word before `k` (white space before it skipped first).
pub fn word_left(row: &Row, k: usize) -> usize {
    let mut i = k.min(row.len());
    while i > 0 && class(&row.0[i - 1]) == Class::Space {
        i -= 1;
    }
    if i == 0 {
        return 0;
    }
    let c = class(&row.0[i - 1]);
    if c == Class::Box {
        return i - 1;
    }
    while i > 0 && class(&row.0[i - 1]) == c {
        i -= 1;
    }
    i
}

/// The end of the word after `k`, then the white space after it on the same line.
pub fn word_right(row: &Row, k: usize) -> usize {
    let len = row.len();
    let start = k.min(len);
    let mut i = start;
    if let Some(it) = row.0.get(i) {
        match class(it) {
            Class::Space => {}
            Class::Box => i += 1,
            c => {
                while row.0.get(i).is_some_and(|x| class(x) == c) {
                    i += 1;
                }
            }
        }
    }
    while matches!(row.0.get(i), Some(Item::Char(c)) if c.is_whitespace() && *c != '\n') {
        i += 1;
    }
    if i == start && i < len {
        i += 1;
    }
    i
}

/// The word around `k` (a double-click): the run of same-class items touching it, a box alone.
pub fn word_at(row: &Row, k: usize) -> (usize, usize) {
    let k = k.min(row.len());
    let after = row.0.get(k).map(class);
    let before = k.checked_sub(1).and_then(|j| row.0.get(j)).map(class);
    let c = match (before, after) {
        (_, Some(Class::Word)) => Class::Word,
        (Some(Class::Word), _) => Class::Word,
        (_, Some(Class::Box)) => return (k, k + 1),
        (Some(Class::Box), None) => return (k - 1, k),
        (_, Some(c)) => c,
        (Some(c), None) => c,
        (None, None) => return (k, k),
    };
    let same = |it: &Item| class(it) == c && *it != Item::Char('\n');
    let mut a = k;
    while a > 0 && same(&row.0[a - 1]) {
        a -= 1;
    }
    let mut b = k;
    while row.0.get(b).is_some_and(same) {
        b += 1;
    }
    (a, b)
}

/// The start of the line holding `k` (after the line break before it).
pub fn line_start(row: &Row, k: usize) -> usize {
    (0..k.min(row.len())).rev().find(|&j| row.0[j] == Item::Char('\n')).map_or(0, |j| j + 1)
}

/// The end of the line holding `k` (before its line break).
pub fn line_end(row: &Row, k: usize) -> usize {
    (k.min(row.len())..row.len()).find(|&j| row.0[j] == Item::Char('\n')).unwrap_or(row.len())
}

/// The lines of a row as item ranges (split at line breaks, the break not included).
pub fn lines(row: &Row) -> Vec<std::ops::Range<usize>> {
    let mut out = vec![];
    let mut start = 0;
    for (i, it) in row.0.iter().enumerate() {
        if *it == Item::Char('\n') {
            out.push(start..i);
            start = i + 1;
        }
    }
    out.push(start..row.len());
    out
}

/// Where a position of source text is, for automatic closing of brackets and quotes and for Return.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lexical {
    /// in code
    Code,
    /// inside a string literal
    Str,
    /// inside a `#` comment
    Comment,
}

/// What offset `k` of `row` is inside: a string (`"..."` with `\` escapes, possibly over lines), a comment (`#` to the
/// line end) or code; boxes are code. Mirrors the lexer of mc-syntax for these two tokens.
pub fn lexical_at(row: &Row, k: usize) -> Lexical {
    let mut state = Lexical::Code;
    let mut items = row.0[..k.min(row.len())].iter();
    while let Some(it) = items.next() {
        let Item::Char(c) = it else { continue };
        state = match (state, *c) {
            (Lexical::Code, '"') => Lexical::Str,
            (Lexical::Code, '#') => Lexical::Comment,
            (Lexical::Str, '\\') => {
                items.next();
                Lexical::Str
            }
            (Lexical::Str, '"') | (Lexical::Comment, '\n') => Lexical::Code,
            (st, _) => st,
        };
    }
    state
}

/// Whether a `*` belongs at the boundary before item `j` of `row` (implicit multiplication next to boxes, a user
/// decision for the box editor): a box touches an operand at it, since the text would otherwise read as a call
/// (`x((sqrt(Q)))` is `x(sqrt(Q))`) or not read at all (`((sqrt(Q)))x`). Left of a box: a box, a closing bracket, a
/// prime, a string or an identifier (a number is left alone: `2((sqrt(2)))` already multiplies). Right of a box: a
/// box, an identifier or number, `(` or a string (`[` after a box is an index). With white space between them none
/// is needed (round 35: a space multiplies, and the editor shows a faint `×` there).
pub fn needs_times(row: &Row, j: usize) -> bool {
    if j == 0 || j >= row.len() {
        return false;
    }
    let (l, r) = (j - 1, j);
    let left_box = matches!(row.0[l], Item::Box(_));
    let right_box = matches!(row.0[r], Item::Box(_));
    let left_end = match row.0[l] {
        Item::Box(_) => true,
        Item::Char(c) if is_close(c) || c == '\'' || c == '"' => true,
        Item::Char(c) if is_word(c) => {
            let s = word_start(row, l + 1);
            matches!(row.0[s], Item::Char(f) if !f.is_ascii_digit() && f != '.')
        }
        _ => false,
    };
    let right_start = match row.0[r] {
        Item::Box(_) => true,
        Item::Char(c) => is_word(c) || c == '(' || c == '"',
        Item::Placeholder => false,
    };
    (right_box && left_end) || (left_box && right_start)
}

/// The linear items a box leaves when Backspace at the start of slot `slot` (or Delete at its end, `forward`)
/// removes it (decision NBUI.6), and where the caret goes among them: a radicand alone; `num/den` and `base^exp`
/// with parentheses around a slot that would not read back as one operand.
pub fn unwrap_box(b: &Node, slot: usize, forward: bool) -> (Vec<Item>, usize) {
    let clean = |r: &Row| -> Vec<Item> { r.0.iter().filter(|i| !matches!(i, Item::Placeholder)).cloned().collect() };
    let (a, op, z, paren_a, paren_z) = match b {
        Node::Radical { radicand, index: None } => {
            let items = clean(radicand);
            let at = if forward { items.len() } else { 0 };
            return (items, at);
        }
        Node::Fraction { num, den } => (num, '/', den, needs_parens(num, false), needs_parens(den, true)),
        Node::Superscript { base, exp } => {
            let exp_power = matches!(exp.0.as_slice(), [Item::Box(x)] if matches!(**x, Node::Superscript { .. }));
            (base, '^', exp, needs_parens(base, true), needs_parens(exp, !exp_power))
        }
        other => {
            let mut items = Vec::new();
            let mut at = 0;
            for (k, s) in other.slots().into_iter().enumerate() {
                if k == slot + usize::from(forward) {
                    at = items.len();
                }
                items.extend(clean(s));
            }
            return (items, at);
        }
    };
    let mut items = Vec::new();
    let wrap = |items: &mut Vec<Item>, r: &Row, p: bool| {
        if p {
            items.push(Item::Char('('));
        }
        items.extend(clean(r));
        if p {
            items.push(Item::Char(')'));
        }
    };
    wrap(&mut items, a, paren_a);
    let op_at = items.len();
    items.push(Item::Char(op));
    wrap(&mut items, z, paren_z);
    let at = match (slot, forward) {
        (0, false) => 0,
        (_, false) => op_at + 1,
        (0, true) => op_at,
        (_, true) => items.len(),
    };
    (items, at)
}

/// Whether a slot's contents need parentheses to stay one operand next to `/` or `^`: more than one item with an
/// operator, a space or a comma at its top level, or (`strict`, the side where a box would regroup) any box.
fn needs_parens(r: &Row, strict: bool) -> bool {
    let items: Vec<&Item> = r.0.iter().filter(|i| !matches!(i, Item::Placeholder)).collect();
    if items.len() <= 1 {
        return strict && matches!(items.first(), Some(Item::Box(_)));
    }
    let mut depth = 0i32;
    for i in &items {
        match i {
            Item::Char('(' | '[' | '{') => depth += 1,
            Item::Char(')' | ']' | '}') => depth -= 1,
            Item::Char(c) if depth == 0 && "+-*/^ ,=<>;:".contains(*c) => return true,
            Item::Box(_) if strict => return true,
            _ => {}
        }
    }
    false
}

/// The characters of a row (a box or placeholder as U+FFFC), for text rules that look at characters only.
pub fn chars(row: &Row, range: std::ops::Range<usize>) -> String {
    row.0[range].iter().map(|i| if let Item::Char(c) = i { *c } else { '\u{fffc}' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_match_the_text_rules() {
        let r = Row::text("f(aa + b*c, d)");
        assert_eq!(word_right(&r, 0), 1);
        assert_eq!(word_right(&r, 1), 2);
        assert_eq!(word_right(&r, 2), 5);
        assert_eq!(word_right(&r, 5), 7);
        assert_eq!(word_left(&r, 7), 5);
        assert_eq!(word_left(&r, 5), 2);
        assert_eq!(word_left(&r, 0), 0);
        assert_eq!(word_at(&r, 3), (2, 4));
        assert_eq!(word_at(&Row::text("x  y"), 2), (1, 3));
        assert_eq!(word_right(&Row::text("ab"), 2), 2);
    }

    #[test]
    fn lexical_places() {
        let r = Row::text("f(\"a(\" # (");
        assert_eq!(lexical_at(&r, 2), Lexical::Code);
        assert_eq!(lexical_at(&r, 4), Lexical::Str);
        assert_eq!(lexical_at(&r, 10), Lexical::Comment);
        assert_eq!(lines(&Row::text("a\nbc\n")), [0..1, 2..4, 5..5]);
    }
}
