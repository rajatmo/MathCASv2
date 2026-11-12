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
//! | `root_sum(p, t => g)`, p free of the variable | `root_sum(p, t => d(g))` (item F1) |
//! | `f(a1, ..., ak)`, f unknown | the sum over i of `derivative(0, .., 1, .., 0)(f)(a...) * ai'` |
//! | `derivative(n...)(f)(a...)` | the same with the order of argument i raised; for a known f, its derivative |
//!
//! The variable may be any term that is not a number, list, sum, product or power: its occurrences are recognised
//! by identity (`d(y(t)^2, y(t))` is `2 y(t)`). A head that depends on the variable is outside this crate
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
    /// the term has a part this crate cannot differentiate: `d` stays unevaluated
    Unsupported,
}

impl From<Abort> for Halt {
    fn from(a: Abort) -> Halt {
        Halt::Abort(a)
    }
}

/// A fuel and interrupt budget: charged once per differentiated node.
pub type Tick<'t> = dyn FnMut(u64) -> Result<(), Abort> + 't;

/// The pieces `(value, condition)` of a piecewise function and its default.
pub type Pieces<'h> = (Vec<(T<'h>, T<'h>)>, Option<T<'h>>);

/// Does `t` contain an explicit complex number (`complex(re, im)`)?
fn has_complex<'h, S: Store + ?Sized>(h: &S, t: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if matches!(h.view(head), View::Sym(s) if &*h.sym_name(s) == "complex") {
                return true;
            }
            stack.extend_from_slice(args);
        }
    }
    false
}

