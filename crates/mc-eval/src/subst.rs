//! Substitution of bindings into terms: pattern-variable bindings into right sides and term conditions (semantics
//! 6.4), arguments into closure bodies and local values into blocks (6.7). Substitution is **hygienic** (6.6): a
//! binding form inside the term whose bound name occurs in a substituted value is renamed in its scope, so a
//! substituted term never captures; a name the form binds shadows the substitution of that name inside it.
//!
//! The binding forms ([`Scopes`]) are the term shapes `mc-syntax` lowers to:
//!
//! | form | binds | in |
//! |---|---|---|
//! | `fn(list(params), body)` | each parameter's name (`x`, `typed(x, T)`, `optional(x, d)`, `held(x)`, `rest(x)`) | the parameter list and `body` |
//! | `block(..., let(x, v), s, ...)`, `var(x, v)` | `x` (also `typed(x, T)`) | the statements after it |
//! | `for(x, xs, body)` | `x` | `body` |
//! | `rule(l, r)`, `rule_delayed(l, r)`, `set(l, r)`, `set_delayed(l, r)` with a compound `l` | the pattern variables of `l` | `l` and `r` |
//! | `define_fn(lhs, body, opts, type, decorators)` | the pattern variables of `lhs` and `opts` | `lhs`, `body`, `opts` |
//!
//! A renamed name `y` becomes `y$k` with the least `k` that occurs neither in the substituted values nor in the
//! form: deterministic, so results do not depend on the evaluation order or the thread count, and not writable as a
//! plain identifier.
//!
//! Every walk is iterative (terms may be arbitrarily deep); work is shared across repeated subterms (terms are DAGs);
//! a subterm whose symbol mask shows none of the substituted names is left as it is.
use crate::matcher::{Binds, Val};
use mc_core::{Heap, Store, Sym, View, T};
use rustc_hash::FxHashMap;

/// The heads of the binding forms and of the wrappers that name their variables.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Scopes {
    pub(crate) fn_: Sym,
    pub(crate) block: Sym,
    pub(crate) let_: Sym,
    pub(crate) var: Sym,
    pub(crate) for_: Sym,
    pub(crate) rule: Sym,
    pub(crate) rule_delayed: Sym,
    pub(crate) set: Sym,
    pub(crate) set_delayed: Sym,
    pub(crate) define_fn: Sym,
    pub(crate) any: Sym,
    pub(crate) seq0: Sym,
    pub(crate) seq1: Sym,
    pub(crate) named: Sym,
    pub(crate) typed: Sym,
    pub(crate) optional: Sym,
    pub(crate) held: Sym,
    pub(crate) rest: Sym,
    /// mask of the binding-form heads: a term without these bits contains no binding form
    pub(crate) mask: u64,
}

impl Scopes {
    pub(crate) fn new(h: &Heap) -> Scopes {
        let s = |n: &str| h.sym(n);
        let mut sc = Scopes {
            fn_: s("fn"),
            block: s("block"),
            let_: s("let"),
            var: s("var"),
            for_: s("for"),
            rule: s(mc_api::names::RULE),
            rule_delayed: s(mc_api::names::RULE_DELAYED),
            set: s("set"),
            set_delayed: s("set_delayed"),
            define_fn: s("define_fn"),
            any: s("any"),
            seq0: s("seq0"),
            seq1: s("seq1"),
            named: s("named"),
            typed: s("typed"),
            optional: s("optional"),
            held: s("held"),
            rest: s("rest"),
            mask: 0,
        };
        sc.mask = [sc.fn_, sc.block, sc.for_, sc.rule, sc.rule_delayed, sc.set, sc.set_delayed, sc.define_fn]
            .iter()
            .fold(0, |m, &x| m | h.mask(h.sym_term(x)));
        sc
    }

