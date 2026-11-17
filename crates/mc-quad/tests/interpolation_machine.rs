//! The functions of `list_interpolation` and `function_interpolation` at points of machine reals are answered by a
//! native; its value must be the language definition's (`nl_li_lang`) bit for bit: grids of one to three
//! dimensions, exact and machine values and coordinates, orders 0 to 4, points inside, on and outside the nodes.
use mc_api::{Engine, StopFlag};
use std::time::{Duration, Instant};

fn eval(e: &mut mc_engine::CoreEngine, src: &str) -> String {
    let stop = StopFlag::new();
    e.eval(src, &stop).map(|r| r.printed).unwrap_or_else(|err| format!("error: {err:?}"))
}

#[test]
fn native_values_are_the_language_values() {
    let t0 = Instant::now();
    let mut e = mc_engine::CoreEngine::new();
    let setups = [
        // one dimension: exact values, exact coordinates, irregular coordinates, machine values
        ("v = [1, 4, 9, 16, 25, 36, 49]; g = [range(7)]", "[x]"),
        ("v = [1, 3/2, -2, 7, 0, 5]; g = [[0, 1/3, 1, 2, 7/2, 4]]", "[x]"),
        ("v = map(t => sin(1.3*t), range(9)); g = [map(t => 0.25*t, range(9))]", "[x]"),
        // two dimensions: an identity matrix (exact zeros and ones), machine samples on a rational grid
        ("v = identity_matrix(5); g = [range(5), [0, 1/4, 1/2, 3/4, 1]]", "[x, y]"),
        ("v = table(sin(0.7*i)*cos(0.3*j) + 0.1*i*j, [i, 6], [j, 5]); g = [range(6), map(t => (t - 1)/4, range(5))]", "[x, y]"),
        // three dimensions
        ("v = table(i^2 - j*k + 0.5*k, [i, 4], [j, 5], [k, 4]); g = [range(4), range(5), [0.0, 0.5, 1.5, 2.0]]", "[x, y, z]"),
    ];
    let points = [-0.4, 0.0, 0.3, 1.0, 1.7, 2.25, 3.0, 3.6, 4.0, 5.9, 7.5];
    let mut bad = vec![];
    let mut checked = 0;
    for (setup, vars) in setups {
        eval(&mut e, setup);
        let d = vars.matches(',').count() + 1;
        for k in 0..=4 {
            for (n, &p) in points.iter().enumerate() {
                let xs: Vec<String> = (0..d).map(|i| format!("{:?}", points[(n + 3 * i) % points.len()] * 0.5 + p * 0.5)).collect();
                let xs = format!("[{}]", xs.join(", "));
                let ks = format!("[{}]", vec![k.to_string(); d].join(", "));
                let native = eval(&mut e, &format!("nl_li_machine(v, g, {ks}, {xs})"));
                let lang = eval(&mut e, &format!("nl_li_lang(v, g, {ks}, {xs})"));
                // the same number, and printed alike (exact against machine shows in the printed form)
                let same = eval(&mut e, &format!("nl_li_machine(v, g, {ks}, {xs}) === nl_li_lang(v, g, {ks}, {xs})"));
                if same != "true" || native != lang {
                    bad.push(format!("{setup} k = {k} at {xs}: native {native}, language {lang}"));
                }
                checked += 1;
            }
        }
    }
    assert!(bad.is_empty(), "{} of {checked} differ:\n{}", bad.len(), bad.join("\n"));
    // through the functions the library returns, too
    assert_eq!(eval(&mut e, "f = list_interpolation([[1, 2, 3], [4, 5, 6], [7, 8, 10]]); f(1.5, 2.5) === nl_li_lang([[1, 2, 3], [4, 5, 6], [7, 8, 10]], [range(3), range(3)], [3, 3], [1.5, 2.5])"), "true");
    // a point that is not all machine reals stays with the language definition
    assert_eq!(eval(&mut e, "f(3/2, 5/2)"), eval(&mut e, "nl_li_lang([[1, 2, 3], [4, 5, 6], [7, 8, 10]], [range(3), range(3)], [3, 3], [3/2, 5/2])"));
    assert_eq!(eval(&mut e, "nl_li_machine([[1, a], [2, 3]], [range(2), range(2)], [1, 1], [1.5, 1.5])"), "nl_li_machine([[1, a], [2, 3]], [[1, 2], [1, 2]], [1, 1], [1.5, 1.5])");
    assert!(t0.elapsed() < Duration::from_secs(60), "took {:?}", t0.elapsed());
}

#[test]
fn values_of_an_interpolation_are_fast() {
    let mut e = mc_engine::CoreEngine::new();
    eval(&mut e, "f = list_interpolation([[1, 2, 3, 4], [2, 2, 6, 4], [3, 6, 3, 12], [4, 4, 12, 4]])");
    let t0 = Instant::now();
    assert_eq!(eval(&mut e, "length(table(f(x, y), [x, 1., 4., 0.03], [y, 1., 4., 0.03]))"), "101");
    // 10201 values: the language definition took about 4 s here, the native in doubles a fiftieth of that
    assert!(t0.elapsed() < Duration::from_secs(2), "took {:?}", t0.elapsed());
}
