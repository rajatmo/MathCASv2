//! What the model asks of its surroundings ([`Env`]): the visual lines of cells (layout), the clipboard and the
//! time; [`FakeEnv`] answers with fixed-width text and no wrapping, for tests and key scripts.
use crate::cell::Cell;
use std::ops::Range;

/// The clipboard's content: plain text and the private flavour (a `.mcn` cell fragment).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Clip {
    /// the text flavour
    pub text: Option<String>,
    /// the private flavour: cells as written by `mc_nbfile::print_cells`
    pub cells: Option<String>,
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
}

/// Fixed-width layout (one unit per character, lines only at line ends), an in-memory clipboard and a manual clock.
#[derive(Clone, Debug, Default)]
pub struct FakeEnv {
    /// the clipboard
    pub clip: Clip,
    /// the time [`Env::now`] answers
    pub time: f64,
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
}
