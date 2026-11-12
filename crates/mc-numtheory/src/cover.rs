//! Number functions added for coverage, defined by formulas (`mc_numeric::formula`) over the
//! natives of this crate: divisibility and prime-factor counts (`divisible`, `is_coprime`, `is_square_free`,
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
    fx.int_of(t).filter(|v| v.unsigned_abs() <= max as u64)
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
    if a.len() > 1 || fx.num(n).is_none() {
        return is_square_free_poly(fx, a);
    }
    let Some(v) = int(fx, n) else {
        // a Gaussian integer with b != 0
        let im = ev(fx, "im_part", &[n])?;
        if int(fx, im).is_some_and(|y| !y.is_zero()) {
            return gaussian_square_free(fx, n);
        }
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

/// `is_square_free(p)`, `is_square_free(p, x)` (or a list of variables), with `modulus: m` as the last argument: a
/// polynomial is square free when no square of a nonconstant polynomial divides it, that is when
/// `gcd(p, d(p, x))` is free of x for each variable x (over `F_m` with the modulus, where a zero derivative makes
/// the gcd p itself: a p-th power). The integer content is not looked at. A term that is not a polynomial in its
/// symbols (`sin(x)`) is square free, as a kernel of its own.
/// Whether a Gaussian integer z = a + b i is square free in Z[i], from N = a^2 + b^2 = prod p^e: p = 2 (the prime
/// 1 + i, ramified) needs e <= 1; p = 3 mod 4 (inert, e even) e <= 2; p = 1 mod 4 (split into two conjugates)
/// e <= 1, or e = 2 with p dividing z (each conjugate once). Units are square free, 0 is not.
fn gaussian_square_free<'h>(fx: &mut Fx<'_, '_, '_, 'h>, z: T<'h>) -> R<'h> {
    let (re, im) = (ev(fx, "re", &[z])?, ev(fx, "im_part", &[z])?);
    if int(fx, re).is_none() || int(fx, im).is_none() {
        return Ok(Some(fx.bool(false)));
    }
    let norm = fx.eval(fx.add(&[fx.mul(&[re, re]), fx.mul(&[im, im])]))?;
    if norm == fx.int(0) {
        return Ok(Some(fx.bool(false)));
    }
    if norm == fx.int(1) {
        return Ok(Some(fx.bool(true)));
    }
    let Some(fs) = factors(fx, norm)? else { return Ok(None) };
    for (p, e) in fs {
        let r = ev(fx, "mod", &[p, fx.int(4)])?;
        let m = int(fx, r).and_then(|r| r.to_i64());
        let ok = match m {
            Some(2) => e <= 1,
            Some(3) => e <= 2,
            Some(1) => {
                e <= 1 || e == 2 && {
                    let (ra, rb) = (ev(fx, "mod", &[re, p])?, ev(fx, "mod", &[im, p])?);
                    ra == fx.int(0) && rb == fx.int(0)
                }
            }
            _ => false,
        };
        if !ok {
            return Ok(Some(fx.bool(false)));
        }
    }
    Ok(Some(fx.bool(true)))
}

fn is_square_free_poly<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let p = a[0];
    let mut opts = None;
    let mut vars = None;
    for &t in &a[1..] {
        match fx.h.view(t) {
            mc_api::View::App { head, .. } if head == fx.h.sym_term(fx.h.sym("options")) => opts = Some(t),
            _ => vars = Some(t),
        }
    }
    if let Some(n) = fx.num(p) {
        // gaussian_integers: true: square free in Z[i]
        if let Some(o) = opts {
            if let mc_api::View::App { args: rules, .. } = fx.h.view(o) {
                let key = fx.h.sym_term(fx.h.sym("gaussian_integers"));
                let on = rules.iter().any(|&r| matches!(fx.h.view(r), mc_api::View::App { args: &[k, v], .. } if k == key && v == fx.bool(true)));
                if on {
                    return gaussian_square_free(fx, p);
                }
            }
        }
        // a number with options or variables: the number's answer (with a modulus every nonzero number is a unit)
        if opts.is_some() {
            return Ok(Some(fx.bool(!n.is_zero())));
        }
        return is_square_free(fx, &[p]);
    }
    let vars: Vec<T<'h>> = match vars {
        Some(v) => fx.as_list(v).map_or_else(|| vec![v], <[T<'h>]>::to_vec),
        None => {
            let vs = ev(fx, "variables", &[p])?;
            fx.as_list(vs).unwrap_or(&[]).iter().copied().filter(|&v| matches!(fx.h.view(v), mc_api::View::Sym(_))).collect()
        }
    };
    if vars.is_empty() {
        return Ok(Some(fx.bool(true)));
    }
    for x in vars {
        let dp = ev(fx, "d", &[p, x])?;
        let mut args = vec![p, dp];
        args.extend(opts);
        let g = ev(fx, "polynomial_gcd", &args)?;
        let deg = ev(fx, "degree", &[g, x])?;
        match fx.int_of(deg) {
            Some(0) => {}
            Some(k) if k > 0 => return Ok(Some(fx.bool(false))),
            // p free of x (gcd 0 has degree -inf), or not a polynomial
            _ => {}
        }
    }
    Ok(Some(fx.bool(true)))
}

