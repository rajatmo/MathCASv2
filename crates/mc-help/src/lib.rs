//! The help index of MathCAS v2: every function the documentation knows, with its area, topic, one-line
//! description and usage lines, in one embedded table that the notebook (completion, usage hints, hover, F1), the
//! language server and the documentation window share. It has two parts, both from the documentation's own
//! sources:
//!
//! - the **catalogue table** `data/index.txt`: every function with its area, topic and the description its library
//!   gives (`FUNCTIONS` lists), written by `mc-docs-pages index` from `mc_docs::catalogue` (a test of `mc-docs`
//!   fails when it is out of date: the libraries cannot be dependencies of this small crate);
//! - the **usage lines** of the function pages (`docs/functions/<name>.mcn`), read by this crate's build script, so
//!   they are always the pages' current ones.
//!
//! [`scan`] has the text scanning the editors need: the name around or before a position, the call a position is
//! inside (with the argument number), the parameters of a usage line.
//!
//! # The catalogue table's format
//!
//! Lines starting with `#` are comments; every other line is `name TAB area TAB topic TAB summary`. Tabs and line
//! breaks inside texts are written as spaces.
pub mod scan;

use std::collections::HashMap;
use std::sync::OnceLock;

/// One signature of a function's page: the call and what it gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Usage {
    /// the call, as `n_integrate(f, [x, a, b])`
    pub call: String,
    /// what it gives (may hold code spans in backticks)
    pub text: String,
}

/// A function of the help index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// the name
    pub name: String,
    /// the area's key (`core`, `data`, `visualisation`, `symbolic`, `higher`)
    pub area: String,
    /// the topic within the area
    pub topic: String,
    /// whether the function has a page
    pub page: bool,
    /// the one-line description from the library (it starts with a signature)
    pub summary: String,
    /// the usage lines of its page (empty without a page)
    pub usages: Vec<Usage>,
}

impl Entry {
    /// The calls of the function: its page's usage calls, else the signature at the start of its description
    /// (`f(a, b)` of `f(a, b): ...`), else the bare name.
    pub fn calls(&self) -> Vec<String> {
        if !self.usages.is_empty() {
            return self.usages.iter().map(|u| u.call.clone()).collect();
        }
        vec![self.summary_call().unwrap_or_else(|| self.name.clone())]
    }

    /// The signature at the start of the description, when it has one.
    fn summary_call(&self) -> Option<String> {
        let s = self.summary.strip_prefix(&self.name)?;
        if s.starts_with('(') {
            let close = scan::matching_close(s, 0)?;
            return Some(format!("{}{}", self.name, &s[..=close]));
        }
        s.starts_with(':').then(|| self.name.clone())
    }

    /// What the call `k` gives, as plain text: the page's usage text, else the description after its signature.
    pub fn text_of(&self, k: usize) -> String {
        if let Some(u) = self.usages.get(k) {
            return plain(&u.text);
        }
        match self.summary_call() {
            Some(call) => {
                let rest = &self.summary[call.len().min(self.summary.len())..];
                rest.trim_start_matches(':').trim().to_string()
            }
            None => self.summary.clone(),
        }
    }

    /// The one-line usage shown by completion lists and hovers: the first call and what it gives.
    pub fn line(&self) -> String {
        let calls = self.calls();
        let text = self.text_of(0);
        let call = calls.first().cloned().unwrap_or_else(|| self.name.clone());
        if text.is_empty() {
            call
        } else {
            format!("{call}: {text}")
        }
    }

    /// Whether the function is called with arguments (its first call has a bracket).
    pub fn is_function(&self) -> bool {
        self.calls().first().is_some_and(|c| c.contains('('))
    }

    /// The call to show while argument `arg` (0-based) is typed: the first call with that many parameters, or one
    /// with a `...` parameter, else the first. Gives the call's number.
    pub fn best_call(&self, arg: usize) -> usize {
        let calls = self.calls();
        calls
            .iter()
            .position(|c| {
                let ps = scan::params(c);
                ps.len() > arg || ps.last().is_some_and(|r| c[r.clone()].contains("..."))
            })
            .unwrap_or(0)
    }
}

