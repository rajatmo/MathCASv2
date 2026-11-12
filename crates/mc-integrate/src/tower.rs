//! The differential field of an integrand: a tower `C(x)(t_1)...(t_n)` over the constants `C`.
//!
//! Every subterm of the integrand is classified (Bronstein, *Symbolic Integration I*, chapters 3 and 5):
//!
//! - **hyperexponential** monomials `t = exp(w)`, `Dt = t Dw`. All exponentials whose arguments are rational
//!   multiples of each other (up to a constant) share one monomial: the arguments are grouped, and each group's
//!   generator `w` is chosen so that every member is an integer multiple of it (`exp(x)`, `exp(2x)`, `exp(-x/2 + 1)`
//!   become `t^2`, `t^4`, `e t^-1` with `t = exp(x/2)`). Hyperbolic functions are written with exponentials; in
//!   [`Mode::Exp`] the trigonometric functions too, as `exp(i u)` with the imaginary unit a constant of the field.
//! - **primitive** monomials: `log(u)` (`Dt = Du/u`), the inverse trigonometric and hyperbolic functions, and the
//!   special functions, each with its known derivative times `Du`: `erf`, `erfc`, `erfi`, `exp_integral_ei`,
//!   `log_integral`, `sin_integral`, `cos_integral`, `sinh_integral`, `cosh_integral`, `fresnel_s`, `fresnel_c`,
//!   the incomplete `gamma(a, u)`, `elliptic_f`, `elliptic_e`, `elliptic_pi` (the parallel integrator's view,
//!   Bronstein 2007: special functions are monomials of the tower like any other).
//! - **algebraic** monomials `t = b^(1/q)` (and `b^a` for a constant non-rational `a`), `Dt = (1/q) t Db/b`: a
//!   hyperexponential over the field below (the relation `t^q = b` is used by the ansatz).
//! - in [`Mode::Pair`], **trigonometric pairs** `s = sin(u)`, `c = cos(u)` with `Ds = c Du`, `Dc = -s Du` and the
//!   relation `s^2 + c^2 = 1`, so answers stay real.
//!
//! The constants (symbols, `pi`, `2^(1/2)`, `log(3)`, `exp(1)`) are variables with derivative zero; an exact complex
//! number contributes the imaginary unit. Inexact numbers and unknown functions are outside the tower.
//!
//! The integrand is first rewritten to a term over fresh symbols, one per variable (pass 1), so that the number of
//! variables is known before any polynomial is built; then each monomial's argument and the integrand are read as
//! rational functions and the derivations computed in order (pass 2): the variables of a monomial's argument always
//! come before it.
use crate::field::Fld;
use crate::terms::{coeffs_in, complex_parts, fun, int_of, is_free, rat_of, ratio};
use mc_api::{Store, View, T};
use mc_arith::num::Rat;
use mc_arith::Arith;
use mc_core::Heap;
use mc_poly::ratfun::RatFun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::{Poly, Z};
use rustc_hash::FxHashMap;

/// How trigonometric functions enter the tower.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// as pairs `sin(u)`, `cos(u)` with the Pythagorean relation (the ansatz; answers real)
    Pair,
    /// as `exp(i u)` (the decision procedure)
    Exp,
}