/// The least prime factor of `n >= 2` by trial division, and whether `n` is a power of it.
fn least_prime_power(n: u64) -> (u64, bool) {
    let p = if n.is_multiple_of(2) {
        2
    } else {
        let mut d = 3;
        while d * d <= n && !n.is_multiple_of(d) {
            d += 2;
        }
        if d * d > n {
            n
        } else {
            d
        }
    };
    let mut m = n;
    while m.is_multiple_of(p) {
        m /= p;
    }
    (p, m == 1)
}

/// [`gaussian_prime_power`] in machine integers for a Gaussian integer of norm below 10^12 (the same rules; the
/// least prime factor of the norm by trial division), `None` for other numbers.
fn small_gaussian_prime_power(z: &Num) -> Option<bool> {
    let Num::Complex(Real::Exact(a), Real::Exact(b)) = z else { return None };
    if !a.is_int() || !b.is_int() {
        return None;
    }
    let (x, y) = (a.num.to_i64()?, b.num.to_i64()?);
    if x.unsigned_abs() > 1_000_000 || y.unsigned_abs() > 1_000_000 || y == 0 {
        return None;
    }
    let (ux, uy) = (x.unsigned_abs(), y.unsigned_abs());
    if x == 0 {
        if uy == 1 {
            return Some(false);
        }
        let (p, power) = least_prime_power(uy);
        return Some(power && (p == 2 || p % 4 == 3));
    }
    let norm = ux * ux + uy * uy;
    if norm == 1 {
        return Some(false);
    }
    let (p, power) = least_prime_power(norm);
    Some(power && (p == 2 || p % 4 == 1 && !(ux % p == 0 && uy % p == 0)))
}

fn is_prime_power<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let n = a[0];
    if let Some(b) = fx.num(n).as_ref().and_then(small_gaussian_prime_power) {
        return Ok(Some(fx.bool(b)));
    }
    let Some(v) = int(fx, n) else {
        // a rational 1/q is the power p^-k when q = p^k
        let (nu, de) = (ev(fx, "numerator", &[n])?, ev(fx, "denominator", &[n])?);
        if let (Some(u), Some(d)) = (int(fx, nu), int(fx, de)) {
            if (u == Int::from_i64(1) || u == Int::from_i64(-1)) && !d.is_zero() {
                return is_prime_power(fx, &[de]);
            }
            return Ok(Some(fx.bool(false)));
        }
        return gaussian_prime_power(fx, n);
    };
    if v.is_zero() {
        return Ok(Some(fx.bool(false)));
    }
    // n = r^k with k the largest exponent for which n is a perfect power: the root r is no power, so n is a prime
    // power exactly when r is prime (no factoring: one integer root per exponent, Newton's iteration)
    let mag = v.mag.clone();
    if mag == Nat::from_u64(1) {
        return Ok(Some(fx.bool(false)));
    }
    for k in (1..=mag.bits()).rev() {
        fx.tick(1)?;
        let r = mag.iroot(k);
        if r.pow_u64(k) == mag {
            let root = fx.integer(&Int::from_nat(r));
            return ev(fx, "is_prime", &[root]).map(Some);
        }
    }
    Ok(Some(fx.bool(false)))
}

