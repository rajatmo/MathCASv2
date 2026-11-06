//! Which names in a page's code are links to function pages (decision NBDOCS.3).
//!
//! Code (usage calls, code spans in prose, example inputs and outputs) is tokenised with `mc_syntax::colour`; a
//! token is a link when it is a global name (not a local, a pattern or a keyword) that the catalogue knows, and either
//! it is called (`name(` follows) or it is longer than one character. Single letters stand for variables far more
//! often than for the one-letter functions (`n`, `d`, `x` in `table(n^2, [n, 1, 5])`), so they link only as calls.
use mc_syntax::TokenClass;
use std::ops::Range;

/// A link: the byte range of the name in its text, and the function it opens.
pub type Link = (Range<usize>, String);

/// The links among the classed `tokens` of `src` (the tokens of `mc_syntax::colour`), for names `known` says are
/// functions.
pub fn code_links(src: &str, tokens: &[(Range<usize>, TokenClass)], known: &dyn Fn(&str) -> bool) -> Vec<Link> {
    let mut out = vec![];
    for (r, class) in tokens {
        if !matches!(class, TokenClass::Global | TokenClass::Undefined) {
            continue;
        }
        let Some(name) = src.get(r.clone()) else { continue };
        if !mc_help::scan::is_name(name) || !known(name) {
            continue;
        }
        let called = src[r.end..].trim_start_matches([' ', '\t']).starts_with('(');
        if called || name.chars().count() > 1 {
            out.push((r.clone(), name.to_string()));
        }
    }
    out
}

/// The links of `src` (tokenised here).
pub fn links_of(src: &str, known: &dyn Fn(&str) -> bool) -> Vec<Link> {
    code_links(src, &mc_syntax::colour(src, None).tokens, known)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(src: &str) -> Vec<String> {
        let known = |n: &str| ["n_integrate", "sin", "n", "d", "table", "pi", "map"].contains(&n);
        links_of(src, &known).into_iter().map(|(_, n)| n).collect()
    }

    #[test]
    fn calls_and_longer_names_link() {
        assert_eq!(names("n_integrate(sin(x), [x, 0, pi])"), ["n_integrate", "sin", "pi"]);
        assert_eq!(names("table(n^2, [n, 1, 5])"), ["table"], "a one-letter variable is not the function n");
        assert_eq!(names("n(pi, 20)"), ["n", "pi"]);
        assert_eq!(names("map(sin, [1, 2])"), ["map", "sin"]);
        assert_eq!(names("fn f(sin) = sin + 1"), Vec::<String>::new(), "a parameter is a local, not the function");
        assert_eq!(names("\"sin(x)\" # sin(x)"), Vec::<String>::new(), "strings and comments are not code");
        assert_eq!(names("unknown(x)"), Vec::<String>::new());
    }
}
