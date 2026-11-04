//! [`B`]: the term builder of one call (canonical sums, products and powers through `mc-arith`, plain calls for
//! everything else), and [`rewrite`], the bottom-up walk every transform of this crate is built on.
use mc_api::{names, Store, Sym, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The symbols this crate looks at, interned once per call.
pub struct Heads {
    /// `add`
    pub add: Sym,
    /// `mul`
    pub mul: Sym,
    /// `pow`
    pub pow: Sym,
    /// `list`
    pub list: Sym,
    /// `complex`, the head of a complex number
    pub complex: Sym,
    /// the constant `e`
    pub e: Sym,
    /// `exp`
    pub exp: Sym,
    /// `log`
    pub log: Sym,
    /// `sin`
    pub sin: Sym,
    /// `cos`
    pub cos: Sym,
    /// `tan`
    pub tan: Sym,
    /// `cot`
    pub cot: Sym,
    /// `sec`
    pub sec: Sym,
    /// `csc`
    pub csc: Sym,
    /// `sinh`
    pub sinh: Sym,
    /// `cosh`
    pub cosh: Sym,
    /// `tanh`
    pub tanh: Sym,
}

/// The builder: the heap, its canonical arithmetic and the interned heads.
pub struct B<'h> {
    /// the canonical arithmetic
    pub ar: Arith<'h, Heap>,
    /// the heads
    pub hd: Heads,
    h: &'h Heap,
}

