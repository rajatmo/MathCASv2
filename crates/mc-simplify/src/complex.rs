//! `complex_expand(e)`, `complex_expand(e, [z1, ...])`: `e` as `a + b*im` with `a` and `b` real, every symbol taken
//! as real except the listed ones (decision A.9).
//!
//! The real and imaginary parts are found bottom up: sums add them, products multiply as complex numbers, integer
//! powers multiply out, other powers of a nonreal base go through the polar form `abs^c (cos(c arg) + i sin(c
//! arg))`; `exp`, `sin`, `cos`, `tan`, `sinh`, `cosh`, `tanh` by their addition theorems (`sin(a + b i) = sin(a)
//! cosh(b) + i cos(a) sinh(b)`), `log(z)` as `log(abs(z)) + i arg(z)` for a nonreal `z`; `re`, `im_part`,
//! `conjugate`, `abs` of the parts. Other functions of real arguments are real; of nonreal arguments they keep
//! `re(...)` and `im_part(...)`. A listed variable `z` has the parts `re(z)` and `im_part(z)`.
use crate::term::B;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;

/// The parts of a term.
type Parts<'h> = (T<'h>, T<'h>);

/// Integer powers multiplied out up to this exponent; larger ones go through the polar form.
const MAX_POWER: i64 = 16;

/// The expansion state of one call.
pub struct Expander<'a, 'h> {
    b: &'a B<'h>,
    complex_vars: Vec<T<'h>>,
    memo: FxHashMap<T<'h>, Parts<'h>>,
}

impl<'a, 'h> Expander<'a, 'h> {
    /// An expander with the variables `complex_vars` complex (all other symbols real).
    pub fn new(b: &'a B<'h>, complex_vars: Vec<T<'h>>) -> Expander<'a, 'h> {
        Expander { b, complex_vars, memo: FxHashMap::default() }
    }

