//! The minimal built-ins (decision F.13), each a [`Native`] registered by [`Kernel::new`](crate::Kernel::new):
//!
//! | symbol | arguments | result |
//! |---|---|---|
//! | `iadd`, `imul` | machine integers | sum, product (declines on overflow) |
//! | `isub`, `iquo`, `imod` | two machine integers | difference, Euclidean quotient and remainder (declines on a zero or, for `imod`, negative divisor) |
//! | `less`, `less_equal`, `greater`, `greater_equal` | two machine integers | `true` / `false` |
//! | `same` | two terms | `true` when identical |
//! | `eval` | one, strict | the argument (evaluated, as strict arguments are) |
//! | `if` | condition, then, else (the last two held; the else branch may be missing) | the chosen branch, evaluated (`null` for a false condition without else) |
//! | `map` | function, `list(...)` | `list(f(x)...)`, elements as a speculative group |
//! | `print` | a term | the term; appends it to the output (effect) |
//! | `define` | symbol (held), value | `null`; sets the own value (effect) |
//!
//! `list` needs no native: it is an ordinary head. Arithmetic on big integers, rationals and reals is `mc-arith`'s;
//! these natives decline anything that is not a machine integer, so its natives can replace them later.
use crate::defs::{Attributes, Hold};
use crate::eval::{Kernel, Res};
use crate::native::{Native, NativeCx, Reply};
use mc_core::{Heap, Store, View, T};

fn int(h: &Heap, t: T<'_>) -> Option<i64> {
    match h.view(t) {
        View::Int(v) => Some(v),
        _ => None,
    }
}

/// A fold over machine integers.
struct Fold(fn(i64, i64) -> Option<i64>, i64);

impl Native for Fold {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        cx.tick(args.len() as u64 / 64)?;
        let mut acc = self.1;
        for &a in args {
            let Some(v) = int(h, a) else { return Ok(Reply::Decline) };
            let Some(r) = (self.0)(acc, v) else { return Ok(Reply::Decline) };
            acc = r;
        }
        Ok(Reply::Done(h.int(acc)))
    }
}

/// A binary operation on machine integers.
struct Binary(fn(i64, i64) -> Option<i64>);

impl Native for Binary {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [a, b] = args else { return Ok(Reply::Decline) };
        Ok(match int(h, *a).zip(int(h, *b)).and_then(|(x, y)| (self.0)(x, y)) {
            Some(v) => Reply::Done(h.int(v)),
            None => Reply::Decline,
        })
    }
}

/// A comparison of two machine integers.
struct Compare(fn(i64, i64) -> bool);

impl Native for Compare {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [a, b] = args else { return Ok(Reply::Decline) };
        Ok(match (int(h, *a), int(h, *b)) {
            (Some(x), Some(y)) => Reply::Done(cx.bool((self.0)(x, y))),
            _ => Reply::Decline,
        })
    }
}

/// `same(a, b)`: structural identity (semantics 1.1), always decided.
struct Same;

impl Native for Same {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        // `same(a, b, c, ...)`: all identical; one argument or none is trivially true
        Ok(Reply::Done(cx.bool(args.windows(2).all(|w| w[0] == w[1]))))
    }
}

/// `if(c, a, b)`: `c` evaluated, the branches held.
struct If;

impl Native for If {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let (c, a, b) = match *args {
            [c, a, b] => (c, a, Some(b)),
            [c, a] => (c, a, None),
            _ => return Ok(Reply::Decline),
        };
        Ok(if cx.is_true(c) {
            Reply::Again(a)
        } else if cx.is_false(c) {
            b.map_or(Reply::Done(cx.null()), Reply::Again)
        } else {
            Reply::Decline
        })
    }
}

/// `map(f, list(xs...))`: the elements form a speculative group (semantics 18.2 item 4).
struct Map;

impl Native for Map {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [f, xs] = args else { return Ok(Reply::Decline) };
        let Some(xs) = cx.as_list(*xs) else { return Ok(Reply::Decline) };
        let h = cx.heap();
        let f = *f;
        // the learned cost of `f`'s calls; 0 when `f` is declared `effect` (no static reason to split)
        let cost = match h.view(f) {
            View::Sym(s) if !cx.is_effect(f) => cx.head_cost(s).max(1),
            _ => 0,
        };
        let out = cx.group(xs.len(), cost, &|cx: &mut NativeCx<'_, '_, 'h>, i| {
            cx.tick(1)?;
            let h = cx.heap();
            cx.eval(h.app(f, &[xs[i]]))
        })?;
        Ok(Reply::Done(cx.list(&out)))
    }
}

