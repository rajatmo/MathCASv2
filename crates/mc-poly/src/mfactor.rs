//! Multivariate factorisation by evaluation and Hensel lifting (decision K.14).
//!
//! For a primitive, square-free q in several variables (primitive with respect to every variable):
//!
//! 1. a polynomial of degree one in some variable is irreducible (it is primitive in that variable);
//! 2. a main variable x is chosen (the smallest positive degree) and the other variables y are set to integer
//!    points a (zeros first, then small values) that keep the leading coefficient `l(y)` of x nonzero and the image
//!    `q(x, a)` square-free; the image with the fewest univariate factors is kept. One factor means q is
//!    irreducible: a factorisation of q gives one of the image of the same degrees;
//! 3. otherwise the factors are lifted from `y = a` by total degree in `y - a` (each step solves univariate
//!    Diophantine equations) with the leading coefficients imposed: `l^(r-1) q` is lifted to `r` factors each of
//!    leading coefficient `l`, which have integer coefficients when the image's factors correspond to the factors
//!    of q. The lift runs modulo word-sized primes; the lifted factors are moved back from the point and combined
//!    by the Chinese remainder theorem until they stop changing, and their primitive parts are checked by
//!    division;
//! 4. when the image splits further than q (the lift does not divide), subsets of the image factors are lifted
//!    as two-factor splits by increasing size.
//!
//! `None` means the method gave up within its size limits; the caller falls back to Kronecker's substitution.
use crate::factor::factor;
use crate::gcd::gcd;
use crate::poly::Poly;
use crate::work::{exact, Meter, WorkResult};
use crate::z::Z;
use crate::modp::{self, Fp, Primes, UPoly};
use crate::resultant::crt;
use mc_arith::num::{Int, Nat};
use rustc_hash::FxHashMap;

/// The most image factors whose subsets are tried.
const MAX_IMAGE_FACTORS: usize = 12;
/// The most monomials in y (up to the lifting degree) the lifting is allowed to handle.
const MAX_LIFT_MONOMIALS: u64 = 200_000;
/// Evaluation points tried for the image with the fewest factors.
const POINTS: usize = 4;

/// The irreducible factors of a primitive square-free multivariate q, or `None` past the size limits.
pub(crate) fn factor_multivariate(q: &Poly, m: &mut Meter) -> WorkResult<Option<Vec<Poly>>> {
    let used = q.used_vars();
    let degs = q.degrees();
    // degree one in a variable: irreducible
    if used.iter().any(|&v| degs[v] == 1) {
        return Ok(Some(vec![q.clone()]));
    }
    // the main variable: the simplest leading coefficient (it is imposed on every lifted factor), then the
    // smallest degree
    let x = *used
        .iter()
        .min_by_key(|&&v| {
            let l = lc_in(q, v);
            (l.total_degree().unwrap_or(0), l.len(), degs[v], v)
        })
        .expect("several variables");
    let ys: Vec<usize> = used.iter().copied().filter(|&v| v != x).collect();
    let Some((point, images)) = choose_point(q, x, &ys, m)? else { return Ok(None) };
    if images.len() == 1 {
        return Ok(Some(vec![q.clone()]));
    }
    if images.len() > MAX_IMAGE_FACTORS {
        return Ok(None);
    }
    // all factors at once
    if let Some(fs) = lift_and_check(q, x, &ys, &point, &images, m)? {
        return Ok(Some(fs));
    }
    // recombination: two-factor lifts of subsets by increasing size
    let mut rest = q.clone();
    let mut pieces = images;
    let mut out = vec![];
    let mut size = 1;
    while 2 * size <= pieces.len() {
        let r = pieces.len();
        let mut found = false;
        let mut idx: Vec<usize> = (0..size).collect();
        loop {
            m.step(1)?;
            let mut a = Poly::constant(q.nv, Z::ONE);
            let mut b = Poly::constant(q.nv, Z::ONE);
            for (i, p) in pieces.iter().enumerate() {
                if idx.contains(&i) {
                    a = a.mul(p, m)?;
                } else {
                    b = b.mul(p, m)?;
                }
            }
            let Some(fs) = lift_and_check(&rest, x, &ys, &point, &[a, b], m)? else {
                if !next_subset(&mut idx, r) {
                    break;
                }
                continue;
            };
            out.push(fs[0].clone());
            rest = exact(rest.div_exact(&fs[0], m)?, "a checked factor divides")?;
            for &i in idx.iter().rev() {
                pieces.remove(i);
            }
            found = true;
            break;
        }
        if !found {
            size += 1;
        }
    }
    if !rest.is_constant() {
        out.push(rest.primitive().1);
    }
    Ok(Some(out))
}

