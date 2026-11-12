//! `function_discontinuities`, `function_singularities`, `function_continuous`, `function_analytic`,
//! `function_meromorphic` (decision R25P.4) on the singular sets of [`super::sets`].
//!
//! - Discontinuities and singularities: the disjunction of the atoms (with the call's condition conjoined).
//! - Continuous, analytic: `true` when the discontinuities (singularities) miss the condition's region, `false`
//!   when they meet it; with parameters `conditional_expression(true, c)` with the condition `c` under which they
//!   miss it.
//! - Meromorphic (over the complexes): the same question for the atoms that are not poles.
use super::sets::{self, Kind, Meet, Mode};
use super::spec::{self, Dom, Spec};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The facts of the call for the atom walk: the parameter facts and the condition.
fn walk_facts<'h>(s: &Spec<'h>) -> Vec<T<'h>> {
    let mut f = s.facts.clone();
    if let Some(c) = s.cond {
        f.push(c);
    }
    f
}

/// `function_discontinuities` (`mode` Disc) and `function_singularities` (Sing).
pub fn set_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, mode: Mode) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if s.dom == Dom::Integers {
        return Ok(None);
    }
    let Some(atoms) = sets::atoms(cx, b, &s.comps, &s.vars, s.complex(), mode, &walk_facts(s))? else { return Ok(None) };
    let ts: Vec<T<'h>> = atoms.iter().map(|a| a.t).collect();
    let d = sets::disjunction(cx, b, &ts, &s.vars, s.complex())?;
    // one real variable: the variable on the left of a bound (`x >= 1`), the two sides of a point kept apart
    let d = if !s.complex() && s.vars.len() == 1 { sets::tidy_cond_split(cx, b, d)? } else { d };
    let Some(c) = s.cond else { return Ok(Some(d)) };
    if cx.is_false(d) {
        return Ok(Some(d));
    }
    let both = cx.eval(h.call(h.sym("and"), &[c, d]))?;
    if !s.complex() && s.vars.len() == 1 {
        let r = cx.eval(h.call(h.sym("reduce"), &[both, s.vars[0], h.sym_term(h.sym("reals"))]))?;
        if !sets::is_head(b, r, "reduce") && !sets::is_head(b, r, mc_api::names::FAILURE) {
            return Ok(Some(r));
        }
    }
    Ok(Some(both))
}

/// `function_continuous` (Disc), `function_analytic` (Sing) and, with `meromorphic`, `function_meromorphic`.
pub fn free_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, mode: Mode, meromorphic: bool) -> Res<Option<T<'h>>> {
    if s.dom == Dom::Integers || s.speed && !spec::params(b, &s.comps, &s.vars).is_empty() {
        return Ok(None);
    }
    // `a g(x)` with a parameter factor `a`: the properties of `g`, which hold for `a == 0` (the zero function) too
    let (comps, factors) = split_factors(cx, b, s)?;
    let Some((atoms, partial)) = sets::atoms_ext(cx, b, &comps, &s.vars, s.complex(), mode, &walk_facts(s))? else { return Ok(None) };
    let mut ts: Vec<T<'h>> = atoms.iter().filter(|a| !meromorphic || a.kind != Kind::Pole).map(|a| a.t).collect();
    if partial && ts.is_empty() {
        return Ok(None);
    }
    if meromorphic {
        // an entire or meromorphic transcendental function of a function with a pole: essential singularity
        let facts = walk_facts(s);
        let mut subs = vec![];
        for &f in &comps {
            transcendental_calls(b, f, &s.vars, &mut subs);
        }
        for u in subs {
            let Some(inner) = sets::atoms(cx, b, &[u], &s.vars, s.complex(), Mode::Sing, &facts)? else { return Ok(None) };
            for a in inner.iter().filter(|a| a.kind == Kind::Pole) {
                if !ts.contains(&a.t) && genuine_pole(cx, b, u, &s.vars, a.t)? {
                    ts.push(a.t);
                }
            }
        }
    }
    let r = sets::meets(cx, b, s, &ts, s.cond)?;
    let mut all = s.comps.clone();
    if let Some(c) = s.cond {
        all.push(c);
    }
    let ps = spec::params(b, &all, &s.vars);
    // some singular points only: they decide that the set meets the region, not that it misses it
    if partial && !matches!(r, Meet::Yes) {
        return Ok(None);
    }
    Ok(match r {
        Meet::Yes if !factors.is_empty() => {
            let f = cx.bool(false);
            let ne: Vec<T<'h>> = factors.iter().map(|&a| b.call("unequal", &[a, b.zero()])).collect();
            let c = cx.eval(b.call("and", &ne))?;
            spec::conditional(cx, b, s, f, c, &ps)?
        }
        Meet::Yes => Some(cx.bool(false)),
        Meet::No => Some(cx.bool(true)),
        Meet::NoWhen(c) => {
            let t = cx.bool(true);
            spec::conditional(cx, b, s, t, c, &ps)?
        }
        Meet::Unknown => None,
    })
}

