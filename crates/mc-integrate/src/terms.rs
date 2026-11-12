//! Small helpers on terms: calls, freeness, numbers, polynomial coefficients in one variable.
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num};
use mc_core::Heap;
use mc_poly::convert::{Conv, Heads};
use mc_poly::ratfun::{self, RatFun};
use mc_poly::work::{Halt, Meter, Work, WorkResult};
use rustc_hash::FxHashMap;

/// The name and arguments of a call with a symbol head.
pub fn fun<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    let View::App { head, args } = h.view(t) else { return None };
    let View::Sym(s) = h.view(head) else { return None };
    Some((h.sym_name(s), args))
}

/// Is `t` free of `x` (no subterm equal to `x`)?
pub fn is_free<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return false;
        }
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    true
}

/// Does `t` contain a call of the symbol named `name`?
pub fn has_call<'h>(h: &'h Heap, t: T<'h>, name: &str) -> bool {
    let s = h.sym_term(h.sym(name));
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if let View::App { head, args } = h.view(u) {
            if head == s {
                return true;
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The number of distinct subterms of `t` (a size measure), counting stops at `cap`.
pub fn size<'h>(h: &'h Heap, t: T<'h>, cap: usize) -> usize {
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if seen.len() >= cap {
            break;
        }
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    seen.len()
}

/// The exact rational value of a number term.
pub fn rat_of<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> Option<Rat> {
    ar.num(t).and_then(|n| n.as_rat().cloned())
}

/// The machine integer of a term.
pub fn int_of<'h>(h: &'h Heap, t: T<'h>) -> Option<i64> {
    match h.view(t) {
        View::Int(v) => Some(v),
        _ => None,
    }
}

/// The real and imaginary parts of an exact complex number.
pub fn complex_parts<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> Option<(Rat, Rat)> {
    match ar.num(t)? {
        Num::Complex(mc_arith::Real::Exact(re), mc_arith::Real::Exact(im)) => Some((re, im)),
        _ => None,
    }
}

/// A budget that stops only with `mc-arith`'s stop flag, and after a fixed amount of work.
pub struct Small(pub u64);

impl Work for Small {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if mc_arith::num::stopped() || self.0 < units {
            return Err(Halt::Stopped);
        }
        self.0 -= units;
        Ok(())
    }
}

/// `a / b` when it is a rational number (the two read as rational functions of their kernels), else `None`.
pub fn ratio<'h>(ar: &Arith<'h, Heap>, a: T<'h>, b: T<'h>) -> Option<Rat> {
    let h = ar.store();
    let (a, b) = (decomplex(ar, a), decomplex(ar, b));
    let hd = Heads::new(h);
    let conv = Conv::rational(ar, &hd);
    let mut w = Small(64);
    let mut m = Meter::new(&mut w);
    let (ring, _) = conv.ring(&[a, b], &[], &mut m).ok()?;
    let ra = ratfun::read(&conv, a, &ring, &mut m).ok()?.ok()?;
    let rb = ratfun::read(&conv, b, &ring, &mut m).ok()?.ok()?;
    if rb.num.is_zero() {
        return None;
    }
    let q = ra.mul(&rb.inv(1, &mut m).ok()?.ok()?, &mut m).ok()?;
    let n = q.num.as_constant().or_else(|| q.num.is_zero().then_some(mc_poly::Z::ZERO))?;
    let d = q.den.as_constant()?;
    Some(mc_poly::qpoly::rat_of(&n, &d))
}

/// `t` with every exact complex number `p + q i` written `p + q I` for a symbol `I` (so that `i x` and `-i x` are
/// multiples of one kernel).
pub fn decomplex<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> T<'h> {
    let h = ar.store();
    let i = h.sym_term(h.sym("integrate`I"));
    let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    fn go<'h>(ar: &Arith<'h, Heap>, t: T<'h>, i: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
        if let Some(&r) = memo.get(&t) {
            return r;
        }
        let h = ar.store();
        let r = if let Some((re, im)) = complex_parts(ar, t) {
            ar.add(&[ar.number(&Num::rat(re)), ar.mul(&[ar.number(&Num::rat(im)), i])])
        } else if let View::App { head, args } = h.view(t) {
            let na: Vec<T<'h>> = args.iter().map(|&a| go(ar, a, i, memo)).collect();
            if na.iter().zip(args.iter()).all(|(p, q)| p == q) {
                t
            } else if Some(head) == Some(h.sym_term(h.sym("add"))) {
                ar.add(&na)
            } else if head == h.sym_term(h.sym("mul")) {
                ar.mul(&na)
            } else if head == h.sym_term(h.sym("pow")) && na.len() == 2 {
                ar.pow(na[0], na[1])
            } else {
                h.app(head, &na)
            }
        } else {
            t
        };
        memo.insert(t, r);
        r
    }
    go(ar, t, i, &mut memo)
}

/// The coefficients (constant term first) of `t` as a polynomial in `x` with coefficients free of `x`, written as
/// terms; `None` when `t` is not such a polynomial.
pub fn coeffs_in<'h>(ar: &Arith<'h, Heap>, t: T<'h>, x: T<'h>) -> Option<Vec<T<'h>>> {
    let h = ar.store();
    let hd = Heads::new(h);
    let conv = Conv::rational(ar, &hd);
    let mut w = Small(256);
    let mut m = Meter::new(&mut w);
    let (ring, _) = conv.ring(&[t], &[x], &mut m).ok()?;
    let vx = ring.var(x)?;
    // every other kernel must be free of x
    if ring.kernels.iter().any(|&k| k != x && !is_free(h, k, x)) {
        return None;
    }
    let r: RatFun = ratfun::read(&conv, t, &ring, &mut m).ok()?.ok()?;
    if r.den.degree(vx).unwrap_or(0) > 0 {
        return None;
    }
    let deg = r.num.degree(vx).unwrap_or(0) as usize;
    let mut out = vec![ar.int(0); deg + 1];
    let den = ratfun::to_term(&conv, &RatFun { num: r.den.clone(), den: mc_poly::Poly::constant(r.den.nvars(), mc_poly::Z::ONE) }, &ring, &mut m).ok()?;
    for (k, c) in r.num.coeffs_in(vx) {
        let ct = conv.int_poly_term(&c, &ring, &mut m).ok()?;
        out[k as usize] = ar.div(ct, den);
    }
    Some(out)
}

/// Does `t` contain an exact complex number with a nonzero imaginary part?
pub fn has_complex<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> bool {
    let h = ar.store();
    let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if seen.insert(u, ()).is_some() {
            continue;
        }
        if complex_parts(ar, u).is_some_and(|(_, im)| !im.is_zero()) {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
    false
}
