//! The rational part: square-free factorisation (Yun) and Hermite reduction (Bronstein, *Symbolic Integration I*,
//! section 2.2, the quadratic version `HermiteReduce`).
use super::upoly::{
    constant, deg, derivative, diophantine, exact, gcd, mul, neg, pow, q, qq, scale, sub, Fld, Ticker, P, R,
};

/// The square-free factorisation of a monic `f`: `[f1, f2, ..., fm]` (monic, pairwise coprime, some possibly 1)
/// with `f = f1 f2^2 ... fm^m` (Yun).
pub fn square_free<K: Fld>(k: &K, f: &[K::E], tk: &mut dyn Ticker) -> R<Vec<P<K::E>>> {
    let mut out = vec![];
    if deg(f).unwrap_or(0) == 0 {
        return Ok(out);
    }
    let fp = derivative(k, f);
    let c = gcd(k, f, &fp, tk)?;
    let mut w = exact(k, f, &c, tk)?;
    let y = exact(k, &fp, &c, tk)?;
    let mut z = sub(k, &y, &derivative(k, &w));
    while deg(&w).unwrap_or(0) > 0 {
        tk.step(1)?;
        let g = gcd(k, &w, &z, tk)?;
        w = exact(k, &w, &g, tk)?;
        let y = exact(k, &z, &g, tk)?;
        z = sub(k, &y, &derivative(k, &w));
        out.push(g);
    }
    Ok(out)
}

/// One term `b / v^j` of the rational part.
pub struct Frac<E> {
    /// the numerator (deg b < deg v)
    pub b: P<E>,
    /// the square-free factor of the denominator (monic)
    pub v: P<E>,
    /// its power
    pub j: u32,
}

/// Hermite reduction of `a / d` (d monic, deg a < deg d): the rational part `Σ b / v^j` and the remainder
/// `(a*, d*)` with `d*` square-free, such that `∫ a/d = Σ b / v^j + ∫ a*/d*`.
#[allow(clippy::type_complexity)]
pub fn reduce<K: Fld>(
    k: &K,
    a: &[K::E],
    d: &[K::E],
    tk: &mut dyn Ticker,
) -> R<(Vec<Frac<K::E>>, P<K::E>, P<K::E>)> {
    let parts = square_free(k, d, tk)?;
    let mut a = a.to_vec();
    let mut d = d.to_vec();
    let mut out = vec![];
    for (i, v) in parts.iter().enumerate().skip(1) {
        let i = i + 1;
        if deg(v).unwrap_or(0) == 0 {
            continue;
        }
        let u = exact(k, &d, &pow(k, v, i as u32, tk)?, tk)?;
        let uvp = mul(k, &u, &derivative(k, v), tk)?;
        for j in (1..i).rev() {
            // B U V' + C V = -A/j
            let rhs = scale(k, &k.of(&qq(-1, j as i64)), &a);
            let (bb, cc) = diophantine(k, &uvp, v, &rhs, tk)?;
            // A <- -j C - U B'
            let jc = scale(k, &k.of(&q(j as i64)), &cc);
            a = sub(k, &neg(k, &jc), &mul(k, &u, &derivative(k, &bb), tk)?);
            if !bb.is_empty() {
                out.push(Frac { b: bb, v: v.clone(), j: j as u32 });
            }
        }
        d = mul(k, &u, v, tk)?;
    }
    if d.is_empty() {
        d = constant(k, k.one());
    }
    Ok((out, a, d))
}
