param([string]$Destination)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$binary = Join-Path $projectRoot 'src-tauri/target/release/tauri-app.exe'
if (!(Test-Path -LiteralPath $binary -PathType Leaf)) { throw 'Build first: npm run tauri build -- --no-bundle -- --offline' }
if (!$Destination) { $Destination = Join-Path $projectRoot ('output/windows-' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
$Destination = [IO.Path]::GetFullPath($Destination)
if (Test-Path -LiteralPath $Destination) { throw 'Choose a new folder; existing deliveries are never overwritten.' }
New-Item -ItemType Directory -Path $Destination | Out-Null
Copy-Item -LiteralPath $binary -Destination $Destination
$config = Get-Content -LiteralPath (Join-Path $projectRoot 'src-tauri/tauri.conf.json') -Raw | ConvertFrom-Json
foreach ($entry in $config.bundle.resources.PSObject.Properties) {
    $source = [IO.Path]::GetFullPath((Join-Path (Join-Path $projectRoot 'src-tauri') $entry.Name))
    $target = [IO.Path]::GetFullPath((Join-Path $Destination $entry.Value))
    if (!$target.StartsWith($Destination + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Resource path escapes the delivery folder.' }
    New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $target
}
Copy-Item -LiteralPath (Join-Path $projectRoot 'docs/windows-release.md') -Destination (Join-Path $Destination 'HUONG-DAN.md')
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'check-windows-release.ps1') -Destination $Destination
Get-ChildItem -LiteralPath $Destination -File -Recurse | ForEach-Object {
    [PSCustomObject]@{path=$_.FullName.Substring($Destination.Length+1);sha256=(Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash;bytes=$_.Length}
} | ConvertTo-Json -Depth 3 | Set-Content -LiteralPath (Join-Path $Destination 'manifest.json') -Encoding UTF8
Write-Output $Destination