/// Whether a Gaussian integer z = a + b i (b != 0) is a unit times a power of one Gaussian prime. With N = a^2 +
/// b^2: an associate of a rational integer n (a = 0) is one when n is 2^k or q^k with q = 3 mod 4 (p = 1 mod 4
/// splits into two primes); otherwise N must be p^k, and for p = 1 mod 4 z must not be divisible by p (else both
/// conjugate primes divide it). Units are not prime powers.
fn gaussian_prime_power<'h>(fx: &mut Fx<'_, '_, '_, 'h>, z: T<'h>) -> R<'h> {
    let (re, im) = (ev(fx, "re", &[z])?, ev(fx, "im_part", &[z])?);
    let (Some(x), Some(y)) = (int(fx, re), int(fx, im)) else { return Ok(Some(fx.bool(false))) };
    if y.is_zero() {
        return Ok(Some(fx.bool(false)));
    }
    let mod4 = |fx: &mut Fx<'_, '_, '_, 'h>, p: T<'h>| -> Res<Option<i64>> {
        let r = ev(fx, "mod", &[p, fx.int(4)])?;
        Ok(int(fx, r).and_then(|r| r.to_i64()))
    };
    if x.is_zero() {
        // i y: the rational integer |y|
        let Some(fs) = factors(fx, im)? else { return Ok(None) };
        let ok = match fs.as_slice() {
            [(p, _)] => matches!(mod4(fx, *p)?, Some(2 | 3)),
            _ => false,
        };
        return Ok(Some(fx.bool(ok)));
    }
    let norm = fx.eval(fx.add(&[fx.mul(&[re, re]), fx.mul(&[im, im])]))?;
    if norm == fx.int(1) {
        return Ok(Some(fx.bool(false)));
    }
    let Some(fs) = factors(fx, norm)? else { return Ok(None) };
    let [(p, _)] = fs.as_slice() else { return Ok(Some(fx.bool(false))) };
    let p = *p;
    let ok = match mod4(fx, p)? {
        Some(2) => true,
        Some(1) => {
            let (ra, rb) = (ev(fx, "mod", &[re, p])?, ev(fx, "mod", &[im, p])?);
            !(ra == fx.int(0) && rb == fx.int(0))
        }
        _ => false,
    };
    Ok(Some(fx.bool(ok)))
}

