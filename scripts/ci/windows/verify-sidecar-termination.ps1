# Windows の終了保証の検証（tasks.md 10.7）。**Job Object の経路を実測する唯一の段**である。
# 3 条件に加えて、孫ありで強制終了した後に次回起動の掃除が走ることも確かめる（Job Object が
# カーネルとして孫まで終了させるので、掃除が見る残存は 0 件になる。それでも掃除の記録行は出る）。
#
# **既知の窓（`job_windows.rs` の doc）**: `CreateProcess` と `AssignProcessToJobObject` の
# 間に子が孫を起動すると、その孫はジョブの外に残りうる。この段は `--spawn-grandchild` で
# 孫を起動直後に作らせるため、**その窓を実際に踏む唯一の検証である**。この段が赤くなった
# ときに最初に疑うべきはこの窓であり、マージン（数十 µs 対 数 ms）を実測できるのは CI だけである。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了エラーで段が落ちるようにする。抽出前は台本側が与えていた）。
$ErrorActionPreference = 'Stop'

$verify = "target\release\jxcel.exe"
$sidecarPath = "target\release\sidecar-smoke.exe"
if (-not (Test-Path $verify)) { throw "検証用の形が見つかりません: ${verify}（--features verification-triggers のビルドではない）" }
if (-not (Test-Path $sidecarPath)) { throw "8.1 の解決先の補助プロセスが見つかりません: $sidecarPath" }
$expected = (Resolve-Path $sidecarPath).Path
$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"

# 期待する実行ファイルに属する補助プロセスだけを数える（名前だけでなくパスで照合する）。
function Get-Sidecars {
  @(Get-Process -Name "sidecar-smoke" -ErrorAction SilentlyContinue |
    Where-Object { $_.Path -and ($_.Path -eq $expected) })
}

function Show-Sidecars {
  param([string]$Phase)
  $procs = Get-Sidecars
  foreach ($p in $procs) {
    $cmd = ""
    try { $cmd = (Get-CimInstance Win32_Process -Filter "ProcessId=$($p.Id)").CommandLine } catch { $cmd = "(取得できない)" }
    Write-Host "    pid=$($p.Id) path=$($p.Path) args=$cmd"
  }
  Write-Host "  ${Phase}: 補助プロセスの数: $($procs.Count)"
}

