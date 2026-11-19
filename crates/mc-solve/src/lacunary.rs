//! Roots of a lacunary polynomial of huge degree: `c x^n + q(x) == 0` with integer coefficients, n beyond
//! [`HUGE_DEGREE`] and q of small degree d (`x^1234567 + 5 x + 1`). No polynomial of degree n is built.
//!
//! - **How many real roots**: Descartes' rule of signs bounds the positive roots by the sign changes V+ of the
//!   coefficients and the negative ones by those V- of p(-x), with the parity of the bound (Descartes 1637;
//!   Prasolov, *Polynomials*, 1.2). When as many distinct roots of each sign are proved as the bound allows,
//!   every real root is found; otherwise nothing is answered.
//! - **Where they are**: inside the unit circle x^n is negligible once |x| < 1 - O(1/n), so the roots there lie
//!   near the roots of q; near |x| = 1 the remaining n - d roots solve x = w (-q(x)/c)^(1/n) for the n-th roots of
//!   unity w, a contraction (its derivative is q'/(n q), tiny) iterated from w (Rouché's theorem on the two circles
//!   gives the counts; Marden, *Geometry of Polynomials*, 27). The roots of q come from Aberth's iteration in
//!   machine numbers; these are starting points only.
//! - **The proof**: each candidate becomes a root object `root([&(p(&1)), x0])` only when Krawczyk's test proves a
//!   single simple root in a small box around it (`mc_algebra::transroot::object_near`; the power is evaluated on
//!   balls by squaring). Exact roots 0, 1 and -1 are tested exactly and given as integers.
//! - **Complex roots** are listed only under `max_roots: k`: the real roots, then roots of q inside the circle, then
//!   the roots near the n-th roots of unity in the order w = exp(2 pi i j/n), j = 1, -1, 2, -2, ...
use mc_algebra::transroot::{object_near, HUGE_DEGREE};
use mc_api::{names, Store, View, T};
use mc_arith::num::Int;
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The largest degree of the low part q.
const LOW_DEGREE: u64 = 400;

/// The equation's polynomial: exponents with machine coefficients, the integer ones, the exact roots.
struct Lacunary {
    /// (exponent, coefficient) in increasing exponent, the factor x^m divided out
    terms: Vec<(u64, f64)>,
    ints: Vec<(u64, Int)>,
    /// the multiplicity of the root 0
    zero: u64,
}

impl Lacunary {
    fn top(&self) -> (u64, f64) {
        *self.terms.last().expect("terms")
    }
    /// q(z) = p(z) minus the top term, in complex machine numbers.
    fn low(&self, z: C) -> C {
        let mut s = C(0.0, 0.0);
        for &(e, c) in &self.terms[..self.terms.len() - 1] {
            s = s.add(z.powu(e).scale(c));
        }
        s
    }
    /// Sign changes of the coefficients of p(s x), s = 1 or -1.
    fn sign_changes(&self, s: i64) -> usize {
        let signs: Vec<bool> = self.ints.iter().map(|(e, c)| c.neg ^ (s < 0 && e % 2 == 1)).collect();
        signs.windows(2).filter(|w| w[0] != w[1]).count()
    }
    /// p(s) exactly for s = 1 or -1.
    fn exact_at(&self, s: i64) -> bool {
        let mut acc = Int::from_i64(0);
        for (e, c) in &self.ints {
            acc = if s < 0 && e % 2 == 1 { acc.add(&c.neg()) } else { acc.add(c) };
        }
        acc.is_zero()
    }
}

/// A complex machine number.
#[derive(Clone, Copy, Debug)]
struct C(f64, f64);

