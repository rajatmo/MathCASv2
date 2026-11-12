//! Constrained nonlinear minimisation in machine reals: the local solver of `find_minimum` with constraints and
//! the global search of `n_minimize` (decisions R.21 to R.24).
//!
//! - **Local:** the augmented Lagrangian method (Powell 1969, Hestenes 1969, Rockafellar 1973 for inequalities;
//!   Nocedal and Wright, *Numerical Optimization*, ch. 17) with BFGS inner minimisations and Armijo backtracking,
//!   then an **active-set Newton polish** on the Karush–Kuhn–Tucker system of the constraints found active: the
//!   residual of that system is computed exactly from the gradients, its Jacobian with a central-difference
//!   Hessian of the Lagrangian, so the polish converges to the machine-precision solution. Penalty and
//!   barrier methods alone stop at about the square root of the machine precision; the polish is where this
//!   solver does better than an interior-point result reported as it stands (decision R.22).
//! - **Global:** a Halton sample of the search box ranked by objective plus a violation penalty, the best and
//!   most separated points as starts of the local solver, the best feasible result kept (multistart; Rinnooy Kan
//!   and Timmer 1987). Deterministic: the same problem gives the same answer.
//! - **Integers:** branch and bound on the local solver with bound constraints (decision R.23).
//!
//! The problem is `min f(x)` subject to `c_i(x) ≥ 0` (inequalities) and `c_j(x) = 0` (equalities); the
//! objective and constraint values come in one vector `[f, c_1, ..., c_m]`, their gradients as the rows of one
//! Jacobian, symbolic when `d` gives one and by central differences otherwise.
// index loops over several parallel arrays read more plainly than zipped iterators here
#![allow(clippy::needless_range_loop)]
use crate::linalg;
use mc_eval::{NativeCx, Res};

/// The kind of a constraint value c.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// c ≥ 0
    Ge,
    /// c = 0
    Eq,
}

/// A problem as the solvers see it.
pub(crate) trait Problem<'h> {
    /// The number of variables.
    fn n(&self) -> usize;
    /// The kinds of the constraints (m of them).
    fn kinds(&self) -> &[Kind];
    /// `[f, c_1, ..., c_m]` at x; `false` when a value is not a finite real.
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool>;
    /// Values at many points stored one after the other; the default evaluates one after the other.
    fn eval_many(&mut self, cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], out: &mut [f64]) -> Res<bool> {
        let (n, w) = (self.n(), self.kinds().len() + 1);
        for k in 0..out.len() / w {
            if !self.eval(cx, &xs[k * n..(k + 1) * n], &mut out[k * w..(k + 1) * w])? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    /// The Jacobian of `[f, c...]` at x, row-major ((1 + m) by n); `false` when not numeric.
    fn jac(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool>;
}

/// The Jacobian by central differences (for problems without a symbolic one).
pub(crate) fn fd_jac<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    x: &[f64],
    out: &mut [f64],
) -> Res<bool> {
    let n = p.n();
    let w = p.kinds().len() + 1;
    let mut pts = Vec::with_capacity(2 * n * n);
    let mut hs = Vec::with_capacity(n);
    for k in 0..n {
        let h = 6e-6 * (1.0 + x[k].abs());
        hs.push(h);
        for s in [1.0, -1.0] {
            let mut y = x.to_vec();
            y[k] += s * h;
            pts.extend_from_slice(&y);
        }
    }
    let mut vals = vec![0.0; 2 * n * w];
    if !p.eval_many(cx, &pts, &mut vals)? {
        return Ok(false);
    }
    for k in 0..n {
        for i in 0..w {
            out[i * n + k] = (vals[(2 * k) * w + i] - vals[(2 * k + 1) * w + i]) / (2.0 * hs[k]);
        }
    }
    Ok(true)
}

/// The constraint violation: the largest |c| of an equality or −c of an inequality.
pub(crate) fn violation(v: &[f64], kinds: &[Kind]) -> f64 {
    kinds.iter().zip(&v[1..]).fold(0.0f64, |s, (k, &c)| match k {
        Kind::Eq => s.max(c.abs()),
        Kind::Ge => s.max(-c),
    })
}

/// A result of the local solver.
#[derive(Clone, Debug)]
pub(crate) struct Point {
    /// the point
    pub x: Vec<f64>,
    /// the objective there
    pub f: f64,
    /// the constraint violation there
    pub viol: f64,
    /// the first-order conditions hold to machine precision (the polish converged)
    pub polished: bool,
}

/// The feasibility tolerance of a point of scale `s`.
pub(crate) fn feas_tol(s: f64) -> f64 {
    1e-7 * (1.0 + s)
}

impl Point {
    /// Is `self` better than `o` (feasible before infeasible, then the lower objective)?
    pub fn better(&self, o: &Point) -> bool {
        let (fa, fb) = (self.viol <= feas_tol(1.0), o.viol <= feas_tol(1.0));
        match (fa, fb) {
            (true, false) => true,
            (false, true) => false,
            (false, false) => self.viol < o.viol,
            (true, true) => self.f < o.f - 1e-12 * (1.0 + o.f.abs()),
        }
    }
}

/// The augmented Lagrangian and its gradient from values `v` and Jacobian `j`.
fn merit(v: &[f64], j: Option<&[f64]>, kinds: &[Kind], lam: &[f64], mu: f64, n: usize, g: &mut [f64]) -> f64 {
    let mut phi = v[0];
    if let Some(j) = j {
        g.copy_from_slice(&j[..n]);
    }
    for (i, k) in kinds.iter().enumerate() {
        let c = v[1 + i];
        let coef = match k {
            Kind::Eq => {
                phi += -lam[i] * c + 0.5 * mu * c * c;
                -lam[i] + mu * c
            }
            Kind::Ge => {
                if c - lam[i] / mu <= 0.0 {
                    phi += -lam[i] * c + 0.5 * mu * c * c;
                    -lam[i] + mu * c
                } else {
                    phi += -lam[i] * lam[i] / (2.0 * mu);
                    0.0
                }
            }
        };
        if let Some(j) = j {
            if coef != 0.0 {
                for k in 0..n {
                    g[k] += coef * j[(1 + i) * n + k];
                }
            }
        }
    }
    phi
}

/// Settings of the local solver.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Settings {
    /// the most BFGS iterations of one inner minimisation
    pub inner: usize,
    /// the most outer (multiplier) iterations
    pub outer: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { inner: 400, outer: 40 }
    }
}

