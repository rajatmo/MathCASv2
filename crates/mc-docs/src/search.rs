//! Incremental search over the catalogue (round 19: forgiving, and still exact first).
//!
//! A query is matched against each function's name, then its one-line description, then its page. Name matches
//! ignore case and underscores (`nintegrate` and `NIntegrate` find `n_integrate`, `besselj` finds `bessel_j`),
//! and a query of two or more letters also matches a name's initials (`fr` finds `find_root`). Only when the exact
//! kinds find few functions are near misses added: names within a small edit distance of the query (one edit for
//! four to seven letters, two from eight; transpositions count as one edit), also as a prefix (`intgr` finds
//! `integrate`), and texts with a word within one edit of each query word of five letters or more. Near misses are
//! ranked after every exact kind, so a correct query never has its results pushed down by them.
//!
//! Within one kind of match, functions with a page come first, then the ones the pages' examples use most, then
//! shorter names, then alphabetical order.
use std::collections::HashMap;

/// What a search looks through for one function.
#[derive(Clone, Debug)]
pub struct Searchable {
    /// the name
    pub name: String,
    /// the one-line description and the page's usage and details, lower case (the description first: its words
    /// rank above the page's)
    pub words: String,
    /// whether the function has a page
    pub documented: bool,
    /// how many pages call the function in their examples (a measure of how much it is used)
    pub uses: usize,
}

/// For each name, how many of the pages' example inputs call it (as `name(`), once per page.
pub fn uses<'a>(names: impl IntoIterator<Item = &'a str>, inputs: &[String]) -> HashMap<String, usize> {
    let mut out = HashMap::new();
    for name in names {
        let call = format!("{name}(");
        let n = inputs.iter().filter(|src| calls(src, &call)).count();
        out.insert(name.to_string(), n);
    }
    out
}

/// Whether `src` contains `call` (`name(`) at the start of a name, not as the end of a longer one.
fn calls(src: &str, call: &str) -> bool {
    src.match_indices(call).any(|(i, _)| src[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_')))
}

/// Why a result matched, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// the name is the query (case and underscores aside)
    Exact,
    /// the name starts with the query
    Prefix,
    /// a word of the name (after `_`) starts with the query
    WordPrefix,
    /// the name contains the query
    Inside,
    /// the query is the initials of the name's words (`fr`: `find_root`)
    Initials,
    /// the one-line description mentions every word of the query
    Summary,
    /// the page mentions every word of the query
    Text,
    /// the name is within a small edit distance of the query, or begins with something that is
    Similar,
    /// every word of the query is, or is within one edit of, a word of the description or page
    SimilarText,
}

impl Rank {
    /// Whether this is a near miss (shown under a "similar" heading).
    pub fn is_near(self) -> bool {
        self >= Rank::Similar
    }
}

/// With fewer exact results than this, near misses are added.
const NEAR_BELOW: usize = 8;

/// One function prepared for searching.
#[derive(Clone, Debug)]
struct Prepared {
    lower: String,
    /// the lower-case name without underscores
    key: String,
    initials: String,
    /// where the description ends in `words`
    summary_end: usize,
    /// the distinct words of the text (indices into the vocabulary), sorted
    vocab: Vec<u32>,
}

/// The catalogue prepared for searching: built once, then every keystroke searches it.
#[derive(Clone, Debug)]
pub struct Searcher {
    items: Vec<Searchable>,
    prepared: Vec<Prepared>,
    /// every distinct word of every text
    vocab: Vec<String>,
}

fn text_words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|w| w.len() >= 2)
}

impl Searcher {
    /// Prepare `items`.
    pub fn new(items: Vec<Searchable>) -> Searcher {
        let mut ids: HashMap<String, u32> = HashMap::new();
        let mut vocab = vec![];
        let prepared = items
            .iter()
            .map(|it| {
                let lower = it.name.to_lowercase();
                let key = lower.replace('_', "");
                let initials: String = lower.split('_').filter_map(|w| w.chars().next()).collect();
                let mut v: Vec<u32> = text_words(&it.words)
                    .map(|w| {
                        *ids.entry(w.to_string()).or_insert_with(|| {
                            vocab.push(w.to_string());
                            (vocab.len() - 1) as u32
                        })
                    })
                    .collect();
                v.sort_unstable();
                v.dedup();
                let summary_end = it.words.find('\n').unwrap_or(it.words.len());
                Prepared { lower, key, initials, summary_end, vocab: v }
            })
            .collect();
        Searcher { items, prepared, vocab }
    }

