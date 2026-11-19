//! Singular sets (decision R25P.3): the conditions under which a function is discontinuous, singular or not
//! meromorphic, and the decision whether such a set meets the region of a call.
//!
//! Every subterm that depends on the variables contributes atoms (conditions on the variables), each of a kind:
//!
//! | kind | real line | complex plane |
//! |---|---|---|
//! | pole | `u == 0` under a negative integer power; `cos(u) == 0` for `tan`, `sec`; `sin(u) == 0` for `cot`, `csc`; `sin(pi u) == 0 and u <= 0` for `gamma`, `beta`, `pochhammer`, `polygamma`; `u == 1` for `zeta` | the same, with `re(u) <= 0` for the gamma family and `cosh(u) == 0`, `sinh(u) == 0` for the hyperbolic ones |
//! | branch | `u <= 0` for `log` and fractional or symbolic powers; `u <= -1`, `u >= 1` for `asin`, `acos`, `atanh`; `u <= 1` for `acosh` | the branch point `u == 0` and the cut `im(u) == 0 and re(u) <= 0`; for the inverse functions their branch points and the two half-line cuts |
//! | jump | `sin(pi u) == 0` for `floor`, `ceiling`, `integer_part`, `fractional_part`; `sin(pi (1/2 + u)) == 0` for `round`; `sin(pi u/m) == 0` for `mod(u, m)`; `u == 0` for `unit_step`, `sign`, `heaviside_theta`, `arg` | `arg`, `sign` as on the line |
//! | kink | `u == 0` for `abs`, `ramp`; `a - b == 0` for `max`, `min`; the ends of `clip` | `true` (nowhere analytic) for `abs`, `re`, `im_part`, `conjugate`, `arg`, `sign`, `unit_step` and the waves |
//!
//! Discontinuities are the poles, branches and jumps; singularities add the kinks; a function is meromorphic when
//! the only singular atoms that meet the region are poles. An undefined head (`g(x)` of a symbol without rules) is
//! taken to be analytic, as a generic function; a defined function this module does not know declines the call.
//!
//! Atoms are tidied (a real rational constant moved to the right of `==` and of the orderings, `re` and `im_part`
//! taken through sums with real constants), evaluated, deduplicated and ordered: conditions without `and` first,
//! then the cuts, each group in canonical order. With one real variable and atoms linear in it the disjunction is
//! given to `reduce` (`x <= 1` for `1/x + log(x - 1)`).
//!
//! **Meeting the region** ([`meets`]): atoms refuted by `refine` under the region and the facts are dropped; then,
//! without parameters, `reduce` (one real variable) or the cylindrical decomposition (`resolve(exists(...))`,
//! polynomial atoms), the complex plane through the real and imaginary parts of the variables (`z -> zr + im zi`,
//! `complex_expand`); a nonconstant polynomial or entire periodic template (`cos(u) == 0` with `u` a nonconstant
//! polynomial) always has a complex zero; otherwise a numeric search for a witness, which is accepted only when
//! proved: a sign change refined by bisection, with proved opposite signs at the exact ends of the
//! bracket, an enclosure of the function over it (continuous there) and the other conjuncts true on its whole ball;
//! or an exact point inside the inequalities. With parameters the condition under which
//! the set misses the region is `resolve(for_all(vars, region implies not atoms))`, after dropping the
//! non-polynomial conjuncts of cuts (which only enlarges the set: the condition stays sufficient).
use super::spec::{Dom, Spec};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// The kind of an atom.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// an isolated pole (allowed in a meromorphic function)
    Pole,
    /// a branch point or cut, or the boundary of the real domain
    Branch,
    /// a jump of a step function
    Jump,
    /// a point where a continuous function is not analytic
    Kink,
}

/// Which atoms are wanted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// discontinuities: poles, branches, jumps
    Disc,
    /// singularities: every kind
    Sing,
}

/// One atom: a condition on the variables and its kind.
#[derive(Clone, Copy, Debug)]
pub struct Atom<'h> {
    /// the condition
    pub t: T<'h>,
    /// its kind
    pub kind: Kind,
}

/// Functions analytic in every argument everywhere (entire), real or complex.
const ENTIRE: &[&str] = &[
    "sin", "cos", "sinh", "cosh", "erf", "erfc", "erfi", "airy_ai", "airy_bi", "airy_ai_prime", "airy_bi_prime",
    "dawson_f", "fresnel_c", "fresnel_s", "sin_integral", "sinh_integral", "sinc", "exp", "laguerre_l", "hermite_h",
    "hypergeometric_0f1_regularized", "hypergeometric_1f1_regularized", "bessel_j0", "kelvin_ber", "kelvin_bei",
    "struve_h0", "fibonacci", "lucas_l",
];
/// Entire in the last argument when the others are fixed (polynomial families in their variable).
const ENTIRE_LAST: &[&str] = &["chebyshev_t", "chebyshev_u", "legendre_p", "gegenbauer_c", "jacobi_p", "hypergeometric_0f1"];
/// Entire in the variable for an integer order.
const INTEGER_ORDER: &[&str] = &["bessel_j", "bessel_i", "spherical_bessel_j"];

/// The atoms of `f` (one term or several components), or `None` for a function this module does not know.
pub fn atoms<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    comps: &[T<'h>],
    vars: &[T<'h>],
    complex: bool,
    mode: Mode,
    facts: &[T<'h>],
) -> Res<Option<Vec<Atom<'h>>>> {
    Ok(match atoms_ext(cx, b, comps, vars, complex, mode, facts)? {
        Some((v, false)) => Some(v),
        _ => None,
    })
}

/// [`atoms`], also when some subterm lists only some of its singular points (`partial`, [`super::table`]): the
/// atoms and whether they are incomplete. Incomplete atoms decide that a set meets a region, never that it misses.
pub fn atoms_ext<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    comps: &[T<'h>],
    vars: &[T<'h>],
    complex: bool,
    mode: Mode,
    facts: &[T<'h>],
) -> Res<Option<(Vec<Atom<'h>>, bool)>> {
    let mut raw: Vec<Atom<'h>> = vec![];
    let mut seen = rustc_hash::FxHashSet::default();
    let mut partial = false;
    for &f in comps {
        let f = hyperbolic_of_log(cx, b, f)?;
        if !walk(cx, b, f, vars, complex, mode, facts, &mut raw, &mut seen, &mut partial)? {
            return Ok(None);
        }
    }
    // evaluate and tidy, drop the ones that never hold, deduplicate
    let mut out: Vec<Atom<'h>> = vec![];
    for a in raw {
        let mut t = tidy_all(cx, b, a.t)?;
        if cx.is_false(t) || zero_of_zero_free(b, t) {
            continue;
        }
        // a condition that no real value of the variables satisfies (`im_part(x + im) <= 0`)
        if !complex && !vars.is_empty() && !is_free_of_parts(b, t) {
            let h = b.heap();
            let mut fs = facts.to_vec();
            fs.push(b.call("element", &[cx.list(vars), h.sym_term(h.sym("reals"))]));
            let r = cx.eval_assuming(&fs, b.call("refine", &[t]))?;
            if cx.is_false(r) {
                continue;
            }
            if cx.is_true(r) {
                t = r;
            }
        }
        if let Some(o) = out.iter_mut().find(|o| o.t == t) {
            if a.kind != Kind::Pole {
                o.kind = a.kind;
            }
            continue;
        }
        out.push(Atom { t, kind: a.kind });
    }
    Ok(Some((out, partial)))
}

/// Does `t` contain no `re` or `im_part`? (Only conditions with those parts are tested for being never true.)
fn is_free_of_parts<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } => !b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "re" | "im_part")) && args.iter().all(|&a| is_free_of_parts(b, a)),
        _ => true,
    }
}

/// Is `t` the equation `g == 0` of a `g` that has no zero: gamma (1/gamma is entire with zeros only at the poles
/// of gamma, DLMF 5.2.1), a power of a zero-free base (b^e = exp(e log b)), a product of such factors and nonzero
/// numbers? (A denominator `gamma(u)` contributes such an atom; it never holds.)
fn zero_of_zero_free<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    fn free<'h>(b: &Build<'h>, g: T<'h>) -> bool {
        let h = b.heap();
        match h.view(g) {
            View::Int(k) => k != 0,
            View::Rational { .. } | View::BigInt { .. } => true,
            View::Sym(s) => matches!(h.sym_name(s).as_str(), "e" | "pi"),
            View::App { head, args } => match b.sym_of(head).map(|s| h.sym_name(s).to_string()).as_deref() {
                Some("gamma") => args.len() == 1,
                Some("exp") => true,
                _ if b.sym_of(head) == Some(b.hd.pow) => args.len() == 2 && free(b, args[0]),
                _ if b.sym_of(head) == Some(b.hd.mul) => args.iter().all(|&a| free(b, a)),
                _ => false,
            },
            _ => false,
        }
    }
    let h = b.heap();
    match h.view(t) {
        View::App { head, args: &[l, r] } if b.sym_of(head) == Some(h.sym("equal")) && r == b.zero() => free(b, l),
        _ => false,
    }
}

