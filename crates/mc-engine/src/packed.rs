//! Functions of one argument on packed arrays (gap G48, decision sp-core.3): the parts of complex numbers (`re`,
//! `im_part`, `conjugate`, `arg`, `abs`) and the elementary functions (`exp`, `log`, `sin`, ...) applied to a packed
//! array of machine reals or machine complex numbers work on the payload and give a packed array, without making a
//! term per element.
//!
//! Each is a wrapper around the function's own native ([`mc_eval::Kernel::wrap_native`]): a call whose one argument
//! is a packed array of the kinds below is computed here, every other call goes to the native. Every element is
//! bitwise what the native gives for it alone (the tests compare the two):
//!
//! - `re`, `im_part`, `conjugate` take the parts apart; a real's imaginary part is the exact 0, its conjugate itself;
//! - `arg` of a complex number is `atan2(im, re)`; of reals it is exact (0 or pi), left to the native;
//! - `abs` of a complex number is `(re^2 + im^2)^(1/2)` as the arithmetic computes it, of a real its magnitude;
//! - the elementary functions are `mc_numeric`'s machine functions ([`Fun::machine`]): a real argument outside a
//!   function's real domain gives a complex value (`log(-2.0)`), so a list of reals may give a list mixing reals and
//!   complex numbers (built as a compound list); a complex argument always gives a complex value.
use mc_api::names;
use mc_core::{Heap, Packed, Store, T};
use mc_eval::{Kernel, Native, NativeCx, Reply, Res};
use mc_numeric::fun::Mach;
use mc_numeric::machine::C64;
use mc_numeric::Fun;

/// Leaves per fuel unit (as the evaluator's element-wise arithmetic).
const PER_UNIT: u64 = 16;

/// The function a wrapper computes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Re,
    Im,
    Conj,
    Arg,
    Abs,
    /// the square root of positive machine reals: the arithmetic's power `x^(1/2)`, `x.powf(0.5)`; also for one
    /// machine real (a hot spot of the profiles: the general native takes four times as long, decision sp-core.6)
    Sqrt,
    Elem(Fun),
}

/// The elementary functions handled here (`sqrt` is a power in the arithmetic, so it stays with its native).
const ELEMENTARY: &[Fun] = &[
    Fun::Exp,
    Fun::Log,
    Fun::Sin,
    Fun::Cos,
    Fun::Tan,
    Fun::Atan,
    Fun::Asin,
    Fun::Acos,
    Fun::Sinh,
    Fun::Cosh,
    Fun::Tanh,
    Fun::Log1p,
    Fun::Expm1,
];

/// Wrap the natives of the functions above. Call after the libraries defining them are installed (and after
/// `canonical_exp`, so the packed case is decided first); a function without a native is left alone.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let mut ops = vec![("re", Op::Re), ("im_part", Op::Im), ("conjugate", Op::Conj), ("arg", Op::Arg), ("abs", Op::Abs), ("sqrt", Op::Sqrt)];
    ops.extend(ELEMENTARY.iter().map(|&f| (f.name(), Op::Elem(f))));
    for (name, op) in ops {
        k.wrap_native(h.sym(name), |inner| Box::new(OnPacked { inner, op }));
    }
}

/// A native with the packed case of [`Op`] in front.
struct OnPacked {
    inner: Box<dyn Native>,
    op: Op,
}

impl Native for OnPacked {
    fn reads_term(&self) -> bool {
        self.inner.reads_term()
    }

    fn curried(&self) -> bool {
        self.inner.curried()
    }

    fn trace(&mut self, visit: &mut dyn FnMut(&mut mc_core::Raw)) {
        self.inner.trace(visit);
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        if let [x] = args {
            if self.op == Op::Sqrt {
                if let mc_core::View::Real(v) = cx.heap().view(*x) {
                    if v.is_finite() && v > 0.0 {
                        return Ok(Reply::Done(cx.heap().real(v.powf(0.5))));
                    }
                }
            }
            if let Some(v) = self.packed(cx, *x)? {
                return Ok(Reply::Done(v));
            }
        }
        self.inner.call(cx, t, args)
    }

