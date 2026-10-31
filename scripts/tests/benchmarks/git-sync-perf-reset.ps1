param([string]$Sample='D:\Quant\BridgePilot\.runtime\git-sync-perf\replay')
$ErrorActionPreference='Stop'
$root='D:\Quant\BridgePilot\.runtime\git-sync-perf'
$samplePath=[IO.Path]::GetFullPath($Sample)
$seed=[IO.Path]::GetFullPath((Join-Path $root 'seed.git'))
foreach($path in @($samplePath,$seed)) { if(-not $path.StartsWith($root+'\',[StringComparison]::OrdinalIgnoreCase)){throw 'Outside isolated workspace'} }
function Git([string]$At,[string[]]$Arguments){$out=& git.exe --no-optional-locks -C $At @Arguments;if($LASTEXITCODE -ne 0){throw "Fixture Git failed: $Arguments"};$out}
$head=Git $samplePath @('rev-parse','HEAD')
$base='44d7e871d6bc6e22bcda0049884fa640d5159135'
if((Git $samplePath @('branch','--show-current')) -ne 'perf'){throw 'Unexpected fixture branch'}
if((Git $samplePath @('remote','get-url','origin')) -ne $seed){throw 'Unexpected fixture remote'}
if((Git $samplePath @('status','--porcelain=v1'))){throw 'Preserve unexpected fixture dirt'}
# Keep result objects reachable before restoring only the disposable test refs.
Git $samplePath @('update-ref','refs/perf-results/baseline',$head)
Git $seed @('update-ref','refs/perf-results/baseline',$head)
Git $samplePath @('update-ref','refs/heads/perf',$base,$head)
Git $seed @('update-ref','refs/heads/perf',$base,$head)
Git $samplePath @('update-ref','refs/remotes/origin/perf',$base)
Git $samplePath @('read-tree','--reset','-u','3077a6d5623250a6ee08ea701c760f2d4ddb78f7')
Git $samplePath @('read-tree','HEAD')
& tar -xf (Join-Path $root 'untracked.tar') -C $samplePath
if($LASTEXITCODE -ne 0){throw 'Fixture untracked restoration failed'}
Copy-Item -LiteralPath (Join-Path $root 'gap-config.json') -Destination (Join-Path $samplePath '.codex/bridgeforge-version.json')
# Match the original snapshot's warmed Git stat state before timing. This does
# not stage content; otherwise sync's ordinary status refresh can change the
# conservative full analysis fingerprint after two read-only previews.
& git.exe -C $samplePath status --porcelain=v1 -uall | Out-Null
if($LASTEXITCODE -ne 0){throw 'Fixture status refresh failed'}
if((Get-Content (Join-Path $samplePath 'VERSION') -Raw).Trim() -ne '1.54.0'){throw 'Fixture version mismatch'}
[pscustomobject]@{restored_head=$base;preserved_result=$head;sample=$samplePath}|ConvertTo-Json -Compress
