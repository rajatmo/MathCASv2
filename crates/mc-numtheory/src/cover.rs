//! Number functions added for coverage (package cov-num), defined by formulas (`mc_numeric::formula`) over the
//! natives of this package: divisibility and prime-factor counts (`divisible`, `is_coprime`, `is_square_free`,
//! `is_prime_power`, `is_composite`, `prime_nu`, `prime_omega`, `liouville_lambda`, `mangoldt_lambda`,
//! `carmichael_lambda`, `divisor_sum`, `kronecker_symbol`, `modular_inverse`, `quotient_remainder`), counting
//! (`catalan_number`, `stirling_s1`, `stirling_s2`, `partitions_q`, `partitions_p`, `ramanujan_tau`), bits
//! (`bit_and`, `bit_or`, `bit_xor`, `bit_not`), digits (`integer_reverse`, `real_digits`, `mixed_radix`) and
//! continued fractions (`continued_fraction`, `from_continued_fraction`).
//!
//! Decision cov-num.3: integer arguments beyond the word size go through the natives (`factor_integer`,
//! `divisors`, ...) as terms; the counting functions work on `Int`s and stop at 10^4 (larger stay unevaluated).
use mc_api::{Store, T};
use mc_arith::num::{Int, Nat, Rat};
use mc_arith::{Kind, Num, Real};
use mc_eval::Res;
use mc_numeric::formula::{Def, Fx};

type R<'h> = Res<Option<T<'h>>>;

macro_rules! def {
    ($name:literal, $ar:expr, $l:expr, $e:expr, $s:literal) => {
        Def { name: $name, arities: $ar, listable: $l, exact: $e, numeric: None, summary: $s }
    };
}

/// An exact integer argument (any size).
fn int<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>) -> Option<Int> {
    match fx.num(t)? {
        Num::Real(Real::Exact(q)) if q.is_int() => Some(q.num),
        _ => None,
    }
}

fn int_t<'h>(fx: &Fx<'_, '_, '_, 'h>, i: &Int) -> T<'h> {
    fx.integer(i)
}

fn ev<'h>(fx: &mut Fx<'_, '_, '_, 'h>, name: &str, args: &[T<'h>]) -> Res<T<'h>> {
    let c = fx.call(name, args);
    fx.eval(c)
}

/// The factorisation [(p, e)] of a non-zero integer (its absolute value) through `factor_integer`.
fn factors<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: T<'h>) -> Res<Option<Vec<(T<'h>, i64)>>> {
    let f = ev(fx, "factor_integer", &[fx.call("abs", &[n])])?;
    let Some(items) = fx.as_list(f) else { return Ok(None) };
    let mut out = vec![];
    for &it in items {
        let Some(&[p, e]) = fx.as_list(it) else { return Ok(None) };
        let Some(e) = fx.int_of(e) else { return Ok(None) };
        if fx.is_int(p, 1) {
            continue;
        }
        out.push((p, e));
    }
    Ok(Some(out))
}

/// A natural number that fits a word.
fn nat_u64(n: &Nat) -> Option<u64> {
    match n.limbs.as_slice() {
        [] => Some(0),
        [x] => Some(*x),
        _ => None,
    }
}

fn small<'h>(fx: &Fx<'_, '_, '_, 'h>, t: T<'h>, max: i64) -> Option<i64> {
    fx.int_of(t).filter(|v| v.abs() <= max)
}

// ---------------------------------------------------------------- divisibility and factor counts

fn divisible<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (n, m) = (a[0], a[1]);
    let (Some(x), Some(y)) = (fx.num(n), fx.num(m)) else { return Ok(None) };
    if !x.is_exact() || !y.is_exact() || y.is_zero() {
        return Ok(None);
    }
    // n/m an integer (Gaussian integers included)
    let q = fx.div(n, m);
    Ok(Some(match fx.num(q) {
        Some(Num::Real(Real::Exact(r))) => fx.bool(r.is_int()),
        Some(Num::Complex(Real::Exact(r), Real::Exact(s))) => fx.bool(r.is_int() && s.is_int()),
        _ => return Ok(None),
    }))
}

fn is_coprime<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if a.len() < 2 || a.iter().any(|&t| int(fx, t).is_none()) {
        return Ok(None);
    }
    for i in 0..a.len() {
        for j in i + 1..a.len() {
            let g = ev(fx, "gcd", &[a[i], a[j]])?;
            if !fx.is_int(g, 1) {
                return Ok(Some(fx.bool(false)));
            }
        }
    }
    Ok(Some(fx.bool(true)))
}

