//! Union, intersection, difference and Boolean functions of mesh regions of the line, plane boundary meshes and
//! boundary meshes of space. Checks independent of the operation: the results' measures satisfy inclusion and
//! exclusion against the measures of the inputs, and on a grid of points membership in each result agrees with the
//! Boolean function of membership in the inputs (grid points chosen off every face). Each test has a time bound.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;
use std::time::{Duration, Instant};

fn ev(e: &mut CoreEngine, src: &str) -> String {
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

fn holds(e: &mut CoreEngine, src: &str) {
    assert_eq!(ev(e, src), "true", "{src}");
}

fn timed(what: &str, limit: u64, f: impl FnOnce()) {
    let t0 = Instant::now();
    f();
    let took = t0.elapsed();
    assert!(took < Duration::from_secs(limit), "{what} took {took:?}");
}

const TETRAHEDRA: &str = "t1 = boundary_mesh_region([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]], [polygon([[3, 2, 1], [1, 2, 4], [2, 3, 4], [4, 3, 1]])]); \
     t2 = boundary_mesh_region([[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 2.]], [polygon([[3, 2, 1], [1, 2, 4], [2, 3, 4], [4, 3, 1]])]);";

#[test]
fn intervals_of_the_line() {
    timed("line meshes", 60, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "a = mesh_region([[0.], [2.]], [line([[1, 2]])]); b = mesh_region([[1.], [3.]], [line([[1, 2]])]);");
        assert_eq!(ev(&mut e, "region_union(a, b)"), "mesh_region([[0.0], [3.0]], [line([[1, 2]])])");
        assert_eq!(ev(&mut e, "region_intersection(a, b)"), "mesh_region([[1.0], [2.0]], [line([[1, 2]])])");
        assert_eq!(ev(&mut e, "region_difference(a, b)"), "mesh_region([[0.0], [1.0]], [line([[1, 2]])])");
        assert_eq!(ev(&mut e, "boolean_region(&(not &2 and &1), [a, b])"), "mesh_region([[0.0], [1.0]], [line([[1, 2]])])");
        // the symmetric difference: two intervals
        assert_eq!(ev(&mut e, "boolean_region(xor, [a, b])"), "mesh_region([[0.0], [1.0], [2.0], [3.0]], [line([[1, 2], [3, 4]])])");
        ev(&mut e, "p = boundary_mesh_region([[0.], [2.]], [point([[1], [2]])]); q = boundary_mesh_region([[1.], [3.]], [point([[1], [2]])]);");
        assert_eq!(ev(&mut e, "region_union(p, q)"), "boundary_mesh_region([[0.0], [3.0]], [point([[1], [2]])], method: [separate_boundaries -> false])");
        assert_eq!(ev(&mut e, "region_intersection(p, q)"), "boundary_mesh_region([[1.0], [2.0]], [point([[1], [2]])], method: [separate_boundaries -> false])");
        assert_eq!(ev(&mut e, "region_difference(p, q)"), "boundary_mesh_region([[0.0], [1.0]], [point([[1], [2]])], method: [separate_boundaries -> false])");
        // disjoint: empty
        assert_eq!(ev(&mut e, "region_intersection(a, mesh_region([[5.], [6.]], [line([[1, 2]])]))"), "empty_region(1)");
        // the complement of everything is unbounded: left as it is
        holds(&mut e, "head(boolean_region(&(not &1), [a])) === boolean_region");
        // membership on a grid agrees with the Boolean function
        holds(&mut e, "{ let u = region_union(a, b); let x = boolean_region(xor, [a, b]); every(range(-0.47, 3.6, 0.31), t => region_member(u, [t]) === (region_member(a, [t]) or region_member(b, [t])) and region_member(x, [t]) === xor(region_member(a, [t]), region_member(b, [t]))) }");
    });
}

