//! More region primitives read into [`Region`]: polyhedra (`pyramid`, `prism`, `hexahedron`, `parallelepiped`),
//! `parallelogram`, `torus` and `filled_torus`, and the unbounded flats and cones (`infinite_line`, `half_line`,
//! `infinite_plane`, `half_plane`, `conic_hull_region`); mesh regions are read by [`crate::mesh`].
//!
//! Face lists follow the usual corner orders: a pyramid's base is its first four corners and the apex the fifth; a
//! prism's ends are corners 1-3 and 4-6; a hexahedron's bottom 1-4 and top 5-8 (as in finite-element meshes, see
//! e.g. the VTK file format documentation for cell types 12-14).
use crate::ex::E;
use crate::region::{vector, vectors, zeros, Pt, Region};
use mc_core::{Store, T};
use mc_eval::NativeCx;

/// The faces of a pyramid with a quadrilateral base.
pub const PYRAMID: &[&[usize]] = &[&[0, 1, 2, 3], &[0, 1, 4], &[1, 2, 4], &[2, 3, 4], &[3, 0, 4]];
/// The faces of a triangular prism.
pub const PRISM: &[&[usize]] = &[&[0, 1, 2], &[3, 4, 5], &[0, 1, 4, 3], &[1, 2, 5, 4], &[2, 0, 3, 5]];
/// The faces of a hexahedron.
pub const HEXAHEDRON: &[&[usize]] = &[&[0, 1, 2, 3], &[4, 5, 6, 7], &[0, 1, 5, 4], &[1, 2, 6, 5], &[2, 3, 7, 6], &[3, 0, 4, 7]];

fn faces(f: &[&[usize]]) -> Vec<Vec<usize>> {
    f.iter().map(|x| x.to_vec()).collect()
}

/// The corners p, p + v1, p + v1 + v2, p + v2 (and the same plus v3) of a parallelogram or parallelepiped.
fn parallel_corners<'h>(e: E<'h>, p: &[T<'h>], vs: &[Pt<'h>]) -> Vec<Pt<'h>> {
    let mut base = vec![p.to_vec(), e.vadd(p, &vs[0]), e.vadd(&e.vadd(p, &vs[0]), &vs[1]), e.vadd(p, &vs[1])];
    if vs.len() == 3 {
        let top: Vec<Pt<'h>> = base.iter().map(|q| e.vadd(q, &vs[2])).collect();
        base.extend(top);
    }
    base
}

/// Two points (or a point and a direction) of a line: the base point and the direction.
fn point_and_direction<'h>(cx: &NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Option<(Pt<'h>, Pt<'h>)> {
    let e = E { h: cx.heap() };
    match args {
        [ps] => {
            let v = vectors(cx, *ps)?;
            (v.len() == 2).then(|| (v[0].clone(), e.vsub(&v[1], &v[0])))
        }
        [p, d] => {
            let (p, d) = (vector(cx, *p)?, vector(cx, *d)?);
            (p.len() == d.len()).then_some((p, d))
        }
        _ => None,
    }
}

