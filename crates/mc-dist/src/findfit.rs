//! `find_fit(data, model, params, vars)`: nonlinear least squares (gap queue G27, decision P6N.7).
//!
//! The data are `[x, y]` pairs, rows `[x1, ..., xk, y]` for k variables, or plain values y (x = 1, 2, ...). The
//! parameters are symbols (start 1.0) or `[p, start]` pairs. The fit minimizes the sum of squared residuals by
//! Levenberg-Marquardt (Marquardt's diagonal scaling, the damping divided by 10 after an accepted step and
//! multiplied by 10 after a rejected one) with a central-difference Jacobian, on machine reals; the model is
//! evaluated by substituting numbers and evaluating the term. The result is the list of rules `p -> value`; a model
//! that does not evaluate to real numbers gives `failure(not_numeric, model)`, and no convergence in 500
//! iterations `failure(not_converged, find_fit, rules)`.
use mc_api::{Store, View, T};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

/// t with the symbols `syms` replaced by `vals` (recursion bounded by the model's depth).
fn subst<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, syms: &[T<'h>], vals: &[T<'h>]) -> T<'h> {
    if let Some(i) = syms.iter().position(|&s| s == t) {
        return vals[i];
    }
    match fx.h.view(t) {
        View::App { head, args } => {
            let xs: Vec<T<'h>> = args.iter().map(|&a| subst(fx, a, syms, vals)).collect();
            if xs.as_slice() == args {
                t
            } else {
                fx.h.app(head, &xs)
            }
        }
        _ => t,
    }
}

struct Problem<'h> {
    model: T<'h>,
    /// the variables then the parameters
    syms: Vec<T<'h>>,
    xs: Vec<Vec<f64>>,
    ys: Vec<f64>,
}

impl<'h> Problem<'h> {
    /// The model at every data point for parameters p.
    fn values(&self, fx: &mut Fx<'_, '_, '_, 'h>, p: &[f64]) -> Res<Option<Vec<f64>>> {
        let mut out = Vec::with_capacity(self.ys.len());
        for x in &self.xs {
            let vals: Vec<T<'h>> = x.iter().chain(p.iter()).map(|&v| fx.real(v)).collect();
            let v = fx.eval(subst(fx, self.model, &self.syms, &vals))?;
            match fx.f64_of(v) {
                Some(v) if v.is_finite() => out.push(v),
                _ => return Ok(None),
            }
        }
        Ok(Some(out))
    }
}

fn sse(f: &[f64], y: &[f64]) -> f64 {
    f.iter().zip(y).map(|(a, b)| (b - a) * (b - a)).sum()
}

/// Solves the symmetric system a d = b (Gaussian elimination with partial pivoting); `None` when singular.
fn solve(mut a: Vec<Vec<f64>>, mut b: Vec<f64>) -> Option<Vec<f64>> {
    let n = b.len();
    for c in 0..n {
        let piv = (c..n).max_by(|&i, &j| a[i][c].abs().total_cmp(&a[j][c].abs()))?;
        if a[piv][c].abs() < 1e-300 {
            return None;
        }
        a.swap(c, piv);
        b.swap(c, piv);
        for r in c + 1..n {
            let f = a[r][c] / a[c][c];
            let (top, bottom) = a.split_at_mut(r);
            for (x, y) in bottom[0][c..n].iter_mut().zip(&top[c][c..n]) {
                *x -= f * y;
            }
            b[r] -= f * b[c];
        }
    }
    let mut x = vec![0.0; n];
    for c in (0..n).rev() {
        let s: f64 = (c + 1..n).map(|k| a[c][k] * x[k]).sum();
        x[c] = (b[c] - s) / a[c][c];
    }
    Some(x)
}

