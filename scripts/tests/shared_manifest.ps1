param([string]$RepositoryRoot, [string]$Base)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
Import-Module (Join-Path $PSHOME 'Modules/Microsoft.PowerShell.Utility/Microsoft.PowerShell.Utility.psd1') -ErrorAction Stop
$tokens = $null; $errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile((Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'), [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($function in $ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst]}, $true)) {
    if ($function.Name -ne 'Invoke-Main') { . ([scriptblock]::Create($function.Extent.Text)) }
}
$ManifestName = 'source/forge/bridgeforge-manifest.json'
$CanonicalRemote = 'https://github.com/freakybridge/BridgePilot.git'
$CanonicalBranch = 'main'
$CommandHomeName = '.bridgeforge-codex'
$CommandHomeLogName = '.bridgeforge-codex-home-update.json'
$TestFailAfterSwap = ''
$TestCrashAfterActionCount = 0
$script:CleanupPending = $false
$profileRoot = Join-Path $Base 'profile'
New-Item -ItemType Directory -Path $profileRoot -Force | Out-Null
$sourceManifest = Read-JsonFile -Path (Join-Path $RepositoryRoot $ManifestName)
if ($sourceManifest.schema_version -ne 2 -or $null -ne $sourceManifest.PSObject.Properties['shared_skills']) { throw 'Expected project-only distribution' }
$entries = @($sourceManifest.project_skeleton.assets | Where-Object { $_.target -like '.agents/skills/*/SKILL.md' })
if ($entries.Count -ne 19) { throw 'Incorrect project Skill count' }
foreach ($entry in $entries) {
    if ($entry.strategy -cne 'whole' -or $entry.id -notlike 'codex.project-workflow.*') { throw 'Project Skill has no explicit ownership' }
    if (-not (Test-Path -LiteralPath (Join-Path $RepositoryRoot $entry.source) -PathType Leaf)) { throw 'Project Skill source missing' }
}
$blocked = $false
try { Assert-Manifest -RepositoryRoot $RepositoryRoot -UserProfile $profileRoot | Out-Null } catch { $blocked = $true }
if (-not $blocked) { throw 'Legacy user distribution accepted the v2 product manifest' }

$script:OriginalAssertRustCliState = (Get-Item Function:Assert-RustCliState).ScriptBlock
function Assert-RustCliState {
    param($Plan, [string]$RepositoryRoot, [switch]$Installed)
    & $script:OriginalAssertRustCliState -Plan $Plan -RepositoryRoot $RepositoryRoot -Installed:$Installed
    if ($script:InjectAtCommit) {
        $path = Join-Path $RepositoryRoot $ManifestName
        $value = Read-JsonFile -Path $path
        $value.($script:DriftSection) | Add-Member -NotePropertyName fixture_drift -NotePropertyValue $true -Force
        [IO.File]::WriteAllText($path, ($value | ConvertTo-Json -Depth 100), (New-Object Text.UTF8Encoding($false)))
    }
}
foreach ($section in @('user_instructions', 'project_skeleton')) {
    foreach ($phase in @('before', 'commit')) {
        $repo = Join-Path $Base "$section-$phase-source"
        $profile = Join-Path $Base "$section-$phase-profile"
        New-Item -ItemType Directory -Path $repo, $profile -Force | Out-Null
        New-Item -ItemType Directory -Path (Split-Path -Parent (Join-Path $repo $ManifestName)) -Force | Out-Null
        Copy-Item -LiteralPath (Join-Path $RepositoryRoot $ManifestName) -Destination (Join-Path $repo $ManifestName)
        $expected = "sha256:$(Get-Sha256 -Path (Join-Path $repo $ManifestName))"
        $manifest = @{ platforms = @{ codex = @{ skills = @() } } } | ConvertTo-Json -Depth 8 | ConvertFrom-Json
        $op = [Guid]::NewGuid().ToString('N')
        $plans = @(New-UpdatePlan -Manifest $manifest -UserProfile $profile -OperationId $op -Commit ('a' * 40))
        $log = Join-Path $profile '.bridgeforge-codex-shared-update.json'
        $script:DriftSection = $section
        $script:InjectAtCommit = $phase -eq 'commit'
        if ($phase -eq 'before') {
            $value = Read-JsonFile -Path (Join-Path $repo $ManifestName)
            $value.$section | Add-Member -NotePropertyName fixture_drift -NotePropertyValue $true
            [IO.File]::WriteAllText((Join-Path $repo $ManifestName), ($value | ConvertTo-Json -Depth 100), (New-Object Text.UTF8Encoding($false)))
        }
        $rejected = $false
        try {
            Invoke-UpdateTransaction -RepositoryRoot $repo -Manifest $manifest -Commit ('a' * 40) -ManifestHash $expected -UserProfile $profile -LogPath $log -OperationId $op -PlatformPlans $plans
        } catch {
            if ($_.Exception.Message -notlike '*Product manifest changed after planning*') { throw }
            $rejected = $true
        }
        if (-not $rejected) { throw "Manifest drift was accepted: $section/$phase" }
        if (Test-Path -LiteralPath $log) { throw 'Failed transaction left its journal.' }
        if (Test-Path -LiteralPath (Join-Path $profile '.codex/bridgeforge-codex-managed.json')) { throw 'Failed transaction committed a ledger.' }
        if ((Get-Sha256 -Path (Join-Path $repo $ManifestName)) -eq $expected.Substring(7)) { throw 'External manifest change was overwritten.' }
    }
}
Write-Output 'project-only manifest and four drift/rollback scenarios passed'

