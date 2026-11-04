//! Checks every [`Store`] implementation must pass. Call [`check`] from a test of the implementing crate.
use crate::packed::{pack_shape, Elem, MAX_RANK, PACK_MIN};
use crate::{names, Packed, Store, View, T};

/// Run all conformance checks on a fresh store. Panics with a description on the first violation.
pub fn check<S: Store>(st: &S) {
    identity(st);
    integers(st);
    reals(st);
    compounds(st);
    concurrent(st);
    packed_arrays(st);
}

/// The checks of a store that packs ([`crate::packed`]): everything [`check`] does, and the packing rule exactly
/// (which lists have a payload). Panics with a description on the first violation.
pub fn check_packing<S: Store>(st: &S) {
    check(st);
    packing_rule(st);
    packed_concurrent(st);
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

/// Awkward machine reals: signed zeros, infinities, NaNs with payloads, subnormals, extremes.
fn awkward_reals(n: usize) -> Vec<f64> {
    let special = [
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        f64::from_bits(0x7FF0_0000_0000_0001),
        f64::from_bits(0xFFF8_0000_0000_00AB),
        f64::MIN_POSITIVE / 8.0,
        f64::MAX,
        f64::MIN,
        1.5,
    ];
    (0..n).map(|i| if i < special.len() { special[i] } else { i as f64 * 0.25 - 3.0 }).collect()
}

/// Machine integers across the store's tiers (immediates, one word, the `i64` extremes).
fn awkward_ints(n: usize) -> Vec<i64> {
    let special = [0, 1, -1, 1 << 29, -(1 << 29) - 1, 1 << 30, -(1 << 40), i64::MAX, i64::MIN, i64::MIN + 1];
    (0..n).map(|i| if i < special.len() { special[i] } else { (i as i64 - 40) * 7919 }).collect()
}

/// `t`'s view is `list(args...)` with exactly these arguments.
fn assert_list<S: Store>(st: &S, t: T<'_>, args: &[T<'_>], what: &str) {
    let list = st.sym_term(st.sym(names::LIST));
    match st.view(t) {
        View::App { head, args: a } => {
            assert_eq!(head.raw(), list.raw(), "{what}: head list");
            assert_eq!(a.len(), args.len(), "{what}: length");
            assert!(a.iter().zip(args).all(|(x, y)| x.raw() == y.raw()), "{what}: the arguments");
        }
        _ => panic!("{what}: a packed array views as a compound term"),
    }
}

/// A payload, when there is one, holds exactly the elements and dimensions.
fn assert_payload(p: Option<Packed<'_>>, elem: Elem, dims: &[u64], words: &[u64], what: &str) {
    let Some(p) = p else { return };
    assert_eq!(p.elem(), elem, "{what}: leaf kind");
    assert_eq!(p.dims(), dims, "{what}: dimensions");
    assert_eq!(p.count(), words.len(), "{what}: leaf count");
    let got: Vec<u64> = match p {
        Packed::Reals { data, .. } => data.iter().map(|v| v.to_bits()).collect(),
        Packed::Ints { data, .. } => data.iter().map(|&v| v as u64).collect(),
    };
    assert_eq!(got, words, "{what}: leaves");
}

