//! Dense univariate polynomials over any [`CoeffRing`] (roadmap phase F, item F2): arithmetic, division,
//! composition, subresultant chains, resultants, discriminants, gcds and square-free parts, written once for the
//! integers, the rationals, prime fields, polynomial rings, rational function fields and number fields.
//!
//! A polynomial is a `Vec` of coefficients, index = degree, trimmed (no zero leading coefficient; the zero
//! polynomial is empty), so equal polynomials are equal vectors.
//!
//! **Subresultant chains** follow Ducos, "Optimizations of the subresultant algorithm", J. Pure Appl. Algebra 145
//! (2000): from `S_d` (regular) and `S_(d-1)` of degree `e`, `S_e` is `lc(S_(d-1))^(d-e-1) S_(d-1) / s_d^(d-e-1)`
//! by Lazard's binary powering with exact divisions (Ducos, section 2), and `S_(e-1)` comes from Ducos's reduction
//! (section 3, Algorithm "next") with `d - e + 1` steps of size `e` instead of a pseudo-remainder of size `d`; every
//! division is exact in the coefficient ring. The chain holds every `S_j`, `0 <= j <= deg Q`, the defective and zero
//! ones included, so phases 11A (projection: principal subresultant coefficients) and 14A (Rothstein–Trager
//! resultants over towers) read what they need from one object (decision F2.2).
use crate::ring::CoeffRing;
use crate::work::{exact, Meter, WorkResult};

/// A quotient and a remainder.
pub type QuoRem<E> = (Vec<E>, Vec<E>);

/// Remove zero leading coefficients.
pub fn trim<R: CoeffRing>(r: &R, mut a: Vec<R::E>) -> Vec<R::E> {
    while a.last().is_some_and(|c| r.is_zero(c)) {
        a.pop();
    }
    a
}

/// The degree (`None` for the zero polynomial).
pub fn deg<E>(a: &[E]) -> Option<usize> {
    a.len().checked_sub(1)
}

/// The leading coefficient (zero for the zero polynomial).
pub fn lc<R: CoeffRing>(r: &R, a: &[R::E]) -> R::E {
    a.last().cloned().unwrap_or_else(|| r.zero())
}

/// The coefficient of `x^j` (zero beyond the degree).
pub fn coef<R: CoeffRing>(r: &R, a: &[R::E], j: usize) -> R::E {
    a.get(j).cloned().unwrap_or_else(|| r.zero())
}

/// The constant polynomial `c`.
pub fn constant<R: CoeffRing>(r: &R, c: R::E) -> Vec<R::E> {
    trim(r, vec![c])
}

/// `a + b`.
pub fn add<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E]) -> Vec<R::E> {
    let n = a.len().max(b.len());
    let v = (0..n)
        .map(|i| match (a.get(i), b.get(i)) {
            (Some(x), Some(y)) => r.add(x, y),
            (Some(x), None) => x.clone(),
            (None, Some(y)) => y.clone(),
            (None, None) => r.zero(),
        })
        .collect();
    trim(r, v)
}

/// `a - b`.
pub fn sub<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E]) -> Vec<R::E> {
    add(r, a, &neg(r, b))
}

/// `-a`.
pub fn neg<R: CoeffRing>(r: &R, a: &[R::E]) -> Vec<R::E> {
    a.iter().map(|c| r.neg(c)).collect()
}

/// `c a`.
pub fn scale<R: CoeffRing>(r: &R, a: &[R::E], c: &R::E, m: &mut Meter) -> WorkResult<Vec<R::E>> {
    if r.is_one(c) {
        return Ok(a.to_vec());
    }
    let mut v = Vec::with_capacity(a.len());
    for x in a {
        v.push(r.mul(x, c, m)?);
    }
    Ok(trim(r, v))
}

/// `a / c` for a constant `c` dividing every coefficient; `None` when one does not divide.
pub fn div_scalar<R: CoeffRing>(r: &R, a: &[R::E], c: &R::E, m: &mut Meter) -> WorkResult<Option<Vec<R::E>>> {
    if r.is_one(c) {
        return Ok(Some(a.to_vec()));
    }
    let mut v = Vec::with_capacity(a.len());
    for x in a {
        match r.div_exact(x, c, m)? {
            Some(q) => v.push(q),
            None => return Ok(None),
        }
    }
    Ok(Some(v))
}

