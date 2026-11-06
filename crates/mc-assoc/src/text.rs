//! The printed form of a dataset for text front ends (decision DATA.10): a table with a header row, numbers aligned
//! right and other values left, strings without quotes, `missing` for missing values; at most [`MAX_ROWS`] rows
//! (the first and the last ones when there are more), cells cut at [`MAX_CELL`] characters and columns up to
//! [`MAX_WIDTH`] characters a line, and a last line with the size.
use crate::table::{DataSyms, Table};
use mc_core::{Heap, Store, View, T};

/// The most rows shown; a longer dataset shows its first [`HEAD`] rows, a `...` line and its last
/// `MAX_ROWS - HEAD` rows.
pub const MAX_ROWS: usize = 20;
const HEAD: usize = 15;
/// The most characters of a cell (longer ones end in `...`).
pub const MAX_CELL: usize = 32;
/// The most characters of a line; the columns that do not fit are counted in the last line.
pub const MAX_WIDTH: usize = 120;

/// A column as shown: its header, its cells (text, aligned right), its width, and whether its header is aligned
/// right.
type Shown = (String, Vec<(String, bool)>, usize, bool);

/// One cell's text and whether it is aligned right.
fn cell<'h>(s: &DataSyms, h: &'h Heap, t: T<'h>, print: &mut dyn FnMut(T<'h>) -> String) -> (String, bool) {
    let (text, right) = if s.is_missing(h, t) {
        ("missing".to_string(), false)
    } else {
        match h.view(t) {
            View::Str(x) => (x.replace('\n', "\\n"), false),
            View::Int(_) | View::Real(_) | View::BigInt { .. } | View::Rational { .. } | View::Ball { .. } => {
                (print(t), true)
            }
            _ => (print(t), false),
        }
    };
    (cut(&text), right)
}

/// `text` cut to [`MAX_CELL`] characters.
fn cut(text: &str) -> String {
    if text.chars().count() <= MAX_CELL {
        return text.to_string();
    }
    let mut s: String = text.chars().take(MAX_CELL - 3).collect();
    s.push_str("...");
    s
}

/// The table text of `t` when it is a dataset (`None` otherwise); `print` gives the text of a value that is not a
/// string (the front end's printer).
pub fn table_text<'h>(h: &'h Heap, t: T<'h>, print: &mut dyn FnMut(T<'h>) -> String) -> Option<String> {
    let s = DataSyms::new(h);
    let tab = Table::of(&s, h, t)?;
    let n = tab.n;
    let shown: Vec<Option<usize>> = if n <= MAX_ROWS {
        (0..n).map(Some).collect()
    } else {
        (0..HEAD).map(Some).chain(std::iter::once(None)).chain((n - (MAX_ROWS - HEAD)..n).map(Some)).collect()
    };
    // the columns that fit, each with its header, cells and width
    let mut columns: Vec<Shown> = vec![];
    let mut width = 0;
    for (j, &name) in tab.names.iter().enumerate() {
        let head = match h.view(name) {
            View::Str(x) => cut(x),
            _ => cut(&print(name)),
        };
        let mut cells: Vec<(String, bool)> = vec![];
        // a column of numbers (missing values aside) is aligned right, its header and missing values too
        let mut numeric = true;
        for r in &shown {
            let c = match r {
                Some(i) => {
                    let x = tab.cols[j].get(h, *i);
                    let c = cell(&s, h, x, print);
                    numeric &= c.1 || s.is_missing(h, x);
                    c
                }
                None => (String::new(), false),
            };
            cells.push(c);
        }
        let numeric = numeric && cells.iter().any(|c| c.1);
        if numeric {
            cells.iter_mut().for_each(|c| c.1 = true);
        }
        let w = cells.iter().map(|c| c.0.chars().count()).chain([head.chars().count(), 3]).max().unwrap_or(3);
        let sep = if columns.is_empty() { 0 } else { 3 };
        if !columns.is_empty() && width + sep + w > MAX_WIDTH {
            break;
        }
        width += sep + w;
        columns.push((head, cells, w, numeric));
    }
    let pad = |text: &str, w: usize, right: bool| {
        let fill = " ".repeat(w - text.chars().count());
        if right {
            format!("{fill}{text}")
        } else {
            format!("{text}{fill}")
        }
    };
    let mut out = String::new();
    let line = |parts: Vec<String>, out: &mut String| {
        out.push_str(parts.join(" | ").trim_end());
        out.push('\n');
    };
    if !columns.is_empty() {
        line(columns.iter().map(|(hd, _, w, right)| pad(hd, *w, *right)).collect(), &mut out);
        out.push_str(&columns.iter().map(|(_, _, w, _)| "-".repeat(*w)).collect::<Vec<_>>().join("-+-"));
        out.push('\n');
        for (r, row) in shown.iter().enumerate() {
            match row {
                Some(_) => line(columns.iter().map(|(_, cells, w, _)| pad(&cells[r].0, *w, cells[r].1)).collect(), &mut out),
                None => line(columns.iter().map(|(_, _, w, right)| pad("...", *w, *right)).collect(), &mut out),
            }
        }
    }
    let plural = |k: usize, what: &str| format!("{k} {what}{}", if k == 1 { "" } else { "s" });
    out.push_str(&format!("dataset: {}, {}", plural(n, "row"), plural(tab.names.len(), "column")));
    if columns.len() < tab.names.len() {
        out.push_str(&format!(" ({} not shown)", tab.names.len() - columns.len()));
    }
    Some(out)
}
