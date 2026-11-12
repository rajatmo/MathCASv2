#![allow(clippy::too_many_arguments, clippy::needless_range_loop, clippy::type_complexity)]
//! Pictures in space:
//!
//! - slice plots: `slice_density_plot3d(f, surface, [x, ...], [y, ...], [z, ...])`, `slice_contour_plot3d` (the
//!   values in bands) and `slice_vector_plot3d([vx, vy, vz], ...)` (arrows on the surface), and their `list_` forms
//!   for a 3D array of values or vectors (layers along z, rows along y; `data_range`). The surface is a name
//!   (`"CenterPlanes"`, the default; `"BackPlanes"`, `"XStackedPlanes"`, `"YStackedPlanes"`, `"ZStackedPlanes"`,
//!   `"DiagonalPlanes"`, `"CenterSphere"`), an equation in x, y, z, a region (`sphere`, `ball`, `cylinder`,
//!   `infinite_plane`, `half_plane`, `implicit_region`), or a list of them. Surfaces that are not planes along
//!   the axes are level sets, triangulated by marching tetrahedra on a 36-point grid;
//! - `list_density_plot3d` (a 3D array as translucent coloured points);
//! - `histogram3d` (counts of points in the cells of a grid as boxes), `smooth_histogram3d` (a two-dimensional kernel
//!   density estimate as a surface);
//! - `rectangle_chart3d`, `sector_chart3d`, `bubble_chart3d`, `array_plot3d`, `list_surface_plot3d`,
//!   `list_fit_plot3d` (points with their least-squares plane), `feature_space_plot3d`, `chromaticity_plot3d`
//!   (the sRGB gamut, or other RGB spaces, in XYZ, xyY, Lab or Luv).
use crate::chartdata::{self, Node};
use crate::charts3::{bandwidth, list, rule};
use crate::space::{finish3, iso_surface, sample, Grid3, PALETTE};
use crate::bad;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;
use mc_graphics::gradient::Gradient;
use mc_numfn::{NumFn, Options, Var};

type Body = for<'a, 'b, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &[T<'h>], Option<T<'h>>) -> Res<Reply<'h>>;

struct S(&'static str, Body);

impl Native for S {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (a, o) = crate::more::split(h, args);
        if a.is_empty() {
            return Ok(Reply::Decline);
        }
        cx.tick(1)?;
        // an undefined name as the data: unevaluated in effect (a failure naming the argument), as for any value
        // that is not data
        if let Some(&f) = a.first() {
            // also `name -> labels`
            let f = match crate::values::call_of(h, f) {
                Some((n, &[x, _])) if n == mc_api::names::RULE => x,
                _ => f,
            };
            if matches!(h.view(f), View::Sym(_)) {
                return Ok(crate::bad(h, self.0, 1));
            }
        }
        (self.1)(cx, &a, o)
    }
}

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let all: [(&'static str, Body); 17] = [
        ("slice_density_plot3d", |cx, a, o| slice(cx, a, o, Kind::Density, false)),
        ("slice_contour_plot3d", |cx, a, o| slice(cx, a, o, Kind::Contour, false)),
        ("slice_vector_plot3d", |cx, a, o| slice(cx, a, o, Kind::Vector, false)),
        ("list_slice_density_plot3d", |cx, a, o| slice(cx, a, o, Kind::Density, true)),
        ("list_slice_contour_plot3d", |cx, a, o| slice(cx, a, o, Kind::Contour, true)),
        ("list_slice_vector_plot3d", |cx, a, o| slice(cx, a, o, Kind::Vector, true)),
        ("list_density_plot3d", list_density_plot3d),
        ("histogram3d", histogram3d),
        ("smooth_histogram3d", smooth_histogram3d),
        ("rectangle_chart3d", rectangle_chart3d),
        ("sector_chart3d", sector_chart3d),
        ("bubble_chart3d", bubble_chart3d),
        ("array_plot3d", array_plot3d),
        ("list_surface_plot3d", list_surface_plot3d),
        ("list_fit_plot3d", list_fit_plot3d),
        ("feature_space_plot3d", feature_space_plot3d),
        ("chromaticity_plot3d", chromaticity_plot3d),
    ];
    for (name, f) in all {
        k.register(h.sym(name), Attributes::default(), Box::new(S(name, f)));
    }
}

fn color<'h>(h: &'h Heap, c: Rgba) -> T<'h> {
    mc_graphics_kernel::rgb_term(h, c)
}

fn pal(k: usize) -> Rgba {
    let (r, g, b) = PALETTE[k % PALETTE.len()];
    Rgba::rgb(r, g, b)
}

fn empty3<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: Option<T<'h>>, who: &str) -> Res<Reply<'h>> {
    let h = cx.heap();
    cx.emit(h.call(h.sym("message"), &[h.sym_term(h.sym(who)), h.string("no numeric data to chart")]))?;
    finish3(cx, vec![], [0.0; 3], [1.0; 3], o, who)
}

/// Polygons (each 3 or 4 corners) with a colour each, as one packed polygon term with vertex colours.
fn faces_term<'h>(h: &'h Heap, faces: &[(Vec<[f64; 3]>, Rgba)]) -> Vec<T<'h>> {
    let mut out = vec![h.call(h.sym("edge_form"), &[h.sym_term(h.sym("none"))])];
    for k in [3usize, 4] {
        let sel: Vec<&(Vec<[f64; 3]>, Rgba)> = faces.iter().filter(|f| f.0.len() == k).collect();
        if sel.is_empty() {
            continue;
        }
        let mut pts = Vec::with_capacity(sel.len() * k * 3);
        let mut cols = Vec::with_capacity(sel.len() * k * 3);
        for (f, c) in &sel {
            for p in f {
                pts.extend_from_slice(p);
                cols.extend_from_slice(&[c.r, c.g, c.b]);
            }
        }
        let vc = h.call(h.sym(mc_api::names::RULE), &[h.sym_term(h.sym("vertex_colors")), h.real_array(&[sel.len(), k, 3], &cols)]);
        out.push(h.call(h.sym("polygon"), &[h.real_array(&[sel.len(), k, 3], &pts), h.call(h.sym("options"), &[vc])]));
    }
    out
}

// ---- slice plots

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Density,
    Contour,
    Vector,
}

/// What gives the values: compiled functions of x, y, z, or a 3D array (trilinear between its points).
enum Source<'h> {
    Fns(Vec<Box<dyn NumFn<'h, f64> + 'h>>),
    Data { n: [usize; 3], comps: usize, v: Vec<f64>, lo: [f64; 3], hi: [f64; 3] },
}

