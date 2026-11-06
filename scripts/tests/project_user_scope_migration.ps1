param(
    [Parameter(Mandatory = $true)][string]$MigratedProjectRoot,
    [string]$RepositoryRoot = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$batch = Join-Path $repository ('.runtime/tests/pw/u' + [Guid]::NewGuid().ToString('N').Substring(0,8))
$source = Join-Path $batch 's'
$fixtureProfile = Join-Path $batch 'profile'
$codex = Join-Path $fixtureProfile '.codex'
$state = Join-Path $codex '.bridgeforge-codex/native-memory-sync'
$legacyHome = Join-Path $fixtureProfile '.bridgeforge-codex'
$legacyCli = Join-Path $codex 'bin/bridgeforge.exe'
$ownedSkill = Join-Path $codex 'skills/develop'
$privateSkill = Join-Path $codex 'skills/personal'
$officialRemote = 'https://github.com/freakybridge/BridgePilot.git'
New-Item -ItemType Directory -Path $source,$legacyHome,$ownedSkill,$privateSkill,$state,(Split-Path -Parent $legacyCli) -Force | Out-Null

function Invoke-FixtureGit([string]$Root, [string[]]$Arguments) {
    $text = & git -C $Root @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Fixture Git failed: $($Arguments[0])" }
    return $text
}
function Commit-Fixture([string]$Root) {
    $null = Invoke-FixtureGit $Root @('init','-b','main')
    $null = Invoke-FixtureGit $Root @('remote','add','origin',$officialRemote)
    $null = Invoke-FixtureGit $Root @('config','core.autocrlf','false')
    $null = Invoke-FixtureGit $Root @('config','core.longpaths','true')
    $null = Invoke-FixtureGit $Root @('config','core.hooksPath','.runtime/no-hooks')
    $null = Invoke-FixtureGit $Root @('add','.')
    $null = Invoke-FixtureGit $Root @('-c','user.name=fixture','-c','user.email=fixture@example.invalid','-c','commit.gpgsign=false','commit','-m','isolation fixture')
    $null = Invoke-FixtureGit $Root @('update-ref','refs/remotes/origin/main','HEAD')
    return (Invoke-FixtureGit $Root @('rev-parse','HEAD')).Trim()
}
function Write-Json($Path,$Value) {
    [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 12), [Text.UTF8Encoding]::new($false))
}
function Copy-FixtureFile([string]$Relative) {
    $origin = [IO.Path]::GetFullPath((Join-Path $repository $Relative))
    $target = [IO.Path]::GetFullPath((Join-Path $source $Relative))
    if (!$origin.StartsWith($repository + '\', [StringComparison]::OrdinalIgnoreCase) -or
        !$target.StartsWith($source + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture copy path escapes its root.' }
    $item = Get-Item -LiteralPath $origin
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Unsafe fixture source: $origin" }
    New-Item -ItemType Directory -Path (Split-Path -Parent $target) -Force | Out-Null
    Copy-Item -LiteralPath $origin -Destination $target
}

# Capture the exact candidate bytes into a disposable checkout. The upstream ref
# here is fixture data; the test does not fetch or claim an official publication.
$paths = @(Invoke-FixtureGit $repository @('-c','core.quotepath=false','ls-files','--cached','--others','--exclude-standard') | Sort-Object -Unique)
foreach ($path in $paths) {
    if (Test-Path -LiteralPath (Join-Path $repository $path) -PathType Leaf) { Copy-FixtureFile $path }
}
foreach ($name in @('bridgeforge.exe','bridgeforge-hook.exe','bridgeforge-factory.exe',
    'build-receipt-cli.json','build-receipt-hook.json','build-receipt-factory.json')) {
    Copy-FixtureFile ".build/bridgeforge/bin/$name"
}
Copy-FixtureFile '.build/bridgeforge/public-ready.json'
$commit = Commit-Fixture $source
[IO.File]::WriteAllText((Join-Path $legacyHome 'VERSION'), 'owned historical checkout')
$null = Commit-Fixture $legacyHome

[IO.File]::WriteAllText($legacyCli, 'verified fixture for the retiring legacy binary')
Write-Json (Join-Path $codex 'bin/bridgeforge-cli-provenance.json') @{schema=1;owner='bridgeforge-user-cli-v1';binary_sha256=(Get-FileHash -LiteralPath $legacyCli -Algorithm SHA256).Hash.ToLowerInvariant()}
[IO.File]::WriteAllText((Join-Path $ownedSkill 'SKILL.md'), 'owned legacy develop')
[IO.File]::WriteAllText((Join-Path $privateSkill 'SKILL.md'), 'personal skill preserved')
$ownedDigest = (Get-FileHash -LiteralPath (Join-Path $ownedSkill 'SKILL.md') -Algorithm SHA256).Hash.ToLowerInvariant()
$recordBytes = [Text.Encoding]::UTF8.GetBytes("SKILL.md`n$ownedDigest`n")
$ownedTree = [Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($recordBytes)).ToLowerInvariant()
$remote = 'https://github.com/offline-fixture/bridgeforge-codex-memories'
$consent = @{decision='approved';policy_version=1;scope='~/.codex/memories/**';sync_mode='bidirectional';auto_hook_maintenance=$true;repository='bridgeforge-codex-memories';require_private=$true;remote=$remote}
$ledgerPath = Join-Path $codex 'bridgeforge-codex-managed.json'
Write-Json $ledgerPath @{schema_version=1;platform='codex';consents=@{native_memories=$consent};records=@{develop=@{source_commit=('a'*40);content_hash="sha256:$ownedTree";installed_at='2026-11-06'}}}
$settings = Join-Path $codex 'config.toml'
[IO.File]::WriteAllText($settings, "[features]`nmemories = true`n[memories]`ngenerate_memories = true`nuse_memories = true`n")
$settingsHash = (Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash
[IO.File]::WriteAllText((Join-Path $state 'remote.txt'), "$remote`n")
$agents = Join-Path $codex 'AGENTS.md'
[IO.File]::WriteAllText($agents, 'legacy user instruction fixture')

$handlers = [ordered]@{}
$oldCommand = $legacyCli.Replace('\','/').Replace("'","''")
$codexCommand = $codex.Replace('\','/').Replace("'","''")
foreach ($event in @('SessionStart','Stop','SessionEnd')) {
    $handler = [ordered]@{type='command';command="'$oldCommand' memory-sync hook-run --event $event --codex-home '$codexCommand'";
        commandWindows="`$ErrorActionPreference = 'Stop'; & '$oldCommand' memory-sync hook-run --event $event --codex-home '$codexCommand'; exit `$LASTEXITCODE";
        bridgeforgeCodexId="bridgeforge-codex.native-memory-sync.v1:$event";timeout=$(if ($event -eq 'SessionEnd') {3} else {120})}
    if ($event -eq 'Stop') { $handler.async = $true }
    $handlers[$event] = @(@{hooks=@($handler)})
}
$handlers['UserPromptSubmit'] = @(@{hooks=@(@{type='command';command='fixture-personal-hook'})})
$hooksPath = Join-Path $codex 'hooks.json'
Write-Json $hooksPath @{hooks=$handlers}
Write-Json (Join-Path $state 'hook-runtime.json') @{schema=1;handlerRevision='bridgeforge-codex.native-memory-sync.v1';lastEvent='SessionStart';executablePath=$legacyCli;verifiedUtc=[DateTime]::UtcNow.ToString('o')}

$beforeUserProfile = $env:USERPROFILE
$beforeCodexHome = $env:CODEX_HOME
$beforeCargoHome = $env:CARGO_HOME
$beforeRustupHome = $env:RUSTUP_HOME
$beforeCargoOffline = $env:CARGO_NET_OFFLINE
try {
    if (!$env:CARGO_HOME) { $env:CARGO_HOME = Join-Path $beforeUserProfile '.cargo' }
    if (!$env:RUSTUP_HOME) { $env:RUSTUP_HOME = Join-Path $beforeUserProfile '.rustup' }
    $env:CARGO_NET_OFFLINE = 'true'
    $bootstrapProfile = Join-Path $batch 'bootstrap-profile'
    $env:USERPROFILE = $bootstrapProfile
    $env:CODEX_HOME = Join-Path $bootstrapProfile '.codex'
    New-Item -ItemType Directory -Path $env:CODEX_HOME -Force | Out-Null
    $prepareEntry = Join-Path $source 'source/forge/templates/maintenance/bridgeforge-codex/scripts/prepare-project.ps1'
    $prepared = & $prepareEntry -SourceRoot $source -SourceCommit $commit | ConvertFrom-Json
    if ($prepared.status -ne 'prepared' -or $prepared.source_commit -ne $commit -or
        (Test-Path -LiteralPath (Join-Path $bootstrapProfile '.bridgeforge-codex')) -or
        (Test-Path -LiteralPath (Join-Path $env:CODEX_HOME 'bin/bridgeforge.exe'))) { throw 'Project bootstrap depended on a user installation.' }
    $env:USERPROFILE = $fixtureProfile
    $env:CODEX_HOME = $codex
    $output = & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $source 'scripts/bridgeforge_codex_shared_update.ps1') `
        -MigrateUserScope -SourceRepositoryRoot $source -PreparedSourceCommit $commit -MigratedProjectRoot $MigratedProjectRoot
    if ($LASTEXITCODE -ne 0) { throw "Isolated user-scope migration failed: $($output -join ' ')" }
} finally {
    $env:USERPROFILE = $beforeUserProfile
    $env:CODEX_HOME = $beforeCodexHome
    $env:CARGO_HOME = $beforeCargoHome
    $env:RUSTUP_HOME = $beforeRustupHome
    $env:CARGO_NET_OFFLINE = $beforeCargoOffline
}
$receipt = ($output -join "`n") | ConvertFrom-Json
if ($receipt.status -ne 'user_scope_migrated' -or $receipt.cleanup_pending -or !$receipt.memory.changed -or !$receipt.memory.hookInstalled -or !$receipt.memory.enabled) {
    throw 'Missing isolated user migration or Memory receipt.'
}
foreach ($path in @($legacyHome,$legacyCli,$ownedSkill)) { if (Test-Path -LiteralPath $path) { throw "Legacy Forge asset remains active: $path" } }
if ([IO.File]::ReadAllText((Join-Path $privateSkill 'SKILL.md')) -ne 'personal skill preserved') { throw 'Unmanaged user Skill changed.' }
if ((Get-FileHash -LiteralPath $settings -Algorithm SHA256).Hash -ne $settingsHash) { throw 'Native Memory settings changed.' }
if ((Get-Content -LiteralPath $ledgerPath -Raw | ConvertFrom-Json).consents.native_memories.remote -ne $remote) { throw 'Native Memory authorization changed.' }
$hooks = Get-Content -LiteralPath $hooksPath -Raw | ConvertFrom-Json
if ($hooks.hooks.UserPromptSubmit[0].hooks[0].command -ne 'fixture-personal-hook') { throw 'Third-party Hook changed.' }
$dedicated = Join-Path $state 'bin/bridgeforge-memory-hook.exe'
Push-Location -LiteralPath $fixtureProfile
try {
    $initialStatus = & $dedicated memory-sync status --codex-home $codex | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or !$initialStatus.hookInstalled -or $initialStatus.hookRuntimeVerified) { throw 'Dedicated runtime or legacy receipt scope is incorrect.' }
    # A fresh reservation forces worker reuse and keeps this fixture offline.
    foreach ($event in @('SessionStart','Stop','SessionEnd')) {
        Write-Json (Join-Path $state 'worker.json') @{schemaVersion=1;token='fixture-worker';pid=0;launcherPid=$PID;startedUtc=[DateTime]::UtcNow.ToString('o')}
        $null = & $dedicated memory-sync hook-run --event $event --codex-home $codex
        if ($LASTEXITCODE -ne 0) { throw "Dedicated Memory Hook failed outside a project: $event" }
        $attempt = Get-Content -LiteralPath (Join-Path $state 'hook-attempt.json') -Raw | ConvertFrom-Json
        if ($attempt.event -ne $event -or $attempt.detail -ne 'worker-reused') { throw "Missing offline worker reuse proof: $event" }
    }
    $status = & $dedicated memory-sync status --codex-home $codex | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0 -or !$status.hookRuntimeVerified -or (Test-Path -LiteralPath (Join-Path $state 'last-synced.json'))) {
        throw 'Dedicated runtime proof or offline boundary is incorrect.'
    }
} finally { Pop-Location }

$result = [ordered]@{schema=1;status='passed';fixture=$batch;source_commit=$commit;bootstrap=$prepared;user_migration=$receipt;memory=$status;remote_operations=0}
Write-Json (Join-Path $batch 'result.json') $result
[ordered]@{status='passed';fixture=$batch;receipt=(Join-Path $batch 'result.json')} | ConvertTo-Json -Compress
