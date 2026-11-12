//! `solve`: reading equations, choosing the method, transcendental and radical equations, and the result.
//!
//! Equations `a == b` become `a - b`; lists and `and` give systems; `true` is dropped and `false` has no
//! solution. Each difference is read as a rational function in its kernels (denominators cleared; a solution
//! that makes a denominator vanish is discarded, decision 3B.9). Then:
//!
//! - every kernel free of the unknowns: linear systems by fraction-free elimination ([`crate::linear`]), one
//!   polynomial equation in one unknown by [`crate::univariate`] (roots repeated by multiplicity, decision 3B.10),
//!   other systems by Groebner bases ([`crate::system`]);
//! - kernels containing an unknown ([`nonpolynomial`]): radicals `u^(p/q)` become new unknowns `y` with
//!   `y^q = u` and the candidates are checked in the original equation; a single other kernel `f(u)` holding the
//!   unknown is solved for as a polynomial unknown and inverted (`exp`, `log`, trigonometric and hyperbolic
//!   functions and their inverses, powers): principal values only (decision 3B.11).
use crate::cx::Env;
use crate::linear::{is_linear, solve_linear};
use crate::system::solve_system;
use crate::univariate::poly_roots;
use mc_api::{Store, View, T};
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// One solution: (unknown, value) for the unknowns it determines.
pub(crate) type Solution<'h> = Vec<(T<'h>, T<'h>)>;

/// Why `solve` gives no answer.
pub(crate) type Unsolved = &'static str;

/// The deepest nesting of inversions (`exp(sin(x)) == 2` is depth 2).
const MAX_DEPTH: usize = 8;

/// The constants that are never unknowns when the unknowns are not given.
const CONSTANTS: &[&str] = &["pi", "e", "im", "euler_gamma", "catalan", "inf", "true", "false", "null"];

/// The equations of `t` as differences `lhs - rhs`: `Ok(None)` for `false`, an error message for inequalities and
/// other relations.
pub(crate) fn read_equations<'h>(env: &Env<'h>, t: T<'h>) -> Result<Option<Vec<T<'h>>>, &'static str> {
    let mut out = vec![];
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if env.is_sym(u, "true") {
            continue;
        }
        if env.is_sym(u, "false") {
            return Ok(None);
        }
        if let Some(items) = env.as_list(u).or_else(|| env.args_of(u, "and")) {
            stack.extend(items.iter().rev());
            continue;
        }
        if let Some(args) = env.args_of(u, "equal") {
            for w in args.windows(2) {
                out.push(env.ar.sub(w[0], w[1]));
            }
            continue;
        }
        if let Some((name, _)) = env.head_name(u) {
            if matches!(
                name.as_str(),
                "less" | "less_equal" | "greater" | "greater_equal" | "unequal" | "or" | "not"
            ) {
                return Err("solve takes equations (use reduce for inequalities)");
            }
        }
        // a bare expression e means e == 0
        out.push(u);
    }
    Ok(Some(out))
}

/// The unknowns: one kernel or a list of kernels.
pub(crate) fn read_unknowns<'h>(env: &Env<'h>, t: T<'h>) -> Result<Vec<T<'h>>, &'static str> {
    let items: Vec<T<'h>> = match env.as_list(t) {
        Some(xs) => xs.to_vec(),
        None => vec![t],
    };
    for &x in &items {
        if env.ar.num(x).is_some() {
            return Err("a number is not an unknown");
        }
        if let Some((name, _)) = env.head_name(x) {
            if matches!(name.as_str(), "add" | "mul" | "list") {
                return Err("an unknown must be a symbol or a function call, not a sum or product");
            }
        }
    }
    Ok(items)
}

/// The symbols of `eqs` other than the constants, in canonical order (when the unknowns are not given).
pub(crate) fn symbols_of<'h>(env: &Env<'h>, eqs: &[T<'h>], m: &mut Meter) -> WorkResult<Vec<T<'h>>> {
    let h = env.h();
    let ring = env.ring(eqs, &[], m)?;
    let found: Vec<T<'h>> = ring
        .kernels
        .iter()
        .copied()
        .filter(|&k| matches!(h.view(k), View::Sym(s) if !CONSTANTS.contains(&h.sym_name(s).as_str())))
        .collect();
    if !found.is_empty() {
        return Ok(found);
    }
    // only function kernels (`sqrt(x) == 3/2`, `exp(x) == 2`): the symbols that occur as arguments, in order
    let mut out: Vec<T<'h>> = vec![];
    let mut stack: Vec<T<'h>> = eqs.iter().rev().copied().collect();
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if !CONSTANTS.contains(&h.sym_name(s).as_str()) && !out.contains(&u) {
                    out.push(u);
                }
            }
            View::App { args, .. } => stack.extend(args.iter().rev().copied()),
            _ => {}
        }
    }
    Ok(out)
}

