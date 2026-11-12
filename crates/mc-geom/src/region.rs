//! Reading region terms: the graphics primitives that are regions (`point`, `line`, `polygon`, `triangle`,
//! `tetrahedron`, `simplex`, `disk`, `circle`, `ball`, `sphere`, `rectangle`, `cuboid`), `interval`, and
//! `implicit_region` and `parametric_region`, with coordinates kept as terms (exact or symbolic).
use mc_core::{Heap, Store, View, T};
use mc_eval::NativeCx;

/// A point as its coordinates.
pub type Pt<'h> = Vec<T<'h>>;

/// The radius of a disk, ball, circle or sphere: one, or one per axis (an axis-aligned ellipse or ellipsoid).
#[derive(Clone, Debug)]
pub enum Radius<'h> {
    /// the same radius on every axis
    One(T<'h>),
    /// a radius per axis
    Axes(Vec<T<'h>>),
}

/// A polygon's ring with the holes cut out of it.
#[derive(Clone, Debug)]
pub struct Ring<'h> {
    /// the outer boundary
    pub outer: Vec<Pt<'h>>,
    /// the holes
    pub holes: Vec<Vec<Pt<'h>>>,
}

/// A region read from its term.
#[derive(Clone, Debug)]
pub enum Region<'h> {
    /// finitely many points
    Points(Vec<Pt<'h>>),
    /// polylines
    Lines(Vec<Vec<Pt<'h>>>),
    /// polygons (in the plane, or planar in space), each with its holes
    Polygon(Vec<Ring<'h>>),
    /// a disk or ball (`solid`), or a circle or sphere
    Ball {
        /// the centre
        c: Pt<'h>,
        /// the radius
        r: Radius<'h>,
        /// the inside too, not only the boundary
        solid: bool,
    },
    /// an axis-aligned box: rectangle, cuboid
    Boxed {
        /// the lower corner
        lo: Pt<'h>,
        /// the upper corner
        hi: Pt<'h>,
    },
    /// the convex hull of k + 1 points (a segment, triangle, tetrahedron, ...)
    Simplex(Vec<Pt<'h>>),
    /// the ring between two concentric circles of the plane
    Annulus {
        /// the centre
        c: Pt<'h>,
        /// the inner radius
        r0: T<'h>,
        /// the outer radius
        r1: T<'h>,
    },
    /// a union of closed intervals of the line
    Intervals(Vec<(T<'h>, T<'h>)>),
    /// the points whose coordinates `vars` satisfy `cond`
    Implicit {
        /// the condition
        cond: T<'h>,
        /// the coordinates' names
        vars: Vec<T<'h>>,
    },
    /// a closed solid bounded by plane faces (pyramid, prism, hexahedron, parallelepiped; taken as convex for
    /// membership), its faces as index lists into `verts`
    Polyhedron {
        /// the corners
        verts: Vec<Pt<'h>>,
        /// the faces, each a ring of corner indices
        faces: Vec<Vec<usize>>,
    },
    /// a torus about the z axis through `c` with inner radius `r0` and outer radius `r1` (the surface, or the solid
    /// when `solid`)
    Torus {
        /// the centre
        c: Pt<'h>,
        /// the inner radius (centre to the hole's edge)
        r0: T<'h>,
        /// the outer radius
        r1: T<'h>,
        /// the solid torus
        solid: bool,
    },
    /// the points p + sum s_i f_i + sum t_j g_j with s_i any real and t_j >= 0: lines, half-lines, planes,
    /// half-planes and cones
    Affine {
        /// the base point
        p: Pt<'h>,
        /// directions taken with any sign
        free: Vec<Pt<'h>>,
        /// directions taken with non-negative coefficients
        pos: Vec<Pt<'h>>,
    },
    /// a solid cylinder (or cone, apex at `b`) about the axis from `a` to `b`, of radius `r` (at `a` for a cone)
    Revolved {
        /// the centre of the (base) end
        a: Pt<'h>,
        /// the centre of the other end (the apex of a cone)
        b: Pt<'h>,
        /// the radius
        r: T<'h>,
        /// a cone
        cone: bool,
    },
    /// a union of pieces that meet only on their boundaries (mesh regions): measures add up
    Union(Vec<Region<'h>>),
    /// the image of a box of parameters
    Parametric {
        /// the coordinates as functions of the parameters
        fs: Vec<T<'h>>,
        /// each parameter with its range
        specs: Vec<(T<'h>, T<'h>, T<'h>)>,
    },
}

/// The head's name and the arguments of `t`, if it is a call with a symbol head.
pub fn call_parts<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
    match h.view(t) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s).to_string(), args)),
            _ => None,
        },
        _ => None,
    }
}

