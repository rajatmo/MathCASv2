//! find_repeat by its definition: the shortest block b such that the list is b repeated with a partial last copy
//! allowed (xs[i] = b[i mod p] for every i, p the length of b), and with a number of copies k the shortest such block
//! with length(xs) >= k p (the empty list when there is none). Checked on seeded random lists made as a random block
//! repeated a random number of times plus a random beginning of it, and on random lists with no shorter period.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn eval(e: &mut CoreEngine, src: &str) -> String {
    match e.eval(src, &StopFlag::new()) {
        Ok(v) => v.printed,
        Err(err) => format!("error: {err}"),
    }
}

struct Lcg(u64);

impl Lcg {
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        lo + ((self.0 >> 33) % (hi - lo + 1) as u64) as i64
    }
}

fn list(v: &[i64]) -> String {
    format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

/// Whether xs is its first p elements repeated, a partial last copy allowed.
fn is_period(xs: &[i64], p: usize) -> bool {
    (0..xs.len()).all(|i| xs[i] == xs[i % p])
}

/// The answer the definition gives: the least period p with length >= k p, as the block, or [] when none.
fn expected(xs: &[i64], k: f64) -> Vec<i64> {
    (1..=xs.len()).find(|&p| xs.len() as f64 >= k * p as f64 && is_period(xs, p)).map(|p| xs[..p].to_vec()).unwrap_or_default()
}

/// Checks the returned block against the definition directly: its repetition covers the list, no shorter block's
/// does, and it fits k times.
fn check_definition(xs: &[i64], got: &[i64], k: f64) {
    if got.is_empty() {
        assert!((1..=xs.len()).all(|p| !(is_period(xs, p) && xs.len() as f64 >= k * p as f64)), "{xs:?}, {k}: [] but a block fits");
        return;
    }
    let p = got.len();
    assert!((0..xs.len()).all(|i| xs[i] == got[i % p]), "{xs:?}: {got:?} repeated is not the list");
    assert!((1..p).all(|q| !is_period(xs, q) || (xs.len() as f64) < k * q as f64), "{xs:?}: a block shorter than {got:?} repeats");
    assert!(xs.len() as f64 >= k * p as f64, "{xs:?}: {got:?} does not fit {k} times");
}

fn parse(s: &str) -> Vec<i64> {
    let inner = s.trim().trim_start_matches('[').trim_end_matches(']');
    if inner.trim().is_empty() {
        return vec![];
    }
    inner.split(',').map(|x| x.trim().parse().unwrap_or_else(|_| panic!("{s}"))).collect()
}

#[test]
fn a_block_and_a_partial_copy() {
    let mut e = CoreEngine::new();
    assert_eq!(eval(&mut e, "find_repeat([1, 2, 3, 4, 1, 2, 5, 4, 1, 2, 3])"), "[1, 2, 3, 4, 1, 2, 5, 4]");
    assert_eq!(eval(&mut e, "find_repeat([1, 2, 3])"), "[1, 2, 3]");
    assert_eq!(eval(&mut e, "find_repeat([1, 2, 3, 4, 1])"), "[1, 2, 3, 4]");
    assert_eq!(eval(&mut e, "find_repeat([1, 2, 3, 4, 1], 1.5)"), "[]");
    assert_eq!(eval(&mut e, "find_repeat(\"abcab\")"), "\"abc\"");
    assert_eq!(eval(&mut e, "find_repeat(\"abcab\", 2)"), "\"\"");
}

#[test]
fn random_repeated_blocks_by_the_definition() {
    let mut e = CoreEngine::new();
    let mut g = Lcg(17);
    let ks = [1.0, 1.5, 2.0, 3.0];
    for case in 0..320 {
        let p = g.int(1, 6) as usize;
        let block: Vec<i64> = (0..p).map(|_| g.int(0, 2)).collect();
        let copies = g.int(1, 4) as usize;
        let extra = g.int(0, p as i64 - 1) as usize;
        let xs: Vec<i64> = (0..copies * p + extra).map(|i| block[i % p]).collect();
        let got = parse(&eval(&mut e, &format!("find_repeat({})", list(&xs))));
        check_definition(&xs, &got, 1.0);
        assert_eq!(got, expected(&xs, 1.0), "{xs:?}");
        // with a number of copies
        let k = ks[case % ks.len()];
        let got = parse(&eval(&mut e, &format!("find_repeat({}, {k})", list(&xs))));
        check_definition(&xs, &got, k);
        assert_eq!(got, expected(&xs, k), "{xs:?}, {k}");
    }
}

#[test]
fn random_lists_without_a_period_are_their_own_block() {
    let mut e = CoreEngine::new();
    let mut g = Lcg(23);
    for _ in 0..80 {
        // a last element that occurs nowhere else: no period shorter than the list
        let n = g.int(1, 12) as usize;
        let mut xs: Vec<i64> = (0..n - 1).map(|_| g.int(0, 3)).collect();
        xs.push(9);
        let got = parse(&eval(&mut e, &format!("find_repeat({})", list(&xs))));
        check_definition(&xs, &got, 1.0);
        assert_eq!(got, xs);
        // two copies cannot fit
        assert_eq!(eval(&mut e, &format!("find_repeat({}, 2)", list(&xs))), "[]", "{xs:?}");
    }
}
