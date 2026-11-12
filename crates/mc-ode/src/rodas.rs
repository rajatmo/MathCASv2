//! The Rosenbrock method RODAS of Hairer and Wanner (*Solving ODEs II*, VI.4 and the code RODAS): six stages,
//! order 4 with an embedded method of order 3, stiffly accurate and L-stable, with a continuous extension of
//! order 3 (decision N3.6). Each step evaluates the Jacobian `J = dF/dY` (and `dF/dt` for non-autonomous
//! systems) once, factors `W = I/(h γ) - J` once and solves six linear systems with it:
//!
//! ```text
//! W k_i = F(t + c_i h, y + sum_j a_ij k_j) + sum_j (c_ij / h) k_j + h d_i dF/dt,   y_new = y + ... + k5 + k6
//! ```
//!
//! The error estimate is k6 (the last stage corrects the embedded solution).
use crate::dense;
use crate::lu::Lu;
use crate::model::Model;
use crate::options::Tolerance;
use crate::step::{error_norm, Attempt, Stepper};
use mc_eval::{NativeCx, Res};

const GAMMA: f64 = 0.25;
const C2: f64 = 0.386;
const C3: f64 = 0.21;
const C4: f64 = 0.63;
const D1: f64 = 0.25;
const D2: f64 = -0.1043;
const D3: f64 = 0.1035;
const D4: f64 = -0.036_200_000_000_000_23;
const A21: f64 = 1.544;
const A31: f64 = 0.946_678_528_081_582_6;
const A32: f64 = 0.255_701_169_898_328_4;
const A41: f64 = 3.314_825_187_068_521;
const A42: f64 = 2.896_124_015_972_201;
const A43: f64 = 0.998_641_913_997_781_7;
const A51: f64 = 1.221_224_509_226_641;
const A52: f64 = 6.019_134_481_288_629;
const A53: f64 = 12.537_083_329_320_87;
const A54: f64 = -0.687_886_036_105_895;
const C21: f64 = -5.6688;
const C31: f64 = -2.430_093_356_833_875;
const C32: f64 = -0.206_359_915_709_191_5;
const C41: f64 = -0.107_352_905_815_137_5;
const C42: f64 = -9.594_562_251_023_355;
const C43: f64 = -20.470_286_148_096_16;
const C51: f64 = 7.496_443_313_967_647;
const C52: f64 = -10.246_804_314_643_52;
const C53: f64 = -33.999_903_528_199_05;
const C54: f64 = 11.708_908_932_061_6;
const C61: f64 = 8.083_246_795_921_522;
const C62: f64 = -7.981_132_988_064_893;
const C63: f64 = -31.521_594_328_743_71;
const C64: f64 = 16.319_305_431_231_36;
const C65: f64 = -6.058_818_238_834_054;
/// The continuous extension: `y(θ) = (1-θ) y0 + θ (y1 + (1-θ) (k'1 + θ k'2))` with `k'1 = sum H2j k_j`,
/// `k'2 = sum H3j k_j`.
const H2: [f64; 5] = [10.126_235_083_445_86, -7.487_995_877_610_167, -34.800_918_615_557_47, -7.992_771_707_568_823, 1.025_137_723_295_662];
const H3: [f64; 5] = [-0.676_280_339_280_125_3, 6.087_714_651_680_015, 16.430_843_208_924_78, 24.767_225_114_183_86, -6.594_389_125_716_872];

/// The step size control: safety factor and bounds of `h_new / h`.
const SAFE: f64 = 0.9;
const GROW: f64 = 6.0;
const SHRINK: f64 = 0.2;

/// Coefficients of a combination of stages: `(stage, coefficient)`.
type Combination = &'static [(usize, f64)];

/// The state of the method between steps.
pub(crate) struct Rodas {
    n: usize,
    /// F, the Jacobian and dF/dt at the start of the step
    f0: Vec<f64>,
    jac: Vec<f64>,
    ft: Vec<f64>,
    /// whether `jac` and `ft` belong to the current point (a rejected step keeps them)
    fresh: bool,
    w: Vec<f64>,
    lu: Lu,
    k: [Vec<f64>; 6],
    u: Vec<f64>,
    du: Vec<f64>,
    rejected: bool,
}

impl Rodas {
    /// The method for a state of size `n`.
    pub fn new(n: usize) -> Rodas {
        let z = || vec![0.0; n];
        Rodas {
            n,
            f0: z(),
            jac: vec![0.0; n * n],
            ft: z(),
            fresh: false,
            w: vec![0.0; n * n],
            lu: Lu::new(n),
            k: [z(), z(), z(), z(), z(), z()],
            u: z(),
            du: z(),
            rejected: false,
        }
    }

    /// `u = y + sum a_j k_j`.
    fn combine(&mut self, y: &[f64], a: &[(usize, f64)]) {
        for (i, (out, &yi)) in self.u.iter_mut().zip(y).enumerate() {
            *out = yi + a.iter().map(|&(j, c)| c * self.k[j][i]).sum::<f64>();
        }
    }

