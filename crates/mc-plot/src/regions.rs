//! Regions and complex functions as pictures: `region_plot` (where conditions hold, filled, with their
//! boundaries; also a region term), `complex_region_plot` (conditions on a complex variable), and
//! `complex_contour_plot` (contours of a real function of z, or the curves where the real and imaginary parts of
//! a complex one take level values).
//!
//! A condition is sampled as its margin (`mc_geom::margin`: a real function at least 0 where it holds, so that
//! the boundary is found between grid points by interpolation); conditions of other forms are sampled as 1 where
//! they hold and -1 elsewhere.
use crate::fields::{gradient, level_lines, levels, region, sample_grid, shade, spec, Grid};
use crate::figure::{Extra, Look};
use crate::more::split;
use crate::{bad, finish, finish_graphic, plot_options};
use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};
use mc_graphics::color::Rgba;

/// Register the natives.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    crate::held::register(k, h, "region_plot", Box::new(RegionPlot { complex: false }));
    crate::held::register(k, h, "complex_region_plot", Box::new(RegionPlot { complex: true }));
    crate::held::register(k, h, "complex_contour_plot", Box::new(ComplexContour));
}

fn list<'h>(h: &'h Heap, items: &[T<'h>]) -> T<'h> {
    h.call(h.sym(names::LIST), items)
}

/// The value function of a condition (margin, or +-1).
fn margin_of<'h>(h: &'h Heap, c: T<'h>) -> T<'h> {
    mc_geom::margin(h, c).unwrap_or_else(|| h.call(h.sym("if"), &[c, h.int(1), h.int(-1)]))
}

/// A light shade of a series colour for filled regions (opaque, so that the cells of a region join without
/// seams).
fn light((r, g, b): (u8, u8, u8)) -> Rgba {
    let m = |c: u8| 1.0 - 0.45 * (1.0 - c as f64 / 255.0);
    Rgba::rgb(m(r), m(g), m(b))
}

fn rgb((r, g, b): (u8, u8, u8)) -> Rgba {
    Rgba::rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0)
}

fn poly_term<'h>(h: &'h Heap, rings: &[Vec<(f64, f64)>]) -> T<'h> {
    let rs: Vec<T<'h>> = rings
        .iter()
        .map(|r| {
            let flat: Vec<f64> = r.iter().flat_map(|&(x, y)| [x, y]).collect();
            h.real_array(&[r.len(), 2], &flat)
        })
        .collect();
    h.call(h.sym("polygon"), &[list(h, &rs)])
}

fn lines_term<'h>(h: &'h Heap, ls: &[Vec<(f64, f64)>]) -> T<'h> {
    let rs: Vec<T<'h>> = ls
        .iter()
        .map(|l| {
            let flat: Vec<f64> = l.iter().flat_map(|&(x, y)| [x, y]).collect();
            h.real_array(&[l.len(), 2], &flat)
        })
        .collect();
    h.call(h.sym("line"), &[list(h, &rs)])
}

/// `x + i y` for the complex variable.
fn complex_point<'h>(h: &'h Heap, x: T<'h>, y: T<'h>) -> T<'h> {
    let i = h.call(h.sym(names::COMPLEX), &[h.int(0), h.int(1)]);
    h.call(h.sym(names::ADD), &[x, h.call(h.sym(names::MUL), &[i, y])])
}

fn rewrite<'h>(h: &'h Heap, f: T<'h>, z: T<'h>, by: T<'h>) -> T<'h> {
    h.call(h.sym("rewrite"), &[f, h.call(h.sym(names::RULE), &[z, by])])
}

/// The rectangle of a complex range `[z, z0, z1]`: the variable and (x0, x1, y0, y1).
fn complex_range<'h>(cx: &mut NativeCx<'_, '_, 'h>, s: T<'h>) -> Res<Option<(T<'h>, [f64; 4])>> {
    let h = cx.heap();
    let s = crate::complex_spec(cx, s)?;
    let Some(&[z, lo, hi]) = cx.as_list(s) else { return Ok(None) };
    if !matches!(h.view(z), View::Sym(_)) {
        return Ok(None);
    }
    let part = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<Option<(f64, f64)>> {
        let v = cx.eval(h.call(h.sym("n"), &[t]))?;
        Ok(match h.view(v) {
            View::Int(a) => Some((a as f64, 0.0)),
            View::Real(a) => Some((a, 0.0)),
            View::App { head, args: &[a, b] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::COMPLEX) => {
                mc_graphics::num::number(h, a).zip(mc_graphics::num::number(h, b))
            }
            _ => None,
        })
    };
    let (Some(a), Some(b)) = (part(cx, lo)?, part(cx, hi)?) else { return Ok(None) };
    let r = [a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1)];
    Ok((r[1] > r[0] && r[3] > r[2]).then_some((z, r)))
}

