# Windows の段（**tasks.md 9.2** / 要件 11.1, 11.2, 11.3, 12.1, 12.2, 12.3, 12.4）。
#
# 10 万行のシートを開き、末尾へ移動し、セルを編集し、取り消して戻すまでを**実際に起動して**
# 観測する。判定の本体は POSIX sh の `scripts/check-grid-observation.sh` であり、**この段は
# Git Bash でそれを呼ぶ** — 3 OS で同じ判定を閉じるための形である（`verification.md`「POSIX
# スクリプトは Windows でも `shell: bash` を明示する」）。**この段は Linux の段
# `scripts/ci/linux/verify-grid-observation.sh` の鏡であり、恒久である** — 1.6 の一時的な段
# （`verify-render-traversal.ps1`）は本段が入った時点で取り除いた（`research.md` の実測は残る）。
#
# 段が足すのは OS 固有の 3 点だけである（`verify-document-session.ps1` と同じ規律）:
#
#   1. **記録の位置**（`%LOCALAPPDATA%\com.jxcel.app\logs\jxcel.log`。4.4 の解決）と、
#      **Windows の綴りのパスを検査器へ渡す前の POSIX 形式への変換**
#   2. **実行ファイルの在り処**（検証用の形は `target\release\jxcel.exe`。反証に渡す配布物は
#      NSIS インストーラを `/S` で導入した `%LOCALAPPDATA%\jxcel\jxcel.exe` であり、1.5 の段が
#      導入を済ませている。AppImage / `.app` のような「そのまま実行できる配布物」は Windows には
#      無い）
#   3. **標本の生成**（10 万行 × 30 列を 1.4 の生成器から `target\observation\` へ書く）
#
# # 何を確かめるか（Linux / macOS の段と同じ。判定は検査器が持つ）
#
#   1. **通常の起動**: `--expect-paint=成立` で走査（要件 11.1 / 12.1）・編集の反映（11.3）・
#      取り消しの往復の成立。
#   2. **描画を成立させない条件の起動**: `--expect-paint=不成立` で、無内容の領域のまま留まらず
#      識別できる情報が提示され、`paint_failed` の記録が残ること（要件 12.2 / 12.3）。
#   3. **反証**: 導入済みの配布物を渡すと検査器が非 0 で落ちること。理由が「観測の行の不在」で
#      あることと、**配布物が起動してウィンドウを開いたこと**（記録の行）まで要求する（起動し
#      なかった実行も事実の欠落で落ちるので、理由を見ないと錠前の実測にならない）。**この段が
#      空回りしていないことの錠前である**（`verification.md`「無いことを確かめる検査には負の
#      対照を付ける」）。
#
# # 検査器側の既知の差に触れないこと
#
# 検査器（POSIX sh）は Windows では子孫の列挙に `pgrep` を使えないなどの差を**自分で明示する**
# ので、この段はその差を埋めない（段は検査器を呼ぶだけで、読み口や片付けの経路を足さない）。
# **例外は後始末の確認である** — 検査器の木の走査が効かない以上、残った WebView2 の補助プロセスを
# 後続の段が単一インスタンスの機構と取り違えうる。したがってこの段が `Get-Process -Name jxcel` で
# 残存 0 件を確かめ、残っていれば落とす（`verify-document-session.ps1` と同じ）。
#
# # 証拠が何を証明し、何を証明しないか
#
#   - 証明する: このランナーの**実物の検証用の形**を起動し、10 万行の走査と編集と取り消しが
#     成立すること、描画を成立させない条件で提示と記録が成ること、導入済みの配布物では観測の
#     行が読めないこと。判定は検査器が要件値で行う（この段は判定を足さない）。
#   - **証明しない**: 観測の行の**読み口**（UI Automation か、記録か）が Windows で成立すること
#     は検査器の側の責務であり、この段はそれを証明しない（段は検査器を呼ぶだけで、読み口を
#     足さない）。画面の見え方・レイアウト・描画の画素は見ない。インストーラの提示や署名も
#     見ない（1.5 の段の担当）。
#   - **この段は Windows では走らせられない。** この開発機に Windows の実行環境も `pwsh` も無い
#     ので、ここで確かめられるのは目視と、既存の Windows の段（`verify-document-session.ps1`）
#     との対比だけである。実行は **CI の Windows ランナーでのみ**確かめる
#     （`verification.md`「ローカルで閉じられないもの」）。
#
# # 位置
#
# 配布物を起動する段である（負の対照）ので、**配布物を起動する段のまとまり**に置く。1.5 の段の
# `/S` 導入より後でなければならない（配布物を反証に使うため）。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了
# エラーで段が落ちるようにする。`verify-document-session.ps1` と同じ）。
$ErrorActionPreference = 'Stop'

