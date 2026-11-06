# Usage: tools\package.ps1 -> from the dist folder (tools\dist.ps1's output), builds
#   MathCAS-v2-<commit>-win-x64.zip   standalone: unzip anywhere and run, nothing is installed
#   MathCAS-v2-<commit>-win-x64.exe   installer (Inno Setup 6, tools\installer\mathcas-v2.iss): per user by default
# into ..\dist-packages (outside the repository), mirrored like the dist. Overrides: MATHCAS_V2_DIST,
# MATHCAS_V2_PACKAGES, MATHCAS_V2_MIRROR, ISCC (the Inno Setup compiler).
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$parent = Split-Path -Parent $root
$dist = if ($env:MATHCAS_V2_DIST) { $env:MATHCAS_V2_DIST } else { Join-Path $parent "dist" }
$out = if ($env:MATHCAS_V2_PACKAGES) { $env:MATHCAS_V2_PACKAGES } else { Join-Path $parent "dist-packages" }
if (-not (Test-Path (Join-Path $dist "mc-repl.exe"))) { throw "no dist at ${dist}: run tools\dist.ps1 first" }
$stamp = Get-Content (Join-Path $dist "VERSION.txt") -Raw
$rev = ($stamp -replace '(?s)^.*commit ([0-9a-f]+).*$', '$1').Trim()
$ver = if ($stamp -match 'MathCAS v2 (\d+\.\d+\.\d+)') { $Matches[1] } else { "dev" }
$name = "MathCAS-v2-$ver-$rev-win-x64"
New-Item -ItemType Directory -Force $out | Out-Null

# the standalone zip: the dist folder's contents at the top level of the archive, under one folder
$stage = Join-Path $env:TEMP "mathcas-v2-package"
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
$top = Join-Path $stage $name
New-Item -ItemType Directory -Force $top | Out-Null
Copy-Item (Join-Path $dist "*") $top -Recurse -Force
$zip = Join-Path $out "$name.zip"
if (Test-Path $zip) { Remove-Item $zip -Force }
Add-Type -AssemblyName System.IO.Compression.FileSystem
[IO.Compression.ZipFile]::CreateFromDirectory($top, $zip, [IO.Compression.CompressionLevel]::Optimal, $true)

# the installer
$iscc = if ($env:ISCC) { $env:ISCC } else {
    @((Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe"), (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
      (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe")) | Where-Object { Test-Path $_ } | Select-Object -First 1
}
if (-not $iscc) { throw "Inno Setup 6 (ISCC.exe) not found: install it or set ISCC" }
$exe = Join-Path $out "$name.exe"
if (Test-Path $exe) { Remove-Item $exe -Force }
& $iscc /Q "/DAppVersion=$ver" "/DAppCommit=$rev" "/DSourceDir=$top" "/DOutputDir=$out" "/DOutputName=$name" (Join-Path $PSScriptRoot "installer\mathcas-v2.iss")
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $exe)) { throw "Inno Setup failed (exit $LASTEXITCODE)" }
Remove-Item $stage -Recurse -Force
Get-ChildItem $out -Filter "$name.*" | Select-Object Name, @{ n = "MB"; e = { [math]::Round($_.Length / 1MB, 1) } }

$mirror = if ($env:MATHCAS_V2_MIRROR) { $env:MATHCAS_V2_MIRROR } else { Join-Path $env:USERPROFILE "Dropbox\Projects\MathCAS-v2-packages" }
if (Test-Path (Split-Path -Parent $mirror)) {
    New-Item -ItemType Directory -Force $mirror | Out-Null
    Copy-Item (Join-Path $out "$name.*") $mirror -Force
    "mirrored to $mirror"
}
