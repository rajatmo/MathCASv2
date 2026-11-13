//! Plots in three dimensions, built as `graphics3d` terms and drawn by mc-graphics' 3D renderer: `plot3d`
//! (surfaces z = f(x, y)), `parametric_plot3d` (surfaces and space curves), `spherical_plot3d`,
//! `revolution_plot3d`, `list_plot3d` (a matrix of heights, or scattered points) and `list_point_plot3d`.
//!
//! A surface is sampled on a grid (61 x 61 points unless `plot_points` says otherwise), drawn as lit quadrilaterals
//! with mesh lines along every fourth grid line (15 each way; `mesh: none` leaves them out, `mesh: n` draws n); a
//! point without a real value leaves a hole. Heights are clipped to a robust range (poles do not flatten the rest).
use crate::figure::robust;
use crate::{bad, finish_graphic, real};
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::{colour, Colour, Rgba};
use mc_graphics::gradient::Gradient;
use mc_numfn::{NumFn, Options, Var};

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    for (name, kind) in [
        ("plot3d", Kind::Plot),
        ("parametric_plot3d", Kind::Parametric),
        ("spherical_plot3d", Kind::Spherical),
        ("revolution_plot3d", Kind::Revolution),
        ("list_plot3d", Kind::List),
        ("list_point_plot3d", Kind::Points),
        ("list_line_plot3d", Kind::Lines),
    ] {
        crate::held::register(k, h, name, Box::new(Plot3 { name, kind }));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Plot,
    Parametric,
    Spherical,
    Revolution,
    List,
    Points,
    Lines,
}

struct Plot3 {
    name: &'static str,
    kind: Kind,
}

/// Surface colours: a light version of each colour of the 2D cycle (teal, terracotta, ochre, ...).
const SURFACES: [Rgba; 4] = [
    Rgba::rgb(0.55, 0.75, 0.77),
    Rgba::rgb(0.90, 0.62, 0.52),
    Rgba::rgb(0.93, 0.80, 0.52),
    Rgba::rgb(0.70, 0.58, 0.68),
];
/// Curves and points: the 2D colour cycle's first colours.
const CURVES: [Rgba; 3] = [Rgba::rgb(0.106, 0.369, 0.388), Rgba::rgb(0.757, 0.314, 0.180), Rgba::rgb(0.788, 0.604, 0.180)];

/// The options of a 3D plot.
struct Opts<'h> {
    /// options of the graphic passed on
    pass: Vec<(String, T<'h>)>,
    /// the plot range given: three ranges (x, y, z); each end `None` when automatic
    range: [[Option<f64>; 2]; 3],
    plot_points: Option<usize>,
    /// mesh lines each way (`Some(0)`: none)
    mesh: Option<usize>,
    gradient: Option<Gradient>,
    /// `plot_style:` per surface (cycled)
    styles: Vec<T<'h>>,
    ignored: Vec<String>,
}

fn is_sym(h: &Heap, t: T<'_>, name: &str) -> bool {
    matches!(h.view(t), View::Sym(s) if h.sym_name(s) == name)
}

/// Options computed and not drawn here: accepted without remark.
const SILENT: &[&str] = &[
    "max_recursion",
    "working_precision",
    "performance_goal",
    "evaluated",
    "method",
    "max_plot_points",
    "plot_theme",
    "lighting",
    "label_style",
    "base_style",
    "ticks_style",
    "axes_style",
    "boundary_style",
    "spherical_region",
    "view_angle",
    "view_vertical",
    "image_padding",
];