impl<'h> Source<'h> {
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, p: [f64; 3]) -> Res<Option<Vec<f64>>> {
        match self {
            Source::Fns(fs) => {
                let mut out = Vec::with_capacity(fs.len());
                let mut r = [0.0];
                for f in fs.iter_mut() {
                    if !f.eval(cx, &p, &mut r)? || !r[0].is_finite() {
                        return Ok(None);
                    }
                    out.push(r[0]);
                }
                Ok(Some(out))
            }
            Source::Data { n, comps, v, lo, hi } => {
                let mut idx = [0usize; 3];
                let mut fr = [0.0; 3];
                for d in 0..3 {
                    let t = if hi[d] > lo[d] { (p[d] - lo[d]) / (hi[d] - lo[d]) * (n[d] - 1) as f64 } else { 0.0 };
                    if !(-1e-9..=(n[d] - 1) as f64 + 1e-9).contains(&t) {
                        return Ok(None);
                    }
                    let t = t.clamp(0.0, (n[d] - 1) as f64);
                    idx[d] = (t.floor() as usize).min(n[d].saturating_sub(2));
                    fr[d] = if n[d] > 1 { t - idx[d] as f64 } else { 0.0 };
                }
                let mut out = vec![0.0; *comps];
                for q in 0..8 {
                    let (a, b, c) = (q & 1, q >> 1 & 1, q >> 2 & 1);
                    let (i, j, k) = ((idx[0] + a).min(n[0] - 1), (idx[1] + b).min(n[1] - 1), (idx[2] + c).min(n[2] - 1));
                    let w = (if a == 1 { fr[0] } else { 1.0 - fr[0] }) * (if b == 1 { fr[1] } else { 1.0 - fr[1] }) * (if c == 1 { fr[2] } else { 1.0 - fr[2] });
                    let base = ((k * n[1] + j) * n[0] + i) * *comps;
                    for (m, o) in out.iter_mut().enumerate() {
                        *o += w * v[base + m];
                    }
                }
                Ok(out.iter().all(|x| x.is_finite()).then_some(out))
            }
        }
    }
}

/// A 3D array of numbers (or of `comps`-vectors), layers along z, rows along y.
fn array3<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<([usize; 3], usize, Vec<f64>)>> {
    let node = chartdata::read(cx, t)?;
    let nz = node.items().len();
    let ny = node.items().first().map_or(0, |l| l.items().len());
    let nx = node.items().first().and_then(|l| l.items().first()).map_or(0, |r| r.items().len());
    let first = node.items().first().and_then(|l| l.items().first()).and_then(|r| r.items().first());
    let comps = match first {
        Some(e) if e.num().is_some() => 1,
        Some(e) => e.items().len(),
        None => 0,
    };
    if nx < 2 || ny < 2 || nz < 2 || comps == 0 {
        return Ok(None);
    }
    let mut v = Vec::with_capacity(nx * ny * nz * comps);
    for l in node.items() {
        if l.items().len() != ny {
            return Ok(None);
        }
        for r in l.items() {
            if r.items().len() != nx {
                return Ok(None);
            }
            for e in r.items() {
                if comps == 1 {
                    v.push(e.num().unwrap_or(f64::NAN));
                } else {
                    let t = e.tuple(comps);
                    match t {
                        Some(t) => v.extend(t),
                        None => v.extend(std::iter::repeat_n(f64::NAN, comps)),
                    }
                }
            }
        }
    }
    Ok(Some(([nx, ny, nz], comps, v)))
}

/// The box of a `data_range` option, else 1 .. n on each axis.
fn data_box<'h>(cx: &mut NativeCx<'_, '_, 'h>, o: Option<T<'h>>, n: [usize; 3]) -> Res<([f64; 3], [f64; 3])> {
    let (mut lo, mut hi) = ([1.0; 3], [n[0] as f64, n[1] as f64, n[2] as f64]);
    if let Some(v) = crate::charts3::option(cx, o, "data_range") {
        let node = chartdata::read(cx, v)?;
        for d in 0..3 {
            if let Some(t) = node.items().get(d).and_then(|e| e.tuple(2)) {
                lo[d] = t[0];
                hi[d] = t[1];
            }
        }
    }
    Ok((lo, hi))
}

/// Axis-aligned plane d = c as a grid of quads.
fn axis_plane(d: usize, c: f64, lo: [f64; 3], hi: [f64; 3], n: usize) -> Vec<Vec<[f64; 3]>> {
    let (a, b) = ((d + 1) % 3, (d + 2) % 3);
    let mut out = vec![];
    for i in 0..n {
        for j in 0..n {
            let pt = |u: usize, v: usize| {
                let mut p = [0.0; 3];
                p[d] = c;
                p[a] = lo[a] + (hi[a] - lo[a]) * u as f64 / n as f64;
                p[b] = lo[b] + (hi[b] - lo[b]) * v as f64 / n as f64;
                p
            };
            out.push(vec![pt(i, j), pt(i + 1, j), pt(i + 1, j + 1), pt(i, j + 1)]);
        }
    }
    out
}

