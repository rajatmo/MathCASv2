//! What an export shows of a notebook, independent of the target format: the cells as a flat list of parts
//! (headings, paragraphs, items with their numbers, code, outputs grouped as the notebook groups them).
use mc_nbfile::images::{image_path, size_of};
use mc_nbfile::{Kind, Notebook, OutputKind};

/// The level of a heading, 1 (title) to 6.
pub(crate) fn heading_level(kind: Kind) -> Option<u8> {
    Some(match kind {
        Kind::Title => 1,
        Kind::Chapter | Kind::Subchapter => 2,
        Kind::Section => 3,
        Kind::Subsection => 4,
        Kind::Subsubsection => 5,
        _ => return None,
    })
}

/// One part of the exported document, in order.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Part {
    /// a title, chapter or section heading (also subtitles: `kind` tells which)
    Heading { kind: Kind, text: String },
    /// a text cell (line breaks kept)
    Para { text: String },
    /// an item: level 1 to 3, the number of a numbered item (`1.`, `2.1.`), whether it is a paragraph without a
    /// bullet
    Item { level: u8, number: Option<String>, bullet: bool, text: String },
    /// a display formula: prose drawn as typeset output
    Formula { text: String },
    /// a program cell: verbatim code without colouring
    Program { text: String },
    /// an input (`init` false) or initialization code cell; `typeset` cells hold canonical box text
    Code { source: String, init: bool, typeset: bool },
    /// the output of the code cell above
    Out(Out),
}

/// One generated cell of an output block.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Out {
    /// a value (consecutive `#=` lines), drawn typeset and right-justified
    Value(String),
    /// a picture value: the path as written, its display size in points when the block has one
    Picture { path: String, size: Option<(u32, u32)> },
    /// `print` output (`#|`)
    Print(String),
    /// a failure or an error (`#!`)
    Failure(String),
    /// a message (`#?`)
    Note(String),
}

/// The counters of numbered items (reset at titles, chapters and sections).
#[derive(Default)]
struct Numbers([u32; 3]);

impl Numbers {
    fn next(&mut self, level: usize) -> String {
        self.0[level] += 1;
        for n in &mut self.0[level + 1..] {
            *n = 0;
        }
        let parts: Vec<String> = self.0[..=level].iter().map(|n| n.max(&1).to_string()).collect();
        format!("{}.", parts.join("."))
    }
}

/// The parts of a notebook, in order. Every cell is shown, closed and hidden ones too (an export is the whole
/// document).
pub(crate) fn parts(nb: &Notebook) -> Vec<Part> {
    let mut out = Vec::new();
    let mut numbers = Numbers::default();
    for cell in &nb.cells {
        let text = cell.content();
        let item = |level: u8, number: Option<String>, bullet: bool| Part::Item { level, number, bullet, text: text.clone() };
        let part = match cell.kind {
            Kind::Title | Kind::Chapter | Kind::Section => {
                numbers = Numbers::default();
                Part::Heading { kind: cell.kind, text: text.clone() }
            }
            Kind::Subtitle | Kind::Subsubtitle | Kind::Subchapter | Kind::Subsection | Kind::Subsubsection => {
                Part::Heading { kind: cell.kind, text: text.clone() }
            }
            Kind::Text => Part::Para { text: text.clone() },
            Kind::Item => item(1, None, true),
            Kind::ItemParagraph => item(1, None, false),
            Kind::Subitem => item(2, None, true),
            Kind::SubitemParagraph => item(2, None, false),
            Kind::Subsubitem => item(3, None, true),
            Kind::ItemNumbered => item(1, Some(numbers.next(0)), false),
            Kind::SubitemNumbered => item(2, Some(numbers.next(1)), false),
            Kind::SubsubitemNumbered => item(3, Some(numbers.next(2)), false),
            Kind::DisplayFormula => Part::Formula { text: text.clone() },
            Kind::Program => Part::Program { text: text.clone() },
            Kind::Input | Kind::Code => {
                Part::Code { source: text.clone(), init: cell.kind == Kind::Code, typeset: cell.attrs.has("typeset") }
            }
        };
        out.push(part);
        for block in &cell.outputs {
            let size = size_of(&block.attrs);
            let mut lines = block.lines.iter().peekable();
            while let Some(first) = lines.next() {
                let mut text = first.text.clone();
                while let Some(l) = lines.next_if(|l| l.kind == first.kind) {
                    text.push('\n');
                    text.push_str(&l.text);
                }
                if first.kind == OutputKind::Data {
                    // the bytes of an embedded picture: the value names it
                    continue;
                }
                out.push(Part::Out(match first.kind {
                    OutputKind::Value => match image_path(&text) {
                        Some(path) => Out::Picture { path, size },
                        None => Out::Value(text),
                    },
                    OutputKind::Print => Out::Print(text),
                    OutputKind::Message => Out::Failure(text),
                    OutputKind::Note | OutputKind::Data => Out::Note(text),
                }));
            }
        }
    }
    out
}

