//! Asymptotic expansions of special functions at an infinity of their argument (item 13C), truncated after
//! `n` terms, as exp-log terms for Gruntz's algorithm and for `asymptotic`.
//!
//! The expansions are the classical ones: NIST DLMF 5.11.1 (Stirling's series for `log_gamma`, with the
//! Bernoulli numbers), 5.15.8 (polygamma), 7.12.1 (`erfc`), 7.12 and 7.5 (`erfi`, Dawson's integral), 6.12.2
//! (`exp_integral_ei`), 8.11.2 and 8.19 (the incomplete gamma function and `exp_integral_e`), 10.40.1-2 (`bessel_i`,
//! `bessel_k`), 9.7.5-8 (the Airy functions), 4.13.10 (de Bruijn's series of `product_log`), and the Dirichlet
//! series of `zeta`. Each holds for the real argument tending to the infinity given (`s`); others give `None`.
use mc_calculus::term::Build;
use mc_api::{Store, T};

/// The Bernoulli numbers B_2, B_4, ..., B_24.
const BERNOULLI: [(i64, i64); 12] = [
    (1, 6),
    (-1, 30),
    (1, 42),
    (-1, 30),
    (5, 66),
    (-691, 2730),
    (7, 6),
    (-3617, 510),
    (43867, 798),
    (-174611, 330),
    (854513, 138),
    (-236364091, 2730),
];

/// The most terms an expansion here has.
pub(crate) const MAX_TERMS: usize = 12;

fn pi<'h>(b: &Build<'h>) -> T<'h> {
    b.sym(b.heap().sym("pi"))
}

fn exp<'h>(b: &Build<'h>, u: T<'h>) -> T<'h> {
    b.pow(b.sym(b.hd.e), u)
}

fn log<'h>(b: &Build<'h>, u: T<'h>) -> T<'h> {
    b.call("log", &[u])
}

/// `(u - 1/2) log u - u + log(2 pi)/2 + sum B_2k / (2k (2k - 1) u^(2k - 1))`, k = 1..n.
pub(crate) fn log_gamma<'h>(b: &Build<'h>, u: T<'h>, n: usize) -> T<'h> {
    let mut terms = vec![
        b.mul(&[b.sub(u, b.rat(1, 2)), log(b, u)]),
        b.neg(u),
        b.mul(&[b.rat(1, 2), log(b, b.mul(&[b.int(2), pi(b)]))]),
    ];
    for (k, &(p, q)) in BERNOULLI.iter().enumerate().take(n.min(MAX_TERMS)) {
        let k = k as i64 + 1;
        terms.push(b.mul(&[b.rat(p, q * 2 * k * (2 * k - 1)), b.powi(u, -(2 * k - 1))]));
    }
    b.add(&terms)
}

/// `sum_(k < n) c_k z^-k` with `c_k` from `coef(k)`.
fn inverse_series<'h>(b: &Build<'h>, z: T<'h>, n: usize, mut coef: impl FnMut(usize) -> T<'h>) -> T<'h> {
    let terms: Vec<T<'h>> = (0..n.clamp(1, MAX_TERMS)).map(|k| b.mul(&[coef(k), b.powi(z, -(k as i64))])).collect();
    b.add(&terms)
}

/// (2k - 1)!! (1 for k = 0) as a term.
fn double_factorial<'h>(b: &Build<'h>, k: usize) -> T<'h> {
    let mut p = vec![b.one()];
    for j in 1..=k {
        p.push(b.int(2 * j as i64 - 1));
    }
    b.mul(&p)
}

fn factorial<'h>(b: &Build<'h>, k: usize) -> T<'h> {
    let mut p = vec![b.one()];
    for j in 1..=k {
        p.push(b.int(j as i64));
    }
    b.mul(&p)
}

/// `erfc(u)` for u → +inf: `exp(-u^2)/(u sqrt(pi)) sum (-1)^k (2k-1)!! / (2u^2)^k`.
fn erfc<'h>(b: &Build<'h>, u: T<'h>, n: usize) -> T<'h> {
    let z = b.mul(&[b.int(2), b.powi(u, 2)]);
    let s = inverse_series(b, z, n, |k| b.mul(&[b.int(if k % 2 == 0 { 1 } else { -1 }), double_factorial(b, k)]));
    b.mul(&[exp(b, b.neg(b.powi(u, 2))), b.powi(u, -1), b.pow(pi(b), b.rat(-1, 2)), s])
}

/// `erfi(u)` for u → +inf: `exp(u^2)/(u sqrt(pi)) sum (2k-1)!! / (2u^2)^k`.
fn erfi<'h>(b: &Build<'h>, u: T<'h>, n: usize) -> T<'h> {
    let z = b.mul(&[b.int(2), b.powi(u, 2)]);
    let s = inverse_series(b, z, n, |k| double_factorial(b, k));
    b.mul(&[exp(b, b.powi(u, 2)), b.powi(u, -1), b.pow(pi(b), b.rat(-1, 2)), s])
}

