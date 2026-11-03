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
//! | any other compound or atom | itself, with patterns inside |
//!
//! `optional(p, d)` and `held(p)` are not supported yet ([`DefError::Unsupported`]).
//!
//! Definitions, as natives (effects, so ordered like every other effect; a malformed definition evaluates to
//! `failure(definition, message)`):
//!
//! | term | effect |
//! |---|---|
//! | `set_delayed(s, e)` | own value `e`, evaluated when `s` is read |
//! | `set_delayed(lhs, e)` | rule `lhs := e` |
//! | `set(s, v)`, `set(lhs, v)` | `v` evaluated first, then as above |
//! | `define_rule(rule_delayed(L, R), list(decs))`, `define_rule(rule(L, R), list(decs))` | rule `L := R` (with `rule`, `R` evaluated first) |
//! | `define_fn(lhs, body, list(opts), result_type, list(decs))` | rule `lhs := body` (options must be empty; the result type is not checked yet) |
//! | `define_sym(s, list(decs))` | attributes from decorators: `associative` with `commutative` (AC), `held`, `effect` |
//!
//! Right sides are terms in which the variables are written as their symbols; a sequence variable stands for the
//! list of its terms (semantics 6.1).
use crate::defs::{Attributes, Hold};
use crate::eval::{Kernel, Res};
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
    rule: Sym,
    rule_delayed: Sym,
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
            rule: h.sym("rule"),
            rule_delayed: h.sym("rule_delayed"),
        }
    }

    /// Mask of the pattern heads: a term without any of these bits contains no pattern.
    fn mask(&self, h: &Heap) -> u64 {
        [self.any, self.seq1, self.seq0, self.typed, self.named, self.optional, self.held, self.condition, self.alternatives]
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

fn type_of(h: &Heap, t: T<'_>) -> Result<Ty, DefError> {
    let Some(s) = sym_of(h, t) else { return Err(DefError::UnknownType) };
    Ok(match h.sym_name(s).as_str() {
        "integer" | "Integer" | "Int" => Ty::Integer,
        "rational" | "Rational" => Ty::Rational,
        "machine_real" | "MachineReal" => Ty::MachineReal,
        "ball" | "Ball" => Ty::Ball,
        "real" | "Real" => Ty::Real,
        "number" | "Number" => Ty::Number,
        "string" | "String" => Ty::String,
        "symbol" | "Symbol" => Ty::Symbol,
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
            0 if is(hd.seq1) => Pat::Seq { var: None, min: 1 },
            0 if is(hd.seq0) => Pat::Seq { var: None, min: 0 },
            1 if is(hd.any) => var(self.var(args[0])?),
            1 if is(hd.seq1) => seq1(self.var(args[0])?),
            1 if is(hd.seq0) => seq0(self.var(args[0])?),
            2 if is(hd.typed) => match self.convert(args[0], depth + 1)? {
                Pat::Blank { var: v, ty: None } => {
                    let ty = type_of(h, args[1])?;
                    match v {
                        Some(v) => typed(v, ty),
                        None => Pat::Blank { var: None, ty: Some(ty) },
                    }
                }
                _ => return Err(DefError::Unsupported("a type on a pattern other than a blank")),
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
            _ if is(hd.alternatives) => alt(args.iter().map(|&a| self.convert(a, depth + 1)).collect::<Result<_, _>>()?),
            _ if is(hd.optional) => return Err(DefError::Unsupported("optional arguments")),
            _ if is(hd.held) => return Err(DefError::Unsupported("held parameters")),
            _ => {
                let Pat::Lit(head) = self.convert(head, depth + 1)? else { return Err(DefError::Unsupported("a pattern in a head")) };
                let args = args.iter().map(|&a| self.convert(a, depth + 1)).collect::<Result<_, _>>()?;
                Pat::App { head, args, info: Default::default() }
            }
        })
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
            Pat::App { args, .. } | Pat::Alt(args) => stack.extend(args.iter()),
            Pat::Cond(inner, _) => stack.push(inner),
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

fn define_now<'h>(cx: &mut NativeCx<'_, '_, 'h>, lhs: T<'h>, body: T<'h>) -> Res<Result<(), DefError>> {
    let h = cx.heap();
    if let View::Sym(s) = h.view(lhs) {
        cx.set_own(s, body)?;
        return Ok(Ok(()));
    }
    let l = match pattern(h, lhs) {
        Ok(l) => l,
        Err(e) => {
            cx.effect()?; // a definition attempt is ordered like the definition itself
            return Ok(Err(e));
        }
    };
    let vars: Vec<u16> = (0..l.names.len() as u16).collect();
    cx.define_term(l.pat, body, &l.names, &vars)
}

/// `null`, or `failure(definition, message)`.
fn reply<'h>(cx: &NativeCx<'_, '_, 'h>, r: Result<(), DefError>) -> Reply<'h> {
    Reply::Done(match r {
        Ok(()) => cx.null(),
        Err(e) => {
            let h = cx.heap();
            h.call(h.sym("failure"), &[h.sym_term(h.sym("definition")), h.string(&e.to_string())])
        }
    })
}

