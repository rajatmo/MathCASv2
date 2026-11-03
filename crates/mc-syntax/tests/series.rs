//! Truncated power series `series_data(x, x0, [c...], v, p)` print as a sum with an order term (decision 9.3 of the
//! library-gaps package); the text reads back as that sum with an `O(...)` call, and other shapes print as calls.
mod common;
use common::raw;
use mc_api::RefStore;
use mc_syntax::{read, Printer};

fn print(src: &str) -> String {
    let store = RefStore::new();
    let ts = read(&store, src).expect("reads");
    let text = Printer::new(&store).expr(ts[0]);
    text
}

fn lower(src: &str) -> String {
    let store = RefStore::new();
    let ts = read(&store, src).expect("reads");
    raw(&store, ts[0])
}

#[test]
fn series_print_as_sums() {
    for (src, want) in [
        ("series_data(x, 0, [1, 1, 1/2], 0, 3)", "1 + x + x^2/2 + O(x^3)"),
        ("series_data(x, 0, [1, 0, -1/6], 1, 4)", "x - x^3/6 + O(x^4)"),
        ("series_data(x, 0, [-1, 2], -1, 1)", "-x^-1 + 2 + O(x)"),
        ("series_data(x, 0, [3/4, -5/7], -2, 0)", "3/(4*x^2) - 5/(7*x) + O(1)"),
        ("series_data(x, 0, [], 3, 3)", "O(x^3)"),
        ("series_data(x, 0, [0, 0], 0, 2)", "O(x^2)"),
        ("series_data(x, 0, [1], 0, 4)", "1 + O(x^4)"),
        ("series_data(x, 1, [e, e], 0, 2)", "e + e*(x - 1) + O((x - 1)^2)"),
        ("series_data(t, -1/2, [1, 1], 0, 2)", "1 + (t + 1/2) + O((t + 1/2)^2)"),
        ("series_data(x, a, [a + b, -c, mul(-1, c)], 0, 3)", "(a + b) - c*(x - a) - c*(x - a)^2 + O((x - a)^3)"),
        ("series_data(x, 0, [123456789012345678901234567890, 1], 0, 2)", "123456789012345678901234567890 + x + O(x^2)"),
        ("2*series_data(x, 0, [1, 1], 0, 2)", "2*(1 + x + O(x^2))"),
        ("f(series_data(x, 0, [], 1, 1))", "f(O(x))"),
        // inconsistent shapes are plain calls
        ("series_data(x, 0, [1, 2, 3], 0, 2)", "series_data(x, 0, [1, 2, 3], 0, 2)"),
        ("series_data(x, 0, [1], 2, 1)", "series_data(x, 0, [1], 2, 1)"),
        ("series_data(x, 0, 1, 0, 2)", "series_data(x, 0, 1, 0, 2)"),
        ("series_data(x, 0, [1], 0, 1.5)", "series_data(x, 0, [1], 0, 1.5)"),
        ("series_data(x, 0, [1], 0)", "series_data(x, 0, [1], 0)"),
    ] {
        assert_eq!(print(src), want, "{src}");
    }
}

#[test]
fn printed_series_read_back_as_sums_with_an_order_term() {
    for (src, want) in [
        ("series_data(x, 0, [1, 1, 1/2], 0, 3)", "add(1, x, mul(pow(x, 2), pow(2, -1)), O(pow(x, 3)))"),
        ("series_data(x, 0, [3/4], -2, 0)", "add(mul(3, pow(mul(4, pow(x, 2)), -1)), O(1))"),
        ("series_data(x, 1, [2], 1, 2)", "add(mul(2, add(x, -1)), O(pow(add(x, -1), 2)))"),
    ] {
        assert_eq!(lower(&print(src)), want, "{src}");
    }
}
