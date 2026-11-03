//! The contract's conformance checks (mc-api) against the real store, in several states.
use mc_core::{Config, Heap, Roots, Session, Store};

#[test]
fn conformance_fresh_heap() {
    mc_api::conformance::check(&Heap::new(Config::default()));
}

#[test]
fn conformance_without_collection() {
    mc_api::conformance::check(&Heap::new(Config::no_collect()));
}

#[test]
fn conformance_on_a_pool() {
    // on a pool thread, wide compounds hash in parallel and collections copy on the pool
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().unwrap();
    pool.install(|| mc_api::conformance::check(&Heap::new(Config::default())));
}

/// After collections the old region is populated: the checks' lookups then go through the old table, and terms
/// rebuilt from scratch must find the promoted ones.
#[test]
fn conformance_with_an_old_region() {
    let mut s = Session::new(Config::stress(), 4, Roots::new());
    s.step(|h: &Heap, keep: &mut Roots| {
        mc_api::conformance::check(h);
        let f = h.sym("f");
        let x = h.sym_term(h.sym("x"));
        keep.push(h.call(f, &[x, h.int(1)]));
        keep.push(h.big_int(false, &[1, 2]));
        keep.push(h.string("abc"));
        keep.push(h.ball(false, &[0, 1, -60i64 as u64, 3]));
        keep.push(h.rational(h.int(1), h.int(3)));
    });
    s.collect_full();
    assert!(s.heap.old_len() >= 5);
    let old = s.heap.old_len();
    s.step(|h: &Heap, _: &mut Roots| mc_api::conformance::check(h));
    s.read(|h, keep| {
        let f = h.sym("f");
        assert_eq!(keep.get(h, 0), h.call(f, &[h.sym_term(h.sym("x")), h.int(1)]));
        assert_eq!(keep.get(h, 1), h.big_int(false, &[1, 2, 0]));
        assert_eq!(keep.get(h, 2), h.string("abc"));
        assert_eq!(keep.get(h, 4), h.rational(h.int(1), h.int(3)));
    });
    s.collect_full();
    assert_eq!(s.heap.old_len(), old, "the second round made no new live terms");
}
