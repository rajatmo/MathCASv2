//! Special-function identities (phase 12C): a priority-ordered table of guarded rules on single calls, and the
//! per-family simplifiers that work on sums. `function_expand` uses the rules marked [`EXPAND`], `simplify` and
//! `full_simplify` those marked [`SIMPLIFY`] as one move of their search (kept only when the complexity drops,
//! unless the rule is marked [`ALWAYS`]: it removes a special function in favour of elementary ones). Guards are
//! questions to the decision service (`mc-decide`): positivity, integrality, bounds; an unanswered guard leaves the
//! call alone.
//!
//! Sources (identities taken from the literature, not from any implementation): DLMF (NIST Digital Library of
//! Mathematical Functions) chapters 4, 5, 8, 9, 10, 15, 19, 22, 24-26; Prudnikov, Brychkov and Marichev, *Integrals
//! and Series* vol. 3 (hypergeometric reductions); Koshy, *Fibonacci and Lucas Numbers* (index shifts).
//!
//! | rule (priority order) | identity | guard | use |
//! |---|---|---|---|
//! | `product_log` | `W(u e^u) = u` (DLMF 4.13.1); `W(w log w) = log w` | `u >= -1`; `w >= 1/e` | both, always |
//! | `elliptic_f` | `F(phi, 1) = log(sec(phi) + tan(phi))` (DLMF 19.6.8, 4.23.41) | `-pi/2 < phi < pi/2` | both, always |
//! | `zeta'` | `zeta'(-2n) = (-1)^n (2n)! zeta(2n + 1)/(2 (2 pi)^(2n))`, `zeta'(0) = -log(2 pi)/2` (DLMF 25.6.11, 25.6.12 with 25.4.2) | `n` a positive integer | both, always |
//! | trigonometric constants | `sin`, `cos`, `tan` of `p pi/q` in radicals (see [`crate::trigconst`]) | `q = 2^k m`, `m` in 1, 3, 5, 15 | expand |
//! | trig of inverse trig | `sin(atan x) = x/sqrt(1 + x^2)`, ... (DLMF 4.24.11-4.24.17, 4.38) | none | both |
//! | `exp(k atanh x)` | `cosh(k atanh x) + sinh(k atanh x)`, each written rationally (DLMF 4.38.8) | `k` an integer, `|k| <= 6` | expand |
//! | `log` | `log(z) = log abs(z) + i arg(z)` for a closed complex `z` (DLMF 4.2.3) | none | both |
//! | `log(sqrt w)` | `log(w)/2`; `log(a^2 - u^2) = log(a - u) + log(a + u)` (`a > 0`; the imaginary parts of the two factors have opposite signs, so their arguments add without wrapping) | none | expand |
//! | `sqrt(-u^2)` | `sqrt(u) sqrt(-u)` (both sides have argument in `(-pi/2, pi/2]`) | none | expand |
//! | `exp_integral_e` | `E_p(x) = x^(p - 1) Gamma(1 - p, x)` (DLMF 8.19.1) | none | simplify |
//! | Bessel at `-x` | `J_nu(-x) = J_nu(x) x^-nu (-x)^nu`, the same for `I`; `K_nu(-x)` by DLMF 10.27.4 with 10.11.? (continuation), its integer-order limit | `nu` an integer for the limit form | expand |
//! | Airy rotations | `Ai(z e^(-+2 pi i/3)) = e^(-+pi i/3) (Ai(z) +- i Bi(z))/2` (DLMF 9.2.11) | the coefficient a primitive cube root of 1 | expand |
//! | Jacobi | the twelve functions by `sn`, `cn`, `dn` (DLMF 22.2.10) | none | expand |
//! | `zeta(1 - s)` | `2^(1 - s) pi^-s sin(pi (1 - s)/2) Gamma(s) zeta(s)` (DLMF 25.4.2) | the argument written `1 - s` | expand |
//! | `hypergeometric` | `1F2(nu + 1/2; nu + 1, 2 nu + 1; w) = Gamma(nu + 1)^2 (w/4)^-nu I_nu(sqrt w)^2` (DLMF 10.8.3, 10.39), `0F1(; b; z)` by `I_(b-1)` (DLMF 10.39.9), `1F1(a; a; z) = e^z`, `2F1(a, b; b; z) = (1 - z)^-a`, `2F1(1, 1; 2; z) = -log(1 - z)/z` (DLMF 15.4.1), equal upper and lower parameters cancelled (DLMF 16.2) | none | both |
//! | `legendre_q(n + 1/2, 1/2, x)` | elementary (DLMF 14.3.12 with 15.8.1; 14.5.17) | `0 <= n <= 10` | expand |
//! | `3F2(+-1/2, 1/2, 1/2; 1, 1; z)` | complete elliptic integrals of `m = (1 - sqrt(1 - z))/2` (Clausen, DLMF 16.12.1, 15.8.13; PBM 7.4.1) | none | both |
//! | `fibonacci(n + k)`, `lucas_l(n + k)` | `(F_k L_n + L_k F_n)/2`, `(5 F_k F_n + L_k L_n)/2` | `n` an integer | expand |
//!
//! Families (sums): gamma functions whose arguments differ by integers over the lowest one (DLMF 5.5.1),
//! polygammas the same way (DLMF 5.15.5), `Li2(z) + Li2(1 - z) = pi^2/6 - log(z) log(1 - z)` (DLMF 25.12.6),
//! arctangents of rationals combined ([`crate::trigconst::atan_sum`]), and `I_-nu(z) - I_nu(z) = 2 sin(nu pi)
//! K_nu(z)/pi` (DLMF 10.27.4).
use crate::facts::{Facts, Kind};
use crate::term::B;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

/// The rule is used by `function_expand`.
pub const EXPAND: u8 = 1;
/// The rule is a move of `simplify` and `full_simplify`.
pub const SIMPLIFY: u8 = 2;
/// The rule's result replaces the call in `simplify` whatever the cost (a special function removed).
pub const ALWAYS: u8 = 4;

/// The replacement of one call by a rule, or `None`.
type RuleFn = for<'a, 'b, 'c, 'd, 'h> fn(&mut NativeCx<'a, 'b, 'h>, &B<'h>, &mut Facts<'c, 'h>, &'d str, &'d [T<'h>]) -> Res<Option<T<'h>>>;

