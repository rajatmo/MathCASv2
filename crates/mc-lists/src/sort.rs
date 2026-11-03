//! Sorting: `sort(xs)` by the canonical order, `sort(xs, by)` by a predicate, and `ordered`.
//!
//! The canonical order is `mc-arith`'s [`Order`] (semantics 8), the order the engine installs as the kernel's order
//! hook; `NativeCx` offers no access to the hook itself (decision I.12). Integers, strings, symbols and compound
//! structure are compared directly (`Canonical`: the same answer, without building number values). Both sorts are
//! one stable merge sort that only asks "must `b` come before `a`?", so an inconsistent predicate gives some
//! permutation, never a panic, and every comparison charges fuel and polls the interrupt.
use crate::ctx::{apply, take, Lib, Meter};
use mc_arith::Order;
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Abort, NativeCx, Reply, Res};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::cmp::Ordering;

/// Why a sort stopped early.
enum Halt<'h> {
    /// the evaluation was aborted (fuel, interrupt)
    Abort(Abort),
    /// the predicate gave a failure: the result
    Fail(T<'h>),
}

impl From<Abort> for Halt<'_> {
    fn from(a: Abort) -> Self {
        Halt::Abort(a)
    }
}

/// The canonical order, walked like [`Order`] (heads, then argument counts, then arguments, stopping at the first
/// difference) but deciding machine integers, strings, symbols and kinds directly; a pair of other numbers, or a
/// possible complex number, is passed to [`Order`]. The answer is the same, without building number values or
/// allocating per comparison.
struct Canonical<'h> {
    h: &'h Heap,
    order: Order<'h, Heap>,
    complex: T<'h>,
    /// symbol -> index into `names`
    syms: RefCell<FxHashMap<Sym, usize>>,
    /// (name, module path): a qualified name split at its last dot (decision B.7)
    names: RefCell<Vec<(Box<str>, Box<str>)>>,
    /// the comparison's stack, kept between comparisons
    stack: RefCell<Vec<Step<'h>>>,
}

/// One step of a comparison: a pair of terms, or the argument counts that follow equal heads.
enum Step<'h> {
    Pair(T<'h>, T<'h>),
    Count(usize, usize),
}

/// The class of a term in semantics 8: numbers 0, strings 1, symbols 2, compound terms 3.
fn class(v: &View<'_>) -> u8 {
    match v {
        View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) | View::Ball { .. } => 0,
        View::Str(_) => 1,
        View::Sym(_) => 2,
        View::App { .. } => 3,
    }
}

