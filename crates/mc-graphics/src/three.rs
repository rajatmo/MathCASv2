//! Three-dimensional graphics: `graphics3d(content, options)` read into a scene of faces, lines, points and labels,
//! seen from a view point in perspective, lit, sorted far to near (the painter's algorithm) and turned into a
//! two-dimensional [`Graphic`] that the page painter draws like any other, so PNG, SVG and PDF come out alike and
//! labels keep the math font.
//!
//! Primitives: `sphere`, `ball`, `cuboid`, `cylinder`, `cone`, `tube`, `polygon`, `triangle`, `line`, `point`,
//! `arrow`, `text`; directives as in two dimensions (colours, `opacity`, `thickness`, `point_size`, `edge_form`,
//! `face_form`, `dashing`, ...); lighting and material directives are accepted and not used. Options: `plot_range`,
//! `box_ratios`, `view_point`, `boxed`, `axes`, `axes_label`, `plot_label`, `image_size`, `background`, `ticks`;
//! others are left out and named in [`Opts::ignored`].
use crate::color::{colour, Colour, Rgba};
use crate::label::Label;
use crate::model::{
    self, boolean, directive, head_name, label_opt, list, sym_is, Aspect, Edge, Face, Graphic, Item, Opts, Shape, Size,
    Style, POLL,
};
use crate::num::number;
use crate::surface::Role;
use crate::{ticks, Failure};
use mc_api::names;
use mc_core::{Packed, Store, View, T};

/// A point in space.
pub type P3 = [f64; 3];

/// The primitive heads of three-dimensional graphics (besides the two-dimensional ones they share).
pub const PRIMITIVES: [&str; 7] = ["sphere", "ball", "cuboid", "cylinder", "cone", "tube", "triangle"];

/// Directives of lighting and materials: accepted, not used.
pub const IGNORED_DIRECTIVES: [&str; 12] = [
    "specularity",
    "glow",
    "lighting",
    "material_shading",
    "cap_form",
    "join_form",
    "toon_shading",
    "point_light",
    "directional_light",
    "spot_light",
    "ambient_light",
    "edge_rendering",
];

/// The option names of `graphics3d` that change the picture.
pub const OPTIONS: [&str; 14] = [
    "plot_range",
    "box_ratios",
    "view_point",
    "view_vertical",
    "boxed",
    "axes",
    "axes_label",
    "plot_label",
    "image_size",
    "background",
    "ticks",
    "font_size",
    "mesh_hint",
    "light",
];

fn sub(a: P3, b: P3) -> P3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: P3, b: P3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: P3, b: P3) -> P3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(a: P3) -> P3 {
    let n = dot(a, a).sqrt();
    if n > 0.0 {
        [a[0] / n, a[1] / n, a[2] / n]
    } else {
        [0.0, 0.0, 1.0]
    }
}
fn add(a: P3, b: P3, k: f64) -> P3 {
    [a[0] + k * b[0], a[1] + k * b[1], a[2] + k * b[2]]
}

/// Whether `t` is coordinates of three or more dimensions: a list of numbers of length three or more, or (nested)
/// lists of such, or a packed array whose last dimension is three or more.
pub fn is_coords<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match h.packed(t) {
        Some(Packed::Reals { dims, .. } | Packed::Ints { dims, .. }) => return dims.last().is_some_and(|&d| d >= 3),
        Some(Packed::Complexes { .. }) => return false,
        None => {}
    }
    let mut cur = t;
    for _ in 0..4 {
        let Some(items) = list(h, cur) else { return false };
        let Some(&first) = items.first() else { return false };
        if list(h, first).is_none() {
            return items.len() >= 3 && items.iter().all(|&e| number(h, e).is_some());
        }
        cur = first;
    }
    false
}

/// A point `[x, y, z]`.
fn p3<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<P3> {
    let &[x, y, z] = list(h, t)? else { return None };
    Some([number(h, x)?, number(h, y)?, number(h, z)?])
}

/// A list of points (a packed `n x 3` array read directly).
fn pts3<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Vec<P3>> {
    match h.packed(t) {
        Some(Packed::Reals { dims, data }) if dims.len() == 2 && dims[1] == 3 => {
            return Some(data.as_chunks::<3>().0.iter().map(|p| [p[0], p[1], p[2]]).filter(|p| p.iter().all(|v| v.is_finite())).collect())
        }
        Some(Packed::Ints { dims, data }) if dims.len() == 2 && dims[1] == 3 => {
            return Some(data.as_chunks::<3>().0.iter().map(|p| [p[0] as f64, p[1] as f64, p[2] as f64]).collect())
        }
        _ => {}
    }
    list(h, t)?.iter().map(|&p| p3(h, p)).collect()
}

/// One list of points or a list of such lists.
fn runs3<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Vec<Vec<P3>>> {
    if let Some(Packed::Reals { dims, .. } | Packed::Ints { dims, .. }) = h.packed(t) {
        if dims.len() == 3 {
            return list(h, t)?.iter().map(|&r| pts3(h, r)).collect();
        }
        return Some(vec![pts3(h, t)?]);
    }
    let items = list(h, t)?;
    match items.first() {
        None => Some(vec![]),
        Some(&f) if p3(h, f).is_some() => Some(vec![pts3(h, t)?]),
        Some(_) => items.iter().map(|&r| pts3(h, r)).collect(),
    }
}

/// What the scene holds.
#[derive(Clone, Debug)]
enum Prim {
    /// a face: its corners; `smooth` faces are pieces of a curved surface (no edges drawn by default); `inside` a
    /// point inside the closed convex solid the face bounds (its faces turned away from the eye are hidden)
    Face { pts: Vec<P3>, smooth: bool, color: Option<Rgba>, inside: Option<P3> },
    Line(Vec<P3>),
    Arrow(Vec<P3>),
    Points(Vec<P3>),
    Text { label: Label, at: P3 },
}

#[derive(Clone, Debug)]
struct Elem {
    prim: Prim,
    style: usize,
}

/// The scene read from the content.
struct Scene {
    elems: Vec<Elem>,
    styles: Vec<Style>,
}

/// Segments around a circle for spheres, cylinders and cones.
const ROUND: usize = 40;