/// A rule of the table: the head it applies to, its priority (lower first), where it is used, its name.
pub struct Rule {
    /// the head of the calls it rewrites
    pub head: &'static str,
    /// lower runs first among the rules of one head
    pub priority: u8,
    /// [`EXPAND`], [`SIMPLIFY`], [`ALWAYS`]
    pub uses: u8,
    /// the identity's name
    pub name: &'static str,
    f: RuleFn,
}

/// The table, in priority order.
pub static TABLE: &[Rule] = &[
    Rule { head: "product_log", priority: 1, uses: EXPAND | SIMPLIFY | ALWAYS, name: "lambert inverse", f: product_log },
    Rule { head: "elliptic_f", priority: 2, uses: EXPAND | SIMPLIFY | ALWAYS, name: "F(phi, 1)", f: elliptic_f },
    Rule { head: "log", priority: 10, uses: EXPAND | SIMPLIFY, name: "log of a closed complex number", f: closed_log },
    Rule { head: "log", priority: 11, uses: EXPAND, name: "log of a square root", f: log_root },
    Rule { head: "pow", priority: 12, uses: EXPAND, name: "sqrt(-u^2)", f: sqrt_neg_square },
    Rule { head: "sin", priority: 20, uses: EXPAND, name: "trigonometric constant", f: trig_constant },
    Rule { head: "cos", priority: 20, uses: EXPAND, name: "trigonometric constant", f: trig_constant },
    Rule { head: "tan", priority: 20, uses: EXPAND, name: "trigonometric constant", f: trig_constant },
    Rule { head: "sin", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "cos", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "tan", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "sinh", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "cosh", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "tanh", priority: 21, uses: EXPAND | SIMPLIFY, name: "trig of inverse trig", f: trig_inverse },
    Rule { head: "exp_integral_e", priority: 30, uses: SIMPLIFY, name: "E_p as incomplete gamma", f: exp_integral_e },
    Rule { head: "bessel_j", priority: 40, uses: EXPAND, name: "Bessel at -x", f: bessel_negative },
    Rule { head: "bessel_i", priority: 40, uses: EXPAND, name: "Bessel at -x", f: bessel_negative },
    Rule { head: "bessel_k", priority: 40, uses: EXPAND, name: "Bessel at -x", f: bessel_negative },
    Rule { head: "airy_ai", priority: 41, uses: EXPAND, name: "Airy rotation", f: airy_rotation },
    Rule { head: "airy_bi", priority: 41, uses: EXPAND, name: "Airy rotation", f: airy_rotation },
    Rule { head: "zeta", priority: 50, uses: EXPAND, name: "zeta reflection", f: zeta_reflection },
    Rule { head: "hypergeometric_pfq", priority: 60, uses: EXPAND | SIMPLIFY, name: "pFq reduction", f: hyper_pfq },
    Rule { head: "hypergeometric_0f1", priority: 61, uses: EXPAND, name: "0F1 as Bessel I", f: hyper_0f1 },
    Rule { head: "hypergeometric1f1", priority: 61, uses: EXPAND | SIMPLIFY | ALWAYS, name: "1F1 reduction", f: hyper_1f1 },
    Rule { head: "hypergeometric2f1", priority: 61, uses: EXPAND | SIMPLIFY | ALWAYS, name: "2F1 reduction", f: hyper_2f1 },
    Rule { head: "legendre_q", priority: 65, uses: EXPAND, name: "Ferrers Q of order 1/2", f: legendre_q_half },
    Rule { head: "fibonacci", priority: 70, uses: EXPAND, name: "Fibonacci index shift", f: fibonacci_shift },
    Rule { head: "lucas_l", priority: 70, uses: EXPAND, name: "Lucas index shift", f: fibonacci_shift },
];

/// The Jacobi functions by `sn`, `cn`, `dn` (DLMF 22.2.10): name, numerator, denominator (`1` for none).
const JACOBI: &[(&str, &str, &str)] = &[
    ("jacobi_ns", "", "sn"),
    ("jacobi_nc", "", "cn"),
    ("jacobi_nd", "", "dn"),
    ("jacobi_sc", "sn", "cn"),
    ("jacobi_sd", "sn", "dn"),
    ("jacobi_cs", "cn", "sn"),
    ("jacobi_cd", "cn", "dn"),
    ("jacobi_ds", "dn", "sn"),
    ("jacobi_dc", "dn", "cn"),
];

/// The rules of `t`'s head with use `mask`, applied in priority order: the first replacement.
pub fn node<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, mask: u8, t: T<'h>) -> Res<Option<(T<'h>, bool)>> {
    let h = b.heap();
    // zeta'(k) is a curried call: derivative(1)(zeta)(k)
    if let Some(r) = zeta_derivative(b, t) {
        return Ok(Some((r, true)));
    }
    let Some((s, args)) = b.call_of(t) else { return Ok(None) };
    let name = h.sym_name(s);
    if mask & EXPAND != 0 {
        if let Some(&(_, num, den)) = JACOBI.iter().find(|j| j.0 == name) {
            if let [u, m] = args {
                let g = |n: &str| b.call_named(&format!("jacobi_{n}"), &[*u, *m]);
                let top = if num.is_empty() { b.int(1) } else { g(num) };
                return Ok(Some((b.div(top, g(den)), false)));
            }
        }
    }
    for r in TABLE.iter().filter(|r| r.head == name && r.uses & mask != 0) {
        if let Some(v) = (r.f)(cx, b, f, &name, args)? {
            return Ok(Some((v, r.uses & ALWAYS != 0)));
        }
    }
    Ok(None)
}

