//! Real sentences with functions that are not polynomial, decided through polynomial relaxations. Every maximal
//! non-polynomial subterm `A` (an atom: `sin(x - cos(x))`, `exp(x)`, `log(u)`, `u^r` with `r` not an integer,
//! `bessel_j(n, u)`, a constant such as `pi`) is a function of the variables it mentions; under the innermost
//! quantifier `Q x` that binds one of them, `Q x: phi(A)` is exactly `Q x: exists a: a == A and phi(a)` and exactly
//! `Q x: for_all a: a == A implies phi(a)`. Replacing `a == A` by a semi-algebraic condition `R(a)` that every
//! value of `A` satisfies (a range: `-1 <= a <= 1` for `sin`, `a >= 1 + u` for `exp(u)`, `a <= u - 1` for `log(u)`,
//! an interval enclosure of `A` over the bounds the formula puts on its variables) weakens the first form and
//! strengthens the second. Taking the first at positive positions and the second under a negation gives a weaker
//! polynomial sentence: when the decomposition finds it false, the sentence is false. The opposite choice gives a
//! stronger one: when it is true, the sentence is true. Nothing else is ever answered. A first try in each
//! direction also replaces each quantifier whose instances weaken (strengthen) the sentence by its instances at
//! `-1`, `0` and `1` (`for_all y: phi` implies `phi(0)`); that try applies to polynomial sentences too (the
//! function form of a decomposition, which stands for its formula).
//!
//! Over the reals a variable ranges where the functions of the formula are real (the convention of `reduce` with
//! `sqrt(x) < 2`): an atom's real domain `D` (`u > 0` for `log(u)`, `u >= 0` for `u^r`) restricts the quantifier that
//! binds it, `exists x: D and ...`, `for_all x: D implies ...`.
//!
//! The ranges used are classical bounds (Abramowitz and Stegun 4.1.33 and 4.2.30 for `log` and `exp`; DLMF 10.14.1,
//! `|J_n(x)| <= 1` for integer `n` and real `x`) and interval arithmetic for the enclosures.
use crate::front::Front;
use crate::cad::Limits;
use crate::natives::metered;
use mc_poly::work::{Meter, Work, WorkResult};
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// The most atoms a relaxation introduces.
const MAX_ATOMS: usize = 6;
/// The cells a relaxed sentence's decomposition may build.
const RELAXED_CELLS: usize = 20_000;
/// The work (units of the evaluator's fuel) it may take.
const RELAXED_FUEL: u64 = 30_000;

/// A variable with its lower and upper numeric bounds.
type VarBounds<'h> = (T<'h>, Option<f64>, Option<f64>);

/// One atom: its term, the fresh variable that stands for it, and its range and domain conditions.
struct Atom<'h> {
    term: T<'h>,
    var: T<'h>,
    range: Vec<T<'h>>,
    domain: Vec<T<'h>>,
}

/// A quantifier being rewritten: its variables, its numeric bounds, the atoms it binds and the polarity of its
/// position.
struct Scope<'h> {
    vars: Vec<T<'h>>,
    bounds: Vec<VarBounds<'h>>,
    atoms: Vec<Atom<'h>>,
    negative: bool,
}

struct Relaxer<'a, 'h> {
    fr: &'a Front<'h>,
    /// true: build the weaker sentence (proves false); false: the stronger one (proves true)
    weaken: bool,
    /// replace the quantifiers that may be by their instances at -1, 0, 1
    instantiate: bool,
    scopes: Vec<Scope<'h>>,
    /// atoms without variables (constants), bound outside everything
    constants: Vec<Atom<'h>>,
    count: usize,
}

/// `resolve(f, reals)` of a sentence `f` with non-polynomial functions: `Some(b)` when a relaxation proves it.
pub fn decide<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, f: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    let fr = Front::new(h);
    for (weaken, instantiate) in [(true, true), (false, true), (true, false), (false, false)] {
        let mut r = Relaxer { fr: &fr, weaken, instantiate, scopes: vec![], constants: vec![], count: 0 };
        let Some(g) = r.formula(cx, f, false)? else { continue };
        if r.count == 0 && !instantiate {
            // nothing to relax: not this method's sentence
            return Ok(None);
        }
        // the constants, bound outside everything (positive position)
        let consts = std::mem::take(&mut r.constants);
        let g = wrap(cx, consts, g, weaken, true);
        if let Some(b) = sentence_within(cx, stop, g)? {
            if weaken && !b {
                return Ok(Some(false));
            }
            if !weaken && b {
                return Ok(Some(true));
            }
        }
    }
    Ok(None)
}

