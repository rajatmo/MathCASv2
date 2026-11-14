//! `interpolation(data)`: a not-a-knot cubic spline (or a piecewise-linear interpolant) through tabulated data, as a
//! callable value `interpolated(xs, ys, ms)` (ms: the second derivatives at the nodes; all zero for linear);
//! `interpolation(data, interpolation_order: k)`: the local polynomial of degree k through the k + 1 nearest nodes,
//! `interpolated(xs, ys, k)` (decision io-num.13). A fourth argument is a derivative order: `interpolated(xs, ys, ms,
//! d)` is the d-th derivative (mc-calculus turns `derivative(d)(interpolated(...))` into it). Data with derivatives
//! (`[x, f, f', ...]`, `[[x, y], f, gradient(, hessian)]`) give Hermite interpolants, `interpolated(xs, ys,
//! hermite(ds))` (see `hermite_data`).
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

/// Whether the value of a data item `[x, y]` (or the item itself) is a call holding a free symbol other than a
/// head, such as `interpolated(xs, ys, ms)(s)`: a value that may become a number, not malformed data.
fn symbolic_value<'h>(h: &'h mc_core::Heap, it: T<'h>) -> bool {
    let v = match h.view(it) {
        View::App { head, args: &[_, y] } if head == h.sym_term(h.sym(names::LIST)) => y,
        _ => it,
    };
    fn free<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
        match h.view(t) {
            View::Sym(_) => true,
            View::App { args, .. } => args.iter().any(|&a| free(h, a)),
            _ => false,
        }
    }
    matches!(h.view(v), View::App { .. }) && free(h, v)
}

fn reals<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let Some(items) = cx.as_list(t) else {
        return Ok(None);
    };
    let mut v = Vec::with_capacity(items.len());
    for &x in items {
        match real(cx, x)? {
            Some(x) => v.push(x),
            None => return Ok(None),
        }
    }
    Ok(Some(v))
}

/// [`reals`] of a list or of a packed array of reals.
fn real_vec<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    if let Some(d) = cx.heap().packed(t).and_then(|p| p.reals()) {
        return Ok(Some(d.to_vec()));
    }
    reals(cx, t)
}

fn bad<'h>(h: &'h mc_core::Heap, k: i64) -> Reply<'h> {
    Reply::Done(failure(
        h,
        "argument",
        &[h.sym_term(h.sym("interpolation")), h.int(k)],
    ))
}

impl Native for Interpolation {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (data, scheme) = match *args {
            [d] => (d, Scheme::Spline),
            [d, o] => match method(cx, o) {
                Some(l) => (d, l),
                None => return Ok(bad(h, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let Some(items) = cx.as_list(data) else {
            return Ok(bad(h, 1));
        };
        if args.len() == 1 {
            if let Some(r) = hermite_data(cx, items)? {
                return Ok(r);
            }
        }
        if let Some(r) = several(cx, items, scheme, args.get(1).copied())? {
            return Ok(r);
        }
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
                // a value with a free symbol (a symbolic point of the inner interpolants of a tensor product,
                // `interpolated(...)(s)`): no interpolant yet; it is built when the value is a number
                _ if symbolic_value(h, it) => return Ok(Reply::Decline),
                _ => return Ok(bad(h, 1)),
            }
        }
        Ok(match one(h, pts, scheme) {
            Some(t) => Reply::Done(t),
            None => bad(h, 1),
        })
    }
}

/// The interpolant of one variable through the points (x, y): `None` for fewer than two points or a repeated x.
fn one<'h>(h: &'h mc_core::Heap, mut pts: Vec<(f64, f64)>, scheme: Scheme) -> Option<T<'h>> {
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    if pts.len() < 2 || pts.windows(2).any(|w| w[0].0 == w[1].0) {
        // at least two points, at distinct x
        return None;
    }
    let xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let ms = if scheme == Scheme::Linear || xs.len() < 3 {
        vec![0.0; xs.len()]
    } else {
        not_a_knot_spline(&xs, &ys)
    };
    let list = |v: &[f64]| h.reals(v);
    if let Scheme::Local(k) = scheme {
        return Some(h.call(h.sym("interpolated"), &[list(&xs), list(&ys), h.int(k as i64)]));
    }
    Some(h.call(h.sym("interpolated"), &[list(&xs), list(&ys), list(&ms)]))
}

