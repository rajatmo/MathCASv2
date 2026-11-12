//! Function composition and threading of heads (decision p6core.28): `composition(f, g, ...)(x)` is
//! `f(g(...(x)))`, `right_composition(f, g, ...)(x)` is `...(g(f(x)))`, `through(p(f, g)(x))` is `p(f(x), g(x))`,
//! and `activate(e)` turns the inert heads `inactive(f)` of `e` into `f`.
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Kernel, Native, NativeCx, Reply, Res};

/// `composition` (`right` false) and `right_composition`: their curried calls apply the functions in turn.
struct Compose {
    right: bool,
}

impl Native for Compose {
    fn curried(&self) -> bool {
        true
    }

    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let View::App { head, .. } = h.view(t) else { return Ok(Reply::Decline) };
        if let View::Sym(own) = h.view(head) {
            // the plain composition is flat and drops `identity`: `composition(f, composition(g, h), identity)` is
            // `composition(f, g, h)`, and the empty composition is `identity` (decision r36core.1)
            let identity = h.sym_term(h.sym("identity"));
            let mut flat: Vec<T<'h>> = Vec::with_capacity(args.len());
            let mut changed = false;
            for &a in args {
                match h.view(a) {
                    View::App { head: ah, args: inner } if matches!(h.view(ah), View::Sym(s) if s == own) => {
                        flat.extend(inner.iter().copied().filter(|&u| u != identity));
                        changed = true;
                    }
                    _ if a == identity => changed = true,
                    _ => flat.push(a),
                }
            }
            // neighbouring transformation functions compose into one (their matrices multiply: the outer function's
            // matrix on the left; the first function of a right composition is the inner one)
            let tfn = h.sym("transformation_function");
            let matrix = |a: T<'h>| match h.view(a) {
                View::App { head: th, args: [m] } if matches!(h.view(th), View::Sym(s) if s == tfn) => Some(*m),
                _ => None,
            };
            let mut merged: Vec<T<'h>> = Vec::with_capacity(flat.len());
            for a in flat {
                if let (Some(prev), Some(next)) = (merged.last().and_then(|&p| matrix(p)), matrix(a)) {
                    let (l, r) = if self.right { (next, prev) } else { (prev, next) };
                    let prod = cx.eval(h.call(h.sym("dot"), &[l, r]))?;
                    if !matches!(h.view(prod), View::App { head: ph, .. } if matches!(h.view(ph), View::Sym(s) if h.sym_name(s) == "dot")) {
                        merged.pop();
                        merged.push(h.call(tfn, &[prod]));
                        changed = true;
                        continue;
                    }
                }
                merged.push(a);
            }
            let flat = merged;
            return Ok(if flat.is_empty() {
                Reply::Done(identity)
            } else if let [only] = flat.as_slice() {
                // one function left: the composition of it alone is it
                Reply::Done(*only)
            } else if changed {
                Reply::Done(h.app(head, &flat))
            } else {
                Reply::Decline
            });
        }
        // only the call composition(f, ...)(x...): the plain composition stays as it is
        let View::App { args: fs, .. } = h.view(head) else { return Ok(Reply::Decline) };
        if matches!(h.view(head), View::App { head: inner, .. } if matches!(h.view(inner), View::App { .. })) {
            return Ok(Reply::Decline);
        }
        let Some(first) = (if self.right { fs.first() } else { fs.last() }) else {
            // the empty composition is the identity
            return Ok(match args {
                [x] => Reply::Done(*x),
                _ => Reply::Decline,
            });
        };
        let mut v = h.app(*first, args);
        let rest: Vec<T<'h>> = if self.right { fs[1..].to_vec() } else { fs[..fs.len() - 1].iter().rev().copied().collect() };
        for f in rest {
            cx.tick(1)?;
            v = h.app(f, &[v]);
        }
        Ok(Reply::Again(v))
    }
}

/// `through(p(f, g, ...)(x, ...))`: `p(f(x, ...), g(x, ...), ...)`; `through(e, p)` only when the inner head is `p`.
struct Through;

impl Native for Through {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (e, only) = match args {
            [e] => (*e, None),
            [e, p] => (*e, Some(*p)),
            _ => return Ok(Reply::Decline),
        };
        let View::App { head, args: xs } = h.view(e) else { return Ok(Reply::Done(e)) };
        let View::App { head: p, args: fs } = h.view(head) else { return Ok(Reply::Done(e)) };
        if only.is_some_and(|q| q != p) {
            return Ok(Reply::Done(e));
        }
        cx.tick(fs.len() as u64)?;
        let calls: Vec<T<'h>> = fs.iter().map(|&f| h.app(f, xs)).collect();
        Ok(Reply::Again(h.app(p, &calls)))
    }
}