fn options<'h>(cx: &NativeCx<'_, '_, 'h>, o: T<'h>) -> Option<Opts<'h>> {
    let h = cx.heap();
    let View::App { head, args } = h.view(o) else { return None };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let num = |t: T<'h>| mc_graphics::num::number(h, t);
    let mut out = Opts { pass: vec![], range: [[None; 2]; 3], plot_points: None, mesh: None, gradient: None, styles: vec![], ignored: vec![] };
    for &r in args {
        let View::App { args: &[k, v], .. } = h.view(r) else { continue };
        let View::Sym(k) = h.view(k) else { continue };
        let key = h.sym_name(k);
        let ok: Option<bool> = (|| {
            match key.as_str() {
                "plot_range" => {
                    if is_sym(h, v, "all") || is_sym(h, v, "automatic") || is_sym(h, v, "full") {
                        return Some(true);
                    }
                    let one = |t: T<'h>| -> Option<[Option<f64>; 2]> {
                        if is_sym(h, t, "all") || is_sym(h, t, "automatic") || is_sym(h, t, "full") {
                            return Some([None; 2]);
                        }
                        let &[a, b] = cx.as_list(t)? else { return None };
                        let end = |e: T<'h>| if matches!(h.view(e), View::Sym(_)) { Some(None) } else { num(e).map(Some) };
                        Some([end(a)?, end(b)?])
                    };
                    if let Some(r) = num(v) {
                        out.range[2] = [Some(-r), Some(r)];
                        return Some(true);
                    }
                    match *cx.as_list(v)? {
                        [x, y, z] => out.range = [one(x)?, one(y)?, one(z)?],
                        [_, _] => out.range[2] = one(v)?,
                        _ => return None,
                    }
                }
                "plot_points" => {
                    let n = match h.view(v) {
                        View::Int(n) => n,
                        _ => match cx.as_list(v).and_then(|l| l.first().copied()).map(|e| h.view(e)) {
                            Some(View::Int(n)) => n,
                            _ => return None,
                        },
                    };
                    out.plot_points = Some(n.clamp(3, 200) as usize);
                }
                "mesh" => {
                    out.mesh = Some(if is_sym(h, v, "none") || cx.is_false(v) {
                        0
                    } else if let View::Int(n) = h.view(v) {
                        n.clamp(0, 100) as usize
                    } else {
                        // all, automatic, full, lists of positions: the default
                        15
                    })
                }
                "color_function" => {
                    out.gradient = match mc_graphics::gradient::read(h, v) {
                        Some(g) => Some(g),
                        None => match h.view(v) {
                            View::Str(s) => Some(mc_graphics::gradient::scheme(s)),
                            _ => return Some(false),
                        },
                    }
                }
                "plot_style" => {
                    out.styles = match cx.as_list(v) {
                        Some(items) => items.to_vec(),
                        None => vec![v],
                    }
                }
                k if mc_graphics::three::OPTIONS.contains(&k) || mc_graphics::model::known_option(k) => out.pass.push((key.clone(), v)),
                k if SILENT.contains(&k) => {}
                _ => return Some(false),
            }
            Some(true)
        })();
        match ok {
            Some(true) => {}
            Some(false) => out.ignored.push(key),
            None => out.ignored.push(format!("{key}: {}", mc_syntax::Printer::new(h).expr(v))),
        }
    }
    Some(out)
}

/// A grid of points in space, `nu x nv`, NaN where there is none.
struct Mesh {
    nu: usize,
    nv: usize,
    p: Vec<[f64; 3]>,
}

impl Mesh {
    fn at(&self, i: usize, j: usize) -> [f64; 3] {
        self.p[i * self.nv + j]
    }
    fn ok(&self, i: usize, j: usize) -> bool {
        self.at(i, j).iter().all(|v| v.is_finite())
    }
}

/// A compiled function of the given variables.
fn compile<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>, vars: &[Sym]) -> Res<Box<dyn NumFn<'h, f64> + 'h>> {
    let vs: Vec<Var> = vars.iter().map(|&v| Var::scalar(v)).collect();
    mc_numfn::compile_expr(cx, f, &vs, &Options::default())
}

/// The value of a compiled function at `at` (NaN where it has none).
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Box<dyn NumFn<'h, f64> + 'h>, at: &[f64]) -> Res<f64> {
    let mut out = [0.0];
    let ok = f.eval(cx, at, &mut out)?;
    Ok(if ok && out[0].is_finite() { out[0] } else { f64::NAN })
}

/// The variable and ends of an iterator `[v, a, b]`.
fn spec<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Sym, f64, f64)>> {
    let h = cx.heap();
    let Some(&[v, a, b]) = cx.as_list(t) else { return Ok(None) };
    let View::Sym(v) = h.view(v) else { return Ok(None) };
    let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else { return Ok(None) };
    Ok((a < b).then_some((v, a, b)))
}

