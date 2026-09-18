#!/bin/sh
# 実起動のマクロの観測（tasks.md 5.2 / 要件 1.2, 1.3, 1.5, 2.1, 2.4, 2.5, 2.6, 5.1, 5.5,
# （**2.3 は挙げない** — 「戻り値と出力を 1 つの面に提示する」は面の側（4.4 の vitest）が判定する。
#  本段は診断の記録を読み口にしており、戻り値と出力の本文を運ばない。2026-09-18 の再検証の指摘）
# 6.1, 6.4, 8.2, 8.3, 9.1, 9.2, 9.3, 11.1, 11.2, 11.3）。**Linux / macOS / Windows（Git Bash）の
# 3 OS で同じものが走る**（POSIX sh）。
#
# 5.2 は「**実起動の観測を、診断の記録を読み口にして要件値で判定する**」ことを求める
# （`data-grid` の 9.2 が確立した形。`scripts/check-grid-observation.sh` と同じ）。単体テストは
# ここを代替できない — 実行基盤（専用スレッド・isolate・時間の上限）と、製品の面を通した
# 「一覧 → 選択 → 実行」は、実際に起動してはじめて走る。
#
# **要件 2.2（実行中も表の表示と操作を止めない）はこの検査の担当ではない。** この開発機では
# 実起動で UI を押して確かめられない（**WebKitGTK の DOM がアクセシビリティの木へ露出しない**。
# 5.2 が実測した）ので、実装は `src-tauri/src/commands/macro.rs` の `実行中でも表の窓は止まらない`
# が**決定的なテスト**（保持のロックを実行の間握らないことの表明 3 つ）で固定している
# （tasks.md 5.3 の `観測` の行に経緯がある）。
#
# # 使い方
#
#   sh scripts/check-macro-observation.sh <実行ファイル> <標本の文書> <記録ファイル> \
#     --budget-document=<予算の標本の文書> [--timeout=秒]
#
#   - 実行ファイル: **`--features verification-triggers` の検証用の形**
#     （`JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers`
#     が作る `target/release/jxcel[.exe]`）。**既定 feature のビルド（配布物を含む）を渡すと
#     必ず非 0 で落ちる** — それが 5.2 の負の対照である（下の「負の対照」）。
#   - 標本の文書: `cargo run -p macro-runtime --example make-macro-document
#     --features verification-samples -- <出力先>` が書き出す `.jxcel`（シート「在庫」＋
#     シナリオごとのマクロ 5 件）。**検査は渡された標本を書き換えない** — 往復の筋書きは
#     標本の写しを作り、**写しのほう**をアプリに保存させる（渡された標本は読み取り専用である）。
#   - `--budget-document=<道>`: 同じ生成器の**予算の種別**
#     （`--kind=budget <出力先>`。10 万行 × 30 列 ＋ マクロ 2 件）が書き出す `.jxcel`。
#     **必須である** — 予算（要件 11.1 / 11.2）を実起動の観測で判定する筋書きがこの標本を使う。
#   - 記録ファイル: アプリの診断記録（4.4 の保存先の `jxcel.log`）。**消さない** — 起動中の
#     アプリが開いたファイルへ書き続けるため、消すと行が失われる。代わりに起動の直前に
#     行数を取り、**それ以降の行だけ**を調べる（前の走行の行で緑にしない）。
#   - `--timeout=秒`: **1 回の起動**で観測の行がそろうのを待つ上限（既定 120 秒。打ち切りの
#     筋書きは時間の上限 30 秒の満了を待つので、それより十分に長い必要がある）。
#
# # 判定はどの記録の欄から来ているか（**欄が増えても壊れないように、ここに 1 つの表を置く**）
#
# 読み口は **2 つ**ある:
#
#   1. **5.1 の検証専用の観測の 1 行**（`マクロの観測: {…}`。`src/shell/verificationMacroRun.ts`
#      の `MacroObservation` と `src-tauri/src/lifecycle.rs` の
#      `register_macro_observation_listener` が正本）。**失敗の理由とフレームを持つのはこの行
#      だけ**である（製品の記録は要件 8.3 の帰結で理由とフレームを持たない。5.1 の申し送り）。
#   2. **製品の記録の 1 行**（`macro_run: …`。`src-tauri/src/commands/macro.rs` の `describe_run`
#      が正本）。**要件 2.6 はこの行についての要求であり**（成否・出力・変更の有無を 1 件残す）、
#      出力は**行数**で載る（本文は載らない — 利用者のデータを含みうるため。下の「出力の読み方」）。
#
# | 判定する値（要件値） | 記録の欄 | どの要件のためか |
# |---|---|---|
# | `outcome=ran` | `outcome` | 2.1（実行できた） |
# | `listed=5` / `names=…`（保存順の 5 件） | `listed` / `names` | 1.3（実行せずに一覧を提示する） |
# | `changes_set_cells=1`（他 3 つは 0） | `changes.set_cells` ほか | 5.1, 5.5（1 セルだけ書き、その合計を提示する） |
# | `elapsed_ms` が数値である | `elapsedMs` | 11.3（所要を計測できる形にし、予算を実起動の観測で判定できるようにする。**2.3 ではない** — 2.3 は「戻り値を 1 つの面に提示する」であり、面の側（4.4 の vitest）が判定する） |
# | `capabilities` が空（標本は宣言しない） | `capabilities` | 8.2（宣言している能力を提示する） |
# | `outcome=failed` / `layer=execution` / `reason` に例外の文言 | `outcome` / `layer` / `reason` | 9.1（理由） |
# | `frames` の先頭が `line=3` / `macro_name` は実行した名前 | `frames[i].line` / `.macro_name` / `.column` | 9.1, 9.3（ソース上の位置。**保存されたソースの原位置**） |
# | `frames` の 2 段目が `line=6` | `frames[i].line` | 9.3（例外に至る呼び出しの並び） |
# | `layer=host_rejected:fileRead` / `reason` に `file.read` | `layer` / `reason` | 8.3, 9.2（拒んだ能力と API の名前） |
# | `outcome=aborted` / `limit=time` / `elapsed_ms >= 30000` | `outcome` / `limit` / `elapsedMs` | 6.1（時間の上限 30 秒での打ち切り） |
# | 続けて走らせた 2 行目が `outcome=ran` で `changes_set_cells=1` | 2 行目の `outcome` / `changes` | 6.4（打ち切りの後も操作できる） |
# | `[検証] セッション: 保存した: バイト数 = N` の行が 1 行ある | セッションの記録行（`src-tauri/src/session/verification.rs`） | 1.2, 1.5（保存と読み込みの往復。**保存が起きたことの独立した根拠**） |
# | 保存された文書を開き直した一覧が同じ `names` | `names` | 1.2, 1.3（同じ名前で提示する） |
# | 開き直した文書でだけ書くマクロが `changes_set_cells=1` | `changes.set_cells` | 1.5（保存された値が読み戻されている） |
# | **`結果 = ran` / `変更 = 1（セル 1 / …）` / `出力 = 3 行`**（製品の記録。実行 1 回につき 1 行） | `macro_run:` の行の `結果` / `変更` / `出力` | 2.6（成否・出力・変更の有無を 1 件残す） |
# | 失敗・拒否の行が `結果 = failed` で `出力 = (失敗のため記録なし)` | 同上 | 2.6（エンジンが出力を運ばない結果の記録） |
# | 打ち切りの行が `結果 = aborted` で `出力 = (打ち切りのため記録なし)` | 同上 | 2.6 |
# | 出力を出さないマクロの行が `出力 = (なし)` | 同上 | 2.6（出力の有無） |
# | `outcome=ran` / `elapsed_ms <= 10000`（10 万行 × 30 列の読み + 集計） | `outcome` / `elapsedMs` | 11.1, 11.3（予算を**実起動の観測**で判定する） |
# | `outcome=ran` / `elapsed_ms <= 5000`（1 万行 × 30 列の書き換え） | `outcome` / `elapsedMs` | 11.2, 11.3（同じ） |
# | `changes_set_cells=300000`（1 万行 × 30 列） | `changes.set_cells` | 11.2（書き換えの規模そのもの） |
# | 予算の標本の一覧が `listed=2` の `names` | `listed` / `names` | 1.3 |
#
# **欄を増やすときは、この表と同じ作業で `EXPECTED_*` の定数と python の平坦化を直す**
# （下の「平坦化」）。ここに挙げていない欄は判定に使っていない。
#
# # 筋書き（標本のマクロ。`crates/macro-runtime/examples/make-macro-document.rs`）
#
# 標準の種別（`--kind=standard`。マクロ 5 件）:
#
#   1. **成功と変更の件数** — `標本の記入`（1 セル書き、**3 行の出力**を出し、戻り値を返す。
#      要件 2.1, 2.6, 5.1, 5.5）
#   2. **失敗の理由とフレーム** — `標本の失敗`（関数の入れ子の内側 3 行目で投げる。要件 9.1–9.3）
#   3. **能力の拒否** — `標本の拒否`（宣言の無い `host.fileRead` を呼ぶ。要件 8.3, 9.2）
#   4. **打ち切りと、その後の操作可能性** — `標本の打ち切り,標本の記入`（**同じ起動の中で**順に
#      走らせる。要件 6.1, 6.4）。1 件目は終わらない繰り返しに入り 30 秒で打ち切られ、2 件目は
#      その後に走って 1 セル書く — **打ち切りの後のウィンドウが使える**ことの観測である
#      （別の起動で確かめると、確かめているのは「次の起動ができること」になる）
#   5. **保存と開き直しの往復** — 検査が**標本の写し**を 1 つ作り、まず
#      `JXCEL_VERIFICATION_SESSION=open,edit,2,save`（`document-session` の引き金）で**製品の保存の
#      経路**に保存させ、そのあと `標本の往復` を走らせる。往復のマクロは**先頭行の数量が 5 の
#      ときだけ** 1 セル書く（引き金の回転で 3 → 5 になる）ので、保存された文書を開き直して
#      いなければ変更の件数が 0 になり、検査は落ちる（要件 1.2, 1.3, 1.5）
#
# 予算の種別（`--kind=budget`。10 万行 × 30 列のシート ＋ マクロ 2 件。**要件 11.3 の
# 「実起動の観測で予算を判定できるようにする」を閉じる筋書きである**）:
#
#   6. **10 万行 × 30 列の全行読み + 集計** — `標本の予算の読み` を走らせ、観測の行の
#      `elapsedMs` を**要件値（10 秒）で判定する**（要件 11.1, 11.3）。マクロ自身が標本の形
#      （10 万行 × 30 列）を確かめ、違えば例外で終わる — **予算の中に収まったことだけでは
#      「縮んだ標本を速く読んだ」を見分けられない**ためである（検査器は戻り値を読めない。
#      5.1 の観測の行は `value` を運ばない）
#   7. **1 万行 × 30 列の書き換え** — `標本の予算の書き換え` を走らせ、`elapsedMs` を**要件値
#      （5 秒）で判定する**（要件 11.2, 11.3）。あわせて**変更の件数が 30 万セル**であることを
#      要件値として読む（1 万行 × 30 列ぶんの書き込みが実際に届いたことの表明である）
#
# # 読み方（平坦化 — なぜ python3 を使うのか）
#
# **観測の行**は JSON 1 行である（`emit` が直列化した本文をそのまま記録へ写す。10.8 の
# `一括転送の結果:` と同じ形）。`frames` は入れ子の並びであり、`reason` は自由な文言
# （例外のメッセージ）を含むので、**`grep` の正規表現で切り出すと逃がし文字で静かに壊れる**。
# したがって python3 で JSON として読み、**判定に使う欄だけを `観測<番号>:<欄>=<値>` の
# 1 行ずつへ平坦化する**（判定そのものは下の sh が行う — python3 は値を作らない）。
# `python3` は 3 OS のランナーに在る（Git Bash にも在る。9.2 の検査器と同じ前提）。
#
# **製品の記録の行**（`macro_run: …`）も同じ python3 が読む（要件 2.6 の判定材料）。行の形は
# `describe_run` が組み立てる**固定の綴り**であり、` / ` 区切りの欄を**欄の名前を錨にして**
# 切り出す（`記録<番号>:<欄>=<値>`）— マクロの名前や理由に区切りが現れても欄が混ざらない。
# 綴りが変われば `記録<番号>:parse_error=1` になり、**判定は値の欠落として落ちる**（黙って
# 緑にならない）。
#
# **本文は平坦化の都合で 3 箇所だけ畳む**: 並び（`names` / `capabilities`）は `|` で連結し、
# 改行は `\n` に置き換える。標本の名前にも理由にも `|` は現れない（標本の名前は
# `crates/macro-runtime/examples/make-macro-document.rs` が持ち、検査器は同じ並びを定数で持つ）。
#
# # 出力の読み方（要件 2.6）
#
# 製品の記録の行の `出力` は**行数**（`出力 = 3 行`）であり、**本文（値・文字列）は載らない** —
# 診断の記録は利用者のデータを含みうるうえ、**書き出して共有されうる**ためである（design.md
# 「Monitoring」）。本文が要る利用者は同じ実行の面（要件 2.3）を見る。
#
# 行が 0 のときは `出力 = (なし)`、**エンジンが出力の並びを運ばない結果**（失敗・打ち切り。
# `RunOutcome::Failed` / `Aborted`）では `(失敗のため記録なし)` / `(打ち切りのため記録なし)` が
# 載る — **「出力が無かった」とは書かない**（実際には出力の後に失敗したマクロがありうる。
# 記録は**書けなかった理由**を書く）。
#
# # 負の対照（この検査自身が持つ）
#
# **既定 feature のビルド（配布物を含む）を渡すと、この検査は非 0 で落ちる。** そのビルドは
# 検証専用の環境変数を読まない（`cfg(feature = "verification-triggers")` の外に読み取りが
# 無い）ので観測の行が 1 行も現れない。ただし**それだけでは錠前の実測にならない** — 起動して
# いない実行も同じ理由で落ちるためである。したがって段は、(1) 起動の形跡（`ウィンドウを開いた:` の
# 行。**この検査はそれを出力に出す**）が成立していることと、(2) 落ちた理由が**観測の行の不在**
# であることの両方を要求する（`scripts/ci/*/verify-macro-observation.*` を参照）。
#
# **負の対照に使う実行ファイルは段が現ソースから作る**（既定 feature のビルド）。配布物の
# 出来合いを渡してはならない — 2026-09-18 の独立検証で、Linux の段が使った AppImage が
# **マクロ機能そのものを持たない機能前のビルド**（`macro_list` の文字列が 0 件）であり、
# 「観測の行が現れない」が**機能の不在**でも成立してしまっていた（錠前が空回りした）。段は
# 使う前に、(a) **機能を含むこと**（`macro_list` の文字列があること）と、(b) **検証専用の
# 識別子を含まないこと**（`JXCEL_VERIFICATION` が 0 件）を確かめ、確かめられなければ明示的に
# 失敗する（機能の不在で錠前を成立させない）。
#
# # 片付け
#
# 起動したアプリと作業領域（標本の写し）は**トラップで必ず片付ける**。残すと後続の段が
# 単一インスタンスの機構に引き継がれ、次の起動が何も記録しない（偽の失敗。10.5 が実測した）。
# 木の走査は `scripts/lib/x11-window.sh` の `x11_kill_tree` を**そのまま使う**（写すと片方だけ
# 直る。`verification.md`「観測と後始末の共有部分は `scripts/lib/` に置く」）。
#
# ## Windows（Git Bash）での残る差（正直に記す）
#
#   - 置き場の `x11_kill_tree` は子孫の列挙に `pgrep` を使う。Git Bash には既定で `pgrep` が
#     無いため、Windows では**起動したプロセスそのもの**だけが終了する。段の側が
#     `Get-Process -Name jxcel` で残存を確かめて落とす（OS 固有の部分は段に置く）。
#   - 残留の確認（`wait_for_no_app`）も `pgrep` を要する。無ければ**確認をスキップしたことを
#     明示して**続ける（黙って飛ばさない）。
#
# # ローカルで閉じられないもの
#
# macOS / Windows での実行はこの開発機では行えない（`verification.md`「ローカルで閉じられない
# もの」）。この検査は 3 OS で同じものが走るが、**走らせた事実は OS ごとに段が残す**。
#
# # 前提
#   - Linux では `DISPLAY` が要る（GUI の無いランナーでは `xvfb-run` の下で呼ぶ）。macOS /
#     Windows では要求しない（この検査は画面もウィンドウツリーも読まない — **記録だけを読む**）。
#   - `python3`（観測の行の JSON と製品の記録の行を読む）。`pgrep`（あれば残留の確認と木の走査に使う）。
#
# # 終了コード
#   0 = 7 つの筋書きがすべて適合 / 1 = 逸脱（起動の形跡が無い・観測の行が現れない・要件値と
#   食い違う）/ 2 = 入力が使えない（引数・実行ファイル・標本・予算の標本・記録・DISPLAY・
#   python3 の不在・未対応の OS）
#
# SC1091 / SC2154: 置き場は**同じリポジトリのファイル**であり、`x11_kill_tree` /
# `x11_pick_poll_sleep` はそこで代入される。qlty は検査対象を一時ディレクトリへ写してから
# 静的解析にかけるため、置き場をたどれず「たどれない・未代入」と報告する（実際の実行では
# `$0` からの相対で解決する。`scripts/check-document-session.sh` と同じ契約）。
# shellcheck disable=SC1091,SC2154
set -eu

