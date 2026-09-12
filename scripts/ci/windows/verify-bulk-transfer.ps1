# Windows の一括転送の検証（tasks.md 10.8）。同じ 2 つの主張を PowerShell で組む
# （起動と記録の読み取りは 10.4 / 10.5 / 10.7 と同じ手段）。配布物は **1.5 の段**（Windows の
# ウィンドウ検証）が `/S` で導入した `%LOCALAPPDATA%\jxcel\jxcel.exe` である（10.5 以降の
# 段は導入済みのものを消費するだけである）。

$ErrorActionPreference = "Stop"
$verify = "target\release\jxcel.exe"
$dist = Join-Path $env:LOCALAPPDATA "jxcel\jxcel.exe"
$recordPath = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
if (-not (Test-Path $verify)) { throw "検証用の形が見つかりません: ${verify}（--features verification-triggers のビルドではない）" }
if (-not (Test-Path $dist)) { throw "配布物が見つかりません: ${dist}（1.5 の段の /S 導入が先に必要）" }
Write-Host "診断記録: $recordPath"

# 1 行あたりのバイト数と行数の一覧（`scripts/check-bulk-transfer.sh` と同じ値。検査器は
# フロントエンドの申告を信用せず、期待バイト数をここで計算し直す）。
$lineBytes = 47
$sizes = @(100, 100000)

function Get-Lines {
  param([int]$Skip)
  if (-not (Test-Path $recordPath)) { return @() }
  $all = @(Get-Content -Path $recordPath -Encoding UTF8 -ErrorAction SilentlyContinue)
  if ($all.Count -le $Skip) { return @() }
  return $all[$Skip..($all.Count - 1)]
}
function Get-Count {
  param([int]$Skip, [string]$Pattern)
  return @(Get-Lines -Skip $Skip | Where-Object { $_ -match $Pattern }).Count
}
function Stop-App {
  param($Process)
  if ($null -eq $Process) { return }
  if (-not $Process.HasExited) { Stop-Process -Id $Process.Id -Force -ErrorAction SilentlyContinue }
  $deadline = (Get-Date).AddSeconds(10)
  while (-not $Process.HasExited -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100 }
}