impl C {
    fn add(self, o: C) -> C {
        C(self.0 + o.0, self.1 + o.1)
    }
    fn sub(self, o: C) -> C {
        C(self.0 - o.0, self.1 - o.1)
    }
    fn mul(self, o: C) -> C {
        C(self.0 * o.0 - self.1 * o.1, self.0 * o.1 + self.1 * o.0)
    }
    fn scale(self, k: f64) -> C {
        C(self.0 * k, self.1 * k)
    }
    fn div(self, o: C) -> C {
        let d = o.0 * o.0 + o.1 * o.1;
        C((self.0 * o.0 + self.1 * o.1) / d, (self.1 * o.0 - self.0 * o.1) / d)
    }
    fn abs(self) -> f64 {
        self.0.hypot(self.1)
    }
    fn powu(self, e: u64) -> C {
        let mut r = C(1.0, 0.0);
        let mut b = self;
        let mut k = e;
        while k > 0 {
            if k & 1 == 1 {
                r = r.mul(b);
            }
            b = b.mul(b);
            k >>= 1;
        }
        r
    }
}

/// The roots of the polynomial with coefficients `c` (index = degree, top nonzero) by Aberth's iteration.
fn aberth(c: &[f64]) -> Vec<C> {
    let d = c.len() - 1;
    if d == 0 {
        return vec![];
    }
    let lead = c[d];
    let eval = |z: C| {
        let mut p = C(0.0, 0.0);
        let mut dp = C(0.0, 0.0);
        for &a in c.iter().rev() {
            dp = dp.mul(z).add(p);
            p = p.mul(z).add(C(a, 0.0));
        }
        (p, dp)
    };
    // starting points on a circle of the Cauchy bound's radius
    let r = 1.0 + c[..d].iter().map(|a| (a / lead).abs()).fold(0.0, f64::max);
    let mut z: Vec<C> = (0..d)
        .map(|k| {
            let t = 2.0 * std::f64::consts::PI * (k as f64 + 0.25) / d as f64;
            C(0.5 * r * t.cos(), 0.5 * r * t.sin())
        })
        .collect();
    for _ in 0..500 {
        let mut moved = 0.0f64;
        for i in 0..d {
            let (p, dp) = eval(z[i]);
            if p.abs() == 0.0 {
                continue;
            }
            let ratio = p.div(dp);
            let mut s = C(0.0, 0.0);
            for (j, &zj) in z.iter().enumerate() {
                if j != i {
                    s = s.add(C(1.0, 0.0).div(z[i].sub(zj)));
                }
            }
            let w = ratio.div(C(1.0, 0.0).sub(ratio.mul(s)));
            z[i] = z[i].sub(w);
            moved = moved.max(w.abs() / z[i].abs().max(1e-300));
        }
        if moved < 1e-15 {
            break;
        }
    }
    z
}

/// The root near the n-th root of unity of index j: the fixed point of x = exp((log(-q(x)/c) + 2 pi i j)/n).
fn outer_root(p: &Lacunary, j: i64) -> Option<C> {
    let (n, c) = p.top();
    let nf = n as f64;
    let t = 2.0 * std::f64::consts::PI * j as f64 / nf;
    let mut x = C(t.cos(), t.sin());
    for _ in 0..60 {
        let g = p.low(x).scale(-1.0 / c);
        if g.abs() == 0.0 || !g.abs().is_finite() {
            return None;
        }
        let (lr, th) = (g.abs().ln(), g.1.atan2(g.0));
        let (m, a) = ((lr / nf).exp(), (th + 2.0 * std::f64::consts::PI * j as f64) / nf);
        let nx = C(m * a.cos(), m * a.sin());
        let step = nx.sub(x).abs();
        x = nx;
        if step < 1e-17 {
            break;
        }
    }
    Some(x)
}

/// The real root of sign s (1 or -1) near |x| = 1: x = s t, t = (-q(s t)/(c s^n))^(1/n) when that is positive.
fn outer_real(p: &Lacunary, s: f64) -> Option<f64> {
    let (n, c) = p.top();
    let sn = if n % 2 == 1 { s } else { 1.0 };
    let mut t = 1.0f64;
    for _ in 0..60 {
        let g = -p.low(C(s * t, 0.0)).0 / (c * sn);
        if g.is_nan() || g <= 0.0 {
            return None;
        }
        let nt = (g.ln() / n as f64).exp();
        let step = (nt - t).abs();
        t = nt;
        if step < 1e-17 {
            break;
        }
    }
    Some(s * t)
}

