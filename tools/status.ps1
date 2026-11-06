# status.ps1 (tools/status.ps1): progress of long-running jobs (test suites, fuzz campaigns, benchmarks, ports).
#
# Every long job writes one small JSON file into the status folder (MC_STATUS_DIR, default ..\build\status beside the
# repository) and rewrites it as it goes:
#
#   {"job": "suite-main", "owner": "coordinator", "title": "workspace tests", "done": 12, "total": 250,
#    "unit": "test binaries", "current": "mc-special tests/special.rs", "state": "running",
#    "started": "2026-11-06T11:40:00Z", "updated": "2026-11-06T11:45:10Z", "message": "3 failed so far"}
#
# `state` is running, done or failed. Any language can write it (write to a temporary file, then rename, so a reader
# never sees half a file). From PowerShell, dot-source this script and call Set-JobStatus.
#
#   tools/status.ps1            print the table once
#   tools/status.ps1 -Watch     redraw it every 2 seconds (Ctrl+C to stop)
#   tools/status.ps1 -All       include finished jobs older than an hour
#   tools/status.ps1 -Clear     remove the status files of finished jobs
param([switch] $Watch, [switch] $All, [switch] $Clear, [int] $Every = 2)

function Get-StatusDir {
    if ($env:MC_STATUS_DIR) { return $env:MC_STATUS_DIR }
    Join-Path (Split-Path -Parent (Split-Path -Parent $PSScriptRoot)) "build\status"
}

# Write or update a job's status file (atomically). Fields left out keep their previous values.
function Set-JobStatus {
    param([Parameter(Mandatory)] [string] $Job, [string] $Title, [string] $Owner, [int] $Done = -1, [int] $Total = -1,
          [string] $Unit, [string] $Current, [string] $State, [string] $Message)
    $dir = Get-StatusDir
    New-Item -ItemType Directory -Force $dir | Out-Null
    $path = Join-Path $dir "$Job.json"
    $now = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    $s = if (Test-Path $path) { Get-Content $path -Raw | ConvertFrom-Json } else {
        [pscustomobject]@{ job = $Job; owner = ""; title = $Job; done = 0; total = 0; unit = ""; current = ""; state = "running"; started = $now; updated = $now; message = "" }
    }
    if ($Title) { $s.title = $Title }
    if ($Owner) { $s.owner = $Owner }
    if ($Done -ge 0) { $s.done = $Done }
    if ($Total -ge 0) { $s.total = $Total }
    if ($Unit) { $s.unit = $Unit }
    if ($PSBoundParameters.ContainsKey('Current')) { $s.current = $Current }
    if ($State) { $s.state = $State }
    if ($PSBoundParameters.ContainsKey('Message')) { $s.message = $Message }
    $s.updated = $now
    $tmp = "$path.$PID.tmp"
    $s | ConvertTo-Json -Compress | Set-Content -Path $tmp -Encoding utf8
    Move-Item -Force $tmp $path
}

function Format-Span([double] $sec) {
    if ($sec -lt 0 -or [double]::IsNaN($sec) -or [double]::IsInfinity($sec)) { return "-" }
    $t = [TimeSpan]::FromSeconds([math]::Round($sec))
    if ($t.TotalHours -ge 1) { "{0}h{1:00}m" -f [int][math]::Floor($t.TotalHours), $t.Minutes } else { "{0}m{1:00}s" -f $t.Minutes, $t.Seconds }
}

function Show-JobStatus {
    $dir = Get-StatusDir
    $now = (Get-Date).ToUniversalTime()
    $rows = @()
    if (Test-Path $dir) {
        foreach ($f in Get-ChildItem $dir -Filter *.json) {
            try { $s = Get-Content $f.FullName -Raw | ConvertFrom-Json } catch { continue }
            $started = [datetime]::Parse($s.started).ToUniversalTime()
            $updated = [datetime]::Parse($s.updated).ToUniversalTime()
            $age = ($now - $updated).TotalSeconds
            if (-not $All -and $s.state -ne "running" -and $age -gt 3600) { continue }
            $elapsed = (($(if ($s.state -eq "running") { $now } else { $updated })) - $started).TotalSeconds
            $pct = if ($s.total -gt 0) { [math]::Min(100, 100.0 * $s.done / $s.total) } else { -1 }
            $eta = if ($s.state -eq "running" -and $s.done -gt 0 -and $s.total -gt $s.done) { $elapsed / $s.done * ($s.total - $s.done) } else { -1 }
            $state = if ($s.state -eq "running" -and $age -gt 120) { "STALE ($([int]$age)s)" } else { $s.state }
            $rows += [pscustomobject]@{ Job = $s.job; Owner = $s.owner; Pct = $pct; Done = $s.done; Total = $s.total; Unit = $s.unit
                State = $state; Elapsed = $elapsed; Eta = $eta; Current = $s.current; Message = $s.message; Title = $s.title }
        }
    }
    $lines = @("MathCAS jobs   $((Get-Date).ToString('HH:mm:ss'))   (status folder: $dir)", "")
    if (-not $rows) { $lines += "  no jobs" }
    foreach ($r in $rows | Sort-Object @{ e = { $_.State -ne "running" } }, Job) {
        $bar = if ($r.Pct -ge 0) { $n = [int][math]::Round($r.Pct / 5); ("#" * $n).PadRight(20, ".") } else { "?" * 20 }
        $pct = if ($r.Pct -ge 0) { "{0,5:N1}%" -f $r.Pct } else { "    ?" }
        $lines += ("  {0,-22} [{1}] {2}  {3}/{4} {5}" -f $r.Job, $bar, $pct, $r.Done, $r.Total, $r.Unit)
        $lines += ("      {0}  elapsed {1}  eta {2}  {3}" -f $r.State, (Format-Span $r.Elapsed), (Format-Span $r.Eta), $(if ($r.Owner) { "($($r.Owner))" } else { "" }))
        if ($r.Current) { $lines += "      now: $($r.Current)" }
        if ($r.Message) { $lines += "      $($r.Message)" }
    }
    $lines
}

# dot-sourced: only define the functions
if ($MyInvocation.InvocationName -eq '.') { return }

if ($Clear) {
    $dir = Get-StatusDir
    if (Test-Path $dir) {
        Get-ChildItem $dir -Filter *.json | Where-Object { (Get-Content $_.FullName -Raw | ConvertFrom-Json).state -ne "running" } | Remove-Item
    }
    return
}
if ($Watch) {
    while ($true) {
        $text = (Show-JobStatus) -join "`n"
        Clear-Host
        Write-Host $text
        Start-Sleep -Seconds $Every
    }
} else {
    Show-JobStatus
}
