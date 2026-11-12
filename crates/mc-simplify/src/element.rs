//! `element(x, domain)`: membership in the domains of [`crate::facts::DOMAINS`], decided where it can be without
//! assumptions (decision A.7).
//!
//! - numbers by their kind and sign (a machine real is in the reals only; a complex number with a nonzero
//!   imaginary part in the complexes only);
//! - closed numeric expressions (`pi`, `sin(1)`, `e^(im sin(7))`) by refinement: realness and sign through
//!   `mc-numeric`, integrality by the distance to the nearest integer, and rationality for the forms known to be
//!   irrational ([`known_irrational`]: `pi`, `e`, `e^q`, `log(q)`, `sin(q)`, radicals `b^(p/r)` left by the
//!   canonical arithmetic, a rational multiple or shift of one of them);
//! - `x` a list (`element([x, y], d)`) or alternatives (`element(x | y, d)`): `false` when one element is not in the
//!   domain, `true` when all are, otherwise the undecided ones remain;
//! - for a signed domain the undecided ones become the unsigned domain and sign conditions:
//!   `element(x, positive_integers)` is `element(x, integers) and x > 0`.
//!
//! Inside `assuming`, `refine` and `simplify` decide `element` from the facts as well (`refine.rs`).
use crate::facts::{Kind, Sgn, Syms, DOMAINS};
use crate::term::B;
use mc_api::{Store, Sym, View, T};
use mc_arith::number::Cmp3;
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{Native, NativeCx, Reply, Res};
use mc_numeric::numeval;
use std::cmp::Ordering;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The native of `element`.
pub struct ElementNative {
    pub(crate) syms: Arc<Syms>,
    pub(crate) stop: Arc<AtomicBool>,
}

/// A domain: its kind and sign condition.
#[derive(Clone, Copy, Debug)]
pub struct Domain {
    /// the kind (`Unknown` for the complexes)
    pub kind: Kind,
    /// the sign condition
    pub sgn: Option<Sgn>,
}

/// The domain named by `d`, if it is one of [`DOMAINS`].
pub fn domain<'h>(b: &B<'h>, d: T<'h>) -> Option<Domain> {
    let h = b.heap();
    let s = b.sym_of(d)?;
    let name = h.sym_name(s);
    DOMAINS.iter().find(|e| e.0 == name).map(|&(_, kind, sgn)| Domain { kind, sgn })
}

/// The name of the unsigned domain of a kind.
fn base_name(k: Kind) -> &'static str {
    match k {
        Kind::Integer => "integers",
        Kind::Rational => "rationals",
        Kind::Real => "reals",
        Kind::Unknown => "complexes",
    }
}

/// The elements of `x`: the items of a list or of alternatives, or `x` itself; and the container head.
pub fn items<'h>(b: &B<'h>, x: T<'h>) -> (Vec<T<'h>>, Option<Sym>) {
    let h = b.heap();
    match b.call_of(x) {
        Some((s, xs)) if s == b.hd.list || h.sym_name(s) == "alternatives" => (xs.to_vec(), Some(s)),
        _ => (vec![x], None),
    }
}

/// Is `t` closed: numbers, the numeric constants and calls of them only?
pub fn closed<'h>(h: &'h Heap, nm: &numeval::Names, t: T<'h>) -> bool {
    let mut stack = vec![t];
    let mut seen = 0usize;
    while let Some(u) = stack.pop() {
        seen += 1;
        if seen > 4096 {
            return false;
        }
        match h.view(u) {
            View::Sym(s) if nm.constant(s).is_none() => return false,
            View::App { head, args } => {
                if !matches!(h.view(head), View::Sym(_)) {
                    return false;
                }
                stack.extend(args.iter().copied());
            }
            View::Str(_) => return false,
            _ => {}
        }
    }
    true
}

