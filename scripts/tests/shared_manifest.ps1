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
$ManifestName = 'bridgeforge-manifest.json'
$CanonicalRemote = 'https://github.com/freakybridge/BridgePilot.git'
$CanonicalBranch = 'main'
$CommandHomeName = '.bridgeforge-codex'
$CommandHomeLogName = '.bridgeforge-codex-home-update.json'
$TestFailAfterSwap = ''
$TestCrashAfterActionCount = 0
$script:CleanupPending = $false
$profileRoot = Join-Path $Base 'profile'
New-Item -ItemType Directory -Path $profileRoot -Force | Out-Null
# The production installer clones with core.autocrlf=false. Model those Git
# bytes instead of the developer checkout's optional CRLF worktree conversion.
$validationRoot = Join-Path $Base 'canonical-source'
New-Item -ItemType Directory -Path $validationRoot -Force | Out-Null
Copy-Item -LiteralPath (Join-Path $RepositoryRoot $ManifestName) -Destination (Join-Path $validationRoot $ManifestName)
$sourceManifest = Read-JsonFile -Path (Join-Path $RepositoryRoot $ManifestName)
foreach ($skill in $sourceManifest.shared_skills.platforms.codex.skills) {
    foreach ($entry in $skill.files) {
        $target = Join-Path $validationRoot $entry.source
        New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($target)) -Force | Out-Null
        $bytes = [IO.File]::ReadAllBytes((Join-Path $RepositoryRoot $entry.source))
        if ($bytes -notcontains 0) {
            $text = [Text.Encoding]::UTF8.GetString($bytes).Replace("`r`n", "`n").Replace("`r", "`n")
            $bytes = [Text.Encoding]::UTF8.GetBytes($text)
        }
        [IO.File]::WriteAllBytes($target, $bytes)
    }
}
$validated = Assert-Manifest -RepositoryRoot $validationRoot -UserProfile $profileRoot -ValidateSources
if ($null -ne $validated.value.PSObject.Properties['project_skeleton'] -or
    $null -ne $validated.value.PSObject.Properties['user_instructions']) { throw 'Shared installer received another scope.' }
# An old installed updater cannot resolve the removed sidecar. Recovery starts
# from install-shared-skills, which dispatches the updater inside its new clone.
$ManifestName = 'bridgeforge-codex-manifest.json'
$blocked = $false
try { Assert-Manifest -RepositoryRoot $RepositoryRoot -UserProfile $profileRoot | Out-Null }
catch { if ($_.Exception.Message -notlike '*Missing root distribution manifest*') { throw }; $blocked = $true }
if (-not $blocked) { throw 'Old sidecar unexpectedly accepted.' }
$ManifestName = 'bridgeforge-manifest.json'
$installer = [IO.File]::ReadAllText((Join-Path $RepositoryRoot 'scripts/install-shared-skills.ps1'))
if (-not $installer.Contains('$updater = Join-Path $cloneRoot') -or
    -not $installer.Contains('-File $updater -SourceRepositoryRoot $cloneRoot')) { throw 'Recovery installer no longer delegates to fresh source.' }

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
foreach ($section in @('user_instructions', 'shared_skills', 'project_skeleton')) {
    foreach ($phase in @('before', 'commit')) {
        $repo = Join-Path $Base "$section-$phase-source"
        $profile = Join-Path $Base "$section-$phase-profile"
        New-Item -ItemType Directory -Path $repo, $profile -Force | Out-Null
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
Write-Output 'unified scopes, old-entry recovery routing and six drift/rollback scenarios passed'

