//! Round 40: normalized vectors, knapsacks with several bounds, keys added to associations held in `var` locals.
use mc_api::{Engine, StopFlag};
use mc_engine::CoreEngine;

fn ev(src: &str) -> String {
    let mut e = CoreEngine::new();
    e.eval(src, &StopFlag::new()).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn normalized_vectors() {
    assert_eq!(ev("normalized([1, 5, 1])"), "[(1/9)*3^(1/2), (5/9)*3^(1/2), (1/9)*3^(1/2)]");
    assert_eq!(ev("normalized([x, y])"), "[x*(abs(x)^2 + abs(y)^2)^(-1/2), y*(abs(x)^2 + abs(y)^2)^(-1/2)]");
    assert_eq!(ev("normalized([x, y], f)"), "[x/f([x, y]), y/f([x, y])]");
    // the result has norm 1 (exact complex entries, and 30 digits), and points the same way (v / normalized(v) is
    // one positive number for every entry)
    assert_eq!(ev("simplify(norm(normalized([1, 2*im, 3, 4*im, 5, 6*im])))"), "1");
    assert_eq!(ev("abs(norm(normalized(n([2, -7, 1/3, 11], 30))) - 1) < 10^-28"), "true");
    assert_eq!(ev("v = [3, -4, 12]; u = normalized(v); [length(union(v/u)), (v/u)[1] > 0]"), "[1, true]");
    // a zero vector stays; a number z is z/abs(z)
    assert_eq!(ev("normalized([0, 0, 0])"), "[0, 0, 0]");
    assert_eq!(ev("normalized(3 + 4*im) == 3/5 + 4*im/5"), "true");
    // a matrix by its 2-norm, a polynomial by an inner-product norm: the norm of the result is 1
    assert_eq!(ev("abs(norm(n(normalized([[1, 2], [4, 5]], norm))) - 1) < 10^-12"), "true");
    assert_eq!(ev("normalized(1 + x + x^2, &(integrate(&1^2, [x, -1, 1])))"), "(5/22)*(1 + x + x^2)");
    assert_eq!(ev("p = normalized(x^3 - x, &(sqrt(integrate(&1^2, [x, -1, 1])))); integrate(p^2, [x, -1, 1])"), "1");
    // the values of a time series, the times kept
    assert_eq!(
        ev("normalized(time_series([1, 2, 3], [0.1, 0.3]))(\"Path\")"),
        "[[0.1, (1/14)*14^(1/2)], [0.2, (1/7)*14^(1/2)], [0.3, (3/14)*14^(1/2)]]"
    );
    // anything else stays
    assert_eq!(ev("normalized(x)"), "normalized(x)");
}

/// Items `[payoff, cost, max count]`: every count vector within the bounds, by enumeration; the best payoff and
/// the count vectors reaching it.
fn brute(items: &[(i64, i64, i64)], caps: (i64, i64, i64)) -> Vec<Vec<i64>> {
    let mut best = i64::MIN;
    let mut at: Vec<Vec<i64>> = vec![];
    let mut cur = vec![0i64; items.len()];
    fn walk(i: usize, items: &[(i64, i64, i64)], caps: (i64, i64, i64), cur: &mut Vec<i64>, tot: (i64, i64, i64), best: &mut i64, at: &mut Vec<Vec<i64>>) {
        if i == items.len() {
            if tot.0 > *best {
                *best = tot.0;
                at.clear();
            }
            if tot.0 == *best {
                at.push(cur.clone());
            }
            return;
        }
        let (p, c, m) = items[i];
        let mut k = 0;
        while k <= m && tot.0 + k * p <= caps.0 && tot.1 + k * c <= caps.1 && tot.2 + k <= caps.2 {
            cur[i] = k;
            walk(i + 1, items, caps, cur, (tot.0 + k * p, tot.1 + k * c, tot.2 + k), best, at);
            k += 1;
        }
        cur[i] = 0;
    }
    walk(0, items, caps, &mut cur, (0, 0, 0), &mut best, &mut at);
    at
}

fn knap(items: &[(i64, i64, i64)], caps: (i64, i64, i64)) -> String {
    let its: Vec<String> = items.iter().map(|(p, c, m)| format!("[{p}, {c}, {m}]")).collect();
    ev(&format!("knapsack_solve([{}], [{}, {}, {}])", its.join(", "), caps.0, caps.1, caps.2))
}

fn list(v: &[i64]) -> String {
    format!("[{}]", v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", "))
}

