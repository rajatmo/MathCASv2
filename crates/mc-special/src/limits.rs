//! Values at the directed infinities and at the poles with a signed limit: `erf(inf)` is 1,
//! `exp_integral_ei(im*inf)` is `im*pi`, `cos_integral(0)` is `-inf`, `inverse_erf(1)` is `inf`. Each is the limit of
//! the function along the ray (or, at a pole on the real line, from the side where the function is real); the
//! other infinities and the poles without a signed limit (gamma at 0, -1, ...) are left to the callers.
use crate::functions::Special;
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::{Arith, Num, Real};
use mc_core::Heap;

/// The direction of a directed infinity in quarter turns (`inf` 0, `im*inf` 1, `-inf` 2, `-im*inf` -1).
pub fn direction(h: &Heap, ar: &Arith<'_, Heap>, x: T<'_>) -> Option<i8> {
    let inf = h.sym_term(h.sym("inf"));
    if x == inf {
        return Some(0);
    }
    let View::App { head, args: [c, y] } = h.view(x) else { return None };
    if head != h.sym_term(h.sym("mul")) || *y != inf {
        return None;
    }
    let one = Rat::from_i64(1);
    match ar.num(*c)? {
        Num::Real(Real::Exact(r)) if r == one.neg() => Some(2),
        Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == one => Some(1),
        Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == one.neg() => Some(-1),
        _ => None,
    }
}

struct B<'a, 'h> {
    h: &'h Heap,
    ar: &'a Arith<'h, Heap>,
}

impl<'h> B<'_, 'h> {
    fn inf(&self) -> T<'h> {
        self.h.sym_term(self.h.sym("inf"))
    }
    fn neg_inf(&self) -> T<'h> {
        self.ar.neg(self.inf())
    }
    fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// q i pi
    fn i_pi(&self, n: i64, d: i64) -> T<'h> {
        let c = self.ar.complex(self.h.int(0), self.ar.rational(self.h.int(n), self.h.int(d)));
        self.ar.mul(&[c, self.h.sym_term(self.h.sym("pi"))])
    }
    fn i(&self, s: i64) -> T<'h> {
        self.ar.complex(self.h.int(0), self.h.int(s))
    }
}

