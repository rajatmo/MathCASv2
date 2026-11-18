//! Positions in scaled coordinates in unbounded regions and in 3D primitives, conic hulls with lines,
//! transformations about a position of the object's box, curves given by segment specifications, rasters of gray
//! levels with opacity, `annulus(r)`, insets with options and texts with a direction. Each picture is read by the
//! graphics reader and checked against the definition of the object.
use mc_core::{Config, Heap};
use mc_graphics::model::{read, Pt, Shape};

fn shapes(src: &str) -> Vec<Shape> {
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, src).expect(src)[0];
    read(&h, t, &|| false).unwrap_or_else(|f| panic!("{src}: {f:?}")).items.into_iter().map(|i| i.shape).collect()
}

fn one(src: &str) -> Shape {
    let mut v = shapes(src);
    assert_eq!(v.len(), 1, "{src}: {v:?}");
    v.remove(0)
}

fn ring_of(src: &str) -> Vec<Pt> {
    match shapes(src).as_slice() {
        [Shape::Polygon(rings)] if rings.len() == 1 => rings[0].clone(),
        other => panic!("{src}: {other:?}"),
    }
}

#[test]
fn half_planes_in_scaled_coordinates() {
    // the boundary through (0, 2.5) and (10, 5) of the range [0, 10]^2, the side of +y
    let ring = ring_of("graphics(half_plane([scaled([0, 0.25]), scaled([1, 0.5])], scaled([0, 1])), options(plot_range -> [[0, 10], [0, 10]]))");
    assert!(ring.iter().all(|p| p.y - (2.5 + p.x / 4.0) >= -1e-9), "{ring:?}");
    assert!(ring.iter().any(|p| (p.y - (2.5 + p.x / 4.0)).abs() < 1e-9));
    // scaled(d, p): p shifted by d of the range: the line through (2, 2) and (1, 4) in [0, 4]^2
    let ring = ring_of("graphics(half_plane([scaled([0, 0.5], [2, 0]), scaled([0, 0.5], [1, 2])], scaled([0, 1])), options(plot_range -> [[0, 4], [0, 4]]))");
    // the line 2x + y = 6, and the side of +y (the scaled vector [0, 1] is [0, 4]): 2x + y >= 6
    let side = |p: &Pt| 6.0 - (2.0 * p.x + p.y);
    assert!(ring.iter().all(|p| side(p) <= 1e-9), "{ring:?}");
    assert!(ring.iter().any(|p| side(p).abs() < 1e-9));
}

#[test]
fn conic_hulls_with_lines() {
    // [0, 0] + a [1, 0] + b [0, 1], a real and b >= 0: the upper half-plane
    let ring = ring_of("graphics(conic_hull_region([0, 0], [[1, 0]], [[0, 1]]), options(plot_range -> [[-1, 2], [-1, 2]]))");
    assert!(ring.iter().all(|p| p.y >= -1e-12), "{ring:?}");
    assert!(ring.iter().any(|p| p.x < -1.0) && ring.iter().any(|p| p.x > 2.0));
    // rays on both sides of the line, or two lines: the whole plane
    for src in [
        "graphics(conic_hull_region([0, 0], [[1, 0]], [[0, 1], [1, -1]]), options(plot_range -> [[-1, 2], [-1, 2]]))",
        "graphics(conic_hull_region([[0, 0]], [[1, 0], [1, 1]], []), options(plot_range -> [[-1, 2], [-1, 2]]))",
    ] {
        let ring = ring_of(src);
        assert_eq!(ring.len(), 4, "{src}");
        assert!(ring.iter().any(|p| p.y < -1.0) && ring.iter().any(|p| p.y > 2.0), "{src}");
    }
    // a line alone (no rays, or rays along it)
    for src in ["graphics(conic_hull_region([0, 0], [[1, 1]], []))", "graphics(conic_hull_region([0, 0], [[1, 1]], [[2, 2]]))"] {
        assert!(matches!(shapes(src).as_slice(), [Shape::Line(_)]), "{src}");
    }
}

#[test]
fn matrices_about_a_position_of_the_box() {
    // the box of the points is [1, 2] x [1, 3]; `right` is (2, 2), `left` (1, 2), `center` (1.5, 2), `top` (1.5, 3)
    for (pos, want) in [
        ("right", [(0.0, -1.0), (2.0, 5.0)]),
        ("left", [(1.0, -1.0), (3.0, 5.0)]),
        ("center", [(0.5, -1.0), (2.5, 5.0)]),
        ("top", [(0.5, -3.0), (2.5, 3.0)]),
    ] {
        let src = format!("graphics(geometric_transformation(point([[1, 1], [2, 3]]), [[[2, 0], [0, 3]], {pos}]))");
        let Shape::Points(ps) = one(&src) else { panic!("{src}") };
        for (p, w) in ps.iter().zip(want) {
            assert!((p.x - w.0).abs() < 1e-12 && (p.y - w.1).abs() < 1e-12, "{src}: {ps:?}");
        }
    }
    // a translation vector is not a position
    let Shape::Points(ps) = one("graphics(geometric_transformation(point([1, 1]), [[[2, 0], [0, 3]], [5, 5]]))") else { panic!() };
    assert_eq!((ps[0].x, ps[0].y), (7.0, 8.0));
}