/// Is the closed real term `t` known to be irrational? (Sound, incomplete: `false` means "not known".)
pub fn known_irrational<'h>(b: &B<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let rational = |u: T<'h>| matches!(h.view(u), View::Int(_) | View::BigInt { .. } | View::Rational { .. });
    let nonzero_rational = |u: T<'h>| rational(u) && u != h.int(0);
    if let View::Sym(s) = h.view(t) {
        let n = h.sym_name(s);
        return n == "pi" || n == "e";
    }
    let Some((s, args)) = b.call_of(t) else { return false };
    let name = h.sym_name(s);
    if s == b.hd.pow {
        let [base, ex] = args else { return false };
        // e^q, pi^k: transcendental for q, k nonzero rationals
        if let Some(bs) = b.sym_of(*base) {
            let bn = h.sym_name(bs);
            if (bn == "e" || bn == "pi") && nonzero_rational(*ex) {
                return true;
            }
        }
        // a radical the canonical arithmetic left as a power: a rational base, a non-integer rational exponent
        if rational(*base) && matches!(h.view(*ex), View::Rational { .. }) {
            return b.ar.compare(*base, h.int(0)).is_some_and(|c| c == Cmp3::Greater);
        }
        return false;
    }
    if s == b.hd.mul {
        // a nonzero rational times one irrational factor
        let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| rational(a));
        return num.iter().all(|&c| c != h.int(0)) && rest.len() == 1 && known_irrational(b, rest[0]);
    }
    if s == b.hd.add {
        // a rational plus one irrational term
        let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| rational(a));
        return num.len() <= 1 && rest.len() == 1 && known_irrational(b, rest[0]);
    }
    if let [u] = args {
        // Lindemann: exp, log, sin, cos, tan of nonzero algebraic (here: rational) arguments are transcendental
        let one = h.int(1);
        return match name.as_str() {
            "exp" | "sin" | "tan" | "sinh" | "tanh" | "atan" => nonzero_rational(*u),
            "cos" | "cosh" => nonzero_rational(*u),
            "log" => rational(*u) && *u != one && b.ar.compare(*u, h.int(0)).is_some_and(|c| c == Cmp3::Greater),
            _ => false,
        };
    }
    false
}

/// Is the closed real term `t` an integer? By the distance to the nearest integer of its machine value.
fn closed_integer<'h>(cx: &mut NativeCx<'_, '_, 'h>, ar: &Arith<'h, Heap>, nm: &numeval::Names, t: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    let Some(x) = ar.num(v).and_then(|n| n.as_real().map(|r| r.to_f64())) else { return Ok(None) };
    if !x.is_finite() || x.abs() > 1e15 {
        return Ok(None);
    }
    let k = h.int(x.round() as i64);
    let d = ar.sub(t, k);
    Ok(match numeval::sign(h, ar, nm, d) {
        Some(Ordering::Equal) => Some(true),
        Some(_) => Some(false),
        None => None,
    })
}

