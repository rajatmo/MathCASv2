//! The natives: checks of primitives and directives, `graphics` (numeric constants made numbers, the whole term
//! checked), `show`, and the colour functions.
use mc_graphics::color::{colour, hex_color, is_symbolic, Colour, Rgba, HEADS};
use mc_graphics::model::{self, Bad, Style, DIRECTIVES, PRIMITIVES};
use mc_graphics::num::number;
use mc_graphics::Failure;
use mc_api::names;
use mc_core::{Heap, Store, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// Register every native with `k`.
pub fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default;
    for name in PRIMITIVES {
        k.register(h.sym(name), a(), Box::new(Primitive(name)));
    }
    for name in DIRECTIVES.iter().filter(|n| **n != "scaled") {
        k.register(h.sym(name), a(), Box::new(Directive(name)));
    }
    for name in HEADS {
        k.register(h.sym(name), a(), Box::new(ColourTerm(name)));
    }
    k.register(h.sym("graphics"), a(), Box::new(Graphics("graphics")));
    k.register(h.sym("graphics3d"), a(), Box::new(Graphics("graphics3d")));
    k.register(h.sym("show"), a(), Box::new(Show));
    k.register(h.sym("color_convert"), a(), Box::new(ColorConvert));
    k.register(h.sym("blend"), a(), Box::new(Blend));
    k.register(h.sym("lighter"), a(), Box::new(Shade { name: "lighter", towards: Rgba::WHITE }));
    k.register(h.sym("darker"), a(), Box::new(Shade { name: "darker", towards: Rgba::BLACK }));
    k.register(h.sym("gradient_color"), a(), Box::new(GradientColor));
    k.register(h.sym("is_color"), a(), Box::new(IsColor));
    k.register(h.sym("multicolumn"), a(), Box::new(Multicolumn));
    k.register(h.sym("color_negate"), a(), Box::new(ColorNegate));
    k.register(h.sym("color_distance"), a(), Box::new(ColorDistance));
    crate::compose::install(k, h);
    crate::transform::install(k, h);
    k.register(h.sym("graphics_complex"), a(), Box::new(GraphicsComplex));
    k.register(h.sym("circle_points"), a(), Box::new(CirclePoints(false)));
    k.register(h.sym("regular_polygon"), a(), Box::new(CirclePoints(true)));
    for name in ["tooltip", "mouseover", "status_area", "popup_window", "light_mode_pane", "dark_mode_pane", "highlighted", "list_animate", "magnify"] {
        k.register(h.sym(name), a(), Box::new(Shown(name)));
    }
}

/// Wrappers of interactive front ends in a static picture: `tooltip(x, tip)`, `mouseover(x, over)`,
/// `status_area(x, text)`, `popup_window(x, content)` and the panes are x itself; `highlighted(x)` is x on a light
/// yellow background (a graphic stays as it is).
struct Shown(&'static str);

impl Native for Shown {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let Some(&x) = args.first() else { return Ok(Reply::Decline) };
        let h = _cx.heap();
        if self.0 == "magnify" {
            // magnify(x): x as a picture; magnify(x, k) of a picture: the picture; otherwise inert
            return Ok(match args.len() {
                1 if mc_graphics::is_graphics(h, x) || mc_graphics::memo::image_path(h, x).is_some() => Reply::Done(x),
                1 => Reply::Again(h.call(h.sym("pane"), &[x])),
                _ if mc_graphics::is_graphics(h, x) || mc_graphics::memo::image_path(h, x).is_some() => Reply::Done(x),
                _ => Reply::Decline,
            });
        }
        if self.0 == "list_animate" {
            // a static picture: the first frame
            // (a value that is not a picture is shown in a pane, so the animation is always a picture)
            return Ok(match _cx.as_list(x).and_then(|l| l.first().copied()) {
                Some(f) if mc_graphics::is_graphics(h, f) || mc_graphics::memo::image_path(h, f).is_some() => Reply::Done(f),
                Some(f) => Reply::Again(h.call(h.sym("pane"), &[f])),
                None => Reply::Decline,
            });
        }
        if self.0 == "highlighted" && !mc_graphics::is_graphics(h, x) {
            // inert: plots read the highlighted curve; anything else is shown as it is
            return Ok(Reply::Decline);
        }
        Ok(Reply::Done(x))
    }
}

/// `multicolumn(list, n)`: the items in n columns, filled down each column, as a grid (a picture).
struct Multicolumn;

impl Native for Multicolumn {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let Some(items) = args.first().and_then(|&l| cx.as_list(l)).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
        if items.is_empty() {
            return Ok(Reply::Decline);
        }
        let n = match args.get(1).map(|&k| h.view(k)) {
            Some(View::Int(k)) if k >= 1 => k as usize,
            // [columns, rows]: the columns given
            Some(_) if args.get(1).and_then(|&k| cx.as_list(k)).and_then(|l| l.first()).is_some_and(|&c| matches!(h.view(c), View::Int(k) if k >= 1)) => {
                match h.view(cx.as_list(args[1]).map(|l| l[0]).unwrap_or(args[1])) {
                    View::Int(k) => k as usize,
                    _ => 1,
                }
            }
            Some(_) if !matches!(args.get(1).and_then(|&k| model::head_name(h, k)), Some((r, _)) if r == names::RULE) => return Ok(Reply::Decline),
            _ => (items.len() as f64).sqrt().ceil() as usize,
        };
        let rows = items.len().div_ceil(n);
        let blank = h.string("");
        let grid: Vec<T<'h>> = (0..rows)
            .map(|r| {
                let row: Vec<T<'h>> = (0..n).map(|c| items.get(c * rows + r).copied().unwrap_or(blank)).collect();
                cx.list(&row)
            })
            .collect();
        Ok(Reply::Again(h.call(h.sym("grid"), &[cx.list(&grid)])))
    }
}

/// `gradient_color(g, t)`: the colour of gradient `g` at `t` (clamped to [0, 1]); a symbolic `t` leaves it.
struct GradientColor;

