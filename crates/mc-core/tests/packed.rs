//! Packed arrays in the real store (semantics 11, decisions N4a.1 to N4a.4): the payload survives minor and major
//! collections, unpacked arguments are a per-cycle cache, the memo keys and values may be packed, construction and
//! unpacking are canonical across threads, and the payload costs 8 bytes per leaf.
use mc_core::{Config, Heap, Packed, Roots, Session, Store, View, PACK_MIN, T};
use rayon::prelude::*;

fn reals(n: usize, seed: f64) -> Vec<f64> {
    (0..n).map(|i| (i as f64 * 0.37 + seed).sin()).collect()
}

/// The leaves of a packed real vector.
fn leaves<'h>(h: &'h Heap, t: T<'h>) -> &'h [f64] {
    h.packed(t).and_then(|p| p.real_vector()).expect("a packed real vector")
}

/// The view's arguments, as the store's own element terms.
fn args<'h>(h: &'h Heap, t: T<'h>) -> &'h [T<'h>] {
    match h.view(t) {
        View::App { args, .. } => args,
        _ => panic!("a packed array views as a compound term"),
    }
}

#[test]
fn payloads_survive_minor_and_major_collections() {
    for cfg in [Config::default(), Config::stress()] {
        let mut s = Session::new(cfg, 4, Roots::new());
        let f = s.heap.sym("f");
        let (v, m) = (reals(5000, 0.0), reals(3 * PACK_MIN, 1.0));
        s.step(|h: &Heap, keep: &mut Roots| {
            let a = h.reals(&v);
            // viewed before the collection: the unpacked arguments are young terms
            assert_eq!(args(h, a).len(), 5000);
            keep.push(a);
            keep.push(h.real_array(&[3, PACK_MIN], &m));
            keep.push(h.call(f, &[a, h.ints(&(0..100).collect::<Vec<i64>>())]));
            // garbage: packed arrays that die young free their boxes
            for k in 0..20 {
                h.reals(&reals(PACK_MIN + k, k as f64));
            }
        });
        for round in 0..3 {
            if round == 1 {
                s.collect_full();
            }
            s.verify();
            s.step(|h: &Heap, keep: &mut Roots| {
                let a = keep.get(h, 0);
                assert_eq!(leaves(h, a), &v[..]);
                assert_eq!(a, h.reals(&v), "still the canonical handle");
                let xs = args(h, a);
                assert!(xs.iter().zip(&v).all(|(&x, &y)| matches!(h.view(x), View::Real(r) if r.to_bits() == y.to_bits())));
                let m2 = keep.get(h, 1);
                let rows = args(h, m2);
                assert_eq!(rows.len(), 3);
                for (i, &r) in rows.iter().enumerate() {
                    assert_eq!(r, h.reals(&m[i * PACK_MIN..(i + 1) * PACK_MIN]), "row {i} is the packed row");
                }
                let View::App { args: fa, .. } = h.view(keep.get(h, 2)) else { panic!("f(...)") };
                assert_eq!(fa[0], a);
                assert!(matches!(h.packed(fa[1]), Some(Packed::Ints { .. })));
                // new garbage each round
                h.reals(&reals(700, round as f64));
            });
        }
        s.state.clear();
        s.collect_full();
        assert_eq!(s.heap.old_len(), 0, "unrooted packed arrays are collected");
        assert_eq!(s.heap.boxed_bytes(), 0, "and their payloads freed");
    }
}

#[test]
fn memo_keys_and_values_may_be_packed() {
    let mut s = Session::new(Config::default(), 4, Roots::new());
    let f = s.heap.sym("f");
    let v = reals(1000, 2.0);
    s.step(|h: &Heap, keep: &mut Roots| {
        let k = h.call(f, &[h.int(1)]);
        let a = h.reals(&v);
        h.memo_put(k, a, 3, 1);
        h.memo_put(a, a, 3, 0);
        keep.push(k);
        keep.push(a);
    });
    s.collect_full();
    s.step(|h: &Heap, keep: &mut Roots| {
        let (k, a) = (keep.get(h, 0), keep.get(h, 1));
        assert_eq!(h.memo_get(k).unwrap().value, a, "a packed value moves with its key");
        assert_eq!(h.memo_get(a).unwrap().value, a, "a packed key moves with its entry");
        // an old packed key with a young value goes through the side memo
        let y = h.call(f, &[h.int(2)]);
        h.memo_put(a, y, 4, 0);
        keep.push(y);
    });
    s.collect_full();
    s.read(|h, keep| assert_eq!(h.memo_get(keep.get(h, 1)).unwrap().value, keep.get(h, 2)));
}

