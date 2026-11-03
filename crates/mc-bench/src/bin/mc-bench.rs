//! The benchmark binary with the stub engine. Once `mc-eval` exists, a binary of the same shape with the real engine
//! runs the suite; the harness itself does not change.
use mc_bench::memory::CountingAlloc;
use mc_bench::stub::StubEngine;
use mc_bench::Engine;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn main() -> ExitCode {
    let factory = || Box::new(StubEngine::new()) as Box<dyn Engine>;
    mc_bench::cli::main(std::env::args().skip(1).collect(), &factory)
}