/// `x^k a`.
pub fn shift_up<R: CoeffRing>(r: &R, a: &[R::E], k: usize) -> Vec<R::E> {
    if a.is_empty() {
        return vec![];
    }
    let mut v = vec![r.zero(); k];
    v.extend_from_slice(a);
    v
}

/// `a b` (schoolbook).
pub fn mul<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    if a.is_empty() || b.is_empty() {
        return Ok(vec![]);
    }
    let mut v = vec![r.zero(); a.len() + b.len() - 1];
    for (i, x) in a.iter().enumerate() {
        if r.is_zero(x) {
            continue;
        }
        m.step(b.len() as u64)?;
        for (j, y) in b.iter().enumerate() {
            let t = r.mul(x, y, m)?;
            v[i + j] = r.add(&v[i + j], &t);
        }
    }
    Ok(trim(r, v))
}

/// `a^e`.
pub fn pow<R: CoeffRing>(r: &R, a: &[R::E], e: u64, m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let mut acc = constant(r, r.one());
    let mut b = a.to_vec();
    let mut e = e;
    while e > 0 {
        if e & 1 == 1 {
            acc = mul(r, &acc, &b, m)?;
        }
        e >>= 1;
        if e > 0 {
            b = mul(r, &b, &b, m)?;
        }
    }
    Ok(acc)
}

/// Division with remainder `a = q b + rem`, `deg rem < deg b`, when every leading-coefficient division is exact
/// (always over a field, or for a monic `b`); `None` when one is not, or `b` is zero.
pub fn divrem<R: CoeffRing>(
    r: &R,
    a: &[R::E],
    b: &[R::E],
    m: &mut Meter,
) -> WorkResult<Option<QuoRem<R::E>>> {
    let Some(db) = deg(b) else { return Ok(None) };
    let lb = &b[db];
    let mut rem = a.to_vec();
    if rem.len() <= db {
        return Ok(Some((vec![], rem)));
    }
    let mut q = vec![r.zero(); rem.len() - db];
    while rem.len() > db {
        let k = rem.len() - 1 - db;
        m.step(b.len() as u64)?;
        let Some(c) = r.div_exact(&rem[rem.len() - 1], lb, m)? else { return Ok(None) };
        for (j, bj) in b.iter().enumerate() {
            let t = r.mul(&c, bj, m)?;
            rem[k + j] = r.sub(&rem[k + j], &t);
        }
        q[k] = c;
        rem.pop();
        rem = trim(r, rem);
    }
    Ok(Some((trim(r, q), rem)))
}

/// The exact quotient `a / b`; `None` when `b` does not divide `a`.
pub fn div_exact<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m: &mut Meter) -> WorkResult<Option<Vec<R::E>>> {
    Ok(match divrem(r, a, b, m)? {
        Some((q, rem)) if rem.is_empty() => Some(q),
        _ => None,
    })
}

/// The pseudo-remainder `lc(b)^(deg a - deg b + 1) a mod b` (`a` itself when `deg a < deg b`).
pub fn prem<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    // invariant: callers pass a nonzero b
    let db = deg(b).expect("a nonzero divisor");
    if a.len() <= db {
        return Ok(a.to_vec());
    }
    let lb = &b[db];
    let mut rem = a.to_vec();
    let mut steps = (a.len() - db) as u64;
    while rem.len() > db {
        m.step((rem.len() + b.len()) as u64)?;
        let lr = rem[rem.len() - 1].clone();
        let k = rem.len() - 1 - db;
        for c in rem.iter_mut() {
            *c = r.mul(c, lb, m)?;
        }
        for (j, bj) in b.iter().enumerate() {
            let t = r.mul(&lr, bj, m)?;
            rem[k + j] = r.sub(&rem[k + j], &t);
        }
        rem.pop();
        rem = trim(r, rem);
        steps -= 1;
    }
    if steps > 0 {
        let s = r.pow(lb, steps, m)?;
        rem = scale(r, &rem, &s, m)?;
    }
    Ok(rem)
}

/// The derivative.
pub fn derivative<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let mut v = Vec::with_capacity(a.len().saturating_sub(1));
    for (i, c) in a.iter().enumerate().skip(1) {
        v.push(r.mul(c, &r.of_i64(i as i64), m)?);
    }
    Ok(trim(r, v))
}

