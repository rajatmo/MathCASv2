//! SPDX licence expressions as crates declare them: identifiers joined by `AND`, `OR` and `WITH`, with
//! parentheses; the older `/` separator of crate manifests means `OR`.
use std::fmt;

/// A parsed licence expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// One licence identifier, such as `MIT`.
    Id(String),
    /// A licence with an exception, such as `Apache-2.0 WITH LLVM-exception`.
    With(String, String),
    /// Every operand applies.
    And(Vec<Expr>),
    /// Any one operand may be chosen.
    Or(Vec<Expr>),
}

/// The licences a shipped dependency may use, in order of preference when a crate offers a choice (round 11 brief,
/// release item 1). An exception (`WITH`) only adds permissions, so `X WITH e` is allowed when `X` is.
pub const ALLOWED: &[&str] = &[
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
    "0BSD",
    "Unlicense",
    "BSL-1.0",
    "Unicode-3.0",
    "MPL-2.0",
    "OFL-1.1",
];

/// Whether `id` is on [`ALLOWED`].
pub fn is_allowed(id: &str) -> bool {
    ALLOWED.contains(&id)
}

/// The deepest parenthesis nesting accepted.
const MAX_DEPTH: usize = 32;

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let join = |f: &mut fmt::Formatter<'_>, parts: &[Expr], op: &str| -> fmt::Result {
            for (i, p) in parts.iter().enumerate() {
                if i > 0 {
                    write!(f, " {op} ")?;
                }
                match p {
                    Expr::And(_) | Expr::Or(_) => write!(f, "({p})")?,
                    _ => write!(f, "{p}")?,
                }
            }
            Ok(())
        };
        match self {
            Expr::Id(id) => f.write_str(id),
            Expr::With(id, exception) => write!(f, "{id} WITH {exception}"),
            Expr::And(parts) => join(f, parts, "AND"),
            Expr::Or(parts) => join(f, parts, "OR"),
        }
    }
}

impl Expr {
    /// Whether the expression can be satisfied using only licences for which `ok` holds.
    pub fn satisfiable(&self, ok: &dyn Fn(&str) -> bool) -> bool {
        match self {
            Expr::Id(id) | Expr::With(id, _) => ok(id),
            Expr::And(parts) => parts.iter().all(|p| p.satisfiable(ok)),
            Expr::Or(parts) => parts.iter().any(|p| p.satisfiable(ok)),
        }
    }

    /// The licences to comply with when redistributing: every operand of `AND`, and for `OR` the operand
    /// satisfiable under `ok` whose first licence comes earliest in [`ALLOWED`]. Each entry is an identifier or
    /// `X WITH e`. `None` when the expression cannot be satisfied.
    pub fn choose(&self, ok: &dyn Fn(&str) -> bool) -> Option<Vec<String>> {
        match self {
            Expr::Id(id) => ok(id).then(|| vec![id.clone()]),
            Expr::With(id, exception) => ok(id).then(|| vec![format!("{id} WITH {exception}")]),
            Expr::And(parts) => {
                let mut all = Vec::new();
                for p in parts {
                    all.extend(p.choose(ok)?);
                }
                Some(all)
            }
            Expr::Or(parts) => parts
                .iter()
                .filter_map(|p| p.choose(ok))
                .min_by_key(|chosen| chosen.first().map_or(usize::MAX, |c| rank(c))),
        }
    }
}

fn rank(choice: &str) -> usize {
    let id = choice.split(" WITH ").next().unwrap_or(choice);
    // A licence with an exception ranks just after the plain licence.
    let with = usize::from(choice.contains(" WITH "));
    ALLOWED.iter().position(|a| *a == id).map_or(usize::MAX, |p| 2 * p + with)
}

/// Parse an expression.
pub fn parse(text: &str) -> Result<Expr, String> {
    let tokens = tokenize(text);
    let mut p = Parser { tokens: &tokens, pos: 0 };
    let e = p.or(0)?;
    if p.pos != tokens.len() {
        return Err(format!("unexpected '{}' in licence expression '{text}'", tokens[p.pos]));
    }
    Ok(e)
}

