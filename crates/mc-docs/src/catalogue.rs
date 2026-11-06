//! The catalogue: every library function with its one-line description (the `FUNCTIONS` lists the engine installs),
//! sorted into the five areas of the system and a topic per library.

/// The five areas of the system, the tiles of the home page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Area {
    /// the language and its data structures: lists, associations, strings, the session
    Core,
    /// reading and writing files, random numbers, statistics
    Data,
    /// plots and images
    Visualisation,
    /// calculus, algebra, equations, numerical methods
    SymbolicNumeric,
    /// number theory, special functions, linear algebra
    Higher,
}

impl Area {
    /// Every area, in the home page's order.
    pub const ALL: [Area; 5] = [Area::Core, Area::Data, Area::Visualisation, Area::SymbolicNumeric, Area::Higher];

    /// The tile's title.
    pub fn title(self) -> &'static str {
        match self {
            Area::Core => "Core language",
            Area::Data => "Data",
            Area::Visualisation => "Visualisation",
            Area::SymbolicNumeric => "Symbolic and numeric computation",
            Area::Higher => "Higher mathematics",
        }
    }

    /// A sentence under the title.
    pub fn blurb(self) -> &'static str {
        match self {
            Area::Core => "Lists and how to build, reshape and search them; associations and sets; text; the session.",
            Area::Data => "Files in and out, random numbers, probability distributions and statistics.",
            Area::Visualisation => "Curves and point sets drawn as images.",
            Area::SymbolicNumeric => {
                "Derivatives, integrals, equations, simplification, polynomials, and numerical integration, \
                 root finding and differential equations."
            }
            Area::Higher => "Number theory, special functions and linear algebra.",
        }
    }

    /// A short word for the self-test and links (`guide:core`).
    pub fn key(self) -> &'static str {
        match self {
            Area::Core => "core",
            Area::Data => "data",
            Area::Visualisation => "visualisation",
            Area::SymbolicNumeric => "symbolic",
            Area::Higher => "higher",
        }
    }

    /// The area of a key.
    pub fn from_key(key: &str) -> Option<Area> {
        Area::ALL.into_iter().find(|a| a.key() == key)
    }
}

/// One function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// the name
    pub name: &'static str,
    /// the one-line description (it starts with the signature)
    pub summary: &'static str,
    /// the area
    pub area: Area,
    /// the topic within the area
    pub topic: &'static str,
}

/// The evaluator's own functions that a program calls by name (no library lists them): the operators' functions
/// and the core tools, described here.
const LANGUAGE: &[(&str, &str)] = &[
    ("add", "add(a, b, ...): the sum a + b + ..."),
    ("mul", "mul(a, b, ...): the product a*b*..."),
    ("pow", "pow(a, b): the power a^b"),
    ("equal", "equal(a, b): a == b, true or false when it can be decided, else left as it is"),
    ("unequal", "unequal(a, b): a != b"),
    ("less", "less(a, b): a < b"),
    ("less_equal", "less_equal(a, b): a <= b"),
    ("greater", "greater(a, b): a > b"),
    ("greater_equal", "greater_equal(a, b): a >= b"),
    ("same", "same(a, b): a === b, whether the two terms are identical (1 and 1.0 are not)"),
    ("not_same", "not_same(a, b): a !== b"),
    ("and", "and(a, b, ...): a and b, evaluated left to right, stopping at the first false"),
    ("or", "or(a, b, ...): a or b, evaluated left to right, stopping at the first true"),
    ("not", "not(a): the negation of a truth value"),
    ("map", "map(f, xs): [f(x1), f(x2), ...] for the list xs"),
    ("print", "print(x, ...): write the values as a line of output; the value is null"),
    ("rewrite", "rewrite(e, rule) or rewrite(e, [rules]): e with every part a rule matches replaced"),
    ("is_ground", "is_ground(e): whether e holds no unknown symbol"),
    ("complex", "complex(re, im): the complex number re + im*im"),
    ("expm1", "expm1(x): exp(x) - 1, accurate for small x"),
    ("log1p", "log1p(x): log(1 + x), accurate for small x"),
];

/// Natives that the syntax lowers to (definitions, blocks, control, bindings) or that serve other natives: not
/// functions a program calls by name, so the documentation leaves them out.
pub const INTERNAL: &[&str] = &[
    "assign",
    "ball_literal",
    "block",
    "check_options",
    "define",
    "define_fn",
    "define_rule",
    "define_sym",
    "fn",
    "for",
    "iadd",
    "if",
    "imod",
    "imul",
    "interpolated",
    "iquo",
    "isub",
    "let",
    "local_cell",
    "match",
    "option_value",
    "set",
    "set_delayed",
    "var",
    "while",
];

/// A `FUNCTIONS` list: names and one-line descriptions.
type Functions = &'static [(&'static str, &'static str)];

