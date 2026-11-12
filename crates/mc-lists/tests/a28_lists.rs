//! Round 28: the matrix builders, triangular parts, partitions, cellular automata and number
//! formatting functions of the prelude section of round 28. Expected values are identities or hand derivations.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

/// Evaluate on a detached thread (one worker thread, fresh session); `TIMEOUT` after 30 s.
fn ev(src: &str) -> String {
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let s = src.to_string();
    std::thread::spawn(move || {
        let stop = StopFlag::new();
        let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
        let out = match e.eval(&s, &stop) {
            Ok(v) => v.printed,
            Err(err) => format!("error: {err}"),
        };
        let _ = tx.send(out);
    });
    rx.recv_timeout(std::time::Duration::from_secs(30)).unwrap_or_else(|_| "TIMEOUT".to_string())
}

#[test]
fn array_pad_sides_levels_and_modes() {
    assert_eq!(ev("array_pad([1, 2, 3], 1)"), "[0, 1, 2, 3, 0]");
    assert_eq!(ev("array_pad([1, 2, 3], [2, 4])"), "[0, 0, 1, 2, 3, 0, 0, 0, 0]");
    assert_eq!(ev("array_pad(range(10), -2)"), "[3, 4, 5, 6, 7, 8]");
    assert_eq!(ev("array_pad([[1, 2], [3, 4]], [[1], [0]])"), "[[0, 0], [1, 2], [3, 4], [0, 0]]");
    assert_eq!(ev("array_pad([1, 2, 3], 2, \"Fixed\")"), "[1, 1, 1, 2, 3, 3, 3]");
    assert_eq!(ev("array_pad([a, b, c], 4, \"Periodic\")"), "[c, a, b, c, a, b, c, a, b, c, a]");
    assert_eq!(ev("array_pad([a, b, c], 4, \"Reflected\")"), "[a, b, c, b, a, b, c, b, a, b, c]");
}

#[test]
fn structured_matrices() {
    assert_eq!(ev("hankel_matrix([x, y, z], [z, a, b, c, d])"), "[[x, y, z, a, b], [y, z, a, b, c], [z, a, b, c, d]]");
    assert_eq!(ev("toeplitz_matrix([1, 2, 3, 4, 5], [1, 6, 7])"), "[[1, 6, 7], [2, 1, 6], [3, 2, 1], [4, 3, 2], [5, 4, 3]]");
    assert_eq!(ev("toeplitz_matrix([1 + 2*im, 3 + 4*im])"), "[[complex(1, 2), complex(3, -4)], [complex(3, 4), complex(1, 2)]]");
    assert_eq!(ev("center_array([1, 2, 3], 6, x)"), "[x, 1, 2, 3, x, x]");
    assert_eq!(ev("array_flatten([[[[1, 2], [3, 4]], 0], [0, [[5]]]])"), "[[1, 2, 0], [3, 4, 0], [0, 0, 5]]");
}

#[test]
fn matrix_entries_may_use_any_symbol() {
    // the index names of the helper are lexical: a matrix of symbols named i, j, row ... is untouched
    assert_eq!(ev("diagonal([[a, b, c], [d, e, f], [g, h, i]])"), "[a, e, i]");
    assert_eq!(ev("hankel_matrix([i, j])"), "[[i, j], [j, 0]]");
    assert_eq!(ev("upper_triangularize([[i, 2], [3, j]])"), "[[i, 2], [0, j]]");
}

#[test]
fn diagonals_and_triangular_parts() {
    assert_eq!(ev("diagonal([[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]], 2)"), "[3, 8]");
    assert_eq!(ev("diagonal([[a, b, c], [d, e, f], [g, h, i]], -1)"), "[d, h]");
    assert_eq!(ev("upper_triangularize([[1, 2, 3], [4, 5, 6], [7, 8, 9]], 1)"), "[[0, 2, 3], [0, 0, 6], [0, 0, 0]]");
    assert_eq!(ev("lower_triangularize([[1.25, 3.2], [7.9, -1.4]])"), "[[1.25, 0.0], [7.9, -1.4]]");
    assert_eq!(ev("upper_triangular_matrix_q([[1, 2], [0, 3]])"), "true");
    assert_eq!(ev("lower_triangular_matrix_q([[1, 2], [0, 3]])"), "false");
    assert_eq!(ev("diagonal_matrix_q([[0, 1], [0, 0]], 1)"), "true");
    assert_eq!(ev("diagonal(x)"), "diagonal(x)");
}