# **ネイティブコマンドの非 0 終了を例外にしない。** PowerShell 7.4 以降は
# `$PSNativeCommandUseErrorActionPreference` が真だと外部コマンドの非 0 終了が
# `NativeCommandError` 例外になり、**負の対照（非 0 が期待）で `$LASTEXITCODE` を読む前に
# この段が落ちる**（偽の失敗）。既定は偽だが、ランナーの設定に依らないよう明示する。
$PSNativeCommandUseErrorActionPreference = $false

$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
New-Item -ItemType Directory -Force -Path (Split-Path $record) | Out-Null
Write-Host "診断記録: $record"

$verify = "target\release\jxcel.exe"
if (-not (Test-Path $verify)) {
  throw "検証用の形が見つかりません: ${verify}（--features verification-triggers のビルドが先に必要。10.4 の段が作ります）"
}
# 配布物は **1.5 の段が `/S` で導入した実行ファイル**である（Windows に「そのまま実行できる
# 配布物」は無い。NSIS インストーラそのものは実行しても検査の対象にならない）。見つからない
# 場合は 1.5 の段と同じ探索に退避する（導入先が変わっても偽の失敗をしない）。
$shipping = Join-Path $env:LOCALAPPDATA "jxcel\jxcel.exe"
if (-not (Test-Path $shipping)) {
  $fallback = Get-ChildItem -Path (Join-Path $env:LOCALAPPDATA "jxcel") -Filter "jxcel.exe" -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($fallback) { $shipping = $fallback.FullName }
}
if (-not (Test-Path $shipping)) {
  throw "配布物が見つかりません: ${shipping}（1.5 の段の /S 導入が先に必要）"
}
if (-not (Test-Path "scripts\check-grid-observation.sh")) {
  throw "検査器が見つかりません: scripts\check-grid-observation.sh（リポジトリの根から実行してください）"
}

# Git for Windows の実体を探す。**`Get-Command bash` を使わない** — ランナーによっては
# `C:\Windows\System32\bash.exe`（WSL の起動口）が先に見つかり、**別のファイルシステムの bash** で
# 検査器を走らせることになる（リポジトリのパスも記録のパスも通らない）。
#
# `git` の実体（`…\Git\cmd\git.exe` または `…\Git\mingw64\bin\git.exe`）から Git の根を数段
# 辿って求める。根の直下の `bin\bash.exe`（推奨の起動口）と `usr\bin\bash.exe` の両方を見る。
function Find-GitForWindows {
  $roots = New-Object System.Collections.Generic.List[string]
  foreach ($candidate in @(
      "$env:ProgramFiles\Git",
      "${env:ProgramFiles(x86)}\Git",
      "$env:LOCALAPPDATA\Programs\Git")) {
    if ($candidate -and (Test-Path $candidate)) { $roots.Add($candidate) }
  }
  $git = Get-Command git -ErrorAction SilentlyContinue
  if ($git -and $git.Source) {
    $dir = Split-Path $git.Source -Parent
    for ($i = 0; $i -lt 4 -and $dir; $i++) {
      $roots.Add($dir)
      $dir = Split-Path $dir -Parent
    }
  }
  foreach ($root in $roots) {
    foreach ($relative in @("bin\bash.exe", "usr\bin\bash.exe")) {
      $candidate = Join-Path $root $relative
      if (Test-Path $candidate) {
        # **根を一緒に返す** — `cygpath` は根からの相対でしか一意に決まらない
        # （`bin\bash.exe` と `usr\bin\bash.exe` では親の数が 1 つ違う）。
        return @{ Root = $root; Bash = $candidate }
      }
    }
  }
  return $null
}

