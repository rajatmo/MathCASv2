//! Boolean operations on plane polygons (union, intersection, difference) through the arrangement of their
//! edges: every edge of both polygons is split where it meets another, each piece is kept when the result lies
//! on exactly one side of it (tested just left and right of its midpoint), oriented with the result on its left,
//! and the kept pieces are linked into rings. Anticlockwise rings are outer boundaries, clockwise ones holes.
//! Works for any simple or self-touching polygons, with holes, and for several pieces; coordinates are machine
//! numbers.

/// A plane point.
pub type P = [f64; 2];

/// The Boolean operation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// in either
    Union,
    /// in both
    Intersection,
    /// in the first and not the second
    Difference,
}

impl Op {
    fn apply(self, a: bool, b: bool) -> bool {
        match self {
            Op::Union => a || b,
            Op::Intersection => a && b,
            Op::Difference => a && !b,
        }
    }
}

/// Strictly inside (even-odd over all rings); points on an edge are not tested by the callers.
fn inside(p: P, rings: &[Vec<P>]) -> bool {
    let mut c = false;
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            if (a[1] > p[1]) != (b[1] > p[1]) {
                let x = a[0] + (p[1] - a[1]) / (b[1] - a[1]) * (b[0] - a[0]);
                if p[0] < x {
                    c = !c;
                }
            }
        }
    }
    c
}