#[test]
fn minors_adjugate_and_projections() {
    assert_eq!(ev("minors([[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]], 2)"), "[[-4, -8, -12, -4, -8, -4], [-8, -16, -24, -8, -16, -8], [-4, -8, -12, -4, -8, -4]]");
    assert_eq!(ev("minors([[1, 2], [3, 4]], 0)"), "[[1]]");
    assert_eq!(ev("adjugate([[15, 3, 4], [3, 15, 11], [-2, 1, 7]])"), "[[94, -17, -27], [-43, 113, -153], [33, -21, 216]]");
    // m . adjugate(m) = det(m) I, also for a singular matrix
    assert_eq!(ev("dot([[1, 2], [2, 4]], adjugate([[1, 2], [2, 4]]))"), "[[0, 0], [0, 0]]");
    assert_eq!(ev("projection([5, 6, 7], [1, 1, 1])"), "[6, 6, 6]");
    assert_eq!(ev("projection([3 + im, 2, 2 - im], [2, 4, 5*im])"), "[complex(2/5, -16/45), complex(4/5, -32/45), complex(8/9, 1)]");
    assert_eq!(ev("orthogonalize([[1, 0, 1], [1, 1, 1]])"), "[[(1/2)*2^(1/2), 0, (1/2)*2^(1/2)], [0, 1, 0]]");
    assert_eq!(ev("orthogonalize([[1, 2], [2, 4]])"), "[[(1/5)*5^(1/2), (2/5)*5^(1/2)]]");
    assert_eq!(ev("euclidean_distance([1, 2, 3], [2, 4, 6])"), "14^(1/2)");
    assert_eq!(ev("range_space([[1, 2, 3], [4, 5, 6], [7, 8, 9]])"), "[[1, 0, -1], [0, 1, 2]]");
}

#[test]
fn partitions_and_automata() {
    assert_eq!(ev("integer_partitions(5)"), "[[5], [4, 1], [3, 2], [3, 1, 1], [2, 2, 1], [2, 1, 1, 1], [1, 1, 1, 1, 1]]");
    assert_eq!(ev("integer_partitions(8, [3])"), "[[6, 1, 1], [5, 2, 1], [4, 3, 1], [4, 2, 2], [3, 3, 2]]");
    assert_eq!(ev("length(integer_partitions(20)) == partitions_p(20)"), "true");
    assert_eq!(ev("integer_partitions(50, all, [6, 9, 20])"), "[[20, 9, 9, 6, 6], [20, 6, 6, 6, 6, 6]]");
    assert_eq!(ev("cellular_automaton(30, [[1], 0], 3)"), "[[0, 0, 0, 1, 0, 0, 0], [0, 0, 1, 1, 1, 0, 0], [0, 1, 1, 0, 0, 1, 0], [1, 1, 0, 1, 1, 1, 1]]");
    // rule 90 from one cell: Pascal's triangle mod 2, 2^(number of ones in t) cells alive at step t
    assert_eq!(ev("map(row => total(row), cellular_automaton(90, [[1], 0], 7))"), "[1, 2, 2, 4, 2, 4, 4, 8]");
    assert_eq!(ev("cellular_automaton(90, [[1], 0], [[3]])"), "[1, 0, 1, 0, 1, 0, 1]");
    assert_eq!(ev("cellular_automaton([30, 2, 1], [[1], 0], 1) == cellular_automaton(30, [[1], 0], 1)"), "true");
}

#[test]
fn bell_polynomials_give_stirling_numbers() {
    assert_eq!(ev("bell_y(4, 2, [x1, x2, x3])"), "3*x2^2 + 4*x1*x3");
    assert_eq!(ev("map(m => map(k => bell_y(m, k, constant_array(1, m)), range(0, m)), range(0, 5))"), "[[1], [0, 1], [0, 1, 1], [0, 1, 3, 1], [0, 1, 7, 6, 1], [0, 1, 15, 25, 10, 1]]");
    assert_eq!(ev("map(k => bell_y(5, k, map(j => (-1)^j*factorial(j), range(0, 5))), range(0, 5))"), "[0, 24, -50, 35, -10, 1]");
    assert_eq!(ev("bell_y(5, [1, 1, 1, 1, 1])"), "52");
}

#[test]
fn numbers_names_and_strings() {
    assert_eq!(ev("cube_root(-27)"), "-3");
    assert_eq!(ev("cube_root(128.)"), "5.039684199579492");
    assert_eq!(ev("real_exponent(1000)"), "3.0");
    assert_eq!(ev("real_exponent(123.456)"), "2.0915122016277716");
    assert_eq!(ev("number_decompose(100000, [86400, 3600, 60, 1])"), "[1, 3, 46, 40]");
    assert_eq!(ev("number_decompose(-99, [10, 5, 2, 1])"), "[-9, -1, -2, 0]");
    assert_eq!(ev("number_decompose(99/2, [10, 5, 2, 1])"), "[4, 1, 2, 1/2]");
    assert_eq!(ev("integer_string(255, 16)"), "\"ff\"");
    assert_eq!(ev("integer_string(5, 2, 8)"), "\"00000101\"");
    assert_eq!(ev("integer_string(12345, 10, 3)"), "\"345\"");
    assert_eq!(ev("integer_name(1234567)"), "\"1 million 234 thousand 567\"");
    assert_eq!(ev("integer_name(-544)"), "\"negative five hundred forty\u{2010}four\"");
    assert_eq!(ev("integer_name(10990089, \"Words\")"), "\"ten million, nine hundred ninety thousand, eighty\u{2010}nine\"");
    assert_eq!(ev("indexed([[a, b, c], [d, e, f]], [1, 2])"), "b");
    assert_eq!(ev("indexed(x, 1)"), "indexed(x, 1)");
}