fn is_square_free<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else {
        // a rational is square free when numerator and denominator are
        if let Some(Num::Real(Real::Exact(_))) = fx.num(n) {
            let (p, q) = (ev(fx, "numerator", &[n])?, ev(fx, "denominator", &[n])?);
            let (Some(x), Some(y)) = (is_square_free(fx, &[p])?, is_square_free(fx, &[q])?) else { return Ok(None) };
            return Ok(Some(fx.bool(x == fx.bool(true) && y == fx.bool(true))));
        }
        return Ok(Some(fx.bool(false)));
    };
    if v.is_zero() {
        return Ok(Some(fx.bool(false)));
    }
    let Some(fs) = factors(fx, n)? else { return Ok(None) };
    Ok(Some(fx.bool(fs.iter().all(|&(_, e)| e == 1))))
}

fn is_prime_power<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else { return Ok(Some(fx.bool(false))) };
    if v.is_zero() {
        return Ok(Some(fx.bool(false)));
    }
    let Some(fs) = factors(fx, n)? else { return Ok(None) };
    Ok(Some(fx.bool(fs.len() == 1)))
}

fn is_composite<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(v) = int(fx, a[0]) else { return Ok(Some(fx.bool(false))) };
    if v.neg || v.cmp_int(&Int::from_i64(1)).is_le() {
        return Ok(Some(fx.bool(false)));
    }
    let p = ev(fx, "is_prime", &[a[0]])?;
    Ok(Some(fx.bool(p == fx.bool(false))))
}

fn prime_counts<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], which: u8) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else { return Ok(None) };
    if v.is_zero() {
        return Ok(None);
    }
    let Some(fs) = factors(fx, n)? else { return Ok(None) };
    let omega: i64 = fs.iter().map(|&(_, e)| e).sum();
    Ok(Some(match which {
        0 => fx.int(fs.len() as i64),
        1 => fx.int(omega),
        _ => fx.int(if omega % 2 == 0 { 1 } else { -1 }),
    }))
}
fn prime_nu<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    prime_counts(fx, a, 0)
}
fn prime_omega<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    prime_counts(fx, a, 1)
}
fn liouville_lambda<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    prime_counts(fx, a, 2)
}

fn mangoldt_lambda<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else { return Ok(None) };
    if v.neg || v.is_zero() {
        return Ok(None);
    }
    let Some(fs) = factors(fx, n)? else { return Ok(None) };
    Ok(Some(match fs.as_slice() {
        [(p, _)] => fx.call("log", &[*p]),
        _ => fx.int(0),
    }))
}

fn carmichael_lambda<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else { return Ok(None) };
    if v.is_zero() {
        return Ok(None);
    }
    let Some(fs) = factors(fx, n)? else { return Ok(None) };
    // lcm of lambda(p^e): phi(p^e), but 2^(e-2) for p = 2, e >= 3
    let mut parts = vec![fx.int(1)];
    for (p, e) in fs {
        let pe = fx.pow(p, fx.int(e));
        let l = if fx.is_int(p, 2) && e >= 3 { fx.pow(fx.int(2), fx.int(e - 2)) } else { ev(fx, "euler_phi", &[pe])? };
        parts.push(l);
    }
    Ok(Some(ev(fx, "lcm", &parts)?))
}

fn divisor_sum<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    let Some(v) = int(fx, n) else { return Ok(None) };
    if v.neg || v.is_zero() {
        return Ok(None);
    }
    let ds = ev(fx, "divisors", &[n])?;
    let Some(ds) = fx.as_list(ds) else { return Ok(None) };
    let mut terms = vec![];
    for &d in ds {
        if a.len() == 3 {
            let test = fx.h.app(a[2], &[d]);
            let c = fx.eval(test)?;
            if c != fx.bool(true) {
                continue;
            }
        }
        terms.push(fx.h.app(a[1], &[d]));
    }
    let s = fx.call("add", &terms);
    Ok(Some(if terms.is_empty() { fx.int(0) } else { s }))
}

fn kronecker_symbol<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let (Some(x), Some(n)) = (int(fx, a[0]), int(fx, a[1])) else { return Ok(None) };
    let (Some(x), Some(mut n)) = (x.to_i64(), n.to_i64()) else { return Ok(None) };
    if n == 0 {
        return Ok(Some(fx.int(i64::from(x == 1 || x == -1))));
    }
    let mut r: i64 = 1;
    if n < 0 {
        n = -n;
        if x < 0 {
            r = -r;
        }
    }
    // the factor 2^k of n: (a/2) = 0 for even a, (-1)^((a^2 - 1)/8) for odd a
    let k = n.trailing_zeros();
    n >>= k;
    if k > 0 {
        if x % 2 == 0 {
            return Ok(Some(fx.int(0)));
        }
        let m8 = x.rem_euclid(8);
        if k % 2 == 1 && (m8 == 3 || m8 == 5) {
            r = -r;
        }
    }
    if n == 1 {
        return Ok(Some(fx.int(r)));
    }
    let j = ev(fx, "jacobi_symbol", &[fx.int(x), fx.int(n)])?;
    Ok(Some(fx.mul(&[fx.int(r), j])))
}

