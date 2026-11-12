//! Machine-real elementary functions (semantics 2.7): the one shared set of Rust functions (spike E decision E.4)
//! that the term evaluator here and every compiled back end call, so results are bitwise identical within a
//! platform build. IEEE binary64, round to nearest; overflow gives infinity and invalid operations NaN.
//!
//! [`REAL`] lists the real functions by name for back ends. [`C64`] extends them to principal complex values,
//! used by the term evaluator when a real argument is outside a function's real domain (`log(-2.0)`,
//! `asin(2.0)`) or the argument is complex.
use crate::fun::Mach;
use std::f64::consts::FRAC_PI_2;
use std::ops::{Add, Div, Mul, Sub};

/// e^x.
pub fn exp(x: f64) -> f64 {
    x.exp()
}
/// The natural logarithm (NaN for x < 0).
pub fn log(x: f64) -> f64 {
    x.ln()
}
/// The sine.
pub fn sin(x: f64) -> f64 {
    x.sin()
}
/// The cosine.
pub fn cos(x: f64) -> f64 {
    x.cos()
}
/// The tangent.
pub fn tan(x: f64) -> f64 {
    x.tan()
}
/// The inverse tangent.
pub fn atan(x: f64) -> f64 {
    x.atan()
}
/// The inverse sine (NaN outside [-1, 1]).
pub fn asin(x: f64) -> f64 {
    x.asin()
}
/// The inverse cosine (NaN outside [-1, 1]).
pub fn acos(x: f64) -> f64 {
    x.acos()
}
/// The hyperbolic sine.
pub fn sinh(x: f64) -> f64 {
    x.sinh()
}
/// The hyperbolic cosine.
pub fn cosh(x: f64) -> f64 {
    x.cosh()
}
/// The hyperbolic tangent.
pub fn tanh(x: f64) -> f64 {
    x.tanh()
}
/// The square root (NaN for x < 0).
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}
/// x^y (`powf`).
pub fn pow(x: f64, y: f64) -> f64 {
    x.powf(y)
}

/// log(1 + x), accurate for small x (the stable form `n` uses for `log(1 + u)`)
pub fn log1p(x: f64) -> f64 {
    x.ln_1p()
}
/// e^x - 1, accurate for small x (the stable form for `exp(u) - 1`)
pub fn expm1(x: f64) -> f64 {
    x.exp_m1()
}

/// A machine function of one argument.
pub type RealFn = fn(f64) -> f64;

/// The real functions of one argument by name, for back ends.
pub const REAL: &[(&str, RealFn)] = &[
    ("exp", exp),
    ("log", log),
    ("sin", sin),
    ("cos", cos),
    ("tan", tan),
    ("atan", atan),
    ("asin", asin),
    ("acos", acos),
    ("sinh", sinh),
    ("cosh", cosh),
    ("tanh", tanh),
    ("sqrt", sqrt),
];

/// A complex machine number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct C64 {
    /// The real part.
    pub re: f64,
    /// The imaginary part.
    pub im: f64,
}

impl Add for C64 {
    type Output = C64;
    fn add(self, o: C64) -> C64 {
        C64::new(self.re + o.re, self.im + o.im)
    }
}

impl Sub for C64 {
    type Output = C64;
    fn sub(self, o: C64) -> C64 {
        C64::new(self.re - o.re, self.im - o.im)
    }
}

impl Mul for C64 {
    type Output = C64;
    fn mul(self, o: C64) -> C64 {
        C64::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }
}

/// The textbook quotient, scaled to avoid premature overflow.
impl Div for C64 {
    type Output = C64;
    fn div(self, o: C64) -> C64 {
        let s = o.re.abs().max(o.im.abs());
        if s == 0.0 || !s.is_finite() {
            let d = o.re * o.re + o.im * o.im;
            return C64::new((self.re * o.re + self.im * o.im) / d, (self.im * o.re - self.re * o.im) / d);
        }
        let (a, b) = (o.re / s, o.im / s);
        let d = a * a + b * b;
        C64::new((self.re * a + self.im * b) / d / s, (self.im * a - self.re * b) / d / s)
    }
}