/// Data with vector values (`[x, [v1, ..., vk]]`) or with points of one or two coordinates (`[[x], v]`,
/// `[[x, y], v]`), round 36: a function of the coordinates (`fn([&1], ...)`, `fn([&1, &2], ...)`). Vector values
/// interpolate component by component (a list of the components). Two coordinates must form a full grid xs x ys:
/// the value at (x, y) is the interpolant in y through the values at x of the interpolants in x along each grid line
/// y = y_j, the tensor product of the one-variable scheme (de Boor, "A Practical Guide to Splines", chapter 17).
/// `None` when the data have scalar values at scalar points (the one-variable case).
fn several<'h>(cx: &mut NativeCx<'_, '_, 'h>, items: &[T<'h>], scheme: Scheme, opts: Option<T<'h>>) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let Some(&[a, b]) = items.first().and_then(|&f| cx.as_list(f)) else { return Ok(None) };
    let vector = cx.as_list(b).is_some();
    if cx.as_list(a).is_none() && !vector {
        return Ok(None);
    }
    let mut coords: Vec<Vec<f64>> = Vec::with_capacity(items.len());
    let mut vals: Vec<Vec<f64>> = Vec::with_capacity(items.len());
    for &it in items {
        let Some(&[p, v]) = cx.as_list(it) else { return Ok(Some(bad(h, 1))) };
        let c = if cx.as_list(p).is_some() { reals(cx, p)? } else { real(cx, p)?.map(|x| vec![x]) };
        let w = if cx.as_list(v).is_some() { reals(cx, v)? } else { real(cx, v)?.map(|x| vec![x]) };
        let (Some(c), Some(w)) = (c, w) else { return Ok(Some(bad(h, 1))) };
        if c.len() != coords.first().map_or(c.len(), Vec::len) || w.len() != vals.first().map_or(w.len(), Vec::len) || w.is_empty() {
            return Ok(Some(bad(h, 1)));
        }
        coords.push(c);
        vals.push(w);
    }
    let dim = coords[0].len();
    let k = vals[0].len();
    let slot = |i: usize| h.sym_term(h.sym(&format!("&{i}")));
    let mut comps = Vec::with_capacity(k);
    match dim {
        1 => {
            for j in 0..k {
                let pts: Vec<(f64, f64)> = coords.iter().zip(&vals).map(|(c, v)| (c[0], v[j])).collect();
                let Some(f) = one(h, pts, scheme) else { return Ok(Some(bad(h, 1))) };
                comps.push(h.app(f, &[slot(1)]));
            }
        }
        2 => {
            let mut xs: Vec<f64> = coords.iter().map(|c| c[0]).collect();
            let mut ys: Vec<f64> = coords.iter().map(|c| c[1]).collect();
            for v in [&mut xs, &mut ys] {
                v.sort_by(f64::total_cmp);
                v.dedup();
            }
            if xs.len() * ys.len() != coords.len() || xs.len() < 2 || ys.len() < 2 {
                return Ok(Some(bad(h, 1)));
            }
            // the grid of values: index (iy, ix); every node once
            let mut grid: Vec<Option<&[f64]>> = vec![None; xs.len() * ys.len()];
            for (c, v) in coords.iter().zip(&vals) {
                let ix = xs.partition_point(|&x| x < c[0]);
                let iy = ys.partition_point(|&y| y < c[1]);
                let cell = &mut grid[iy * xs.len() + ix];
                if cell.is_some() {
                    return Ok(Some(bad(h, 1)));
                }
                *cell = Some(v);
            }
            let interpolation = h.sym("interpolation");
            for j in 0..k {
                let mut lines = Vec::with_capacity(ys.len());
                for (iy, &y) in ys.iter().enumerate() {
                    let pts: Vec<(f64, f64)> =
                        xs.iter().enumerate().map(|(ix, &x)| (x, grid[iy * xs.len() + ix].map_or(0.0, |v| v[j]))).collect();
                    let Some(f) = one(h, pts, scheme) else { return Ok(Some(bad(h, 1))) };
                    lines.push(cx.list(&[h.real(y), h.app(f, &[slot(1)])]));
                }
                let mut iargs = vec![cx.list(&lines)];
                iargs.extend(opts);
                comps.push(h.app(h.call(interpolation, &iargs), &[slot(2)]));
            }
        }
        _ => return Ok(Some(bad(h, 1))),
    }
    let params: Vec<T<'h>> = (1..=dim).map(slot).collect();
    let body = if vector { cx.list(&comps) } else { comps[0] };
    Ok(Some(Reply::Done(h.call(h.sym("fn"), &[cx.list(&params), body]))))
}