/// Whether `x` is in the domain: decided for numbers and closed numeric expressions, `None` otherwise.
pub fn decide<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, nm: &numeval::Names, x: T<'h>, dom: Domain) -> Res<Option<bool>> {
    let h = b.heap();
    let ar = &b.ar;
    // a number: its kind
    let kind = match h.view(x) {
        View::Int(_) | View::BigInt { .. } => Some(Kind::Integer),
        View::Rational { .. } => Some(Kind::Rational),
        View::Real(_) | View::Ball { .. } => Some(Kind::Real),
        _ => None,
    };
    if b.complex_parts(x).is_some() {
        return Ok(Some(dom.kind == Kind::Unknown));
    }
    let (kind, real) = match kind {
        Some(k) => (Some(k), true),
        None => {
            if !closed(h, nm, x) {
                return Ok(None);
            }
            match numeval::is_real(h, ar, nm, x) {
                Some(true) => (None, true),
                Some(false) => return Ok(Some(dom.kind == Kind::Unknown)),
                // a closed expression without a value (a singular point) is in no domain; one not decided stays
                None => return Ok(None),
            }
        }
    };
    if dom.kind == Kind::Unknown {
        return Ok(Some(true));
    }
    debug_assert!(real);
    // the sign condition
    if let Some(sg) = dom.sgn {
        let s = match kind {
            Some(_) => ar.compare(x, h.int(0)).and_then(|c| match c {
                Cmp3::Less => Some(Ordering::Less),
                Cmp3::Equal => Some(Ordering::Equal),
                Cmp3::Greater => Some(Ordering::Greater),
                _ => None,
            }),
            None => numeval::sign(h, ar, nm, x),
        };
        let Some(s) = s else { return Ok(None) };
        let ok = match sg {
            Sgn::Pos => s == Ordering::Greater,
            Sgn::Neg => s == Ordering::Less,
            Sgn::NonNeg => s != Ordering::Less,
            Sgn::NonPos => s != Ordering::Greater,
        };
        if !ok {
            return Ok(Some(false));
        }
    }
    Ok(match (kind, dom.kind) {
        (_, Kind::Real) => Some(true),
        (Some(k), dk) => Some(k <= dk),
        (None, Kind::Rational) => {
            if known_irrational(b, x) {
                Some(false)
            } else {
                // an integer value is rational; anything else is not known
                closed_integer(cx, ar, nm, x)?.and_then(|i| i.then_some(true))
            }
        }
        (None, Kind::Integer) => {
            if known_irrational(b, x) {
                Some(false)
            } else {
                closed_integer(cx, ar, nm, x)?
            }
        }
        (None, Kind::Unknown) => Some(true),
    })
}

/// The sign condition `x op 0` of a signed domain.
fn sign_condition<'h>(b: &B<'h>, x: T<'h>, sg: Sgn) -> T<'h> {
    let name = match sg {
        Sgn::Pos => "greater",
        Sgn::Neg => "less",
        Sgn::NonNeg => "greater_equal",
        Sgn::NonPos => "less_equal",
    };
    b.call_named(name, &[x, b.int(0)])
}

impl Native for ElementNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x, d] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        let b = B::new(h, &self.stop);
        let Some(dom) = domain(&b, *d) else { return Ok(Reply::Decline) };
        let (xs, container) = items(&b, *x);
        if xs.is_empty() {
            return Ok(Reply::Decline);
        }
        let mut open = Vec::new();
        for &u in &xs {
            cx.tick(1)?;
            match decide(cx, &b, &self.syms.num, u, dom)? {
                Some(false) => return Ok(Reply::Done(cx.bool(false))),
                Some(true) => {}
                None => open.push(u),
            }
        }
        if open.is_empty() {
            return Ok(Reply::Done(cx.bool(true)));
        }
        let rebuilt = |items: &[T<'h>]| match (container, items) {
            (_, [one]) => *one,
            (Some(s), _) => b.call(s, items),
            (None, _) => items[0],
        };
        if let Some(sg) = dom.sgn {
            // the unsigned domain and one sign condition per element
            let mut conj = vec![b.call(h.sym("element"), &[rebuilt(&open), h.sym_term(h.sym(base_name(dom.kind)))])];
            conj.extend(open.iter().map(|&u| sign_condition(&b, u, sg)));
            return Ok(Reply::Again(b.call_named("and", &conj)));
        }
        if open.len() < xs.len() {
            return Ok(Reply::Again(b.call(h.sym("element"), &[rebuilt(&open), *d])));
        }
        Ok(Reply::Decline)
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// The native of `not_element(x, domain)` (item F3): `true` when `element(x, domain)` is `false`, `false` when
/// it is `true`, otherwise unevaluated.
pub struct NotElementNative;

impl Native for NotElementNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x, d] = args else { return Ok(Reply::Decline) };
        let h = cx.heap();
        let e = cx.eval(h.call(h.sym("element"), &[*x, *d]))?;
        Ok(if cx.is_true(e) {
            Reply::Done(cx.bool(false))
        } else if cx.is_false(e) {
            Reply::Done(cx.bool(true))
        } else {
            Reply::Decline
        })
    }

    fn reads_term(&self) -> bool {
        false
    }
}
