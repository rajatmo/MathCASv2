//! The Meijer G-function `meijer_g([[a_1, ..., a_n], [a_(n+1), ..., a_p]], [[b_1, ..., b_m], [b_(m+1), ..., b_q]],
//! z)` and the generalized `meijer_g(as, bs, z, r)` = G(z^(1/r)) (round 22).
//!
//! Numerical values at any precision and across the complex plane come from [`crate::meijer_num`] (Slater's
//! theorem on the side of |z| = 1 where it converges, perturbation of coincident poles). This module gives the
//! exact results, in this order (Luke 1969, sections 5.2-5.6; Prudnikov, Brychkov and Marichev, *Integrals and
//! Series* 3, 1990, sections 8.2.2 and 8.4):
//!
//! 1. **Order reduction** (PBM 8.2.2.7-8): a parameter a_j (j <= n) equal to a b_k (k > m), or a_j (j > n) equal
//!    to a b_k (k <= m), cancels.
//! 2. **z = 0**: 0 when every Re b_k > 0 (k <= m, p <= q); the k-th Slater coefficient when b_k = 0 and the others
//!    are positive; `complex_infinity` when one is negative or p > q.
//! 3. **Named functions** valid for every parameter (no confluent case): G^{1,0}_{0,1} = z^b e^-z, G^{1,1}_{1,1} =
//!    Gamma(1 - a + b) z^b (1 + z)^(a - b - 1), G^{1,0}_{0,2} = z^((b1+b2)/2) J_(b1-b2)(2 sqrt z) (elementary for
//!    b1 - b2 = +-1/2), G^{2,0}_{0,2} = 2 z^((b1+b2)/2) K_(b1-b2)(2 sqrt z), G^{2,1}_{1,2} = Gamma(1 - a + b1)
//!    Gamma(1 - a + b2) z^b1 U(1 - a + b1, 1 + b1 - b2, z), G^{2,0}_{1,2} = z^b1 e^-z U(a - b2, 1 + b1 - b2, z)
//!    (PBM 8.4.3.1, 8.4.2.5, 8.4.19.1, 8.4.23.1, 8.4.46.1, 8.4.16.?; DLMF 10.2.2, 10.27.4, 13.4.4 by the Mellin
//!    transforms of both sides); p > q through G^{n,m}_{q,p}(1/z | 1 - b; 1 - a) (DLMF 16.19.1).
//! 4. **Slater's sum** (DLMF 16.17.2) of hypergeometric functions when the poles are simple, for p < q, and for
//!    p = q when (-1)^(p-m-n) = -1 (the singular point at z = -1, so the sum continues the function to the whole
//!    plane cut along (-inf, -1]); for p = q with the singular point at z = 1 the result is a `piecewise` of the
//!    two sums on |z| < 1 and |z| > 1. Each pFq is then written as a named function where one is known: 0F0 as
//!    exp, 1F0 as a power, 0F1 as Bessel J or I (circular and hyperbolic functions at order +-1/2), 1F1(a; 2a; w)
//!    with a = 1/2 as e^(w/2) I_0(w/2), 2F1(1/2, +-1/2; 1; m) as the complete elliptic integrals.
//!
//! Other exact arguments stay unevaluated (`n` gives their values); numbers with one inexact evaluate.
use mc_api::T;
use mc_arith::num::{Int, Rat};
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use mc_numeric::cball::CBall;
use mc_numeric::formula::{Def, Fx};
use mc_numeric::numeval::{cball_term, mach_term};

type R<'h> = Res<Option<T<'h>>>;

/// The parameters as terms (see [`crate::meijer_num::Params`] for the grouping).
#[derive(Clone)]
struct G<'h> {
    an: Vec<T<'h>>,
    ap: Vec<T<'h>>,
    bm: Vec<T<'h>>,
    bq: Vec<T<'h>>,
}

