# test.ps1 (tools/test.ps1): the workspace test suite with progress you can watch.
#
# Builds every test binary once (cargo test --no-run), then runs them one at a time under tools/guard.ps1 (a memory
# cap and a time limit per binary), printing [k/N pct%] per binary with its counts and time, writing the job's status
# file for tools/status.ps1 (job name -Job), and ending with a summary and the list of failures. Doc tests run last as
# one step. Exit code 1 when anything failed.
#
#   tools/test.ps1 [-Package mc-io,mc-docs] [-Job suite] [-Owner coordinator] [-MemMB 3072] [-TimeoutSec 1200]
#                  [-Threads 4] [-NoDoc] [-Log <file>]
# Uses CARGO_TARGET_DIR like cargo (set it to your own build folder).
param([string[]] $Package = @(), [string] $Job = "suite", [string] $Owner = "", [int] $MemMB = 3072,
      [int] $TimeoutSec = 1200, [int] $Threads = 4, [switch] $NoDoc, [string] $Log = "")
# native programs write progress to stderr: keep going and check exit codes instead
$ErrorActionPreference = "Continue"
# -File passes "a,b" as one string
$Package = @($Package | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "status.ps1")
$guard = Join-Path $PSScriptRoot "guard.ps1"
if (-not $Log) { $Log = Join-Path ([IO.Path]::GetTempPath()) "mc-test-$Job.log" }
"" | Set-Content $Log -Encoding utf8

$sel = if ($Package.Count) { $Package | ForEach-Object { "-p", $_ } } else { @("--workspace") }
Set-JobStatus -Job $Job -Title "tests: $(if ($Package.Count) { $Package -join ', ' } else { 'workspace' })" -Owner $Owner `
    -Done 0 -Total 0 -Unit "test binaries" -Current "building the test binaries" -State running -Message ""
Write-Host "building the test binaries ..."
$json = & cargo test --release --offline @sel --no-run --message-format=json 2>$null
if ($LASTEXITCODE -ne 0 -or -not $json) {
    Set-JobStatus -Job $Job -State failed -Current "" -Message "the build failed (run cargo test --no-run to see why)"
    Write-Host "cargo test --no-run failed"
    exit 1
}
$bins = @()
foreach ($line in $json) {
    if (-not $line.StartsWith('{"reason":"compiler-artifact"')) { continue }
    $a = $line | ConvertFrom-Json
    if (-not $a.executable -or -not $a.profile.test) { continue }
    $pkg = ($a.package_id -split '[#@ ]')[0]; $pkg = $pkg.Substring($pkg.LastIndexOf('/') + 1)
    $kind = $a.target.kind[0]
    $label = if ($kind -eq "lib") { "$($a.target.name) (unit)" } else { "$($a.target.name) ($kind)" }
    $bins += [pscustomobject]@{ Package = $pkg; Label = $label; Exe = $a.executable; Dir = (Split-Path -Parent $a.manifest_path) }
}
$bins = $bins | Sort-Object Package, Label
$steps = $bins.Count + $(if ($NoDoc) { 0 } else { 1 })
$passed = 0; $failed = 0; $ignored = 0; $failures = @(); $t0 = Get-Date
$k = 0
foreach ($b in $bins) {
    $k++
    $name = "$($b.Package) $($b.Label)"
    Set-JobStatus -Job $Job -Done ($k - 1) -Total $steps -Current $name -Message "$passed passed, $failed failed so far"
    $out = Join-Path ([IO.Path]::GetTempPath()) "mc-test-$Job-$k.txt"
    $env:CARGO_MANIFEST_DIR = $b.Dir
    $env:RUST_TEST_THREADS = "$Threads"
    $start = Get-Date
    $verdict = & powershell -NoProfile -ExecutionPolicy Bypass -File $guard -Exe $b.Exe -OutFile $out -WorkDir $b.Dir `
        -MemMB $MemMB -TimeoutSec $TimeoutSec
    $secs = ((Get-Date) - $start).TotalSeconds
    $text = if (Test-Path $out) { Get-Content $out -Raw } else { "" }
    Add-Content $Log "===== $name`n$text"
    $p = 0; $f = 0; $i = 0
    foreach ($m in [regex]::Matches($text, 'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored')) {
        $p += [int]$m.Groups[1].Value; $f += [int]$m.Groups[2].Value; $i += [int]$m.Groups[3].Value
    }
    $passed += $p; $failed += $f; $ignored += $i
    $bad = $f -gt 0 -or -not ("$verdict" -match '^ok')
    if ($bad) {
        $names = [regex]::Matches($text, '(?m)^test (\S+) \.\.\. FAILED') | ForEach-Object { $_.Groups[1].Value }
        $failures += "$name : $verdict$(if ($names) { ' : ' + ($names -join ', ') })"
        if ($f -eq 0) { $failed++ }
    }
    $pct = 100.0 * $k / $steps
    $mark = if ($bad) { "FAIL" } else { "ok" }
    Write-Host ("[{0}/{1} {2,5:N1}%] {3,-4} {4,-45} {5,4} passed {6,2} failed {7,6:N1}s" -f $k, $steps, $pct, $mark, $name, $p, $f, $secs)
    Remove-Item $out -ErrorAction SilentlyContinue
}
if (-not $NoDoc) {
    $k++
    Set-JobStatus -Job $Job -Done ($k - 1) -Total $steps -Current "doc tests" -Message "$passed passed, $failed failed so far"
    $doc = & cargo test --release --offline @sel --doc 2>&1 | Out-String
    Add-Content $Log "===== doc tests`n$doc"
    $p = 0; $f = 0
    foreach ($m in [regex]::Matches($doc, 'test result: \w+\. (\d+) passed; (\d+) failed')) { $p += [int]$m.Groups[1].Value; $f += [int]$m.Groups[2].Value }
    $passed += $p; $failed += $f
    if ($f -gt 0 -or $LASTEXITCODE -ne 0) { $failures += "doc tests: $f failed" }
    Write-Host ("[{0}/{1} 100.0%] {2,-4} {3,-45} {4,4} passed {5,2} failed" -f $k, $steps, $(if ($f) { "FAIL" } else { "ok" }), "doc tests", $p, $f)
}
$total = ((Get-Date) - $t0).TotalSeconds
$summary = "$passed passed, $failed failed, $ignored ignored in $([math]::Round($total))s"
Set-JobStatus -Job $Job -Done $steps -Total $steps -Current "" -State $(if ($failures) { "failed" } else { "done" }) -Message $summary
Write-Host ""
Write-Host $summary
if ($failures) { Write-Host "failures:"; $failures | ForEach-Object { Write-Host "  $_" }; Write-Host "full output: $Log"; exit 1 }