/// Prose without its code-span backticks.
pub fn plain(text: &str) -> String {
    text.replace('`', "")
}

/// The help index: entries by name.
#[derive(Clone, Debug, Default)]
pub struct Index {
    entries: Vec<Entry>,
    by_name: HashMap<String, usize>,
}

/// The embedded catalogue table.
const TABLE: &str = include_str!("../data/index.txt");

mod pages {
    include!(concat!(env!("OUT_DIR"), "/pages.rs"));
}

impl Index {
    /// The index built into the program.
    pub fn embedded() -> &'static Index {
        static INDEX: OnceLock<Index> = OnceLock::new();
        INDEX.get_or_init(|| {
            Index::parse(TABLE).unwrap_or_default().with_pages(pages::PAGES.iter().map(|(n, u)| (*n, *u)))
        })
    }

    /// The text of the embedded catalogue table (to compare it with a freshly generated one).
    pub fn embedded_text() -> &'static str {
        TABLE
    }

    /// An index of `entries`.
    pub fn new(entries: Vec<Entry>) -> Index {
        let by_name = entries.iter().enumerate().map(|(i, e)| (e.name.clone(), i)).collect();
        Index { entries, by_name }
    }

    /// Read a catalogue table; the first malformed line is an error (line number and why).
    pub fn parse(text: &str) -> Result<Index, String> {
        let mut entries: Vec<Entry> = vec![];
        for (k, line) in text.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let f: Vec<&str> = line.splitn(4, '\t').collect();
            let [name, area, topic, summary] = f[..] else {
                return Err(format!("line {}: an entry is name TAB area TAB topic TAB summary", k + 1));
            };
            entries.push(Entry {
                name: name.to_string(),
                area: area.to_string(),
                topic: topic.to_string(),
                page: false,
                summary: summary.to_string(),
                usages: vec![],
            });
        }
        Ok(Index::new(entries))
    }

    /// The index with the usage lines of `pages` (name, then (call, text) per signature): a page's function gets
    /// them and is marked as having a page.
    pub fn with_pages<'a>(mut self, pages: impl IntoIterator<Item = (&'a str, &'a [(&'a str, &'a str)])>) -> Index {
        for (name, usages) in pages {
            if let Some(&i) = self.by_name.get(name) {
                let e = &mut self.entries[i];
                e.page = true;
                e.usages = usages.iter().map(|(c, t)| Usage { call: c.to_string(), text: t.to_string() }).collect();
            }
        }
        self
    }

    /// The catalogue table of `entries` (what [`Index::parse`] reads; pages and usage lines are not part of it).
    pub fn write(entries: &[Entry]) -> String {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let mut out = String::from(
            "# The MathCAS help index's catalogue, written by `mc-docs-pages index` from the documentation's catalogue.\n\
             # Do not edit: regenerate it. Format: name TAB area TAB topic TAB summary.\n",
        );
        for e in entries {
            out.push_str(&format!("{}\t{}\t{}\t{}\n", e.name, e.area, clean(&e.topic), clean(&e.summary)));
        }
        out
    }

    /// The entry of `name`.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).map(|&i| &self.entries[i])
    }

    /// Every entry, in catalogue order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entries matching `prefix` (see [`score`]), best first; ties: functions with a page first, then shorter
    /// names, then alphabetical.
    pub fn complete(&self, prefix: &str) -> Vec<&Entry> {
        let mut hits: Vec<(u8, &Entry)> =
            self.entries.iter().filter_map(|e| score(&e.name, prefix).map(|s| (s, e))).collect();
        hits.sort_by(|(sa, a), (sb, b)| {
            sa.cmp(sb).then(b.page.cmp(&a.page)).then(a.name.len().cmp(&b.name.len())).then(a.name.cmp(&b.name))
        });
        hits.into_iter().map(|(_, e)| e).collect()
    }
}