$git = Find-GitForWindows
if (-not $git) {
  throw "Git for Windows（Git Bash）が見つかりません（3 OS で同じ POSIX sh の検査器を走らせるために必要です）"
}
$bash = $git.Bash
Write-Host "Git Bash: $bash"

# **Windows の綴りのパスを検査器（POSIX sh）へ渡す前に POSIX 形式へ直す。**検査器は記録を
# `[ -f ]` / `wc -l` / `tail` / `grep` で扱い、標本と記録の 3 つを 1 回の走行で行き来するので、
# 綴りを 1 つに寄せておく（`cygpath` は Git for Windows に同梱されている）。取れなければ**元の
# 綴りのまま渡し、そのことを出力に残す**（黙って落ちる経路を作らない）。
$cygpath = Join-Path $git.Root "usr\bin\cygpath.exe"
$cygpathUsable = Test-Path $cygpath
function Convert-ToPosixPath {
  param([string]$Path)
  if (-not $cygpathUsable) { return $Path }
  # **相対パスを先に絶対へ直す** — `cygpath -u` は相対パスの解決をカレントディレクトリに委ねるため、
  # 検査器が別の作業ディレクトリから走ったときに同じファイルを指さなくなる。
  $absolute = $Path
  $resolved = Resolve-Path -LiteralPath $Path -ErrorAction SilentlyContinue
  if ($resolved) {
    $absolute = $resolved.Path
  } elseif (-not [System.IO.Path]::IsPathRooted($Path)) {
    $absolute = Join-Path (Get-Location) $Path
  }
  $converted = @(& $cygpath -u $absolute 2>$null)
  if ($LASTEXITCODE -ne 0 -or $converted.Count -eq 0) { return $Path }
  return (($converted -join '').Trim())
}
if ($cygpathUsable) {
  Write-Host "パスの変換: cygpath = $cygpath"
} else {
  Write-Host "注意: cygpath が見つからないので Windows の綴りのまま検査器へ渡します（MSYS のランタイムは受け付けます）: $cygpath"
}
$recordPosix = Convert-ToPosixPath $record

# 検査器を走らせ、**その出力と終了コードの両方**を返す。`$ErrorActionPreference = 'Stop'` の下では
# 非 0 の外部コマンドが例外になりうるため、この関数の中だけ `Continue` にして `$LASTEXITCODE` を
# 自分で読む（負の対照は非 0 が期待である）。**引き金（`JXCEL_VERIFICATION_*`）は検査器が設定する**
# — この段は環境変数を足さない（3 OS で同じ判定を閉じるため）。`--expect-paint` の値は日本語で
# あるが、PowerShell は `CreateProcessW` の広い綴りで引数を渡し、MSYS のランタイムが内部の UTF-8 へ
# 直すので、コードページを経由しない。
function Invoke-GridObservationCheck {
  param([string]$Exe, [int]$TimeoutSeconds, [string]$ExpectPaint)
  # **自動変数 `$args` を使わない** — 関数の引数の配列と衝突する（配列の展開 `@args` が
  # 検査器の引数ではなく関数の引数を展開しうる）。
  $checkArgs = @("scripts/check-grid-observation.sh", (Convert-ToPosixPath $Exe), $samplePosix,
    $recordPosix, "--timeout=$TimeoutSeconds", "--expect-paint=$ExpectPaint")
  $previous = $ErrorActionPreference
  $ErrorActionPreference = 'Continue'
  try {
    $output = & $bash @checkArgs 2>&1 | Out-String
    $code = $LASTEXITCODE
  } finally {
    $ErrorActionPreference = $previous
  }
  return @{ Output = $output; Code = $code }
}

