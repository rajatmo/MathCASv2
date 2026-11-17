//! `reduce(cond, vars, complexes)` for polynomial conditions with order relations, and conditions written with
//! `re`, `im_part` and `conjugate` of the unknowns: each unknown `z` is `a + b i` with two real variables, every
//! atom is split into its real and imaginary parts (polynomial arithmetic on the pairs), and the real cylindrical
//! decomposition solves the resulting real condition (`mc_cad::reduce_reals`). The answer is written with `re(z)`
//! and `im_part(z)` for `a` and `b`, the real part first.
//!
//! An equation `p == q` holds when both parts of `p - q` vanish; `p != q` when one does not; an order relation
//! `p < q` holds when `p - q` is real and its real part is negative (an order relation between non-real numbers
//! is false). A negation is pushed to the atoms. A denominator is cleared with the conjugate:
//! `1/(P + iQ) = (P - iQ)/(P^2 + Q^2)`, and the condition `P^2 + Q^2 != 0` is added to the atom.
use mc_api::{names, Store, View, T};
use mc_eval::{NativeCx, Res};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A complex value as a pair of real terms (real part, imaginary part).
type Pair<'h> = (T<'h>, T<'h>);

struct Cx<'a, 'b, 'c, 'h> {
    cx: &'a mut NativeCx<'b, 'c, 'h>,
    h: &'h mc_core::Heap,
    vars: Vec<T<'h>>,
    /// the real variables, two per unknown
    parts: Vec<Pair<'h>>,
    /// conditions that a denominator does not vanish, collected for the atom being converted
    dom: Vec<T<'h>>,
}

/// Does `t` hold `re`, `im_part` or `conjugate` of something?
pub(crate) fn has_parts<'h>(h: &'h impl Store, t: T<'h>) -> bool {
    let mut stack = mc_api::walk::Walk::new(t);
    while let Some(u) = stack.pop() {
        if let View::App { head, args } = h.view(u) {
            if let View::Sym(s) = h.view(head) {
                if matches!(h.sym_name(s).as_str(), "re" | "im_part" | "conjugate") {
                    return true;
                }
            }
            stack.extend(args.iter().copied());
        }
    }
    false
}

/// The complex reduction; `None` when a formula is outside what the split handles.
pub(crate) fn complexes<'h>(cx: &mut NativeCx<'_, '_, 'h>, stop: &Arc<AtomicBool>, cond: T<'h>, vars: &[T<'h>]) -> Res<Option<T<'h>>> {
    let h = cx.heap();
    let mut real_vars = vec![];
    let mut parts = vec![];
    for &v in vars {
        let View::Sym(s) = h.view(v) else { return Ok(None) };
        let n = h.sym_name(s);
        let a = h.sym_term(h.sym(&format!("{n}$re")));
        let b = h.sym_term(h.sym(&format!("{n}$im")));
        real_vars.extend([a, b]);
        parts.push((a, b));
    }
    let mut c = Cx { cx, h, vars: vars.to_vec(), parts, dom: vec![] };
    let Some(rc) = c.formula(cond, false)? else { return Ok(None) };
    let Some(t) = mc_cad::reduce_reals(c.cx, stop, rc, &real_vars)? else { return Ok(None) };
    // the real variables back as re(z), im_part(z)
    let mut rules = vec![];
    for (&v, &(a, b)) in vars.iter().zip(&c.parts) {
        rules.push(h.call(h.sym(names::RULE), &[a, h.call(h.sym("re"), &[v])]));
        rules.push(h.call(h.sym(names::RULE), &[b, h.call(h.sym("im_part"), &[v])]));
    }
    let t = replace(h, t, &rules_pairs(h, &rules));
    Ok(Some(points(c.cx, t, vars)?))
}

fn rules_pairs<'h>(h: &'h impl Store, rules: &[T<'h>]) -> Vec<(T<'h>, T<'h>)> {
    rules
        .iter()
        .filter_map(|&r| match h.view(r) {
            View::App { args: &[a, b], .. } => Some((a, b)),
            _ => None,
        })
        .collect()
}