/// Builds terms.
struct B<'h> {
    h: &'h Heap,
}

impl<'h> B<'h> {
    fn call(&self, f: &str, a: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(f), a)
    }
    fn list(&self, a: &[T<'h>]) -> T<'h> {
        self.call(mc_api::names::LIST, a)
    }
    fn rule(&self, k: &str, v: T<'h>) -> T<'h> {
        self.call(mc_api::names::RULE, &[self.h.sym_term(self.h.sym(k)), v])
    }
    fn rgb(&self, c: Rgba) -> T<'h> {
        mc_graphics_kernel::rgb_term(self.h, c)
    }
}

/// The content of a surface: the quadrilaterals of the grid whose corners all have values (coloured by height
/// through `grad`, or in `color`), then the mesh lines.
fn surface<'h>(b: &B<'h>, m: &Mesh, color: Rgba, grad: Option<(&Gradient, (f64, f64))>, mesh: usize, style: Option<T<'h>>) -> Vec<T<'h>> {
    let h = b.h;
    let mut quads = vec![];
    let mut colors = vec![];
    for i in 0..m.nu - 1 {
        for j in 0..m.nv - 1 {
            if !(m.ok(i, j) && m.ok(i + 1, j) && m.ok(i + 1, j + 1) && m.ok(i, j + 1)) {
                continue;
            }
            let c = [m.at(i, j), m.at(i + 1, j), m.at(i + 1, j + 1), m.at(i, j + 1)];
            quads.extend(c.iter().flatten());
            if let Some((g, (lo, hi))) = grad {
                for p in c {
                    let k = g.at(if hi > lo { (p[2] - lo) / (hi - lo) } else { 0.5 });
                    colors.extend_from_slice(&[k.r, k.g, k.b]);
                }
            }
        }
    }
    let n = quads.len() / 12;
    let mut out = vec![];
    if n > 0 {
        let pts = h.real_array(&[n, 4, 3], &quads);
        let poly = if grad.is_some() {
            let vc = h.real_array(&[n, 4, 3], &colors);
            b.call("polygon", &[pts, b.call("options", &[b.rule("vertex_colors", vc)])])
        } else {
            b.call("polygon", &[pts])
        };
        let mut part = vec![b.rgb(color)];
        part.extend(style);
        part.push(poly);
        out.push(b.list(&part));
    }
    if mesh > 0 {
        let mut runs = vec![];
        let every = |n: usize| ((n - 1) as f64 / (mesh + 1) as f64).max(1.0);
        let lines = |count: usize, len: usize, at: &dyn Fn(usize, usize) -> [f64; 3], ok: &dyn Fn(usize, usize) -> bool, runs: &mut Vec<T<'h>>| {
            let step = every(count);
            let mut k = step;
            while (k.round() as usize) < count - 1 {
                let i = k.round() as usize;
                let mut run: Vec<f64> = vec![];
                for j in 0..len {
                    if ok(i, j) {
                        run.extend_from_slice(&at(i, j));
                    } else if run.len() >= 6 {
                        runs.push(h.real_array(&[run.len() / 3, 3], &run));
                        run.clear();
                    } else {
                        run.clear();
                    }
                }
                if run.len() >= 6 {
                    runs.push(h.real_array(&[run.len() / 3, 3], &run));
                }
                k += step;
            }
        };
        lines(m.nu, m.nv, &|i, j| m.at(i, j), &|i, j| m.ok(i, j), &mut runs);
        lines(m.nv, m.nu, &|i, j| m.at(j, i), &|i, j| m.ok(j, i), &mut runs);
        if !runs.is_empty() {
            let pen = vec![
                b.rgb(Rgba { a: 0.45, ..Rgba::rgb(0.15, 0.15, 0.15) }),
                b.call("absolute_thickness", &[h.real(0.5)]),
                b.call("line", &[b.list(&runs)]),
            ];
            out.push(b.list(&pen));
        }
    }
    out
}

/// The robust extent of the heights of the meshes (poles clipped).
fn heights(ms: &[Mesh]) -> Option<(f64, f64)> {
    let zs: Vec<f64> = ms.iter().flat_map(|m| m.p.iter().map(|p| p[2])).filter(|z| z.is_finite()).collect();
    if zs.is_empty() {
        return None;
    }
    let (lo, hi) = robust(zs);
    Some(if hi > lo { (lo, hi) } else { (lo - 1.0, hi + 1.0) })
}

