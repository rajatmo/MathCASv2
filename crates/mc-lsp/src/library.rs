//! The names every MathCAS session knows before it reads a file: the keywords, the libraries' functions (their
//! `FUNCTIONS` lists), the evaluator's own natives and the prelude's definitions. They feed completion and hover,
//! and name resolution (a name the kernel knows stays bare inside a module, as in the engine). Every documented
//! function also has its entry of the help index (`mc_help`: usage lines from its documentation page, the same the
//! notebook and the documentation window show).
use crate::outline::{self, Item, ItemKind};
use mc_load::stdlib;
use std::collections::BTreeMap;

/// The natives of the evaluator and the engine that a program calls by name, beyond the libraries' `FUNCTIONS`
/// lists (the operators' heads, such as `add`, are written as operators and left out).
const CORE: &[(&str, &str)] = &[
    ("map", "map(f, xs): the list [f(x) for each element x of xs]"),
    ("print", "print(x): writes x to the output and returns it"),
    ("define", "define(s, v): gives the symbol s the value v"),
    ("im", "im: the imaginary unit, complex(0, 1)"),
    ("inf", "inf: positive infinity (also written ∞)"),
    // the numeric compiler's natives (`mc-jit`, not a dependency here: it would pull the code generator into the server)
    ("compile", "compile(f): compiles the typed function f to machine code (calls fall back to the interpreter when needed)"),
    ("compiled_info", "compiled_info(f): what was compiled for f: types, code size, back end, runs and declines"),
    // the engine's notebook natives (`mc_engine::FUNCTIONS`)
    ("out", "out(n): the value of numbered statement n of a notebook session; out() the last value, out(-k) the k-th previous one"),
    ("message", "message(tag, text): a warning from tag (a function's name); front ends show it apart from print output"),
    ("timing", "timing(e): [seconds, value]: e evaluated, with the wall-clock time the evaluation took in seconds; timing(e, breakdown: true) adds where the time went"),
    ("profile", "profile(e): [seconds, value, rows]: e evaluated with every call measured; rows [f, kind, calls, own seconds, total seconds] by own time"),
];

/// Where a known name comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// a library or evaluator function or constant
    Library,
    /// a definition of the prelude (`lib/prelude.mc`)
    Prelude,
}

/// A known name: where it comes from, a one-line description, and for prelude definitions the signature line and
/// the comment above it.
#[derive(Clone, Debug)]
pub struct Entry {
    /// library or prelude
    pub origin: Origin,
    /// the description (library), or the definition's first line (prelude)
    pub detail: String,
    /// the comment above a prelude definition
    pub doc: Option<String>,
    /// the function's entry in the help index, when the documentation knows it
    pub help: Option<&'static mc_help::Entry>,
}

/// The known names, by name.
pub struct Library {
    entries: BTreeMap<String, Entry>,
}

impl Default for Library {
    fn default() -> Library {
        Library::new()
    }
}

impl Library {
    /// Every library list, the evaluator's natives and the prelude.
    pub fn new() -> Library {
        let mut entries = BTreeMap::new();
        let lists = [CORE, mc_lists::FUNCTIONS, mc_assoc::FUNCTIONS, mc_numeric::FUNCTIONS, mc_numtheory::FUNCTIONS, mc_poly::FUNCTIONS, mc_calculus::FUNCTIONS, mc_strings::FUNCTIONS, mc_roots::FUNCTIONS, mc_quad::FUNCTIONS, mc_ode::FUNCTIONS, mc_graphics::FUNCTIONS, mc_plot::FUNCTIONS, mc_io::FUNCTIONS, mc_simplify::FUNCTIONS, mc_random::FUNCTIONS, mc_special::FUNCTIONS, mc_linalg::FUNCTIONS, mc_solve::FUNCTIONS, mc_dist::FUNCTIONS];
        for (name, desc) in lists.iter().flat_map(|l| l.iter()) {
            let e = Entry { origin: Origin::Library, detail: desc.to_string(), doc: None, help: None };
            entries.entry(name.to_string()).or_insert(e);
        }
        let text = stdlib::PRELUDE;
        let parsed = mc_syntax::parse(text);
        let ids = mc_syntax::identifiers(text);
        let sigs = Signatures::new(text);
        for it in outline::items(&parsed.program, &ids) {
            if it.kind == ItemKind::Module {
                continue;
            }
            let (detail, doc) = sigs.of(&it);
            entries.entry(it.name).or_insert(Entry { origin: Origin::Prelude, detail, doc, help: None });
        }
        // the help index: every documented function (also those of libraries not listed above)
        let index = mc_help::Index::embedded();
        for h in index.entries() {
            let e = entries.entry(h.name.clone()).or_insert(Entry {
                origin: Origin::Library,
                detail: h.line(),
                doc: None,
                help: None,
            });
            e.help = Some(h);
        }
        Library { entries }
    }

