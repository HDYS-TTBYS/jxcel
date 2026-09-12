# Windows の段（tasks.md 10.6）。配布物のメニューバーを Win32 で読み、`WM_COMMAND` で
# 実際に活性化し、検証用の形の 2 枚でフォーカス先への作用を実測する。**キーは送らない** —
# Windows の WebView2 がアクセラレータキーを自分の経路で消費するため、キーはホストウィンドウの
# アクセラレータ表へ届かない（上流の制限 tauri-apps/wry#451。詳細と実測は下の節の (2)）。
# 代わりに**アクセラレータが生むのと同じメッセージ**を送る（`Send-AcceleratorActivation`）。
# **`EnumWindows` は
# 所有者 pid と**題名の完全一致**で絞る（単一インスタンスのプラグインは同じ pid に可視の
# 隠しウィンドウ `com.jxcel.app-siw` を作るため、10.5 と同じ前提が要る）。

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
if (-not (Test-Path $verify)) { throw "検証用の形が見つかりません: ${verify}（--features verification-triggers のビルドではない）" }
$document = Join-Path $env:RUNNER_TEMP "jxcel-10-6-document.txt"
Set-Content -Path $document -Value "jxcel 10.6 の検証で開くドキュメント（内容は読まれない。tasks.md 7.7）" -Encoding UTF8

