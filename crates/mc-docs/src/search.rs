//! Incremental search over the catalogue: names that start with the query first, then names that contain it, then
//! functions whose description or page mentions every word of the query. Within one kind of match, functions with a
//! page come first, then the ones the pages' examples use most, then shorter names.

/// What a search looks through for one function.
#[derive(Clone, Debug)]
pub struct Searchable {
    /// the name
    pub name: String,
    /// the description and the page's usage and details, lower case
    pub words: String,
    /// whether the function has a page
    pub documented: bool,
    /// how many pages call the function in their examples (a measure of how much it is used)
    pub uses: usize,
}

/// For each name, how many of the pages' example inputs call it (as `name(`), once per page.
pub fn uses<'a>(names: impl IntoIterator<Item = &'a str>, inputs: &[String]) -> std::collections::HashMap<String, usize> {
    let mut out = std::collections::HashMap::new();
    for name in names {
        let call = format!("{name}(");
        let n = inputs.iter().filter(|src| calls(src, &call)).count();
        out.insert(name.to_string(), n);
    }
    out
}

/// Whether `src` contains `call` (`name(`) at the start of a name, not as the end of a longer one.
fn calls(src: &str, call: &str) -> bool {
    src.match_indices(call).any(|(i, _)| {
        src[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'))
    })
}

/// Why a result matched, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    /// the name is the query
    Exact,
    /// the name starts with the query
    Prefix,
    /// a word of the name (after `_`) starts with the query
    WordPrefix,
    /// the name contains the query
    Inside,
    /// the text mentions every word of the query
    Text,
}

/// The names matching `query`, best first (ties: documented first, then more uses, shorter names, alphabetical), at
/// most `limit`.
pub fn search(items: &[Searchable], query: &str, limit: usize) -> Vec<(String, Rank)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return vec![];
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let name_query = q.replace(' ', "_");
    let mut hits: Vec<(Rank, &Searchable)> = items
        .iter()
        .filter_map(|it| {
            let name = it.name.to_lowercase();
            let rank = if name == name_query {
                Rank::Exact
            } else if name.starts_with(&name_query) {
                Rank::Prefix
            } else if name.split('_').skip(1).any(|w| w.starts_with(&name_query)) {
                Rank::WordPrefix
            } else if name.contains(&name_query) {
                Rank::Inside
            } else if words.iter().all(|w| it.words.contains(w)) {
                Rank::Text
            } else {
                return None;
            };
            Some((rank, it))
        })
        .collect();
    hits.sort_by(|(ra, a), (rb, b)| {
        ra.cmp(rb)
            .then(b.documented.cmp(&a.documented))
            .then(b.uses.cmp(&a.uses))
            .then(a.name.len().cmp(&b.name.len()))
            .then(a.name.cmp(&b.name))
    });
    hits.truncate(limit);
    hits.into_iter().map(|(r, it)| (it.name.clone(), r)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> Vec<Searchable> {
        [
            ("n_integrate", "the integral of f numerically"),
            ("integrate", "an antiderivative"),
            ("int_part", "the integer part"),
            ("find_root", "a root of f near x0"),
            ("solve", "the solutions of equations; roots by radicals"),
        ]
        .iter()
        .map(|(n, w)| Searchable { name: n.to_string(), words: w.to_string(), documented: false, uses: 0 })
        .collect()
    }

    fn names(q: &str) -> Vec<String> {
        search(&items(), q, 10).into_iter().map(|(n, _)| n).collect()
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
