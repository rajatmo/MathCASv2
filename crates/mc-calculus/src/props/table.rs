//! What each special function is (round 37): the knowledge the function properties read, kept once as data.
//!
//! An [`Entry`] describes one function of one argument (the *variable argument*; the others are parameters, fixed
//! unless the entry holds jointly in all its arguments): where it is defined and real on the real line, where it is
//! defined in the complex plane, its singular set on the line and in the plane as atoms of a kind (pole, branch,
//! jump, kink: [`super::sets`]), its range on the reals and in the plane, and whether it is bounded on the reals.
//! The facts are templates in `$u` (the variable argument), `$a`, `$b`, ... (the other arguments in order) and `$y`
//! (the value), read by the reader and instantiated by substitution; a template may name the parameters, so one
//! entry covers every parameter value (`not_element($a, integers) and $u <= 0`: a branch cut only for a
//! non-integer order). Every property reads the same entry: the domain ([`super::conditions`]), the singular sets
//! (singularities, discontinuities, analyticity, continuity, meromorphy: [`super::sets`]), the range
//! ([`super::range`]), surjectivity (a bounded function is not onto); the sign, monotonicity, convexity and
//! injectivity sample the function on the domain read here. Compositions are taken apart by the callers: a
//! subterm contributes its conditions with its own argument substituted.
//!
//! Conventions: `real` is the set where the function is defined and real-valued (`""` unknown; `"~"` the
//! complement of the real singular atoms, for functions whose only singularities are poles); a singular atom `"?"`
//! stands for a set of points where the function is continuous but not analytic that is not listed (the Cantor
//! function): continuity is decided, analyticity is not; `partial` marks atoms that are some of the singular
//! points only (a pole of the Weierstrass functions at 0): analyticity is decided when they meet the region, the
//! sets are not listed. Ranges are `(condition on the parameters, range)` pairs, the first that holds.
//!
//! The facts are from the NIST Digital Library of Mathematical Functions (DLMF; chapter and equation in the
//! `refs` of each entry), with the hypergeometric families read through their series (DLMF 16.2: `p <= q` entire,
//! `p = q + 1` the cut `[1, inf)` and convergence at 1 when `re(sum b - sum a) > 0`), and the classical orthogonal
//! families of non-integer degree through their hypergeometric forms at `(1 - z)/2` (DLMF 15.9, 18.5.7, 14.3.1).
use super::sets::{Kind, Mode};
use crate::common::is_free;
use crate::term::Build;
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};

/// Boundedness on the real domain.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bound {
    /// not known (or not bounded)
    No,
    /// bounded above
    Above,
    /// bounded below
    Below,
    /// bounded
    Both,
}

/// One function (module comment).
pub struct Entry {
    /// the function's name
    pub name: &'static str,
    /// the number of arguments
    pub arity: usize,
    /// the index of the variable argument
    pub var: usize,
    /// the facts hold jointly in all arguments (the parameters may be variables too)
    pub joint: bool,
    /// a condition on the parameters for the entry to apply (`""`: always)
    pub when: &'static str,
    /// defined and real on the real line
    pub real: &'static str,
    /// defined in the complex plane
    pub complex: &'static str,
    /// the singular atoms on the real line
    pub sr: Option<&'static [(&'static str, Kind)]>,
    /// the singular atoms in the plane
    pub sc: Option<&'static [(&'static str, Kind)]>,
    /// the real atoms are some singular points only
    pub partial: bool,
    /// the complex atoms are some singular points only
    pub cpartial: bool,
    /// smooth where defined, apart from the listed atoms (`false`: kinks or jumps, decided by samples of the
    /// function itself)
    pub smooth: bool,
    /// ranges on the reals: (condition on the parameters, condition on `$y`)
    pub ranges: &'static [(&'static str, &'static str)],
    /// the range in the plane (a condition on `$y`)
    pub crange: &'static str,
    /// boundedness on the real domain
    pub bound: Bound,
    /// onto the reals: (condition on the parameters, answer), the first that holds
    pub onto: &'static [(&'static str, bool)],
    /// a condition on the parameters under which the function is periodic in the variable (not injective)
    pub periodic: &'static str,
    /// the strict sign on the real domain: (condition on the parameters, 1 positive or -1 negative), the first that
    /// holds
    pub signs: &'static [(&'static str, i8)],
    /// strictly increasing (1) or decreasing (-1) on the real domain (an interval), the first condition that holds
    pub mono: &'static [(&'static str, i8)],
    /// strictly convex (1) or concave (-1) on the real domain (an interval), the first condition that holds
    pub convex: &'static [(&'static str, i8)],
    /// the literature (the record of where each fact comes from; not read by the code)
    #[allow(dead_code)]
    pub refs: &'static str,
}

const E: Entry = Entry {
    name: "",
    arity: 1,
    var: 0,
    joint: false,
    when: "",
    real: "",
    complex: "",
    sr: None,
    sc: None,
    partial: false,
    cpartial: false,
    smooth: true,
    ranges: &[],
    crange: "",
    bound: Bound::No,
    onto: &[],
    periodic: "",
    signs: &[],
    mono: &[],
    convex: &[],
    refs: "",
};

use Kind::{Branch as BR, Jump as JU, Kink as KI, Pole as PO};

/// The branch point at 0 and the cut along the nonpositive reals.
const LOG_CUT: &[(&str, Kind)] = &[("$u == 0", BR), ("im_part($u) == 0 and re($u) <= 0", BR)];
/// Entire: no singular atom.
const NONE: &[(&str, Kind)] = &[];
/// Defined and real on the whole line, defined in the whole plane, entire.
const ENTIRE: Entry = Entry { real: "true", complex: "true", sr: Some(NONE), sc: Some(NONE), ..E };
/// Defined on the positive reals and off 0 in the plane, with the cut of the logarithm.
const POSITIVE: Entry = Entry { real: "$u > 0", complex: "$u != 0", sr: Some(&[("$u <= 0", BR)]), sc: Some(LOG_CUT), ..E };

