//! Regions of two dimensions drawn as primitives (round 37): sphere, ellipsoid with a matrix, cuboid of one corner,
//! simplex, empty region. Each shape is checked against its definition.
use mc_core::{Config, Heap, Store};
use mc_graphics::model::{shape, Shape};

fn read<'h>(h: &'h Heap, src: &str) -> mc_api::T<'h> {
    mc_syntax::read(h, src).expect(src)[0]
}

/// The shape of the primitive `src` (a call), read as the 2D graphics reader does.
fn shape_of(h: &Heap, src: &str) -> Option<Shape> {
    let t = read(h, src);
    let mc_api::View::App { head, args } = h.view(t) else { panic!("{src}") };
    let mc_api::View::Sym(s) = h.view(head) else { panic!("{src}") };
    shape(h, &h.sym_name(s), args).ok().flatten()
}

#[test]
fn ellipsoid_with_a_matrix_is_its_quadratic_form() {
    let h = Heap::new(Config::default());
    // the documented sigma [[5, 2], [2, 5]] and two variations (a rotated one with another centre, a diagonal one)
    for (src, c, s) in [
        ("ellipsoid([0, 0], [[5, 2], [2, 5]])", (0.0, 0.0), [5.0, 2.0, 5.0]),
        ("ellipsoid([1, -2], [[2, -1.5], [-1.5, 3]])", (1.0, -2.0), [2.0, -1.5, 3.0]),
        ("ellipsoid([0, 0], [[4, 0], [0, 1]])", (0.0, 0.0), [4.0, 0.0, 1.0]),
    ] {
        let Some(Shape::Polygon(rings)) = shape_of(&h, src) else { panic!("{src}") };
        let det = s[0] * s[2] - s[1] * s[1];
        // sigma^-1 = [[s2, -s1], [-s1, s0]] / det
        for p in &rings[0] {
            let (u, v) = (p.x - c.0, p.y - c.1);
            let q = (s[2] * u * u - 2.0 * s[1] * u * v + s[0] * v * v) / det;
            assert!((q - 1.0).abs() < 1e-12, "{src}: {q}");
        }
        assert_eq!(rings[0].len(), 128);
    }
    // the diagonal case reaches its semi-axes 2 and 1
    let Some(Shape::Polygon(rings)) = shape_of(&h, "ellipsoid([0, 0], [[4, 0], [0, 1]])") else { panic!() };
    let xmax = rings[0].iter().map(|p| p.x).fold(f64::MIN, f64::max);
    let ymax = rings[0].iter().map(|p| p.y).fold(f64::MIN, f64::max);
    assert!((xmax - 2.0).abs() < 1e-12 && (ymax - 1.0).abs() < 1e-3, "{xmax} {ymax}");
    // not symmetric, not positive definite: not an ellipsoid
    assert!(shape_of(&h, "ellipsoid([0, 0], [[5, 2], [3, 5]])").is_none());
    assert!(shape_of(&h, "ellipsoid([0, 0], [[1, 2], [2, 1]])").is_none());
}

#[test]
fn sphere_cuboid_simplex_and_empty_region_in_the_plane() {
    let h = Heap::new(Config::default());
    // the sphere of the plane is the circle, of any centre and radius
    assert_eq!(shape_of(&h, "sphere([1/2, 1/2], 0.7071067811865476)"), shape_of(&h, "circle([1/2, 1/2], 0.7071067811865476)"));
    assert_eq!(shape_of(&h, "sphere()"), shape_of(&h, "circle()"));
    assert_eq!(shape_of(&h, "sphere([3, 1])"), shape_of(&h, "circle([3, 1])"));
    // a sphere of three dimensions is not a 2D primitive
    assert!(shape_of(&h, "sphere([1, 2, 3])").is_none());
    // cuboid(p) is the unit square at p
    assert_eq!(shape_of(&h, "cuboid([1, 1])"), shape_of(&h, "rectangle([1, 1], [2, 2])"));
    assert_eq!(shape_of(&h, "cuboid([-1, 0.5])"), shape_of(&h, "rectangle([-1, 0.5], [0, 1.5])"));
    // simplices: a triangle, a segment, a point; simplex(2) is the standard triangle
    assert_eq!(shape_of(&h, "simplex([[0, 0], [1, 1], [2, 0]])"), shape_of(&h, "polygon([[0, 0], [1, 1], [2, 0]])"));
    assert_eq!(shape_of(&h, "simplex([[0, 0], [1, 1]])"), shape_of(&h, "line([[0, 0], [1, 1]])"));
    assert_eq!(shape_of(&h, "simplex([[2, 3]])"), shape_of(&h, "point([[2, 3]])"));
    assert_eq!(shape_of(&h, "simplex(2)"), shape_of(&h, "triangle()"));
    assert_eq!(shape_of(&h, "empty_region(2)"), Some(Shape::Polygon(vec![])));
}
