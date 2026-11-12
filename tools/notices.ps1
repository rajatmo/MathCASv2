# Usage: tools\notices.ps1 [-Out FILE] [-Target TRIPLE] [-Check]
#   Builds mc-notices (crates/mc-bench) and writes THIRD-PARTY-NOTICES.txt for the shipped programs: every crate in
#   their dependency graph (normal dependencies, resolved for the target; default the host) with its licence, each
#   distinct licence text once, the bundled fonts' licences and notes on embedded data. Fails (exit 1) when a
#   dependency's licence is not on the allowed list (crates/mc-bench/src/notices/spdx.rs). -Check only checks.
#   Builds into CARGO_TARGET_DIR (default ..\build\target, as tools\dist.ps1); offline, like the dist build.
param(
    [string] $Out = "",
    [string] $Target = "",
    [switch] $Check
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path (Split-Path -Parent $root) "build\target" }
$env:CARGO_TARGET_DIR = $targetDir
Push-Location $root
try {
    cargo build --release --offline -p mc-bench --bin mc-notices
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    $exe = Join-Path $targetDir "release\mc-notices.exe"
    if (-not (Test-Path $exe)) { $exe = Join-Path $targetDir "release/mc-notices" }
    $toolArgs = @("--offline", "--manifest-path", (Join-Path $root "Cargo.toml"))
    if ($Target) { $toolArgs += @("--target", $Target) }
    if ($Check) { $toolArgs += "--check" } elseif ($Out) { $toolArgs += @("--out", $Out) }
    & $exe @toolArgs
    if ($LASTEXITCODE -ne 0) { throw "mc-notices failed (exit $LASTEXITCODE)" }
} finally { Pop-Location }
