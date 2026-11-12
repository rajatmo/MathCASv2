//! Number payloads (I3): canonical integer tiers, views, placement, payload accounting and freeing (A.3, A.4), and
//! identity across collections.
use mc_core::{Config, Heap, Roots, Session, Store, View, SMALL_MAX, SMALL_MIN, T};

fn big(h: &Heap, words: usize, seed: u64) -> T<'_> {
    let w: Vec<u64> = (0..words as u64).map(|i| (i + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ seed).collect();
    h.big_int(false, &w)
}

#[test]
fn integer_tiers_are_canonical() {
    let h = Heap::new(Config::default());
    // immediates at the edges, one-limb words beyond them; both view as machine integers
    for v in [SMALL_MIN, SMALL_MAX, SMALL_MIN - 1, SMALL_MAX + 1, i64::MAX, i64::MIN, -(1 << 40)] {
        let t = h.int(v);
        assert!(matches!(h.view(t), View::Int(w) if w == v), "{v}");
        let m = v.unsigned_abs();
        assert_eq!(h.big_int(v < 0, &[m, 0, 0]), t, "big_int demotes {v}");
    }
    assert_eq!(h.big_int(true, &[0, 0]), h.int(0), "no negative zero");
    // one limb beyond i64: still a word node, viewed as a big integer
    let w = h.big_int(false, &[u64::MAX]);
    assert!(matches!(h.view(w), View::BigInt { neg: false, mag: [m] } if *m == u64::MAX));
    let w = h.big_int(true, &[(1 << 63) + 1]);
    assert!(matches!(h.view(w), View::BigInt { neg: true, mag: [m] } if *m == (1 << 63) + 1));
    // two limbs and more: payloads, hash-consed by value
    let p = h.big_int(true, &[5, 7]);
    assert!(matches!(h.view(p), View::BigInt { neg: true, mag: [5, 7] }));
    assert_eq!(p, h.big_int(true, &[5, 7, 0]));
    assert_ne!(p, h.big_int(false, &[5, 7]));
    assert_eq!(h.structural_hash(p), h.structural_hash(h.big_int(true, &[5, 7])));
}

#[test]
fn rationals_reals_balls_and_strings() {
    let h = Heap::new(Config::default());
    let r = h.rational(h.int(-1), h.int(4));
    assert!(matches!(h.view(r), View::Rational { num, den } if num == h.int(-1) && den == h.int(4)));
    let big_den = h.big_int(false, &[0, 1]);
    let r2 = h.rational(h.int(3), big_den);
    assert_eq!(r2, h.rational(h.int(3), h.big_int(false, &[0, 1])));
    assert_ne!(r, r2);
    // reals are identified by bit pattern
    let nan1 = h.real(f64::from_bits(0x7FF8_0000_0000_0001));
    assert_eq!(nan1, h.real(f64::from_bits(0x7FF8_0000_0000_0001)));
    assert_ne!(nan1, h.real(f64::from_bits(0x7FF8_0000_0000_0002)));
    assert_ne!(h.real(0.0), h.real(-0.0));
    // balls: sign and words
    let b = h.ball(true, &[1, 2, 3, 4, 5]);
    assert!(matches!(h.view(b), View::Ball { neg: true, words: [1, 2, 3, 4, 5] }));
    assert_ne!(b, h.ball(false, &[1, 2, 3, 4, 5]));
    // strings: empty, non-ASCII, word-boundary lengths, long (boxed)
    let long: String = "\u{2211}\u{e9}".repeat(400);
    for s in ["", "a", "abcdefgh", "abcdefghi", "\u{2211} \u{e9}", long.as_str()] {
        let t = h.string(s);
        assert!(matches!(h.view(t), View::Str(v) if v == s), "{s:?}");
        assert_eq!(t, h.string(s));
    }
    assert_ne!(h.string("ab"), h.string("ab\0"), "length is part of a string's identity");
}

#[test]
fn payload_placement_and_accounting() {
    let h = Heap::new(Config::default());
    // at most 64 words in the limb arena, longer payloads boxed
    let _ = big(&h, 64, 1);
    assert_eq!(h.boxed_bytes(), 0);
    let _ = big(&h, 65, 1);
    assert_eq!(h.boxed_bytes(), 65 * 8);
    let _ = big(&h, 65, 1);
    assert_eq!(h.boxed_bytes(), 65 * 8, "an equal payload is not stored twice");
    let _ = h.string(&"x".repeat(65 * 8 + 1));
    assert_eq!(h.boxed_bytes(), 65 * 8 + 66 * 8);
}

/// A.3: boxed payloads of dead young nodes are freed at the minor collection, survivors' boxes change owner; dead
/// old ones are freed at the major collection.
#[test]
fn boxed_payloads_are_freed() {
    let mut s = Session::new(Config::stress(), 4, Roots::new());
    s.step(|h: &Heap, keep: &mut Roots| {
        use rayon::prelude::*;
        let v: Vec<_> = (0..100u64).into_par_iter().map(|i| big(h, 100, i)).collect();
        for t in &v[..10] {
            keep.push(*t);
        }
    });
    assert_eq!(s.heap.boxed_bytes(), 10 * 800);
    assert_eq!(s.heap.gc_stats().freed_boxes, 90);
    let want: Vec<u64> = s.read(|h, keep| (0..10).map(|i| h.structural_hash(keep.get(h, i))).collect());
    // identity across promotion: rebuilding gives the kept handle and the same structural hash
    s.read(|h, keep| {
        for (i, &w) in want.iter().enumerate() {
            assert_eq!(keep.get(h, i), big(h, 100, i as u64));
            assert_eq!(h.structural_hash(keep.get(h, i)), w);
        }
    });
    s.state.truncate(3);
    s.collect_full();
    assert_eq!(s.heap.boxed_bytes(), 3 * 800);
    assert_eq!(s.heap.old_len(), 3);
    s.state.clear();
    s.collect_full();
    assert_eq!(s.heap.boxed_bytes(), 0);
    assert_eq!(s.heap.old_len(), 0);
    s.verify();
}

/// The nursery trigger counts payload bytes (I3.3): a few huge numbers start collections as many nodes would.
#[test]
fn payload_bytes_trigger_collection() {
    let mut s = Session::new(Config::default(), 2, Roots::new());
    for i in 0..40u64 {
        s.step(|h: &Heap, _: &mut Roots| {
            let _ = big(h, 1 << 14, i); // 128 KB each, all garbage
        });
    }
    assert!(s.heap.gc_stats().minors >= 4, "about 5 MB of payloads in a 1 MB nursery");
    assert!(s.heap.boxed_bytes() <= 2 << 20);
}