#[test]
fn curves_from_segment_specifications() {
    // a cubic Bezier piece of 4 points, then a line to 2 more points continuing from the last
    let src = "graphics(filled_curve([[1, 4, 3], [0, 2, 0]], [[0, 0], [1, 1], [2, 1], [3, 0], [4, 0], [5, 0]]))";
    let Shape::Polygon(rings) = one(src) else { panic!("{src}") };
    let r = &rings[0];
    assert_eq!((r[0].x, r[0].y), (0.0, 0.0));
    assert_eq!((r[r.len() - 1].x, r[r.len() - 1].y), (5.0, 0.0));
    assert!(r.iter().any(|p| (p.x, p.y) == (4.0, 0.0)));
    // the Bezier curve's midpoint (t = 1/2): (0 + 3 + 6 + 3)/8 = 1.5, (0 + 3 + 3 + 0)/8 = 0.75
    assert!(r.iter().any(|p| (p.x - 1.5).abs() < 1e-12 && (p.y - 0.75).abs() < 1e-12));
    // two components, the points of each in their own list
    let src = "graphics(joined_curve([[[0, 2, 0]], [[0, 3, 0]]], [[[0, 0], [1, 0]], [[0, 1], [1, 1], [2, 1]]]))";
    let Shape::Line(runs) = one(src) else { panic!("{src}") };
    assert_eq!(runs.iter().map(Vec::len).collect::<Vec<_>>(), [2, 3]);
    // more points asked than given: not such a curve
    let h = Heap::new(Config::default());
    let t = mc_syntax::read(&h, "graphics(filled_curve([[1, 4, 3], [1, 3, 3]], [[0, 0], [1, 1], [2, 1], [3, 0]]))").unwrap()[0];
    assert!(read(&h, t, &|| false).is_err());
}

#[test]
fn rasters_of_gray_and_opacity_and_rings() {
    let Shape::Raster { rgba, .. } = one("graphics(raster([[[0.5, 0.25], [1, 1]]]))") else { panic!() };
    assert_eq!(rgba.len(), 8);
    assert!(rgba[0] == rgba[1] && rgba[1] == rgba[2] && (127..=128).contains(&rgba[0]) && (63..=64).contains(&rgba[3]), "{rgba:?}");
    assert_eq!(&rgba[4..], &[255, 255, 255, 255]);
    // annulus(r): the ring between r/2 and r about the origin
    let Shape::Polygon(rings) = one("graphics(annulus(3))") else { panic!() };
    let radius = |r: &[Pt]| r.iter().map(|p| p.x.hypot(p.y)).fold(0.0, f64::max);
    assert!((radius(&rings[0]) - 3.0).abs() < 1e-12 && (radius(&rings[1]) - 1.5).abs() < 1e-12);
    // the inner circle runs the other way, so the non-zero fill leaves the hole empty (both forms)
    let area = |r: &[Pt]| (0..r.len()).map(|i| r[i].x * r[(i + 1) % r.len()].y - r[(i + 1) % r.len()].x * r[i].y).sum::<f64>();
    assert!(area(&rings[0]) > 0.0 && area(&rings[1]) < 0.0);
    let Shape::Polygon(rings) = one("graphics(annulus([0, 0], [1, 2]))") else { panic!() };
    assert!(area(&rings[0]) > 0.0 && area(&rings[1]) < 0.0);
}

#[test]
fn insets_with_options_texts_with_a_direction_and_scaled_space() {
    assert!(matches!(shapes("graphics(inset(graphics(disk([0, 0])), options(background -> yellow)))").as_slice(), [Shape::Inset { .. }]));
    assert!(matches!(shapes("graphics(inset(graphics(disk([0, 0])), background -> yellow))").as_slice(), [Shape::Inset { .. }]));
    assert!(matches!(shapes("graphics(text(\"a\", [0, 0], automatic, [2, 1]))").as_slice(), [Shape::Text { .. }]));
    // lines and polygons of space at fractions of the box
    let h = Heap::new(Config::default());
    for src in [
        "graphics3d([line([scaled([0, 0.2, 0]), scaled([1, 0.8, 1])]), point([0, 0, 0])])",
        "graphics3d(polygon([scaled([0, 0, 0.2]), scaled([0.5, 1, 0.8]), [1, 1, 1]]))",
        "graphics3d(point([scaled([0.5, 0.5, 0.5])]))",
        // a polygon of space with a hole
        "graphics3d(polygon([[0, 0, 0], [2, 0, 0], [1, 1.7, 0]] -> [[[0.5, 0.3, 0], [1.5, 0.3, 0], [1, 1.2, 0]]]))",
    ] {
        let t = mc_syntax::read(&h, src).unwrap()[0];
        assert!(read(&h, t, &|| false).is_ok(), "{src}");
    }
}