impl<'h> B<'h> {
    /// A builder on `h`; big exact numbers poll `stop`.
    pub fn new(h: &'h Heap, stop: &Arc<AtomicBool>) -> B<'h> {
        let hd = Heads {
            add: h.sym(names::ADD),
            mul: h.sym(names::MUL),
            pow: h.sym(names::POW),
            list: h.sym(names::LIST),
            complex: h.sym(names::COMPLEX),
            e: h.sym("e"),
            exp: h.sym("exp"),
            log: h.sym("log"),
            sin: h.sym("sin"),
            cos: h.sym("cos"),
            tan: h.sym("tan"),
            cot: h.sym("cot"),
            sec: h.sym("sec"),
            csc: h.sym("csc"),
            sinh: h.sym("sinh"),
            cosh: h.sym("cosh"),
            tanh: h.sym("tanh"),
        };
        B { ar: Arith::new(h).with_stop(stop.clone()), hd, h }
    }

    /// The heap.
    pub fn heap(&self) -> &'h Heap {
        self.h
    }
    /// A machine integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// The rational n/d (canonical).
    pub fn rat(&self, n: i64, d: i64) -> T<'h> {
        self.ar.rational(self.h.int(n), self.h.int(d))
    }
    /// The complex number `re + im*i` (parts canonical real numbers, `im` not zero).
    pub fn complex(&self, re: T<'h>, im: T<'h>) -> T<'h> {
        self.h.call(self.hd.complex, &[re, im])
    }
    /// The imaginary unit.
    pub fn i(&self) -> T<'h> {
        self.complex(self.int(0), self.int(1))
    }
    /// A canonical sum.
    pub fn add(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.int(0),
            [a] => *a,
            _ => self.ar.add(args),
        }
    }
    /// A canonical product.
    pub fn mul(&self, args: &[T<'h>]) -> T<'h> {
        match args {
            [] => self.int(1),
            [a] => *a,
            _ => self.ar.mul(args),
        }
    }
    /// A canonical power.
    pub fn pow(&self, b: T<'h>, e: T<'h>) -> T<'h> {
        self.ar.pow(b, e)
    }
    /// `b^k` for a machine integer k.
    pub fn powi(&self, b: T<'h>, k: i64) -> T<'h> {
        self.ar.pow(b, self.h.int(k))
    }
    /// `-a`
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.ar.neg(a)
    }
    /// `a - b`
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.sub(a, b)
    }
    /// `a / b`
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.div(a, b)
    }
    /// The call `s(args...)`, evaluated later.
    pub fn call(&self, s: Sym, args: &[T<'h>]) -> T<'h> {
        self.h.call(s, args)
    }
    /// The call `name(args...)`, evaluated later.
    pub fn call_named(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// `head(args...)` with canonical construction when the head is `add`, `mul` or `pow`.
    pub fn app(&self, head: T<'h>, args: &[T<'h>]) -> T<'h> {
        match (self.sym_of(head), args) {
            (Some(s), _) if s == self.hd.add => self.add(args),
            (Some(s), _) if s == self.hd.mul => self.mul(args),
            (Some(s), [b, e]) if s == self.hd.pow => self.pow(*b, *e),
            _ => self.h.app(head, args),
        }
    }
    /// The symbol of a symbol term.
    pub fn sym_of(&self, t: T<'h>) -> Option<Sym> {
        match self.h.view(t) {
            View::Sym(s) => Some(s),
            _ => None,
        }
    }
    /// `(s, args)` when `t` is a call with symbol head `s`.
    pub fn call_of(&self, t: T<'h>) -> Option<(Sym, &'h [T<'h>])> {
        match self.h.view(t) {
            View::App { head, args } => self.sym_of(head).map(|s| (s, args)),
            _ => None,
        }
    }
    /// The argument of a one-argument call of `s`.
    pub fn unary(&self, t: T<'h>, s: Sym) -> Option<T<'h>> {
        match self.call_of(t) {
            Some((g, [a])) if g == s => Some(*a),
            _ => None,
        }
    }
    /// The terms of a sum (`t` itself when it is not one).
    pub fn terms(&self, t: T<'h>) -> Vec<T<'h>> {
        match self.call_of(t) {
            Some((s, args)) if s == self.hd.add => args.to_vec(),
            _ => vec![t],
        }
    }
    /// The factors of a product (`t` itself when it is not one).
    pub fn factors(&self, t: T<'h>) -> Vec<T<'h>> {
        match self.call_of(t) {
            Some((s, args)) if s == self.hd.mul => args.to_vec(),
            _ => vec![t],
        }
    }
    /// `(base, k)` when `t` is `base^k` with a machine integer `k`; `(t, 1)` otherwise.
    pub fn int_power(&self, t: T<'h>) -> (T<'h>, i64) {
        match self.call_of(t) {
            Some((s, [b, e])) if s == self.hd.pow => match self.h.view(*e) {
                View::Int(k) => (*b, k),
                _ => (t, 1),
            },
            _ => (t, 1),
        }
    }
    /// Is `t` an exact or inexact number (complex numbers included)?
    pub fn is_number(&self, t: T<'h>) -> bool {
        self.ar.num(t).is_some()
    }
    /// The parts `(re, im)` of a complex number term.
    pub fn complex_parts(&self, t: T<'h>) -> Option<(T<'h>, T<'h>)> {
        match self.call_of(t) {
            Some((s, [re, im])) if s == self.hd.complex => Some((*re, *im)),
            _ => None,
        }
    }
    /// `exp(u)` as a call (also for the base `e`).
    pub fn exp(&self, u: T<'h>) -> T<'h> {
        self.call(self.hd.exp, &[u])
    }
    /// The exponent `u` when `t` is `exp(u)` or `e^u`.
    pub fn exp_arg(&self, t: T<'h>) -> Option<T<'h>> {
        if let Some(u) = self.unary(t, self.hd.exp) {
            return Some(u);
        }
        match self.call_of(t) {
            Some((s, [b, u])) if s == self.hd.pow && self.sym_of(*b) == Some(self.hd.e) => Some(*u),
            _ => None,
        }
    }
}

/// The size of `t` as a tree: every symbol, number, string and call head counts 1 (the cost [`crate::simplify`]
/// minimises). Shared subterms are counted once per occurrence, through a memo over the DAG.
pub fn leaf_count<'h>(h: &'h Heap, t: T<'h>) -> u64 {
    let mut memo: FxHashMap<T<'h>, u64> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    while let Some((u, ready)) = stack.pop() {
        if memo.contains_key(&u) {
            continue;
        }
        match h.view(u) {
            View::App { head, args } => {
                if ready {
                    let n = memo[&head] + args.iter().map(|a| memo[a]).sum::<u64>();
                    memo.insert(u, n);
                } else {
                    stack.push((u, true));
                    stack.push((head, false));
                    stack.extend(args.iter().map(|&a| (a, false)));
                }
            }
            View::Rational { .. } => {
                memo.insert(u, 3);
            }
            _ => {
                memo.insert(u, 1);
            }
        }
    }
    memo[&t]
}

/// A rule of [`rewrite`]: the replacement of one (already rebuilt) node, or `None` to keep it.
pub type Rule<'r, 'h> = dyn FnMut(&mut NativeCx<'_, '_, 'h>, &B<'h>, T<'h>) -> Res<Option<T<'h>>> + 'r;

/// `t` rewritten bottom up: every compound node is rebuilt from its rewritten arguments (canonically for sums,
/// products and powers) and then offered to `rule`, whose replacement is kept as it is (not walked again). Each
/// node costs one unit of fuel. Iterative, with a memo over the DAG, so deep and shared terms are cheap.
pub fn rewrite<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>, rule: &mut Rule<'_, 'h>) -> Res<T<'h>> {
    let h = b.heap();
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut args_buf = Vec::new();
    while let Some((u, ready)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            done.insert(u, u);
            continue;
        };
        // numbers (complex numbers are calls) are left alone
        if b.complex_parts(u).is_some() {
            done.insert(u, u);
            continue;
        }
        if !ready {
            stack.push((u, true));
            stack.extend(args.iter().filter(|a| !done.contains_key(*a)).map(|&a| (a, false)));
            continue;
        }
        cx.tick(1)?;
        args_buf.clear();
        args_buf.extend(args.iter().map(|a| done[a]));
        let node = if args_buf.as_slice() == args { u } else { b.app(head, &args_buf) };
        let out = rule(cx, b, node)?.unwrap_or(node);
        done.insert(u, out);
    }
    Ok(done[&t])
}

/// `t` rewritten with `rule` and evaluated, repeatedly, until it no longer changes (at most `max` rounds).
pub fn fixed<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mut t: T<'h>, max: usize, rule: &mut Rule<'_, 'h>) -> Res<T<'h>> {
    for _ in 0..max {
        let r = rewrite(cx, b, t, rule)?;
        if r == t {
            break;
        }
        let r = cx.eval(r)?;
        if r == t {
            break;
        }
        t = r;
    }
    Ok(t)
}

/// `name(t)` evaluated (a library function of another package: `together`, `factor`, `expand`, ...).
pub fn apply<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, name: &str, t: T<'h>) -> Res<T<'h>> {
    cx.eval(b.call_named(name, &[t]))
}
