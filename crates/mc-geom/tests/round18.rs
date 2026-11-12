//! Round 18: mesh regions, polyhedra, tori, flats and cones, cylinders, ellipsoid distances, triangles
//! from measurements, polygon forms, relations and bounding regions, symbolic membership.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

fn check(cases: &[(&str, &str)]) {
    let mut e = CoreEngine::new();
    for (src, want) in cases {
        assert_eq!(eval(&mut e, src), *want, "{src}");
    }
}

#[test]
fn mesh_regions_are_canonical() {
    check(&[
        ("mesh_region([[0], [1], [2]], [line([1, 2]), point(3)])", "mesh_region([[0.0], [1.0], [2.0]], [line([[1, 2]]), point([[3]])])"),
        ("mesh_region([[0], [1], [2], [3], [4], [5]], [line([1, 3]), line([3, 4]), line([5, 6])])", "mesh_region([[0.0], [2.0], [3.0], [4.0], [5.0]], [line([[1, 2], [2, 3], [4, 5]])])"),
        ("mesh_region([[0, 0], [1, 0], [0, 1]], triangle([1, 2, 3]))", "mesh_region([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]], [polygon([[1, 2, 3]])])"),
        ("t = mesh_region([[0, 0, 0], [2, 0, 0], [2, 2, 0], [0, 2, 0], [1, 1, 2]], tetrahedron([[1, 2, 3, 5], [1, 3, 4, 5]]))", "null"),
        ("t", "mesh_region([[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 2.0, 0.0], [0.0, 2.0, 0.0], [1.0, 1.0, 2.0]], [tetrahedron([[1, 2, 3, 5], [1, 3, 4, 5]])])"),
        ("mesh_cell_count(t)", "[5, 9, 7, 2]"),
        ("mesh_cells(t, 1)", "[line([3, 2]), line([2, 1]), line([1, 3]), line([3, 5]), line([5, 2]), line([1, 5]), line([4, 3]), line([1, 4]), line([4, 5])]"),
        ("volume(t)", "2.6666666666666665"),
        ("s = mesh_region([[0, 0], [1, 0], [1, 1], [0, 1]], polygon([1, 2, 3, 4]))", "null"),
        ("s", "mesh_region([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], [polygon([[1, 2, 3, 4]])])"),
        ("mesh_primitives(s, 1)[1]", "line([[0.0, 0.0], [1.0, 0.0]])"),
        ("[area(s), region_member(s, [1/2, 1/2]), is_mesh_region(s), is_mesh_region(disk())]", "[1.0, true, true, false]"),
        ("b = boundary_mesh_region([[0, 0], [3, 0], [3, 3], [0, 3], [1, 1], [2, 1], [2, 2], [1, 2]], line([1, 2, 3, 4, 1]), line([5, 6, 7, 8, 5]))", "null"),
        ("b", "boundary_mesh_region([[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0], [1.0, 1.0], [2.0, 1.0], [2.0, 2.0], [1.0, 2.0]], [line([[1, 2], [2, 3], [3, 4], [4, 1]])], [line([[5, 8], [8, 7], [7, 6], [6, 5]])], method: [\"SeparateBoundaries\" -> false])"),
        ("[mesh_cell_count(b), area(b)]", "[[8, 8], 8.0]"),
    ]);
}