/// Data with derivatives (Hermite interpolation): items `[x, f, f', f'', ...]` (x a number or `[x]`) give the
/// piecewise Hermite interpolant `interpolated(xs, ys, hermite(ds))`, ds the derivatives at each node
/// (`mc_numeric::spline::hermite`: on each interval the polynomial of degree 2m - 1 matching the m - 1 derivatives at
/// both ends). Items `[[x, y], f, [f_x, f_y]]` or `[[x, y], f, [f_x, f_y], [[f_xx, f_xy], [f_yx, f_yy]]]` on a full grid
/// give a function of two coordinates, Hermite in both directions (the tensor-product construction of the Hermite
/// bicubic patch: de Boor, "Bicubic spline interpolation", J. Math. Phys. 41, 1962; Burden and Faires, "Numerical
/// Analysis", section 3.4): along each grid line y = y_j the y-derivatives of order k = 0, 1 (and 2 with the Hessian)
/// are interpolated in x, Hermite where their x-derivatives are given (f_x, f_xx for k = 0, f_xy for k = 1) and by the
/// cubic spline otherwise; at a point (x, y) these give the values and y-derivatives at each y_j, interpolated in y by
/// the one-variable Hermite scheme. The result takes every given value and derivative at the nodes. `None` when the
/// items are not of these forms (the data then go to the other schemes).
fn hermite_data<'h>(cx: &mut NativeCx<'_, '_, 'h>, items: &[T<'h>]) -> Res<Option<Reply<'h>>> {
    let h = cx.heap();
    let Some(first) = items.first().and_then(|&f| cx.as_list(f)) else { return Ok(None) };
    if first.len() < 3 {
        return Ok(None);
    }
    let dim = cx.as_list(first[0]).map_or(1, <[T<'h>]>::len);
    let two = dim == 2 && (3..=4).contains(&first.len());
    if dim != 1 && !two {
        return Ok(None);
    }
    // a value that may become a number (an inner interpolant at a symbolic point): built later
    if items.iter().any(|&it| cx.as_list(it).is_some_and(|parts| parts[1..].iter().any(|&p| symbolic_value(h, p)))) {
        return Ok(Some(Reply::Decline));
    }
    if dim == 1 {
        let mut rows: Vec<(f64, f64, Vec<f64>)> = Vec::with_capacity(items.len());
        for &it in items {
            let Some(parts) = cx.as_list(it).filter(|p| p.len() == first.len()) else { return Ok(Some(bad(h, 1))) };
            let x = match cx.as_list(parts[0]) {
                Some(&[x]) => real(cx, x)?,
                Some(_) => None,
                None => real(cx, parts[0])?,
            };
            let mut vals = Vec::with_capacity(parts.len() - 1);
            for &p in &parts[1..] {
                match real(cx, p)? {
                    Some(v) => vals.push(v),
                    None => return Ok(Some(bad(h, 1))),
                }
            }
            let Some(x) = x else { return Ok(Some(bad(h, 1))) };
            let f = vals.remove(0);
            rows.push((x, f, vals));
        }
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        if rows.len() < 2 || rows.windows(2).any(|w| w[0].0 == w[1].0) {
            return Ok(Some(bad(h, 1)));
        }
        let xs: Vec<f64> = rows.iter().map(|r| r.0).collect();
        let ys: Vec<f64> = rows.iter().map(|r| r.1).collect();
        let ds: Vec<Vec<f64>> = rows.into_iter().map(|r| r.2).collect();
        return Ok(Some(Reply::Done(hermite_term(h, &xs, &ys, &ds))));
    }
    // two coordinates: [[x, y], f, [fx, fy]] and optionally [[fxx, fxy], [fyx, fyy]]
    struct Node {
        x: f64,
        y: f64,
        f: f64,
        g: [f64; 2],
        hess: Option<[f64; 3]>,
    }
    let mut nodes = Vec::with_capacity(items.len());
    for &it in items {
        let Some(parts) = cx.as_list(it).filter(|p| p.len() == first.len()) else { return Ok(Some(bad(h, 1))) };
        let (Some(c), Some(f), Some(g)) = (reals(cx, parts[0])?, real(cx, parts[1])?, reals(cx, parts[2])?) else {
            return Ok(Some(bad(h, 1)));
        };
        if c.len() != 2 || g.len() != 2 {
            return Ok(Some(bad(h, 1)));
        }
        let hess = match parts.get(3) {
            None => None,
            Some(&m) => {
                let rows: Option<Vec<T<'h>>> = cx.as_list(m).map(<[T<'h>]>::to_vec);
                let Some(rows) = rows.filter(|r| r.len() == 2) else { return Ok(Some(bad(h, 1))) };
                let (Some(r0), Some(r1)) = (reals(cx, rows[0])?, reals(cx, rows[1])?) else { return Ok(Some(bad(h, 1))) };
                if r0.len() != 2 || r1.len() != 2 {
                    return Ok(Some(bad(h, 1)));
                }
                Some([r0[0], r0[1], r1[1]])
            }
        };
        nodes.push(Node { x: c[0], y: c[1], f, g: [g[0], g[1]], hess });
    }
    let mut xs: Vec<f64> = nodes.iter().map(|n| n.x).collect();
    let mut ys: Vec<f64> = nodes.iter().map(|n| n.y).collect();
    for v in [&mut xs, &mut ys] {
        v.sort_by(f64::total_cmp);
        v.dedup();
    }
    if xs.len() * ys.len() != nodes.len() || xs.len() < 2 || ys.len() < 2 {
        return Ok(Some(bad(h, 1)));
    }
    let mut grid: Vec<Option<usize>> = vec![None; nodes.len()];
    for (k, nd) in nodes.iter().enumerate() {
        let cell = &mut grid[ys.partition_point(|&y| y < nd.y) * xs.len() + xs.partition_point(|&x| x < nd.x)];
        if cell.is_some() {
            return Ok(Some(bad(h, 1)));
        }
        *cell = Some(k);
    }
    let at = |iy: usize, ix: usize| &nodes[grid[iy * xs.len() + ix].expect("a full grid")];
    let with_hessian = nodes[0].hess.is_some();
    let slot = |i: usize| h.sym_term(h.sym(&format!("&{i}")));
    let mut lines = Vec::with_capacity(ys.len());
    for (iy, &y) in ys.iter().enumerate() {
        let row: Vec<&Node> = (0..xs.len()).map(|ix| at(iy, ix)).collect();
        let fv: Vec<f64> = row.iter().map(|n| n.f).collect();
        let fd: Vec<Vec<f64>> = row.iter().map(|n| match n.hess {
            Some(m) => vec![n.g[0], m[0]],
            None => vec![n.g[0]],
        }).collect();
        let gv: Vec<f64> = row.iter().map(|n| n.g[1]).collect();
        let mut item = vec![h.real(y), h.app(hermite_term(h, &xs, &fv, &fd), &[slot(1)])];
        let along = |vals: Vec<f64>, ds: Option<Vec<Vec<f64>>>| -> Option<T<'h>> {
            match ds {
                Some(ds) => Some(hermite_term(h, &xs, &vals, &ds)),
                None => one(h, xs.iter().copied().zip(vals).collect(), Scheme::Spline),
            }
        };
        let gd = with_hessian.then(|| row.iter().map(|n| vec![n.hess.map_or(0.0, |m| m[1])]).collect());
        let Some(g) = along(gv, gd) else { return Ok(Some(bad(h, 1))) };
        item.push(h.app(g, &[slot(1)]));
        if with_hessian {
            let hv: Vec<f64> = row.iter().map(|n| n.hess.map_or(0.0, |m| m[2])).collect();
            let Some(hh) = along(hv, None) else { return Ok(Some(bad(h, 1))) };
            item.push(h.app(hh, &[slot(1)]));
        }
        lines.push(cx.list(&item));
    }
    if nodes.iter().any(|n| n.hess.is_some() != with_hessian) {
        return Ok(Some(bad(h, 1)));
    }
    let inner = h.app(h.call(h.sym("interpolation"), &[cx.list(&lines)]), &[slot(2)]);
    Ok(Some(Reply::Done(h.call(h.sym("fn"), &[cx.list(&[slot(1), slot(2)]), inner]))))
}

