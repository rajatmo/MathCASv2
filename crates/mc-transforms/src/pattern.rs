//! The matcher of the transform tables.
//!
//! A table is source text in the language itself: one statement per entry, a list `[lhs, rhs]` or `[lhs, rhs,
//! cond]`. In an entry `t` is the variable of the transform and `s` the variable of the result; the one-letter
//! names `a b c d k m n p q r v` are parameters (they match any term free of `t`); other names are themselves.
//! The left side is evaluated (canonical form) and matched structurally, with three liberties that make one entry
//! cover the usual variants:
//!
//! - a product matches the factors with `t` one to one (in any order) and binds a lone parameter factor to the
//!   quotient of the rest (`a*t` matches `t`, `-t`, `3 b t`);
//! - a sum matches its terms with `t` one to one and binds a lone parameter term to the difference of the rest;
//! - a power with a parameter exponent matches its base alone with exponent 1 (`t^n` matches `t`).
//!
//! The right side and the condition are instantiated with the bindings and evaluated; an entry applies when its
//! condition evaluates to `true`.
use mc_calculus::common::{args_of, is_free};
use mc_calculus::term::Build;
use mc_api::{Store, View, T};
use mc_eval::{NativeCx, Res};

const PARAMS: &[&str] = &["a", "b", "c", "d", "k", "m", "n", "p", "q", "r", "v"];

/// One entry: left side (evaluated), right side and condition (unevaluated templates).
struct Entry<'h> {
    lhs: T<'h>,
    rhs: T<'h>,
    cond: Option<T<'h>>,
    /// the region of convergence of the entry's value (a fourth item of the entry), instantiated like the value
    strip: Option<T<'h>>,
}

/// A table read for one transform call.
pub(crate) struct Table<'h> {
    entries: Vec<Entry<'h>>,
    params: Vec<T<'h>>,
    x: T<'h>,
}

type Binds<'h> = Vec<(T<'h>, T<'h>)>;

