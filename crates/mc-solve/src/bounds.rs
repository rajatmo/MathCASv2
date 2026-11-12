//! Bounds on integer unknowns from equations (for the box search of `solve` over the integers).
//!
//! An equation `sum a_i m_i = C` whose monomials `m_i` are all nonnegative (each unknown in them has an even exponent
//! or a lower bound of at least 0) and whose coefficients `a_i` have the sign of C bounds every unknown v that has a
//! pure power `a v^k` among its terms: `|v| <= (C/a)^(1/k)` (the other terms only add to it). So `x^2 + 2 y^3 = 3681`
//! with `x, y > 0` gives `x <= 60`, `y <= 12`.
use crate::cx::Env;
use mc_api::{Store, View, T};
use mc_core::Heap;
use mc_poly::ratfun;
use mc_poly::work::{Meter, Unlimited};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The bound on |v| for each unknown that one of the equations gives.
pub(crate) fn equation_bounds<'h>(h: &'h Heap, vars: &[T<'h>], los: &[Option<i64>], equations: &[T<'h>]) -> Vec<Option<i64>> {
    let env = Env::new(h, Arc::new(AtomicBool::new(false)), "solve");
    let mut work = Unlimited;
    let mut m = Meter::new(&mut work);
    let mut out: Vec<Option<i64>> = vec![None; vars.len()];
    for &eq in equations {
        let View::App { args: &[l, r], .. } = h.view(eq) else { continue };
        let d = env.ar.sub(l, r);
        let Ok(ring) = env.ring(&[d], vars, &mut m) else { continue };
        // every other kernel a number: integer coefficients only
        let idx: Vec<usize> = vars.iter().filter_map(|&v| ring.var(v)).collect();
        if idx.len() != vars.len() || ring.kernels.iter().enumerate().any(|(i, _)| !idx.contains(&i)) {
            continue;
        }
        let Ok(Ok(f)) = ratfun::read(&env.rconv(), d, &ring, &mut m) else { continue };
        if !f.den.is_constant() {
            continue;
        }
        let p = f.num;
        let c0 = -p.constant_term().as_i64().map(|c| c as f64).unwrap_or(f64::NAN);
        if !(c0.is_finite() && c0 != 0.0) {
            continue;
        }
        let sign = c0.signum();
        let mut ok = true;
        let mut pure: Vec<(usize, f64, u32)> = vec![];
        for (e, c) in p.terms() {
            if e.iter().all(|&k| k == 0) {
                continue;
            }
            let Some(a) = c.as_i64() else {
                ok = false;
                break;
            };
            if (a as f64).signum() != sign {
                ok = false;
                break;
            }
            // nonnegative monomial
            for (j, &v) in idx.iter().enumerate() {
                if e[v] % 2 == 1 && !los[j].is_some_and(|lo| lo >= 0) {
                    ok = false;
                }
            }
            let used: Vec<usize> = idx.iter().enumerate().filter(|(_, &v)| e[v] > 0).map(|(j, _)| j).collect();
            if let [j] = used.as_slice() {
                pure.push((*j, (a as f64).abs(), e[idx[*j]]));
            }
        }
        if !ok {
            continue;
        }
        for (j, a, k) in pure {
            let b = (c0.abs() / a).powf(1.0 / k as f64);
            if b.is_finite() && b < 1e9 {
                // a little slack against rounding: the search checks every point exactly
                let b = b.floor() as i64 + 1;
                out[j] = Some(out[j].map_or(b, |o: i64| o.min(b)));
            }
        }
    }
    out
}
