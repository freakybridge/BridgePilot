#requires -Version 5.1
[CmdletBinding()]
param([switch]$FullLifecycle)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$entry = Join-Path $repoRoot 'source/orchestrator/skills/bridgepilot/scripts/bridgepilot_update.ps1'
$tokens = $null
$errors = $null
$null = [Management.Automation.Language.Parser]::ParseFile($entry, [ref]$tokens, [ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
. $entry -FunctionsOnly
$root = Join-Path $repoRoot ('.runtime/tests/bpu/' + [guid]::NewGuid().ToString('N').Substring(0, 8))
$remote = Join-Path $root 'upstream'
$source = Join-Path $root 'source'
$null = New-Item -ItemType Directory -Path $root -Force
$checks = 0
function Get-TestDigest([string]$Path) {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($Path)
    try { return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '') }
    finally { $stream.Dispose(); $algorithm.Dispose() }
}
function Expect-Rejection([scriptblock]$Action, [string]$Pattern) {
    try { & $Action } catch {
        if ($_.Exception.Message -notlike "*$Pattern*") { throw }
        $script:checks++
        return
    }
    throw "Expected rejection: $Pattern"
}
$null = Invoke-Checked git @('init', '--initial-branch=main', $remote)
$null = Invoke-Checked git @('-C', $remote, 'config', 'user.email', 'fixture@example.invalid')
$null = Invoke-Checked git @('-C', $remote, 'config', 'user.name', 'BP fixture')
[IO.File]::WriteAllText((Join-Path $remote 'tracked.txt'), 'initial')
$null = Invoke-Checked git @('-C', $remote, 'add', 'tracked.txt')
$null = Invoke-Checked git @('-C', $remote, 'commit', '-m', 'fixture initial')
$oldCount = $env:GIT_CONFIG_COUNT
$oldKey = $env:GIT_CONFIG_KEY_0
$oldValue = $env:GIT_CONFIG_VALUE_0
try {
    # Redirect the canonical URL only in this test process; production identity
    # checks must still see the exact official origin stored by clone.
    $env:GIT_CONFIG_COUNT = '1'
    $env:GIT_CONFIG_KEY_0 = 'url.' + $remote.Replace('\', '/') + '.insteadOf'
    $env:GIT_CONFIG_VALUE_0 = $script:Repository
    $first = Update-Source $source
    if (!$first -or (Update-Source $source) -cne $first) { throw 'Idempotent update changed HEAD.' }
    $checks += 2
    [IO.File]::WriteAllText((Join-Path $remote 'tracked.txt'), 'next')
    $null = Invoke-Checked git @('-C', $remote, 'commit', '-am', 'fixture next')
    $next = Update-Source $source
    if ($next -ceq $first -or [IO.File]::ReadAllText((Join-Path $source 'tracked.txt')) -ne 'next') { throw 'Fast-forward update failed.' }
    $checks++
    [IO.File]::WriteAllText((Join-Path $source 'tracked.txt'), 'user edit')
    Expect-Rejection { Update-Source $source } 'local changes'
    if ([IO.File]::ReadAllText((Join-Path $source 'tracked.txt')) -ne 'user edit') { throw 'User edit was overwritten.' }
    [IO.File]::WriteAllText((Join-Path $source 'tracked.txt'), 'next')
    $null = Invoke-Checked git @('-C', $source, 'remote', 'set-url', 'origin', 'https://example.invalid/other.git')
    Expect-Rejection { Update-Source $source } 'origin differs'
    $null = Invoke-Checked git @('-C', $source, 'remote', 'set-url', 'origin', $script:Repository)
    $null = Invoke-Checked git @('-C', $source, 'switch', '-c', 'local-work')
    Expect-Rejection { Update-Source $source } 'remain on main'
    $null = Invoke-Checked git @('-C', $source, 'switch', 'main')
    $null = Invoke-Checked git @('-C', $source, 'config', 'user.email', 'fixture@example.invalid')
    $null = Invoke-Checked git @('-C', $source, 'config', 'user.name', 'BP fixture')
    [IO.File]::WriteAllText((Join-Path $source 'tracked.txt'), 'local commit')
    $null = Invoke-Checked git @('-C', $source, 'commit', '-am', 'local fixture')
    $local = (Invoke-Checked git @('-C', $source, 'rev-parse', 'HEAD')).Trim()
    Expect-Rejection { Update-Source $source } 'failed'
    if ((Invoke-Checked git @('-C', $source, 'rev-parse', 'HEAD')).Trim() -cne $local) { throw 'Local commit was reset.' }
    $unknown = Join-Path $root 'unknown'
    $null = New-Item -ItemType Directory -Path $unknown
    [IO.File]::WriteAllText((Join-Path $unknown 'keep.txt'), 'preserve')
    Expect-Rejection { Update-Source $unknown } 'not an owned Git checkout'
    foreach ($journal in @('.runtime/bridgepilot-install/journal.json', '.runtime/bridgepilot-skill/journal.json')) {
        $pendingHome = Join-Path $root ([guid]::NewGuid().ToString('N'))
        $pending = Join-Path $pendingHome $journal
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $pending) -Force
        [IO.File]::WriteAllText($pending, 'pending')
        Expect-Rejection { Assert-NotPending $pendingHome } 'Pending BP transaction'
    }
    if ($FullLifecycle) {
        $product = Join-Path $root 'product'
        $null = Invoke-Checked git @('clone', '--config', 'core.longpaths=true', '--no-hardlinks', '--', $repoRoot, $product)
        $changed = Invoke-Checked git @('-C', $repoRoot, 'diff', '--name-only', 'HEAD')
        $added = Invoke-Checked git @('-C', $repoRoot, 'ls-files', '--others', '--exclude-standard')
        foreach ($relative in (($changed + "`n" + $added) -split '\r?\n' | Where-Object { $_ })) {
            $destination = Join-Path $product $relative
            $null = New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force
            [IO.File]::Copy((Join-Path $repoRoot $relative), $destination, $true)
        }
        $null = Invoke-Checked git @('-C', $product, 'config', 'user.email', 'fixture@example.invalid')
        $null = Invoke-Checked git @('-C', $product, 'config', 'user.name', 'BP fixture')
        $null = Invoke-Checked git @('-C', $product, 'add', '--all')
        $null = Invoke-Checked git @('-C', $product, 'commit', '-m', 'isolated source updater candidate')
        $env:GIT_CONFIG_KEY_0 = 'url.' + $product.Replace('\', '/') + '.insteadOf'
        $managedSource = Join-Path $root 'managed-source'
        $testHome = Join-Path $root 'installed'
        $testSkills = Join-Path $root 'codex/skills'
        $null = New-Item -ItemType Directory -Path (Split-Path -Parent $testSkills) -Force
        foreach ($attempt in 1..2) {
            $call = if ($attempt -eq 1) {
                @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $entry, '-InstallationHome', $testHome, '-SkillsRoot', $testSkills, '-SourceRoot', $managedSource)
            } else {
                @('-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $testSkills 'bridgepilot/scripts/bridgepilot_update.ps1'), '-SourceRoot', $managedSource)
            }
            $receipt = Invoke-Checked powershell $call
            if ($receipt -notlike '*source_update_completed*') { throw 'Full source installation lacks success receipt.' }
            $configPath = Join-Path $testHome '.runtime/config/bridgepilot.json'
            if ($attempt -eq 1) {
                if (Test-Path -LiteralPath $configPath) { throw 'Source installation created a private archive.' }
                $testWorkspace = Join-Path $root 'private-workspace'
                $testLauncher = Join-Path $testHome '.build/bridgepilot/bin/bp.exe'
                $null = Invoke-Bp $testLauncher @('init', '--workspace', $testWorkspace)
                $null = Invoke-Bp $testLauncher @('configure', '--product-root', $testHome, '--workspace', $testWorkspace)
                $configBefore = [IO.File]::ReadAllText($configPath)
                $archivePath = Join-Path $testWorkspace 'doc/5_project_knowledgebase/bridgepilot/bridgepilot.sqlite3'
                $archiveBefore = Get-TestDigest $archivePath
            } else {
                if ([IO.File]::ReadAllText($configPath) -cne $configBefore -or (Get-TestDigest $archivePath) -cne $archiveBefore) { throw 'Repeated source update modified private configuration or archive.' }
            }
            $checks++
        }
        $installedCore = Join-Path $testHome '.build/bridgepilot/bin/bp.exe'
        $identity = Invoke-Bp $installedCore @('self-test')
        if ('bp-source-update-v1' -notin $identity.capabilities) { throw 'Installed source updater capability is missing.' }
        $checks++
    }
    Write-Output ("BP_SOURCE_UPDATE_TESTS passed=$checks")
} finally {
    $env:GIT_CONFIG_COUNT = $oldCount
    $env:GIT_CONFIG_KEY_0 = $oldKey
    $env:GIT_CONFIG_VALUE_0 = $oldValue
}
