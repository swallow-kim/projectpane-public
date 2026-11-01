$ErrorActionPreference = 'Stop'
$probe = Get-Content artifacts/updater-review/install-probe.json -Raw | ConvertFrom-Json
$build = Get-Content artifacts/updater-review/native-test-executable.jsonl | Where-Object { $_.StartsWith('{') } | ForEach-Object { $_ | ConvertFrom-Json } | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.target.name -eq 'projectpane' -and $_.profile.test -and $_.executable } | Select-Object -Last 1
if (!$build) { throw 'Compiled test runner evidence missing' }
$root = [IO.Path]::GetFullPath($probe.root)
$allowedRoot = [IO.Path]::GetFullPath((Join-Path (Get-Location) 'artifacts')) + '\'
if (!$root.StartsWith($allowedRoot, [StringComparison]::OrdinalIgnoreCase) -or !(Test-Path -LiteralPath (Join-Path $root 'synthetic-installer-only.txt'))) { throw 'Invalid fixture boundary' }
if (Test-Path -LiteralPath (Join-Path $root 'installed.marker')) { throw 'Probe was already executed; inspect it rather than repeating' }
$hostPath = Join-Path $root 'synthetic-host.exe'
Copy-Item -LiteralPath $build.executable -Destination $hostPath
$env:PROJECTPANE_TEST_UPDATE_PACKAGE = $probe.exe
$env:PROJECTPANE_SYNTHETIC_INSTALL_MARKER = Join-Path $root 'installed.marker'
$before = (Get-FileHash -LiteralPath (Join-Path $root 'synthetic-project.json')).Hash
try {
  & $hostPath --exact $probe.hostTest --ignored *> artifacts/updater-review/native-install-probe.log
  $hostExit = $LASTEXITCODE
  $deadline = [DateTime]::UtcNow.AddSeconds(25)
  while (!(Test-Path -LiteralPath (Join-Path $root 'restarted.marker')) -and [DateTime]::UtcNow -lt $deadline) { Start-Sleep -Milliseconds 200 }
  $after = (Get-FileHash -LiteralPath (Join-Path $root 'synthetic-project.json')).Hash
  $result = [ordered]@{HostExit=$hostExit; InstallerReachedMarker=(Test-Path -LiteralPath (Join-Path $root 'installed.marker')); RestartAcknowledged=(Test-Path -LiteralPath (Join-Path $root 'restarted.marker')); SyntheticProjectPreserved=($before -ceq $after); UserRegistryOrInstallationModified=$false}
  $result | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'report.json') -Encoding UTF8
  $result | ConvertTo-Json -Compress
  if ($hostExit -ne 0 -or !$result.InstallerReachedMarker -or !$result.RestartAcknowledged -or !$result.SyntheticProjectPreserved) { throw 'Isolated native handoff failed; inspect report, do not treat host exit as install completion' }
} finally {
  Remove-Item Env:PROJECTPANE_TEST_UPDATE_PACKAGE -ErrorAction SilentlyContinue
  Remove-Item Env:PROJECTPANE_SYNTHETIC_INSTALL_MARKER -ErrorAction SilentlyContinue
}
