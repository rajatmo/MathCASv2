//! The pattern and definition terms that `mc-syntax` lowers source to (its decisions C.8 to C.10, C.12), turned
//! into the evaluator's patterns and definitions (decision F.14). With these natives registered, the glue between
//! the parser and the evaluator only evaluates lowered terms.
//!
//! Patterns ([`pattern`]):
//!
//! | term | pattern |
//! |---|---|
//! | `any()`, `any(x)` | `_`, `x_` |
//! | `seq1()`, `seq1(x)`, `seq0()`, `seq0(x)` | `__`, `x__`, `___`, `x___` |
//! | `typed(any(x), T)` | `x_ : T`, for the atomic types of semantics 16 (`integer`, `rational`, `machine_real`, `ball`, `real`, `number`, `string`, `symbol`; also capitalised, and `Int`) |
//! | `named(x, p)` | `x @ p` |
//! | `condition(p, c)` | `p /; c`: `c` is a term evaluated with the bindings substituted; it may mention only variables bound in `p` |
//! | `alternatives(p, q, ...)` | `p \| q \| ...` |
//! | `key_value_pattern([p1, ...])`, `key_value_pattern(p)` | an association or list of rules with distinct entries matching `p1`, ... (C9B.11) |
//! | `repeated(p)`, `repeated(p, n)`, `repeated(p, [n])`, `repeated(p, [m, n])` | a run of 1 to any, 1 to `n`, exactly `n`, `m` to `n` (`inf`: any number) arguments each matching `p` (C9B.4) |
//! | `held(p)` | `p` (the argument position is held: see the definitions below) |
//! | `longest(r)`, `longest(r, k)`, `shortest(r)`, `shortest(r, k)` | the run pattern `r` (`xs__`, `xs___`, `repeated(...)`) tried longest / shortest first, before the unmarked runs, higher priority `k` first (decision r30.4); on another pattern, that pattern |
//! | any other compound or atom | itself, with patterns inside |
//!
//! `optional(p, d)` as a direct argument of a definition's left side defines a rule per case (below); elsewhere,
//! with `p` a named blank, it is an argument that may be missing, `x` then bound to `d` (C9B.13).
//!
//! Definitions, as natives (effects, so ordered like every other effect; a malformed definition evaluates to
//! `failure(definition, message)`):
//!
//! | term | effect |
//! |---|---|
//! | `set_delayed(s, e)` | own value `e`, evaluated when `s` is read |
//! | `set_delayed(lhs, e)` | rule `lhs := e` |
//! | `set(s, v)`, `set(lhs, v)` | `v` evaluated first, then as above |
//! | `define_rule(rule_delayed(L, R), list(decs))`, `define_rule(rule(L, R), list(decs))` | rule `L := R` (with `rule`, `R` evaluated first); an open rule set |
//! | `define_fn(lhs, body, list(opts), result_type, list(decs))` | rule `lhs := body`, plus the catch-all `f(args___) := failure(no_match, f, args)` (04 section 6.2); the result type is not checked yet |
//! | `define_sym(s, list(decs))` | the decorators' properties |
//!
//! Left sides: a direct argument `held(p)` holds that position of the head (a held sequence variable, every
//! position from there on; decision H.6); a direct argument `optional(p, d)` (`p` a named blank, possibly typed)
//! defines the rule with the argument and one without it whose right side (and condition) is
//! `block(let(x, d), ...)` (H.7); options `optional(p, d)` after `;` add a variant with a trailing
//! `options(...)` argument, read by `option_value` and checked by `check_options` (H.8).
//!
//! Decorators (04 section 6.4): `listable`, `effect`, `held` (all arguments), `handles_failures`, `uses_assumptions`,
//! `associative` with `commutative`, `identity(e)` (with both), on the head symbol; `on(s)` stores the rules with `s` (an
//! argument-head rule, semantics 6.5). On a `fn`, `compiled` evaluates `compile(f)` after the definition (the numeric
//! compiler, item 1b): the definition's value is `null` when that gives `f`, else what it gave. Others give
//! `failure(definition, ...)`.
//!
//! Right sides are terms in which the variables are written as their symbols; a sequence variable stands for the
//! list of its terms (semantics 6.1); substitution into them is hygienic (6.6).
use crate::defs::{Attributes, Hold};
use crate::eval::{Kernel, Res};
use crate::index::RuleRhs;
use crate::native::{Native, NativeCx, Reply};
use crate::pat::{alt, blank, cond_term, named, seq0, seq1, typed, var, DefError, Pat, Ty, MAX_PATTERN_DEPTH};
use mc_core::{Heap, Raw, Store, Sym, View, T};

/// The head symbols of the shapes.
#[derive(Clone, Copy)]
struct Heads {
    any: Sym,
    seq1: Sym,
    seq0: Sym,
    typed: Sym,
    named: Sym,
    optional: Sym,
    held: Sym,
    condition: Sym,
    alternatives: Sym,
    repeated: Sym,
    key_value: Sym,
    /// `verbatim(p)`: the term p itself, its pattern constructs matched literally
    verbatim: Sym,
    /// `hold_pattern(p)`: the pattern p (held where it is written, so it is not evaluated)
    hold_pattern: Sym,
    longest: Sym,
    shortest: Sym,
    rule: Sym,
    rule_delayed: Sym,
    let_: Sym,
    block: Sym,
}

impl Heads {
    fn new(h: &Heap) -> Heads {
        Heads {
            any: h.sym("any"),
            seq1: h.sym("seq1"),
            seq0: h.sym("seq0"),
            typed: h.sym("typed"),
            named: h.sym("named"),
            optional: h.sym("optional"),
            held: h.sym("held"),
            condition: h.sym("condition"),
            alternatives: h.sym("alternatives"),
            repeated: h.sym("repeated"),
            key_value: h.sym("key_value_pattern"),
            verbatim: h.sym("verbatim"),
            hold_pattern: h.sym("hold_pattern"),
            longest: h.sym("longest"),
            shortest: h.sym("shortest"),
            rule: h.sym("rule"),
            rule_delayed: h.sym("rule_delayed"),
            let_: h.sym("let"),
            block: h.sym("block"),
        }
    }

    /// Mask of the pattern heads: a term without any of these bits contains no pattern.
    fn mask(&self, h: &Heap) -> u64 {
        [
            self.any,
            self.seq1,
            self.seq0,
            self.typed,
            self.named,
            self.optional,
            self.held,
            self.condition,
            self.alternatives,
            self.repeated,
            self.key_value,
            self.verbatim,
            self.hold_pattern,
        ]
        .iter()
            .fold(0, |m, &s| m | h.mask(h.sym_term(s)))
    }
}