# ---------------------------------------------------------------------------
# 要件値（**判定はこの定数と記録の欄だけで行う**。冒頭の表を参照）
# ---------------------------------------------------------------------------

# 標本のマクロの並び（保存順）。**`crates/macro-runtime/examples/make-macro-document.rs` の
# `SPECIMEN_MACRO_NAMES` と同じ並びでなければならない** — 標本へマクロを足す・名前を変える
# ときは、この定数と `EXPECTED_LISTED` を**同じ作業で**直す（片方だけ直すと落ちるので、写しが
# 黙って古くならない。`check-menu-shortcut.sh` の `EXPECTED` と同じ規律）。
EXPECTED_NAMES='標本の記入|標本の往復|標本の拒否|標本の失敗|標本の打ち切り'

# 一覧の件数（要件 1.3。`listed` の欄）。
EXPECTED_LISTED=5

# 予算の種別の標本のマクロの並び（保存順）。**`crates/macro-runtime/examples/make-macro-document.rs`
# の `BUDGET_MACRO_NAMES` と同じ並びでなければならない**（標準の並びと同じ規律）。
EXPECTED_BUDGET_NAMES='標本の予算の読み|標本の予算の書き換え'
# 予算の種別の標本の一覧の件数（要件 1.3）。
EXPECTED_BUDGET_LISTED=2

