//! Scripted sessions of the REPL with the stub engine.
use mc_bench::stub::StubEngine;
use mc_bench::{Engine, EngineError, Evaluated, StopFlag};
use mc_repl::Repl;

fn session(engine: Box<dyn Engine>, input: &str) -> (String, Vec<String>) {
    let mut repl = Repl::new(engine, StopFlag::new());
    let mut out = Vec::new();
    repl.run(&mut input.as_bytes(), &mut out).unwrap();
    (String::from_utf8(out).unwrap(), repl.history().to_vec())
}

fn stub() -> Box<dyn Engine> {
    Box::new(StubEngine::new())
}

#[test]
fn evaluates_and_keeps_the_session() {
    let (out, history) = session(stub(), "x = 6\nx*7\nfib(10)\n");
    assert!(out.starts_with("stub 0 (:help for commands)\nmc> 6\nmc> 42\nmc> 55\nmc> \n"), "{out}");
    assert_eq!(history, ["x = 6", "x*7", "fib(10)"]);
}

#[test]
fn open_brackets_continue_the_input() {
    let (out, history) = session(stub(), "sum([1,\n 2,\n # a comment )\n 3])\n");
    assert!(out.contains("mc> ..> ..> ..> 6\n"), "{out}");
    assert_eq!(history, ["sum([1,\n 2,\n # a comment )\n 3])"]);
    let (out, history) = session(stub(), "f(1,\n:cancel\n2\n");
    assert!(out.contains("..> mc> 2\n"), "{out}");
    assert_eq!(history, ["2"]);
}

#[test]
fn commands() {
    let (out, _) = session(stub(), ":help\n:time\n1 + 1\n:time\n:history\n:redo\n:redo 9\n:bogus\n:quit\n3\n");
    assert!(out.contains(":quit          leave"), "{out}");
    assert!(out.contains("timing on\nmc> 2\ntime: "), "{out}");
    assert!(out.contains("timing off"), "{out}");
    assert!(out.contains("   1  1 + 1\n"), "{out}");
    assert!(out.contains("mc> 2\nmc> no history entry 9"), "{out}");
    assert!(out.contains("unknown command :bogus"), "{out}");
    assert!(!out.contains("mc> 3\n"), "nothing runs after :quit: {out}");
}

#[test]
fn errors_point_at_the_position() {
    let (out, _) = session(stub(), "(1 +\n 2 2)\n7/2\n");
    assert!(out.contains("syntax error at line 2, column 4: expected )\n   2 2)\n     ^\n"), "{out}");
    assert!(out.contains("error: stub: 7/2 is not an integer"), "{out}");
}

/// An engine whose evaluation is interrupted: it raises the stop flag as the console handler would on Ctrl+C,
/// then lets the stub run a loop that polls it.
struct InterruptedOnce {
    inner: StubEngine,
    fired: bool,
}

impl Engine for InterruptedOnce {
    fn name(&self) -> String {
        self.inner.name()
    }
    fn eval(&mut self, src: &str, stop: &StopFlag) -> Result<Evaluated, EngineError> {
        if !self.fired {
            self.fired = true;
            stop.raise();
        }
        self.inner.eval(src, stop)
    }
    fn bind_last(&mut self, name: &str) -> Result<(), EngineError> {
        self.inner.bind_last(name)
    }
}

#[test]
fn interrupt_stops_the_evaluation_and_the_loop_goes_on() {
    let engine = Box::new(InterruptedOnce { inner: StubEngine::new(), fired: false });
    let (out, _) = session(engine, "busy(10^15)\nbusy(10)\n");
    assert!(out.contains("mc> interrupted\nmc> 10\n"), "{out}");
}

#[test]
fn a_stale_interrupt_does_not_stop_the_next_evaluation() {
    let stop = StopFlag::new();
    stop.raise(); // pressed while the loop was waiting for input
    let mut repl = Repl::new(stub(), stop);
    let mut out = Vec::new();
    repl.run(&mut "busy(10^4)\n".as_bytes(), &mut out).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("mc> 10000\n"));
}

#[test]
fn an_interrupt_at_the_prompt_drops_the_input() {
    // Reading `f(1,` then the end of input with the flag raised: what a console read cut by Ctrl+C looks like.
    struct CutRead<'a> {
        stop: StopFlag,
        parts: Vec<&'a [u8]>,
    }
    impl std::io::Read for CutRead<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if self.parts.is_empty() {
                return Ok(0);
            }
            let part = self.parts.remove(0);
            if part.is_empty() {
                self.stop.raise();
            }
            buf[..part.len()].copy_from_slice(part);
            Ok(part.len())
        }
    }
    let stop = StopFlag::new();
    let reader = CutRead { stop: stop.clone(), parts: vec![b"f(1,\n", b"", b"2 + 2\n"] };
    let mut repl = Repl::new(stub(), stop);
    let mut out = Vec::new();
    repl.run(&mut std::io::BufReader::new(reader), &mut out).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("mc> ..> \nmc> 4\nmc> \n"), "{out}");
}
