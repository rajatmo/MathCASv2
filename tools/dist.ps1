# Usage: tools\dist.ps1   -> builds the release binaries and copies them, with the benchmark cases and a short
# guide, to ..\dist-v2 (outside the repository). Overrides: MATHCAS_V2_DIST (target folder), MATHCAS_V2_MIRROR
# (mirror folder; default Dropbox\Projects\MathCAS-v2-dist when Dropbox exists), CARGO_TARGET_DIR.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dist = if ($env:MATHCAS_V2_DIST) { $env:MATHCAS_V2_DIST } else { Join-Path (Split-Path -Parent $root) "dist-v2" }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }

Push-Location $root
try {
    cargo build --release --offline -p mc-repl -p mc-engine
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }

New-Item -ItemType Directory -Force $dist | Out-Null
foreach ($exe in @("mc-repl.exe", "mc-bench.exe")) {
    Copy-Item (Join-Path $targetDir "release\$exe") (Join-Path $dist $exe) -Force
}
$cases = Join-Path $dist "bench\cases"
if (Test-Path $cases) { Remove-Item $cases -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $dist "bench") | Out-Null
Copy-Item (Join-Path $root "bench\cases") $cases -Recurse -Force
Copy-Item (Join-Path $root "tools\dist-README.txt") (Join-Path $dist "README.txt") -Force
$rev = (git -C $root rev-parse --short HEAD)
Set-Content -Path (Join-Path $dist "VERSION.txt") -Value "MathCAS v2 development build, commit $rev, built $(Get-Date -Format s)" -Encoding utf8
Get-ChildItem $dist | Select-Object Name, Length

$mirror = if ($env:MATHCAS_V2_MIRROR) { $env:MATHCAS_V2_MIRROR } else { Join-Path $env:USERPROFILE "Dropbox\Projects\MathCAS-v2-dist" }
if (Test-Path (Split-Path -Parent $mirror)) {
    New-Item -ItemType Directory -Force $mirror | Out-Null
    Copy-Item (Join-Path $dist "*") $mirror -Recurse -Force
    "mirrored to $mirror"
}