/// The truth of a real polynomial sentence by the decomposition, all of its work (preprocessing included) within
/// [`RELAXED_FUEL`]; `None` past it or outside the method.
fn sentence_within<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, g: T<'h>) -> Res<Option<bool>> {
    let h = cx.heap();
    let r = metered(cx, stop, |m| -> WorkResult<Option<bool>> {
        let mut capped = Capped { inner: m.work(), left: RELAXED_FUEL, over: false };
        let r = {
            let mut mm = Meter::new(&mut capped);
            (|| -> WorkResult<Option<bool>> {
                let fr = Front::new(h);
                let Ok(mut p) = fr.problem(g, &[], &mut mm)? else { return Ok(None) };
                if p.prob.nfree != 0 {
                    return Ok(None);
                }
                if let Some(q) = crate::vs::eliminate(&p.prob, &mut mm)? {
                    p.prob = q;
                }
                let lim = Limits { max_cells: RELAXED_CELLS, ..Limits::default() };
                let Ok(o) = crate::cad::decompose(&p.prob, &lim, &mut |_, _| {}, &mut mm)? else { return Ok(None) };
                Ok(o.truth)
            })()
        };
        match r {
            Err(_) if capped.over => Ok(None),
            r => r,
        }
    })?;
    Ok(r.flatten())
}

/// A meter's work with a cap.
struct Capped<'a> {
    inner: &'a mut dyn Work,
    left: u64,
    over: bool,
}

impl Work for Capped<'_> {
    fn charge(&mut self, units: u64) -> WorkResult<()> {
        if units > self.left {
            self.over = true;
            return Err(mc_poly::work::Halt::Stopped);
        }
        self.left -= units;
        self.inner.charge(units)
    }
}

/// `body` under the atoms' quantifier: `exists a: R and D and body` when the relaxation weakens at this position,
/// `for_all a: R and D implies body` when it strengthens. The domains are conditions of the binding quantifier,
/// whose kind `exists_binder` decides how they join (`D and ...` or `D implies ...`).
fn wrap<'h>(cx: &mut NativeCx<'_, '_, 'h>, atoms: Vec<Atom<'h>>, body: T<'h>, exists_here: bool, exists_binder: bool) -> T<'h> {
    let h = cx.heap();
    if atoms.is_empty() {
        return body;
    }
    let vars: Vec<T<'h>> = atoms.iter().map(|a| a.var).collect();
    let range: Vec<T<'h>> = atoms.iter().flat_map(|a| a.range.iter().copied()).collect();
    let domain: Vec<T<'h>> = atoms.iter().flat_map(|a| a.domain.iter().copied()).collect();
    let and = |v: Vec<T<'h>>| match v.len() {
        0 => h.sym_term(h.sym(names::TRUE)),
        1 => v[0],
        _ => h.call(h.sym("and"), &v),
    };
    let vl = cx.list(&vars);
    let inner = if exists_here {
        h.call(h.sym("exists"), &[vl, h.call(h.sym("and"), &[and(range), body])])
    } else {
        h.call(h.sym("for_all"), &[vl, h.call(h.sym("implies"), &[and(range), body])])
    };
    if domain.is_empty() {
        inner
    } else if exists_binder {
        h.call(h.sym("and"), &[and(domain), inner])
    } else {
        h.call(h.sym("implies"), &[and(domain), inner])
    }
}

