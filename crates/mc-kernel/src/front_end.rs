//! What a front end's session needs from the engine beyond the library (round 13): `manipulate(expr, [a, lo, hi],
//! ...)` stays as written, its arguments held (`hold_all`, no rules), so a notebook receives its printed form and
//! evaluates `{ let a = v; ...; expr }` itself for each position of the controls (gap G81; no protocol message is
//! needed). Every session the kernel, `mc-repl` and the documentation runner make gets it.
use mc_api::Store;
use mc_engine::CoreEngine;

/// The heads front ends handle themselves.
pub const HELD: [&str; 1] = ["manipulate"];

/// The functions of front ends, with their usage lines (for the documentation's catalogue and the help index).
pub const FUNCTIONS: &[(&str, &str)] = &[(
    "manipulate",
    "manipulate(expr, [a, lo, hi], [b, lo, hi, step], ...): expr with a slider per parameter in the notebook; held      elsewhere",
)];

/// A new engine set up for front ends.
pub fn session(config: mc_engine::EngineConfig) -> CoreEngine {
    let mut e = CoreEngine::with_config(config);
    install(&mut e);
    e
}

/// Set up `engine` for front ends: [`HELD`] heads hold their arguments.
pub fn install(engine: &mut CoreEngine) {
    engine.install(|k, h| {
        for name in HELD {
            let s = h.sym(name);
            let a = k.attributes(s);
            k.set_attributes(s, mc_eval::Attributes { hold: mc_eval::Hold::All, ..a });
        }
    });
}