/// BFGS on the augmented Lagrangian from x; returns the new x (unchanged when the first value is not numeric).
#[allow(clippy::too_many_arguments)]
fn inner<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    x0: &[f64],
    lam: &[f64],
    mu: f64,
    iters: usize,
    v: &mut Vec<f64>,
    jac: &mut Vec<f64>,
) -> Res<Option<Vec<f64>>> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let w = kinds.len() + 1;
    let mut x = x0.to_vec();
    v.resize(w, 0.0);
    jac.resize(w * n, 0.0);
    if !p.eval(cx, &x, v)? || !p.jac(cx, &x, jac)? {
        return Ok(None);
    }
    let mut g = vec![0.0; n];
    let mut phi = merit(v, Some(jac), &kinds, lam, mu, n, &mut g);
    let mut hinv: Vec<f64> = (0..n * n).map(|k| if k % (n + 1) == 0 { 1.0 } else { 0.0 }).collect();
    let mut first = true;
    let mut tv = vec![0.0; w];
    let mut dummy = vec![0.0; n];
    for _ in 0..iters {
        cx.tick(1)?;
        let gn = g.iter().fold(0.0f64, |s, e| s.max(e.abs()));
        if gn <= 1e-13 * (1.0 + phi.abs()) {
            break;
        }
        let mut d: Vec<f64> = (0..n).map(|i| -(0..n).map(|k| hinv[i * n + k] * g[k]).sum::<f64>()).collect();
        let mut slope: f64 = d.iter().zip(&g).map(|(a, b)| a * b).sum();
        if slope >= 0.0 || !slope.is_finite() {
            for (i, h) in hinv.iter_mut().enumerate() {
                *h = if i % (n + 1) == 0 { 1.0 } else { 0.0 };
            }
            d = g.iter().map(|e| -e).collect();
            slope = -g.iter().map(|e| e * e).sum::<f64>();
        }
        // a first step of length at most about the scale of x
        let xs = 1.0 + x.iter().fold(0.0f64, |s, e| s.max(e.abs()));
        let dn = d.iter().fold(0.0f64, |s, e| s.max(e.abs()));
        let mut t = if first && dn > 0.0 { (0.1 * xs / dn).min(1.0) } else { 1.0 };
        if dn * t > 100.0 * xs {
            t = 100.0 * xs / dn;
        }
        let mut accepted = None;
        for _ in 0..60 {
            let y: Vec<f64> = x.iter().zip(&d).map(|(a, b)| a + t * b).collect();
            if p.eval(cx, &y, &mut tv)? {
                let ph = merit(&tv, None, &kinds, lam, mu, n, &mut dummy);
                if ph.is_finite() && ph <= phi + 1e-4 * t * slope {
                    accepted = Some((y, ph));
                    break;
                }
            }
            t *= 0.5;
        }
        let Some((y, _)) = accepted else { break };
        let mut jy = vec![0.0; w * n];
        if !p.jac(cx, &y, &mut jy)? || !p.eval(cx, &y, &mut tv)? {
            break;
        }
        let mut gy = vec![0.0; n];
        let ph = merit(&tv, Some(&jy), &kinds, lam, mu, n, &mut gy);
        let s: Vec<f64> = y.iter().zip(&x).map(|(a, b)| a - b).collect();
        let yv: Vec<f64> = gy.iter().zip(&g).map(|(a, b)| a - b).collect();
        let sy: f64 = s.iter().zip(&yv).map(|(a, b)| a * b).sum();
        let sn = s.iter().fold(0.0f64, |m, e| m.max(e.abs()));
        let small = sn <= 1e-15 * xs && (phi - ph).abs() <= 1e-15 * (1.0 + phi.abs());
        if sy > 1e-14 * s.iter().map(|e| e * e).sum::<f64>().sqrt() * yv.iter().map(|e| e * e).sum::<f64>().sqrt() {
            if first {
                // scale the initial inverse Hessian (Nocedal and Wright 6.20)
                let yy: f64 = yv.iter().map(|e| e * e).sum();
                let sc = sy / yy;
                for (i, h) in hinv.iter_mut().enumerate() {
                    *h = if i % (n + 1) == 0 { sc } else { 0.0 };
                }
                first = false;
            }
            // H+ = (I - rho s y^T) H (I - rho y s^T) + rho s s^T
            let rho = 1.0 / sy;
            let hy: Vec<f64> = (0..n).map(|i| (0..n).map(|k| hinv[i * n + k] * yv[k]).sum()).collect();
            let yhy: f64 = yv.iter().zip(&hy).map(|(a, b)| a * b).sum();
            for i in 0..n {
                for k in 0..n {
                    hinv[i * n + k] += -rho * (hy[i] * s[k] + s[i] * hy[k]) + (rho * rho * yhy + rho) * s[i] * s[k];
                }
            }
        }
        x = y;
        g = gy;
        phi = ph;
        v.copy_from_slice(&tv);
        jac.copy_from_slice(&jy);
        if small {
            break;
        }
    }
    Ok(Some(x))
}

