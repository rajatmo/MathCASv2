//! Round 38: new core-language, list, uncertainty and interval functions. Each test carries the documented inputs and
//! variations of its own, checked by a second route: a construction from other functions, a brute-force search, a
//! closed form, or substitution.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};
use std::time::Duration;

/// Evaluate every statement in one fresh session (one worker thread, big stack).
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::Builder::new()
        .stack_size(256 << 20)
        .spawn(move || {
            let stop = StopFlag::new();
            let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
            let out = match e.eval(&s, &stop) {
                Ok(v) => v.printed,
                Err(err) => format!("error: {err}"),
            };
            let _ = tx.send(out);
        })
        .expect("spawn");
    rx.recv_timeout(Duration::from_secs(120)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

fn same(a: &str, b: &str) {
    assert_eq!(ev(a), ev(b), "{a}  vs  {b}");
}

/// Blocks of radius r with the edge repeated are the runs of the list padded by its end elements.
#[test]
fn array_filter_blocks() {
    assert_eq!(ev("array_filter(f, range(4), 1)"), "[f([1, 1, 2]), f([1, 2, 3]), f([2, 3, 4]), f([3, 4, 4])]");
    same("array_filter(f, range(10), 2)", "map(f, partition(join([1, 1], range(10), [10, 10]), 5, 1))");
    same("array_filter(total, [1, 4, 9, 16, 25], 1, padding: 0)", "map(i => total(take(drop([0, 1, 4, 9, 16, 25, 0], i - 1), 3)), range(5))");
    same("array_filter(f, [a, b, c, d], 1, padding: \"Periodic\")", "map(f, partition([d, a, b, c, d, a], 3, 1))");
    same("array_filter(f, [a, b, c, d], 1, padding: \"Reflected\")", "map(f, partition([b, a, b, c, d, c], 3, 1))");
    // two levels: the sums of the 3 x 3 blocks are the sums over the clamped neighbourhoods
    same(
        "array_filter(x => total(flatten(x)), [[1, 2, 3], [4, 5, 6], [7, 8, 9]], 1)",
        "{ let m = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]; let c = k => min(max(k, 1), 3); table(total(flatten(table(m[c(i + p), c(j + q)], p, [-1, 0, 1], q, [-1, 0, 1]))), i, 3, j, 3) }",
    );
    assert_eq!(
        ev("array_filter(f, [[1, 2, 3], [10, 20, 30]], [0, 1])"),
        "[[f([[1, 1, 2]]), f([[1, 2, 3]]), f([[2, 3, 3]])], [f([[10, 10, 20]]), f([[10, 20, 30]]), f([[20, 30, 30]])]]"
    );
    same("array_filter(f, [a, b, c], [[1, 0]])", "[f([a, a]), f([a, b]), f([b, c])]");
}

/// Reducing a level is applying the function to the vectors along it: total along level 1 is the column sums.
#[test]
fn array_reduce_levels() {
    assert_eq!(ev("array_reduce(mean, [[1, 2, 3, 4, 5], [6, 7, 8, 9, 10], [11, 12, 13, 14, 15]], 2)"), "[3, 8, 13]");
    assert_eq!(ev("array_reduce(mean, [[1, 2, 3, 4, 5], [6, 7, 8, 9, 10], [11, 12, 13, 14, 15]], 1)"), "[6, 7, 8, 9, 10]");
    same("array_reduce(total, [[1, 2, 3], [4, 5, 6]], 1)", "total([[1, 2, 3], [4, 5, 6]])");
    same("array_reduce(f, [[[1, 2], [3, 4]], [[5, 6], [7, 8]]], 2)", "{ let a = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]; table(f([a[i, 1, k], a[i, 2, k]]), i, 2, k, 2) }");
    same("array_reduce(f, [[[1, 2], [3, 4]], [[5, 6], [7, 8]]], [3, 1])", "{ let a = [[[1, 2], [3, 4]], [[5, 6], [7, 8]]]; table(f(table(a[i, j, k], k, 2, i, 2)), j, 2) }");
    same("array_reduce(apply(h), [[a, b], [c, d]], -1)", "[h(a, b), h(c, d)]");
}

/// The count of matching subsets is the length of the list of them; with overlaps every pair is counted.
#[test]
fn subset_counts() {
    assert_eq!(ev("subset_count([a, a, b, b, b, c, c, d], [a, b, c])"), "2");
    assert_eq!(ev("subset_count([1, 2, a, b, 3, c, d, 4, 5, 6], [_: Int, _: Sym])"), "4");
    assert_eq!(ev("subset_count([a, b, c, 1, 2], [_: Sym, _: Int], overlaps: true)"), "6");
    assert_eq!(ev("subset_count([a, a, b, b, b, c, c, d], [a] | [__])"), "3");
    same("subset_count([1, 2, 3, 4, 5, 6], [x_, y_] when x + y == 7)", "length(subset_cases([1, 2, 3, 4, 5, 6], [x_, y_] when x + y == 7))");
    same("subset_count([p, q, r, 1, 2, 3, 4], [_: Sym, _: Int], overlaps: true)", "3*4");
}

/// The shortest tour is the minimum over every ordering of the points (brute force over the permutations).
#[test]
fn shortest_tours() {
    assert_eq!(ev("find_shortest_tour([[2, 2], [5, 2], [5, 3], [4, 5], [4, 4], [3, 2]])"), "[5 + 2^(1/2) + 13^(1/2), [1, 4, 5, 3, 2, 6, 1]]");
    let brute = |pts: &str| {
        format!(
            "{{ let p = {pts}; let len = o => total(map(i => sqrt(total((p[o[i]] - p[o[i + 1]])^2)), range(length(o) - 1))); n(min(map(q => len(join([1], q, [1])), permutations(range(2, length(p))))), 20) }}"
        )
    };
    for pts in ["[[0, 0], [3, 1], [1, 4], [5, 5], [2, 2], [6, 0], [4, 3]]", "[[1.5, 0.25], [0.1, 2.0], [2.2, 2.2], [3.0, 0.5], [0.7, 0.9], [2.5, 1.4]]"] {
        let fst = ev(&format!("n(find_shortest_tour({pts})[1], 20)"));
        let bf = ev(&brute(pts));
        let (a, b): (f64, f64) = (fst.parse().unwrap(), bf.parse().unwrap());
        assert!((a - b).abs() < 1e-12 * b.abs(), "{pts}: {fst} vs {bf}");
        // the length given is the length of the tour given
        let check = format!("{{ let p = {pts}; let r = find_shortest_tour(p); let o = r[2]; n(r[1] - total(map(i => sqrt(total((p[o[i]] - p[o[i + 1]])^2)), range(length(o) - 1))), 20) }}");
        let d: f64 = ev(&check).parse().unwrap();
        assert!(d.abs() < 1e-12, "{pts}: {d}");
    }
    assert_eq!(ev("find_shortest_tour([1, 5, 2, 9, 3])[1]"), "16");
    assert_eq!(ev("find_shortest_tour([\"cat\", \"dog\", \"do\", \"cap\", \"dock\"])[1]"), "11");
    // a path between fixed ends: the square's three sides
    assert_eq!(ev("find_shortest_tour([[0, 0], [1, 0], [1, 1], [0, 1]], 1, 4)"), "[3, [1, 2, 3, 4]]");
    // infinite distances are no steps
    assert_eq!(ev("find_shortest_tour([1, 2, 3, 4], distance_function: (a, b) => if abs(a - b) == 2 then inf else 1)[1]"), "4");
    assert!(ev("find_shortest_tour([1, 2, 3], distance_function: (a, b) => if abs(a - b) == 1 then 1 else inf)").starts_with("failure(not_found"));
}

/// Names and symbols convert both ways.
#[test]
fn symbols_by_name() {
    assert_eq!(ev("symbol(\"x17\") === x17"), "true");
    assert_eq!(ev("symbol_name(symbol(\"y32\"))"), "\"y32\"");
    assert_eq!(ev("map(symbol_name, [x, alpha, beta1])"), "[\"x\", \"alpha\", \"beta1\"]");
    assert_eq!(ev("symbol(\"1 + 2\")"), "failure(argument, symbol, 1)");
    assert_eq!(ev("symbol(\"if\")"), "failure(argument, symbol, 1)");
}

/// The cleanup runs after the body, also when the body throws; the value is the body's.
#[test]
fn cleanup_after_body() {
    assert_eq!(ev("with_cleanup({ x = 7; x^2 }, unset(x))"), "49");
    assert_eq!(ev("with_cleanup({ x = 7; x^2 }, unset(x)); x"), "x");
    // init, body, cleanup in this order; the value is the body's
    assert_eq!(ev("reap(with_cleanup(sow(1), sow(2), sow(3)))"), "[2, [[1, 2, 3]]]");
    assert_eq!(ev("reap(catch(with_cleanup({ sow(1); throw(t) }, sow(2))))"), "[t, [[1, 2]]]");
}

/// break() ends the innermost loop with null, continue() its iteration; checked against closed forms.
#[test]
fn loop_break_continue() {
    assert_eq!(ev("{ var i = 1; while i <= 10 { if i > 2 then break(); i = i + 1 }; i }"), "3");
    assert_eq!(ev("{ var n = 1; while true { if n > 10 then break(); n = n + 1 } }"), "null");
    assert_eq!(ev("{ var s = 0; for k in range(100) { if is_odd(k) then continue(); s = s + k }; s }"), "2550");
    // the first k with k^2 > 1000 is 32: the loop over 1..100 stops there
    assert_eq!(ev("{ var last = 0; for k in range(100) { last = k; if k^2 > 1000 then break() }; last }"), "32");
    // only the innermost loop ends: 3 inner iterations (j = 1, 2, 3) for each of 4 outer ones
    assert_eq!(ev("{ var c = 0; for i in range(4) { for j in range(10) { if j > 3 then break(); c = c + 1 } }; c }"), "12");
    assert_eq!(ev("break()"), "failure(break)");
}

/// verbatim matches pattern constructs literally; hold_pattern keeps its pattern unevaluated.
#[test]
fn literal_and_held_patterns() {
    assert_eq!(ev("match_q(x_, verbatim(x_))"), "true");
    assert_eq!(ev("match_q(any(expression), verbatim(x_))"), "false");
    assert_eq!(ev("rewrite([f(x), f(x_), f(y_), f(x_, y_)], f(verbatim(x_)) -> x^2)"), "[f(x), x^2, f(y_), f(x_, y_)]");
    assert_eq!(ev("free_of(f(_), verbatim(_))"), "false");
    assert_eq!(ev("free_of(f(a), verbatim(_))"), "true");
    assert_eq!(ev("cases([1, b -> automatic, c -> 3], hold_pattern(_ -> automatic))"), "[b -> automatic]");
    assert_eq!(ev("match_q(a + b, hold_pattern(_ + _))"), "true");
    assert_eq!(ev("delete_cases([b, e, d, a, b, c, d], apply(alternatives, [b, c]))"), "[e, d, a, d]");
}

/// unitize decides zero rigorously: an enclosure away from 0, or simplify proving 0; undecided stays.
#[test]
fn unitize_values() {
    assert_eq!(ev("unitize(pi)"), "1");
    assert_eq!(ev("unitize(sqrt(2) + sqrt(3) - sqrt(5 + 2*sqrt(6)))"), "0");
    assert_eq!(ev("unitize(sqrt(2) + sqrt(3) - sqrt(5 + 2*sqrt(6)) + 10^-40)"), "1");
    assert_eq!(ev("[unitize(e, 3), unitize(pi, 3)]"), "[0, 1]");
    assert_eq!(ev("unitize([[1, 0, 2.5], [0.0, -3, 0]])"), "[[1, 0, 1], [0, 1, 0]]");
    assert_eq!(ev("unitize(x)"), "unitize(x)");
    // a symmetric pattern of zeros: unitize of m - transpose(m) is 0 exactly where m is symmetric
    same("unitize([[1, 2], [2, 5]] - transpose([[1, 2], [2, 5]]))", "[[0, 0], [0, 0]]");
}

/// Small list predicates against their definitions.
#[test]
fn list_predicates() {
    assert_eq!(ev("is_duplicate_free([1, 7, 8, 4, 3, 4, 1, 9, 9, 2])"), "false");
    same("is_duplicate_free([3, 1, 4, 1.0, 5])", "length(unique([3, 1, 4, 1.0, 5])) == 5");
    assert_eq!(ev("is_duplicate_free([[1, 2], [3, 0]], (a, b) => total(a) == total(b))"), "false");
    assert_eq!(ev("is_duplicate_free([[1, 2], [3, 1]], (a, b) => total(a) == total(b))"), "true");
    assert_eq!(ev("matching_dissimilarity([1, 0, 1, 1, 0], [1, 1, 0, 1, 1])"), "3/5");
    assert_eq!(ev("matching_dissimilarity([true, false, true], [true, true, false])"), "2/3");
    same("matching_dissimilarity([1, 1, 0, 0, 1, 0, 1], [0, 1, 0, 1, 1, 1, 1])", "hamming_distance([1, 1, 0, 0, 1, 0, 1], [0, 1, 0, 1, 1, 1, 1])/7");
    assert_eq!(ev("max_machine_number == (2 - 2^-52)*2.0^1023"), "true");
    assert_eq!(ev("max_machine_number*min_machine_number"), "3.9999999999999996");
}

/// Associations held in symbols are updated in place.
#[test]
fn associations_in_symbols() {
    assert_eq!(ev("a = assoc(x -> 1, y -> 2); associate_to(a, x -> 10); associate_to(a, [z -> 3]); a"), "assoc(x -> 10, y -> 2, z -> 3)");
    assert_eq!(ev("a = assoc(x -> 1, y -> 2, z -> 3); key_drop_from(a, [x, w]); a"), "assoc(y -> 2, z -> 3)");
    assert_eq!(ev("a = assoc(p -> 1); associate_to(a, assoc(q -> 2)) === a"), "true");
}

/// First- and second-order propagation against closed forms: for x y the variance is (3 0.1)^2 + (2 0.2)^2; for
/// exp(s) to second order the mean is exp(m) (1 + v/2) and the variance exp(2m) v (1 + v/2), v the variance of s.
#[test]
fn uncertainty_propagation() {
    assert_eq!(ev("around_replace(exp(s) + exp(s/2), s -> around(2., 0.3))"), "around(10.107337927389695, 2.624459103948052)");
    assert_eq!(ev("around_replace(sin(s), s -> around(1.56, 0.01), 2)"), "around(0.9998917231439548, 0.00012905437448170262)");
    assert_eq!(ev("around_replace(exp(s) + exp(r/2), [s, r] -> vector_around([2., 2.], [[0.3, 0.3], 0.5]))"), "around(10.107337927389695, 2.446208582020831)");
    let r = ev("{ let a = around_replace(x*y, [x -> around(2., 0.1), y -> around(3., 0.2)]); [a[1] - 6, a[2] - 0.5] }");
    assert_eq!(r, "[0.0, 0.0]", "x y: {r}");
    let r = ev("{ let a = around_replace(exp(s), s -> around(1/2, 1/5), 2); let v = 1/25; abs(n(a[1] - exp(1/2)*(1 + v/2))) < 10^-15 and abs(n(a[2]^2 - exp(1)*v*(1 + v/2))) < 10^-15 }");
    assert_eq!(r, "true", "exp second order: {r}");
    // a variable used twice is correlated with itself: s - s has no uncertainty
    assert_eq!(ev("around_replace(s - s + 1, s -> around(2., 0.3))"), "around(1.0, 0.0)");
    assert_eq!(ev("vector_around([1.8, 2.4], [[0.3, 0.4], 0.5])"), "vector_around([1.8, 2.4], [[0.09, 0.06], [0.06, 0.16000000000000003]])");
    same("vector_around([1, 2], [[1/10, 1/5], [[1, 1/2], [1/2, 1]]])", "vector_around([1, 2], [[1/100, 1/100], [1/100, 1/25]])");
}

/// Intersections against membership: a point is in the intersection exactly when it is in every interval.
#[test]
fn interval_intersections() {
    assert_eq!(ev("interval_intersection(interval([-2, 3]), interval([1, 4]))"), "interval([1, 3])");
    assert_eq!(ev("interval_intersection(interval([-2, 0]), interval([1, 4]))"), "interval()");
    assert_eq!(ev("interval_intersection(interval([-2, pi]), interval([e, 4]))"), "interval([e, pi])");
    assert_eq!(ev("interval_intersection(interval([-2, -1/2], [1/2, 2]), centered_interval(0, 1 + im))"), "interval([-1, -1/2], [1/2, 1])");
    let r = ev("{ let a = interval([-3, -1], [0, 2], [5, 8]); let b = interval([-2, 1], [1.5, 6]); let c = interval_intersection(a, b); \
        all_true(range(-4, 9, 1/4), x => interval_member_q(c, x) === (interval_member_q(a, x) and interval_member_q(b, x))) }");
    assert_eq!(r, "true");
}

/// Divisions: the documented cases, and for others every value is a multiple of the step and the range is covered.
#[test]
fn tick_divisions() {
    assert_eq!(ev("find_divisions([0, 1], 5)"), "[0, 1/5, 2/5, 3/5, 4/5, 1]");
    assert_eq!(ev("find_divisions([0.1, 0.9], 5)"), "[0, 1/5, 2/5, 3/5, 4/5, 1]");
    assert_eq!(ev("find_divisions([0, 10, pi], 5)"), "[0, pi, 2*pi, 3*pi, 4*pi]");
    assert_eq!(ev("find_divisions([0, 1], 5, 2)"), "[0, 1/4, 1/2, 3/4, 1]");
    assert_eq!(ev("find_divisions([0, 1], 5, 3)"), "[0, 1/3, 2/3, 1]");
    assert_eq!(ev("find_divisions([0, 1], [5, 5])[2][2]"), "[1/5, 1/4, 3/10, 7/20, 2/5]");
    for (a, b, k) in [("-3.7", "12.2", 4), ("0.003", "0.0171", 6), ("-250", "-17", 10)] {
        let r = ev(&format!(
            "{{ let d = find_divisions([{a}, {b}], {k}); let s = d[2] - d[1]; [d[1] <= {a}, last(d) >= {b}, all_true(d, x => is_integer(x/s)), member_of([1, 2, 5], s/10^floor(n(log(s)/log(10))))] }}"
        ));
        assert_eq!(r, "[true, true, true, true]", "{a} {b} {k}");
    }
}