/// The arguments `u` of the calls `F(u)` of `t` in which `F` is a transcendental function (entire, or with poles)
/// and `u` depends on a variable: a pole of `u` is an essential singularity of `F(u)` (decision F27.2).
fn transcendental_calls<'h>(b: &Build<'h>, t: T<'h>, vars: &[T<'h>], out: &mut Vec<T<'h>>) {
    let h = b.heap();
    let dep = |u: T<'h>| vars.iter().any(|&x| !crate::common::is_free(b, u, x));
    let View::App { head, args } = h.view(t) else { return };
    if let Some(s) = b.sym_of(head) {
        let name = h.sym_name(s).to_string();
        let last = args.last().copied();
        let is_t = if s == b.hd.pow {
            // c^u for a constant base
            args.len() == 2 && !dep(args[0]) && dep(args[1])
        } else {
            matches!(
                name.as_str(),
                "exp" | "sin" | "cos" | "sinh" | "cosh" | "tan" | "cot" | "sec" | "csc" | "tanh" | "coth" | "sech" | "csch" | "erf" | "erfc" | "erfi" | "gamma"
                    | "airy_ai" | "airy_bi" | "bessel_j" | "bessel_i" | "dawson_f" | "fresnel_c" | "fresnel_s" | "sin_integral" | "sinh_integral" | "sinc"
            )
        };
        if is_t {
            if let Some(u) = if s == b.hd.pow { Some(args[1]) } else { last } {
                if dep(u) {
                    out.push(u);
                }
            }
        }
    }
    for &a in args {
        transcendental_calls(b, a, vars, out);
    }
}

/// Is the pole atom `a` of `u` a pole (and not a removable point such as `x = 0` for `sin(x)/x`)? One variable:
/// the solutions of the atom are checked by the growth of `|u|` towards them; otherwise accepted.
fn genuine_pole<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, vars: &[T<'h>], a: T<'h>) -> Res<bool> {
    use mc_api::names;
    let h = b.heap();
    let [x] = vars else { return Ok(true) };
    let sol = cx.eval(b.call("solve", &[a, *x, h.sym_term(h.sym("complexes"))]))?;
    let Some(items) = cx.as_list(sol).map(<[T<'h>]>::to_vec) else { return Ok(true) };
    if items.is_empty() {
        return Ok(true);
    }
    let gen = h.sym("c");
    let imu = h.sym_term(h.sym("im"));
    for it in items {
        let Some(&[r]) = cx.as_list(it) else { return Ok(true) };
        let View::App { args: &[_, p], .. } = h.view(r) else { return Ok(true) };
        // a family (c(1)): the member 0
        let p = cx.eval(h.call(h.sym("rewrite"), &[p, h.call(h.sym(names::RULE), &[h.call(gen, &[b.one()]), b.zero()])]))?;
        let mut sizes = vec![];
        for e in [-6i64, -9] {
            let eps = b.mul(&[b.pow(b.int(10), b.int(e)), b.add(&[b.rat(4, 5), b.mul(&[b.rat(3, 5), imu])])]);
            let pt = cx.eval(b.add(&[p, eps]))?;
            let v = cx.eval(h.call(h.sym("rewrite"), &[u, h.call(h.sym(names::RULE), &[*x, pt])]))?;
            let m = cx.eval(h.call(h.sym("n"), &[b.call("abs", &[v])]))?;
            match h.view(m) {
                View::Real(r) => sizes.push(r),
                View::Int(k) => sizes.push(k as f64),
                _ => return Ok(true),
            }
        }
        if sizes[0] > 1e3 && sizes[1] > 5.0 * sizes[0] {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A single component `a g` with factors `a` free of the variables that are not numbers: (`[g]`, the factors).
fn split_factors<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>) -> Res<(Vec<T<'h>>, Vec<T<'h>>)> {
    let h = b.heap();
    let [f] = s.comps.as_slice() else { return Ok((s.comps.clone(), vec![])) };
    let View::App { head, args } = h.view(*f) else { return Ok((s.comps.clone(), vec![])) };
    if b.sym_of(head) != Some(b.hd.mul) {
        return Ok((s.comps.clone(), vec![]));
    }
    let free = |t: T<'h>| s.vars.iter().all(|&x| crate::common::is_free(b, t, x));
    let (fs, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| matches!(h.view(a), View::Sym(_)) && free(a) && b.ar.num(a).is_none() && !spec::params(b, &[a], &s.vars).is_empty());
    if fs.is_empty() || rest.is_empty() {
        return Ok((s.comps.clone(), vec![]));
    }
    let g = cx.eval(b.mul(&rest))?;
    Ok((vec![g], fs))
}