impl<'h> Canonical<'h> {
    fn new(h: &'h Heap) -> Canonical<'h> {
        Canonical {
            h,
            order: Order::new(h),
            complex: h.sym_term(h.sym(mc_api::names::COMPLEX)),
            syms: RefCell::default(),
            names: RefCell::default(),
            stack: RefCell::default(),
        }
    }

    fn name(&self, s: Sym) -> usize {
        if let Some(&i) = self.syms.borrow().get(&s) {
            return i;
        }
        let full = self.h.sym_name(s);
        let split = match full.rfind('.') {
            Some(i) => (full[i + 1..].into(), full[..i].into()),
            None => (full.into(), "".into()),
        };
        let mut names = self.names.borrow_mut();
        names.push(split);
        self.syms.borrow_mut().insert(s, names.len() - 1);
        names.len() - 1
    }

    fn cmp(&self, a: T<'h>, b: T<'h>) -> Ordering {
        let mut stack = self.stack.borrow_mut();
        stack.clear();
        stack.push(Step::Pair(a, b));
        while let Some(step) = stack.pop() {
            let (a, b) = match step {
                Step::Count(m, n) => match m.cmp(&n) {
                    Ordering::Equal => continue,
                    o => return o,
                },
                Step::Pair(a, b) => (a, b),
            };
            if a == b {
                continue;
            }
            let (va, vb) = (self.h.view(a), self.h.view(b));
            let o = match (&va, &vb) {
                (View::Int(x), View::Int(y)) => x.cmp(y),
                // code point order is UTF-8 byte order
                (View::Str(x), View::Str(y)) => x.cmp(y),
                (View::Sym(x), View::Sym(y)) => {
                    let (i, j) = (self.name(*x), self.name(*y));
                    let names = self.names.borrow();
                    names[i].cmp(&names[j])
                }
                // a complex number is a number: its order is the arithmetic's
                (View::App { head, .. }, _) | (_, View::App { head, .. }) if *head == self.complex => {
                    self.order.cmp(a, b)
                }
                (View::App { head: h1, args: a1 }, View::App { head: h2, args: a2 }) => {
                    for (&x, &y) in a1.iter().zip(a2.iter()).rev() {
                        stack.push(Step::Pair(x, y));
                    }
                    stack.push(Step::Count(a1.len(), a2.len()));
                    stack.push(Step::Pair(*h1, *h2));
                    continue;
                }
                _ => match class(&va).cmp(&class(&vb)) {
                    Ordering::Equal => self.order.cmp(a, b),
                    o => o,
                },
            };
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    }
}

/// Runs shorter than this are sorted by insertion first.
const RUN: usize = 16;

/// Stable merge sort of `v` where `before(b, a)` says `b` must precede `a` (strictly).
fn merge_sort<'h>(
    v: &mut Vec<T<'h>>,
    before: &mut dyn FnMut(T<'h>, T<'h>) -> Result<bool, Halt<'h>>,
) -> Result<(), Halt<'h>> {
    let n = v.len();
    for lo in (0..n).step_by(RUN) {
        let hi = (lo + RUN).min(n);
        for i in lo + 1..hi {
            let mut j = i;
            while j > lo && before(v[j], v[j - 1])? {
                v.swap(j, j - 1);
                j -= 1;
            }
        }
    }
    let mut buf = Vec::with_capacity(n);
    let mut width = RUN;
    while width < n {
        buf.clear();
        for lo in (0..n).step_by(2 * width) {
            let mid = (lo + width).min(n);
            let hi = (lo + 2 * width).min(n);
            let (mut i, mut j) = (lo, mid);
            while i < mid && j < hi {
                // the right element goes first only when it must: equal elements keep their order
                if before(v[j], v[i])? {
                    buf.push(v[j]);
                    j += 1;
                } else {
                    buf.push(v[i]);
                    i += 1;
                }
            }
            buf.extend_from_slice(&v[i..mid]);
            buf.extend_from_slice(&v[j..hi]);
        }
        std::mem::swap(v, &mut buf);
        width *= 2;
    }
    Ok(())
}

/// `pred(a, b)` as a decision: `true` is true, a failure halts, anything else is false.
fn decide<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, pred: T<'h>, a: T<'h>, b: T<'h>) -> Result<bool, Halt<'h>> {
    let h = cx.heap();
    let v = apply(cx, pred, &[a, b])?;
    if lib.is_failure(h, v) {
        return Err(Halt::Fail(v));
    }
    Ok(cx.is_true(v))
}

/// `sort(xs)`: the arguments of `xs` in canonical order; `sort(xs, by)`: ordered so that `by(a, b)` is `true` when
/// `a` comes before `b` (elements for which neither holds keep their order).
pub(crate) fn sort<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h = cx.heap();
    let (xs, by) = match *args {
        [xs] => (xs, None),
        [xs, by] => (xs, Some(by)),
        _ => return Ok(Reply::Decline),
    };
    let (head, xs) = take!(lib.operand(h, t, xs, 1));
    let mut v = xs.to_vec();
    let r = match by {
        None => {
            let order = Canonical::new(h);
            let mut m = Meter::default();
            merge_sort(&mut v, &mut |b, a| {
                m.step(cx)?;
                Ok(order.cmp(b, a) == Ordering::Less)
            })
        }
        Some(by) => merge_sort(&mut v, &mut |b, a| {
            cx.tick(1)?;
            decide(lib, cx, by, b, a)
        }),
    };
    match r {
        Ok(()) => Ok(lib.rebuilt(h, head, &v)),
        Err(Halt::Abort(a)) => Err(a),
        Err(Halt::Fail(f)) => Ok(Reply::Done(f)),
    }
}

