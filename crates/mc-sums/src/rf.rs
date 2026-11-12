//! Rational functions over the integers in the variables of one ring (`num / den` in lowest terms), with the
//! operations the summation algorithms need: shifts `x -> x + h`, composition, coefficients in one variable, and
//! linear systems over the field of rational functions in the other variables.
use mc_arith::num::Rat;
use mc_poly::gcd::gcd;
use mc_poly::qpoly::rat_parts;
use mc_poly::work::{exact, Meter, WorkResult};
use mc_poly::{Poly, Z};

/// `num / den`, coprime, `den` with a positive leading coefficient.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rf {
    /// the numerator
    pub num: Poly,
    /// the denominator
    pub den: Poly,
}

/// `a / b` for an exact division.
pub fn div(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    if b.as_constant() == Some(Z::ONE) {
        return Ok(a.clone());
    }
    exact(a.div_exact(b, m)?, "a divisor divides")
}

/// The integer `v` as a polynomial.
pub fn zc(nv: usize, v: i64) -> Poly {
    Poly::constant(nv, Z::from_i128(v as i128))
}

/// `p(x_v + h)` for a polynomial `h`.
pub fn compose(p: &Poly, v: usize, q: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    let cs = p.coeffs_in(v);
    if cs.is_empty() {
        return Ok(p.clone());
    }
    // Horner by decreasing exponent
    let mut acc = Poly::zero(p.nvars());
    let mut last = cs[0].0;
    for (e, c) in cs {
        if last > e {
            acc = acc.mul(&q.pow((last - e) as u64, m)?, m)?;
        }
        acc = acc.add(&c);
        last = e;
    }
    if last > 0 {
        acc = acc.mul(&q.pow(last as u64, m)?, m)?;
    }
    Ok(acc)
}

/// `p(x_v + h)` for an integer `h`.
pub fn shift(p: &Poly, v: usize, h: i64, m: &mut Meter) -> WorkResult<Poly> {
    if h == 0 || p.degree(v).unwrap_or(0) == 0 {
        return Ok(p.clone());
    }
    let nv = p.nvars();
    let q = Poly::var(nv, v).add(&zc(nv, h));
    compose(p, v, &q, m)
}

/// The degree of `p` in `v` (`None` for zero).
pub fn deg(p: &Poly, v: usize) -> Option<u32> {
    if p.is_zero() {
        None
    } else {
        Some(p.degree(v).unwrap_or(0))
    }
}

/// The coefficient of `x_v^e` (free of `x_v`).
pub fn coeff(p: &Poly, v: usize, e: u32) -> Poly {
    p.coeffs_in(v).into_iter().find(|c| c.0 == e).map(|c| c.1).unwrap_or_else(|| Poly::zero(p.nvars()))
}

/// The leading coefficient in `v`.
pub fn lcoeff(p: &Poly, v: usize) -> Poly {
    match deg(p, v) {
        Some(d) => coeff(p, v, d),
        None => Poly::zero(p.nvars()),
    }
}

/// Does `p` contain `x_v`?
pub fn has(p: &Poly, v: usize) -> bool {
    p.degree(v).unwrap_or(0) > 0
}

