//! The roots of a polynomial in one variable whose coefficients are polynomials in other kernels (parameters,
//! radicals of earlier solutions, inexact numbers).
//!
//! The polynomial is factored over the integers (in all its kernels, decision 3B.2); each irreducible factor of
//! degree d in the variable gives d roots, repeated by the factor's multiplicity:
//!
//! - a factor in `x^g` only (g the gcd of its exponents) whose reduced degree d/g is at most 4 is solved for
//!   `y = x^g`, and every y gives the g roots `y^(1/g) (-1)^(2j/g)`;
//! - degree 1: `-a0/a1`; degree 2: the quadratic formula;
//! - degree 3: Cardano's formula on the depressed cubic `t^3 + p t + q`. With rational coefficients the forms are
//!   real-friendly: one real root as a sum of real cube roots and a conjugate pair, or (casus irreducibilis, three
//!   real roots) the trigonometric form `2 sqrt(-p/3) cos(acos(3q/(2p) sqrt(-3/p))/3 - 2 pi k/3)`; otherwise the
//!   principal cube root `C` of `-q/2 + sqrt(q^2/4 + p^3/27)` and `w^k C - p/(3 w^k C)` with `w = (-1 + i sqrt 3)/2`
//!   (correct for every branch, decision 3B.3);
//! - degree 4: Ferrari's method on the depressed quartic `y^4 + p y^2 + q y + r`: biquadratic when q = 0, else a
//!   nonzero root m of the resolvent cubic `8m^3 + 8p m^2 + (2p^2 - 8r) m - q^2` (a rational root when there is one,
//!   else the cubic's first root, which is real and positive for rational coefficients), `s = sqrt(2m)` and the
//!   roots `(+-s +- sqrt(-(2p + 2m +- 2q/s)))/2`;
//! - degree 5 and more: the root objects `root(x => f(x), k)`, k = 1..d (decision 3B.4).
use crate::cx::Env;
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::Num;
use mc_poly::convert::Ring;
use mc_poly::factor::factor;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};

/// The roots of `p` in ring variable `v`, with multiplicities; `None` when p is identically zero (every value is
/// a root). Factors free of `v` (conditions on the parameters) are ignored: the generic case.
pub(crate) fn poly_roots<'h>(
    env: &Env<'h>,
    p: &Poly,
    ring: &Ring<'h>,
    v: usize,
    m: &mut Meter,
) -> WorkResult<Option<Vec<T<'h>>>> {
    if p.is_zero() {
        return Ok(None);
    }
    if p.degree(v).unwrap_or(0) == 0 {
        return Ok(Some(vec![]));
    }
    let fs = factor(p, false, m)?;
    let mut out = vec![];
    for (f, e) in &fs.factors {
        if f.degree(v).unwrap_or(0) == 0 {
            continue;
        }
        let rs = irreducible_roots(env, f, ring, v, m)?;
        for _ in 0..*e {
            out.extend_from_slice(&rs);
        }
    }
    Ok(Some(out))
}

/// The distinct roots of a factor (one per root, in the order of the formulas).
pub(crate) fn irreducible_roots<'h>(
    env: &Env<'h>,
    f: &Poly,
    ring: &Ring<'h>,
    v: usize,
    m: &mut Meter,
) -> WorkResult<Vec<T<'h>>> {
    let d = f.degree(v).unwrap_or(0) as usize;
    let cs = f.coeffs_in(v);
    let g = cs.iter().fold(0u32, |g, (k, _)| gcd_u32(g, *k)) as usize;
    let c = env.conv();
    // the coefficients of x^(g k) as terms, k = 0 .. d/g
    let mut coefs = vec![env.int(0); d / g + 1];
    for (k, p) in &cs {
        coefs[*k as usize / g] = c.int_poly_term(p, ring, m)?;
    }
    let a = Alg { env };
    if g > 1 && d / g == 1 {
        // a pure power c1 x^g + c0: the coefficients factored (cheap, and the root reads as a ratio of products)
        for (k, p) in &cs {
            coefs[*k as usize / g] = factored_term(env, p, ring, m)?;
        }
    }
    if g > 1 && d / g <= 4 {
        let ys = formula_roots(env, &coefs, m)?;
        let mut out = Vec::with_capacity(d);
        for y in ys {
            let base = a.pow(y, 1, g as i64);
            for j in 0..g as i64 {
                out.push(a.mul(&[base, a.root_of_unity(j, g as i64)]));
            }
        }
        return Ok(out);
    }
    if d <= 4 {
        return formula_roots(env, &coefs, m);
    }
    root_objects(env, f, ring, v, m)
}