#[test]
fn knapsack_with_several_bounds() {
    // payoff, cost and count bounds as a list, rules or an association; named items keep their names
    assert_eq!(ev("knapsack_solve([[3, 2, 7], [2, 1, 5]], [15, 10, 5])"), "[5, 0]");
    assert_eq!(ev("knapsack_solve([\"forks\" -> [3, 4], \"spoons\" -> [2, 3, 2]], [30, 30])"), "[\"forks\" -> 6, \"spoons\" -> 2]");
    assert_eq!(
        ev("knapsack_solve(assoc(\"forks\" -> assoc(\"Payoff\" -> 3, \"Cost\" -> 4), \"spoons\" -> assoc(\"Payoff\" -> 2, \"Cost\" -> 3, \"MaxCount\" -> 2)), [30, 30, 5])"),
        "assoc(\"forks\" -> 5, \"spoons\" -> 0)"
    );
    assert_eq!(ev("knapsack_solve([[10, 1], [8, 2, 20], [9, 7, 6]], [25, 30, 10])"), "[0, 2, 1]");
    assert_eq!(
        ev("knapsack_solve([[8, 9, 2], [4, 4, 5], [5, 6]], assoc(\"MaxTotalCost\" -> 30, \"MaxTotalCount\" -> 4, \"MaxTotalPayoff\" -> 35))"),
        "[2, 0, 2]"
    );
    assert_eq!(ev("knapsack_solve([[8, 1], [6, 2, 20], [9, 7, 6]], [inf, 30, 5])"), "[1, 0, 4]");
    // several optimal choices: unevaluated
    assert_eq!(
        ev("knapsack_solve([[8, 9, 2], [4, 4, 5], [5, 6]], [\"MaxTotalPayoff\" -> 35, \"MaxTotalCost\" -> 30])"),
        "knapsack_solve([[8, 9, 2], [4, 4, 5], [5, 6]], [\"MaxTotalPayoff\" -> 35, \"MaxTotalCost\" -> 30])"
    );
    // variations checked against an enumeration of every count vector
    let cases: [(&[(i64, i64, i64)], (i64, i64, i64)); 5] = [
        (&[(7, 5, 3), (4, 3, 6), (2, 1, 9)], (40, 25, 8)),
        (&[(9, 4, 2), (5, 2, 5), (3, 3, 4), (1, 1, 10)], (31, 17, 9)),
        (&[(6, 7, 4), (11, 9, 3)], (100, 50, 6)),
        (&[(3, 2, 7), (2, 1, 5)], (15, 30, 15)),
        (&[(5, 3, 4), (8, 5, 2), (13, 8, 1)], (24, 13, 4)),
    ];
    for (items, caps) in cases {
        let opt = brute(items, caps);
        let got = knap(items, caps);
        if opt.len() == 1 {
            assert_eq!(got, list(&opt[0]), "{items:?} {caps:?}");
        } else {
            assert!(got.starts_with("knapsack_solve("), "{items:?} {caps:?}: {got} (optima {opt:?})");
        }
    }
    // quantities in the unit of their bound
    assert_eq!(
        ev("knapsack_solve([[3, quantity(2, \"Minutes\"), 7], [2, quantity(60, \"Seconds\"), 5]], [15, quantity(10, \"Minutes\"), 5])"),
        "[5, 0]"
    );
}

#[test]
fn new_keys_of_associations_in_var_locals() {
    assert_eq!(ev("{ var b = assoc(\"z\" -> 1); b[\"w\"] = 5; b }"), "assoc(\"z\" -> 1, \"w\" -> 5)");
    assert_eq!(ev("{ var b = assoc(z -> 1); b[z] = 5; b }"), "assoc(z -> 5)");
    assert_eq!(ev("{ var b = assoc(); b[st(1, 2)] = 3; b[st(1, 2)] = b[st(1, 2)] + 1; b }"), "assoc(st(1, 2) -> 4)");
    // the same as an assignment to a symbol at the top level
    assert_eq!(ev("a = assoc(z -> 1); a[w] = 5; a"), ev("{ var a = assoc(z -> 1); a[w] = 5; a }"));
    assert_eq!(ev("{ var b = [1, 2]; b[2] = 5; b }"), "[1, 5]");
    assert_eq!(ev("{ var m = [[1, 2], [3, 4]]; m[2, 1] = 0; m }"), "[[1, 2], [0, 4]]");
    // a counting loop
    assert_eq!(ev("{ var t = assoc(); for x in [a, b, a, c, a] { t[x] = if key_exists(t, x) then t[x] + 1 else 1 }; t }"), "assoc(a -> 3, b -> 1, c -> 1)");
}
