//! The alias table (design 08, section 3.9): `Esc` name `Esc` inserts a character or a template. One entry per
//! line of `data/aliases.tsv`: `alias <TAB> result <TAB> kind`, kinds `greek-short`, `greek`, `tex`, `html`,
//! `operator`, `constant`, `letterlike`, `space`, `template`. A template's result is canonical box text with `□`
//! placeholders and `▮` where the caret goes. The first entry for a name wins.
use std::collections::HashMap;
use std::sync::OnceLock;

/// The table's source.
pub const TABLE: &str = include_str!("../data/aliases.tsv");

/// What an alias inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alias {
    /// characters
    Text(&'static str),
    /// canonical box text with placeholders (see [`crate::Editor::insert_template`])
    Template(&'static str),
}

/// One entry of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// the name typed between the `Esc` markers
    pub alias: &'static str,
    /// what it inserts
    pub result: &'static str,
    /// its kind (see the module documentation)
    pub kind: &'static str,
}

/// Every entry, in table order (for help windows and completion).
pub fn entries() -> &'static [Entry] {
    static E: OnceLock<Vec<Entry>> = OnceLock::new();
    E.get_or_init(|| {
        TABLE
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                let (alias, result, kind) = (f.next()?, f.next()?, f.next()?);
                (!alias.is_empty() && !result.is_empty()).then_some(Entry { alias, result, kind })
            })
            .collect()
    })
}

/// Looks an alias up.
pub fn lookup(name: &str) -> Option<Alias> {
    static M: OnceLock<HashMap<&'static str, Entry>> = OnceLock::new();
    let m = M.get_or_init(|| {
        let mut m = HashMap::new();
        for e in entries() {
            m.entry(e.alias).or_insert(*e);
        }
        m
    });
    m.get(name).map(|e| if e.kind == "template" { Alias::Template(e.result) } else { Alias::Text(e.result) })
}

/// [`lookup`] for the notebook's cells: a template is offered only when its boxes are the three a cell has (square
/// root, fraction, power: `sqrtt`, `fract`, `pw`); the others (integrals, sums, matrices, subscripts) are unknown.
pub fn lookup_cell(name: &str) -> Option<Alias> {
    match lookup(name)? {
        Alias::Template(t) => {
            let row = crate::from_text(t);
            (crate::math::restrict(&row) == row).then_some(Alias::Template(t))
        }
        text => Some(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canonical::from_text;

    #[test]
    fn the_design_examples() {
        for (a, r) in [
            ("a", "α"),
            ("e", "ϵ"),
            ("ce", "ε"),
            ("th", "θ"),
            ("q", "θ"),
            ("cph", "φ"),
            ("G", "Γ"),
            ("O", "Ω"),
            ("alpha", "α"),
            ("Alpha", "Α"),
            ("vartheta", "ϑ"),
            ("\\alpha", "α"),
            ("\\infty", "∞"),
            ("\\leq", "≤"),
            ("\\rightarrow", "→"),
            ("\\int", "∫"),
            ("&alpha;", "α"),
            ("&le;", "≤"),
            ("->", "→"),
            (":>", "⧴"),
            ("!=", "≠"),
            ("==", "⩵"),
            ("*", "×"),
            (".", "·"),
            ("inf", "∞"),
            ("elem", "∈"),
            ("!elem", "∉"),
            ("sub=", "⊆"),
            ("===", "≡"),
            ("ee", "ⅇ"),
            ("ii", "ⅈ"),
            ("dd", "ⅆ"),
            ("pi", "π"),
            ("dsR", "ℝ"),
            ("scl", "ℓ"),
            ("goR", "ℜ"),
            ("hb", "ℏ"),
            ("al", "ℵ"),
            ("es", "∅"),
            ("  ", "\u{2009}"),
        ] {
            assert_eq!(lookup(a), Some(Alias::Text(r)), "{a}");
        }
        assert_eq!(lookup("nope"), None);
        assert!(entries().len() > 800);
    }

    #[test]
    fn templates_are_valid_box_text() {
        let mut n = 0;
        for e in entries().iter().filter(|e| e.kind == "template") {
            assert_eq!(e.result.matches('▮').count(), 1, "{}", e.alias);
            let r = from_text(e.result);
            assert!(r.0.iter().any(|i| matches!(i, crate::Item::Box(_))), "{} is not a box", e.alias);
            n += 1;
        }
        assert!(n >= 8);
        for t in ["intt", "dintt", "sumt", "prodt", "dt", "cbrt", "surd", "mat"] {
            assert!(matches!(lookup(t), Some(Alias::Template(_))), "{t}");
        }
    }

    #[test]
    fn every_line_has_three_fields_and_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for l in TABLE.lines() {
            assert_eq!(l.split('\t').count(), 3, "{l:?}");
            assert!(seen.insert(l.split('\t').next().unwrap()), "duplicate {l:?}");
        }
    }
}
