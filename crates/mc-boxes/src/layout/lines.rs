//! Choosing line breaks in a row (design 08, section 4.8): lowest bracket depth first, then after `,` and `;`,
//! then before relations, then before `+`/`-`, then before `*`/`/`, then after spaces; never inside a token or a
//! box. Greedy, one line at a time.

/// What the breaker needs to know about one piece of a row.
#[derive(Clone, Copy, Debug)]
pub(super) struct PieceInfo {
    /// width including the space before it
    pub width: f32,
    /// the space before it (dropped at the start of a line)
    pub space_before: f32,
    /// the piece is a forced line break (a new line character); it ends its line
    pub newline: bool,
    /// how good a break before this piece is (lower is better), `None` where no break may go
    pub class: Option<u8>,
    /// bracket depth before the piece
    pub depth: usize,
}

/// Breaks before a piece whose own class says so: after `,`/`;` (0), before relations (1), before `+ -` (2),
/// before `* /` (3), after a space (4), anywhere else between tokens (5).
pub(super) const CLASS_ANY: u8 = 5;

/// Where a line may only break when nothing better fits: a candidate must start beyond this share of the line.
const MIN_FILL: f32 = 0.4;

/// One line: pieces `start..end`, indented by `indent` ems.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct LineSpan {
    pub start: usize,
    pub end: usize,
    pub indent: f32,
}

/// The lines of a row of `pieces` at width `max` (ems); `indent_per_level` ems per bracket level for
/// continuation lines (0 for none).
pub(super) fn break_lines(pieces: &[PieceInfo], max: Option<f32>, indent_per_level: f32) -> Vec<LineSpan> {
    let n = pieces.len();
    let mut lines = Vec::new();
    let mut start = 0;
    let mut indent = 0.0;
    loop {
        let avail = max.map(|m| (m - indent).max(m * 0.5));
        // the furthest end that fits (at least one piece), stopping at a forced break
        let mut x = 0.0;
        let mut end = start;
        let mut forced = None;
        while end < n {
            let p = pieces[end];
            let w = if end == start { p.width - p.space_before } else { p.width };
            if end > start && avail.is_some_and(|a| x + w > a) {
                break;
            }
            x += w;
            end += 1;
            if p.newline {
                forced = Some(end);
                break;
            }
        }
        let stop = if let Some(f) = forced {
            f
        } else if end >= n {
            n
        } else {
            choose(pieces, start, end, avail.unwrap_or(f32::INFINITY))
        };
        let next_indent = if stop < n { indent_per_level * pieces[stop].depth as f32 } else { 0.0 };
        lines.push(LineSpan { start, end: stop, indent });
        if stop >= n {
            // a final new line leaves an empty last line
            if pieces.last().is_some_and(|p| p.newline) && stop == n {
                lines.push(LineSpan { start: n, end: n, indent: next_indent });
            }
            return lines;
        }
        start = stop;
        indent = next_indent.min(max.unwrap_or(f32::INFINITY) * 0.5);
    }
}

/// The best break before a piece in `start + 1 ..= end` (a line of `start..fit_end` fits); if no break is allowed
/// there, the first allowed break after `end` (the line overflows), or the end of the row.
fn choose(pieces: &[PieceInfo], start: usize, end: usize, avail: f32) -> usize {
    let mut x = 0.0;
    let mut best: Option<(usize, usize, u8, usize)> = None; // (k, depth, class, filled)
    for k in start + 1..=end {
        x += pieces[k - 1].width;
        if k >= pieces.len() {
            break;
        }
        let Some(class) = pieces[k].class else { continue };
        let filled = usize::from(x >= MIN_FILL * avail);
        let key = (1 - filled, pieces[k].depth, class);
        let better = match best {
            None => true,
            Some((_, d, c, f)) => key <= (1 - f, d, c),
        };
        if better {
            best = Some((k, pieces[k].depth, class, filled));
        }
    }
    match best {
        Some((k, ..)) => k,
        None => (end + 1..pieces.len()).find(|&k| pieces[k].class.is_some()).unwrap_or(pieces.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(width: f32, class: Option<u8>, depth: usize) -> PieceInfo {
        PieceInfo { width, space_before: 0.0, newline: false, class, depth }
    }

    #[test]
    fn breaks_at_the_lowest_depth_then_the_best_class() {
        // a + f(b, c) + d with unit widths: "a" "+" "f" "(" "b" "," "c" ")" "+" "d"
        let row = [
            p(1.0, None, 0),
            p(1.0, Some(2), 0),
            p(1.0, Some(5), 0),
            p(1.0, None, 0),
            p(1.0, Some(5), 1),
            p(1.0, None, 1),
            p(1.0, Some(0), 1),
            p(1.0, None, 1),
            p(1.0, Some(2), 0),
            p(1.0, Some(5), 0),
        ];
        let lines = break_lines(&row, Some(8.5), 1.0);
        assert_eq!(lines[0], LineSpan { start: 0, end: 8, indent: 0.0 });
        assert_eq!(lines[1], LineSpan { start: 8, end: 10, indent: 0.0 });
        // narrower: the depth-0 breaks would leave a nearly empty line, so the break goes after the comma
        let narrow = break_lines(&row, Some(6.5), 1.0);
        assert_eq!(narrow[0].end, 6, "{narrow:?}");
        assert_eq!(narrow[1].indent, 1.0);
        assert!(break_lines(&row, None, 1.0).len() == 1);
    }

    #[test]
    fn forced_breaks_and_overflow() {
        let mut nl = p(0.0, None, 1);
        nl.newline = true;
        let row = [p(1.0, None, 0), nl, p(1.0, Some(5), 1), p(10.0, None, 1), p(1.0, Some(5), 1)];
        let lines = break_lines(&row, Some(5.0), 2.0);
        assert_eq!(lines[0], LineSpan { start: 0, end: 2, indent: 0.0 });
        assert_eq!(lines[1].start, 2);
        assert_eq!(lines[1].indent, 2.0);
        // the wide piece cannot be split: its line overflows and breaks after it
        assert_eq!(lines[1].end, 4);
        assert_eq!(lines.last().unwrap().end, 5);
    }
}
