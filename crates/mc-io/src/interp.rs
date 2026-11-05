//! `interpolation(data)`: a natural cubic spline (or a piecewise-linear interpolant) through tabulated data, as a
//! callable value `interpolated(xs, ys, ms)` (ms: the second derivatives at the nodes; all zero for linear).
use crate::failure;
use mc_api::names;
use mc_core::{Store, View, T};
use mc_eval::{Native, NativeCx, Reply, Res};

/// `interpolation(data)`, `interpolation(data, method: cubic | linear)`.
pub(crate) struct Interpolation;

/// A machine real for a number or a closed numeric expression.
fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let direct = |t: T<'h>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    if let Some(x) = direct(t) {
        return Ok(Some(x).filter(|x| x.is_finite()));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(direct(v).filter(|x| x.is_finite()))
}

fn reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let Some(items) = cx.as_list(t) else { return Ok(None) };
    let mut v = Vec::with_capacity(items.len());
    for &x in items {
        match real(cx, x)? {
            Some(x) => v.push(x),
            None => return Ok(None),
        }
    }
    Ok(Some(v))
}

fn bad<'h>(h: &'h mc_core::Heap, k: i64) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym("interpolation")), h.int(k)]))
}

impl Native for Interpolation {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (data, linear) = match *args {
            [d] => (d, false),
            [d, o] => match method(cx, o) {
                Some(l) => (d, l),
                None => return Ok(bad(h, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(items) = cx.as_list(data) else { return Ok(bad(h, 1)) };
        let mut pts = Vec::with_capacity(items.len());
        for (i, &it) in items.iter().enumerate() {
            if i % 4096 == 0 {
                cx.tick(1)?;
            }
            let p = match cx.as_list(it) {
                Some(&[px, py]) => (real(cx, px)?, real(cx, py)?),
                Some(_) => return Ok(bad(h, 1)),
                None => (Some(i as f64 + 1.0), real(cx, it)?),
            };
            match p {
                (Some(x), Some(y)) => pts.push((x, y)),
                _ => return Ok(bad(h, 1)),
            }
        }
        pts.sort_by(|a, b| a.0.total_cmp(&b.0));
        if pts.len() < 2 || pts.windows(2).any(|w| w[0].0 == w[1].0) {
            // at least two points, at distinct x
            return Ok(bad(h, 1));
        }
        let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
        let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
        let ms = if linear || xs.len() < 3 { vec![0.0; xs.len()] } else { natural_spline(&xs, &ys) };
        let list = |v: &[f64]| h.reals(v);
        Ok(Reply::Done(h.call(h.sym("interpolated"), &[list(&xs), list(&ys), list(&ms)])))
    }
}

/// `options(method -> cubic | linear)`: `Some(true)` for linear.
fn method<'h>(cx: &NativeCx<'_, '_, 'h>, o: T<'h>) -> Option<bool> {
    let h = cx.heap();
    let View::App { head, args: &[r] } = h.view(o) else { return None };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let View::App { head, args: &[k, v] } = h.view(r) else { return None };
    if head != h.sym_term(h.sym(names::RULE)) || k != h.sym_term(h.sym("method")) {
        return None;
    }
    match h.view(v) {
        View::Sym(s) if h.sym_name(s) == "linear" => Some(true),
        View::Sym(s) if h.sym_name(s) == "cubic" => Some(false),
        _ => None,
    }
}

/// The second derivatives of the natural cubic spline through (xs, ys) (zero at both ends): the tridiagonal system
/// h_{i-1} M_{i-1} + 2 (h_{i-1} + h_i) M_i + h_i M_{i+1} = 6 (s_i - s_{i-1}), solved by the Thomas algorithm.
fn natural_spline(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = xs.len();
    let hs: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let slopes: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / hs[i]).collect();
    let mut diag = vec![0.0; n];
    let mut rhs = vec![0.0; n];
    let mut sup = vec![0.0; n];
    for i in 1..n - 1 {
        diag[i] = 2.0 * (hs[i - 1] + hs[i]);
        sup[i] = hs[i];
        rhs[i] = 6.0 * (slopes[i] - slopes[i - 1]);
    }
    // forward elimination over the interior rows 1..n-1 (M_0 = M_{n-1} = 0)
    for i in 2..n - 1 {
        let w = hs[i - 1] / diag[i - 1];
        diag[i] -= w * sup[i - 1];
        rhs[i] -= w * rhs[i - 1];
    }
    let mut m = vec![0.0; n];
    for i in (1..n - 1).rev() {
        m[i] = (rhs[i] - sup[i] * m[i + 1]) / diag[i];
    }
    m
}

/// The curried call `interpolated(xs, ys, ms)(x)`.
pub(crate) struct Interpolated;

impl Native for Interpolated {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        let View::App { args: &[xt, yt, mt], .. } = h.view(head) else { return Ok(Reply::Decline) };
        let &[x] = args else { return Ok(Reply::Decline) };
        // the nodes are packed arrays (64 points or more): read in place, no copy per call
        let slice = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<std::borrow::Cow<'h, [f64]>>> {
            if let Some(d) = h.packed(t).and_then(|p| p.reals()) {
                return Ok(Some(std::borrow::Cow::Borrowed(d)));
            }
            Ok(reals(cx, t)?.map(std::borrow::Cow::Owned))
        };
        let (Some(xs), Some(ys), Some(ms)) = (slice(cx, xt)?, slice(cx, yt)?, slice(cx, mt)?) else {
            return Ok(Reply::Decline);
        };
        if xs.len() < 2 || xs.len() != ys.len() || xs.len() != ms.len() {
            return Ok(Reply::Decline);
        }
        // a list of points maps
        if let Some(pts) = cx.as_list(x) {
            let mut out = Vec::with_capacity(pts.len());
            for &p in pts {
                match value_at(cx, &xs, &ys, &ms, p)? {
                    Some(v) => out.push(v),
                    None => return Ok(Reply::Decline),
                }
            }
            return Ok(Reply::Done(h.call(h.sym(names::LIST), &out)));
        }
        Ok(match value_at(cx, &xs, &ys, &ms, x)? {
            Some(v) => Reply::Done(v),
            None => Reply::Decline,
        })
    }

    fn curried(&self) -> bool {
        true
    }

    fn reads_term(&self) -> bool {
        true
    }
}