/// The local solver from `x0`: the augmented Lagrangian, then the active-set Newton polish. `None` when the
/// values are not numeric at the start.
pub(crate) fn local<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    x0: &[f64],
    s: Settings,
) -> Res<Option<Point>> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let m = kinds.len();
    let mut lam = vec![0.0; m];
    let mut mu = 10.0;
    let mut x = x0.to_vec();
    let (mut v, mut jac) = (Vec::new(), Vec::new());
    let mut prev = f64::INFINITY;
    let mut ok = false;
    for _ in 0..if m == 0 { 1 } else { s.outer } {
        let Some(y) = inner(cx, p, &x, &lam, mu, s.inner, &mut v, &mut jac)? else { break };
        ok = true;
        x = y;
        if m == 0 {
            break;
        }
        let viol = violation(&v, &kinds);
        for i in 0..m {
            let c = v[1 + i];
            lam[i] = match kinds[i] {
                Kind::Eq => lam[i] - mu * c,
                Kind::Ge => (lam[i] - mu * c).max(0.0),
            };
        }
        let xs = 1.0 + x.iter().fold(0.0f64, |a, e| a.max(e.abs()));
        if viol <= 1e-12 * xs && (prev - viol).abs() <= 1e-12 * xs {
            break;
        }
        if viol > 0.25 * prev {
            mu = (mu * 10.0).min(1e12);
        }
        prev = viol;
    }
    if !ok {
        return Ok(None);
    }
    let mut v = vec![0.0; m + 1];
    if !p.eval(cx, &x, &mut v)? {
        return Ok(None);
    }
    let mut pt = Point { f: v[0], viol: violation(&v, &kinds), x, polished: false };
    if let Some(q) = polish(cx, p, &pt, &lam)? {
        if q.viol <= pt.viol.max(feas_tol(1.0)) && q.f <= pt.f + 1e-7 * (1.0 + pt.f.abs()) {
            pt = q;
        }
    }
    if !pt.polished && m == 0 {
        // a nonsmooth objective (abs, max): the Nelder-Mead simplex from the BFGS point, which needs no gradient
        if let Some(q) = nelder_mead(cx, p, &pt.x, 2000 * n.max(1))? {
            if q.f < pt.f {
                pt = q;
            }
        }
    }
    Ok(Some(pt))
}