    /// The entry of a known name.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// Whether a session knows `name` before reading anything.
    pub fn knows(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Every known name, in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Entry)> {
        self.entries.iter().map(|(n, e)| (n.as_str(), e))
    }
}

/// Signature lines and comments of one text's definitions, for hover, completion and the outline.
pub struct Signatures<'t> {
    /// the lines, without line breaks
    lines: Vec<&'t str>,
    /// the byte offset of each line's start
    starts: Vec<usize>,
}

impl<'t> Signatures<'t> {
    /// The lines of `text`.
    pub fn new(text: &'t str) -> Signatures<'t> {
        let lines = text.split('\n').map(|l| l.trim_end_matches('\r')).collect();
        let mut starts = vec![0];
        starts.extend(text.match_indices('\n').map(|(i, _)| i + 1));
        Signatures { lines, starts }
    }

    /// The line holding byte `name_at` (the definition's signature line, trimmed, shortened if very long), and the
    /// comment lines directly above the statement starting at byte `stmt_at` (their `#` removed).
    pub fn at(&self, stmt_at: usize, name_at: usize) -> (String, Option<String>) {
        let line_of = |at: usize| self.starts.partition_point(|&s| s <= at) - 1;
        let mut sig = self.lines[line_of(name_at)].trim().to_string();
        if sig.chars().count() > 160 {
            sig = sig.chars().take(160).collect::<String>() + " ...";
        }
        let mut doc = vec![];
        let mut l = line_of(stmt_at);
        while l > 0 {
            l -= 1;
            match self.lines[l].trim().strip_prefix('#') {
                Some(c) if !c.starts_with('!') && !c.starts_with('%') => doc.push(c.strip_prefix(' ').unwrap_or(c).trim_end()),
                _ => break,
            }
        }
        doc.reverse();
        (sig, (!doc.is_empty()).then(|| doc.join("\n")))
    }

    /// [`Signatures::at`] for a definition of the text's outline.
    pub fn of(&self, it: &Item) -> (String, Option<String>) {
        self.at(it.span.start as usize, it.name_span.start as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_names() {
        let lib = Library::new();
        assert_eq!(lib.get("range").unwrap().origin, Origin::Library);
        assert!(lib.get("factor").unwrap().detail.starts_with("factor("));
        let clamp = lib.get("clamp").unwrap();
        assert_eq!(clamp.origin, Origin::Prelude);
        assert_eq!(clamp.detail, "fn clamp(x, lo, hi) = min(max(x, lo), hi)");
        assert_eq!(clamp.doc.as_deref(), Some("clamp(x, lo, hi): x limited to the interval [lo, hi]"));
        assert!(lib.knows("map") && lib.knows("pi") && !lib.knows("no_such_name"));
        // documented functions carry their help entry, prelude functions too
        assert!(lib.get("n_integrate").and_then(|e| e.help).is_some_and(|h| h.page));
        assert!(lib.get("clamp").and_then(|e| e.help).is_some());
    }

    #[test]
    fn signatures_and_comments() {
        let text = "#! mathcas 2\n# one\n#two\nfn f(x) =\n  x\n";
        assert_eq!(Signatures::new(text).at(24, 27), ("fn f(x) =".into(), Some("one\ntwo".into())));
        assert_eq!(Signatures::new("x = 1").at(0, 0), ("x = 1".into(), None));
        assert_eq!(Signatures::new("x = 1\n").at(6, 6), (String::new(), None));
    }
}