#[test]
fn polyhedra_tori_and_flats() {
    check(&[
        ("p = pyramid([[0, 0, 0], [2, 0, 0], [2, 2, 0], [0, 2, 0], [1, 1, 2]])", "null"),
        ("p", "pyramid([[0, 0, 0], [2, 0, 0], [2, 2, 0], [0, 2, 0], [1, 1, 2]])"),
        ("[volume(p), region_centroid(p), region_distance(p, [1, 2, 3]), signed_region_distance(p, [1, 1, 1])]", "[8/3, [1, 1, 1/2], 2^(1/2), (-1/5)*5^(1/2)]"),
        ("volume(prism([[1, 0, 1], [0, 0, 0], [2, 0, 0], [1, 2, 1], [0, 2, 0], [2, 2, 0]]))", "2"),
        ("region_nearest(parallelepiped([0, 0, 0], [[1, 0, 0], [1, 1, 0], [0, 1, 1]]), [2, 2, 2])", "[2, 2, 1]"),
        ("[area(torus([0, 0, 0], [1/2, 1])), volume(filled_torus([0, 0, 0], [1/2, 1]))]", "[(3/4)*pi^2, (3/32)*pi^2]"),
        ("[region_measure(half_line([0, 0], [1, 1])), region_centroid(half_line([0, 0], [1, 1])), region_nearest(half_line([0, 0], [1, 2]), [1, 1])]", "[inf, [indeterminate, indeterminate], [3/5, 6/5]]"),
        ("[signed_region_distance(half_plane([0, 0], [1, 0], [0, 1]), [2, 3]), region_nearest(infinite_plane([[0, 0, 0], [1, 0, 0], [0, 3, 1]]), [1, 2, 3])]", "[-3, [1, 27/10, 9/10]]"),
        ("[region_member(conic_hull_region([[0, 0]], [[1, -1], [1, 1]]), [1, 0]), is_bounded_region(infinite_line([[0, 0], [1, 1]]))]", "[true, false]"),
        ("c = cone([[0, 0, 0], [1, 1, 1]], 2)", "null"),
        ("c", "cone([[0, 0, 0], [1, 1, 1]], 2)"),
        ("[volume(c), region_centroid(c), region_distance(c, [1, 2, 3]), signed_region_distance(c, [1/3, 1/4, 1/5])]", "[(4/3)*pi*3^(1/2), [1/4, 1/4, 1/4], 5^(1/2), (-47/180)*3^(1/2)]"),
        ("region_bounds(cylinder([[0, 0, 0], [0, 0, 1]], 2))", "[[-2, 2], [-2, 2], [0, 1]]"),
        ("[region_nearest(ellipsoid([0, 0, 0], [1, 2, 3]), [0, 3, 4]), signed_region_distance(ellipsoid([0, 0, 0], [3, 2, 1]), [2, 0, 0])]", "[[0, 6/5, 12/5], (-1/2)*2^(1/2)]"),
    ]);
}

#[test]
fn triangles_polygons_relations() {
    check(&[
        ("sss_triangle(3, 4, 5)", "triangle([[0, 0], [5, 0], [16/5, 12/5]])"),
        ("aas_triangle(pi/6, pi/3, 1)", "triangle([[0, 0], [2, 0], [3/2, (1/2)*3^(1/2)]])"),
        ("asa_triangle(pi/6, 1, pi/3)", "triangle([[0, 0], [1, 0], [3/4, (1/4)*3^(1/2)]])"),
        ("sas_triangle(1, pi/2, 2)", "triangle([[0, 0], [5^(1/2), 0], [(4/5)*5^(1/2), (2/5)*5^(1/2)]])"),
        ("region_member(sss_triangle(3, 4, 5), [x, y])", "element(x | y, reals) and 3*x >= 4*y and y >= 0 and 3*y + 4*x <= 20"),
        ("region_member(rectangle([0, 0], [2, 2]), [x, y])", "element(x | y, reals) and 0 <= x and x <= 2 and 0 <= y and y <= 2"),
        ("[perimeter(disk()), perimeter(sss_triangle(3, 4, 5))]", "[2*pi, 12]"),
        ("circumsphere([[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1]])", "sphere([1/2, 1/2, 1/2], (1/2)*3^(1/2))"),
        ("[region_moment(ball(), [2, 2, 4]), region_moment(disk(), [2, 2])]", "[(4/3465)*pi, (1/24)*pi]"),
        ("[is_constant_region(disk([0, 0], 1)), is_constant_region(disk([a, b], 1))]", "[true, false]"),
        ("canonicalize_polygon(rectangle())", "polygon([[0, 0], [0, 1], [1, 0], [1, 1]], [1, 3, 4, 2])"),
        ("[is_simple_polygon(polygon([[0, 0], [1, 1], [1, 0], [0, 1]])), is_convex_polygon(rectangle())]", "[false, true]"),
        ("[region_disjoint(line([[0], [1]]), interval([2, 4])), region_equal(point([2]), interval([2, 2])), region_within(interval([-3, 1]), line([[0], [1]]))]", "[true, true, true]"),
        ("bounding_region([[3, 10], [6, 3], [10, 2], [2, 8], [3, 3]], \"MinDisk\")", "disk([13/2, 6], (1/2)*113^(1/2))"),
        ("bounding_region([[3, 10], [6, 3], [10, 2], [2, 8], [3, 3]], \"MinConvexPolygon\")", "polygon([[3, 10], [2, 8], [3, 3], [10, 2]], [1, 2, 3, 4])"),
    ]);
}
