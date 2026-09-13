# Windows の段（tasks.md 5.3 / 要件 8.2、8.3）。判定の本体は POSIX sh の
# `scripts/check-document-session.sh` であり、**この段は Git Bash でそれを呼ぶ** — 3 OS で同じ
# 判定を閉じるための形である（`verification.md`「POSIX スクリプトは Windows でも `shell: bash`
# を明示する」）。段が足すのは OS 固有の 3 つだけである:
#
#   1. 記録の位置（`%LOCALAPPDATA%\com.jxcel.app\logs\jxcel.log`。4.4 の解決）と、
#      **Windows の綴りのパスを検査器へ渡す前の POSIX 形式への変換**
#   2. **配布物の在り処**: Windows の配布物は NSIS インストーラであり、実行できるのは `/S` で
#      導入された `%LOCALAPPDATA%\jxcel\jxcel.exe` である（1.5 の段が導入を済ませている。
#      10.5 以降の段はそれを消費するだけである）。AppImage / `.app` のような「そのまま実行できる
#      配布物」は Windows には無い
#   3. **残留プロセスの確認**: 検査器（POSIX sh）は子孫の列挙に `pgrep` を使うが、既定の
#      Git Bash には無い。したがって**検査器は「確認をスキップした」と明示して続け**、この段が
#      `Get-Process` で確かめる（黙って飛ばさない。検査器の doc「Windows（Git Bash）での残る差」）
#
# # 何を確かめるか（Linux / macOS の段と同じ 3 つ）
#
#   1. **正例**: 検証用の形（`target\release\jxcel.exe`。10.4 の段が
#      `--features verification-triggers` で作ったもの）をドキュメント付きで起動し、引き金
#      （`JXCEL_VERIFICATION_SESSION=open,edit,4,save`）の 6 つの事実が記録されること。検査器は
#      起動の形跡を先に確かめ、区切りの後ろだけを数え、保存のバイト数が雛形と異なること、
#      2 回目の保存（**雛形の新しい写しからの 2 回目の走行**）が同一バイト列であることを要求する。
#      あわせて**この段の側でも**、区切りの後ろに「引き金を読んだ」と「保存した」がそれぞれ
#      ちょうど 2 行であることを数える（検査器の内部判定は走行の直前に取った行数から始まるが、
#      段の側で別の区切りを取れば、前の段が書いた行で満たされる経路が閉じる）。
#   2. **負の対照**: 導入済みの配布物を渡すと検査器が非 0 で落ちること。**落ちた理由が「引き金の
#      行の不在」であること**と、**配布物が起動してドキュメントの窓を開いたこと**まで要求する
#      （起動しなかった実行も事実の欠落で落ちるので、理由を見ないと錠前の実測にならない）。
#   3. **入力が使えない場合**: 存在しない実行ファイルを渡すと 2 で落ちること
#      （`verification.md`「入力が無いに 0 を返さない」）。
#
# # 証拠が何を証明し、何を証明しないか
#
#   - 証明する: **このランナーの実物のアプリ**が、ドキュメントを読み込み、一括の変更を適用し、
#     保存し、そのバイト列が同じ入力の 2 回目で一致すること。判定はアプリ自身の記録と、保存先の
#     ファイルの大きさ・バイト列で行う（画面も UI Automation も読まない — 3 OS で同じ判定を
#     閉じるため）。
#   - 証明しない: 画面の見え方・メニューからの保存・保存先の選択の提示（別の段 / 別のタスク）。
#   - **アクセシビリティの許可・キー注入・前面化は要らない**（この段はそれらを使わない）。
#     10.6 と 10.8 の Windows の段がそれぞれ「キーは WebView2 が消費する」「配布物は `/S` で
#     導入したものを使う」と記している制約の中で、**この段はそのいずれの影響も受けない** —
#     記録を読むだけであり、負の対照は導入済みの実行ファイルを起動するだけだからである。
#   - **限界**: アプリの記録（自己申告）を一次証拠にする。ただし保存が実際に書き出したファイルの
#     側は検査器が自分で読み、バイト数とバイト列を照合するので、記録だけを信じる経路は無い。
#
# # 片付け
#
# 検査器は起動したアプリを**回収まで行って**終了する（POSIX sh の `kill` / `wait` は Git Bash でも
# 効く）。`pgrep` が無いので**木の走査は効かない**（WebView2 の補助プロセスはアプリの死で道連れに
# なるが、保証ではない）。したがってこの段が `Get-Process -Name jxcel` で**残存 0 件**を確かめ、
# 残っていれば落とす（後続の段が単一インスタンスの機構に引き継がれて偽の失敗をするのを防ぐ）。
#
# # 位置
#
# 配布物を起動する段である（負の対照）ので、**配布物を起動する段のまとまり**（下の
# 「配布物の負の対照」の節の直前）に置く。あちらの節は「自分より後ろに配布物を起動する段が無い」
# ことを前提にしているため、あちらより前でなければならない。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了
# エラーで段が落ちるようにする。抽出前は台本側が与えていた。10.6 / 10.8 の段と同じ）。
$ErrorActionPreference = 'Stop'