/// The gradient of the Lagrangian ∇f − Σ λ_i ∇c_i over the constraints `act`.
fn lagrangian_grad(j: &[f64], act: &[usize], lam: &[f64], n: usize) -> Vec<f64> {
    let mut g = j[..n].to_vec();
    for (a, &i) in act.iter().enumerate() {
        for k in 0..n {
            g[k] -= lam[a] * j[(1 + i) * n + k];
        }
    }
    g
}

/// The active-set Newton polish (decision R.22): Newton's method on ∇f − Σ λ_i ∇c_i = 0, c_A = 0 over the
/// equalities and the inequalities active at `pt`, the Hessian of the Lagrangian by central differences of its
/// gradient. Accepted when it converges, the multipliers of the inequalities are non-negative and every constraint
/// holds.
fn polish<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, pt: &Point, lam0: &[f64]) -> Res<Option<Point>> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let m = kinds.len();
    let w = m + 1;
    let mut v = vec![0.0; w];
    let mut j = vec![0.0; w * n];
    let mut x = pt.x.clone();
    if !p.eval(cx, &x, &mut v)? || !p.jac(cx, &x, &mut j)? {
        return Ok(None);
    }
    let xs = 1.0 + x.iter().fold(0.0f64, |a, e| a.max(e.abs()));
    // the active set: equalities, and inequalities at (or nearly at) their bound
    let mut act: Vec<usize> = (0..m)
        .filter(|&i| {
            let gn = (0..n).fold(0.0f64, |a, k| a.max(j[(1 + i) * n + k].abs()));
            kinds[i] == Kind::Eq || v[1 + i] <= 1e-6 * (1.0 + gn * xs)
        })
        .collect();
    if act.len() > n {
        // keep the equalities and the inequalities with the largest multipliers
        act.sort_by(|&a, &b| {
            let ka = (kinds[a] == Kind::Eq, lam0[a]);
            let kb = (kinds[b] == Kind::Eq, lam0[b]);
            kb.0.cmp(&ka.0).then(kb.1.total_cmp(&ka.1))
        });
        act.truncate(n);
        act.sort();
    }
    let na = act.len();
    let mut lam: Vec<f64> = act.iter().map(|&i| lam0[i]).collect();
    let dim = n + na;
    let mut converged = false;
    for _ in 0..30 {
        cx.tick(1)?;
        let gl = lagrangian_grad(&j, &act, &lam, n);
        let mut r: Vec<f64> = gl.clone();
        r.extend(act.iter().map(|&i| v[1 + i]));
        let rn = r.iter().fold(0.0f64, |a, e| a.max(e.abs()));
        // the Hessian of the Lagrangian by central differences of its gradient
        let mut hcols = vec![0.0; n * n];
        for k in 0..n {
            let h = 1e-5 * (1.0 + x[k].abs());
            let mut gp = vec![0.0; w * n];
            let mut gm = vec![0.0; w * n];
            let mut y = x.clone();
            y[k] = x[k] + h;
            if !p.jac(cx, &y, &mut gp)? {
                return Ok(None);
            }
            y[k] = x[k] - h;
            if !p.jac(cx, &y, &mut gm)? {
                return Ok(None);
            }
            let a = lagrangian_grad(&gp, &act, &lam, n);
            let b = lagrangian_grad(&gm, &act, &lam, n);
            for i in 0..n {
                hcols[i * n + k] = (a[i] - b[i]) / (2.0 * h);
            }
        }
        let mut kk = vec![0.0; dim * dim];
        for i in 0..n {
            for k in 0..n {
                kk[i * dim + k] = 0.5 * (hcols[i * n + k] + hcols[k * n + i]);
            }
            for (a, &c) in act.iter().enumerate() {
                kk[i * dim + n + a] = -j[(1 + c) * n + i];
                kk[(n + a) * dim + i] = j[(1 + c) * n + i];
            }
        }
        let rhs: Vec<f64> = r.iter().map(|e| -e).collect();
        let Some(step) = linalg::solve(dim, &kk, &rhs) else { return Ok(None) };
        // backtracking on the residual
        let mut t = 1.0;
        let mut moved = false;
        for _ in 0..20 {
            let y: Vec<f64> = (0..n).map(|k| x[k] + t * step[k]).collect();
            let ly: Vec<f64> = (0..na).map(|a| lam[a] + t * step[n + a]).collect();
            let mut vy = vec![0.0; w];
            let mut jy = vec![0.0; w * n];
            if p.eval(cx, &y, &mut vy)? && p.jac(cx, &y, &mut jy)? {
                let mut ry = lagrangian_grad(&jy, &act, &ly, n);
                ry.extend(act.iter().map(|&i| vy[1 + i]));
                let rny = ry.iter().fold(0.0f64, |a, e| a.max(e.abs()));
                if rny < rn || rny <= 1e-15 * xs {
                    x = y;
                    lam = ly;
                    v = vy;
                    j = jy;
                    moved = true;
                    break;
                }
            }
            t *= 0.5;
        }
        let sn = (0..n).fold(0.0f64, |a, k| a.max((t * step[k]).abs()));
        if !moved || sn <= 4.0 * f64::EPSILON * xs {
            converged = rn <= 1e-8 * (1.0 + v[0].abs()) || sn <= 4.0 * f64::EPSILON * xs;
            break;
        }
    }
    if !converged {
        return Ok(None);
    }
    // the multipliers of active inequalities must not be negative
    let scale = 1.0 + (0..n).fold(0.0f64, |a, k| a.max(j[k].abs()));
    if act.iter().zip(&lam).any(|(&i, &l)| kinds[i] == Kind::Ge && l < -1e-7 * scale) {
        return Ok(None);
    }
    Ok(Some(Point { f: v[0], viol: violation(&v, &kinds), x, polished: true }))
}

