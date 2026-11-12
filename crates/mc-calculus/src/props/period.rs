//! `function_period` (decision R25P.5): the smallest positive period of a function of one variable (a list of
//! periods for several variables, one per variable with the others fixed; a list of one complex period over
//! the complexes); `0` for a function that is not periodic.
//!
//! - **Candidates**: every periodic subterm `g(w)` with `w = a x + b` linear in `x` has the period `p / a` for the
//!   period `p` of `g`: `2 pi` (`sin`, `cos`, `sec`, `csc`), `pi` (`tan`, `cot`), `2 pi im` (`sinh`, `cosh`,
//!   `sech`, `csch`, `exp`), `pi im` (`tanh`, `coth`), `1` (the waves, `fractional_part`), `m` (`mod(w, m)`); a
//!   power `c^w` of a constant is `exp(w log(c))` (`(-1)^n` has the period 2). Over the integers `mod(p(n), m)`
//!   of a polynomial with integer coefficients has the candidate `m`.
//! - **Combination**: the candidates must be commensurable (rational ratios); their least common multiple `T` is
//!   a period. Over the reals a candidate that is not real, or a dependence on `x` outside the periodic subterms
//!   when `f` is a polynomial, means no period (`0`); incommensurable candidates mean no period.
//! - **Reduction and check**: the smallest period is `T / k` for the largest `k <= 24` (over the integers the
//!   smallest divisor `d` of the integer period) such that `f(x + T/k) == f(x)` at six points to 30 digits; `T`
//!   itself is checked the same way before it is returned, so every answer is checked numerically.
use super::sets::is_head;
use super::spec::{Dom, Spec};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// `function_period` for the call `s`.
pub fn period<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>) -> Res<Option<T<'h>>> {
    let mut out = vec![];
    for &x in &s.vars {
        let others: Vec<T<'h>> = s.vars.iter().copied().filter(|&v| v != x).collect();
        let Some(p) = one(cx, b, s, x, &others)? else { return Ok(None) };
        out.push(p);
    }
    Ok(Some(if s.vars.len() == 1 && s.dom != Dom::Complexes {
        out[0]
    } else if s.dom == Dom::Complexes && s.vars.len() == 1 {
        let z = b.zero();
        if out[0] == z {
            out[0]
        } else {
            cx.list(&out)
        }
    } else {
        cx.list(&out)
    }))
}