/// `a(x)` at a ring element (Horner).
pub fn eval<R: CoeffRing>(r: &R, a: &[R::E], x: &R::E, m: &mut Meter) -> WorkResult<R::E> {
    let mut v = r.zero();
    for c in a.iter().rev() {
        v = r.add(&r.mul(&v, x, m)?, c);
    }
    Ok(v)
}

/// The composition `g(h(x))` (Horner in `h`).
pub fn compose<R: CoeffRing>(r: &R, g: &[R::E], h: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let mut v: Vec<R::E> = vec![];
    for c in g.iter().rev() {
        v = mul(r, &v, h, m)?;
        v = add(r, &v, &constant(r, c.clone()));
    }
    Ok(v)
}

/// The content (a gcd of the coefficients, from [`CoeffRing::gcd`]).
pub fn content<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<R::E> {
    let mut g = r.zero();
    for c in a {
        g = r.gcd(&g, c, m)?;
        if r.is_one(&g) {
            break;
        }
    }
    Ok(g)
}

/// The primitive part `a / content(a)`.
pub fn primitive<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    if a.is_empty() {
        return Ok(vec![]);
    }
    let c = content(r, a, m)?;
    exact(div_scalar(r, a, &c, m)?, "the content divides every coefficient")
}

/// `a` divided by its leading coefficient (over a field).
pub fn monic<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    if a.is_empty() {
        return Ok(vec![]);
    }
    let l = lc(r, a);
    exact(div_scalar(r, a, &l, m)?, "a field divides by a nonzero leading coefficient")
}

/// A greatest common divisor: monic over a field (Euclid), the primitive part of the last nonzero subresultant
/// times the gcd of the contents over a domain.
pub fn gcd<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m: &mut Meter) -> WorkResult<Vec<R::E>> {
    if a.is_empty() {
        return if r.is_field() { monic(r, b, m) } else { Ok(b.to_vec()) };
    }
    if b.is_empty() {
        return if r.is_field() { monic(r, a, m) } else { Ok(a.to_vec()) };
    }
    if r.is_field() {
        let (mut x, mut y) = (a.to_vec(), b.to_vec());
        while !y.is_empty() {
            m.step(x.len() as u64)?;
            let (_, rem) = exact(divrem(r, &x, &y, m)?, "a field divides")?;
            x = y;
            y = rem;
        }
        return monic(r, &x, m);
    }
    let (ca, cb) = (content(r, a, m)?, content(r, b, m)?);
    let c = r.gcd(&ca, &cb, m)?;
    let (pa, pb) = (primitive(r, a, m)?, primitive(r, b, m)?);
    let (big, small) = if pa.len() >= pb.len() { (pa, pb) } else { (pb, pa) };
    let ch = subresultant_chain(r, &big, &small, m)?;
    // the last nonzero polynomial of the chain (S_deg small is small itself up to a constant)
    let last = ch.s.iter().find(|s| !s.is_empty()).cloned().unwrap_or_else(|| small.clone());
    let g = primitive(r, &last, m)?;
    scale(r, &g, &c, m)
}