# **予算（要件 11.1 / 11.2 の要件値そのもの。ミリ秒）。**
#
# 観測の行の `elapsedMs` がこれ以下であることを要求する（要件 11.3 の「予算を実起動の観測で
# 判定できるようにする」）。**閾値を緩めない** — 緩めれば予算を満たさない実装が緑になる。
# criterion のベンチ（`crates/macro-runtime/benches/bulk.rs`）と同じ要件値であり、判定の場が
# 2 つある（ベンチは計測の分布で、この検査は実起動の 1 回で見る）。
BUDGET_READ_MS=10000
BUDGET_REWRITE_MS=5000
# 書き換えの規模（1 万行 × 30 列 = 30 万セル。要件 11.2 の規模そのもの）。
EXPECTED_BUDGET_CELLS=300000

# `標本の記入`（シナリオ 1 と 4 の 2 件目）が出す出力の行数（要件 2.6。標本のマクロが
# `console.log` / `info` / `warn` を 1 行ずつ出す）。
EXPECTED_OUTPUT_LINES=3
# 出力の欄の綴り（要件 2.6）。**エンジンが出力の並びを運ばない結果では「無かった」と
# 書かない** — `describe_run` が書けなかった理由を書く（検査器はその綴りを要件値として読む）。
EXPECTED_OUTPUT_NONE='(なし)'
EXPECTED_OUTPUT_FAILED='(失敗のため記録なし)'
EXPECTED_OUTPUT_ABORTED='(打ち切りのため記録なし)'
# 変更の欄の綴り（要件 2.6 の「変更の有無」。標準の標本の記入は 1 セルだけ書く）。
EXPECTED_RECORD_CHANGES='1（セル 1 / 追加 0 / 削除 0 / 複製 0）'
EXPECTED_RECORD_NO_CHANGES='0（セル 0 / 追加 0 / 削除 0 / 複製 0）'
EXPECTED_BUDGET_RECORD_CHANGES='300000（セル 300000 / 追加 0 / 削除 0 / 複製 0）'

# `標本の記入` が書くセルの数（要件 5.1, 5.5。`changes.set_cells` の欄）。
EXPECTED_CELLS=1
# `標本の往復` が**保存された文書を開き直したときだけ**書くセルの数（要件 1.5。同じ欄）。
EXPECTED_ROUNDTRIP_CELLS=1

# `標本の失敗` の投げる位置（**保存されたソースの原位置**。要件 9.1, 9.3）。
# 標本のソースは 3 行目の `  throw new Error("検証用の失敗");` で投げ、6 行目の
# `  return 内側();` から呼ぶ（`crates/macro-runtime/examples/make-macro-document.rs`）。
# **行と列は 1 起点**である（`MacroFrame` の契約）。列は**投げる式とその呼び出しの位置**であり、
# 実測（2026-09-18。Linux）で `throw` の行は 9 列目（`  throw new Error(...)` の `new`）、
# 呼び出しは 10 列目（`  return 内側();` の `内側`）である。
EXPECTED_FAILURE_LINE=3
EXPECTED_FAILURE_COLUMN=9
EXPECTED_FAILURE_CALL_LINE=6
EXPECTED_FAILURE_CALL_COLUMN=10
# 例外の文言（`reason` の欄。9.1 の「理由」）。
EXPECTED_FAILURE_REASON='検証用の失敗'

# `標本の拒否` の拒否の層（要件 8.3, 9.2）。層は `host_rejected:<拒んだ API の名前>` の形で
# 運ばれる（`verificationMacroRun.ts` の `flattenFailure`）。**能力の名前は `reason` の欄に
# 現れる**（拒否の理由は「ホスト API host.fileRead には能力 file.read の宣言が要る」である。
# 2026-09-18 にエンジンの理由を日本語へ揃えたため、引用を追随させた）。
EXPECTED_REJECTION_LAYER='host_rejected:fileRead'
EXPECTED_REJECTION_CAPABILITY='file.read'
# `標本の拒否` が `host.fileRead` を呼ぶ位置（**保存されたソースの原位置**。9.2）。2 行目の
# `const 本文: string = host.fileRead("/etc/hostname");` の**呼んだ API の名前**の位置であり、
# 実測（2026-09-18。Linux）で 25 列目である。
EXPECTED_REJECTION_LINE=2
EXPECTED_REJECTION_COLUMN=25

# 時間の上限（要件 6.1 の既定 30 秒）。打ち切られた実行の `elapsedMs` がこれ以上であること。
# **閾値を緩めない** — 満了前に打ち切られていれば、それは別の打ち切りである。
ABORT_MIN_MS=30000

# 往復の筋書きで、セッションの引き金に回転させる行数（`open,edit,2,save`。3 行の標本で
# 先頭行の数量が 3 → 5 になる）。
ROUNDTRIP_SESSION_ROWS=2

# 標本の要求の行（**起動の識別**であり、判定ではない）。
REQUEST_MARKER='検証用のマクロの実行を要求した:'
# 起動の形跡（`ウィンドウを開いた: label=…`。3 OS で同じ綴りである。10.4 / 5.3 と同じ読み口）。
WINDOW_MARKER='ウィンドウを開いた:'
# 観測の行の印。**`grep -E` に渡すので、正規表現の記号は使わない**（この印は記号を含まない）。
OBSERVATION_MARKER='マクロの観測:'
# 製品の記録の行の印（要件 2.6。`src-tauri/src/commands/macro.rs` の `describe_run`）。**同じく
# 正規表現の記号を含まない**。
MACRO_RECORD_MARKER='macro_run:'
# セッションの引き金の保存の行（往復の筋書きの錠前。`src-tauri/src/session/verification.rs`）。
SESSION_SAVED_MARKER='\[検証\] セッション: 保存した: バイト数 = '

_x11_lib_dir=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
# 片付け（木の走査）と検出粒度は**置き場の 1 実装を使う**（この検査はウィンドウを観測しないので、
# 置き場から読むのはこの 2 つだけである）。
. "$_x11_lib_dir/lib/x11-window.sh"

usage() {
  echo "使い方: sh scripts/check-macro-observation.sh <実行ファイル> <標本の文書> <記録ファイル> --budget-document=<予算の標本の文書> [--timeout=秒]" >&2
  exit 2
}

if [ "$#" -lt 3 ]; then
  usage
fi

app=$1
document=$2
record=$3
shift 3
timeout_seconds=120
budget_document=''
for argument in "$@"; do
  case "$argument" in
    --timeout=*)
      timeout_seconds=${argument#--timeout=}
      ;;
    --budget-document=*)
      budget_document=${argument#--budget-document=}
      ;;
    *)
      echo "NG: 解釈できない引数です: $argument" >&2
      usage
      ;;
  esac
done

case "$timeout_seconds" in
  '' | *[!0-9]*)
    echo "NG: タイムアウトが秒の整数ではありません: $timeout_seconds" >&2
    exit 2
    ;;
esac
if [ "$timeout_seconds" -le 0 ]; then
  echo "NG: タイムアウトが 0 以下です: $timeout_seconds" >&2
  exit 2
fi

if [ ! -f "$app" ] || [ ! -x "$app" ]; then
  echo "NG: 実行ファイルがありません（実行権限も必要です）: $app" >&2
  echo "    --features verification-triggers のビルド（例 JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers）を先に行ってください" >&2
  exit 2
fi
if [ ! -f "$document" ]; then
  echo "NG: 標本の文書がありません: $document" >&2
  echo "    cargo run -p macro-runtime --example make-macro-document --features verification-samples -- <出力先> で作ってください" >&2
  exit 2