impl Native for GradientColor {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [g, t] = args else { return Ok(Reply::Decline) };
        let Some(grad) = mc_graphics::gradient::read(h, *g) else { return Ok(bad(h, "gradient_color", 1)) };
        // a closed numeric expression (`pi/4`) is its value
        let v = match number(h, *t) {
            Some(v) => Some(v),
            None => number(h, n_value(cx, *t)?),
        };
        Ok(match v {
            Some(v) => Reply::Done(rgb_term(h, grad.at(v))),
            None if is_symbolic(h, *t) => Reply::Decline,
            None => bad(h, "gradient_color", 2),
        })
    }
}

fn failure<'h>(h: &'h Heap, tag: &str, details: &[T<'h>]) -> T<'h> {
    let mut a = vec![h.sym_term(h.sym(tag))];
    a.extend_from_slice(details);
    h.call(h.sym(names::FAILURE), &a)
}

/// `failure(argument, f, k)`: argument `k` of `f` is wrong.
fn bad<'h>(h: &'h Heap, f: &str, k: usize) -> Reply<'h> {
    Reply::Done(failure(h, "argument", &[h.sym_term(h.sym(f)), h.int(k as i64)]))
}

/// A primitive: checked when its coordinates are numbers (a failure names the wrong argument), left as it is.
struct Primitive(&'static str);

impl Native for Primitive {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        cx.tick(1)?;
        Ok(match model::shape(h, self.0, args) {
            // coordinates in three (or more) dimensions: a primitive of 3D graphics or a region
            Err(Bad::Arg(k))
                if matches!((self.0, k), ("point" | "line" | "polygon" | "arrow" | "bezier_curve" | "b_spline_curve", 1) | ("text", 2))
                    && args.get(k - 1).is_some_and(|&a| mc_graphics::three::is_coords(h, a)) =>
            {
                Reply::Decline
            }
            // coordinates in one dimension: a region on the line (`line([[0], [1]])`)
            Err(Bad::Arg(1)) if matches!(self.0, "point" | "line" | "polygon") && args.first().is_some_and(|&a| one_dimensional(h, a)) => Reply::Decline,
            // several centres: each is checked when the graphic is read
            Err(Bad::Arg(1)) if matches!(self.0, "disk" | "circle") && model::several_centres(h, args) => Reply::Decline,
            // indices into the points of a graphics_complex or mesh region (`disk(1, 0.5)`, `line([1, 2, 3])`)
            Err(Bad::Arg(1)) if args.first().is_some_and(|&a| indices_only(h, a)) => Reply::Decline,
            Err(Bad::Arg(k)) => bad(h, self.0, k),
            _ => Reply::Decline,
        })
    }
}

/// Whether `t` is a positive integer or a (nested) list of them: indices of points rather than coordinates.
fn indices_only(h: &Heap, t: T<'_>) -> bool {
    match h.view(t) {
        View::Int(k) => k >= 1,
        View::App { head, args } => {
            matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST) && !args.is_empty() && args.iter().all(|&a| indices_only(h, a))
        }
        _ => false,
    }
}

/// Whether `t` is a point of one coordinate, or a list (of lists) of such points.
fn one_dimensional(h: &Heap, t: T<'_>) -> bool {
    let mut cur = t;
    for _ in 0..3 {
        let View::App { head, args } = h.view(cur) else { return false };
        if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == names::LIST) {
            return false;
        }
        match args {
            [x] if mc_graphics::num::number(h, *x).is_some() || matches!(h.view(*x), View::Sym(_)) => return true,
            [first, ..] => cur = *first,
            [] => return false,
        }
    }
    false
}

/// A directive: checked like a primitive.
struct Directive(&'static str);

impl Native for Directive {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let mut s = Style::default();
        Ok(match model::directive(h, t, &mut s) {
            Err(Bad::Arg(k)) => bad(h, self.0, k),
            _ => Reply::Decline,
        })
    }
}

/// `rgb_color`, `hue`, `gray_level`: `rgb_color("#rrggbb")` becomes numbers; components that are neither numbers
/// nor symbolic are wrong.
struct ColourTerm(&'static str);

impl Native for ColourTerm {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if let [l] = args {
            // the components as one list: rgb_color([r, g, b]), hue([h, s, b]), ...
            if let Some(items) = cx.as_list(*l).filter(|v| (1..=5).contains(&v.len())) {
                return Ok(Reply::Again(h.call(h.sym(self.0), items)));
            }
        }
        if let (true, [s]) = (self.0 == "rgb_color", args) {
            if let View::Str(s) = h.view(*s) {
                return Ok(match hex_color(s) {
                    Some(c) => Reply::Done(rgb_term(h, c)),
                    None => bad(h, self.0, 1),
                });
            }
        }
        if colour(h, t) == Colour::Bad {
            if let Some(k) = args.iter().position(|&a| number(h, a).is_none() && !is_symbolic(h, a)) {
                return Ok(bad(h, self.0, k + 1));
            }
        }
        Ok(Reply::Decline)
    }
}

/// `rgb_color(r, g, b)` (and the opacity when it is not 1).
pub fn rgb_term<'h>(h: &'h Heap, c: Rgba) -> T<'h> {
    let mut a = vec![h.real(c.r), h.real(c.g), h.real(c.b)];
    if c.a != 1.0 {
        a.push(h.real(c.a));
    }
    h.call(h.sym("rgb_color"), &a)
}

/// Nodes of the content visited between fuel charges.
const CHUNK: u64 = 4096;
/// Nesting depth past which `graphics` leaves the content as it is.
const MAX_DEPTH: usize = 64;

/// `graphics(content)`, `graphics(content, options(...))`: numeric constants and closed numeric expressions in
/// coordinates become machine reals (`pi` -> 3.14159...), labels of `text` are left alone; then the graphic is
/// read: content that is not graphics, and bad options, give failures; symbolic coordinates leave it as it is.
struct Graphics(&'static str);

impl Native for Graphics {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        if !matches!(args.len(), 1 | 2) {
            return Ok(Reply::Decline);
        }
        let mut seen = 0u64;
        let mut changed = false;
        let mut new = Vec::with_capacity(2);
        for &a in args {
            let v = numeric(cx, a, 0, &mut seen)?;
            changed |= v != a;
            new.push(v);
        }
        let g = if changed { h.call(h.sym(self.0), &new) } else { t };
        let stop = cx.stop_flag().clone();
        let stopped = move || stop.load(std::sync::atomic::Ordering::Relaxed);
        Ok(Reply::Done(match model::read(h, g, &stopped) {
            Ok(_) => g,
            Err(f) if f.tag == "not_numeric" => g,
            Err(f) => f.term(h),
        }))
    }
}