/// The next subset of `idx.len()` indices below `r` in lexicographic order; false after the last.
fn next_subset(idx: &mut [usize], r: usize) -> bool {
    let size = idx.len();
    let mut i = size;
    while i > 0 && idx[i - 1] == r - size + i - 1 {
        i -= 1;
    }
    if i == 0 {
        return false;
    }
    idx[i - 1] += 1;
    for j in i..size {
        idx[j] = idx[j - 1] + 1;
    }
    true
}

/// A small deterministic sequence of evaluation points: all zeros, then pseudo-random small integers.
fn candidate(k: usize, attempt: usize) -> Vec<i64> {
    if attempt == 0 {
        return vec![0; k];
    }
    // splitmix64 per coordinate
    let mix = |mut z: u64| {
        z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    };
    let range = 1 + attempt as u64 / 3;
    (0..k)
        .map(|i| {
            let s = mix((attempt as u64) << 20 | i as u64);
            // zeros keep the shifted polynomial sparse: about half the coordinates on the early attempts
            if attempt < 12 && s % 2 == 0 {
                return 0;
            }
            let v = 1 + ((s >> 1) % range) as i64;
            if (s >> 40) % 2 == 0 {
                v
            } else {
                -v
            }
        })
        .collect()
}

/// The image of q at y = a.
fn eval_at(q: &Poly, ys: &[usize], a: &[i64]) -> Poly {
    let mut p = q.clone();
    for (&v, &c) in ys.iter().zip(a) {
        p = p.eval_var(v, &Z::from_i128(c as i128));
    }
    p
}

/// The point with the fewest image factors among the first good ones, and those factors (primitive, in x).
#[allow(clippy::type_complexity)]
fn choose_point(q: &Poly, x: usize, ys: &[usize], m: &mut Meter) -> WorkResult<Option<(Vec<i64>, Vec<Poly>)>> {
    let n = q.degree(x).unwrap_or(0);
    let mut best: Option<(Vec<i64>, Vec<Poly>)> = None;
    let mut good = 0;
    for attempt in 0..40 {
        m.step(1)?;
        let a = candidate(ys.len(), attempt);
        let img = eval_at(q, ys, &a);
        if img.degree(x).unwrap_or(0) != n {
            continue;
        }
        let fs = factor(&img, false, m)?;
        if fs.factors.iter().any(|(_, e)| *e > 1) {
            continue;
        }
        let pieces: Vec<Poly> = fs.factors.into_iter().map(|(f, _)| f).collect();
        let count = pieces.len();
        if best.as_ref().is_none_or(|b| count < b.1.len()) {
            best = Some((a, pieces));
        }
        good += 1;
        if good >= POINTS || count == 1 {
            break;
        }
    }
    Ok(best)
}

// ------------------------------------------------------------------------------------- lifting

/// A polynomial in x and y mod p: y exponents -> dense coefficient in x.
type Mv = FxHashMap<Vec<u32>, UPoly>;
/// The same with integer coefficients.
type MvZ = FxHashMap<Vec<u32>, Vec<Z>>;
/// A factor's CRT image (in the original variables): exponents -> symmetric residue.
type Image = FxHashMap<Vec<u32>, Int>;

fn deg(e: &[u32]) -> u32 {
    e.iter().sum()
}

/// p (shifted so that the point is the origin) by homogeneous parts in y: y exponents -> dense x coefficients.
fn to_parts(p: &Poly, x: usize, ys: &[usize], top: usize) -> Vec<MvZ> {
    let mut out: Vec<MvZ> = vec![FxHashMap::default(); top + 1];
    for (e, c) in p.terms() {
        let key: Vec<u32> = ys.iter().map(|&v| e[v]).collect();
        let d = deg(&key) as usize;
        let slot = out[d].entry(key).or_default();
        let k = e[x] as usize;
        if slot.len() <= k {
            slot.resize(k + 1, Z::ZERO);
        }
        slot[k] = slot[k].add(c);
    }
    out
}

