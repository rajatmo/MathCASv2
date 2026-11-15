//! Round 42: membership of a symbolic point in simplices, polylines, convex polygons and polyhedra, flats, solid
//! cylinders and cones and tori, as conditions on the coordinates.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn same(e: &mut CoreEngine, src: &str, expected: &str) {
    let got = ev(e, src);
    let want = ev(e, expected);
    assert_eq!(got, want, "{src}");
}

/// The condition for `region` agrees with the membership of numeric points on a grid of `side`^d points of the box
/// [lo, hi]^d (inside, outside and on the boundary).
fn agrees_on_grid(e: &mut CoreEngine, region: &str, vars: &[&str], lo: i64, hi: i64, side: usize) {
    let cond = ev(e, &format!("region_member({region}, [{}])", vars.join(", ")));
    assert!(!cond.starts_with("region_member"), "{region}: {cond}");
    let n = vars.len();
    let mut idx = vec![0usize; n];
    loop {
        let pt: Vec<String> = idx
            .iter()
            .map(|&i| {
                // rational grid points, the ends included
                let num = lo * (side as i64 - 1) + (hi - lo) * i as i64;
                format!("{num}/{}", side - 1)
            })
            .collect();
        let rules: Vec<String> = vars.iter().zip(&pt).map(|(v, p)| format!("{v} -> {p}")).collect();
        let by_cond = ev(e, &format!("rewrite({cond}, [{}])", rules.join(", ")));
        let direct = ev(e, &format!("region_member({region}, [{}])", pt.join(", ")));
        assert_eq!(by_cond, direct, "{region} at {pt:?}");
        let mut k = 0;
        while k < n {
            idx[k] += 1;
            if idx[k] < side {
                break;
            }
            idx[k] = 0;
            k += 1;
        }
        if k == n {
            break;
        }
    }
}

#[test]
fn simplices_of_every_dimension() {
    let mut e = CoreEngine::new();
    same(
        &mut e,
        "region_member(simplex([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]]), [x, y, z, t])",
        "element(x | y | z | t, reals) and t + x + y + z == 1 and y >= 0 and z >= 0 and x + y + z <= 1 and x >= 0",
    );
    same(
        &mut e,
        "region_member(polygon([[1, 0, 0], [0, 1, 0], [0, 0, 1]]), [x, y, z])",
        "element(x | y | z, reals) and x + y + z == 1 and y >= 0 and x + y <= 1 and x >= 0",
    );
    // independent check: numeric membership on grids (the 4D simplex's grid has points on its hyperplane)
    agrees_on_grid(&mut e, "tetrahedron([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]])", &["x", "y", "z"], -1, 2, 7);
    agrees_on_grid(&mut e, "simplex([[1, 0, 0, 0], [0, 1, 0, 0], [0, 0, 1, 0], [0, 0, 0, 1]])", &["x", "y", "z", "t"], 0, 1, 5);
    agrees_on_grid(&mut e, "triangle([[2, 0, 0], [0, 0, 2], [0, 2, 2]])", &["x", "y", "z"], 0, 2, 5);
    agrees_on_grid(&mut e, "line([[1, 2, 3], [4, 5, 6]])", &["x", "y", "z"], 1, 4, 7);
    agrees_on_grid(&mut e, "line([[0, 0], [1, 2], [1, 0]])", &["x", "y"], -1, 2, 7);
}

#[test]
fn convex_polygons_and_polyhedra() {
    let mut e = CoreEngine::new();
    same(
        &mut e,
        "region_member(polygon([[1, 0, 0], [1, 1, 1], [0, 0, 1]]), [x, y, z])",
        "element(x | y | z, reals) and x + z == 1 + y and y >= 0 and x <= 1 and x >= y",
    );
    agrees_on_grid(&mut e, "regular_polygon([0, 0], [1, 0], 4)", &["x", "y"], -2, 2, 9);
    agrees_on_grid(&mut e, "parallelogram([0, 0], [[1, 0], [1, 1]])", &["x", "y"], -1, 2, 7);
    agrees_on_grid(&mut e, "polygon([[0, 0, 0], [1, 0, 0], [1, 1, 1], [0, 1, 1]])", &["x", "y", "z"], 0, 1, 5);
    agrees_on_grid(
        &mut e,
        "hexahedron([[0, 0, 0], [1, 0, 0], [2, 1, 0], [1, 1, 0], [0, 0, 1], [1, 0, 1], [2, 1, 1], [1, 1, 1]])",
        &["x", "y", "z"],
        -1,
        2,
        7,
    );
    agrees_on_grid(&mut e, "pyramid([[0, 0, 0], [2, 0, 0], [2, 2, 0], [0, 2, 0], [1, 1, 2]])", &["x", "y", "z"], 0, 2, 5);
    agrees_on_grid(&mut e, "prism([[1, 0, 1], [0, 0, 0], [2, 0, 0], [1, 2, 1], [0, 2, 0], [2, 2, 0]])", &["x", "y", "z"], 0, 2, 5);
    // a polygon that is not convex keeps its membership numeric
    assert!(ev(&mut e, "region_member(polygon([[0, 0], [2, 0], [1, 1], [2, 2], [0, 2]]), [x, y])").starts_with("region_member"));
}

#[test]
fn flats_cylinders_cones_and_tori() {
    let mut e = CoreEngine::new();
    same(&mut e, "region_member(half_line([0, 0], [1, 1]), [x, y])", "element(x | y, reals) and x == y and x >= 0");
    same(
        &mut e,
        "region_member(infinite_line([[x0, y0], [x1, y1]]), [x, y])",
        "element(x | y, reals) and x*y1 + x0*y + x1*y0 == x*y0 + x0*y1 + x1*y",
    );
    same(
        &mut e,
        "region_member(cylinder([[0, 0, 0], [1, 1, 1]], 2), [x, y, z])",
        "element(x | y | z, reals) and x + y + z >= 0 and x + y + z <= 3 and x^2 + y^2 + z^2 <= 6 + x*y + x*z + y*z",
    );
    same(
        &mut e,
        "region_member(torus([c1, c2, c3], [r0, r1]), [x, y, z])",
        "conditional_expression(element(x | y | z, reals) and r0*r1 + (x - c1)^2 + (y - c2)^2 + (z - c3)^2 == (r0 + r1)*((x - c1)^2 + (y - c2)^2)^(1/2), r0 > 0 and r1 > 0 and r0 < r1)",
    );
    agrees_on_grid(&mut e, "half_line([0, 0, 0], [1, 2, 3])", &["x", "y", "z"], -1, 3, 5);
    agrees_on_grid(&mut e, "cylinder([[0, 0, 0], [1, 1, 1]], 2)", &["x", "y", "z"], -2, 3, 6);
    agrees_on_grid(&mut e, "cone([[0, 0, 0], [1, 1, 1]], 2)", &["x", "y", "z"], -2, 3, 6);
    agrees_on_grid(&mut e, "cone([[1, 0, 0], [1, 0, 2]], 1)", &["x", "y", "z"], 0, 2, 5);
    agrees_on_grid(&mut e, "filled_torus([0, 0, 0], [1, 3])", &["x", "y", "z"], -3, 3, 7);
}
