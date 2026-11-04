//! Random cases (fixed seeds) against Rust oracles: the set operations against `BTreeSet`, and associations under a
//! random sequence of changes against an insertion-ordered vector of pairs, with sizes on both sides of the indexed
//! path (more than 16 keys) and every statement in its own step (so with stress, a collection between any two).
mod common;
use std::collections::BTreeSet;

/// A small deterministic generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn list(&mut self, len: u64, range: u64) -> Vec<i64> {
        (0..self.below(len + 1)).map(|_| self.below(range) as i64 - 5).collect()
    }
}

fn text(xs: &[i64]) -> String {
    format!("[{}]", xs.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

fn set_text(s: &BTreeSet<i64>) -> String {
    text(&s.iter().copied().collect::<Vec<_>>())
}

#[test]
fn sets_agree_with_btreeset() {
    let mut e = common::engine(2, false);
    let mut r = Rng(0x9E37_79B9_7F4A_7C15);
    for case in 0..300 {
        let n = 1 + r.below(4) as usize;
        let lists: Vec<Vec<i64>> = (0..n).map(|_| r.list(40, 60)).collect();
        let sets: Vec<BTreeSet<i64>> = lists.iter().map(|l| l.iter().copied().collect()).collect();
        let args = lists.iter().map(|l| text(l)).collect::<Vec<_>>().join(", ");
        let union: BTreeSet<i64> = sets.iter().flatten().copied().collect();
        let others = &sets[1..];
        let inter: BTreeSet<i64> = sets[0].iter().copied().filter(|x| others.iter().all(|s| s.contains(x))).collect();
        let comp: BTreeSet<i64> = sets[0].iter().copied().filter(|x| others.iter().all(|s| !s.contains(x))).collect();
        assert_eq!(common::eval(&mut e, &format!("union({args})")), set_text(&union), "case {case}");
        assert_eq!(common::eval(&mut e, &format!("intersection({args})")), set_text(&inter), "case {case}");
        assert_eq!(common::eval(&mut e, &format!("complement({args})")), set_text(&comp), "case {case}");
        let (a, b) = (&lists[0], &lists[n - 1]);
        let sub = sets[0].is_subset(&sets[n - 1]);
        let src = format!("is_subset({}, {})", text(a), text(b));
        assert_eq!(common::eval(&mut e, &src), sub.to_string(), "case {case}");
    }
}

/// The oracle: entries in insertion order, later values replacing earlier ones in place.
#[derive(Default)]
struct Ordered(Vec<(i64, i64)>);

impl Ordered {
    fn insert(&mut self, k: i64, v: i64) {
        match self.0.iter_mut().find(|e| e.0 == k) {
            Some(e) => e.1 = v,
            None => self.0.push((k, v)),
        }
    }
    fn delete(&mut self, k: i64) {
        self.0.retain(|e| e.0 != k);
    }
    fn get(&self, k: i64) -> Option<i64> {
        self.0.iter().find(|e| e.0 == k).map(|e| e.1)
    }
    fn text(&self) -> String {
        let es: Vec<String> = self.0.iter().map(|(k, v)| format!("{k} -> {v}")).collect();
        format!("assoc({})", es.join(", "))
    }
}

/// Random changes and lookups, checked statement by statement.
fn associations(threads: usize, stress: bool, seed: u64, steps: usize) {
    let mut e = common::engine(threads, stress);
    let mut r = Rng(seed);
    for round in 0..4 {
        let keys = [8, 30, 120, 400][round];
        let mut m = Ordered::default();
        let rules: Vec<(i64, i64)> = (0..keys).map(|_| (r.below(keys) as i64, r.below(1000) as i64)).collect();
        for &(k, v) in &rules {
            m.insert(k, v);
        }
        let src: Vec<String> = rules.iter().map(|(k, v)| format!("{k} -> {v}")).collect();
        assert_eq!(common::eval(&mut e, &format!("a = assoc([{}])\na", src.join(", "))), m.text());
        for step in 0..steps {
            let k = r.below(keys + 10) as i64;
            let what = format!("seed {seed}, round {round}, step {step}");
            match r.below(6) {
                0 => {
                    let v = r.below(1000) as i64;
                    m.insert(k, v);
                    common::eval(&mut e, &format!("a = insert(a, {k} -> {v})"));
                }
                1 => {
                    m.delete(k);
                    common::eval(&mut e, &format!("a = delete_key(a, {k})"));
                }
                2 => {
                    let want = m.get(k).map_or("none".to_string(), |v| v.to_string());
                    assert_eq!(common::eval(&mut e, &format!("lookup(a, {k}, none)")), want, "{what}");
                }
                3 => {
                    let want = m.get(k).map_or(format!("failure(missing_key, {k})"), |v| v.to_string());
                    assert_eq!(common::eval(&mut e, &format!("lookup(a, {k})")), want, "{what}");
                }
                4 => {
                    let want = m.get(k).is_some().to_string();
                    assert_eq!(common::eval(&mut e, &format!("key_exists(a, {k})")), want, "{what}");
                }
                _ => {
                    // every key in one statement (the indexed path for large associations)
                    let want: Vec<i64> = (0..keys as i64 + 10).map(|k| m.get(k).unwrap_or(-1)).collect();
                    let src = format!("map(k => lookup(a, k, -1), range(0, {}))", keys + 9);
                    assert_eq!(common::eval(&mut e, &src), text(&want), "{what}");
                }
            }
        }
        assert_eq!(common::eval(&mut e, "a"), m.text(), "seed {seed}, round {round}");
        assert_eq!(common::eval(&mut e, "length(a)"), m.0.len().to_string());
    }
}

#[test]
fn associations_agree_with_an_ordered_vector() {
    associations(1, false, 0xD1B5_4A32_D192_ED03, 150);
    associations(2, false, 0x8CB9_2BA7_2F3D_8DD7, 150);
}

#[test]
fn associations_agree_under_stress() {
    associations(2, true, 0xABCD_EF01_2345_6789, 60);
}
