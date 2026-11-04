//! MathCAS v2 simplification (roadmap package 2B).
//!
//! - [`simplify`](mod@simplify): `simplify(e)`, the cheapest equivalent form found by a bounded search over
//!   rational, trigonometric and exponential rewritings (cost: the leaf count);
//! - [`trig`]: `trig_to_exp`, `exp_to_trig`, `trig_expand`, `trig_reduce`;
//! - [`power`]: `power_expand`, the power and logarithm laws without their side conditions;
//! - [`term`]: the term builder and the bottom-up rewriting walk they share.
//!
//! The rational moves are package K's (`together`, `cancel`, `factor`, `expand`), called through the evaluator.
//!
//! ```
//! use mc_bench::{Engine, StopFlag};
//! let mut e = mc_engine::CoreEngine::new();
//! let stop = StopFlag::new();
//! assert_eq!(e.eval("simplify(sin(x)^2 + cos(x)^2)", &stop).unwrap().printed, "1");
//! assert_eq!(e.eval("simplify(d(x/(1 + x), x))", &stop).unwrap().printed, "(1 + x)^-2");
//! ```
//!
//! Decisions of this package are numbered 2B.1, 2B.2, ... in the comments where they are applied.
#![warn(missing_docs)]
pub mod power;
pub mod simplify;
pub mod term;
pub mod trig;

use mc_api::{Store, T};
use mc_core::Heap;
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use term::B;

/// Every function of this package with a one-line description.
pub const FUNCTIONS: &[(&str, &str)] = &[
    (
        "simplify",
        "simplify(e): the simplest equivalent form found (fewest leaves) by rational, trigonometric and exponential rewritings",
    ),
    ("trig_expand", "trig_expand(e): sines and cosines of sums and integer multiples by the addition theorems, expanded"),
    ("trig_reduce", "trig_reduce(e): products and powers of sines and cosines as sums of multiple angles"),
    ("trig_to_exp", "trig_to_exp(e): trigonometric and hyperbolic functions as exponentials"),
    ("exp_to_trig", "exp_to_trig(e): exponentials of imaginary arguments by Euler's formula, exp(i x) = cos(x) + i sin(x)"),
    (
        "power_expand",
        "power_expand(e): (a b)^c = a^c b^c, (a^b)^c = a^(b c), log(a b) = log(a) + log(b), log(a^b) = b log(a), without side conditions",
    ),
];

/// The functions of this package.
#[derive(Clone, Copy)]
enum Fun {
    Simplify,
    TrigExpand,
    TrigReduce,
    TrigToExp,
    ExpToTrig,
    PowerExpand,
}

const TABLE: &[(&str, Fun)] = &[
    ("simplify", Fun::Simplify),
    ("trig_expand", Fun::TrigExpand),
    ("trig_reduce", Fun::TrigReduce),
    ("trig_to_exp", Fun::TrigToExp),
    ("exp_to_trig", Fun::ExpToTrig),
    ("power_expand", Fun::PowerExpand),
];

/// The native of one function: one argument, mapped over nothing else (a list argument is walked like any term).
struct SimplifyNative {
    f: Fun,
    stop: Arc<AtomicBool>,
}

impl Native for SimplifyNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [e] = args else { return Ok(Reply::Decline) };
        let h: &'h Heap = cx.heap();
        let _scope = mc_arith::num::StopScope::enter(Some(&self.stop));
        let b = B::new(h, &self.stop);
        let r = match self.f {
            Fun::Simplify => simplify::simplify(cx, &b, *e)?,
            Fun::TrigExpand => trig::trig_expand(cx, &b, *e)?,
            Fun::TrigReduce => trig::trig_reduce(cx, &b, *e)?,
            Fun::TrigToExp => trig::trig_to_exp(cx, &b, *e)?,
            Fun::ExpToTrig => trig::exp_to_trig(cx, &b, *e)?,
            Fun::PowerExpand => {
                let r = power::power_expand(cx, &b, *e)?;
                cx.eval(r)?
            }
        };
        Ok(Reply::Done(r))
    }

    fn reads_term(&self) -> bool {
        false
    }
}

/// Register the natives of this package with `k`. `stop` is the flag `mc-arith`'s number kernels poll (the
/// engine's arithmetic stop flag). Install after `mc-poly`: `simplify` calls its functions.
pub fn install(k: &mut Kernel, h: &Heap, stop: &Arc<AtomicBool>) {
    for &(name, f) in TABLE {
        k.register(h.sym(name), Attributes::default(), Box::new(SimplifyNative { f, stop: stop.clone() }));
    }
}
