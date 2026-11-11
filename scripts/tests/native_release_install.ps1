[CmdletBinding()]
param([string]$ProjectRoot = (Resolve-Path (Join-Path $PSScriptRoot '../..')).Path)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$cli = Join-Path $ProjectRoot '.build/bridgeforge/bin/bridgeforge.exe'
$batch = Join-Path $ProjectRoot ('.runtime/tests/release-entry-install/' + [guid]::NewGuid().ToString('N'))
$fixture = Join-Path $batch 'project'
New-Item -ItemType Directory -Path $fixture | Out-Null

function Invoke-FixtureGit([string[]]$Arguments) {
    $output = & git.exe -C $fixture @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw ('Fixture Git failed: ' + ($output -join "`n")) }
}
function Tree {
    $result = [ordered]@{}
    Get-ChildItem -LiteralPath $fixture -Recurse -Force -File | Sort-Object FullName | ForEach-Object {
        $result[$_.FullName.Substring($fixture.Length + 1)] = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash
    }
    return ($result | ConvertTo-Json -Compress)
}
function Move-FixturePath([string]$Source, [string]$Destination) {
    $root = [IO.Path]::GetFullPath($batch).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    $from = (Resolve-Path -LiteralPath $Source).Path
    $to = [IO.Path]::GetFullPath($Destination)
    if (!$from.StartsWith($root, [StringComparison]::OrdinalIgnoreCase) -or !$to.StartsWith($root, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Fixture move escapes its test directory'
    }
    New-Item -ItemType Directory -Path (Split-Path -Parent $to) -Force | Out-Null
    Move-Item -LiteralPath $from -Destination $to
}
function Invoke-NativeCli([string[]]$Arguments, [bool]$ExpectFailure = $false) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $output = & $cli @Arguments 2>&1
    $code = $LASTEXITCODE
    $timer.Stop()
    if ($ExpectFailure) {
        if ($code -eq 0) { throw 'Expected rejection was accepted' }
        return [pscustomobject]@{ exit_code = $code; output = ($output -join "`n"); elapsed_ms = $timer.ElapsedMilliseconds }
    }
    if ($code -ne 0) { throw ('Native CLI failed: ' + ($output -join "`n")) }
    return [pscustomobject]@{ value = (($output -join "`n") | ConvertFrom-Json); elapsed_ms = $timer.ElapsedMilliseconds }
}

Set-Content -LiteralPath (Join-Path $fixture '.gitignore') -Value ".runtime/`n.build/`n.deps/" -Encoding utf8NoBOM
Set-Content -LiteralPath (Join-Path $fixture 'VERSION') -Value '1.0.0' -Encoding utf8NoBOM
Set-Content -LiteralPath (Join-Path $fixture 'CHANGELOG.md') -Value "# Changes`n`n## [1.0.0]`n`n- Initial source." -Encoding utf8NoBOM
Invoke-FixtureGit @('init','-b','main')
Invoke-FixtureGit @('config','user.name','fixture')
Invoke-FixtureGit @('config','user.email','fixture@example.invalid')
Invoke-FixtureGit @('config','commit.gpgsign','false')
Invoke-FixtureGit @('config','core.hooksPath','.runtime/no-hooks')
Invoke-FixtureGit @('remote','add','origin','https://github.com/fixture/project.git')
Invoke-FixtureGit @('config','branch.main.remote','origin')
Invoke-FixtureGit @('config','branch.main.merge','refs/heads/main')
Invoke-FixtureGit @('add','.')
Invoke-FixtureGit @('commit','-m','chore: fixture')
Invoke-FixtureGit @('update-ref','refs/remotes/origin/main','HEAD')
$specPath = Join-Path $batch 'spec.json'
@{schema=1;product='Fixture';repository='fixture/project';version_file='VERSION';changelog='CHANGELOG.md';tag_prefix='v';include=@('VERSION','CHANGELOG.md','Cargo.toml');build=@();artifacts=@()} |
    ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $specPath -Encoding utf8NoBOM
$argsBase = @('project-sync','--project-root',$fixture,'--template-root',$ProjectRoot,'--mode','update')
$original = Tree
$preview = Invoke-NativeCli ($argsBase + @('--release-spec',$specPath))
if ($preview.value.gaps.Count -ne 0) { throw 'Configured fixture has readiness gaps' }
foreach ($path in @('Cargo.toml','.build/project-release/bin/bridgeforge.exe','.build/project-release/bin/receipt.json',
    '.codex/skills/develop/SKILL.md','.codex/skills/git-sync/references/first-use.md',
    '.codex/maintenance/bridgeforge-codex/MAINTENANCE.md')) {
    if ($preview.value.safe -notcontains $path) { throw ('Missing exact planned target: ' + $path) }
}
if ((Tree) -ne $original) { throw 'Dry-run wrote fixture files' }
$installed = Invoke-NativeCli ($argsBase + @('--release-spec',$specPath,'--apply','--confirmed-risk','--plan-fingerprint',$preview.value.aggregate_fingerprint))
if ($installed.value.status -ne 'succeeded' -or $installed.value.release_installation.status -ne 'ready') { throw 'Installed entry not ready' }
$check = Invoke-NativeCli @('project-release','check','--project-root',$fixture)
if ($check.value.status -ne 'ready') { throw 'Native driver check failed' }
$after = Tree
$repeat = Invoke-NativeCli $argsBase
if ($repeat.value.safe.Count -or $repeat.value.risk.Count -or $repeat.value.gaps.Count) { throw 'Repeated plan is not empty' }
$noop = Invoke-NativeCli ($argsBase + @('--apply','--plan-fingerprint',$repeat.value.aggregate_fingerprint))
if ($noop.value.written -ne 0 -or $null -ne $noop.value.recovery -or (Tree) -ne $after) { throw 'No-op rewrote files or created recovery artifacts' }

