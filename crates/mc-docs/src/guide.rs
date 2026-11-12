//! Topic guides (round 21): short notebooks in `docs/guides/<NN>-<slug>.mcn`, one per topic, read from the same
//! notebook format as the function pages but with a free body.
//!
//! The cells come in this order:
//!
//! 1. one `title` cell, then at least one `text` cell (what the topic covers);
//! 2. `section` cells with `subsection` cells below them, holding `text`, `item` and `subitem` cells (prose with the
//!    one inline markup of the pages, code spans in backticks) and input cells (`cell`) whose outputs are written
//!    by the engine: `mc-docs-pages update --guides [stem ...]`; inputs run in order in one session per guide, so a
//!    later input may use what an earlier one defined;
//! 3. the section "Common mistakes" (somewhere);
//! 4. the last section "Related functions", holding one `text` cell of code spans, the functions whose pages the
//!    guide points to (each must exist).
//!
//! The file's stem is `<NN>-<slug>` (the number orders the guides); the window opens a guide by its slug
//! (`mc-docs topic:<slug>`). Images an output shows (drawings, `.svgz`; raster pictures, `.png`) are in `<stem>.files/` beside the guide, as for pages.
use crate::inline::spans;
use crate::page::{image_path, Problem};
use mc_nbfile::{Kind, Notebook, OutputLine};

/// The sections every guide has.
pub const MISTAKES: &str = "Common mistakes";
/// The last section of every guide: the functions it points to.
pub const RELATED: &str = "Related functions";
/// The fewest example inputs of a guide.
pub const MIN_INPUTS: usize = 6;

/// One cell of a guide's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Part {
    /// a section (1) or subsection (2) heading
    Heading(u8, String),
    /// prose (may hold code spans)
    Text(String),
    /// a bullet (level 0) or sub-bullet (1)
    Item(u8, String),
    /// an input and its stored outputs
    Input {
        /// the source
        source: String,
        /// the output lines
        output: Vec<OutputLine>,
    },
}

/// A topic guide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Guide {
    /// the file's stem, `<NN>-<slug>`
    pub stem: String,
    /// the name the window uses, the stem without its number
    pub slug: String,
    /// the title
    pub title: String,
    /// the body after the title
    pub parts: Vec<Part>,
    /// the functions of "Related functions"
    pub related: Vec<String>,
}

/// The slug of a guide's stem: the stem without its leading `NN-`.
pub fn slug_of(stem: &str) -> &str {
    match stem.split_once('-') {
        Some((n, rest)) if !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => stem,
    }
}

impl Guide {
    /// Read guide `stem` from its notebook text: the guide as far as it could be read, and every problem
    /// (`known` says whether a name is a function of the catalogue).
    pub fn read(stem: &str, text: &str, known: &dyn Fn(&str) -> bool) -> (Guide, Vec<Problem>) {
        let mut guide = Guide { stem: stem.to_string(), slug: slug_of(stem).to_string(), ..Guide::default() };
        let nb = match Notebook::parse(text) {
            Ok(nb) => nb,
            Err(e) => return (guide, vec![Problem { line: e.line, message: e.message }]),
        };
        let lines: Vec<usize> = nb.first_lines().iter().map(|&l| l as usize - 1).collect();
        let mut problems = vec![];
        let mut intro = false;
        let mut section = String::new();
        for (i, cell) in nb.cells.iter().enumerate() {
            let line = lines[i];
            let mut problem = |m: String| problems.push(Problem { line, message: m });
            let content = cell.content();
            let t = content.trim().to_string();
            if i == 0 {
                if cell.kind != Kind::Title {
                    problem("a guide starts with a title cell".into());
                } else {
                    guide.title = t;
                    continue;
                }
            }
            if !cell.is_code() {
                if let Err(at) = spans(&t) {
                    problem(format!("the backtick at character {} opens a code span that is not closed", at + 1));
                }
            }
            match cell.kind {
                Kind::Title => problem("a second title cell".into()),
                Kind::Section => {
                    section = t.clone();
                    guide.parts.push(Part::Heading(1, t));
                }
                Kind::Subsection => guide.parts.push(Part::Heading(2, t)),
                Kind::Text if section == RELATED => match crate::page::see_also(&t) {
                    Ok(names) => {
                        for n in &names {
                            if !known(n) {
                                problem(format!("{RELATED} names `{n}`, which is not a function"));
                            }
                        }
                        guide.related.extend(names);
                    }
                    Err(m) => problem(m),
                },
                Kind::Text => {
                    intro |= section.is_empty();
                    guide.parts.push(Part::Text(t));
                }
                Kind::Item | Kind::Subitem => {
                    if section == RELATED {
                        problem(format!("{RELATED} holds one text cell of code spans"));
                    }
                    guide.parts.push(Part::Item(u8::from(cell.kind == Kind::Subitem), t));
                }
                Kind::Input => {
                    let output: Vec<OutputLine> = cell.output_lines().cloned().collect();
                    for o in &output {
                        if let Some(p) = image_path(o) {
                            let folder = format!("{stem}.files/");
                            let ok = p.strip_prefix(&folder).is_some_and(|r| {
                                !r.is_empty() && !r.contains(['/', '\\', ':']) && !r.starts_with('.') && (r.ends_with(".svgz") || r.ends_with(".png"))
                            });
                            if !ok {
                                problem(format!("the image `{p}` is not a drawing (.svgz) or PNG file directly in `{folder}`"));
                            }
                        }
                    }
                    if section == RELATED {
                        problem(format!("{RELATED} holds one text cell of code spans"));
                    }
                    guide.parts.push(Part::Input { source: content, output });
                }
                other => problem(format!("a `{}` cell does not belong in a guide", other.name())),
            }
        }
        let mut whole = |m: String| problems.push(Problem { line: 0, message: m });
        if guide.title.is_empty() {
            whole("the guide has no title".into());
        }
        if !intro {
            whole("a text cell after the title says what the guide covers".into());
        }
        let sections: Vec<&str> = guide
            .parts
            .iter()
            .filter_map(|p| match p {
                Part::Heading(1, t) => Some(t.as_str()),
                _ => None,
            })
            .collect();
        if !sections.contains(&MISTAKES) {
            whole(format!("the guide has no section `{MISTAKES}`"));
        }
        if sections.last() != Some(&RELATED) {
            whole(format!("the last section is `{RELATED}`"));
        }
        if guide.related.len() < 3 {
            whole(format!("{RELATED} names at least 3 functions"));
        }
        let inputs = guide.inputs();
        if inputs < MIN_INPUTS {
            whole(format!("the guide has {inputs} example inputs (at least {MIN_INPUTS})"));
        }
        (guide, problems)
    }

