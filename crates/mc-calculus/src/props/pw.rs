//! `piecewise_expand(f)`, `piecewise_expand(f, cond)` (decision R25P.10): nested piecewise functions, `if`,
//! `max`, `min`, `clip`, `unit_step`, `boole`, `abs` and `sign` as one `piecewise([[v1, c1], ...], d)`;
//! `floor`, `ceiling`, `round`, `integer_part` and `prime_pi` of one variable inside a bounded condition.
//!
//! - **Exclusive pieces** when every condition is a relation (or a connective of relations): each case's
//!   condition is conjoined with the negations of the cases before it (`max(a, b, c)`: `a` where `a - b >= 0 and
//!   a - c >= 0`, `b` where `a - b < 0 and b - c >= 0`, else `c`); sums, products and functions of piecewise
//!   terms take the product of their cases, dropping the empty ones. Conditions in one variable are reduced
//!   (`x > 0 and x - 1 >= 0` is `x >= 1`), and bounds that the call's condition implies are left out; piecewise
//!   functions inside a condition are expanded too. Pieces with equal values merge (`or`), the pieces are in the
//!   order of their values (`order_values`), and the default (the value of `piecewise` where no condition holds)
//!   is 0 when that is a value, otherwise the last value (with symbol conditions the input's default); its piece is
//!   left out. Wave values are factored.
//! - **Ordered pieces** when a condition is a symbol (`piecewise([[piecewise([[4, a]], 2), b]], 3)`): the
//!   first-match order is kept and each piece's condition is the conjunction along its path (`[[4, b and a],
//!   [2, b]], 3`).
//! - Step functions inside a bounded condition in one variable: their jumps (where the argument crosses an
//!   integer, or the primes for `prime_pi`) cut the interval; on each piece the step function is the constant
//!   it takes at the piece's midpoint (exact arithmetic), and the side a jump point belongs to is found by
//!   evaluating at it.
//! - The other step and piecewise-linear functions of a real variable: `ramp(u)` is `u` where `u >= 0`;
//!   `real_abs(u)` as `abs`; `real_sign(u)` and `sign(u)` of a polynomial are `-1`, `0`, `1` by the sign of `u`;
//!   `unit_box(u)` is `1` on `-1/2 <= u <= 1/2`; `unit_triangle(u)` is `1 + u` on `[-1, 0)` and `1 - u` on
//!   `[0, 1]`; the forms of several arguments are the products of the one-argument forms. `floor(u, a)`,
//!   `ceiling(u, a)`, `round(u, a)` are `a` times the step of `u/a`; `fractional_part(u)` is `u - integer_part(u)`;
//!   `quotient(m, n)` and `mod(m, n)` for a nonzero number `n` are `floor(m/n)` and `m - n floor(m/n)` (the
//!   definitions, Graham, Knuth and Patashnik, Concrete Mathematics, 3.1 and 3.4), so their jumps are found as
//!   those of `floor`; for a variable `n` the same, and `floor(u)` of several variables is cut where u crosses
//!   the integers between its least and greatest value in the region (`0 <= k <= 3 m`: `k/m` from 0 to 3), the
//!   conditions written on u.
use super::sets::{self, is_head};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A case: value and condition.
type Case<'h> = (T<'h>, T<'h>);

/// The `method:` suboptions read.
#[derive(Default)]
struct Opts<'h> {
    /// `orderless_conditions -> true`: exclusive conditions even with symbols
    orderless: bool,
    /// `boolean_result -> true`: a disjunction instead of a piecewise
    boolean: bool,
    /// `condition_simplifier -> reduce`: every condition reduced
    reduce: bool,
    /// `value_simplifier -> f`: `f` applied to every value
    values: Option<T<'h>>,
}

/// Read `method: [name -> value, ...]`.
fn read_method<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, o: &mut Opts<'h>) {
    let h = b.heap();
    let Some(rules) = cx.as_list(v) else { return };
    for &r in rules {
        let View::App { args: &[k, val], .. } = h.view(r) else { continue };
        // the name as a symbol or a string (`"ValueSimplifier"` is `value_simplifier`)
        let name = match h.view(k) {
            View::Sym(name) => h.sym_name(name).to_string(),
            View::Str(s) => snake(s),
            _ => continue,
        };
        match name.as_str() {
            "orderless_conditions" => o.orderless = cx.is_true(val),
            "boolean_result" => o.boolean = cx.is_true(val),
            "condition_simplifier" => o.reduce = val == h.sym_term(h.sym("reduce")),
            "value_simplifier" if !cx.is_false(val) && val != h.sym_term(h.sym("none")) => o.values = Some(val),
            _ => {}
        }
    }
}

