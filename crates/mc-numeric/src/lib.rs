//! MathCAS v2 numerics (library item L): exact constants, elementary functions on exact, ball and machine
//! numbers, and numerical evaluation `n` (semantics 2.3 to 2.7 and 15).
//!
//! - [`consts`]: `pi`, `e`, `euler_gamma`, `catalan` as balls by binary splitting, cached;
//! - [`elementary`]: exp, log, sin, cos, tan, atan, asin, acos, sinh, cosh, tanh, sqrt and real powers on balls,
//!   with enclosures (bit-burst series, Newton steps with a rigorous last step);
//! - [`cball`]: the same functions, `abs`, `arg` and powers on complex balls (principal branches, enclosures from
//!   the real ball functions of the parts; roadmap 07 item N0 item 4);
//! - [`machine`]: the one shared set of machine-real functions (spike E decision E.4) and complex extensions;
//! - [`exact`]: exact identities (`sin(pi) = 0`, `log(e) = 1`, `cos(pi/3) = 1/2`, `exp(log(x)) = x`, parity);
//! - [`numeval`]: numerical evaluation of terms, `n(e)` and `n(e, d)` with precision doubling, comparisons and
//!   sign decisions by refinement;
//! - [`library`]: the registry through which other crates' functions of real arguments (the special functions)
//!   become numeric nodes of [`numeval`];
//! - [`value`]: exact values behind inexact numbers, decimal digits of balls, the simplest rational in an interval;
//! - the number functions of roadmap item 1C (private modules `parts`, `approx`, `piecewise`): `re`,
//!   `im_part`, `conjugate`, `arg`, rules extending `abs` and `sign` to closed terms, `precision`, `accuracy`,
//!   `set_precision`, `rationalize`, `chop`, `clip`, `rescale`, `unit_step`, and `n(e, digits: d)`;
//! - [`install`] / [`install_with_stop`]: the natives and the comparison rules for an `mc-eval` kernel.
//!
//! ```
//! use mc_numeric::consts::Const;
//! let pi = Const::Pi.ball(200).unwrap();
//! assert!(pi.has_digits(55));
//! assert_eq!(pi.mid.to_sci(20).0, "31415926535897932385");
//! ```
//!
//! Decisions of this crate are numbered L.1, L.2, ... in the comments where they are applied:
//!
//! - **L.1** `install(k, h)` takes the heap as well (the kernel holds none, and symbols belong to a heap);
//!   `install_with_stop(k, h, stop)` also takes the flag the numeric loops poll.
//! - **L.2** Exact identities ([`exact`]): `exp(x)` stays an `exp` call (not `e^x`) except `exp(0) = 1`,
//!   `exp(1) = e`, `exp(log(x)) = x` and `exp(q pi i)` at the special angles; `log(0)` stays; `sqrt(x)` is the power
//!   `x^(1/2)`; other exact arguments stay unevaluated (`sin(1)`).
//! - **L.3** Constants are cached process-wide at the best precision computed, never after a stop.
//! - **L.4** Ball algorithms: bit-burst exp and sin/cos, Newton for log and atan with a rigorous last step, the
//!   other functions as ball expressions of these ([`elementary`]).
//! - **L.5** Supported ranges: exp for |x| < 2^40, sin and cos for |x| < 2^(2^22); beyond, no ball value.
//! - **L.6** `n` evaluates maximal numeric subterms (islands) and keeps the rest of the term, so it maps over lists
//!   and through any head; machine reals inside an island make it a machine computation (contagion).
//! - **L.7** `n(e, d)`: first precision ceil(d log2 10) + 16 bits, doubling to max(16 p0, p0 + 4096) bits (at most
//!   the size limit), the result rounded to p0; `d` not a positive integer gives `failure(invalid_argument, call)`,
//!   p0 beyond the size limit `failure(limit, memory, call)`.
//! - **L.8** Comparisons with constants and functions are decided by refinement (rules on the comparison heads);
//!   `==` is decided only false (values apart) or for two coinciding exact points; undecided stays unevaluated.
//! - **L.9** Machine arguments outside a function's real domain give the principal complex value (`log(-1.0)`), as
//!   `mc-arith`'s machine powers do.
//! - **L.10** The elementary functions map over a list argument (there is no `listable` attribute yet).
//! - **L.11** Integer square roots run with the stop flag off (they cannot survive a stopped multiplication).
//! - **L.12** (superseded by N0C.1) No ball functions of complex balls: such calls stay unevaluated.
//!
//! Item N0C (complex balls; details in [`cball`] and `NOTES.md`):
//!
//! - **N0C.1** Complex ball functions are ball expressions of the real ball functions of the parts (Arb's `acb`
//!   approach); the natives evaluate complex-ball arguments (`sqrt(z)` of a complex ball is computed, not left as
//!   a power).
//! - **N0C.2** Principal branches of [`machine::C64`]; an exact zero imaginary part is the upper side of a cut; a
//!   region touching a cut with an inexact part, a pole or a singular point has no value (`None`, the call stays).
//! - **N0C.3** Real arguments inside a real domain use the real functions: real results keep an exact zero
//!   imaginary part.
//! - **N0C.4** `n` evaluates complex islands (complex numbers and (-1)^q units are numeric nodes); a real island
//!   whose real evaluation fails where the complex one succeeds (log(-2), asin(2)) continues complex, and a real
//!   ball outside a real domain gives the principal complex value (as L.9 for machine reals); `is_real` decides
//!   `false` for an imaginary part that excludes 0; comparisons ignore complex islands.
//! - **N0C.5** A complex ball has d digits when each part has, or is below 10^-d times the other part.
//! - **N0C.6** Powers: integers by binary powering, half-integers through sqrt, others exp(w log z); the engine's
//!   `pow` calls [`numeval::ball_power`] for ball powers `mc-arith` leaves unevaluated.
//!
//! Item 1C (number functions; details in the modules):
//!
//! - **1C.1** Complex parts of numbers are exact operations on the parts: `im_part` of any real is the exact 0;
//!   `arg` of any real is exact (0 or pi), of an exact complex number an exact `atan` form evaluated again.
//! - **1C.2** On terms, `re`, `im_part`, `conjugate` distribute over sums of c * (-1)^q * (known-real factors);
//!   known real: closed numeric terms with real values and calls of `re`, `im_part`, `abs`, `arg`.
//! - **1C.3** `chop` replaces inexact reals below delta (a ball entirely) by the exact 0, complex parts
//!   separately; a chopped imaginary part leaves the real part alone.
//! - **1C.4** `precision` and `accuracy` are decimal digits as machine reals, `inf` for exact; a ball's come from
//!   its working precision and its radius, whichever is worse; over a term the least.
//! - **1C.5** `set_precision` resets every number to a ball whose radius is only the new rounding (machine reals
//!   by their binary value), then evaluates closed subterms; `inf` gives exact binary values.
//! - **1C.6** `rationalize(x)`: the simplest rational in an inexact number's interval if within 10^-4/q^2;
//!   `rationalize(x, dx)`: the simplest rational within dx for every kind of real, closed terms by refinement.
//! - **1C.7** `abs` and `sign` of closed terms that are not numbers are rules here (the natives for numbers are
//!   `mc-numtheory`'s); they decide signs by refinement and use the decomposition of 1C.2.
//! - **1C.8** `unit_step` and `clip` give exact results where the answer is a bound or a step value: `clip(5.0)`
//!   is 1, `unit_step(2.5)` is 1; undecided comparisons leave the call.
//! - **1C.9** `n(e, digits: d)` is `n(e, d)`; both forms together must agree; other options are
//!   `failure(unknown_option, name)`.
//! - **1C.10** `rescale(xs)` of equal values gives zeros.
#![warn(missing_docs)]

