//! The decision service: one [`Decider`] per call answers sign, zero, interval, realness and integrality questions
//! about expressions under the facts of an assumption context.
//!
//! **Tiers**, cheapest first; each answer is either proved exactly or certified by ball arithmetic, and a question
//! no tier settles is "unknown" (never a guess):
//!
//! 1. *Structure and facts* ([`Facts`]): the kind and range of the expression from its structure (sums add ranges,
//!    products multiply signs, ...), met with what the facts say of each part; closed numeric parts by ball
//!    refinement (`mc-numeric`), so a value whose enclosure excludes zero has its sign before anything is proved.
//! 2. *Equational facts* ([`crate::basis`]): the expression is reduced modulo the degrevlex Groebner basis of the
//!    equations of the context, and the normal form is decided instead (`a^2 - b^2 + 1` is `1` when `a + b == 0`).
//! 3. *Ball enclosure over the bounds*: when every variable has finite bounds from the facts, each is replaced by a
//!    ball enclosing its interval and the expression is evaluated in ball arithmetic; an enclosure that excludes zero
//!    proves the sign on the whole box (for one variable the interval is also cut into eight pieces). Then the
//!    *quadratic forms* ([`crate::quad`]): a real polynomial of degree at most two is decided by an exact LDL^T
//!    decomposition.
//! 4. *Real root isolation* ([`crate::sturm`]): a polynomial in one real variable on the interval the facts give is
//!    decided by Sturm sequences of its square-free parts, exactly.
//! 5. *`reduce`* for one real variable ([`Reals`]): the facts on the variable conjoined with the negated question
//!    are reduced; an empty set proves the answer. The trait is the hook for several variables (phase 11B).
//!
//! **Memo and time**: every answer is kept for the life of the decider (one call), and the tiers after the first
//! stop at a deadline ([`BUDGET`]), after which questions they would answer stay unknown.
use crate::basis::Basis;
use crate::build::Bld;
use crate::facts::{Bound, Facts, Info, Kind, Range, Syms};
use crate::quad::{definite, definite_on, Definite};
use crate::sturm::{self, End, QP};
use mc_api::{Store, View, T};
use mc_arith::num::{Ball, Float, Int, Mag, Nat, Rat};
use mc_arith::{Num, Real};
use mc_eval::{NativeCx, Res};
use mc_poly::convert::{Conv, Heads};
use mc_poly::work::{Meter, WorkResult};
use rustc_hash::FxHashMap;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The time the tiers after the first may take in one decider.
pub const BUDGET: Duration = Duration::from_secs(3);
/// Most variables of a quadratic form, most degree of a polynomial for root isolation and `reduce`.
const MAX_QUAD_VARS: usize = 8;
const MAX_DEGREE: usize = 48;
/// Bits of the balls of the enclosure tier.
const BALL_BITS: u64 = 128;

/// What is known of the sign of a value.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sign {
    /// `< 0`
    Neg,
    /// `== 0`
    Zero,
    /// `> 0`
    Pos,
    /// `<= 0`
    NonPos,
    /// `>= 0`
    NonNeg,
    /// `!= 0` (real or not)
    NonZero,
    /// real, sign unknown
    Real,
    /// nothing
    Unknown,
}

impl Sign {
    /// The sign as -1, 0 or 1 when it is decided.
    pub fn exact(self) -> Option<i8> {
        match self {
            Sign::Neg => Some(-1),
            Sign::Zero => Some(0),
            Sign::Pos => Some(1),
            _ => None,
        }
    }
    /// Is the value proved positive?
    pub fn is_pos(self) -> bool {
        self == Sign::Pos
    }
    /// Is the value proved negative?
    pub fn is_neg(self) -> bool {
        self == Sign::Neg
    }
    /// Is the value proved `>= 0`?
    pub fn is_nonneg(self) -> bool {
        matches!(self, Sign::Pos | Sign::Zero | Sign::NonNeg)
    }
    /// Is the value proved `<= 0`?
    pub fn is_nonpos(self) -> bool {
        matches!(self, Sign::Neg | Sign::Zero | Sign::NonPos)
    }
    /// Is the value proved nonzero?
    pub fn is_nonzero(self) -> bool {
        matches!(self, Sign::Pos | Sign::Neg | Sign::NonZero)
    }
}

/// The hook of the last tier: is a conjunction of conditions on real variables unsatisfiable?
pub trait Reals<'h> {
    /// `Some(true)` when `conds` (relations, combined by `and`) have no real solution in `vars`, `Some(false)` when
    /// they have one, `None` when not decided.
    fn empty(&mut self, cx: &mut NativeCx<'_, '_, 'h>, conds: &[T<'h>], vars: &[T<'h>]) -> Res<Option<bool>>;
}

/// A maker of the last tier's hook, registered once by the crate that implements it (the decomposition, phase
/// 11B), so that every decider decides questions in several variables.
pub trait RealsFactory: Send + Sync {
    /// A hook for one decider.
    fn make<'h>(&self) -> Box<dyn Reals<'h>>;
}