/// A pattern lowered from a term, with the symbol of each variable (`names[v]` is variable `v`).
pub struct Lowered {
    /// the pattern (not yet compiled: pass it to a definition)
    pub pat: Pat,
    /// the variables' symbols, by variable number
    pub names: Vec<Sym>,
}

fn sym_of<'h>(h: &'h Heap, t: T<'h>) -> Option<Sym> {
    match h.view(t) {
        View::Sym(s) => Some(s),
        _ => None,
    }
}

/// The call `t` as `(head symbol, arguments)`.
fn call_of<'h>(h: &'h Heap, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => sym_of(h, head).map(|s| (s, args)),
        _ => None,
    }
}

/// The symbols occurring in `t` (iteratively, each subterm once).
fn symbols<'h>(h: &'h Heap, t: T<'h>, out: &mut Vec<Sym>) {
    let mut seen = rustc_hash::FxHashSet::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(s) => out.push(s),
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
}

pub(crate) fn type_of(h: &Heap, t: T<'_>) -> Result<Ty, DefError> {
    // `Head[f]`: a compound term with head `f` (the type language of 04 section 9; C9B.14)
    if let Some((s, &[f])) = call_of(h, t) {
        if h.sym_name(s) == "Head" {
            return sym_of(h, f).map(Ty::Head).ok_or(DefError::UnknownType);
        }
    }
    let Some(s) = sym_of(h, t) else { return Err(DefError::UnknownType) };
    Ok(match h.sym_name(s).as_str() {
        "integer" | "Integer" | "Int" => Ty::Integer,
        "rational" | "Rational" => Ty::Rational,
        "machine_real" | "MachineReal" => Ty::MachineReal,
        "ball" | "Ball" => Ty::Ball,
        "real" | "Real" => Ty::Real,
        "number" | "Number" => Ty::Number,
        "string" | "String" | "Str" => Ty::String,
        "symbol" | "Symbol" | "Sym" => Ty::Symbol,
        "Rat" => Ty::Rational,
        "Float" => Ty::Float,
        "Complex" => Ty::Complex,
        _ => return Err(DefError::UnknownType),
    })
}

struct Lower<'a, 'h> {
    h: &'h Heap,
    hd: &'a Heads,
    mask: u64,
    names: Vec<Sym>,
}

impl<'h> Lower<'_, 'h> {
    fn var(&mut self, x: T<'h>) -> Result<u16, DefError> {
        let s = sym_of(self.h, x).ok_or(DefError::Unsupported("a pattern variable that is not a symbol"))?;
        Ok(match self.names.iter().position(|&n| n == s) {
            Some(i) => i as u16,
            None => {
                self.names.push(s);
                (self.names.len() - 1) as u16
            }
        })
    }

    /// The names of the variables bound anywhere in pattern term `t` (not inside condition bodies or types).
    fn collect(&mut self, t: T<'h>) -> Result<(), DefError> {
        let (h, hd) = (self.h, self.hd);
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if h.mask(u) & self.mask == 0 {
                continue;
            }
            let View::App { head, args } = h.view(u) else { continue };
            match sym_of(h, head) {
                Some(s) if (s == hd.any || s == hd.seq1 || s == hd.seq0) && args.len() == 1 => {
                    self.var(args[0])?;
                }
                Some(s) if s == hd.named && args.len() == 2 => {
                    self.var(args[0])?;
                    stack.push(args[1]);
                }
                Some(s) if (s == hd.condition || s == hd.typed) && args.len() == 2 => stack.push(args[0]),
                // the pattern constructs inside verbatim(p) are literal: they bind nothing
                Some(s) if s == hd.verbatim && args.len() == 1 => {}
                _ => {
                    stack.push(head);
                    stack.extend_from_slice(args);
                }
            }
        }
        Ok(())
    }

    fn convert(&mut self, t: T<'h>, depth: usize) -> Result<Pat, DefError> {
        let (h, hd) = (self.h, self.hd);
        if depth > MAX_PATTERN_DEPTH {
            return Err(DefError::TooLarge);
        }
        if h.mask(t) & self.mask == 0 {
            return Ok(Pat::Lit(Raw::new(t)));
        }
        let View::App { head, args } = h.view(t) else { return Ok(Pat::Lit(Raw::new(t))) };
        let s = sym_of(h, head);
        let is = |x: Sym| s == Some(x);
        Ok(match args.len() {
            0 if is(hd.any) => blank(),
            0 if is(hd.seq1) => Pat::Seq { var: None, min: 1, ty: None, mark: None },
            0 if is(hd.seq0) => Pat::Seq { var: None, min: 0, ty: None, mark: None },
            1 if is(hd.any) => var(self.var(args[0])?),
            1 if is(hd.seq1) => seq1(self.var(args[0])?),
            1 if is(hd.seq0) => seq0(self.var(args[0])?),
            2 if is(hd.typed) => match self.convert(args[0], depth + 1)? {
                // `x: numeric`: a term that is a number or evaluates numerically to one (`pi`, `sqrt(2)`), the
                // condition `is_numeric(x)`: a function of a numeric procedure (`n_integrate`) waits for numbers
                Pat::Blank { var: Some(v), ty: None } if sym_of(h, args[1]).is_some_and(|s| h.sym_name(s) == "numeric") => {
                    let name = self.names[v as usize];
                    let test = h.app(h.sym_term(h.sym("is_numeric")), &[h.sym_term(name)]);
                    cond_term(var(v), &[v], &[name], test)
                }
                Pat::Blank { var: v, ty: None } => {
                    let ty = type_of(h, args[1])?;
                    match v {
                        Some(v) => typed(v, ty),
                        None => Pat::Blank { var: None, ty: Some(ty) },
                    }
                }
                // `xs__: T`: a run of arguments each of type T (C9B2.3)
                Pat::Seq { var, min, ty: None, mark } => Pat::Seq { var, min, ty: Some(type_of(h, args[1])?), mark },
                _ => return Err(DefError::Unsupported("a type on a pattern other than a blank or a sequence variable")),
            },
            2 if is(hd.named) => named(self.var(args[0])?, self.convert(args[1], depth + 1)?),
            2 if is(hd.condition) => {
                let inner = self.convert(args[0], depth + 1)?;
                let bound = bound_names(&inner, &self.names);
                let mut used = vec![];
                symbols(h, args[1], &mut used);
                let mut vars = vec![];
                let mut names = vec![];
                for u in used {
                    if let Some(i) = self.names.iter().position(|&n| n == u) {
                        if !bound.contains(&u) {
                            return Err(DefError::ConditionScope(i as u16));
                        }
                        vars.push(i as u16);
                        names.push(u);
                    }
                }
                cond_term(inner, &vars, &names, args[1])
            }
            1 | 2 if is(hd.repeated) => {
                let (min, max) = match args.get(1) {
                    None => (1, u32::MAX),
                    Some(&spec) => repeat_counts(h, spec).ok_or(DefError::Unsupported("repeated(p, n): n, [n] or [m, n] counts"))?,
                };
                Pat::Repeat { p: Box::new(self.convert(args[0], depth + 1)?), min, max, mark: None }
            }
            // a run that takes the longest / shortest run first, with a priority among the marked runs (r30.4); on a
            // single pattern the mark changes nothing (it takes one argument)
            1 | 2 if is(hd.longest) || is(hd.shortest) => {
                let long = is(hd.longest);
                let pri = match args.get(1).map(|&k| h.view(k)) {
                    None => 0,
                    Some(View::Int(k)) => k,
                    Some(_) => return Err(DefError::Unsupported("longest(p, k), shortest(p, k): an integer priority k")),
                };
                match self.convert(args[0], depth + 1)? {
                    Pat::Seq { var, min, ty, .. } => Pat::Seq { var, min, ty, mark: Some((long, pri)) },
                    Pat::Repeat { p, min, max, .. } => Pat::Repeat { p, min, max, mark: Some((long, pri)) },
                    p => p,
                }
            }
            1 if is(hd.key_value) => {
                let list = h.sym_term(h.sym(mc_api::names::LIST));
                let items: Vec<T<'h>> = match h.view(args[0]) {
                    View::App { head, args: xs } if head == list => xs.to_vec(),
                    _ => vec![args[0]],
                };
                Pat::KeyValue(items.iter().map(|&a| self.convert(a, depth + 1)).collect::<Result<_, _>>()?)
            }
            _ if is(hd.alternatives) => alt(args.iter().map(|&a| self.convert(a, depth + 1)).collect::<Result<_, _>>()?),
            // inside a pattern (a definition's direct arguments are expanded before, H.7): an argument that may be
            // missing, its variable then bound to the default (C9B.13)
            2 if is(hd.optional) => {
                let p = self.convert(args[0], depth + 1)?;
                // an optional run `optional(xs__, d)`: the run when the arguments are there, else the run [d] (under an
                // associative-commutative head, d itself) (r18c14.1)
                let run = match &p {
                    Pat::Seq { var: Some(v), .. } => Some(*v),
                    _ => None,
                };
                match whole_var(&p).or(run) {
                    Some(v) => Pat::Opt { p: Box::new(p), var: v, default: Raw::new(args[1]) },
                    None => return Err(DefError::Unsupported("an optional argument without one variable naming it")),
                }
            }
            _ if is(hd.optional) => return Err(DefError::Unsupported("optional(p) without a default")),
            1 if is(hd.held) || is(hd.hold_pattern) => self.convert(args[0], depth + 1)?,
            1 if is(hd.verbatim) => Pat::Lit(Raw::new(args[0])),
            _ => {
                let hp = self.convert(head, depth + 1)?;
                let args: Vec<Pat> = args.iter().map(|&a| self.convert(a, depth + 1)).collect::<Result<_, _>>()?;
                match hp {
                    Pat::Lit(head) if matches!(h.view(head.get(h)), View::Sym(_)) => Pat::App { head, args, info: Default::default() },
                    // a compound or pattern head: `call$(head, args...)` (C9B2.4)
                    hp => {
                        let mut all = vec![hp];
                        all.extend(args);
                        Pat::App { head: Raw::new(h.sym_term(h.sym(crate::pat::CALL))), args: all, info: Default::default() }
                    }
                }
            }
        })
    }
}