/// `t` with closed numeric expressions in it evaluated to machine reals (see [`Graphics`]).
fn numeric<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, depth: usize, seen: &mut u64) -> Res<T<'h>> {
    let h = cx.heap();
    *seen += 1;
    if seen.is_multiple_of(CHUNK) {
        cx.tick(1)?;
    }
    if depth > MAX_DEPTH || h.packed(t).is_some() {
        return Ok(t);
    }
    match h.view(t) {
        View::Sym(_) => n_value(cx, t),
        View::App { head, args } => {
            let structural = match h.view(head) {
                View::Sym(s) => {
                    let n = h.sym_name(s);
                    n == names::LIST
                        || n == names::RULE
                        || n == "options"
                        || n == "graphics"
                        || n == "graphics3d"
                        || n == "style"
                        || n == "directive"
                        || n == "scaled"
                        || mc_graphics::three::PRIMITIVES.contains(&n.as_str())
                        || PRIMITIVES.contains(&n.as_str())
                        || model::CONTAINERS.contains(&n.as_str())
                        || DIRECTIVES.contains(&n.as_str())
                        || HEADS.contains(&n.as_str())
                }
                _ => false,
            };
            if !structural {
                return n_value(cx, t);
            }
            // the label of `text` and the name of an option stay as written
            let keep_first = matches!(h.view(head), View::Sym(s) if { let n = h.sym_name(s); n == "text" || n == names::RULE });
            let mut out = Vec::with_capacity(args.len());
            let mut changed = false;
            for (i, &a) in args.iter().enumerate() {
                let v = if i == 0 && keep_first { a } else { numeric(cx, a, depth + 1, seen)? };
                changed |= v != a;
                out.push(v);
            }
            Ok(if changed { h.app(head, &out) } else { t })
        }
        _ => Ok(t),
    }
}

/// `n(t)` when that is a number, else `t`.
fn n_value<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let h = cx.heap();
    if let View::Sym(s) = h.view(t) {
        // names that are values of directives and options are never numbers
        let n = h.sym_name(s);
        if mc_graphics::color::named(&n).is_some() || matches!(n.as_str(), "none" | "automatic" | "all" | "full" | "true" | "false") {
            return Ok(t);
        }
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => v,
        // a real number computed through complex arithmetic: a rounding-error imaginary part is dropped
        View::App { head, args: &[re, im] } if matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "complex") => match (h.view(re), h.view(im)) {
            (View::Real(a), View::Real(b)) if a.is_finite() && b.abs() <= 1e-10 * a.abs().max(1.0) => h.real(a),
            _ => t,
        },
        _ => t,
    })
}

/// The content or options of a graphic with closed numeric expressions in coordinates made machine reals, as
/// `graphics` does: plots apply it to their `epilog` and `prolog`.
pub fn numeric_content<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
    let mut seen = 0u64;
    numeric(cx, t, 0, &mut seen)
}

/// `show(g1, g2, ..., options)`: one graphic of the content of all, with the options given to `show` first, then
/// those of each graphic in order (the first value of an option wins). Lists of graphics are taken apart.
struct Show;