/// Solve `eqs = 0` (differences) for `xs`.
pub(crate) fn solve_eqs<'h>(
    env: &Env<'h>,
    eqs: &[T<'h>],
    xs: &[T<'h>],
    m: &mut Meter,
    depth: usize,
) -> WorkResult<Result<Vec<Solution<'h>>, Unsolved>> {
    let mut work = vec![];
    for &e in eqs {
        match env.ar.num(e) {
            Some(x) if x.is_zero() => {}
            Some(_) => return Ok(Ok(vec![])),
            None => work.push(e),
        }
    }
    if work.is_empty() {
        return Ok(Ok(vec![vec![]]));
    }
    let ring = env.ring(&work, xs, m)?;
    let bad: Vec<T<'h>> =
        ring.kernels.iter().copied().filter(|k| !xs.contains(k) && env.contains_any(*k, xs)).collect();
    if !bad.is_empty() {
        return nonpolynomial(env, &work, xs, &bad, m, depth);
    }
    let rc = env.rconv();
    let mut nums: Vec<Poly> = vec![];
    let mut dens: Vec<Poly> = vec![];
    for &e in &work {
        match ratfun::read(&rc, e, &ring, m)? {
            Ok(f) => {
                if !f.num.is_zero() {
                    nums.push(f.num);
                }
                if !f.den.is_constant() {
                    dens.push(f.den);
                }
            }
            Err(_) => return Ok(Err("an equation divides by zero")),
        }
    }
    let vars: Vec<usize> = xs.iter().map(|&x| ring.var(x).expect("the unknowns are in the ring")).collect();
    if nums.is_empty() {
        return Ok(Ok(vec![vec![]]));
    }
    // a nonzero equation free of the unknowns: no solution (for generic parameters)
    if nums.iter().any(|p| vars.iter().all(|&v| p.degree(v).unwrap_or(0) == 0)) {
        return Ok(Ok(vec![]));
    }
    let sols: Vec<Solution<'h>> = if is_linear(&nums, &vars) {
        match solve_linear(env, &nums, &ring, &vars, m)? {
            None => vec![],
            Some(s) => vec![s],
        }
    } else if vars.len() == 1 && nums.len() == 1 {
        match poly_roots(env, &nums[0], &ring, vars[0], m)? {
            None => vec![vec![]],
            Some(rs) => rs.into_iter().map(|r| vec![(xs[0], r)]).collect(),
        }
    } else {
        solve_system(env, &nums, &ring, &vars, m)?
    };
    if dens.is_empty() {
        return Ok(Ok(sols));
    }
    // discard the solutions that make a denominator vanish
    let c = env.conv();
    let den_terms: Vec<T<'h>> = dens.iter().map(|d| c.int_poly_term(d, &ring, m)).collect::<WorkResult<_>>()?;
    let mut kept = vec![];
    for s in sols {
        let mut ok = true;
        for &d in &den_terms {
            let v = env.subst(d, &s, m)?;
            if env.is_zero(v, m)? {
                ok = false;
                break;
            }
        }
        if ok {
            kept.push(s);
        }
    }
    Ok(Ok(kept))
}

/// Equations with kernels `bad` that contain unknowns.
fn nonpolynomial<'h>(
    env: &Env<'h>,
    eqs: &[T<'h>],
    xs: &[T<'h>],
    bad: &[T<'h>],
    m: &mut Meter,
    depth: usize,
) -> WorkResult<Result<Vec<Solution<'h>>, Unsolved>> {
    if depth >= MAX_DEPTH {
        return Ok(Err("the equations nest functions of the unknowns too deeply"));
    }
    let h = env.h();
    // radicals u^(p/q): a new unknown y = u^(1/q) per base u with y^q = u
    let radicals: Vec<(T<'h>, T<'h>, mc_arith::num::Rat)> = bad
        .iter()
        .filter_map(|&k| match h.view(k) {
            View::App { head, args: &[b, e] } if head == env.hd.pow => {
                let q = env.ar.num(e)?.as_rat()?.clone();
                (!q.is_int()).then_some((k, b, q))
            }
            _ => None,
        })
        .collect();
    if radicals.len() == bad.len() {
        return radical_equations(env, eqs, xs, &radicals, m, depth);
    }
    // a sum of logarithms (with rational coefficients) and terms free of the unknown: one logarithm of the product,
    // log(prod u_i^(D a_i)) == -D r, solved as the rational equation prod u_i^(D a_i) == exp(-D r); every
    // solution is checked against the given equation (a root of the product with a log on another branch is
    // refuted)
    if xs.len() == 1 && eqs.len() == 1 && bad.len() >= 2 && bad.iter().all(|&k| env.args_of(k, "log").is_some_and(|a| a.len() == 1)) {
        if let Some(sols) = log_sum(env, eqs[0], xs, bad, m, depth)? {
            return Ok(Ok(sols));
        }
    }
    // one kernel f(u) holding the unknown, or powers of one exponential (exp(c u), b^(c u), x^(c a) with rational
    // ratios c): a new unknown y for the kernel, solve for y, then invert (decision 11C.9)
    if xs.len() == 1 && eqs.len() == 1 {
        let y = env.sym(&format!("solve`y{depth}"));
        let (k, e) = if bad.len() == 1 {
            (bad[0], env.subst(eqs[0], &[(bad[0], y)], m)?)
        } else if let Some((k, powers)) = exponential_family(env, bad) {
            let rules: Vec<(T<'h>, T<'h>)> = powers.iter().map(|&(b, n)| (b, env.ar.pow(y, n))).collect();
            (k, env.subst(eqs[0], &rules, m)?)
        } else {
            // a product of factors in different functions of the unknown (`sin(x) cos(x) == 0`): one by one
            if bad.len() <= 4 {
                if let Some(sols) = factored_kernels(env, eqs, xs, bad, m, depth)? {
                    return Ok(Ok(sols));
                }
            }
            return Ok(Err("the equations are not polynomial in the unknowns, and no method applies"));
        };
        if env.contains_any(e, xs) {
            // a u e^(b u) = c: the product_log normal form
            if let Some(sols) = crate::plog::solve(env, e, y, k, xs[0], m)? {
                let mut out = vec![];
                for s in sols {
                    if !refuted(env, eqs, xs, &s, m)? {
                        out.push(s);
                    }
                }
                return Ok(Ok(out));
            }
            // a product of factors in x and the kernel (`x e^x == 0`): the factors one by one
            if let Some(sols) = factored_kernel(env, eqs, xs, e, y, k, m, depth)? {
                return Ok(Ok(sols));
            }
            // log(x) with x outside (`x log(x) == c`, `log(x)/x == c`): x = e^t turns the kernel into t and x into
            // e^t, a Lambert form in t (Corless et al. 1996, section 2); every solution is checked in the equation
            if env.args_of(k, "log").is_some_and(|a| a.len() == 1 && a[0] == xs[0]) {
                let t = env.sym(&format!("solve`t{depth}"));
                let in_t = env.subst(eqs[0], &[(k, t)], m)?;
                let e_t = env.ar.pow(env.sym("e"), t);
                let in_t = env.subst(in_t, &[(xs[0], e_t)], m)?;
                if !env.contains_any(in_t, &[xs[0]]) {
                    let r = solve_eqs(env, &[in_t], &[t], m, depth + 1)?;
                    if let Ok(ts) = r {
                        let mut out: Vec<Solution<'h>> = vec![];
                        for s in ts {
                            let Some(&(_, v)) = s.first() else { continue };
                            let value = env.subst(env.ar.pow(env.sym("e"), t), &[(t, v)], m)?;
                            let sol: Solution<'h> = vec![(xs[0], value)];
                            if !out.contains(&sol) && !refuted(env, eqs, xs, &sol, m)? {
                                out.push(sol);
                            }
                        }
                        if !out.is_empty() {
                            return Ok(Ok(out));
                        }
                    }
                }
            }
            return Ok(Err("the unknown occurs both inside and outside a function"));
        }
        let ys = match solve_eqs(env, &[e], &[y], m, depth + 1)? {
            Ok(s) => s,
            Err(why) => return Ok(Err(why)),
        };
        let mut seen: Vec<T<'h>> = vec![];
        let mut out: Vec<Solution<'h>> = vec![];
        for s in ys {
            let Some(&(_, v)) = s.first() else {
                return Ok(Err("the equation holds for every value of a function of the unknown"));
            };
            // a repeated root of the kernel gives the same solutions
            if seen.contains(&v) {
                continue;
            }
            seen.push(v);
            let Some(branches) = invert(env, k, v, xs[0], m)? else {
                return Ok(Err("no inverse is known for a function of the unknown"));
            };
            for br in branches {
                let sols = if br.inner == xs[0] {
                    vec![vec![(xs[0], br.value)]]
                } else {
                    let eq = env.ar.sub(br.inner, br.value);
                    match solve_eqs(env, &[eq], xs, m, depth + 1)? {
                        Ok(sols) => sols,
                        Err(why) => return Ok(Err(why)),
                    }
                };
                for s in sols {
                    if let Some(f) = br.keep {
                        // a condition on the solution itself (re(e) > 0 for b^e == 0): kept unless it fails
                        let t = env.subst(f, &s, m)?;
                        if env.machine(t).is_some_and(|(re, _)| re <= 0.0) {
                            continue;
                        }
                    }
                    let s: Solution<'h> = match br.cond {
                        Some(c) => s.into_iter().map(|(x, v)| (x, with_condition(env, v, c))).collect(),
                        None => s,
                    };
                    if out.contains(&s) || refuted(env, eqs, xs, &s, m)? {
                        continue;
                    }
                    out.push(s);
                }
            }
        }
        return Ok(Ok(out));
    }
    Ok(Err("the equations are not polynomial in the unknowns, and no method applies"))
}

