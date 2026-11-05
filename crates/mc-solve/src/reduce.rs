//! `reduce(cond, x, reals)`: a condition on one real variable (polynomial and rational equations and inequalities
//! with rational coefficients, combined by `and`, `or`, `not` and lists) as a disjunction of intervals and points
//! (decision 3B.13).
//!
//! The real roots of the irreducible factors of every numerator and denominator are isolated exactly
//! ([`crate::realroots`]) and refined until their intervals are disjoint; they cut the line into points and open
//! cells. A cell's truth is decided at a rational sample point by exact signs; at a root of the factor f, a
//! polynomial vanishes when f divides it and otherwise has the sign of the cell to its right. A point where a
//! denominator vanishes is outside the domain (false). Adjacent true pieces merge into intervals, written
//! `a < x and x <= b`, `x < a`, `x == a`; roots are rationals, quadratic radicals, or root objects.
use crate::cx::{Env, Out};
use crate::realroots::{halve, isolate, sign_at, Interval};
use crate::univariate::Alg;
use mc_api::{names, T};
use mc_arith::num::{Int, Rat};
use mc_poly::factor::factor;
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};
use std::cmp::Ordering;

/// A relation of an atom `d rel 0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

/// A condition.
enum Cond {
    Const(bool),
    /// atom index
    Atom(usize, Rel),
    And(Vec<Cond>),
    Or(Vec<Cond>),
    Not(Box<Cond>),
}

/// An atom: the difference as numerator / denominator (dense integer polynomials in x).
struct Atom {
    num: Vec<Z>,
    den: Vec<Z>,
    /// the indices of the irreducible factors dividing num and den
    num_factors: Vec<usize>,
    den_factors: Vec<usize>,
}