/// The atoms of `piecewise([[v1, c1], ...], d)` of one real variable: the atoms of each value inside its
/// region (`c_i` and none of the earlier conditions), and the points where the pieces meet: a boundary of a
/// condition is a jump when the one-sided values and the value there differ, a pole when they grow.
#[allow(clippy::too_many_arguments)]
fn piecewise_atoms<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    u: T<'h>,
    args: &[T<'h>],
    vars: &[T<'h>],
    complex: bool,
    mode: Mode,
    facts: &[T<'h>],
    out: &mut Vec<Atom<'h>>,
) -> Res<bool> {
    let h = b.heap();
    let [x] = vars else { return Ok(false) };
    let x = *x;
    if complex || args.is_empty() || args.len() > 2 {
        return Ok(false);
    }
    let Some(rows) = cx.as_list(args[0]).map(<[T<'h>]>::to_vec) else { return Ok(false) };
    let mut pieces: Vec<(T<'h>, T<'h>)> = vec![];
    for r in rows {
        let Some(&[v, c]) = cx.as_list(r) else { return Ok(false) };
        pieces.push((v, c));
    }
    let default = args.get(1).copied().unwrap_or_else(|| b.zero());
    // the regions: first match
    let mut earlier: Vec<T<'h>> = vec![];
    let mut regions: Vec<(T<'h>, T<'h>)> = vec![];
    for &(v, c) in &pieces {
        let r = if earlier.is_empty() { c } else { b.call("and", &[c, b.call("not", &[b.call("or", &earlier)])]) };
        regions.push((v, r));
        earlier.push(c);
    }
    let rest = if earlier.is_empty() { cx.bool(true) } else { b.call("not", &[b.call("or", &earlier)]) };
    regions.push((default, rest));
    for (v, r) in regions {
        let Some(inner) = atoms(cx, b, &[v], vars, complex, mode, facts)? else { return Ok(false) };
        for a in inner {
            let t = cx.eval(b.call("and", &[a.t, r]))?;
            if cx.is_false(t) {
                continue;
            }
            let red = cx.eval(b.call("reduce", &[t, x, h.sym_term(h.sym("reals"))]))?;
            if cx.is_false(red) {
                continue;
            }
            out.push(Atom { t, kind: a.kind });
        }
    }
    // boundaries of the conditions
    let mut points: Vec<T<'h>> = vec![];
    for &(_, c) in &pieces {
        let mut stack = mc_api::walk::Walk::new(c);
        while let Some(t) = stack.pop() {
            let View::App { head, args: ra } = h.view(t) else { continue };
            let Some(sy) = b.sym_of(head) else { continue };
            match h.sym_name(sy).as_str() {
                "and" | "or" | "not" => stack.extend(ra.iter().copied()),
                "less" | "less_equal" | "greater" | "greater_equal" | "equal" | "unequal" if ra.len() == 2 => {
                    let eq = b.call("equal", &[ra[0], ra[1]]);
                    let sol = cx.eval(b.call("solve", &[eq, x, h.sym_term(h.sym("reals"))]))?;
                    let Some(items) = cx.as_list(sol).map(<[T<'h>]>::to_vec) else { return Ok(false) };
                    for it in items {
                        let Some(&[rule]) = cx.as_list(it) else { return Ok(false) };
                        let View::App { args: &[_, v], .. } = h.view(rule) else { return Ok(false) };
                        if !crate::common::is_free(b, v, x) || is_head(b, v, mc_decide::CONDITIONAL) {
                            return Ok(false);
                        }
                        if !points.contains(&v) {
                            points.push(v);
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
    }
    // each boundary point by the one-sided limits of the piecewise function and its value there (values at
    // points beside it prove nothing): an infinite limit is a pole; finite limits that differ, or a value that
    // differs from them, a jump; equal ones nothing; a limit or a comparison not decided leaves the call
    let infinite = |t: T<'h>| {
        let inf = h.sym_term(h.sym("inf"));
        t == inf || t == b.neg(inf) || t == h.sym_term(h.sym("complex_infinity"))
    };
    for e in points {
        cx.tick(1)?;
        let mut sides = vec![];
        for dir in ["from_above", "from_below"] {
            let rule = h.call(h.sym(names::RULE), &[x, e]);
            let d = h.call(h.sym(names::RULE), &[h.sym_term(h.sym("direction")), h.sym_term(h.sym(dir))]);
            let l = cx.eval(b.call("limit", &[u, rule, h.call(h.sym("options"), &[d])]))?;
            if is_head(b, l, "limit") || is_head(b, l, names::FAILURE) {
                return Ok(false);
            }
            sides.push(l);
        }
        let kind = if sides.iter().any(|&l| infinite(l)) {
            Some(Kind::Pole)
        } else {
            match super::proof::closed_sign(cx, b, b.sub(sides[0], sides[1]))? {
                None => return Ok(false),
                Some(s) if s != 0 => Some(Kind::Jump),
                Some(_) => {
                    let v = cx.eval(h.call(h.sym("rewrite"), &[u, h.call(h.sym(names::RULE), &[x, e])]))?;
                    let vn = cx.eval(h.call(h.sym("n"), &[v]))?;
                    if matches!(h.view(vn), View::Real(_) | View::Int(_)) {
                        match super::proof::closed_sign(cx, b, b.sub(v, sides[0]))? {
                            None => return Ok(false),
                            Some(0) => None,
                            Some(_) => Some(Kind::Jump),
                        }
                    } else {
                        None
                    }
                }
            }
        };
        if let Some(kind) = kind {
            out.push(Atom { t: b.call("equal", &[x, e]), kind });
        }
    }
    Ok(true)
}

/// `tanh(log(u))` and the like as rational functions of `u` (`exp(log(u)) = u` on the whole plane): `(u^2 - 1)/(u^2 + 1)`
/// is continuous at the points where `log` is not.
fn hyperbolic_of_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<T<'h>> {
    // shared subterms rewritten once
    hyperbolic_of_log_in(cx, b, t, &mut mc_api::walk::memo())
}

fn hyperbolic_of_log_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    if let Some(&r) = memo.get(&t) {
        return Ok(r);
    }
    let r = hyperbolic_of_log_node(cx, b, t, memo)?;
    memo.insert(t, r);
    Ok(r)
}

fn hyperbolic_of_log_node<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let mut new = Vec::with_capacity(args.len());
    let mut changed = false;
    for &a in args {
        let r = hyperbolic_of_log_in(cx, b, a, memo)?;
        changed |= r != a;
        new.push(r);
    }
    let name = b.sym_of(head).map(|s| h.sym_name(s).to_string());
    if let (Some(n), [a]) = (name.as_deref(), new.as_slice()) {
        if matches!(n, "tanh" | "sinh" | "cosh" | "sech") {
            if let View::App { head: lh, args: &[u] } = h.view(*a) {
                if b.sym_of(lh) == Some(h.sym("log")) {
                    let u2 = b.powi(u, 2);
                    let e = match n {
                        "tanh" => b.div(b.sub(u2, b.one()), b.add(&[u2, b.one()])),
                        "sinh" => b.div(b.sub(u2, b.one()), b.mul(&[b.int(2), u])),
                        "cosh" => b.div(b.add(&[u2, b.one()]), b.mul(&[b.int(2), u])),
                        _ => b.div(b.mul(&[b.int(2), u]), b.add(&[u2, b.one()])),
                    };
                    return cx.eval(e);
                }
            }
        }
    }
    if !changed {
        return Ok(t);
    }
    cx.eval(h.app(head, &new))
}

/// Postorder walk adding the atoms of every subterm; `false` for an unknown function.
#[allow(clippy::too_many_arguments)]
fn walk<'h>(
    cx: &mut NativeCx<'_, '_, 'h>,
    b: &Build<'h>,
    u: T<'h>,
    vars: &[T<'h>],
    complex: bool,
    mode: Mode,
    facts: &[T<'h>],
    out: &mut Vec<Atom<'h>>,
    seen: &mut rustc_hash::FxHashSet<T<'h>>,
    partial: &mut bool,
) -> Res<bool> {
    let h = b.heap();
    cx.tick(1)?;
    let depends = |t: T<'h>| vars.iter().any(|&x| !is_free(b, t, x)) || !super::spec::params(b, &[t], vars).is_empty();
    if !seen.insert(u) || !depends(u) {
        return Ok(true);
    }
    let dv = |t: T<'h>| vars.iter().any(|&x| !is_free(b, t, x));
    let View::App { head, args } = h.view(u) else { return Ok(true) };
    let Some(s) = b.sym_of(head) else {
        // a compound head free of the variables (`log(z^3)(y)`) is a generic function of its arguments
        if dv(head) {
            return Ok(false);
        }
        for &a in args {
            if !walk(cx, b, a, vars, complex, mode, facts, out, seen, partial)? {
                return Ok(false);
            }
        }
        return Ok(true);
    };
    if h.sym_name(s) == "piecewise" {
        return piecewise_atoms(cx, b, u, args, vars, complex, mode, facts, out);
    }
    for &a in args {
        if !walk(cx, b, a, vars, complex, mode, facts, out, seen, partial)? {
            return Ok(false);
        }
    }
    let name = h.sym_name(s).to_string();
    let zero = b.zero();
    let one = b.one();
    let pi = h.sym_term(h.sym("pi"));
    let imu = h.sym_term(h.sym("im"));
    let rel = |n: &str, l: T<'h>, r: T<'h>| h.call(h.sym(n), &[l, r]);
    let and = |a: T<'h>, c: T<'h>| h.call(h.sym("and"), &[a, c]);
    let mut push = |t: T<'h>, kind: Kind| out.push(Atom { t, kind });
    // the branch point and cut of the principal logarithm and root at u
    let log_cut = |u: T<'h>| and(rel("equal", part("im_part", b, u), zero), rel("less_equal", part("re", b, u), zero));
    let gamma_pole = |u: T<'h>| {
        let side = if complex { part("re", b, u) } else { u };
        and(rel("equal", b.call("sin", &[b.mul(&[pi, u])]), zero), rel("less_equal", side, zero))
    };
    if s == b.hd.pow {
        let [base, ex] = args else { return Ok(false) };
        // c^u over the reals with c (a number or a parameter) proved negative: real only at integer u
        // (exp(u log|c|) e^(i pi u)), a set without interior: singular and discontinuous everywhere
        if !complex && !dv(*base) && dv(*ex) {
            let c = cx.eval(*base)?;
            let negative = match b.ar.compare(c, zero) {
                Some(cmp) => cmp == mc_arith::Cmp3::Less,
                None if b.ar.num(c).is_some() => false,
                None => {
                    let r = cx.eval_assuming(facts, b.call("simplify", &[rel("less", c, zero)]))?;
                    cx.is_true(r)
                }
            };
            if negative {
                push(h.sym_term(h.sym("true")), Kind::Branch);
                return Ok(true);
            }
        }
        if !depends(*base) {
            // c^u: entire for a numeric nonzero c (e included)
            let c = cx.eval(*base)?;
            let ok = c == h.sym_term(b.hd.e) || b.ar.num(c).is_some_and(|_| c != zero) || matches!(h.view(c), View::Sym(_)) && is_constant(h, c);
            return Ok(ok);
        }
        match h.view(*ex) {
            View::Int(k) if k < 0 => push(rel("equal", *base, zero), Kind::Pole),
            View::Int(_) | View::BigInt { .. } => {}
            _ => {
                let integer = !dv(*ex) && {
                    let r = cx.eval_assuming(facts, h.call(h.sym("refine"), &[h.call(h.sym("element"), &[*ex, h.sym_term(h.sym("integers"))])]))?;
                    cx.is_true(r)
                };
                if integer {
                    let r = cx.eval_assuming(facts, h.call(h.sym("refine"), &[rel("greater_equal", *ex, zero)]))?;
                    if !cx.is_true(r) {
                        push(rel("equal", *base, zero), Kind::Pole);
                    }
                } else if !dv(*ex) && b.ar.num(*ex).is_none() && !super::spec::params(b, &[*ex], vars).is_empty() {
                    // a symbolic exponent not known to be an integer: not decided here
                    return Ok(false);
                } else if complex {
                    push(rel("equal", *base, zero), Kind::Branch);
                    push(log_cut(*base), Kind::Branch);
                } else if positive_exponent(b, *ex) && nonneg(b, *base) {
                    // a positive power of a quantity that is never negative: continuous at its zeros; not
                    // analytic at a zero of order m unless m ex is an even integer (`(x^2)^(1/2)` is |x|, `(x^4)^(1/2)`
                    // is x^2)
                    if mode == Mode::Sing && !root_zeros(cx, b, *base, *ex, vars, &mut push)? {
                        return Ok(false);
                    }
                } else if let Some(a) = local_minimum_zeros(cx, b, *base, *ex, vars, mode)? {
                    push(a, Kind::Branch);
                } else {
                    push(rel("less_equal", *base, zero), Kind::Branch);
                }
            }
        }
        return Ok(true);
    }
    if s == b.hd.add || s == b.hd.mul || s == b.hd.list {
        return Ok(true);
    }
    let a0 = args.first().copied();
    let one_arg = args.len() == 1;
    match (name.as_str(), complex) {
        (n, _) if ENTIRE.contains(&n) => {}
        // a polynomial family of an integer degree (a non-integer degree has the branch point -1: the table)
        (n, _) if ENTIRE_LAST.contains(&n) && (n == "hypergeometric_0f1" || matches!(h.view(args[0]), View::Int(_))) => {
            if args[..args.len() - 1].iter().any(|&p| depends(p)) {
                return Ok(false);
            }
            // 0F1(b; z) at a pole b = 0, -1, ...: defined nowhere (function_analytic(hypergeometric_0f1(-1, z), z) was true)
            if n == "hypergeometric_0f1" && args.len() == 2 && matches!(h.view(args[0]), View::Int(k) if k <= 0) {
                push(cx.bool(true), Kind::Pole);
            }
        }
        (n, _) if INTEGER_ORDER.contains(&n) && args.len() == 2 => {
            let integer = matches!(h.view(args[0]), View::Int(_));
            // j_n of a negative integer order is (-1)^n y_(-n-1): a pole at 0 (DLMF 10.47.10)
            if n == "spherical_bessel_j" && matches!(h.view(args[0]), View::Int(k) if k < 0) {
                push(rel("equal", args[1], zero), Kind::Pole);
            }
            if !integer {
                if depends(args[0]) {
                    return Ok(false);
                }
                let z = args[1];
                if complex {
                    push(rel("equal", z, zero), Kind::Branch);
                    push(log_cut(z), Kind::Branch);
                } else {
                    push(rel("less_equal", z, zero), Kind::Branch);
                }
            }
        }
        ("log", _) if one_arg => {
            let u = a0.unwrap();
            if complex {
                push(rel("equal", u, zero), Kind::Branch);
                push(log_cut(u), Kind::Branch);
            } else {
                push(rel("less_equal", u, zero), Kind::Branch);
            }
        }
        ("tan" | "sec", _) if one_arg => push(rel("equal", b.call("cos", &[a0.unwrap()]), zero), Kind::Pole),
        ("cot" | "csc", _) if one_arg => push(rel("equal", b.call("sin", &[a0.unwrap()]), zero), Kind::Pole),
        ("tanh" | "sech", true) if one_arg => push(rel("equal", b.call("cosh", &[a0.unwrap()]), zero), Kind::Pole),
        ("coth" | "csch", true) if one_arg => push(rel("equal", b.call("sinh", &[a0.unwrap()]), zero), Kind::Pole),
        ("tanh" | "sech" | "atan" | "asinh" | "gudermannian", false) => {}
        ("coth" | "csch", false) if one_arg => push(rel("equal", a0.unwrap(), zero), Kind::Pole),
        ("gudermannian", true) if one_arg => push(rel("equal", b.call("cosh", &[a0.unwrap()]), zero), Kind::Branch),
        ("logistic_sigmoid", false) => {}
        ("logistic_sigmoid", true) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", b.add(&[one, b.call("exp", &[b.neg(u)])]), zero), Kind::Pole)
        }
        ("asin" | "acos" | "atanh", false) if one_arg => {
            let u = a0.unwrap();
            push(rel("less_equal", u, b.neg(one)), Kind::Branch);
            push(rel("greater_equal", u, one), Kind::Branch);
        }
        ("acosh", false) if one_arg => push(rel("less_equal", a0.unwrap(), one), Kind::Branch),
        ("acot", false) if one_arg => push(rel("equal", a0.unwrap(), zero), Kind::Jump),
        // the reciprocal inverse functions on the real line: defined for `|u| > 1` (`acoth`), `|u| >= 1`
        // (`acsc`, `asec`), `0 < u <= 1` (`asech`), `u != 0` (`acsch`)
        ("acsch", false) if one_arg => push(rel("equal", a0.unwrap(), zero), Kind::Pole),
        ("acoth", false) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", u, one), Kind::Pole);
            push(rel("equal", u, b.neg(one)), Kind::Pole);
            push(and(rel("greater", u, b.neg(one)), rel("less", u, one)), Kind::Branch);
        }
        ("acsc" | "asec", false) if one_arg => {
            let u = a0.unwrap();
            push(and(rel("greater", u, b.neg(one)), rel("less", u, one)), Kind::Branch);
        }
        ("asech", false) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", u, zero), Kind::Pole);
            push(rel("less", u, zero), Kind::Branch);
            push(rel("greater", u, one), Kind::Branch);
        }
        ("asin" | "acos" | "atanh" | "acosh", true) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", u, b.neg(one)), Kind::Branch);
            push(rel("equal", u, one), Kind::Branch);
            let (re, im) = (part("re", b, u), part("im_part", b, u));
            if name == "acosh" {
                push(and(rel("equal", im, zero), rel("less_equal", re, one)), Kind::Branch);
            } else {
                push(and(rel("equal", im, zero), rel("greater_equal", re, one)), Kind::Branch);
                push(and(rel("equal", im, zero), rel("less_equal", re, b.neg(one))), Kind::Branch);
            }
        }
        ("atan" | "asinh", true) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", b.sub(u, imu), zero), Kind::Branch);
            push(rel("equal", b.add(&[u, imu]), zero), Kind::Branch);
            let (re, im) = (part("re", b, u), part("im_part", b, u));
            push(and(rel("equal", re, zero), rel("greater_equal", im, one)), Kind::Branch);
            push(and(rel("equal", re, zero), rel("less_equal", im, b.neg(one))), Kind::Branch);
        }
        ("acot" | "acoth" | "asec" | "acsc" | "asech" | "acsch", true) if one_arg => {
            // branch points only (their cuts are not listed): enough to deny meromorphy, not to list the cuts
            if mode == Mode::Disc {
                return Ok(false);
            }
            let u = a0.unwrap();
            let pts: &[T<'h>] = match name.as_str() {
                "acot" => &[imu, b.neg(imu)],
                "acoth" => &[one, b.neg(one)],
                "asec" | "acsc" | "asech" => &[zero, one, b.neg(one)],
                _ => &[zero, imu, b.neg(imu)],
            };
            for &p in pts {
                push(rel("equal", u, p), Kind::Branch);
            }
        }
        ("gamma", _) if one_arg => push(gamma_pole(a0.unwrap()), Kind::Pole),
        ("factorial", _) if one_arg => push(gamma_pole(b.add(&[a0.unwrap(), one])), Kind::Pole),
        ("digamma", _) if one_arg => push(gamma_pole(a0.unwrap()), Kind::Pole),
        ("polygamma", _) if args.len() == 2 && !depends(args[0]) => push(gamma_pole(args[1]), Kind::Pole),
        ("log_gamma", false) if one_arg => push(rel("less_equal", a0.unwrap(), zero), Kind::Branch),
        ("beta", _) if args.len() == 2 => {
            for &a in args {
                if depends(a) {
                    push(gamma_pole(a), Kind::Pole);
                }
            }
        }
        ("pochhammer", _) if args.len() == 2 => {
            let s = b.add(&[args[0], args[1]]);
            push(gamma_pole(s), Kind::Pole);
        }
        ("gamma", _) if args.len() == 2 => {
            // the upper incomplete gamma function: entire in the order, a branch at 0 in the argument unless the order
            // is a positive integer n (then (n - 1)! e^(-z) times a polynomial, entire; DLMF 8.4.8)
            let (a, z) = (args[0], args[1]);
            let positive_integer = |t: T<'h>| matches!(h.view(t), View::Int(k) if k > 0);
            if depends(z) && !positive_integer(a) {
                let order_cond = if vars.iter().any(|&x| !is_free(b, a, x)) || b.ar.num(a).is_some() {
                    None
                } else {
                    Some(b.call("not", &[b.call("and", &[b.call("element", &[a, h.sym_term(h.sym("integers"))]), rel("greater", a, zero)])]))
                };
                let with = |c: T<'h>| match order_cond {
                    Some(o) => b.call("and", &[o, c]),
                    None => c,
                };
                if complex {
                    push(with(rel("equal", z, zero)), Kind::Branch);
                    push(with(log_cut(z)), Kind::Branch);
                } else {
                    push(with(rel("less_equal", z, zero)), Kind::Branch);
                }
            }
        }
        ("zeta", _) if one_arg => push(rel("equal", a0.unwrap(), one), Kind::Pole),
        // step functions
        ("floor" | "ceiling" | "integer_part" | "fractional_part", false) if one_arg => {
            push(rel("equal", b.call("sin", &[b.mul(&[pi, a0.unwrap()])]), zero), Kind::Jump)
        }
        ("round", false) if one_arg => {
            push(rel("equal", b.call("sin", &[b.mul(&[pi, b.add(&[b.rat(1, 2), a0.unwrap()])])]), zero), Kind::Jump)
        }
        ("mod" | "quotient", false) if args.len() == 2 && !depends(args[1]) => {
            push(rel("equal", b.call("sin", &[b.div(b.mul(&[pi, args[0]]), args[1])]), zero), Kind::Jump)
        }
        ("square_wave", false) if one_arg => {
            push(rel("equal", b.call("sin", &[b.mul(&[b.int(2), pi, a0.unwrap()])]), zero), Kind::Jump)
        }
        ("sawtooth_wave", false) if one_arg => push(rel("equal", b.call("sin", &[b.mul(&[pi, a0.unwrap()])]), zero), Kind::Jump),
        ("triangle_wave", false) if one_arg => {
            if mode == Mode::Sing {
                push(rel("equal", b.call("cos", &[b.mul(&[b.int(2), pi, a0.unwrap()])]), zero), Kind::Kink)
            }
        }
        ("unit_step" | "sign" | "heaviside_theta" | "real_sign", false) if one_arg => push(rel("equal", a0.unwrap(), zero), Kind::Jump),
        ("unit_step" | "heaviside_theta", false) => {
            for &a in args {
                if depends(a) {
                    push(rel("equal", a, zero), Kind::Jump);
                }
            }
        }
        ("arg", false) if one_arg => push(rel("equal", a0.unwrap(), zero), Kind::Jump),
        ("unit_box", false) if one_arg => {
            let u = a0.unwrap();
            push(rel("equal", u, b.rat(-1, 2)), Kind::Jump);
            push(rel("equal", u, b.rat(1, 2)), Kind::Jump);
        }
        // kinks
        ("abs" | "ramp" | "real_abs", false) if one_arg => {
            if mode == Mode::Sing {
                // a zero of even order k is no kink: |u| = +-u and ramp(u) = u or 0 near it (`abs(x^2)` is x^2)
                let u = a0.unwrap();
                match odd_order_zeros(cx, b, u, vars)? {
                    Some(a) => push(a, Kind::Kink),
                    None => push(rel("equal", u, zero), Kind::Kink),
                }
            }
        }
        ("max" | "min", false) => {
            let items: Vec<T<'h>> = match args {
                [l] => match cx.as_list(*l) {
                    Some(v) => v.to_vec(),
                    None => vec![*l],
                },
                _ => args.to_vec(),
            };
            if mode == Mode::Sing {
                for i in 0..items.len() {
                    for j in i + 1..items.len() {
                        if depends(items[i]) || depends(items[j]) {
                            push(rel("equal", b.sub(items[i], items[j]), zero), Kind::Kink);
                        }
                    }
                }
            }
        }
        ("clip", false) => {
            let u = args[0];
            let (lo, hi) = match args.get(1).and_then(|&r| cx.as_list(r)) {
                Some([l, r]) => (*l, *r),
                _ => (b.neg(one), one),
            };
            // clip(u, [lo, hi], [vlo, vhi]): a jump where a value differs from its bound (a discontinuity), a kink
            // where it is the bound
            let (vlo, vhi) = match args.get(2).and_then(|&r| cx.as_list(r)) {
                Some([l, r]) => (*l, *r),
                _ => (lo, hi),
            };
            for (at, v) in [(lo, vlo), (hi, vhi)] {
                let numbers = b.ar.num(v).is_some() && b.ar.num(at).is_some();
                if v != at && !numbers && mode == Mode::Disc {
                    // symbolic: a jump only where the value differs from the bound
                    push(b.call("and", &[rel("equal", u, at), rel("unequal", v, at)]), Kind::Jump);
                } else if v != at {
                    push(rel("equal", u, at), Kind::Jump);
                } else if mode == Mode::Sing {
                    push(rel("equal", u, at), Kind::Kink);
                }
            }
        }
        ("unit_triangle", false) if one_arg => {
            if mode == Mode::Sing {
                let u = a0.unwrap();
                for k in [-1, 0, 1] {
                    push(rel("equal", u, b.int(k)), Kind::Kink);
                }
            }
        }
        ("re" | "im_part" | "conjugate", false) => {}
        // over the complexes: continuous everywhere, analytic nowhere
        ("abs" | "re" | "im_part" | "conjugate" | "real_abs" | "triangle_wave", true) => {
            if mode == Mode::Sing {
                push(cx.bool(true), Kind::Kink)
            }
        }
        ("sign", true) if one_arg => {
            push(rel("equal", a0.unwrap(), zero), Kind::Jump);
            if mode == Mode::Sing {
                push(cx.bool(true), Kind::Kink)
            }
        }
        ("arg", true) if one_arg => {
            push(rel("equal", a0.unwrap(), zero), Kind::Jump);
            push(log_cut(a0.unwrap()), Kind::Jump);
            if mode == Mode::Sing {
                push(cx.bool(true), Kind::Kink)
            }
        }
        ("unit_step" | "heaviside_theta" | "square_wave" | "sawtooth_wave" | "floor" | "ceiling" | "round" | "integer_part" | "fractional_part" | "max" | "min" | "clip" | "ramp" | "unit_box" | "unit_triangle", true) => {
            // nowhere analytic over the complexes, discontinuous along lines: decided as `true` both ways
            push(cx.bool(true), Kind::Jump)
        }
        _ => {
            // the special functions of the table
            if let Some(hit) = super::table::lookup(cx, b, u, vars, facts)? {
                let Some((found, part)) = hit.atoms(cx, b, complex, mode)? else { return Ok(false) };
                *partial |= part;
                for (t, kind) in found {
                    push(t, kind);
                }
                return Ok(true);
            }
            // an undefined head is a generic (analytic) function
            if !cx.is_defined(s) {
                return Ok(true);
            }
            return Ok(false);
        }
    }
    Ok(true)
}

