//! The `.mcn` notebook file format of MathCAS v2: [`Notebook::parse`], [`Notebook::print`], cell fragments for the
//! clipboard ([`print_cells`], [`parse_cells`]) and [`write_atomic`] for saving.
//!
//! The format is plain UTF-8 text, LF or CR LF (a rewrite keeps the file's line ends), a leading BOM skipped:
//!
//! ```text
//! file         = header { blank } { cell }
//! header       = "#! mathcas 2 notebook" [ attrs ] NL
//! cell         = marker NL body { output-block }
//! marker       = "#% " kind [ attrs ]
//! kind         = "cell" | "input" | "code" | "text" | "title" | "subtitle" | ... (see [`Kind`])
//! attrs        = { " " attr }            attr = name | name "=" value      value: [0-9A-Za-z_.:x-]+
//! body         = source lines (code kinds) | "# " text lines and "#" empty lines (prose kinds)
//! output-block = "#% output" [ attrs ] NL { out-line }
//! out-line     = "#| " print | "#= " value | "#! " failure or error | "#? " message | "#~ " embedded data
//! ```
//!
//! Pictures travel inside the file (round 13, user decision: one file per notebook, no sidecar folder): an output
//! block's `#~` lines carry the files its `image("name")` values name, see [`embed`].
//!
//! Every marker, prose and output line is a comment, so a notebook is also a script whose statements are its code
//! cells. A `#%` line whose first word is not a kind (or `output`) is an ordinary comment line of the cell it is in;
//! when that word looks like a misspelt kind a warning names the line ([`Notebook::read`]). Attributes the format
//! does not know are kept and written back unchanged. Printing a parsed notebook gives back the text it was parsed
//! from when the text is in the canonical layout (a marker before every cell, `cell` rather than `input`, single
//! spaces in marker lines, no blank lines between output blocks).
//!
//! A notebook that uses only `cell` and `text` cells without attributes and at most one output block per cell
//! without attributes is exactly the format before the extensions, which older readers understand.
pub mod embed;
pub mod images;

use std::fmt;
use std::io::{self, Write};
use std::path::Path;

/// The first line of a notebook (compared word by word, so extra spaces are accepted).
pub const HEADER: &str = "#! mathcas 2 notebook";
/// The marker line before a code cell (the canonical spelling of [`Kind::Input`]).
pub const CELL: &str = "#% cell";
/// The marker line before a text cell.
pub const TEXT: &str = "#% text";
/// The marker line before an output block of the code cell above it.
pub const OUTPUT: &str = "#% output";
/// The first line of a cell fragment ([`print_cells`]), as put on the clipboard.
pub const FRAGMENT: &str = "#% cells";

/// The kind of a cell, written after `#% `. The kinds are the cell styles of the notebook; the file only fixes how
/// the body is written: source lines for the code kinds ([`Kind::is_code`]), comment lines for the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `cell` (also read as `input`): an input cell
    Input,
    /// `code`: an initialization code cell
    Code,
    /// `text`
    Text,
    /// `title`
    Title,
    /// `subtitle`
    Subtitle,
    /// `subsubtitle`
    Subsubtitle,
    /// `chapter`
    Chapter,
    /// `subchapter`
    Subchapter,
    /// `section`
    Section,
    /// `subsection`
    Subsection,
    /// `subsubsection`
    Subsubsection,
    /// `item`
    Item,
    /// `itemparagraph`
    ItemParagraph,
    /// `subitem`
    Subitem,
    /// `subitemparagraph`
    SubitemParagraph,
    /// `subsubitem`
    Subsubitem,
    /// `itemnumbered`
    ItemNumbered,
    /// `subitemnumbered`
    SubitemNumbered,
    /// `subsubitemnumbered`
    SubsubitemNumbered,
    /// `displayformula`
    DisplayFormula,
    /// `program`
    Program,
}

