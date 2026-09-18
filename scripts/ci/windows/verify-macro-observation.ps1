# Windows の段（**tasks.md 5.2** / 要件 1.2, 1.3, 1.5, 2.1, 2.5, 5.1, 5.5, 6.1, 6.4, 8.2, 8.3,
# （**2.3 は挙げない** — 「戻り値と出力を 1 つの面に提示する」は面の側（4.4 の vitest）が判定する。
#  本段は診断の記録を読み口にしており、戻り値と出力の本文を運ばない。2026-09-18 の再検証の指摘）
# 9.1, 9.2, 9.3）。
#
# 実起動のマクロの観測を **Windows のランナーで**閉じる。判定の本体は POSIX sh の
# `scripts/check-macro-observation.sh` であり、**この段は Git Bash でそれを呼ぶ** — 3 OS で同じ
# 判定を閉じるための形である（`verification.md`「POSIX スクリプトは Windows でも `shell: bash` を
# 明示する」）。**この段は Linux の段 `scripts/ci/linux/verify-macro-observation.sh` の鏡であり、
# 恒久である**（`data-grid` の 9.2 の段と同じ形に乗っている。2 つ目の流儀を作らない）。
#
# # **この段は Windows では走らせていない（この開発機に Windows の実行環境も `pwsh` も無い）**
#
# 実測したのは **Linux の段だけ**である。ここで確かめられるのは目視と、兄弟の段
# （`scripts/ci/windows/verify-grid-observation.ps1` / `verify-document-session.ps1`）との対比
# だけである。**実行は CI の Windows ランナーでのみ**確かめる（`verification.md`
# 「ローカルで閉じられないもの」）。macOS の段も同じ理由で走らせていない。
#
# 段が足すのは OS 固有の 2 点だけである（9.2 の Windows の段と同じ規律）:
#
#   1. **記録の位置**（`%LOCALAPPDATA%\com.jxcel.app\logs\jxcel.log`。4.4 の解決）と、
#      **Windows の綴りのパスを検査器へ渡す前の POSIX 形式への変換**
#   2. **標本の生成**（標準と予算の 2 種を 5.1 の生成器から `target\observation\` へ書く）
#
# # 何を確かめるか（検査器の doc が正本）
#
#   1. 実行の成功と変更の件数、製品の記録の成否・出力の行数・変更の有無（要件 2.1, 2.6, 5.1, 5.5）
#   2. 失敗の理由とフレーム（要件 9.1–9.3）
#   3. 能力の拒否（要件 8.3, 9.2）
#   4. 打ち切りと、その後の操作可能性（要件 6.1, 6.4）
#   5. 保存と開き直しの往復（要件 1.2, 1.3, 1.5）
#   6. 10 万行 × 30 列の読み + 集計の予算（要件 11.1, 11.3）
#   7. 1 万行 × 30 列の書き換えの予算（要件 11.2, 11.3）
#
# **判定は要件値で行う**（検査器が持つ。この段は判定を足さない）。**引き金
# （`JXCEL_VERIFICATION_*`）は検査器が設定する** — この段は環境変数を足さない。
#
# # `python3` を先に確かめる（この検査は観測の行の JSON を python3 で読む）
#
# 検査器は観測の行（JSON 1 行。入れ子の `frames` と自由な文言の `reason` を含む）と製品の記録の
# 行を **python3** で読む（9.2 の検査器と同じ前提であり、3 OS のランナーに在る）。**在ることを
# この段が先に確かめる** — 確かめずに走らせると、検査器が「入力が使えない」の 2 で落ち、
# **判定に到達していない**のに逸脱と紛らわしい失敗になる（切り分けに CI の往復が要る）。
#
# # 反証（この段が空回りしていないことの錠前）
#
# **既定 feature のビルドを渡すと検査器は非 0 で落ちる。** そのビルドは検証専用の環境変数を
# 読まないので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** — 起動して
# いない実行も同じ理由で落ちるためである。したがってこの段は、(1) 落ちた理由が「観測の行の不在」
# であることと、(2) **そのビルドが実際に起動してウィンドウを開いたこと**（検査器が出す
# `ウィンドウ:` の行）の両方を要求する。
#
# **負の対照の実行ファイルはこの段が現ソースから作る**（`cargo build --release -p jxcel
# --features tauri/custom-protocol`。既定 feature ＋ Tauri の production 経路）。**NSIS で導入した配布物の出来合いを渡してはならない** — 2026-09-18 の独立検証で、
# Linux の段が使っていた配布物が**マクロ機能そのものを持たない機能前のビルド**であり、
# 「観測の行が現れない」が機能の不在でも成立してしまっていた（錠前が空回りした。§5 の指摘）。
# 使う前に (a) 機能を含むこと（`macro_list` の文字列があること）と (b) 検証専用の識別子を
# 含まないこと（`JXCEL_VERIFICATION` が 0 件）と (c) **フロントエンドの資産を埋め込んでいる
# こと**（`dist\assets\index-*.js` の名前が実行ファイルにあること）を確かめ、確かめられなければ
# 明示的に失敗する。(c) が要るのは、`tauri/custom-protocol` を付けない `cargo build` が
# **フロントエンドを埋め込まない**ためである（窓は開くがフロントエンドが走らず、錠前が
# 「フロントエンドが無いから観測の行が現れない」で成立してしまう。Linux の段が 2026-09-18 に
# 実測した）。したがってビルドは `--features tauri/custom-protocol` を付けて行う。
#
# **文字列の検査は Git Bash の `grep -a -c` で行う。** `strings` は Git for Windows に
# 同梱されていない（Linux / macOS の段は `strings -a` を使う）。`-c` は最後まで読む —
# `-q` は一致した時点で読むのをやめ、書き手が EPIPE で非 0 終了する経路であり、10.5 の
# macOS の段が偽の失敗として実測している。
#
# # 検査器側の既知の差に触れないこと
#
# 検査器（POSIX sh）は Windows では子孫の列挙に `pgrep` を使えない差を**自分で明示する**
# （残留の確認をスキップしたことを出力に残す）。この段はその差を埋めない。**例外は後始末の確認で
# ある** — 検査器の木の走査が効かない以上、残った WebView2 の補助プロセスを後続の段が単一
# インスタンスの機構と取り違えうる。したがってこの段が `Get-Process -Name jxcel` で残存 0 件を
# 確かめ、残っていれば落とす（9.2 の Windows の段と同じ）。
#
# # 位置
#
# 配布物を起動する段のまとまりに置く（既定 feature のビルドを自分で作るので、配布物には
# 依存しない。順序の制約は「検証用の形がビルドされていること」だけである）。

