# icons.ps1 (tools/icons.ps1): the icons of the windows and the application icon, generated from their TikZ sources.
#
# Interface icons: every assets/icons/src/<name>.tex (a standalone TikZ picture on the common 24 x 24 canvas of
# icon-style.tex) is compiled with latex and converted with dvisvgm (glyphs as paths, no fonts) to
# assets/icons/<name>.svg. The application icon: assets/app/src/icon.tex is compiled with pdflatex and rasterised with
# pdftocairo (transparent outside the rounded square) to assets/app/icon_128.png, icon_256.png and the sizes 32 to 256
# of icon.ico; icon-small.tex gives the 16 and 24 pixel images of icon.ico (Python with Pillow writes the .ico). The
# generated files are committed: building the programs never needs TeX.
#
#   tools/icons.ps1 [-Only <name,...>] [-Job icons] [-Owner ""] [-NoApp]
# Needs latex, pdflatex, dvisvgm, pdftocairo (TeX Live or MiKTeX) and python with Pillow. Progress is printed as
# [k/N pct%] and written to the status file of job -Job (see tools/status.ps1).
param([string[]] $Only = @(), [string] $Job = "icons", [string] $Owner = "", [switch] $NoApp)
$ErrorActionPreference = "Continue"
$Only = @($Only | ForEach-Object { $_ -split ',' } | Where-Object { $_ })
$root = Split-Path -Parent $PSScriptRoot
. (Join-Path $PSScriptRoot "status.ps1")
$src = Join-Path $root "assets/icons/src"
$out = Join-Path $root "assets/icons"
$app = Join-Path $root "assets/app"
$work = Join-Path ([IO.Path]::GetTempPath()) "mc-icons-$PID"
New-Item -ItemType Directory -Force $work | Out-Null

foreach ($tool in "latex", "pdflatex", "dvisvgm", "pdftocairo", "python") {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { Write-Host "missing: $tool"; exit 1 }
}
$sources = @(Get-ChildItem $src -Filter *.tex | Where-Object { $_.BaseName -ne "icon-style" } |
    Where-Object { -not $Only.Count -or $Only -contains $_.BaseName } | Sort-Object Name)
$total = $sources.Count + $(if ($NoApp) { 0 } else { 3 })
Set-JobStatus -Job $Job -Title "icons from TikZ" -Owner $Owner -Done 0 -Total $total -Unit "steps" -Current "" -State running -Message ""
$k = 0; $failed = @()
function Step([string] $what) {
    $script:k++
    $pct = [math]::Floor(100 * ($script:k - 1) / [math]::Max(1, $total))
    Write-Host ("[{0}/{1} {2}%] {3}" -f $script:k, $total, $pct, $what)
    Set-JobStatus -Job $Job -Done ($script:k - 1) -Current $what -Message "$($failed.Count) failed so far"
}

# the interface icons
foreach ($s in $sources) {
    Step $s.BaseName
    Push-Location $src
    $null = & latex -interaction=nonstopmode -halt-on-error "-output-directory=$work" $s.Name 2>&1
    $ok = $LASTEXITCODE -eq 0
    Pop-Location
    $dvi = Join-Path $work "$($s.BaseName).dvi"
    $svg = Join-Path $out "$($s.BaseName).svg"
    if ($ok) {
        # the page is the 24 x 24 canvas (standalone's paper size), not the ink
        $null = & dvisvgm --no-fonts --bbox=papersize --precision=3 -o $svg $dvi 2>&1
        $ok = $LASTEXITCODE -eq 0 -and (Test-Path $svg)
    }
    if (-not $ok) { $failed += $s.BaseName; Write-Host "  failed: $($s.BaseName) (log in $work)" }
}

if (-not $NoApp) {
    $asrc = Join-Path $app "src"
    Step "application icon"
    Push-Location $asrc
    foreach ($t in "icon", "icon-small") {
        $null = & pdflatex -interaction=nonstopmode -halt-on-error "-output-directory=$work" "$t.tex" 2>&1
        if ($LASTEXITCODE -ne 0) { $failed += "app $t" }
    }
    Pop-Location
    Step "application icon pictures"
    $pdf = Join-Path $work "icon.pdf"; $small = Join-Path $work "icon-small.pdf"
    foreach ($size in 32, 48, 64, 128, 256) {
        $null = & pdftocairo -png -transp -singlefile -scale-to $size $pdf (Join-Path $work "icon-$size") 2>&1
    }
    foreach ($size in 16, 24) {
        $null = & pdftocairo -png -transp -singlefile -scale-to $size $small (Join-Path $work "icon-$size") 2>&1
    }
    Copy-Item (Join-Path $work "icon-128.png") (Join-Path $app "icon_128.png") -Force
    Copy-Item (Join-Path $work "icon-256.png") (Join-Path $app "icon_256.png") -Force
    Step "icon.ico"
    # every size from its own picture (16 and 24 from the small design)
    $py = @'
import sys, os
from PIL import Image
work, ico = sys.argv[1], sys.argv[2]
sizes = [16, 24, 32, 48, 64, 128, 256]
images = [Image.open(os.path.join(work, f"icon-{s}.png")).convert("RGBA") for s in sizes]
images[-1].save(ico, format="ICO", sizes=[(s, s) for s in sizes], append_images=images[:-1])
'@
    $pyFile = Join-Path $work "ico.py"
    Set-Content -Path $pyFile -Value $py -Encoding utf8
    $null = & python -I $pyFile $work (Join-Path $app "icon.ico") 2>&1
    if ($LASTEXITCODE -ne 0) { $failed += "app ico" }
}

$state = if ($failed.Count) { "failed" } else { "done" }
Set-JobStatus -Job $Job -Done $total -Current "" -State $state -Message $(if ($failed.Count) { "failed: $($failed -join ', ')" } else { "$total steps" })
if ($failed.Count) { Write-Host "failed: $($failed -join ', ')"; exit 1 }
Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
Write-Host "done: $($sources.Count) icons$(if (-not $NoApp) { ', the application icon' })"