impl Kind {
    /// Every kind, in the order of the style table.
    pub const ALL: [Kind; 21] = [
        Kind::Title,
        Kind::Subtitle,
        Kind::Subsubtitle,
        Kind::Chapter,
        Kind::Subchapter,
        Kind::Section,
        Kind::Subsection,
        Kind::Subsubsection,
        Kind::Text,
        Kind::Code,
        Kind::Input,
        Kind::Item,
        Kind::ItemParagraph,
        Kind::Subitem,
        Kind::SubitemParagraph,
        Kind::Subsubitem,
        Kind::ItemNumbered,
        Kind::SubitemNumbered,
        Kind::SubsubitemNumbered,
        Kind::DisplayFormula,
        Kind::Program,
    ];

    /// The word written in the marker line.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Input => "cell",
            Kind::Code => "code",
            Kind::Text => "text",
            Kind::Title => "title",
            Kind::Subtitle => "subtitle",
            Kind::Subsubtitle => "subsubtitle",
            Kind::Chapter => "chapter",
            Kind::Subchapter => "subchapter",
            Kind::Section => "section",
            Kind::Subsection => "subsection",
            Kind::Subsubsection => "subsubsection",
            Kind::Item => "item",
            Kind::ItemParagraph => "itemparagraph",
            Kind::Subitem => "subitem",
            Kind::SubitemParagraph => "subitemparagraph",
            Kind::Subsubitem => "subsubitem",
            Kind::ItemNumbered => "itemnumbered",
            Kind::SubitemNumbered => "subitemnumbered",
            Kind::SubsubitemNumbered => "subsubitemnumbered",
            Kind::DisplayFormula => "displayformula",
            Kind::Program => "program",
        }
    }

    /// The kind written as `word` in a marker line (`input` is another spelling of `cell`).
    pub fn from_name(word: &str) -> Option<Kind> {
        if word == "input" {
            return Some(Kind::Input);
        }
        Kind::ALL.into_iter().find(|k| k.name() == word)
    }

    /// Whether the body is source (evaluated when the file runs as a script) rather than comment lines.
    pub fn is_code(self) -> bool {
        matches!(self, Kind::Input | Kind::Code)
    }
}

/// One attribute of a marker or header line: a flag (`closed`) or a name with a value (`label=3`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attr {
    /// the name
    pub name: String,
    /// the value after `=`, if any
    pub value: Option<String>,
}

/// The attributes of a marker or header line, in the order they are written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Attrs(pub Vec<Attr>);

fn is_name(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic()) && cs.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn is_value(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-'))
}

impl Attrs {
    /// No attributes.
    pub fn new() -> Attrs {
        Attrs(vec![])
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether the attribute `name` is present (as a flag or with a value).
    pub fn has(&self, name: &str) -> bool {
        self.0.iter().any(|a| a.name == name)
    }

    /// The value of `name`, if it is present with one.
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.iter().find(|a| a.name == name).and_then(|a| a.value.as_deref())
    }

    /// The value of `name` as a number, if it is present with a value that is one.
    pub fn number<T: std::str::FromStr>(&self, name: &str) -> Option<T> {
        self.get(name).and_then(|v| v.parse().ok())
    }

    /// Set `name` (a flag when `value` is `None`), in place if it is present, else at the end.
    pub fn set(&mut self, name: &str, value: Option<String>) {
        match self.0.iter_mut().find(|a| a.name == name) {
            Some(a) => a.value = value,
            None => self.0.push(Attr { name: name.to_string(), value }),
        }
    }

    /// Set or remove the flag `name`.
    pub fn flag(&mut self, name: &str, on: bool) {
        if on {
            if !self.has(name) {
                self.set(name, None);
            }
        } else {
            self.remove(name);
        }
    }

    /// Remove `name`.
    pub fn remove(&mut self, name: &str) {
        self.0.retain(|a| a.name != name);
    }

    /// Read the attributes of `text` (space-separated); the second result describes malformed ones, which are kept.
    pub fn parse(text: &str) -> (Attrs, Vec<String>) {
        let mut attrs = Attrs::new();
        let mut bad = vec![];
        for word in text.split(' ').filter(|w| !w.is_empty()) {
            let (name, value) = match word.split_once('=') {
                Some((n, v)) => (n, Some(v)),
                None => (word, None),
            };
            if !is_name(name) || value.is_some_and(|v| !is_value(v)) {
                bad.push(format!("malformed attribute `{word}`"));
            }
            attrs.0.push(Attr { name: name.to_string(), value: value.map(str::to_string) });
        }
        (attrs, bad)
    }
}

