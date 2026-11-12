//! Printed outputs (design 4.8, 4.9): line breaking of linear output text at the cell's width, and the sample shown
//! for an output too large to show in full.
//!
//! Both work on the printed (linear) text of a value and count characters, as outputs are set in a monospaced font.
//! Neither recurses on the text's nesting beyond a fixed depth, so any text is safe.
use std::ops::Range;

/// Outputs whose text has more characters than this are shown as a sample (design 4.9). The design's budget is 1 MiB;
/// the notebook draws text through egui, whose layout of a single text grows with every
/// character, so the budget is lower and a full view is drawn in pieces (decision NBM.4).
pub const LARGE_OUTPUT: usize = 100_000;

/// The size of the first sample of a large output, in characters (design 4.9: about 2,000).
pub const SAMPLE: usize = 2_000;

/// Deeper nesting than this is not looked into when sampling: it is elided as a whole.
const MAX_DEPTH: usize = 64;

/// Penalties of break candidates: a lower penalty is a better place to break (design 4.8).
fn penalty(depth: usize, before: char, after: char) -> u32 {
    let kind = match (before, after) {
        (',' | ';', _) => 0,
        (_, '+' | '-') => 2,
        (_, '>' | '<' | '=' | '!' | '&' | '|') => 3,
        (_, '*' | '/' | '·' | '×') => 4,
        (_, '^') => 6,
        _ => 5,
    };
    depth as u32 * 10 + kind
}

/// The lines of `text` broken to at most `cols` characters each (design 4.8): byte ranges of `text`, in order. A
/// line ends at a line end of the text, or at a space chosen by the lowest penalty among the spaces in the later
/// half of the line: bracket depth first, then after `,` and `;`, then before `+ -`, comparison and logic
/// operators, then `* /`, then `^` (the space itself belongs to no line). Spaces inside string literals are not
/// candidates. A line with no candidate is cut at `cols` characters. Continuation lines start at the left edge.
pub fn wrap(text: &str, cols: usize) -> Vec<Range<usize>> {
    let cols = cols.max(8);
    let mut out = Vec::new();
    // the line being filled: its start (bytes) and the characters since it
    let mut start = 0;
    let mut chars = 0usize;
    // break candidates of the line: (byte of the space, characters before it in the line, penalty)
    let mut cands: Vec<(usize, usize, u32)> = Vec::new();
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    let mut prev = '\0';
    let mut iter = text.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c == '\n' {
            out.push(start..i);
            start = i + 1;
            chars = 0;
            cands.clear();
            prev = c;
            continue;
        }
        if chars == cols {
            // the line is full: break at the best candidate of its later half, else here
            let half = cols / 2;
            let best = cands
                .iter()
                .filter(|&&(_, at, _)| at >= half)
                .min_by(|a, b| a.2.cmp(&b.2).then(b.1.cmp(&a.1)))
                .or_else(|| cands.iter().max_by_key(|&&(_, at, _)| at))
                .copied();
            match best {
                Some((at, before, _)) if c != ' ' || at != i => {
                    out.push(start..at);
                    start = at + 1;
                    chars -= before + 1;
                    cands.retain(|&(b, _, _)| b > at);
                    for cand in &mut cands {
                        cand.1 -= before + 1;
                    }
                }
                _ => {
                    out.push(start..i);
                    start = i;
                    chars = 0;
                    cands.clear();
                }
            }
        }
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else {
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth = depth.saturating_sub(1),
                ' ' if i > start => {
                    let after = iter.peek().map_or(' ', |&(_, n)| n);
                    if after != ' ' {
                        cands.push((i, chars, penalty(depth, prev, after)));
                    }
                }
                _ => {}
            }
        }
        chars += 1;
        prev = c;
    }
    out.push(start..text.len());
    out
}

