//! A function page read from its notebook (the format in the crate documentation), with the problems found.
use crate::inline::{spans, Span};
use mc_nbfile::{Kind, Notebook, OutputKind, OutputLine};
use std::fmt;

/// The example groups a page may have, in the order they must appear ("Basic examples" first and required).
pub const GROUPS: [&str; 6] =
    ["Basic examples", "Scope", "Options", "Applications", "Properties and relations", "Possible issues"];

/// One signature of the usage block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    /// the call (the leading code span)
    pub call: String,
    /// the rest of the cell: what the call gives (may hold code spans)
    pub text: String,
}

/// One fact of the Details section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// the text (may hold code spans)
    pub text: String,
    /// 0 for an `item`, 1 for a `subitem`
    pub level: u8,
}

/// A cell of an example group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    /// prose (may hold code spans)
    Text(String),
    /// an input and its stored output lines
    Input {
        /// the source
        source: String,
        /// the output lines of every block, in order
        output: Vec<OutputLine>,
        /// the cell's position in the notebook
        cell: usize,
    },
}

/// An example group ("Basic examples", "Scope", ...).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    /// the group's name
    pub title: String,
    /// its cells
    pub blocks: Vec<Block>,
}

impl Group {
    /// The number of inputs.
    pub fn inputs(&self) -> usize {
        self.blocks.iter().filter(|b| matches!(b, Block::Input { .. })).count()
    }
}

/// A function page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// the function's name (the title)
    pub name: String,
    /// the signatures
    pub usage: Vec<Usage>,
    /// the Details items
    pub details: Vec<Item>,
    /// the example groups
    pub examples: Vec<Group>,
    /// the functions under See also
    pub see_also: Vec<String>,
}

/// A problem in a page: the 1-based line of the cell it is about (0 for the whole file) and what is wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// the line of the cell's marker
    pub line: usize,
    /// what is wrong
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

/// The part of the page being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Start,
    Usage,
    Details,
    Examples,
    SeeAlso,
    End,
}

/// The image an output line shows: the path in `image("path")`.
pub fn image_path(line: &OutputLine) -> Option<&str> {
    if line.kind != OutputKind::Value {
        return None;
    }
    let inner = line.text.trim().strip_prefix("image(\"")?.strip_suffix("\")")?;
    (!inner.contains('"')).then_some(inner)
}

impl Page {
    /// Read the page `name` from its notebook text: the page as far as it could be read, and every problem.
    pub fn read(name: &str, text: &str) -> (Page, Vec<Problem>) {
        match Notebook::parse(text) {
            Ok(nb) => Page::from_notebook(name, &nb),
            Err(e) => {
                (Page { name: name.to_string(), ..Page::default() }, vec![Problem { line: e.line, message: e.message }])
            }
        }
    }