/// The variable that names the whole term a single-term pattern matches: `x_` (possibly typed), `x @ p`, or such a
/// pattern under a condition.
fn whole_var(p: &Pat) -> Option<u16> {
    match p {
        Pat::Blank { var: Some(v), .. } | Pat::Named(v, _) => Some(*v),
        Pat::Cond(q, _) => whole_var(q),
        _ => None,
    }
}

/// The counts of `repeated(p, spec)`: `n` (1 to n), `[n]` (exactly n), `[m, n]` (m to n); `inf` for no upper limit.
fn repeat_counts(h: &Heap, spec: T<'_>) -> Option<(u32, u32)> {
    let count = |t: T<'_>| -> Option<u32> {
        match h.view(t) {
            View::Int(n) if n >= 0 => Some(u32::try_from(n).unwrap_or(u32::MAX)),
            View::Sym(s) if h.sym_name(s) == "inf" => Some(u32::MAX),
            _ => None,
        }
    };
    match h.view(spec) {
        View::App { head, args } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::LIST) => match args {
            [n] => count(*n).map(|n| (n, n)),
            [m, n] => Some((count(*m)?, count(*n)?)).filter(|(m, n)| m <= n),
            _ => None,
        },
        _ => count(spec).map(|n| (1, n)),
    }
}

/// The symbols of the variables a lowered pattern binds.
fn bound_names(p: &Pat, names: &[Sym]) -> Vec<Sym> {
    let mut out = vec![];
    let mut stack = vec![p];
    while let Some(q) = stack.pop() {
        match q {
            Pat::Blank { var: Some(v), .. } | Pat::Seq { var: Some(v), .. } => out.push(names[*v as usize]),
            Pat::Named(v, inner) => {
                out.push(names[*v as usize]);
                stack.push(inner);
            }
            Pat::App { args, .. } | Pat::Alt(args) | Pat::KeyValue(args) => stack.extend(args.iter()),
            Pat::Cond(inner, _) | Pat::Repeat { p: inner, .. } | Pat::Opt { p: inner, .. } => stack.push(inner),
            _ => {}
        }
    }
    out
}

/// Lower a pattern term (see the module documentation).
pub fn pattern<'h>(h: &'h Heap, t: T<'h>) -> Result<Lowered, DefError> {
    let hd = Heads::new(h);
    let mut lw = Lower { h, hd: &hd, mask: hd.mask(h), names: vec![] };
    lw.collect(t)?;
    let pat = lw.convert(t, 0)?;
    Ok(Lowered { pat, names: lw.names })
}

