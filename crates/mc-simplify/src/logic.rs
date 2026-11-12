//! Equational reasoning under universally quantified equations (phase 12A): `simplify(l == r, for_all([a, b],
//! eq))` is `true` when rewriting with the axioms brings both sides to one normal form.
//!
//! Each axiom `for_all(vars, s == t)` (several joined by `and`) becomes
//!
//! - a **commutativity** declaration when `t` is `s` with the two arguments of its head swapped (`f(a, b) == f(b,
//!   a)`): the arguments of that head are kept sorted in the canonical order (rewriting modulo commutativity);
//! - otherwise a **rewrite rule** oriented from the larger side to the smaller (leaf count), or as written when the
//!   sizes are equal (`f(a, f(b, c)) == f(f(a, b), c)` turns right-nested products left-nested); a rule whose right
//!   side has variables its left side lacks is reversed, or dropped.
//!
//! Normal forms are computed innermost first, at most [`STEPS`] rewrites and terms of at most [`MAX_SIZE`] leaves.
//! Only a proof is reported: two different normal forms say nothing (the rules are not a complete system; a
//! Knuth-Bendix completion, Knuth and Bendix 1970, could make them one for more theories), and the goal is then
//! left to the other moves of `simplify`. Every step is an instance of an axiom, so a `true` is always a proof.
use crate::term::B;
use mc_api::{Store, Sym, View, T};
use mc_eval::{NativeCx, Res};
use rustc_hash::FxHashMap;

/// Most rewrite steps of one normal form.
pub const STEPS: usize = 2000;
/// Largest term (leaves) rewriting may produce.
pub const MAX_SIZE: usize = 400;

/// The equational theory of the context.
struct Theory<'h> {
    rules: Vec<(T<'h>, T<'h>)>,
    vars: Vec<Sym>,
    commutative: Vec<Sym>,
}

fn size<'h>(h: &'h mc_core::Heap, t: T<'h>) -> usize {
    match h.view(t) {
        View::App { head, args } => size(h, head) + args.iter().map(|&a| size(h, a)).sum::<usize>(),
        _ => 1,
    }
}

fn vars_of<'h>(h: &'h mc_core::Heap, t: T<'h>, vars: &[Sym], out: &mut Vec<Sym>) {
    match h.view(t) {
        View::Sym(s) if vars.contains(&s) => {
            if !out.contains(&s) {
                out.push(s)
            }
        }
        View::App { head, args } => {
            vars_of(h, head, vars, out);
            for &a in args {
                vars_of(h, a, vars, out);
            }
        }
        _ => {}
    }
}