/// The families of sums (see the module documentation) with use `mask`: the first replacement of `t`.
pub fn family<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mask: u8, t: T<'h>) -> Res<Option<T<'h>>> {
    let Some((s, _)) = b.call_of(t) else { return Ok(None) };
    if s != b.hd.add {
        return Ok(None);
    }
    let h = b.heap();
    let has = |n: &str| h.mask(t) & h.mask(h.sym_term(h.sym(n))) != 0;
    if has("gamma") {
        if let Some(r) = shift_family(cx, b, t, "gamma")? {
            return Ok(Some(r));
        }
    }
    if has("polygamma") {
        if let Some(r) = shift_family(cx, b, t, "polygamma")? {
            return Ok(Some(r));
        }
    }
    if has("polylog") {
        if let Some(r) = polylog_reflection(b, t) {
            return Ok(Some(r));
        }
    }
    if has("atan") {
        if let Some(r) = crate::trigconst::atan_sum(b, t, mask & SIMPLIFY == 0) {
            return Ok(Some(r));
        }
    }
    if mask & SIMPLIFY != 0 && has("exp_integral_e") {
        if let Some(r) = exp_integral_difference(b, t) {
            return Ok(Some(r));
        }
    }
    if mask & SIMPLIFY != 0 && has("bessel_i") {
        if let Some(r) = bessel_i_difference(b, t) {
            return Ok(Some(r));
        }
    }
    Ok(None)
}

// ---------------------------------------------------------------------------------------------------------------
// helpers

fn int_of<'h>(b: &B<'h>, t: T<'h>) -> Option<i64> {
    match b.heap().view(t) {
        View::Int(k) => Some(k),
        _ => None,
    }
}

fn is_neg_number<'h>(b: &B<'h>, t: T<'h>) -> bool {
    b.is_number(t) && b.complex_parts(t).is_none() && b.ar.compare(t, b.int(0)).is_some_and(|o| o == mc_arith::number::Cmp3::Less)
}

/// Is `t` written with a leading minus sign (a negative number, a product with a negative coefficient, or a sum
/// whose first non-numeric term is one)?
pub fn neg_form<'h>(b: &B<'h>, t: T<'h>) -> bool {
    match b.call_of(t) {
        Some((m, args)) if m == b.hd.mul => args.first().is_some_and(|&c| is_neg_number(b, c)),
        Some((a, args)) if a == b.hd.add => args.iter().find(|x| !b.is_number(**x)).is_some_and(|&x| neg_form(b, x)),
        _ => is_neg_number(b, t),
    }
}

/// `a - c` term by term (the canonical `-1*(u + v)` is not distributed).
fn minus<'h>(b: &B<'h>, a: T<'h>, c: T<'h>) -> T<'h> {
    let mut ts = b.terms(a);
    ts.extend(b.terms(c).into_iter().map(|u| b.neg(u)));
    b.add(&ts)
}

/// `-t` term by term (a sum stays a sum).
fn negate<'h>(b: &B<'h>, t: T<'h>) -> T<'h> {
    let ts: Vec<T<'h>> = b.terms(t).into_iter().map(|u| b.neg(u)).collect();
    b.add(&ts)
}

fn pi<'h>(b: &B<'h>) -> T<'h> {
    let h = b.heap();
    h.sym_term(h.sym("pi"))
}

fn is_integer<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Facts<'_, 'h>, t: T<'h>) -> Res<bool> {
    if matches!(f.b.heap().view(t), View::Int(_) | View::BigInt { .. }) {
        return Ok(true);
    }
    Ok(f.info(cx, t)?.kind == Kind::Integer)
}

fn holds<'h>(cx: &mut NativeCx<'_, '_, 'h>, f: &mut Facts<'_, 'h>, rel: &str, l: T<'h>, r: T<'h>) -> Res<bool> {
    Ok(f.d.compare(cx, rel, l, r)? == Some(true))
}

// ---------------------------------------------------------------------------------------------------------------
// the rules

/// `W(u e^u) = u` for `u >= -1`; `W(w log w) = log w` for `w >= 1/e` (DLMF 4.13.1: the principal branch inverts
/// `u e^u` on `[-1, inf)`).
fn product_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [a] = args else { return Ok(None) };
    let fs = b.factors(*a);
    if fs.len() < 2 {
        return Ok(None);
    }
    for &g in &fs {
        if let Some(v) = b.exp_arg(g) {
            let rest = b.div(*a, g);
            if rest == v && holds(cx, f, "greater_equal", v, b.int(-1))? {
                return Ok(Some(v));
            }
        }
        if let Some(w) = b.unary(g, b.hd.log) {
            let rest = b.div(*a, g);
            let inv_e = cx.eval(b.exp(b.int(-1)))?;
            if rest == w && holds(cx, f, "greater_equal", w, inv_e)? {
                return Ok(Some(g));
            }
        }
    }
    Ok(None)
}

/// `F(phi, 1) = gd^-1(phi) = log(sec(phi) + tan(phi))` for `-pi/2 < phi < pi/2` (DLMF 19.6.8, 4.23.41).
fn elliptic_f<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [phi, m] = args else { return Ok(None) };
    if *m != b.int(1) {
        return Ok(None);
    }
    let half_pi = b.mul(&[b.rat(1, 2), pi(b)]);
    if !holds(cx, f, "less", *phi, half_pi)? || !holds(cx, f, "greater", *phi, b.neg(half_pi))? {
        return Ok(None);
    }
    let s = b.add(&[b.call(b.hd.sec, &[*phi]), b.call(b.hd.tan, &[*phi])]);
    Ok(Some(b.call(b.hd.log, &[s])))
}

/// `zeta'(-2n)` and `zeta'(0)` (DLMF 25.6.11; 25.6.12 at the trivial zeros by the functional equation 25.4.2).
fn zeta_derivative<'h>(b: &B<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let View::App { head, args: &[k] } = h.view(t) else { return None };
    let View::App { head: dz, args: &[zeta] } = h.view(head) else { return None };
    if b.sym_of(zeta).map(|s| h.sym_name(s)) != Some("zeta".into()) {
        return None;
    }
    let (s, &[one]) = b.call_of(dz)? else { return None };
    if h.sym_name(s) != "derivative" || one != b.int(1) {
        return None;
    }
    let k = int_of(b, k)?;
    if k == 0 {
        // zeta'(0) = -log(2 pi)/2
        return Some(b.mul(&[b.rat(-1, 2), b.call(b.hd.log, &[b.mul(&[b.int(2), pi(b)])])]));
    }
    if k >= 0 || k % 2 != 0 || k < -100 {
        return None;
    }
    let n = -k / 2;
    let fact = b.call_named("factorial", &[b.int(2 * n)]);
    let sign = if n % 2 == 0 { 1 } else { -1 };
    let den = b.mul(&[b.int(2), b.powi(b.mul(&[b.int(2), pi(b)]), 2 * n)]);
    Some(b.mul(&[b.int(sign), fact, b.call_named("zeta", &[b.int(2 * n + 1)]), b.powi(den, -1)]))
}