/// An equation `e == 0` (`y` standing for the kernel `k` of the unknown `x`) whose numerator is a product of
/// several non-constant factors in `x` and `y`: the solutions of each factor (with `y` put back), each checked
/// against the given equations. `None` when the numerator is irreducible.
#[allow(clippy::too_many_arguments)]
fn factored_kernel<'h>(env: &Env<'h>, eqs: &[T<'h>], xs: &[T<'h>], e: T<'h>, y: T<'h>, k: T<'h>, m: &mut Meter, depth: usize) -> WorkResult<Option<Vec<Solution<'h>>>> {
    let ring = env.ring(&[e], &[y, xs[0]], m)?;
    let Ok(f) = ratfun::read(&env.rconv(), e, &ring, m)? else { return Ok(None) };
    let fs = mc_poly::factor::factor(&f.num, false, m)?;
    let parts: Vec<Poly> = fs.factors.into_iter().map(|(p, _)| p).filter(|p| (0..ring.nvars()).any(|v| p.degree(v).unwrap_or(0) > 0)).collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let c = env.conv();
    let mut out: Vec<Solution<'h>> = vec![];
    for p in parts {
        let t = c.int_poly_term(&p, &ring, m)?;
        let t = env.subst(t, &[(y, k)], m)?;
        let sols = match solve_eqs(env, &[t], xs, m, depth + 1)? {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        for s in sols {
            if !out.contains(&s) && !refuted(env, eqs, xs, &s, m)? {
                out.push(s);
            }
        }
    }
    Ok(Some(out))
}

/// The solutions of `e == 0`, a linear combination with rational coefficients of the logarithms `logs` plus a term
/// free of the unknown (see [`nonpolynomial`]); `None` when `e` is not of that form.
fn log_sum<'h>(env: &Env<'h>, e: T<'h>, xs: &[T<'h>], logs: &[T<'h>], m: &mut Meter, depth: usize) -> WorkResult<Option<Vec<Solution<'h>>>> {
    let ys: Vec<T<'h>> = (0..logs.len()).map(|i| env.sym(&format!("solve`l{depth}_{i}"))).collect();
    let rules: Vec<(T<'h>, T<'h>)> = logs.iter().copied().zip(ys.iter().copied()).collect();
    let ey = env.subst(e, &rules, m)?;
    if env.contains_any(ey, xs) {
        return Ok(None);
    }
    let zero_all: Vec<(T<'h>, T<'h>)> = ys.iter().map(|&y| (y, env.int(0))).collect();
    let rest = env.subst(ey, &zero_all, m)?;
    let mut coefs = vec![];
    for (i, &y) in ys.iter().enumerate() {
        let mut r = zero_all.clone();
        r[i] = (y, env.int(1));
        let a = env.ar.sub(env.subst(ey, &r, m)?, rest);
        let Some(q) = env.ar.num(a).and_then(|n| n.as_rat().cloned()) else { return Ok(None) };
        coefs.push(q);
    }
    // linear in the logarithms: the combination reproduces the equation
    let lin = env.ar.add(&std::iter::once(rest).chain(ys.iter().zip(&coefs).map(|(&y, q)| env.ar.mul(&[env.ar.number(&mc_arith::Num::Real(mc_arith::Real::Exact(q.clone()))), y]))).collect::<Vec<_>>());
    if !env.is_zero(env.ar.sub(ey, lin), m)? {
        return Ok(None);
    }
    // D a_i integers
    let mut d: u64 = 1;
    for q in &coefs {
        let Some(den) = mc_arith::num::Int::from_nat(q.den.clone()).to_i64() else { return Ok(None) };
        d = lcm(d, den.unsigned_abs());
        if d > 1000 {
            return Ok(None);
        }
    }
    let dt = env.int(d as i64);
    let mut prod = vec![];
    for (k, q) in logs.iter().zip(&coefs) {
        let ai = env.ar.mul(&[env.ar.number(&mc_arith::Num::Real(mc_arith::Real::Exact(q.clone()))), dt]);
        // invariant: the logs were checked to have one argument
        let u = env.args_of(*k, "log").expect("a log")[0];
        prod.push(env.ar.pow(u, ai));
    }
    let target = env.ar.pow(env.sym("e"), env.ar.neg(env.ar.mul(&[dt, rest])));
    let eq = env.ar.sub(env.ar.mul(&prod), target);
    let eq = env.together(eq, m)?;
    let sols = match solve_eqs(env, &[eq], xs, m, depth + 1)? {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };
    let mut out = vec![];
    for s in sols {
        if !out.contains(&s) && !refuted(env, &[e], xs, &s, m)? {
            out.push(s);
        }
    }
    Ok(Some(out))
}

/// `v` under the condition `c`: `conditional_expression(v, c)`, conjoined with the condition v already has.
pub(crate) fn with_condition<'h>(env: &Env<'h>, v: T<'h>, c: T<'h>) -> T<'h> {
    match env.args_of(v, "conditional_expression") {
        Some(&[w, d]) => env.call("conditional_expression", &[w, env.call("and", &[c, d])]),
        _ => env.call("conditional_expression", &[v, c]),
    }
}