/// `interpolated(xs, ys, hermite([[d1, ...], ...]))`: the Hermite interpolant with the derivatives at each node.
fn hermite_term<'h>(h: &'h mc_core::Heap, xs: &[f64], ys: &[f64], ds: &[Vec<f64>]) -> T<'h> {
    let rows: Vec<T<'h>> = ds.iter().map(|d| h.reals(d)).collect();
    let list = h.call(h.sym(names::LIST), &rows);
    h.call(h.sym("interpolated"), &[h.reals(xs), h.reals(ys), h.call(h.sym("hermite"), &[list])])
}

/// The interpolation scheme.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Scheme {
    /// the not-a-knot cubic spline (the default)
    Spline,
    /// straight segments
    Linear,
    /// the local polynomial of this degree
    Local(usize),
}

/// `options(method -> cubic | linear, interpolation_order -> k)`; `None` for anything else. The order 1 is the
/// linear scheme; an order with `method: cubic` is the spline.
fn method<'h>(cx: &NativeCx<'_, '_, 'h>, o: T<'h>) -> Option<Scheme> {
    let h = cx.heap();
    let View::App { head, args: rules } = h.view(o) else {
        return None;
    };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let mut scheme = Scheme::Spline;
    let mut order = None;
    for &r in rules {
        let View::App { head, args: &[k, v] } = h.view(r) else {
            return None;
        };
        if head != h.sym_term(h.sym(names::RULE)) {
            return None;
        }
        if k == h.sym_term(h.sym("method")) {
            scheme = match h.view(v) {
                View::Sym(s) if h.sym_name(s) == "linear" => Scheme::Linear,
                View::Sym(s) if h.sym_name(s) == "cubic" => Scheme::Spline,
                _ => return None,
            };
        } else if k == h.sym_term(h.sym("interpolation_order")) {
            match h.view(v) {
                View::Int(k @ 1..=20) => order = Some(k as usize),
                _ => return None,
            }
        } else {
            return None;
        }
    }
    Some(match (scheme, order) {
        (Scheme::Spline, Some(1)) => Scheme::Linear,
        (Scheme::Spline, Some(k)) => Scheme::Local(k),
        (s, _) => s,
    })
}

