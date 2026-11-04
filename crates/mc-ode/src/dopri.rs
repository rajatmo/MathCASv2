//! The explicit Runge–Kutta method of Dormand and Prince of order 5 with an embedded method of order 4 (Hairer,
//! Nørsett, Wanner, *Solving ODEs I*, II.5 and the code DOPRI5): first same as last, step size control with Lund
//! stabilisation, the continuous extension of order 4, and the stiffness test of DOPRI5 (decision N3.5).
use crate::dense;
use crate::model::Model;
use crate::options::Tolerance;
use crate::step::{error_norm, Attempt, Stepper};
use mc_eval::{NativeCx, Res};

const C2: f64 = 1.0 / 5.0;
const C3: f64 = 3.0 / 10.0;
const C4: f64 = 4.0 / 5.0;
const C5: f64 = 8.0 / 9.0;
const A21: f64 = 1.0 / 5.0;
const A31: f64 = 3.0 / 40.0;
const A32: f64 = 9.0 / 40.0;
const A41: f64 = 44.0 / 45.0;
const A42: f64 = -56.0 / 15.0;
const A43: f64 = 32.0 / 9.0;
const A51: f64 = 19372.0 / 6561.0;
const A52: f64 = -25360.0 / 2187.0;
const A53: f64 = 64448.0 / 6561.0;
const A54: f64 = -212.0 / 729.0;
const A61: f64 = 9017.0 / 3168.0;
const A62: f64 = -355.0 / 33.0;
const A63: f64 = 46732.0 / 5247.0;
const A64: f64 = 49.0 / 176.0;
const A65: f64 = -5103.0 / 18656.0;
const A71: f64 = 35.0 / 384.0;
const A73: f64 = 500.0 / 1113.0;
const A74: f64 = 125.0 / 192.0;
const A75: f64 = -2187.0 / 6784.0;
const A76: f64 = 11.0 / 84.0;
const E1: f64 = 71.0 / 57600.0;
const E3: f64 = -71.0 / 16695.0;
const E4: f64 = 71.0 / 1920.0;
const E5: f64 = -17253.0 / 339200.0;
const E6: f64 = 22.0 / 525.0;
const E7: f64 = -1.0 / 40.0;
const D1: f64 = -12715105075.0 / 11282082432.0;
const D3: f64 = 87487479700.0 / 32700410799.0;
const D4: f64 = -10690763975.0 / 1880347072.0;
const D5: f64 = 701980252875.0 / 199316789632.0;
const D6: f64 = -1453857185.0 / 822651844.0;
const D7: f64 = 69997945.0 / 29380423.0;

/// The step size control: `beta` of the Lund stabilisation, the exponent `1/5 - 0.75 beta`, safety factor and the
/// bounds of `h_new / h`.
const BETA: f64 = 0.04;
const EXPO: f64 = 0.2 - BETA * 0.75;
const SAFE: f64 = 0.9;
const GROW: f64 = 10.0;
const SHRINK: f64 = 0.2;

/// The stiffness test: `h λ` beyond the stability boundary for `STIFF_STEPS` accepted steps signals stiffness;
/// `NONSTIFF_STEPS` steps within it reset the count.
const STABILITY: f64 = 3.25;
const STIFF_STEPS: u32 = 15;
const NONSTIFF_STEPS: u32 = 6;

/// The state of the method between steps.
pub(crate) struct Dopri {
    n: usize,
    /// the stages k1..k7 (k1 = F(t, y) at the start, k7 = F at the new point)
    k: [Vec<f64>; 7],
    ynew: Vec<f64>,
    ystage: Vec<f64>,
    err: Vec<f64>,
    /// whether k1 holds F at the current point
    fsal: bool,
    facold: f64,
    rejected: bool,
    stiff_count: u32,
    nonstiff_count: u32,
}

impl Dopri {
    /// The method for a state of size `n`.
    pub fn new(n: usize) -> Dopri {
        let z = || vec![0.0; n];
        Dopri {
            n,
            k: [z(), z(), z(), z(), z(), z(), z()],
            ynew: z(),
            ystage: z(),
            err: z(),
            fsal: false,
            facold: 1e-4,
            rejected: false,
            stiff_count: 0,
            nonstiff_count: 0,
        }
    }

    /// `ystage = y + h * sum(a_j k_j)`.
    fn stage(&mut self, y: &[f64], h: f64, a: &[(usize, f64)]) {
        for i in 0..self.n {
            let s: f64 = a.iter().map(|&(j, c)| c * self.k[j][i]).sum();
            self.ystage[i] = y[i] + h * s;
        }
    }

    /// Evaluate stage `j` at `t` from `ystage`.
    fn eval<'h>(&mut self, cx: &mut NativeCx<'_, '_, 'h>, m: &mut Model<'h>, t: f64, j: usize) -> Res<bool> {
        let mut kj = std::mem::take(&mut self.k[j]);
        let ok = m.f(cx, t, &self.ystage, &mut kj)?;
        self.k[j] = kj;
        Ok(ok)
    }
}