# 1 回の起動で、行数ごとに 1 回ずつ転送させ、記録から 2 つの主張を判定する。
function Test-Transfer {
  param([string]$Exe, [string]$Phase, [int]$TimeoutSeconds)
  $skip = 0
  if (Test-Path $recordPath) { $skip = @(Get-Content -Path $recordPath -Encoding UTF8).Count }
  $env:JXCEL_VERIFICATION_BULK_ROWS = ($sizes -join ",")
  $p = $null
  try {
    $p = Start-Process -FilePath $Exe -PassThru
  } finally {
    Remove-Item Env:\JXCEL_VERIFICATION_BULK_ROWS -ErrorAction SilentlyContinue
  }
  try {
    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    while ((Get-Date) -lt $deadline) {
      if ((Get-Count -Skip $skip -Pattern '一括転送の結果:.*"invocations":') -ge $sizes.Count) { break }
      if ($p.HasExited) { break }
      Start-Sleep -Milliseconds 200
    }
    $observed = Get-Count -Skip $skip -Pattern '一括転送の結果:.*"invocations":'
    if ($observed -lt $sizes.Count) {
      throw "($Phase) 行数ごとの一括転送の結果が $TimeoutSeconds 秒以内に現れませんでした（観測 $observed 行。期待 $($sizes.Count) 行。転送が 1 件も起きていないか、失敗しています）"
    }
    $request = Get-Count -Skip $skip -Pattern '検証用の一括転送を要求した:'
    if ($request -ne 1) {
      throw "($Phase) 要求の行（'検証用の一括転送を要求した:'）が 1 行ではありません（観測 $request 行。検証用の形で起動していないか、環境変数が解釈されていません）"
    }
    $requestLine = @(Get-Lines -Skip $skip | Where-Object { $_ -match '検証用の一括転送を要求した:' })[0]
    Write-Host "($Phase) 要求の記録: $requestLine"
    $total = Get-Count -Skip $skip -Pattern 'jxcel::commands::bulk.*受信バイト数 = '
    if ($total -ne $sizes.Count) {
      throw "($Phase) 一括転送の呼び出しの合計が行数の件数と一致しません（観測 $total 回。期待 $($sizes.Count) 回 = 行数ごとにちょうど 1 回）"
    }
    foreach ($rows in $sizes) {
      $bytes = $rows * $lineBytes
      if ($bytes -le 0) { throw "($Phase) 行数 $rows の期待バイト数が 0 以下です（ペイロードが空である）" }
      $calls = Get-Count -Skip $skip -Pattern ("jxcel::commands::bulk.*受信バイト数 = " + $bytes + "$")
      if ($calls -ne 1) {
        throw "($Phase) 行数 ${rows}（期待 $bytes B）の bulk_echo の記録行が 1 行ではありません（観測 $calls 行。1 回の呼び出しで受け渡せていないか、重複しています）"
      }
      $results = @(Get-Lines -Skip $skip | Where-Object {
        $_ -match ('"rows":' + $rows + '[,}]') -and
        $_ -match ('"sentBytes":' + $bytes + '[,}]') -and
        $_ -match ('"receivedBytes":' + $bytes + '[,}]') -and
        $_ -match '"byteIdentical":true' -and
        $_ -match '"invocations":1[,}]'
      })
      if ($results.Count -ne 1) {
        throw "($Phase) 行数 ${rows}（期待 $bytes B）の一括転送の結果が 1 行ではありません（観測 $($results.Count) 行。送信・受信のバイト数・バイト一致・呼び出し回数 1 のいずれかが期待と一致しません）"
      }
      Write-Host "($Phase) 行数=$rows 呼び出し回数=1 期待バイト数=$bytes / $($results[0])"
    }
    Write-Host "($Phase) OK: 行数の一覧（$($sizes -join ',')）のすべてで、1 回の呼び出しがちょうど 1 行であり、往復のバイト数が一致した"
    Write-Host "($Phase) OK: 呼び出し回数は行数によらず定数 1 である（合計 $total 回 = 行数の件数 $($sizes.Count) 件。行数に比例しない）"
  } finally {
    Stop-App $p
  }
}

Write-Host "検証 1/2: 検証用の形で 100 行と 100,000 行を 1 回ずつ転送し、呼び出し回数が一定であることを検証する"
Test-Transfer -Exe $verify -Phase "検証 1/2" -TimeoutSeconds 60
Write-Host "検証 2/2: 反証 — 配布物（既定のビルド）は検証用の引き金を読まないので転送は起きない。検査は失敗しなければならない"
$beforeNeg = 0
if (Test-Path $recordPath) { $beforeNeg = @(Get-Content -Path $recordPath -Encoding UTF8).Count }
$failed = $false
try {
  Test-Transfer -Exe $dist -Phase "検証 2/2" -TimeoutSeconds 20
} catch {
  $failed = $true
  Write-Host "反証: 期待どおり失敗しました: $($_.Exception.Message)"
}
if (-not $failed) {
  throw "NG: 配布物に対して検査が成功してしまった（転送が 1 件も起きていないのに通っている）"
}
# **配布物が起動していたこと**まで要求する — 起動しなかった場合も「結果が現れない」に
# なるので、それだけでは錠前の実測にならない（起動行が記録にあることを確かめる）。
if ((Get-Count -Skip $beforeNeg -Pattern '残留プロセスの掃除で') -lt 1) {
  throw "NG: 反証で配布物が起動した形跡がありません（起動しなかった実行を錠前の実測と取り違えない）"
}
if ((Get-Count -Skip $beforeNeg -Pattern '検証用の一括転送を要求した:') -ne 0) {
  throw "NG: 配布物が検証用の引き金を読んでいます（既定のビルドに検証用の経路が入っている）"
}
Write-Host "反証: 配布物（既定のビルド）は起動したが、検証用の引き金を読まないため転送が 1 件も起きず検査が失敗することを確認した"
