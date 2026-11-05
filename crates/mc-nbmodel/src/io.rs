//! The model and the `.mcn` file (design 7.2): cells with attributes, generated cells as output blocks.
//!
//! Reading: consecutive output lines with the same prefix form one generated cell: `#|` a Print cell, `#=` an
//! Output cell, `#!` an Output cell showing a failure when its block is numbered or its text is a failure (else a
//! Message cell holding an error), `#?` a Message cell (a warning). Writing: the generated cells after a code cell
//! become its output blocks; a numbered Output cell closes a block (`n=N`), the rest share one block. Generated
//! cells that follow no code cell cannot be written and are dropped (decision NB-core.4).
use crate::cell::{Cell, CellId};
use crate::notebook::Notebook;
use crate::style::Style;
use mc_nbfile::{self as file, Attrs, OutputBlock, OutputKind, OutputLine};

/// The attributes the model interprets on a cell marker.
const KNOWN: [&str; 13] = [
    "label",
    "typeset",
    "traditional",
    "closed",
    "shown",
    "noeval",
    "readonly",
    "nodelete",
    "init",
    "noinit",
    "together",
    "hidden",
    "evaluatable",
];

fn cell_from_file(nb: &mut Notebook, fc: &file::Cell) -> Cell {
    let style = Style::of_kind(fc.kind);
    let mut c = nb.new_cell(style, &fc.content());
    let a = &fc.attrs;
    c.label = a.number("label");
    c.props.typeset = a.has("typeset");
    c.props.traditional = a.has("traditional");
    c.closed = if a.has("closed") { Some(0) } else { a.number::<usize>("shown").map(|k| k.saturating_sub(1)) };
    if a.has("noeval") {
        c.props.evaluatable = Some(false);
    } else if a.has("evaluatable") {
        c.props.evaluatable = Some(true);
    }
    if a.has("readonly") {
        c.props.editable = Some(false);
    }
    if a.has("nodelete") {
        c.props.deletable = Some(false);
    }
    if a.has("init") {
        c.props.initialization = Some(true);
    } else if a.has("noinit") {
        c.props.initialization = Some(false);
    }
    c.props.together = a.has("together");
    c.props.hidden = a.has("hidden");
    c.extra = Attrs(a.0.iter().filter(|x| !KNOWN.contains(&x.name.as_str())).cloned().collect());
    c
}

fn generated_from_block(nb: &mut Notebook, input: CellId, block: &OutputBlock, out: &mut Vec<Cell>) {
    let n: Option<u32> = block.attrs.number("n");
    let extra = Attrs(block.attrs.0.iter().filter(|a| a.name != "n").cloned().collect());
    let mut k = 0;
    while k < block.lines.len() {
        let kind = block.lines[k].kind;
        let mut j = k;
        while j < block.lines.len() && block.lines[j].kind == kind {
            j += 1;
        }
        let text = block.lines[k..j].iter().map(|l| l.text.as_str()).collect::<Vec<_>>().join("\n");
        let (style, label, failure) = match kind {
            OutputKind::Print => (Style::Print, None, false),
            OutputKind::Value => (Style::Output, n, false),
            OutputKind::Message if n.is_some() || text.starts_with("failure(") || text == "aborted" => {
                (Style::Output, n, true)
            }
            OutputKind::Message => (Style::Message, None, true),
            OutputKind::Note => (Style::Message, None, false),
        };
        let mut c = nb.new_cell(style, &text);
        c.label = label;
        c.failure = failure;
        c.input_of = Some(input);
        if style == Style::Output {
            c.out_extra = extra.clone();
        }
        out.push(c);
        k = j;
    }
}

impl Notebook {
    /// The model of a parsed file.
    pub fn from_file(f: &file::Notebook) -> Notebook {
        let mut nb = Notebook::new();
        nb.header = f.header.clone();
        let mut cells = vec![];
        for fc in &f.cells {
            let c = cell_from_file(&mut nb, fc);
            let id = c.id;
            cells.push(c);
            for b in &fc.outputs {
                generated_from_block(&mut nb, id, b, &mut cells);
            }
        }
        nb.cells = cells;
        nb.structure_changed();
        nb.sel = crate::Selection::Gap(nb.cells.len());
        nb.mark_saved();
        nb
    }

    /// The model of a notebook text.
    pub fn parse(text: &str) -> Result<Notebook, file::ParseError> {
        file::Notebook::parse(text).map(|f| Notebook::from_file(&f))
    }