impl<'h> G<'h> {
    fn mnpq(&self) -> (usize, usize, usize, usize) {
        (self.bm.len(), self.an.len(), self.an.len() + self.ap.len(), self.bm.len() + self.bq.len())
    }
    fn flipped(&self, fx: &Fx<'_, '_, '_, 'h>) -> G<'h> {
        let f = |v: &[T<'h>]| v.iter().map(|&x| fx.sub(fx.int(1), x)).collect::<Vec<_>>();
        G { an: f(&self.bm), ap: f(&self.bq), bm: f(&self.an), bq: f(&self.ap) }
    }
    fn all(&self) -> impl Iterator<Item = &T<'h>> {
        self.an.iter().chain(&self.ap).chain(&self.bm).chain(&self.bq)
    }
}

/// `[[..], [..]]` as two vectors.
fn pair<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<(Vec<T<'h>>, Vec<T<'h>>)> {
    let l = fx.as_list(t)?;
    if l.len() != 2 {
        return None;
    }
    Some((fx.as_list(l[0])?.to_vec(), fx.as_list(l[1])?.to_vec()))
}

fn rule<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (Some((an, ap)), Some((bm, bq))) = (pair(fx, a[0]), pair(fx, a[1])) else { return Ok(None) };
    let z = a[2];
    let r = a.get(3).copied();
    // a list of arguments threads (matrices too: the items are lists again)
    if let Some(zs) = fx.as_list(z) {
        fx.tick(zs.len() as u64)?;
        let items: Vec<T<'h>> = zs
            .iter()
            .map(|&zi| {
                let mut xs = a.to_vec();
                xs[2] = zi;
                fx.call("meijer_g", &xs)
            })
            .collect();
        return Ok(Some(fx.list(&items)));
    }
    let g = G { an, ap, bm, bq };
    if let Some(v) = numeric(fx, &g, z, r)? {
        return Ok(Some(v));
    }
    // numbers throughout, one inexact, and no value by the series (cancellation beyond the precision cap at a
    // large argument): the named closed forms below still apply (exp(-z) underflows to 0.0), but a machine pFq
    // that overflowed gives `indeterminate` there, which is not an answer
    let inexact = {
        let mut ts: Vec<T<'h>> = g.all().copied().collect();
        ts.push(z);
        ts.iter().map(|&t| fx.num(t)).collect::<Option<Vec<Num>>>().is_some_and(|ns| ns.iter().any(|n| n.kind() != Kind::Exact))
    };
    let g = cancel(fx, g)?;
    let z = match r {
        None => z,
        Some(r) if fx.is_int(r, 1) => z,
        Some(r) => fx.eval(fx.pow(z, fx.div(fx.int(1), r)))?,
    };
    if fx.num(z).is_some_and(|n| n.is_exact_zero()) {
        return at_zero(fx, &g);
    }
    let v = reduce(fx, &g, z, 0)?;
    // numbers throughout: a sum of general pFq series is no simpler than the function (its own numerical
    // values are better), so only named functions are returned
    let all_numbers = g.all().chain(std::iter::once(&z)).all(|&t| fx.num(t).is_some());
    let v = match v {
        Some(t) if all_numbers => {
            let t = fx.eval(t)?;
            if fx.contains(t, "hypergeometric_pfq") {
                None
            } else {
                Some(t)
            }
        }
        v => v,
    };
    match v {
        Some(v) => {
            let v = fx.eval(v)?;
            if inexact && (fx.machine_of(v).is_some_and(|x| !x.is_finite()) || v == fx.sym("indeterminate") || fx.infinity(v).is_some() || v == fx.sym("complex_infinity")) {
                return Ok(None);
            }
            Ok(Some(v))
        }
        None => {
            // the cancelled form, if shorter
            let (m, n, p, q) = g.mnpq();
            let (m0, n0, p0, q0) = {
                let (Some((an, ap)), Some((bm, bq))) = (pair(fx, a[0]), pair(fx, a[1])) else { return Ok(None) };
                (bm.len(), an.len(), an.len() + ap.len(), bm.len() + bq.len())
            };
            if (m, n, p, q) != (m0, n0, p0, q0) {
                return Ok(Some(build(fx, &g, z)));
            }
            Ok(None)
        }
    }
}

