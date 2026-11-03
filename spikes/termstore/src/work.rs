//! Workloads, written once against [`Store`]. Each returns its phases' times and a fingerprint of the result, so
//! the stores can be checked against each other. Ids refer to the benchmark suite (02-benchmarks.md).
use crate::{fingerprint, Rng, Store, View};
use std::time::{Duration, Instant};

pub struct Out {
    pub phases: Vec<(&'static str, Duration)>,
    pub check: u64,
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

// ---------------------------------------------------------------------------------------------------------------
// R3: symbolic derivative of a random expression tree

struct Calc<S> {
    add: S,
    mul: S,
    pow: S,
    sin: S,
    cos: S,
    exp: S,
    log: S,
    x: S,
    y: S,
}

impl<S: Clone> Calc<S> {
    fn new<St: Store<S = S>>(st: &mut St) -> Calc<S> {
        Calc {
            add: st.sym("add"),
            mul: st.sym("mul"),
            pow: st.sym("pow"),
            sin: st.sym("sin"),
            cos: st.sym("cos"),
            exp: st.sym("exp"),
            log: st.sym("log"),
            x: st.sym("x"),
            y: st.sym("y"),
        }
    }
}

fn gen_calc<St: Store>(st: &mut St, c: &Calc<St::S>, rng: &mut Rng, size: usize) -> St::T {
    if size <= 1 {
        return match rng.below(3) {
            0 => st.sym_term(&c.x),
            1 => st.sym_term(&c.y),
            _ => st.int(rng.below(9) as i64 + 1),
        };
    }
    match rng.below(10) {
        0..=2 => {
            let k = 1 + rng.below(size as u64 - 1) as usize;
            let a = gen_calc(st, c, rng, k);
            let b = gen_calc(st, c, rng, size - k);
            let h = if rng.below(2) == 0 { c.add.clone() } else { c.mul.clone() };
            st.app(&h, vec![a, b])
        }
        3 => {
            let a = gen_calc(st, c, rng, size - 1);
            let n = st.int(rng.below(4) as i64 + 2);
            st.app(&c.pow, vec![a, n])
        }
        k => {
            let a = gen_calc(st, c, rng, size - 1);
            let h = [&c.sin, &c.cos, &c.exp, &c.log, &c.sin, &c.cos][(k - 4) as usize].clone();
            st.app(&h, vec![a])
        }
    }
}

fn mk_add<St: Store>(st: &mut St, c: &Calc<St::S>, a: St::T, b: St::T) -> St::T {
    match (st.as_int(&a), st.as_int(&b)) {
        (Some(0), _) => b,
        (_, Some(0)) => a,
        (Some(x), Some(y)) => st.int(x.wrapping_add(y)),
        _ => st.app(&c.add, vec![a, b]),
    }
}

fn mk_mul<St: Store>(st: &mut St, c: &Calc<St::S>, a: St::T, b: St::T) -> St::T {
    match (st.as_int(&a), st.as_int(&b)) {
        (Some(0), _) | (_, Some(0)) => st.int(0),
        (Some(1), _) => b,
        (_, Some(1)) => a,
        (Some(x), Some(y)) => st.int(x.wrapping_mul(y)),
        _ => st.app(&c.mul, vec![a, b]),
    }
}

fn deriv<St: Store>(st: &mut St, c: &Calc<St::S>, t: &St::T) -> St::T {
    let Some((h, a)) = parts(st, t) else {
        return match st.view(t) {
            View::Sym(s) if st.sym_is(s, &c.x) => st.int(1),
            _ => st.int(0),
        };
    };
    if st.sym_is(&h, &c.add) {
        let (da, db) = (deriv(st, c, &a[0]), deriv(st, c, &a[1]));
        mk_add(st, c, da, db)
    } else if st.sym_is(&h, &c.mul) {
        let (da, db) = (deriv(st, c, &a[0]), deriv(st, c, &a[1]));
        let l = mk_mul(st, c, da, a[1].clone());
        let r = mk_mul(st, c, a[0].clone(), db);
        mk_add(st, c, l, r)
    } else if st.sym_is(&h, &c.pow) {
        let n = st.as_int(&a[1]).unwrap();
        let da = deriv(st, c, &a[0]);
        let n1 = st.int(n - 1);
        let p = st.app(&c.pow, vec![a[0].clone(), n1]);
        let nn = st.int(n);
        let m = mk_mul(st, c, nn, p);
        mk_mul(st, c, m, da)
    } else {
        let da = deriv(st, c, &a[0]);
        let outer = if st.sym_is(&h, &c.sin) {
            st.app(&c.cos, vec![a[0].clone()])
        } else if st.sym_is(&h, &c.cos) {
            let s = st.app(&c.sin, vec![a[0].clone()]);
            let m1 = st.int(-1);
            mk_mul(st, c, m1, s)
        } else if st.sym_is(&h, &c.exp) {
            t.clone()
        } else {
            let m1 = st.int(-1);
            st.app(&c.pow, vec![a[0].clone(), m1])
        };
        mk_mul(st, c, outer, da)
    }
}

pub fn r3_derivative<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let c = Calc::new(st);
    let mut rng = Rng::new(3);
    let t = timed(&mut ph, "build", || gen_calc(st, &c, &mut rng, n));
    let d = timed(&mut ph, "derivative", || deriv(st, &c, &t));
    Out { phases: ph, check: fingerprint(st, &d) }
}

// ---------------------------------------------------------------------------------------------------------------
// R8: expand (a+b+c+d)^n as plain terms: monomials are mul(pow(v, k), ...) terms, like terms collected in a hash map

fn mono_times<St: Store>(st: &mut St, mul: &St::S, pow: &St::S, vars: &[St::S], m: &St::T, v: usize) -> St::T {
    let mut exps = vec![0i64; vars.len()];
    if let Some((_, factors)) = parts(st, m) {
        for f in &factors {
            if let Some((_, pa)) = parts(st, f) {
                if let View::Sym(s) = st.view(&pa[0]) {
                    let i = vars.iter().position(|w| st.sym_is(w, s)).unwrap();
                    exps[i] = st.as_int(&pa[1]).unwrap();
                }
            }
        }
    }
    exps[v] += 1;
    let mut fs = vec![];
    for (i, &e) in exps.iter().enumerate() {
        if e > 0 {
            let base = st.sym_term(&vars[i]);
            let k = st.int(e);
            fs.push(st.app(pow, vec![base, k]));
        }
    }
    st.app(mul, fs)
}

pub fn r8_expand<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let (add, mul, pow) = (st.sym("add"), st.sym("mul"), st.sym("pow"));
    let vars: Vec<St::S> = ["a", "b", "c", "d"].iter().map(|v| st.sym(v)).collect();
    let one = st.app(&mul, vec![]);
    let result = timed(&mut ph, "expand", || {
        let mut poly: Vec<(St::T, i64)> = vec![(one, 1)];
        for _ in 0..n {
            // collect like terms: hash buckets compared with `same`
            let mut buckets: rustc_hash::FxHashMap<u64, Vec<usize>> = Default::default();
            let mut next: Vec<(St::T, i64)> = vec![];
            for (m, c) in &poly {
                for v in 0..vars.len() {
                    let m2 = mono_times(st, &mul, &pow, &vars, m, v);
                    let h = st.hash_term(&m2);
                    let b = buckets.entry(h).or_default();
                    match b.iter().find(|&&i| st.same(&next[i].0, &m2)) {
                        Some(&i) => next[i].1 = next[i].1.wrapping_add(*c),
                        None => {
                            b.push(next.len());
                            next.push((m2, *c));
                        }
                    }
                }
            }
            poly = next;
        }
        let terms: Vec<St::T> = poly
            .into_iter()
            .map(|(m, c)| {
                let k = st.int(c);
                st.app(&mul, vec![k, m])
            })
            .collect();
        st.app(&add, terms)
    });
    Out { phases: ph, check: fingerprint(st, &result) }
}