fn closed_log<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [z] = args else { return Ok(None) };
    crate::trigconst::closed_log(cx, b, &f.syms().num, *z)
}

/// `log(sqrt(w)) = log(w)/2`, then `log(a^2 - u^2) = log(a - u) + log(a + u)` for a positive rational square.
fn log_root<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [z] = args else { return Ok(None) };
    let Some((p, &[w, e])) = b.call_of(*z) else { return Ok(None) };
    if p != b.hd.pow || e != b.rat(1, 2) {
        return Ok(None);
    }
    let inner = difference_of_squares(b, w).unwrap_or_else(|| b.call(b.hd.log, &[w]));
    Ok(Some(b.mul(&[b.rat(1, 2), inner])))
}

/// `log(a - u) + log(a + u)` for `w = a^2 - u^2` (a a positive rational, the square's coefficient a rational square).
fn difference_of_squares<'h>(b: &B<'h>, w: T<'h>) -> Option<T<'h>> {
    let ts = b.terms(w);
    let [c, sq] = ts.as_slice() else { return None };
    let h = b.heap();
    let root = |q: T<'h>| -> Option<T<'h>> {
        let r = b.pow(q, b.rat(1, 2));
        matches!(h.view(r), View::Int(_) | View::Rational { .. }).then_some(r)
    };
    if !b.is_number(*c) || is_neg_number(b, *c) {
        return None;
    }
    let a = root(*c)?;
    let (k, base) = match b.call_of(*sq) {
        Some((m, &[k, x])) if m == b.hd.mul && is_neg_number(b, k) => (b.neg(k), x),
        _ => return None,
    };
    let (u, two) = b.int_power(base);
    if two != 2 {
        return None;
    }
    let u = b.mul(&[root(k)?, u]);
    Some(b.add(&[b.call(b.hd.log, &[b.add(&[a, b.neg(u)])]), b.call(b.hd.log, &[b.add(&[a, u])])]))
}

/// `sqrt(-u^2) = sqrt(u) sqrt(-u)`.
fn sqrt_neg_square<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [w, e] = args else { return Ok(None) };
    if *e != b.rat(1, 2) {
        return Ok(None);
    }
    let Some((m, &[k, sq])) = b.call_of(*w) else { return Ok(None) };
    if m != b.hd.mul || k != b.int(-1) {
        return Ok(None);
    }
    let (u, two) = b.int_power(sq);
    if two != 2 {
        return Ok(None);
    }
    let half = b.rat(1, 2);
    Ok(Some(b.mul(&[b.pow(u, half), b.pow(b.neg(u), half)])))
}

/// `sin`, `cos`, `tan` of a rational multiple of pi in radicals.
fn trig_constant<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [u] = args else { return Ok(None) };
    let Some((p, q)) = crate::trigconst::pi_multiple(b, *u) else { return Ok(None) };
    let sc = |sin| crate::trigconst::sin_cos(b, sin, p, q);
    Ok(match head {
        "sin" => sc(true),
        "cos" => sc(false),
        _ => match (sc(true), sc(false)) {
            (Some(s), Some(c)) if c != b.int(0) => Some(b.div(s, c)),
            _ => None,
        },
    })
}

/// `f(g(x))` for a circular or hyperbolic `f` and an inverse `g` (DLMF 4.24.11-4.24.17 and the hyperbolic
/// analogues of 4.38), written with `x` and square roots.
fn trig_inverse<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [u] = args else { return Ok(None) };
    let Some((g, &[x])) = b.call_of(*u) else { return Ok(None) };
    let h = b.heap();
    let gname = h.sym_name(g);
    let half = b.rat(1, 2);
    let one = b.int(1);
    let r1px2 = b.pow(b.add(&[one, b.powi(x, 2)]), half); // sqrt(1 + x^2)
    let r1mx2 = b.pow(b.sub(one, b.powi(x, 2)), half); // sqrt(1 - x^2)
    let inv = |t: T<'h>| b.powi(t, -1);
    // g, sin|sinh(g x), cos|cosh(g x), tan|tanh(g x)
    let table: [(&str, T<'h>, T<'h>, T<'h>); 6] = [
        ("atan", b.mul(&[x, inv(r1px2)]), inv(r1px2), x),
        ("asin", x, r1mx2, b.mul(&[x, inv(r1mx2)])),
        ("acos", r1mx2, x, b.mul(&[r1mx2, inv(x)])),
        ("atanh", b.mul(&[x, inv(r1mx2)]), inv(r1mx2), x),
        ("asinh", x, r1px2, b.mul(&[x, inv(r1px2)])),
        (
            "acosh",
            b.mul(&[b.pow(b.add(&[x, b.int(-1)]), half), b.pow(b.add(&[x, one]), half)]),
            x,
            b.mul(&[b.pow(b.add(&[x, b.int(-1)]), half), b.pow(b.add(&[x, one]), half), inv(x)]),
        ),
    ];
    let Some(&(_, s, c, t)) = table.iter().find(|e| e.0 == gname) else { return Ok(None) };
    let circular = matches!(gname.as_str(), "atan" | "asin" | "acos");
    Ok(match (head, circular) {
        ("sin", true) | ("sinh", false) => Some(s),
        ("cos", true) | ("cosh", false) => Some(c),
        ("tan", true) | ("tanh", false) => Some(t),
        _ => None,
    })
}

/// `E_p(x) = x^(p - 1) Gamma(1 - p, x)` (DLMF 8.19.1).
fn exp_integral_e<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [p, x] = args else { return Ok(None) };
    if b.is_number(*p) {
        // E_n(x) with a number n is a name of its own; the sums holding the matching gamma are handled by
        // `exp_integral_difference`
        return Ok(None);
    }
    let g = b.call_named("gamma", &[minus(b, b.int(1), *p), *x]);
    Ok(Some(b.mul(&[b.pow(*x, b.add(&[*p, b.int(-1)])), g])))
}