/// `meijer_g(...)` of these parameters at z.
fn build<'h>(fx: &Fx<'_, '_, '_, 'h>, g: &G<'h>, z: T<'h>) -> T<'h> {
    let a = fx.list(&[fx.list(&g.an), fx.list(&g.ap)]);
    let b = fx.list(&[fx.list(&g.bm), fx.list(&g.bq)]);
    fx.call("meijer_g", &[a, b, z])
}

// ---------------------------------------------------------------- numbers

/// Every parameter and z numbers, one inexact: the value of [`crate::meijer_num::meijer_g`].
fn numeric<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g: &G<'h>, z: T<'h>, r: Option<T<'h>>) -> R<'h> {
    let mut ts: Vec<T<'h>> = g.all().copied().collect();
    ts.push(z);
    if let Some(r) = r {
        ts.push(r);
    }
    let Some(nums) = ts.iter().map(|&t| fx.num(t)).collect::<Option<Vec<Num>>>() else { return Ok(None) };
    if nums.iter().all(|n| n.kind() == Kind::Exact) {
        return Ok(None);
    }
    let machine = nums.iter().any(|n| n.kind() == Kind::Machine);
    let prec = |r: &Real| r.prec();
    let p = if machine {
        53
    } else {
        nums.iter()
            .filter_map(|n| match n {
                Num::Real(r) => prec(r),
                Num::Complex(x, y) => match (prec(x), prec(y)) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                },
            })
            .min()
            .unwrap_or(53)
    };
    let (na, np, nm) = (g.an.len(), g.ap.len(), g.bm.len());
    let nq = g.bq.len();
    // exact parameters become balls at a working precision above the cancellation of the Slater series (the terms
    // near e^(3 z^(1/3)) of G^{3,0}_{0,3} at z = 1e4 are 1e28 times the value: a parameter ball of radius 2^-117 gave
    // garbage, X26-6); the first pass measures that cancellation
    let build = |w: u64| -> (Vec<CBall>, crate::meijer_num::Params) {
        let ball = |r: &Real| match r {
            Real::Machine(v) => mc_arith::num::Ball::exact(crate::ball::float_of(*v)),
            Real::Ball(b, _) => b.clone(),
            Real::Exact(q) => crate::ball::rat_ball(q, w),
        };
        let cb = |n: &Num| match n {
            Num::Real(r) => CBall::real(ball(r)),
            Num::Complex(x, y) => CBall::new(ball(x), ball(y)),
        };
        let cs: Vec<CBall> = nums.iter().map(cb).collect();
        let par = crate::meijer_num::Params {
            an: cs[..na].to_vec(),
            ap: cs[na..na + np].to_vec(),
            bm: cs[na + np..na + np + nm].to_vec(),
            bq: cs[na + np + nm..na + np + nm + nq].to_vec(),
        };
        (cs, par)
    };
    let (mut cs, mut par) = build(p + 64);
    if r.is_none() {
        let need = crate::meijer_num::cancellation_bits(&par, &cs[na + np + nm + nq]);
        if need > 0.0 {
            // the value can be as small as e^-(3 z^(1/3)) against terms of e^(3 z^(1/3)): twice the largest term
            (cs, par) = build(p + 64 + 2 * need.ceil() as u64 + 64);
        }
    }
    let zc = &cs[na + np + nm + nq];
    let rc = r.map(|_| &cs[cs.len() - 1]);
    fx.tick(1 + p / 64)?;
    let v = crate::meijer_num::meijer_g(&par, zc, rc, p);
    if mc_arith::num::stopped() {
        return Ok(None);
    }
    let Some(mut v) = v else { return Ok(None) };
    // real parameters at a positive argument: a real value (the imaginary part is rounding noise)
    let zpos = matches!(&nums[na + np + nm + nq], Num::Real(x) if x.is_positive());
    if zpos && nums.iter().all(|n| matches!(n, Num::Real(_))) {
        v = CBall::real(v.re);
    }
    if machine {
        let re = crate::ball::f64_of(&v.re.mid);
        let im = crate::ball::f64_of(&v.im.mid);
        let m = if v.is_real() || (im == 0.0 && nums.iter().all(|n| matches!(n, Num::Real(_)))) {
            mc_numeric::fun::Mach::Real(re)
        } else {
            mc_numeric::fun::Mach::Complex(mc_numeric::machine::C64 { re, im })
        };
        return Ok(Some(mach_term(&fx.ar, m)));
    }
    Ok(Some(cball_term(&fx.ar, v, p)))
}