impl<'h> Stepper<'h> for Dopri {
    fn start(&mut self, cx: &mut NativeCx<'_, '_, 'h>, m: &mut Model<'h>, t: f64, y: &[f64]) -> Res<bool> {
        if self.fsal {
            return Ok(true);
        }
        let mut k1 = std::mem::take(&mut self.k[0]);
        let ok = m.f(cx, t, y, &mut k1)?;
        self.k[0] = k1;
        self.fsal = ok;
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
        let stages: [(f64, &[(usize, f64)]); 5] = [
            (C2, &[(0, A21)]),
            (C3, &[(0, A31), (1, A32)]),
            (C4, &[(0, A41), (1, A42), (2, A43)]),
            (C5, &[(0, A51), (1, A52), (2, A53), (3, A54)]),
            (1.0, &[(0, A61), (1, A62), (2, A63), (3, A64), (4, A65)]),
        ];
        for (j, (c, a)) in stages.iter().enumerate() {
            self.stage(y, h, a);
            if !self.eval(cx, m, t + c * h, j + 1)? {
                return Ok(Attempt::NotNumeric(t + c * h, self.ystage.clone()));
            }
        }
        // the solution of order 5 and F there (the first stage of the next step)
        let ysti = self.ystage.clone();
        self.stage(y, h, &[(0, A71), (2, A73), (3, A74), (4, A75), (5, A76)]);
        if !self.eval(cx, m, t + h, 6)? {
            return Ok(Attempt::NotNumeric(t + h, self.ystage.clone()));
        }
        std::mem::swap(&mut self.ynew, &mut self.ystage);
        self.ystage = ysti;
        for i in 0..self.n {
            let k = &self.k;
            self.err[i] = h * (E1 * k[0][i] + E3 * k[2][i] + E4 * k[3][i] + E5 * k[4][i] + E6 * k[5][i] + E7 * k[6][i]);
        }
        let err = error_norm(&self.err, y, &self.ynew, tol);
        if !err.is_finite() {
            self.rejected = true;
            return Ok(Attempt::Rejected { h_next: h * SHRINK });
        }
        let fac11 = err.powf(EXPO);
        // Lund stabilisation
        let fac = (fac11 / self.facold.powf(BETA) / SAFE).clamp(1.0 / GROW, 1.0 / SHRINK);
        if err > 1.0 {
            self.rejected = true;
            return Ok(Attempt::Rejected { h_next: h / (fac11 / SAFE).min(1.0 / SHRINK) });
        }
        self.facold = err.max(1e-4);
        let mut h_next = h / fac;
        if self.rejected {
            h_next = h_next.signum() * h_next.abs().min(h.abs());
        }
        self.rejected = false;
        // the stiffness test: h times the estimate |k7 - k6| / |ynew - ysti| of the dominant eigenvalue
        let (mut num, mut den) = (0.0, 0.0);
        for i in 0..self.n {
            num += (self.k[6][i] - self.k[5][i]).powi(2);
            den += (self.ynew[i] - self.ystage[i]).powi(2);
        }
        let mut stiff = false;
        if den > 0.0 && (h * (num / den).sqrt()).abs() > STABILITY {
            self.nonstiff_count = 0;
            self.stiff_count += 1;
            stiff = self.stiff_count >= STIFF_STEPS;
        } else {
            self.nonstiff_count += 1;
            if self.nonstiff_count >= NONSTIFF_STEPS {
                self.stiff_count = 0;
            }
        }
        Ok(Attempt::Accepted { h_next, stiff })
    }

    fn accept(&mut self, y: &mut [f64], h: f64, dense: &mut Vec<f64>) {
        let n = self.n;
        let k = &self.k;
        let r1: Vec<f64> = (0..n).map(|i| self.ynew[i] - y[i]).collect();
        let r2: Vec<f64> = (0..n).map(|i| h * k[0][i] - r1[i]).collect();
        let r3: Vec<f64> = (0..n).map(|i| r1[i] - h * k[6][i] - r2[i]).collect();
        let r4: Vec<f64> = (0..n)
            .map(|i| h * (D1 * k[0][i] + D3 * k[2][i] + D4 * k[3][i] + D5 * k[4][i] + D6 * k[5][i] + D7 * k[6][i]))
            .collect();
        dense::monomials(y, [&r1, &r2, &r3, &r4], dense);
        y.copy_from_slice(&self.ynew);
        // first same as last
        self.k.swap(0, 6);
        self.fsal = true;
    }

    fn f0(&self) -> &[f64] {
        &self.k[0]
    }

    fn order(&self) -> i32 {
        5
    }
}
