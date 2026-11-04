#requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Core)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Core = (Resolve-Path -LiteralPath $Core).Path
$buildRoot = [IO.Path]::GetFullPath((Join-Path $repo '.build')) + '\'
if (!$Core.StartsWith($buildRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Use a repository build artifact.' }
$id = [guid]::NewGuid().ToString('N')
$root = Join-Path $repo ('.runtime/tests/bp-rebuild/' + $id)
$bundle = Join-Path $repo ('.build/bridgepilot/rebuild-tests/' + $id)
$testHome = Join-Path $root 'home'
$null = New-Item -ItemType Directory -Path $root -Force
function Invoke-Core([string[]]$Arguments) {
    $raw = & $Core @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw ($raw | Out-String) }
    return (($raw | Out-String) | ConvertFrom-Json)
}
function Snapshot {
    return (@(Get-ChildItem -LiteralPath $testHome -File -Recurse | Sort-Object FullName | ForEach-Object {
        $_.FullName.Substring($testHome.Length) + ':' + (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    }) -join "`n")
}
function Require-Rejection([scriptblock]$Action, [string]$Pattern) {
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message -notlike "*$Pattern*") { throw }
        if ((Snapshot) -cne $script:before) { throw 'Rejected update changed the installation.' }
        return
    }
    throw "Expected rejection: $Pattern"
}
$launcher = Join-Path (Split-Path -Parent $Core) 'bp-launcher.exe'
$null = Invoke-Core @('bundle', '--output', $bundle, '--core', $Core, '--launcher', $launcher)
$null = Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome)
$before = Snapshot
$manifestPath = Join-Path $bundle 'package.json'
$package = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
foreach ($name in @('bp-core.exe', 'bp-launcher.exe')) {
    $path = Join-Path $bundle ('payload/' + $name)
    $body = [IO.File]::ReadAllBytes($path)
    $pe = [BitConverter]::ToInt32($body, 0x3c)
    $body[$pe + 8] = $body[$pe + 8] -bxor 1
    # Resolve the actual CodeView record through the PE debug directory. The
    # program itself may contain the literal RSDS as ordinary code or data.
    $rsds = -1
    $optional = $pe + 24
    $debugEntry = $optional + 112 + 6 * 8
    $rva = [BitConverter]::ToInt32($body, $debugEntry)
    $size = [BitConverter]::ToInt32($body, $debugEntry + 4)
    $sections = $optional + [BitConverter]::ToUInt16($body, $pe + 20)
    $debugRaw = -1
    for ($index = 0; $index -lt [BitConverter]::ToUInt16($body, $pe + 6); $index++) {
        $section = $sections + $index * 40
        $base = [BitConverter]::ToInt32($body, $section + 12)
        $length = [BitConverter]::ToInt32($body, $section + 16)
        if ($rva -ge $base -and $rva + $size -le $base + $length) {
            $debugRaw = [BitConverter]::ToInt32($body, $section + 20) + $rva - $base
        }
    }
    if ($debugRaw -lt 0) { throw 'Expected a mapped debug directory.' }
    for ($at = $debugRaw; $at -lt $debugRaw + $size; $at += 28) {
        if ([BitConverter]::ToInt32($body, $at + 12) -eq 2) { $rsds = [BitConverter]::ToInt32($body, $at + 24); break }
    }
    if ($rsds -lt 0) { throw 'Expected a real RSDS debug record.' }
    $body[$rsds + 4] = $body[$rsds + 4] -bxor 1
    [IO.File]::WriteAllBytes($path, $body)
    ($package.assets | Where-Object { $_.source -eq ('payload/' + $name) }).sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
}
[IO.File]::WriteAllText($manifestPath, ($package | ConvertTo-Json -Depth 20))
$preview = Invoke-Core @('install-preview', '--bundle', $bundle, '--home', $testHome)
if ($preview.mode -ne 'reused' -or $preview.writes -or (Snapshot) -cne $before) { throw 'Rebuild preview was not a zero-write reuse.' }
$reused = Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome)
if ($reused.status -ne 'installation_current' -or $reused.writes -or (Snapshot) -cne $before) { throw 'Rebuild did not preserve the installed files and history.' }
$null = Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome)
if ((Snapshot) -cne $before) { throw 'Repeated rebuild install changed files.' }
$originalManifest = [IO.File]::ReadAllText($manifestPath)
$asset = $package.assets | Where-Object { $_.source -eq 'payload/bp-core.exe' }
$corePath = Join-Path $bundle $asset.source
$originalCore = [IO.File]::ReadAllBytes($corePath)
# An extra byte preserves program execution but is a genuine non-metadata difference.
[IO.File]::WriteAllBytes($corePath, ($originalCore + [byte]1))
$asset.sha256 = (Get-FileHash -LiteralPath $corePath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText($manifestPath, ($package | ConvertTo-Json -Depth 20))
Require-Rejection { Invoke-Core @('install-preview', '--bundle', $bundle, '--home', $testHome) } 'reuse a version'
[IO.File]::WriteAllBytes($corePath, $originalCore)
[IO.File]::WriteAllText($manifestPath, $originalManifest)
$launcherAsset = $package.assets | Where-Object { $_.source -eq 'payload/bp-launcher.exe' }
$launcherPath = Join-Path $bundle $launcherAsset.source
$originalLauncher = [IO.File]::ReadAllBytes($launcherPath)
# Restore the exact installed core to exercise the old launcher-only loophole.
$installed = Invoke-Core @('install-status', '--home', $testHome)
[IO.File]::Copy((Join-Path $testHome $installed.installation.current.executable), $corePath, $true)
$asset.sha256 = (Get-FileHash -LiteralPath $corePath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllBytes($launcherPath, ($originalLauncher + [byte]1))
$launcherAsset.sha256 = (Get-FileHash -LiteralPath $launcherPath -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText($manifestPath, ($package | ConvertTo-Json -Depth 20))
Require-Rejection { Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome) } 'launcher or package assets'
[IO.File]::WriteAllBytes($launcherPath, $originalLauncher)
[IO.File]::WriteAllBytes($corePath, $originalCore)
[IO.File]::WriteAllText($manifestPath, $originalManifest)
$owned = Join-Path $testHome 'README.md'
$originalOwned = [IO.File]::ReadAllBytes($owned)
[IO.File]::WriteAllText($owned, 'user edit')
$before = Snapshot
Require-Rejection { Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome) } 'user edits'
[IO.File]::WriteAllBytes($owned, $originalOwned)
$before = Snapshot
$pending = Join-Path $testHome '.runtime/bridgepilot-install/journal.json'
[IO.File]::WriteAllText($pending, 'pending')
$before = Snapshot
Require-Rejection { Invoke-Core @('install', '--bundle', $bundle, '--home', $testHome) } 'install-recover'
$receipt = @{status='passed'; rebuild_reused=$true; preview_zero_write=$true; installation_preserved=$true; real_difference_blocked=$true; user_edits_preserved=$true; pending_blocked=$true; home=$testHome; bundle=$bundle}
[IO.File]::WriteAllText((Join-Path $root 'receipt.json'), ($receipt | ConvertTo-Json))
Write-Output ($receipt | ConvertTo-Json)