impl fmt::Display for Attrs {
    /// Each attribute with a space before it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for a in &self.0 {
            match &a.value {
                Some(v) => write!(f, " {}={v}", a.name)?,
                None => write!(f, " {}", a.name)?,
            }
        }
        Ok(())
    }
}

/// One line of an output block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputLine {
    /// what the line reports
    pub kind: OutputKind,
    /// the text, without the prefix
    pub text: String,
}

/// The kinds of output line, by their prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OutputKind {
    /// `#|`: a line of `print` output
    Print,
    /// `#=`: a line of a statement's value
    Value,
    /// `#!`: a line of a failure, an error, or the reason the run stopped (`interrupted`, a limit)
    Message,
    /// `#?`: a line of a message (a warning from library code)
    Note,
    /// `#~`: a line of a file embedded in the output block (a picture and its descriptions, [`embed`])
    Data,
}

impl OutputKind {
    /// The prefix of lines of this kind.
    pub fn prefix(self) -> &'static str {
        match self {
            OutputKind::Print => "#|",
            OutputKind::Value => "#=",
            OutputKind::Message => "#!",
            OutputKind::Note => "#?",
            OutputKind::Data => "#~",
        }
    }

    fn of(line: &str) -> Option<(OutputKind, &str)> {
        let kinds = [OutputKind::Print, OutputKind::Value, OutputKind::Message, OutputKind::Note, OutputKind::Data];
        kinds.into_iter().find_map(|k| {
            let rest = line.strip_prefix(k.prefix())?;
            Some((k, rest.strip_prefix(' ').unwrap_or(rest)))
        })
    }
}

/// An output block: `#% output` with its attributes (`n=N` the output's number, `size=WxH` a graphic's display
/// size) and its lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutputBlock {
    /// the attributes of the `#% output` line
    pub attrs: Attrs,
    /// the lines
    pub lines: Vec<OutputLine>,
}

impl OutputBlock {
    /// A block without attributes holding these lines.
    pub fn of(lines: Vec<OutputLine>) -> OutputBlock {
        OutputBlock { attrs: Attrs::new(), lines }
    }
}

/// A problem in a notebook text: the line (1-based) and what is wrong. As an error it stops reading; as a warning
/// ([`Notebook::read`]) it only reports something that was read leniently.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// the 1-based line of the problem
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for ParseError {}

/// One cell, its output blocks and the blank lines after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// the kind written in the marker
    pub kind: Kind,
    /// the marker's attributes
    pub attrs: Attrs,
    /// the body lines as in the file: source lines for code kinds, the comment lines (`# text`, `#`) for the rest
    pub lines: Vec<String>,
    /// the output blocks (code kinds only)
    pub outputs: Vec<OutputBlock>,
    /// blank lines after the cell (before the next marker or the end of the file)
    pub gap: usize,
}

impl Cell {
    /// A cell of `kind` holding `content`: source for code kinds, prose for the others (each line written with the
    /// comment prefix `# `, an empty line as `#`).
    pub fn new(kind: Kind, content: &str) -> Cell {
        let lines = if kind.is_code() {
            content.split('\n').map(str::to_string).collect()
        } else {
            content.split('\n').map(|l| if l.is_empty() { "#".to_string() } else { format!("# {l}") }).collect()
        };
        let mut lines: Vec<String> = lines;
        // a content with a final line end has no extra empty line
        if content.is_empty() || content.ends_with('\n') {
            lines.pop();
        }
        Cell { kind, attrs: Attrs::new(), lines, outputs: vec![], gap: 1 }
    }

    /// An input cell with this source and no output.
    pub fn code(source: &str) -> Cell {
        Cell::new(Kind::Input, source)
    }