/// The k-th point of the Halton sequence in the unit cube of dimension `d` (the first primes as bases; beyond 64
/// dimensions the bases repeat with a scramble of the index).
fn halton(k: u64, d: usize, out: &mut [f64]) {
    const P: [u64; 32] = [
        2, 3, 5, 7, 11, 13, 17, 19, 23, 29, 31, 37, 41, 43, 47, 53, 59, 61, 67, 71, 73, 79, 83, 89, 97, 101, 103, 107,
        109, 113, 127, 131,
    ];
    for (i, o) in out.iter_mut().enumerate().take(d) {
        let b = P[i % P.len()];
        let mut kk = k + 1 + (i / P.len()) as u64 * 7919;
        let (mut f, mut r) = (1.0, 0.0);
        while kk > 0 {
            f /= b as f64;
            r += f * (kk % b) as f64;
            kk /= b;
        }
        *o = r;
    }
}

/// The global search (decision R.21): `samples` Halton points of the box, the `starts` best separated ones (and
/// the given points `extra`) as starts of the local solver; the best result.
#[allow(clippy::too_many_arguments)]
pub(crate) fn global<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    lo: &[f64],
    hi: &[f64],
    samples: usize,
    starts: usize,
    extra: &[Vec<f64>],
    s: Settings,
) -> Res<Option<Point>> {
    let n = p.n();
    let kinds = p.kinds().to_vec();
    let w = kinds.len() + 1;
    let mut cand: Vec<(f64, Vec<f64>)> = Vec::new();
    const BLOCK: usize = 512;
    let mut u = vec![0.0; n];
    let mut k = 0u64;
    while (k as usize) < samples {
        cx.tick(1)?;
        let cnt = BLOCK.min(samples - k as usize);
        let mut xs = Vec::with_capacity(cnt * n);
        for i in 0..cnt {
            halton(k + i as u64, n, &mut u);
            xs.extend((0..n).map(|d| lo[d] + u[d] * (hi[d] - lo[d])));
        }
        let mut vals = vec![0.0; cnt * w];
        if p.eval_many(cx, &xs, &mut vals)? {
            for i in 0..cnt {
                let v = &vals[i * w..(i + 1) * w];
                let score = v[0] + 1e3 * (1.0 + v[0].abs()) * violation(v, &kinds);
                if score.is_finite() {
                    cand.push((score, xs[i * n..(i + 1) * n].to_vec()));
                }
            }
        } else {
            // some point is not numeric: one at a time
            let mut v = vec![0.0; w];
            for i in 0..cnt {
                let x = &xs[i * n..(i + 1) * n];
                if p.eval(cx, x, &mut v)? {
                    let score = v[0] + 1e3 * (1.0 + v[0].abs()) * violation(&v, &kinds);
                    if score.is_finite() {
                        cand.push((score, x.to_vec()));
                    }
                }
            }
        }
        k += cnt as u64;
    }
    cand.sort_by(|a, b| a.0.total_cmp(&b.0));
    // the best points at least a little apart (a tenth of the box diagonal over the start count)
    let diag = (0..n).map(|d| (hi[d] - lo[d]).powi(2)).sum::<f64>().sqrt();
    let sep = 0.25 * diag / (starts as f64).max(1.0);
    let mut chosen: Vec<Vec<f64>> = extra.to_vec();
    for (_, x) in &cand {
        if chosen.len() >= starts + extra.len() {
            break;
        }
        if chosen.iter().all(|c| c.iter().zip(x).map(|(a, b)| (a - b).powi(2)).sum::<f64>().sqrt() >= sep) {
            chosen.push(x.clone());
        }
    }
    let mut best: Option<Point> = None;
    for x0 in chosen {
        cx.tick(1)?;
        if let Some(pt) = local(cx, p, &x0, s)? {
            if best.as_ref().is_none_or(|b| pt.better(b)) {
                best = Some(pt);
            }
        }
    }
    Ok(best)
}