/// Packed construction and element-wise construction give one handle, one hash and one view, at every length
/// around the threshold, for reals, integers and rectangular arrays; mixed, ragged and non-list terms have no
/// payload.
fn packed_arrays<S: Store>(st: &S) {
    let list = st.sym(names::LIST);
    for n in [0, 1, 2, PACK_MIN - 1, PACK_MIN, PACK_MIN + 1, 3 * PACK_MIN + 5, 1000] {
        let vs = awkward_reals(n);
        let elems: Vec<T<'_>> = vs.iter().map(|&v| st.real(v)).collect();
        let a = st.reals(&vs);
        let b = st.call(list, &elems);
        assert_eq!(a, b, "reals({n}) is the list built element by element");
        assert_eq!(st.structural_hash(a), st.structural_hash(b));
        assert_list(st, a, &elems, "reals");
        let words: Vec<u64> = vs.iter().map(|v| v.to_bits()).collect();
        assert_payload(st.packed(a), Elem::Real, &[n as u64], &words, "reals");
        // the same list inside another term
        let f = st.sym("f");
        assert_eq!(st.call(f, &[a, st.int(1)]), st.call(f, &[b, st.int(1)]));

        let is = awkward_ints(n);
        let elems: Vec<T<'_>> = is.iter().map(|&v| st.int(v)).collect();
        let a = st.ints(&is);
        assert_eq!(a, st.call(list, &elems), "ints({n}) is the list built element by element");
        assert_list(st, a, &elems, "ints");
        let words: Vec<u64> = is.iter().map(|&v| v as u64).collect();
        assert_payload(st.packed(a), Elem::Int, &[n as u64], &words, "ints");
    }
    // matrices and rank-3 arrays: the rows are the arrays of the rows' leaves
    for dims in [[3usize, PACK_MIN].as_slice(), &[PACK_MIN, 2], &[2, 3, 2 * PACK_MIN], &[4, 4, 4], &[2, 0], &[0, 3]] {
        let n: usize = dims.iter().product();
        let vs = awkward_reals(n);
        let a = st.real_array(dims, &vs);
        let row = n.checked_div(dims[0]).unwrap_or(0);
        let rows: Vec<T<'_>> = (0..dims[0]).map(|i| st.real_array(&dims[1..], &vs[i * row..(i + 1) * row])).collect();
        assert_eq!(a, st.call(list, &rows), "real_array({dims:?}) is the list of its rows");
        assert_list(st, a, &rows, "real_array");
        let words: Vec<u64> = vs.iter().map(|v| v.to_bits()).collect();
        let d: Vec<u64> = dims.iter().map(|&d| d as u64).collect();
        assert_payload(st.packed(a), Elem::Real, &d, &words, "real_array");
        let is = awkward_ints(n);
        let rows: Vec<T<'_>> = (0..dims[0]).map(|i| st.int_array(&dims[1..], &is[i * row..(i + 1) * row])).collect();
        assert_eq!(st.int_array(dims, &is), st.call(list, &rows), "int_array({dims:?}) is the list of its rows");
    }
    // mixed kinds, other heads, ragged rows and big integers have no payload
    let mut mixed: Vec<T<'_>> = awkward_reals(2 * PACK_MIN).iter().map(|&v| st.real(v)).collect();
    mixed[PACK_MIN] = st.int(3);
    assert!(st.packed(st.call(list, &mixed)).is_none(), "a list of reals and an integer is not packed");
    let reals: Vec<T<'_>> = mixed.iter().map(|_| st.real(2.0)).collect();
    assert!(st.packed(st.call(st.sym("g"), &reals)).is_none(), "only lists are packed");
    let r1 = st.reals(&awkward_reals(PACK_MIN));
    let r2 = st.reals(&awkward_reals(PACK_MIN + 1));
    let ragged = st.call(list, &[r1, r2]);
    assert!(st.packed(ragged).is_none(), "ragged rows are not packed");
    assert_list(st, ragged, &[r1, r2], "ragged");
    let mut ints: Vec<T<'_>> = (0..2 * PACK_MIN as i64).map(|i| st.int(i)).collect();
    ints[5] = st.big_int(false, &[1, 1]);
    assert!(st.packed(st.call(list, &ints)).is_none(), "an integer beyond i64 is not packed");
    let ri = st.call(list, &[r1, st.ints(&awkward_ints(PACK_MIN))]);
    assert!(st.packed(ri).is_none(), "rows of different leaf kinds are not packed");
}

/// The rule exactly: which lists carry a payload, against [`pack_shape`] on their views.
fn packing_rule<S: Store>(st: &S) {
    let list = st.sym(names::LIST);
    let row = |n: usize| -> Vec<T<'_>> { (0..n).map(|i| st.real(i as f64)).collect() };
    for n in [1, PACK_MIN - 1, PACK_MIN, 2 * PACK_MIN] {
        let t = st.call(list, &row(n));
        assert_eq!(st.packed(t).is_some(), n >= PACK_MIN, "a list of {n} reals");
        let t = st.ints(&awkward_ints(n));
        assert_eq!(st.packed(t).is_some(), n >= PACK_MIN, "a list of {n} integers");
    }
    // short rows that together reach the threshold, built from compound rows
    let short = st.call(list, &row(4));
    let m = st.call(list, &[short; PACK_MIN / 4]);
    let p = st.packed(m).expect("rows of 4 reals reaching the threshold");
    assert_eq!(p.dims(), &[(PACK_MIN / 4) as u64, 4]);
    let below = st.call(list, &[short; PACK_MIN / 4 - 1]);
    assert!(st.packed(below).is_none(), "below the threshold");
    // deep: rank MAX_RANK is packed, one more is not
    let mut t = st.reals(&[1.0; PACK_MIN]);
    for _ in 1..MAX_RANK {
        t = st.call(list, &[t]);
    }
    assert_eq!(st.packed(t).map(|p| p.rank()), Some(MAX_RANK));
    assert!(st.packed(st.call(list, &[t])).is_none(), "rank above the limit");
    // the reference rule agrees on the lists made above
    for t in [m, below, short, st.call(list, &[short, short]), t] {
        if let View::App { args, .. } = st.view(t) {
            assert_eq!(pack_shape(st, args).is_some(), st.packed(t).is_some(), "the reference rule");
        }
    }
}

/// Threads building the same packed arrays by different routes get the same handles.
fn packed_concurrent<S: Store>(st: &S) {
    let list = st.sym(names::LIST);
    let ids: Vec<Vec<u32>> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..4)
            .map(|k| {
                sc.spawn(move || {
                    (0..200)
                        .map(|i| {
                            let vs: Vec<f64> = (0..PACK_MIN + i % 7).map(|j| (i * j) as f64).collect();
                            if (i + k) % 2 == 0 {
                                st.reals(&vs).raw()
                            } else {
                                let elems: Vec<T<'_>> = vs.iter().map(|&v| st.real(v)).collect();
                                st.call(list, &elems).raw()
                            }
                        })
                        .collect::<Vec<u32>>()
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    for w in &ids[1..] {
        assert_eq!(w, &ids[0], "concurrent packed construction is canonical");
    }
}
