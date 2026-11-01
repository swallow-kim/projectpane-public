#requires -Version 7.2
<#
Opt-in only, after the operator confirms availability:
pwsh -NoProfile -STA -File scripts/windows-ime-check.ps1 -Interactive `
  -Executable D:\path\projectpane.exe -OutputDirectory C:\Example\ProjectPane\artifacts\ime-unique
No installation, input injection, focus manipulation, or IME/settings changes.
Raw artifacts contain fixture paths and typed text; use only the supplied fixture.
#>
param(
  [Parameter(Mandatory)][string]$Executable,
  [Parameter(Mandatory)][string]$OutputDirectory,
  [switch]$Interactive,
  [ValidateRange(1, 14)][int]$StartAt = 1,
  [ValidateRange(10, 180)][int]$StepTimeoutSeconds = 180,
  [switch]$IncludeTextFile
)
Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or -not $Interactive) { throw 'windows_and_explicit_interactive_required' }
if ([Threading.Thread]::CurrentThread.ApartmentState -ne 'STA') { throw 'sta_required' }
$Executable = (Resolve-Path -LiteralPath $Executable).Path
$node = (Get-Command node -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
$repo = Split-Path $PSScriptRoot
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$allowed = [IO.Path]::GetFullPath((Join-Path $repo 'artifacts')) + [IO.Path]::DirectorySeparatorChar
if (-not $OutputDirectory.StartsWith($allowed, [StringComparison]::OrdinalIgnoreCase)) { throw 'output_requires_repository_artifacts' }
& git -C $repo check-ignore --quiet -- $OutputDirectory
if ($LASTEXITCODE -ne 0 -or (Test-Path -LiteralPath $OutputDirectory)) { throw 'output_requires_new_ignored_directory' }
function Assert-NoReparse([string]$Path) {
  $current = [IO.Path]::GetFullPath($Path)
  while ($current) {
    if ((Test-Path -LiteralPath $current) -and ((Get-Item -Force -LiteralPath $current).Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'reparse_boundary' }
    $current = Split-Path $current -Parent
  }
}
Assert-NoReparse $OutputDirectory
$names = @('projectpane', 'projectpane-read-worker', [IO.Path]::GetFileNameWithoutExtension($Executable)) | Select-Object -Unique
if (@(Get-Process -Name $names -ErrorAction SilentlyContinue).Count) { throw 'existing_app_preserved' }
$token = [Guid]::NewGuid().ToString('N')
$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$root = Join-Path $tempBase "ProjectPane-ime-$token"
$marker = Join-Path $root '.ime-owner'
$fixture = Join-Path $root 'fixture'
$child = Join-Path $fixture '한글'
$owned = [Collections.Generic.List[object]]::new()
$primary = $null; $observer = $null; $form = $null; $timer = $null
$state = @{ seq = 0; index = $StartAt - 1; active = $false; pending = $false; finished = $false; error = $null; deadline = [DateTime]::UtcNow.AddSeconds(45) }
$report = [ordered]@{ status = 'NOT_RUN'; ui01Passed = $false; p2Complete = $false; fixture = $fixture; retainedRoot = $root;
  requestedRange = [ordered]@{ start = $StartAt; end = 14 };
  appSha256 = (Get-FileHash -LiteralPath $Executable -Algorithm SHA256).Hash;
  normalAppExit = $false; zeroRemaining = $false; safeCleanup = $false; reason = $null }
function Register-Process($Process) {
  $null = $Process.Handle
  $entry = [pscustomobject]@{ Process = $Process; StartTicks = $Process.StartTime.ToUniversalTime().Ticks }
  $owned.Add($entry)
  return $entry
}
function Is-Alive($Entry) {
  $Entry.Process.Refresh()
  return (-not $Entry.Process.HasExited -and $Entry.Process.StartTime.ToUniversalTime().Ticks -eq $Entry.StartTicks)
}
function Track-Descendants {
  $rows = @(Get-CimInstance Win32_Process -OperationTimeoutSec 2)
  $known = @{}
  foreach ($entry in $owned) { $known[$entry.Process.Id] = $entry }
  do {
    $added = $false
    foreach ($row in $rows) {
      if ($known.ContainsKey([int]$row.ProcessId) -or -not $known.ContainsKey([int]$row.ParentProcessId)) { continue }
      $parent = $known[[int]$row.ParentProcessId]; $created = $row.CreationDate.ToUniversalTime().Ticks
      $end = if (Is-Alive $parent) { [long]::MaxValue } else { $parent.Process.ExitTime.ToUniversalTime().Ticks }
      if ($created -lt $parent.StartTicks -or $created -gt $end) { continue }
      $process = Get-Process -Id $row.ProcessId -ErrorAction SilentlyContinue
      if ($null -ne $process -and [Math]::Abs($process.StartTime.ToUniversalTime().Ticks - $created) -lt 10) {
        $known[$process.Id] = Register-Process $process; $added = $true
      }
    }
  } while ($added)
}
function Send-Command([string]$Action, [bool]$Physical = $false, [bool]$Outcome = $false) {
  $state.seq++
  $id = if ($state.index -lt $steps.Count) { $steps[$state.index].Id } else { '' }
  $command = @{ token = $token; seq = $state.seq; action = $Action; id = $id; physical = $Physical; outcome = $Outcome }
  $path = Join-Path $OutputDirectory 'command.json'
  $command | ConvertTo-Json | Set-Content -LiteralPath "$path.tmp" -Encoding utf8NoBOM
  Move-Item -LiteralPath "$path.tmp" -Destination $path -Force
}
$actions = @('composing-enter', 'committed-enter', 'composing-ctrl-l', 'composing-f6', 'composing-escape', 'committed-ctrl-l', 'committed-f6')
$steps = @(
  foreach ($target in @('address', 'filter')) {
    foreach ($action in $actions) {
      $field = if ($target -eq 'address') { '주소' } else { '이름 필터' }
      $prepare = if ($target -eq 'address') {
        "준비: 이름 필터를 비우고 부모 경로로 직접 이동하여 목록이 안정될 때까지 기다리세요. 주소에 자식 경로의 마지막 '한글'을 제외한 부분을 입력/붙여넣기 하세요."
      } else {
        "준비: 이름 필터를 비우고 부모 경로로 직접 이동하세요. 목록에 '한글'과 'other-folder'가 보이는지 확인하세요. '패널 1 이름 필터'를 직접 클릭하세요. 필터가 없으면 단계를 건너뛰지 말고 취소하고 미구현 상태를 보고하세요."
      }
      $operation = switch ($action) {
        'composing-enter' { "한글 IME로 '한글'을 직접 입력하고 마지막 글자 조합 중 Enter를 한 번 누르세요. 기대: 조합만 확정, 폴더 탐색 없음. 필터에서는 확정된 이름에 맞게 목록이 바뀔 수 있습니다. 두 번째 Enter는 누르지 마세요." }
        'committed-enter' { "한글 IME로 '한글'을 직접 입력하고 오른쪽 화살표로 확정하세요. 1초 기다린 뒤 Enter를 직접 누르세요. 기대: 주소에서는 자식 폴더로 이동하여 IME-child-proof.txt가 보임. 필터에서는 주소/폴더는 유지되고 '한글' 항목만 보이며 other-folder는 숨겨짐." }
        'composing-ctrl-l' { "한글 IME로 '한글'을 직접 입력하고 조합 중 Ctrl+L을 누르세요. 기대: 현재 입력칸에서 조합 유지, 주소 전체 선택/포커스 이동/탐색 없음. 1초 기다린 뒤 오른쪽 화살표로 확정하세요." }
        'composing-f6' { "한글 IME로 '한글'을 직접 입력하고 조합 중 F6을 누르세요. 기대: 현재 입력칸에서 조합 유지, 주소 전체 선택/포커스 이동/탐색 없음. 1초 기다린 뒤 오른쪽 화살표로 확정하세요." }
        'composing-escape' { "한글 IME로 '한글'을 직접 입력하고 조합 중 Escape를 누르세요. 기대: IME 조합 취소, 폴더 탐색 없음. 1초 기다리세요. 조합 종료 이벤트가 없거나 실제 취소를 확인할 수 없으면 결과 확인란을 체크하지 마세요." }
        'committed-ctrl-l' { "한글 IME로 '한글'을 직접 입력하고 오른쪽 화살표로 확정하세요. 1초 기다리고 End, Ctrl+L을 누르세요. 기대: 주소 입력칸으로 포커스 이동 및 주소 전체 선택." }
        'committed-f6' { "2패널 또는 4패널 상태에서 패널 1의 대상 입력칸을 직접 클릭하세요. 한글 IME로 '한글'을 직접 입력하고 오른쪽 화살표로 확정하세요. 1초 기다리고 End, F6만 누르세요 (Shift/Ctrl/Alt 없이). 기대: 다음 보이는 패널인 패널 2의 주소 입력칸으로 포커스 이동 및 전체 선택. 패널 2의 주소가 비어 있으면 빈 주소에 커서가 있는 것이 정상입니다. 패널 1에 머물거나 숨겨진 패널로 이동하면 실패입니다. 다음 단계는 다시 패널 1에서 준비하세요." }
      }
      @{ Id = "${target}:$action"; Text = "대상: 패널 1 $field / $action`r`n$prepare`r`n`r`n수행: $operation`r`n`r`n시작 이후 이 단계의 한글 조합 전체를 직접 수행해야 합니다. 키만 누르면 INCOMPLETE입니다. 마지막 동작 후 1초 기다린 다음 완료를 누르세요." }
    }
  }
)
function Show-Step {
  $form.Text = "ProjectPane — IME 요청 범위 $StartAt–14 (이전 결과 미포함)"
  $instructions.Text = "각 단계: [시작] → 앱에서 직접 수행 → 1초 대기 → 이 창에서 확인 후 [완료].`r`n조합 도중에는 이 창을 클릭하지 마세요. 준비/수행 각각 최대 $StepTimeoutSeconds 초. 취소/창 닫기 = NOT_RUN.`r`n`r`n$($steps[$state.index].Text)`r`n`r`n자식 경로: $child`r`n부모 경로: $fixture`r`n`r`n기록은 수동 CDP 관찰이며 물리 입력 여부는 사용자 확인입니다. 조합 순서/결과/대상별 상태를 기록합니다. 전체 UI-01 PASS 또는 P2 완료를 판정하지 않습니다."
  $physical.Checked = $false; $outcome.Checked = $false
  $begin.Enabled = $true; $complete.Enabled = $false
  $state.deadline = [DateTime]::UtcNow.AddSeconds($StepTimeoutSeconds)
}
try {
  Assert-NoReparse $root
  if (Test-Path -LiteralPath $root) { throw 'owned_root_exists' }
  [IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
  [IO.Directory]::CreateDirectory($child) | Out-Null
  [IO.Directory]::CreateDirectory((Join-Path $fixture 'other-folder')) | Out-Null
  [IO.File]::WriteAllText($marker, $token)
  [IO.File]::WriteAllText((Join-Path $child 'IME-child-proof.txt'), 'Owned Korean IME navigation evidence.')
  if ($IncludeTextFile) { [IO.File]::WriteAllText((Join-Path $fixture '한글 메모.txt'), 'Optional owned UTF-8 fixture.') }
  @{ root = $root; token = $token; executable = $Executable } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'owner.json')
  $listener = [Net.Sockets.TcpListener]::new([Net.IPAddress]::Loopback, 0)
  $listener.Start(); $port = $listener.LocalEndpoint.Port; $listener.Stop()
  # Recheck immediately before launch; refuse to attach to an existing app instance.
  if (@(Get-Process -Name $names -ErrorAction SilentlyContinue).Count) { throw 'existing_app_preserved' }
  $info = [Diagnostics.ProcessStartInfo]::new($Executable)
  $info.UseShellExecute = $false; $info.WindowStyle = 'Normal'
  $info.ArgumentList.Add('--folder'); $info.ArgumentList.Add($fixture)
  $info.Environment['PROJECTPANE_DATA_DIR'] = Join-Path $root 'data'
  $info.Environment['WEBVIEW2_USER_DATA_FOLDER'] = Join-Path $root 'webview-profile'
  $info.Environment['WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS'] = "--remote-debugging-address=127.0.0.1 --remote-debugging-port=$port"
  $primary = Register-Process ([Diagnostics.Process]::Start($info))
  $readyClock = [Diagnostics.Stopwatch]::StartNew()
  do {
    if (-not (Is-Alive $primary)) { throw 'primary_exited_before_ready' }
    Track-Descendants
    $connections = @(Get-NetTCPConnection -LocalPort $port -State Listen -ErrorAction SilentlyContinue)
    if ($connections.Count) {
      Track-Descendants
      foreach ($connection in $connections) {
        if ($connection.LocalAddress -notin @('127.0.0.1', '::1')) { throw 'cdp_not_loopback' }
        if (-not @($owned | Where-Object { $_.Process.Id -eq $connection.OwningProcess -and (Is-Alive $_) }).Count) { throw 'cdp_owner_mismatch' }
      }
      break
    }
    Start-Sleep -Milliseconds 200
  } while ($readyClock.Elapsed.TotalSeconds -lt 30)
  if (-not $connections.Count) { throw 'cdp_listener_timeout' }
  $sidecar = [Diagnostics.ProcessStartInfo]::new($node)
  $sidecar.UseShellExecute = $false; $sidecar.CreateNoWindow = $true; $sidecar.WindowStyle = 'Hidden'; $sidecar.WorkingDirectory = $repo
  $sidecar.RedirectStandardError = $true
  foreach ($argument in @((Join-Path $PSScriptRoot 'windows-ime-observe.mjs'), '--port', "$port", '--root', $root, '--token', $token, '--output', $OutputDirectory, '--step-timeout', "$StepTimeoutSeconds", '--start-at', "$StartAt")) { $sidecar.ArgumentList.Add($argument) }
  $observer = Register-Process ([Diagnostics.Process]::Start($sidecar))
  Add-Type -AssemblyName System.Windows.Forms
  $form = [Windows.Forms.Form]::new(); $form.Text = 'ProjectPane — 물리 한글 IME 관찰 (opt-in)'
  $form.Width = 850; $form.Height = 610; $form.StartPosition = 'CenterScreen'
  $instructions = [Windows.Forms.TextBox]::new(); $instructions.Multiline = $true; $instructions.ReadOnly = $true
  $instructions.SetBounds(15, 15, 800, 370); $instructions.ScrollBars = 'Vertical'; $instructions.Text = '관찰기 준비 중…'
  $physical = [Windows.Forms.CheckBox]::new(); $physical.Text = '내가 실제 키보드와 한글 IME로 수행했습니다 (자동 입력 아님)'; $physical.SetBounds(15, 395, 790, 25)
  $outcome = [Windows.Forms.CheckBox]::new(); $outcome.Text = '이 단계의 기대 결과를 직접 확인했습니다 (아니면 체크 해제)'; $outcome.SetBounds(15, 425, 790, 25)
  $begin = [Windows.Forms.Button]::new(); $begin.Text = '시작'; $begin.SetBounds(15, 465, 140, 40); $begin.Enabled = $false
  $complete = [Windows.Forms.Button]::new(); $complete.Text = '완료 / 결과 기록'; $complete.SetBounds(175, 465, 190, 40); $complete.Enabled = $false
  $cancel = [Windows.Forms.Button]::new(); $cancel.Text = '취소 (NOT_RUN)'; $cancel.SetBounds(390, 465, 180, 40)
  $progress = [Windows.Forms.Label]::new(); $progress.SetBounds(15, 520, 790, 30)
  $form.Controls.AddRange(@($instructions, $physical, $outcome, $begin, $complete, $cancel, $progress))
  $begin.Add_Click({ try { Send-Command 'start'; $state.active = $true; $begin.Enabled = $false; $state.deadline = [DateTime]::UtcNow.AddSeconds($StepTimeoutSeconds) } catch { $state.error = $_; $form.Close() } })
  $complete.Add_Click({ try { Send-Command 'complete' $physical.Checked $outcome.Checked; $state.pending = $true; $complete.Enabled = $false } catch { $state.error = $_; $form.Close() } })
  $cancel.Add_Click({ $form.Close() })
  $timer = [Windows.Forms.Timer]::new(); $timer.Interval = 250
  $timer.Add_Tick({
    try {
      if ([DateTime]::UtcNow -gt $state.deadline) { $report.reason = 'operator_timeout'; $form.Close(); return }
      $progress.Text = "단계 $($state.index + 1)/$($steps.Count) · 남은 시간 $([Math]::Max(0, [int]($state.deadline - [DateTime]::UtcNow).TotalSeconds))초"
      if (-not (Is-Alive $primary)) { throw 'app_exited_during_observation' }
      $path = Join-Path $OutputDirectory 'observer-state.json'
      if (-not (Test-Path -LiteralPath $path)) { if (-not (Is-Alive $observer)) { throw 'observer_exited_before_ready' }; return }
      $status = Get-Content -Raw -LiteralPath $path | ConvertFrom-Json -AsHashtable
      if ($status['activity']) {
        $activity = $status['activity']
        $progress.Text += " · 관측 입력 $($activity.trustedInputEvents)회 / 조합 $($activity.trustedCompositionEvents)회 / 단계 밖 $($activity.outsideStepInputEvents)회"
      }
      if ($status['finished']) {
        $state.finished = $true; $report.status = $status.status
        $report.reason = if ($status['reason'] -is [string]) { $status['reason'] } else { $null }
        $form.Close(); return
      }
      if (-not (Is-Alive $observer)) { throw 'observer_exited' }
      if ($status['ready'] -and -not $state.active -and -not $begin.Enabled) { Show-Step }
      if ($state.active -and -not $state.pending -and $status['active'] -eq $steps[$state.index].Id) { $complete.Enabled = $true }
      if ($state.pending -and $status.steps[$state.index].status -ne 'NOT_RUN') {
        $state.index++; $state.pending = $false; $state.active = $false
        if ($state.index -lt $steps.Count) { Show-Step }
      }
    } catch { $state.error = $_; $form.Close() }
  })
  $timer.Start(); $null = $form.ShowDialog(); $timer.Stop()
  if ($null -ne $state.error) { throw $state.error }
} catch { $report.status = 'FAIL'; $report.reason = $_.ToString() }
finally {
  if ($null -ne $timer) { $timer.Stop(); $timer.Dispose() }
  if ($null -ne $form) { $form.Dispose() }
  try {
    if ($null -ne $observer -and (Is-Alive $observer)) {
      if (-not $state.finished) { Send-Command 'cancel' }
      $null = $observer.Process.WaitForExit(10000)
    }
    if ($null -ne $observer -and -not (Is-Alive $observer)) {
      $observer.Process.StandardError.ReadToEnd() | Set-Content -LiteralPath (Join-Path $OutputDirectory 'observer-stderr.txt')
    }
    Track-Descendants
    if ($null -ne $primary -and (Is-Alive $primary)) { $null = $primary.Process.CloseMainWindow() }
    $closing = [Diagnostics.Stopwatch]::StartNew()
    do {
      Track-Descendants
      $remaining = @($owned | Where-Object { Is-Alive $_ })
      if ($remaining.Count -eq 0) { break }
      Start-Sleep -Milliseconds 200
    } while ($closing.Elapsed.TotalSeconds -lt 10)
    $report.zeroRemaining = $remaining.Count -eq 0
    $report.normalAppExit = $null -ne $primary -and -not (Is-Alive $primary) -and $primary.Process.ExitCode -eq 0
    if (Test-Path -LiteralPath $OutputDirectory) {
      $owned | ForEach-Object { @{ pid = $_.Process.Id; startTicks = $_.StartTicks } } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $OutputDirectory 'process-identities.json')
    }
    # Preserve every incomplete, cancelled, failed, or unclean run for diagnosis.
    if ($report.status -eq 'OBSERVED' -and $report.zeroRemaining -and $report.normalAppExit -and $observer.Process.ExitCode -eq 0) {
      Assert-NoReparse $root
      Assert-NoReparse $OutputDirectory
      if ([IO.Path]::GetFullPath($root) -cne (Join-Path $tempBase "ProjectPane-ime-$token") -or [IO.File]::ReadAllText($marker) -cne $token) { throw 'cleanup_owner_boundary' }
      $tree = @(Get-ChildItem -LiteralPath $root -Recurse -Force)
      if (@($tree | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }).Count) { throw 'cleanup_reparse_rejected' }
      $logs = Join-Path $root 'data/foundation/logs'
      if (Test-Path -LiteralPath $logs) { Copy-Item -LiteralPath $logs -Destination (Join-Path $OutputDirectory 'native-logs') -Recurse }
      Remove-Item -LiteralPath $root -Recurse -Force
      $report.safeCleanup = $true; $report.retainedRoot = $null
    }
    if (-not $report.zeroRemaining) { $report.status = 'FAIL'; $report.reason = 'owned_processes_remaining_preserved_no_force_kill' }
    if ($report.status -eq 'OBSERVED' -and -not $report.normalAppExit) { $report.status = 'FAIL'; $report.reason = 'app_exit_not_normal' }
  } catch { $report.status = 'FAIL'; $report.reason = $_.ToString() }
  if (Test-Path -LiteralPath $OutputDirectory) { $report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $OutputDirectory 'harness-report.json') }
  foreach ($entry in $owned) { $entry.Process.Dispose() }
}
Write-Output "IME observation: $($report.status); requested range: $StartAt–14 (no previous outcomes imported); artifacts: $OutputDirectory; retained root: $($report.retainedRoot)"
if ($report.status -eq 'FAIL') { exit 1 }