/// `v` without its condition.
pub(crate) fn strip_condition<'h>(env: &Env<'h>, v: T<'h>) -> T<'h> {
    match env.args_of(v, "conditional_expression") {
        Some(&[w, _]) => strip_condition(env, w),
        _ => v,
    }
}

/// Equations with radicals of the unknowns: new unknowns for the radicals, then a check of every candidate.
fn radical_equations<'h>(
    env: &Env<'h>,
    eqs: &[T<'h>],
    xs: &[T<'h>],
    radicals: &[(T<'h>, T<'h>, mc_arith::num::Rat)],
    m: &mut Meter,
    depth: usize,
) -> WorkResult<Result<Vec<Solution<'h>>, Unsolved>> {
    // group by base: q = the lcm of the denominators; kernel u^(a/b) = y^(a q / b)
    let mut bases: Vec<(T<'h>, u64)> = vec![];
    for (_, b, q) in radicals {
        let d = q.den.to_u128().unwrap_or(0) as u64;
        if d == 0 || d > 64 {
            return Ok(Err("a radical of the unknowns has too large a root index"));
        }
        match bases.iter_mut().find(|(u, _)| u == b) {
            Some(e) => e.1 = lcm(e.1, d),
            None => bases.push((*b, d)),
        }
    }
    let mut rules = vec![];
    let mut new_eqs = vec![];
    let mut all = xs.to_vec();
    for (i, &(u, q)) in bases.iter().enumerate() {
        let y = env.sym(&format!("solve`r{depth}_{i}"));
        all.push(y);
        new_eqs.push(env.ar.sub(env.ar.pow(y, env.int(q as i64)), u));
        for (k, b, r) in radicals {
            if *b == u {
                // u^(a/d) = y^(a q / d)
                let a = r.num.to_i64().unwrap_or(1);
                let d = r.den.to_u128().unwrap_or(1) as i64;
                rules.push((*k, env.ar.pow(y, env.int(a * (q as i64) / d))));
            }
        }
    }
    for &e in eqs {
        new_eqs.push(env.subst(e, &rules, m)?);
    }
    let sols = match solve_eqs(env, &new_eqs, &all, m, depth + 1)? {
        Ok(s) => s,
        Err(why) => return Ok(Err(why)),
    };
    let mut out: Vec<Solution<'h>> = vec![];
    for s in sols {
        let s: Solution<'h> = s.into_iter().filter(|(x, _)| xs.contains(x)).collect();
        if out.contains(&s) {
            continue;
        }
        if verify(env, eqs, &s, m)? != Some(false) {
            out.push(s);
        }
    }
    Ok(Ok(out))
}

fn lcm(a: u64, b: u64) -> u64 {
    fn gcd(a: u64, b: u64) -> u64 {
        if b == 0 {
            a
        } else {
            gcd(b, a % b)
        }
    }
    a / gcd(a, b) * b
}

/// Does the solution satisfy the equations? `Some(false)` only when a check decides it fails.
pub(crate) fn verify<'h>(env: &Env<'h>, eqs: &[T<'h>], s: &Solution<'h>, m: &mut Meter) -> WorkResult<Option<bool>> {
    let mut all = Some(true);
    for &e in eqs {
        let v = env.subst(e, s, m)?;
        let v = env.together(v, m)?;
        if env.ar.num(v).is_some_and(|x| x.is_zero()) {
            continue;
        }
        match env.numeric_zero(v) {
            Some(false) => return Ok(Some(false)),
            Some(true) => {}
            None => all = None,
        }
    }
    Ok(all)
}

/// k with b^k = v for positive rationals b != 1 and v, |k| <= 64.
fn integer_log<'h>(env: &Env<'h>, b: T<'h>, v: T<'h>) -> Option<i64> {
    let (bq, vq) = (env.ar.num(b)?.as_rat()?.clone(), env.ar.num(v)?.as_rat()?.clone());
    let zero = mc_arith::num::Rat::from_i64(0);
    let one = mc_arith::num::Rat::from_i64(1);
    if bq.cmp_value(&zero).is_le() || vq.cmp_value(&zero).is_le() || bq == one {
        return None;
    }
    let inv = bq.recip()?;
    let (mut up, mut down) = (one.clone(), one);
    for k in 0..=64i64 {
        if up == vq {
            return Some(k);
        }
        if down == vq {
            return Some(-k);
        }
        up = up.mul(&bq);
        down = down.mul(&inv);
    }
    None
}