/// p as a product of its factors over the integers.
fn factored_term<'h>(env: &Env<'h>, p: &Poly, ring: &Ring<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
    let c = env.conv();
    let fs = factor(p, false, m)?;
    let mut parts = vec![env.ar.integer(&fs.unit.to_int())];
    for (f, e) in &fs.factors {
        let t = c.int_poly_term(f, ring, m)?;
        parts.push(if *e == 1 { t } else { env.ar.pow(t, env.int(*e as i64)) });
    }
    Ok(env.ar.mul(&parts))
}

/// The root objects `root(x => f(x), k)`, k = 1 .. degree.
pub(crate) fn root_objects<'h>(
    env: &Env<'h>,
    f: &Poly,
    ring: &Ring<'h>,
    v: usize,
    m: &mut Meter,
) -> WorkResult<Vec<T<'h>>> {
    let h = env.h();
    let x = ring.kernels[v];
    let body = env.conv().int_poly_term(f, ring, m)?;
    // the bound variable: the variable itself when it is a symbol, else the symbol x
    let (var, body) = if matches!(h.view(x), View::Sym(_)) {
        (x, body)
    } else {
        let s = env.sym("x");
        (s, env.subst(body, &[(x, s)], m)?)
    };
    let lam = env.call("fn", &[env.list(&[var]), body]);
    let d = f.degree(v).unwrap_or(0) as i64;
    Ok((1..=d).map(|k| env.call("root", &[lam, env.int(k)])).collect())
}

fn gcd_u32(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd_u32(b, a % b)
    }
}

/// The roots of `sum coefs[k] y^k` (degree 1 to 4, leading coefficient nonzero) by the formulas.
pub(crate) fn formula_roots<'h>(env: &Env<'h>, coefs: &[T<'h>], m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
    let a = Alg { env };
    match coefs.len() - 1 {
        1 => Ok(vec![a.neg(a.div(coefs[0], coefs[1]))]),
        2 => Ok(a.quadratic(coefs[2], coefs[1], coefs[0])),
        3 => a.cubic(coefs[3], coefs[2], coefs[1], coefs[0], m),
        4 => a.quartic(coefs, m),
        _ => unreachable!("formulas for degrees 1 to 4"),
    }
}

/// Canonical arithmetic on terms for the formulas.
pub(crate) struct Alg<'e, 'h> {
    pub env: &'e Env<'h>,
}

