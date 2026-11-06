//! Randomised checks against oracles in Rust (`num-bigint` for exact arithmetic), with a fixed seed.
mod common;
use common::Env;
use num_bigint::BigInt;
use num_traits::{One, Signed, Zero};

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
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as u64) as i64
    }
    /// A random integer of up to `digits` decimal digits, either sign.
    fn big(&mut self, digits: u64) -> BigInt {
        let n = 1 + self.below(digits);
        let mut s = String::new();
        for _ in 0..n {
            s.push(char::from(b'0' + self.below(10) as u8));
        }
        let v: BigInt = s.parse().unwrap();
        if self.below(2) == 0 {
            -v
        } else {
            v
        }
    }
}

fn list<T: ToString>(xs: &[T]) -> String {
    format!("[{}]", xs.iter().map(T::to_string).collect::<Vec<_>>().join(", "))
}

/// The greatest common divisor (nonnegative).
fn gcd(a: &BigInt, b: &BigInt) -> BigInt {
    let (mut a, mut b) = (a.abs(), b.abs());
    while !b.is_zero() {
        let r = &a % &b;
        a = b;
        b = r;
    }
    a
}

#[test]
fn sums_and_products_of_integers_agree_with_bigint() {
    let mut env = Env::new();
    let mut r = Rng(1);
    for round in 0..40 {
        let n = r.below(300) as usize;
        let digits = [3, 18, 19, 40][round % 4];
        let xs: Vec<BigInt> = (0..n).map(|_| r.big(digits)).collect();
        let want: BigInt = xs.iter().sum();
        assert_eq!(env.eval(&format!("sum({})", list(&xs))), want.to_string(), "round {round}");
        let ys = &xs[..n.min(40)];
        let want: BigInt = ys.iter().product();
        assert_eq!(env.eval(&format!("product({})", list(ys))), want.to_string(), "round {round}");
    }
}

#[test]
fn sums_of_rationals_agree_with_bigint() {
    let mut env = Env::new();
    let mut r = Rng(2);
    for round in 0..20 {
        let n = 1 + r.below(200);
        let mut num = BigInt::zero();
        let mut den = BigInt::one();
        let mut terms = vec![];
        for _ in 0..n {
            let p = BigInt::from(r.int(-50, 50));
            let q = BigInt::from(r.int(1, 1000));
            terms.push(format!("{p}/{q}"));
            num = num * &q + p * &den;
            den *= q;
            let g = gcd(&num, &den);
            if !g.is_zero() {
                num /= &g;
                den /= &g;
            }
        }
        let want = if den.is_one() { num.to_string() } else { format!("{num}/{den}") };
        assert_eq!(env.eval(&format!("sum([{}])", terms.join(", "))), want, "round {round}");
    }
}

#[test]
fn sorting_integers_and_rationals_agrees_with_the_numeric_order() {
    let mut env = Env::new();
    let mut r = Rng(3);
    for round in 0..30 {
        let n = r.below(400) as usize;
        let mut xs: Vec<i64> = (0..n).map(|_| r.int(-1000, 1000)).collect();
        let src = list(&xs);
        xs.sort();
        assert_eq!(env.eval(&format!("sort({src})")), list(&xs), "round {round}");
        assert_eq!(env.eval(&format!("ordered(sort({src}))")), "true");
        assert_eq!(env.eval(&format!("sort({src}, greater)")), list(&xs.iter().rev().collect::<Vec<_>>()));
        // rationals k/7 sort by value, and print reduced
        let ks: Vec<i64> = (0..n.min(60)).map(|_| r.int(-100, 100)).collect();
        let src: Vec<String> = ks.iter().map(|k| format!("{k}/7")).collect();
        let mut sorted = ks.clone();
        sorted.sort();
        let printed: Vec<String> = sorted
            .iter()
            .map(|&k| {
                let g = gcd(&BigInt::from(k), &BigInt::from(7));
                let (p, q) = if g.is_zero() { (BigInt::zero(), BigInt::one()) } else { (k / &g, 7 / &g) };
                if q.is_one() {
                    p.to_string()
                } else {
                    format!("{p}/{q}")
                }
            })
            .collect();
        assert_eq!(env.eval(&format!("sort([{}])", src.join(", "))), format!("[{}]", printed.join(", ")));
    }
}

/// The oracle's form of `part(xs, i)` for a list of integers.
fn part(xs: &[i64], i: i64) -> String {
    let n = xs.len() as i64;
    match i {
        1.. if i <= n => xs[(i - 1) as usize].to_string(),
        ..=-1 if -i <= n => xs[(n + i) as usize].to_string(),
        0 => "list".into(),
        _ => format!("failure(index, {i}, {n})"),
    }
}

