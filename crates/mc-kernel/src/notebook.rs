//! The `.mcn` notebook format (specified in the crate documentation): [`Notebook::parse`], [`Notebook::print`],
//! the runner [`run`] that evaluates the code cells in one session and can rewrite their output blocks, and
//! [`write_atomic`] for writing the result back.
use crate::cell::{run_cell, CellEvent, Status};
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, Origin};
use std::fmt;
use std::io::{self, Write};
use std::path::Path;

/// The first line of a notebook (compared word by word, so extra spaces are accepted).
pub const HEADER: &str = "#! mathcas 2 notebook";

/// The marker line before a code cell.
pub const CELL: &str = "#% cell";
/// The marker line before a text cell.
pub const TEXT: &str = "#% text";
/// The marker line before the output block of the code cell above it.
pub const OUTPUT: &str = "#% output";

/// A notebook: cells in order, plus the layout details that make printing give back the text that was parsed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notebook {
    /// blank lines between the header and the first cell
    pub gap: usize,
    /// the cells, in order
    pub cells: Vec<Cell>,
    /// whether lines end with CR LF (as the parsed text did); printing uses the same line ends
    pub crlf: bool,
}

/// One cell and the blank lines after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    /// what the cell holds
    pub kind: CellKind,
    /// blank lines after the cell (before the next marker or the end of the file)
    pub gap: usize,
}

/// The two kinds of cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CellKind {
    /// MathCAS source, and the output block of its last run (if it has one)
    Code {
        /// the source lines (without line ends)
        source: Vec<String>,
        /// the output block: `None` when there is none
        output: Option<Vec<OutputLine>>,
    },
    /// Prose: the lines as they are in the file, each normally starting with `# ` (see [`Cell::text`])
    Text {
        /// the raw lines
        lines: Vec<String>,
    },
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    /// `#|`: a line of `print` output
    Print,
    /// `#=`: a line of the value of the cell's last statement
    Value,
    /// `#!`: a line of a failure, an error, or the reason the run stopped (`interrupted`, a limit)
    Message,
}

impl OutputKind {
    /// The prefix of lines of this kind.
    pub fn prefix(self) -> &'static str {
        match self {
            OutputKind::Print => "#|",
            OutputKind::Value => "#=",
            OutputKind::Message => "#!",
        }
    }

    fn of(line: &str) -> Option<(OutputKind, &str)> {
        [OutputKind::Print, OutputKind::Value, OutputKind::Message].into_iter().find_map(|k| {
            let rest = line.strip_prefix(k.prefix())?;
            Some((k, rest.strip_prefix(' ').unwrap_or(rest)))
        })
    }
}

/// Why a text is not a notebook.
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

/// The marker lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Marker {
    Cell,
    Text,
    Output,
}

