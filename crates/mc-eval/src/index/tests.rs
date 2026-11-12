use super::*;
use crate::eval::{EvalConfig, Kernel};
use crate::pat::{call, lit, var};
use mc_core::shapes::Rng;
use mc_core::Config;
use std::sync::atomic::AtomicBool;

fn konst(v: i64) -> Rhs {
    Arc::new(move |_: &Binds<'_>, h: &Heap| h.int(v))
}

/// A rule index read concurrently with its one writer shows each rule set before or after each definition: never a
/// missing old rule, never a wrong order, also while the tree's depth grows and rules are replaced (I2.4, I2.9).
#[test]
fn readers_see_before_or_after_each_definition() {
    let h = Heap::new(Config::no_collect());
    let k = Kernel::new(&h, EvalConfig::default());
    let (g, w) = (h.sym("g"), h.sym("w"));
    let rs = RuleSet::new(false);
    let c = |p: Pat| k.compile_pattern(&h, p).expect("pattern").pat;
    rs.insert(&h, Rule::new(c(call(&h, g, vec![var(0)])), RuleRhs::Native(konst(-1))));
    const N: i64 = 3000;
    // subject j: g(w^d(j)) with d = j % 4; its literal rule (once published) comes first, the general one last
    let subject = |j: i64| {
        let mut t = h.int(j);
        for _ in 0..j % 4 {
            t = h.call(w, &[t]);
        }
        h.call(g, &[t])
    };
    let subjects: Vec<_> = (0..N).map(subject).collect();
    let done = AtomicBool::new(false);
    std::thread::scope(|s| {
        s.spawn(|| {
            for j in 0..N {
                rs.insert(&h, Rule::new(c(lit(subjects[j as usize])), RuleRhs::Native(konst(j))));
                if j % 10 == 0 {
                    // the same left side again: a replacement while readers may hold the old rule
                    rs.insert(&h, Rule::new(c(lit(subjects[j as usize])), RuleRhs::Native(konst(j))));
                }
            }
            done.store(true, Release);
        });
        for r in 0..3u64 {
            let (rs, h, subjects, done) = (&rs, &h, &subjects, &done);
            s.spawn(move || {
                let mut rng = Rng::new(r + 1);
                let mut seen = 0;
                while !done.load(Acquire) || seen < 1000 {
                    let j = rng.below(N as u64) as usize;
                    let ids = rs.candidate_ids(h, subjects[j]);
                    assert_eq!(ids.last(), Some(&0), "the general rule is missing or out of order: {ids:?}");
                    assert!(ids.len() <= 2, "extra candidates {ids:?}");
                    if ids.len() == 2 {
                        let Pat::Lit(t) = rs.rule(ids[0]).lhs else { panic!("not a literal rule") };
                        assert_eq!(t.get(h), subjects[j]);
                    }
                    seen += 1;
                }
            });
        }
    });
    for (j, &t) in subjects.iter().enumerate() {
        assert_eq!(rs.candidate_ids(&h, t).len(), 2, "subject {j} after all definitions");
    }
    assert_eq!(rs.len() as i64, N + 1, "replacements must not add rules");
}

/// Edges whose key codes collide stay separate: the lookup compares handles (A.2).
#[test]
fn colliding_codes_keep_separate_edges() {
    let h = Heap::new(Config::no_collect());
    let t = Trie::new();
    let (a, b) = (h.int(1), h.int(2));
    let node = t.node(0);
    // two edges filed under one code by hand, as a hash collision would
    let code = Key { kind: KeyKind::Atom, term: a, arity: 0 }.code(&h);
    for (term, child) in [(a, 7u32), (b, 9)] {
        let head = node.edges.get(code).unwrap_or(0);
        let e = t.edges.push(Edge { kind: KeyKind::Atom, arity: 0, term: Raw::new(term), child, next: head });
        node.edges.set(code, e as u32 + 1);
    }
    assert_eq!(t.child(&h, node, Key { kind: KeyKind::Atom, term: a, arity: 0 }), Some(7));
    // `b` under `a`'s code is found only through the chain, by handle
    let mut e = node.edges.get(code).unwrap();
    let mut found = None;
    while e != 0 {
        let edge = t.edges.get(e as usize - 1).unwrap();
        if edge.term == Raw::new(b) {
            found = Some(edge.child);
        }
        e = edge.next;
    }
    assert_eq!(found, Some(9));
    assert_eq!(t.child(&h, node, Key { kind: KeyKind::Atom, term: h.int(3), arity: 0 }), None);
}
