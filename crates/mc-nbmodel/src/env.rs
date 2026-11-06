//! What the model asks of its surroundings ([`Env`]): the visual lines of cells (layout), the clipboard and the
//! time; [`FakeEnv`] answers with fixed-width text and no wrapping, for tests and key scripts.
use crate::cell::Cell;
use std::ops::Range;

/// The clipboard's content: plain text and the private flavours (a `.mcn` cell fragment, or boxes).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clip {
    /// the text flavour
    pub text: Option<String>,
    /// the private flavour of cells: cells as written by `mc_nbfile::print_cells`
    pub cells: Option<String>,
    /// the private flavour of boxes (round 13): the canonical box text (4.4) of a copied range holding a square
    /// root, fraction or power box; `text` then holds its linear text
    pub boxes: Option<String>,
}

impl Clip {
    /// Text only (plain text, LaTeX, a range without boxes).
    pub fn text(t: String) -> Clip {
        Clip { text: Some(t), ..Clip::default() }
    }

    /// The clipboard of a copied box range: its linear text, and its canonical text as the box flavour unless
    /// `plain` (Copy as plain text) or the range holds no box.
    pub fn of_boxes(row: &mc_boxes::Row, plain: bool) -> Clip {
        let row = mc_boxes::restrict(row);
        let boxes = (!plain && mc_boxes::has_boxes(&row)).then(|| mc_boxes::to_text(&row));
        Clip { text: Some(mc_boxes::linear_text(&row)), cells: None, boxes }
    }
}

/// Layout queries, clipboard and time for commands.
pub trait Env {
    /// The visual lines of a cell's content as byte ranges (wrapping included; a line end is not in any range).
    fn lines(&mut self, cell: &Cell) -> Vec<Range<usize>>;
    /// The horizontal position of `pos` in its visual line, in any unit the other methods share.
    fn x_of(&mut self, cell: &Cell, pos: usize) -> f32;
    /// The position in `line` closest to the horizontal position `x`.
    fn pos_at(&mut self, cell: &Cell, line: Range<usize>, x: f32) -> usize;
    /// Seconds since some fixed start (typing runs end after a pause).
    fn now(&self) -> f64;
    /// Put text and optionally cells on the clipboard.
    fn set_clipboard(&mut self, clip: Clip);
    /// What the clipboard holds.
    fn clipboard(&mut self) -> Clip;
    /// In a typeset cell: the caret position on the visual line above (`up`) or below the caret at `offset` of the
    /// row at `path` (wrapped content), `None` on the first / last line.
    fn box_line(&mut self, cell: &Cell, path: &[mc_boxes::Step], offset: usize, up: bool) -> Option<(mc_boxes::RowPath, usize)> {
        let _ = (cell, path, offset, up);
        None
    }
    /// In a typeset cell: the start (or, with `end`, the end before its line break) of the visual line holding
    /// offset `offset` of the root row (Home and End on wrapped content); `None` leaves it to the line breaks.
    fn box_line_edge(&mut self, cell: &Cell, offset: usize, end: bool) -> Option<usize> {
        let _ = (cell, offset, end);
        None
    }
    /// Whether the cell is shown as one unit with two caret places, before and after it (a typeset output, whose
    /// boxes the caret does not enter): character and word motions jump across it and a double-click selects it.
    fn atomic(&mut self, cell: &Cell) -> bool {
        let _ = cell;
        false
    }
}

/// Fixed-width layout (one unit per character, lines only at line ends), an in-memory clipboard and a manual clock.
#[derive(Clone, Debug, Default)]
pub struct FakeEnv {
    /// the clipboard
    pub clip: Clip,
    /// the time [`Env::now`] answers
    pub time: f64,
    /// the cells [`Env::atomic`] answers `true` for (typeset outputs)
    pub atomic: Vec<crate::cell::CellId>,
    /// every Output cell counts as a typeset output (key scripts: `TypesetOutputs`)
    pub typeset_outputs: bool,
}

impl Env for FakeEnv {
    fn lines(&mut self, cell: &Cell) -> Vec<Range<usize>> {
        crate::text::lines(&cell.text)
    }

    fn x_of(&mut self, cell: &Cell, pos: usize) -> f32 {
        let start = crate::text::line_start(&cell.text, pos);
        cell.text[start..pos].chars().count() as f32
    }

    fn pos_at(&mut self, cell: &Cell, line: Range<usize>, x: f32) -> usize {
        let mut pos = line.start;
        let mut col = 0.0;
        for c in cell.text[line.clone()].chars() {
            if col + 0.5 > x {
                break;
            }
            col += 1.0;
            pos += c.len_utf8();
        }
        pos
    }

    fn now(&self) -> f64 {
        self.time
    }

    fn set_clipboard(&mut self, clip: Clip) {
        self.clip = clip;
    }

    fn clipboard(&mut self) -> Clip {
        self.clip.clone()
    }

    fn atomic(&mut self, cell: &Cell) -> bool {
        let output = cell.style == crate::style::Style::Output && !cell.failure && cell.graphic().is_none();
        self.atomic.contains(&cell.id) || self.typeset_outputs && output
    }
}