impl Rf {
    /// The constant 0.
    pub fn zero(nv: usize) -> Rf {
        Rf { num: Poly::zero(nv), den: zc(nv, 1) }
    }
    /// The constant 1.
    pub fn one(nv: usize) -> Rf {
        Rf { num: zc(nv, 1), den: zc(nv, 1) }
    }
    /// An integer.
    pub fn int(nv: usize, v: i64) -> Rf {
        Rf { num: zc(nv, v), den: zc(nv, 1) }
    }
    /// A rational number.
    pub fn rat(nv: usize, q: &Rat) -> Rf {
        let (n, d) = rat_parts(q);
        Rf { num: Poly::constant(nv, n), den: Poly::constant(nv, d) }
    }
    /// A polynomial.
    pub fn poly(p: Poly) -> Rf {
        let nv = p.nvars();
        Rf { num: p, den: zc(nv, 1) }
    }
    /// The number of variables.
    pub fn nv(&self) -> usize {
        self.num.nvars()
    }
    /// Is it zero?
    pub fn is_zero(&self) -> bool {
        self.num.is_zero()
    }
    /// Is it the constant 1?
    pub fn is_one(&self) -> bool {
        self.num == self.den
    }
    /// The rational number it is, if it is constant.
    pub fn as_rat(&self) -> Option<(Z, Z)> {
        Some((self.num.as_constant()?, self.den.as_constant()?))
    }
    /// The integer it is, if any.
    pub fn as_int(&self) -> Option<i64> {
        let (n, d) = self.as_rat()?;
        if d.is_one() {
            n.as_i64()
        } else {
            None
        }
    }
    /// Does it contain `x_v`?
    pub fn has(&self, v: usize) -> bool {
        has(&self.num, v) || has(&self.den, v)
    }
    /// `num / den` in lowest terms.
    pub fn new(num: Poly, den: Poly, m: &mut Meter) -> WorkResult<Rf> {
        let nv = num.nvars();
        if num.is_zero() {
            return Ok(Rf::zero(nv));
        }
        let g = gcd(&num, &den, m)?;
        let (mut n, mut d) = if g.as_constant() == Some(Z::ONE) { (num, den) } else { (div(&num, &g, m)?, div(&den, &g, m)?) };
        if d.lc().is_neg() {
            n = n.neg();
            d = d.neg();
        }
        Ok(Rf { num: n, den: d })
    }
    /// The sum.
    pub fn add(&self, o: &Rf, m: &mut Meter) -> WorkResult<Rf> {
        if self.is_zero() {
            return Ok(o.clone());
        }
        if o.is_zero() {
            return Ok(self.clone());
        }
        if self.den == o.den {
            return Rf::new(self.num.add(&o.num), self.den.clone(), m);
        }
        let g = gcd(&self.den, &o.den, m)?;
        let (a, b) = (div(&o.den, &g, m)?, div(&self.den, &g, m)?);
        let num = self.num.mul(&a, m)?.add(&o.num.mul(&b, m)?);
        let den = self.den.mul(&a, m)?;
        Rf::new(num, den, m)
    }
    /// The negation.
    pub fn neg(&self) -> Rf {
        Rf { num: self.num.neg(), den: self.den.clone() }
    }
    /// The difference.
    pub fn sub(&self, o: &Rf, m: &mut Meter) -> WorkResult<Rf> {
        self.add(&o.neg(), m)
    }
    /// The product.
    pub fn mul(&self, o: &Rf, m: &mut Meter) -> WorkResult<Rf> {
        if self.is_zero() || o.is_zero() {
            return Ok(Rf::zero(self.nv()));
        }
        let g1 = gcd(&self.num, &o.den, m)?;
        let g2 = gcd(&o.num, &self.den, m)?;
        let num = div(&self.num, &g1, m)?.mul(&div(&o.num, &g2, m)?, m)?;
        let den = div(&self.den, &g2, m)?.mul(&div(&o.den, &g1, m)?, m)?;
        let (num, den) = if den.lc().is_neg() { (num.neg(), den.neg()) } else { (num, den) };
        Ok(Rf { num, den })
    }
    /// The product with a polynomial.
    pub fn mul_poly(&self, p: &Poly, m: &mut Meter) -> WorkResult<Rf> {
        self.mul(&Rf::poly(p.clone()), m)
    }
    /// The inverse (`None` for zero).
    pub fn inv(&self) -> Option<Rf> {
        if self.is_zero() {
            return None;
        }
        let (n, d) = (self.den.clone(), self.num.clone());
        Some(if d.lc().is_neg() { Rf { num: n.neg(), den: d.neg() } } else { Rf { num: n, den: d } })
    }
    /// The quotient (`None` when dividing by zero).
    pub fn div(&self, o: &Rf, m: &mut Meter) -> WorkResult<Option<Rf>> {
        match o.inv() {
            Some(i) => Ok(Some(self.mul(&i, m)?)),
            None => Ok(None),
        }
    }
    /// An integer power (negative powers of zero give `None`).
    pub fn powi(&self, e: i64, m: &mut Meter) -> WorkResult<Option<Rf>> {
        let b = if e < 0 {
            match self.inv() {
                Some(b) => b,
                None => return Ok(None),
            }
        } else {
            self.clone()
        };
        let e = e.unsigned_abs();
        Ok(Some(Rf { num: b.num.pow(e, m)?, den: b.den.pow(e, m)? }))
    }
    /// `f(x_v + h)`.
    pub fn shift(&self, v: usize, h: i64, m: &mut Meter) -> WorkResult<Rf> {
        if h == 0 {
            return Ok(self.clone());
        }
        // a shift keeps lowest terms and the sign of the leading coefficient
        Ok(Rf { num: shift(&self.num, v, h, m)?, den: shift(&self.den, v, h, m)? })
    }
    /// `f` with `x_v` replaced by the polynomial `q` (`None` when the denominator vanishes).
    pub fn compose(&self, v: usize, q: &Poly, m: &mut Meter) -> WorkResult<Option<Rf>> {
        let d = compose(&self.den, v, q, m)?;
        if d.is_zero() {
            return Ok(None);
        }
        Ok(Some(Rf::new(compose(&self.num, v, q, m)?, d, m)?))
    }
    /// `f` at `x_v = a` (`None` when the denominator vanishes there).
    pub fn at(&self, v: usize, a: i64, m: &mut Meter) -> WorkResult<Option<Rf>> {
        let z = Z::from_i128(a as i128);
        let d = self.den.eval_var(v, &z);
        if d.is_zero() {
            return Ok(None);
        }
        Ok(Some(Rf::new(self.num.eval_var(v, &z), d, m)?))
    }
}

