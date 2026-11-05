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
    Ok(ring
        .kernels
        .iter()
        .copied()
        .filter(|&k| matches!(h.view(k), View::Sym(s) if !CONSTANTS.contains(&h.sym_name(s).as_str())))
        .collect())
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
    // exponentials exp(c u) or b^(c u) of one argument u with rational ratios: powers of one exponential
    if bad.len() > 1 {
        if let Some(rules) = exponential_family(env, bad) {
            let mut new = vec![];
            for &e in eqs {
                new.push(env.subst(e, &rules, m)?);
            }
            return solve_eqs(env, &new, xs, m, depth + 1);
        }
    }
    // one kernel f(u) holding the unknown: solve for it, then invert
    if xs.len() == 1 && eqs.len() == 1 && bad.len() == 1 {
        let k = bad[0];
        let y = env.sym(&format!("solve`y{depth}"));
        let e = env.subst(eqs[0], &[(k, y)], m)?;
        if env.contains_any(e, xs) {
            return Ok(Err("the unknown occurs both inside and outside a function"));
        }
        let ys = match solve_eqs(env, &[e], &[y], m, depth + 1)? {
            Ok(s) => s,
            Err(why) => return Ok(Err(why)),
        };
        let mut out = vec![];
        for s in ys {
            let Some(&(_, v)) = s.first() else {
                return Ok(Err("the equation holds for every value of a function of the unknown"));
            };
            let Some((inner, value)) = invert(env, k, v, xs[0]) else {
                return Ok(Err("no inverse is known for a function of the unknown"));
            };
            let eq = env.ar.sub(inner, value);
            match solve_eqs(env, &[eq], xs, m, depth + 1)? {
                Ok(sols) => {
                    for s in sols {
                        // principal values: keep what does not fail a check of the original equation
                        if verify(env, eqs, &s, m)? != Some(false) {
                            out.push(s);
                        }
                    }
                }
                Err(why) => return Ok(Err(why)),
            }
        }
        return Ok(Ok(out));
    }
    Ok(Err("the equations are not polynomial in the unknowns, and no method applies"))
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

/// Rules rewriting kernels `exp(c_i u)` (or `b^(c_i u)` with one base b free of the unknowns) as integer powers of
/// one kernel `exp(g u)`, g the gcd of the rational ratios; `None` unless every kernel has that form.
fn exponential_family<'h>(env: &Env<'h>, bad: &[T<'h>]) -> Option<Vec<(T<'h>, T<'h>)>> {
    let h = env.h();
    // (base, exponent) with base None for exp
    let parts: Vec<(Option<T<'h>>, T<'h>)> = bad
        .iter()
        .map(|&k| match h.view(k) {
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
        None => env.call("exp", &[arg]),
        Some(b) => env.ar.pow(b, arg),
    };
    let mut rules = vec![];
    for (i, &k) in bad.iter().enumerate() {
        let n = env.ar.div(env.ar.number(&mc_arith::Num::rat(ratios[i].clone())), g);
        rules.push((k, env.ar.pow(kernel, n)));
    }
    Some(rules)
}

/// `inner == value'` equivalent (on principal branches) to `k == v` for a kernel k holding the unknown x.
fn invert<'h>(env: &Env<'h>, k: T<'h>, v: T<'h>, x: T<'h>) -> Option<(T<'h>, T<'h>)> {
    let h = env.h();
    if let View::App { head, args: &[b, e] } = h.view(k) {
        if head == env.hd.pow {
            let (in_b, in_e) = (env.contains_any(b, &[x]), env.contains_any(e, &[x]));
            return match (in_b, in_e) {
                // b^e == v with e holding x: e == log(v) / log(b)
                (false, true) => {
                    if let Some(k) = integer_log(env, b, v) {
                        return Some((e, env.int(k)));
                    }
                    let lv = env.call("log", &[v]);
                    if env.is_sym(b, "e") {
                        Some((e, lv))
                    } else {
                        Some((e, env.ar.div(lv, env.call("log", &[b]))))
                    }
                }
                // b^e == v with b holding x: b == v^(1/e)
                (true, false) => Some((b, env.ar.pow(v, env.ar.pow(e, env.int(-1))))),
                _ => None,
            };
        }
    }
    let (name, args) = env.head_name(k)?;
    let &[u] = args else { return None };
    let inverse = match name.as_str() {
        "exp" => "log",
        "log" => "exp",
        "sin" => "asin",
        "cos" => "acos",
        "tan" => "atan",
        "sinh" => "asinh",
        "cosh" => "acosh",
        "tanh" => "atanh",
        "asin" => "sin",
        "acos" => "cos",
        "atan" => "tan",
        "asinh" => "sinh",
        "acosh" => "cosh",
        "atanh" => "tanh",
        _ => return None,
    };
    Some((u, env.call(inverse, &[v])))
}

/// The result term: a list of solutions, each a list of rules `x -> value`. Solutions whose values are all closed
/// numeric terms are sorted by their machine values (real part, then imaginary part, unknown by unknown).
pub(crate) fn result_term<'h>(env: &Env<'h>, sols: Vec<Solution<'h>>) -> T<'h> {
    let h = env.h();
    let rule = h.sym(mc_api::names::RULE);
    let keys: Option<Vec<Vec<(f64, f64)>>> =
        sols.iter().map(|s| s.iter().map(|&(_, v)| env.machine(v)).collect::<Option<Vec<_>>>()).collect();
    let mut order: Vec<usize> = (0..sols.len()).collect();
    if let Some(keys) = keys {
        order.sort_by(|&a, &b| {
            for (x, y) in keys[a].iter().zip(&keys[b]) {
                let o = x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1));
                if o.is_ne() {
                    return o;
                }
            }
            std::cmp::Ordering::Equal
        });
    }
    let items: Vec<T<'h>> = order
        .into_iter()
        .map(|i| {
            let rules: Vec<T<'h>> = sols[i].iter().map(|&(x, v)| h.call(rule, &[x, v])).collect();
            env.list(&rules)
        })
        .collect();
    env.list(&items)
}