/// The graphics3d term of the content with the plot's options (axes on, ratios as given or `ratios`).
fn graphic<'h>(b: &B<'h>, content: Vec<T<'h>>, o: &Opts<'h>, ranges: Option<[[f64; 2]; 3]>, ratios: Option<[f64; 3]>) -> T<'h> {
    let h = b.h;
    let mut opts = vec![b.rule("axes", h.sym_term(h.sym("true")))];
    if let Some(r) = ratios {
        opts.push(b.rule("box_ratios", h.reals(&r)));
    }
    if let Some(rs) = ranges {
        let rs: Vec<T<'h>> = rs.iter().map(|r| h.reals(r)).collect();
        opts.push(b.rule("plot_range", b.list(&rs)));
    }
    for (k, v) in &o.pass {
        opts.retain(|r| !matches!(h.view(*r), View::App { args: &[n, _], .. } if matches!(h.view(n), View::Sym(s) if h.sym_name(s) == *k)));
        opts.push(b.rule(k, *v));
    }
    b.call("graphics3d", &[b.list(&content), b.call("options", &opts)])
}

/// The colour of `plot_style` entry `t`, if it names one.
fn style_color(h: &Heap, t: T<'_>) -> Option<Rgba> {
    let mut st = mc_graphics::model::Style::default();
    mc_graphics::model::styled(h, &[t], &mut st).ok()?;
    match colour(h, t) {
        Colour::Value(c) => Some(c),
        _ => (st.color != mc_graphics::model::Style::default().color).then_some(st.color),
    }
}

/// A grid size for the plot.
fn grid_n(o: &Opts) -> usize {
    o.plot_points.unwrap_or(61)
}