/// Is `t` a real numeric quantity built of numbers and constants (`-1/e`, `2^(1/2)/2`)?
fn numeric_quantity<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    numeric_quantity_in(h, t, &mut mc_api::walk::memo())
}

fn numeric_quantity_in<'h>(h: &'h mc_core::Heap, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, t, |memo| {
        match h.view(t) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) => true,
            View::Sym(_) => is_constant(h, t),
            View::App { head, args } => {
                matches!(h.view(head), View::Sym(s) if matches!(h.sym_name(s).as_str(), "add" | "mul" | "pow"))
                    && args.iter().all(|&a| numeric_quantity_in(h, a, memo))
            }
            _ => false,
        }
    })
}

/// Is the symbol `t` a constant (`pi`, `euler_gamma`, ...)?
fn is_constant<'h>(h: &'h mc_core::Heap, t: T<'h>) -> bool {
    matches!(h.view(t), View::Sym(s) if matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "catalan" | "golden_ratio" | "degree" | "glaisher"))
}

/// `re(u)` or `im_part(u)` taken through real constant terms and factors: `re(1 + z^2)` is `1 + re(z^2)`.
pub fn part<'h>(name: &str, b: &Build<'h>, u: T<'h>) -> T<'h> {
    let h = b.heap();
    let real_num = |t: T<'h>| real_rational(b, t);
    if let View::App { head, args } = h.view(u) {
        if b.sym_of(head) == Some(b.hd.add) {
            let (nums, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&t| real_num(t));
            if !nums.is_empty() && !rest.is_empty() {
                let r = b.call(name, &[b.add(&rest)]);
                return if name == "re" {
                    let mut v = nums;
                    v.push(r);
                    b.add(&v)
                } else {
                    r
                };
            }
        }
        if b.sym_of(head) == Some(b.hd.mul) {
            let (nums, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&t| real_num(t));
            if !nums.is_empty() && !rest.is_empty() {
                let mut v = nums;
                v.push(b.call(name, &[b.mul(&rest)]));
                return b.mul(&v);
            }
        }
    }
    b.call(name, &[u])
}