# **ネイティブコマンドの非 0 終了を例外にしない。** PowerShell 7.4 以降は
# `$PSNativeCommandUseErrorActionPreference` が真だと外部コマンドの非 0 終了が
# `NativeCommandError` 例外になり、**負の対照（非 0 が期待）で `$LASTEXITCODE` を読む前に
# この段が落ちる**（偽の失敗）。既定は偽だが、ランナーの設定に依らないよう明示する。
$PSNativeCommandUseErrorActionPreference = $false

$record = Join-Path $env:LOCALAPPDATA "com.jxcel.app\logs\jxcel.log"
New-Item -ItemType Directory -Force -Path (Split-Path $record) | Out-Null
Write-Host "診断記録: $record"

# 5.2 の引き金が書き換える行数。**保存のバイト数が雛形と変わる値**でなければならない
# （5.2 の実測: 行数 1〜25・39・40 の差分は −1〜+24 で 0 は無い。4 は 2140 B → 2147 B）。
$rows = 4
$template = "crates\document-format\tests\fixtures\golden\v1\anchored.jxcel"
if (-not (Test-Path $template)) { throw "雛形のドキュメントがありません: $template" }

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

# **Windows の綴りのパスを検査器（POSIX sh）へ渡す前に POSIX 形式へ直す。**検査器は記録と雛形を
# `[ -f ]` / `wc -l` / `tail` / `grep` で扱い、保存先のファイルを `cmp` で比べる。MSYS のランタイムは
# `C:\…` も受け付ける（`scripts/ci/windows/verify-bundled-sidecar.sh` が `${LOCALAPPDATA}/jxcel` を
# そのまま POSIX の検査器へ渡している）が、**この段は 1 回の走行で記録・雛形・保存先の 3 つを
# 行き来し、`cmp` の引数にも渡す**ので、綴りを 1 つに寄せておく（`cygpath` は Git for Windows に
# 同梱されている）。取れなければ**元の綴りのまま渡し、そのことを出力に残す**（黙って落ちる
# 経路を作らない）。
$cygpath = Join-Path $git.Root "usr\bin\cygpath.exe"
$cygpathUsable = Test-Path $cygpath
function Convert-ToPosixPath {
  param([string]$Path)
  if (-not $cygpathUsable) { return $Path }
  # **相対パスを先に絶対へ直す** — `cygpath -u` は相対パスの解決をカレントディレクトリに委ねるため、
  # 検査器が別の作業ディレクトリから走ったときに同じファイルを指さなくなる（この段は Git Bash を
  # カレントのまま起動するので現状は一致するが、依存を作らない）。
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
$templatePosix = Convert-ToPosixPath $template

# 記録の行数。**各呼び出しの直前**に取り、それ以降の行だけを調べる（記録は追記式であり、
# 検査器は失敗のときに末尾を出力へダンプするので、出力全体を `grep` すると前の呼び出しの行で
# 満たされうる。10.8 の段と同じ規律）。
function Get-RecordLineCount {
  if (-not (Test-Path $record)) { return 0 }
  return @(Get-Content -Path $record -Encoding UTF8 -ErrorAction SilentlyContinue).Count
}
function Get-CountAfter {
  param([int]$Skip, [string]$Pattern)
  if (-not (Test-Path $record)) { return 0 }
  $all = @(Get-Content -Path $record -Encoding UTF8 -ErrorAction SilentlyContinue)
  if ($all.Count -le $Skip) { return 0 }
  return @($all[$Skip..($all.Count - 1)] | Where-Object { $_ -match $Pattern }).Count
}

# 検査器を走らせ、**その出力と終了コードの両方**を返す。`$ErrorActionPreference = 'Stop'` の下では
# 非 0 の外部コマンドが例外になりうるため、この関数の中だけ `Continue` にして `$LASTEXITCODE` を
# 自分で読む（負の対照は非 0 が期待である）。
function Invoke-SessionCheck {
  param([string]$Exe, [int]$TimeoutSeconds)
  # **自動変数 `$args` を使わない** — 関数の引数の配列と衝突する（配列の展開 `@args` が
  # 検査器の引数ではなく関数の引数を展開しうる）。
  $exePosix = Convert-ToPosixPath $Exe
  $checkArgs = @("scripts/check-document-session.sh", $exePosix, "$TimeoutSeconds",
    $recordPosix, $templatePosix, "$rows")
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

Write-Host "--- 1/3 正例: 検証用の形で引き金の走行を 2 回行う（読み込み → 一括の適用 → 保存。2 回目は決定性）"
$beforePos = Get-RecordLineCount
$result = Invoke-SessionCheck -Exe $verify -TimeoutSeconds 60
Write-Host $result.Output
if ($result.Code -ne 0) {
  throw "正例: 検査器が $($result.Code) で落ちました（検証用の形で引き金の走行が成立していません）"
}
# **この段の区切り**でも事実を数える（前の段の行では満たせない）。
$readCount = Get-CountAfter -Skip $beforePos -Pattern ("引き金を読んだ: 書き換える行数 = " + $rows + "$")
if ($readCount -ne 2) {
  throw "正例で「引き金を読んだ」の行が 2 行ではありません（観測 $readCount 行。2 回の走行で 2 行が期待。前の段の行では満たせない）"
}
$saveCount = Get-CountAfter -Skip $beforePos -Pattern '保存した: バイト数 = '
if ($saveCount -ne 2) {
  throw "正例で「保存した」の行が 2 行ではありません（観測 $saveCount 行。2 回の走行で 2 行が期待）"
}
Write-Host "OK: 正例: 検証用の形で 2 回の走行が成立し、6 つの事実がそれぞれ 2 行そろった"

Write-Host "--- 2/3 負の対照: 導入済みの配布物を渡すと検査器が非 0 で落ちること"
$beforeNeg = Get-RecordLineCount
$result = Invoke-SessionCheck -Exe $shipping -TimeoutSeconds 45
Write-Host $result.Output
if ($result.Code -eq 0) {
  throw "配布物に対して検査器が成功してしまった（既定のビルドに検証専用の引き金が入っている）"
}
if ($result.Code -eq 2) {
  throw "負の対照が「入力が使えない」で落ちた（検査器が走っていない）"
}
# **落ちた理由が「引き金の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
if ($result.Output -notmatch "'引き金を読んだ' の行が 1 行ではありません") {
  throw "負の対照が「引き金の行の不在」以外の理由で落ちた（検査器が錠前を見ていない可能性がある）"
}
# **配布物が実際に起動してドキュメントの窓を開いたこと**（検査器が出す起動の形跡の行）。
if ($result.Output -notmatch '起動の形跡: .*ウィンドウを開いた: label=doc-') {
  throw "負の対照で配布物が起動した形跡がありません（起動しなかった実行を錠前の実測と取り違えない）"
}
if ((Get-CountAfter -Skip $beforeNeg -Pattern '引き金を読んだ: 書き換える行数 = ') -ne 0) {
  throw "配布物が検証専用の引き金を読んでいます（既定のビルドに検証専用の経路が入っている）"
}
Write-Host "OK: 反証: 配布物は起動してドキュメントの窓を開いたが、引き金を読まないため検査器が非 0 で落ちた"

Write-Host "--- 3/3 入力が使えない場合: 存在しない実行ファイルなら 2 で落ちること"
$result = Invoke-SessionCheck -Exe (Join-Path (Get-Location) "no-such-verification-build.exe") -TimeoutSeconds 10
Write-Host $result.Output
if ($result.Code -ne 2) {
  throw "入力が使えない場合に 2 を返しませんでした（返した値: $($result.Code)。1 = 逸脱 / 0 = 適合はどちらも誤り — 入力の不在を適合とも逸脱とも報告しない）"
}
Write-Host "OK: 入力が使えない場合（実行ファイルの不在）は 2 で落ちた"

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
Write-Host "OK: Windows: 引き金の走行（正例 2 回・負の対照・入力の不在）を 3 つとも実測した（画面の見え方とメニューからの保存は別の段の担当）"
