//! Digits in base golden_ratio, coprimality in the Gaussian integers, Catalan numbers of large integers, the
//! financial numerals, integer partitions with parts that are not positive. Checked independently: the digits by
//! the sum of the powers they select, coprimality by the norms, Catalan numbers by factorials.
use mc_api::{Engine, StopFlag};
use mc_engine::{CoreEngine, EngineConfig};

fn eval(src: &str) -> String {
    let mut e = CoreEngine::with_config(EngineConfig { threads: 1, ..EngineConfig::default() });
    e.eval(src, &StopFlag::new()).map(|v| v.printed).unwrap_or_else(|err| panic!("{src}: {err}"))
}

#[test]
fn golden_ratio_digits() {
    assert_eq!(eval("real_digits(pi, golden_ratio, 20)"), "[[1, 0, 0, 0, 1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1, 0], 3]");
    assert_eq!(
        eval("table(real_digits(n, golden_ratio, 10), n, 5)"),
        "[[[1, 0, 0, 0, 0, 0, 0, 0, 0, 0], 1], [[1, 0, 0, 1, 0, 0, 0, 0, 0, 0], 2], [[1, 0, 0, 0, 1, 0, 0, 0, 0, 0], 3], [[1, 0, 1, 0, 1, 0, 0, 0, 0, 0], 3], [[1, 0, 0, 0, 1, 0, 0, 1, 0, 0], 4]]"
    );
    assert_eq!(eval("number_digit(pi, range(2, -4, -1), golden_ratio)"), "[1, 0, 0, 0, 1, 0, 0]");
    // the integers 1 to 30 are finite sums of the powers their digits select (exactly, with sqrt(5))
    assert_eq!(
        eval("table({ r = real_digits(n, golden_ratio, 40); simplify(sum(r[1][k]*((1 + sqrt(5))/2)^(r[2] - k), [k, 1, 40]) - n) }, n, 30)"),
        format!("[{}]", vec!["0"; 30].join(", "))
    );
    // pi: the selected powers come within the last place, phi^(3 - 60), of the value
    assert_eq!(
        eval("{ r = real_digits(pi, golden_ratio, 60); d = n(pi - sum(r[1][k]*golden_ratio^(r[2] - k), [k, 1, 60]), 40); 0 <= d and d < n(golden_ratio^(3 - 60), 40) }"),
        "true"
    );
    // no two adjacent ones (the greedy expansion)
    assert_eq!(eval("{ r = real_digits(e^3, golden_ratio, 80)[1]; every(range(79), k => r[k]*r[k + 1] == 0) }"), "true");
    // other bases and arguments are unchanged
    assert_eq!(eval("real_digits(1/3, 2, 4)"), "[[1, 0, 1, 0], -1]");
}

#[test]
fn coprime_in_the_gaussian_integers() {
    // 5 = (2 + i)(2 - i) and 7 (a Gaussian prime) have norms 25 and 49, which are coprime
    assert_eq!(eval("is_coprime(5, 7, gaussian_integers: true)"), "true");
    assert_eq!(eval("gcd(25, 49)"), "1");
    // 2 = -i (1 + i)^2 and 4 + 2 i = (1 + i)(3 - i) share 1 + i
    assert_eq!(eval("is_coprime(2, 4 + 2*im, gaussian_integers: true)"), "false");
    assert_eq!(eval("is_coprime(3, 6, gaussian_integers: true)"), "false");
    assert_eq!(eval("is_coprime(5, 7, gaussian_integers: false)"), "true");
}

#[test]
fn catalan_numbers_of_large_integers() {
    assert_eq!(eval("table(mod(catalan_number(2^n - 1), 10), n, 20)"), "[1, 5, 9, 5, 9, 5, 9, 7, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5]");
    assert_eq!(eval("{ m = 2^17 - 1; catalan_number(m) == factorial(2*m)/(factorial(m)*factorial(m + 1)) }"), "true");
    assert_eq!(eval("catalan_number(7/2)"), "(8192/315)/pi");
}

#[test]
fn financial_numerals() {
    // 195 ten-thousands, a zero, 650: one, hundred, nine, ten, five, ten-thousand, zero, six, hundred, five, ten
    assert_eq!(eval("to_character_code(integer_string(1950650, \"TraditionalChineseFinancial\"))"), "[22777, 20336, 29590, 25342, 20237, 33836, 38646, 38520, 20336, 20237, 25342]");
    assert_eq!(eval("to_character_code(integer_string(100010000, \"TraditionalChineseFinancial\"))"), "[22777, 20740, 38646, 22777, 33836]");
    assert_eq!(eval("to_character_code(integer_string(0, \"TraditionalChineseFinancial\"))"), "[38646]");
}

#[test]
fn partitions_with_parts_that_are_not_positive() {
    assert_eq!(eval("integer_partitions(5, 10, [1, -1])"), "[[-1, -1, 1, 1, 1, 1, 1, 1, 1], [-1, 1, 1, 1, 1, 1, 1], [1, 1, 1, 1, 1]]");
    // each one has the sum 5 and at most 10 parts; with j parts -1 there are 5 + j ones, 5 + 2 j <= 10
    assert_eq!(eval("map(p => [total(p), length(p) <= 10], integer_partitions(5, 10, [1, -1]))"), "[[5, true], [5, true], [5, true]]");
    // 0 from at most 4 of -1, 0, 2: zero to four zeros, or -1, -1, 2 with zero or one zero
    assert_eq!(eval("length(integer_partitions(0, 4, [2, -1, 0]))"), "7");
    assert_eq!(eval("integer_partitions(5, 2, [3, 2, 1])"), "[[3, 2]]");
}