/// The interpolant at `x` (a failure outside the data's range), or `None` for a symbolic `x`.
fn value_at<'h>(cx: &mut NativeCx<'_, '_, 'h>, xs: &[f64], ys: &[f64], ms: &[f64], x: T<'h>) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some(xv) = real(cx, x)? else { return Ok(None) };
    let (lo, hi) = (xs[0], xs[xs.len() - 1]);
    if !(lo..=hi).contains(&xv) {
        let dom = h.call(h.sym(names::LIST), &[h.real(lo), h.real(hi)]);
        return Ok(Some(failure(h, "out_of_range", &[x, dom])));
    }
    // the interval [x_i, x_{i+1}] holding xv
    let i = xs.partition_point(|&a| a <= xv).clamp(1, xs.len() - 1) - 1;
    let hi_ = xs[i + 1] - xs[i];
    let (a, b) = ((xs[i + 1] - xv) / hi_, (xv - xs[i]) / hi_);
    let v = a * ys[i] + b * ys[i + 1] + ((a * a * a - a) * ms[i] + (b * b * b - b) * ms[i + 1]) * hi_ * hi_ / 6.0;
    Ok(Some(h.real(v)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spline_of_a_line_is_the_line() {
        let xs = [0.0, 1.0, 2.5, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * x - 1.0).collect();
        assert!(natural_spline(&xs, &ys).iter().all(|m| m.abs() < 1e-12));
    }
}