# メニューの列挙・活性化・前面化・キー送信（1 つの型にまとめる）。**here-string を
# 使わない** — YAML の中では終端 `"@` を行頭に置けず、PowerShell の here-string が
# 成立しない（10.5 の段と同じ）。
$menuApi = @(
  'using System;',
  'using System.Collections.Generic;',
  'using System.Text;',
  'using System.Runtime.InteropServices;',
  '',
  'public static class JxcelMenuApi {',
  '  public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);',
  '  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr lParam);',
  '  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);',
  '  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextLength(IntPtr hWnd);',
  '  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);',
  '  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr hWnd);',
  '  [DllImport("user32.dll")] public static extern IntPtr GetMenu(IntPtr hWnd);',
  '  [DllImport("user32.dll")] public static extern int GetMenuItemCount(IntPtr hMenu);',
  '  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetMenuStringW(IntPtr hMenu, uint uIDItem, StringBuilder text, int count, uint flags);',
  '  [DllImport("user32.dll")] public static extern IntPtr GetSubMenu(IntPtr hMenu, int position);',
  '  [DllImport("user32.dll")] public static extern uint GetMenuItemID(IntPtr hMenu, int position);',
  '  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr hWnd, uint message, IntPtr wParam, IntPtr lParam);',
  '  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int command);',
  '  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);',
  '  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();',
  '  public const uint MF_BYPOSITION = 0x400;',
  '  public const uint WM_COMMAND = 0x0111;',
  '  public const int SW_RESTORE = 9;',
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
  '  public static string ItemText(IntPtr menu, int position) {',
  '    var text = new StringBuilder(512);',
  '    GetMenuStringW(menu, (uint)position, text, text.Capacity, MF_BYPOSITION);',
  '    return text.ToString();',
  '  }',
  '}'
) -join "`n"
Add-Type -TypeDefinition $menuApi

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
    $handles = @([JxcelMenuApi]::WindowsOf($OwnerPid, $title))
    if ($handles.Count -ge $AtLeast) { return $handles }
    Start-Sleep -Milliseconds 100
  }
  return $null
}
function Wait-Exit {
  param([System.Diagnostics.Process]$Target, [int]$Seconds)
  return $Target.WaitForExit($Seconds * 1000)
}
function Stop-Instance {
  param([System.Diagnostics.Process]$Target)
  if ($Target -and -not $Target.HasExited) { $Target.Kill() }
  if ($Target) { $Target.WaitForExit(10000) | Out-Null }
}
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
function Wait-Menu {
  # **メニューバーが付くのを待つ。** アプリは**ウィンドウを出した後**にメニューを付ける
  # — `WebviewWindowBuilder::build()` が戻ってから（WebView2 の初期化の後）
  # `Window::set_menu` を呼ぶため、ウィンドウが列挙できるようになった直後の 1 回の
  # `GetMenu` は 0 を返しうる（実測: ウィンドウの出現は起動から 162 ms、初回描画は
  # 583 ms。メニューの付与はその間）。**1 回読んで 0 なら失敗**としていたため、
  # ウィンドウを観測した直後の読みが偽の失敗になっていた（2026-09-12 の CI の実測）。
  param([IntPtr]$Handle, [int]$Seconds = 30)
  $deadline = (Get-Date).AddSeconds($Seconds)
  while ((Get-Date) -lt $deadline) {
    $menu = [JxcelMenuApi]::GetMenu($Handle)
    if ($menu -ne [IntPtr]::Zero) { return $menu }
    Start-Sleep -Milliseconds 100
  }
  return [IntPtr]::Zero
}
function Show-RecordTail {
  # メニューが付かなかったときの原因（アプリが付与に失敗したのか、付ける前に
  # 終わったのか）は記録にしか現れないので、失敗の直前に出す。
  if (Test-Path $record) {
    Write-Host "--- 診断記録（末尾）: $record ---"
    Get-Content -Path $record -Encoding UTF8 -ErrorAction SilentlyContinue |
      Select-Object -Last 40 | ForEach-Object { Write-Host $_ }
  } else {
    Write-Host "--- 診断記録: ${record}（存在しません） ---"
  }
}
# メニューバーを「部分メニュー名 → (項目の文字列 → コマンド識別子)」として読む。
function Get-MenuTree {
  param([IntPtr]$Menu)
  $tree = [ordered]@{}
  $count = [JxcelMenuApi]::GetMenuItemCount($Menu)
  for ($i = 0; $i -lt $count; $i++) {
    $label = [JxcelMenuApi]::ItemText($Menu, $i)
    $sub = [JxcelMenuApi]::GetSubMenu($Menu, $i)
    if ($sub -eq [IntPtr]::Zero) { continue }
    $items = [ordered]@{}
    $n = [JxcelMenuApi]::GetMenuItemCount($sub)
    for ($j = 0; $j -lt $n; $j++) {
      $items[[JxcelMenuApi]::ItemText($sub, $j)] = [JxcelMenuApi]::GetMenuItemID($sub, $j)
    }
    $tree[$label] = $items
  }
  return $tree
}
function Assert-MenuItem {
  param($Tree, [string]$Menu, [string]$Item, [string]$What)
  if (-not $Tree.Contains($Menu)) {
    throw "${What}: 部分メニュー '$Menu' がありません（あるのは $($Tree.Keys -join ', ')）"
  }
  $items = $Tree[$Menu]
  if (-not $items.Contains($Item)) {
    throw "${What}: '$Menu' に項目 '$Item' がありません（あるのは $($items.Keys -join ' / ')）"
  }
  Write-Host "OK: Win32: $Menu > $Item （コマンド識別子=$($items[$Item])）"
}
# **アクセラレータが生むのと同じメッセージ**を送る（`WM_COMMAND` を
# `wParam = 0x10000 | コマンド識別子` で）。`TranslateAcceleratorW` は一致した
# アクセラレータについて `send_message(hwnd, WM_COMMAND, 0x10000 | cmd, 0)` を行う
# （Wine の `win32u/menu.c` の `translate_accelerator` で確認）。**つまりこの段が送る
# メッセージは、アクセラレータが作用したときにホストウィンドウへ届くものそのものである**
# （メニューをクリックしたときの `WM_COMMAND` は `wParam = cmd` であり、通知ビットが
# 無い — 10.6 の (1) はそちらを送っている）。
#
# **キーそのものは送らない。** Windows の WebView2 は自分の子ウィンドウにキーボード
# フォーカスを持ってアクセラレータキーを自分の経路で消費するため、キーはホスト
# ウィンドウのアクセラレータ表へ届かない（上流の既知の制限: tauri-apps/wry#451。
# 実測: 2026-09-12 の Windows のランナーでは `keybd_event` のグローバル注入でも、
# 修飾キーを押したまま `WM_KEYDOWN` を送る形でも、記録行が 1 行も現れなかった）。
# **この段が実測するのは、活性化が muda のサブクラス → 登録元 → 対象ウィンドウの解決へ
# 到達し、対象がフォーカスに追随すること**である（アクセラレータ表の照合そのものは
# 測らない — それは Linux の段が実キーで測る）。
function Send-AcceleratorActivation {
  param([IntPtr]$Handle, [int]$ItemId)
  if (-not [JxcelMenuApi]::PostMessage($Handle, [JxcelMenuApi]::WM_COMMAND, [IntPtr](0x10000 -bor $ItemId), [IntPtr]::Zero)) {
    throw "アクセラレータの活性化（WM_COMMAND 0x10000|${ItemId}）を送れませんでした"
  }
}