/// The text of `text` with the line breaks of [`wrap`] that fall on spaces made line ends: the same length in
/// bytes and characters as `text`, so positions in it are positions in `text`. Breaks with no space (a token longer
/// than a line) are left to the painter.
pub fn wrapped(text: &str, cols: usize) -> String {
    let mut bytes = text.as_bytes().to_vec();
    for r in wrap(text, cols) {
        if bytes.get(r.end) == Some(&b' ') {
            bytes[r.end] = b'\n';
        }
    }
    String::from_utf8(bytes).expect("a space replaced by a line end")
}

/// Whether an output's text is too large to show in full by default.
pub fn is_large(text: &str) -> bool {
    text.len() > LARGE_OUTPUT && text.chars().count() > LARGE_OUTPUT
}

/// A sample of `text` of about `budget` characters (design 4.9): the middle of long lists, argument sequences and
/// atoms (numbers, names, strings) is replaced by `«n»`, `n` the number of elements (or characters) left out; the
/// brackets stay balanced. A text within the budget is returned whole.
pub fn sample(text: &str, budget: usize) -> String {
    let budget = budget.max(16);
    if text.chars().count() <= budget {
        return text.to_string();
    }
    let mut out = String::new();
    sample_seq(text, budget, 0, &mut out);
    out
}

/// The parts of a sequence: runs of text and bracketed groups (open, inner range, close), at top level.
enum Part {
    Text(Range<usize>),
    Group(char, Range<usize>, Option<char>),
}

fn parts(s: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut text_from = 0;
    let mut group: Option<(char, usize)> = None;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => {
                if depth == 0 {
                    if text_from < i {
                        out.push(Part::Text(text_from..i));
                    }
                    group = Some((c, i + 1));
                }
                depth += 1;
            }
            ')' | ']' | '}' if depth > 0 => {
                depth -= 1;
                if depth == 0 {
                    let (open, from) = group.take().expect("a group is open");
                    out.push(Part::Group(open, from..i, Some(c)));
                    text_from = i + 1;
                }
            }
            _ => {}
        }
    }
    match group {
        Some((open, from)) => out.push(Part::Group(open, from..s.len(), None)),
        None if text_from < s.len() => out.push(Part::Text(text_from..s.len())),
        None => {}
    }
    out
}

/// The top-level elements of a group's inner text, split at its commas (each without the space after the comma).
fn elements(s: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut from = 0;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if in_string {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(from..i);
                from = i + 1;
                if s[from..].starts_with(' ') {
                    from += 1;
                }
            }
            _ => {}
        }
    }
    if from < s.len() || !out.is_empty() {
        out.push(from..s.len());
    }
    out
}

/// Append a sample of the sequence `s` (text and groups) in about `budget` characters.
fn sample_seq(s: &str, budget: usize, depth: usize, out: &mut String) {
    let len = s.chars().count();
    if len <= budget {
        out.push_str(s);
        return;
    }
    let ps = parts(s);
    // each part gets a share of the budget by its length, at least a few characters; parts past the budget go
    let start = out.chars().count();
    for (k, p) in ps.iter().enumerate() {
        if out.chars().count() - start > budget {
            out.push_str(&format!("«{}»", ps.len() - k));
            break;
        }
        let (r, group) = match p {
            Part::Text(r) => (r.clone(), None),
            Part::Group(open, r, close) => (r.clone(), Some((*open, *close))),
        };
        let part_len = s[r.clone()].chars().count();
        let share = ((budget as f64) * part_len as f64 / len as f64).ceil() as usize;
        let share = share.max(12);
        match group {
            None => sample_atom(&s[r], share, out),
            Some((open, close)) => {
                out.push(open);
                if depth >= MAX_DEPTH {
                    let n = elements(&s[r]).len();
                    out.push_str(&format!("«{n}»"));
                } else {
                    sample_group(&s[r], share, depth + 1, out);
                }
                if let Some(c) = close {
                    out.push(c);
                }
            }
        }
    }
}

