//! Linear programming and mixed-integer linear programming in machine reals: the problems of `n_minimize`,
//! `find_minimum` and their relatives whose objective and constraints are all affine (decision R.20).
//!
//! - `simplex`: the two-phase dense simplex method with Bland's rule (no cycling; Dantzig 1963, Bland 1977) on
//!   the standard form min c·y, A y = b, y ≥ 0, b ≥ 0. Free variables are split as x = p − q, inequalities get
//!   slack columns. The problems of the documentation are small (a few dozen columns), so the dense tableau is the
//!   simplest correct choice.
//! - `solve`: the general form (rows `a·x ≤ b`, `a·x ≥ b`, `a·x = b`, every variable free), with the vertex
//!   polished afterwards: the active rows are solved again by Gaussian elimination, so a vertex with exact rational
//!   data comes out correctly rounded instead of carrying the tableau's rounding.
//! - `branch_and_bound`: integer variables by depth-first branch and bound on the LP relaxation (Land and Doig
//!   1960), the best bound pruning, a node limit keeping it bounded.
// index loops over several parallel arrays read more plainly than zipped iterators here
#![allow(clippy::needless_range_loop)]
use crate::linalg;

/// The relation of a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Rel {
    /// a·x ≤ b
    Le,
    /// a·x ≥ b
    Ge,
    /// a·x = b
    Eq,
}

/// One row a·x (rel) b.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// the coefficients
    pub a: Vec<f64>,
    /// the relation
    pub rel: Rel,
    /// the right-hand side
    pub b: f64,
}

/// The outcome of a linear program.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Lp {
    /// a minimiser and the minimum
    Optimal(Vec<f64>, f64),
    /// no point satisfies the rows
    Infeasible,
    /// the objective is unbounded below on the feasible set
    Unbounded,
}

/// Pivot tolerance of the tableau.
const EPS: f64 = 1e-10;

/// The dense simplex method on min c·y, A y = b (b ≥ 0), y ≥ 0. `None` when the iteration limit is hit.
fn simplex(a: &[Vec<f64>], b: &[f64], c: &[f64]) -> Option<Result<Vec<f64>, bool>> {
    // Ok(y), Err(true) infeasible, Err(false) unbounded
    let (m, n) = (a.len(), c.len());
    // tableau with m artificial columns: [A | I | b]
    let w = n + m + 1;
    let mut t: Vec<Vec<f64>> = (0..m)
        .map(|i| {
            let mut r = vec![0.0; w];
            r[..n].copy_from_slice(&a[i]);
            r[n + i] = 1.0;
            r[w - 1] = b[i];
            r
        })
        .collect();
    let mut basis: Vec<usize> = (n..n + m).collect();
    let limit = 50 * (n + m) + 1000;
    // phase 1: minimise the sum of the artificials
    let mut cost1 = vec![0.0; n + m];
    for c1 in cost1.iter_mut().skip(n) {
        *c1 = 1.0;
    }
    if !run(&mut t, &mut basis, &cost1, n + m, limit)? {
        return Some(Err(false));
    }
    let infeas: f64 = basis.iter().enumerate().filter(|&(_, &j)| j >= n).map(|(i, _)| t[i][w - 1]).sum();
    let scale = 1.0 + b.iter().fold(0.0f64, |s, v| s.max(v.abs()));
    if infeas > 1e-8 * scale {
        return Some(Err(true));
    }
    // drive artificials out of the basis where possible
    for i in 0..m {
        if basis[i] >= n {
            if let Some(j) = (0..n).find(|&j| t[i][j].abs() > EPS) {
                pivot(&mut t, &mut basis, i, j);
            }
        }
    }
    // phase 2 on the original columns (artificial columns barred)
    let mut cost2 = c.to_vec();
    cost2.extend(std::iter::repeat_n(0.0, m));
    for row in t.iter_mut() {
        for v in row.iter_mut().skip(n).take(m) {
            *v = 0.0;
        }
    }
    for (i, &j) in basis.iter().enumerate() {
        if j >= n {
            // a redundant row: its artificial stays basic at zero; keep it there
            t[i][j] = 1.0;
        }
    }
    if !run(&mut t, &mut basis, &cost2, n, limit)? {
        return Some(Err(false));
    }
    let mut y = vec![0.0; n];
    for (i, &j) in basis.iter().enumerate() {
        if j < n {
            y[j] = t[i][w - 1];
        }
    }
    Some(Ok(y))
}

