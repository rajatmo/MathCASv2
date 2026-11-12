//! A light scan of source text for the canonical box text: which characters are inside literals (strings,
//! comments, backquoted names) and which brackets match. This is all the reader needs to find box shapes, and it
//! never fails: unbalanced brackets simply have no match.

/// The escape of the canonical text: the character after it is an ordinary character, not a bracket, a quote or a
/// comment sign. The writer puts it before a slot's unmatched brackets, an unterminated string's quote and a `#`
/// without a line end, so that the box shapes around the slot still read back (round 33); the reader drops it, and
/// [`source_text`] gives the text without it. U+2060 (word joiner): invisible, never typed.
pub const ESCAPE: char = '\u{2060}';

/// The scan of one text, by character index.
pub(crate) struct Scan {
    pub chars: Vec<char>,
    /// for an opening bracket outside literals: the index of its matching closing bracket
    pub close: Vec<Option<usize>>,
    /// whether the character is part of a string, comment or backquoted name
    pub literal: Vec<bool>,
    /// what would stop the text from reading back as one slot: unmatched brackets, the opening quote of an
    /// unterminated string or backquoted name, the `#` of a comment that runs to the end of the text
    pub loose: Vec<usize>,
}

fn closer(c: char) -> Option<char> {
    match c {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        _ => None,
    }
}

impl Scan {
    pub fn new(text: &str) -> Scan {
        Scan::of(text.chars().collect())
    }

    pub fn of(chars: Vec<char>) -> Scan {
        let n = chars.len();
        let mut close = vec![None; n];
        let mut literal = vec![false; n];
        let mut stack: Vec<(usize, char)> = Vec::new();
        let mut loose = Vec::new();
        let mut i = 0;
        while i < n {
            let c = chars[i];
            match c {
                ESCAPE if i + 1 < n => {
                    literal[i] = true;
                    literal[i + 1] = true;
                    i += 2;
                    continue;
                }
                '"' | '`' => {
                    // a string (with backslash escapes) or a backquoted name, to its closing quote or the end
                    let open = i;
                    literal[i] = true;
                    i += 1;
                    while i < n && chars[i] != c {
                        literal[i] = true;
                        if c == '"' && chars[i] == '\\' && i + 1 < n {
                            i += 1;
                            literal[i] = true;
                        }
                        i += 1;
                    }
                    if i < n {
                        literal[i] = true;
                    } else {
                        loose.push(open);
                    }
                }
                '#' => {
                    let open = i;
                    while i < n && chars[i] != '\n' {
                        literal[i] = true;
                        i += 1;
                    }
                    if i == n {
                        loose.push(open);
                    }
                    continue;
                }
                '(' | '[' | '{' => stack.push((i, c)),
                ')' | ']' | '}' => {
                    // a closer matches the innermost opener of its kind only if that opener is on top; otherwise it
                    // is stray and left unmatched
                    match stack.last() {
                        Some(&(j, o)) if closer(o) == Some(c) => {
                            stack.pop();
                            close[j] = Some(i);
                        }
                        _ => loose.push(i),
                    }
                }
                _ => {}
            }
            i += 1;
        }
        loose.extend(stack.iter().map(|&(j, _)| j));
        loose.sort_unstable();
        Scan { chars, close, literal, loose }
    }

    /// Whether `chars[i]` is the code character `c` (not inside a literal).
    pub fn is(&self, i: usize, c: char) -> bool {
        i < self.chars.len() && self.chars[i] == c && !self.literal[i]
    }

    /// Whether `lo..hi` is exactly one bracket group opened by `open`.
    pub fn is_group(&self, lo: usize, hi: usize, open: char) -> bool {
        hi > lo + 1 && self.is(lo, open) && self.close[lo] == Some(hi - 1)
    }