/// Fresh coordinate symbols of the complex plane.
fn plane(h: &Heap) -> (Sym, Sym) {
    (h.sym("plot$x"), h.sym("plot$y"))
}

/// `region_plot(cond, [x, a, b], [y, c, d])`, `region_plot([c1, c2, ...], ...)`, `region_plot(reg)`;
/// `complex_region_plot(cond, [z, z0, z1])`.
struct RegionPlot {
    complex: bool,
}

impl Native for RegionPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = if self.complex { "complex_region_plot" } else { "region_plot" };
        let (a, o) = split(h, args);
        let base = Look { exact: true, size: Some((460.0, 440.0)), ..Look::default() };
        let mut look = match o {
            Some(o) => match plot_options(cx, o, base, &["plot_points", "bounding_style", "mesh", "max_recursion"], name) {
                Some(l) => l,
                None => return Ok(bad(h, name, a.len() as i64 + 1)),
            },
            None => base,
        };
        let (conds, x, y, rect) = match (self.complex, a.as_slice()) {
            (false, [r]) => return region_term(cx, *r, look, o),
            (false, [c, sx, sy]) => {
                let Some((x, a0, b0)) = spec(cx, *sx)? else { return Ok(bad(h, name, 2)) };
                let Some((y, c0, d0)) = spec(cx, *sy)? else { return Ok(bad(h, name, 3)) };
                let cs: Vec<T<'h>> = cx.as_list(*c).map(|l| l.to_vec()).unwrap_or_else(|| vec![*c]);
                // labeled, legended and styled conditions: the condition, with what the wrapper gives
                let (cs, w) = crate::unwrap_all(h, &cs);
                w.apply(cx, &mut look);
                (cs, x, y, [a0, b0, c0, d0])
            }
            (true, [c, s]) => {
                let Some((z, r)) = complex_range(cx, *s)? else { return Ok(bad(h, name, 2)) };
                let (x, y) = plane(h);
                let zz = complex_point(h, h.sym_term(x), h.sym_term(y));
                let cs: Vec<T<'h>> = cx.as_list(*c).map(|l| l.to_vec()).unwrap_or_else(|| vec![*c]);
                // labeled, legended and styled conditions: the condition, with what the wrapper gives
                let (cs, w) = crate::unwrap_all(h, &cs);
                w.apply(cx, &mut look);
                (cs.into_iter().map(|c| rewrite(h, c, z, zz)).collect(), x, y, r)
            }
            _ => return Ok(Reply::Decline),
        };
        region_picture(cx, &conds, x, y, rect, look)
    }
}

/// The picture of the regions where each condition holds, over the rectangle.
fn region_picture<'h>(cx: &mut NativeCx<'_, '_, 'h>, conds: &[T<'h>], x: Sym, y: Sym, [a, b, c, d]: [f64; 4], look: Look<'h>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let n = look.plot_points.unwrap_or(120).clamp(10, 600);
    let xs: Vec<f64> = (0..n).map(|i| a + (b - a) * i as f64 / (n - 1) as f64).collect();
    let ys: Vec<f64> = (0..n).map(|j| c + (d - c) * j as f64 / (n - 1) as f64).collect();
    let mut under = vec![];
    let mut edges = vec![];
    for (k, &cond) in conds.iter().enumerate() {
        let m = margin_of(h, cond);
        let grid: Grid = sample_grid(cx, m, x, y, &xs, &ys)?;
        let rings = region(&grid, 0.0);
        if rings.is_empty() {
            continue;
        }
        let col = look.color(k);
        under.push(list(h, &[mc_graphics_kernel::rgb_term(h, light(col)), poly_term(h, &rings)]));
        let ls = level_lines(&grid, 0.0);
        if !ls.is_empty() {
            edges.push(list(h, &[mc_graphics_kernel::rgb_term(h, rgb(col)), h.call(h.sym("absolute_thickness"), &[h.real(1.2)]), lines_term(h, &ls)]));
        }
    }
    under.extend(edges);
    let extra = Extra { under, bounds: Some([(a, b), (c, d)]), entries: vec![] };
    finish(cx, &[], &look, extra)
}