    fn packed<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, x: T<'h>) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        let Some(p) = h.packed(x) else { return Ok(None) };
        cx.tick(p.count() as u64 / PER_UNIT)?;
        Ok(apply(h, self.op, p))
    }
}

/// `op` of every leaf of `p`; `None` where the native decides (integer leaves, `arg` of reals).
fn apply<'h>(h: &'h Heap, op: Op, p: Packed<'h>) -> Option<T<'h>> {
    let shape: Vec<usize> = p.dims().iter().map(|&d| d as usize).collect();
    match (op, p) {
        (Op::Re | Op::Conj, Packed::Reals { .. }) => Some(h.real_array(&shape, p.reals()?)),
        (Op::Im, Packed::Reals { data, .. }) => Some(h.int_array(&shape, &vec![0; data.len()])),
        (Op::Abs, Packed::Reals { data, .. }) => Some(h.real_array(&shape, &data.iter().map(|x| x.abs()).collect::<Vec<_>>())),
        (Op::Re, Packed::Complexes { data, .. }) => Some(h.real_array(&shape, &data.iter().step_by(2).copied().collect::<Vec<_>>())),
        (Op::Im, Packed::Complexes { data, .. }) => {
            Some(h.real_array(&shape, &data.iter().skip(1).step_by(2).copied().collect::<Vec<_>>()))
        }
        (Op::Conj, Packed::Complexes { data, .. }) => {
            Some(h.complex_array(&shape, &data.chunks(2).flat_map(|z| [z[0], -z[1]]).collect::<Vec<_>>()))
        }
        (Op::Arg, Packed::Complexes { data, .. }) => {
            Some(h.real_array(&shape, &data.chunks(2).map(|z| z[1].atan2(z[0])).collect::<Vec<_>>()))
        }
        (Op::Abs, Packed::Complexes { data, .. }) => {
            // the native's sqrt(re^2 + im^2): the sum of two machine reals, then the machine power 1/2
            Some(h.real_array(&shape, &data.chunks(2).map(|z| (z[0] * z[0] + z[1] * z[1]).powf(0.5)).collect::<Vec<_>>()))
        }
        (Op::Sqrt, Packed::Reals { data, .. }) if data.iter().all(|&x| x.is_finite() && x > 0.0) => {
            Some(h.real_array(&shape, &data.iter().map(|x| x.powf(0.5)).collect::<Vec<_>>()))
        }
        (Op::Elem(f), Packed::Reals { data, .. }) => Some(machine_values(h, &shape, data.iter().map(|&x| f.machine(Mach::Real(x))))),
        (Op::Elem(f), Packed::Complexes { data, .. }) => {
            Some(machine_values(h, &shape, data.chunks(2).map(|z| f.machine(Mach::Complex(C64::new(z[0], z[1]))))))
        }
        _ => None,
    }
}

/// The array of machine values: packed reals or packed complex numbers when all are of one kind, otherwise a
/// compound list of both.
fn machine_values<'h>(h: &'h Heap, shape: &[usize], vals: impl Iterator<Item = Mach>) -> T<'h> {
    let vals: Vec<Mach> = vals.collect();
    if vals.iter().all(|m| matches!(m, Mach::Real(_))) {
        let re: Vec<f64> = vals.iter().map(|m| m.c64().re).collect();
        return h.real_array(shape, &re);
    }
    if vals.iter().all(|m| matches!(m, Mach::Complex(_))) {
        let parts: Vec<f64> = vals.iter().flat_map(|m| [m.c64().re, m.c64().im]).collect();
        return h.complex_array(shape, &parts);
    }
    let complex = h.sym(names::COMPLEX);
    let n = vals.len();
    mc_api::packed::build_nested(h, shape, n, |i| match vals[i] {
        Mach::Real(x) => h.real(x),
        Mach::Complex(z) => h.call(complex, &[h.real(z.re), h.real(z.im)]),
    })
}
