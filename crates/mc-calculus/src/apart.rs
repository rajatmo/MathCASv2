//! Partial fractions: `apart` (package 2C, item 1). OWNED BY THE APART WORK PACKAGE.
//!
//! `apart(e, x)` reads `e` as a quotient `N/D` of integer polynomials in its kernels (`mc-poly`'s rational reader,
//! in lowest terms). Every kernel other than `x` must be free of `x` (symbols such as `a`, calls such as `sin(y)`):
//! those are the parameters, and the coefficient field is K = Q(parameters). Then:
//!
//! 1. `D` is factored over Z (`mc_poly::factor`); the factors free of `x` form the scalar `C`, the others
//!    `D = prod p_i^k_i` (irreducible over Q in x, pairwise coprime);
//! 2. `N/C = Q*D + R` by division in K[x] (pseudo-division, then the scalar factor divided out);
//! 3. for each `q_i = p_i^k_i` with cofactor `M_i = D/q_i`: `A_i = R * M_i^-1 mod q_i` (the inverse by the extended
//!    Euclidean algorithm in K[x], remainders made monic), so that `R/D = sum A_i/q_i`;
//! 4. `A_i` in base `p_i`: `A_i = a_0 + a_1 p_i + ... + a_(k-1) p_i^(k-1)` with `deg a_j < deg p_i`, giving the terms
//!    `a_j / p_i^(k-j)`.
//!
//! The result is `Q + sum a_ij / p_i^j`, each numerator of degree below its factor's degree (constants over linear
//! factors, linear numerators over irreducible quadratics, ...). Elements of K[x] are kept as `num/den` with integer
//! polynomials, `den` free of `x`, reduced by gcds after every operation (decision 2C.A1).
//!
//! Declines (`Ok(None)`, the call stays as it is): a kernel that contains `x` non-rationally (`sin(x)`, `x^(1/2)`),
//! an inexact or complex number in `e` (decision 2C.A2), division by zero, `apart(e)` when `e` has not exactly one
//! variable (a symbol). An input free of `x`, or whose denominator in lowest terms is free of `x`, is returned as it
//! is (or as its expanded quotient when a denominator containing `x` cancelled). Work is charged to the evaluator's
//! fuel; an interrupt aborts, the memory limit gives `failure(limit, memory)`.
use crate::integrate::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{Abort, NativeCx, Res};
use mc_poly::convert::{Conv, Heads, Ring};
use mc_poly::gcd::gcd;
use mc_poly::work::{exact, Halt, Meter, Work, WorkResult};
use mc_poly::{Poly, QPoly, Z};

/// The partial fraction decomposition of `e` in `x` (`x` = `None`: the only variable of `e`), else `None`
/// (the call stays as it is).
pub fn apart<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, e: T<'h>, x: Option<T<'h>>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut tick = Tick { cx, abort: None };
    let r = {
        let mut m = Meter::new(&mut tick);
        decompose(b, e, x, &mut m)
    };
    match r {
        Ok(t) => Ok(t),
        Err(Halt::Stopped) => match tick.abort {
            Some(a) => Err(a),
            None => Ok(Some(h.call(h.sym(names::FAILURE), &[sym(b, names::failure::INTERRUPTED)]))),
        },
        Err(Halt::Memory) => {
            let tags = [sym(b, names::failure::LIMIT), sym(b, names::failure::MEMORY)];
            Ok(Some(h.call(h.sym(names::FAILURE), &tags)))
        }
    }
}

fn sym<'h>(b: &Build<'h>, name: &str) -> T<'h> {
    let h = b.heap();
    h.sym_term(h.sym(name))
}

/// The evaluator's fuel and interrupt as a [`Work`] budget, also polling `mc-arith`'s stop flag.
struct Tick<'c, 'a, 'k, 'h> {
    cx: &'c mut NativeCx<'a, 'k, 'h>,
    abort: Option<Abort>,
}

impl Work for Tick<'_, '_, '_, '_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if mc_arith::num::stopped() {
            return Err(Halt::Stopped);
        }
        match self.cx.tick(units) {
            Ok(()) => Ok(()),
            Err(a) => {
                self.abort = Some(a);
                Err(Halt::Stopped)
            }
        }
    }
}