/// p with y_v replaced by y_v + a for every y variable (the point moved to the origin), or back with -a.
fn shift(p: &Poly, ys: &[usize], a: &[i64], sign: i64, m: &mut Meter) -> WorkResult<Poly> {
    let mut cur = p.clone();
    for (&v, &av) in ys.iter().zip(a) {
        if av == 0 {
            continue;
        }
        let c = Z::from_i128((sign * av) as i128);
        let mut terms = vec![];
        for (e, k) in cur.terms() {
            m.step(1)?;
            let n = e[v];
            // (y + c)^n = sum binom(n, j) c^(n-j) y^j
            let mut binom = Z::ONE;
            for j in 0..=n {
                let mut f = e.to_vec();
                f[v] = j;
                terms.push((f, k.mul(&binom).mul(&c.pow((n - j) as u64))));
                binom = binom.mul_i64((n - j) as i64).div_exact(&Z::from_i128((j + 1) as i128));
            }
        }
        cur = Poly::from_terms(p.nv, terms);
    }
    Ok(cur)
}

/// The leading coefficient of p in x, as a polynomial.
fn lc_in(p: &Poly, x: usize) -> Poly {
    // invariant: callers pass polynomials of positive degree in x
    p.coeffs_in(x).into_iter().next().map(|(_, c)| c).expect("a nonzero polynomial")
}

/// The degree-k part of a * b mod p for a and b given by homogeneous parts, keeping exponents within `bound`.
fn hom_mul(f: Fp, a: &[Mv], b: &[Mv], k: usize, bound: &[u32], m: &mut Meter) -> WorkResult<Mv> {
    let mut out: Mv = FxHashMap::default();
    for j in 0..=k {
        let (Some(pa), Some(pb)) = (a.get(j), b.get(k - j)) else { continue };
        for (ea, ca) in pa {
            for (eb, cb) in pb {
                if ea.iter().zip(eb).zip(bound).any(|((p, q), b)| p + q > *b) {
                    continue;
                }
                m.step(1)?;
                let key: Vec<u32> = ea.iter().zip(eb).map(|(p, q)| p + q).collect();
                let prod = modp::mul(f, ca, cb);
                let slot = out.entry(key).or_default();
                *slot = modp::add(f, slot, &prod);
            }
        }
    }
    out.retain(|_, c| !c.is_empty());
    Ok(out)
}

/// The lifting problem for one point, with integer data.
struct Problem<'a> {
    /// A = l^(r-1) q shifted, by homogeneous parts
    a: Vec<MvZ>,
    /// the shifted leading coefficient l(y + a) (constant in x)
    lsh: MvZ,
    /// l(a)
    la: Z,
    /// the image factors, dense in x
    us: &'a [Vec<Z>],
    /// q*'s degree in each y
    bound: Vec<u32>,
    top: usize,
}

