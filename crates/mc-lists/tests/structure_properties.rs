//! Package 1A's structural functions against oracles in Rust, on random inputs with a fixed seed.
mod common;
use common::Env;
use std::collections::HashMap;

/// A small deterministic generator (64-bit LCG, high bits).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0 >> 33
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn ints(&mut self, n: usize, lo: i64, hi: i64) -> Vec<i64> {
        (0..n).map(|_| lo + self.below((hi - lo + 1) as u64) as i64).collect()
    }
}

fn list<T: ToString>(xs: &[T]) -> String {
    format!("[{}]", xs.iter().map(T::to_string).collect::<Vec<_>>().join(", "))
}

fn lists<T: ToString>(xss: &[Vec<T>]) -> String {
    format!("[{}]", xss.iter().map(|xs| list(xs)).collect::<Vec<_>>().join(", "))
}

/// Every arrangement of `k` positions of `0..n`, lexicographically.
fn arrangements(n: usize, k: usize) -> Vec<Vec<usize>> {
    let mut out = vec![];
    let mut path = vec![];
    fn go(n: usize, k: usize, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if path.len() == k {
            out.push(path.clone());
            return;
        }
        for i in 0..n {
            if !path.contains(&i) {
                path.push(i);
                go(n, k, path, out);
                path.pop();
            }
        }
    }
    go(n, k, &mut path, &mut out);
    out
}

#[test]
fn permutations_are_the_distinct_arrangements_in_first_occurrence_order() {
    let mut env = Env::new();
    let mut r = Rng(11);
    for _ in 0..60 {
        let n = r.below(7) as usize;
        let xs: Vec<String> = (0..n).map(|_| ["a", "b", "c", "d"][r.below(4) as usize].to_string()).collect();
        let k = r.below(n as u64 + 1) as usize;
        // the oracle: arrangements of positions in lexicographic order, repeated value sequences dropped
        let mut seen = std::collections::HashSet::new();
        let want: Vec<Vec<String>> = arrangements(n, k)
            .into_iter()
            .map(|p| p.iter().map(|&i| xs[i].clone()).collect::<Vec<_>>())
            .filter(|v| seen.insert(v.clone()))
            .collect();
        assert_eq!(env.eval(&format!("permutations({}, [{k}])", list(&xs))), lists(&want), "{xs:?} {k}");
    }
}

#[test]
fn subsets_and_tuples_agree_with_enumeration() {
    let mut env = Env::new();
    let mut r = Rng(12);
    for _ in 0..30 {
        let n = r.below(8) as usize;
        let xs = r.ints(n, -5, 5);
        // subsets: by size, then lexicographically by position (bit patterns enumerate every subset once)
        let mut want: Vec<Vec<usize>> = (0u32..1 << n).map(|m| (0..n).filter(|&i| m >> i & 1 == 1).collect()).collect();
        want.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        let want: Vec<Vec<i64>> = want.iter().map(|p| p.iter().map(|&i| xs[i]).collect()).collect();
        assert_eq!(env.eval(&format!("subsets({})", list(&xs))), lists(&want));
        let k = r.below(4) as usize;
        let mut tup: Vec<Vec<i64>> = vec![vec![]];
        for _ in 0..k {
            tup = tup.iter().flat_map(|t| xs.iter().map(move |&x| [t.clone(), vec![x]].concat())).collect();
        }
        assert_eq!(env.eval(&format!("tuples({}, {k})", list(&xs))), lists(&tup));
    }
}

#[test]
fn orderings_are_stable_and_key_sorts_break_ties_by_the_elements() {
    let mut env = Env::new();
    let mut r = Rng(13);
    for _ in 0..30 {
        let n = r.below(200) as usize;
        let xs = r.ints(n, -20, 20);
        let mut ix: Vec<usize> = (0..n).collect();
        ix.sort_by_key(|&i| xs[i]);
        let pos: Vec<usize> = ix.iter().map(|i| i + 1).collect();
        assert_eq!(env.eval(&format!("ordering({})", list(&xs))), list(&pos));
        // equal keys: the elements in increasing order (decision cov-core.7)
        let mut by = xs.clone();
        by.sort_by_key(|&x| (x.rem_euclid(7), x));
        assert_eq!(env.eval(&format!("sort_by({}, k => imod(k, 7))", list(&xs))), list(&by));
        if n > 0 {
            let best = xs.iter().enumerate().fold(0, |b, (i, x)| if x.abs() > xs[b].abs() { i } else { b });
            assert_eq!(env.eval(&format!("max_by({}, k => k^2)", list(&xs))), xs[best].to_string());
            let least = xs.iter().enumerate().fold(0, |b, (i, x)| if x.abs() < xs[b].abs() { i } else { b });
            assert_eq!(env.eval(&format!("min_by({}, k => k^2)", list(&xs))), xs[least].to_string());
        }
    }
}

#[test]
fn groups_agree_with_hash_maps() {
    let mut env = Env::new();
    let mut r = Rng(14);
    for _ in 0..30 {
        let n = r.below(300) as usize;
        let xs = r.ints(n, 0, 12);
        let (mut keys, mut groups): (Vec<i64>, HashMap<i64, Vec<i64>>) = (vec![], HashMap::new());
        for &x in &xs {
            let k = x % 4;
            groups.entry(k).or_insert_with(|| {
                keys.push(k);
                vec![]
            });
            groups.get_mut(&k).unwrap().push(x);
        }
        let gathered: Vec<Vec<i64>> = keys.iter().map(|k| groups[k].clone()).collect();
        assert_eq!(env.eval(&format!("gather_by({}, k => imod(k, 4))", list(&xs))), lists(&gathered));
        let counted: Vec<String> = keys.iter().map(|k| format!("{k} -> {}", groups[k].len())).collect();
        assert_eq!(env.eval(&format!("count_by({}, k => imod(k, 4))", list(&xs))), format!("assoc({})", counted.join(", ")));
        let firsts: Vec<i64> = keys.iter().map(|k| groups[k][0]).collect();
        assert_eq!(env.eval(&format!("unique_by({}, k => imod(k, 4))", list(&xs))), list(&firsts));
        let mut order = vec![];
        let mut counts: HashMap<i64, usize> = HashMap::new();
        for &x in &xs {
            *counts.entry(x).or_insert_with(|| {
                order.push(x);
                0
            }) += 1;
        }
        let tally: Vec<Vec<i64>> = order.iter().map(|&x| vec![x, counts[&x] as i64]).collect();
        assert_eq!(env.eval(&format!("tally({})", list(&xs))), lists(&tally));
    }
}