fn is_composite<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(v) = int(fx, a[0]) else {
        // a Gaussian integer a + b i (b != 0): composite when it is not a unit (norm 1) and not a Gaussian prime
        let (re, im) = (ev(fx, "re", &[a[0]])?, ev(fx, "im_part", &[a[0]])?);
        if let (Some(x), Some(y)) = (int(fx, re), int(fx, im)) {
            if !y.is_zero() {
                let norm = x.mul(&x).add(&y.mul(&y));
                if norm.cmp_int(&Int::from_i64(1)).is_le() {
                    return Ok(Some(fx.bool(false)));
                }
                let p = ev(fx, "is_prime", &[a[0]])?;
                return Ok(Some(fx.bool(p == fx.bool(false))));
            }
        }
        return Ok(Some(fx.bool(false)));
    };
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
    // a list of integers: one sum each
    if let Some(items) = fx.as_list(n).map(<[T<'h>]>::to_vec) {
        let out: Vec<T<'h>> = items
            .iter()
            .map(|&x| {
                let mut b = a.to_vec();
                b[0] = x;
                fx.call("divisor_sum", &b)
            })
            .collect();
        return Ok(Some(fx.list(&out)));
    }
    let Some(v) = int(fx, n) else {
        if a.len() == 2 && fx.num(n).is_none() {
            return divisor_sum_symbolic(fx, n, a[1]);
        }
        return Ok(None);
    };
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

/// The sum of f(d) over the divisors d of a symbolic n, term by term (f(d) a sum of constant multiples of known
/// arithmetic functions of d), by the classical divisor-sum identities (Apostol, Introduction to Analytic Number
/// Theory, theorems 2.1, 2.2, 2.10, 2.17 and exercise 2.6; Hardy and Wright 16.7): sum 1 = sigma_0(n), sum d^k =
/// sigma_k(n), sum mu(d) = [n = 1], sum mu(d)^2 = 2^omega(n), sum mu(d)/d = phi(n)/n, sum phi(d) = n,
/// sum Lambda(d) = log(n), sum mu(n/d) log(d) = Lambda(n). Any other term declines.
fn divisor_sum_symbolic<'h>(fx: &mut Fx<'_, '_, '_, 'h>, n: T<'h>, f: T<'h>) -> R<'h> {
    if fx.as_list(n).is_some() {
        return Ok(None);
    }
    let d = fx.sym("divisor_sum·d");
    let body = fx.eval(fx.h.app(f, &[d]))?;
    let body = fx.eval(fx.call("expand", &[body]))?;
    let terms: Vec<T<'h>> = match fx.call_of(body, "add") {
        Some(ts) => ts.to_vec(),
        None => vec![body],
    };
    fx.tick(terms.len() as u64 + 1)?;
    let free = |fx: &Fx<'_, '_, '_, 'h>, t: T<'h>| !fx.contains(t, "divisor_sum·d");
    let mut out = Vec::with_capacity(terms.len());
    for t in terms {
        let factors: Vec<T<'h>> = match fx.call_of(t, "mul") {
            Some(fs) => fs.to_vec(),
            None => vec![t],
        };
        let (consts, vars): (Vec<T<'h>>, Vec<T<'h>>) = factors.into_iter().partition(|&x| free(fx, x));
        let c = fx.mul(&consts);
        let one = fx.int(1);
        let is_call1 = |fx: &Fx<'_, '_, '_, 'h>, x: T<'h>, name: &str, arg: T<'h>| fx.call_of(x, name).is_some_and(|g| g.len() == 1 && g[0] == arg);
        let value = match vars.as_slice() {
            [] => fx.call("divisor_sigma", &[fx.int(0), n]),
            [v] if *v == d => fx.call("divisor_sigma", &[one, n]),
            [v] if fx.call_of(*v, "pow").is_some_and(|g| g.len() == 2 && g[0] == d && free(fx, g[1])) => {
                let k = fx.call_of(*v, "pow").map(|g| g[1]).unwrap_or(one);
                fx.call("divisor_sigma", &[k, n])
            }
            [v] if is_call1(fx, *v, "moebius_mu", d) => fx.call("kronecker_delta", &[fx.sub(one, n)]),
            [v] if is_call1(fx, *v, "euler_phi", d) => n,
            [v] if is_call1(fx, *v, "mangoldt_lambda", d) => fx.call("log", &[n]),
            [v] if fx.call_of(*v, "pow").is_some_and(|g| g.len() == 2 && is_call1(fx, g[0], "moebius_mu", d) && fx.is_int(g[1], 2)) => {
                fx.pow(fx.int(2), fx.call("prime_nu", &[n]))
            }
            [p, q] => {
                let inv = fx.pow(d, fx.int(-1));
                let quot = fx.eval(fx.mul(&[n, inv]))?;
                let pair = |a: T<'h>, b: T<'h>| (a, b);
                let mut hit = None;
                for (x, y) in [pair(*p, *q), pair(*q, *p)] {
                    if is_call1(fx, x, "moebius_mu", d) && y == inv {
                        hit = Some(fx.div(fx.call("euler_phi", &[n]), n));
                    } else if is_call1(fx, x, "moebius_mu", quot) && is_call1(fx, y, "log", d) {
                        hit = Some(fx.call("mangoldt_lambda", &[n]));
                    }
                }
                match hit {
                    Some(v) => v,
                    None => return Ok(None),
                }
            }
            _ => return Ok(None),
        };
        out.push(fx.mul(&[c, value]));
    }
    Ok(Some(fx.eval(fx.add(&out))?))
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
    // the table p(0..=n) is kept per thread: a table of values p(1), ..., p(1000) costs one extension
    let known = PARTITIONS.with(|t| t.borrow().len());
    if n >= known {
        fx.tick(((n as f64).powf(1.5) / 256.0) as u64)?;
    }
    let v = PARTITIONS.with(|t| {
        let mut p = t.borrow_mut();
        if p.is_empty() {
            p.push(Int::from_i64(1));
        }
        // Euler's pentagonal number recurrence p(m) = sum_k (-1)^(k+1) (p(m - k(3k-1)/2) + p(m - k(3k+1)/2)):
        // O(m^1.5) additions
        for m in p.len()..=n {
            let mut acc = Int::from_i64(0);
            let mut k = 1usize;
            loop {
                let g1 = k * (3 * k - 1) / 2;
                if g1 > m {
                    break;
                }
                let mut t = p[m - g1].clone();
                let g2 = k * (3 * k + 1) / 2;
                if g2 <= m {
                    t = t.add(&p[m - g2]);
                }
                acc = if k % 2 == 1 { acc.add(&t) } else { acc.sub(&t) };
                k += 1;
            }
            p.push(acc);
        }
        if !distinct {
            return p[n].clone();
        }
        // distinct parts: prod (1 + q^k) = prod (1 - q^(2k)) / prod (1 - q^k), so Q(n) = sum over the pentagonal
        // numbers g (signs (-1)^k) of the sign times p(n - 2 g)
        let mut acc = p[n].clone();
        let mut k = 1usize;
        while k * (3 * k - 1) <= n {
            let mut t = p[n - k * (3 * k - 1)].clone();
            if k * (3 * k + 1) <= n {
                t = t.add(&p[n - k * (3 * k + 1)]);
            }
            acc = if k % 2 == 1 { acc.sub(&t) } else { acc.add(&t) };
            k += 1;
        }
        acc
    });
    Ok(Some(int_t(fx, &v)))
}
thread_local! {
    static PARTITIONS: std::cell::RefCell<Vec<Int>> = const { std::cell::RefCell::new(Vec::new()) };
}
fn partitions_p<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    partitions(fx, a, false)
}
fn partitions_q<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    partitions(fx, a, true)
}

