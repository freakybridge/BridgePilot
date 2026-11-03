#requires -Version 5.1
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$tokens = $null; $parseErrors = $null
$entry = Join-Path $repoRoot 'scripts/bridgeforge_codex_shared_update.ps1'
$ast = [Management.Automation.Language.Parser]::ParseFile($entry, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
$definition = $ast.Find({ param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Get-ForgeProductVersion' }, $true)
if (-not $definition) { throw 'Forge version resolver missing.' }
. ([scriptblock]::Create($definition.Extent.Text))
$expected = [regex]::Match([IO.File]::ReadAllText((Join-Path $repoRoot 'source/forge/cli/Cargo.toml')), '(?m)^version = "(?<v>[^"]+)"').Groups['v'].Value
if ((Get-ForgeProductVersion $repoRoot) -ne $expected) { throw 'Native Forge version resolver mismatch.' }
$root = Join-Path $repoRoot ('.runtime/tests/version-governance/powershell-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path (Join-Path $root 'source/cli') -Force
[IO.File]::WriteAllText((Join-Path $root 'VERSION'), '1.0.0')
[IO.File]::WriteAllText((Join-Path $root 'Cargo.toml'), "[workspace.package]`nversion='16.1.0'`n")
[IO.File]::WriteAllText((Join-Path $root 'source/cli/Cargo.toml'), "[package]`nname='bridgeforge-cli'`nversion.workspace=true`n")
if ((Get-ForgeProductVersion $root) -ne '16.1.0') { throw 'Legacy inherited native version mismatch.' }
[IO.File]::WriteAllText((Join-Path $root 'source/cli/Cargo.toml'), "[package]`nname='bridgeforge-cli'`nversion='17.0.0'`n")
if ((Get-ForgeProductVersion $root) -ne '17.0.0') { throw 'Explicit version confused with BP VERSION.' }
[IO.File]::WriteAllText((Join-Path $root 'source/cli/Cargo.toml'), "[package]`nname='bridgeforge-cli'`nversion='invalid'`n")
$rejected = $false
try { $null = Get-ForgeProductVersion $root } catch { $rejected = $true }
if (-not $rejected) { throw 'Invalid version was accepted.' }
$updater = Join-Path $repoRoot 'source/orchestrator/skills/bridgepilot/scripts/bridgepilot_update.ps1'
$null = [Management.Automation.Language.Parser]::ParseFile($updater, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw ($parseErrors | Out-String) }
Write-Output 'VERSION_GOVERNANCE_POWERSHELL passed=5'