/// What a variable is.
#[derive(Clone, Debug)]
pub enum Kind<'h> {
    /// the integration variable
    X,
    /// a constant (derivative zero)
    Const,
    /// the imaginary unit (a constant with `i^2 = -1`)
    Imag,
    /// `exp(w)`, `w` the generator (rewritten)
    Exp { arg: T<'h> },
    /// `log(u)`
    Log { arg: T<'h> },
    /// `b^e` for a constant `e` (rewritten base and exponent)
    Power { base: T<'h>, expo: T<'h>, q: Option<u32> },
    /// `sin(u)`, its partner `cos(u)` at index `partner`
    Sin { arg: T<'h>, partner: usize },
    /// `cos(u)`, its partner `sin(u)` at index `partner`
    Cos { arg: T<'h>, partner: usize },
    /// a primitive with derivative `factor * D(arg)` (inverse functions, special functions)
    Prim { factor: T<'h>, arg: T<'h>, name: &'static str },
}

/// One variable: its fresh symbol, the term it stands for, and its kind.
#[derive(Clone, Debug)]
pub struct Var<'h> {
    /// the fresh symbol of pass 1
    pub sym: T<'h>,
    /// the term of the variable in results
    pub term: T<'h>,
    /// what it is
    pub kind: Kind<'h>,
    /// a candidate of the ansatz, not in the integrand
    pub candidate: bool,
}

/// A group of exponentials: generator `w` (a term in the original variables) and its variable.
#[derive(Clone, Debug)]
struct Group<'h> {
    gen: T<'h>,
    var: Option<usize>,
    /// the multiples of the generator used
    used: Vec<i64>,
}

/// The differential field of one integrand.
pub struct Tower<'h> {
    /// the field
    pub fld: Fld,
    /// the variables
    pub vars: Vec<Var<'h>>,
    /// `D v` for every variable
    pub deriv: Vec<RatFun>,
    /// the integrand
    pub f: RatFun,
    /// the index of the integration variable
    pub x: usize,
    /// the mode it was built in
    pub mode: Mode,
    /// are the monomials known to be algebraically independent (exponentials grouped, no algebraic monomial, no
    /// pair, logarithms of distinct arguments, no unknown dependencies)? The decision procedure needs it.
    pub reduced: bool,
    /// the terms of the spare constants assigned so far (variables `vars.len()`, `vars.len() + 1`, ...)
    pub spare: Vec<T<'h>>,
    /// the base of each algebraic monomial
    pub bases: Vec<Option<RatFun>>,
}

/// Is the variable `v` a monomial of the tower (not a constant)?
pub fn is_monomial(k: &Kind<'_>) -> bool {
    !matches!(k, Kind::Const | Kind::Imag)
}

/// Builds a tower.
pub struct Builder<'b, 'h> {
    pub(crate) ar: &'b Arith<'h, Heap>,
    pub(crate) h: &'h Heap,
    pub(crate) x: T<'h>,
    pub(crate) mode: Mode,
    pub(crate) vars: Vec<Var<'h>>,
    memo: FxHashMap<T<'h>, Option<T<'h>>>,
    consts: FxHashMap<T<'h>, usize>,
    groups: Vec<Group<'h>>,
    logs: FxHashMap<T<'h>, usize>,
    powers: FxHashMap<(T<'h>, T<'h>), usize>,
    pairs: FxHashMap<T<'h>, (usize, usize)>,
    prims: FxHashMap<T<'h>, usize>,
    imag: Option<usize>,
    xvar: usize,
    reduced: bool,
    /// the rewritten integrand
    pub(crate) rf: Option<T<'h>>,
}

/// The number of spare constant variables (assigned after pass 1: square roots of discriminants of the ansatz).
pub const SPARE: usize = 3;

impl<'b, 'h> Builder<'b, 'h> {
    /// A builder for integrands in `x`.
    pub fn new(ar: &'b Arith<'h, Heap>, x: T<'h>, mode: Mode) -> Self {
        let h = ar.store();
        let mut b = Builder {
            ar,
            h,
            x,
            mode,
            vars: vec![],
            memo: FxHashMap::default(),
            consts: FxHashMap::default(),
            groups: vec![],
            logs: FxHashMap::default(),
            powers: FxHashMap::default(),
            pairs: FxHashMap::default(),
            prims: FxHashMap::default(),
            imag: None,
            xvar: 0,
            reduced: true,
            rf: None,
        };
        b.xvar = b.push(x, Kind::X, false);
        b
    }

    fn push(&mut self, term: T<'h>, kind: Kind<'h>, candidate: bool) -> usize {
        let k = self.vars.len();
        let sym = self.h.sym_term(self.h.sym(&format!("integrate`v{k}")));
        self.vars.push(Var { sym, term, kind, candidate });
        k
    }

    fn sym(&self, v: usize) -> T<'h> {
        self.vars[v].sym
    }

    fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }

    fn pi(&self) -> T<'h> {
        self.h.sym_term(self.h.sym("pi"))
    }

    fn im(&self) -> T<'h> {
        self.ar.complex(self.ar.int(0), self.ar.int(1))
    }

    /// The integrand in normal form for the tower: `e^u` and `b^u` (u depending on x) as `exp`, hyperbolic
    /// functions (and, in [`Mode::Exp`], trigonometric ones) through `exp`, `log(b, u)` as a quotient.
    pub fn normalize(&self, t: T<'h>) -> T<'h> {
        let mut memo: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
        self.norm(t, &mut memo)
    }

    fn norm(&self, t: T<'h>, memo: &mut FxHashMap<T<'h>, T<'h>>) -> T<'h> {
        if let Some(&r) = memo.get(&t) {
            return r;
        }
        if is_free(self.h, t, self.x) {
            return t;
        }
        let ar = self.ar;
        let r = match self.h.view(t) {
            View::App { head, args } => {
                let nargs: Vec<T<'h>> = args.iter().map(|&a| self.norm(a, memo)).collect();
                let name = match self.h.view(head) {
                    View::Sym(s) => self.h.sym_name(s),
                    _ => String::new(),
                };
                let exp = |u: T<'h>| self.call("exp", &[u]);
                match (name.as_str(), nargs.as_slice()) {
                    ("add", _) => ar.add(&nargs),
                    ("mul", _) => ar.mul(&nargs),
                    ("pow", [b, e]) => {
                        if *b == self.h.sym_term(self.h.sym("e")) {
                            exp(*e)
                        } else if !is_free(self.h, *e, self.x) {
                            exp(ar.mul(&[*e, self.call("log", &[*b])]))
                        } else if let Some((n, u)) = fun(self.h, *b).filter(|(n, a)| n == "exp" && a.len() == 1).map(|(n, a)| (n, a[0])) {
                            let _ = n;
                            exp(ar.mul(&[*e, u]))
                        } else {
                            ar.pow(*b, *e)
                        }
                    }
                    ("log", [b, u]) => ar.div(self.call("log", &[*u]), self.call("log", &[*b])),
                    ("sinh" | "cosh" | "tanh" | "coth" | "sech" | "csch", [u]) => {
                        let (p, m) = (exp(*u), exp(ar.neg(*u)));
                        hyper(ar, &name, p, m, ar.rational(ar.int(1), ar.int(2)))
                    }
                    ("sin" | "cos" | "tan" | "cot" | "sec" | "csc", [u]) if self.mode == Mode::Exp => {
                        let i = self.im();
                        let (p, m) = (exp(ar.mul(&[i, *u])), exp(ar.neg(ar.mul(&[i, *u]))));
                        trig_exp(ar, &name, p, m, i)
                    }
                    _ => self.h.app(head, &nargs),
                }
            }
            _ => t,
        };
        memo.insert(t, r);
        r
    }

    /// The derivative factor `g` of a primitive `F(u)` with `dF/dx = g du/dx` (normalized), or `None` when `t` is
    /// not a primitive the tower knows. The name is the head.
    fn prim_factor(&self, t: T<'h>) -> Option<(T<'h>, T<'h>, &'static str)> {
        let ar = self.ar;
        let (name, args) = fun(self.h, t)?;
        let one = ar.int(1);
        let half = ar.rational(one, ar.int(2));
        let sq = |u: T<'h>| ar.pow(u, ar.int(2));
        let exp = |u: T<'h>| self.call("exp", &[u]);
        let isqrtpi = ar.pow(self.pi(), ar.rational(ar.int(-1), ar.int(2)));
        let x = self.x;
        let free = |u: T<'h>| is_free(self.h, u, x);
        let r = match (name.as_str(), args) {
            ("atan", [u]) => (ar.pow(ar.add(&[one, sq(*u)]), ar.int(-1)), *u, "atan"),
            ("acot", [u]) => (ar.neg(ar.pow(ar.add(&[one, sq(*u)]), ar.int(-1))), *u, "acot"),
            ("atanh", [u]) => (ar.pow(ar.sub(one, sq(*u)), ar.int(-1)), *u, "atanh"),
            ("acoth", [u]) => (ar.pow(ar.sub(one, sq(*u)), ar.int(-1)), *u, "acoth"),
            ("asin", [u]) => (ar.pow(ar.sub(one, sq(*u)), ar.neg(half)), *u, "asin"),
            ("acos", [u]) => (ar.neg(ar.pow(ar.sub(one, sq(*u)), ar.neg(half))), *u, "acos"),
            ("asinh", [u]) => (ar.pow(ar.add(&[one, sq(*u)]), ar.neg(half)), *u, "asinh"),
            ("acosh", [u]) => (ar.pow(ar.sub(sq(*u), one), ar.neg(half)), *u, "acosh"),
            ("erf", [u]) => (ar.mul(&[ar.int(2), isqrtpi, exp(ar.neg(sq(*u)))]), *u, "erf"),
            ("erfc", [u]) => (ar.mul(&[ar.int(-2), isqrtpi, exp(ar.neg(sq(*u)))]), *u, "erfc"),
            ("erfi", [u]) => (ar.mul(&[ar.int(2), isqrtpi, exp(sq(*u))]), *u, "erfi"),
            ("exp_integral_ei", [u]) => (ar.div(exp(*u), *u), *u, "exp_integral_ei"),
            ("log_integral", [u]) => (ar.pow(self.call("log", &[*u]), ar.int(-1)), *u, "log_integral"),
            ("sin_integral", [u]) => (ar.div(self.call("sin", &[*u]), *u), *u, "sin_integral"),
            ("cos_integral", [u]) => (ar.div(self.call("cos", &[*u]), *u), *u, "cos_integral"),
            ("sinh_integral", [u]) => (ar.div(self.call("sinh", &[*u]), *u), *u, "sinh_integral"),
            ("cosh_integral", [u]) => (ar.div(self.call("cosh", &[*u]), *u), *u, "cosh_integral"),
            ("fresnel_s", [u]) => (self.call("sin", &[ar.mul(&[half, self.pi(), sq(*u)])]), *u, "fresnel_s"),
            ("fresnel_c", [u]) => (self.call("cos", &[ar.mul(&[half, self.pi(), sq(*u)])]), *u, "fresnel_c"),
            ("gamma", [a, u]) if free(*a) => {
                (ar.neg(ar.mul(&[ar.pow(*u, ar.sub(*a, one)), exp(ar.neg(*u))])), *u, "gamma")
            }
            ("elliptic_e", [phi, m]) if free(*m) => {
                (ar.pow(ar.sub(one, ar.mul(&[*m, sq(self.call("sin", &[*phi]))])), half), *phi, "elliptic_e")
            }
            ("elliptic_f", [phi, m]) if free(*m) => {
                (ar.pow(ar.sub(one, ar.mul(&[*m, sq(self.call("sin", &[*phi]))])), ar.neg(half)), *phi, "elliptic_f")
            }
            ("elliptic_pi", [n, phi, m]) if free(*m) && free(*n) => {
                let s2 = sq(self.call("sin", &[*phi]));
                let a = ar.pow(ar.sub(one, ar.mul(&[*n, s2])), ar.int(-1));
                let b = ar.pow(ar.sub(one, ar.mul(&[*m, s2])), ar.neg(half));
                (ar.mul(&[a, b]), *phi, "elliptic_pi")
            }
            _ => return None,
        };
        Some((self.normalize(r.0), r.1, r.2))
    }

    /// Pass 0: the arguments of the exponentials of `t` (normalized) and of the derivative factors of its
    /// primitives, grouped by rational proportionality modulo constants.
    pub fn collect_exps(&mut self, t: T<'h>) {
        let mut seen: FxHashMap<T<'h>, ()> = FxHashMap::default();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if seen.insert(u, ()).is_some() || is_free(self.h, u, self.x) {
                continue;
            }
            if let Some((n, a)) = fun(self.h, u) {
                if n == "exp" && a.len() == 1 {
                    let (vs, _) = self.split_exp(a[0]);
                    for v in vs {
                        self.add_to_group(v);
                    }
                }
                if let Some((g, _, _)) = self.prim_factor(u) {
                    stack.push(g);
                }
            }
            if let View::App { head, args } = self.h.view(u) {
                stack.push(head);
                stack.extend_from_slice(args);
            }
        }
    }

    /// `u = p + s_1 + ... + c`: the summands polynomial in x together (`p`, one exponential, so `exp(-x^2 + x)`
    /// stays one monomial), each other summand alone (`exp(x + exp(x)) = exp(x) exp(exp(x))`), and the constant
    /// `c`.
    fn split_exp(&self, u: T<'h>) -> (Vec<T<'h>>, T<'h>) {
        let (v, c) = self.split_const(u);
        let summands: Vec<T<'h>> = match fun(self.h, v) {
            Some((n, args)) if n == "add" => args.to_vec(),
            _ => vec![v],
        };
        if summands.len() < 2 {
            return (vec![v], c);
        }
        let (poly, other): (Vec<T<'h>>, Vec<T<'h>>) =
            summands.iter().partition(|&&s| coeffs_in(self.ar, s, self.x).is_some());
        let mut out = vec![];
        if !poly.is_empty() {
            out.push(self.ar.add(&poly));
        }
        out.extend(other);
        (out, c)
    }

    /// `u = v + c` with `c` the summands free of x.
    fn split_const(&self, u: T<'h>) -> (T<'h>, T<'h>) {
        match fun(self.h, u) {
            Some((n, args)) if n == "add" => {
                let (c, v): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| is_free(self.h, a, self.x));
                (self.ar.add(&v), self.ar.add(&c))
            }
            _ if is_free(self.h, u, self.x) => (self.ar.int(0), u),
            _ => (u, self.ar.int(0)),
        }
    }

    /// Put `v` into its group: a member is `n g` for the generator `g` and an integer `n`.
    fn add_to_group(&mut self, v: T<'h>) -> Option<(usize, i64)> {
        if is_free(self.h, v, self.x) {
            return None;
        }
        for (gi, g) in self.groups.iter_mut().enumerate() {
            let Some(r) = ratio(self.ar, v, g.gen) else { continue };
            if let Some(n) = rat_int(&r) {
                return Some((gi, n));
            }
            if g.var.is_some() {
                // the group's monomial exists already: a new generator would change it
                self.reduced = false;
                continue;
            }
            // a smaller generator: g / q when r = p/q
            let (p, q) = rat_pq(&r)?;
            g.gen = self.ar.mul(&[g.gen, self.ar.rational(self.ar.int(1), self.ar.int(q))]);
            for k in &mut g.used {
                *k *= q;
            }
            return Some((gi, p));
        }
        self.groups.push(Group { gen: v, var: None, used: vec![] });
        Some((self.groups.len() - 1, 1))
    }

    /// The constant `c` as a rewritten term: rational numbers stay, exact complex numbers use the imaginary
    /// unit, sums, products and integer powers of constants are rewritten inside, anything else is a variable.
    fn constant(&mut self, c: T<'h>) -> Option<T<'h>> {
        let ar = self.ar;
        if let Some(n) = ar.num(c) {
            if n.as_rat().is_some() {
                return Some(c);
            }
            let (re, im) = complex_parts(ar, c)?;
            let i = self.imag_var();
            let i = self.sym(i);
            return Some(ar.add(&[ar.number(&mc_arith::Num::rat(re)), ar.mul(&[ar.number(&mc_arith::Num::rat(im)), i])]));
        }
        match fun(self.h, c) {
            Some((n, args)) if n == "add" || n == "mul" => {
                let r: Option<Vec<T<'h>>> = args.iter().map(|&a| self.constant(a)).collect();
                let r = r?;
                return Some(if n == "add" { ar.add(&r) } else { ar.mul(&r) });
            }
            Some((n, [b, e])) if n == "pow" && int_of(self.h, *e).is_some() => {
                let b = self.constant(*b)?;
                return Some(ar.pow(b, *e));
            }
            _ => {}
        }
        if let Some(&v) = self.consts.get(&c) {
            return Some(self.sym(v));
        }
        let v = self.push(c, Kind::Const, false);
        self.consts.insert(c, v);
        Some(self.sym(v))
    }

    fn imag_var(&mut self) -> usize {
        if let Some(i) = self.imag {
            return i;
        }
        let i = self.push(self.im(), Kind::Imag, false);
        self.imag = Some(i);
        i
    }

    /// Pass 1: `t` (normalized) rewritten over the variables' symbols, registering monomials; `None` when some
    /// subterm is outside the tower.
    pub fn rw(&mut self, t: T<'h>) -> Option<T<'h>> {
        if let Some(r) = self.memo.get(&t) {
            return *r;
        }
        let r = self.rw_new(t);
        self.memo.insert(t, r);
        r
    }

    fn rw_new(&mut self, t: T<'h>) -> Option<T<'h>> {
        let ar = self.ar;
        if t == self.x {
            return Some(self.sym(self.xvar));
        }
        if is_free(self.h, t, self.x) {
            return self.constant(t);
        }
        let (name, args) = fun(self.h, t)?;
        match (name.as_str(), args) {
            ("add" | "mul", _) => {
                let r: Option<Vec<T<'h>>> = args.iter().map(|&a| self.rw(a)).collect();
                let r = r?;
                Some(if name == "add" { ar.add(&r) } else { ar.mul(&r) })
            }
            ("pow", [b, e]) => self.power(*b, *e),
            ("exp", [u]) => {
                let (vs, c) = self.split_exp(*u);
                let mut fs = vec![];
                for v in vs {
                    let (gi, n) = self.add_to_group(v)?;
                    let var = self.group_var(gi)?;
                    if !self.groups[gi].used.contains(&n) {
                        self.groups[gi].used.push(n);
                    }
                    fs.push(ar.pow(self.sym(var), ar.int(n)));
                }
                if !self.ar.num(c).is_some_and(|z| z.is_exact_zero()) {
                    fs.push(self.constant(self.call("exp", &[c]))?);
                }
                Some(ar.mul(&fs))
            }
            ("log", [u]) => {
                let a = self.rw(*u)?;
                if let Some(&v) = self.logs.get(&a) {
                    return Some(self.sym(v));
                }
                let v = self.push(t, Kind::Log { arg: a }, false);
                self.logs.insert(a, v);
                Some(self.sym(v))
            }
            ("sin" | "cos" | "tan" | "cot" | "sec" | "csc", [u]) => {
                // Mode::Exp rewrote these already
                let (s, c) = self.pair(*u)?;
                let (s, c) = (self.sym(s), self.sym(c));
                let inv = |a: T<'h>| ar.pow(a, ar.int(-1));
                Some(match name.as_str() {
                    "sin" => s,
                    "cos" => c,
                    "tan" => ar.mul(&[s, inv(c)]),
                    "cot" => ar.mul(&[c, inv(s)]),
                    "sec" => inv(c),
                    _ => inv(s),
                })
            }
            _ => {
                let (factor, arg, pname) = self.prim_factor(t)?;
                if let Some(&v) = self.prims.get(&t) {
                    return Some(self.sym(v));
                }
                let a = self.rw(arg)?;
                let g = self.rw(factor)?;
                let v = self.push(t, Kind::Prim { factor: g, arg: a, name: pname }, false);
                self.prims.insert(t, v);
                Some(self.sym(v))
            }
        }
    }

    fn group_var(&mut self, gi: usize) -> Option<usize> {
        if let Some(v) = self.groups[gi].var {
            return Some(v);
        }
        let gen = self.groups[gi].gen;
        let a = self.rw(gen)?;
        let v = self.push(self.call("exp", &[gen]), Kind::Exp { arg: a }, false);
        self.groups[gi].var = Some(v);
        Some(v)
    }

    fn pair(&mut self, u: T<'h>) -> Option<(usize, usize)> {
        let a = self.rw(u)?;
        if let Some(&p) = self.pairs.get(&a) {
            return Some(p);
        }
        self.reduced = false;
        let s = self.vars.len();
        let st = self.call("sin", &[u]);
        let ct = self.call("cos", &[u]);
        self.push(st, Kind::Sin { arg: a, partner: s + 1 }, false);
        self.push(ct, Kind::Cos { arg: a, partner: s }, false);
        self.pairs.insert(a, (s, s + 1));
        Some((s, s + 1))
    }

    /// `b^e` (e free of x) rewritten: an integer power of the rewritten base, or an algebraic monomial.
    fn power(&mut self, b: T<'h>, e: T<'h>) -> Option<T<'h>> {
        let ar = self.ar;
        if let Some(k) = int_of(self.h, e) {
            let rb = self.rw(b)?;
            return Some(ar.pow(rb, ar.int(k)));
        }
        if !is_free(self.h, e, self.x) {
            return None;
        }
        // e = p/q: b^floor(e) times (b^(1/q))^(p mod q)
        if let Some(q) = rat_of(ar, e) {
            let (p, d) = rat_pq(&q)?;
            if d > 64 {
                return None;
            }
            let fl = p.div_euclid(d);
            let rem = p.rem_euclid(d);
            let rb = self.rw(b)?;
            let one_q = ar.rational(ar.int(1), ar.int(d));
            let v = self.power_var(b, rb, one_q, Some(d as u32));
            return Some(ar.mul(&[ar.pow(rb, ar.int(fl)), ar.pow(self.sym(v), ar.int(rem))]));
        }
        // a symbolic exponent: an integer summand comes out
        let (k, rest) = match fun(self.h, e) {
            Some((n, args)) if n == "add" => {
                let (ints, rest): (Vec<T<'h>>, Vec<T<'h>>) = args.iter().partition(|&&a| int_of(self.h, a).is_some());
                (ints.iter().filter_map(|&a| int_of(self.h, a)).sum::<i64>(), ar.add(&rest))
            }
            _ => (0, e),
        };
        let rb = self.rw(b)?;
        let v = self.power_var(b, rb, rest, None);
        Some(ar.mul(&[ar.pow(rb, ar.int(k)), self.sym(v)]))
    }

    fn power_var(&mut self, b: T<'h>, rb: T<'h>, e: T<'h>, q: Option<u32>) -> usize {
        if let Some(&v) = self.powers.get(&(rb, e)) {
            return v;
        }
        self.reduced = false;
        let re = self.constant(e).unwrap_or(e);
        let v = self.push(self.ar.pow(b, e), Kind::Power { base: rb, expo: re, q }, false);
        self.powers.insert((rb, e), v);
        v
    }

    /// Register a candidate primitive (a special function of the ansatz) for the term `t`.
    pub fn candidate(&mut self, t: T<'h>) -> Option<usize> {
        if let Some(&v) = self.prims.get(&t) {
            return Some(v);
        }
        let (factor, arg, name) = self.prim_factor(t)?;
        self.collect_exps(factor);
        let a = self.rw(arg)?;
        let g = self.rw(factor)?;
        let v = self.push(t, Kind::Prim { factor: g, arg: a, name }, true);
        self.prims.insert(t, v);
        Some(v)
    }

    /// Register a candidate `t` of the ansatz with `D t = factor D(arg)` (`factor` and `arg` normalized terms in x).
    pub fn candidate_with(&mut self, t: T<'h>, factor: T<'h>, arg: T<'h>) -> Option<usize> {
        if let Some(&v) = self.prims.get(&t) {
            return Some(v);
        }
        let factor = self.normalize(factor);
        self.collect_exps(factor);
        let a = self.rw(arg)?;
        let g = self.rw(factor)?;
        let v = self.push(t, Kind::Prim { factor: g, arg: a, name: "candidate" }, true);
        self.prims.insert(t, v);
        Some(v)
    }

    /// The exponential groups: (generator, variable, multiples used).
    pub fn exp_groups(&self) -> Vec<(T<'h>, Option<usize>, Vec<i64>)> {
        self.groups.iter().map(|g| (g.gen, g.var, g.used.clone())).collect()
    }

    /// The trigonometric pairs: (argument term in x, sin variable, cos variable).
    pub fn trig_pairs(&self) -> Vec<(T<'h>, usize, usize)> {
        let mut out = vec![];
        for (k, v) in self.vars.iter().enumerate() {
            if let Kind::Sin { partner, .. } = v.kind {
                if let Some((_, args)) = fun(self.h, v.term) {
                    out.push((args[0], k, partner));
                }
            }
        }
        out
    }

    /// Pass 2: the tower, with the integrand `rf` (rewritten).
    pub fn finish(self, rf: T<'h>, m: &mut Meter) -> WorkResult<Option<Tower<'h>>> {
        let nv = self.vars.len() + SPARE;
        let fld = Fld { nv, imag: self.imag };
        let index: FxHashMap<T<'h>, usize> = self.vars.iter().enumerate().map(|(k, v)| (v.sym, k)).collect();
        let rd = Reader { h: self.h, ar: self.ar, fld, index: &index };
        let Some(f) = rd.read(rf, m)? else { return Ok(None) };
        let mut tw = Tower { fld, vars: self.vars, deriv: vec![], f, x: self.xvar, mode: self.mode, reduced: self.reduced, spare: vec![], bases: vec![] };
        tw.bases = vec![None; tw.vars.len() + SPARE];
        for k in 0..tw.vars.len() {
            let d = match tw.vars[k].kind.clone() {
                Kind::X => fld.int(1),
                Kind::Const | Kind::Imag => fld.int(0),
                Kind::Exp { arg } => {
                    let Some(a) = rd.read(arg, m)? else { return Ok(None) };
                    let da = tw.d(&a, m)?;
                    fld.mulm(&da, &fld.var(k), m)?
                }
                Kind::Log { arg } => {
                    let Some(a) = rd.read(arg, m)? else { return Ok(None) };
                    let da = tw.d(&a, m)?;
                    match fld.divm(&da, &a, m)? {
                        Some(r) => r,
                        None => return Ok(None),
                    }
                }
                Kind::Power { base, expo, .. } => {
                    let Some(b) = rd.read(base, m)? else { return Ok(None) };
                    let Some(e) = rd.read(expo, m)? else { return Ok(None) };
                    let db = tw.d(&b, m)?;
                    let Some(q) = fld.divm(&db, &b, m)? else { return Ok(None) };
                    let t = fld.mulm(&q, &e, m)?;
                    tw.bases[k] = Some(b);
                    fld.mulm(&t, &fld.var(k), m)?
                }
                Kind::Sin { arg, partner } => {
                    let Some(a) = rd.read(arg, m)? else { return Ok(None) };
                    let da = tw.d(&a, m)?;
                    fld.mulm(&da, &fld.var(partner), m)?
                }
                Kind::Cos { arg, partner } => {
                    let Some(a) = rd.read(arg, m)? else { return Ok(None) };
                    let da = tw.d(&a, m)?;
                    fld.neg_of(&fld.mulm(&da, &fld.var(partner), m)?)
                }
                Kind::Prim { factor, arg, .. } => {
                    let Some(a) = rd.read(arg, m)? else { return Ok(None) };
                    let Some(g) = rd.read(factor, m)? else { return Ok(None) };
                    let da = tw.d(&a, m)?;
                    fld.mulm(&da, &g, m)?
                }
            };
            tw.deriv.push(d);
        }
        for _ in 0..SPARE {
            tw.deriv.push(fld.int(0));
        }
        Ok(Some(tw))
    }
}

impl Fld {
    /// `-a`.
    pub fn neg_of(&self, a: &RatFun) -> RatFun {
        RatFun { num: a.num.neg(), den: a.den.clone() }
    }
}

/// The hyperbolic function `name` of `u` in terms of `p = exp(u)`, `m = exp(-u)`.
fn hyper<'h>(ar: &Arith<'h, Heap>, name: &str, p: T<'h>, m: T<'h>, half: T<'h>) -> T<'h> {
    let inv = |a: T<'h>| ar.pow(a, ar.int(-1));
    let two = ar.int(2);
    match name {
        "sinh" => ar.mul(&[half, ar.sub(p, m)]),
        "cosh" => ar.mul(&[half, ar.add(&[p, m])]),
        "tanh" => ar.mul(&[ar.sub(p, m), inv(ar.add(&[p, m]))]),
        "coth" => ar.mul(&[ar.add(&[p, m]), inv(ar.sub(p, m))]),
        "sech" => ar.mul(&[two, inv(ar.add(&[p, m]))]),
        _ => ar.mul(&[two, inv(ar.sub(p, m))]),
    }
}

/// The trigonometric function `name` of `u` in terms of `p = exp(i u)`, `m = exp(-i u)`.
fn trig_exp<'h>(ar: &Arith<'h, Heap>, name: &str, p: T<'h>, m: T<'h>, i: T<'h>) -> T<'h> {
    let inv = |a: T<'h>| ar.pow(a, ar.int(-1));
    let two = ar.int(2);
    let s = ar.mul(&[ar.sub(p, m), inv(ar.mul(&[two, i]))]);
    let c = ar.mul(&[ar.add(&[p, m]), inv(two)]);
    match name {
        "sin" => s,
        "cos" => c,
        "tan" => ar.mul(&[s, inv(c)]),
        "cot" => ar.mul(&[c, inv(s)]),
        "sec" => inv(c),
        _ => inv(s),
    }
}

fn rat_int(r: &Rat) -> Option<i64> {
    let (p, q) = rat_pq(r)?;
    (q == 1).then_some(p)
}

/// Numerator and denominator of a rational with machine-sized parts.
pub fn rat_pq(r: &Rat) -> Option<(i64, i64)> {
    let (n, d) = mc_poly::qpoly::rat_parts(r);
    Some((n.as_i64()?, d.as_i64()?))
}

/// Reads rewritten terms (sums, products, integer powers, rationals and the variables' symbols) as field elements.
pub struct Reader<'r, 'h> {
    pub(crate) h: &'h Heap,
    pub(crate) ar: &'r Arith<'h, Heap>,
    pub(crate) fld: Fld,
    pub(crate) index: &'r FxHashMap<T<'h>, usize>,
}

impl<'h> Reader<'_, 'h> {
    /// The element of `t`; `None` when a subterm is not a variable, or a division by zero.
    pub fn read(&self, t: T<'h>, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        let mut memo: FxHashMap<T<'h>, RatFun> = FxHashMap::default();
        self.go(t, &mut memo, m)
    }

    fn go(&self, t: T<'h>, memo: &mut FxHashMap<T<'h>, RatFun>, m: &mut Meter) -> WorkResult<Option<RatFun>> {
        if let Some(r) = memo.get(&t) {
            return Ok(Some(r.clone()));
        }
        m.step(1)?;
        let fld = self.fld;
        let r = if let Some(&v) = self.index.get(&t) {
            fld.var(v)
        } else if let Some(q) = rat_of(self.ar, t) {
            let (n, d) = mc_poly::qpoly::rat_parts(&q);
            RatFun::constant(fld.nv, n, d)
        } else {
            let Some((name, args)) = fun(self.h, t) else { return Ok(None) };
            match (name.as_str(), args) {
                ("add", _) => {
                    let mut acc = fld.int(0);
                    for &a in args {
                        let Some(r) = self.go(a, memo, m)? else { return Ok(None) };
                        acc = fld.addm(&acc, &r, m)?;
                    }
                    acc
                }
                ("mul", _) => {
                    let mut acc = fld.int(1);
                    for &a in args {
                        let Some(r) = self.go(a, memo, m)? else { return Ok(None) };
                        acc = fld.mulm(&acc, &r, m)?;
                    }
                    acc
                }
                ("pow", [b, e]) => {
                    let Some(k) = int_of(self.h, *e) else { return Ok(None) };
                    let Some(rb) = self.go(*b, memo, m)? else { return Ok(None) };
                    match fld.powm(&rb, k, m)? {
                        Some(r) => r,
                        None => return Ok(None),
                    }
                }
                _ => return Ok(None),
            }
        };
        memo.insert(t, r.clone());
        Ok(Some(r))
    }
}

impl<'h> Tower<'h> {
    /// The total derivation `D a = sum over v of (da/dv) D v`.
    pub fn d(&self, a: &RatFun, m: &mut Meter) -> WorkResult<RatFun> {
        let fld = self.fld;
        let mut acc = fld.int(0);
        for v in 0..self.deriv.len() {
            if fld.free_of(a, v) || fld.is_zero_el(&self.deriv[v]) {
                continue;
            }
            let p = fld.partial(a, v, m)?;
            let t = fld.mulm(&p, &self.deriv[v], m)?;
            acc = fld.addm(&acc, &t, m)?;
        }
        Ok(acc)
    }

    /// The monomial variables (the integration variable and the tower's monomials).
    pub fn monomials(&self) -> Vec<usize> {
        (0..self.vars.len()).filter(|&v| is_monomial(&self.vars[v].kind)).collect()
    }

    /// The constant variables (spares included).
    pub fn constants(&self) -> Vec<usize> {
        (0..self.fld.nv).filter(|&v| v >= self.vars.len() || !is_monomial(&self.vars[v].kind)).collect()
    }
}

impl Fld {
    /// Is `a` zero?
    pub fn is_zero_el(&self, a: &RatFun) -> bool {
        a.num.is_zero()
    }
}

/// `Poly` helpers.
pub fn poly_one(nv: usize) -> Poly {
    Poly::constant(nv, Z::ONE)
}

/// A tower for `f` in `x`: normalized, rewritten, read; `None` when `f` is outside the tower. `extra` registers
/// candidates on the builder before the tower is fixed.
pub fn build<'h>(
    ar: &Arith<'h, Heap>,
    f: T<'h>,
    x: T<'h>,
    mode: Mode,
    extra: &mut dyn FnMut(&mut Builder<'_, 'h>),
    m: &mut Meter,
) -> WorkResult<Option<Tower<'h>>> {
    let mut b = Builder::new(ar, x, mode);
    let nf = b.normalize(f);
    b.collect_exps(nf);
    let Some(rf) = b.rw(nf) else { return Ok(None) };
    b.rf = Some(rf);
    extra(&mut b);
    b.finish(rf, m)
}

/// Is `t` a polynomial in `x` of degree `deg` with coefficients free of x? Its coefficients, constant first.
pub fn poly_in<'h>(ar: &Arith<'h, Heap>, t: T<'h>, x: T<'h>, deg: usize) -> Option<Vec<T<'h>>> {
    let c = coeffs_in(ar, t, x)?;
    (c.len() == deg + 1).then_some(c)
}