/// `t` with the left sides of the rules replaced by their right sides (shared subterms once).
fn replace<'h>(h: &'h impl Store, t: T<'h>, rules: &[(T<'h>, T<'h>)]) -> T<'h> {
    fn go<'h>(h: &'h impl Store, t: T<'h>, rules: &[(T<'h>, T<'h>)], memo: &mut mc_api::walk::Memo<T<'h>, T<'h>>) -> T<'h> {
        if let Some(r) = rules.iter().find(|r| r.0 == t) {
            return r.1;
        }
        if let Some(&r) = memo.get(&t) {
            return r;
        }
        let r = match h.view(t) {
            View::App { head, args } => {
                let new: Vec<T<'h>> = args.iter().map(|&a| go(h, a, rules, memo)).collect();
                h.app(head, &new)
            }
            _ => t,
        };
        memo.insert(t, r);
        r
    }
    go(h, t, rules, &mut mc_api::walk::memo())
}

/// For one unknown whose every case is `re(z) == c and im_part(z) == d` with constants: `z == c + d i`.
fn points<'h>(cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, vars: &[T<'h>]) -> Res<T<'h>> {
    let h = cx.heap();
    let [z] = vars else { return Ok(t) };
    let name = |u: T<'h>| match h.view(u) {
        View::App { head, args } => match h.view(head) {
            View::Sym(s) => Some((h.sym_name(s), args)),
            _ => None,
        },
        _ => None,
    };
    let re = h.call(h.sym("re"), &[*z]);
    let im = h.call(h.sym("im_part"), &[*z]);
    let constant = |u: T<'h>| {
        let mut stack = mc_api::walk::Walk::new(u);
        while let Some(w) = stack.pop() {
            match h.view(w) {
                View::Sym(s) if !matches!(h.sym_name(s).as_str(), "pi" | "e") => return false,
                View::App { args, .. } => stack.extend(args.iter().copied()),
                _ => {}
            }
        }
        true
    };
    let case = |u: T<'h>| -> Option<(T<'h>, T<'h>)> {
        let (n, a) = name(u)?;
        if n != "and" || a.len() != 2 {
            return None;
        }
        let side = |e: T<'h>, lhs: T<'h>| match name(e) {
            Some((n, &[l, r])) if n == "equal" && l == lhs && constant(r) => Some(r),
            _ => None,
        };
        Some((side(a[0], re)?, side(a[1], im)?))
    };
    let items: Vec<T<'h>> = match name(t) {
        Some((n, a)) if n == "or" => a.to_vec(),
        _ => vec![t],
    };
    let mut out = vec![];
    for &u in &items {
        let Some((c, d)) = case(u) else { return Ok(t) };
        let i = cx.eval(h.call(h.sym("complex"), &[h.int(0), h.int(1)]))?;
        let value = cx.eval(h.call(h.sym("add"), &[c, h.call(h.sym("mul"), &[d, i])]))?;
        out.push(h.call(h.sym("equal"), &[*z, value]));
    }
    Ok(match out.as_slice() {
        [one] => *one,
        _ => h.call(h.sym("or"), &out),
    })
}