fi
if [ -z "$budget_document" ]; then
  echo "NG: 予算の標本の文書が指定されていません（--budget-document=<道>。要件 11.1–11.3 の筋書きが使います）" >&2
  echo "    cargo run -p macro-runtime --example make-macro-document --features verification-samples -- --kind=budget <出力先> で作ってください" >&2
  exit 2
fi
if [ ! -f "$budget_document" ]; then
  echo "NG: 予算の標本の文書がありません: $budget_document" >&2
  exit 2
fi
if [ -z "$record" ]; then
  echo "NG: 記録ファイルが指定されていません（アプリの診断記録の保存先を渡してください）" >&2
  exit 2
fi
if ! command -v python3 >/dev/null 2>&1; then
  echo "NG: python3 がありません（観測の行は JSON 1 行であり、入れ子の欄を読むのに使います。3 OS のランナーに在ります）" >&2
  exit 2
fi

case "$(uname -s 2>/dev/null || echo unknown)" in
  Linux)
    os_mode=linux
    ;;
  Darwin)
    os_mode=macos
    ;;
  MINGW* | MSYS* | CYGWIN*)
    os_mode=windows
    ;;
  *)
    echo "NG: この検査は Linux / macOS / Windows（Git Bash）用です（uname -s = $(uname -s 2>/dev/null || echo unknown)）" >&2
    exit 2
    ;;
esac

# Linux では表示サーバが要る（GUI アプリを起動できない）。macOS / Windows は OS が画面を持ち、
# この検査は画面を読まないので要求しない（要求すると macOS の段が**観測の前に** 2 で落ちる）。
if [ "$os_mode" = linux ] && [ -z "${DISPLAY:-}" ]; then
  echo "NG: DISPLAY が設定されていません（GUI アプリを起動できません。CI では xvfb-run の下で呼ぶ）" >&2
  exit 2
fi

# **親の環境から検証専用の引き金が漏れないようにする。**とくに `JXCEL_VERIFICATION_EXIT_AFTER_MS`
# は走行の途中でアプリを終わらせ、記録を断ち切る（＝偽の失敗）。この検査は自分が使う引き金だけを
# 起動の行で渡す（親の値に依らない）。
unset JXCEL_VERIFICATION_DENY_CLOSE 2>/dev/null || true
unset JXCEL_VERIFICATION_INITIAL_SCREEN 2>/dev/null || true
unset JXCEL_VERIFICATION_EXIT_AFTER_MS 2>/dev/null || true
unset JXCEL_VERIFICATION_BULK_ROWS 2>/dev/null || true
unset JXCEL_VERIFICATION_GRID_OBSERVATION 2>/dev/null || true
unset JXCEL_VERIFICATION_GRID_PAINT_FAILURE 2>/dev/null || true
unset JXCEL_VERIFICATION_GRID_PASTE 2>/dev/null || true
unset JXCEL_VERIFICATION_MACRO_RUN 2>/dev/null || true
unset JXCEL_VERIFICATION_SESSION 2>/dev/null || true

x11_pick_poll_sleep
poll_sleep=$x11_poll_sleep

work=$(mktemp -d "${TMPDIR:-/tmp}/jxcel-macro-observation.XXXXXX")
app_pid=''
app_log=''
# 往復の筋書きが書き換えるのは**写し**である（標本をその場で書き換えない）。
roundtrip_document="$work/roundtrip.jxcel"
observations_file="$work/observations.txt"
# 製品の記録の行（要件 2.6 の判定材料。走行ごとに切り出す）。
product_records_file="$work/records.txt"
parsed=''
recorded=''

# 起動したアプリを終了し、**回収する**（`wait` まで行うので、戻った時点でプロセスは確実に
# 消えている。生きていると単一インスタンスの機構が次の起動を引き継ぎ、次の走行は何も記録しない。
# 10.5 が実測した偽の失敗である）。
stop_app() {
  if [ -n "${app_pid:-}" ] && kill -0 "$app_pid" 2>/dev/null; then
    x11_kill_tree "$app_pid"
    ticks=0
    while [ "$ticks" -lt 50 ] && kill -0 "$app_pid" 2>/dev/null; do
      sleep "$poll_sleep"
      ticks=$((ticks + 1))
    done
    if kill -0 "$app_pid" 2>/dev/null; then
      kill -9 "$app_pid" 2>/dev/null || true
    fi
    wait "$app_pid" 2>/dev/null || true
  fi
  app_pid=''
}

# 同じ名前のプロセスが残っていないことを確かめる（**次の起動の前の錠前**）。`pgrep` が無い環境
# （既定の Git Bash）では**確認をスキップしたことを明示して**続ける（「確かめた」と偽らない）。
wait_for_no_app() {
  if ! command -v pgrep >/dev/null 2>&1; then
    echo "注意: pgrep が無いので残留プロセスの確認をスキップします（Windows の段は Get-Process で確かめます）"
    return 0
  fi
  ticks=0
  while [ "$ticks" -lt 50 ] && pgrep -x jxcel >/dev/null 2>&1; do
    sleep "$poll_sleep"
    ticks=$((ticks + 1))
  done
  if pgrep -x jxcel >/dev/null 2>&1; then
    echo "注意: 名前 jxcel のプロセスが残っています（単一インスタンスの機構が次の起動を引き継ぐと、次の走行は何も記録しません）"
  fi
}

# shellcheck disable=SC2329 # 下の trap から呼ばれる（shellcheck は trap を追えない）
cleanup() {
  stop_app
  if [ -n "${work:-}" ] && [ -d "$work" ]; then
    rm -rf "$work"
  fi
  :
}
trap cleanup EXIT INT TERM

# 記録の現在の行数（無ければ 0）。**起動の直前に取る。**
record_lines() {
  if [ -f "$record" ]; then
    wc -l < "$record" | tr -d ' '
  else
    echo 0
  fi
}

# 記録の `<開始行数>` より後ろの行を全部出す。**行末の CR は落とす**（`\r\n` の記録では `$` に
# 錨を置く照合が空振りする。綴りに依存しないようにしておく）。
#
# **回転（8 MB を超えたときの世代ファイルへの切り替え）に耐える。**記録は
# `max_file_size`（8 MB。`src-tauri/src/lifecycle.rs`）を超えると回転し、現行のファイルは
# **回転のあとの行だけ**になる（世代ファイルは `jxcel_<日付>_<時刻>.log`）。区切りの行数を
# 数えるだけでは、回転が起きた瞬間に `tail -n +N` が空を返し、**観測できているのに「観測の行が
# 現れない」と報告する偽の失敗**になる（この検査の実測: 2026-09-18、8 MB を超えていた記録で
# 3 番目の筋書きが落ちた）。したがって現在の行数が区切りより少なければ、**現在のファイル全体**を
# 読む（回転で新しくなったファイルは、その走行の行だけを含む）。
record_slice() {
  from=$1
  if [ ! -f "$record" ]; then
    return 0
  fi
  current=$(wc -l < "$record" | tr -d ' ')
  if [ "$current" -ge "$from" ]; then
    tail -n "+$((from + 1))" "$record" 2>/dev/null || true
  else
    cat "$record" 2>/dev/null || true
  fi
}

# 記録の `<開始行数>` より後ろから、`grep -E` のパターンに一致する行を全部出す。
record_after() {
  record_slice "$1" | tr -d '\r' | grep -E -- "$2" || true
}

# 記録の `<開始行数>` より後ろから、`grep -E` のパターンに一致する行の数。
record_count_after() {
  record_after "$1" "$2" | wc -l | tr -d ' '
}

# アプリ自身の出力（`$app_log`。**走行ごとに新しいファイルである**）から、`grep -E` のパターンに
# 一致する行を出す。**起動の形跡の控えの読み口である** — 記録が回転した直後の走行では、起動の
# 行が世代ファイルの側へ移りうる（判定には使わない事実なので、この検査は**この走行の出力**から
# 読んでよい。アプリは標準出力と保存先の両方へ同じ行を書く）。
app_log_line() {
  if [ -f "$1" ]; then
    tr -d '\r' < "$1" | grep -E -- "$2" | head -n 1 || true
  fi
}

report_failure() {
  echo "NG: $1" >&2
  if [ -n "${app_log:-}" ] && [ -f "$app_log" ]; then
    echo "--- アプリの出力（末尾 20 行） ---" >&2
    tail -n 20 "$app_log" >&2 || true
  fi
  if [ -n "${parsed:-}" ]; then
    echo "--- 観測の行（平坦化） ---" >&2
    printf '%s\n' "$parsed" >&2
  fi
  exit 1
}