/// A level function (a term in x, y, z, zero on the surface) for a surface given as an equation or a region.
fn level<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>, vars: &[Sym; 3]) -> Option<T<'h>> {
    let h = cx.heap();
    let e = mc_geom::ex::E { h };
    let x: Vec<T<'h>> = vars.iter().map(|&v| h.sym_term(v)).collect();
    let (name, args) = crate::values::call_of(h, s)?;
    let num = |t: T<'h>| -> Option<Vec<T<'h>>> { cx.as_list(t).map(|l| l.to_vec()).filter(|l| l.len() == 3) };
    let dot = |a: &[T<'h>], b: &[T<'h>]| -> T<'h> { e.f("add", &(0..3).map(|k| mul2(&e, a[k], b[k])).collect::<Vec<_>>()) };
    let minus = |a: &[T<'h>], b: &[T<'h>]| -> Vec<T<'h>> { (0..3).map(|k| e.sub(a[k], b[k])).collect() };
    let cross = |a: &[T<'h>], b: &[T<'h>]| -> Vec<T<'h>> {
        vec![e.sub(mul2(&e, a[1], b[2]), mul2(&e, a[2], b[1])), e.sub(mul2(&e, a[2], b[0]), mul2(&e, a[0], b[2])), e.sub(mul2(&e, a[0], b[1]), mul2(&e, a[1], b[0]))]
    };
    match (name.as_str(), args) {
        ("equal", &[a, b]) => Some(e.sub(a, b)),
        ("sphere" | "ball", _) => {
            let c = args.first().and_then(|&c| num(c)).unwrap_or_else(|| vec![h.int(0), h.int(0), h.int(0)]);
            let r = args.get(1).copied().unwrap_or(h.int(1));
            let d = minus(&x, &c);
            Some(e.sub(dot(&d, &d), mul2(&e, r, r)))
        }
        ("cylinder", _) => {
            let (p1, p2) = match args.first().and_then(|&p| cx.as_list(p)) {
                Some(&[a, b]) => (num(a)?, num(b)?),
                _ => (vec![h.int(0), h.int(0), h.int(-1)], vec![h.int(0), h.int(0), h.int(1)]),
            };
            let r = args.get(1).copied().unwrap_or(h.int(1));
            let axis = minus(&p2, &p1);
            let c = cross(&minus(&x, &p1), &axis);
            Some(e.sub(dot(&c, &c), mul2(&e, mul2(&e, r, r), dot(&axis, &axis))))
        }
        ("infinite_plane", &[p]) if cx.as_list(p).is_some_and(|l| l.len() == 3) => {
            let ps = cx.as_list(p)?.to_vec();
            let (a, b, c) = (num(ps[0])?, num(ps[1])?, num(ps[2])?);
            let n = cross(&minus(&b, &a), &minus(&c, &a));
            Some(dot(&n, &minus(&x, &a)))
        }
        ("infinite_plane", &[p, vs]) => {
            let a = num(p)?;
            let vs = cx.as_list(vs)?.to_vec();
            let n = cross(&num(*vs.first()?)?, &num(*vs.get(1)?)?);
            Some(dot(&n, &minus(&x, &a)))
        }
        ("half_plane", &[ps, v]) => {
            let ps = cx.as_list(ps)?.to_vec();
            let (a, b) = (num(*ps.first()?)?, num(*ps.get(1)?)?);
            let n = cross(&minus(&b, &a), &num(v)?);
            Some(dot(&n, &minus(&x, &a)))
        }
        ("implicit_region", &[cond, vs]) => {
            let own: Vec<T<'h>> = cx.as_list(vs)?.to_vec();
            if own.len() != 3 {
                return None;
            }
            let g = match crate::values::call_of(h, cond) {
                Some((n, &[a, b])) if n == "equal" => e.sub(a, b),
                _ => mc_geom::margin(h, cond)?,
            };
            let rules: Vec<T<'h>> = own.iter().zip(&x).map(|(&a, &b)| h.call(h.sym(mc_api::names::RULE), &[a, b])).collect();
            Some(h.call(h.sym("rewrite"), &[g, list(h, &rules)]))
        }
        _ => None,
    }
}

/// The faces of the slice surfaces.
fn surfaces<'h>(cx: &mut NativeCx<'_, '_, 'h>, spec: Option<T<'h>>, vars: Option<&[Sym; 3]>, lo: [f64; 3], hi: [f64; 3]) -> Res<Vec<Vec<[f64; 3]>>> {
    let h = cx.heap();
    let mid = |d: usize| 0.5 * (lo[d] + hi[d]);
    let mut out = vec![];
    let specs: Vec<T<'h>> = match spec {
        None => vec![],
        Some(s) => match cx.as_list(s) {
            Some(l) if !l.is_empty() && !l.iter().all(|&e| real_like(h, e)) => l.to_vec(),
            _ => vec![s],
        },
    };
    let n = 28;
    let stacked = |d: usize, out: &mut Vec<Vec<[f64; 3]>>| {
        for k in 0..5 {
            out.extend(axis_plane(d, lo[d] + (hi[d] - lo[d]) * (k as f64 + 0.5) / 5.0, lo, hi, 14));
        }
    };
    if specs.is_empty() {
        for d in 0..3 {
            out.extend(axis_plane(d, mid(d), lo, hi, n));
        }
        return Ok(out);
    }
    for s in specs {
        let name = match h.view(s) {
            View::Str(t) => Some(t.to_string()),
            _ => None,
        };
        match name.as_deref() {
            Some("CenterPlanes") | Some("CenterCutBox") => (0..3).for_each(|d| out.extend(axis_plane(d, mid(d), lo, hi, n))),
            Some("BackPlanes") => {
                out.extend(axis_plane(0, lo[0], lo, hi, n));
                out.extend(axis_plane(1, hi[1], lo, hi, n));
                out.extend(axis_plane(2, lo[2], lo, hi, n));
            }
            Some("XStackedPlanes") => stacked(0, &mut out),
            Some("YStackedPlanes") => stacked(1, &mut out),
            Some("ZStackedPlanes") => stacked(2, &mut out),
            Some(_) | None => {
                let g = match (vars, name.as_deref()) {
                    (Some(v), None) => level(cx, s, v),
                    _ => None,
                };
                let g = match (g, name.as_deref(), vars) {
                    (Some(g), _, _) => Some(g),
                    (None, Some("DiagonalPlanes"), Some(v)) => {
                        let (x, y) = (h.sym_term(v[0]), h.sym_term(v[1]));
                        let e = mc_geom::ex::E { h };
                        Some(e.sub(mul2(&e, e.sub(x, h.real(mid(0))), e.sub(y, h.real(mid(1)))), h.int(0)))
                    }
                    (None, Some(nm), Some(v)) if nm.contains("Sphere") => {
                        let e = mc_geom::ex::E { h };
                        let r = (0..3).map(|d| hi[d] - lo[d]).fold(f64::INFINITY, f64::min) * 0.4;
                        let sq: Vec<T<'h>> = (0..3).map(|d| {
                            let t = e.sub(h.sym_term(v[d]), h.real(mid(d)));
                            mul2(&e, t, t)
                        }).collect();
                        Some(e.sub(e.f("add", &sq), h.real(r * r)))
                    }
                    _ => None,
                };
                match (g, vars) {
                    (Some(g), Some(v)) => {
                        let grid = sample(cx, g, v, lo, hi, 36)?;
                        let tris = iso_surface(&grid, 0.0);
                        for t in tris.chunks(9) {
                            out.push(vec![[t[0], t[1], t[2]], [t[3], t[4], t[5]], [t[6], t[7], t[8]]]);
                        }
                    }
                    _ => (0..3).for_each(|d| out.extend(axis_plane(d, mid(d), lo, hi, n))),
                }
            }
        }
    }
    Ok(out)
}

fn mul2<'h>(e: &mc_geom::ex::E<'h>, a: T<'h>, b: T<'h>) -> T<'h> {
    e.mul(&[a, b])
}

fn real_like(h: &Heap, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::Real(_))
}