// ---------------------------------------------------------------------------------------------------------------
// R9: deep nesting f(f(...f(x)...)): build, measure depth, replace x by y, drop

pub fn r9_deep<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let (f, x, y) = (st.sym("f"), st.sym("x"), st.sym("y"));
    let t = timed(&mut ph, "build", || {
        let mut t = st.sym_term(&x);
        for _ in 0..n {
            t = st.app(&f, vec![t]);
        }
        t
    });
    let depth = timed(&mut ph, "depth", || {
        let mut d = 0usize;
        let mut cur = t.clone();
        while let Some((_, a)) = parts(st, &cur) {
            d += 1;
            cur = a[0].clone();
        }
        d
    });
    assert_eq!(depth, n);
    let r = timed(&mut ph, "replace", || {
        let mut heads = 0usize;
        let mut cur = t.clone();
        while let Some((_, a)) = parts(st, &cur) {
            heads += 1;
            cur = a[0].clone();
        }
        let mut r = st.sym_term(&y);
        for _ in 0..heads {
            r = st.app(&f, vec![r]);
        }
        r
    });
    let check = fingerprint(st, &r);
    timed(&mut ph, "drop", || {
        drop(t);
        drop(r);
    });
    Out { phases: ph, check }
}

// ---------------------------------------------------------------------------------------------------------------
// Random trees over heads h0..h9 (arity 1..3) and leaves (small integers and symbols s0..s9)

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

fn syms<St: Store>(st: &mut St, prefix: &str, n: usize) -> Vec<St::S> {
    (0..n).map(|i| st.sym(&format!("{prefix}{i}"))).collect()
}

// R10: one bottom-up pass of 10 rules (even heads renamed, h1/h3 of a single integer folded)

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
    let t = timed(&mut ph, "build", || gen_tree(st, &heads, &leaves, &mut rng, n));
    let r = timed(&mut ph, "replace", || replace_pass(st, &heads, &t));
    Out { phases: ph, check: fingerprint(st, &r) }
}

