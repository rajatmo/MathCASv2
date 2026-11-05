//! Plots, first version: `plot` and `list_plot` rendered straight to PNG images.
//!
//! A plot evaluates to `image("path")`, where `path` is a PNG file in the system's temporary folder
//! (`mathcas-plots/`); front ends show it (the Jupyter kernel displays it inline). There is no graphics term model
//! yet: that comes with the data-structure work for file formats. Functions are sampled adaptively (64 even points, refined where the curve bends or stops)
//! through `mc_numfn::compile_expr` (compiled when it pays); a value that is not a finite real breaks the curve, and
//! so does a jump larger than the visible range (a pole). The y range is robust: the 1% and 99% quantiles of the
//! sampled values, widened to the full range when no value lies far outside them.
#![warn(missing_docs)]

mod render;

use mc_api::names;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use mc_numfn::{NumFn, Options, Var};
use render::{Axis, Corner, Look, Scale, Series};
use std::sync::atomic::{AtomicU64, Ordering};

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "plot",
        "plot(f, [x, a, b]): a PNG image of f for x from a to b (plot([f1, f2, ...], [x, a, b]) for several curves); evaluates to image(path)",
    ),
    (
        "list_plot",
        "list_plot(data): a PNG image of the points [y1, y2, ...] (at x = 1, 2, ...) or [[x1, y1], ...]; joined: true draws a line; evaluates to image(path)",
    ),
    ("log_plot", "log_plot(f, [x, a, b]): plot with a logarithmic y axis (values that are not positive are left out)"),
    ("log_log_plot", "log_log_plot(f, [x, a, b]): plot with logarithmic x and y axes (0 < a < b)"),
    ("list_log_plot", "list_log_plot(data): list_plot with a logarithmic y axis"),
    ("list_log_log_plot", "list_log_log_plot(data): list_plot with logarithmic x and y axes"),
    ("log_linear_plot", "log_linear_plot(f, [x, a, b]): plot with a logarithmic x axis (0 < a < b)"),
    ("list_log_linear_plot", "list_log_linear_plot(data): list_plot with a logarithmic x axis"),
    (
        "x_scale",
        "option of every plot: x_scale: linear | log | signed_log | signed_log(c) (sign(v) log10(1 + |v|/c), default c = 1; linear near 0, logarithmic far from it, both signs)",
    ),
    ("y_scale", "option of every plot: y_scale: linear | log | signed_log | signed_log(c), as x_scale"),
    ("x_label", "option of every plot: x_label: \"text\", the label under the x axis"),
    ("y_label", "option of every plot: y_label: \"text\", the label beside the y axis (read upwards)"),
    (
        "legend",
        "option of every plot: legend: [\"text\", ...], one entry per curve or data set, in order, in a framed box",
    ),
    (
        "legend_position",
        "option of every plot: legend_position: top_right (default) | top_left | bottom_left | bottom_right",
    ),
];

/// Register the natives with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    let lin = Scale::default();
    let ylog = Scale {
        x: Axis::Linear,
        y: Axis::Log,
    };
    let xlog = Scale {
        x: Axis::Log,
        y: Axis::Linear,
    };
    let both = Scale {
        x: Axis::Log,
        y: Axis::Log,
    };
    k.register(
        h.sym("plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "plot",
            scale: lin,
        }),
    );
    k.register(
        h.sym("log_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_plot",
            scale: ylog,
        }),
    );
    k.register(
        h.sym("log_log_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_log_plot",
            scale: both,
        }),
    );
    k.register(
        h.sym("list_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_plot",
            scale: lin,
        }),
    );
    k.register(
        h.sym("list_log_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_plot",
            scale: ylog,
        }),
    );
    k.register(
        h.sym("list_log_log_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_log_plot",
            scale: both,
        }),
    );
    k.register(
        h.sym("log_linear_plot"),
        Attributes::default(),
        Box::new(Plot {
            name: "log_linear_plot",
            scale: xlog,
        }),
    );
    k.register(
        h.sym("list_log_linear_plot"),
        Attributes::default(),
        Box::new(ListPlot {
            name: "list_log_linear_plot",
            scale: xlog,
        }),
    );
}

fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

fn bad<'h>(h: &'h Heap, f: &str, k: i64) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(k)]))
}

/// A machine real for a number or a closed numeric expression (through `n`).
fn real<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let direct = |t: T<'h>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    if let Some(x) = direct(t) {
        return Ok(Some(x).filter(|x| x.is_finite()));
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(direct(v).filter(|x| x.is_finite()))
}