/// `ValueSimplifier` as `value_simplifier`.
fn snake(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// `piecewise_expand` for its arguments.
pub fn expand<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let mut args = args.to_vec();
    let mut region: Vec<T<'h>> = cx.assumptions().to_vec();
    let mut opts = Opts::default();
    if let Some(&last) = args.last() {
        if is_head(b, last, "options") {
            args.pop();
            let View::App { args: rules, .. } = h.view(last) else { return Ok(None) };
            let mut assumed = vec![];
            for &r in rules {
                if let View::App { args: &[k, v], .. } = h.view(r) {
                    if k == h.sym_term(h.sym("assumptions")) {
                        assumed.push(v);
                    } else if k == h.sym_term(h.sym("method")) {
                        read_method(cx, b, v, &mut opts);
                    }
                }
            }
            // assumptions: replaces the context's facts
            if !assumed.is_empty() {
                region = assumed;
            }
        }
    }
    let (f, cond) = match args.as_slice() {
        [f] => (*f, None),
        [f, c] => (*f, Some(*c)),
        [f, c, d] if *d == h.sym_term(h.sym("reals")) => (*f, Some(*c)),
        _ => return Ok(None),
    };
    // the waves as floor expressions (defined in the calculus library); their values are then factored
    let mut f = f;
    let mut waves = false;
    let wave_heads = ["sawtooth_wave", "square_wave", "triangle_wave"];
    if wave_heads.iter().any(|n| h.mask(f) & h.mask(h.sym_term(h.sym(n))) != 0) {
        // (the mask can match by chance: nothing to rewrite means the usual expansion)
        let plain = cx.eval(h.call(h.sym("calc_waves"), &[f]))?;
        if plain != f {
            f = plain;
            waves = true;
        }
    }
    let mut cond = cond;
    if cond.is_some_and(|c| c == h.sym_term(h.sym("reals"))) {
        cond = None;
    }
    if let Some(c) = cond {
        region.push(c);
    }
    let reg = match region.as_slice() {
        [] => None,
        [r] => Some(*r),
        _ => Some(cx.eval(h.call(h.sym("and"), &region))?),
    };
    // the mode: ordered when some condition is not a relation
    let ordered = has_symbol_condition(b, f) && !opts.orderless && !opts.boolean;
    if ordered {
        let mut out = vec![];
        let t = cx.bool(true);
        let Some(d) = ordered_cases(cx, b, f, t, &mut out)? else { return Ok(None) };
        if out.is_empty() {
            return Ok(Some(f));
        }
        let pieces: Vec<T<'h>> = out.iter().map(|&(v, c)| cx.list(&[v, c])).collect();
        let pl = cx.list(&pieces);
        return Ok(Some(cx.eval(b.call("piecewise", &[pl, d]))?));
    }
    let f = step_forms(cx, b, f, reg)?;
    // the variables: symbols that are not numeric constants (pi)
    let mut vars = vec![];
    for x in variables(b, f) {
        let nx = cx.eval(b.call("n", &[x]))?;
        if b.ar.num(nx).is_none() {
            vars.push(x);
        }
    }
    let t = cx.bool(true);
    let Some(cases) = cases_of(cx, b, f, reg, &vars, t)? else { return Ok(None) };
    if opts.boolean {
        // or over the pieces of value and condition
        let mut ors = vec![];
        for (v, c) in cases {
            ors.push(cx.eval(b.call("and", &[c, v]))?);
        }
        return Ok(Some(cx.eval(b.call("or", &ors))?));
    }
    if cases.len() <= 1 && !contains_piecewise(b, f) {
        // nothing to expand: the expression itself
        return Ok(Some(f));
    }
    // simplify each condition; drop the empty ones
    // pieces with equal values merged: the value and the alternatives of its condition
    let mut groups: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    // with symbol conditions (orderless) the input's default (the last case) stays the default
    let symbolic = has_symbol_condition(b, f);
    let mut last_value = None;
    for (v, c) in cases {
        cx.tick(1)?;
        let Some(c) = simplify_cond(cx, b, c, reg, &vars, opts.reduce)? else { continue };
        let v = value_form(cx, b, v, &vars, waves, opts.values)?;
        last_value = Some(v);
        match groups.iter_mut().find(|g| g.0 == v) {
            Some(g) => {
                if !g.1.contains(&c) {
                    g.1.push(c)
                }
            }
            None => groups.push((v, vec![c])),
        }
    }
    if groups.is_empty() {
        return Ok(None);
    }
    // the pieces in the canonical order of their values; the default is 0 when that is a value, else the last
    let mut vals: Vec<T<'h>> = groups.iter().map(|g| g.0).collect();
    order_values(cx, b, &mut vals)?;
    let zero = b.zero();
    let d = match last_value {
        _ if vals.contains(&zero) => zero,
        Some(l) if symbolic => l,
        _ => vals[vals.len() - 1],
    };
    let mut pieces = vec![];
    for v in vals {
        if v == d {
            continue;
        }
        let Some(i) = groups.iter().position(|g| g.0 == v) else { continue };
        let (_, mut cs) = groups.remove(i);
        // the alternatives of a merged piece in the canonical order
        cx.sort(&mut cs);
        let c = if cs.len() == 1 { cs[0] } else { cx.eval(b.call("or", &cs))? };
        pieces.push(cx.list(&[v, c]));
    }
    let pl = cx.list(&pieces);
    Ok(Some(cx.eval(b.call("piecewise", &[pl, d]))?))
}

/// The values in the order of the pieces: the canonical order, except that when every value that is not a number
/// is a polynomial with real number coefficients in one variable, those polynomials come after the numbers ordered
/// as polynomials: by degree, then by the coefficients of the primitive part (the content taken out with the sign of
/// the leading coefficient, as `factor` writes it) from the leading one down, then by the content (`-1 + x`, `x`,
/// `1 + x`, `-1 + 2*x`; `4*(-1 + x)` before `4*x` before `-2*(-1 + 2*x)`; `1 - x` before `1 + x`; `-x` before `x`).
fn order_values<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, vals: &mut Vec<T<'h>>) -> Res<()> {
    let h = b.heap();
    cx.sort(vals);
    let is_num = |v: T<'h>| b.ar.num(v).is_some();
    // the one kernel: a variable, or one other term (`floor(x)`, `sin(x)`) the values are polynomials in
    let mut used: Vec<T<'h>> = vec![];
    for &v in vals.iter() {
        kernels(b, v, &mut used);
    }
    let [x] = used.as_slice() else { return Ok(()) };
    let x = *x;
    // the coefficients (leading first) of each value that is not a number
    let mut keyed: Vec<(Vec<f64>, T<'h>)> = vec![];
    let approx = |cx: &mut NativeCx<'_, '_, 'h>, c: T<'h>| -> Res<Option<f64>> {
        if !b.ar.num(c).is_some_and(|n| n.as_real().is_some()) {
            return Ok(None);
        }
        Ok(match h.view(cx.eval(b.call("n", &[c]))?) {
            View::Real(y) => Some(y),
            View::Int(k) => Some(k as f64),
            _ => None,
        })
    };
    for &v in vals.iter().filter(|&&v| !is_num(v)) {
        if sets::poly_degree(b, v, &[x]).is_none() {
            return Ok(());
        }
        let cl = cx.eval(b.call("coefficient_list", &[b.call("expand", &[v]), x]))?;
        let Some(cs) = cx.as_list(cl).map(|c| c.to_vec()) else { return Ok(()) };
        let Some(&lead) = cs.last() else { return Ok(()) };
        // the content: the gcd of the coefficients with the sign of the leading one
        let g = cx.eval(b.call("gcd", &cs))?;
        let Some(sl) = approx(cx, lead)? else { return Ok(()) };
        let content = if sl < 0.0 { cx.eval(b.neg(g))? } else { g };
        if b.ar.num(content).is_none() || content == b.zero() {
            return Ok(());
        }
        let mut key = vec![cs.len() as f64];
        for &c in cs.iter().rev() {
            let q = cx.eval(b.div(c, content))?;
            let Some(f) = approx(cx, q)? else { return Ok(()) };
            key.push(f);
        }
        let Some(fc) = approx(cx, content)? else { return Ok(()) };
        key.push(fc);
        keyed.push((key, v));
    }
    // a stable sort: equal keys keep the canonical order
    keyed.sort_by(|a, c| a.0.iter().zip(&c.0).map(|(p, q)| p.total_cmp(q)).find(|o| o.is_ne()).unwrap_or(a.0.len().cmp(&c.0.len())));
    let nums: Vec<T<'h>> = vals.iter().copied().filter(|&v| is_num(v)).collect();
    *vals = nums.into_iter().chain(keyed.into_iter().map(|k| k.1)).collect();
    Ok(())
}