/// Append the elements of a group's inner text: the first and last ones that fit `budget` (each sampled to a
/// fraction of it), the elements between them as `«n»`.
fn sample_group(s: &str, budget: usize, depth: usize, out: &mut String) {
    if s.chars().count() <= budget {
        out.push_str(s);
        return;
    }
    let els = elements(s);
    if els.len() <= 1 {
        sample_seq(s, budget, depth, out);
        return;
    }
    let cap = (budget / 4).max(12);
    let render = |r: &Range<usize>| {
        let mut t = String::new();
        sample_seq(&s[r.clone()], cap, depth, &mut t);
        t
    };
    let (mut front, mut back): (Vec<String>, Vec<String>) = (vec![], vec![]);
    let (mut i, mut j) = (0, els.len());
    let mut used = 0;
    while i < j {
        let take_front = front.len() <= back.len();
        let r = if take_front { &els[i] } else { &els[j - 1] };
        let t = render(r);
        let n = t.chars().count() + 2;
        if used + n > budget && !front.is_empty() {
            break;
        }
        used += n;
        if take_front {
            front.push(t);
            i += 1;
        } else {
            back.push(t);
            j -= 1;
        }
    }
    back.reverse();
    let mut items = front;
    if i < j {
        items.push(format!("«{}»", j - i));
    }
    items.extend(back);
    out.push_str(&items.join(", "));
}