    /// Read the page `name` from a parsed notebook.
    pub fn from_notebook(name: &str, nb: &Notebook) -> (Page, Vec<Problem>) {
        let mut page = Page { name: name.to_string(), ..Page::default() };
        let mut problems = vec![];
        // the marker line of each cell is the line before its first body line
        let lines: Vec<usize> = nb.first_lines().iter().map(|&l| l as usize - 1).collect();
        let mut part = Part::Start;
        for (i, cell) in nb.cells.iter().enumerate() {
            let line = lines[i];
            let mut problem = |message: String| problems.push(Problem { line, message });
            let content = cell.content();
            let text = content.trim().to_string();
            if !cell.is_code() {
                if let Err(at) = spans(&text) {
                    problem(format!("the backtick at character {} opens a code span that is not closed", at + 1));
                }
            }
            match (part, cell.kind) {
                (_, Kind::Section) => {
                    let next = match text.as_str() {
                        "Details" => Part::Details,
                        "Examples" => Part::Examples,
                        "See also" => Part::SeeAlso,
                        other => {
                            problem(format!("unknown section `{other}` (Details, Examples, See also)"));
                            continue;
                        }
                    };
                    let order = [Part::Usage, Part::Details, Part::Examples, Part::SeeAlso];
                    let rank = |p: Part| order.iter().position(|&q| q == p).unwrap_or(0);
                    if part == Part::Start || part == Part::End || rank(next) <= rank(part) {
                        problem(format!(
                            "section `{text}` out of order (Details, Examples, See also follow the usage)"
                        ));
                    }
                    if part == Part::Usage && page.usage.is_empty() {
                        problem("the usage block is empty".into());
                    }
                    if part == Part::Details && page.details.is_empty() {
                        problem("the Details section has no items".into());
                    }
                    part = next;
                }
                (Part::Start, Kind::Title) => {
                    if text != name {
                        problem(format!("the title `{text}` is not the page's name `{name}`"));
                    }
                    part = Part::Usage;
                }
                (Part::Start, _) => {
                    problem("a page starts with a title cell naming the function".into());
                    part = Part::Usage;
                }
                (Part::Usage, Kind::Text) => match usage(name, &text) {
                    Ok(u) => page.usage.push(u),
                    Err(m) => problem(m),
                },
                (Part::Details, Kind::Item | Kind::Subitem) => {
                    let level = u8::from(cell.kind == Kind::Subitem);
                    if text.is_empty() {
                        problem("an empty item".into());
                    }
                    page.details.push(Item { text, level });
                }
                (Part::Examples, Kind::Subsection) => {
                    let Some(rank) = GROUPS.iter().position(|g| *g == text) else {
                        problem(format!("unknown example group `{text}` ({})", GROUPS.join(", ")));
                        page.examples.push(Group { title: text, blocks: vec![] });
                        continue;
                    };
                    let last = page.examples.last().and_then(|g| GROUPS.iter().position(|x| *x == g.title));
                    if page.examples.is_empty() && rank != 0 {
                        problem("the examples start with the group `Basic examples`".into());
                    } else if last.is_some_and(|l| rank <= l) {
                        problem(format!("the group `{text}` is out of order or repeated ({})", GROUPS.join(", ")));
                    }
                    page.examples.push(Group { title: text, blocks: vec![] });
                }
                (Part::Examples, Kind::Text | Kind::Input) => {
                    let Some(group) = page.examples.last_mut() else {
                        problem("an example cell before the first group (`Basic examples`)".into());
                        continue;
                    };
                    if cell.is_code() {
                        let output: Vec<OutputLine> = cell.output_lines().cloned().collect();
                        for o in &output {
                            if let Some(p) = image_path(o) {
                                if let Err(m) = image_ok(name, p) {
                                    problem(m);
                                }
                            }
                        }
                        group.blocks.push(Block::Input { source: content, output, cell: i });
                    } else {
                        group.blocks.push(Block::Text(text));
                    }
                }
                (Part::SeeAlso, Kind::Text) => {
                    match see_also(&text) {
                        Ok(names) => page.see_also = names,
                        Err(m) => problem(m),
                    }
                    part = Part::End;
                }
                (_, k) => problem(format!("a `{}` cell does not belong here ({})", k.name(), expected(part))),
            }
        }
        match part {
            Part::Start => problems.push(Problem { line: 0, message: "the page is empty".into() }),
            Part::End => {}
            _ => problems.push(Problem { line: 0, message: "the page does not end with a See also section".into() }),
        }
        if !page.examples.iter().any(|g| g.title == GROUPS[0]) && part != Part::Start {
            problems.push(Problem { line: 0, message: "the page has no `Basic examples` group".into() });
        }
        for g in &page.examples {
            if g.inputs() == 0 {
                problems.push(Problem { line: 0, message: format!("the group `{}` has no input", g.title) });
            }
        }
        (page, problems)
    }

    /// Every input of the page with its group, in order.
    pub fn inputs(&self) -> impl Iterator<Item = (&Group, &Block)> {
        self.examples
            .iter()
            .flat_map(|g| g.blocks.iter().filter(|b| matches!(b, Block::Input { .. })).map(move |b| (g, b)))
    }

    /// The prose of the page (usage, details, example texts) for searching.
    pub fn words(&self) -> String {
        let mut out = String::new();
        for u in &self.usage {
            out.push_str(&u.call);
            out.push(' ');
            out.push_str(&crate::inline::plain(&u.text));
            out.push('\n');
        }
        for d in &self.details {
            out.push_str(&crate::inline::plain(&d.text));
            out.push('\n');
        }
        out
    }
}

/// What may come next in a part, for messages.
fn expected(part: Part) -> &'static str {
    match part {
        Part::Start => "a page starts with its title",
        Part::Usage => "the usage block holds text cells, one per signature",
        Part::Details => "Details holds item cells",
        Part::Examples => "Examples holds subsections with text and input cells",
        Part::SeeAlso => "See also holds one text cell",
        Part::End => "nothing follows See also",
    }
}

/// A usage cell: a leading code span calling `name`, then text.
fn usage(name: &str, text: &str) -> Result<Usage, String> {
    let parts = spans(text).map_err(|_| "a usage cell with an unclosed code span".to_string())?;
    let (Some(Span::Code(call)), rest) = (parts.first(), &parts[parts.len().min(1)..]) else {
        return Err("a usage cell starts with the call in backticks".into());
    };
    let calls_name = call.strip_prefix(name).is_some_and(|r| r.is_empty() || r.starts_with('('));
    if !calls_name {
        return Err(format!("the usage `{call}` does not call `{name}`"));
    }
    let after: String = rest
        .iter()
        .map(|s| match s {
            Span::Text(t) => t.to_string(),
            Span::Code(c) => format!("`{c}`"),
        })
        .collect();
    let after = after.trim().to_string();
    if after.is_empty() {
        return Err(format!("the usage `{call}` says nothing about what it gives"));
    }
    Ok(Usage { call: call.to_string(), text: after })
}

