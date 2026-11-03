//! Handles: canonical construction across threads (including very wide terms, A.8), stale handles, deep terms.
use mc_core::{Config, Heap, Roots, Session, Store, View, T};
use rayon::prelude::*;

/// I1.8: the same very wide list built many times on a pool must get one handle (the parallel hash combiner carries
/// the combined length), and the same structural hash as a sequential build.
#[test]
fn wide_terms_are_canonical() {
    let mut s = Session::new(Config::default(), 4, Roots::new());
    let list = s.heap.sym("list");
    let seq = {
        let h = Heap::new(Config::default());
        let xs: Vec<T> = (0..200_000).map(|i| h.int(i)).collect();
        h.structural_hash(h.call(h.sym("list"), &xs))
    };
    s.step(|h: &Heap, keep: &mut Roots| {
        let xs: Vec<T> = (0..200_000).map(|i| h.int(i)).collect();
        let ids: Vec<T> = (0..21).into_par_iter().map(|_| h.call(list, &xs)).collect();
        assert!(ids.iter().all(|&t| t == ids[0]));
        assert_eq!(h.structural_hash(ids[0]), seq);
        keep.push(ids[0]);
    });
    s.collect_full();
    s.step(|h: &Heap, keep: &mut Roots| {
        let xs: Vec<T> = (0..200_000).map(|i| h.int(i)).collect();
        assert_eq!(h.call(list, &xs), keep.get(h, 0), "rebuilt after promotion");
    });
}

/// A handle that was not rooted across a minor collection finds a removed segment and panics; it never reads
/// another term (young ids are not reused).
#[test]
#[should_panic(expected = "use of a collected young term")]
fn stale_young_handle_panics() {
    let mut h = Heap::new(Config::default());
    let f = h.sym("f");
    let bits = h.call(f, &[h.int(1)]).raw();
    h.collect_minor(&mut []);
    let _ = h.view(T::from_raw(bits));
}

/// An old id beyond the region after a major collection is caught by the bounds check.
#[test]
#[should_panic]
fn moved_away_old_handle_panics() {
    let mut s = Session::new(Config::default(), 1, Roots::new());
    let f = s.heap.sym("f");
    s.step(|h: &Heap, keep: &mut Roots| {
        for i in 0..100 {
            keep.push(h.call(f, &[h.int(i)]));
        }
    });
    s.collect_full();
    let last = s.read(|h, keep| keep.get(h, 99).raw());
    s.state.clear();
    s.collect_full();
    let _ = s.heap.view(T::from_raw(last));
}

/// Deep terms are promoted and compacted without deep recursion.
#[test]
fn deep_terms_survive_collection() {
    let mut s = Session::new(Config::stress(), 2, Roots::new());
    let g = s.heap.sym("g");
    s.step(|h: &Heap, keep: &mut Roots| {
        let mut t = h.int(0);
        for _ in 0..200_000 {
            t = h.call(g, &[t]);
        }
        keep.push(t);
    });
    s.collect_full();
    s.read(|h, keep| {
        let mut t = keep.get(h, 0);
        let mut depth = 0;
        while let View::App { args, .. } = h.view(t) {
            t = args[0];
            depth += 1;
        }
        assert_eq!(depth, 200_000);
    });
}

/// Per-thread id chunks break creation order: a parent built on one thread can get a smaller id than a child built
/// later on another. Such out-of-order edges are marked with the explicit stack (I1.2), at the minor collection and
/// again at the major one, and the terms survive intact.
#[test]
fn out_of_order_edges_survive_collection() {
    let mut h = Heap::new(Config::stress());
    let (f, g) = (h.sym("f"), h.sym("g"));
    let mut keep = Roots::new();
    {
        let hr = &h;
        let _ = hr.call(f, &[hr.int(0)]); // this thread's id chunk is reserved first
        let child = std::thread::scope(|sc| sc.spawn(|| hr.call(g, &[hr.int(1)]).raw()).join().unwrap());
        let child = T::from_raw(child);
        // the parent comes from this thread's earlier chunk: a smaller id than its child
        let parent = hr.call(f, &[hr.call(f, &[child]), child]);
        assert!(parent.raw() < child.raw());
        keep.push(parent);
    }
    h.verify(&mut [&mut keep]);
    h.collect_full(&mut [&mut keep]);
    h.verify(&mut [&mut keep]);
    assert!(h.gc_stats().out_of_order >= 2, "minor and major marking met the edge");
    let t = keep.get(&h, 0);
    let child = h.call(g, &[h.int(1)]);
    assert_eq!(t, h.call(f, &[h.call(f, &[child]), child]));
    assert_eq!(h.old_len(), 3);
}
