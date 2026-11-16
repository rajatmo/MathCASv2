//! Complex numbers with a rational imaginary part print in display mode as a sum with the sign of the imaginary part:
//! `a - 3*im/4`, never `a + -3*im/4`.
use mc_api::RefStore;
use mc_syntax::{read, Printer};
use std::time::{Duration, Instant};

fn display(src: &str) -> String {
    let store = RefStore::new();
    let ts = read(&store, src).expect("reads");
    let text = Printer::new(&store).with_display().expr(ts[0]);
    text
}

#[test]
fn negative_rational_imaginary_parts_print_with_a_minus() {
    let t0 = Instant::now();
    for (src, want) in [
        ("complex(5598/5365, -621/5365)", "5598/5365 - 621*im/5365"),
        ("complex(-2, -1/2)", "-2 - im/2"),
        ("complex(1/2, 3/4)", "1/2 + 3*im/4"),
        ("complex(1/2, -3)", "1/2 - 3im"),
        ("complex(0, -1/2)", "-im/2"),
        ("[complex(2/5, -16/45), complex(7/10, -1/10)]", "[2/5 - 16*im/45, 7/10 - im/10]"),
    ] {
        assert_eq!(display(src), want, "{src}");
    }
    assert!(t0.elapsed() < Duration::from_secs(10));
}
