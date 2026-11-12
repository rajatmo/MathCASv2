//! String patterns (round 30, decisions S.6 to S.10): the pattern terms of the language read as patterns over the
//! characters of a string, and a backtracking matcher for them.
//!
//! | term | matches |
//! |---|---|
//! | `"abc"` | that text |
//! | `_`, `x_` | one character (bound to `x` as a one-character string) |
//! | `__`, `x__`, `___`, `x___` | a run of one or more / zero or more characters |
//! | `digit_character`, `letter_character`, `whitespace_character`, `word_character`, `punctuation_character`, `hexadecimal_character` | one character of that class |
//! | `whitespace`, `number_string` | a run of white space; the text of a decimal number (`-12`, `3.25`, `.5`) |
//! | `start_of_string`, `end_of_string`, `start_of_line`, `end_of_line`, `word_boundary` | the empty text at such a place |
//! | `string_expression(p, q, ...)` (`p ~~ q`) | `p`, then `q`, ... |
//! | `p \| q`, `[p, q, ...]` | `p` or `q` (tried in order) |
//! | `repeated(p)`, `repeated(p, n)`, `repeated(p, [n])`, `repeated(p, [m, n])` | `p` repeated (1 or more, 1 to n, n, m to n times; `inf`: no bound) |
//! | `x @ p` | what `p` matches, bound to `x` |
//! | `p when c` | `p`, then `c` gives `true` with the bindings so far |
//! | `x_ when not match_q(x, c)`, `x @ p when not match_q(x, c)` | one character / what `p` matches, when that text does not match the string pattern `c` (an exception) |
//! | any other compound term | its value, read as a pattern (`characters("aeiou")` is a list: alternatives) |
//! | `longest(p)`, `shortest(p)` | `p` with its runs and repetitions taking as much / as little as possible first |
//! | `regular_expression("...")` | a regular expression (the common syntax: `.`, `[...]`, `\d \w \s \b`, `* + ? {m,n}` with lazy `?`, `\|`, groups, `^ $`) |
//!
//! **Greedy by default (S.6).** Runs (`__`, `___`), repetitions and classes repeated take the longest text first:
//! a string pattern is matched like a regular expression, where `x__ ~~ __` on `"abcdefg"` gives `x = "abcdef"`.
//! `shortest(p)` makes the runs inside `p` take the shortest text first (the parts before it keep their own order:
//! `x__ ~~ longest(__)` still gives `x = "abcdef"`, as without the marker). (A term pattern's sequence variables take
//! the shortest run first, semantics 6.2: the two languages differ on purpose, as the strings' does in common use.)
//!
//! **The first match (S.7).** The matcher is a backtracking search over the pattern in the order the table gives
//! (alternatives in order, runs by length in the direction above); the first complete match is the result, as a
//! regular-expression engine's. A variable bound twice must bind the same text (`x_ ~~ x_` finds a doubled
//! character); bindings are strings.
//!
//! **Limits (S.8).** The search charges fuel by the steps it takes and polls the interrupt; repetitions of
//! compound patterns nest at most [`MAX_DEPTH`] deep (a longer repetition is `failure(limit, string_pattern)`),
//! repetitions of one character never nest.
use mc_core::{Store, Sym, View, T};
use mc_eval::{NativeCx, Res};

/// Most nested repetitions of a compound pattern (each holds stack frames).
pub(crate) const MAX_DEPTH: usize = 4000;

/// A class of single characters.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Class {
    Any,
    Digit,
    Letter,
    Space,
    Word,
    Punct,
    Hex,
    /// a capital letter
    Upper,
    /// one of these characters
    Set(Vec<char>),
    /// none of these classes
    Not(Vec<Class>),
}

impl Class {
    fn has(&self, c: char, ic: bool) -> bool {
        match self {
            Class::Any => true,
            Class::Digit => c.is_ascii_digit(),
            Class::Letter => c.is_alphabetic(),
            Class::Space => c.is_whitespace(),
            Class::Word => c.is_alphanumeric(),
            Class::Punct => is_punct(c),
            Class::Hex => c.is_ascii_hexdigit(),
            Class::Upper => c.is_uppercase(),
            Class::Set(cs) => cs.iter().any(|&d| eq(c, d, ic)),
            Class::Not(cs) => !cs.iter().any(|k| k.has(c, ic)),
        }
    }
}

