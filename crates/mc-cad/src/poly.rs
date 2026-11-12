//! Polynomial helpers of the decomposition: levels, normal forms of factors, and the evaluation of an integer
//! polynomial at a sample point whose coordinates lie in one number field, including Lazard's valuation-based
//! residue at points where the polynomial vanishes identically (McCallum, Parusiński and Paunescu 2019).
use mc_algebra::field::{Elem, NumberField};
use mc_arith::num::{Int, Rat};
use mc_poly::factor::factor;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};
use rustc_hash::FxHashMap;

/// The level of `p`: one more than its highest variable, 0 for a constant.
pub fn level(p: &Poly) -> usize {
    p.used_vars().into_iter().max().map_or(0, |v| v + 1)
}

/// The integer `c` as a rational.
pub fn zrat(c: &Z) -> Rat {
    Rat::from_int(c.to_int())
}

/// The rational `n/d`.
pub fn rat(n: i64, d: i64) -> Rat {
    Rat::new(Int::from_i64(n), Int::from_i64(d))
}

/// The sign of a rational.
pub fn rsign(q: &Rat) -> i32 {
    if q.is_zero() {
        0
    } else if q.num.neg {
        -1
    } else {
        1
    }
}

/// The irreducible factors of `p` that are not constants, primitive with positive leading coefficient, with
/// multiplicities; and the sign of the unit.
pub fn factors(p: &Poly, m: &mut Meter) -> WorkResult<(i32, Vec<(Poly, u32)>)> {
    let f = factor(p, false, m)?;
    Ok((f.unit.signum(), f.factors.into_iter().filter(|(q, _)| !q.is_constant()).collect()))
}

/// The dense coefficients of `p` in variable `v` (index = degree), each a polynomial in the other variables.
pub fn coeffs(p: &Poly, v: usize) -> Vec<Poly> {
    let cs = p.coeffs_in(v);
    let d = cs.first().map_or(0, |c| c.0 as usize);
    let mut out = vec![Poly::zero(p.nvars()); d + 1];
    for (e, c) in cs {
        out[e as usize] = c;
    }
    out
}

/// Powers of the coordinates of a sample point, cached.
pub struct Powers<'a> {
    k: &'a NumberField,
    pt: &'a [Elem],
    cache: FxHashMap<(usize, u32), Elem>,
}

impl<'a> Powers<'a> {
    /// The powers of the point `pt` (coordinates of the variables `0..pt.len()` in `k`).
    pub fn new(k: &'a NumberField, pt: &'a [Elem]) -> Powers<'a> {
        Powers { k, pt, cache: FxHashMap::default() }
    }

    fn pow(&mut self, i: usize, e: u32) -> Elem {
        if e == 0 {
            return self.k.one();
        }
        if e == 1 {
            return self.pt[i].clone();
        }
        if let Some(x) = self.cache.get(&(i, e)) {
            return x.clone();
        }
        let r = self.k.mul(&self.pow(i, e - 1), &self.pt[i]);
        self.cache.insert((i, e), r.clone());
        r
    }

    /// The value of the monomial `c * prod x_i^e_i` over the variables of the point.
    fn monomial(&mut self, e: &[u32], c: &Z) -> Elem {
        let mut v = self.k.of(&zrat(c));
        for (i, &ei) in e.iter().enumerate().take(self.pt.len()) {
            if ei > 0 {
                let p = self.pow(i, ei);
                v = self.k.mul(&v, &p);
            }
        }
        v
    }
}

/// `p` at the point (the variables `0..pt.len()`) as a dense polynomial in variable `pt.len()` over `k`; the
/// variables above it must not occur.
pub fn eval_uni(k: &NumberField, p: &Poly, pt: &[Elem], m: &mut Meter) -> WorkResult<Vec<Elem>> {
    let d = pt.len();
    let deg = if d < p.nvars() { p.degree(d).unwrap_or(0) as usize } else { 0 };
    let mut out = vec![k.zero(); deg + 1];
    let mut pw = Powers::new(k, pt);
    for (e, c) in p.terms() {
        m.step(d as u64 + 1)?;
        let j = if d < p.nvars() { e[d] as usize } else { 0 };
        let t = pw.monomial(e, c);
        out[j] = k.add(&out[j], &t);
    }
    Ok(trim(out))
}

