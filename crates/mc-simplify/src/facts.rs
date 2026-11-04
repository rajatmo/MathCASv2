//! Facts of an assumption context (semantics 13) and the decisions they support: the sign, the bounds, realness
//! and integrality of an expression.
//!
//! **Facts** (decision A.3): `a < b`, `a <= b`, `a > b`, `a >= b`, `a == b`, `a != b` for any expressions,
//! `element(x, domain)` (also `element([x, y], domain)`) with the domains of [`DOMAINS`], and `and`s and lists of
//! them (the evaluator flattens those). Anything else is ignored: an unknown fact proves nothing. An order fact
//! makes both sides real.
//!
//! **Decisions** come from [`Facts::info`]: a kind (integer, rational, real or unknown) and a range (lower and upper
//! bounds, strict or not, and whether the value is known to be nonzero), found from the structure of the expression
//! (sums add ranges, products multiply signs, even powers of reals are nonnegative, `exp` of a real is positive,
//! ...) met with what the facts say about the expression itself (`e op c` for a number `c`, and `a op b` when
//! `a - b` is `e` or `-e`). Closed numeric expressions (`pi - 3`) are decided by `mc-numeric`'s refinement. Every
//! answer is sound; "unknown" is always allowed. The recursion over the expression is bounded by [`MAX_DEPTH`].
use crate::term::B;
use mc_api::{Store, Sym, View, T};
use mc_arith::number::Cmp3;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use mc_numeric::numeval;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;

/// Deeper subterms are not looked at: their kind and range are unknown.
pub const MAX_DEPTH: u32 = 48;

/// The domains of `element(x, domain)`: name, kind, and the sign fact it carries.
pub const DOMAINS: &[(&str, Kind, Option<Sgn>)] = &[
    ("integers", Kind::Integer, None),
    ("rationals", Kind::Rational, None),
    ("reals", Kind::Real, None),
    ("complexes", Kind::Unknown, None),
    ("positive_integers", Kind::Integer, Some(Sgn::Pos)),
    ("negative_integers", Kind::Integer, Some(Sgn::Neg)),
    ("nonnegative_integers", Kind::Integer, Some(Sgn::NonNeg)),
    ("nonpositive_integers", Kind::Integer, Some(Sgn::NonPos)),
    ("positive_rationals", Kind::Rational, Some(Sgn::Pos)),
    ("negative_rationals", Kind::Rational, Some(Sgn::Neg)),
    ("positive_reals", Kind::Real, Some(Sgn::Pos)),
    ("negative_reals", Kind::Real, Some(Sgn::Neg)),
    ("nonnegative_reals", Kind::Real, Some(Sgn::NonNeg)),
    ("nonpositive_reals", Kind::Real, Some(Sgn::NonPos)),
];

/// What is known of the kind of a value, from the most to the least specific.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Kind {
    /// an integer
    Integer,
    /// a rational number
    Rational,
    /// a real number
    Real,
    /// nothing (possibly not real)
    Unknown,
}

/// A sign condition of a domain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sgn {
    /// `> 0`
    Pos,
    /// `< 0`
    Neg,
    /// `>= 0`
    NonNeg,
    /// `<= 0`
    NonPos,
}

/// A relation of an order or equality fact.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Op {
    /// The relation with its sides exchanged (`a < b` is `b > a`).
    fn flip(self) -> Op {
        match self {
            Op::Lt => Op::Gt,
            Op::Le => Op::Ge,
            Op::Gt => Op::Lt,
            Op::Ge => Op::Le,
            o => o,
        }
    }
    fn is_order(self) -> bool {
        matches!(self, Op::Lt | Op::Le | Op::Gt | Op::Ge)
    }
}

/// A bound: a real number and whether it is excluded.
#[derive(Clone, Copy, Debug)]
pub struct Bound<'h> {
    /// the number
    pub c: T<'h>,
    /// the bound itself is excluded
    pub strict: bool,
}