    /// A text cell with this prose.
    pub fn text(text: &str) -> Cell {
        Cell::new(Kind::Text, text)
    }

    /// Whether the body is source.
    pub fn is_code(&self) -> bool {
        self.kind.is_code()
    }

    /// The source of a code cell as one text (lines joined with `\n`), or `None` for a prose cell.
    pub fn source(&self) -> Option<String> {
        self.is_code().then(|| self.lines.join("\n"))
    }

    /// The prose of a prose cell, its lines without the comment prefix (`# ` or `#`), or `None` for a code cell.
    pub fn prose(&self) -> Option<String> {
        (!self.is_code()).then(|| {
            let strip = |l: &String| l.strip_prefix("# ").or_else(|| l.strip_prefix('#')).unwrap_or(l).to_string();
            self.lines.iter().map(strip).collect::<Vec<_>>().join("\n")
        })
    }

    /// The content: the source of a code cell, the prose of the others.
    pub fn content(&self) -> String {
        self.source().or_else(|| self.prose()).unwrap_or_default()
    }

    /// Every output line of every block, in order.
    pub fn output_lines(&self) -> impl Iterator<Item = &OutputLine> {
        self.outputs.iter().flat_map(|b| b.lines.iter())
    }

    /// Replace the output blocks by one block without attributes holding `lines` (none for `None`): the output of a
    /// run in the format before the extensions.
    pub fn set_output(&mut self, lines: Option<Vec<OutputLine>>) {
        self.outputs = lines.map(OutputBlock::of).into_iter().collect();
    }

    /// The number of lines the cell takes in the file, gap included.
    fn height(&self) -> usize {
        1 + self.gap + self.lines.len() + self.outputs.iter().map(|b| 1 + b.lines.len()).sum::<usize>()
    }

    fn print_into(&self, line: &mut dyn FnMut(&str)) {
        line(&format!("#% {}{}", self.kind.name(), self.attrs));
        self.lines.iter().for_each(|l| line(l));
        for b in &self.outputs {
            line(&format!("{OUTPUT}{}", b.attrs));
            for o in &b.lines {
                let prefix = o.kind.prefix();
                if o.text.is_empty() {
                    line(prefix);
                } else {
                    line(&format!("{prefix} {}", o.text));
                }
            }
        }
        (0..self.gap).for_each(|_| line(""));
    }
}

/// A notebook: header attributes, cells in order, plus the layout details that make printing give back the text
/// that was parsed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notebook {
    /// the header's attributes (`grouping=manual`, `magnification=1.25`, ...)
    pub header: Attrs,
    /// blank lines between the header and the first cell
    pub gap: usize,
    /// the cells, in order
    pub cells: Vec<Cell>,
    /// whether lines end with CR LF (as the parsed text did); printing uses the same line ends
    pub crlf: bool,
}

/// What a line of a notebook is.
enum Line {
    /// a cell marker with its kind and attributes text
    Cell(Kind, String),
    /// an output marker with its attributes text
    Output(String),
    /// anything else
    Other,
}

/// What `line` is: a marker is `#% `, a kind word (or `output`), then nothing or a space and attributes.
fn classify(line: &str) -> Line {
    let Some(rest) = line.trim_end().strip_prefix("#% ") else { return Line::Other };
    let (word, attrs) = rest.split_once(' ').unwrap_or((rest, ""));
    if word == "output" {
        return Line::Output(attrs.to_string());
    }
    match Kind::from_name(word) {
        Some(k) => Line::Cell(k, attrs.to_string()),
        None => Line::Other,
    }
}

/// The kind `line` probably meant, when it is `#% word ...` with `word` a near miss of a kind name.
fn misspelt_marker(line: &str) -> Option<&'static str> {
    let rest = line.trim_end().strip_prefix("#% ")?;
    let word = rest.split(' ').next()?;
    if word.len() < 3 || !word.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let word = word.to_ascii_lowercase();
    let names = Kind::ALL.iter().map(|k| k.name()).chain(["output", "input"]);
    names.filter(|n| *n != word).find(|n| edit_distance(&word, n) <= 2.min(n.len() / 3).max(1))
}