/// Install `lhs := body` from terms, between evaluations.
pub fn define<'h>(k: &mut Kernel, h: &'h Heap, lhs: T<'h>, body: T<'h>) -> Result<(), DefError> {
    let l = pattern(h, lhs)?;
    let vars: Vec<u16> = (0..l.names.len() as u16).collect();
    k.define_term(h, l.pat, body, &l.names, &vars)
}

// ---------------------------------------------------------------------------------------------------------------
// The definition natives

/// The decorators of a definition (04 section 6.4) that have a meaning here.
#[derive(Default)]
struct Decorators<'h> {
    /// `on(s)`: store the rule with `s` (an argument-head rule, semantics 6.5)
    attach: Option<Sym>,
    ac: bool,
    listable: bool,
    effect: bool,
    held: bool,
    handles_failures: bool,
    uses_assumptions: bool,
    identity: Option<T<'h>>,
}

impl<'h> Decorators<'h> {
    fn read(h: &'h Heap, decs: &[T<'h>]) -> Result<Decorators<'h>, DefError> {
        let mut d = Decorators::default();
        let (mut assoc, mut comm) = (false, false);
        for &dec in decs {
            match h.view(dec) {
                View::Sym(s) => match h.sym_name(s).as_str() {
                    "associative" => assoc = true,
                    "commutative" => comm = true,
                    "listable" => d.listable = true,
                    "effect" => d.effect = true,
                    "held" => d.held = true,
                    "handles_failures" => d.handles_failures = true,
                    "uses_assumptions" => d.uses_assumptions = true,
                    _ => return Err(DefError::Unsupported("this decorator")),
                },
                View::App { head, args: &[a] } => match sym_of(h, head).map(|s| h.sym_name(s)).as_deref() {
                    Some("on") => d.attach = Some(sym_of(h, a).ok_or(DefError::BadAttachment)?),
                    Some("identity") => d.identity = Some(a),
                    _ => return Err(DefError::Unsupported("this decorator")),
                },
                _ => return Err(DefError::Unsupported("this decorator")),
            }
        }
        if assoc != comm {
            return Err(DefError::Unsupported("associative without commutative, or the reverse"));
        }
        if d.identity.is_some() && !assoc {
            return Err(DefError::Unsupported("an identity element without associative and commutative"));
        }
        d.ac = assoc;
        Ok(d)
    }

    /// Give `s` these properties and the argument modes of its `held` parameters (`held_mask`, by position).
    fn apply(&self, cx: &mut NativeCx<'_, '_, 'h>, s: Sym, held_mask: u32) -> Res<()> {
        let old = cx.attributes(s);
        let mut a = old;
        a.ac |= self.ac;
        a.listable |= self.listable;
        a.effect |= self.effect;
        a.handles_failures |= self.handles_failures;
        a.uses_assumptions |= self.uses_assumptions;
        if self.held {
            a.hold = Hold::All;
        }
        if held_mask != 0 {
            a.hold = Hold::from_mask(a.hold.mask() | held_mask);
        }
        if a != old {
            cx.set_attributes(s, a)?;
        }
        if let Some(e) = self.identity {
            let ev = cx.ev;
            ev.set_identity(cx.cx, s, Some(e))?;
        }
        Ok(())
    }
}

/// The local binding an optional parameter's pattern stands for when its argument is missing, and the variable's
/// name: `any(x)` gives `x`, `typed(any(x), T)` gives `typed(x, T)` (checked when bound); `named(x, p)` and a
/// condition `condition(p, c)` give the binding of their variable (the default is not tested against `p` or `c`:
/// it is what the definition says the argument is when it is missing); a sequence variable `seq1(x)`,
/// `seq0(x)` binds `x` to the list of the one default term (semantics 6.1: a sequence variable stands for the list
/// of its terms). The third value tells that the default is to be wrapped in a list.
fn default_binding<'h>(h: &'h Heap, hd: &Heads, p: T<'h>) -> Result<(T<'h>, Sym, bool), DefError> {
    let unsupported = DefError::Unsupported("an optional parameter without one variable naming the whole argument");
    match call_of(h, p) {
        Some((s, &[x])) if s == hd.any || s == hd.seq1 || s == hd.seq0 => {
            Ok((x, sym_of(h, x).ok_or(unsupported)?, s != hd.any))
        }
        Some((s, &[inner, ty])) if s == hd.typed => match call_of(h, inner) {
            Some((a, &[x])) if a == hd.any => {
                let name = sym_of(h, x).ok_or(unsupported)?;
                Ok((h.call(hd.typed, &[x, ty]), name, false))
            }
            _ => Err(unsupported),
        },
        Some((s, &[x, _])) if s == hd.named => Ok((x, sym_of(h, x).ok_or(unsupported)?, false)),
        Some((s, &[q, _])) if s == hd.condition => {
            let (b, name, seq) = default_binding(h, hd, q)?;
            // the binding of a typed variable checks its type; under a condition the default is taken as given
            let plain = match call_of(h, b) {
                Some((t, &[x, _])) if t == hd.typed => x,
                _ => b,
            };
            Ok((plain, name, seq))
        }
        _ => Err(unsupported),
    }
}

/// `let(binding, d)` for an optional parameter missing (see [`default_binding`]).
fn default_let<'h>(h: &'h Heap, hd: &Heads, p: T<'h>, d: T<'h>) -> Result<T<'h>, DefError> {
    let (b, _, seq) = default_binding(h, hd, p)?;
    let v = if seq { h.call(h.sym(mc_api::names::LIST), &[d]) } else { d };
    Ok(h.call(hd.let_, &[b, v]))
}

/// Most optional parameters of one clause (each doubles the rules it defines).
const MAX_OPTIONAL: usize = 6;

/// One rule of a definition: its left side (a term pattern) and right side.
struct Clause<'h> {
    lhs: T<'h>,
    body: T<'h>,
}