#[test]
fn boundary_meshes_of_the_plane() {
    timed("plane boundary meshes", 60, || {
        let mut e = CoreEngine::new();
        ev(&mut e, "a = boundary_mesh_region([[0., 4.], [2., 1.], [4., 4.]], [line([[1, 2], [2, 3], [3, 1]])]); b = boundary_mesh_region([[0., 0.], [2., 3.], [4., 0.]], [line([[1, 3], [3, 2], [2, 1]])]);");
        holds(&mut e, "head(region_union(a, b)) === boundary_mesh_region and head(region_difference(a, b)) === boundary_mesh_region");
        holds(&mut e, "abs(area(region_union(a, b)) - (area(a) + area(b) - area(region_intersection(a, b)))) < 1e-12");
        holds(&mut e, "abs(area(region_difference(a, b)) - (area(a) - area(region_intersection(a, b)))) < 1e-12");
        // the triangles meet in the rhombus (2, 1), (8/3, 2), (2, 3), (4/3, 2): diagonals 2 and 4/3, area 4/3
        holds(&mut e, "abs(area(region_intersection(a, b)) - 4/3) < 1e-12");
        holds(&mut e, "{ let u = region_union(a, b); let d = region_difference(a, b); every(tuples(range(0.13, 4, 0.37), 2), p => region_member(u, p) === (region_member(a, p) or region_member(b, p)) and region_member(d, p) === (region_member(a, p) and not region_member(b, p))) }");
    });
}

#[test]
fn boundary_meshes_of_space() {
    timed("space boundary meshes", 120, || {
        let mut e = CoreEngine::new();
        ev(&mut e, TETRAHEDRA);
        ev(&mut e, "u = region_union(t1, t2); i = region_intersection(t1, t2); d = region_difference(t1, t2); x = boolean_region(&(&1 and not &2), [t1, t2]);");
        holds(&mut e, "every([u, i, d, x], r => head(r) === boundary_mesh_region)");
        // the intersection is the tetrahedron (0,0,0), (1,0,0), (0,1,0), (1/4,1/4,1/2): volume 1/12 by hand
        holds(&mut e, "abs(volume(i) - 1/12) < 1e-12");
        holds(&mut e, "abs(volume(u) - (volume(t1) + volume(t2) - volume(i))) < 1e-12");
        holds(&mut e, "abs(volume(d) - (volume(t1) - volume(i))) < 1e-12 and abs(volume(x) - volume(d)) < 1e-12");
        // centroids: the volume-weighted parts add up (the union from the difference and the second solid)
        holds(&mut e, "norm(region_centroid(u)*volume(u) - (region_centroid(d)*volume(d) + region_centroid(t2)*volume(t2))) < 1e-12");
        // membership on a grid of points off the faces
        holds(&mut e, "every(tuples(range(0.0371, 1.1, 0.173), 3), p => region_member(u, p) === (region_member(t1, p) or region_member(t2, p)) and region_member(i, p) === (region_member(t1, p) and region_member(t2, p)) and region_member(d, p) === (region_member(t1, p) and not region_member(t2, p)))");
        // a cube less a smaller cube in a corner: a solid that is not convex
        ev(&mut e, "c1 = boundary_mesh_region([[0.,0.,0.],[2.,0.,0.],[2.,2.,0.],[0.,2.,0.],[0.,0.,2.],[2.,0.,2.],[2.,2.,2.],[0.,2.,2.]], [polygon([[1,4,3,2],[5,6,7,8],[1,2,6,5],[2,3,7,6],[3,4,8,7],[4,1,5,8]])]); \
                    c2 = boundary_mesh_region([[1.,1.,1.],[3.,1.,1.],[3.,3.,1.],[1.,3.,1.],[1.,1.,3.],[3.,1.,3.],[3.,3.,3.],[1.,3.,3.]], [polygon([[1,4,3,2],[5,6,7,8],[1,2,6,5],[2,3,7,6],[3,4,8,7],[4,1,5,8]])]);");
        holds(&mut e, "abs(volume(region_difference(c1, c2)) - 7) < 1e-12 and abs(volume(region_union(c1, c2)) - 15) < 1e-12 and abs(volume(region_intersection(c1, c2)) - 1) < 1e-12");
        holds(&mut e, "norm(region_centroid(region_difference(c1, c2)) - [13/14, 13/14, 13/14]) < 1e-12");
        holds(&mut e, "{ let d = region_difference(c1, c2); every(tuples(range(0.11, 3, 0.37), 3), p => region_member(d, p) === (region_member(c1, p) and not region_member(c2, p))) }");
        // disjoint solids: the intersection is empty
        assert_eq!(ev(&mut e, "region_intersection(t1, boundary_mesh_region([[5., 5., 5.], [6., 5., 5.], [5., 6., 5.], [5., 5., 6.]], [polygon([[3, 2, 1], [1, 2, 4], [2, 3, 4], [4, 3, 1]])]))"), "empty_region(3)");
    });
}