fn find_fit<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (data, model, params) = (a[0], a[1], a[2]);
    let vars: Vec<T<'h>> = match a.get(3) {
        Some(&v) => fx.as_list(v).map_or_else(|| vec![v], |l| l.to_vec()),
        None => vec![],
    };
    // parameters and start values
    let mut ps = vec![];
    let mut p0 = vec![];
    for &p in fx.as_list(params).map_or_else(|| vec![params], |l| l.to_vec()).iter() {
        match fx.as_list(p) {
            Some(&[s, v]) => match fx.eval(fx.call("n", &[v])).map(|nv| fx.f64_of(nv))? {
                Some(v) => {
                    ps.push(s);
                    p0.push(v);
                }
                None => return Ok(None),
            },
            _ => {
                ps.push(p);
                p0.push(1.0);
            }
        }
    }
    // the data rows
    let Some(rows) = fx.as_list(data) else { return Ok(None) };
    let k = vars.len();
    let (mut xs, mut ys) = (vec![], vec![]);
    for (i, &row) in rows.iter().enumerate() {
        let nrow = fx.eval(fx.call("n", &[row]))?;
        let vals: Vec<f64> = match fx.as_list(nrow) {
            Some(r) => r.iter().map(|&v| fx.f64_of(v)).collect::<Option<Vec<f64>>>().unwrap_or_default(),
            None => fx.f64_of(nrow).map(|v| vec![(i + 1) as f64, v]).unwrap_or_default(),
        };
        if vals.len() != k.max(1) + 1 && !(k == 0 && vals.len() == 2) {
            return Ok(Some(fx.failure(&["argument", "find_fit"])));
        }
        let (x, y) = vals.split_at(vals.len() - 1);
        xs.push(x[..k.min(x.len())].to_vec());
        ys.push(y[0]);
    }
    let mut syms = vars.clone();
    syms.extend(ps.iter().copied());
    let pr = Problem { model, syms, xs, ys };
    let np = ps.len();
    let mut p = p0;
    let Some(mut f) = pr.values(fx, &p)? else {
        return Ok(Some(fx.h.call(fx.h.sym("failure"), &[fx.sym("not_numeric"), model])));
    };
    let mut cost = sse(&f, &pr.ys);
    let mut derivs = vec![];
    for &s in &ps {
        let d = fx.eval(fx.call("d", &[model, s]))?;
        derivs.push((!fx.contains(d, "derivative") && !fx.contains(d, "d")).then_some(d));
    }
    let mut lambda = 1e-3;
    let mut converged = false;
    for _ in 0..500 {
        fx.tick(pr.ys.len() as u64 * (2 * np as u64 + 1))?;
        // the Jacobian: the symbolic derivatives when they exist, else central differences
        let mut jac = vec![vec![0.0; np]; pr.ys.len()];
        for j in 0..np {
            if let Some(dj) = derivs[j] {
                let dp = Problem { model: dj, syms: pr.syms.clone(), xs: pr.xs.clone(), ys: vec![] };
                if let Some(col) = dp.values(fx, &p)? {
                    for i in 0..pr.ys.len() {
                        jac[i][j] = col[i];
                    }
                    continue;
                }
            }
            let h = 1e-6 * p[j].abs().max(1e-3);
            let (mut up, mut dn) = (p.clone(), p.clone());
            up[j] += h;
            dn[j] -= h;
            let (Some(fu), Some(fd)) = (pr.values(fx, &up)?, pr.values(fx, &dn)?) else { break };
            for i in 0..pr.ys.len() {
                jac[i][j] = (fu[i] - fd[i]) / (2.0 * h);
            }
        }
        let r: Vec<f64> = pr.ys.iter().zip(&f).map(|(y, v)| y - v).collect();
        let mut jtj = vec![vec![0.0; np]; np];
        let mut jtr = vec![0.0; np];
        for i in 0..pr.ys.len() {
            for a in 0..np {
                jtr[a] += jac[i][a] * r[i];
                for b in 0..np {
                    jtj[a][b] += jac[i][a] * jac[i][b];
                }
            }
        }
        let mut accepted = false;
        for _ in 0..30 {
            let mut m = jtj.clone();
            for d in 0..np {
                m[d][d] += lambda * jtj[d][d].max(1e-12);
            }
            let Some(step) = solve(m, jtr.clone()) else {
                lambda = if lambda == 0.0 { 1e-9 } else { lambda * 10.0 };
                continue;
            };
            let q: Vec<f64> = p.iter().zip(&step).map(|(a, b)| a + b).collect();
            if let Some(fq) = pr.values(fx, &q)? {
                let c = sse(&fq, &pr.ys);
                // within rounding of the sum near the minimum, a step that does not increase it is accepted
                if c <= cost * (1.0 + 1e-13) {
                    let small = step.iter().zip(&q).all(|(s, v)| s.abs() <= 1e-12 * v.abs().max(1e-300));
                    let flat = cost - c <= 1e-14 * cost.max(1e-300);
                    p = q;
                    f = fq;
                    cost = c;
                    // down to pure Gauss-Newton steps near the solution
                    lambda = if lambda < 1e-9 { 0.0 } else { lambda / 10.0 };
                    accepted = true;
                    converged = small || (flat && lambda == 0.0);
                    break;
                }
            }
            lambda = if lambda == 0.0 { 1e-9 } else { lambda * 10.0 };
        }
        if !accepted || converged {
            converged = true;
            break;
        }
    }
    let rules: Vec<T<'h>> = ps.iter().zip(&p).map(|(&s, &v)| fx.h.call(fx.h.sym("rule"), &[s, fx.real(v)])).collect();
    let rules = fx.list(&rules);
    Ok(Some(if converged {
        rules
    } else {
        fx.h.call(fx.h.sym("failure"), &[fx.sym("not_converged"), fx.sym("find_fit"), rules])
    }))
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[Def {
    name: "find_fit",
    arities: &[3, 4],
    listable: false,
    exact: Some(find_fit),
    numeric: None,
    summary: "find_fit(data, model, params, x): the parameter values minimizing the squared residuals of model (Levenberg-Marquardt)",
}];
