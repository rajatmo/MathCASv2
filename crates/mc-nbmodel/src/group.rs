//! Cell groups (design 2.3, 2.4): derived from the styles in one pass with a stack, plus groups made by hand, and
//! which cells closed groups hide. The tree is recomputed after every structural change: O(number of cells).
use crate::style::{GroupRule, Style};

/// A group: the cells `start..end` (at least two), headed by `start`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// the first cell (the head)
    pub start: usize,
    /// one past the last cell
    pub end: usize,
    /// the enclosing group (an index into [`Groups::list`])
    pub parent: Option<usize>,
    /// the bracket column: 1 for a group without subgroups, one more than its deepest subgroup otherwise (cell
    /// brackets are column 0), so the outermost group is drawn rightmost
    pub column: usize,
}

/// The group tree of a notebook and the visibility of its cells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    /// the groups, sorted by start, enclosing groups before enclosed ones
    pub list: Vec<Group>,
    /// for each cell, whether it is shown (not hidden in a closed group)
    pub visible: Vec<bool>,
}

/// What grouping needs to know about a cell.
#[derive(Clone, Copy, Debug)]
pub struct CellInfo {
    /// the style
    pub style: Style,
    /// marked `together` (grouped by hand)
    pub together: bool,
    /// the group it heads is closed, showing its k-th cell
    pub closed: Option<usize>,
}

struct Open {
    start: usize,
    rule: GroupRule,
}

/// The groups of the automatic rules (design 2.3).
fn automatic(cells: &[CellInfo]) -> Vec<(usize, usize)> {
    let mut found = vec![];
    let mut stack: Vec<Open> = vec![];
    let mut close_while = |stack: &mut Vec<Open>, at: usize, pred: &dyn Fn(GroupRule) -> bool| {
        while let Some(top) = stack.last() {
            if !pred(top.rule) {
                break;
            }
            let top = stack.pop().expect("non-empty");
            if at - top.start >= 2 {
                found.push((top.start, at));
            }
        }
    };
    for (i, c) in cells.iter().enumerate() {
        let rule = c.style.info().group;
        match rule {
            GroupRule::Title(0) => close_while(&mut stack, i, &|_| true),
            GroupRule::Title(_) => close_while(&mut stack, i, &|r| !matches!(r, GroupRule::Title(_))),
            GroupRule::Section(n) => close_while(&mut stack, i, &|r| match r {
                GroupRule::Title(_) => false,
                GroupRule::Section(m) => m >= n,
                _ => true,
            }),
            GroupRule::Item(n) => close_while(&mut stack, i, &|r| match r {
                GroupRule::Input => true,
                GroupRule::Item(m) => m >= n,
                _ => false,
            }),
            GroupRule::Normal => close_while(&mut stack, i, &|r| matches!(r, GroupRule::Input | GroupRule::Item(_))),
            GroupRule::Input => close_while(&mut stack, i, &|r| r == GroupRule::Input),
            GroupRule::Output => {}
        }
        match rule {
            GroupRule::Title(0) | GroupRule::Section(_) | GroupRule::Item(_) | GroupRule::Input => {
                stack.push(Open { start: i, rule })
            }
            _ => {}
        }
    }
    close_while(&mut stack, cells.len(), &|_| true);
    found
}