/// The rules a clause `lhs := body` stands for, and the argument positions held by `held` parameters: `optional`
/// parameters expand to a rule with the argument and one without it, whose body (and condition) binds the default
/// by `let` (04 section 6.2); options after `;` (`opts`, 04 section 7) add a trailing `options(...)` argument read
/// by `option_value`, unknown names giving `failure(unknown_option, name)`.
fn clauses<'h>(h: &'h Heap, hd: &Heads, lhs: T<'h>, body: T<'h>, opts: &[T<'h>]) -> Result<(Vec<Clause<'h>>, u32), DefError> {
    let (core, cond) = match call_of(h, lhs) {
        Some((s, &[p, c])) if s == hd.condition => (p, Some(c)),
        _ => (lhs, None),
    };
    let View::App { head: f, args } = h.view(core) else { return Ok((vec![Clause { lhs, body }], 0)) };
    let mut held = 0u32;
    let mut plain = Vec::with_capacity(args.len());
    for (i, &a) in args.iter().enumerate() {
        match call_of(h, a) {
            Some((s, &[p])) if s == hd.held => {
                let rest = matches!(call_of(h, p), Some((q, _)) if q == hd.seq0 || q == hd.seq1);
                held |= if rest { !0u32 << i.min(31) } else { 1u32 << i.min(31) };
                plain.push(p);
            }
            _ => plain.push(a),
        }
    }
    let optional: Vec<usize> =
        (0..plain.len()).filter(|&i| matches!(call_of(h, plain[i]), Some((s, &[_, _])) if s == hd.optional)).collect();
    if optional.len() > MAX_OPTIONAL {
        return Err(DefError::TooLarge);
    }
    if optional.is_empty() && opts.is_empty() && held == 0 {
        return Ok((vec![Clause { lhs, body }], 0));
    }
    // the options' bindings: (binding, name, default)
    let mut named = vec![];
    for &o in opts {
        let Some((s, &[p, d])) = call_of(h, o) else { return Err(DefError::Unsupported("an option without a default")) };
        if s != hd.optional {
            return Err(DefError::Unsupported("an option without a default"));
        }
        let (b, name, _) = default_binding(h, hd, p)?;
        named.push((b, name, d));
    }
    let o = h.sym_term(h.sym("options$"));
    let block = |stmts: Vec<T<'h>>, last: T<'h>| {
        if stmts.is_empty() {
            last
        } else {
            let mut s = stmts;
            s.push(last);
            h.call(hd.block, &s)
        }
    };
    let mut out = vec![];
    // all optional arguments present first; bit j of `absent` drops the j-th optional argument
    for absent in 0u32..1 << optional.len() {
        let mut xs = Vec::with_capacity(plain.len() + 1);
        let mut lets = vec![];
        for (i, &a) in plain.iter().enumerate() {
            match optional.iter().position(|&k| k == i) {
                Some(j) => {
                    // invariant: `optional` holds only positions of `optional(p, d)` calls
                    let Some((_, &[p, d])) = call_of(h, a) else { unreachable!("an optional argument") };
                    if absent >> j & 1 != 0 {
                        lets.push(default_let(h, hd, p, d)?);
                    } else {
                        xs.push(p);
                    }
                }
                None => xs.push(a),
            }
        }
        let cond = cond.map(|c| block(lets.clone(), c));
        for given in if named.is_empty() { &[false][..] } else { &[true, false][..] } {
            let mut xs = xs.clone();
            let mut stmts = vec![];
            if *given {
                xs.push(h.call(hd.named, &[o, h.call(h.sym("options"), &[h.call(hd.seq0, &[])])]));
                let names: Vec<T<'h>> = named.iter().map(|(_, n, _)| h.string(&h.sym_name(*n))).collect();
                stmts.push(h.call(h.sym("check_options"), &[o, h.call(h.sym(mc_api::names::LIST), &names)]));
            }
            stmts.extend(lets.iter().copied());
            for &(b, name, d) in &named {
                let v = if *given { h.call(h.sym("option_value"), &[o, h.string(&h.sym_name(name)), d]) } else { d };
                stmts.push(h.call(hd.let_, &[b, v]));
            }
            let mut lhs = h.app(f, &xs);
            if let Some(c) = cond {
                lhs = h.call(hd.condition, &[lhs, c]);
            }
            out.push(Clause { lhs, body: block(stmts, body) });
        }
    }
    Ok((out, held))
}