/// Levenshtein distance of two ASCII words.
fn edit_distance(a: &str, b: &str) -> usize {
    let b = b.as_bytes();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.bytes().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// `lines` without its trailing blank lines, and how many there were.
fn split_gap(mut lines: Vec<String>) -> (Vec<String>, usize) {
    let keep = lines.iter().rposition(|l| !is_blank(l)).map_or(0, |i| i + 1);
    let gap = lines.len() - keep;
    lines.truncate(keep);
    (lines, gap)
}

/// The lines of `text` without line ends, and whether they end with CR LF.
fn lines_of(text: &str) -> (Vec<&str>, bool) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let crlf = text.contains("\r\n");
    let body = text.strip_suffix('\n').unwrap_or(text);
    (body.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect(), crlf)
}

/// A block between markers: the marker, its 1-based line, the lines after it.
struct Block {
    marker: Line,
    at: usize,
    lines: Vec<String>,
}

/// What [`read_cells`] found: the lines before the first marker, the cells, warnings.
type CellsRead = (Vec<String>, Vec<Cell>, Vec<ParseError>);

/// The cells of `lines` (the notebook after its header; `first` is the 1-based line number of `lines[0]`), the
/// lines before the first marker, and warnings.
fn read_cells(lines: &[&str], first: usize) -> Result<CellsRead, ParseError> {
    let mut warnings = vec![];
    let mut preamble = vec![];
    let mut blocks: Vec<Block> = vec![];
    for (i, &line) in lines.iter().enumerate() {
        let at = first + i;
        match (classify(line), blocks.last_mut()) {
            (Line::Other, b) => {
                if let Some(meant) = misspelt_marker(line) {
                    let message = format!("`{}` is not a cell marker (did you mean `#% {meant}`?)", line.trim_end());
                    warnings.push(ParseError { line: at, message });
                }
                match b {
                    Some(b) => b.lines.push(line.to_string()),
                    None => preamble.push(line.to_string()),
                }
            }
            (marker, _) => blocks.push(Block { marker, at, lines: vec![] }),
        }
    }
    let mut cells: Vec<Cell> = vec![];
    let mut attrs_of = |text: &str, at: usize| {
        let (attrs, bad) = Attrs::parse(text);
        warnings.extend(bad.into_iter().map(|message| ParseError { line: at, message }));
        attrs
    };
    let mut blocks = blocks.into_iter().peekable();
    while let Some(Block { marker, at, lines }) = blocks.next() {
        let cell = match marker {
            Line::Cell(kind, attrs) => {
                let attrs = attrs_of(&attrs, at);
                let mut outputs = vec![];
                let mut gap;
                let mut body = lines;
                if kind.is_code() && blocks.peek().is_some_and(|b| matches!(b.marker, Line::Output(_))) {
                    // the source keeps blank lines before the output marker, so the text prints back as it was
                    gap = 0;
                    while let Some(Block { marker: Line::Output(oattrs), at: out_at, lines: out }) =
                        blocks.next_if(|b| matches!(b.marker, Line::Output(_)))
                    {
                        let (out, g) = split_gap(out);
                        gap = g;
                        let lines = out
                            .iter()
                            .enumerate()
                            .map(|(k, l)| match OutputKind::of(l) {
                                Some((kind, text)) => Ok(OutputLine { kind, text: text.to_string() }),
                                None => Err(ParseError {
                                    line: out_at + 1 + k,
                                    message: "an output block holds only lines starting with `#|`, `#=`, `#!` or \
                                              `#?` (or `#~` data; start a new cell with `#% cell`)"
                                        .into(),
                                }),
                            })
                            .collect::<Result<_, _>>()?;
                        outputs.push(OutputBlock { attrs: attrs_of(&oattrs, out_at), lines });
                    }
                } else {
                    (body, gap) = split_gap(body);
                }
                Cell { kind, attrs, lines: body, outputs, gap }
            }
            Line::Output(_) => {
                let message = "an output block follows a code cell".into();
                return Err(ParseError { line: at, message });
            }
            Line::Other => unreachable!("blocks start at markers"),
        };
        cells.push(cell);
    }
    Ok((preamble, cells, warnings))
}