/// The runs of consecutive `together` cells.
fn manual(cells: &[CellInfo]) -> Vec<(usize, usize)> {
    let mut found = vec![];
    let mut i = 0;
    while i < cells.len() {
        if cells[i].together {
            let start = i;
            while i < cells.len() && cells[i].together {
                i += 1;
            }
            if i - start >= 2 {
                found.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    found
}

fn crosses(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 < b.0 && b.0 < a.1 && a.1 < b.1 || b.0 < a.0 && a.0 < b.1 && b.1 < a.1
}

impl Groups {
    /// The groups of `cells`: the automatic rules plus groups by hand, or only groups by hand when `manual_only`
    /// (the notebook-wide "Manual grouping" mode). A group by hand that would cross an automatic group is dropped.
    pub fn compute(cells: &[CellInfo], manual_only: bool) -> Groups {
        let mut spans = if manual_only { vec![] } else { automatic(cells) };
        for m in manual(cells) {
            if !spans.iter().any(|&s| s == m || crosses(s, m)) {
                spans.push(m);
            }
        }
        spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
        let mut list: Vec<Group> = vec![];
        let mut open: Vec<usize> = vec![];
        for (start, end) in spans {
            while open.last().is_some_and(|&g| list[g].end <= start) {
                open.pop();
            }
            list.push(Group { start, end, parent: open.last().copied(), column: 1 });
            open.push(list.len() - 1);
        }
        // children come after their parents, so a backward pass sees every child before its parent
        for g in (0..list.len()).rev() {
            if let Some(p) = list[g].parent {
                list[p].column = list[p].column.max(list[g].column + 1);
            }
        }
        let mut visible = vec![true; cells.len()];
        for g in &list {
            if let Some(k) = cells[g.start].closed {
                let shown = g.start + k.min(g.end - g.start - 1);
                for (i, v) in visible.iter_mut().enumerate().take(g.end).skip(g.start) {
                    if i != shown {
                        *v = false;
                    }
                }
            }
        }
        Groups { list, visible }
    }

    /// The groups containing cell `i`, innermost first.
    pub fn containing(&self, i: usize) -> Vec<usize> {
        let mut found: Vec<usize> =
            (0..self.list.len()).filter(|&g| self.list[g].start <= i && i < self.list[g].end).collect();
        found.reverse();
        found
    }

    /// The group headed by cell `i`, if it heads one (the outermost, when several start there).
    pub fn headed_by(&self, i: usize) -> Option<usize> {
        self.list.iter().position(|g| g.start == i)
    }

    /// The end of the run of cells that belong with cell `i` when it is moved past: one past the last cell of the
    /// outermost closed group around `i`, else `i + 1`.
    pub fn after_hidden(&self, i: usize) -> usize {
        let mut end = i + 1;
        while end < self.visible.len() && !self.visible[end] {
            end += 1;
        }
        end
    }

    /// The groups as text: cells named by `names`, groups in braces (`{i o} t {S {SS a}}`), for tests.
    pub fn notation(&self, names: &[String]) -> String {
        let mut out = String::new();
        for (i, name) in names.iter().enumerate() {
            if !out.is_empty() {
                out.push(' ');
            }
            for _ in self.list.iter().filter(|g| g.start == i) {
                out.push('{');
            }
            out.push_str(name);
            for _ in self.list.iter().filter(|g| g.end == i + 1) {
                out.push('}');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Style::*;

    fn notation(cells: &[(Style, &str)]) -> String {
        let infos: Vec<_> = cells.iter().map(|&(style, _)| CellInfo { style, together: false, closed: None }).collect();
        let names: Vec<_> = cells.iter().map(|c| c.1.to_string()).collect();
        Groups::compute(&infos, false).notation(&names)
    }

    #[test]
    fn titles() {
        let v = [(Title, "T"), (Subtitle, "st"), (Subsubtitle, "sst"), (Text, "x"), (Title, "T2"), (Text, "y")];
        assert_eq!(notation(&v), "{T st sst x} {T2 y}");
    }

    #[test]
    fn sections() {
        let v = [
            (Text, "t0"),
            (Section, "S"),
            (Subsection, "SS"),
            (Subsubsection, "SSS"),
            (Text, "a"),
            (Subsection, "SS2"),
            (Text, "b"),
            (Section, "S2"),
        ];
        assert_eq!(notation(&v), "t0 {S {SS {SSS a}} {SS2 b}} S2");
    }

    #[test]
    fn inputs_and_outputs() {
        let v = [
            (Input, "i"),
            (Output, "o"),
            (Text, "t"),
            (Input, "i2"),
            (Print, "p"),
            (Message, "m"),
            (Output, "o2"),
            (Output, "o3"),
            (Input, "i3"),
            (Input, "i4"),
            (Output, "o4"),
        ];
        assert_eq!(notation(&v), "{i o} t {i2 p m o2 o3} i3 {i4 o4}");
        assert_eq!(notation(&[(Text, "t"), (Output, "o"), (Print, "p"), (Output, "o2")]), "t o p o2");
        assert_eq!(notation(&[(Code, "c"), (Output, "o"), (Input, "i"), (Text, "t"), (Output, "o2")]), "{c o} i t o2");
    }

    #[test]
    fn items() {
        let v = [
            (Item, "i"),
            (ItemParagraph, "ip"),
            (Subitem, "s"),
            (Subsubitem, "ss"),
            (Subitem, "s2"),
            (Item, "i2"),
            (Text, "t"),
            (ItemNumbered, "n"),
            (SubitemNumbered, "sn"),
        ];
        assert_eq!(notation(&v), "{i ip {s ss} s2} i2 t {n sn}");
        let v = [(Section, "S"), (Item, "i"), (Subitem, "s"), (Input, "c"), (Output, "o"), (Subsection, "SS")];
        assert_eq!(notation(&v), "{S {i {s {c o}}} SS}");
    }

    #[test]
    fn a_whole_document() {
        let v = [
            (Title, "Title"),
            (Section, "S1"),
            (Text, "t"),
            (Input, "In"),
            (Output, "Out"),
            (Input, "x"),
            (Subsection, "Sub"),
            (Item, "i1"),
            (Subitem, "s1"),
            (Item, "i2"),
            (Text, "t2"),
            (Section, "S2"),
            (Code, "c"),
            (Message, "m"),
            (Print, "pr"),
            (Output, "o"),
            (Chapter, "cc"),
            (Section, "sec"),
        ];
        assert_eq!(notation(&v), "{Title {S1 t {In Out} x {Sub {i1 s1} i2 t2}} {S2 {c m pr o}} {cc sec}}");
    }

    #[test]
    fn columns_parents_and_visibility() {
        let mut infos: Vec<_> = [Section, Input, Output, Text, Section]
            .into_iter()
            .map(|style| CellInfo { style, together: false, closed: None })
            .collect();
        let g = Groups::compute(&infos, false);
        assert_eq!(g.list.len(), 2);
        assert_eq!((g.list[0].start, g.list[0].end, g.list[0].column), (0, 4, 2));
        assert_eq!((g.list[1].start, g.list[1].end, g.list[1].column, g.list[1].parent), (1, 3, 1, Some(0)));
        assert_eq!(g.containing(2), [1, 0]);
        infos[1].closed = Some(1);
        let g = Groups::compute(&infos, false);
        assert_eq!(g.visible, [true, false, true, true, true]);
        infos[0].closed = Some(0);
        let g = Groups::compute(&infos, false);
        assert_eq!(g.visible, [true, false, false, false, true]);
        assert_eq!(g.after_hidden(0), 4);
    }

    #[test]
    fn groups_by_hand() {
        let mut infos: Vec<_> = [Text, Text, Text, Input]
            .into_iter()
            .map(|style| CellInfo { style, together: false, closed: None })
            .collect();
        infos[1].together = true;
        infos[2].together = true;
        let names: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        assert_eq!(Groups::compute(&infos, false).notation(&names), "a {b c} d");
        assert_eq!(Groups::compute(&infos, true).notation(&names), "a {b c} d");
    }
}