impl Native for Plot3 {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let (args, o) = match args.split_last() {
            Some((&o, rest)) if matches!(h.view(o), View::App { head, .. } if head == h.sym_term(h.sym("options"))) => {
                (rest, options(cx, o))
            }
            _ => (args, None),
        };
        let o = o.unwrap_or(Opts { pass: vec![], range: [[None; 2]; 3], plot_points: None, mesh: None, gradient: None, styles: vec![], ignored: vec![] });
        let b = B { h };
        // legended, labeled, callout and style around the functions or data (and around each of a list) are taken
        // off: a 3D plot draws neither legends nor labels
        let mut args = args.to_vec();
        if let Some(first) = args.first_mut() {
            *first = crate::strip_wrappers(cx, *first);
        }
        let args = args.as_slice();
        match self.kind {
            Kind::Plot => {
                let [f, sx, sy] = args else { return Ok(Reply::Decline) };
                let Some((x, a, bx)) = spec(cx, *sx)? else { return Ok(bad(h, name, 2)) };
                let Some((y, c, d)) = spec(cx, *sy)? else { return Ok(bad(h, name, 3)) };
                let fs: Vec<T<'h>> = cx.as_list(*f).map(|l| l.to_vec()).unwrap_or_else(|| vec![*f]);
                let n = grid_n(&o);
                let mut meshes = vec![];
                for &f in &fs {
                    let mut fun = compile(cx, f, &[x, y])?;
                    let mut p = Vec::with_capacity(n * n);
                    for i in 0..n {
                        cx.tick(1)?;
                        let xi = a + (bx - a) * i as f64 / (n - 1) as f64;
                        for j in 0..n {
                            let yj = c + (d - c) * j as f64 / (n - 1) as f64;
                            p.push([xi, yj, value(cx, &mut fun, &[xi, yj])?]);
                        }
                    }
                    meshes.push(Mesh { nu: n, nv: n, p });
                }
                let Some(auto) = heights(&meshes) else { return Ok(bad(h, name, 1)) };
                let z = [o.range[2][0].unwrap_or(auto.0), o.range[2][1].unwrap_or(auto.1)];
                let z = if z[1] > z[0] { z } else { [auto.0, auto.1] };
                let content = self.surfaces(&b, &meshes, &o, (z[0], z[1]));
                let ranges = [[a, bx], [c, d], z];
                finish_graphic(cx, graphic(&b, content, &o, Some(ranges), Some([1.0, 1.0, 0.4])), name, &o.ignored)
            }
            Kind::Parametric | Kind::Spherical | Kind::Revolution => {
                let (f, su, sv) = match (self.kind, args) {
                    (_, [f, su, sv]) => (*f, *su, Some(*sv)),
                    (Kind::Parametric | Kind::Revolution, [f, su]) => (*f, *su, None),
                    _ => return Ok(Reply::Decline),
                };
                let Some((u, ua, ub)) = spec(cx, su)? else { return Ok(bad(h, name, 2)) };
                let v = match sv {
                    Some(sv) => match spec(cx, sv)? {
                        Some(v) => Some(v),
                        None => return Ok(bad(h, name, 3)),
                    },
                    None => None,
                };
                // the components: [fx, fy, fz] (several: a list of them) or a radius / a height
                let comps: Vec<Vec<T<'h>>> = match self.kind {
                    Kind::Parametric => {
                        let Some(items) = cx.as_list(f) else { return Ok(bad(h, name, 1)) };
                        if items.len() == 3 && items.iter().all(|&e| cx.as_list(e).is_none()) {
                            vec![items.to_vec()]
                        } else {
                            let mut all = vec![];
                            for &e in items {
                                match cx.as_list(e) {
                                    Some(c) if c.len() == 3 => all.push(c.to_vec()),
                                    _ => return Ok(bad(h, name, 1)),
                                }
                            }
                            all
                        }
                    }
                    _ => cx.as_list(f).map(|l| l.iter().map(|&e| vec![e]).collect()).unwrap_or_else(|| vec![vec![f]]),
                };
                let n = grid_n(&o);
                let mut meshes = vec![];
                let mut curves = vec![];
                for comp in comps {
                    let vars: Vec<Sym> = match (self.kind, v) {
                        (Kind::Revolution, Some((v, ..))) => vec![u, v],
                        (Kind::Revolution, None) => vec![u],
                        (_, Some((v, ..))) => vec![u, v],
                        _ => vec![u],
                    };
                    let mut funs = vec![];
                    for &e in &comp {
                        funs.push(compile(cx, e, &vars)?);
                    }
                    // the point at parameters (s, t)
                    let mut point = |cx: &mut NativeCx<'_, '_, 'h>, s: f64, t: f64| -> Res<[f64; 3]> {
                        Ok(match self.kind {
                            Kind::Parametric => {
                                let at: Vec<f64> = if vars.len() == 2 { vec![s, t] } else { vec![s] };
                                [value(cx, &mut funs[0], &at)?, value(cx, &mut funs[1], &at)?, value(cx, &mut funs[2], &at)?]
                            }
                            Kind::Spherical => {
                                // radius at polar angle s and azimuth t
                                let r = value(cx, &mut funs[0], &[s, t])?;
                                [r * s.sin() * t.cos(), r * s.sin() * t.sin(), r * s.cos()]
                            }
                            _ => {
                                // the curve z = f(s) at distance s from the axis, turned by t
                                let z = if vars.len() == 2 { value(cx, &mut funs[0], &[s, t])? } else { value(cx, &mut funs[0], &[s])? };
                                [s * t.cos(), s * t.sin(), z]
                            }
                        })
                    };
                    let (va, vb) = match (self.kind, v) {
                        (Kind::Revolution, None) => (0.0, 2.0 * std::f64::consts::PI),
                        (_, Some((_, a, b))) => (a, b),
                        _ => (0.0, 0.0),
                    };
                    if v.is_none() && self.kind == Kind::Parametric {
                        // a space curve
                        let m = 400;
                        let mut pts = Vec::with_capacity(m);
                        for i in 0..m {
                            if i % 64 == 0 {
                                cx.tick(1)?;
                            }
                            let s = ua + (ub - ua) * i as f64 / (m - 1) as f64;
                            pts.push(point(cx, s, 0.0)?);
                        }
                        curves.push(pts);
                        continue;
                    }
                    let mut p = Vec::with_capacity(n * n);
                    for i in 0..n {
                        cx.tick(1)?;
                        let s = ua + (ub - ua) * i as f64 / (n - 1) as f64;
                        for j in 0..n {
                            let t = va + (vb - va) * j as f64 / (n - 1) as f64;
                            p.push(point(cx, s, t)?);
                        }
                    }
                    meshes.push(Mesh { nu: n, nv: n, p });
                }
                let mut content = vec![];
                if !meshes.is_empty() {
                    let z = heights(&meshes).unwrap_or((0.0, 1.0));
                    content = self.surfaces(&b, &meshes, &o, z);
                }
                for (k, pts) in curves.iter().enumerate() {
                    let mut runs = vec![];
                    let mut run: Vec<f64> = vec![];
                    for p in pts {
                        if p.iter().all(|v| v.is_finite()) {
                            run.extend_from_slice(p);
                        } else {
                            if run.len() >= 6 {
                                runs.push(h.real_array(&[run.len() / 3, 3], &run));
                            }
                            run.clear();
                        }
                    }
                    if run.len() >= 6 {
                        runs.push(h.real_array(&[run.len() / 3, 3], &run));
                    }
                    let color = o.styles.get(k % o.styles.len().max(1)).and_then(|&s| style_color(h, s)).unwrap_or(CURVES[k % CURVES.len()]);
                    content.push(b.list(&[b.rgb(color), b.call("absolute_thickness", &[h.real(1.5)]), b.call("line", &[b.list(&runs)])]));
                }
                if content.is_empty() {
                    return Ok(bad(h, name, 1));
                }
                let ranges = (o.range.iter().flatten().any(Option::is_some)).then(|| {
                    let all: Vec<[f64; 3]> = meshes.iter().flat_map(|m| m.p.iter().copied()).chain(curves.iter().flatten().copied()).filter(|p| p.iter().all(|v| v.is_finite())).collect();
                    let mut r = [[0.0, 1.0]; 3];
                    for (k, rk) in r.iter_mut().enumerate() {
                        let lo = all.iter().map(|p| p[k]).fold(f64::INFINITY, f64::min);
                        let hi = all.iter().map(|p| p[k]).fold(f64::NEG_INFINITY, f64::max);
                        *rk = [o.range[k][0].unwrap_or(lo), o.range[k][1].unwrap_or(hi)];
                    }
                    r
                });
                finish_graphic(cx, graphic(&b, content, &o, ranges, None), name, &o.ignored)
            }
            Kind::List | Kind::Points | Kind::Lines => {
                let [data] = args else { return Ok(Reply::Decline) };
                let data = &unlabelled(cx, *data);
                // several data sets of points (lists of triples, or matrices of heights): each in its colour
                if self.kind != Kind::List {
                    if let Some(sets) = cx.as_list(*data).filter(|items| items.len() > 1 && items.iter().all(|&e| depth3(cx, e))) {
                        let sets = sets.to_vec();
                        let mut content = vec![];
                        for (k, set) in sets.into_iter().enumerate() {
                            let Some(pts) = points3(cx, set)? else { return Ok(bad(h, name, 1)) };
                            let arr = h.real_array(&[pts.len() / 3, 3], &pts);
                            let prim = if self.kind == Kind::Points { b.call("point", &[arr]) } else { b.call("line", &[arr]) };
                            content.push(b.list(&[b.rgb(CURVES[k % CURVES.len()]), b.call("absolute_point_size", &[h.real(4.0)]), b.call("absolute_thickness", &[h.real(1.5)]), prim]));
                        }
                        return finish_graphic(cx, graphic(&b, content, &o, None, None), name, &o.ignored);
                    }
                }
                let Some(rows) = cx.as_list(*data) else { return Ok(bad(h, name, 1)) };
                // [[x, y, z], ...] points, or a matrix of heights (rows along y)
                let mut table: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
                for (i, &r) in rows.iter().enumerate() {
                    if i % 4096 == 0 {
                        cx.tick(1)?;
                    }
                    let Some(items) = cx.as_list(unlabelled(cx, r)) else { return Ok(bad(h, name, 1)) };
                    let mut vals = Vec::with_capacity(items.len());
                    for &e in items {
                        vals.push(data_value(cx, e)?.unwrap_or(f64::NAN));
                    }
                    table.push(vals);
                }
                // points when every row is a triple (a 3 x 3 list is a matrix of heights for list_plot3d)
                let points = table.iter().all(|r| r.len() == 3) && (self.kind != Kind::List || table.len() > 3);
                let (triples, matrix): (Vec<[f64; 3]>, Vec<Vec<f64>>) =
                    if points { (table.iter().map(|r| [r[0], r[1], r[2]]).collect(), vec![]) } else { (vec![], table) };
                let mut content = vec![];
                match self.kind {
                    Kind::Points | Kind::Lines => {
                        let pts: Vec<f64> = if triples.is_empty() {
                            // heights of a matrix as points
                            matrix.iter().enumerate().flat_map(|(j, r)| r.iter().enumerate().map(move |(i, &z)| [i as f64 + 1.0, j as f64 + 1.0, z])).filter(|p| p[2].is_finite()).flatten().collect()
                        } else {
                            triples.iter().filter(|p| p.iter().all(|v| v.is_finite())).flatten().copied().collect()
                        };
                        if pts.is_empty() {
                            return Ok(bad(h, name, 1));
                        }
                        let arr = h.real_array(&[pts.len() / 3, 3], &pts);
                        let prim = if self.kind == Kind::Points { b.call("point", &[arr]) } else { b.call("line", &[arr]) };
                        content.push(b.list(&[b.rgb(CURVES[0]), b.call("absolute_point_size", &[h.real(4.0)]), b.call("absolute_thickness", &[h.real(1.5)]), prim]));
                    }
                    _ => {
                        let mesh = if triples.is_empty() {
                            let ny = matrix.len();
                            let nx = matrix.first().map_or(0, |r| r.len());
                            if ny < 2 || nx < 2 || matrix.iter().any(|r| r.len() != nx) {
                                return Ok(bad(h, name, 1));
                            }
                            let mut p = Vec::with_capacity(nx * ny);
                            for i in 0..nx {
                                for (j, row) in matrix.iter().enumerate() {
                                    p.push([i as f64 + 1.0, j as f64 + 1.0, row[i]]);
                                }
                            }
                            Mesh { nu: nx, nv: ny, p }
                        } else {
                            match scattered(cx, &triples)? {
                                Some(m) => m,
                                None => return Ok(bad(h, name, 1)),
                            }
                        };
                        let z = heights(std::slice::from_ref(&mesh)).unwrap_or((0.0, 1.0));
                        content = self.surfaces(&b, &[mesh], &o, z);
                    }
                }
                let ratios = (self.kind == Kind::List).then_some([1.0, 1.0, 0.4]);
                finish_graphic(cx, graphic(&b, content, &o, None, ratios), name, &o.ignored)
            }
        }
    }
}