/// The pieces `(value, condition)` and the default of `piecewise([[v1, c1], ...], default)`; `None` for another shape.
pub fn piecewise_parts<'h>(b: &Build<'h>, args: &[T<'h>]) -> Option<Pieces<'h>> {
    let h = b.heap();
    let (list, default) = match args {
        [l] => (*l, None),
        [l, d] => (*l, Some(*d)),
        _ => return None,
    };
    let View::App { head, args: items } = h.view(list) else { return None };
    if b.sym_of(head) != Some(b.hd.list) {
        return None;
    }
    let mut pieces = Vec::with_capacity(items.len());
    for &it in items {
        match h.view(it) {
            View::App { head, args: &[v, c] } if b.sym_of(head) == Some(b.hd.list) => pieces.push((v, c)),
            _ => return None,
        }
    }
    Some((pieces, default))
}

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
            // a constant list (the parameters of hypergeometric_pfq or meijer_g): its
            // derivative is 0, not a list of zeros (R22-03)
            if !ready && self.b.sym_of(head) == Some(self.b.hd.list) && self.is_free(u) {
                // the parts too (a rule may ask for the derivative of an argument inside the list)
                let mut parts = vec![u];
                while let Some(w) = parts.pop() {
                    self.memo.insert(w, self.b.zero());
                    if let View::App { head, args } = h.view(w) {
                        parts.push(head);
                        parts.extend(args.iter().copied());
                    }
                }
                continue;
            }
            // a constant algebraic number root(f, k): its pure function is not differentiated as a term (round 37)
            if !ready && self.b.sym_of(head) == Some(h.sym("root")) && self.is_free(u) {
                self.memo.insert(u, self.b.zero());
                continue;
            }
            if !ready {
                stack.push((u, true));
                // the condition of a conditional is not differentiated
                let rule = self.rule_of(head, args.len());
                let skip = usize::from(rule.is_some_and(|r| matches!(r, Rule::Branches)));
                let children: Vec<T<'h>> = match rule {
                    // a sum over roots: only the summand's body (its functions are not differentiated as terms)
                    _ if self.root_sum_body(head, args).is_some() => self.root_sum_body(head, args).into_iter().collect(),
                    // the values and the default of a piecewise function, not its conditions
                    Some(Rule::Piecewise) => match piecewise_parts(self.b, args) {
                        Some((pieces, default)) => pieces.iter().map(|p| p.0).chain(default).collect(),
                        None => return Err(Halt::Unsupported),
                    },
                    _ => args.iter().skip(skip).copied().collect(),
                };
                for &a in children.iter().rev() {
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

    /// The derivative of `integrate(g, t)` (`integrate(d(g), t)`, or `g` when `t` is the variable) and of
    /// `integrate(g, [t, a, b])` by the Leibniz rule: `g(b) b' - g(a) a' + integrate(d(g), [t, a, b])` (decision
    /// 2A.7); `None` for other shapes.
    fn integral(&self, args: &[T<'h>], int: T<'h>) -> Option<T<'h>> {
        let b = self.b;
        let h = b.heap();
        let [g, spec] = args else { return None };
        let at = |v: T<'h>, t: T<'h>| {
            let rule = h.call(h.sym(mc_api::names::RULE), &[t, v]);
            h.call(h.sym("rewrite"), &[*g, rule])
        };
        match h.view(*spec) {
            View::Sym(_) if *spec == self.x => Some(*g),
            View::Sym(_) => {
                let dg = self.dv(*g);
                Some(if b.is_zero(dg) { b.zero() } else { h.app(int, &[dg, *spec]) })
            }
            View::App { head, args: &[t, lo, hi] } if b.sym_of(head) == Some(b.hd.list) => {
                let mut parts = vec![];
                let (dlo, dhi) = (self.dv(lo), self.dv(hi));
                if !b.is_zero(dhi) {
                    parts.push(b.mul(&[at(hi, t), dhi]));
                }
                if !b.is_zero(dlo) {
                    parts.push(b.mul(&[b.int(-1), at(lo, t), dlo]));
                }
                // the variable of integration is bound: an integrand written in the outer variable's name does not vary
                let dg = if t == self.x { b.zero() } else { self.dv(*g) };
                if !b.is_zero(dg) {
                    parts.push(h.app(int, &[dg, *spec]));
                }
                Some(b.add(&parts))
            }
            _ => None,
        }
    }

    /// The derivative of an inert transform `inactive(T)(f, t, s)` (T a Laplace, Fourier, Fourier sine or cosine
    /// transform) or `inactive(convolve)(f, g, t, s)`, itself inert: 0 in the bound variable t; in s the transform
    /// of -t f (Laplace), i t f (Fourier, kernel exp(i w t)), t f under the cosine transform (sine), -t f under the
    /// sine transform (cosine), convolve(f, g', t, s); in another variable the transform of the derivative of the
    /// integrand (differentiation under the integral sign; Debnath and Bhatta, Integral Transforms, 3.4, 2.4).
    fn transform(&self, name: &str, args: &[T<'h>], head: T<'h>) -> Option<T<'h>> {
        let b = self.b;
        let h = b.heap();
        let inert = |n: &str| h.call(h.sym("inactive"), &[h.sym_term(h.sym(n))]);
        let (fs, t, s) = match (name, args) {
            ("laplace_transform" | "fourier_transform" | "fourier_sin_transform" | "fourier_cos_transform", &[f, t, s]) => (vec![f], t, s),
            ("convolve", &[f, g, t, s]) => (vec![f, g], t, s),
            _ => return None,
        };
        if !matches!(h.view(t), View::Sym(_)) || !matches!(h.view(s), View::Sym(_)) || t == s {
            return None;
        }
        if t == self.x {
            return Some(b.zero());
        }
        let mut parts = vec![];
        if s == self.x {
            let f = fs[0];
            parts.push(match name {
                "laplace_transform" => h.app(head, &[b.mul(&[b.int(-1), t, f]), t, s]),
                "fourier_transform" => h.app(head, &[b.mul(&[h.sym_term(h.sym("im")), t, f]), t, s]),
                "fourier_sin_transform" => h.app(inert("fourier_cos_transform"), &[b.mul(&[t, f]), t, s]),
                "fourier_cos_transform" => h.app(inert("fourier_sin_transform"), &[b.mul(&[b.int(-1), t, f]), t, s]),
                _ => h.app(head, &[f, b.call("d", &[fs[1], t]), t, s]),
            });
        }
        // the integrands' own dependence on the variable
        for (i, &f) in fs.iter().enumerate() {
            let df = self.dv(f);
            if !b.is_zero(df) {
                let mut a = args.to_vec();
                a[i] = df;
                parts.push(h.app(head, &a));
            }
        }
        Some(b.add(&parts))
    }

    /// The body `g` of `root_sum(p, t => g)`, when the call has that shape.
    fn root_sum_body(&self, head: T<'h>, args: &[T<'h>]) -> Option<T<'h>> {
        let b = self.b;
        let h = b.heap();
        if b.sym_of(head).map(|s| h.sym_name(s)).as_deref() != Some("root_sum") {
            return None;
        }
        let &[_, g] = args else { return None };
        let View::App { head: gh, args: &[_, body] } = h.view(g) else { return None };
        (b.sym_of(gh).map(|s| h.sym_name(s)).as_deref() == Some("fn")).then_some(body)
    }

    /// The derivative of `root_sum(p, t => g)` with `p` free of the variable: `root_sum(p, t => d(g))` (the sum is
    /// over roots that do not depend on the variable; `mc-algebra`).
    fn root_sum(&mut self, args: &[T<'h>]) -> Option<T<'h>> {
        let b = self.b;
        let h = b.heap();
        let &[p, g] = args else { return None };
        if !self.is_free(p) {
            return None;
        }
        let View::App { head, args: &[params, body] } = h.view(g) else { return None };
        if b.sym_of(head).map(|s| h.sym_name(s)).as_deref() != Some("fn") {
            return None;
        }
        let db = self.dv(body);
        if b.is_zero(db) {
            return Some(b.zero());
        }
        Some(b.call("root_sum", &[p, h.app(head, &[params, db])]))
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
            // pFq(a; b; z): the parameters are lists (not the table's scalar arguments)
            if h.sym_name(s) == "hypergeometric_pfq" && args.len() == 3 {
                return Ok(self.chain(args, |i| if i == 2 { rules::hyppfq_z(b, args) } else { rules::opaque_partial(b, "hypergeometric_pfq", args, i) }));
            }
            if h.sym_name(s) == "hypergeometric_pfq_regularized" && args.len() == 3 {
                let name = "hypergeometric_pfq_regularized";
                return Ok(self.chain(args, |i| if i == 2 { rules::hyppfq_reg_z(b, args) } else { rules::opaque_partial(b, name, args, i) }));
            }
            if h.sym_name(s) == "bilateral_hypergeometric_pfq" && args.len() == 3 {
                let name = "bilateral_hypergeometric_pfq";
                return Ok(self.chain(args, |i| if i == 2 { rules::bilateral_z(b, args) } else { rules::opaque_partial(b, name, args, i) }));
            }
            if h.sym_name(s) == "integrate" {
                if let Some(v) = self.integral(args, head) {
                    return Ok(v);
                }
            }
            if h.sym_name(s) == "root_sum" {
                if let Some(v) = self.root_sum(args) {
                    return Ok(v);
                }
            }
            // abs, sign and arg are piecewise constant (abs: sign) only on the real line: with an explicit complex
            // number in the argument (`arg(1 + im x)`) they vary continuously, so keep the derivative form
            let name = h.sym_name(s);
            if matches!(&*name, "abs" | "sign" | "arg") && args.len() == 1 && has_complex(h, args[0]) {
                return Ok(self.chain(args, |i| b.app(b.derivative(&unit(1, i), head), args)));
            }
            // elliptic_log([x, y], [a, b]) = integral from inf to x of dt/(2 sqrt(t^3 + a t^2 + b t)) on the branch of
            // y: d/dx = 1/(2 y), and y only chooses the sign (the generic chain rule multiplied by the list [x', y'] and
            // gave a list); a varying a or b is not done here
            if &*name == "elliptic_log" {
                if let [pt, ab] = args {
                    let point = match h.view(*pt) {
                        View::App { head: lh, args: &[x, y] } if lh == h.sym_term(b.hd.list) => Some((x, y)),
                        _ => None,
                    };
                    if let (Some((x, y)), true) = (point, self.is_free(*ab)) {
                        let dx = self.dv(x);
                        return Ok(b.mul(&[dx, b.div(b.one(), b.mul(&[b.int(2), y]))]));
                    }
                }
            }
            return match rules::lookup(&name, args.len()) {
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
                Some(Rule::Piecewise) => {
                    let Some((pieces, default)) = piecewise_parts(b, args) else { return Err(Halt::Unsupported) };
                    let list = b.hd.list;
                    let ds: Vec<T<'h>> = pieces.iter().map(|&(v, c)| h.call(list, &[self.dv(v), c])).collect();
                    let mut out = vec![h.call(list, &ds)];
                    out.extend(default.map(|d| self.dv(d)));
                    Ok(h.app(head, &out))
                }
                Some(Rule::Unsupported) => Err(Halt::Unsupported),
            };
        }
        // the inert integral `inactive(integrate)(g, spec)` has the derivative of the integral, itself inert
        if let View::App { head: ih, args: &[inner] } = h.view(head) {
            if b.sym_of(ih).map(|s| h.sym_name(s)).as_deref() == Some("inactive")
                && b.sym_of(inner).map(|s| h.sym_name(s)).as_deref() == Some("integrate")
            {
                if let Some(v) = self.integral(args, head) {
                    return Ok(v);
                }
            }
            if b.sym_of(ih).map(|s| h.sym_name(s)).as_deref() == Some("inactive") {
                if let Some(name) = b.sym_of(inner).map(|s| h.sym_name(s).to_string()) {
                    if let Some(v) = self.transform(&name, args, head) {
                        return Ok(v);
                    }
                }
            }
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
                    Some(name) if rules::resolvable(name, &o) => match resolve(b, f, &o, args, tick) {
                        Err(Halt::Unsupported) => b.app(b.derivative(&o, f), args),
                        r => r?,
                    },
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
    // abs', sign' and arg' are the real-line derivatives: keep them unevaluated at a complex argument
    if b.sym_of(f).is_some_and(|s| matches!(&*h.sym_name(s), "abs" | "sign" | "arg")) && args.iter().any(|&a| has_complex(h, a)) {
        return Err(Halt::Unsupported);
    }
    let vars: Vec<T<'h>> = (1..=args.len()).map(|i| h.sym_term(h.sym(&format!("derivative$z{i}")))).collect();
    let mut g = h.app(f, &vars);
    for (&v, &n) in vars.iter().zip(orders) {
        g = nth(b, g, v, n, tick)?;
    }
    Ok(substitute(b, g, &vars, args, tick)?)
}

/// The n-th derivative of `t` in `x`, stopping early at 0 or at a fixed point (`exp`).
pub fn nth<'h>(b: &Build<'h>, mut t: T<'h>, x: T<'h>, n: i64, tick: &mut Tick<'_>) -> Result<T<'h>, Halt> {
    // indeterminate is no constant: its derivative is indeterminate, not 0
    if n > 0 && t == b.heap().sym_term(b.heap().sym("indeterminate")) {
        return Ok(t);
    }
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
