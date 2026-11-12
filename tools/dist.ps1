# Usage: tools\dist.ps1   -> builds the release binaries and copies them, with the benchmark cases, the example
# scripts, the VS Code extension (folder and .vsix) and a short guide, to ..\dist (outside the repository; builds from ..\build\target unless CARGO_TARGET_DIR is set). Overrides: MATHCAS_V2_DIST (target folder), MATHCAS_V2_MIRROR
# (mirror folder; default Dropbox\MathCAS v2\dist when Dropbox exists), CARGO_TARGET_DIR.
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$dist = if ($env:MATHCAS_V2_DIST) { $env:MATHCAS_V2_DIST } else { Join-Path (Split-Path -Parent $root) "dist" }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path (Split-Path -Parent $root) "build\target" }

# build where the binaries are copied from (the variable reaches cargo and tools\notices.ps1)
$env:CARGO_TARGET_DIR = $targetDir
Push-Location $root
try {
    cargo build --release --offline -p mc-repl -p mc-engine -p mc-lsp -p mc-kernel -p mc-bench -p mc-notebook -p mc-docs
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
} finally { Pop-Location }

New-Item -ItemType Directory -Force $dist | Out-Null
foreach ($exe in @("mc-repl.exe", "mc-bench.exe", "mc-lsp.exe", "mc-kernel.exe", "mc-notebook.exe", "mc-docs.exe", "mc-docs-pages.exe")) {
    Copy-Item (Join-Path $targetDir "release\$exe") (Join-Path $dist $exe) -Force
}
$cases = Join-Path $dist "bench\cases"
if (Test-Path $cases) { Remove-Item $cases -Recurse -Force }
New-Item -ItemType Directory -Force (Join-Path $dist "bench") | Out-Null
Copy-Item (Join-Path $root "bench\cases") $cases -Recurse -Force
$examples = Join-Path $dist "examples"
if (Test-Path $examples) { Remove-Item $examples -Recurse -Force }
Copy-Item (Join-Path $root "examples") $examples -Recurse -Force
# the editor extension: the folder (install by copying, see its INSTALL.txt) and a packed .vsix
$editors = Join-Path $dist "editors"
if (Test-Path $editors) { Remove-Item $editors -Recurse -Force }
New-Item -ItemType Directory -Force $editors | Out-Null
Copy-Item (Join-Path $root "editors\vscode") (Join-Path $editors "vscode") -Recurse -Force
Get-ChildItem $dist -Filter "mathcas-*.vsix" | Remove-Item -Force
# the Jupyter kernel (register with: python jupyter\install.py)
$jupyter = Join-Path $dist "jupyter"
if (Test-Path $jupyter) { Remove-Item $jupyter -Recurse -Force }
New-Item -ItemType Directory -Force $jupyter | Out-Null
Get-ChildItem (Join-Path $root "editors\jupyter") -File | Copy-Item -Destination $jupyter -Force
& (Join-Path $PSScriptRoot "vsix.ps1") -OutDir $dist | Out-Null
Copy-Item (Join-Path $root "tools\dist-README.txt") (Join-Path $dist "README.txt") -Force
Copy-Item (Join-Path $root "docs\guide\quickstart.pdf") (Join-Path $dist "quickstart.pdf") -Force
# the application icon (also embedded in the executables): the installer and its shortcuts use it
Copy-Item (Join-Path $root "assets\app\icon.ico") (Join-Path $dist "icon.ico") -Force
$rev = (git -C $root rev-parse --short HEAD)
# the release number from the workspace manifest ([workspace.package] version)
$ver = ((Select-String -Path (Join-Path $root "Cargo.toml") -Pattern '^version = "([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value)
Set-Content -Path (Join-Path $dist "VERSION.txt") -Value "MathCAS v2 $ver, commit $rev, built $(Get-Date -Format s)" -Encoding utf8
Copy-Item (Join-Path $root "LICENSE") (Join-Path $dist "LICENSE.txt") -Force
# the third-party notices: every crate in the shipped programs with its licence text, the bundled fonts' licences
& (Join-Path $PSScriptRoot "notices.ps1") -Out (Join-Path $dist "THIRD-PARTY-NOTICES.txt")
Copy-Item (Join-Path $root "CHANGELOG.md") (Join-Path $dist "CHANGELOG.md") -Force
Get-ChildItem $dist | Select-Object Name, Length

$mirror = if ($env:MATHCAS_V2_MIRROR) { $env:MATHCAS_V2_MIRROR } else { Join-Path $env:USERPROFILE "Dropbox\MathCAS v2\dist" }
if (Test-Path (Split-Path -Parent $mirror)) {
    New-Item -ItemType Directory -Force $mirror | Out-Null
    Copy-Item (Join-Path $dist "*") $mirror -Recurse -Force
    "mirrored to $mirror"
}
