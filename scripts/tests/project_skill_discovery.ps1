param([Parameter(Mandatory = $true)][string]$ProjectRoot)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$project = (Resolve-Path -LiteralPath $ProjectRoot).Path
$repository = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '../..')).Path
$fixture = Join-Path $repository ('.runtime/tests/project-workflows/codex-' + [Guid]::NewGuid().ToString('N'))
$codexHome = Join-Path $fixture '.codex'
New-Item -ItemType Directory -Path $codexHome -Force | Out-Null
$entry = (Get-Command codex.exe -CommandType Application).Source

$start = [Diagnostics.ProcessStartInfo]::new($entry)
$start.WorkingDirectory = $project
$start.ArgumentList.Add('app-server')
$start.ArgumentList.Add('--stdio')
$start.UseShellExecute = $false
$start.CreateNoWindow = $true
$start.RedirectStandardInput = $true
$start.RedirectStandardOutput = $true
$start.RedirectStandardError = $true
$start.StandardOutputEncoding = [Text.UTF8Encoding]::new($false)
$start.Environment['CODEX_HOME'] = $codexHome
$start.Environment['USERPROFILE'] = $fixture
$start.Environment['HOME'] = $fixture
$server = [Diagnostics.Process]::Start($start)
$stderr = $server.StandardError.ReadToEndAsync()
$timer = [Diagnostics.Stopwatch]::StartNew()

function Write-Request($Value) {
    $server.StandardInput.WriteLine(($Value | ConvertTo-Json -Depth 10 -Compress))
    $server.StandardInput.Flush()
}
function Read-Response([int]$Id) {
    while ($timer.ElapsedMilliseconds -lt 45000) {
        $line = $server.StandardOutput.ReadLineAsync()
        if (!$line.Wait([Math]::Max(1, 45000 - $timer.ElapsedMilliseconds)) -or $null -eq $line.Result) {
            break
        }
        $response = $line.Result | ConvertFrom-Json -AsHashtable
        if ($response['id'] -eq $Id) {
            if ($response['error']) { throw ($response['error'] | ConvertTo-Json -Compress) }
            return $response['result']
        }
    }
    throw "Codex did not return response $Id within the fixture deadline."
}

try {
    Write-Request @{jsonrpc='2.0';id=1;method='initialize';params=@{clientInfo=@{name='forge-project-scope-test';version='1.0.0'}}}
    $null = Read-Response 1
    Write-Request @{jsonrpc='2.0';method='initialized'}
    Write-Request @{jsonrpc='2.0';id=2;method='skills/list';params=@{cwds=@($project);forceReload=$true}}
    $found = Read-Response 2
    $expected = @((Get-ChildItem -LiteralPath (Join-Path $project '.agents/skills') -Directory) | Where-Object {
        $_.Name -in @('archive-scan','autopilot','cleanup','collab','confirm','create-worktree','debate','develop','escalate','explain','find-doc','focus','git-sync','plan','refactor-scan','spinoff','summary','sync-docs','todo')
    })
    if ($expected.Count -ne 19) { throw 'Fixture is missing Forge project Skills.' }
    foreach ($skill in $expected) {
        $path = Join-Path $skill.FullName 'SKILL.md'
        $entries = @($found.data[0].skills | Where-Object { $_.name -eq $skill.Name -and $_.enabled })
        if ($entries.Count -ne 1 -or $entries[0].scope -cne 'repo' -or
            [IO.Path]::GetFullPath($entries[0].path).Replace('\\?\','') -ne $path) {
            throw "Codex did not discover exactly one active project entry for $($skill.Name): $path"
        }
    }
    $receipt = [ordered]@{schema=1;status='passed';project=$project;skill_count=$expected.Count;skills=$found.data[0].skills;errors=$found.data[0].errors}
    $receipt | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $fixture 'receipt.json') -Encoding utf8NoBOM
    [ordered]@{status='passed';project=$project;skill_count=$expected.Count;receipt=(Join-Path $fixture 'receipt.json')} | ConvertTo-Json -Compress
} finally {
    $server.StandardInput.Close()
    if (!$server.WaitForExit(3000)) { $server.Kill($true); $server.WaitForExit() }
    [IO.File]::WriteAllText((Join-Path $fixture 'app-server.stderr.log'), $stderr.GetAwaiter().GetResult())
    $server.Dispose()
}