/// The candidates of `t` in `x`: `Ok(false)` when some dependence on `x` is outside a periodic subterm.
fn candidates<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, x: T<'h>, integers: bool, out: &mut Vec<T<'h>>) -> Res<bool> {
    let h = b.heap();
    cx.tick(1)?;
    if is_free(b, t, x) {
        return Ok(true);
    }
    if t == x {
        return Ok(false);
    }
    let View::App { head, args } = h.view(t) else { return Ok(false) };
    let Some(s) = b.sym_of(head) else { return Ok(false) };
    let name = h.sym_name(s).to_string();
    let pi = h.sym_term(h.sym("pi"));
    let imu = h.sym_term(h.sym("im"));
    // the coefficient of x in w when w is linear in x
    let slope = |cx: &mut NativeCx<'_, '_, 'h>, w: T<'h>| -> Res<Option<T<'h>>> {
        let a = cx.eval(b.call("d", &[w, x]))?;
        if !is_free(b, a, x) || is_head(b, a, "d") || a == b.zero() {
            return Ok(None);
        }
        Ok(Some(a))
    };
    if s == b.hd.pow {
        let [base, ex] = args else { return Ok(false) };
        if is_free(b, *base, x) {
            // c^w = exp(w log(c))
            let l = if *base == h.sym_term(b.hd.e) { b.one() } else { cx.eval(b.call("log", &[*base]))? };
            let Some(a) = slope(cx, *ex)? else {
                // exp of a periodic function (exp(sin(x))): the periods of the exponent (B26-23)
                let before = out.len();
                return Ok(candidates(cx, b, *ex, x, integers, out)? && out.len() > before);
            };
            out.push(cx.eval(b.div(b.mul(&[b.int(2), pi, imu]), b.mul(&[a, l])))?);
            return Ok(true);
        }
        if is_free(b, *ex, x) {
            return candidates(cx, b, *base, x, integers, out);
        }
        return Ok(false);
    }
    if s == b.hd.add || s == b.hd.mul || s == b.hd.list {
        // `w - floor(w)` is the fractional part of a linear `w`: period 1 / slope (D26-17)
        let mut skip = vec![false; args.len()];
        if s == b.hd.add {
            for (i, &a) in args.iter().enumerate() {
                let View::App { head: mh, args: &[m, fl] } = h.view(a) else { continue };
                if b.sym_of(mh) != Some(b.hd.mul) || m != b.int(-1) {
                    continue;
                }
                let View::App { head: fh, args: &[w] } = h.view(fl) else { continue };
                if b.sym_of(fh).map(|f| h.sym_name(f).to_string()).as_deref() != Some("floor") {
                    continue;
                }
                if let Some(j) = args.iter().position(|&o| o == w) {
                    if let Some(sl) = slope(cx, w)? {
                        if sl == b.one() {
                            skip[i] = true;
                            skip[j] = true;
                            out.push(b.one());
                        }
                    }
                }
            }
        }
        for (i, &a) in args.iter().enumerate() {
            if !skip[i] && !candidates(cx, b, a, x, integers, out)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    let base = match name.as_str() {
        "sin" | "cos" | "sec" | "csc" => Some(b.mul(&[b.int(2), pi])),
        "tan" | "cot" => Some(pi),
        "sinh" | "cosh" | "sech" | "csch" => Some(b.mul(&[b.int(2), pi, imu])),
        "tanh" | "coth" => Some(b.mul(&[pi, imu])),
        "square_wave" | "triangle_wave" | "sawtooth_wave" | "fractional_part" => Some(b.one()),
        _ => None,
    };
    if let (Some(p), [w]) = (base, args) {
        let Some(a) = slope(cx, *w)? else {
            // a periodic function of a periodic one (sin(cos(x))): the periods of the inner function (B26-23)
            let before = out.len();
            return Ok(candidates(cx, b, *w, x, integers, out)? && out.len() > before);
        };
        out.push(cx.eval(b.div(p, a))?);
        return Ok(true);
    }
    if name == "mod" && args.len() == 2 && is_free(b, args[1], x) {
        let (w, m) = (args[0], args[1]);
        if let Some(a) = slope(cx, w)? {
            out.push(cx.eval(b.div(m, a))?);
            return Ok(true);
        }
        if integers && matches!(h.view(m), View::Int(_)) && integer_polynomial(cx, b, w, x)? {
            out.push(m);
            return Ok(true);
        }
        return Ok(false);
    }
    // a function of periodic subterms (abs(sin(x)), sin(x)^2, exp(cos(x))): their candidates
    if !matches!(name.as_str(), "floor" | "ceiling" | "round" | "integer_part" | "unit_step" | "sign" | "abs" | "exp" | "log" | "sqrt")
        && !cx.is_defined(s)
    {
        return Ok(false);
    }
    for &a in args {
        if !candidates(cx, b, a, x, integers, out)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Is `w` a polynomial in `x` with integer coefficients?
fn integer_polynomial<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, w: T<'h>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    let cl = cx.eval(b.call("coefficient_list", &[w, x]))?;
    Ok(match cx.as_list(cl) {
        Some(cs) => cs.iter().all(|&c| matches!(h.view(c), View::Int(_) | View::BigInt { .. })),
        None => false,
    })
}

/// The period of `f` in `x` (the other variables fixed).
fn one<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, x: T<'h>, others: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let integers = s.dom == Dom::Integers;
    let mut cands = vec![];
    let mut ok = true;
    for &f in &s.comps {
        if !candidates(cx, b, f, x, integers, &mut cands)? {
            ok = false;
        }
    }
    let zero = b.zero();
    // a constant function has no smallest period: 0 (D26-10)
    if s.comps.iter().all(|&f| is_free(b, f, x)) {
        return Ok(Some(zero));
    }
    if !ok {
        // not periodic when f is a nonconstant polynomial or rational function of x; otherwise not decided
        let all_rational = s.comps.iter().all(|&f| super::sets::poly_degree(b, f, &[x]).is_some());
        // an unbounded staircase `floor(a x + c)` is not periodic either
        let staircase = s.comps.iter().all(|&f| {
            matches!(h.view(f), View::App { head, args: &[w] }
                if b.sym_of(head).is_some_and(|n| matches!(h.sym_name(n).as_str(), "floor" | "ceiling" | "round" | "integer_part"))
                    && super::sets::poly_degree(b, w, &[x]) == Some(1))
        });
        return Ok(if all_rational || staircase { Some(zero) } else { None });
    }
    if cands.is_empty() {
        return Ok(None);
    }
    // a period that is not real has no meaning over the reals and the integers: not periodic there (the
    // parameters are real there)
    let ps = super::spec::params(b, &s.comps, &s.vars);
    let mut facts = s.facts.clone();
    if !ps.is_empty() && s.dom != Dom::Complexes {
        facts.push(h.call(h.sym("element"), &[cx.list(&ps), h.sym_term(h.sym("reals"))]));
    }
    let mut fixed = vec![];
    for &p in &cands {
        let p = if s.dom == Dom::Complexes {
            p
        } else {
            let im = cx.eval_assuming(&facts, b.call("refine", &[b.call("im_part", &[p])]))?;
            if im != zero {
                let r = cx.eval_assuming(&facts, b.call("simplify", &[im]))?;
                if r != zero {
                    return Ok(if ps.is_empty() { Some(zero) } else { None });
                }
            }
            let a = cx.eval_assuming(&facts, b.call("refine", &[b.call("abs", &[p])]))?;
            if is_head(b, a, "abs") {
                return Ok(None);
            }
            a
        };
        fixed.push(p);
    }
    // numerically constant (`sin(x)^2 + cos(x)^2`): every shift is a period, so there is no smallest one
    if !integers {
        let sqrt2 = cx.eval(b.call("sqrt", &[b.int(2)]))?;
        if is_period(cx, b, s, x, others, b.rat(1357, 10000), false)? && is_period(cx, b, s, x, others, sqrt2, false)? {
            return Ok(Some(zero));
        }
    }
    // commensurable: rational ratios to the first
    let p0 = fixed[0];
    let (mut num_l, mut den_g) = (1i64, 0i64);
    for &p in &fixed {
        let r = cx.eval(b.call("simplify", &[b.div(p, p0)]))?;
        let (n, d) = match h.view(r) {
            View::Int(k) => (k, 1),
            View::Rational { num, den } => match (h.view(num), h.view(den)) {
                (View::Int(n), View::Int(d)) => (n, d),
                _ => return Ok(None),
            },
            _ => {
                // an irrational ratio of two numeric periods: not periodic
                let rn = cx.eval(b.call("n", &[r]))?;
                return Ok(if matches!(h.view(rn), View::Real(_)) { Some(zero) } else { None });
            }
        };
        let n = n.abs();
        num_l = lcm(num_l, n);
        den_g = gcd(den_g, d);
        if num_l > 1 << 40 {
            return Ok(None);
        }
    }
    let t = cx.eval(b.mul(&[b.rat(num_l, den_g.max(1)), p0]))?;
    if integers {
        // the integer period: the numerator of t when t is rational
        let tn = match h.view(t) {
            View::Int(k) => k,
            View::Rational { num, .. } => match h.view(num) {
                View::Int(k) => k,
                _ => return Ok(None),
            },
            _ => {
                let rn = cx.eval(b.call("n", &[t]))?;
                return Ok(if matches!(h.view(rn), View::Real(_)) { Some(zero) } else { None });
            }
        };
        let tn = tn.abs();
        if tn == 0 || tn > 100_000 {
            return Ok(None);
        }
        for d in 1..=tn {
            if tn % d != 0 {
                continue;
            }
            cx.tick(1)?;
            if is_period(cx, b, s, x, others, b.int(d), true)? {
                return Ok(Some(b.int(d)));
            }
        }
        return Ok(None);
    }
    for k in (1..=24).rev() {
        cx.tick(1)?;
        let p = cx.eval(b.div(t, b.int(k)))?;
        if is_period(cx, b, s, x, others, p, false)? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

fn lcm(a: i64, b: i64) -> i64 {
    if a == 0 || b == 0 {
        0
    } else {
        (a / gcd(a, b) * b).abs()
    }
}

/// `f(x + p) == f(x)` at six points to 30 digits (integers for a sequence; complex points over the complexes),
/// the other variables and the parameters at fixed values; at least three points must be defined.
fn is_period<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, x: T<'h>, others: &[T<'h>], p: T<'h>, integers: bool) -> Res<bool> {
    let h = b.heap();
    let ps = super::spec::params(b, &s.comps, &s.vars);
    let imu = h.sym_term(h.sym("im"));
    let mut fixed = vec![];
    for (i, &o) in others.iter().chain(ps.iter()).enumerate() {
        fixed.push(h.call(h.sym(names::RULE), &[o, b.rat(37 + 11 * i as i64, 29)]));
    }
    let points: Vec<T<'h>> = if integers {
        (0..6).map(|k| b.int(3 * k - 7)).collect()
    } else if s.dom == Dom::Complexes {
        [(13, 17, 5, 11), (-7, 9, 3, 13), (21, 23, -2, 7), (3, 31, 1, 3), (-19, 13, -5, 17), (5, 37, 9, 29)]
            .iter()
            .map(|&(a, c, d, e)| b.add(&[b.rat(a, c), b.mul(&[b.rat(d, e), imu])]))
            .collect()
    } else {
        [(13, 17), (-7, 9), (21, 23), (3, 31), (-19, 13), (45, 37)].iter().map(|&(a, c)| b.rat(a, c)).collect()
    };
    let fr = cx.list(&fixed);
    let p = cx.eval(h.call(h.sym("rewrite"), &[p, fr]))?;
    // a huge integer power underflows the samples: `g^N` has the period of `g` (N odd) or of `abs(g)` (N even)
    let comps: Vec<T<'h>> = s.comps.iter().map(|&f| strip_big_power(b, f)).collect();
    // a step function changes only at its jumps: samples spread over the periods (B26-1)
    let steps = !integers && comps.iter().any(|&f| has_steps(b, f));
    let points: Vec<T<'h>> = if steps {
        let pn = cx.eval(h.call(h.sym("n"), &[p]))?;
        let pn = if matches!(h.view(pn), View::Real(_)) { p } else { b.one() };
        (0..120i64).map(|j| b.add(&[b.rat(1, 17), b.mul(&[b.rat(37 * j, 100), pn])])).collect()
    } else {
        points
    };
    let mut good = 0;
    for x0 in points {
        cx.tick(1)?;
        let mut rules = fixed.clone();
        rules.push(h.call(h.sym(names::RULE), &[x, x0]));
        let mut rules2 = fixed.clone();
        rules2.push(h.call(h.sym(names::RULE), &[x, b.add(&[x0, p])]));
        let (r1, r2) = (cx.list(&rules), cx.list(&rules2));
        for &f in &comps {
            let a = cx.eval(h.call(h.sym("rewrite"), &[f, r1]))?;
            let c = cx.eval(h.call(h.sym("rewrite"), &[f, r2]))?;
            let av = cx.eval(h.call(h.sym("n"), &[a, b.int(30)]))?;
            let cv = cx.eval(h.call(h.sym("n"), &[c, b.int(30)]))?;
            if !numeric(h, av) || !numeric(h, cv) {
                continue;
            }
            // |c - a|^2 <= 10^-40 (1 + |a|^2), through the real and imaginary parts (complex balls)
            let sq = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>| -> Res<T<'h>> {
                let re = cx.eval(b.call("re", &[t]))?;
                let im = cx.eval(b.call("im_part", &[t]))?;
                cx.eval(b.add(&[b.powi(re, 2), b.powi(im, 2)]))
            };
            let d = cx.eval(b.sub(cv, av))?;
            let (dd, aa) = (sq(cx, d)?, sq(cx, av)?);
            let tol = b.mul(&[b.add(&[b.one(), aa]), b.pow(b.int(10), b.int(-40))]);
            let ok = cx.eval(b.call("less_equal", &[dd, tol]))?;
            if !cx.is_true(ok) {
                return Ok(false);
            }
            good += 1;
        }
    }
    Ok(good >= 3)
}

