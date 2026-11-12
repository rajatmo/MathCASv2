//! Positions in a cell's text (byte offsets on character boundaries): character and word steps, lines. Every
//! function takes any position: one past the text or inside a character is first moved back to a boundary, so a
//! stale position never panics (round 11 robustness).

/// `pos` moved back onto a character boundary of `s` (and to at most its length).
pub fn floor(s: &str, pos: usize) -> usize {
    let mut p = pos.min(s.len());
    while !s.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// The position of the character before `pos`.
pub fn prev_char(s: &str, pos: usize) -> usize {
    let pos = floor(s, pos);
    s[..pos].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// The position after the character at `pos`.
pub fn next_char(s: &str, pos: usize) -> usize {
    let pos = floor(s, pos);
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
    let mut chars = s[..floor(s, pos)].chars();
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
    let mut i = floor(s, pos);
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

/// The end of the word after `pos`, then the white space after it (the caret moves to the next word start).
pub fn word_right(s: &str, pos: usize) -> usize {
    let pos = floor(s, pos);
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
    let pos = floor(s, pos);
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
    let pos = floor(s, pos);
    s[..pos].rfind('\n').map_or(0, |i| i + 1)
}

/// The end of the line containing `pos`.
pub fn line_end(s: &str, pos: usize) -> usize {
    let pos = floor(s, pos);
    s[pos..].find('\n').map_or(s.len(), |i| pos + i)
}

/// Text with every line break as `\n` (`\r\n` and a lone `\r` from other programs).
pub fn unix_lines(s: &str) -> String {
    if s.contains('\r') {
        s.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        s.to_string()
    }
}

/// Text with the tabs at the start of each line replaced by `indent` (tabs elsewhere, in strings say, stay).
pub fn untab(s: &str, indent: &str) -> String {
    if !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for (k, line) in s.split('\n').enumerate() {
        if k > 0 {
            out.push('\n');
        }
        let lead = line.len() - line.trim_start_matches([' ', '\t']).len();
        for ch in line[..lead].chars() {
            if ch == '\t' {
                out.push_str(indent);
            } else {
                out.push(ch);
            }
        }
        out.push_str(&line[lead..]);
    }
    out
}

/// Source text cut into blocks at its blank lines (lines of white space) that are outside brackets, strings and
/// comments; each block without the blank lines around it. Text without such a line is one block.
pub fn code_blocks(s: &str) -> Vec<String> {
    let mut blocks = vec![];
    let mut cur: Vec<&str> = vec![];
    let mut depth: i64 = 0;
    let mut in_str = false;
    for line in s.split('\n') {
        if line.trim().is_empty() && depth <= 0 && !in_str {
            if !cur.is_empty() {
                blocks.push(cur.join("\n"));
                cur.clear();
            }
            continue;
        }
        cur.push(line);
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match (in_str, c) {
                (true, '\\') => {
                    chars.next();
                }
                (true, '"') | (false, '"') => in_str = !in_str,
                (false, '#') => break,
                (false, '(' | '[' | '{') => depth += 1,
                (false, ')' | ']' | '}') => depth -= 1,
                _ => {}
            }
        }
    }
    if !cur.is_empty() {
        blocks.push(cur.join("\n"));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_text() {
        assert_eq!(unix_lines("a\r\nb\rc\n"), "a\nb\nc\n");
        assert_eq!(untab("\tf(x)\n\t\tg\t1\n  \th", "    "), "    f(x)\n        g\t1\n      h");
        assert_eq!(code_blocks("a = 1\n\nb = 2\n"), ["a = 1", "b = 2"]);
        assert_eq!(code_blocks("\n\nf(x) := {\n\n    x\n}\n\n\ng(1)"), ["f(x) := {\n\n    x\n}", "g(1)"]);
        assert_eq!(code_blocks("s = \"a\n\nb\"\nt"), ["s = \"a\n\nb\"\nt"], "a blank line in a string");
        assert_eq!(code_blocks("x # (\n\ny"), ["x # (", "y"], "brackets in comments do not count");
        assert_eq!(code_blocks("one\ntwo"), ["one\ntwo"]);
    }

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
