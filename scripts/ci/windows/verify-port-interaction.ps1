# Windows の段（**tasks.md 7.2**）。移植口の確認の使い捨ての画面（`smoke-port-probe`）が
# Windows で**実際に描画される**ことを起動して確かめる。観測は 10.4 の段と同じ
# `MainWindowHandle` と診断記録を使う（**この段のために新しい系統を作らない** — 1.6「既存の
# 3 OS の検証マトリクスに一時的な段を足し、新しい系統を作らない」）。
#
# # 主張はここでは判定しない（AT-SPI が無い）
#
# 7.2 の 3 つの主張は、使い捨ての画面が**アクセシビリティの木**（AT-SPI）へ出す 1 行から読む。
# **Windows には AT-SPI が無い**（AT-SPI は D-Bus 上の仕組みであり、WebView2 は UI Automation を
# 使う。Linux の検査器 `scripts/check-port-interaction.sh` を Git Bash で呼んでも、観測の行を
# 読む経路が無い）。したがって**この段は Windows で主張を判定しない**。ここで確かめるのは
# 次の 2 つである:
#
#   1. 移植口の確認の画面が WebView2 で**実際に描画される**こと（`画面=smoke-port-probe` の成立行）。
#   2. その画面が Windows で**起動できる**こと（ウィンドウが現れ、初回描画が成立する）。
#
# **判定できないことを「成立」と読み替えない** — この段が緑でも、7.2 の Windows の証拠には
# ならない。恒久の観測は 9.2 が担う。
#
# # 一時的な段である
#
# 9.2 が 3 OS の観測を恒久の段として載せた時点で、**この段は取り除く**。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの
# 終了エラーで段が落ちるようにする。10.4 の段と同じ）。
$ErrorActionPreference = 'Stop'

# 4.4 の解決（Windows は %LOCALAPPDATA%\{識別子}\logs）。
$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
New-Item -ItemType Directory -Force -Path (Split-Path $record) | Out-Null
Write-Host "診断記録: $record"

# 描画が成立したことを示す行は 3 つある（`Painted` / ソフトウェアラスタライザ経由 / 期限超過の
# あとに成立した行。いずれも `画面=` を運ぶので、どの画面が描画されたかの証拠として同等）。
$heartbeatPatterns = @(
  "初回描画が成立した: label=",
  "初回描画は成立したがソフトウェアラスタライザ経由である: label=",
  "期限超過のあとに描画の通知が届いた（不成立の提示を取り下げる）: label="
)

$expected = "smoke-port-probe"
$exe = "target\release\jxcel.exe"
if (-not (Test-Path $exe)) { throw "検証用の形の実行ファイルがありません: $exe（先に検証用のビルドを走らせてください）" }

# **記録は起動の前に空にする**（前回の起動の行で偽の成功をしない。10.4 の段と同じ規律）。
if (Test-Path $record) { Remove-Item $record -Force }
$env:JXCEL_VERIFICATION_INITIAL_SCREEN = $expected

$p = Start-Process -FilePath $exe -PassThru
try {
  # 1. ウィンドウの出現（10.4 の段と同じ観測）。
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
  Write-Host "ウィンドウ: '$title' が現れました (pid=$($p.Id))"

  # 2. 初回描画の成立と、そこに載る**描画された画面**。ウィンドウ生成から 3 秒が期限なので、
  #    これを大きく超える猶予（20 秒）で足りる。
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
      $match = [regex]::Match($heartbeat, "画面=(\S+)")
      $actual = if ($match.Success) { $match.Groups[1].Value } else { "（報告なし）" }
      if ($actual -eq $expected) { break }
      throw "描画は成立しましたが、描画された画面が期待と一致しません: 期待 screen=$expected / 実際 screen=${actual}（起動時の指定が登録簿に無いか、描画が既定の画面へ落ちています）"
    }
  }
  if (-not $heartbeat) { throw "初回描画の成立行が 20 秒以内に現れませんでした（初回描画が成立していない）" }
  Write-Host "初回描画: $heartbeat"
  Write-Host "描画された画面: $actual（期待 $expected）"
} finally {
  if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue }
  # プロセスの終了を待ってから戻る（次回の起動が記録を削除するため。10.4 の段と同じ）。
  if ($p) { $p.WaitForExit(5000) | Out-Null }
}

Write-Host "注記: この段は移植口の確認の画面が Windows で描画されることだけを確かめました。"
Write-Host "注記: 7.2 の 3 つの主張（走査・選択・列幅と列の位置）の Windows の観測は 9.2 が担います。"