fn slice<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>, kind: Kind, data: bool) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = match (kind, data) {
        (Kind::Density, false) => "slice_density_plot3d",
        (Kind::Contour, false) => "slice_contour_plot3d",
        (Kind::Vector, false) => "slice_vector_plot3d",
        (Kind::Density, true) => "list_slice_density_plot3d",
        (Kind::Contour, true) => "list_slice_contour_plot3d",
        (Kind::Vector, true) => "list_slice_vector_plot3d",
    };
    let (mut src, spec, lo, hi, vars, domain) = if data {
        let Some((n, comps, v)) = array3(cx, a[0])? else { return empty3(cx, o, who) };
        if (kind == Kind::Vector) != (comps == 3) {
            return Ok(bad(h, who, 1));
        }
        let (lo, hi) = data_box(cx, o, n)?;
        // a region's own variables name the coordinates
        let vars = a.get(1).and_then(|&s| match crate::values::call_of(h, s) {
            Some((n, &[_, vs])) if n == "implicit_region" => cx.as_list(vs).and_then(|l| {
                let s: Vec<Sym> = l.iter().filter_map(|&v| if let View::Sym(s) = h.view(v) { Some(s) } else { None }).collect();
                (s.len() == 3).then(|| [s[0], s[1], s[2]])
            }),
            _ => None,
        });
        let vars = vars.unwrap_or([h.sym("x"), h.sym("y"), h.sym("z")]);
        (Source::Data { n, comps, v, lo, hi }, a.get(1).copied(), lo, hi, vars, None)
    } else {
        let [f, rest @ ..] = a else { return Ok(Reply::Decline) };
        // f, [surface], [x, ...], [y, ...], [z, ...]
        let mut domain = None;
        let built: Vec<T<'h>>;
        let (spec, specs) = match rest.len() {
            3 => (None, rest),
            4 => (Some(rest[0]), &rest[1..]),
            1 | 2 => {
                // element([x, y, z], region): the region's bounds as the box, faces outside it left out
                let el = rest[rest.len() - 1];
                let Some((n, &[vs, reg])) = crate::values::call_of(h, el) else { return Ok(Reply::Decline) };
                if n != "element" {
                    return Ok(Reply::Decline);
                }
                let Some(vs) = cx.as_list(vs).map(|l| l.to_vec()).filter(|l| l.len() == 3) else { return Ok(bad(h, who, 2)) };
                let b = cx.eval(h.call(h.sym("region_bounds"), &[reg]))?;
                let b = chartdata::read(cx, b)?;
                let mut sp = vec![];
                for (d, v) in vs.iter().enumerate() {
                    // a region whose bounds are not known: the unit box of the standard primitives
                    let t = b.items().get(d).and_then(|e| e.tuple(2)).unwrap_or_else(|| vec![-1.0, 1.0]);
                    sp.push(list(h, &[*v, h.real(t[0]), h.real(t[1])]));
                }
                built = sp;
                domain = Some(reg);
                (if rest.len() == 2 { Some(rest[0]) } else { None }, &built[..])
            }
            _ => return Ok(Reply::Decline),
        };
        let (vars, lo, hi) = match crate::space::ranges(cx, specs, who)? {
            Ok(r) => r,
            Err(e) => return Ok(e),
        };
        let comps: Vec<T<'h>> = match kind {
            Kind::Vector => match cx.as_list(*f) {
                Some(l) if l.len() == 3 => l.to_vec(),
                _ => return Ok(bad(h, who, 1)),
            },
            _ => vec![*f],
        };
        let vs: Vec<Var> = vars.iter().map(|&s| Var::scalar(s)).collect();
        let mut fs = vec![];
        for c in comps {
            fs.push(mc_numfn::compile_expr(cx, c, &vs, &Options::default())?);
        }
        (Source::Fns(fs), spec, lo, hi, vars, domain)
    };
    let mut faces = surfaces(cx, spec, Some(&vars), lo, hi)?;
    if let Some(reg) = domain {
        let mut kept = Vec::with_capacity(faces.len());
        for (k, f) in faces.into_iter().enumerate() {
            if k % 512 == 0 {
                cx.tick(1)?;
            }
            let c: [f64; 3] = std::array::from_fn(|d| f.iter().map(|p| p[d]).sum::<f64>() / f.len() as f64);
            let inside = cx.eval(h.call(h.sym("region_member"), &[reg, h.reals(&c)]))?;
            if !matches!(h.view(inside), View::Sym(s) if h.sym_name(s) == "false") {
                kept.push(f);
            }
        }
        faces = kept;
    }
    let grad = Gradient::default_gradient();
    let mut prims = vec![];
    match kind {
        Kind::Density | Kind::Contour => {
            let mut vals = Vec::with_capacity(faces.len());
            for (k, f) in faces.iter().enumerate() {
                if k % 2048 == 0 {
                    cx.tick(1)?;
                }
                let c: [f64; 3] = std::array::from_fn(|d| f.iter().map(|p| p[d]).sum::<f64>() / f.len() as f64);
                vals.push(src.at(cx, c)?.map(|v| v[0]));
            }
            let (a0, b0) = vals.iter().flatten().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            if !a0.is_finite() {
                return empty3(cx, o, who);
            }
            let bands = 10.0;
            let colored: Vec<(Vec<[f64; 3]>, Rgba)> = faces
                .into_iter()
                .zip(vals)
                .filter_map(|(f, v)| {
                    let v = v?;
                    let mut t = if b0 > a0 { (v - a0) / (b0 - a0) } else { 0.5 };
                    if kind == Kind::Contour {
                        t = ((t * bands).floor().min(bands - 1.0) + 0.5) / bands;
                    }
                    Some((f, grad.at(t)))
                })
                .collect();
            prims.extend(faces_term(h, &colored));
        }
        Kind::Vector => {
            // arrows at the centres of a thinned set of faces, scaled to the largest
            let step = (faces.len() / 260).max(1);
            let mut arrows = vec![];
            for f in faces.iter().step_by(step) {
                let c: [f64; 3] = std::array::from_fn(|d| f.iter().map(|p| p[d]).sum::<f64>() / f.len() as f64);
                if let Some(v) = src.at(cx, c)? {
                    arrows.push((c, [v[0], v[1], v[2]]));
                }
            }
            let top = arrows.iter().map(|(_, v)| (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()).fold(1e-300f64, f64::max);
            let len = (0..3).map(|d| hi[d] - lo[d]).fold(f64::INFINITY, f64::min) / 9.0;
            // the surfaces, faint
            let faint: Vec<(Vec<[f64; 3]>, Rgba)> = faces.into_iter().map(|f| (f, Rgba::rgb(0.85, 0.87, 0.9))).collect();
            let mut part = vec![h.call(h.sym("opacity"), &[h.real(0.35)])];
            part.extend(faces_term(h, &faint));
            prims.push(list(h, &part));
            for (p, v) in arrows {
                let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                if m < 1e-300 {
                    continue;
                }
                let s = len * (0.35 + 0.65 * m / top) / m;
                let q = [p[0] + v[0] * s, p[1] + v[1] * s, p[2] + v[2] * s];
                prims.push(list(h, &[color(h, grad.at(m / top)), h.call(h.sym("arrow"), &[list(h, &[h.reals(&p), h.reals(&q)])])]));
            }
        }
    }
    finish3(cx, prims, lo, hi, o, who)
}