/// Does `e` contain a negative integer power whose base contains `x`?
fn has_x_denominator<'h>(b: &Build<'h>, e: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let mut stack = vec![e];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(t) = stack.pop() {
        if !seen.insert(t) {
            continue;
        }
        if let View::App { head, args } = h.view(t) {
            if b.sym_of(head) == Some(b.hd.pow) && args.len() == 2 {
                if let View::Int(n) = h.view(args[1]) {
                    if n < 0 && !is_free(b, args[0], x) {
                        return true;
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

fn decompose<'h>(b: &Build<'h>, e: T<'h>, x: Option<T<'h>>, m: &mut Meter) -> WorkResult<Option<T<'h>>> {
    let h = b.heap();
    let hd = Heads::new(h);
    let conv = Conv::rational(&b.ar, &hd);
    let (ring, _) = conv.ring(&[e], &[], m)?;
    // the variable
    let x = match x {
        Some(x) => x,
        None => match ring.kernels.as_slice() {
            [k] if matches!(h.view(*k), View::Sym(_)) => *k,
            _ => return Ok(None),
        },
    };
    if is_free(b, e, x) {
        return Ok(Some(e));
    }
    let Some(v) = ring.var(x) else { return Ok(None) };
    // every other kernel is a parameter: free of x and not a number
    for (i, &k) in ring.kernels.iter().enumerate() {
        if ring.is_numeric(&b.ar, i) || (i != v && !is_free(b, k, x)) {
            return Ok(None);
        }
    }
    let f = match mc_poly::ratfun::read(&conv, e, &ring, m)? {
        Ok(f) => f,
        Err(_) => return Ok(None),
    };
    let nv = ring.nvars();
    let k = K { v, nv };
    if f.den.degree(v).unwrap_or(0) == 0 {
        // nothing to decompose
        if !has_x_denominator(b, e, x) {
            return Ok(Some(e));
        }
        return Ok(Some(mc_poly::ratfun::to_term(&conv, &f, &ring, m)?));
    }
    // the denominator: x-free content C and the factors in x
    let fac = mc_poly::factor::factor(&f.den, false, m)?;
    let mut c = Poly::constant(nv, fac.unit.clone());
    let mut parts: Vec<(Poly, u32)> = vec![];
    for (p, mult) in fac.factors {
        if p.degree(v).unwrap_or(0) == 0 {
            c = c.mul(&p.pow(mult as u64, m)?, m)?;
        } else if k.lc(&p).lc().is_neg() {
            // a factor with a positive leading coefficient in x: `x - a`, not `a - x`
            if mult % 2 == 1 {
                c = c.neg();
            }
            parts.push((p.neg(), mult));
        } else {
            parts.push((p, mult));
        }
    }
    let mut d = Poly::constant(nv, Z::ONE);
    let mut qs = Vec::with_capacity(parts.len());
    for (p, mult) in &parts {
        let q = p.pow(*mult as u64, m)?;
        d = d.mul(&q, m)?;
        qs.push(q);
    }
    let n = k.reduce(f.num.clone(), c, m)?;
    let dk = k.poly(d.clone());
    let (quot, rem) = k.divrem(&n, &dk, m)?;
    let mut out: Vec<T<'h>> = vec![k.term(&conv, &ring, &quot, m)?];
    for ((p, mult), q) in parts.iter().zip(&qs) {
        let cof = exact(d.div_exact(q, m)?, "a factor divides the denominator")?;
        let qk = k.poly(q.clone());
        let Some(inv) = k.inverse(&k.poly(cof), &qk, m)? else { return Ok(None) };
        let mut a = k.divrem(&k.mul(&rem, &inv, m)?, &qk, m)?.1;
        let pk = k.poly(p.clone());
        let pt = conv.int_poly_term(p, &ring, m)?;
        for j in (1..=*mult).rev() {
            if a.num.is_zero() {
                break;
            }
            let (next, coef) = k.divrem(&a, &pk, m)?;
            if !coef.num.is_zero() {
                let ct = k.term(&conv, &ring, &coef, m)?;
                out.push(b.mul(&[ct, b.powi(pt, -(j as i64))]));
            }
            a = next;
        }
        if !a.num.is_zero() {
            // cannot happen when deg A < k deg p; stay safe on user input
            return Ok(None);
        }
    }
    if mc_arith::num::stopped() {
        return Err(Halt::Stopped);
    }
    Ok(Some(b.add(&out)))
}

/// An element `num/den` of K[x], K = Q(parameters): integer polynomials, `den` free of x and nonzero with a positive
/// leading coefficient, `gcd(num, den) = 1` (`den = 1` for zero).
#[derive(Clone, Debug)]
struct Kp {
    num: Poly,
    den: Poly,
}

/// Arithmetic in K[x]: `v` is the variable x of a ring of `nv` variables.
struct K {
    v: usize,
    nv: usize,
}

impl K {
    fn one(&self) -> Poly {
        Poly::constant(self.nv, Z::ONE)
    }
    fn poly(&self, p: Poly) -> Kp {
        Kp { num: p, den: self.one() }
    }
    fn zero(&self) -> Kp {
        self.poly(Poly::zero(self.nv))
    }
    fn deg(&self, a: &Kp) -> Option<u32> {
        a.num.degree(self.v)
    }
    /// `num/den` in lowest terms.
    fn reduce(&self, num: Poly, den: Poly, m: &mut Meter) -> WorkResult<Kp> {
        if num.is_zero() {
            return Ok(self.zero());
        }
        let g = gcd(&num, &den, m)?;
        let (mut n, mut d) = if g.as_constant() == Some(Z::ONE) {
            (num, den)
        } else {
            (exact(num.div_exact(&g, m)?, "a gcd divides")?, exact(den.div_exact(&g, m)?, "a gcd divides")?)
        };
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        Ok(Kp { num: n, den: d })
    }
    fn add(&self, a: &Kp, b: &Kp, m: &mut Meter) -> WorkResult<Kp> {
        if a.den == b.den {
            return self.reduce(a.num.add(&b.num), a.den.clone(), m);
        }
        let num = a.num.mul(&b.den, m)?.add(&b.num.mul(&a.den, m)?);
        self.reduce(num, a.den.mul(&b.den, m)?, m)
    }
    fn sub(&self, a: &Kp, b: &Kp, m: &mut Meter) -> WorkResult<Kp> {
        self.add(a, &Kp { num: b.num.neg(), den: b.den.clone() }, m)
    }
    fn mul(&self, a: &Kp, b: &Kp, m: &mut Meter) -> WorkResult<Kp> {
        self.reduce(a.num.mul(&b.num, m)?, a.den.mul(&b.den, m)?, m)
    }
    /// The leading coefficient in x (an integer polynomial free of x) of a nonzero polynomial.
    fn lc(&self, p: &Poly) -> Poly {
        p.coeffs_in(self.v).swap_remove(0).1
    }
    /// The inverse of the leading coefficient of a nonzero `a` (a scalar of K).
    fn lc_inv(&self, a: &Kp, m: &mut Meter) -> WorkResult<Kp> {
        self.reduce(a.den.clone(), self.lc(&a.num), m)
    }
    /// Quotient and remainder of `a` by a nonzero `b` in K[x] (pseudo-division of the numerators).
    fn divrem(&self, a: &Kp, b: &Kp, m: &mut Meter) -> WorkResult<(Kp, Kp)> {
        // invariant: every caller divides by a modulus or a factor, which are nonzero
        let db = self.deg(b).expect("division by a nonzero polynomial");
        let lb = self.lc(&b.num);
        let mut r = a.num.clone();
        let mut q = Poly::zero(self.nv);
        let mut scale = self.one();
        let mut ex = vec![0u32; self.nv];
        while let Some(dr) = r.degree(self.v).filter(|&dr| dr >= db) {
            m.step((r.len() + b.num.len() + q.len()) as u64)?;
            let lr = self.lc(&r);
            ex[self.v] = dr - db;
            let t = lr.mul_term(&ex, &Z::ONE);
            q = q.mul(&lb, m)?.add(&t);
            r = r.mul(&lb, m)?.sub(&t.mul(&b.num, m)?);
            scale = scale.mul(&lb, m)?;
        }
        // lb^s a.num = q b.num + r, so a = (q b.den / (lb^s a.den)) b + r / (lb^s a.den)
        let den = scale.mul(&a.den, m)?;
        let quot = self.reduce(q.mul(&b.den, m)?, den.clone(), m)?;
        let rem = self.reduce(r, den, m)?;
        Ok((quot, rem))
    }
    /// `s` with `s a = 1 mod md` and `deg s < deg md`, or `None` when `a` and `md` are not coprime.
    fn inverse(&self, a: &Kp, md: &Kp, m: &mut Meter) -> WorkResult<Option<Kp>> {
        let (mut r0, mut r1) = (md.clone(), self.divrem(a, md, m)?.1);
        let (mut s0, mut s1) = (self.zero(), self.poly(self.one()));
        while !r1.num.is_zero() {
            m.step(1)?;
            let (q, r) = self.divrem(&r0, &r1, m)?;
            let s = self.sub(&s0, &self.mul(&q, &s1, m)?, m)?;
            r0 = r1;
            s0 = s1;
            r1 = r;
            s1 = s;
            if !r1.num.is_zero() {
                // keep the remainders monic (the invariant s_i a = r_i mod md survives scaling both)
                let c = self.lc_inv(&r1, m)?;
                r1 = self.mul(&r1, &c, m)?;
                s1 = self.mul(&s1, &c, m)?;
            }
        }
        if self.deg(&r0) != Some(0) {
            return Ok(None);
        }
        let c = self.lc_inv(&r0, m)?;
        let s = self.mul(&s0, &c, m)?;
        Ok(Some(self.divrem(&s, md, m)?.1))
    }
    /// The canonical term of `a`.
    fn term<'h>(&self, conv: &Conv<'_, 'h, mc_core::Heap>, ring: &Ring<'h>, a: &Kp, m: &mut Meter) -> WorkResult<T<'h>> {
        if let Some(d) = a.den.as_constant() {
            return conv.to_term(&QPoly::new(a.num.clone(), d), ring, m);
        }
        let n = conv.int_poly_term(&a.num, ring, m)?;
        let d = conv.int_poly_term(&a.den, ring, m)?;
        let ar = conv.ar;
        Ok(ar.mul(&[n, ar.pow(d, ar.store().int(-1))]))
    }
}