#[test]
fn construction_and_unpacking_are_canonical_across_threads() {
    let s = Session::new(Config::default(), 4, Roots::new());
    let v = reals(100_000, 3.0);
    s.read(|h, _| {
        let list = h.sym("list");
        // the same list by three routes on many tasks
        let hs: Vec<u32> = (0..24)
            .into_par_iter()
            .map(|k| match k % 3 {
                0 => h.reals(&v).raw(),
                1 => {
                    let xs: Vec<T<'_>> = v.iter().map(|&x| h.real(x)).collect();
                    h.call(list, &xs).raw()
                }
                _ => h.real_array(&[1000, 100], &v).raw(),
            })
            .collect();
        let (a, m) = (hs[0], hs[2]);
        assert!(hs.iter().enumerate().all(|(k, &x)| x == if k % 3 == 2 { m } else { a }));
        let a = T::from_raw(a);
        // views raced on many tasks (parallel unpacking) give one slice of canonical handles
        let views: Vec<usize> = (0..8).into_par_iter().map(|_| args(h, a).as_ptr() as usize).collect();
        assert!(views.iter().all(|&p| p == views[0]), "one cached slice");
        let seq: Vec<T<'_>> = v.iter().map(|&x| h.real(x)).collect();
        assert_eq!(args(h, a), &seq[..]);
        // the matrix's rows are packed rows; its leaves are the vector's
        let m = T::from_raw(m);
        assert_eq!(h.packed(m).unwrap().reals(), Some(&v[..]));
        assert_eq!(h.structural_hash(h.call(list, args(h, m))), h.structural_hash(m));
    });
}

#[test]
fn a_packed_payload_costs_eight_bytes_per_leaf() {
    let mut s = Session::new(Config::default(), 1, Roots::new());
    let n = 1_000_000;
    let ints: Vec<i64> = (0..n as i64).map(|i| i * 1_000_003).collect();
    s.step(|h: &Heap, keep: &mut Roots| {
        keep.push(h.reals(&reals(n, 0.0)));
        keep.push(h.ints(&ints));
    });
    s.collect_full();
    let boxed = s.heap.boxed_bytes();
    assert!((16 * n..16 * n + 64).contains(&boxed), "two payloads of {n} words: {boxed} bytes");
    assert!(s.heap.reserved_bytes() < 16 * n + (16 << 20), "no heap cell per leaf");
    assert!(s.heap.old_len() <= 2);
}

#[test]
fn integers_cover_every_tier_and_mixed_lists_stay_compound() {
    let s = Session::new(Config::default(), 1, Roots::new());
    s.read(|h, _| {
        let list = h.sym("list");
        let vs: Vec<i64> = (0..200).map(|i| match i % 4 {
            0 => i,
            1 => -(1 << 29) - i,
            2 => i64::MAX - i,
            _ => i64::MIN + i,
        }).collect();
        let a = h.ints(&vs);
        let elems: Vec<T<'_>> = vs.iter().map(|&v| h.int(v)).collect();
        assert_eq!(a, h.call(list, &elems));
        assert_eq!(args(h, a), &elems[..]);
        assert_eq!(h.packed(a).unwrap().int_vector(), Some(&vs[..]));
        let mut mixed = elems.clone();
        mixed[7] = h.real(7.0);
        let t = h.call(list, &mixed);
        assert!(h.packed(t).is_none());
        assert_eq!(args(h, t), &mixed[..]);
    });
}

#[test]
fn complex_payloads_survive_collections_and_unpack_canonically() {
    for cfg in [Config::default(), Config::stress()] {
        let mut s = Session::new(cfg, 4, Roots::new());
        let parts: Vec<f64> = reals(2 * 3000, 0.5);
        s.step(|h: &Heap, keep: &mut Roots| {
            let z = h.complexes(&parts);
            assert!(matches!(h.packed(z), Some(Packed::Complexes { data, .. }) if data == &parts[..]));
            keep.push(z);
        });
        s.collect_full();
        s.verify();
        s.step(|h: &Heap, keep: &mut Roots| {
            let z = keep.get(h, 0);
            assert_eq!(h.packed(z).map(|p| p.count()), Some(3000));
            let complex = h.sym("complex");
            let xs = args(h, z);
            assert_eq!(xs[7], h.call(complex, &[h.real(parts[14]), h.real(parts[15])]));
            // built element by element: the same handle
            assert_eq!(h.call(h.sym("list"), xs), z);
            assert_eq!(z, h.complexes(&parts), "still the canonical handle");
        });
        s.verify();
    }
}