/// The kernel `exp(g u)` (or `b^(g u)`, with one base b) that every kernel `exp(c_i u)` of `bad` is an integer power
/// of, g the gcd of the rational ratios c_i, with those powers; `None` unless every kernel has that form.
fn exponential_family<'h>(env: &Env<'h>, bad: &[T<'h>]) -> Option<(T<'h>, Vec<(T<'h>, T<'h>)>)> {
    let h = env.h();
    // (base, exponent) with base None for exp
    let parts: Vec<(Option<T<'h>>, T<'h>)> = bad
        .iter()
        .map(|&k| match h.view(k) {
            // `e^u` is the exponential (the engine's one form of `exp(u)`)
            View::App { head, args: &[b, e] } if head == env.hd.pow && matches!(h.view(b), View::Sym(s) if h.sym_name(s) == "e") => {
                Some((None, e))
            }
            View::App { head, args: &[b, e] } if head == env.hd.pow => Some((Some(b), e)),
            _ => match env.head_name(k) {
                Some((name, &[a])) if name == "exp" => Some((None, a)),
                _ => None,
            },
        })
        .collect::<Option<_>>()?;
    let base = parts[0].0;
    if parts.iter().any(|p| p.0 != base) {
        return None;
    }
    let a0 = parts[0].1;
    let ratios: Vec<mc_arith::num::Rat> =
        parts.iter().map(|p| env.ar.num(env.ar.div(p.1, a0)).and_then(|x| x.as_rat().cloned())).collect::<Option<_>>()?;
    // g = gcd of the ratios (gcd of numerators over lcm of denominators)
    let mut gn = mc_poly::Z::ZERO;
    let mut gd = mc_poly::Z::ONE;
    for r in &ratios {
        let n = mc_poly::Z::from_int(r.num.clone());
        let d = mc_poly::Z::from_int(mc_arith::num::Int::from_nat(r.den.clone()));
        gn = gn.gcd(&n);
        gd = gd.mul(&d).div_exact(&gd.gcd(&d));
    }
    let g = env.ar.div(env.ar.integer(&gn.to_int()), env.ar.integer(&gd.to_int()));
    let arg = env.ar.mul(&[g, a0]);
    let kernel = match base {
        None => env.ar.pow(env.sym("e"), arg),
        Some(b) => env.ar.pow(b, arg),
    };
    let mut powers = vec![];
    for (i, &k) in bad.iter().enumerate() {
        let n = env.ar.div(env.ar.number(&mc_arith::Num::rat(ratios[i].clone())), g);
        env.ar.num(n)?.as_int()?;
        powers.push((k, n));
    }
    Some((kernel, powers))
}

/// One branch of an inverse: `inner == value` (under `cond`, a condition on the generated parameters, kept in the
/// solution), with `keep` a quantity whose real part the solution itself must keep positive.
pub(crate) struct Branch<'h> {
    pub inner: T<'h>,
    pub value: T<'h>,
    pub cond: Option<T<'h>>,
    pub keep: Option<T<'h>>,
}