/// `print(x)`: appends `x` to the output, returns it.
struct Print;

impl Native for Print {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else {
            // `print(a, b, ...)`: one line, the texts of the arguments joined (decision p6core.24)
            if args.len() < 2 {
                return Ok(Reply::Decline);
            }
            let h = cx.heap();
            let text = h.sym_term(h.sym("to_string"));
            let parts: Vec<T<'h>> = args.iter().map(|&a| h.app(text, &[a])).collect();
            let joined = h.app(h.sym_term(h.sym("string_join")), &parts);
            return Ok(Reply::Again(h.app(h.sym_term(h.sym("print")), &[joined])));
        };
        cx.emit(*x)?;
        Ok(Reply::Done(*x))
    }
}

/// `define(s, v)`: the own value of symbol `s` (held) becomes `v`.
struct Define;

impl Native for Define {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [s, v] = args else { return Ok(Reply::Decline) };
        let View::Sym(s) = cx.heap().view(*s) else { return Ok(Reply::Decline) };
        cx.set_own(s, *v)?;
        Ok(Reply::Done(cx.null()))
    }
}

/// Register the built-ins.
pub(crate) fn register(k: &mut Kernel, h: &Heap) {
    let strict = Attributes::default();
    let mut reg = |name: &str, a: Attributes, n: Box<dyn Native>| k.register(h.sym(name), a, n);
    reg("iadd", strict, Box::new(Fold(i64::checked_add, 0)));
    reg("imul", strict, Box::new(Fold(i64::checked_mul, 1)));
    reg("isub", strict, Box::new(Binary(i64::checked_sub)));
    reg("iquo", strict, Box::new(Binary(|x, y| if y == 0 { None } else { x.checked_div_euclid(y) })));
    reg("imod", strict, Box::new(Binary(|x, m| if m <= 0 { None } else { Some(x.rem_euclid(m)) })));
    reg("less", strict, Box::new(Compare(|x, y| x < y)));
    reg("less_equal", strict, Box::new(Compare(|x, y| x <= y)));
    reg("greater", strict, Box::new(Compare(|x, y| x > y)));
    reg("greater_equal", strict, Box::new(Compare(|x, y| x >= y)));
    reg("same", strict, Box::new(Same));
    reg("if", Attributes { hold: Hold::Rest, ..strict }, Box::new(If));
    reg("map", strict, Box::new(Map));
    reg("print", Attributes { effect: true, ..strict }, Box::new(Print));
    reg("eval", strict, Box::new(EvalNow));
    reg("unevaluated", Attributes { hold: Hold::All, ..strict }, Box::new(Unevaluated));
    reg("define", Attributes { hold: Hold::First, effect: true, ..strict }, Box::new(Define));
    crate::syntax::register(k, h);
    crate::lang::register(k, h);
    crate::nonlocal::register(k, h);
    crate::constrained::register(k, h);
}

/// `eval(x)`: `x`, evaluated (as every strict argument is). Inside a held argument it marks the part to evaluate
/// first; elsewhere it is the identity (decision p6core.14).
struct EvalNow;

impl Native for EvalNow {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let [x] = args else { return Ok(Reply::Decline) };
        Ok(Reply::Done(*x))
    }
}

/// `unevaluated(x)`: stays as it is; as a strict argument of a call the call receives `x` itself, unevaluated, and
/// gets the wrapper back if no definition applies (the evaluator does it, [`crate::machine`]). Its first evaluation
/// turns the unwrapping on.
struct Unevaluated;

impl Native for Unevaluated {
    fn reads_term(&self) -> bool {
        false
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        cx.ev.k.uneval_seen.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(Reply::Decline)
    }
}

/// A native that always declines (a placeholder while a native is being wrapped, [`crate::Kernel::wrap_native`]).
pub struct Declines;

impl Native for Declines {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Decline)
    }
}