/// The marker on `line`, if it is one: exactly `#% cell`, `#% text` or `#% output` (trailing spaces allowed).
/// Any other line starting with `#%` is an ordinary comment.
fn marker(line: &str) -> Option<Marker> {
    match line.trim_end() {
        CELL => Some(Marker::Cell),
        TEXT => Some(Marker::Text),
        OUTPUT => Some(Marker::Output),
        _ => None,
    }
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

impl Cell {
    /// A code cell with this source and no output.
    pub fn code(source: &str) -> Cell {
        let source = source.lines().map(str::to_string).collect();
        Cell { kind: CellKind::Code { source, output: None }, gap: 1 }
    }

    /// A text cell with this prose: every line gets the comment prefix `# ` (an empty line becomes `#`).
    pub fn text(text: &str) -> Cell {
        let lines = text.lines().map(|l| if l.is_empty() { "#".to_string() } else { format!("# {l}") }).collect();
        Cell { kind: CellKind::Text { lines }, gap: 1 }
    }

    /// The source of a code cell as one text (lines joined with `\n`), or `None` for a text cell.
    pub fn source(&self) -> Option<String> {
        match &self.kind {
            CellKind::Code { source, .. } => Some(source.join("\n")),
            CellKind::Text { .. } => None,
        }
    }

    /// The prose of a text cell, its lines without the comment prefix (`# ` or `#`), or `None` for a code cell.
    pub fn prose(&self) -> Option<String> {
        match &self.kind {
            CellKind::Text { lines } => {
                let strip = |l: &String| l.strip_prefix("# ").or_else(|| l.strip_prefix('#')).unwrap_or(l).to_string();
                Some(lines.iter().map(strip).collect::<Vec<_>>().join("\n"))
            }
            CellKind::Code { .. } => None,
        }
    }

    /// The output block of a code cell.
    pub fn output(&self) -> Option<&[OutputLine]> {
        match &self.kind {
            CellKind::Code { output, .. } => output.as_deref(),
            CellKind::Text { .. } => None,
        }
    }

    /// The number of lines the cell takes in the file, gap included.
    fn height(&self) -> usize {
        1 + self.gap
            + match &self.kind {
                CellKind::Code { source, output } => source.len() + output.as_ref().map_or(0, |o| 1 + o.len()),
                CellKind::Text { lines } => lines.len(),
            }
    }
}

impl Notebook {
    /// An empty notebook.
    pub fn new() -> Notebook {
        Notebook { gap: 1, ..Notebook::default() }
    }

    /// Read a notebook. A byte order mark is skipped; lines may end with LF or CR LF.
    pub fn parse(text: &str) -> Result<Notebook, ParseError> {
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let crlf = text.contains("\r\n");
        let body = text.strip_suffix('\n').unwrap_or(text);
        let lines: Vec<&str> = body.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
        let header = lines.first().copied().unwrap_or("");
        if !header.split_whitespace().eq(HEADER.split_whitespace()) {
            let message = format!("a notebook starts with the line `{HEADER}`");
            return Err(ParseError { line: 1, message });
        }
        // the blocks between markers: (marker, 1-based line of the marker, lines after it)
        let mut preamble = vec![];
        let mut blocks: Vec<(Marker, usize, Vec<String>)> = vec![];
        for (i, &line) in lines.iter().enumerate().skip(1) {
            match (marker(line), blocks.last_mut()) {
                (Some(m), _) => blocks.push((m, i + 1, vec![])),
                (None, Some(b)) => b.2.push(line.to_string()),
                (None, None) => preamble.push(line.to_string()),
            }
        }
        let mut nb = Notebook { crlf, ..Notebook::default() };
        if preamble.iter().all(|l| is_blank(l)) {
            nb.gap = preamble.len();
        } else {
            // source before the first marker: a code cell (printing adds its marker)
            let (source, gap) = split_gap(preamble);
            nb.cells.push(Cell { kind: CellKind::Code { source, output: None }, gap });
        }
        let mut blocks = blocks.into_iter().peekable();
        while let Some((m, at, lines)) = blocks.next() {
            let cell = match m {
                Marker::Text => {
                    let (lines, gap) = split_gap(lines);
                    Cell { kind: CellKind::Text { lines }, gap }
                }
                Marker::Cell if blocks.peek().is_some_and(|b| b.0 == Marker::Output) => {
                    let (_, out_at, out) = blocks.next().expect("peeked");
                    let (out, gap) = split_gap(out);
                    let output = out
                        .iter()
                        .enumerate()
                        .map(|(k, l)| match OutputKind::of(l) {
                            Some((kind, text)) => Ok(OutputLine { kind, text: text.to_string() }),
                            None => Err(ParseError {
                                line: out_at + 1 + k,
                                message: "an output block holds only lines starting with `#|`, `#=` or `#!` \
                                          (start a new cell with `#% cell`)"
                                    .into(),
                            }),
                        })
                        .collect::<Result<_, _>>()?;
                    // the source keeps blank lines before the output marker, so the text prints back as it was
                    Cell { kind: CellKind::Code { source: lines, output: Some(output) }, gap }
                }
                Marker::Cell => {
                    let (source, gap) = split_gap(lines);
                    Cell { kind: CellKind::Code { source, output: None }, gap }
                }
                Marker::Output => {
                    let message = "an output block follows a code cell".into();
                    return Err(ParseError { line: at, message });
                }
            };
            nb.cells.push(cell);
        }
        Ok(nb)
    }

    /// The text of the notebook. For a text in the canonical layout (every cell behind its marker, output lines
    /// written as the runner writes them), `parse` then `print` gives the text back unchanged.
    pub fn print(&self) -> String {
        let nl = if self.crlf { "\r\n" } else { "\n" };
        let mut out = String::new();
        let mut line = |s: &str| {
            out.push_str(s);
            out.push_str(nl);
        };
        line(HEADER);
        (0..self.gap).for_each(|_| line(""));
        for cell in &self.cells {
            match &cell.kind {
                CellKind::Code { source, output } => {
                    line(CELL);
                    source.iter().for_each(|l| line(l));
                    if let Some(output) = output {
                        line(OUTPUT);
                        for o in output {
                            let prefix = o.kind.prefix();
                            if o.text.is_empty() {
                                line(prefix);
                            } else {
                                line(&format!("{prefix} {}", o.text));
                            }
                        }
                    }
                }
                CellKind::Text { lines } => {
                    line(TEXT);
                    lines.iter().for_each(|l| line(l));
                }
            }
            (0..cell.gap).for_each(|_| line(""));
        }
        out
    }

    /// The 1-based line in the printed notebook of each cell's first source line (the line after its marker).
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

/// The output block of a run: its events, then a line for a stop.
fn output_block(events: &[CellEvent], status: Status) -> Option<Vec<OutputLine>> {
    let mut out = vec![];
    let mut push = |kind, text: &str| out.extend(text.split('\n').map(|t| OutputLine { kind, text: t.to_string() }));
    for e in events {
        match e {
            CellEvent::Output(s) => push(OutputKind::Print, s),
            CellEvent::Value(s) => push(OutputKind::Value, s),
            CellEvent::Failure(s) => push(OutputKind::Message, s),
            CellEvent::Error(r) => push(OutputKind::Message, &r.rendered),
        }
    }
    if status == Status::Interrupted {
        push(OutputKind::Message, "interrupted");
    }
    (!out.is_empty()).then_some(out)
}

/// Evaluate the code cells of `nb` in order in `engine`, every cell from `origin` (the notebook file: imports are
/// looked up next to it) with its errors at their lines in the notebook, handing each cell's events to `show` as
/// they happen. Returns how the run ended: the status of the cell where it stopped (`error`, `interrupted`,
/// `limit`), else `failure` if some cell had a failure, else `ok`.
///
/// With `rewrite`, every cell that ran gets the output block of this run (none if it showed nothing), and the
/// cells after a stop lose theirs, so the notebook records one run. Error positions are those of the notebook as
/// rewritten, which is the file [`write_atomic`] then writes; without `rewrite` the notebook is left as it is and
/// positions are those of the file as read.
pub fn run(
    engine: &mut CoreEngine,
    nb: &mut Notebook,
    origin: &Origin,
    stop: &StopFlag,
    rewrite: bool,
    show: &mut dyn FnMut(&CellEvent),
) -> Status {
    let mut overall = Status::Ok;
    for i in 0..nb.cells.len() {
        let Some(src) = nb.cells[i].source() else { continue };
        // earlier cells' output blocks are final by now, so this is the cell's line in the file to be written
        let line = nb.first_lines()[i];
        let mut events = vec![];
        let status = run_cell(engine, &src, origin, line, stop, &mut |e| {
            show(&e);
            events.push(e);
        });
        if rewrite {
            if let CellKind::Code { output, .. } = &mut nb.cells[i].kind {
                *output = output_block(&events, status);
            }
        }
        match status {
            Status::Ok => {}
            Status::Failure => overall = Status::Failure,
            Status::Error | Status::Interrupted | Status::Limit => {
                if rewrite {
                    for c in &mut nb.cells[i + 1..] {
                        if let CellKind::Code { output, .. } = &mut c.kind {
                            *output = None;
                        }
                    }
                }
                return status;
            }
        }
    }
    overall
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
mod tests {
    use super::*;

    const SAMPLE: &str = "#! mathcas 2 notebook\n\n#% text\n# Title\n#\n# More prose.\n\n#% cell\nx = 1\nprint(x)\n\
                          #% output\n#| 1\n#= 1\n\n#% cell\n# a comment\n#% not a marker\ny = 2\n\n\n#% cell\n1/0\n\
                          #% output\n#! failure(division_by_zero)\n#|\n";

    #[test]
    fn round_trip() {
        let nb = Notebook::parse(SAMPLE).unwrap();
        assert_eq!(nb.print(), SAMPLE);
        assert_eq!(nb.cells.len(), 4);
        assert_eq!(nb.cells[0].prose().unwrap(), "Title\n\nMore prose.");
        assert_eq!(nb.cells[1].source().unwrap(), "x = 1\nprint(x)");
        let out = nb.cells[1].output().unwrap();
        assert_eq!(out[0], OutputLine { kind: OutputKind::Print, text: "1".into() });
        assert_eq!(out[1].kind, OutputKind::Value);
        assert_eq!(nb.cells[2].source().unwrap(), "# a comment\n#% not a marker\ny = 2");
        assert_eq!((nb.cells[2].gap, nb.cells[2].output()), (2, None));
        assert_eq!(nb.cells[3].output().unwrap()[1], OutputLine { kind: OutputKind::Print, text: "".into() });
        assert_eq!(nb.first_lines(), [4, 9, 16, 22]);
        // CR LF line ends are kept
        let crlf = SAMPLE.replace('\n', "\r\n");
        let nb2 = Notebook::parse(&crlf).unwrap();
        assert!(nb2.crlf);
        assert_eq!(nb2.print(), crlf);
        assert_eq!(nb2.cells, nb.cells);
    }

    #[test]
    fn built_notebooks_print_and_parse_back() {
        let mut nb = Notebook::new();
        nb.cells.push(Cell::text("Hello\n\nworld"));
        nb.cells.push(Cell::code("1 + 1\n"));
        nb.cells.push(Cell::code("2"));
        if let CellKind::Code { output, .. } = &mut nb.cells[1].kind {
            *output = Some(vec![OutputLine { kind: OutputKind::Value, text: "2".into() }]);
        }
        let text = nb.print();
        assert_eq!(
            text,
            "#! mathcas 2 notebook\n\n#% text\n# Hello\n#\n# world\n\n#% cell\n1 + 1\n#% output\n#= 2\n\n#% cell\n2\n\n"
        );
        assert_eq!(Notebook::parse(&text).unwrap(), nb);
    }

    #[test]
    fn layouts_that_print_differently() {
        // source before the first marker becomes a code cell; a missing final line end is added
        let nb = Notebook::parse("#! mathcas 2 notebook\n1 + 1\n\n#% cell\n2").unwrap();
        assert_eq!(nb.cells.len(), 2);
        assert_eq!(nb.print(), "#! mathcas 2 notebook\n#% cell\n1 + 1\n\n#% cell\n2\n");
        // `#|x` reads as `#| x`
        let nb = Notebook::parse("#!  mathcas  2  notebook\n#% cell\n1\n#% output\n#=1\n").unwrap();
        assert_eq!(nb.print(), "#! mathcas 2 notebook\n#% cell\n1\n#% output\n#= 1\n");
        assert_eq!(Notebook::parse("\u{feff}#! mathcas 2 notebook\n").unwrap(), Notebook::default());
    }

    #[test]
    fn errors() {
        let e = |t: &str| Notebook::parse(t).unwrap_err();
        assert_eq!(e("").line, 1);
        assert_eq!(e("#! mathcas 2\n#% cell\n1\n").line, 1);
        assert!(e("x").to_string().starts_with("line 1: a notebook starts with"));
        let err = e("#! mathcas 2 notebook\n#% cell\n1\n#% output\n#= 1\n2\n");
        assert_eq!(err.line, 6);
        assert!(err.message.contains("start a new cell"), "{err}");
        assert_eq!(e("#! mathcas 2 notebook\n#% text\n# t\n#% output\n").line, 4);
        assert_eq!(e("#! mathcas 2 notebook\n#% cell\n#% output\n#% output\n").line, 4);
    }

    #[test]
    fn output_blocks() {
        let events =
            [CellEvent::Output("a\nb".into()), CellEvent::Failure("failure(x)".into()), CellEvent::Value("3".into())];
        let block = output_block(&events, Status::Failure).unwrap();
        let kinds: Vec<_> = block.iter().map(|l| (l.kind, l.text.as_str())).collect();
        use OutputKind::*;
        assert_eq!(kinds, [(Print, "a"), (Print, "b"), (Message, "failure(x)"), (Value, "3")]);
        assert_eq!(output_block(&[], Status::Ok), None);
        assert_eq!(output_block(&[], Status::Interrupted).unwrap()[0].text, "interrupted");
    }

    #[test]
    fn atomic_writes() {
        let dir = std::env::temp_dir().join(format!("mc-kernel-atomic-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.mcn");
        write_atomic(&path, "one\n").unwrap();
        write_atomic(&path, "two\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "two\n");
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["n.mcn"], "no temporary file is left");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
