//! Matching a pattern term against subjects, for `cases` and `count`.
//!
//! This is a local adapter (decision I.10): `mc-eval` compiles patterns only through `Kernel::compile_pattern`,
//! which a native cannot reach, so the library matches the pattern terms of `mc-syntax` itself. The pattern is first
//! checked with `mc_eval::syntax::pattern` (the evaluator's definition-time checks), then compiled into [`P`] once
//! per call. Matching is continuation-passing, so sequence variables backtrack completely; recursion follows the
//! pattern's structure only (bounded by [`MAX_DEPTH`] and [`MAX_NODES`]), never the subject's.
//!
//! Supported: `_`, `x_`, `x_ : T` (the atomic types of `mc-eval`), `x__`, `x___` (as arguments), `x @ p`,
//! `p when c` (the condition substituted and evaluated; it holds when it gives `true`), `p | q`, literal parts.
//! Arguments of associative and commutative heads match positionally against the canonical order (incomplete:
//! `x_ + y_` matches only sums of two terms). Once `NativeCx` can compile patterns this module should give way to
//! `NativeCx::matches`, which matches modulo AC.
use crate::ctx::{Lib, Meter};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{NativeCx, Res, Ty};
use rustc_hash::FxHashMap;

/// Deepest pattern nesting.
pub(crate) const MAX_DEPTH: usize = 256;
/// Most pattern nodes.
pub(crate) const MAX_NODES: usize = 4096;

/// A compiled pattern.
enum P<'h> {
    /// exactly this term
    Lit(T<'h>),
    /// `_`, `x_`, `x_ : T`
    Blank { var: Option<Sym>, ty: Option<Ty> },
    /// `x__` (min 1), `x___` (min 0)
    Seq { var: Option<Sym>, min: usize },
    /// a compound pattern
    App { head: Box<P<'h>>, args: Vec<P<'h>> },
    /// `p | q | ...`
    Alt(Vec<P<'h>>),
    /// `x @ p`
    Named(Sym, Box<P<'h>>),
    /// `p when c`
    Cond(Box<P<'h>>, T<'h>),
}

impl P<'_> {
    fn min_len(&self) -> usize {
        match self {
            P::Seq { min, .. } => *min,
            _ => 1,
        }
    }
}

/// A variable's value.
#[derive(Clone, Copy)]
enum Bound<'h> {
    One(T<'h>),
    Seq(&'h [T<'h>]),
}

impl PartialEq for Bound<'_> {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (Bound::One(a), Bound::One(b)) => a == b,
            (Bound::Seq(a), Bound::Seq(b)) => a == b,
            _ => false,
        }
    }
}

