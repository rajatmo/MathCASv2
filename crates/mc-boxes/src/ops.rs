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

/// Binary operators drawn with thin spaces around them (unless unary).
pub fn is_binary(c: char) -> bool {
    matches!(c, '+' | '-' | '±' | '∓' | '×' | '·' | '∧' | '∨' | '∪' | '∩')
}

/// Relations drawn with thin spaces around them.
pub fn is_relation(c: char) -> bool {
    matches!(c, '=' | '<' | '>' | '≤' | '≥' | '≠' | '⩵' | '→' | '⧴' | '↔' | '⇒' | '∈' | '∉' | '⊂' | '⊃' | '⊆' | '⊇' | '≡' | '≈' | '∝')
}