# 観測の行と製品の記録の行を平坦化する（**それぞれの形に合った読み方で、判定に使う欄だけを
# 1 行ずつへ写す**。冒頭の「読み方」）。引数は 2 つ: 観測の行を切り出したファイルと、製品の
# 記録の行（`macro_run:`）を切り出したファイル。
flatten_observations() {
  python3 - "$1" "$2" <<'PY'
import json
import re
import sys

LABEL = "マクロの観測: "

# 製品の記録の行の形（`src-tauri/src/commands/macro.rs` の `describe_run` が正本）。
# **欄の名前を錨にして切り出す** — マクロの名前や理由に ` / ` が現れても欄が混ざらない。
# 区切りの空白は許容する（綴りの側の見た目に依らせない）。欄の名前が変われば一致しなくなり、
# `parse_error` として現れる（判定は値の欠落として落ちる）。
PRODUCT = re.compile(
    r"ウィンドウ = (?P<ウィンドウ>.*?)\s*/\s*マクロ = (?P<マクロ>.*?)\s*/\s*種別 = (?P<種別>.*?)\s*/\s*"
    r"結果 = (?P<結果>.*?)\s*/\s*打ち切り = (?P<打ち切り>.*?)\s*/\s*変更 = (?P<変更>.*?)\s*/\s*"
    r"出力 = (?P<出力>.*?)\s*/\s*所要 = (?P<所要>.*) ms$"
)


def joined(values):
    """並びを `|` で連結する（本文に `|` は現れない。冒頭の「読み方」）。"""
    return "|".join(str(value) for value in values)


def text(value):
    """自由な文言（例外のメッセージ）を 1 行へ畳む。"""
    if value is None:
        return ""
    return str(value).replace("\r", " ").replace("\n", "\\n")


def flatten_products(path):
    """製品の記録の行を `記録<番号>:<欄>=<値>` へ写す（要件 2.6 の判定材料）。"""
    index = 0
    with open(path, encoding="utf-8", errors="replace") as handle:
        for line in handle:
            if "macro_run:" not in line:
                continue
            index += 1
            prefix = f"記録{index}:"
            matched = PRODUCT.search(line.rstrip("\n"))
            if not matched:
                print(f"{prefix}parse_error=1")
                continue
            for field in ("ウィンドウ", "マクロ", "種別", "結果", "打ち切り", "変更", "出力"):
                print(f"{prefix}{field}={text(matched.group(field))}")
            print(f"{prefix}所要_ms={text(matched.group('所要'))}")


path = sys.argv[1]
products_path = sys.argv[2]
index = 0
with open(path, encoding="utf-8", errors="replace") as handle:
    for line in handle:
        at = line.find(LABEL)
        if at < 0:
            continue
        payload = line[at + len(LABEL):].strip()
        try:
            observation = json.loads(payload)
        except ValueError as error:
            print(f"parse_error_line{index + 1}={error}")
            continue
        if not isinstance(observation, dict):
            print(f"parse_error_line{index + 1}=not-an-object")
            continue
        index += 1
        prefix = f"観測{index}:"
        print(f"{prefix}requested={text(observation.get('requested'))}")
        listed = observation.get("listed")
        print(f"{prefix}listed={'' if listed is None else listed}")
        print(f"{prefix}names={joined(observation.get('names') or [])}")
        print(f"{prefix}chosen={text(observation.get('chosen'))}")
        print(f"{prefix}capabilities={joined(observation.get('capabilities') or [])}")
        print(f"{prefix}outcome={text(observation.get('outcome'))}")
        changes = observation.get("changes")
        if isinstance(changes, dict):
            for key in ("set_cells", "inserted_rows", "removed_rows", "duplicated_rows"):
                print(f"{prefix}changes_{key}={changes.get(key, '')}")
        elapsed = observation.get("elapsedMs")
        print(f"{prefix}elapsed_ms={'' if elapsed is None else elapsed}")
        print(f"{prefix}limit={text(observation.get('limit'))}")
        print(f"{prefix}layer={text(observation.get('layer'))}")
        print(f"{prefix}reason={text(observation.get('reason'))}")
        frames = observation.get("frames")
        if not isinstance(frames, list):
            frames = []
        print(f"{prefix}frames={len(frames)}")
        for position, frame in enumerate(frames):
            if not isinstance(frame, dict):
                continue
            print(f"{prefix}frame{position}_macro={text(frame.get('macro_name'))}")
            print(f"{prefix}frame{position}_function={text(frame.get('function'))}")
            print(f"{prefix}frame{position}_line={frame.get('line', '')}")
            print(f"{prefix}frame{position}_column={frame.get('column', '')}")
print(f"観測の行数={index}")
flatten_products(products_path)
PY
}

# 平坦化した 1 つの値（`観測<番号>:<欄>`）。無ければ空文字。
obs_value() {
  printf '%s\n' "$parsed" | sed -n "s/^観測$1:$2=//p" | head -n 1
}

# 平坦化した製品の記録の値（`記録<番号>:<欄>`）。無ければ空文字。**要件 2.6 の判定材料**。
record_value() {
  printf '%s\n' "$parsed" | sed -n "s/^記録$1:$2=//p" | head -n 1
}

# 起動 1 回分。引数は「起動引数の文書」「待つ印（`grep -E`）」「待つ行数」「待つの呼び名」
# 「渡す引き金（`名前=値` の並び。0 個以上）」である。**起動の直前に記録の行数を取り、そのあと
# だけを読む。**
#
# 引き金は `env` の要領で**この検査が自分で渡す**（親の値に依らない）。どの走行も初期画面を
# グリッドにする — マクロの変更を適用するには、そのウィンドウでグリッドがシートを開いている
# ことが要る（`window/mod.rs` の `macro_run_script` の doc）。**待てなかったら止まる**（呼び出し
# 側は続けても意味が無い。判定は「そろわなかった」ことそのものである）。
launch_wait() {
  launch_document=$1
  wait_marker=$2
  expected_lines=$3
  wait_what=$4
  shift 4
  run_serial=$((run_serial + 1))
  app_log="$work/app-${run_serial}.log"
  baseline=$(record_lines)
  # **走行ごとに WebView2 の利用者データのフォルダを分ける**（Windows のため。前の走行の終わりは
  # `kill` であり、Windows では `TerminateProcess` であるため、WebView2 が利用者データの
  # フォルダを握ったまま残り、次の起動が `HRESULT(0x800700AA)`「要求されたリソースは使用中です」
  # で **WebView2 の生成に失敗する**（9.2 の段が CI で実測した — そのときは観測の行が 1 行も
  # 出ない走行になった）。Linux / macOS はこの環境変数を読まないので、影響は無い。
  webview_data="$work/webview-${run_serial}"
  mkdir -p "$webview_data"
  env JXCEL_VERIFICATION_INITIAL_SCREEN=grid "WEBVIEW2_USER_DATA_FOLDER=$webview_data" "$@" \
    "$app" "$launch_document" >"$app_log" 2>&1 &
  app_pid=$!

  deadline=$(( $(date +%s) + timeout_seconds ))
  window_line=''
  while [ "$(date +%s)" -lt "$deadline" ]; do
    if [ -z "$window_line" ]; then
      window_line=$(record_after "$baseline" "$WINDOW_MARKER" | head -n 1)
      if [ -z "$window_line" ]; then
        # 記録が回転した直後は、起動の行が世代ファイルの側へ移りうる（`record_slice` の doc）。
        # **この走行の出力**から読む（アプリは標準出力と保存先の両方へ同じ行を書く）。
        window_line=$(app_log_line "$app_log" "$WINDOW_MARKER")
      fi
      if [ -n "$window_line" ]; then
        # 起動の形跡（**判定の前に出す** — 配布物の負の対照は、この行が在ることと、落ちた理由が
        # 観測の行の不在であることの両方を要求する）。
        echo "ウィンドウ: $window_line"
      fi
    fi
    if [ "$(record_count_after "$baseline" "$wait_marker")" -ge "$expected_lines" ]; then
      break
    fi
    # 途中で落ちた場合は待ち続けない（行は現れない）。
    if ! kill -0 "$app_pid" 2>/dev/null; then
      break
    fi
    sleep 0.2
  done

  observed=$(record_count_after "$baseline" "$wait_marker")
  if [ "$observed" -lt "$expected_lines" ]; then
    echo "起動の形跡: ${window_line:-（${WINDOW_MARKER} の行がありません）}"
    report_failure "${wait_what}の行が ${timeout_seconds} 秒以内に ${expected_lines} 行そろいませんでした（観測 ${observed} 行。検証用の形で起動していないか、仕込みが読まれていません）"
  fi

  # 観測の行と製品の記録の行を切り出してからアプリを止める（**記録は追記式であり、行は次の
  # 起動まで残る**）。
  record_after "$baseline" "$OBSERVATION_MARKER" > "$observations_file"
  record_after "$baseline" "$MACRO_RECORD_MARKER" > "$product_records_file"
  parsed=$(flatten_observations "$observations_file" "$product_records_file")
  stop_app
  wait_for_no_app
}

# マクロの実行の筋書きの起動（待つのは**観測の行**である）。
run_app() {
  run_app_lines=$1
  shift
  run_app_document=$1
  shift
  launch_wait "$run_app_document" "$OBSERVATION_MARKER" "$run_app_lines" マクロの観測 "$@"
}