/// `exp_integral_ei(u)` for u → ±inf: `exp(u)/u sum k!/u^k`.
fn ei<'h>(b: &Build<'h>, u: T<'h>, n: usize) -> T<'h> {
    let s = inverse_series(b, u, n, |k| factorial(b, k));
    b.mul(&[exp(b, u), b.powi(u, -1), s])
}

/// `a_k(nu) = prod_(j=1..k) (4 nu^2 - (2j - 1)^2) / (k! 8^k)`, the coefficients of the Hankel expansions.
fn hankel_coefficient<'h>(b: &Build<'h>, nu: T<'h>, k: usize) -> T<'h> {
    let mut p = vec![b.pow(b.mul(&[factorial(b, k), b.pow(b.int(8), b.int(k as i64))]), b.int(-1))];
    let mu = b.mul(&[b.int(4), b.powi(nu, 2)]);
    for j in 1..=k {
        let odd = 2 * j as i64 - 1;
        p.push(b.sub(mu, b.int(odd * odd)));
    }
    b.mul(&p)
}

/// The Airy coefficients u_k (9.7.2): u_0 = 1, u_k = u_(k-1) (6k-5)(6k-3)(6k-1) / (216 k (2k-1)).
fn airy_u<'h>(b: &Build<'h>, k: usize) -> T<'h> {
    let mut p = vec![b.one()];
    for j in 1..=k as i64 {
        p.push(b.rat((6 * j - 5) * (6 * j - 3) * (6 * j - 1), 216 * j * (2 * j - 1)));
    }
    b.mul(&p)
}

/// Unsigned Stirling numbers of the first kind [n, k].
fn stirling1(n: usize, k: usize) -> i64 {
    let mut row = vec![1i64];
    for i in 0..n {
        let mut next = vec![0i64; row.len() + 1];
        for (j, &c) in row.iter().enumerate() {
            next[j + 1] += c;
            next[j] += c * i as i64;
        }
        row = next;
    }
    row.get(k).copied().unwrap_or(0)
}