// ---------------------------------------------------------------- exact forms

/// Is the term the exact rational q?
fn rat<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<Rat> {
    fx.rat_of(t)
}

/// Is `t` an exact integer (as a rational)?
fn integer<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<bool> {
    let q = rat(fx, t)?;
    Some(q.is_int())
}

/// a - b evaluated.
fn diff<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: T<'h>, b: T<'h>) -> Res<T<'h>> {
    let d = fx.sub(a, b);
    fx.eval(d)
}

/// Order reduction (module comment, step 1).
fn cancel<'h>(fx: &mut Fx<'_, '_, '_, 'h>, mut g: G<'h>) -> Res<G<'h>> {
    'outer: loop {
        for i in 0..g.an.len() {
            for j in 0..g.bq.len() {
                let d = diff(fx, g.an[i], g.bq[j])?;
                if fx.num(d).is_some_and(|n| n.is_exact_zero()) {
                    g.an.remove(i);
                    g.bq.remove(j);
                    continue 'outer;
                }
            }
        }
        for i in 0..g.ap.len() {
            for j in 0..g.bm.len() {
                let d = diff(fx, g.ap[i], g.bm[j])?;
                if fx.num(d).is_some_and(|n| n.is_exact_zero()) {
                    g.ap.remove(i);
                    g.bm.remove(j);
                    continue 'outer;
                }
            }
        }
        return Ok(g);
    }
}

/// The Slater coefficient A_k (module comment of [`crate::meijer_num`]).
fn coefficient<'h>(fx: &Fx<'_, '_, '_, 'h>, g: &G<'h>, k: usize) -> T<'h> {
    let bk = g.bm[k];
    let one = fx.int(1);
    let (mut num_args, mut den_args) = (vec![], vec![]);
    for (l, &bl) in g.bm.iter().enumerate() {
        if l != k {
            num_args.push(fx.sub(bl, bk));
        }
    }
    for &al in &g.an {
        num_args.push(fx.add(&[one, bk, fx.neg(al)]));
    }
    for &bl in &g.bq {
        den_args.push(fx.add(&[one, bk, fx.neg(bl)]));
    }
    for &al in &g.ap {
        den_args.push(fx.sub(al, bk));
    }
    // 1/Gamma is 0 at a pole of an exact argument, so the term of the series vanishes (B23-19: a_3 - b_2 = -2 in
    // G^{3,0}_{3,3} failed with `pole`); with a numerator Gamma at a pole too the quotient is a limit and stays as it is
    let pole = |t: T<'h>| rat(fx, t).is_some_and(|q| q.is_int() && (q.num.neg || q.is_zero()));
    if den_args.iter().any(|&a| pole(a)) && !num_args.iter().any(|&a| pole(a)) {
        return fx.int(0);
    }
    let num: Vec<T<'h>> = num_args.iter().map(|&a| fx.call("gamma", &[a])).collect();
    let den: Vec<T<'h>> = den_args.iter().map(|&a| fx.call("gamma", &[a])).collect();
    fx.div(fx.mul(&num), fx.mul(&den))
}

/// G at z = 0 (module comment, step 2).
fn at_zero<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g: &G<'h>) -> R<'h> {
    let (m, _, p, q) = g.mnpq();
    if p > q {
        return Ok(Some(fx.sym("complex_infinity")));
    }
    if m == 0 {
        return Ok(Some(fx.int(0)));
    }
    let mut zero_at = None;
    for (k, &b) in g.bm.iter().enumerate() {
        let Some(n) = fx.num(b) else { return Ok(None) };
        let re = match &n {
            Num::Real(r) => r.clone(),
            Num::Complex(x, _) => x.clone(),
        };
        if re.is_negative() {
            return Ok(Some(fx.sym("complex_infinity")));
        }
        if re.is_zero() {
            if zero_at.is_some() {
                return Ok(None);
            }
            zero_at = Some(k);
        }
    }
    match zero_at {
        None => Ok(Some(fx.int(0))),
        Some(k) => {
            let c = coefficient(fx, g, k);
            Ok(Some(fx.eval(c)?))
        }
    }
}