    /// The file of the model, and for each model cell the index of the file cell it is written in (generated cells:
    /// the cell whose output blocks hold them; `None` for those that cannot be written).
    pub fn to_file_mapped(&self) -> (file::Notebook, Vec<Option<usize>>) {
        let mut f = file::Notebook::new();
        f.header = self.header.clone();
        let mut map = vec![];
        let mut i = 0;
        while i < self.cells.len() {
            let c = &self.cells[i];
            if c.generated() {
                // generated cells after no code cell
                map.push(None);
                i += 1;
                continue;
            }
            let kind = c.style.kind().expect("not generated");
            let mut fc = file::Cell::new(kind, &c.text);
            fc.attrs = cell_attrs(c);
            if c.expr_view {
                // a cell shown as its source is written as the cell the source describes; a source that does not
                // read as one cell is kept as program text, so nothing is lost (decision NB-core.6)
                let parsed = file::parse_cells(&format!("{}\n{}\n", file::FRAGMENT, c.text));
                fc = match parsed {
                    Some(mut cells) if cells.len() == 1 && cells[0].outputs.is_empty() => cells.remove(0),
                    _ => file::Cell::new(file::Kind::Program, &c.text),
                };
            }
            let kind = fc.kind;
            map.push(Some(f.cells.len()));
            i += 1;
            if kind.is_code() {
                let mut block = OutputBlock::default();
                while i < self.cells.len() && self.cells[i].generated() {
                    let g = &self.cells[i];
                    let prefix = match (g.style, g.failure) {
                        (Style::Print, _) => OutputKind::Print,
                        (Style::Output, false) => OutputKind::Value,
                        (Style::Output, true) | (Style::Message, true) => OutputKind::Message,
                        _ => OutputKind::Note,
                    };
                    block.lines.extend(g.text.split('\n').map(|t| OutputLine { kind: prefix, text: t.to_string() }));
                    map.push(Some(f.cells.len()));
                    if g.style == Style::Output && g.label.is_some() {
                        block.attrs.set("n", g.label.map(|n| n.to_string()));
                        block.attrs.0.extend(g.out_extra.0.iter().cloned());
                        fc.outputs.push(std::mem::take(&mut block));
                    }
                    i += 1;
                }
                if !block.lines.is_empty() {
                    fc.outputs.push(block);
                }
            }
            f.cells.push(fc);
        }
        // one blank line between cells, none after the last
        if let Some(last) = f.cells.last_mut() {
            last.gap = 0;
        }
        (f, map)
    }

    /// The file of the model.
    pub fn to_file(&self) -> file::Notebook {
        self.to_file_mapped().0
    }

    /// The text of the model as a `.mcn` file.
    pub fn to_text(&self) -> String {
        self.to_file().print()
    }

    /// The 1-based line of cell `i`'s first content line in the file as it would be written (for error positions).
    pub(crate) fn file_line(&self, i: usize) -> u32 {
        let (f, map) = self.to_file_mapped();
        map[i].map_or(1, |k| f.first_lines()[k])
    }

    /// The cells as a clipboard fragment (with the generated cells that follow code cells among them).
    pub(crate) fn fragment(&self, ids: &[CellId]) -> String {
        let mut sub = Notebook::new();
        sub.cells = ids.iter().filter_map(|id| self.cell(*id).cloned()).collect();
        file::print_cells(&sub.to_file().cells)
    }

    /// The cells of a fragment, with fresh ids.
    pub(crate) fn cells_of_fragment(&mut self, text: &str) -> Option<Vec<Cell>> {
        let cells = file::parse_cells(text)?;
        let mut f = file::Notebook::new();
        f.cells = cells;
        let sub = Notebook::from_file(&f);
        Some(sub.cells.into_iter().map(|c| self.renumber(c)).collect())
    }

    /// `c` with a fresh id.
    pub(crate) fn renumber(&mut self, mut c: Cell) -> Cell {
        c.id = self.fresh_id();
        c
    }
}

/// The marker attributes of a cell, in the fixed order: label, style marks, state marks, then unknown ones.
fn cell_attrs(c: &Cell) -> Attrs {
    let mut a = Attrs::new();
    if let Some(n) = c.label {
        a.set("label", Some(n.to_string()));
    }
    a.flag("typeset", c.props.typeset);
    a.flag("traditional", c.props.traditional);
    match c.closed {
        Some(0) => a.flag("closed", true),
        Some(k) => a.set("shown", Some((k + 1).to_string())),
        None => {}
    }
    match c.props.evaluatable {
        Some(false) => a.flag("noeval", true),
        Some(true) if !c.style.evaluatable() => a.flag("evaluatable", true),
        _ => {}
    }
    a.flag("readonly", c.props.editable == Some(false));
    a.flag("nodelete", c.props.deletable == Some(false));
    match c.props.initialization {
        Some(true) if c.style != Style::Code => a.flag("init", true),
        Some(false) if c.style == Style::Code => a.flag("noinit", true),
        _ => {}
    }
    a.flag("together", c.props.together);
    a.flag("hidden", c.props.hidden);
    a.0.extend(c.extra.0.iter().cloned());
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_round_trip_through_the_model() {
        let text = "#! mathcas 2 notebook magnification=1.25

#% title
# T

#% section closed
# S

#% cell label=1 future=x
1 + 1
3 + 3
#% output n=1
#= 2
#% output n=2
#| hi
#= 6
#% output
#? note

#% code label=3 together
f(x) = 1

#% cell noeval
2
#% output
#! failure(division_by_zero)
#= [6, 3]
";
        let nb = Notebook::parse(text).unwrap();
        let styles: Vec<Style> = nb.cells().iter().map(|c| c.style).collect();
        use Style::*;
        assert_eq!(styles, [Title, Section, Input, Output, Print, Output, Message, Code, Input, Output, Output]);
        assert_eq!(nb.cells()[3].label, Some(1));
        assert!(nb.cells()[9].failure && nb.cells()[9].label.is_none());
        assert_eq!(nb.to_text(), text);
        assert!(!nb.is_modified());
    }

    #[test]
    fn errors_and_messages() {
        let text = "#! mathcas 2 notebook\n\n#% cell\n1 +\n#% output\n#! <input>:1:4: expected\n#!   1 +\n";
        let nb = Notebook::parse(text).unwrap();
        assert_eq!(nb.cells()[1].style, Style::Message);
        assert_eq!(nb.cells()[1].text, "<input>:1:4: expected\n  1 +");
        assert_eq!(nb.to_text(), text);
    }
}