static REALS: std::sync::OnceLock<Box<dyn RealsFactory>> = std::sync::OnceLock::new();

/// Register the hook of the last tier for every decider made afterwards (the first registration wins).
pub fn register_reals(f: Box<dyn RealsFactory>) {
    let _ = REALS.set(f);
}

/// The default hook: `reduce` for one real variable (by the evaluator, so its answers are memoised); several
/// variables are not decided until `reduce` takes them (phase 11B).
pub struct OneVariable;

impl<'h> Reals<'h> for OneVariable {
    fn empty(&mut self, cx: &mut NativeCx<'_, '_, 'h>, conds: &[T<'h>], vars: &[T<'h>]) -> Res<Option<bool>> {
        let [x] = vars else { return Ok(None) };
        let h = cx.heap();
        let c = if conds.len() == 1 { conds[0] } else { h.call(h.sym("and"), conds) };
        let r = cx.eval(h.call(h.sym("reduce"), &[c, *x, h.sym_term(h.sym("reals"))]))?;
        if cx.is_false(r) {
            return Ok(Some(true));
        }
        // an answer that is a condition on x (not a failure, not the unevaluated call) describes a nonempty set
        let ok = match h.view(r) {
            View::App { head, .. } => matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "less" | "less_equal" | "greater" | "greater_equal" | "equal" | "and" | "or")),
            _ => cx.is_true(r),
        };
        Ok(ok.then_some(false))
    }
}

/// Counts of the tiers that decided something (for tests and measurements).
#[derive(Clone, Copy, Default, Debug)]
pub struct Stats {
    /// questions answered by the memo
    pub memo: u32,
    /// decided by the Groebner normal form
    pub basis: u32,
    /// decided by a ball enclosure
    pub balls: u32,
    /// decided as a quadratic form
    pub quad: u32,
    /// decided by root isolation
    pub sturm: u32,
    /// decided by `reduce`
    pub reduce: u32,
}

/// The decider of one call.
pub struct Decider<'h> {
    /// the facts and the first tier
    pub facts: Facts<'h>,
    /// the builder
    pub b: Rc<Bld<'h>>,
    /// the differences `l - r` of the equational facts `l == r`
    eqs: Vec<T<'h>>,
    /// the compiled basis (`None` until asked)
    basis: Option<Option<Basis<'h>>>,
    memo: FxHashMap<T<'h>, Info<'h>>,
    deadline: Instant,
    reals: Box<dyn Reals<'h>>,
    /// what the tiers decided
    pub stats: Stats,
    /// the facts as given
    terms: Vec<T<'h>>,
    /// the depth of nested tier runs (a normal form is decided by the later tiers only)
    nested: u32,
}

impl<'h> Decider<'h> {
    /// A decider for the facts `facts` (terms as the context holds them).
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, facts: &[T<'h>]) -> Res<Decider<'h>> {
        let h = cx.heap();
        let b = Rc::new(Bld::new(h, stop));
        let syms = Syms::of(h);
        Decider::with(cx, b, syms, facts)
    }

    /// A decider for the facts of the call's assumption context (the result depends on the context).
    pub fn of_context(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>) -> Res<Decider<'h>> {
        let facts = cx.assumptions();
        Decider::new(cx, stop, facts)
    }