fn modular_inverse<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if int(fx, a[0]).is_none() || int(fx, a[1]).is_none() {
        return Ok(None);
    }
    let v = ev(fx, "power_mod", &[a[0], fx.int(-1), a[1]])?;
    Ok(Some(v))
}

fn quotient_remainder<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if fx.num(a[0]).is_none() || fx.num(a[1]).is_none() {
        return Ok(None);
    }
    let q = fx.call("quotient", a);
    let r = fx.call("mod", a);
    Ok(Some(fx.list(&[q, r])))
}

// ---------------------------------------------------------------- counting

fn catalan_number<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    if let Some(k) = small(fx, n, 100_000) {
        if k < 0 {
            return Ok(Some(fx.int(if k == -1 { -1 } else { 0 })));
        }
        let b = ev(fx, "binomial", &[fx.int(2 * k), fx.int(k)])?;
        return Ok(Some(fx.div(b, fx.int(k + 1))));
    }
    if fx.num(n).is_some_and(|x| x.kind() != Kind::Exact) || fx.num(n).is_some() {
        let g = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| fx.call("gamma", &[t]);
        let num = fx.mul(&[fx.pow(fx.int(4), n), g(fx, fx.add(&[n, fx.rat(1, 2)]))]);
        let den = fx.mul(&[fx.sqrt(fx.sym("pi")), g(fx, fx.add(&[n, fx.int(2)]))]);
        return Ok(Some(fx.div(num, den)));
    }
    Ok(None)
}

const COUNT_MAX: i64 = 10_000;

/// Stirling numbers of the first (signed) or second kind by the row recurrence.
fn stirling<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], first: bool) -> R<'h> {
    let (Some(n), Some(k)) = (small(fx, a[0], COUNT_MAX), small(fx, a[1], COUNT_MAX)) else { return Ok(None) };
    if n < 0 || k < 0 {
        return Ok(None);
    }
    if k > n {
        return Ok(Some(fx.int(0)));
    }
    let (n, k) = (n as usize, k as usize);
    fx.tick((n * k / 64) as u64)?;
    let mut row = vec![Int::from_i64(0); k + 1];
    row[0] = Int::from_i64(1);
    for i in 1..=n {
        for j in (1..=k.min(i)).rev() {
            let t = if first {
                // s(i, j) = s(i-1, j-1) - (i-1) s(i-1, j)
                row[j - 1].sub(&row[j].mul(&Int::from_i64(i as i64 - 1)))
            } else {
                // S(i, j) = S(i-1, j-1) + j S(i-1, j)
                row[j - 1].add(&row[j].mul(&Int::from_i64(j as i64)))
            };
            row[j] = t;
        }
        row[0] = Int::from_i64(0);
    }
    Ok(Some(int_t(fx, &row[k])))
}
fn stirling_s1<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    stirling(fx, a, true)
}
fn stirling_s2<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    stirling(fx, a, false)
}

/// The number of partitions of n (`distinct` false) or into distinct parts, by the products' coefficients.
fn partitions<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], distinct: bool) -> R<'h> {
    let Some(n) = small(fx, a[0], COUNT_MAX) else { return Ok(None) };
    if n < 0 {
        return Ok(Some(fx.int(0)));
    }
    let n = n as usize;
    fx.tick((n * n / 4096) as u64)?;
    let mut c = vec![Int::from_i64(0); n + 1];
    c[0] = Int::from_i64(1);
    for part in 1..=n {
        if distinct {
            for m in (part..=n).rev() {
                let t = c[m].add(&c[m - part]);
                c[m] = t;
            }
        } else {
            for m in part..=n {
                let t = c[m].add(&c[m - part]);
                c[m] = t;
            }
        }
    }
    Ok(Some(int_t(fx, &c[n])))
}
fn partitions_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    partitions(fx, a, false)
}
fn partitions_q<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    partitions(fx, a, true)
}

/// tau(n): the coefficient of q^n in q prod (1 - q^k)^24.
fn ramanujan_tau<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(n) = small(fx, a[0], 2000) else { return Ok(None) };
    if n <= 0 {
        return Ok(Some(fx.int(0)));
    }
    let m = (n - 1) as usize;
    fx.tick((m * m / 256) as u64)?;
    // prod (1 - q^k) by Euler's pentagonal series, then its 24th power (truncated)
    let mut e = vec![Int::from_i64(0); m + 1];
    let mut k: i64 = 0;
    loop {
        let mut any = false;
        for g in [k * (3 * k - 1) / 2, k * (3 * k + 1) / 2] {
            if g as usize <= m {
                any = true;
                e[g as usize] = Int::from_i64(if k % 2 == 0 { 1 } else { -1 });
            }
            if k == 0 {
                break;
            }
        }
        if !any {
            break;
        }
        k += 1;
    }
    let mul = |x: &[Int], y: &[Int]| {
        let mut out = vec![Int::from_i64(0); m + 1];
        for (i, a) in x.iter().enumerate() {
            if a.is_zero() {
                continue;
            }
            for (j, b) in y.iter().enumerate().take(m + 1 - i) {
                if !b.is_zero() {
                    out[i + j] = out[i + j].add(&a.mul(b));
                }
            }
        }
        out
    };
    let e2 = mul(&e, &e);
    let e4 = mul(&e2, &e2);
    let e8 = mul(&e4, &e4);
    let e16 = mul(&e8, &e8);
    let e24 = mul(&e16, &e8);
    Ok(Some(int_t(fx, &e24[m])))
}

