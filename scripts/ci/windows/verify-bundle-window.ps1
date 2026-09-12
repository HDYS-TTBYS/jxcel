# 起動の検証（Windows）。**仮想ディスプレイのような追加設定は不要**である。
# ウィンドウの有無は .NET の `Process.MainWindowHandle` で判定する。これは
# 呼び出し元と同じデスクトップ上のトップレベルウィンドウを列挙するため、
# ランナーの子プロセスとして起動したアプリに対して成立する（デスクトップが
# 可視かどうかに依存しない）。
# 検証対象は配布物（NSIS インストーラ `target/release/bundle/nsis/*-setup.exe`）を
# `/S` で無人インストールして得られる実行ファイルである。既定の installMode は
# `currentUser` なので導入先は `%LOCALAPPDATA%\jxcel\jxcel.exe` になる。
# WebView2 ランタイムが導入されていない場合は、インストーラが bootstrapper を
# 取得して `/silent /install` で導入する（既定の downloadBootstrapper）。
#
# 同じポーリングが**起動からウィンドウ表示までの時間**も計測する（tasks.md 10.3 /
# 要件 1.3, 6.8）。計測区間の始点は OS が記録したプロセス生成時刻
# （`Process.StartTime`）である。`Start-Process` の戻りを待ってから計ると
# その待ち時間の分だけ計測値が小さく出る。値は `target/startup-measurements.txt`
# に `windows=<ミリ秒>` として書き、CI の出力にも出す。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了エラーで段が落ちるようにする。抽出前は台本側が与えていた）。
$ErrorActionPreference = 'Stop'

$setup = Get-ChildItem "target/release/bundle/nsis/*-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "NSIS インストーラが見つかりません" }

# 「追加のインストールを要求せずに起動する」ことの前提を記録する（要件 1.2、tasks.md 10.4）。
# WebView2 ランタイムが無い環境では、インストーラが bootstrapper を取得して
# `/silent /install` で導入する（既定の downloadBootstrapper。上のコメント）。**ランナー
# には通常プリインストール済み**なので、この経路は実際には走らないことが多い — その事実も
# 出力に残し、「証明できていないこと」を隠さない。導入済みの判定は EdgeUpdate の
# クライアント登録（`pv` に版）で行う。
$wv2Keys = @(
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
)
function Get-WebView2Version {
  foreach ($key in $wv2Keys) {
    if (Test-Path $key) {
      $version = (Get-ItemProperty -Path $key -ErrorAction SilentlyContinue).pv
      if ($version) { return $version }
    }
  }
  return $null
}
$wv2Before = Get-WebView2Version
if ($wv2Before) {
  Write-Host "WebView2 ランタイム: 導入済み (pv=$wv2Before) — bootstrapper の経路はこの実行では走らない"
} else {
  Write-Host "WebView2 ランタイム: 未導入 — bootstrapper が /silent /install で導入する経路を実測する"
}

Start-Process -FilePath $setup.FullName -ArgumentList "/S" -Wait

if (-not $wv2Before) {
  $wv2After = Get-WebView2Version
  if (-not $wv2After) { throw "WebView2 ランタイムが導入されていない（bootstrapper が動いていない）" }
  Write-Host "WebView2 ランタイム: 無人導入で導入された (pv=$wv2After) — 利用者の操作は無い"
}

$exe = Join-Path $env:LOCALAPPDATA "jxcel\jxcel.exe"
if (-not (Test-Path $exe)) {
  $found = Get-ChildItem -Path $env:LOCALAPPDATA -Filter "jxcel.exe" -Recurse -Depth 3 -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($found) { $exe = $found.FullName }
}
if (-not (Test-Path $exe)) { throw "導入された実行ファイルが見つかりません" }

$measure = "target/startup-measurements.txt"
if (Test-Path $measure) { Remove-Item $measure -Force }

# 冷えたランナーの**初回起動だけ**が持つ一度きりのコスト（WebView2 ランタイムの
# 初回読み込み。research.md「起動時間」は「Windows の初回起動はページキャッシュに
# 乗っていないため定常状態より悪い」と記す）を計測区間へ混ぜない。要件 1.3 の根拠で
# ある Tauri の公式ベンチは**ウォーム実行**であり、ここでも 1 回起動して捨ててから
# 計測する。冷えた値も情報として出力する（判定には使わない）。
$warmMeasure = $null
$warm = Start-Process -FilePath $exe -PassThru
$warmStart = $warm.StartTime
$warmDeadline = (Get-Date).AddSeconds(60)
while ((Get-Date) -lt $warmDeadline) {
  Start-Sleep -Milliseconds 100
  if ($warm.HasExited) { break }
  $warm.Refresh()
  if ($warm.MainWindowHandle -ne 0) {
    $warmMeasure = [int]((Get-Date) - $warmStart).TotalMilliseconds
    break
  }
}
if (-not $warm.HasExited) { Stop-Process -Id $warm.Id -Force }
$warmText = if ($null -ne $warmMeasure) { "$warmMeasure" } else { "(計測なし)" }
Write-Host "参考: ウォームアップ起動（計測には使わない）: windows=$warmText"

$p = Start-Process -FilePath $exe -PassThru
$start = $p.StartTime
$deadline = (Get-Date).AddSeconds(60)
$title = $null
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Milliseconds 100
  if ($p.HasExited) { throw "アプリがウィンドウを出す前に終了しました (exit=$($p.ExitCode))" }
  $p.Refresh()
  if ($p.MainWindowHandle -ne 0) { $title = $p.MainWindowTitle; break }
}
if (-not $title) { throw "60 秒以内にウィンドウが現れませんでした" }
if ($title -notmatch "jxcel") { throw "予期しないウィンドウタイトル: $title" }
# 計測値は「観測した時刻」なので、実際の表示より最大 0.1 秒大きく出る（保守側）。
$elapsedMs = [int]((Get-Date) - $start).TotalMilliseconds
Set-Content -Path $measure -Value "windows=$elapsedMs" -Encoding ascii
Write-Host "OK: ウィンドウ '$title' が現れました (pid=$($p.Id), 起動から $elapsedMs ms)"
Write-Host "計測値: windows=${elapsedMs}（書き出し先 ${measure}）"
Stop-Process -Id $p.Id -Force
