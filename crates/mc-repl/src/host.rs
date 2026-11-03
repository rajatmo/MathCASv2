//! The real engine as the loop's [`Host`]: `:load` evaluates a file with [`CoreEngine::eval_file`], `:modules`
//! lists the module table.
use crate::Host;
use mc_bench::{Engine, EngineError, StopFlag};
use mc_engine::{CoreEngine, Event, RunError, Script};
use std::path::Path;

impl Host for CoreEngine {
    fn engine(&mut self) -> &mut dyn Engine {
        self
    }

    fn load(&mut self, path: &str, stop: &StopFlag) -> Result<Vec<String>, String> {
        self.eval_file(Path::new(path), stop).map(|s| script_lines(&s)).map_err(|e| run_error(&e))
    }

    fn modules(&self) -> Vec<(String, Vec<String>)> {
        CoreEngine::modules(self).map(|m| (m.name.clone(), m.public.iter().cloned().collect())).collect()
    }
}

/// The lines a script shows, in order: its `print` output, the failures of its statements, and the value of its
/// last statement unless that is `null` or a failure already shown.
pub fn script_lines(s: &Script) -> Vec<String> {
    let mut lines: Vec<String> = s
        .events
        .iter()
        .map(|e| match e {
            Event::Output(l) | Event::Failure(l) => l.clone(),
        })
        .collect();
    if let Some(v) = shown_value(s) {
        lines.push(v.to_string());
    }
    lines
}

/// The value of the last statement as a script shows it: not `null`, not a failure (those are shown as they
/// happen), and not the value the script's last output line already shows.
///
/// `print(x)` returns `x`, so a script ending in `print(x)` would show x twice; the runner instead leaves out a
/// last value equal to the output line printed just before it (for a string, the line is its text and the value
/// its literal). A script ending in `print(3); 3` therefore shows 3 once (decision 9.5 of the library-gaps
/// package: `print` keeps returning its argument, which `f(print(x))` relies on).
pub fn shown_value(s: &Script) -> Option<&str> {
    let printed = s.last.printed.as_str();
    let shown = match s.events.last() {
        Some(Event::Failure(f)) => f == printed,
        Some(Event::Output(line)) => {
            let mut literal = String::new();
            mc_syntax::string_literal(line, &mut literal);
            line == printed || literal == printed
        }
        None => false,
    };
    (!s.last_is_null && !shown).then_some(printed)
}

/// The message for an evaluation that did not run to the end.
pub fn run_error(e: &RunError) -> String {
    match e {
        RunError::Read(e) => e.render(),
        RunError::Engine(EngineError::Stopped) => "interrupted".into(),
        RunError::Engine(e) => format!("error: {e}"),
    }
}
