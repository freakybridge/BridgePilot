param([string]$RepositoryRoot, [string]$Base)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens = $null; $errors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile((Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($f in $ast.FindAll({param($n) $n -is [Management.Automation.Language.FunctionDefinitionAst]}, $true)) {
    . ([scriptblock]::Create($f.Extent.Text))
}
$CanonicalRemote = 'https://github.com/freakybridge/BridgePilot.git'
$CanonicalBranch = 'main'
$ManifestName = 'source/forge/bridgeforge-manifest.json'
$CommandHomeName = '.bridgeforge-codex'
$CommandHomeLogName = '.bridgeforge-codex-home-update.json'
$TestCrashAfterActionCount = 0
$TestFailAfterSwap = ''
$script:CleanupPending = $false
foreach ($scenario in @('success','rollback','edited','unowned','drift','ledger-drift','recovery-edit','fresh-recovery-edit','legacy-journal','ledger-recovery-edit')) {
    $profileRoot = Join-Path $Base $scenario
    $target = Join-Path $profileRoot '.codex/skills/bridgeforge-codex'
    $other = Join-Path $profileRoot '.codex/skills/other/SKILL.md'
    New-Item -ItemType Directory -Path $target,(Split-Path -Parent $other) -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $target 'SKILL.md'), 'legacy discoverable entry')
    [IO.File]::WriteAllText($other, 'preserved unrelated skill')
    $oldHash = Get-DirectoryContentHash -Root $target
    $record = @{source_commit=('a'*40); content_hash=$oldHash; installed_at='historical'}
    $otherRecord = @{source_commit=('a'*40); content_hash=(Get-DirectoryContentHash -Root (Split-Path -Parent $other)); installed_at='historical'}
    $ledgerPath = Join-Path $profileRoot '.codex/bridgeforge-codex-managed.json'
    $records = @{other=$otherRecord}
    if ($scenario -ne 'unowned') { $records['bridgeforge-codex']=$record }
    Write-JsonAtomic -Path $ledgerPath -Value @{schema_version=1; platform='codex'; records=$records}
    $beforeLedger = Get-Sha256 -Path $ledgerPath
    if ($scenario -eq 'fresh-recovery-edit') {
        $resolvedTarget = [IO.Path]::GetFullPath($target)
        if (!$resolvedTarget.StartsWith([IO.Path]::GetFullPath($Base).TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture target escaped test root.' }
        Remove-Item -LiteralPath $resolvedTarget -Recurse -Force
    }
    if ($scenario -eq 'edited') { [IO.File]::WriteAllText((Join-Path $target 'SKILL.md'), 'user edit') }
    $validated = Assert-Manifest -RepositoryRoot $RepositoryRoot -UserProfile $profileRoot
    $manifest = $validated.value
    $manifest.platforms.codex.skills = @($manifest.platforms.codex.skills | Where-Object name -eq 'bridgeforge-codex')
    $op = [guid]::NewGuid().ToString('N')
    $failed = $false
    try {
        $plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $profileRoot -OperationId $op -Commit ('b'*40) -OnlyBundle 'bridgeforge-codex')
        if (@($plans[0].actions | Where-Object name -ne 'bridgeforge-codex').Count) { throw 'Scope expanded.' }
        if ($scenario -eq 'drift') { [IO.File]::WriteAllText((Join-Path $target 'SKILL.md'), 'external edit after plan') }
        if ($scenario -eq 'ledger-drift') {
            $external = Read-JsonFile -Path $ledgerPath
            $external.records.other.installed_at = 'external ledger edit'
            Write-JsonAtomic -Path $ledgerPath -Value $external
            $beforeLedger = Get-Sha256 -Path $ledgerPath
        }
        if ($scenario -in @('recovery-edit','fresh-recovery-edit','legacy-journal','ledger-recovery-edit')) {
            $action = $plans[0].actions[0]
            if (Test-Path -LiteralPath $target) { Move-Item -LiteralPath $target -Destination $action.backup }
            Copy-SkillToStage -RepositoryRoot $RepositoryRoot -Skill $manifest.platforms.codex.skills[0] -Stage $action.stage
            Move-Item -LiteralPath $action.stage -Destination $target
            if ($scenario -in @('recovery-edit','fresh-recovery-edit')) {
                [IO.File]::WriteAllText((Join-Path $target 'MAINTENANCE.md'), 'external recovery edit')
            }
            if ($scenario -eq 'legacy-journal') { $action.Remove('desired_content_hash') }
            if ($scenario -eq 'ledger-recovery-edit') {
                $desired = New-LedgerValue -Platform 'codex' -PlatformManifest $manifest.platforms.codex -Commit ('b'*40) -RetainedRecords $plans[0].retained_records
                Write-JsonAtomic -Path $plans[0].ledger_stage -Value $desired
                $plans[0].ledger_desired_hash = Get-Sha256 -Path $plans[0].ledger_stage
                Move-Item -LiteralPath $ledgerPath -Destination $plans[0].ledger_backup
                Move-Item -LiteralPath $plans[0].ledger_stage -Destination $ledgerPath
                $external = Read-JsonFile -Path $ledgerPath
                $external.records.other.installed_at = 'external recovery ledger'
                Write-JsonAtomic -Path $ledgerPath -Value $external
                $beforeLedger = Get-Sha256 -Path $ledgerPath
            }
            $journal = Join-Path $profileRoot '.bridgeforge-codex-shared-update.json'
            Write-JsonAtomic -Path $journal -Value @{schema_version=1; operation_id=$op; committed=$false; platforms=$plans; bundles=@()}
            Restore-InterruptedOperation -LogPath $journal -UserProfile $profileRoot
            throw 'External recovery edit was accepted.'
        }
        $TestFailAfterSwap = if ($scenario -eq 'rollback') { 'codex:1' } else { '' }
        Invoke-UpdateTransaction -RepositoryRoot $RepositoryRoot -Manifest $manifest -Commit ('b'*40) -ManifestHash $validated.hash -UserProfile $profileRoot -LogPath (Join-Path $profileRoot '.bridgeforge-codex-shared-update.json') -OperationId $op -PlatformPlans $plans
    } catch {
        if ($scenario -eq 'success') { throw }
        $failed = $true
    }
    if ($scenario -eq 'success') {
        if (Test-Path -LiteralPath (Join-Path $target 'SKILL.md')) { throw 'Legacy menu entry survived.' }
        if (!(Test-Path -LiteralPath (Join-Path $target 'MAINTENANCE.md'))) { throw 'Internal workflow missing.' }
        $now = Read-Ledger -Path $ledgerPath -Platform 'codex'
        if ((Get-LedgerRecord -Ledger $now -Name 'other').installed_at -ne 'historical') { throw 'Unrelated ledger record changed.' }
        $again = @(New-UpdatePlan -Manifest $manifest -UserProfile $profileRoot -OperationId ([guid]::NewGuid().ToString('N')) -Commit ('b'*40) -OnlyBundle 'bridgeforge-codex')
        if ($again[0].actions.Count -ne 0 -or $again[0].ledger_needs_update) { throw 'Repeated install is not reusable.' }
    } else {
        if (!$failed) { throw "Expected protection for $scenario" }
        if ($scenario -eq 'rollback' -and (Get-DirectoryContentHash -Root $target) -ne $oldHash) { throw 'Rollback did not restore legacy bytes.' }
        if ((Get-Sha256 -Path $ledgerPath) -ne $beforeLedger) { throw 'Failed installation changed ledger.' }
        if ($scenario -eq 'edited' -and [IO.File]::ReadAllText((Join-Path $target 'SKILL.md')) -ne 'user edit') { throw 'User edits lost.' }
        if ($scenario -eq 'drift' -and [IO.File]::ReadAllText((Join-Path $target 'SKILL.md')) -ne 'external edit after plan') { throw 'External edits lost.' }
        if ($scenario -in @('recovery-edit','fresh-recovery-edit') -and [IO.File]::ReadAllText((Join-Path $target 'MAINTENANCE.md')) -ne 'external recovery edit') { throw 'External recovery edits lost.' }
    }
    if ([IO.File]::ReadAllText($other) -ne 'preserved unrelated skill') { throw 'Other skill changed.' }
}
$handbook = Join-Path $RepositoryRoot 'source/forge/templates/maintenance/bridgeforge-codex/MAINTENANCE.md'
$entry = [IO.File]::ReadAllText($handbook)
if (!$entry.Contains('source_root') -or !$entry.Contains('scripts/bridgeforge_codex_shared_update.ps1')) { throw 'Fresh BP-only bootstrap source is missing.' }
if (Test-Path -LiteralPath (Join-Path $RepositoryRoot 'source/forge/templates/skills/bridgeforge-codex/SKILL.md')) { throw 'Source still exposes the old skill.' }
@{status='passed'; cases=11; migration='internal-maintenance'} | ConvertTo-Json -Compress
