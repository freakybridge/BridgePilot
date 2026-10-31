param([Parameter(Mandatory)][string]$Cli, [string]$Sample='D:\Quant\BridgePilot\.runtime\git-sync-perf\replay', [string]$Variant='baseline', [ValidateSet('plan','preview','ordinary','release-chain','publish','invalidation')][string]$Scenario='plan')
$ErrorActionPreference='Stop'
$root='D:\Quant\BridgePilot\.runtime\git-sync-perf'
if (-not ([IO.Path]::GetFullPath($Sample)).StartsWith($root + '\',[StringComparison]::OrdinalIgnoreCase)) { throw 'Only isolated samples are allowed' }
$output=Join-Path $root "results/$Variant"
New-Item -ItemType Directory -Force -Path $output | Out-Null
$utf8=[Text.UTF8Encoding]::new($false)
$env:BRIDGEFORGE_PROFILE='1'
$null='{}'|ConvertFrom-Json
$null=@{schema=1}|ConvertTo-Json
$remote=(& git.exe --no-optional-locks -C $Sample remote get-url origin) -join ''
if($LASTEXITCODE -ne 0 -or -not ([IO.Path]::GetFullPath($remote)).StartsWith($root + '\',[StringComparison]::OrdinalIgnoreCase)){throw 'Benchmark remote must be local to its isolated root'}
$pipeline=[Diagnostics.Stopwatch]::StartNew()
function Measure-Cli([string]$Name,[string[]]$Arguments,[int]$ExpectedExit=0) {
    $start=[Diagnostics.Stopwatch]::StartNew()
    $psi=[Diagnostics.ProcessStartInfo]::new($Cli)
    $psi.WorkingDirectory=$Sample
    $psi.UseShellExecute=$false
    $psi.CreateNoWindow=$true
    $psi.RedirectStandardOutput=$true
    $psi.RedirectStandardError=$true
    foreach($arg in $Arguments){$psi.ArgumentList.Add($arg)}
    $p=[Diagnostics.Process]::Start($psi)
    $stdout=$p.StandardOutput.ReadToEndAsync()
    $stderr=$p.StandardError.ReadToEndAsync()
    if(-not $p.WaitForExit(900000)){$p.Kill($true);throw 'Isolated CLI exceeded 15 minute bound'}
    $raw=$stdout.GetAwaiter().GetResult()
    $errorText=$stderr.GetAwaiter().GetResult()
    $start.Stop()
    [IO.File]::WriteAllText((Join-Path $output "$Name.stdout.json"),$raw,$utf8)
    [IO.File]::WriteAllText((Join-Path $output "$Name.stderr.txt"),$errorText,$utf8)
    $receipt=if($raw.Trim()){$raw|ConvertFrom-Json}else{$null}
    $row=[ordered]@{variant=$Variant;scenario=$Name;wall_ms=$start.Elapsed.TotalMilliseconds;cli_ms=($receipt.profile.cli_elapsed_us/1000.0);launch_exit_emit_ms=($start.Elapsed.TotalMilliseconds-$receipt.profile.cli_elapsed_us/1000.0);exit=$p.ExitCode;status=$receipt.status;profile=$receipt.profile;timings_ms=$receipt.timings_ms}
    [IO.File]::WriteAllText((Join-Path $output "$Name.measurement.json"),($row|ConvertTo-Json -Depth 30),$utf8)
    Write-Host ([pscustomobject]$row | Select-Object variant,scenario,wall_ms,cli_ms,exit,status | ConvertTo-Json -Compress)
    if($p.ExitCode -ne $ExpectedExit){throw "$Name failed: $errorText"}
    return $receipt
}
if($Scenario -eq 'plan') { $null=Measure-Cli 'repair-plan' @('git-sync','--repair-baseline-plan');exit }
if($Scenario -eq 'preview') { $null=Measure-Cli 'preview' @('git-sync','--release-preview','--message','fix: isolated measured release');exit }
if($Scenario -eq 'ordinary') { $null=Measure-Cli 'ordinary' @('git-sync','--message','chore: isolated measured sync');exit }
if($Scenario -eq 'invalidation') {
    $p=Measure-Cli 'drift-seed' @('git-sync','--release-preview','--message','fix: isolated measured drift')
    $packages=[ordered]@{}
    foreach($id in $p.affected_packages){$packages[$id]=@{level='patch';compatibility='compatible';reason='Isolated drift probe'}}
    $analysis=@{schema=1;fingerprint=$p.fingerprint;product=@{level='patch';compatibility='compatible';reason='Isolated drift probe'};packages=$packages;checks=@()}
    $analysisFile=Join-Path $Sample '.runtime/performance-analysis.json'
    [IO.File]::WriteAllText($analysisFile,($analysis|ConvertTo-Json -Depth 30),$utf8)
    $readme=Join-Path $Sample 'README.md'
    $config=Join-Path $Sample '.codex/bridgeforge-version.json'
    $readmeBytes=[IO.File]::ReadAllBytes($readme)
    $configBytes=[IO.File]::ReadAllBytes($config)
    $oldEnvironment=$env:BRIDGEFORGE_PERF_DRIFT
    foreach($kind in @('file','configuration','environment')) {
        try {
            if($kind -eq 'file'){[IO.File]::AppendAllText($readme,"`nIsolated drift probe.`n",$utf8)}
            if($kind -eq 'configuration'){$cfg=[Text.Encoding]::UTF8.GetString($configBytes)|ConvertFrom-Json;$cfg|Add-Member -NotePropertyName build_inputs -NotePropertyValue @('performance-recipe.json') -Force;[IO.File]::WriteAllText($config,($cfg|ConvertTo-Json -Depth 100),$utf8)}
            if($kind -eq 'environment'){$env:BRIDGEFORGE_PERF_DRIFT='changed'}
            $null=Measure-Cli "drift-$kind" @('git-sync','--release-preview','--message','fix: isolated measured drift','--release-analysis-file','.runtime/performance-analysis.json') 2
            if((Get-Content (Join-Path $output "drift-$kind.stderr.txt") -Raw) -notmatch 'stale'){throw 'Drift was not rejected for stale analysis'}
        } finally {
            [IO.File]::WriteAllBytes($readme,$readmeBytes)
            [IO.File]::WriteAllBytes($config,$configBytes)
            $env:BRIDGEFORGE_PERF_DRIFT=$oldEnvironment
        }
    }
    exit
}
if($Scenario -eq 'release-chain') {
    $p=Measure-Cli 'repair-plan' @('git-sync','--repair-baseline-plan')
    $null=Measure-Cli 'repair-apply' @('git-sync','--repair-baseline-apply','--repair-baseline-fingerprint',$p.fingerprint,'--confirmed-choice')
}
$p=Measure-Cli 'unbound-preview' @('git-sync','--release-preview','--message','fix: isolated measured release')
$packages=[ordered]@{}
foreach($id in $p.affected_packages){$packages[$id]=[ordered]@{level='patch';compatibility='compatible';reason='Isolated performance replay of preserved source changes; not a business release decision.'}}
$analysis=[ordered]@{schema=1;fingerprint=$p.fingerprint;product=[ordered]@{level='patch';compatibility='compatible';reason='Isolated performance replay only.'};packages=$packages;checks=@()}
$analysisFile=Join-Path $Sample '.runtime/performance-analysis.json'
New-Item -ItemType Directory -Force -Path (Split-Path $analysisFile) | Out-Null
[IO.File]::WriteAllText($analysisFile,($analysis|ConvertTo-Json -Depth 30),$utf8)
$null=Measure-Cli 'bound-preview' @('git-sync','--release-preview','--message','fix: isolated measured release','--release-analysis-file','.runtime/performance-analysis.json')
$null=Measure-Cli 'release' @('git-sync','--release','--message','fix: isolated measured release','--release-analysis-file','.runtime/performance-analysis.json')
$null=Measure-Cli 'no-change-release' @('git-sync','--release','--message','chore: isolated unchanged release')
$null=Measure-Cli 'ordinary-clean' @('git-sync','--message','chore: isolated measured sync')
[IO.File]::AppendAllText((Join-Path $Sample 'README.md'),"`nIsolated ordinary-sync performance probe.`n",$utf8)
$null=Measure-Cli 'ordinary-dirty' @('git-sync','--message','docs: isolated measured ordinary change')
$pipeline.Stop()
[IO.File]::WriteAllText((Join-Path $output 'pipeline.json'),(@{scenario=$Scenario;wall_ms=$pipeline.Elapsed.TotalMilliseconds;agent_wait_between_stages_ms=0;fixture_preparation_included=$false;remote=$remote;cli_sha256=(Get-FileHash -LiteralPath $Cli).Hash}|ConvertTo-Json),$utf8)