impl Notebook {
    /// An empty notebook.
    pub fn new() -> Notebook {
        Notebook { gap: 1, ..Notebook::default() }
    }

    /// Read a notebook. A byte order mark is skipped; lines may end with LF or CR LF.
    pub fn parse(text: &str) -> Result<Notebook, ParseError> {
        Notebook::read(text).map(|(nb, _)| nb)
    }

    /// Read a notebook, with warnings about what was read leniently (a misspelt marker kept as a comment line, a
    /// malformed attribute kept as written).
    pub fn read(text: &str) -> Result<(Notebook, Vec<ParseError>), ParseError> {
        let (lines, crlf) = lines_of(text);
        let header = lines.first().copied().unwrap_or("");
        let mut words = header.split(' ').filter(|w| !w.is_empty());
        if !words.by_ref().take(4).eq(HEADER.split_whitespace()) {
            let message = format!("a notebook starts with the line `{HEADER}`");
            return Err(ParseError { line: 1, message });
        }
        let (header, mut warnings) = Attrs::parse(&words.collect::<Vec<_>>().join(" "));
        let mut warnings: Vec<ParseError> = warnings.drain(..).map(|message| ParseError { line: 1, message }).collect();
        let (preamble, cells, more) = read_cells(&lines[1..], 2)?;
        warnings.extend(more);
        let mut nb = Notebook { header, crlf, ..Notebook::default() };
        if preamble.iter().all(|l| is_blank(l)) {
            nb.gap = preamble.len();
        } else {
            // source before the first marker: a code cell (printing adds its marker)
            let (lines, gap) = split_gap(preamble);
            nb.cells.push(Cell { kind: Kind::Input, attrs: Attrs::new(), lines, outputs: vec![], gap });
        }
        nb.cells.extend(cells);
        Ok((nb, warnings))
    }

    /// The text of the notebook. For a text in the canonical layout, `parse` then `print` gives it back unchanged.
    pub fn print(&self) -> String {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::new();
        let mut line = |s: &str| {
            out.push_str(s);
            out.push_str(nl);
        };
        line(&format!("{HEADER}{}", self.header));
        (0..self.gap).for_each(|_| line(""));
        for cell in &self.cells {
            cell.print_into(&mut line);
        }
        out
    }

    /// The 1-based line in the printed notebook of each cell's first body line (the line after its marker).
    pub fn first_lines(&self) -> Vec<u32> {
        let mut at = 1 + self.gap;
        self.cells
            .iter()
            .map(|c| {
                let first = at + 2;
                at += c.height();
                first as u32
            })
            .collect()
    }
}

/// The cells as a fragment: the line [`FRAGMENT`], then the cells exactly as a notebook writes them (LF line ends).
pub fn print_cells(cells: &[Cell]) -> String {
    let mut out = String::new();
    let mut line = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    line(FRAGMENT);
    cells.iter().for_each(|c| c.print_into(&mut line));
    out
}

/// The cells of a fragment written by [`print_cells`], or `None` when `text` is not one.
pub fn parse_cells(text: &str) -> Option<Vec<Cell>> {
    let (lines, _) = lines_of(text);
    if lines.first().map(|l| l.trim_end()) != Some(FRAGMENT) {
        return None;
    }
    let (preamble, cells, _) = read_cells(&lines[1..], 2).ok()?;
    preamble.iter().all(|l| is_blank(l)).then_some(cells)
}

/// Write `text` to `path` atomically: into a temporary file next to it, which then replaces `path`, so that a
/// reader sees the old or the new file, never a part. Nothing is written when the file already holds `text`.
pub fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    if std::fs::read(path).is_ok_and(|old| old == text.as_bytes()) {
        return Ok(());
    }
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not a file path"))?;
    let tmp = path.with_file_name(format!(".{}.{}.tmp", name.to_string_lossy(), std::process::id()));
    let written = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    match written.and_then(|()| std::fs::rename(&tmp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests;
