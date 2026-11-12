//! R10 and R11 from spike 1 (`spikes/termstore/src/work.rs`), copied unchanged.
use super::{fingerprint, Rng, Store, View};
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