/// The next generated parameter `c(k)` for a value: k is one more than the largest index in `v`.
fn next_parameter<'h>(env: &Env<'h>, v: T<'h>) -> T<'h> {
    let h = env.h();
    let mut k = 0i64;
    let mut stack = vec![v];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == env.gen {
                if let Some(&a) = args.first() {
                    if let View::Int(i) = h.view(a) {
                        k = k.max(i);
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    h.app(env.gen, &[env.int(k + 1)])
}

/// The branches of the inverse of the kernel `k` (holding the unknown x) at the value v (decision 11C.9): periodic
/// functions give every solution with a generated integer parameter `c(n)` and the condition `element(c(n),
/// integers)` (over the reals the exponential and hyperbolic families keep their real branches only); inverse
/// functions give the function's value when v is in their range, under the condition that it is when that is not
/// decided; `None` when no inverse is known.
fn invert<'h>(env: &Env<'h>, k: T<'h>, v: T<'h>, x: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<Branch<'h>>>> {
    let h = env.h();
    let pi = env.sym("pi");
    let i = env.ar.number(&mc_arith::Num::i());
    let periodic = env.cfg.periodic;
    let c = next_parameter(env, v);
    let ints = env.call("element", &[c, env.sym("integers")]);
    let one = |inner: T<'h>, value: T<'h>| Branch { inner, value, cond: None, keep: None };
    let fam = |inner: T<'h>, value: T<'h>, period: T<'h>| {
        if periodic {
            Branch { inner, value: env.ar.add(&[value, env.ar.mul(&[period, c])]), cond: Some(ints), keep: None }
        } else {
            Branch { inner, value, cond: None, keep: None }
        }
    };
    let two_pi = env.ar.mul(&[env.int(2), pi]);
    let two_pi_i = env.ar.mul(&[env.int(2), pi, i]);
    let pi_i = env.ar.mul(&[pi, i]);
    let reals = env.cfg.reals;
    let is_zero = env.ar.num(v).is_some_and(|n| n.is_zero());
    if let View::App { head, args: &[b, e] } = h.view(k) {
        if head == env.hd.pow {
            let (in_b, in_e) = (env.contains_any(b, &[x]), env.contains_any(e, &[x]));
            return Ok(match (in_b, in_e) {
                // b^e == v with e holding x: e == (log(v) + 2 pi i c)/log(b)
                (false, true) => {
                    if is_zero {
                        return Ok(Some(vec![]));
                    }
                    let is_e = env.is_sym(b, "e");
                    let lb = if is_e { env.int(1) } else { env.call("log", &[b]) };
                    let principal = match integer_log(env, b, v) {
                        Some(n) => env.int(n),
                        None if is_e => env.call("log", &[v]),
                        None => env.ar.div(env.call("log", &[v]), lb),
                    };
                    let zero = mc_arith::num::Rat::from_i64(0);
                    let real_base = is_e || env.ar.num(b).is_some_and(|n| n.as_rat().is_some_and(|q| q.cmp_value(&zero).is_gt()));
                    if reals || !periodic || !real_base {
                        Some(vec![one(e, principal)])
                    } else {
                        Some(vec![fam(e, principal, env.ar.div(two_pi_i, lb))])
                    }
                }
                // b^e == v with b holding x: b == v^(1/e) (principal)
                (true, false) => Some(vec![one(b, env.ar.pow(v, env.ar.pow(e, env.int(-1))))]),
                // b^e == 0 with both holding x: b == 0 where re(e) > 0
                (true, true) if is_zero => Some(vec![Branch { inner: b, value: v, cond: None, keep: Some(e) }]),
                _ => None,
            });
        }
    }
    let Some((name, args)) = env.head_name(k) else { return Ok(None) };
    let &[u] = args else { return Ok(None) };
    let f = |g: &str| env.call(g, &[v]);
    let neg = |t: T<'h>| env.ar.neg(t);
    Ok(Some(match name.as_str() {
        "exp" => {
            if is_zero {
                vec![]
            } else if reals || !periodic {
                vec![one(u, f("log"))]
            } else {
                vec![fam(u, f("log"), two_pi_i)]
            }
        }
        "sin" => vec![fam(u, env.ar.add(&[pi, neg(f("asin"))]), two_pi), fam(u, f("asin"), two_pi)],
        "cos" => vec![fam(u, neg(f("acos")), two_pi), fam(u, f("acos"), two_pi)],
        // the real cube root: cube_root(u) == v has u == v^3 (a value outside the range is refuted by the check)
        "cube_root" => vec![one(u, env.ar.pow(v, env.int(3)))],
        "tan" => vec![fam(u, f("atan"), pi)],
        "cot" => vec![fam(u, f("acot"), pi)],
        "sec" => vec![fam(u, neg(f("asec")), two_pi), fam(u, f("asec"), two_pi)],
        "csc" => vec![fam(u, env.ar.add(&[pi, neg(f("acsc"))]), two_pi), fam(u, f("acsc"), two_pi)],
        "sinh" if reals => vec![one(u, f("asinh"))],
        "sinh" => vec![fam(u, env.ar.add(&[pi_i, neg(f("asinh"))]), two_pi_i), fam(u, f("asinh"), two_pi_i)],
        "cosh" if reals => vec![one(u, neg(f("acosh"))), one(u, f("acosh"))],
        "cosh" => vec![fam(u, neg(f("acosh")), two_pi_i), fam(u, f("acosh"), two_pi_i)],
        "tanh" if reals => vec![one(u, f("atanh"))],
        "tanh" => vec![fam(u, f("atanh"), pi_i)],
        "coth" if reals => vec![one(u, f("acoth"))],
        "coth" => vec![fam(u, f("acoth"), pi_i)],
        // csch(u) == v and sech(u) == v are sinh(u) == 1/v and cosh(u) == 1/v; 0 is not a value (round 37)
        "csch" | "sech" if is_zero => vec![],
        "csch" if reals => vec![one(u, f("acsch"))],
        "csch" => {
            let w = env.call("asinh", &[env.ar.pow(v, env.int(-1))]);
            vec![fam(u, env.ar.add(&[pi_i, neg(w)]), two_pi_i), fam(u, w, two_pi_i)]
        }
        "sech" if reals => vec![one(u, neg(f("asech"))), one(u, f("asech"))],
        "sech" => {
            let w = env.call("acosh", &[env.ar.pow(v, env.int(-1))]);
            vec![fam(u, neg(w), two_pi_i), fam(u, w, two_pi_i)]
        }
        // the Gudermannian function is a bijection of the reals onto (-pi/2, pi/2): over the reals its inverse;
        // inverse_gudermannian of a real value has the one real solution gudermannian(v) (round 37)
        "gudermannian" if reals => match env.machine(v) {
            Some((a, 0.0)) if a.abs() >= std::f64::consts::FRAC_PI_2 => vec![],
            _ => vec![one(u, f("inverse_gudermannian"))],
        },
        "inverse_gudermannian" if reals || env.machine(v).is_some_and(|(_, b)| b == 0.0) => vec![one(u, f("gudermannian"))],
        // logistic_sigmoid(u) = 1/(1 + e^-u) == v: u == log(v/(1 - v)), over the complexes with period 2 pi i; 0 and 1
        // are not values (round 37)
        "logistic_sigmoid" => {
            let is_one = env.ar.num(env.ar.sub(v, env.int(1))).is_some_and(|n| n.is_zero());
            if is_zero || is_one {
                vec![]
            } else {
                let w = env.call("log", &[env.ar.div(v, env.ar.sub(env.int(1), v))]);
                match env.machine(v) {
                    Some((a, 0.0)) if reals && (a <= 0.0 || a >= 1.0) => vec![],
                    _ if reals || !periodic => vec![one(u, w)],
                    _ => vec![fam(u, w, two_pi_i)],
                }
            }
        }
        // sign(u) == 0 and real_sign(u) == 0: u == 0; other values are taken on half-lines, not at points
        "sign" | "real_sign" if is_zero => vec![one(u, v)],
        // |u| == v: u == v or u == -v for v >= 0 (the real reading of abs; over the complexes the
        // solutions form a circle). A negative number v has none; a symbolic v gets the condition v >= 0.
        "abs" | "real_abs" => match env.machine(v) {
            Some((a, 0.0)) if a < 0.0 => vec![],
            Some(_) if is_zero => vec![one(u, v)],
            Some((_, 0.0)) => vec![one(u, neg(v)), one(u, v)],
            Some(_) => vec![],
            None => {
                let c = env.call("greater_equal", &[v, env.int(0)]);
                vec![Branch { inner: u, value: neg(v), cond: Some(c), keep: None }, Branch { inner: u, value: v, cond: Some(c), keep: None }]
            }
        },
        // erf(u) == v: u == inverse_erf(v); a real v with |v| >= 1 is not a value of erf at a finite point
        "erf" | "erfc" => {
            let (lo, hi, inv) = if name == "erf" { (-1.0, 1.0, "inverse_erf") } else { (0.0, 2.0, "inverse_erfc") };
            match env.machine(v) {
                Some((a, b)) if b == 0.0 && (a <= lo || a >= hi) => vec![],
                _ => vec![one(u, f(inv))],
            }
        }
        // the inverse functions: the value of the function, where v is in their range
        // acsch(u) and asech(u) are never 0 (round 37)
        "acsch" | "asech" if is_zero => vec![],
        "log" | "asin" | "acos" | "atan" | "asinh" | "acosh" | "atanh" | "acsch" | "asech" => {
            let g = match name.as_str() {
                "log" => "exp",
                "asin" => "sin",
                "acos" => "cos",
                "atan" => "tan",
                "asinh" => "sinh",
                "acosh" => "cosh",
                // acsch(z) = asinh(1/z), asech(z) = acosh(1/z): the ranges of asinh and acosh
                "acsch" => "csch",
                "asech" => "sech",
                _ => "tanh",
            };
            let value = if g == "exp" { env.ar.pow(env.sym("e"), v) } else { f(g) };
            match range_condition(env, &name, v, m)? {
                Some(None) => vec![one(u, value)],
                Some(Some(cond)) => vec![Branch { inner: u, value, cond: Some(cond), keep: None }],
                // v outside the range: no solution
                None => vec![],
            }
        }
        _ => return Ok(None),
    }))
}

/// Is `v` in the range of the principal inverse function `name`? `Some(None)` when it is, `None` when it is not,
/// `Some(Some(c))` with the condition when that is not decided (v has parameters): the honest condition in place
/// of a mere warning about inverse functions (decision 11C.10). The edges of the ranges are decided by
/// the check of the solution by substitution.
fn range_condition<'h>(env: &Env<'h>, name: &str, v: T<'h>, _m: &mut Meter) -> WorkResult<Option<Option<T<'h>>>> {
    let pi = env.sym("pi");
    let half_pi = env.ar.mul(&[env.ar.div(env.int(1), env.int(2)), pi]);
    let re = env.call("re", &[v]);
    let im = env.call("im_part", &[v]);
    let lt = |a: T<'h>, b: T<'h>| env.call("less", &[a, b]);
    let le = |a: T<'h>, b: T<'h>| env.call("less_equal", &[a, b]);
    // a closed numeric value: decided by its machine value
    if let Some((a, b)) = env.machine(v) {
        let p = std::f64::consts::PI;
        let t = 1e-12;
        let inside = match name {
            "log" => -p < b && b <= p + t,
            "asin" | "atan" => -p / 2.0 - t <= a && a <= p / 2.0 + t,
            "acos" => -t <= a && a <= p + t,
            "asinh" | "atanh" | "acsch" => -p / 2.0 - t <= b && b <= p / 2.0 + t,
            _ => a >= -t && -p < b && b <= p + t,
        };
        return Ok(if inside { Some(None) } else { None });
    }
    let cond = match name {
        // -pi < im(v) <= pi
        "log" => env.call("and", &[lt(env.ar.neg(pi), im), le(im, pi)]),
        "asin" | "atan" => env.call("and", &[lt(env.ar.neg(half_pi), re), lt(re, half_pi)]),
        "acos" => env.call("and", &[lt(env.int(0), re), lt(re, pi)]),
        "asinh" | "atanh" | "acsch" => env.call("and", &[lt(env.ar.neg(half_pi), im), lt(im, half_pi)]),
        // acosh: re(v) > 0 and -pi < im(v) <= pi
        _ => env.call("and", &[lt(env.int(0), re), lt(env.ar.neg(pi), im), le(im, pi)]),
    };
    Ok(Some(Some(cond)))
}

/// The number of nodes of a term (a size for choosing between equal forms).
pub(crate) fn size<'h>(env: &Env<'h>, t: T<'h>) -> usize {
    let h = env.h();
    let mut n = 0;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        n += 1;
        if let View::App { args, .. } = h.view(u) {
            stack.extend_from_slice(args);
        }
    }
    n
}