/// The named forms and Slater's sums (module comment, steps 3 and 4).
fn reduce<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g: &G<'h>, z: T<'h>, depth: u32) -> R<'h> {
    let (m, n, p, q) = g.mnpq();
    let one = fx.int(1);
    let half = fx.rat(1, 2);
    let sqrt = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.pow(t, fx.rat(1, 2));
    if m == 0 && p < q {
        return Ok(Some(fx.int(0)));
    }
    match (m, n, p, q) {
        (1, 0, 0, 1) => {
            let b = g.bm[0];
            return Ok(Some(fx.mul(&[fx.pow(z, b), fx.call("exp", &[fx.neg(z)])])));
        }
        (1, 1, 1, 1) => {
            let (a, b) = (g.an[0], g.bm[0]);
            let c = fx.call("gamma", &[fx.add(&[one, b, fx.neg(a)])]);
            let e = fx.add(&[a, fx.neg(b), fx.int(-1)]);
            return Ok(Some(fx.mul(&[c, fx.pow(z, b), fx.pow(fx.add(&[one, z]), e)])));
        }
        (1, 0, 0, 2) => {
            let (b1, b2) = (g.bm[0], g.bq[0]);
            let nu = diff(fx, b1, b2)?;
            let s = sqrt(fx, z);
            let two_s = fx.mul(&[fx.int(2), s]);
            let rpi = fx.pow(fx.sym("pi"), fx.rat(-1, 2));
            if rat(fx, nu) == Some(Rat::new(Int::from_i64(1), Int::from_i64(2))) {
                // z^(b1 - 1/2) sin(2 sqrt z)/sqrt(pi)
                let e = fx.sub(b1, half);
                return Ok(Some(fx.mul(&[fx.pow(z, e), fx.call("sin", &[two_s]), rpi])));
            }
            if rat(fx, nu) == Some(Rat::new(Int::from_i64(-1), Int::from_i64(2))) {
                // cos is even: 2 sqrt(c u^2) = 2 sqrt(c) u for c > 0 whatever the sign of u
                let two_e = fx.mul(&[fx.int(2), even_sqrt(fx, z)]);
                return Ok(Some(fx.mul(&[fx.pow(z, b1), fx.call("cos", &[two_e]), rpi])));
            }
            let e = fx.mul(&[half, fx.add(&[b1, b2])]);
            // an argument c u^2 (the form z^(1/r) of an r = 1/2 call) gives the root sqrt(c) u, the x > 0
            // convention of the Mellin representations
            let two_s = fx.mul(&[fx.int(2), even_sqrt(fx, z)]);
            return Ok(Some(fx.mul(&[fx.pow(z, e), fx.call("bessel_j", &[nu, two_s])])));
        }
        (2, 0, 0, 2) => {
            let (b1, b2) = (g.bm[0], g.bm[1]);
            let nu = diff(fx, b1, b2)?;
            let s = sqrt(fx, z);
            let e = fx.mul(&[half, fx.add(&[b1, b2])]);
            return Ok(Some(fx.mul(&[fx.int(2), fx.pow(z, e), fx.call("bessel_k", &[nu, fx.mul(&[fx.int(2), s])])])));
        }
        (2, 1, 1, 2) => {
            let (a, b1, b2) = (g.an[0], g.bm[0], g.bm[1]);
            let c1 = fx.call("gamma", &[fx.add(&[one, b1, fx.neg(a)])]);
            let c2 = fx.call("gamma", &[fx.add(&[one, b2, fx.neg(a)])]);
            let u = fx.call("hypergeometric_u", &[fx.add(&[one, b1, fx.neg(a)]), fx.add(&[one, b1, fx.neg(b2)]), z]);
            return Ok(Some(fx.mul(&[c1, c2, fx.pow(z, b1), u])));
        }
        (2, 0, 1, 2) => {
            let (a, b1, b2) = (g.ap[0], g.bm[0], g.bm[1]);
            // a = b2 + 1: z^b2 Gamma(b1 - b2, z) (U(1, c, z) = z^(1-c) e^z Gamma(c - 1, z), DLMF 13.6.6 with Kummer's
            // transformation 13.2.40); likewise with b1 and b2 exchanged
            for (x, y) in [(b1, b2), (b2, b1)] {
                let d = diff(fx, a, y)?;
                if fx.is_int(d, 1) {
                    let e = diff(fx, x, y)?;
                    return Ok(Some(fx.mul(&[fx.pow(z, y), fx.call("gamma", &[e, z])])));
                }
            }
            let u = fx.call("hypergeometric_u", &[fx.sub(a, b2), fx.add(&[one, b1, fx.neg(b2)]), z]);
            return Ok(Some(fx.mul(&[fx.pow(z, b1), fx.call("exp", &[fx.neg(z)]), u])));
        }
        _ => {}
    }
    if p > q {
        if depth > 0 {
            return Ok(None);
        }
        let f = g.flipped(fx);
        let f = G {
            an: f.an.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
            ap: f.ap.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
            bm: f.bm.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
            bq: f.bq.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
        };
        let zi = fx.eval(fx.div(one, z))?;
        return reduce(fx, &f, zi, depth + 1);
    }
    let sign_neg = (p as i64 - m as i64 - n as i64).rem_euclid(2) == 1;
    // p = q with m + n < p and the singular point at -1: the Slater sum on |z| < 1 (zero when m = 0) is not the
    // function for |z| > 1 (the loop contour changes sides of the unit circle), so it takes the two-sided path
    let two_sided = p == q && sign_neg && m + n < p;
    if p < q || (sign_neg && !two_sided) {
        return slater(fx, g, z);
    }
    // p = q, singular point at z = 1: the two sums on either side of the unit circle
    if let Some(Num::Real(r)) = fx.num(z) {
        let v = r.to_f64().abs();
        if v < 1.0 {
            return slater(fx, g, z);
        }
        if v > 1.0 {
            let f = g.flipped(fx);
            let zi = fx.eval(fx.div(one, z))?;
            return slater(fx, &f, zi);
        }
        return Ok(None);
    }
    if depth > 0 {
        return Ok(None);
    }
    let Some(inside) = slater(fx, g, z)? else { return Ok(None) };
    let f = g.flipped(fx);
    let zi = fx.eval(fx.div(one, z))?;
    let Some(outside) = slater(fx, &f, zi)? else { return Ok(None) };
    let az = fx.call("abs", &[z]);
    let c1 = fx.call("less", &[az, one]);
    let c2 = fx.call("greater", &[az, one]);
    let cases = fx.list(&[fx.list(&[inside, c1]), fx.list(&[outside, c2])]);
    Ok(Some(fx.call("piecewise", &[cases])))
}