/// Whether `t` is a list whose elements are not lists: a point.
pub fn is_vector<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
    cx.as_list(t).is_some_and(|l| !l.is_empty() && l.iter().all(|&x| cx.as_list(x).is_none()))
}

pub(crate) fn vector<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Pt<'h>> {
    is_vector(cx, t).then(|| cx.as_list(t).map(|l| l.to_vec())).flatten()
}

/// A list of points of one dimension.
pub(crate) fn vectors<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Pt<'h>>> {
    let l = cx.as_list(t)?;
    let v: Vec<Pt<'h>> = l.iter().map(|&p| vector(cx, p)).collect::<Option<_>>()?;
    let d = v.first()?.len();
    v.iter().all(|p| p.len() == d).then_some(v)
}

fn is_rule(h: &Heap, t: T<'_>) -> bool {
    call_parts(h, t).is_some_and(|(n, a)| n == "rule" && a.len() == 2)
}

/// The options term at the end of a primitive's arguments is not part of the region.
pub(crate) fn without_options<'h>(h: &'h Heap, args: &'h [T<'h>]) -> &'h [T<'h>] {
    match args.split_last() {
        Some((&o, rest)) if call_parts(h, o).is_some_and(|(n, _)| n == "options") => rest,
        _ => args,
    }
}

pub(crate) fn zeros<'h>(h: &'h Heap, n: usize) -> Pt<'h> {
    vec![h.int(0); n]
}

/// The unit vectors of n dimensions with the origin first: the standard simplex.
fn standard_simplex<'h>(h: &'h Heap, n: usize) -> Vec<Pt<'h>> {
    let mut v = vec![zeros(h, n)];
    for i in 0..n {
        let mut p = zeros(h, n);
        p[i] = h.int(1);
        v.push(p);
    }
    v
}

/// The regular tetrahedron with edge 1 and centroid at the origin, one face parallel to the xy plane:
/// (0, 0, √6/4), (-√3/6, ∓1/2, -√6/12), (√3/3, 0, -√6/12).
fn regular_tetrahedron<'h>(h: &'h Heap) -> Vec<Pt<'h>> {
    let (mul, pow) = (h.sym_term(h.sym(mc_api::names::MUL)), h.sym_term(h.sym(mc_api::names::POW)));
    let surd = |q: (i64, i64), n: i64| h.app(mul, &[h.rational(h.int(q.0), h.int(q.1)), h.app(pow, &[h.int(n), h.rational(h.int(1), h.int(2))])]);
    let half = h.rational(h.int(1), h.int(2));
    let neg_half = h.rational(h.int(-1), h.int(2));
    let low = surd((-1, 12), 6);
    vec![
        vec![h.int(0), h.int(0), surd((1, 4), 6)],
        vec![surd((-1, 6), 3), neg_half, low],
        vec![surd((-1, 6), 3), half, low],
        vec![surd((1, 3), 3), h.int(0), low],
    ]
}

fn radius<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, n: usize) -> Option<Radius<'h>> {
    match cx.as_list(t) {
        Some(l) if l.len() == n && l.iter().all(|&x| cx.as_list(x).is_none()) => Some(Radius::Axes(l.to_vec())),
        Some(_) => None,
        None => Some(Radius::One(t)),
    }
}

