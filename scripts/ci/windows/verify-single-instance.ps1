# Windows の段（tasks.md 10.5）。ウィンドウは `EnumWindows` で数え（所有者 pid と**題名の
# 完全一致**で絞る — 単一インスタンスのプラグインは同じ pid に可視の隠しウィンドウ
# `com.jxcel.app-siw` を作るため、題名の一致が要る）、閉鎖要求は `PostMessage(WM_CLOSE)` で
# 起こす（同一利用者・同一セッションなので許可は要らない）。**(a)/(b) は配布物（NSIS で /S
# 導入済みの実行ファイル）、(c) は検証用の形**で測る（10.4 と同じ前提）。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了エラーで段が落ちるようにする。抽出前は台本側が与えていた）。
$ErrorActionPreference = 'Stop'

$title = "jxcel"
$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
New-Item -ItemType Directory -Force -Path (Split-Path $record) | Out-Null
Write-Host "診断記録: $record"

# 導入先は 1.5 の段と同じ前提（既定の installMode は currentUser）。見つからない場合は
# 1.5 の段と同じ探索に退避する（導入先が変わっても偽の失敗をしない）。
$installed = Join-Path $env:LOCALAPPDATA "jxcel\jxcel.exe"
if (-not (Test-Path $installed)) {
  $found = Get-ChildItem -Path $env:LOCALAPPDATA -Filter "jxcel.exe" -Recurse -Depth 3 -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($found) { $installed = $found.FullName }
}
if (-not (Test-Path $installed)) { throw "導入された実行ファイルが見つかりません: $installed" }
$verify = "target/release/jxcel.exe"
if (-not (Test-Path $verify)) { throw "検証用の形が見つかりません: $verify" }
$document = Join-Path $env:RUNNER_TEMP "jxcel-10-5-document.txt"
Set-Content -Path $document -Value "jxcel 10.5 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）" -Encoding UTF8

# 配布物に検証専用の識別子が入っていないこと（5.4 の片付けの規約）と、検証用の形には
# 入っていること。バイト列を Latin-1 で文字列にして部分一致を見る（`findstr` は
# バイナリに対して信頼できない）。
function Test-ByteString {
  param([string]$Path, [string]$Needle)
  $text = [System.Text.Encoding]::Latin1.GetString([System.IO.File]::ReadAllBytes($Path))
  return $text.Contains($Needle)
}
if (Test-ByteString -Path $installed -Needle "JXCEL_VERIFICATION") {
  throw "配布物の実行ファイルに検証専用の識別子が入っています: $installed"
}
if (-not (Test-ByteString -Path $verify -Needle "JXCEL_VERIFICATION_DENY_CLOSE")) {
  throw "検証用の形に JXCEL_VERIFICATION_DENY_CLOSE がありません（--features verification-triggers のビルドではない）"
}
Write-Host "検証 (片付け): 配布物に JXCEL_VERIFICATION は無く（バイト列の照合で 0 件）、検証用の形は JXCEL_VERIFICATION_DENY_CLOSE を持つ"

# ウィンドウの列挙と閉鎖要求の送信（1 つの型にまとめる）。**here-string を使わない** —
# YAML の中では終端 `"@` を行頭に置けず、PowerShell の here-string が成立しない。
$windowApi = @(
  'using System;',
  'using System.Collections.Generic;',
  'using System.Runtime.InteropServices;',
  'using System.Text;',
  '',
  'public static class JxcelWindowApi {',
  '  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);',
  '  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);',
  '  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);',
  '  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextLength(IntPtr hWnd);',
  '  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);',
  '  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);',
  '  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);',
  '  public const uint WM_CLOSE = 0x0010;',
  '  public static List<IntPtr> WindowsOf(int ownerPid, string title) {',
  '    var result = new List<IntPtr>();',
  '    EnumWindows(delegate(IntPtr hWnd, IntPtr lParam) {',
  '      uint owner;',
  '      GetWindowThreadProcessId(hWnd, out owner);',
  '      if (owner != (uint)ownerPid) { return true; }',
  '      int length = GetWindowTextLength(hWnd);',
  '      if (length <= 0) { return true; }',
  '      var text = new StringBuilder(length + 1);',
  '      GetWindowText(hWnd, text, text.Capacity);',
  '      if (text.ToString() == title) { result.Add(hWnd); }',
  '      return true;',
  '    }, IntPtr.Zero);',
  '    return result;',
  '  }',
  '}'
) -join "`n"
Add-Type -TypeDefinition $windowApi

