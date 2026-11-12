# depcheck.ps1 (tools/depcheck.ps1): the dependency structure of the workspace, measured, and its rules checked.
#
# From `cargo metadata` (no network, no build): every workspace crate with
#   layer   the longest path of normal dependencies from the crate down to a crate with none (0 = a leaf)
#   lines   lines of Rust in the crate's folder (src, tests, examples, benches)
#   direct  workspace crates it depends on directly (normal and build dependencies; dev-dependencies do not count)
#   trans   workspace crates it depends on, transitively
#   fanin   workspace crates that depend on it directly
# then checks the rules in tools/depcheck-rules.txt, one per line:
#   <crate> must not depend on <crate>      transitively, over normal and build dependencies
#   * must not depend on <crate>            no crate but <crate> itself may depend on it
# Lines starting with # are comments. Exit code 1 on a violation (or an unknown crate in a rule).
#
#   tools/depcheck.ps1 [-Sort layer|lines|name|fanin] [-Markdown] [-Rules <file>]
#   -Markdown  print the table as a Markdown table (for docs/design/12-architecture.md)
param([ValidateSet("layer", "lines", "name", "fanin", "trans")] [string] $Sort = "layer", [switch] $Markdown,
      [string] $Rules = "")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
if (-not $Rules) { $Rules = Join-Path $PSScriptRoot "depcheck-rules.txt" }

Push-Location $root
try {
    $raw = & cargo metadata --format-version 1 --no-deps --offline 2> $null
    if ($LASTEXITCODE -ne 0) { throw "cargo metadata failed" }
} finally { Pop-Location }
$meta = ($raw -join "`n") | ConvertFrom-Json

# workspace crates and their direct workspace dependencies (normal and build)
$names = @{}
foreach ($p in $meta.packages) { $names[$p.name] = $p }
$deps = @{}; $devdeps = @{}
foreach ($p in $meta.packages) {
    $d = New-Object System.Collections.Generic.HashSet[string]
    $v = New-Object System.Collections.Generic.HashSet[string]
    foreach ($x in $p.dependencies) {
        if (-not $names.ContainsKey($x.name)) { continue }
        if ($x.kind -eq "dev") { [void]$v.Add($x.name) } else { [void]$d.Add($x.name) }
    }
    $deps[$p.name] = $d; $devdeps[$p.name] = $v
}

# transitive closure and layer, by depth-first search with memo
$closure = @{}; $layer = @{}
function Visit([string] $n, [System.Collections.Generic.HashSet[string]] $stack) {
    if ($closure.ContainsKey($n)) { return }
    if (-not $stack.Add($n)) { throw "dependency cycle through $n" }
    $all = New-Object System.Collections.Generic.HashSet[string]
    $lay = 0
    foreach ($m in $deps[$n]) {
        Visit $m $stack
        [void]$all.Add($m)
        $all.UnionWith($closure[$m])
        if ($layer[$m] + 1 -gt $lay) { $lay = $layer[$m] + 1 }
    }
    [void]$stack.Remove($n)
    $closure[$n] = $all; $layer[$n] = $lay
}
foreach ($n in $names.Keys) { Visit $n (New-Object System.Collections.Generic.HashSet[string]) }

$fanin = @{}
foreach ($n in $names.Keys) { $fanin[$n] = 0 }
foreach ($n in $names.Keys) { foreach ($m in $deps[$n]) { $fanin[$m]++ } }

$rows = foreach ($n in $names.Keys) {
    $dir = Split-Path -Parent $names[$n].manifest_path
    $lines = 0
    foreach ($f in (Get-ChildItem -Path $dir -Recurse -Filter *.rs -File)) {
        $lines += ([IO.File]::ReadAllLines($f.FullName)).Count
    }
    [pscustomobject]@{ crate = $n; layer = $layer[$n]; lines = $lines; direct = $deps[$n].Count;
                       trans = $closure[$n].Count; fanin = $fanin[$n] }
}
$rows = switch ($Sort) {
    "layer" { $rows | Sort-Object @{ e = "layer"; Descending = $true }, crate }
    "name" { $rows | Sort-Object crate }
    default { $rows | Sort-Object @{ e = $Sort; Descending = $true }, crate }
}

if ($Markdown) {
    "| crate | layer | lines | direct | trans | fan-in |"
    "|---|---:|---:|---:|---:|---:|"
    foreach ($r in $rows) { "| ``{0}`` | {1} | {2} | {3} | {4} | {5} |" -f $r.crate, $r.layer, $r.lines, $r.direct, $r.trans, $r.fanin }
} else {
    "{0,-22} {1,5} {2,7} {3,6} {4,6} {5,6}" -f "crate", "layer", "lines", "direct", "trans", "fanin"
    foreach ($r in $rows) { "{0,-22} {1,5} {2,7} {3,6} {4,6} {5,6}" -f $r.crate, $r.layer, $r.lines, $r.direct, $r.trans, $r.fanin }
}
$total = ($rows | Measure-Object -Property lines -Sum).Sum
$top = ($rows | Measure-Object -Property layer -Maximum).Maximum
""
"{0} crates, {1} lines, {2} layers (0 to {3})" -f $rows.Count, $total, ($top + 1), $top

# rules
$violations = 0; $checked = 0
if (Test-Path $Rules) {
    foreach ($line in (Get-Content $Rules)) {
        $t = $line.Trim()
        if (-not $t -or $t.StartsWith("#")) { continue }
        if ($t -notmatch '^(\S+)\s+must not depend on\s+(\S+)$') { "rule not understood: $t"; $violations++; continue }
        $from = $Matches[1]; $to = $Matches[2]
        if (-not $names.ContainsKey($to) -or ($from -ne "*" -and -not $names.ContainsKey($from))) {
            "rule names an unknown crate: $t"; $violations++; continue
        }
        $checked++
        $sources = if ($from -eq "*") { @($names.Keys | Where-Object { $_ -ne $to }) } else { @($from) }
        foreach ($s in $sources) {
            if ($closure[$s].Contains($to)) {
                $via = if ($deps[$s].Contains($to)) { "directly" } else {
                    "through " + ((@($deps[$s] | Where-Object { $_ -eq $to -or $closure[$_].Contains($to) }) | Sort-Object) -join ", ")
                }
                "VIOLATION: $s depends on $to ($via)  [rule: $t]"
                $violations++
            }
        }
    }
}
"{0} rules checked, {1} violations" -f $checked, $violations
if ($violations) { exit 1 }
exit 0