impl<'h> Cx<'_, '_, '_, 'h> {
    fn call(&self, name: &str, args: &[T<'h>]) -> T<'h> {
        self.h.call(self.h.sym(name), args)
    }

    /// The symbols of a quantifier's variable list (or one symbol).
    fn names(&self, t: T<'h>) -> Option<Vec<T<'h>>> {
        let items = self.cx.as_list(t).map(<[T<'h>]>::to_vec).unwrap_or_else(|| vec![t]);
        items.iter().all(|&v| matches!(self.h.view(v), View::Sym(_))).then_some(items)
    }

    fn neg(&mut self, t: T<'h>) -> Res<T<'h>> {
        let n = self.call("mul", &[self.h.int(-1), t]);
        self.cx.eval(n)
    }

    fn expand(&mut self, t: T<'h>) -> Res<T<'h>> {
        let e = self.call("expand", &[t]);
        self.cx.eval(e)
    }

    fn add(&mut self, a: Pair<'h>, b: Pair<'h>) -> Res<Pair<'h>> {
        let (r, i) = (self.call("add", &[a.0, b.0]), self.call("add", &[a.1, b.1]));
        Ok((self.cx.eval(r)?, self.cx.eval(i)?))
    }

    fn mul(&mut self, a: Pair<'h>, b: Pair<'h>) -> Res<Pair<'h>> {
        let h = self.h;
        let m1 = self.call("mul", &[a.0, b.0]);
        let m2 = self.call("mul", &[a.1, b.1]);
        let m3 = self.call("mul", &[a.0, b.1]);
        let m4 = self.call("mul", &[a.1, b.0]);
        let r = self.call("add", &[m1, self.call("mul", &[h.int(-1), m2])]);
        let i = self.call("add", &[m3, m4]);
        Ok((self.expand(r)?, self.expand(i)?))
    }

    /// The pair of `t`; `None` outside polynomial arithmetic of the unknowns and real constants.
    fn pair(&mut self, t: T<'h>) -> Res<Option<Pair<'h>>> {
        let h = self.h;
        let zero = h.int(0);
        match h.view(t) {
            View::Int(_) | View::BigInt { .. } | View::Rational { .. } | View::Real(_) => Ok(Some((t, zero))),
            View::Sym(s) => {
                if let Some(k) = self.vars.iter().rposition(|&v| v == t) {
                    return Ok(Some(self.parts[k]));
                }
                Ok(match h.sym_name(s).as_str() {
                    "im" => Some((zero, h.int(1))),
                    "pi" | "e" | "euler_gamma" | "golden_ratio" | "catalan" => Some((t, zero)),
                    _ => None,
                })
            }
            View::App { head, args } => {
                let View::Sym(s) = h.view(head) else { return Ok(None) };
                let name = h.sym_name(s);
                match (name.as_str(), args) {
                    ("add", _) => {
                        let mut acc = (zero, zero);
                        for &a in args {
                            let Some(p) = self.pair(a)? else { return Ok(None) };
                            acc = self.add(acc, p)?;
                        }
                        return Ok(Some(acc));
                    }
                    ("mul", _) => {
                        let mut acc = (h.int(1), zero);
                        for &a in args {
                            let Some(p) = self.pair(a)? else { return Ok(None) };
                            acc = self.mul(acc, p)?;
                        }
                        return Ok(Some(acc));
                    }
                    ("pow", [b, e]) => {
                        if let View::Int(k) = h.view(*e) {
                            let Some(p) = self.pair(*b)? else { return Ok(None) };
                            if k.unsigned_abs() > 64 {
                                return Ok(None);
                            }
                            let mut acc = (h.int(1), zero);
                            for _ in 0..k.unsigned_abs() {
                                acc = self.mul(acc, p)?;
                            }
                            if k >= 0 {
                                return Ok(Some(acc));
                            }
                            // 1/(P + iQ) = (P - iQ)/(P^2 + Q^2)
                            let ni = self.neg(acc.1)?;
                            let n2 = self.mul(acc, (acc.0, ni))?;
                            let den = n2.0;
                            self.dom.push(self.call("unequal", &[den, zero]));
                            let inv = self.call("pow", &[den, h.int(-1)]);
                            let (r, i) = (self.call("mul", &[acc.0, inv]), self.call("mul", &[h.int(-1), acc.1, inv]));
                            return Ok(Some((self.cx.eval(r)?, self.cx.eval(i)?)));
                        }
                    }
                    ("complex", [r, i]) => return Ok(Some((*r, *i))),
                    ("re", [u]) => {
                        let Some(p) = self.pair(*u)? else { return Ok(None) };
                        return Ok(Some((p.0, zero)));
                    }
                    ("im_part", [u]) => {
                        let Some(p) = self.pair(*u)? else { return Ok(None) };
                        return Ok(Some((p.1, zero)));
                    }
                    ("conjugate", [u]) => {
                        let Some(p) = self.pair(*u)? else { return Ok(None) };
                        let neg = self.call("mul", &[h.int(-1), p.1]);
                        return Ok(Some((p.0, self.cx.eval(neg)?)));
                    }
                    _ => {}
                }
                // a real constant (no unknown, no symbol but the numeric constants)
                if self.constant(t)? {
                    return Ok(Some((t, zero)));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    fn constant(&mut self, t: T<'h>) -> Res<bool> {
        let h = self.h;
        let mut stack = mc_api::walk::Walk::new(t);
        while let Some(u) = stack.pop() {
            match h.view(u) {
                View::Sym(s) if !matches!(h.sym_name(s).as_str(), "pi" | "e" | "euler_gamma" | "golden_ratio" | "catalan") => return Ok(false),
                View::App { head, args } => {
                    stack.push(head);
                    stack.extend(args.iter().copied());
                }
                _ => {}
            }
        }
        // the value must be real: a numeric evaluation without an imaginary part
        let n = self.call("n", &[t]);
        let v = self.cx.eval(n)?;
        Ok(matches!(h.view(v), View::Real(_) | View::Int(_) | View::Rational { .. }))
    }

    /// The real condition of a formula (`neg`: of its negation).
    fn formula(&mut self, t: T<'h>, neg: bool) -> Res<Option<T<'h>>> {
        let h = self.h;
        if self.cx.is_true(t) || self.cx.is_false(t) {
            let b = self.cx.is_true(t) != neg;
            return Ok(Some(h.sym_term(h.sym(if b { names::TRUE } else { names::FALSE }))));
        }
        let (name, args) = match h.view(t) {
            View::App { head, args } => match h.view(head) {
                View::Sym(s) => (h.sym_name(s), args),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        let rel = match name.as_str() {
            "and" | "or" | "list" => {
                let conj = (name != "or") != neg;
                let mut out = vec![];
                for &a in args {
                    let Some(u) = self.formula(a, neg)? else { return Ok(None) };
                    out.push(u);
                }
                return Ok(Some(self.call(if conj { "and" } else { "or" }, &out)));
            }
            "not" => {
                let [u] = args else { return Ok(None) };
                return self.formula(*u, !neg);
            }
            // a quantifier over complex variables: over the real and imaginary parts, the condition of the
            // three-argument form joined to the body (`for_all` implies it, `exists` conjoins it)
            "exists" | "for_all" if matches!(args.len(), 2 | 3) => {
                let all = (name == "for_all") != neg;
                let Some(bound) = self.names(args[0]) else { return Ok(None) };
                let mark = (self.vars.len(), self.parts.len());
                let mut real = vec![];
                for &v in &bound {
                    let View::Sym(sy) = h.view(v) else { return Ok(None) };
                    let n = h.sym_name(sy);
                    let (a, b) = (h.sym_term(h.sym(&format!("{n}$re"))), h.sym_term(h.sym(&format!("{n}$im"))));
                    self.vars.push(v);
                    self.parts.push((a, b));
                    real.extend([a, b]);
                }
                let body = match args {
                    [_, b] => self.formula(*b, neg),
                    [_, c, b] => {
                        let (c, b) = (self.formula(*c, all), self.formula(*b, neg));
                        match (c?, b?) {
                            (Some(c), Some(b)) => Ok(Some(self.call(if all { "or" } else { "and" }, &[c, b]))),
                            _ => Ok(None),
                        }
                    }
                    _ => Ok(None),
                };
                self.vars.truncate(mark.0);
                self.parts.truncate(mark.1);
                let Some(body) = body? else { return Ok(None) };
                let list = self.cx.list(&real);
                return Ok(Some(self.call(if all { "for_all" } else { "exists" }, &[list, body])));
            }
            "equal" | "unequal" | "less" | "less_equal" | "greater" | "greater_equal" => name.clone(),
            _ => return Ok(None),
        };
        if args.len() < 2 {
            return Ok(None);
        }
        let mut atoms = vec![];
        for w in args.windows(2) {
            self.dom.clear();
            let (Some(p), Some(q)) = (self.pair(w[0])?, self.pair(w[1])?) else { return Ok(None) };
            let (nr, ni) = (self.neg(q.0)?, self.neg(q.1)?);
            let (r, i) = self.add(p, (nr, ni))?;
            let zero = h.int(0);
            let rel = if neg { negate(&rel) } else { rel.as_str() };
            let atom = match rel {
                "equal" => self.call("and", &[self.call("equal", &[r, zero]), self.call("equal", &[i, zero])]),
                "unequal" => self.call("or", &[self.call("unequal", &[r, zero]), self.call("unequal", &[i, zero])]),
                // the negation of an order relation is true also for non-real differences
                _ if neg => self.call("or", &[self.call("unequal", &[i, zero]), self.call(rel, &[r, zero])]),
                _ => self.call("and", &[self.call("equal", &[i, zero]), self.call(rel, &[r, zero])]),
            };
            let mut all = self.dom.clone();
            all.push(atom);
            atoms.push(self.call("and", &all));
        }
        // a chain is the conjunction of its links (the negation of a chain, a disjunction)
        Ok(Some(match atoms.as_slice() {
            [one] => *one,
            _ => self.call(if neg { "or" } else { "and" }, &atoms),
        }))
    }
}

/// The relation that holds exactly when `rel` does not (between real numbers).
fn negate(rel: &str) -> &'static str {
    match rel {
        "equal" => "unequal",
        "unequal" => "equal",
        "less" => "greater_equal",
        "less_equal" => "greater",
        "greater" => "less_equal",
        _ => "less",
    }
}