/// The lift mod p: each factor as y exponents -> dense x residues, or `None` for an unlucky prime.
fn lift_mod(pr: &Problem<'_>, f: Fp, m: &mut Meter) -> WorkResult<Option<Vec<Mv>>> {
    let red = |c: &[Z]| -> UPoly {
        let mut v: UPoly = c.iter().map(|z| z.rem_u64(f.p)).collect();
        modp::trim(&mut v);
        v
    };
    let r = pr.us.len();
    let top = pr.top;
    let la = pr.la.rem_u64(f.p);
    if la == 0 {
        return Ok(None);
    }
    // the image factors scaled to leading coefficient l(a)
    let mut ui: Vec<UPoly> = vec![];
    for u in pr.us {
        let d = red(u);
        if d.len() != u.len() {
            return Ok(None);
        }
        // invariant: d has the length of u (checked above), so a nonzero leading coefficient
        let lc = *d.last().expect("a nonconstant factor");
        ui.push(modp::scale(f, &d, f.mul(la, f.inv(lc))));
    }
    // s_i with sum s_i prod_{j != i} u_j = 1
    let mut si: Vec<UPoly> = vec![];
    for i in 0..r {
        let mut b: UPoly = vec![1];
        for (j, u) in ui.iter().enumerate() {
            if j != i {
                b = modp::rem(f, &modp::mul(f, &b, u), &ui[i]);
            }
        }
        let (g, s, _) = modp::xgcd(f, &b, &ui[i]);
        if g.len() != 1 {
            return Ok(None);
        }
        si.push(s);
    }
    let zero: Vec<u32> = vec![0; pr.bound.len()];
    let mut fs: Vec<Vec<Mv>> = vec![];
    for u in &ui {
        let di = u.len() - 1;
        let mut fh: Vec<Mv> = vec![FxHashMap::default(); top + 1];
        fh[0].insert(zero.clone(), u.clone());
        for (e, c) in &pr.lsh {
            let d = deg(e) as usize;
            if d == 0 || d > top {
                continue;
            }
            let c0 = c[0].rem_u64(f.p);
            if c0 == 0 {
                continue;
            }
            let mut v = vec![0u64; di + 1];
            v[di] = c0;
            fh[d].insert(e.clone(), v);
        }
        fs.push(fh);
    }
    // partial products g[i] = F_0 ... F_(i+1), by homogeneous parts
    let mut g: Vec<Vec<Mv>> = vec![vec![FxHashMap::default(); top + 1]; r - 1];
    let partial = |g: &mut Vec<Vec<Mv>>, fs: &Vec<Vec<Mv>>, k: usize, m: &mut Meter| -> WorkResult<()> {
        for i in 0..r - 1 {
            let left = if i == 0 { &fs[0] } else { &g[i - 1] };
            let part = hom_mul(f, left, &fs[i + 1], k, &pr.bound, m)?;
            g[i][k] = part;
        }
        Ok(())
    };
    partial(&mut g, &fs, 0, m)?;
    for k in 1..=top {
        m.step(1)?;
        partial(&mut g, &fs, k, m)?;
        let prod = &g[r - 2][k];
        let ak: Mv = pr.a[k].iter().map(|(e, c)| (e.clone(), red(c))).filter(|(_, c)| !c.is_empty()).collect();
        let mut keys: Vec<Vec<u32>> = ak.keys().cloned().collect();
        keys.extend(prod.keys().filter(|e| !ak.contains_key(*e)).cloned());
        // the corrections of degree k, per factor
        let mut sig: Vec<Mv> = vec![FxHashMap::default(); r];
        for e in keys {
            let empty = vec![];
            let c = modp::sub(f, ak.get(&e).unwrap_or(&empty), prod.get(&e).unwrap_or(&empty));
            if c.is_empty() {
                continue;
            }
            for i in 0..r {
                let sigma = modp::rem(f, &modp::mul(f, &c, &si[i]), &ui[i]);
                if !sigma.is_empty() {
                    sig[i].insert(e.clone(), sigma);
                }
            }
        }
        if sig.iter().all(|s| s.is_empty()) {
            continue;
        }
        // the partial products change by the corrections times the degree-0 parts (exactly, as they are of degree k)
        let times = |a: &Mv, u: &UPoly| -> Mv { a.iter().map(|(e, c)| (e.clone(), modp::mul(f, c, u))).collect() };
        let plus = |a: &mut Mv, b: Mv| {
            for (e, c) in b {
                let slot = a.entry(e).or_default();
                *slot = modp::add(f, slot, &c);
            }
        };
        let mut delta = sig[0].clone();
        let mut left0 = ui[0].clone();
        for i in 0..r - 1 {
            m.step(1)?;
            let mut d = times(&delta, &ui[i + 1]);
            plus(&mut d, times(&sig[i + 1], &left0));
            left0 = modp::mul(f, &left0, &ui[i + 1]);
            let mut gk = std::mem::take(&mut g[i][k]);
            plus(&mut gk, d.clone());
            gk.retain(|_, c| !c.is_empty());
            g[i][k] = gk;
            delta = d;
        }
        for (i, si_k) in sig.into_iter().enumerate() {
            plus(&mut fs[i][k], si_k);
            fs[i][k].retain(|_, c| !c.is_empty());
        }
    }
    Ok(Some(
        fs.into_iter()
            .map(|parts| {
                let mut all: Mv = FxHashMap::default();
                for p in parts {
                    all.extend(p.into_iter().filter(|(_, c)| !c.is_empty()));
                }
                all
            })
            .collect(),
    ))
}

/// The most primes combined before the lift gives up.
const MAX_PRIMES: usize = 40;