fn tokenize(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut word = String::new();
    for c in text.chars() {
        if c.is_whitespace() || matches!(c, '(' | ')' | '/') {
            if !word.is_empty() {
                tokens.push(std::mem::take(&mut word));
            }
            match c {
                '(' | ')' => tokens.push(c.to_string()),
                '/' => tokens.push("OR".into()),
                _ => {}
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        tokens.push(word);
    }
    tokens
}

struct Parser<'a> {
    tokens: &'a [String],
    pos: usize,
}

impl Parser<'_> {
    fn next_is(&self, t: &str) -> bool {
        self.tokens.get(self.pos).is_some_and(|x| x == t)
    }

    fn or(&mut self, depth: usize) -> Result<Expr, String> {
        let mut parts = vec![self.and(depth)?];
        while self.next_is("OR") {
            self.pos += 1;
            parts.push(self.and(depth)?);
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { Expr::Or(parts) })
    }

    fn and(&mut self, depth: usize) -> Result<Expr, String> {
        let mut parts = vec![self.with(depth)?];
        while self.next_is("AND") {
            self.pos += 1;
            parts.push(self.with(depth)?);
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { Expr::And(parts) })
    }

    fn with(&mut self, depth: usize) -> Result<Expr, String> {
        let atom = self.atom(depth)?;
        if !self.next_is("WITH") {
            return Ok(atom);
        }
        self.pos += 1;
        let Expr::Id(id) = atom else {
            return Err("WITH needs a single licence on its left".into());
        };
        let exception = self.identifier()?;
        Ok(Expr::With(id, exception))
    }

    fn atom(&mut self, depth: usize) -> Result<Expr, String> {
        if self.next_is("(") {
            if depth >= MAX_DEPTH {
                return Err("licence expression nested too deeply".into());
            }
            self.pos += 1;
            let e = self.or(depth + 1)?;
            if !self.next_is(")") {
                return Err("missing ')' in licence expression".into());
            }
            self.pos += 1;
            return Ok(e);
        }
        self.identifier().map(Expr::Id)
    }

    fn identifier(&mut self) -> Result<String, String> {
        match self.tokens.get(self.pos) {
            Some(t) if !matches!(t.as_str(), "(" | ")" | "AND" | "OR" | "WITH") => {
                self.pos += 1;
                Ok(t.clone())
            }
            Some(t) => Err(format!("expected a licence identifier, found '{t}'")),
            None => Err("licence expression ends early".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> Expr {
        Expr::Id(s.into())
    }

    #[test]
    fn parses_the_forms_crates_use() {
        assert_eq!(parse("MIT").unwrap(), id("MIT"));
        assert_eq!(parse("MIT/Apache-2.0").unwrap(), Expr::Or(vec![id("MIT"), id("Apache-2.0")]));
        assert_eq!(parse("Apache-2.0 / MIT").unwrap(), Expr::Or(vec![id("Apache-2.0"), id("MIT")]));
        assert_eq!(
            parse("(MIT OR Apache-2.0) AND Unicode-3.0").unwrap(),
            Expr::And(vec![Expr::Or(vec![id("MIT"), id("Apache-2.0")]), id("Unicode-3.0")])
        );
        assert_eq!(
            parse("Apache-2.0 WITH LLVM-exception").unwrap(),
            Expr::With("Apache-2.0".into(), "LLVM-exception".into())
        );
        // AND binds tighter than OR.
        assert_eq!(
            parse("MIT OR BSD-3-Clause AND Zlib").unwrap(),
            Expr::Or(vec![id("MIT"), Expr::And(vec![id("BSD-3-Clause"), id("Zlib")])])
        );
        let e = "(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0";
        assert_eq!(parse(e).unwrap().to_string(), e);
    }

    #[test]
    fn rejects_malformed_expressions() {
        assert!(parse("").is_err());
        assert!(parse("MIT OR").is_err());
        assert!(parse("(MIT").is_err());
        assert!(parse("MIT)").is_err());
        assert!(parse("(MIT OR Zlib) WITH x").is_err());
        assert!(parse(&format!("{}MIT{}", "(".repeat(40), ")".repeat(40))).is_err());
    }

    #[test]
    fn satisfiability_and_choice() {
        let allowed = |s: &str| is_allowed(s);
        assert!(parse("Apache-2.0 OR GPL-2.0-only").unwrap().satisfiable(&allowed));
        assert!(!parse("GPL-3.0-only").unwrap().satisfiable(&allowed));
        assert!(!parse("MIT AND GPL-2.0-only").unwrap().satisfiable(&allowed));
        assert!(parse("Apache-2.0 WITH LLVM-exception").unwrap().satisfiable(&allowed));
        assert_eq!(parse("Zlib OR Apache-2.0 OR MIT").unwrap().choose(&allowed), Some(vec!["MIT".into()]));
        assert_eq!(parse("Unlicense OR MIT").unwrap().choose(&allowed), Some(vec!["MIT".into()]));
        assert_eq!(
            parse("(MIT OR Apache-2.0) AND Unicode-3.0").unwrap().choose(&allowed),
            Some(vec!["MIT".into(), "Unicode-3.0".into()])
        );
        assert_eq!(
            parse("Apache-2.0 WITH LLVM-exception").unwrap().choose(&allowed),
            Some(vec!["Apache-2.0 WITH LLVM-exception".into()])
        );
        assert_eq!(parse("GPL-2.0-only").unwrap().choose(&allowed), None);
    }
}