    /// How many functions it searches.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether it searches nothing.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The names matching `query`, best first, at most `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<(String, Rank)> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return vec![];
        }
        let words: Vec<&str> = q.split_whitespace().collect();
        let name_query = words.join("_");
        let key_query: String = name_query.chars().filter(|c| *c != '_').collect();
        let one_word = words.len() == 1;
        let mut hits: Vec<(Rank, usize, usize)> = vec![];
        let mut matched = vec![false; self.items.len()];
        for (k, (it, p)) in self.items.iter().zip(&self.prepared).enumerate() {
            let rank = if p.lower == name_query || (!key_query.is_empty() && p.key == key_query) {
                Rank::Exact
            } else if p.lower.starts_with(&name_query) || (!key_query.is_empty() && p.key.starts_with(&key_query)) {
                Rank::Prefix
            } else if p.lower.split('_').skip(1).any(|w| w.starts_with(&name_query)) {
                Rank::WordPrefix
            } else if p.lower.contains(&name_query) || (key_query.len() >= 3 && p.key.contains(&key_query)) {
                Rank::Inside
            } else if one_word && q.len() >= 2 && p.initials.len() >= 2 && p.initials == q {
                Rank::Initials
            } else if words.iter().all(|w| it.words[..p.summary_end].contains(w)) {
                Rank::Summary
            } else if words.iter().all(|w| it.words.contains(w)) {
                Rank::Text
            } else {
                continue;
            };
            matched[k] = true;
            hits.push((rank, 0, k));
        }
        if hits.len() < NEAR_BELOW {
            self.near(&words, &key_query, &mut matched, &mut hits);
        }
        // near misses: fewer edits first
        hits.sort_by(|(ra, da, a), (rb, db, b)| {
            let (a, b) = (&self.items[*a], &self.items[*b]);
            ra.cmp(rb)
                .then(da.cmp(db))
                .then(b.documented.cmp(&a.documented))
                .then(b.uses.cmp(&a.uses))
                .then(a.name.len().cmp(&b.name.len()))
                .then(a.name.cmp(&b.name))
        });
        hits.truncate(limit);
        hits.into_iter().map(|(r, _, k)| (self.items[k].name.clone(), r)).collect()
    }

    /// Near misses: names within the allowed edit distance (whole or as a prefix), then texts whose words are.
    fn near(&self, words: &[&str], key_query: &str, matched: &mut [bool], hits: &mut Vec<(Rank, usize, usize)>) {
        let allowed = |n: usize| match n {
            0..=3 => 0,
            4..=7 => 1,
            _ => 2,
        };
        let qb = key_query.as_bytes();
        let max = allowed(qb.len());
        if max > 0 {
            for (k, p) in self.prepared.iter().enumerate() {
                let d = prefix_distance(qb, p.key.as_bytes(), max).min(distance(qb, p.key.as_bytes(), max));
                if !matched[k] && d <= max {
                    matched[k] = true;
                    hits.push((Rank::Similar, d, k));
                }
            }
        }
        // the text: every query word is a word of the text or within one edit of one (words of five letters or
        // more; shorter ones must be there as they are)
        let mut sets: Vec<Vec<bool>> = vec![];
        for w in words {
            let wb = w.as_bytes();
            let fuzzy = wb.len() >= 5;
            let set: Vec<bool> = self
                .vocab
                .iter()
                .map(|v| v.contains(w) || (fuzzy && v.len().abs_diff(wb.len()) <= 1 && distance(wb, v.as_bytes(), 1) <= 1))
                .collect();
            if !set.iter().any(|b| *b) {
                return;
            }
            sets.push(set);
        }
        for (k, p) in self.prepared.iter().enumerate() {
            if !matched[k] && sets.iter().all(|s| p.vocab.iter().any(|&v| s[v as usize])) {
                matched[k] = true;
                hits.push((Rank::SimilarText, 0, k));
            }
        }
    }
}

/// What a search looks through for one topic guide (round 23).
#[derive(Clone, Debug)]
pub struct GuideText {
    /// the title, lower case
    pub title: String,
    /// the headings, prose and example inputs, lower case
    pub words: String,
}