/// An `i128` as an integer term's value.
fn int_of_i128(v: i128) -> Int {
    let mag = v.unsigned_abs();
    let two64 = Int::from_i64(1 << 32).mul(&Int::from_i64(1 << 32));
    let r = Int::from_i64((mag >> 64) as i64).mul(&two64).add(&Int::from_nat(Nat::from_u64(mag as u64)));
    if v < 0 {
        r.neg()
    } else {
        r
    }
}

/// tau(n): the coefficient of q^n in q prod (1 - q^k)^24, for n up to 200000. By Jacobi's identity
/// prod (1 - q^k)^3 = sum over j of (-1)^j (2j + 1) q^(j(j+1)/2) (a series with about sqrt(2n) terms), so the 24th
/// power is the eighth power of that sparse series: its square (sparse by sparse), then six products of a dense
/// series with the sparse one, in `i128` (|tau(n)| < n^6 stays below 2^127; an overflow is declined).
fn ramanujan_tau<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>]) -> R<'h> {
    let Some(n) = small(fx, a[0], 200_000) else { return Ok(None) };
    if n <= 0 {
        return Ok(Some(fx.int(0)));
    }
    let m = (n - 1) as usize;
    // the sparse series: (exponent, coefficient)
    let mut sp: Vec<(usize, i128)> = vec![];
    let mut j: usize = 0;
    while j * (j + 1) / 2 <= m {
        sp.push((j * (j + 1) / 2, if j.is_multiple_of(2) { (2 * j + 1) as i128 } else { -((2 * j + 1) as i128) }));
        j += 1;
    }
    fx.tick((6 * sp.len() * m / 256 + 1) as u64)?;
    let mut cur = vec![0i128; m + 1];
    for &(e1, c1) in &sp {
        for &(e2, c2) in &sp {
            if e1 + e2 <= m {
                cur[e1 + e2] += c1 * c2;
            }
        }
    }
    for _ in 0..6 {
        let mut next = vec![0i128; m + 1];
        for (i, &c) in cur.iter().enumerate() {
            if c == 0 {
                continue;
            }
            for &(e, s) in sp.iter().take_while(|&&(e, _)| i + e <= m) {
                let Some(p) = c.checked_mul(s) else { return Ok(None) };
                let Some(t) = next[i + e].checked_add(p) else { return Ok(None) };
                next[i + e] = t;
            }
        }
        cur = next;
    }
    Ok(Some(int_t(fx, &int_of_i128(cur[m]))))
}

// ---------------------------------------------------------------- bits

/// Two's-complement limbs of `x` in `w` limbs (sign-extended).
fn twos(x: &Int, w: usize) -> Vec<u64> {
    let mut v = x.mag.limbs.clone();
    v.resize(w, 0);
    if x.neg {
        let mut carry = true;
        for l in v.iter_mut() {
            let (s, c) = (!*l).overflowing_add(carry as u64);
            *l = s;
            carry = c;
        }
    }
    v
}

/// The integer whose two's-complement representation is `v` (top bit = sign).
fn from_twos(mut v: Vec<u64>) -> Int {
    let neg = v.last().is_some_and(|l| l >> 63 == 1);
    if neg {
        let mut carry = true;
        for l in v.iter_mut() {
            let (s, c) = (!*l).overflowing_add(carry as u64);
            *l = s;
            carry = c;
        }
    }
    Int::new(neg, Nat::from_limbs(v))
}

fn bits<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], op: u8) -> R<'h> {
    // arbitrary-size integers: sign-extended two's complement (Knuth 4.1, bitwise operations on infinite-precision
    // signed integers); a bit-not is -1 - n
    if op == 3 && a.len() != 1 {
        return Ok(None);
    }
    let xs: Option<Vec<Int>> = a.iter().map(|&t| int(fx, t)).collect();
    let Some(xs) = xs else { return bits_symbolic(fx, a, op) };
    let w = xs.iter().map(|x| x.mag.limbs.len()).max().unwrap_or(0) + 1;
    fx.tick(w as u64 * xs.len() as u64 / 64 + 1)?;
    let mut acc: Vec<u64> = vec![if op == 0 { u64::MAX } else { 0 }; w];
    for x in &xs {
        let t = twos(x, w);
        for (r, l) in acc.iter_mut().zip(&t) {
            *r = match op {
                0 => *r & l,
                1 => *r | l,
                2 => *r ^ l,
                _ => !l,
            };
        }
    }
    Ok(Some(int_t(fx, &from_twos(acc))))
}