impl Native for Show {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let options = h.sym("options");
        let (gs, own) = match args.split_last() {
            Some((&o, rest)) if matches!(h.view(o), View::App { head, .. } if head == h.sym_term(options)) => (rest, Some(o)),
            _ => (args, None),
        };
        if gs.is_empty() {
            return Ok(Reply::Decline);
        }
        let mut contents = Vec::new();
        let mut rules: Vec<(String, T<'h>)> = Vec::new();
        let add_rules = |o: T<'h>, rules: &mut Vec<(String, T<'h>)>| -> Result<(), Failure> {
            for (k, v) in model::rules(h, o)? {
                if !rules.iter().any(|(n, _)| *n == k) {
                    rules.push((k, v));
                }
            }
            Ok(())
        };
        if let Some(o) = own {
            if let Err(f) = add_rules(o, &mut rules) {
                return Ok(Reply::Done(f.term(h)));
            }
        }
        // the graphics, lists of them flattened (bounded: a list of lists of graphics at most)
        let mut flat = Vec::new();
        for (i, &g) in gs.iter().enumerate() {
            match cx.as_list(g) {
                Some(items) => flat.extend(items.iter().map(|&e| (i, e))),
                None => flat.push((i, g)),
            }
        }
        // plots (`image(path)`) as the graphics they were drawn from
        for e in flat.iter_mut() {
            e.1 = crate::compose::resolve(cx, e.1)?;
            // a region (a mesh region or a primitive) as its picture
            if let Some((name, ra)) = model::head_name(h, e.1) {
                let region = matches!(name.as_str(), "mesh_region" | "boundary_mesh_region")
                    || mc_graphics::three::PRIMITIVES.contains(&name.as_str())
                    || PRIMITIVES.contains(&name.as_str());
                if region && name != "text" {
                    let solid = matches!(name.as_str(), "pyramid" | "prism" | "hexahedron" | "tetrahedron" | "torus" | "filled_torus" | "cuboid" | "cylinder" | "cone" | "infinite_plane" | "parallelepiped");
                    let three = solid || ra.first().is_some_and(|&p| mc_graphics::three::is_coords(h, p));
                    e.1 = h.call(h.sym(if three { "graphics3d" } else { "graphics" }), &[e.1]);
                }
            }
        }
        let own_range = rules.iter().any(|(k, _)| k == "plot_range");
        let mut graphic_opts = vec![];
        let mut legends: Vec<T<'h>> = vec![];
        let mut kind: Option<T<'h>> = None;
        for (i, g) in flat {
            cx.tick(1)?;
            if let View::App { args: ga, .. } = h.view(g) {
                graphic_opts.push(ga.get(1).copied());
                if let Some(Some(l)) = ga.get(1).map(|&o| model::rules(h, o).ok().and_then(|rs| rs.into_iter().find(|(k, _)| k == "legend").map(|r| r.1))) {
                    legends.extend(cx.as_list(l).unwrap_or(&[]).iter().copied());
                }
            }
            let View::App { head, args: ga } = h.view(g) else { return Ok(bad(h, "show", i + 1)) };
            // all two-dimensional, or all three-dimensional
            let kind = *kind.get_or_insert(head);
            if (head != h.sym_term(h.sym("graphics")) && head != h.sym_term(h.sym("graphics3d"))) || head != kind || ga.is_empty() || ga.len() > 2 {
                return Ok(bad(h, "show", i + 1));
            }
            contents.push(ga[0]);
            if let Some(&o) = ga.get(1) {
                if let Err(f) = add_rules(o, &mut rules) {
                    return Ok(Reply::Done(f.term(h)));
                }
            }
        }
        let three = kind == Some(h.sym_term(h.sym("graphics3d")));
        // three-dimensional: the box around all the content, unless show gives a range
        if three && !own_range {
            rules.retain(|(k, _)| k != "plot_range");
        }
        // combined plots: the union of their ranges, all their legends
        if !three && !own_range && graphic_opts.len() > 1 {
            if let Some([x0, x1, y0, y1]) = crate::compose::union_range(h, &graphic_opts) {
                let range = cx.list(&[h.reals(&[x0, x1]), h.reals(&[y0, y1])]);
                rules.retain(|(k, _)| k != "plot_range");
                rules.push(("plot_range".into(), range));
            }
        }
        // axis and frame labels: for each axis the first graphic's that is not `none`
        for key in ["frame_label", "axes_label"] {
            if own.is_some_and(|o| model::rules(h, o).is_ok_and(|rs| rs.iter().any(|(k, _)| k == key))) {
                continue;
            }
            let mut pair: [Option<T<'h>>; 2] = [None, None];
            for &o in &graphic_opts {
                let Some(v) = o.and_then(|o| model::rules(h, o).ok()).and_then(|rs| rs.into_iter().find(|(k, _)| k == key)).map(|r| r.1) else {
                    continue;
                };
                if let Some(&[x, y]) = cx.as_list(v) {
                    for (slot, l) in pair.iter_mut().zip([x, y]) {
                        if slot.is_none() && !matches!(h.view(l), View::Sym(s) if h.sym_name(s) == "none") {
                            *slot = Some(l);
                        }
                    }
                }
            }
            if pair.iter().any(Option::is_some) {
                let none = h.sym_term(h.sym("none"));
                let v = cx.list(&[pair[0].unwrap_or(none), pair[1].unwrap_or(none)]);
                rules.retain(|(k, _)| k != key);
                rules.push((key.into(), v));
            }
        }
        // explicit ticks and grid lines (the powers of ten of log plots): those of every graphic, per axis
        for key in ["ticks", "grid_lines"] {
            if own.is_some_and(|o| model::rules(h, o).is_ok_and(|rs| rs.iter().any(|(k, _)| k == key))) || graphic_opts.len() < 2 {
                continue;
            }
            let per: Option<Vec<[&[T<'h>]; 2]>> = graphic_opts
                .iter()
                .map(|&o| {
                    let v = model::rules(h, o?).ok()?.into_iter().find(|(k, _)| k == key)?.1;
                    let &[x, y] = cx.as_list(v)? else { return None };
                    Some([cx.as_list(x)?, cx.as_list(y)?])
                })
                .collect();
            if let Some(per) = per {
                let axis = |k: usize| -> T<'h> {
                    let mut seen: Vec<T<'h>> = vec![];
                    for p in &per {
                        for &e in p[k] {
                            if !seen.contains(&e) {
                                seen.push(e);
                            }
                        }
                    }
                    cx.list(&seen)
                };
                let merged = cx.list(&[axis(0), axis(1)]);
                rules.retain(|(k, _)| k != key);
                rules.push((key.into(), merged));
            }
        }
        if legends.len() > 1 && !own.is_some_and(|o| model::rules(h, o).is_ok_and(|rs| rs.iter().any(|(k, _)| k == "legend"))) {
            rules.retain(|(k, _)| k != "legend");
            rules.push(("legend".into(), cx.list(&legends)));
        }
        let rule = h.sym(names::RULE);
        let rs: Vec<T<'h>> = rules.iter().map(|(k, v)| h.call(rule, &[h.sym_term(h.sym(k)), *v])).collect();
        let mut a = vec![cx.list(&contents)];
        if !rs.is_empty() {
            a.push(h.call(options, &rs));
        }
        Ok(Reply::Again(h.call(h.sym(if three { "graphics3d" } else { "graphics" }), &a)))
    }
}

/// Whether a term is an exact number (an integer or a rational).
fn exact_number<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    matches!(h.view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// Whether a colour term has exact components: a named colour whose components are 0 or 1, or `rgb_color` or
/// `gray_level` with exact arguments.
fn exact_colour<S: Store + ?Sized>(h: &S, t: T<'_>) -> bool {
    match h.view(t) {
        View::Sym(s) => mc_graphics::color::named(&h.sym_name(s)).is_some_and(|c| [c.r, c.g, c.b, c.a].iter().all(|&v| v == 0.0 || v == 1.0)),
        View::App { args, .. } => {
            model::head_name(h, t).is_some_and(|(n, _)| n == "rgb_color" || n == "gray_level") && args.iter().all(|&a| exact_number(h, a))
        }
        _ => false,
    }
}

/// `v` as an exact number when it is a rational of small denominator (within rounding), else as a real.
fn exactly(h: &Heap, v: f64) -> T<'_> {
    if v.is_finite() {
        // continued fraction convergents up to a denominator of 10^6
        let (mut p0, mut q0, mut p1, mut q1) = (0i64, 1i64, 1i64, 0i64);
        let mut x = v;
        for _ in 0..40 {
            let a = x.floor();
            if a.abs() > 1e12 {
                break;
            }
            let a = a as i64;
            let (p2, q2) = (a * p1 + p0, a * q1 + q0);
            if q2 > 1_000_000 {
                break;
            }
            (p0, q0, p1, q1) = (p1, q1, p2, q2);
            if (p1 as f64 / q1 as f64 - v).abs() <= 1e-12 * v.abs().max(1.0) {
                return if q1 == 1 { h.int(p1) } else { h.rational(h.int(p1), h.int(q1)) };
            }
            let f = x - a as f64;
            if f.abs() < 1e-300 {
                break;
            }
            x = 1.0 / f;
        }
    }
    h.real(v)
}

/// `rgb_color(r, g, b)` (and the opacity when it is not 1), with exact components when `exact`.
fn rgb_term_exact(h: &Heap, c: Rgba, exact: bool) -> T<'_> {
    if !exact {
        return rgb_term(h, c);
    }
    let mut a = vec![exactly(h, c.r), exactly(h, c.g), exactly(h, c.b)];
    if c.a != 1.0 {
        a.push(exactly(h, c.a));
    }
    h.call(h.sym("rgb_color"), &a)
}

