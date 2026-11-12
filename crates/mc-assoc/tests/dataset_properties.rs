//! Datasets against Rust models on random tables (fixed seeds): grouped totals, means and counts with missing
//! values, the stable sort by one and two key columns, inner and left joins, pivot totals, and construction from rows
//! with absent keys.
mod common;
use common::{engine, eval};
use std::collections::BTreeMap;

/// A xorshift generator (fixed seeds: the tests are reproducible).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> i64 {
        (self.next() % n) as i64
    }
}

/// A random table: key column `k` in 0..keys, value column `v` (integers, some missing).
fn table(rng: &mut Rng, n: usize, keys: u64) -> Vec<(i64, Option<i64>)> {
    (0..n).map(|_| (rng.below(keys), if rng.below(5) == 0 { None } else { Some(rng.below(2001) - 1000) })).collect()
}

/// The source of a dataset with columns `"k"` and `"v"`.
fn source(t: &[(i64, Option<i64>)]) -> String {
    let ks: Vec<String> = t.iter().map(|r| r.0.to_string()).collect();
    let vs: Vec<String> = t.iter().map(|r| r.1.map_or("missing()".to_string(), |v| v.to_string())).collect();
    format!("dataset(assoc(\"k\" -> [{}], \"v\" -> [{}]))", ks.join(", "), vs.join(", "))
}

fn list<T: ToString>(xs: impl IntoIterator<Item = T>) -> String {
    format!("[{}]", xs.into_iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

#[test]
fn grouped_aggregates_match_a_model() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut e = engine(1, false);
    for (round, &(n, keys)) in [(0, 3), (1, 1), (20, 4), (200, 7), (1000, 31), (5000, 3)].iter().enumerate() {
        let t = table(&mut rng, n, keys);
        eval(&mut e, &format!("t = {}; null", source(&t)));
        // the model: keys in order of first occurrence
        let mut order: Vec<i64> = vec![];
        let mut groups: BTreeMap<i64, Vec<Option<i64>>> = BTreeMap::new();
        for &(k, v) in &t {
            if !groups.contains_key(&k) {
                order.push(k);
            }
            groups.entry(k).or_default().push(v);
        }
        let totals = order.iter().map(|k| groups[k].iter().flatten().sum::<i64>());
        let counts = order.iter().map(|k| groups[k].len());
        let present = order.iter().map(|k| groups[k].iter().flatten().count());
        let maxima = order.iter().map(|k| groups[k].iter().flatten().max().map_or("missing()".into(), |m| m.to_string()));
        let agg = "aggregate(t, \"k\", [\"v\" -> total, \"n\" -> [\"v\", count], \"m\" -> [\"v\", max], count])";
        assert_eq!(eval(&mut e, &format!("column({agg}, \"k\")")), list(&order), "round {round}");
        assert_eq!(eval(&mut e, &format!("column({agg}, \"v\")")), list(totals), "round {round}");
        assert_eq!(eval(&mut e, &format!("column({agg}, \"n\")")), list(present), "round {round}");
        assert_eq!(eval(&mut e, &format!("column({agg}, \"m\")")), list(maxima), "round {round}");
        assert_eq!(eval(&mut e, &format!("column({agg}, \"count\")")), list(counts), "round {round}");
        // the mean is exact: total / count of the present values
        assert_eq!(
            eval(&mut e, "column(aggregate(t, \"k\", \"v\" -> mean), \"v\") === map(d => if length(select(column(d, \"v\"), is_integer)) == 0 then missing() else total(select(column(d, \"v\"), is_integer))/length(select(column(d, \"v\"), is_integer)), values(group_by(t, \"k\")))"),
            "true",
            "round {round}"
        );
    }
}

#[test]
fn sort_by_is_a_stable_sort() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut e = engine(1, false);
    for &(n, keys) in &[(0, 1), (1, 1), (50, 5), (700, 40), (3000, 1000)] {
        let t = table(&mut rng, n, keys);
        eval(&mut e, &format!("t = add_column({}, \"i\", range({n})); null", source(&t)));
        let mut ix: Vec<usize> = (0..n).collect();
        ix.sort_by_key(|&i| t[i].0);
        assert_eq!(eval(&mut e, "column(sort_by(t, \"k\"), \"i\")"), list(ix.iter().map(|i| i + 1)), "{n}");
        assert_eq!(eval(&mut e, "column(sort_by(t, r => -r[\"k\"]), \"i\")"), {
            let mut ix: Vec<usize> = (0..n).collect();
            ix.sort_by_key(|&i| -t[i].0);
            list(ix.iter().map(|i| i + 1))
        });
        // two keys; missing values of v sort after the numbers
        let mut ix: Vec<usize> = (0..n).collect();
        ix.sort_by_key(|&i| (t[i].0, t[i].1.is_none(), t[i].1));
        assert_eq!(eval(&mut e, "column(sort_by(t, [\"k\", \"v\"]), \"i\")"), list(ix.iter().map(|i| i + 1)), "{n}");
    }
}

