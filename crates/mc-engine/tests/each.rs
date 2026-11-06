//! Statement-by-statement evaluation for notebooks (`CoreEngine::eval_each`): which values are shown, the line
//! counter and the history `out`, output per `print` call, library warnings, image values, and `name_kinds`.
use mc_bench::StopFlag;
use mc_engine::{CoreEngine, EachOptions, EngineConfig, InMemory, NameKind, Origin, Report, RunError};

fn engine() -> CoreEngine {
    CoreEngine::with_config(EngineConfig { threads: 2, ..EngineConfig::default() })
}

const NUMBERED: EachOptions = EachOptions { numbered: true };
const EACH: EachOptions = EachOptions { numbered: false };

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
fn every_shown_value_is_reported() {
    let mut e = engine();
    assert_eq!(run(&mut e, "1+1\n3+3", EACH), ["-:2", "-:6"]);
    // definitions and assignments are null; `;` hides a value; output comes one report per `print` call
    let src = "x = 5\nfn f(t) = t^2\nf(x);\nprint(\"a\"); print(\"b\")\nf(2); 7\n[1, 2]";
    // (`print(x)` returns x; a value its own output line shows is not shown again)
    assert_eq!(run(&mut e, src, EACH), ["->a", "->b", "-:7", "-:[1, 2]"]);
    assert_eq!(run(&mut e, "print(3) + 1\nprint(\"q\")\nprint(\"r\"); 5", EACH), ["->3", "-:4", "->q", "->r", "-:5"]);
    // failures are reported as they happen, also when hidden by `;`
    assert_eq!(run(&mut e, "1/0;\n2", EACH), ["-!failure(division_by_zero)", "-:2"]);
    // a `;` inside brackets does not hide the statement's value; a comment after the statement is not a `;`
    assert_eq!(run(&mut e, "{ 1; 2 }\n3 # x;", EACH), ["-:2", "-:3"]);
    // the scripts' path is unchanged: only the last value
    assert_eq!(e.eval_in("1\n2", &Origin::input(), &StopFlag::new()).unwrap().last.printed, "2");
}

#[test]
fn statements_are_numbered_and_kept() {
    let mut e = engine();
    assert_eq!(e.line(), 1);
    // the observed vector: a cell `1+1 / 3+3` as the second input gives Out[2], Out[3]; the next input is 4
    assert_eq!(run(&mut e, "0;", NUMBERED), Vec::<String>::new());
    assert_eq!(run(&mut e, "1+1\n3+3", NUMBERED), ["2:2", "3:6"]);
    assert_eq!(e.line(), 4);
    // definitions and hidden values take numbers; hidden values are kept in the history
    assert_eq!(run(&mut e, "y = 10\n2*y;\nprint(\"p\")", NUMBERED), ["6>p"]);
    assert_eq!(e.line(), 7);
    assert_eq!(run(&mut e, "out(2)\nout(5)\nout()\nout(-3)", NUMBERED), ["7:2", "8:20", "9:20", "10:2"]);
    // `out(4)` is the assignment `y = 10`: its history entry is the value assigned (gap G86)
    assert_eq!(run(&mut e, "out(4)\nout(0)", NUMBERED), ["11:10", "12!failure(argument, out, 1)"]);
    // a read error takes one number and runs nothing
    assert_eq!(run(&mut e, "1 +", NUMBERED), ["error: read"]);
    assert_eq!(e.line(), 14);
    // failures are kept: `out(n)` gives them back
    assert_eq!(run(&mut e, "1/0\nout(-1)", NUMBERED), ["14!failure(division_by_zero)", "15!failure(division_by_zero)"]);
    // values change with the definitions they mention (`out` keeps the normal form at the time)
    assert_eq!(run(&mut e, "z + 1\nz = 1\nout(16)", NUMBERED), ["16:1 + z", "18:1 + z"]);
    // numbering without `values: each` also works, and unnumbered evaluations leave the counter alone
    assert_eq!(run(&mut e, "5", EACH), ["-:5"]);
    assert_eq!(e.line(), 19);
}

