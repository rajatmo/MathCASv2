//! Scanning source text for editor help: the name at or before a position, the call a position is inside, and
//! the parameters of a usage line. The scanner knows enough of the language to skip strings (`"..."` with
//! backslash escapes), backquoted names and comments (`#` to the end of the line); it never fails.
use std::ops::Range;

/// Whether `c` may be part of a name.
pub fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether `s` is a name: a letter or `_` first, then letters, digits and `_`.
pub fn is_name(s: &str) -> bool {
    let mut it = s.chars();
    it.next().is_some_and(|c| c.is_alphabetic() || c == '_') && it.all(is_name_char)
}

/// The name around byte `pos` of `text` (the caret inside it or at either end of it).
pub fn word_at(text: &str, pos: usize) -> Option<Range<usize>> {
    let pos = floor(text, pos);
    let start = text[..pos].char_indices().rev().take_while(|&(_, c)| is_name_char(c)).last().map_or(pos, |(i, _)| i);
    let end = text[pos..].char_indices().find(|&(_, c)| !is_name_char(c)).map_or(text.len(), |(i, _)| pos + i);
    is_name(&text[start..end]).then_some(start..end)
}

/// The name ending at byte `pos` (a name being typed before the caret).
pub fn word_before(text: &str, pos: usize) -> Option<Range<usize>> {
    let pos = floor(text, pos);
    let start = text[..pos].char_indices().rev().take_while(|&(_, c)| is_name_char(c)).last().map_or(pos, |(i, _)| i);
    is_name(&text[start..pos]).then_some(start..pos)
}

/// `pos` moved back to a character boundary of `text` (and into it).
fn floor(text: &str, pos: usize) -> usize {
    let mut p = pos.min(text.len());
    while !text.is_char_boundary(p) {
        p -= 1;
    }
    p
}

/// A call around a position: `f(a, b|, c)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// the function's name as written (`f`, or `M.f` qualified)
    pub name: String,
    /// where the name is
    pub name_range: Range<usize>,
    /// the byte of the opening bracket
    pub open: usize,
    /// the argument the position is in (0-based: the commas before it at the call's own level)
    pub arg: usize,
}

/// The innermost call `name(...)` whose brackets hold byte `pos` (the closing bracket may be missing: a call
/// being typed). Lists, blocks and grouping brackets inside the call are part of an argument; a position inside a
/// string or a comment is still inside the call around them.
pub fn call_at(text: &str, pos: usize) -> Option<Call> {
    calls_at(text, pos).into_iter().next()
}

