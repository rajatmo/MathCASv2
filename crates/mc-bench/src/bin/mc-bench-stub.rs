//! The benchmark binary with the stub engine, for the runner's own tests. The `mc-bench` binary beside it has
//! the same shape with the real engine.
use mc_bench::memory::CountingAlloc;
use mc_bench::stub::StubEngine;
use mc_api::Engine;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn main() -> ExitCode {
    let factory = || Box::new(StubEngine::new()) as Box<dyn Engine>;
    mc_bench::cli::main(std::env::args().skip(1).collect(), &factory)
}