fn sphere(c: P3, r: [f64; 3], out: &mut Vec<Prim>) {
    let (nlat, nlon) = (ROUND / 2, ROUND);
    let at = |i: usize, j: usize| {
        let th = std::f64::consts::PI * i as f64 / nlat as f64;
        let ph = 2.0 * std::f64::consts::PI * j as f64 / nlon as f64;
        [c[0] + r[0] * th.sin() * ph.cos(), c[1] + r[1] * th.sin() * ph.sin(), c[2] + r[2] * th.cos()]
    };
    for i in 0..nlat {
        for j in 0..nlon {
            let mut pts = vec![at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            if i == 0 {
                pts.remove(0);
            } else if i == nlat - 1 {
                pts.remove(2);
            }
            out.push(Prim::Face { pts, smooth: true, color: None, inside: Some(c) });
        }
    }
}

/// Two unit vectors across the axis `d`.
fn across(d: P3) -> (P3, P3) {
    let a = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] };
    let u = unit(cross(d, a));
    (u, unit(cross(d, u)))
}

/// A cylinder (`r1 == r0`), a cone (`r1 == 0`) or a frustum from `a` to `b`, with its end caps.
fn solid_of_revolution(a: P3, b: P3, r0: f64, r1: f64, caps: bool, out: &mut Vec<Prim>) {
    let d = unit(sub(b, a));
    let (u, v) = across(d);
    let ring = |c: P3, r: f64| -> Vec<P3> {
        (0..ROUND)
            .map(|k| {
                let t = 2.0 * std::f64::consts::PI * k as f64 / ROUND as f64;
                add(add(c, u, r * t.cos()), v, r * t.sin())
            })
            .collect()
    };
    let (ra, rb) = (ring(a, r0), ring(b, r1));
    // with its caps the solid is closed
    let inside = caps.then(|| add(a, sub(b, a), 0.5));
    for k in 0..ROUND {
        let k1 = (k + 1) % ROUND;
        let pts = if r1 == 0.0 { vec![ra[k], ra[k1], b] } else { vec![ra[k], ra[k1], rb[k1], rb[k]] };
        out.push(Prim::Face { pts, smooth: true, color: None, inside });
    }
    if caps {
        out.push(Prim::Face { pts: ra.into_iter().rev().collect(), smooth: false, color: None, inside });
        if r1 > 0.0 {
            out.push(Prim::Face { pts: rb, smooth: false, color: None, inside });
        }
    }
}

fn cuboid(lo: P3, hi: P3, out: &mut Vec<Prim>) {
    let c = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
    for f in [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]] {
        out.push(Prim::Face { pts: f.iter().map(|&i| c(i)).collect(), smooth: false, color: None, inside: Some(add(lo, sub(hi, lo), 0.5)) });
    }
}

/// The colours of `vertex_colors: ...` on a polygon: one colour per face (the mean of its corners' colours), or
/// `None`.
fn face_colors<S: Store + ?Sized>(h: &S, o: T<'_>, faces: usize) -> Option<Vec<Rgba>> {
    let (_, rs) = head_name(h, o).filter(|(n, _)| n == "options")?;
    let v = rs.iter().find_map(|&r| match head_name(h, r) {
        Some((n, &[k, v])) if n == names::RULE && sym_is(h, k, "vertex_colors") => Some(v),
        _ => None,
    })?;
    let mean = |cs: &[Rgba]| -> Rgba {
        let n = cs.len().max(1) as f64;
        let s = cs.iter().fold([0.0; 4], |s, c| [s[0] + c.r, s[1] + c.g, s[2] + c.b, s[3] + c.a]);
        Rgba { r: s[0] / n, g: s[1] / n, b: s[2] / n, a: s[3] / n }
    };
    let one = |t: T<'_>| -> Option<Rgba> {
        match colour(h, t) {
            Colour::Value(c) => Some(c),
            _ => {
                let p = list(h, t)?;
                let v: Vec<f64> = p.iter().map(|&x| number(h, x)).collect::<Option<_>>()?;
                (v.len() >= 3).then(|| Rgba { r: v[0], g: v[1], b: v[2], a: v.get(3).copied().unwrap_or(1.0) })
            }
        }
    };
    if let Some(Packed::Reals { dims, data }) = h.packed(v) {
        // faces x corners x 3 (or 4)
        if dims.len() == 3 && dims[0] as usize == faces && dims[2] >= 3 {
            let (k, c) = (dims[1] as usize, dims[2] as usize);
            return Some(
                (0..faces)
                    .map(|f| {
                        let cs: Vec<Rgba> = (0..k)
                            .map(|j| {
                                let at = (f * k + j) * c;
                                Rgba { r: data[at], g: data[at + 1], b: data[at + 2], a: if c > 3 { data[at + 3] } else { 1.0 } }
                            })
                            .collect();
                        mean(&cs)
                    })
                    .collect(),
            );
        }
    }
    let items = list(h, v)?;
    if faces == 1 {
        let cs: Vec<Rgba> = items.iter().map(|&c| one(c)).collect::<Option<_>>()?;
        return Some(vec![mean(&cs)]);
    }
    items
        .iter()
        .map(|&f| {
            let cs: Vec<Rgba> = list(h, f)?.iter().map(|&c| one(c)).collect::<Option<_>>()?;
            Some(mean(&cs))
        })
        .collect()
}