/// The laws of the bitwise operations on integers that hold whatever the integer values of the symbolic
/// arguments are: and/or are idempotent, xor cancels in pairs; 0 and -1 (all bits set) are the identity and the
/// absorbing element of and/or, 0 the identity of xor and xor with -1 is the complement; the complement is an
/// involution. Exact integers are folded; an argument that is a number but not an integer, or a list, declines.
fn bits_symbolic<'h>(fx: &mut Fx<'_, '_, '_, 'h>, a: &[T<'h>], op: u8) -> R<'h> {
    let mut konst: Vec<Int> = Vec::new();
    let mut syms: Vec<T<'h>> = Vec::new();
    for &t in a {
        if let Some(i) = int(fx, t) {
            konst.push(i);
        } else if fx.num(t).is_some() || fx.as_list(t).is_some() {
            return Ok(None);
        } else {
            syms.push(t);
        }
    }
    fx.tick(a.len() as u64 + 1)?;
    if op == 3 {
        // the complement of a complement
        return Ok(fx.call_of(a[0], "bit_not").filter(|g| g.len() == 1).map(|g| g[0]));
    }
    let name = ["bit_and", "bit_or", "bit_xor"][op as usize];
    // the folded constant
    let c: Option<T<'h>> = if konst.is_empty() {
        None
    } else {
        let ks: Vec<T<'h>> = konst.iter().map(|k| int_t(fx, k)).collect();
        match bits(fx, &ks, op)? {
            Some(t) => Some(t),
            None => return Ok(None),
        }
    };
    let (zero, all) = (fx.int(0), fx.int(-1));
    // the symbolic arguments after idempotence (and/or) or pair cancellation (xor), in first-seen order
    let mut kept: Vec<T<'h>> = Vec::new();
    for &s in &syms {
        match kept.iter().position(|&k| k == s) {
            Some(p) if op == 2 => {
                kept.remove(p);
            }
            Some(_) => {}
            None => kept.push(s),
        }
    }
    let mut c = c;
    match op {
        0 if c == Some(zero) => return Ok(Some(zero)),
        1 if c == Some(all) => return Ok(Some(all)),
        0 if c == Some(all) => c = None,
        1 | 2 if c == Some(zero) => c = None,
        _ => {}
    }
    // xor with all bits set is the complement
    let mut complement = false;
    if op == 2 && c == Some(all) && !kept.is_empty() {
        complement = true;
        c = None;
    }
    let mut args: Vec<T<'h>> = c.into_iter().collect();
    args.extend(kept.iter().copied());
    let body = match args.len() {
        0 => fx.int(if op == 0 { -1 } else { 0 }),
        1 => args[0],
        _ => fx.call(name, &args),
    };
    let out = if complement { fx.call("bit_not", &[body]) } else { body };
    if out == fx.call(name, a) {
        return Ok(None);
    }
    Ok(Some(out))
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
    // a list of integers: each reversed
    if let Some(items) = fx.as_list(a[0]) {
        let items: Vec<T<'h>> = items.to_vec();
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            let mut args = a.to_vec();
            args[0] = it;
            out.push(fx.call("integer_reverse", &args));
        }
        return Ok(Some(fx.list(&out)));
    }
    if int(fx, a[0]).is_none() {
        return Ok(None);
    }
    let b = a.get(1).copied().unwrap_or(fx.int(10));
    // a third argument pads the digit list with leading zeros to that length (the reversal keeps the length)
    let ds = match a.get(2) {
        Some(&len) => ev(fx, "integer_digits", &[a[0], b, len])?,
        None => ev(fx, "integer_digits", &[a[0], b])?,
    };
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
    // real_digits(x, b, len, start): written in the language library (lib/prelude.mc)
    if a.len() == 4 {
        return Ok(Some(fx.call("real_digits_at", a)));
    }
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
            if let Some(n) = len.filter(|&n| n > 0 && !p.is_zero()) {
                // the first n digits by one division (no cycle search)
                let Some((ds, e)) = leading_digits(&p, &d, b, n) else { return Ok(None) };
                return Ok(Some(fx.list(&[to_list(fx, &ds), fx.int(e)])));
            }
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
            // a machine real carries 16 decimal digits (53 bits are 15.95 of them); digits asked beyond are unknown
            let machine = matches!(&r, Real::Machine(_));
            let per = (bits as f64) / (b as f64).log2();
            let known = if machine { per.ceil().max(1.0) as usize } else { per.floor().max(1.0) as usize };
            let n = len.unwrap_or(known);
            if machine && n > known {
                let head = digits_of_real(fx, x, b, known, bits)?;
                let Some(h) = head else { return Ok(None) };
                let Some(&[ds, e]) = fx.as_list(h) else { return Ok(None) };
                let mut items: Vec<T<'h>> = fx.as_list(ds).unwrap_or(&[]).to_vec();
                items.resize(n, fx.sym("indeterminate"));
                return Ok(Some(fx.list(&[fx.list(&items), e])));
            }
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
    let Some((ds, e)) = leading_digits(&q.num.mag, &q.den, b, n) else { return Ok(None) };
    let items: Vec<T<'h>> = ds.iter().map(|&d| fx.int(d as i64)).collect();
    Ok(Some(fx.list(&[fx.list(&items), fx.int(e)])))
}