// ---------------------------------------------------------------------------------------------------------------
// R11: evaluation to normal form, then again after redefining an unrelated symbol, then after redefining a contained
// one. Rules: inc(n) -> n+1, dbl(n) -> 2n, sum(a, b) -> a+b on integers; wrap and pair are constructors.

struct Ev<S> {
    inc: S,
    dbl: S,
    sum: S,
}

fn eval<St: Store>(st: &mut St, ev: &Ev<St::S>, t: &St::T) -> St::T {
    if let Some(r) = st.memo_get(t) {
        return r;
    }
    let r = match parts(st, t) {
        None => t.clone(),
        Some((h, a)) => {
            let a: Vec<St::T> = a.iter().map(|x| eval(st, ev, x)).collect();
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
    let ev = Ev { inc: st.sym("inc"), dbl: st.sym("dbl"), sum: st.sym("sum") };
    let wrap = st.sym("wrap");
    let pair = st.sym("pair");
    let heads = vec![ev.inc.clone(), ev.dbl.clone(), ev.sum.clone(), wrap, pair];
    let leaves = syms(st, "s", 10);
    let unrelated = st.sym("unrelated");
    let mut rng = Rng::new(11);
    let t = timed(&mut ph, "build", || gen_tree(st, &heads, &leaves, &mut rng, n));
    let r1 = timed(&mut ph, "eval", || eval(st, &ev, &t));
    st.redefine(&unrelated);
    let r2 = timed(&mut ph, "re-eval (unrelated change)", || eval(st, &ev, &t));
    st.redefine(&leaves[3]);
    let r3 = timed(&mut ph, "re-eval (s3 changed)", || eval(st, &ev, &t));
    assert!(st.same(&r1, &r2) && st.same(&r1, &r3));
    Out { phases: ph, check: fingerprint(st, &r1) }
}

// ---------------------------------------------------------------------------------------------------------------
// S3: replace elements of a rows x cols integer matrix (list of lists), k times

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
    let mut rng = Rng::new(33);
    timed(&mut ph, "10^4 replacements", || {
        for k in 0..10_000 {
            let (i, j) = (rng.below(rows as u64) as usize, rng.below(cols as u64) as usize);
            let (_, mut rs) = parts(st, &m).unwrap();
            let (_, mut row) = parts(st, &rs[i]).unwrap();
            row[j] = st.int(-(k as i64));
            rs[i] = st.app(&list, row);
            m = st.app(&list, rs);
        }
    });
    Out { phases: ph, check: fingerprint(st, &m) }
}

// S6: transpose a square matrix

pub fn s6_transpose<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let list = st.sym("list");
    let side = (n as f64).sqrt() as usize;
    let m = timed(&mut ph, "build", || {
        let rs: Vec<St::T> = (0..side)
            .map(|i| {
                let row: Vec<St::T> = (0..side).map(|j| st.int((i * side + j) as i64)).collect();
                st.app(&list, row)
            })
            .collect();
        st.app(&list, rs)
    });
    let t = timed(&mut ph, "transpose", || {
        let (_, rs) = parts(st, &m).unwrap();
        let rows: Vec<Vec<St::T>> = rs.iter().map(|r| parts(st, r).unwrap().1).collect();
        let cols: Vec<St::T> = (0..side)
            .map(|j| {
                let col: Vec<St::T> = rows.iter().map(|r| r[j].clone()).collect();
                st.app(&list, col)
            })
            .collect();
        st.app(&list, cols)
    });
    Out { phases: ph, check: fingerprint(st, &t) }
}

// S8: allocation stress, n distinct small terms f(i, g(i)) kept alive

pub fn s8_alloc<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let (f, g, list) = (st.sym("f"), st.sym("g"), st.sym("list"));
    let all = timed(&mut ph, "build", || {
        let v: Vec<St::T> = (0..n as i64)
            .map(|i| {
                let a = st.int(i);
                let b = st.int(i);
                let gb = st.app(&g, vec![b]);
                st.app(&f, vec![a, gb])
            })
            .collect();
        v
    });
    let l = st.app(&list, all);
    Out { phases: ph, check: fingerprint(st, &l) }
}

// Equality and hashing of two independently built equal trees (the cost hash-consing removes)

pub fn eq_compare<St: Store>(st: &mut St, n: usize) -> Out {
    let mut ph = vec![];
    let heads = syms(st, "h", 10);
    let leaves = syms(st, "s", 10);
    let a = gen_tree(st, &heads, &leaves, &mut Rng::new(7), n);
    let b = gen_tree(st, &heads, &leaves, &mut Rng::new(7), n);
    let same = timed(&mut ph, "100 x same", || (0..100).all(|_| st.same(&a, &b)));
    assert!(same);
    timed(&mut ph, "100 x hash", || {
        let mut x = 0u64;
        for _ in 0..100 {
            x ^= st.hash_term(&a);
        }
        x
    });
    Out { phases: ph, check: fingerprint(st, &a) }
}