/// Slater's sum over the b's with simple poles (module comment, step 4), or `None`.
fn slater<'h>(fx: &mut Fx<'_, '_, '_, 'h>, g: &G<'h>, z: T<'h>) -> R<'h> {
    let (m, n, p, _) = g.mnpq();
    let one = fx.int(1);
    // simple poles: the b_k (k <= m) pairwise not differing by integers (an unknown difference counts as generic)
    let g = G {
        an: g.an.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
        ap: g.ap.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
        bm: g.bm.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
        bq: g.bq.iter().map(|&t| fx.eval(t)).collect::<Res<_>>()?,
    };
    for k in 0..m {
        for l in 0..m {
            if l != k {
                let d = diff(fx, g.bm[l], g.bm[k])?;
                if fx.num(d).is_some() && integer(fx, d) != Some(false) {
                    return Ok(None);
                }
                if fx.num(d).is_none() && is_integer_shift(fx, d) {
                    return Ok(None);
                }
            }
        }
    }
    let w = if (p as i64 - m as i64 - n as i64).rem_euclid(2) == 1 { fx.neg(z) } else { z };
    let w = fx.eval(w)?;
    let mut terms = vec![];
    for k in 0..m {
        let bk = g.bm[k];
        let mut upper = vec![];
        for &a in g.an.iter().chain(&g.ap) {
            upper.push(fx.eval(fx.add(&[one, bk, fx.neg(a)]))?);
        }
        let mut lower = vec![];
        for (l, &b) in g.bm.iter().enumerate().filter(|&(l, _)| l != k).chain(g.bq.iter().enumerate().map(|(l, b)| (l + m, b))) {
            let _ = l;
            lower.push(fx.eval(fx.add(&[one, bk, fx.neg(b)]))?);
        }
        let c = fx.eval(coefficient(fx, &g, k))?;
        if fx.num(c).is_some_and(|v| v.is_exact_zero()) {
            continue;
        }
        // a lower parameter at a pole of the series: no closed form here
        if lower.iter().any(|&b| rat(fx, b).is_some_and(|q| q.is_int() && q.cmp_value(&Rat::from_i64(0)) != std::cmp::Ordering::Greater)) {
            return Ok(None);
        }
        let Some(f) = named_pfq(fx, upper, lower, w)? else { return Ok(None) };
        terms.push(fx.mul(&[c, fx.pow(z, bk), f]));
    }
    Ok(Some(fx.add(&terms)))
}