// ---- data in space

fn list_density_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "list_density_plot3d";
    let Some((n, comps, v)) = array3(cx, a[0])? else { return empty3(cx, o, who) };
    if comps != 1 {
        return Ok(bad(h, who, 1));
    }
    let (lo, hi) = data_box(cx, o, n)?;
    let g = Grid3 { lo, hi, n, v };
    let Some((a0, b0)) = g.extent() else { return empty3(cx, o, who) };
    let grad = Gradient::default_gradient();
    let s = n.iter().copied().max().unwrap_or(1).div_ceil(24).max(1);
    let mut prims = vec![h.call(h.sym("absolute_point_size"), &[h.real(6.0)])];
    for k in (0..n[2]).step_by(s) {
        for j in (0..n[1]).step_by(s) {
            for i in (0..n[0]).step_by(s) {
                let x = g.at(i, j, k);
                if !x.is_finite() {
                    continue;
                }
                let t = if b0 > a0 { (x - a0) / (b0 - a0) } else { 0.5 };
                prims.push(list(h, &[color(h, Rgba { a: 0.1 + 0.6 * t, ..grad.at(t) }), h.call(h.sym("point"), &[h.reals(&g.point(i, j, k))])]));
            }
        }
    }
    finish3(cx, prims, lo, hi, o, who)
}

/// Points of a list of pairs.
fn pairs(n: &Node<'_>) -> Vec<(f64, f64)> {
    n.items().iter().filter_map(|e| e.tuple(2)).map(|t| (t[0], t[1])).collect()
}

fn span(v: impl Iterator<Item = f64>) -> (f64, f64) {
    let (a, b) = v.filter(|x| x.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), x| (a.min(x), b.max(x)));
    if !a.is_finite() {
        (0.0, 1.0)
    } else if b > a {
        (a, b)
    } else {
        (a - 0.5, a + 0.5)
    }
}

fn histogram3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "histogram3d";
    let node = chartdata::read(cx, a[0])?;
    // one data set of pairs or several
    let sets: Vec<Vec<(f64, f64)>> = {
        let p = pairs(&node);
        if !p.is_empty() { vec![p] } else { node.items().iter().map(pairs).filter(|s| !s.is_empty()).collect() }
    };
    if sets.is_empty() {
        return empty3(cx, o, who);
    }
    let all: Vec<(f64, f64)> = sets.iter().flatten().copied().collect();
    let (rx, ry) = (span(all.iter().map(|p| p.0)), span(all.iter().map(|p| p.1)));
    let nb = match a.get(1).and_then(|&b| real_number(h, b)) {
        Some(k) if k >= 1.0 => (k as usize).min(60),
        _ => ((all.len() as f64).sqrt().ceil() as usize).clamp(4, 20),
    };
    let kind = a.get(2).and_then(|&k| if let View::Str(s) = h.view(k) { Some(s.to_uppercase()) } else { None }).unwrap_or_default();
    let mut prims = vec![h.call(h.sym("edge_form"), &[h.call(h.sym("gray_level"), &[h.real(0.35)])])];
    let mut top = 0.0f64;
    let (wx, wy) = ((rx.1 - rx.0) / nb as f64, (ry.1 - ry.0) / nb as f64);
    for (g, s) in sets.iter().enumerate() {
        let mut c = vec![0.0; nb * nb];
        for &(x, y) in s {
            let i = (((x - rx.0) / wx) as usize).min(nb - 1);
            let j = (((y - ry.0) / wy) as usize).min(nb - 1);
            c[j * nb + i] += 1.0;
        }
        let n = s.len() as f64;
        let mut part = vec![color(h, pal(g))];
        if sets.len() > 1 {
            part.push(h.call(h.sym("opacity"), &[h.real(0.7)]));
        }
        for j in 0..nb {
            for i in 0..nb {
                let v = match kind.as_str() {
                    "PROBABILITY" => c[j * nb + i] / n,
                    "PDF" => c[j * nb + i] / n / (wx * wy),
                    _ => c[j * nb + i],
                };
                if v <= 0.0 {
                    continue;
                }
                top = top.max(v);
                let (x0, y0) = (rx.0 + wx * i as f64, ry.0 + wy * j as f64);
                part.push(h.call(h.sym("cuboid"), &[h.reals(&[x0, y0, 0.0]), h.reals(&[x0 + wx, y0 + wy, v])]));
            }
        }
        prims.push(list(h, &part));
    }
    finish3(cx, prims, [rx.0, ry.0, 0.0], [rx.1, ry.1, top.max(1e-300) * 1.05], o, who)
}

fn real_number(h: &Heap, t: T<'_>) -> Option<f64> {
    mc_graphics::num::number(h, t)
}

/// A surface over a grid of heights `z[j][i]` coloured by height.
fn height_surface<'h>(h: &'h Heap, z: &[Vec<f64>], rx: (f64, f64), ry: (f64, f64), grad: &Gradient) -> Vec<T<'h>> {
    let (ny, nx) = (z.len(), z[0].len());
    let (lo, hi) = span(z.iter().flatten().copied());
    let mut faces = vec![];
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let p = |a: usize, b: usize| [rx.0 + (rx.1 - rx.0) * a as f64 / (nx - 1) as f64, ry.0 + (ry.1 - ry.0) * b as f64 / (ny - 1) as f64, z[b][a]];
            let f = vec![p(i, j), p(i + 1, j), p(i + 1, j + 1), p(i, j + 1)];
            if f.iter().any(|q| !q[2].is_finite()) {
                continue;
            }
            let m = f.iter().map(|q| q[2]).sum::<f64>() / 4.0;
            faces.push((f, grad.at((m - lo) / (hi - lo))));
        }
    }
    faces_term(h, &faces)
}

