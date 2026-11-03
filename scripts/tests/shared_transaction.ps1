param([string]$RepositoryRoot, [string]$Base)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
$tokens = $null
$errors = $null
$source = Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'
$ast = [System.Management.Automation.Language.Parser]::ParseFile($source, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($function in $ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true)) {
    if ($function.Name -ne 'Invoke-Main') { . ([scriptblock]::Create($function.Extent.Text)) }
}
$ManifestName = 'source/forge/bridgeforge-manifest.json'
$CommandHomeName = '.bridgeforge-codex'
$CommandHomeLogName = '.bridgeforge-codex-home-update.json'
$TestCrashAfterActionCount = 0
$TestFailAfterSwap = ''

function Assert-True {
    param([bool]$Value, [string]$Message)
    if (-not $Value) { throw $Message }
}

foreach ($scenario in @('rollback', 'committed-cleanup')) {
    $fixtureProfile = Join-Path $Base $scenario
    $repo = Join-Path $fixtureProfile 'source'
    $bin = Join-Path $fixtureProfile '.codex/bin'
    $skills = Join-Path $fixtureProfile '.codex/skills'
    New-Item -ItemType Directory -Path $repo, $bin, $skills -Force | Out-Null
    New-Item -ItemType Directory -Path (Split-Path -Parent (Join-Path $repo $ManifestName)) -Force | Out-Null
    Copy-Item -LiteralPath (Join-Path $RepositoryRoot $ManifestName) -Destination (Join-Path $repo $ManifestName)
    [IO.File]::WriteAllText((Join-Path $repo 'SKILL.md'), 'new skill')
    $op = [Guid]::NewGuid().ToString('N')
    $commit = 'a' * 40
    $bundleHome = [ordered]@{
        target = Join-Path $fixtureProfile $CommandHomeName
        stage = Join-Path $fixtureProfile ".bridgeforge-codex-stage-$op"
        backup = Join-Path $fixtureProfile ".bridgeforge-codex-backup-$op"
        log = Join-Path $fixtureProfile $CommandHomeLogName
        operation_id = $op
        had_original = $true
        needs_swap = $true
        status = 'staged'
    }
    New-Item -ItemType Directory -Path $bundleHome.target, $bundleHome.stage -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $bundleHome.target 'VERSION'), 'old home')
    [IO.File]::WriteAllText((Join-Path $bundleHome.stage 'VERSION'), 'new home')
    $cli = [ordered]@{
        target = Join-Path $bin 'bridgeforge.exe'
        stage = Join-Path $bin ".bridgeforge-stage-$op.exe"
        backup = Join-Path $bin ".bridgeforge-backup-$op.exe"
        had_original = $true
        needs_swap = $true
        status = 'staged'
    }
    Copy-Item -LiteralPath (Join-Path $RepositoryRoot '.build/bridgeforge/bin/bridgeforge-hook.exe') -Destination $cli.target
    [IO.File]::WriteAllText($cli.stage, 'new cli transaction bytes')
    $oldCliHash = Get-Sha256 -Path $cli.target
    $userTarget = Join-Path $fixtureProfile '.codex/AGENTS.md'
    [IO.File]::WriteAllBytes($userTarget, [byte[]]@(255, 0, 34, 13, 10))
    $oldUserHash = Get-Sha256 -Path $userTarget
    $userPlan = New-UserAgentsPlan -RepositoryRoot $RepositoryRoot -UserProfile $fixtureProfile -OperationId $op -RustCliPlan @{ needs_swap = $false; target = (Join-Path $RepositoryRoot '.build/bridgeforge/bin/bridgeforge.exe') }
    $fileHash = Get-Sha256 -Path (Join-Path $repo 'SKILL.md')
    $manifest = @{ platforms = @{ codex = @{ skills = @(@{ name = 'probe'; files = @(@{ source = 'SKILL.md'; target = 'SKILL.md'; sha256 = $fileHash }) }) } } } | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId $op -Commit $commit)
    $log = Join-Path $fixtureProfile '.bridgeforge-codex-shared-update.json'
    $script:CleanupPending = $false
    $child = $null
    try {
        if ($scenario -eq 'committed-cleanup') {
            $info = New-Object Diagnostics.ProcessStartInfo
            $info.FileName = $cli.target
            $info.Arguments = 'pre-tool'
            $info.UseShellExecute = $false
            $info.CreateNoWindow = $true
            $info.RedirectStandardInput = $true
            $info.RedirectStandardOutput = $true
            $info.RedirectStandardError = $true
            $child = New-Object Diagnostics.Process
            $child.StartInfo = $info
            Assert-True ($child.Start()) 'Cannot launch image holder'
            Start-Sleep -Milliseconds 150
            Assert-True (-not $child.HasExited) 'Image holder unexpectedly exited'
        }
        $TestFailAfterSwap = if ($scenario -eq 'rollback') { 'codex:1' } else { '' }
        $failed = $false
        try {
            Invoke-UpdateTransaction -RepositoryRoot $repo -Manifest $manifest -Commit $commit -ManifestHash ("sha256:$(Get-Sha256 -Path (Join-Path $RepositoryRoot $ManifestName))") -UserProfile $fixtureProfile -LogPath $log -OperationId $op -PlatformPlans $plans -CommandHomePlan $bundleHome -RustCliPlan $cli -UserAgentsPlan $userPlan
        }
        catch {
            if ($scenario -ne 'rollback' -or $_.Exception.Message -notlike '*Injected test failure*') { throw }
            $failed = $true
        }
        if ($scenario -eq 'rollback') {
            Assert-True $failed 'Expected rollback injection'
            Assert-True ((Get-Sha256 -Path $cli.target) -eq $oldCliHash) 'CLI was not rolled back'
            Assert-True ([IO.File]::ReadAllText((Join-Path $bundleHome.target 'VERSION')) -eq 'old home') 'Home was not rolled back'
            Assert-True (-not (Test-Path (Join-Path $skills 'probe'))) 'Skill was not rolled back'
            Assert-True (-not (Test-Path $log)) 'Successful rollback left a journal'
            Assert-True ((Get-Sha256 -Path $userTarget) -eq $oldUserHash) 'User instructions were not rolled back byte for byte'
            Assert-True (-not (Test-Path $userPlan.backup)) 'Rollback left a redundant backup'
        }
        else {
            Assert-True $script:CleanupPending 'Running image should defer backup cleanup'
            Assert-True ((Read-JsonFile -Path $log).committed) 'Shared durable commit is missing'
            Assert-True ([IO.File]::ReadAllText($cli.target) -eq 'new cli transaction bytes') 'Committed CLI was reverted'
            Assert-True ([IO.File]::ReadAllText((Join-Path $bundleHome.target 'VERSION')) -eq 'new home') 'Committed Home was reverted'
            Assert-True ([IO.File]::ReadAllText((Join-Path $skills 'probe/SKILL.md')) -eq 'new skill') 'Committed Skill was reverted'
            $child.Kill()
            $child.WaitForExit()
            Restore-InterruptedOperation -LogPath $log -UserProfile $fixtureProfile
            Assert-True (-not (Test-Path $log)) 'Deferred cleanup did not recover'
            Assert-True (-not (Test-Path $cli.backup)) 'CLI backup was not cleaned after process exit'
            Assert-True ([IO.File]::ReadAllText($cli.target) -eq 'new cli transaction bytes') 'Cleanup changed committed CLI'
            Assert-True ((Get-Sha256 -Path $userTarget) -eq $userPlan.desired_hash) 'Committed user instructions were reverted'
            Assert-True ((Get-Sha256 -Path $userPlan.backup) -eq $oldUserHash) 'Committed cleanup deleted or changed the user backup'
        }
    }
    finally {
        if ($null -ne $child) {
            if (-not $child.HasExited) { $child.Kill(); $child.WaitForExit() }
            $child.Dispose()
        }
    }
}
foreach ($scenario in @('create', 'noop', 'failure', 'interrupted', 'drift', 'backup-conflict')) {
    $script:CleanupPending = $false
    $fixtureProfile = Join-Path $Base ("user-" + [char]0x7528 + [char]0x6237 + "-$scenario")
    New-Item -ItemType Directory -Path (Join-Path $fixtureProfile '.codex') -Force | Out-Null
    $userTarget = Join-Path $fixtureProfile '.codex/AGENTS.md'
    if ($scenario -ne 'create') { [IO.File]::WriteAllText($userTarget, 'local preference') }
    if ($scenario -eq 'noop') { Copy-Item -LiteralPath (Join-Path $RepositoryRoot 'source/forge/templates/user/AGENTS.md') -Destination $userTarget }
    $op = [Guid]::NewGuid().ToString('N')
    $originalEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = [Text.Encoding]::GetEncoding(437)
        $userPlan = New-UserAgentsPlan -RepositoryRoot $RepositoryRoot -UserProfile $fixtureProfile -OperationId $op -RustCliPlan @{ needs_swap = $false; target = (Join-Path $RepositoryRoot '.build/bridgeforge/bin/bridgeforge.exe') }
        Assert-True ([Console]::OutputEncoding.CodePage -eq 437) 'CLI staging did not restore console encoding'
    }
    finally { [Console]::OutputEncoding = $originalEncoding }
    $log = Join-Path $fixtureProfile '.bridgeforge-codex-shared-update.json'
    $manifest = @{ platforms = @{ codex = @{ skills = @() } } } | ConvertTo-Json -Depth 8 | ConvertFrom-Json
    $plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId $op -Commit ('a' * 40))
    $TestFailAfterSwap = if ($scenario -eq 'failure') { 'user-agents' } else { '' }
    if ($scenario -eq 'drift') { [IO.File]::WriteAllText($userTarget, 'external edit') }
    if ($scenario -eq 'backup-conflict') { [IO.File]::WriteAllText($userPlan.backup, 'external backup') }
    $failed = $false
    try {
        if ($scenario -eq 'interrupted') {
            Write-JsonAtomic -Path $log -Value @{ schema_version = 1; operation_id = $op; committed = $false; platforms = $plans; bundles = @($userPlan) }
            Install-UserAgents -Plan $userPlan -UserProfile $fixtureProfile -OperationId $op
            Restore-InterruptedOperation -LogPath $log -UserProfile $fixtureProfile
        }
        elseif ($scenario -in @('drift', 'backup-conflict')) {
            Install-UserAgents -Plan $userPlan -UserProfile $fixtureProfile -OperationId $op
        }
        else {
            Invoke-UpdateTransaction -RepositoryRoot $RepositoryRoot -Manifest $manifest -Commit ('a' * 40) -ManifestHash ("sha256:$(Get-Sha256 -Path (Join-Path $RepositoryRoot $ManifestName))") -UserProfile $fixtureProfile -LogPath $log -OperationId $op -PlatformPlans $plans -UserAgentsPlan $userPlan
        }
    }
    catch {
        if ($scenario -notin @('failure', 'drift', 'backup-conflict')) { throw }
        $failed = $true
    }
    if ($scenario -in @('failure', 'drift', 'backup-conflict')) { Assert-True $failed "Expected rejection for $scenario" }
    if ($scenario -in @('failure', 'interrupted', 'backup-conflict')) {
        Assert-True ([IO.File]::ReadAllText($userTarget) -eq 'local preference') "Original file lost for $scenario"
    }
    elseif ($scenario -eq 'drift') {
        Assert-True ([IO.File]::ReadAllText($userTarget) -eq 'external edit') 'Concurrent edit was overwritten'
    }
    else {
        Assert-True ((Get-Sha256 -Path $userTarget) -eq $userPlan.desired_hash) "Installed content differs for $scenario"
        Assert-True (-not (Test-Path $userPlan.backup)) "Unnecessary backup for $scenario"
    }
    if ($scenario -eq 'noop') { Assert-True (-not $userPlan.needs_swap) 'Identical content was scheduled for replacement' }
    if ($scenario -in @('create', 'noop', 'failure', 'interrupted')) {
        Assert-True (-not (Test-Path $log)) "Completed recovery left a journal for $scenario"
        Assert-True (-not $script:CleanupPending) "Cleanup was unexpectedly deferred for $scenario"
    }
    if ($scenario -eq 'backup-conflict') { Assert-True ([IO.File]::ReadAllText($userPlan.backup) -eq 'external backup') 'Existing backup was overwritten' }
}
# Exercise the new Skill through the existing transaction, in this isolated profile only.
# Source provenance (canonical main) is tested by the updater; this case checks packaging and swap.
$fixtureProfile = Join-Path $Base 'autopilot-install'
New-Item -ItemType Directory -Path (Join-Path $fixtureProfile '.codex/skills') -Force | Out-Null
$catalog = ([IO.File]::ReadAllText((Join-Path $RepositoryRoot 'source/forge/bridgeforge-manifest.json')) | ConvertFrom-Json).shared_skills
$selected = @($catalog.platforms.codex.skills | Where-Object { $_.name -in @('autopilot', 'confirm') })
Assert-True ($selected.Count -eq 2 -and @($selected.name | Select-Object -Unique).Count -eq 2) 'Budget Skills are missing or duplicated in the distribution catalog'
$manifest = @{ platforms = @{ codex = @{ skills = $selected } } } | ConvertTo-Json -Depth 12 | ConvertFrom-Json
$op = [Guid]::NewGuid().ToString('N')
$TestFailAfterSwap = ''
$plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId $op -Commit ('a' * 40))
$log = Join-Path $fixtureProfile '.bridgeforge-codex-shared-update.json'
Invoke-UpdateTransaction -RepositoryRoot $RepositoryRoot -Manifest $manifest -Commit ('a' * 40) -ManifestHash ("sha256:$(Get-Sha256 -Path (Join-Path $RepositoryRoot $ManifestName))") -UserProfile $fixtureProfile -LogPath $log -OperationId $op -PlatformPlans $plans
foreach ($skill in $selected) {
    foreach ($file in $skill.files) {
        $installed = Join-Path (Join-Path (Join-Path $fixtureProfile '.codex/skills') $skill.name) $file.target
        Assert-True ((Get-Sha256 -Path $installed) -eq (Get-Sha256 -Path (Join-Path $RepositoryRoot $file.source))) ('Installed Skill file differs: ' + $skill.name + '/' + $file.target)
    }
}
$repeat = @(New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId ([Guid]::NewGuid().ToString('N')) -Commit ('a' * 40))
Assert-True (@($repeat[0].actions).Count -eq 0) 'Identical autopilot reinstall is not a no-op'
Assert-True (-not (Test-Path -LiteralPath $log)) 'Completed Skill installation left an active transaction'
Write-Output 'autopilot isolated packaging and no-op installation passed'
# A released catalog retires only ledger-owned, byte-identical legacy Skills.
foreach ($scenario in @('retire', 'retire-drift')) {
    $fixtureProfile = Join-Path $Base $scenario
    $skillsRoot = Join-Path $fixtureProfile '.codex/skills'
    $records = [ordered]@{}
    foreach ($name in @('snapshot', 'resume')) {
        $target = Join-Path $skillsRoot $name
        New-Item -ItemType Directory -Path $target -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $target 'SKILL.md'), "legacy $name")
        $records[$name] = @{ source_commit = ('a' * 40); content_hash = (Get-DirectoryContentHash -Root $target); installed_at = '2026-10-08T00:00:00Z' }
    }
    $ledger = Join-Path $fixtureProfile '.codex/bridgeforge-codex-managed.json'
    [IO.File]::WriteAllText($ledger, (@{ schema_version = 1; platform = 'codex'; records = $records } | ConvertTo-Json -Depth 10))
    if ($scenario -eq 'retire-drift') { [IO.File]::WriteAllText((Join-Path $skillsRoot 'resume/SKILL.md'), 'custom content') }
    $selected = @($catalog.platforms.codex.skills | Where-Object { $_.name -eq 'summary' })
    Assert-True ($selected.Count -eq 1) 'summary is absent from catalog'
    Assert-True (@($catalog.platforms.codex.skills | Where-Object { $_.name -in @('snapshot', 'resume') }).Count -eq 0) 'Retired Skill is still distributed'
    $manifest = @{ platforms = @{ codex = @{ skills = $selected } } } | ConvertTo-Json -Depth 12 | ConvertFrom-Json
    $op = [Guid]::NewGuid().ToString('N')
    if ($scenario -eq 'retire-drift') {
        $rejected = $false
        try { $null = New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId $op -Commit ('b' * 40) }
        catch { if ($_.Exception.Message -notlike '*content drifted*') { throw }; $rejected = $true }
        Assert-True $rejected 'Customized retired Skill must block removal'
        Assert-True ([IO.File]::ReadAllText((Join-Path $skillsRoot 'resume/SKILL.md')) -eq 'custom content') 'Customized content changed'
        continue
    }
    $plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $fixtureProfile -OperationId $op -Commit ('b' * 40))
    Assert-True (@($plans[0].actions | Where-Object { $_.kind -eq 'remove' }).Count -eq 2) 'Missing retirement actions'
    $log = Join-Path $fixtureProfile '.bridgeforge-codex-shared-update.json'
    Invoke-UpdateTransaction -RepositoryRoot $RepositoryRoot -Manifest $manifest -Commit ('b' * 40) -ManifestHash ("sha256:$(Get-Sha256 -Path (Join-Path $RepositoryRoot $ManifestName))") -UserProfile $fixtureProfile -LogPath $log -OperationId $op -PlatformPlans $plans
    foreach ($name in @('snapshot', 'resume')) { Assert-True (-not (Test-Path -LiteralPath (Join-Path $skillsRoot $name))) 'Retired Skill remains installed' }
    Assert-True (Test-Path -LiteralPath (Join-Path $skillsRoot 'summary/references/switch-mode.md')) 'Cross-machine handoff reference was not installed'
}
Write-Output 'summary installation and legacy Skill retirement passed'
Write-Output 'confirm weekly quota reference packaging passed'
Write-Output 'shared bundle rollback and deferred committed cleanup passed'