// ---------------------------------------------------------------- bits

fn bits<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], op: u8) -> R<'h> {
    let xs: Option<Vec<i128>> = a.iter().map(|&t| int(fx, t).and_then(|i| i.to_i64()).map(i128::from)).collect();
    let Some(xs) = xs else { return Ok(None) };
    let v = match op {
        0 => xs.iter().fold(-1i128, |acc, x| acc & x),
        1 => xs.iter().fold(0i128, |acc, x| acc | x),
        2 => xs.iter().fold(0i128, |acc, x| acc ^ x),
        _ => match xs.as_slice() {
            [x] => !x,
            _ => return Ok(None),
        },
    };
    Ok(Some(fx.integer(&Int::from_i64(v as i64))))
}
fn bit_and<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bits(fx, a, 0)
}
fn bit_or<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bits(fx, a, 1)
}
fn bit_xor<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bits(fx, a, 2)
}
fn bit_not<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    bits(fx, a, 3)
}

// ---------------------------------------------------------------- digits

fn integer_reverse<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    if int(fx, a[0]).is_none() {
        return Ok(None);
    }
    let b = a.get(1).copied().unwrap_or(fx.int(10));
    let ds = ev(fx, "integer_digits", &[a[0], b])?;
    let Some(items) = fx.as_list(ds) else { return Ok(None) };
    let rev: Vec<T<'h>> = items.iter().rev().copied().collect();
    let l = fx.list(&rev);
    Ok(Some(fx.call("from_digits", &[l, b])))
}

/// `mixed_radix(bases)` is inert: `integer_digits` and `from_digits` read it.
fn mixed_radix<'h>(_fx: &mut Fx<'_, '_, '_, 'h>, _a: &[T<'h>]) -> R<'h> {
    Ok(None)
}

/// The digits of an exact rational p/q >= 0 in base b: (prefix digits, repeating cycle, exponent), leading zeros
/// removed (decision cov-num.4: a cycle of zeros is no cycle).
fn rational_digits(p: &Nat, q: &Nat, b: u64) -> Option<(Vec<u64>, Vec<u64>, i64)> {
    let bn = Nat::from_u64(b);
    let (ip, mut r) = p.divrem(q);
    let mut prefix: Vec<u64> = vec![];
    let mut e: i64 = 0;
    if !ip.is_zero() {
        let mut v = ip;
        let mut ds = vec![];
        while !v.is_zero() {
            let (qq, d) = v.divrem(&bn);
            ds.push(nat_u64(&d).unwrap_or(0));
            v = qq;
        }
        ds.reverse();
        e = ds.len() as i64;
        prefix = ds;
    }
    // the fraction digits by long division, remembering remainders for the cycle
    let mut seen: std::collections::HashMap<Vec<u64>, usize> = std::collections::HashMap::new();
    let mut frac: Vec<u64> = vec![];
    let mut cycle_at = None;
    while !r.is_zero() {
        if let Some(&i) = seen.get(&r.limbs) {
            cycle_at = Some(i);
            break;
        }
        if frac.len() > 100_000 {
            return None;
        }
        seen.insert(r.limbs.clone(), frac.len());
        let (d, rr) = r.mul_u64(b).divrem(q);
        frac.push(nat_u64(&d).unwrap_or(0));
        r = rr;
    }
    let (mut head, mut cycle) = match cycle_at {
        Some(i) => (frac[..i].to_vec(), frac[i..].to_vec()),
        None => (frac, vec![]),
    };
    let mut pre = prefix;
    pre.append(&mut head);
    if pre.is_empty() && cycle.is_empty() {
        return Some((vec![0], vec![], 0));
    }
    // leading zeros: from the prefix, then by rotating the cycle
    if e == 0 {
        loop {
            match pre.first() {
                Some(0) => {
                    pre.remove(0);
                    e -= 1;
                }
                Some(_) => break,
                None => {
                    if cycle.iter().all(|&d| d == 0) {
                        break;
                    }
                    if cycle[0] != 0 {
                        break;
                    }
                    cycle.rotate_left(1);
                    e -= 1;
                }
            }
        }
    }
    Some((pre, cycle, e))
}

