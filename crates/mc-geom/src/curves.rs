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
    let h = cx.heap();
    let (name, args) = call_parts(h, t)?;
    if name != "joined_curve" || args.len() != 1 {
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