/// A sum with `E_p(x)` and an incomplete gamma `Gamma(1 - p, x)` (times `x^(p - 1)`): `E_p(x)` written as that
/// gamma, so that the terms combine (DLMF 8.19.1).
fn exp_integral_difference<'h>(b: &B<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let (ee, gm) = (h.sym("exp_integral_e"), h.sym("gamma"));
    let terms = b.terms(t);
    let mut hit = None;
    for &u in &terms {
        let factors: Vec<T<'h>> = match b.call_of(u) {
            Some((m, fs)) if m == b.hd.mul => fs.to_vec(),
            _ => vec![u],
        };
        for &f in &factors {
            if let Some((s, &[p, x])) = b.call_of(f) {
                if s == ee && b.is_number(p) {
                    // is there a gamma(1 - p, x) among the terms?
                    let want = minus(b, b.int(1), p);
                    let found = terms.iter().any(|&v| {
                        let fs: Vec<T<'h>> = match b.call_of(v) {
                            Some((m, fs)) if m == b.hd.mul => fs.to_vec(),
                            _ => vec![v],
                        };
                        fs.iter().any(|&g| matches!(b.call_of(g), Some((s2, &[a, y])) if s2 == gm && a == want && y == x))
                    });
                    if found {
                        hit = Some((f, p, x));
                    }
                }
            }
        }
    }
    let (f, p, x) = hit?;
    let g = b.call_named("gamma", &[minus(b, b.int(1), p), x]);
    let new = b.mul(&[b.pow(x, b.add(&[p, b.int(-1)])), g]);
    let out: Vec<T<'h>> = terms
        .iter()
        .map(|&u| match b.call_of(u) {
            Some((m, fs)) if m == b.hd.mul && fs.contains(&f) => b.call(m, &fs.iter().map(|&q| if q == f { new } else { q }).collect::<Vec<_>>()),
            _ if u == f => new,
            _ => u,
        })
        .collect();
    Some(b.add(&out))
}

/// `J_nu(-x)`, `I_nu(-x)`, `K_nu(-x)` by the values at `x` (analytic continuation, DLMF 10.11.1, 10.34.1 with
/// 10.27.4; for an integer order the limit of the `K` form).
fn bessel_negative<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [nu, a] = args else { return Ok(None) };
    let Some((m, fs)) = b.call_of(*a) else { return Ok(None) };
    if m != b.hd.mul || fs.first() != Some(&b.int(-1)) {
        return Ok(None);
    }
    let x = b.neg(*a);
    let (nu, half) = (*nu, b.rat(1, 2));
    let _ = half;
    let p = |e: T<'h>, base: T<'h>| b.pow(base, e);
    let mnu = b.neg(nu);
    Ok(match head {
        // J_nu(-x) = J_nu(x) x^-nu (-x)^nu: J_nu(z) z^-nu is entire and even
        "bessel_j" | "bessel_i" => Some(b.mul(&[b.call_named(head, &[nu, x]), p(mnu, x), p(nu, *a)])),
        "bessel_k" => {
            let k = b.call_named("bessel_k", &[nu, x]);
            let i = b.call_named("bessel_i", &[nu, x]);
            if is_integer(cx, f, nu)? {
                // the limit nu -> n of the general form: (log x - log(-x)) I_n(x) + (-1)^n K_n(x)
                let d = b.add(&[b.call(b.hd.log, &[x]), b.neg(b.call(b.hd.log, &[*a]))]);
                Some(b.add(&[b.mul(&[d, i]), b.mul(&[k, b.pow(b.int(-1), nu)])]))
            } else {
                // K_nu(-x) = K_nu(x) x^nu (-x)^-nu + (pi/2)(x^nu (-x)^-nu - x^-nu (-x)^nu) I_nu(x) csc(nu pi)
                let d = b.add(&[b.mul(&[p(nu, x), p(mnu, *a)]), b.neg(b.mul(&[p(mnu, x), p(nu, *a)]))]);
                let csc = b.call(b.hd.csc, &[b.mul(&[nu, pi(b)])]);
                Some(b.add(&[b.mul(&[k, p(nu, x), p(mnu, *a)]), b.mul(&[b.rat(1, 2), pi(b), d, i, csc])]))
            }
        }
        _ => None,
    })
}

/// `Ai(c z)`, `Bi(c z)` for a primitive cube root of unity `c` (DLMF 9.2.11): the marker of the rotation.
fn airy_rotation<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [a] = args else { return Ok(None) };
    let fs = b.factors(*a);
    let (nums, rest): (Vec<T<'h>>, Vec<T<'h>>) = fs.into_iter().partition(|&u| closed_number(b, u));
    if nums.is_empty() || rest.is_empty() {
        return Ok(None);
    }
    let c = b.mul(&nums);
    let cube = cx.eval(b.call_named("expand", &[b.powi(c, 3)]))?;
    if cube != b.int(1) {
        return Ok(None);
    }
    let im = cx.eval(b.call_named("n", &[b.call_named("im_part", &[c])]))?;
    let up = match b.heap().view(im) {
        View::Real(v) if v.abs() > 0.1 => v > 0.0,
        _ => return Ok(None),
    };
    let z = b.mul(&rest);
    // c = e^(-2 pi i/3) (im < 0): Ai(c z) = e^(-pi i/3) (Ai(z) + i Bi(z))/2; c = e^(2 pi i/3): the conjugate;
    // Bi(c z) = e^(+-pi i/6) (Ai(z) -+ i Bi(z)) * (sqrt 3 ... ) is left alone
    if head != "airy_ai" {
        return Ok(None);
    }
    let s3 = b.pow(b.int(3), b.rat(1, 2));
    let i = b.i();
    let (w, sgn) = if up { (b.add(&[b.int(1), b.mul(&[i, s3])]), -1) } else { (b.add(&[b.int(1), b.mul(&[b.neg(i), s3])]), 1) };
    let ai = b.call_named("airy_ai", &[z]);
    let bi = b.call_named("airy_bi", &[z]);
    Ok(Some(b.mul(&[b.rat(1, 4), w, b.add(&[ai, b.mul(&[b.complex(b.int(0), b.int(sgn)), bi])])])))
}

fn closed_number<'h>(b: &B<'h>, t: T<'h>) -> bool {
    if b.is_number(t) {
        return true;
    }
    match b.call_of(t) {
        Some((p, &[x, e])) if p == b.hd.pow => b.is_number(x) && b.is_number(e),
        _ => false,
    }
}