    /// The number of example inputs.
    pub fn inputs(&self) -> usize {
        self.parts.iter().filter(|p| matches!(p, Part::Input { .. })).count()
    }

    /// The first prose after the title: what the guide covers (for the lists of guides).
    pub fn summary(&self) -> &str {
        self.parts
            .iter()
            .find_map(|p| match p {
                Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .unwrap_or("")
    }

    /// What search looks through: the title, headings, prose and example inputs (lower case; round 23).
    pub fn search_text(&self) -> crate::search::GuideText {
        let mut words = String::new();
        for p in &self.parts {
            match p {
                Part::Heading(_, t) | Part::Text(t) | Part::Item(_, t) => words.push_str(t),
                Part::Input { source, .. } => words.push_str(source),
            }
            words.push(' ');
        }
        for r in &self.related {
            words.push_str(r);
            words.push(' ');
        }
        crate::search::GuideText { title: self.title.to_lowercase(), words: words.to_lowercase() }
    }

    /// The prose and code of the guide for find in page and the tests: every example input.
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        self.parts.iter().filter_map(|p| match p {
            Part::Input { source, .. } => Some(source.as_str()),
            _ => None,
        })
    }
}

/// The guides of `lib`, in the order of their file names.
pub fn all(lib: &crate::library::Library) -> Vec<Guide> {
    let cat = crate::catalogue::Catalogue::new();
    lib.guide_stems()
        .into_iter()
        .filter_map(|stem| {
            let text = lib.guide_text(&stem)?;
            Some(Guide::read(&stem, &text, &|n| cat.get(n).is_some()).0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = "#! mathcas 2 notebook

#% title
# Topic

#% text
# What it covers, with `solve`.

#% section
# Work

#% cell
1 + 1
#% output
#= 2

#% section
# Common mistakes

#% text
# `x(x - a)` is a call.

#% section
# Related functions

#% text
# `solve`, `d`, `n`
";

    fn problems(text: &str) -> Vec<String> {
        Guide::read("01-topic", text, &|_| true).1.into_iter().map(|p| p.message).collect()
    }

    #[test]
    fn slugs_drop_the_number() {
        assert_eq!(slug_of("01-getting-started"), "getting-started");
        assert_eq!(slug_of("plain"), "plain");
        assert_eq!(slug_of("a-b"), "a-b");
    }

    #[test]
    fn a_guide_is_read_and_its_problems_reported() {
        let (g, ps) = Guide::read("01-topic", GOOD, &|_| true);
        assert_eq!(g.slug, "topic");
        assert_eq!(g.title, "Topic");
        assert_eq!(g.related, ["solve", "d", "n"]);
        assert_eq!(g.summary(), "What it covers, with `solve`.");
        // only the size rule is broken by this short example
        assert_eq!(ps.iter().map(|p| p.message.clone()).collect::<Vec<_>>().len(), 1, "{ps:?}");
        let has = |t: &str, n: &str| {
            let ps = problems(t);
            assert!(ps.iter().any(|p| p.contains(n)), "expected `{n}` in {ps:?}");
        };
        has(&GOOD.replace("# Common mistakes", "# Pitfalls"), "no section `Common mistakes`");
        has(&GOOD.replace("# Related functions", "# More"), "the last section is");
        has(&GOOD.replace("# `x(x - a)` is a call.", "# `x(x - a) is a call."), "not closed");
        has(&GOOD.replace("#% title\n# Topic", "#% text\n# Topic"), "starts with a title");
        has(&GOOD.replace("#= 2", "#= image(\"other.files/a.png\")"), "file directly in");
        assert!(Guide::read("01-topic", GOOD, &|n| n != "d").1.iter().any(|p| p.message.contains("`d`")));
    }
}
