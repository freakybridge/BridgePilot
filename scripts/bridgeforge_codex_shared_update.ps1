#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(DontShow = $true)]
    [string]$ResumeSourceCommit,

    [Parameter(DontShow = $true)]
    [string]$SourceRepositoryRoot,

    [Parameter(DontShow = $true)]
    [switch]$InternalMaintenanceOnly,

    [switch]$MigrateUserScope,
    [string]$PreparedSourceCommit,
    [string[]]$MigratedProjectRoot,
    [switch]$NoDependentProjects,
    [switch]$InstallMemoryRuntime,

    [Parameter(DontShow = $true)]
    [switch]$TestNonWindows,

    [Parameter(DontShow = $true)]
    [int]$TestCrashAfterActionCount = 0,

    [Parameter(DontShow = $true)]
    [string]$TestFailAfterSwap,

    [Parameter(DontShow = $true)]
    [int]$TestHoldLockMilliseconds = 0
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version 2.0

$CanonicalRemote = "https://github.com/freakybridge/BridgePilot.git"
$CanonicalBranch = "main"
$ManifestName = "source/forge/bridgeforge-manifest.json"
$OperationLogName = ".bridgeforge-codex-shared-update.json"
$CommandHomeName = ".bridgeforge-codex"
$CommandHomeLogName = ".bridgeforge-codex-home-update.json"
$script:PhaseTimings = [ordered]@{}
$script:CleanupPending = $false
$script:UserAgentsReceipt = $null
$script:PublicArtifactsReceipt = $null
$script:MemoryMigrationReceipt = $null
$script:MemoryRuntimePath = $null
$script:UpdateTimer = [Diagnostics.Stopwatch]::StartNew()
$script:BuildCounts = [ordered]@{ cargo_build_invocations = 0; installed_cli_reused = 0; public_cli_reused = 0; source_clones = 0; source_reused = 0 }

function Complete-PhaseTiming {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][Diagnostics.Stopwatch]$Stopwatch
    )
    $Stopwatch.Stop()
    $script:PhaseTimings[$Name] = [math]::Round($Stopwatch.Elapsed.TotalMilliseconds, 1)
}

function Write-UpdateReceipt {
    param(
        [Parameter(Mandatory = $true)][string]$Status,
        [string]$Commit,
        [string]$Mode,
        [int]$ActionCount = 0,
        [string]$ErrorMessage
    )
    if ($script:UpdateTimer.IsRunning) {
        $script:UpdateTimer.Stop()
    }
    $script:PhaseTimings["total"] = [math]::Round($script:UpdateTimer.Elapsed.TotalMilliseconds, 1)
    $receipt = [ordered]@{
        status = $Status
        source_commit = if ([string]::IsNullOrWhiteSpace($Commit)) { $null } else { $Commit }
        mode = if ([string]::IsNullOrWhiteSpace($Mode)) { $null } else { $Mode }
        action_count = $ActionCount
        timings_ms = $script:PhaseTimings
        timing_semantics = 'component_preparation includes rust_cli_*; total includes all phases'
        build_counts = $script:BuildCounts
        cleanup_pending = $script:CleanupPending
        user_agents = $script:UserAgentsReceipt
        public_artifacts = $script:PublicArtifactsReceipt
    }
    if (-not [string]::IsNullOrWhiteSpace($ErrorMessage)) {
        $receipt["error"] = $ErrorMessage
    }
    Write-Host "BRIDGEFORGE_CODEX_SHARED_UPDATE_RECEIPT $($receipt | ConvertTo-Json -Depth 6 -Compress)"
}

function Assert-Windows {
    if ($TestNonWindows -or [Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "bridgeforge-codex shared-skill distribution supports Windows only."
    }
}

function Invoke-Git {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$WorkingDirectory
    )
    $oldLocation = Get-Location
    try {
        if ($WorkingDirectory) {
            Set-Location -LiteralPath $WorkingDirectory
        }
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = "Continue"
        try {
            $output = & git @Arguments 2>&1
            $exitCode = $LASTEXITCODE
        }
        finally {
            $ErrorActionPreference = $previousPreference
        }
        if ($exitCode -ne 0) {
            throw "git $($Arguments -join ' ') failed: $($output -join [Environment]::NewLine)"
        }
        return @($output)
    }
    finally {
        Set-Location -LiteralPath $oldLocation
    }
}

function Get-NormalizedRemote {
    param([Parameter(Mandatory = $true)][string]$Remote)
    $value = $Remote.Trim().TrimEnd("/")
    if ($value.EndsWith(".git", [StringComparison]::OrdinalIgnoreCase)) {
        $value = $value.Substring(0, $value.Length - 4)
    }
    return $value.ToLowerInvariant()
}

function Test-SafeRelativePath {
    param([Parameter(Mandatory = $true)][string]$Path)
    if ([string]::IsNullOrWhiteSpace($Path) -or [IO.Path]::IsPathRooted($Path)) {
        return $false
    }
    $parts = $Path.Replace("\", "/").Split("/")
    if ($parts.Count -eq 0) {
        return $false
    }
    foreach ($part in $parts) {
        if ([string]::IsNullOrWhiteSpace($part) -or $part -eq "." -or $part -eq "..") {
            return $false
        }
    }
    return $true
}

function Get-PathUnderRoot {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$RelativePath
    )
    if (-not (Test-SafeRelativePath -Path $RelativePath)) {
        throw "Unsafe relative path in manifest: $RelativePath"
    }
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd("\", "/")
    $candidate = [IO.Path]::GetFullPath((Join-Path $rootFull $RelativePath))
    $prefix = $rootFull + [IO.Path]::DirectorySeparatorChar
    if (-not $candidate.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Manifest path escapes source root: $RelativePath"
    }
    return $candidate
}

function Get-Sha256 {
    param([Parameter(Mandatory = $true)][string]$Path)
    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Get-TextSha256 {
    param([Parameter(Mandatory = $true)][string]$Text)
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [Text.Encoding]::UTF8.GetBytes($Text)
        return ([BitConverter]::ToString($sha.ComputeHash($bytes))).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha.Dispose()
    }
}

function Get-SkillContentHash {
    param([Parameter(Mandatory = $true)]$Skill)
    $files = @($Skill.files)
    [Array]::Sort(
        $files,
        [Collections.Generic.Comparer[object]]::Create(
            [Comparison[object]]{
                param($left, $right)
                return [StringComparer]::Ordinal.Compare(
                    ([string]$left.target).Replace("\", "/"),
                    ([string]$right.target).Replace("\", "/")
                )
            }
        )
    )
    $lines = @()
    foreach ($file in $files) {
        $hash = ([string]$file.sha256).ToLowerInvariant()
        if ($hash.StartsWith("sha256:")) {
            $hash = $hash.Substring(7)
        }
        $target = ([string]$file.target).Replace("\", "/")
        $lines += "$target`n$hash"
    }
    return "sha256:$(Get-TextSha256 -Text (($lines -join "`n") + "`n"))"
}

function Get-DirectoryContentHash {
    param([Parameter(Mandatory = $true)][string]$Root)
    if (-not (Test-Path -LiteralPath $Root -PathType Container) -or (Test-ReparsePoint -Path $Root)) {
        throw "Managed skill directory is missing or is a reparse point: $Root"
    }
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd("\", "/")
    $lines = @()
    foreach ($file in @(Get-ChildItem -LiteralPath $rootFull -File -Recurse -Force)) {
        if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "Managed skill file may not be a reparse point: $($file.FullName)"
        }
        $relative = $file.FullName.Substring($rootFull.Length + 1).Replace("\", "/")
        $lines += "$relative`n$(Get-Sha256 -Path $file.FullName)"
    }
    [Array]::Sort($lines, [StringComparer]::Ordinal)
    return "sha256:$(Get-TextSha256 -Text (($lines -join "`n") + "`n"))"
}

function Assert-DirectoryContentHash {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Expected,
        [Parameter(Mandatory = $true)][string]$Context
    )
    if ((Get-DirectoryContentHash -Root $Path) -ne $Expected.ToLowerInvariant()) {
        throw "$Context content verification failed: $Path"
    }
}

function Get-RustWorkspaceContentHash {
    param([Parameter(Mandatory = $true)][string]$RepositoryRoot)
    $records = @()
    foreach ($relative in @('Cargo.toml', 'Cargo.lock')) {
        $file = Join-Path $RepositoryRoot $relative
        if (Test-ReparsePoint -Path $file) { throw "Managed workspace input is linked: $relative" }
        $records += "$relative`n$(Get-Sha256 -Path $file)"
    }
    $sourceRoot = Join-Path $RepositoryRoot 'source'
    $pendingDirectories = New-Object 'Collections.Generic.Stack[string]'
    $pendingDirectories.Push($sourceRoot)
    while ($pendingDirectories.Count -gt 0) {
        $directory = $pendingDirectories.Pop()
        if (-not (Test-Path -LiteralPath $directory -PathType Container) -or (Test-ReparsePoint -Path $directory)) {
            throw "Managed workspace source directory is missing or is a reparse point: $directory"
        }
        # Walk one level at a time: recursive file enumeration may skip a
        # junction entirely and therefore cannot prove that source is local.
        foreach ($entry in @(Get-ChildItem -LiteralPath $directory -Force)) {
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Managed workspace source may not be a reparse point: $($entry.FullName)"
            }
            if ($entry.PSIsContainer) { $pendingDirectories.Push($entry.FullName) }
        }
    }
    $records += "source`n$(Get-DirectoryContentHash -Root $sourceRoot)"
    return "sha256:$(Get-TextSha256 -Text (($records -join "`n") + "`n"))"
}

function Enter-UpdateMutex {
    param([Parameter(Mandatory = $true)][string]$UserProfile)
    $name = "Local\BridgeForgeCodex.SharedSkillUpdate.$(Get-TextSha256 -Text $UserProfile)"
    $mutex = New-Object Threading.Mutex($false, $name)
    try {
        $acquired = $mutex.WaitOne(0)
    }
    catch [Threading.AbandonedMutexException] {
        $acquired = $true
    }
    if (-not $acquired) {
        $mutex.Dispose()
        throw "Another bridgeforge-codex shared-skill update is already running for this user."
    }
    return $mutex
}

function Read-JsonFile {
    param([Parameter(Mandatory = $true)][string]$Path)
    try {
        $raw = Get-Content -LiteralPath $Path -Raw -Encoding UTF8
        Assert-JsonUniqueObjectKeys -Json $raw -Path $Path
        return $raw | ConvertFrom-Json
    }
    catch {
        throw "Invalid JSON file '$Path': $($_.Exception.Message)"
    }
}

function Assert-JsonUniqueObjectKeys {
    param(
        [Parameter(Mandatory = $true)][string]$Json,
        [Parameter(Mandatory = $true)][string]$Path
    )
    $frames = New-Object System.Collections.ArrayList
    $index = 0
    while ($index -lt $Json.Length) {
        $character = $Json[$index]
        if ($character -eq '{') {
            [void]$frames.Add(@{ kind = "object"; keys = @{} })
            $index += 1
            continue
        }
        if ($character -eq '[') {
            [void]$frames.Add(@{ kind = "array" })
            $index += 1
            continue
        }
        if ($character -eq '}' -or $character -eq ']') {
            if ($frames.Count -gt 0) {
                $frames.RemoveAt($frames.Count - 1)
            }
            $index += 1
            continue
        }
        if ($character -ne '"') {
            $index += 1
            continue
        }

        $start = $index
        $index += 1
        $closed = $false
        while ($index -lt $Json.Length) {
            if ($Json[$index] -eq '\') {
                $index += 2
                continue
            }
            if ($Json[$index] -eq '"') {
                $closed = $true
                $index += 1
                break
            }
            $index += 1
        }
        if (-not $closed) {
            throw "Invalid JSON file '$Path': unterminated string."
        }
        $probe = $index
        while ($probe -lt $Json.Length -and [char]::IsWhiteSpace($Json[$probe])) {
            $probe += 1
        }
        if ($probe -ge $Json.Length -or $Json[$probe] -ne ':') {
            continue
        }
        if ($frames.Count -eq 0 -or $frames[$frames.Count - 1].kind -ne "object") {
            throw "Invalid JSON file '$Path': property key outside an object."
        }
        $token = $Json.Substring($start, $index - $start)
        $key = [string]($token | ConvertFrom-Json)
        $keys = $frames[$frames.Count - 1].keys
        if ($keys.ContainsKey($key)) {
            throw "Invalid JSON file '$Path': duplicate JSON key '$key'."
        }
        $keys[$key] = $true
    }
}

function Assert-ExactJsonProperties {
    param(
        [Parameter(Mandatory = $true)]$Value,
        [Parameter(Mandatory = $true)][string[]]$Expected,
        [Parameter(Mandatory = $true)][string]$Label
    )
    $actual = @($Value.PSObject.Properties | ForEach-Object { [string]$_.Name })
    $missing = @($Expected | Where-Object { $actual -cnotcontains $_ })
    $extra = @($actual | Where-Object { $Expected -cnotcontains $_ })
    if ($missing.Count -gt 0 -or $extra.Count -gt 0 -or $actual.Count -ne $Expected.Count) {
        throw "$Label fields are not exact."
    }
}

function Write-JsonAtomic {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)]$Value
    )
    $parent = Split-Path -Parent $Path
    if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
    }
    $temporary = "$Path.tmp-$PID-$([Guid]::NewGuid().ToString('N'))"
    try {
        $Value | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $temporary -Encoding UTF8
        Move-Item -LiteralPath $temporary -Destination $Path -Force
    }
    finally {
        if (Test-Path -LiteralPath $temporary) {
            Remove-Item -LiteralPath $temporary -Force
        }
    }
}