function Get-HandleLabel {
  # **`[object]` で受けて `@()` で包む。** PowerShell は 1 要素の配列を関数の戻り値で
  # スカラー（`IntPtr`）へ開いてしまうため、`IEnumerable` で受けると
  # 「1 枚だけのとき」に引数の型変換で落ちる（実測: Windows のメニュー検査。
  # `Wait-Windows` が 1 枚を返した時点で
  # `Cannot convert the "655462" value of type "System.IntPtr" to type
  # "System.Collections.IEnumerable"`）。
  param([object]$Handles)
  return ((@($Handles) | ForEach-Object { '0x{0:x}' -f $_.ToInt64() }) -join ' ')
}
function Get-InstanceCount {
  return @(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue).Count
}
function Get-RecordLines {
  param([int]$Skip = 0)
  if (-not (Test-Path $record)) { return @() }
  $lines = @(Get-Content -Path $record -Encoding UTF8 -ErrorAction SilentlyContinue)
  if ($lines.Count -le $Skip) { return @() }
  return $lines[$Skip..($lines.Count - 1)]
}
function Find-RecordLine {
  param([int]$Skip, [string]$Pattern, [int]$Seconds = 10)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    $hit = Get-RecordLines -Skip $Skip | Where-Object { $_ -match $Pattern } | Select-Object -First 1
    if ($hit) { return $hit }
    Start-Sleep -Milliseconds 200
  }
  return $null
}
function Get-RecordCount {
  param([int]$Skip, [string]$Pattern)
  return @(Get-RecordLines -Skip $Skip | Where-Object { $_ -match $Pattern }).Count
}
function Wait-Windows {
  param([int]$OwnerPid, [int]$AtLeast, [int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    $handles = @([JxcelWindowApi]::WindowsOf($OwnerPid, $title))
    if ($handles.Count -ge $AtLeast) { return $handles }
    Start-Sleep -Milliseconds 100
  }
  return $null
}
function Wait-WindowsGone {
  param([int]$OwnerPid, [int]$Seconds)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    if (@([JxcelWindowApi]::WindowsOf($OwnerPid, $title)).Count -eq 0) { return $true }
    Start-Sleep -Milliseconds 100
  }
  return $false
}
function Wait-Exit {
  param([System.Diagnostics.Process]$Target, [int]$Seconds)
  return $Target.WaitForExit($Seconds * 1000)
}
function Stop-Instance {
  param([System.Diagnostics.Process]$Target)
  if (-not $Target.HasExited) { $Target.Kill() }
  $Target.WaitForExit(10000) | Out-Null
}
function Send-Close {
  param([System.Collections.IEnumerable]$Handles)
  foreach ($handle in $Handles) {
    [JxcelWindowApi]::PostMessage($handle, [JxcelWindowApi]::WM_CLOSE, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null
  }
}

Write-Host "検証 (a)(b): 1 つ目として配布物を起動する（引数なし → ドキュメントを関連付けないウィンドウ）"
$phaseAB = @(Get-RecordLines).Count
$first = Start-Process -FilePath $installed -PassThru
if (-not (Wait-Windows -OwnerPid $first.Id -AtLeast 1 -Seconds 60)) {
  Stop-Instance $first
  throw "1 つ目のウィンドウが現れませんでした"
}
if (-not (Find-RecordLine -Skip $phaseAB -Pattern "ウィンドウを開いた: label=empty-1 " -Seconds 10)) {
  Stop-Instance $first
  throw "1 つ目のウィンドウが label=empty-1 として記録に現れません"
}
$firstHandles = @([JxcelWindowApi]::WindowsOf($first.Id, $title))
Write-Host "検証 (a): 1 つ目の起動: pid=$($first.Id) ウィンドウ数=$($firstHandles.Count) ハンドル=$(Get-HandleLabel -Handles $firstHandles)"

Write-Host "検証 (a)(b): 同じ配布物を再度実行する（ドキュメント位置を渡す → 単一インスタンスが引数を引き渡す）"
$second = Start-Process -FilePath $installed -ArgumentList $document -PassThru
if (-not (Wait-Exit -Target $second -Seconds 60)) {
  Stop-Instance $second
  throw "2 つ目の起動が終了しません（2 つ目が常駐している。単一インスタンスが成立していない）"
}
if ($second.ExitCode -ne 0) {
  throw "2 つ目の起動が終了コード $($second.ExitCode) で終わった（0 であるべき）"
}
Write-Host "検証 (a): 2 つ目の起動は終了コード 0 で終わった（常駐しない）"
if ($first.HasExited) { throw "2 つ目の起動の後に 1 つ目のプロセスが終了した" }
if ((Get-InstanceCount) -ne 1) {
  throw "常駐しているインスタンスが $(Get-InstanceCount) 件になった（1 件であるべき。2 つ目は常駐してはならない）"
}
Write-Host "検証 (a): 常駐インスタンス数=$(Get-InstanceCount)（2 つ目は常駐していない）"

$secondHandles = Wait-Windows -OwnerPid $first.Id -AtLeast ($firstHandles.Count + 1) -Seconds 60
if (-not $secondHandles) {
  throw "2 つ目の起動の後、ウィンドウ数が $($firstHandles.Count + 1) 以上になりませんでした（引き継いだ側がウィンドウを提示していない）"
}
foreach ($handle in $firstHandles) {
  if (-not [JxcelWindowApi]::IsWindow($handle)) {
    throw "ドキュメントを開いたときに既存のウィンドウが閉じた（既存のウィンドウを閉じてはならない）"
  }
}
$handover = Find-RecordLine -Skip $phaseAB -Pattern "二重起動を引き継ぎました.*ドキュメント要求 " -Seconds 10
if (-not $handover) { throw "引き継ぎの行（二重起動を引き継ぎました … ドキュメント要求 …）が記録に現れません" }
$docLabelLine = Find-RecordLine -Skip $phaseAB -Pattern "ウィンドウを開いた: label=doc-" -Seconds 10
if (-not $docLabelLine) { throw "新しいウィンドウが label=doc-*（ドキュメント付き）として記録に現れません" }
$docLabel = (($docLabelLine -split "label=")[1] -split " ")[0]
Write-Host "検証 (b): ウィンドウ数=$($secondHandles.Count)（1 つ目=$($firstHandles.Count) から 1 増えた） 既存のハンドル=$(Get-HandleLabel -Handles $firstHandles) は残っている"
Write-Host "検証 (b): 新しいウィンドウのラベル=${docLabel}（doc-* ＝ ドキュメント付き。ラベルは記録が出す）"
Write-Host "検証 (b): 記録（引き継ぎ）: $handover"

Write-Host "検証 (a): 引数なしで再度実行する（既にあるウィンドウを前面に出すだけで、新しいウィンドウを作らない）"
$phaseArgless = @(Get-RecordLines).Count
$third = Start-Process -FilePath $installed -PassThru
if (-not (Wait-Exit -Target $third -Seconds 60)) {
  Stop-Instance $third
  throw "引数なしの 2 つ目の起動が終了しません"
}
if ($third.ExitCode -ne 0) {
  throw "引数なしの 2 つ目の起動が終了コード $($third.ExitCode) で終わった（0 であるべき）"
}
$argless = Find-RecordLine -Skip $phaseArgless -Pattern "二重起動を引き継ぎました.*ドキュメント要求なし" -Seconds 10
if (-not $argless) { throw "引数なしの引き継ぎの行（… ドキュメント要求なし）が記録に現れません" }
$settle = (Get-Date).AddSeconds(3)
while ((Get-Date) -lt $settle) {
  if (@([JxcelWindowApi]::WindowsOf($first.Id, $title)).Count -ne $secondHandles.Count) {
    throw "引数なしの 2 つ目の起動でウィンドウ数が $($secondHandles.Count) から変わった（新しいウィンドウを作ってはならない）"
  }
  Start-Sleep -Milliseconds 200
}
Write-Host "検証 (a): 引数なしの 2 つ目の起動も終了コード 0 で終わり、ウィンドウ数=$($secondHandles.Count) のまま変わらない"
Write-Host "検証 (a): 記録（引数なしの引き継ぎ）: $argless"

Write-Host "検証: 配布物のインスタンスを片付ける"
Stop-Instance $first
if (-not (Wait-WindowsGone -OwnerPid $first.Id -Seconds 60)) {
  throw "配布物のウィンドウが消えませんでした"
}

$phaseDeny = @(Get-RecordLines).Count
Write-Host "検証 (c): 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE=doc-1 → doc-1 の終了を拒否する委譲先）"
$env:JXCEL_VERIFICATION_DENY_CLOSE = "doc-1"
try {
  $denied = Start-Process -FilePath $verify -ArgumentList $document -PassThru
} finally {
  Remove-Item Env:\JXCEL_VERIFICATION_DENY_CLOSE -ErrorAction SilentlyContinue
}
if (-not (Wait-Windows -OwnerPid $denied.Id -AtLeast 1 -Seconds 60)) {
  Stop-Instance $denied
  throw "検証用の形のウィンドウが現れませんでした"
}
if (-not (Find-RecordLine -Skip $phaseDeny -Pattern "ウィンドウを開いた: label=doc-1 " -Seconds 10)) {
  Stop-Instance $denied
  throw "検証用の形が doc-1 というラベルのウィンドウを開いていません（拒否の対象が存在しない）"
}
if (-not (Find-RecordLine -Skip $phaseDeny -Pattern "初回描画が成立した: label=doc-1 " -Seconds 20) -and
    -not (Find-RecordLine -Skip $phaseDeny -Pattern "初回描画は成立したがソフトウェアラスタライザ経由である: label=doc-1 " -Seconds 1) -and
    -not (Find-RecordLine -Skip $phaseDeny -Pattern "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label=doc-1 " -Seconds 1)) {
  Stop-Instance $denied
  throw "初回描画の成立行が現れません（購読が張られたことを確認できない）"
}
$denyHandles = @([JxcelWindowApi]::WindowsOf($denied.Id, $title))
Write-Host "検証 (c): ウィンドウ数=$($denyHandles.Count) ハンドル=$(Get-HandleLabel -Handles $denyHandles)"
Write-Host "検証 (c): WM_CLOSE を送る（拒否される委譲先）"
Send-Close -Handles $denyHandles
$denyLine = Find-RecordLine -Skip $phaseDeny -Pattern "can_close_window: 呼び出し元ウィンドウ = doc-1 / 判定 = 拒否" -Seconds 10
if (-not $denyLine) {
  Stop-Instance $denied
  throw "拒否の往復（can_close_window の判定 = 拒否）が記録に現れません"
}
$settleDeny = (Get-Date).AddSeconds(3)
while ((Get-Date) -lt $settleDeny) {
  $live = @([JxcelWindowApi]::WindowsOf($denied.Id, $title))
  if ($live.Count -lt $denyHandles.Count) {
    throw "拒否されたはずのウィンドウが閉じた（委譲先の拒否が効いていない）"
  }
  foreach ($handle in $denyHandles) {
    if (-not [JxcelWindowApi]::IsWindow($handle)) {
      throw "拒否されたはずのウィンドウ $(Get-HandleLabel -Handles @($handle)) が消えた"
    }
  }
  if ($denied.HasExited) { throw "拒否された後にプロセス（pid=$($denied.Id)）が終了した" }
  Start-Sleep -Milliseconds 200
}
$roundTrips = Get-RecordCount -Skip $phaseDeny -Pattern "can_close_window: 呼び出し元ウィンドウ = doc-1 "
if ($roundTrips -ne 1) {
  throw "1 回の終了要求に対する拒否の往復が $roundTrips 回だった（1 回であるべき。7.6 の実測）"
}
Write-Host "検証 (c): 拒否 — ウィンドウは残り（$(Get-HandleLabel -Handles $denyHandles)）、プロセス pid=$($denied.Id) も生存"
Write-Host "検証 (c): 記録（拒否の往復）: $denyLine"
Write-Host "検証 (c): 拒否の往復の回数=${roundTrips}（1 回の終了要求につき 1 回）"

Stop-Instance $denied
if (-not (Wait-WindowsGone -OwnerPid $denied.Id -Seconds 60)) {
  throw "拒否の検証の後始末でウィンドウが消えませんでした"
}

$phaseAllow = @(Get-RecordLines).Count
Write-Host "検証 (c): 対照 — 検証用の形を起動する（JXCEL_VERIFICATION_DENY_CLOSE なし → 常に許可する委譲先）"
$allowed = Start-Process -FilePath $verify -ArgumentList $document -PassThru
if (-not (Wait-Windows -OwnerPid $allowed.Id -AtLeast 1 -Seconds 60)) {
  Stop-Instance $allowed
  throw "対照のウィンドウが現れませんでした"
}
if (-not (Find-RecordLine -Skip $phaseAllow -Pattern "初回描画が成立した: label=doc-1 " -Seconds 20) -and
    -not (Find-RecordLine -Skip $phaseAllow -Pattern "初回描画は成立したがソフトウェアラスタライザ経由である: label=doc-1 " -Seconds 1) -and
    -not (Find-RecordLine -Skip $phaseAllow -Pattern "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label=doc-1 " -Seconds 1)) {
  Stop-Instance $allowed
  throw "対照の初回描画の成立行が現れません"
}
$allowHandles = @([JxcelWindowApi]::WindowsOf($allowed.Id, $title))
Write-Host "検証 (c): WM_CLOSE を送る（拒否しない委譲先）"
Send-Close -Handles $allowHandles
if (-not (Wait-WindowsGone -OwnerPid $allowed.Id -Seconds 60)) {
  throw "拒否しない委譲先で WM_CLOSE を送ったが、ウィンドウが消えませんでした（許可が効いていない）"
}
if (-not (Wait-Exit -Target $allowed -Seconds 60)) {
  throw "最後のウィンドウが閉じた後にプロセスが終了しませんでした（要件 2.8）"
}
if ($allowed.ExitCode -ne 0) {
  throw "対照のプロセスが終了コード $($allowed.ExitCode) で終わった（0 であるべき）"
}
$allowLine = Find-RecordLine -Skip $phaseAllow -Pattern "判定 = 許可" -Seconds 5
Write-Host "検証 (c): 対照 — ウィンドウは消え（$(Get-HandleLabel -Handles $allowHandles)）、プロセスは終了コード 0 で終わった"
Write-Host "検証 (c): 記録（許可の往復）: $allowLine"

if ((Get-InstanceCount) -ne 0) {
  throw "後始末の後に常駐しているインスタンスが $(Get-InstanceCount) 件あります"
}
Write-Host "検証 (後始末): 常駐インスタンス数=0（残存プロセスなし）"
Write-Host "OK: 単一インスタンス化と複数ウィンドウの 3 経路（(a) 再実行が常駐せずウィンドウが増える / (b) 新しいウィンドウが doc-* で既存が閉じない / (c) 拒否では閉じず許可では閉じる）を検証した"