/// The digits of the integer `m` in base `b`, most significant first (no leading zeros; empty for 0). A power of
/// two takes bit groups, base 10 the divide-and-conquer decimal conversion, any other base machine-word chunks.
fn nat_digits(m: &Nat, b: u64) -> Vec<u64> {
    if m.is_zero() {
        return vec![];
    }
    if b.is_power_of_two() {
        let k = b.trailing_zeros() as u64;
        let total = m.bits().div_ceil(k);
        return (0..total)
            .rev()
            .map(|i| (0..k).fold(0u64, |acc, j| acc | (u64::from(m.bit(i * k + j)) << j)))
            .collect();
    }
    if b == 10 {
        return m.to_decimal().bytes().map(|c| u64::from(c - b'0')).collect();
    }
    let mut chunk = 1u64;
    let mut width = 0usize;
    while let Some(c) = chunk.checked_mul(b) {
        chunk = c;
        width += 1;
    }
    let mut out: Vec<u64> = vec![];
    let mut v = m.clone();
    while !v.is_zero() {
        let (qq, mut r) = v.divrem_u64(chunk);
        v = qq;
        for _ in 0..width {
            out.push(r % b);
            r /= b;
        }
    }
    while out.last() == Some(&0) {
        out.pop();
    }
    out.reverse();
    out
}