/// `g` for `g^N`, `abs(g)` for an even `N`, when `N` is an integer of at least 64 in absolute value.
fn strip_big_power<'h>(b: &Build<'h>, f: T<'h>) -> T<'h> {
    let h = b.heap();
    if let View::App { head, args: &[g, n] } = h.view(f) {
        if b.sym_of(head) == Some(b.hd.pow) {
            if let View::Int(k) = h.view(n) {
                if k.unsigned_abs() >= 64 && !matches!(h.view(g), View::Int(_) | View::Rational { .. }) {
                    return if k % 2 == 0 { b.call("abs", &[g]) } else { g };
                }
            }
        }
    }
    f
}

/// Does `f` contain a function that is constant between jumps?
fn has_steps<'h>(b: &Build<'h>, f: T<'h>) -> bool {
    let h = b.heap();
    match h.view(f) {
        View::App { head, args } => {
            b.sym_of(head).is_some_and(|s| {
                matches!(h.sym_name(s).as_str(), "floor" | "ceiling" | "round" | "integer_part" | "sign" | "unit_step" | "mod" | "fractional_part" | "heaviside_theta" | "unit_box" | "clip" | "square_wave")
            }) || args.iter().any(|&a| has_steps(b, a))
        }
        _ => false,
    }
}

/// A number (real, complex or a ball)?
fn numeric<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    match h.view(t) {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. } => true,
        View::App { head, args } => {
            matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "add" | "mul" | "complex"))
                && args.iter().all(|&a| numeric(h, a) || matches!(h.view(a), View::Sym(s) if h.sym_name(s) == "im"))
        }
        _ => false,
    }
}
