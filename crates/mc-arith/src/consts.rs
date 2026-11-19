//! Certified rectangles of exact numeric constants, for the one power rule that needs a value:
//! `(b^a)^k = b^(a k)` for real a and k holds when `a arg(b)` lies in `(-pi, pi]` (then `arg(b^a) = a arg(b)` and
//! `log(b^a) = a log(b)`). That is certain for a base proved positive (any real a), for `a = 2` or `a = -2` and a
//! base with a positive real part (`|arg b| < pi/2`), and for `a = -1` and a base off the negative real axis.
//!
//! The rectangle of a term is computed in exact rational interval arithmetic from rational and complex rational
//! numbers, the constants `pi`, `e`, `euler_gamma`, `catalan`, `golden_ratio`, `degree` (each between two
//! 14-decimal rationals around its value), sums, products and integer powers up to 8 in magnitude. Any other term
//! (a variable, a machine or ball number, a function) gives no rectangle, and the power stays as it is.
use crate::num::{Int, Rat};
use crate::term::{int_of, real_of, Names};
use crate::number::Real;
use mc_api::{Store, View, T};
use std::cmp::Ordering;

/// A closed interval of rationals.
#[derive(Clone)]
struct Iv(Rat, Rat);

impl Iv {
    fn point(x: Rat) -> Iv {
        Iv(x.clone(), x)
    }
    fn add(&self, o: &Iv) -> Iv {
        Iv(self.0.add(&o.0), self.1.add(&o.1))
    }
    fn neg(&self) -> Iv {
        Iv(self.1.neg(), self.0.neg())
    }
    fn mul(&self, o: &Iv) -> Iv {
        let ps = [self.0.mul(&o.0), self.0.mul(&o.1), self.1.mul(&o.0), self.1.mul(&o.1)];
        let mut lo = ps[0].clone();
        let mut hi = ps[0].clone();
        for p in &ps[1..] {
            if p.cmp_value(&lo) == Ordering::Less {
                lo = p.clone();
            }
            if p.cmp_value(&hi) == Ordering::Greater {
                hi = p.clone();
            }
        }
        Iv(lo, hi)
    }
    fn is_zero(&self) -> bool {
        self.0.is_zero() && self.1.is_zero()
    }
    fn positive(&self) -> bool {
        self.0.cmp_value(&Rat::from_i64(0)) == Ordering::Greater
    }
    fn negative(&self) -> bool {
        self.1.cmp_value(&Rat::from_i64(0)) == Ordering::Less
    }
}

/// A rectangle `re + i im` holding a complex number.
#[derive(Clone)]
struct Rect {
    re: Iv,
    im: Iv,
}

impl Rect {
    fn real(x: Iv) -> Rect {
        Rect { re: x, im: Iv::point(Rat::from_i64(0)) }
    }
    fn add(&self, o: &Rect) -> Rect {
        Rect { re: self.re.add(&o.re), im: self.im.add(&o.im) }
    }
    fn mul(&self, o: &Rect) -> Rect {
        if self.im.is_zero() && o.im.is_zero() {
            return Rect::real(self.re.mul(&o.re));
        }
        Rect { re: self.re.mul(&o.re).add(&self.im.mul(&o.im).neg()), im: self.re.mul(&o.im).add(&self.im.mul(&o.re)) }
    }
}

/// `m/10^14` and `(m + 1)/10^14`.
fn around(m: i64) -> Iv {
    let d = Int::from_i64(100_000_000_000_000);
    Iv(Rat::new(Int::from_i64(m), d.clone()), Rat::new(Int::from_i64(m + 1), d))
}

/// Most nodes visited for one rectangle.
const NODES: usize = 64;

fn rect<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, t: T<'h>, budget: &mut usize) -> Option<Rect> {
    *budget = budget.checked_sub(1)?;
    if let Some(Real::Exact(q)) = real_of(st, t) {
        return Some(Rect::real(Iv::point(q)));
    }
    match st.view(t) {
        View::Sym(s) => Some(Rect::real(match st.sym_name(s).as_str() {
            "pi" => around(314_159_265_358_979),
            "e" => around(271_828_182_845_904),
            "euler_gamma" => around(57_721_566_490_153),
            "catalan" => around(91_596_559_417_721),
            "golden_ratio" => around(161_803_398_874_989),
            "degree" => around(1_745_329_251_994),
            _ => return None,
        })),
        View::App { head, args } => {
            if head == n.complex && args.len() == 2 {
                let (Some(Real::Exact(a)), Some(Real::Exact(b))) = (real_of(st, args[0]), real_of(st, args[1])) else { return None };
                Some(Rect { re: Iv::point(a), im: Iv::point(b) })
            } else if head == n.add || head == n.mul {
                let mut acc: Option<Rect> = None;
                for &a in args {
                    let r = rect(st, n, a, budget)?;
                    acc = Some(match acc {
                        None => r,
                        Some(x) if head == n.add => x.add(&r),
                        Some(x) => x.mul(&r),
                    });
                }
                acc
            } else if head == n.pow && args.len() == 2 {
                let k = int_of(st, args[1])?;
                let k = k.to_i64().filter(|k| (1..=8).contains(k))?;
                let b = rect(st, n, args[0], budget)?;
                let mut r = b.clone();
                for _ in 1..k {
                    r = r.mul(&b);
                }
                Some(r)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Is `(b^a)^k = b^(a k)` certain for every real k, by a rectangle of the constant `b` (see the module text)?
pub(crate) fn power_of_power_holds<'h, S: Store + ?Sized>(st: &'h S, n: &Names<'h>, b: T<'h>, a: &Real) -> bool {
    let mut budget = NODES;
    let Some(r) = rect(st, n, b, &mut budget) else { return false };
    let positive_real = r.im.is_zero() && r.re.positive();
    if positive_real {
        return true;
    }
    let Real::Exact(q) = a else { return false };
    if !q.is_int() {
        return false;
    }
    match q.num.to_i64() {
        Some(2) | Some(-2) => r.re.positive(),
        Some(-1) => r.re.positive() || r.im.positive() || r.im.negative(),
        _ => false,
    }
}
