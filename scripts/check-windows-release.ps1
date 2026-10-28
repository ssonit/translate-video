param([string]$Delivery = $PSScriptRoot, [string]$ReportPath)
$ErrorActionPreference = 'Stop'
$Delivery = [IO.Path]::GetFullPath($Delivery).TrimEnd('\', '/')
$prefix = $Delivery + [IO.Path]::DirectorySeparatorChar
$checks = @()
$manifest = Join-Path $Delivery 'manifest.json'
if (!(Test-Path -LiteralPath $manifest -PathType Leaf)) { throw 'manifest.json is missing. Point -Delivery at the packaged folder.' }
$entries = Get-Content -LiteralPath $manifest -Raw | ConvertFrom-Json
if (!$entries.Count) { throw 'The delivery manifest is empty.' }
$seen = @{}
foreach ($entry in $entries) {
    $relative = [string]$entry.path
    if (!$relative -or [string]$entry.sha256 -notmatch '^[A-Fa-f0-9]{64}$' -or $null -eq $entry.bytes -or [string]$entry.bytes -notmatch '^\d+$') {
        throw 'Invalid manifest entry: path, SHA-256 and non-negative byte count are required.'
    }
    $full = [IO.Path]::GetFullPath((Join-Path $Delivery $relative))
    if ([IO.Path]::IsPathRooted($relative) -or !$full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase) -or $seen.ContainsKey($full)) {
        throw 'Unsafe or duplicate manifest path.'
    }
    $seen[$full] = $true
    $exists = Test-Path -LiteralPath $full -PathType Leaf
    $matches = $false
    if ($exists) {
        $inputFile = [IO.File]::OpenRead($full)
        $sha = [Security.Cryptography.SHA256]::Create()
        try { $hash = [BitConverter]::ToString($sha.ComputeHash($inputFile)).Replace('-', '') }
        finally { $sha.Dispose(); $inputFile.Dispose() }
        $matches = (Get-Item -LiteralPath $full).Length -eq $entry.bytes -and $hash -eq $entry.sha256
    }
    $checks += [PSCustomObject]@{ path = $relative; exists = $exists; matches = $matches }
}
$tools = [ordered]@{}
foreach ($name in @('ffmpeg', 'ffprobe', 'yt-dlp', 'ollama', 'py', 'python')) {
    $command = Get-Command $name -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    $tools[$name] = [bool]$command
}
$webview = @()
foreach ($base in @(${env:ProgramFiles(x86)}, $env:ProgramFiles, $env:LOCALAPPDATA)) {
    if (!$base) { continue }
    $application = Join-Path $base 'Microsoft/EdgeWebView/Application'
    if (Test-Path -LiteralPath $application -PathType Container) {
        $webview += @(Get-ChildItem -LiteralPath $application -Directory | Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'msedgewebview2.exe') -PathType Leaf } | Select-Object -ExpandProperty Name)
    }
}
$required = @(
    'tauri-app.exe', 'HUONG-DAN.md', 'check-windows-release.ps1',
    'scripts/faster_whisper.py', 'douyin/fetch.py', 'douyin/__init__.py',
    'douyin/dtk/__init__.py', 'douyin/dtk/signing/__init__.py',
    'douyin/dtk/signing/native/__init__.py', 'douyin/dtk/signing/native/abogus.py',
    'douyin/dtk/signing/native/sm3.py', 'douyin/dtk/signing/native/websign.py'
)
$missing = @($required | Where-Object { !(Test-Path -LiteralPath (Join-Path $Delivery $_) -PathType Leaf) })
$unlisted = @($required | Where-Object { !$seen.ContainsKey([IO.Path]::GetFullPath((Join-Path $Delivery $_))) })
$report = [ordered]@{
    checkedAt = (Get-Date).ToString('o')
    windowsVersion = [Environment]::OSVersion.Version.ToString()
    process64Bit = [Environment]::Is64BitProcess
    manifestPassed = @($checks | Where-Object { !$_.matches }).Count -eq 0 -and $missing.Count -eq 0 -and $unlisted.Count -eq 0
    missingRequiredFiles = $missing
    unlistedRequiredFiles = $unlisted
    files = $checks
    webView2Versions = @($webview | Sort-Object -Unique)
    toolsInPath = $tools
    note = 'Read-only preflight. Does not certify a clean machine, installer, app startup, models, voices, playback or audio quality.'
}
$json = $report | ConvertTo-Json -Depth 6
if ($ReportPath) {
    $stream = [IO.File]::Open([IO.Path]::GetFullPath($ReportPath), [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
    try {
        $bytes = [Text.UTF8Encoding]::new($false).GetBytes($json)
        $stream.Write($bytes, 0, $bytes.Length)
    } finally { $stream.Dispose() }
}
Write-Output $json
if (!$report.manifestPassed) { exit 1 }
