//! Scratch scopes (decisions G.1 to G.6): terms made in a scope are private to it and dropped when it ends, shared
//! terms are found rather than copied, handles stay canonical, the memo and unpacked arguments stay private, stale
//! handles are caught, and many scopes in one evaluation do not grow the heap.
use mc_core::{detached, scratch_active, Config, Heap, Roots, Session, Store, View, PACK_MIN, T};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn session(cfg: Config) -> Session<Roots> {
    Session::new(cfg, 2, Roots::new())
}

/// `f(g(k, 1.5), [k, k + 1])` for an integer `k`.
fn term<'h>(h: &'h Heap, k: i64) -> T<'h> {
    let (f, g, list) = (h.sym("f"), h.sym("g"), h.sym("list"));
    let inner = h.call(g, &[h.int(k), h.real(1.5)]);
    let l = h.call(list, &[h.int(k), h.int(k + 1)]);
    h.call(f, &[inner, l])
}

#[test]
fn scope_terms_are_private_and_canonical() {
    let mut s = session(Config::default());
    s.step(|h: &Heap, keep: &mut Roots| {
        let shared = term(h, 1);
        keep.push(shared);
        let before = h.scratch_stats();
        {
            let sc = h.scratch().expect("a scope");
            assert!(scratch_active());
            // a term that exists is found, not copied
            assert_eq!(term(h, 1), shared);
            assert!(!h.is_scratch(term(h, 1)));
            // new terms are scratch terms, canonical within the scope
            let a = term(h, 7);
            assert!(h.is_scratch(a));
            assert_eq!(a, term(h, 7));
            assert_ne!(a, term(h, 8));
            assert_eq!(h.structural_hash(a), {
                let x = detached(|| term(h, 7));
                assert!(!h.is_scratch(x), "detached work builds shared terms");
                h.structural_hash(x)
            });
            match h.view(a) {
                View::App { head, args } => {
                    assert_eq!(head, h.sym_term(h.sym("f")));
                    assert_eq!(args.len(), 2);
                }
                _ => panic!("a compound"),
            }
            assert!(sc.terms() >= 3);
            // nested requests run inside the open scope
            assert!(h.scratch().is_none());
        }
        assert!(!scratch_active());
        let after = h.scratch_stats();
        assert_eq!(after.scopes, before.scopes + 1);
        assert!(after.terms > before.terms);
        assert!(after.declined > before.declined);
    });
}

#[test]
fn export_copies_into_the_shared_heap() {
    let mut s = session(Config::stress());
    s.step(|h: &Heap, keep: &mut Roots| {
        let (big, packed) = {
            let sc = h.scratch().expect("a scope");
            let big = h.big_int(true, &[7, 9, 11]);
            let x = h.call(h.sym("h"), &[term(h, 40), h.rational(h.int(3), h.int(7)), h.string("scratch"), big]);
            let v: Vec<f64> = (0..2 * PACK_MIN).map(|i| i as f64 * 0.25).collect();
            let p = h.reals(&v);
            assert!(h.is_scratch(x) && h.is_scratch(p));
            let (ex, ep) = (sc.export(x), sc.export(p));
            assert!(!h.is_scratch(ex) && !h.is_scratch(ep));
            // exporting twice gives the same shared handle
            assert_eq!(sc.export(x), ex);
            (ex, ep)
        };
        // after the scope the same terms built again are the exported handles
        let x = h.call(h.sym("h"), &[term(h, 40), h.rational(h.int(3), h.int(7)), h.string("scratch"), h.big_int(true, &[7, 9, 11])]);
        assert_eq!(big, x);
        let v: Vec<f64> = (0..2 * PACK_MIN).map(|i| i as f64 * 0.25).collect();
        assert_eq!(packed, h.reals(&v));
        keep.push(big);
        keep.push(packed);
    });
    s.collect_full();
    s.verify();
}

#[test]
fn stale_handles_panic() {
    let mut s = session(Config::default());
    s.step(|h: &Heap, _: &mut Roots| {
        let stale = {
            let _sc = h.scratch().expect("a scope");
            term(h, 99)
        };
        let r = catch_unwind(AssertUnwindSafe(|| h.view(stale)));
        assert!(r.is_err(), "a handle used after its scope panics");
        // the next scope on the slot uses other pages: the old handle is still recognised
        let _sc = h.scratch().expect("a scope");
        let _ = term(h, 5);
        assert!(catch_unwind(AssertUnwindSafe(|| h.view(stale))).is_err());
        // another thread cannot read a scope's terms
        let mine = term(h, 123);
        let other = std::thread::scope(|t| t.spawn(|| catch_unwind(AssertUnwindSafe(|| h.structural_hash(mine)))).join());
        assert!(other.expect("joined").is_err());
    });
}