    /// A decider with a builder and symbols of the caller.
    pub fn with(cx: &mut NativeCx<'_, '_, 'h>, b: Rc<Bld<'h>>, syms: Arc<Syms>, facts: &[T<'h>]) -> Res<Decider<'h>> {
        let f = Facts::new(cx, b.clone(), syms, facts)?;
        let mut eqs = vec![];
        for (d, rel) in f.relations() {
            if rel == "equal" && !b.is_number(d) {
                eqs.push(d);
            }
        }
        Ok(Decider {
            facts: f,
            b,
            eqs,
            basis: None,
            memo: FxHashMap::default(),
            deadline: Instant::now() + BUDGET,
            reals: REALS.get().map_or_else(|| Box::new(OneVariable) as Box<dyn Reals<'h>>, |f| f.make()),
            stats: Stats::default(),
            terms: facts.to_vec(),
            nested: 0,
        })
    }

    /// Replace the hook of the last tier (several variables, phase 11B).
    pub fn set_reals(&mut self, r: Box<dyn Reals<'h>>) {
        self.reals = r;
    }

    /// The facts as given.
    pub fn fact_terms(&self) -> &[T<'h>] {
        &self.terms
    }

    /// Are there no facts?
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    // --- answers --------------------------------------------------------------------------------------------

    /// The kind and range of `t`, from every tier.
    pub fn info(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Info<'h>> {
        if let Some(i) = self.memo.get(&t) {
            self.stats.memo += 1;
            return Ok(*i);
        }
        let mut i = self.facts.info(cx, t)?;
        if !self.facts.decided(&i) && !self.terms.is_empty() && self.nested == 0 && Instant::now() < self.deadline {
            self.nested += 1;
            let r = self.tiers(cx, t, i);
            self.nested -= 1;
            i = r?;
            self.facts.set_cached(t, i);
        }
        self.memo.insert(t, i);
        Ok(i)
    }

    /// What is known of the sign of `t`.
    pub fn sign(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Sign> {
        let i = self.info(cx, t)?;
        Ok(self.sign_of(&i))
    }

    /// The sign recorded in an [`Info`].
    pub fn sign_of(&self, i: &Info<'h>) -> Sign {
        let f = &self.facts;
        if f.is_zero(i) {
            Sign::Zero
        } else if f.positive(i) {
            Sign::Pos
        } else if f.negative(i) {
            Sign::Neg
        } else if f.nonneg(i) {
            Sign::NonNeg
        } else if f.nonpos(i) {
            Sign::NonPos
        } else if i.range.nonzero {
            Sign::NonZero
        } else if i.is_real() {
            Sign::Real
        } else {
            Sign::Unknown
        }
    }

    /// Is `t` an integer? `Some(true)` when proved, `None` when not known (a non-integer is not proved here).
    pub fn is_integer(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<bool>> {
        if self.b.is_integer(t) {
            return Ok(Some(true));
        }
        if matches!(self.b.heap().view(t), View::Rational { .. }) {
            return Ok(Some(false));
        }
        Ok((self.info(cx, t)?.kind == Kind::Integer).then_some(true))
    }

    /// Is `t` real? `Some(true)` when proved.
    pub fn is_real(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<bool>> {
        Ok(self.info(cx, t)?.is_real().then_some(true))
    }

    /// Is `lo < t < hi` (`lo <= t` when `lo_closed`, `t <= hi` when `hi_closed`)? `None` when not decided.
    pub fn in_interval(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        t: T<'h>,
        lo: T<'h>,
        lo_closed: bool,
        hi: T<'h>,
        hi_closed: bool,
    ) -> Res<Option<bool>> {
        let a = self.compare(cx, if lo_closed { "greater_equal" } else { "greater" }, t, lo)?;
        let c = self.compare(cx, if hi_closed { "less_equal" } else { "less" }, t, hi)?;
        Ok(match (a, c) {
            (Some(true), Some(true)) => Some(true),
            (Some(false), _) | (_, Some(false)) => Some(false),
            _ => None,
        })
    }

    /// The comparison `l rel r` (`rel` the name of a relation head) decided: `None` when not decided or `rel` is
    /// not a relation.
    pub fn compare(&mut self, cx: &mut NativeCx<'_, '_, 'h>, rel: &str, l: T<'h>, r: T<'h>) -> Res<Option<bool>> {
        if !matches!(rel, "less" | "less_equal" | "greater" | "greater_equal" | "equal" | "unequal") {
            return Ok(None);
        }
        // abs(u) against a nonnegative number c: u^2 against c^2 for a real u (item F3)
        if let Some((u, c)) = self.abs_against(cx, l, r)? {
            if rel != "equal" && rel != "unequal" {
                let b = self.b.clone();
                return self.compare(cx, rel, b.powi(u, 2), b.powi(c, 2));
            }
        }
        let d = self.b.sub(l, r);
        let mut i = self.info(cx, d)?;
        if !i.is_real() && !matches!(rel, "equal" | "unequal") {
            return Ok(None);
        }
        let b = self.b.clone();
        if !self.facts.decided(&i) && i.is_real() && b.call_of(d).is_some_and(|(s, _)| s == b.hd.add) {
            // the factored difference may have factors of known sign (`n^3 - n` for an integer n >= 1)
            let fd = cx.eval(b.call_named("factor", &[d]))?;
            if fd != d {
                let j = self.info(cx, fd)?;
                if self.facts.decided(&j) || self.facts.nonneg(&j) || self.facts.nonpos(&j) {
                    i = j;
                }
            }
        }
        let s = self.sign_of(&i);
        let decide = |yes: bool, no: bool| if yes { Some(true) } else if no { Some(false) } else { None };
        Ok(match rel {
            "less" => decide(s.is_neg(), s.is_nonneg()),
            "less_equal" => decide(s.is_nonpos(), s.is_pos()),
            "greater" => decide(s.is_pos(), s.is_nonpos()),
            "greater_equal" => decide(s.is_nonneg(), s.is_neg()),
            "equal" => decide(s == Sign::Zero, s.is_nonzero()),
            _ => decide(s.is_nonzero(), s == Sign::Zero),
        })
    }

    /// `(u, c)` when `l` is `abs(u)` of a real `u` and `r` a nonnegative real number `c`.
    fn abs_against(&mut self, cx: &mut NativeCx<'_, '_, 'h>, l: T<'h>, r: T<'h>) -> Res<Option<(T<'h>, T<'h>)>> {
        let b = self.b.clone();
        let Some((s, [u])) = b.call_of(l) else { return Ok(None) };
        if s != self.facts.syms().abs_sym() || !b.is_rational(r) || b.ar.compare(r, b.int(0)).is_none_or(|c| c == mc_arith::Cmp3::Less) {
            return Ok(None);
        }
        Ok(self.facts.info(cx, *u)?.is_real().then_some((*u, r)))
    }

    /// The truth of a condition: `true`, `false`, relations (also chains `a < b < c`), `and`, `or`, `not`, lists
    /// (as `and`) and `element(x, domain)`; `None` when not decided.
    pub fn truth(&mut self, cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>) -> Res<Option<bool>> {
        cx.tick(1)?;
        if cx.is_true(c) {
            return Ok(Some(true));
        }
        if cx.is_false(c) {
            return Ok(Some(false));
        }
        let b = self.b.clone();
        let Some((name, args)) = b.named_call(c) else { return Ok(None) };
        let name = name.as_str();
        match name {
            "and" | "list" => {
                let mut all = Some(true);
                for &a in args {
                    match self.truth(cx, a)? {
                        Some(false) => return Ok(Some(false)),
                        Some(true) => {}
                        None => all = None,
                    }
                }
                Ok(all)
            }
            "or" => {
                let mut any = Some(false);
                for &a in args {
                    match self.truth(cx, a)? {
                        Some(true) => return Ok(Some(true)),
                        Some(false) => {}
                        None => any = None,
                    }
                }
                Ok(any)
            }
            "not" => match args {
                [a] => Ok(self.truth(cx, *a)?.map(|v| !v)),
                _ => Ok(None),
            },
            "element" => match args {
                [x, d] => self.element(cx, *x, *d),
                _ => Ok(None),
            },
            _ if args.len() >= 2 => {
                let mut all = Some(true);
                for w in args.windows(2) {
                    match self.compare(cx, name, w[0], w[1])? {
                        Some(false) => return Ok(Some(false)),
                        Some(true) => {}
                        None => all = None,
                    }
                }
                Ok(all)
            }
            _ => Ok(None),
        }
    }

    /// `element(x, d)` for the domains of [`crate::facts::DOMAINS`]: `true` when every element of `x` has the
    /// domain's kind and sign, `false` when one has the contrary sign.
    pub fn element(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, d: T<'h>) -> Res<Option<bool>> {
        use crate::facts::{Sgn, DOMAINS};
        let h = self.b.heap();
        let Some(name) = self.b.sym_of(d).map(|s| h.sym_name(s)) else { return Ok(None) };
        let name = name.as_str();
        let Some(&(_, kind, sgn)) = DOMAINS.iter().find(|e| e.0 == name) else { return Ok(None) };
        let (xs, _) = crate::facts::items(&self.b, x);
        let mut all = true;
        for u in xs {
            let i = self.info(cx, u)?;
            let s = self.sign_of(&i);
            let kind_ok = kind == Kind::Unknown && i.is_real() || i.kind <= kind && i.kind != Kind::Unknown;
            let (ok, bad) = match sgn {
                None => (true, false),
                Some(Sgn::Pos) => (s.is_pos(), s.is_nonpos()),
                Some(Sgn::Neg) => (s.is_neg(), s.is_nonneg()),
                Some(Sgn::NonNeg) => (s.is_nonneg(), s.is_neg()),
                Some(Sgn::NonPos) => (s.is_nonpos(), s.is_pos()),
            };
            if bad {
                return Ok(Some(false));
            }
            all &= kind_ok && ok;
        }
        Ok(all.then_some(true))
    }

    /// The terms the facts make integers.
    pub fn integer_terms(&self) -> Vec<T<'h>> {
        self.facts.integer_terms()
    }

    /// The normal form of `t` modulo the equational facts, when it differs from `t`.
    pub fn normal_form(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
        if self.eqs.is_empty() {
            return Ok(None);
        }
        if self.basis.is_none() {
            let eqs = self.eqs.clone();
            self.basis = Some(Basis::of(cx, &eqs)?);
        }
        let Some(Some(basis)) = self.basis.clone() else { return Ok(None) };
        let r = basis.reduce(cx, &self.b.ar, t)?;
        Ok(r.filter(|&r| r != t))
    }

    // --- the tiers after the first ------------------------------------------------------------------------------

    fn tiers(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, mut i: Info<'h>) -> Res<Info<'h>> {
        let mut u = t;
        if let Some(r) = self.normal_form(cx, t)? {
            let ri = self.facts.info(cx, r)?;
            // the normal form has the value of t wherever the facts hold
            let merged = merge(&self.facts, i, ri);
            if self.facts.decided(&merged) {
                self.stats.basis += 1;
                return Ok(merged);
            }
            i = merged;
            u = r;
        }
        let b = self.b.clone();
        let vars = self.variables(u);
        let Some(vars) = vars else { return Ok(i) };
        if vars.is_empty() {
            return Ok(i);
        }
        let mut real = true;
        for &v in &vars {
            real &= self.facts.info(cx, v)?.is_real();
        }
        if !real {
            return Ok(i);
        }
        // balls over the bounds
        if let Some(s) = self.enclose(cx, u, &vars)? {
            self.stats.balls += 1;
            return Ok(with_sign(&b, i, s));
        }
        // polynomial tiers
        if let Some(p) = self.polynomial(cx, u)? {
            let (ring_vars, terms) = p;
            if ring_vars.len() <= MAX_QUAD_VARS && terms.iter().all(|(e, _)| e.iter().sum::<u32>() <= 2) {
                if let Some(s) = quadratic(ring_vars.len(), &terms) {
                    self.stats.quad += 1;
                    i = with_sign(&b, i, s);
                    if self.facts.decided(&i) {
                        return Ok(i);
                    }
                }
            }
            if ring_vars.len() <= MAX_QUAD_VARS && terms.iter().all(|(e, _)| e.iter().sum::<u32>() <= 2) {
                if let Some(s) = self.s_procedure(cx, u)? {
                    self.stats.quad += 1;
                    i = with_sign(&b, i, s);
                    if self.facts.decided(&i) {
                        return Ok(i);
                    }
                }
            }
            if let [x] = ring_vars.as_slice() {
                if let Some(s) = self.isolate(cx, *x, &terms)? {
                    self.stats.sturm += 1;
                    let j = with_sign(&b, i, s);
                    if self.facts.decided(&j) || self.sign_of(&j) != self.sign_of(&i) {
                        i = j;
                    }
                    if self.facts.decided(&i) {
                        return Ok(i);
                    }
                }
            }
        }
        // reduce, for a rational function of one variable (several with a registered hook)
        if vars.len() <= 4 && rational(b.heap(), u) {
            if let Some(s) = self.by_reduce(cx, u, &vars)? {
                self.stats.reduce += 1;
                i = with_sign(&b, i, s);
            }
        }
        Ok(i)
    }

    /// The variables of `t`: its symbols other than the numeric constants (at most 8); `None` when there are more.
    fn variables(&self, t: T<'h>) -> Option<Vec<T<'h>>> {
        let all = self.b.symbols(t, 16)?;
        let nm = &self.facts.syms().num;
        let v: Vec<T<'h>> = all.into_iter().filter(|&s| self.b.sym_of(s).is_some_and(|s| nm.constant(s).is_none())).collect();
        (v.len() <= 8).then_some(v)
    }

    /// The sign of `t` from ball enclosures of the intervals of its variables.
    fn enclose(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, vars: &[T<'h>]) -> Res<Option<(i32, bool)>> {
        let b = self.b.clone();
        let h = b.heap();
        // a machine real inside would make the evaluation inexact and uncertified; only continuous functions whose
        // ball versions enclose their range are evaluated on wide balls
        if !enclosable(h, t) {
            return Ok(None);
        }
        let mut boxes = vec![];
        for &v in vars {
            let iv = self.facts.info(cx, v)?;
            let (Some(lo), Some(hi)) = (iv.range.lo, iv.range.hi) else { return Ok(None) };
            let (Some(a), Some(c)) = (self.ball_of(cx, lo.c)?, self.ball_of(cx, hi.c)?) else { return Ok(None) };
            boxes.push((v, a, c));
        }
        // the whole box first, then (for one variable) eight pieces
        let tries: &[u32] = if boxes.len() == 1 { &[1, 8] } else { &[1] };
        'tries: for &n in tries {
            let mut sign: Option<i32> = None;
            for k in 0..n {
                cx.tick(1)?;
                let mut rules = vec![];
                for (v, a, c) in &boxes {
                    let ball = piece(a, c, k, n);
                    let bt = b.ar.number(&Num::Real(Real::Ball(ball, BALL_BITS)));
                    rules.push(h.call(h.sym(mc_api::names::RULE), &[*v, bt]));
                }
                let rl = cx.list(&rules);
                let r = cx.eval(h.call(h.sym("rewrite"), &[t, rl]))?;
                let s = match b.ar.num(r) {
                    Some(Num::Real(Real::Ball(x, _))) if !x.contains_zero() => {
                        if x.mid.neg {
                            -1
                        } else {
                            1
                        }
                    }
                    Some(Num::Real(Real::Exact(q))) if !q.is_zero() => sturm::rsign(&q),
                    _ => continue 'tries,
                };
                if sign.is_some_and(|x| x != s) {
                    continue 'tries;
                }
                sign = Some(s);
            }
            if let Some(s) = sign {
                return Ok(Some((s, true)));
            }
        }
        Ok(None)
    }

    /// A ball enclosing the real number or closed real constant `c`.
    fn ball_of(&mut self, cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>) -> Res<Option<Ball>> {
        let b = self.b.clone();
        let h = b.heap();
        let v = if b.is_number(c) { c } else { cx.eval(h.call(h.sym("n"), &[c, h.int(40)]))? };
        Ok(match b.ar.num(v) {
            Some(Num::Real(Real::Exact(q))) => rat_ball(&q),
            Some(Num::Real(Real::Ball(x, _))) => Some(x),
            Some(Num::Real(Real::Machine(f))) if f.is_finite() => Some(Ball::exact(float_of(f))),
            _ => None,
        })
    }

    /// `t` as a polynomial over Q in its kernels when every kernel is a real symbol: (kernels, terms).
    #[allow(clippy::type_complexity)]
    fn polynomial(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Vec<T<'h>>, Vec<(Vec<u32>, Rat)>)>> {
        let b = self.b.clone();
        let hd = Heads::new(b.heap());
        let conv = Conv::new(&b.ar, &hd);
        let mut tk = crate::basis::Tick { cx, abort: None };
        let r = {
            let mut m = Meter::new(&mut tk);
            (|| -> WorkResult<Option<(Vec<T<'h>>, Vec<(Vec<u32>, Rat)>)>> {
                let (ring, shared) = conv.ring(&[t], &[], &mut m)?;
                if ring.kernels.iter().any(|&k| b.sym_of(k).is_none()) {
                    return Ok(None);
                }
                let q = conv.to_poly(t, &ring, &shared, &mut m)?;
                if (0..ring.nvars()).any(|v| q.num.degree(v).unwrap_or(0) as usize > MAX_DEGREE) {
                    return Ok(None);
                }
                let terms = (0..q.num.len()).map(|i| (q.num.exps(i).to_vec(), q.coef(i))).collect();
                Ok(Some((ring.kernels.clone(), terms)))
            })()
        };
        if let Some(a) = tk.abort {
            return Err(a);
        }
        Ok(r.ok().flatten())
    }

    /// The sign of the quadratic `t` on the set of one quadratic fact `g >= 0` (or `g > 0`, `g <= 0`, ...) in its
    /// variables, by the S-procedure ([`definite_on`]), for `t` and for `-t`.
    fn s_procedure(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(i32, bool)>> {
        let b = self.b.clone();
        for (d, rel) in self.facts.relations() {
            let (g, strict) = match rel {
                "greater" => (d, true),
                "greater_equal" => (d, false),
                "less" => (b.neg(d), true),
                "less_equal" => (b.neg(d), false),
                _ => continue,
            };
            // a constant fact or one in other variables says nothing here
            let Some(gv) = self.variables(g) else { continue };
            if gv.is_empty() {
                continue;
            }
            let Some((ks, mut terms)) = self.polynomial_pair(cx, t, g)? else { continue };
            let n = ks.len();
            if n > MAX_QUAD_VARS || terms.iter().flatten().any(|(e, _)| e.iter().sum::<u32>() > 2) {
                continue;
            }
            let gt = terms.pop().expect("two polynomials");
            let et = terms.pop().expect("two polynomials");
            cx.tick(16)?;
            match definite_on(n, &et, &gt, strict) {
                Some(Definite::Positive) => return Ok(Some((1, true))),
                Some(Definite::Nonnegative) => return Ok(Some((1, false))),
                None => {}
            }
            let neg: Vec<(Vec<u32>, Rat)> = et.iter().map(|(e, c)| (e.clone(), c.neg())).collect();
            match definite_on(n, &neg, &gt, strict) {
                Some(Definite::Positive) => return Ok(Some((-1, true))),
                Some(Definite::Nonnegative) => return Ok(Some((-1, false))),
                None => {}
            }
        }
        Ok(None)
    }

    /// `t` and `g` as polynomials over Q in one ring of real symbols: (kernels, [terms of t, terms of g]).
    #[allow(clippy::type_complexity)]
    fn polynomial_pair(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, g: T<'h>) -> Res<Option<(Vec<T<'h>>, Vec<Vec<(Vec<u32>, Rat)>>)>> {
        let b = self.b.clone();
        let hd = Heads::new(b.heap());
        let conv = Conv::new(&b.ar, &hd);
        let mut tk = crate::basis::Tick { cx, abort: None };
        let r = {
            let mut m = Meter::new(&mut tk);
            (|| -> WorkResult<Option<(Vec<T<'h>>, Vec<Vec<(Vec<u32>, Rat)>>)>> {
                let (ring, shared) = conv.ring(&[t, g], &[], &mut m)?;
                if ring.kernels.iter().any(|&k| b.sym_of(k).is_none()) {
                    return Ok(None);
                }
                let mut out = vec![];
                for u in [t, g] {
                    let q = conv.to_poly(u, &ring, &shared, &mut m)?;
                    out.push((0..q.num.len()).map(|i| (q.num.exps(i).to_vec(), q.coef(i))).collect());
                }
                Ok(Some((ring.kernels.clone(), out)))
            })()
        };
        if let Some(a) = tk.abort {
            return Err(a);
        }
        let r = r.ok().flatten();
        // every kernel must be a real variable
        if let Some((ks, _)) = &r {
            for &k in ks {
                if !self.facts.info(cx, k)?.is_real() {
                    return Ok(None);
                }
            }
        }
        Ok(r)
    }

    /// Root isolation of the polynomial `terms` in `x` on the interval of `x`.
    fn isolate(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>, terms: &[(Vec<u32>, Rat)]) -> Res<Option<(i32, bool)>> {
        let ix = self.facts.info(cx, x)?;
        let end = |bd: Option<Bound<'h>>, b: &Bld<'h>| -> Option<End> {
            match bd {
                None => Some(End::Inf),
                Some(bd) => exact_rat(b, bd.c).map(|q| End::At(q, !bd.strict)),
            }
        };
        let (Some(lo), Some(hi)) = (end(ix.range.lo, &self.b), end(ix.range.hi, &self.b)) else { return Ok(None) };
        let deg = terms.iter().map(|(e, _)| e[0] as usize).max().unwrap_or(0);
        let mut p: QP = vec![Rat::from_i64(0); deg + 1];
        for (e, c) in terms {
            p[e[0] as usize] = p[e[0] as usize].add(c);
        }
        let mut steps = 0u64;
        let r = sturm::sign_on(&p, &lo, &hi, &mut steps);
        cx.tick(steps)?;
        Ok(r.filter(|&(s, _)| s != 0))
    }

    /// The sign of `t` (in the one variable `x`) by `reduce` of the facts on `x` with each sign case.
    fn by_reduce(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, xs: &[T<'h>]) -> Res<Option<(i32, bool)>> {
        let b = self.b.clone();
        let h = b.heap();
        let mut conds = vec![];
        let zero = b.int(0);
        for (d, rel) in self.facts.relations() {
            let vs = self.variables(d);
            if vs.is_some_and(|vs| !vs.is_empty() && vs.iter().all(|v| xs.contains(v))) {
                conds.push(h.call(h.sym(rel), &[d, zero]));
            }
        }
        if conds.is_empty() {
            return Ok(None);
        }
        let ask = |me: &mut Self, cx: &mut NativeCx<'_, '_, 'h>, rel: &str| -> Res<Option<bool>> {
            if Instant::now() > me.deadline {
                return Ok(None);
            }
            let mut cs = conds.clone();
            cs.push(h.call(h.sym(rel), &[t, zero]));
            me.reals.empty(cx, &cs, xs)
        };
        if ask(self, cx, "less_equal")? == Some(true) {
            return Ok(Some((1, true)));
        }
        if ask(self, cx, "greater_equal")? == Some(true) {
            return Ok(Some((-1, true)));
        }
        if ask(self, cx, "less")? == Some(true) {
            return Ok(Some((1, false)));
        }
        if ask(self, cx, "greater")? == Some(true) {
            return Ok(Some((-1, false)));
        }
        Ok(None)
    }
}

/// Is `t` a rational function of its symbols with rational coefficients (sums, products, integer powers)?
fn rational<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    let mut stack = vec![t];
    let mut seen = 0;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 1024 {
            return false;
        }
        match h.view(u) {
            View::Sym(_) | View::Int(_) | View::BigInt { .. } | View::Rational { .. } => {}
            View::App { head, args } => {
                let ok = match h.view(head) {
                    View::Sym(s) => {
                        let n = h.sym_name(s);
                        n == mc_api::names::ADD || n == mc_api::names::MUL || n == mc_api::names::POW && args.len() == 2 && matches!(h.view(args[1]), View::Int(_))
                    }
                    _ => false,
                };
                if !ok {
                    return false;
                }
                stack.extend_from_slice(args);
            }
            _ => return false,
        }
    }
    true
}

/// May `t` be evaluated on wide balls: no machine real, and only sums, products, powers and continuous elementary
/// functions (whose ball versions enclose their range on the ball, failing where they are not defined)?
fn enclosable<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    const HEADS: &[&str] = &[mc_api::names::ADD, mc_api::names::MUL, mc_api::names::POW, "exp", "log", "sin", "cos", "sinh", "cosh", "tanh", "atan", "asinh"];
    let mut stack = vec![t];
    let mut seen = 0;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 2048 {
            return false;
        }
        match h.view(u) {
            View::Real(_) | View::Str(_) => return false,
            View::App { head, args } => {
                let ok = match h.view(head) {
                    View::Sym(s) => HEADS.contains(&h.sym_name(s).as_str()) || h.sym_name(s) == mc_api::names::COMPLEX,
                    _ => false,
                };
                if !ok {
                    return false;
                }
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    true
}

/// The exact rational of a real number term (a machine real is a dyadic rational).
fn exact_rat<'h>(b: &Bld<'h>, c: T<'h>) -> Option<Rat> {
    match b.ar.num(c)? {
        Num::Real(Real::Exact(q)) => Some(q),
        Num::Real(Real::Machine(f)) if f.is_finite() => Some(rat_of_f64(f)),
        _ => None,
    }
}

/// The exact value of a finite machine real.
fn rat_of_f64(f: f64) -> Rat {
    let fl = float_of(f);
    let m = Int::new(fl.neg, fl.man.clone());
    if fl.exp >= 0 {
        Rat::from_int(m.shl(fl.exp as u64))
    } else {
        Rat::new(m, Int::from_nat(Nat::from_u64(1).shl(fl.exp.unsigned_abs())))
    }
}

/// A finite machine real as an exact float.
fn float_of(f: f64) -> Float {
    if f == 0.0 {
        return Float::zero();
    }
    let bits = f.to_bits();
    let neg = bits >> 63 == 1;
    let e = ((bits >> 52) & 0x7ff) as i64;
    let frac = bits & ((1u64 << 52) - 1);
    let (man, exp) = if e == 0 { (frac, -1074) } else { (frac | (1u64 << 52), e - 1075) };
    Float::new(neg, Nat::from_u64(man), exp)
}

/// A ball enclosing a rational.
fn rat_ball(q: &Rat) -> Option<Ball> {
    let n = Ball::exact(Float::new(q.num.neg, q.num.mag.clone(), 0));
    if q.den == Nat::from_u64(1) {
        return Some(n);
    }
    let d = Ball::exact(Float::new(false, q.den.clone(), 0));
    n.div(&d, BALL_BITS)
}

/// The ball enclosing piece `k` of `n` (a power of two) of the hull of the balls `a` and `c`.
fn piece(a: &Ball, c: &Ball, k: u32, n: u32) -> Ball {
    let sh = n.trailing_zeros() as i64;
    let w = c.mid.sub_exact(&a.mid);
    let at = |j: u32| {
        let x = w.mul_exact(&Float::from_i64(j as i64));
        let x = Float::new(x.neg, x.man.clone(), x.exp - sh);
        a.mid.add_exact(&x)
    };
    let (p, q) = (at(k), at(k + 1));
    let s = p.add_exact(&q);
    let mid = Float::new(s.neg, s.man.clone(), s.exp - 1);
    let half = Mag::from_float_up(&q.sub_exact(&p).abs()).mul_2exp(-1);
    Ball { mid, rad: half.add(&a.rad).add(&c.rad) }
}

/// `i` with the sign `(s, strict)` recorded.
fn with_sign<'h>(b: &Bld<'h>, mut i: Info<'h>, (s, strict): (i32, bool)) -> Info<'h> {
    let z = b.int(0);
    if i.kind > Kind::Real {
        i.kind = Kind::Real;
    }
    match s {
        1 => {
            i.range.lo = Some(Bound { c: z, strict });
        }
        -1 => {
            i.range.hi = Some(Bound { c: z, strict });
        }
        _ => {
            i.range.lo = Some(Bound { c: z, strict: false });
            i.range.hi = Some(Bound { c: z, strict: false });
        }
    }
    if strict && s != 0 {
        i.range.nonzero = true;
    }
    i
}

/// The information of a value known in two ways (the normal form `r` of `t`): `r`'s when it says more.
fn merge<'h>(f: &Facts<'h>, i: Info<'h>, r: Info<'h>) -> Info<'h> {
    if f.decided(&r) || r.is_real() && !i.is_real() {
        return Info { kind: i.kind.min(r.kind), range: r.range };
    }
    let mut out = i;
    out.kind = i.kind.min(r.kind);
    if out.range.lo.is_none() {
        out.range.lo = r.range.lo;
    }
    if out.range.hi.is_none() {
        out.range.hi = r.range.hi;
    }
    out.range.nonzero |= r.range.nonzero;
    if !out.is_real() {
        out.range = Range { nonzero: out.range.nonzero, ..Range::default() };
    }
    out
}

/// The sign of a quadratic form, both ways.
fn quadratic(n: usize, terms: &[(Vec<u32>, Rat)]) -> Option<(i32, bool)> {
    match definite(n, terms) {
        Some(Definite::Positive) => return Some((1, true)),
        Some(Definite::Nonnegative) => return Some((1, false)),
        None => {}
    }
    let neg: Vec<(Vec<u32>, Rat)> = terms.iter().map(|(e, c)| (e.clone(), c.neg())).collect();
    match definite(n, &neg) {
        Some(Definite::Positive) => Some((-1, true)),
        Some(Definite::Nonnegative) => Some((-1, false)),
        None => None,
    }
}

/// The relation heads of facts (for callers that walk conditions).
pub fn is_relation(name: &str) -> bool {
    matches!(name, "less" | "less_equal" | "greater" | "greater_equal" | "equal" | "unequal")
}

/// The sign of `t` under the facts of the call's assumption context, by every tier (the one-question entry point for
/// callers without a decider of their own: `limit`'s and `integrate`'s sign helpers). A number is decided without
/// building a decider.
pub fn sign_in_context<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Sign> {
    let h = cx.heap();
    let ar = mc_arith::Arith::new(h);
    if let Some(Num::Real(x)) = ar.num(t) {
        return Ok(match mc_arith::number::compare(&x, &Real::int(0)) {
            mc_arith::Cmp3::Less => Sign::Neg,
            mc_arith::Cmp3::Equal => Sign::Zero,
            mc_arith::Cmp3::Greater => Sign::Pos,
            _ => Sign::Real,
        });
    }
    // the arithmetic flag of the decider is never raised: the calling native's stop scope polls the engine's
    let flag = Arc::new(AtomicBool::new(false));
    let mut d = Decider::of_context(cx, &flag)?;
    d.sign(cx, t)
}