    /// The name a parameter or local binding introduces: `x`, or `x` inside `typed`, `optional`, `held`, `rest` (and
    /// `any`, for a pattern-style parameter).
    pub(crate) fn binder_name<'h>(&self, h: &'h Heap, mut p: T<'h>) -> Option<Sym> {
        // the wrappers nest at most a few levels; the bound keeps a hostile term from looping long
        for _ in 0..8 {
            match h.view(p) {
                View::Sym(s) => return Some(s),
                View::App { head, args } if !args.is_empty() => {
                    let View::Sym(w) = h.view(head) else { return None };
                    let wrapper = [self.typed, self.optional, self.held, self.rest, self.any].contains(&w);
                    if !wrapper || (w == self.any && args.len() != 1) {
                        return None;
                    }
                    p = args[0];
                }
                _ => return None,
            }
        }
        None
    }

    /// The pattern variables of pattern term `l`: the names in `any(x)`, `seq1(x)`, `seq0(x)` and `named(x, p)`.
    pub(crate) fn pattern_vars<'h>(&self, h: &'h Heap, l: T<'h>, out: &mut Vec<Sym>) {
        let pmask = [self.any, self.seq0, self.seq1, self.named].iter().fold(0, |m, &x| m | h.mask(h.sym_term(x)));
        let mut stack = vec![l];
        while let Some(u) = stack.pop() {
            if h.mask(u) & pmask == 0 {
                continue;
            }
            let View::App { head, args } = h.view(u) else { continue };
            if let View::Sym(w) = h.view(head) {
                let named = w == self.named && args.len() == 2;
                if (named || ((w == self.any || w == self.seq0 || w == self.seq1) && args.len() == 1))
                    && matches!(h.view(args[0]), View::Sym(_))
                {
                    if let View::Sym(x) = h.view(args[0]) {
                        if !out.contains(&x) {
                            out.push(x);
                        }
                    }
                    if named {
                        stack.push(args[1]);
                    }
                    continue;
                }
            }
            stack.push(head);
            stack.extend_from_slice(args);
        }
    }
}

/// The term variable `v` stands for: its term, or `list(...)` of a sequence variable's terms (semantics 6.1).
pub(crate) fn value<'h>(h: &'h Heap, b: &Binds<'h>, v: u16, list: Sym) -> T<'h> {
    match b.get(v) {
        Val::One(t) => t,
        Val::Seq(..) => h.call(list, b.seq(v)),
        Val::Unbound => h.call(list, &[]),
    }
}

/// Does symbol `s` occur in `t`?
pub(crate) fn contains_sym<'h>(h: &'h Heap, t: T<'h>, s: Sym) -> bool {
    let st = h.sym_term(s);
    let bit = h.mask(st);
    let mut stack = vec![t];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(u) = stack.pop() {
        if h.mask(u) & bit == 0 || !seen.insert(u) {
            continue;
        }
        match h.view(u) {
            View::Sym(_) if u == st => return true,
            View::App { head, args } => {
                stack.push(head);
                stack.extend_from_slice(args);
            }
            _ => {}
        }
    }
    false
}

/// `t` with each symbol `pairs[i].0` replaced by `pairs[i].1`, hygienically (see the module documentation).
pub(crate) fn substitute<'h>(h: &'h Heap, sc: &Scopes, t: T<'h>, pairs: &[(Sym, T<'h>)]) -> T<'h> {
    let keys = pairs.iter().fold(0u64, |m, (s, _)| m | h.mask(h.sym_term(*s)));
    if pairs.is_empty() || h.mask(t) & keys == 0 {
        return t;
    }
    if h.mask(t) & sc.mask == 0 {
        return plain(h, t, pairs, keys);
    }
    let mut s = Sub { h, sc, envs: vec![Env::new(h, pairs.to_vec())], done: FxHashMap::default() };
    s.run(t)
}

/// Work and value slots of [`plain_small`].
const SMALL: usize = 48;

