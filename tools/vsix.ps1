# Usage: tools\vsix.ps1 [-OutDir folder]   -> packs editors\vscode into mathcas-<version>.vsix (default: the current
# folder), installable with:  code --install-extension mathcas-<version>.vsix
# A .vsix is a zip of the extension under extension/, a manifest and a content-types file; this builds it with .NET
# alone (no Node tools). Entry names use '/' as the format requires.
param([string] $OutDir = ".")
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$ext = Join-Path $root "editors\vscode"
$pkg = Get-Content -Raw -Encoding utf8 (Join-Path $ext "package.json") | ConvertFrom-Json
$esc = { param($s) [System.Security.SecurityElement]::Escape($s) }

$manifest = @"
<?xml version="1.0" encoding="utf-8"?>
<PackageManifest Version="2.0.0" xmlns="http://schemas.microsoft.com/developer/vsx-schema/2011" xmlns:d="http://schemas.microsoft.com/developer/vsx-schema-design/2011">
  <Metadata>
    <Identity Language="en-US" Id="$($pkg.name)" Version="$($pkg.version)" Publisher="$($pkg.publisher)" />
    <DisplayName>$(& $esc $pkg.displayName)</DisplayName>
    <Description xml:space="preserve">$(& $esc $pkg.description)</Description>
    <Categories>Programming Languages</Categories>
    <Icon>extension/icon.png</Icon>
    <Properties>
      <Property Id="Microsoft.VisualStudio.Code.Engine" Value="$($pkg.engines.vscode)" />
    </Properties>
  </Metadata>
  <Installation>
    <InstallationTarget Id="Microsoft.VisualStudio.Code" />
  </Installation>
  <Dependencies />
  <Assets>
    <Asset Type="Microsoft.VisualStudio.Code.Manifest" Path="extension/package.json" Addressable="true" />
    <Asset Type="Microsoft.VisualStudio.Services.Icons.Default" Path="extension/icon.png" Addressable="true" />
  </Assets>
</PackageManifest>
"@
$types = @"
<?xml version="1.0" encoding="utf-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension=".json" ContentType="application/json" />
  <Default Extension=".js" ContentType="application/javascript" />
  <Default Extension=".png" ContentType="image/png" />
  <Default Extension=".txt" ContentType="text/plain" />
  <Default Extension=".vsixmanifest" ContentType="text/xml" />
</Types>
"@

New-Item -ItemType Directory -Force $OutDir | Out-Null
$out = Join-Path (Resolve-Path $OutDir) "$($pkg.name)-$($pkg.version).vsix"
if (Test-Path -LiteralPath $out) { Remove-Item -LiteralPath $out -Force }
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$zip = [System.IO.Compression.ZipFile]::Open($out, [System.IO.Compression.ZipArchiveMode]::Create)
try {
    $utf8 = [System.Text.UTF8Encoding]::new($false)
    foreach ($entry in @(@("extension.vsixmanifest", $manifest), @("[Content_Types].xml", $types))) {
        $w = [System.IO.StreamWriter]::new($zip.CreateEntry($entry[0]).Open(), $utf8)
        try { $w.Write($entry[1]) } finally { $w.Dispose() }
    }
    $base = (Resolve-Path $ext).Path.TrimEnd('\') + '\'
    foreach ($f in Get-ChildItem -LiteralPath $ext -Recurse -File) {
        $name = "extension/" + $f.FullName.Substring($base.Length).Replace('\', '/')
        [void][System.IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $f.FullName, $name)
    }
} finally { $zip.Dispose() }
$out