impl C64 {
    /// re + im i.
    pub fn new(re: f64, im: f64) -> C64 {
        C64 { re, im }
    }
    /// A real number.
    pub fn real(re: f64) -> C64 {
        C64 { re, im: 0.0 }
    }
    /// e^z.
    pub fn exp(self) -> C64 {
        let r = self.re.exp();
        C64::new(r * self.im.cos(), r * self.im.sin())
    }
    /// The principal logarithm: imaginary part in (-pi, pi].
    pub fn log(self) -> C64 {
        C64::new(self.re.hypot(self.im).ln(), self.im.atan2(self.re))
    }
    /// The principal square root: real part >= 0.
    pub fn sqrt(self) -> C64 {
        if self.im == 0.0 {
            return if self.re >= 0.0 {
                C64::real(self.re.sqrt())
            } else {
                C64::new(0.0, (-self.re).sqrt().copysign(self.im))
            };
        }
        let m = self.re.hypot(self.im);
        let a = ((m + self.re) / 2.0).sqrt();
        let b = ((m - self.re) / 2.0).sqrt().copysign(self.im);
        C64::new(a, b)
    }
    /// The sine.
    pub fn sin(self) -> C64 {
        C64::new(self.re.sin() * self.im.cosh(), self.re.cos() * self.im.sinh())
    }
    /// The cosine.
    pub fn cos(self) -> C64 {
        C64::new(self.re.cos() * self.im.cosh(), -(self.re.sin() * self.im.sinh()))
    }
    /// The tangent.
    pub fn tan(self) -> C64 {
        self.sin() / self.cos()
    }
    /// The hyperbolic sine.
    pub fn sinh(self) -> C64 {
        C64::new(self.re.sinh() * self.im.cos(), self.re.cosh() * self.im.sin())
    }
    /// The hyperbolic cosine.
    pub fn cosh(self) -> C64 {
        C64::new(self.re.cosh() * self.im.cos(), self.re.sinh() * self.im.sin())
    }
    /// The hyperbolic tangent.
    pub fn tanh(self) -> C64 {
        self.sinh() / self.cosh()
    }
    /// The principal inverse tangent, atan z = (log(1 + iz) - log(1 - iz)) / (2i).
    pub fn atan(self) -> C64 {
        let iz = C64::new(-self.im, self.re);
        let one = C64::real(1.0);
        let d = (one + iz).log() - (one - iz).log();
        C64::new(d.im / 2.0, -d.re / 2.0)
    }
    /// The principal inverse sine, asin z = -i log(iz + sqrt(1 - z^2)).
    pub fn asin(self) -> C64 {
        let one = C64::real(1.0);
        let iz = C64::new(-self.im, self.re);
        let l = (iz + (one - self * self).sqrt()).log();
        C64::new(l.im, -l.re)
    }
    /// The principal inverse cosine, pi/2 - asin z.
    pub fn acos(self) -> C64 {
        C64::real(FRAC_PI_2) - self.asin()
    }
}