#[test]
fn memo_entries_stay_private() {
    let mut s = session(Config::default());
    s.step(|h: &Heap, keep: &mut Roots| {
        let key = term(h, 1);
        let old_value = term(h, 2);
        keep.push(key);
        keep.push(old_value);
        let k2 = term(h, 3);
        keep.push(k2);
        {
            let _sc = h.scratch().expect("a scope");
            let v = term(h, 50);
            h.memo_put(key, v, 1, 0);
            assert_eq!(h.memo_get(key).map(|m| m.value), Some(v));
            let sk = term(h, 51);
            h.memo_put(sk, sk, 1, 0);
            assert_eq!(h.memo_get(sk).map(|m| m.value), Some(sk));
            // a shared value equal to a scratch term (made by "another task" after it) is not handed to the scope
            let twin = detached(|| {
                let t = term(h, 50);
                h.memo_put(k2, t, 1, 0);
                t
            });
            assert!(!h.is_scratch(twin));
            assert_ne!(twin, v);
            assert!(h.memo_get(k2).is_none(), "a twin of a scratch term is a miss");
            // a shared value unrelated to the scope is a hit
            h.memo_put(old_value, old_value, 1, 0);
            assert_eq!(h.memo_get(old_value).map(|m| m.value), Some(old_value));
        }
        // the private entries are gone; the shared key never saw the scratch value
        assert!(h.memo_get(key).is_none());
        assert_eq!(h.memo_get(k2).map(|m| m.value), Some(term(h, 50)));
    });
}

#[test]
fn packed_arguments_are_private_in_a_scope() {
    let mut s = session(Config::default());
    s.step(|h: &Heap, keep: &mut Roots| {
        let v: Vec<f64> = (0..4 * PACK_MIN).map(|i| (i as f64).sqrt()).collect();
        let shared = h.reals(&v);
        keep.push(shared);
        {
            let _sc = h.scratch().expect("a scope");
            // a leaf value made in the scope first, then the view: the view gives the scope's handle
            let leaf = h.real(v[5]);
            assert!(h.is_scratch(leaf));
            let View::App { args, .. } = h.view(shared) else { panic!("a list") };
            assert_eq!(args[5], leaf);
        }
        // outside, the shared cache is unaffected and shared
        let View::App { args, .. } = h.view(shared) else { panic!("a list") };
        assert!(!h.is_scratch(args[5]));
        assert_eq!(args[5], h.real(v[5]));
    });
}

#[test]
fn many_scopes_keep_the_heap_small() {
    let mut s = session(Config::default());
    s.step(|h: &Heap, _: &mut Roots| {
        let base = h.sym("base");
        let run = |round: i64| {
            let _sc = h.scratch().expect("a scope");
            let mut acc = h.int(round);
            for i in 0..200 {
                acc = h.call(base, &[acc, h.real(i as f64 + round as f64 * 0.5)]);
            }
            h.structural_hash(acc)
        };
        let first: Vec<u64> = (0..10).map(run).collect();
        for r in 0..20_000 {
            run(r);
        }
        // the same work gives the same structural results (nothing leaked into later scopes)
        let again: Vec<u64> = (0..10).map(run).collect();
        assert_eq!(first, again);
    });
    let st = s.heap.scratch_stats();
    assert!(st.scopes >= 20_020 && st.largest >= 400, "{st:?}");
    // 4 million terms were made; the shared heap holds none of them
    assert!(s.heap.stored() < 1000, "stored {}", s.heap.stored());
    assert!(s.heap.reserved_bytes() < 64 << 20, "reserved {}", s.heap.reserved_bytes());
}

#[test]
fn scopes_on_parallel_tasks_are_independent() {
    for cfg in [Config::default(), Config::stress()] {
        let mut s = Session::new(cfg, 4, Roots::new());
        let hashes = s.step(|h: &Heap, keep: &mut Roots| {
            let hs: Vec<u64> = rayon_map(64, |i| {
                let _sc = h.scratch();
                let mut acc = h.int(i as i64);
                for j in 0..300 {
                    acc = h.call(h.sym("p"), &[acc, h.real(j as f64)]);
                }
                h.structural_hash(acc)
            });
            keep.push(term(h, 1));
            hs
        });
        let reference: Vec<u64> = s.step(|h: &Heap, _: &mut Roots| {
            (0..64)
                .map(|i| {
                    let mut acc = h.int(i as i64);
                    for j in 0..300 {
                        acc = h.call(h.sym("p"), &[acc, h.real(j as f64)]);
                    }
                    h.structural_hash(acc)
                })
                .collect()
        });
        assert_eq!(hashes, reference);
        s.collect_full();
        s.verify();
    }
}

/// `f(0..n)` on the pool, each call detached as the evaluator's jobs are.
fn rayon_map<R: Send>(n: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    use rayon::prelude::*;
    (0..n).into_par_iter().map(|i| detached(|| f(i))).collect()
}

#[test]
fn scopes_off_by_config() {
    let mut s = session(Config { scratch: false, ..Config::default() });
    s.step(|h: &Heap, _: &mut Roots| {
        assert!(h.scratch().is_none());
        assert!(!h.is_scratch(term(h, 3)));
    });
}