/// `region_plot(reg)`: an implicit region over its bounds; other regions drawn as their primitives, filled.
fn region_term<'h>(cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>, look: Look<'h>, o: Option<T<'h>>) -> Res<Reply<'h>> {
    let h = cx.heap();
    let Some(reg) = mc_geom::read(cx, r) else { return Ok(bad(h, "region_plot", 1)) };
    // the bounds, padded
    let b = cx.eval(h.call(h.sym("region_bounds"), &[r]))?;
    let mut bounds = vec![];
    if let Some(rows) = cx.as_list(b) {
        for &row in rows {
            let Some(&[lo, hi]) = cx.as_list(row) else { return Ok(bad(h, "region_plot", 1)) };
            let (Some(lo), Some(hi)) = (crate::real(cx, lo)?, crate::real(cx, hi)?) else { return Ok(bad(h, "region_plot", 1)) };
            bounds.push((lo, hi));
        }
    }
    if bounds.len() != 2 {
        if bounds.len() == 3 {
            // a solid: shown in space
            let g = h.call(h.sym("graphics3d"), &[list(h, &[mc_graphics_kernel::rgb_term(h, light(look.color(0))), r])]);
            return finish_graphic(cx, g, "region_plot", &[]);
        }
        return Ok(bad(h, "region_plot", 1));
    }
    let pad = |(lo, hi): (f64, f64)| {
        let w = (hi - lo).abs().max(1e-9);
        (lo - 0.05 * w, hi + 0.05 * w)
    };
    let (bx, by) = (pad(bounds[0]), pad(bounds[1]));
    match reg {
        mc_geom::Region::Implicit { cond, vars } => {
            let (View::Sym(x), View::Sym(y)) = (h.view(vars[0]), h.view(vars[1])) else { return Ok(bad(h, "region_plot", 1)) };
            let rect = match look.range {
                [[Some(a), Some(b)], [Some(c), Some(d)]] => [a, b, c, d],
                _ => [bx.0, bx.1, by.0, by.1],
            };
            if let Some((n, args)) = mc_geom::region::call_parts(h, cond) {
                if n == "equal" && args.len() == 2 {
                    // a curve: its level line
                    let f = h.call(h.sym(names::ADD), &[args[0], h.call(h.sym(names::MUL), &[h.int(-1), args[1]])]);
                    let n = look.plot_points.unwrap_or(160);
                    let xs: Vec<f64> = (0..n).map(|i| rect[0] + (rect[1] - rect[0]) * i as f64 / (n - 1) as f64).collect();
                    let ys: Vec<f64> = (0..n).map(|j| rect[2] + (rect[3] - rect[2]) * j as f64 / (n - 1) as f64).collect();
                    let grid = sample_grid(cx, f, x, y, &xs, &ys)?;
                    let ls = level_lines(&grid, 0.0);
                    let under = vec![list(h, &[mc_graphics_kernel::rgb_term(h, rgb(look.color(0))), h.call(h.sym("absolute_thickness"), &[h.real(1.6)]), lines_term(h, &ls)])];
                    let extra = Extra { under, bounds: Some([(rect[0], rect[1]), (rect[2], rect[3])]), entries: vec![] };
                    return finish(cx, &[], &look, extra);
                }
            }
            region_picture(cx, &[cond], x, y, rect, look)
        }
        _ => {
            let _ = o;
            let face = h.call(h.sym("face_form"), &[mc_graphics_kernel::rgb_term(h, light(look.color(0)))]);
            let edge = h.call(h.sym("edge_form"), &[mc_graphics_kernel::rgb_term(h, rgb(look.color(0)))]);
            let col = mc_graphics_kernel::rgb_term(h, rgb(look.color(0)));
            let under = vec![list(h, &[col, face, edge, r])];
            let extra = Extra { under, bounds: Some([bx, by]), entries: vec![] };
            finish(cx, &[], &look, extra)
        }
    }
}

/// `complex_contour_plot(f, [z, z0, z1])`: a real f as a filled contour plot over the rectangle; a complex f as
/// the level lines of its real part (one colour) and its imaginary part (another); `f == g` as the curves where
/// the real and the imaginary part of f - g vanish.
struct ComplexContour;