/// Define `lhs := body` (a symbol's own value, or the rules of [`clauses`]), attached to `attach` if given; the
/// decorators' properties go to the left side's head. With `checked` (a `fn`), the head also gets the catch-all
/// `f(args___) := failure(no_match, f, args)` (04 section 6.2), which every clause is an instance of, so it is tried
/// last.
#[allow(clippy::too_many_arguments)]
fn define_now<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    hd: &Heads,
    lhs: T<'h>,
    body: T<'h>,
    opts: &[T<'h>],
    decs: &[T<'h>],
    checked: bool,
) -> Res<Result<(), DefError>> {
    let h = cx.heap();
    // a protected target (decision r17fix.4): the left side's symbol or head, or the symbol a rule attaches to
    let attach = Decorators::read(h, decs).ok().and_then(|d| d.attach);
    let target = match h.view(lhs) {
        View::Sym(s) => Some(s),
        _ => head_symbol(h, hd, lhs),
    };
    if let Some(p) = target.into_iter().chain(attach).find(|&s| cx.protected(s)) {
        cx.effect()?;
        return Ok(Err(DefError::Protected(p)));
    }
    if let View::Sym(s) = h.view(lhs) {
        if !decs.is_empty() || !opts.is_empty() {
            cx.effect()?;
            return Ok(Err(DefError::Unsupported("decorators or options on a value definition")));
        }
        cx.set_own(s, body)?;
        return Ok(Ok(()));
    }
    let plan = Decorators::read(h, decs).and_then(|d| clauses(h, hd, lhs, body, opts).map(|c| (d, c)));
    let lowered = plan.and_then(|(d, (cs, held))| {
        let rules = cs.iter().map(|c| pattern(h, c.lhs).map(|l| (l, c.body))).collect::<Result<Vec<_>, _>>()?;
        Ok((d, rules, held))
    });
    let (decs, rules, held) = match lowered {
        Ok(x) => x,
        Err(e) => {
            cx.effect()?; // a definition attempt is ordered like the definition itself
            return Ok(Err(e));
        }
    };
    let f = head_symbol(h, hd, lhs);
    if let Some(f) = f {
        decs.apply(cx, f, held)?;
    }
    // A clause `f(xs...) when c` is as general as the catch-all below: equally general rules keep definition order,
    // so the catch-all an earlier clause added is taken out here and added again after this clause.
    let seq_clause = checked && {
        let core = match call_of(h, lhs) {
            Some((s, &[p, _])) if s == hd.condition => p,
            _ => lhs,
        };
        core != lhs && matches!(call_of(h, core), Some((_, &[p])) if matches!(call_of(h, p), Some((s, &[_])) if s == hd.seq0))
    };
    if let (true, Some(f)) = (seq_clause, f) {
        let all = catch_all(h, hd, f);
        if let Err(e) = cx.unset_rule(all.pat)? {
            return Ok(Err(e));
        }
    }
    // the definition as written, kept with its first clause (a definition with optional arguments or options gives
    // several clauses; it is listed once, as written)
    let mut src = Some(h.call(hd.rule_delayed, &[lhs, body]));
    for (l, body) in rules {
        let vars: Vec<u16> = (0..l.names.len() as u16).collect();
        let src = src.take().map(Raw::new);
        let rhs = RuleRhs::Term { body: Raw::new(body), names: l.names.into(), vars: vars.into(), src };
        let ev = cx.ev;
        if let Err(e) = ev.define_rule(cx.cx, l.pat, rhs, decs.attach)? {
            return Ok(Err(e));
        }
    }
    // A clause `f(xs...)` without a condition already matches every call; the catch-all has the same pattern and
    // would replace it, so it is left out.
    let catches_all = matches!(call_of(h, lhs), Some((_, &[p])) if matches!(call_of(h, p), Some((s, &[_])) if s == hd.seq0));
    if let (true, Some(f), false) = (checked, f, catches_all) {
        // The catch-all fires only on ground arguments (no free symbols): with a symbolic argument a clause's
        // condition or type test may be undecided, so the call stays unevaluated like any symbolic term, and
        // numeric code (find_root, plot, n_integrate) can substitute numbers later (04 section 6.2).
        let args = h.sym("args$");
        let all = catch_all(h, hd, f);
        let nm = &cx.ev.k.names;
        let fail = h.call(nm.failure, &[h.sym_term(nm.no_match), h.sym_term(f), h.sym_term(args)]);
        let rhs = RuleRhs::Term { body: Raw::new(fail), names: all.names.into(), vars: vec![0].into(), src: None };
        let ev = cx.ev;
        if let Err(e) = ev.define_rule(cx.cx, all.pat, rhs, None)? {
            return Ok(Err(e));
        }
    }
    Ok(Ok(()))
}

/// The left side of a `fn`'s catch-all, `f(args$___) when is_ground(args$)`.
fn catch_all(h: &Heap, hd: &Heads, f: Sym) -> Lowered {
    let args = h.sym("args$");
    let call = h.call(f, &[h.call(hd.seq0, &[h.sym_term(args)])]);
    let guarded = h.call(hd.condition, &[call, h.call(h.sym("is_ground"), &[h.sym_term(args)])]);
    // invariant: a fixed, well-formed pattern
    pattern(h, guarded).expect("a catch-all pattern")
}

/// The head symbol of a definition's left side (under a top condition).
fn head_symbol(h: &Heap, hd: &Heads, lhs: T<'_>) -> Option<Sym> {
    let core = match call_of(h, lhs) {
        Some((s, &[p, _])) if s == hd.condition => p,
        _ => lhs,
    };
    call_of(h, core).map(|(s, _)| s)
}

/// `null`, or `failure(definition, message)`.
fn reply<'h>(cx: &NativeCx<'_, '_, 'h>, r: Result<(), DefError>) -> Reply<'h> {
    Reply::Done(match r {
        Ok(()) => cx.null(),
        Err(DefError::Protected(s)) => protected_failure(cx.heap(), s),
        Err(e) => {
            let h = cx.heap();
            use mc_api::names;
            h.call(h.sym(names::FAILURE), &[h.sym_term(h.sym(names::failure::DEFINITION)), h.string(&e.to_string())])
        }
    })
}

/// `failure(protected, s)`: a change to the definitions of a protected symbol (decision r17fix.4).
pub(crate) fn protected_failure<'h>(h: &'h Heap, s: Sym) -> T<'h> {
    h.call(h.sym(mc_api::names::FAILURE), &[h.sym_term(h.sym("protected")), h.sym_term(s)])
}

/// `unset(x, ...)`: each argument a symbol (its own value, identity element and rules removed; its native and
/// attributes stay) or a left side `f(...)` (the rule with that left side up to renaming removed). Gives `null`;
/// `failure(protected, x)` for a protected symbol, `failure(not_defined, lhs)` when no rule has that left side,
/// `failure(definition, ...)` for a left side that is not a pattern. Works during an evaluation (an effect of the
/// frontier: what is removed is retired until the next safe point, so readers on other tasks finish safely).
struct Unset;

impl Native for Unset {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        cx.effect()?;
        let hd = Heads::new(h);
        for &x in a {
            // `unset([f, g])` as `unset(f, g)`
            let items: Vec<T<'h>> = match cx.as_list(x) {
                Some(xs) => xs.to_vec(),
                None => vec![x],
            };
            for x in items {
                match h.view(x) {
                    View::Sym(s) => {
                        if cx.protected(s) {
                            return Ok(Reply::Done(protected_failure(h, s)));
                        }
                        cx.unset(s)?;
                    }
                    View::App { .. } => {
                        if let Some(s) = head_symbol(h, &hd, x).filter(|&s| cx.protected(s)) {
                            return Ok(Reply::Done(protected_failure(h, s)));
                        }
                        let l = match pattern(h, x) {
                            Ok(l) => l,
                            Err(e) => return Ok(reply(cx, Err(e))),
                        };
                        match cx.unset_rule(l.pat)? {
                            Ok(true) => {}
                            Ok(false) => {
                                let f = h.call(h.sym(mc_api::names::FAILURE), &[h.sym_term(h.sym("not_defined")), x]);
                                return Ok(Reply::Done(f));
                            }
                            Err(e) => return Ok(reply(cx, Err(e))),
                        }
                    }
                    View::Str(n) if is_identifier(n) || n.contains('*') => {
                        let ss = if n.contains('*') { matching_symbols(cx, n) } else { vec![h.sym(n)] };
                        for s in ss {
                            if cx.protected(s) {
                                return Ok(Reply::Done(protected_failure(h, s)));
                            }
                            cx.unset(s)?;
                        }
                    }
                    _ => return Ok(Reply::Done(arg_failure(h, "unset", x))),
                }
            }
        }
        Ok(Reply::Done(cx.null()))
    }
}

/// `protect(f, ...)` (`on`) and `unprotect(f, ...)`: set or clear the `protected` attribute of each symbol (or each
/// symbol of a list argument; a string naming an identifier stands for that symbol); the value is the list of the
/// names, as strings, of the symbols whose attribute changed (decision r17fix.4).
struct Protect {
    on: bool,
}

