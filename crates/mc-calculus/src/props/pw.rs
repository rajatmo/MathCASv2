//! `piecewise_expand(f)`, `piecewise_expand(f, cond)` (decision R25P.10): nested piecewise functions, `if`,
//! `max`, `min`, `clip`, `unit_step`, `boole`, `abs` and `sign` as one `piecewise([[v1, c1], ...], d)`;
//! `floor`, `ceiling`, `round`, `integer_part` and `prime_pi` of one variable inside a bounded condition.
//!
//! - **Exclusive pieces** when every condition is a relation (or a connective of relations): each case's
//!   condition is conjoined with the negations of the cases before it (`max(a, b, c)`: `a` where `a - b >= 0 and
//!   a - c >= 0`, `b` where `a - b < 0 and b - c >= 0`, else `c`); sums, products and functions of piecewise
//!   terms take the product of their cases, dropping the empty ones. Conditions in one variable are reduced
//!   (`x > 0 and x - 1 >= 0` is `x >= 1`), and bounds that the call's condition implies are left out. A piece
//!   whose value is 0 becomes the default (the value of `piecewise` where no condition holds), otherwise the last
//!   case does; the other pieces are sorted by value.
//! - **Ordered pieces** when a condition is a symbol (`piecewise([[piecewise([[4, a]], 2), b]], 3)`): the
//!   first-match order is kept and each piece's condition is the conjunction along its path (`[[4, b and a],
//!   [2, b]], 3`).
//! - Step functions inside a bounded condition in one variable: their jumps (where the argument crosses an
//!   integer, or the primes for `prime_pi`) cut the interval; on each piece the step function is the constant
//!   it takes at the piece's midpoint (exact arithmetic), and the side a jump point belongs to is found by
//!   evaluating at it.
use super::sets::{self, is_head};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// A case: value and condition.
type Case<'h> = (T<'h>, T<'h>);

/// The `method:` suboptions read.
#[derive(Default)]
struct Opts {
    /// `"OrderlessConditions" -> true`: exclusive conditions even with symbols
    orderless: bool,
    /// `"BooleanResult" -> true`: a disjunction instead of a piecewise
    boolean: bool,
    /// `"ConditionSimplifier" -> reduce`: every condition reduced
    reduce: bool,
}

/// Read `method: ["Name" -> value, ...]`.
fn read_method<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, v: T<'h>, o: &mut Opts) {
    let h = b.heap();
    let Some(rules) = cx.as_list(v) else { return };
    for &r in rules {
        let View::App { args: &[k, val], .. } = h.view(r) else { continue };
        let View::Str(name) = h.view(k) else { continue };
        match name {
            "OrderlessConditions" => o.orderless = cx.is_true(val),
            "BooleanResult" => o.boolean = cx.is_true(val),
            "ConditionSimplifier" => o.reduce = val == h.sym_term(h.sym("reduce")),
            _ => {}
        }
    }
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
    // the waves as floor expressions (defined in the calculus library)
    let h2 = b.heap();
    let wave_heads = ["sawtooth_wave", "square_wave", "triangle_wave"];
    if wave_heads.iter().any(|n| h2.mask(f) & h2.mask(h2.sym_term(h2.sym(n))) != 0) {
        // (the mask can match by chance: nothing to rewrite means the usual expansion)
        let plain = cx.eval(h2.call(h2.sym("calc_waves"), &[f]))?;
        if plain != f {
            let mut call = vec![plain];
            call.extend(args.iter().skip(1).copied());
            return Ok(Some(cx.eval(h2.call(h2.sym("piecewise_expand"), &call))?));
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
    let vars = variables(b, f);
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
        // nothing to expand: the expression itself (B26-24)
        return Ok(Some(f));
    }
    // simplify each condition; drop the empty ones
    let mut simp: Vec<Case<'h>> = vec![];
    for (v, c) in cases {
        cx.tick(1)?;
        let Some(c) = simplify_cond(cx, b, c, reg, &vars, opts.reduce)? else { continue };
        let v = cx.eval(v)?;
        simp.push((v, c));
    }
    if simp.is_empty() {
        return Ok(None);
    }
    // the default: a zero piece, else the last
    let zero = b.zero();
    let di = simp.iter().position(|&(v, _)| v == zero).unwrap_or(simp.len() - 1);
    let (d, _) = simp.remove(di);
    let mut vals: Vec<T<'h>> = simp.iter().map(|p| p.0).collect();
    cx.sort(&mut vals);
    let mut sorted = vec![];
    for v in vals {
        if let Some(i) = simp.iter().position(|p| p.0 == v) {
            sorted.push(simp.remove(i));
        }
    }
    let pieces: Vec<T<'h>> = sorted.iter().map(|&(v, c)| cx.list(&[v, c])).collect();
    let pl = cx.list(&pieces);
    Ok(Some(cx.eval(b.call("piecewise", &[pl, d]))?))
}

/// Does `t` contain a piecewise head (or a step function)?
fn contains_piecewise<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => {
            b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "piecewise" | "if" | "max" | "min" | "clip" | "unit_step" | "boole" | "abs" | "sign" | "floor" | "ceiling" | "round" | "integer_part" | "prime_pi"))
                || args.iter().any(|&a| contains_piecewise(b, a))
        }
        _ => false,
    }
}

/// Is some condition of a piecewise or `if` in `t` a symbol (not a relation)?
fn has_symbol_condition<'h>(b: &Build<'h>, t: T<'h>) -> bool {
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
    args.iter().any(|&a| has_symbol_condition(b, a))
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
            Some(vec![(lo, c1), (hi, c2), (u, c3)])
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
        "abs" if args.len() == 1 && sets::poly_degree(b, args[0], vars).is_some() => {
            let u = args[0];
            Some(vec![(b.neg(u), cx.eval(b.call("less", &[u, b.zero()]))?), (u, cx.eval(b.call("greater_equal", &[u, b.zero()]))?)])
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
    Ok(Some(combos.into_iter().map(|(vals, c)| (h.app(head, &vals), c)).collect()))
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
        let linear = ps.iter().all(|&p| match h.view(p) {
            View::App { args, .. } => args.iter().all(|&a| sets::poly_degree(b, a, &[x]).is_some_and(|d| d <= 1)),
            _ => false,
        });
        let r = if linear || full_reduce {
            let r = cx.eval(b.call("reduce", &[all, x, h.sym_term(h.sym("reals"))]))?;
            if is_head(b, r, "reduce") || is_head(b, r, names::FAILURE) {
                all
            } else {
                r
            }
        } else {
            let mut v = vec![];
            for &p in &ps {
                let moved = move_right(cx, b, p)?;
                v.push(sets::tidy_rel(cx, b, moved)?);
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