impl<'h> Table<'h> {
    /// The table of `src` with `t` as `x` and `s` as `y`; an unreadable table (a defect the tests catch) is empty.
    pub(crate) fn read(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, src: &str, x: T<'h>, y: T<'h>) -> Res<Table<'h>> {
        Table::read_named(cx, b, src, ("t", x), ("s", y))
    }

    /// [`Table::read`] with other names for the two variables (`("s", s), ("t", t)` for an inverse table).
    pub(crate) fn read_named(cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, src: &str, xv: (&str, T<'h>), yv: (&str, T<'h>)) -> Res<Table<'h>> {
        let (x, y) = (xv.1, yv.1);
        let h = b.heap();
        let params: Vec<T<'h>> = PARAMS.iter().map(|p| h.sym_term(h.sym(&format!("transform·{p}")))).collect();
        let mut tb = Table { entries: vec![], params, x };
        let Ok(stmts) = mc_syntax::read(h, src) else { return Ok(tb) };
        for st in stmts {
            let st = rename(b, st, (xv.0, x), (yv.0, y), &tb.params);
            let Some(items) = args_of(b, st, b.hd.list) else { continue };
            let (lhs, rhs, cond, strip) = match items {
                [l, r] => (*l, *r, None, None),
                [l, r, c] => (*l, *r, Some(*c), None),
                [l, r, c, s] => (*l, *r, Some(*c), Some(*s)),
                _ => continue,
            };
            let lhs = cx.eval(lhs)?;
            tb.entries.push(Entry { lhs, rhs, cond, strip });
        }
        Ok(tb)
    }

    /// The right side of the first entry that matches `f` (and whose condition holds), evaluated.
    pub(crate) fn lookup(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<Option<T<'h>>> {
        Ok(self.lookup_strip(cx, b, f)?.map(|(r, _)| r))
    }

    /// [`Table::lookup`] with the entry's region of convergence (evaluated) when it has one.
    pub(crate) fn lookup_strip(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, f: T<'h>) -> Res<Option<(T<'h>, Option<T<'h>>)>> {
        for e in &self.entries {
            cx.tick(1)?;

            let mut binds = vec![];
            if !self.mat(cx, b, e.lhs, f, &mut binds)? {
                continue;
            }

            if let Some(c) = e.cond {
                let Some(c) = self.inst(b, c, &binds) else { continue };
                // `possibly(c)`: the entry applies unless `c` is decided false (a parameter with no value yet)
                let (c, lenient) = match args_of(b, c, b.heap().sym("possibly")) {
                    Some(&[inner]) => (inner, true),
                    _ => (c, false),
                };
                let c = cx.eval(c)?;
                if lenient {
                    if cx.is_false(c) {
                        continue;
                    }
                } else if !cx.is_true(c) {
                    continue;
                }
            }
            let Some(r) = self.inst(b, e.rhs, &binds) else { continue };
            let r = cx.eval(r)?;
            if mc_calculus::common::is_failure(b, r) {
                continue;
            }
            let strip = match e.strip.and_then(|s| self.inst(b, s, &binds)) {
                Some(s) => Some(cx.eval(s)?),
                None => None,
            };
            return Ok(Some((r, strip)));
        }
        Ok(None)
    }

    fn is_param(&self, t: T<'h>) -> bool {
        self.params.contains(&t)
    }

    /// Does `t` mention a parameter?
    fn has_param(&self, b: &Build<'h>, t: T<'h>) -> bool {
        self.params.iter().any(|&p| !is_free(b, t, p))
    }

    fn bind(&self, b: &Build<'h>, p: T<'h>, t: T<'h>, binds: &mut Binds<'h>) -> bool {
        if !is_free(b, t, self.x) {
            return false;
        }
        match binds.iter().find(|(q, _)| *q == p) {
            Some(&(_, v)) => v == t,
            None => {
                binds.push((p, t));
                true
            }
        }
    }

    fn mat(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, p: T<'h>, t: T<'h>, binds: &mut Binds<'h>) -> Res<bool> {
        let h = b.heap();
        if self.is_param(p) {
            return Ok(self.bind(b, p, t, binds));
        }
        if !self.has_param(b, p) {
            return Ok(p == t);
        }
        // a part without t: all of its parameters bound, or a lone parameter (times a number)
        if is_free(b, p, self.x) {
            if let Some(v) = self.inst(b, p, binds) {
                if !self.has_param(b, v) {
                    let v = cx.eval(v)?;
                    return Ok(v == t);
                }
            }
        }
        let View::App { head, args: pargs } = h.view(p) else { return Ok(false) };
        let ps = b.sym_of(head);
        if ps == Some(b.hd.mul) || ps == Some(b.hd.add) {
            let mul = ps == Some(b.hd.mul);
            let targets = match args_of(b, t, if mul { b.hd.mul } else { b.hd.add }) {
                Some(a) => a.to_vec(),
                None => vec![t],
            };
            let (rest_p, with_x): (Vec<T<'h>>, Vec<T<'h>>) = pargs.iter().partition(|&&q| is_free(b, q, self.x));
            return self.assign(cx, b, &with_x, &targets, &rest_p, mul, binds);
        }
        if ps == Some(b.hd.pow) {
            if let [pb, pe] = pargs {
                if let Some(&[tb, te]) = args_of(b, t, b.hd.pow) {
                    let mut trial = binds.clone();
                    if self.mat(cx, b, *pb, tb, &mut trial)? && self.mat(cx, b, *pe, te, &mut trial)? {
                        *binds = trial;
                        return Ok(true);
                    }
                }
                if self.is_param(*pe) && !binds.iter().any(|(q, _)| q == pe) {
                    let mut trial = binds.clone();
                    if self.mat(cx, b, *pb, t, &mut trial)? {
                        trial.push((*pe, b.one()));
                        *binds = trial;
                        return Ok(true);
                    }
                }
                return Ok(false);
            }
        }
        // a call: the same head, the arguments in order
        let View::App { head: th, args: targs } = h.view(t) else { return Ok(false) };
        if th != head || targs.len() != pargs.len() {
            return Ok(false);
        }
        let mut trial = binds.clone();
        for (&q, &u) in pargs.iter().zip(targs) {
            if !self.mat(cx, b, q, u, &mut trial)? {
                return Ok(false);
            }
        }
        *binds = trial;
        Ok(true)
    }

    /// The factors (terms) of a pattern with `t` matched one to one with distinct targets, by backtracking; the
    /// targets left over must be free of `t` and match the rest of the pattern (`rest_p`) as one product (sum).
    #[allow(clippy::too_many_arguments)]
    fn assign(
        &self,
        cx: &mut NativeCx<'_, '_, 'h>,
        b: &Build<'h>,
        with_x: &[T<'h>],
        targets: &[T<'h>],
        rest_p: &[T<'h>],
        mul: bool,
        binds: &mut Binds<'h>,
    ) -> Res<bool> {
        let Some((&first, more)) = with_x.split_first() else {
            if targets.iter().any(|&u| !is_free(b, u, self.x)) {
                return Ok(false);
            }
            let left = if mul { b.mul(targets) } else { b.add(targets) };
            let left = cx.eval(left)?;
            return self.rest(cx, b, rest_p, left, mul, binds);
        };
        for i in 0..targets.len() {
            let mut trial = binds.clone();
            if self.mat(cx, b, first, targets[i], &mut trial)? {
                let mut others = targets.to_vec();
                others.remove(i);
                if self.assign(cx, b, more, &others, rest_p, mul, &mut trial)? {
                    *binds = trial;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    /// The part of a product (sum) pattern free of `t` against the product (sum) `left` of the other targets.
    fn rest(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &Build<'h>, rest_p: &[T<'h>], left: T<'h>, mul: bool, binds: &mut Binds<'h>) -> Res<bool> {
        let unit = if mul { b.one() } else { b.zero() };
        if rest_p.is_empty() {
            return Ok(left == unit);
        }
        // the known part (numbers, bound parameters) and at most one unbound parameter
        let mut known = vec![];
        let mut free_param = None;
        for &q in rest_p {
            match self.inst(b, q, binds) {
                Some(v) if !self.has_param(b, v) => known.push(v),
                _ if self.is_param(q) && free_param.is_none() => free_param = Some((q, b.one())),
                // a number times a parameter in a sum (`n/2` in `gamma(n/2 + s/2)`)
                _ if !mul && free_param.is_none() => match args_of(b, q, b.hd.mul) {
                    Some(&[c, p]) if self.is_param(p) && matches!(b.heap().view(c), View::Int(_) | View::Rational { .. }) => {
                        free_param = Some((p, c))
                    }
                    _ => return Ok(false),
                },
                _ => return Ok(false),
            }
        }
        let k = cx.eval(if mul { b.mul(&known) } else { b.add(&known) })?;
        match free_param {
            None => Ok(k == left),
            Some((q, c)) => {
                let v = cx.eval(if mul { b.div(left, k) } else { b.div(b.sub(left, k), c) })?;
                if mul && b.is_zero(v) {
                    return Ok(false);
                }
                Ok(self.bind(b, q, v, binds))
            }
        }
    }

    /// `t` with the bound parameters replaced (plain calls); `None` when a parameter is unbound.
    fn inst(&self, b: &Build<'h>, t: T<'h>, binds: &Binds<'h>) -> Option<T<'h>> {
        if self.is_param(t) {
            return binds.iter().find(|(q, _)| *q == t).map(|&(_, v)| v);
        }
        let h = b.heap();
        let View::App { head, args } = h.view(t) else { return Some(t) };
        if !self.has_param(b, t) {
            return Some(t);
        }
        let mut out = Vec::with_capacity(args.len());
        for &a in args {
            out.push(self.inst(b, a, binds)?);
        }
        Some(h.app(head, &out))
    }
}

/// `t` with `t` as `x`, `s` as `y` and the parameter names as the parameter symbols (plain calls).
fn rename<'h>(b: &Build<'h>, t: T<'h>, x: (&str, T<'h>), y: (&str, T<'h>), params: &[T<'h>]) -> T<'h> {
    let h = b.heap();
    match h.view(t) {
        View::Sym(s) => {
            let name = h.sym_name(s);
            if name == x.0 {
                return x.1;
            }
            if name == y.0 {
                return y.1;
            }
            match PARAMS.iter().position(|&p| p == name) {
                Some(i) => params[i],
                None => t,
            }
        }
        View::App { head, args } => {
            let head = rename(b, head, x, y, params);
            let new: Vec<T<'h>> = args.iter().map(|&a| rename(b, a, x, y, params)).collect();
            h.app(head, &new)
        }
        _ => t,
    }
}