impl Native for ComplexContour {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = "complex_contour_plot";
        let (a, o) = split(h, args);
        let [f, s] = a.as_slice() else { return Ok(Reply::Decline) };
        let Some((z, [x0, x1, y0, y1])) = complex_range(cx, *s)? else { return Ok(bad(h, name, 2)) };
        let (x, y) = plane(h);
        let zz = complex_point(h, h.sym_term(x), h.sym_term(y));
        let base = Look { exact: true, size: Some((460.0, 440.0)), ..Look::default() };
        let look = match o {
            Some(o) => match plot_options(cx, o, base, &["plot_points", "contours", "color_function", "color_bar"], name) {
                Some(l) => l,
                None => return Ok(bad(h, name, 3)),
            },
            None => base,
        };
        let n = look.plot_points.unwrap_or(90).clamp(10, 400);
        let xs: Vec<f64> = (0..n).map(|i| x0 + (x1 - x0) * i as f64 / (n - 1) as f64).collect();
        let ys: Vec<f64> = (0..n).map(|j| y0 + (y1 - y0) * j as f64 / (n - 1) as f64).collect();
        let items: Vec<T<'h>> = cx.as_list(*f).map(|l| l.to_vec()).unwrap_or_else(|| vec![*f]);
        let re = |t: T<'h>| h.call(h.sym("re"), &[t]);
        let im = |t: T<'h>| h.call(h.sym("im_part"), &[t]);
        // equations: the curves of their real and imaginary parts
        let eqs: Option<Vec<T<'h>>> = items
            .iter()
            .map(|&e| match mc_geom::region::call_parts(h, e) {
                Some((n, a)) if n == "equal" && a.len() == 2 => Some(h.call(h.sym(names::ADD), &[a[0], h.call(h.sym(names::MUL), &[h.int(-1), a[1]])])),
                _ => None,
            })
            .collect();
        let mut under = vec![];
        let line_of = |h: &'h Heap, col: Rgba, ls: &[Vec<(f64, f64)>]| {
            list(h, &[mc_graphics_kernel::rgb_term(h, col), h.call(h.sym("absolute_thickness"), &[h.real(1.4)]), lines_term(h, ls)])
        };
        let palette = [rgb(look.color(0)), rgb(look.color(1)), rgb(look.color(2)), rgb(look.color(3))];
        if let Some(eqs) = eqs {
            for (k, g) in eqs.into_iter().enumerate() {
                let g = rewrite(h, g, z, zz);
                for (part, col) in [(re(g), palette[(2 * k) % 4]), (im(g), palette[(2 * k + 1) % 4])] {
                    let grid = sample_grid(cx, part, x, y, &xs, &ys)?;
                    let ls = level_lines(&grid, 0.0);
                    if !ls.is_empty() {
                        under.push(line_of(h, col, &ls));
                    }
                }
            }
            let extra = Extra { under, bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
            return finish(cx, &[], &look, extra);
        }
        let g = rewrite(h, items[0], z, zz);
        // real-valued (the imaginary part vanishes at the samples): filled contours of the function
        let ig = sample_grid(cx, im(g), x, y, &xs[..xs.len().min(12)], &ys[..ys.len().min(12)])?;
        let rg = sample_grid(cx, re(g), x, y, &xs, &ys)?;
        let real_valued = ig.values().iter().all(|v| !v.is_finite() || v.abs() <= 1e-12);
        let Some(extent) = rg.extent() else { return Ok(bad(h, name, 1)) };
        if real_valued && items.len() == 1 {
            let grad = gradient(&look);
            let ls = levels(&look.contours, extent);
            let mut bounds = vec![extent.0];
            bounds.extend(&ls);
            bounds.push(extent.1);
            for k in 0..=ls.len() {
                cx.tick(1)?;
                let c = if k == 0 { f64::NEG_INFINITY } else { ls[k - 1] };
                let rings = region(&rg, c);
                if !rings.is_empty() {
                    let col = shade(&grad, extent, 0.5 * (bounds[k] + bounds[k + 1]));
                    under.push(list(h, &[mc_graphics_kernel::rgb_term(h, col), poly_term(h, &rings)]));
                }
            }
            let mut lines = vec![];
            for &c in &ls {
                lines.extend(level_lines(&rg, c));
            }
            if !lines.is_empty() {
                under.push(list(h, &[h.call(h.sym("gray_level"), &[h.real(0.15), h.real(0.55)]), h.call(h.sym("absolute_thickness"), &[h.real(0.75)]), lines_term(h, &lines)]));
            }
            let mut look = look;
            look.bar_range = Some(extent);
            let extra = Extra { under, bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
            return finish(cx, &[], &look, extra);
        }
        // complex: level lines of the real part and of the imaginary part
        let ig = sample_grid(cx, im(g), x, y, &xs, &ys)?;
        for (grid, col) in [(&rg, palette[0]), (&ig, palette[1])] {
            let Some(ext) = grid.extent() else { continue };
            let mut ls = vec![];
            for c in levels(&look.contours, ext) {
                ls.extend(level_lines(grid, c));
            }
            if !ls.is_empty() {
                under.push(line_of(h, col, &ls));
            }
        }
        let extra = Extra { under, bounds: Some([(x0, x1), (y0, y1)]), entries: vec![] };
        finish(cx, &[], &look, extra)
    }
}