fn cross(o: P, a: P, b: P) -> f64 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// The parameters (in (0, 1)) where segment pq is cut by segment rs: a proper crossing, or the ends of rs lying
/// on pq (touching and overlapping edges).
fn cuts(p: P, q: P, r: P, s: P, eps: f64, out: &mut Vec<f64>) {
    let d = [q[0] - p[0], q[1] - p[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 == 0.0 {
        return;
    }
    let on = |x: P| -> Option<f64> {
        let c = cross(p, q, x);
        if c.abs() > eps * len2.sqrt() {
            return None;
        }
        let t = ((x[0] - p[0]) * d[0] + (x[1] - p[1]) * d[1]) / len2;
        (t > 0.0 && t < 1.0).then_some(t)
    };
    out.extend(on(r));
    out.extend(on(s));
    let e = [s[0] - r[0], s[1] - r[1]];
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() <= 1e-300 {
        return;
    }
    let w = [r[0] - p[0], r[1] - p[1]];
    let t = (w[0] * e[1] - w[1] * e[0]) / den;
    let u = (w[0] * d[1] - w[1] * d[0]) / den;
    if t > 0.0 && t < 1.0 && (-1e-12..=1.0 + 1e-12).contains(&u) {
        out.push(t);
    }
}

/// The signed area of a ring (anticlockwise positive).
pub fn area(r: &[P]) -> f64 {
    let n = r.len();
    (0..n).map(|i| r[i][0] * r[(i + 1) % n][1] - r[(i + 1) % n][0] * r[i][1]).sum::<f64>() / 2.0
}

/// The result: each outer ring with its holes.
pub type Shapes = Vec<(Vec<P>, Vec<Vec<P>>)>;

/// `a op b`, each given as rings filled by the even-odd rule.
pub fn boolean(a: &[Vec<P>], b: &[Vec<P>], op: Op) -> Shapes {
    boolean_fill(a, b, op, Fill::EvenOdd)
}

/// How a closed curve fills the plane.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fill {
    /// a point is inside when a ray from it crosses the curve an odd number of times
    EvenOdd,
    /// a point is inside when the curve winds about it a non-zero number of times
    NonZero,
    /// inside when the curve winds about it a positive number of times (anticlockwise)
    Positive,
    /// inside when the curve winds about it a negative number of times (clockwise)
    Negative,
}

/// The winding number of the closed rings about `p` (anticlockwise turns positive).
fn winding(p: P, rings: &[Vec<P>]) -> i32 {
    let mut w = 0i32;
    for r in rings {
        let n = r.len();
        for i in 0..n {
            let (a, b) = (r[i], r[(i + 1) % n]);
            let side = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
            if a[1] <= p[1] && b[1] > p[1] && side > 0.0 {
                w += 1;
            } else if a[1] > p[1] && b[1] <= p[1] && side < 0.0 {
                w -= 1;
            }
        }
    }
    w
}

/// Inside by the fill rule (strictly; points on an edge are not tested by the callers).
fn inside_fill(p: P, rings: &[Vec<P>], fill: Fill) -> bool {
    match fill {
        Fill::EvenOdd => inside(p, rings),
        Fill::NonZero => winding(p, rings) != 0,
        Fill::Positive => winding(p, rings) > 0,
        Fill::Negative => winding(p, rings) < 0,
    }
}

/// `a op b` with the first set filled by `fill` (`b` always by the even-odd rule).
pub fn boolean_fill(a: &[Vec<P>], b: &[Vec<P>], op: Op, fill: Fill) -> Shapes {
    let all: Vec<P> = a.iter().chain(b).flatten().copied().collect();
    let scale = all.iter().fold(0.0f64, |m, p| m.max(p[0].abs()).max(p[1].abs())).max(1e-300);
    let eps = 1e-10 * scale;
    let mut segs: Vec<(P, P)> = vec![];
    for r in a.iter().chain(b) {
        let n = r.len();
        for i in 0..n {
            let (p, q) = (r[i], r[(i + 1) % n]);
            if (p[0] - q[0]).abs() > eps || (p[1] - q[1]).abs() > eps {
                segs.push((p, q));
            }
        }
    }
    // vertices, merged when closer than eps
    let mut verts: Vec<P> = vec![];
    let vid = |p: P, verts: &mut Vec<P>| -> usize {
        if let Some(k) = verts.iter().position(|v| (v[0] - p[0]).abs() <= eps && (v[1] - p[1]).abs() <= eps) {
            return k;
        }
        verts.push(p);
        verts.len() - 1
    };
    let mut edges: Vec<(usize, usize)> = vec![];
    for (i, &(p, q)) in segs.iter().enumerate() {
        let mut ts = vec![0.0, 1.0];
        for (j, &(r, s)) in segs.iter().enumerate() {
            if i != j {
                cuts(p, q, r, s, eps, &mut ts);
            }
        }
        ts.sort_by(f64::total_cmp);
        let pts: Vec<usize> = ts.iter().map(|&t| vid([p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t], &mut verts)).collect();
        for w in pts.windows(2) {
            if w[0] != w[1] {
                let e = (w[0].min(w[1]), w[0].max(w[1]));
                if !edges.contains(&e) {
                    edges.push(e);
                }
            }
        }
    }
    // the directed edges with the result on their left
    let mut directed: Vec<(usize, usize)> = vec![];
    for &(u, v) in &edges {
        let (p, q) = (verts[u], verts[v]);
        let m = [(p[0] + q[0]) / 2.0, (p[1] + q[1]) / 2.0];
        let len = ((q[0] - p[0]).powi(2) + (q[1] - p[1]).powi(2)).sqrt();
        let d = (len * 1e-6).min(scale * 1e-7).max(eps * 10.0);
        let nrm = [-(q[1] - p[1]) / len * d, (q[0] - p[0]) / len * d];
        let left = [m[0] + nrm[0], m[1] + nrm[1]];
        let right = [m[0] - nrm[0], m[1] - nrm[1]];
        let res = |x: P| op.apply(inside_fill(x, a, fill), inside(x, b));
        match (res(left), res(right)) {
            (true, false) => directed.push((u, v)),
            (false, true) => directed.push((v, u)),
            _ => {}
        }
    }
    // link the edges into rings: at each vertex, the outgoing edge first clockwise from the way back
    let mut used = vec![false; directed.len()];
    let mut rings: Vec<Vec<P>> = vec![];
    for start in 0..directed.len() {
        if used[start] {
            continue;
        }
        let mut ring = vec![];
        let mut cur = start;
        loop {
            used[cur] = true;
            let (u, v) = directed[cur];
            ring.push(verts[u]);
            let back = (verts[u][1] - verts[v][1]).atan2(verts[u][0] - verts[v][0]);
            let next = (0..directed.len())
                .filter(|&k| !used[k] && directed[k].0 == v)
                .min_by(|&i, &j| {
                    let ang = |k: usize| {
                        let w = verts[directed[k].1];
                        let a = (w[1] - verts[v][1]).atan2(w[0] - verts[v][0]);
                        // clockwise turn from `back`, in (0, 2 pi]
                        let mut t = back - a;
                        while t <= 0.0 {
                            t += std::f64::consts::TAU;
                        }
                        t
                    };
                    ang(i).total_cmp(&ang(j))
                });
            match next {
                Some(k) => cur = k,
                None => break,
            }
            if directed[cur].0 == directed[start].0 && used[cur] {
                break;
            }
        }
        let ring = simplify(ring, eps);
        if ring.len() >= 3 && area(&ring).abs() > eps * eps {
            rings.push(ring);
        }
    }
    let (outer, holes): (Vec<Vec<P>>, Vec<Vec<P>>) = rings.into_iter().partition(|r| area(r) > 0.0);
    let mut shapes: Shapes = outer.into_iter().map(|o| (o, vec![])).collect();
    for hole in holes {
        // the smallest outer ring around the hole's centre
        let c = [hole.iter().map(|p| p[0]).sum::<f64>() / hole.len() as f64, hole.iter().map(|p| p[1]).sum::<f64>() / hole.len() as f64];
        let probe = if inside(c, std::slice::from_ref(&hole)) { c } else { hole[0] };
        let owner = (0..shapes.len()).filter(|&k| inside(probe, std::slice::from_ref(&shapes[k].0))).min_by(|&i, &j| area(&shapes[i].0).total_cmp(&area(&shapes[j].0)));
        if let Some(k) = owner {
            shapes[k].1.push(hole);
        }
    }
    shapes
}

/// A ring without vertices on a straight line between their neighbours.
fn simplify(mut r: Vec<P>, eps: f64) -> Vec<P> {
    let mut changed = true;
    while changed && r.len() >= 3 {
        changed = false;
        let n = r.len();
        for i in 0..n {
            let (a, b, c) = (r[(i + n - 1) % n], r[i], r[(i + 1) % n]);
            let len = ((c[0] - a[0]).powi(2) + (c[1] - a[1]).powi(2)).sqrt().max(1e-300);
            if cross(a, b, c).abs() / len <= eps && (b[0] - a[0]) * (c[0] - b[0]) + (b[1] - a[1]) * (c[1] - b[1]) >= 0.0 {
                r.remove(i);
                changed = true;
                break;
            }
        }
    }
    r
}

/// A ring rotated to start at its least vertex (lowest x, then y): a canonical form for results and tests.
pub fn canonical(mut r: Vec<P>) -> Vec<P> {
    if let Some(k) = (0..r.len()).min_by(|&i, &j| r[i][0].total_cmp(&r[j][0]).then(r[i][1].total_cmp(&r[j][1]))) {
        r.rotate_left(k);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64, s: f64) -> Vec<P> {
        vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
    }

    fn total(s: &Shapes) -> f64 {
        s.iter().map(|(o, hs)| area(o) + hs.iter().map(|h| area(h)).sum::<f64>()).sum()
    }

    #[test]
    fn overlapping_squares() {
        let (a, b) = (vec![sq(0.0, 0.0, 2.0)], vec![sq(1.0, 1.0, 2.0)]);
        assert!((total(&boolean(&a, &b, Op::Union)) - 7.0).abs() < 1e-9);
        assert!((total(&boolean(&a, &b, Op::Intersection)) - 1.0).abs() < 1e-9);
        assert!((total(&boolean(&a, &b, Op::Difference)) - 3.0).abs() < 1e-9);
        let i = boolean(&a, &b, Op::Intersection);
        assert_eq!(canonical(i[0].0.clone()), sq(1.0, 1.0, 1.0));
    }

    #[test]
    fn holes_and_pieces() {
        // a square with a smaller one taken out of its middle: one ring with a hole
        let d = boolean(&[sq(0.0, 0.0, 4.0)], &[sq(1.0, 1.0, 2.0)], Op::Difference);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].1.len(), 1);
        assert!((total(&d) - 12.0).abs() < 1e-9);
        // disjoint squares: two pieces; shared edges merge
        assert_eq!(boolean(&[sq(0.0, 0.0, 1.0)], &[sq(3.0, 0.0, 1.0)], Op::Union).len(), 2);
        let m = boolean(&[sq(0.0, 0.0, 1.0)], &[sq(1.0, 0.0, 1.0)], Op::Union);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].0.len(), 4);
        assert!(boolean(&[sq(0.0, 0.0, 1.0)], &[sq(3.0, 0.0, 1.0)], Op::Intersection).is_empty());
    }

    #[test]
    fn clockwise_input_and_concave() {
        let mut a = sq(0.0, 0.0, 2.0);
        a.reverse();
        // an L shape
        let l = vec![[0.0, 0.0], [3.0, 0.0], [3.0, 1.0], [1.0, 1.0], [1.0, 3.0], [0.0, 3.0]];
        let u = boolean(&[a], &[l], Op::Intersection);
        assert!((total(&u) - 3.0).abs() < 1e-9);
    }
}