/// The polynomial of `f` in `x` when it is lacunary of huge degree (one huge power, the rest of degree at most
/// [`LOW_DEGREE`], integer coefficients that fit a machine integer).
fn read<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, x: T<'h>) -> Res<Option<Lacunary>> {
    let Some(mut ts) = crate::domain::sparse_terms(cx, f, x)? else { return Ok(None) };
    ts.retain(|(_, c)| !c.is_zero());
    ts.sort_by_key(|t| t.0);
    if ts.len() < 2 {
        return Ok(None);
    }
    let n = ts[ts.len() - 1].0;
    let d = ts[ts.len() - 2].0;
    if n <= HUGE_DEGREE as u64 || d > LOW_DEGREE {
        return Ok(None);
    }
    let zero = ts[0].0;
    let ints: Vec<(u64, Int)> = ts.iter().map(|(e, c)| (e - zero, c.clone())).collect();
    let mut terms = vec![];
    for (e, c) in &ints {
        let Some(v) = c.to_i64() else { return Ok(None) };
        terms.push((*e, v as f64));
    }
    Ok(Some(Lacunary { terms, ints, zero }))
}

/// A proved root: its value term and its approximate value.
struct Found<'h> {
    v: T<'h>,
    at: C,
}

/// Every real root of `f` (a lacunary polynomial of huge degree in x), proved complete by Descartes' bounds, in
/// increasing order; with `extra` > 0 also that many complex roots. `None` when the shape does not fit or the
/// count is not proved.
fn roots<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>, x: T<'h>, extra: usize) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    let Some(p) = read(cx, f, x)? else { return Ok(None) };
    let slot = h.sym_term(h.sym("&1"));
    // the body of the root objects: p(&1) without the factor &1^m
    let mut body_terms = vec![];
    for (e, c) in &p.ints {
        let coef = mc_arith::Arith::new(h).integer(c);
        body_terms.push(h.call(h.sym(names::MUL), &[coef, h.call(h.sym(names::POW), &[slot, h.int(*e as i64)])]));
    }
    let body = cx.eval(h.call(h.sym(names::ADD), &body_terms))?;
    let mut found: Vec<Found<'h>> = vec![];
    let add = |cx: &mut NativeCx<'_, '_, 'h>, found: &mut Vec<Found<'h>>, z: C, real: bool| -> Res<bool> {
        // the box: well inside the spacing 2 pi/n of the roots near the circle and the precision of z
        let rad = 1e-9 * z.abs().max(1e-3);
        let Some(v) = object_near(cx, stop, body, slot, z.0, z.1, rad, real)? else { return Ok(false) };
        if found.iter().any(|g| g.v == v) {
            return Ok(true);
        }
        found.push(Found { v, at: z });
        Ok(true)
    };
    // candidates: exact 1 and -1, the roots of q inside the circle, the two real roots near the circle
    let low_deg = p.terms[p.terms.len() - 2].0 as usize;
    let mut qc = vec![0.0; low_deg + 1];
    for &(e, c) in &p.terms[..p.terms.len() - 1] {
        qc[e as usize] = c;
    }
    let inner: Vec<C> = aberth(&qc).into_iter().filter(|z| z.abs() < 0.999 && z.abs() > 0.0).collect();
    let mut exact = vec![];
    for s in [-1i64, 1] {
        if p.exact_at(s) {
            exact.push(s);
        }
    }
    let mut real_cands: Vec<f64> = inner.iter().filter(|z| z.1.abs() <= 1e-9 * z.0.abs().max(1e-300)).map(|z| z.0).collect();
    real_cands.extend(outer_real(&p, 1.0));
    real_cands.extend(outer_real(&p, -1.0));
    let mut real_found: Vec<(f64, T<'h>)> = exact.iter().map(|&s| (s as f64, h.int(s))).collect();
    for r in real_cands {
        if exact.iter().any(|&s| (r - s as f64).abs() < 1e-6) {
            continue;
        }
        let before = found.len();
        add(cx, &mut found, C(r, 0.0), true)?;
        if found.len() > before {
            let g = &found[found.len() - 1];
            real_found.push((g.at.0, g.v));
        }
    }
    // complete: as many roots of each sign as Descartes' bound
    let pos = real_found.iter().filter(|r| r.0 > 0.0).count();
    let neg = real_found.iter().filter(|r| r.0 < 0.0).count();
    if pos != p.sign_changes(1) || neg != p.sign_changes(-1) {
        return Ok(None);
    }
    real_found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<T<'h>> = vec![];
    let mut zero_done = p.zero == 0;
    for (v, t) in &real_found {
        if !zero_done && *v > 0.0 {
            out.push(h.int(0));
            zero_done = true;
        }
        out.push(*t);
    }
    if !zero_done {
        out.push(h.int(0));
    }
    // complex roots on request
    if extra > 0 {
        let mut want = extra;
        let mut cands: Vec<C> = inner.iter().filter(|z| z.1.abs() > 1e-9 * z.0.abs().max(1e-300)).copied().collect();
        let mut j = 1i64;
        while cands.len() < 4 * extra + 8 && j <= 4 * extra as i64 + 8 {
            for s in [j, -j] {
                if let Some(z) = outer_root(&p, s) {
                    cands.push(z);
                }
            }
            j += 1;
        }
        let base = found.len();
        for z in cands {
            if want == 0 {
                break;
            }
            let n0 = found.len();
            add(cx, &mut found, z, false)?;
            if found.len() > n0 {
                want -= 1;
            }
        }
        out.extend(found[base..].iter().map(|g| g.v));
    }
    Ok(Some(out))
}