/// The least common multiple of polynomials.
pub fn lcm(a: &Poly, b: &Poly, m: &mut Meter) -> WorkResult<Poly> {
    if a.is_zero() || b.is_zero() {
        return Ok(Poly::zero(a.nvars()));
    }
    let g = gcd(a, b, m)?;
    let l = div(a, &g, m)?.mul(b, m)?;
    Ok(if l.lc().is_neg() { l.neg() } else { l })
}

/// A basis of the solutions of the homogeneous system `rows . x = 0` over the rational functions (each row has
/// `nx` entries). Gaussian elimination with the smallest pivot; at most `max` unknowns.
pub fn nullspace(rows: &[Vec<Rf>], nx: usize, m: &mut Meter) -> WorkResult<Vec<Vec<Rf>>> {
    let nv = rows.first().and_then(|r| r.first()).map(|r| r.nv()).unwrap_or(1);
    let mut a: Vec<Vec<Rf>> = rows.iter().filter(|r| r.iter().any(|x| !x.is_zero())).cloned().collect();
    let mut pivots: Vec<(usize, usize)> = vec![];
    let mut r = 0;
    for c in 0..nx {
        m.step(1)?;
        // the simplest nonzero pivot in column c
        let mut best: Option<(usize, u64)> = None;
        for (i, row) in a.iter().enumerate().skip(r) {
            if !row[c].is_zero() {
                let size = (row[c].num.len() + row[c].den.len()) as u64 * 64 + row[c].num.max_bits() + row[c].den.max_bits();
                if best.is_none_or(|b| size < b.1) {
                    best = Some((i, size));
                }
            }
        }
        let Some((p, _)) = best else { continue };
        a.swap(r, p);
        // normalise the pivot row
        let inv = a[r][c].inv().expect("nonzero pivot");
        let row: Vec<Rf> = a[r].iter().map(|x| x.mul(&inv, m)).collect::<WorkResult<_>>()?;
        a[r] = row;
        for i in 0..a.len() {
            if i != r && !a[i][c].is_zero() {
                let f = a[i][c].clone();
                let pivot_row = a[r].clone();
                for (j, pj) in pivot_row.iter().enumerate().take(nx) {
                    if !pj.is_zero() {
                        let t = pj.mul(&f, m)?;
                        a[i][j] = a[i][j].sub(&t, m)?;
                    }
                }
            }
        }
        pivots.push((r, c));
        r += 1;
        if r == a.len() {
            break;
        }
    }
    let pivot_cols: Vec<usize> = pivots.iter().map(|p| p.1).collect();
    let mut basis = vec![];
    for free in (0..nx).filter(|c| !pivot_cols.contains(c)) {
        let mut x = vec![Rf::zero(nv); nx];
        x[free] = Rf::one(nv);
        for &(pr, pc) in &pivots {
            x[pc] = a[pr][free].neg();
        }
        basis.push(x);
    }
    Ok(basis)
}

/// One solution of `rows . x = rhs` (`None` when inconsistent); free unknowns are 0.
pub fn solve(rows: &[Vec<Rf>], rhs: &[Rf], nx: usize, m: &mut Meter) -> WorkResult<Option<Vec<Rf>>> {
    // the homogeneous system in nx + 1 unknowns, the last one fixed to -1
    let aug: Vec<Vec<Rf>> = rows
        .iter()
        .zip(rhs)
        .map(|(r, b)| {
            let mut v = r.clone();
            v.push(b.clone());
            v
        })
        .collect();
    let basis = nullspace(&aug, nx + 1, m)?;
    // a basis vector with a nonzero last entry, scaled to -1; the others with zero last entries are dropped
    for v in basis {
        if !v[nx].is_zero() {
            let s = v[nx].inv().expect("nonzero").neg();
            let mut out = Vec::with_capacity(nx);
            for x in &v[..nx] {
                out.push(x.mul(&s, m)?);
            }
            return Ok(Some(out));
        }
    }
    Ok(None)
}