/// What is known of the real value of an expression: bounds (only for a known real) and whether it is nonzero.
#[derive(Clone, Copy, Debug, Default)]
pub struct Range<'h> {
    /// a lower bound
    pub lo: Option<Bound<'h>>,
    /// an upper bound
    pub hi: Option<Bound<'h>>,
    /// known to be nonzero
    pub nonzero: bool,
}

/// The kind and range of an expression.
#[derive(Clone, Copy, Debug)]
pub struct Info<'h> {
    /// the kind
    pub kind: Kind,
    /// the range (empty unless the kind is at least real)
    pub range: Range<'h>,
}

impl<'h> Info<'h> {
    const UNKNOWN: Info<'static> = Info { kind: Kind::Unknown, range: Range { lo: None, hi: None, nonzero: false } };

    fn unknown() -> Info<'h> {
        Info::UNKNOWN
    }
    fn real(range: Range<'h>) -> Info<'h> {
        Info { kind: Kind::Real, range }
    }
    /// Is the value a real number?
    pub fn is_real(&self) -> bool {
        self.kind <= Kind::Real
    }
}

/// One order or equality fact `l op r`, with `d = l - r` for matching differences.
#[derive(Clone, Copy)]
struct Rel<'h> {
    l: T<'h>,
    op: Op,
    r: T<'h>,
    d: T<'h>,
    neg_d: T<'h>,
}

/// The symbols facts and decisions look at, interned once per session.
pub struct Syms {
    less: Sym,
    less_equal: Sym,
    greater: Sym,
    greater_equal: Sym,
    equal: Sym,
    unequal: Sym,
    element: Sym,
    pub(crate) abs: Sym,
    pub(crate) sign: Sym,
    pub(crate) pi: Sym,
    atan: Sym,
    sinh: Sym,
    tanh: Sym,
    cosh: Sym,
    floor: Sym,
    ceiling: Sym,
    round: Sym,
    domains: Vec<(Sym, Kind, Option<Sgn>)>,
    /// `mc-numeric`'s names for deciding closed numeric expressions
    pub(crate) num: numeval::Names,
}

impl Syms {
    /// The symbols of heap `h`.
    pub fn new(h: &Heap) -> Syms {
        Syms {
            less: h.sym("less"),
            less_equal: h.sym("less_equal"),
            greater: h.sym("greater"),
            greater_equal: h.sym("greater_equal"),
            equal: h.sym("equal"),
            unequal: h.sym("unequal"),
            element: h.sym("element"),
            abs: h.sym("abs"),
            sign: h.sym("sign"),
            pi: h.sym("pi"),
            atan: h.sym("atan"),
            sinh: h.sym("sinh"),
            tanh: h.sym("tanh"),
            cosh: h.sym("cosh"),
            floor: h.sym("floor"),
            ceiling: h.sym("ceiling"),
            round: h.sym("round"),
            domains: DOMAINS.iter().map(|&(n, k, s)| (h.sym(n), k, s)).collect(),
            num: numeval::Names::new(h),
        }
    }

    /// Is `s` a comparison head?
    pub(crate) fn relation(&self, s: Sym) -> Option<()> {
        self.op(s).map(|_| ())
    }

    /// The relation of a comparison head.
    fn op(&self, s: Sym) -> Option<Op> {
        Some(match s {
            _ if s == self.less => Op::Lt,
            _ if s == self.less_equal => Op::Le,
            _ if s == self.greater => Op::Gt,
            _ if s == self.greater_equal => Op::Ge,
            _ if s == self.equal => Op::Eq,
            _ if s == self.unequal => Op::Ne,
            _ => return None,
        })
    }