/// `reduce(p == 0, x, reals)` and `solve(p == 0, x, reals)` for a lacunary polynomial of huge degree, and
/// `solve(p == 0, x, max_roots: k)` (the real roots, then complex ones up to k in all). `None` for another form.
pub(crate) fn huge_roots<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    stop: &Arc<AtomicBool>,
    reduce: bool,
    args: &[T<'h>],
    max_roots: Option<usize>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let (eq, x, reals) = match args {
        [eq, x] => (*eq, *x, false),
        [eq, x, d] if matches!(h.view(*d), View::Sym(s) if h.sym_name(s) == "reals") => (*eq, *x, true),
        _ => return Ok(None),
    };
    if !matches!(h.view(x), View::Sym(_)) {
        return Ok(None);
    }
    let View::App { head, args: &[l, r] } = h.view(eq) else { return Ok(None) };
    if head != h.sym_term(h.sym("equal")) {
        return Ok(None);
    }
    let extra = match (reals, max_roots) {
        (true, _) => 0,
        (false, Some(k)) if !reduce => k,
        _ => return Ok(None),
    };
    let f = cx.eval(h.call(h.sym(names::ADD), &[l, h.call(h.sym(names::MUL), &[h.int(-1), r])]))?;
    let Some(mut vs) = roots(cx, stop, f, x, extra)? else { return Ok(None) };
    if let Some(k) = max_roots {
        vs.truncate(k);
    }
    Ok(Some(if reduce {
        let eqs: Vec<T<'h>> = vs.iter().map(|&v| h.call(h.sym("equal"), &[x, v])).collect();
        match eqs.len() {
            0 => h.sym_term(h.sym("false")),
            1 => eqs[0],
            _ => h.call(h.sym("or"), &eqs),
        }
    } else {
        let rules: Vec<T<'h>> = vs.iter().map(|&v| h.call(h.sym(names::LIST), &[h.call(h.sym(names::RULE), &[x, v])])).collect();
        h.call(h.sym(names::LIST), &rules)
    }))
}
