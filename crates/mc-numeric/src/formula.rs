//! Functions defined by formulas (decision P6N.1): a function whose values are an expression in
//! other functions gets a native from a [`Def`] instead of its own numerical algorithms.
//!
//! A [`Def`] has two rules. The **numeric** rule runs when every argument is a number and one of them is inexact
//! (a machine real, a ball, or a complex number with such parts): it returns the value, or a formula in other
//! functions (`sech(x)` is `1/cosh(x)`), which the evaluator then evaluates at the arguments' precision. The
//! **exact** rule runs otherwise (exact and symbolic arguments): special values, symmetries, closed forms
//! (`chebyshev_t(3, x)` is a polynomial), or `None` to leave the call unevaluated. Both see the call through an
//! [`Fx`], which can evaluate subterms. `n` reaches the numeric rule through its arguments: `n(sech(1))` evaluates
//! `sech(1.0)`.
//!
//! Lists thread over the arguments of a listable definition (unequal lengths give `failure(shape_mismatch)`).
use mc_api::{names, Store, View, T};
use mc_arith::num::{Int, Rat};
use mc_arith::{Arith, Kind, Num, Real};
use mc_core::Heap;
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// A rule of a [`Def`]: the replacement for the call with these arguments, or `None`.
pub type Rule = for<'a, 'b, 'c, 'k, 'h> fn(&'c mut Fx<'a, 'b, 'k, 'h>, &[T<'h>]) -> Res<Option<T<'h>>>;

/// A function defined by formulas.
#[derive(Clone, Copy)]
pub struct Def {
    /// the symbol name
    pub name: &'static str,
    /// the numbers of arguments the rules accept
    pub arities: &'static [usize],
    /// whether lists thread over the arguments
    pub listable: bool,
    /// the rule for exact or symbolic arguments
    pub exact: Option<Rule>,
    /// the rule when every argument is a number and one is inexact
    pub numeric: Option<Rule>,
    /// the one-line description for the function list
    pub summary: &'static str,
}

/// A call's access to the evaluator and the arithmetic.
pub struct Fx<'a, 'b, 'k, 'h> {
    /// the native's context
    pub cx: &'a mut NativeCx<'b, 'k, 'h>,
    /// canonical arithmetic
    pub ar: Arith<'h, Heap>,
    /// the heap
    pub h: &'h Heap,
}

impl<'h> Fx<'_, '_, '_, 'h> {
    /// Evaluates `t`.
    pub fn eval(&mut self, t: T<'h>) -> Res<T<'h>> {
        self.cx.eval(t)
    }
    /// `name(args)` (not evaluated).
    pub fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }
    /// The symbol `name` as a term.
    pub fn sym(&self, name: &str) -> T<'h> {
        self.h.sym_term(self.h.sym(name))
    }
    /// An integer.
    pub fn int(&self, v: i64) -> T<'h> {
        self.h.int(v)
    }
    /// The rational n/d.
    pub fn rat(&self, n: i64, d: i64) -> T<'h> {
        self.ar.rational(self.h.int(n), self.h.int(d))
    }
    /// An exact rational.
    pub fn q(&self, q: &Rat) -> T<'h> {
        self.ar.number(&Num::Real(Real::Exact(q.clone())))
    }
    /// An exact integer.
    pub fn integer(&self, i: &Int) -> T<'h> {
        self.ar.integer(i)
    }
    /// A machine real.
    pub fn real(&self, v: f64) -> T<'h> {
        self.h.real(v)
    }
    /// The canonical sum.
    pub fn add(&self, xs: &[T<'h>]) -> T<'h> {
        self.ar.add(xs)
    }
    /// The canonical product.
    pub fn mul(&self, xs: &[T<'h>]) -> T<'h> {
        self.ar.mul(xs)
    }
    /// The canonical power.
    pub fn pow(&self, b: T<'h>, e: T<'h>) -> T<'h> {
        self.ar.pow(b, e)
    }
    /// a - b
    pub fn sub(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.sub(a, b)
    }
    /// a / b
    pub fn div(&self, a: T<'h>, b: T<'h>) -> T<'h> {
        self.ar.div(a, b)
    }
    /// -a
    pub fn neg(&self, a: T<'h>) -> T<'h> {
        self.ar.neg(a)
    }
    /// sqrt(a), the canonical power a^(1/2)
    pub fn sqrt(&self, a: T<'h>) -> T<'h> {
        self.ar.pow(a, self.rat(1, 2))
    }
    /// `inf`
    pub fn inf(&self) -> T<'h> {
        self.sym("inf")
    }
    /// `im`, the imaginary unit
    pub fn i(&self) -> T<'h> {
        self.ar.complex(self.h.int(0), self.h.int(1))
    }
    /// `list(items)`
    pub fn list(&self, items: &[T<'h>]) -> T<'h> {
        self.cx.list(items)
    }
    /// The elements of a list.
    pub fn as_list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        self.cx.as_list(t)
    }
    /// `true` or `false`.
    pub fn bool(&self, b: bool) -> T<'h> {
        self.cx.bool(b)
    }
    /// The number `t` is.
    pub fn num(&self, t: T<'h>) -> Option<Num> {
        self.ar.num(t)
    }
    /// An exact rational argument.
    pub fn rat_of(&self, t: T<'h>) -> Option<Rat> {
        match self.ar.num(t)? {
            Num::Real(Real::Exact(q)) => Some(q),
            _ => None,
        }
    }
    /// An exact integer argument that fits in an `i64`.
    pub fn int_of(&self, t: T<'h>) -> Option<i64> {
        match self.h.view(t) {
            View::Int(v) => Some(v),
            _ => None,
        }
    }
    /// A machine real argument (exact and ball reals are not).
    pub fn machine_of(&self, t: T<'h>) -> Option<f64> {
        match self.ar.num(t)? {
            Num::Real(Real::Machine(v)) => Some(v),
            _ => None,
        }
    }
    /// A real number of any kind, as an `f64`.
    pub fn f64_of(&self, t: T<'h>) -> Option<f64> {
        match self.ar.num(t)? {
            Num::Real(r) => Some(r.to_f64()),
            _ => None,
        }
    }
    /// Is `t` the exact number v?
    pub fn is_int(&self, t: T<'h>, v: i64) -> bool {
        matches!(self.h.view(t), View::Int(x) if x == v)
    }
    /// Is `t` a call of `name`? Its arguments.
    pub fn call_of(&self, t: T<'h>, name: &str) -> Option<&'h [T<'h>]> {
        match self.h.view(t) {
            View::App { head, args } if head == self.sym(name) => Some(args),
            _ => None,
        }
    }
    /// Does `t` contain a call of `name` (or the symbol)?
    pub fn contains(&self, t: T<'h>, name: &str) -> bool {
        let s = self.sym(name);
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if u == s {
                return true;
            }
            if let View::App { head, args } = self.h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
        false
    }
    /// -t when t is a negative number or a product with a negative number (for parity rules).
    pub fn negated(&self, t: T<'h>) -> Option<T<'h>> {
        let neg_num = |u: T<'h>| match self.ar.num(u) {
            Some(Num::Real(r)) => r.is_negative(),
            _ => false,
        };
        let negative = match self.h.view(t) {
            View::App { head, args } if head == self.sym(names::MUL) => args.iter().any(|&a| neg_num(a)),
            _ => neg_num(t),
        };
        negative.then(|| self.ar.neg(t))
    }
    /// Is `t` a directed infinity `inf`, `-inf`, `im*inf`, `-im*inf`? Its direction in quarter turns (0, 2, 1, -1).
    pub fn infinity(&self, t: T<'h>) -> Option<i8> {
        let inf = self.inf();
        if t == inf {
            return Some(0);
        }
        let View::App { head, args: [c, y] } = self.h.view(t) else { return None };
        if head != self.sym(names::MUL) || *y != inf {
            return None;
        }
        let one = Rat::from_i64(1);
        match self.ar.num(*c)? {
            Num::Real(Real::Exact(r)) if r == one.neg() => Some(2),
            Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == one => Some(1),
            Num::Complex(re, Real::Exact(b)) if re.is_exact_zero() && b == one.neg() => Some(-1),
            _ => None,
        }
    }
    /// Charges fuel.
    pub fn tick(&mut self, n: u64) -> Res<()> {
        self.cx.tick(n)
    }
    /// `failure(tags...)`
    pub fn failure(&self, tags: &[&str]) -> T<'h> {
        self.cx.failure(tags)
    }
}

/// Is the number inexact (a machine real or a ball, or complex with such parts)?
pub fn inexact(n: &Num) -> bool {
    n.kind() != Kind::Exact
}

struct FormulaNative {
    def: Def,
}

impl Native for FormulaNative {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let d = &self.def;
        if !d.arities.contains(&args.len()) {
            return Ok(Reply::Decline);
        }
        let h = cx.heap();
        if d.listable {
            let lists: Vec<Option<&[T<'h>]>> = args.iter().map(|&a| cx.as_list(a)).collect();
            if let Some(len) = lists.iter().flatten().map(|l| l.len()).next() {
                if lists.iter().flatten().any(|l| l.len() != len) {
                    return Ok(Reply::Done(cx.failure(&["shape_mismatch"])));
                }
                cx.tick(len as u64)?;
                let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
                let items: Vec<T<'h>> = (0..len)
                    .map(|i| {
                        let xs: Vec<T<'h>> = args.iter().zip(&lists).map(|(&a, l)| l.map_or(a, |l| l[i])).collect();
                        h.app(head, &xs)
                    })
                    .collect();
                return Ok(Reply::Again(cx.list(&items)));
            }
        }
        if crate::series_hook::any(h, args) {
            // a truncated power series (gap G59); the evaluator's own interrupt is polled by the series algebra
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            if let Some(v) = crate::series_hook::apply(cx, &stop, d.name, args)? {
                return Ok(Reply::Done(v));
            }
        }
        if !crate::uncertain::HEADS.contains(&d.name) && crate::uncertain::any(h, args) {
            if let Some(v) = crate::uncertain::apply(cx, t, args)? {
                return Ok(Reply::Again(v));
            }
        }
        if let Some(xs) = numeric_arguments(cx, args)? {
            let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
            return Ok(Reply::Again(h.app(head, &xs)));
        }
        let ar = Arith::new(h);
        let nums: Vec<Option<Num>> = args.iter().map(|&a| ar.num(a)).collect();
        let numeric = nums.iter().all(Option::is_some) && nums.iter().flatten().any(inexact);
        let rule = if numeric { d.numeric.or(d.exact) } else { d.exact };
        let Some(rule) = rule else { return Ok(Reply::Decline) };
        cx.tick(1)?;
        let r = {
            let mut fx = Fx { cx, ar, h };
            rule(&mut fx, args)?
        };
        Ok(match r {
            Some(v) if v != t => Reply::Again(v),
            _ => Reply::Decline,
        })
    }
}

/// Registers the natives of `defs`.
pub fn install(k: &mut Kernel, h: &Heap, defs: &[Def]) {
    for &def in defs {
        k.register(h.sym(def.name), Attributes::default(), Box::new(FormulaNative { def }));
    }
}

/// Contagion into function arguments (decision P6N.11): when one argument is an inexact number and others are closed
/// numeric terms that are not numbers (`pi/3`, `sqrt(2)`), those become numbers of the same kind (machine reals, or
/// balls at the ball's precision), so `f(pi/3, 0.5)` evaluates like `f(1.0471975511965976, 0.5)`. `None` when
/// nothing changes or a term does not become a number.
pub fn numeric_arguments<'h>(cx: &mut NativeCx<'_, '_, 'h>, args: &[T<'h>]) -> Res<Option<Vec<T<'h>>>> {
    let h = cx.heap();
    let ar = Arith::new(h);
    let nums: Vec<Option<Num>> = args.iter().map(|&a| ar.num(a)).collect();
    if nums.iter().all(Option::is_some) || !nums.iter().flatten().any(inexact) {
        return Ok(None);
    }
    let machine = nums.iter().flatten().any(|n| n.kind() == Kind::Machine);
    let digits = if machine {
        None
    } else {
        let ball = args.iter().zip(&nums).find(|(_, n)| n.as_ref().is_some_and(inexact)).map(|(a, _)| *a);
        let Some(b) = ball else { return Ok(None) };
        let p = cx.eval(h.call(h.sym("precision"), &[b]))?;
        match ar.num(p) {
            Some(Num::Real(r)) if r.to_f64().is_finite() && r.to_f64() >= 1.0 => Some(h.int(r.to_f64().ceil() as i64)),
            _ => return Ok(None),
        }
    };
    let mut out = Vec::with_capacity(args.len());
    for (&a, n) in args.iter().zip(&nums) {
        if n.is_some() {
            out.push(a);
            continue;
        }
        // a list of exact numbers (the parameters of `meijer_g`) stays exact: its entries become balls inside the
        // function at the precision its cancellation needs, not at the precision of the argument z (X26-6)
        if cx.as_list(a).is_some_and(|l| l.iter().all(|&e| cx.as_list(e).is_some_and(|m| m.iter().all(|&x| ar.num(x).is_some_and(|v| v.is_exact()))) || ar.num(e).is_some_and(|v| v.is_exact()))) {
            out.push(a);
            continue;
        }
        let v = match digits {
            Some(d) => cx.eval(h.call(h.sym("n"), &[a, d]))?,
            None => cx.eval(h.call(h.sym("n"), &[a]))?,
        };
        if ar.num(v).is_none() {
            return Ok(None);
        }
        out.push(v);
    }
    Ok((out.as_slice() != args).then_some(out))
}
