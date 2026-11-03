[CmdletBinding()]
param([Parameter(Mandatory=$true)][string]$RepositoryRoot,
      [Parameter(Mandatory=$true)][string]$ProfileRoot,
      [Parameter(Mandatory=$true)][string]$PreparedCli)
$ErrorActionPreference='Stop'
$parseTokens=$null; $parseErrors=$null
$ast=[System.Management.Automation.Language.Parser]::ParseFile((Join-Path $RepositoryRoot 'scripts/bridgeforge_codex_shared_update.ps1'),[ref]$parseTokens,[ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
foreach ($function in $ast.FindAll({param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst]},$true)) {
    if ($function.Name -ne 'Invoke-Main') { . ([scriptblock]::Create($function.Extent.Text)) }
}
$script:PhaseTimings=[ordered]@{}
$identity=Get-RustCliReuseIdentity -Binary $PreparedCli -RepositoryRoot $RepositoryRoot
if (-not $identity) { throw 'Actual new CLI was rejected by updater capability and source checks.' }
$binRoot=Join-Path $ProfileRoot '.codex/bin'
New-Item -ItemType Directory -Path $binRoot -Force | Out-Null
$operation=[Guid]::NewGuid().ToString('N')
$target=Join-Path $binRoot 'bridgeforge.exe'
$stage=Join-Path $binRoot ".bridgeforge-stage-$operation.exe"
$backup=Join-Path $binRoot ".bridgeforge-backup-$operation.exe"
if (Test-Path -LiteralPath $target) { throw 'Isolated updater target must initially be absent.' }
Copy-Item -LiteralPath $PreparedCli -Destination $stage
$plan=[ordered]@{target=$target;stage=$stage;backup=$backup;had_original=$false;needs_swap=$true;status='staged';cache_identity=$identity;binary_sha256=(Get-Sha256 -Path $stage)}
Prepare-PublicArtifacts -Plan $plan -RepositoryRoot $RepositoryRoot -UserProfile $ProfileRoot
Assert-RustCliState -Plan $plan -RepositoryRoot $RepositoryRoot
Install-RustCli -Plan $plan
Assert-RustCliState -Plan $plan -RepositoryRoot $RepositoryRoot -Installed
$self=(& $target self-test --json | Out-String) | ConvertFrom-Json
if (@($self.capabilities) -notcontains 'template-only-v1' -or [int]$self.build_identity.protocol -ne 1) {
    throw 'Updater installed an unproven bootstrap rather than the prepared public CLI.'
}
$result=[ordered]@{schema=1;status='passed';scope='isolated updater component; no user installation';
    target=$target;binary_sha256=(Get-Sha256 -Path $target);source_identity=$identity;compiled_identity=$self.build_identity;
    prepared_receipt=$script:PublicArtifactsReceipt.receipt}
$result | ConvertTo-Json -Depth 12