/// The value of `f(args)` when an argument is a directed infinity and the limit is known.
pub fn at_infinity<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, f: Special, args: &[T<'h>]) -> Option<T<'h>> {
    use Special as S;
    let b = B { h, ar };
    if f == S::BetaRegularized {
        return beta_regularized_at_infinity(h, ar, args);
    }
    let last = *args.last()?;
    let d = direction(h, ar, last);
    let first = direction(h, ar, args[0]);
    let v = match (f, d) {
        (S::Gamma | S::LogGamma | S::Digamma, Some(0)) => b.inf(),
        (S::Zeta, Some(0)) => b.int(1),
        (S::Polygamma, Some(0)) => {
            let n = ar.num(args[0])?;
            match n.as_int()?.to_i64()? {
                -1 | 0 => b.inf(),
                n if n >= 1 => b.int(0),
                _ => return None,
            }
        }
        (S::Erf, Some(0)) => b.int(1),
        (S::Erf, Some(2)) => b.int(-1),
        (S::Erfc, Some(0)) => b.int(0),
        (S::Erfc, Some(2)) => b.int(2),
        (S::Erfi, Some(0)) => b.inf(),
        (S::Erfi, Some(2)) => b.neg_inf(),
        (S::Erfi, Some(1)) => b.i(1),
        (S::Erfi, Some(-1)) => b.i(-1),
        (S::ExpIntegralEi, Some(0)) => b.inf(),
        (S::ExpIntegralEi, Some(2)) => b.int(0),
        (S::ExpIntegralEi, Some(1)) => b.i_pi(1, 1),
        (S::ExpIntegralEi, Some(-1)) => b.i_pi(-1, 1),
        (S::LogIntegral, Some(0)) => b.inf(),
        (S::SinIntegral, Some(0)) => ar.mul(&[ar.rational(h.int(1), h.int(2)), h.sym_term(h.sym("pi"))]),
        (S::SinIntegral, Some(2)) => ar.mul(&[ar.rational(h.int(-1), h.int(2)), h.sym_term(h.sym("pi"))]),
        // Si(i y) = i Shi(y) (DLMF 6.2.15, 6.2.16 with sin(i t) = i sinh t) and Shi(y) -> +-inf as y -> +-inf
        (S::SinIntegral, Some(1)) => ar.mul(&[b.i(1), b.inf()]),
        (S::SinIntegral, Some(-1)) => ar.mul(&[b.i(-1), b.inf()]),
        (S::CosIntegral, Some(0)) => b.int(0),
        (S::CosIntegral, Some(2)) => b.i_pi(1, 1),
        (S::CosIntegral, Some(1 | -1)) => b.inf(),
        (S::SinhIntegral, Some(0)) => b.inf(),
        (S::SinhIntegral, Some(2)) => b.neg_inf(),
        (S::SinhIntegral, Some(1)) => b.i_pi(1, 2),
        (S::SinhIntegral, Some(-1)) => b.i_pi(-1, 2),
        (S::CoshIntegral, Some(0)) => b.inf(),
        (S::CoshIntegral, Some(1)) => b.i_pi(1, 2),
        (S::CoshIntegral, Some(-1)) => b.i_pi(-1, 2),
        (S::GammaInc, Some(0)) | (S::GammaRegularized, Some(0)) => b.int(0),
        (S::EllipticK, Some(0 | 2)) => b.int(0),
        (S::EllipticF, Some(0 | 2)) | (S::InverseJacobiSn, Some(0 | 2)) => b.int(0),
        (S::EllipticPi, Some(0 | 2)) => b.int(0),
        (S::AiryAi | S::AiryAiPrime, Some(0)) => b.int(0),
        (S::AiryBi | S::AiryBiPrime, Some(0)) => b.inf(),
        (S::BesselK, Some(0)) => b.int(0),
        // |J_nu(x)|, |Y_nu(x)| <= C/sqrt(|x|) along the real axis for a real order (DLMF 10.17.3, 10.17.4), and the
        // continuation to x < 0 only multiplies by e^(+-i pi nu) and adds i cos(pi nu) J_nu (DLMF 10.11.1, 10.11.2)
        (S::BesselJ | S::BesselY, Some(0 | 2)) if args.len() == 2 && matches!(ar.num(args[0]), Some(Num::Real(_))) => b.int(0),
        // log Gamma along the negative axis has no limit (its real part follows ln|Gamma|, which has poles and
        // zeros-like dips between them); along the imaginary axis |log Gamma(i y)| grows like y log y (DLMF 5.11.1)
        (S::LogGamma, Some(2)) => h.sym_term(h.sym("indeterminate")),
        (S::LogGamma, Some(1 | -1)) => h.sym_term(h.sym("complex_infinity")),
        // the complete E(m) = sqrt(1 - m) E(m/(m - 1)) (the imaginary-modulus transformation, DLMF 19.7.5) with
        // E(1) = 1: E(m) ~ sqrt(1 - m) as |m| -> inf in every direction, +inf along the negative axis; along the
        // positive axis the value lies on the cut and only its modulus has a limit
        (S::EllipticE, Some(2)) if args.len() == 1 => b.inf(),
        (S::EllipticE, Some(0)) if args.len() == 1 => h.sym_term(h.sym("complex_infinity")),
        _ if matches!(f, S::EllipticE) && args.len() == 1 && last == h.sym_term(h.sym("complex_infinity")) => {
            h.sym_term(h.sym("complex_infinity"))
        }
        _ => match (f, first) {
            (S::EllipticPi, Some(0 | 2)) => b.int(0),
            _ => return None,
        },
    };
    Some(v)
}

/// I_z(a, b) at z = inf, -inf or complex infinity for positive integers a, b (at most 10^4): then
/// I_z(a, b) = sum_(j=a)^(n) C(n, j) z^j (1 - z)^(n - j) with n = a + b - 1 (DLMF 8.17.5 with 8.17.2) is a polynomial
/// of degree n >= 1 whose leading coefficient sum_(j=a)^(n) C(n, j) (-1)^(n - j) = (-1)^(b - 1) C(n - 1, a - 1) is not 0
/// (the alternating partial sum of a binomial row). So the value is (-1)^(b - 1) inf at inf, (-1)^(a) inf at -inf
/// ((-1)^(b - 1 + n) = (-1)^a), and complex infinity at complex infinity.
fn beta_regularized_at_infinity<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, args: &[T<'h>]) -> Option<T<'h>> {
    let [z, a, b] = args else { return None };
    let small = |t: &T<'h>| match h.view(*t) {
        View::Int(v) if (1..=10_000).contains(&v) => Some(v),
        _ => None,
    };
    let (a, b) = (small(a)?, small(b)?);
    let inf = h.sym_term(h.sym("inf"));
    let signed = |odd: bool| if odd { ar.neg(inf) } else { inf };
    if *z == h.sym_term(h.sym("complex_infinity")) {
        return Some(*z);
    }
    match direction(h, ar, *z)? {
        0 => Some(signed((b - 1) % 2 == 1)),
        2 => Some(signed(a % 2 == 1)),
        _ => None,
    }
}

/// The signed limit at an exact pole on the real line, where it has one: `cos_integral(0)` is `-inf`.
pub fn at_pole<'h>(h: &'h Heap, ar: &Arith<'h, Heap>, f: Special, args: &[T<'h>]) -> Option<T<'h>> {
    use Special as S;
    let b = B { h, ar };
    let x = ar.num(*args.last()?)?;
    let is = |v: i64| mc_arith::number::equal(&x, &Num::int(v)) == Some(true);
    Some(match f {
        S::CosIntegral | S::CoshIntegral | S::ExpIntegralEi if x.is_exact_zero() => b.neg_inf(),
        S::LogIntegral if is(1) => b.neg_inf(),
        S::LogGamma => b.inf(),
        S::InverseErf if is(1) => b.inf(),
        S::InverseErf if is(-1) => b.neg_inf(),
        S::InverseErfc if x.is_exact_zero() => b.inf(),
        S::InverseErfc if is(2) => b.neg_inf(),
        S::Polylog if is(1) => b.inf(),
        _ => return None,
    })
}