/// `zeta(1 - s) = 2^(1 - s) pi^-s sin(pi (1 - s)/2) Gamma(s) zeta(s)` (DLMF 25.4.2) for an argument written
/// `1 - s` (a leading minus on its variable part).
fn zeta_reflection<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [w] = args else { return Ok(None) };
    if b.is_number(*w) {
        return Ok(None);
    }
    let ts = b.terms(*w);
    if !ts.contains(&b.int(1)) {
        return Ok(None);
    }
    let var: Vec<T<'h>> = ts.iter().copied().filter(|&t| t != b.int(1)).collect();
    let v = b.add(&var);
    if !neg_form(b, v) {
        return Ok(None);
    }
    let s = negate(b, v);
    let r = b.mul(&[
        b.call_named("gamma", &[s]),
        b.pow(b.int(2), minus(b, b.int(1), s)),
        b.pow(pi(b), b.neg(s)),
        b.call(b.hd.sin, &[b.mul(&[b.rat(1, 2), pi(b), *w])]),
        b.call_named("zeta", &[s]),
    ]);
    Ok(Some(r))
}

/// `pFq` reductions: equal upper and lower parameters cancelled (DLMF 16.2), then the named cases.
fn hyper_pfq<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [up, lo, z] = args else { return Ok(None) };
    let (Some((l1, us)), Some((l2, ls))) = (b.call_of(*up), b.call_of(*lo)) else { return Ok(None) };
    if l1 != b.hd.list || l2 != b.hd.list {
        return Ok(None);
    }
    let mut us = us.to_vec();
    let mut ls = ls.to_vec();
    let mut cancelled = false;
    let mut i = 0;
    while i < us.len() {
        if let Some(j) = ls.iter().position(|&l| l == us[i]) {
            us.remove(i);
            ls.remove(j);
            cancelled = true;
        } else {
            i += 1;
        }
    }
    let lst = |v: &[T<'h>]| b.call(b.hd.list, v);
    // 3F2(1/2, 1/2, 1/2; 1, 1; z) = (2 K(m)/pi)^2 (Clausen's formula with the quadratic transformation of
    // 2F1(1/4, 1/4; 1; .), DLMF 16.12.1, 15.8.13, 19.5.1) and its contiguous neighbour 3F2(-1/2, 1/2, 1/2; 1, 1; z)
    // = 4 (K^2 + 2 E^2 - 2 E K)/pi^2, m = (1 - sqrt(1 - z))/2 (Prudnikov, Brychkov and Marichev vol. 3, 7.4.1)
    if let ([a1, a2, a3], [c1, c2]) = (us.as_slice(), ls.as_slice()) {
        let half = b.rat(1, 2);
        let mut ups = [*a1, *a2, *a3];
        cx.sort(&mut ups);
        if *c1 == b.int(1) && *c2 == b.int(1) && ups[1] == half && ups[2] == half && (ups[0] == half || ups[0] == b.rat(-1, 2)) {
            let m = b.add(&[half, b.mul(&[b.rat(-1, 2), b.pow(minus(b, b.int(1), *z), half)])]);
            let k = b.call_named("elliptic_k", &[m]);
            let e = b.call_named("elliptic_e", &[m]);
            let ip2 = b.powi(pi(b), -2);
            let r = if ups[0] == half {
                b.mul(&[b.int(4), b.powi(k, 2), ip2])
            } else {
                b.add(&[b.mul(&[b.int(4), b.powi(k, 2), ip2]), b.mul(&[b.int(8), b.powi(e, 2), ip2]), b.mul(&[b.int(-8), e, k, ip2])])
            };
            return Ok(Some(r));
        }
    }
    // 1F2(nu + 1/2; nu + 1, 2 nu + 1; w) = Gamma(nu + 1)^2 (w/4)^-nu I_nu(sqrt w)^2 (DLMF 10.8.3 for I by 10.39)
    if let ([a], [c1, c2]) = (us.as_slice(), ls.as_slice()) {
        let nu = minus(b, *a, b.rat(1, 2));
        let (p1, p2) = (b.add(&[nu, b.int(1)]), b.add(&[b.mul(&[b.int(2), nu]), b.int(1)]));
        if (*c1 == p1 && *c2 == p2) || (*c1 == p2 && *c2 == p1) {
            let i = b.call_named("bessel_i", &[nu, b.pow(*z, b.rat(1, 2))]);
            let r = b.mul(&[b.powi(b.call_named("gamma", &[p1]), 2), b.pow(b.mul(&[b.rat(1, 4), *z]), b.neg(nu)), b.powi(i, 2)]);
            return Ok(Some(r));
        }
    }
    match (us.len(), ls.len()) {
        (0, 0) => Ok(Some(b.exp(*z))),
        (1, 0) => Ok(Some(b.pow(minus(b, b.int(1), *z), b.neg(us[0])))),
        (0, 1) => Ok(Some(b.call_named("hypergeometric_0f1", &[ls[0], *z]))),
        (1, 1) => Ok(Some(hyper_1f1(cx, b, f, "", &[us[0], ls[0], *z])?.unwrap_or_else(|| b.call_named("hypergeometric1f1", &[us[0], ls[0], *z])))),
        (2, 1) => Ok(Some(hyper_2f1(cx, b, f, "", &[us[0], us[1], ls[0], *z])?.unwrap_or_else(|| b.call_named("hypergeometric2f1", &[us[0], us[1], ls[0], *z])))),
        _ if cancelled => Ok(Some(b.call_named("hypergeometric_pfq", &[lst(&us), lst(&ls), *z]))),
        _ => Ok(None),
    }
}

/// `0F1(; b; z) = Gamma(b) z^((1 - b)/2) I_(b - 1)(2 sqrt z)` (DLMF 10.39.9).
fn hyper_0f1<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [c, z] = args else { return Ok(None) };
    let nu = b.add(&[*c, b.int(-1)]);
    let r = b.mul(&[
        b.call_named("gamma", &[*c]),
        b.pow(*z, b.mul(&[b.rat(-1, 2), nu])),
        b.call_named("bessel_i", &[nu, b.mul(&[b.int(2), b.pow(*z, b.rat(1, 2))])]),
    ]);
    Ok(Some(r))
}