/// How well `name` matches a typed `prefix`, ignoring case (lower is better): 0 the name starts with it, 1 a word
/// of the name (after `_`) starts with it, 2 its characters appear in order in the name; `None` otherwise.
pub fn score(name: &str, prefix: &str) -> Option<u8> {
    let n = name.to_lowercase();
    let p = prefix.to_lowercase();
    if n.starts_with(&p) {
        return Some(0);
    }
    if n.split('_').skip(1).any(|w| !p.is_empty() && w.starts_with(&p)) {
        return Some(1);
    }
    let mut it = n.chars();
    p.chars().all(|c| it.any(|d| d == c)).then_some(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "# a comment\n\
        n_integrate\tsymbolic\tNumerical integration\tn_integrate(f, [x, a, b]): numerically\n\
        int_part\tcore\tLists\tint_part(x): the integer part of x\n\
        pi\thigher\tSpecial functions\tpi: the ratio of a circle's circumference to its diameter\n\
        integrate\tsymbolic\tCalculus\tintegrate(f, x): an antiderivative\n";

    const PAGES: &[(&str, &[(&str, &str)])] = &[
        (
            "n_integrate",
            &[
                ("n_integrate(f, [x, a, b])", "gives a numerical approximation to the integral of `f`."),
                ("n_integrate(f, [x, a, b], [y, c, d])", "gives a multiple integral."),
            ],
        ),
        ("integrate", &[("integrate(f, x)", "gives an antiderivative.")]),
        ("no_such", &[("no_such()", "is ignored.")]),
    ];

    fn index() -> Index {
        Index::parse(T).unwrap().with_pages(PAGES.iter().map(|(n, u)| (*n, *u)))
    }

    #[test]
    fn reads_and_writes_the_table() {
        let ix = index();
        assert_eq!(ix.entries().len(), 4);
        let n = ix.get("n_integrate").unwrap();
        assert!(n.page);
        assert_eq!(n.usages.len(), 2);
        assert_eq!(n.line(), "n_integrate(f, [x, a, b]): gives a numerical approximation to the integral of f.");
        let again = Index::parse(&Index::write(ix.entries())).unwrap().with_pages(PAGES.iter().map(|(n, u)| (*n, *u)));
        assert_eq!(again.entries(), ix.entries());
        assert!(Index::parse("f\tcore\n").is_err());
        assert!(!ix.get("pi").unwrap().page);
    }

    #[test]
    fn calls_from_pages_or_descriptions() {
        let ix = index();
        let ip = ix.get("int_part").unwrap();
        assert_eq!(ip.calls(), ["int_part(x)"]);
        assert_eq!(ip.line(), "int_part(x): the integer part of x");
        assert!(ip.is_function());
        let pi = ix.get("pi").unwrap();
        assert_eq!(pi.calls(), ["pi"]);
        assert!(!pi.is_function());
        assert_eq!(pi.line(), "pi: the ratio of a circle's circumference to its diameter");
        let n = ix.get("n_integrate").unwrap();
        assert_eq!(n.best_call(0), 0);
        assert_eq!(n.best_call(2), 1);
        assert_eq!(n.best_call(7), 0);
    }

    #[test]
    fn completion_order() {
        let ix = index();
        let names = |p: &str| ix.complete(p).into_iter().map(|e| e.name.as_str()).collect::<Vec<_>>();
        // prefix matches first, a page before a shorter name without one
        assert_eq!(names("int"), ["integrate", "int_part", "n_integrate"]);
        assert_eq!(names("INTEG"), ["integrate", "n_integrate"]);
        assert_eq!(names("part"), ["int_part"]);
        assert_eq!(names("ntg"), ["integrate", "n_integrate"]);
        assert!(names("zz").is_empty());
        assert_eq!(score("n_integrate", "integ"), Some(1));
    }

    #[test]
    fn the_embedded_table_reads() {
        let ix = Index::embedded();
        assert!(ix.entries().len() > 300, "{}", ix.entries().len());
        let t = ix.get("table").expect("table");
        assert!(t.page && t.is_function());
        assert!(t.line().starts_with("table("));
        assert!(ix.get("n_integrate").is_some_and(|e| e.area == "symbolic"));
    }
}
