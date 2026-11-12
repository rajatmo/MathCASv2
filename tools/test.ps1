# test.ps1 (tools/test.ps1): the workspace test suite on every core, with progress you can watch.
#
# 1. Builds every test binary once (cargo test --no-run, all cores), printing the build's progress as it goes
#    (compiled units against the count of the last build).
# 2. Runs the binaries in parallel, several at a time, each under tools/guard.ps1 (a memory cap and a time limit per
#    binary), the slowest first (times of the last run are kept in the target folder). Doc tests run as one more
#    parallel step. A binary starts only when the machine has free memory for it.
# 3. Prints [k/N pct%] as each binary finishes, a heartbeat every 15 s with what is running and the time left, keeps
#    the job's status file current for tools/status.ps1 (job name -Job), and ends with a summary and the failures.
# Exit code 1 when anything failed.
#
#   tools/test.ps1 [-Package mc-io,mc-docs] [-Job suite] [-Owner main] [-MemMB 3072] [-TimeoutSec 1200]
#                  [-Threads 2] [-Jobs 0] [-NoDoc] [-Log <file>]
#   -Threads  test threads inside one binary (RUST_TEST_THREADS)
#   -Jobs     binaries at once; 0 = logical cores / Threads
# Uses CARGO_TARGET_DIR like cargo (set it to your own build folder).
param([string[]] $Package = @(), [string] $Job = "suite", [string] $Owner = "", [int] $MemMB = 3072,
      [int] $TimeoutSec = 1200, [int] $Threads = 2, [int] $Jobs = 0, [switch] $NoDoc, [string] $Log = "")
# native programs write progress to stderr: keep going and check exit codes instead
$ErrorActionPreference = "Continue"
# -File passes "a,b" as one string
$Package = @($Package | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "status.ps1")
$guard = Join-Path $PSScriptRoot "guard.ps1"
$cores = [Environment]::ProcessorCount
if ($Jobs -le 0) { $Jobs = [math]::Max(1, [math]::Floor($cores / [math]::Max(1, $Threads))) }
if (-not $Log) { $Log = Join-Path ([IO.Path]::GetTempPath()) "mc-test-$Job.log" }
"" | Set-Content $Log -Encoding utf8
$target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $root "target" }
New-Item -ItemType Directory -Force $target | Out-Null
$scope = if ($Package.Count) { ($Package | Sort-Object) -join '+' } else { 'workspace' }
$unitsFile = Join-Path $target "mc-test-units-$scope.txt"
$timesFile = Join-Path $target "mc-test-times.tsv"
$t0 = Get-Date