# **この 1 行は Actions の `shell: pwsh` が生成する台本の先頭と同じである**（`throw` などの終了
# エラーで段が落ちるようにする。9.2 / 5.3 の段と同じ）。
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
  throw "検証用の形が見つかりません: ${verify}（--features verification-triggers のビルドが先に必要です）"
}
if (-not (Test-Path "scripts\check-macro-observation.sh")) {
  throw "検査器が見つかりません: scripts\check-macro-observation.sh（リポジトリの根から実行してください）"
}

# Git for Windows の実体を探す。**`Get-Command bash` を使わない** — ランナーによっては
# `C:\Windows\System32\bash.exe`（WSL の起動口）が先に見つかり、**別のファイルシステムの bash** で
# 検査器を走らせることになる（リポジトリのパスも記録のパスも通らない）。
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
        # **根を一緒に返す** — `cygpath` は根からの相対でしか一意に決まらない。
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

# **検査器が要る道具を先に確かめる**（この検査は観測の行の JSON を python3 で読む。9.2 の検査器も
# 同じ前提を持ち、3 OS のランナーに在る）。無ければ**判定に到達していない**ので、はっきり言う。
& $bash -c "command -v python3 >/dev/null 2>&1"
if ($LASTEXITCODE -ne 0) {
  throw "Git Bash から python3 が見つかりません（この検査は観測の行の JSON を python3 で読みます。3 OS のランナーには在る前提です）"
}