/// The terms `t` is a polynomial in (sums, products and integer powers taken apart; numbers left out).
fn kernels<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    if b.ar.num(t).is_some() || out.len() > 1 {
        return;
    }
    if let View::App { head, args } = h.view(t) {
        let s = b.sym_of(head);
        if s == Some(b.hd.add) || s == Some(b.hd.mul) {
            args.iter().for_each(|&a| kernels(b, a, out));
            return;
        }
        if let (true, &[base, e]) = (s == Some(b.hd.pow), args) {
            if matches!(h.view(e), View::Int(k) if k > 0) {
                kernels(b, base, out);
                return;
            }
        }
    }
    if !out.contains(&t) {
        out.push(t);
    }
}

/// A piece's value in its stated form: the value simplifier applied; a wave's value factored (`4*(-1 + x)`); an
/// even power of a sum with the base's leading coefficient positive (`(-1 + a)^4`, as `factor` writes the base).
fn value_form<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, vars: &[T<'h>], waves: bool, simp: Option<T<'h>>) -> Res<T<'h>> {
    let h = b.heap();
    let mut v = cx.eval(v)?;
    if let Some(s) = simp {
        v = cx.eval(h.app(s, &[v]))?;
    }
    // a wave's polynomial value, or one not written expanded (`1 - 4*(-1/4 + x)`), factored
    if b.ar.num(v).is_none() && sets::poly_degree(b, v, vars).is_some() {
        let expanded = cx.eval(b.call("expand", &[v]))?;
        if waves || expanded != v {
            v = cx.eval(b.call("factor", &[v]))?;
        }
    }
    // (-w)^k = w^k for an even integer k
    if let View::App { head, args: &[base, ex] } = h.view(v) {
        if b.sym_of(head) == Some(b.hd.pow) && matches!(h.view(ex), View::Int(k) if k % 2 == 0) && matches!(h.view(base), View::App { head: bh, .. } if b.sym_of(bh) == Some(b.hd.add)) {
            let fv = cx.eval(b.call("factor", &[v]))?;
            if let View::App { head: ph, args: &[w, e2] } = h.view(fv) {
                // only the sign of the base changes
                if b.sym_of(ph) == Some(b.hd.pow) && e2 == ex && cx.eval(b.add(&[w, base]))? == b.zero() {
                    v = fv;
                }
            }
        }
    }
    Ok(v)
}

/// Does `t` contain a piecewise head (or a step function)?
fn contains_piecewise<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    contains_piecewise_in(b, t, &mut mc_api::walk::memo())
}

fn contains_piecewise_in<'h>(b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, t, |memo| {
        let h = b.heap();
        match h.view(t) {
            View::App { head, args } => {
                b.sym_of(head).is_some_and(|s| {
                    matches!(
                        h.sym_name(s).as_str(),
                        "piecewise"
                            | "if"
                            | "max"
                            | "min"
                            | "clip"
                            | "unit_step"
                            | "boole"
                            | "abs"
                            | "sign"
                            | "floor"
                            | "ceiling"
                            | "round"
                            | "integer_part"
                            | "prime_pi"
                            | "ramp"
                            | "real_abs"
                            | "real_sign"
                            | "unit_box"
                            | "unit_triangle"
                    )
                }) || args.iter().any(|&a| contains_piecewise_in(b, a, memo))
            }
            _ => false,
        }
    })
}

/// `t` with the step functions that are written by others (module comment) rewritten into them: `floor(u, a)`,
/// `ceiling(u, a)`, `round(u, a)`, `fractional_part(u)`, `quotient(m, n)` and `mod(m, n)` for a nonzero number
/// `n`, and the products behind `unit_box`, `unit_triangle` and `unit_step` of several arguments.
fn step_forms<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, reg: Option<T<'h>>) -> Res<T<'h>> {
    let bounded = reg.is_some();
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    cx.tick(1)?;
    let mut out = Vec::with_capacity(args.len());
    for &a in args {
        out.push(step_forms(cx, b, a, reg)?);
    }
    let name = b.sym_of(head).map(|s| h.sym_name(s).to_string()).unwrap_or_default();
    // the forms in floor and integer_part help only where the region cuts them into steps
    let nonzero_number = |n: T<'h>| bounded && n != b.zero() && b.ar.num(n).is_some_and(|v| v.as_real().is_some());
    // a divisor that is a variable: the steps of floor(m/n) where the region bounds m/n
    let symbolic = match (name.as_str(), out.as_slice(), reg) {
        ("quotient" | "mod", &[m, n], Some(r)) if b.ar.num(n).is_none() && matches!(h.view(n), View::Sym(_)) => {
            let u = cx.eval(b.div(m, n))?;
            let vs = variables(b, u);
            vs.len() > 1 && ratio_bounds(cx, b, u, r, &vs)?.is_some()
        }
        _ => false,
    };
    let symbolic = |_: T<'h>| symbolic;
    let r = match (name.as_str(), out.as_slice()) {
        ("floor" | "ceiling" | "round", &[u, a]) if nonzero_number(a) => b.mul(&[a, b.call(&name, &[b.div(u, a)])]),
        ("fractional_part", &[u]) if bounded => b.sub(u, b.call("integer_part", &[u])),
        ("quotient", &[m, n]) if nonzero_number(n) || symbolic(n) => b.call("floor", &[b.div(m, n)]),
        ("mod", &[m, n]) if nonzero_number(n) || symbolic(n) => b.sub(m, b.mul(&[n, b.call("floor", &[b.div(m, n)])])),
        ("unit_box" | "unit_triangle" | "unit_step", us) if us.len() > 1 => {
            let fs: Vec<T<'h>> = us.iter().map(|&u| b.call(&name, &[u])).collect();
            b.mul(&fs)
        }
        _ if out.as_slice() == args => return Ok(t),
        _ => h.app(head, &out),
    };
    cx.eval(r)
}

/// Is some condition of a piecewise or `if` in `t` a symbol (not a relation)?
fn has_symbol_condition<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    has_symbol_condition_in(b, t, &mut mc_api::walk::memo())
}