echo "check-macro-observation: 実行ファイル=${app}"
echo "check-macro-observation: 標本=${document}"
echo "check-macro-observation: 予算の標本=${budget_document}"
echo "check-macro-observation: 記録=${record}"
echo "check-macro-observation: 待ちの上限=${timeout_seconds} 秒 / OS=${os_mode}"
echo "check-macro-observation: 標本のマクロの並び=${EXPECTED_NAMES}"
echo "check-macro-observation: 予算の標本のマクロの並び=${EXPECTED_BUDGET_NAMES}"

run_serial=0

# ---------------------------------------------------------------------------
# 筋書き 1: 実行の成功と変更の件数（要件 2.1, 2.6, 5.1, 5.5）
# ---------------------------------------------------------------------------
echo "観測 1/7: 実行の成功と変更の件数（標本の記入）"
run_app 1 "$document" JXCEL_VERIFICATION_MACRO_RUN='標本の記入'

if [ "$(obs_value 1 requested)" != '標本の記入' ]; then
  report_failure "観測の requested が仕込んだ名前と違います（観測=$(obs_value 1 requested) / 期待=標本の記入）"
fi
if [ "$(obs_value 1 outcome)" != 'ran' ]; then
  report_failure "実行が成功していません（outcome=$(obs_value 1 outcome) / 期待=ran。要件 2.1）"
fi
if [ "$(obs_value 1 listed)" != "$EXPECTED_LISTED" ] ||
  [ "$(obs_value 1 names)" != "$EXPECTED_NAMES" ]; then
  report_failure "一覧の件数か並びが標本と違います（listed=$(obs_value 1 listed) / 期待=${EXPECTED_LISTED}、names=$(obs_value 1 names)。要件 1.3）"
fi
if [ "$(obs_value 1 capabilities)" != '' ]; then
  report_failure "標本は能力を宣言していないのに、提示された能力が空ではありません（capabilities=$(obs_value 1 capabilities)。要件 8.2）"
fi
if [ "$(obs_value 1 changes_set_cells)" != "$EXPECTED_CELLS" ]; then
  report_failure "書き込んだセルの数が ${EXPECTED_CELLS} ではありません（changes.set_cells=$(obs_value 1 changes_set_cells)。要件 5.1, 5.5）"
fi
for kind in inserted_rows removed_rows duplicated_rows; do
  if [ "$(obs_value 1 "changes_${kind}")" != '0' ]; then
    report_failure "${kind} が 0 ではありません（$(obs_value 1 "changes_${kind}")。標本の記入はセルしか書かない）"
  fi
done
case "$(obs_value 1 elapsed_ms)" in
  '' | *[!0-9]*)
    report_failure "実行の所要が数値ではありません（elapsed_ms=$(obs_value 1 elapsed_ms)。要件 11.3。**2.3 ではない** — 2.3 は面の側が判定する）"
    ;;
esac
# **製品の記録の 1 行**（要件 2.6）: 成否・出力・変更の有無が 1 件残っていること。**出力は行数**
# であり、本文は載らない（冒頭の「出力の読み方」）。
if [ "$(record_value 1 マクロ)" != '標本の記入' ]; then
  report_failure "製品の記録が実行したマクロを指していません（マクロ=$(record_value 1 マクロ)。要件 2.6）"
fi
if [ "$(record_value 1 結果)" != 'ran' ]; then
  report_failure "製品の記録の成否が実行の結果と違います（結果=$(record_value 1 結果) / 期待=ran。要件 2.6）"
fi
if [ "$(record_value 1 変更)" != "$EXPECTED_RECORD_CHANGES" ]; then
  report_failure "製品の記録の変更の有無が違います（変更=$(record_value 1 変更) / 期待=${EXPECTED_RECORD_CHANGES}。要件 2.6）"
fi
if [ "$(record_value 1 出力)" != "${EXPECTED_OUTPUT_LINES} 行" ]; then
  report_failure "製品の記録の出力の行数が違います（出力=$(record_value 1 出力) / 期待=${EXPECTED_OUTPUT_LINES} 行。要件 2.6）"
fi
echo "  成功: outcome=ran / 一覧 ${EXPECTED_LISTED} 件 / changes.set_cells=${EXPECTED_CELLS} / 所要 $(obs_value 1 elapsed_ms) ms"
echo "  製品の記録: 結果=$(record_value 1 結果) / 変更=$(record_value 1 変更) / 出力=$(record_value 1 出力)"

# ---------------------------------------------------------------------------
# 筋書き 2: 失敗の理由とフレーム（要件 9.1–9.3）
# ---------------------------------------------------------------------------
echo "観測 2/7: 失敗の理由とフレーム（標本の失敗）"
run_app 1 "$document" JXCEL_VERIFICATION_MACRO_RUN='標本の失敗'

if [ "$(obs_value 1 outcome)" != 'failed' ]; then
  report_failure "例外が失敗として終わっていません（outcome=$(obs_value 1 outcome) / 期待=failed。要件 9.1）"
fi
if [ "$(obs_value 1 layer)" != 'execution' ]; then
  report_failure "失敗の層が実行ではありません（layer=$(obs_value 1 layer) / 期待=execution。要件 9.1）"
fi
case "$(obs_value 1 reason)" in
  *"$EXPECTED_FAILURE_REASON"*)
    ;;
  *)
    report_failure "失敗の理由に例外の文言がありません（reason=$(obs_value 1 reason) / 期待に ${EXPECTED_FAILURE_REASON} を含む。要件 9.1）"
    ;;
esac
frames=$(obs_value 1 frames)
case "$frames" in
  '' | *[!0-9]*)
    report_failure "フレームの数が数値ではありません（frames=${frames}）"
    ;;
esac
# **内側（先頭）のフレームが投げた位置を指す**（要件 9.1, 9.3）。呼び出しの並びは内側から外側である。
if [ "${frames:-0}" -lt 2 ]; then
  report_failure "フレームが 2 段未満です（frames=${frames}。要件 9.3 は例外に至る呼び出しの並びを求める）"
fi
if [ "$(obs_value 1 frame0_macro)" != '標本の失敗' ]; then
  report_failure "フレームが実行したマクロを指していません（frame0.macro_name=$(obs_value 1 frame0_macro)。要件 9.3）"
fi
if [ "$(obs_value 1 frame0_line)" != "$EXPECTED_FAILURE_LINE" ] ||
  [ "$(obs_value 1 frame0_column)" != "$EXPECTED_FAILURE_COLUMN" ]; then
  report_failure "投げた位置が保存されたソースの原位置ではありません（frame0=$(obs_value 1 frame0_line):$(obs_value 1 frame0_column) / 期待=${EXPECTED_FAILURE_LINE}:${EXPECTED_FAILURE_COLUMN}。要件 9.1 はソースの行と列を求める）"
fi
if [ "$(obs_value 1 frame1_line)" != "$EXPECTED_FAILURE_CALL_LINE" ] ||
  [ "$(obs_value 1 frame1_column)" != "$EXPECTED_FAILURE_CALL_COLUMN" ]; then
  report_failure "呼び出しの段が原位置を指していません（frame1=$(obs_value 1 frame1_line):$(obs_value 1 frame1_column) / 期待=${EXPECTED_FAILURE_CALL_LINE}:${EXPECTED_FAILURE_CALL_COLUMN}。要件 9.3）"
fi
# 製品の記録も同じ実行を残している（要件 2.6）。**エンジンは失敗の腕で出力の並びを運ばない**
# ので、記録は「無かった」ではなく**書けなかった理由**を載せる（冒頭の「出力の読み方」）。
if [ "$(record_value 1 結果)" != 'failed' ]; then
  report_failure "製品の記録の成否が失敗ではありません（結果=$(record_value 1 結果) / 期待=failed。要件 2.6）"
fi
if [ "$(record_value 1 出力)" != "$EXPECTED_OUTPUT_FAILED" ]; then
  report_failure "製品の記録の出力の欄が失敗の事情を運んでいません（出力=$(record_value 1 出力) / 期待=${EXPECTED_OUTPUT_FAILED}。要件 2.6）"
fi
echo "  失敗: layer=execution / reason に ${EXPECTED_FAILURE_REASON} / frame0=${EXPECTED_FAILURE_LINE}:${EXPECTED_FAILURE_COLUMN} / frame1=${EXPECTED_FAILURE_CALL_LINE}:${EXPECTED_FAILURE_CALL_COLUMN}"
echo "  製品の記録: 結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)"

# ---------------------------------------------------------------------------
# 筋書き 3: 能力の拒否（要件 8.3, 9.2）
# ---------------------------------------------------------------------------
echo "観測 3/7: 宣言の無い能力の拒否（標本の拒否）"
run_app 1 "$document" JXCEL_VERIFICATION_MACRO_RUN='標本の拒否'