/// `real_digits(x)`, `real_digits(x, b)`, `real_digits(x, b, len)`: the digits of |x| in base b and the exponent
/// e with |x| = 0.d1 d2 ... * b^e; exact rationals give their repeating cycle as a final sublist, inexact numbers
/// their significant digits, other exact numbers need `len`.
fn real_digits<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    let b = match a.get(1) {
        Some(&t) => match fx.int_of(t).filter(|v| *v >= 2 && *v <= 1 << 30) {
            Some(v) => v as u64,
            None => return Ok(None),
        },
        None => 10,
    };
    let len = match a.get(2) {
        Some(&t) => Some(match fx.int_of(t).filter(|v| *v >= 0 && *v <= 1_000_000) {
            Some(v) => v as usize,
            None => return Ok(None),
        }),
        None => None,
    };
    let to_list = |fx: &Fx<'_, '_, '_, 'h>, ds: &[u64]| {
        let items: Vec<T<'h>> = ds.iter().map(|&d| fx.int(d as i64)).collect();
        fx.list(&items)
    };
    match fx.num(x) {
        Some(Num::Real(Real::Exact(q))) => {
            let (p, d) = (q.num.mag.clone(), q.den.clone());
            let Some((pre, cycle, e)) = rational_digits(&p, &d, b) else { return Ok(None) };
            if let Some(n) = len {
                // the first n digits, the cycle repeated, padded with zeros
                let mut ds = pre.clone();
                while ds.len() < n && !cycle.is_empty() {
                    ds.extend_from_slice(&cycle);
                }
                ds.resize(n, 0);
                return Ok(Some(fx.list(&[to_list(fx, &ds), fx.int(e)])));
            }
            let mut items: Vec<T<'h>> = pre.iter().map(|&d| fx.int(d as i64)).collect();
            if !cycle.is_empty() {
                items.push(to_list(fx, &cycle));
            }
            Ok(Some(fx.list(&[fx.list(&items), fx.int(e)])))
        }
        Some(Num::Real(r)) => {
            // an inexact real: its significant digits (machine reals 53 bits)
            let bits = match &r {
                Real::Machine(_) => 53,
                Real::Ball(_, p) => *p,
                _ => 53,
            };
            let n = len.unwrap_or(((bits as f64) / (b as f64).log2()).floor().max(1.0) as usize);
            digits_of_real(fx, x, b, n, bits)
        }
        Some(Num::Complex(..)) => Ok(None),
        None => {
            // an exact numeric expression: needs len
            let Some(n) = len else { return Ok(None) };
            let digits = (n as f64 * (b as f64).log10()).ceil() as i64 + 20;
            let v = ev(fx, "n", &[x, fx.int(digits)])?;
            if !matches!(fx.num(v), Some(Num::Real(_))) {
                return Ok(None);
            }
            digits_of_real(fx, v, b, n, (digits as f64 * 3.33) as u64)
        }
    }
}

/// The first n digits of a real number term (rounded to n digits for a machine real; truncated otherwise).
fn digits_of_real<'h>(fx: &mut Fx<'_, '_, '_, 'h>, x: T<'h>, b: u64, n: usize, _bits: u64) -> R<'h> {
    let Some(Num::Real(r)) = fx.num(x) else { return Ok(None) };
    // the exact value of the number: a rational
    let q: Rat = match &r {
        Real::Machine(v) => {
            if !v.is_finite() {
                return Ok(None);
            }
            if b == 10 {
                // the machine real's nearest n-digit decimal
                let s = format!("{:.*e}", n.saturating_sub(1), v.abs());
                let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
                let ds: Vec<u64> = m.chars().filter_map(|c| c.to_digit(10)).map(u64::from).collect();
                let e: i64 = e.parse().unwrap_or(0);
                if *v == 0.0 {
                    return Ok(Some(fx.list(&[fx.list(&vec![fx.int(0); n]), fx.int(0)])));
                }
                let items: Vec<T<'h>> = ds.iter().map(|&d| fx.int(d as i64)).collect();
                return Ok(Some(fx.list(&[fx.list(&items), fx.int(e + 1)])));
            }
            let Some(q) = fx.rat_of(fx.call("rationalize", &[x, fx.int(0)])) else {
                let ex = fx.eval(fx.call("set_precision", &[x, fx.sym("inf")]))?;
                match fx.rat_of(ex) {
                    Some(q) => return digits_exact(fx, &q, b, n),
                    None => return Ok(None),
                }
            };
            q
        }
        _ => {
            let ex = fx.eval(fx.call("set_precision", &[x, fx.sym("inf")]))?;
            match fx.rat_of(ex) {
                Some(q) => q,
                None => return Ok(None),
            }
        }
    };
    digits_exact(fx, &q, b, n)
}

fn digits_exact<'h>(fx: &mut Fx<'_, '_, '_, 'h>, q: &Rat, b: u64, n: usize) -> R<'h> {
    let Some((pre, cycle, e)) = rational_digits(&q.num.mag, &q.den, b) else { return Ok(None) };
    let mut ds = pre;
    while ds.len() < n && !cycle.is_empty() {
        ds.extend_from_slice(&cycle);
    }
    ds.resize(n, 0);
    let items: Vec<T<'h>> = ds.iter().map(|&d| fx.int(d as i64)).collect();
    Ok(Some(fx.list(&[fx.list(&items), fx.int(e)])))
}