fn has_symbol_condition_in<'h>(b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, t, |memo| {
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return false };
        let name = b.sym_of(head).map(|s| h.sym_name(s).to_string()).unwrap_or_default();
        let sym = |c: T<'h>| matches!(h.view(c), View::Sym(s) if !matches!(h.sym_name(s).as_str(), "true" | "false"));
        if name == "if" && args.first().is_some_and(|&c| sym(c)) {
            return true;
        }
        if name == "piecewise" {
            if let Some(&l) = args.first() {
                if let View::App { args: rows, .. } = h.view(l) {
                    for &r in rows {
                        if let View::App { args: &[_, c], .. } = h.view(r) {
                            if sym(c) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        args.iter().any(|&a| has_symbol_condition_in(b, a, memo))
    })
}

/// The rows and default of a `piecewise` or `if`.
fn rows_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Option<(Vec<Case<'h>>, T<'h>)> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return None };
    match b.sym_of(head).map(|s| h.sym_name(s).to_string()).as_deref() {
        Some("if") => match args {
            [c, a, d] => Some((vec![(*a, *c)], *d)),
            [c, a] => Some((vec![(*a, *c)], b.zero())),
            _ => None,
        },
        Some("piecewise") => {
            let rows = cx.as_list(args[0])?;
            let mut out = vec![];
            for &r in rows {
                let [v, c] = cx.as_list(r)? else { return None };
                out.push((*v, *c));
            }
            let d = args.get(1).copied().unwrap_or_else(|| b.zero());
            Some((out, d))
        }
        _ => None,
    }
}

/// Ordered mode: the pieces along the paths; returns the default of the top level.
fn ordered_cases<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, path: T<'h>, out: &mut Vec<Case<'h>>) -> Res<Option<T<'h>>> {
    let Some((rows, d)) = rows_of(cx, b, t) else { return Ok(Some(t)) };
    let conj = |cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, c: T<'h>| -> Res<T<'h>> {
        if cx.is_true(a) {
            return Ok(c);
        }
        // flat: (c and b) and a is c and b and a
        let h = b.heap();
        let mut v: Vec<T<'h>> = match h.view(a) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
            _ => vec![a],
        };
        v.push(c);
        cx.eval(b.call("and", &v))
    };
    for (v, c) in rows {
        let p = conj(cx, path, c)?;
        if rows_of(cx, b, v).is_some() {
            let Some(inner_d) = ordered_cases(cx, b, v, p, out)? else { return Ok(None) };
            out.push((inner_d, p));
        } else {
            out.push((v, p));
        }
    }
    if rows_of(cx, b, d).is_some() {
        return ordered_cases(cx, b, d, path, out);
    }
    Ok(Some(d))
}

/// The symbols of `t` (the candidate variables), sorted by name.
fn variables<'h>(b: &Build<'h>, t: T<'h>) -> Vec<T<'h>> {
    super::spec::params(b, &[t], &[])
}

/// The negation of a condition, with relations flipped.
fn negate<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if let View::App { head, args: &[l, r] } = h.view(c) {
        if let Some(s) = b.sym_of(head) {
            let flip = match h.sym_name(s).as_str() {
                "less" => Some("greater_equal"),
                "less_equal" => Some("greater"),
                "greater" => Some("less_equal"),
                "greater_equal" => Some("less"),
                "equal" => Some("unequal"),
                "unequal" => Some("equal"),
                _ => None,
            };
            if let Some(n) = flip {
                return cx.eval(b.call(n, &[l, r]));
            }
        }
    }
    // De Morgan over a connective of relations, so that the negation stays a formula of relations
    if let View::App { head, args } = h.view(c) {
        let dual = match b.sym_of(head).map(|s| h.sym_name(s).to_string()).as_deref() {
            Some("and") => Some("or"),
            Some("or") => Some("and"),
            _ => None,
        };
        if let Some(d) = dual {
            if args.iter().all(|&a| is_relation(b, a)) {
                let mut ns = vec![];
                for &a in args {
                    ns.push(negate(cx, b, a)?);
                }
                return cx.eval(b.call(d, &ns));
            }
        }
    }
    cx.eval(b.call("not", &[c]))
}