#[test]
fn access_agrees_with_slices() {
    let mut env = Env::new();
    let mut r = Rng(4);
    for round in 0..60 {
        let n = r.below(12) as usize;
        let xs: Vec<i64> = (0..n).map(|_| r.int(0, 99)).collect();
        env.eval(&format!("xs = {}", list(&xs)));
        let len = n as i64;
        for _ in 0..6 {
            let i = r.int(-len - 2, len + 2);
            assert_eq!(env.eval(&format!("xs[{i}]")), part(&xs, i), "round {round}: {xs:?}[{i}]");
            let want = if i.unsigned_abs() as usize > n {
                format!("failure(index, {i}, {n})")
            } else if i >= 0 {
                list(&xs[..i as usize])
            } else {
                list(&xs[n - i.unsigned_abs() as usize..])
            };
            assert_eq!(env.eval(&format!("take(xs, {i})")), want, "take {xs:?} {i}");
            let want = if i.unsigned_abs() as usize > n {
                format!("failure(index, {i}, {n})")
            } else if i >= 0 {
                list(&xs[i as usize..])
            } else {
                list(&xs[..n - i.unsigned_abs() as usize])
            };
            assert_eq!(env.eval(&format!("drop(xs, {i})")), want, "drop {xs:?} {i}");
            // spans with positive ends
            let (a, b) = (r.int(1, len + 1), r.int(0, len));
            let want = if b + 1 < a {
                format!("failure(index, {b}, {n})")
            } else {
                list(&xs[(a - 1) as usize..b.max(a - 1) as usize])
            };
            assert_eq!(env.eval(&format!("xs[{a}:{b}]")), want, "{xs:?}[{a}:{b}]");
        }
        let mut firsts: Vec<i64> = vec![];
        for &x in &xs {
            if !firsts.contains(&x) {
                firsts.push(x);
            }
        }
        assert_eq!(env.eval("unique(xs)"), list(&firsts), "unique {xs:?}");
        let mut rev = xs.clone();
        rev.reverse();
        assert_eq!(env.eval("reverse(xs)"), list(&rev));
        assert_eq!(env.eval("reverse(reverse(xs)) == xs"), "true");
        if n > 0 {
            assert_eq!(env.eval("max(xs)"), xs.iter().max().unwrap().to_string());
            assert_eq!(env.eval("min(xs)"), xs.iter().min().unwrap().to_string());
            assert_eq!(env.eval("join(take(xs, 1), rest(xs)) == xs"), "true");
            assert_eq!(env.eval("append(most(xs), last(xs)) == xs"), "true");
        }
    }
}

/// A random nested list of integers, as source text, its flattening to every depth, and its depth.
fn nested(r: &mut Rng, level: u32, out: &mut Vec<(i64, u32)>) -> String {
    let n = r.below(4);
    let mut items = vec![];
    for _ in 0..n {
        if level < 5 && r.below(3) == 0 {
            items.push(nested(r, level + 1, out));
        } else {
            let v = r.int(0, 9);
            out.push((v, level));
            items.push(v.to_string());
        }
    }
    format!("[{}]", items.join(", "))
}

#[test]
fn flatten_agrees_with_a_recursive_oracle() {
    let mut env = Env::new();
    let mut r = Rng(5);
    for _ in 0..80 {
        let mut leaves = vec![];
        let src = nested(&mut r, 0, &mut leaves);
        let all: Vec<i64> = leaves.iter().map(|l| l.0).collect();
        assert_eq!(env.eval(&format!("flatten({src})")), list(&all), "{src}");
        assert_eq!(env.eval(&format!("total({src}, inf)")), all.iter().sum::<i64>().to_string(), "{src}");
        assert_eq!(env.eval(&format!("flatten({src}) == flatten(flatten({src}, 1))")), "true", "{src}");
    }
}

#[test]
fn transpose_and_ranges_agree_with_oracles() {
    let mut env = Env::new();
    let mut r = Rng(6);
    for _ in 0..40 {
        let (rows, cols) = (1 + r.below(5) as usize, r.below(5) as usize);
        let m: Vec<Vec<i64>> = (0..rows).map(|_| (0..cols).map(|_| r.int(0, 99)).collect()).collect();
        let src = format!("[{}]", m.iter().map(|row| list(row)).collect::<Vec<_>>().join(", "));
        let t: Vec<String> = (0..cols).map(|j| list(&m.iter().map(|row| row[j]).collect::<Vec<_>>())).collect();
        assert_eq!(env.eval(&format!("transpose({src})")), format!("[{}]", t.join(", ")));
        assert_eq!(
            env.eval(&format!("same(transpose(transpose({src})), {src})")),
            if cols > 0 { "true" } else { "false" }
        );
        let (a, b) = (r.int(-20, 20), r.int(-20, 20));
        let s = [1, 2, 3, -1, -3][r.below(5) as usize];
        let want: Vec<i64> =
            if s > 0 { (a..=b).step_by(s as usize).collect() } else { (b..=a).rev().step_by((-s) as usize).collect() };
        assert_eq!(env.eval(&format!("range({a}, {b}, {s})")), list(&want), "range({a}, {b}, {s})");
    }
}

/// A random term over `f`, `g`, `x`, `y`, with its depth and leaf count by the definition.
fn tree(r: &mut Rng, level: u32) -> (String, u64, u64) {
    if level > 6 || r.below(3) == 0 {
        return (["x", "y", "1"][r.below(3) as usize].into(), 1, 1);
    }
    let n = r.below(4);
    let head = ["f", "g"][r.below(2) as usize];
    let kids: Vec<(String, u64, u64)> = (0..n).map(|_| tree(r, level + 1)).collect();
    let depth = 1 + kids.iter().map(|k| k.1).max().unwrap_or(0);
    let leaves = 1 + kids.iter().map(|k| k.2).sum::<u64>();
    (format!("{head}({})", kids.iter().map(|k| k.0.clone()).collect::<Vec<_>>().join(", ")), depth, leaves)
}

#[test]
fn depth_and_leaf_count_agree_with_their_definitions() {
    let mut env = Env::new();
    let mut r = Rng(7);
    for _ in 0..100 {
        let (src, d, n) = tree(&mut r, 0);
        assert_eq!(env.eval(&format!("depth({src})")), d.to_string(), "{src}");
        assert_eq!(env.eval(&format!("leaf_count({src})")), n.to_string(), "{src}");
    }
    // a hash-consed term with 2^100 paths: linear time, exact count
    let want: BigInt = (BigInt::from(1) << 101usize) - 1;
    assert_eq!(env.eval("rule dup(t_) := f(t, t)\nleaf_count(nest(dup, x, 100))"), want.to_string());
    assert_eq!(env.eval("depth(nest(dup, x, 100))"), "101");
}