/// `reduce(cond, x)` or `reduce(cond, x, reals)`.
pub(crate) fn reduce<'h>(env: &Env<'h>, args: &[T<'h>], m: &mut Meter) -> WorkResult<Out<'h>> {
    let (cond, x) = match args {
        [c, x] => (*c, *x),
        [c, x, d] if env.is_sym(*d, "reals") => (*c, *x),
        [_, _, _] => return Ok(env.arg_failure("reduce works over the reals")),
        _ => return Ok(env.arg_failure("expected a condition, a variable and reals")),
    };
    if env.ar.num(x).is_some() || env.as_list(x).is_some() {
        return Ok(env.arg_failure("reduce takes one variable"));
    }
    let mut diffs: Vec<T<'h>> = vec![];
    let c = match parse(env, cond, &mut diffs) {
        Ok(c) => c,
        Err(msg) => return Ok(env.arg_failure(msg)),
    };
    // the atoms as rational functions of x with rational coefficients
    let ring = env.ring(&diffs, &[x], m)?;
    if ring.kernels.iter().any(|&k| k != x) {
        return Ok(env.failure("unsolved", "reduce handles one real variable with rational coefficients"));
    }
    let v = ring.var(x).expect("x is in the ring");
    let rc = env.rconv();
    let mut atoms: Vec<Atom> = vec![];
    let mut factors: Vec<Vec<Z>> = vec![];
    for &d in &diffs {
        let f = match ratfun::read(&rc, d, &ring, m)? {
            Ok(f) => f,
            Err(_) => return Ok(env.arg_failure("a condition divides by zero")),
        };
        let nf = factor_indices(&f.num, v, &mut factors, m)?;
        let df = factor_indices(&f.den, v, &mut factors, m)?;
        atoms.push(Atom { num: dense(&f.num, v), den: dense(&f.den, v), num_factors: nf, den_factors: df });
    }
    // the real roots of every factor, refined until disjoint: (factor, index among its real roots, interval)
    let mut roots: Vec<(usize, usize, Interval)> = vec![];
    for (i, f) in factors.iter().enumerate() {
        for (j, iv) in isolate(f, m)?.into_iter().enumerate() {
            roots.push((i, j, iv));
        }
    }
    loop {
        m.step(roots.len() as u64 + 1)?;
        roots.sort_by(|a, b| a.2.lo.cmp_value(&b.2.lo).then(a.2.hi.cmp_value(&b.2.hi)));
        let clash = (1..roots.len()).find(|&k| roots[k - 1].2.hi.cmp_value(&roots[k].2.lo) != Ordering::Less);
        let Some(k) = clash else { break };
        // halve the wider of the two (an exact root has width zero)
        let w = |iv: &Interval| iv.hi.sub(&iv.lo);
        let pick = if w(&roots[k - 1].2).cmp_value(&w(&roots[k].2)) == Ordering::Less { k } else { k - 1 };
        let f = &factors[roots[pick].0];
        roots[pick].2 = halve(f, &roots[pick].2);
    }
    // samples of the open cells: below the first root, between roots, above the last
    let one = Rat::from_i64(1);
    let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
    let samples: Vec<Rat> = if roots.is_empty() {
        vec![Rat::from_i64(0)]
    } else {
        let mut s = vec![roots[0].2.lo.sub(&one)];
        for k in 1..roots.len() {
            s.push(roots[k - 1].2.hi.add(&roots[k].2.lo).mul(&half));
        }
        s.push(roots[roots.len() - 1].2.hi.add(&one));
        s
    };
    // signs of every atom: at the cells (num sign * den sign) and at the roots
    let cell_sign = |a: &Atom, s: &Rat| -> Option<i32> {
        let d = sign_at(&a.den, s);
        (d != 0).then(|| sign_at(&a.num, s) * d)
    };
    let mut truth: Vec<bool> = vec![];
    for k in 0..samples.len() {
        m.step(atoms.len() as u64 + 1)?;
        let signs: Vec<Option<i32>> = atoms.iter().map(|a| cell_sign(a, &samples[k])).collect();
        truth.push(eval(&c, &signs) == Some(true));
        if k < roots.len() {
            let fi = roots[k].0;
            let signs: Vec<Option<i32>> = atoms
                .iter()
                .map(|a| {
                    if a.den_factors.contains(&fi) {
                        None
                    } else if a.num_factors.contains(&fi) {
                        Some(0)
                    } else {
                        cell_sign(a, &samples[k + 1])
                    }
                })
                .collect();
            truth.push(eval(&c, &signs) == Some(true));
        }
    }
    // pieces: cell 0, root 0, cell 1, ..., root k-1, cell k
    let mut root_term = |k: usize| -> WorkResult<T<'h>> { root_value(env, x, &factors[roots[k].0], roots[k].1, &ring, m) };
    let mut parts: Vec<T<'h>> = vec![];
    let mut i = 0;
    let mut values: Vec<Option<T<'h>>> = vec![None; roots.len()];
    let mut value = |k: usize, values: &mut Vec<Option<T<'h>>>| -> WorkResult<T<'h>> {
        if let Some(t) = values[k] {
            return Ok(t);
        }
        let t = root_term(k)?;
        values[k] = Some(t);
        Ok(t)
    };
    while i < truth.len() {
        if !truth[i] {
            i += 1;
            continue;
        }
        let start = i;
        while i + 1 < truth.len() && truth[i + 1] {
            i += 1;
        }
        let end = i;
        i += 1;
        // piece p: even = cell p/2, odd = root (p-1)/2
        if start == end && start % 2 == 1 {
            let r = value((start - 1) / 2, &mut values)?;
            parts.push(env.call("equal", &[x, r]));
            continue;
        }
        let lower = if start == 0 {
            None
        } else if start % 2 == 1 {
            Some(env.call("less_equal", &[value((start - 1) / 2, &mut values)?, x]))
        } else {
            Some(env.call("less", &[value(start / 2 - 1, &mut values)?, x]))
        };
        let upper = if end == truth.len() - 1 {
            None
        } else if end % 2 == 1 {
            Some(env.call("less_equal", &[x, value((end - 1) / 2, &mut values)?]))
        } else {
            Some(env.call("less", &[x, value(end / 2, &mut values)?]))
        };
        parts.push(match (lower, upper) {
            (None, None) => env.sym(names::TRUE),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (Some(a), Some(b)) => env.call("and", &[a, b]),
        });
    }
    Ok(Out::Again(match parts.as_slice() {
        [] => env.sym(names::FALSE),
        [one] => *one,
        _ => env.call("or", &parts),
    }))
}

/// The truth of a condition from the signs of its atoms (`None` for an atom outside its domain).
fn eval(c: &Cond, signs: &[Option<i32>]) -> Option<bool> {
    match c {
        Cond::Const(b) => Some(*b),
        Cond::Atom(i, rel) => {
            let s = signs[*i]?;
            Some(match rel {
                Rel::Lt => s < 0,
                Rel::Le => s <= 0,
                Rel::Gt => s > 0,
                Rel::Ge => s >= 0,
                Rel::Eq => s == 0,
                Rel::Ne => s != 0,
            })
        }
        Cond::And(cs) => {
            let mut all = true;
            for c in cs {
                all &= eval(c, signs)?;
            }
            Some(all)
        }
        Cond::Or(cs) => {
            let mut any = false;
            for c in cs {
                any |= eval(c, signs)?;
            }
            Some(any)
        }
        Cond::Not(c) => eval(c, signs).map(|b| !b),
    }
}

/// The condition tree of `t`; the differences of its atoms are pushed to `diffs`.
fn parse<'h>(env: &Env<'h>, t: T<'h>, diffs: &mut Vec<T<'h>>) -> Result<Cond, &'static str> {
    if env.is_sym(t, names::TRUE) {
        return Ok(Cond::Const(true));
    }
    if env.is_sym(t, names::FALSE) {
        return Ok(Cond::Const(false));
    }
    if let Some(items) = env.as_list(t) {
        return Ok(Cond::And(items.iter().map(|&u| parse(env, u, diffs)).collect::<Result<_, _>>()?));
    }
    let Some((name, args)) = env.head_name(t) else { return Err("expected equations and inequalities") };
    let rel = match name.as_str() {
        "and" => return Ok(Cond::And(args.iter().map(|&u| parse(env, u, diffs)).collect::<Result<_, _>>()?)),
        "or" => return Ok(Cond::Or(args.iter().map(|&u| parse(env, u, diffs)).collect::<Result<_, _>>()?)),
        "not" => match args {
            [u] => return Ok(Cond::Not(Box::new(parse(env, *u, diffs)?))),
            _ => return Err("not takes one argument"),
        },
        "less" => Rel::Lt,
        "less_equal" => Rel::Le,
        "greater" => Rel::Gt,
        "greater_equal" => Rel::Ge,
        "equal" => Rel::Eq,
        "unequal" => Rel::Ne,
        _ => return Err("expected equations and inequalities"),
    };
    if args.len() < 2 {
        return Err("a comparison takes two sides");
    }
    // a chain a < b < c is (a < b) and (b < c)
    let mut out = vec![];
    for w in args.windows(2) {
        diffs.push(env.ar.sub(w[0], w[1]));
        out.push(Cond::Atom(diffs.len() - 1, rel));
    }
    Ok(if out.len() == 1 { out.pop().expect("one") } else { Cond::And(out) })
}