fn smooth_histogram3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "smooth_histogram3d";
    let node = chartdata::read(cx, a[0])?;
    let pts = pairs(&node);
    if pts.len() < 2 {
        return empty3(cx, o, who);
    }
    let mut xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    xs.sort_by(f64::total_cmp);
    ys.sort_by(f64::total_cmp);
    let (bx, by) = (bandwidth(&xs), bandwidth(&ys));
    let rx = (xs[0] - 2.0 * bx, xs[xs.len() - 1] + 2.0 * bx);
    let ry = (ys[0] - 2.0 * by, ys[ys.len() - 1] + 2.0 * by);
    let n = 40;
    let norm = pts.len() as f64 * bx * by * std::f64::consts::TAU;
    let mut z = vec![vec![0.0; n]; n];
    for j in 0..n {
        cx.tick(1)?;
        for i in 0..n {
            let (x, y) = (rx.0 + (rx.1 - rx.0) * i as f64 / (n - 1) as f64, ry.0 + (ry.1 - ry.0) * j as f64 / (n - 1) as f64);
            z[j][i] = pts.iter().map(|&(px, py)| (-0.5 * (((x - px) / bx).powi(2) + ((y - py) / by).powi(2))).exp()).sum::<f64>() / norm;
        }
    }
    let top = z.iter().flatten().copied().fold(1e-300f64, f64::max);
    let prims = height_surface(h, &z, rx, ry, &Gradient::default_gradient());
    finish3(cx, prims, [rx.0, ry.0, 0.0], [rx.1, ry.1, top * 1.05], o, who)
}

// ---- 3D charts

fn rectangle_chart3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "rectangle_chart3d";
    let node = chartdata::read(cx, a[0])?;
    let groups = node.tuple_groups(3);
    if groups.is_empty() {
        return empty3(cx, o, who);
    }
    let mut prims = vec![h.call(h.sym("edge_form"), &[h.call(h.sym("gray_level"), &[h.real(0.35)])])];
    let (mut x, mut zmax, mut dmax) = (0.0f64, 0.0f64, 0.0f64);
    for (g, (gn, items)) in groups.iter().enumerate() {
        for (k, (n, t)) in items.iter().enumerate() {
            let (w, d, ht) = (t[0].abs(), t[1].abs(), t[2]);
            let mut part = vec![color(h, pal(if groups.len() > 1 { g } else { k }))];
            if let Some(gn) = gn {
                part.extend_from_slice(&gn.style);
            }
            part.extend_from_slice(&n.style);
            part.push(h.call(h.sym("cuboid"), &[h.reals(&[x, 0.0, 0.0f64.min(ht)]), h.reals(&[x + w, d, 0.0f64.max(ht)])]));
            prims.push(list(h, &part));
            x += w * 1.05;
            zmax = zmax.max(ht.abs());
            dmax = dmax.max(d);
        }
        x += 0.3;
    }
    finish3(cx, prims, [0.0, 0.0, 0.0], [x.max(1e-9), dmax.max(1e-9), zmax.max(1e-9)], o, who)
}

/// The faces of a sector of a ring extruded from z0 to z1 (top, bottom, sides).
fn sector_prism(c: [f64; 2], r: f64, a: f64, b: f64, z0: f64, z1: f64) -> Vec<Vec<[f64; 3]>> {
    let n = ((b - a).abs() / 0.12).ceil().max(2.0) as usize;
    let arc: Vec<[f64; 2]> = (0..=n).map(|k| {
        let t = a + (b - a) * k as f64 / n as f64;
        [c[0] + r * t.cos(), c[1] + r * t.sin()]
    }).collect();
    let mut faces = vec![];
    for w in arc.windows(2) {
        faces.push(vec![[c[0], c[1], z1], [w[0][0], w[0][1], z1], [w[1][0], w[1][1], z1]]);
        faces.push(vec![[c[0], c[1], z0], [w[1][0], w[1][1], z0], [w[0][0], w[0][1], z0]]);
        faces.push(vec![[w[0][0], w[0][1], z0], [w[1][0], w[1][1], z0], [w[1][0], w[1][1], z1], [w[0][0], w[0][1], z1]]);
    }
    for e in [arc[0], arc[n]] {
        faces.push(vec![[c[0], c[1], z0], [e[0], e[1], z0], [e[0], e[1], z1], [c[0], c[1], z1]]);
    }
    faces
}

fn sector_chart3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "sector_chart3d";
    let node = chartdata::read(cx, a[0])?;
    let mut groups = node.tuple_groups(3);
    if groups.is_empty() {
        groups = node.tuple_groups(2);
    }
    if groups.is_empty() {
        return empty3(cx, o, who);
    }
    let rmax = groups.iter().flat_map(|g| g.1.iter().map(|t| t.1[1].abs())).fold(1e-300f64, f64::max);
    let hmax = groups.iter().flat_map(|g| g.1.iter().map(|t| t.1.get(2).copied().unwrap_or(1.0).abs())).fold(1e-300f64, f64::max);
    let mut faces = vec![];
    for (gi, (_, items)) in groups.iter().enumerate() {
        let total: f64 = items.iter().map(|t| t.1[0].abs()).sum::<f64>().max(1e-300);
        let c = [2.3 * gi as f64, 0.0];
        let mut ang = std::f64::consts::FRAC_PI_2;
        for (k, (_, t)) in items.iter().enumerate() {
            let span = std::f64::consts::TAU * t[0].abs() / total;
            let ht = t.get(2).copied().unwrap_or(1.0).abs() / hmax * 0.6;
            for f in sector_prism(c, t[1].abs() / rmax, ang - span, ang, 0.0, ht) {
                faces.push((f, pal(k)));
            }
            ang -= span;
        }
    }
    let prims = faces_term(h, &faces);
    let gx = 2.3 * (groups.len() - 1) as f64;
    finish3(cx, prims, [-1.05, -1.05, 0.0], [gx + 1.05, 1.05, 0.65], o.map(|o| no_axes(h, o)).or_else(|| Some(h.call(h.sym("options"), &[rule(h, "axes", h.sym_term(h.sym("false")))]))), who)
}

/// The options with `axes: false` first (later rules win).
fn no_axes<'h>(h: &'h Heap, o: T<'h>) -> T<'h> {
    let mut rs = vec![rule(h, "axes", h.sym_term(h.sym("false")))];
    if let View::App { args, .. } = h.view(o) {
        rs.extend(args.iter().copied());
    }
    h.call(h.sym("options"), &rs)
}