# ---------------------------------------------------------------- build, with progress
# @( ... ): an if-expression unwraps a one-element array into a string, and splatting a string passes its characters
$sel = @(if ($Package.Count) { $Package | ForEach-Object { "-p", $_ } } else { "--workspace" })
$title = "tests: $(if ($Package.Count) { $Package -join ', ' } else { 'workspace' })"
$lastUnits = if (Test-Path $unitsFile) { [int](Get-Content $unitsFile -Raw) } else { 0 }
Set-JobStatus -Job $Job -Title $title -Owner $Owner -Done 0 -Total $lastUnits -Unit "build units" `
    -Current "building the test binaries" -State running -Message ""
Write-Host "building the test binaries on $cores cores$(if ($lastUnits) { " (about $lastUnits units)" }) ..."
$buildLog = Join-Path ([IO.Path]::GetTempPath()) "mc-test-$Job-build.txt"
$json = New-Object System.Collections.Generic.List[string]
$units = 0; $lastShow = Get-Date; $lastPct = -1
& cargo test --release --offline @sel --no-run --message-format=json 2> $buildLog | ForEach-Object {
    $json.Add($_)
    if ($_.StartsWith('{"reason":"compiler-artifact"')) {
        $units++
        $pct = if ($lastUnits) { [math]::Min(99, [math]::Floor(100 * $units / $lastUnits)) } else { -1 }
        $fresh = $_.Contains('"fresh":true')
        if ((((Get-Date) - $lastShow).TotalSeconds -ge 10 -or ($pct -ge 0 -and $pct -ge $lastPct + 10)) -and -not $fresh) {
            $name = if ($_ -match '"target":\{[^}]*"name":"([^"]+)"') { $Matches[1] } else { "" }
            $of = if ($lastUnits) { "/$lastUnits $pct%" } else { "" }
            Write-Host ("  [build] {0}{1} units, {2}, compiling {3}" -f $units, $of, (Format-Span ((Get-Date) - $t0).TotalSeconds), $name)
            Set-JobStatus -Job $Job -Done $units -Total ([math]::Max($units, $lastUnits)) -Current "building: $name"
            $lastShow = Get-Date; $lastPct = $pct
        }
    }
}
$buildOk = $LASTEXITCODE -eq 0
if (-not $buildOk -or $json.Count -eq 0) {
    Get-Content $buildLog -Tail 40 | ForEach-Object { Write-Host $_ }
    # the compiler's own messages are in the JSON stream, not in stderr
    foreach ($line in $json) {
        if ($line.StartsWith('{"reason":"compiler-message"')) {
            $m = ($line | ConvertFrom-Json).message
            if ($m.level -eq 'error') { Write-Host $m.rendered }
        }
    }
    Set-JobStatus -Job $Job -State failed -Current "" -Message "the build failed (errors above)"
    Write-Host "cargo test --no-run failed"
    exit 1
}
Set-Content $unitsFile "$units"
$buildSecs = ((Get-Date) - $t0).TotalSeconds
Write-Host ("  [build] done: {0} units in {1}" -f $units, (Format-Span $buildSecs))

# ---------------------------------------------------------------- the steps
$bins = @()
foreach ($line in $json) {
    if (-not $line.StartsWith('{"reason":"compiler-artifact"')) { continue }
    $a = $line | ConvertFrom-Json
    if (-not $a.executable -or -not $a.profile.test) { continue }
    $pkg = ($a.package_id -split '[#@ ]')[0]; $pkg = $pkg.Substring($pkg.LastIndexOf('/') + 1)
    $kind = $a.target.kind[0]
    $label = if ($kind -eq "lib") { "$($a.target.name) (unit)" } else { "$($a.target.name) ($kind)" }
    $bins += [pscustomobject]@{ Name = "$pkg $label"; Exe = $a.executable; Dir = (Split-Path -Parent $a.manifest_path); Args = "" }
}
if (-not $NoDoc) {
    $cargo = (Get-Command cargo).Source
    $docArgs = (@("test", "--release", "--offline") + $sel + @("--doc")) -join ','
    $bins += [pscustomobject]@{ Name = "doc tests"; Exe = $cargo; Dir = $root; Args = $docArgs }
}
# slowest first (times of the last run), unknown ones first of all: the run ends when its longest step ends
$times = @{}
if (Test-Path $timesFile) {
    foreach ($l in Get-Content $timesFile) { $f = $l -split "`t"; if ($f.Count -eq 2) { $times[$f[0]] = [double]$f[1] } }
}
$expect = { param($n) if ($times.ContainsKey($n)) { $times[$n] } else { 1e9 } }
$bins = @($bins | Sort-Object @{ Expression = { & $expect $_.Name }; Descending = $true }, Name)
$steps = $bins.Count
$known = ($bins | Where-Object { $times.ContainsKey($_.Name) } | ForEach-Object { $times[$_.Name] } | Measure-Object -Sum).Sum
Write-Host ("running {0} test binaries, {1} at a time, {2} test threads each{3} ..." -f $steps, $Jobs, $Threads,
    $(if ($known) { ", about " + (Format-Span ($known / $Jobs)) + " of work per slot" } else { "" }))
Set-JobStatus -Job $Job -Done 0 -Total $steps -Unit "test binaries" -Current "starting" -Message ""

