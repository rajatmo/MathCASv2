//! The integration domain of the multidimensional methods (cubature, Monte Carlo, quasi-Monte Carlo) as a map
//! of the unit cube, and the integrand evaluated in blocks on it (plan 10, section 2.1).
//!
//! Each range `[x, a, s1, ..., b]` is cut into its K pieces; coordinate u in [0, 1] of the cube goes to piece
//! j = floor(K u), where it is mapped affinely onto the piece, or by the rational maps of `machine` onto an infinite
//! piece. Inner ranges whose ends depend on the outer variables are mapped after the outer ones, their ends
//! evaluated for the whole block at once. The weight of a point is the Jacobian of the whole map; a point that
//! maps to an infinite end has weight 0 and the integrand is not evaluated there.
//!
//! [`Sampler`] evaluates `f(x(u)) w(u)` for a block of points: the map, then one `eval_flat` call of the integrand
//! (compiled code split across the session's threads by `mc_numfn::Par`, the interpreter otherwise). The order of
//! points and the reductions done by the methods do not depend on the number of threads, so neither do results.
use crate::machine::{piece, End};
use crate::nested::Bound;
use mc_eval::{NativeCx, Res};
use mc_numfn::NumFn;
use rayon::prelude::*;

/// Points mapped and evaluated per call of the integrand (bounds the memory of a block).
pub(crate) const CHUNK: usize = 1 << 15;

/// Points per task of the parallel maps of constant ranges.
const MAP_BLOCK: usize = 2048;

/// The ranges of a multiple integral as a map of the unit cube.
pub(crate) struct Domain<'h> {
    /// the points of each range (`[a, s1, ..., b]`), outermost first
    pub ranges: Vec<Vec<Bound<'h>>>,
}

/// One coordinate of piece `j` of `k` pieces with ends `p`, `q`: the value and the derivative of the map at
/// local position `s` in [0, 1]; `None` at an infinite end or on an empty piece.
#[inline]
fn coord(k: usize, s: f64, p: End, q: End) -> Option<(f64, f64)> {
    let pc = piece(p, q)?;
    let t = pc.t0 + s * (pc.t1 - pc.t0);
    let (x, dx) = pc.map.apply(t)?;
    Some((x, k as f64 * (pc.t1 - pc.t0) * dx))
}

/// A finite stand-in for a coordinate whose weight is 0 (so that inner ends can still be evaluated).
fn stand_in(p: End, q: End) -> f64 {
    match (p, q) {
        (End::At(a), _) => a,
        (_, End::At(b)) => b,
        _ => 0.0,
    }
}

/// Piece index and local position of coordinate `u` among `k` pieces.
#[inline]
fn locate(k: usize, u: f64) -> (usize, f64) {
    let ku = u * k as f64;
    let j = (ku as usize).min(k - 1);
    (j, ku - j as f64)
}

impl<'h> Domain<'h> {
    /// The number of variables.
    pub fn dim(&self) -> usize {
        self.ranges.len()
    }

    /// The number of pieces of range `k`.
    pub fn pieces(&self, k: usize) -> usize {
        self.ranges[k].len() - 1
    }

    /// Are the ends of range `k` constants?
    fn constant_range(&self, k: usize) -> bool {
        self.ranges[k].iter().all(|b| matches!(b, Bound::Const(_)))
    }