    /// Splits `lo..hi` at the top-level occurrences of `sep` (outside brackets and literals); with `first_only`,
    /// at the first one only.
    pub fn split(&self, lo: usize, hi: usize, sep: &str, first_only: bool) -> Vec<(usize, usize)> {
        let sep: Vec<char> = sep.chars().collect();
        let mut parts = Vec::new();
        let mut start = lo;
        let mut i = lo;
        while i < hi {
            if !self.literal[i] {
                if let Some(j) = self.close[i] {
                    if j < hi {
                        i = j + 1;
                        continue;
                    }
                }
                if i + sep.len() <= hi && self.chars[i..i + sep.len()] == sep[..] && (parts.is_empty() || !first_only) {
                    parts.push((start, i));
                    i += sep.len();
                    start = i;
                    continue;
                }
            }
            i += 1;
        }
        parts.push((start, hi));
        parts
    }

    /// Whether `sep` occurs at top level in `lo..hi`.
    pub fn has_top(&self, lo: usize, hi: usize, sep: &str) -> bool {
        self.split(lo, hi, sep, true).len() > 1
    }

    pub fn string(&self, lo: usize, hi: usize) -> String {
        self.chars[lo..hi].iter().collect()
    }
}

/// A slot's text with [`ESCAPE`] before everything that would keep it from reading back as one slot, and the byte
/// offsets in `text` of the characters that got one. Text with balanced brackets and closed literals is unchanged.
pub(crate) fn escape_slot(text: &str) -> (String, Vec<usize>) {
    let mut chars: Vec<char> = text.chars().collect();
    // the byte offset in `text` of each character (`None`: an escape put in here)
    let mut orig: Vec<Option<usize>> = text.char_indices().map(|(b, _)| Some(b)).collect();
    // an escape can uncover more (the brackets of what was a string): until nothing is loose
    loop {
        let scan = Scan::of(chars);
        chars = scan.chars;
        if scan.loose.is_empty() {
            break;
        }
        for &k in scan.loose.iter().rev() {
            chars.insert(k, ESCAPE);
            orig.insert(k, None);
        }
    }
    let at = (0..chars.len()).filter(|&k| orig[k].is_none()).filter_map(|k| orig.get(k + 1).copied().flatten()).collect();
    (chars.into_iter().collect(), at)
}

/// Canonical text as source for the reader of the language: without the escapes.
pub fn source_text(text: &str) -> std::borrow::Cow<'_, str> {
    if text.contains(ESCAPE) {
        std::borrow::Cow::Owned(text.chars().filter(|&c| c != ESCAPE).collect())
    } else {
        std::borrow::Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brackets_match_outside_literals() {
        let s = Scan::new(r#"f("(", [a]) # (x"#);
        assert_eq!(s.close[1], Some(10));
        assert_eq!(s.close[3], None, "a bracket in a string");
        assert_eq!(s.close[7], Some(9));
        assert!(s.literal[13]);
        assert_eq!(s.split(2, 10, ",", false), vec![(2, 5), (6, 10)]);
        assert!(!Scan::new("(a)]").is(3, '('));
        let u = Scan::new("((a)");
        assert_eq!(u.close[0], None);
        assert_eq!(u.close[1], Some(3));
        assert_eq!(u.loose, vec![0]);
    }

    #[test]
    fn loose_brackets_and_literals_are_escaped() {
        let e = |t: &str| escape_slot(t).0.replace(ESCAPE, "~");
        assert_eq!(e("(a + b)"), "(a + b)");
        assert_eq!(e("(a"), "~(a");
        assert_eq!(e("a)"), "a~)");
        assert_eq!(e(")("), "~)~(");
        assert_eq!(e("(]"), "~(~]");
        assert_eq!(e("f(x) + [1, (2"), "f(x) + ~[1, ~(2");
        assert_eq!(e("\"a)"), "~\"a~)");
        assert_eq!(e("a # (b"), "a ~# ~(b");
        assert_eq!(e("a # b\n(c)"), "a # b\n(c)");
        assert_eq!(escape_slot("xy)").1, vec![2]);
        // an escaped text is stable
        let once = escape_slot("((a]").0;
        assert_eq!(escape_slot(&once).0, once);
        assert_eq!(source_text(&once), "((a]");
    }
}
