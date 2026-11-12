//! Residues over a real radical field (item F1; `mc-algebra` decision F1.4): when the roots of the
//! denominator's factor `q` that carry the residues of an irreducible residue factor are the roots of a binomial
//! `x^n - c` with `n` in 3, 4, 6, `q` splits over the real field `K = Q(ρ)`, `ρ` the real n-th root of `c`, into
//! linear and quadratic factors (`x^n - c = prod Φ_k(x/ρ) ρ^φ(k)` over the divisors `k` of `n`, `φ(k) <= 2`).
//! The part `∫ A/q` (`A` the partial-fraction numerator of `q`) is then integrated over `K` by the generic
//! Rothstein–Trager code with real `log`/`atan` output, `K` being an exact field: signs of its elements are decided
//! by `mc-algebra`'s embedding (certified enclosures), square roots by factoring `y^2 - a` over `K`. So
//! `∫ 1/(x^3 - 2)` is `log` and `atan` terms in `2^(1/3)` rather than a sum over roots.
use super::logpart::{self, Base};
use super::upoly::{Fld, Stop, Ticker, P, R};
use super::Budget;
use mc_calculus::term::Build;
use mc_algebra::algnum::AlgNum;
use mc_algebra::field::{Elem, NumberField};
use mc_algebra::{embed, kpoly, qpoly};
use mc_api::T;
use mc_arith::num::{Int, Rat};
use mc_arith::Num;
use mc_poly::work::{Meter, Unlimited};

/// A real number field with its generator's embedding.
pub struct RealField {
    k: NumberField,
}

fn unlimited<X>(f: impl FnOnce(&mut Meter) -> mc_poly::work::WorkResult<X>) -> Option<X> {
    let mut w = Unlimited;
    let mut m = Meter::new(&mut w);
    f(&mut m).ok()
}

impl Fld for RealField {
    type E = Elem;
    fn zero(&self) -> Elem {
        vec![]
    }
    fn of(&self, c: &Rat) -> Elem {
        self.k.of(c)
    }
    fn is_zero(&self, a: &Elem) -> bool {
        a.is_empty()
    }
    fn add(&self, a: &Elem, b: &Elem) -> Elem {
        self.k.add(a, b)
    }
    fn neg(&self, a: &Elem) -> Elem {
        self.k.neg(a)
    }
    fn mul(&self, a: &Elem, b: &Elem) -> Elem {
        self.k.mul(a, b)
    }
    fn inv(&self, a: &Elem) -> Option<Elem> {
        self.k.inv(a)
    }
    fn sign(&self, a: &Elem) -> Option<i32> {
        let x = AlgNum { k: self.k.clone(), a: a.clone() };
        unlimited(|m| x.sign(m)).flatten()
    }
    fn sqrt(&self, a: &Elem) -> Option<Elem> {
        if a.is_empty() {
            return Some(vec![]);
        }
        // a linear factor of y^2 - a over K
        let p: kpoly::KPoly = vec![self.k.neg(a), vec![], self.k.one()];
        let fs = unlimited(|m| kpoly::factor(&self.k, &p, m)).flatten()?;
        let lin = fs.factors.iter().find(|(f, _)| f.len() == 2)?;
        Some(self.k.neg(&lin.0[0]))
    }
    fn as_rat(&self, a: &Elem) -> Option<Rat> {
        self.k.as_rat(a)
    }
}

impl Base for RealField {
    fn factor(&self, p: &[Elem], tk: &mut dyn Ticker) -> R<Vec<P<Elem>>> {
        let mut budget = Budget::new(tk);
        let r = {
            let mut m = Meter::new(&mut budget);
            kpoly::factor(&self.k, p, &mut m)
        };
        match r {
            Ok(Some(f)) => Ok(f.factors.into_iter().map(|(g, _)| g).collect()),
            Ok(None) => Err(Stop::GiveUp),
            Err(_) => Err(budget.stop()),
        }
    }
}

/// The terms of `∫ A/q` over the real radical field of `q` appended to `out` when `q` (monic, irreducible over Q)
/// is a binomial `x^n - c`, `n` in 3, 4, 6, with a real root; `Ok(false)` when it is not (nothing appended).
pub fn integrate<'h>(
    b: &Build<'h>,
    x: T<'h>,
    q: &[Rat],
    a: &[Rat],
    d: &[Rat],
    tk: &mut dyn Ticker,
    out: &mut Vec<T<'h>>,
) -> R<bool> {
    let n = q.len() - 1;
    if !matches!(n, 3 | 4 | 6) || q[1..n].iter().any(|c| !c.is_zero()) {
        return Ok(false);
    }
    let c = q[0].neg();
    // the real n-th root: positive for c > 0; for c < 0 (n odd) minus the positive root of -c
    let neg = c.num.neg;
    if neg && n.is_multiple_of(2) {
        return Ok(false);
    }
    let Some(rs) = unlimited(|m| embed::roots(q, m)).flatten() else { return Ok(false) };
    let real: Vec<_> = rs.iter().filter(|e| e.real).collect();
    let Some(e) = real.iter().rev().find(|e| neg || !e.re.num.neg).or(real.first()) else { return Ok(false) };
    let k = NumberField::new(q.to_vec(), Some((*e).clone()));
    let field = RealField { k };
    // the generator's term
    let absc = if neg { c.neg() } else { c.clone() };
    let rad = b.pow(b.ar.number(&Num::rat(absc)), b.ar.number(&Num::rat(Rat::new(Int::from_i64(1), Int::from_i64(n as i64)))));
    let gen = if neg { b.neg(rad) } else { rad };
    let et = |el: &Elem| -> T<'h> {
        let parts: Vec<T<'h>> = el
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.is_zero())
            .map(|(i, c)| b.mul(&[b.ar.number(&Num::rat(c.clone())), b.powi(gen, i as i64)]))
            .collect();
        b.add(&parts)
    };
    // the partial-fraction numerator of q: A = a (d/q)^-1 mod q
    let Some(rest) = unlimited(|m| qpoly::divrem(d, q, m)).map(|r| r.0) else { return Ok(false) };
    let kq = NumberField::new(q.to_vec(), None);
    let Some(inv) = kq.inv(&kq.reduce(&rest)) else { return Ok(false) };
    let num = kq.mul(&kq.reduce(a), &inv);
    let ak: P<Elem> = kpoly::lift(&num);
    let qk: P<Elem> = kpoly::lift(q);
    let mut part = vec![];
    match logpart::integrate(b, x, &field, &et, &ak, &qk, tk, &mut part) {
        Ok(()) => {
            out.extend(part);
            Ok(true)
        }
        Err(Stop::Abort(a)) => Err(Stop::Abort(a)),
        Err(Stop::GiveUp) => Ok(false),
    }
}