    /// Map the points `u` (each `dim()` coordinates in [0, 1]) to `x` and their weights `w`. `Err(point)`: an end of
    /// an inner range is not numeric at the outer variables' values `point`.
    pub fn map(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        u: &[f64],
        x: &mut [f64],
        w: &mut [f64],
    ) -> Res<Result<(), Vec<f64>>> {
        let d = self.dim();
        let n = w.len();
        w.fill(1.0);
        for k in 0..d {
            let kp = self.pieces(k);
            if self.constant_range(k) {
                let ends: Vec<End> = self.ranges[k]
                    .iter()
                    .map(|b| match b {
                        Bound::Const(e) => *e,
                        Bound::Expr(_) => End::At(0.0),
                    })
                    .collect();
                let one = |u: &[f64], x: &mut [f64], w: &mut [f64]| {
                    for i in 0..w.len() {
                        let (j, s) = locate(kp, u[i * d + k]);
                        match coord(kp, s, ends[j], ends[j + 1]) {
                            Some((v, dv)) => {
                                x[i * d + k] = v;
                                w[i] *= dv;
                            }
                            None => {
                                x[i * d + k] = stand_in(ends[j], ends[j + 1]);
                                w[i] = 0.0;
                            }
                        }
                    }
                };
                if n >= 2 * MAP_BLOCK && rayon::current_num_threads() > 1 {
                    u.par_chunks(MAP_BLOCK * d)
                        .zip(x.par_chunks_mut(MAP_BLOCK * d))
                        .zip(w.par_chunks_mut(MAP_BLOCK))
                        .for_each(|((u, x), w)| one(u, x, w));
                } else {
                    one(u, x, w);
                }
                continue;
            }
            // ends depending on the outer variables: each such end evaluated for the whole block
            let mut values: Vec<Option<Vec<f64>>> = Vec::with_capacity(kp + 1);
            let prefix: Vec<f64> = (0..n).flat_map(|i| x[i * d..i * d + k].iter().copied()).collect();
            for b in self.ranges[k].iter_mut() {
                match b {
                    Bound::Const(_) => values.push(None),
                    Bound::Expr(g) => {
                        let mut v = vec![0.0; n];
                        if !g.eval_flat(cx, &prefix, &mut v)? {
                            // the first point where the end is not numeric
                            for i in 0..n {
                                let mut y = [0.0];
                                if !g.eval(cx, &prefix[i * k..(i + 1) * k], &mut y)? {
                                    return Ok(Err(prefix[i * k..(i + 1) * k].to_vec()));
                                }
                                v[i] = y[0];
                            }
                        }
                        values.push(Some(v));
                    }
                }
            }
            for i in 0..n {
                if i % 4096 == 4095 {
                    cx.tick(1)?;
                }
                let (j, s) = locate(kp, u[i * d + k]);
                let end = |m: usize| match (&self.ranges[k][m], &values[m]) {
                    (Bound::Const(e), _) => *e,
                    (_, Some(v)) => End::At(v[i]),
                    (_, None) => End::At(0.0),
                };
                let (p, q) = (end(j), end(j + 1));
                match coord(kp, s, p, q) {
                    Some((v, dv)) => {
                        x[i * d + k] = v;
                        w[i] *= dv;
                    }
                    None => {
                        x[i * d + k] = stand_in(p, q);
                        w[i] = 0.0;
                    }
                }
            }
        }
        Ok(Ok(()))
    }
}

/// The integrand on the unit cube: `f(x(u)) w(u)`, evaluated in blocks.
pub(crate) struct Sampler<'h> {
    /// the integrand, a function of every variable (outer ones first)
    pub f: Box<dyn NumFn<'h, f64> + 'h>,
    /// the map of the cube
    pub dom: Domain<'h>,
    /// evaluations of the integrand so far
    pub calls: u64,
}

impl<'h> Sampler<'h> {
    /// The number of variables.
    pub fn dim(&self) -> usize {
        self.dom.dim()
    }

    /// The values `f(x(u)) w(u)` at the points `u` (each `dim()` coordinates) into `out`. `Err(point)`: the
    /// integrand (or an end of an inner range) is not a finite number at `point`.
    pub fn eval(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: &[f64], out: &mut [f64]) -> Res<Result<(), Vec<f64>>> {
        let d = self.dim();
        let mut x = Vec::new();
        let mut w = Vec::new();
        let mut xs = Vec::new();
        let mut at = Vec::new();
        for (uc, oc) in u.chunks(CHUNK * d).zip(out.chunks_mut(CHUNK)) {
            let n = oc.len();
            x.resize(n * d, 0.0);
            w.resize(n, 0.0);
            if let Err(p) = self.dom.map(cx, uc, &mut x, &mut w)? {
                return Ok(Err(p));
            }
            // the points of non-zero weight
            xs.clear();
            at.clear();
            for i in 0..n {
                if w[i] != 0.0 {
                    at.push(i);
                    xs.extend_from_slice(&x[i * d..(i + 1) * d]);
                }
            }
            let mut ys = vec![0.0; at.len()];
            self.calls += at.len() as u64;
            let ok = at.is_empty() || self.f.eval_flat(cx, &xs, &mut ys)?;
            oc.fill(0.0);
            for (m, &i) in at.iter().enumerate() {
                let p = &xs[m * d..(m + 1) * d];
                if !ok {
                    // find the first point that is not numeric (a whole batch may also fail when a call declined)
                    let mut y = [0.0];
                    if !self.f.eval(cx, p, &mut y)? {
                        return Ok(Err(p.to_vec()));
                    }
                    ys[m] = y[0];
                }
                let v = ys[m] * w[i];
                if !v.is_finite() {
                    return Ok(Err(p.to_vec()));
                }
                oc[i] = v;
            }
        }
        Ok(Ok(()))
    }

    /// The variables' values at the point `u` of the cube (for messages); `None` if an end is not numeric there.
    pub fn point(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: &[f64]) -> Res<Option<Vec<f64>>> {
        let mut x = vec![0.0; u.len()];
        let mut w = [0.0];
        Ok(self.dom.map(cx, u, &mut x, &mut w)?.ok().map(|()| x))
    }
}
