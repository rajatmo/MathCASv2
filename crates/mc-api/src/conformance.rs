//! Checks every [`Store`] implementation must pass. Call [`check`] from a test of the implementing crate.
use crate::{Store, View};

/// Run all conformance checks on a fresh store. Panics with a description on the first violation.
pub fn check<S: Store>(st: &S) {
    identity(st);
    integers(st);
    reals(st);
    compounds(st);
    concurrent(st);
}

fn identity<S: Store>(st: &S) {
    let f = st.sym("f");
    assert_eq!(st.sym("f"), f, "a symbol name maps to one symbol");
    assert_eq!(st.sym_name(f), "f");
    let x = st.sym_term(st.sym("x"));
    let a = st.call(f, &[x, st.int(1)]);
    let b = st.call(f, &[st.sym_term(st.sym("x")), st.int(1)]);
    assert_eq!(a, b, "equal terms are the same handle");
    assert_eq!(st.structural_hash(a), st.structural_hash(b));
    assert_ne!(a, st.call(f, &[x, st.int(2)]));
    assert_ne!(st.int(1), st.real(1.0), "1 and 1.0 are distinct terms (semantics 1.1)");
    assert_eq!(st.string("abc"), st.string("abc"));
    assert!(matches!(st.view(st.string("abc")), View::Str("abc")));
}

fn integers<S: Store>(st: &S) {
    for v in [0i64, 1, -1, 1 << 29, -(1 << 29), 1 << 30, i64::MAX, i64::MIN] {
        assert!(matches!(st.view(st.int(v)), View::Int(w) if w == v), "int {v} round trips");
    }
    // canonical: a value that fits is always the small form
    assert_eq!(st.big_int(false, &[7, 0, 0]), st.int(7));
    assert_eq!(st.big_int(true, &[7]), st.int(-7));
    assert_eq!(st.big_int(false, &[]), st.int(0));
    assert_eq!(st.big_int(true, &[1u64 << 63]), st.int(i64::MIN));
    let big = st.big_int(false, &[1, 2]);
    assert!(matches!(st.view(big), View::BigInt { neg: false, mag: [1, 2] }));
    assert_eq!(big, st.big_int(false, &[1, 2, 0]), "big integers are hash-consed by value");
    let over = st.big_int(false, &[1u64 << 63]);
    assert!(matches!(st.view(over), View::BigInt { neg: false, mag: [m] } if *m == 1u64 << 63));
    let r = st.rational(st.int(1), st.int(3));
    assert!(matches!(st.view(r), View::Rational { .. }));
    assert_eq!(r, st.rational(st.int(1), st.int(3)));
}

fn reals<S: Store>(st: &S) {
    assert_eq!(st.real(1.5), st.real(1.5));
    assert_ne!(st.real(0.0), st.real(-0.0), "reals are identified by bit pattern");
    assert!(matches!(st.view(st.real(2.5)), View::Real(v) if v == 2.5));
    let b = st.ball(false, &[0, 1, -60i64 as u64, 3]);
    assert_eq!(b, st.ball(false, &[0, 1, -60i64 as u64, 3]));
    assert!(matches!(st.view(b), View::Ball { neg: false, words: [0, 1, _, 3] }));
}

fn compounds<S: Store>(st: &S) {
    let g = st.sym("g");
    let x = st.sym_term(st.sym("x"));
    // any term may be a head
    let gx = st.call(g, &[x]);
    let curried = st.app(gx, &[st.int(2)]);
    match st.view(curried) {
        View::App { head, args } => {
            assert_eq!(head, gx);
            assert_eq!(args, &[st.int(2)]);
        }
        _ => panic!("compound view"),
    }
    let empty = st.call(st.sym("list"), &[]);
    assert!(matches!(st.view(empty), View::App { args: [], .. }));
    // deep terms must not need a deep stack in the store
    let mut t = x;
    for _ in 0..100_000 {
        t = st.call(g, &[t]);
    }
    assert!(matches!(st.view(t), View::App { .. }));
}

fn concurrent<S: Store>(st: &S) {
    // many threads building overlapping terms get the same handles
    let ids: Vec<Vec<u32>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..4)
            .map(|_| {
                sc.spawn(|| {
                    let h = st.sym("h");
                    (0..2000).map(|i| st.call(h, &[st.int(i % 500), st.int(i % 7)]).raw()).collect::<Vec<u32>>()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for w in &ids[1..] {
        assert_eq!(w, &ids[0], "concurrent construction is canonical");
    }
}