#[test]
fn rearrangements_agree_with_slices() {
    let mut env = Env::new();
    let mut r = Rng(15);
    for _ in 0..40 {
        let n = r.below(40) as usize;
        let xs = r.ints(n, -100, 100);
        let (k, d) = (1 + r.below(5) as usize, 1 + r.below(5) as usize);
        let parts: Vec<Vec<i64>> = if n < k { vec![] } else { (0..=(n - k) / d).map(|i| xs[i * d..i * d + k].to_vec()).collect() };
        assert_eq!(env.eval(&format!("partition({}, {k}, {d})", list(&xs))), lists(&parts));
        let s = r.below(100) as i64 - 50;
        let mut rot = xs.clone();
        if n > 0 {
            rot.rotate_left(s.rem_euclid(n as i64) as usize);
        }
        assert_eq!(env.eval(&format!("rotate_left({}, {s})", list(&xs))), list(&rot));
        assert_eq!(env.eval(&format!("rotate_right(rotate_left({}, {s}), {s})", list(&xs))), list(&xs));
        let m = r.below(60) as usize;
        let padded: Vec<i64> = if m >= n { [vec![7; m - n], xs.clone()].concat() } else { xs[n - m..].to_vec() };
        assert_eq!(env.eval(&format!("pad_left({}, {m}, 7)", list(&xs))), list(&padded));
        let padded: Vec<i64> = if m >= n { [xs.clone(), vec![7; m - n]].concat() } else { xs[..m].to_vec() };
        assert_eq!(env.eval(&format!("pad_right({}, {m}, 7)", list(&xs))), list(&padded));
        let riffled: Vec<i64> = xs.iter().enumerate().flat_map(|(i, &x)| if i + 1 < n { vec![x, 0] } else { vec![x] }).collect();
        assert_eq!(env.eval(&format!("riffle({}, 0)", list(&xs))), list(&riffled));
    }
}

/// A random nested list of integers and symbols, as source text, with the positions of its integers in the order
/// `position` gives them (children before parents; heads hold no integers).
fn nested(r: &mut Rng, depth: u32, path: &mut Vec<usize>, ints: &mut Vec<Vec<usize>>) -> String {
    if depth == 0 || r.below(3) == 0 {
        if r.below(2) == 0 {
            ints.push(path.clone());
            return (r.below(100) as i64 - 50).to_string();
        }
        return "s".to_string();
    }
    let n = r.below(4) as usize;
    let mut items = vec![];
    for i in 0..n {
        path.push(i + 1);
        items.push(nested(r, depth - 1, path, ints));
        path.pop();
    }
    format!("[{}]", items.join(", "))
}

#[test]
fn positions_and_free_of_agree_with_a_recursive_walk() {
    let mut env = Env::new();
    let mut r = Rng(16);
    for _ in 0..60 {
        let mut ints = vec![];
        let src = nested(&mut r, 5, &mut vec![], &mut ints);
        let want: Vec<String> = ints.iter().map(|p| list(p)).collect();
        assert_eq!(env.eval(&format!("position({src}, _: Int)")), list(&want), "{src}");
        assert_eq!(env.eval(&format!("free_of({src}, _: Int)")), (ints.is_empty()).to_string(), "{src}");
        for p in &ints {
            let ix: Vec<String> = p.iter().map(usize::to_string).collect();
            assert!(env.eval(&format!("is_integer(part({src}, {}))", ix.join(", "))) == "true");
        }
    }
}

#[test]
fn inner_and_outer_agree_with_matrix_products() {
    let mut env = Env::new();
    let mut r = Rng(17);
    for _ in 0..20 {
        let (m, n, p) = (1 + r.below(5) as usize, 1 + r.below(5) as usize, 1 + r.below(5) as usize);
        let a: Vec<Vec<i64>> = (0..m).map(|_| r.ints(n, -9, 9)).collect();
        let b: Vec<Vec<i64>> = (0..n).map(|_| r.ints(p, -9, 9)).collect();
        let c: Vec<Vec<i64>> =
            (0..m).map(|i| (0..p).map(|j| (0..n).map(|k| a[i][k] * b[k][j]).sum()).collect()).collect();
        assert_eq!(env.eval(&format!("inner(mul, {}, {})", lists(&a), lists(&b))), lists(&c));
        let (u, v) = (r.ints(m, -9, 9), r.ints(p, -9, 9));
        let o: Vec<Vec<i64>> = u.iter().map(|x| v.iter().map(|y| x * y).collect()).collect();
        assert_eq!(env.eval(&format!("outer(mul, {}, {})", list(&u), list(&v))), lists(&o));
        assert_eq!(env.eval(&format!("table(i*j, i, {}, j, {})", list(&u), list(&v))), lists(&o));
        assert_eq!(env.eval(&format!("transpose(outer(mul, {}, {})) == outer(mul, {}, {})", list(&u), list(&v), list(&v), list(&u))), "true");
    }
}