/// Simplex iterations with Bland's rule over the first `cols` columns; `Some(false)` unbounded, `None` at the
/// iteration limit.
fn run(t: &mut [Vec<f64>], basis: &mut [usize], cost: &[f64], cols: usize, limit: usize) -> Option<bool> {
    let m = t.len();
    let w = t.first().map_or(0, |r| r.len());
    for _ in 0..limit {
        // reduced costs r_j = c_j - c_B B^-1 a_j (the tableau holds B^-1 A)
        let entering = (0..cols).find(|&j| {
            if basis.contains(&j) {
                return false;
            }
            let mut r = cost[j];
            for i in 0..m {
                r -= cost[basis[i]] * t[i][j];
            }
            r < -1e-11
        });
        let Some(j) = entering else { return Some(true) };
        // ratio test, ties by the smallest basis index (Bland)
        let mut best: Option<(usize, f64)> = None;
        for i in 0..m {
            if t[i][j] > EPS {
                let ratio = t[i][w - 1] / t[i][j];
                best = match best {
                    None => Some((i, ratio)),
                    Some((k, r)) if ratio < r - 1e-12 || (ratio <= r + 1e-12 && basis[i] < basis[k]) => Some((i, ratio)),
                    keep => keep,
                };
            }
        }
        let Some((i, _)) = best else { return Some(false) };
        pivot(t, basis, i, j);
    }
    None
}

fn pivot(t: &mut [Vec<f64>], basis: &mut [usize], i: usize, j: usize) {
    let p = t[i][j];
    for v in t[i].iter_mut() {
        *v /= p;
    }
    let row = t[i].clone();
    for (k, r) in t.iter_mut().enumerate() {
        if k != i && r[j] != 0.0 {
            let f = r[j];
            for (v, &rv) in r.iter_mut().zip(&row) {
                *v -= f * rv;
            }
            r[j] = 0.0;
        }
    }
    basis[i] = j;
}

/// Minimise c·x subject to the rows, every variable free.
pub(crate) fn solve(c: &[f64], rows: &[Row]) -> Lp {
    let n = c.len();
    // columns: p (n), q (n), one slack per inequality
    let slacks = rows.iter().filter(|r| r.rel != Rel::Eq).count();
    let cols = 2 * n + slacks;
    let mut a = Vec::with_capacity(rows.len());
    let mut b = Vec::with_capacity(rows.len());
    let mut s = 0;
    for r in rows {
        let mut row = vec![0.0; cols];
        for k in 0..n {
            row[k] = r.a[k];
            row[n + k] = -r.a[k];
        }
        match r.rel {
            Rel::Le => {
                row[2 * n + s] = 1.0;
                s += 1;
            }
            Rel::Ge => {
                row[2 * n + s] = -1.0;
                s += 1;
            }
            Rel::Eq => {}
        }
        let mut rhs = r.b;
        if rhs < 0.0 {
            for v in row.iter_mut() {
                *v = -*v;
            }
            rhs = -rhs;
        }
        a.push(row);
        b.push(rhs);
    }
    let mut cc = vec![0.0; cols];
    for k in 0..n {
        cc[k] = c[k];
        cc[n + k] = -c[k];
    }
    let y = match simplex(&a, &b, &cc) {
        None | Some(Err(true)) => return Lp::Infeasible,
        Some(Err(false)) => return Lp::Unbounded,
        Some(Ok(y)) => y,
    };
    let mut x: Vec<f64> = (0..n).map(|k| y[k] - y[n + k]).collect();
    polish(&mut x, rows);
    let v = c.iter().zip(&x).map(|(a, b)| a * b).sum();
    Lp::Optimal(x, v)
}

/// The vertex solved again from its active rows (decision R.20): n independent active rows determine x; their
/// solution replaces x when it still satisfies every row.
fn polish(x: &mut [f64], rows: &[Row]) {
    let n = x.len();
    if n == 0 {
        return;
    }
    let resid = |x: &[f64], r: &Row| r.a.iter().zip(x).map(|(a, b)| a * b).sum::<f64>() - r.b;
    let scale = |r: &Row| 1.0 + r.b.abs() + r.a.iter().fold(0.0f64, |s, v| s.max(v.abs()));
    let mut active: Vec<&Row> = rows.iter().filter(|r| resid(x, r).abs() <= 1e-7 * scale(r)).collect();
    // equality rows first, then the inequalities, keeping an independent set by elimination
    active.sort_by_key(|r| r.rel != Rel::Eq);
    let mut chosen: Vec<&Row> = Vec::new();
    let mut basis: Vec<Vec<f64>> = Vec::new();
    for r in active {
        let mut v = r.a.clone();
        for bv in &basis {
            let piv = bv.iter().position(|e| e.abs() > 1e-12).unwrap_or(0);
            let f = v[piv] / bv[piv];
            for (a, b) in v.iter_mut().zip(bv) {
                *a -= f * b;
            }
        }
        let norm = v.iter().fold(0.0f64, |s, e| s.max(e.abs()));
        if norm > 1e-9 * scale(r) {
            basis.push(v);
            chosen.push(r);
            if chosen.len() == n {
                break;
            }
        }
    }
    if chosen.len() != n {
        return;
    }
    let m: Vec<f64> = chosen.iter().flat_map(|r| r.a.iter().copied()).collect();
    let rhs: Vec<f64> = chosen.iter().map(|r| r.b).collect();
    let Some(sol) = linalg::solve(n, &m, &rhs) else { return };
    let ok = rows.iter().all(|r| {
        let d = resid(&sol, r);
        let t = 1e-9 * scale(r);
        match r.rel {
            Rel::Le => d <= t,
            Rel::Ge => d >= -t,
            Rel::Eq => d.abs() <= t,
        }
    });
    if ok {
        x.copy_from_slice(&sol);
    }
}