// ---------------------------------------------------------------- continued fractions

/// x as (a + b sqrt(d))/c with integers, b != 0, d > 1 not a square, from a canonical sum of a rational and a
/// rational multiple of a square root of an integer.
fn quadratic<'h>(fx: &Fx<'_, '_, '_, 'h>, x: T<'h>) -> Option<(i128, i128, i128, i128)> {
    let terms: Vec<T<'h>> = match fx.call_of(x, "add") {
        Some(ts) => ts.to_vec(),
        None => vec![x],
    };
    let mut rat = Rat::from_i64(0);
    let mut root: Option<(Rat, i128)> = None;
    for t in terms {
        if let Some(q) = fx.rat_of(t) {
            rat = rat.add(&q);
            continue;
        }
        let (coef, s) = match fx.call_of(t, "mul") {
            Some(&[c, s]) => (fx.rat_of(c)?, s),
            None => (Rat::from_i64(1), t),
            _ => return None,
        };
        let &[d, e] = fx.call_of(s, "pow")? else { return None };
        let half = Rat::new(Int::from_i64(1), Int::from_i64(2));
        let ev = fx.rat_of(e)?;
        let dd = fx.int_of(d)? as i128;
        if root.is_some() || dd <= 1 {
            return None;
        }
        let coef = if ev == half {
            coef
        } else if ev == half.neg() {
            // d^(-1/2) = d^(1/2)/d
            coef.mul(&Rat::new(Int::from_i64(1), Int::from_i64(dd as i64)))
        } else {
            return None;
        };
        root = Some((coef, dd));
    }
    let (bq, d) = root?;
    // common denominator
    let (rd, bd) = (nat_u64(&rat.den)? as i128, nat_u64(&bq.den)? as i128);
    let c = lcm_i(rd, bd)?;
    let a = rat.num.to_i64()? as i128 * (c / rd);
    let b = bq.num.to_i64()? as i128 * (c / bd);
    Some((a, b, c, d))
}