impl Native for Protect {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        cx.effect()?;
        let name = if self.on { "protect" } else { "unprotect" };
        let mut syms = Vec::new();
        for &x in a {
            let items: Vec<T<'h>> = match cx.as_list(x) {
                Some(xs) => xs.to_vec(),
                None => vec![x],
            };
            for x in items {
                match h.view(x) {
                    View::Sym(s) => syms.push(s),
                    View::Str(n) if is_identifier(n) => syms.push(h.sym(n)),
                    // a name pattern: the user's symbols whose names match (`*` any characters)
                    View::Str(n) if n.contains('*') => syms.extend(matching_symbols(cx, n)),
                    _ => return Ok(Reply::Done(arg_failure(h, name, x))),
                }
            }
        }
        let mut changed = Vec::new();
        for s in syms {
            let at = cx.attributes(s);
            if at.protected != self.on {
                cx.set_attributes(s, crate::Attributes { protected: self.on, ..at })?;
                changed.push(h.string(&h.sym_name(s)));
            }
        }
        Ok(Reply::Done(cx.list(&changed)))
    }
}

/// The user's symbols ([`NativeCx::user_symbols`]) whose names match the pattern `p`, in which `*` stands for any
/// characters.
fn matching_symbols(cx: &NativeCx<'_, '_, '_>, p: &str) -> Vec<Sym> {
    let h = cx.heap();
    cx.user_symbols().into_iter().filter(|&s| is_identifier(&h.sym_name(s)) && glob(p, &h.sym_name(s))).collect()
}

/// Does `name` match `p`, `*` matching any run of characters?
fn glob(p: &str, name: &str) -> bool {
    let parts: Vec<&str> = p.split('*').collect();
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if parts.len() == 1 {
        return p == name;
    }
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) || name.len() < first.len() + last.len() {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    true
}

/// Is `n` a plain identifier (letters, digits, `_`, not starting with a digit)?
fn is_identifier(n: &str) -> bool {
    let mut cs = n.chars();
    cs.next().is_some_and(|c| c.is_alphabetic() || c == '_') && cs.all(|c| c.is_alphanumeric() || c == '_')
}

/// `failure(argument, f, x)`.
fn arg_failure<'h>(h: &'h Heap, f: &str, x: T<'h>) -> T<'h> {
    h.call(h.sym(mc_api::names::FAILURE), &[h.sym_term(h.sym("argument")), h.sym_term(h.sym(f)), x])
}

/// `set_delayed(lhs, e)` and `set(lhs, v)` (the attributes decide whether the value was evaluated).
struct SetDef(Heads);

impl Native for SetDef {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [lhs, v] = a else { return Ok(Reply::Decline) };
        // destructuring: `[a, b] = [x, y]` assigns element by element (nested lists too) and gives the value
        if let (Some(ls), Some(vs)) = (cx.as_list(*lhs), cx.as_list(*v)) {
            if ls.len() == vs.len() {
                let h = cx.heap();
                let set = h.sym_term(h.sym("set"));
                for (&l, &x) in ls.iter().zip(vs) {
                    let r = cx.eval(h.app(set, &[l, x]))?;
                    if matches!(h.view(r), View::App { head, .. } if matches!(h.view(head), View::Sym(f) if h.sym_name(f) == "failure")) {
                        return Ok(Reply::Done(r));
                    }
                }
                return Ok(Reply::Done(*v));
            }
        }
        if let Some(r) = set_part(cx, *lhs, *v)? {
            return Ok(r);
        }
        let r = define_now(cx, &self.0, *lhs, *v, &[], &[], false)?;
        Ok(reply(cx, r))
    }
}

/// `s[k1, ..., kn] = v` where `s` is a symbol holding a list or an association: `s` becomes its value with that
/// part replaced (terms are immutable; the symbol is rebound), and the result is `v`. A key missing from an
/// association at the last level is added at the end. `None` for any other left side (a definition).
fn set_part<'h>(cx: &mut NativeCx<'_, '_, 'h>, lhs: T<'h>, v: T<'h>) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(lhs) else { return Ok(None) };
    if !matches!(h.view(head), View::Sym(p) if h.sym_name(p) == "part") || args.len() < 2 {
        return Ok(None);
    }
    let (s, idx) = (args[0], &args[1..]);
    if !matches!(h.view(s), View::Sym(_)) {
        return Ok(None);
    }
    let cur = cx.eval(s)?;
    let is_container = |t: T<'h>| {
        h.packed(t).is_some()
            || matches!(h.view(t), View::App { head, .. } if matches!(h.view(head), View::Sym(f) if matches!(&*h.sym_name(f), "list" | "assoc")))
    };
    if cur == s || !is_container(cur) {
        return Ok(None);
    }
    let mut ks = Vec::with_capacity(idx.len());
    for &i in idx {
        ks.push(cx.eval(i)?);
    }
    let rule = h.sym_term(h.sym("rule"));
    let pos = if ks.len() == 1 { ks[0] } else { cx.list(&ks) };
    let mut new = cx.eval(h.app(h.sym_term(h.sym("replace_part")), &[cur, h.app(rule, &[pos, v])]))?;
    if new == cur && ks.len() == 1 {
        // a new key of an association
        if matches!(h.view(cur), View::App { head, .. } if matches!(h.view(head), View::Sym(f) if h.sym_name(f) == "assoc")) {
            new = cx.eval(h.app(h.sym_term(h.sym("append")), &[cur, h.app(rule, &[ks[0], v])]))?;
        }
    }
    let mut at = vec![cur];
    at.extend_from_slice(&ks);
    if new == cur && cx.eval(h.app(head, &at))? != v {
        return Ok(None);
    }
    let set = h.sym_term(h.sym("set"));
    cx.eval(h.app(set, &[s, new]))?;
    Ok(Some(Reply::Done(v)))
}

/// `define_rule(rule_delayed(L, R) | rule(L, R), list(decorators))`.
struct DefineRule(Heads);

impl Native for DefineRule {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [r, decs] = a else { return Ok(Reply::Decline) };
        let Some((s, [l, rhs])) = call_of(h, *r) else { return Ok(Reply::Decline) };
        let Some(decs) = cx.as_list(*decs) else { return Ok(Reply::Decline) };
        if !(s == self.0.rule || s == self.0.rule_delayed) {
            return Ok(Reply::Decline);
        }
        let body = if s == self.0.rule { cx.eval(*rhs)? } else { *rhs };
        if s == self.0.rule && decs.is_empty() {
            if let Some(r) = set_part(cx, *l, body)? {
                return Ok(r);
            }
        }
        let r = define_now(cx, &self.0, *l, body, &[], decs, false)?;
        Ok(reply(cx, r))
    }
}

