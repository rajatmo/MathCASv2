use super::*;
use crate::eval::{EvalConfig, Kernel};
use crate::pat::{call, lit, seq0, seq1, typed, var, Ty};
use crate::Attributes;
use mc_core::shapes::{fingerprint, Rng};
use mc_core::{Config, Heap, Store};

fn fp<'h>(h: &'h Heap, b: &Binds<'h>) -> Vec<u64> {
    (0..b.len() as u16)
        .map(|v| match b.get(v) {
            Val::Unbound => 7,
            Val::One(t) => fingerprint(h, t),
            Val::Seq(..) => b.seq(v).iter().fold(11u64, |a, &t| a.rotate_left(5) ^ fingerprint(h, t)),
        })
        .collect()
}

/// The pruning AC strategy finds the same first match as the naive enumeration of the order (semantics 6.2).
#[test]
fn pruning_and_naive_enumeration_agree() {
    let h = Heap::new(Config::no_collect());
    let mut k = Kernel::new(&h, EvalConfig::default());
    let (add, f) = (h.sym("add"), h.sym("f"));
    k.set_attributes(add, Attributes { ac: true, ..Attributes::default() });
    let atoms: Vec<_> = ["a", "b", "c"].iter().map(|s| h.sym_term(h.sym(s))).chain([h.int(1), h.int(2)]).collect();
    let mut rng = Rng::new(3);
    let subjects: Vec<_> = (0..200)
        .map(|_| {
            let n = 1 + rng.below(6) as usize;
            let a: Vec<_> = (0..n)
                .map(|_| {
                    let t = atoms[rng.below(atoms.len() as u64) as usize];
                    if rng.below(100) < 30 {
                        h.call(f, &[t])
                    } else {
                        t
                    }
                })
                .collect();
            k.make(&h, h.sym_term(add), &a)
        })
        .collect();
    let pats: Vec<_> = (0..200)
        .map(|_| {
            let n = 1 + rng.below(4) as usize;
            let mut a: Vec<Pat> = (0..n)
                .map(|_| match rng.below(5) {
                    0 => lit(atoms[rng.below(atoms.len() as u64) as usize]),
                    1 => call(&h, f, vec![var(rng.below(3) as u16)]),
                    2 => typed(rng.below(3) as u16, Ty::Symbol),
                    _ => var(rng.below(3) as u16),
                })
                .collect();
            if rng.below(100) < 60 {
                a.push(if rng.below(2) == 0 { seq0(3) } else { seq1(3) });
            }
            k.compile_pattern(&h, call(&h, add, a)).expect("pattern")
        })
        .collect();
    let ev = Eval { heap: &h, k: &k };
    let mut cx = Cx::frontier(k.epoch(), 1);
    let (mut b1, mut b2, mut s) = (Binds::default(), Binds::default(), Binds::default());
    let mut matched = 0;
    for p in &pats {
        for &t in &subjects {
            let m1 = Matcher::new(&ev, &mut cx).naive().first(&p.pat, p.nvars, t, &mut s, &mut b1);
            let m2 = Matcher::new(&ev, &mut cx).first(&p.pat, p.nvars, t, &mut s, &mut b2);
            assert_eq!(m1, m2);
            if m1 {
                matched += 1;
                assert_eq!(fp(&h, &b1), fp(&h, &b2));
            }
        }
    }
    assert!(matched > 100, "the test should exercise matches ({matched})");
}