/// `activate(e)`: every head `inactive(f)` in `e` replaced by `f`, then evaluated. `activate(e, p)` replaces only the
/// heads `inactive(f)` whose `f` matches the pattern `p` (a symbol is compared directly); `heads: false` leaves the
/// heads of calls alone (only arguments are walked).
struct Activate {
    inactive: Sym,
}

/// Which inert heads `activate` turns back: all, one symbol, or the matches of a pattern.
enum Which<'h> {
    All,
    Lit(T<'h>),
    Pat(mc_eval::Pattern),
}

impl Native for Activate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let (args, heads) = split_heads_option(h, args);
        let (e, which) = match args {
            [e] => (*e, Which::All),
            [e, p] => {
                let Ok(l) = mc_eval::syntax::pattern(h, *p) else { return Ok(Reply::Decline) };
                if matches!(l.pat, mc_eval::Pat::Lit(_)) {
                    (*e, Which::Lit(*p))
                } else {
                    match cx.compile_pattern(l.pat) {
                        Ok(c) => (*e, Which::Pat(c)),
                        Err(_) => return Ok(Reply::Decline),
                    }
                }
            }
            _ => return Ok(Reply::Decline),
        };
        let heads = match heads {
            None => true,
            Some(v) => match h.view(v) {
                View::Sym(s) if h.sym_name(s) == "false" => false,
                View::Sym(s) if h.sym_name(s) == "true" => true,
                _ => return Ok(Reply::Decline),
            },
        };
        let marker = h.sym_term(self.inactive);
        let mut stack = vec![(e, false)];
        let mut done: std::collections::HashMap<T<'h>, T<'h>> = Default::default();
        while let Some((u, expanded)) = stack.pop() {
            if done.contains_key(&u) {
                continue;
            }
            if h.packed(u).is_some() {
                done.insert(u, u);
                continue;
            }
            let View::App { head, args } = h.view(u) else {
                done.insert(u, u);
                continue;
            };
            if !expanded {
                cx.tick(1)?;
                stack.push((u, true));
                if heads {
                    stack.push((head, false));
                }
                stack.extend(args.iter().map(|&a| (a, false)));
                continue;
            }
            let mut nh = if heads { done[&head] } else { head };
            if let View::App { head: w, args: [f] } = h.view(nh) {
                if w == marker {
                    let take = match &which {
                        Which::All => true,
                        Which::Lit(p) => *p == *f,
                        Which::Pat(p) => cx.matches(p, *f)?,
                    };
                    if take {
                        nh = *f;
                    }
                }
            }
            let na: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
            done.insert(u, h.app(nh, &na));
        }
        Ok(Reply::Again(done[&e]))
    }
}

/// The arguments before a final `options(heads -> v)`, and `v`.
fn split_heads_option<'a, 'h>(h: &'h Heap, args: &'a [T<'h>]) -> (&'a [T<'h>], Option<T<'h>>) {
    let Some((&last, before)) = args.split_last() else { return (args, None) };
    let View::App { head, args: items } = h.view(last) else { return (args, None) };
    if !matches!(h.view(head), View::Sym(s) if h.sym_name(s) == "options") {
        return (args, None);
    }
    let v = items.iter().find_map(|&r| match h.view(r) {
        View::App { args: &[k, v], .. } if matches!(h.view(k), View::Sym(s) if h.sym_name(s) == "heads") => Some(v),
        _ => None,
    });
    (before, v)
}

/// `inactive(f)`: an inert head; `inactive(f)(x)` stays a call until `activate`.
struct Inert;

impl Native for Inert {
    fn call<'h>(&self, _cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, _args: &[T<'h>]) -> Res<Reply<'h>> {
        Ok(Reply::Decline)
    }
}

/// Register the functions of this module.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let a = Attributes::default();
    k.register(h.sym("composition"), a, Box::new(Compose { right: false }));
    k.register(h.sym("right_composition"), a, Box::new(Compose { right: true }));
    k.register(h.sym("through"), a, Box::new(Through));
    let inactive = h.sym("inactive");
    k.register(h.sym("activate"), a, Box::new(Activate { inactive }));
    k.register(inactive, a, Box::new(Inert));
}