# Model the exact old Forge layout, including custom content outside the declared files.
$manifest = Get-Content -LiteralPath (Join-Path $ProjectRoot 'source/forge/bridgeforge-manifest.json') -Raw | ConvertFrom-Json
$relocated = @($manifest.project_skeleton.assets | Where-Object { $_.PSObject.Properties['relocation'] })
if ($relocated.Count -ne 35) { throw 'Expected 35 explicitly registered workflow relocations' }
foreach ($asset in $relocated) {
    Move-FixturePath (Join-Path $fixture $asset.target) (Join-Path $fixture $asset.relocation.previous_target)
}
$customReference = Join-Path $fixture '.agents/skills/develop/references/custom.txt'
[IO.File]::WriteAllText($customReference, 'preserve custom reference')
$oldLayout = Tree
$relocation = Invoke-NativeCli $argsBase
if ($relocation.value.retired.Count -ne 35 -or (Tree) -ne $oldLayout) { throw 'Old workflow layout was not planned without writes' }
$moved = Invoke-NativeCli ($argsBase + @('--apply','--confirmed-risk','--plan-fingerprint',$relocation.value.aggregate_fingerprint))
if ($moved.value.status -ne 'succeeded' -or [IO.File]::ReadAllText($customReference) -ne 'preserve custom reference') { throw 'Workflow relocation failed preservation' }
foreach ($asset in $relocated) {
    if (!(Test-Path -LiteralPath (Join-Path $fixture $asset.target)) -or (Test-Path -LiteralPath (Join-Path $fixture $asset.relocation.previous_target))) { throw 'Old and new Forge workflow entries coexist' }
}

# Model an already installed runtime that predates project-scoped workflows.
Move-FixturePath (Join-Path $fixture '.codex/skills') (Join-Path $batch 'v2-skill-copy')
Move-FixturePath (Join-Path $fixture '.codex/maintenance') (Join-Path $batch 'v2-maintenance-copy')
$private = Join-Path $fixture '.agents/skills/personal/SKILL.md'
New-Item -ItemType Directory -Path (Split-Path -Parent $private) -Force | Out-Null
[IO.File]::WriteAllText($private, 'personal fixture skill')
$previousScope = Tree
$upgrade = Invoke-NativeCli $argsBase
if ($upgrade.value.safe -notcontains '.codex/skills/develop/SKILL.md' -or (Tree) -ne $previousScope) { throw 'Old installation preview did not plan a zero-write project Skill migration' }
$migrated = Invoke-NativeCli ($argsBase + @('--apply','--confirmed-risk','--plan-fingerprint',$upgrade.value.aggregate_fingerprint))
if ($migrated.value.status -ne 'succeeded' -or [IO.File]::ReadAllText($private) -ne 'personal fixture skill') { throw 'Project workflow upgrade or preservation failed' }

$legacy = Join-Path $fixture '.codex/skills/develop/SKILL.md'
$managedSkillBytes = [IO.File]::ReadAllBytes($legacy)
New-Item -ItemType Directory -Path (Split-Path -Parent $legacy) -Force | Out-Null
[IO.File]::WriteAllText($legacy, 'personal same-name skill')
$collision = Tree
$collisionResult = Invoke-NativeCli $argsBase $true
if ($collisionResult.output -notlike '*unowned or modified project workflow*' -or (Tree) -ne $collision) { throw 'Same-name project Skill was not preserved' }
Move-FixturePath $legacy (Join-Path $batch 'preserved-same-name.md')
[IO.File]::WriteAllBytes($legacy, $managedSkillBytes)

Set-Content -LiteralPath (Join-Path $fixture 'VERSION') -Value '1.0.1' -Encoding utf8NoBOM
$drifted = Tree
$rejected = Invoke-NativeCli ($argsBase + @('--apply','--plan-fingerprint',$repeat.value.aggregate_fingerprint)) $true
if ((Tree) -ne $drifted) { throw 'Stale fingerprint rejection wrote files' }
$result = [ordered]@{schema=1;status='passed';fixture=$fixture;dry_run_zero_write=$true;runtime_targets_planned=$true;installed=$installed.value;noop=$noop.value;project_workflows_migrated=$true;same_name_conflict_preserved=$true;stale_fingerprint_zero_write=$true;timings_ms=@{plan=$preview.elapsed_ms;apply=$installed.elapsed_ms;repeat_plan=$repeat.elapsed_ms;noop_apply=$noop.elapsed_ms};platform_network_calls=0}
$result | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath (Join-Path $batch 'result.json') -Encoding utf8NoBOM
$result | ConvertTo-Json -Depth 12 -Compress