/// `1F1(a; a; z) = e^z`, `1F1(1; 2; z) = (e^z - 1)/z` (DLMF 13.6.1, 13.6.2 cases).
fn hyper_1f1<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [a, c, z] = args else { return Ok(None) };
    if a == c {
        return Ok(Some(b.exp(*z)));
    }
    if *a == b.int(1) && *c == b.int(2) {
        return Ok(Some(b.div(b.add(&[b.exp(*z), b.int(-1)]), *z)));
    }
    Ok(None)
}

/// `2F1(a, b; b; z) = (1 - z)^-a`, `2F1(1, 1; 2; z) = -log(1 - z)/z` (DLMF 15.4.1, 15.4.6).
fn hyper_2f1<'h>(_cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [a1, a2, c, z] = args else { return Ok(None) };
    if a2 == c {
        return Ok(Some(b.pow(minus(b, b.int(1), *z), b.neg(*a1))));
    }
    if a1 == c {
        return Ok(Some(b.pow(minus(b, b.int(1), *z), b.neg(*a2))));
    }
    if *a1 == b.int(1) && *a2 == b.int(1) && *c == b.int(2) {
        return Ok(Some(b.neg(b.div(b.call(b.hd.log, &[minus(b, b.int(1), *z)]), *z))));
    }
    Ok(None)
}

/// The Ferrers function `Q^(1/2)_nu(x)` for `nu = n + 1/2` (`0 <= n <= 10`): `-(n + 1) sqrt(pi) ((1 - x)/(1 + x))^(1/4)
/// sqrt(1 - w) P(w)`, `w = (1 - x)/2`, `P = 2F1(-n, n + 2; 3/2; w)` a polynomial (DLMF 14.3.12 at `mu = 1/2` with
/// Euler's transformation 15.8.1; equal to DLMF 14.5.17).
fn legendre_q_half<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, _f: &mut Facts<'_, 'h>, _head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [nu, mu, x] = args else { return Ok(None) };
    let half = b.rat(1, 2);
    if *mu != half {
        return Ok(None);
    }
    let Some(n) = int_of(b, minus(b, *nu, half)) else { return Ok(None) };
    if !(0..=10).contains(&n) {
        return Ok(None);
    }
    // P(w) = sum_k (-n)_k (n + 2)_k / ((3/2)_k k!) w^k
    let w = b.mul(&[half, minus(b, b.int(1), *x)]);
    let mut terms = vec![];
    let mut c = b.int(1);
    for k in 0..=n {
        terms.push(b.mul(&[c, b.powi(w, k)]));
        let kk = b.int(k);
        let num = b.mul(&[b.add(&[b.int(-n), kk]), b.add(&[b.int(n + 2), kk])]);
        let den = b.mul(&[b.add(&[b.rat(3, 2), kk]), b.add(&[kk, b.int(1)])]);
        c = b.div(b.mul(&[c, num]), den);
    }
    let p = cx.eval(b.call_named("expand", &[b.add(&terms)]))?;
    let one_minus_w = b.add(&[b.int(1), b.mul(&[half, b.add(&[b.int(-1), *x])])]);
    let r = b.mul(&[
        b.int(-(n + 1)),
        b.pow(pi(b), half),
        p,
        b.pow(minus(b, b.int(1), *x), b.rat(1, 4)),
        b.pow(one_minus_w, half),
        b.pow(b.add(&[b.int(1), *x]), b.rat(-1, 4)),
    ]);
    Ok(Some(r))
}

fn fib(k: i64) -> (i64, i64) {
    // (F_k, L_k) for |k| <= 90
    let (mut f0, mut f1) = (0i64, 1i64);
    for _ in 0..k.abs() {
        let t = f0 + f1;
        f0 = f1;
        f1 = t;
    }
    let (fk, lk) = (f0, 2 * f1 - f0);
    if k < 0 && k % 2 == 0 {
        (-fk, lk)
    } else if k < 0 {
        (fk, -lk)
    } else {
        (fk, lk)
    }
}