/// Evaluate `t` and tidy every relation in it (see [`tidy_rel`]).
pub fn tidy_all<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let t = cx.eval(t)?;
    match h.view(t) {
        View::App { head, args } if matches!(b.sym_of(head).map(|s| h.sym_name(s)).as_deref(), Some("and" | "or")) => {
            let mut v = Vec::with_capacity(args.len());
            for &a in args {
                v.push(tidy_rel(cx, b, a)?);
            }
            cx.eval(h.app(head, &v))
        }
        _ => tidy_rel(cx, b, t),
    }
}

/// `l op 0` with real rational terms in `l`: those terms moved to the right (`-1 + x^2 == 0` is `x^2 == 1`); not
/// for `!=`, nor for complex constants (`-im + z^2 == 0` stays).
pub fn tidy_rel<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args: &[l, r] } = h.view(c) else { return Ok(c) };
    let Some(s) = b.sym_of(head) else { return Ok(c) };
    if !matches!(h.sym_name(s).as_str(), "equal" | "greater" | "greater_equal" | "less" | "less_equal") || r != b.zero() {
        return Ok(c);
    }
    // p^k == 0 for a positive integer k is p == 0
    if h.sym_name(s) == "equal" {
        if let View::App { head: ph, args: &[base, ex] } = h.view(l) {
            if b.sym_of(ph) == Some(b.hd.pow) && matches!(h.view(ex), View::Int(k) if k > 0) {
                return tidy_rel(cx, b, h.call(s, &[base, r]));
            }
        }
    }
    let View::App { head: ah, args: terms } = h.view(l) else { return Ok(c) };
    if b.sym_of(ah) != Some(b.hd.add) {
        return Ok(c);
    }
    let (num, rest): (Vec<T<'h>>, Vec<T<'h>>) = terms.iter().partition(|&&t| real_rational(b, t));
    if num.is_empty() || rest.is_empty() {
        return Ok(c);
    }
    let k = b.neg(b.add(&num));
    cx.eval(h.call(s, &[b.add(&rest), k]))
}

/// An integer or a rational number.
pub fn real_rational<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    matches!(b.heap().view(t), View::Int(_) | View::BigInt { .. } | View::Rational { .. })
}

/// A key of a number that sorts as the number.
fn num_key(x: f64) -> String {
    format!("#{:030.9}", x + 1e15)
}

/// Is `t` a call of `name`?
pub fn is_head<'h>(b: &Build<'h>, t: T<'h>, name: &str) -> bool {
    let h = b.heap();
    matches!(h.view(t), View::App { head, .. } if b.sym_of(head) == Some(h.sym(name)))
}

/// The order key of a term (decision R25P.3): numbers first, then constants, then symbols and their powers,
/// products and sums (a sum or product by its largest part, a power by its base then exponent), then calls by
/// head name and arguments.
fn order_key<'h>(b: &Build<'h>, t: T<'h>) -> String {
    // shared parts keyed once
    order_key_in(b, t, &mut mc_api::walk::memo())
}

fn order_key_in<'h>(b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, String>) -> String {
    mc_api::walk::cached(memo, t, |memo| order_key_node(b, t, memo))
}

fn order_key_node<'h>(b: &Build<'h>, t: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, String>) -> String {
    let h = b.heap();
    match h.view(t) {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) => {
            let mut k = "0".to_string();
            key(b, t, &mut k);
            k
        }
        View::Sym(s) => {
            let n = h.sym_name(s);
            if is_constant(h, t) {
                format!("1{n}")
            } else {
                format!("2{n}")
            }
        }
        View::App { head, args } => {
            let s = b.sym_of(head);
            if s == Some(b.hd.add) || s == Some(b.hd.mul) {
                let mut ks: Vec<String> = args.iter().filter(|&&a| b.ar.num(a).is_none()).map(|&a| order_key_in(b, a, memo)).collect();
                ks.sort();
                ks.pop().unwrap_or_else(|| "0".to_string())
            } else if s == Some(b.hd.pow) {
                format!("{}^{}", order_key_in(b, args[0], memo), order_key_in(b, args[1], memo))
            } else {
                let mut k = "3".to_string();
                key(b, t, &mut k);
                k
            }
        }
        _ => "9".to_string(),
    }
}

/// The order key of a condition: its left side, then its full key.
fn cond_key<'h>(b: &Build<'h>, t: T<'h>) -> String {
    let h = b.heap();
    let mut full = String::new();
    key(b, t, &mut full);
    match h.view(t) {
        View::App { head, args: &[l, _] } if b.sym_of(head).is_some_and(|s| matches!(h.sym_name(s).as_str(), "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal")) => {
            format!("{}\u{1}{}", order_key(b, l), full)
        }
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) && !args.is_empty() => format!("{}\u{1}{}", cond_key(b, args[0]), full),
        _ => full,
    }
}