/// The display hint for front ends: the PNG file an image term `image("path")` names and its size in pixels
/// (width, height), read from the file's header. `None` when `t` is not an image term or the file is not a
/// readable PNG; the value is then shown as text.
pub fn image_of<'h>(h: &'h Heap, t: T<'h>) -> Option<(String, u32, u32)> {
    let View::App { head, args } = h.view(t) else { return None };
    let (View::Sym(s), [path]) = (h.view(head), args) else { return None };
    let View::Str(path) = h.view(*path) else { return None };
    if h.sym_name(s) != "image" {
        return None;
    }
    let (w, ht) = png_size(std::path::Path::new(path))?;
    Some((path.to_string(), w, ht))
}

/// The width and height of the PNG file at `path`, from its signature and `IHDR` chunk.
pub fn png_size(path: &std::path::Path) -> Option<(u32, u32)> {
    use std::io::Read;
    let mut head = [0u8; 24];
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    png_header_size(&head)
}

/// The size in the first 24 bytes of a PNG file: the signature, then the `IHDR` chunk's length, type, width and
/// height (big-endian).
fn png_header_size(head: &[u8; 24]) -> Option<(u32, u32)> {
    const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if head[..8] != SIGNATURE || &head[12..16] != b"IHDR" {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
    Some((be(16), be(20))).filter(|&(w, h)| w > 0 && h > 0)
}

/// The image term for a written PNG file.
fn image<'h>(h: &'h Heap, path: &str) -> T<'h> {
    h.call(h.sym("image"), &[h.string(path)])
}

/// Write the rendered plot to a fresh PNG file, and the same plot as a vector PDF next to it (same name, `.pdf`:
/// `export_data("f.pdf", plot)` copies it), and give the PNG's image term (or a failure).
fn finish<'h>(cx: &mut NativeCx<'_, '_, 'h>, series: &[Series], look: &Look) -> Res<Reply<'h>> {
    let h = cx.heap();
    // writing a file is an effect: in program order, never speculatively
    cx.effect()?;
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join("mathcas-plots");
    let path = dir.join(format!(
        "plot-{}-{}.png",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    let written = std::fs::create_dir_all(&dir)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            render::render(series, look).ok_or_else(|| "rendering failed".to_string())
        })
        .and_then(|png| std::fs::write(&path, png).map_err(|e| e.to_string()))
        .and_then(|()| {
            let pdf = render::render_pdf(series, look);
            std::fs::write(path.with_extension("pdf"), pdf).map_err(|e| e.to_string())
        });
    Ok(Reply::Done(match written {
        // forward slashes: the path prints without escapes on every system
        Ok(()) => image(h, &path.to_string_lossy().replace('\\', "/")),
        Err(e) => failure(h, "io", &[h.string(&e)]),
    }))
}

/// `plot(f, [x, a, b])` and its log-scale forms.
struct Plot {
    name: &'static str,
    scale: Scale,
}

impl Native for Plot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let base = Look {
            scale: self.scale,
            joined: true,
            ..Look::default()
        };
        let (f, spec, look) = match args {
            [f, spec] => (f, spec, base),
            [f, spec, o] => match plot_options(cx, *o, base, false) {
                Some(look) => (f, spec, look),
                None => return Ok(bad(h, name, 3)),
            },
            _ => return Ok(Reply::Decline),
        };
        let scale = look.scale;
        let Some(&[xv, a, b]) = cx.as_list(*spec) else {
            return Ok(bad(h, name, 2));
        };
        let View::Sym(x) = h.view(xv) else {
            return Ok(bad(h, name, 2));
        };
        let (Some(a), Some(b)) = (real(cx, a)?, real(cx, b)?) else {
            return Ok(bad(h, name, 2));
        };
        // the sampled interval, on the x axis (logarithms on a log axis)
        let (Some(ua), Some(ub)) = (scale.x.to(a), scale.x.to(b)) else {
            return Ok(bad(h, name, 2));
        };
        if ua >= ub {
            return Ok(bad(h, name, 2));
        }
        let fs: Vec<T<'h>> = match cx.as_list(*f) {
            Some(fs) if !fs.is_empty() => fs.to_vec(),
            Some(_) => return Ok(bad(h, name, 1)),
            None => vec![*f],
        };
        let mut series = Vec::with_capacity(fs.len());
        for f in fs {
            series.push(sample(cx, f, x, ua, ub, scale)?);
        }
        finish(cx, &series, &look)
    }
}

