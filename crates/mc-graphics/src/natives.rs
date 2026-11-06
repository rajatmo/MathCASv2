//! The natives: checks of primitives and directives, `graphics` (numeric constants made numbers, the whole term
//! checked), `show`, and the colour functions.
use crate::color::{colour, hex_color, is_symbolic, Colour, Rgba, HEADS};
use crate::model::{self, Bad, Style, DIRECTIVES, PRIMITIVES};
use crate::num::number;
use crate::Failure;
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
    crate::compose::install(k, h);
}

/// `gradient_color(g, t)`: the colour of gradient `g` at `t` (clamped to [0, 1]); a symbolic `t` leaves it.
struct GradientColor;

impl Native for GradientColor {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [g, t] = args else { return Ok(Reply::Decline) };
        let Some(grad) = crate::gradient::read(h, *g) else { return Ok(bad(h, "gradient_color", 1)) };
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
                if matches!((self.0, k), ("point" | "line" | "polygon" | "arrow" | "bezier_curve", 1) | ("text", 2))
                    && args.get(k - 1).is_some_and(|&a| crate::three::is_coords(h, a)) =>
            {
                Reply::Decline
            }
            // several centres: each is checked when the graphic is read
            Err(Bad::Arg(1)) if matches!(self.0, "disk" | "circle") && model::several_centres(h, args) => Reply::Decline,
            Err(Bad::Arg(k)) => bad(h, self.0, k),
            _ => Reply::Decline,
        })
    }
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
            if let Some(items) = cx.as_list(*l).filter(|v| (1..=4).contains(&v.len())) {
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
                        || crate::three::PRIMITIVES.contains(&n.as_str())
                        || PRIMITIVES.contains(&n.as_str())
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
        if crate::color::named(&n).is_some() || matches!(n.as_str(), "none" | "automatic" | "all" | "full" | "true" | "false") {
            return Ok(t);
        }
    }
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(if matches!(h.view(v), View::Real(x) if x.is_finite()) { v } else { t })
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
        }
        let own_range = rules.iter().any(|(k, _)| k == "plot_range");
        let mut graphic_opts = vec![];
        let mut legends: Vec<T<'h>> = vec![];
        for (i, g) in flat {
            cx.tick(1)?;
            if let View::App { args: ga, .. } = h.view(g) {
                graphic_opts.push(ga.get(1).copied());
                if let Some(Some(l)) = ga.get(1).map(|&o| model::rules(h, o).ok().and_then(|rs| rs.into_iter().find(|(k, _)| k == "legend").map(|r| r.1))) {
                    legends.extend(cx.as_list(l).unwrap_or(&[]).iter().copied());
                }
            }
            let View::App { head, args: ga } = h.view(g) else { return Ok(bad(h, "show", i + 1)) };
            if head != h.sym_term(h.sym("graphics")) || ga.is_empty() || ga.len() > 2 {
                return Ok(bad(h, "show", i + 1));
            }
            contents.push(ga[0]);
            if let Some(&o) = ga.get(1) {
                if let Err(f) = add_rules(o, &mut rules) {
                    return Ok(Reply::Done(f.term(h)));
                }
            }
        }
        // combined plots: the union of their ranges, all their legends
        if !own_range && graphic_opts.len() > 1 {
            if let Some([x0, x1, y0, y1]) = crate::compose::union_range(h, &graphic_opts) {
                let range = cx.list(&[h.reals(&[x0, x1]), h.reals(&[y0, y1])]);
                rules.retain(|(k, _)| k != "plot_range");
                rules.push(("plot_range".into(), range));
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
        Ok(Reply::Again(h.call(h.sym("graphics"), &a)))
    }
}

/// A colour argument: its value, `Err(None)` when it is symbolic (decline), `Err(Some(reply))` when it is wrong.
fn colour_arg<'h>(h: &'h Heap, f: &str, t: T<'h>, k: usize) -> Result<Rgba, Option<Reply<'h>>> {
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
        let &[c, m] = args else { return Ok(Reply::Decline) };
        let c = match colour_arg(h, "color_convert", c, 1) {
            Ok(c) => c.clamped(),
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        let View::Sym(m) = h.view(m) else { return Ok(bad(h, "color_convert", 2)) };
        let alpha = |mut v: Vec<T<'h>>| {
            if c.a != 1.0 {
                v.push(h.real(c.a));
            }
            v
        };
        Ok(Reply::Done(match h.sym_name(m).as_str() {
            "rgb_color" => rgb_term(h, c),
            "hue" => {
                let (hu, s, b) = c.hsb();
                h.call(h.sym("hue"), &alpha(vec![h.real(hu), h.real(s), h.real(b)]))
            }
            "gray_level" => h.call(h.sym("gray_level"), &alpha(vec![h.real(c.gray())])),
            _ => return Ok(bad(h, "color_convert", 2)),
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
            return Ok(Reply::Done(rgb_term(h, c)));
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
                if v.len() == 1 {
                    v[0]
                } else {
                    v[k].mix(v[k + 1], x - k as f64)
                }
            }
        };
        Ok(Reply::Done(rgb_term(h, c)))
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
        let c = match colour_arg(h, self.name, c, 1) {
            Ok(c) => c,
            Err(r) => return Ok(r.unwrap_or(Reply::Decline)),
        };
        let f = match f {
            None => 1.0 / 3.0,
            Some(f) => match number(h, f) {
                Some(f) if (0.0..=1.0).contains(&f) => f,
                None if is_symbolic(h, f) => return Ok(Reply::Decline),
                _ => return Ok(bad(h, self.name, 2)),
            },
        };
        Ok(Reply::Done(rgb_term(h, c.mix(Rgba { a: c.a, ..self.towards }, f))))
    }
}