/// Substitution into a small term without binding forms, on fixed buffers (no allocation, no sharing of repeated
/// subterms: most rule right sides are a few nodes). `None` when the term outgrows the buffers.
fn plain_small<'h>(h: &'h Heap, t: T<'h>, pairs: &[(Sym, T<'h>)], keys: u64) -> Option<T<'h>> {
    let mut work = [(t, false); SMALL];
    let mut nw = 1;
    let mut vals = [t; SMALL];
    let mut nv = 0;
    while nw > 0 {
        nw -= 1;
        let (u, expanded) = work[nw];
        if expanded {
            let View::App { head, args } = h.view(u) else { unreachable!("only compounds are expanded") };
            let parts = &vals[nv - args.len() - 1..nv];
            let r = if parts[0] == head && parts[1..] == *args { u } else { h.app(parts[0], &parts[1..]) };
            nv -= args.len() + 1;
            vals[nv] = r;
            nv += 1;
            continue;
        }
        let r = if h.mask(u) & keys == 0 {
            u
        } else {
            match h.view(u) {
                View::Sym(s) => pairs.iter().rev().find(|(p, _)| *p == s).map_or(u, |(_, v)| *v),
                View::App { head, args } => {
                    // the children are visited in order (head first) and leave their results in order
                    if nw + args.len() + 2 > SMALL || nv + args.len() + 1 > SMALL {
                        return None;
                    }
                    work[nw] = (u, true);
                    nw += 1;
                    for &a in args.iter().rev() {
                        work[nw] = (a, false);
                        nw += 1;
                    }
                    work[nw] = (head, false);
                    nw += 1;
                    continue;
                }
                _ => u,
            }
        };
        if nv == SMALL {
            return None;
        }
        vals[nv] = r;
        nv += 1;
    }
    Some(vals[0])
}

/// Substitution into a term without binding forms.
fn plain<'h>(h: &'h Heap, t: T<'h>, pairs: &[(Sym, T<'h>)], keys: u64) -> T<'h> {
    if let Some(r) = plain_small(h, t, pairs, keys) {
        return r;
    }
    let mut done: FxHashMap<T<'h>, T<'h>> = FxHashMap::default();
    let mut stack = vec![(t, false)];
    let mut parts: Vec<T<'h>> = vec![];
    while let Some((u, expanded)) = stack.pop() {
        if done.contains_key(&u) {
            continue;
        }
        let r = if h.mask(u) & keys == 0 {
            u
        } else {
            match h.view(u) {
                View::Sym(s) => pairs.iter().rev().find(|(p, _)| *p == s).map_or(u, |(_, v)| *v),
                View::App { head, args } => {
                    if !expanded {
                        stack.push((u, true));
                        stack.push((head, false));
                        stack.extend(args.iter().map(|&a| (a, false)));
                        continue;
                    }
                    parts.clear();
                    parts.extend(args.iter().map(|a| done[a]));
                    h.app(done[&head], &parts)
                }
                _ => u,
            }
        };
        done.insert(u, r);
    }
    done[&t]
}

/// The substitution in force in one scope.
struct Env<'h> {
    pairs: Vec<(Sym, T<'h>)>,
    /// mask of the names substituted
    keys: u64,
    /// mask of the values
    vals: u64,
}

impl<'h> Env<'h> {
    fn new(h: &'h Heap, pairs: Vec<(Sym, T<'h>)>) -> Env<'h> {
        let keys = pairs.iter().fold(0u64, |m, (s, _)| m | h.mask(h.sym_term(*s)));
        let vals = pairs.iter().fold(0u64, |m, (_, v)| m | h.mask(*v));
        Env { pairs, keys, vals }
    }
}