/// The primitives of `name(args)`; `None` when it is not a three-dimensional primitive (or its coordinates are
/// not numbers).
fn primitive<S: Store + ?Sized>(h: &S, name: &str, args: &[T<'_>]) -> Option<Vec<Prim>> {
    let (args, opts) = match args.split_last() {
        Some((&o, rest)) if head_name(h, o).is_some_and(|(n, _)| n == "options") => (rest, Some(o)),
        _ => (args, None),
    };
    let mut out = vec![];
    let radius = |k: usize| -> Option<f64> {
        match args.get(k) {
            Some(&r) => number(h, r).filter(|r| *r >= 0.0),
            None => Some(1.0),
        }
    };
    match name {
        "sphere" | "ball" => {
            let r = radius(1)?;
            let cs = match args.first() {
                None => vec![[0.0; 3]],
                Some(&c) => match p3(h, c) {
                    Some(c) => vec![c],
                    None => pts3(h, c)?,
                },
            };
            for c in cs {
                sphere(c, [r; 3], &mut out);
            }
        }
        "ellipsoid" => {
            let c = p3(h, *args.first()?)?;
            let r = p3(h, *args.get(1)?)?;
            sphere(c, r, &mut out);
        }
        "cuboid" => match args {
            [] => cuboid([0.0; 3], [1.0; 3], &mut out),
            [a] => match p3(h, *a) {
                Some(a) => cuboid(a, add(a, [1.0; 3], 1.0), &mut out),
                None => {
                    for a in pts3(h, *a)? {
                        cuboid(a, add(a, [1.0; 3], 1.0), &mut out);
                    }
                }
            },
            [a, b, ..] => {
                let (a, b) = (p3(h, *a)?, p3(h, *b)?);
                cuboid([a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])], [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])], &mut out);
            }
        },
        "cylinder" | "cone" | "tube" => {
            let ends = match args.first() {
                None => vec![vec![[0.0, 0.0, -1.0], [0.0, 0.0, 1.0]]],
                Some(&e) => runs3(h, e)?,
            };
            let r = match (name, args.get(1)) {
                ("tube", None) => 0.05,
                _ => radius(1)?,
            };
            for e in ends {
                if name == "tube" {
                    for w in e.windows(2) {
                        solid_of_revolution(w[0], w[1], r, r, false, &mut out);
                    }
                } else {
                    let (&a, &b) = (e.first()?, e.get(1)?);
                    solid_of_revolution(a, b, r, if name == "cone" { 0.0 } else { r }, true, &mut out);
                }
            }
        }
        "polygon" | "triangle" => {
            let rings = match args {
                [] if name == "triangle" => vec![vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]],
                [p] => runs3(h, *p)?,
                [p, faces] => {
                    let pts = pts3(h, *p)?;
                    let idx = |f: T<'_>| -> Option<Vec<P3>> {
                        list(h, f)?
                            .iter()
                            .map(|&i| match h.view(i) {
                                View::Int(k) if k >= 1 && (k as usize) <= pts.len() => Some(pts[k as usize - 1]),
                                _ => None,
                            })
                            .collect()
                    };
                    let fs = list(h, *faces)?;
                    if fs.first().is_some_and(|&f| list(h, f).is_some()) {
                        fs.iter().map(|&f| idx(f)).collect::<Option<_>>()?
                    } else {
                        vec![idx(*faces)?]
                    }
                }
                _ => return None,
            };
            let colors = opts.and_then(|o| face_colors(h, o, rings.len()));
            let mesh = rings.len() > 8;
            for (k, pts) in rings.into_iter().enumerate() {
                if name == "triangle" && pts.len() > 3 {
                    for t in pts.chunks(3).filter(|c| c.len() == 3) {
                        out.push(Prim::Face { pts: t.to_vec(), smooth: false, color: None, inside: None });
                    }
                } else if pts.len() >= 3 {
                    let color = colors.as_ref().and_then(|c| c.get(k).copied());
                    out.push(Prim::Face { pts, smooth: colors.is_some() || mesh, color, inside: None });
                }
            }
        }
        "line" => out.extend(runs3(h, *args.first()?)?.into_iter().map(Prim::Line)),
        "arrow" => out.extend(runs3(h, *args.first()?)?.into_iter().map(Prim::Arrow)),
        "point" => {
            let p = *args.first()?;
            out.push(Prim::Points(match p3(h, p) {
                Some(q) => vec![q],
                None => pts3(h, p)?,
            }));
        }
        "text" => {
            let at = p3(h, *args.get(1)?)?;
            let inner = match head_name(h, args[0]) {
                Some((n, a)) if n == "style" && !a.is_empty() => a[0],
                _ => args[0],
            };
            out.push(Prim::Text { label: Label::of(h, inner), at });
        }
        _ => return None,
    }
    Some(out)
}