/// The guides matching `query`, best first: those whose title holds every word of the query (underscores and case
/// aside), then those whose text (headings, prose, example inputs) does; within a kind the guide that mentions the words most
/// often first, then the guides' order. Returns
/// indices into `items`.
pub fn search_guides(items: &[GuideText], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase().replace('_', " ");
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.is_empty() {
        return vec![];
    }
    let mut hits: Vec<(u8, std::cmp::Reverse<usize>, usize)> = vec![];
    for (k, g) in items.iter().enumerate() {
        let title = g.title.replace('_', " ");
        let body = g.words.replace('_', " ");
        // within a kind, the guide that mentions the words most often first
        let mentions = words.iter().map(|w| body.matches(w).count()).sum();
        if words.iter().all(|w| title.contains(w)) {
            hits.push((0, std::cmp::Reverse(mentions), k));
        } else if words.iter().all(|w| title.contains(w) || body.contains(w)) {
            hits.push((1, std::cmp::Reverse(mentions), k));
        }
    }
    hits.sort();
    hits.into_iter().map(|(_, _, k)| k).collect()
}

/// The names matching `query` in `items`, best first, at most `limit` (a [`Searcher`] made for one search).
pub fn search(items: &[Searchable], query: &str, limit: usize) -> Vec<(String, Rank)> {
    Searcher::new(items.to_vec()).search(query, limit)
}

/// The optimal-string-alignment distance of `a` and `b` (a transposition of neighbours is one edit), or more
/// than `max` as soon as it must be.
fn distance(a: &[u8], b: &[u8], max: usize) -> usize {
    edit_rows(a, b, max).map_or(max + 1, |last| last[b.len()])
}

/// The least distance of `q` to a prefix of `name` (`intgr` is one edit from `integr`, a prefix of `integrate`).
fn prefix_distance(q: &[u8], name: &[u8], max: usize) -> usize {
    let name = &name[..name.len().min(q.len() + max)];
    edit_rows(q, name, max).map_or(max + 1, |last| last.iter().copied().min().unwrap_or(max + 1))
}