/// The regions of this module, or `None`.
pub fn read_more<'h>(cx: &NativeCx<'_, '_, 'h>, name: &str, args: &[T<'h>]) -> Option<Region<'h>> {
    let h = cx.heap();
    let e = E { h };
    Some(match (name, args) {
        ("pyramid", [p]) => Region::Polyhedron { verts: vectors(cx, *p).filter(|v| v.len() == 5 && v[0].len() == 3)?, faces: faces(PYRAMID) },
        ("prism", [p]) => Region::Polyhedron { verts: vectors(cx, *p).filter(|v| v.len() == 6 && v[0].len() == 3)?, faces: faces(PRISM) },
        ("hexahedron", [p]) => Region::Polyhedron { verts: vectors(cx, *p).filter(|v| v.len() == 8 && v[0].len() == 3)?, faces: faces(HEXAHEDRON) },
        ("parallelogram", []) => {
            let vs = vec![vec![h.int(1), h.int(0)], vec![h.int(1), h.int(1)]];
            Region::Polygon(vec![crate::region::Ring { outer: parallel_corners(e, &zeros(h, 2), &vs), holes: vec![] }])
        }
        ("parallelogram" | "parallelepiped", [p, vs]) => {
            let p = vector(cx, *p)?;
            let vs = vectors(cx, *vs).filter(|v| v[0].len() == p.len())?;
            match vs.len() {
                1 => Region::Lines(vec![vec![p.clone(), e.vadd(&p, &vs[0])]]),
                2 => Region::Polygon(vec![crate::region::Ring { outer: parallel_corners(e, &p, &vs), holes: vec![] }]),
                3 if p.len() == 3 => Region::Polyhedron { verts: parallel_corners(e, &p, &vs), faces: faces(HEXAHEDRON) },
                // any other dimension: p + t1 v1 + ... + tk vk with each t in [0, 1]
                k if k <= 8 && k <= p.len() => {
                    let ts: Vec<T<'h>> = (1..=k).map(|i| h.sym_term(h.sym(&format!("parallelepiped$t{i}")))).collect();
                    let (add, mul) = (h.sym_term(h.sym(mc_api::names::ADD)), h.sym_term(h.sym(mc_api::names::MUL)));
                    let fs: Vec<T<'h>> = (0..p.len())
                        .map(|c| {
                            let mut terms = vec![p[c]];
                            terms.extend((0..k).map(|i| h.app(mul, &[vs[i][c], ts[i]])));
                            h.app(add, &terms)
                        })
                        .collect();
                    Region::Parametric { fs, specs: ts.into_iter().map(|t| (t, h.int(0), h.int(1))).collect() }
                }
                _ => return None,
            }
        }
        ("torus" | "filled_torus", []) => {
            Region::Torus { c: zeros(h, 3), r0: h.rational(h.int(1), h.int(2)), r1: h.int(1), solid: name == "filled_torus" }
        }
        ("torus" | "filled_torus", [c]) => {
            Region::Torus { c: vector(cx, *c).filter(|c| c.len() == 3)?, r0: h.rational(h.int(1), h.int(2)), r1: h.int(1), solid: name == "filled_torus" }
        }
        ("torus" | "filled_torus", [c, r]) => {
            let &[r0, r1] = cx.as_list(*r)? else { return None };
            Region::Torus { c: vector(cx, *c).filter(|c| c.len() == 3)?, r0, r1, solid: name == "filled_torus" }
        }
        ("infinite_line", a) => {
            let (p, d) = point_and_direction(cx, a)?;
            Region::Affine { p, free: vec![d], pos: vec![] }
        }
        ("half_line", a) => {
            let (p, d) = point_and_direction(cx, a)?;
            Region::Affine { p, free: vec![], pos: vec![d] }
        }
        ("infinite_plane", [ps]) => {
            let v = vectors(cx, *ps).filter(|v| v.len() == 3)?;
            Region::Affine { p: v[0].clone(), free: vec![e.vsub(&v[1], &v[0]), e.vsub(&v[2], &v[0])], pos: vec![] }
        }
        ("infinite_plane", [p, vs]) => {
            let p = vector(cx, *p)?;
            let vs = vectors(cx, *vs).filter(|v| v.len() == 2 && v[0].len() == p.len())?;
            Region::Affine { p, free: vs, pos: vec![] }
        }
        ("half_plane", [ps, w]) => {
            let (p, d) = point_and_direction(cx, &[*ps])?;
            let w = vector(cx, *w).filter(|w| w.len() == p.len())?;
            Region::Affine { p, free: vec![d], pos: vec![w] }
        }
        ("half_plane", [p, v, w]) => {
            let (p, v, w) = (vector(cx, *p)?, vector(cx, *v)?, vector(cx, *w)?);
            (p.len() == v.len() && v.len() == w.len()).then_some(())?;
            Region::Affine { p, free: vec![v], pos: vec![w] }
        }
        ("conic_hull_region", [ps]) => {
            let v = vectors(cx, *ps).filter(|v| v.len() >= 2)?;
            Region::Affine { p: v[0].clone(), free: vec![], pos: v[1..].iter().map(|q| e.vsub(q, &v[0])).collect() }
        }
        ("conic_hull_region", [ps, ws]) => {
            let v = vectors(cx, *ps)?;
            let w = vectors(cx, *ws).filter(|w| w[0].len() == v[0].len())?;
            let mut pos: Vec<Pt<'h>> = v[1..].iter().map(|q| e.vsub(q, &v[0])).collect();
            pos.extend(w);
            Region::Affine { p: v[0].clone(), free: vec![], pos }
        }
        ("polygon", [ps, idx]) => {
            let v = vectors(cx, *ps)?;
            let ix = |l: &[T<'h>]| -> Option<Vec<Pt<'h>>> {
                l.iter().map(|&i| match h.view(i) {
                    mc_core::View::Int(k) if k >= 1 && (k as usize) <= v.len() => Some(v[k as usize - 1].clone()),
                    _ => None,
                }).collect()
            };
            let l = cx.as_list(*idx)?;
            let rings: Vec<Vec<Pt<'h>>> = match ix(l) {
                Some(r) => vec![r],
                None => l.iter().map(|&x| cx.as_list(x).and_then(ix)).collect::<Option<_>>()?,
            };
            if rings.iter().any(|r| r.len() < 3) {
                return None;
            }
            Region::Polygon(rings.into_iter().map(|outer| crate::region::Ring { outer, holes: vec![] }).collect())
        }
        ("cylinder" | "cone", []) => {
            let (a, b) = (vec![h.int(0), h.int(0), h.int(-1)], vec![h.int(0), h.int(0), h.int(1)]);
            Region::Revolved { a, b, r: h.int(1), cone: name == "cone" }
        }
        ("cylinder" | "cone", [ps]) | ("cylinder" | "cone", [ps, _]) => {
            let v = vectors(cx, *ps).filter(|v| v.len() == 2)?;
            let r = args.get(1).copied().unwrap_or_else(|| h.int(1));
            if cx.as_list(r).is_some() {
                return None;
            }
            Region::Revolved { a: v[0].clone(), b: v[1].clone(), r, cone: name == "cone" }
        }
        ("mesh_region" | "boundary_mesh_region", _) => return crate::mesh::region(cx, name, args),
        _ => return None,
    })
}