/// Exclusive mode: the cases of `t` under `path`; `None` when a step function cannot be cut.
fn cases_of<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, reg: Option<T<'h>>, vars: &[T<'h>], path: T<'h>) -> Res<Option<Vec<Case<'h>>>> {
    let h = b.heap();
    cx.tick(1)?;
    let and2 = |cx: &mut NativeCx<'_, '_, 'h>, a: T<'h>, c: T<'h>| -> Res<T<'h>> {
        if cx.is_true(a) {
            return Ok(c);
        }
        if cx.is_true(c) {
            return Ok(a);
        }
        // flat, the path first
        let mut v: Vec<T<'h>> = vec![];
        for t in [a, c] {
            match h.view(t) {
                View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => v.extend(args.iter().copied()),
                _ => v.push(t),
            }
        }
        cx.eval(b.call("and", &v))
    };
    let View::App { head, args } = h.view(t) else { return Ok(Some(vec![(t, path)])) };
    let name = b.sym_of(head).map(|s| h.sym_name(s).to_string()).unwrap_or_default();
    // the cases of this node with their own conditions
    let own: Option<Vec<Case<'h>>> = match name.as_str() {
        "if" | "piecewise" => {
            let Some((rows, d)) = rows_of(cx, b, t) else { return Ok(None) };
            let mut out = vec![];
            let mut before: Vec<T<'h>> = vec![];
            for (v, c) in rows {
                let mut cs = vec![c];
                cs.extend(before.iter().copied());
                let cc = if cs.len() == 1 { c } else { cx.eval(b.call("and", &cs))? };
                out.push((v, cc));
                before.push(negate(cx, b, c)?);
            }
            let dc = match before.as_slice() {
                [] => cx.bool(true),
                [c] => *c,
                _ => cx.eval(b.call("and", &before))?,
            };
            out.push((d, dc));
            Some(out)
        }
        "max" | "min" if args.len() >= 2 => {
            let (ge, lt) = if name == "max" { ("greater_equal", "less") } else { ("less_equal", "greater") };
            let mut out = vec![];
            let n = args.len();
            for i in 0..n - 1 {
                let mut cs = vec![];
                for j in 0..n {
                    if j < i {
                        cs.push(cx.eval(b.call(lt, &[b.sub(args[j], args[i]), b.zero()]))?);
                    } else if j > i {
                        cs.push(cx.eval(b.call(ge, &[b.sub(args[i], args[j]), b.zero()]))?);
                    }
                }
                let c = if cs.len() == 1 { cs[0] } else { cx.eval(b.call("and", &cs))? };
                out.push((args[i], c));
            }
            out.push((args[n - 1], cx.bool(true)));
            // the last case holds where no earlier did
            let mut neg = vec![];
            let earlier: Vec<T<'h>> = out[..out.len() - 1].iter().map(|c| c.1).collect();
            for c in earlier {
                neg.push(negate(cx, b, c)?);
            }
            if let Some(l) = out.last_mut() {
                l.1 = match neg.as_slice() {
                    [] => cx.bool(true),
                    [c] => *c,
                    _ => cx.eval(b.call("and", &neg))?,
                };
            }
            Some(out)
        }
        "clip" => {
            let u = args[0];
            let (lo, hi) = match args.get(1).and_then(|&r| cx.as_list(r)) {
                Some([l, r]) => (*l, *r),
                _ => (b.int(-1), b.one()),
            };
            let c1 = cx.eval(b.call("greater", &[b.sub(lo, u), b.zero()]))?;
            let c2a = cx.eval(b.call("less", &[b.sub(hi, u), b.zero()]))?;
            let n1 = negate(cx, b, c1)?;
            let c2 = cx.eval(b.call("and", &[c2a, n1]))?;
            let n2 = negate(cx, b, c2a)?;
            let c3 = cx.eval(b.call("and", &[n1, n2]))?;
            // clip(u, [lo, hi], [vlo, vhi]): vlo below lo and vhi above hi
            let (vlo, vhi) = match args.get(2).and_then(|&r| cx.as_list(r)) {
                Some([l, r]) => (*l, *r),
                _ => (lo, hi),
            };
            Some(vec![(vlo, c1), (vhi, c2), (u, c3)])
        }
        "unit_step" if args.len() == 1 => {
            let u = args[0];
            Some(vec![(b.zero(), cx.eval(b.call("less", &[u, b.zero()]))?), (b.one(), cx.eval(b.call("greater_equal", &[u, b.zero()]))?)])
        }
        "boole" if args.len() == 1 => {
            let c = args[0];
            let n = negate(cx, b, c)?;
            Some(vec![(b.zero(), n), (b.one(), c)])
        }
        "abs" | "real_abs" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            Some(vec![(b.neg(u), cx.eval(b.call("less", &[u, b.zero()]))?), (u, cx.eval(b.call("greater_equal", &[u, b.zero()]))?)])
        }
        "ramp" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            Some(vec![(b.zero(), cx.eval(b.call("less", &[u, b.zero()]))?), (u, cx.eval(b.call("greater_equal", &[u, b.zero()]))?)])
        }
        "sign" | "real_sign" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            Some(vec![
                (b.int(-1), cx.eval(b.call("less", &[u, b.zero()]))?),
                (b.zero(), cx.eval(b.call("equal", &[u, b.zero()]))?),
                (b.one(), cx.eval(b.call("greater", &[u, b.zero()]))?),
            ])
        }
        "unit_box" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            let inside = cx.eval(b.call("and", &[b.call("less_equal", &[b.rat(-1, 2), u]), b.call("less_equal", &[u, b.rat(1, 2)])]))?;
            let outside = cx.eval(b.call("or", &[b.call("less", &[u, b.rat(-1, 2)]), b.call("greater", &[u, b.rat(1, 2)])]))?;
            Some(vec![(b.one(), inside), (b.zero(), outside)])
        }
        "unit_triangle" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            let left = cx.eval(b.call("and", &[b.call("less_equal", &[b.int(-1), u]), b.call("less", &[u, b.zero()])]))?;
            let right = cx.eval(b.call("and", &[b.call("less_equal", &[b.zero(), u]), b.call("less_equal", &[u, b.one()])]))?;
            let outside = cx.eval(b.call("or", &[b.call("less", &[u, b.int(-1)]), b.call("greater", &[u, b.one()])]))?;
            Some(vec![(b.add(&[b.one(), u]), left), (b.sub(b.one(), u), right), (b.zero(), outside)])
        }
        "floor" | "ceiling" | "round" | "integer_part" | "prime_pi" if args.len() == 1 => match steps(cx, b, t, reg, vars)? {
            Some(v) => Some(v),
            None => return Ok(None),
        },
        _ => None,
    };
    if let Some(own) = own {
        // the values may be piecewise themselves
        let mut out = vec![];
        for (v, c) in own {
            let p = and2(cx, path, c)?;
            if cx.is_false(p) {
                continue;
            }
            // piecewise functions inside the condition split it (left whole when they cannot be)
            let ps = if contains_piecewise(b, p) { cond_cases(cx, b, p, reg, vars)?.unwrap_or_else(|| vec![p]) } else { vec![p] };
            for p in ps {
                // a step function left whole (no bounded region) is a value
                if v == t {
                    out.push((v, p));
                    continue;
                }
                match cases_of(cx, b, v, reg, vars, p)? {
                    Some(vs) => out.extend(vs),
                    None => return Ok(None),
                }
            }
        }
        return Ok(Some(out));
    }
    // a function of piecewise arguments: the product of their cases
    let mut combos: Vec<(Vec<T<'h>>, T<'h>)> = vec![(vec![], path)];
    for &a in args {
        if !contains_piecewise(b, a) {
            for c in combos.iter_mut() {
                c.0.push(a);
            }
            continue;
        }
        let mut next = vec![];
        for (vals, p) in combos {
            let Some(cs) = cases_of(cx, b, a, reg, vars, p)? else { return Ok(None) };
            for (v, c) in cs {
                if cx.is_false(c) || !satisfiable(cx, b, c, reg, vars)? {
                    continue;
                }
                let mut nv = vals.clone();
                nv.push(v);
                next.push((nv, c));
            }
        }
        combos = next;
        if combos.len() > 256 {
            return Ok(None);
        }
    }
    let mut out = vec![];
    for (vals, c) in combos {
        let v = h.app(head, &vals);
        // the function of the cases' values may split now (`abs(u - floor(u))` with the floor a number)
        if vals.as_slice() != args && contains_piecewise(b, v) {
            let ve = cx.eval(v)?;
            if ve != t && contains_piecewise(b, ve) {
                if let Some(cs) = cases_of(cx, b, ve, reg, vars, c)? {
                    out.extend(cs);
                    continue;
                }
            }
        }
        out.push((v, c));
    }
    Ok(Some(out))
}