#[test]
fn imports_and_modules_are_one_statement_each() {
    let mut e = engine();
    e.set_sources(Box::new(InMemory::new().with("m.mc", "module m {\n  pub fn twice(x) = 2*x\n  pub k = 3\n}\n7")));
    let origin = Origin::new("main.mc", Some("".into()));
    let mut out = vec![];
    let src = "import m\nmodule g {\n  pub fn h(x) = x\n}\ntwice(k)\ng.h(4)";
    e.eval_each(src, &origin, &StopFlag::new(), NUMBERED, &mut |r| out.push(r)).unwrap();
    assert_eq!(
        out,
        [
            Report::Value { n: Some(3), printed: "6".into(), image: None },
            Report::Value { n: Some(4), printed: "4".into(), image: None },
        ],
        "the module file's statement `7` is not a statement of the input"
    );
    assert_eq!(e.line(), 5);
    let kinds = e.name_kinds();
    let kind = |n: &str| kinds.iter().find(|(m, _)| m == n).map(|(_, k)| *k);
    assert_eq!(kind("m"), Some(NameKind::Module));
    assert_eq!(kind("m.twice"), Some(NameKind::Function));
    assert_eq!(kind("m.k"), Some(NameKind::Value));
    assert_eq!(kind("g.h"), Some(NameKind::Function));
    assert_eq!(kind("sin"), Some(NameKind::Function));
    assert_eq!(kind("pi"), Some(NameKind::Value));
    assert_eq!(kind("out#line"), None, "hidden names are not offered");
    assert_eq!(e.names(), kinds.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
}

#[test]
fn warnings_and_images() {
    let mut e = engine();
    let ode = "n_dsolve([y'(t) == y(t), y(0) == 1], y, [t, 0, 10], max_steps: 3);";
    let r = run(&mut e, ode, NUMBERED);
    assert_eq!(r.len(), 1, "{r:?}");
    assert!(r[0].starts_with("1?n_dsolve: the step limit was reached at t = "), "{r:?}");
    // scripts see the warning as a printed line, as before
    let s = e.eval_in(ode, &Origin::input(), &StopFlag::new()).unwrap();
    assert!(s.last.output[0].starts_with("n_dsolve: the step limit was reached"), "{:?}", s.last.output);
    // `message` from programs too
    assert_eq!(run(&mut e, "message(mine, \"careful\")\nmessage(1, 2)", EACH), ["-?mine: careful", "-:message(1, 2)"]);
    // a plot's value is an image of a readable PNG file
    assert_eq!(run(&mut e, "plot(sin(t), [t, 0, 3])", NUMBERED), ["2#640x400"]);
    // an image term naming no readable PNG is shown as text
    assert_eq!(run(&mut e, "image(\"nowhere.png\")", EACH), ["-:image(\"nowhere.png\")"]);
}

#[test]
fn an_interrupt_keeps_the_running_statements_number() {
    let mut e = engine();
    let stop = StopFlag::new();
    let raiser = {
        let stop = stop.clone();
        std::thread::spawn(move || {
            // the loop below runs until the flag is raised; give it a moment to start
            std::thread::park_timeout(std::time::Duration::from_millis(200));
            stop.raise();
        })
    };
    let mut out = vec![];
    let src = "1\n{ var i = 0; while true { i = i + 1 } }\n3";
    let r = e.eval_each(src, &Origin::input(), &stop, NUMBERED, &mut |r| out.push(r));
    raiser.join().unwrap();
    assert!(matches!(r, Err(RunError::Engine(mc_bench::EngineError::Stopped))), "{r:?}");
    assert_eq!(out, [Report::Value { n: Some(1), printed: "1".into(), image: None }]);
    assert_eq!(e.line(), 3, "the interrupted statement took number 2");
    assert_eq!(run(&mut e, "out(1)", NUMBERED), ["3:1"]);
}

#[test]
fn assignments_are_in_the_history() {
    // gap G86: `%` and `%%` after assignments give the values assigned; a definition `fn` stays out
    let mut e = engine();
    assert_eq!(run(&mut e, "a = 1 + 1\n%", NUMBERED), ["2:2"]);
    assert_eq!(run(&mut e, "b = 3\nc = 4\n%%", NUMBERED), ["5:3"]);
    assert_eq!(run(&mut e, "c = 7;\n%", NUMBERED), ["7:7"]);
    assert_eq!(run(&mut e, "fn g(x) = x\n%", NUMBERED), ["9!failure(history, 8)"]);
}