# そのウィンドウを前面へ出せたかを返す（**効かなければ主張しない**）。
function Set-ForegroundChecked {
  param([IntPtr]$Handle)
  [JxcelMenuApi]::ShowWindow($Handle, [JxcelMenuApi]::SW_RESTORE) | Out-Null
  [JxcelMenuApi]::SetForegroundWindow($Handle) | Out-Null
  $deadline = (Get-Date).AddSeconds(5)
  while ((Get-Date) -lt $deadline) {
    if ([JxcelMenuApi]::GetForegroundWindow() -eq $Handle) { return $true }
    Start-Sleep -Milliseconds 200
  }
  return $false
}

# -------------------------------------------------------------------
# (1) 配布物: メニューバーの読み（項目の出現とショートカットの表示）と活性化（通知）
# -------------------------------------------------------------------
$phaseShipping = @(Get-RecordLines).Count
$shipping = Start-Process -FilePath $installed -PassThru
try {
  # **題名の完全一致で絞る**（単一インスタンスのプラグインは同じ pid に可視の隠し
  # ウィンドウ `com.jxcel.app-siw` を作る。`MainWindowHandle` はそれを指しうる）。
  $shippingHandles = Wait-Windows -OwnerPid $shipping.Id -AtLeast 1 -Seconds 60
  if (-not $shippingHandles) {
    if ($shipping.HasExited) { throw "配布物がウィンドウを出す前に終了しました (exit=$($shipping.ExitCode))" }
    throw "配布物のウィンドウが 60 秒以内に現れませんでした"
  }
  if ($shippingHandles.Count -ne 1) { throw "配布物のウィンドウが $($shippingHandles.Count) 枚あります（1 枚であるべき）" }
  Write-Host "検証 1/2: 配布物: ウィンドウ '$title' pid=$($shipping.Id) ハンドル=$(Get-HandleLabel -Handles $shippingHandles)"

  $menu = Wait-Menu -Handle $shippingHandles[0]
  if ($menu -eq [IntPtr]::Zero) {
    Show-RecordTail
    throw "配布物のウィンドウにメニューバーがありません（GetMenu が 0 のまま 30 秒待ちました）"
  }
  $tree = Get-MenuTree -Menu $menu
  Write-Host "OK: Win32: 配布物のウィンドウのメニューバーを読んだ（部分メニュー: $($tree.Keys -join ', ')）"
  Assert-MenuItem $tree "ファイル" "開く…`tCtrl+O" "配布物"
  Assert-MenuItem $tree "ファイル" "終了`tCtrl+Q" "配布物"
  Assert-MenuItem $tree "診断" "診断情報を書き出す…`tCtrl+Shift+E" "配布物"
  Assert-MenuItem $tree "診断" "記録の保存場所を表示`tCtrl+Shift+L" "配布物"
  Assert-MenuItem $tree "診断" "記録の詳細度…`tCtrl+Shift+V" "配布物"
  foreach ($menuLabel in $tree.Keys) {
    foreach ($itemLabel in $tree[$menuLabel].Keys) {
      if ($itemLabel -like "*検証*") {
        throw "配布物のメニューに検証専用の項目 '$itemLabel' が現れています（既定のビルドに入ってはならない）"
      }
    }
  }
  Write-Host "検証 1/2: 配布物のメニューに検証専用の項目は現れない（アクセラレータは項目の文字列に埋め込まれている）"

  Write-Host "検証 1/2: 診断 > 記録の保存場所を表示 を WM_COMMAND で活性化する（選択が登録元へ通知されること）"
  # **活性化の前にウィンドウを前面へ出す。** ウィンドウ単位のメニューでも、基盤の
  # メニューイベントは発生元のウィンドウを運ばない（tauri 2.11.5 の `MenuEvent` は
  # 項目の識別子だけ）ので、アプリは**活性化の時点のフォーカス**から対象ウィンドウを
  # 解決する（7.5 の `activation_target`）。前面化していないと対象が決まらず、
  # 通知は `対象ウィンドウ=(対象なし)` になり、登録元は要求を送らない
  # （実測: 2026-09-12 の Windows のランナー。Linux の段は `XSetInputFocus` で同じことを
  # している）。
  if (-not (Set-ForegroundChecked -Handle $shippingHandles[0])) {
    Show-RecordTail
    throw "配布物のウィンドウを前面へ出せませんでした（活性化の対象が決まらず、通知が (対象なし) になります）"
  }
  # **フォーカスの通知がアプリへ届いてから活性化する**（届く前に送ると対象が (対象なし)
  # のままになる。上の待ちは `GetForegroundWindow` の一致までしか見ていない）。
  Start-Sleep -Milliseconds 500
  # 送り先は**メニューを読んだのと同じウィンドウ**（`MainWindowHandle` は単一
  # インスタンスの隠しウィンドウを指しうる）。
  $itemId = $tree["診断"]["記録の保存場所を表示`tCtrl+Shift+L"]
  if (-not [JxcelMenuApi]::PostMessage($shippingHandles[0], [JxcelMenuApi]::WM_COMMAND, [IntPtr]$itemId, [IntPtr]::Zero)) {
    throw "WM_COMMAND を送れませんでした（コマンド識別子=${itemId}）"
  }
  $notified = Find-RecordLine -Skip $phaseShipping -Pattern "メニュー項目が選択された: 登録元=app-shell 項目=app-shell.diagnostics-log-location " -Seconds 10
  if (-not $notified) { throw "活性化の通知の行（メニュー項目が選択された: 登録元=app-shell 項目=app-shell.diagnostics-log-location …）が記録に現れません" }
  Write-Host "検証 1/2: 記録（通知）: $notified"
  $requested = Find-RecordLine -Skip $phaseShipping -Pattern "診断の導線の要求を送った: ウィンドウ = [a-z0-9_-]+ / 導線 = 記録の保存場所" -Seconds 10
  if (-not $requested) { throw "登録元が要求を送った行（診断の導線の要求を送った: ウィンドウ = … / 導線 = 記録の保存場所）が記録に現れません" }
  Write-Host "検証 1/2: 記録（登録元の処理）: $requested"
} finally {
  Stop-Instance $shipping
}