impl<'h> Alg<'_, 'h> {
    pub fn int(&self, v: i64) -> T<'h> {
        self.env.int(v)
    }
    pub fn rat(&self, n: i64, d: i64) -> T<'h> {
        self.env.ar.div(self.int(n), self.int(d))
    }
    pub fn add(&self, xs: &[T<'h>]) -> T<'h> {
        self.env.ar.add(xs)
    }
    pub fn mul(&self, xs: &[T<'h>]) -> T<'h> {
        self.env.ar.mul(xs)
    }
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.env.ar.sub(a, b)
    }
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.env.ar.div(a, b)
    }
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.env.ar.neg(a)
    }
    /// a^(n/d).
    pub fn pow(&self, a: T<'h>, n: i64, d: i64) -> T<'h> {
        self.env.ar.pow(a, self.rat(n, d))
    }
    pub fn sqrt(&self, a: T<'h>) -> T<'h> {
        self.pow(a, 1, 2)
    }
    /// The imaginary unit.
    pub fn i(&self) -> T<'h> {
        self.env.ar.number(&Num::i())
    }
    /// exp(2 pi i j / n) as the unit (-1)^(2j/n) (1 for j = 0).
    pub fn root_of_unity(&self, j: i64, n: i64) -> T<'h> {
        if j == 0 {
            return self.int(1);
        }
        self.pow(self.int(-1), 2 * j, n)
    }
    /// w = (-1 + i sqrt(3))/2 (k = 1) and its square (k = 2).
    fn omega(&self, k: i64) -> T<'h> {
        let s = self.mul(&[self.i(), self.sqrt(self.int(3))]);
        let s = if k == 1 { s } else { self.neg(s) };
        self.mul(&[self.rat(1, 2), self.add(&[self.int(-1), s])])
    }
    fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.env.call(name, args)
    }
    /// The rational value of a term, if it is one.
    fn rational(&self, t: T<'h>) -> Option<Rat> {
        self.env.ar.num(t).and_then(|x| x.as_rat().cloned())
    }
    /// The sign of a rational.
    fn sign(&self, q: &Rat) -> i32 {
        if q.is_zero() {
            0
        } else if q.num.cmp_int(&mc_arith::num::Int::from_i64(0)).is_lt() {
            -1
        } else {
            1
        }
    }
    /// The real cube root of a real closed term (sign decided numerically), else the principal one.
    fn real_cbrt(&self, t: T<'h>) -> T<'h> {
        let e = self.env;
        let neg = match e.ar.num(t) {
            Some(Num::Real(x)) => x.is_negative(),
            Some(_) => false,
            None => mc_numeric::numeval::sign(e.h(), &e.ar, &e.nm, t) == Some(std::cmp::Ordering::Less),
        };
        if neg {
            self.neg(self.pow(self.neg(t), 1, 3))
        } else {
            self.pow(t, 1, 3)
        }
    }

    /// The two roots of a y^2 + b y + c, minus sign first.
    pub fn quadratic(&self, a: T<'h>, b: T<'h>, c: T<'h>) -> Vec<T<'h>> {
        let disc = self.sub(self.mul(&[b, b]), self.mul(&[self.int(4), a, c]));
        let r = self.sqrt(disc);
        let two_a = self.mul(&[self.int(2), a]);
        let nb = self.neg(b);
        vec![self.div(self.sub(nb, r), two_a), self.div(self.add(&[nb, r]), two_a)]
    }

    /// The three roots of a y^3 + b y^2 + c y + d (Cardano).
    pub fn cubic(&self, a: T<'h>, b: T<'h>, c: T<'h>, d: T<'h>, m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
        let e = self.env;
        // y = t - b/(3a); t^3 + p t + q = 0
        let shift = self.neg(self.div(b, self.mul(&[self.int(3), a])));
        let p = e.together(
            self.div(self.sub(self.mul(&[self.int(3), a, c]), self.mul(&[b, b])), self.mul(&[self.int(3), a, a])),
            m,
        )?;
        let q = e.together(
            self.div(
                self.add(&[
                    self.mul(&[self.int(2), b, b, b]),
                    self.mul(&[self.int(-9), a, b, c]),
                    self.mul(&[self.int(27), a, a, d]),
                ]),
                self.mul(&[self.int(27), a, a, a]),
            ),
            m,
        )?;
        let ts = self.depressed_cubic(p, q, m)?;
        Ok(ts.into_iter().map(|t| self.add(&[t, shift])).collect())
    }

    /// The roots of t^3 + p t + q; the first is real (and the largest when all are) for rational p, q.
    fn depressed_cubic(&self, p: T<'h>, q: T<'h>, m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
        let e = self.env;
        if e.is_zero(p, m)? {
            let c = self.real_cbrt(self.neg(q));
            return Ok(vec![c, self.mul(&[self.omega(1), c]), self.mul(&[self.omega(2), c])]);
        }
        // D = q^2/4 + p^3/27
        let disc = e.together(self.add(&[self.mul(&[self.rat(1, 4), q, q]), self.mul(&[self.rat(1, 27), p, p, p])]), m)?;
        if let (Some(_), Some(dr)) = (self.rational(p), self.rational(disc)) {
            if self.sign(&dr) < 0 {
                // three real roots: 2 sqrt(-p/3) cos(acos(3q/(2p) sqrt(-3/p))/3 - 2 pi k/3)
                let amp = self.mul(&[self.int(2), self.sqrt(self.mul(&[self.rat(-1, 3), p]))]);
                let arg = self.mul(&[self.rat(3, 2), q, self.pow(p, -1, 1), self.sqrt(self.mul(&[self.int(-3), self.pow(p, -1, 1)]))]);
                let phi = self.mul(&[self.rat(1, 3), self.call("acos", &[arg])]);
                let pi = e.sym("pi");
                return Ok((0..3)
                    .map(|k| {
                        let ang = self.add(&[phi, self.mul(&[self.rat(-2 * k, 3), pi])]);
                        self.mul(&[amp, self.call("cos", &[ang])])
                    })
                    .collect());
            }
            // one real root: A + B with real cube roots A, B of -q/2 +- sqrt(D); the others -(A+B)/2 +- i sqrt(3)(A-B)/2
            let sd = self.sqrt(disc);
            let hq = self.mul(&[self.rat(-1, 2), q]);
            let ca = self.real_cbrt(self.add(&[hq, sd]));
            let cb = self.real_cbrt(self.sub(hq, sd));
            let s = self.add(&[ca, cb]);
            let dlt = self.mul(&[self.rat(1, 2), self.i(), self.sqrt(self.int(3)), self.sub(ca, cb)]);
            let mid = self.mul(&[self.rat(-1, 2), s]);
            return Ok(vec![s, self.add(&[mid, dlt]), self.sub(mid, dlt)]);
        }
        // general: C the principal cube root, roots w^k C - p/(3 w^k C)
        let c = self.pow(self.add(&[self.mul(&[self.rat(-1, 2), q]), self.sqrt(disc)]), 1, 3);
        let mut out = vec![];
        for k in 0..3 {
            let wc = if k == 0 { c } else { self.mul(&[self.omega(k), c]) };
            out.push(self.sub(wc, self.div(p, self.mul(&[self.int(3), wc]))));
        }
        Ok(out)
    }

    /// The four roots of sum coefs[k] y^k (Ferrari).
    pub fn quartic(&self, coefs: &[T<'h>], m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
        let e = self.env;
        let (a, b, c, d, f) = (coefs[4], coefs[3], coefs[2], coefs[1], coefs[0]);
        let shift = self.neg(self.div(b, self.mul(&[self.int(4), a])));
        let p = e.together(
            self.div(
                self.add(&[self.mul(&[self.int(8), a, c]), self.mul(&[self.int(-3), b, b])]),
                self.mul(&[self.int(8), a, a]),
            ),
            m,
        )?;
        let q = e.together(
            self.div(
                self.add(&[
                    self.mul(&[b, b, b]),
                    self.mul(&[self.int(-4), a, b, c]),
                    self.mul(&[self.int(8), a, a, d]),
                ]),
                self.mul(&[self.int(8), a, a, a]),
            ),
            m,
        )?;
        let r = e.together(
            self.div(
                self.add(&[
                    self.mul(&[self.int(-3), b, b, b, b]),
                    self.mul(&[self.int(256), a, a, a, f]),
                    self.mul(&[self.int(-64), a, a, b, d]),
                    self.mul(&[self.int(16), a, b, b, c]),
                ]),
                self.mul(&[self.int(256), a, a, a, a]),
            ),
            m,
        )?;
        let ys = if e.is_zero(q, m)? {
            // biquadratic: y^2 = z for the roots z of z^2 + p z + r
            let mut out = vec![];
            for z in self.quadratic(self.int(1), p, r) {
                let s = self.sqrt(z);
                out.push(self.neg(s));
                out.push(s);
            }
            out
        } else {
            let mm = self.resolvent_root(p, q, r, m)?;
            let s = self.sqrt(self.mul(&[self.int(2), mm]));
            let base = self.add(&[self.mul(&[self.int(2), p]), self.mul(&[self.int(2), mm])]);
            let tq = self.div(self.mul(&[self.int(2), q]), s);
            let r1 = self.sqrt(self.neg(self.add(&[base, tq])));
            let r2 = self.sqrt(self.neg(self.sub(base, tq)));
            let h = self.rat(1, 2);
            vec![
                self.mul(&[h, self.sub(self.neg(s), r2)]),
                self.mul(&[h, self.add(&[self.neg(s), r2])]),
                self.mul(&[h, self.sub(s, r1)]),
                self.mul(&[h, self.add(&[s, r1])]),
            ]
        };
        Ok(ys.into_iter().map(|y| self.add(&[y, shift])).collect())
    }

    /// A nonzero root of 8m^3 + 8p m^2 + (2p^2 - 8r) m - q^2 (q nonzero): a rational one when it exists (the
    /// largest), else the cubic formula's first root.
    fn resolvent_root(&self, p: T<'h>, q: T<'h>, r: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        let e = self.env;
        let c2 = self.mul(&[self.int(8), p]);
        let c1 = self.add(&[self.mul(&[self.int(2), p, p]), self.mul(&[self.int(-8), r])]);
        let c0 = self.neg(self.mul(&[q, q]));
        if let (Some(r2), Some(r1), Some(r0)) = (self.rational(c2), self.rational(c1), self.rational(c0)) {
            if let Some(root) = rational_root(&[r0, r1, r2, Rat::from_i64(8)]) {
                return Ok(e.ar.number(&Num::rat(root)));
            }
        }
        let ms = self.cubic(self.int(8), c2, c1, c0, m)?;
        Ok(ms[0])
    }
}

/// The largest rational root of a polynomial with rational coefficients (index = degree), by the rational root
/// theorem on the integer polynomial (candidates +-p/q, p | a0, q | an; small coefficients only).
fn rational_root(c: &[Rat]) -> Option<Rat> {
    // clear denominators
    let mut den = mc_arith::num::Int::from_i64(1);
    for x in c {
        let d = mc_arith::num::Int::from_nat(x.den.clone());
        den = lcm_int(&den, &d);
    }
    let ints: Vec<i128> = c
        .iter()
        .map(|x| {
            let v = Rat::from_int(den.clone()).mul(x);
            v.num.to_i64().map(|y| y as i128)
        })
        .collect::<Option<Vec<_>>>()?;
    let (a0, an) = (ints[0], *ints.last()?);
    if a0 == 0 {
        return Some(Rat::from_i64(0));
    }
    let divisors = |v: i128| -> Option<Vec<i128>> {
        let v = v.abs();
        if v > 1_000_000_000_000 {
            return None;
        }
        let mut ds = vec![];
        let mut k = 1i128;
        while k * k <= v {
            if v % k == 0 {
                ds.push(k);
                ds.push(v / k);
            }
            k += 1;
            if k > 2_000_000 {
                return None;
            }
        }
        Some(ds)
    };
    let (ps, qs) = (divisors(a0)?, divisors(an)?);
    let mut best: Option<Rat> = None;
    for &pp in &ps {
        for &qq in &qs {
            for s in [-1i128, 1] {
                let x = Rat::new(mc_arith::num::Int::from_i64((s * pp) as i64), mc_arith::num::Int::from_i64(qq as i64));
                // Horner with rationals
                let mut acc = Rat::from_i64(0);
                for k in (0..c.len()).rev() {
                    acc = acc.mul(&x).add(&c[k]);
                }
                if acc.is_zero() && best.as_ref().is_none_or(|b| x.cmp_value(b).is_gt()) {
                    best = Some(x);
                }
            }
        }
    }
    best
}

fn lcm_int(a: &mc_arith::num::Int, b: &mc_arith::num::Int) -> mc_arith::num::Int {
    let (za, zb) = (Z::from_int(a.clone()), Z::from_int(b.clone()));
    za.mul(&zb).div_exact(&za.gcd(&zb)).abs().to_int()
}