function Assert-Repository {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [switch]$SkipFetch,
        [string]$ExpectedCommit,
        [string]$InstalledHome
    )
    if (-not (Test-Path -LiteralPath $Root -PathType Container)) {
        throw "Source repository does not exist: $Root"
    }
    $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd("\") + "\"
    $repositoryFull = [IO.Path]::GetFullPath($Root).TrimEnd("\")
    $installed = -not [string]::IsNullOrWhiteSpace($InstalledHome)
    if ($installed) {
        if ($repositoryFull -ne [IO.Path]::GetFullPath($InstalledHome).TrimEnd('\') -or
            -not $SkipFetch -or $ExpectedCommit -notmatch '^[0-9a-f]{40}$') {
            throw 'Installed source reuse requires the exact managed home and a freshly observed commit.'
        }
        Assert-NoReparseUnderProfile -Path $repositoryFull -UserProfile (Split-Path -Parent $InstalledHome)
    }
    elseif (-not $repositoryFull.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Source repository must be a clean clone under the system temporary directory."
    }
    $remote = ((Invoke-Git -WorkingDirectory $Root -Arguments @("config", "--get", "remote.origin.url")) -join "").Trim()
    if ((Get-NormalizedRemote $remote) -ne (Get-NormalizedRemote $CanonicalRemote)) {
        throw "Source repository origin is not the canonical bridgeforge-codex remote."
    }
    if (-not $SkipFetch) {
        Invoke-Git -WorkingDirectory $Root -Arguments @(
            "fetch",
            "--no-tags",
            "--prune",
            "origin",
            "+refs/heads/main:refs/remotes/origin/main"
        ) | Out-Null
    }
    $branch = ((Invoke-Git -WorkingDirectory $Root -Arguments @("symbolic-ref", "--short", "HEAD")) -join "").Trim()
    if ($branch -ne $CanonicalBranch) {
        throw "Source repository must have main checked out; found '$branch'."
    }
    $commit = ((Invoke-Git -WorkingDirectory $Root -Arguments @("rev-parse", "HEAD")) -join "").Trim().ToLowerInvariant()
    if ($commit -notmatch "^[0-9a-f]{40}$") {
        throw "Source repository HEAD is not a full commit SHA."
    }
    $remoteCommit = ((
        Invoke-Git -WorkingDirectory $Root -Arguments @("rev-parse", "refs/remotes/origin/main")
    ) -join "").Trim().ToLowerInvariant()
    if ($remoteCommit -notmatch "^[0-9a-f]{40}$" -or $commit -ne $remoteCommit) {
        throw "Source repository HEAD does not match the fetched canonical origin/main."
    }
    if (-not [string]::IsNullOrWhiteSpace($ExpectedCommit) -and $commit -ne $ExpectedCommit) {
        throw 'Canonical source changed after the remote probe; retry with a fresh source selection.'
    }
    if (Test-Path -LiteralPath (Join-Path $Root ".gitmodules")) {
        throw "Submodules are not allowed in the shared-skill source."
    }
    $gitlinks = @(
        Invoke-Git -WorkingDirectory $Root -Arguments @("ls-files", "--stage") |
            Where-Object { [string]$_ -match "^160000\s" }
    )
    if ($gitlinks.Count -gt 0) {
        throw "Submodules are not allowed in the shared-skill source."
    }
    $changes = @(
        Invoke-Git -WorkingDirectory $Root -Arguments @(
            "-c", "core.excludesFile=$(Join-Path $Root '.git\info\exclude')",
            "status", "--porcelain", "--untracked-files=all"
        )
    )
    if ($changes.Count -gt 0) {
        throw "Source repository clone is not clean."
    }
    return $commit
}

function Get-PlatformConfig {
    param(
        [Parameter(Mandatory = $true)][string]$Platform,
        [Parameter(Mandatory = $true)][string]$UserProfile
    )
    if ($Platform -eq "codex") {
        return @{
            skills_root = Join-Path $UserProfile ".codex\skills"
            ledger = Join-Path $UserProfile ".codex\bridgeforge-codex-managed.json"
            manifest_target = "~/.codex/skills"
        }
    }
    throw "Unsupported platform in manifest: $Platform"
}

function Get-PlatformManifest {
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$Platform
    )
    $property = $Manifest.platforms.PSObject.Properties[$Platform]
    if ($null -eq $property) {
        throw "Manifest is missing platform '$Platform'."
    }
    return $property.Value
}

function Assert-ProductManifestHash {
    param([Parameter(Mandatory = $true)][string]$RepositoryRoot, [Parameter(Mandatory = $true)][string]$Expected)
    $path = Join-Path $RepositoryRoot $ManifestName
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or
        "sha256:$(Get-Sha256 -Path $path)" -ne $Expected) {
        throw "Product manifest changed after planning."
    }
}

function Assert-Manifest {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryRoot,
        [Parameter(Mandatory = $true)][string]$UserProfile,
        [switch]$ValidateSources
    )
    $manifestPath = Join-Path $RepositoryRoot $ManifestName
    if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
        throw "Missing root distribution manifest: $ManifestName"
    }
    $capturedHash = "sha256:$(Get-Sha256 -Path $manifestPath)"
    $product = Read-JsonFile -Path $manifestPath
    Assert-ExactJsonProperties -Value $product -Expected @("schema_version", "user_instructions", "shared_skills", "project_skeleton") -Label "Product manifest"
    if ([int]$product.schema_version -ne 1) { throw "Unsupported product manifest schema." }
    foreach ($section in @("user_instructions", "shared_skills", "project_skeleton")) {
        if ($null -eq $product.$section -or $product.$section -isnot [PSCustomObject]) {
            throw "Product manifest section '$section' must be an object."
        }
    }
    $manifest = $product.shared_skills
    Assert-ExactJsonProperties `
        -Value $manifest `
        -Expected @("schema_version", "canonical_remote", "branch", "platforms") `
        -Label "Manifest top-level"
    if ([int]$manifest.schema_version -ne 1) {
        throw "Unsupported bridgeforge-codex manifest schema."
    }
    if ((Get-NormalizedRemote ([string]$manifest.canonical_remote)) -ne (Get-NormalizedRemote $CanonicalRemote)) {
        throw "Manifest product remote does not match the installer contract."
    }
    if ([string]$manifest.branch -ne $CanonicalBranch) {
        throw "Manifest branch does not match main."
    }
    Assert-ExactJsonProperties -Value $manifest.platforms -Expected @("codex") -Label "Manifest platforms"

    foreach ($platform in @("codex")) {
        $platformManifest = Get-PlatformManifest -Manifest $manifest -Platform $platform
        Assert-ExactJsonProperties `
            -Value $platformManifest `
            -Expected @("target", "skills") `
            -Label "Manifest platform '$platform'"
        $config = Get-PlatformConfig -Platform $platform -UserProfile $UserProfile
        if ([string]$platformManifest.target -ne $config.manifest_target) {
            throw "Manifest target for $platform is not the fixed user skill directory."
        }
        $names = @{}
        foreach ($skill in @($platformManifest.skills)) {
            Assert-ExactJsonProperties `
                -Value $skill `
                -Expected @("name", "files") `
                -Label "Manifest skill"
            $name = [string]$skill.name
            if ($name -cnotmatch "^[a-z0-9][a-z0-9-]*$") {
                throw "Unsafe skill name in manifest: $name"
            }
            $nameKey = $name.ToLowerInvariant()
            if ($names.ContainsKey($nameKey)) {
                throw "Duplicate skill name in $platform manifest: $name"
            }
            $names[$nameKey] = $true
            $targets = @{}
            $files = @($skill.files)
            if ($files.Count -eq 0) {
                throw "Skill '$name' has no files in the manifest."
            }
            foreach ($file in $files) {
                Assert-ExactJsonProperties `
                    -Value $file `
                    -Expected @("source", "target", "sha256") `
                    -Label "Manifest file for '$name'"
                $source = [string]$file.source
                $target = [string]$file.target
                if (
                    $source.Contains("\") -or $target.Contains("\") -or
                    $source.IndexOfAny([char[]]@('*', '?', '[')) -ge 0 -or
                    $target.IndexOfAny([char[]]@('*', '?', '[')) -ge 0 -or
                    -not (Test-SafeRelativePath $source) -or
                    -not (Test-SafeRelativePath $target)
                ) {
                    throw "Unsafe file path in manifest skill '$name'."
                }
                $targetKey = $target.Replace("\", "/").ToLowerInvariant()
                if ($targets.ContainsKey($targetKey)) {
                    throw "Duplicate target path in manifest skill '$name': $target"
                }
                $targets[$targetKey] = $true
                $sourcePath = Get-PathUnderRoot -Root $RepositoryRoot -RelativePath $source
                $declaredHash = [string]$file.sha256
                if ($declaredHash -cnotmatch "^sha256:[0-9a-f]{64}$") {
                    throw "Invalid SHA-256 in manifest for '$source'."
                }
                $expected = $declaredHash.Substring(7)
                if ($ValidateSources) {
                    if (-not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
                        throw "Manifest source file is missing: $source"
                    }
                    if ((Get-Sha256 -Path $sourcePath) -ne $expected) {
                        throw "Manifest SHA-256 mismatch for '$source'."
                    }
                }
            }
            if ($name -eq 'bridgeforge-codex' -and
                (-not $targets.ContainsKey('maintenance.md') -or $targets.ContainsKey('skill.md') -or $targets.ContainsKey('agents/openai.yaml'))) {
                throw 'Forge maintenance bundle must be internal and must not expose a Codex skill.'
            }
        }
        if ($platform -eq "codex" -and -not $names.ContainsKey("bridgeforge-codex")) {
            throw "Codex must distribute the internal Forge maintenance bundle."
        }
    }
    Assert-ProductManifestHash -RepositoryRoot $RepositoryRoot -Expected $capturedHash
    return @{
        value = $manifest
        path = $manifestPath
        hash = $capturedHash
    }
}

function Read-Ledger {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Platform
    )
    if (-not (Test-Path -LiteralPath $Path)) {
        return $null
    }
    $ledger = Read-JsonFile -Path $Path
    if ([int]$ledger.schema_version -ne 1 -or [string]$ledger.platform -ne $Platform -or $null -eq $ledger.records) {
        throw "Invalid managed ledger for ${platform}: $Path"
    }
    foreach ($property in $ledger.records.PSObject.Properties) {
        $name = $property.Name
        $record = $property.Value
        if ($name -notmatch "^[A-Za-z0-9][A-Za-z0-9._-]*$" -or
            [string]$record.source_commit -notmatch "^[0-9a-f]{40}$" -or
            [string]$record.content_hash -notmatch "^sha256:[0-9a-f]{64}$" -or
            [string]::IsNullOrWhiteSpace([string]$record.installed_at)) {
            throw "Invalid record '$name' in managed ledger for $platform."
        }
    }
    $consentsProperty = $ledger.PSObject.Properties["consents"]
    if ($null -ne $consentsProperty) {
        if ($Platform -ne "codex") {
            throw "Managed ledger consents are only valid for codex: $Path"
        }
        $consents = $consentsProperty.Value
        $names = @($consents.PSObject.Properties | ForEach-Object { $_.Name })
        $nativeConsent = $consents.native_memories
        $validNativeConsent = $false
        if ($nativeConsent -is [string]) {
            $validNativeConsent = ([string]$nativeConsent -in @("approved", "declined"))
        }
        elseif ($null -ne $nativeConsent) {
            $consentNames = @($nativeConsent.PSObject.Properties | ForEach-Object { $_.Name } | Sort-Object)
            $expectedConsentNames = @(
                "auto_hook_maintenance",
                "decision",
                "policy_version",
                "remote",
                "repository",
                "require_private",
                "scope",
                "sync_mode"
            )
            $remoteText = [string]$nativeConsent.remote
            $normalizedRemote = $remoteText.Trim().TrimEnd([char[]]"/\\").ToLowerInvariant()
            if ($normalizedRemote.EndsWith(".git")) {
                $normalizedRemote = $normalizedRemote.Substring(0, $normalizedRemote.Length - 4)
            }
            $remoteMatchesRepository = (
                $normalizedRemote.EndsWith("/bridgeforge-codex-memories") -or
                $normalizedRemote.EndsWith(":bridgeforge-codex-memories")
            )
            $validNativeConsent = (
                @(Compare-Object -ReferenceObject $expectedConsentNames -DifferenceObject $consentNames).Count -eq 0 -and
                [string]$nativeConsent.decision -in @("approved", "declined") -and
                [int]$nativeConsent.policy_version -eq 1 -and
                [string]$nativeConsent.scope -eq "~/.codex/memories/**" -and
                [string]$nativeConsent.sync_mode -eq "bidirectional" -and
                [bool]$nativeConsent.auto_hook_maintenance -and
                [string]$nativeConsent.repository -eq "bridgeforge-codex-memories" -and
                [bool]$nativeConsent.require_private -and
                (
                    ([string]$nativeConsent.decision -eq "approved" -and
                        -not [string]::IsNullOrWhiteSpace($remoteText) -and
                        $remoteMatchesRepository) -or
                    ([string]$nativeConsent.decision -eq "declined" -and
                        $null -eq $nativeConsent.remote)
                )
            )
        }
        if ($names.Count -ne 1 -or $names[0] -ne "native_memories" -or -not $validNativeConsent) {
            throw "Invalid managed ledger consents for ${platform}: $Path"
        }
    }
    return $ledger
}

