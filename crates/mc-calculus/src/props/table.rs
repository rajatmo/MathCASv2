//! What each special function is: the knowledge the function properties read, kept once as data.
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
//! injectivity are proved on the domain read here (or refuted at points of it). Compositions are taken apart by
//! the callers: a subterm contributes its conditions with its own argument substituted.
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
    /// smooth where defined, apart from the listed atoms (`false`: kinks or jumps, refuted by differences of the
    /// function itself, never proved by them)
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
    /// injective on its real domain: (condition on the parameters, answer), the first that holds
    pub inj: &'static [(&'static str, bool)],
    /// injective on its domain in the plane: (condition on the parameters, answer), the first that holds
    pub cinj: &'static [(&'static str, bool)],
    /// the strict sign on the real domain: (condition on the parameters, 1 positive or -1 negative), the first that
    /// holds; 2 (-2) when only nonnegative (nonpositive) is known
    pub signs: &'static [(&'static str, i8)],
    /// strictly increasing (1) or decreasing (-1) on the real domain (an interval), the first condition that holds;
    /// 2 (-2) when only nondecreasing (nonincreasing) is known
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
    inj: &[],
    cinj: &[],
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
    // sinc = j_0 is even with sinc' = -j_1 (DLMF 10.51.2), j_1(x) = (pi/(2 x))^(1/2) J_(3/2)(x) (10.47.3): monotone
    // between consecutive zeros of J_(3/2), so on [0, j(3/2,2)] the extrema are sinc(0) = 1 and sinc(j(3/2,1)) =
    // -0.21723; beyond, |sinc(x)| <= 1/x <= 1/j(3/2,2) = 0.12944
    Entry { name: "sinc", ranges: &[("", "sinc(bessel_j_zero(3/2, 1)) <= $y and $y <= 1")], bound: Bound::Both, onto: &[("", false)], refs: "DLMF 10.47.3, 10.51.2", ..ENTIRE },
    Entry { name: "scorer_gi", bound: Bound::Both, refs: "DLMF 9.12.4, 9.12.27, 9.12.31", ..ENTIRE },
    Entry { name: "scorer_gi_prime", ranges: &[("", "true")], refs: "DLMF 9.12.4, 9.12.31", ..ENTIRE },
    Entry { name: "scorer_hi", signs: &[("", 1)], mono: &[("", 1)], convex: &[("", 1)], ranges: &[("", "$y > 0")], bound: Bound::Below, refs: "DLMF 9.12.5, 9.12.20, 9.12.30", ..ENTIRE },
    Entry { name: "scorer_hi_prime", signs: &[("", 1)], mono: &[("", 1)], convex: &[("", 1)], ranges: &[("", "$y > 0")], bound: Bound::Below, refs: "DLMF 9.12.5, 9.12.20", ..ENTIRE },
    // error functions and their inverses (DLMF 7.2, 7.5, 7.17)
    Entry { name: "erf", mono: &[("", 1)], ranges: &[("", "-1 < $y and $y < 1")], bound: Bound::Both, refs: "DLMF 7.2.1 (2/sqrt(pi) times the integral of exp(-t^2) from 0 to x)", ..ENTIRE },
    Entry {
        name: "erfc",
        signs: &[("", 1)],
        mono: &[("", -1)],
        ranges: &[("", "0 < $y and $y < 2")],
        bound: Bound::Both,
        refs: "DLMF 7.2.2 (2/sqrt(pi) times the integral of exp(-t^2) from x to inf)",
        ..ENTIRE
    },
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
        // the value z has Q(a, z) = x and Q(a, 0) = 1, Q decreasing: z >= 0, and 0 only at x = 1
        signs: &[("$a > 0", 2)],
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
    Entry { name: "logistic_sigmoid", complex: "1 + exp($u) != 0", signs: &[("", 1)], mono: &[("", 1)], ranges: &[("", "0 < $y and $y < 1")], crange: "$y - 1 != 0 and $y != 0", bound: Bound::Both, refs: "1/(1 + e^-u): positive, increasing (its derivative s (1 - s) > 0)", ..E },
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
        // a product of positive factors and Gamma(u/2 + 1), which has no zero: 0 is not a value
        onto: &[("", false)],
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
    // the hyperharmonic numbers of an integer order r >= 1 continued to real m: (m + 1) ... (m + r - 1)/(r - 1)!
    // (psi(m + r) - psi(r)), with the simple poles of psi at m + r = 0, -1, ... (DLMF 5.2.2, 5.7.6)
    Entry {
        name: "hyper_harmonic_number",
        arity: 2,
        var: 1,
        when: "element($a, integers) and $a >= 1",
        real: "$u > -$a or not_element($u, integers)",
        complex: "re($u) > -$a or not_element($u, integers)",
        sr: Some(&[("sin(pi*$u) == 0 and $u <= -$a", PO)]),
        sc: Some(&[("sin(pi*$u) == 0 and re($u) <= -$a", PO)]),
        refs: "DLMF 5.2.2, 5.7.6",
        ..E
    },
    // functions of an integer: complex-valued off the integers (subfactorial: Gamma(z + 1, -1)/e, entire in z, DLMF
    // 8.2.(ii)); the alternating harmonic numbers (a factor (-1)^z); the tau function (integers only)
    Entry { name: "subfactorial", real: "element($u, integers) and $u >= 0", complex: "true", sr: Some(&[("true", BR)]), sc: Some(NONE), refs: "DLMF 8.2.(ii)", ..E },
    // the alternating harmonic numbers log 2 - (-1)^z (psi(z/2 + 1) - psi((z + 1)/2))/2 (the partial sums of DLMF
    // 4.6.1 at 1 continued by DLMF 5.15 and the integral of t^z/(1 + t) over [0, 1]): (-1)^z entire, the digamma
    // functions with simple poles at z = -1, -2, ... that do not cancel (the residues of the bracket are +-1)
    Entry { name: "alternating_harmonic_number", real: "element($u, integers)", complex: AHN_COMPLEX, sr: Some(&[("true", BR)]), sc: Some(AHN_POLES), refs: "DLMF 4.6.1, 5.15", ..E },
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
    // F(phi|m) increases: its derivative (1 - m sin(phi)^2)^(-1/2) is positive where it is real (DLMF 19.2.4)
    // and the integrand has period pi, so F(phi + pi|m) = F(phi|m) + 2 K(m) with K(m) > 0: unbounded both ways, every
    // real value taken
    Entry { name: "elliptic_f", arity: 2, when: "$a < 1", mono: &[("", 1)], real: "true", sr: Some(NONE), ranges: &[("", "true")], onto: &[("", true)], refs: "DLMF 19.2.4, 19.2.8", ..E },
    Entry { name: "elliptic_e", arity: 2, when: "$a < 1", real: "true", sr: Some(NONE), refs: "DLMF 19.2.5", ..E },
    Entry {
        name: "elliptic_pi",
        arity: 2,
        joint: true,
        real: "$u != 1 and $a < 1",
        complex: "$u != 1 and $a != 1",
        sr: Some(&[("$u == 1", BR), ("$a >= 1", BR)]),
        // in the plane, in the parameter m: the logarithmic branch point 1 (the integral diverges like log(1/sqrt(1 - m))
        // for n < 1, DLMF 19.6.5) and the cut [1, inf) of 1/sqrt(1 - m sin(t)^2); the points in n are not listed
        sc: Some(&[("$a == 1", BR), ("im_part($a) == 0 and re($a) >= 1", BR)]),
        cpartial: true,
        refs: "DLMF 19.2.7, 19.6.5, 19.7.9",
        ..E
    },
    // q(m) = exp(-pi K(1 - m)/K(m)) increases on m <= 1 from q(-inf) = -1 to q(1) = 1: for m < 0, q(m) = -q(m/(m - 1))
    // (K(m) = (1 - m)^(-1/2) K(m/(m - 1)), DLMF 19.7.5, and the nome of the imaginary transformation, DLMF 22.17.1-22.17.2)
    // with m/(m - 1) -> 1- as m -> -inf; its inverse m(q) increases on (-1, 1] from -inf to 1
    Entry {
        name: "elliptic_nome_q",
        mono: &[("", 1)],
        real: "$u <= 1",
        complex: "true",
        sr: Some(&[("$u >= 1", BR)]),
        ranges: &[("", "-1 < $y and $y <= 1")],
        bound: Bound::Both,
        onto: &[("", false)],
        refs: "DLMF 22.2.1, 19.2.8, 19.7.5",
        ..E
    },
    Entry {
        name: "inverse_elliptic_nome_q",
        mono: &[("", 1)],
        real: "-1 < $u and $u <= 1",
        // theta_2(q)^4/theta_3(q)^4 inside the unit disc (DLMF 20.2.1-20.2.3; the circle is the natural boundary of the
        // theta functions) and the value 1 at q = 1
        complex: "abs($u) < 1 or $u == 1",
        sr: Some(&[("$u <= -1", BR), ("$u >= 1", BR)]),
        sc: Some(&[("abs($u) >= 1", BR)]),
        ranges: &[("", "$y <= 1")],
        bound: Bound::Above,
        onto: &[("", false)],
        refs: "DLMF 20.9.1, 22.2.2, 19.7.5",
        ..E
    },
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
    // am(x|m) is the integral of dn(t|m) from 0 to x, and dn >= (1 - m)^(1/2) > 0 for m < 1 (DLMF 22.16.1, 22.2.6)
    Entry { name: "jacobi_amplitude", arity: 2, when: "$a < 1", mono: &[("", 1)], real: "true", sr: Some(NONE), ranges: &[("", "true")], refs: "DLMF 22.16.1", ..E },
    // theta functions are entire in z for a nome inside the unit disc (DLMF 20.2.1-20.2.4); Neville's for m < 1
    // (nome in (-1, 1), DLMF 20.1, 22.2.1)
    Entry { name: "elliptic_theta", arity: 3, var: 1, when: "abs($b) < 1", periodic: "true", bound: Bound::Both, refs: "DLMF 20.2.1", ..ENTIRE },
    Entry { name: "elliptic_theta_prime", arity: 3, var: 1, when: "abs($b) < 1", periodic: "true", bound: Bound::Both, refs: "DLMF 20.2.1", ..ENTIRE },
    Entry { name: "neville_theta_c", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    // theta_d and theta_n are theta_3 and theta_4 of a real argument and a real nome q in (-1, 1) divided by their
    // values at 0: the products prod (1 - q^(2n)) (1 +- 2 q^(2n - 1) cos(2 z) + q^(4n - 2)) have every factor at least
    // (1 - |q|^(2n - 1))^2 > 0 (DLMF 20.5.3, 20.5.4), so both are positive
    Entry { name: "neville_theta_d", arity: 2, periodic: "$a < 1", signs: &[("$a < 1", 1)], real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 20.5.3, 22.2.1", ..E },
    Entry { name: "neville_theta_n", arity: 2, periodic: "$a < 1", signs: &[("$a < 1", 1)], real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 20.5.4, 22.2.1", ..E },
    Entry { name: "neville_theta_s", arity: 2, periodic: "$a < 1", real: "$a < 1", sr: Some(&[("$a >= 1", BR)]), refs: "DLMF 20.1, 22.2.1", ..E },
    // Weierstrass functions: sigma entire in z and the invariants (DLMF 23.2.6); p, p', zeta with a pole at 0
    // (and the other lattice points, not listed: DLMF 23.2.4-23.2.5)
    Entry { name: "weierstrass_sigma", arity: 2, joint: true, refs: "DLMF 23.2.6", ..ENTIRE },
    Entry { name: "weierstrass_p", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.4", ..E },
    Entry { name: "weierstrass_p_prime", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.4", ..E },
    Entry { name: "weierstrass_zeta", arity: 2, sr: Some(&[("$u == 0", PO)]), sc: Some(&[("$u == 0", PO)]), partial: true, cpartial: true, refs: "DLMF 23.2.5", ..E },
    // modular functions of the upper half plane, the real line their natural boundary (DLMF 23.15, 27.14.12). They
    // take real values on vertical lines of the half plane, which a non-real argument u(x) may run along: `real`
    // lists those lines (none meets the real line): eta(i t) > 0 (the product 27.14.12 with q = e^(-2 pi t), and
    // eta(tau + 24) = eta(tau)), eta(12 + i t) = -eta(i t) (the factor e^(i pi tau/12)); lambda(i t) in (0, 1) (23.15.2 with real nome, lambda(tau + 2) = lambda(tau)) and
    // lambda(1 + i t) = lambda(i t)/(lambda(i t) - 1) < 0; J real on re tau in Z/2 (J(-conj(tau)) = conj(J(tau)),
    // period 1) with J(i t) >= 1 (23.15.7: J(i) = 1 its minimum on the axis). Other curves where the values are
    // real (the images of the unit circle) are not listed. The signs hold where the argument is on the listed lines.
    Entry {
        name: "dedekind_eta",
        real: "im_part($u) > 0 and element(re($u)/12, integers)",
        complex: "im_part($u) > 0",
        sr: Some(&[("im_part($u) <= 0", BR)]),
        sc: Some(&[("im_part($u) <= 0", BR)]),
        signs: &[("im_part($u) > 0 and element(re($u)/24, integers)", 1), ("im_part($u) > 0 and element((re($u) - 12)/24, integers)", -1)],
        // eta(tau + 24) = eta(tau)
        cinj: &[("", false)],
        refs: "DLMF 27.14.12",
        ..E
    },
    Entry {
        name: "klein_invariant_j",
        real: "im_part($u) > 0 and element(2*re($u), integers)",
        complex: "im_part($u) > 0",
        sr: Some(&[("im_part($u) <= 0", BR)]),
        sc: Some(&[("im_part($u) <= 0", BR)]),
        signs: &[("im_part($u) > 0 and element(re($u), integers)", 1)],
        // J(tau + 1) = J(tau)
        cinj: &[("", false)],
        refs: "DLMF 23.15.7",
        ..E
    },
    Entry {
        name: "modular_lambda",
        real: "im_part($u) > 0 and element(re($u), integers)",
        complex: "im_part($u) > 0",
        sr: Some(&[("im_part($u) <= 0", BR)]),
        sc: Some(&[("im_part($u) <= 0", BR)]),
        signs: &[("im_part($u) > 0 and element(re($u)/2, integers)", 1), ("im_part($u) > 0 and element((re($u) - 1)/2, integers)", -1)],
        // lambda(tau + 2) = lambda(tau)
        cinj: &[("", false)],
        refs: "DLMF 23.15.6",
        ..E
    },
    // Bessel, Hankel, Kelvin, Struve, Anger-Weber functions (DLMF 10.2, 10.8, 10.25, 10.47, 10.61, 10.65, 11.2,
    // 11.10)
    Entry { name: "bessel_k", arity: 2, var: 1, onto: &[("element($a, reals)", false)], signs: &[("element($a, reals)", 1)], mono: &[("element($a, reals)", -1)], convex: &[("element($a, reals)", 1)], ranges: &[("element($a, reals)", "$y > 0")], bound: Bound::Below, refs: "DLMF 10.25.3, 10.31.1, 10.37", ..POSITIVE },
    // ranges of the oscillating functions of order 0 from the first extremum and a decreasing envelope: J0' = -J1 and
    // Y0' = -Y1 (DLMF 10.6.2), so between consecutive positive zeros of J1 (Y1) J0 (Y0) is monotone and the extrema
    // on [0, j(1,2)] ((0, y(1,2)]) are J0(0) = 1 and J0(j(1,1)) (Y0(y(1,1)), Y0 -> -inf at 0+, DLMF 10.7.3); beyond,
    // |J0|, |Y0| <= M0(x) = (J0^2 + Y0^2)^(1/2), decreasing on x > 0 by Nicholson's integral (DLMF 10.9.30: K0(2 x
    // sinh t) decreases in x), and M0(j(1,2)) = 0.30087 < -J0(j(1,1)) = 0.40276, M0(y(1,2)) = 0.34172 < Y0(y(1,1)) =
    // 0.52079 (30-digit values). J0 is even and |J_n| <= 1 for integer n (DLMF 10.14.1).
    Entry {
        name: "bessel_j",
        arity: 2,
        var: 1,
        when: "element($a, integers)",
        ranges: &[("$a == 0", "bessel_j(0, bessel_j_zero(1, 1)) <= $y and $y <= 1")],
        bound: Bound::Both,
        onto: &[("", false)],
        refs: "DLMF 10.6.2, 10.9.30, 10.14.1",
        ..ENTIRE
    },
    Entry {
        name: "bessel_y",
        arity: 2,
        var: 1,
        when: "$a == 0",
        ranges: &[("", "$y <= bessel_y(0, bessel_y_zero(1, 1))")],
        bound: Bound::Above,
        onto: &[("", false)],
        refs: "DLMF 10.6.2, 10.7.3, 10.9.30",
        ..POSITIVE
    },
    // I_n of an integer order (I_(-n) = I_n, DLMF 10.27.1): entire, the series (x/2)^n sum (x^2/4)^k/(k! (n + k)!)
    // (DLMF 10.25.2) has positive terms, so I_n of even n is even and >= 0 (I_0 >= 1, = 1 at 0 only), of odd n odd;
    // I_n' = (I_(n-1) + I_(n+1))/2 (DLMF 10.29.1): odd n increasing (two even orders, I_0 > 0 or a zero at 0 only),
    // even n convex (I_n'' = (I_(n-2) + 2 I_n + I_(n+2))/4, three even orders); unbounded (DLMF 10.30.4)
    Entry {
        name: "bessel_i",
        arity: 2,
        var: 1,
        when: "element($a/2, integers)",
        signs: &[("$a == 0", 1), ("", 2)],
        convex: &[("", 1)],
        ranges: &[("$a == 0", "$y >= 1"), ("", "$y >= 0")],
        bound: Bound::Below,
        onto: &[("", false)],
        refs: "DLMF 10.25.2, 10.27.1, 10.29.1",
        ..ENTIRE
    },
    Entry {
        name: "bessel_i",
        arity: 2,
        var: 1,
        when: "element(($a + 1)/2, integers)",
        mono: &[("", 1)],
        ranges: &[("", "true")],
        onto: &[("", true)],
        refs: "DLMF 10.25.2, 10.27.1, 10.29.1, 10.30.4",
        ..ENTIRE
    },
    // Y_nu of an order nu >= 0 tends to -inf at 0+ (DLMF 10.7.3, 10.7.7) and |Y_nu| <= M_nu(x) <= M_nu(1) on x >= 1
    // (Nicholson, DLMF 10.9.30): bounded above on x > 0, not onto
    Entry { name: "bessel_y", arity: 2, var: 1, when: "$a >= 0", bound: Bound::Above, onto: &[("", false)], refs: "DLMF 10.7.3, 10.7.7, 10.9.30", ..POSITIVE },
    Entry { name: "bessel_y", arity: 2, var: 1, refs: "DLMF 10.2.3, 10.8.1", ..POSITIVE },
    Entry { name: "bessel_j", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > 0", complex: "$u != 0 or re($a) > 0", refs: "DLMF 10.2.2", ..POSITIVE },
    Entry { name: "bessel_i", arity: 2, var: 1, when: "not_element($a, integers)", real: "$u > 0 or $u == 0 and $a > 0", complex: "$u != 0 or re($a) > 0", refs: "DLMF 10.25.2", ..POSITIVE },
    // not injective on the cut plane: H(z) = c z^(-1/2) e^(i z) (1 + eps(z)), c != 0, eps -> 0 in the sectors |arg z| <= pi
    // - delta (H1; H2 with e^(-i z) in the mirror sectors), DLMF 10.17.5-10.17.6; for every w != 0 Rouche's theorem on the
    // discs around the solutions z_k of c z^(-1/2) e^(i z) = w, e^(i z) = w z^(1/2)/c, z_k = 2 pi k - i log|w (2 pi k)^(1/2)/c| +
    // O(1), gives infinitely many solutions of H(z) = w
    Entry { name: "hankel_h1", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(LOG_CUT), cinj: &[("", false)], refs: "DLMF 10.4.3, 10.2.(ii), 10.17.5", ..E },
    Entry { name: "hankel_h2", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(LOG_CUT), cinj: &[("", false)], refs: "DLMF 10.4.3, 10.17.5", ..E },
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
    // for an integer order h(z) = e^(+-i z) P(1/z) with a nonzero polynomial P (DLMF 10.49.6, 10.49.7, 10.47.15): an
    // essential singularity at inf, so by Picard's great theorem every value but one is taken infinitely often (not
    // injective in the plane)
    Entry { name: "spherical_hankel_h1", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(SPH_HANKEL), cinj: &[("element($a, integers)", false)], refs: "DLMF 10.47.5, 10.49.6", ..E },
    Entry { name: "spherical_hankel_h2", arity: 2, var: 1, real: "false", complex: "$u != 0", sr: Some(&[("true", BR)]), sc: Some(SPH_HANKEL), cinj: &[("element($a, integers)", false)], refs: "DLMF 10.47.6, 10.49.7", ..E },
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
    // L_nu(x) = (x/2)^(nu + 1) times a series in (x/2)^2 with the positive coefficients 1/(Gamma(k + 3/2) Gamma(k + nu +
    // 3/2)) for nu > -3/2 (DLMF 11.2.2): for nu > -1 every term increases on x > 0, from L_nu(0) = 0
    Entry {
        name: "struve_l",
        arity: 2,
        var: 1,
        when: "not_element($a, integers)",
        signs: &[("$a > -1", 2)],
        mono: &[("$a > -1", 1)],
        real: "$u > 0 or $u == 0 and $a > -1",
        complex: "$u != 0 or re($a) > -1",
        refs: "DLMF 11.2.2",
        ..POSITIVE
    },
    // J_nu(x) = (1/pi) int_0^pi cos(nu t - x sin t) dt: |J_nu| <= 1 for real nu and x (DLMF 11.10.1), and J_n = J_n
    // of Bessel for an integer n (DLMF 11.10.4): the range of order 0 is that of bessel_j(0, x)
    Entry {
        name: "anger_j",
        arity: 2,
        var: 1,
        when: "element($a, reals)",
        ranges: &[("$a == 0", "bessel_j(0, bessel_j_zero(1, 1)) <= $y and $y <= 1")],
        bound: Bound::Both,
        onto: &[("", false)],
        refs: "DLMF 11.10.1, 11.10.4",
        ..ENTIRE
    },
    Entry { name: "anger_j", arity: 2, var: 1, refs: "DLMF 11.10.1, 11.10.(i)", ..ENTIRE },
    Entry { name: "weber_e", arity: 2, var: 1, refs: "DLMF 11.10.2, 11.10.(i)", ..ENTIRE },
    // Lommel functions s_(mu,nu) = z^(mu + 1)/((mu + 1)^2 - nu^2) 1F2(1; (mu - nu + 3)/2, (mu + nu + 3)/2; -z^2/4)
    // (DLMF 11.9.3; t with +z^2/4): z^(mu + 1) times an even entire series that is 1 at 0. Undefined for every z when
    // mu - nu or mu + nu is an odd negative integer (the denominator (mu + 1)^2 - nu^2 vanishes at -1, a lower parameter
    // is a pole of the series below). Otherwise: the cut of z^(mu + 1) for a non-integer mu, a pole of order -mu - 1 at
    // 0 for an integer mu < -1, entire for an integer mu >= -1 (s_(2,1/3) = z^3 times an even entire series).
    Entry {
        name: "lommel_s1",
        arity: 3,
        var: 2,
        joint: true,
        real: LOMMEL_REAL,
        complex: LOMMEL_COMPLEX,
        sr: Some(LOMMEL_SR),
        sc: Some(LOMMEL_SC),
        refs: "DLMF 11.9.3",
        ..E
    },
    Entry { name: "lommel_t1", arity: 3, var: 2, joint: true, real: LOMMEL_REAL, complex: LOMMEL_COMPLEX, sr: Some(LOMMEL_SR), sc: Some(LOMMEL_SC), refs: "DLMF 11.9.3 (the series with +z^2/4)", ..E },
    Entry {
        name: "lommel_s2",
        arity: 3,
        var: 2,
        when: "not_element($a, integers) and not_element($b, integers) and not_element(($a - $b + 1)/2, integers) and not_element(($a + $b + 1)/2, integers)",
        refs: "DLMF 11.9.5",
        ..POSITIVE
    },
    Entry {
        name: "lommel_t2",
        arity: 3,
        var: 2,
        when: "not_element($a, integers) and not_element($b, integers) and not_element(($a - $b + 1)/2, integers) and not_element(($a + $b + 1)/2, integers)",
        refs: "DLMF 11.9.5",
        ..POSITIVE
    },
    // parabolic cylinder and Mathieu functions: solutions of equations with entire coefficients (DLMF 12.2, 28.2)
    // D_nu(x) = U(-nu - 1/2, x) for real nu < 0 is e^(-x^2/4)/Gamma(-nu) times the integral of t^(-nu-1) e^(-t^2/2 - x t)
    // over t > 0 (DLMF 12.5.1, 12.1.3): positive; it tends to 0 at inf (12.9.1) and to inf at -inf (12.9.3, the term
    // with 1/Gamma(-nu) != 0), so every positive value is taken. For nu <= -1/2, D'' = (x^2/4 - nu - 1/2) D > 0 (12.2.4):
    // strictly convex, and a convex positive function tending to 0 at inf strictly decreases. For nu = 0, 1, ... D_n =
    // e^(-x^2/4) He_n(x) (12.7.2) is bounded: not onto.
    Entry {
        name: "parabolic_cylinder_d",
        arity: 2,
        var: 1,
        signs: &[("$a < 0", 1)],
        mono: &[("$a <= -1/2", -1)],
        convex: &[("$a <= -1/2", 1)],
        ranges: &[("$a < 0", "$y > 0")],
        onto: &[("$a < 0", false), ("element($a, integers) and $a >= 0", false)],
        refs: "DLMF 12.2.4, 12.5.1, 12.7.2, 12.9.1, 12.9.3",
        ..ENTIRE
    },
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
        // Li_s(x) = x/Gamma(s) times the integral of t^(s-1)/(e^t - x) over t > 0 for s > 0 (DLMF 25.12.11): x/(e^t - x)
        // increases and is convex in x < 1 (its derivatives e^t/(e^t - x)^2, 2 e^t/(e^t - x)^3 are positive)
        mono: &[("$a > 0", 1)],
        convex: &[("$a > 0", 1)],
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
    // q-functions: the Euler function phi(x) = (x; x)_inf inside the unit disc, the circle its natural boundary. On
    // (-1, 1) it is continuous, at most 1 - x in absolute value on (0, 1), and tends to 0 at -1 through phi(-r) =
    // phi(r^2)^3/(phi(r) phi(r^4)) and the transformation of eta (phi(e^(-t)) ~ (2 pi/t)^(1/2) e^(-pi^2/(6 t)), DLMF
    // 23.15.5, 27.14.12): bounded, so not onto the reals
    Entry {
        name: "q_pochhammer",
        real: "-1 < $u and $u < 1",
        complex: "abs($u) < 1",
        sr: Some(&[("$u <= -1", BR), ("$u >= 1", BR)]),
        sc: Some(&[("abs($u) >= 1", BR)]),
        bound: Bound::Both,
        onto: &[("", false)],
        refs: "DLMF 17.2.4, 23.15.5, 27.14.2, 27.14.12",
        ..E
    },
    // the q-factorial [x]_q! = (q; q)_inf/(q^(x + 1); q)_inf (1 - q)^(-x) for 0 < q < 1, Gamma(x + 1) at q = 1 and
    // q^(x (x - 1)/2) [x]_(1/q)! for q > 1 (DLMF 5.18.1-5.18.4, 17.2.1); for an integer n >= 0 the polynomial
    // prod_(k <= n) (1 - q^k)/(1 - q), real for every q. For q > 0: simple poles at x = -1, -2, ... (the factor
    // 1 - q^(x + 1 + k) of the denominator), no zero (every factor is finite and nonzero), so 0 is not a value (not
    // onto); between the poles -2 and -1 the sign is constant and |f| tends to inf at both ends, so a value is taken
    // twice (not injective). For q < 0 a non-integer x gives the non-real q^(x + 1); at q = 0 the value is 1 for x > -1
    // and q^(x + 1) is undefined for x < -1; for q < 0 the integers x <= -1 are poles too (the factor 1 - q^0).
    Entry {
        name: "q_factorial",
        arity: 2,
        joint: true,
        real: "element($u, integers) and $u >= 0 or not_element($u, integers) and ($a > 0 or $a == 0 and $u > -1)",
        sr: Some(&[("$a != 0 and sin(pi*$u) == 0 and $u <= -1", PO), ("$a < 0 and not_element($u, integers)", BR), ("$a == 0 and $u <= -1", BR)]),
        onto: &[("$a > 0", false)],
        inj: &[("$a > 0", false)],
        refs: "DLMF 5.18.1-5.18.4, 17.2.1",
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
        // entire for a = 1, 2, ... (a polynomial times e^(-u)), a branch point at 0 otherwise
        sr: Some(&[("not(element($a, integers) and $a > 0) and $u <= 0", BR)]),
        sc: Some(&[
            ("not(element($a, integers) and $a > 0) and $u == 0", BR),
            ("not(element($a, integers) and $a > 0) and im_part($u) == 0 and re($u) <= 0", BR),
        ]),
        refs: "DLMF 8.2.2, 8.4.8",
        ..E
    },
    Entry { name: "gamma", arity: 3, var: 1, joint: true, complex: "$u != 0 and $b != 0 or re($a) > 0", refs: "DLMF 8.2.2, 8.2.3", ..E },
    Entry {
        name: "gamma_regularized",
        arity: 2,
        var: 1,
        // at 0 Gamma(a, 0) is infinite for re(a) <= 0 while 1/Gamma(a) = 0 at a = 0, -1, ...: undefined there
        real: "$u > 0 or $u == 0 and $a > 0 or element($a, integers) and ($a > 0 or $u != 0)",
        complex: "$u != 0 or re($a) > 0",
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
        // I_x(a, 1) = x^a (DLMF 8.17.2 with B(a, 1) = 1/a): for a positive integer a defined on the whole line, even
        // (x and -x give the same value) for an even a, strictly increasing for an odd a
        inj: &[("$b == 1 and element($a/2, integers) and $a > 0", false), ("$b == 1 and element(($a - 1)/2, integers) and $a > 0", true)],
        refs: "DLMF 8.17.2, 8.17.7",
        ..E
    },
    // the binomial coefficient Gamma(n + 1)/(Gamma(k + 1) Gamma(n - k + 1)): a polynomial in n for a nonnegative
    // integer k, entire in k for a nonnegative integer n (1/Gamma entire, DLMF 5.2.1, 26.3.1)
    Entry { name: "binomial", arity: 2, var: 0, when: "element($a, integers) and $a >= 0", refs: "DLMF 1.2.6, 26.3.1", ..ENTIRE },
    Entry { name: "binomial", arity: 2, var: 1, when: "element($a, integers) and $a >= 0", refs: "DLMF 5.2.1, 26.3.1", ..ENTIRE },
    // Pochhammer, falling factorial and digamma by the gamma function (DLMF 5.2.5, 5.15.1, 26.1)
    // (x)_(1/2) = Gamma(x + 1/2)/Gamma(x): continuous on (-1/2, inf), tending to -inf at -1/2+ (the pole of Gamma(x + 1/2)
    // against 1/Gamma(-1/2) < 0) and to +inf at inf (Gamma(x + 1/2)/Gamma(x) ~ x^(1/2), DLMF 5.11.12), so by the
    // intermediate value theorem every real value is taken
    Entry {
        name: "pochhammer",
        arity: 2,
        joint: true,
        when: "$a == 1/2",
        real: "$u + $a > 0 or not_element($u + $a, integers)",
        complex: "re($u + $a) > 0 or not_element($u + $a, integers)",
        ranges: &[("", "true")],
        onto: &[("", true)],
        refs: "DLMF 5.2.5, 5.11.12",
        ..E
    },
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
        mono: &[("", 1)],
        bound: Bound::Both,
        refs: "DLMF 19.2.4, 19.7.4",
        ..E
    },
    // Coulomb wave functions: H+- = G +- i F are not real on the line (DLMF 33.2.7)
    Entry { name: "coulomb_h1", arity: 3, var: 2, real: "false", sr: Some(&[("true", BR)]), refs: "DLMF 33.2.7", ..E },
    Entry { name: "coulomb_h2", arity: 3, var: 2, real: "false", sr: Some(&[("true", BR)]), refs: "DLMF 33.2.7", ..E },
    // F_l(eta, rho) = C_l(eta) rho^(l + 1) e^(-i rho) M(l + 1 - i eta, 2 l + 2, 2 i rho): for an integer l >= 0 and a
    // real eta, rho^(l + 1) times a power series in rho with real coefficients (DLMF 33.2.4, 33.6.5: the coefficients
    // of the Maclaurin series follow a real three-term recurrence), so entire and real on the line; otherwise real for
    // rho > 0, for rho < 0 when l is an integer, finite at 0 for l >= -1
    Entry { name: "coulomb_f", arity: 3, var: 2, when: "element($a, integers) and $a >= 0 and element($b, reals)", refs: "DLMF 33.2.4, 33.6.5", ..ENTIRE },
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
    Entry { name: "alternating_harmonic_number", arity: 2, when: "$a == 1", real: "element($u, integers)", complex: AHN_COMPLEX, sr: Some(&[("true", BR)]), sc: Some(AHN_POLES), refs: "DLMF 4.6.1, 5.15", ..E },
    // inverse Jacobi functions for real m other than 1: the real domain of the principal branch is the range of the
    // function on the line (for m > 1 through Jacobi's real transformation), the values bounded by a period
    // (DLMF 22.15.1-22.15.12, 22.17.4, 22.16.(i)); the numeric values are real exactly on these sets. Where that
    // domain is an interval, the function is the elliptic integral of DLMF 22.15(ii) (arcsn x = integral of
    // ((1 - t^2)(1 - m t^2))^(-1/2) from 0 to x, arccn x = integral of ((1 - t^2)(1 - m + m t^2))^(-1/2) from x to 1,
    // ...), whose integrand is positive inside: increasing (sn, sd, sc, nd) or decreasing (cn, cd, dn), and
    // nonnegative where the integral starts at an end (cn, cd, dn, nd); for m > 1 through sn(z|m) = m^(-1/2)
    // sn(m^(1/2) z|1/m), cn(z|m) = dn(m^(1/2) z|1/m), dn(z|m) = cn(m^(1/2) z|1/m) (DLMF 22.17.2-22.17.4): arcdc(x|m) =
    // m^(-1/2) arccd(x|1/m), arcnc(x|m) = m^(-1/2) arcnd(x|1/m), arcsd(x|m) = m^(-1/2) arcsc(m^(1/2) x|1/m), ...
    // Singular on the line: the complement of the open interior of the real domain (its finite ends are zeros of the
    // integrand's radicand, branch points; beyond them the principal values are not real, and between a branch point
    // and the next one the line is a cut), and 0 for arccs with m < 1 and arcds with m > 1, real on the whole line but
    // odd with arccs(x|m) = integral of ((1 + t^2)(1 - m + t^2))^(-1/2) from x to inf for x > 0 (DLMF 22.15.11; ds:
    // ((t^2 + m)(t^2 + m - 1))^(-1/2), 22.15.12), which tends to K(m) at 0+ and to -K(m) at 0-: a jump.
    Entry {
        name: "inverse_jacobi_sn",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and -1 <= $u*$a^(1/2) and $u*$a^(1/2) <= 1",
        sr: Some(&[("$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and ($u <= -$a^(-1/2) or $u >= $a^(-1/2))", BR)]),
        onto: &[("", false)],
        mono: &[("", 1)],
        refs: "DLMF 22.15.1, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cn",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and (1 - 1/$a)^(1/2) <= $u and $u <= 1",
        sr: Some(&[("$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and ($u <= (1 - 1/$a)^(1/2) or $u >= 1)", BR)]),
        onto: &[("", false)],
        mono: &[("", -1)],
        signs: &[("", 2)],
        refs: "DLMF 22.15.2, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_dn",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and min((1 - $a)^(1/2), 1) <= $u and $u <= max((1 - $a)^(1/2), 1) or $a > 1 and -1 <= $u and $u <= 1",
        sr: Some(&[("$a < 1 and ($u <= min((1 - $a)^(1/2), 1) or $u >= max((1 - $a)^(1/2), 1)) or $a > 1 and ($u <= -1 or $u >= 1)", BR)]),
        onto: &[("", false)],
        mono: &[("0 < $a and $a < 1", -1), ("$a > 1", -1)],
        signs: &[("0 < $a and $a < 1", 2), ("$a > 1", 2)],
        refs: "DLMF 22.15.3, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cd",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and ($u <= -1 or $u >= 1)",
        sr: Some(&[("$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and -1 <= $u and $u <= 1", BR)]),
        onto: &[("", false)],
        mono: &[("$a < 1", -1)],
        signs: &[("$a < 1", 2)],
        refs: "DLMF 22.15.4, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_sd",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and -1 <= $u*(1 - $a)^(1/2) and $u*(1 - $a)^(1/2) <= 1 or $a > 1",
        sr: Some(&[("$a < 1 and ($u <= -(1 - $a)^(-1/2) or $u >= (1 - $a)^(-1/2))", BR)]),
        onto: &[("", false)],
        mono: &[("", 1)],
        refs: "DLMF 22.15.5, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_nd",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and min((1 - $a)^(-1/2), 1) <= $u and $u <= max((1 - $a)^(-1/2), 1) or $a > 1 and ($u <= -1 or $u >= 1)",
        sr: Some(&[("$a < 1 and ($u <= min((1 - $a)^(-1/2), 1) or $u >= max((1 - $a)^(-1/2), 1)) or $a > 1 and -1 <= $u and $u <= 1", BR)]),
        onto: &[("", false)],
        mono: &[("0 < $a and $a < 1", 1)],
        signs: &[("0 < $a and $a < 1", 2)],
        refs: "DLMF 22.15.6, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_dc",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and -1 <= $u and $u <= 1",
        sr: Some(&[("$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and ($u <= -1 or $u >= 1)", BR)]),
        onto: &[("", false)],
        mono: &[("$a > 1", -1)],
        signs: &[("$a > 1", 2)],
        refs: "DLMF 22.15.7, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_nc",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and 1 <= $u and $u <= (1 - 1/$a)^(-1/2)",
        sr: Some(&[("$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and ($u <= 1 or $u >= (1 - 1/$a)^(-1/2))", BR)]),
        onto: &[("", false)],
        mono: &[("$a > 1", 1)],
        signs: &[("$a > 1", 2)],
        refs: "DLMF 22.15.8, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_ns",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -1 or $u >= 1) or $a > 1 and ($u <= -$a^(1/2) or $u >= $a^(1/2))",
        sr: Some(&[("$a < 1 and -1 <= $u and $u <= 1 or $a > 1 and -$a^(1/2) <= $u and $u <= $a^(1/2)", BR)]),
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
        mono: &[("", 1)],
        refs: "DLMF 22.15.10, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_cs",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 or $a > 1 and ($u <= -($a - 1)^(1/2) or $u >= ($a - 1)^(1/2))",
        sr: Some(&[("$a < 1 and $u == 0 or $a > 1 and -($a - 1)^(1/2) <= $u and $u <= ($a - 1)^(1/2)", BR)]),
        onto: &[("", false)],
        refs: "DLMF 22.15.11, 22.17.4",
        ..E
    },
    Entry {
        name: "inverse_jacobi_ds",
        arity: 2,
        when: M_NOT_1,
        real: "$a < 1 and ($u <= -(1 - $a)^(1/2) or $u >= (1 - $a)^(1/2)) or $a > 1",
        sr: Some(&[("$a < 1 and -(1 - $a)^(1/2) <= $u and $u <= (1 - $a)^(1/2) or $a > 1 and $u == 0", BR)]),
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
    // sd and ds are not defined on the real plane (x, m) where x^2 (1 - m) >= 1 (sd, m < 1) and where x^2 <= 1 - m (ds,
    // m < 1): the complements of the real domains of the entries above (DLMF 22.15.5, 22.15.12)
    Entry { name: "inverse_jacobi_sd", arity: 2, joint: true, sr: Some(&[("$a < 1 and $u^2*(1 - $a) >= 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_ds", arity: 2, joint: true, sr: Some(&[("$a < 1 and $u^2 <= 1 - $a", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_sc", arity: 2, joint: true, sr: Some(&[("$a == 2 and $u == 1", BR)]), partial: true, ..E },
    Entry { name: "inverse_jacobi_cs", arity: 2, joint: true, sr: Some(&[("$a == 2 and $u == 1", BR)]), partial: true, ..E },
    // steps, waves and the Cantor function (continuous, not analytic on the Cantor set, values in [0, 1])
    // the distribution function of the Cantor measure: nondecreasing
    Entry { name: "cantor_staircase", mono: &[("", 2)], real: "true", complex: "element($u, reals)", sr: Some(&[("?", KI)]), smooth: false, ranges: &[("", "0 <= $y and $y <= 1")], bound: Bound::Both, ..E },
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
    Entry { name: "log_gamma", convex: &[("", 1)], bound: Bound::Below, refs: "log-convex gamma on (0, inf) (Bohr-Mollerup; psi'(x) = sum 1/(x + k)^2 > 0, DLMF 5.15.1), DLMF 5.4.(iii), 5.5", ..E },
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
/// The parameters of the Lommel functions s, t where they are undefined: mu - nu or mu + nu an odd negative integer.
macro_rules! lommel_bad {
    () => {
        "element(($a - $b + 1)/2, integers) and $a - $b < 0 or element(($a + $b + 1)/2, integers) and $a + $b < 0"
    };
}
/// Where s, t are defined and real on the line.
const LOMMEL_REAL: &str = concat!("not(", lommel_bad!(), ") and ($u > 0 or $u == 0 and $a >= -1 or $u < 0 and element($a, integers))");
/// Where s, t are defined in the plane.
const LOMMEL_COMPLEX: &str = concat!("not(", lommel_bad!(), ") and ($u != 0 or re($a) > -1 or $a == -1)");
/// The singular atoms of s, t on the line.
const LOMMEL_SR: &[(&str, Kind)] = &[
    (lommel_bad!(), BR),
    ("not_element($a, integers) and $u <= 0", BR),
    ("element($a, integers) and $a < -1 and $u == 0", PO),
];
/// The singular atoms of s, t in the plane.
const LOMMEL_SC: &[(&str, Kind)] = &[
    (lommel_bad!(), BR),
    ("not_element($a, integers) and $u == 0", BR),
    ("not_element($a, integers) and im_part($u) == 0 and re($u) <= 0", BR),
    ("element($a, integers) and $a < -1 and $u == 0", PO),
];
/// The alternating harmonic numbers in the plane: off the negative integers.
const AHN_COMPLEX: &str = "re($u) > -1 or not_element($u, integers)";
/// Their poles.
const AHN_POLES: &[(&str, Kind)] = &[("sin(pi*$u) == 0 and re($u) <= -1", PO)];
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
    inj: Vec<(String, bool)>,
    cinj: Vec<(String, bool)>,
    signs: Vec<(String, i8)>,
    mono: Vec<(String, i8)>,
    convex: Vec<(String, i8)>,
    /// the atoms are the singular points, not the discontinuities (the discontinuities are not decided)
    singular_only: bool,
}

impl Facts {
    fn of(e: &Entry) -> Facts {
        let own = |v: &[(&str, Kind)]| v.iter().map(|(s, k)| (s.to_string(), *k)).collect::<Vec<_>>();
        Facts {
            onto: e.onto.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            periodic: e.periodic.into(),
            inj: e.inj.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
            cinj: e.cinj.iter().map(|(w, v)| (w.to_string(), *v)).collect(),
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
            singular_only: false,
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
    if let Some(hit) = q_hypergeometric(cx, b, &name, args, vars)? {
        return Ok(Some(hit));
    }
    if let Some(hit) = weierstrass_lattice(cx, b, &name, args, vars)? {
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

/// The basic hypergeometric series `r phi s(a; b; q, z)` of a rational nome `0 < |q| < 1` with `r <= s` and rational
/// lower parameters that are not powers `q^(-k)`, in `z`: entire (DLMF 17.4.1). The terms are `(a; q)_n/((b; q)_n
/// (q; q)_n) ((-1)^n q^(n (n - 1)/2))^(1 + s - r) z^n`; the denominators `(b_j; q)_n` stay away from 0 (they tend to
/// `(b_j; q)_inf != 0` because no factor `1 - b_j q^k` vanishes and `q^k -> 0`), `(q; q)_n -> (q; q)_inf != 0`, the
/// numerators are bounded, and the factor `q^(n (n - 1)/2 (1 + s - r))` with `1 + s - r >= 1` decays faster than any
/// geometric sequence, so the series converges for every `z`; it is analytic as a locally uniform limit of
/// polynomials.
fn q_hypergeometric<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, name: &str, args: &[T<'h>], vars: &[T<'h>]) -> Res<Option<Hit<'h>>> {
    let h = b.heap();
    let ("q_hypergeometric_pfq", &[la, lb, q, z]) = (name, args) else { return Ok(None) };
    let (Some(a), Some(bs)) = (cx.as_list(la).map(<[T<'h>]>::to_vec), cx.as_list(lb).map(<[T<'h>]>::to_vec)) else { return Ok(None) };
    if a.len() > bs.len() || [la, lb, q].iter().any(|&t| depends(b, t, vars)) {
        return Ok(None);
    }
    let exact = |t: T<'h>| matches!(h.view(t), View::Int(_) | View::Rational { .. });
    if !exact(q) || !bs.iter().all(|&t| exact(t)) {
        return Ok(None);
    }
    let abs_q = cx.eval(b.call("abs", &[q]))?;
    if b.ar.compare(q, b.zero()) == Some(mc_arith::Cmp3::Equal) || b.ar.compare(abs_q, b.one()) != Some(mc_arith::Cmp3::Less) {
        return Ok(None);
    }
    // no lower parameter is a power q^(-k), k = 0, 1, ...: b q^k = 1 for no k (the products decrease in modulus
    // once below 1, so the search stops there)
    for &bj in &bs {
        let mut v = bj;
        let mut below = false;
        for _ in 0..4096 {
            if b.ar.compare(v, b.one()) == Some(mc_arith::Cmp3::Equal) {
                return Ok(None);
            }
            let av = cx.eval(b.call("abs", &[v]))?;
            if b.ar.compare(av, b.one()) == Some(mc_arith::Cmp3::Less) {
                below = true;
                break;
            }
            v = cx.eval(b.mul(&[v, q]))?;
        }
        if !below {
            return Ok(None);
        }
    }
    let rules = vec![h.call(h.sym(names::RULE), &[placeholder(b, "u"), z])];
    Ok(Some(Hit { facts: Facts::of(&Entry { refs: "DLMF 17.4.1", ..ENTIRE }), rules, arg: z, pf: vec![] }))
}

/// The half-period `w2`, `w3`, the quasi-periods `eta2`, `eta3` and the root `e3` of the lattice with real invariants
/// `[g2, g3]` (the argument a list of two terms), on the real plane of the invariants. With the roots `e1, e2, e3` of
/// `4 t^3 - g2 t - g3` labelled as the numeric values have them (for three real roots `e1 > e2 > e3` when `g3 > 0` and
/// `e1 < e2 < e3` when `g3 < 0`; for one real root it is `e1` when `g2 > 0` and `e2` when `g2 < 0`, the conjugate pair
/// in the other two places: DLMF 23.3.(i), the roots of the cubic) the singular points are:
///
/// - the lines `g2 = 0` and `g3 = 0`, where the labelling of the roots (and with it of the half-periods, `w1 = int
///   from e1 to inf dt/sqrt(4 t^3 - g2 t - g3)`, DLMF 23.2.2, 23.3.7) jumps; the half-periods and quasi-periods are
///   sums of the root-labelled ones, `w2 = -w1 - w3`, `eta2 = -eta1 - eta3` (DLMF 23.2.1, 23.2.13);
/// - the discriminant `g2^3 - 27 g3^2 = 0`, where two roots meet: `e2 = e3` (the middle and the last root in both
///   orders), `e3` is continuous with a square-root branch there, and `w3 = i K'(k)/(e1 - e3)^(1/2)`, `w2 = -w1 - w3`
///   (DLMF 23.6.1-23.6.2 with the modulus `k^2 = (e2 - e3)/(e1 - e3) -> 0`, `K'` growing like `log(4/k)`, DLMF
///   19.12.1) and, by the Legendre relation `eta1 w3 - eta3 w1 = i pi/2` (DLMF 23.2.14) with `w1`, `eta1` finite there,
///   `eta3 = (eta1 w3 - i pi/2)/w1` and `eta2 = -eta1 - eta3` grow without bound: discontinuities of `w2, w3, eta2, eta3`.
///
/// Elsewhere the three roots are simple and distinct, so they and the integrals are analytic in the invariants. The line
/// `g2 = 0` (`g3 = 0`) is a singular atom when the invariant depends on the variables. `e1`, `e2`, `w1`, `eta1` are
/// not covered: they are analytic at the discriminant (a simple root), and `e2` has no jump on `g3 = 0`.
fn weierstrass_lattice<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, name: &str, args: &[T<'h>], vars: &[T<'h>]) -> Res<Option<Hit<'h>>> {
    let h = b.heap();
    let root = match name {
        "weierstrass_half_period_w2" | "weierstrass_half_period_w3" | "weierstrass_eta2" | "weierstrass_eta3" => false,
        "weierstrass_e3" => true,
        _ => return Ok(None),
    };
    let &[inv] = args else { return Ok(None) };
    let Some(g) = cx.as_list(inv).map(<[T<'h>]>::to_vec) else { return Ok(None) };
    let &[g2, g3] = g.as_slice() else { return Ok(None) };
    let mut sr: Vec<(String, Kind)> = vec![];
    if depends(b, g2, vars) {
        sr.push(("first($u) == 0".into(), Kind::Jump));
    }
    if depends(b, g3, vars) {
        sr.push(("last($u) == 0".into(), Kind::Jump));
    }
    sr.push(("first($u)^3 - 27*last($u)^2 == 0".into(), if root { Kind::Kink } else { Kind::Branch }));
    let rules = vec![h.call(h.sym(names::RULE), &[placeholder(b, "u"), inv])];
    let mut f = Facts::of(&E);
    f.sr = Some(sr);
    f.singular_only = root;
    Ok(Some(Hit { facts: f, rules, arg: inv, pf: vec![] }))
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
    let mut poly = join(nm.iter().map(|a| format!("element({a}, integers) and {a} <= 0")).collect(), " or ", "false");
    // U_nu = (nu + 1) 2F1(-nu, nu + 2; 3/2; w) at nu = -1 is 0 everywhere (U_(-n) = -U_(n - 2), DLMF 18.6.1 with
    // the series of sin((n + 1) t)/sin(t)): a polynomial for every integer degree
    if name == "chebyshev_u" {
        poly = format!("element({}, integers)", nm[0]);
    }
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
            inj: vec![],
            cinj: vec![],
            signs: vec![],
            mono: vec![],
            convex: vec![],
            singular_only: false,
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
            inj: vec![],
            cinj: vec![],
            signs: vec![],
            mono: vec![],
            convex: vec![],
            singular_only: false,
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
            inj: vec![],
            cinj: vec![],
            signs: vec![],
            mono: vec![],
            convex: vec![],
            singular_only: false,
        }
    };
    let mut f = f;
    f.bound = bound;
    // p = q with real parameters 0 < a_i < b_i (sorted in pairs): the iterated Euler integral (DLMF 16.5.2, from
    // 13.4.1) writes pFq(a; b; x) as the integral of e^(x s) against a positive measure of s in [0, 1] (the product of
    // the beta densities t_i^(a_i - 1) (1 - t_i)^(b_i - a_i - 1) of s = t_1 ... t_p), so it is positive, increasing and
    // convex on the whole line (its derivatives integrate s e^(x s) and s^2 e^(x s)); the regularized one divides it
    // by the positive Gamma(b_1) ... Gamma(b_q)
    if p == q && p > 0 && num.iter().chain(&den).all(|&t| real_num(t)) {
        // compared exactly: a_i > 0 and b_i > a_i, each pair of the sorted lists
        let order = |x: &T<'h>, y: &T<'h>| match b.ar.compare(*x, *y) {
            Some(mc_arith::Cmp3::Less) => std::cmp::Ordering::Less,
            Some(mc_arith::Cmp3::Greater) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        };
        let (mut na, mut nb) = (num.clone(), den.clone());
        na.sort_by(order);
        nb.sort_by(order);
        let pos = na.iter().all(|&a| b.ar.compare(a, b.zero()) == Some(mc_arith::Cmp3::Greater));
        let above = na.iter().zip(&nb).all(|(&a, &c)| b.ar.compare(c, a) == Some(mc_arith::Cmp3::Greater));
        if pos && above {
            f.signs = vec![(String::new(), 1)];
            f.mono = vec![(String::new(), 1)];
            f.convex = vec![(String::new(), 1)];
            f.bound = Bound::Below;
        }
    }
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
        if self.facts.singular_only && mode == Mode::Disc {
            return Ok(None);
        }
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

    /// The sign the range on the reals implies: 1 (-1) when every value is positive (negative), 2 (-2) when every
    /// value is nonnegative (nonpositive); `None` when the range is not known or has values of both signs.
    pub fn range_sign(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>) -> Res<Option<i8>> {
        let h = b.heap();
        let y = h.sym_term(h.sym("y·range"));
        let Some(r) = self.range(cx, b, y)? else { return Ok(None) };
        if cx.is_true(r) {
            return Ok(None);
        }
        let mut facts = vec![r, h.call(h.sym("element"), &[y, h.sym_term(h.sym("reals"))])];
        facts.extend(self.pf.iter().copied());
        for (rel, s) in [("greater", 1), ("less", -1), ("greater_equal", 2), ("less_equal", -2)] {
            let v = cx.eval_assuming(&facts, b.call("refine", &[b.call(rel, &[y, b.zero()])]))?;
            if cx.is_true(v) {
                return Ok(Some(s));
            }
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

    /// Is the set where the function is real stated for complex arguments (it names `im_part` of the argument: the
    /// modular functions), so that its facts hold for a non-real argument on that set?
    pub fn real_in_plane(&self) -> bool {
        self.facts.real.contains("im_part")
    }

    /// Boundedness on the real domain.
    pub fn bound(&self) -> Bound {
        self.facts.bound
    }

    /// Onto the reals, by the first recorded parameter condition that holds; `None` when none does.
    pub fn onto(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>) -> Res<Option<bool>> {
        self.first(cx, b, self.facts.onto.clone())
    }

    /// Injective on the real domain (or, `complex`, on the domain in the plane), by the first recorded parameter
    /// condition that holds; `None` when none does.
    pub fn injective(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, complex: bool) -> Res<Option<bool>> {
        self.first(cx, b, if complex { self.facts.cinj.clone() } else { self.facts.inj.clone() })
    }

    /// The answer of the first pair whose parameter condition holds.
    fn first(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, list: Vec<(String, bool)>) -> Res<Option<bool>> {
        for (w, v) in list {
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
/// does not list completely (a jump may hide between points of the derivative)? Such functions are refuted by
/// differences of the function itself.
pub fn not_smooth(name: &str) -> bool {
    let mut entries = TABLE.iter().filter(|e| e.name == name).peekable();
    entries.peek().is_some() && entries.any(|e| !e.smooth || (e.sr.is_none() || e.partial) && !e.real.is_empty())
}