/// The region a term describes, or `None` when it is not one this crate knows.
pub fn read<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Region<'h>> {
    let h = cx.heap();
    let (name, all) = call_parts(h, t)?;
    let args = without_options(h, all);
    let one = || h.int(1);
    Some(match (name.as_str(), args) {
        ("region", [r]) => return read(cx, *r),
        ("point", [p]) => match vector(cx, *p) {
            Some(v) => Region::Points(vec![v]),
            None => Region::Points(vectors(cx, *p)?),
        },
        ("line", [p]) => match vectors(cx, *p) {
            Some(v) if v.len() >= 2 => Region::Lines(vec![v]),
            Some(_) => return None,
            None => {
                let parts: Vec<Vec<Pt<'h>>> = cx.as_list(*p)?.iter().map(|&q| vectors(cx, q).filter(|v| v.len() >= 2)).collect::<Option<_>>()?;
                Region::Lines(parts)
            }
        },
        ("polygon", [r]) if is_rule(h, *r) => {
            let (_, ab) = call_parts(h, *r)?;
            let outer = vectors(cx, ab[0])?;
            let holes = match vectors(cx, ab[1]) {
                Some(v) => vec![v],
                None => cx.as_list(ab[1])?.iter().map(|&q| vectors(cx, q)).collect::<Option<_>>()?,
            };
            Region::Polygon(vec![Ring { outer, holes }])
        }
        ("polygon", [p]) => match vectors(cx, *p) {
            Some(v) if v.len() >= 3 => Region::Polygon(vec![Ring { outer: v, holes: vec![] }]),
            Some(_) => return None,
            None => {
                let rings: Vec<Ring<'h>> = cx
                    .as_list(*p)?
                    .iter()
                    .map(|&q| vectors(cx, q).filter(|v| v.len() >= 3).map(|outer| Ring { outer, holes: vec![] }))
                    .collect::<Option<_>>()?;
                Region::Polygon(rings)
            }
        },
        ("triangle", []) => Region::Simplex(standard_simplex(h, 2)),
        ("triangle", [p]) => Region::Simplex(vectors(cx, *p).filter(|v| v.len() == 3)?),
        ("tetrahedron", []) => Region::Simplex(regular_tetrahedron(h)),
        ("tetrahedron", [p]) => Region::Simplex(vectors(cx, *p).filter(|v| v.len() == 4 && v[0].len() == 3)?),
        ("simplex", [p]) => match h.view(*p) {
            View::Int(n) if (0..=12).contains(&n) => Region::Simplex(standard_simplex(h, n as usize)),
            _ => {
                let v = vectors(cx, *p)?;
                (v.len() <= v[0].len() + 1).then_some(())?;
                Region::Simplex(v)
            }
        },
        ("disk" | "circle", []) => Region::Ball { c: zeros(h, 2), r: Radius::One(one()), solid: name == "disk" },
        ("ball" | "sphere", []) => Region::Ball { c: zeros(h, 3), r: Radius::One(one()), solid: name == "ball" },
        ("disk" | "circle" | "ball" | "sphere", [c]) => {
            let c = vector(cx, *c)?;
            Region::Ball { c, r: Radius::One(one()), solid: matches!(name.as_str(), "disk" | "ball") }
        }
        ("disk" | "circle" | "ball" | "sphere" | "ellipsoid", [c, r]) => {
            let c = vector(cx, *c)?;
            let r = radius(cx, *r, c.len())?;
            Region::Ball { c, r, solid: name != "circle" && name != "sphere" }
        }
        ("annulus", []) => Region::Annulus { c: zeros(h, 2), r0: h.rational(h.int(1), h.int(2)), r1: one() },
        ("annulus", [c]) => Region::Annulus { c: vector(cx, *c).filter(|v| v.len() == 2)?, r0: h.rational(h.int(1), h.int(2)), r1: one() },
        ("annulus", [c, r]) => {
            let Some(&[r0, r1]) = cx.as_list(*r) else { return None };
            Region::Annulus { c: vector(cx, *c).filter(|v| v.len() == 2)?, r0, r1 }
        }
        ("rectangle", []) => Region::Boxed { lo: zeros(h, 2), hi: vec![one(), one()] },
        ("cuboid", []) => Region::Boxed { lo: zeros(h, 3), hi: vec![one(), one(), one()] },
        ("rectangle" | "cuboid", [a]) => {
            let lo = vector(cx, *a)?;
            let hi = lo.iter().map(|&x| h.call(h.sym("add"), &[x, one()])).collect();
            Region::Boxed { lo, hi }
        }
        ("rectangle" | "cuboid", [a, b]) => {
            let (lo, hi) = (vector(cx, *a)?, vector(cx, *b)?);
            (lo.len() == hi.len()).then_some(())?;
            Region::Boxed { lo, hi }
        }
        ("interval", parts) if !parts.is_empty() => {
            let v = parts
                .iter()
                .map(|&p| match cx.as_list(p) {
                    Some(&[a, b]) => Some((a, b)),
                    _ => None,
                })
                .collect::<Option<_>>()?;
            Region::Intervals(v)
        }
        ("implicit_region", [cond, vars]) => {
            let vars = match cx.as_list(*vars) {
                Some(l) => l.to_vec(),
                None => vec![*vars],
            };
            // a variable with a range [z, a, b] adds a <= z <= b to the condition
            let mut cond = *cond;
            let mut names = vec![];
            for v in vars {
                match cx.as_list(v) {
                    Some(&[z, a, b]) if matches!(h.view(z), View::Sym(_)) => {
                        let le = |x: T<'h>, y: T<'h>| h.call(h.sym("less_equal"), &[x, y]);
                        let range = h.call(h.sym("and"), &[le(a, z), le(z, b)]);
                        cond = h.call(h.sym("and"), &[cond, range]);
                        names.push(z);
                    }
                    _ if matches!(h.view(v), View::Sym(_)) => names.push(v),
                    _ => return None,
                }
            }
            Region::Implicit { cond, vars: names }
        }
        ("parametric_region", [fs, specs]) => {
            let fs = cx.as_list(*fs)?.to_vec();
            let inf = || h.sym_term(h.sym("inf"));
            let spec = |s: T<'h>| match cx.as_list(s) {
                // [u, a, b] with scalar bounds ([t, [r, 0, 1], [z, 0, 2]] is three specs, not t from a list to a list)
                Some(&[u, a, b]) if matches!(h.view(u), View::Sym(_)) && cx.as_list(a).is_none() && cx.as_list(b).is_none() => Some((u, a, b)),
                // a bare parameter runs over all reals
                None if matches!(h.view(s), View::Sym(_)) => Some((s, h.call(h.sym("mul"), &[h.int(-1), inf()]), inf())),
                _ => None,
            };
            let specs = match spec(*specs) {
                Some(s) if cx.as_list(*specs).is_some() => vec![s],
                _ => cx.as_list(*specs)?.iter().map(|&s| spec(s)).collect::<Option<_>>()?,
            };
            Region::Parametric { fs, specs }
        }
        _ => return crate::shapes::read_more(cx, &name, args),
    })
}

impl Region<'_> {
    /// The number of coordinates of its points.
    pub fn embedding(&self) -> usize {
        match self {
            Region::Points(p) => p[0].len(),
            Region::Lines(l) => l[0][0].len(),
            Region::Polygon(r) => r[0].outer[0].len(),
            Region::Ball { c, .. } | Region::Annulus { c, .. } => c.len(),
            Region::Boxed { lo, .. } => lo.len(),
            Region::Simplex(v) => v[0].len(),
            Region::Intervals(_) => 1,
            Region::Implicit { vars, .. } => vars.len(),
            Region::Parametric { fs, .. } => fs.len(),
            Region::Polyhedron { verts, .. } => verts[0].len(),
            Region::Torus { c, .. } | Region::Affine { p: c, .. } | Region::Revolved { a: c, .. } => c.len(),
            Region::Union(parts) => parts[0].embedding(),
        }
    }
}
