//! Degenerate primitives are the lower-dimensional sets they actually are: random triangles with equal or collinear
//! corners, tetrahedra with coplanar corners, polygons on a line, lines with equal ends, disks, balls, circles and
//! spheres of radius 0, rectangles and cuboids with equal corners. Dimension, measure, centroid, bounds and
//! membership are checked against the definition of the actual set (the centroid as the mean over the set in its
//! own measure, membership by the convex hull).
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

/// A list of numbers printed by the engine (after `n`).
fn numbers(e: &mut CoreEngine, src: &str) -> Vec<f64> {
    let s = eval(e, &format!("n({src})"));
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']').replace(['[', ']'], "");
    inner.split(',').map(|x| x.trim().parse().unwrap_or_else(|_| panic!("{src} -> {s}"))).collect()
}

fn number(e: &mut CoreEngine, src: &str) -> f64 {
    numbers(e, src)[0]
}

/// A small linear congruential generator (seeded, so the cases are the same on every run).
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    /// An integer in [lo, hi].
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

fn pt(p: &[i64]) -> String {
    format!("[{}]", p.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

fn close(a: &[f64], b: &[f64], what: &str) {
    assert_eq!(a.len(), b.len(), "{what}: {a:?} vs {b:?}");
    for (x, y) in a.iter().zip(b) {
        assert!((x - y).abs() <= 1e-9 * (1.0 + y.abs()), "{what}: {a:?} vs {b:?}");
    }
}

/// The segment hull of collinear integer points: the two outermost.
fn outer_two(ps: &[Vec<i64>]) -> (Vec<f64>, Vec<f64>) {
    let f: Vec<Vec<f64>> = ps.iter().map(|p| p.iter().map(|&x| x as f64).collect()).collect();
    let far = (0..f.len()).max_by(|&i, &j| dist(&f[i], &f[0]).total_cmp(&dist(&f[j], &f[0]))).unwrap();
    let d: Vec<f64> = f[far].iter().zip(&f[0]).map(|(a, b)| a - b).collect();
    let t = |q: &Vec<f64>| q.iter().zip(&f[0]).zip(&d).map(|((a, b), c)| (a - b) * c).sum::<f64>();
    let lo = (0..f.len()).min_by(|&i, &j| t(&f[i]).total_cmp(&t(&f[j]))).unwrap();
    let hi = (0..f.len()).max_by(|&i, &j| t(&f[i]).total_cmp(&t(&f[j]))).unwrap();
    (f[lo].clone(), f[hi].clone())
}

fn dist(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).sum::<f64>().sqrt()
}

/// Checks a region that is the segment from a to b (a != b): dimension 1, its length, its midpoint, its bounds, and
/// membership of points on it, on its line beyond the ends and off its line.
fn check_segment(e: &mut CoreEngine, reg: &str, a: &[f64], b: &[f64]) {
    assert_eq!(eval(e, &format!("region_dimension({reg})")), "1", "{reg}");
    close(&[number(e, &format!("region_measure({reg})"))], &[dist(a, b)], reg);
    close(&[number(e, &format!("arc_length({reg})"))], &[dist(a, b)], reg);
    assert_eq!(eval(e, &format!("area({reg})")), "0", "{reg}");
    let mid: Vec<f64> = a.iter().zip(b).map(|(x, y)| (x + y) / 2.0).collect();
    close(&numbers(e, &format!("region_centroid({reg})")), &mid, reg);
    let bounds: Vec<f64> = a.iter().zip(b).flat_map(|(&x, &y)| [x.min(y), x.max(y)]).collect();
    close(&numbers(e, &format!("region_bounds({reg})")), &bounds, reg);
    let at = |t: (i64, i64)| -> String {
        let c: Vec<String> = a.iter().zip(b).map(|(x, y)| format!("({} + ({})*({})/{})", *x as i64, (*y - *x) as i64, t.0, t.1)).collect();
        format!("[{}]", c.join(", "))
    };
    for (t, inside) in [((1, 3), true), ((0, 1), true), ((1, 1), true), ((5, 4), false), ((-1, 7), false)] {
        assert_eq!(eval(e, &format!("region_member({reg}, {})", at(t))), inside.to_string(), "{reg} at {t:?}");
    }
    // off the line: the midpoint moved along a normal direction
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| y - x).collect();
    let mut nrm = vec![0.0; a.len()];
    nrm[0] = -d[1];
    nrm[1] = d[0];
    let off: Vec<String> = mid.iter().zip(&nrm).map(|(m, n)| format!("{}", m + n / 7.0)).collect();
    assert_eq!(eval(e, &format!("region_member({reg}, [{}])", off.join(", "))), "false", "{reg} off the line");
}