/// Is `d` an integer plus symbols that cancel (not decidable here): `false` (treated as generic).
fn is_integer_shift<'h>(_fx: &Fx<'_, '_, '_, 'h>, _d: T<'h>) -> bool {
    false
}

/// A square root of `y` for a rule that is even in it (cos, sin(s)/s, cosh): c u^2 gives sqrt(c) u whatever the sign
/// of u (so 2 sqrt(x^2/4) is x, not |x|), otherwise the principal y^(1/2).
fn even_sqrt<'h>(fx: &Fx<'_, '_, '_, 'h>, y: T<'h>) -> T<'h> {
    let half = fx.rat(1, 2);
    let (coeff, rest): (Rat, Vec<T<'h>>) = match fx.call_of(y, mc_api::names::MUL) {
        Some(args) => match args.first().and_then(|&c| fx.rat_of(c)) {
            Some(c) => (c, args[1..].to_vec()),
            None => (Rat::from_i64(1), args.to_vec()),
        },
        None => (Rat::from_i64(1), vec![y]),
    };
    // every factor an even power u^(2k): the root is the product of the u^k
    let roots: Option<Vec<T<'h>>> = rest
        .iter()
        .map(|&f| {
            let a = fx.call_of(f, mc_api::names::POW)?;
            let e = fx.int_of(*a.get(1)?)?;
            (e > 0 && e % 2 == 0).then(|| fx.pow(a[0], fx.int(e / 2)))
        })
        .collect();
    match roots {
        Some(rs) if !rs.is_empty() && coeff.cmp_value(&Rat::from_i64(0)) == std::cmp::Ordering::Greater => {
            let mut fs = vec![fx.pow(fx.q(&coeff), half)];
            fs.extend(rs);
            fx.mul(&fs)
        }
        _ => fx.pow(y, half),
    }
}