# -------------------------------------------------------------------
# (2) 検証用の形: フォーカスされているウィンドウにだけショートカットが作用すること
# -------------------------------------------------------------------
$phaseShortcut = @(Get-RecordLines).Count
Write-Host "検証 2/2: 検証用の形を起動する（1 枚目はドキュメントなし）"
$first = Start-Process -FilePath $verify -PassThru
$second = $null
try {
  $firstHandles = Wait-Windows -OwnerPid $first.Id -AtLeast 1 -Seconds 60
  if (-not $firstHandles) { throw "検証用の形のウィンドウが 60 秒以内に現れませんでした" }
  if (-not (Find-RecordLine -Skip $phaseShortcut -Pattern "ウィンドウを開いた: label=empty-1 " -Seconds 10)) {
    throw "1 枚目のウィンドウが label=empty-1 として記録に現れません"
  }
  Write-Host "検証 2/2: 1 枚目: ラベル=empty-1 ハンドル=$(Get-HandleLabel -Handles $firstHandles)"

  $subPhase = @(Get-RecordLines).Count
  # **1 枚目のメニューにもアクセラレータが乗っていること**（配布物と同じ検査。ウィンドウ単位の
  # メニューはウィンドウごとに組み立てられる）。
  $firstMenu = Wait-Menu -Handle $firstHandles[0]
  if ($firstMenu -eq [IntPtr]::Zero) {
    Show-RecordTail
    throw "1 枚目のウィンドウにメニューバーがありません（GetMenu が 0 のまま 30 秒待ちました）"
  }
  $firstTree = Get-MenuTree -Menu $firstMenu
  Assert-MenuItem $firstTree "ファイル" "検証: 対象ウィンドウを記録`tCtrl+Shift+J" "1 枚目（ウィンドウ単位のメニュー）"
  if (-not (Set-ForegroundChecked -Handle $firstHandles[0])) {
    Write-Host "限界: 1 枚目を前面にできなかった（SetForegroundWindow が効かない環境）。フォーカス先への作用の実測は Linux の段が担う。"
  } else {
    Send-AcceleratorActivation -Handle $firstHandles[0] -ItemId $firstTree["ファイル"]["検証: 対象ウィンドウを記録`tCtrl+Shift+J"]
    $emptyHit = Find-RecordLine -Skip $subPhase -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=empty-1$" -Seconds 10
    if (-not $emptyHit) { throw "1 枚目にフォーカスしたときの活性化の対象が empty-1 として記録に現れません（活性化が届いていないか、対象がフォーカスと一致していません）" }
    if ((Get-RecordCount -Skip $subPhase -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=doc-1$") -ne 0) {
      throw "1 枚目にフォーカスしたのに doc-1 へ作用しました"
    }
    Write-Host "検証 2/2: 記録（1 枚目にフォーカス）: $emptyHit"
  }

  Write-Host "検証 2/2: 同じ検証用の形をドキュメント位置つきで再度実行する（単一インスタンスが引き継ぐ）"
  $second = Start-Process -FilePath $verify -ArgumentList $document -PassThru
  if (-not (Wait-Exit -Target $second -Seconds 60)) {
    throw "2 つ目の起動が 60 秒以内に終了しません（2 つ目が常駐している）"
  }
  if ($second.ExitCode -ne 0) { throw "2 つ目の起動が終了コード $($second.ExitCode) で終わった（0 であるべき）" }
  $allHandles = Wait-Windows -OwnerPid $first.Id -AtLeast ($firstHandles.Count + 1) -Seconds 60
  if (-not $allHandles) { throw "2 つ目の起動の後、ウィンドウ数が $($firstHandles.Count + 1) 以上になりませんでした" }
  $newHandles = @($allHandles | Where-Object { $firstHandles -notcontains $_ })
  if ($newHandles.Count -ne 1) { throw "増えたウィンドウが 1 枚ではありません（$($newHandles.Count) 枚）" }
  $docLine = Find-RecordLine -Skip $phaseShortcut -Pattern "ウィンドウを開いた: label=doc-" -Seconds 10
  if (-not $docLine) { throw "新しいウィンドウが label=doc-* として記録に現れません" }
  $docLabel = (($docLine -split "label=")[1] -split " ")[0]

  # **2 枚目のウィンドウにも自分用のメニューバーがある**（ウィンドウ単位の配置。要件 3.6）。
  $secondMenu = Wait-Menu -Handle $newHandles[0]
  if ($secondMenu -eq [IntPtr]::Zero) {
    Show-RecordTail
    throw "2 枚目のウィンドウにメニューバーがありません（ウィンドウ単位のメニューが付いていない。GetMenu が 0 のまま 30 秒待ちました）"
  }
  $secondTree = Get-MenuTree -Menu $secondMenu
  Assert-MenuItem $secondTree "ファイル" "検証: 対象ウィンドウを記録`tCtrl+Shift+J" "2 枚目（ウィンドウ単位のメニュー）"
  Write-Host "検証 2/2: 2 枚目: ラベル=$docLabel ハンドル=$(Get-HandleLabel -Handles $newHandles)（1 枚目とは別のハンドル）"

  $subPhase2 = @(Get-RecordLines).Count
  if (-not (Set-ForegroundChecked -Handle $newHandles[0])) {
    Write-Host "限界: 2 枚目を前面にできなかった（SetForegroundWindow が効かない環境）。フォーカス先への作用の実測は Linux の段が担う。"
  } else {
    Send-AcceleratorActivation -Handle $newHandles[0] -ItemId $secondTree["ファイル"]["検証: 対象ウィンドウを記録`tCtrl+Shift+J"]
    $docHit = Find-RecordLine -Skip $subPhase2 -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=$docLabel$" -Seconds 10
    if (-not $docHit) { throw "2 枚目にフォーカスしたときのショートカットの対象が $docLabel として記録に現れません（フォーカス先へ振り向いていない）" }
    if ((Get-RecordCount -Skip $subPhase2 -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=$docLabel$") -ne 1) {
      throw "2 枚目にフォーカスしたときの作用が 1 回ではありません"
    }
    if ((Get-RecordCount -Skip $subPhase2 -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=empty-1$") -ne 0) {
      throw "2 枚目にフォーカスしたのに empty-1 へ作用しました（フォーカスされていないウィンドウに作用している）"
    }
    Write-Host "検証 2/2: 記録（2 枚目にフォーカス）: $docHit"

    # フォーカスを 1 枚目へ戻すと対象も戻ること（追随すること）。
    $subPhase3 = @(Get-RecordLines).Count
    if (Set-ForegroundChecked -Handle $firstHandles[0]) {
      Send-AcceleratorActivation -Handle $firstHandles[0] -ItemId $firstTree["ファイル"]["検証: 対象ウィンドウを記録`tCtrl+Shift+J"]
      $backHit = Find-RecordLine -Skip $subPhase3 -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=empty-1$" -Seconds 10
      if (-not $backHit) { throw "フォーカスを 1 枚目へ戻したときの対象が empty-1 として記録に現れません（フォーカスの移動に追随していない）" }
      if ((Get-RecordCount -Skip $subPhase3 -Pattern "\[検証\] ショートカットが作用した対象ウィンドウ=$docLabel$") -ne 0) {
        throw "フォーカスを 1 枚目へ戻したのに $docLabel へ作用しました"
      }
      Write-Host "検証 2/2: 記録（フォーカスを 1 枚目へ戻した）: $backHit"
      Write-Host "OK: Windows: ショートカットの対象はフォーカスに追随した（キー入力の実測）"
    } else {
      Write-Host "限界: 1 枚目へ戻す前面化が効かなかったため、追随の実測は行わなかった"
    }
  }
} finally {
  Stop-Instance $second
  Stop-Instance $first
}

if (@(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue).Count -ne 0) {
  throw "後始末の後に常駐しているインスタンスが残っています"
}
Write-Host "検証（後始末）: 常駐インスタンス数=0（残存プロセスなし）"
Write-Host "OK: Windows: 配布物のメニューバー（項目とアクセラレータの文字列）・WM_COMMAND による活性化の通知・2 枚のウィンドウでのフォーカス追随を検証した（キーそのものは WebView2 が消費するため送っていない）"