# **Windows の綴りのパスを検査器（POSIX sh）へ渡す前に POSIX 形式へ直す。**検査器は記録を
# `[ -f ]` / `wc -l` / `tail` / `grep` で扱い、標本と記録を行き来するので、綴りを 1 つに寄せて
# おく（`cygpath` は Git for Windows に同梱されている）。取れなければ**元の綴りのまま渡し、
# そのことを出力に残す**（黙って落ちる経路を作らない）。
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
# — この段は環境変数を足さない（3 OS で同じ判定を閉じるため）。**予算の標本はどの走行でも渡す**
# （検査器が必須にする）。
function Invoke-MacroObservationCheck {
  param([string]$Exe, [string]$SamplePosix, [string]$BudgetPosix, [int]$TimeoutSeconds)
  # **自動変数 `$args` を使わない** — 関数の引数の配列と衝突する。
  $checkArgs = @("scripts/check-macro-observation.sh", (Convert-ToPosixPath $Exe), $SamplePosix,
    $recordPosix, "--budget-document=$BudgetPosix", "--timeout=$TimeoutSeconds")
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

# 実行ファイルに ASCII の文字列が何回現れるかを数える（**負の対照の錠前**に使う）。
#
# `strings` は Git for Windows に同梱されていないので、**Git Bash の `grep -a -c` を使う**
# （Linux / macOS の段は `strings -a` を使う。同じ規律の別の道具である）。`-c` は最後まで読む —
# `-q` は一致した時点で読むのをやめ、書き手が EPIPE で非 0 終了する経路であり、10.5 の macOS の
# 段が偽の失敗として実測している。**引数は `bash -c` の中へ埋め込まず、位置引数として渡す**
# （パスに引用符が現れても壊れない）。
function Get-BinaryMarkerCount {
  param([string]$Path, [string]$Pattern)
  $posix = Convert-ToPosixPath $Path
  $count = & $bash -c 'grep -a -c -- "$1" "$2" || true' _ $Pattern $posix | Select-Object -First 1
  return [int]("$count".Trim())
}

# **WebView2 の補助プロセスが消えるまで待つ。** WebView2 は利用者データのフォルダを、その補助
# プロセス（`msedgewebview2.exe`）が閉じるまで握る。前の走行の直後に次の走行を始めると `wry` が
# WebView2 の生成に失敗し（`0x800700AA`「要求されたリソースは使用中です」）、**観測の行が 1 行も
# 出ない走行**になる（CI の実測。9.2 の段が記録している）。**この検査は 1 回で 6 回アプリを
# 起動する**ので、段は検査の前後にここで待つ。待つだけで殺さない — 同じ名前のプロセスは他の
# WebView2 のアプリのものでもありうる。**検査器自身も走行ごとに利用者データのフォルダを分ける**
# （`WEBVIEW2_USER_DATA_FOLDER`）ので、二重に守る。
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
# `target\` はリポジトリの中で、かつ配布物に入らない（`.gitignore`）。**必ず片付ける。**
New-Item -ItemType Directory -Force -Path "target\observation" | Out-Null
$sample = "target\observation\macro-observation-$PID.jxcel"
$budget = "target\observation\macro-budget-$PID.jxcel"
# 負の対照の実行ファイルと、検証用の形の退避（下の「反証」）。**必ず片付ける。**
$defaultBinary = "target\observation\jxcel-default-$PID.exe"
$verifySaved = "target\observation\jxcel-verify-$PID.exe"
try {
  Write-Host "標本を作る（標準）: シート「在庫」（3 行 × 2 列）＋マクロ 5 件（5.1 の生成器）"
  & cargo run --release -q -p macro-runtime --example make-macro-document --features verification-samples -- $sample
  if ($LASTEXITCODE -ne 0) {
    throw "標本の生成に失敗しました（cargo の終了コード $LASTEXITCODE）: $sample"
  }
  Write-Host "標本を作る（予算）: 10 万行 × 30 列（1.4 の生成器）＋マクロ 2 件（5.1 の生成器 --kind=budget）"
  & cargo run --release -q -p macro-runtime --example make-macro-document --features verification-samples -- --kind=budget $budget
  if ($LASTEXITCODE -ne 0) {
    throw "予算の標本の生成に失敗しました（cargo の終了コード $LASTEXITCODE）: $budget"
  }
  $samplePosix = Convert-ToPosixPath $sample
  $budgetPosix = Convert-ToPosixPath $budget

  Wait-WebView2Idle
  Write-Host "観測: 7 つの筋書き（成功と出力の記録・失敗・拒否・打ち切りと復帰・保存と開き直し・読みの予算・書き換えの予算）"
  $result = Invoke-MacroObservationCheck -Exe $verify -SamplePosix $samplePosix -BudgetPosix $budgetPosix -TimeoutSeconds 180
  Write-Host $result.Output
  if ($result.Code -ne 0) {
    throw "検査器が $($result.Code) で落ちました（実行の成功と出力の行数の記録・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復・予算の 2 本のいずれかが観測できていません）"
  }
  Write-Host "OK: 7 つの筋書きが成立した（判定は検査器が要件値で行う）"

  # -------------------------------------------------------------------------
  # 反証: **既定 feature の現ソースのビルド**は検証専用の環境変数を読まないので、観測の行は
  # 現れず検査器は非 0 で落ちる。**使う前に、その実体が機能を含み検証専用の識別子を含まない
  # ことを確かめる**（錠前が空回りしないように。冒頭の「反証」）。
  #
  # **検証用の形を退避してから**既定 feature をビルドする（`target\release\jxcel.exe` を
  # 上書きするためである）。後続の段は検証用の形を使うので、**同じパスへ戻す**。
  # -------------------------------------------------------------------------
  # **`tauri/custom-protocol` を付ける理由**: これを付けない `cargo build` はフロントエンドの
  # 資産を実行ファイルへ埋め込まない（Tauri の production 経路を選ぶ feature であり、
  # `npx tauri build` が付けるものと同じである）。埋め込まれないと窓は開いてもフロントエンドが
  # 1 行も走らず、錠前が**「引き金を読まないから観測の行が出ない」ではなく「フロントエンドが
  # 無いから出ない」**で成立してしまう（Linux の段が 2026-09-18 に実測した）。**この段は下の
  # 錠前 (c) で埋め込みを確かめる。**
  Write-Host "反証の実行ファイルを作る: 既定 feature の現ソース（cargo build --release -p jxcel --features tauri/custom-protocol）"
  Copy-Item -LiteralPath $verify -Destination $verifySaved -Force
  # **直前に dist を既定の形で作り直す**: tauri/custom-protocol はフロントエンドの資産を
  # dist から実行ファイルへ埋め込むため、この段より前の検証用のビルドが残した dist
  # （JXCEL_VERIFICATION_BUILD=1 で作られたもの）を使うと、既定 feature のビルドなのに
  # 検証専用の識別子が埋め込まれる（2026-09-18 の Linux の段の実測と同じ罠）。順序の罠なので、
  # ここで既定の形へ戻す。JXCEL_VERIFICATION_BUILD を外して呼ぶ。
  $previousVerificationBuild = $env:JXCEL_VERIFICATION_BUILD
  $env:JXCEL_VERIFICATION_BUILD = $null
  & npm run build
  $env:JXCEL_VERIFICATION_BUILD = $previousVerificationBuild
  & cargo build --release -p jxcel --features tauri/custom-protocol
  if ($LASTEXITCODE -ne 0) {
    throw "既定 feature のビルドに失敗しました（cargo の終了コード $LASTEXITCODE）"
  }
  Copy-Item -LiteralPath $verify -Destination $defaultBinary -Force
  Copy-Item -LiteralPath $verifySaved -Destination $verify -Force

  # 錠前 (b): 検証専用の識別子を含まない（既定 feature のビルドである）。
  $verificationMarkers = Get-BinaryMarkerCount -Path $defaultBinary -Pattern "JXCEL_VERIFICATION"
  if ($verificationMarkers -ne 0) {
    throw "反証に使う実行ファイルが検証専用の識別子を含んでいます（JXCEL_VERIFICATION = ${verificationMarkers} 件。既定 feature のビルドではありません）"
  }
  # 錠前 (a): 機能を含む（マクロのコマンドが入っている）。**無ければ錠前が成立しない** —
  # 機能前のビルドを反証に使うと、「観測の行が現れない」が機能の不在でも成立してしまう。
  $macroMarkers = Get-BinaryMarkerCount -Path $defaultBinary -Pattern "macro_list"
  if ($macroMarkers -eq 0) {
    throw "反証に使う実行ファイルにマクロの機能がありません（macro_list の文字列が 0 件。機能前のビルドを錠前に使わない）"
  }
  # 錠前 (c): **フロントエンドの資産が埋め込まれている**（いまの dist の入口の資産の名前が
  # 実行ファイルにある）。無ければ窓は開いてもフロントエンドが走らず、錠前が「フロントエンドが
  # 無いから観測の行が現れない」で成立してしまう（`tauri/custom-protocol` を付けない
  # `cargo build` がその形である）。
  $entryAsset = Get-ChildItem -Path "dist\assets" -Filter "index-*.js" -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if (-not $entryAsset) {
    throw "フロントエンドの資産が見つかりません: dist\assets\index-*.js（先に npm run build が必要です）"
  }
  if ((Get-BinaryMarkerCount -Path $defaultBinary -Pattern $entryAsset.Name) -eq 0) {
    throw "反証に使う実行ファイルにフロントエンドの資産が埋め込まれていません（$($entryAsset.Name) が 0 件。tauri/custom-protocol を付けずにビルドしていませんか）"
  }
  Write-Host "錠前: 反証の実行ファイルは機能を含み（macro_list = ${macroMarkers} 件）、検証専用の識別子を含まず（JXCEL_VERIFICATION = 0 件）、フロントエンドの資産を埋め込んでいる（$($entryAsset.Name)）"

  Wait-WebView2Idle
  Write-Host "反証: 既定 feature の現ソースのビルド（検証専用の引き金を読まない）で検査が非 0 で落ちること"
  $result = Invoke-MacroObservationCheck -Exe $defaultBinary -SamplePosix $samplePosix -BudgetPosix $budgetPosix -TimeoutSeconds 60
  Write-Host $result.Output
  if ($result.Code -eq 0) {
    throw "既定 feature のビルドに対して検査が成功してしまった（観測の行が無いのに通っている。既定のビルドに検証専用の経路が入っている）"
  }
  if ($result.Code -eq 2) {
    throw "反証が「入力が使えない」で落ちた（検査器が走っていない）"
  }
  # **落ちた理由が「観測の行の不在」であること**（起動しなかった実行も事実の欠落で落ちる）。
  if ($result.Output -notmatch '観測の行') {
    throw "反証が「観測の行が読めない」以外の理由で落ちた（検査器が動いていない可能性がある）"
  }
  # **実行ファイルが実際に起動してウィンドウを開いたこと**（検査器が出す起動の形跡の行）。
  if ($result.Output -notmatch 'ウィンドウ:') {
    throw "反証で実行ファイルのウィンドウが観測されていません（起動しなかった実行を錠前の実測と取り違えない）"
  }
  Write-Host "反証: 期待どおり非 0 で落ちました（既定 feature のビルドは起動したが、観測の行が現れない）"
} finally {
  Remove-Item -LiteralPath $sample -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $budget -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $defaultBinary -Force -ErrorAction SilentlyContinue
  Remove-Item -LiteralPath $verifySaved -Force -ErrorAction SilentlyContinue
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
Write-Host "OK: 3 OS の観測の段（Windows）— 実行の成功と変更の件数・失敗の理由とフレーム・能力の拒否・打ち切りとその後の操作・保存と開き直しの往復が成立した"
