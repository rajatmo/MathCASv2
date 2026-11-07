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
    /// a union of closed intervals of the line
    Intervals(Vec<(T<'h>, T<'h>)>),
    /// the points whose coordinates `vars` satisfy `cond`
    Implicit {
        /// the condition
        cond: T<'h>,
        /// the coordinates' names
        vars: Vec<T<'h>>,
    },
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

fn vector<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Pt<'h>> {
    is_vector(cx, t).then(|| cx.as_list(t).map(|l| l.to_vec())).flatten()
}

/// A list of points of one dimension.
fn vectors<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Pt<'h>>> {
    let l = cx.as_list(t)?;
    let v: Vec<Pt<'h>> = l.iter().map(|&p| vector(cx, p)).collect::<Option<_>>()?;
    let d = v.first()?.len();
    v.iter().all(|p| p.len() == d).then_some(v)
}

fn is_rule(h: &Heap, t: T<'_>) -> bool {
    call_parts(h, t).is_some_and(|(n, a)| n == "rule" && a.len() == 2)
}

/// The options term at the end of a primitive's arguments is not part of the region.
fn without_options<'h>(h: &'h Heap, args: &'h [T<'h>]) -> &'h [T<'h>] {
    match args.split_last() {
        Some((&o, rest)) if call_parts(h, o).is_some_and(|(n, _)| n == "options") => rest,
        _ => args,
    }
}

fn zeros<'h>(h: &'h Heap, n: usize) -> Pt<'h> {
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

fn radius<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>, n: usize) -> Option<Radius<'h>> {
    match cx.as_list(t) {
        Some(l) if l.len() == n && l.iter().all(|&x| cx.as_list(x).is_none()) => Some(Radius::Axes(l.to_vec())),
        Some(_) => None,
        None => Some(Radius::One(t)),
    }
}

/// The region a term describes, or `None` when it is not one this package knows.
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
        ("tetrahedron", []) => Region::Simplex(standard_simplex(h, 3)),
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
            vars.iter().all(|&v| matches!(h.view(v), View::Sym(_))).then_some(())?;
            Region::Implicit { cond: *cond, vars }
        }
        ("parametric_region", [fs, specs]) => {
            let fs = cx.as_list(*fs)?.to_vec();
            let spec = |s: T<'h>| match cx.as_list(s) {
                Some(&[u, a, b]) if matches!(h.view(u), View::Sym(_)) => Some((u, a, b)),
                _ => None,
            };
            let specs = match spec(*specs) {
                Some(s) => vec![s],
                None => cx.as_list(*specs)?.iter().map(|&s| spec(s)).collect::<Option<_>>()?,
            };
            Region::Parametric { fs, specs }
        }
        _ => return None,
    })
}

impl Region<'_> {
    /// The number of coordinates of its points.
    pub fn embedding(&self) -> usize {
        match self {
            Region::Points(p) => p[0].len(),
            Region::Lines(l) => l[0][0].len(),
            Region::Polygon(r) => r[0].outer[0].len(),
            Region::Ball { c, .. } => c.len(),
            Region::Boxed { lo, .. } => lo.len(),
            Region::Simplex(v) => v[0].len(),
            Region::Intervals(_) => 1,
            Region::Implicit { vars, .. } => vars.len(),
            Region::Parametric { fs, .. } => fs.len(),
        }
    }
}