/// A problem with extra bound constraints x_k ≥ b (`true`) or x_k ≤ b (`false`), for branch and bound.
pub(crate) struct Bounded<'a, 'h> {
    /// the problem
    pub inner: &'a mut dyn Problem<'h>,
    /// the bounds
    pub bounds: Vec<(usize, f64, bool)>,
    kinds: Vec<Kind>,
}

impl<'a, 'h> Bounded<'a, 'h> {
    /// The problem with the bounds.
    pub fn new(inner: &'a mut dyn Problem<'h>, bounds: Vec<(usize, f64, bool)>) -> Self {
        let mut kinds = inner.kinds().to_vec();
        kinds.extend(std::iter::repeat_n(Kind::Ge, bounds.len()));
        Bounded { inner, bounds, kinds }
    }
}

impl<'h> Problem<'h> for Bounded<'_, 'h> {
    fn n(&self) -> usize {
        self.inner.n()
    }
    fn kinds(&self) -> &[Kind] {
        &self.kinds
    }
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        let w = self.inner.kinds().len() + 1;
        if !self.inner.eval(cx, x, &mut out[..w])? {
            return Ok(false);
        }
        for (i, &(k, b, ge)) in self.bounds.iter().enumerate() {
            out[w + i] = if ge { x[k] - b } else { b - x[k] };
        }
        Ok(true)
    }
    fn jac(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        let n = self.n();
        let w = self.inner.kinds().len() + 1;
        if !self.inner.jac(cx, x, &mut out[..w * n])? {
            return Ok(false);
        }
        for (i, &(k, _, ge)) in self.bounds.iter().enumerate() {
            let row = &mut out[(w + i) * n..(w + i + 1) * n];
            row.fill(0.0);
            row[k] = if ge { 1.0 } else { -1.0 };
        }
        Ok(true)
    }
}

/// The solver of a node's relaxation.
pub(crate) type Relaxation<'s, 'h> = dyn FnMut(&mut NativeCx<'_, '_, 'h>, &mut dyn Problem<'h>) -> Res<Option<Point>> + 's;

/// The most nodes of a nonlinear branch and bound.
pub(crate) const MAX_NL_NODES: usize = 400;