/// Yun's square-free decomposition over a field of characteristic 0: `[(a_1, 1), (a_2, 2), ...]` with `a = lc *
/// prod a_i^i`, each `a_i` monic and square-free, pairwise coprime (factors equal to 1 left out).
pub fn square_free<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<Vec<(Vec<R::E>, u32)>> {
    let mut out = vec![];
    if a.len() <= 1 {
        return Ok(out);
    }
    let da = derivative(r, a, m)?;
    let g = gcd(r, a, &da, m)?;
    let mut b = exact(div_exact(r, a, &g, m)?, "the gcd divides")?;
    let mut c = exact(div_exact(r, &da, &g, m)?, "the gcd divides")?;
    let mut d = sub(r, &c, &derivative(r, &b, m)?);
    let mut i = 1;
    while b.len() > 1 {
        m.step(b.len() as u64)?;
        let ai = gcd(r, &b, &d, m)?;
        b = exact(div_exact(r, &b, &ai, m)?, "the gcd divides")?;
        c = exact(div_exact(r, &d, &ai, m)?, "the gcd divides")?;
        d = sub(r, &c, &derivative(r, &b, m)?);
        if ai.len() > 1 {
            out.push((monic(r, &ai, m)?, i));
        }
        i += 1;
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------- subresultants

/// The subresultant chain of `P` and `Q`, `deg P >= deg Q >= 0`, `Q` nonzero: `s[j]` is the `j`-th subresultant
/// polynomial `S_j` (the determinant polynomial of the Sylvester submatrix, empty for zero) for `0 <= j <= deg Q`,
/// with `S_(deg Q) = lc(Q)^(deg P - deg Q - 1) Q` when `deg P > deg Q` and `Q` when the degrees are equal.
#[derive(Clone, Debug, PartialEq)]
pub struct Chain<E> {
    /// `S_0, S_1, ..., S_(deg Q)`
    pub s: Vec<Vec<E>>,
}

impl<E: Clone + PartialEq + std::fmt::Debug> Chain<E> {
    /// The `j`-th principal subresultant coefficient: the coefficient of `x^j` in `S_j` (zero when `S_j` is
    /// defective or zero).
    pub fn psc<R: CoeffRing<E = E>>(&self, r: &R, j: usize) -> E {
        self.s.get(j).map_or_else(|| r.zero(), |p| coef(r, p, j))
    }
    /// Every principal subresultant coefficient, `psc_0, ..., psc_(deg Q)`.
    pub fn principal_coefficients<R: CoeffRing<E = E>>(&self, r: &R) -> Vec<E> {
        (0..self.s.len()).map(|j| self.psc(r, j)).collect()
    }
    /// The resultant `S_0`.
    pub fn resultant<R: CoeffRing<E = E>>(&self, r: &R) -> E {
        self.psc(r, 0)
    }
    /// The subresultant remainder sequence after `P` and `Q`: the nonzero `S_j`, `j < deg Q`, that follow a
    /// regular `S_(j+1)` (degree `j + 1`), in decreasing `j` (Collins' subresultant PRS).
    pub fn remainders(&self) -> Vec<Vec<E>> {
        let q = self.s.len() - 1;
        (0..q).rev().filter(|&j| !self.s[j].is_empty() && self.s[j + 1].len() == j + 2).map(|j| self.s[j].clone()).collect()
    }
    /// The last nonzero subresultant (a gcd of `P` and `Q` up to a factor in the ring).
    pub fn last_nonzero(&self) -> Option<&Vec<E>> {
        self.s.iter().find(|p| !p.is_empty())
    }
}

/// Lazard's `x^n / y^(n-1)` by binary powering, every division exact (Ducos 2000, section 2).
fn lazard_power<R: CoeffRing>(r: &R, x: &R::E, y: &R::E, n: u64, m: &mut Meter) -> WorkResult<R::E> {
    if n == 0 {
        // x^0 / y^-1 = y
        return Ok(y.clone());
    }
    let mut a = 1u64 << (63 - n.leading_zeros());
    let mut c = x.clone();
    let mut rest = n - a;
    while a > 1 {
        a /= 2;
        c = exact(r.div_exact(&r.mul(&c, &c, m)?, y, m)?, "Lazard's division is exact")?;
        if rest >= a {
            c = exact(r.div_exact(&r.mul(&c, x, m)?, y, m)?, "Lazard's division is exact")?;
            rest -= a;
        }
    }
    Ok(c)
}

/// `S_e = lc(B)^(delta-1) B / s^(delta-1)` (Lazard).
fn lazard<R: CoeffRing>(r: &R, b: &[R::E], s: &R::E, delta: u64, m: &mut Meter) -> WorkResult<Vec<R::E>> {
    let x = lc(r, b);
    // c = x^(delta-1) / s^(delta-2), then S_e = c B / s
    let c = lazard_power(r, &x, s, delta - 1, m)?;
    let cb = scale(r, b, &c, m)?;
    exact(div_scalar(r, &cb, s, m)?, "Lazard's division is exact")
}

/// Ducos's reduction: `S_(e-1)` from `A = S_d` (or a polynomial with the same leading behaviour, see
/// [`subresultant_chain`]), `B = S_(d-1)` of degree `e`, `C = S_e` and `s = s_d`, the principal coefficient
/// of `S_d`.
fn ducos_next<R: CoeffRing>(
    r: &R,
    a: &[R::E],
    b: &[R::E],
    c: &[R::E],
    s: &R::E,
    m: &mut Meter,
) -> WorkResult<Option<Vec<R::E>>> {
    let (d, e) = (a.len() - 1, c.len() - 1);
    let cd1 = lc(r, b);
    let se = lc(r, c);
    // H_j = se x^j for j < e; H_e = se x^e - C (degree < e)
    let mut he: Vec<R::E> = c[..e].iter().map(|x| r.neg(x)).collect();
    he = trim(r, he);
    // D accumulates sum_{j<e} a_j se x^j + sum_{e<=j<d} a_j H_j
    let mut dsum: Vec<R::E> = Vec::with_capacity(e);
    for aj in a.iter().take(e) {
        dsum.push(r.mul(aj, &se, m)?);
    }
    dsum = trim(r, dsum);
    dsum = add(r, &dsum, &scale(r, &he, &a[e], m)?);
    let mut h = he;
    // x H_(j-1) reduced by B: H_j = x H_(j-1) - coef_e(x H_(j-1)) B / cd1
    let reduce = |h: &[R::E], m: &mut Meter| -> WorkResult<Option<QuoRem<R::E>>> {
        let xh = shift_up(r, h, 1);
        let t = coef(r, &xh, e);
        let tb = scale(r, b, &t, m)?;
        let Some(tb) = div_scalar(r, &tb, &cd1, m)? else { return Ok(None) };
        Ok(Some((sub(r, &xh, &tb), vec![t])))
    };
    for aj in a.iter().take(d).skip(e + 1) {
        m.step(e as u64 + 1)?;
        let Some((hj, _)) = reduce(&h, m)? else { return Ok(None) };
        h = hj;
        dsum = add(r, &dsum, &scale(r, &h, aj, m)?);
    }
    let Some(dsum) = div_scalar(r, &dsum, &lc(r, a), m)? else { return Ok(None) };
    // S_(e-1) = (-1)^(d-e+1) (cd1 (x H_(d-1) + D) - coef_e(x H_(d-1)) B) / s
    let xh = shift_up(r, &h, 1);
    let t = coef(r, &xh, e);
    let u = sub(r, &scale(r, &add(r, &xh, &dsum), &cd1, m)?, &scale(r, b, &t, m)?);
    let Some(mut u) = div_scalar(r, &u, s, m)? else { return Ok(None) };
    if (d - e + 1) % 2 == 1 {
        u = neg(r, &u);
    }
    Ok(Some(u))
}

/// `S_(e-1) = prem(A, -B) / (s^delta lc(A))`, the classical step (used when Ducos's reduction cannot run).
fn classical_next<R: CoeffRing>(
    r: &R,
    a: &[R::E],
    b: &[R::E],
    s: &R::E,
    delta: u64,
    m: &mut Meter,
) -> WorkResult<Vec<R::E>> {
    let pr = prem(r, a, &neg(r, b), m)?;
    let den = r.mul(&r.pow(s, delta, m)?, &lc(r, a), m)?;
    exact(div_scalar(r, &pr, &den, m)?, "the subresultant division is exact")
}

#[cfg(test)]
static DUCOS_FALLBACKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The subresultant chain (see [`Chain`]). `deg P >= deg Q`, `Q` nonzero.
pub fn subresultant_chain<R: CoeffRing>(r: &R, p: &[R::E], q: &[R::E], m: &mut Meter) -> WorkResult<Chain<R::E>> {
    subresultant_chain_with(r, p, q, true, m)
}

/// The chain with Ducos's reduction (`ducos`) or the classical pseudo-remainder step (for tests and
/// comparisons).
pub fn subresultant_chain_with<R: CoeffRing>(
    r: &R,
    p: &[R::E],
    q: &[R::E],
    ducos: bool,
    m: &mut Meter,
) -> WorkResult<Chain<R::E>> {
    // invariant: callers pass a nonzero q of degree at most deg p
    let (dp, dq) = (deg(p).expect("P nonzero"), deg(q).expect("Q nonzero"));
    assert!(dp >= dq, "deg P >= deg Q");
    let mut s_out: Vec<Vec<R::E>> = vec![vec![]; dq + 1];
    let lq = lc(r, q);
    if dq == 0 {
        // the Sylvester matrix is lc(Q) times the identity of size deg P
        s_out[0] = constant(r, r.pow(&lq, dp as u64, m)?);
        return Ok(Chain { s: s_out });
    }
    s_out[dq] = if dp > dq { scale(r, q, &r.pow(&lq, (dp - dq - 1) as u64, m)?, m)? } else { q.to_vec() };
    let mut s = r.pow(&lq, (dp - dq) as u64, m)?;
    let mut a = q.to_vec();
    let mut b = prem(r, p, &neg(r, q), m)?;
    // with equal degrees the first remainder has one factor lc(Q) too many in Ducos's normalisation
    loop {
        m.step(a.len() as u64)?;
        let d = a.len() - 1;
        if b.is_empty() {
            break;
        }
        let e = b.len() - 1;
        s_out[d - 1] = b.clone();
        let delta = (d - e) as u64;
        let c = if delta > 1 {
            let c = lazard(r, &b, &s, delta, m)?;
            s_out[e] = c.clone();
            c
        } else {
            b.clone()
        };
        if e == 0 {
            break;
        }
        let next = if ducos { ducos_next(r, &a, &b, &c, &s, m)? } else { None };
        #[cfg(test)]
        if ducos && next.is_none() {
            DUCOS_FALLBACKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        b = match next {
            Some(n) => n,
            None => classical_next(r, &a, &b, &s, delta, m)?,
        };
        a = c;
        s = lc(r, &a);
    }
    Ok(Chain { s: s_out })
}

/// The resultant of `a` and `b` (the determinant of the Sylvester matrix; zero when either is zero).
pub fn resultant<R: CoeffRing>(r: &R, a: &[R::E], b: &[R::E], m: &mut Meter) -> WorkResult<R::E> {
    if a.is_empty() || b.is_empty() {
        return Ok(r.zero());
    }
    let (da, db) = (a.len() - 1, b.len() - 1);
    if da >= db {
        Ok(subresultant_chain(r, a, b, m)?.resultant(r))
    } else {
        // res(a, b) = (-1)^(deg a deg b) res(b, a)
        let v = subresultant_chain(r, b, a, m)?.resultant(r);
        Ok(if (da * db) % 2 == 1 { r.neg(&v) } else { v })
    }
}

/// The discriminant `(-1)^(n(n-1)/2) res(a, a') / lc(a)` (1 for degree 1, 0 for constants); `None` when the leading
/// coefficient does not divide the resultant, which happens only in a ring of characteristic dividing `n`.
pub fn discriminant<R: CoeffRing>(r: &R, a: &[R::E], m: &mut Meter) -> WorkResult<Option<R::E>> {
    let n = a.len().saturating_sub(1);
    if n == 0 {
        return Ok(Some(r.zero()));
    }
    if n == 1 {
        return Ok(Some(r.one()));
    }
    let da = derivative(r, a, m)?;
    // res(a, a') with deg a' = n - 1 (or lower in characteristic p): the Sylvester matrix of size 2n - 1
    let res = if da.is_empty() { r.zero() } else { subresultant_chain(r, a, &da, m)?.resultant(r) };
    let res = if da.len() < n {
        // a' of lower degree (characteristic p): the formal resultant with deg a' = n - 1 is lc(a) times more
        // per missing degree (expansion along the first column), so multiply back
        let missing = (n - da.len()) as u64;
        r.mul(&res, &r.pow(&lc(r, a), missing, m)?, m)?
    } else {
        res
    };
    let Some(q) = r.div_exact(&res, &lc(r, a), m)? else { return Ok(None) };
    Ok(Some(if (n * (n - 1) / 2) % 2 == 1 { r.neg(&q) } else { q }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ring::{Integers, PolyRing, Rationals, Zp};
    use crate::work::Unlimited;
    use crate::{Poly, Z};
    use mc_arith::num::Rat;

    fn z(v: &[i64]) -> Vec<Z> {
        trim(&Integers, v.iter().map(|&x| Z::S(x)).collect())
    }

    /// The determinant polynomial of the j-th Sylvester submatrix, computed directly (Bareiss over Q).
    fn sres_direct(p: &[Z], q: &[Z], j: usize) -> Vec<Rat> {
        let (dp, dq) = (p.len() - 1, q.len() - 1);
        let rows = dp + dq - 2 * j;
        let cols = dp + dq - j;
        // rows: x^k P for k < dq - j, x^k Q for k < dp - j; columns = coefficients of x^(cols-1) .. x^0
        let mut mat: Vec<Vec<Rat>> = vec![];
        let row = |c: &[Z], k: usize| -> Vec<Rat> {
            let mut v = vec![Rat::from_i64(0); cols];
            for (i, x) in c.iter().enumerate() {
                let pw = i + k;
                v[cols - 1 - pw] = Rat::from_int(x.to_int());
            }
            v
        };
        for k in (0..dq - j).rev() {
            mat.push(row(p, k));
        }
        for k in (0..dp - j).rev() {
            mat.push(row(q, k));
        }
        // S_j = sum_{i<=j} det(M_i) x^i, M_i = first rows-1 columns and the column of x^i
        let det = |m: Vec<Vec<Rat>>| -> Rat {
            let n = m.len();
            let mut a = m;
            let mut d = Rat::from_i64(1);
            for c in 0..n {
                let Some(piv) = (c..n).find(|&i| !a[i][c].is_zero()) else { return Rat::from_i64(0) };
                if piv != c {
                    a.swap(piv, c);
                    d = d.neg();
                }
                d = d.mul(&a[c][c]);
                let inv = a[c][c].recip().unwrap();
                let (top, rest) = a.split_at_mut(c + 1);
                let pivot = &top[c];
                for row in rest.iter_mut() {
                    let f = row[c].mul(&inv);
                    for (x, y) in row.iter_mut().zip(pivot.iter()).skip(c) {
                        *x = x.sub(&y.mul(&f));
                    }
                }
            }
            d
        };
        let mut out = vec![];
        for i in 0..=j {
            let sub: Vec<Vec<Rat>> = mat
                .iter()
                .map(|r| {
                    let mut v = r[..rows - 1].to_vec();
                    v.push(r[cols - 1 - i].clone());
                    v
                })
                .collect();
            out.push(det(sub));
        }
        trim(&Rationals, out)
    }

    fn as_rat(p: &[Z]) -> Vec<Rat> {
        p.iter().map(|c| Rat::from_int(c.to_int())).collect()
    }

    #[test]
    fn chains_match_determinants() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let r = Integers;
        let mut seed = 12345u64;
        let mut rnd = |k: i64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 33) as i64 % (2 * k + 1)) - k
        };
        let cases: Vec<(Vec<Z>, Vec<Z>)> = vec![
            // the documentation's example: (x-1)^2 (x-2)(x-3), (x-1)(x-4)^2
            (z(&[6, -17, 17, -7, 1]), z(&[-16, 24, -9, 1])),
            (z(&[-1, 5, 0, 3, 0, 0, 0, 2]), z(&[-9, 8, 0, 0, 0, 0, 7])),
            // defective chains: a common factor and gaps
            (z(&[0, 0, 0, 0, 0, 1, 0, 1]), z(&[1, 0, 0, 1])),
            (z(&[2, 0, 0, 0, 0, 0, 1]), z(&[3, 0, 1])),
        ];
        let mut all = cases;
        for _ in 0..60 {
            let dp = 2 + rnd(3).unsigned_abs() as usize * 2;
            let dq = 1 + (rnd(100).unsigned_abs() as usize) % dp;
            let mut p: Vec<Z> = (0..=dp).map(|_| Z::S(rnd(5))).collect();
            let mut q: Vec<Z> = (0..=dq).map(|_| Z::S(rnd(5))).collect();
            // sparse coefficients make defective chains
            for (i, c) in p.iter_mut().enumerate() {
                if i % 3 == 1 {
                    *c = Z::ZERO;
                }
            }
            for (i, c) in q.iter_mut().enumerate() {
                if i % 2 == 1 && rnd(1) == 0 {
                    *c = Z::ZERO;
                }
            }
            p[dp] = Z::S(1 + rnd(2).abs());
            q[dq] = Z::S(1 + rnd(2).abs());
            all.push((trim(&r, p), trim(&r, q)));
        }
        for (p, q) in &all {
            let ch = subresultant_chain(&r, p, q, &mut m).unwrap();
            let cl = subresultant_chain_with(&r, p, q, false, &mut m).unwrap();
            assert_eq!(ch, cl, "Ducos and classical differ for {p:?}, {q:?}");
            let dq = q.len() - 1;
            for j in 0..dq {
                assert_eq!(as_rat(&ch.s[j]), sres_direct(p, q, j), "S_{j} of {p:?}, {q:?}");
            }
        }
        assert_eq!(DUCOS_FALLBACKS.load(std::sync::atomic::Ordering::Relaxed), 0, "Ducos's divisions are exact");
        // the expected values: S_2 = 38 - 49x + 11x^2, S_1 = -36 + 36x, S_0 = 0
        let ch = subresultant_chain(&r, &all[0].0, &all[0].1, &mut m).unwrap();
        assert_eq!(ch.s[2], z(&[38, -49, 11]));
        assert_eq!(ch.s[1], z(&[-36, 36]));
        assert!(ch.s[0].is_empty());
        assert_eq!(ch.remainders(), vec![z(&[38, -49, 11]), z(&[-36, 36])]);
    }

    #[test]
    fn resultants_gcds_over_rings() {
        let mut w = Unlimited;
        let mut m = Meter::new(&mut w);
        let r = Integers;
        // res(x^2 - 2, x - 1) = -1, res(x - 1, x^2 - 2) = -1 (degrees 1 and 2: even product)
        assert_eq!(resultant(&r, &z(&[-2, 0, 1]), &z(&[-1, 1]), &mut m).unwrap(), Z::S(-1));
        assert_eq!(resultant(&r, &z(&[-1, 1]), &z(&[-2, 0, 1]), &mut m).unwrap(), Z::S(-1));
        // the discriminant of 2x^2 + 5x + 3 is 25 - 24
        assert_eq!(discriminant(&r, &z(&[3, 5, 2]), &mut m).unwrap(), Some(Z::S(1)));
        // agrees with the modular resultant of the crate
        let a = z(&[3, -1, 4, 1, -5, 9, 2]);
        let b = z(&[-6, 5, 3, 5, 8]);
        let pa = Poly::from_dense(1, 0, &a);
        let pb = Poly::from_dense(1, 0, &b);
        let want = crate::resultant::resultant(&pa, &pb, 0, &mut m).unwrap().as_constant().unwrap();
        assert_eq!(resultant(&r, &a, &b, &mut m).unwrap(), want);
        // gcd over Z and over F_7
        let g = gcd(&r, &mul(&r, &a, &z(&[1, 1]), &mut m).unwrap(), &mul(&r, &b, &z(&[1, 1]), &mut m).unwrap(), &mut m).unwrap();
        assert!(g == z(&[1, 1]) || g == z(&[-1, -1]), "{g:?}");
        let f7 = Zp::new(7);
        let a7: Vec<u64> = vec![6, 0, 1]; // x^2 - 1
        let b7: Vec<u64> = vec![1, 1]; // x + 1
        assert_eq!(gcd(&f7, &a7, &b7, &mut m).unwrap(), vec![1, 1]);
        // square-free parts over Q: x^2 (x - 1)^3
        let q = Rationals;
        let f: Vec<Rat> = as_rat(&z(&[0, 0, -1, 3, -3, 1]));
        let sf = square_free(&q, &f, &mut m).unwrap();
        assert_eq!(sf.len(), 2);
        assert_eq!((sf[0].1, sf[1].1), (2, 3));
        // a parameter: res_x(x^2 + y, x - y) = y^2 + y over Z[y]
        let pr = PolyRing { nv: 1 };
        let y = Poly::var(1, 0);
        let c = |n: i64| Poly::constant(1, Z::S(n));
        let a = vec![y.clone(), c(0), c(1)];
        let b = vec![y.neg(), c(1)];
        let want = y.mul(&y, &mut m).unwrap().add(&y);
        assert_eq!(resultant(&pr, &a, &b, &mut m).unwrap(), want);
        // composition
        let h = z(&[0, 1, 1]);
        let g = z(&[1, 0, 1]);
        assert_eq!(compose(&r, &g, &h, &mut m).unwrap(), z(&[1, 0, 1, 2, 1]));
    }
}
