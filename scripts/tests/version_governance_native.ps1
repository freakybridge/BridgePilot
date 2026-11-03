#requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$Core, [Parameter(Mandatory=$true)][string]$LegacyPackage)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$Core = (Resolve-Path -LiteralPath $Core).Path
$LegacyPackage = (Resolve-Path -LiteralPath $LegacyPackage).Path
$buildRoot = [IO.Path]::GetFullPath((Join-Path $repo '.build')) + [IO.Path]::DirectorySeparatorChar
if (-not $Core.StartsWith($buildRoot, [StringComparison]::OrdinalIgnoreCase) -or
    -not $LegacyPackage.StartsWith($buildRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Only repository build artifacts may be tested.' }
$id = [guid]::NewGuid().ToString('N')
$root = Join-Path $repo ('.runtime/tests/version-governance/native-' + $id)
$bundle = Join-Path $repo ('.build/bridgepilot/version-governance/bp-1.0.0-' + $id)
$testHome = Join-Path $root 'home'
$null = New-Item -ItemType Directory -Path $root -Force
function Invoke-Core([string]$Exe, [string[]]$Arguments) {
    $raw = & $Exe @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw ($raw | Out-String) }
    return (($raw | Out-String) | ConvertFrom-Json)
}
function Snapshot([string]$Path) {
    $records = @(Get-ChildItem -LiteralPath $Path -File -Recurse | Sort-Object FullName | ForEach-Object {
        $_.FullName.Substring($Path.Length) + ':' + (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    })
    return ($records -join "`n")
}
function Require-Rejection([scriptblock]$Action, [string]$Pattern) {
    $rejected = $false
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message -notlike "*$Pattern*") { throw }
        $rejected = $true
    }
    if (-not $rejected) { throw "Expected rejection: $Pattern" }
}
$identity = Invoke-Core $Core @('self-test')
if ($identity.version -ne '1.0.0' -or $identity.components.orchestrator.version -ne '0.14.0' -or
    $identity.components.forge.delivery -ne 'source-reference') { throw 'Three-version identity mismatch.' }
$launcher = Join-Path (Split-Path -Parent $Core) 'bp-launcher.exe'
$package = Invoke-Core $Core @('bundle', '--output', $bundle, '--core', $Core, '--launcher', $launcher)
if ($package.package_schema -ne 2 -or $package.assets -ne 13) { throw 'Modern package identity mismatch.' }
$legacyCore = Join-Path $LegacyPackage 'payload/bp-core.exe'
$legacyIdentity = Invoke-Core $legacyCore @('self-test')
if ($legacyIdentity.version -ne '0.13.0') { throw 'A real legacy 0.13.0 artifact is required.' }
$legacyInstall = Invoke-Core $legacyCore @('install', '--bundle', $LegacyPackage, '--home', $testHome)
$stable = Join-Path $testHome '.build/bridgepilot/bin/bp.exe'
if ((Invoke-Core $stable @('self-test')).version -ne '0.13.0') { throw 'Legacy stable entry failed.' }
if ((Invoke-Core $stable @('--launcher-self-test')).launcher_protocol -ne 1) { throw 'Legacy launcher protocol mismatch.' }
$beforeLauncher = (Get-FileHash -LiteralPath $stable -Algorithm SHA256).Hash
$before = Snapshot $testHome
$preview = Invoke-Core $Core @('install-preview', '--bundle', $bundle, '--home', $testHome)
if ($preview.writes -or (Snapshot $testHome) -cne $before) { throw 'Migration preview wrote installation files.' }
$packagePath = Join-Path $bundle 'package.json'
$original = [IO.File]::ReadAllBytes($packagePath)
$tampered = [IO.File]::ReadAllText($packagePath) | ConvertFrom-Json
$tampered.components.orchestrator.version = '0.99.0'
[IO.File]::WriteAllText($packagePath, ($tampered | ConvertTo-Json -Depth 30))
Require-Rejection { Invoke-Core $Core @('install-preview', '--bundle', $bundle, '--home', $testHome) } 'component versions'
if ((Snapshot $testHome) -cne $before) { throw 'Rejected package modified installation.' }
[IO.File]::WriteAllBytes($packagePath, $original)
$installed = Invoke-Core $Core @('install', '--bundle', $bundle, '--home', $testHome)
$modern = Invoke-Core $stable @('self-test')
$protocol = Invoke-Core $stable @('--launcher-self-test')
if ($modern.version -ne '1.0.0' -or $modern.components.orchestrator.version -ne '0.14.0' -or $protocol.launcher_protocol -ne 2) { throw 'Migrated stable entry cannot read modern metadata.' }
$afterLauncher = (Get-FileHash -LiteralPath $stable -Algorithm SHA256).Hash
if ($afterLauncher -eq $beforeLauncher) { throw 'Legacy launcher was retained.' }
$claim = [IO.File]::ReadAllText((Join-Path $testHome '.runtime/config/bridgepilot-home.json')) | ConvertFrom-Json
if ($claim.launcher_sha256 -ne $afterLauncher.ToLowerInvariant()) { throw 'Launcher ownership claim was not migrated.' }
$after = Snapshot $testHome
$null = Invoke-Core $Core @('install', '--bundle', $bundle, '--home', $testHome)
if ((Snapshot $testHome) -cne $after) { throw 'Repeated installation changed owned bytes.' }
Require-Rejection { Invoke-Core $Core @('install-rollback', '--home', $testHome) } 'cross-namespace rollback is blocked'
if ((Snapshot $testHome) -cne $after) { throw 'Blocked rollback modified installation.' }
if (Test-Path -LiteralPath (Join-Path $testHome '.runtime/config/bridgepilot.json')) { throw 'Migration created a private archive binding.' }
$receipt = [ordered]@{
    status='passed'; legacy_version=$legacyIdentity.version; bp_version=$modern.version;
    components=$modern.components; package_schema=2; legacy_launcher_protocol=1; launcher_protocol=2;
    preview_zero_write=$true; tampered_components_rejected=$true; repeated_install_unchanged=$true;
    legacy_reader_rollback_blocked=$true; private_archive_created=$false; home=$testHome; bundle=$bundle
}
$body = $receipt | ConvertTo-Json -Depth 20
[IO.File]::WriteAllText((Join-Path $root 'receipt.json'), $body)
[IO.File]::WriteAllText((Join-Path $repo '.runtime/tests/version-governance/native-smoke-latest.json'), $body)
Write-Output $body
