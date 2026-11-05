//! [`Diff`]: the derivative of one term with respect to one variable.
//!
//! The term is walked as a DAG on an explicit stack (no recursion on terms, decision 1.6), each distinct subterm
//! differentiated once (a memo keyed by the hash-consed handle) after its arguments. A node's derivative is built
//! from its arguments and their derivatives:
//!
//! | node | derivative |
//! |---|---|
//! | the variable | 1 |
//! | any other atom | 0 |
//! | `a1 + ... + an` | the sum of the derivatives |
//! | `a1 * ... * an` | the sum over i of `ai' * (the other factors)` |
//! | `b^c` | `c b^(c-1) b'` if `c' = 0`, `b^c log(b) c'` if `b' = 0`, else `b^c (c' log(b) + c b'/b)` |
//! | a known function ([`crate::rules`]) | its rule: partials times argument derivatives, linear, branches |
//! | `f(a1, ..., ak)`, f unknown | the sum over i of `derivative(0, .., 1, .., 0)(f)(a...) * ai'` |
//! | `derivative(n...)(f)(a...)` | the same with the order of argument i raised; for a known f, its derivative |
//!
//! The variable may be any term that is not a number, list, sum, product or power: its occurrences are recognised
//! by identity (`d(y(t)^2, y(t))` is `2 y(t)`). A head that depends on the variable is outside this package
//! ([`Halt::Unsupported`]).
use crate::rules::{self, Rule};
use crate::term::Build;
use mc_api::{Store, View, T};
use mc_eval::Abort;
use rustc_hash::FxHashMap;

/// Why a derivative was not computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Halt {
    /// the evaluator's fuel or interrupt (passed on)
    Abort(Abort),
    /// the term has a part this package cannot differentiate: `d` stays unevaluated
    Unsupported,
}

impl From<Abort> for Halt {
    fn from(a: Abort) -> Halt {
        Halt::Abort(a)
    }
}

/// A fuel and interrupt budget: charged once per differentiated node.
pub type Tick<'t> = dyn FnMut(u64) -> Result<(), Abort> + 't;

/// The derivative with respect to one variable, with its memo.
pub struct Diff<'b, 'h> {
    b: &'b Build<'h>,
    x: T<'h>,
    memo: FxHashMap<T<'h>, T<'h>>,
    free: FxHashMap<T<'h>, bool>,
}

impl<'b, 'h> Diff<'b, 'h> {
    /// The derivative with respect to `x`.
    pub fn new(b: &'b Build<'h>, x: T<'h>) -> Diff<'b, 'h> {
        Diff { b, x, memo: FxHashMap::default(), free: FxHashMap::default() }
    }