if [ "$(obs_value 1 outcome)" != 'failed' ]; then
  report_failure "拒否が失敗として終わっていません（outcome=$(obs_value 1 outcome) / 期待=failed。要件 8.3）"
fi
if [ "$(obs_value 1 layer)" != "$EXPECTED_REJECTION_LAYER" ]; then
  report_failure "拒否の層が拒んだ API の名前を持っていません（layer=$(obs_value 1 layer) / 期待=${EXPECTED_REJECTION_LAYER}。要件 9.2）"
fi
case "$(obs_value 1 reason)" in
  *"$EXPECTED_REJECTION_CAPABILITY"*)
    ;;
  *)
    report_failure "拒否の理由に能力の名前がありません（reason=$(obs_value 1 reason) / 期待に ${EXPECTED_REJECTION_CAPABILITY} を含む。要件 8.3）"
    ;;
esac
case "$(obs_value 1 capabilities)" in
  '')
    ;;
  *)
    report_failure "能力を宣言していないのに、提示された能力が空ではありません（capabilities=$(obs_value 1 capabilities)。要件 8.2）"
    ;;
esac
if [ "$(obs_value 1 frame0_line)" != "$EXPECTED_REJECTION_LINE" ] ||
  [ "$(obs_value 1 frame0_column)" != "$EXPECTED_REJECTION_COLUMN" ]; then
  report_failure "拒否のフレームが呼び出しの位置を指していません（frame0=$(obs_value 1 frame0_line):$(obs_value 1 frame0_column) / 期待=${EXPECTED_REJECTION_LINE}:${EXPECTED_REJECTION_COLUMN}。要件 9.2）"
fi
# 拒否も「失敗として終わった実行」である（要件 2.6 の成否は failed）。
if [ "$(record_value 1 結果)" != 'failed' ] ||
  [ "$(record_value 1 出力)" != "$EXPECTED_OUTPUT_FAILED" ]; then
  report_failure "製品の記録が拒否を失敗として運んでいません（結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)。要件 2.6）"
fi
echo "  拒否: layer=${EXPECTED_REJECTION_LAYER} / reason に ${EXPECTED_REJECTION_CAPABILITY} / frame0=${EXPECTED_REJECTION_LINE}:${EXPECTED_REJECTION_COLUMN}"
echo "  製品の記録: 結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)"

# ---------------------------------------------------------------------------
# 筋書き 4: 打ち切りと、その後の操作可能性（要件 6.1, 6.4）
#
# **同じ起動の中で 2 件を順に走らせる** — 1 件目が時間の上限で打ち切られ、2 件目がその後に走って
# 1 セル書く。これが「打ち切りの後もそのウィンドウが使える」ことの観測である。
# ---------------------------------------------------------------------------
echo "観測 4/7: 打ち切りと、その後の操作可能性（標本の打ち切り → 標本の記入）"
run_app 2 "$document" JXCEL_VERIFICATION_MACRO_RUN='標本の打ち切り,標本の記入'

if [ "$(obs_value 1 requested)" != '標本の打ち切り' ] ||
  [ "$(obs_value 2 requested)" != '標本の記入' ]; then
  report_failure "仕込んだ並びの順に観測の行が出ていません（1 行目=$(obs_value 1 requested) / 2 行目=$(obs_value 2 requested) / 期待=標本の打ち切り, 標本の記入）"
fi
if [ "$(obs_value 1 outcome)" != 'aborted' ]; then
  report_failure "終わらない繰り返しが打ち切られていません（outcome=$(obs_value 1 outcome) / 期待=aborted。要件 6.1）"
fi
if [ "$(obs_value 1 limit)" != 'time' ]; then
  report_failure "打ち切りの種類が時間ではありません（limit=$(obs_value 1 limit) / 期待=time。要件 6.1）"
fi
abort_ms=$(obs_value 1 elapsed_ms)
case "$abort_ms" in
  '' | *[!0-9]*)
    report_failure "打ち切りまでの所要が数値ではありません（elapsed_ms=${abort_ms}）"
    ;;
esac
if [ "$abort_ms" -lt "$ABORT_MIN_MS" ]; then
  report_failure "打ち切りが時間の上限（${ABORT_MIN_MS} ms）より前に起きています（elapsed_ms=${abort_ms}。要件 6.1）"
fi
if [ "$(obs_value 2 outcome)" != 'ran' ]; then
  report_failure "打ち切りの後に走らせたマクロが成功していません（2 行目の outcome=$(obs_value 2 outcome) / 期待=ran。要件 6.4）"
fi
if [ "$(obs_value 2 changes_set_cells)" != "$EXPECTED_CELLS" ]; then
  report_failure "打ち切りの後に走らせたマクロの変更が ${EXPECTED_CELLS} セルではありません（2 行目の changes.set_cells=$(obs_value 2 changes_set_cells)。要件 6.4）"
fi
# **実行 1 回につき記録 1 行**（要件 2.6）。2 件走らせたので記録は 2 行あり、1 行目が打ち切り、
# 2 行目が成功である（打ち切りの行が出力を運べないことも見る）。
if [ "$(record_value 1 マクロ)" != '標本の打ち切り' ] ||
  [ "$(record_value 1 結果)" != 'aborted' ] ||
  [ "$(record_value 1 出力)" != "$EXPECTED_OUTPUT_ABORTED" ]; then
  report_failure "打ち切りの製品の記録が違います（マクロ=$(record_value 1 マクロ) / 結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)。要件 2.6）"
fi
if [ "$(record_value 2 マクロ)" != '標本の記入' ] ||
  [ "$(record_value 2 結果)" != 'ran' ] ||
  [ "$(record_value 2 出力)" != "${EXPECTED_OUTPUT_LINES} 行" ]; then
  report_failure "打ち切りの後の実行の製品の記録が違います（マクロ=$(record_value 2 マクロ) / 結果=$(record_value 2 結果) / 出力=$(record_value 2 出力)。要件 2.6）"
fi
if [ -n "$(record_value 3 マクロ)" ]; then
  report_failure "実行 2 回に対して製品の記録が 3 行以上あります（3 行目=$(record_value 3 マクロ)。要件 2.6 は実行 1 回につき 1 件を求める）"
fi
echo "  打ち切り: outcome=aborted / limit=time / 所要 ${abort_ms} ms（上限 ${ABORT_MIN_MS} ms 以上）"
echo "  その後の実行: outcome=ran / changes.set_cells=$(obs_value 2 changes_set_cells)（同じ起動の同じウィンドウ）"
echo "  製品の記録 1: 結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)"
echo "  製品の記録 2: 結果=$(record_value 2 結果) / 出力=$(record_value 2 出力)"

# ---------------------------------------------------------------------------
# 筋書き 5: 保存と開き直しの往復（要件 1.2, 1.3, 1.5）
#
# 標本の**写し**を作り、まず `document-session` の引き金（`open,edit,2,save`）で**製品の保存の
# 経路**に保存させる。そのあと同じ写しを開き直して `標本の往復` を走らせる — このマクロは
# **保存された文書でだけ**（先頭行の数量が 5 のときだけ）1 セル書く。
# ---------------------------------------------------------------------------
echo "観測 5/7: 保存と開き直しの往復（セッションの引き金で保存 → 標本の往復）"
cp "$document" "$roundtrip_document"

# 5a. 保存（**マクロは走らせない**。保存そのものを製品の経路に任せる）。待つのは保存の行である。
save_baseline=$(record_lines)
launch_wait "$roundtrip_document" "$SESSION_SAVED_MARKER" 1 セッションの保存 \
  "JXCEL_VERIFICATION_SESSION=open,edit,${ROUNDTRIP_SESSION_ROWS},save"

saved_lines=$(record_count_after "$save_baseline" "$SESSION_SAVED_MARKER")
if [ "$saved_lines" -ne 1 ]; then
  report_failure "保存の行が 1 行ではありません（観測 ${saved_lines} 行。往復の前提が成立していません — 保存されていない文書を開き直しても往復にはなりません）"
fi
saved_line=$(record_after "$save_baseline" "$SESSION_SAVED_MARKER" | head -n 1)
echo "  保存: $saved_line"

# 5b. 同じ写しを開き直して往復のマクロを走らせる。
run_app 1 "$roundtrip_document" JXCEL_VERIFICATION_MACRO_RUN='標本の往復'

if [ "$(obs_value 1 names)" != "$EXPECTED_NAMES" ]; then
  report_failure "保存して開き直した文書のマクロの並びが変わりました（names=$(obs_value 1 names) / 期待=${EXPECTED_NAMES}。要件 1.2, 1.3）"
fi
if [ "$(obs_value 1 outcome)" != 'ran' ]; then
  report_failure "開き直した文書で往復のマクロが成功していません（outcome=$(obs_value 1 outcome) / 期待=ran。要件 1.5）"