# ---------------------------------------------------------------- parallel run
function Get-FreeMB { [int]((Get-CimInstance Win32_OperatingSystem).FreePhysicalMemory / 1024) }
$queue = New-Object System.Collections.Generic.Queue[object]
$bins | ForEach-Object { $queue.Enqueue($_) }
$running = @()
$passed = 0; $failed = 0; $ignored = 0; $failures = @(); $k = 0; $newTimes = @{}
$lastBeat = Get-Date; $id = 0
$env:RUST_TEST_THREADS = "$Threads"
while ($queue.Count -gt 0 -or $running.Count -gt 0) {
    # start what fits: free slots, and free memory for at least half of a binary's cap
    while ($queue.Count -gt 0 -and $running.Count -lt $Jobs) {
        if ($running.Count -gt 0 -and (Get-FreeMB) -lt ($MemMB / 2)) { break }
        $b = $queue.Dequeue(); $id++
        $out = Join-Path ([IO.Path]::GetTempPath()) "mc-test-$Job-$id.txt"
        $verdictFile = "$out.verdict"
        $env:CARGO_MANIFEST_DIR = $b.Dir
        $ga = @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "`"$guard`"", "-Exe", "`"$($b.Exe)`"",
                "-OutFile", "`"$out`"", "-WorkDir", "`"$($b.Dir)`"", "-MemMB", "$MemMB", "-TimeoutSec", "$TimeoutSec")
        if ($b.Args) { $ga += @("-ArgCsv", "`"$($b.Args)`"") }
        $p = Start-Process powershell -ArgumentList $ga -NoNewWindow -PassThru -RedirectStandardOutput $verdictFile
        $running += [pscustomobject]@{ Bin = $b; Proc = $p; Out = $out; Verdict = $verdictFile; Start = (Get-Date) }
    }
    Start-Sleep -Milliseconds 250
    $still = @()
    foreach ($r in $running) {
        if (-not $r.Proc.HasExited) { $still += $r; continue }
        $k++
        $secs = ((Get-Date) - $r.Start).TotalSeconds
        $newTimes[$r.Bin.Name] = $secs
        $verdict = if (Test-Path $r.Verdict) { (Get-Content $r.Verdict -Raw).Trim() } else { "" }
        $text = if (Test-Path $r.Out) { Get-Content $r.Out -Raw } else { "" }
        Add-Content $Log "===== $($r.Bin.Name)`n$text"
        $p = 0; $f = 0; $i = 0
        foreach ($m in [regex]::Matches("$text", 'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored')) {
            $p += [int]$m.Groups[1].Value; $f += [int]$m.Groups[2].Value; $i += [int]$m.Groups[3].Value
        }
        $passed += $p; $failed += $f; $ignored += $i
        $bad = $f -gt 0 -or -not ("$verdict" -match '^ok')
        if ($bad) {
            $names = [regex]::Matches("$text", '(?m)^test (\S+) \.\.\. FAILED') | ForEach-Object { $_.Groups[1].Value }
            $failures += "$($r.Bin.Name) : $verdict$(if ($names) { ' : ' + ($names -join ', ') })"
            if ($f -eq 0) { $failed++ }
        }
        $mark = if ($bad) { "FAIL" } else { "ok" }
        Write-Host ("[{0}/{1} {2,5:N1}%] {3,-4} {4,-45} {5,4} passed {6,2} failed {7,6:N1}s" -f $k, $steps, (100.0 * $k / $steps), $mark, $r.Bin.Name, $p, $f, $secs)
        Remove-Item $r.Out, $r.Verdict -ErrorAction SilentlyContinue
    }
    $running = $still
    if (((Get-Date) - $lastBeat).TotalSeconds -ge 15 -and $running.Count -gt 0) {
        $lastBeat = Get-Date
        $now = Get-Date
        $left = 0.0
        foreach ($r in $running) { $e = & $expect $r.Bin.Name; if ($e -lt 1e9) { $left += [math]::Max(0, $e - ($now - $r.Start).TotalSeconds) } }
        foreach ($b in $queue) { $e = & $expect $b.Name; if ($e -lt 1e9) { $left += $e } }
        $eta = if ($times.Count) { ", about " + (Format-Span ($left / $Jobs)) + " left" } else { "" }
        $names = ($running | Sort-Object Start | ForEach-Object { "$($_.Bin.Name) $(Format-Span ($now - $_.Start).TotalSeconds)" }) -join '; '
        Write-Host ("  ... {0}/{1} done, {2} running, {3} queued{4}: {5}" -f $k, $steps, $running.Count, $queue.Count, $eta, $names)
        Set-JobStatus -Job $Job -Done $k -Total $steps -Current ("running: " + (($running | ForEach-Object { $_.Bin.Name }) -join ', ')) `
            -Message "$passed passed, $failed failed so far$eta"
    } elseif ($k -gt 0) {
        Set-JobStatus -Job $Job -Done $k -Total $steps -Message "$passed passed, $failed failed so far"
    }
}

# keep the times for the next run's order and estimate (old entries stay for binaries not run this time)
foreach ($n in $newTimes.Keys) { $times[$n] = $newTimes[$n] }
($times.Keys | Sort-Object | ForEach-Object { "$_`t$([math]::Round($times[$_], 1))" }) | Set-Content $timesFile -Encoding utf8

$total = ((Get-Date) - $t0).TotalSeconds
$summary = "$passed passed, $failed failed, $ignored ignored in $([math]::Round($total))s (build $([math]::Round($buildSecs))s, $Jobs binaries at a time)"
Set-JobStatus -Job $Job -Done $steps -Total $steps -Current "" -State $(if ($failures) { "failed" } else { "done" }) -Message $summary
Write-Host ""
Write-Host $summary
if ($failures) { Write-Host "failures:"; $failures | ForEach-Object { Write-Host "  $_" }; Write-Host "full output: $Log"; exit 1 }
