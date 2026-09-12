# Windows の描画検証（tasks.md 10.4）。3 回起動する（配布物・smoke-table・smoke-editor）。
# ウィンドウの観測は 1.5 の段と同じ `MainWindowHandle`。描画と**描画された画面**は診断記録を
# 読む（第 2 引数は**期待する「描画された画面」**であり、要求した識別子ではない）。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了エラーで段が落ちるようにする。抽出前は台本側が与えていた）。
$ErrorActionPreference = 'Stop'

# 4.4 の解決（Windows は %LOCALAPPDATA%\{識別子}\logs）。
$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
New-Item -ItemType Directory -Force -Path (Split-Path $record) | Out-Null
Write-Host "診断記録: $record"

# 描画が成立したことを示す行は 3 つある: `Painted` / ソフトウェアラスタライザ経由 /
# **期限を超えてから成立した行**（下の 3 つ目。判定は `NoPaint` のままだが `画面=` を
# 運ぶので、どの画面が描画されたかの証拠としては同等である）。
$heartbeatPatterns = @(
  "初回描画が成立した: label=",
  "初回描画は成立したがソフトウェアラスタライザ経由である: label=",
  # **期限を超えてから描画が成立した場合**（遅いランナー）。この行も `画面=` を運ぶので、
  # どの画面が描画されたかの証拠としては同等である。
  "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label="
)

function Test-Render {
  param(
    [string]$Exe,
    [string]$Expected,
    [string]$What
  )
  if (-not (Test-Path $Exe)) { throw "${What}: 実行ファイルがありません: $Exe" }
  # **記録は起動の前に空にする**（前回の起動の行で偽の成功をしない）。
  if (Test-Path $record) { Remove-Item $record -Force }
  if ($Expected) { $env:JXCEL_VERIFICATION_INITIAL_SCREEN = $Expected }
  else { Remove-Item Env:\JXCEL_VERIFICATION_INITIAL_SCREEN -ErrorAction SilentlyContinue }
  $p = Start-Process -FilePath $Exe -PassThru
  try {
    $deadline = (Get-Date).AddSeconds(60)
    $title = $null
    while ((Get-Date) -lt $deadline) {
      Start-Sleep -Milliseconds 100
      if ($p.HasExited) { throw "${What}: アプリがウィンドウを出す前に終了しました (exit=$($p.ExitCode))" }
      $p.Refresh()
      if ($p.MainWindowHandle -ne 0) { $title = $p.MainWindowTitle; break }
    }
    if (-not $title) { throw "${What}: 60 秒以内にウィンドウが現れませんでした" }
    if ($title -notmatch "jxcel") { throw "${What}: 予期しないウィンドウタイトル: $title" }
    Write-Host "OK: ウィンドウ '$title' が現れました (pid=$($p.Id))"

    $heartbeat = $null
    $actual = $null
    $renderDeadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $renderDeadline) {
      Start-Sleep -Milliseconds 200
      if (Test-Path $record) {
        foreach ($line in (Get-Content -Path $record -Encoding UTF8)) {
          foreach ($pattern in $heartbeatPatterns) {
            if ($line -like "*$pattern*") { $heartbeat = $line }
          }
        }
      }
      if ($heartbeat) {
        # 成立行が報告するのは**実際に描画されていた画面**である。通知はウィンドウごとに
        # 1 回なので、待っても変わらない（8.2 の契約）。
        $match = [regex]::Match($heartbeat, "画面=(\S+)")
        $actual = if ($match.Success) { $match.Groups[1].Value } else { "（報告なし）" }
        if (-not $Expected) { break }
        if ($actual -eq $Expected) { break }
        throw "${What}: 描画は成立しましたが、描画された画面が期待と一致しません: 期待 screen=$Expected / 実際 screen=${actual}（起動時の指定が登録簿に無いか、描画が既定の画面へ落ちています）"
      }
    }
    if (-not $heartbeat) { throw "${What}: 初回描画の成立行が 20 秒以内に現れませんでした（初回描画が成立していない）" }
    Write-Host "初回描画: $heartbeat"
    if ($Expected) {
      Write-Host "描画された画面: ${actual}（期待 ${Expected}）"
      # 要求の行は**証明ではなく起動の識別**として出す（あれば。配布物は出さない）。
      foreach ($line in (Get-Content -Path $record -Encoding UTF8)) {
        if ($line -like "*検証用の初期画面を指定した: screen=$Expected*") { Write-Host "初期画面の指定: $line" }
      }
    }
  } finally {
    if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
    # プロセスの終了を待ってから戻る。次回の起動は診断記録を削除するので、直前の
    # プロセスが記録のハンドルを握ったままだと削除が失敗しうる（無言の取りこぼしを作らない）。
    if ($p) { $p.WaitForExit(5000) | Out-Null }
  }
}

# 導入先は 1.5 の段と同じ前提（既定の installMode は currentUser）。見つからない場合は
# 1.5 の段と同じ探索に退避する（導入先が変わっても偽の失敗をしない）。
$installed = Join-Path $env:LOCALAPPDATA "jxcel\jxcel.exe"
if (-not (Test-Path $installed)) {
  $found = Get-ChildItem -Path $env:LOCALAPPDATA -Filter "jxcel.exe" -Recurse -Depth 3 -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($found) { $installed = $found.FullName }
}
if (-not (Test-Path $installed)) { throw "導入された実行ファイルが見つかりません: $installed" }
# 配布物は環境変数を読まない（9.7）ので初期画面は 9.6 の空ウィンドウの画面である。
# それでも `画面=empty-window` を要求する（配布物でも画面の報告経路が働くことの証明）。
Write-Host "検証 1/3: 配布物（NSIS インストーラで /S 導入済みの実行ファイル）を起動する。追加の操作は無い"
Test-Render -Exe $installed -Expected "empty-window" -What "配布物"
Write-Host "検証 2/3: 表形式の最小画面 smoke-table（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
Test-Render -Exe "target/release/jxcel.exe" -Expected "smoke-table" -What "画面 smoke-table"
Write-Host "検証 3/3: 文字編集の最小画面 smoke-editor（検証用の形。スモーク画面のコードとシェルの構造は配布物と共通）"
Test-Render -Exe "target/release/jxcel.exe" -Expected "smoke-editor" -What "画面 smoke-editor"