/// The colour `c` as a term of the colour model `model` (a head name, or a model name such as `"CMYK"`).
fn in_model<'h>(h: &'h Heap, c: Rgba, model: &str) -> Option<T<'h>> {
    let head = match model {
        "rgb_color" | "RGB" => "rgb_color",
        "hue" | "HSB" => "hue",
        "gray_level" | "Grayscale" | "GrayLevel" => "gray_level",
        "cmyk_color" | "CMYK" => "cmyk_color",
        "lab_color" | "LAB" | "CIELab" => "lab_color",
        "xyz_color" | "XYZ" => "xyz_color",
        "luv_color" | "LUV" | "CIELuv" => "luv_color",
        "lch_color" | "LCH" | "LCHab" => "lch_color",
        _ => return None,
    };
    let comps: Vec<f64> = match head {
        "rgb_color" => vec![c.r, c.g, c.b],
        "hue" => {
            let (hu, s, b) = c.hsb();
            vec![hu, s, b]
        }
        "gray_level" => vec![c.gray()],
        "cmyk_color" => mc_graphics::color::to_cmyk(c).to_vec(),
        "lab_color" => mc_graphics::color::to_lab(c).to_vec(),
        "xyz_color" => mc_graphics::color::to_xyz(c).to_vec(),
        "luv_color" => mc_graphics::color::to_luv(c).to_vec(),
        _ => {
            let [l, a, b] = mc_graphics::color::to_lab(c);
            vec![l, (a * a + b * b).sqrt(), (b.atan2(a) / (2.0 * std::f64::consts::PI)).rem_euclid(1.0)]
        }
    };
    let mut a: Vec<T<'h>> = comps.into_iter().map(|v| h.real(v)).collect();
    if c.a != 1.0 {
        a.push(h.real(c.a));
    }
    Some(h.call(h.sym(head), &a))
}

/// The model a colour term is written in (its head; `rgb_color` for named colours).
fn model_of<S: Store + ?Sized>(h: &S, t: T<'_>) -> String {
    match model::head_name(h, t) {
        Some((n, _)) if HEADS.contains(&n.as_str()) => n,
        _ => "rgb_color".into(),
    }
}

/// A colour given as a list of components: 1 a gray level, 3 red, green and blue, 4 CMYK.
fn list_colour<S: Store + ?Sized>(h: &S, t: T<'_>) -> Option<Rgba> {
    let items = model::list(h, t)?;
    let v: Vec<f64> = items.iter().map(|&x| number(h, x)).collect::<Option<_>>()?;
    match v.as_slice() {
        [g] => Some(Rgba::rgb(*g, *g, *g)),
        [r, g, b] => Some(Rgba::rgb(*r, *g, *b)),
        [c, m, y, k] => Some(Rgba::rgb((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))),
        _ => None,
    }
}

/// `is_color(x)`: whether `x` is a colour with numeric components.
struct IsColor;

impl Native for IsColor {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [x] = args else { return Ok(Reply::Decline) };
        // a theme colour (or one switched by the light or dark appearance) is a name for a colour the front end
        // chooses: not itself a colour value
        let themed = model::head_name(h, *x).is_some_and(|(n, _)| n == "theme_color" || n == "light_dark_switched");
        let yes = matches!(colour(h, *x), Colour::Value(_)) && !matches!(h.view(*x), View::Str(_)) && !themed;
        Ok(Reply::Done(h.sym_term(h.sym(if yes { names::TRUE } else { names::FALSE }))))
    }
}

/// `color_negate(c)`: the complementary colour (red, green and blue each subtracted from 1), in the model `c` is
/// written in; a list of colours elementwise.
struct ColorNegate;

impl Native for ColorNegate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [x] = args else { return Ok(Reply::Decline) };
        if let Some(cs) = cx.as_list(*x) {
            let parts: Vec<T<'h>> = cs.iter().map(|&k| h.call(h.sym("color_negate"), &[k])).collect();
            return Ok(Reply::Again(cx.list(&parts)));
        }
        let c = match colour_arg(h, "color_negate", *x, 1) {
            Ok(c) => c,
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        let n = Rgba { r: 1.0 - c.r, g: 1.0 - c.g, b: 1.0 - c.b, a: c.a };
        let m = model_of(h, *x);
        if m == "gray_level" {
            let mut a = vec![if exact_colour(h, *x) { exactly(h, 1.0 - c.r) } else { h.real(1.0 - c.r) }];
            if c.a != 1.0 {
                a.push(h.real(c.a));
            }
            return Ok(Reply::Done(h.call(h.sym("gray_level"), &a)));
        }
        if m == "rgb_color" {
            return Ok(Reply::Done(rgb_term_exact(h, n, exact_colour(h, *x))));
        }
        Ok(Reply::Done(in_model(h, n, &m).unwrap_or_else(|| rgb_term(h, n))))
    }
}

/// `color_distance(c1, c2)`: the CIE 1976 difference of two colours (Euclidean in L*a*b*, L from 0 to 1); lists
/// elementwise.
struct ColorDistance;