/// Append a text without groups (an atom or operators between groups), its middle elided when it is longer than
/// `budget` characters.
fn sample_atom(s: &str, budget: usize, out: &mut String) {
    let n = s.chars().count();
    if n <= budget {
        out.push_str(s);
        return;
    }
    let keep = budget / 2;
    let head: String = s.chars().take(keep).collect();
    let tail: String = s.chars().skip(n - keep).collect();
    out.push_str(&format!("{head}«{}»{tail}", n - 2 * keep));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str, cols: usize) -> Vec<&str> {
        wrap(text, cols).into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn short_text_and_line_ends_stay() {
        assert_eq!(lines("x^2 + 1", 40), ["x^2 + 1"]);
        assert_eq!(lines("a\nb", 40), ["a", "b"]);
        assert_eq!(lines("", 40), [""]);
    }

    /// The same list at three widths (design 11, M4 exit: wrap tests at three widths).
    #[test]
    fn a_list_at_three_widths() {
        let t = "[[1, 2, 3], [4, 5, 6], [7, 8, 9], [10, 11, 12]]";
        assert_eq!(lines(t, 30), ["[[1, 2, 3], [4, 5, 6],", "[7, 8, 9], [10, 11, 12]]"]);
        assert_eq!(lines(t, 20), ["[[1, 2, 3],", "[4, 5, 6],", "[7, 8, 9],", "[10, 11, 12]]"]);
        assert_eq!(lines(t, 12), ["[[1, 2, 3],", "[4, 5, 6],", "[7, 8, 9],", "[10, 11,", "12]]"]);
        for cols in [12, 20, 30] {
            assert!(lines(t, cols).iter().all(|l| l.chars().count() <= cols));
        }
    }

    #[test]
    fn sums_break_before_plus_at_the_lowest_depth() {
        let t = "x^10 + 10*x^9*y + 45*x^8*y^2 + 120*x^7*y^3 + f(a + b, c)";
        let ls = lines(t, 24);
        assert_eq!(ls, ["x^10 + 10*x^9*y", "+ 45*x^8*y^2", "+ 120*x^7*y^3", "+ f(a + b, c)"]);
        // inside brackets only when nothing else fits: before `+`, then after `,`
        assert_eq!(lines("f(aaaa + bbbb, cccc) + g", 12), ["f(aaaa", "+ bbbb,", "cccc) + g"]);
    }

    #[test]
    fn long_tokens_are_cut_and_strings_are_kept() {
        let digits = "1234567890".repeat(5);
        let ls = lines(&digits, 20);
        assert_eq!(ls.len(), 3);
        assert_eq!(ls.concat(), digits);
        let t = "[\"a b c d e f g h\", x]";
        // no candidate inside the string: the line is cut at the width
        assert_eq!(lines(t, 12)[0].chars().count(), 12);
    }

    #[test]
    fn wrapped_keeps_positions() {
        let t = "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20]";
        let w = wrapped(t, 20);
        assert_eq!(w.len(), t.len());
        assert!(w.lines().count() > 3);
        assert_eq!(w.replace('\n', " "), t);
    }

    #[test]
    fn samples_elide_the_middle_and_stay_balanced() {
        let list = format!("[{}]", (1..=100_000).map(|k| k.to_string()).collect::<Vec<_>>().join(", "));
        assert!(is_large(&list));
        let s = sample(&list, 200);
        assert!(s.starts_with("[1, 2, 3, ") && s.ends_with(", 99999, 100000]"), "{s}");
        assert!(s.contains('«') && s.chars().count() <= 230, "{s}");
        let n: usize = s.split('«').nth(1).unwrap().split('»').next().unwrap().parse().unwrap();
        let shown = s.matches(", ").count() + 1 - 1;
        assert_eq!(n + shown, 100_000, "{s}");
        // doubling the budget shows more
        assert!(sample(&list, 400).len() > s.len());
        // nested: the inner lists are sampled too
        let nested = format!("[{}]", (0..300).map(|_| list[..2_000].to_string() + "]").collect::<Vec<_>>().join(", "));
        let s = sample(&nested, 500);
        assert_eq!(s.matches('[').count(), s.matches(']').count(), "{s}");
        // a big number keeps its head and tail digits
        let big = "7".repeat(5000);
        let s = sample(&big, 100);
        assert_eq!(s, format!("{}«4900»{}", "7".repeat(50), "7".repeat(50)));
        // within the budget: unchanged
        assert_eq!(sample("[1, 2]", 100), "[1, 2]");
    }

    /// Design 11, M4 exit: a 5 MB output shows a sample, and copying its cell copies all of it.
    #[test]
    fn a_five_megabyte_output_is_sampled_and_copied_whole() {
        use crate::{Bracket, Command, FakeEnv, Notebook};
        let big = format!("[{}]", (0..700_000).map(|k| k.to_string()).collect::<Vec<_>>().join(", "));
        assert!(big.len() > 5_000_000 && is_large(&big));
        let start = std::time::Instant::now();
        let s = sample(&big, SAMPLE);
        assert!(s.chars().count() < SAMPLE + 100 && s.contains('«'));
        let rows = wrap(&big, 100);
        assert!(rows.len() > 50_000 && rows.iter().all(|r| big[r.clone()].chars().count() <= 100));
        assert!(start.elapsed().as_secs_f64() < 5.0, "sampling and wrapping took {:?}", start.elapsed());
        let text = format!("#! mathcas 2 notebook\n\n#% cell label=1\nrange(700000)\n#% output n=1\n#= {big}\n");
        let mut nb = Notebook::parse(&text).unwrap();
        let id = nb.cells()[1].id;
        let mut env = FakeEnv::default();
        nb.apply(Command::ClickBracket(Bracket::Cell(id), false, false), &mut env);
        nb.apply(Command::Copy, &mut env);
        assert!(env.clip.text.as_deref().is_some_and(|t| t.contains(&big)), "the whole output is copied");
    }

    #[test]
    fn deep_and_unbalanced_text_is_safe() {
        let deep = "[".repeat(10_000) + &"]".repeat(10_000);
        let s = sample(&deep, 100);
        assert!(s.contains('«'));
        let open = "f(".repeat(3000) + "x";
        assert!(!sample(&open, 100).is_empty());
        assert!(wrap(&deep, 40).len() > 100);
        assert!(!sample("\"unterminated, string, list", 10).is_empty());
    }
}
