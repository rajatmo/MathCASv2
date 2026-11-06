//! Positions in a cell's text (byte offsets on character boundaries): character and word steps, lines.

/// The position of the character before `pos`.
pub fn prev_char(s: &str, pos: usize) -> usize {
    s[..pos].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// The position after the character at `pos`.
pub fn next_char(s: &str, pos: usize) -> usize {
    s[pos..].chars().next().map_or(pos, |c| pos + c.len_utf8())
}

/// Where a position of source text is, for automatic closing of brackets and quotes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lexical {
    /// in code
    Code,
    /// inside a string literal
    Str,
    /// inside a `#` comment
    Comment,
}

/// What `pos` of source text `s` is inside: a string (`"..."` with `\` escapes, possibly over lines), a comment
/// (`#` to the line end) or code. Mirrors the lexer of mc-syntax for these two tokens.
pub fn lexical_at(s: &str, pos: usize) -> Lexical {
    let mut state = Lexical::Code;
    let mut chars = s[..pos.min(s.len())].chars();
    while let Some(c) = chars.next() {
        state = match (state, c) {
            (Lexical::Code, '"') => Lexical::Str,
            (Lexical::Code, '#') => Lexical::Comment,
            (Lexical::Str, '\\') => {
                chars.next();
                Lexical::Str
            }
            (Lexical::Str, '"') | (Lexical::Comment, '\n') => Lexical::Code,
            (st, _) => st,
        };
    }
    state
}

/// The class of a character for "natural word" motion: identifiers and numbers are words, runs of operator
/// characters are words, white space separates.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Word,
    Punct,
}

fn class(c: char) -> Class {
    if c.is_whitespace() {
        Class::Space
    } else if c.is_alphanumeric() || c == '_' {
        Class::Word
    } else {
        Class::Punct
    }
}

/// The start of the word before `pos` (white space before the caret is skipped first).
pub fn word_left(s: &str, pos: usize) -> usize {
    let mut i = pos;
    while i > 0 && class(s[..i].chars().next_back().expect("non-empty")) == Class::Space {
        i = prev_char(s, i);
    }
    if i == 0 {
        return 0;
    }
    let k = class(s[..i].chars().next_back().expect("non-empty"));
    while i > 0 && class(s[..i].chars().next_back().expect("non-empty")) == k {
        i = prev_char(s, i);
    }
    i
}

/// The end of the word after `pos`, then the white space after it (as the reference moves to the next word start).
pub fn word_right(s: &str, pos: usize) -> usize {
    let mut i = pos;
    if let Some(c) = s[i..].chars().next() {
        let k = class(c);
        if k != Class::Space {
            while s[i..].chars().next().is_some_and(|c| class(c) == k) {
                i = next_char(s, i);
            }
        }
    }
    while s[i..].chars().next().is_some_and(|c| class(c) == Class::Space && c != '\n') {
        i = next_char(s, i);
    }
    if i == pos && i < s.len() {
        i = next_char(s, i);
    }
    i
}

/// The word around `pos` (for double-click): the run of same-class characters touching it.
pub fn word_at(s: &str, pos: usize) -> (usize, usize) {
    let after = s[pos..].chars().next();
    let before = s[..pos].chars().next_back();
    let k = match (before, after) {
        (_, Some(c)) if class(c) == Class::Word => Class::Word,
        (Some(c), _) if class(c) == Class::Word => Class::Word,
        (_, Some(c)) => class(c),
        (Some(c), None) => class(c),
        (None, None) => return (pos, pos),
    };
    let mut a = pos;
    while a > 0 && s[..a].chars().next_back().is_some_and(|c| class(c) == k && c != '\n') {
        a = prev_char(s, a);
    }
    let mut b = pos;
    while s[b..].chars().next().is_some_and(|c| class(c) == k && c != '\n') {
        b = next_char(s, b);
    }
    (a, b)
}

/// The byte ranges of the lines of `s` (split at `\n`, the line end not included).
pub fn lines(s: &str) -> Vec<std::ops::Range<usize>> {
    let mut out = vec![];
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '\n' {
            out.push(start..i);
            start = i + 1;
        }
    }
    out.push(start..s.len());
    out
}

/// The start of the line containing `pos`.
pub fn line_start(s: &str, pos: usize) -> usize {
    s[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// The end of the line containing `pos`.
pub fn line_end(s: &str, pos: usize) -> usize {
    s[pos..].find('\n').map_or(s.len(), |i| pos + i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        let s = "f(aa + b*c, d)";
        assert_eq!(word_right(s, 0), 1);
        assert_eq!(word_right(s, 1), 2);
        assert_eq!(word_right(s, 2), 5);
        assert_eq!(word_right(s, 5), 7);
        assert_eq!(word_left(s, 7), 5);
        assert_eq!(word_left(s, 5), 2);
        assert_eq!(word_left(s, 0), 0);
        assert_eq!(word_at(s, 3), (2, 4));
        assert_eq!(word_at(s, 4), (2, 4));
        assert_eq!(word_at("x  y", 2), (1, 3));
        assert_eq!(word_right("ab", 2), 2);
        assert_eq!(word_left("é é", 5), 3);
    }

    #[test]
    fn line_ranges() {
        assert_eq!(lines("a\nbc\n"), [0..1, 2..4, 5..5]);
        assert_eq!(line_start("a\nbc", 3), 2);
        assert_eq!(line_end("a\nbc", 0), 1);
        assert_eq!(prev_char("aé", 3), 1);
        assert_eq!(next_char("aé", 1), 3);
    }
}