impl Native for ColorDistance {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [a, b, rest @ ..] = args else { return Ok(Reply::Decline) };
        // distance_function: a formula's name (a function of two L*a*b* colours is not read)
        let mut formula = mc_graphics::color::Difference::Cie76;
        for &o in rest {
            let rules = match model::head_name(h, o) {
                Some((n, &[k, v])) if n == names::RULE => match h.view(k) {
                    View::Sym(k) => vec![(h.sym_name(k), v)],
                    _ => vec![],
                },
                _ => model::rules(h, o).unwrap_or_default(),
            };
            for (k, v) in rules {
                if k == "distance_function" {
                    match h.view(v) {
                        View::Str(s) => match mc_graphics::color::Difference::named(s) {
                            Some(d) => formula = d,
                            None => return Ok(Reply::Decline),
                        },
                        View::Sym(s) if h.sym_name(s) == "automatic" => {}
                        _ => return Ok(Reply::Decline),
                    }
                }
            }
        }
        let f = h.sym("color_distance");
        match (cx.as_list(*a).map(|l| l.to_vec()), cx.as_list(*b).map(|l| l.to_vec())) {
            (Some(xs), Some(ys)) if xs.len() == ys.len() => {
                let parts: Vec<T<'h>> = xs.iter().zip(&ys).map(|(&x, &y)| h.call(f, &[&[x, y][..], rest].concat())).collect();
                return Ok(Reply::Again(cx.list(&parts)));
            }
            (Some(xs), None) => {
                let parts: Vec<T<'h>> = xs.iter().map(|&x| h.call(f, &[&[x, *b][..], rest].concat())).collect();
                return Ok(Reply::Again(cx.list(&parts)));
            }
            (None, Some(ys)) => {
                let parts: Vec<T<'h>> = ys.iter().map(|&y| h.call(f, &[&[*a, y][..], rest].concat())).collect();
                return Ok(Reply::Again(cx.list(&parts)));
            }
            _ => {}
        }
        let p = match colour_arg(h, "color_distance", *a, 1) {
            Ok(c) => c,
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        let q = match colour_arg(h, "color_distance", *b, 2) {
            Ok(c) => c,
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        Ok(Reply::Done(h.real(mc_graphics::color::difference(p, q, formula))))
    }
}

/// A colour argument: its value, `Err(None)` when it is symbolic (decline), `Err(Some(reply))` when it is wrong.
fn colour_arg<'h>(h: &'h Heap, f: &str, t: T<'h>, k: usize) -> Result<Rgba, Option<Reply<'h>>> {
    // an image value is left to the library's definitions for images
    if mc_graphics::memo::image_path(h, t).is_some() {
        return Err(None);
    }
    match colour(h, t) {
        Colour::Value(c) => Ok(c),
        Colour::Symbolic => Err(None),
        Colour::Bad => Err(Some(bad(h, f, k))),
    }
}

/// `color_convert(c, model)`: the colour `c` as `rgb_color(r, g, b)`, `hue(h, s, b)` or `gray_level(g)` (the
/// opacity appended when it is not 1).
struct ColorConvert;

impl Native for ColorConvert {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [c, m, ..] = args else { return Ok(Reply::Decline) };
        // an image value is left to the library's definitions for images
        if mc_graphics::memo::image_path(h, *c).is_some() {
            return Ok(Reply::Decline);
        }
        let model = match h.view(*m) {
            View::Sym(s) => h.sym_name(s),
            View::Str(s) => s.to_string(),
            _ => return Ok(bad(h, "color_convert", 2)),
        };
        let value = match list_colour(h, *c) {
            Some(v) => v,
            None => match colour(h, *c) {
                Colour::Value(v) => v,
                // `hue(h, s, b)` with symbolic parts in RGB: each channel b - b s max(0, min(k, 4 - k, 1)) with
                // k = mod(n + 6 h, 6), n = 5, 3, 1 for red, green, blue (the closed form of the hexcone model, Smith,
                // "Color gamut transform pairs", SIGGRAPH 1978, written without cases)
                Colour::Symbolic if matches!(model.as_str(), "rgb_color" | "RGB") => match model::head_name(h, *c) {
                    Some((n, &[hh, s, b])) if n == "hue" => {
                        let channel = |k0: i64| {
                            let k = h.call(h.sym("mod"), &[h.call(h.sym(names::ADD), &[h.int(k0), h.call(h.sym(names::MUL), &[h.int(6), hh])]), h.int(6)]);
                            let four_minus = h.call(h.sym(names::ADD), &[h.int(4), h.call(h.sym(names::MUL), &[h.int(-1), k])]);
                            let m = h.call(h.sym("max"), &[h.int(0), h.call(h.sym("min"), &[k, four_minus, h.int(1)])]);
                            h.call(h.sym(names::ADD), &[b, h.call(h.sym(names::MUL), &[h.int(-1), b, s, m])])
                        };
                        return Ok(Reply::Again(h.call(h.sym("rgb_color"), &[channel(5), channel(3), channel(1)])));
                    }
                    _ => return Ok(Reply::Decline),
                },
                Colour::Symbolic => return Ok(Reply::Decline),
                Colour::Bad => {
                    // a list of colours: each
                    if let Some(cs) = cx.as_list(*c).filter(|cs| !cs.is_empty() && cs.iter().all(|&k| matches!(colour(h, k), Colour::Value(_)) || list_colour(h, k).is_some())) {
                        let parts: Vec<T<'h>> = cs
                            .iter()
                            .map(|&k| {
                                let mut a = vec![k];
                                a.extend_from_slice(&args[1..]);
                                h.call(h.sym("color_convert"), &a)
                            })
                            .collect();
                        return Ok(Reply::Again(cx.list(&parts)));
                    }
                    return Ok(bad(h, "color_convert", 1));
                }
            },
        };
        // `white_point: "F2"` or `xyz_color(x, y, z)`: XYZ relative to that white
        if matches!(model.as_str(), "xyz_color" | "XYZ") {
            for &o in &args[2..] {
                let rules = match model::head_name(h, o) {
                    Some((n, &[k, v])) if n == names::RULE => match h.view(k) {
                        View::Sym(k) => vec![(h.sym_name(k), v)],
                        _ => vec![],
                    },
                    _ => model::rules(h, o).unwrap_or_default(),
                };
                let Some(v) = rules.into_iter().find_map(|(k, v)| (k == "white_point").then_some(v)) else { continue };
                let white = match h.view(v) {
                    View::Str(s) => mc_graphics::color::illuminant(s),
                    _ => match model::head_name(h, v) {
                        Some((n, a)) if n == "xyz_color" && a.len() >= 3 => {
                            let w: Option<Vec<f64>> = a[..3].iter().map(|&x| number(h, x)).collect();
                            w.filter(|w| w[1] > 0.0).map(|w| [w[0] / w[1], 1.0, w[2] / w[1]])
                        }
                        _ => None,
                    },
                };
                let Some(white) = white else { return Ok(bad(h, "color_convert", 3)) };
                let c = value.clamped();
                let x = mc_graphics::color::adapt_to_white(mc_graphics::color::to_xyz(c), white);
                let mut a: Vec<T<'h>> = x.into_iter().map(|v| h.real(v)).collect();
                if c.a != 1.0 {
                    a.push(h.real(c.a));
                }
                return Ok(Reply::Done(h.call(h.sym("xyz_color"), &a)));
            }
        }
        Ok(Reply::Done(match in_model(h, value.clamped(), &model) {
            Some(t) => t,
            None => return Ok(bad(h, "color_convert", 2)),
        }))
    }
}

