#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$Core,
    [Parameter(Mandatory = $true)][string]$Bundle,
    [Parameter(Mandatory = $true)][string]$LegacyBundle
)
$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$root = Join-Path $repo ('.runtime/tests/bridgepilot-git-sync/' + [guid]::NewGuid().ToString('N'))
$testHome = Join-Path $root 'home'
$skills = Join-Path $root 'skills'
$trace = New-Object Collections.Generic.List[object]
function Invoke-Core([string[]]$Arguments) {
    $raw = & $Core @Arguments
    if ($LASTEXITCODE -ne 0) { throw "BP command failed: $($Arguments -join ' ')" }
    $value = $raw | ConvertFrom-Json
    $trace.Add(@{ command = $Arguments; receipt = $value })
    return $value
}
$null = Invoke-Core @('install-preview', '--bundle', $LegacyBundle, '--home', $testHome)
if (Test-Path -LiteralPath $testHome) { throw 'Legacy preview wrote the installation home.' }
$null = Invoke-Core @('install', '--bundle', $LegacyBundle, '--home', $testHome)
$old = Invoke-Core @('install-status', '--home', $testHome)
if ($old.installation.current.version -ne '2.0.2') { throw 'This regression requires the retained 2.0.2 package.' }
$null = Invoke-Core @('install-preview', '--bundle', $Bundle, '--home', $testHome)
$null = Invoke-Core @('install', '--bundle', $Bundle, '--home', $testHome)
$stable = Join-Path $testHome '.build/bridgepilot/bin/bp.exe'
$identity = & $stable self-test | ConvertFrom-Json
if ($LASTEXITCODE -ne 0 -or 'bp-git-sync-skill-v1' -notin $identity.capabilities) { throw 'Stable launcher did not select the new core.' }
$null = Invoke-Core @('skill-install-preview', '--home', $testHome, '--skills-root', $skills)
if (Test-Path -LiteralPath $skills) { throw 'Skill preview wrote its target.' }
$null = Invoke-Core @('skill-install', '--home', $testHome, '--skills-root', $skills)
$status = Invoke-Core @('skill-status', '--home', $testHome, '--skills-root', $skills)
if (!$status.current -or !$status.skills.'bridgepilot-git-sync'.current -or @($status.skills.PSObject.Properties).Count -ne 4) { throw 'All four managed skills must be current.' }
$null = Invoke-Core @('skill-install', '--home', $testHome, '--skills-root', $skills)
$before = Get-Content -LiteralPath (Join-Path $testHome '.runtime/config/bridgepilot-installation.json') -Raw
$previousErrorAction = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
try { $rollback = & $Core install-rollback --home $testHome 2>&1 }
finally { $ErrorActionPreference = $previousErrorAction }
if ($LASTEXITCODE -eq 0 -or ($rollback -join '') -notmatch 'cannot maintain the bridgepilot-git-sync Skill') { throw 'Rollback to a core without managed git-sync was not blocked.' }
$after = Get-Content -LiteralPath (Join-Path $testHome '.runtime/config/bridgepilot-installation.json') -Raw
if ($before -cne $after) { throw 'Blocked rollback changed the installation identity.' }
if (!(Test-Path -LiteralPath (Join-Path $skills 'bridgepilot-git-sync/scripts/list_projects.py'))) { throw 'Managed project-list script is missing.' }
$receipt = @{ status = 'passed'; legacy_version = '2.0.2'; candidate_version = $identity.version; managed_skills = 4; rollback_blocked = $true; trace = $trace.ToArray() }
$null = New-Item -ItemType Directory -Path $root -Force
$receiptPath = Join-Path $root 'receipt.json'
[IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 30), (New-Object Text.UTF8Encoding($false)))
@{ status = 'passed'; receipt = $receiptPath; candidate_version = $identity.version; managed_skills = 4 } | ConvertTo-Json -Compress