/// Read the content of a `graphics3d` term into a scene. Things that are neither primitives nor directives are
/// left out.
fn scene<S: Store + ?Sized>(h: &S, content: T<'_>, stop: &dyn Fn() -> bool) -> Result<Scene, Failure> {
    let mut sc = Scene { elems: vec![], styles: vec![Style::uncoloured()] };
    let single = [content];
    let mut stack: Vec<(&[T<'_>], usize)> = vec![(&single[..], 0)];
    let mut seen = 0usize;
    while let Some((rest, style)) = stack.last_mut() {
        let Some((&e, tail)) = rest.split_first() else {
            stack.pop();
            continue;
        };
        *rest = tail;
        let cur = *style;
        seen += 1;
        if seen.is_multiple_of(POLL) && stop() {
            return Err(Failure::new("interrupted", ""));
        }
        if let Some(items) = list(h, e) {
            stack.push((items, cur));
            continue;
        }
        if let Some((name, args)) = head_name(h, e) {
            if (name == "graphics3d" || model::TRANSPARENT.contains(&name.as_str())) && !args.is_empty() {
                stack.push((&args[..1], cur));
                continue;
            }
            if name == "style" && !args.is_empty() {
                let mut s = sc.styles[cur].clone();
                let _ = model::styled(h, &args[1..], &mut s);
                sc.styles.push(s);
                stack.push((&args[..1], sc.styles.len() - 1));
                continue;
            }
            if IGNORED_DIRECTIVES.contains(&name.as_str()) {
                continue;
            }
            if let Some(ps) = primitive(h, &name, args) {
                sc.elems.extend(ps.into_iter().map(|prim| Elem { prim, style: cur }));
                continue;
            }
        }
        let mut s = sc.styles[cur].clone();
        if let Ok(true) = directive(h, e, &mut s) {
            sc.styles.push(s);
            if let Some(top) = stack.last_mut() {
                top.1 = sc.styles.len() - 1;
            }
        }
    }
    Ok(sc)
}

/// The options of a 3D graphic.
#[derive(Clone, Debug)]
struct Opts3 {
    range: [[Option<f64>; 2]; 3],
    ratios: Option<P3>,
    view: P3,
    vertical: P3,
    boxed: bool,
    axes: bool,
    axes_label: [Option<Label>; 3],
    plot_label: Option<Label>,
    image_size: (Option<f64>, Option<f64>),
    background: Option<Rgba>,
    themed_background: bool,
    ticks: bool,
    font_size: f64,
    ignored: Vec<String>,
}

impl Default for Opts3 {
    fn default() -> Self {
        Opts3 {
            range: [[None; 2]; 3],
            ratios: None,
            view: [1.3, -2.4, 2.0],
            vertical: [0.0, 0.0, 1.0],
            boxed: true,
            axes: false,
            axes_label: [None, None, None],
            plot_label: None,
            image_size: (None, None),
            background: Some(Rgba::WHITE),
            themed_background: true,
            ticks: true,
            font_size: 10.0,
            ignored: vec![],
        }
    }
}

/// A view point by name.
fn named_view(name: &str) -> Option<P3> {
    Some(match name {
        "front" => [0.0, -3.4, 0.0],
        "back" => [0.0, 3.4, 0.0],
        "left" => [-3.4, 0.0, 0.0],
        "right" => [3.4, 0.0, 0.0],
        "above" | "top" => [0.0, -0.01, 3.4],
        "below" | "bottom" => [0.0, -0.01, -3.4],
        _ => return None,
    })
}

fn options3<S: Store + ?Sized>(h: &S, o: Option<T<'_>>) -> Result<Opts3, Failure> {
    let mut opts = Opts3::default();
    let Some(o) = o else { return Ok(opts) };
    for (name, v) in model::rules(h, o)? {
        let ok = match name.as_str() {
            "plot_range" => (|| {
                if sym_is(h, v, "all") || sym_is(h, v, "automatic") || sym_is(h, v, "full") {
                    return Some(());
                }
                if let Some(r) = number(h, v) {
                    opts.range = [[None; 2], [None; 2], [Some(-r), Some(r)]];
                    return Some(());
                }
                let items = list(h, v)?;
                match items {
                    [x, y, z] => opts.range = [model::range(h, *x)?, model::range(h, *y)?, model::range(h, *z)?],
                    [_, _] => opts.range[2] = model::range(h, v)?,
                    _ => return None,
                }
                Some(())
            })(),
            "box_ratios" => {
                if sym_is(h, v, "automatic") {
                    opts.ratios = None;
                    Some(())
                } else {
                    p3(h, v).filter(|r| r.iter().all(|x| *x > 0.0)).map(|r| opts.ratios = Some(r))
                }
            }
            "view_point" => match h.view(v) {
                View::Sym(s) => named_view(&h.sym_name(s)).map(|p| opts.view = p),
                _ => p3(h, v)
                    .or_else(|| {
                        // [inf, 0, 0] and the like: far away along an axis
                        let items = list(h, v)?;
                        (items.len() == 3).then(|| {
                            let c = |t: T<'_>| match h.view(t) {
                                View::Sym(_) => 100.0,
                                _ => number(h, t).unwrap_or(0.0),
                            };
                            [c(items[0]), c(items[1]), c(items[2])]
                        })
                    })
                    .filter(|p| dot(*p, *p) > 1e-12)
                    .map(|p| opts.view = p),
            },
            "view_vertical" => p3(h, v).filter(|p| p.iter().all(|x| x.is_finite()) && dot(*p, *p) > 1e-12).map(|p| opts.vertical = p),
            "boxed" => boolean(h, v).map(|b| opts.boxed = b),
            "axes" => match boolean(h, v) {
                Some(b) => {
                    opts.axes = b;
                    Some(())
                }
                None => list(h, v).map(|l| opts.axes = l.iter().any(|&e| boolean(h, e) == Some(true))),
            },
            "axes_label" => match list(h, v) {
                Some(ls) if ls.len() == 3 => {
                    opts.axes_label = [label_opt(h, ls[0]), label_opt(h, ls[1]), label_opt(h, ls[2])];
                    Some(())
                }
                _ => {
                    opts.axes_label[2] = label_opt(h, v);
                    Some(())
                }
            },
            "plot_label" => {
                opts.plot_label = label_opt(h, v);
                Some(())
            }
            "image_size" => {
                let size = |t: T<'_>| -> Option<Option<f64>> {
                    if sym_is(h, t, "automatic") || sym_is(h, t, "all") {
                        Some(None)
                    } else if let Some(w) = model::named_width(h, t) {
                        Some(Some(w))
                    } else {
                        number(h, t).filter(|w| *w >= 1.0 && *w <= model::MAX_SIDE).map(Some)
                    }
                };
                match list(h, v) {
                    Some(&[w, ht]) => (|| {
                        opts.image_size = (size(w)?, size(ht)?);
                        Some(())
                    })(),
                    _ => size(v).map(|w| opts.image_size = (w, None)),
                }
            }
            "background" => {
                opts.themed_background = false;
                if sym_is(h, v, "none") {
                    opts.background = None;
                    Some(())
                } else {
                    match colour(h, v) {
                        Colour::Value(c) => {
                            opts.background = Some(c);
                            Some(())
                        }
                        _ => None,
                    }
                }
            }
            "ticks" => {
                opts.ticks = !(sym_is(h, v, "none") || sym_is(h, v, names::FALSE));
                Some(())
            }
            "font_size" => number(h, v).filter(|s| *s > 0.0 && *s <= 1000.0).map(|s| opts.font_size = s),
            n if model::known_option(n) || n == "lighting" || n == "spherical_region" => Some(()),
            _ => {
                opts.ignored.push(name.clone());
                Some(())
            }
        };
        if ok.is_none() {
            opts.ignored.push(format!("{name}: {}", mc_syntax::Printer::new(h).expr(v)));
        }
    }
    Ok(opts)
}

/// The camera: scaled coordinates (the box centred at the origin, its longest side 1 when the ratios are the
/// data's) and a perspective view from the view point.
struct Camera {
    centre: P3,
    scale: P3,
    eye: P3,
    right: P3,
    up: P3,
    fwd: P3,
}

impl Camera {
    fn scaled(&self, p: P3) -> P3 {
        [(p[0] - self.centre[0]) * self.scale[0], (p[1] - self.centre[1]) * self.scale[1], (p[2] - self.centre[2]) * self.scale[2]]
    }
    /// Screen coordinates (y up) and depth of a point.
    fn project(&self, p: P3) -> (f64, f64, f64) {
        let d = sub(self.scaled(p), self.eye);
        let z = dot(d, self.fwd).max(1e-6);
        (dot(d, self.right) / z, dot(d, self.up) / z, z)
    }
}

/// Cut a polygon to the box `lo..hi` (Sutherland-Hodgman, one plane at a time).
fn clip_polygon(mut pts: Vec<P3>, lo: P3, hi: P3) -> Vec<P3> {
    for k in 0..3 {
        for (bound, keep_above) in [(lo[k], true), (hi[k], false)] {
            if pts.is_empty() {
                return pts;
            }
            let inside = |p: &P3| if keep_above { p[k] >= bound } else { p[k] <= bound };
            let mut out = Vec::with_capacity(pts.len() + 2);
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                let (ia, ib) = (inside(&a), inside(&b));
                if ia {
                    out.push(a);
                }
                if ia != ib {
                    let t = (bound - a[k]) / (b[k] - a[k]);
                    out.push(add(a, sub(b, a), t));
                }
            }
            pts = out;
        }
    }
    pts
}