/// Does `t` contain a power with a fractional exponent whose denominator exceeds 2 (a cube root of Cardano's
/// formula, kept in the formula's form)?
fn has_higher_radical<'h>(env: &Env<'h>, t: T<'h>) -> bool {
    let h = env.h();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == env.hd.pow && args.len() == 2 {
                if let Some(q) = env.ar.num(args[1]).and_then(|n| n.as_rat().cloned()) {
                    if q.den.to_u128().is_some_and(|d| d > 2) {
                        return true;
                    }
                }
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// Is the tidied form `t` of `v` not larger than it, allowing for the signs it moves into the terms of its sums (two
/// nodes and one per term)?
pub(crate) fn not_larger<'h>(env: &Env<'h>, t: T<'h>, v: T<'h>) -> bool {
    let h = env.h();
    let mut terms = 0;
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == env.hd.add {
                terms += args.len();
            }
            if head == env.hd.add || head == env.hd.mul {
                stack.extend_from_slice(args);
            }
        }
    }
    size(env, t) <= size(env, v) + 2 + terms
}

/// Does `t` contain a call of the head `name`?
pub(crate) fn has_head<'h>(env: &Env<'h>, t: T<'h>, name: &str) -> bool {
    let h = env.h();
    let hd = env.sym(name);
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if head == hd {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The symbols of `t` (other than the constants) that are not in `xs`, and the generated parameters `c(k)`.
fn free_symbols<'h>(env: &Env<'h>, t: T<'h>, xs: &[T<'h>], out: &mut Vec<T<'h>>) {
    let h = env.h();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        match h.view(u) {
            View::Sym(s) => {
                if !xs.contains(&u) && !out.contains(&u) && !CONSTANTS.contains(&h.sym_name(s).as_str()) {
                    out.push(u);
                }
            }
            View::App { head, args } => {
                // the bound variable of a root object's function is not a parameter
                if head == env.sym("root") || head == env.sym("fn") {
                    continue;
                }
                if head == env.gen {
                    if !out.contains(&u) {
                        out.push(u);
                    }
                    continue;
                }
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
}

/// Sample values for the symbols of a solution that is checked numerically: generated parameters get small
/// integers, other symbols values away from the usual special points.
const SAMPLES: [[i64; 6]; 3] = [[7, 13, 21, 37, 33, 19], [17, 4, 29, 11, 8, 26], [3, 23, 9, 31, 5, 14]];

/// Is the solution `s` of `eqs` (differences) wrong? Decided exactly (the residual expands to 0), else numerically
/// on complex balls (closed terms: well beyond 30 digits), else at sample values of the parameters (and of the
/// generated integer parameters). `true` only when every check that can be made fails: a solution is dropped only
/// when it is shown wrong (decision 11C.6).
pub(crate) fn refuted<'h>(env: &Env<'h>, eqs: &[T<'h>], xs: &[T<'h>], s: &Solution<'h>, m: &mut Meter) -> WorkResult<bool> {
    let s: Solution<'h> = s.iter().map(|&(x, v)| (x, strip_condition(env, v))).collect();
    let s = &s;
    let mut open = vec![];
    for &e in eqs {
        let v = env.subst(e, s, m)?;
        let v = env.together(v, m)?;
        if env.ar.num(v).is_some_and(|x| x.is_zero()) {
            continue;
        }
        match env.numeric_zero(v) {
            Some(false) => return Ok(true),
            Some(true) => {}
            None => open.push(v),
        }
    }
    if open.is_empty() {
        return Ok(false);
    }
    let mut syms = vec![];
    for &v in &open {
        free_symbols(env, v, xs, &mut syms);
    }
    if syms.is_empty() || syms.len() > 12 {
        return Ok(false);
    }
    let (mut tried, mut failed) = (0, 0);
    'sample: for sample in SAMPLES {
        let rules: Vec<(T<'h>, T<'h>)> = syms
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let k = sample[i % 6];
                // generated parameters: small integers; parameters: k/10
                let val = if matches!(env.h().view(p), View::App { .. }) { env.int(k % 3 - 1) } else { env.ar.div(env.int(k), env.int(10)) };
                (p, val)
            })
            .collect();
        let mut fails = false;
        for &v in &open {
            let w = env.subst(v, &rules, m)?;
            match env.numeric_zero(w) {
                Some(true) => {}
                Some(false) => fails = true,
                None => continue 'sample,
            }
        }
        // right at one sample: kept (a generic solution can fail at others, on a branch cut)
        if !fails {
            return Ok(false);
        }
        tried += 1;
        failed += 1;
    }
    Ok(tried > 0 && failed == tried)
}