/// Branch and bound over the integer variables `int` with `solve` the relaxation solver at each node (decision
/// R.23); the integer variables of the result are rounded.
pub(crate) fn integer<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    p: &mut dyn Problem<'h>,
    int: &[bool],
    solve: &mut Relaxation<'_, 'h>,
) -> Res<Option<Point>> {
    let mut best: Option<Point> = None;
    let mut stack: Vec<Vec<(usize, f64, bool)>> = vec![Vec::new()];
    let mut nodes = 0;
    while let Some(bounds) = stack.pop() {
        nodes += 1;
        if nodes > MAX_NL_NODES {
            break;
        }
        cx.tick(1)?;
        let mut bp = Bounded::new(p, bounds.clone());
        let Some(pt) = solve(cx, &mut bp)? else { continue };
        if pt.viol > feas_tol(1.0) {
            continue;
        }
        if best.as_ref().is_some_and(|b| pt.f >= b.f - 1e-12 * (1.0 + b.f.abs())) {
            continue;
        }
        let frac = (0..pt.x.len())
            .filter(|&k| int[k])
            .map(|k| (k, (pt.x[k] - pt.x[k].round()).abs()))
            .filter(|&(_, f)| f > 1e-6)
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match frac {
            None => {
                // round, then re-solve the continuous variables with the integers fixed
                let mut fixed = bounds.clone();
                for k in 0..pt.x.len() {
                    if int[k] {
                        fixed.push((k, pt.x[k].round(), true));
                        fixed.push((k, pt.x[k].round(), false));
                    }
                }
                let mut fp = Bounded::new(p, fixed);
                let q = solve(cx, &mut fp)?.filter(|q| q.viol <= feas_tol(1.0));
                let mut q = q.unwrap_or(pt);
                for k in 0..q.x.len() {
                    if int[k] {
                        q.x[k] = q.x[k].round();
                    }
                }
                let mut v = vec![0.0; p.kinds().len() + 1];
                if p.eval(cx, &q.x, &mut v)? {
                    q.f = v[0];
                    q.viol = violation(&v, p.kinds());
                    if q.viol <= feas_tol(1.0) && best.as_ref().is_none_or(|b| q.f < b.f) {
                        best = Some(q);
                    }
                }
            }
            Some((k, _)) => {
                let mut lo = bounds.clone();
                lo.push((k, pt.x[k].floor(), false));
                let mut hi = bounds;
                hi.push((k, pt.x[k].ceil(), true));
                if pt.x[k] - pt.x[k].floor() < 0.5 {
                    stack.push(hi);
                    stack.push(lo);
                } else {
                    stack.push(lo);
                    stack.push(hi);
                }
            }
        }
    }
    Ok(best)
}

/// The Nelder–Mead simplex method (Nelder and Mead 1965, with the adaptive coefficients of Gao and Han 2012) on
/// an unconstrained objective from x0, restarted once from its result; `None` when the start is not numeric.
fn nelder_mead<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>, x0: &[f64], iters: usize) -> Res<Option<Point>> {
    let n = p.n();
    let mut v = vec![0.0; 1];
    let mut f = |cx: &mut NativeCx<'_, '_, 'h>, x: &[f64]| -> Res<f64> {
        Ok(if p.eval(cx, x, &mut v)? { v[0] } else { f64::INFINITY })
    };
    let nf = n as f64;
    let (alpha, beta, gamma, delta) = (1.0, 1.0 + 2.0 / nf, 0.75 - 1.0 / (2.0 * nf), 1.0 - 1.0 / nf);
    let mut best = x0.to_vec();
    let mut fbest = f(cx, &best)?;
    if !fbest.is_finite() {
        return Ok(None);
    }
    for round in 0..2 {
        let step = if round == 0 { 0.05 } else { 1e-4 };
        let mut simplex: Vec<(Vec<f64>, f64)> = Vec::with_capacity(n + 1);
        simplex.push((best.clone(), fbest));
        for k in 0..n {
            let mut y = best.clone();
            y[k] += step * (1.0 + y[k].abs());
            let fy = f(cx, &y)?;
            simplex.push((y, fy));
        }
        for _ in 0..iters {
            cx.tick(1)?;
            simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
            let (fl, fh) = (simplex[0].1, simplex[n].1);
            let size = simplex[1..].iter().fold(0.0f64, |a, (y, _)| {
                a.max(y.iter().zip(&simplex[0].0).fold(0.0f64, |b, (p, q)| b.max((p - q).abs())))
            });
            let xs = 1.0 + simplex[0].0.iter().fold(0.0f64, |a, e| a.max(e.abs()));
            if (fh - fl).abs() <= 1e-16 * (fl.abs() + 1e-300) && size <= 1e-15 * xs || size <= 1e-17 * xs {
                break;
            }
            let c: Vec<f64> = (0..n).map(|k| simplex[..n].iter().map(|(y, _)| y[k]).sum::<f64>() / nf).collect();
            let worst = simplex[n].0.clone();
            let at = |t: f64| -> Vec<f64> { (0..n).map(|k| c[k] + t * (c[k] - worst[k])).collect() };
            let xr = at(alpha);
            let fr = f(cx, &xr)?;
            if fr < simplex[0].1 {
                let xe = at(alpha * beta);
                let fe = f(cx, &xe)?;
                simplex[n] = if fe < fr { (xe, fe) } else { (xr, fr) };
            } else if fr < simplex[n - 1].1 {
                simplex[n] = (xr, fr);
            } else {
                let (xc, fc) = if fr < fh {
                    let y = at(alpha * gamma);
                    let fy = f(cx, &y)?;
                    (y, fy)
                } else {
                    let y = at(-gamma);
                    let fy = f(cx, &y)?;
                    (y, fy)
                };
                if fc < fh.min(fr) {
                    simplex[n] = (xc, fc);
                } else {
                    let x0 = simplex[0].0.clone();
                    for item in simplex.iter_mut().skip(1) {
                        let y: Vec<f64> = (0..n).map(|k| x0[k] + delta * (item.0[k] - x0[k])).collect();
                        let fy = f(cx, &y)?;
                        *item = (y, fy);
                    }
                }
            }
        }
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        if simplex[0].1 <= fbest {
            best = simplex[0].0.clone();
            fbest = simplex[0].1;
        }
    }
    Ok(Some(Point { x: best, f: fbest, viol: 0.0, polished: false }))
}