/// The most bytes of an order key: a condition with shared parts is a tree exponentially larger than itself, and
/// keys that agree on this many bytes keep their order (the sorts are stable).
const KEY_LIMIT: usize = 1 << 16;

/// A key for the canonical order of conditions: heads by name, then arguments, numbers as numbers (at most
/// [`KEY_LIMIT`] bytes).
fn key<'h>(b: &Build<'h>, t: T<'h>, out: &mut String) {
    if out.len() > KEY_LIMIT {
        return;
    }
    let h = b.heap();
    match h.view(t) {
        View::Int(k) => out.push_str(&num_key(k as f64)),
        View::Rational { num, den } => {
            let (n, d) = (match h.view(num) { View::Int(k) => k, _ => 0 }, match h.view(den) { View::Int(k) => k, _ => 1 });
            out.push_str(&num_key(n as f64 / d as f64));
        }
        View::Real(x) => out.push_str(&num_key(x)),
        View::Sym(s) => {
            let n = h.sym_name(s);
            if n == "im" {
                out.push_str(&format!("#complex(0,{})", num_key(1.0)));
            } else {
                out.push_str(&n);
            }
        }
        View::App { head, args } => {
            // k*im: a complex number
            if b.sym_of(head) == Some(b.hd.mul) && args.len() == 2 && args[1] == h.sym_term(h.sym("im")) {
                if let View::Int(k) = h.view(args[0]) {
                    out.push_str(&format!("#complex(0,{k:+})"));
                    return;
                }
            }
            let name = match b.sym_of(head) {
                Some(s) => match h.sym_name(s).as_str() {
                    "add" => "plus".to_string(),
                    "mul" => "times".to_string(),
                    "pow" => "power".to_string(),
                    "im_part" => "im".to_string(),
                    n => n.replace('_', ""),
                },
                None => "?".to_string(),
            };
            out.push_str(&name);
            out.push('[');
            for (i, &a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                key(b, a, out);
            }
            out.push(']');
        }
        _ => out.push('~'),
    }
}

/// The disjunction of the atoms in the order of the module comment.
pub fn disjunction<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, atoms: &[T<'h>], vars: &[T<'h>], complex: bool) -> Res<T<'h>> {
    let h = b.heap();
    if atoms.iter().any(|&a| cx.is_true(a)) {
        return Ok(cx.bool(true));
    }
    let mut items: Vec<(bool, String, T<'h>)> = atoms
        .iter()
        .map(|&a| {
            (is_head(b, a, "and"), cond_key(b, a), a)
        })
        .collect();
    items.sort_by(|a, c| a.0.cmp(&c.0).then_with(|| a.1.cmp(&c.1)));
    let sorted: Vec<T<'h>> = items.into_iter().map(|i| i.2).collect();
    let d = match sorted.as_slice() {
        [] => return Ok(cx.bool(false)),
        [a] => *a,
        _ => h.call(h.sym("or"), &sorted),
    };
    if !complex && vars.len() == 1 && sorted.len() > 1 && sorted.iter().all(|&a| linear_atom(b, a, vars[0])) {
        let r = cx.eval(h.call(h.sym("reduce"), &[d, vars[0], h.sym_term(h.sym("reals"))]))?;
        if !is_head(b, r, "reduce") && !is_head(b, r, names::FAILURE) {
            return Ok(r);
        }
    }
    cx.eval(d)
}

/// A relation between expressions at most linear in `x` with rational coefficients.
fn linear_atom<'h>(b: &Build<'h>, a: T<'h>, x: T<'h>) -> bool {
    let h = b.heap();
    let View::App { head, args } = h.view(a) else { return false };
    let Some(s) = b.sym_of(head) else { return false };
    if !matches!(h.sym_name(s).as_str(), "equal" | "less" | "less_equal" | "greater" | "greater_equal" | "unequal") {
        return false;
    }
    args.iter().all(|&t| {
        poly_degree(b, t, &[x]).is_some_and(|d| d <= 1)
    })
}

/// The total degree of `t` as a polynomial in `syms` with rational coefficients (other symbols count as
/// coefficients only if `syms` is empty... no: any other symbol is a coefficient), or `None`.
pub fn poly_degree<'h>(b: &Build<'h>, t: T<'h>, syms: &[T<'h>]) -> Option<u32> {
    // shared subterms measured once (the degree saturates)
    poly_degree_in(b, t, syms, &mut mc_api::walk::memo())
}

fn poly_degree_in<'h>(b: &Build<'h>, t: T<'h>, syms: &[T<'h>], memo: &mut mc_api::walk::Memo<T<'h>, Option<u32>>) -> Option<u32> {
    mc_api::walk::cached(memo, t, |memo| poly_degree_node(b, t, syms, memo))
}

fn poly_degree_node<'h>(b: &Build<'h>, t: T<'h>, syms: &[T<'h>], memo: &mut mc_api::walk::Memo<T<'h>, Option<u32>>) -> Option<u32> {
    let h = b.heap();
    if syms.contains(&t) {
        return Some(1);
    }
    match h.view(t) {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } => Some(0),
        View::Sym(_) => {
            if is_constant(h, t) || t == h.sym_term(h.sym("im")) {
                None
            } else {
                Some(0)
            }
        }
        View::App { head, args } => {
            let s = b.sym_of(head)?;
            if s == b.hd.add {
                args.iter().map(|&a| poly_degree_in(b, a, syms, memo)).try_fold(0, |m, d| d.map(|d| m.max(d)))
            } else if s == b.hd.mul {
                args.iter().map(|&a| poly_degree_in(b, a, syms, memo)).try_fold(0u32, |m, d| d.map(|d| m.saturating_add(d)))
            } else if s == b.hd.pow {
                let [base, ex] = args else { return None };
                match h.view(*ex) {
                    View::Int(k) if k >= 0 => poly_degree_in(b, *base, syms, memo).map(|d| d.saturating_mul(u32::try_from(k).unwrap_or(u32::MAX))),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is every relation in `t` (through `and`, `or`, `not`) between polynomials in `syms` and the parameters?
pub fn polynomial_formula<'h>(b: &Build<'h>, t: T<'h>, syms: &[T<'h>]) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    polynomial_formula_in(b, t, syms, &mut mc_api::walk::memo())
}

fn polynomial_formula_in<'h>(b: &Build<'h>, t: T<'h>, syms: &[T<'h>], memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, t, |memo| {
        let h = b.heap();
        match h.view(t) {
            View::Sym(s) => matches!(h.sym_name(s).as_str(), "true" | "false"),
            View::App { head, args } => {
                let Some(s) = b.sym_of(head) else { return false };
                match h.sym_name(s).as_str() {
                    "and" | "or" | "not" | "implies" => args.iter().all(|&a| polynomial_formula_in(b, a, syms, memo)),
                    "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal" => {
                        args.iter().all(|&a| poly_degree(b, a, syms).is_some())
                    }
                    "element" => args.len() == 2 && args[1] == h.sym_term(h.sym("reals")),
                    _ => false,
                }
            }
            _ => false,
        }
    })
}

/// Does the set meet the region?
#[derive(Clone, Copy, Debug)]
pub enum Meet<'h> {
    /// it does (a witness or a proof)
    Yes,
    /// it does not
    No,
    /// it does not when the condition on the parameters holds
    NoWhen(T<'h>),
    /// not decided
    Unknown,
}

/// Is `e` a positive rational number?
/// The zeros of a base `u >= 0` where `u^ex` (`ex` a positive rational, not an integer) is not real-analytic: near a
/// zero of order m, u = c t^m (1 + O(t)) with c > 0 and m even, so u^ex = c^ex |t|^(m ex) (1 + ...), analytic exactly
/// when m ex is an even integer. A polynomial in one variable by its square-free factors (each factor's zeros have
/// the order of its index); `w^k` as `w` with the exponent `k ex`; a base without real zeros (exp, cosh, a positive
/// constant plus terms >= 0) has none. `false` when the zeros are not known.
fn root_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, ex: T<'h>, vars: &[T<'h>], push: &mut dyn FnMut(T<'h>, Kind)) -> Res<bool> {
    let h = b.heap();
    let even_int = |cx: &mut NativeCx<'_, '_, 'h>, r: T<'h>| -> Res<bool> {
        let r = cx.eval(r)?;
        Ok(matches!(h.view(r), View::Int(k) if k % 2 == 0))
    };
    if no_real_zero(b, u) {
        return Ok(true);
    }
    if let View::App { head, args: &[w, k] } = h.view(u) {
        if b.sym_of(head) == Some(b.hd.pow) && matches!(h.view(k), View::Int(_)) {
            let e2 = cx.eval(b.mul(&[k, ex]))?;
            if even_int(cx, e2)? {
                return Ok(true);
            }
            return root_zeros(cx, b, w, e2, vars, push);
        }
    }
    let [x] = vars else { return Ok(false) };
    if poly_degree(b, u, &[*x]).is_none() || !super::spec::params(b, &[u], vars).is_empty() {
        return Ok(false);
    }
    let l = cx.eval(b.call("factor_square_free_list", &[u]))?;
    let Some(fs) = cx.as_list(l).map(|v| v.to_vec()) else { return Ok(false) };
    for f in fs {
        let Some(&[g, i]) = cx.as_list(f) else { return Ok(false) };
        if is_free(b, g, *x) {
            continue;
        }
        let r = b.mul(&[i, ex]);
        if !even_int(cx, r)? {
            push(h.call(h.sym("equal"), &[g, b.zero()]), Kind::Branch);
        }
    }
    Ok(true)
}

/// The real points where `u^ex` (`ex` not an integer) is not continuous (`Disc`) or not analytic (`Sing`), for a
/// polynomial `u` of degree at most 8 in one variable (parameters allowed): `u <= 0` without the zeros where u is at
/// least 0 nearby, those whose first nonzero derivative u^(k) has an even order k and is positive (a local minimum 0:
/// `sqrt(x^2 + a)` is continuous for a >= 0, at a = 0 too), and for `Sing` only those where k ex is an even integer
/// (u^ex = c |t|^(k ex) (1 + ...) there). `None` for other bases (the caller takes `u <= 0`).
fn local_minimum_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, ex: T<'h>, vars: &[T<'h>], mode: Mode) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = vars else { return Ok(None) };
    let Some(n) = poly_degree(b, u, &[*x]) else { return Ok(None) };
    if !(2..=8).contains(&n) || b.ar.num(ex).is_none() {
        return Ok(None);
    }
    let mut ds = vec![u];
    for _ in 0..n {
        let last = *ds.last().unwrap_or(&u);
        ds.push(cx.eval(b.call("d", &[last, *x]))?);
    }
    let mut keep = vec![];
    for k in (2..=n as usize).step_by(2) {
        if mode == Mode::Sing {
            let r = cx.eval(b.mul(&[b.int(k as i64), ex]))?;
            if !matches!(h.view(r), View::Int(m) if m % 2 == 0) {
                continue;
            }
        }
        let mut c: Vec<T<'h>> = ds[..k].iter().map(|&d| h.call(h.sym("equal"), &[d, b.zero()])).collect();
        c.push(h.call(h.sym("greater"), &[ds[k], b.zero()]));
        keep.push(h.call(h.sym("and"), &c));
    }
    let le = h.call(h.sym("less_equal"), &[u, b.zero()]);
    if keep.is_empty() {
        return Ok(Some(le));
    }
    without_points(cx, b, le, &keep, u, *x, vars).map(Some)
}