/// The axioms among the facts (`for_all(vars, eq)` with `eq` an equation or a conjunction of equations).
fn theory<'h>(b: &B<'h>, facts: &[T<'h>]) -> Option<Theory<'h>> {
    let h = b.heap();
    let (for_all, equal, and) = (h.sym("for_all"), h.sym("equal"), h.sym("and"));
    let mut th = Theory { rules: vec![], vars: vec![], commutative: vec![] };
    for &f in facts {
        let Some((s, [vs, body])) = b.call_of(f) else { continue };
        if s != for_all {
            continue;
        }
        let vs: Vec<Sym> = match b.call_of(*vs) {
            Some((l, xs)) if l == b.hd.list => xs.iter().filter_map(|&x| b.sym_of(x)).collect(),
            _ => b.sym_of(*vs).into_iter().collect(),
        };
        let mut eqs = vec![*body];
        let mut i = 0;
        while i < eqs.len() {
            if let Some((a, xs)) = b.call_of(eqs[i]) {
                if a == and {
                    let xs = xs.to_vec();
                    eqs.remove(i);
                    eqs.extend(xs);
                    continue;
                }
            }
            i += 1;
        }
        for e in eqs {
            let Some((q, [l, r])) = b.call_of(e) else { continue };
            if q != equal {
                continue;
            }
            for v in &vs {
                if !th.vars.contains(v) {
                    th.vars.push(*v);
                }
            }
            // commutativity: f(a, b) == f(b, a)
            if let (Some((fl, [a1, a2])), Some((fr, [b1, b2]))) = (b.call_of(*l), b.call_of(*r)) {
                if fl == fr && a1 == b2 && a2 == b1 && a1 != a2 && vs.iter().any(|v| b.sym_of(*a1) == Some(*v)) && vs.iter().any(|v| b.sym_of(*a2) == Some(*v)) {
                    th.commutative.push(fl);
                    continue;
                }
            }
            let (sl, sr) = (size(h, *l), size(h, *r));
            let (mut lhs, mut rhs) = if sr > sl { (*r, *l) } else { (*l, *r) };
            let (mut vl, mut vr) = (vec![], vec![]);
            vars_of(h, lhs, &vs, &mut vl);
            vars_of(h, rhs, &vs, &mut vr);
            if !vr.iter().all(|v| vl.contains(v)) {
                if vl.iter().all(|v| vr.contains(v)) && b.call_of(rhs).is_some() {
                    std::mem::swap(&mut lhs, &mut rhs);
                } else {
                    continue;
                }
            }
            // a variable alone on the left would rewrite everything
            if b.call_of(lhs).is_none() {
                continue;
            }
            th.rules.push((lhs, rhs));
        }
    }
    (!th.rules.is_empty() || !th.commutative.is_empty()).then_some(th)
}

impl<'h> Theory<'h> {
    fn is_var(&self, b: &B<'h>, t: T<'h>) -> Option<Sym> {
        b.sym_of(t).filter(|s| self.vars.contains(s))
    }

    /// First-order matching of `p` against `t` (commutative heads in both orders).
    fn matches(&self, b: &B<'h>, p: T<'h>, t: T<'h>, binds: &mut FxHashMap<Sym, T<'h>>) -> bool {
        if let Some(v) = self.is_var(b, p) {
            return match binds.get(&v) {
                Some(&u) => u == t,
                None => {
                    binds.insert(v, t);
                    true
                }
            };
        }
        let h = b.heap();
        match (h.view(p), h.view(t)) {
            (View::App { head: hp, args: ap }, View::App { head: ht, args: at }) => {
                if hp != ht || ap.len() != at.len() {
                    return false;
                }
                let saved = binds.clone();
                if ap.iter().zip(at).all(|(&x, &y)| self.matches(b, x, y, binds)) {
                    return true;
                }
                *binds = saved.clone();
                let comm = b.sym_of(hp).is_some_and(|s| self.commutative.contains(&s));
                if comm && ap.len() == 2 && self.matches(b, ap[0], at[1], binds) && self.matches(b, ap[1], at[0], binds) {
                    return true;
                }
                *binds = saved;
                false
            }
            _ => p == t,
        }
    }

    fn subst(&self, b: &B<'h>, t: T<'h>, binds: &FxHashMap<Sym, T<'h>>) -> T<'h> {
        if let Some(v) = self.is_var(b, t) {
            return binds.get(&v).copied().unwrap_or(t);
        }
        let h = b.heap();
        match h.view(t) {
            View::App { head, args } => {
                let xs: Vec<T<'h>> = args.iter().map(|&a| self.subst(b, a, binds)).collect();
                h.app(head, &xs)
            }
            _ => t,
        }
    }

    /// One innermost rewrite step (the arguments of commutative heads sorted), or `None` at a normal form.
    fn step(&self, cx: &NativeCx<'_, '_, 'h>, b: &B<'h>, t: T<'h>) -> Option<T<'h>> {
        let h = b.heap();
        if let View::App { head, args } = h.view(t) {
            for (i, &a) in args.iter().enumerate() {
                if let Some(r) = self.step(cx, b, a) {
                    let mut xs = args.to_vec();
                    xs[i] = r;
                    return Some(h.app(head, &xs));
                }
            }
            if b.sym_of(head).is_some_and(|s| self.commutative.contains(&s)) && args.len() == 2 && cx.cmp(args[1], args[0]) == std::cmp::Ordering::Less {
                return Some(h.app(head, &[args[1], args[0]]));
            }
        }
        for &(l, r) in &self.rules {
            let mut binds = FxHashMap::default();
            if self.matches(b, l, t, &mut binds) {
                let out = self.subst(b, r, &binds);
                if out != t {
                    return Some(out);
                }
            }
        }
        None
    }

    fn normal(&self, cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, mut t: T<'h>) -> Res<Option<T<'h>>> {
        for _ in 0..STEPS {
            cx.tick(1)?;
            match self.step(cx, b, t) {
                None => return Ok(Some(t)),
                Some(r) => {
                    if size(b.heap(), r) > MAX_SIZE {
                        return Ok(None);
                    }
                    t = r;
                }
            }
        }
        Ok(None)
    }
}

/// `true` when the facts' axioms prove the equation `e`; `None` otherwise.
pub fn prove<'h>(cx: &mut NativeCx<'_, '_, 'h>, b: &B<'h>, facts: &[T<'h>], e: T<'h>) -> Res<Option<T<'h>>> {
    let h = b.heap();
    let Some((s, [l, r])) = b.call_of(e) else { return Ok(None) };
    if s != h.sym("equal") {
        return Ok(None);
    }
    let Some(th) = theory(b, facts) else { return Ok(None) };
    let (Some(nl), Some(nr)) = (th.normal(cx, b, *l)?, th.normal(cx, b, *r)?) else { return Ok(None) };
    Ok((nl == nr).then(|| cx.bool(true)))
}