/// `blend([c1, ..., cn])`: the average; `blend([c1, ..., cn], t)`: the colour a fraction `t` (clamped to [0, 1])
/// along c1 .. cn, evenly spaced.
struct Blend;

impl Native for Blend {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (cs, at) = match args {
            [c] => (*c, None),
            [c, t] => (*c, Some(*t)),
            _ => return Ok(Reply::Decline),
        };
        let Some(items) = cx.as_list(cs).filter(|v| !v.is_empty()) else { return Ok(bad(h, "blend", 1)) };
        // [[t1, c1], [t2, c2], ...]: colours at given values, mixed linearly between them
        if let (Some(t), true) = (at, items.iter().all(|&e| cx.as_list(e).is_some_and(|p| p.len() == 2))) {
            let mut stops = Vec::with_capacity(items.len());
            for &e in items {
                let &[u, c] = cx.as_list(e).unwrap_or(&[]) else { return Ok(bad(h, "blend", 1)) };
                let Some(u) = number(h, u) else { return Ok(bad(h, "blend", 1)) };
                match colour_arg(h, "blend", c, 1) {
                    Ok(c) => stops.push((u, c)),
                    Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
                }
            }
            stops.sort_by(|a, b| a.0.total_cmp(&b.0));
            let Some(t) = number(h, t) else {
                return Ok(if is_symbolic(h, t) { Reply::Decline } else { bad(h, "blend", 2) });
            };
            let k = stops.partition_point(|s| s.0 <= t);
            let c = if k == 0 {
                stops[0].1
            } else if k == stops.len() {
                stops[k - 1].1
            } else {
                let (a, b) = (stops[k - 1], stops[k]);
                a.1.mix(b.1, if b.0 > a.0 { (t - a.0) / (b.0 - a.0) } else { 0.0 })
            };
            return Ok(Reply::Done(rgb_term(h, c)));
        }
        let exact = items.iter().all(|&c| exact_colour(h, c))
            && at.is_none_or(|t| exact_number(h, t) || cx.as_list(t).is_some_and(|ws| ws.iter().all(|&w| exact_number(h, w))));
        let mut v = Vec::with_capacity(items.len());
        for &c in items {
            cx.tick(1)?;
            match colour_arg(h, "blend", c, 1) {
                Ok(c) => v.push(c),
                Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
            }
        }
        // [w1, w2, ...]: the mean weighted by these
        if let Some(ws) = at.and_then(|t| cx.as_list(t)) {
            let w: Option<Vec<f64>> = ws.iter().map(|&x| number(h, x).filter(|x| *x >= 0.0)).collect();
            let Some(w) = w.filter(|w| w.len() == v.len() && w.iter().sum::<f64>() > 0.0) else { return Ok(bad(h, "blend", 2)) };
            let total: f64 = w.iter().sum();
            let mut c = Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
            for (k, x) in v.iter().zip(&w) {
                c = Rgba { r: c.r + k.r * x / total, g: c.g + k.g * x / total, b: c.b + k.b * x / total, a: c.a + k.a * x / total };
            }
            return Ok(Reply::Done(rgb_term_exact(h, c, exact)));
        }
        let c = match at {
            None => {
                let n = v.len() as f64;
                let sum = v.iter().fold(Rgba { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }, |s, c| Rgba {
                    r: s.r + c.r,
                    g: s.g + c.g,
                    b: s.b + c.b,
                    a: s.a + c.a,
                });
                Rgba { r: sum.r / n, g: sum.g / n, b: sum.b / n, a: sum.a / n }
            }
            Some(t) => {
                let Some(t) = number(h, t) else {
                    return Ok(if is_symbolic(h, t) { Reply::Decline } else { bad(h, "blend", 2) });
                };
                let x = t.clamp(0.0, 1.0) * (v.len() - 1) as f64;
                let k = (x.floor() as usize).min(v.len().saturating_sub(2));
                // colours that are all CIE L*a*b* values are mixed in that space and stay one (decision r36core.2)
                let labs: Option<Vec<[f64; 3]>> = items
                    .iter()
                    .map(|&c| match model::head_name(h, c) {
                        Some((n, a)) if n == "lab_color" && a.len() == 3 => {
                            Some([number(h, a[0])?, number(h, a[1])?, number(h, a[2])?])
                        }
                        _ => None,
                    })
                    .collect();
                if let (Some(l), true) = (labs.as_ref(), v.len() > 1) {
                    let s = x - k as f64;
                    let m = |i: usize| (1.0 - s) * l[k][i] + s * l[k + 1][i];
                    return Ok(Reply::Done(h.call(h.sym("lab_color"), &[h.real(m(0)), h.real(m(1)), h.real(m(2))])));
                }
                if v.len() == 1 {
                    v[0]
                } else {
                    v[k].mix(v[k + 1], x - k as f64)
                }
            }
        };
        Ok(Reply::Done(rgb_term_exact(h, c, exact)))
    }
}

/// `lighter(c)`, `lighter(c, f)`, `darker(c)`, `darker(c, f)`: `c` a fraction `f` (default 1/3) of the way to
/// white or black.
struct Shade {
    name: &'static str,
    towards: Rgba,
}