# 補助プロセス（と孫）が現れるまで待ち、**そのコマンドラインがこのアプリの識別子を
# --parent-pid に持つこと**まで確かめる（起動していないのに残存 0 で通る経路を作らない）。
function Wait-Started {
  param([int]$Want, [int]$AppPid, [int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  do {
    $procs = Get-Sidecars
    if ($procs.Count -ge $Want) {
      foreach ($p in $procs) {
        $cmd = ""
        try { $cmd = (Get-CimInstance Win32_Process -Filter "ProcessId=$($p.Id)").CommandLine } catch { $cmd = "" }
        if ($cmd -match "--parent-pid\s+$AppPid(\s|$)") { return $procs }
      }
    }
    Start-Sleep -Milliseconds 200
  } while ((Get-Date) -lt $deadline)
  return @()
}

function Wait-Zero {
  param([int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  do {
    if (@(Get-Sidecars).Count -eq 0) { return $true }
    Start-Sleep -Milliseconds 100
  } while ((Get-Date) -lt $deadline)
  return (@(Get-Sidecars).Count -eq 0)
}

function Confirm-Exit {
  param($Process, [string]$Phase, [int]$Seconds)
  if (-not $Process.WaitForExit($Seconds * 1000)) {
    throw "NG: ${Phase} アプリが $Seconds 秒以内に終了しませんでした"
  }
  if ($Process.ExitCode -ne 0) {
    throw "NG: ${Phase} アプリの終了コードが $($Process.ExitCode)（通常終了は 0 であるべき）"
  }
}

try {
  # ---- (a) 通常終了 ----
  Write-Host "== (a) 通常終了（sidecar:8000） =="
  $env:JXCEL_VERIFICATION_EXIT_AFTER_MS = "sidecar:8000"
  $p = Start-Process -FilePath $verify -PassThru
  $procs = @(Wait-Started -Want 1 -AppPid $p.Id -Seconds 60)
  if ($procs.Count -lt 1) { throw "NG: (a) 補助プロセス（--parent-pid=$($p.Id)）が 60 秒以内に現れませんでした" }
  Write-Host "(a) 補助プロセスが動作していることを観測した:"
  Show-Sidecars "(a) 起動を観測"
  Confirm-Exit -Process $p -Phase "(a)" -Seconds 60
  if (-not (Wait-Zero 15)) { Show-Sidecars "(a) 残存"; throw "NG: (a) 通常終了の後に補助プロセスが残っています" }
  Write-Host "(a) 通常終了の後の一覧（残存 0）:"
  Show-Sidecars "(a) 通常終了の後"
  Write-Host "OK: (a) 通常終了の後に補助プロセスは残らない（5.6 の shutdown_all → 3.3 の Job Object）"

  # ---- (b) 強制終了（本番の経路） ----
  Write-Host "== (b) 強制終了（Stop-Process -Force。本番の経路） =="
  $env:JXCEL_VERIFICATION_EXIT_AFTER_MS = "sidecar:600000"
  $p = Start-Process -FilePath $verify -PassThru
  $procs = @(Wait-Started -Want 1 -AppPid $p.Id -Seconds 60)
  if ($procs.Count -lt 1) { throw "NG: (b) 補助プロセス（--parent-pid=$($p.Id)）が 60 秒以内に現れませんでした" }
  Write-Host "(b) 強制終了の前の一覧:"
  Show-Sidecars "(b) 強制終了の前"
  $killed = $p.Id
  Write-Host "(b) アプリ（pid=${killed}）へ Stop-Process -Force を送る（終了処理は走らない）"
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Stop-Process -Id $p.Id -Force
  $p.WaitForExit(10000) | Out-Null
  if (-not (Wait-Zero 15)) { Show-Sidecars "(b) 残存"; throw "NG: (b) 強制終了の後に補助プロセスが残っています" }
  $sw.Stop()
  Write-Host "(b) 強制終了から約 $([int]$sw.ElapsedMilliseconds) ms で残存 0 になった"
  Write-Host "(b) 強制終了の後の一覧（残存 0）:"
  Show-Sidecars "(b) 強制終了の後"
  Write-Host "OK: (b) 強制終了の後に補助プロセスは残らない（Windows の機構は 3.3 の Job Object。"
  Write-Host "        カーネルが強制するため終了処理が走らない異常終了の後も有効）"

  # ---- (c) 孫あり・通常終了 ----
  Write-Host "== (c) 孫プロセスあり・通常終了（sidecar-grandchild:8000） =="
  $env:JXCEL_VERIFICATION_EXIT_AFTER_MS = "sidecar-grandchild:8000"
  $p = Start-Process -FilePath $verify -PassThru
  $procs = @(Wait-Started -Want 2 -AppPid $p.Id -Seconds 60)
  if ($procs.Count -lt 2) { throw "NG: (c) 補助プロセスと孫の 2 つ（--parent-pid=$($p.Id)）が 60 秒以内に現れませんでした（観測: $($procs.Count) 件）" }
  Write-Host "(c) 補助プロセスと孫が動作していることを観測した:"
  Show-Sidecars "(c) 起動を観測"
  Confirm-Exit -Process $p -Phase "(c)" -Seconds 60
  if (-not (Wait-Zero 15)) { Show-Sidecars "(c) 残存"; throw "NG: (c) 通常終了の後に補助プロセスが残っています（孫を含む）" }
  Write-Host "(c) 通常終了の後の一覧（残存 0）:"
  Show-Sidecars "(c) 通常終了の後"
  Write-Host "OK: (c) 孫を持つ補助プロセスも通常終了で残らない（3.3 の Job Object が孫まで届く）"

  # ---- (d) 補助: 孫あり・強制終了 → 次回起動の掃除 ----
  Write-Host "== (d) 補助: 孫あり・強制終了 → 次回起動の掃除（3.5） =="
  $env:JXCEL_VERIFICATION_EXIT_AFTER_MS = "sidecar-grandchild:600000"
  $p = Start-Process -FilePath $verify -PassThru
  $procs = @(Wait-Started -Want 2 -AppPid $p.Id -Seconds 60)
  if ($procs.Count -lt 2) { throw "NG: (d) 補助プロセスと孫の 2 つ（--parent-pid=$($p.Id)）が 60 秒以内に現れませんでした（観測: $($procs.Count) 件）" }
  Write-Host "(d) 強制終了の前の一覧（補助プロセスと孫）:"
  Show-Sidecars "(d) 強制終了の前"
  $sw = [Diagnostics.Stopwatch]::StartNew()
  Stop-Process -Id $p.Id -Force
  $p.WaitForExit(10000) | Out-Null
  if (-not (Wait-Zero 15)) {
    Show-Sidecars "(d) 残存"
    throw "NG: (d) 強制終了の後に孫が残っています（Job Object の KILL_ON_JOB_CLOSE が孫まで届いていない）"
  }
  $sw.Stop()
  Write-Host "(d) 強制終了から約 $([int]$sw.ElapsedMilliseconds) ms で孫を含めて残存 0 になった（Job Object はカーネルが強制する）"
  Write-Host "(d) 強制終了の後の一覧（残存 0）:"
  Show-Sidecars "(d) 強制終了の後"
  # 次回起動で起動時の残留掃除（3.5）を走らせ、その記録行を出す（Job Object が既に 0 に
  # した場合でも掃除そのものは走る。掃除の経路が生きていることを記録でも確かめる）。
  $before = 0
  if (Test-Path $record) { $before = @(Get-Content -LiteralPath $record).Count }
  $env:JXCEL_VERIFICATION_EXIT_AFTER_MS = "exit:2500"
  $q = Start-Process -FilePath $verify -PassThru
  Confirm-Exit -Process $q -Phase "(d) 次回起動" -Seconds 60
  $sweepLine = $null
  if (Test-Path $record) {
    $sweepLine = @(Get-Content -LiteralPath $record | Select-Object -Skip $before) |
      Where-Object { $_ -match "残留プロセスの掃除で" } | Select-Object -Last 1
  }
  if (-not $sweepLine) { throw "NG: (d) 次回起動の掃除の記録（残留プロセスの掃除で N 件を終了した）が現れませんでした: $record" }
  Write-Host "(d) 記録（5.1 の起動時の掃除）: $sweepLine"
  if (-not (Wait-Zero 15)) { Show-Sidecars "(d) 残存"; throw "NG: (d) 次回起動の掃除の後も補助プロセスが残っています" }
  Write-Host "(d) 掃除の後の一覧（残存 0）:"
  Show-Sidecars "(d) 掃除の後"
  Write-Host "OK: (d) 孫ありで強制終了しても残存 0（Job Object）であり、次回起動の掃除も走る（3.5）"
} finally {
  Remove-Item Env:\JXCEL_VERIFICATION_EXIT_AFTER_MS -ErrorAction SilentlyContinue
  Get-Process -Name "jxcel" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
  Get-Sidecars | Stop-Process -Force -ErrorAction SilentlyContinue
  Write-Host "検証（後始末）: 残存するアプリと補助プロセスを終了した"
}
# 後始末の完了を待ってから主張する（`Stop-Process` は非同期であり、直後の列挙が
# 間に合わないことがある。偽の失敗を作らない）。
$cleanDeadline = (Get-Date).AddSeconds(10)
while ((Get-Date) -lt $cleanDeadline -and @(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue).Count -ne 0) {
  Start-Sleep -Milliseconds 100
}
if (@(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue).Count -ne 0) { throw "後始末の後に常駐しているインスタンスが残っています" }
if (-not (Wait-Zero 10)) { Show-Sidecars "後始末"; throw "後始末の後に補助プロセスが残っています" }
Write-Host "検証（後始末）: 残存プロセス数=0（アプリ・補助プロセスとも）"
Write-Host "OK: Windows: 通常終了・強制終了・孫ありの 3 条件すべてで残存補助プロセスが 0 であることを検証した"
