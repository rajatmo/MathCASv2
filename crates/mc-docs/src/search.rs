//! Incremental search over the catalogue: names that start with the query first, then names that contain it, then
//! functions whose description or page mentions every word of the query.

/// What a search looks through for one function.
#[derive(Clone, Debug)]
pub struct Searchable {
    /// the name
    pub name: String,
    /// the description and the page's usage and details, lower case
    pub words: String,
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

/// The names matching `query`, best first (ties: shorter names, then alphabetical), at most `limit`.
pub fn search(items: &[Searchable], query: &str, limit: usize) -> Vec<(String, Rank)> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return vec![];
    }
    let words: Vec<&str> = q.split_whitespace().collect();
    let name_query = q.replace(' ', "_");
    let mut hits: Vec<(Rank, &str)> = items
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
            Some((rank, it.name.as_str()))
        })
        .collect();
    hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.len().cmp(&b.1.len())).then(a.1.cmp(b.1)));
    hits.truncate(limit);
    hits.into_iter().map(|(r, n)| (n.to_string(), r)).collect()
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
        .map(|(n, w)| Searchable { name: n.to_string(), words: w.to_string() })
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
}