fi
if [ "$(obs_value 1 changes_set_cells)" != "$EXPECTED_ROUNDTRIP_CELLS" ]; then
  report_failure "保存された文書を開き直していません（changes.set_cells=$(obs_value 1 changes_set_cells) / 期待=${EXPECTED_ROUNDTRIP_CELLS}。往復のマクロは保存された値（先頭行の数量 = 5）を見たときだけ書く。要件 1.5）"
fi
# **出力を出さないマクロの記録**（要件 2.6 の「出力の有無」）。往復のマクロは console を
# 呼ばないので `出力 = (なし)` である。
if [ "$(record_value 1 結果)" != 'ran' ] ||
  [ "$(record_value 1 出力)" != "$EXPECTED_OUTPUT_NONE" ]; then
  report_failure "出力を出さないマクロの製品の記録が違います（結果=$(record_value 1 結果) / 出力=$(record_value 1 出力) / 期待=${EXPECTED_OUTPUT_NONE}。要件 2.6）"
fi
echo "  往復: 一覧 ${EXPECTED_LISTED} 件（同じ並び）/ changes.set_cells=${EXPECTED_ROUNDTRIP_CELLS}"
echo "  製品の記録: 結果=$(record_value 1 結果) / 出力=$(record_value 1 出力)"

# ---------------------------------------------------------------------------
# 筋書き 6: 10 万行 × 30 列の全行読み + 集計（要件 11.1, 11.3）
#
# **予算を実起動の観測で判定する。** 判定するのは観測の行の `elapsedMs` であり、要件値
# （10 秒）そのものである。マクロ自身が標本の形（10 万行 × 30 列）を確かめ、違えば例外で
# 終わる — 検査器は戻り値を読めない（観測の行は `value` を運ばない）ので、**縮んだ標本を
# 速く読んだこと**を見分ける手立てはこの表明だけである。
# ---------------------------------------------------------------------------
echo "観測 6/7: 10 万行 × 30 列の全行読み + 集計（標本の予算の読み。要件 11.1, 11.3）"
run_app 1 "$budget_document" JXCEL_VERIFICATION_MACRO_RUN='標本の予算の読み'

if [ "$(obs_value 1 listed)" != "$EXPECTED_BUDGET_LISTED" ] ||
  [ "$(obs_value 1 names)" != "$EXPECTED_BUDGET_NAMES" ]; then
  report_failure "予算の標本の一覧が違います（listed=$(obs_value 1 listed) / 期待=${EXPECTED_BUDGET_LISTED}、names=$(obs_value 1 names)。要件 1.3）"
fi
if [ "$(obs_value 1 outcome)" != 'ran' ]; then
  report_failure "10 万行 × 30 列の読み + 集計が成功していません（outcome=$(obs_value 1 outcome) / 期待=ran。標本の形が要件 11.1 と違えば標本のマクロが例外で終わる）"
fi
if [ "$(obs_value 1 changes_set_cells)" != '0' ]; then
  report_failure "読みだけのマクロが変更を生んでいます（changes.set_cells=$(obs_value 1 changes_set_cells)）"
fi
read_ms=$(obs_value 1 elapsed_ms)
case "$read_ms" in
  '' | *[!0-9]*)
    report_failure "読みの所要が数値ではありません（elapsed_ms=${read_ms}。要件 11.3）"
    ;;
esac
if [ "$read_ms" -gt "$BUDGET_READ_MS" ]; then
  report_failure "10 万行 × 30 列の読み + 集計が予算（${BUDGET_READ_MS} ms）を超えました（elapsed_ms=${read_ms}。要件 11.1, 11.3）"
fi
if [ "$(record_value 1 結果)" != 'ran' ] ||
  [ "$(record_value 1 変更)" != "$EXPECTED_RECORD_NO_CHANGES" ] ||
  [ "$(record_value 1 出力)" != "$EXPECTED_OUTPUT_NONE" ]; then
  report_failure "予算の読みの製品の記録が違います（結果=$(record_value 1 結果) / 変更=$(record_value 1 変更) / 出力=$(record_value 1 出力)。要件 2.6）"
fi
echo "  読み: outcome=ran / changes.set_cells=0 / 所要 ${read_ms} ms（予算 ${BUDGET_READ_MS} ms 以内）"

# ---------------------------------------------------------------------------
# 筋書き 7: 1 万行 × 30 列の書き換え（要件 11.2, 11.3）
#
# 同じく**実起動の観測で予算（5 秒）を判定する**。あわせて**変更の件数**（1 万行 × 30 列 =
# 30 万セル）を要件値として読む — 予算の中に収まったことだけでは、**書き込みが届かなかった
# 速い実行**を見分けられない。
# ---------------------------------------------------------------------------
echo "観測 7/7: 1 万行 × 30 列の書き換え（標本の予算の書き換え。要件 11.2, 11.3）"
run_app 1 "$budget_document" JXCEL_VERIFICATION_MACRO_RUN='標本の予算の書き換え'

if [ "$(obs_value 1 outcome)" != 'ran' ]; then
  report_failure "1 万行 × 30 列の書き換えが成功していません（outcome=$(obs_value 1 outcome) / 期待=ran。標本の形が要件 11.2 と違えば標本のマクロが例外で終わる）"
fi
if [ "$(obs_value 1 changes_set_cells)" != "$EXPECTED_BUDGET_CELLS" ]; then
  report_failure "書き換えたセルの数が ${EXPECTED_BUDGET_CELLS} ではありません（changes.set_cells=$(obs_value 1 changes_set_cells) / 期待=${EXPECTED_BUDGET_CELLS}。要件 11.2 の規模そのものである）"
fi
for kind in inserted_rows removed_rows duplicated_rows; do
  if [ "$(obs_value 1 "changes_${kind}")" != '0' ]; then
    report_failure "${kind} が 0 ではありません（$(obs_value 1 "changes_${kind}")。標本の予算の書き換えはセルしか書かない）"
  fi
done
rewrite_ms=$(obs_value 1 elapsed_ms)
case "$rewrite_ms" in
  '' | *[!0-9]*)
    report_failure "書き換えの所要が数値ではありません（elapsed_ms=${rewrite_ms}。要件 11.3）"
    ;;
esac
if [ "$rewrite_ms" -gt "$BUDGET_REWRITE_MS" ]; then
  report_failure "1 万行 × 30 列の書き換えが予算（${BUDGET_REWRITE_MS} ms）を超えました（elapsed_ms=${rewrite_ms}。要件 11.2, 11.3）"
fi
if [ "$(record_value 1 結果)" != 'ran' ] ||
  [ "$(record_value 1 変更)" != "$EXPECTED_BUDGET_RECORD_CHANGES" ]; then
  report_failure "予算の書き換えの製品の記録が違います（結果=$(record_value 1 結果) / 変更=$(record_value 1 変更)。要件 2.6）"
fi
echo "  書き換え: outcome=ran / changes.set_cells=${EXPECTED_BUDGET_CELLS} / 所要 ${rewrite_ms} ms（予算 ${BUDGET_REWRITE_MS} ms 以内）"

echo "OK: 実行の成功と変更の件数（要件 2.1, 5.1, 5.5）"
echo "OK: 実行 1 回の記録が成否・出力の行数・変更の有無を運ぶ（要件 2.6。出力を出すマクロは ${EXPECTED_OUTPUT_LINES} 行、出さないマクロは ${EXPECTED_OUTPUT_NONE}、失敗と打ち切りは理由つき）"
echo "OK: 失敗の理由とフレーム（要件 9.1, 9.3。frame0=${EXPECTED_FAILURE_LINE}:${EXPECTED_FAILURE_COLUMN} は保存されたソースの原位置）"
echo "OK: 宣言の無い能力の拒否（要件 8.3, 9.2。層 ${EXPECTED_REJECTION_LAYER} と理由の能力名 ${EXPECTED_REJECTION_CAPABILITY}）"
echo "OK: 打ち切りと、その後の操作可能性（要件 6.1, 6.4。時間の上限 ${ABORT_MIN_MS} ms 以上で打ち切られ、同じ起動の続けての実行が 1 セル書いた）"
echo "OK: 保存と開き直しの往復（要件 1.2, 1.3, 1.5。保存の行が 1 行あり、開き直した一覧が同じ並びで、保存された値を見たマクロが 1 セル書いた）"
echo "OK: 10 万行 × 30 列の読み + 集計が実起動の観測で予算（${BUDGET_READ_MS} ms。要件 11.1）の中に収まった"
echo "OK: 1 万行 × 30 列の書き換え（${EXPECTED_BUDGET_CELLS} セル）が実起動の観測で予算（${BUDGET_REWRITE_MS} ms。要件 11.2）の中に収まった"
echo "注意: この検査は記録を読む検査であり、画面もウィンドウツリーも読んでいない（ウィンドウが開いたことはアプリの記録で確かめている）"
exit 0