/// A data value of a 3D point or line plot as a machine real: a number, the centre of `around(x, dx)`, or the value of
/// a labelled one (`labeled(v, l)`, `callout(v, l)`, `v -> l`; the labels are not drawn).
fn data_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    if let View::App { head, args } = h.view(t) {
        if let View::Sym(s) = h.view(head) {
            let name = h.sym_name(s);
            match (name.as_str(), args) {
                ("around", &[x, _]) | ("labeled" | "callout", &[x, _, ..]) | (mc_api::names::RULE, &[x, _]) => return data_value(cx, x),
                _ => {}
            }
        }
    }
    real(cx, t)
}

/// The data of a 3D point or line plot without the labels of `data -> labels` (the labels are not drawn).
fn unlabelled<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> T<'h> {
    let h = cx.heap();
    match h.view(t) {
        View::App { head, args: &[d, _] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == mc_api::names::RULE) && cx.as_list(d).is_some() => d,
        _ => t,
    }
}

/// Whether `t` is a list of lists of lists or of numbers in rows (a list of triples, or a matrix): one data set of a
/// 3D point or line plot.
fn depth3<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
    cx.as_list(t).is_some_and(|rows| !rows.is_empty() && rows.iter().all(|&r| cx.as_list(r).is_some_and(|items| items.iter().all(|&e| cx.as_list(e).is_none()))))
}