/// The zeros of odd order of a polynomial `u` of degree 2 to 8 in one variable (parameters allowed): `u == 0` without
/// the zeros whose first nonzero derivative has an even order. `None` for other terms.
fn odd_order_zeros<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, u: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let [x] = vars else { return Ok(None) };
    let Some(n) = poly_degree(b, u, &[*x]) else { return Ok(None) };
    if !(2..=8).contains(&n) {
        return Ok(None);
    }
    let mut ds = vec![u];
    for _ in 0..n {
        let last = *ds.last().unwrap_or(&u);
        ds.push(cx.eval(b.call("d", &[last, *x]))?);
    }
    let keep: Vec<T<'h>> = (2..=n as usize)
        .step_by(2)
        .map(|k| {
            let mut c: Vec<T<'h>> = ds[..k].iter().map(|&d| h.call(h.sym("equal"), &[d, b.zero()])).collect();
            c.push(h.call(h.sym("unequal"), &[ds[k], b.zero()]));
            h.call(h.sym("and"), &c)
        })
        .collect();
    let eq = h.call(h.sym("equal"), &[u, b.zero()]);
    without_points(cx, b, eq, &keep, u, *x, vars).map(Some)
}

/// `set and not (k_1 or k_2 ...)`; `set` itself when no point is left out (decided by `reduce` without parameters, so
/// that the printed set is the one it was), the reduced form when some is.
fn without_points<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, set: T<'h>, keep: &[T<'h>], u: T<'h>, x: T<'h>, vars: &[T<'h>]) -> Res<T<'h>> {
    let h = b.heap();
    let reals = h.sym_term(h.sym("reals"));
    let any = cx.eval(if keep.len() == 1 { keep[0] } else { h.call(h.sym("or"), keep) })?;
    let plain = super::spec::params(b, &[u], vars).is_empty();
    let any = if plain && !cx.is_false(any) { cx.eval(h.call(h.sym("reduce"), &[any, x, reals]))? } else { any };
    if cx.is_false(any) {
        return Ok(set);
    }
    let r = cx.eval(h.call(h.sym("and"), &[set, h.call(h.sym("not"), &[any])]))?;
    if plain {
        let rr = cx.eval(h.call(h.sym("reduce"), &[r, x, reals]))?;
        if !is_head(b, rr, "reduce") && !is_head(b, rr, mc_api::names::FAILURE) {
            return Ok(rr);
        }
    }
    Ok(r)
}

/// Is `u` positive for every real value of the variables by its structure (exp, cosh, a positive number, sums with
/// a positive term and terms >= 0, products of such)?
fn no_real_zero<'h>(b: &Build<'h>, u: T<'h>) -> bool {
    let h = b.heap();
    match h.view(u) {
        View::Int(k) => k > 0,
        View::Rational { .. } => b.ar.compare(u, b.zero()) == Some(mc_arith::Cmp3::Greater),
        View::App { head, args } => {
            let Some(s) = b.sym_of(head) else { return false };
            if s == b.hd.add {
                return args.iter().all(|&a| nonneg(b, a)) && args.iter().any(|&a| no_real_zero(b, a));
            }
            if s == b.hd.mul {
                return args.iter().all(|&a| no_real_zero(b, a));
            }
            if s == b.hd.pow {
                return args[0] == h.sym_term(b.hd.e) || no_real_zero(b, args[0]);
            }
            matches!(h.sym_name(s).as_str(), "exp" | "cosh")
        }
        _ => false,
    }
}

fn positive_exponent<'h>(b: &Build<'h>, e: T<'h>) -> bool {
    matches!(b.heap().view(e), View::Rational { .. }) && b.ar.compare(e, b.zero()) == Some(mc_arith::Cmp3::Greater)
}

/// Is `u` never negative for real values of the variables (by its structure)?
pub fn nonneg<'h>(b: &Build<'h>, u: T<'h>) -> bool {
    // shared subterms are decided once (a term can be a tree exponentially larger than itself)
    nonneg_in(b, u, &mut mc_api::walk::memo())
}

fn nonneg_in<'h>(b: &Build<'h>, u: T<'h>, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
    mc_api::walk::cached(memo, u, |memo| {
        let h = b.heap();
        match h.view(u) {
            View::Int(k) => k >= 0,
            View::Rational { .. } => b.ar.compare(u, b.zero()) != Some(mc_arith::Cmp3::Less),
            View::App { head, args } => {
                let Some(s) = b.sym_of(head) else { return false };
                if s == b.hd.add || s == b.hd.mul && args.iter().all(|&a| nonneg_in(b, a, memo)) {
                    return args.iter().all(|&a| nonneg_in(b, a, memo));
                }
                if s == b.hd.pow {
                    let [base, ex] = args else { return false };
                    return match h.view(*ex) {
                        View::Int(k) => k % 2 == 0 || nonneg_in(b, *base, memo),
                        View::Rational { num, den } => {
                            matches!(h.view(den), View::Int(d) if d % 2 == 0) && nonneg_in(b, *base, memo) || matches!(h.view(num), View::Int(_)) && nonneg_in(b, *base, memo)
                        }
                        _ => *base == h.sym_term(b.hd.e),
                    };
                }
                matches!(h.sym_name(s).as_str(), "abs" | "exp" | "cosh") 
            }
            _ => false,
        }
    })
}

/// Whether the atoms meet the region `cond` (with the facts about the parameters); see the module comment.
pub fn meets<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, atoms: &[T<'h>], cond: Option<T<'h>>) -> Res<Meet<'h>> {
    let h = b.heap();
    let complex = s.dom == Dom::Complexes;
    if atoms.iter().any(|&a| cx.is_true(a)) && cond.is_none() {
        return Ok(Meet::Yes);
    }
    // refute atoms under the region and the facts
    let mut facts: Vec<T<'h>> = s.facts.clone();
    let reals_fact = h.call(h.sym("element"), &[cx.list(&s.vars), h.sym_term(h.sym("reals"))]);
    if !complex {
        facts.push(reals_fact);
    }
    // an empty region (`im_part(z) != 0` for a real z)
    if let Some(c) = cond {
        let r = if complex { cx.eval(c)? } else { cx.eval_assuming(&[reals_fact], h.call(h.sym("refine"), &[c]))? };
        if cx.is_false(r) {
            return Ok(Meet::No);
        }
        facts.push(c);
    }
    let fact = match facts.as_slice() {
        [] => cx.bool(true),
        [f] => *f,
        _ => h.call(h.sym("and"), &facts),
    };
    let mut live = vec![];
    for &a in atoms {
        cx.tick(1)?;
        let mut t = a;
        if !cx.is_true(fact) {
            let r = cx.eval_assuming(&[fact], h.call(h.sym("refine"), &[a]))?;
            if cx.is_false(r) {
                continue;
            }
            // the conjunction with the region may be false at once (`x == 0 and x != 0`)
            if let Some(c) = cond {
                let j = cx.eval(h.call(h.sym("and"), &[a, c]))?;
                if cx.is_false(j) {
                    continue;
                }
            }
            t = a;
        }
        live.push(t);
    }
    if live.is_empty() {
        return Ok(Meet::No);
    }
    // a point `x == c` (c a real number built of numbers and constants, `-1/e`) of the whole line or plane: in the
    // region
    if cond.is_none() {
        for &a in &live {
            let View::App { head, args: &[l, r] } = h.view(a) else { continue };
            if b.sym_of(head) == Some(h.sym("equal")) && s.vars.contains(&l) && numeric_quantity(h, r) {
                let v = cx.eval(b.call("n", &[r]))?;
                if matches!(h.view(v), View::Real(x) if x.is_finite()) || matches!(h.view(v), View::Int(_)) {
                    return Ok(Meet::Yes);
                }
            }
        }
    }
    let mut all = live.clone();
    if let Some(c) = cond {
        all.push(c);
    }
    let ps = super::spec::params(b, &all, &s.vars);
    let set = match live.as_slice() {
        [a] => *a,
        _ => h.call(h.sym("or"), &live),
    };
    let region_and = |set: T<'h>| match cond {
        Some(c) => h.call(h.sym("and"), &[set, c]),
        None => set,
    };
    if ps.is_empty() {
        if complex {
            return meets_complex(cx, b, s, &live, cond);
        }
        let full = region_and(set);
        if s.vars.len() == 1 {
            let r = cx.eval(h.call(h.sym("reduce"), &[full, s.vars[0], h.sym_term(h.sym("reals"))]))?;
            if cx.is_false(r) {
                return Ok(Meet::No);
            }
            if !is_head(b, r, "reduce") && !is_head(b, r, names::FAILURE) {
                return Ok(Meet::Yes);
            }
        } else if polynomial_formula(b, full, &s.vars) {
            let r = cx.eval(b.call("resolve", &[b.call("exists", &[cx.list(&s.vars), full]), h.sym_term(h.sym("reals"))]))?;
            if cx.is_true(r) {
                return Ok(Meet::Yes);
            }
            if cx.is_false(r) {
                return Ok(Meet::No);
            }
        }
        if s.vars.len() == 1 && witness_line(cx, b, &live, cond, s.vars[0])? {
            return Ok(Meet::Yes);
        }
        return Ok(Meet::Unknown);
    }
    // parameters: the condition under which the set misses the region, atom by atom. An atom `P and Q` with `Q` the
    // conjuncts free of the variables (conditions on the parameters alone, `not_element(n, integers)`) misses the
    // region exactly when `not Q` or `for_all(vars, region implies not P)`; the second part is decided by
    // quantifier elimination on a polynomial superset of P (over the complexes on the real and imaginary parts).
    // When the superset is larger than P (non-polynomial conjuncts dropped), its condition to miss the region is
    // sufficient for P, but that it meets the region says nothing about P.
    let mut cond_r = cond;
    let mut vars = s.vars.clone();
    let mut back = vec![];
    if let (true, Some(c)) = (complex, cond) {
        let (rv, rc, bk) = realify(cx, b, c, &s.vars, &ps)?;
        cond_r = Some(rc);
        vars = rv;
        back = bk;
    }
    let mut exact = true;
    let mut misses = vec![];
    for &a in &live {
        cx.tick(1)?;
        let mut parts = vec![];
        conjuncts_of(b, a, &mut parts);
        let (qs, rest): (Vec<T<'h>>, Vec<T<'h>>) = parts.into_iter().partition(|&p| s.vars.iter().all(|&x| crate::common::is_free(b, p, x)));
        let mut alts: Vec<T<'h>> = qs.iter().map(|&q| h.call(h.sym("not"), &[q])).collect();
        if !rest.is_empty() {
            let p = match rest.as_slice() {
                [p] => *p,
                _ => h.call(h.sym("and"), &rest),
            };
            let p = if complex {
                let (rv, rp, bk) = realify(cx, b, p, &s.vars, &ps)?;
                vars = rv;
                back = bk;
                cx.eval(rp)?
            } else {
                p
            };
            let Some(e) = enlarge(cx, b, p, &vars)? else { return Ok(Meet::Unknown) };
            exact &= e == p;
            let miss = match cond_r {
                Some(c) => h.call(h.sym("implies"), &[c, h.call(h.sym("not"), &[e])]),
                None => h.call(h.sym("not"), &[e]),
            };
            if !polynomial_formula(b, miss, &vars) {
                return Ok(Meet::Unknown);
            }
            let r = cx.eval(b.call("resolve", &[b.call("for_all", &[cx.list(&vars), miss]), h.sym_term(h.sym("reals"))]))?;
            if is_head(b, r, "resolve") || is_head(b, r, names::FAILURE) || is_head(b, r, "for_all") {
                return Ok(Meet::Unknown);
            }
            let r = if back.is_empty() { r } else { cx.eval(h.call(h.sym("rewrite"), &[r, cx.list(&back)]))? };
            alts.push(r);
        }
        if alts.is_empty() {
            // an atom that holds everywhere (refuted above when the facts exclude it)
            return Ok(Meet::Yes);
        }
        misses.push(cx.eval(h.call(h.sym("or"), &alts))?);
    }
    let mut r = cx.eval(h.call(h.sym("and"), &misses))?;
    // a condition the decision service refutes (`(element(a, integers) and a <= 0) and (not_element(a, integers) or
    // a > 0)`): `conditional_expression(v, r)` is then undefined
    if !cx.is_true(r) && !cx.is_false(r) {
        let v = cx.eval(h.call(h.sym(mc_decide::CONDITIONAL), &[cx.bool(true), r]))?;
        if v == h.sym_term(h.sym("undefined")) || propositionally_false(b, r) {
            r = cx.bool(false);
        }
    }
    // under the facts about the parameters
    if !s.facts.is_empty() {
        let pf = match s.facts.as_slice() {
            [f] => *f,
            _ => h.call(h.sym("and"), &s.facts),
        };
        let rr = cx.eval_assuming(&[pf], h.call(h.sym("refine"), &[r]))?;
        if cx.is_true(rr) {
            return Ok(Meet::No);
        }
        // the set meets the region for every admissible value of the parameters
        if cx.is_false(rr) {
            return Ok(if exact { Meet::Yes } else { Meet::Unknown });
        }
    }
    if cx.is_true(r) {
        return Ok(Meet::No);
    }
    if cx.is_false(r) {
        return Ok(if exact { Meet::Yes } else { Meet::Unknown });
    }
    // (with a superset the condition is sufficient: `a > 0` for the poles of gamma(x^2 + a))
    Ok(Meet::NoWhen(r))
}