    /// Stage `s`: solve `W k_s = du + sum (c_j / h) k_j + h d dF/dt`.
    fn solve(&mut self, s: usize, h: f64, c: &[(usize, f64)], d: f64) {
        let mut ks = std::mem::take(&mut self.k[s]);
        for (i, v) in ks.iter_mut().enumerate() {
            *v = self.du[i] + c.iter().map(|&(j, cj)| cj / h * self.k[j][i]).sum::<f64>() + h * d * self.ft[i];
        }
        self.lu.solve(&mut ks);
        self.k[s] = ks;
    }
}

impl<'h> Stepper<'h> for Rodas {
    fn start(&mut self, cx: &mut NativeCx<'_, '_, 'h>, m: &mut Model<'h>, t: f64, y: &[f64]) -> Res<bool> {
        self.fresh = false;
        let mut f0 = std::mem::take(&mut self.f0);
        let ok = m.f(cx, t, y, &mut f0)?;
        self.f0 = f0;
        Ok(ok)
    }

    fn attempt(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        m: &mut Model<'h>,
        t: f64,
        y: &[f64],
        h: f64,
        tol: &Tolerance,
    ) -> Res<Attempt> {
        let n = self.n;
        if !self.fresh {
            let (mut jac, mut ft) = (std::mem::take(&mut self.jac), std::mem::take(&mut self.ft));
            let ok = m.jacobian(cx, t, y, &self.f0, &mut jac, &mut ft)?;
            (self.jac, self.ft) = (jac, ft);
            if !ok {
                return Ok(Attempt::NotNumeric(t, y.to_vec()));
            }
            self.fresh = true;
        }
        let diag = 1.0 / (h * GAMMA);
        for (wi, ji) in self.w.iter_mut().zip(&self.jac) {
            *wi = -ji;
        }
        for i in 0..n {
            self.w[i * n + i] += diag;
        }
        if !self.lu.factor(&self.w) {
            self.rejected = true;
            return Ok(Attempt::Rejected { h_next: h * 0.5 });
        }
        self.du.copy_from_slice(&self.f0);
        self.solve(0, h, &[], D1);
        // stages 2-4 at intermediate points, 5 and 6 at the end of the step
        let stages: [(f64, Combination, Combination, f64); 3] = [
            (C2, &[(0, A21)], &[(0, C21)], D2),
            (C3, &[(0, A31), (1, A32)], &[(0, C31), (1, C32)], D3),
            (C4, &[(0, A41), (1, A42), (2, A43)], &[(0, C41), (1, C42), (2, C43)], D4),
        ];
        for (s, (c, a, cc, d)) in stages.iter().enumerate() {
            self.combine(y, a);
            let mut du = std::mem::take(&mut self.du);
            let ok = m.f(cx, t + c * h, &self.u, &mut du)?;
            self.du = du;
            if !ok {
                return Ok(Attempt::NotNumeric(t + c * h, self.u.clone()));
            }
            self.solve(s + 1, h, cc, *d);
        }
        self.combine(y, &[(0, A51), (1, A52), (2, A53), (3, A54)]);
        for (s, cc) in [(4, &[(0, C51), (1, C52), (2, C53), (3, C54)][..]), (5, &[(0, C61), (1, C62), (2, C63), (3, C64), (4, C65)][..])] {
            if s == 5 {
                for i in 0..n {
                    self.u[i] += self.k[4][i];
                }
            }
            let mut du = std::mem::take(&mut self.du);
            let ok = m.f(cx, t + h, &self.u, &mut du)?;
            self.du = du;
            if !ok {
                return Ok(Attempt::NotNumeric(t + h, self.u.clone()));
            }
            self.solve(s, h, cc, 0.0);
        }
        for i in 0..n {
            self.u[i] += self.k[5][i];
        }
        let err = error_norm(&self.k[5], y, &self.u, tol);
        if !err.is_finite() {
            self.rejected = true;
            return Ok(Attempt::Rejected { h_next: h * SHRINK });
        }
        let fac = (err.powf(0.25) / SAFE).clamp(1.0 / GROW, 1.0 / SHRINK);
        if err > 1.0 {
            self.rejected = true;
            return Ok(Attempt::Rejected { h_next: h / fac });
        }
        let mut h_next = h / fac;
        if self.rejected {
            h_next = h_next.signum() * h_next.abs().min(h.abs());
        }
        self.rejected = false;
        Ok(Attempt::Accepted { h_next, stiff: true })
    }

    fn accept(&mut self, y: &mut [f64], _h: f64, dense: &mut Vec<f64>) {
        let n = self.n;
        let k = &self.k;
        let r1: Vec<f64> = (0..n).map(|i| self.u[i] - y[i]).collect();
        let r2: Vec<f64> = (0..n).map(|i| (0..5).map(|j| H2[j] * k[j][i]).sum()).collect();
        let r3: Vec<f64> = (0..n).map(|i| (0..5).map(|j| H3[j] * k[j][i]).sum()).collect();
        let r4 = vec![0.0; n];
        dense::monomials(y, [&r1, &r2, &r3, &r4], dense);
        y.copy_from_slice(&self.u);
    }

    fn f0(&self) -> &[f64] {
        &self.f0
    }

    fn order(&self) -> i32 {
        4
    }
}