/// Cut a segment to the box; `None` when nothing of it is inside.
fn clip_segment(a: P3, b: P3, lo: P3, hi: P3) -> Option<(P3, P3)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let d = sub(b, a);
    for k in 0..3 {
        if d[k].abs() < 1e-300 {
            if a[k] < lo[k] || a[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (u, v) = ((lo[k] - a[k]) / d[k], (hi[k] - a[k]) / d[k]);
        let (u, v) = (u.min(v), u.max(v));
        t0 = t0.max(u);
        t1 = t1.min(v);
        if t0 > t1 {
            return None;
        }
    }
    Some((add(a, d, t0), add(a, d, t1)))
}

/// Something to draw with its depth (larger is farther).
struct Drawn {
    depth: f64,
    shape: Shape,
    style: Style,
}

/// The lit colour of a face of colour `c` with unit normal `n` (turned towards the eye), seen along `view` (unit,
/// from the face to the eye): soft ambient light, a key light from the upper left, a fill light from the right,
/// and a faint highlight.
fn shade(c: Rgba, n: P3, cam: &Camera, view: P3) -> Rgba {
    let key = unit(add(add(add([0.0; 3], cam.up, 0.8), cam.right, -0.5), cam.fwd, -0.6));
    let fill = unit(add(add([0.0; 3], cam.right, 0.8), cam.fwd, -0.4));
    let top = [0.0, 0.0, 1.0];
    let diffuse = 0.55 * dot(n, key).max(0.0) + 0.2 * dot(n, fill).max(0.0) + 0.1 * dot(n, top).max(0.0);
    let k = 0.42 + diffuse;
    let half = unit(add(key, view, 1.0));
    let spec = 0.18 * dot(n, half).max(0.0).powi(24);
    Rgba { r: (c.r * k + spec).min(1.0), g: (c.g * k + spec).min(1.0), b: (c.b * k + spec).min(1.0), a: c.a }
}

/// Newell's normal of a polygon (not normalised).
fn newell(q: &[P3]) -> P3 {
    let mut nrm = [0.0; 3];
    for i in 0..q.len() {
        let (a, b) = (q[i], q[(i + 1) % q.len()]);
        nrm[0] += (a[1] - b[1]) * (a[2] + b[2]);
        nrm[1] += (a[2] - b[2]) * (a[0] + b[0]);
        nrm[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    nrm
}

/// The default colour of faces in 3D graphics: a warm light gray-beige (lighting gives it shape).
const FACE: Rgba = Rgba::rgb(0.86, 0.84, 0.80);
/// The box and axes.
const BOX: Rgba = Rgba::rgb(0.35, 0.35, 0.35);

/// Read a `graphics3d(content)` or `graphics3d(content, options(...))` term as the two-dimensional picture of the
/// scene.
pub fn read<S: Store + ?Sized>(h: &S, t: T<'_>, stop: &dyn Fn() -> bool) -> Result<Graphic, Failure> {
    let (content, o) = match head_name(h, t) {
        Some((n, args)) if n == "graphics3d" => match args {
            [c] => (*c, None),
            [c, o] => (*c, Some(*o)),
            _ => return Err(Failure::new("not_graphics", "graphics3d takes content and options")),
        },
        _ => return Err(Failure::new("not_graphics", mc_syntax::Printer::new(h).expr(t))),
    };
    let sc = scene(h, content, stop)?;
    let o3 = options3(h, o)?;
    picture(sc, o3, stop)
}

/// Whether `t` is a `graphics3d` term.
pub fn is_graphics3d<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    head_name(h, t).is_some_and(|(n, a)| n == "graphics3d" && matches!(a.len(), 1 | 2))
}

fn picture(sc: Scene, o3: Opts3, stop: &dyn Fn() -> bool) -> Result<Graphic, Failure> {
    let p = prepare(sc, o3, stop)?;
    p.draw(p.o3.view, p.o3.vertical, stop)
}

/// A face ready to be drawn from any view point.
#[derive(Clone, Debug)]
struct PFace {
    /// the corners, cut to the box when the plot range clips (data coordinates)
    pts: Vec<P3>,
    /// the unit normal in scaled coordinates (either way round: it is turned to the eye when drawn)
    nrm: P3,
    /// the centre in scaled coordinates
    mid: P3,
    /// smooth shading: the unit vertex normal at each corner (`None` where there is none)
    vns: Option<Vec<Option<P3>>>,
    smooth: bool,
    color: Option<Rgba>,
    /// the outward normal of a face of a closed convex solid (scaled coordinates): hidden when it faces away
    outward: Option<P3>,
}

/// A primitive ready to be drawn from any view point.
#[derive(Clone, Debug)]
enum PPrim {
    Face(PFace),
    /// line segments cut to the box; the last one ends in an arrowhead for an arrow
    Segs { segs: Vec<(P3, P3)>, arrow: bool },
    Points(Vec<P3>),
    Text { label: Label, at: P3 },
}

/// A three-dimensional graphic read and prepared once: the box, the clipping to it, the scaling and the normals
/// (what does not depend on the view point) are worked out, so drawing it from another view point only projects,
/// lights and sorts. [`Prepared::graphic`] gives the two-dimensional picture for a [`View3d`].
#[derive(Clone, Debug)]
pub struct Prepared {
    elems: Vec<(PPrim, usize)>,
    styles: Vec<Style>,
    o3: Opts3,
    lo: P3,
    hi: P3,
    centre: P3,
    scale: P3,
}

/// Where a three-dimensional graphic is seen from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3d {
    /// the eye in scaled coordinates: the box centred at the origin, its longest side 1 (as the `view_point`
    /// option)
    pub view_point: P3,
    /// the direction that is drawn upwards (as the `view_vertical` option; `[0, 0, 1]` by default)
    pub view_vertical: P3,
    /// magnification: 1 fits the picture to the page, 2 shows the middle half of it twice as large
    pub zoom: f64,
}

/// The view point after a drag: `turn` radians about the vertical (positive: the scene turns to the right, so the
/// eye moves left) and `tilt` radians towards the vertical (positive: the eye rises), at the same distance. The eye
/// stays off the vertical by a little, so "up" stays defined.
pub fn orbit(view_point: P3, vertical: P3, turn: f64, tilt: f64) -> P3 {
    let r = dot(view_point, view_point).sqrt();
    if !(r > 0.0 && r.is_finite()) {
        return Opts3::default().view;
    }
    let up = if dot(vertical, vertical) > 1e-24 && vertical.iter().all(|v| v.is_finite()) { unit(vertical) } else { [0.0, 0.0, 1.0] };
    let along = dot(view_point, up);
    let mut flat = add(view_point, up, -along);
    if dot(flat, flat) < 1e-24 * r * r {
        // on the vertical: any direction across it
        flat = cross(up, [1.0, 0.0, 0.0]);
        if dot(flat, flat) < 1e-12 {
            flat = cross(up, [0.0, 1.0, 0.0]);
        }
    }
    let f = unit(flat);
    let limit = std::f64::consts::FRAC_PI_2 - 0.01;
    let el = (along.atan2(dot(flat, flat).sqrt()) + tilt).clamp(-limit, limit);
    // the flat part turned about the vertical by -turn (Rodrigues; f is across the vertical)
    let side = cross(up, f);
    let f2 = add(add([0.0; 3], f, (-turn).cos()), side, (-turn).sin());
    add(add([0.0; 3], unit(f2), r * el.cos()), up, r * el.sin())
}

impl Prepared {
    /// Read a `graphics3d` term and prepare it.
    pub fn read<S: Store + ?Sized>(h: &S, t: T<'_>, stop: &dyn Fn() -> bool) -> Result<Prepared, Failure> {
        let (content, o) = match head_name(h, t) {
            Some((n, args)) if n == "graphics3d" => match args {
                [c] => (*c, None),
                [c, o] => (*c, Some(*o)),
                _ => return Err(Failure::new("not_graphics", "graphics3d takes content and options")),
            },
            _ => return Err(Failure::new("not_graphics", mc_syntax::Printer::new(h).expr(t))),
        };
        let sc = scene(h, content, stop)?;
        let o3 = options3(h, o)?;
        prepare(sc, o3, stop)
    }

    /// The view the graphic's options give (`view_point`, zoom 1).
    pub fn default_view(&self) -> View3d {
        View3d { view_point: self.o3.view, view_vertical: self.o3.vertical, zoom: 1.0 }
    }

    /// The number of primitives (faces, line runs, point sets, texts).
    pub fn len(&self) -> usize {
        self.elems.len()
    }

    /// Whether the scene is empty.
    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// The two-dimensional picture seen from `view`. With `page` (width, height in points) the page has that size
    /// whatever the view, the picture fitted into it with equal units (a front end redraws into the same
    /// rectangle while the view turns); without it the page is shaped like the picture, as a plain render.
    pub fn graphic(&self, view: &View3d, page: Option<(f64, f64)>, stop: &dyn Fn() -> bool) -> Result<Graphic, Failure> {
        let eye = view.view_point;
        if !(eye.iter().all(|v| v.is_finite()) && dot(eye, eye) > 1e-12) {
            return Err(Failure::new("argument", format!("view point {eye:?}")));
        }
        if !(view.zoom.is_finite() && view.zoom > 0.0) {
            return Err(Failure::new("argument", format!("zoom {}", view.zoom)));
        }
        let up = view.view_vertical;
        if !(up.iter().all(|v| v.is_finite()) && dot(up, up) > 1e-12) {
            return Err(Failure::new("argument", format!("view vertical {up:?}")));
        }
        let mut g = self.draw(eye, up, stop)?;
        if let Some((w, ht)) = page {
            g.opts.image_size = (Some(w), Some(ht));
        }
        if view.zoom != 1.0 {
            let b = crate::page::bounds(&g.items, stop)?;
            let pad = g.opts.padding.unwrap_or(0.02);
            for (k, (lo, hi)) in b.into_iter().enumerate() {
                if !(lo.is_finite() && hi > lo) {
                    continue;
                }
                let (mid, half) = ((lo + hi) / 2.0, (hi - lo) * (0.5 + pad) / view.zoom);
                g.opts.plot_range[k] = [Some(mid - half), Some(mid + half)];
            }
            g.opts.clip = Some(view.zoom > 1.0);
        }
        Ok(g)
    }

    /// The camera for the eye at `eye` with `vertical` drawn upwards.
    fn camera(&self, eye: P3, vertical: P3) -> Camera {
        let fwd = unit([-eye[0], -eye[1], -eye[2]]);
        let mut right = cross(fwd, vertical);
        if dot(right, right) < 1e-12 {
            // looking along the vertical: any direction across it
            right = cross(fwd, [1.0, 0.0, 0.0]);
            if dot(right, right) < 1e-12 {
                right = [0.0, 1.0, 0.0];
            }
        }
        let right = unit(right);
        let up = unit(cross(right, fwd));
        Camera { centre: self.centre, scale: self.scale, eye, right, up, fwd }
    }

    /// The picture from the eye at `eye`, the page shaped like it.
    fn draw(&self, eye: P3, vertical: P3, stop: &dyn Fn() -> bool) -> Result<Graphic, Failure> {
        let (o3, lo, hi, centre, scale) = (&self.o3, self.lo, self.hi, self.centre, self.scale);
        let cam = self.camera(eye, vertical);
        let mut drawn: Vec<Drawn> = Vec::with_capacity(self.elems.len() + 16);
        let pt = |x: f64, y: f64| model::Pt { x, y, scaled: false };
        let proj = |p: P3| {
            let (x, y, z) = cam.project(p);
            (pt(x, y), z)
        };
        // depth bias of lines over the faces they lie on
        let bias = 0.004 * dot(eye, eye).sqrt();
        let mut n = 0usize;
        for (prim, style) in &self.elems {
            n += 1;
            if n.is_multiple_of(POLL) && stop() {
                return Err(Failure::new("interrupted", ""));
            }
            let st = &self.styles[*style];
            match prim {
                PPrim::Face(f) => {
                    let view = unit(sub(eye, f.mid));
                    // the back of an opaque closed solid is hidden by its front: not drawn
                    if f.outward.is_some_and(|o| dot(o, view) < -1e-9) && !matches!(st.face, Face::None) && st.opacity >= 1.0 && f.color.is_none_or(|c| c.a >= 1.0) {
                        continue;
                    }
                    let nrm = if dot(f.nrm, view) < 0.0 { [-f.nrm[0], -f.nrm[1], -f.nrm[2]] } else { f.nrm };
                    let base = match (f.color, st.face) {
                        (Some(c), _) => c.faded(st.opacity),
                        (None, Face::None) => {
                            // edges only
                            let mut s = st.clone();
                            s.face = Face::None;
                            s.edge = Some(st.edge.clone().unwrap_or(Edge { color: BOX, thickness: Size::Abs(0.6), dashing: vec![] }));
                            let ps: Vec<(model::Pt, f64)> = f.pts.iter().map(|&p| proj(p)).collect();
                            let depth = ps.iter().map(|p| p.1).sum::<f64>() / ps.len() as f64;
                            drawn.push(Drawn { depth, shape: Shape::Polygon(vec![ps.iter().map(|p| p.0).collect()]), style: s });
                            continue;
                        }
                        (None, Face::Color(c)) => c,
                        (None, Face::Current) => {
                            if st.color == Rgba::BLACK && sc_default(st) {
                                FACE.faded(st.opacity)
                            } else {
                                st.ink()
                            }
                        }
                    };
                    let lit = match &f.vns {
                        Some(vns) => {
                            // the mean of the light at the corners, each with its vertex normal turned to the eye
                            let mut acc = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
                            for vn in vns {
                                let mut vn = vn.unwrap_or(nrm);
                                if dot(vn, view) < 0.0 {
                                    vn = [-vn[0], -vn[1], -vn[2]];
                                }
                                let c = shade(base, vn, &cam, view);
                                acc = Rgba { r: acc.r + c.r, g: acc.g + c.g, b: acc.b + c.b, a: acc.a + c.a };
                            }
                            let k = vns.len() as f64;
                            Rgba { r: acc.r / k, g: acc.g / k, b: acc.b / k, a: acc.a / k }
                        }
                        None => shade(base, nrm, &cam, view),
                    };
                    let ps: Vec<(model::Pt, f64)> = f.pts.iter().map(|&p| proj(p)).collect();
                    let depth = ps.iter().map(|p| p.1).sum::<f64>() / ps.len() as f64;
                    let mut s = Style { face: Face::Color(lit), ..Style::default() };
                    s.edge = match (&st.edge, f.smooth) {
                        (Some(e), _) => Some(e.clone()),
                        // a hairline in the face's own colour hides the seams between the pieces of a surface
                        (None, true) if lit.a >= 0.99 => Some(Edge { color: lit, thickness: Size::Abs(0.5), dashing: vec![] }),
                        (None, true) => None,
                        (None, false) => Some(Edge { color: Rgba { a: 0.5 * lit.a, ..BOX }, thickness: Size::Abs(0.5), dashing: vec![] }),
                    };
                    drawn.push(Drawn { depth, shape: Shape::Polygon(vec![ps.iter().map(|p| p.0).collect()]), style: s });
                }
                PPrim::Segs { segs, arrow } => {
                    let last = segs.len().saturating_sub(1);
                    for (i, &(a, b)) in segs.iter().enumerate() {
                        let ((pa, za), (pb, zb)) = (proj(a), proj(b));
                        let shape = if *arrow && i == last { Shape::Arrow(vec![vec![pa, pb]]) } else { Shape::Line(vec![vec![pa, pb]]) };
                        drawn.push(Drawn { depth: (za + zb) / 2.0 - bias, shape, style: st.clone() });
                    }
                }
                PPrim::Points(pts) => {
                    for &p in pts {
                        let (q, z) = proj(p);
                        let mut s = st.clone();
                        if s.point_size == Style::default().point_size {
                            s.point_size = Size::Abs(5.0);
                        }
                        drawn.push(Drawn { depth: z - bias, shape: Shape::Points(vec![q]), style: s });
                    }
                }
                PPrim::Text { label, at } => {
                    let (q, z) = proj(*at);
                    drawn.push(Drawn { depth: z - 2.0 * bias, shape: Shape::Text { label: label.clone(), at: q, offset: (0.0, 0.0) }, style: st.clone() });
                }
            }
        }
        // the box's edges and the axes
        let corner = |i: usize| [if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] }];
        let edges: [(usize, usize); 12] = [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)];
        let thin = Style { thickness: Size::Abs(0.75), color: Rgba { a: 0.8, ..BOX }, role: Role::Frame, ..Style::default() };
        if o3.boxed {
            for &(i, j) in &edges {
                let ((pa, za), (pb, zb)) = (proj(corner(i)), proj(corner(j)));
                drawn.push(Drawn { depth: (za + zb) / 2.0 - bias, shape: Shape::Line(vec![vec![pa, pb]]), style: thin.clone() });
            }
        }
        let mut labels: Vec<Drawn> = vec![];
        if o3.axes {
            let mid = |i: usize, j: usize| {
                let (a, b) = (corner(i), corner(j));
                cam.project([(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0]).2
            };
            // x and y along the bottom edges nearest the eye; z along the leftmost vertical edge
            let pick = |cands: &[(usize, usize)]| *cands.iter().min_by(|a, b| mid(a.0, a.1).total_cmp(&mid(b.0, b.1))).unwrap_or(&cands[0]);
            let xe = pick(&[(0, 1), (2, 3)]);
            let ye = pick(&[(0, 2), (1, 3)]);
            let ze = *[(0, 4), (1, 5), (2, 6), (3, 7)]
                .iter()
                .min_by(|a, b| cam.project(corner(a.0)).0.total_cmp(&cam.project(corner(b.0)).0))
                .unwrap_or(&(0, 4));
            let box_mid = cam.scaled(centre);
            let tick_style = Style { thickness: Size::Abs(0.75), color: BOX, role: Role::Frame, ..Style::default() };
            let text_style = Style { color: Rgba::rgb(0.15, 0.15, 0.15), font_size: Some(o3.font_size), role: Role::Text, ..Style::default() };
            for (k, (i, j)) in [(0, xe), (1, ye), (2, ze)] {
                let (a, b) = (corner(i), corner(j));
                if !o3.boxed {
                    let ((pa, za), (pb, zb)) = (proj(a), proj(b));
                    drawn.push(Drawn { depth: (za + zb) / 2.0 - bias, shape: Shape::Line(vec![vec![pa, pb]]), style: thin.clone() });
                }
                // outward: from the box's centre towards the edge, across the axis
                let m = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
                let mut out = sub(cam.scaled(m), box_mid);
                out[k] = 0.0;
                let out = unit(out);
                // the outward direction back in data units, 3% of the box
                let step = |p: P3, f: f64| -> P3 {
                    let mut q = p;
                    for c in 0..3 {
                        if c != k && scale[c] > 0.0 {
                            q[c] += out[c] * f / scale[c];
                        }
                    }
                    q
                };
                if o3.ticks {
                    for (u, l) in ticks::auto(lo[k], hi[k], 5) {
                        let mut p = a;
                        p[k] = u;
                        let ((pa, _), (pb, _)) = (proj(p), proj(step(p, 0.025)));
                        labels.push(Drawn { depth: 0.0, shape: Shape::Line(vec![vec![pa, pb]]), style: tick_style.clone() });
                        if let Some(l) = l {
                            let (q, _) = proj(step(p, 0.08));
                            labels.push(Drawn { depth: 0.0, shape: Shape::Text { label: l, at: q, offset: (0.0, 0.0) }, style: text_style.clone() });
                        }
                    }
                }
                if let Some(l) = &o3.axes_label[k] {
                    let mut p = a;
                    p[k] = (lo[k] + hi[k]) / 2.0;
                    let (q, _) = proj(step(p, 0.17));
                    labels.push(Drawn { depth: 0.0, shape: Shape::Text { label: l.clone(), at: q, offset: (0.0, 0.0) }, style: text_style.clone() });
                }
            }
        }
        drawn.sort_by(|a, b| b.depth.total_cmp(&a.depth));
        drawn.extend(labels);
        let mut g = Graphic { items: Vec::with_capacity(drawn.len()), styles: vec![Style::default()], opts: Opts::default() };
        g.styles.reserve(drawn.len());
        for d in drawn {
            g.styles.push(d.style);
            g.items.push(Item { shape: d.shape, style: g.styles.len() - 1 });
        }
        g.opts.aspect = Aspect::Auto;
        g.opts.padding = Some(0.03);
        g.opts.image_size = match o3.image_size {
            (None, None) => (Some(360.0), None),
            s => s,
        };
        g.opts.background = o3.background;
        g.opts.themed_background = o3.themed_background;
        g.opts.plot_label = o3.plot_label.clone();
        g.opts.font_size = o3.font_size;
        g.opts.ignored = o3.ignored.clone();
        g.opts.clip = Some(false);
        Ok(g)
    }
}