/// pFq(upper; lower; w) with equal parameters cancelled, as a named function where one is known (module comment).
fn named_pfq<'h>(fx: &mut Fx<'_, '_, '_, 'h>, mut upper: Vec<T<'h>>, mut lower: Vec<T<'h>>, w: T<'h>) -> R<'h> {
    'outer: loop {
        for i in 0..upper.len() {
            for j in 0..lower.len() {
                if upper[i] == lower[j] {
                    upper.remove(i);
                    lower.remove(j);
                    continue 'outer;
                }
            }
        }
        break;
    }
    let one = fx.int(1);
    let half = fx.rat(1, 2);
    let q = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| rat(fx, t);
    let r = |n: i64, d: i64| Rat::new(Int::from_i64(n), Int::from_i64(d));
    match (upper.as_slice(), lower.as_slice()) {
        ([], []) => return Ok(Some(fx.call("exp", &[w]))),
        ([a], []) => return Ok(Some(fx.pow(fx.sub(one, w), fx.neg(*a)))),
        ([], [c]) => {
            // 0F1(; c; -y) = Gamma(c) y^((1-c)/2) J_(c-1)(2 sqrt y); 0F1(; c; y) with I
            let (y, circ) = match fx.negated(w) {
                Some(y) => (y, true),
                None => (w, false),
            };
            let even = q(fx, *c) == Some(r(3, 2)) || q(fx, *c) == Some(r(1, 2));
            let sy = if even { even_sqrt(fx, y) } else { fx.pow(y, half) };
            let two_sy = fx.mul(&[fx.int(2), sy]);
            if q(fx, *c) == Some(r(3, 2)) {
                let f = fx.call(if circ { "sin" } else { "sinh" }, &[two_sy]);
                return Ok(Some(fx.div(f, two_sy)));
            }
            if q(fx, *c) == Some(r(1, 2)) {
                return Ok(Some(fx.call(if circ { "cos" } else { "cosh" }, &[two_sy])));
            }
            let nu = fx.sub(*c, one);
            let e = fx.mul(&[half, fx.sub(one, *c)]);
            let f = fx.call(if circ { "bessel_j" } else { "bessel_i" }, &[nu, two_sy]);
            return Ok(Some(fx.mul(&[fx.call("gamma", &[*c]), fx.pow(y, e), f])));
        }
        ([a], [c]) if q(fx, *a) == Some(r(1, 2)) && q(fx, *c) == Some(r(3, 2)) => {
            // 1F1(1/2; 3/2; -y) = sqrt(pi) erf(sqrt y)/(2 sqrt y), with erfi for +y
            let (y, f) = match fx.negated(w) {
                Some(y) => (y, "erf"),
                None => (w, "erfi"),
            };
            let sy = fx.eval(fx.pow(y, half))?;
            let sp = fx.pow(fx.sym("pi"), half);
            return Ok(Some(fx.div(fx.mul(&[sp, fx.call(f, &[sy])]), fx.mul(&[fx.int(2), sy]))));
        }
        ([a], [c]) if q(fx, *a) == Some(r(1, 2)) && fx.is_int(*c, 1) => {
            // 1F1(1/2; 1; w) = e^(w/2) I_0(w/2) (I_0 even)
            let hw = fx.mul(&[half, w]);
            let arg = fx.negated(hw).unwrap_or(hw);
            return Ok(Some(fx.mul(&[fx.call("exp", &[hw]), fx.call("bessel_i", &[fx.int(0), arg])])));
        }
        ([a, b], [c]) if fx.is_int(*c, 1) => {
            let (qa, qb) = (q(fx, *a), q(fx, *b));
            let h = Some(r(1, 2));
            let mh = Some(r(-1, 2));
            let two_pi = fx.div(fx.int(2), fx.sym("pi"));
            if qa == h && qb == h {
                return Ok(Some(fx.mul(&[two_pi, fx.call("elliptic_k", &[w])])));
            }
            if (qa == h && qb == mh) || (qa == mh && qb == h) {
                return Ok(Some(fx.mul(&[two_pi, fx.call("elliptic_e", &[w])])));
            }
        }
        _ => {}
    }
    Ok(Some(fx.call("hypergeometric_pfq", &[fx.list(&upper), fx.list(&lower), w])))
}

/// The definition of this module.
pub static DEFS: &[Def] = &[Def {
    name: "meijer_g",
    arities: &[3, 4],
    listable: false,
    exact: Some(rule),
    numeric: None,
    summary: "meijer_g([[a1, ..., an], [a(n+1), ..., ap]], [[b1, ..., bm], [b(m+1), ..., bq]], z): the Meijer G-function G^(m,n)_(p,q)(z); meijer_g(as, bs, z, r) is the generalized function G(z^(1/r))",
}];
