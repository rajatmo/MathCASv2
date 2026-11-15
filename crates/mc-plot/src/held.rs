//! Plots hold their functions. A function plot evaluates its first argument as before, but an expression that
//! evaluates to a failure with a symbolic variable (`n_integrate(f(x, r), [r, 0, 1])` with `x` the plot
//! variable) is kept as written, so the plot evaluates it at each sample point instead. In a list of functions
//! each element is kept or replaced on its own.
use mc_core::{Heap, Raw, Store, View, T};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};

/// The attributes of a function plot: the first argument held (and evaluated by [`Held`]).
fn held_attributes() -> Attributes {
    Attributes { hold: Hold::First, ..Attributes::default() }
}

/// Register `native` as `name` holding its first argument.
pub(crate) fn register(k: &mut Kernel, h: &Heap, name: &str, native: Box<dyn Native>) {
    k.register(h.sym(name), held_attributes(), Box::new(Held(native, name.to_string())));
}

/// A plot native with its first argument held.
pub(crate) struct Held(Box<dyn Native>, String);

fn is_failure<'h>(cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
    let h = cx.heap();
    h.packed(t).is_none() && matches!(h.view(t), View::App { head, .. } if head == h.sym_term(h.sym("failure")))
}

impl Native for Held {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // monitors take a function (design NUM.8): a value that is not one is a failure showing the form to write. The
        // evaluation monitor is called at every sample with the sample point (one argument per variable); a plot
        // takes no steps, so a step monitor is an unknown option, never accepted and ignored
        let (args, mons) = match mc_numfn::take_monitors(cx, args, &mc_numfn::MONITOR_OPTIONS) {
            Ok(v) => v,
            Err(f) => return Ok(Reply::Done(f)),
        };
        if mons.steps() {
            let h = cx.heap();
            return Ok(Reply::Done(unknown_option(h, "step_monitor")));
        }
        if mons.any() {
            let before = MONITORS.with(|m| m.replace(Some((mons.clone(), false))));
            let r = self.sampled(cx, t, &args);
            let (_, attached) = MONITORS.with(|m| m.replace(before)).unwrap_or_default();
            let r = r?;
            let h = cx.heap();
            // a failure of the monitor ends the plot and is its value
            if let Some(f) = mons.failure(h) {
                return Ok(Reply::Done(f));
            }
            // a plot that samples no function of its own (or not through the sampler) has no place to call it
            if !attached {
                return Ok(Reply::Done(unknown_option(h, "evaluation_monitor")));
            }
            return Ok(r);
        }
        let before = MONITORS.with(|m| m.replace(None));
        let r = self.sampled(cx, t, &args);
        MONITORS.with(|m| m.replace(before));
        r
    }
    fn trace(&mut self, visit: &mut dyn FnMut(&mut Raw)) {
        self.0.trace(visit)
    }
    fn reads_term(&self) -> bool {
        self.0.reads_term()
    }
    fn curried(&self) -> bool {
        self.0.curried()
    }
}

thread_local! {
    /// The monitors of the plot being drawn on this thread, and whether a sampled function took them.
    static MONITORS: std::cell::RefCell<Option<(mc_numfn::Monitors, bool)>> = const { std::cell::RefCell::new(None) };
}

/// `failure(unknown_option, name)`.
fn unknown_option<'h>(h: &'h Heap, name: &str) -> T<'h> {
    h.call(h.sym("failure"), &[h.sym_term(h.sym("unknown_option")), h.sym_term(h.sym(name))])
}

/// The function `expr` of `vars` at machine reals, as the plots sample it: [`mc_numfn::compile_expr`], with the
/// evaluation monitor of the plot being drawn (if any) called at every evaluation with the sample point.
pub(crate) fn compile<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    expr: T<'h>,
    vars: &[mc_numfn::Var],
    opts: &mc_numfn::Options,
) -> Res<Box<dyn mc_numfn::NumFn<'h, f64> + 'h>> {
    let mons = MONITORS.with(|m| {
        let mut m = m.borrow_mut();
        m.as_mut().map(|(mons, attached)| {
            *attached = true;
            mons.clone()
        })
    });
    match mons {
        Some(mons) => mc_numfn::compile_expr_monitored(cx, expr, vars, opts, &mons),
        None => mc_numfn::compile_expr(cx, expr, vars, opts),
    }
}

