#requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory)][string]$PreviousBundle,
      [string]$Core, [string]$Launcher)
$ErrorActionPreference='Stop'
$repo=Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
if (!$Core) {$Core=Join-Path $repo '.build/bridgeforge/test-target/debug/bp.exe'}
if (!$Launcher) {$Launcher=Join-Path $repo '.build/bridgeforge/test-target/debug/bp-launcher.exe'}
$fixture=Join-Path $repo ('.runtime/tests/bp-update-migration/'+[guid]::NewGuid().ToString('N'))
$installedHome=Join-Path $fixture 'home'
$skills=Join-Path $fixture 'skills'
$newBundle=Join-Path $fixture 'candidate'
New-Item -ItemType Directory -Path $fixture -Force | Out-Null
function Invoke-MigrationBp([string[]]$Arguments) {
    $raw=& $Core @Arguments
    if ($LASTEXITCODE -ne 0) {throw ('BP migration fixture failed: '+($Arguments -join ' '))}
    return ($raw | ConvertFrom-Json)
}
$previous=Get-Content -LiteralPath (Join-Path $PreviousBundle 'package.json') -Raw | ConvertFrom-Json
$null=Invoke-MigrationBp @('install','--bundle',$PreviousBundle,'--home',$installedHome)
$null=Invoke-MigrationBp @('skill-install','--home',$installedHome,'--skills-root',$skills)
if (!(Test-Path -LiteralPath (Join-Path $skills 'bridgeforge-update/SKILL.md'))) {throw 'Legacy entry not installed'}
$null=Invoke-MigrationBp @('bundle','--output',$newBundle,'--core',$Core,'--launcher',$Launcher)
$null=Invoke-MigrationBp @('install-preview','--bundle',$newBundle,'--home',$installedHome)
$null=Invoke-MigrationBp @('install','--bundle',$newBundle,'--home',$installedHome)
$null=Invoke-MigrationBp @('skill-install-preview','--home',$installedHome,'--skills-root',$skills)
$null=Invoke-MigrationBp @('skill-install','--home',$installedHome,'--skills-root',$skills)
$status=Invoke-MigrationBp @('skill-status','--home',$installedHome,'--skills-root',$skills)
if (!$status.current -or !$status.skills.'bridgepilot-update'.current) {throw 'Renamed Skill not current'}
foreach ($old in @((Join-Path $skills 'bridgeforge-update/SKILL.md'),(Join-Path $skills 'bridgeforge-update/bridgepilot-local.json'),(Join-Path $installedHome 'source/orchestrator/skills/bridgeforge-update/SKILL.md'))) {
    if (Test-Path -LiteralPath $old) {throw 'Managed legacy entry remained active'}
}
$repeat=Invoke-MigrationBp @('skill-install','--home',$installedHome,'--skills-root',$skills)
if ($repeat.status -ne 'user_skill_current') {throw 'Repeated projection was not a no-op'}
@{status='passed';from_version=$previous.version;to_version=$status.binding.core_version;legacy_entry_retired=$true;new_entry_current=$true;fixture=$fixture;real_home_modified=$false} | ConvertTo-Json | Tee-Object -FilePath (Join-Path $fixture 'result.json')