/// The last row of the edit-distance table of `a` against `b`, or `None` when every entry of a row exceeds `max`
/// (the distance and every prefix distance then do too).
fn edit_rows(a: &[u8], b: &[u8], max: usize) -> Option<Vec<usize>> {
    let n = b.len();
    let mut prev2: Vec<usize> = vec![0; n + 1];
    let mut prev: Vec<usize> = (0..=n).collect();
    let mut cur = vec![0; n + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=n {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut v = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                v = v.min(prev2[j - 2] + 1);
            }
            cur[j] = v;
        }
        if cur.iter().all(|&v| v > max) {
            return None;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
    }
    Some(prev)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guides_are_found_by_title_then_by_text() {
        let g = |t: &str, w: &str| GuideText { title: t.into(), words: w.into() };
        let items = vec![
            g("calculus", "limits; series expansions and laplace_transform"),
            g("transforms and series", "laplace transform of exp(-t); laplace_transform(t, s)"),
            g("plotting", "plot(sin(x))"),
        ];
        assert_eq!(search_guides(&items, "series"), vec![1, 0]);
        assert_eq!(search_guides(&items, "Laplace_Transform"), vec![1, 0]);
        assert_eq!(search_guides(&items, "transforms series"), vec![1]);
        assert_eq!(search_guides(&items, "plot"), vec![2]);
        assert!(search_guides(&items, "zeta").is_empty() && search_guides(&items, "  ").is_empty());
    }

    fn items() -> Vec<Searchable> {
        [
            ("n_integrate", "the integral of f numerically"),
            ("integrate", "an antiderivative"),
            ("int_part", "the integer part"),
            ("find_root", "a root of f near x0"),
            ("solve", "the solutions of equations; roots by radicals"),
            ("bessel_j", "the bessel function of the first kind\nsee also bessel_y"),
            ("factor", "factors a polynomial\nthe factorisation over the rationals"),
        ]
        .iter()
        .map(|(n, w)| Searchable { name: n.to_string(), words: w.to_string(), documented: false, uses: 0 })
        .collect()
    }

    fn names(q: &str) -> Vec<String> {
        search(&items(), q, 10).into_iter().map(|(n, _)| n).collect()
    }

    fn ranked(q: &str) -> Vec<(String, Rank)> {
        search(&items(), q, 10)
    }

    #[test]
    fn ranking() {
        assert_eq!(names("int"), ["int_part", "integrate", "n_integrate"]);
        assert_eq!(names("integrate"), ["integrate", "n_integrate"]);
        assert_eq!(names("root"), ["find_root", "solve"]);
        assert_eq!(names("ROOT near"), ["find_root"]);
        assert_eq!(names("find root"), ["find_root"]);
        assert!(names("  ").is_empty());
        assert!(names("zzz").is_empty());
        assert_eq!(search(&items(), "int", 1).len(), 1);
    }

    #[test]
    fn names_match_without_underscores_and_by_initials() {
        assert_eq!(ranked("nintegrate")[0], ("n_integrate".into(), Rank::Exact));
        assert_eq!(ranked("NIntegrate")[0], ("n_integrate".into(), Rank::Exact));
        assert_eq!(ranked("besselj")[0], ("bessel_j".into(), Rank::Exact));
        assert_eq!(ranked("findr")[0], ("find_root".into(), Rank::Prefix));
        assert_eq!(ranked("fr")[0], ("find_root".into(), Rank::Initials));
    }

    #[test]
    fn the_description_ranks_above_the_page() {
        // "factor" is in factor's description, "integers" only in its page; "bessel" in bessel_j's name
        assert_eq!(ranked("polynomial"), [("factor".into(), Rank::Summary)]);
        assert_eq!(ranked("factorisation"), [("factor".into(), Rank::Text)]);
    }

    #[test]
    fn near_misses_come_only_after_exact_kinds() {
        assert_eq!(ranked("intgrate")[0], ("integrate".into(), Rank::Similar));
        assert_eq!(ranked("integarte")[0], ("integrate".into(), Rank::Similar), "a transposition is one edit");
        assert_eq!(ranked("intgr")[0], ("integrate".into(), Rank::Similar), "a prefix within one edit");
        assert_eq!(ranked("besel")[0], ("bessel_j".into(), Rank::Similar));
        assert_eq!(names("numericaly"), ["n_integrate"], "a text word within one edit");
        assert_eq!(ranked("numericaly")[0].1, Rank::SimilarText);
        // short queries are never fuzzy
        assert!(names("slv").is_empty());
        // an exact hit keeps its place: near misses come after it
        let r = ranked("solve");
        assert_eq!(r[0], ("solve".into(), Rank::Exact));
        assert!(r.iter().skip(1).all(|(_, k)| *k > Rank::Exact));
    }

    #[test]
    fn distances() {
        assert_eq!(distance(b"kitten", b"sitting", 5), 3);
        assert_eq!(distance(b"ab", b"ba", 2), 1);
        assert_eq!(distance(b"abc", b"abc", 0), 0);
        assert!(distance(b"abcdef", b"zzzzzz", 1) > 1);
        assert_eq!(prefix_distance(b"intgr", b"integrate", 1), 1);
        assert_eq!(prefix_distance(b"integ", b"integrate", 1), 0);
    }

    #[test]
    fn documented_and_used_functions_rank_first_among_equals() {
        let mut its = items();
        // "integrate" and "int_part" are both prefix matches of "int": the shorter name wins until the other has a
        // page, then the one with a page wins; among undocumented ones, more uses win
        assert_eq!(names("int")[0], "int_part");
        its.iter_mut().find(|i| i.name == "integrate").unwrap().documented = true;
        assert_eq!(search(&its, "int", 10)[0].0, "integrate");
        its.iter_mut().find(|i| i.name == "integrate").unwrap().documented = false;
        its.iter_mut().find(|i| i.name == "integrate").unwrap().uses = 3;
        assert_eq!(search(&its, "int", 10)[0].0, "integrate");
    }

    #[test]
    fn uses_count_calls_not_name_endings() {
        let inputs = vec!["n_integrate(f, [x, 0, 1])".to_string(), "integrate(x, x) + d(x, x)".to_string()];
        let u = uses(["integrate", "n_integrate", "d", "x"], &inputs);
        assert_eq!(u["integrate"], 1);
        assert_eq!(u["n_integrate"], 1);
        assert_eq!(u["d"], 1);
        assert_eq!(u["x"], 0);
    }
}