fn gcd_i(mut a: i128, mut b: i128) -> i128 {
    a = a.abs();
    b = b.abs();
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn lcm_i(a: i128, b: i128) -> Option<i128> {
    (a / gcd_i(a, b)).checked_mul(b)
}

fn isqrt(n: i128) -> i128 {
    let mut x = (n as f64).sqrt() as i128;
    while x * x > n {
        x -= 1;
    }
    while (x + 1) * (x + 1) <= n {
        x += 1;
    }
    x
}

/// The periodic continued fraction of (a + b sqrt(d))/c: the pre-period terms and the period (PQa algorithm).
fn quadratic_cf(a: i128, b: i128, c: i128, d: i128) -> Option<(Vec<i128>, Vec<i128>)> {
    // (P + sqrt(D))/Q with Q | D - P^2
    let (mut p, mut q, dd) = if b > 0 { (a, c, b.checked_mul(b)?.checked_mul(d)?) } else { (-a, -c, b.checked_mul(b)?.checked_mul(d)?) };
    let (mut p2, mut q2, mut d2) = (p, q, dd);
    if (d2 - p2 * p2) % q2 != 0 {
        p2 = p2.checked_mul(q2.abs())?;
        d2 = d2.checked_mul(q2.checked_mul(q2)?)?;
        q2 = q2.checked_mul(q2.abs())?;
    }
    (p, q) = (p2, q2);
    let dd = d2;
    let s = isqrt(dd);
    let mut seen: std::collections::HashMap<(i128, i128), usize> = std::collections::HashMap::new();
    let mut terms = vec![];
    for _ in 0..10_000 {
        if let Some(&i) = seen.get(&(p, q)) {
            return Some((terms[..i].to_vec(), terms[i..].to_vec()));
        }
        seen.insert((p, q), terms.len());
        let num = if q > 0 { p + s } else { p + s + 1 };
        let t = num.div_euclid(q);
        let t = if q < 0 && num.rem_euclid(q) != 0 { (num as f64 / q as f64).floor() as i128 } else { t };
        terms.push(t);
        p = t * q - p;
        q = (dd - p * p) / q;
        if q == 0 {
            return None;
        }
    }
    None
}

fn continued_fraction<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let x = a[0];
    let limit = match a.get(1) {
        Some(&t) => match fx.int_of(t).filter(|v| *v >= 0 && *v <= 100_000) {
            Some(v) => Some(v as usize),
            None => return Ok(None),
        },
        None => None,
    };
    let as_list = |fx: &Fx<'_, '_, '_, 'h>, ts: &[Int]| {
        let items: Vec<T<'h>> = ts.iter().map(|t| fx.integer(t)).collect();
        fx.list(&items)
    };
    // exact rationals: Euclid's algorithm
    if let Some(q) = fx.rat_of(x) {
        let mut terms = vec![];
        let (mut n, mut d) = (q.num, q.den);
        while !d.is_zero() && limit.is_none_or(|l| terms.len() < l) {
            let (t, r) = n.divrem_floor(&d);
            terms.push(t);
            n = Int::from_nat(d);
            d = r;
        }
        return Ok(Some(as_list(fx, &terms)));
    }
    // quadratic irrationals: the periodic form, or its first terms
    if let Some((qa, qb, qc, qd)) = quadratic(fx, x) {
        if let Some((pre, period)) = quadratic_cf(qa, qb, qc, qd) {
            let to_t = |fx: &Fx<'_, '_, '_, 'h>, v: i128| fx.integer(&Int::from_i64(v as i64));
            if let Some(l) = limit {
                let mut out = vec![];
                let mut i = 0;
                while out.len() < l {
                    let v = if i < pre.len() { pre[i] } else { period[(i - pre.len()) % period.len()] };
                    out.push(to_t(fx, v));
                    i += 1;
                }
                return Ok(Some(fx.list(&out)));
            }
            let mut items: Vec<T<'h>> = pre.iter().map(|&v| to_t(fx, v)).collect();
            let per: Vec<T<'h>> = period.iter().map(|&v| to_t(fx, v)).collect();
            items.push(fx.list(&per));
            return Ok(Some(fx.list(&items)));
        }
    }
    // other reals: the terms of a precise enough value (machine reals: of their exact value, stopping where it
    // ends or the digits run out)
    let num = fx.num(x);
    let machine = matches!(num, Some(Num::Real(Real::Machine(_))));
    let l = match (limit, machine) {
        (Some(l), _) => l,
        (None, true) => 20,
        (None, false) => return Ok(None),
    };
    let digits = (l as i64) * 2 + 30;
    let v = if machine { x } else { ev(fx, "n", &[x, fx.int(digits)])? };
    let Some(Num::Real(r)) = fx.num(v) else { return Ok(None) };
    let (q, bits) = match &r {
        Real::Machine(f) => {
            let ex = fx.eval(fx.call("set_precision", &[v, fx.sym("inf")]))?;
            let Some(q) = fx.rat_of(ex) else { return Ok(None) };
            let _ = f;
            (q, 53.0)
        }
        Real::Ball(_, p) => {
            let ex = fx.eval(fx.call("set_precision", &[v, fx.sym("inf")]))?;
            let Some(q) = fx.rat_of(ex) else { return Ok(None) };
            (q, *p as f64)
        }
        Real::Exact(q) => (q.clone(), f64::INFINITY),
    };
    // terms while the convergents' denominators stay well inside the precision
    let mut terms = vec![];
    let (mut n, mut d) = (q.num, q.den);
    let (mut k0, mut k1) = (Int::from_i64(0), Int::from_i64(1));
    while !d.is_zero() && terms.len() < l {
        let (t, r) = n.divrem_floor(&d);
        let k2 = t.mul(&k1).add(&k0);
        if machine || bits.is_finite() {
            let lim = if machine { 26.0 } else { bits / 2.0 - 4.0 };
            if (k2.mag.bits() as f64) > lim && limit.is_none() {
                break;
            }
        }
        terms.push(t);
        k0 = k1;
        k1 = k2;
        n = Int::from_nat(d);
        d = r;
    }
    Ok(Some(as_list(fx, &terms)))
}

fn from_continued_fraction<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(items) = fx.as_list(a[0]) else { return Ok(None) };
    if items.is_empty() {
        return Ok(None);
    }
    let (prefix, period) = match fx.as_list(items[items.len() - 1]) {
        Some(p) => (&items[..items.len() - 1], Some(p)),
        None => (items, None),
    };
    // the tail value: the periodic part's fixed point, or the last term
    let one = fx.int(1);
    let mut v = match period {
        Some(per) if !per.is_empty() => {
            // y = [b0; b1, ..., y]: y = (p y + p')/(q y + q') with the convergent matrix of the period
            let (mut p, mut pp, mut q, mut qq) = (one, fx.int(0), fx.int(0), one);
            for &b in per {
                // multiply by [[b, 1], [1, 0]]
                let np = fx.add(&[fx.mul(&[p, b]), pp]);
                let nq = fx.add(&[fx.mul(&[q, b]), qq]);
                pp = p;
                qq = q;
                p = np;
                q = nq;
            }
            // q y^2 + (q' - p) y - p' = 0, the positive root
            let bq = fx.sub(qq, p);
            let disc = fx.add(&[fx.mul(&[bq, bq]), fx.mul(&[fx.int(4), q, pp])]);
            let y = fx.div(fx.add(&[fx.neg(bq), fx.sqrt(disc)]), fx.mul(&[fx.int(2), q]));
            Some(y)
        }
        _ => None,
    };
    for &t in prefix.iter().rev() {
        v = Some(match v {
            None => t,
            Some(y) => fx.add(&[t, fx.div(one, y)]),
        });
    }
    let Some(v) = v else { return Ok(None) };
    if period.is_some() {
        return Ok(Some(fx.eval(fx.call("simplify", &[v]))?));
    }
    Ok(Some(v))
}

