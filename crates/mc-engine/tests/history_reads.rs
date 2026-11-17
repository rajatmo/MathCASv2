//! The history read back: a display form at the top of an output is left out (`out()` after `short(list)` is the
//! list), and an earlier output read inside `table` is the output with the iterator standing for its values.
use mc_api::StopFlag;
use mc_engine::{CoreEngine, EachOptions, EngineConfig, Origin, Report, RunError};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

const NUMBERED: EachOptions = EachOptions { numbered: true };

/// The reports of one evaluation, compactly: `n:value`, `n!failure`, `n>output`, `n?tag: message`, `n#image`
/// (`-` for no number), and `error` when it did not run to the end.
fn run(e: &mut CoreEngine, src: &str, options: EachOptions) -> Vec<String> {
    let num = |n: Option<u64>| n.map_or("-".to_string(), |n| n.to_string());
    let mut out = vec![];
    let r = e.eval_each(src, &Origin::input(), &StopFlag::new(), options, &mut |r| {
        out.push(match r {
            Report::Value { n, printed, image: None } => format!("{}:{printed}", num(n)),
            Report::Value { n, image: Some(i), .. } => format!("{}#{}x{}", num(n), i.width, i.height),
            Report::Failure { n, printed } => format!("{}!{printed}", num(n)),
            Report::Output { n, text } => format!("{}>{text}", num(n)),
            Report::Message { n, tag, text } => format!("{}?{tag}: {text}", num(n)),
        })
    });
    if let Err(err) = r {
        out.push(match err {
            RunError::Read(_) => "error: read".to_string(),
            RunError::Engine(e) => format!("error: {e}"),
        });
    }
    out
}

#[test]
fn display_forms_are_left_out_of_the_history() {
    let mut e = engine();
    assert_eq!(run(&mut e, "short([1, 2, 3])\nout()\nlength(out(1))", NUMBERED), ["1:short([1, 2, 3])", "2:[1, 2, 3]", "3:3"]);
    assert_eq!(run(&mut e, "matrix_form([[1, 2], [3, 4]])\ndimensions(out())", NUMBERED)[1], "5:[2, 2]");
}

#[test]
fn an_output_read_in_a_table_takes_the_iterator() {
    let mut e = engine();
    // the iterator is the variable of the earlier output: its values go into it
    assert_eq!(run(&mut e, "a + k^2\ntable(out(), k, 3)\ntable(out(1), k, [0, 5])", NUMBERED), ["1:a + k^2", "2:[1 + a, 4 + a, 9 + a]", "3:[a, 25 + a]"]);
    // a body that does not read the history keeps its lexical iterator
    assert_eq!(run(&mut e, "k = 7\ntable(k, k, 2)", NUMBERED), ["5:[1, 2]"]);
}