/// The second derivatives M_i of the not-a-knot cubic spline through (xs, ys), n >= 3 points.
///
/// Interior rows: h_{i-1} M_{i-1} + 2 (h_{i-1} + h_i) M_i + h_i M_{i+1} = 6 (s_i - s_{i-1}). The end conditions make
/// the third derivative continuous at the second and the second-to-last node, (M_1 - M_0)/h_0 = (M_2 - M_1)/h_1 and
/// likewise at the other end, so the first two and the last two pieces are one cubic each. Unlike the natural spline
/// (M = 0 at both ends) this does not force zero curvature at the ends: a function with y'' != 0 there (for example
/// a thermal integral near 0) is followed, and every cubic is reproduced exactly. M_0 and M_{n-1} are eliminated
/// into the first and last interior rows; the remaining tridiagonal system is solved by the Thomas algorithm.
/// Three points give the parabola through them.
fn not_a_knot_spline(xs: &[f64], ys: &[f64]) -> Vec<f64> {
    let n = xs.len();
    let hs: Vec<f64> = xs.windows(2).map(|w| w[1] - w[0]).collect();
    let slopes: Vec<f64> = (0..n - 1).map(|i| (ys[i + 1] - ys[i]) / hs[i]).collect();
    if n == 3 {
        return vec![2.0 * (slopes[1] - slopes[0]) / (xs[2] - xs[0]); 3];
    }
    // rows 1..=n-2 of the tridiagonal system: sub[i] M_{i-1} + diag[i] M_i + sup[i] M_{i+1} = rhs[i]
    let (mut sub, mut diag, mut sup, mut rhs) =
        (vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n]);
    for i in 1..n - 1 {
        sub[i] = hs[i - 1];
        diag[i] = 2.0 * (hs[i - 1] + hs[i]);
        sup[i] = hs[i];
        rhs[i] = 6.0 * (slopes[i] - slopes[i - 1]);
    }
    // M_0 = ((h_0 + h_1) M_1 - h_0 M_2)/h_1 into row 1
    let (h0, h1) = (hs[0], hs[1]);
    diag[1] += h0 * (h0 + h1) / h1;
    sup[1] -= h0 * h0 / h1;
    sub[1] = 0.0;
    // M_{n-1} = ((h_{n-2} + h_{n-3}) M_{n-2} - h_{n-2} M_{n-3})/h_{n-3} into row n-2
    let (ha, hb) = (hs[n - 3], hs[n - 2]);
    diag[n - 2] += hb * (hb + ha) / ha;
    sub[n - 2] -= hb * hb / ha;
    sup[n - 2] = 0.0;
    for i in 2..n - 1 {
        let w = sub[i] / diag[i - 1];
        diag[i] -= w * sup[i - 1];
        rhs[i] -= w * rhs[i - 1];
    }
    let mut m = vec![0.0; n];
    for i in (1..n - 1).rev() {
        m[i] = (rhs[i] - sup[i] * m[i + 1]) / diag[i];
    }
    m[0] = ((h0 + h1) * m[1] - h0 * m[2]) / h1;
    m[n - 1] = ((hb + ha) * m[n - 2] - hb * m[n - 3]) / ha;
    m
}