/// The definitions of this module.
pub static DEFS: &[Def] = &[
    def!("divisible", &[2], true, Some(divisible), "divisible(n, m): true when n/m is an integer (Gaussian integers included)"),
    def!("is_coprime", &[2, 3, 4, 5, 6, 7, 8], true, Some(is_coprime), "is_coprime(a, b, ...): true when the integers are pairwise coprime"),
    def!("is_square_free", &[1], true, Some(is_square_free), "is_square_free(n): true when no square of a prime divides n"),
    def!("is_prime_power", &[1], true, Some(is_prime_power), "is_prime_power(n): true when |n| is a power p^k of a prime, k >= 1"),
    def!("is_composite", &[1], true, Some(is_composite), "is_composite(n): true for integers n > 1 that are not prime"),
    def!("prime_nu", &[1], true, Some(prime_nu), "prime_nu(n): the number of distinct primes dividing n"),
    def!("prime_omega", &[1], true, Some(prime_omega), "prime_omega(n): the number of prime factors of n counted with multiplicity"),
    def!("liouville_lambda", &[1], true, Some(liouville_lambda), "liouville_lambda(n): (-1)^prime_omega(n)"),
    def!("mangoldt_lambda", &[1], true, Some(mangoldt_lambda), "mangoldt_lambda(n): log p when n is a power of the prime p, else 0"),
    def!("carmichael_lambda", &[1], true, Some(carmichael_lambda), "carmichael_lambda(n): the exponent of the multiplicative group modulo n"),
    def!("divisor_sum", &[2, 3], false, Some(divisor_sum), "divisor_sum(n, f): the sum of f(d) over the divisors d of n; divisor_sum(n, f, cond) over those with cond(d) true"),
    def!("kronecker_symbol", &[2], true, Some(kronecker_symbol), "kronecker_symbol(a, n): the Kronecker symbol, the Jacobi symbol extended to all integers n"),
    def!("modular_inverse", &[2], true, Some(modular_inverse), "modular_inverse(k, n): the r with k r = 1 mod n"),
    def!("quotient_remainder", &[2], true, Some(quotient_remainder), "quotient_remainder(m, n): [quotient(m, n), mod(m, n)]"),
    def!("catalan_number", &[1], true, Some(catalan_number), "catalan_number(n): the Catalan number binomial(2 n, n)/(n + 1)"),
    def!("stirling_s1", &[2], true, Some(stirling_s1), "stirling_s1(n, k): the signed Stirling number of the first kind"),
    def!("stirling_s2", &[2], true, Some(stirling_s2), "stirling_s2(n, k): the Stirling number of the second kind"),
    def!("partitions_p", &[1], true, Some(partitions_p), "partitions_p(n): the number of partitions of n"),
    def!("partitions_q", &[1], true, Some(partitions_q), "partitions_q(n): the number of partitions of n into distinct parts"),
    def!("ramanujan_tau", &[1], true, Some(ramanujan_tau), "ramanujan_tau(n): the Ramanujan tau function, the coefficients of q prod (1 - q^k)^24"),
    def!("bit_and", &[1, 2, 3, 4, 5, 6, 7, 8], true, Some(bit_and), "bit_and(n1, n2, ...): the bitwise and of integers (two's complement)"),
    def!("bit_or", &[1, 2, 3, 4, 5, 6, 7, 8], true, Some(bit_or), "bit_or(n1, n2, ...): the bitwise or of integers"),
    def!("bit_xor", &[1, 2, 3, 4, 5, 6, 7, 8], true, Some(bit_xor), "bit_xor(n1, n2, ...): the bitwise exclusive or of integers"),
    def!("bit_not", &[1], true, Some(bit_not), "bit_not(n): the bitwise complement -1 - n"),
    def!("integer_reverse", &[1, 2], true, Some(integer_reverse), "integer_reverse(n, b): the integer with the base-b digits of n reversed (b = 10 by default)"),
    def!("mixed_radix", &[1], false, Some(mixed_radix), "mixed_radix([b1, ..., bk]): a mixed-radix base for integer_digits and from_digits"),
    def!("real_digits", &[1, 2, 3], false, Some(real_digits), "real_digits(x), real_digits(x, b), real_digits(x, b, len): [digits, e] with |x| = 0.d1 d2 ... b^e; a rational's repeating digits form a final sublist"),
    def!("continued_fraction", &[1, 2], false, Some(continued_fraction), "continued_fraction(x), continued_fraction(x, n): the continued fraction terms of x (a quadratic irrational's period as a final sublist)"),
    def!("from_continued_fraction", &[1], false, Some(from_continued_fraction), "from_continued_fraction(terms): the number with these continued fraction terms (a final sublist repeats)"),
];