/// Checks a region that is the one point p: dimension 0, counting measure 1, centroid and bounds p, membership.
fn check_point(e: &mut CoreEngine, reg: &str, p: &[i64]) {
    assert_eq!(eval(e, &format!("region_dimension({reg})")), "0", "{reg}");
    assert_eq!(eval(e, &format!("region_measure({reg})")), "1", "{reg}");
    let pf: Vec<f64> = p.iter().map(|&x| x as f64).collect();
    close(&numbers(e, &format!("region_centroid({reg})")), &pf, reg);
    let bounds: Vec<f64> = pf.iter().flat_map(|&x| [x, x]).collect();
    close(&numbers(e, &format!("region_bounds({reg})")), &bounds, reg);
    assert_eq!(eval(e, &format!("region_member({reg}, {})", pt(p))), "true", "{reg}");
    let mut q = p.to_vec();
    q[0] += 1;
    assert_eq!(eval(e, &format!("region_member({reg}, {})", pt(&q))), "false", "{reg}");
}

#[test]
fn triangles_with_equal_or_collinear_corners_are_segments() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "region_centroid(triangle([[0, 0], [1, 1], [1, 1]]))"), "[1/2, 1/2]");
    assert_eq!(eval(&mut e, "region_measure(triangle([[0, 0], [1, 1], [1, 1]]))"), "2^(1/2)");
    let mut g = Lcg(7);
    for case in 0..40 {
        let dim = 2 + case % 2;
        let a: Vec<i64> = (0..dim).map(|_| g.int(-5, 5)).collect();
        let mut d: Vec<i64> = (0..dim).map(|_| g.int(-4, 4)).collect();
        if d.iter().all(|&x| x == 0) {
            d[0] = 1;
        }
        // the third corner on the line through the first two (or equal to one of them), in a random order
        let ts = [g.int(-2, 3), g.int(-2, 3)];
        let mut corners: Vec<Vec<i64>> = vec![a.clone(), a.iter().zip(&d).map(|(x, y)| x + y).collect()];
        corners.push(a.iter().zip(&d).map(|(x, y)| x + ts[0] * y).collect());
        if case % 3 == 0 {
            corners.push(a.iter().zip(&d).map(|(x, y)| x + ts[1] * y).collect());
        }
        let k = g.int(0, corners.len() as i64 - 1) as usize;
        corners.rotate_left(k);
        let (lo, hi) = outer_two(&corners);
        let list = corners.iter().map(|p| pt(p)).collect::<Vec<_>>().join(", ");
        let reg = match corners.len() {
            3 if dim == 2 => format!("triangle([{list}])"),
            3 => format!("simplex([{list}])"),
            _ if dim == 3 => format!("tetrahedron([{list}])"),
            _ => format!("polygon([{list}])"),
        };
        check_segment(&mut e, &reg, &lo, &hi);
    }
}

#[test]
fn coplanar_tetrahedra_are_polygons() {
    let mut e = CoreEngine::new();
    let mut g = Lcg(11);
    for _ in 0..25 {
        // four corners in the plane z = c1 x + c2 y + c0, the region their convex hull
        let (c0, c1, c2) = (g.int(-3, 3), g.int(-2, 2), g.int(-2, 2));
        let xy: Vec<(i64, i64)> = loop {
            let v: Vec<(i64, i64)> = (0..4).map(|_| (g.int(-4, 4), g.int(-4, 4))).collect();
            // not all on one line
            let cr = |i: usize, j: usize, k: usize| (v[j].0 - v[i].0) * (v[k].1 - v[i].1) - (v[j].1 - v[i].1) * (v[k].0 - v[i].0);
            if cr(0, 1, 2) != 0 || cr(0, 1, 3) != 0 {
                break v;
            }
        };
        let corners: Vec<Vec<i64>> = xy.iter().map(|&(x, y)| vec![x, y, c0 + c1 * x + c2 * y]).collect();
        let reg = format!("tetrahedron([{}])", corners.iter().map(|p| pt(p)).collect::<Vec<_>>().join(", "));
        assert_eq!(eval(&mut e, &format!("region_dimension({reg})")), "2", "{reg}");
        // the hull in the xy projection: area and centroid, lifted (the area scales by sqrt(1 + c1^2 + c2^2))
        let mut pts: Vec<(f64, f64)> = xy.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
        pts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
        pts.dedup();
        let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
        let mut hull: Vec<(f64, f64)> = vec![];
        for pass in 0..2 {
            let start = hull.len();
            let order: Vec<(f64, f64)> = if pass == 0 { pts.clone() } else { pts.iter().rev().copied().collect() };
            for p in order {
                while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                    hull.pop();
                }
                hull.push(p);
            }
            hull.pop();
        }
        let (mut a2, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for i in 0..hull.len() {
            let (p, q) = (hull[i], hull[(i + 1) % hull.len()]);
            let c = p.0 * q.1 - q.0 * p.1;
            a2 += c;
            cx += (p.0 + q.0) * c;
            cy += (p.1 + q.1) * c;
        }
        let (cx, cy) = (cx / (3.0 * a2), cy / (3.0 * a2));
        let lift = ((1 + c1 * c1 + c2 * c2) as f64).sqrt();
        close(&[number(&mut e, &format!("region_measure({reg})"))], &[a2.abs() / 2.0 * lift], &reg);
        close(&numbers(&mut e, &format!("region_centroid({reg})")), &[cx, cy, c0 as f64 + c1 as f64 * cx + c2 as f64 * cy], &reg);
        assert_eq!(eval(&mut e, &format!("volume({reg})")), "0", "{reg}");
        // membership: the centroid is in, a point off the plane is not
        let z = c0 as f64 + c1 as f64 * cx + c2 as f64 * cy;
        assert_eq!(eval(&mut e, &format!("region_member({reg}, [{cx}, {cy}, {z}])")), "true", "{reg}");
        assert_eq!(eval(&mut e, &format!("region_member({reg}, [{cx}, {cy}, {}])", z + 0.5)), "false", "{reg}");
    }
}