# **WebView2 の補助プロセスが消えるまで待つ。** WebView2 は利用者データのフォルダを、その
# 補助プロセス（`msedgewebview2.exe`）が閉じるまで握る。前の走行の直後に次の走行を始めると
# `wry` が WebView2 の生成に失敗し（`0x800700AA`「要求されたリソースは使用中です」）、
# **観測の行が 1 行も出ない走行**になる（CI の実測: 観測 2/2 がこれで落ち、記録に観測の行が
# 無かった）。検査器は自分の走行の木しか片付けない（Windows では `pgrep` を持たないので
# 補助プロセスの列挙が効かない）ため、**段がここで待つ**。待つだけで殺さない — 同じ名前の
# プロセスは他の WebView2 のアプリのものでもありうる。
function Wait-WebView2Idle {
  param([int]$TimeoutSeconds = 30)
  $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
  while ((Get-Date) -lt $deadline) {
    $helpers = @(Get-Process -Name "msedgewebview2" -ErrorAction SilentlyContinue)
    if ($helpers.Count -eq 0) { return }
    Start-Sleep -Milliseconds 500
  }
  Write-Warning "WebView2 の補助プロセスが $TimeoutSeconds 秒以内に消えませんでした（次の走行が失敗しうる）"
}

# **標本はリポジトリの中（`target\observation\`）へ書く。** 生成器は `cargo` の側で走るので、
# **リポジトリの外のファイルシステムはアプリと共有されない**環境がある（Linux の段が実測した）。
# `target\` はリポジトリの中で、かつ配布物に入らない（`.gitignore`）。**必ず片付ける**（後続の段へ
# 残さない）。
New-Item -ItemType Directory -Force -Path "target\observation" | Out-Null
$sample = "target\observation\grid-observation-$PID.jxcel"
try {
  Write-Host "標本を作る: 10 万行 × 30 列（1.4 の生成器）"
  & cargo run --release -q -p data-grid --example make-large-sheet --features verification-samples -- 100000 30 $sample
  if ($LASTEXITCODE -ne 0) {
    throw "標本の生成に失敗しました（cargo の終了コード $LASTEXITCODE）: $sample"
  }
  $samplePosix = Convert-ToPosixPath $sample

  Write-Host "観測 1/2: 通常の起動（走査・編集・取り消しの成立）"
  $result = Invoke-GridObservationCheck -Exe $verify -TimeoutSeconds 240 -ExpectPaint "成立"
  Write-Host $result.Output
  if ($result.Code -ne 0) {
    throw "観測 1/2: 検査器が $($result.Code) で落ちました（走査・編集・取り消しの成立が観測できていません）"
  }
  Write-Host "OK: 観測 1/2: 通常の起動で走査・編集・取り消しが成立した"

  Wait-WebView2Idle
  Write-Host "観測 2/2: 描画を成立させない条件の起動（要件 12.2 / 12.3）"
  # **走行ごとに WebView2 の利用者データのフォルダを分ける。**前の走行の終わりは Git Bash の
  # `kill` であり、Windows では `TerminateProcess` である（穏やかな終了ではない）ため、
  # WebView2 が利用者データのフォルダを握ったまま残り、次の走行が
  # `HRESULT(0x800700AA)`「要求されたリソースは使用中です」で **WebView2 の生成に失敗する**
  # （CI の実測: 観測の行が 1 行も出ない走行になった）。別のフォルダなら前の走行の状態に
  # 依存しない。**この環境変数を読むかどうかは移植口の実装に依る**ので、効かなければ下の
  # 再試行が受け止める（どちらも判定を弱めない — 観測の行が出ることは変わらず要求する）。
  $webviewData = Join-Path $env:RUNNER_TEMP "jxcel-webview-run2"
  New-Item -ItemType Directory -Force -Path $webviewData | Out-Null
  $env:WEBVIEW2_USER_DATA_FOLDER = $webviewData
  $result = Invoke-GridObservationCheck -Exe $verify -TimeoutSeconds 120 -ExpectPaint "不成立"
  # **「観測の行が出なかった」ときだけ 1 回やり直す。**前の走行の後始末（上記）が間に合わず
  # 移植口の生成に失敗した走行は、**何も観測していない**（製品の欠陥ではない）。やり直しても
  # 観測の行が出なければ、そのまま下の判定で落ちる — **判定は弱めない**（下の 2 つの時点で
  # 観測が成立していることを要求し続ける）。
  if ($result.Code -ne 0 -and $result.Output -match '記録から観測の行') {
    Write-Host "注意: 観測の行が出なかったため、20 秒待って 1 回だけやり直します（前の走行の後始末）"
    Start-Sleep -Seconds 20
    Wait-WebView2Idle
    $result = Invoke-GridObservationCheck -Exe $verify -TimeoutSeconds 120 -ExpectPaint "不成立"
  }
  Write-Host $result.Output
  if ($result.Code -ne 0) {
    throw "観測 2/2: 検査器が $($result.Code) で落ちました（描画不成立の提示と記録が観測できていません）"
  }
  Write-Host "OK: 観測 2/2: 描画を成立させない条件で提示と記録が成立した"

  Wait-WebView2Idle
  # 反証: **配布物は検証専用の初期画面を読まない**（9.7 の片付けの規約）ので、観測の画面は
  # 現れず観測の行は読めない。検査器は非 0 で落ちなければならない（落ちなければ、検査器が観測の
  # 行を本当に見ていないことになる）。
  Write-Host "反証: 導入済みの配布物（検証用の初期画面を読まない）で検査が非 0 で落ちること"
  $result = Invoke-GridObservationCheck -Exe $shipping -TimeoutSeconds 60 -ExpectPaint "成立"
  Write-Host $result.Output
  if ($result.Code -eq 0) {
    throw "配布物に対して検査が成功してしまった（観測の行が無いのに通っている。既定のビルドに検証専用の経路が入っている）"
  }
  if ($result.Code -eq 2) {
    throw "反証が「入力が使えない」で落ちた（検査器が走っていない）"
  }
  # **落ちた理由が「観測の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
  if ($result.Output -notmatch '観測の行') {
    throw "反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）"
  }
  # **配布物が実際に起動してウィンドウを開いたこと**（検査器が出す起動の形跡の行）。
  if ($result.Output -notmatch 'ウィンドウ:') {
    throw "反証で配布物のウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）"
  }
  Write-Host "反証: 期待どおり非 0 で落ちました（配布物は起動したが、観測の画面を要求しないため観測の行が無い）"
} finally {
  Remove-Item -LiteralPath $sample -Force -ErrorAction SilentlyContinue
}

# **残留プロセスの確認**（検査器は Windows では `pgrep` を持たないので木の走査が効かない。
# この段が `Get-Process` で確かめ、残っていれば落として片付ける）。
$stale = @(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue)
if ($stale.Count -ne 0) {
  Write-Host "注意: jxcel のプロセスが $($stale.Count) 件残っています（検査器の片付けの後に残ったもの）"
  $stale | Stop-Process -Force -ErrorAction SilentlyContinue
  Start-Sleep -Seconds 2
  $stale = @(Get-Process -Name "jxcel" -ErrorAction SilentlyContinue)
  if ($stale.Count -ne 0) {
    throw "後始末のあとに jxcel のプロセスが $($stale.Count) 件残っています（後続の段が単一インスタンスの機構に引き継がれます）"
  }
}
Write-Host "OK: 後始末: 常駐インスタンス数=0（残存プロセスなし）"
Write-Host "OK: 3 OS の観測の段（Windows）— 走査・編集・取り消しと、描画不成立の提示と記録が成立した"