mod approx;
pub mod cball;
pub mod consts;
pub mod elementary;
pub mod exact;
pub mod fun;
pub mod library;
pub mod machine;
pub mod formula;
pub mod formula_lib;
pub mod more;
pub mod quantity;
pub mod uncertain;
mod uncertain_boxes;
mod natives;
mod numbers;
pub mod numeval;
pub mod spline;
mod parts;
mod piecewise;
mod series;
pub mod series_hook;
pub mod value;

pub use fun::Fun;
pub use natives::{failure_tags, install, install_with_stop};

/// The functions and constants of this crate, each with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("pi", "the constant pi (Chudnovsky series by binary splitting under n); a positive real"),
    ("e", "the constant e, the base of the natural logarithm; a positive real"),
    ("euler_gamma", "Euler's constant gamma = 0.5772... (Brent-McMillan under n); a positive real"),
    ("catalan", "Catalan's constant G = 0.9159...; a positive real"),
    ("golden_ratio", "the golden ratio (1 + sqrt(5))/2 = 1.6180...; a positive real"),
    ("machine_epsilon", "machine_epsilon: the smallest machine number e with 1 + e != 1, 2^-52 (a library definition)"),
    ("min_machine_number", "min_machine_number: the smallest positive machine number of full precision, 2^-1022 (a library definition)"),
    ("khinchin", "khinchin: Khinchin's constant 2.6854...; n(khinchin, p) sums a zeta-function series (library helper)"),
    ("machine_precision", "machine_precision: the precision of machine numbers, 53 log10(2) = 15.9545... (a prelude definition)"),
    ("exp", "exp(x): e^x; exact at 0, 1, log(y) and q pi i; real and complex balls and machine numbers numerically"),
    ("log", "log(x): the principal natural logarithm; exact at 1, e, e^q, exp(q) and -1"),
    ("sin", "sin(x): the sine; exact at rational multiples of pi with denominator 1, 2, 3, 4 or 6"),
    ("cos", "cos(x): the cosine; exact at rational multiples of pi with denominator 1, 2, 3, 4 or 6"),
    ("tan", "tan(x): the tangent; exact at rational multiples of pi with denominator 1, 3, 4 or 6"),
    ("atan", "atan(x): the inverse tangent; exact at 0, ±1, ±3^(1/2), ±3^(-1/2)"),
    ("asin", "asin(x): the principal inverse sine; exact at 0, ±1/2, ±2^(-1/2), ±3^(1/2)/2, ±1"),
    ("acos", "acos(x): the principal inverse cosine; exact at 0, ±1/2, ±2^(-1/2), ±3^(1/2)/2, ±1"),
    ("sinh", "sinh(x): the hyperbolic sine"),
    ("cosh", "cosh(x): the hyperbolic cosine"),
    ("tanh", "tanh(x): the hyperbolic tangent"),
    ("sqrt", "sqrt(x): the principal square root, the canonical power x^(1/2)"),
    (
        "n",
        "n(e): machine numbers; n(e, d), n(e, digits: d): balls with d correct digits, or precision_not_reached",
    ),
    ("is_positive", "is_positive(x): true/false for a number or closed numeric expression when decided"),
    ("is_negative", "is_negative(x): true/false for a number or closed numeric expression when decided"),
    ("is_real", "is_real(x): true for real numbers and real closed numeric expressions, false for non-real numbers"),
    ("re", numbers::description("re")),
    ("im_part", numbers::description("im_part")),
    ("conjugate", numbers::description("conjugate")),
    ("arg", numbers::description("arg")),
    ("precision", numbers::description("precision")),
    ("accuracy", numbers::description("accuracy")),
    ("set_precision", numbers::description("set_precision")),
    ("rationalize", numbers::description("rationalize")),
    ("chop", numbers::description("chop")),
    ("clip", numbers::description("clip")),
    ("rescale", numbers::description("rescale")),
    ("unit_step", numbers::description("unit_step")),
    ("sec", "sec(x): the secant 1/cos(x)"),
    ("csc", "csc(x): the cosecant 1/sin(x)"),
    ("cot", "cot(x): the cotangent 1/tan(x)"),
    ("sech", "sech(x): the hyperbolic secant 1/cosh(x)"),
    ("csch", "csch(x): the hyperbolic cosecant 1/sinh(x)"),
    ("coth", "coth(x): the hyperbolic cotangent 1/tanh(x)"),
    ("asinh", "asinh(z): the inverse hyperbolic sine log(z + sqrt(z^2 + 1))"),
    ("acosh", "acosh(z): the inverse hyperbolic cosine, principal branch"),
    ("atanh", "atanh(z): the inverse hyperbolic tangent (log(1 + z) - log(1 - z))/2"),
    ("acot", "acot(z): the inverse cotangent atan(1/z)"),
    ("asec", "asec(z): the inverse secant acos(1/z)"),
    ("acsc", "acsc(z): the inverse cosecant asin(1/z)"),
    ("acoth", "acoth(z): the inverse hyperbolic cotangent atanh(1/z)"),
    ("asech", "asech(z): the inverse hyperbolic secant acosh(1/z)"),
    ("acsch", "acsch(z): the inverse hyperbolic cosecant asinh(1/z)"),
    ("sinc", "sinc(x): sin(x)/x, with sinc(0) = 1"),
    ("log10", "log10(x): the base-10 logarithm log(x)/log(10); exact powers of 10 give integers"),
    ("log2", "log2(x): the base-2 logarithm log(x)/log(2); exact powers of 2 give integers"),
    ("gudermannian", "gudermannian(x): the Gudermannian function 2 atan(tanh(x/2))"),
    ("inverse_gudermannian", "inverse_gudermannian(x): the inverse Gudermannian function 2 atanh(tan(x/2))"),
    ("integer_part", "integer_part(x): x truncated toward zero (componentwise for complex x)"),
    ("fractional_part", "fractional_part(x): x - integer_part(x), with the sign of x"),
    ("unit_box", "unit_box(x, ...): 1 for |x| <= 1/2, else 0 (a product over the arguments)"),
    ("unit_triangle", "unit_triangle(x, ...): the triangle 1 - |x| for |x| < 1, else 0 (a product over the arguments)"),
    ("ramp", "ramp(x): x for x >= 0, else 0"),
    ("heaviside_theta", "heaviside_theta(x, ...): 1 for x > 0, 0 for x < 0, unevaluated at 0 (a product over the arguments)"),
    ("dirac_delta", "dirac_delta(x): the Dirac delta, 0 at nonzero reals"),
    ("real_abs", "real_abs(x): |x| for real x (its derivative is real_sign)"),
    ("real_sign", "real_sign(x): -1, 0 or 1 for real x"),
    ("surd", "surd(x, n): the real n-th root of x, negative for x < 0 and odd n"),
    ("re_im", "re_im(z): the list [re(z), im_part(z)]"),
    ("abs_arg", "abs_arg(z): the list [abs(z), arg(z)]"),
    ("around", "around(x, dx): the value x with standard uncertainty dx, propagated to first order through arithmetic and functions; around(list): mean and standard deviation"),
    ("interval", "interval([a, b], ...): the union of closed intervals, carried through arithmetic and functions"),
    ("centered_interval", "centered_interval(c, r): the interval c +- r, carried through arithmetic and functions by a midpoint-radius estimate"),
    ("quantity", "quantity(m, unit): the magnitude m in a unit (a string such as \"Meters\", or products and powers of them), with conversions in arithmetic"),
    ("unit_convert", "unit_convert(q, unit): the quantity q in another unit of the same dimension; unit_convert(q): in SI base units"),
    ("quantity_magnitude", "quantity_magnitude(q): the magnitude of a quantity; quantity_magnitude(q, unit): its magnitude in that unit"),
    ("quantity_unit", "quantity_unit(q): the unit of a quantity"),
];