function Test-ReparsePoint {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return $false
    }
    $item = Get-Item -LiteralPath $Path -Force
    return (($item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Assert-WriteAccess {
    param([Parameter(Mandatory = $true)][string]$Path)
    $cursor = $Path
    while (-not (Test-Path -LiteralPath $cursor -PathType Container)) {
        if (Test-Path -LiteralPath $cursor) {
            throw "Target path component exists but is not a directory: $cursor"
        }
        $parent = Split-Path -Parent $cursor
        if ([string]::IsNullOrWhiteSpace($parent) -or $parent -eq $cursor) {
            throw "No existing parent available for write preflight: $Path"
        }
        $cursor = $parent
    }
    $probe = Join-Path $cursor ".bridgeforge-codex-write-probe-$PID-$([Guid]::NewGuid().ToString('N'))"
    try {
        [IO.File]::WriteAllText($probe, "probe")
    }
    catch {
        throw "Target directory is not writable: $Path"
    }
    finally {
        if (Test-Path -LiteralPath $probe) {
            Remove-Item -LiteralPath $probe -Force
        }
    }
}

function Assert-NoReparseUnderProfile {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$UserProfile
    )
    $profileFull = [IO.Path]::GetFullPath($UserProfile).TrimEnd("\")
    $cursor = [IO.Path]::GetFullPath($Path).TrimEnd("\")
    if (-not ($cursor.Equals($profileFull, [StringComparison]::OrdinalIgnoreCase) -or
        $cursor.StartsWith($profileFull + "\", [StringComparison]::OrdinalIgnoreCase))) {
        throw "User-level target escapes USERPROFILE: $Path"
    }
    while ($cursor.Length -ge $profileFull.Length) {
        if ((Test-Path -LiteralPath $cursor) -and (Test-ReparsePoint -Path $cursor)) {
            throw "User-level target path may not traverse a junction or symbolic link: $cursor"
        }
        if ($cursor.Equals($profileFull, [StringComparison]::OrdinalIgnoreCase)) {
            break
        }
        $cursor = Split-Path -Parent $cursor
    }
}

function Get-LedgerNames {
    param($Ledger)
    if ($null -eq $Ledger) {
        return @()
    }
    return @($Ledger.records.PSObject.Properties | ForEach-Object { $_.Name })
}

function Get-LedgerRecord {
    param($Ledger, [Parameter(Mandatory = $true)][string]$Name)
    if ($null -eq $Ledger) {
        return $null
    }
    $property = @($Ledger.records.PSObject.Properties | Where-Object { $_.Name -ieq $Name })
    if ($property.Count -eq 0) {
        return $null
    }
    return $property[0].Value
}

function New-UpdatePlan {
    param(
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$UserProfile,
        [Parameter(Mandatory = $true)][string]$OperationId,
        [Parameter(Mandatory = $true)][string]$Commit,
        [string]$OnlyBundle
    )
    $platformPlans = @()
    foreach ($platform in @("codex")) {
        $config = Get-PlatformConfig -Platform $platform -UserProfile $UserProfile
        $platformManifest = Get-PlatformManifest -Manifest $Manifest -Platform $platform
        if ($OnlyBundle -and @($platformManifest.skills | Where-Object { $_.name -cne $OnlyBundle }).Count -gt 0) {
            throw 'Scoped maintenance cannot include other bundles.'
        }
        $ledgerOriginalHash = if (Test-Path -LiteralPath $config.ledger) { Get-Sha256 -Path $config.ledger } else { $null }
        $ledger = Read-Ledger -Path $config.ledger -Platform $platform
        $ledgerReadHash = if (Test-Path -LiteralPath $config.ledger) { Get-Sha256 -Path $config.ledger } else { $null }
        if ($ledgerOriginalHash -cne $ledgerReadHash) { throw 'Shared ledger changed while reading; preserve external edits.' }
        $managedNames = @(Get-LedgerNames -Ledger $ledger)
        $retainedRecords = $null
        if ($OnlyBundle) {
            if ($OnlyBundle -cne 'bridgeforge-codex') { throw 'Only the internal maintenance bundle supports scoped updates.' }
            $retainedRecords = [ordered]@{}
            foreach ($name in $managedNames) {
                if ($name -cne $OnlyBundle) { $retainedRecords[$name] = Get-LedgerRecord -Ledger $ledger -Name $name }
            }
            $managedNames = @($managedNames | Where-Object { $_ -ceq $OnlyBundle })
        }
        $managed = @{}
        foreach ($name in $managedNames) {
            $managed[$name.ToLowerInvariant()] = $true
        }
        $managedContentHashes = @{}
        foreach ($name in $managedNames) {
            $key = $name.ToLowerInvariant()
            $target = Join-Path $config.skills_root $name
            if (-not (Test-Path -LiteralPath $target)) {
                continue
            }
            if (Test-ReparsePoint -Path $target) {
                throw "Managed skill target may not be a junction or symbolic link: $target"
            }
            $actual = Get-DirectoryContentHash -Root $target
            $record = Get-LedgerRecord -Ledger $ledger -Name $name
            if ($null -eq $record -or [string]$record.content_hash -ne $actual) {
                throw "Managed skill content drifted outside bridgeforge-codex: $platform/${name}: $target"
            }
            $managedContentHashes[$key] = $actual
        }
        $manifestNames = @{}
        $ledgerNeedsUpdate = $false
        $preservedConsents = $null
        if ($platform -eq "codex" -and $null -ne $ledger -and
            $null -ne $ledger.PSObject.Properties["consents"]) {
            $preservedConsents = [ordered]@{
                native_memories = $ledger.consents.native_memories
            }
        }
        foreach ($skill in @($platformManifest.skills)) {
            $name = [string]$skill.name
            $manifestNames[$name.ToLowerInvariant()] = $true
            $target = Join-Path $config.skills_root $name
            if ((Test-Path -LiteralPath $target) -and -not $managed.ContainsKey($name.ToLowerInvariant())) {
                throw "Unmanaged skill conflict for $platform/${name}: $target"
            }
            if ((Test-Path -LiteralPath $target) -and (Test-ReparsePoint -Path $target)) {
                throw "Managed skill target may not be a junction or symbolic link: $target"
            }
        }
        $actions = @()
        foreach ($name in $managedNames | Sort-Object) {
            if (-not $manifestNames.ContainsKey($name.ToLowerInvariant())) {
                $ledgerNeedsUpdate = $true
                $target = Join-Path $config.skills_root $name
                if ((Test-Path -LiteralPath $target) -and (Test-ReparsePoint -Path $target)) {
                    throw "Managed skill target may not be a junction or symbolic link: $target"
                }
                $hadOriginal = [bool](Test-Path -LiteralPath $target)
                $originalHash = if ($hadOriginal) {
                    $managedContentHashes[$name.ToLowerInvariant()]
                }
                else {
                    $null
                }
                $actions += [ordered]@{
                    kind = "remove"
                    name = $name
                    target = $target
                    stage = $null
                    backup = Join-Path $config.skills_root ".$name.bridgeforge-codex-backup-$OperationId"
                    had_original = $hadOriginal
                    original_content_hash = $originalHash
                    status = "pending"
                }
            }
        }
        $upserts = @($platformManifest.skills | Sort-Object @{ Expression = { if ([string]$_.name -eq "bridgeforge-codex") { 1 } else { 0 } } }, name)
        foreach ($skill in $upserts) {
            $name = [string]$skill.name
            $target = Join-Path $config.skills_root $name
            $hadOriginal = [bool](Test-Path -LiteralPath $target)
            $originalHash = if ($hadOriginal) {
                $managedContentHashes[$name.ToLowerInvariant()]
            }
            else {
                $null
            }
            $desiredHash = Get-SkillContentHash -Skill $skill
            $ledgerRecord = Get-LedgerRecord -Ledger $ledger -Name $name
            if ($hadOriginal -and $null -ne $ledgerRecord -and
                [string]$ledgerRecord.content_hash -eq $desiredHash -and
                $originalHash -eq $desiredHash) {
                if ([string]$ledgerRecord.source_commit -ne $Commit) {
                    $ledgerNeedsUpdate = $true
                }
                continue
            }
            $ledgerNeedsUpdate = $true
            $actions += [ordered]@{
                kind = "upsert"
                name = $name
                target = $target
                stage = Join-Path $config.skills_root ".$name.bridgeforge-codex-stage-$OperationId"
                backup = Join-Path $config.skills_root ".$name.bridgeforge-codex-backup-$OperationId"
                had_original = $hadOriginal
                original_content_hash = $originalHash
                desired_content_hash = $desiredHash
                status = "pending"
            }
        }
        Assert-NoReparseUnderProfile -Path $config.skills_root -UserProfile $UserProfile
        Assert-NoReparseUnderProfile -Path (Split-Path -Parent $config.ledger) -UserProfile $UserProfile
        Assert-WriteAccess -Path $config.skills_root
        Assert-WriteAccess -Path (Split-Path -Parent $config.ledger)
        $platformPlans += [ordered]@{
            platform = $platform
            skills_root = $config.skills_root
            ledger = $config.ledger
            ledger_stage = "$($config.ledger).stage-$OperationId"
            ledger_backup = "$($config.ledger).backup-$OperationId"
            ledger_had_original = [bool](Test-Path -LiteralPath $config.ledger)
            ledger_original_hash = $ledgerOriginalHash
            ledger_desired_hash = $null
            ledger_needs_update = $ledgerNeedsUpdate
            ledger_status = "pending"
            preserved_consents = $preservedConsents
            retained_records = $retainedRecords
            actions = $actions
        }
    }
    return $platformPlans
}

function Assert-OperationPath {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Root
    )
    $rootFull = [IO.Path]::GetFullPath($Root).TrimEnd("\", "/")
    $pathFull = [IO.Path]::GetFullPath($Path)
    if (-not $pathFull.StartsWith($rootFull + "\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "Unsafe path in recovery log: $Path"
    }
}

function Remove-SafeTree {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        return
    }
    if (Test-ReparsePoint -Path $Path) {
        throw "Refusing to recursively remove a reparse point: $Path"
    }
    Remove-Item -LiteralPath $Path -Recurse -Force
}

function Get-BundleHash {
    param($Plan, [string]$Path)
    if ([string]$Plan.kind -eq "home") { return Get-DirectoryContentHash -Root $Path }
    return Get-Sha256 -Path $Path
}

function Initialize-BundlePlan {
    param($Plan, [string]$Kind)
    if ($null -eq $Plan) { return }
    $Plan["kind"] = $Kind
    $Plan["original_hash"] = if ([bool]$Plan.had_original) { Get-BundleHash -Plan $Plan -Path ([string]$Plan.target) } else { $null }
    $Plan["desired_hash"] = if ([bool]$Plan.needs_swap) { Get-BundleHash -Plan $Plan -Path ([string]$Plan.stage) } else { $Plan.original_hash }
}

function Assert-BundlePlan {
    param($Plan, [string]$UserProfile, [string]$OperationId)
    if ($null -eq $Plan) { return }
    if ([string]$Plan.kind -eq "home") {
        $stem = $CommandHomeName.TrimStart(".")
        $target = Join-Path $UserProfile $CommandHomeName
        $stage = Join-Path $UserProfile ".$stem-stage-$OperationId"
        $backup = Join-Path $UserProfile ".$stem-backup-$OperationId"
    }
    elseif ([string]$Plan.kind -eq "cli") {
        $bin = Join-Path $UserProfile ".codex\bin"
        $target = Join-Path $bin "bridgeforge.exe"
        $stage = Join-Path $bin ".bridgeforge-stage-$OperationId.exe"
        $backup = Join-Path $bin ".bridgeforge-backup-$OperationId.exe"
    }
    elseif ([string]$Plan.kind -eq "cli-provenance") {
        $bin = Join-Path $UserProfile '.codex\bin'
        $target = Join-Path $bin 'bridgeforge-cli-provenance.json'
        $stage = Join-Path $bin ".bridgeforge-provenance-stage-$OperationId.json"
        $backup = Join-Path $bin ".bridgeforge-provenance-backup-$OperationId.json"
    }
    elseif ([string]$Plan.kind -eq "user-agents") {
        $codexRoot = Join-Path $UserProfile ".codex"
        $target = Join-Path $codexRoot "AGENTS.md"
        $stage = Join-Path $codexRoot ".AGENTS.bridgeforge-stage-$OperationId"
        $backup = Join-Path $codexRoot ".AGENTS.bridgeforge-backup-$OperationId.md"
    }
    else { throw "Invalid shared bundle component kind." }
    if ([string]$Plan.target -ne $target -or [string]$Plan.stage -ne $stage -or [string]$Plan.backup -ne $backup) {
        throw "Shared bundle recovery contains unexpected paths."
    }
    foreach ($path in @($target, $stage, $backup)) {
        Assert-NoReparseUnderProfile -Path $path -UserProfile $UserProfile
    }
    if ([string]$Plan.desired_hash -notmatch "^(sha256:)?[0-9a-f]{64}$" -or
        ([bool]$Plan.had_original -and [string]$Plan.original_hash -notmatch "^(sha256:)?[0-9a-f]{64}$")) {
        throw "Shared bundle recovery lacks content hashes."
    }
}

function Remove-BundlePath {
    param($Plan, [string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) { return }
    if ([string]$Plan.kind -eq "home") { Remove-SafeTree -Path $Path }
    else { Remove-Item -LiteralPath $Path -Force }
}

function Restore-BundlePlan {
    param($Plan, [bool]$Committed)
    if ($null -eq $Plan -or -not [bool]$Plan.needs_swap) { return }
    $target = [string]$Plan.target
    $backup = [string]$Plan.backup
    $stage = [string]$Plan.stage
    if (Test-RetiringBundle $Plan) {
        if ($Committed) {
            if ((Test-Path -LiteralPath $target) -or -not (Test-Path -LiteralPath $backup) -or
                (Get-BundleHash -Plan $Plan -Path $backup) -cne [string]$Plan.original_hash) {
                throw "Committed retirement or its recovery copy changed: $target"
            }
        }
        elseif (Test-Path -LiteralPath $backup) {
            if ((Test-Path -LiteralPath $target) -or
                (Get-BundleHash -Plan $Plan -Path $backup) -cne [string]$Plan.original_hash) {
                throw "Retirement rollback cannot prove ownership; preserve both paths: $target"
            }
            Move-Item -LiteralPath $backup -Destination $target
        }
        elseif (-not (Test-Path -LiteralPath $target) -or
            (Get-BundleHash -Plan $Plan -Path $target) -cne [string]$Plan.original_hash) {
            throw "Retirement rollback original changed or is missing: $target"
        }
        return
    }
    if ($Committed) {
        if (-not (Test-Path -LiteralPath $target) -or (Get-BundleHash -Plan $Plan -Path $target) -ne [string]$Plan.desired_hash) {
            throw "Committed shared bundle target is missing or changed: $target"
        }
        if ([string]$Plan.kind -eq "user-agents" -and [bool]$Plan.had_original) {
            if (-not (Test-Path -LiteralPath $backup -PathType Leaf) -or
                (Get-BundleHash -Plan $Plan -Path $backup) -ne [string]$Plan.original_hash) {
                throw "Committed user instruction backup is missing or changed: $backup"
            }
        }
        else { Remove-BundlePath -Plan $Plan -Path $backup }
    }
    elseif (Test-Path -LiteralPath $backup) {
        if (-not [bool]$Plan.had_original -or (Get-BundleHash -Plan $Plan -Path $backup) -ne [string]$Plan.original_hash) {
            throw "Shared bundle rollback backup differs from its witness: $backup"
        }
        if (Test-Path -LiteralPath $target) {
            if ((Get-BundleHash -Plan $Plan -Path $target) -ne [string]$Plan.desired_hash) { throw "Shared bundle target changed; preserving external state: $target" }
            Remove-BundlePath -Plan $Plan -Path $target
        }
        Move-Item -LiteralPath $backup -Destination $target
    }
    elseif ([bool]$Plan.had_original) {
        if (-not (Test-Path -LiteralPath $target) -or (Get-BundleHash -Plan $Plan -Path $target) -ne [string]$Plan.original_hash) {
            throw "Shared bundle original and rollback backup cannot be verified: $target"
        }
    }
    elseif (Test-Path -LiteralPath $target) {
        if ((Get-BundleHash -Plan $Plan -Path $target) -ne [string]$Plan.desired_hash) { throw "Shared bundle new target changed; preserving external state: $target" }
        Remove-BundlePath -Plan $Plan -Path $target
    }
    Remove-BundlePath -Plan $Plan -Path $stage
}

function Test-RetiringBundle($Plan) {
    if ($Plan -is [System.Collections.IDictionary]) { return $Plan.Contains('retire') -and [bool]$Plan.retire }
    return ($null -ne $Plan.PSObject.Properties['retire']) -and [bool]$Plan.retire
}

function Move-RetiringBundle($Plan) {
    if ((Test-Path -LiteralPath $Plan.backup) -or
        (Get-BundleHash -Plan $Plan -Path $Plan.target) -cne [string]$Plan.original_hash) {
        throw "Legacy Forge asset changed after planning: $($Plan.target)"
    }
    Move-Item -LiteralPath $Plan.target -Destination $Plan.backup
    if ((Get-BundleHash -Plan $Plan -Path $Plan.backup) -cne [string]$Plan.original_hash) {
        throw "Retired Forge asset backup differs from its reviewed bytes: $($Plan.target)"
    }
}

function Restore-InterruptedOperation {
    param(
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$UserProfile
    )
    if (-not (Test-Path -LiteralPath $LogPath)) {
        return
    }
    $log = Read-JsonFile -Path $LogPath
    if ([int]$log.schema_version -ne 1 -or [string]::IsNullOrWhiteSpace([string]$log.operation_id)) {
        throw "Invalid bridgeforge-codex shared update recovery log."
    }
    $operationId = [string]$log.operation_id
    if ($operationId -notmatch "^[0-9a-f]{32}$") {
        throw "Invalid operation identifier in recovery log."
    }
    $bundles = @()
    if ($null -ne $log.PSObject.Properties["bundles"]) { $bundles = @($log.bundles) }
    $seenBundles = @{}
    foreach ($bundle in $bundles) {
        Assert-BundlePlan -Plan $bundle -UserProfile $UserProfile -OperationId $operationId
        if ($seenBundles.ContainsKey([string]$bundle.kind)) { throw "Duplicate shared bundle component." }
        $seenBundles[[string]$bundle.kind] = $true
    }
    $seenPlatforms = @{}
    foreach ($platformPlan in @($log.platforms)) {
        $platform = [string]$platformPlan.platform
        if ($seenPlatforms.ContainsKey($platform)) {
            throw "Duplicate platform in recovery log: $platform"
        }
        $seenPlatforms[$platform] = $true
        $config = Get-PlatformConfig -Platform $platform -UserProfile $UserProfile
        $expectedLedgerStage = "$($config.ledger).stage-$operationId"
        $expectedLedgerBackup = "$($config.ledger).backup-$operationId"
        if ([string]$platformPlan.skills_root -ne $config.skills_root -or
            [string]$platformPlan.ledger -ne $config.ledger -or
            [string]$platformPlan.ledger_stage -ne $expectedLedgerStage -or
            [string]$platformPlan.ledger_backup -ne $expectedLedgerBackup) {
            throw "Recovery log contains unexpected paths for platform '$platform'."
        }
        foreach ($action in @($platformPlan.actions)) {
            $name = [string]$action.name
            $kind = [string]$action.kind
            if ($name -notmatch "^[A-Za-z0-9][A-Za-z0-9._-]*$" -or $kind -notin @("upsert", "remove")) {
                throw "Recovery log contains an invalid skill action."
            }
            $expectedTarget = Join-Path $config.skills_root $name
            $expectedBackup = Join-Path $config.skills_root ".$name.bridgeforge-codex-backup-$operationId"
            $expectedStage = if ($kind -eq "upsert") {
                Join-Path $config.skills_root ".$name.bridgeforge-codex-stage-$operationId"
            }
            else {
                $null
            }
            if ([string]$action.target -ne $expectedTarget -or
                [string]$action.backup -ne $expectedBackup -or
                [string]$action.stage -ne [string]$expectedStage) {
                throw "Recovery log contains unexpected paths for '$platform/$name'."
            }
            $originalHash = [string]$action.original_content_hash
            if (([bool]$action.had_original -and $originalHash -notmatch "^sha256:[0-9a-f]{64}$") -or
                (-not [bool]$action.had_original -and -not [string]::IsNullOrWhiteSpace($originalHash))) {
                throw "Recovery log lacks a valid original content hash for '$platform/$name'."
            }
        }
        if ([bool]$log.committed) {
            foreach ($action in @($platformPlan.actions)) {
                foreach ($temporary in @([string]$action.stage, [string]$action.backup)) {
                    if (-not [string]::IsNullOrWhiteSpace($temporary)) {
                        Assert-OperationPath -Path $temporary -Root $config.skills_root
                        Remove-SafeTree -Path $temporary
                    }
                }
            }
            foreach ($temporary in @([string]$platformPlan.ledger_stage, [string]$platformPlan.ledger_backup)) {
                if (-not [string]::IsNullOrWhiteSpace($temporary)) {
                    Assert-OperationPath -Path $temporary -Root (Split-Path -Parent $config.ledger)
                    if (Test-Path -LiteralPath $temporary) {
                        Remove-Item -LiteralPath $temporary -Force
                    }
                }
            }
            continue
        }

        foreach ($action in @($platformPlan.actions) | Sort-Object -Property name -Descending) {
            Assert-OperationPath -Path ([string]$action.target) -Root $config.skills_root
            Assert-OperationPath -Path ([string]$action.backup) -Root $config.skills_root
            if (-not [string]::IsNullOrWhiteSpace([string]$action.stage)) {
                Assert-OperationPath -Path ([string]$action.stage) -Root $config.skills_root
                Remove-SafeTree -Path ([string]$action.stage)
            }
            if ([bool]$action.had_original) {
                if (Test-Path -LiteralPath ([string]$action.backup)) {
                    Assert-DirectoryContentHash `
                        -Path ([string]$action.backup) `
                        -Expected ([string]$action.original_content_hash) `
                        -Context "Recovery backup for $platform/$([string]$action.name)"
                    Assert-RecoveryTarget -Action $action
                    Remove-SafeTree -Path ([string]$action.target)
                    Move-Item -LiteralPath ([string]$action.backup) -Destination ([string]$action.target)
                }
                elseif (-not (Test-Path -LiteralPath ([string]$action.target) -PathType Container) -or
                    (Get-DirectoryContentHash -Root ([string]$action.target)) -ne [string]$action.original_content_hash) {
                    throw "Recovery backup is missing and the original target cannot be verified for $platform/$([string]$action.name)."
                }
            }
            else {
                Assert-RecoveryTarget -Action $action
                Remove-SafeTree -Path ([string]$action.target)
                Remove-SafeTree -Path ([string]$action.backup)
            }
        }
        Assert-OperationPath -Path ([string]$platformPlan.ledger_backup) -Root (Split-Path -Parent $config.ledger)
        if ([bool]$platformPlan.ledger_had_original) {
            if (Test-Path -LiteralPath ([string]$platformPlan.ledger_backup)) {
                if (Test-Path -LiteralPath $config.ledger) {
                    Assert-RecoveryLedger -Plan $platformPlan
                    Remove-Item -LiteralPath $config.ledger -Force
                }
                Move-Item -LiteralPath ([string]$platformPlan.ledger_backup) -Destination $config.ledger
            }
        }
        elseif (Test-Path -LiteralPath $config.ledger) {
            Assert-RecoveryLedger -Plan $platformPlan
            Remove-Item -LiteralPath $config.ledger -Force
        }
        if (Test-Path -LiteralPath ([string]$platformPlan.ledger_stage)) {
            Remove-Item -LiteralPath ([string]$platformPlan.ledger_stage) -Force
        }
    }
    $scopeOnly = $null -ne $log.PSObject.Properties['user_scope'] -and [bool]$log.user_scope
    if ((!$scopeOnly -or $seenPlatforms.Count -gt 0) -and
        (-not $seenPlatforms.ContainsKey("codex") -or $seenPlatforms.Count -ne 1)) {
        throw "Recovery log must contain exactly the codex platform."
    }
    foreach ($bundle in $bundles) { Restore-BundlePlan -Plan $bundle -Committed ([bool]$log.committed) }
    if (@($bundles | Where-Object { [string]$_.kind -eq "home" }).Count -gt 0) {
        $homeLog = Join-Path $UserProfile $CommandHomeLogName
        if (Test-Path -LiteralPath $homeLog) {
            $homeState = Read-JsonFile -Path $homeLog
            if ([string]$homeState.operation_id -ne $operationId) { throw "Command-home journal belongs to another operation." }
            Remove-Item -LiteralPath $homeLog -Force
        }
    }
    Remove-Item -LiteralPath $LogPath -Force
}

function Assert-RecoveryTarget {
    param($Action)
    if (-not (Test-Path -LiteralPath ([string]$Action.target))) { return }
    if ([string]$Action.kind -ne 'upsert' -or
        $null -eq $Action.PSObject.Properties['desired_content_hash'] -or
        [string]$Action.desired_content_hash -notmatch '^sha256:[0-9a-f]{64}$') {
        throw 'Recovery cannot prove installed target ownership; preserve target and journal.'
    }
    Assert-DirectoryContentHash -Path ([string]$Action.target) -Expected ([string]$Action.desired_content_hash) -Context 'Recovery target; preserve external edits'
}

function Assert-RecoveryLedger {
    param($Plan)
    if ($null -eq $Plan.PSObject.Properties['ledger_desired_hash'] -or
        [string]$Plan.ledger_desired_hash -notmatch '^[0-9a-f]{64}$' -or
        (Get-Sha256 -Path ([string]$Plan.ledger)) -cne [string]$Plan.ledger_desired_hash) {
        throw 'Recovery ledger changed or lacks a write witness; preserve ledger and journal.'
    }
}

function Assert-LedgerSnapshot {
    param($Plan)
    $actual = if (Test-Path -LiteralPath ([string]$Plan.ledger)) { Get-Sha256 -Path ([string]$Plan.ledger) } else { $null }
    if ($actual -cne $Plan.ledger_original_hash) { throw 'Shared ledger changed after planning; preserve external edits.' }
}

function Copy-SkillToStage {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryRoot,
        [Parameter(Mandatory = $true)]$Skill,
        [Parameter(Mandatory = $true)][string]$Stage
    )
    Remove-SafeTree -Path $Stage
    New-Item -ItemType Directory -Path $Stage -Force | Out-Null
    foreach ($file in @($Skill.files)) {
        $source = Get-PathUnderRoot -Root $RepositoryRoot -RelativePath ([string]$file.source)
        $target = Get-PathUnderRoot -Root $Stage -RelativePath ([string]$file.target)
        $parent = Split-Path -Parent $target
        if (-not (Test-Path -LiteralPath $parent -PathType Container)) {
            New-Item -ItemType Directory -Path $parent -Force | Out-Null
        }
        Copy-Item -LiteralPath $source -Destination $target
    }
    Assert-DirectoryContentHash `
        -Path $Stage `
        -Expected (Get-SkillContentHash -Skill $Skill) `
        -Context "Staged content for $($Skill.name)"
}

function Assert-PlatformTargets {
    param(
        [Parameter(Mandatory = $true)]$PlatformManifest,
        [Parameter(Mandatory = $true)][string]$SkillsRoot,
        [Parameter(Mandatory = $true)][string]$Platform
    )
    foreach ($skill in @($PlatformManifest.skills)) {
        $target = Join-Path $SkillsRoot ([string]$skill.name)
        Assert-DirectoryContentHash `
            -Path $target `
            -Expected (Get-SkillContentHash -Skill $skill) `
            -Context "Installed content for $Platform/$([string]$skill.name)"
    }
}

function New-LedgerValue {
    param(
        [Parameter(Mandatory = $true)][string]$Platform,
        [Parameter(Mandatory = $true)]$PlatformManifest,
        [Parameter(Mandatory = $true)][string]$Commit,
        $PreservedConsents,
        $RetainedRecords
    )
    $records = [ordered]@{}
    if ($null -ne $RetainedRecords) {
        foreach ($name in $RetainedRecords.Keys) { $records[$name] = $RetainedRecords[$name] }
    }
    $installedAt = [DateTime]::UtcNow.ToString("o")
    foreach ($skill in @($PlatformManifest.skills) | Sort-Object -Property name) {
        $records[[string]$skill.name] = [ordered]@{
            source_commit = $Commit
            content_hash = Get-SkillContentHash -Skill $skill
            installed_at = $installedAt
        }
    }
    $ledger = [ordered]@{
        schema_version = 1
        platform = $Platform
        records = $records
    }
    if ($Platform -eq "codex" -and $null -ne $PreservedConsents) {
        $ledger.consents = [ordered]@{
            native_memories = $PreservedConsents.native_memories
        }
    }
    return $ledger
}

function Invoke-UpdateTransaction {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryRoot,
        [Parameter(Mandatory = $true)]$Manifest,
        [Parameter(Mandatory = $true)][string]$Commit,
        [Parameter(Mandatory = $true)][string]$ManifestHash,
        [Parameter(Mandatory = $true)][string]$UserProfile,
        [Parameter(Mandatory = $true)][string]$LogPath,
        [Parameter(Mandatory = $true)][string]$OperationId,
        [Parameter(Mandatory = $true)][AllowEmptyCollection()]$PlatformPlans,
        $CommandHomePlan,
        $RustCliPlan,
        $UserAgentsPlan,
        [object[]]$RetirementPlans = @(),
        [switch]$UserScope,
        $UserHookWitness
    )
    Assert-ProductManifestHash -RepositoryRoot $RepositoryRoot -Expected $ManifestHash
    foreach ($platformPlan in $PlatformPlans) { Assert-LedgerSnapshot -Plan $platformPlan }
    Initialize-BundlePlan -Plan $CommandHomePlan -Kind "home"
    Initialize-BundlePlan -Plan $RustCliPlan -Kind "cli"
    # User instruction witnesses come from Rust staging, before the transaction.
    $bundles = @(@($CommandHomePlan, $RustCliPlan, $UserAgentsPlan) | Where-Object { $null -ne $_ }) + $RetirementPlans
    foreach ($bundle in $bundles) { Assert-BundlePlan -Plan $bundle -UserProfile $UserProfile -OperationId $OperationId }
    $log = [ordered]@{
        schema_version = 1
        operation_id = $OperationId
        source_commit = $Commit
        manifest_hash = $ManifestHash
        started_at = [DateTime]::UtcNow.ToString("o")
        committed = $false
        platforms = $PlatformPlans
        bundles = $bundles
        user_scope = [bool]$UserScope
    }
    Write-JsonAtomic -Path $LogPath -Value $log
    $completedActionCount = 0

    try {
        if ($null -ne $UserHookWitness) {
            $currentHooks = if (Test-Path -LiteralPath $UserHookWitness.path) { Get-Sha256 -Path $UserHookWitness.path } else { $null }
            if ($currentHooks -cne $UserHookWitness.hash) { throw 'User Hook configuration changed after retirement planning.' }
        }
        foreach ($retirement in $RetirementPlans) { Move-RetiringBundle $retirement }
        if ($null -ne $CommandHomePlan) { Install-CommandHome -Plan $CommandHomePlan }
        if ($null -ne $RustCliPlan) { Install-RustCli -Plan $RustCliPlan }
        if ($null -ne $UserAgentsPlan) {
            Install-UserAgents -Plan $UserAgentsPlan -UserProfile $UserProfile -OperationId $OperationId
            if ($TestFailAfterSwap -eq "user-agents") { throw "Injected test failure after user-agents swap." }
        }
        Write-JsonAtomic -Path $LogPath -Value $log
        foreach ($platformPlan in $PlatformPlans) {
            $platformManifest = Get-PlatformManifest -Manifest $Manifest -Platform ([string]$platformPlan.platform
            )
            $platformActionCount = 0
            $skillByName = @{}
            foreach ($skill in @($platformManifest.skills)) {
                $skillByName[[string]$skill.name] = $skill
            }
            if (-not (Test-Path -LiteralPath ([string]$platformPlan.skills_root) -PathType Container)) {
                New-Item -ItemType Directory -Path ([string]$platformPlan.skills_root) -Force | Out-Null
            }
            foreach ($action in @($platformPlan.actions)) {
                if ([string]$action.kind -eq "upsert") {
                    Copy-SkillToStage -RepositoryRoot $RepositoryRoot -Skill $skillByName[[string]$action.name] -Stage ([string]$action.stage)
                }
                if (Test-Path -LiteralPath ([string]$action.backup)) {
                    throw "Unexpected transaction backup already exists: $($action.backup)"
                }
                $targetExists = [bool](Test-Path -LiteralPath ([string]$action.target) -PathType Container)
                if ($targetExists -ne [bool]$action.had_original) {
                    throw "Managed target changed after planning: $($action.target)"
                }
                if ([bool]$action.had_original) {
                    Assert-DirectoryContentHash `
                        -Path ([string]$action.target) `
                        -Expected ([string]$action.original_content_hash) `
                        -Context "Pre-swap target for $([string]$platformPlan.platform)/$([string]$action.name)"
                    Move-Item -LiteralPath ([string]$action.target) -Destination ([string]$action.backup)
                    Assert-DirectoryContentHash `
                        -Path ([string]$action.backup) `
                        -Expected ([string]$action.original_content_hash) `
                        -Context "Transaction backup for $([string]$platformPlan.platform)/$([string]$action.name)"
                }
                if ([string]$action.kind -eq "upsert") {
                    Move-Item -LiteralPath ([string]$action.stage) -Destination ([string]$action.target)
                    Assert-DirectoryContentHash `
                        -Path ([string]$action.target) `
                        -Expected (Get-SkillContentHash -Skill $skillByName[[string]$action.name]) `
                        -Context "Post-swap target for $([string]$platformPlan.platform)/$([string]$action.name)"
                }
                $action.status = "complete"
                Write-JsonAtomic -Path $LogPath -Value $log
                $completedActionCount += 1
                $platformActionCount += 1
                $failurePoint = "$([string]$platformPlan.platform):$platformActionCount"
                if (-not [string]::IsNullOrWhiteSpace($TestFailAfterSwap) -and
                    $TestFailAfterSwap -eq $failurePoint) {
                    throw "Injected test failure after swap $failurePoint; completed global actions: $completedActionCount."
                }
                if ($TestCrashAfterActionCount -gt 0 -and
                    $completedActionCount -ge $TestCrashAfterActionCount) {
                    [Environment]::Exit(91)
                }
            }
        }

        foreach ($platformPlan in $PlatformPlans) {
            $platformManifest = Get-PlatformManifest -Manifest $Manifest -Platform ([string]$platformPlan.platform)
            $ledgerValue = New-LedgerValue `
                -Platform ([string]$platformPlan.platform) `
                -PlatformManifest $platformManifest `
                -Commit $Commit `
                -PreservedConsents $platformPlan.preserved_consents -RetainedRecords $platformPlan.retained_records
            Write-JsonAtomic -Path ([string]$platformPlan.ledger_stage) -Value $ledgerValue
            $platformPlan.ledger_desired_hash = Get-Sha256 -Path ([string]$platformPlan.ledger_stage)
        }
        Write-JsonAtomic -Path $LogPath -Value $log
        foreach ($platformPlan in $PlatformPlans) {
            Assert-LedgerSnapshot -Plan $platformPlan
            if (Test-Path -LiteralPath ([string]$platformPlan.ledger)) {
                Move-Item -LiteralPath ([string]$platformPlan.ledger) -Destination ([string]$platformPlan.ledger_backup)
            }
            Move-Item -LiteralPath ([string]$platformPlan.ledger_stage) -Destination ([string]$platformPlan.ledger)
            $platformPlan.ledger_status = "complete"
            Write-JsonAtomic -Path $LogPath -Value $log
        }
        foreach ($platformPlan in $PlatformPlans) {
            $platform = [string]$platformPlan.platform
            Assert-PlatformTargets `
                -PlatformManifest (Get-PlatformManifest -Manifest $Manifest -Platform $platform) `
                -SkillsRoot ([string]$platformPlan.skills_root) `
                -Platform $platform
        }
        if ($null -ne $UserAgentsPlan -and
            (Get-Sha256 -Path ([string]$UserAgentsPlan.target)) -ne [string]$UserAgentsPlan.desired_hash) {
            throw "User instructions changed before shared commit."
        }
        Assert-RustCliState -Plan $RustCliPlan -RepositoryRoot $RepositoryRoot -Installed
        Assert-ProductManifestHash -RepositoryRoot $RepositoryRoot -Expected $ManifestHash
        if ($null -ne $UserHookWitness) {
            $currentHooks = if (Test-Path -LiteralPath $UserHookWitness.path) { Get-Sha256 -Path $UserHookWitness.path } else { $null }
            if ($currentHooks -cne $UserHookWitness.hash) { throw 'User Hook configuration changed during retirement.' }
        }
        $log.committed = $true
        Write-JsonAtomic -Path $LogPath -Value $log
    }
    catch {
        $failure = $_
        try {
            Restore-InterruptedOperation -LogPath $LogPath -UserProfile $UserProfile
        }
        catch {
            Write-Error "Shared skill update failed and automatic recovery also failed: $($_.Exception.Message)"
        }
        throw $failure
    }
    # The durable shared commit above covers Home, CLI, Skills and ledger together.
    # Cleanup is recoverable housekeeping; it must never undo committed components.
    try { Restore-InterruptedOperation -LogPath $LogPath -UserProfile $UserProfile }
    catch {
        $script:CleanupPending = $true
        Write-Warning "Shared update committed; backup cleanup deferred: $($_.Exception.Message)"
    }
}

function New-CanonicalClone {
    $root = Join-Path ([IO.Path]::GetTempPath()) "bridgeforge-codex-shared-$([Guid]::NewGuid().ToString('N'))"
    Invoke-Git -Arguments @(
        "clone",
        "--branch", $CanonicalBranch,
        "--single-branch",
        "--depth", "1",
        "--filter=blob:none",
        "--sparse",
        "--no-recurse-submodules",
        $CanonicalRemote,
        $root
    ) | Out-Null
    return $root
}

# Probe before downloading. A matching installed source still passes all source
# and manifest checks; only the clone/fetch work is skipped. Never fetch in place.
function Select-ResumeSource {
    param([Parameter(Mandatory = $true)][string]$UserProfile,
          [Parameter(Mandatory = $true)][string]$ExpectedCommit)
    if ($ExpectedCommit -notmatch '^[0-9a-f]{40}$') { throw 'Resume requires the completed receipt source commit.' }
    $installedSourceRoot = Join-Path $UserProfile $CommandHomeName
    $commit = Assert-Repository -Root $installedSourceRoot -SkipFetch -ExpectedCommit $ExpectedCommit -InstalledHome $installedSourceRoot
    $script:BuildCounts.source_reused += 1
    return [ordered]@{ root=$installedSourceRoot; clone=$null; commit=$commit }
}

function Select-CanonicalSource {
    param([Parameter(Mandatory = $true)][string]$UserProfile)
    $lines = @(Invoke-Git -Arguments @('ls-remote', '--exit-code', '--refs', $CanonicalRemote, "refs/heads/$CanonicalBranch"))
    if ($lines.Count -ne 1 -or [string]$lines[0] -notmatch '^([0-9a-fA-F]{40})\s+refs/heads/main$') {
        throw 'Canonical remote probe did not return one main commit.'
    }
    $expected = $Matches[1].ToLowerInvariant()
    $installedSourceRoot = Join-Path $UserProfile $CommandHomeName
    if (Test-Path -LiteralPath $installedSourceRoot) {
        # Dirty/unowned homes are errors, even if the remote has advanced.
        $identity = Get-RepositoryIdentity -Root $installedSourceRoot
        if ([string]$identity.commit -eq $expected -and
            (Get-NormalizedRemote ([string]$identity.remote)) -eq (Get-NormalizedRemote $CanonicalRemote)) {
            $commit = Assert-Repository -Root $installedSourceRoot -SkipFetch -ExpectedCommit $expected -InstalledHome $installedSourceRoot
            $script:BuildCounts.source_reused += 1
            return [ordered]@{ root=$installedSourceRoot; clone=$null; commit=$commit }
        }
    }
    $clone = New-CanonicalClone
    try {
        $script:BuildCounts.source_clones += 1
        $commit = Assert-Repository -Root $clone -SkipFetch -ExpectedCommit $expected
        Invoke-Git -WorkingDirectory $clone -Arguments @('-c', 'core.autocrlf=false', 'sparse-checkout', 'disable') | Out-Null
        return [ordered]@{ root=$clone; clone=$clone; commit=$commit }
    }
    catch {
        if (Test-Path -LiteralPath $clone) { Remove-SafeTree -Path $clone }
        throw
    }
}

function Get-GitHubRepositoryIdentity {
    param([Parameter(Mandatory = $true)][string]$Remote)
    $normalized = Get-NormalizedRemote $Remote
    if ($normalized -notmatch '^https://github\.com/([a-z0-9_.-]+)/([a-z0-9_.-]+)$') {
        throw 'Repository rename verification requires an explicit GitHub HTTPS identity.'
    }
    $endpoint = "https://api.github.com/repos/$($Matches[1])/$($Matches[2])"
    return Invoke-RestMethod -Uri $endpoint -Method Get -Headers @{
        'Accept' = 'application/vnd.github+json'
        'User-Agent' = 'BridgeForge-managed-update'
        'X-GitHub-Api-Version' = '2022-11-28'
    } -TimeoutSec 30
}

function Test-CanonicalHomeRemote {
    param([Parameter(Mandatory = $true)][string]$Remote)
    $normalized = Get-NormalizedRemote $Remote
    $canonical = Get-NormalizedRemote $CanonicalRemote
    if ($normalized -eq $canonical) { return $true }
    # The former official name is accepted only while GitHub proves the same
    # current repository identity. No origin rewrite and no arbitrary aliases.
    if ($normalized -ne 'https://github.com/freakybridge/bridgeforgecodex') { return $false }
    $previous = Get-GitHubRepositoryIdentity -Remote $Remote
    $current = Get-GitHubRepositoryIdentity -Remote $CanonicalRemote
    return ([long]$previous.id -gt 0 -and [long]$previous.id -eq [long]$current.id -and
        [string]$previous.full_name -eq [string]$current.full_name -and
        (Get-NormalizedRemote ([string]$current.html_url)) -eq $canonical)
}

function Get-RepositoryIdentity {
    param([Parameter(Mandatory = $true)][string]$Root)
    if (-not (Test-Path -LiteralPath $Root -PathType Container) -or
        (Test-ReparsePoint -Path $Root)) {
        throw "bridgeforge-codex command home is missing or unsafe: $Root"
    }
    $remote = ((Invoke-Git -WorkingDirectory $Root -Arguments @(
        "config", "--get", "remote.origin.url"
    )) -join "").Trim()
    if (-not (Test-CanonicalHomeRemote -Remote $remote)) {
        throw "bridgeforge-codex command home origin is not canonical: $Root"
    }
    $commit = ((Invoke-Git -WorkingDirectory $Root -Arguments @(
        "rev-parse", "HEAD"
    )) -join "").Trim().ToLowerInvariant()
    if ($commit -notmatch "^[0-9a-f]{40}$") {
        throw "bridgeforge-codex command home HEAD is invalid: $Root"
    }
    $changes = @(
        Invoke-Git -WorkingDirectory $Root -Arguments @(
            "-c", "core.excludesFile=$(Join-Path $Root '.git\info\exclude')",
            "status", "--porcelain", "--untracked-files=all"
        )
    )
    if ($changes.Count -gt 0) {
        throw "bridgeforge-codex command home contains local changes: $Root"
    }
    return [ordered]@{ remote = $remote; commit = $commit }
}

function New-CommandHomePlan {
    param(
        [Parameter(Mandatory = $true)][string]$SourceRoot,
        [Parameter(Mandatory = $true)][string]$UserProfile,
        [Parameter(Mandatory = $true)][string]$OperationId,
        [Parameter(Mandatory = $true)][string]$Commit
    )
    $target = Join-Path $UserProfile $CommandHomeName
    $homeStem = $CommandHomeName.TrimStart(".")
    $stage = Join-Path $UserProfile ".$homeStem-stage-$OperationId"
    $backup = Join-Path $UserProfile ".$homeStem-backup-$OperationId"
    $log = Join-Path $UserProfile $CommandHomeLogName
    Assert-NoReparseUnderProfile -Path $target -UserProfile $UserProfile
    Assert-WriteAccess -Path $target
    if ((Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $backup)) {
        throw "bridgeforge-codex command-home transaction path already exists."
    }
    $hadOriginal = [bool](Test-Path -LiteralPath $target -PathType Container)
    if ((Test-Path -LiteralPath $target) -and -not $hadOriginal) {
        throw "bridgeforge-codex command home target is not a directory: $target"
    }
    if ($hadOriginal) {
        $identity = Get-RepositoryIdentity -Root $target
        if ([string]$identity.commit -eq $Commit -and
            (Get-NormalizedRemote ([string]$identity.remote)) -eq (Get-NormalizedRemote $CanonicalRemote)) {
            return [ordered]@{
                target = $target
                stage = $stage
                backup = $backup
                log = $log
                operation_id = $OperationId
                had_original = $true
                needs_swap = $false
                status = "current"
            }
        }
    }
    Copy-Item -LiteralPath $SourceRoot -Destination $stage -Recurse -Force
    $stagedIdentity = Get-RepositoryIdentity -Root $stage
    if ([string]$stagedIdentity.commit -ne $Commit) {
        Remove-SafeTree -Path $stage
        throw "Staged bridgeforge-codex command home commit verification failed."
    }
    return [ordered]@{
        target = $target
        stage = $stage
        backup = $backup
        log = $log
        operation_id = $OperationId
        had_original = $hadOriginal
        needs_swap = $true
        status = "staged"
    }
}

function Install-CommandHome {
    param([Parameter(Mandatory = $true)]$Plan)
    if (-not [bool]$Plan.needs_swap) {
        return
    }
    try {
        $Plan.status = "prepared"
        Write-JsonAtomic -Path ([string]$Plan.log) -Value $Plan
        if ([bool]$Plan.had_original) {
            Move-Item -LiteralPath ([string]$Plan.target) -Destination ([string]$Plan.backup)
            $Plan.status = "backed_up"
            Write-JsonAtomic -Path ([string]$Plan.log) -Value $Plan
        }
        Move-Item -LiteralPath ([string]$Plan.stage) -Destination ([string]$Plan.target)
        $Plan.status = "installed"
        Write-JsonAtomic -Path ([string]$Plan.log) -Value $Plan
    }
    catch {
        if ((Test-Path -LiteralPath ([string]$Plan.target)) -and
            -not [bool]$Plan.had_original) {
            Remove-SafeTree -Path ([string]$Plan.target)
        }
        if ((Test-Path -LiteralPath ([string]$Plan.backup)) -and
            -not (Test-Path -LiteralPath ([string]$Plan.target))) {
            Move-Item -LiteralPath ([string]$Plan.backup) -Destination ([string]$Plan.target)
        }
        if (Test-Path -LiteralPath ([string]$Plan.stage)) {
            Remove-SafeTree -Path ([string]$Plan.stage)
        }
        if (Test-Path -LiteralPath ([string]$Plan.log)) {
            Remove-Item -LiteralPath ([string]$Plan.log) -Force
        }
        throw
    }
}

function Undo-CommandHome {
    param([Parameter(Mandatory = $true)]$Plan)
    if ([string]$Plan.status -eq "installed") {
        if (Test-Path -LiteralPath ([string]$Plan.target)) {
            Remove-SafeTree -Path ([string]$Plan.target)
        }
        if ([bool]$Plan.had_original -and (Test-Path -LiteralPath ([string]$Plan.backup))) {
            Move-Item -LiteralPath ([string]$Plan.backup) -Destination ([string]$Plan.target)
        }
    }
    elseif ([string]$Plan.status -eq "staged" -and
        (Test-Path -LiteralPath ([string]$Plan.stage))) {
        Remove-SafeTree -Path ([string]$Plan.stage)
    }
    if (Test-Path -LiteralPath ([string]$Plan.log)) {
        Remove-Item -LiteralPath ([string]$Plan.log) -Force
    }
}

function Restore-CommandHomeOperation {
    param([Parameter(Mandatory = $true)][string]$UserProfile)
    $logPath = Join-Path $UserProfile $CommandHomeLogName
    if (-not (Test-Path -LiteralPath $logPath)) {
        return
    }
    $value = Read-JsonFile -Path $logPath
    $operationId = [string]$value.operation_id
    if ($operationId -notmatch "^[0-9a-f]{32}$") {
        throw "Invalid bridgeforge-codex command-home recovery log."
    }
    $stem = $CommandHomeName.TrimStart(".")
    $target = Join-Path $UserProfile $CommandHomeName
    $stage = Join-Path $UserProfile ".$stem-stage-$operationId"
    $backup = Join-Path $UserProfile ".$stem-backup-$operationId"
    if ([string]$value.target -ne $target -or [string]$value.stage -ne $stage -or
        [string]$value.backup -ne $backup) {
        throw "Command-home recovery log contains unexpected paths."
    }
    $status = [string]$value.status
    if ($status -eq "committed") {
        if (-not (Test-Path -LiteralPath $target -PathType Container)) {
            throw "Committed command-home recovery is missing the installed target."
        }
        if (Test-Path -LiteralPath $backup) {
            Remove-SafeTree -Path $backup
        }
    }
    elseif ($status -eq "installed") {
        if (Test-Path -LiteralPath $target) {
            Remove-SafeTree -Path $target
        }
        if ([bool]$value.had_original) {
            if (-not (Test-Path -LiteralPath $backup -PathType Container)) {
                throw "Command-home rollback backup is missing."
            }
            Move-Item -LiteralPath $backup -Destination $target
        }
    }
    elseif ($status -eq "backed_up") {
        if (-not (Test-Path -LiteralPath $backup -PathType Container) -or
            (Test-Path -LiteralPath $target)) {
            throw "Command-home backup state is inconsistent."
        }
        Move-Item -LiteralPath $backup -Destination $target
    }
    elseif ($status -ne "prepared") {
        throw "Unknown command-home recovery status: $status"
    }
    if (Test-Path -LiteralPath $stage) {
        Remove-SafeTree -Path $stage
    }
    Remove-Item -LiteralPath $logPath -Force
}

function Get-ForgeProductVersion {
    param([string]$RepositoryRoot)
    foreach ($relative in @('source/forge/cli/Cargo.toml', 'source/cli/Cargo.toml')) {
        $path = Join-Path $RepositoryRoot $relative
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) { continue }
        $text = Get-Content -LiteralPath $path -Raw
        $section = [regex]::Match($text, '(?ms)^\[package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
        $version = [regex]::Match($section.Groups['body'].Value, '(?m)^version\s*=\s*["''](?<version>\d+\.\d+\.\d+)["'']\s*$')
        if ($version.Success) { return $version.Groups['version'].Value }
        if ($section.Groups['body'].Value -notmatch '(?m)^version\.workspace\s*=\s*true\s*$') { throw 'Forge CLI native version is missing.' }
        $workspace = Get-Content -LiteralPath (Join-Path $RepositoryRoot 'Cargo.toml') -Raw
        $section = [regex]::Match($workspace, '(?ms)^\[workspace\.package\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
        $version = [regex]::Match($section.Groups['body'].Value, '(?m)^version\s*=\s*["''](?<version>\d+\.\d+\.\d+)["'']\s*$')
        if (-not $version.Success) { throw 'Legacy Forge workspace version is missing.' }
        return $version.Groups['version'].Value
    }
    # Frozen products before source projection used VERSION for Forge alone.
    if ((Test-Path -LiteralPath (Join-Path $RepositoryRoot 'source/forge/core/Cargo.toml')) -or
        (Test-Path -LiteralPath (Join-Path $RepositoryRoot 'source/core/Cargo.toml'))) { throw 'Forge CLI source is missing.' }
    return (Get-Content -LiteralPath (Join-Path $RepositoryRoot 'VERSION') -Raw).Trim()
}

function Get-RustCliReuseIdentity {
    param([string]$Binary, [string]$RepositoryRoot)
    try {
        if (-not (Test-Path -LiteralPath $Binary -PathType Leaf) -or (Test-ReparsePoint -Path $Binary)) { return $null }
        $test = & $Binary self-test --json 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        $self = ($test -join "`n") | ConvertFrom-Json
        $expectedVersion = Get-ForgeProductVersion -RepositoryRoot $RepositoryRoot
        if ([int]$self.schema -ne 1 -or [string]$self.name -ne 'bridgeforge' -or
            [string]$self.status -ne 'ok' -or [string]$self.version -ne $expectedVersion -or
            @($self.capabilities) -notcontains 'user-cli-reuse-v1') { return $null }
        $result = & $Binary build-environment --source-root $RepositoryRoot 2>$null
        if ($LASTEXITCODE -ne 0) { return $null }
        $identity = ($result -join "`n") | ConvertFrom-Json
        if ([int]$identity.schema -ne 1 -or [string]$identity.status -ne 'cacheable' -or
            [string]$identity.identity -notmatch '^sha256:[0-9a-f]{64}$') { return $null }
        return [string]$identity.identity
    } catch { return $null }
}

function Assert-RustCliState {
    param($Plan, [string]$RepositoryRoot, [switch]$Installed)
    if ($null -eq $Plan -or -not $Plan.Contains('binary_sha256')) { return }
    $binary = if ($Installed -or -not [bool]$Plan.needs_swap) { [string]$Plan.target } else { [string]$Plan.stage }
    if ((Get-Sha256 -Path $binary) -ne [string]$Plan.binary_sha256) { throw 'Managed CLI bytes changed after preparation.' }
    if ($Plan.cache_identity -and (Get-RustCliReuseIdentity -Binary $binary -RepositoryRoot $RepositoryRoot) -ne [string]$Plan.cache_identity) { throw 'Managed CLI build inputs changed after preparation.' }
    if ((Get-Sha256 -Path $binary) -ne [string]$Plan.binary_sha256) { throw 'Managed CLI changed during validation.' }
}

function Save-RustCliProvenance {
    param($Plan, [string]$RepositoryRoot, [string]$UserProfile)
    if (-not $Plan.Contains('cache_identity') -or [string]::IsNullOrWhiteSpace([string]$Plan.cache_identity)) { return }
    try {
        $path = Join-Path $UserProfile '.codex/bin/bridgeforge-cli-provenance.json'
        Assert-NoReparseUnderProfile -Path $path -UserProfile $UserProfile
        if (Test-Path -LiteralPath $path) {
            $old = Read-JsonFile -Path $path
            if ([int]$old.schema -ne 1 -or [string]$old.owner -ne 'bridgeforge-user-cli-v1') { return }
        }
        if ((Get-RustCliReuseIdentity -Binary $Plan.target -RepositoryRoot $RepositoryRoot) -ne $Plan.cache_identity -or
            (Get-Sha256 -Path $Plan.target) -ne $Plan.binary_sha256) { return }
        Write-JsonAtomic -Path $path -Value ([ordered]@{ schema=1; owner='bridgeforge-user-cli-v1'; identity=$Plan.cache_identity; binary_sha256=$Plan.binary_sha256 })
    } catch { Write-Verbose "Optional CLI reuse provenance not saved: $($_.Exception.Message)" }
}

function New-RustCliPlan {
    param(
        [Parameter(Mandatory = $true)][string]$RepositoryRoot,
        [Parameter(Mandatory = $true)][string]$UserProfile,
        [Parameter(Mandatory = $true)][string]$OperationId,
        [switch]$KeepBuildTree
    )
    $manifest = Join-Path $RepositoryRoot "Cargo.toml"
    $lockfile = Join-Path $RepositoryRoot "Cargo.lock"
    if (-not (Test-Path -LiteralPath $manifest -PathType Leaf) -or
        -not (Test-Path -LiteralPath $lockfile -PathType Leaf)) {
        throw "Managed Rust workspace is incomplete."
    }
    $binRoot = Join-Path $UserProfile '.codex/bin'
    $target = Join-Path $binRoot 'bridgeforge.exe'
    $provenance = Join-Path $binRoot 'bridgeforge-cli-provenance.json'
    $stage = Join-Path $binRoot ".bridgeforge-stage-$OperationId.exe"
    $backup = Join-Path $binRoot ".bridgeforge-backup-$OperationId.exe"
    if ((Test-Path -LiteralPath $stage) -or (Test-Path -LiteralPath $backup)) { throw 'CLI staging paths already exist.' }
    $trustedInstalled = $false
    $phase = [Diagnostics.Stopwatch]::StartNew()
    try {
        Assert-NoReparseUnderProfile -Path $provenance -UserProfile $UserProfile
        if (Test-Path -LiteralPath $provenance -PathType Leaf) {
            $saved = Read-JsonFile -Path $provenance
            if ([int]$saved.schema -eq 1 -and [string]$saved.owner -eq 'bridgeforge-user-cli-v1' -and
                (Test-Path -LiteralPath $target -PathType Leaf) -and
                (Get-Sha256 -Path $target) -eq [string]$saved.binary_sha256) {
                $trustedInstalled = $true
                $identity = Get-RustCliReuseIdentity -Binary $target -RepositoryRoot $RepositoryRoot
                if ($identity -and $identity -eq [string]$saved.identity -and
                    (Get-Sha256 -Path $target) -eq [string]$saved.binary_sha256) {
                    $script:BuildCounts.installed_cli_reused += 1
                    return [ordered]@{ target=$target; stage=$stage; backup=$backup; had_original=$true; needs_swap=$false; status='current'; cache_identity=$identity; binary_sha256=[string]$saved.binary_sha256 }
                }
            }
        }
    } catch { Write-Verbose "CLI reuse unavailable: $($_.Exception.Message)" }
    finally { Complete-PhaseTiming -Name 'rust_cli_reuse_probe' -Stopwatch $phase }
    # A previous preparation may have succeeded while installation rolled back.
    # The installed CLI locates a verified public candidate; the candidate itself
    # must prove the target version/source identity before any swap is planned.
    try {
        if ($trustedInstalled -and (Test-Path -LiteralPath $target -PathType Leaf) -and -not (Test-ReparsePoint -Path $target)) {
            $located = & $target public-artifacts locate --product-root $RepositoryRoot --codex-home (Join-Path $UserProfile '.codex') 2>$null
            if ($LASTEXITCODE -eq 0) {
                $candidate = ($located -join "`n") | ConvertFrom-Json
                if ([int]$candidate.schema -eq 1 -and [string]$candidate.status -eq 'available') {
                    $cached = [string]$candidate.binary
                    Assert-NoReparseUnderProfile -Path $cached -UserProfile $UserProfile
                    $candidateHash = (Get-Sha256 -Path $cached)
                    $identity = Get-RustCliReuseIdentity -Binary $cached -RepositoryRoot $RepositoryRoot
                    if ($identity -and $identity -eq [string]$candidate.identity -and
                        "sha256:$candidateHash" -eq [string]$candidate.binary_sha256 -and
                        (Get-Sha256 -Path $cached) -eq $candidateHash) {
                        Copy-Item -LiteralPath $cached -Destination $stage
                        if ((Get-Sha256 -Path $stage) -ne $candidateHash) { throw 'Cached CLI changed while staging.' }
                        $script:BuildCounts.public_cli_reused += 1
                        return [ordered]@{ target=$target; stage=$stage; backup=$backup; had_original=$true; needs_swap=$true; status='staged'; cache_identity=$identity; binary_sha256=$candidateHash }
                    }
                }
            }
        }
    } catch {
        if (Test-Path -LiteralPath $stage) { throw }
        Write-Verbose "Public CLI bootstrap unavailable: $($_.Exception.Message)"
    }
    $targetDirectory = Join-Path ([IO.Path]::GetTempPath()) "bridgeforge-cli-build-$OperationId"
    $sourceBefore = Get-RustWorkspaceContentHash -RepositoryRoot $RepositoryRoot
    if (Test-Path -LiteralPath $targetDirectory) {
        Remove-SafeTree -Path $targetDirectory
    }
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    $phase = [Diagnostics.Stopwatch]::StartNew()
    Push-Location -LiteralPath (Split-Path -Parent $manifest)
    try {
        $script:BuildCounts.cargo_build_invocations += 1
        $output = & cargo build --locked --profile release --manifest-path $manifest --target-dir $targetDirectory --bin bridgeforge 2>&1
        $exitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousPreference
        Complete-PhaseTiming -Name 'rust_cli_build' -Stopwatch $phase
        Pop-Location
    }
    if ($exitCode -ne 0) {
        if (Test-Path -LiteralPath $targetDirectory) {
            Remove-SafeTree -Path $targetDirectory
        }
        throw "Managed Rust CLI build failed: $($output -join [Environment]::NewLine)"
    }
    if ((Get-RustWorkspaceContentHash -RepositoryRoot $RepositoryRoot) -ne $sourceBefore) {
        Remove-SafeTree -Path $targetDirectory
        throw 'Managed source changed during CLI bootstrap.'
    }
    $built = Join-Path $targetDirectory "release\bridgeforge.exe"
    if (-not (Test-Path -LiteralPath $built -PathType Leaf)) {
        Remove-SafeTree -Path $targetDirectory
        throw "Managed Rust CLI build produced no bridgeforge.exe."
    }
    $phase = [Diagnostics.Stopwatch]::StartNew()
    $selfTest = & $built self-test --json 2>&1
    Complete-PhaseTiming -Name 'rust_cli_self_test' -Stopwatch $phase
    if ($LASTEXITCODE -ne 0) {
        Remove-SafeTree -Path $targetDirectory
        throw "Managed Rust CLI self-test failed: $($selfTest -join [Environment]::NewLine)"
    }
    try {
        $receipt = ($selfTest -join "`n") | ConvertFrom-Json
    }
    catch {
        Remove-SafeTree -Path $targetDirectory
        throw "Managed Rust CLI self-test did not return JSON."
    }
    if ([int]$receipt.schema -ne 1 -or [string]$receipt.name -ne "bridgeforge" -or
        [string]$receipt.status -ne "ok" -or [string]$receipt.version -ne (Get-ForgeProductVersion -RepositoryRoot $RepositoryRoot)) {
        Remove-SafeTree -Path $targetDirectory
        throw "Managed Rust CLI self-test identity is invalid."
    }
    $binRoot = Join-Path $UserProfile ".codex\bin"
    if (-not (Test-Path -LiteralPath $binRoot -PathType Container)) {
        New-Item -ItemType Directory -Path $binRoot -Force | Out-Null
    }
    $target = Join-Path $binRoot "bridgeforge.exe"
    $stage = Join-Path $binRoot ".bridgeforge-stage-$OperationId.exe"
    $backup = Join-Path $binRoot ".bridgeforge-backup-$OperationId.exe"
    Copy-Item -LiteralPath $built -Destination $stage
    $phase = [Diagnostics.Stopwatch]::StartNew()
    $cacheIdentity = Get-RustCliReuseIdentity -Binary $built -RepositoryRoot $RepositoryRoot
    $binaryHash = Get-Sha256 -Path $built
    Complete-PhaseTiming -Name 'rust_cli_identity' -Stopwatch $phase
    if (-not $KeepBuildTree) { Remove-SafeTree -Path $targetDirectory }
    $current = Test-Path -LiteralPath $target -PathType Leaf
    $needsSwap = -not $current -or (Get-Sha256 -Path $target) -ne (Get-Sha256 -Path $stage)
    if (-not $needsSwap) {
        Remove-Item -LiteralPath $stage -Force
    }
    return [ordered]@{
        target = $target
        stage = $stage
        backup = $backup
        had_original = [bool]$current
        needs_swap = [bool]$needsSwap
        status = if ($needsSwap) { "staged" } else { "current" }
        cache_identity = $cacheIdentity
        binary_sha256 = $binaryHash
        bootstrap_target = if ($KeepBuildTree) { $targetDirectory } else { $null }
    }
}

function Prepare-PublicArtifacts {
    param($Plan, [string]$RepositoryRoot, [string]$UserProfile)
    $binary = if ([bool]$Plan.needs_swap) { [string]$Plan.stage } else { [string]$Plan.target }
    if (-not $Plan.cache_identity) { throw 'Public artifact preparation requires a verified compiler/source identity.' }
    # A Cargo bootstrap from the full factory workspace is only a helper. The
    # installed public CLI must come from the managed public source projection,
    # including its independently compiled source/lock provenance.
    $arguments = @('public-artifacts', 'prepare', '--product-root', $RepositoryRoot,
        '--codex-home', (Join-Path $UserProfile '.codex'))
    $phase = [Diagnostics.Stopwatch]::StartNew()
    try {
        $output = & $binary @arguments
        if ($LASTEXITCODE -ne 0) { throw 'Public artifact preparation failed; user installation was not committed.' }
        $receipt = ($output -join "`n") | ConvertFrom-Json
        if ([int]$receipt.schema -ne 1 -or [string]$receipt.status -ne 'prepared') {
            throw 'Public artifact preparation returned an invalid receipt.'
        }
        $located = & $binary public-artifacts locate --product-root $RepositoryRoot --codex-home (Join-Path $UserProfile '.codex')
        if ($LASTEXITCODE -ne 0) { throw 'Prepared public CLI lookup failed.' }
        $candidate = ($located -join "`n") | ConvertFrom-Json
        if ([int]$candidate.schema -ne 1 -or [string]$candidate.status -ne 'available') {
            throw 'Prepared public CLI identity unavailable.'
        }
        $publicBinary = [string]$candidate.binary
        Assert-NoReparseUnderProfile -Path $publicBinary -UserProfile $UserProfile
        $publicHash = Get-Sha256 -Path $publicBinary
        if ("sha256:$publicHash" -ne [string]$candidate.binary_sha256) { throw 'Prepared CLI bytes changed.' }
        $needsSwap = -not (Test-Path -LiteralPath ([string]$Plan.target) -PathType Leaf) -or
            (Get-Sha256 -Path ([string]$Plan.target)) -ne $publicHash
        if ($needsSwap) {
            Copy-Item -LiteralPath $publicBinary -Destination ([string]$Plan.stage) -Force
            if ((Get-Sha256 -Path ([string]$Plan.stage)) -ne $publicHash) { throw 'Prepared CLI changed while staging.' }
        } elseif (Test-Path -LiteralPath ([string]$Plan.stage)) {
            Remove-Item -LiteralPath ([string]$Plan.stage) -Force
        }
        $Plan.needs_swap = $needsSwap
        $Plan.status = if ($needsSwap) { 'staged' } else { 'current' }
        $Plan.binary_sha256 = $publicHash
        $Plan.cache_identity = [string]$candidate.identity
        $script:PublicArtifactsReceipt = $receipt
    } finally { Complete-PhaseTiming -Name 'public_artifacts' -Stopwatch $phase }
}

function Install-RustCli {
    param([Parameter(Mandatory = $true)]$Plan)
    if (-not [bool]$Plan.needs_swap) { return }
    if ([bool]$Plan.had_original) {
        Move-Item -LiteralPath ([string]$Plan.target) -Destination ([string]$Plan.backup)
    }
    try {
        Move-Item -LiteralPath ([string]$Plan.stage) -Destination ([string]$Plan.target)
        $Plan.status = "installed"
    }
    catch {
        if ([bool]$Plan.had_original -and (Test-Path -LiteralPath ([string]$Plan.backup))) {
            Move-Item -LiteralPath ([string]$Plan.backup) -Destination ([string]$Plan.target)
        }
        throw
    }
}

function Undo-RustCli {
    param([Parameter(Mandatory = $true)]$Plan)
    if ([string]$Plan.status -eq "installed") {
        if (Test-Path -LiteralPath ([string]$Plan.target)) {
            Remove-Item -LiteralPath ([string]$Plan.target) -Force
        }
        if ([bool]$Plan.had_original -and (Test-Path -LiteralPath ([string]$Plan.backup))) {
            Move-Item -LiteralPath ([string]$Plan.backup) -Destination ([string]$Plan.target)
        }
    }
    if (Test-Path -LiteralPath ([string]$Plan.stage)) {
        Remove-Item -LiteralPath ([string]$Plan.stage) -Force
    }
}

function New-UserAgentsPlan {
    param([string]$RepositoryRoot, [string]$UserProfile, [string]$OperationId, $RustCliPlan)
    $binary = if ([bool]$RustCliPlan.needs_swap) { [string]$RustCliPlan.stage } else { [string]$RustCliPlan.target }
    $previousEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
        $output = & $binary user-agents-stage --product-root $RepositoryRoot --user-profile $UserProfile --operation-id $OperationId
        if ($LASTEXITCODE -ne 0) { throw "Managed Rust user instruction staging failed." }
    }
    finally { [Console]::OutputEncoding = $previousEncoding }
    return (($output -join [Environment]::NewLine) | ConvertFrom-Json)
}

function Install-UserAgents {
    param($Plan, [string]$UserProfile, [string]$OperationId)
    Assert-BundlePlan -Plan $Plan -UserProfile $UserProfile -OperationId $OperationId
    $exists = Test-Path -LiteralPath ([string]$Plan.target)
    if ($exists -ne [bool]$Plan.had_original -or
        ($exists -and ((Get-Sha256 -Path ([string]$Plan.target)) -ne [string]$Plan.original_hash))) {
        throw "User instructions changed after planning; preserving the current file."
    }
    if (-not [bool]$Plan.needs_swap) { return }
    if (Test-Path -LiteralPath ([string]$Plan.backup)) { throw "User instruction backup already exists." }
    if ((Get-Sha256 -Path ([string]$Plan.stage)) -ne [string]$Plan.desired_hash) {
        throw "User instruction stage changed after planning."
    }
    if ([bool]$Plan.had_original) {
        Move-Item -LiteralPath ([string]$Plan.target) -Destination ([string]$Plan.backup)
        if ((Get-Sha256 -Path ([string]$Plan.backup)) -ne [string]$Plan.original_hash) {
            throw "User instruction backup differs from the planned original."
        }
    }
    Move-Item -LiteralPath ([string]$Plan.stage) -Destination ([string]$Plan.target)
    $Plan.status = "installed"
}

function New-UserRetirementPlans([string]$UserProfile, [string]$Operation) {
    $legacyCheckout = Join-Path $UserProfile $CommandHomeName
    $bin = Join-Path $UserProfile '.codex\bin'
    $cli = Join-Path $bin 'bridgeforge.exe'
    $proof = Join-Path $bin 'bridgeforge-cli-provenance.json'
    $cliHash = $null
    $proofHash = $null
    foreach ($path in @($legacyCheckout, $cli, $proof)) { Assert-NoReparseUnderProfile -Path $path -UserProfile $UserProfile }
    if ((Test-Path -LiteralPath $cli) -or (Test-Path -LiteralPath $proof)) {
        if (-not (Test-Path -LiteralPath $cli -PathType Leaf) -or -not (Test-Path -LiteralPath $proof -PathType Leaf)) {
            throw 'The legacy user CLI and its provenance are incomplete; both are preserved.'
        }
        $cliHash = Get-Sha256 -Path $cli
        $proofHash = Get-Sha256 -Path $proof
        $record = Read-JsonFile -Path $proof
        if ($record.schema -ne 1 -or $record.owner -cne 'bridgeforge-user-cli-v1' -or
            $record.binary_sha256 -cne $cliHash -or (Get-Sha256 -Path $proof) -cne $proofHash) { throw 'Legacy user CLI ownership is unproven; preserve it.' }
    }
    $checkoutIdentity = if (Test-Path -LiteralPath $legacyCheckout) { Get-RepositoryIdentity -Root $legacyCheckout } else { $null }
    $plans = @(
        [ordered]@{kind='home'; target=$legacyCheckout; stage=(Join-Path $UserProfile ".bridgeforge-codex-stage-$Operation"); backup=(Join-Path $UserProfile ".bridgeforge-codex-backup-$Operation")},
        [ordered]@{kind='cli'; target=$cli; stage=(Join-Path $bin ".bridgeforge-stage-$Operation.exe"); backup=(Join-Path $bin ".bridgeforge-backup-$Operation.exe")},
        [ordered]@{kind='cli-provenance'; target=$proof; stage=(Join-Path $bin ".bridgeforge-provenance-stage-$Operation.json"); backup=(Join-Path $bin ".bridgeforge-provenance-backup-$Operation.json")}
    )
    foreach ($plan in $plans) {
        if (-not (Test-Path -LiteralPath $plan.target)) { continue }
        $plan.original_hash = switch ($plan.kind) {
            'cli' { $cliHash }
            'cli-provenance' { $proofHash }
            default { Get-BundleHash -Plan $plan -Path $plan.target }
        }
        if ((Get-BundleHash -Plan $plan -Path $plan.target) -cne $plan.original_hash) {
            throw "Legacy Forge ownership changed after verification: $($plan.target)"
        }
        if ($plan.kind -eq 'home') {
            $after = Get-RepositoryIdentity -Root $legacyCheckout
            if ($null -eq $checkoutIdentity -or $after.commit -cne $checkoutIdentity.commit -or $after.remote -cne $checkoutIdentity.remote) {
                throw 'Legacy Forge checkout changed during ownership verification.'
            }
        }
        $plan.desired_hash = $plan.original_hash
        $plan.retire = $true
        $plan.had_original = $true
        $plan.needs_swap = $true
        Assert-BundlePlan -Plan $plan -UserProfile $UserProfile -OperationId $Operation
        if ((Test-Path -LiteralPath $plan.stage) -or (Test-Path -LiteralPath $plan.backup)) { throw 'Retirement recovery paths already exist.' }
        $plan
    }
}

function Invoke-CheckedJson([string]$Program, [string[]]$Arguments) {
    $previousEncoding = [Console]::OutputEncoding
    try {
        [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false)
        $output = & $Program @Arguments
        if ($LASTEXITCODE -ne 0) { throw "Managed command failed: $($Arguments[0])" }
    } finally { [Console]::OutputEncoding = $previousEncoding }
    return ($output -join "`n") | ConvertFrom-Json
}

function Get-UserHookRetirementWitness([string]$Path, [object[]]$Retirements) {
    if (-not (Test-Path -LiteralPath $Path)) { return @{path=$Path;hash=$null} }
    $witness = Get-Sha256 -Path $Path
    $document = Read-JsonFile -Path $Path
    $retiringPaths = @($Retirements | Where-Object { $_.kind -in @('home', 'cli', 'skill') } | ForEach-Object {
        $pathText = ([string]$_.target).Replace('\','/').ToLowerInvariant()
        $pathText
        $relativeCodex = $pathText.IndexOf('/.codex/')
        if ($relativeCodex -ge 0) { $pathText.Substring($relativeCodex + 1) }
    })
    $events = @()
    if ($null -ne $document.PSObject.Properties['hooks']) { $events = @($document.hooks.PSObject.Properties) }
    foreach ($event in $events) {
        foreach ($group in @($event.Value)) {
            foreach ($handler in @($group.hooks)) {
                foreach ($property in $handler.PSObject.Properties) {
                    if ($property.Name -notlike 'command*' -or $property.Value -isnot [string]) { continue }
                    $command = $property.Value.Replace('\','/').ToLowerInvariant()
                    $depends = @($retiringPaths | Where-Object { $command.Contains($_) }).Count -gt 0
                    if ($retiringPaths.Count -and ($depends -or
                        $command -match '(?i)\.codex/+bin/+bridgeforge\.exe|(?:^|/)\.bridgeforge-codex/(?!native-memory-sync/)|codex_memory_sync_hook')) {
                        throw "User Hook for $($event.Name) still references the retiring Forge runtime. Preserve the legacy installation."
                    }
                }
            }
        }
    }
    if ((Get-Sha256 -Path $Path) -cne $witness) { throw 'User Hooks changed while checking retirement dependencies.' }
    return @{path=$Path;hash=$witness}
}

function Install-DedicatedMemoryRuntime([string]$Repository, [string]$migrationProfile, $Identity) {
    $codex = Join-Path $migrationProfile '.codex'
    $destination = Join-Path $codex '.bridgeforge-codex/native-memory-sync/bin/bridgeforge-memory-hook.exe'
    Assert-NoReparseUnderProfile -Path $destination -UserProfile $migrationProfile
    $previousSource = $env:BRIDGEFORGE_BUILD_SOURCE_SHA
    $previousLock = $env:BRIDGEFORGE_BUILD_LOCK_SHA
    $env:BRIDGEFORGE_BUILD_SOURCE_SHA = [string]$Identity.source_tree_sha256
    $env:BRIDGEFORGE_BUILD_LOCK_SHA = [string]$Identity.lockfile_sha256
    Push-Location -LiteralPath $Repository
    try {
        & cargo build --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/memory -p bridgeforge-cli --bin bridgeforge-memory-hook | Out-Host
        if ($LASTEXITCODE -ne 0) { throw 'Native Memory Hook runtime build failed.' }
    } finally {
        Pop-Location
        $env:BRIDGEFORGE_BUILD_SOURCE_SHA = $previousSource
        $env:BRIDGEFORGE_BUILD_LOCK_SHA = $previousLock
    }
    $candidate = Join-Path $Repository '.build/bridgeforge/cargo/memory/release/bridgeforge-memory-hook.exe'
    $self = Invoke-CheckedJson $candidate @('self-test', '--json')
    if ($self.name -cne 'bridgeforge-memory-hook' -or $self.status -cne 'ok' -or
        $self.build_identity.source_tree_sha256 -cne $Identity.source_tree_sha256 -or
        $self.build_identity.lockfile_sha256 -cne $Identity.lockfile_sha256) { throw 'Memory runtime provenance differs from the prepared Forge product.' }
    $digest = Get-Sha256 -Path $candidate
    if (Test-Path -LiteralPath $destination) {
        if ((Get-Sha256 -Path $destination) -cne $digest) { throw 'Dedicated Memory runtime bytes differ; preserve the existing runtime.' }
    } else {
        $parent = Split-Path -Parent $destination
        New-Item -ItemType Directory -Path $parent -Force | Out-Null
        $stage = Join-Path $parent ('.bridgeforge-memory-stage-' + [Guid]::NewGuid().ToString('N'))
        try {
            Copy-Item -LiteralPath $candidate -Destination $stage
            if ((Get-Sha256 -Path $stage) -cne $digest) { throw 'Memory runtime changed while staging.' }
            [IO.File]::Move($stage, $destination)
        } finally {
            if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Force }
        }
    }
    $script:MemoryRuntimePath = $destination
    $receipt = Invoke-CheckedJson $destination @('memory-sync', 'retarget-hook', '--confirmed-migration', '--codex-home', $codex)
    if ($receipt.status -cne 'hook_scope_ready') { throw 'Native Memory Hook migration did not produce a ready receipt.' }
    $script:MemoryMigrationReceipt = $receipt
    return $receipt
}

function Invoke-UserScopeMigration {
    Assert-Windows
    if (!$SourceRepositoryRoot -or $PreparedSourceCommit -cnotmatch '^[0-9a-f]{40}$' -or
        (!$MigratedProjectRoot -and !$NoDependentProjects) -or ($MigratedProjectRoot -and $NoDependentProjects)) {
        throw 'Explicit user retirement requires prepared Forge source and all migrated projects, or -NoDependentProjects.'
    }
    $repository = [IO.Path]::GetFullPath($SourceRepositoryRoot).TrimEnd('\')
    $source = Get-RepositoryIdentity -Root $repository
    $upstream = ((Invoke-Git -WorkingDirectory $repository -Arguments @('rev-parse', 'refs/remotes/origin/main')) -join '').Trim()
    $root = ((Invoke-Git -WorkingDirectory $repository -Arguments @('rev-parse', '--show-toplevel')) -join '').Trim()
    if ($source.commit -cne $PreparedSourceCommit -or $upstream -cne $PreparedSourceCommit -or [IO.Path]::GetFullPath($root).TrimEnd('\') -ne $repository) {
        throw 'The prepared Forge source is outside its fixed repository and upstream commit.'
    }
    $manifestPath = Join-Path $repository $ManifestName
    $manifestHash = "sha256:$(Get-Sha256 -Path $manifestPath)"
    $product = Read-JsonFile -Path $manifestPath
    if ($product.schema_version -ne 2 -or $null -ne $product.PSObject.Properties['shared_skills']) { throw 'The product has not migrated to project-scoped workflows.' }
    $cli = Join-Path $repository '.build/bridgeforge/bin/bridgeforge.exe'
    $proof = Read-JsonFile -Path (Join-Path $repository '.build/bridgeforge/bin/build-receipt-cli.json')
    if ($proof.generated_asset_id -cne 'codex.bridgeforge-cli' -or $proof.binary_sha256 -cne "sha256:$(Get-Sha256 -Path $cli)") {
        throw 'The prepared Forge CLI is missing or differs from its build receipt.'
    }
    $self = Invoke-CheckedJson $cli @('self-test', '--json')
    $declared = @($product.project_skeleton.generated_assets | Where-Object { $_.id -eq 'codex.bridgeforge-cli' })
    if ($self.name -cne 'bridgeforge' -or $declared.Count -ne 1 -or
        $self.build_identity.source_tree_sha256 -cne $declared[0].source_tree_sha256 -or
        $self.build_identity.lockfile_sha256 -cne $declared[0].lockfile_sha256) { throw 'The prepared CLI differs from the current project distribution.' }
    foreach ($project in $MigratedProjectRoot) {
        $project = [IO.Path]::GetFullPath($project)
        $ready = Invoke-CheckedJson $cli @('check', 'baseline', '--root', $project)
        if ($ready.state -cne 'clean' -or $ready.model -notin @('template-only-v1', 'factory-template-v1') -or !$ready.runtime_verified) {
            throw "Project has no current project-scoped baseline: $project"
        }
    }

    $migrationProfile = [IO.Path]::GetFullPath($env:USERPROFILE)
    if ($env:CODEX_HOME -and [IO.Path]::GetFullPath($env:CODEX_HOME).TrimEnd('\', '/') -ne (Join-Path $migrationProfile '.codex')) {
        throw 'User Forge retirement requires the default Codex home; an alternate CODEX_HOME may still depend on it.'
    }
    if ($repository -eq (Join-Path $migrationProfile $CommandHomeName)) { throw 'The retiring user checkout cannot be the migration source.' }
    $mutex = Enter-UpdateMutex -UserProfile $migrationProfile
    try {
        $log = Join-Path $migrationProfile $OperationLogName
        Restore-InterruptedOperation -LogPath $log -UserProfile $migrationProfile
        Restore-CommandHomeOperation -UserProfile $migrationProfile
        $operation = [Guid]::NewGuid().ToString('N')
        $empty = [pscustomobject]@{platforms=[pscustomobject]@{codex=[pscustomobject]@{target='~/.codex/skills';skills=@()}}}
        $ledger = Join-Path $migrationProfile '.codex/bridgeforge-codex-managed.json'
        $plans = @()
        if (Test-Path -LiteralPath $ledger) {
            Assert-NoReparseUnderProfile -Path $ledger -UserProfile $migrationProfile
            $plans = @(New-UpdatePlan -Manifest $empty -UserProfile $migrationProfile -OperationId $operation -Commit $source.commit | Where-Object { $_.ledger_needs_update })
        }
        $retire = @(New-UserRetirementPlans -UserProfile $migrationProfile -Operation $operation)
        $hooks = Join-Path $migrationProfile '.codex/hooks.json'
        Assert-NoReparseUnderProfile -Path $hooks -UserProfile $migrationProfile
        $memory = $null
        if ($InstallMemoryRuntime -or ((Test-Path -LiteralPath $hooks) -and
            ([IO.File]::ReadAllText($hooks) -match 'bridgeforge-codex\.native-memory-sync|codex_memory_sync_hook'))) {
            $memory = Install-DedicatedMemoryRuntime $repository $migrationProfile $self.build_identity
        }
        $removedSkills = @($plans | ForEach-Object { $_.actions } | Where-Object { $_.kind -eq 'remove' } |
            ForEach-Object { @{kind='skill';target=$_.target} })
        $hookWitness = Get-UserHookRetirementWitness -Path $hooks -Retirements ($retire + $removedSkills)
        if ($proof.binary_sha256 -cne "sha256:$(Get-Sha256 -Path $cli)") { throw 'Prepared Forge CLI changed before user migration.' }
        $agents = New-UserAgentsPlan -RepositoryRoot $repository -UserProfile $migrationProfile -OperationId $operation -RustCliPlan @{target=$cli;needs_swap=$false}
        if ($plans.Count -or $retire.Count -or $agents.needs_swap) {
            Invoke-UpdateTransaction -RepositoryRoot $repository -Manifest $empty -Commit $source.commit -ManifestHash $manifestHash `
                -UserProfile $migrationProfile -LogPath $log -OperationId $operation -PlatformPlans $plans -UserAgentsPlan $agents -RetirementPlans $retire `
                -UserScope -UserHookWitness $hookWitness
        }
        [ordered]@{schema=1;status='user_scope_migrated';source_commit=$source.commit;project_count=@($MigratedProjectRoot | Where-Object { $_ }).Count;
            retired_recovery=@($retire | ForEach-Object {$_.backup});memory=$memory;cleanup_pending=$script:CleanupPending} | ConvertTo-Json -Depth 6 -Compress
    } finally { $mutex.ReleaseMutex(); $mutex.Dispose() }
}

try {
    if ($MigrateUserScope) { Invoke-UserScopeMigration }
    else { throw 'User-level Forge distribution is retired. Install project workflows from INSTALL.md, then explicitly migrate the user scope.' }
    exit 0
}
catch {
    if ($MigrateUserScope) {
        [ordered]@{schema=1;status='user_scope_incomplete';error=$_.Exception.Message;
            memory_runtime=$script:MemoryRuntimePath;memory_migration=$script:MemoryMigrationReceipt;
            cleanup_pending=$script:CleanupPending} | ConvertTo-Json -Depth 6 -Compress
    } else { Write-UpdateReceipt -Status "failed" -Mode "failed" -ErrorMessage $_.Exception.Message }
    Write-Error $_.Exception.Message
    exit 1
}
