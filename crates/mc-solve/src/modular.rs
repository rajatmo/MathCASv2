//! Solving modulo an integer (`solve(eqs, xs, modulus: n)`, `roots(eq, x, modulus: n)`; decision 11C.8).
//!
//! The equations are read as polynomials with integer coefficients (a rational coefficient needs a denominator
//! prime to n) and every point of `(Z/n)^k` is tried, for at most [`MAX_POINTS`] points: exact and complete for any
//! modulus, prime or not (`x^4 - 2x^3 + 5x - 10` has the four roots 2, 7, 10, 11 modulo 12). The solutions are the
//! residues in `[0, n)`, in increasing order (the last unknown fastest).
use crate::cx::{Env, Out};
use crate::natives::{equations_and_unknowns, SolveFn};
use mc_api::{names, Store, View, T};
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;

/// The most points tried.
pub const MAX_POINTS: u64 = 1_000_000;

/// `f(eqs, xs, modulus: n)` for `solve` and `roots`.
pub(crate) fn solve_mod<'h>(env: &Env<'h>, f: SolveFn, args: &[T<'h>], n: T<'h>, m: &mut Meter) -> WorkResult<Out<'h>> {
    let h = env.h();
    let View::Int(p) = h.view(n) else { return Ok(env.arg_failure("the modulus must be an integer")) };
    let p = p as u64;
    let (eqs, xs) = match equations_and_unknowns(env, args, m)? {
        Ok(v) => v,
        Err(out) => return Ok(out),
    };
    let ring = env.ring(&eqs, &xs, m)?;
    let vars: Vec<usize> = xs.iter().map(|&x| ring.var(x).expect("the unknowns are in the ring")).collect();
    if ring.kernels.iter().enumerate().any(|(i, k)| !vars.contains(&i) && env.ar.num(*k).is_none()) {
        return Ok(env.arg_failure("modular equations must be polynomials with integer coefficients"));
    }
    let rc = env.rconv();
    let mut polys: Vec<Poly> = vec![];
    for &e in &eqs {
        let Ok(fr) = ratfun::read(&rc, e, &ring, m)? else { return Ok(env.arg_failure("an equation divides by zero")) };
        if !fr.den.is_constant() || fr.den.as_constant().is_some_and(|d| gcd(d.rem_u64(p), p) != 1) {
            return Ok(env.arg_failure("modular equations must be polynomials with integer coefficients"));
        }
        polys.push(fr.num);
    }
    // numeric kernels other than the unknowns: integers only
    let mut base: Vec<u64> = vec![0; ring.nvars()];
    for (i, &k) in ring.kernels.iter().enumerate() {
        if vars.contains(&i) {
            continue;
        }
        match env.ar.num(k).and_then(|x| x.as_int().cloned()) {
            Some(z) => base[i] = mc_poly::Z::from_int(z).rem_u64(p),
            None => return Ok(env.arg_failure("modular equations must be polynomials with integer coefficients")),
        }
    }
    let k = vars.len() as u32;
    let total = (p as u128).checked_pow(k).filter(|&t| t <= MAX_POINTS as u128);
    let Some(total) = total else { return Ok(env.failure("unsolved", "too many residues to try")) };
    let mut sols: Vec<Vec<u64>> = vec![];
    let mut point = base.clone();
    for idx in 0..total as u64 {
        m.step(polys.len() as u64 + 1)?;
        let mut r = idx;
        for &v in vars.iter().rev() {
            point[v] = r % p;
            r /= p;
        }
        if polys.iter().all(|q| eval_mod(q, &point, p) == 0) {
            sols.push(vars.iter().map(|&v| point[v]).collect());
        }
    }
    let rule = h.sym(names::RULE);
    match f {
        SolveFn::Roots => {
            let x = xs[0];
            let eqs: Vec<T<'h>> = sols.iter().map(|s| env.call("equal", &[x, env.int(s[0] as i64)])).collect();
            Ok(Out::Done(match eqs.as_slice() {
                [] => env.sym(names::FALSE),
                [one] => *one,
                _ => env.call("or", &eqs),
            }))
        }
        _ => {
            let items: Vec<T<'h>> = sols
                .iter()
                .map(|s| env.list(&xs.iter().zip(s).map(|(&x, &v)| h.call(rule, &[x, env.int(v as i64)])).collect::<Vec<_>>()))
                .collect();
            Ok(Out::Done(env.list(&items)))
        }
    }
}

/// q at the point, modulo p.
fn eval_mod(q: &Poly, point: &[u64], p: u64) -> u64 {
    let mut acc: u128 = 0;
    for (e, c) in q.terms() {
        let mut t = c.rem_u64(p) as u128;
        for (i, &k) in e.iter().enumerate() {
            if k > 0 {
                t = t * pow_mod(point[i], k as u64, p) as u128 % p as u128;
            }
        }
        acc = (acc + t) % p as u128;
    }
    acc as u64
}

fn pow_mod(b: u64, mut e: u64, p: u64) -> u64 {
    let (mut r, mut b) = (1u128 % p as u128, b as u128 % p as u128);
    while e > 0 {
        if e & 1 == 1 {
            r = r * b % p as u128;
        }
        b = b * b % p as u128;
        e >>= 1;
    }
    r as u64
}

fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}