/// `set_delayed(lhs, e)` and `set(lhs, v)` (the attributes decide whether the value was evaluated).
struct SetDef;

impl Native for SetDef {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [lhs, v] = a else { return Ok(Reply::Decline) };
        let r = define_now(cx, *lhs, *v)?;
        Ok(reply(cx, r))
    }
}

/// `define_rule(rule_delayed(L, R) | rule(L, R), list(decorators))`.
struct DefineRule(Heads);

impl Native for DefineRule {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [r, decs] = a else { return Ok(Reply::Decline) };
        let Some((s, [l, rhs])) = call_of(h, *r) else { return Ok(Reply::Decline) };
        if !(s == self.0.rule || s == self.0.rule_delayed) {
            return Ok(Reply::Decline);
        }
        if cx.as_list(*decs).is_some_and(|d| !d.is_empty()) {
            cx.effect()?;
            return Ok(reply(cx, Err(DefError::Unsupported("rule decorators"))));
        }
        let body = if s == self.0.rule { cx.eval(*rhs)? } else { *rhs };
        let r = define_now(cx, *l, body)?;
        Ok(reply(cx, r))
    }
}

/// `define_fn(lhs, body, list(options), result_type | null, list(decorators))`.
struct DefineFn;

impl Native for DefineFn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let [lhs, body, opts, _result, decs] = a else { return Ok(Reply::Decline) };
        let empty = |cx: &NativeCx<'_, '_, 'h>, t| cx.as_list(t).is_some_and(|l| l.is_empty());
        if !empty(cx, *opts) || !empty(cx, *decs) {
            cx.effect()?;
            return Ok(reply(cx, Err(DefError::Unsupported("function options and decorators"))));
        }
        let r = define_now(cx, *lhs, *body)?;
        Ok(reply(cx, r))
    }
}

/// `define_sym(s, list(decorators))`.
struct DefineSym;

impl Native for DefineSym {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, a: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [s, decs] = a else { return Ok(Reply::Decline) };
        let (Some(s), Some(decs)) = (sym_of(h, *s), cx.as_list(*decs)) else { return Ok(Reply::Decline) };
        cx.effect()?;
        let mut attrs = cx.attributes(s);
        let (mut assoc, mut comm) = (false, false);
        for &d in decs {
            match sym_of(h, d).map(|d| h.sym_name(d)).as_deref() {
                Some("associative") => assoc = true,
                Some("commutative") => comm = true,
                Some("held") => attrs.hold = Hold::All,
                Some("effect") => attrs.effect = true,
                _ => return Ok(reply(cx, Err(DefError::Unsupported("this decorator")))),
            }
        }
        if assoc != comm {
            return Ok(reply(cx, Err(DefError::Unsupported("associative without commutative, or the reverse"))));
        }
        attrs.ac |= assoc;
        cx.set_attributes(s, attrs)?;
        Ok(reply(cx, Ok(())))
    }
}

/// Register the definition natives.
pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let fx = |hold| Attributes { hold, effect: true, ..Attributes::default() };
    k.register(h.sym("set_delayed"), fx(Hold::All), Box::new(SetDef));
    k.register(h.sym("set"), fx(Hold::First), Box::new(SetDef));
    k.register(h.sym("define_rule"), fx(Hold::All), Box::new(DefineRule(Heads::new(h))));
    k.register(h.sym("define_fn"), fx(Hold::All), Box::new(DefineFn));
    k.register(h.sym("define_sym"), fx(Hold::All), Box::new(DefineSym));
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
        let opt = c(h, "f", &[c(h, "optional", &[any("x"), h.int(1)])]);
        assert!(matches!(pattern(h, opt), Err(DefError::Unsupported(_))));
    }
}