/// The points of one 3D data set as flat x, y, z reals: `[[x, y, z], ...]`, or a matrix of heights (z at x = column,
/// y = row); points without three finite coordinates left out. `None` when nothing is left.
fn points3<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<Vec<f64>>> {
    let Some(rows) = cx.as_list(t).map(|r| r.to_vec()) else { return Ok(None) };
    let mut table: Vec<Vec<f64>> = Vec::with_capacity(rows.len());
    for (i, r) in rows.into_iter().enumerate() {
        if i % 4096 == 0 {
            cx.tick(1)?;
        }
        let Some(items) = cx.as_list(unlabelled(cx, r)).map(|l| l.to_vec()) else { return Ok(None) };
        let mut vals = Vec::with_capacity(items.len());
        for e in items {
            vals.push(data_value(cx, e)?.unwrap_or(f64::NAN));
        }
        table.push(vals);
    }
    let pts: Vec<f64> = if table.iter().all(|r| r.len() == 3) {
        table.iter().filter(|p| p.iter().all(|v| v.is_finite())).flatten().copied().collect()
    } else {
        table.iter().enumerate().flat_map(|(j, r)| r.iter().enumerate().map(move |(i, &z)| [i as f64 + 1.0, j as f64 + 1.0, z])).filter(|p| p[2].is_finite()).flatten().collect()
    };
    Ok((!pts.is_empty()).then_some(pts))
}