/// The curve of `f` in `x` on [a, b], sampled adaptively: `INITIAL` even points, then each interval is halved
/// while its midpoint lies more than `TOLERANCE` (as a fraction of the visible y range, about half a pixel) off the
/// chord, or while the value is a number at one end and not at the other (to find where it stops), up to `DEPTH`
/// halvings and `BUDGET` evaluations in all. `None` where the value is not a finite real.
/// The interval [a, b] and the points are on the axes (logarithms on log axes), so refinement adapts to the curve as
/// it is drawn.
fn sample<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    f: T<'h>,
    x: Sym,
    a: f64,
    b: f64,
    scale: Scale,
) -> Res<Series> {
    let fun = mc_numfn::compile_expr(cx, f, &[Var::scalar(x)], &Options::default())?;
    let mut s = Sampler {
        fun,
        evals: 0,
        scale,
    };
    let mut grid = Vec::with_capacity(INITIAL);
    for k in 0..INITIAL {
        let xk = a + (b - a) * k as f64 / (INITIAL - 1) as f64;
        grid.push((xk, s.at(cx, xk)?));
    }
    // the visible y range of the coarse curve sets the tolerance
    let span = {
        let (lo, hi) = render::robust(grid.iter().filter_map(|p| p.1).collect());
        if hi > lo {
            hi - lo
        } else {
            1.0
        }
    };
    let mut pts = vec![grid[0]];
    for w in grid.windows(2) {
        s.refine(cx, w[0], w[1], span, DEPTH, &mut pts)?;
    }
    Ok(Series { points: pts })
}

/// Even points before refinement.
const INITIAL: usize = 64;
/// Halvings of an initial interval at most.
const DEPTH: u32 = 10;
/// Evaluations per curve at most.
const BUDGET: u64 = 20_000;
/// Deviation from the chord, as a fraction of the visible y range, that asks for a halving.
const TOLERANCE: f64 = 0.0015;

struct Sampler<'h> {
    fun: Box<dyn NumFn<'h, f64> + 'h>,
    evals: u64,
    scale: Scale,
}

impl<'h> Sampler<'h> {
    /// The curve at `u` on the x axis: the value on the y axis, `None` where there is none.
    fn at(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: f64) -> Res<Option<f64>> {
        self.evals += 1;
        if self.evals.is_multiple_of(64) {
            cx.tick(1)?;
        }
        let x = self.scale.x.from(u);
        let mut out = [0.0];
        let ok = self.fun.eval(cx, &[x], &mut out)?;
        Ok(if ok && out[0].is_finite() {
            self.scale.y.to(out[0])
        } else {
            None
        })
    }

    /// Push the points of (p, q], refined; `p` is already pushed. Recursion depth is at most `DEPTH`.
    fn refine(
        &mut self,
        cx: &mut NativeCx<'_, '_, 'h>,
        p: (f64, Option<f64>),
        q: (f64, Option<f64>),
        span: f64,
        depth: u32,
        out: &mut Vec<(f64, Option<f64>)>,
    ) -> Res<()> {
        if depth > 0 && self.evals < BUDGET {
            let xm = 0.5 * (p.0 + q.0);
            let m = (xm, self.at(cx, xm)?);
            let split = match (p.1, m.1, q.1) {
                (Some(a), Some(c), Some(b)) => (c - 0.5 * (a + b)).abs() > TOLERANCE * span,
                (None, None, None) => false,
                _ => true,
            };
            if split {
                self.refine(cx, p, m, span, depth - 1, out)?;
                return self.refine(cx, m, q, span, depth - 1, out);
            }
            out.push(m);
        }
        out.push(q);
        Ok(())
    }
}

/// `list_plot(data)`, `list_plot(data, joined: true)` and the log-scale forms.
struct ListPlot {
    name: &'static str,
    scale: Scale,
}

impl Native for ListPlot {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let name = self.name;
        let base = Look {
            scale: self.scale,
            ..Look::default()
        };
        let (data, look) = match args {
            [d] => (*d, base),
            [d, o] => match plot_options(cx, *o, base, true) {
                Some(look) => (*d, look),
                None => return Ok(bad(h, name, 2)),
            },
            _ => return Ok(Reply::Decline),
        };
        let sc = look.scale;
        let Some(items) = cx.as_list(data) else {
            return Ok(bad(h, name, 1));
        };
        let mut pts = Vec::with_capacity(items.len());
        for (i, &it) in items.iter().enumerate() {
            if i % 4096 == 0 {
                cx.tick(1)?;
            }
            let p = match cx.as_list(it) {
                Some(&[px, py]) => (real(cx, px)?, real(cx, py)?),
                Some(_) => return Ok(bad(h, name, 1)),
                None => (Some(i as f64 + 1.0), real(cx, it)?),
            };
            match p {
                // a point whose x is not on a log axis (not positive) is left out
                (Some(px), y) => {
                    if let Some(u) = sc.x.to(px) {
                        pts.push((u, y.and_then(|y| sc.y.to(y))));
                    }
                }
                (None, _) => return Ok(bad(h, name, 1)),
            }
        }
        if pts.iter().all(|p| p.1.is_none()) {
            return Ok(bad(h, name, 1));
        }
        finish(cx, &[Series { points: pts }], &look)
    }
}

