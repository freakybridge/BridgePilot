param([string]$RepositoryRoot, [string]$Base)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2
$tokens=$null; $errors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'),[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw ($errors | Out-String) }
foreach ($f in $ast.FindAll({param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true)) {
    if ($f.Name -ne 'Invoke-Main') { . ([scriptblock]::Create($f.Extent.Text)) }
}
$CanonicalRemote='https://github.com/freakybridge/BridgePilot.git'
$CanonicalBranch='main'
$CommandHomeName='.bridgeforge-codex'
$script:BuildCounts=[ordered]@{source_reused=0;source_clones=0}
$profileRoot=Join-Path $Base 'profile'
$homeRoot=Join-Path $profileRoot $CommandHomeName
New-Item -ItemType Directory -Force -Path $homeRoot | Out-Null
$script:Head='a'*40
$script:RemoteHead=$script:Head
$script:Origin=$CanonicalRemote
$script:Dirty=$false
$script:Offline=$false
$script:Calls=@()
function Invoke-Git {
    param([string[]]$Arguments,[string]$WorkingDirectory)
    $script:Calls += ($Arguments -join ' ')
    switch ($Arguments[0]) {
        'ls-remote' { if ($script:Offline) { throw 'offline fixture' }; return "$($script:RemoteHead)`trefs/heads/main" }
        'config' { return $script:Origin }
        'rev-parse' { return $script:Head }
        'symbolic-ref' { return 'main' }
        'ls-files' { return @() }
        '-c' { if ($script:Dirty) { return ' M VERSION' }; return @() }
        default { throw "Unexpected Git operation: $Arguments" }
    }
}
function New-CanonicalClone { throw 'fixture clone requested' }
function Expect-Failure {
    param([scriptblock]$Action,[string]$Expected)
    try { & $Action | Out-Null } catch {
        if ($_.Exception.Message -notlike "*$Expected*") { throw }
        return
    }
    throw "Expected failure: $Expected"
}
$selected=Select-CanonicalSource -UserProfile $profileRoot
if ($selected.root -ne $homeRoot -or $selected.clone -or $selected.commit -ne $script:Head) { throw 'Incorrect reused source' }
if ($script:BuildCounts.source_reused -ne 1 -or @($script:Calls | Where-Object {$_ -like 'fetch*' -or $_ -like 'clone*'}).Count) { throw 'Matching source fetched or cloned' }
$script:RemoteHead='b'*40
Expect-Failure { Select-CanonicalSource -UserProfile $profileRoot } 'fixture clone requested'
$script:Dirty=$true
Expect-Failure { Select-CanonicalSource -UserProfile $profileRoot } 'local changes'
$script:Dirty=$false
$script:Origin='https://github.com/unowned/repository.git'
Expect-Failure { Select-CanonicalSource -UserProfile $profileRoot } 'origin is not canonical'
$script:Origin=$CanonicalRemote
$script:Offline=$true
Expect-Failure { Select-CanonicalSource -UserProfile $profileRoot } 'offline fixture'
$script:Offline=$false
$script:RemoteHead='invalid'
Expect-Failure { Select-CanonicalSource -UserProfile $profileRoot } 'remote probe'
Expect-Failure { Assert-Repository -Root $homeRoot -SkipFetch -ExpectedCommit ('b'*40) -InstalledHome $homeRoot } 'changed after the remote probe'
Expect-Failure { Assert-Repository -Root $homeRoot -SkipFetch -ExpectedCommit $script:Head -InstalledHome (Join-Path $profileRoot 'other') } 'exact managed home'
$script:Offline=$true
$script:Calls=@()
$resumed=Select-ResumeSource -UserProfile $profileRoot -ExpectedCommit $script:Head
if ($resumed.commit -ne $script:Head -or @($script:Calls | Where-Object {$_ -like 'ls-remote*' -or $_ -like 'fetch*' -or $_ -like 'clone*'}).Count) { throw 'Protocol resume queried upstream or changed the selected source' }
Expect-Failure { Select-ResumeSource -UserProfile $profileRoot -ExpectedCommit ('b'*40) } 'changed after the remote probe'
Expect-Failure { Select-ResumeSource -UserProfile $profileRoot -ExpectedCommit 'invalid' } 'receipt source commit'
$script:Dirty=$true
Expect-Failure { Select-ResumeSource -UserProfile $profileRoot -ExpectedCommit $script:Head } 'not clean'
$script:Dirty=$false
$script:Origin='https://github.com/unowned/repository.git'
Expect-Failure { Select-ResumeSource -UserProfile $profileRoot -ExpectedCommit $script:Head } 'origin is not'
Write-Output 'Matching source reuse, bounded protocol resume, offline, ownership, drift and exact-path gates passed.'