#[test]
fn joins_match_nested_loops() {
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let mut e = engine(1, false);
    for &(n, m, keys) in &[(0, 5, 3), (5, 0, 3), (30, 20, 6), (300, 200, 50)] {
        let (a, b) = (table(&mut rng, n, keys), table(&mut rng, m, keys));
        eval(&mut e, &format!("a = add_column({}, \"i\", range({n})); null", source(&a)));
        eval(&mut e, &format!("b = add_column({}, \"j\", range({m})); null", source(&b)));
        // the model: for each row of a in order, its partners in b in order
        let mut inner: Vec<(usize, usize)> = vec![];
        let mut left: Vec<(usize, Option<usize>)> = vec![];
        for (i, ra) in a.iter().enumerate() {
            let js: Vec<usize> = (0..m).filter(|&j| b[j].0 == ra.0).collect();
            if js.is_empty() {
                left.push((i, None));
            }
            for j in js {
                inner.push((i, j));
                left.push((i, Some(j)));
            }
        }
        assert_eq!(eval(&mut e, "column(join_on(a, b, \"k\"), \"i\")"), list(inner.iter().map(|p| p.0 + 1)));
        assert_eq!(eval(&mut e, "column(join_on(a, b, \"k\"), \"j\")"), list(inner.iter().map(|p| p.1 + 1)));
        let lj = left.iter().map(|p| p.1.map_or("missing()".to_string(), |j| (j + 1).to_string()));
        assert_eq!(eval(&mut e, "column(join_on(a, b, \"k\", how: left), \"j\")"), list(lj));
        assert_eq!(eval(&mut e, "columns(join_on(a, b, \"k\"))"), "[\"k\", \"v\", \"i\", \"v_right\", \"j\"]");
    }
}

#[test]
fn pivot_totals_and_rows_round_trip() {
    let mut rng = Rng(0x0ddb_1a5e_5bad_5eed);
    let mut e = engine(1, false);
    let t = table(&mut rng, 400, 6);
    eval(&mut e, &format!("t = add_column({}, \"c\", map(i => imod(i, 4), range(400))); null", source(&t)));
    // pivot totals agree with aggregating by both keys
    assert_eq!(
        eval(&mut e, "p = pivot(t, \"k\", \"c\", \"v\", total); a = aggregate(t, [\"k\", \"c\"], \"v\" -> total); \
            union(map(r => p[select(range(length(p)), i => p[i, \"k\"] === r[\"k\"])[1], r[\"c\"]] === r[\"v\"], rows(a)))"),
        "[true]"
    );
    // rows with absent keys build the same dataset as its columns
    assert_eq!(eval(&mut e, "dataset(rows(t)) === t"), "true");
    assert_eq!(
        eval(&mut e, "columns(dataset(map(r => select_keys(r, k => not is_missing(r[k])), rows(t))), columns(t)) === t"),
        "true"
    );
}
