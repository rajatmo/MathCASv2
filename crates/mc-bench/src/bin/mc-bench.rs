//! The benchmark binary with the real engine (`mc-bench run`, `list`, `summary`; see `mc_bench::cli`).
use mc_api::Engine;
use mc_bench::memory::CountingAlloc;
use mc_engine::CoreEngine;
use std::process::ExitCode;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

fn main() -> ExitCode {
    let factory = || Box::new(CoreEngine::new()) as Box<dyn Engine>;
    mc_bench::cli::main(std::env::args().skip(1).collect(), &factory)
}
