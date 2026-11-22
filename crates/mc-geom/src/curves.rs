//! Curves joined from pieces, `joined_curve([line(...), bezier_curve(...), b_spline_curve(...)])`: the curve runs
//! through the pieces in order, a straight segment joining the end of one piece to the start of the next where they
//! differ. A `bezier_curve` of k <= 4 control points is the Bezier curve of degree k - 1 (Bernstein form); a
//! `b_spline_curve` of k <= 4 control points with the default degree k - 1 is that same curve (clamped, one span).
//! Its length is the sum of the integrals of the speed of each polynomial piece over [0, 1], its centroid the
//! integrals of the point times the speed over the length, all by symbolic integration (exact data give exact
//! values; a piece whose integral has no closed form leaves the call unevaluated).
use crate::ex::E;
use crate::region::{call_parts, vectors, Pt};
use mc_core::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The polynomial pieces of a joined curve, each as its control points (two for a straight segment).
pub fn pieces<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Vec<Pt<'h>>>> {
    pieces_named(cx, t, "joined_curve")
}

/// The pieces of the boundary of a plane `filled_curve([...])` of one component: those of the joined curve and the
/// straight segment back from its end to its start.
pub fn filled_pieces<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Vec<Pt<'h>>>> {
    let mut ps = pieces_named(cx, t, "filled_curve")?;
    if ps[0][0].len() != 2 {
        return None;
    }
    let (first, last) = (ps[0][0].clone(), ps.last()?.last()?.clone());
    if first != last {
        ps.push(vec![last, first]);
    }
    Some(ps)
}

fn pieces_named<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, want: &str) -> Option<Vec<Vec<Pt<'h>>>> {
    let h = cx.heap();
    let (name, args) = call_parts(h, t)?;
    if name != want || args.len() != 1 {
        return None;
    }
    let items: Vec<T<'h>> = match cx.as_list(args[0]) {
        Some(l) => l.to_vec(),
        None => vec![args[0]],
    };
    let mut out: Vec<Vec<Pt<'h>>> = vec![];
    for it in items {
        let (n, a) = call_parts(h, it)?;
        let [p] = a else { return None };
        let pts = vectors(cx, *p)?;
        let segs: Vec<Vec<Pt<'h>>> = match n.as_str() {
            "line" if pts.len() >= 2 => pts.windows(2).map(|w| w.to_vec()).collect(),
            "bezier_curve" | "b_spline_curve" if (2..=4).contains(&pts.len()) => vec![pts],
            _ => return None,
        };
        // the straight join from the last end to this start
        if let (Some(last), Some(first)) = (out.last().and_then(|s| s.last()).cloned(), segs.first().and_then(|s| s.first()).cloned()) {
            if last != first {
                out.push(vec![last, first]);
            }
        }
        out.extend(segs);
    }
    (!out.is_empty() && out.iter().all(|s| s.iter().all(|p| p.len() == out[0][0].len()))).then_some(out)
}

/// The point at parameter `t` of the Bezier curve with these control points (Bernstein form).
fn point<'h>(e: E<'h>, ctrl: &[Pt<'h>], t: T<'h>) -> Vec<T<'h>> {
    let k = ctrl.len() - 1;
    let binom = |i: usize| -> i64 { (0..i).fold(1i64, |acc, j| acc * (k - j) as i64 / (j + 1) as i64) };
    (0..ctrl[0].len())
        .map(|d| {
            let terms: Vec<T<'h>> = (0..=k)
                .map(|i| e.mul(&[e.int(binom(i)), e.pow(t, e.int(i as i64)), e.pow(e.sub(e.int(1), t), e.int((k - i) as i64)), ctrl[i][d]]))
                .collect();
            e.add(&terms)
        })
        .collect()
}