/// Is the formula false for every truth value of its atoms (`not element(n, integers) and not not_element(n,
/// integers)`)? `not_element(u, d)` is read as `not element(u, d)`; at most 10 atoms are tried.
fn propositionally_false<'h>(b: &Build<'h>, t: T<'h>) -> bool {
    // the atoms, with not_element read through element
    fn atoms<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
        let h = b.heap();
        if let View::App { head, args } = h.view(t) {
            match b.sym_of(head).map(|s| h.sym_name(s)).as_deref() {
                Some("and" | "or" | "not") => {
                    for &a in args {
                        atoms(b, a, out);
                    }
                    return;
                }
                Some("not_element") => {
                    let e = h.call(h.sym("element"), args);
                    if !out.contains(&e) {
                        out.push(e);
                    }
                    return;
                }
                _ => {}
            }
        }
        if !out.contains(&t) {
            out.push(t);
        }
    }
    fn value<'h>(b: &Build<'h>, t: T<'h>, atoms: &[T<'h>], bits: u32) -> bool {
        // shared subterms are decided once (a term can be a tree exponentially larger than itself)
        value_in(b, t, atoms, bits, &mut mc_api::walk::memo())
    }

    fn value_in<'h>(b: &Build<'h>, t: T<'h>, atoms: &[T<'h>], bits: u32, memo: &mut mc_api::walk::Memo<T<'h>, bool>) -> bool {
        mc_api::walk::cached(memo, t, |memo| {
            let h = b.heap();
            let at = |u: T<'h>| atoms.iter().position(|&a| a == u).is_some_and(|i| bits >> i & 1 == 1);
            if let View::Sym(s) = h.view(t) {
                match h.sym_name(s).as_str() {
                    "true" => return true,
                    "false" => return false,
                    _ => {}
                }
            }
            if let View::App { head, args } = h.view(t) {
                match b.sym_of(head).map(|s| h.sym_name(s)).as_deref() {
                    Some("and") => return args.iter().all(|&a| value_in(b, a, atoms, bits, memo)),
                    Some("or") => return args.iter().any(|&a| value_in(b, a, atoms, bits, memo)),
                    Some("not") if args.len() == 1 => return !value_in(b, args[0], atoms, bits, memo),
                    Some("not_element") => return !at(h.call(h.sym("element"), args)),
                    _ => {}
                }
            }
            at(t)
        })
    }
    let mut ats = vec![];
    atoms(b, t, &mut ats);
    ats.len() <= 10 && (0..1u32 << ats.len()).all(|bits| !value(b, t, &ats, bits))
}

/// The conjuncts of a nested `and` (the term itself when it is none).
fn conjuncts_of<'h>(b: &Build<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    let h = b.heap();
    match h.view(t) {
        View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => {
            for &a in args {
                conjuncts_of(b, a, out);
            }
        }
        _ => out.push(t),
    }
}

/// A polynomial superset of the atom: the atom itself when polynomial; a cut with its non-polynomial conjuncts
/// dropped; `None` otherwise.
fn enlarge<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, a: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = b.heap();
    if polynomial_formula(b, a, vars) {
        return Ok(Some(a));
    }
    if let View::App { head, args } = h.view(a) {
        if b.sym_of(head) == Some(h.sym("and")) {
            let kept: Vec<T<'h>> = args.iter().copied().filter(|&c| polynomial_formula(b, c, vars)).collect();
            if !kept.is_empty() {
                return Ok(Some(match kept.as_slice() {
                    [k] => *k,
                    _ => cx.eval(h.call(h.sym("and"), &kept))?,
                }));
            }
        }
    }
    Ok(None)
}

/// The formula over the real and imaginary parts of the complex variables and parameters: the new real
/// variables (those of `vars` first), the formula, and the rules taking the parameters' parts back to
/// `re(p)`, `im_part(p)`.
#[allow(clippy::type_complexity)]
pub fn realify<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, vars: &[T<'h>], ps: &[T<'h>]) -> Res<(Vec<T<'h>>, T<'h>, Vec<T<'h>>)> {
    let h = b.heap();
    let imu = h.sym_term(h.sym("im"));
    let mut subst = vec![];
    let mut rv = vec![];
    let mut back = vec![];
    for (i, &z) in vars.iter().chain(ps.iter()).enumerate() {
        let name = match h.view(z) {
            View::Sym(s) => h.sym_name(s).to_string(),
            _ => format!("v{i}"),
        };
        let (zr, zi) = (h.sym_term(h.sym(&format!("{name}$re"))), h.sym_term(h.sym(&format!("{name}$im"))));
        subst.push(h.call(h.sym(names::RULE), &[z, b.add(&[zr, b.mul(&[imu, zi])])]));
        if i < vars.len() {
            rv.push(zr);
            rv.push(zi);
        } else {
            back.push(h.call(h.sym(names::RULE), &[zr, b.call("re", &[z])]));
            back.push(h.call(h.sym(names::RULE), &[zi, b.call("im_part", &[z])]));
        }
    }
    let t = cx.eval(h.call(h.sym("rewrite"), &[t, cx.list(&subst)]))?;
    let r = real_formula(cx, b, t)?;
    Ok((rv, r, back))
}

/// A formula about complex expressions of real symbols as one about their real and imaginary parts.
fn real_formula<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<T<'h>> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let Some(s) = b.sym_of(head) else { return Ok(t) };
    let ce = |cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>| cx.eval(b.call("complex_expand", &[u]));
    match h.sym_name(s).as_str() {
        "and" | "or" | "not" | "implies" => {
            let mut v = vec![];
            for &a in args {
                v.push(real_formula(cx, b, a)?);
            }
            Ok(h.app(head, &v))
        }
        n @ ("equal" | "unequal") => {
            let d = b.sub(args[0], args[1]);
            let re = ce(cx, b.call("re", &[d]))?;
            let im = ce(cx, b.call("im_part", &[d]))?;
            let z = b.zero();
            Ok(if n == "equal" {
                b.call("and", &[b.call("equal", &[re, z]), b.call("equal", &[im, z])])
            } else {
                b.call("or", &[b.call("unequal", &[re, z]), b.call("unequal", &[im, z])])
            })
        }
        "less" | "less_equal" | "greater" | "greater_equal" => {
            let l = ce(cx, args[0])?;
            let r = ce(cx, args[1])?;
            Ok(h.app(head, &[l, r]))
        }
        _ => Ok(t),
    }
}

/// Complex variables without parameters (module comment).
fn meets_complex<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &Spec<'h>, live: &[T<'h>], cond: Option<T<'h>>) -> Res<Meet<'h>> {
    let h = b.heap();
    if cond.is_none() {
        for &a in live {
            if always_zero_somewhere(cx, b, a, &s.vars)? {
                return Ok(Meet::Yes);
            }
        }
    }
    let set = match live {
        [a] => *a,
        _ => h.call(h.sym("or"), live),
    };
    let full = match cond {
        Some(c) => h.call(h.sym("and"), &[set, c]),
        None => set,
    };
    let (rv, rf, _) = realify(cx, b, full, &s.vars, &[])?;
    if polynomial_formula(b, rf, &rv) {
        let r = cx.eval(b.call("resolve", &[b.call("exists", &[cx.list(&rv), rf]), h.sym_term(h.sym("reals"))]))?;
        if cx.is_true(r) {
            return Ok(Meet::Yes);
        }
        if cx.is_false(r) {
            return Ok(Meet::No);
        }
    }
    Ok(Meet::Unknown)
}