    /// `true`, `false` or `None` for a comparison `l op r` decided by the facts.
    pub fn compare<'h>(&self, f: &mut Facts<'_, 'h>, cx: &mut NativeCx<'_, '_, 'h>, s: Sym, l: T<'h>, r: T<'h>) -> Res<Option<bool>> {
        let Some(op) = self.op(s) else { return Ok(None) };
        let d = f.b.sub(l, r);
        let i = f.info(cx, d)?;
        if !i.is_real() {
            return Ok(None);
        }
        let (pos, neg, zero) = (f.positive(&i), f.negative(&i), f.is_zero(&i));
        let (nonneg, nonpos, nonzero) = (f.nonneg(&i), f.nonpos(&i), i.range.nonzero || pos || neg);
        Ok(match op {
            Op::Lt => decide(neg, nonneg),
            Op::Le => decide(nonpos, pos),
            Op::Gt => decide(pos, nonpos),
            Op::Ge => decide(nonneg, neg),
            Op::Eq => decide(zero, nonzero),
            Op::Ne => decide(nonzero, zero),
        })
    }
}

/// `Some(true)` when `yes` is proved, `Some(false)` when `no` is.
fn decide(yes: bool, no: bool) -> Option<bool> {
    match (yes, no) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// The facts of one decision, with a cache of the expressions looked at.
pub struct Facts<'a, 'h> {
    /// the builder (canonical arithmetic)
    pub b: &'a B<'h>,
    s: &'a Syms,
    rels: Vec<Rel<'h>>,
    kinds: FxHashMap<T<'h>, Kind>,
    cache: FxHashMap<T<'h>, Info<'h>>,
    closed: FxHashMap<T<'h>, bool>,
}

impl<'a, 'h> Facts<'a, 'h> {
    /// The facts `facts` (terms, as the context holds them).
    pub fn new(cx: &mut NativeCx<'_, '_, 'h>, b: &'a B<'h>, s: &'a Syms, facts: &[T<'h>]) -> Res<Facts<'a, 'h>> {
        let mut f = Facts { b, s, rels: vec![], kinds: FxHashMap::default(), cache: FxHashMap::default(), closed: FxHashMap::default() };
        for &t in facts {
            cx.tick(1)?;
            f.add(t);
        }
        Ok(f)
    }

    /// The symbols.
    pub fn syms(&self) -> &'a Syms {
        self.s
    }

    /// Are there no facts?
    pub fn is_empty(&self) -> bool {
        self.rels.is_empty() && self.kinds.is_empty()
    }

    fn add(&mut self, t: T<'h>) {
        let Some((s, args)) = self.b.call_of(t) else { return };
        if let (Some(op), [l, r]) = (self.s.op(s), args) {
            self.relation(*l, op, *r);
        } else if let (true, [x, dom]) = (s == self.s.element, args) {
            let Some(&(_, kind, sgn)) = self.b.sym_of(*dom).and_then(|d| self.s.domains.iter().find(|e| e.0 == d)) else {
                return;
            };
            let xs = match self.b.call_of(*x) {
                Some((l, items)) if l == self.b.hd.list => items.to_vec(),
                _ => vec![*x],
            };
            for x in xs {
                self.kind_fact(x, kind);
                let zero = self.b.int(0);
                match sgn {
                    Some(Sgn::Pos) => self.relation(x, Op::Gt, zero),
                    Some(Sgn::Neg) => self.relation(x, Op::Lt, zero),
                    Some(Sgn::NonNeg) => self.relation(x, Op::Ge, zero),
                    Some(Sgn::NonPos) => self.relation(x, Op::Le, zero),
                    None => {}
                }
            }
        }
    }

    fn kind_fact(&mut self, x: T<'h>, k: Kind) {
        let e = self.kinds.entry(x).or_insert(Kind::Unknown);
        *e = (*e).min(k);
    }

    /// The fact `l op r`, with a number on the right when there is one.
    fn relation(&mut self, l: T<'h>, op: Op, r: T<'h>) {
        let (l, op, r) = if self.b.is_number(l) && !self.b.is_number(r) { (r, op.flip(), l) } else { (l, op, r) };
        if op.is_order() {
            self.kind_fact(l, Kind::Real);
            self.kind_fact(r, Kind::Real);
        }
        let d = self.b.sub(l, r);
        let neg_d = self.b.sub(r, l);
        self.rels.push(Rel { l, op, r, d, neg_d });
    }

    // --- decisions --------------------------------------------------------------------------------------------

    /// Is the value positive?
    pub fn positive(&self, i: &Info<'h>) -> bool {
        i.is_real() && i.range.lo.is_some_and(|lo| self.sgn(lo.c).is_some_and(|o| o == Ordering::Greater || o == Ordering::Equal && lo.strict))
    }
    /// Is the value negative?
    pub fn negative(&self, i: &Info<'h>) -> bool {
        i.is_real() && i.range.hi.is_some_and(|hi| self.sgn(hi.c).is_some_and(|o| o == Ordering::Less || o == Ordering::Equal && hi.strict))
    }
    /// Is the value nonnegative?
    pub fn nonneg(&self, i: &Info<'h>) -> bool {
        i.is_real() && i.range.lo.is_some_and(|lo| self.sgn(lo.c).is_some_and(|o| o != Ordering::Less))
    }
    /// Is the value nonpositive?
    pub fn nonpos(&self, i: &Info<'h>) -> bool {
        i.is_real() && i.range.hi.is_some_and(|hi| self.sgn(hi.c).is_some_and(|o| o != Ordering::Greater))
    }
    /// Is the value zero?
    pub fn is_zero(&self, i: &Info<'h>) -> bool {
        self.nonneg(i) && self.nonpos(i)
    }

    /// The sign of a real number term.
    fn sgn(&self, c: T<'h>) -> Option<Ordering> {
        self.cmp(c, self.b.int(0))
    }

    /// The order of two real number terms.
    fn cmp(&self, a: T<'h>, c: T<'h>) -> Option<Ordering> {
        match self.b.ar.compare(a, c)? {
            Cmp3::Less => Some(Ordering::Less),
            Cmp3::Equal => Some(Ordering::Equal),
            Cmp3::Greater => Some(Ordering::Greater),
            _ => None,
        }
    }

    /// The kind and range of `t`.
    pub fn info(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Info<'h>> {
        self.info_at(cx, t, 0)
    }

    fn info_at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: u32) -> Res<Info<'h>> {
        if let Some(i) = self.cache.get(&t) {
            return Ok(*i);
        }
        if depth > MAX_DEPTH {
            return Ok(Info::unknown());
        }
        cx.tick(1)?;
        let mut i = self.structure(cx, t, depth)?;
        self.meet_facts(&mut i, t);
        if !self.decided(&i) && self.is_closed(t) {
            self.numeric(&mut i, t);
        }
        self.cache.insert(t, i);
        Ok(i)
    }

    /// Is the sign of the value known?
    fn decided(&self, i: &Info<'h>) -> bool {
        self.positive(i) || self.negative(i) || self.is_zero(i)
    }

    /// Is `t` a closed expression: no symbols in argument positions other than the numeric constants?
    fn is_closed(&mut self, t: T<'h>) -> bool {
        if let Some(&c) = self.closed.get(&t) {
            return c;
        }
        let h = self.b.heap();
        let mut stack = vec![t];
        let mut seen = 0usize;
        let mut ok = true;
        while let Some(u) = stack.pop() {
            seen += 1;
            if seen > 4096 {
                ok = false;
                break;
            }
            match h.view(u) {
                View::Sym(s) if self.s.num.constant(s).is_none() => {
                    ok = false;
                    break;
                }
                View::App { args, .. } => stack.extend(args.iter().copied()),
                View::Str(_) => {
                    ok = false;
                    break;
                }
                _ => {}
            }
        }
        self.closed.insert(t, ok);
        ok
    }

    /// Decide a closed numeric `t` by refinement (as `is_positive` does without facts).
    fn numeric(&self, i: &mut Info<'h>, t: T<'h>) {
        let h = self.b.heap();
        if numeval::is_real(h, &self.b.ar, &self.s.num, t) != Some(true) {
            return;
        }
        let zero = Bound { c: self.b.int(0), strict: true };
        i.kind = i.kind.min(Kind::Real);
        match numeval::sign(h, &self.b.ar, &self.s.num, t) {
            Some(Ordering::Greater) => meet_lo(self, &mut i.range, zero),
            Some(Ordering::Less) => meet_hi(self, &mut i.range, zero),
            Some(Ordering::Equal) => {
                let z = Bound { strict: false, ..zero };
                meet_lo(self, &mut i.range, z);
                meet_hi(self, &mut i.range, z);
            }
            None => {}
        }
        if self.positive(i) || self.negative(i) {
            i.range.nonzero = true;
        }
    }

    /// What the facts say about `t` itself, met into `i`.
    fn meet_facts(&self, i: &mut Info<'h>, t: T<'h>) {
        if let Some(&k) = self.kinds.get(&t) {
            i.kind = i.kind.min(k);
        }
        let zero = self.b.int(0);
        for rel in &self.rels {
            let (op, c) = if rel.l == t && self.b.is_number(rel.r) {
                (rel.op, rel.r)
            } else if rel.d == t {
                (rel.op, zero)
            } else if rel.neg_d == t {
                (rel.op.flip(), zero)
            } else {
                continue;
            };
            if !self.b.ar.num(c).is_some_and(|n| matches!(n, mc_arith::number::Num::Real(_))) {
                continue;
            }
            let r = &mut i.range;
            match op {
                Op::Lt => meet_hi(self, r, Bound { c, strict: true }),
                Op::Le => meet_hi(self, r, Bound { c, strict: false }),
                Op::Gt => meet_lo(self, r, Bound { c, strict: true }),
                Op::Ge => meet_lo(self, r, Bound { c, strict: false }),
                Op::Eq => {
                    meet_lo(self, r, Bound { c, strict: false });
                    meet_hi(self, r, Bound { c, strict: false });
                    if i.kind > Kind::Real {
                        i.kind = Kind::Real;
                    }
                }
                Op::Ne => {
                    if self.sgn(c) == Some(Ordering::Equal) {
                        r.nonzero = true;
                    }
                }
            }
        }
        if !i.is_real() {
            // bounds mean nothing for a value that may not be real
            i.range.lo = None;
            i.range.hi = None;
        } else if self.positive(i) || self.negative(i) {
            i.range.nonzero = true;
        }
    }

    /// The kind and range of `t` from its structure.
    fn structure(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: u32) -> Res<Info<'h>> {
        let b = self.b;
        let h = b.heap();
        // numbers
        match h.view(t) {
            View::Int(_) | View::BigInt { .. } => return Ok(self.point(t, Kind::Integer)),
            View::Rational { .. } => return Ok(self.point(t, Kind::Rational)),
            View::Real(_) | View::Ball { .. } => return Ok(self.point(t, Kind::Real)),
            View::App { .. } => {}
            _ => return Ok(Info::unknown()),
        }
        let Some((s, args)) = b.call_of(t) else { return Ok(Info::unknown()) };
        let hd = &b.hd;
        let d = depth + 1;
        if s == hd.add {
            let mut acc = self.info_at(cx, args[0], d)?;
            for &a in &args[1..] {
                let i = self.info_at(cx, a, d)?;
                acc = self.sum(&acc, &i);
            }
            return Ok(acc);
        }
        if s == hd.mul {
            return self.product(cx, args, d);
        }
        if let (true, [base, ex]) = (s == hd.pow, args) {
            if b.sym_of(*base) == Some(hd.e) {
                return self.exp(cx, *ex, d);
            }
            return self.power(cx, *base, *ex, d);
        }
        let [u] = args else { return Ok(Info::unknown()) };
        let u = *u;
        let zero = b.int(0);
        let one = b.int(1);
        Ok(if s == hd.exp {
            self.exp(cx, u, d)?
        } else if s == self.s.abs {
            let iu = self.info_at(cx, u, d)?;
            let kind = if iu.kind <= Kind::Rational { iu.kind } else { Kind::Real };
            let mut r = Range { lo: Some(Bound { c: zero, strict: false }), hi: None, nonzero: iu.range.nonzero };
            if iu.range.nonzero || self.positive(&iu) || self.negative(&iu) {
                r.nonzero = true;
                r.lo = Some(Bound { c: zero, strict: true });
            }
            Info { kind, range: r }
        } else if s == self.s.sign {
            let iu = self.info_at(cx, u, d)?;
            if !iu.is_real() {
                return Ok(Info::unknown());
            }
            let minus = b.int(-1);
            let lo = if self.nonneg(&iu) { zero } else { minus };
            let hi = if self.nonpos(&iu) { zero } else { one };
            let lo = if self.positive(&iu) { one } else { lo };
            let hi = if self.negative(&iu) { minus } else { hi };
            Info {
                kind: Kind::Integer,
                range: Range { lo: Some(Bound { c: lo, strict: false }), hi: Some(Bound { c: hi, strict: false }), nonzero: iu.range.nonzero },
            }
        } else if s == hd.log {
            // log(u) for a positive u has the sign of u - 1
            let iu = self.info_at(cx, u, d)?;
            if !self.positive(&iu) {
                return Ok(Info::unknown());
            }
            let v = self.info_at(cx, b.sub(u, one), d)?;
            Info::real(self.sign_range(&v))
        } else if s == hd.sin || s == hd.cos {
            let iu = self.info_at(cx, u, d)?;
            if !iu.is_real() {
                return Ok(Info::unknown());
            }
            Info::real(Range { lo: Some(Bound { c: b.int(-1), strict: false }), hi: Some(Bound { c: one, strict: false }), nonzero: false })
        } else if s == self.s.cosh {
            let iu = self.info_at(cx, u, d)?;
            if !iu.is_real() {
                return Ok(Info::unknown());
            }
            Info::real(Range { lo: Some(Bound { c: one, strict: false }), hi: None, nonzero: true })
        } else if s == self.s.sinh || s == self.s.tanh || s == self.s.atan {
            // odd and increasing: the sign of the argument
            let iu = self.info_at(cx, u, d)?;
            if !iu.is_real() {
                return Ok(Info::unknown());
            }
            Info::real(self.sign_range(&iu))
        } else if s == self.s.floor || s == self.s.ceiling || s == self.s.round {
            let iu = self.info_at(cx, u, d)?;
            if !iu.is_real() {
                return Ok(Info::unknown());
            }
            Info { kind: Kind::Integer, range: Range::default() }
        } else {
            Info::unknown()
        })
    }

    /// A number: its own bounds.
    fn point(&self, c: T<'h>, kind: Kind) -> Info<'h> {
        let at = Some(Bound { c, strict: false });
        Info { kind, range: Range { lo: at, hi: at, nonzero: self.sgn(c).is_some_and(|o| o != Ordering::Equal) } }
    }

    /// The range that only records the sign of `i` (bounds at 0).
    fn sign_range(&self, i: &Info<'h>) -> Range<'h> {
        let z = self.b.int(0);
        let mut r = Range { nonzero: i.range.nonzero, ..Range::default() };
        if self.positive(i) {
            r.lo = Some(Bound { c: z, strict: true });
        } else if self.nonneg(i) {
            r.lo = Some(Bound { c: z, strict: false });
        }
        if self.negative(i) {
            r.hi = Some(Bound { c: z, strict: true });
        } else if self.nonpos(i) {
            r.hi = Some(Bound { c: z, strict: false });
        }
        r
    }

    /// The sum of two values.
    fn sum(&self, a: &Info<'h>, c: &Info<'h>) -> Info<'h> {
        let kind = a.kind.max(c.kind);
        if kind > Kind::Real {
            return Info::unknown();
        }
        let add = |x: Option<Bound<'h>>, y: Option<Bound<'h>>| {
            let (x, y) = (x?, y?);
            Some(Bound { c: self.b.add(&[x.c, y.c]), strict: x.strict || y.strict })
        };
        let mut r = Range { lo: add(a.range.lo, c.range.lo), hi: add(a.range.hi, c.range.hi), nonzero: false };
        // the bound arithmetic may leave the reals (a machine overflow): keep only real numbers
        r.lo = r.lo.filter(|x| self.sgn(x.c).is_some());
        r.hi = r.hi.filter(|x| self.sgn(x.c).is_some());
        let mut i = Info { kind, range: r };
        if self.positive(&i) || self.negative(&i) {
            i.range.nonzero = true;
        }
        i
    }

    /// A product: a numeric coefficient scales the range of the rest; the rest has the product of the signs.
    fn product(&mut self, cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>], d: u32) -> Res<Info<'h>> {
        let b = self.b;
        let (coef, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| b.is_number(a));
        let mut kind = Kind::Integer;
        let mut neg = false;
        let mut strict = true;
        let mut nonzero = true;
        let mut signed = true;
        let mut single: Option<Info<'h>> = None;
        for &f in &rest {
            let i = self.info_at(cx, f, d)?;
            kind = kind.max(i.kind);
            nonzero &= i.range.nonzero;
            if self.nonneg(&i) {
                strict &= self.positive(&i);
            } else if self.nonpos(&i) {
                strict &= self.negative(&i);
                neg = !neg;
            } else {
                signed = false;
            }
            if rest.len() == 1 {
                single = Some(i);
            }
        }
        if kind > Kind::Real {
            return Ok(Info::unknown());
        }
        let z = b.int(0);
        let base = match single {
            Some(i) => i,
            None => {
                let mut r = Range { nonzero, ..Range::default() };
                if signed {
                    let at = Some(Bound { c: z, strict });
                    if neg {
                        r.hi = at;
                    } else {
                        r.lo = at;
                    }
                }
                Info { kind, range: r }
            }
        };
        let Some(&k) = coef.first() else { return Ok(base) };
        let kk = self.number_kind(k);
        if kk > Kind::Real || coef.len() > 1 {
            return Ok(Info::unknown());
        }
        let kind = base.kind.max(kk);
        let scale = |x: Bound<'h>| Bound { c: b.mul(&[k, x.c]), strict: x.strict };
        let r = &base.range;
        let range = match self.sgn(k) {
            Some(Ordering::Greater) => Range { lo: r.lo.map(scale), hi: r.hi.map(scale), nonzero: r.nonzero },
            Some(Ordering::Less) => Range { lo: r.hi.map(scale), hi: r.lo.map(scale), nonzero: r.nonzero },
            Some(Ordering::Equal) => return Ok(self.point(z, Kind::Integer)),
            None => Range::default(),
        };
        Ok(Info { kind, range })
    }

    /// The kind of a real number term.
    fn number_kind(&self, c: T<'h>) -> Kind {
        match self.b.heap().view(c) {
            View::Int(_) | View::BigInt { .. } => Kind::Integer,
            View::Rational { .. } => Kind::Rational,
            View::Real(_) | View::Ball { .. } => Kind::Real,
            _ => Kind::Unknown,
        }
    }

    /// `exp(u)`: positive for a real `u` (at least 1 for a nonnegative one).
    fn exp(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>, d: u32) -> Res<Info<'h>> {
        let iu = self.info_at(cx, u, d)?;
        if !iu.is_real() {
            return Ok(Info::unknown());
        }
        let b = self.b;
        let lo = if self.positive(&iu) {
            Bound { c: b.int(1), strict: true }
        } else if self.nonneg(&iu) {
            Bound { c: b.int(1), strict: false }
        } else {
            Bound { c: b.int(0), strict: true }
        };
        let hi = if self.negative(&iu) {
            Some(Bound { c: b.int(1), strict: true })
        } else if self.nonpos(&iu) {
            Some(Bound { c: b.int(1), strict: false })
        } else {
            None
        };
        Ok(Info::real(Range { lo: Some(lo), hi, nonzero: true }))
    }

    /// `base^ex`.
    fn power(&mut self, cx: &mut NativeCx<'_, '_, 'h>, base: T<'h>, ex: T<'h>, d: u32) -> Res<Info<'h>> {
        let ib = self.info_at(cx, base, d)?;
        let ie = self.info_at(cx, ex, d)?;
        let z = self.b.int(0);
        let at = |strict| Some(Bound { c: z, strict });
        if ie.kind == Kind::Integer && ib.is_real() {
            let kind = if ib.kind == Kind::Integer && self.nonneg(&ie) {
                Kind::Integer
            } else if ib.kind <= Kind::Rational {
                Kind::Rational
            } else {
                Kind::Real
            };
            // an even exponent gives a nonnegative value, an odd one keeps the sign of the base
            let parity = self.parity(cx, ex, d)?;
            let nonzero = ib.range.nonzero || self.positive(&ib) || self.negative(&ib);
            let mut r = Range { nonzero, ..Range::default() };
            match parity {
                Some(true) => r.lo = at(nonzero),
                Some(false) => {
                    if self.positive(&ib) {
                        r.lo = at(true);
                    } else if self.negative(&ib) {
                        r.hi = at(true);
                    } else if self.nonneg(&ib) {
                        r.lo = at(false);
                    } else if self.nonpos(&ib) {
                        r.hi = at(false);
                    }
                }
                None => {
                    if self.positive(&ib) {
                        r.lo = at(true);
                    } else if self.nonneg(&ib) {
                        r.lo = at(false);
                    }
                }
            }
            return Ok(Info { kind, range: r });
        }
        if ie.is_real() && self.positive(&ib) {
            return Ok(Info::real(Range { lo: at(true), hi: None, nonzero: true }));
        }
        if self.positive(&ie) && self.nonneg(&ib) {
            return Ok(Info::real(Range { lo: at(false), hi: None, nonzero: ib.range.nonzero }));
        }
        Ok(Info::unknown())
    }

    /// Whether the integer `n` is even (`Some(true)`), odd (`Some(false)`), or not known.
    pub fn parity(&mut self, cx: &mut NativeCx<'_, '_, 'h>, n: T<'h>, d: u32) -> Res<Option<bool>> {
        let b = self.b;
        let half = b.mul(&[b.rat(1, 2), n]);
        if self.info_at(cx, half, d + 1)?.kind == Kind::Integer {
            return Ok(Some(true));
        }
        let half_odd = b.mul(&[b.rat(1, 2), b.sub(n, b.int(1))]);
        if self.info_at(cx, half_odd, d + 1)?.kind == Kind::Integer {
            return Ok(Some(false));
        }
        Ok(None)
    }
}

/// Raise the lower bound of `r` to `x` if that is higher.
fn meet_lo<'h>(f: &Facts<'_, 'h>, r: &mut Range<'h>, x: Bound<'h>) {
    r.lo = Some(match r.lo {
        None => x,
        Some(old) => match f.cmp(x.c, old.c) {
            Some(Ordering::Greater) => x,
            Some(Ordering::Equal) => Bound { c: old.c, strict: old.strict || x.strict },
            _ => old,
        },
    });
}

/// Lower the upper bound of `r` to `x` if that is lower.
fn meet_hi<'h>(f: &Facts<'_, 'h>, r: &mut Range<'h>, x: Bound<'h>) {
    r.hi = Some(match r.hi {
        None => x,
        Some(old) => match f.cmp(x.c, old.c) {
            Some(Ordering::Less) => x,
            Some(Ordering::Equal) => Bound { c: old.c, strict: old.strict || x.strict },
            _ => old,
        },
    });
}

