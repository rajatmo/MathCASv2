//! The notation of the rule files (`rules/*.rules`) and its parser.
//!
//! A rule file is a list of rules, one per line (a line that starts with white space continues the one before):
//!
//! ```text
//! ## 4.1.0.1 products of powers of sines and cosines        (a section heading)
//! # a comment
//! int((a_. * sin(e_. + f_. * x))^m_. * (b_. * cos(e_. + f_. * x))^n_.)
//!     := (a*sin(e + f*x))^(m + 1) * (b*cos(e + f*x))^(n + 1)/(a*b*f*(m + 1))
//!     when free(a, b, e, f, m, n) and eq(m + n + 2, 0) and ne(m, -1)
//! ```
//!
//! The left side is `int(p)` with `p` a pattern in v2's pattern language: `a_` is a pattern variable, `a_.` an
//! optional argument whose default is the unit of its place (1 in a product and as an exponent, 0 in a sum: v2's
//! `optional(a_, 1)` and `optional(a_, 0)`), and `x` the variable of integration. The right side and the condition
//! use the variables without the underscore. Operators: `+ - * / ^`, the comparisons `== != < <= > >=`, `and`,
//! `or`, `not`; numbers are integers (`1/2` is a rational, as in the language). Function names are the language's
//! (`sin`, `hypergeometric_2f1`, ...) or the utilities of [`crate::util`] (`int`, `subst`, `free`, `integer`, ...);
//! names are checked when the table is built.
use std::fmt;

/// A node of a rule's left side, right side or condition. Owned by the table (no heap terms), so the table is built
/// once per process.
#[derive(Clone, Debug, PartialEq)]
pub enum N {
    /// an integer
    Int(i64),
    /// the variable of integration
    X,
    /// pattern variable `i`
    Var(u16),
    /// optional pattern variable `i` (left sides only)
    Opt(u16),
    /// a constant symbol (`pi`, `e`, `im`)
    Sym(Box<str>),
    /// a call of a function or utility
    Call(Box<str>, Vec<N>),
    /// a sum
    Add(Vec<N>),
    /// a product
    Mul(Vec<N>),
    /// a power
    Pow(Box<N>, Box<N>),
    /// a comparison: `==`, `!=`, `<`, `<=`, `>`, `>=`
    Cmp(Cmp, Box<N>, Box<N>),
    /// conjunction
    And(Vec<N>),
    /// disjunction
    Or(Vec<N>),
    /// negation
    Not(Box<N>),
}

/// A comparison operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

/// One rule as written.
#[derive(Clone, Debug)]
pub struct RuleText {
    /// the section heading it is under
    pub section: String,
    /// its line in the file (1-based)
    pub line: usize,
    /// the pattern (inside `int(...)`)
    pub lhs: N,
    /// the right side
    pub rhs: N,
    /// the condition, if any
    pub cond: Option<N>,
    /// the names of the pattern variables, by number
    pub vars: Vec<String>,
}

/// A parse error.
#[derive(Debug, Clone)]
pub struct ParseError {
    /// the file's line
    pub line: usize,
    /// what is wrong
    pub msg: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Num(i64),
    Id(String),
    /// `a_` (false) or `a_.` (true)
    PVar(String, bool),
    Op(&'static str),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut out = vec![];
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c.is_ascii_digit() {
            let st = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push(Tok::Num(s[st..i].parse().map_err(|_| "number too large".to_string())?));
            continue;
        }
        if c.is_ascii_alphabetic() {
            let st = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            let w = &s[st..i];
            if let Some(name) = w.strip_suffix('_') {
                let opt = i < b.len() && b[i] == b'.';
                if opt {
                    i += 1;
                }
                out.push(Tok::PVar(name.to_string(), opt));
            } else {
                out.push(Tok::Id(w.to_string()));
            }
            continue;
        }
        let two = if i + 1 < b.len() { &s[i..i + 2] } else { "" };
        let op: &'static str = match two {
            ":=" => ":=",
            "==" => "==",
            "!=" => "!=",
            "<=" => "<=",
            ">=" => ">=",
            _ => match c {
                b'+' => "+",
                b'-' => "-",
                b'*' => "*",
                b'/' => "/",
                b'^' => "^",
                b'(' => "(",
                b')' => ")",
                b',' => ",",
                b'<' => "<",
                b'>' => ">",
                _ => return Err(format!("unexpected character {:?}", c as char)),
            },
        };
        i += op.len();
        out.push(Tok::Op(op));
    }
    Ok(out)
}