/// The table.
pub static TABLE: &[Entry] = &[
    // exponential, logarithmic, sine and cosine integrals (DLMF 6.2.5-6.2.16, 6.4; the maxima of Ci and Si: 6.2.11,
    // 6.2.9 with Ci'(x) = cos(x)/x, Si'(x) = sin(x)/x and lobes of decreasing size)
    Entry { name: "cos_integral", ranges: &[("", "$y <= cos_integral(pi/2)")], bound: Bound::Above, refs: "DLMF 6.2.11, 6.4.2", ..POSITIVE },
    Entry { name: "cosh_integral", mono: &[("", 1)], ranges: &[("", "true")], refs: "DLMF 6.2.16, 6.4.5", ..POSITIVE },
    Entry {
        name: "exp_integral_ei",
        real: "$u != 0",
        complex: "$u != 0",
        sr: Some(&[("$u == 0", BR)]),
        sc: Some(LOG_CUT),
        ranges: &[("", "true")],
        refs: "DLMF 6.2.5, 6.2.6",
        ..E
    },
    Entry {
        name: "log_integral",
        real: "$u >= 0 and $u != 1",
        complex: "$u != 1",
        sr: Some(&[("$u <= 0", BR), ("$u == 1", BR)]),
        ranges: &[("", "true")],
        refs: "DLMF 6.2.8",
        ..E
    },
    Entry { name: "sin_integral", ranges: &[("", "-sin_integral(pi) <= $y and $y <= sin_integral(pi)")], bound: Bound::Both, refs: "DLMF 6.2.9", ..ENTIRE },
    Entry { name: "sinh_integral", ranges: &[("", "true")], refs: "DLMF 6.2.15", ..ENTIRE },
    // generalized exponential integral E_n(z) = z^(n-1) Gamma(1-n, z): a pole at 0 for n = 0, -1, ..., otherwise the
    // cut of the logarithm (DLMF 8.19.1, 8.19.7, 8.19.8)
    Entry {
        name: "exp_integral_e",
        mono: &[("not(element($a, integers) and $a <= 0)", -1)],
        ranges: &[("element(($a + 1)/2, integers) and $a < 0", "true")],
        arity: 2,
        var: 1,
        joint: true,
        real: "$u > 0 or $u == 0 and $a > 1 or element($a, integers) and $a <= 0 and $u != 0",
        complex: "$u != 0 or re($a) > 1",
        sr: Some(&[("not(element($a, integers) and $a <= 0) and $u <= 0", BR), ("element($a, integers) and $a <= 0 and $u == 0", PO)]),
        sc: Some(&[
            ("not(element($a, integers) and $a <= 0) and $u == 0", BR),
            ("not(element($a, integers) and $a <= 0) and im_part($u) == 0 and re($u) <= 0", BR),
            ("element($a, integers) and $a <= 0 and $u == 0", PO),
        ]),
        refs: "DLMF 8.19.1, 8.19.7",
        ..E
    },
    // Airy and Scorer functions: entire (DLMF 9.2, 9.12.4-5); Ai, Gi bounded on the line and Bi bounded below
    // (9.7.5-9.7.11, 9.12.27-31); the derivatives oscillate with amplitude |x|^(1/4) (9.7.10, 9.7.12); Hi, Hi'
    // positive from 0 at -inf to inf (9.12.(viii) and the integral 9.12.20)
    Entry { name: "airy_ai", bound: Bound::Both, refs: "DLMF 9.7.5, 9.7.9", ..ENTIRE },
    Entry { name: "airy_bi", bound: Bound::Below, refs: "DLMF 9.7.7, 9.7.11", ..ENTIRE },
    Entry { name: "airy_ai_prime", ranges: &[("", "true")], refs: "DLMF 9.7.10", ..ENTIRE },
    Entry { name: "airy_bi_prime", ranges: &[("", "true")], refs: "DLMF 9.7.12", ..ENTIRE },
    Entry { name: "scorer_gi", bound: Bound::Both, refs: "DLMF 9.12.4, 9.12.27, 9.12.31", ..ENTIRE },
    Entry { name: "scorer_gi_prime", ranges: &[("", "true")], refs: "DLMF 9.12.4, 9.12.31", ..ENTIRE },
    Entry { name: "scorer_hi", signs: &[("", 1)], mono: &[("", 1)], convex: &[("", 1)], ranges: &[("", "$y > 0")], bound: Bound::Below, refs: "DLMF 9.12.5, 9.12.20, 9.12.30", ..ENTIRE },
    Entry { name: "scorer_hi_prime", signs: &[("", 1)], mono: &[("", 1)], convex: &[("", 1)], ranges: &[("", "$y > 0")], bound: Bound::Below, refs: "DLMF 9.12.5, 9.12.20", ..ENTIRE },
    // error functions and their inverses (DLMF 7.2, 7.5, 7.17)
    Entry { name: "dawson_f", bound: Bound::Both, refs: "DLMF 7.2.5, 7.8 (bounded: a maximum near 0.924, odd)", ..ENTIRE },
    Entry { name: "fresnel_c", ranges: &[("", "-fresnel_c(1) <= $y and $y <= fresnel_c(1)")], bound: Bound::Both, refs: "DLMF 7.2.7, 7.3", ..ENTIRE },
    Entry {
        name: "fresnel_s",
        ranges: &[("", "-fresnel_s(2^(1/2)) <= $y and $y <= fresnel_s(2^(1/2))")],
        bound: Bound::Both,
        refs: "DLMF 7.2.8, 7.3",
        ..ENTIRE
    },
    Entry { name: "fresnel_f", bound: Bound::Both, refs: "DLMF 7.2.10", ..ENTIRE },
    Entry { name: "fresnel_g", bound: Bound::Both, refs: "DLMF 7.2.11", ..ENTIRE },
    Entry { name: "inverse_erf", mono: &[("", 1)], real: "-1 < $u and $u < 1", sr: Some(&[("$u <= -1", BR), ("$u >= 1", BR)]), ranges: &[("", "true")], refs: "DLMF 7.17.1", ..E },
    Entry { name: "inverse_erfc", mono: &[("", -1)], real: "0 < $u and $u < 2", sr: Some(&[("$u <= 0", BR), ("$u >= 2", BR)]), ranges: &[("", "true")], refs: "DLMF 7.17.1", ..E },
    // the inverses of the regularized incomplete gamma and beta functions on their principal branches (x >= 0;
    // 0 <= x <= 1): Q(a, x) decreases from 1 to 0 on x >= 0 for a > 0, I_x(a, b) increases from 0 to 1 (DLMF 8.2.4,
    // 8.4, 8.17.2, 8.17.(ii))
    Entry {
        name: "inverse_gamma_regularized",
        mono: &[("$a > 0", -1)],
        arity: 2,
        var: 1,
        joint: true,
        real: "$a > 0 and 0 < $u and $u <= 1",
        sr: Some(&[("$a <= 0", BR), ("$u <= 0", BR), ("$u >= 1", BR)]),
        ranges: &[("", "$a > 0 and $y >= 0")],
        onto: &[("$a > 0", false)],
        refs: "DLMF 8.2.4, 8.4",
        ..E
    },
    Entry {
        name: "inverse_beta_regularized",
        mono: &[("", 1)],
        arity: 3,
        when: "$a > 0 and $b > 0",
        real: "0 <= $u and $u <= 1",
        // near 0 the inverse is (c s)^(1/a) times a series, near 1 likewise with b: branch points unless 1/a (1/b)
        // is an integer
        sr: Some(&[("not_element(1/$a, integers) and $u == 0", BR), ("not_element(1/$b, integers) and $u == 1", BR)]),
        ranges: &[("", "0 <= $y and $y <= 1")],
        bound: Bound::Both,
        refs: "DLMF 8.17.2, 8.17.(ii)",
        ..E
    },
    // the Gudermannian function and its inverse, the logistic function (DLMF 4.23.39-4.23.42)
    Entry { name: "gudermannian", mono: &[("", 1)], ranges: &[("", "-pi/2 < $y and $y < pi/2")], bound: Bound::Both, refs: "DLMF 4.23.39", ..E },
    Entry { name: "inverse_gudermannian", real: "cos($u) > 0", sr: Some(&[("cos($u) <= 0", BR)]), sc: Some(&[("$u == pi/2", BR)]), cpartial: true, ranges: &[("", "true")], refs: "DLMF 4.23.41", ..E },
    Entry { name: "logistic_sigmoid", complex: "1 + exp($u) != 0", ranges: &[("", "0 < $y and $y < 1")], crange: "$y - 1 != 0 and $y != 0", bound: Bound::Both, ..E },
    // the Lambert W function: principal branch, real from -1/e, the cut (-inf, -1/e] (DLMF 4.13)
    Entry {
        name: "product_log",
        real: "$u >= -1/e",
        mono: &[("", 1)],
        convex: &[("", -1)],
        complex: "true",
        sr: Some(&[("$u <= -1/e", BR)]),
        sc: Some(&[("$u == -1/e", BR), ("im_part($u) == 0 and re($u) <= -1/e", BR)]),
        ranges: &[("", "$y >= -1")],
        bound: Bound::Below,
        refs: "DLMF 4.13.1",
        ..E
    },
    Entry { name: "product_log", arity: 2, var: 1, complex: "element($a, integers) and ($a == 0 or $u != 0)", refs: "DLMF 4.13", ..E },
    // gamma-type functions: Barnes G entire (DLMF 5.17.1), log G off the zeros of G; the double factorial
    // 2^(z/2) (2/pi)^((1 - cos(pi z))/4) Gamma(z/2 + 1) with poles at the negative even integers; the hyperfactorial
    // real from -1 and at the integers; the alternating factorial with poles at the integers below -1
    Entry { name: "barnes_g", ranges: &[("", "true")], refs: "DLMF 5.17.1-5.17.3", ..ENTIRE },
    Entry {
        name: "log_barnes_g",
        real: "$u > 0",
        complex: "re($u) > 0 or not_element($u, integers)",
        sr: Some(&[("$u <= 0", BR)]),
        ranges: &[("", "true")],
        refs: "DLMF 5.17.1, 5.17.4",
        ..E
    },
    Entry {
        name: "factorial2",
        real: "$u >= 0 or not_element($u/2, integers)",
        complex: "re($u) >= 0 or not_element($u/2, integers)",
        sr: Some(&[("sin(pi*$u/2) == 0 and $u <= -2", PO)]),
        sc: Some(&[("sin(pi*$u/2) == 0 and re($u) <= -2", PO)]),
        refs: "DLMF 5.2.1 (double factorial by the gamma function)",
        ..E
    },
    Entry { name: "hyperfactorial", real: "element($u, integers) or $u >= -1", complex: "true", sr: Some(&[("$u <= -1", BR)]), sc: Some(&[("$u == -1", BR)]), cpartial: true, bound: Bound::Below, refs: "the K-function, DLMF 5.17", ..E },
    Entry {
        name: "alternating_factorial",
        real: "$u > -2 or not_element($u, integers)",
        complex: "re($u) > -2 or not_element($u, integers)",
        sr: Some(&[("sin(pi*$u) == 0 and $u <= -2", PO)]),
        sc: Some(&[("sin(pi*$u) == 0 and re($u) <= -2", PO)]),
        refs: "the gamma function, DLMF 5.2",
        ..E
    },
    // functions of an integer: complex-valued off the integers (subfactorial: Gamma(z + 1, -1)/e, entire in z, DLMF
    // 8.2.(ii)); the alternating harmonic numbers (a factor (-1)^z); the tau function (integers only)
    Entry { name: "subfactorial", real: "element($u, integers) and $u >= 0", complex: "true", sr: Some(&[("true", BR)]), sc: Some(NONE), refs: "DLMF 8.2.(ii)", ..E },
    Entry { name: "alternating_harmonic_number", real: "element($u, integers)", sr: Some(&[("true", BR)]), refs: "DLMF 25.11", ..E },
    Entry { name: "ramanujan_tau", real: "element($u, integers)", complex: "element($u, integers)", sr: Some(&[("true", BR)]), refs: "DLMF 27.14.20", ..E },
    // elliptic integrals and the nome (DLMF 19.2.4-19.2.8, 19.6, 19.7.9, 22.2.1-22.2.2)
    Entry {
        name: "elliptic_k",
        signs: &[("", 1)],
        mono: &[("", 1)],
        convex: &[("", 1)],
        real: "$u < 1",
        complex: "$u != 1",
        sr: Some(&[("$u >= 1", BR)]),
        sc: Some(&[("$u == 1", BR), ("im_part($u) == 0 and re($u) >= 1", BR)]),
        ranges: &[("", "$y > 0")],
        bound: Bound::Below,
        refs: "DLMF 19.2.8, 19.6.1, 19.9.1",
        ..E
    },
    Entry {
        name: "elliptic_e",
        real: "$u <= 1",
        signs: &[("", 1)],
        mono: &[("", -1)],
        convex: &[("", -1)],
        complex: "true",
        sr: Some(&[("$u >= 1", BR)]),
        sc: Some(&[("$u == 1", BR), ("im_part($u) == 0 and re($u) >= 1", BR)]),
        ranges: &[("", "$y >= 1")],
        bound: Bound::Below,
        refs: "DLMF 19.2.8, 19.6.1",
        ..E
    },
    Entry { name: "elliptic_f", arity: 2, when: "$a < 1", real: "true", sr: Some(NONE), refs: "DLMF 19.2.4", ..E },
    Entry { name: "elliptic_e", arity: 2, when: "$a < 1", real: "true", sr: Some(NONE), refs: "DLMF 19.2.5", ..E },
    Entry {
        name: "elliptic_pi",
        arity: 2,
        joint: true,
        real: "$u != 1 and $a < 1",
        complex: "$u != 1 and $a != 1",
        sr: Some(&[("$u == 1", BR), ("$a >= 1", BR)]),
        refs: "DLMF 19.2.7, 19.6.5, 19.7.9",
        ..E
    },
    Entry { name: "elliptic_nome_q", mono: &[("", 1)], real: "$u <= 1", complex: "true", sr: Some(&[("$u >= 1", BR)]), refs: "DLMF 22.2.1, 19.2.8", ..E },
    Entry { name: "inverse_elliptic_nome_q", mono: &[("", 1)], real: "-1 < $u and $u <= 1", sr: Some(&[("$u <= -1", BR), ("$u >= 1", BR)]), refs: "DLMF 20.9.1, 22.2.2", ..E },
    // Jacobi elliptic functions of real x and m: sn, cn, dn real-analytic; the zeros of cn at (2k + 1) K(m) and of
    // sn at 2k K(m) for m < 1, of dn at (2k + 1) K(1/m)/sqrt(m) and of sn at 2k K(1/m)/sqrt(m) for m > 1 (Jacobi's
    // real transformation), sn = tanh and cn = dn = sech at m = 1 (DLMF 22.2, 22.4.1, 22.5 (special values), 22.17.4)
    Entry { name: "jacobi_sn", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, real: "true", sr: Some(NONE), refs: "DLMF 22.2.4, 22.17.4", ..E },
    Entry { name: "jacobi_cn", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, signs: &[("$a >= 1", 1)], real: "true", sr: Some(NONE), refs: "DLMF 22.2.5, 22.17.4", ..E },
    Entry { name: "jacobi_dn", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, signs: &[("$a <= 1", 1)], real: "true", sr: Some(NONE), refs: "DLMF 22.2.6, 22.17.4", ..E },
    Entry { name: "jacobi_nc", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, signs: &[("$a >= 1", 1)], real: "~", sr: Some(CN_ZEROS), refs: "DLMF 22.2.10, 22.4.1", ..E },
    Entry { name: "jacobi_sc", arity: 2, joint: true, onto: &[("$a <= 1", true), ("$a > 1", false)], periodic: PERIODIC_M, real: "~", sr: Some(CN_ZEROS), refs: "DLMF 22.2.10, 22.4.1", ..E },
    Entry { name: "jacobi_dc", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, real: "~", sr: Some(CN_ZEROS), refs: "DLMF 22.2.10, 22.4.1", ..E },
    Entry { name: "jacobi_nd", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, signs: &[("$a <= 1", 1)], real: "~", sr: Some(DN_ZEROS), refs: "DLMF 22.2.10, 22.17.4", ..E },
    Entry { name: "jacobi_sd", arity: 2, joint: true, onto: &[("$a < 1", false), ("$a >= 1", true)], periodic: PERIODIC_M, real: "~", sr: Some(DN_ZEROS), refs: "DLMF 22.2.10, 22.17.4", ..E },
    Entry { name: "jacobi_cd", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, real: "~", sr: Some(DN_ZEROS), refs: "DLMF 22.2.10, 22.17.4", ..E },
    Entry { name: "jacobi_ns", arity: 2, joint: true, onto: &[("element($a, reals)", false)], periodic: PERIODIC_M, real: "~", sr: Some(SN_ZEROS), refs: "DLMF 22.2.10, 22.4.1, 22.17.4", ..E },
    Entry { name: "jacobi_cs", arity: 2, joint: true, onto: &[("$a < 1", true), ("$a >= 1", false)], periodic: PERIODIC_M, real: "~", sr: Some(SN_ZEROS), refs: "DLMF 22.2.10, 22.4.1, 22.17.4", ..E },
    Entry { name: "jacobi_ds", arity: 2, joint: true, onto: &[("$a <= 1", false), ("$a > 1", true)], periodic: PERIODIC_M, real: "~", sr: Some(SN_ZEROS), refs: "DLMF 22.2.10, 22.4.1, 22.17.4", ..E },
    Entry { name: "jacobi_amplitude", arity: 2, when: "$a < 1", real: "true", sr: Some(NONE), ranges: &[("", "true")], refs: "DLMF 22.16.1", ..E },
    // theta functions are entire in z for a nome inside the unit disc (DLMF 20.2.1-20.2.4); Neville's for m < 1
    // (nome in (-1, 1), DLMF 20.1, 22.2.1)
    Entry { name: "elliptic_theta", arity: 3, var: 1, when: "abs($b) < 1", periodic: "true", bound: Bound::Both, refs: "DLMF 20.2.1", ..ENTIRE },
    Entry { name: "elliptic_theta_prime", arity: 3, var: 1, when: "abs($b) < 1", periodic: "true", bound: Bound::Both, refs: "DLMF 20.2.1", ..ENTIRE },
    Entry { name: "neville_theta_c", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    Entry { name: "neville_theta_d", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    Entry { name: "neville_theta_n", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    Entry { name: "neville_theta_s", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    // Weierstrass functions: sigma entire in z and the invariants (DLMF 23.2.6); p, p', zeta with a pole at 0
    // (and the other lattice points, not listed: DLMF 23.2.4-23.2.5)
    Entry { name: "weierstrass_sigma", arity: 2, joint: true, refs: "DLMF 23.2.6", ..ENTIRE },
    Entry { name: "weierstrass_p", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.4", ..E },
    Entry { name: "weierstrass_p_prime", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.4", ..E },
    Entry { name: "weierstrass_zeta", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.5", ..E },
    // modular functions of the upper half plane, the real line their natural boundary (DLMF 23.15, 27.14.12)
    Entry { name: "dedekind_eta", real: "false", complex: "im_part($u) > 0", sr: Some(&[("true", BR)]), sc: Some(&[("im_part($u) <= 0", BR)]), refs: "DLMF 27.14.12", ..E },
    Entry { name: "klein_invariant_j", real: "false", complex: "im_part($u) > 0", sr: Some(&[("true", BR)]), sc: Some(&[("im_part($u) <= 0", BR)]), refs: "DLMF 23.15.7", ..E },
    Entry { name: "modular_lambda", real: "false", complex: "im_part($u) > 0", sr: Some(&[("true", BR)]), sc: Some(&[("im_part($u) <= 0", BR)]), refs: "DLMF 23.15.6", ..E },
    // Bessel, Hankel, Kelvin, Struve, Anger-Weber functions (DLMF 10.2, 10.8, 10.25, 10.47, 10.61, 10.65, 11.2,
    // 11.10)
    Entry { name: "bessel_k", arity: 2, var: 1, onto: &[("element($a, reals)", false)], signs: &[("element($a, reals)", 1)], mono: &[("element($a, reals)", -1)], convex: &[("element($a, reals)", 1)], ranges: &[("element($a, reals)", "$y > 0")], bound: Bound::Below, refs: "DLMF 10.25.3, 10.31.1, 10.37", ..POSITIVE },
    Entry { name: "bessel_y", arity: 2, var: 1, refs: "DLMF 10.2.3, 10.8.1", ..POSITIVE },
    Entry { name: "bessel_j", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > 0", complex: "$u != 0 or re($a) > 0", refs: "DLMF 10.2.2", ..POSITIVE },
    Entry { name: "bessel_i", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > 0", complex: "$u != 0 or re($a) > 0", refs: "DLMF 10.25.2", ..POSITIVE },
    Entry { name: "hankel_h1", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(LOG_CUT), refs: "DLMF 10.4.3, 10.2.(ii)", ..E },
    Entry { name: "hankel_h2", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(LOG_CUT), refs: "DLMF 10.4.3", ..E },
    Entry {
        name: "spherical_bessel_j",
        arity: 2,
        var: 1,
        when: "not_element($a, integers)",
        real: "$u > 0 or $u == 0 and $a > 0",
        complex: "$u != 0 or re($a) > 0",
        refs: "DLMF 10.47.3",
        ..POSITIVE
    },
    Entry { name: "spherical_bessel_y", arity: 2, var: 1, when: "not_element($a, integers)", refs: "DLMF 10.47.4", ..POSITIVE },
    Entry {
        name: "spherical_bessel_y",
        arity: 2,
        var: 1,
        when: "element($a, integers)",
        real: "$u != 0",
        complex: "$u != 0",
        sr: Some(&[("$u == 0", PO)]),
        sc: Some(&[("$u == 0", PO)]),
        refs: "DLMF 10.47.4, 10.49.4",
        ..E
    },
    Entry { name: "spherical_hankel_h1", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(SPH_HANKEL), refs: "DLMF 10.47.5, 10.49.6", ..E },
    Entry { name: "spherical_hankel_h2", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(SPH_HANKEL), refs: "DLMF 10.47.6, 10.49.7", ..E },
    Entry { name: "kelvin_ber", arity: 2, var: 1, when: "element($a, integers)", ranges: &[("", "true")], refs: "DLMF 10.65.1, 10.67.1", ..ENTIRE },
    Entry { name: "kelvin_bei", arity: 2, var: 1, when: "element($a, integers)", ranges: &[("", "true")], refs: "DLMF 10.65.1, 10.67.1", ..ENTIRE },
    // ber + i bei = J_nu(x e^(3 pi i/4)): the leading term (x/2)^nu cos(3 nu pi/4) (sin for bei), the next with the
    // other of cos, sin and the power nu + 2 (DLMF 10.61.1, 10.65.1)
    Entry {
        name: "kelvin_ber",
        arity: 2,
        var: 1,
        when: "not_element($a, integers)",
        real: "$u > 0 or $u == 0 and ($a > 0 or cos(3*pi*$a/4) == 0 and $a > -2)",
        complex: "$u != 0 or re($a) > 0 or cos(3*pi*$a/4) == 0 and re($a) > -2",
        refs: "DLMF 10.61.1, 10.65.1",
        ..POSITIVE
    },
    Entry {
        name: "kelvin_bei",
        arity: 2,
        var: 1,
        when: "not_element($a, integers)",
        real: "$u > 0 or $u == 0 and ($a > 0 or sin(3*pi*$a/4) == 0 and $a > -2)",
        complex: "$u != 0 or re($a) > 0 or sin(3*pi*$a/4) == 0 and re($a) > -2",
        refs: "DLMF 10.61.1, 10.65.1",
        ..POSITIVE
    },
    // ker + i kei = e^(-nu pi i/2) K_nu(x e^(pi i/4)) (DLMF 10.61.2) with K_nu(w) ~ Gamma(|nu|)/2 (w/2)^(-|nu|) and the
    // next term w^(2 - |nu|) a quarter turn further (10.30.2, 10.31.1): finite at 0 when the leading coefficient
    // vanishes and |nu| <= 2 (the logarithm of K_0 is real: kei_0(0) = -pi/4)
    Entry {
        name: "kelvin_ker",
        arity: 2,
        var: 1,
        when: "element($a, reals)",
        real: "$u > 0 or $u == 0 and cos(pi*($a/2 + abs($a)/4)) == 0 and abs($a) <= 2 and $a != 0",
        complex: "$u != 0 or cos(pi*($a/2 + abs($a)/4)) == 0 and abs($a) <= 2 and $a != 0",
        refs: "DLMF 10.61.2, 10.30.2, 10.31.1",
        ..POSITIVE
    },
    Entry {
        name: "kelvin_kei",
        arity: 2,
        var: 1,
        when: "element($a, reals)",
        real: "$u > 0 or $u == 0 and sin(pi*($a/2 + abs($a)/4)) == 0 and abs($a) <= 2",
        complex: "$u != 0 or sin(pi*($a/2 + abs($a)/4)) == 0 and abs($a) <= 2",
        refs: "DLMF 10.61.2, 10.30.2, 10.31.1",
        ..POSITIVE
    },
    // Struve functions (z/2)^(nu + 1) times an entire series (DLMF 11.2.1, 11.2.2)
    Entry {
        name: "struve_h",
        arity: 2,
        var: 1,
        when: "element($a, integers)",
        real: "$a >= -1 or $u != 0",
        complex: "$a >= -1 or $u != 0",
        sr: Some(&[("$a < -1 and $u == 0", PO)]),
        sc: Some(&[("$a < -1 and $u == 0", PO)]),
        refs: "DLMF 11.2.1",
        ..E
    },
    Entry { name: "struve_h", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > -1", complex: "$u != 0 or re($a) > -1", refs: "DLMF 11.2.1", ..POSITIVE },
    Entry {
        name: "struve_l",
        arity: 2,
        var: 1,
        when: "element($a, integers)",
        real: "$a >= -1 or $u != 0",
        complex: "$a >= -1 or $u != 0",
        sr: Some(&[("$a < -1 and $u == 0", PO)]),
        sc: Some(&[("$a < -1 and $u == 0", PO)]),
        refs: "DLMF 11.2.2",
        ..E
    },
    Entry { name: "struve_l", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > -1", complex: "$u != 0 or re($a) > -1", refs: "DLMF 11.2.2", ..POSITIVE },
    Entry { name: "anger_j", arity: 2, var: 1, refs: "DLMF 11.10.1, 11.10.(i)", ..ENTIRE },
    Entry { name: "weber_e", arity: 2, var: 1, refs: "DLMF 11.10.2, 11.10.(i)", ..ENTIRE },
    // parabolic cylinder and Mathieu functions: solutions of equations with entire coefficients (DLMF 12.2, 28.2)
    Entry { name: "parabolic_cylinder_d", arity: 2, var: 1, refs: "DLMF 12.2.(i), 12.2.5", ..ENTIRE },
    Entry { name: "mathieu_c", arity: 3, var: 2, joint: true, refs: "DLMF 28.2.(i)", ..ENTIRE },
    Entry { name: "mathieu_s", arity: 3, var: 2, joint: true, refs: "DLMF 28.2.(i)", ..ENTIRE },
    Entry { name: "mathieu_c_prime", arity: 3, var: 2, joint: true, refs: "DLMF 28.2.(i)", ..ENTIRE },
    Entry { name: "mathieu_s_prime", arity: 3, var: 2, joint: true, refs: "DLMF 28.2.(i)", ..ENTIRE },
    Entry { name: "mathieu_characteristic_a", arity: 2, var: 1, when: "element($a, reals)", real: "true", sr: Some(&[("?", KI)]), refs: "DLMF 28.2.(v), 28.7", ..E },
    Entry { name: "mathieu_characteristic_b", arity: 2, var: 1, when: "element($a, reals)", real: "true", sr: Some(&[("?", KI)]), refs: "DLMF 28.2.(v), 28.7", ..E },
    Entry { name: "spheroidal_ps", arity: 4, var: 3, when: "element($a, integers) and element($b, integers)", refs: "DLMF 30.2, 30.3", ..ENTIRE },
    // the confluent function U (DLMF 13.2.6, 13.2.7, 13.2.16-13.2.18, 13.2.40, 13.4.4): a polynomial for a = 0, -1,
    // ...; z^(1 - b) times a polynomial (a pole at 0 for b = 2, 3, ...) when b is an integer and a - b + 1 = 0, -1,
    // ...; otherwise the cut of the logarithm, finite at 0 when re(b) < 1; positive and decreasing from inf for
    // a > 0, b >= 1
    Entry {
        name: "hypergeometric_u",
        signs: &[(U_POSITIVE, 1)],
        mono: &[(U_POSITIVE, -1)],
        convex: &[(U_POSITIVE, 1)],
        arity: 3,
        var: 2,
        real: "$u > 0 or $u == 0 and $b < 1 or element($a, integers) and $a <= 0 or element($b, integers) and element($a - $b, integers) and $a - $b <= -1 and $u != 0",
        complex: "$u != 0 or re($b) < 1 or element($a, integers) and $a <= 0",
        sr: Some(&[
            ("not(element($a, integers) and $a <= 0) and not(element($b, integers) and element($a - $b, integers) and $a - $b <= -1) and $u <= 0", BR),
            ("not(element($a, integers) and $a <= 0) and element($b, integers) and element($a - $b, integers) and $a - $b <= -1 and $u == 0", PO),
        ]),
        sc: Some(&[
            ("not(element($a, integers) and $a <= 0) and not(element($b, integers) and element($a - $b, integers) and $a - $b <= -1) and $u == 0", BR),
            ("not(element($a, integers) and $a <= 0) and not(element($b, integers) and element($a - $b, integers) and $a - $b <= -1) and im_part($u) == 0 and re($u) <= 0", BR),
            ("not(element($a, integers) and $a <= 0) and element($b, integers) and element($a - $b, integers) and $a - $b <= -1 and $u == 0", PO),
        ]),
        ranges: &[("$a > 0 and $b >= 1 and not(element($b, integers) and element($a - $b, integers) and $a - $b <= -1)", "$y > 0")],
        refs: "DLMF 13.2.6, 13.2.7, 13.2.16-13.2.18, 13.2.40, 13.4.4",
        ..E
    },
    // Whittaker M = e^(-z/2) z^(mu + 1/2) M(1/2 + mu - k, 1 + 2 mu, z): undefined for 2 mu = -1, -2, ... (DLMF 13.14.2)
    Entry {
        name: "whittaker_m",
        arity: 3,
        var: 2,
        real: "(re($b) >= 0 or not_element(2*$b, integers)) and ($u > 0 or $u == 0 and $b > -1/2 or element($b + 1/2, integers) and $b + 1/2 > 0)",
        complex: "(re($b) >= 0 or not_element(2*$b, integers)) and ($u != 0 or re($b) > -1/2)",
        sr: Some(&[("not(element($b + 1/2, integers) and $b + 1/2 > 0) and $u <= 0", BR), ("not(re($b) >= 0 or not_element(2*$b, integers))", BR)]),
        sc: Some(&[
            ("not(element($b + 1/2, integers) and $b + 1/2 > 0) and $u == 0", BR),
            ("not(element($b + 1/2, integers) and $b + 1/2 > 0) and im_part($u) == 0 and re($u) <= 0", BR),
            ("not(re($b) >= 0 or not_element(2*$b, integers))", BR),
        ]),
        refs: "DLMF 13.14.2, 13.14.(ii)",
        ..E
    },
    // Whittaker W = e^(-z/2) z^(mu + 1/2) U(1/2 + mu - k, 1 + 2 mu, z), even in mu: finite at 0 for |re(mu)| < 1/2 and
    // mu = +-1/2 (DLMF 13.14.3, 13.14.18-13.14.20, 13.14.31); the polynomial cases are left out by the condition
    Entry {
        name: "whittaker_w",
        arity: 3,
        var: 2,
        when: "not(element(1/2 + $b - $a, integers) and 1/2 + $b - $a <= 0) and not(element(1/2 - $b - $a, integers) and 1/2 - $b - $a <= 0)",
        real: "$u > 0 or $u == 0 and (-1/2 < $b and $b < 1/2 or $b == 1/2 or $b == -1/2)",
        complex: "$u != 0 or -1/2 < re($b) and re($b) < 1/2 or $b == 1/2 or $b == -1/2",
        sr: Some(&[("$u <= 0", BR)]),
        sc: Some(LOG_CUT),
        refs: "DLMF 13.14.3, 13.14.18-13.14.20, 13.14.31",
        ..E
    },
    // the polylogarithm: rational with a pole at 1 for s = 0, -1, ...; otherwise the cut [1, inf), finite at 1 for
    // re(s) > 1; increasing on (-inf, 1] to zeta(s) for s > 1, to inf for 0 < s <= 1 (DLMF 25.12.10-25.12.12)
    Entry {
        name: "polylog",
        mono: &[("$a >= 1", 1)],
        arity: 2,
        var: 1,
        real: "$u < 1 or $u == 1 and $a > 1 or element($a, integers) and $a <= 0 and $u != 1",
        complex: "$u != 1 or re($a) > 1",
        sr: Some(&[("not(element($a, integers) and $a <= 0) and $u >= 1", BR), ("element($a, integers) and $a <= 0 and $u == 1", PO)]),
        sc: Some(&[
            ("not(element($a, integers) and $a <= 0) and $u == 1", BR),
            ("not(element($a, integers) and $a <= 0) and im_part($u) == 0 and re($u) >= 1", BR),
            ("element($a, integers) and $a <= 0 and $u == 1", PO),
        ]),
        ranges: &[("$a > 1", "$y <= zeta($a)"), ("0 < $a and $a <= 1", "true")],
        refs: "DLMF 25.12.10, 25.12.11, 25.12.12",
        ..E
    },
    Entry { name: "prime_zeta_p", real: "$u > 1", sr: Some(&[("$u <= 1", BR)]), refs: "the prime zeta function sum mu(k)/k log zeta(k s), DLMF 25.2", ..E },
    Entry { name: "riemann_siegel_z", real: "true", sr: Some(NONE), refs: "DLMF 25.9", ..E },
    // q-functions: the Euler function (x; x)_inf inside the unit disc, the circle its natural boundary; the
    // q-factorial (q; q)_inf/(q^(z + 1); q)_inf (1 - q)^(-z) with poles at z = -1, -2, ... for 0 < q < 1 (DLMF 17.2)
    Entry {
        name: "q_pochhammer",
        real: "-1 < $u and $u < 1",
        complex: "abs($u) < 1",
        sr: Some(&[("$u <= -1", BR), ("$u >= 1", BR)]),
        sc: Some(&[("abs($u) >= 1", BR)]),
        refs: "DLMF 17.2.4, 27.14.2",
        ..E
    },
    Entry {
        name: "q_factorial",
        arity: 2,
        when: "0 < $a and $a < 1",
        real: "$u > -1 or not_element($u, integers)",
        sr: Some(&[("sin(pi*$u) == 0 and $u <= -1", PO)]),
        refs: "DLMF 17.2.1, 5.18.(ii)",
        ..E
    },
    // the incomplete gamma functions Gamma(a, z) = int_z^inf t^(a-1) e^(-t) dt: entire in a, the cut of z^a otherwise;
    // real for z < 0 only for a positive integer, finite at 0 for re(a) > 0 (DLMF 8.2.2, 8.2.(ii), 8.4.8); the
    // regularized Q(a, z) = Gamma(a, z)/Gamma(a) entire in z for an integer a (8.4.10, 8.4.11: e^(-z) times a
    // polynomial, or 0), decreasing from 1 to 0 on z >= 0 for a > 0 (8.2.4)
    Entry {
        name: "gamma",
        arity: 2,
        var: 1,
        joint: true,
        real: "$u > 0 or $u == 0 and $a > 0 or $u < 0 and element($a, integers) and $a > 0",
        complex: "$u != 0 or re($a) > 0",
        refs: "DLMF 8.2.2, 8.4.8",
        ..E
    },
    Entry { name: "gamma", arity: 3, var: 1, joint: true, complex: "$u != 0 and $b != 0 or re($a) > 0", refs: "DLMF 8.2.2, 8.2.3", ..E },
    Entry {
        name: "gamma_regularized",
        arity: 2,
        var: 1,
        real: "$u > 0 or $u == 0 and $a > 0 or element($a, integers)",
        complex: "$u != 0 or re($a) > 0 or element($a, integers)",
        sr: Some(&[("not_element($a, integers) and $u <= 0", BR)]),
        sc: Some(&[("not_element($a, integers) and $u == 0", BR), ("not_element($a, integers) and im_part($u) == 0 and re($u) <= 0", BR)]),
        ranges: &[("$a > 0 and not_element($a, integers)", "0 < $y and $y <= 1")],
        refs: "DLMF 8.2.4, 8.4.10, 8.4.11",
        ..E
    },
    // the beta function Gamma(a) Gamma(b)/Gamma(a + b) (DLMF 5.12.1) and the incomplete one int_0^x t^(a-1) (1-t)^(b-1)
    // dt with its branch points 0 (unless a is a positive integer) and 1 (unless b is), undefined for a = 0, -1, ...
    // (x^a/a); regularized by B(a, b) (DLMF 8.17.1, 8.17.2, 8.17.7)
    Entry {
        name: "beta",
        arity: 2,
        joint: true,
        real: "($u > 0 or not_element($u, integers)) and ($a > 0 or not_element($a, integers))",
        complex: "(re($a) > 0 or not_element($a, integers)) and (re($u) > 0 or not_element($u, integers))",
        sr: Some(&[("element($u, integers) and $u <= 0", PO), ("element($a, integers) and $a <= 0", PO)]),
        refs: "DLMF 5.12.1",
        ..E
    },
    Entry {
        name: "beta",
        arity: 3,
        real: "not(element($a, integers) and $a <= 0) and ($u > 0 or $u == 0 and $a > 0 or element($a, integers) and $a > 0) and ($u < 1 or $u == 1 and $b > 0 or element($b, integers) and $b > 0)",
        sr: Some(INCOMPLETE_BETA_R),
        sc: Some(INCOMPLETE_BETA_C),
        refs: "DLMF 8.17.1, 8.17.7",
        ..E
    },
    Entry {
        name: "beta_regularized",
        arity: 3,
        real: "not(element($a, integers) and $a <= 0) and not(element($b, integers) and $b <= 0) and ($u > 0 or $u == 0 and $a > 0 or element($a, integers) and $a > 0) and ($u < 1 or $u == 1 and $b > 0 or element($b, integers) and $b > 0)",
        complex: "not(element($a, integers) and $a <= 0) and not(element($b, integers) and $b <= 0) and ($u != 0 or re($a) > 0) and ($u != 1 or re($b) > 0)",
        sr: Some(INCOMPLETE_BETA_R),
        sc: Some(INCOMPLETE_BETA_C),
        refs: "DLMF 8.17.2, 8.17.7",
        ..E
    },
    // the binomial coefficient Gamma(n + 1)/(Gamma(k + 1) Gamma(n - k + 1)): a polynomial in n for a nonnegative
    // integer k, entire in k for a nonnegative integer n (1/Gamma entire, DLMF 5.2.1, 26.3.1)
    Entry { name: "binomial", arity: 2, var: 0, when: "element($a, integers) and $a >= 0", refs: "DLMF 1.2.6, 26.3.1", ..ENTIRE },
    Entry { name: "binomial", arity: 2, var: 1, when: "element($a, integers) and $a >= 0", refs: "DLMF 5.2.1, 26.3.1", ..ENTIRE },
    // Pochhammer, falling factorial and digamma by the gamma function (DLMF 5.2.5, 5.15.1, 26.1)
    Entry {
        name: "pochhammer",
        arity: 2,
        joint: true,
        real: "$u + $a > 0 or not_element($u + $a, integers)",
        complex: "re($u + $a) > 0 or not_element($u + $a, integers)",
        refs: "DLMF 5.2.5",
        ..E
    },
    Entry {
        name: "factorial_power",
        arity: 2,
        joint: true,
        real: "$u >= 0 or not_element($u, integers)",
        complex: "re($u) >= 0 or not_element($u, integers)",
        refs: "DLMF 26.1 (Gamma(x + 1)/Gamma(x - n + 1))",
        ..E
    },
    Entry { name: "digamma", real: "$u >= 1 or not_element($u, integers)", complex: "re($u) > 0 or not_element($u, integers)", refs: "DLMF 5.2.2, 5.7.6", ..E },
    // distributions as functions off their support
    Entry { name: "dirac_delta", real: "$u != 0", complex: "$u < 0 or $u > 0", refs: "DLMF 1.17", ..E },
    Entry { name: "dirac_comb", real: "not_element($u, integers)", complex: "not_element($u, integers) and element($u, reals)", refs: "DLMF 1.17", ..E },
    // the Legendre function of the second kind: Ferrers' Q on (-1, 1), log singularities at +-1, undefined for a negative
    // integer degree (DLMF 14.3.2, 14.3.7)
    Entry {
        name: "legendre_q",
        arity: 2,
        var: 1,
        real: "-1 < $u and $u < 1 and ($a >= 0 or not_element($a, integers))",
        complex: "$u != -1 and $u != 1 and (re($a) >= 0 or not_element($a, integers))",
        refs: "DLMF 14.3.2, 14.3.7",
        ..E
    },
    // F(phi|m) for m > 1: real for sin(phi)^2 <= 1/m on the principal piece, a square-root branch at its ends; the
    // quarter period K(m) is not real beyond (DLMF 19.2.4, 19.7.4)
    Entry {
        name: "elliptic_f",
        arity: 2,
        when: "$a > 1",
        real: "-asin($a^(-1/2)) <= $u and $u <= asin($a^(-1/2))",
        sr: Some(&[("$u <= -asin($a^(-1/2)) or $u >= asin($a^(-1/2))", BR)]),
        bound: Bound::Both,
        refs: "DLMF 19.2.4, 19.7.4",
        ..E
    },
    // Coulomb wave functions: H+- = G +- i F are not real on the line (DLMF 33.2.7)
    Entry { name: "coulomb_h1", arity: 3, var: 2, real: "false", sr: Some(&[("true", BR)]), refs: "DLMF 33.2.7", ..E },
    Entry { name: "coulomb_h2", arity: 3, var: 2, real: "false", sr: Some(&[("true", BR)]), refs: "DLMF 33.2.7", ..E },
    // F_l(eta, rho) = C_l(eta) rho^(l + 1) e^(-i rho) M(l + 1 - i eta, 2 l + 2, 2 i rho): real for rho > 0, for rho < 0
    // when l is an integer, finite at 0 for l >= -1 (DLMF 33.2.4)
    Entry {
        name: "coulomb_f",
        arity: 3,
        var: 2,
        when: "element($a, reals) and element($b, reals)",
        real: "element($a, integers) and $u < 0 or $u >= 0 and $a >= -1 or $u > 0",
        refs: "DLMF 33.2.4",
        ..E
    },
    // the Mittag-Leffler function: entire for a > 0 (DLMF 10.46.3), 1/(1 - z) at a = 0, E_a(z) = 1 - E_(-a)(1/z) for
    // a < 0
    Entry { name: "mittag_leffler_e", arity: 2, var: 1, joint: true, sr: Some(&[("$a == 0 and $u == 1", PO), ("$a < 0 and $u == 0", BR)]), refs: "DLMF 10.46.3", ..E },
    // the alternating harmonic numbers in their two-argument form
    Entry { name: "alternating_harmonic_number", arity: 2, when: "$a == 1", real: "element($u, integers)", sr: Some(&[("true", BR)]), refs: "DLMF 25.11", ..E },
    // inverse Jacobi functions for real m other than 1: the real domain of the principal branch is the range of the
    // function on the line (for m > 1 through Jacobi's real transformation), the values bounded by a period
    // (DLMF 22.15.1-22.15.12, 22.17.4, 22.16.(i)); for dn, cd, nd, dc, ds only m < 1, where the numeric values agree
    // with that branch (for m > 1 they are complex on part of it: left to the function's own crate)
    Entry {
        name: "inverse_jacobi_sn",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and -1 <= $u*$a^(1/2) and $u*$a^(1/2) <= 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.1, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cn",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and (1 - 1/$a)^(1/2) <= $u and $u <= 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.2, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_dn",
        arity: 2,
        when: "$a < 1",
        real: "$a < 1 and min((1 - $a)^(1/2), 1) <= $u and $u <= max((1 - $a)^(1/2), 1) or $a > 1 and -1 <= $u and $u <= 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.3, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cd",
        arity: 2,
        when: "$a < 1",
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and ($u <= -1 or $u >= 1)",
        onto: &[("", false)],
        refs: "DLMF 22.15.4, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_sd",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u*(1 - $a)^(1/2) and $u*(1 - $a)^(1/2) <= 1 or $a > 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.5, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_nd",
        arity: 2,
        when: "$a < 1",
        real: "$a < 1 and min((1 - $a)^(-1/2), 1) <= $u and $u <= max((1 - $a)^(-1/2), 1) or $a > 1 and ($u <= -1 or $u >= 1)",
        onto: &[("", false)],
        refs: "DLMF 22.15.6, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_dc",
        arity: 2,
        when: "$a < 1",
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and -1 <= $u and $u <= 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.7, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_nc",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and 1 <= $u and $u <= (1 - 1/$a)^(-1/2)",
        onto: &[("", false)],
        refs: "DLMF 22.15.8, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_ns",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and ($u <= -$a^(1/2) or $u >= $a^(1/2))",
        onto: &[("", false)],
        refs: "DLMF 22.15.9, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_sc",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 or $a > 1 and -1 <= $u*($a - 1)^(1/2) and $u*($a - 1)^(1/2) <= 1",
        sr: Some(&[("$a > 1 and ($u <= -($a - 1)^(-1/2) or $u >= ($a - 1)^(-1/2))", BR)]),
        onto: &[("", false)],
        refs: "DLMF 22.15.10, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cs",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 or $a > 1 and ($u <= -($a - 1)^(1/2) or $u >= ($a - 1)^(1/2))",
        onto: &[("", false)],
        refs: "DLMF 22.15.11, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_ds",
        arity: 2,
        when: "$a < 1",
        real: "$a < 1 and ($u <= -(1 - $a)^(1/2) or $u >= (1 - $a)^(1/2)) or $a > 1",
        onto: &[("", false)],
        refs: "DLMF 22.15.12, 22.17.4",
        ..E
    },
    // jointly in (x, m): a branch point where the function turns, at m = 1/2 (m = 2 for sc, cs); some singular points
    // only
    Entry { name: "inverse_jacobi_sn", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_cn", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_dn", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_cd", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_dc", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_nc", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_nd", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_ns", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_sd", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 2^(1/2)", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_ds", arity: 2, joint: true, sr: Some(&[("$a == 1/2 and $u == 2^(-1/2)", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_sc", arity: 2, joint: true, sr: Some(&[("$a == 2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_cs", arity: 2, joint: true, sr: Some(&[("$a == 2 and $u == 1", BR)]), partial: true, ..E },
    // steps, waves and the Cantor function (continuous, not analytic on the Cantor set, values in [0, 1])
    Entry { name: "cantor_staircase", real: "true", complex: "element($u, reals)", sr: Some(&[("?", KI)]), smooth: false, ranges: &[("", "0 <= $y and $y <= 1")], bound: Bound::Both, ..E },
    Entry {
        name: "heaviside_lambda",
        real: "true",
        complex: "element($u, reals)",
        sr: Some(&[("$u == -1", KI), ("$u == 0", KI), ("$u == 1", KI)]),
        smooth: false,
        ranges: &[("", "0 <= $y and $y <= 1")],
        bound: Bound::Both,
        ..E
    },
    Entry {
        name: "heaviside_pi",
        real: "$u != -1/2 and $u != 1/2",
        complex: "$u < -1/2 or -1/2 < $u and $u < 1/2 or $u > 1/2",
        sr: Some(&[("$u == -1/2", JU), ("$u == 1/2", JU)]),
        smooth: false,
        ranges: &[("", "$y == 0 or $y == 1")],
        bound: Bound::Both,
        ..E
    },
    Entry { name: "heaviside_theta", smooth: false, ranges: &[("", "$y == 0 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "unit_step", smooth: false, ranges: &[("", "$y == 0 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "unit_box", smooth: false, ranges: &[("", "$y == 0 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "unit_triangle", smooth: false, ranges: &[("", "0 <= $y and $y <= 1")], bound: Bound::Both, ..E },
    Entry { name: "sign", smooth: false, ranges: &[("", "$y == -1 or $y == 0 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "real_sign", smooth: false, ranges: &[("", "$y == -1 or $y == 0 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "arg", smooth: false, ranges: &[("", "$y == 0 or $y == pi")], bound: Bound::Both, ..E },
    Entry { name: "ramp", smooth: false, ranges: &[("", "$y >= 0")], bound: Bound::Below, ..E },
    Entry { name: "real_abs", smooth: false, ranges: &[("", "$y >= 0")], bound: Bound::Below, ..E },
    Entry { name: "abs", smooth: false, crange: "re($y) >= 0 and im_part($y) == 0", ..E },
    Entry { name: "clip", smooth: false, ranges: &[("", "-1 <= $y and $y <= 1")], bound: Bound::Both, ..E },
    Entry { name: "floor", smooth: false, onto: &[("", false)], ranges: &[("", "element($y, integers)")], ..E },
    Entry { name: "ceiling", smooth: false, onto: &[("", false)], ranges: &[("", "element($y, integers)")], ..E },
    Entry { name: "round", smooth: false, onto: &[("", false)], ranges: &[("", "element($y, integers)")], ..E },
    Entry { name: "integer_part", smooth: false, onto: &[("", false)], ranges: &[("", "element($y, integers)")], ..E },
    Entry { name: "fractional_part", smooth: false, ranges: &[("", "-1 < $y and $y < 1")], bound: Bound::Both, ..E },
    Entry { name: "triangle_wave", real: "true", complex: "element($u, reals)", smooth: false, ranges: &[("", "-1 <= $y and $y <= 1")], bound: Bound::Both, ..E },
    Entry { name: "sawtooth_wave", real: "true", complex: "element($u, reals)", smooth: false, ranges: &[("", "0 <= $y and $y < 1")], bound: Bound::Both, ..E },
    Entry { name: "square_wave", real: "true", complex: "element($u, reals)", smooth: false, ranges: &[("", "$y == -1 or $y == 1")], bound: Bound::Both, ..E },
    Entry { name: "triangle_wave", arity: 2, var: 1, joint: true, real: "true", complex: "element($u, reals) and element($a, reals)", smooth: false, ..E },
    Entry { name: "sawtooth_wave", arity: 2, var: 1, joint: true, real: "true", complex: "element($u, reals) and element($a, reals)", smooth: false, ..E },
    Entry { name: "square_wave", arity: 2, var: 1, joint: true, real: "true", complex: "element($u, reals) and element($a, reals)", smooth: false, ..E },
    // the parts of a number and the elementary functions with a closed range (DLMF 4.2.(i), 4.14, 4.28)
    Entry { name: "re", smooth: false, ranges: &[("", "true")], crange: "im_part($y) == 0", ..E },
    Entry { name: "im_part", smooth: false, ranges: &[("", "$y == 0")], crange: "im_part($y) == 0", ..E },
    Entry { name: "conjugate", smooth: false, ranges: &[("", "true")], crange: "true", ..E },
    Entry { name: "log", crange: "-pi < im_part($y) and im_part($y) <= pi", ..E },
    Entry { name: "sin", crange: "true", bound: Bound::Both, ..E },
    Entry { name: "cos", crange: "true", bound: Bound::Both, ..E },
    Entry { name: "sinh", crange: "true", ..E },
    Entry { name: "cosh", crange: "true", ..E },
    Entry { name: "exp", crange: "$y != 0", ..E },
    Entry { name: "sech", ranges: &[("", "0 < $y and $y <= 1")], bound: Bound::Both, refs: "DLMF 4.28, 4.29", ..E },
    Entry { name: "coth", ranges: &[("", "$y < -1 or $y > 1")], refs: "DLMF 4.28", ..E },
    Entry { name: "csch", ranges: &[("", "$y != 0")], refs: "DLMF 4.28", ..E },
    // the reciprocal inverse functions on their principal branches (DLMF 4.23.(ii), 4.23.(viii), 4.37.(ii),
    // 4.37.(viii)): real for |u| > 1 (acoth), |u| >= 1 (asec, acsc), 0 < u <= 1 (asech), u != 0 (acsch), all u
    // (acot); their branch points in the plane
    Entry { name: "acoth", real: "$u < -1 or $u > 1", complex: "($u - 1)*($u + 1) != 0", ranges: &[("", "$y != 0")], refs: "DLMF 4.37.(ii)", ..E },
    Entry { name: "acsch", real: "$u != 0", complex: "$u != 0", ranges: &[("", "$y != 0")], refs: "DLMF 4.37.(ii)", ..E },
    Entry { name: "asech", real: "0 < $u and $u <= 1", complex: "$u != 0", ranges: &[("", "$y >= 0")], mono: &[("", -1)], bound: Bound::Below, refs: "DLMF 4.37.(ii)", ..E },
    Entry { name: "asec", real: "$u <= -1 or $u >= 1", complex: "$u != 0", refs: "DLMF 4.23.(ii)", ..E },
    Entry { name: "acsc", real: "$u <= -1 or $u >= 1", complex: "$u != 0", refs: "DLMF 4.23.(ii)", ..E },
    Entry { name: "acot", real: "true", complex: "$u != im and $u != -im", refs: "DLMF 4.23.(ii)", ..E },
    Entry { name: "tan", ranges: &[("", "true")], refs: "DLMF 4.14", ..E },
    Entry { name: "gamma", ranges: &[("", "$y != 0")], refs: "DLMF 5.2, 5.4 (the extrema of the pieces tend to 0)", ..E },
    Entry { name: "factorial", ranges: &[("", "$y != 0")], refs: "DLMF 5.2, 5.4", ..E },
    Entry { name: "log_gamma", bound: Bound::Below, refs: "log-convex gamma on (0, inf) (Bohr-Mollerup), DLMF 5.4.(iii), 5.5", ..E },
    // roots of a real number
    Entry { name: "cube_root", mono: &[("", 1)], real: "true", complex: "element($u, reals)", sr: Some(&[("$u == 0", KI)]), sc: Some(&[("true", BR)]), ranges: &[("", "true")], crange: "im_part($y) == 0", ..E },
    Entry {
        name: "surd",
        arity: 2,
        when: "element(($a - 1)/2, integers) and $a > 0",
        mono: &[("", 1)],
        real: "true",
        complex: "element($u, reals)",
        sr: Some(&[("$u == 0", KI)]),
        sc: Some(&[("true", BR)]),
        ranges: &[("", "true")],
        crange: "im_part($y) == 0",
        ..E
    },
    Entry {
        name: "surd",
        arity: 2,
        when: "element(($a - 1)/2, integers) and $a < 0",
        real: "$u != 0",
        complex: "element($u, reals) and $u != 0",
        sr: Some(&[("$u == 0", BR)]),
        ranges: &[("", "$y != 0")],
        ..E
    },
    Entry {
        name: "surd",
        arity: 2,
        when: "element($a/2, integers) and $a > 0",
        mono: &[("", 1)],
        real: "$u >= 0",
        complex: "$u >= 0",
        sr: Some(&[("$u <= 0", BR)]),
        ranges: &[("", "$y >= 0")],
        bound: Bound::Below,
        ..E
    },
    Entry {
        name: "surd",
        arity: 2,
        when: "element($a/2, integers) and $a < 0",
        mono: &[("", -1)],
        real: "$u > 0",
        complex: "$u > 0",
        sr: Some(&[("$u <= 0", BR)]),
        ranges: &[("", "$y > 0")],
        bound: Bound::Below,
        ..E
    },
];

/// U(a, b, x) > 0, decreasing and convex on its domain (an interval) for a > 0 off the Laurent-polynomial cases:
/// the integral 13.4.4 and U' = -a U(a + 1, b + 1, x) (DLMF 13.3.22).
const U_POSITIVE: &str = "$a > 0 and not(element($b, integers) and element($a - $b, integers) and $a - $b <= -1)";
/// Real m other than 1: the Jacobi functions are periodic in x (DLMF 22.4.(ii), 22.17.4).
const PERIODIC_M: &str = "$a < 1 or $a > 1";
/// Real m other than 1.
const M_NOT_1: &str = "$a < 1 or $a > 1";
/// The branch points of the incomplete beta function on the line (DLMF 8.17.7: x^a/a 2F1(a, 1 - b; a + 1; x)).
const INCOMPLETE_BETA_R: &[(&str, Kind)] = &[
    ("not(element($a, integers) and $a > 0) and $u <= 0", BR),
    ("not(element($b, integers) and $b > 0) and $u >= 1", BR),
];
/// The branch points and cuts of the incomplete beta function in the plane.
const INCOMPLETE_BETA_C: &[(&str, Kind)] = &[
    ("not(element($a, integers) and $a > 0) and $u == 0", BR),
    ("not(element($a, integers) and $a > 0) and im_part($u) == 0 and re($u) <= 0", BR),
    ("not(element($b, integers) and $b > 0) and $u == 1", BR),
    ("not(element($b, integers) and $b > 0) and im_part($u) == 0 and re($u) >= 1", BR),
];
/// The zeros of cn (poles of nc, sc, dc).
const CN_ZEROS: &[(&str, Kind)] = &[("$a < 1 and cos(pi*$u/(2*elliptic_k($a))) == 0", PO)];
/// The zeros of dn (poles of nd, sd, cd).
const DN_ZEROS: &[(&str, Kind)] = &[("$a > 1 and cos(pi*$u*$a^(1/2)/(2*elliptic_k(1/$a))) == 0", PO)];
/// The zeros of sn (poles of ns, cs, ds).
const SN_ZEROS: &[(&str, Kind)] = &[
    ("$a < 1 and sin(pi*$u/(2*elliptic_k($a))) == 0", PO),
    ("$a == 1 and $u == 0", PO),
    ("$a > 1 and sin(pi*$u*$a^(1/2)/(2*elliptic_k(1/$a))) == 0", PO),
];
/// The spherical Hankel functions: a pole at 0 for an integer order, a branch point and the cut otherwise.
const SPH_HANKEL: &[(&str, Kind)] = &[
    ("element($a, integers) and $u == 0", PO),
    ("not_element($a, integers) and $u == 0", BR),
    ("not_element($a, integers) and im_part($u) == 0 and re($u) <= 0", BR),
];

/// The facts of one call, as templates (from an entry or built for a hypergeometric family).
#[derive(Clone)]
struct Facts {
    real: String,
    complex: String,
    sr: Option<Vec<(String, Kind)>>,
    sc: Option<Vec<(String, Kind)>>,
    partial: bool,
    cpartial: bool,
    ranges: Vec<(String, String)>,
    crange: String,
    bound: Bound,
    onto: Vec<(String, bool)>,
    periodic: String,
    signs: Vec<(String, i8)>,
    mono: Vec<(String, i8)>,
    convex: Vec<(String, i8)>,
}

impl Facts {
    fn of(e: &Entry) -> Facts {
        let own = |v: &[(&str, Kind)]| v.iter().map(|(s, k)| (s.to_string(), *k)).collect::<Vec<_>>();
        Facts {
            onto: e.onto.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            periodic: e.periodic.into(),
            signs: e.signs.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            mono: e.mono.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            convex: e.convex.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            real: e.real.into(),
            complex: e.complex.into(),
            sr: e.sr.map(own),
            sc: e.sc.map(own),
            partial: e.partial,
            cpartial: e.cpartial,
            ranges: e.ranges.iter().map(|(w, r)| (w.to_string(), r.to_string())).collect(),
            crange: e.crange.into(),
            bound: e.bound,
        }
    }
}

/// Singular atoms with their kinds, and whether they are some of the singular points only.
pub type Atoms<'h> = (Vec<(T<'h>, Kind)>, bool);

/// A call found in the table: its facts and the substitution of the placeholders.
pub struct Hit<'h> {
    facts: Facts,
    rules: Vec<T<'h>>,
    /// the variable argument
    pub arg: T<'h>,
    /// the parameter facts the call was looked up under
    pf: Vec<T<'h>>,
}

/// The placeholder symbol of `$name`.
fn placeholder<'h>(b: &Build<'h>, name: &str) -> T<'h> {
    let h = b.heap();
    h.sym_term(h.sym(&format!("tpl_{name}")))
}

/// The template `s` with its placeholders replaced, evaluated.
fn inst<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, s: &str, rules: &[T<'h>]) -> Res<T<'h>> {
    let h = b.heap();
    let src = s.replace('$', "tpl_");
    let t = cx.eval(b.call("to_expression", &[h.string(&src)]))?;
    cx.eval(h.call(h.sym("rewrite"), &[t, cx.list(rules)]))
}

/// The names of the other arguments in order.
const NAMES: [&str; 6] = ["a", "b", "c", "d", "f", "g"];

/// Does `t` depend on one of `vars`?
fn depends<'h>(b: &Build<'h>, t: T<'h>, vars: &[T<'h>]) -> bool {
    vars.iter().any(|&x| !is_free(b, t, x))
}

/// `t` with every `not_element(u, s)` written `not(element(u, s))`, the form `refine` decides under facts.
fn positive_form<'h>(b: &Build<'h>, t: T<'h>) -> T<'h> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return t };
    let new: Vec<T<'h>> = args.iter().map(|&a| positive_form(b, a)).collect();
    if b.sym_of(head) == Some(h.sym("not_element")) && new.len() == 2 {
        return b.call("not", &[b.call("element", &new)]);
    }
    h.app(head, &new)
}

/// `c` refined under the facts.
fn refined<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, facts: &[T<'h>]) -> Res<T<'h>> {
    let h = b.heap();
    cx.eval_assuming(facts, h.call(h.sym("refine"), &[positive_form(b, c)]))
}

/// Is `c` true under the facts (by `refine`)?
fn holds<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, c: T<'h>, facts: &[T<'h>]) -> Res<bool> {
    if cx.is_true(c) {
        return Ok(true);
    }
    if cx.is_false(c) {
        return Ok(false);
    }
    let r = refined(cx, b, c, facts)?;
    Ok(cx.is_true(r))
}

/// The entry of the call `t` (a function of `vars`), if the table knows it and its parameter condition holds under
/// `facts`.
pub fn lookup<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, t: T<'h>, vars: &[T<'h>], facts: &[T<'h>]) -> Res<Option<Hit<'h>>> {
    let h = b.heap();
    let View::App { head, args } = h.view(t) else { return Ok(None) };
    let Some(s) = b.sym_of(head) else { return Ok(None) };
    let name = h.sym_name(s).to_string();
    if let Some(hit) = hypergeometric(cx, b, &name, args, vars)? {
        return Ok(Some(hit));
    }
    for e in TABLE.iter().filter(|e| e.name == name && e.arity == args.len()) {
        let others: Vec<T<'h>> = args.iter().enumerate().filter(|&(i, _)| i != e.var).map(|(_, &a)| a).collect();
        if !e.joint && others.iter().any(|&o| depends(b, o, vars)) {
            continue;
        }
        let mut rules = vec![h.call(h.sym(names::RULE), &[placeholder(b, "u"), args[e.var]])];
        for (i, &o) in others.iter().enumerate() {
            rules.push(h.call(h.sym(names::RULE), &[placeholder(b, NAMES[i]), o]));
        }
        if !e.when.is_empty() {
            let w = inst(cx, b, e.when, &rules)?;
            if !holds(cx, b, w, facts)? {
                continue;
            }
        }
        return Ok(Some(Hit { facts: Facts::of(e), rules, arg: args[e.var], pf: facts.to_vec() }));
    }
    Ok(None)
}

/// The hypergeometric families (DLMF 16.2, 15.2, 13.2) and the orthogonal families of non-integer degree through
/// their hypergeometric forms (DLMF 18.5.7, 14.3.1, 15.9.1-15.9.5).
fn hypergeometric<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, name: &str, args: &[T<'h>], vars: &[T<'h>]) -> Res<Option<Hit<'h>>> {
    let h = b.heap();
    // (numerator parameters, denominator parameters, argument, regularized)
    let (num, den, z, reg): (Vec<T<'h>>, Vec<T<'h>>, T<'h>, bool) = match (name, args) {
        ("hypergeometric_pfq" | "hypergeometric_pfq_regularized", [a, bb, z]) => {
            let (Some(a), Some(bb)) = (cx.as_list(*a).map(<[T<'h>]>::to_vec), cx.as_list(*bb).map(<[T<'h>]>::to_vec)) else { return Ok(None) };
            (a, bb, *z, name.ends_with("regularized"))
        }
        ("hypergeometric_0f1", [bb, z]) => (vec![], vec![*bb], *z, false),
        ("hypergeometric_0f1_regularized", [bb, z]) => (vec![], vec![*bb], *z, true),
        ("hypergeometric_1f1", [a, bb, z]) => (vec![*a], vec![*bb], *z, false),
        ("hypergeometric_2f1", [a, bb, c, z]) => (vec![*a, *bb], vec![*c], *z, false),
        ("hypergeometric_2f1_regularized", [a, bb, c, z]) => (vec![*a, *bb], vec![*c], *z, true),
        // T_nu = 2F1(-nu, nu; 1/2; w), U_nu = (nu + 1) 2F1(-nu, nu + 2; 3/2; w), P_nu = 2F1(-nu, nu + 1; 1; w),
        // C_nu^l = (2 l)_nu/nu! 2F1(-nu, nu + 2 l; l + 1/2; w), P_nu^(a, b) = (a + 1)_nu/nu! 2F1(-nu, nu + a + b + 1;
        // a + 1; w), w = (1 - z)/2
        ("chebyshev_t" | "chebyshev_u" | "legendre_p" | "gegenbauer_c" | "jacobi_p", _) => {
            let Some((&z, ps)) = args.split_last() else { return Ok(None) };
            let nu = ps[0];
            let w = cx.eval(b.div(b.sub(b.one(), z), b.int(2)))?;
            let (n2, c) = match (name, ps) {
                ("chebyshev_t", [_]) => (nu, b.rat(1, 2)),
                ("chebyshev_u", [_]) => (b.add(&[nu, b.int(2)]), b.rat(3, 2)),
                ("legendre_p", [_]) => (b.add(&[nu, b.one()]), b.one()),
                ("gegenbauer_c", [_, l]) => (b.add(&[nu, b.mul(&[b.int(2), *l])]), b.add(&[*l, b.rat(1, 2)])),
                ("jacobi_p", [_, al, be]) => (b.add(&[nu, *al, *be, b.one()]), b.add(&[*al, b.one()])),
                _ => return Ok(None),
            };
            let n1 = b.neg(nu);
            (vec![cx.eval(n1)?, cx.eval(n2)?], vec![cx.eval(c)?], w, true)
        }
        _ => return Ok(None),
    };
    // the facts hold jointly in the parameters (a pole of a denominator parameter is a pole in it)
    let _ = vars;
    let (p, q) = (num.len(), den.len());
    // bounded below on the line: 0F1(; b; x) for real b >= 1/2 (positive for x >= 0, Gamma(b) x^((1 - b)/2)
    // J_(b-1)(2 sqrt(-x)) for x < 0 with a decaying amplitude, DLMF 10.16.9), 1F1(a; b; x) for 0 < a <= b (positive:
    // the series for x >= 0, Kummer's transformation e^x 1F1(b - a; b; -x) for x < 0, DLMF 13.2.39)
    let real_num = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::Rational { .. }) || matches!(h.view(t), View::Real(v) if v.is_finite());
    let bound = match (num.as_slice(), den.as_slice()) {
        ([], [bb]) if real_num(*bb) && b.ar.compare(*bb, b.rat(1, 2)).is_some_and(|c| c != mc_arith::Cmp3::Less) => Bound::Below,
        ([a], [bb]) if real_num(*a) && real_num(*bb) && b.ar.compare(*a, b.zero()) == Some(mc_arith::Cmp3::Greater) && b.ar.compare(*bb, *a).is_some_and(|c| c != mc_arith::Cmp3::Less) => Bound::Below,
        _ => Bound::No,
    };
    let mut rules = vec![h.call(h.sym(names::RULE), &[placeholder(b, "u"), z])];
    let mut nm = vec![];
    let mut dm = vec![];
    for (i, &a) in num.iter().enumerate() {
        let k = format!("n{i}");
        rules.push(h.call(h.sym(names::RULE), &[placeholder(b, &k), a]));
        nm.push(format!("${k}"));
    }
    for (i, &d) in den.iter().enumerate() {
        let k = format!("m{i}");
        rules.push(h.call(h.sym(names::RULE), &[placeholder(b, &k), d]));
        dm.push(format!("${k}"));
    }
    let join = |v: Vec<String>, op: &str, empty: &str| if v.is_empty() { empty.to_string() } else { v.join(op) };
    // B: no denominator parameter at a pole; P: a terminating series
    let (br, bc) = if reg {
        ("true".to_string(), "true".to_string())
    } else {
        (
            join(dm.iter().map(|m| format!("({m} > 0 or not_element({m}, integers))")).collect(), " and ", "true"),
            join(dm.iter().map(|m| format!("(re({m}) > 0 or not_element({m}, integers))")).collect(), " and ", "true"),
        )
    };
    let poly = join(nm.iter().map(|a| format!("element({a}, integers) and {a} <= 0")).collect(), " or ", "false");
    let excess = format!("re(({}) - ({})) > 0", join(dm.clone(), " + ", "0"), join(nm.clone(), " + ", "0"));
    let f = if p <= q {
        Facts {
            real: br.clone(),
            complex: bc.clone(),
            sr: Some(vec![(format!("not({br})"), Kind::Branch)]),
            sc: Some(vec![(format!("not({bc})"), Kind::Branch)]),
            partial: false,
            cpartial: false,
            ranges: vec![],
            crange: String::new(),
            bound: Bound::No,
            onto: vec![],
            periodic: String::new(),
            signs: vec![],
            mono: vec![],
            convex: vec![],
        }
    } else if p == q + 1 {
        Facts {
            real: format!("({br}) and ({poly} or $u < 1 or $u == 1 and {excess})"),
            complex: format!("({bc}) and ({poly} or $u != 1 or {excess})"),
            sr: Some(vec![(format!("not({br})"), Kind::Branch), (format!("not({poly}) and $u >= 1"), Kind::Branch)]),
            sc: Some(vec![
                (format!("not({bc})"), Kind::Branch),
                (format!("not({poly}) and $u == 1"), Kind::Branch),
                (format!("not({poly}) and im_part($u) == 0 and re($u) >= 1"), Kind::Branch),
            ]),
            partial: false,
            cpartial: false,
            ranges: vec![],
            crange: String::new(),
            bound: Bound::No,
            onto: vec![],
            periodic: String::new(),
            signs: vec![],
            mono: vec![],
            convex: vec![],
        }
    } else {
        // a divergent series unless it terminates
        let pt = inst(cx, b, &poly, &rules)?;
        if !cx.is_true(pt) {
            return Ok(None);
        }
        Facts {
            real: br.clone(),
            complex: bc.clone(),
            sr: Some(vec![(format!("not({br})"), Kind::Branch)]),
            sc: Some(vec![(format!("not({bc})"), Kind::Branch)]),
            partial: false,
            cpartial: false,
            ranges: vec![],
            crange: String::new(),
            bound: Bound::No,
            onto: vec![],
            periodic: String::new(),
            signs: vec![],
            mono: vec![],
            convex: vec![],
        }
    };
    let mut f = f;
    f.bound = bound;
    Ok(Some(Hit { facts: f, rules, arg: z, pf: vec![] }))
}

impl<'h> Hit<'h> {
    /// Where the function is defined (and real, over the reals); `None` when not known.
    pub fn domain(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, complex: bool) -> Res<Option<T<'h>>> {
        let t = if complex { &self.facts.complex } else { &self.facts.real };
        if t.is_empty() {
            return Ok(None);
        }
        if t == "~" {
            // the complement of the singular atoms
            let Some(atoms) = (if complex { &self.facts.sc } else { &self.facts.sr }).clone() else { return Ok(None) };
            let mut parts = vec![];
            for (a, _) in atoms {
                let v = inst(cx, b, &a, &self.rules)?;
                parts.push(b.call("not", &[v]));
            }
            return Ok(Some(match parts.as_slice() {
                [] => cx.bool(true),
                [p] => cx.eval(*p)?,
                _ => cx.eval(b.call("and", &parts))?,
            }));
        }
        Ok(Some(inst(cx, b, t, &self.rules)?))
    }

    /// The singular atoms of the given mode (no kinks for discontinuities) and whether they are some only; `None`
    /// when not known.
    pub fn atoms(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, complex: bool, mode: Mode) -> Res<Option<Atoms<'h>>> {
        let Some(v) = (if complex { &self.facts.sc } else { &self.facts.sr }).clone() else { return Ok(None) };
        let mut out = vec![];
        for (a, k) in v {
            if k == Kind::Kink && mode == Mode::Disc {
                continue;
            }
            if a == "?" {
                // an unlisted set of non-analytic points
                return Ok(None);
            }
            let t = inst(cx, b, &a, &self.rules)?;
            if cx.is_false(t) {
                continue;
            }
            // an atom the parameters' facts refute (a branch cut only for a non-integer order)
            if !self.pf.is_empty() {
                let r = refined(cx, b, t, &self.pf)?;
                if cx.is_false(r) {
                    continue;
                }
            }
            out.push((t, k));
        }
        Ok(Some((out, if complex { self.facts.cpartial } else { self.facts.partial })))
    }

    /// The range on the reals (a condition on `y`) of the function of its variable argument; `None` when not known.
    pub fn range(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
        let h = b.heap();
        let mut rules = self.rules.clone();
        rules.push(h.call(h.sym(names::RULE), &[placeholder(b, "y"), y]));
        for (w, r) in self.facts.ranges.clone() {
            if !w.is_empty() {
                let c = inst(cx, b, &w, &rules)?;
                if !holds(cx, b, c, &self.pf)? {
                    continue;
                }
            }
            return Ok(Some(inst(cx, b, &r, &rules)?));
        }
        Ok(None)
    }

    /// The range in the complex plane; `None` when not known.
    pub fn crange(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, y: T<'h>) -> Res<Option<T<'h>>> {
        if self.facts.crange.is_empty() {
            return Ok(None);
        }
        let h = b.heap();
        let mut rules = self.rules.clone();
        rules.push(h.call(h.sym(names::RULE), &[placeholder(b, "y"), y]));
        Ok(Some(inst(cx, b, &self.facts.crange, &rules)?))
    }

    /// Boundedness on the real domain.
    pub fn bound(&self) -> Bound {
        self.facts.bound
    }

    /// Onto the reals, by the first recorded parameter condition that holds; `None` when none does.
    pub fn onto(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>) -> Res<Option<bool>> {
        for (w, v) in self.facts.onto.clone() {
            let c = inst(cx, b, &w, &self.rules)?;
            if holds(cx, b, c, &self.pf)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }

    /// Periodic in the variable under the parameter facts?
    pub fn periodic(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>) -> Res<bool> {
        if self.facts.periodic.is_empty() {
            return Ok(false);
        }
        let c = inst(cx, b, &self.facts.periodic, &self.rules)?;
        holds(cx, b, c, &self.pf)
    }

    /// The strict sign (`order` 0), monotonicity (1) or convexity (2) on the real domain under the parameter
    /// facts; `None` when not known.
    pub fn shape(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, order: usize) -> Res<Option<i8>> {
        let list = match order {
            0 => &self.facts.signs,
            1 => &self.facts.mono,
            _ => &self.facts.convex,
        };
        for (w, v) in list.clone() {
            if w.is_empty() {
                return Ok(Some(v));
            }
            let c = inst(cx, b, &w, &self.rules)?;
            if holds(cx, b, c, &self.pf)? {
                return Ok(Some(v));
            }
        }
        Ok(None)
    }
}

/// Is `name` a function the table describes as not smooth (steps, kinks), or one whose singular set on the line it
/// does not list completely (a jump may hide between samples of the derivative)? Such functions are decided by
/// samples of the function itself.
pub fn not_smooth(name: &str) -> bool {
    let mut entries = TABLE.iter().filter(|e| e.name == name).peekable();
    entries.peek().is_some() && entries.any(|e| !e.smooth || (e.sr.is_none() || e.partial) && !e.real.is_empty())
}