/// The alternatives of a condition with piecewise functions inside, each free of them where it can be: a relation
/// of piecewise arguments is split by their cases (`max(a, b) <= c` is `a - b >= 0 and a <= c` or `a - b < 0 and
/// b <= c`), `and` takes the product of its parts' alternatives, `or` their union. The alternatives together are
/// equivalent to the condition (those refuted are dropped); `None` when it is too large.
fn cond_cases<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, reg: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let h = b.heap();
    cx.tick(1)?;
    if !contains_piecewise(b, c) {
        return Ok(Some(vec![c]));
    }
    let View::App { head, args } = h.view(c) else { return Ok(Some(vec![c])) };
    let name = b.sym_of(head).map(|s| h.sym_name(s).to_string()).unwrap_or_default();
    let mut out = vec![];
    match name.as_str() {
        "and" => {
            out.push(cx.bool(true));
            for &a in args {
                let Some(sub) = cond_cases(cx, b, a, reg, vars)? else { return Ok(None) };
                let mut next = vec![];
                for &p in &out {
                    for &s in &sub {
                        let t = cx.eval(b.call("and", &[p, s]))?;
                        if !cx.is_false(t) && satisfiable(cx, b, t, reg, vars)? {
                            next.push(t);
                        }
                    }
                }
                out = next;
                if out.len() > 64 {
                    return Ok(None);
                }
            }
        }
        "or" => {
            for &a in args {
                let Some(sub) = cond_cases(cx, b, a, reg, vars)? else { return Ok(None) };
                out.extend(sub);
            }
            if out.len() > 64 {
                return Ok(None);
            }
        }
        "not" if args.len() == 1 && is_relation(b, args[0]) => {
            let n = negate(cx, b, args[0])?;
            if is_head(b, n, "not") {
                return Ok(None);
            }
            return cond_cases(cx, b, n, reg, vars);
        }
        _ if is_relation(b, c) => {
            // the relation of each case of its arguments, under that case's condition
            let t = cx.bool(true);
            let Some(cs) = cases_of(cx, b, c, reg, vars, t)? else { return Ok(None) };
            if cs.len() > 64 {
                return Ok(None);
            }
            for (rel, cc) in cs {
                let r = cx.eval(rel)?;
                let a = cx.eval(b.call("and", &[cc, r]))?;
                if !cx.is_false(a) && satisfiable(cx, b, a, reg, vars)? {
                    out.push(a);
                }
            }
        }
        _ => return Ok(Some(vec![c])),
    }
    Ok(Some(out))
}

/// Is `t` a relation (`==`, `!=`, `<`, `<=`, `>`, `>=`)?
fn is_relation<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    let View::App { head, args: &[_, _] } = h.view(t) else { return false };
    b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal"))
}

/// Not refuted: one variable by `reduce`, otherwise by the evaluation of the condition.
fn satisfiable<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, reg: Option<T<'h>>, vars: &[T<'h>]) -> Res<bool> {
    let h = b.heap();
    let full = match reg {
        Some(r) => cx.eval(b.call("and", &[c, r]))?,
        None => c,
    };
    if cx.is_false(full) {
        return Ok(false);
    }
    let vs: Vec<T<'h>> = vars.iter().copied().filter(|&x| !is_free(b, full, x)).collect();
    if let [x] = vs.as_slice() {
        let r = cx.eval(b.call("reduce", &[full, *x, h.sym_term(h.sym("reals"))]))?;
        if cx.is_false(r) {
            return Ok(false);
        }
    } else if vs.len() > 1 && vs.len() <= 3 && sets::polynomial_formula(b, full, &vs) {
        let l = cx.list(&vs);
        let r = cx.eval(b.call("resolve", &[b.call("exists", &[l, full]), h.sym_term(h.sym("reals"))]))?;
        if cx.is_false(r) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The condition simplified: one variable reduced, bounds implied by the region left out; `None` when empty.
fn simplify_cond<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, reg: Option<T<'h>>, vars: &[T<'h>], full_reduce: bool) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if !satisfiable(cx, b, c, reg, vars)? {
        return Ok(None);
    }
    // conjuncts (without repeats) by variable: those in one variable reduced together
    let mut parts: Vec<T<'h>> = vec![];
    let all_parts: Vec<T<'h>> = match h.view(c) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![c],
    };
    for p in all_parts {
        if !parts.contains(&p) {
            parts.push(p);
        }
    }
    let one_var = |t: T<'h>| -> Option<T<'h>> {
        let vs: Vec<T<'h>> = vars.iter().copied().filter(|&x| !is_free(b, t, x)).collect();
        match vs.as_slice() {
            [x] => Some(*x),
            _ => None,
        }
    };
    let mut groups: Vec<(T<'h>, Vec<T<'h>>)> = vec![];
    let mut rest = vec![];
    for p in parts {
        match one_var(p) {
            Some(x) => match groups.iter_mut().find(|g| g.0 == x) {
                Some(g) => g.1.push(p),
                None => groups.push((x, vec![p])),
            },
            None => rest.push(p),
        }
    }
    let mut out = vec![];
    for (x, ps) in groups {
        let all = if ps.len() == 1 { ps[0] } else { cx.eval(b.call("and", &ps))? };
        // linear conditions reduced to intervals; others with the constants moved right
        let linear = ps.iter().all(|&p| linear_formula(b, p, x));
        let r = if linear || full_reduce {
            let r = cx.eval(b.call("reduce", &[all, x, h.sym_term(h.sym("reals"))]))?;
            if is_head(b, r, "reduce") || is_head(b, r, names::FAILURE) {
                all
            } else {
                r
            }
        } else {
            let mut v = vec![];
            let mut dropped = vec![false; ps.len()];
            for (i, &p) in ps.iter().enumerate() {
                // a polynomial conjunct the others kept imply (or true for every real x) is left out
                if ps.len() <= 4 && sets::polynomial_formula(b, p, &[x]) {
                    let mut rest: Vec<T<'h>> = ps.iter().enumerate().filter(|&(j, _)| j != i && !dropped[j]).map(|(_, &q)| q).collect();
                    rest.push(negate(cx, b, p)?);
                    let r = cx.eval(b.call("reduce", &[b.call("and", &rest), x, h.sym_term(h.sym("reals"))]))?;
                    if cx.is_false(r) {
                        dropped[i] = true;
                        continue;
                    }
                }
                let moved = move_right(cx, b, p)?;
                let tidy = sets::tidy_rel(cx, b, moved)?;
                v.push(positive_lead(cx, b, tidy, x)?);
            }
            if v.len() == 1 {
                v[0]
            } else {
                cx.eval(b.call("and", &v))?
            }
        };
        // bounds the region implies
        let r = match reg {
            Some(g) => drop_implied(cx, b, r, g)?,
            None => r,
        };
        if !cx.is_true(r) {
            out.push(sets::tidy_cond(cx, b, r)?);
        }
    }
    // keep the original order: the reduced groups where their first conjunct was
    out.extend(rest);
    Ok(Some(match out.as_slice() {
        [] => cx.bool(true),
        [o] => *o,
        _ => cx.eval(b.call("and", &out))?,
    }))
}

/// Is `p` a relation of linear polynomials in `x`, or `and`, `or`, `not` of such?
fn linear_formula<'h>(b: &Build<'h>, p: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let View::App { head, args } = h.view(p) else { return false };
    if is_relation(b, p) {
        return args.iter().all(|&a| linear_in(b, a, x));
    }
    b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "and" | "or" | "not")) && args.iter().all(|&a| linear_formula(b, a, x))
}