/// Work items of the iterative walk; results go to a value stack.
enum Frame<'h> {
    /// substitute into a term under an environment
    Visit(T<'h>, u32),
    /// rebuild a compound from its head and `n` arguments on the value stack
    App(usize),
    /// rebuild compound `t` (its head unchanged) from `n` arguments on the value stack
    Args(T<'h>, usize),
    /// remember the result on top of the value stack for this term and environment
    Done(T<'h>, u32),
}

struct Sub<'a, 'h> {
    h: &'h Heap,
    sc: &'a Scopes,
    envs: Vec<Env<'h>>,
    done: FxHashMap<(T<'h>, u32), T<'h>>,
}

impl<'h> Sub<'_, 'h> {
    fn run(&mut self, t: T<'h>) -> T<'h> {
        let h = self.h;
        let mut frames = vec![Frame::Visit(t, 0)];
        let mut vals: Vec<T<'h>> = vec![];
        while let Some(f) = frames.pop() {
            match f {
                Frame::Visit(u, e) => {
                    let env = &self.envs[e as usize];
                    if h.mask(u) & env.keys == 0 {
                        vals.push(u);
                        continue;
                    }
                    if let Some(&r) = self.done.get(&(u, e)) {
                        vals.push(r);
                        continue;
                    }
                    match h.view(u) {
                        View::Sym(s) => vals.push(env.pairs.iter().rev().find(|(p, _)| *p == s).map_or(u, |(_, v)| *v)),
                        View::App { head, args } => {
                            frames.push(Frame::Done(u, e));
                            if !self.binder(u, head, args, e, &mut frames) {
                                frames.push(Frame::App(args.len()));
                                frames.extend(args.iter().rev().map(|&a| Frame::Visit(a, e)));
                                frames.push(Frame::Visit(head, e));
                            }
                        }
                        _ => vals.push(u),
                    }
                }
                Frame::App(n) => {
                    let parts = vals.split_off(vals.len() - n - 1);
                    vals.push(h.app(parts[0], &parts[1..]));
                }
                Frame::Args(u, n) => {
                    let parts = vals.split_off(vals.len() - n);
                    let View::App { head, args } = h.view(u) else { unreachable!("Args frames hold compounds") };
                    vals.push(if *args == *parts { u } else { h.app(head, &parts) });
                }
                Frame::Done(u, e) => {
                    let r = *vals.last().expect("a visit leaves its result");
                    self.done.insert((u, e), r);
                }
            }
        }
        vals.pop().expect("the walk leaves one result")
    }

    /// Plan the walk of a binding form: push the frames that substitute each argument in its scope. False when `u`
    /// is not a binding form (then it is walked as an ordinary compound).
    fn binder(&mut self, u: T<'h>, head: T<'h>, args: &[T<'h>], e: u32, frames: &mut Vec<Frame<'h>>) -> bool {
        let (h, sc) = (self.h, *self.sc);
        let View::Sym(f) = h.view(head) else { return false };
        if h.mask(head) & sc.mask == 0 {
            return false;
        }
        let compound = |t: T<'h>| matches!(h.view(t), View::App { .. });
        // per argument: whether it is in the inner scope
        let (inner_args, names): (&[bool], Vec<Sym>) = if f == sc.fn_ && args.len() == 2 {
            let View::App { args: ps, .. } = h.view(args[0]) else { return false };
            let mut names = vec![];
            for &p in ps {
                match sc.binder_name(h, p) {
                    Some(x) if !names.contains(&x) => names.push(x),
                    Some(_) => {}
                    None => return false,
                }
            }
            (&[true, true], names)
        } else if f == sc.for_ && args.len() == 3 {
            let Some(x) = sc.binder_name(h, args[0]) else { return false };
            (&[true, false, true], vec![x])
        } else if (f == sc.rule || f == sc.rule_delayed || ((f == sc.set || f == sc.set_delayed) && compound(args[0])))
            && args.len() == 2
        {
            let mut names = vec![];
            sc.pattern_vars(h, args[0], &mut names);
            (&[true, true], names)
        } else if f == sc.define_fn && args.len() == 5 {
            let mut names = vec![];
            sc.pattern_vars(h, args[0], &mut names);
            sc.pattern_vars(h, args[2], &mut names);
            (&[true, true, true, false, false], names)
        } else if f == sc.block {
            self.block(u, args, e, frames);
            return true;
        } else {
            return false;
        };
        let inner = self.derive(e, &names, u);
        frames.push(Frame::Args(u, args.len()));
        for (i, &a) in args.iter().enumerate().rev() {
            frames.push(Frame::Visit(a, if inner_args[i] { inner } else { e }));
        }
        true
    }

    /// A block: each `let` or `var` starts a new scope for the statements after it; its value is in the old one.
    fn block(&mut self, u: T<'h>, stmts: &[T<'h>], e: u32, frames: &mut Vec<Frame<'h>>) {
        let (h, sc) = (self.h, *self.sc);
        // (statement, its own frames) planned forward, pushed backward
        let mut plan: Vec<Vec<Frame<'h>>> = Vec::with_capacity(stmts.len());
        let mut env = e;
        for &s in stmts {
            let local = match h.view(s) {
                View::App { head, args } if args.len() == 2 && (head == h.sym_term(sc.let_) || head == h.sym_term(sc.var)) => {
                    sc.binder_name(h, args[0]).map(|x| (x, args))
                }
                _ => None,
            };
            match local {
                Some((x, args)) => {
                    let next = self.derive(env, &[x], u);
                    // let(x, v): `x` in the new scope (a rename applies to it), `v` in the old one
                    plan.push(vec![Frame::Args(s, 2), Frame::Visit(args[1], env), Frame::Visit(args[0], next)]);
                    env = next;
                }
                None => plan.push(vec![Frame::Visit(s, env)]),
            }
        }
        frames.push(Frame::Args(u, stmts.len()));
        for p in plan.into_iter().rev() {
            frames.extend(p);
        }
    }

    /// The environment inside a form binding `names` (whose whole term is `scope`): the bound names are no longer
    /// substituted, and a bound name that occurs in a substituted value is renamed.
    fn derive(&mut self, e: u32, names: &[Sym], scope: T<'h>) -> u32 {
        let h = self.h;
        let env = &self.envs[e as usize];
        let bit = |s: Sym| h.mask(h.sym_term(s));
        let captures = |pairs: &[(Sym, T<'h>)], vals: u64, y: Sym| {
            vals & bit(y) != 0 && pairs.iter().any(|&(_, v)| contains_sym(h, v, y))
        };
        if names.iter().all(|&y| env.keys & bit(y) == 0 && !captures(&env.pairs, env.vals, y)) {
            return e;
        }
        let mut pairs: Vec<(Sym, T<'h>)> = env.pairs.iter().copied().filter(|(k, _)| !names.contains(k)).collect();
        let vals = pairs.iter().fold(0u64, |m, (_, v)| m | h.mask(*v));
        let mut renames: Vec<(Sym, T<'h>)> = vec![];
        for &y in names {
            if !captures(&pairs, vals, y) {
                continue;
            }
            let base = h.sym_name(y);
            let fresh = (1u32..)
                .map(|k| h.sym(&format!("{base}${k}")))
                .find(|&c| {
                    !contains_sym(h, scope, c)
                        && !pairs.iter().any(|&(_, v)| contains_sym(h, v, c))
                        && !renames.iter().any(|&(_, v)| v == h.sym_term(c))
                })
                .expect("some suffix is free");
            renames.push((y, h.sym_term(fresh)));
        }
        pairs.extend(renames);
        self.envs.push(Env::new(h, pairs));
        (self.envs.len() - 1) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mc_core::Config;

    #[test]
    fn substitution_is_hygienic() {
        let h = &Heap::new(Config::no_collect());
        let sc = Scopes::new(h);
        fn c<'h>(h: &'h Heap, n: &str, a: &[T<'h>]) -> T<'h> {
            h.call(h.sym(n), a)
        }
        let s = |n: &str| h.sym_term(h.sym(n));
        let (x, y) = (s("x"), s("y"));
        // fn([y], x*y) with x := y + 1: the parameter is renamed
        let lam = c(h, "fn", &[c(h, "list", &[y]), c(h, "mul", &[x, y])]);
        let v = c(h, "add", &[y, h.int(1)]);
        let r = substitute(h, &sc, lam, &[(h.sym("x"), v)]);
        let y1 = s("y$1");
        assert_eq!(r, c(h, "fn", &[c(h, "list", &[y1]), c(h, "mul", &[v, y1])]));
        // fn([x], x) with x := 3: shadowed
        let id = c(h, "fn", &[c(h, "list", &[x]), x]);
        assert_eq!(substitute(h, &sc, id, &[(h.sym("x"), h.int(3))]), id);
        // block(let(x, x), x) with x := 2: the value sees the outer x, the rest the local one
        let b = c(h, "block", &[c(h, "let", &[x, x]), x]);
        assert_eq!(substitute(h, &sc, b, &[(h.sym("x"), h.int(2))]), c(h, "block", &[c(h, "let", &[x, h.int(2)]), x]));
        // rule(f(any(y)), y + x) with x := y: the pattern variable is renamed in the pattern and the right side
        let rl = c(h, "rule", &[c(h, "f", &[c(h, "any", &[y])]), c(h, "add", &[y, x])]);
        let r = substitute(h, &sc, rl, &[(h.sym("x"), y)]);
        assert_eq!(r, c(h, "rule", &[c(h, "f", &[c(h, "any", &[y1])]), c(h, "add", &[y1, y])]));
        // a deep term
        let mut deep = x;
        for _ in 0..200_000 {
            deep = c(h, "g", &[deep]);
        }
        let r = substitute(h, &sc, c(h, "fn", &[c(h, "list", &[y]), deep]), &[(h.sym("x"), y)]);
        assert!(contains_sym(h, r, h.sym("y$1")));
    }
}