/// Punctuation: the ASCII punctuation characters and the common Unicode ones (general punctuation block,
/// inverted marks, guillemets).
fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation() || matches!(c, '\u{2010}'..='\u{2027}' | '\u{2030}'..='\u{205E}' | '¡' | '¿' | '«' | '»' | '·')
}

fn eq(a: char, b: char, ic: bool) -> bool {
    a == b || (ic && a.to_lowercase().eq(b.to_lowercase()))
}

/// A place where the empty text matches.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Anchor {
    StartString,
    EndString,
    StartLine,
    EndLine,
    WordBoundary,
}

/// A compiled string pattern.
#[derive(Clone, Debug)]
pub(crate) enum Node<'h> {
    Lit(Vec<char>),
    One(Class),
    /// `min` to `max` characters of a class (no nesting)
    Run { class: Class, min: usize, max: usize },
    Cat(Vec<Node<'h>>),
    Alt(Vec<Node<'h>>),
    Rep { p: Box<Node<'h>>, min: usize, max: usize },
    Bind { var: usize, p: Box<Node<'h>> },
    Cond { p: Box<Node<'h>>, c: T<'h> },
    /// what `p` matches when that text does not match `not` as a whole
    Except { p: Box<Node<'h>>, not: Box<Node<'h>> },
    /// `longest(p)` (true), `shortest(p)` (false)
    Mode(Box<Node<'h>>, bool),
    At(Anchor),
}

/// The pattern variables of one pattern, by index.
pub(crate) struct Vars {
    pub syms: Vec<Sym>,
}

impl Vars {
    fn of(&mut self, s: Sym) -> usize {
        match self.syms.iter().position(|&x| x == s) {
            Some(i) => i,
            None => {
                self.syms.push(s);
                self.syms.len() - 1
            }
        }
    }
}

fn seq_run(min: usize) -> Node<'static> {
    Node::Run { class: Class::Any, min, max: usize::MAX }
}

/// The counts of `repeated(p, spec)`: `n` (1 to n), `[n]` (exactly n), `[m, n]`; `inf` for no bound.
fn counts<'h>(h: &'h mc_core::Heap, spec: T<'h>) -> Option<(usize, usize)> {
    let num = |t: T<'h>| match h.view(t) {
        View::Int(v) if v >= 0 => Some(v as usize),
        View::Sym(s) if h.sym_name(s) == "inf" => Some(usize::MAX),
        _ => None,
    };
    match h.view(spec) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "list") => match *args {
            [n] => num(n).map(|n| (n, n)),
            [m, n] => Some((num(m)?, num(n)?)),
            _ => None,
        },
        _ => num(spec).map(|n| (1, n)),
    }
}

/// The symbols that are string patterns by themselves (character classes, runs, anchors).
pub(crate) const CLASS_NAMES: &[&str] = &[
    "digit_character",
    "letter_character",
    "whitespace_character",
    "word_character",
    "punctuation_character",
    "hexadecimal_character",
    "whitespace",
    "number_string",
    "start_of_string",
    "end_of_string",
    "start_of_line",
    "end_of_line",
    "word_boundary",
];

/// Whether a symbol name is one of [`CLASS_NAMES`].
pub(crate) fn is_class_name(n: &str) -> bool {
    CLASS_NAMES.contains(&n)
}

/// How a compile evaluates a term that is not a pattern form (`characters("aeiou")` held inside a condition).
pub(crate) type Ev<'e, 'h> = &'e mut dyn FnMut(T<'h>) -> Res<T<'h>>;

/// The pattern of a term, or `None` when the term is not a string pattern (the caller then declines). A compound
/// term that is no pattern form is evaluated once (through `ev`) and its value read as a pattern.
pub(crate) fn compile<'h>(h: &'h mc_core::Heap, t: T<'h>, vars: &mut Vars, ev: Ev<'_, 'h>) -> Res<Option<Node<'h>>> {
    Compiler { h, vars, ev }.go(t, 0, false)
}

struct Compiler<'v, 'e, 'h> {
    h: &'h mc_core::Heap,
    vars: &'v mut Vars,
    ev: Ev<'e, 'h>,
}