/// Is `a` of degree at most 1 in `x`, with coefficients free of it (constants such as `pi` allowed)?
fn linear_in<'h>(b: &Build<'h>, a: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    if a == x || is_free(b, a, x) {
        return true;
    }
    let View::App { head, args } = h.view(a) else { return false };
    match b.sym_of(head) {
        Some(s) if s == b.hd.add => args.iter().all(|&t| linear_in(b, t, x)),
        Some(s) if s == b.hd.mul => {
            let with: Vec<T<'h>> = args.iter().copied().filter(|&t| !is_free(b, t, x)).collect();
            matches!(with.as_slice(), [t] if linear_in(b, *t, x))
        }
        _ => false,
    }
}

/// `p op c` for a polynomial `p` in `x` with a negative leading coefficient and a number `c` as `-p op' -c` (the
/// relation flipped): the leading coefficient positive (`5*x - x^2 <= 6` is `-5*x + x^2 >= -6`).
fn positive_lead<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>, x: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args: &[l, c] } = h.view(r) else { return Ok(r) };
    let Some(s) = b.sym_of(head) else { return Ok(r) };
    let flip = match h.sym_name(s).as_str() {
        "less" => "greater",
        "less_equal" => "greater_equal",
        "greater" => "less",
        "greater_equal" => "less_equal",
        "equal" => "equal",
        "unequal" => "unequal",
        _ => return Ok(r),
    };
    if b.ar.num(c).is_none() || sets::poly_degree(b, l, &[x]).is_none_or(|d| d < 1) {
        return Ok(r);
    }
    let cl = cx.eval(b.call("coefficient_list", &[l, x]))?;
    let Some(&lead) = cx.as_list(cl).and_then(|cs| cs.last()) else { return Ok(r) };
    let neg = cx.eval(b.call("less", &[lead, b.zero()]))?;
    if !cx.is_true(neg) {
        return Ok(r);
    }
    cx.eval(b.call(flip, &[b.neg(l), b.neg(c)]))
}

/// `l op r` as `l - r op 0` (for the tidying of constants).
fn move_right<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    if let View::App { head, args: &[l, r] } = h.view(p) {
        let rel = b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "equal" | "less" | "less_equal" | "greater" | "greater_equal"));
        if rel && r != b.zero() {
            let d = cx.eval(b.sub(l, r))?;
            return cx.eval(h.app(head, &[d, b.zero()]));
        }
    }
    Ok(p)
}

/// The conjuncts of `r` that the region does not imply.
fn drop_implied<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>, reg: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let parts: Vec<T<'h>> = match h.view(r) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![r],
    };
    let mut kept = vec![];
    for p in parts {
        let i = cx.eval_assuming(&[reg], b.call("refine", &[p]))?;
        if !cx.is_true(i) {
            kept.push(p);
        }
    }
    Ok(match kept.as_slice() {
        [] => cx.bool(true),
        [k] => *k,
        _ => cx.eval(b.call("and", &kept))?,
    })
}

/// The cases of a step function of one variable inside a bounded region (module comment).
fn steps<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, reg: Option<T<'h>>, vars: &[T<'h>]) -> Res<Option<Vec<Case<'h>>>> {
    let h = b.heap();
    let View::App { head, args: &[u] } = h.view(t) else { return Ok(None) };
    let name = b.sym_of(head).map(|s| h.sym_name(s).to_string()).unwrap_or_default();
    let atom = Ok(Some(vec![(t, cx.bool(true))]));
    let Some(reg) = reg else { return atom };
    let vs: Vec<T<'h>> = vars.iter().copied().filter(|&x| !is_free(b, u, x)).collect();
    if vs.len() > 1 && name == "floor" {
        return steps_of_bounded(cx, b, u, reg, &vs);
    }
    let [x] = vs.as_slice() else { return Ok(None) };
    let x = *x;
    // the interval of the region's conditions on x alone
    let parts: Vec<T<'h>> = match h.view(reg) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![reg],
    };
    let mine: Vec<T<'h>> = parts.into_iter().filter(|&p| !is_free(b, p, x) && vars.iter().all(|&v| v == x || is_free(b, p, v))).collect();
    if mine.is_empty() {
        return atom;
    }
    let mr = if mine.len() == 1 { mine[0] } else { cx.eval(b.call("and", &mine))? };
    let r = cx.eval(b.call("reduce", &[mr, x, h.sym_term(h.sym("reals"))]))?;
    let (lo, hi) = match bounds(cx, b, r, x)? {
        Some(v) => v,
        None => return atom,
    };
    let (lo, hi) = (lo.0, hi.0);
    // the jump points: u crosses an integer (a half-integer for round), or x a prime for prime_pi
    let mut jumps: Vec<T<'h>> = vec![];
    if name == "prime_pi" {
        if u != x {
            return Ok(None);
        }
        let (a, c) = (lo.ceil() as i64, hi.floor() as i64);
        if c - a > 2000 {
            return Ok(None);
        }
        for k in a.max(2)..=c {
            if (2..k).take_while(|d| d * d <= k).all(|d| k % d != 0) && (k as f64) > lo && (k as f64) < hi {
                jumps.push(b.int(k));
            }
        }
    } else {
        // u monotonic in x over the interval: its integer crossings
        let (ul, uh) = (value(cx, b, u, x, lo)?, value(cx, b, u, x, hi)?);
        let (Some(ul), Some(uh)) = (ul, uh) else { return Ok(None) };
        let (a, c) = (ul.min(uh), ul.max(uh));
        if c - a > 200.0 || sets::poly_degree(b, u, &[x]) != Some(1) {
            return Ok(None);
        }
        let shift = if name == "round" { 0.5 } else { 0.0 };
        let mut k = (a - shift).floor() as i64;
        while (k as f64) + shift <= c {
            let target = b.add(&[b.int(k), b.rat(if name == "round" { 1 } else { 0 }, 2)]);
            let sol = cx.eval(b.call("solve", &[b.call("equal", &[u, target]), x]))?;
            if let Some(&[row]) = cx.as_list(sol) {
                if let Some(&[rule]) = cx.as_list(row) {
                    if let View::App { args: &[_, v], .. } = h.view(rule) {
                        let vn = value(cx, b, v, x, 0.0)?;
                        if vn.is_some_and(|p| p >= lo && p <= hi) {
                            jumps.push(v);
                        }
                    }
                }
            }
            k += 1;
        }
    }
    // the pieces between the jumps: the value at the midpoint, the jump point's side by evaluation
    let mut pts: Vec<(f64, T<'h>)> = vec![];
    for j in jumps {
        let v = value(cx, b, j, x, 0.0)?.unwrap_or(f64::NAN);
        pts.push((v, j));
    }
    pts.sort_by(|a, c| a.0.total_cmp(&c.0));
    let n = pts.len();
    let at = |cx: &mut NativeCx<'_, '_, 'h>, p: T<'h>| -> Res<T<'h>> { cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, p])])) };
    // the pieces as (value, condition at the left end, condition at the right end); neighbours with one value merge
    let mut pieces: Vec<(T<'h>, Option<T<'h>>, Option<T<'h>>)> = vec![];
    for i in 0..=n {
        let left = if i == 0 { None } else { Some(pts[i - 1].1) };
        let right = if i == n { None } else { Some(pts[i].1) };
        let mid = match (left, right) {
            (Some(l), Some(r)) => cx.eval(b.div(b.add(&[l, r]), b.int(2)))?,
            (Some(l), None) => cx.eval(b.call("rationalize", &[h.real(0.5 * (pts[i - 1].0 + hi)), b.zero()]))?.max_or(l, b),
            (None, Some(r)) => cx.eval(b.call("rationalize", &[h.real(0.5 * (lo + pts[i].0)), b.zero()]))?.max_or(r, b),
            (None, None) => return Ok(None),
        };
        let val = at(cx, mid)?;
        // an end belongs here when the function there equals this piece's value
        let lc = match left {
            Some(l) => Some(b.call(if at(cx, l)? == val { "greater_equal" } else { "greater" }, &[x, l])),
            None => None,
        };
        let rc = match right {
            Some(r) => Some(b.call(if at(cx, r)? == val { "less_equal" } else { "less" }, &[x, r])),
            None => None,
        };
        match pieces.last_mut() {
            Some(last) if last.0 == val => last.2 = rc,
            _ => pieces.push((val, lc, rc)),
        }
    }
    let mut out = vec![];
    for (val, lc, rc) in pieces {
        let cs: Vec<T<'h>> = [lc, rc].into_iter().flatten().collect();
        let c = match cs.as_slice() {
            [c] => cx.eval(*c)?,
            _ => cx.eval(b.call("and", &cs))?,
        };
        out.push((val, c));
    }
    Ok(Some(out))
}

/// `floor(u)` of several variables (`quotient(k, m)` is `floor(k/m)`) where the region bounds u: with lo and hi
/// the least and greatest values of u there (`min_value`, `max_value`), the pieces `j <= u < j + 1` for the
/// integers j from floor(lo) to floor(hi), written as conditions on u itself, the bounds the region implies left
/// out (the first piece `u < floor(lo) + 1`, the last `u >= floor(hi)`); a last piece at hi only when the region
/// reaches it. `None` when the bounds are not numbers or span more than 200 integers.
fn steps_of_bounded<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, reg: T<'h>, vs: &[T<'h>]) -> Res<Option<Vec<Case<'h>>>> {
    let Some((a, c)) = ratio_bounds(cx, b, u, reg, vs)? else { return Ok(None) };
    if a == c {
        return Ok(Some(vec![(b.int(a), cx.bool(true))]));
    }
    let mut out = vec![];
    for j in a..=c {
        // (as the pieces of one variable: `1 <= u and u < 2`, `u >= 3`)
        let mut cs = vec![];
        if j > a {
            cs.push(if j < c { b.call("less_equal", &[b.int(j), u]) } else { b.call("greater_equal", &[u, b.int(j)]) });
        }
        if j < c {
            cs.push(b.call("less", &[u, b.int(j + 1)]));
        }
        let cond = match cs.as_slice() {
            [one] => *one,
            _ => b.call("and", &cs),
        };
        // the last piece only when the region reaches floor(hi) (not refuted)
        if j == c && !satisfiable(cx, b, b.call("greater_equal", &[u, b.int(j)]), Some(reg), vs)? {
            continue;
        }
        out.push((b.int(j), cond));
    }
    Ok(Some(out))
}

