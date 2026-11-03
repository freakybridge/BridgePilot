param([string]$RepositoryRoot)
$ErrorActionPreference='Stop'
Set-StrictMode -Version 2
$tokens=$null;$errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($function in $ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true)) {
    if ($function.Name -ne 'Invoke-Main') { . ([scriptblock]::Create($function.Extent.Text)) }
}
$CanonicalRemote='https://github.com/freakybridge/BridgePilot.git'
$script:calls=0;$script:oldId=42
function Get-GitHubRepositoryIdentity {
    param([string]$Remote)
    $script:calls++
    $id=if ($Remote -like '*BridgeForgeCodex*') { $script:oldId } else { 42 }
    return [pscustomobject]@{ id=$id;full_name='freakybridge/BridgePilot';html_url='https://github.com/freakybridge/BridgePilot' }
}
if (-not (Test-CanonicalHomeRemote $CanonicalRemote) -or $script:calls -ne 0) { throw 'Canonical origin must not add network work.' }
if (-not (Test-CanonicalHomeRemote 'https://github.com/freakybridge/BridgeForgeCodex.git') -or $script:calls -ne 2) { throw 'Verified rename was not recognized.' }
$script:oldId=99
if (Test-CanonicalHomeRemote 'https://github.com/freakybridge/BridgeForgeCodex.git') { throw 'Recreated old repository must not be adopted.' }
$before=$script:calls
if ((Test-CanonicalHomeRemote 'https://github.com/another/BridgeForgeCodex.git') -or $script:calls -ne $before) { throw 'Arbitrary home must not be probed or adopted.' }
function Get-GitHubRepositoryIdentity { param([string]$Remote) throw 'simulated service unavailable' }
$rejected=$false
try { Test-CanonicalHomeRemote 'https://github.com/freakybridge/BridgeForgeCodex.git' | Out-Null } catch { $rejected=$true }
if (-not $rejected) { throw 'Unavailable identity must block safely.' }
Write-Output '5 canonical-home rename and no-extra-normal-network scenarios passed'

# An already-current commit with the old URL must still use the owned swap.
$CommandHomeName='.bridgeforge-codex';$CommandHomeLogName='.bridgeforge-codex-home-update.json';$CanonicalBranch='main'
$script:fixtureRoot=Join-Path $RepositoryRoot ('.runtime/tests/native-release/home-rename-' + [Guid]::NewGuid().ToString('N'))
$profile=Join-Path $script:fixtureRoot 'profile';$source=Join-Path $script:fixtureRoot 'source';$script:fixtureHome=Join-Path $profile $CommandHomeName
New-Item -ItemType Directory -Path $script:fixtureHome,$source -Force | Out-Null
[IO.File]::WriteAllText((Join-Path $source 'marker.txt'),'verified canonical source')
function Get-RepositoryIdentity {
    param([string]$Root)
    return @{commit=('a'*40);remote=$(if ($Root -eq $script:fixtureHome) {'https://github.com/freakybridge/BridgeForgeCodex.git'} else {$CanonicalRemote})}
}
$plan=New-CommandHomePlan -SourceRoot $source -UserProfile $profile -OperationId 'test' -Commit ('a'*40)
if (-not $plan.needs_swap -or -not $plan.had_original -or -not (Test-Path -LiteralPath (Join-Path $plan.stage 'marker.txt'))) { throw 'Same-commit renamed home must use the existing staged swap.' }
$script:BuildCounts=@{source_reused=0;source_clones=0}
function Invoke-Git { param([string[]]$Arguments,[string]$WorkingDirectory) if ($Arguments[0] -eq 'ls-remote') { return (('a'*40) + "`trefs/heads/main") } }
function New-CanonicalClone { return $source }
function Assert-Repository { param([string]$Root,[switch]$SkipFetch,[string]$ExpectedCommit,[string]$InstalledHome) if ($Root -eq $script:fixtureHome) { throw 'Old origin must not enter strict source reuse.' };return ('a'*40) }
$selected=Select-CanonicalSource -UserProfile $profile
if ($selected.root -ne $source -or $script:BuildCounts.source_clones -ne 1 -or $script:BuildCounts.source_reused -ne 0) { throw 'Same-commit renamed source must clone canonical input.' }
Write-Output '2 same-commit rename source/swap scenarios passed'