/// The node of a compile step, or `Ok(None)` out of the enclosing function.
macro_rules! node {
    ($e:expr) => {
        match $e? {
            Some(n) => n,
            None => return Ok(None),
        }
    };
}

impl<'h> Compiler<'_, '_, 'h> {
    fn var(&mut self, x: T<'h>) -> Option<usize> {
        match self.h.view(x) {
            View::Sym(s) => Some(self.vars.of(s)),
            _ => None,
        }
    }

    fn all(&mut self, ps: &[T<'h>], d: usize) -> Res<Option<Vec<Node<'h>>>> {
        let mut ns = Vec::with_capacity(ps.len());
        for &p in ps {
            ns.push(node!(self.go(p, d, false)));
        }
        Ok(Some(ns))
    }

    /// `p when not match_q(x, c)` with `p` binding `x` (how an exception `except(c)`, `except(c, p)` is written):
    /// what `p` matches when that text does not match the string pattern `c`.
    fn except(&mut self, p: T<'h>, c: T<'h>, d: usize) -> Res<Option<Node<'h>>> {
        let h = self.h;
        let call = |t: T<'h>, f: &str| match h.view(t) {
            View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == f) => Some(args),
            _ => None,
        };
        let Some(&[m]) = call(c, "not") else { return Ok(None) };
        let Some(&[x, q]) = call(m, "match_q") else { return Ok(None) };
        let binds_x = match call(p, "any").or_else(|| call(p, "named")) {
            Some(args) => args.first() == Some(&x),
            None => false,
        };
        if !binds_x {
            return Ok(None);
        }
        let p = node!(self.go(p, d, false));
        let not = node!(self.go(q, d, false));
        Ok(Some(match (p, not) {
            // one character that is not one of a class: a class
            (Node::Bind { var, p }, Node::One(k)) if matches!(*p, Node::One(Class::Any)) => {
                Node::Bind { var, p: Box::new(Node::One(Class::Not(vec![k]))) }
            }
            (p, not) => Node::Except { p: Box::new(p), not: Box::new(not) },
        }))
    }

    fn go(&mut self, t: T<'h>, depth: usize, evaluated: bool) -> Res<Option<Node<'h>>> {
        if depth > 200 {
            return Ok(None);
        }
        let h = self.h;
        let d = depth + 1;
        Ok(Some(match h.view(t) {
            View::Str(s) => Node::Lit(s.chars().collect()),
            View::Sym(s) => match &*h.sym_name(s) {
            "digit_character" => Node::One(Class::Digit),
            "letter_character" => Node::One(Class::Letter),
            "whitespace_character" => Node::One(Class::Space),
            "word_character" => Node::One(Class::Word),
            "punctuation_character" => Node::One(Class::Punct),
            "hexadecimal_character" => Node::One(Class::Hex),
            "whitespace" => Node::Run { class: Class::Space, min: 1, max: usize::MAX },
            "number_string" => number_string(),
            "start_of_string" => Node::At(Anchor::StartString),
            "end_of_string" => Node::At(Anchor::EndString),
            "start_of_line" => Node::At(Anchor::StartLine),
            "end_of_line" => Node::At(Anchor::EndLine),
            "word_boundary" => Node::At(Anchor::WordBoundary),
            _ => return Ok(None),
        },
        View::App { head, args } => {
            let View::Sym(f) = h.view(head) else { return Ok(None) };
            let name = h.sym_name(f);
            match (&*name, args) {
                ("any", []) => Node::One(Class::Any),
                ("any", &[x]) => match self.var(x) {
                    Some(var) => Node::Bind { var, p: Box::new(Node::One(Class::Any)) },
                    None => return Ok(None),
                },
                ("seq1" | "seq0", []) => seq_run(usize::from(&*name == "seq1")),
                ("seq1" | "seq0", &[x]) => match self.var(x) {
                    Some(var) => Node::Bind { var, p: Box::new(seq_run(usize::from(&*name == "seq1"))) },
                    None => return Ok(None),
                },
                ("named", &[x, p]) => {
                    let Some(var) = self.var(x) else { return Ok(None) };
                    Node::Bind { var, p: Box::new(node!(self.go(p, d, false))) }
                }
                ("condition", &[p, c]) => match self.except(p, c, d)? {
                    Some(n) => n,
                    None => Node::Cond { p: Box::new(node!(self.go(p, d, false))), c },
                },
                ("alternatives" | "list", ps) => alternatives(node!(self.all(ps, d))),
                ("string_expression", ps) => Node::Cat(node!(self.all(ps, d))),
                ("repeated", &[p]) => repeat(node!(self.go(p, d, false)), 1, usize::MAX),
                ("repeated", &[p, spec]) => {
                    let Some((lo, hi)) = counts(h, spec) else { return Ok(None) };
                    repeat(node!(self.go(p, d, false)), lo, hi)
                }
                ("longest", &[p] | &[p, _]) => Node::Mode(Box::new(node!(self.go(p, d, false))), true),
                ("shortest", &[p] | &[p, _]) => Node::Mode(Box::new(node!(self.go(p, d, false))), false),
                ("regular_expression", &[r]) => match h.view(r) {
                    View::Str(s) => return Ok(crate::regex::parse(h, s, self.vars)),
                    _ => return Ok(None),
                },
                _ if !evaluated => {
                    let v = (self.ev)(t)?;
                    return self.go(v, d, true);
                }
                _ => return Ok(None),
            }
        }
        _ => return Ok(None),
    }))
    }
}

