//! From terms to problems: formulas with `and`, `or`, `not`, `implies`, `equivalent`, `xor`, lists, comparisons
//! (chains included), `element(x, reals)`, `exists` and `for_all`; quantified Boolean variables are expanded; bound
//! variables are renamed apart and the formula is put in prenex form (negations pushed through quantifiers,
//! quantifiers pulled out of conjunctions and disjunctions); the atoms are read as rational functions with rational
//! coefficients in the variables, and anything else (a transcendental function, a constant such as `pi`) declines.
use crate::cad::Problem;
use crate::formula::{Fm, PAtom, Quant, Rel};
use mc_api::{names, Store, View, T};
use mc_arith::Arith;
use mc_core::Heap;
use mc_poly::convert::{Conv, Heads};
use mc_poly::ratfun;
use mc_poly::work::{Meter, WorkResult};
use mc_poly::Poly;
use std::cell::Cell as StdCell;

/// A formula with quantifiers over term atoms.
#[derive(Clone, Debug)]
pub enum QFm<'h> {
    /// true or false
    Const(bool),
    /// `d rel 0`
    Rel(Rel, T<'h>),
    /// a Boolean variable
    Bool(T<'h>),
    /// conjunction
    And(Vec<QFm<'h>>),
    /// disjunction
    Or(Vec<QFm<'h>>),
    /// negation
    Not(Box<QFm<'h>>),
    /// a quantifier over variables
    Q(Quant, Vec<T<'h>>, Box<QFm<'h>>),
}

/// A quantifier prefix (outermost first) and the quantifier-free matrix.
pub type Prenex<'h> = (Vec<(Quant, T<'h>)>, QFm<'h>);

/// Reading and rewriting formulas.
pub struct Front<'h> {
    /// the canonicaliser
    pub ar: Arith<'h, Heap>,
    /// the heads
    pub hd: Heads<'h>,
    fresh: StdCell<u32>,
}

/// Names of numeric constants: a formula with them is not polynomial with rational coefficients.
const CONSTANTS: &[&str] = &["pi", "e", "im", "euler_gamma", "catalan", "golden_ratio", "degree", "glaisher", "khinchin", "inf"];

/// The most `abs`, `max` and `min` split into cases in one formula.
const MAX_SPLITS: usize = 8;
/// The most cases from `abs`, `max` and `min` in one formula.
const MAX_CASES: usize = 64;

/// The problem of a formula and the variables of its decomposition order.
pub struct Parsed<'h> {
    /// the problem
    pub prob: Problem,
    /// the variable of each index
    pub vars: Vec<T<'h>>,
    /// does the formula have quantifiers?
    pub quantified: bool,
}

impl<'h> Front<'h> {
    /// A reader over the heap.
    pub fn new(h: &'h Heap) -> Front<'h> {
        Front { ar: Arith::new(h), hd: Heads::new(h), fresh: StdCell::new(0) }
    }

    fn h(&self) -> &'h Heap {
        self.ar.store()
    }

    /// The symbol name of a symbol term.
    pub fn sym_name(&self, t: T<'h>) -> Option<String> {
        let h = self.h();
        match h.view(t) {
            View::Sym(s) => Some(h.sym_name(s)),
            _ => None,
        }
    }

    /// The head name and arguments of a call with a symbol head.
    pub fn head(&self, t: T<'h>) -> Option<(String, &'h [T<'h>])> {
        let h = self.h();
        match h.view(t) {
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => Some((h.sym_name(s), args)),
                _ => None,
            },
            _ => None,
        }
    }

    /// The items of a list term.
    pub fn list(&self, t: T<'h>) -> Option<&'h [T<'h>]> {
        match self.h().view(t) {
            View::App { head, args } if head == self.hd.list => Some(args),
            _ => None,
        }
    }

    /// The variables of a quantifier: a symbol or a list of symbols.
    pub fn qvars(&self, t: T<'h>) -> Option<Vec<T<'h>>> {
        let v: Vec<T<'h>> = match self.list(t) {
            Some(items) => items.to_vec(),
            None => vec![t],
        };
        v.iter().all(|&x| self.sym_name(x).is_some_and(|n| !CONSTANTS.contains(&n.as_str()))).then_some(v)
    }

    /// Does `t` contain a quantifier?
    pub fn has_quantifier(&self, t: T<'h>) -> bool {
        let h = self.h();
        let mut stack = vec![t];
        while let Some(u) = stack.pop() {
            if let View::App { head, args } = h.view(u) {
                if let View::Sym(s) = h.view(head) {
                    if matches!(h.sym_name(s).as_str(), "exists" | "for_all") {
                        return true;
                    }
                }
                stack.extend(args.iter().copied());
            }
        }
        false
    }

    /// The formula of a term; `Err` with a reason for what is not a real polynomial formula.
    pub fn parse(&self, t: T<'h>) -> Result<QFm<'h>, &'static str> {
        let h = self.h();
        if let Some(n) = self.sym_name(t) {
            return Ok(match n.as_str() {
                names::TRUE => QFm::Const(true),
                names::FALSE => QFm::Const(false),
                _ => QFm::Bool(t),
            });
        }
        if let Some(items) = self.list(t) {
            return Ok(QFm::And(items.iter().map(|&u| self.parse(u)).collect::<Result<_, _>>()?));
        }
        let Some((name, args)) = self.head(t) else { return Err("expected a formula") };
        let rel = match name.as_str() {
            "and" => return Ok(QFm::And(args.iter().map(|&u| self.parse(u)).collect::<Result<_, _>>()?)),
            "or" => return Ok(QFm::Or(args.iter().map(|&u| self.parse(u)).collect::<Result<_, _>>()?)),
            "not" => {
                let [u] = args else { return Err("not takes one argument") };
                return Ok(QFm::Not(Box::new(self.parse(*u)?)));
            }
            "implies" => {
                let [a, b] = args else { return Err("implies takes two arguments") };
                return Ok(QFm::Or(vec![QFm::Not(Box::new(self.parse(*a)?)), self.parse(*b)?]));
            }
            "equivalent" => {
                let fs: Vec<QFm<'h>> = args.iter().map(|&u| self.parse(u)).collect::<Result<_, _>>()?;
                let all = QFm::And(fs.clone());
                let none = QFm::And(fs.into_iter().map(|f| QFm::Not(Box::new(f))).collect());
                return Ok(QFm::Or(vec![all, none]));
            }
            "xor" => {
                let [a, b] = args else { return Err("xor of two formulas") };
                let (a, b) = (self.parse(*a)?, self.parse(*b)?);
                let na = QFm::Not(Box::new(a.clone()));
                let nb = QFm::Not(Box::new(b.clone()));
                return Ok(QFm::Or(vec![QFm::And(vec![a, nb]), QFm::And(vec![na, b])]));
            }
            "element" => {
                let [_, d] = args else { return Err("element of two arguments") };
                return if self.sym_name(*d).as_deref() == Some("reals") { Ok(QFm::Const(true)) } else { Err("a domain other than the reals") };
            }
            "exists" | "for_all" => {
                let q = if name == "exists" { Quant::Exists } else { Quant::ForAll };
                let (vs, body) = match args {
                    [v, b] => (*v, self.parse(*b)?),
                    [v, c, b] => {
                        let (c, b) = (self.parse(*c)?, self.parse(*b)?);
                        let body = if q == Quant::Exists { QFm::And(vec![c, b]) } else { QFm::Or(vec![QFm::Not(Box::new(c)), b]) };
                        (*v, body)
                    }
                    _ => return Err("a quantifier takes variables and a formula"),
                };
                let Some(vs) = self.qvars(vs) else { return Err("quantified variables must be symbols") };
                return Ok(QFm::Q(q, vs, Box::new(body)));
            }
            "less" => Rel::Lt,
            "less_equal" => Rel::Le,
            "greater" => Rel::Gt,
            "greater_equal" => Rel::Ge,
            "equal" => Rel::Eq,
            "unequal" => Rel::Ne,
            _ => return Err("expected equations and inequalities"),
        };
        if args.len() < 2 {
            return Err("a comparison takes two sides");
        }
        let mut out = vec![];
        for w in args.windows(2) {
            out.push(QFm::Rel(rel, self.ar.sub(w[0], w[1])));
        }
        let _ = h;
        Ok(if out.len() == 1 { out.pop().expect("one") } else { QFm::And(out) })
    }

    /// The exponent 1/2.
    fn half(&self) -> T<'h> {
        self.ar.div(self.ar.int(1), self.ar.int(2))
    }

    /// A fresh variable for a bound one.
    fn fresh_var(&self) -> T<'h> {
        let n = self.fresh.get() + 1;
        self.fresh.set(n);
        let h = self.h();
        h.sym_term(h.sym(&format!("cad$v{n}")))
    }

    /// Replace the variable `x` by `y` in a formula (terms rewritten by substitution).
    fn rename(&self, f: &QFm<'h>, x: T<'h>, y: T<'h>, m: &mut Meter) -> WorkResult<QFm<'h>> {
        Ok(match f {
            QFm::Const(b) => QFm::Const(*b),
            QFm::Rel(r, d) => QFm::Rel(*r, self.subst(*d, x, y, m)?),
            QFm::Bool(b) => QFm::Bool(if *b == x { y } else { *b }),
            QFm::And(v) => QFm::And(v.iter().map(|g| self.rename(g, x, y, m)).collect::<WorkResult<_>>()?),
            QFm::Or(v) => QFm::Or(v.iter().map(|g| self.rename(g, x, y, m)).collect::<WorkResult<_>>()?),
            QFm::Not(g) => QFm::Not(Box::new(self.rename(g, x, y, m)?)),
            QFm::Q(q, vs, g) => {
                if vs.contains(&x) {
                    f.clone()
                } else {
                    QFm::Q(*q, vs.clone(), Box::new(self.rename(g, x, y, m)?))
                }
            }
        })
    }

    /// `t` with the symbol `x` replaced by `y` (canonicalised again).
    pub fn subst(&self, t: T<'h>, x: T<'h>, y: T<'h>, m: &mut Meter) -> WorkResult<T<'h>> {
        let h = self.h();
        if t == x {
            return Ok(y);
        }
        match h.view(t) {
            View::App { head, args } => {
                m.step(1)?;
                let mut changed = false;
                let mut new = Vec::with_capacity(args.len());
                for &a in args {
                    let b = self.subst(a, x, y, m)?;
                    changed |= b != a;
                    new.push(b);
                }
                if !changed {
                    return Ok(t);
                }
                Ok(if head == self.hd.add {
                    self.ar.add(&new)
                } else if head == self.hd.mul {
                    self.ar.mul(&new)
                } else if head == self.hd.pow {
                    self.ar.pow(new[0], new[1])
                } else {
                    h.app(head, &new)
                })
            }
            _ => Ok(t),
        }
    }

    /// Bound variables renamed apart, Boolean quantified variables expanded, negations pushed inward; the
    /// quantifier prefix (in order) and the quantifier-free matrix.
    pub fn prenex(&self, f: &QFm<'h>, m: &mut Meter) -> WorkResult<Result<Prenex<'h>, &'static str>> {
        let mut prefix = vec![];
        let g = match self.pull(f, false, &mut prefix, m)? {
            Ok(g) => g,
            Err(e) => return Ok(Err(e)),
        };
        Ok(Ok((prefix, g)))
    }

    fn pull(&self, f: &QFm<'h>, neg: bool, prefix: &mut Vec<(Quant, T<'h>)>, m: &mut Meter) -> WorkResult<Result<QFm<'h>, &'static str>> {
        m.step(1)?;
        Ok(Ok(match f {
            QFm::Const(b) => QFm::Const(*b != neg),
            QFm::Rel(r, d) => QFm::Rel(if neg { r.negate() } else { *r }, *d),
            QFm::Bool(b) => {
                if neg {
                    QFm::Not(Box::new(QFm::Bool(*b)))
                } else {
                    QFm::Bool(*b)
                }
            }
            QFm::Not(g) => return self.pull(g, !neg, prefix, m),
            QFm::And(v) | QFm::Or(v) => {
                let mut out = vec![];
                for g in v {
                    match self.pull(g, neg, prefix, m)? {
                        Ok(x) => out.push(x),
                        Err(e) => return Ok(Err(e)),
                    }
                }
                let and = matches!(f, QFm::And(_)) != neg;
                if and {
                    QFm::And(out)
                } else {
                    QFm::Or(out)
                }
            }
            QFm::Q(q, vs, g) => {
                let q = match (q, neg) {
                    (Quant::Exists, false) | (Quant::ForAll, true) => Quant::Exists,
                    _ => Quant::ForAll,
                };
                let mut body = (**g).clone();
                for &x in vs {
                    if bool_only(self.h(), &body, x) {
                        // a Boolean variable: both values
                        let t = bool_subst(&body, x, true);
                        let e = bool_subst(&body, x, false);
                        body = if q == Quant::Exists { QFm::Or(vec![t, e]) } else { QFm::And(vec![t, e]) };
                        continue;
                    }
                    if !occurs(self.h(), &body, x) {
                        continue;
                    }
                    let y = self.fresh_var();
                    body = self.rename(&body, x, y, m)?;
                    prefix.push((q, y));
                }
                // the body under the same polarity (the quantifier already carries the negation)
                return self.pull(&body, neg, prefix, m);
            }
        }))
    }

    /// The formula with every `abs`, `max` and `min` split into cases, at most `depth` splits along one branch and
    /// `budget` branches in all.
    fn split_cases(&self, f: &QFm<'h>, depth: usize, budget: &mut usize, m: &mut Meter) -> WorkResult<Option<QFm<'h>>> {
        let Some((k, cases)) = self.piecewise(f) else { return Ok(Some(f.clone())) };
        if depth == 0 || *budget < cases.len() {
            return Ok(None);
        }
        *budget -= cases.len() - 1;
        let mut alts = vec![];
        for (c, v) in cases {
            let g = self.replace(f, k, v, m)?;
            let Some(g) = self.split_cases(&g, depth - 1, budget, m)? else { return Ok(None) };
            alts.push(QFm::And(vec![c, g]));
        }
        Ok(Some(QFm::Or(alts)))
    }

    /// The cases of a formula term with `abs`, `max` and `min`: formula terms whose disjunction is the formula,
    /// each without them (`None` beyond the bounds; the formula itself when it has none).
    pub fn term_cases(&self, t: T<'h>, m: &mut Meter) -> WorkResult<Option<Vec<T<'h>>>> {
        let mut out = vec![];
        let mut budget = MAX_CASES;
        if !self.term_split(t, MAX_SPLITS, &mut budget, &mut out, m)? {
            return Ok(None);
        }
        Ok(Some(out))
    }

    fn term_split(&self, t: T<'h>, depth: usize, budget: &mut usize, out: &mut Vec<T<'h>>, m: &mut Meter) -> WorkResult<bool> {
        let h = self.h();
        let Some(k) = self.innermost_piecewise(t) else {
            out.push(t);
            return Ok(true);
        };
        let Some((_, cases)) = self.piecewise(&QFm::Rel(Rel::Eq, k)) else { return Ok(false) };
        if depth == 0 || *budget < cases.len() {
            return Ok(false);
        }
        *budget -= cases.len() - 1;
        for (c, v) in cases {
            let QFm::Rel(r, d) = c else { return Ok(false) };
            let ct = h.call(h.sym(r.name()), &[d, self.ar.int(0)]);
            let body = self.subst(t, k, v, m)?;
            if !self.term_split(h.call(h.sym("and"), &[ct, body]), depth - 1, budget, out, m)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Replace every atom `f rel 0` with the radical `k = sqrt(B)` (`B >= 0` everywhere, by a sum-of-squares
    /// certificate) by sign conditions on `P`, `Q` and `P^2 - Q^2 B`, where `f = P + Q sqrt(B)` (Weispfenning's
    /// quadratic virtual substitution rules): with `D = P^2 - Q^2 B`, `f > 0` iff `D > 0 and P > 0` or
    /// `D < 0 and Q > 0` or `D == 0 and P > 0 and Q >= 0`; `f == 0` iff `D == 0 and P Q <= 0`. `false` (nothing
    /// changed) when the radicand is not certified nonnegative or a denominator has the radical.
    #[allow(clippy::too_many_arguments)]
    fn square_out(
        &self,
        k: T<'h>,
        base: T<'h>,
        ring: &mc_poly::convert::Ring<'h>,
        diffs: &mut Vec<T<'h>>,
        rels: &mut Vec<Rel>,
        fm: &mut Fm,
        m: &mut Meter,
    ) -> WorkResult<bool> {
        let c = Conv::rational(&self.ar, &self.hd);
        let Some(vk) = ring.var(k) else { return Ok(false) };
        // every kernel of the radicand must be a variable of the ring (an inexact number is not)
        let rb = c.ring(&[base], &[], m)?.0;
        if rb.kernels.iter().any(|&q| ring.var(q).is_none()) {
            return Ok(false);
        }
        let Ok(b) = ratfun::read(&c, base, ring, m)? else { return Ok(false) };
        let Some(bc) = b.den.as_constant() else { return Ok(false) };
        if b.num.degree(vk).unwrap_or(0) > 0 || bc.is_neg() || b.num.nvars() != ring.nvars() {
            return Ok(false);
        }
        if !crate::sos::nonnegative(&b.num, m)? {
            return Ok(false);
        }
        let bcp = Poly::constant(ring.nvars(), bc.clone());
        // the atoms with k: (index, P', Q', denominator)
        let mut parts = vec![];
        for (i, &d) in diffs.iter().enumerate() {
            if !contains(self.h(), d, k) {
                continue;
            }
            let Ok(f) = ratfun::read(&c, d, ring, m)? else { return Ok(false) };
            if f.den.degree(vk).unwrap_or(0) > 0 || f.num.nvars() != ring.nvars() {
                return Ok(false);
            }
            let cs = crate::poly::coeffs(&f.num, vk);
            let jmax = (cs.len().saturating_sub(1)) / 2;
            let (mut p, mut q) = (Poly::zero(ring.nvars()), Poly::zero(ring.nvars()));
            for (e, ce) in cs.iter().enumerate() {
                let j = e / 2;
                // k^e = (N_B / c)^j k^(e mod 2), scaled by c^jmax
                let w = ce.mul(&b.num.pow(j as u64, m)?, m)?.mul(&bcp.pow((jmax - j) as u64, m)?, m)?;
                if e % 2 == 0 {
                    p = p.add(&w);
                } else {
                    q = q.add(&w);
                }
            }
            parts.push((i, p, q, f.den));
        }
        fn atom<'h>(d: &mut Vec<T<'h>>, r: &mut Vec<Rel>, t: T<'h>, rel: Rel) -> Fm {
            d.push(t);
            r.push(rel);
            Fm::Atom(d.len() - 1)
        }
        // f > 0 (pos) or f < 0
        fn strict<'h>(d: &mut Vec<T<'h>>, r: &mut Vec<Rel>, (tp, tq, td): (T<'h>, T<'h>, T<'h>), pos: bool) -> Fm {
            let (s, w) = if pos { (Rel::Gt, Rel::Ge) } else { (Rel::Lt, Rel::Le) };
            Fm::Or(vec![
                Fm::And(vec![atom(d, r, td, Rel::Gt), atom(d, r, tp, s)]),
                Fm::And(vec![atom(d, r, td, Rel::Lt), atom(d, r, tq, s)]),
                Fm::And(vec![atom(d, r, td, Rel::Eq), atom(d, r, tp, s), atom(d, r, tq, w)]),
            ])
        }
        fn zero<'h>(d: &mut Vec<T<'h>>, r: &mut Vec<Rel>, (tp, tq, td): (T<'h>, T<'h>, T<'h>)) -> Fm {
            Fm::And(vec![
                atom(d, r, td, Rel::Eq),
                Fm::Or(vec![
                    Fm::And(vec![atom(d, r, tp, Rel::Le), atom(d, r, tq, Rel::Ge)]),
                    Fm::And(vec![atom(d, r, tp, Rel::Ge), atom(d, r, tq, Rel::Le)]),
                ]),
            ])
        }
        for (i, p, q, den) in parts {
            let tp = ratfun::to_term(&c, &ratfun::RatFun { num: p.clone(), den: den.clone() }, ring, m)?;
            let tq = ratfun::to_term(&c, &ratfun::RatFun { num: q.clone(), den: den.clone() }, ring, m)?;
            let dd = bcp.mul(&p.mul(&p, m)?, m)?.sub(&q.mul(&q, m)?.mul(&b.num, m)?);
            let td = c.int_poly_term(&dd, ring, m)?;
            let t = (tp, tq, td);
            let g = match rels[i] {
                Rel::Gt => strict(diffs, rels, t, true),
                Rel::Lt => strict(diffs, rels, t, false),
                Rel::Eq => zero(diffs, rels, t),
                Rel::Ge => Fm::Or(vec![strict(diffs, rels, t, true), zero(diffs, rels, t)]),
                Rel::Le => Fm::Or(vec![strict(diffs, rels, t, false), zero(diffs, rels, t)]),
                Rel::Ne => Fm::Or(vec![strict(diffs, rels, t, true), strict(diffs, rels, t, false)]),
            };
            *fm = replace_atom(fm, i, &g);
            // the old atom is unused
            diffs[i] = self.ar.int(0);
        }
        Ok(true)
    }

    /// An innermost `abs(u)`, `max(a, b, ...)` or `min(a, b, ...)` in an atom of a quantifier-free formula, with its
    /// cases: (condition, value) pairs covering the space.
    #[allow(clippy::type_complexity)]
    fn piecewise(&self, f: &QFm<'h>) -> Option<(T<'h>, Vec<(QFm<'h>, T<'h>)>)> {
        let k = match f {
            QFm::Rel(_, d) => self.innermost_piecewise(*d),
            QFm::And(v) | QFm::Or(v) => return v.iter().find_map(|g| self.piecewise(g)),
            QFm::Not(g) => return self.piecewise(g),
            _ => None,
        }?;
        let (name, args) = self.head(k)?;
        let ge = |a: T<'h>, b: T<'h>| QFm::Rel(Rel::Ge, self.ar.sub(a, b));
        let lt = |a: T<'h>, b: T<'h>| QFm::Rel(Rel::Lt, self.ar.sub(a, b));
        Some((
            k,
            match (name.as_str(), args) {
                ("abs", [u]) => vec![(ge(*u, self.ar.int(0)), *u), (lt(*u, self.ar.int(0)), self.ar.neg(*u))],
                // max(a, rest): a when a >= max(rest), else max(rest)
                ("max" | "min", [a, rest @ ..]) if !rest.is_empty() => {
                    let h = self.h();
                    let other = if rest.len() == 1 { rest[0] } else { h.call(h.sym(&name), rest) };
                    if name == "max" {
                        vec![(ge(*a, other), *a), (lt(*a, other), other)]
                    } else {
                        vec![(ge(other, *a), *a), (lt(other, *a), other)]
                    }
                }
                _ => return None,
            },
        ))
    }

    /// A subterm `abs(u)`, `max(..)`, `min(..)` of `t` with none inside its arguments.
    fn innermost_piecewise(&self, t: T<'h>) -> Option<T<'h>> {
        let h = self.h();
        let View::App { args, .. } = h.view(t) else { return None };
        if let Some(k) = args.iter().find_map(|&a| self.innermost_piecewise(a)) {
            return Some(k);
        }
        match self.head(t) {
            Some((n, a)) if (n == "abs" && a.len() == 1) || ((n == "max" || n == "min") && a.len() >= 2) => Some(t),
            _ => None,
        }
    }

    /// The formula with the subterm `k` replaced by `v` in every atom.
    fn replace(&self, f: &QFm<'h>, k: T<'h>, v: T<'h>, m: &mut Meter) -> WorkResult<QFm<'h>> {
        Ok(match f {
            QFm::Rel(r, d) => QFm::Rel(*r, self.subst(*d, k, v, m)?),
            QFm::And(w) => QFm::And(w.iter().map(|g| self.replace(g, k, v, m)).collect::<WorkResult<_>>()?),
            QFm::Or(w) => QFm::Or(w.iter().map(|g| self.replace(g, k, v, m)).collect::<WorkResult<_>>()?),
            QFm::Not(g) => QFm::Not(Box::new(self.replace(g, k, v, m)?)),
            g => g.clone(),
        })
    }

    /// The problem of `cond`: the free variables `free` (in order) after any other symbols (parameters), then the
    /// bound variables. `Err` with a reason when the formula is not a real polynomial formula.
    pub fn problem(&self, cond: T<'h>, free: &[T<'h>], m: &mut Meter) -> WorkResult<Result<Parsed<'h>, &'static str>> {
        let f = match self.parse(cond) {
            Ok(f) => f,
            Err(e) => return Ok(Err(e)),
        };
        let quantified = self.has_quantifier(cond);
        let (prefix, matrix) = match self.prenex(&f, m)? {
            Ok(x) => x,
            Err(e) => return Ok(Err(e)),
        };
        // abs, max and min: split into cases (decision CAD.17)
        let mut budget = MAX_CASES;
        let Some(matrix) = self.split_cases(&matrix, MAX_SPLITS, &mut budget, m)? else {
            return Ok(Err("too many piecewise functions"));
        };
        let mut diffs: Vec<T<'h>> = vec![];
        let mut rels: Vec<Rel> = vec![];
        let fm = match to_fm(&matrix, &mut diffs, &mut rels) {
            Ok(x) => x,
            Err(e) => return Ok(Err(e)),
        };
        let mut prefix = prefix;
        let mut fm = fm;
        let c = Conv::rational(&self.ar, &self.hd);
        let mut params: Vec<T<'h>> = vec![];
        // square roots become variables (decision CAD.12): sqrt(t) is r with r^2 == t and r >= 0, bound by the
        // quantifier of the innermost variable of t (exists for free variables)
        let mut rounds = 0;
        loop {
            rounds += 1;
            let mut extra: Vec<T<'h>> = params.clone();
            extra.extend(free.iter().copied());
            extra.extend(prefix.iter().map(|p| p.1));
            let ring = c.ring(&diffs, &extra, m)?.0;
            let mut radical = None;
            for &k in &ring.kernels {
                if extra.contains(&k) {
                    continue;
                }
                if let Some(n) = self.sym_name(k) {
                    if CONSTANTS.contains(&n.as_str()) {
                        return Ok(Err("not a polynomial formula with rational coefficients"));
                    }
                    params.push(k);
                    continue;
                }
                match self.head(k) {
                    Some((n, [base, e])) if n == names::POW && *e == self.half() => {
                        if radical.is_none() {
                            radical = Some((k, *base));
                        }
                    }
                    _ => return Ok(Err("not a polynomial formula with rational coefficients")),
                }
            }
            let Some((k, base)) = radical else { break };
            if rounds > 8 {
                return Ok(Err("too many radicals"));
            }
            // a sentence with a radical of a nonnegative radicand: squared out of its atoms (decision CAD.21)
            if free.is_empty() && params.is_empty() && self.square_out(k, base, &ring, &mut diffs, &mut rels, &mut fm, m)? {
                continue;
            }
            let r = self.fresh_var();
            for d in diffs.iter_mut() {
                *d = self.subst(*d, k, r, m)?;
            }
            let base = self.subst(base, k, r, m)?;
            // where r goes: after the innermost bound variable of the radicand, else first of the bound ones
            let pos = (0..prefix.len()).rev().find(|&i| contains(self.h(), base, prefix[i].1));
            let (at, q) = match pos {
                Some(i) => (i + 1, prefix[i].0),
                None => (0, Quant::Exists),
            };
            prefix.insert(at, (q, r));
            diffs.push(self.ar.sub(self.ar.pow(r, self.ar.int(2)), base));
            rels.push(Rel::Eq);
            let eq = Fm::Atom(diffs.len() - 1);
            diffs.push(r);
            rels.push(Rel::Ge);
            let ge = Fm::Atom(diffs.len() - 1);
            fm = match q {
                Quant::Exists => Fm::And(vec![eq, ge, fm]),
                Quant::ForAll => Fm::Or(vec![Fm::negation(Fm::And(vec![eq, ge])), fm]),
            };
        }
        let bound: Vec<T<'h>> = prefix.iter().map(|p| p.1).collect();
        let mut vars = params.clone();
        vars.extend(free.iter().copied());
        vars.extend(bound.iter().copied());
        let ring = c.ring(&[], &vars, m)?.0;
        let nv = vars.len();
        if ring.nvars() != nv {
            return Ok(Err("repeated variables"));
        }
        // the ring orders its kernels canonically: ring variable j is decomposition variable perm[j]
        let perm: Vec<usize> = ring.kernels.iter().map(|k| vars.iter().position(|v| v == k).expect("a variable")).collect();
        let mut atoms = vec![];
        for (i, &d) in diffs.iter().enumerate() {
            let rf = match ratfun::read(&c, d, &ring, m)? {
                Ok(x) => x,
                Err(_) => return Ok(Err("a condition divides by zero")),
            };
            if rf.num.nvars() != nv {
                return Ok(Err("not a polynomial formula with rational coefficients"));
            }
            atoms.push(PAtom { num: rf.num.permute(&perm), den: rf.den.permute(&perm), rel: rels[i] });
        }
        let nfree = params.len() + free.len();
        let quants = prefix.iter().map(|p| p.0).collect();
        Ok(Ok(Parsed { prob: Problem { nv, nfree, quants, atoms, matrix: fm }, vars, quantified }))
    }
}

fn occurs<'h>(h: &'h Heap, f: &QFm<'h>, x: T<'h>) -> bool {
    match f {
        QFm::Const(_) => false,
        QFm::Rel(_, d) => contains(h, *d, x),
        QFm::Bool(b) => *b == x,
        QFm::And(v) | QFm::Or(v) => v.iter().any(|g| occurs(h, g, x)),
        QFm::Not(g) => occurs(h, g, x),
        QFm::Q(_, vs, g) => !vs.contains(&x) && occurs(h, g, x),
    }
}

fn contains<'h>(h: &'h Heap, t: T<'h>, x: T<'h>) -> bool {
    let mut stack = vec![t];
    while let Some(u) = stack.pop() {
        if u == x {
            return true;
        }
        if let View::App { head, args } = h.view(u) {
            stack.push(head);
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// Does `x` occur only as a Boolean atom (and at least once)?
fn bool_only<'h>(h: &'h Heap, f: &QFm<'h>, x: T<'h>) -> bool {
    fn walk<'h>(h: &'h Heap, f: &QFm<'h>, x: T<'h>, seen: &mut bool) -> bool {
        match f {
            QFm::Const(_) => true,
            QFm::Rel(_, d) => !contains(h, *d, x),
            QFm::Bool(b) => {
                if *b == x {
                    *seen = true;
                }
                true
            }
            QFm::And(v) | QFm::Or(v) => v.iter().all(|g| walk(h, g, x, seen)),
            QFm::Not(g) => walk(h, g, x, seen),
            QFm::Q(_, vs, g) => vs.contains(&x) || walk(h, g, x, seen),
        }
    }
    let mut seen = false;
    walk(h, f, x, &mut seen) && seen
}

fn bool_subst<'h>(f: &QFm<'h>, x: T<'h>, v: bool) -> QFm<'h> {
    match f {
        QFm::Bool(b) if *b == x => QFm::Const(v),
        QFm::And(w) => QFm::And(w.iter().map(|g| bool_subst(g, x, v)).collect()),
        QFm::Or(w) => QFm::Or(w.iter().map(|g| bool_subst(g, x, v)).collect()),
        QFm::Not(g) => QFm::Not(Box::new(bool_subst(g, x, v))),
        QFm::Q(q, vs, g) if !vs.contains(&x) => QFm::Q(*q, vs.clone(), Box::new(bool_subst(g, x, v))),
        g => g.clone(),
    }
}

/// `f` with the atom `i` replaced by `g`.
fn replace_atom(f: &Fm, i: usize, g: &Fm) -> Fm {
    match f {
        Fm::Atom(j) if *j == i => g.clone(),
        Fm::And(v) => Fm::And(v.iter().map(|h| replace_atom(h, i, g)).collect()),
        Fm::Or(v) => Fm::Or(v.iter().map(|h| replace_atom(h, i, g)).collect()),
        Fm::Not(h) => Fm::Not(Box::new(replace_atom(h, i, g))),
        h => h.clone(),
    }
}

/// The matrix with term atoms numbered.
fn to_fm<'h>(f: &QFm<'h>, diffs: &mut Vec<T<'h>>, rels: &mut Vec<Rel>) -> Result<Fm, &'static str> {
    Ok(match f {
        QFm::Const(b) => Fm::Const(*b),
        QFm::Rel(r, d) => {
            diffs.push(*d);
            rels.push(*r);
            Fm::Atom(diffs.len() - 1)
        }
        QFm::Bool(_) => return Err("free Boolean variables"),
        QFm::And(v) => Fm::And(v.iter().map(|g| to_fm(g, diffs, rels)).collect::<Result<_, _>>()?),
        QFm::Or(v) => Fm::Or(v.iter().map(|g| to_fm(g, diffs, rels)).collect::<Result<_, _>>()?),
        QFm::Not(g) => Fm::negation(to_fm(g, diffs, rels)?),
        QFm::Q(..) => return Err("a quantifier inside the matrix"),
    })
}