/// The signed area and the centroid of the region a closed plane curve of polynomial pieces bounds, by Green's
/// theorem: A = 1/2 ∮ (x dy - y dx), A x̄ = 1/2 ∮ x^2 dy, A ȳ = -1/2 ∮ y^2 dx, each piece integrated exactly over
/// [0, 1] (the integrands are polynomials). The area is returned without its sign (the sign only tells the sense
/// of the curve); `None` when it is zero.
pub fn filled_area_and_centroid<'h>(cx: &mut NativeCx<'_, '_, 'h>, ps: &[Vec<Pt<'h>>]) -> Res<Option<(T<'h>, [T<'h>; 2])>> {
    let h = cx.heap();
    let e = E { h };
    let t = e.sym("curve$t");
    let range = e.list(&[t, e.int(0), e.int(1)]);
    let (mut a, mut mx, mut my) = (vec![], vec![], vec![]);
    for ctrl in ps {
        cx.tick(1)?;
        let p = point(e, ctrl, t);
        let (dx, dy) = (e.f("d", &[p[0], t]), e.f("d", &[p[1], t]));
        let mut int = |f: T<'h>| -> Res<T<'h>> { cx.eval(e.f("integrate", &[e.f("expand", &[f]), range])) };
        a.push(int(e.sub(e.mul(&[p[0], dy]), e.mul(&[p[1], dx])))?);
        mx.push(int(e.mul(&[e.sq(p[0]), dy]))?);
        my.push(int(e.mul(&[e.sq(p[1]), dx]))?);
    }
    let a2 = cx.eval(e.f("simplify", &[e.add(&a)]))?;
    let Some(v) = crate::ex::num(cx, a2)? else { return Ok(None) };
    if v == 0.0 {
        return Ok(None);
    }
    let cx_ = cx.eval(e.f("simplify", &[e.div(e.add(&mx), a2)]))?;
    let cy_ = cx.eval(e.f("simplify", &[e.neg(e.div(e.add(&my), a2))]))?;
    let area = cx.eval(e.f("simplify", &[e.mul(&[e.rat(1, 2), e.f("abs", &[a2])])]))?;
    Ok(Some((area, [cx_, cy_])))
}

/// The control points of the pieces as machine numbers.
pub fn numeric_pieces<'h>(cx: &mut NativeCx<'_, '_, 'h>, ps: &[Vec<Pt<'h>>]) -> Res<Option<Vec<Vec<Vec<f64>>>>> {
    let mut out = vec![];
    for p in ps {
        match crate::ex::points(cx, p)? {
            Some(v) => out.push(v),
            None => return Ok(None),
        }
    }
    Ok(Some(out))
}

/// The point at `t` of a Bezier piece of machine numbers (de Casteljau).
pub fn eval_piece(ctrl: &[Vec<f64>], t: f64) -> Vec<f64> {
    let mut b: Vec<Vec<f64>> = ctrl.to_vec();
    for k in (1..b.len()).rev() {
        for i in 0..k {
            for d in 0..b[i].len() {
                b[i][d] = (1.0 - t) * b[i][d] + t * b[i + 1][d];
            }
        }
    }
    b.swap_remove(0)
}

/// The distance from `p` to a polynomial piece: the least |B(t) - p| over [0, 1], found by sampling and then
/// refining each sampled local minimum by golden-section search (|B(t) - p|^2 is a polynomial of degree at most
/// 6, so 64 samples separate its local minima for the pieces of at most four control points).
pub fn piece_distance(ctrl: &[Vec<f64>], p: &[f64]) -> f64 {
    let f = |t: f64| eval_piece(ctrl, t).iter().zip(p).map(|(a, b)| (a - b) * (a - b)).sum::<f64>();
    if ctrl.len() == 2 {
        // a segment: the projection, clamped
        let d: Vec<f64> = ctrl[1].iter().zip(&ctrl[0]).map(|(a, b)| a - b).collect();
        let dd: f64 = d.iter().map(|x| x * x).sum();
        let t = if dd == 0.0 { 0.0 } else { (p.iter().zip(&ctrl[0]).zip(&d).map(|((a, b), c)| (a - b) * c).sum::<f64>() / dd).clamp(0.0, 1.0) };
        return f(t).sqrt();
    }
    let n = 64;
    let vals: Vec<f64> = (0..=n).map(|i| f(i as f64 / n as f64)).collect();
    let mut best = vals.iter().copied().fold(f64::INFINITY, f64::min);
    for i in 0..=n {
        let left = if i == 0 { f64::INFINITY } else { vals[i - 1] };
        let right = if i == n { f64::INFINITY } else { vals[i + 1] };
        if vals[i] <= left && vals[i] <= right {
            let (mut a, mut b) = (((i as f64) - 1.0).max(0.0) / n as f64, ((i as f64) + 1.0).min(n as f64) / n as f64);
            let g = 0.5 * (5f64.sqrt() - 1.0);
            for _ in 0..200 {
                let (c, d) = (b - g * (b - a), a + g * (b - a));
                if f(c) < f(d) {
                    b = d;
                } else {
                    a = c;
                }
                if b - a < 1e-16 {
                    break;
                }
            }
            best = best.min(f(0.5 * (a + b)));
        }
    }
    best.sqrt()
}

/// The winding number of a closed curve of polynomial pieces about `p`, from a polyline of 256 points a piece; the
/// caller makes sure that `p` is far from the curve compared with the polyline's error.
pub fn winding_number(ps: &[Vec<Vec<f64>>], p: &[f64]) -> i64 {
    let mut pts = vec![];
    for c in ps {
        let k = if c.len() == 2 { 1 } else { 256 };
        for i in 0..k {
            pts.push(eval_piece(c, i as f64 / k as f64));
        }
    }
    let mut w = 0i64;
    for i in 0..pts.len() {
        let (a, b) = (&pts[i], &pts[(i + 1) % pts.len()]);
        let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
        if a[1] <= p[1] {
            if b[1] > p[1] && cross > 0.0 {
                w += 1;
            }
        } else if b[1] <= p[1] && cross < 0.0 {
            w -= 1;
        }
    }
    w
}

/// A bound for the distance between a closed curve of polynomial pieces and its polyline of 256 points a piece:
/// the chord error of each piece, max |B''| / 8 / 256^2 (B'' is bounded by k (k - 1) times the largest second
/// difference of the control points for k + 1 of them).
pub fn polyline_error(ps: &[Vec<Vec<f64>>]) -> f64 {
    let mut err = 0.0f64;
    for c in ps {
        let k = c.len() - 1;
        if k < 2 {
            continue;
        }
        let mut m = 0.0f64;
        for i in 0..k - 1 {
            let s: f64 = (0..c[i].len()).map(|d| (c[i + 2][d] - 2.0 * c[i + 1][d] + c[i][d]).powi(2)).sum::<f64>().sqrt();
            m = m.max(s);
        }
        err = err.max((k * (k - 1)) as f64 * m / 8.0 / (256.0 * 256.0));
    }
    err
}

/// `(length, [moments])` of a joined curve: the integrals of the speed and of each coordinate times the speed.
pub fn length_and_moments<'h>(cx: &mut NativeCx<'_, '_, 'h>, ps: &[Vec<Pt<'h>>], moments: bool) -> Res<Option<(T<'h>, Vec<T<'h>>)>> {
    let h = cx.heap();
    let e = E { h };
    let t = e.sym("curve$t");
    let n = ps[0][0].len();
    let mut len = vec![];
    let mut mom: Vec<Vec<T<'h>>> = vec![vec![]; n];
    let unevaluated = |v: T<'h>| match call_parts(h, v) {
        Some((name, _)) => name == "integrate" || name == "failure",
        None => false,
    };
    for ctrl in ps {
        cx.tick(1)?;
        let p = point(e, ctrl, t);
        let speed2: Vec<T<'h>> = p.iter().map(|&c| e.sq(e.f("d", &[c, t]))).collect();
        let speed = cx.eval(e.sqrt(e.f("expand", &[e.add(&speed2)])))?;
        let range = e.list(&[t, e.int(0), e.int(1)]);
        let l = cx.eval(e.f("integrate", &[speed, range]))?;
        if unevaluated(l) {
            return Ok(None);
        }
        len.push(l);
        if moments {
            for d in 0..n {
                let m = cx.eval(e.f("integrate", &[e.f("expand", &[e.mul(&[p[d], speed])]), range]))?;
                if unevaluated(m) {
                    return Ok(None);
                }
                mom[d].push(m);
            }
        }
    }
    let total = cx.eval(e.f("simplify", &[e.add(&len)]))?;
    let ms = mom.iter().map(|m| e.add(m)).collect();
    if matches!(h.view(total), View::Int(0)) {
        return Ok(None);
    }
    Ok(Some((total, ms)))
}