/// The first `n` significant digits (truncated, zero-padded where the expansion ends) of the positive rational
/// p/q in base `b` and the exponent e with p/q = 0.d1 d2 ... * b^e, from one integer division
/// floor(p b^s / q) and a base conversion, so the cost is that of a big division however long the cycle is.
fn leading_digits(p: &Nat, q: &Nat, b: u64, n: usize) -> Option<(Vec<u64>, i64)> {
    if p.is_zero() || n == 0 || q.is_zero() {
        return None;
    }
    let lg = (b as f64).log2();
    let mut e = ((p.bits() as f64 - q.bits() as f64) / lg).floor() as i64 + 1;
    let (lo_pow, hi_pow) = (Nat::pow(b, n as u64 - 1), Nat::pow(b, n as u64));
    for _ in 0..8 {
        let s = n as i64 - e;
        let m = if s >= 0 { p.mul(&Nat::pow(b, s as u64)).div(q) } else { p.div(&q.mul(&Nat::pow(b, s.unsigned_abs()))) };
        if m.cmp(&lo_pow) == std::cmp::Ordering::Less {
            e -= 1;
        } else if m.cmp(&hi_pow) != std::cmp::Ordering::Less {
            e += 1;
        } else {
            let ds = nat_digits(&m, b);
            return (ds.len() == n).then_some((ds, e));
        }
    }
    None
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
        (None, true) => 64,
        (None, false) => return Ok(None),
    };
    // a machine real without a count: the terms shared by every real that rounds to it, the continued fractions
    // of the ends x -+ ulp/2 of its interval taken together while they agree (the reals with a given leading
    // part form an interval, so the shared terms are those of every point between)
    if let (Some(Num::Real(Real::Machine(f))), None) = (&num, limit) {
        if f.is_finite() && *f != 0.0 {
            let ex = fx.eval(fx.call("set_precision", &[x, fx.sym("inf")]))?;
            let Some(q) = fx.rat_of(ex) else { return Ok(None) };
            let e = ((f.to_bits() >> 52) & 0x7ff) as i64;
            // half an ulp: 2^(e - 1023 - 53) for a normal number, 2^(-1075) for a subnormal one
            let k = if e == 0 { -1075 } else { e - 1023 - 53 };
            let mut half = Rat::from_i64(1);
            let two = if k < 0 { Rat::new(Int::from_i64(1), Int::from_i64(2)) } else { Rat::from_i64(2) };
            for _ in 0..k.unsigned_abs() {
                half = half.mul(&two);
            }
            // the terms shared by the ends x -+ ulp/2 (so by every real that rounds to x); where the binary value
            // itself ends (1.5 = [1; 2]) its last term too
            let (mut a, mut b, mut c) = (q.sub(&half), q.add(&half), q.clone());
            let mut terms = vec![];
            while terms.len() < l {
                let (ta, ra) = a.floor_fract();
                let (tb, rb) = b.floor_fract();
                let (tc, rc) = c.floor_fract();
                if ta != tb {
                    if rc.is_zero() {
                        terms.push(tc);
                    }
                    break;
                }
                terms.push(ta);
                if ra.is_zero() || rb.is_zero() || rc.is_zero() {
                    break;
                }
                // (t -> 1/t reverses the order of the ends, which does not matter for the comparison)
                let (Some(ia), Some(ib), Some(ic)) = (ra.recip(), rb.recip(), rc.recip()) else { break };
                (a, b, c) = (ia, ib, ic);
            }
            return Ok(Some(as_list(fx, &terms)));
        }
    }
    // an exact constant: digits enough for l terms whose convergents' denominators stay inside the precision,
    // doubled until they are (a term beyond the precision would be a digit of the rounding, not of x); past
    // MAX_CF_DIGITS the call is left unevaluated
    const MAX_CF_DIGITS: i64 = 20_000;
    let mut digits = (l as i64) * 2 + 30;
    loop {
        let v = if machine { x } else { ev(fx, "n", &[x, fx.int(digits)])? };
        let Some(Num::Real(r)) = fx.num(v) else { return Ok(None) };
        let Real::Ball(_, p) = &r else { break };
        if limit.is_none() {
            break;
        }
        let ex = fx.eval(fx.call("set_precision", &[v, fx.sym("inf")]))?;
        let Some(q) = fx.rat_of(ex) else { return Ok(None) };
        let lim = *p as f64 / 2.0 - 4.0;
        let mut terms = vec![];
        let (mut n, mut d) = (q.num, q.den);
        let (mut k0, mut k1) = (Int::from_i64(0), Int::from_i64(1));
        let mut short = false;
        while !d.is_zero() && terms.len() < l {
            let (t, r) = n.divrem_floor(&d);
            let k2 = t.mul(&k1).add(&k0);
            if (k2.mag.bits() as f64) > lim {
                short = true;
                break;
            }
            terms.push(t);
            k0 = k1;
            k1 = k2;
            n = Int::from_nat(d);
            d = r;
        }
        if !short {
            return Ok(Some(as_list(fx, &terms)));
        }
        if digits >= MAX_CF_DIGITS {
            return Ok(None);
        }
        digits = (digits * 2).min(MAX_CF_DIGITS);
    }
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
    def!("is_square_free", &[1, 2, 3], true, Some(is_square_free), "is_square_free(n): true when no square of a prime divides n"),
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
    def!("integer_reverse", &[1, 2, 3], true, Some(integer_reverse), "integer_reverse(n, b): the integer with the base-b digits of n reversed (b = 10 by default)"),
    def!("mixed_radix", &[1], false, Some(mixed_radix), "mixed_radix([b1, ..., bk]): a mixed-radix base for integer_digits and from_digits"),
    def!("real_digits", &[1, 2, 3, 4], false, Some(real_digits), "real_digits(x), real_digits(x, b), real_digits(x, b, len), real_digits(x, b, len, start): [digits, e] with |x| = 0.d1 d2 ... b^e; a rational's repeating digits form a final sublist"),
    def!("continued_fraction", &[1, 2], false, Some(continued_fraction), "continued_fraction(x), continued_fraction(x, n): the continued fraction terms of x (a quadratic irrational's period as a final sublist)"),
    def!("from_continued_fraction", &[1], false, Some(from_continued_fraction), "from_continued_fraction(terms): the number with these continued fraction terms (a final sublist repeats)"),
];
