//! A light scan of source text for the canonical box text: which characters are inside literals (strings,
//! comments, backquoted names) and which brackets match. This is all the reader needs to find box shapes, and it
//! never fails: unbalanced brackets simply have no match.

/// The scan of one text, by character index.
pub(crate) struct Scan {
    pub chars: Vec<char>,
    /// for an opening bracket outside literals: the index of its matching closing bracket
    pub close: Vec<Option<usize>>,
    /// whether the character is part of a string, comment or backquoted name
    pub literal: Vec<bool>,
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
        let mut i = 0;
        while i < n {
            let c = chars[i];
            match c {
                '"' | '`' => {
                    // a string (with backslash escapes) or a backquoted name, to its closing quote or the end
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
                    }
                }
                '#' => {
                    while i < n && chars[i] != '\n' {
                        literal[i] = true;
                        i += 1;
                    }
                    continue;
                }
                '(' | '[' | '{' => stack.push((i, c)),
                ')' | ']' | '}' => {
                    // a closer matches the innermost opener of its kind only if that opener is on top; otherwise it
                    // is stray and left unmatched
                    if let Some(&(j, o)) = stack.last() {
                        if closer(o) == Some(c) {
                            stack.pop();
                            close[j] = Some(i);
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
        Scan { chars, close, literal }
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
    }
}