impl Native for Shade {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (c, f) = match args {
            [c] => (*c, None),
            [c, f] => (*c, Some(*f)),
            _ => return Ok(Reply::Decline),
        };
        // a list of colours: each
        if let Some(cs) = cx.as_list(c) {
            let parts: Vec<T<'h>> = cs.iter().map(|&k| {
                let mut a = vec![k];
                a.extend(f);
                h.call(h.sym(self.name), &a)
            }).collect();
            return Ok(Reply::Again(cx.list(&parts)));
        }
        let f_exact = f.is_none_or(|f| exact_number(h, f));
        let exact = exact_colour(h, c) && f_exact;
        // an opacity given as the fourth component is kept as it was given
        let alpha = model::head_name(h, c).and_then(|(n, a)| (n == "rgb_color" && a.len() == 4 && a[..3].iter().all(|&x| exact_number(h, x))).then(|| a[3]));
        let c = match colour_arg(h, self.name, c, 1) {
            Ok(c) => c,
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        let f = match f {
            None => 1.0 / 3.0,
            Some(f) => match number(h, f) {
                // past the ends: clamped (nothing beyond white or black)
                Some(f) => f.clamp(0.0, 1.0),
                None if is_symbolic(h, f) => return Ok(Reply::Decline),
                _ => return Ok(bad(h, self.name, 2)),
            },
        };
        let m = c.mix(Rgba { a: c.a, ..self.towards }, f);
        if let (Some(a), true) = (alpha, f_exact) {
            let View::App { args, .. } = h.view(rgb_term_exact(h, Rgba { a: 1.0, ..m }, true)) else { return Ok(Reply::Decline) };
            let mut v = args.to_vec();
            v.push(a);
            return Ok(Reply::Done(h.call(h.sym("rgb_color"), &v)));
        }
        Ok(Reply::Done(rgb_term_exact(h, m, exact)))
    }
}

/// `graphics_complex(points, content)`: the content with each coordinate given as an index into the points
/// replaced by that point (so it draws as ordinary primitives).
struct GraphicsComplex;

impl Native for GraphicsComplex {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [pts, content, ..] = args else { return Ok(Reply::Decline) };
        let Some(pts) = cx.as_list(*pts).map(|l| l.to_vec()) else { return Ok(Reply::Decline) };
        fn indices<'h>(cx: &NativeCx<'_, '_, 'h>, pts: &[T<'h>], t: T<'h>) -> Option<T<'h>> {
            let h = cx.heap();
            match h.view(t) {
                View::Int(k) if k >= 1 && (k as usize) <= pts.len() => Some(pts[k as usize - 1]),
                _ => {
                    let items = cx.as_list(t)?;
                    let out: Vec<T<'h>> = items.iter().map(|&x| indices(cx, pts, x)).collect::<Option<_>>()?;
                    Some(cx.list(&out))
                }
            }
        }
        fn walk<'h>(cx: &NativeCx<'_, '_, 'h>, pts: &[T<'h>], t: T<'h>) -> T<'h> {
            let h = cx.heap();
            if let Some(items) = cx.as_list(t) {
                let out: Vec<T<'h>> = items.iter().map(|&x| walk(cx, pts, x)).collect();
                return cx.list(&out);
            }
            let View::App { head, args } = h.view(t) else { return t };
            let View::Sym(s) = h.view(head) else { return t };
            let name = h.sym_name(s);
            if name == "style" && !args.is_empty() {
                let mut a = args.to_vec();
                a[0] = walk(cx, pts, a[0]);
                return h.app(head, &a);
            }
            let positional = PRIMITIVES.contains(&name.as_str()) || mc_graphics::three::PRIMITIVES.contains(&name.as_str()) || name == "arrow";
            if positional && !args.is_empty() && name != "text" {
                if let Some(p) = indices(cx, pts, args[0]) {
                    let mut a = args.to_vec();
                    a[0] = p;
                    return h.app(head, &a);
                }
            }
            if name == "text" && args.len() >= 2 {
                if let Some(p) = indices(cx, pts, args[1]) {
                    let mut a = args.to_vec();
                    a[1] = p;
                    return h.app(head, &a);
                }
            }
            t
        }
        Ok(Reply::Done(walk(cx, &pts, *content)))
    }
}

/// `circle_points(n)`, `circle_points(r, n)`, `circle_points(c, r, n)`: n points equally spaced on a circle,
/// the first at angle -pi/2 + pi/n (exact), or at the angle theta when the radius is written `[r, theta]`;
/// `regular_polygon(...)` the polygon through them.
struct CirclePoints(bool);

impl Native for CirclePoints {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (c, r, n) = match args {
            [n] => (None, h.int(1), *n),
            [r, n] => (None, *r, *n),
            [c, r, n] => (Some(*c), *r, *n),
            _ => return Ok(Reply::Decline),
        };
        let View::Int(k) = h.view(n) else { return Ok(Reply::Decline) };
        if !(1..=10_000).contains(&k) {
            return Ok(Reply::Decline);
        }
        let call = |f: &str, a: &[T<'h>]| h.call(h.sym(f), a);
        let pi = h.sym_term(h.sym("pi"));
        // a radius `[r, theta]`: the radius and the angle of the first point (R20-08)
        let (r, start) = match cx.as_list(r) {
            Some(&[r, theta]) => (r, Some(theta)),
            _ => (r, None),
        };
        let center = c.and_then(|c| cx.as_list(c).map(|l| l.to_vec())).filter(|l| l.len() == 2);
        let mut pts = Vec::with_capacity(k as usize);
        for j in 0..k {
            // -pi/2 + pi/n + 2 pi j/n
            let frac = |p: i64, q: i64| call(names::MUL, &[h.int(p), call(names::POW, &[h.int(q), h.int(-1)])]);
            let ang = match start {
                Some(theta) => call(names::ADD, &[theta, call(names::MUL, &[frac(2 * j, k), pi])]),
                None => call(names::ADD, &[call(names::MUL, &[frac(-1, 2), pi]), call(names::MUL, &[frac(1 + 2 * j, k), pi])]),
            };
            let mut x = call(names::MUL, &[r, call("cos", &[ang])]);
            let mut y = call(names::MUL, &[r, call("sin", &[ang])]);
            if let Some(cc) = &center {
                x = call(names::ADD, &[cc[0], x]);
                y = call(names::ADD, &[cc[1], y]);
            }
            pts.push(cx.list(&[x, y]));
        }
        let l = cx.list(&pts);
        Ok(Reply::Again(if self.0 { call("polygon", &[l]) } else { l }))
    }
}

/// The registrations that wrap natives of crates installed after this one; call once all crates are in.
pub fn install_late(k: &mut Kernel, h: &Heap) {
    crate::transform::install_late(k, h);
}