struct P<'a> {
    t: &'a [Tok],
    i: usize,
    vars: Vec<String>,
    /// reading the left side: pattern variables are introduced
    lhs: bool,
}

impl P<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.peek(), Some(Tok::Op(o)) if *o == op)
    }
    fn is_id(&self, w: &str) -> bool {
        matches!(self.peek(), Some(Tok::Id(o)) if o == w)
    }
    fn expect(&mut self, op: &str) -> Result<(), String> {
        if self.is_op(op) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {op:?} at token {} ({:?})", self.i, self.peek()))
        }
    }
    fn or(&mut self) -> Result<N, String> {
        let mut v = vec![self.and()?];
        while self.is_id("or") {
            self.i += 1;
            v.push(self.and()?);
        }
        Ok(if v.len() == 1 { v.pop().unwrap() } else { N::Or(v) })
    }
    fn and(&mut self) -> Result<N, String> {
        let mut v = vec![self.not()?];
        while self.is_id("and") {
            self.i += 1;
            v.push(self.not()?);
        }
        Ok(if v.len() == 1 { v.pop().unwrap() } else { N::And(v) })
    }
    fn not(&mut self) -> Result<N, String> {
        if self.is_id("not") {
            self.i += 1;
            return Ok(N::Not(Box::new(self.not()?)));
        }
        self.cmp()
    }
    fn cmp(&mut self) -> Result<N, String> {
        let a = self.sum()?;
        let op = match self.peek() {
            Some(Tok::Op("==")) => Cmp::Eq,
            Some(Tok::Op("!=")) => Cmp::Ne,
            Some(Tok::Op("<")) => Cmp::Lt,
            Some(Tok::Op("<=")) => Cmp::Le,
            Some(Tok::Op(">")) => Cmp::Gt,
            Some(Tok::Op(">=")) => Cmp::Ge,
            _ => return Ok(a),
        };
        self.i += 1;
        let b = self.sum()?;
        Ok(N::Cmp(op, Box::new(a), Box::new(b)))
    }
    fn sum(&mut self) -> Result<N, String> {
        let mut v = vec![self.product()?];
        loop {
            if self.is_op("+") {
                self.i += 1;
                v.push(self.product()?);
            } else if self.is_op("-") {
                self.i += 1;
                v.push(neg(self.product()?));
            } else {
                break;
            }
        }
        Ok(if v.len() == 1 { v.pop().unwrap() } else { N::Add(v) })
    }
    fn product(&mut self) -> Result<N, String> {
        let mut v = vec![self.unary()?];
        loop {
            if self.is_op("*") {
                self.i += 1;
                v.push(self.unary()?);
            } else if self.is_op("/") {
                self.i += 1;
                let d = self.unary()?;
                v.push(recip(d));
            } else {
                break;
            }
        }
        Ok(if v.len() == 1 { v.pop().unwrap() } else { N::Mul(v) })
    }
    fn unary(&mut self) -> Result<N, String> {
        if self.is_op("-") {
            self.i += 1;
            return Ok(neg(self.unary()?));
        }
        self.power()
    }
    fn power(&mut self) -> Result<N, String> {
        let b = self.primary()?;
        if self.is_op("^") {
            self.i += 1;
            let e = self.unary()?;
            return Ok(N::Pow(Box::new(b), Box::new(e)));
        }
        Ok(b)
    }
    fn primary(&mut self) -> Result<N, String> {
        let t = self.peek().cloned().ok_or("unexpected end")?;
        self.i += 1;
        match t {
            Tok::Num(v) => Ok(N::Int(v)),
            Tok::Op("(") => {
                let e = self.or()?;
                self.expect(")")?;
                Ok(e)
            }
            Tok::PVar(name, opt) => {
                if !self.lhs {
                    return Err(format!("pattern variable {name}_ outside the left side"));
                }
                let i = self.var(&name, true)?;
                Ok(if opt { N::Opt(i) } else { N::Var(i) })
            }
            Tok::Id(w) => {
                if self.is_op("(") {
                    self.i += 1;
                    let mut args = vec![];
                    if !self.is_op(")") {
                        loop {
                            args.push(self.or()?);
                            if self.is_op(",") {
                                self.i += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    self.expect(")")?;
                    return Ok(N::Call(w.into(), args));
                }
                if w == "x" {
                    return Ok(N::X);
                }
                if let Some(i) = self.vars.iter().position(|v| *v == w) {
                    return Ok(N::Var(i as u16));
                }
                if matches!(w.as_str(), "pi" | "e" | "im") {
                    return Ok(N::Sym(w.into()));
                }
                if self.lhs {
                    return Err(format!("{w}: a name in a pattern must be a pattern variable ({w}_) or x"));
                }
                Ok(N::Var(self.var(&w, false)?))
            }
            Tok::Op(o) => Err(format!("unexpected {o:?}")),
        }
    }
    fn var(&mut self, name: &str, create: bool) -> Result<u16, String> {
        if let Some(i) = self.vars.iter().position(|v| v == name) {
            return Ok(i as u16);
        }
        if !create {
            return Err(format!("{name} is not a pattern variable of the rule"));
        }
        self.vars.push(name.to_string());
        Ok((self.vars.len() - 1) as u16)
    }
}

/// `-a`
pub fn neg(a: N) -> N {
    match a {
        N::Int(v) => N::Int(-v),
        a => N::Mul(vec![N::Int(-1), a]),
    }
}

/// `1/a`
pub fn recip(a: N) -> N {
    N::Pow(Box::new(a), Box::new(N::Int(-1)))
}

/// Parse a rule file.
pub fn parse_file(text: &str) -> Result<Vec<RuleText>, ParseError> {
    // join continuation lines
    let mut stmts: Vec<(usize, String, String)> = vec![];
    let mut section = String::new();
    for (k, line) in text.lines().enumerate() {
        let t = line.trim_end();
        if let Some(h) = t.strip_prefix("##") {
            section = h.trim().to_string();
            continue;
        }
        if t.trim_start().starts_with('#') || t.trim().is_empty() {
            continue;
        }
        if t.starts_with(' ') || t.starts_with('\t') {
            if let Some(last) = stmts.last_mut() {
                last.2.push(' ');
                last.2.push_str(t.trim());
                continue;
            }
        }
        stmts.push((k + 1, section.clone(), t.to_string()));
    }
    stmts.into_iter().map(|(line, section, s)| parse_rule(&s, line, section)).collect()
}

fn parse_rule(s: &str, line: usize, section: String) -> Result<RuleText, ParseError> {
    let err = |msg: String| ParseError { line, msg };
    let toks = lex(s).map_err(err)?;
    let split = toks.iter().position(|t| *t == Tok::Op(":=")).ok_or_else(|| err("no :=".into()))?;
    let when = toks.iter().position(|t| *t == Tok::Id("when".into())).unwrap_or(toks.len());
    if when < split {
        return Err(err("when before :=".into()));
    }
    let mut p = P { t: &toks[..split], i: 0, vars: vec![], lhs: true };
    let lhs = p.or().map_err(err)?;
    if p.i != split {
        return Err(err("trailing tokens on the left side".into()));
    }
    let N::Call(f, mut args) = lhs else {
        return Err(err("the left side must be int(pattern)".into()));
    };
    if &*f != "int" || args.len() != 1 {
        return Err(err("the left side must be int(pattern)".into()));
    }
    let lhs = args.pop().unwrap();
    let vars = p.vars;
    let mut p = P { t: &toks[split + 1..when], i: 0, vars, lhs: false };
    let rhs = p.or().map_err(err)?;
    if p.i != when - split - 1 {
        return Err(err(format!("trailing tokens on the right side at {}", p.i)));
    }
    let cond = if when < toks.len() {
        let mut q = P { t: &toks[when + 1..], i: 0, vars: p.vars.clone(), lhs: false };
        let c = q.or().map_err(err)?;
        if q.i != toks.len() - when - 1 {
            return Err(err("trailing tokens in the condition".into()));
        }
        Some(c)
    } else {
        None
    };
    Ok(RuleText { section, line, lhs, rhs, cond, vars: p.vars })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_rule() {
        let r = parse_file(
            "## s\nint((a_. * sin(e_. + f_. * x))^m_.) := a*m/(f + 1)\n    when free(a, e, f, m) and m != -1\n",
        )
        .unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].vars, ["a", "e", "f", "m"]);
        assert_eq!(r[0].section, "s");
        assert!(r[0].cond.is_some());
        assert!(matches!(r[0].lhs, N::Pow(..)));
    }

    #[test]
    fn rejects_unknown_names() {
        assert!(parse_file("int(sin(x)) := y\n").is_err());
        assert!(parse_file("int(sin(a)) := 1\n").is_err());
    }
}
