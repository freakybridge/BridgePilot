#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$InstallationHome,
    [string]$SkillsRoot,
    [string]$SourceRoot,
    [Parameter(DontShow = $true)][switch]$FunctionsOnly
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0
$script:Repository = 'https://github.com/freakybridge/BridgePilot.git'
$script:UpdateStage = 'preflight'
$script:RuntimeInstalled = $false
$script:SkillsInstalled = $false
$script:RecoveryCore = $null

function Get-AbsolutePath([string]$Path) {
    if ($Path.StartsWith('\\?\UNC\', [StringComparison]::OrdinalIgnoreCase)) {
        $Path = '\\' + $Path.Substring(8)
    } elseif ($Path.StartsWith('\\?\')) {
        $Path = $Path.Substring(4)
    }
    if ($Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)') { throw 'An absolute Windows path is required.' }
    $full = [IO.Path]::GetFullPath($Path)
    if ($full.TrimEnd('\') -eq [IO.Path]::GetPathRoot($full).TrimEnd('\')) { throw 'A volume root cannot be an installation directory.' }
    return $full.TrimEnd('\')
}

function Assert-OrdinaryPath([string]$Path) {
    $current = [IO.Path]::GetFullPath($Path)
    while ($current) {
        if (Test-Path -LiteralPath $current) {
            $item = Get-Item -LiteralPath $current -Force
            if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw "Linked path is not supported: $current"
            }
        }
        $parent = Split-Path -Parent $current
        if ($parent -eq $current) { break }
        $current = $parent
    }
}

function Invoke-Checked([string]$Program, [string[]]$Arguments) {
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        $output = & $Program @Arguments 2>&1
        $code = $LASTEXITCODE
    } finally { $ErrorActionPreference = $previous }
    if ($code -ne 0) {
        throw "$Program failed ($code): $($output -join [Environment]::NewLine)"
    }
    return ($output -join [Environment]::NewLine)
}

function Invoke-Bp([string]$Program, [string[]]$Arguments) {
    $result = Invoke-Checked $Program $Arguments | ConvertFrom-Json
    if ($result.PSObject.Properties['status'] -and $result.status -eq 'blocked') { throw 'BP rejected the requested operation.' }
    return $result
}

function Read-Json([string]$Path) {
    Assert-OrdinaryPath $Path
    return Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
}

function Assert-Source([string]$Root) {
    Assert-OrdinaryPath $Root
    if (!(Test-Path -LiteralPath (Join-Path $Root '.git') -PathType Container)) {
        throw 'The source directory is not an owned Git checkout; preserve it.'
    }
    $actual = (Invoke-Checked git @('-C', $Root, 'rev-parse', '--show-toplevel')).Trim()
    if ([IO.Path]::GetFullPath($actual).TrimEnd('\') -ne [IO.Path]::GetFullPath($Root).TrimEnd('\')) {
        throw 'The source directory is not the Git root.'
    }
    $remote = (Invoke-Checked git @('-C', $Root, 'config', '--get-all', 'remote.origin.url')).Trim()
    if ($remote -cne $script:Repository) { throw 'Source origin differs from the official BP repository.' }
    $branch = (Invoke-Checked git @('-C', $Root, 'branch', '--show-current')).Trim()
    if ($branch -cne 'main') { throw 'Source checkout must remain on main.' }
    $dirty = Invoke-Checked git @('-C', $Root, 'status', '--porcelain', '--untracked-files=all')
    if ($dirty.Trim()) { throw 'Source checkout has local changes; preserve them before updating.' }
}

function Update-Source([string]$Root) {
    Assert-OrdinaryPath $Root
    if (!(Test-Path -LiteralPath $Root)) {
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $Root) -Force
        $null = Invoke-Checked git @('clone', '--config', 'core.longpaths=true', '--branch', 'main', '--single-branch', '--', $script:Repository, $Root)
    }
    Assert-Source $Root
    # Fetch only main; do not follow a caller's branch/refspec configuration.
    $null = Invoke-Checked git @('-C', $Root, 'fetch', '--no-tags', 'origin', '+refs/heads/main:refs/remotes/origin/main')
    $null = Invoke-Checked git @('-C', $Root, 'merge-base', '--is-ancestor', 'HEAD', 'refs/remotes/origin/main')
    $null = Invoke-Checked git @('-C', $Root, 'merge', '--ff-only', 'refs/remotes/origin/main')
    Assert-Source $Root
    return (Invoke-Checked git @('-C', $Root, 'rev-parse', 'HEAD')).Trim()
}

function Assert-NotPending([string]$InstalledHome) {
    foreach ($relative in @('.runtime/bridgepilot-install/journal.json', '.runtime/bridgepilot-skill/journal.json')) {
        if (Test-Path -LiteralPath (Join-Path $InstalledHome $relative)) {
            throw "Pending BP transaction: $relative. Use its install-recover or skill-recover entry first."
        }
    }
}

function Get-UpdateDigest([string]$Path) {
    Assert-OrdinaryPath $Path
    $algorithm = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($Path)
    try { return ([BitConverter]::ToString($algorithm.ComputeHash($stream))).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $algorithm.Dispose() }
}

function Test-CurrentInstallation($Receipt, [string]$Commit, [string]$InstalledHome, [string]$Skills, $Runtime, $Skill) {
    if (!$Receipt -or !$Receipt.PSObject.Properties['installation'] -or $Receipt.status -ne 'source_update_completed' -or
        $Receipt.source_commit -cne $Commit -or $Receipt.home -cne $InstalledHome -or $Receipt.skills_root -cne $Skills -or !$Skill.current) { return $false }
    if (($Receipt.installation | ConvertTo-Json -Depth 20 -Compress) -cne ($Runtime.installation | ConvertTo-Json -Depth 20 -Compress)) { return $false }
    foreach ($file in $Runtime.installation.files) {
        $path = Get-AbsolutePath (Join-Path $InstalledHome $file.target)
        if (!$path.StartsWith($InstalledHome + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Installed asset escapes home.' }
        if (!(Test-Path -LiteralPath $path -PathType Leaf) -or (Get-UpdateDigest $path) -cne $file.sha256) { return $false }
    }
    return $true
}

function Test-RuntimeReused($Runtime) {
    return $Runtime.PSObject.Properties['mode'] -and $Runtime.mode -eq 'reused' -and
        $Runtime.PSObject.Properties['writes'] -and !$Runtime.writes
}

function Invoke-SourceUpdate {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) { throw 'BP source installation supports Windows only.' }
    $codexRoot = if ($env:CODEX_HOME) { $env:CODEX_HOME } else { Join-Path $env:USERPROFILE '.codex' }
    $bindingPath = Join-Path (Split-Path -Parent $PSScriptRoot) 'bridgepilot-local.json'
    if (Test-Path -LiteralPath $bindingPath) {
        $binding = Read-Json $bindingPath
        if ($binding.schema -ne 1 -or $binding.product -ne 'BridgePilot') { throw 'Invalid BP local binding.' }
        if ($InstallationHome -and (Get-AbsolutePath $InstallationHome) -ne (Get-AbsolutePath $binding.home)) {
            throw 'Requested home differs from the installed Skill binding.'
        }
        if ($SkillsRoot -and (Get-AbsolutePath $SkillsRoot) -ne (Get-AbsolutePath $binding.skills_root)) {
            throw 'Requested skills root differs from the installed Skill binding.'
        }
        $InstallationHome = $binding.home
        $SkillsRoot = $binding.skills_root
    }
    if (!$InstallationHome) { $InstallationHome = Join-Path ([Environment]::GetFolderPath('MyDocuments')) 'Codex/BridgePilot' }
    if (!$SkillsRoot) { $SkillsRoot = Join-Path $codexRoot 'skills' }
    if (!$SourceRoot) { $SourceRoot = Join-Path $env:USERPROFILE '.bridgepilot-source' }
    $InstallationHome = Get-AbsolutePath $InstallationHome
    $SkillsRoot = Get-AbsolutePath $SkillsRoot
    $SourceRoot = Get-AbsolutePath $SourceRoot
    foreach ($path in @($InstallationHome, $SkillsRoot, $SourceRoot)) { Assert-OrdinaryPath $path }
    for ($i = 0; $i -lt 3; $i++) {
        for ($j = $i + 1; $j -lt 3; $j++) {
            $paths = @($InstallationHome, $SkillsRoot, $SourceRoot)
            if ($paths[$i] -eq $paths[$j] -or $paths[$i].StartsWith($paths[$j] + '\', [StringComparison]::OrdinalIgnoreCase) -or $paths[$j].StartsWith($paths[$i] + '\', [StringComparison]::OrdinalIgnoreCase)) {
                throw 'Installation home, skills root and source must be separate directories.'
            }
        }
    }
    Assert-NotPending $InstallationHome
    foreach ($program in @('git', 'cargo', 'rustc')) { $null = Get-Command $program -ErrorAction Stop }
    $launcher = Join-Path $InstallationHome '.build/bridgepilot/bin/bp.exe'
    $installation = Join-Path $InstallationHome '.runtime/config/bridgepilot-installation.json'
    if (Test-Path -LiteralPath $installation) {
        $existingRuntime = Invoke-Bp $launcher @('install-status', '--home', $InstallationHome)
        # The old core validates its own complete ownership registry before any update.
        $existingSkill = Invoke-Bp $launcher @('skill-status', '--home', $InstallationHome, '--skills-root', $SkillsRoot)
    } elseif (Test-Path -LiteralPath $InstallationHome) {
        if (@(Get-ChildItem -LiteralPath $InstallationHome -Force).Count) { throw 'Unowned nonempty BP home is preserved.' }
    }
    $lockRoot = Join-Path (Split-Path -Parent $SourceRoot) '.runtime/bridgepilot-source-update'
    Assert-OrdinaryPath $lockRoot
    $null = New-Item -ItemType Directory -Path $lockRoot -Force
    $lock = [IO.File]::Open((Join-Path $lockRoot 'writer.lock'), [IO.FileMode]::OpenOrCreate, [IO.FileAccess]::ReadWrite, [IO.FileShare]::None)
    try {
        $script:UpdateStage = 'source'
        $commit = Update-Source $SourceRoot
        $receiptPath = Join-Path $SourceRoot '.runtime/bridgepilot-source-update/receipt.json'
        if ((Test-Path -LiteralPath $installation) -and (Test-Path -LiteralPath $receiptPath)) {
            $savedReceipt = Read-Json $receiptPath
            if (Test-CurrentInstallation $savedReceipt $commit $InstallationHome $SkillsRoot $existingRuntime $existingSkill) {
                Assert-NotPending $InstallationHome
                $verifiedRuntime = Invoke-Bp $launcher @('install-status', '--home', $InstallationHome)
                $verifiedSkill = Invoke-Bp $launcher @('skill-status', '--home', $InstallationHome, '--skills-root', $SkillsRoot)
                if (!(Test-CurrentInstallation $savedReceipt $commit $InstallationHome $SkillsRoot $verifiedRuntime $verifiedSkill)) { throw 'Installation changed during reuse validation.' }
                $savedReceipt | Add-Member -NotePropertyName mode -NotePropertyValue 'reused' -Force
                $savedReceipt | Add-Member -NotePropertyName build_invocations -NotePropertyValue 0 -Force
                Write-Output ('BRIDGEPILOT_SOURCE_UPDATE_RECEIPT ' + ($savedReceipt | ConvertTo-Json -Depth 20 -Compress))
                return
            }
        }
        $target = Join-Path $SourceRoot '.build/bridgepilot/cargo'
        $manifest = Join-Path $SourceRoot 'Cargo.toml'
        foreach ($relative in @('Cargo.toml', 'Cargo.lock', 'INSTALL.md', 'source/orchestrator/cli/Cargo.toml')) {
            Assert-OrdinaryPath (Join-Path $SourceRoot $relative)
            if (!(Test-Path -LiteralPath (Join-Path $SourceRoot $relative) -PathType Leaf)) { throw "BP source lacks $relative" }
        }
        $script:UpdateStage = 'build'
        Push-Location -LiteralPath $SourceRoot
        try {
            $null = Invoke-Checked cargo @('build', '--locked', '--release', '--manifest-path', $manifest, '--target-dir', $target, '-p', 'bridgepilot-orchestrator', '--bin', 'bp', '--bin', 'bp-launcher')
        } finally { Pop-Location }
        Assert-Source $SourceRoot
        if ((Invoke-Checked git @('-C', $SourceRoot, 'rev-parse', 'HEAD')).Trim() -cne $commit) { throw 'Source commit changed during build.' }
        $core = Join-Path $target 'release/bp.exe'
        $script:RecoveryCore = $core
        $builtLauncher = Join-Path $target 'release/bp-launcher.exe'
        $identity = Invoke-Bp $core @('self-test')
        if ($identity.program -ne 'bridgepilot' -or 'bp-source-update-v1' -notin $identity.capabilities) { throw 'The selected main commit does not support BP source updates.' }
        $bundle = Join-Path $SourceRoot ('.build/bridgepilot/bundles/' + $commit + '-' + [guid]::NewGuid().ToString('N'))
        $script:UpdateStage = 'bundle'
        $null = Invoke-Bp $core @('bundle', '--output', $bundle, '--core', $core, '--launcher', $builtLauncher)
        Assert-NotPending $InstallationHome
        $script:UpdateStage = 'runtime-install'
        $null = Invoke-Bp $core @('install-preview', '--bundle', $bundle, '--home', $InstallationHome)
        $runtime = Invoke-Bp $core @('install', '--bundle', $bundle, '--home', $InstallationHome)
        $runtimeReused = Test-RuntimeReused $runtime
        $script:RuntimeInstalled = !$runtimeReused
        $script:UpdateStage = 'skill-install'
        $null = Invoke-Bp $launcher @('skill-install-preview', '--home', $InstallationHome, '--skills-root', $SkillsRoot)
        $skill = Invoke-Bp $launcher @('skill-install', '--home', $InstallationHome, '--skills-root', $SkillsRoot)
        $script:SkillsInstalled = $true
        $script:UpdateStage = 'verification'
        $verifiedRuntime = Invoke-Bp $launcher @('install-status', '--home', $InstallationHome)
        $final = Invoke-Bp $launcher @('skill-status', '--home', $InstallationHome, '--skills-root', $SkillsRoot)
        if (!$final.current) { throw 'BP Skill verification did not report current.' }
        $receipt = [ordered]@{ schema = 1; status = 'source_update_completed'; repository = $script:Repository; branch = 'main'; source_commit = $commit; source_root = $SourceRoot; home = $InstallationHome; skills_root = $SkillsRoot; version = $identity.version; runtime = $runtime; skill = $skill; tasks_resumed = $false }
        $receipt['installation'] = $verifiedRuntime.installation
        $receipt['mode'] = if ($runtimeReused) { 'reused_after_build' } else { 'updated' }
        $receipt['build_invocations'] = 1
        if ($identity.PSObject.Properties['components']) { $receipt['components'] = $identity.components }
        $receiptPath = Join-Path $SourceRoot '.runtime/bridgepilot-source-update/receipt.json'
        Assert-OrdinaryPath $receiptPath
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $receiptPath) -Force
        [IO.File]::WriteAllText($receiptPath, ($receipt | ConvertTo-Json -Depth 20), (New-Object Text.UTF8Encoding($false)))
        Write-Output ('BRIDGEPILOT_SOURCE_UPDATE_RECEIPT ' + ($receipt | ConvertTo-Json -Depth 20 -Compress))
    } finally { $lock.Dispose() }
}

if (!$FunctionsOnly) {
    try { Invoke-SourceUpdate } catch {
        Write-Output ('BRIDGEPILOT_SOURCE_UPDATE_RECEIPT ' + (@{ schema = 1; status = 'failed'; stage = $script:UpdateStage; runtime_installed = $script:RuntimeInstalled; skills_installed = $script:SkillsInstalled; error = $_.Exception.Message; recovery_core = $script:RecoveryCore; recovery = 'Preserve source and installation. For a pending rename journal use this fixed candidate core install-recover or skill-recover; old launchers may not read the new journal. Do not delete pending records.' } | ConvertTo-Json -Compress))
        exit 1
    }
}