/// The curried call `interpolated(xs, ys, ms)(x)`.
pub(crate) struct Interpolated;

impl Native for Interpolated {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else {
            return Ok(Reply::Decline);
        };
        let View::App { args: data, .. } = h.view(head) else {
            return Ok(Reply::Decline);
        };
        let (xt, yt, mt, order) = match *data {
            [xt, yt, mt] => (xt, yt, mt, 0),
            [xt, yt, mt, d] => match h.view(d) {
                View::Int(d @ 0..=64) => (xt, yt, mt, d as u32),
                _ => return Ok(Reply::Decline),
            },
            _ => return Ok(Reply::Decline),
        };
        // the local scheme: the degree in place of the second derivatives
        let degree = match h.view(mt) {
            View::Int(k @ 1..=20) => Some(k as usize),
            _ => None,
        };
        let &[x] = args else {
            return Ok(Reply::Decline);
        };
        // the Hermite scheme: `hermite([[d1, ...], ...])`, the derivatives at each node
        if let View::App { head: hh, args: &[ds] } = h.view(mt) {
            if hh != h.sym_term(h.sym("hermite")) {
                return Ok(Reply::Decline);
            }
            let (Some(xs), Some(ys)) = (real_vec(cx, xt)?, real_vec(cx, yt)?) else { return Ok(Reply::Decline) };
            let Some(rows) = cx.as_list(ds).map(<[T<'h>]>::to_vec) else { return Ok(Reply::Decline) };
            let mut dv = Vec::with_capacity(rows.len());
            for r in rows {
                match real_vec(cx, r)? {
                    Some(v) => dv.push(v),
                    None => return Ok(Reply::Decline),
                }
            }
            if xs.len() < 2 || ys.len() != xs.len() || dv.len() != xs.len() {
                return Ok(Reply::Decline);
            }
            let at = |cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>| -> Res<Option<T<'h>>> {
                let Some(xv) = real(cx, p)? else { return Ok(None) };
                if !(xs[0]..=xs[xs.len() - 1]).contains(&xv) {
                    let dom = h.call(h.sym(names::LIST), &[h.real(xs[0]), h.real(xs[xs.len() - 1])]);
                    return Ok(Some(failure(h, "out_of_range", &[p, dom])));
                }
                Ok(mc_numeric::spline::hermite(&xs, &ys, &dv, xv, order).map(|v| h.real(v)))
            };
            if let Some(pts) = cx.as_list(x).map(<[T<'h>]>::to_vec) {
                let mut out = Vec::with_capacity(pts.len());
                for p in pts {
                    match at(cx, p)? {
                        Some(v) => out.push(v),
                        None => return Ok(Reply::Decline),
                    }
                }
                return Ok(Reply::Done(h.call(h.sym(names::LIST), &out)));
            }
            return Ok(at(cx, x)?.map_or(Reply::Decline, Reply::Done));
        }
        // the nodes are packed arrays (64 points or more): read in place, no copy per call
        let slice =
            |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<std::borrow::Cow<'h, [f64]>>> {
                if let Some(d) = h.packed(t).and_then(|p| p.reals()) {
                    return Ok(Some(std::borrow::Cow::Borrowed(d)));
                }
                Ok(reals(cx, t)?.map(std::borrow::Cow::Owned))
            };
        let ms = match degree {
            Some(_) => Some(std::borrow::Cow::Owned(vec![])),
            None => slice(cx, mt)?,
        };
        let (Some(xs), Some(ys), Some(ms)) = (slice(cx, xt)?, slice(cx, yt)?, ms) else {
            return Ok(Reply::Decline);
        };
        let ms: &[f64] = if degree.is_some() { &[] } else { &ms };
        let how = Eval { ms, degree, order };
        if xs.len() < 2 || xs.len() != ys.len() || (degree.is_none() && xs.len() != ms.len()) {
            return Ok(Reply::Decline);
        }
        // a list of points maps
        if let Some(pts) = cx.as_list(x) {
            let mut out = Vec::with_capacity(pts.len());
            for &p in pts {
                match value_at(cx, &xs, &ys, &how, p)? {
                    Some(v) => out.push(v),
                    None => return Ok(Reply::Decline),
                }
            }
            return Ok(Reply::Done(h.call(h.sym(names::LIST), &out)));
        }
        Ok(match value_at(cx, &xs, &ys, &how, x)? {
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

/// How an interpolant is evaluated: the spline's second derivatives or the local degree, and the derivative order.
struct Eval<'a> {
    ms: &'a [f64],
    degree: Option<usize>,
    order: u32,
}

/// The interpolant (or its derivative) at `x` (a failure outside the data's range), or `None` for a symbolic `x`.
fn value_at<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    xs: &[f64],
    ys: &[f64],
    how: &Eval<'_>,
    x: T<'h>,
) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let Some(xv) = real(cx, x)? else {
        return Ok(None);
    };
    let (lo, hi) = (xs[0], xs[xs.len() - 1]);
    if !(lo..=hi).contains(&xv) {
        let dom = h.call(h.sym(names::LIST), &[h.real(lo), h.real(hi)]);
        return Ok(Some(failure(h, "out_of_range", &[x, dom])));
    }
    // the evaluation compiled code shares (bit for bit the same value)
    Ok(match how.degree {
        Some(k) => mc_numeric::spline::local(xs, ys, k, xv, how.order),
        None => mc_numeric::spline::derivative(xs, ys, how.ms, xv, how.order),
    }
    .map(|v| h.real(v)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_spline_of_a_line_is_the_line() {
        let xs = [0.0, 1.0, 2.5, 4.0];
        let ys: Vec<f64> = xs.iter().map(|x| 3.0 * x - 1.0).collect();
        assert!(not_a_knot_spline(&xs, &ys).iter().all(|m| m.abs() < 1e-12));
    }

    /// Not-a-knot reproduces every cubic: the second derivatives are exact, also at the ends (a natural spline would
    /// give 0 there), on uneven nodes and for the smallest sizes.
    #[test]
    fn the_spline_of_a_cubic_is_the_cubic() {
        let p = |x: f64| 2.0 * x * x * x - 3.0 * x * x + 0.5 * x - 7.0;
        let p2 = |x: f64| 12.0 * x - 6.0;
        for xs in [
            vec![0.0, 1.0, 2.5, 4.0],
            vec![-1.0, -0.2, 0.3, 1.1, 2.0, 3.7, 4.0],
            vec![0.0, 0.5, 1.0, 1.5, 2.0],
        ] {
            let ys: Vec<f64> = xs.iter().map(|&x| p(x)).collect();
            let ms = not_a_knot_spline(&xs, &ys);
            for (x, m) in xs.iter().zip(&ms) {
                assert!((m - p2(*x)).abs() < 1e-9, "x = {x}: {m} vs {}", p2(*x));
            }
            // and the values between the nodes
            for k in 0..=40 {
                let x = xs[0] + (xs[xs.len() - 1] - xs[0]) * k as f64 / 40.0;
                let v = mc_numeric::spline::value(&xs, &ys, &ms, x).unwrap();
                assert!((v - p(x)).abs() < 1e-9, "x = {x}: {v} vs {}", p(x));
            }
        }
    }

    #[test]
    fn three_points_give_the_parabola() {
        let xs = [0.0, 1.0, 3.0];
        let ys: Vec<f64> = xs.iter().map(|x| 1.5 * x * x - x + 2.0).collect();
        assert!(not_a_knot_spline(&xs, &ys)
            .iter()
            .all(|m| (m - 3.0).abs() < 1e-12));
    }

    /// A function with curvature at the end of the table (y = x^2 + x^3 near 0, y''(0) = 2) is followed between the
    /// first nodes; the natural spline's y''(0) = 0 gave a large relative error there.
    #[test]
    fn curvature_at_the_ends_is_kept() {
        let f = |x: f64| x * x + x * x * x;
        let xs: Vec<f64> = (0..=50).map(|k| k as f64 * 0.02).collect();
        let ys: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
        let ms = not_a_knot_spline(&xs, &ys);
        for x in [0.003, 0.005, 0.01, 0.015, 0.97, 0.995] {
            let v = mc_numeric::spline::value(&xs, &ys, &ms, x).unwrap();
            assert!(((v - f(x)) / f(x)).abs() < 1e-9, "x = {x}: {v} vs {}", f(x));
        }
    }
}