fn bubble_chart3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "bubble_chart3d";
    let node = chartdata::read(cx, a[0])?;
    let groups = node.tuple_groups(4);
    if groups.is_empty() {
        return empty3(cx, o, who);
    }
    let all: Vec<&Vec<f64>> = groups.iter().flat_map(|g| g.1.iter().map(|t| &t.1)).collect();
    let r: Vec<(f64, f64)> = (0..3).map(|d| span(all.iter().map(|t| t[d]))).collect();
    let smax = all.iter().map(|t| t[3].abs()).fold(1e-300f64, f64::max);
    let ext = r.iter().map(|q| q.1 - q.0).fold(f64::INFINITY, f64::min);
    let mut prims = vec![];
    for (g, (_, items)) in groups.iter().enumerate() {
        let mut part = vec![color(h, pal(g))];
        for (n, t) in items {
            let rad = 0.12 * ext * (t[3].abs() / smax).cbrt().max(0.15);
            let mut one = n.style.clone();
            one.push(h.call(h.sym("sphere"), &[h.reals(&[t[0], t[1], t[2]]), h.real(rad)]));
            part.push(list(h, &one));
        }
        prims.push(list(h, &part));
    }
    let pad = 0.15 * ext;
    finish3(cx, prims, [r[0].0 - pad, r[1].0 - pad, r[2].0 - pad], [r[0].1 + pad, r[1].1 + pad, r[2].1 + pad], o, who)
}

fn array_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "array_plot3d";
    let Some((n, comps, v)) = array3(cx, a[0])? else { return empty3(cx, o, who) };
    if comps != 1 || n[0] * n[1] * n[2] > 64_000 {
        return Ok(bad(h, who, 1));
    }
    let (lo, hi) = v.iter().filter(|x| x.is_finite()).fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)));
    let mut prims = vec![h.call(h.sym("edge_form"), &[h.call(h.sym("gray_level"), &[h.real(0.5)])])];
    for k in 0..n[2] {
        for j in 0..n[1] {
            for i in 0..n[0] {
                let x = v[(k * n[1] + j) * n[0] + i];
                if !x.is_finite() || x == 0.0 {
                    continue;
                }
                // 0 empty, 1 black (as array_plot), values between as grays
                let t = if hi > lo && (lo < 0.0 || hi > 1.0) { (x - lo) / (hi - lo) } else { x.clamp(0.0, 1.0) };
                let g = 1.0 - 0.85 * t;
                prims.push(list(h, &[h.call(h.sym("gray_level"), &[h.real(g)]), h.call(h.sym("cuboid"), &[h.reals(&[i as f64, j as f64, k as f64]), h.reals(&[i as f64 + 1.0, j as f64 + 1.0, k as f64 + 1.0])])]));
            }
        }
    }
    finish3(cx, prims, [0.0; 3], [n[0] as f64, n[1] as f64, n[2] as f64], o.map(|o| no_axes(h, o)).or_else(|| Some(h.call(h.sym("options"), &[rule(h, "axes", h.sym_term(h.sym("false")))]))), who)
}

fn list_surface_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "list_surface_plot3d";
    let node = chartdata::read(cx, a[0])?;
    // a grid of points (rows of [x, y, z]), else scattered points
    let grid: Option<Vec<Vec<[f64; 3]>>> = node.items().iter().map(|r| r.items().iter().map(|e| e.tuple(3).map(|t| [t[0], t[1], t[2]])).collect::<Option<Vec<_>>>()).collect();
    let mut prims = vec![];
    let mut all: Vec<[f64; 3]> = vec![];
    match grid.filter(|g| g.len() > 1 && g.iter().all(|r| r.len() == g[0].len() && r.len() > 1)) {
        Some(g) => {
            let mut faces = vec![];
            for j in 0..g.len() - 1 {
                for i in 0..g[0].len() - 1 {
                    faces.push((vec![g[j][i], g[j][i + 1], g[j + 1][i + 1], g[j + 1][i]], pal(0)));
                }
            }
            all = g.into_iter().flatten().collect();
            prims.extend(faces_term(h, &faces));
        }
        None => {
            let pts: Vec<[f64; 3]> = node.items().iter().filter_map(|e| e.tuple(3)).map(|t| [t[0], t[1], t[2]]).collect();
            if pts.is_empty() {
                return empty3(cx, o, who);
            }
            // triangles of the points' xy Delaunay-free fan: a grid of heights by nearest point
            let flat: Vec<f64> = pts.iter().flatten().copied().collect();
            prims.push(list(h, &[color(h, pal(0)), h.call(h.sym("absolute_point_size"), &[h.real(4.0)]), h.call(h.sym("point"), &[h.real_array(&[pts.len(), 3], &flat)])]));
            all = pts;
        }
    }
    let r: Vec<(f64, f64)> = (0..3).map(|d| span(all.iter().map(|p| p[d]))).collect();
    finish3(cx, prims, [r[0].0, r[1].0, r[2].0], [r[0].1, r[1].1, r[2].1], o, who)
}

fn list_fit_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "list_fit_plot3d";
    let data = match crate::values::call_of(h, a[0]) {
        Some((n, &[x, _])) if n == mc_api::names::RULE => x,
        _ => a[0],
    };
    let node = chartdata::read(cx, data)?;
    let pts: Vec<[f64; 3]> = node.items().iter().filter_map(|e| e.tuple(3)).map(|t| [t[0], t[1], t[2]]).collect();
    if pts.len() < 3 {
        return empty3(cx, o, who);
    }
    // least squares z = a + b x + c y (normal equations)
    let mut m = [[0.0; 4]; 3];
    for p in &pts {
        let r = [1.0, p[0], p[1]];
        for i in 0..3 {
            for j in 0..3 {
                m[i][j] += r[i] * r[j];
            }
            m[i][3] += r[i] * p[2];
        }
    }
    let coef = solve3(m);
    let r: Vec<(f64, f64)> = (0..3).map(|d| span(pts.iter().map(|p| p[d]))).collect();
    let flat: Vec<f64> = pts.iter().flatten().copied().collect();
    let mut prims = vec![list(h, &[color(h, Rgba::rgb(0.106, 0.369, 0.388)), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[h.real_array(&[pts.len(), 3], &flat)])])];
    let (mut zlo, mut zhi) = r[2];
    if let Some([c0, c1, c2]) = coef {
        let z = |x: f64, y: f64| c0 + c1 * x + c2 * y;
        let corners = [[r[0].0, r[1].0], [r[0].1, r[1].0], [r[0].1, r[1].1], [r[0].0, r[1].1]];
        let face: Vec<[f64; 3]> = corners.iter().map(|c| [c[0], c[1], z(c[0], c[1])]).collect();
        for p in &face {
            zlo = zlo.min(p[2]);
            zhi = zhi.max(p[2]);
        }
        let mut part = vec![h.call(h.sym("opacity"), &[h.real(0.6)])];
        part.extend(faces_term(h, &[(face, pal(1))]));
        prims.push(list(h, &part));
    }
    finish3(cx, prims, [r[0].0, r[1].0, zlo], [r[0].1, r[1].1, if zhi > zlo { zhi } else { zlo + 1.0 }], o, who)
}