/// floor of the least and of the greatest value of u in the region (`min_value`, `max_value`); `None` when they are
/// not numbers or more than 200 apart.
fn ratio_bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, reg: T<'h>, vs: &[T<'h>]) -> Res<Option<(i64, i64)>> {
    let h = b.heap();
    let l = cx.list(vs);
    let mut ends = [0f64; 2];
    for (i, f) in ["min_value", "max_value"].into_iter().enumerate() {
        let v = cx.eval(b.call(f, &[cx.list(&[u, reg]), l]))?;
        let v = cx.eval(b.call("n", &[v]))?;
        ends[i] = match h.view(v) {
            View::Real(y) if y.is_finite() => y,
            View::Int(k) => k as f64,
            _ => return Ok(None),
        };
    }
    let (a, c) = (ends[0].floor() as i64, ends[1].floor() as i64);
    Ok((c - a <= 200).then_some((a, c)))
}

trait MaxOr<'h> {
    fn max_or(self, other: T<'h>, b: &Build<'h>) -> T<'h>;
}

impl<'h> MaxOr<'h> for T<'h> {
    /// `self` (a rational midpoint) unless it failed to be a number.
    fn max_or(self, other: T<'h>, b: &Build<'h>) -> T<'h> {
        if b.ar.num(self).is_some() {
            self
        } else {
            other
        }
    }
}

/// The real value of `t` at `x = v`.
fn value<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>, v: f64) -> Res<Option<f64>> {
    let h = b.heap();
    let r = cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
    let r = cx.eval(b.call("n", &[r]))?;
    Ok(match h.view(r) {
        View::Real(y) if y.is_finite() => Some(y),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The bounds of an interval `a < x and x < b` (or with `<=`) as numbers with their terms.
#[allow(clippy::type_complexity)]
fn bounds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, r: T<'h>, x: T<'h>) -> Res<Option<((f64, T<'h>), (f64, T<'h>))>> {
    let h = b.heap();
    let parts: Vec<T<'h>> = match h.view(r) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
        _ => vec![r],
    };
    let (mut lo, mut hi) = (None, None);
    for p in parts {
        let View::App { head, args: &[l, rr] } = h.view(p) else { return Ok(None) };
        let Some(s) = b.sym_of(head) else { return Ok(None) };
        let n = h.sym_name(s).to_string();
        let (bound, is_lo) = match (n.as_str(), l == x, rr == x) {
            ("less" | "less_equal", true, _) => (rr, false),
            ("less" | "less_equal", _, true) => (l, true),
            ("greater" | "greater_equal", true, _) => (rr, true),
            ("greater" | "greater_equal", _, true) => (l, false),
            _ => return Ok(None),
        };
        let v = cx.eval(b.call("n", &[bound]))?;
        let f = match h.view(v) {
            View::Real(y) => y,
            View::Int(k) => k as f64,
            _ => return Ok(None),
        };
        if is_lo {
            lo = Some((f, bound));
        } else {
            hi = Some((f, bound));
        }
    }
    Ok(match (lo, hi) {
        (Some(a), Some(c)) => Some((a, c)),
        _ => None,
    })
}
