//! Conditions of rules and the pure values they compute: a condition is decided on the terms the match bound,
//! with canonical arithmetic only (no evaluation), so it is cheap and has no effects.
use crate::syntax::{Cmp, N};
use crate::util::{call_of, const_sign, free_of, is_integer, is_rational, pos_looking, small_rat};
use mc_api::{Store, View, T};
use mc_arith::{Arith, Cmp3};
use mc_core::Heap;

/// The value of a node from the bound terms `vals` and the variable `x`, built by canonical arithmetic; calls of
/// functions are plain calls (evaluated later). `None` for a utility that needs the interpreter (`int`, `subst`,
/// `expand`) or a numeric utility applied to a non-number.
pub fn value<'h>(ar: &Arith<'h, Heap>, n: &N, vals: &[T<'h>], x: T<'h>) -> Option<T<'h>> {
    let h = ar.store();
    Some(match n {
        N::Int(v) => h.int(*v),
        N::X => x,
        N::Var(i) | N::Opt(i) => vals[*i as usize],
        N::Sym(s) => constant(ar, s),
        N::Add(v) => {
            let a: Option<Vec<T<'h>>> = v.iter().map(|a| value(ar, a, vals, x)).collect();
            ar.add(&a?)
        }
        N::Mul(v) => {
            let a: Option<Vec<T<'h>>> = v.iter().map(|a| value(ar, a, vals, x)).collect();
            ar.mul(&a?)
        }
        N::Pow(b, e) => ar.pow(value(ar, b, vals, x)?, value(ar, e, vals, x)?),
        N::Call(f, args) => {
            let a: Option<Vec<T<'h>>> = args.iter().map(|a| value(ar, a, vals, x)).collect();
            let a = a?;
            return function(ar, f, &a);
        }
        N::Cmp(..) | N::And(_) | N::Or(_) | N::Not(_) => return None,
    })
}

/// `pi`, `e`, `im`
pub fn constant<'h>(ar: &Arith<'h, Heap>, s: &str) -> T<'h> {
    let h = ar.store();
    match s {
        "im" => ar.complex(h.int(0), h.int(1)),
        _ => h.sym_term(h.sym(s)),
    }
}

/// A function of already built arguments: the numeric utilities computed, `sqrt` and `exp` as powers, other
/// functions as plain calls; `None` for the interpreter's utilities.
pub fn function<'h>(ar: &Arith<'h, Heap>, f: &str, a: &[T<'h>]) -> Option<T<'h>> {
    let h = ar.store();
    Some(match (f, a) {
        ("sqrt", [u]) => ar.pow(*u, ar.rational(h.int(1), h.int(2))),
        ("exp", [u]) => ar.pow(constant(ar, "e"), *u),
        ("numerator", [u]) => match small_rat(h, *u) {
            Some((n, _)) => h.int(n as i64),
            None => return None,
        },
        ("denominator", [u]) => match small_rat(h, *u) {
            Some((_, d)) => h.int(d as i64),
            None => return None,
        },
        // the integer part rounds toward zero, the fractional part is the rest (with the sign of u)
        ("int_part", [u]) => match small_rat(h, *u) {
            Some((n, d)) => h.int((n / d) as i64),
            None => return None,
        },
        ("frac_part", [u]) => match small_rat(h, *u) {
            Some((n, d)) => ar.rational(h.int((n % d) as i64), h.int(d as i64)),
            None => return None,
        },
        ("simp", [u]) => *u,
        ("int" | "subst", _) => return None,
        (f, _) if crate::table::EVAL_UTILITIES.contains(&f) => return None,
        _ => h.call(h.sym(f), a),
    })
}

/// Is `t` zero by canonical arithmetic?
pub fn is_zero<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> bool {
    ar.num(t).is_some_and(|n| n.is_exact_zero())
}

/// Does the condition hold for the bound terms?
pub fn holds<'h>(ar: &Arith<'h, Heap>, n: &N, vals: &[T<'h>], x: T<'h>) -> bool {
    let h = ar.store();
    let v = |a: &N| value(ar, a, vals, x);
    match n {
        N::And(c) => c.iter().all(|c| holds(ar, c, vals, x)),
        N::Or(c) => c.iter().any(|c| holds(ar, c, vals, x)),
        N::Not(c) => !holds(ar, c, vals, x),
        N::Cmp(op, a, b) => {
            let (Some(a), Some(b)) = (v(a), v(b)) else {
                return false;
            };
            compare(ar, *op, a, b)
        }
        N::Call(f, args) => {
            let Some(a): Option<Vec<T<'h>>> = args.iter().map(v).collect() else {
                return false;
            };
            match &**f {
                "free" => a.iter().all(|&t| free_of(h, t, x)),
                "integer" => a.iter().all(|&t| is_integer(h, t)),
                "rational" => a.iter().all(|&t| is_rational(h, t)),
                "half_integer" => a.iter().all(|&t| small_rat(h, t).is_some_and(|(_, d)| d == 2)),
                "odd" => a.iter().all(|&t| small_rat(h, t).is_some_and(|(n, d)| d == 1 && n % 2 != 0)),
                "even" => a.iter().all(|&t| small_rat(h, t).is_some_and(|(n, d)| d == 1 && n % 2 == 0)),
                "eq" => a.len() == 2 && is_zero(ar, ar.sub(a[0], a[1])),
                "ne" => a.len() == 2 && !is_zero(ar, ar.sub(a[0], a[1])),
                "pos" => a.iter().all(|&t| !is_zero(ar, t) && pos_looking(ar, t)),
                "neg" => a.iter().all(|&t| !is_zero(ar, t) && !pos_looking(ar, t)),
                "number" => a.iter().all(|&t| ar.num(t).is_some()),
                "nonzero_number" => a.iter().all(|&t| ar.num(t).is_some_and(|n| !n.is_zero())),
                "symbol" => a.iter().all(|&t| matches!(h.view(t), View::Sym(_))),
                "complex_free" => a.iter().all(|&t| complex_free(ar, t)),
                _ => false,
            }
        }
        _ => false,
    }
}

/// A comparison: `==` and `!=` by canonical arithmetic (`!=` holds unless the difference is zero), the orders only
/// when decided (both real numbers, or a difference whose sign is evident).
pub fn compare<'h>(ar: &Arith<'h, Heap>, op: Cmp, a: T<'h>, b: T<'h>) -> bool {
    match op {
        Cmp::Eq => is_zero(ar, ar.sub(a, b)),
        Cmp::Ne => !is_zero(ar, ar.sub(a, b)),
        _ => {
            let c = match ar.compare(a, b) {
                Some(Cmp3::Less) => -1,
                Some(Cmp3::Equal) => 0,
                Some(Cmp3::Greater) => 1,
                Some(_) => return false,
                None => match const_sign(ar, ar.sub(a, b)) {
                    Some(s) => s as i32,
                    None => return false,
                },
            };
            match op {
                Cmp::Lt => c < 0,
                Cmp::Le => c <= 0,
                Cmp::Gt => c > 0,
                Cmp::Ge => c >= 0,
                _ => false,
            }
        }
    }
}

fn complex_free<'h>(ar: &Arith<'h, Heap>, t: T<'h>) -> bool {
    let h = ar.store();
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let Some(n) = ar.num(u) {
            if n.as_real().is_none() {
                return false;
            }
            continue;
        }
        if let View::App { args, .. } = h.view(u) {
            if call_of(h, u).is_some_and(|(s, _)| h.sym_name(s) == "complex") {
                return false;
            }
            stack.extend_from_slice(args);
        }
    }
    true
}