/// `p | q | ...`: one class when every alternative is one character.
fn alternatives(ns: Vec<Node<'_>>) -> Node<'_> {
    let mut chars = vec![];
    for n in &ns {
        match n {
            Node::Lit(cs) if cs.len() == 1 => chars.push(cs[0]),
            _ => return Node::Alt(ns),
        }
    }
    Node::One(Class::Set(chars))
}

/// `repeated(p, [min, max])`: a run when `p` is one character of a class, so long repetitions do not nest.
pub(crate) fn repeat(p: Node<'_>, min: usize, max: usize) -> Node<'_> {
    match p {
        Node::One(class) => Node::Run { class, min, max },
        Node::Lit(cs) if cs.len() == 1 => Node::Run { class: Class::Set(cs), min, max },
        p => Node::Rep { p: Box::new(p), min, max },
    }
}

/// `number_string`: an optional minus sign, then digits with an optional fractional part, or a point and digits.
fn number_string() -> Node<'static> {
    let digits = |min| Node::Run { class: Class::Digit, min, max: usize::MAX };
    let point = || Node::Lit(vec!['.']);
    Node::Cat(vec![
        Node::Run { class: Class::Set(vec!['-']), min: 0, max: 1 },
        Node::Alt(vec![
            Node::Cat(vec![digits(1), Node::Rep { p: Box::new(Node::Cat(vec![point(), digits(0)])), min: 0, max: 1 }]),
            Node::Cat(vec![point(), digits(1)]),
        ]),
    ])
}

/// The first character every match starts with, when there is one (a search skips other positions).
pub(crate) fn first_char(n: &Node<'_>) -> Option<char> {
    match n {
        Node::Lit(cs) => cs.first().copied(),
        Node::Cat(ns) => ns.first().and_then(first_char),
        Node::Bind { p, .. } | Node::Cond { p, .. } | Node::Except { p, .. } | Node::Mode(p, _) => first_char(p),
        Node::Rep { p, min, .. } if *min >= 1 => first_char(p),
        _ => None,
    }
}

/// A match: start and end (character positions, end exclusive) and the variables' spans.
#[derive(Clone, Debug)]
pub(crate) struct Hit {
    pub start: usize,
    pub end: usize,
    pub binds: Vec<Option<(usize, usize)>>,
}

/// What the continuation of a match does with an end position.
type K<'k, 'x, 'a, 'b, 'h> = &'k mut dyn FnMut(&mut M<'x, 'a, 'b, 'h>, usize) -> Res<bool>;

/// The matcher of one pattern over one text.
pub(crate) struct M<'x, 'a, 'b, 'h> {
    pub text: &'x [char],
    pub ic: bool,
    pub binds: Vec<Option<(usize, usize)>>,
    pub vars: &'x [Sym],
    pub cx: &'x mut NativeCx<'a, 'b, 'h>,
    steps: u64,
    depth: usize,
    /// a repetition nested past [`MAX_DEPTH`]: the search stops, the caller gives a failure
    pub over: bool,
}

/// Steps between two charges of fuel.
const STEP_BATCH: u64 = 1024;

impl<'x, 'a, 'b, 'h> M<'x, 'a, 'b, 'h> {
    pub fn new(text: &'x [char], ic: bool, vars: &'x [Sym], cx: &'x mut NativeCx<'a, 'b, 'h>) -> Self {
        M { text, ic, binds: vec![None; vars.len()], vars, cx, steps: 0, depth: 0, over: false }
    }

    fn step(&mut self) -> Res<()> {
        self.steps += 1;
        if self.steps.is_multiple_of(STEP_BATCH) {
            self.cx.tick(STEP_BATCH / 16)?;
        }
        Ok(())
    }

    /// The text of a span as a string.
    pub fn slice(&self, a: usize, b: usize) -> String {
        self.text[a..b].iter().collect()
    }

    /// `body` with each bound variable replaced by its text, evaluated.
    pub fn apply(&mut self, body: T<'h>) -> Res<T<'h>> {
        let h = self.cx.heap();
        let mut vals = vec![];
        let mut pats = vec![];
        for (i, b) in self.binds.iter().enumerate() {
            if let Some((a, e)) = *b {
                vals.push(h.string(&self.slice(a, e)));
                pats.push(h.call(h.sym("any"), &[h.sym_term(self.vars[i])]));
            }
        }
        if vals.is_empty() {
            return self.cx.eval(body);
        }
        let list = h.sym(mc_api::names::LIST);
        let rule = h.call(h.sym(mc_api::names::RULE_DELAYED), &[h.call(list, &pats), body]);
        let t = h.call(h.sym("replace"), &[h.call(list, &vals), rule]);
        self.cx.eval(t)
    }

    fn holds(&self, a: Anchor, pos: usize) -> bool {
        let n = self.text.len();
        match a {
            Anchor::StartString => pos == 0,
            Anchor::EndString => pos == n,
            Anchor::StartLine => pos == 0 || self.text[pos - 1] == '\n',
            Anchor::EndLine => pos == n || self.text[pos] == '\n',
            Anchor::WordBoundary => {
                let w = |i: usize| self.text.get(i).is_some_and(|c| c.is_alphanumeric() || *c == '_');
                (pos > 0 && w(pos - 1)) != w(pos)
            }
        }
    }

    /// Match `n` at `pos`, then the continuation; `long`: runs take the longest text first.
    pub fn m(&mut self, n: &'x Node<'h>, pos: usize, long: bool, k: K<'_, 'x, 'a, 'b, 'h>) -> Res<bool> {
        if self.over {
            return Ok(false);
        }
        self.step()?;
        let len = self.text.len();
        match n {
            Node::Lit(cs) => {
                let ok = pos + cs.len() <= len && cs.iter().zip(&self.text[pos..]).all(|(&a, &b)| eq(b, a, self.ic));
                if ok {
                    k(self, pos + cs.len())
                } else {
                    Ok(false)
                }
            }
            Node::One(c) => {
                if pos < len && c.has(self.text[pos], self.ic) {
                    k(self, pos + 1)
                } else {
                    Ok(false)
                }
            }
            Node::Run { class, min, max } => {
                let mut avail = 0;
                while avail < *max && pos + avail < len && class.has(self.text[pos + avail], self.ic) {
                    avail += 1;
                }
                if avail < *min {
                    return Ok(false);
                }
                if long {
                    for l in (*min..=avail).rev() {
                        self.step()?;
                        if k(self, pos + l)? {
                            return Ok(true);
                        }
                    }
                } else {
                    for l in *min..=avail {
                        self.step()?;
                        if k(self, pos + l)? {
                            return Ok(true);
                        }
                    }
                }
                Ok(false)
            }
            Node::Cat(ns) => self.cat(ns, pos, long, k),
            Node::Alt(ns) => {
                for p in ns {
                    if self.m(p, pos, long, k)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            Node::Rep { p, min, max } => self.rep(p, 0, *min, *max, pos, long, k),
            Node::Bind { var, p } => {
                let v = *var;
                if let Some((a, e)) = self.binds[v] {
                    // a variable bound before: the same text again
                    let l = e - a;
                    let same = pos + l <= len && (0..l).all(|i| eq(self.text[pos + i], self.text[a + i], self.ic));
                    return if same { k(self, pos + l) } else { Ok(false) };
                }
                self.m(p, pos, long, &mut |s, q| {
                    s.binds[v] = Some((pos, q));
                    let r = k(s, q);
                    s.binds[v] = None;
                    r
                })
            }
            Node::Cond { p, c } => {
                let c = *c;
                self.m(p, pos, long, &mut |s, q| {
                    let v = s.apply(c)?;
                    if s.cx.is_true(v) {
                        k(s, q)
                    } else {
                        Ok(false)
                    }
                })
            }
            Node::Except { p, not } => self.m(p, pos, long, &mut |s, q| {
                if s.whole(not, pos, q)? {
                    Ok(false)
                } else {
                    k(s, q)
                }
            }),
            Node::Mode(p, l) => self.m(p, pos, *l, k),
            Node::At(a) => {
                if self.holds(*a, pos) {
                    k(self, pos)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// Whether `n` matches exactly the text from `a` to `b` (with the bindings so far).
    fn whole(&mut self, n: &'x Node<'h>, a: usize, b: usize) -> Res<bool> {
        let text = self.text;
        let mut sub = M {
            text: &text[a..b],
            ic: self.ic,
            binds: vec![None; self.binds.len()],
            vars: self.vars,
            cx: &mut *self.cx,
            steps: 0,
            depth: self.depth,
            over: false,
        };
        let r = sub.m(n, 0, true, &mut |s, q| Ok(q == s.text.len()))?;
        self.over |= sub.over;
        Ok(r)
    }

    fn cat(&mut self, ns: &'x [Node<'h>], pos: usize, long: bool, k: K<'_, 'x, 'a, 'b, 'h>) -> Res<bool> {
        match ns.split_first() {
            None => k(self, pos),
            Some((n, [])) => self.m(n, pos, long, k),
            Some((n, rest)) => self.m(n, pos, long, &mut |s, q| s.cat(rest, q, long, k)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rep(
        &mut self,
        p: &'x Node<'h>,
        count: usize,
        min: usize,
        max: usize,
        pos: usize,
        long: bool,
        k: K<'_, 'x, 'a, 'b, 'h>,
    ) -> Res<bool> {
        if self.depth >= MAX_DEPTH {
            self.over = true;
            return Ok(false);
        }
        let more = |s: &mut Self, k: K<'_, 'x, 'a, 'b, 'h>| -> Res<bool> {
            if count >= max {
                return Ok(false);
            }
            s.depth += 1;
            let r = s.m(p, pos, long, &mut |s, q| {
                if q == pos {
                    // an empty repetition ends the run
                    if count + 1 >= min {
                        k(s, q)
                    } else {
                        Ok(false)
                    }
                } else {
                    s.rep(p, count + 1, min, max, q, long, k)
                }
            });
            s.depth -= 1;
            r
        };
        if long {
            if more(self, k)? {
                return Ok(true);
            }
            if count >= min {
                return k(self, pos);
            }
            Ok(false)
        } else {
            if count >= min && k(self, pos)? {
                return Ok(true);
            }
            more(self, k)
        }
    }

    /// The first match of `n` starting at `start`, ending at the end of the text if `whole`.
    pub fn at(&mut self, n: &'x Node<'h>, start: usize, whole: bool) -> Res<Option<Hit>> {
        let mut hit = None;
        let len = self.text.len();
        self.m(n, start, true, &mut |s, q| {
            if whole && q != len {
                return Ok(false);
            }
            hit = Some(Hit { start, end: q, binds: s.binds.clone() });
            Ok(true)
        })?;
        Ok(hit)
    }

    /// Every distinct end of a match of `n` starting at `start`, in the order the search finds them.
    pub fn all_at(&mut self, n: &'x Node<'h>, start: usize) -> Res<Vec<Hit>> {
        let mut hits: Vec<Hit> = vec![];
        self.m(n, start, true, &mut |s, q| {
            if !hits.iter().any(|x| x.end == q) {
                hits.push(Hit { start, end: q, binds: s.binds.clone() });
            }
            Ok(false)
        })?;
        Ok(hits)
    }
}