/// `ordered(xs)`: are the arguments in canonical order (equal neighbours allowed)? `ordered(xs, by)`: is
/// `by(a, b)` `true` for every pair of neighbours `a, b`?
pub(crate) fn ordered<'h>(lib: &Lib, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
    let h: &'h Heap = cx.heap();
    let (xs, by) = match *args {
        [xs] => (xs, None),
        [xs, by] => (xs, Some(by)),
        _ => return Ok(Reply::Decline),
    };
    let (_, xs) = take!(lib.operand(h, t, xs, 1));
    let order = Canonical::new(h);
    let mut m = Meter::default();
    for w in xs.windows(2) {
        let ok = match by {
            None => {
                m.step(cx)?;
                order.cmp(w[0], w[1]) != Ordering::Greater
            }
            Some(by) => {
                cx.tick(1)?;
                match decide(lib, cx, by, w[0], w[1]) {
                    Ok(b) => b,
                    Err(Halt::Abort(a)) => return Err(a),
                    Err(Halt::Fail(f)) => return Ok(Reply::Done(f)),
                }
            }
        };
        if !ok {
            return Ok(Reply::Done(cx.bool(false)));
        }
    }
    Ok(Reply::Done(cx.bool(true)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_api::Store;

    #[test]
    fn the_fast_paths_agree_with_the_order() {
        let h = mc_core::Heap::new(mc_core::Config::no_collect());
        let x = h.sym_term(h.sym("x"));
        let terms = [
            h.int(-3),
            h.int(2),
            h.int(i64::MAX),
            h.big_int(false, &[0, 1]),
            h.real(2.5),
            h.string("b"),
            h.string("ab"),
            h.string("\u{e9}"),
            h.string("z"),
            x,
            h.sym_term(h.sym("M.a")),
            h.call(h.sym("f"), &[x]),
            h.call(h.sym("complex"), &[h.int(1), h.int(1)]),
        ];
        // random compound terms over these atoms
        let heads = [h.sym_term(h.sym("f")), h.sym_term(h.sym("g")), h.sym_term(h.sym("M.f")), terms[11]];
        let mut seed = 7u64;
        let mut next = |n: usize| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as usize % n
        };
        let mut all = terms.to_vec();
        for _ in 0..300 {
            let k = next(4);
            let args: Vec<T> = (0..k).map(|_| all[next(all.len())]).collect();
            all.push(h.app(heads[next(heads.len())], &args));
        }
        let (fast, order) = (Canonical::new(&h), Order::new(&h));
        for &a in &all {
            for &b in &all {
                assert_eq!(fast.cmp(a, b), order.cmp(a, b), "{:?} {:?}", h.view(a), h.view(b));
            }
        }
    }

    #[test]
    fn merge_sort_is_stable_and_total_on_inconsistent_predicates() {
        let h = mc_core::Heap::new(mc_core::Config::no_collect());
        // pairs (key, tag) encoded as key*1000 + tag; sort by key only
        let keys: Vec<i64> = (0..1000).map(|i| ((i * 7919) % 37) * 1000 + i).collect();
        let mut v: Vec<T> = keys.iter().map(|&k| h.int(k)).collect();
        let key = |t: T| match h.view(t) {
            mc_api::View::Int(k) => k,
            _ => unreachable!(),
        };
        merge_sort(&mut v, &mut |b, a| Ok(key(b) / 1000 < key(a) / 1000)).ok().unwrap();
        let got: Vec<i64> = v.iter().map(|&t| key(t)).collect();
        let mut want = keys.clone();
        want.sort_by_key(|k| k / 1000);
        assert_eq!(got, want);
        // a predicate that is not an order: a permutation, no panic
        let mut v: Vec<T> = keys.iter().map(|&k| h.int(k)).collect();
        let mut flip = false;
        merge_sort(&mut v, &mut |_, _| {
            flip = !flip;
            Ok(flip)
        })
        .ok()
        .unwrap();
        let mut got: Vec<i64> = v.iter().map(|&t| key(t)).collect();
        got.sort();
        let mut all = keys;
        all.sort();
        assert_eq!(got, all);
    }
}