/// The See also cell: code spans, each a name, separated by commas or spaces.
fn see_also(text: &str) -> Result<Vec<String>, String> {
    let bad = || "See also lists names as code spans separated by commas: `a`, `b`".to_string();
    let parts = spans(text).map_err(|_| bad())?;
    let mut names = vec![];
    for p in parts {
        match p {
            Span::Code(name) if !name.is_empty() && !name.contains(char::is_whitespace) => names.push(name.to_string()),
            Span::Text(t) if t.chars().all(|c| c == ',' || c.is_whitespace()) => {}
            _ => return Err(bad()),
        }
    }
    if names.is_empty() {
        return Err(bad());
    }
    Ok(names)
}

/// An image path of page `name` is relative and inside the page's `.files` folder.
fn image_ok(name: &str, path: &str) -> Result<(), String> {
    let folder = format!("{name}.files/");
    let rest = path.strip_prefix(&folder).ok_or_else(|| format!("the image `{path}` is not in `{folder}`"))?;
    if rest.is_empty() || rest.contains(['/', '\\', ':']) || rest.starts_with('.') || !rest.ends_with(".png") {
        return Err(format!("the image `{path}` is not a PNG file directly in `{folder}`"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "#! mathcas 2 notebook

#% title
# sq

#% text
# `sq(x)` gives the square of `x`.

#% section
# Details

#% item
# Works on numbers.

#% section
# Examples

#% subsection
# Basic examples

#% text
# Square a number:

#% cell
sq(3)
#% output
#= 9

#% subsection
# Scope

#% cell
plot(x, [x, 0, 1])
#% output
#= image(\"sq.files/sq-2.png\")

#% section
# See also

#% text
# `pow`, `sqrt`
";

    #[test]
    fn a_good_page() {
        let (p, problems) = Page::read("sq", GOOD);
        assert_eq!(problems, vec![]);
        assert_eq!(p.usage, vec![Usage { call: "sq(x)".into(), text: "gives the square of `x`.".into() }]);
        assert_eq!(p.details.len(), 1);
        assert_eq!(p.examples.iter().map(|g| g.title.as_str()).collect::<Vec<_>>(), ["Basic examples", "Scope"]);
        assert_eq!(p.examples[0].inputs(), 1);
        assert_eq!(p.see_also, ["pow", "sqrt"]);
        assert_eq!(p.inputs().count(), 2);
    }

    fn problems(text: &str) -> Vec<String> {
        Page::read("sq", text).1.into_iter().map(|p| p.message).collect()
    }

    fn has(text: &str, needle: &str) {
        let ps = problems(text);
        assert!(ps.iter().any(|p| p.contains(needle)), "expected `{needle}` in {ps:?}");
    }

    #[test]
    fn structure_problems() {
        has(&GOOD.replace("# sq\n", "# sqr\n"), "is not the page's name");
        has(&GOOD.replace("`sq(x)` gives", "sq(x) gives"), "starts with the call");
        has(&GOOD.replace("`sq(x)` gives the square of `x`.", "`sq(x)`"), "says nothing");
        has(&GOOD.replace("`sq(x)` gives", "`cube(x)` gives"), "does not call `sq`");
        has(&GOOD.replace("# Scope", "# Extras"), "unknown example group");
        has(&GOOD.replace("# Basic examples", "# Scope"), "start with the group `Basic examples`");
        has(&GOOD.replace("# Scope", "# Basic examples"), "out of order or repeated");
        has(&GOOD.replace("#% item", "#% text"), "does not belong here");
        has(&GOOD.replace("# `pow`, `sqrt`", "# pow, sqrt"), "code spans separated by commas");
        has(&GOOD.replace("# `pow`, `sqrt`", "# `pow` and `sqrt`"), "code spans separated by commas");
        has(&GOOD.replace("# `pow`, `sqrt`", "# ``"), "code spans separated by commas");
        assert_eq!(Page::read("sq", &GOOD.replace("`pow`, `sqrt`", "`pow` `sqrt`")).0.see_also, ["pow", "sqrt"]);
        has(&GOOD.replace("sq.files/sq-2.png", "C:/tmp/a.png"), "is not in `sq.files/`");
        has(&GOOD.replace("sq.files/sq-2.png", "sq.files/../a.png"), "not a PNG file directly");
        has(&GOOD.replace("of `x`.", "of `x."), "not closed");
        has(&GOOD.replace("# Details", "# Notes"), "unknown section");
        let no_see_also = &GOOD[..GOOD.find("#% section\n# See also").unwrap()];
        has(no_see_also, "does not end with a See also section");
        has(
            &GOOD.replace("#% cell\nplot(x, [x, 0, 1])\n#% output\n#= image(\"sq.files/sq-2.png\")\n", ""),
            "has no input",
        );
        has("#! mathcas 2 notebook\n", "empty");
        has("not a notebook", "starts with the line");
        has(&format!("{GOOD}\n#% text\n# more\n"), "nothing follows See also");
    }

    #[test]
    fn images() {
        let line = |t: &str| OutputLine { kind: OutputKind::Value, text: t.into() };
        assert_eq!(image_path(&line("image(\"a.files/b.png\")")), Some("a.files/b.png"));
        assert_eq!(image_path(&line("3")), None);
        assert_eq!(image_path(&OutputLine { kind: OutputKind::Print, text: "image(\"a\")".into() }), None);
    }
}