/// `F(n + k) = (F_k L_n + L_k F_n)/2`, `L(n + k) = (5 F_k F_n + L_k L_n)/2` for an integer `n` (from Binet's
/// formulas).
fn fibonacci_shift<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, f: &mut Facts<'_, 'h>, head: &str, args: &[T<'h>]) -> Res<Option<T<'h>>> {
    let [u] = args else { return Ok(None) };
    let ts = b.terms(*u);
    let Some(k) = ts.iter().find_map(|&t| int_of(b, t)) else { return Ok(None) };
    if k == 0 || k.abs() > 60 {
        return Ok(None);
    }
    let n = b.add(&ts.iter().copied().filter(|&t| int_of(b, t).is_none()).collect::<Vec<_>>());
    if !is_integer(cx, f, n)? {
        return Ok(None);
    }
    let (fk, lk) = fib(k);
    let (fl, ll) = (b.call_named("fibonacci", &[n]), b.call_named("lucas_l", &[n]));
    Ok(Some(if head == "fibonacci" {
        b.add(&[b.mul(&[b.rat(fk, 2), ll]), b.mul(&[b.rat(lk, 2), fl])])
    } else {
        b.add(&[b.mul(&[b.rat(5 * fk, 2), fl]), b.mul(&[b.rat(lk, 2), ll])])
    }))
}

// ---------------------------------------------------------------------------------------------------------------
// the families

/// `gamma(u + k)` (or `polygamma(n, u + k)`) terms of a sum whose arguments differ by integers, all written by the
/// lowest argument `u + k0`: gamma by the recurrence (DLMF 5.5.1), the coefficient of `gamma(u + k0)` collected and
/// expanded; polygamma by DLMF 5.15.5, `psi^(n)(z + 1) = psi^(n)(z) + (-1)^n n! z^(-n - 1)`.
fn shift_family<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, which: &str) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let g = h.sym(which);
    let poly = which == "polygamma";
    // (term index, coefficient, order n for polygamma, argument)
    let terms = b.terms(t);
    let mut items: Vec<(usize, T<'h>, T<'h>, T<'h>)> = vec![];
    for (i, &u) in terms.iter().enumerate() {
        let fs = b.factors(u);
        let pos: Vec<usize> = fs.iter().enumerate().filter(|(_, &x)| matches!(b.call_of(x), Some((s, _)) if s == g)).map(|(j, _)| j).collect();
        let [j] = pos.as_slice() else { continue };
        let Some((_, gargs)) = b.call_of(fs[*j]) else { continue };
        let (n, arg) = match (poly, gargs) {
            (false, [a]) => (b.int(0), *a),
            (true, [n, a]) if int_of(b, *n).is_some_and(|n| (0..=20).contains(&n)) => (*n, *a),
            _ => continue,
        };
        let coef = b.mul(&fs.iter().enumerate().filter(|(k, _)| k != j).map(|(_, &x)| x).collect::<Vec<_>>());
        items.push((i, coef, n, arg));
    }
    // a group: two or more terms of one order whose arguments differ by integers
    for a in 0..items.len() {
        let group: Vec<usize> = (0..items.len())
            .filter(|&c| items[c].2 == items[a].2 && crate::powers::int_difference(b, items[c].3, items[a].3).is_some_and(|k| k.abs() <= 32))
            .collect();
        if group.len() < 2 {
            continue;
        }
        // the lowest argument
        let low = *group.iter().min_by_key(|&&c| crate::powers::int_difference(b, items[c].3, items[a].3).unwrap_or(0)).unwrap_or(&a);
        let base = items[low].3;
        let n = items[low].2;
        let mut out: Vec<T<'h>> = terms.iter().enumerate().filter(|(i, _)| !group.iter().any(|&c| items[c].0 == *i)).map(|(_, &x)| x).collect();
        let mut coef = Vec::new();
        let mut extra = Vec::new();
        for &c in &group {
            let k = crate::powers::int_difference(b, items[c].3, base).unwrap_or(0);
            if poly {
                // psi^(n)(z + k) = psi^(n)(z) + (-1)^n n! sum_(j < k) (z + j)^(-n - 1)
                coef.push(items[c].1);
                if k > 0 {
                    let nn = int_of(b, n).unwrap_or(0);
                    let mut fact = 1i64;
                    for j in 2..=nn {
                        fact = fact.saturating_mul(j);
                    }
                    let s = if nn % 2 == 0 { fact } else { -fact };
                    let sum: Vec<T<'h>> = (0..k).map(|j| b.powi(b.add(&[base, b.int(j)]), -nn - 1)).collect();
                    extra.push(b.mul(&[items[c].1, b.int(s), b.add(&sum)]));
                }
            } else {
                // gamma(z + k) = z (z + 1) ... (z + k - 1) gamma(z)
                let fs: Vec<T<'h>> = (0..k).map(|j| b.add(&[base, b.int(j)])).collect();
                coef.push(b.mul(&[items[c].1, b.mul(&fs)]));
            }
        }
        let c = cx.eval(b.call_named("expand", &[b.add(&coef)]))?;
        let head = if poly { b.call(g, &[n, base]) } else { b.call(g, &[base]) };
        out.push(b.mul(&[c, head]));
        let extra = if extra.len() == 1 { extra } else { vec![b.add(&extra)] };
        out.extend(extra);
        return Ok(Some(b.add(&out)));
    }
    Ok(None)
}

/// `c Li2(z) + c Li2(1 - z) = c (pi^2/6 - log(z) log(1 - z))` (DLMF 25.12.6).
fn polylog_reflection<'h>(b: &B<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let li = h.sym("polylog");
    let terms = b.terms(t);
    let parts: Vec<Option<(T<'h>, T<'h>)>> = terms
        .iter()
        .map(|&u| {
            let (c, l) = match b.call_of(u) {
                Some((m, &[c, l])) if m == b.hd.mul && b.is_number(c) => (c, l),
                _ => (b.int(1), u),
            };
            match b.call_of(l) {
                Some((s, &[two, z])) if s == li && two == b.int(2) => Some((c, z)),
                _ => None,
            }
        })
        .collect();
    for i in 0..parts.len() {
        let Some((c, z)) = parts[i] else { continue };
        let w = minus(b, b.int(1), z);
        let Some(j) = (0..parts.len()).find(|&j| j != i && parts[j] == Some((c, w))) else { continue };
        let (z, w) = if neg_form(b, z) { (w, z) } else { (z, w) };
        let mut out: Vec<T<'h>> = terms.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &x)| x).collect();
        let v = b.add(&[b.mul(&[b.rat(1, 6), b.powi(pi(b), 2)]), b.neg(b.mul(&[b.call(b.hd.log, &[z]), b.call(b.hd.log, &[w])]))]);
        out.push(b.mul(&[c, v]));
        return Some(b.add(&out));
    }
    None
}

/// `c I_-nu(z) - c I_nu(z) = 2 c sin(nu pi) K_nu(z)/pi` (DLMF 10.27.4).
fn bessel_i_difference<'h>(b: &B<'h>, t: T<'h>) -> Option<T<'h>> {
    let h = b.heap();
    let bi = h.sym("bessel_i");
    let terms = b.terms(t);
    let parts: Vec<Option<(T<'h>, T<'h>, T<'h>)>> = terms
        .iter()
        .map(|&u| {
            let (c, l) = match b.call_of(u) {
                Some((m, &[c, l])) if m == b.hd.mul && b.is_number(c) => (c, l),
                _ => (b.int(1), u),
            };
            match b.call_of(l) {
                Some((s, &[nu, z])) if s == bi => Some((c, nu, z)),
                _ => None,
            }
        })
        .collect();
    for i in 0..parts.len() {
        let Some((c, nu, z)) = parts[i] else { continue };
        if neg_form(b, nu) {
            continue;
        }
        let Some(j) = (0..parts.len()).find(|&j| parts[j] == Some((b.neg(c), negate(b, nu), z))) else { continue };
        let mut out: Vec<T<'h>> = terms.iter().enumerate().filter(|&(k, _)| k != i && k != j).map(|(_, &x)| x).collect();
        let k = b.call_named("bessel_k", &[nu, z]);
        let s = b.call(b.hd.sin, &[b.mul(&[nu, pi(b)])]);
        out.push(b.mul(&[b.int(-2), c, s, k, b.powi(pi(b), -1)]));
        return Some(b.add(&out));
    }
    None
}