fn solve3(mut m: [[f64; 4]; 3]) -> Option<[f64; 3]> {
    for c in 0..3 {
        let p = (c..3).max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))?;
        m.swap(c, p);
        if m[c][c].abs() < 1e-300 {
            return None;
        }
        for r in 0..3 {
            if r != c {
                let f = m[r][c] / m[c][c];
                for k in c..4 {
                    m[r][k] -= f * m[c][k];
                }
            }
        }
    }
    Some([m[0][3] / m[0][0], m[1][3] / m[1][1], m[2][3] / m[2][2]])
}

fn feature_space_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "feature_space_plot3d";
    let groups = crate::panels::projected(cx, a[0], 3)?;
    if groups.is_empty() {
        return empty3(cx, o, who);
    }
    let mut prims = vec![];
    for (g, (_, pts)) in groups.iter().enumerate() {
        let flat: Vec<f64> = pts.iter().flat_map(|p| p.iter().copied().take(3)).collect();
        let curves = [Rgba::rgb(0.106, 0.369, 0.388), Rgba::rgb(0.757, 0.314, 0.180), Rgba::rgb(0.788, 0.604, 0.180), Rgba::rgb(0.357, 0.173, 0.31)];
        prims.push(list(h, &[color(h, curves[g % 4]), h.call(h.sym("absolute_point_size"), &[h.real(5.0)]), h.call(h.sym("point"), &[h.real_array(&[pts.len(), 3], &flat)])]));
    }
    let all: Vec<&Vec<f64>> = groups.iter().flat_map(|g| g.1.iter()).collect();
    let r: Vec<(f64, f64)> = (0..3).map(|d| span(all.iter().map(|p| p[d]))).collect();
    finish3(cx, prims, [r[0].0, r[1].0, r[2].0], [r[0].1, r[1].1, r[2].1], o, who)
}

// ---- chromaticity in space

/// CIE XYZ of linear RGB in the space with these primaries and white (xy), Y of white 1.
fn rgb_to_xyz_matrix(p: [[f64; 2]; 4]) -> [[f64; 3]; 3] {
    let col = |q: [f64; 2]| [q[0] / q[1], 1.0, (1.0 - q[0] - q[1]) / q[1]];
    let (r, g, b, w) = (col(p[0]), col(p[1]), col(p[2]), col(p[3]));
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    // scale the columns so that (1, 1, 1) maps to the white
    let s = solve3([[m[0][0], m[0][1], m[0][2], w[0]], [m[1][0], m[1][1], m[1][2], w[1]], [m[2][0], m[2][1], m[2][2], w[2]]]).unwrap_or([1.0; 3]);
    std::array::from_fn(|i| std::array::from_fn(|j| m[i][j] * s[j]))
}

fn to_space(xyz: [f64; 3], space: &str) -> [f64; 3] {
    let wn = [0.95047, 1.0, 1.08883];
    match space {
        "XYY" => {
            let s = (xyz[0] + xyz[1] + xyz[2]).max(1e-12);
            [xyz[0] / s, xyz[1] / s, xyz[1]]
        }
        "LAB" | "LCH" => {
            let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
            let (fx, fy, fz) = (f(xyz[0] / wn[0]), f(xyz[1] / wn[1]), f(xyz[2] / wn[2]));
            [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
        }
        "LUV" => {
            let d = xyz[0] + 15.0 * xyz[1] + 3.0 * xyz[2];
            let dn = wn[0] + 15.0 * wn[1] + 3.0 * wn[2];
            let (u, v) = if d > 1e-12 { (4.0 * xyz[0] / d, 9.0 * xyz[1] / d) } else { (4.0 * wn[0] / dn, 9.0 * wn[1] / dn) };
            let (un, vn) = (4.0 * wn[0] / dn, 9.0 * wn[1] / dn);
            let y = xyz[1] / wn[1];
            let l = if y > 216.0 / 24389.0 { 116.0 * y.cbrt() - 16.0 } else { 24389.0 / 27.0 * y };
            [l, 13.0 * l * (u - un), 13.0 * l * (v - vn)]
        }
        _ => xyz,
    }
}

fn chromaticity_plot3d<'h>(cx: &mut NativeCx<'_, '_, 'h>, a: &[T<'h>], o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let who = "chromaticity_plot3d";
    let space = a.get(1).and_then(|&s| if let View::Str(s) = h.view(s) { Some(s.to_uppercase()) } else { None }).unwrap_or_else(|| "XYZ".into());
    let names: Vec<String> = a
        .first()
        .map(|&s| cx.as_list(s).map(|l| l.to_vec()).unwrap_or_else(|| vec![s]))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|t| if let View::Str(s) = h.view(t) { Some(s.to_string()) } else { None })
        .collect();
    let names = if names.is_empty() { vec!["sRGB".to_string()] } else { names };
    let mut faces = vec![];
    let mut all = vec![];
    let n = 10;
    for (k, nm) in names.iter().enumerate() {
        let p = crate::panels::primaries(nm).unwrap_or_else(|| crate::panels::primaries("sRGB").unwrap_or_default());
        let m = rgb_to_xyz_matrix(p);
        let map = |c: [f64; 3]| -> [f64; 3] { to_space(std::array::from_fn(|i| m[i][0] * c[0] + m[i][1] * c[1] + m[i][2] * c[2]), &space) };
        // the six faces of the RGB cube, each a grid of quads coloured by their sRGB look
        for d in 0..3 {
            for side in [0.0, 1.0] {
                for i in 0..n {
                    for j in 0..n {
                        let corner = |u: usize, v: usize| {
                            let mut c = [0.0; 3];
                            c[d] = side;
                            c[(d + 1) % 3] = u as f64 / n as f64;
                            c[(d + 2) % 3] = v as f64 / n as f64;
                            c
                        };
                        let cs = [corner(i, j), corner(i + 1, j), corner(i + 1, j + 1), corner(i, j + 1)];
                        let mid: [f64; 3] = std::array::from_fn(|q| cs.iter().map(|c| c[q]).sum::<f64>() / 4.0);
                        let pts: Vec<[f64; 3]> = cs.iter().map(|&c| map(c)).collect();
                        all.extend(pts.iter().copied());
                        let col = if k == 0 { Rgba::rgb(mid[0], mid[1], mid[2]) } else { Rgba { a: 0.5, ..pal(k) } };
                        faces.push((pts, col));
                    }
                }
            }
        }
    }
    let prims = faces_term(h, &faces);
    let r: Vec<(f64, f64)> = (0..3).map(|d| span(all.iter().map(|p| p[d]))).collect();
    finish3(cx, prims, [r[0].0, r[1].0, r[2].0], [r[0].1, r[1].1, r[2].1], o, who)
}