impl<'a, 'h> Relaxer<'a, 'h> {
    /// The formula `t` (at a negative position when `neg`) with its atoms replaced; `None` outside the method.
    fn formula(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, neg: bool) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        if let Some(n) = self.fr.sym_name(t) {
            return Ok(matches!(n.as_str(), names::TRUE | names::FALSE).then_some(t));
        }
        let Some((name, args)) = self.fr.head(t) else { return Ok(None) };
        let args = args.to_vec();
        match (name.as_str(), args.as_slice()) {
            ("and" | "or", _) => {
                let mut out = vec![];
                for &a in &args {
                    let Some(g) = self.formula(cx, a, neg)? else { return Ok(None) };
                    out.push(g);
                }
                Ok(Some(h.call(h.sym(&name), &out)))
            }
            ("not", [a]) => Ok(self.formula(cx, *a, !neg)?.map(|g| h.call(h.sym("not"), &[g]))),
            ("implies", [a, b]) => {
                let Some(ga) = self.formula(cx, *a, !neg)? else { return Ok(None) };
                let Some(gb) = self.formula(cx, *b, neg)? else { return Ok(None) };
                Ok(Some(h.call(h.sym("implies"), &[ga, gb])))
            }
            ("exists" | "for_all", [v, ..]) if args.len() == 2 || args.len() == 3 => {
                let Some(vars) = self.fr.qvars(*v) else { return Ok(None) };
                let exists = name == "exists";
                // exists(x, c, b) is exists(x, c and b); for_all(x, c, b) is for_all(x, c implies b)
                let body = match args.as_slice() {
                    [_, c, b] => h.call(h.sym(if exists { "and" } else { "implies" }), &[*c, *b]),
                    _ => args[1],
                };
                // the first try: a quantifier that may be weakened (strengthened) by the choice of the relaxation
                // is replaced by its instances at -1, 0 and 1 (for_all x: phi implies phi(0)), which often decides
                // the sentence without a decomposition in that variable
                if self.instantiate && (exists != neg) != self.weaken && vars.len() <= 2 {
                    let mut insts = vec![body];
                    for &x in &vars {
                        let mut next = vec![];
                        for &b in &insts {
                            for k in [-1, 0, 1] {
                                let rule = h.call(h.sym(names::RULE), &[x, h.int(k)]);
                                next.push(cx.eval(h.call(h.sym("rewrite"), &[b, rule]))?);
                            }
                        }
                        insts = next;
                    }
                    let mut out = vec![];
                    for b in insts {
                        let Some(g) = self.formula(cx, b, neg)? else { return Ok(None) };
                        out.push(g);
                    }
                    return Ok(Some(h.call(h.sym(if exists { "or" } else { "and" }), &out)));
                }
                let bounds = self.bounds(cx, &vars, body, exists)?;
                self.scopes.push(Scope { vars: vars.clone(), bounds, atoms: vec![], negative: neg });
                let g = self.formula(cx, body, neg)?;
                let scope = self.scopes.pop().expect("the scope pushed above");
                let Some(g) = g else { return Ok(None) };
                // weakening at a positive position (or strengthening at a negative one) binds the atom by exists
                let exists_here = self.weaken != scope.negative;
                let g = wrap(cx, scope.atoms, g, exists_here, exists);
                Ok(Some(h.call(h.sym(&name), &[cx.list(&vars), g])))
            }
            ("equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal", _) if args.len() >= 2 => {
                let mut out = vec![];
                for &a in &args {
                    let Some(g) = self.term(cx, a)? else { return Ok(None) };
                    out.push(g);
                }
                Ok(Some(h.call(h.sym(&name), &out)))
            }
            // the function form of a decomposition stands for its formula
            ("cylindrical_decomposition", [g, _, md]) if matches!(h.view(*md), View::Str("Function")) => self.formula(cx, *g, neg),
            ("element", [x, d]) if self.fr.sym_name(*d).as_deref() == Some("reals") => Ok(self.term(cx, *x)?.map(|g| h.call(h.sym("element"), &[g, *d]))),
            _ => Ok(None),
        }
    }

    /// The term `t` with its maximal non-polynomial subterms replaced by fresh variables.
    fn term(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        match h.view(t) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } => return Ok(Some(t)),
            View::Sym(_) if self.fr.qvars(t).is_some() => return Ok(Some(t)),
            View::App { head, args } => {
                let hn = self.fr.sym_name(head);
                let poly_node = match hn.as_deref() {
                    Some(names::ADD) | Some(names::MUL) => true,
                    Some(names::POW) => matches!(h.view(args[1]), View::Int(_)),
                    _ => false,
                };
                if poly_node {
                    let mut out = vec![];
                    for &a in args {
                        let Some(g) = self.term(cx, a)? else { return Ok(None) };
                        out.push(g);
                    }
                    return Ok(Some(h.app(head, &out)));
                }
            }
            _ => {}
        }
        self.atom(cx, t)
    }

    /// The fresh variable of the atom `t` (one per atom and binding quantifier), with its conditions.
    fn atom(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<T<'h>>> {
        let h = cx.heap();
        // the innermost scope that binds a variable of the atom; none for a constant
        let mut mentioned = vec![];
        for (i, s) in self.scopes.iter().enumerate() {
            if s.vars.iter().any(|&x| mentions(h, t, x)) {
                mentioned.push(i);
            }
        }
        let at = mentioned.last().copied();
        let known = match at {
            Some(i) => self.scopes[i].atoms.iter().find(|a| a.term == t).map(|a| a.var),
            None => self.constants.iter().find(|a| a.term == t).map(|a| a.var),
        };
        if let Some(v) = known {
            return Ok(Some(v));
        }
        // a symbol that no quantifier binds: a free variable, outside the method
        if at.is_none() && !self.free_of_symbols(cx, t) {
            return Ok(None);
        }
        if self.count >= MAX_ATOMS {
            return Ok(None);
        }
        self.count += 1;
        let var = h.sym_term(h.sym(&format!("relax${}", self.count)));
        let Some((range, domain)) = self.conditions(cx, t, var)? else { return Ok(None) };
        let a = Atom { term: t, var, range, domain };
        match at {
            Some(i) => self.scopes[i].atoms.push(a),
            None => self.constants.push(a),
        }
        Ok(Some(var))
    }

    /// Does `t` mention no symbol other than the named constants?
    fn free_of_symbols(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> bool {
        let h = cx.heap();
        let mut st = mc_api::walk::Walk::new(t);
        while let Some(u) = st.pop() {
            match h.view(u) {
                View::Sym(_) => {
                    if self.fr.qvars(u).is_some() {
                        return false;
                    }
                }
                View::App { args, .. } => st.extend(args.iter().copied()),
                _ => {}
            }
        }
        true
    }

    /// The range and domain conditions of the atom `t` for its variable `a`; `None` for a function whose real
    /// domain is not known here.
    fn conditions(&mut self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, a: T<'h>) -> Res<Option<(Vec<T<'h>>, Vec<T<'h>>)>> {
        let h = cx.heap();
        let rel = |n: &str, l: T<'h>, r: T<'h>| h.call(h.sym(n), &[l, r]);
        let zero = h.int(0);
        let one = h.int(1);
        let mut range = vec![];
        let mut domain = vec![];
        let constant = self.free_of_symbols(cx, t);
        if !constant {
            let Some((name, args)) = self.fr.head(t) else { return Ok(None) };
            match (name.as_str(), args) {
                ("sin" | "cos", [_]) => {
                    range.push(rel("less_equal", h.int(-1), a));
                    range.push(rel("less_equal", a, one));
                }
                ("tanh", [_]) => {
                    range.push(rel("less", h.int(-1), a));
                    range.push(rel("less", a, one));
                }
                ("atan", [_]) => {
                    // |atan| < pi/2 < 8/5
                    range.push(rel("less", ratio(h, -8, 5), a));
                    range.push(rel("less", a, ratio(h, 8, 5)));
                }
                ("cosh", [_]) => range.push(rel("greater_equal", a, one)),
                ("sinh", [_]) | ("asinh", [_]) => {}
                ("exp", [u]) => {
                    range.push(rel("greater", a, zero));
                    // exp(u) >= 1 + u
                    if let Some(p) = self.plain(cx, *u) {
                        range.push(rel("greater_equal", a, h.call(h.sym(names::ADD), &[one, p])));
                    }
                }
                ("log", [u]) => {
                    // log(u) <= u - 1 on u > 0
                    let Some(p) = self.plain(cx, *u) else { return Ok(None) };
                    domain.push(rel("greater", p, zero));
                    range.push(rel("less_equal", a, h.call(h.sym(names::ADD), &[p, h.int(-1)])));
                }
                ("bessel_j", [n, _]) if matches!(h.view(*n), View::Int(_)) => {
                    range.push(rel("less_equal", h.int(-1), a));
                    range.push(rel("less_equal", a, one));
                }
                (names::POW, [u, ex]) => {
                    let ru = number(cx, *ex)?;
                    let bu = number(cx, *u)?;
                    match (bu, ru) {
                        // c^v with c > 0: positive
                        (Some(c), _) if c > 0.0 && self.free_of_symbols(cx, *u) => {
                            range.push(rel("greater", a, zero));
                            // e^v >= 1 + v
                            if self.fr.sym_name(*u).as_deref() == Some("e") {
                                if let Some(p) = self.plain(cx, *ex) {
                                    range.push(rel("greater_equal", a, h.call(h.sym(names::ADD), &[one, p])));
                                }
                            }
                        }
                        // u^r with r a real constant that is not an integer: real for u >= 0 (u > 0 when r < 0)
                        (_, Some(rv)) if self.free_of_symbols(cx, *ex) && rv.fract() != 0.0 => {
                            let Some(p) = self.plain(cx, *u) else { return Ok(None) };
                            domain.push(rel(if rv > 0.0 { "greater_equal" } else { "greater" }, p, zero));
                            range.push(rel("greater_equal", a, zero));
                        }
                        _ => return Ok(None),
                    }
                }
                _ => return Ok(None),
            }
        }
        // an enclosure over the bounds of the variables (all of them bounded), or the value of a constant
        // (inside the box only: the range must hold at every point of the quantifier's domain)
        if let Some((lo, hi, inside)) = self.enclosure(cx, t, constant)? {
            let r = h.call(h.sym("and"), &[rel("less_equal", lo, a), rel("less_equal", a, hi)]);
            range.push(match inside {
                Some(b) => h.call(h.sym("implies"), &[b, r]),
                None => r,
            });
        } else if constant {
            return Ok(None);
        }
        Ok(Some((range, domain)))
    }

    /// `u` when it is a rational function of the bound variables (no atom inside), else `None`.
    fn plain(&self, cx: &mut NativeCx<'_, '_, 'h>, u: T<'h>) -> Option<T<'h>> {
        let h = cx.heap();
        fn ok<'h>(fr: &Front<'h>, h: &'h impl Store, u: T<'h>) -> bool {
            match h.view(u) {
                View::Int(_) | View::BigInt { .. } | View::Rational { .. } => true,
                View::Sym(_) => fr.qvars(u).is_some(),
                View::App { head, args } => match fr.sym_name(head).as_deref() {
                    Some(names::ADD) | Some(names::MUL) => args.iter().all(|&a| ok(fr, h, a)),
                    Some(names::POW) => matches!(h.view(args[1]), View::Int(_)) && ok(fr, h, args[0]),
                    _ => false,
                },
                _ => false,
            }
        }
        ok(self.fr, h, u).then_some(u)
    }

    /// Rational bounds `[lo, hi]` of `t` over the box of its variables (interval arithmetic), with the condition
    /// that the variables are in the box, or of the constant `t`; `None` when a variable is unbounded or the
    /// evaluation gives no finite interval.
    #[allow(clippy::type_complexity)]
    fn enclosure(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, constant: bool) -> Res<Option<(T<'h>, T<'h>, Option<T<'h>>)>> {
        let h = cx.heap();
        if constant {
            let Some(v) = number(cx, t)? else { return Ok(None) };
            let m = 1e-9 * (1.0 + v.abs());
            return Ok(rat_below(h, v - m).zip(rat_above(h, v + m)).map(|(l, r)| (l, r, None)));
        }
        let mut rules = vec![];
        let mut inside = vec![];
        for s in &self.scopes {
            for &(x, lo, hi) in &s.bounds {
                if !mentions(h, t, x) {
                    continue;
                }
                let (Some(lo), Some(hi)) = (lo, hi) else { return Ok(None) };
                let (Some(l), Some(r)) = (rat_below(h, lo), rat_above(h, hi)) else { return Ok(None) };
                let iv = h.call(h.sym("interval"), &[cx.list(&[l, r])]);
                rules.push(h.call(h.sym(names::RULE), &[x, iv]));
                inside.push(h.call(h.sym("less_equal"), &[l, x]));
                inside.push(h.call(h.sym("less_equal"), &[x, r]));
            }
        }
        // every variable of t bounded
        for s in &self.scopes {
            for &x in &s.vars {
                if mentions(h, t, x) && !s.bounds.iter().any(|b| b.0 == x && b.1.is_some() && b.2.is_some()) {
                    return Ok(None);
                }
            }
        }
        if rules.is_empty() {
            return Ok(None);
        }
        let e = cx.eval(h.call(h.sym("rewrite"), &[t, cx.list(&rules)]))?;
        let Some((n, args)) = self.fr.head(e) else { return Ok(None) };
        if n != "interval" || args.len() != 1 {
            return Ok(None);
        }
        let Some(&[l, r]) = cx.as_list(args[0]) else { return Ok(None) };
        let (Some(l), Some(r)) = (number(cx, l)?, number(cx, r)?) else { return Ok(None) };
        // a margin for the conversion to machine numbers
        let m = 1e-12 * (1.0 + l.abs().max(r.abs()));
        let b = h.call(h.sym("and"), &inside);
        Ok(rat_below(h, l - m).zip(rat_above(h, r + m)).map(|(l, r)| (l, r, Some(b))))
    }

    /// The numeric bounds of each of `vars` that the conjuncts of `body` (or its antecedent, for `for_all`) state.
    fn bounds(&self, cx: &mut NativeCx<'_, '_, 'h>, vars: &[T<'h>], body: T<'h>, exists: bool) -> Res<Vec<VarBounds<'h>>> {
        let mut conj = vec![];
        let src = match self.fr.head(body) {
            Some((n, [c, _])) if n == "implies" && !exists => *c,
            _ if exists => body,
            _ => return Ok(vars.iter().map(|&x| (x, None, None)).collect()),
        };
        flatten_and(self.fr, src, &mut conj);
        let mut out: Vec<VarBounds<'h>> = vars.iter().map(|&x| (x, None, None)).collect();
        for c in conj {
            let Some((rel, args)) = self.fr.head(c) else { continue };
            let up = match rel.as_str() {
                "less" | "less_equal" => true,
                "greater" | "greater_equal" => false,
                _ => continue,
            };
            // a chain a < b < c: each neighbouring pair
            for w in args.windows(2) {
                let (l, r) = (w[0], w[1]);
                for b in out.iter_mut() {
                    // l < r: an upper bound of l, a lower bound of r
                    if l == b.0 {
                        if let Some(v) = number(cx, r)? {
                            if up {
                                b.2 = Some(b.2.map_or(v, |u: f64| u.min(v)));
                            } else {
                                b.1 = Some(b.1.map_or(v, |u: f64| u.max(v)));
                            }
                        }
                    } else if r == b.0 {
                        if let Some(v) = number(cx, l)? {
                            if up {
                                b.1 = Some(b.1.map_or(v, |u: f64| u.max(v)));
                            } else {
                                b.2 = Some(b.2.map_or(v, |u: f64| u.min(v)));
                            }
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

fn flatten_and<'h>(fr: &Front<'h>, t: T<'h>, out: &mut Vec<T<'h>>) {
    match fr.head(t) {
        Some((n, args)) if n == "and" => args.iter().for_each(|&a| flatten_and(fr, a, out)),
        _ => out.push(t),
    }
}

fn mentions<'h>(h: &'h impl Store, t: T<'h>, x: T<'h>) -> bool {
    let mut st = mc_api::walk::Walk::new(t);
    while let Some(u) = st.pop() {
        if u == x {
            return true;
        }
        if let View::App { args, .. } = h.view(u) {
            st.extend(args.iter().copied());
        }
    }
    false
}

/// A real number term as a finite machine number.
fn number<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>) -> Res<Option<f64>> {
    let h = cx.heap();
    let v = cx.eval(h.call(h.sym("n"), &[t]))?;
    Ok(match h.view(v) {
        View::Real(x) if x.is_finite() => Some(x),
        View::Int(k) => Some(k as f64),
        _ => None,
    })
}

/// The term p/q.
fn ratio<'h>(h: &'h impl Store, p: i64, q: i64) -> T<'h> {
    h.call(h.sym(names::MUL), &[h.int(p), h.call(h.sym(names::POW), &[h.int(q), h.int(-1)])])
}

/// The scale of the rational bounds.
const SCALE: f64 = (1u64 << 30) as f64;

/// A dyadic rational at most `v`.
fn rat_below<'h>(h: &'h impl Store, v: f64) -> Option<T<'h>> {
    let k = (v * SCALE).floor();
    (k.abs() < 9.0e15).then(|| ratio(h, k as i64, 1 << 30))
}

/// A dyadic rational at least `v`.
fn rat_above<'h>(h: &'h impl Store, v: f64) -> Option<T<'h>> {
    let k = (v * SCALE).ceil();
    (k.abs() < 9.0e15).then(|| ratio(h, k as i64, 1 << 30))
}