    fn zero(&self) -> T<'h> {
        self.b.int(0)
    }

    fn named(&self, n: &str, args: &[T<'h>]) -> T<'h> {
        self.b.call_named(n, args)
    }

    /// `a + b i`
    fn join(&self, (a, c): Parts<'h>) -> T<'h> {
        self.b.add(&[a, self.b.mul(&[self.b.i(), c])])
    }

    /// The parts of `t`.
    pub fn parts(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Parts<'h>> {
        if let Some(p) = self.memo.get(&t) {
            return Ok(*p);
        }
        cx.tick(1)?;
        let p = self.compute(cx, t)?;
        self.memo.insert(t, p);
        Ok(p)
    }

    fn mul2(&self, (a, c): Parts<'h>, (d, e): Parts<'h>) -> Parts<'h> {
        let b = self.b;
        (b.sub(b.mul(&[a, d]), b.mul(&[c, e])), b.add(&[b.mul(&[a, e]), b.mul(&[c, d])]))
    }

    fn compute(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Parts<'h>> {
        let b = self.b;
        let h = b.heap();
        let z = self.zero();
        if let Some((re, im)) = b.complex_parts(t) {
            return Ok((re, im));
        }
        match h.view(t) {
            View::Sym(_) => {
                if self.complex_vars.contains(&t) {
                    return Ok((self.named("re", &[t]), self.named("im_part", &[t])));
                }
                return Ok((t, z));
            }
            View::App { .. } => {}
            _ => return Ok((t, z)),
        }
        let Some((s, args)) = b.call_of(t) else {
            return Ok((self.named("re", &[t]), self.named("im_part", &[t])));
        };
        let name = h.sym_name(s);
        if s == b.hd.add {
            let (mut re, mut im) = (vec![], vec![]);
            for &a in args {
                let (x, y) = self.parts(cx, a)?;
                re.push(x);
                im.push(y);
            }
            return Ok((b.add(&re), b.add(&im)));
        }
        if s == b.hd.mul {
            let mut acc = (b.int(1), z);
            for &a in args {
                let p = self.parts(cx, a)?;
                acc = self.mul2(acc, p);
            }
            return Ok(acc);
        }
        if s == b.hd.list {
            // a list: elementwise (the caller joins each element)
            return Ok((t, z));
        }
        if s == b.hd.pow {
            let [base, ex] = args else { return Ok((t, z)) };
            if b.sym_of(*base) == Some(b.hd.e) {
                return self.exp(cx, *ex);
            }
            let pb = self.parts(cx, *base)?;
            let pe = self.parts(cx, *ex)?;
            if pb.1 == z && pe.1 == z {
                return Ok((t, z));
            }
            if let (View::Int(n), true) = (h.view(*ex), pe.1 == z) {
                if n.abs() <= MAX_POWER && n != 0 {
                    let mut acc = (b.int(1), z);
                    for _ in 0..n.abs() {
                        acc = self.mul2(acc, pb);
                    }
                    if n < 0 {
                        // 1/(a + b i) = (a - b i)/(a^2 + b^2)
                        let m = b.add(&[b.powi(acc.0, 2), b.powi(acc.1, 2)]);
                        return Ok((b.div(acc.0, m), b.neg(b.div(acc.1, m))));
                    }
                    return Ok(acc);
                }
            }
            if pe.1 != z {
                // b^e = exp(e log b)
                let l = self.log_parts(pb);
                let p = self.mul2(pe, l);
                return self.exp_parts(p);
            }
            // a real exponent c of a nonreal base: abs^c (cos(c arg) + i sin(c arg))
            let r = b.add(&[b.powi(pb.0, 2), b.powi(pb.1, 2)]);
            let c = *ex;
            let arg = self.named("arg", &[self.join(pb)]);
            let m = b.pow(r, b.mul(&[b.rat(1, 2), c]));
            let ang = b.mul(&[c, arg]);
            return Ok((b.mul(&[m, self.named("cos", &[ang])]), b.mul(&[m, self.named("sin", &[ang])])));
        }
        let [u] = args else {
            // several arguments: real when all are
            for &a in args {
                if self.parts(cx, a)?.1 != z {
                    return Ok((self.named("re", &[t]), self.named("im_part", &[t])));
                }
            }
            return Ok((t, z));
        };
        let (a, c) = self.parts(cx, *u)?;
        let (sin, cos, sinh, cosh) = (|x| self.named("sin", &[x]), |x| self.named("cos", &[x]), |x| self.named("sinh", &[x]), |x| self.named("cosh", &[x]));
        Ok(match name.as_str() {
            "re" => (a, z),
            "im_part" => (c, z),
            "conjugate" => (a, b.neg(c)),
            "abs" => {
                if c == z {
                    (t, z)
                } else {
                    (b.pow(b.add(&[b.powi(a, 2), b.powi(c, 2)]), b.rat(1, 2)), z)
                }
            }
            "arg" if c != z => (self.named("arg", &[self.join((a, c))]), z),
            _ if c == z => (t, z),
            "exp" => return self.exp(cx, *u),
            "sin" => (b.mul(&[sin(a), cosh(c)]), b.mul(&[cos(a), sinh(c)])),
            "cos" => (b.mul(&[cos(a), cosh(c)]), b.neg(b.mul(&[sin(a), sinh(c)]))),
            "sinh" => (b.mul(&[sinh(a), cos(c)]), b.mul(&[cosh(a), sin(c)])),
            "cosh" => (b.mul(&[cosh(a), cos(c)]), b.mul(&[sinh(a), sin(c)])),
            "tan" | "tanh" => {
                let two = b.int(2);
                let (a2, c2) = (b.mul(&[two, a]), b.mul(&[two, c]));
                if name == "tan" {
                    let d = b.add(&[cos(a2), cosh(c2)]);
                    (b.div(sin(a2), d), b.div(sinh(c2), d))
                } else {
                    let d = b.add(&[cosh(a2), cos(c2)]);
                    (b.div(sinh(a2), d), b.div(sin(c2), d))
                }
            }
            "log" => self.log_parts((a, c)),
            _ => (self.named("re", &[t]), self.named("im_part", &[t])),
        })
    }

    /// The parts of `exp(u)`.
    fn exp(&mut self, cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>) -> Res<Parts<'h>> {
        let p = self.parts(cx, u)?;
        self.exp_parts(p)
    }

    fn exp_parts(&self, (a, c): Parts<'h>) -> Res<Parts<'h>> {
        let b = self.b;
        let e = b.exp(a);
        if c == self.zero() {
            return Ok((e, c));
        }
        Ok((b.mul(&[e, self.named("cos", &[c])]), b.mul(&[e, self.named("sin", &[c])])))
    }

    /// The parts of `log(a + b i)`: `log(a^2 + b^2)/2 + i arg(a + b i)` (`log(a)` for b = 0).
    fn log_parts(&self, (a, c): Parts<'h>) -> Parts<'h> {
        let b = self.b;
        if c == self.zero() {
            return (self.named("log", &[a]), c);
        }
        let r = b.add(&[b.powi(a, 2), b.powi(c, 2)]);
        (b.mul(&[b.rat(1, 2), self.named("log", &[r])]), self.named("arg", &[self.join((a, c))]))
    }

    /// `t` as `a + b i`, evaluated; lists element by element.
    pub fn expand(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<T<'h>> {
        let b = self.b;
        if let Some((s, items)) = b.call_of(t) {
            if s == b.hd.list {
                let mut out = Vec::with_capacity(items.len());
                for &x in items {
                    out.push(self.expand(cx, x)?);
                }
                return Ok(b.call(s, &out));
            }
        }
        let p = self.parts(cx, t)?;
        let j = self.join(p);
        cx.eval(j)
    }
}
