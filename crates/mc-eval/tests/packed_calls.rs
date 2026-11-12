//! Calls with a packed argument do not unpack it: argument-head dispatch and the rule index read a packed array's
//! head and length from its payload.
use mc_core::{Config, Heap, Store, View};
use mc_eval::pat::{call, var};
use mc_eval::{session, EvalConfig, Rhs};
use std::sync::Arc;

#[test]
fn calls_with_a_packed_argument_do_not_unpack_it() {
    const N: usize = 1_000_000;
    let mut s = session(Config::no_collect(), 1, EvalConfig::default());
    s.step(|h, k| {
        let one = h.sym("one");
        let rhs: Rhs = Arc::new(|_, h: &Heap| h.int(1));
        k.define(h, call(h, one, vec![var(0)]), rhs).expect("rule");
    });
    let before = s.heap.reserved_bytes();
    let got = s.step(|h, k| {
        let v = h.reals(&(0..N).map(|i| i as f64).collect::<Vec<_>>());
        k.roots.push(v);
        let named = k.eval(h, h.call(h.sym("one"), &[v])).value;
        let undefined = k.eval(h, h.call(h.sym("g"), &[v])).value;
        (matches!(h.view(named), View::Int(1)), h.packed(undefined).is_none() && matches!(h.view(undefined), View::App { .. }))
    });
    assert_eq!(got, (true, true));
    let grown = s.heap.reserved_bytes() - before;
    // the payload (8 bytes a leaf) and small change; unpacking would add a node and a slot per leaf
    assert!(grown < N * 8 + (4 << 20), "grew {grown} bytes");
}