/// The libraries: topic, area and `FUNCTIONS` list, in the order the guides show the topics.
fn sources() -> Vec<(&'static str, Area, Functions)> {
    vec![
        ("Language", Area::Core, LANGUAGE),
        ("Lists", Area::Core, mc_lists::FUNCTIONS),
        ("Associations and sets", Area::Core, mc_assoc::FUNCTIONS),
        ("Strings", Area::Core, mc_strings::FUNCTIONS),
        ("Session", Area::Core, mc_engine::FUNCTIONS),
        ("Files", Area::Data, mc_io::FUNCTIONS),
        ("Random numbers", Area::Data, mc_random::FUNCTIONS),
        ("Distributions and statistics", Area::Data, mc_dist::FUNCTIONS),
        ("Graphics", Area::Visualisation, mc_graphics::FUNCTIONS),
        ("Plots", Area::Visualisation, mc_plot::FUNCTIONS),
        ("Calculus", Area::SymbolicNumeric, mc_calculus::FUNCTIONS),
        ("Polynomials", Area::SymbolicNumeric, mc_poly::FUNCTIONS),
        ("Simplification", Area::SymbolicNumeric, mc_simplify::FUNCTIONS),
        ("Equations", Area::SymbolicNumeric, mc_solve::FUNCTIONS),
        ("Numbers and precision", Area::SymbolicNumeric, mc_numeric::FUNCTIONS),
        ("Root finding", Area::SymbolicNumeric, mc_roots::FUNCTIONS),
        ("Numerical integration", Area::SymbolicNumeric, mc_quad::FUNCTIONS),
        ("Differential equations", Area::SymbolicNumeric, mc_ode::FUNCTIONS),
        ("Compiled functions", Area::SymbolicNumeric, mc_jit::FUNCTIONS),
        ("Number theory", Area::Higher, mc_numtheory::FUNCTIONS),
        ("Special functions", Area::Higher, mc_special::FUNCTIONS),
        ("Linear algebra", Area::Higher, mc_linalg::FUNCTIONS),
    ]
}

/// Every function, in topic order, each name once (the first library to list a name keeps it).
#[derive(Clone, Debug)]
pub struct Catalogue {
    /// the functions
    pub entries: Vec<Entry>,
}

impl Catalogue {
    /// The catalogue of the libraries this program was built with.
    pub fn new() -> Catalogue {
        let mut seen = std::collections::HashSet::new();
        let mut entries = vec![];
        for (topic, area, list) in sources() {
            for &(name, summary) in list {
                if seen.insert(name) {
                    entries.push(Entry { name, summary, area, topic });
                }
            }
        }
        Catalogue { entries }
    }

    /// The function `name`.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// The topics of an area, in order, with their functions sorted by name.
    pub fn topics(&self, area: Area) -> Vec<(&'static str, Vec<&Entry>)> {
        let mut out: Vec<(&'static str, Vec<&Entry>)> = vec![];
        for e in self.entries.iter().filter(|e| e.area == area) {
            match out.iter_mut().find(|(t, _)| *t == e.topic) {
                Some((_, v)) => v.push(e),
                None => out.push((e.topic, vec![e])),
            }
        }
        for (_, v) in &mut out {
            v.sort_by_key(|e| e.name);
        }
        out
    }
}

impl Default for Catalogue {
    fn default() -> Catalogue {
        Catalogue::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_area_has_functions() {
        let c = Catalogue::new();
        for a in Area::ALL {
            assert!(!c.topics(a).is_empty(), "{a:?}");
            assert_eq!(Area::from_key(a.key()), Some(a));
        }
        assert_eq!(c.get("n_integrate").map(|e| e.area), Some(Area::SymbolicNumeric));
        assert_eq!(c.get("table").map(|e| e.topic), Some("Lists"));
        assert_eq!(c.get("plot").map(|e| e.area), Some(Area::Visualisation));
        let mut names: Vec<_> = c.entries.iter().map(|e| e.name).collect();
        let n = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), n);
    }

    /// Every function the engine knows from a library is in the catalogue (a library added to the engine but not
    /// here would make its functions invisible in the documentation).
    #[test]
    fn the_catalogue_covers_the_engine() {
        let c = Catalogue::new();
        let mut engine = mc_engine::CoreEngine::new();
        let missing: Vec<String> = engine
            .name_kinds()
            .into_iter()
            .filter(|(n, k)| *k == mc_engine::NameKind::Function && c.get(n).is_none() && !n.contains('.'))
            .map(|(n, _)| n)
            .filter(|n| engine.has_native(n))
            .collect();
        let unexpected: Vec<_> = missing.iter().filter(|n| !INTERNAL.contains(&n.as_str())).collect();
        assert!(unexpected.is_empty(), "functions the catalogue does not list: {unexpected:?}");
        // and the language list names only functions that exist
        for (n, _) in LANGUAGE {
            assert!(engine.has_native(n), "{n}");
        }
    }
}