impl Plot3 {
    /// The content of the surfaces, each in its colour (or by height through the colour function).
    fn surfaces<'h>(&self, b: &B<'h>, meshes: &[Mesh], o: &Opts<'h>, z: (f64, f64)) -> Vec<T<'h>> {
        let mut content = vec![];
        let mesh = o.mesh.unwrap_or(if meshes.len() > 2 { 0 } else { 15 });
        for (k, m) in meshes.iter().enumerate() {
            let style = (!o.styles.is_empty()).then(|| o.styles[k % o.styles.len()]);
            let color = style.and_then(|s| style_color(b.h, s)).unwrap_or(SURFACES[k % SURFACES.len()]);
            let grad = o.gradient.as_ref().map(|g| (g, z));
            content.extend(surface(b, m, color, grad, mesh, style));
        }
        content
    }
}

/// Scattered points `[x, y, z]` as a surface: heights on a 41 x 41 grid over their extent, each the
/// inverse-distance-weighted mean of the nearest points (grid points far from every point are left out).
fn scattered(cx: &mut NativeCx<'_, '_, '_>, pts: &[[f64; 3]]) -> Res<Option<Mesh>> {
    let pts: Vec<[f64; 3]> = pts.iter().copied().filter(|p| p.iter().all(|v| v.is_finite())).collect();
    if pts.len() < 3 {
        return Ok(None);
    }
    let ext = |k: usize| pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| (a.min(p[k]), b.max(p[k])));
    let ((x0, x1), (y0, y1)) = (ext(0), ext(1));
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    let n = 41;
    // the typical spacing of the points: grid points farther than twice it from all of them have no value
    let spacing = ((x1 - x0) * (y1 - y0) / pts.len() as f64).sqrt();
    let mut p = Vec::with_capacity(n * n);
    for i in 0..n {
        cx.tick(1)?;
        let x = x0 + (x1 - x0) * i as f64 / (n - 1) as f64;
        for j in 0..n {
            let y = y0 + (y1 - y0) * j as f64 / (n - 1) as f64;
            let (mut wsum, mut zsum, mut nearest) = (0.0, 0.0, f64::INFINITY);
            let mut exact = None;
            for q in &pts {
                let d2 = ((q[0] - x) / (x1 - x0)).powi(2) + ((q[1] - y) / (y1 - y0)).powi(2);
                let d = ((q[0] - x).powi(2) + (q[1] - y).powi(2)).sqrt();
                nearest = nearest.min(d);
                if d2 < 1e-24 {
                    exact = Some(q[2]);
                    break;
                }
                let w = 1.0 / (d2 * d2);
                wsum += w;
                zsum += w * q[2];
            }
            let z = match exact {
                Some(z) => z,
                None if nearest <= 2.0 * spacing => zsum / wsum,
                None => f64::NAN,
            };
            p.push([x, y, z]);
        }
    }
    Ok(Some(Mesh { nu: n, nv: n, p }))
}
