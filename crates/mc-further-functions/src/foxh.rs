//! Machine-number values of the Fox H function by its residue series.
//!
//! H^(m,n)_(p,q)(z) = 1/(2 pi i) int Theta(s) z^(-s) ds with Theta(s) = prod_(j<=m) G(b_j + B_j s) prod_(j<=n)
//! G(1 - a_j - A_j s) / (prod_(j>m) G(1 - b_j - B_j s) prod_(j>n) G(a_j + A_j s)) (Kilbas and Saigo, "H-Transforms",
//! 2004, 1.1-1.2; Mathai, Saxena, Haubold, "The H-function", 2010, 1.1). With mu = sum B_j - sum A_j and
//! delta = prod A_j^(-A_j) prod B_j^(B_j), the sum of the residues at the poles s = -(b_j + k)/B_j of the first
//! product converges for every z when mu > 0 (for |z| < delta when mu = 0); the sum at the poles s = (1 - a_j +
//! k)/A_j of the second converges for every z when mu < 0 (for |z| > delta when mu = 0) (Kilbas-Saigo, Thms. 1.2,
//! 1.3). The residues are (-1)^k/(k! B_j) and (-1)^k/(k! A_j) times the other factors (the second sum with the sign
//! of the clockwise contour already folded in).
//!
//! Poles of different gamma factors that coincide (or nearly) make a pole of higher order: then the native
//! declines and the library evaluates the perturbed series in ball arithmetic (machine numbers cannot carry the
//! cancellation of the 1/eps terms).
//! Logarithms of the gamma functions keep the large factors of high terms in range (Lanczos approximation, g = 7,
//! with the reflection formula on the left half plane).
use crate::num::{self, C};
use mc_core::T;
use mc_eval::{Native, NativeCx, Reply, Res};

/// ln Gamma(z) up to a multiple of 2 pi i (only its exponential is used).
pub(crate) fn lgamma(z: C) -> C {
    const G: f64 = 7.0;
    const COEF: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if z.0 < 0.5 {
        // Gamma(z) Gamma(1 - z) = pi/sin(pi z)
        // sin(pi z) = (-1)^k sin(pi (z - k)), k the nearest integer: no rounding of pi z near the poles
        let k = z.0.round();
        let f = z.0 - k;
        let s = csin((std::f64::consts::PI * f, std::f64::consts::PI * z.1));
        let s = if (k as i64) % 2 == 0 { s } else { (-s.0, -s.1) };
        let l = clog(s);
        let r = lgamma((1.0 - z.0, -z.1));
        return (std::f64::consts::PI.ln() - l.0 - r.0, -l.1 - r.1);
    }
    let z = (z.0 - 1.0, z.1);
    let mut x = (COEF[0], 0.0);
    for (i, &c) in COEF.iter().enumerate().skip(1) {
        x = num::add(x, num::div((c, 0.0), (z.0 + i as f64, z.1)));
    }
    let t = (z.0 + G + 0.5, z.1);
    // 0.5 ln(2 pi) + (z + 0.5) ln t - t + ln x
    let lt = clog(t);
    let a = num::mul((z.0 + 0.5, z.1), lt);
    let lx = clog(x);
    (0.5 * (2.0 * std::f64::consts::PI).ln() + a.0 - t.0 + lx.0, a.1 - t.1 + lx.1)
}

pub(crate) fn clog(z: C) -> C {
    (num::abs(z).ln(), z.1.atan2(z.0))
}

pub(crate) fn cexp(z: C) -> C {
    let m = z.0.exp();
    (m * z.1.cos(), m * z.1.sin())
}

fn csin(z: C) -> C {
    (z.0.sin() * z.1.cosh(), z.0.cos() * z.1.sinh())
}

/// Whether Gamma has a pole at z (a non-positive integer, up to rounding).
fn gamma_pole(z: C) -> bool {
    z.1.abs() < 1e-12 && z.0 <= 0.5 && (z.0 - z.0.round()).abs() < 1e-12
}

/// The parameters: (a_j, A_j) for j <= n, for j > n, (b_j, B_j) for j <= m, for j > m.
#[derive(Clone)]
pub(crate) struct Params {
    pub an: Vec<(C, f64)>,
    pub ap: Vec<(C, f64)>,
    pub bm: Vec<(C, f64)>,
    pub bq: Vec<(C, f64)>,
}

impl Params {
    fn all_a(&self) -> impl Iterator<Item = &(C, f64)> {
        self.an.iter().chain(self.ap.iter())
    }
    fn all_b(&self) -> impl Iterator<Item = &(C, f64)> {
        self.bm.iter().chain(self.bq.iter())
    }
    fn mu(&self) -> f64 {
        self.all_b().map(|b| b.1).sum::<f64>() - self.all_a().map(|a| a.1).sum::<f64>()
    }
    fn delta(&self) -> f64 {
        self.all_a().map(|a| -a.1 * a.1.ln()).sum::<f64>().exp() * self.all_b().map(|b| b.1 * b.1.ln()).sum::<f64>().exp()
    }