/// The expansion of `f(params..., u)` as u → `s`·inf with `n` terms, or `None` when not listed (or oscillating).
pub(crate) fn at_infinity<'h>(b: &Build<'h>, f: &str, params: &[T<'h>], u: T<'h>, s: i8, n: usize) -> Option<T<'h>> {
    let half = b.rat(1, 2);
    let quarter = b.rat(1, 4);
    Some(match (f, params, s) {
        ("erf", [], 1) => b.sub(b.one(), erfc(b, u, n)),
        ("erf", [], _) => b.sub(erfc(b, b.neg(u), n), b.one()),
        ("erfc", [], 1) => erfc(b, u, n),
        ("erfc", [], _) => b.sub(b.int(2), erfc(b, b.neg(u), n)),
        ("erfi", [], 1) => erfi(b, u, n),
        ("erfi", [], _) => b.neg(erfi(b, b.neg(u), n)),
        ("dawson_f", [], _) => {
            let z = b.mul(&[b.int(2), b.powi(u, 2)]);
            b.mul(&[half, b.powi(u, -1), inverse_series(b, z, n, |k| double_factorial(b, k))])
        }
        ("exp_integral_ei", [], _) => ei(b, u, n),
        ("log_integral", [], 1) => ei(b, log(b, u), n),
        ("exp_integral_e", [m], 1) => {
            // exp(-u)/u sum (-1)^k (m)_k / u^k
            let s = inverse_series(b, u, n, |k| {
                let mut p = vec![b.int(if k % 2 == 0 { 1 } else { -1 })];
                for j in 0..k {
                    p.push(b.add(&[*m, b.int(j as i64)]));
                }
                b.mul(&p)
            });
            b.mul(&[exp(b, b.neg(u)), b.powi(u, -1), s])
        }
        ("gamma", [a], 1) => {
            // the upper incomplete gamma function: u^(a-1) exp(-u) sum (a-1)(a-2)...(a-k) / u^k
            let s = inverse_series(b, u, n, |k| {
                let mut p = vec![b.one()];
                for j in 1..=k {
                    p.push(b.sub(*a, b.int(j as i64)));
                }
                b.mul(&p)
            });
            b.mul(&[b.pow(u, b.sub(*a, b.one())), exp(b, b.neg(u)), s])
        }
        ("bessel_i", [nu], 1) => {
            let s = inverse_series(b, u, n, |k| b.mul(&[b.int(if k % 2 == 0 { 1 } else { -1 }), hankel_coefficient(b, *nu, k)]));
            b.mul(&[exp(b, u), b.pow(b.mul(&[b.int(2), pi(b), u]), b.rat(-1, 2)), s])
        }
        ("bessel_k", [nu], 1) => {
            let s = inverse_series(b, u, n, |k| hankel_coefficient(b, *nu, k));
            b.mul(&[b.pow(b.mul(&[pi(b), b.powi(b.mul(&[b.int(2), u]), -1)]), half), exp(b, b.neg(u)), s])
        }
        (g @ ("airy_ai" | "airy_bi" | "airy_ai_prime" | "airy_bi_prime"), [], 1) => {
            let zeta = b.mul(&[b.rat(2, 3), b.pow(u, b.rat(3, 2))]);
            let ai = g.starts_with("airy_ai");
            let prime = g.ends_with("prime");
            let s = inverse_series(b, zeta, n, |k| {
                let uk = airy_u(b, k);
                let ck = if prime && k > 0 { b.mul(&[b.rat(-(6 * k as i64 + 1), 6 * k as i64 - 1), uk]) } else { uk };
                if ai && k % 2 == 1 {
                    b.neg(ck)
                } else {
                    ck
                }
            });
            let e = exp(b, if ai { b.neg(zeta) } else { zeta });
            let scale = if ai { b.mul(&[half, b.pow(pi(b), b.rat(-1, 2))]) } else { b.pow(pi(b), b.rat(-1, 2)) };
            let power = if prime { b.pow(u, quarter) } else { b.pow(u, b.neg(quarter)) };
            let sign = if ai && prime { b.int(-1) } else { b.one() };
            b.mul(&[sign, scale, power, e, s])
        }
        ("digamma", [], 1) => digamma(b, u, n),
        ("polygamma", [m], 1) if *m == b.zero() => digamma(b, u, n),
        ("polygamma", [m], 1) => {
            // (-1)^(m+1) [ (m-1)!/u^m + m!/(2 u^(m+1)) + sum B_2k (2k+m-1)!/((2k)! u^(2k+m)) ], m a positive integer
            let mi = match b.heap().view(*m) {
                mc_api::View::Int(k) if (1..=20).contains(&k) => k as usize,
                _ => return None,
            };
            let mut terms = vec![
                b.mul(&[factorial(b, mi - 1), b.powi(u, -(mi as i64))]),
                b.mul(&[half, factorial(b, mi), b.powi(u, -(mi as i64) - 1)]),
            ];
            for (k, &(p, q)) in BERNOULLI.iter().enumerate().take(n.min(MAX_TERMS)) {
                let k = k + 1;
                let c = b.mul(&[b.rat(p, q), factorial(b, 2 * k + mi - 1), b.pow(factorial(b, 2 * k), b.int(-1))]);
                terms.push(b.mul(&[c, b.powi(u, -((2 * k + mi) as i64))]));
            }
            b.mul(&[b.int(if mi % 2 == 1 { 1 } else { -1 }), b.add(&terms)])
        }
        ("zeta", [], 1) => {
            let mut terms = vec![b.one()];
            for k in 2..=(n.clamp(1, MAX_TERMS) + 1) {
                terms.push(b.pow(b.int(k as i64), b.neg(u)));
            }
            b.add(&terms)
        }
        ("product_log", [], 1) => {
            // L1 - L2 + sum_(k >= 0, m >= 1, k + m <= n) (-1)^k [k+m, k+1] / m! L2^m / L1^(k+m)
            let l1 = log(b, u);
            let l2 = log(b, l1);
            let mut terms = vec![l1, b.neg(l2)];
            let n = n.clamp(1, MAX_TERMS);
            for k in 0..n {
                for m in 1..=(n - k) {
                    let c = stirling1(k + m, k + 1);
                    let c = b.mul(&[b.int(if k % 2 == 0 { c } else { -c }), b.pow(factorial(b, m), b.int(-1))]);
                    terms.push(b.mul(&[c, b.powi(l2, m as i64), b.powi(l1, -((k + m) as i64))]));
                }
            }
            b.add(&terms)
        }
        _ => return None,
    })
}

/// `digamma(u)` for u → +inf: `log u - 1/(2u) - sum B_2k / (2k u^(2k))`.
fn digamma<'h>(b: &Build<'h>, u: T<'h>, n: usize) -> T<'h> {
    let mut terms = vec![log(b, u), b.mul(&[b.rat(-1, 2), b.powi(u, -1)])];
    for (k, &(p, q)) in BERNOULLI.iter().enumerate().take(n.min(MAX_TERMS)) {
        let k = k as i64 + 1;
        terms.push(b.mul(&[b.rat(-p, q * 2 * k), b.powi(u, -2 * k)]));
    }
    b.add(&terms)
}