#[test]
fn polygons_and_lines_on_a_line_or_a_point() {
    let mut e = CoreEngine::new();
    check_segment(&mut e, "polygon([[0, 0], [1, 1], [2, 2]])", &[0.0, 0.0], &[2.0, 2.0]);
    check_segment(&mut e, "polygon([[0, 0], [2, 2], [1, 1], [1, 1]])", &[0.0, 0.0], &[2.0, 2.0]);
    check_segment(&mut e, "polygon([[3, 1], [-1, 1], [0, 1], [3, 1]])", &[-1.0, 1.0], &[3.0, 1.0]);
    check_point(&mut e, "polygon([[2, 3], [2, 3], [2, 3]])", &[2, 3]);
    check_point(&mut e, "line([[2, 3], [2, 3]])", &[2, 3]);
    check_point(&mut e, "line([[2, 3, 1], [2, 3, 1], [2, 3, 1]])", &[2, 3, 1]);
    check_point(&mut e, "triangle([[4, -1], [4, -1], [4, -1]])", &[4, -1]);
    check_point(&mut e, "tetrahedron([[1, 1, 1], [1, 1, 1], [1, 1, 1], [1, 1, 1]])", &[1, 1, 1]);
    // repeated corners of a polygon with an inside change nothing
    assert_eq!(eval(&mut e, "region_measure(polygon([[0, 0], [1, 0], [1, 0], [1, 1], [0, 1], [0, 0]]))"), "1");
    assert_eq!(eval(&mut e, "region_centroid(polygon([[0, 0], [2, 0], [2, 0], [2, 2], [0, 2]]))"), "[1, 1]");
    // a polyline with a repeated point is measured as it was
    assert_eq!(eval(&mut e, "region_measure(line([[0, 0], [0, 0], [3, 4]]))"), "5");
    assert_eq!(eval(&mut e, "region_centroid(line([[0, 0], [0, 0], [3, 4]]))"), "[3/2, 2]");
}

#[test]
fn round_regions_of_radius_zero_and_flat_boxes() {
    let mut e = CoreEngine::new();
    let mut g = Lcg(3);
    for _ in 0..6 {
        let p2 = [g.int(-5, 5), g.int(-5, 5)];
        let p3 = [g.int(-5, 5), g.int(-5, 5), g.int(-5, 5)];
        for reg in [format!("disk({}, 0)", pt(&p2)), format!("circle({}, 0)", pt(&p2)), format!("disk({}, [0, 0])", pt(&p2)), format!("rectangle({0}, {0})", pt(&p2)), format!("annulus({}, [0, 0])", pt(&p2))] {
            check_point(&mut e, &reg, &p2);
        }
        for reg in [format!("ball({}, 0)", pt(&p3)), format!("sphere({}, 0)", pt(&p3)), format!("cuboid({0}, {0})", pt(&p3))] {
            check_point(&mut e, &reg, &p3);
        }
        // an ellipse with one semi-axis 0 is a segment along the other
        let a = g.int(1, 4);
        let reg = format!("disk({}, [{a}, 0])", pt(&p2));
        check_segment(&mut e, &reg, &[(p2[0] - a) as f64, p2[1] as f64], &[(p2[0] + a) as f64, p2[1] as f64]);
    }
    // a rectangle flat in one coordinate is a segment
    check_segment(&mut e, "rectangle([0, 1], [0, 4])", &[0.0, 1.0], &[0.0, 4.0]);
    // an annulus with equal radii is a circle
    assert_eq!(eval(&mut e, "region_dimension(annulus([0, 0], [2, 2]))"), "1");
    assert_eq!(eval(&mut e, "region_measure(annulus([0, 0], [2, 2]))"), "4*pi");
}