/// Work out what of the scene does not depend on the view point: the box, the clipping to it, the scaling, the
/// face and vertex normals.
fn prepare(sc: Scene, o3: Opts3, stop: &dyn Fn() -> bool) -> Result<Prepared, Failure> {
    // the box: the data's extent, or the plot range's ends
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut grow = |p: &P3| {
        for k in 0..3 {
            if p[k].is_finite() {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
    };
    for e in &sc.elems {
        match &e.prim {
            Prim::Face { pts, .. } | Prim::Line(pts) | Prim::Arrow(pts) | Prim::Points(pts) => pts.iter().for_each(&mut grow),
            Prim::Text { at, .. } => grow(at),
        }
    }
    let mut clip = false;
    for k in 0..3 {
        if !lo[k].is_finite() {
            (lo[k], hi[k]) = (-1.0, 1.0);
        }
        if hi[k] - lo[k] <= 1e-12 * lo[k].abs().max(1.0) {
            let d = lo[k].abs().max(1.0) * 0.5;
            lo[k] -= d;
            hi[k] += d;
        }
        if let Some(a) = o3.range[k][0] {
            lo[k] = a;
            clip = true;
        }
        if let Some(b) = o3.range[k][1] {
            hi[k] = b;
            clip = true;
        }
        if hi[k] <= lo[k] {
            hi[k] = lo[k] + 1.0;
        }
    }
    let len = [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]];
    let ratios = o3.ratios.unwrap_or(len);
    let m = ratios.iter().cloned().fold(0.0, f64::max).max(1e-300);
    let scale = [ratios[0] / m / len[0], ratios[1] / m / len[1], ratios[2] / m / len[2]];
    let centre = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let scaled = |p: P3| [(p[0] - centre[0]) * scale[0], (p[1] - centre[1]) * scale[1], (p[2] - centre[2]) * scale[2]];
    // smooth shading: at each vertex of a smooth face the mean of the normals of the faces around it (each
    // turned to agree with the first seen), each face lit as the mean of its vertices' light
    let key = |p: P3| p.map(|v| (v * 1e9).round() as i64);
    let mut vnormals: std::collections::HashMap<[i64; 3], P3> = std::collections::HashMap::new();
    let mut n = 0usize;
    for e in &sc.elems {
        if let Prim::Face { pts, smooth: true, .. } = &e.prim {
            n += 1;
            if n.is_multiple_of(POLL) && stop() {
                return Err(Failure::new("interrupted", ""));
            }
            let fnrm = unit(newell(&pts.iter().map(|&p| scaled(p)).collect::<Vec<_>>()));
            for &p in pts {
                let acc = vnormals.entry(key(p)).or_insert([0.0; 3]);
                let f = if dot(*acc, fnrm) < 0.0 { [-fnrm[0], -fnrm[1], -fnrm[2]] } else { fnrm };
                *acc = add(*acc, f, 1.0);
            }
        }
    }
    let mut elems = Vec::with_capacity(sc.elems.len());
    for e in sc.elems {
        n += 1;
        if n.is_multiple_of(POLL) && stop() {
            return Err(Failure::new("interrupted", ""));
        }
        let arrow = matches!(e.prim, Prim::Arrow(_));
        let prim = match e.prim {
            Prim::Face { pts: corners, smooth, color, inside } => {
                let pts = if clip { clip_polygon(corners.clone(), lo, hi) } else { corners.clone() };
                if pts.len() < 3 {
                    continue;
                }
                // Newell's normal in scaled coordinates
                let q: Vec<P3> = pts.iter().map(|&p| scaled(p)).collect();
                let mid = q.iter().fold([0.0; 3], |s, p| add(s, *p, 1.0 / q.len() as f64));
                let nrm = unit(newell(&q));
                // a face of a closed convex solid: its outward normal (the solid stays closed unless clipped)
                let outward = inside.filter(|_| !clip).map(|c| if dot(nrm, sub(mid, scaled(c))) < 0.0 { [-nrm[0], -nrm[1], -nrm[2]] } else { nrm });
                let vns = (smooth && !clip).then(|| corners.iter().map(|&p| vnormals.get(&key(p)).map(|&v| unit(v))).collect());
                PPrim::Face(PFace { pts, nrm, mid, vns, smooth, color, outward })
            }
            Prim::Line(pts) | Prim::Arrow(pts) => {
                let segs: Vec<(P3, P3)> =
                    pts.windows(2).filter_map(|w| if clip { clip_segment(w[0], w[1], lo, hi) } else { Some((w[0], w[1])) }).collect();
                PPrim::Segs { segs, arrow }
            }
            Prim::Points(pts) => {
                PPrim::Points(pts.into_iter().filter(|p| !(clip && (0..3).any(|k| p[k] < lo[k] || p[k] > hi[k]))).collect())
            }
            Prim::Text { label, at } => PPrim::Text { label, at },
        };
        elems.push((prim, e.style));
    }
    Ok(Prepared { elems, styles: sc.styles, o3, lo, hi, centre, scale })
}

/// Whether a style has the default colour still (faces then take the 3D default).
fn sc_default(st: &Style) -> bool {
    st.color == Style::default().color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_keeps_the_inside() {
        let sq = vec![[-1.0, -1.0, 0.0], [2.0, -1.0, 0.0], [2.0, 2.0, 0.0], [-1.0, 2.0, 0.0]];
        let c = clip_polygon(sq, [0.0; 3], [1.0; 3]);
        assert_eq!(c.len(), 4);
        assert!(c.iter().all(|p| p.iter().all(|v| (0.0..=1.0).contains(v))));
        assert_eq!(clip_segment([-1.0, 0.5, 0.5], [2.0, 0.5, 0.5], [0.0; 3], [1.0; 3]), Some(([0.0, 0.5, 0.5], [1.0, 0.5, 0.5])));
        assert_eq!(clip_segment([-1.0, 5.0, 0.5], [2.0, 5.0, 0.5], [0.0; 3], [1.0; 3]), None);
    }

    #[test]
    fn spheres_close_at_the_poles() {
        let mut v = vec![];
        sphere([0.0; 3], [1.0; 3], &mut v);
        assert_eq!(v.len(), ROUND * ROUND / 2);
        let Prim::Face { pts, .. } = &v[0] else { panic!() };
        assert_eq!(pts.len(), 3);
    }
}