/// The solutions as they are returned: each checked by substitution ([`refuted`] ones dropped), values in the
/// simplest of their forms (one fraction in lowest terms with the content taken out, when that is not larger),
/// duplicates of families removed (the caller puts them in the order of solutions, [`crate::order`], once they are
/// evaluated).
pub(crate) fn finish<'h>(
    env: &Env<'h>,
    eqs: &[T<'h>],
    xs: &[T<'h>],
    sols: Vec<Solution<'h>>,
    m: &mut Meter,
) -> WorkResult<Vec<Solution<'h>>> {
    let mut out: Vec<Solution<'h>> = Vec::with_capacity(sols.len());
    let mut keys: Vec<Option<Vec<(i64, i64)>>> = vec![];
    for s in sols {
        let s: Solution<'h> = s
            .into_iter()
            .map(|(x, v)| {
                if has_head(env, v, "root") || has_head(env, v, "conditional_expression") || env.ar.num(v).is_some() || has_higher_radical(env, v) {
                    return Ok((x, v));
                }
                let t = env.tidy(v, m)?;
                Ok((x, if not_larger(env, t, v) { t } else { v }))
            })
            .collect::<WorkResult<_>>()?;
        if refuted(env, eqs, xs, &s, m)? {
            continue;
        }
        let fam = s.iter().any(|&(_, v)| has_head(env, v, "conditional_expression"));
        if fam {
            // the same family twice (`-acos(1) + 2 pi c(1)` and `acos(1) + 2 pi c(1)`): compared at a generic value
            // of the parameters
            let key = family_key(env, &s, m)?;
            if out.contains(&s) || key.is_some() && keys.contains(&key) {
                continue;
            }
            keys.push(key);
        }
        out.push(s);
    }
    Ok(out)
}

/// The machine values (rounded) of a family's values at the generated parameters 0.37, 0.71, ...: equal for the
/// same family written twice.
fn family_key<'h>(env: &Env<'h>, s: &Solution<'h>, m: &mut Meter) -> WorkResult<Option<Vec<(i64, i64)>>> {
    let mut out = vec![];
    for &(_, v) in s {
        let w = strip_condition(env, v);
        let mut ps = vec![];
        free_symbols(env, w, &[], &mut ps);
        let rules: Vec<(T<'h>, T<'h>)> =
            ps.iter().enumerate().map(|(i, &p)| (p, env.ar.div(env.int(37 + 34 * i as i64), env.int(100)))).collect();
        let w = env.subst(w, &rules, m)?;
        let Some((a, b)) = env.machine(w) else { return Ok(None) };
        out.push(((a * 1e9).round() as i64, (b * 1e9).round() as i64));
    }
    Ok(Some(out))
}

/// The result term: a list of solutions, each a list of rules `x -> value`, in the order given.
pub(crate) fn result_term<'h>(env: &Env<'h>, sols: Vec<Solution<'h>>) -> T<'h> {
    let h = env.h();
    let rule = h.sym(mc_api::names::RULE);
    let items: Vec<T<'h>> = sols
        .iter()
        .map(|s| {
            let rules: Vec<T<'h>> = s.iter().map(|&(x, v)| h.call(rule, &[x, v])).collect();
            env.list(&rules)
        })
        .collect();
    env.list(&items)
}

/// An equation whose numerator is a product of several non-constant factors in the unknown and its functions
/// `bad` (`sin(x) cos(x) == 0`): the solutions of each factor, each checked against the given equation. `None`
/// when the numerator is irreducible or a factor cannot be solved.
fn factored_kernels<'h>(env: &Env<'h>, eqs: &[T<'h>], xs: &[T<'h>], bad: &[T<'h>], m: &mut Meter, depth: usize) -> WorkResult<Option<Vec<Solution<'h>>>> {
    let ys: Vec<T<'h>> = (0..bad.len()).map(|i| env.sym(&format!("solve`k{depth}_{i}"))).collect();
    let rules: Vec<(T<'h>, T<'h>)> = bad.iter().copied().zip(ys.iter().copied()).collect();
    let e = env.subst(eqs[0], &rules, m)?;
    let mut vars = ys.clone();
    vars.push(xs[0]);
    let ring = env.ring(&[e], &vars, m)?;
    let Ok(f) = ratfun::read(&env.rconv(), e, &ring, m)? else { return Ok(None) };
    let fs = mc_poly::factor::factor(&f.num, false, m)?;
    let parts: Vec<Poly> = fs.factors.into_iter().map(|(p, _)| p).filter(|p| (0..ring.nvars()).any(|v| p.degree(v).unwrap_or(0) > 0)).collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let c = env.conv();
    let back: Vec<(T<'h>, T<'h>)> = ys.iter().copied().zip(bad.iter().copied()).collect();
    let mut out: Vec<Solution<'h>> = vec![];
    for p in parts {
        let t = c.int_poly_term(&p, &ring, m)?;
        let t = env.subst(t, &back, m)?;
        let sols = match solve_eqs(env, &[t], xs, m, depth + 1)? {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        for s in sols {
            if !out.contains(&s) && !refuted(env, eqs, xs, &s, m)? {
                out.push(s);
            }
        }
    }
    Ok(Some(out))
}
