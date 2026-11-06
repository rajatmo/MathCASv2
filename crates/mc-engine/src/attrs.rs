//! Attributes as values (decision p6core.26): `attributes(f)`, `set_attributes(f, a)`, `clear_attributes(f, a)`.
//!
//! The names: `listable` (map over list arguments), `hold_all`, `hold_first`, `hold_rest` (arguments held),
//! `orderless` and `flat` (both mean an associative and commutative head: arguments flattened and sorted, matching
//! modulo AC), `effect` (calls perform effects: never evaluated ahead of their turn), `handles_failures` (failures
//! arrive as arguments). `protected`, `locked`, `read_protected`, `numeric_function`, `one_identity` and `constant`
//! are accepted and change nothing.
use mc_core::{Heap, Store, Sym, View, T};
use mc_eval::{Attributes, Hold, Kernel, Native, NativeCx, Reply, Res};

/// The names [`attributes`] gives, in this order.
const NAMES: &[&str] = &["effect", "handles_failures", "hold_all", "hold_first", "hold_rest", "listable", "orderless"];

/// Names accepted and ignored.
const IGNORED: &[&str] = &["protected", "locked", "read_protected", "numeric_function", "one_identity", "constant", "flat"];

fn names_of(a: Attributes) -> Vec<&'static str> {
    NAMES
        .iter()
        .copied()
        .filter(|n| match *n {
            "effect" => a.effect,
            "handles_failures" => a.handles_failures,
            "hold_all" => a.hold == Hold::All,
            "hold_first" => a.hold == Hold::First,
            "hold_rest" => a.hold == Hold::Rest,
            "listable" => a.listable,
            "orderless" => a.ac,
            _ => false,
        })
        .collect()
}

/// `a` with attribute `name` set (`on`) or cleared; `None` for an unknown name.
fn with(mut a: Attributes, name: &str, on: bool) -> Option<Attributes> {
    let hold = |a: &mut Attributes, h: Hold| {
        if on {
            a.hold = h;
        } else if a.hold == h {
            a.hold = Hold::None;
        }
    };
    match name {
        "listable" => a.listable = on,
        "effect" => a.effect = on,
        "handles_failures" => a.handles_failures = on,
        "hold_all" => hold(&mut a, Hold::All),
        "hold_first" => hold(&mut a, Hold::First),
        "hold_rest" => hold(&mut a, Hold::Rest),
        "orderless" | "flat" if on => a.ac = true,
        "orderless" => a.ac = false,
        n if IGNORED.contains(&n) => {}
        _ => return None,
    }
    Some(a)
}

/// The symbols named by `t`: one symbol or a list of symbols.
fn syms<'h>(h: &'h Heap, cx: &NativeCx<'_, '_, 'h>, t: T<'h>) -> Option<Vec<Sym>> {
    match h.view(t) {
        View::Sym(s) => Some(vec![s]),
        _ => cx.as_list(t)?.iter().map(|&x| if let View::Sym(s) = h.view(x) { Some(s) } else { None }).collect(),
    }
}

/// `attributes(f)`: the list of the attribute names of the symbol `f`.
struct AttributesOf;

impl Native for AttributesOf {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, _t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f] = args else { return Ok(Reply::Decline) };
        let View::Sym(s) = h.view(*f) else { return Ok(Reply::Decline) };
        // a function that maps over lists itself is listable to the user (decision io-core.1)
        let a = Attributes { listable: cx.maps_lists(s), ..cx.attributes(s) };
        let v: Vec<T<'h>> = names_of(a).iter().map(|n| h.sym_term(h.sym(n))).collect();
        Ok(Reply::Done(cx.list(&v)))
    }
}

/// `set_attributes(f, a)` (`set` true) and `clear_attributes(f, a)`: `a` one name or a list; `f` a symbol or a list.
struct SetAttributes {
    set: bool,
}

impl Native for SetAttributes {
    fn call<'h>(&self, cx: &mut NativeCx<'_, '_, 'h>, t: T<'h>, args: &[T<'h>]) -> Res<Reply<'h>> {
        let h = cx.heap();
        let [f, a] = args else { return Ok(Reply::Decline) };
        let failure = |cx: &NativeCx<'_, '_, 'h>, k: i64| {
            let head = match h.view(t) {
                View::App { head, .. } => head,
                _ => t,
            };
            let tag = |n: &str| h.sym_term(h.sym(n));
            Reply::Done(h.app(tag("failure"), &[tag("argument"), head, cx.heap().int(k)]))
        };
        let Some(fs) = syms(h, cx, *f) else { return Ok(failure(cx, 1)) };
        let Some(ns) = syms(h, cx, *a) else { return Ok(failure(cx, 2)) };
        for s in fs {
            let mut at = cx.attributes(s);
            for &n in &ns {
                match with(at, &h.sym_name(n), self.set) {
                    Some(b) => at = b,
                    None => return Ok(failure(cx, 2)),
                }
            }
            cx.set_attributes(s, at)?;
        }
        Ok(Reply::Done(h.sym_term(h.sym("null"))))
    }
}

/// Register `attributes`, `set_attributes` and `clear_attributes`.
pub(crate) fn install(k: &mut Kernel, h: &Heap) {
    let held = Attributes { hold: Hold::All, effect: true, ..Attributes::default() };
    k.register(h.sym("attributes"), Attributes { hold: Hold::All, ..Attributes::default() }, Box::new(AttributesOf));
    k.register(h.sym("set_attributes"), held, Box::new(SetAttributes { set: true }));
    k.register(h.sym("clear_attributes"), held, Box::new(SetAttributes { set: false }));
}