/// The most nodes of one branch and bound.
pub(crate) const MAX_NODES: usize = 20_000;

/// Minimise c·x subject to the rows with the variables `int` integral; `None` when the node limit was reached
/// before the search finished (the best point found so far is then not proven optimal).
pub(crate) fn branch_and_bound(c: &[f64], rows: &[Row], int: &[bool], tick: &mut dyn FnMut() -> bool) -> Option<Lp> {
    let mut best: Option<(Vec<f64>, f64)> = None;
    let mut unbounded = false;
    let mut stack: Vec<Vec<Row>> = vec![rows.to_vec()];
    let mut nodes = 0;
    while let Some(rs) = stack.pop() {
        nodes += 1;
        if nodes > MAX_NODES || !tick() {
            return None;
        }
        match solve(c, &rs) {
            Lp::Infeasible => continue,
            Lp::Unbounded => {
                unbounded = true;
                continue;
            }
            Lp::Optimal(x, v) => {
                if best.as_ref().is_some_and(|(_, bv)| v >= *bv - 1e-12 * (1.0 + bv.abs())) {
                    continue;
                }
                // the most fractional integer variable
                let frac = (0..x.len())
                    .filter(|&k| int[k])
                    .map(|k| (k, (x[k] - x[k].round()).abs()))
                    .filter(|&(_, f)| f > 1e-7 * (1.0 + x[0].abs().min(1.0)))
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                match frac {
                    None => {
                        let mut xr = x.clone();
                        for k in 0..xr.len() {
                            if int[k] {
                                xr[k] = xr[k].round();
                            }
                        }
                        best = Some((xr, v));
                    }
                    Some((k, _)) => {
                        let mut e = vec![0.0; x.len()];
                        e[k] = 1.0;
                        let mut lo = rs.clone();
                        lo.push(Row { a: e.clone(), rel: Rel::Le, b: x[k].floor() });
                        let mut hi = rs;
                        hi.push(Row { a: e, rel: Rel::Ge, b: x[k].ceil() });
                        // the nearer branch first (pushed last)
                        if x[k] - x[k].floor() < 0.5 {
                            stack.push(hi);
                            stack.push(lo);
                        } else {
                            stack.push(lo);
                            stack.push(hi);
                        }
                    }
                }
            }
        }
    }
    Some(match best {
        Some((x, _)) => {
            // the value recomputed from the rounded point
            let v = c.iter().zip(&x).map(|(a, b)| a * b).sum();
            Lp::Optimal(x, v)
        }
        None if unbounded => Lp::Unbounded,
        None => Lp::Infeasible,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(a: &[f64], rel: Rel, b: f64) -> Row {
        Row { a: a.to_vec(), rel, b }
    }

    #[test]
    fn small_programs() {
        // min x + y, x + 2y >= 3, x >= -1  ->  x = -1, y = 2
        let r = solve(&[1.0, 1.0], &[row(&[1.0, 2.0], Rel::Ge, 3.0), row(&[1.0, 0.0], Rel::Ge, -1.0)]);
        assert_eq!(r, Lp::Optimal(vec![-1.0, 2.0], 1.0));
        // min x + y, 3x + 2y >= 7, x, y >= 0  ->  7/3
        let r = solve(&[1.0, 1.0], &[row(&[3.0, 2.0], Rel::Ge, 7.0), row(&[1.0, 0.0], Rel::Ge, 0.0), row(&[0.0, 1.0], Rel::Ge, 0.0)]);
        let Lp::Optimal(x, v) = r else { panic!() };
        assert_eq!(v, 7.0 / 3.0);
        assert_eq!(x, vec![7.0 / 3.0, 0.0]);
        // unbounded and infeasible
        assert_eq!(solve(&[1.0], &[row(&[1.0], Rel::Le, 3.0)]), Lp::Unbounded);
        assert_eq!(solve(&[1.0], &[row(&[1.0], Rel::Le, 3.0), row(&[1.0], Rel::Ge, 4.0)]), Lp::Infeasible);
    }

    #[test]
    fn integer_programs() {
        // min x + y, 3x + 2y >= 7, x, y >= 0, x integer -> 2.5 at x = 2, y = 0.5 (or x = 1, y = 2: 3)
        let rows = [row(&[3.0, 2.0], Rel::Ge, 7.0), row(&[1.0, 0.0], Rel::Ge, 0.0), row(&[0.0, 1.0], Rel::Ge, 0.0)];
        let r = branch_and_bound(&[1.0, 1.0], &rows, &[true, false], &mut || true).unwrap();
        let Lp::Optimal(x, v) = r else { panic!("{r:?}") };
        assert!(x[0] == 2.0 && (x[1] - 0.5).abs() < 1e-15 && (v - 2.5).abs() < 1e-15, "{x:?} {v}");
    }
}