/// `p == 0` with `p` a nonconstant polynomial in the variables, or `g(p) == 0` for `g` among `cos`, `sin`,
/// `cosh`, `sinh` (or a gamma pole), which always has complex solutions.
fn always_zero_somewhere<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, a: T<'h>, vars: &[T<'h>]) -> Res<bool> {
    let h = b.heap();
    let a = if is_head(b, a, "and") {
        // a gamma pole: sin(pi u) == 0 and re(u) <= 0 has solutions when u is a nonconstant polynomial
        let View::App { args, .. } = h.view(a) else { return Ok(false) };
        args[0]
    } else {
        a
    };
    let View::App { head, args: &[l, r] } = h.view(a) else { return Ok(false) };
    if b.sym_of(head) != Some(h.sym("equal")) {
        return Ok(false);
    }
    let d = cx.eval(b.sub(l, r))?;
    if poly_degree(b, d, vars).is_some_and(|k| k >= 1) && !vars.iter().all(|&x| is_free(b, d, x)) {
        // nonconstant in some variable: the degree in that variable is positive
        return Ok(true);
    }
    if let View::App { head: gh, args: &[u] } = h.view(d) {
        if b.sym_of(gh).is_some_and(|g| matches!(h.sym_name(g).as_str(), "cos" | "sin" | "cosh" | "sinh")) && poly_degree(b, u, vars).is_some_and(|k| k >= 1) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A numeric witness on the real line that some atom holds inside the region (module comment).
fn witness_line<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, live: &[T<'h>], cond: Option<T<'h>>, x: T<'h>) -> Res<bool> {
    let h = b.heap();
    let mut pts: Vec<f64> = (-400..=400).map(|k| k as f64 * 0.05 + 0.00123).collect();
    for k in -12..=12 {
        let p = 10f64.powf(k as f64 / 2.0);
        pts.push(p);
        pts.push(-p);
    }
    pts.sort_by(f64::total_cmp);
    let at = |cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, v: f64| -> Res<T<'h>> {
        let r = cx.eval(h.call(h.sym("rewrite"), &[t, h.call(h.sym(names::RULE), &[x, h.real(v)])]))?;
        cx.eval(h.call(h.sym("n"), &[r]))
    };
    let inside = |cx: &mut NativeCx<'_, '_, 'h>, v: f64| -> Res<bool> {
        match cond {
            Some(c) => {
                let r = at(cx, c, v)?;
                Ok(cx.is_true(r))
            }
            None => Ok(true),
        }
    };
    for &a in live {
        // split into the equation (if any) and the other conjuncts
        let parts: Vec<T<'h>> = match h.view(a) {
            View::App { head, args } if b.sym_of(head) == Some(h.sym("and")) => args.to_vec(),
            _ => vec![a],
        };
        let eq = parts.iter().copied().find(|&p| is_head(b, p, "equal"));
        let others: Vec<T<'h>> = parts.iter().copied().filter(|&p| Some(p) != eq).collect();
        let ok_others = |cx: &mut NativeCx<'_, '_, 'h>, v: f64| -> Res<bool> {
            for &o in &others {
                let r = at(cx, o, v)?;
                if !cx.is_true(r) {
                    return Ok(false);
                }
            }
            inside(cx, v)
        };
        // the conjuncts (and the region) hold at the exact point q, or for every point of the ball q
        let mut all: Vec<T<'h>> = others.clone();
        all.extend(cond);
        let exact_ok = |cx: &mut NativeCx<'_, '_, 'h>, q: T<'h>| -> Res<bool> { super::proof::holds_at(cx, b, &all, x, q) };
        let mut tries = 0;
        match eq {
            None => {
                for &v in &pts {
                    cx.tick(1)?;
                    if ok_others(cx, v)? {
                        tries += 1;
                        if tries > 24 {
                            break;
                        }
                        let q = super::proof::exact_point(cx, b, v)?;
                        if exact_ok(cx, q)? {
                            return Ok(true);
                        }
                    }
                }
            }
            Some(e) => {
                let View::App { args: &[l, r], .. } = h.view(e) else { continue };
                let g = cx.eval(b.sub(l, r))?;
                let val = |cx: &mut NativeCx<'_, '_, 'h>, v: f64| -> Res<Option<f64>> {
                    let y = at(cx, g, v)?;
                    Ok(match h.view(y) {
                        View::Real(y) if y.is_finite() => Some(y),
                        View::Int(k) => Some(k as f64),
                        _ => None,
                    })
                };
                let mut prev: Option<(f64, f64)> = None;
                let gb = super::proof::with_ball_constants(cx, b, g)?;
                for &v in &pts {
                    cx.tick(1)?;
                    let y = val(cx, v)?;
                    if let (Some((pv, py)), Some(y)) = (prev, y) {
                        if py == 0.0 && ok_others(cx, pv)? {
                            // an exact zero at the exact point
                            let q = super::proof::exact_point(cx, b, pv)?;
                            if super::proof::point_sign(cx, b, g, x, q, &all)? == Some(0) {
                                return Ok(true);
                            }
                        }
                        if py * y < 0.0 && v - pv < 1.0 && tries < 24 {
                            tries += 1;
                            // bisect to a small residual
                            let (mut lo, mut hi, mut ylo) = (pv, v, py);
                            let mut good = true;
                            for _ in 0..60 {
                                let m = 0.5 * (lo + hi);
                                let Some(ym) = val(cx, m)? else {
                                    good = false;
                                    break;
                                };
                                if ym == 0.0 {
                                    lo = m;
                                    hi = m;
                                    break;
                                }
                                if ym * ylo < 0.0 {
                                    hi = m;
                                } else {
                                    lo = m;
                                    ylo = ym;
                                }
                            }
                            // a zero inside [lo, hi] proved: opposite proved signs at the exact ends, g continuous on
                            // the bracket (an enclosure over it), the other conjuncts and the region true on the
                            // whole bracket (evaluated on its ball)
                            if good && lo < hi {
                                let (ql, qh) = (super::proof::exact_point(cx, b, lo)?, super::proof::exact_point(cx, b, hi)?);
                                let (sl, sh) = (super::proof::point_sign(cx, b, g, x, ql, &[])?, super::proof::point_sign(cx, b, g, x, qh, &[])?);
                                // an end of the bracket that is itself an exact zero (a zero at a binary rational,
                                // as sin(2 pi x) at 1/2)
                                for (s, q) in [(sl, ql), (sh, qh)] {
                                    if s == Some(0) && exact_ok(cx, q)? {
                                        return Ok(true);
                                    }
                                }
                                if matches!((sl, sh), (Some(1), Some(-1)) | (Some(-1), Some(1))) && super::proof::enclose(cx, b, gb, x, lo, hi)?.is_some() {
                                    let ball = super::proof::ball_of(b, lo, hi);
                                    if super::proof::holds_at(cx, b, &all, x, ball)? {
                                        return Ok(true);
                                    }
                                }
                            } else if good && ok_others(cx, lo)? {
                                let q = super::proof::exact_point(cx, b, lo)?;
                                if super::proof::point_sign(cx, b, g, x, q, &all)? == Some(0) {
                                    return Ok(true);
                                }
                            }
                        }
                    }
                    prev = y.map(|y| (v, y));
                }
            }
        }
    }
    Ok(false)
}

/// A condition in the conventional form (decision R25P.9): `c < p` alone becomes `p > c` (a bound of an interval
/// `c < p and p < d` stays), and `p < c or c < p` becomes `p != c`.
pub fn tidy_cond<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<T<'h>> {
    tidy_cond_with(cx, b, t, true)
}

/// [`tidy_cond`] keeping `x < c or x > c` apart (the form of a real domain or range of one variable).
pub fn tidy_cond_split<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>) -> Res<T<'h>> {
    tidy_cond_with(cx, b, t, false)
}

fn tidy_cond_with<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, collapse: bool) -> Res<T<'h>> {
    // shared parts are tidied once (a condition can be a tree exponentially larger than itself)
    tidy_cond_in(cx, b, t, collapse, &mut mc_api::walk::memo())
}

fn tidy_cond_in<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, collapse: bool, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    if let Some(&r) = memo.get(&t) {
        return Ok(r);
    }
    let r = tidy_cond_node(cx, b, t, collapse, memo)?;
    memo.insert(t, r);
    Ok(r)
}

fn tidy_cond_node<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, collapse: bool, memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> Res<T<'h>> {
    let h = b.heap();
    let flip = |n: &str| match n {
        "less" => Some("greater"),
        "less_equal" => Some("greater_equal"),
        "greater" => Some("less"),
        "greater_equal" => Some("less_equal"),
        _ => None,
    };
    let rel_parts = |u: T<'h>| -> Option<(String, T<'h>, T<'h>)> {
        let View::App { head, args: &[l, r] } = h.view(u) else { return None };
        let n = h.sym_name(b.sym_of(head)?).to_string();
        flip(&n)?;
        Some((n, l, r))
    };
    let is_num = |u: T<'h>| b.ar.num(u).is_some() || is_constant(h, u) || numeric_quantity(h, u);
    let View::App { head, args } = h.view(t) else { return Ok(t) };
    let Some(hs) = b.sym_of(head) else { return Ok(t) };
    match h.sym_name(hs).as_str() {
        "or" => {
            if let [p, q] = args {
                if let (true, Some((n1, l1, r1)), Some((n2, l2, r2))) = (collapse, rel_parts(*p), rel_parts(*q)) {
                    if n1 == "less" && n2 == "less" && l1 == r2 && r1 == l2 {
                        let (v, c) = if is_num(l1) { (r1, l1) } else { (l1, r1) };
                        return cx.eval(b.call("unequal", &[v, c]));
                    }
                }
            }
            let mut v = vec![];
            for &a in args {
                v.push(tidy_cond_in(cx, b, a, collapse, memo)?);
            }
            cx.eval(h.app(head, &v))
        }
        "and" => {
            // the symbols bounded on both sides by the relations here
            let mut v = vec![];
            for &a in args {
                let keep = match rel_parts(a) {
                    Some((_, l, r)) if is_num(l) && !is_num(r) => args.iter().any(|&o| o != a && rel_parts(o).is_some_and(|(_, l2, r2)| l2 == r && is_num(r2))),
                    _ => true,
                };
                v.push(if keep { if rel_parts(a).is_some() { a } else { tidy_cond_in(cx, b, a, collapse, memo)? } } else { tidy_cond_in(cx, b, a, collapse, memo)? });
            }
            cx.eval(h.app(head, &v))
        }
        n => {
            if let (Some(f), [l, r]) = (flip(n), args) {
                if is_num(*l) && !is_num(*r) {
                    return cx.eval(b.call(f, &[*r, *l]));
                }
            }
            Ok(t)
        }
    }
}
