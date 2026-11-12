//! Facts of an assumption context (semantics 13) and the decisions they support: the sign, the bounds, realness
//! and integrality of an expression.
//!
//! Since item F3 this is an adapter over `mc-decide`'s decision service ([`mc_decide::Decider`]): the facts are
//! compiled there once per call (order facts into bounds, equations into a Groebner basis), and every question of
//! this crate (`refine`'s rules, `simplify`'s sign decisions, `power_expand`, the predicates `is_positive`, ...)
//! is answered by its tiers: structure and facts, ball refinement, the normal form modulo the equations, ball
//! enclosures over the bounds, quadratic forms, root isolation and `reduce` for one variable. Every answer is
//! sound; "unknown" is always allowed.
use crate::term::B;
use mc_api::{Store, Sym, T};
use mc_core::Heap;
use mc_decide::{Bld, Decider};
use mc_eval::{NativeCx, Res};
use mc_numeric::numeval;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use mc_decide::facts::{Bound, Info, Kind, Range, Sgn, DOMAINS, MAX_DEPTH};

/// The symbols facts and decisions look at, interned once per session.
pub struct Syms {
    /// the decision service's symbols
    inner: Arc<mc_decide::Syms>,
    relations: [Sym; 6],
    pub(crate) abs: Sym,
    pub(crate) sign: Sym,
    pub(crate) pi: Sym,
    /// `mc-numeric`'s names for deciding closed numeric expressions
    pub(crate) num: numeval::Names,
    /// the deciders' arithmetic flag (never raised: the calling native's stop scope polls the engine's flag)
    flag: Arc<AtomicBool>,
}

impl Syms {
    /// The symbols of heap `h`.
    pub fn new(h: &Heap) -> Syms {
        let r = ["less", "less_equal", "greater", "greater_equal", "equal", "unequal"].map(|n| h.sym(n));
        Syms {
            inner: Arc::new(mc_decide::Syms::new(h)),
            relations: r,
            abs: h.sym("abs"),
            sign: h.sym("sign"),
            pi: h.sym("pi"),
            num: numeval::Names::new(h),
            flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Is `s` a comparison head?
    pub(crate) fn relation(&self, s: Sym) -> Option<()> {
        self.relations.contains(&s).then_some(())
    }

    /// `true`, `false` or `None` for a comparison `l op r` decided by the facts.
    pub fn compare<'h>(&self, f: &mut Facts<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, s: Sym, l: T<'h>, r: T<'h>) -> Res<Option<bool>> {
        let name = cx.heap().sym_name(s);
        f.d.compare(cx, &name, l, r)
    }
}

/// The facts of one decision: the decider of the call and the caller's builder.
pub struct Facts<'a, 'h> {
    /// the builder (canonical arithmetic)
    pub b: &'a B<'h>,
    s: &'a Syms,
    /// the decision service
    pub d: Decider<'h>,
}

impl<'a, 'h> Facts<'a, 'h> {
    /// The facts `facts` (terms, as the context holds them).
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, b: &'a B<'h>, s: &'a Syms, facts: &[T<'h>]) -> Res<Facts<'a, 'h>> {
        let bld = Rc::new(Bld::new(b.heap(), &s.flag));
        let d = Decider::with(cx, bld, s.inner.clone(), facts)?;
        Ok(Facts { b, s, d })
    }

    /// The symbols.
    pub fn syms(&self) -> &'a Syms {
        self.s
    }

    /// Are there no facts?
    pub fn is_empty(&self) -> bool {
        self.d.is_empty()
    }

    /// The kind and range of `t`.
    pub fn info(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Info<'h>> {
        self.d.info(cx, t)
    }

    /// Is the value positive?
    pub fn positive(&self, i: &Info<'h>) -> bool {
        self.d.facts.positive(i)
    }
    /// Is the value negative?
    pub fn negative(&self, i: &Info<'h>) -> bool {
        self.d.facts.negative(i)
    }
    /// Is the value nonnegative?
    pub fn nonneg(&self, i: &Info<'h>) -> bool {
        self.d.facts.nonneg(i)
    }
    /// Is the value nonpositive?
    pub fn nonpos(&self, i: &Info<'h>) -> bool {
        self.d.facts.nonpos(i)
    }
    /// Is the value zero?
    pub fn is_zero(&self, i: &Info<'h>) -> bool {
        self.d.facts.is_zero(i)
    }
    /// Is the sign of the value known?
    pub fn decided(&self, i: &Info<'h>) -> bool {
        self.d.facts.decided(i)
    }

    /// Whether the integer `n` is even (`Some(true)`), odd (`Some(false)`), or not known.
    pub fn parity(&mut self, cx: &mut NativeCx<'_, '_, 'h>, n: T<'h>, d: u32) -> Res<Option<bool>> {
        self.d.facts.parity(cx, n, d)
    }

    /// The truth of a condition under the facts (`and`, `or`, `not`, relations, `element`).
    pub fn truth(&mut self, cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>) -> Res<Option<bool>> {
        self.d.truth(cx, c)
    }
}