    /// The derivative of `t`.
    pub fn of(&mut self, t: T<'h>, tick: &mut Tick<'_>) -> Result<T<'h>, Halt> {
        let h = self.b.heap();
        let mut stack = vec![(t, false)];
        while let Some((u, ready)) = stack.pop() {
            if self.memo.contains_key(&u) {
                continue;
            }
            if u == self.x {
                self.memo.insert(u, self.b.one());
                continue;
            }
            let View::App { head, args } = h.view(u) else {
                self.memo.insert(u, self.b.zero());
                continue;
            };
            if !ready {
                stack.push((u, true));
                // the condition of a conditional is not differentiated
                let skip = usize::from(self.rule_of(head, args.len()).is_some_and(|r| matches!(r, Rule::Branches)));
                for &a in args.iter().skip(skip).rev() {
                    if !self.memo.contains_key(&a) {
                        stack.push((a, false));
                    }
                }
                continue;
            }
            tick(1)?;
            let v = self.node(head, args, tick)?;
            self.memo.insert(u, v);
        }
        Ok(self.memo[&t])
    }

    /// The derivative of an already visited argument.
    fn dv(&self, a: T<'h>) -> T<'h> {
        self.memo[&a]
    }

    /// The rule of a call with a symbol head, if the name has one.
    fn rule_of(&self, head: T<'h>, arity: usize) -> Option<Rule> {
        let s = self.b.sym_of(head)?;
        match s {
            s if s == self.b.hd.add || s == self.b.hd.mul || s == self.b.hd.pow => None,
            s => rules::lookup(&self.b.heap().sym_name(s), arity),
        }
    }

    /// The derivative of `head(args)`, the arguments' derivatives (except a condition's) known.
    fn node(&mut self, head: T<'h>, args: &[T<'h>], tick: &mut Tick<'_>) -> Result<T<'h>, Halt> {
        let b = self.b;
        let h = b.heap();
        if let Some(s) = b.sym_of(head) {
            if s == b.hd.add {
                let ds: Vec<T<'h>> = args.iter().map(|&a| self.dv(a)).filter(|&d| !b.is_zero(d)).collect();
                return Ok(b.add(&ds));
            }
            if s == b.hd.mul {
                return Ok(self.product(args));
            }
            if s == b.hd.pow {
                if let [base, ex] = args {
                    return Ok(self.power(head, *base, *ex));
                }
            }
            return match rules::lookup(&h.sym_name(s), args.len()) {
                Some(Rule::Partials(ps)) if ps.len() == args.len() => Ok(self.chain(args, |i| ps[i](b, args))),
                Some(Rule::Partials(_)) | None => Ok(self.chain(args, |i| b.app(b.derivative(&unit(args.len(), i), head), args))),
                Some(Rule::Constant) => Ok(b.zero()),
                Some(Rule::Linear) => {
                    let ds: Vec<T<'h>> = args.iter().map(|&a| self.dv(a)).collect();
                    Ok(h.app(head, &ds))
                }
                Some(Rule::Branches) => {
                    let mut out = vec![args[0]];
                    out.extend(args[1..].iter().map(|&a| self.dv(a)));
                    Ok(h.app(head, &out))
                }
                Some(Rule::Unsupported) => Err(Halt::Unsupported),
            };
        }
        // a compound head: the derivative operator applied, or another function-valued term free of the variable
        if !self.is_free(head) {
            return Err(Halt::Unsupported);
        }
        if let Some((orders, f)) = b.as_derivative(head).filter(|(o, _)| o.len() == args.len()) {
            let known = b.sym_of(f).map(|s| h.sym_name(s));
            let mut parts = Vec::with_capacity(args.len());
            for (i, &a) in args.iter().enumerate() {
                if b.is_zero(self.dv(a)) {
                    parts.push(b.zero());
                    continue;
                }
                let mut o = orders.clone();
                o[i] += 1;
                parts.push(match &known {
                    Some(name) if rules::resolvable(name, &o) => resolve(b, f, &o, args, tick)?,
                    _ => b.app(b.derivative(&o, f), args),
                });
            }
            return Ok(self.chain(args, |i| parts[i]));
        }
        Ok(self.chain(args, |i| b.app(b.derivative(&unit(args.len(), i), head), args)))
    }

    /// The chain rule: the sum over the arguments of `partial(i) * ai'`, skipping arguments with derivative 0.
    fn chain(&self, args: &[T<'h>], mut partial: impl FnMut(usize) -> T<'h>) -> T<'h> {
        let b = self.b;
        let mut terms = Vec::new();
        for (i, &a) in args.iter().enumerate() {
            let da = self.dv(a);
            if !b.is_zero(da) {
                terms.push(b.mul(&[partial(i), da]));
            }
        }
        b.add(&terms)
    }

    /// The product rule.
    fn product(&self, args: &[T<'h>]) -> T<'h> {
        let b = self.b;
        let mut terms = Vec::new();
        let mut factors = Vec::with_capacity(args.len());
        for (i, &a) in args.iter().enumerate() {
            let da = self.dv(a);
            if b.is_zero(da) {
                continue;
            }
            factors.clear();
            factors.extend(args.iter().enumerate().filter(|&(j, _)| j != i).map(|(_, &f)| f));
            factors.push(da);
            terms.push(b.mul(&factors));
        }
        b.add(&terms)
    }

    /// The derivative of `base^ex`.
    fn power(&self, head: T<'h>, base: T<'h>, ex: T<'h>) -> T<'h> {
        let b = self.b;
        let (db, de) = (self.dv(base), self.dv(ex));
        match (b.is_zero(db), b.is_zero(de)) {
            (true, true) => b.zero(),
            (false, true) => {
                let lowered = b.pow(base, b.sub(ex, b.one()));
                b.mul(&[ex, lowered, db])
            }
            (true, false) => {
                let t = b.heap().app(head, &[base, ex]);
                b.mul(&[t, self.log(base), de])
            }
            (false, false) => {
                let t = b.heap().app(head, &[base, ex]);
                let inner = b.add(&[b.mul(&[de, self.log(base)]), b.mul(&[ex, db, b.powi(base, -1)])]);
                b.mul(&[t, inner])
            }
        }
    }

    /// `log(base)`, 1 for the constant e.
    fn log(&self, base: T<'h>) -> T<'h> {
        if self.b.sym_of(base) == Some(self.b.hd.e) {
            self.b.one()
        } else {
            self.b.call("log", &[base])
        }
    }

    /// Is `t` free of the variable? (Iterative, memoised.)
    fn is_free(&mut self, t: T<'h>) -> bool {
        let h = self.b.heap();
        let mut stack = vec![(t, false)];
        while let Some((u, ready)) = stack.pop() {
            if self.free.contains_key(&u) {
                continue;
            }
            if u == self.x {
                self.free.insert(u, false);
                continue;
            }
            let View::App { head, args } = h.view(u) else {
                self.free.insert(u, true);
                continue;
            };
            if ready {
                let f = self.free[&head] && args.iter().all(|a| self.free[a]);
                self.free.insert(u, f);
                continue;
            }
            stack.push((u, true));
            stack.push((head, false));
            stack.extend(args.iter().map(|&a| (a, false)));
        }
        self.free[&t]
    }
}

/// The orders (0, .., 1, .., 0) with the 1 at `i`.
fn unit(k: usize, i: usize) -> Vec<i64> {
    let mut o = vec![0; k];
    o[i] = 1;
    o
}

/// The derivative `derivative(orders)(f)` of a known function `f`, at `args`: f applied to fresh variables,
/// differentiated `orders[i]` times in the i-th of them, and the arguments substituted for the variables.
pub fn resolve<'h>(b: &Build<'h>, f: T<'h>, orders: &[i64], args: &[T<'h>], tick: &mut Tick<'_>) -> Result<T<'h>, Halt> {
    let h = b.heap();
    let vars: Vec<T<'h>> = (1..=args.len()).map(|i| h.sym_term(h.sym(&format!("derivative$z{i}")))).collect();
    let mut g = h.app(f, &vars);
    for (&v, &n) in vars.iter().zip(orders) {
        g = nth(b, g, v, n, tick)?;
    }
    Ok(substitute(b, g, &vars, args, tick)?)
}

/// The n-th derivative of `t` in `x`, stopping early at 0 or at a fixed point (`exp`).
pub fn nth<'h>(b: &Build<'h>, mut t: T<'h>, x: T<'h>, n: i64, tick: &mut Tick<'_>) -> Result<T<'h>, Halt> {
    for _ in 0..n {
        let next = Diff::new(b, x).of(t, tick)?;
        if b.is_zero(next) || next == t {
            return Ok(next);
        }
        t = next;
    }
    Ok(t)
}

/// `t` with each of `vars` (symbols) replaced by the corresponding term of `vals`, sums, products and powers
/// rebuilt canonically.
fn substitute<'h>(b: &Build<'h>, t: T<'h>, vars: &[T<'h>], vals: &[T<'h>], tick: &mut Tick<'_>) -> Result<T<'h>, Abort> {
    let h = b.heap();
    let mut done: FxHashMap<T<'h>, T<'h>> = vars.iter().copied().zip(vals.iter().copied()).collect();
    let mut stack = vec![(t, false)];
    while let Some((u, ready)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        let View::App { head, args } = h.view(u) else {
            done.insert(u, u);
            continue;
        };
        if ready {
            tick(1)?;
            let a: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
            let v = b.app(done[&head], &a);
            done.insert(u, v);
            continue;
        }
        stack.push((u, true));
        stack.push((head, false));
        stack.extend(args.iter().map(|&a| (a, false)));
    }
    Ok(done[&t])
}
