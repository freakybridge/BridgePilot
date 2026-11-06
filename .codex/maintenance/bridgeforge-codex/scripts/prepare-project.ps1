#requires -Version 5.1
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)][string]$SourceRoot,
    [Parameter(Mandatory = $true)][ValidatePattern('^[0-9a-f]{40}$')][string]$SourceCommit
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Read-Git([string[]]$Arguments) {
    $result = & git -C $SourceRoot @Arguments
    if ($LASTEXITCODE -ne 0) { throw "Cannot verify official Forge source ($($Arguments[0]))." }
    return ($result -join "`n").Trim()
}

$SourceRoot = [IO.Path]::GetFullPath($SourceRoot).TrimEnd('\', '/')
$actualRoot = Read-Git @('rev-parse', '--show-toplevel')
if ([IO.Path]::GetFullPath($actualRoot) -ne $SourceRoot) { throw 'SourceRoot must be the official repository root.' }
$origin = Read-Git @('remote', 'get-url', 'origin')
if ($origin -notmatch '^https://github\.com/freakybridge/BridgePilot(?:\.git)?/?$') { throw 'Forge source has an unexpected origin.' }
$head = Read-Git @('rev-parse', 'HEAD')
$upstream = Read-Git @('rev-parse', 'refs/remotes/origin/main')
if ($head -cne $SourceCommit -or $upstream -cne $SourceCommit -or (Read-Git @('status', '--porcelain=v1'))) {
    throw 'Forge preparation requires the clean commit fixed by the official source fetch.'
}

Push-Location -LiteralPath $SourceRoot
try {
    $output = & cargo run --locked --release --manifest-path Cargo.toml --target-dir .build/bridgeforge/cargo/local --bin bridgeforge-factory -- build-assets --factory --project-root .
    if ($LASTEXITCODE -ne 0) { throw 'Managed Forge artifact preparation failed.' }
    $prepared = ($output -join "`n") | ConvertFrom-Json
} finally { Pop-Location }

$receipt = Join-Path $SourceRoot '.build/bridgeforge/public-ready.json'
$binary = Join-Path $SourceRoot '.build/bridgeforge/bin/bridgeforge.exe'
if ($prepared.status -notin @('prepared', 'current') -or
    [IO.Path]::GetFullPath([string]$prepared.public_artifacts).Replace('\\?\', '') -ne $receipt -or
    -not (Test-Path -LiteralPath $receipt -PathType Leaf) -or
    -not (Test-Path -LiteralPath $binary -PathType Leaf) -or
    (Read-Git @('rev-parse', 'HEAD')) -cne $SourceCommit -or (Read-Git @('status', '--porcelain=v1'))) {
    throw 'Forge preparation receipt or fixed source changed.'
}

[ordered]@{ schema=1; status='prepared'; source_commit=$SourceCommit; source_root=$SourceRoot; cli=$binary; public_artifacts=$receipt } | ConvertTo-Json -Compress