impl Held {
    /// The plot with its function evaluated (or kept, see the module docs) and infinite windows made finite.
    fn sampled<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let Some((&f, rest)) = args.split_first() else {
            return self.0.call(cx, t, args);
        };
        let v = cx.eval(f)?;
        let first = if is_failure(cx, v) {
            f
        } else {
            match (cx.as_list(f), cx.as_list(v)) {
                (Some(fs), Some(vs)) if fs.len() == vs.len() && vs.iter().any(|&e| is_failure(cx, e)) => {
                    let items: Vec<T<'h>> =
                        fs.iter().zip(vs).map(|(&held, &e)| if is_failure(cx, e) { held } else { e }).collect();
                    cx.list(&items)
                }
                _ => v,
            }
        };
        let mut all = Vec::with_capacity(args.len());
        all.push(first);
        for &r in rest {
            all.push(finite_window(cx, r));
        }
        // a domain `element([x, y], region)` in place of the ranges: the region's bounding box, and the function
        // taken only where the region holds
        if let Some(&domain) = rest.first() {
            if let Some((specs, cond)) = region_domain(cx, domain)? {
                let h = cx.heap();
                let masked = |cx: &mut NativeCx<'_, '_, 'h>, f: T<'h>| -> T<'h> {
                    match (self.1.as_str(), cond) {
                        (_, None) => f,
                        ("region_plot" | "complex_region_plot", Some(c)) => h.call(h.sym("and"), &[f, c]),
                        ("plot" | "plot3d" | "contour_plot" | "density_plot" | "re_im_plot" | "abs_arg_plot" | "discrete_plot", Some(c)) => {
                            let _ = &cx;
                            h.call(h.sym("piecewise"), &[cx.list(&[cx.list(&[f, c])]), h.sym_term(h.sym("indeterminate"))])
                        }
                        _ => f,
                    }
                };
                all[0] = match cx.as_list(first).map(|l| l.to_vec()) {
                    Some(fs) if !matches!(self.1.as_str(), "parametric_plot" | "parametric_plot3d" | "polar_plot") => {
                        let items: Vec<T<'h>> = fs.into_iter().map(|f| masked(cx, f)).collect();
                        cx.list(&items)
                    }
                    _ => masked(cx, first),
                };
                all.splice(1..2, specs);
            }
        }
        self.0.call(cx, t, &all)
    }
}

/// `+1` for the symbol `inf`, `-1` for `-inf`, `0` for anything else.
fn infinity_sign<'h>(h: &'h Heap, t: T<'h>) -> i8 {
    match mc_syntax::Printer::new(h).expr(t).as_str() {
        "inf" => 1,
        "-inf" => -1,
        _ => 0,
    }
}

/// A range `[x, a, b]` with an infinite end drawn over a finite window: `[x, -inf, inf]` as `[x, -10, 10]`, `[x,
/// a, inf]` as `[x, a, a + 10]`, `[x, -inf, b]` as `[x, b - 10, b]` (a plot cannot show more than a window of the
/// whole line; the window is the one a reader gets from `[x, -10, 10]`). Any other argument as it is.
fn finite_window<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> T<'h> {
    let h = cx.heap();
    let Some(&[v, a, b]) = cx.as_list(t) else { return t };
    if !matches!(h.view(v), View::Sym(_)) {
        return t;
    }
    const W: i64 = 10;
    let (sa, sb) = (infinity_sign(h, a), infinity_sign(h, b));
    let (lo, hi) = match (sa, sb) {
        (-1, 1) => (h.int(-W), h.int(W)),
        (-1, 0) => (h.call(h.sym(mc_api::names::ADD), &[b, h.int(-W)]), b),
        (0, 1) => (a, h.call(h.sym(mc_api::names::ADD), &[a, h.int(W)])),
        _ => return t,
    };
    cx.list(&[v, lo, hi])
}

/// A domain `element(x, region)` or `element([x, y, ...], region)`: the ranges `[x, lo, hi]` of the region's
/// bounding box, one per variable, and the condition that a point is in the region when it can be written down
/// (`None` when the box is all there is to use). `None` for anything else.
fn region_domain<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<(Vec<T<'h>>, Option<T<'h>>)>> {
    let h = cx.heap();
    let Some((n, &[vars, reg])) = crate::values::call_of(h, t) else { return Ok(None) };
    if n != "element" {
        return Ok(None);
    }
    let names: Vec<T<'h>> = match cx.as_list(vars) {
        Some(l) => l.to_vec(),
        None => vec![vars],
    };
    if names.is_empty() || names.len() > 3 || !names.iter().all(|&v| matches!(h.view(v), View::Sym(_))) {
        return Ok(None);
    }
    // an interval `interval([a, b])` as a region on the line: its ends; any other region: its numerical bounds
    let bounds = if let (Some(("interval", &[ab])), 1) = (crate::values::call_of(h, reg).as_ref().map(|(n, a)| (n.as_str(), *a)), names.len()) {
        match cx.as_list(ab) {
            Some(&[a, b]) => Some(cx.list(&[cx.list(&[a, b])])),
            _ => None,
        }
    } else {
        Some(cx.eval(h.call(h.sym("region_bounds"), &[reg]))?)
    };
    let Some(bounds) = bounds.and_then(|b| cx.as_list(b).map(|l| l.to_vec())) else { return Ok(None) };
    if bounds.len() != names.len() {
        return Ok(None);
    }
    let mut specs = Vec::with_capacity(names.len());
    for (&v, &b) in names.iter().zip(&bounds) {
        let Some(&[lo, hi]) = cx.as_list(b) else { return Ok(None) };
        if crate::real(cx, lo)?.is_none() || crate::real(cx, hi)?.is_none() {
            return Ok(None);
        }
        specs.push(cx.list(&[v, lo, hi]));
    }
    let point = if names.len() == 1 { names[0] } else { cx.list(&names) };
    let member = cx.eval(h.call(h.sym("region_member"), &[reg, point]))?;
    // `element(x | y, reals) and c`: the condition c
    let cond = match crate::values::call_of(h, member) {
        Some((n, args)) if n == "and" => {
            let keep: Vec<T<'h>> = args.iter().copied().filter(|&a| !matches!(crate::values::call_of(h, a), Some((e, _)) if e == "element")).collect();
            match keep.len() {
                0 => None,
                1 => Some(keep[0]),
                _ => Some(h.call(h.sym("and"), &keep)),
            }
        }
        Some((n, _)) if n == "region_member" => None,
        _ => Some(member),
    };
    Ok(Some((specs, cond)))
}
