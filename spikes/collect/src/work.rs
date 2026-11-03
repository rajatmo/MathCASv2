//! Workloads, written once against [`Store`], each a sequence of "evaluations" separated by safe points. At every
//! safe point the workload passes the handles it still needs as roots and continues with the handles the collector
//! wrote back; it holds no other term handle across a safe point (symbol handles are always valid). Each returns its
//! phases' times, a store-independent fingerprint of everything it kept, and counters that must agree across stores.
//! S3, S8, R10 and R11 are the cases of spike 1 (02-benchmarks.md) with safe points added where a session would have
//! them; `long_session` is the synthetic session trace of spike 1b.
use crate::{fingerprint, Rng, Store, View};
use std::time::{Duration, Instant};

pub struct Out {
    pub phases: Vec<(&'static str, Duration)>,
    pub check: u64,
    /// counters that must agree across stores (memo misses, sizes)
    pub notes: Vec<(&'static str, u64)>,
}

fn timed<R>(phases: &mut Vec<(&'static str, Duration)>, name: &'static str, f: impl FnOnce() -> R) -> R {
    let t = Instant::now();
    let r = f();
    phases.push((name, t.elapsed()));
    r
}

fn parts<St: Store>(st: &St, t: &St::T) -> Option<(St::S, Vec<St::T>)> {
    match st.view(t) {
        View::App(h, a) => Some((h.clone(), a.to_vec())),
        _ => None,
    }
}

fn syms<St: Store>(st: &mut St, prefix: &str, n: usize) -> Vec<St::S> {
    (0..n).map(|i| st.sym(&format!("{prefix}{i}"))).collect()
}

/// Random trees over the given heads (arity 1..3) and leaves (small integers and the given symbols); spike 1's
/// generator.
fn gen_tree<St: Store>(st: &mut St, heads: &[St::S], leaves: &[St::S], rng: &mut Rng, size: usize) -> St::T {
    if size <= 1 {
        return if rng.below(2) == 0 {
            st.int(rng.below(20) as i64)
        } else {
            let s = leaves[rng.below(leaves.len() as u64) as usize].clone();
            st.sym_term(&s)
        };
    }
    let arity = (1 + rng.below(3) as usize).min(size - 1);
    let mut rest = size - 1;
    let mut args = vec![];
    for i in 0..arity {
        let k = if i + 1 == arity { rest } else { 1 + rng.below((rest - (arity - i - 1)) as u64) as usize };
        let k = k.max(1);
        rest -= k.min(rest);
        args.push(gen_tree(st, heads, leaves, rng, k));
    }
    let h = heads[rng.below(heads.len() as u64) as usize].clone();
    st.app(&h, args)
}

// ---------------------------------------------------------------------------------------------------------------
// S3: replace elements of a 1000 x (n/1000) integer matrix 10^4 times; each replacement is one evaluation, so a
// safe point follows it with the matrix as the only root. Every old row and old outer list becomes garbage.

pub fn s3_parts<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let list = st.sym("list");
    let (rows, cols) = (1000, n / 1000);
    let mut m = timed(&mut ph, "build", || {
        let rs: Vec<St::T> = (0..rows)
            .map(|i| {
                let row: Vec<St::T> = (0..cols).map(|j| st.int((i * cols + j) as i64)).collect();
                st.app(&list, row)
            })
            .collect();
        st.app(&list, rs)
    });
    st.safe_point(&mut [std::slice::from_mut(&mut m)], false);
    let mut rng = Rng::new(33);
    timed(&mut ph, "10^4 replacements", || {
        for k in 0..10_000 {
            let (i, j) = (rng.below(rows as u64) as usize, rng.below(cols as u64) as usize);
            let (_, mut rs) = parts(st, &m).unwrap();
            let (_, mut row) = parts(st, &rs[i]).unwrap();
            row[j] = st.int(-(k as i64));
            rs[i] = st.app(&list, row);
            m = st.app(&list, rs);
            st.safe_point(&mut [std::slice::from_mut(&mut m)], false);
        }
    });
    Out { phases: ph, check: fingerprint(st, &m), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// S8: allocation stress, n distinct small terms f(i, g(i)), all kept: 100 evaluations each build n/100 of them as one
// list held in the session. Nothing ever dies, so collections are pure overhead.

pub fn s8_alloc<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let (f, g, list) = (st.sym("f"), st.sym("g"), st.sym("list"));
    let chunk = n / 100;
    let mut kept: Vec<St::T> = vec![];
    timed(&mut ph, "build", || {
        for c in 0..100 {
            let v: Vec<St::T> = (0..chunk)
                .map(|k| {
                    let i = (c * chunk + k) as i64;
                    let a = st.int(i);
                    let gb = st.app(&g, vec![a.clone()]);
                    st.app(&f, vec![a, gb])
                })
                .collect();
            let l = st.app(&list, v);
            kept.push(l);
            st.safe_point(&mut [&mut kept[..]], false);
        }
    });
    let all = st.app(&list, kept);
    Out { phases: ph, check: fingerprint(st, &all), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// R10: one bottom-up pass of 10 rules over a random tree (spike 1), with a safe point after the build (the tree is
// live) and after the pass (only the result is live: most of the input tree is garbage).

fn replace_pass<St: Store>(st: &mut St, heads: &[St::S], t: &St::T) -> St::T {
    let Some((h, a)) = parts(st, t) else { return t.clone() };
    let a: Vec<St::T> = a.iter().map(|x| replace_pass(st, heads, x)).collect();
    let k = heads.iter().position(|w| st.sym_is(w, &h)).unwrap();
    if k % 2 == 0 {
        let h2 = heads[(k + 1) % heads.len()].clone();
        return st.app(&h2, a);
    }
    if (k == 1 || k == 3) && a.len() == 1 {
        if let Some(v) = st.as_int(&a[0]) {
            return st.int(v + k as i64);
        }
    }
    st.app(&h, a)
}

pub fn r10_replace<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let heads = syms(st, "h", 10);
    let leaves = syms(st, "s", 10);
    let mut rng = Rng::new(10);
    let mut t = timed(&mut ph, "build", || gen_tree(st, &heads, &leaves, &mut rng, n));
    timed(&mut ph, "safe point (tree live)", || st.safe_point(&mut [std::slice::from_mut(&mut t)], true));
    let mut r = timed(&mut ph, "replace", || replace_pass(st, &heads, &t));
    timed(&mut ph, "safe point (tree dead)", || st.safe_point(&mut [std::slice::from_mut(&mut r)], true));
    Out { phases: ph, check: fingerprint(st, &r), notes: vec![] }
}

// ---------------------------------------------------------------------------------------------------------------
// R11: evaluation to normal form with memoisation, then again after redefining an unrelated symbol, then after
// redefining a contained one (spike 1), with a forced collection before each re-evaluation. The memo entries of the
// live term and its normal form must survive it: the second evaluation must be a single memo hit.

struct Ev<S> {
    inc: S,
    dbl: S,
    sum: S,
    /// symbols with own values (long session): `defs[i]` is the value of `defined[i]`
    defined: Vec<S>,
}

/// Innermost evaluation with the memo (semantics 4.1, 4.4); `misses` counts terms evaluated without a memo hit.
fn eval<St: Store>(st: &mut St, ev: &Ev<St::S>, defs: &[St::T], t: &St::T, misses: &mut u64) -> St::T {
    if let Some(r) = st.memo_get(t) {
        return r;
    }
    *misses += 1;
    let r = match st.view(t) {
        View::Sym(s) => match ev.defined.iter().position(|d| st.sym_is(d, s)) {
            Some(i) => eval(st, ev, defs, &defs[i].clone(), misses),
            None => t.clone(),
        },
        View::Int(_) => t.clone(),
        View::App(..) => {
            let (h, a) = parts(st, t).unwrap();
            let a: Vec<St::T> = a.iter().map(|x| eval(st, ev, defs, x, misses)).collect();
            let ints: Vec<Option<i64>> = a.iter().map(|x| st.as_int(x)).collect();
            if st.sym_is(&h, &ev.inc) && a.len() == 1 && ints[0].is_some() {
                st.int(ints[0].unwrap() + 1)
            } else if st.sym_is(&h, &ev.dbl) && a.len() == 1 && ints[0].is_some() {
                st.int(ints[0].unwrap().wrapping_mul(2))
            } else if st.sym_is(&h, &ev.sum) && a.len() == 2 && ints[0].is_some() && ints[1].is_some() {
                st.int(ints[0].unwrap().wrapping_add(ints[1].unwrap()))
            } else {
                st.app(&h, a)
            }
        }
    };
    st.memo_put(t, &r);
    r
}

pub fn r11_reeval<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let ev = Ev { inc: st.sym("inc"), dbl: st.sym("dbl"), sum: st.sym("sum"), defined: vec![] };
    let (wrap, pair) = (st.sym("wrap"), st.sym("pair"));
    let heads = vec![ev.inc.clone(), ev.dbl.clone(), ev.sum.clone(), wrap, pair];
    let leaves = syms(st, "s", 10);
    let unrelated = st.sym("unrelated");
    let mut rng = Rng::new(11);
    let t = timed(&mut ph, "build", || gen_tree(st, &heads, &leaves, &mut rng, n));
    let mut m = [0u64; 3];
    let r1 = timed(&mut ph, "eval", || eval(st, &ev, &[], &t, &mut m[0]));
    let mut live = [t, r1];
    timed(&mut ph, "collect", || st.safe_point(&mut [&mut live[..]], true));
    st.redefine(&unrelated);
    let r2 = timed(&mut ph, "re-eval (unrelated change)", || eval(st, &ev, &[], &live[0], &mut m[1]));
    let mut live = [live[0].clone(), live[1].clone(), r2];
    timed(&mut ph, "collect", || st.safe_point(&mut [&mut live[..]], true));
    st.redefine(&leaves[3]);
    let r3 = timed(&mut ph, "re-eval (s3 changed)", || eval(st, &ev, &[], &live[0], &mut m[2]));
    assert!(st.same(&live[1], &live[2]) && st.same(&live[1], &r3));
    let notes = vec![("misses eval", m[0]), ("misses unrelated", m[1]), ("misses s3", m[2])];
    Out { phases: ph, check: fingerprint(st, &r3), notes }
}

// ---------------------------------------------------------------------------------------------------------------
// Long session: 1000 evaluations. Each builds a random temporary expression of `size` nodes (garbage after the
// evaluation) over the R11 rules plus five symbols with own values, evaluates it with the memo, and keeps a small
// result `out(k, piece of the normal form, an earlier result)` in the session table (so kept results share
// structure and point backwards). Every 50th evaluation redefines one of the five symbols: its old value becomes
// garbage and every memo entry that depends on it is invalidated by the stamps.

pub fn long_session<St: Store>(st: &mut St, size: usize) -> Out {
    let mut ph = vec![];
    let defined = syms(st, "c", 5);
    let ev = Ev { inc: st.sym("inc"), dbl: st.sym("dbl"), sum: st.sym("sum"), defined: defined.clone() };
    let (wrap, pair, out, list) = (st.sym("wrap"), st.sym("pair"), st.sym("out"), st.sym("list"));
    let heads = vec![ev.inc.clone(), ev.dbl.clone(), ev.sum.clone(), wrap, pair];
    let plain = syms(st, "s", 10);
    let mut leaves = plain.clone();
    leaves.extend(defined.iter().cloned());
    let mut rng = Rng::new(12);
    // definitions do not mention the defined symbols, so own values cannot loop
    let mut defs: Vec<St::T> = (0..5).map(|_| gen_tree(st, &heads, &plain, &mut rng, 200)).collect();
    let mut session: Vec<St::T> = vec![];
    let mut misses = 0u64;
    timed(&mut ph, "1000 evaluations", || {
        for k in 0..1000usize {
            if k % 50 == 49 {
                let j = (k / 50) % 5;
                defs[j] = gen_tree(st, &heads, &plain, &mut rng, 200);
                st.redefine(&defined[j]);
            }
            let temp = gen_tree(st, &heads, &leaves, &mut rng, size);
            let nf = eval(st, &ev, &defs, &temp, &mut misses);
            // a piece of the normal form: four steps down, choosing arguments by k
            let mut piece = nf;
            for d in 0..4 {
                match parts(st, &piece) {
                    Some((_, a)) if !a.is_empty() => piece = a[(k + d) % a.len()].clone(),
                    _ => break,
                }
            }
            let prev = if k > 0 { session[k / 2].clone() } else { st.int(0) };
            let kk = st.int(k as i64);
            let r = st.app(&out, vec![kk, piece, prev]);
            session.push(r);
            st.safe_point(&mut [&mut defs[..], &mut session[..]], false);
        }
    });
    // Fingerprint each kept result's own parts and check its back pointer by identity: fingerprinting the results
    // as trees would expand the shared chain of back pointers many times over.
    let d = st.app(&list, defs);
    let mut check = fingerprint(st, &d);
    for (k, r) in session.iter().enumerate() {
        let (_, a) = parts(st, r).unwrap();
        assert_eq!(st.as_int(&a[0]), Some(k as i64));
        assert!(k == 0 || st.same(&a[2], &session[k / 2]), "back pointer of result {k} lost");
        check = (check.rotate_left(5) ^ fingerprint(st, &a[1])).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    Out { phases: ph, check, notes: vec![("memo misses", misses)] }
}