/// The plot options `options(k -> v, ...)`: `x_scale`, `y_scale` (`linear`, `log`, `signed_log`, `signed_log(c)`),
/// `x_label`, `y_label` (strings), `legend` (a list of strings), `legend_position` (a corner) and, when
/// `joined_ok`, `joined` (`true` / `false`), applied to `base`. `None` for anything else.
fn plot_options<'h>(
    cx: &NativeCx<'_, '_, 'h>,
    o: T<'h>,
    base: Look,
    joined_ok: bool,
) -> Option<Look> {
    let h = cx.heap();
    let View::App { head, args } = h.view(o) else {
        return None;
    };
    if head != h.sym_term(h.sym("options")) {
        return None;
    }
    let mut look = base;
    let text = |t: T<'h>| match h.view(t) {
        View::Str(s) => Some(s.to_string()),
        _ => None,
    };
    for &r in args {
        let View::App {
            head,
            args: &[k, v],
        } = h.view(r)
        else {
            return None;
        };
        if head != h.sym_term(h.sym(names::RULE)) {
            return None;
        }
        let View::Sym(k) = h.view(k) else { return None };
        match h.sym_name(k).as_str() {
            "joined" if joined_ok => look.joined = v == h.sym_term(h.sym("true")),
            "x_scale" => look.scale.x = axis_option(h, v)?,
            "y_scale" => look.scale.y = axis_option(h, v)?,
            "x_label" => look.x_label = Some(text(v)?),
            "y_label" => look.y_label = Some(text(v)?),
            "legend" => {
                look.legend = cx
                    .as_list(v)?
                    .iter()
                    .map(|&e| text(e))
                    .collect::<Option<_>>()?
            }
            "legend_position" => look.legend_at = corner_option(h, v)?,
            _ => return None,
        }
    }
    Some(look)
}

/// A corner: `top_right`, `top_left`, `bottom_left` or `bottom_right`.
fn corner_option(h: &Heap, v: T<'_>) -> Option<Corner> {
    let View::Sym(s) = h.view(v) else { return None };
    match h.sym_name(s).as_str() {
        "top_right" => Some(Corner::TopRight),
        "top_left" => Some(Corner::TopLeft),
        "bottom_left" => Some(Corner::BottomLeft),
        "bottom_right" => Some(Corner::BottomRight),
        _ => None,
    }
}

/// An axis scale: `linear`, `log`, `signed_log` (c = 1) or `signed_log(c)` with a number c > 0.
fn axis_option(h: &Heap, v: T<'_>) -> Option<Axis> {
    let num = |t: T<'_>| match h.view(t) {
        View::Int(i) => Some(i as f64),
        View::Real(x) => Some(x),
        _ => None,
    };
    match h.view(v) {
        View::Sym(s) => match h.sym_name(s).as_str() {
            "linear" => Some(Axis::Linear),
            "log" => Some(Axis::Log),
            "signed_log" => Some(Axis::SignedLog(1.0)),
            _ => None,
        },
        View::App { head, args: &[c] } if head == h.sym_term(h.sym("signed_log")) => num(c)
            .filter(|c| *c > 0.0 && c.is_finite())
            .map(Axis::SignedLog),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_header() {
        let mut head = [0u8; 24];
        head[..8].copy_from_slice(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
        head[8..12].copy_from_slice(&13u32.to_be_bytes());
        head[12..16].copy_from_slice(b"IHDR");
        head[16..20].copy_from_slice(&640u32.to_be_bytes());
        head[20..24].copy_from_slice(&400u32.to_be_bytes());
        assert_eq!(png_header_size(&head), Some((640, 400)));
        head[12] = b'X';
        assert_eq!(png_header_size(&head), None);
        assert_eq!(png_size(std::path::Path::new("no such file.png")), None);
    }
}