/// The dense coefficients of a polynomial in x alone.
fn dense(p: &Poly, v: usize) -> Vec<Z> {
    if p.is_zero() {
        return vec![Z::ZERO];
    }
    p.to_dense(v)
}

/// The indices (in `factors`) of the irreducible factors of p, adding new ones.
fn factor_indices(p: &Poly, v: usize, factors: &mut Vec<Vec<Z>>, m: &mut Meter) -> WorkResult<Vec<usize>> {
    if p.is_zero() || p.degree(v).unwrap_or(0) == 0 {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for (f, _) in factor(p, false, m)?.factors {
        let d = f.to_dense(v);
        let i = match factors.iter().position(|g| *g == d) {
            Some(i) => i,
            None => {
                factors.push(d);
                factors.len() - 1
            }
        };
        out.push(i);
    }
    Ok(out)
}

/// The j-th real root of the irreducible f (dense) as a term: a rational, a quadratic radical, or a root object.
fn root_value<'h>(
    env: &Env<'h>,
    x: T<'h>,
    f: &[Z],
    j: usize,
    ring: &mc_poly::convert::Ring<'h>,
    m: &mut Meter,
) -> WorkResult<T<'h>> {
    let a = Alg { env };
    let z = |c: &Z| env.ar.integer(&c.to_int());
    match f.len() - 1 {
        1 => Ok(a.neg(a.div(z(&f[0]), z(&f[1])))),
        2 => {
            // the factors have positive leading coefficients: the minus root is the smaller
            let rs = a.quadratic(z(&f[2]), z(&f[1]), z(&f[0]));
            Ok(rs[j])
        }
        _ => {
            let v = ring.var(x).expect("x is in the ring");
            let p = Poly::from_dense(ring.nvars(), v, f);
            let objs = crate::univariate::root_objects(env, &p, ring, v, m)?;
            Ok(objs[j])
        }
    }
}