/// The continuation of a match: what must also hold for the match to succeed.
type K<'a, 'l, 'h> = &'a mut dyn FnMut(&mut Matcher<'l, 'h>, &mut NativeCx<'_, '_, 'h>) -> Res<bool>;

/// A compiled pattern with its bindings.
pub(crate) struct Matcher<'l, 'h> {
    lib: &'l Lib,
    h: &'h Heap,
    pat: P<'h>,
    binds: Vec<(Sym, Bound<'h>)>,
    meter: Meter,
}

/// The type of `x_ : T`, by the names `mc-eval` accepts.
fn type_of(h: &Heap, t: T<'_>) -> Option<Ty> {
    let View::Sym(s) = h.view(t) else { return None };
    Some(match h.sym_name(s).as_str() {
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
        _ => return None,
    })
}

/// Does `t` have type `ty`?
fn has_type(h: &Heap, t: T<'_>, ty: Ty) -> bool {
    let v = h.view(t);
    match ty {
        Ty::Integer => matches!(v, View::Int(_) | View::BigInt { .. }),
        Ty::Rational => matches!(v, View::Rational { .. }),
        Ty::MachineReal => matches!(v, View::Real(_)),
        Ty::Ball => matches!(v, View::Ball { .. }),
        Ty::Real | Ty::Number => {
            matches!(v, View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. })
        }
        Ty::String => matches!(v, View::Str(_)),
        Ty::Symbol => matches!(v, View::Sym(_)),
        Ty::Float => matches!(v, View::Real(_) | View::Ball { .. }),
        Ty::Complex => crate::measure::is_complex_number(h, t),
        Ty::Head(s) => matches!(v, View::App { head, .. } if head == h.sym_term(s)),
    }
}

/// Compilation of a pattern term.
struct Compiler<'l, 'h> {
    lib: &'l Lib,
    h: &'h Heap,
    /// symbol mask of the pattern heads: a subterm without them is a literal
    mask: u64,
    nodes: usize,
    /// the variables seen, and whether each is a sequence variable (one kind per variable)
    kinds: Vec<(Sym, bool)>,
}

impl<'h> Compiler<'_, 'h> {
    /// The variable of `any(x)`, `seq1(x)`, ... (`None` for an anonymous one); `None` overall when the argument is
    /// not a symbol or the variable is used as the other kind elsewhere.
    fn var(&mut self, args: &[T<'h>], seq: bool) -> Option<Option<Sym>> {
        match args {
            [] => Some(None),
            [v] => match self.h.view(*v) {
                View::Sym(s) => self.kind(s, seq).then_some(Some(s)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Record that `v` is a single (`seq` false) or sequence variable; false if it was the other kind.
    fn kind(&mut self, v: Sym, seq: bool) -> bool {
        match self.kinds.iter().find(|(s, _)| *s == v) {
            Some(&(_, k)) => k == seq,
            None => {
                self.kinds.push((v, seq));
                true
            }
        }
    }

    /// `t` as a pattern; `arg`: it is an argument (where sequence variables may stand).
    fn compile(&mut self, t: T<'h>, depth: usize, arg: bool) -> Option<P<'h>> {
        let (h, s) = (self.h, &self.lib.s);
        self.nodes += 1;
        if depth > MAX_DEPTH || self.nodes > MAX_NODES {
            return None;
        }
        if h.mask(t) & self.mask == 0 {
            return Some(P::Lit(t));
        }
        let View::App { head, args } = h.view(t) else {
            return Some(P::Lit(t));
        };
        let hs = match h.view(head) {
            View::Sym(f) => Some(f),
            _ => None,
        };
        Some(match hs {
            Some(f) if f == s.any => P::Blank { var: self.var(args, false)?, ty: None },
            Some(f) if f == s.typed => {
                let [p, ty] = args else { return None };
                let View::App { head: g, args: va } = h.view(*p) else {
                    return None;
                };
                if g != h.sym_term(s.any) {
                    return None;
                }
                P::Blank { var: self.var(va, false)?, ty: Some(type_of(h, *ty)?) }
            }
            Some(f) if (f == s.seq0 || f == s.seq1) && arg => {
                P::Seq { var: self.var(args, true)?, min: usize::from(f == s.seq1) }
            }
            Some(f) if f == s.named => {
                let [v, p] = args else { return None };
                let View::Sym(v) = h.view(*v) else {
                    return None;
                };
                if !self.kind(v, false) {
                    return None;
                }
                P::Named(v, Box::new(self.compile(*p, depth + 1, false)?))
            }
            Some(f) if f == s.condition => {
                let [p, c] = args else { return None };
                P::Cond(Box::new(self.compile(*p, depth + 1, false)?), *c)
            }
            Some(f) if f == s.alternatives => {
                P::Alt(args.iter().map(|&a| self.compile(a, depth + 1, false)).collect::<Option<_>>()?)
            }
            _ => P::App {
                head: Box::new(self.compile(head, depth + 1, false)?),
                args: args.iter().map(|&a| self.compile(a, depth + 1, true)).collect::<Option<_>>()?,
            },
        })
    }
}

impl<'l, 'h> Matcher<'l, 'h> {
    /// The matcher of the pattern term `p`, or `None` when it is not a valid pattern.
    pub fn new(lib: &'l Lib, h: &'h Heap, p: T<'h>) -> Option<Matcher<'l, 'h>> {
        // the evaluator's checks: variable kinds, alternatives, condition scope
        mc_eval::syntax::pattern(h, p).ok()?;
        let s = &lib.s;
        let mask = [s.any, s.seq0, s.seq1, s.typed, s.named, s.condition, s.alternatives]
            .iter()
            .fold(0, |m, &x| m | h.mask(h.sym_term(x)));
        let pat = Compiler { lib, h, mask, nodes: 0, kinds: vec![] }.compile(p, 0, false)?;
        Some(Matcher { lib, h, pat, binds: vec![], meter: Meter::default() })
    }

    /// Does the pattern match `t`? On success the bindings stay until the next call.
    pub fn matches(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<bool> {
        self.binds.clear();
        // the pattern lives in `self`; matching borrows it while the bindings change
        let pat = std::mem::replace(&mut self.pat, P::Lit(t));
        let r = self.m(cx, &pat, t, &mut |_, _| Ok(true));
        self.pat = pat;
        r
    }

    fn m(&mut self, cx: &mut NativeCx<'_, '_, 'h>, p: &P<'h>, t: T<'h>, k: K<'_, 'l, 'h>) -> Res<bool> {
        self.meter.step(cx)?;
        let h = self.h;
        match p {
            P::Lit(u) => {
                if *u == t {
                    k(self, cx)
                } else {
                    Ok(false)
                }
            }
            P::Blank { var, ty } => {
                if ty.is_some_and(|ty| !has_type(h, t, ty)) {
                    return Ok(false);
                }
                self.bind(cx, *var, Bound::One(t), k)
            }
            // compilation puts sequence variables only among arguments
            P::Seq { .. } => Ok(false),
            P::App { head, args } => {
                let View::App { head: th, args: ta } = h.view(t) else {
                    return Ok(false);
                };
                if ta.len() < args.iter().map(P::min_len).sum() {
                    return Ok(false);
                }
                self.m(cx, head, th, &mut |s, cx| s.args(cx, args, ta, k))
            }
            P::Alt(ps) => {
                for q in ps {
                    if self.m(cx, q, t, k)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            P::Named(v, q) => self.m(cx, q, t, &mut |s, cx| s.bind(cx, Some(*v), Bound::One(t), k)),
            P::Cond(q, c) => self.m(cx, q, t, &mut |s, cx| {
                if s.holds(cx, *c)? {
                    k(s, cx)
                } else {
                    Ok(false)
                }
            }),
        }
    }

    /// Match the argument patterns `ps` against the arguments `ts`.
    fn args(&mut self, cx: &mut NativeCx<'_, '_, 'h>, ps: &[P<'h>], ts: &'h [T<'h>], k: K<'_, 'l, 'h>) -> Res<bool> {
        let Some((p, rest)) = ps.split_first() else {
            return if ts.is_empty() { k(self, cx) } else { Ok(false) };
        };
        let need: usize = rest.iter().map(P::min_len).sum();
        if ts.len() < need + p.min_len() {
            return Ok(false);
        }
        match p {
            P::Seq { var, min } => {
                for len in *min..=ts.len() - need {
                    self.meter.step(cx)?;
                    let (now, later) = ts.split_at(len);
                    if self.bind(cx, *var, Bound::Seq(now), &mut |s, cx| s.args(cx, rest, later, k))? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            _ => self.m(cx, p, ts[0], &mut |s, cx| s.args(cx, rest, &ts[1..], k)),
        }
    }

    /// Bind `var` (if named) to `v`, consistently with an earlier binding, and continue.
    fn bind(&mut self, cx: &mut NativeCx<'_, '_, 'h>, var: Option<Sym>, v: Bound<'h>, k: K<'_, 'l, 'h>) -> Res<bool> {
        let Some(var) = var else { return k(self, cx) };
        if let Some((_, old)) = self.binds.iter().find(|(s, _)| *s == var) {
            return if *old == v { k(self, cx) } else { Ok(false) };
        }
        let n = self.binds.len();
        self.binds.push((var, v));
        let r = k(self, cx)?;
        if !r {
            self.binds.truncate(n);
        }
        Ok(r)
    }

    /// Does the condition `c` give `true` with the bindings substituted?
    fn holds(&mut self, cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>) -> Res<bool> {
        let c = self.substitute(cx, c)?;
        let v = cx.eval(c)?;
        Ok(cx.is_true(v))
    }

    /// `t` with every bound variable replaced by its value (a sequence by the list of its terms, semantics 6.1).
    /// Iterative, each shared subterm once.
    pub fn substitute(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
        let h = self.h;
        let bm = self.binds.iter().fold(0, |m, &(s, _)| m | h.mask(h.sym_term(s)));
        if h.mask(t) & bm == 0 {
            return Ok(t);
        }
        let list = h.sym_term(self.lib.s.list);
        let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        let mut stack = vec![(t, false)];
        while let Some((u, expanded)) = stack.pop() {
            self.meter.step(cx)?;
            if done.contains_key(&u) {
                continue;
            }
            if h.mask(u) & bm == 0 {
                done.insert(u, u);
                continue;
            }
            let v = match h.view(u) {
                View::Sym(s) => match self.binds.iter().find(|(v, _)| *v == s) {
                    Some((_, Bound::One(v))) => *v,
                    Some((_, Bound::Seq(vs))) => h.app(list, vs),
                    None => u,
                },
                View::App { head, args } if !expanded => {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                    continue;
                }
                View::App { head, args } => {
                    let args: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
                    h.app(done[&head], &args)
                }
                _ => u,
            };
            done.insert(u, v);
        }
        Ok(done[&t])
    }
}