/// The value of `p` (of level at most `pt.len()`) at the point.
pub fn eval_full(k: &NumberField, p: &Poly, pt: &[Elem], m: &mut Meter) -> WorkResult<Elem> {
    let mut pw = Powers::new(k, pt);
    let mut v = k.zero();
    for (e, c) in p.terms() {
        m.step(pt.len() as u64 + 1)?;
        let t = pw.monomial(e, c);
        v = k.add(&v, &t);
    }
    Ok(v)
}

/// Remove leading zero coefficients.
pub fn trim(mut a: Vec<Elem>) -> Vec<Elem> {
    while a.last().is_some_and(|c| c.is_empty()) {
        a.pop();
    }
    a
}

/// The Taylor shift `a(t + c)` of a dense polynomial over `k`.
fn shift(k: &NumberField, a: &[Elem], c: &Elem, m: &mut Meter) -> WorkResult<Vec<Elem>> {
    let mut b = a.to_vec();
    let n = b.len();
    for i in 0..n {
        m.step((n - i) as u64)?;
        for j in (i..n.saturating_sub(1)).rev() {
            let t = k.mul(&b[j + 1], c);
            b[j] = k.add(&b[j], &t);
        }
    }
    Ok(b)
}

/// Lazard's residue of `p` (of level `pt.len() + 1`) at the point: `p` at the point as a polynomial in the next
/// variable when that is not identically zero; otherwise, variable by variable, the lowest coefficient of the
/// expansion in powers of `x_i - a_i` that does not vanish identically (the valuation-based evaluation of
/// Lazard's projection). Returns the residue and whether `p` vanished identically at the point.
pub fn lazard_residue(k: &NumberField, p: &Poly, pt: &[Elem], m: &mut Meter) -> WorkResult<(Vec<Elem>, bool)> {
    let direct = eval_uni(k, p, pt, m)?;
    if !direct.is_empty() {
        return Ok((direct, false));
    }
    let d = pt.len();
    let nv = p.nvars();
    // sparse terms over k: exponents of all variables, coefficient
    let mut g: Vec<(Vec<u32>, Elem)> = p.terms().map(|(e, c)| (e.to_vec(), k.of(&zrat(c)))).collect();
    for i in 0..d {
        // group by the exponents without x_i, dense in x_i
        let mut groups: FxHashMap<Vec<u32>, Vec<Elem>> = FxHashMap::default();
        for (e, c) in &g {
            let mut f = e.clone();
            let j = f[i] as usize;
            f[i] = 0;
            let v = groups.entry(f).or_default();
            if v.len() <= j {
                v.resize(j + 1, k.zero());
            }
            v[j] = k.add(&v[j], c);
        }
        let mut shifted: Vec<(Vec<u32>, Vec<Elem>)> = vec![];
        for (f, v) in groups {
            shifted.push((f, shift(k, &v, &pt[i], m)?));
        }
        let maxj = shifted.iter().map(|s| s.1.len()).max().unwrap_or(0);
        let mut next = vec![];
        for j in 0..maxj {
            next.clear();
            for (f, v) in &shifted {
                if let Some(c) = v.get(j) {
                    if !c.is_empty() {
                        next.push((f.clone(), c.clone()));
                    }
                }
            }
            if !next.is_empty() {
                break;
            }
        }
        g = std::mem::take(&mut next);
        if g.is_empty() {
            return Ok((vec![], true));
        }
    }
    let deg = g.iter().map(|(e, _)| if d < nv { e[d] as usize } else { 0 }).max().unwrap_or(0);
    let mut out = vec![k.zero(); deg + 1];
    for (e, c) in g {
        let j = if d < nv { e[d] as usize } else { 0 };
        out[j] = k.add(&out[j], &c);
    }
    Ok((trim(out), true))
}