/// `define_fn(lhs, body, list(options), result_type | null, list(decorators))`.
struct DefineFn(Heads);

impl Native for DefineFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [lhs, body, opts, _result, decs] = a else { return Ok(Reply::Decline) };
        let (Some(opts), Some(decs)) = (cx.as_list(*opts), cx.as_list(*decs)) else { return Ok(Reply::Decline) };
        // `@compiled` (item 1b): define without it, then evaluate `compile(f)`
        let h = cx.heap();
        let compiled = h.sym_term(h.sym("compiled"));
        let rest: Vec<T<'h>> = decs.iter().copied().filter(|&d| d != compiled).collect();
        let r = define_now(cx, &self.0, *lhs, *body, opts, &rest, true)?;
        if let (Ok(()), true, Some(f)) = (&r, rest.len() < decs.len(), head_symbol(h, &self.0, *lhs)) {
            return Ok(Reply::Done(compile_after(cx, f)?));
        }
        Ok(reply(cx, r))
    }
}

/// The value of a definition decorated `@compiled`: `compile(f)` is evaluated after the definition (the numeric
/// compiler registers `compile`); its value `f` means success and the definition gives `null`, anything else
/// (`failure(not_compilable, f, reason)`, or `compile(f)` unevaluated when no compiler is installed) is the value.
fn compile_after<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: Sym) -> Res<T<'h>> {
    let h = cx.heap();
    let ft = h.sym_term(f);
    let v = cx.eval(h.call(h.sym("compile"), &[ft]))?;
    Ok(if v == ft { cx.null() } else { v })
}

/// `define_sym(s, list(decorators))`.
struct DefineSym;

impl Native for DefineSym {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [s, decs] = a else { return Ok(Reply::Decline) };
        let (Some(s), Some(decs)) = (sym_of(h, *s), cx.as_list(*decs)) else { return Ok(Reply::Decline) };
        cx.effect()?;
        if cx.protected(s) {
            return Ok(Reply::Done(protected_failure(h, s)));
        }
        match Decorators::read(h, decs) {
            Ok(d) if d.attach.is_none() => {
                d.apply(cx, s, 0)?;
                Ok(reply(cx, Ok(())))
            }
            Ok(_) => Ok(reply(cx, Err(DefError::Unsupported("on(...) on a symbol declaration")))),
            Err(e) => Ok(reply(cx, Err(e))),
        }
    }
}

/// Register the definition natives.
/// Numeric constants: symbols that stand for a number, so a term built from them and numbers is ground.
const CONSTANTS: [&str; 7] = ["pi", "e", "euler_gamma", "catalan", "golden_ratio", "inf", "im"];

/// Is `t` ground: no free symbol other than the numeric constants, at any depth below compound heads? Numbers,
/// strings and packed arrays are ground; heads are not inspected (`square(1)` is ground).
pub(crate) fn is_ground(h: &Heap, t: T<'_>) -> bool {
    if h.packed(t).is_some() {
        return true;
    }
    match h.view(t) {
        View::Sym(s) => {
            let n = h.sym_name(s);
            CONSTANTS.contains(&n.as_str()) || n == "true" || n == "false" || n == "null"
        }
        View::App { args, .. } => args.iter().all(|&a| is_ground(h, a)),
        _ => true,
    }
}

/// `is_ground(args...)`: `true` when every argument is ground ([`is_ground`]); the condition of a `fn`'s catch-all.
struct IsGround;

impl Native for IsGround {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        Ok(Reply::Done(cx.bool(args.iter().all(|&a| is_ground(h, a)))))
    }
}

pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let fx = |hold| Attributes { hold, effect: true, ..Attributes::default() };
    k.register(h.sym("is_ground"), Attributes::default(), Box::new(IsGround));
    k.register(h.sym("set_delayed"), fx(Hold::All), Box::new(SetDef(Heads::new(h))));
    // an assignment keeps a failure as the value it assigns (decision p6core.25: `r = find_root(...)` that fails
    // leaves `r` the failure, not unassigned)
    k.register(h.sym("set"), Attributes { handles_failures: true, ..fx(Hold::First) }, Box::new(SetDef(Heads::new(h))));
    k.register(h.sym("define_rule"), fx(Hold::All), Box::new(DefineRule(Heads::new(h))));
    k.register(h.sym("define_fn"), fx(Hold::All), Box::new(DefineFn(Heads::new(h))));
    k.register(h.sym("define_sym"), fx(Hold::All), Box::new(DefineSym));
    k.register(h.sym("unset"), fx(Hold::All), Box::new(Unset));
    k.register(h.sym("protect"), fx(Hold::All), Box::new(Protect { on: true }));
    k.register(h.sym("unprotect"), fx(Hold::All), Box::new(Protect { on: false }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::Config;

    #[test]
    fn lowering_checks_condition_scope() {
        let h = &Heap::new(Config::no_collect());
        fn c<'h>(h: &'h Heap, n: &str, a: &[T<'h>]) -> T<'h> {
            h.call(h.sym(n), a)
        }
        let s = |n: &str| h.sym_term(h.sym(n));
        let any = |x: &str| c(h, "any", &[s(x)]);
        // f(condition(any(x), less(y, 1)), any(y)): the condition reads y, bound outside
        let bad = c(h, "f", &[c(h, "condition", &[any("x"), c(h, "less", &[s("y"), h.int(1)])]), any("y")]);
        assert!(matches!(pattern(h, bad), Err(DefError::ConditionScope(_))));
        let good = c(h, "condition", &[c(h, "f", &[any("x"), any("y")]), c(h, "less", &[s("x"), s("y")])]);
        assert_eq!(pattern(h, good).expect("pattern").names.len(), 2);
        // an optional argument inside a pattern (C9B.13) has a variable naming it: a named blank or a run
        // (r18c14.1)
        let opt = c(h, "f", &[c(h, "optional", &[any("x"), h.int(1)])]);
        assert!(pattern(h, opt).is_ok());
        let run = c(h, "f", &[c(h, "optional", &[c(h, "seq0", &[s("x")]), h.int(1)])]);
        assert!(pattern(h, run).is_ok());
        let bad_opt = c(h, "f", &[c(h, "optional", &[c(h, "g", &[any("x")]), h.int(1)])]);
        assert!(matches!(pattern(h, bad_opt), Err(DefError::Unsupported(_))));
    }
}