    /// ln of Theta(s) without the factor `skip` (true: of the first product of b's at index i, false: of the a's
    /// up to n at index i); None at a pole of another factor (a pole of higher order)
    fn log_theta(&self, s: C, skip_b: Option<usize>, skip_a: Option<usize>) -> Option<(C, bool)> {
        let mut acc = (0.0, 0.0);
        let mut zero = false;
        for (i, &(b, bb)) in self.bm.iter().enumerate() {
            if skip_b == Some(i) {
                continue;
            }
            let w = num::add(b, (bb * s.0, bb * s.1));
            if gamma_pole(w) {
                return None;
            }
            acc = num::add(acc, lgamma(w));
        }
        for (i, &(a, aa)) in self.an.iter().enumerate() {
            if skip_a == Some(i) {
                continue;
            }
            let w = num::sub((1.0, 0.0), num::add(a, (aa * s.0, aa * s.1)));
            if gamma_pole(w) {
                return None;
            }
            acc = num::add(acc, lgamma(w));
        }
        for &(b, bb) in &self.bq {
            let w = num::sub((1.0, 0.0), num::add(b, (bb * s.0, bb * s.1)));
            if gamma_pole(w) {
                zero = true;
            } else {
                acc = num::sub(acc, lgamma(w));
            }
        }
        for &(a, aa) in &self.ap {
            let w = num::add(a, (aa * s.0, aa * s.1));
            if gamma_pole(w) {
                zero = true;
            } else {
                acc = num::sub(acc, lgamma(w));
            }
        }
        Some((acc, zero))
    }

    /// The residue series at z; None when a pole of higher order is met.
    fn series(&self, z: C) -> Option<C> {
        let mu = self.mu();
        let use_b = if mu.abs() < 1e-14 { num::abs(z) < self.delta() } else { mu > 0.0 };
        let lz = clog(z);
        let mut sum = (0.0, 0.0);
        let mut big = 0.0f64;
        let count = if use_b { self.bm.len() } else { self.an.len() };
        if count == 0 {
            return Some((0.0, 0.0));
        }
        for j in 0..count {
            let (c, sc) = if use_b { self.bm[j] } else { self.an[j] };
            let mut small = 0;
            let mut lfact = 0.0f64;
            for k in 0..4000usize {
                if k > 0 {
                    lfact += (k as f64).ln();
                }
                let s = if use_b { (-(c.0 + k as f64) / sc, -c.1 / sc) } else { ((1.0 - c.0 + k as f64) / sc, -c.1 / sc) };
                let (lt, zero) = if use_b { self.log_theta(s, Some(j), None)? } else { self.log_theta(s, None, Some(j))? };
                let mut term = (0.0, 0.0);
                if !zero {
                    // (-1)^k/(k! sc) Theta' z^(-s)
                    let e = num::sub(lt, num::mul(s, lz));
                    let e = (e.0 - lfact - sc.ln(), e.1);
                    term = cexp(e);
                    if k % 2 == 1 {
                        term = (-term.0, -term.1);
                    }
                    if !(term.0.is_finite() && term.1.is_finite()) {
                        return None;
                    }
                }
                sum = num::add(sum, term);
                big = big.max(num::abs(term));
                if num::abs(term) <= 1e-18 * num::abs(sum) || (zero && k > 50) || num::abs(term) < 1e-300 {
                    small += 1;
                    if small > 4 && k > 8 {
                        break;
                    }
                } else {
                    small = 0;
                }
            }
        }
        // a sum that cancels more than 8 digits is left to ball arithmetic
        if big > 1e8 * num::abs(sum) {
            return None;
        }
        Some(sum)
    }

    /// H(z) when the poles of the summed factors are simple and the sum does not cancel much (else None: the caller
    /// uses ball arithmetic).
    pub(crate) fn value(&self, z: C) -> Option<C> {
        if !self.separated() {
            return None;
        }
        self.series(z)
    }

    /// Whether the poles of different factors of the summed product stay apart (in the first 200 of each).
    fn separated(&self) -> bool {
        let mu = self.mu();
        let set = if mu >= 0.0 { &self.bm } else { &self.an };
        for i in 0..set.len() {
            for j in (i + 1)..set.len() {
                let (c1, s1) = set[i];
                let (c2, s2) = set[j];
                for k in 0..200 {
                    // pole k of factor i equals a pole l of factor j when (c1 + k)/s1 - c2/s2 is a multiple of 1/s2
                    let t = ((c1.0 + k as f64) / s1 - c2.0 / s2) * s2;
                    if (c1.1 / s1 - c2.1 / s2).abs() < 1e-9 && t >= -1e-9 && (t - t.round()).abs() < 1e-7 {
                        return false;
                    }
                }
            }
        }
        true
    }
}

fn pairs<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<(C, f64)>>> {
    let Some(items) = cx.as_list(t).map(|l| l.to_vec()) else { return Ok(None) };
    let mut out = vec![];
    for it in items {
        let Some(v) = num::complexes_of(cx, it)? else { return Ok(None) };
        let [a, s] = v.as_slice() else { return Ok(None) };
        if s.1 != 0.0 || s.0 <= 0.0 {
            return Ok(None);
        }
        out.push((*a, s.0));
    }
    Ok(Some(out))
}

/// `c30_fox_machine(an, ap, bm, bq, z)`: H(z) in machine numbers, or unevaluated.
pub(crate) struct FoxMachine;

impl Native for FoxMachine {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [an, ap, bm, bq, z] = args else { return Ok(Reply::Decline) };
        let (Some(an), Some(ap), Some(bm), Some(bq)) = (pairs(cx, *an)?, pairs(cx, *ap)?, pairs(cx, *bm)?, pairs(cx, *bq)?) else { return Ok(Reply::Decline) };
        let Some(zv) = num::complex_of(cx, *z)? else { return Ok(Reply::Decline) };
        cx.tick(1)?;
        let p = Params { an, ap, bm, bq };
        let real = zv.1 == 0.0 && zv.0 > 0.0 && p.all_a().chain(p.all_b()).all(|x| x.0 .1 == 0.0);
        let Some(v) = p.value(zv) else { return Ok(Reply::Decline) };
        if !(v.0.is_finite() && v.1.is_finite()) {
            return Ok(Reply::Decline);
        }
        Ok(Reply::Done(num::term(h, if real { (v.0, 0.0) } else { v })))
    }
}