/// Lift the image factors `us` of q at `point` to factors of q; `None` when the lift does not give a
/// factorisation (the image split further than q) or would be too large.
///
/// The factors of q* = l^(r-1) q with leading coefficient l have integer coefficients, so the lift runs mod
/// word-sized primes and the images are combined by the Chinese remainder theorem until they stop changing; the
/// candidates are then checked by division. The lift runs by total degree k in y - a; the factors and the partial
/// products are kept as homogeneous parts, so each step multiplies only the parts of degree k. Exponents beyond
/// q*'s degree in each variable are never formed: the lift agrees with the true factors (and their partial
/// products, which divide q*) at every step.
fn lift_and_check(
    q: &Poly,
    x: usize,
    ys: &[usize],
    point: &[i64],
    us: &[Poly],
    m: &mut Meter,
) -> WorkResult<Option<Vec<Poly>>> {
    let nv = q.nv;
    let r = us.len();
    let l = lc_in(q, x);
    let la = eval_at(&l, ys, point).as_constant().unwrap_or(Z::ZERO);
    if la.is_zero() {
        return Ok(None);
    }
    let qs = q.mul(&l.pow(r as u64 - 1, m)?, m)?;
    let degs = qs.degrees();
    let bound: Vec<u32> = ys.iter().map(|&v| degs[v]).collect();
    let mut boxed: u64 = 1;
    for &b in &bound {
        boxed = boxed.saturating_mul(b as u64 + 1);
    }
    if boxed > MAX_LIFT_MONOMIALS {
        return Ok(None);
    }
    let a = shift(&qs, ys, point, 1, m)?;
    let top = a.terms().map(|(e, _)| ys.iter().map(|&v| e[v]).sum::<u32>()).max().unwrap_or(0) as usize;
    let mut lsh: MvZ = FxHashMap::default();
    for p in to_parts(&shift(&l, ys, point, 1, m)?, x, ys, top) {
        lsh.extend(p);
    }
    let dense: Vec<Vec<Z>> = us.iter().map(|u| u.to_dense(x)).collect();
    let pr = Problem { a: to_parts(&a, x, ys, top), lsh, la, us: &dense, bound, top };
    let mut images: Vec<Image> = vec![FxHashMap::default(); r];
    let mut modulus = Nat::from_u64(1);
    let mut used = 0;
    for p in Primes::large() {
        if used >= MAX_PRIMES {
            return Ok(None);
        }
        m.step(1)?;
        let f = Fp::new(p);
        let Some(lifted) = lift_mod(&pr, f, m)? else { continue };
        used += 1;
        let mut changed = false;
        for (img, fm) in images.iter_mut().zip(&lifted) {
            // the factor mod p moved back to the point (sparse again), as residues by exponent
            let mut terms = vec![];
            for (e, c) in fm {
                for (i, &v) in c.iter().enumerate() {
                    if v != 0 {
                        let mut ex = vec![0u32; nv];
                        ex[x] = i as u32;
                        for (j, &w) in ys.iter().enumerate() {
                            ex[w] = e[j];
                        }
                        terms.push((ex, Z::from_i128(v as i128)));
                    }
                }
            }
            let back = shift(&Poly::from_terms(nv, terms), ys, point, -1, m)?;
            let res: FxHashMap<Vec<u32>, u64> =
                back.terms().map(|(e, c)| (e.to_vec(), c.rem_u64(p))).filter(|(_, v)| *v != 0).collect();
            let mut keys: Vec<Vec<u32>> = img.keys().cloned().collect();
            keys.extend(res.keys().filter(|e| !img.contains_key(*e)).cloned());
            for key in keys {
                let r = res.get(&key).copied().unwrap_or(0);
                let old = img.get(&key).cloned().unwrap_or_else(|| Int::from_i64(0));
                let new = crt(&old, &modulus, r, p);
                if new != old {
                    changed = true;
                }
                if new.is_zero() {
                    img.remove(&key);
                } else {
                    img.insert(key, new);
                }
            }
        }
        modulus = modulus.mul_u64(p);
        // check when the images stopped changing, and at once after one prime when they are small (the common case)
        let small = used == 1 && images.iter().all(|img| img.values().all(|v| v.mag.bits() < 40));
        if changed && !small {
            continue;
        }
        if let Some(out) = check(q, x, &images, m)? {
            return Ok(Some(out));
        }
        if !changed {
            // stable images that do not divide: the image split further than q
            return Ok(None);
        }
    }
    Ok(None)
}

/// The candidate factors from their CRT images, primitive in x, checked by division of q.
fn check(q: &Poly, x: usize, images: &[Image], m: &mut Meter) -> WorkResult<Option<Vec<Poly>>> {
    let nv = q.nv;
    let mut rest = q.clone();
    let mut out = vec![];
    for img in images {
        let terms = img.iter().map(|(e, c)| (e.clone(), Z::from_int(c.clone()))).collect();
        let g = Poly::from_terms(nv, terms).primitive().1;
        if g.degree(x).unwrap_or(0) == 0 {
            return Ok(None);
        }
        let mut c = Poly::zero(nv);
        for (_, co) in g.coeffs_in(x) {
            c = gcd(&c, &co, m)?;
            if c.is_constant() {
                break;
            }
        }
        let g = if c.is_constant() { g } else { exact(g.div_exact(&c, m)?, "the content divides")?.primitive().1 };
        match rest.div_exact(&g, m)? {
            Some(qt) => rest = qt,
            None => return Ok(None),
        }
        out.push(g);
    }
    if !rest.is_constant() {
        return Ok(None);
    }
    Ok(Some(out))
}