/// Every call whose brackets hold byte `pos`, innermost first (see [`call_at`]).
pub fn calls_at(text: &str, pos: usize) -> Vec<Call> {
    let pos = floor(text, pos);
    // open brackets: (byte, bracket, commas at its level)
    let mut stack: Vec<(usize, u8, usize)> = vec![];
    let b = text.as_bytes();
    let mut i = 0;
    while i < pos {
        match b[i] {
            b'"' | b'`' => {
                let q = b[i];
                i += 1;
                while i < pos && b[i] != q {
                    if b[i] == b'\\' && q == b'"' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'#' => {
                while i < pos && b[i] != b'\n' {
                    i += 1;
                }
            }
            c @ (b'(' | b'[' | b'{') => stack.push((i, c, 0)),
            b')' | b']' | b'}' => {
                stack.pop();
            }
            b',' => {
                if let Some(top) = stack.last_mut() {
                    top.2 += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    let mut out = vec![];
    for &(open, bracket, commas) in stack.iter().rev() {
        if bracket != b'(' {
            continue;
        }
        let before = &text[..open];
        let start = before
            .char_indices()
            .rev()
            .take_while(|&(_, c)| is_name_char(c) || c == '.')
            .last()
            .map_or(open, |(i, _)| i);
        let name = &text[start..open];
        let simple = name.rsplit('.').next().unwrap_or_default();
        if !name.is_empty() && !name.starts_with('.') && is_name(simple) {
            out.push(Call { name: name.to_string(), name_range: start..open, open, arg: commas });
        }
    }
    out
}

/// The byte of the bracket closing the one at byte `open` of `s` (any of `( [ {`), skipping strings.
pub fn matching_close(s: &str, open: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// The parameters of a usage call `f(a, [x, lo, hi], opts...)`: the byte range of each, trimmed (empty for a
/// call without brackets or arguments).
pub fn params(call: &str) -> Vec<Range<usize>> {
    let Some(open) = call.find('(') else { return vec![] };
    let close = matching_close(call, open).unwrap_or(call.len());
    let mut out = vec![];
    let mut depth = 0usize;
    let mut start = open + 1;
    let b = call.as_bytes();
    let push = |a: usize, z: usize, out: &mut Vec<Range<usize>>| {
        let s = &call[a..z];
        let lead = s.len() - s.trim_start().len();
        let trail = s.len() - s.trim_end().len();
        if a + lead < z - trail {
            out.push(a + lead..z - trail);
        }
    };
    for (i, &c) in b.iter().enumerate().take(close).skip(open + 1) {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                push(start, i, &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push(start, close, &mut out);
    out
}

/// The parameter of `call` that argument `arg` fills: that parameter, or a last parameter with `...` (it takes
/// the rest); `None` past the end.
pub fn highlight(call: &str, arg: usize) -> Option<Range<usize>> {
    let ps = params(call);
    match ps.get(arg) {
        Some(r) => Some(r.clone()),
        None => ps.last().filter(|r| call[(*r).clone()].contains("...")).cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words() {
        let t = "y = n_integrate(sin(x), [x, 0, 1])";
        assert_eq!(word_at(t, 6).map(|r| &t[r]), Some("n_integrate"));
        assert_eq!(word_at(t, 4).map(|r| &t[r]), Some("n_integrate"));
        assert_eq!(word_at(t, 15).map(|r| &t[r]), Some("n_integrate"));
        assert_eq!(word_at(t, 2), None);
        assert_eq!(word_at(t, 31), None, "a number is not a name");
        assert_eq!(word_before(t, 9).map(|r| &t[r]), Some("n_int"));
        assert_eq!(word_before(t, 3), None);
        assert_eq!(word_at("αβ", 2).map(|r| &"αβ"[r]), Some("αβ"));
        assert_eq!(word_at("αβ", 1).map(|r| &"αβ"[r]), Some("αβ"), "inside a character");
        assert_eq!(word_at("", 0), None);
    }

    fn call(t: &str) -> Option<(String, usize)> {
        let pos = t.find('|').expect("a caret mark");
        let text = t.replace('|', "");
        call_at(&text, pos).map(|c| (c.name, c.arg))
    }

    #[test]
    fn calls() {
        assert_eq!(call("f(|"), Some(("f".into(), 0)));
        assert_eq!(call("f(a, |b)"), Some(("f".into(), 1)));
        assert_eq!(call("n_integrate(f, [x, |0, 1])"), Some(("n_integrate".into(), 1)));
        assert_eq!(call("f(g(x), |)"), Some(("f".into(), 1)));
        assert_eq!(call("f(g(x|), y)"), Some(("g".into(), 0)));
        assert_eq!(call("f((a + b), |)"), Some(("f".into(), 1)));
        assert_eq!(call("f(((a)/(b)), (|"), Some(("f".into(), 1)), "a fraction box's text inside a call");
        assert_eq!(call("f(\"a, (b\", |"), Some(("f".into(), 1)), "brackets and commas in strings are text");
        assert_eq!(call("f(a # (, \n, |"), Some(("f".into(), 1)), "and in comments");
        assert_eq!(call("M.f(x, |"), Some(("M.f".into(), 1)));
        assert_eq!(call("f(x)|"), None);
        assert_eq!(call("(a, |"), None);
        assert_eq!(call("2(a|"), None);
        assert_eq!(call("[f, |]"), None);
        assert_eq!(call("x = 1\ng(|"), Some(("g".into(), 0)));
        let names: Vec<String> = calls_at("f(a, g(b, h(|", 12).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["h", "g", "f"]);
    }

    #[test]
    fn parameters() {
        let c = "n_integrate(f, [x, a, b], opts...)";
        let ps: Vec<&str> = params(c).into_iter().map(|r| &c[r]).collect();
        assert_eq!(ps, ["f", "[x, a, b]", "opts..."]);
        assert!(params("pi").is_empty());
        assert!(params("f()").is_empty());
        assert_eq!(highlight(c, 1).map(|r| &c[r]), Some("[x, a, b]"));
        assert_eq!(highlight(c, 5).map(|r| &c[r]), Some("opts..."));
        assert_eq!(highlight("f(a, b)", 2), None);
        assert_eq!(matching_close("(a, \")\", [b])x", 0), Some(12));
    }
}
