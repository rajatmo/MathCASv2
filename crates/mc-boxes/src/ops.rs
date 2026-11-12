//! Character classes shared by layout and editing.
use crate::tree::{Item, Row};

/// Two- and three-character operators drawn as one glyph in Input and Output cells (design 08, section 3.2); the
/// caret steps over them as one unit and Backspace removes them whole. Longest first.
pub const OPERATOR_PAIRS: &[(&str, char)] =
    &[("<->", '↔'), ("->", '→'), (":>", '⧴'), ("<=", '≤'), (">=", '≥'), ("!=", '≠'), ("==", '⩵'), ("=>", '⇒')];

/// The operator pair starting at item `k` of `row`: its length in items and its glyph.
pub fn pair_at(row: &Row, k: usize) -> Option<(usize, char)> {
    'pairs: for (text, glyph) in OPERATOR_PAIRS {
        let n = text.chars().count();
        if k + n > row.0.len() {
            continue;
        }
        for (j, c) in text.chars().enumerate() {
            if row.0[k + j] != Item::Char(c) {
                continue 'pairs;
            }
        }
        // `===` and `=!=` are not `==` followed by `=`
        if *text == "==" && (row.0.get(k + 2) == Some(&Item::Char('=')) || (k > 0 && row.0[k - 1] == Item::Char('='))) {
            continue;
        }
        return Some((n, *glyph));
    }
    None
}

/// The operator pair containing item `k` (strictly inside or starting at it): (start, length).
pub fn pair_around(row: &Row, k: usize) -> Option<(usize, usize)> {
    (k.saturating_sub(2)..=k).filter_map(|s| pair_at(row, s).map(|(n, _)| (s, n))).find(|&(s, n)| s <= k && k < s + n)
}

/// Characters that continue a name or number: no line break and no operand boundary between two of them.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Opening brackets.
pub fn is_open(c: char) -> bool {
    matches!(c, '(' | '[' | '{')
}

/// Closing brackets.
pub fn is_close(c: char) -> bool {
    matches!(c, ')' | ']' | '}')
}

/// Whether `c` is drawn as a legend's swatch in typeset output: a short line (`line_legend`), a square
/// (`swatch_legend`), a dot (`point_legend`).
pub fn is_swatch(c: char) -> bool {
    matches!(c, '━' | '■' | '●')
}

/// The closing bracket of an opening one.
pub fn closer_of(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

/// The start of the primary ending at `k` in `row`, if any: a name or number token, a bracketed group with the name
/// it calls or indexes, a string, a box or a placeholder (the operand a box key takes, design 08, section 4.2).
pub fn primary_before(row: &Row, k: usize) -> Option<std::ops::Range<usize>> {
    if k == 0 || k > row.len() {
        return None;
    }
    let ch = |j: usize| match row.0.get(j) {
        Some(Item::Char(c)) => Some(*c),
        _ => None,
    };
    match &row.0[k - 1] {
        Item::Box(_) | Item::Placeholder => Some(k - 1..k),
        Item::Char(c) if is_close(*c) => {
            let open = matching_open(row, k - 1)?;
            // a call or an index: the name (or bracket group) it applies to
            let start = match open.checked_sub(1).and_then(ch) {
                Some(p) if is_word(p) || p == '\'' => word_start(row, open),
                _ => open,
            };
            Some(start..k)
        }
        Item::Char('"') => {
            let open = (0..k - 1).rev().find(|&j| ch(j) == Some('"') && (j == 0 || ch(j - 1) != Some('\\')))?;
            Some(open..k)
        }
        Item::Char(c) if is_word(*c) || *c == '\'' || *c == '.' => {
            let s = word_start(row, k);
            (s < k).then_some(s..k)
        }
        _ => None,
    }
}

/// The start of the last name or number token ending at `k` (`2x` gives `x`; `x1`, `1.5`, `f''` are one token).
pub fn word_start(row: &Row, k: usize) -> usize {
    let ch = |j: usize| match row.0.get(j) {
        Some(Item::Char(c)) => Some(*c),
        _ => None,
    };
    let mut run = k;
    while run > 0 && ch(run - 1).is_some_and(|c| is_word(c) || c == '.' || c == '\'') {
        run -= 1;
    }
    // tokenise the run forwards: a number is digits with an optional fraction; a name starts with a letter or `_`
    let mut starts = Vec::new();
    let mut j = run;
    while j < k {
        let c = ch(j).unwrap_or(' ');
        starts.push(j);
        if c.is_ascii_digit() {
            while j < k && ch(j).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
            }
            if ch(j) == Some('.') && ch(j + 1).is_some_and(|c| c.is_ascii_digit()) {
                j += 1;
                while j < k && ch(j).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
            }
        } else if is_word(c) {
            while j < k && ch(j).is_some_and(is_word) {
                j += 1;
            }
            while j < k && ch(j) == Some('\'') {
                j += 1;
            }
        } else {
            j += 1;
        }
    }
    starts.last().copied().unwrap_or(k)
}

/// The opening bracket matching the closing one at `close` (boxes count as single items).
pub fn matching_open(row: &Row, close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for j in (0..close.min(row.len())).rev() {
        match row.0[j] {
            Item::Char(c) if is_close(c) => depth += 1,
            Item::Char(c) if is_open(c) => {
                if depth == 0 {
                    return Some(j);
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    None
}

/// Whether the base of a power is drawn in parentheses: it is not one operand. A name, a number, a call or a
/// bracketed group, a string, a radical or a base with an index stand as they are; `2m`, `-x`, `a + b`, a fraction
/// or another power would read as something else next to the exponent (`2m²` is `2(m²)`), so the layout puts the
/// parentheses the linear text has (round 33).
pub fn compound_base(base: &Row) -> bool {
    use crate::tree::Node;
    let n = base.len();
    match base.0.as_slice() {
        [] | [Item::Placeholder] | [Item::Char(_)] => false,
        [Item::Box(b)] => matches!(**b, Node::Fraction { .. } | Node::Superscript { .. } | Node::BigOp { .. }),
        // a number before a bracket is a product (`2(x)`), not a call
        [Item::Char(d), .., Item::Char(c)] if d.is_ascii_digit() && is_close(*c) => true,
        _ => primary_before(base, n) != Some(0..n),
    }
}

/// Binary operators drawn with thin spaces around them (unless unary).
pub fn is_binary(c: char) -> bool {
    matches!(c, '+' | '-' | '−' | '±' | '∓' | '×' | '·' | '∧' | '∨' | '∪' | '∩')
}

/// Relations drawn with thin spaces around them.
pub fn is_relation(c: char) -> bool {
    matches!(c, '=' | '<' | '>' | '≤' | '≥' | '≠' | '⩵' | '→' | '⧴' | '↔' | '⇒' | '∈' | '∉' | '⊂' | '⊃' | '⊆' | '⊇' | '≡' | '≈' | '∝')
}