/// The title of the document: the first title cell's text (its first line), if any.
/// Where the multiplication signs of a code text go (round 35: a space between two operands multiplies, and the
/// notebook shows a faint sign there): for each such gap on one line, the byte offset of its middle space. An export
/// writes the sign in place of that space, so the columns of the code do not move. A product across a line end has
/// no space to take and gets none.
pub(crate) fn product_marks(src: &str) -> Vec<usize> {
    mc_syntax::parse(src)
        .products
        .iter()
        .filter_map(|g| {
            let (a, b) = (g.start as usize, g.end as usize);
            let gap = src.get(a..b)?;
            (!gap.is_empty() && gap.bytes().all(|c| c == b' ')).then_some(a + gap.len() / 2)
        })
        .collect()
}

pub(crate) fn first_title(nb: &Notebook) -> Option<String> {
    let cell = nb.cells.iter().find(|c| c.kind == Kind::Title)?;
    let t = cell.content();
    let t = t.lines().next().unwrap_or("").trim().to_string();
    (!t.is_empty()).then_some(t)
}

/// Prose split at code spans: (text, is code). A backtick without a partner is kept as a character.
pub(crate) fn code_spans(text: &str) -> Vec<(String, bool)> {
    let mut out: Vec<(String, bool)> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        match after.find('`') {
            Some(close) if close > 0 && !after[..close].contains('\n') => {
                if open > 0 {
                    out.push((rest[..open].to_string(), false));
                }
                out.push((after[..close].to_string(), true));
                rest = &after[close + 1..];
            }
            _ => {
                out.push((rest[..open + 1].to_string(), false));
                rest = after;
            }
        }
    }
    if !rest.is_empty() {
        out.push((rest.to_string(), false));
    }
    // merge neighbouring plain pieces
    let mut merged: Vec<(String, bool)> = Vec::new();
    for (t, code) in out {
        match merged.last_mut() {
            Some((prev, false)) if !code => prev.push_str(&t),
            _ => merged.push((t, code)),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_spans_split_prose() {
        assert_eq!(code_spans("a `b` c"), [("a ".into(), false), ("b".into(), true), (" c".into(), false)]);
        assert_eq!(code_spans("x ` y"), [("x ` y".to_string(), false)]);
        assert_eq!(code_spans("``"), [("``".to_string(), false)]);
    }

    #[test]
    fn numbered_items_count_within_sections() {
        let nb = Notebook::parse(
            "#! mathcas 2 notebook\n\n#% itemnumbered\n# a\n\n#% subitemnumbered\n# b\n\n#% itemnumbered\n# c\n\n\
             #% section\n# S\n\n#% itemnumbered\n# d\n",
        )
        .unwrap();
        let numbers: Vec<String> = parts(&nb)
            .into_iter()
            .filter_map(|p| match p {
                Part::Item { number, .. } => number,
                _ => None,
            })
            .collect();
        assert_eq!(numbers, ["1.", "1.1.", "2.", "1."]);
    }

    #[test]
    fn output_lines_group_by_kind() {
        let nb = Notebook::parse(
            "#! mathcas 2 notebook\n\n#% cell\nx\n#% output size=100x50\n#| a\n#| b\n#= image(\"p.png\")\n#! f\n",
        )
        .unwrap();
        let p = parts(&nb);
        assert_eq!(p[1], Part::Out(Out::Print("a\nb".into())));
        assert_eq!(p[2], Part::Out(Out::Picture { path: "p.png".into(), size: Some((100, 50)) }));
        assert_eq!(p[3], Part::Out(Out::Failure("f".into())));
    }
}
