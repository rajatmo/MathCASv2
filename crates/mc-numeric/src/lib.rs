//! MathCAS v2 numerics (library package L): exact constants, elementary functions on exact, ball and machine
//! numbers, and numerical evaluation `n` (semantics 2.3 to 2.7 and 15).
//!
//! - [`consts`]: `pi`, `e`, `euler_gamma`, `catalan` as balls by binary splitting, cached;
//! - [`elementary`]: exp, log, sin, cos, tan, atan, asin, acos, sinh, cosh, tanh, sqrt and real powers on balls,
//!   with enclosures (bit-burst series, Newton steps with a rigorous last step);
//! - [`machine`]: the one shared set of machine-real functions (spike E decision E.4) and complex extensions;
//! - [`exact`]: exact identities (`sin(pi) = 0`, `log(e) = 1`, `cos(pi/3) = 1/2`, `exp(log(x)) = x`, parity);
//! - [`numeval`]: numerical evaluation of terms, `n(e)` and `n(e, d)` with precision doubling, comparisons and
//!   sign decisions by refinement;
//! - [`value`]: exact values behind inexact numbers, decimal digits of balls, the simplest rational in an interval;
//! - the number functions of roadmap package 1C (private modules `parts`, `approx`, `piecewise`): `re`,
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
//! Decisions of this package are numbered L.1, L.2, ... in the comments where they are applied:
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
//! - **L.12** No ball functions of complex balls: such calls stay unevaluated.
//!
//! Package 1C (number functions; details in the modules):
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
pub mod consts;
pub mod elementary;
pub mod exact;
pub mod fun;
pub mod machine;
mod natives;
mod numbers;
pub mod numeval;
mod parts;
mod piecewise;
mod series;
pub mod value;

pub use fun::Fun;
pub use natives::{failure_tags, install, install_with_stop};

/// The functions and constants of this package, each with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("pi", "the constant pi (Chudnovsky series by binary splitting under n); a positive real"),
    ("e", "the constant e, the base of the natural logarithm; a positive real"),
    ("euler_gamma", "Euler's constant gamma = 0.5772... (Brent-McMillan under n); a positive real"),
    ("catalan", "Catalan's constant G = 0.9159...; a positive real"),
    ("exp", "exp(x): e^x; exact at 0, 1, log(y) and q pi i; balls and machine numbers numerically"),
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
];
