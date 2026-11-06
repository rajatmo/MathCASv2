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

/// `activate(e)`: every head `inactive(f)` in `e` replaced by `f`, then evaluated.
struct Activate {
    inactive: Sym,
}

impl Native for Activate {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [e] = args else { return Ok(Reply::Decline) };
        let marker = h.sym_term(self.inactive);
        let mut stack = vec![(*e, false)];
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
                stack.push((head, false));
                stack.extend(args.iter().map(|&a| (a, false)));
                continue;
            }
            let mut nh = done[&head];
            if let View::App { head: w, args: [f] } = h.view(nh) {
                if w == marker {
                    nh = *f;
                }
            }
            let na: Vec<T<'h>> = args.iter().map(|a| done[a]).collect();
            done.insert(u, h.app(nh, &na));
        }
        Ok(Reply::Again(done[e]))
    }
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