#[test]
fn sequence_functions_and_optimization() {
    assert_eq!(ev("find_sequence_function([1, 1, 2, 3, 5, 8, 13], n)"), "fibonacci(n)");
    assert_eq!(ev("find_sequence_function([2, 5, 10, 17, 26, 37, 50], n)"), "1 + n^2");
    assert_eq!(ev("find_sequence_function([1, 2, 6, 24, 120, 720, 5040], n)"), "factorial(n)");
    assert_eq!(ev("linear_optimization(x + y, [x + 2*y >= 3, x >= 0, y >= 0], [x, y])"), "[x -> 0, y -> 3/2]");
    assert_eq!(ev("convex_optimization(x^2 + y^2, [x + y >= 2], [x, y])"), "[x -> 1, y -> 1]");
}

#[test]
fn prime_zeta_and_constants() {
    // P(2) = 0.452247420041065498506543364832247934173...
    assert_eq!(ev("n(prime_zeta_p(2), 30)"), "0.452247420041065498506543364832248");
    assert_eq!(ev("n(golden_ratio, 30)"), "1.6180339887498948482045868343656381");
    assert_eq!(ev("n(golden_ratio)"), "1.618033988749895");
    assert_eq!(ev("round(machine_precision)"), "16");
}

#[test]
fn display_wrappers_stay_and_numeric_arrays_normalize() {
    assert_eq!(ev("percent_form(0.35)"), "percent_form(0.35)");
    assert_eq!(ev("base_form(33, 2)"), "base_form(33, 2)");
    assert_eq!(ev("matrix_form([[1, 2], [3, 4]])"), "matrix_form([[1, 2], [3, 4]])");
    assert_eq!(ev("normal(numeric_array([0, 1, 2], \"UnsignedInteger8\"))"), "[0, 1, 2]");
    assert_eq!(ev("normal(numeric_array([0, 1000, 2000], \"UnsignedInteger8\", \"ClipAndCoerce\"))"), "[0, 255, 255]");
    assert_eq!(ev("normal(numeric_array([0.1, 1.1, 2.1], \"UnsignedInteger8\", \"Round\"))"), "[0, 1, 2]");
}

#[test]
fn later_forms() {
    assert_eq!(ev("integer_partitions(1/2)"), "[]");
    assert_eq!(ev("integer_partitions(1/2, all, [1/6, 1/3])"), "[[1/3, 1/6], [1/6, 1/6, 1/6]]");
    assert_eq!(ev("cube_root([0, 8, inf])"), "[0, 2, inf]");
    assert_eq!(ev("center_array([1, 2, 3], [5, 5])"), "[[0, 0, 0, 0, 0], [0, 0, 0, 0, 0], [0, 1, 2, 3, 0], [0, 0, 0, 0, 0], [0, 0, 0, 0, 0]]");
    assert_eq!(ev("center_array([1, 2, 3, 4, 5, 6, 7, 8, 9], 3)"), "[4, 5, 6]");
    assert_eq!(ev("array_pad([a, b, c], 4, \"ReversedNegation\")"), "[c, -c, -b, -a, a, b, c, -c, -b, -a, a]");
    assert_eq!(ev("euclidean_distance([a, b], [a, c])"), "abs(b - c)");
    assert_eq!(ev("projection([1, 2, 3], [[1, 1, 0], [0, 1, 0]])"), "[1, 2, 0]");
    assert_eq!(ev("linear_optimization([1, 1], [[[1, 0], [0, 1]], [0, 0]], [[[1, 2]], [-3]])"), "[0, 3/2]");
    assert_eq!(ev("linear_optimization(dot([3, 4], v), [dot([1, -2], v) <= 3, dot([-1, 1], v) <= 2], v)"), "[v -> [-7, -5]]");
    assert_eq!(ev("map(n => bell_y(transpose([map(i => 1/factorial(n - i + 1), range(n)), map(i => factorial(i), range(n))])), range(1, 7))"), "[1, 2, 5, 14, 42, 132, 429]");
    assert_eq!(ev("integer_string(1988, \"Roman\")"), "\"MCMLXXXVIII\"");
    assert_eq!(ev("find_sequence_function([1 + a, 1 + a^2, 1 + a^3, 1 + a^4, 1 + a^5], n)"), "1 + a^n");
}