/// The elements of a list of machine numbers (machine reals, or complex numbers with machine parts), read without
/// the evaluator (a packed array through its payload); `None` for any other list or term (gap queue G48). Each
/// element keeps its kind: a complex leaf with a zero imaginary part is a complex number, as in packed complex
/// arrays and for one number alone.
pub fn machine_list<H: mc_api::Store + ?Sized>(h: &H, t: mc_api::T<'_>) -> Option<Vec<Mach>> {
    match h.packed(t) {
        Some(mc_api::Packed::Reals { dims, data }) => {
            return (dims.len() == 1).then(|| data.iter().map(|&x| Mach::Real(x)).collect());
        }
        Some(mc_api::Packed::Complexes { dims, data }) => {
            return (dims.len() == 1).then(|| data.chunks(2).map(|z| Mach::Complex(C64::new(z[0], z[1]))).collect());
        }
        _ => {}
    }
    let mc_api::View::App { head, args } = h.view(t) else { return None };
    if head != h.sym_term(h.sym(mc_api::names::LIST)) || args.len() < 64 {
        return None;
    }
    let complex = h.sym_term(h.sym(mc_api::names::COMPLEX));
    args.iter()
        .map(|&a| match h.view(a) {
            mc_api::View::Real(x) => Some(Mach::Real(x)),
            mc_api::View::App { head, args: &[re, im] } if head == complex => match (h.view(re), h.view(im)) {
                (mc_api::View::Real(x), mc_api::View::Real(y)) => Some(Mach::Complex(C64::new(x, y))),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

/// The list term of machine numbers: packed reals when every element is real, otherwise each element a real or
/// a complex number by its kind.
pub fn machine_list_term<'h, H: mc_api::Store + ?Sized>(h: &'h H, vals: &[Mach]) -> mc_api::T<'h> {
    if vals.iter().all(|m| matches!(m, Mach::Real(_))) {
        let re: Vec<f64> = vals.iter().map(|m| m.c64().re).collect();
        return h.reals(&re);
    }
    let complex = h.sym(mc_api::names::COMPLEX);
    let items: Vec<mc_api::T<'h>> = vals
        .iter()
        .map(|m| match *m {
            Mach::Real(x) => h.real(x),
            Mach::Complex(z) => h.call(complex, &[h.real(z.re), h.real(z.im)]),
        })
        .collect();
    h.call(h.sym(mc_api::names::LIST), &items)
}

/// A sum or product of machine real lists (equal lengths) and machine numbers, elementwise without the evaluator
/// (gap queue G48). `None` unless some argument is such a list and every other argument a machine real, a machine
/// integer, or such a list; complex operands are left to the evaluator's element-wise arithmetic, which folds them
/// as the arithmetic does.
pub fn list_arith<'h, H: mc_api::Store + ?Sized>(h: &'h H, product: bool, args: &[mc_api::T<'h>]) -> Option<mc_api::T<'h>> {
    if args.len() < 2 {
        return None;
    }
    let mut scalar = if product { 1.0 } else { 0.0 };
    let mut lists: Vec<Vec<f64>> = vec![];
    for &a in args {
        match h.view(a) {
            mc_api::View::Real(x) => scalar = if product { scalar * x } else { scalar + x },
            mc_api::View::Int(n) => {
                let x = n as f64;
                scalar = if product { scalar * x } else { scalar + x }
            }
            _ => {
                let l = machine_list(h, a)?;
                lists.push(l.into_iter().map(|m| if let Mach::Real(x) = m { Some(x) } else { None }).collect::<Option<Vec<f64>>>()?);
            }
        }
    }
    let first = lists.first()?;
    let n = first.len();
    if lists.iter().any(|l| l.len() != n) {
        return None;
    }
    let out: Vec<f64> =
        (0..n).map(|i| lists.iter().fold(scalar, |acc, l| if product { acc * l[i] } else { acc + l[i] })).collect();
    Some(h.reals(&out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: C64, re: f64, im: f64) -> bool {
        (a.re - re).abs() <= 1e-14 * re.abs().max(1.0) && (a.im - im).abs() <= 1e-14 * im.abs().max(1.0)
    }

    #[test]
    fn principal_values() {
        assert!(close(C64::real(-2.0).log(), 2f64.ln(), std::f64::consts::PI));
        assert!(close(C64::real(-4.0).sqrt(), 0.0, 2.0));
        assert!(close(C64::new(3.0, 4.0).sqrt(), 2.0, 1.0));
        // asin(2) = pi/2 - i log(2 + sqrt 3) on the principal branch from above (Im z = 0+)
        let a = C64::real(2.0).asin();
        assert!((a.re - FRAC_PI_2).abs() < 1e-14 && (a.im.abs() - (2.0 + 3f64.sqrt()).ln()).abs() < 1e-14);
        // the complex functions agree with the real ones on the real line
        for x in [-0.7, 0.2, 0.9] {
            let z = C64::real(x);
            assert!(close(z.atan(), x.atan(), 0.0) && close(z.asin(), x.asin(), 0.0) && close(z.acos(), x.acos(), 0.0));
            assert!(close(z.sin(), x.sin(), 0.0) && close(z.tanh(), x.tanh(), 0.0) && close(z.exp(), x.exp(), 0.0));
        }
        // exp(i pi) = -1
        assert!(close(C64::new(0.0, std::f64::consts::PI).exp(), -1.0, 0.0));
        assert_eq!(REAL.len(), 12);
    }
}