/// A problem whose constraints are multiplied by positive scales, so that a constraint whose own size is small
/// (`x^2 + y^2 <= 1e-10`, value 1e-10 at the origin) is judged against that size and not against the absolute
/// feasibility tolerance (a point violating it by 4e-10 is nowhere near feasible).
pub(crate) struct Scaled<'a, 'h> {
    inner: &'a mut dyn Problem<'h>,
    scales: Vec<f64>,
}

impl<'a, 'h> Scaled<'a, 'h> {
    /// The problem with constraint `i` multiplied by `scales[i]`.
    pub fn new(inner: &'a mut dyn Problem<'h>, scales: Vec<f64>) -> Self {
        // the solver sees scales of at most 1e15 (beyond that the gradients swamp the objective)
        Scaled { inner, scales: scales.into_iter().map(|s| s.min(1e15)).collect() }
    }
}

impl<'h> Problem<'h> for Scaled<'_, 'h> {
    fn n(&self) -> usize {
        self.inner.n()
    }
    fn kinds(&self) -> &[Kind] {
        self.inner.kinds()
    }
    fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        if !self.inner.eval(cx, x, out)? {
            return Ok(false);
        }
        for (i, &s) in self.scales.iter().enumerate() {
            out[1 + i] *= s;
        }
        Ok(true)
    }
    fn jac(&mut self, cx: &mut NativeCx<'_, '_, 'h>, x: &[f64], out: &mut [f64]) -> Res<bool> {
        if !self.inner.jac(cx, x, out)? {
            return Ok(false);
        }
        let n = self.inner.n();
        for (i, &s) in self.scales.iter().enumerate() {
            for v in &mut out[(1 + i) * n..(2 + i) * n] {
                *v *= s;
            }
        }
        Ok(true)
    }
}

/// The scales that make each constraint's value at the origin one when it is smaller: `1 /
/// min(1, |c_i(0)|)`; 1 for a constraint that is zero or at least 1 there, or not numeric.
pub(crate) fn constraint_scales<'h>(cx: &mut NativeCx<'_, '_, 'h>, p: &mut dyn Problem<'h>) -> Res<Vec<f64>> {
    let m = p.kinds().len();
    let mut v = vec![0.0; m + 1];
    if m == 0 || !p.eval(cx, &vec![0.0; p.n()], &mut v)? {
        return Ok(vec![1.0; m]);
    }
    Ok(v[1..].iter().map(|c| if c.abs() > 0.0 && c.abs() < 1.0 { 1.0 / c.abs() } else { 1.0 }).collect())
}
