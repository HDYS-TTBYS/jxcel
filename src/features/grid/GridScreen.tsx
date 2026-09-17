/**
 * グリッド画面 — シートを表として見せる画面（tasks.md 8.1。要件 1.1、1.4、1.5、1.6）。
 *
 * 所有: `GridScreen`（design.md「Components and Interfaces → Frontend Layer」の GridScreen、
 * 「File Structure Plan」の `src/features/grid/GridScreen.tsx`）。**本 module は群 8 の骨格で
 * あり、8.2〜8.9 がここへ機能を足す**（下の「8.2〜8.9 への申し送り」）。
 *
 * # 画面の契約（器に差し込まれる側が守ること）
 *
 * 1. **受け取るのは `ScreenProps` だけである。** 本 module の画面は引数を 1 つも宣言しない
 *    （器が渡す 2 つをこの画面は使わない）。`SHELL_SCREEN_REGISTRY`（`src/shell/Layout.tsx`）へ
 *    **1 件**登録され、`ScreenDefinition.component` の型（`ComponentType<ScreenProps>`）が
 *    「余分な props を要求しない」ことを型で強制する。
 * 2. **自前のレイアウト・遷移を持たない。** ウィンドウ全体の面とヘッダは器が持つ。本 module が
 *    描くのは領域の内側だけであり、遷移の入口（`navigate`）を使わない — 表示対象のシートを
 *    選ぶ画面遷移は本機能の外である（design.md「Out of Boundary」）。
 * 3. **自前の配色を持たない。** 色は [`APPEARANCE_VARS`]（`src/shell/theme.ts`）の 10 本の
 *    カスタムプロパティだけを参照する（`GridScreen.test.ts` が源の走査で固定する）。
 *
 * # 失敗の隔離（**どこまでが器で、どこからが画面か**）
 *
 * 器（`src/shell/ScreenBoundary.tsx`）が隔離するのは**描画・コミットと、効果の同期的な例外**
 * だけである。React のエラー境界は**イベントハンドラと非同期の失敗（`Promise` の拒否）を
 * 捕まえない**ため、それらは画面内の状態として扱うほかない。本 module の扱いは次のとおりである。
 *
 * | 事象 | 扱い |
 * |---|---|
 * | 状態の問い合わせ（`document_state`）の失敗 | 内容の領域に失敗を出し、**再試行**を出す |
 * | シートを開く呼び出し（`grid_open_sheet`）の失敗 | 同上 |
 * | 開けるシートが無い・ドキュメントが無い | 同上（**空の状態ではない** — 利用者に打つ手がある） |
 * | 移植口の操作（8.2〜8.9 が結線する）が届いた | **告知の 1 行**を出す。内容は消さず、再試行も出さない（開き直しても直らない失敗である） |
 * | 窓の移送の失敗（生バイト経路の不達） | **画面は何もしない。** 7.3 の窓の記憶が未取得のまま残し、次の引きが再試行する（設計の誤り表「経路の失敗」。読み込み中のままである） |
 * | 描画・コミット・効果の同期的な例外 | 器の `ScreenBoundary` が隔離する（本 module は捕まえない） |
 *
 * 画面内の状態は [`GridScreenState`]（内容の領域）と、内容の領域の上に出る 2 つ — 告知 1 行
 * （[`GridScreenModel.notice`]）と、直近の確定の報告（[`GridScreenModel.editReport`]。8.3）で
 * ある。**告知で内容を置き換えない**のは、選ばれた 1 つの操作の失敗で表示中の表を失うと、
 * 利用者が見ていたものを失うためである。
 *
 * # 2 つの空の状態（要件 1.5、1.6。**判定する欄を明記する**）
 *
 * | 状態 | 判定する欄 | 提示 |
 * |---|---|---|
 * | 列が 1 本も宣言されていない（要件 1.6） | `document_state` の `DocumentSheet.columns === 0` | **表を描かず**、スキーマが未定義であることを示す |
 * | 列はあるが行が 1 件も無い（要件 1.5） | `grid_open_sheet` の応答 `GridSheetSummary.row_count === 0`（かつ `columns` が非空） | **列の構成を示したうえで**、行が無いことを示す |
 * | 行がある | `row_count > 0` | 表を描く |
 *
 * **2 つを区別するのは列の数である**（行数ではない — どちらの状態でも 0 件でありうる）。この
 * 規則は 6.1 の `GridSheetSummary` の doc が持っており、**画面は自前の規則を書かない**。
 *
 * **要件 1.6 は開く呼び出しの応答では届かない。** 列 0 本の計画を [`GridSession::open`] は
 * `SchemaUnusable` として拒む（`crates/data-grid/src/api.rs` の `open` の docs「その提示は画面が
 * セッション無しに行う」）ので、**開く前に**セッションの状態（`DocumentSheet.columns`）で判定する。
 * 防御として、開いた応答の `columns` が空である場合も同じ提示にする（`GridSheetSummary` の型は
 * それを許す）。
 *
 * [`GridSession::open`]: ../../ipc/bindings.ts
 *
 * # データの経路（何をどの順に呼ぶか）
 *
 * 1. `document_state` でセッションの状態を読み、**先頭のシート**を表示対象にする（シートを選ぶ
 *    手段は本機能の外であり、窓が運ぶのは 1 つのシートぶんだけである）
 * 2. 列が 1 本も無ければここで止める（上の表。開く呼び出しはしない）
 * 3. `grid_open_sheet` でシートを開き、`GridSheetSummary`（列の構成と行数）を受ける
 * 4. 行があれば `grid_set_view` に**空の指定**（絞り込み無し・並べ替え無し・展開無し）を渡し、
 *    **可視行の順序を導出させる**。**これを呼ばないと窓はつねに行 0 件で返り、表は読み込み中の
 *    ままになる**（`GridSession::set_view` が順序の導出そのものである。8.1 の起動観測で実測した）。
 *    操作（並べ替え・絞り込み）は 8.8 がここへ足す
 * 5. 表を描く場合、**窓の記憶（7.3）を組み立て、移植口（7.1 / 7.2）へ仕様を渡す**。
 *    `RendererSpec.getCell` は窓の記憶のものをそのまま渡し、窓が届いたら `invalidate` で
 *    その区間を描き直させる（移植口は知らせが無ければ描き直さない）
 *
 * 手順 4 と 5 のあいだに**世代の整合**がある。**境界の応答が世代を運ぶ**（タスク 10.1。
 * `GridOpenResponse` / `GridViewResponse` / `GridEditResponse` の `generation`。10 進の文字列）
 * ので、画面はそれを**採用するだけ**である — 数え直さない（かつての「成功ごとに +1」は、
 * 1 つのコマンドの内側で世代が複数回進む経路でつねにずれる）。
 * 記憶へ渡す行数は**可視行数**（手順 4 の応答）である — 窓の区間は可視行の序数で表される。
 *
 * 境界の口は [`GridClient`]（`./gridClient`）1 つを通す。**本 module は `invoke` もコマンド名も
 * 知らない。**
 *
 * # 8.2 が足したもの（現在位置・選択・追従）
 *
 * **選択と現在位置は画面が持つ。**表を描く状態（`ready` の腕）が `selection` を持ち、その 1 つの
 * 値だけが、数え上げの表示（要件 2.5）と移植口へ下ろす選択（`handle.setSelection`）の両方へ渡る
 * — **写しを 2 つ持たない**ので、画面に出ている数と描かれている選択はずれようがない。
 *
 * | 要件 | どこが担うか |
 * |---|---|
 * | 2.1 現在位置を 1 つ持ち、区別して示す | `ready.selection`（型が「表を描くときは選択がある」を表す）。区別して示すのは移植口の実装（Glide の焦点の環） |
 * | 2.2 方向の指示で隣接するセルへ移る | `./selection` の移動の規則（打鍵は `GridSurface` の器が受ける） |
 * | 2.3 矩形・行の全体・列の全体 | 同じ module の 3 つの規則。ポインタの操作（行見出し・見出し）は移植口の通知として届く |
 * | 2.4 表示範囲の追随 | `followTarget` が宛先を決め、`handle.scrollTo` が動かす。可視の区間は `RendererSpec.onVisibleSpanChange` が知らせる（**8.2 が移植口へ足した口である**） |
 * | 2.5 行数・列数・セル数 | `selectionCounts` を表の上の 1 行に出す |
 * | 2.6 確定した選択を複製・貼り付け・削除・取り消しの対象にする | **`ready.selection` がその口である**（8.6 / 8.7 / 8.9 が読む）。**行の削除・複製は 8.6 が実装した**（`./rowOps` が選択の行を対象にする）。**範囲の複製・貼り付けは 8.7、取り消し・やり直しは 8.9** である |
 *
 * # 8.3 が足したもの（セルの編集と、確定の報告）
 *
 * **編集の起動・確定・取消の 3 つは `./cellEdit` の 1 つの関数（`settleCellEdit`）へ集まる。**
 * 入力手段（7.4）が出す口は `commit(text)` と `cancel()` の 2 つだけであり、そのどちらもが同じ
 * 経路を通る。**取消が境界へ何も送らないこと**はこの形から出る（取消の腕は `GridClient` を
 * 1 つも触らない。`cellEdit.test.ts` が「偽の境界のどの口も例外を投げる」形で固定する）。
 *
 * | 論点 | 決定 | どこが担うか |
 * |---|---|---|
 * | 入力手段の選択（要件 3.1、10.3） | **登録簿（`editorRegistry.resolve`）だけが解決する。** 画面に型ごとの分岐は 1 つも無い | `CellEditorPanel`（本 module の内側の成分） |
 * | 編集するセル | **現在位置の 1 セルだけである**（範囲の編集は 8.7 の貼り付けの担当） | `ready.editing` が位置を持ち、`SetCells` の 1 件になる |
 * | 編集の面をどこに描くか | **表の面（`GridSurface`）の中、数え上げの行と表の器の間**である。移植口に「セルの上へ DOM を重ねる」口は無く（`RendererSpec` にそんな欄は無い）、開いているセルを**位置の提示つきで**出す方が、覆われたセルを探すより読める | `GridSurface` の編集の面 |
 * | 確定の結果（要件 3.4、3.5） | **表の上に 1 つの報告**として出す（型強制の一覧と、残った違反）。**窓の印とは別である** — 移植口のセルが運ぶのは違反の有無だけで（`RenderCell.violated`）、型強制は**起きた出来事**であってセルの状態ではない | `model.editReport` |
 * | 取消の値の復帰（要件 3.6） | 何もしない。文書は適用の前にあり、**値を戻すのは「編集を閉じる」ことそのもの**である（窓の記憶も捨てない） | `gridScreenEditSettled` の `cancelled` の腕 |
 * | 適用のあとの作り直し（要件 1.7） | 影響を受けた行の窓を捨てる（`./cellEdit`）。**描き直させるのは 7.3 の記憶と移植口の既存の結線**（8.1 の `onArrival` → `RendererHandle.invalidate`） | `./cellEdit` と `GridSurface` |
 *
 * ## 編集の面は表の上に出る（**現在位置の追跡**）
 *
 * 編集中のセルは `ready.editing`（`CellEdit`）が持つ — 位置と、**開いた時点の表示文字列**である。
 * 位置だけを持たないのは、入力手段の初期値が要るためであり（`CellEditorProps.initialText`）、
 * **開いた時点で写す**のは、窓の到着で描き直されても入力中の値が動かないようにするためである
 * （要件 3.5 の「値を捨てない」は、打っている最中に足元が変わることでも壊れる）。
 *
 * 表を描いていない状態（`loading` / `failed` / 空の 2 つ）は `editing` を持たない — **描かれて
 * いないセルは編集できない**（型がそれを表す）。
 *
 * ## 確定の報告（要件 3.4、3.5。**提示の本体は 8.4**）
 *
 * 報告は 2 つを持つ:
 *
 *   - **型強制**（要件 3.4）— 変換が起きたことと、**変換前の値**（`GridCoercionNotice.before`）。
 *     変換の前後はどちらも境界が表示文字列で運ぶ（`GridCoercionNotice` の doc）ので、画面は
 *     解釈せずに両方を並べる。**前の値を落とさない**ことは検査が固定する
 *   - **残った違反**（要件 3.5）— 適用のあとに再検証した列の違反の位置（`GridEditOutcome.violations`）
 *     と、**シート全体**の違反の総数（`violation_total`。適応層が `GridSession::violation_total()`
 *     から写す — **再検証した列に閉じない**。閉じているのは位置の一覧のほうである）。
 *     **値は文書に残っている** — 判定する側（`schema-engine`）は `WriteOrigin::Edit` を決して
 *     拒否せず、`grid_apply_edit` は適合しない値も破棄せずに返す（`src-tauri/src/commands/grid.rs`
 *     の同コマンドの docs）。したがって 3.5 の「保持」は**画面が値を捨てないこと**で満たす
 *
 * **8.4 が引き取ったもの**（下の「8.4 が確定させたもの」）: 違反の印の色、違反のバーと巡回
 * （`grid_find_violation` と、違反の位置への現在位置の移動）。**本 module が持つのも、直近の
 * 確定で生じた違反を 1 行使に出して閉じられるようにするところまで**である（位置の一覧は
 * `GridEditOutcome.violations` が運ぶが、**理由の文言はこの経路に無い** — `GridViolation` の
 * `reason` は `grid_find_violation` が組み立てる）。
 *
 * # 8.4 が確定させたもの（違反の提示と巡回。`./violations` / `./violationBar`）
 *
 * 違反の提示は 4 つに割れる。**それぞれ源が別**であり、1 つの経路へ畳まない。
 *
 * | 要件 | 何を出すか | 源 | どこが担うか |
 * |---|---|---|---|
 * | 4.1 違反しているセル | 印（地色の上書き） | 窓が運ぶセルごとの違反の札（5.1） | 移植口の実装（本 module は `RenderCell.violated` を渡すだけである） |
 * | 4.2 指定したセルの理由 | 理由の文言 | `grid_find_violation` の `reason`（組み立てるのは適応層）。要求は**指したセルの文書の列**を名指しする（タスク 10.6） | `./violations` の `reasonInRow` と、表（窓の印と行の識別子を読む） |
 * | 4.3 シート全体の総数 | 数 | `grid_set_view` / `grid_apply_edit` の応答 | `ready.violationTotal` と `./violationBar` |
 * | 4.4 次の違反への移動 | 現在位置の移動と、その違反の理由 | `grid_find_violation`（前向き） | `./violations` の `nextViolation` と `gridScreenNextViolation` |
 *
 * **文言は 1 つしか無い。**理由の文は適応層が `ViolationReason` / `Expected` から組み立てた
 * ものであり（生成物の `GridViolation` の doc）、本 module も `./violations` も**写すだけ**で
 * ある。したがって同じ違反が経路によって別の言い方になることはない。
 *
 * ## 決定（**7.1 / 8.1 が開いたままにした点をどう閉じたか**）
 *
 * | 論点 | 決定 | 理由 |
 * |---|---|---|
 * | **違反の印の色**（8.1 の開いた点 1） | **移植口を広げない。**色は実装（`glideAdapter.tsx` の `VIOLATION_THEME`）の既定のままにする | ① 移植口が保証するのは**印を落とさないこと**であり（4.1 が求めるのは「区別できる形で示す」である）、色そのものは Glide の `Theme` という**ライブラリ固有の型**に属する — `RendererSpec` へ載せると、移植口を差し替えるたびにその写しを書き直すことになる。② 画面は**自前の配色を持たない**（本 module の契約 3）ので、色を渡す口を作っても渡せる色が無い — `APPEARANCE_VARS` の 10 本は器のクロームの色であり、違反の色は無い（足すのは器の設計の変更である）。③ 実装を差し替えても「違反が区別できること」は要件として残るが、その**色の値**は要件ではない |
 * | 巡回の向き | **前向きだけである。**起点は現在の行の次（`+1`） | 要件 4.4 は「次の違反への移動」である。逆向きは要件に無く、序数の解決の単調性も成り立たない（`./violations` の module doc） |
 * | いまの行の違反を起点にしない | `from` は `現在の行 + 1` である | いまの行の違反を返すと、違反の上で押しても動かない（「次の違反へ」が壊れて見える） |
 * | **違反の位置（可視行の序数）の求め方** | 行の識別子から**二分探索で解く**（`./violations` の「序数の解決」） | **境界が序数を運ばない**（応答は行の識別子と列だけである）。序数を持つのは Rust 側の索引であり、落ちているのは応答の欄だけである — 境界が運べば 1 回の問い合わせで済む（`design.md`「8.4 が確定させたもの」の申し送り） |
 * | 巡回の費用 | 可視行数の対数（10 万行で 17 回）。**検査が問い合わせの回数（20 回以内）を固定する** | 索引は可視行の序数を鍵に持つので、問い合わせは「起点以降で最初の違反」を返す。序数はその単調な述語の二分探索で確定する。**行数を走査する経路を作らない**（要件 11 の目的はどの操作でも待たされないことである） |
 * | これ以上違反が無いとき | **正常な結果**（`exhausted`）としてバーに出す。告知（`notice`）には載せない | 生成物の `GridViolationResponse` が「`violation: None` はその向きに違反が無い場合であり**正常な結果である**」と定めている。失敗の告知に載せると、利用者の操作が失敗したように読める |
 * | 総数が 0 でも巡回の操作を残す | 残す（無効にしない） | 総数は**行を持たない違反**（列そのものの問題）も数えるが、探索はそれを移動先にしない — 総数と探索の対象は同じ集合ではない（`./violationBar` の module doc） |
 * | 確定の報告（8.3）とバーの関係 | **別のものである**（報告は確定のたびに出て閉じられる記録、バーは表を描いている間つねに出る提示）。数を出すのはどちらも同じ源（シート全体の総数）をそのまま置くので、食い違いようがない | `./violationBar` の module doc の表 |
 * | 適用のあとの違反（4.6） | 総数は `GridEditOutcome.violation_total` で置き換え、**いまの提示は取り下げる。**そのあと表が理由を引き直す | 解消されたかどうかはこの遷移では分からない（分かるのは窓の印の取り直しと、境界への問い合わせである）。取り下げておけば、**解消された違反の提示が残らない** |
 * | 印そのものの更新（4.6） | **7.3 の窓の記憶の経路である**（`EditOutcome.affected` → `invalidate` → 取り直し → `onArrival` → `Handle.invalidate` → 描き直し） | 8.3 が組んだ経路をそのまま使う。窓の違反の札は索引から作られる（`crates/data-grid/src/transport/mod.rs`）ので、適用が索引を差分更新すれば、取り直した窓の印は新しい |
 *
 * ## 表が担うこと（**窓の印と行の識別子を読める唯一の場所である**）
 *
 * 理由（4.2）と、適用のあとの引き直し（4.6）は**本 module の `GridSurface`** が行う。窓の印
 * （`RenderCell.violated`）と行の識別子（`WindowCache.rowId`）を読むには窓の記憶が要り、
 * 記憶を持つのは表だからである。表は**読み取りだけ**を行い、状態へ入れるのは遷移
 * （`gridScreenViolationReason`）である。
 *
 * 引き直しの契機は 3 つある: ① **現在位置が変わった**（移動のたび。**印の無いセルでは境界へ
 * 問い合わせない** — 門番は窓の印である）、② **窓が届いた**（未取得だったセルと、適用のあとの
 * 取り直し）、③ **適用が成功した**（その場で 1 度引く — 索引は既に新しいので、古い印に
 * 引きずられない）。
 *
 * 遅れて届いた答えは捨てる（世代の印）。現在位置がもう違うのに、前の位置の理由を出す経路を
 * 作らない。
 *
 * ## 4.5（入れ子の内側の位置）は 8.5 が実装した
 *
 * 窓はセルごとに**内側の位置の札**を運ぶ（`transport` の `marks`）が、移植口の `RenderCell` は
 * それを運ばない（`violated` の 1 ビットだけである）。8.4 はこの提示を 8.5 へ送り、
 * **移植口を広げなかった**。8.5 は同じ判断を保ったまま実装した: 内側の位置は
 * [`WindowCache.nestedMarks`]（8.5 が記憶へ足した口）から読み、`./nestedInspector` の
 * `NestedInspector` が `a.b` / `tags[2]` / 「セル直下」を書き分けて示す（要件 4.5）。
 *
 * ## 単体テストが観測しないもの（実物の起動で観測する）
 *
 * ① **印の色**（`VIOLATION_THEME` の地色が実際に塗られること。単体テストは「印を落とさない」
 * ことまでであり、`glideAdapter.test.ts` が `themeOverride` を載せることを見ている）、
 * ② **追随が実際にスクロールを起こすこと**（`followSelection` は「`scrollTo` へ何を渡すか」まで
 * を固定する）、③ **`grid_find_violation` が表示範囲の外の違反へ実際に到達すること**（Rust 側の
 * 索引の性質であり、`crates/data-grid/src/view/violations.rs` の `find` とその検査が担う）。
 * 観測の場所は 8.1 / 8.2 と同じ段（`smoke-port-probe` と
 * `scripts/check-port-interaction.sh`。**標本の面は既に違反の帯を描いている** — `probeCell` が
 * `row % 7 === 3` を違反として返す。帯の中の画素を 1 つ読めば色の主張を足せる）と、9.2 の
 * 台本である。
 *
 * # 8.5 が確定させたもの（入れ子の展開と詳細表示。`./nestedInspector` / `./columnSpace`）
 *
 * **入れ子に閉じた決定は `./nestedInspector` が持ち、列の写像は `./columnSpace` が持つ。**
 * 本 module が足したのは、① 状態の欄 3 つ（`view` / `generation` / `detail`）、② その遷移
 * （`gridScreenViewSettled` / `gridScreenDetailOpened` / `gridScreenDetailClosed` /
 * `gridScreenDetailEditSettled`）、③ 境界へ送る純粋な非同期関数（`applyGridView`）、
 * ④ 表の面が窓の記憶を組む口（`createGridSurfaceCache`）である。
 *
 * | 論点 | 決定 | どこが担うか |
 * |---|---|---|
 * | 展開の操作（要件 5.1、5.2） | **押された 1 件を、いまの指定へ足した完全な記述として送る**（`withExpansion`）。送っている途中の押下は `pendingViewRef` へ積む | `GridScreen` の `expand` と `./nestedInspector` |
 * | 展開の状態の保持（要件 5.3） | `ready.view` が持つ。**走査はここへ触れない**ので失われない | `GridScreenState` の `view` |
 * | **展開の結果の列の構成**（要件 5.1、5.2。申し送り 2 の修復） | `grid_set_view` の応答が運ぶ**導出後**の構成を `ready.summary.columns` へ採用し、表の面を同じ 1 つの経路（`summary` を依存に持つ効果）で組み直す | `gridScreenViewSettled` と `GridSurface` |
 * | 段数の上限の誘導（要件 5.4） | 記述の印（`expandability` が `capped`）から「詳細表示へ」を出し、押すと**現在位置の行のその列**の詳細表示を開く | `./nestedInspector` の `nestedColumnControls` と `gridScreenDetailOpened` |
 * | 詳細表示（要件 4.5、5.5、5.6） | 値を読むのは**表**（窓の記憶を持つ側）であり、開いている位置は状態が持つ。**同じ遷移でセルの編集と同一の規律**（`gridScreenDetailEditSettled` が `gridScreenEditSettled` を呼ぶ） | `GridSurface` の中の `NestedInspector` |
 * | 世代（10.1 が境界へ移した） | **境界の応答が運ぶ 10 進の文字列をそのまま採用する**（数え直さない）。**組み直さずに**記憶へ下ろす（構成が変わったときだけ、構成を依存に持つ効果が組み直す） | `ready.generation` と `GridSurface` の効果 |
 *
 * **要件 5.1 / 5.2 の見える結果は、申し送り 2 の修復で届くようになった。** 境界が返す導出後の
 * 構成を状態が採用し、表の面がそれを描く（展開すると内側の位置が列として並び、折りたたむと
 * 元の 1 本に戻る）。**届かないのは値の構造そのものである**（申し送り 1）— 内側の宣言は境界が
 * 材料として運ぶ（`ColumnDescriptor.members`。タスク 10.3 が閉じた 7.4 の申し送り 6）ので、
 * 詳細表示は**折りたたみのままでも**内側のフィールドを名と型で並べられ、読めないのは値の構造
 * だけである。
 *
 * # 8.6 が足したもの（行の追加・削除・複製。`./rowOps`）
 *
 * **判断と往復は `./rowOps` が持ち、状態を持つのは本 module である**（8.3 のセルの編集と同じ
 * 分担である）。本 module が足したのは、① 状態の欄 1 つ（`ready.pendingDelete`）、② その遷移
 * （`gridScreenDeleteRequested` / `gridScreenDeleteCancelled` / `gridScreenRowOperationSettled`）、
 * ③ 表の面から入口へ渡す材料（表示の指定・行数・**1 画面に見えている行数**）と、
 * ④ 3 つの操作と確認の面（`RowOperations`）である。
 *
 * | 論点 | 決定 | どこが担うか |
 * |---|---|---|
 * | 3 つの操作の置き場所 | **表の面の上に行として出す**（打鍵・メニューの結線は要件 7.8 / 9.9 の担当であり、**8.7 が足す**）。3 つの操作が要するのは窓の記憶と選択であり、その両方を持つのは表だからである | `GridSurface` の中の `RowOperations` |
 * | 足す位置の意味（要件 6.1） | **現在位置の行の位置へ 1 行**（その行の**上**に入る。表計算の「上に行を挿入」と同じである）。数はつねに 1 である（まとめて足すのは貼り付けの補充であり、`PasteRange` が担う） | `./rowOps` の `InsertRows` の組み立て |
 * | **挿入の位置の座標空間**（要件 8.6） | 送るのは**可視の序数**である（`{ anchor: "Before", ordinal }`。文書の位置へ写すのはドメイン）。並べ替え・絞り込みが効いていてもそのまま送れる（tasks.md 10.4） | `./rowOps` の `planRowOperation` |
 * | 追加した行の既定値（要件 6.1） | **画面は値を 1 つも作らない**（`InsertRows` は位置と数しか運ばない）。既定値を書くのは宣言（`CompiledSchema::default_row`）であり、画面は**取り直した窓がそれを運ぶ**ように行数を作り直す | `./rowOps` の `applyRowOperation` |
 * | 削除の確認（要件 6.5） | 閾値は**いま 1 画面に見えている行数**（移植口の `onVisibleSpanChange` が報せた区間。**先読みの幅ではない**）。超えるときは**送らずに**数を示して尋ねる。**選択が動けば取り下げる**（尋ねた数と消える数が食い違わない） | `./rowOps` の `deleteNeedsConfirmation`、`ready.pendingDelete`、`RowOperations` |
 * | 確認への取り消し | **境界へ 1 つも送らない**（`./rowOps` の計画の腕であり、往復へ載らない）。報告・告知も動かさない | `gridScreenDeleteCancelled` |
 * | 行数が変わったあと（要件 1.7） | **新しい行数で記憶を作り直す**（`WindowCache.clear(row_count)`）。窓の区間は序数であるため、`invalidate` では足りない | `./rowOps` の `applyRowOperation` |
 * | 行の位置の提示（要件 6.5） | ① **窓が覆う行数**（`visibleRows` → 移植口の `rowCount`）② **提示する行数**（`summary.row_count`）③ 現在位置と選択の**寄せ**（`clampSelection`）、の 3 つを同じ遷移で置き換える。**応答が影響を受けた行の表示の序数を運んでいれば、その先頭へ現在位置を移す**（9.8 の規則はこの 1 つの遷移にあり、行の操作・貼り付け・履歴が同じ材料を使う。10.5） | `gridScreenRowOperationSettled` |
 *
 * **`clear` を呼ぶ理由（表の面は記憶を組み直すのに、なぜ要るか）。**表の面は可視行数を依存に
 * 持つ効果で器と記憶を組み直す（**移植口へ行数を渡す唯一の口が組み立てである** —
 * `RendererHandle` に行数を押し込む口は無い）ので、いまの結線では記憶はその組み直しでも新しく
 * なる。それでも `clear` を呼ぶのは、**記憶の契約が「行数が変わったら新しい数を渡して作り直せ」
 * だからである**（7.3 の申し送り。引数の無い `clear` は組み立て時の数へ戻り、**増えた行は
 * 永久に読み込み中のまま**になり、減った先は古い窓のまま配られる）。組み直しに頼ると、同じ
 * 記憶を使い続ける呼び出し（器を組み直さない経路）で増えた行が読めなくなる。
 *
 * **単体テストが観測しないもの（8.6）。**① **実際に文書へ行が足され・消え、既定値が入ること**
 * は Rust 側の契約である（`crates/data-grid` の検査）— 本 module は「何を送ったか」までしか
 * 主張しない。② **確認の面が現れ、押下が届くこと**（`onClick` の配線）は `node` の環境
 * （DOM なし。`jsdom` も `@testing-library` も足していない — `vitest.config.ts` の判断）では
 * 観測できない。③ **移植口が新しい行数を描くこと**（行の番号が増減すること）も同じであり、
 * 単体テストは「状態と提示の数が変わること」までである。観測の場所は実起動（9.2 の台本と
 * `smoke-port-probe`。**標本の面は行の番号を描いている**）である。
 *
 * # 8.7 が足したもの（範囲の複製・貼り付けと、打鍵・メニューの 2 つの入口。`./clipboard`）
 *
 * **判断と往復は `./clipboard` が持ち、状態を持つのは本 module である**（8.3 のセルの編集・
 * 8.6 の行の操作と同じ分担である）。本 module が足したのは、① 遷移 1 つ
 * （`gridScreenPasteSettled`）、② 移植口の 3 つの口（`onCopy` / `onPaste` / `copySelection`）の
 * 結線、③ 表の面から判断へ渡す材料（窓の記憶の 3 つの口）である — ③ は `./clipboard` の
 * [`createClipboardSurface`] が組み立て、**本 module は材料と行き先を渡すだけ**である。
 *
 * **メニューの活性化は本 module が購読する**（`./clipboardRequests`。要件 7.8 の後者）。器
 * （`src-tauri`）が `編集 > 複製` の選択を対象ウィンドウへイベントで送り、本 module が
 * **打鍵と同じ入口**（`RendererHandle.copySelection`）へ渡す。**貼り付けの項目は器が登録して
 * いない**（クリップボードの読み口が無いため。理由は `design.md` と `./clipboardRequests` の doc）。
 *
 * | 論点 | 決定 | どこが担うか |
 * |---|---|---|
 * | 複製のテキストを作る場所 | **画面が作る**（移植口の `onCopy` は文字列を返す口であり、7.2 の実装は素通しである）。値は**窓の記憶の `getCell`** から読み、規則は `crates/data-grid/src/edit/paste.rs` の写しで書く — **境界に「範囲を読む」コマンドが無い**ため、ドメインの書く側（`PasteCodec::write`）は画面から到達できない | `./clipboard` の `planCopy` / `tableText` |
 * | テキストの形式（要件 7.2） | 行の区切りは LF、列の区切りは TAB。区切りと `"` を含む値は囲み、囲みの中の `"` を `""` へ倍にする。**Rust の `PasteCodec::write` と 1 バイトも違わないことを実測で突き合わせた**（`clipboard.test.ts` の golden） | `./clipboard` の `tableText` |
 * | 複製できないとき | **空文字を返さない。**窓が届いていないセルが 1 つでもあれば、拒否して理由を告知へ出す（空文字を返せば、利用者には「複製できた」と見え、クリップボードは空になる） | `planCopy` の `refused` と `createGridRendererSpec` の `refusePromise` |
 * | **貼り付けの宛先の座標空間**（要件 8.6、8.9） | 錨は**物理の行（`RowId`）と文書の列**であり、歩く順序（`rows`）は**表示されている行の並び**である。**8.6 の「挿入の位置を写せない」制約は掛からない** — 渡すのは行の識別子であり、可視の序数から引ける（識別子で対象を決めた削除・複製と同じ理由） | `./clipboard` の `planPaste` |
 * | 行の補充の境目（要件 7.4） | 渡す並びは「矩形の行数」と「錨から先に残る可視行数」の小さい方である。**短く渡すとドメインが行を足し、既存の行へ書かない**（利用者から見れば貼り付けたはずの行が増える）ので、矩形の行数を `./clipboard` が数える（囲みの中の改行を数えない） | `./clipboard` の `tableTextRowCount` |
 * | 貼り付けのテキスト | **1 バイトも変えない**（改行の正規化も、列の解釈もしない）。解釈はドメインの `PasteCodec::parse` が唯一の源である | `./clipboard` の `planPaste` |
 * | 行数が変わったあと（要件 1.7） | 適用が影響を受けた行を持てば `WindowCache.clear(row_count)` を呼び、**反映の形は 8.6 と同じ 1 つを通る**（行数を置き換え、現在位置と選択を寄せ、消えた行の面を閉じる）。貼り付けは**行を補充しうる**ので、`invalidate` では足りない | `./clipboard` の `applyPaste` と `gridScreenPasteSettled`（`appliedRowOperation`） |
 * | **打鍵からの実行**（要件 7.8 の前者） | **移植口の 2 つの口がその経路である。**7.2 の面（`GlideSurface`）が DOM の `copy` / `paste` を捕獲の段で受け、移植口へ渡す — 打鍵（Ctrl+C / Ctrl+V）はそのイベントを起こす | `createGridRendererSpec` の `onCopy` / `onPaste` |
 * | **メニューからの実行**（要件 7.8 の後者） | **複製は結線した**（器が `編集 > 複製` を登録し、活性化をイベントで送り、本 module が購読して**打鍵と同じ入口**を呼ぶ）。**貼り付けは結線していない** — 障碍は**クリップボードの読み口が無いこと**であり（読み口は DOM の `paste` だけ、プラグインは依存に無い、`navigator.clipboard.readText()` は 7.2 の実起動で `不可`）、**読み口が無いまま `Ctrl+V` を項目に登録すると、いま動いている打鍵の貼り付けを基盤のメニューが奪って壊す** | 複製は `./clipboardRequests` と `src-tauri/src/commands/grid.rs`。貼り付けは `design.md`「貼り付けの項目を今 登録しない理由」 |
 *
 * **単体テストが観測しないもの（8.7）。**① **文書へ実際に値が書かれること**と、補充される行の
 * 既定値は Rust 側の契約である（`crates/data-grid` の検査）— 本 module は「何を送ったか」まで
 * しか主張しない。② **クリップボードとの往復**（要件 7.2 の後半。他の表計算アプリケーションとの
 * 間で範囲を往復できること）は、**実機のクリップボードと他のアプリケーション**を要するので
 * `node` の環境（DOM なし）では観測できない。7.2 のレビューは、システムのクリップボードの
 * 読み戻しがこの観測環境では確かめられないことを記録している。観測の場所は実起動（9.2 の台本と
 * `smoke-port-probe`）と人手の手順である。③ **打鍵が DOM の `copy` / `paste` を実際に起こすこと**
 * も同じ環境では観測できない（7.2 の `glideAdapter.test.ts` が固定するのは捕獲の段の配線であり、
 * 実機の打鍵ではない）。
 *
 * # 8.3〜8.9 への申し送り（本 module が足す予定の場所）
 *
 * - **8.8〜8.9**: 移植口の残る 2 つの**操作**（`onColumnResize` / `onColumnMove`）。
 *   **8.8 が結線した**（`./viewOps` の判断を通し、画面の遷移へ渡す。`createGridRendererSpec`
 *   の doc「列幅と列の移動は 8.8 が結線した」）— 8.1 が「黙って何もしない実装にしない」ために
 *   置いた `onUnavailable` の経路は、**結線と同時に落とした**（未結線の操作が 1 つも無くなった
 *   ためである）
 * - **8.7（範囲の複製・貼り付けと、メニュー・打鍵）**: 移植口の `onCopy` / `onPaste` を結線し、
 *   **複製はメニューからも実行できるようにした**（器の `MenuRegistry` への `data-grid.copy` の
 *   登録・プラットフォームで解決した綴り・活性化のイベント・本 module の購読。上の「8.7 が
 *   足したもの」）。**残っているのはメニューからの貼り付けだけである** — クリップボードの
 *   読み口が無い（`./clipboard` の module doc と design.md に理由を記録した）。
 *   **9.9（取り消しとやり直しのメニュー）は 8.9 が同じ形（器の登録 ＋ イベント ＋ 購読）で足す**
 * - **8.4（違反の提示）**: 実装済みである（上の「8.4 が確定させたもの」）。本 module が受け取る
 *   `violation_total` は 6.2 の時点で既に**シート全体**の数であり、広げる作業は無かった
 * - **8.6（行の増減）**: 実装済みである（下の「8.6 が足したもの」）。7.3 の申し送り（行数が
 *   変わったら `WindowCache.clear(rowCount)`）は `./rowOps` が担う
 * - **8.8（列幅・列順）**: 列幅と表示上の列順は `createDisplayState`（7.5）が持つ。変化は
 *   **次の `mount` の仕様**に載せる（`RendererHandle` に幅や順を押し込む口は無い。7.2 の申し送り）。
 * - **列の添字の恒等が崩れるのは 2 つである（8.5 が 1 つ目を閉じた）**: 崩すのは ① 8.8 の列順
 *   ② 入れ子の展開であり、**②は 8.5 が閉じた**（写像は `./columnSpace` の 1 つであり、窓の読み
 *   `WindowCache.getCell` と編集の宛先 `WindowCache.documentColumn` が同じ値を引く）。①（8.8）
 *   は**同じ 1 つへ揃える**こと — 列の並びの変更は窓の中身を変えないので、揃えるのは `getCell`
 *   へ渡す位置の側である（8.8 の担当）
 * - **8.4 の提示も同じ写像を通る（境界修復の後に足した是正。10.6 が順方向も足した）**: 境界の
 *   `GridViolationLocation` が運ぶ列は**文書の列**であるため、`./violations` の
 *   `reasonInRow` / `nextViolation` は `ColumnSpace.displayPosition`（**逆向き**。文書の列 +
 *   内側の位置 → 表示の位置）で落としてから名乗る・着く。**10.6 は順方向も使う** — 4.2 の要求は
 *   指したセルの**文書の列**を運ぶ（`ColumnSpace.documentColumn`。写せなければ指定を送らない）。
 *   本 module は写像を渡すだけであり（`GridSurface` は `summary` から 1 つ引き、巡回は押下ごとに
 *   `summary.columns` から引く）、**バー（`./violationBar`）と `gridScreenNextViolation` は
 *   表示の位置を受け取る側なので変わっていない**
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
} from "react";

import { APPEARANCE_VARS } from "../../shell/theme";
import { assertNever, describeIpcError } from "../../ipc/client";
import type {
  ColumnDescriptor,
  GridCoercionNotice,
  GridEditOutcome,
  GridExpansionState,
  GridHistoryDirection,
  GridSheetSummary,
  GridViolationLocation,
  GridViewSpec,
} from "../../ipc/bindings";
import {
  settleCellEdit,
  type CellEditIntent,
  type CellEditSettlement,
} from "./cellEdit";
import { ViolationBar } from "./violationBar";
import {
  nextViolation,
  reasonInRow,
  violationMark,
  type ViolationPresentation,
  type ViolationReading,
} from "./violations";
import { createDisplayState, type DisplayStateStore } from "./displayState";
import { columnEditor } from "./editors";
import type { ColumnConstraints, EditCarrier } from "./editorRegistry";
import { createColumnSpace } from "./columnSpace";
import { constraintsOf, withReferenceRows } from "./columnConstraints";
import {
  NestedColumnControls,
  NestedInspector,
  declaredInnerPositions,
} from "./nestedInspector";
import {
  applyViewOperation,
  drawnColumns,
  hasRowRestriction,
  layoutKeyOf,
  rowOrderKeyOf,
  type ViewOperation,
} from "./viewOps";
import {
  REFERENCE_PAGE_SIZE,
  loadReferenceRows,
  type ReferenceRows,
} from "./referenceRows";
import { ViewBar } from "./viewBar";
import { WINDOW_ROWS, createWindowCache, type WindowCache } from "./windowCache";
import {
  applyRowOperation,
  planRowOperation,
  rowTargets,
  runRowOperationPlan,
  type DeleteConfirmation,
  type RowOperationSettlement,
  type RowOperationTarget,
  type RowSendIntent,
} from "./rowOps";
import { createGlideAdapter } from "./renderer/glideAdapter";
import {
  applyHistory,
  installGridHistoryRequests,
  type HistorySettlement,
} from "./history";
import {
  createClipboardSurface,
  runPastePlan,
  type CopyPlan,
  type PastePayload,
  type PastePlan,
  type PasteSettlement,
} from "./clipboard";
import { installGridCopyRequests, type CopyEntry } from "./clipboardRequests";
import {
  clampSelection,
  followTarget,
  initialSelection,
  selectionAt,
  selectionCounts,
  selectionForKey,
} from "./selection";
import type {
  CellPosition,
  CellRange,
  GridRendererPort,
  RenderCell,
  RenderColumn,
  RendererHandle,
  RendererSelection,
  RendererSpec,
  RowMarkerMode,
  RowSpan,
  VisibleSpan,
} from "./renderer/port";
import { EMPTY_GRID_VIEW, createGridClient, type GridClient } from "./gridClient";

/**
 * この画面の識別子。`src/shell/Layout.tsx` の登録簿が同じ綴りを使うための単一の定義である
 * （検証専用の初期画面の指定 `JXCEL_VERIFICATION_INITIAL_SCREEN=grid` もこの綴りである）。
 */
export const GRID_SCREEN_ID = "grid";

/**
 * 既定の境界の口。**モジュール定数である** — 画面は `ScreenProps` 以外の props を持てないので、
 * 差し替えの口は検査側（`GridScreen.test.ts` が [`loadGridScreenState`] へ渡す偽の実装）にある。
 */
const DEFAULT_CLIENT: GridClient = createGridClient();

/**
 * 移植口の実装。**モジュール定数である**（`mount` だけを持つ状態の無い値であり、面ごとに
 * 作り直す理由が無い）。実物の起動で観測されるのはこの実装である。
 */
const GRID_RENDERER_PORT: GridRendererPort = createGlideAdapter();

// ===========================================================================
// 1. 画面の状態
// ===========================================================================

/**
 * 内容の領域に出す状態。**判別可能な合併型である**（`status` で網羅的に分岐する）。
 */
export type GridScreenState =
  /** 読み込みの途中（器を描く前。効果は [`GridScreen`] が持つ）。 */
  | { readonly status: "loading" }
  /**
   * 開けなかった。`canRetry` は**再試行に意味があるか**である（開く流れの失敗は真、開き直しても
   * 直らない失敗は偽）。
   */
  | { readonly status: "failed"; readonly message: string; readonly canRetry: boolean }
  /** 列が 1 本も宣言されていない（要件 1.6）。**表を描かない。** */
  | { readonly status: "no-schema"; readonly sheetName: string }
  /** 列はあるが行が 1 件も無い（要件 1.5）。**列の構成を示し、行が無いことを示す。** */
  | {
      readonly status: "no-rows";
      readonly sheetName: string;
      readonly columns: readonly ColumnDescriptor[];
    }
  /** 表を描く（行がある）。 */
  | {
      readonly status: "ready";
      readonly sheet: string;
      readonly summary: GridSheetSummary;
      /**
       * 可視行の数（`grid_set_view` の応答）。**窓の区間は可視行の序数である**ため、記憶へ
       * 渡す行数はこれである（シートの行数ではない。絞り込みが効けば両者は食い違う）。
       */
      readonly visibleRows: number;
      /**
       * **絞り込みによって表示されていない行の数**（要件 8.7。8.8 が足した）。
       *
       * 源は `grid_set_view` の応答（`GridViewResponse.hidden_rows`）だけである — **画面は
       * 数え直さない**（数えるにはシートの行数が要り、それは絞り込みの結果ではない）。
       * `visibleRows` とこの数の和が**シートの行数**であることは、ドメインの
       * `RowOrder::hidden` の doc が定めている。
       */
      readonly hiddenRows: number;
      /**
       * **列幅と表示上の列順**（8.8 がここへ載せた。要件 8.1、8.2）。
       *
       * 7.5 はこれを表の面（`GridSurface`）の中に組んでいたが、8.8 は**画面の状態へ上げた**。
       * 上げた理由は、表示上の列順が**表示の位置の空間そのもの**であり、画面の複数の場所が
       * それを読むためである（表が描く列、列ごとの操作の行、違反の提示と巡回の着地点）。
       * 表の面だけが持つと、同じ写像が 2 箇所に現れ、片方だけが並びに追随する
       * （`./viewOps` の module doc「表示の並びは 1 つである」）。
       *
       * **境界へは渡らない**（要件 8.5。設計の割り方の根拠）。この値が `grid_set_view` の
       * 要求に現れないことは、`viewOps.test.ts` と `GridScreen.test.ts` の両方が固定する。
       */
      readonly display: DisplayStateStore;
      /**
       * **描かれる列の内容の同一性**（表示の並びと、描かれる幅。[`layoutKeyOf`]）。
       *
       * 列幅と表示上の列順は、移植口へ**次の `mount` の仕様として**届く（`RendererHandle` に
       * 幅や順を押し込む口が無い。design.md「列幅・列順の反映」）。この鍵は、その組み直しが
       * 要るかを決める 1 つの依存である（[`DisplayState`] は可変の値なので、参照の同一性では
       * 変化を観測できない）。
       */
      readonly layoutKey: string;
      /**
       * **行の集合の同一性**（並べ替えと絞り込み。[`rowOrderKeyOf`]）。
       *
       * これが変わると、**取得済みの窓は別の行を指す**（7.3 の `clear` の doc）— 表の面は
       * この鍵が変わったときに組み直す（窓の記憶も器も作り直す）。**世代だけでは足りない**:
       * 世代は編集でも進むが（`./cellEdit`）、編集は行の並びを変えない（要件 8.8）。
       */
      readonly rowOrderKey: string;
      /**
       * 選択（**現在位置と矩形**。要件 2.1、2.2、2.3）。
       *
       * **この腕が持つことが「表を描いている間は現在位置が 1 つある」を型で表している**
       * （他の腕は持たない — 要件 2.1 は表があるときの性質である）。単体テストはこの値を
       * 描いて数え上げを読み、8.6 / 8.7 / 8.9 はこの値を操作の対象として読む（要件 2.6）。
       */
      readonly selection: RendererSelection;
      /**
       * 編集中のセル（要件 3.1）。`null` なら編集していない。
       *
       * **この腕が持つことが「描かれている表のセルしか編集できない」を型で表している**
       * （他の腕は持たない）。位置だけでなく**開いた時点の表示文字列**を持つ理由は、
       * 入力手段の初期値（`CellEditorProps.initialText`）が要るためであり、開いた時点で写す
       * 理由は、窓の到着で表が描き直されても**入力中の値が足元で変わらない**ためである。
       */
      readonly editing: CellEdit | null;
      /**
       * 表示中のシートに存在する違反の総数（要件 4.3）。**シート全体**の数である。
       *
       * 源は**開いたときの `grid_set_view` の応答**（`GridViewResponse.violation_total`）と、
       * そのあとの適用の応答（`GridEditOutcome.violation_total`。要件 4.6）だけである。
       * **画面は数え直さない** — 数を保つのは索引（`ViolationIndex`）であり、差分で最新に
       * 保たれる（`crates/data-grid` の module docs「違反の総数をどう閉じるか」）。
       *
       * この腕が持つことが「表を描いているときは総数が分かっている」を型で表している
       * （他の腕は `grid_set_view` を呼んでいないので、総数の源が無い）。
       */
      readonly violationTotal: number;
      /**
       * いまの違反の提示（要件 4.2、4.4）。`null` なら出すものが無い。
       *
       * [`ViolationPresentation`] の 2 つの腕は**別の出来事**である — `reason` はいまの位置の
       * 違反の理由（要件 4.2。表が窓の印を見て引く）、`exhausted` は巡回が尽きたこと
       * （要件 4.4 の正常な結果）である。**後者を告知（`notice`）に載せない**のは、利用者の
       * 操作が失敗したわけではないためである。
       *
       * **現在位置が動けば取り下げる**（提示は「いまの位置についてのもの」だからである）。
       */
      readonly violation: ViolationPresentation | null;
      /**
       * いまの表示の指定（**完全な記述**。8.5 が足した）。
       *
       * 並べ替え・絞り込み・展開の 3 つを 1 つの形で持つ（生成物の `GridViewSpec`）。**展開の
       * 状態はここに住む**ので、走査（現在位置の移動・窓の取り直し）では失われない
       * （要件 5.3）。`grid_set_view` へ送るのはつねに**この値に 1 件を足したもの**である
       * （`./nestedInspector` の `withExpansion`）— 押された 1 件だけを送ると、ドメインは
       * 要求に現れない展開を折りたたみへ戻す（`answer_set_view` の規約）。
       */
      readonly view: GridViewSpec;
      /**
       * `GridSession` が持つ**世代**（**10 進の文字列**。タスク 10.1）。
       *
       * **源は境界の応答ただ 1 つである**（`GridOpenResponse` / `GridViewResponse` /
       * `GridEditResponse` の `generation`）— 画面はそれを採用するだけであり、数え直さない。
       * かつての「`grid_set_view` の成功ごとに +1」「`affected` が空でなければ +1」という
       * 規則（`./cellEdit` の `generationAfterEdit`）は**消した**: 展開の適用は 1 つの
       * コマンドの内側で 2 回進むので、数え上げはつねにずれる。**ずれると、以後の窓の要求が
       * 古い世代を名乗り、Rust 側が空の窓を返す**（`WindowCodec::is_stale`）— 取り直した窓は
       * 永久に読み込み中のままになる。
       *
       * 開いた直後の値は `grid_set_view`（空の指定）の応答が運ぶ（`api.rs` の `set_view` が
       * つねに 1 つ進めるので 1 である）。
       */
      readonly generation: string;
      /**
       * 開いている詳細表示（8.5。`null` なら開いていない）。**この腕が持つ**ことが、描かれて
       * いる表のセルにしか詳細表示が無いことを型で表している。
       *
       * 状態に載せる理由は、**入れ子の詳細が表の面の中に描かれる**ためである（窓の記憶を持つのは
       * 表であり、値を読むにはそこが要る）。
       */
      readonly detail: CellDetail | null;
      /**
       * 削除の確認を待っている対象（8.6。要件 6.5）。`null` なら尋ねていない。
       *
       * **この腕が持つ**ことが「描かれている表の行についての確認である」を型で表している
       * （他の腕は表を持たないので、消す行も無い）。求めるかどうかを決めるのは `./rowOps` の
       * 閾値であり（**いま 1 画面に見えている行数**）、状態はその答えを掲げるだけである。
       *
       * **選択が動けば取り下げる**（数が変わりうるので、古い数を掲げたまま尋ねない）。
       * **行数が変わったときも取り下げる**（尋ねた対象はもう無い）。
       */
      readonly pendingDelete: DeleteConfirmation | null;
    };

/**
 * 開いている詳細表示（8.5。要件 4.5、5.5、5.7）。
 *
 * `position` は**表示の位置**である（見出しに 1 起点で名乗る。列の内側の位置は列の記述が持つ）。
 * `edit` は**編集の面の世代**である — 面は打たれた文字を自分の状態に持つので、確定・取消の
 * たびにこの数を進めて作り直す（確定したのに打ちかけの文字が残る、という食い違いを作らない。
 * 失敗では進めない — 適用されていないので、打たれている値を捨てる理由が無い）。
 */
export interface CellDetail {
  readonly position: CellPosition;
  readonly edit: number;
}

/**
 * 編集中の 1 セル（要件 3.1）。
 *
 * `position` は**表示の位置**である（`CellPosition`。移植口の座標と同じ空間）。文書の位置
 * （`GridCellAddress`）へ写すのは確定のときであり、その写像を持つのは窓の記憶である
 * （`./cellEdit` の module doc「宛先は文書の位置である」）。
 */
export interface CellEdit {
  /** 編集しているセル（可視行の序数と列の添字）。 */
  readonly position: CellPosition;
  /** 開いた時点の表示文字列（`WindowCache.getCell` の写し）。 */
  readonly initialText: string;
}

/**
 * 直近の確定の報告（要件 3.4、3.5。**提示の本体は 8.4**）。
 *
 * 2 つを持つのは、この 2 つが**別の要件**であり、片方だけが起きることがあるためである —
 * 変換は起きたが違反は無い（型強制が正常に働いた場合）ことも、その逆もある。1 つの文に
 * 畳むと、どちらの要件の提示なのかが読めなくなる。
 */
export interface CellEditReport {
  /** 型強制の記録（要件 3.4）。**変換の前後をそのまま運ぶ**（解釈は画面に無い）。 */
  readonly coercions: readonly GridCoercionNotice[];
  /** 確定のあとに残っている違反の位置（要件 3.5）。 */
  readonly violations: readonly GridViolationLocation[];
  /**
   * 違反の総数。**シート全体**の数である（生成物の `GridEditOutcome.violation_total` の doc。
   * 適応層が `GridSession::violation_total()` から写す — 再検証した列には閉じない）。
   */
  readonly violationTotal: number;
}

/**
 * 画面の状態機械。**読み込みは試行の番号で駆動する**（読み込みの効果は番号を依存に持ち、
 * 再試行は番号を進める）。
 */
export interface GridScreenModel {
  /** 読み込みの試行の番号（0 が最初。再試行のたびに 1 つ進む）。 */
  readonly attempt: number;
  /** 内容の領域に出す状態。 */
  readonly state: GridScreenState;
  /**
   * 器が捕まえない失敗（イベントハンドラ・非同期）の 1 行。`null` なら無い。
   *
   * **内容の領域とは別に持つ**（選ばれた 1 つの操作の失敗で、表示中の表を失わないためである）。
   */
  readonly notice: string | null;
  /**
   * 直近の確定の報告（要件 3.4、3.5）。`null` なら出すものが無い（提示する型強制も違反も無い）。
   *
   * **確定のたびに置き換わる**（古い変換の記録を新しい確定に重ねない）。**取消と失敗では
   * 動かない** — どちらも文書を変えていないので、前の報告はまだ「直近の確定」のままである。
   */
  readonly editReport: CellEditReport | null;
}

/** 画面の初期状態（読み込みの前）。 */
export function initialGridScreenModel(): GridScreenModel {
  return { attempt: 0, state: { status: "loading" }, notice: null, editReport: null };
}

/** 読み込みの結果を入れる。**試行の番号は動かさない**（番号が動くと読み込みが走り直す）。 */
export function gridScreenLoaded(
  model: GridScreenModel,
  state: GridScreenState,
): GridScreenModel {
  // 開き直しの結果なので、前の告知と前の確定の報告は落とす（古い失敗・古い変換を新しい表示へ
  // 重ねない。開き直せば表の中身そのものが変わりうる）。
  return { attempt: model.attempt, state, notice: null, editReport: null };
}

/** 器が捕まえない失敗を告知として積む（**内容の領域と報告は変えない**）。 */
export function gridScreenFailed(model: GridScreenModel, message: string): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: message,
    editReport: model.editReport,
  };
}

/**
 * 選択（現在位置と矩形）を入れ替える（要件 2.1、2.2、2.3）。**表を描いていないときは何もしない**
 * — 描いていない表に現在位置は無い（`ready` の腕だけが選択を持つ。上の型）。
 *
 * **解除（`null`）は受け取らない。**要件 2.1 は「現在位置となるセルを 1 つ持つ」と言っており、
 * 表を描いている間はつねに 1 つでなければならない。実装が解除を報せてきたとき（Glide の Escape
 * など）は、**いまの選択を新しい値として置き直す** — 器は選択を制御されているので、置き直さない
 * と「解除された選択が描かれたまま、画面の写しは残る」というずれになる（8.1 のレビューが
 * 名指しした危険である）。
 */
export function gridScreenSelectionChanged(
  model: GridScreenModel,
  selection: RendererSelection | null,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  const next = selection ?? { ...model.state.selection };
  const moved = currentPositionMoved(model.state.selection, next);
  return {
    attempt: model.attempt,
    // **動いたら違反の提示を取り下げる**（要件 4.2 の理由は「いまの位置のもの」である。
    // 取り下げないと、動く前に見ていたセルの理由が新しいセルへ貼られたままになる。引き直しは
    // 表が行う — 窓の印を読むには記憶（`./windowCache`）が要るためである）。
    // **削除の確認も取り下げる**（8.6。確認は「その選択を消す」という問いであり、範囲が
    // 変われば数も変わる — 古い数を掲げたまま尋ね続けると、尋ねた数と消える数が食い違う）。
    state: {
      ...model.state,
      selection: next,
      violation: moved ? null : model.state.violation,
      pendingDelete: sameSelection(model.state.selection, next) ? model.state.pendingDelete : null,
    },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/** 現在位置（現在のセル）が動いたか。**列だけの移動も動いたうちである。** */
function currentPositionMoved(before: RendererSelection, after: RendererSelection): boolean {
  return (
    before.current.row !== after.current.row || before.current.column !== after.current.column
  );
}

/** **選択の全体**（現在位置と矩形の両端）が同じか。据え置きの判定と、確認の取り下げに使う。 */
function sameSelection(before: RendererSelection, after: RendererSelection): boolean {
  return (
    !currentPositionMoved(before, after) &&
    before.range.start.row === after.range.start.row &&
    before.range.start.column === after.range.start.column &&
    before.range.end.row === after.range.end.row &&
    before.range.end.column === after.range.end.column
  );
}

/** 告知を閉じる。 */
export function gridScreenNoticeDismissed(model: GridScreenModel): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: null,
    editReport: model.editReport,
  };
}

/** 再試行する（**番号を進め、読み込みの状態へ戻す**）。 */
export function gridScreenRetried(model: GridScreenModel): GridScreenModel {
  return { attempt: model.attempt + 1, state: { status: "loading" }, notice: null, editReport: null };
}

/**
 * セルの編集を開く（要件 3.1）。**表を描いていないときは何もしない** — 描かれていないセルは
 * 編集できない（`ready` の腕だけが `editing` を持つ。上の型）。
 *
 * 初期値は**呼び出し側が渡す**（開いた時点の表示文字列）。画面の状態に入れるのは、窓の到着で
 * 表が描き直されても入力中の値が動かないようにするためである（`CellEdit` の doc）。
 */
export function gridScreenEditStarted(
  model: GridScreenModel,
  position: CellPosition,
  initialText: string,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  return {
    attempt: model.attempt,
    state: { ...model.state, editing: { position, initialText } },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/**
 * 確定の 1 往復（`./cellEdit`）の結果を画面へ反映する（要件 3.3、3.4、3.5、3.6）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 取消 | **入力手段を閉じるだけである。**文書も表示も報告も動かない（要件 3.6 — 適用の前なので、値はまだ変わっていない） |
 * | 適用 | 入力手段を閉じ、**報告を置き換える**（型強制と、残った違反。要件 3.4、3.5） |
 * | 適用できなかった | **入力手段は開いたままにする。**適用されていないので、打たれている値を閉じて捨てる理由が無い（理由は 8.1 の告知として出す） |
 *
 * 報告が `null` になるのは、**提示するものが 1 つも無い**ときである（変換も違反も無い確定）—
 * そのときは古い報告も落ちる（報告は「直近の確定」を指す。前の変換を新しい確定に重ねない）。
 */
export function gridScreenEditSettled(
  model: GridScreenModel,
  settlement: CellEditSettlement,
): GridScreenModel {
  switch (settlement.status) {
    case "cancelled":
      return gridScreenEditClosed(model);
    case "failed":
      // **適用されていない。**打たれている値を閉じて捨てる理由が無いので、開いたままにする。
      return gridScreenFailed(model, `編集を適用できませんでした: ${settlement.message}`);
    case "applied": {
      const closed = gridScreenEditClosed(model);
      return {
        ...closed,
        editReport: reportOf(settlement.outcome),
        state: stateAfterEdit(closed.state, settlement.outcome, settlement.generation),
      };
    }
    default:
      return assertNever(settlement, "確定の結果の分岐が網羅されていない");
  }
}

/** 確定の報告を閉じる（**文書も値も動かない** — 提示を消すだけである）。 */
export function gridScreenEditReportDismissed(model: GridScreenModel): GridScreenModel {
  return {
    attempt: model.attempt,
    state: model.state,
    notice: model.notice,
    editReport: null,
  };
}

/**
 * 適用の結果を、表を描く状態へ反映する（要件 4.6）。
 *
 * 総数を `GridEditOutcome.violation_total`（**シート全体**の数）で置き換え、**いま出している
 * 違反の提示を取り下げる**。取り下げるのは、そのセルの違反が解消されたかどうかがこの時点では
 * 分からないためである — 分かるのは窓の印の取り直し（7.3 の `invalidate` の経路）と、
 * 境界への問い合わせ（`./violations` の `reasonInRow`）であり、どちらもこの遷移の外で起きる。
 * **取り下げておけば、解消された違反の提示が残ることはない**（残ると要件 4.6 が満たされない）。
 *
 * 世代もここで入れる（タスク 10.1）。**値は適用の応答が運ぶものであり、画面は数え直さない**
 * （かつての `generationAfterEdit` は消した — ドメインの規則の写しを 2 つ持つと、片方だけが
 * 正しいまま残る）。入れないと、以後の窓の要求が古い世代を名乗り、**取り直した窓が永久に
 * 読み込み中のまま**になる。
 *
 * 結果が無い（`None`。`grid_history` の腕）ときは**何も動かさない** — 適用していないので、
 * 総数を変える根拠が無い（生成物の doc が「適用では `None` を取り得ない」と定めている）。
 */
function stateAfterEdit(
  state: GridScreenState,
  outcome: GridEditOutcome | null,
  generation: string,
): GridScreenState {
  if (state.status !== "ready" || outcome === null) {
    return state;
  }
  return {
    ...state,
    violationTotal: outcome.violation_total,
    violation: null,
    // **応答が運ぶ世代をそのまま入れる**（タスク 10.1。数え直さない）。
    generation,
  };
}

// ===========================================================================
// 2.5 入れ子の展開と詳細表示（8.5。要件 4.5、5.1〜5.7）
// ===========================================================================

/**
 * 表示の指定を送った結果（**画面が状態を決めるのに要るものだけ**）。
 *
 * 成功の腕は**送った指定をそのまま持ち帰る** — 状態へ入れるのは「境界が受け取った指定」で
 * なければならない（組み立て直すと、送ったものと入れたものが食い違いうる）。**導出後の列の
 * 構成も持ち帰る**（境界が唯一の源である。組み立て直すと、ドメインが導出した構成と画面が
 * 描く列が食い違いうる）。
 */
export type GridViewSettlement =
  | {
      readonly status: "applied";
      readonly view: GridViewSpec;
      /**
       * **導出後**の列の構成（左から右への表示順。入れ子の展開を含む。要件 5.1、5.2）。
       *
       * `grid_set_view` の応答が運ぶものであり、**展開・折りたたみが見える唯一の源**である
       * （据え置くと、展開を指定しても描かれる列が変わらない）。
       */
      readonly columns: readonly ColumnDescriptor[];
      /** 可視行数（絞り込みが効けばシートの行数と違う）。窓が覆う行数である。 */
      readonly visibleRows: number;
      /** **絞り込みによって表示されていない行の数**（要件 8.7。応答が運ぶ数そのものである）。 */
      readonly hiddenRows: number;
      /** **シート全体**の違反の総数（要件 4.3）。 */
      readonly violationTotal: number;
      /**
       * **応答を組み立てた時点の世代**（10 進の文字列。`GridViewResponse.generation` そのもの）。
       *
       * `grid_set_view` は 1 つの呼び出しの内側で世代を複数回進めるので（折りたたみ・順序の
       * 導出・展開の適用）、画面が +1 で数えることはできない。
       */
      readonly generation: string;
    }
  | { readonly status: "failed"; readonly message: string };

/**
 * 表示の指定を境界へ送る（要件 5.1、5.2、8.3、8.4）。**開く流れと同じ規律の純粋な非同期関数である**
 * （効果はこれを呼ぶだけであり、検査は偽の境界を渡して「何を送ったか」を読める）。
 *
 * 例外を投げない（`GridClient` の口は封筒の失敗を値で返す）。
 */
export async function applyGridView(
  client: GridClient,
  view: GridViewSpec,
): Promise<GridViewSettlement> {
  const answer = await client.setView(view);
  if (answer.status === "error") {
    return {
      status: "failed",
      message: `表示の指定を適用できませんでした: ${describeIpcError(answer.error)}`,
    };
  }
  return {
    status: "applied",
    view,
    columns: answer.data.columns,
    visibleRows: answer.data.visible_rows,
    // **隠れている行の数も応答が運ぶ**（要件 8.7）。画面は数え直さない。
    hiddenRows: answer.data.hidden_rows,
    violationTotal: answer.data.violation_total,
    // **世代も応答が運ぶ**（タスク 10.1）。画面は数え直さない。
    generation: answer.data.generation,
  };
}

/**
 * 2 つの構成が**同じ並び**を表すか（表示の位置ごとに同じ列が並んでいるか）。
 *
 * 同一性の綴りは [`columnKey`]（`ColumnDescriptor` の doc が定める（`column`, `path`）の対）で
 * ある。**名前や札まで見ない**のは、それらが位置の対から決まる導出物だからである
 * （同じ位置の対なら表示名も葉の型も同じである）。
 *
 * 状態へ構成を入れるときに使い、同じ並びなら**前の値（参照）を据え置く** — 表の面
 * （`GridSurface`）は `summary` を依存に持つ 1 つの効果で器と窓の記憶を組むので、同一性が
 * 落ちると**組み直し**、走査の位置（表示範囲）と取得済みの窓を捨てることになる。
 * 並べ替え・絞り込みは列の構成を変えない（`derive_layout` は展開だけを読む）ため、この据え置きが
 * 効くのはそれらの指定である。
 */
function sameLayout(
  left: readonly ColumnDescriptor[],
  right: readonly ColumnDescriptor[],
): boolean {
  return (
    left === right ||
    (left.length === right.length &&
      left.every((column, display) => {
        const other = right[display];
        return other !== undefined && columnKey(column) === columnKey(other);
      }))
  );
}

/**
 * 表示の指定の適用を画面へ反映する（要件 5.1、5.2、5.3）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 適用された | **送った指定をそのまま状態へ入れる**（展開の状態がここに住むので、走査では失われない）。**導出後の列の構成を採用し**、可視行数・違反の総数を置き換え、**世代は応答が運ぶ値を採用する**（数え直さない） |
 * | 適用できなかった | **状態を 1 つも動かさず**、理由を告知として出す（世代も進めない — 進めると、窓の要求が存在しない世代を名乗る） |
 *
 * # 列の構成を採用することが要件 5.1 / 5.2 の見える結果である
 *
 * 境界は**導出後**の構成（`GridViewResponse.columns`）を返すので、状態の `summary` をその
 * 並びで置き換える。**これが無いと、展開を指定しても描かれる列は変わらない** — 画面は開いた
 * ときの構成を描き続ける（8.5 の申し送り 2 が実測した欠陥であり、本遷移がその修復である）。
 *
 * 表の面（`GridSurface`）は `summary` を依存に持つ 1 つの効果で器・窓の記憶・移植口を組むので、
 * 構成が変われば**同じ 1 つの経路**で組み直る（移植口に列を後から差し替える口が無いため、
 * 組み直しが唯一の道である）。組み直しは**表示範囲と取得済みの窓を捨てる**（列の数が変わる
 * 以上、走査の位置は元の意味を保たない）。構成が**同じ並び**であるときは据え置く（[`sameLayout`]）
 * — 窓が運ぶ列は変わらないので、記憶している窓の内容はそのまま正しい（変わったのは世代だけ
 * である — `GridSurface` が世代を記憶へ下ろす）。並べ替え・絞り込みのように**行の並びを変える**
 * 指定は `rowOrderKey` が変わる — 8.8 はそれを組み立ての効果の依存に入れた（**`WindowCache.clear`
 * を別に呼ばない**。序数を鍵とする窓は行の集合が変われば別の行を指すので、**組み直しが同じ
 * ことをする**。合図が 2 つあると、片方だけが古い記憶を残す日が来る）。
 */
export function gridScreenViewSettled(
  model: GridScreenModel,
  settlement: GridViewSettlement,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  switch (settlement.status) {
    case "applied": {
      // **表示状態は列数が変わったら作り直す**（7.5 の規律 — 列数は作るときに 1 度だけ受け取り、
      // `columnOrder` は常に「いまの列数」の置換である）。列数が同じなら据え置く（幅と並びは
      // 利用者の操作の結果であり、指定の適用で捨てる理由が無い）。
      const display =
        settlement.columns.length === model.state.summary.columns.length
          ? model.state.display
          : createDisplayState({ columnCount: settlement.columns.length });
      return {
        attempt: model.attempt,
        state: {
          ...model.state,
          view: settlement.view,
          // **導出後の構成を採用する**（要件 5.1、5.2）。同じ並びなら前の値を据え置く
          // （器と窓の記憶を無駄に組み直さない）。境界の型は可変の並び（`Array`）なので写す。
          summary: sameLayout(model.state.summary.columns, settlement.columns)
            ? model.state.summary
            : { ...model.state.summary, columns: [...settlement.columns] },
          visibleRows: settlement.visibleRows,
          // **隠れている行の数も採用する**（要件 8.7）。表示の指定の適用がその数を知る唯一の
          // 経路である（編集の応答は行の側の数を運ばない）。
          hiddenRows: settlement.hiddenRows,
          display,
          // 組み直しの 2 つの合図を**採用した値から引き直す**（構成が変われば列数も変わり、
          // 表示状態は作り直されるので、幅と並びの鍵も新しい構成のものになる）。
          layoutKey: layoutKeyOf(display),
          rowOrderKey: rowOrderKeyOf(settlement.view),
          violationTotal: settlement.violationTotal,
          // **縮んだ構成へ現在位置を寄せる**（要件 2.1、5.2）。入れ子を折りたたむと列数が減り、
          // 最後の列にあった現在位置が**描かれる表の外**へ残る — そのまま移植口へ渡すと、
          // 数え上げの行の文言と描かれている列が食い違う（境界の修復のレビューが実測）。
          selection: clampSelection(model.state.selection, {
            rowCount: settlement.visibleRows,
            columnCount: settlement.columns.length,
          }),
          // **行の集合が変われば確認を取り下げる**（8.6。並べ替え・絞り込みは「何番目の行が
          // どの行か」を変えるので、尋ねた対象はもう同じ行ではない）。行数が動かない指定
          // （入れ子の展開）では据え置く。
          pendingDelete:
            settlement.visibleRows === model.state.visibleRows
              ? model.state.pendingDelete
              : null,
          // **応答が運ぶ世代をそのまま入れる**（タスク 10.1）。`grid_set_view` は 1 つの
          // 呼び出しの内側で世代を複数回進める（折りたたみ・順序の導出・展開の適用）ので、
          // +1 の数え上げでは追いつけない。
          generation: settlement.generation,
          // 詳細表示は**開いたままにする**（構成が変わったことを理由に閉じる理由が無い。位置は
          // 表示の位置であり、いまの構成の同じ位置の列を詳しく見せる — design.md の 8.5 の表）。
        },
        notice: model.notice,
        editReport: model.editReport,
      };
    }
    case "failed":
      // **適用されていないので、状態は 1 つも動かさない**（打たれた指定を捨てる理由が無い）。
      return gridScreenFailed(model, settlement.message);
    default:
      return assertNever(settlement, "表示の指定の結果の分岐が網羅されていない");
  }
}

/**
 * 詳細表示を開く（要件 4.5、5.5）。**表を描いていないときは何もしない**（描かれていない表の
 * セルに詳細表示は無い）。
 *
 * 同じ位置を開き直したときは**編集の面の世代を進める**（作り直す）— 前に打ちかけた文字を
 * 持ち越さない。
 */
export function gridScreenDetailOpened(
  model: GridScreenModel,
  position: CellPosition,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  const current = model.state.detail;
  const edit = current !== null && samePosition(current.position, position) ? current.edit + 1 : 0;
  return {
    attempt: model.attempt,
    state: { ...model.state, detail: { position, edit } },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/** 詳細表示を閉じる（**値も文書も動かない** — 表示を消すだけである）。 */
export function gridScreenDetailClosed(model: GridScreenModel): GridScreenModel {
  if (model.state.status !== "ready" || model.state.detail === null) {
    return model;
  }
  return {
    attempt: model.attempt,
    state: { ...model.state, detail: null },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/**
 * 詳細表示の中の編集の 1 往復を画面へ反映する（要件 5.7）。**規律はセルの編集と同一である**
 * — 遷移そのものが [`gridScreenEditSettled`] である（報告・告知・世代・取消の扱いを 2 度
 * 書かない）。
 *
 * 違うのは 1 点だけである: セルの編集は**入力手段を閉じる**が、詳細表示は**面を初期状態へ
 * 戻す**（鍵を進める）。取消と確定の両方で戻す — どちらも「その編集は終わった」であり、
 * 打ちかけの文字を次の編集へ持ち越さない。失敗では戻さない（適用されていないので、打たれて
 * いる値を捨てる理由が無い — セルの編集と同じである）。
 */
export function gridScreenDetailEditSettled(
  model: GridScreenModel,
  settlement: CellEditSettlement,
): GridScreenModel {
  const settled = gridScreenEditSettled(model, settlement);
  if (settled.state.status !== "ready" || settled.state.detail === null) {
    return settled;
  }
  if (settlement.status === "failed") {
    return settled;
  }
  return {
    ...settled,
    state: {
      ...settled.state,
      detail: { position: settled.state.detail.position, edit: settled.state.detail.edit + 1 },
    },
  };
}

/** 同じセルか（表示の位置どうしの比較）。 */
function samePosition(before: CellPosition, after: CellPosition): boolean {
  return before.row === after.row && before.column === after.column;
}

/**
 * 現在位置の違反の理由の読み取りを画面へ反映する（要件 4.2、4.6）。**表が窓の印を見て引いた
 * 結果**がここへ来る（現在位置を動かすのは巡回であり、こちらではない）。
 *
 * 「尽きた」（`exhausted`）はこの経路では起きない（型は巡回と共有している）。起きたときに
 * 何もしないのは、**理由の経路の答えとして「違反がもう無い」と言う根拠が無い**ためである
 * （探索は行の単位で尽きるが、いまの位置についての言明ではない）。
 */
export function gridScreenViolationReason(
  model: GridScreenModel,
  reading: ViolationReading,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  switch (reading.kind) {
    case "reason":
      return withViolation(model, {
        kind: "reason",
        position: reading.position,
        reason: reading.reason,
      });
    case "cleared":
      return withViolation(model, null);
    case "exhausted":
      return model;
    case "failed":
      return gridScreenFailed(model, `違反の理由を取得できませんでした: ${reading.message}`);
    default:
      return assertNever(reading, "違反の読み取りの分岐が網羅されていない");
  }
}

/**
 * 巡回（「次の違反へ」）の読み取りを画面へ反映する（要件 4.4）。
 *
 * | 読み取り | 何が起きるか |
 * |---|---|
 * | 違反が見つかった | **現在位置をその位置へ移す**（範囲は 1 セルへ畳む。[`selectionAt`]）— 表示範囲の外にあっても、追随（要件 2.4）が `scrollTo` を呼ぶ。移った先の理由をその場で出す |
 * | 尽きた | **現在位置を動かさない。**「これ以上違反はありません」を出す（**失敗ではない**） |
 * | 取り下げ | 提示を消す（この経路では起きない） |
 * | 失敗した | 現在位置を動かさず、理由を**告知**へ出す（経路の失敗である） |
 */
export function gridScreenNextViolation(
  model: GridScreenModel,
  reading: ViolationReading,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  switch (reading.kind) {
    case "reason": {
      // **移ってから提示を置く**（移ると選択の遷移が提示を取り下げるためである）。
      const moved = gridScreenSelectionChanged(model, selectionAt(reading.position));
      return withViolation(moved, {
        kind: "reason",
        position: reading.position,
        reason: reading.reason,
      });
    }
    case "exhausted":
      return withViolation(model, { kind: "exhausted" });
    case "cleared":
      return withViolation(model, null);
    case "failed":
      return gridScreenFailed(model, `次の違反を取得できませんでした: ${reading.message}`);
    default:
      return assertNever(reading, "違反の読み取りの分岐が網羅されていない");
  }
}

/**
 * 違反の提示を入れ替える（**表を描いていないときは何もしない**）。
 *
 * 同じ値を置き直すときは**同じ model を返す**（状態を替えない遷移は React に再描画を
 * 起こさせない — 窓の到着のたびに引き直す経路があるので、無駄な再描画を作らない）。
 */
function withViolation(
  model: GridScreenModel,
  violation: ViolationPresentation | null,
): GridScreenModel {
  if (model.state.status !== "ready" || model.state.violation === violation) {
    return model;
  }
  return { ...model, state: { ...model.state, violation } };
}

/** 入力手段を閉じる（**表を描いていないときは何もしない**）。 */
function gridScreenEditClosed(model: GridScreenModel): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  return {
    attempt: model.attempt,
    state: { ...model.state, editing: null },
    notice: model.notice,
    editReport: model.editReport,
  };
}

/**
 * 適用の結果を報告へ写す（要件 3.4、3.5）。
 *
 * **解釈を 1 つも足さない** — 変換の前後の表示文字列も、違反の位置も、総数も、境界が運んだ値を
 * そのまま置く。出すものが 1 つも無ければ `null` を返す（空の報告を出しても、利用者には
 * 「何かあった」と読める枠が残るだけである）。
 */
function reportOf(outcome: GridEditOutcome | null): CellEditReport | null {
  if (outcome === null) {
    // 適用では起こらない（生成物の doc）。`grid_history` の腕のための守りである。
    return null;
  }
  const nothing =
    outcome.coercions.length === 0 &&
    outcome.violations.length === 0 &&
    outcome.violation_total === 0;
  if (nothing) {
    return null;
  }
  return {
    coercions: outcome.coercions,
    violations: outcome.violations,
    violationTotal: outcome.violation_total,
  };
}

// ===========================================================================
// 2.6 行の増減（8.6。要件 6.1、6.2、6.3、6.5）
// ===========================================================================

/**
 * 削除の確認を掲げる（要件 6.5）。**境界へは何も送らない** — 数を示して尋ねるだけである。
 *
 * 尋ねるかどうかを決めるのは `./rowOps` の閾値であり（**いま 1 画面に見えている行数**）、
 * この遷移はその答えを状態へ置くだけである（表を描いていないときは何もしない — 描かれていない
 * 表の行は消せない）。
 */
export function gridScreenDeleteRequested(
  model: GridScreenModel,
  confirmation: DeleteConfirmation,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  return { ...model, state: { ...model.state, pendingDelete: confirmation } };
}

/**
 * 確認への取り消し（要件 6.5）。**境界へ 1 つも送らない** — 取り消しは「適用しない」ことで
 * あり、適用する操作は文書へ届いていない（`./rowOps` の取り消しの腕は送る腕を持たない）。
 * 窓の記憶も触らない（表示は変わっていない）。
 *
 * 文書を触っていないので、**確定の報告と告知も動かさない**（`./cellEdit` の取消と同じ規律で
 * ある。取消は「直前の操作が何か」を変えない）。
 */
export function gridScreenDeleteCancelled(model: GridScreenModel): GridScreenModel {
  return withPendingDelete(model, null);
}

/**
 * 行の操作の 1 往復（`./rowOps`）の結果を画面へ反映する（要件 6.1、6.2、6.3、6.5、1.7）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 適用された | **行数・可視行数・提示する数を置き換え**、現在位置と選択を新しい表の範囲へ寄せ、**確認を取り下げる** |
 * | 適用できなかった | **確認は開いたままにする。**適用されていないので、取り下げる理由が無い（理由は 8.1 の告知として出す） |
 *
 * **取り消しはこの遷移を通らない**（`./rowOps` の計画の腕であり、往復を 1 つも起こさない
 * — [`gridScreenDeleteCancelled`] がそれである）。
 *
 * 行数を置き換えるのは**2 つの値**である。① **窓が覆う行数**（`visibleRows`。移植口へ渡す
 * 数であり、これが据え置かれると増えた行を永久に読めない）② **提示する行数**（`summary.row_count`。
 * 要件 6.2 が示す数である）。絞り込みが無い間は両者は等しい（可視の順序は文書の順序そのもので
 * ある）ので、**境界が運ぶのはシートの行数のほう**であり（`GridEditOutcome.row_count`）、
 * それを両方へ置く。8.8 が絞り込みを結線するときは、可視行数を別に取り直すこと（絞り込みが
 * 隠した行を足したときに、この等式は崩れる）。
 *
 * 現在位置と選択を寄せるのは**列の側で 8.5 が直したのと同じ欠陥**を閉じるためである — 行が
 * 減ったのに位置が残ると、数え上げの行は「現在位置 10 行」と名乗るのに描かれている行は 3 行、
 * という**利用者に見える食い違い**になる。
 *
 * 消えた行に開いている面（入力手段・詳細表示）は**閉じる**。開いたまま残すと、確定の宛先
 * （行の識別子。窓にしか無い）を引けず、「まだ届いていない」という**理由として誤った**告知に
 * なる（消えたのである）。
 */
export function gridScreenRowOperationSettled(
  model: GridScreenModel,
  settlement: RowOperationSettlement,
): GridScreenModel {
  switch (settlement.status) {
    case "failed":
      // **適用されていない。**確認を取り下げる理由が無いので、開いたままにする（セルの編集が
      // 失敗したときに入力手段を開いたままにするのと同じ規律である）。
      return gridScreenFailed(model, `行の操作を適用できませんでした: ${settlement.message}`);
    case "applied":
      return appliedRowOperation(model, settlement.outcome, settlement.generation);
    default:
      return assertNever(settlement, "行の操作の結果の分岐が網羅されていない");
  }
}

/**
 * 貼り付けの 1 往復（`./clipboard`）の結果を画面へ反映する（8.7。要件 7.3、7.4、1.7）。
 *
 * **反映の形は 8.6 の行の操作と同じ 1 つである**（[`appliedRowOperation`] を通る）— 貼り付けは
 * **行を補充しうる**ので、行数を置き換え、現在位置と選択を寄せ、消えた行の面を閉じる必要が
 * ある。形を 2 つに割ると、片方だけが寄せを持つ日が来る（利用者には「現在位置 10 行」と名乗り
 * ながら表は 3 行、という食い違いとして現れる）。
 *
 * **適用できなかったときは内容を消さない**（1 行の告知として理由を出す。8.1 の失敗の隔離の表）。
 */
export function gridScreenPasteSettled(
  model: GridScreenModel,
  settlement: PasteSettlement,
): GridScreenModel {
  switch (settlement.status) {
    case "failed":
      return gridScreenFailed(model, `貼り付けを適用できませんでした: ${settlement.message}`);
    case "applied":
      return appliedRowOperation(model, settlement.outcome, settlement.generation);
    default:
      return assertNever(settlement, "貼り付けの結果の分岐が網羅されていない");
  }
}

/**
 * 取り消しとやり直しの 1 往復（8.9。要件 9.2、9.3、9.8）の結果を画面へ反映する。
 *
 * **反映の形は 8.6 / 8.7 と同じ 1 つである**（[`appliedRowOperation`] を通る）— 取り消しは
 * **行数を変えうる**ので、提示する行数・窓が覆う行数・現在位置と選択の寄せ・消えた行の面を
 * 閉じることが要る。形を 2 つに割れば、片方だけが寄せを持つ日が来る（利用者には「現在位置
 * 10 行」と名乗りながら表は 3 行、という食い違いとして現れる）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 適用された | 行数と位置を置き換え、**対象となった範囲へ現在位置を移す**（要件 9.8） |
 * | 進める履歴が無い | **何も動かさない**（失敗でも、告知を出す理由でもない。要件 9.2、9.3） |
 * | 適用できなかった | 理由を**告知**へ出す（表も状態も動かない） |
 *
 * **対象となった範囲へ移す**のは**応答が運ぶ表示の序数**（`GridEditOutcome.affected_ordinals`）
 * であり、`./history` は序数を 1 つも解決しない（10.5 が窓の記憶からの解決を消した）。移す先が
 * 無ければ（序数が空）**動かさない** — 推測した序数へ動かせば、無関係な行を名乗ることになる
 * （要件 8.6 の取り違えの行版）。
 *
 * 移した先が新しい表の範囲の外なら、**既存の寄せ**（[`clampSelection`]）が表の端へ寄せる
 * （8.6 が行数の減少で通るのと同じ道である）。移した選択が表示範囲の外にあれば、**追随**
 * （要件 2.4）が `handle.scrollTo` を呼んで見えるようにする — 9.8 の後半はこの既存の 1 本で
 * 満たす（`./history` の module doc「移動した先が見えること」）。
 */
export function gridScreenHistorySettled(
  model: GridScreenModel,
  settlement: HistorySettlement,
): GridScreenModel {
  switch (settlement.status) {
    case "failed":
      return gridScreenFailed(model, `取り消し・やり直しを実行できませんでした: ${settlement.message}`);
    case "empty":
      // **文書が動いていない。**作り直すものも、移すものも、名乗るものも無い（要件 9.2、9.3）。
      return model;
    case "applied":
      return appliedRowOperation(model, settlement.outcome, settlement.generation);
    default:
      return assertNever(settlement, "履歴の結果の分岐が網羅されていない");
  }
}

/** 確認を入れ替える（**表を描いていないときは何もしない**）。 */
function withPendingDelete(
  model: GridScreenModel,
  confirmation: DeleteConfirmation | null,
): GridScreenModel {
  if (model.state.status !== "ready" || model.state.pendingDelete === confirmation) {
    return model;
  }
  return { ...model, state: { ...model.state, pendingDelete: confirmation } };
}

/** 適用の結果を行の状態へ反映する（[`gridScreenRowOperationSettled`] の本体）。 */
function appliedRowOperation(
  model: GridScreenModel,
  outcome: GridEditOutcome | null,
  /**
   * **応答を組み立てた時点の世代**（10 進の文字列。タスク 10.1。行の操作・貼り付け・履歴の
   * どの経路も、適用の応答が運ぶ値をそのまま渡す）。
   */
  generation: string,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  if (outcome === null) {
    // 適用では起こらない（生成物の doc。`grid_history` の腕である）。**何も動かさず、確認だけ
    // 取り下げる** — 尋ねた対象が適用されたかどうかは、この腕では言えない。
    return withPendingDelete(model, null);
  }
  const state = model.state;
  const rowCount = outcome.row_count;
  const columnCount = state.summary.columns.length;
  /**
   * いまの表の行数（**窓が覆う数**）。
   *
   * 絞り込みが無い間は可視の順序が文書の順序そのものであるので、適用の応答が運ぶ行数
   * （`GridEditOutcome.row_count` ＝ シートの行数）をそのまま使える。**指定が行を絞っている間は
   * 使えない** — 応答の数はシートの行数であり可視行数ではない（要件 8.7 の 2 つの数の
   * 区別）。その場合は**いまの可視行数を据え置き**、画面が表示の指定を送り直して取り直す
   * （[`needsViewRefresh`]）。
   */
  const rowBound = hasRowRestriction(state.view) ? state.visibleRows : rowCount;
  /**
   * 現在位置を移す先（**表示の序数**。要件 9.8）。**応答が運ぶ影響を受けた行の序数の先頭**
   * である — 行の識別子（`affected`）ではなく、窓の記憶でもない。
   *
   * 序数を写すのは `RowOrder` を持つ適応層であり（10.5）、**写せなかった行は応答に載らない**
   * （順序に無い行＝削除で消えた行・絞り込みで隠れた行・別のシートの行）。したがって
   * **空なら移す先が無い** — 動かさない（推測した序数へ動かせば、無関係な行を名乗る。要件 8.6 の
   * 取り違えの行版）。10.5 より前は `./history` が窓の記憶から引いており、**記憶がその行を
   * 保っていなければ引けなかった**（8.9 のレビューが実測した最小の再現は行の追加のやり直しで
   * ある）。経路は 1 つである（行の操作・貼り付け・履歴のどれもこの遷移を通る）。
   */
  const target = outcome.affected_ordinals[0] ?? null;
  /**
   * 反映の出発点になる選択（要件 9.8）。
   *
   * **移す先があるとき（取り消し・やり直し・行の操作・貼り付け）は、その行へ現在位置を移して
   * 選択を 1 セルへ畳む。**列は現在位置のものを保つ（境界が運ぶのは行の序数だけであり、
   * 影響を受けた列は分からない）。移す先が無ければいまの選択のままであり、**下の寄せだけ**を
   * 受ける（行を消した適用である）。
   */
  const moved =
    target === null
      ? state.selection
      : selectionAt({ row: target, column: state.selection.current.column });
  return {
    attempt: model.attempt,
    state: {
      ...state,
      // 提示する行数（要件 6.2 の数）。同じ数なら前の値を据え置く（要約の同一性は表の面の
      // 組み直しの判断に効く）。
      summary:
        state.summary.row_count === rowCount
          ? state.summary
          : { ...state.summary, row_count: rowCount },
      // **窓が覆う行数も新しい行数である**（上の `rowBound`）。
      visibleRows: rowBound,
      // **縮んだ表へ現在位置と選択を寄せる**（要件 6.5 の「行の位置の提示が直ちに更新される」）。
      // 8.9 の移動先が新しい範囲の外にある場合も、この 1 つの寄せが端へ寄せる。
      selection: clampSelection(moved, { rowCount: rowBound, columnCount }),
      // 消えた行に開いていた面は閉じる（`rowBound === 0` なら両方とも閉じる）。
      editing:
        state.editing !== null && state.editing.position.row >= rowBound ? null : state.editing,
      detail: state.detail !== null && state.detail.position.row >= rowBound ? null : state.detail,
      pendingDelete: null,
      // 違反の総数は適用の応答が運ぶ数（**シート全体**）で置き換え、いまの提示は取り下げる
      // （要件 4.6。解消されたかどうかは、窓の印の取り直しと境界への問い合わせで決まる）。
      violationTotal: outcome.violation_total,
      violation: null,
      // **応答が運ぶ世代をそのまま入れる**（タスク 10.1。数え直さない — 行の追加・削除・
      // 貼り付けの補充・取り消しはどれも世代を進めうるし、進めない適用もある）。
      generation,
    },
    notice: model.notice,
    editReport: model.editReport,
  };
}

// ===========================================================================
// 2.7 表示の操作（8.8。要件 8.1、8.2、8.3、8.4、8.5、8.7）
// ===========================================================================

/**
 * 列の幅を変える（要件 8.1）。`RendererSpec.onColumnResize` の知らせと、列ごとの操作の入力の
 * **両方がここへ来る**（同じ空間（表示位置）を取り、同じ 1 つの状態を動かす）。
 *
 * **境界へは 1 つも送らない。**列幅は窓の中身を変えないので、保存される列の順序を変える経路が
 * 無い（要件 8.5）— 送る口を作れば、その経路が生まれる。
 *
 * 返すのは**新しいモデル**である（値が変わったときだけ）。変わらない知らせ（範囲の外、
 * 同じ幅）では**引数をそのまま返す** — 状態は可変の値（[`DisplayStateStore`]）なので、
 * 参照の同一性だけが「変わっていない」ことの印であり、そのまま返すことが組み直しを止める。
 */
export function gridScreenColumnResized(
  model: GridScreenModel,
  displayPosition: number,
  width: number,
): GridScreenModel {
  return withDisplayChange(model, (display) => {
    display.setColumnWidth(displayPosition, width);
  });
}

/**
 * 列を表示の並びの中で運ぶ（要件 8.2）。`RendererSpec.onColumnMove` の知らせと、列ごとの左右の
 * 操作の**両方がここへ来る**（幅と同じ理由である）。
 *
 * **幅は動かない** — 幅の鍵は列そのものであり、位置ではない（7.5 の規則。並びを変えても同じ列が
 * 同じ幅で描かれる）。
 */
export function gridScreenColumnMoved(
  model: GridScreenModel,
  from: number,
  to: number,
): GridScreenModel {
  return withDisplayChange(model, (display) => {
    display.moveColumn(from, to);
  });
}

/**
 * 表示状態（幅・並び）への変更を 1 つの形で反映する（[`gridScreenColumnResized`] /
 * [`gridScreenColumnMoved`] の本体）。
 *
 * **変えたあとの鍵と比べて、変わっていなければ状態を据え置く** — 範囲の外の知らせや恒等の移動は
 * 状態を変えない（7.5 の規律）ので、組み直しの合図も動かしてはならない（動かすと、描画層が
 * 宣言の外の位置を報せるたびに器を作り直すことになる）。
 */
function withDisplayChange(
  model: GridScreenModel,
  change: (display: DisplayStateStore) => void,
): GridScreenModel {
  if (model.state.status !== "ready") {
    // 表を描いていないときは列が無い（知らせの宛先が無い。7.5 の「範囲の外の入力と同じ扱い」）。
    return model;
  }
  const before = model.state.layoutKey;
  change(model.state.display);
  const after = layoutKeyOf(model.state.display);
  if (after === before) {
    return model;
  }
  return { ...model, state: { ...model.state, layoutKey: after } };
}

/**
 * 行を増減したあとに、**表示の指定を送り直して数を取り直す**必要があるか（要件 8.7）。
 *
 * 要るのは 2 つが同時に成り立つときだけである。
 *
 * 1. **行の集合が変わった**（適用の応答の行数が、適用前のシートの行数と違う）。値だけを書く
 *    適用（`SetCells` と、行を補充しない `PasteRange`）ではドメインも順序を導出し直さないので
 *    （`api.rs` の `settle`「値だけの編集では順序を導出し直さない」）、画面が数を取り直す理由が
 *    無い — **そして取り直してはならない**: 絞り込みの条件に合う値へ書き換えた行が、確定と
 *    同時に画面から消えることになる（要件 8.8 が名指しした「編集した行を見失う」経路である）。
 * 2. **行の集合を絞っている指定が効いている**（[`hasRowRestriction`]）。指定が無ければ、適用の
 *    応答が運ぶ行数がそのまま可視行数である（可視の順序は文書の順序そのものである）。
 *
 * 数を知る唯一の源は `grid_set_view` の応答である（`GridViewResponse.visible_rows` /
 * `hidden_rows`）— 適用の応答は行の側の数を運ばない（生成物の `GridEditOutcome` の doc）。
 */
export function needsViewRefresh(options: {
  readonly view: GridViewSpec;
  readonly outcome: GridEditOutcome | null;
  /** 適用の前の**シートの行数**（`ready.summary.row_count`）。 */
  readonly sheetRowsBefore: number;
}): boolean {
  const outcome = options.outcome;
  if (outcome === null || outcome.affected.length === 0) {
    // 何も変わっていない（行数も位置も動かない）。
    return false;
  }
  if (!hasRowRestriction(options.view)) {
    return false;
  }
  return outcome.row_count !== options.sheetRowsBefore;
}

// ===========================================================================
// 2. 開く流れ（純粋な非同期関数。効果はこれを呼ぶだけである）
// ===========================================================================

/**
 * セッションの状態を読み、表示するシートを開き、**内容の領域に出す状態**を決める。
 *
 * 例外を投げない（`GridClient` の口は封筒の失敗を値で返す）。宣言された列が 1 本も無いときは
 * **開く呼び出しをしない**（モジュール doc の表）。
 */
export async function loadGridScreenState(client: GridClient): Promise<GridScreenState> {
  const answer = await client.readDocumentState();
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error), canRetry: true };
  }

  const session = answer.data.status;
  if (session.state === "Unavailable") {
    return {
      status: "failed",
      message: `ドキュメントを読み込めませんでした: ${session.reason}`,
      canRetry: true,
    };
  }
  if (session.state === "Absent") {
    // 空の状態ではない（表の対象そのものが無い）。利用者には開く手立てがあるので再試行を出す。
    return {
      status: "failed",
      message: "このウィンドウにはドキュメントがありません",
      canRetry: true,
    };
  }

  const sheet = session.sheets[0];
  if (sheet === undefined) {
    return { status: "failed", message: "このドキュメントにはシートがありません", canRetry: true };
  }
  if (sheet.columns === 0) {
    // 要件 1.6。**開く呼び出しをしない**（モジュール doc「2 つの空の状態」）。
    return { status: "no-schema", sheetName: sheet.name };
  }

  const opened = await client.openSheet(sheet.id);
  if (opened.status === "error") {
    return { status: "failed", message: describeIpcError(opened.error), canRetry: true };
  }
  const summary = opened.data.sheet;
  if (summary.columns.length === 0) {
    // 防御: `GridSheetSummary` の型は列 0 本を許す（現在の Rust 側は開く前に拒む）。
    return { status: "no-schema", sheetName: sheet.name };
  }
  if (summary.row_count === 0) {
    // 要件 1.5。**列の構成を示したうえで**行が無いことを示す（表を描かないので順序も要らない）。
    return { status: "no-rows", sheetName: sheet.name, columns: summary.columns };
  }

  // 可視行の順序は `grid_set_view` が導出する（`GridSession::set_view` が `recompute_order`
  // を行う）。**これを呼ばないと窓はつねに行 0 件で返り、表は読み込み中のままになる**
  // （8.1 の起動観測で実測した）。渡すのは「絞り込み無し・並べ替え無し・展開無し」であり、
  // 操作ではない（並べ替え・絞り込みの操作は 8.8 の担当である）。
  //
  // **この呼び出しの応答も導出後の構成を運ぶ**（`GridViewResponse.columns`）が、状態へ入れる
  // 構成は `grid_open_sheet` の要約のままである — 空の指定では両者が一致する（展開が 1 つも
  // 無いので、ドメインの `derive_layout` は宣言の列をそのまま返す）ため、**同じものを 2 度
  // 受け取って片方を捨てる**経路を作らない。採用は**構成が変わりうる呼び出し**（利用者の操作）
  // にだけ要る（[`gridScreenViewSettled`]）。
  const derived = await client.setView(EMPTY_GRID_VIEW);
  if (derived.status === "error") {
    return { status: "failed", message: describeIpcError(derived.error), canRetry: true };
  }
  // 開いた直後の表示状態（7.5。幅は 1 つも設定されておらず、並びは構成そのものである）。
  const display = createDisplayState({ columnCount: summary.columns.length });
  return {
    status: "ready",
    sheet: sheet.id,
    summary,
    // **窓が覆うのは可視行である**（窓の区間は可視行の序数で表される。`RowSpan` の doc）ので、
    // 記憶へ渡す行数はシートの行数ではなく応答の可視行数である。
    visibleRows: derived.data.visible_rows,
    // **絞り込みで隠れている行の数**（要件 8.7）。空の指定では 0 である（可視の順序は文書の
    // 順序そのもの）が、**画面は数え直さない** — 置くのは応答が運んだ数である。
    hiddenRows: derived.data.hidden_rows,
    // **表を描き始める時点から現在位置が 1 つある**（要件 2.1）。先頭のセルである（開いた直後に
    // 見えているのは先頭の窓なので、追随も要らない）。
    selection: initialSelection(),
    // 開いた直後は編集していない（要件 3.1。編集は利用者の起動で始まる）。
    editing: null,
    // **シート全体の違反の総数**（要件 4.3）。順序を導出させた呼び出しがその数を運ぶ
    // （`GridSession` はこの呼び出しで索引を組み立てる — `gridClient.ts` の `setView` の doc）。
    violationTotal: derived.data.violation_total,
    // 開いた直後は出すべき違反の理由が 1 つも無い（表が現在位置を読んでから引く）。
    violation: null,
    // 開いた直後の表示の指定は空である（絞り込み無し・並べ替え無し・展開無し。要件 5.1 の
    // 展開は利用者の操作で足す）。
    view: EMPTY_GRID_VIEW,
    // **列幅と表示上の列順**（8.8。要件 8.1、8.2）。開いた直後は構成そのものであり、幅は
    // 1 つも設定されていない — 2 つの鍵もその状態から引き直す（組み直しの合図である）。
    display,
    layoutKey: layoutKeyOf(display),
    rowOrderKey: rowOrderKeyOf(EMPTY_GRID_VIEW),
    // **世代は応答が運ぶ値そのものである**（タスク 10.1）。`grid_open_sheet` の応答も世代を
    // 運ぶが（開いた直後は `Generation::FIRST`）、**その直後に呼ぶ `grid_set_view` の応答が
    // より新しい値を持つ**（`set_view` はつねに 1 つ進める）ので、表を描き始める時点の値は
    // こちらである — 画面は数え直さない。
    generation: derived.data.generation,
    // 開いた直後は詳細表示を開いていない（要件 5.5。開くのは利用者の操作である）。
    detail: null,
    // 開いた直後は削除の確認を待っていない（8.6。尋ねるのは利用者の操作の後である）。
    pendingDelete: null,
  };
}

// ===========================================================================
// 3. 移植口へ渡す仕様（**キャンバスを要さない純粋な部分**）
// ===========================================================================

/**
 * 移植口へ渡す仕様を組む。**引く口（`getCell`）・列・行数と、選択に関わる 3 つである。**
 *
 * 選択に関わる 3 つは 8.2 が結線した（8.1 は選択を使わなかった）:
 *
 *   - `selection`: マウントの時点の選択（**表を描くときは現在位置が 1 つある**。要件 2.1）
 *   - `onSelectionChange`: **実装が起こした**変化（ポインタ・行見出し・Glide の側に残した束縛）
 *     を画面へ上げる。画面はそれをそのまま自分の選択として取り込む
 *   - `onVisibleSpanChange`: 見えている区間（追随の判断と窓の先読みの材料。要件 2.4）
 *
 * 編集の起動は 8.3 が結線した（要件 3.1）。**初期値をここで引く**（`getCell`）のは、入力手段に
 * 見せるべき値が**いま描かれている値そのもの**だからである — 画面が別の経路で値を持ち寄ると、
 * 描かれている値と打ち直しの初期値が食い違いうる。
 *
 * **複製と貼り付けは 8.7 が結線した**（要件 7.1、7.2）。移植口の 2 つの口は `Promise` を返すので、
 * 判断（`./clipboard` の [`planCopy`] / [`planPaste`]）と往復（同 [`applyPaste`]）を**この組の
 * 中で繋ぐ**。表の面（`GridSurface`）が渡すのは、窓の記憶を読む 2 つの判断と、境界へ送る 1 つで
 * ある — **移植口は文字列を解釈せず、境界の形も知らない**（7.1 の契約）。
 *
 * **列幅と列の移動は 8.8 が結線した**（要件 8.1、8.2）。2 つは**表示位置**で報せられる
 * （`port.ts` の doc。Glide の添字をそのまま位置として渡す）ので、`./viewOps` の内部の翻訳を
 * 通さずに画面の遷移へ渡せる — **画面の遷移も表示位置を取る**（幅の鍵は列そのものであり、
 * 位置を列へ写すのは 7.5 の状態 1 箇所である）。
 *
 * **未結線の操作は 1 つも残っていない。**8.1 が「黙って何もしない実装にしない」ために置いた
 * `onUnavailable` の経路（と、その語の一覧 `OPERATION_NAMES`）は、8.8 が最後の 2 つを結線した
 * 時点で**落とした** — 残せば、6 つすべてが結線された後に「まだ使えない」という語だけが
 * 増えも減りもしないまま残る（到達しない分岐は検査で覆えない）。
 */
export function createGridRendererSpec(options: {
  readonly getCell: (position: CellPosition) => RenderCell;
  readonly columns: readonly RenderColumn[];
  readonly rowCount: number;
  readonly selection: RendererSelection | null;
  readonly rowMarkers: RowMarkerMode;
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  readonly onVisibleSpanChange: (span: VisibleSpan) => void;
  /** 編集の起動（要件 3.1）。**位置と、いま描かれている値**を渡す。 */
  readonly onActivateEditor: (position: CellPosition, initialText: string) => void;
  /** 選択の範囲の複製の判断（要件 7.1、7.2。`./clipboard` の [`planCopy`]）。 */
  readonly copyRange: (range: CellRange) => CopyPlan;
  /** 貼り付けの判断（要件 7.3、7.4、8.6、8.9。`./clipboard` の [`planPaste`]）。 */
  readonly pasteAt: (anchor: CellPosition, text: string) => PastePlan;
  /** 貼り付けの 1 往復（要件 7.3、1.7。`./clipboard` の [`applyPaste`]）。 */
  readonly sendPaste: (payload: PastePayload) => Promise<void>;
  /** **送らなかった理由**を画面へ上げる口（複製と貼り付けの拒否。8.6 の `onRefused` と同じ）。 */
  readonly onRefused: (message: string) => void;
  /** 列の幅が変わった（要件 8.1。**表示位置**で報せられる）。 */
  readonly onColumnResize: (displayPosition: number, width: number) => void;
  /** 列が並びの中で運ばれた（要件 8.2。**表示順の位置どうし**で報せられる）。 */
  readonly onColumnMove: (from: number, to: number) => void;
}): RendererSpec {
  const refusePromise = (message: string): Promise<never> => {
    // **告知へ上げてから拒否する。**移植口の実装は拒否を記録するだけで、クリップボードへは
    // 書かず・適用もしない（`glideAdapter.tsx` の `GlideSurface`）。
    options.onRefused(message);
    return Promise.reject(new Error(message));
  };

  return {
    columns: options.columns,
    rowCount: options.rowCount,
    selection: options.selection,
    rowMarkers: options.rowMarkers,
    getCell: options.getCell,
    onSelectionChange: options.onSelectionChange,
    onVisibleSpanChange: options.onVisibleSpanChange,
    onActivateEditor: (position) => {
      // **描かれている値が初期値である**（要件 3.1）。引く口は同期であり投げないので、
      // ここで例外が画面を巻き込むことはない（`RendererSpec.getCell` の不変条件）。
      options.onActivateEditor(position, options.getCell(position).text);
    },
    // **2 つの知らせはそのまま画面の遷移へ渡る**（要件 8.1、8.2）。移植口の実装は Glide の
    // 添字をそのまま位置として渡すので、ここで写し直す必要が無い（`port.ts` の doc。
    // 写し直すと、向きの取り違えが片方だけに起きる）。
    onColumnResize: options.onColumnResize,
    onColumnMove: options.onColumnMove,
    onCopy: (range) => {
      const plan = options.copyRange(range);
      return plan.kind === "refused"
        ? refusePromise(plan.message)
        : Promise.resolve(plan.text);
    },
    onPaste: (anchor, text) =>
      // **腕の振り分けは `./clipboard` が持つ**（8.6 の `runRowOperationPlan` と同じ形である。
      // ここで分岐を書き直すと、腕が増えた日に片方だけが追随する）。
      runPastePlan(options.pasteAt(anchor, text), {
        send: options.sendPaste,
        refuse: refusePromise,
      }),
  };
}

/**
 * メニューの活性化を複製の入口へ渡す口を組む（要件 7.8 の後者。8.7）。
 *
 * **`GridSurface` の効果の本体である**（`followSelection` と同じ理由で切り出してある — 効果は
 * 走らせないと観測できず、この module は React の部品なので `node` 環境の検査から組み立てられ
 * ない）。`handleOf` は移植口の取っ手を引く関数であり、画面は `() => handleRef.current` を渡す
 * （器を組み立て直しても新しい取っ手を指す）。
 *
 * 入口は**移植口の `copySelection` だけ**である — 範囲の決定も、テキストの作成も、クリップ
 * ボードへ渡すことも、移植口の内側で 1 つに閉じている（打鍵が着くのと同じメソッドである）。
 * ここが持つのは 2 つだけである:
 *
 * 1. **器がまだ無いとき（`null`）は何もしない。**投げない — 活性化は非同期に届くので、
 *    片付いた後の画面を叩く経路を作らない（打鍵の面も同じである。`glideAdapter.tsx` の
 *    `attachCopyKeystroke`）
 * 2. **失敗を記録する。**複製できない理由（窓が届いていないセルを含む）は、移植口の `onCopy` が
 *    **告知へ出してから**拒否する（`createGridRendererSpec` の `refusePromise`）ので、ここでは
 *    記録だけを行う
 */
export function createGridCopyEntry(handleOf: () => RendererHandle | null): CopyEntry {
  return () => {
    const handle = handleOf();
    if (handle === null) {
      return;
    }
    void handle.copySelection().catch((error: unknown) => {
      console.error("選択の範囲を複製できなかった", error);
    });
  };
}

/**
 * 選択を移植口へ下ろし、必要なら表示範囲を追随させる（要件 2.4）。
 *
 * **`GridSurface` の効果の本体である。**効果は走らせないと観測できない（`node` の環境には
 * DOM が無く、`renderToStaticMarkup` は効果を実行しない）ので、検査できる形に切り出してある
 * — 要件 4.4 の「違反の位置へ現在位置を移す」が `scrollTo` へ**何を渡すか**は、この関数を
 * 偽の取っ手で呼べば読める（`GridScreen.test.ts`「次の違反への移動は、表示範囲の外の違反へ
 * 現在位置を移す」）。
 *
 * 下ろす値と追随の判断は**同じ 1 つの選択**を見る（写しを作らない）。
 */
export function followSelection(
  handle: Pick<RendererHandle, "setSelection" | "scrollTo">,
  visible: VisibleSpan | null,
  selection: RendererSelection,
): void {
  handle.setSelection(selection);
  const target = followTarget(visible, selection);
  if (target !== null) {
    handle.scrollTo(target);
  }
}

// ===========================================================================
// 4. 表（窓の記憶と移植口を組み立てる場所）
// ===========================================================================

/**
 * 行見出し列の出し方。**行の全体をポインタで選べるようにする**（要件 2.3 の 2 つ目）。
 *
 * Glide は行見出しの分の添字を内部で補正する（`getCellContent` / 列幅 / 選択の正規化 /
 * 見えている区間 / `scrollTo` のいずれも）ので、画面はこの値を渡すだけでよい（design.md
 * 「7.2 が決めたこと」の行見出し列の行）。数字は Glide が 1 起点で描く。
 */
const ROW_MARKERS: RowMarkerMode = "clickable-number";

/** 表を入れる枠。**ここが移植口の器である**（確定した寸法が要る。`createGlideAdapter` の doc）。 */
const TABLE_STYLE = {
  flex: "1 1 auto",
  minHeight: 0,
  width: "100%",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  borderRadius: "0.25rem",
  overflow: "hidden",
} as const;

/** 表の面（数え上げの行と、移植口の器）。**縦に伸びる**（器は確定した寸法を要する）。 */
const SURFACE_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  flex: "1 1 auto",
  minHeight: 0,
} as const;

/** 選択の数え上げの行（要件 2.5）。**表の上に出す**。 */
const SELECTION_STYLE = {
  margin: 0,
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenMuted})`,
} as const;

/** 行の操作の行（8.6。要件 6.1、6.2、6.3、6.5）。**数え上げの行の隣に出す**。 */
const ROW_OPS_STYLE = {
  display: "flex",
  alignItems: "center",
  flexWrap: "wrap",
  gap: "0.5rem",
} as const;

/**
 * 削除の確認（要件 6.5）。**その場に出す**（器の外の対話を開かない）。
 *
 * 枠線は器の配色の 1 本だけを参照する（画面は自前の配色を持たない — module doc の契約 4）。
 */
const CONFIRM_STYLE = {
  display: "inline-flex",
  alignItems: "center",
  gap: "0.5rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  borderRadius: "0.25rem",
  padding: "0.25rem 0.5rem",
} as const;

/**
 * 窓の記憶がまだ無いときに返すセルの札（8.5）。**「値なし」ではなく「未取得」である**
 * （`RenderCell.loading` の doc。空文字は値なしと区別がつかない）。
 */
const UNKNOWN_CELL: RenderCell = { text: "", variant: "Any", violated: false, loading: true };

/** 表を組み立てる指定。 */
interface GridSurfaceProps {
  /** 開いたシートの識別子（窓の要求が名乗る。`grid_open_sheet` に渡した文字列と同一である）。 */
  readonly sheet: string;
  /** 開いた応答の要約（列の構成と行数）。 */
  readonly summary: GridSheetSummary;
  /**
   * **描かれる列の並び**（表示順。要件 8.2）。
   *
   * 画面が 1 度だけ組んで渡す（`./viewOps` の [`drawnColumns`]）— 表・列ごとの操作の行・
   * 窓の記憶の写像が**同じ 1 つの並び**を見るようにするためである（写像が 2 つあると、
   * 描かれている値と編集の宛先が別の列を指す）。
   */
  readonly columns: readonly ColumnDescriptor[];
  /**
   * **いまの表示状態**（8.8。要件 8.1、8.2）。列幅と表示上の列順である。
   *
   * ここへ渡るのは**読み取りのためだけ**である（`renderColumns` は描画のための組み立てであり、
   * 状態を変えない）。表がこの値から組むのは、移植口へ渡す列（見出しと幅）だけである — 表示の
   * 位置の空間（[`columns`]）は画面が 1 度だけ組んで渡す。
   */
  readonly display: DisplayStateStore;
  /**
   * 描かれる列の内容の同一性（[`layoutKeyOf`]）。**面を組み直す合図である。**
   *
   * 幅と並びは移植口へ次の `mount` の仕様として届く（押し込む口が無い）ので、この鍵が
   * 変わったときに器・窓の記憶・移植口を**揃って組み直す**。
   */
  readonly layoutKey: string;
  /**
   * 行の集合の同一性（[`rowOrderKeyOf`]）。**面を組み直す合図である。**
   *
   * 並べ替えと絞り込みは「何番目の行がどの行か」を変えるので、**取得済みの窓は別の行を指す**
   * （7.3 の `clear` の doc）。世代（`generation`）では代用できない — 世代は値だけの編集でも
   * 進む（応答が運ぶ値であり、画面が規則を持つのではない。タスク 10.1）が、そのとき行の並びは
   * 動かない（要件 8.8）。
   */
  readonly rowOrderKey: string;
  /** 可視行の数（窓が覆う行数）。 */
  readonly visibleRows: number;
  /**
   * 絞り込みで隠れている行の数（要件 8.7）。**面を組み直す合図でもある** — 行の増減のあとに
   * 隠れている数だけが変わった場合（絞り込みの条件に合わない行を足した場合）は、可視行数も
   * 並びも動かないので、この数が唯一の手掛かりである。
   */
  readonly hiddenRows: number;
  /**
   * いまの世代（**10 進の文字列**。源は境界の応答ただ 1 つである。タスク 10.1）。
   *
   * **依存には入れない**（入れると、編集のたびに器を組み直して走査の位置を失う）。組み立ての
   * 時点の値として読み、以後の変化は専用の効果が記憶へ下ろす。
   */
  readonly generation: string;
  /** 選択（現在位置と矩形）。**画面の状態が持つ唯一の値である**（写しをここに作らない）。 */
  readonly selection: RendererSelection;
  /** 編集中のセル（要件 3.1）。`null` なら編集していない。 */
  readonly editing: CellEdit | null;
  /** 開いている詳細表示（8.5。要件 5.5）。`null` なら開いていない。 */
  readonly detail: CellDetail | null;
  /** 削除の確認を待っている対象（8.6。要件 6.5）。`null` なら尋ねていない。 */
  readonly pendingDelete: DeleteConfirmation | null;
  /**
   * 境界の口。**編集の 1 往復（`./cellEdit`）が使う** — カードの記憶（下）と組にするのは、
   * 確定が宛先（窓の行の識別子）と、適用のあとの作り直し（影響を受けた行の窓を捨てる）の
   * 両方を要するためである。
   */
  readonly client: GridClient;
  /** 選択が変わった（打鍵・ポインタのどちらでも）ことを画面へ上げる口。 */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  /** 編集が起動された（要件 3.1。位置と、いま描かれている値）ことを画面へ上げる口。 */
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  /** 確定の 1 往復の結果を画面へ上げる口（要件 3.3、3.4、3.5、3.6）。 */
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
  /**
   * 詳細表示の中の編集の結果を画面へ上げる口（要件 5.7）。**セルの編集と同じ遷移を使う**
   * （`./cellEdit` の 1 往復は同一であり、違うのは反映の遷移だけである）。
   */
  readonly onDetailEditSettled: (settlement: CellEditSettlement) => void;
  /** 詳細表示を閉じる（8.5。**値も文書も動かない**）。 */
  readonly onDetailClosed: () => void;
  /**
   * いまの位置の違反の読み取りを画面へ上げる口（要件 4.2、4.6）。
   *
   * 窓の印と行の識別子を読むには記憶が要る（[`WindowCache`]）ので、**引くのはここ**であり、
   * 画面の状態へ入れるのは遷移（[`gridScreenViolationReason`]）である。
   */
  readonly onViolationRead: (reading: ViolationReading) => void;
  /** 列の幅が変わった（要件 8.1。**表示位置**で報せられる）。 */
  readonly onColumnResize: (displayPosition: number, width: number) => void;
  /** 列が並びの中で運ばれた（要件 8.2。**表示順の位置どうし**で報せられる）。 */
  readonly onColumnMove: (from: number, to: number) => void;
  /**
   * 行の操作（8.6。要件 6.1、6.2、6.3）を画面へ上げる口。
   *
   * **表が行うのは判断と往復だけであり、状態を持つのは画面である**（`./cellEdit` が確定の
   * 結果を `onEditSettled` で上げるのと同じ形である）。
   */
  readonly onRowOperationSettled: (settlement: RowOperationSettlement) => void;
  /**
   * 貼り付けの 1 往復（8.7。要件 7.3、7.4、1.7）の結果を画面へ上げる口。
   *
   * **行数を置き換え、現在位置を寄せるのは画面の遷移である**（[`gridScreenPasteSettled`]）。
   * 貼り付けは行を補充しうるので、反映の形は行の操作と同じ 1 つを通る。
   */
  readonly onPasteSettled: (settlement: PasteSettlement) => void;
  /**
   * 取り消し・やり直しの 1 往復（8.9。要件 9.2、9.3、9.8）の結果を画面へ上げる口。
   *
   * **画面の中の 2 つの操作とメニューの活性化が、この 1 つの口へ着く**（`./history` の
   * `HistoryEntry`）。反映（[`gridScreenHistorySettled`]）は 8.6 の行の操作と同じ形を通る。
   */
  readonly onHistorySettled: (settlement: HistorySettlement) => void;
  /** 削除の確認を求める（要件 6.5。**送っていない**。数を示して尋ねるだけである）。 */
  readonly onDeleteRequested: (confirmation: DeleteConfirmation) => void;
  /** 確認への取り消し（**送らない**）。 */
  readonly onDeleteCancelled: () => void;
  /** 返さずに理由を告げる（複製と貼り付けの拒否。8.6 の行の操作はここへ来ない）。 */
  readonly onRefused: (message: string) => void;
}

/**
 * 表の面が使う窓の記憶を組む（**効果の外に出した純粋な部分である**）。
 *
 * 効果（`useEffect`）の中で組み立てると、検査は「画面がどの列の写像で記憶を組んだか」を観測
 * できない（`node` の環境には DOM が無く、効果は走らない）。8.1 が移植口の仕様
 * （[`createGridRendererSpec`]）と追随（[`followSelection`]）を同じ理由で外へ出している。
 *
 * **列の写像をここで 1 度だけ引く**（`./columnSpace`）— 表示の位置から文書の列への写像は、
 * 窓の読み（`getCell`）と編集の宛先（`./cellEdit` が [`WindowCache.documentColumn`] を通して
 * 読む）の**両方**がこれを使う。8.5 の申し送り（8.3 のレビューが実測）はこれを求めていた。
 *
 * **写像を組む並びは「描かれる列の並び」である**（8.8）。表示上の列順を変えると表示の位置の
 * 意味が変わるので、構成の順で組むと**描かれている値と編集の宛先が別の列を指す**。
 */
export function createGridSurfaceCache(options: {
  readonly sheet: string;
  /**
   * **描かれる列の並び**（表示順。`./viewOps` の [`drawnColumns`] が組む）。
   *
   * 構成（`summary.columns`）ではなくこれを渡すのは、**表示上の列順を変えると写像そのものが
   * 変わる**ためである（要件 8.2）— 構成の順で組むと、描かれている値と編集の宛先が別の列を
   * 指す（`./viewOps` の module doc「表示の並びは 1 つである」）。
   */
  readonly columns: readonly ColumnDescriptor[];
  readonly visibleRows: number;
  /** いまの世代（**10 進の文字列**。源は境界の応答ただ 1 つである。タスク 10.1）。 */
  readonly generation: string;
  readonly client: GridClient;
  /** 窓が記憶に入ったときの通知（区間は実際に記憶した範囲）。 */
  readonly onArrival?: (span: RowSpan) => void;
}): WindowCache {
  return createWindowCache({
    sheet: options.sheet,
    // **描かれる並びが、そのまま表示の位置の空間である**（入れ子の展開と表示上の列順を含む）。
    columns: createColumnSpace(options.columns),
    // **窓が覆うのは可視行である**（絞り込みが効いていればシートの行数と食い違う）。
    rowCount: options.visibleRows,
    generation: options.generation,
    transport: (argument) => options.client.readWindow(argument),
    ...(options.onArrival === undefined ? {} : { onArrival: options.onArrival }),
  });
}

/**
 * 表そのもの。**開いたシート 1 つぶんの窓の記憶（7.3）と移植口（7.1 / 7.2）を組み立て、
 * 現在位置と選択（8.2）を結線する。**
 *
 * 組み立てはマウントの効果 1 つで行い、後始末で移植口を片付けて記憶を手放す（`dispose`。以後の
 * 応答は捨てられる）。**列の並びは表示状態（7.5）から組む**ので、列の構成が変わったとき
 * （入れ子の展開・折りたたみ。要件 5.1、5.2）と 8.8 が列幅・列順を変えたときは、**新しい仕様で
 * マウントし直す**（`RendererHandle` に幅や順を押し込む口が無い。移植口に列を差し替える口が
 * 無いことは 7.2 の申し送りである）。効果の依存に `summary` があることが、その 1 つの経路で
 * ある — **導出後の構成を採用した状態が渡ってくれば、器・窓の記憶・移植口が揃って組み直る**
 * （列の写像も `createGridSurfaceCache` が新しい構成から組み直すので、写像は 1 つのままである）。
 *
 * 効果は 4 つである: ① 器の組み立て（依存はシートと列の構成と可視行数だけ — **選択を依存に
 * 入れない**。入れると打鍵のたびに器を組み直し、React の根と Glide の部品を作り直して走査の
 * 位置を失う）、② 選択を移植口へ下ろし、必要なら追随させる（依存は選択だけ）、③ **世代を
 * 記憶へ下ろす**（8.5。**組み直さない** — 適用は世代だけを進めて列の構成を変えないので、
 * 組み直すと走査の位置まで失われる）、④ 現在位置の違反の理由を引く（要件 4.2）。
 *
 * **詳細表示（8.5）もここが描く。**値を読むには窓の記憶が要り、記憶を持つのは表だからである
 * （8.4 が違反の理由を読むのと同じ理由）。開いている位置そのものは画面の状態（`detail`）が持つ
 * — そうしないと、開いていることを検査から観測できない。
 */
function GridSurface({
  sheet,
  summary,
  columns,
  display,
  layoutKey,
  rowOrderKey,
  visibleRows,
  hiddenRows,
  generation,
  selection,
  editing,
  detail,
  pendingDelete,
  client,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onDetailEditSettled,
  onDetailClosed,
  onViolationRead,
  onColumnResize,
  onColumnMove,
  onRowOperationSettled,
  onPasteSettled,
  onHistorySettled,
  onDeleteRequested,
  onDeleteCancelled,
  onRefused,
}: GridSurfaceProps): ReactElement {
  const containerRef = useRef<HTMLDivElement | null>(null);
  // 移植口の取っ手。**窓の到着（非同期）と選択の効果が使う**ので、効果の外に置く。
  const handleRef = useRef<RendererHandle | null>(null);
  /**
   * 窓が届いた回数（**React の描き直しを起こすためだけの数である**）。
   *
   * 窓の到着は移植口の描き直し（`RendererHandle.invalidate`）を起こすが、**React の描き直しは
   * 起こさない**。詳細表示の中身（要約と、違反している内側の位置）は窓が運ぶので、届いたことを
   * 画面の側へも伝えないと、**未取得のままの表示が残る**（次の無関係な描き直しまで）。
   *
   * 到着は走査で 256 行ごとにしか起きない（`WINDOW_ROWS`）ので、開いていないときにも数える
   * 費用は無視できる（描き直すのは十数の要素である）。
   */
  const [arrivals, setArrivals] = useState(0);
  /**
   * 窓の記憶。**描画（編集の面）からも使う**ので ref に置く — 組み立てはマウントの効果の中で
   * 行われるが、確定は利用者の操作（描画の外の出来事）から来る。
   */
  const cacheRef = useRef<WindowCache | null>(null);
  /**
   * いま見えている区間（要件 2.4 の追随の判断と、7.3 の先読みの材料）。
   *
   * **状態にしない**（描き直しが要らない）。走査のたびに再描画すると、窓の到着のたびに表を
   * 組み直すことになる。値は移植口の知らせ（`onVisibleSpanChange`）が書き換える。
   */
  const visibleRef = useRef<VisibleSpan | null>(null);
  /**
   * **いま 1 画面に見えている行数**（8.6。要件 6.5 の閾値）。移植口の知らせだけが書き、
   * `null` は「まだ知らない」である。
   *
   * [`visibleRef`] と別に持つのは、**開いた直後の見当が「1 画面」ではない**ためである —
   * あれは窓の先読みの幅（`WINDOW_ROWS` ＝ 256 行）であり、画面の高さではない。代用すると、
   * 1 画面に収まらない削除（要件 6.5）が確認を求めなくなる。
   */
  const viewportRowsRef = useRef<number | null>(null);
  /**
   * いまの選択（**移植口の知らせの中から最新の値を読むため**の写し）。
   *
   * 窓の到着と、確定のあとの引き直しは効果の外（移植口の callback と、非同期の続き）から
   * 起こるので、その時点の選択を知る必要がある。効果（選択が変わったときに走るもの）が
   * 書き換える。
   */
  const selectionRef = useRef<RendererSelection>(selection);
  /**
   * 違反の読み取りの世代。**遅れて届いた答えを捨てる**ために使う（現在位置がもう違うのに、
   * 前の位置の理由を出す経路を閉じる）。
   */
  const violationTokenRef = useRef(0);
  /**
   * 窓が届いたら**もう一度**違反を引き直すか。
   *
   * 未取得のセルは印が読めない（`violationMark` が `unknown` を返す）ので、そのときは
   * 引き直しを予約する。適用のあとも予約する — 窓の印は取り直しの途中であり、**編集で
   * 新しく生じた違反**（印がまだ無い）を取りこぼさないためである（要件 4.6）。
   */
  const violationWaitingRef = useRef(false);
  /**
   * いまの構成の写像（**2 つの空間の唯一の口**。要件 4.2、4.4）。逆向きは境界の答える列を
   * 表示の位置へ落とし、順方向は**指したセルの文書の列**を要求へ載せる（タスク 10.6）。
   *
   * 窓の記憶が組む写像（[`createGridSurfaceCache`]）と**同じ並びから引く**（どちらも
   * [`columns`] ＝ 表示順の構成である）。したがって写像は 1 つのままである — 表が読む
   * セルと、違反の提示が名乗る位置は、同じ並びの同じ位置を指す。
   */
  const space = useMemo(() => createColumnSpace(columns), [columns]);

  /**
   * いまの位置の違反の理由を引き直す（要件 4.2）。
   *
   * **窓の印を門番にする**（理由を引くのは印のあるセルに限る — 矢印で動くたびに境界へ
   * 問い合わせる経路を作らない）。印が読めない（未取得）ときは予約だけして引き直し、窓の
   * 到着（`onArrival`）でもう一度試す。
   */
  const refreshViolation = (position: CellPosition, awaitingRefetch = false): void => {
    const cache = cacheRef.current;
    if (cache === null) {
      return;
    }
    const token = (violationTokenRef.current += 1);
    const mark = violationMark(cache.getCell(position));
    violationWaitingRef.current = mark === "unknown" || awaitingRefetch;
    if (mark === "unknown") {
      // 印が読めない。**推測しない**（窓が届いたら読み直す）。
      return;
    }
    if (mark === "clear") {
      // 印が無い。引くものが無い（取り下げる）。ここは同期的に答える — 動いた直後に古い理由が
      // 残る瞬間を作らない。
      onViolationRead({ kind: "cleared" });
      return;
    }
    void reasonInRow({
      client,
      current: position,
      rowId: cache.rowId(position),
      space,
    }).then((reading) => {
      if (token !== violationTokenRef.current) {
        return;
      }
      onViolationRead(reading);
    });
  };

  // 表の大きさ（現在位置を寄せる先。要件 2.2 の端の扱いと、行・列の全体の選択に要る）。
  const bounds = { rowCount: visibleRows, columnCount: columns.length };

  /**
   * 表の器が打鍵を受ける口。**方向の指示だけを引き受け、残りは流す**（`./selection` の
   * module doc の表）。ここで例外を投げない — `ScreenBoundary` は**イベントハンドラの例外を
   * 捕まえない**（`src/shell/ScreenBoundary.tsx`）ので、投げれば画面が壊れる。
   */
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>): void => {
    const next = selectionForKey(event, selection, bounds);
    if (next === null) {
      return;
    }
    // 引き受けた打鍵はブラウザの走査を止める（矢印は器をスクロールさせてしまう）。
    event.preventDefault();
    onSelectionChange(next);
  };

  // 表そのものの組み立て。**選択が変わっても組み直さない** — 器を作り直すと React の根と
  // Glide の部品が作り直され、走査の位置（表示範囲）も失われる。仕様へ渡す選択はマウントの
  // 時点の値であり（`RendererSpec.selection` の doc）、以後の更新は下の効果が
  // `handle.setSelection` で下ろす。**したがってこの効果の依存に選択を入れない**
  // （入れると打鍵のたびに組み直すことになる）。効果の閉包は依存が変わった回の描画の値なので、
  // 器を組み直すとき（シートが変わったとき）はそのときの選択が初期値になる。
  useEffect(() => {
    const container = containerRef.current;
    if (container === null) {
      return undefined;
    }

    // 開いた直後に見えている区間の見当。**実装が知らせてくるまでの値である**（Glide は
    // マウントの直後に本当の区間を知らせる）。
    const openingSpan: VisibleSpan = {
      rows: { start: 0, count: Math.min(visibleRows, WINDOW_ROWS) },
      columns: { start: 0, count: columns.length },
    };
    visibleRef.current = openingSpan;

    const cache = createGridSurfaceCache({
      sheet,
      // **描かれる列の並び**である（表示上の列順を含む。要件 8.2）。
      columns,
      // **窓が覆うのは可視行である**（絞り込みが効いていればシートの行数と食い違う）。
      visibleRows,
      // 組み立ての時点の世代。以後の変化は下の効果が記憶へ下ろす（**組み直さない**）。
      generation,
      client,
      // 窓が届いたら、その区間を描き直させる（移植口は知らせが無ければ描き直さない）。
      onArrival: (span) => {
        handleRef.current?.invalidate(span);
        // **画面の側にも知らせる**（詳細表示の中身は窓が運ぶ。上の `arrivals` の doc）。
        setArrivals((count) => count + 1);
        // 印を読み直す約束をしていたなら、いまの位置についてもう一度引く（要件 4.2、4.6）。
        if (violationWaitingRef.current) {
          refreshViolation(selectionRef.current.current);
        }
      },
    });

    // 貼り付け・複製の面（要件 7.1、7.2、7.3、7.4）。**材料と行き先を渡すだけで、判断も往復も
    // 本 module には書かない** — `GridSurface` は React の部品であり、`node` 環境の検査から
    // 組み立てられないので、ここに判断を置くと覆えない穴になる（`createClipboardSurface` の doc）。
    const clipboard = createClipboardSurface({
      client,
      cache: () => cacheRef.current,
      visibleRows,
      onSettled: onPasteSettled,
      // 適用のあとは違反を引き直す（要件 4.6。セルの編集・行の操作と同じ規律である）。
      onApplied: () => {
        refreshViolation(selectionRef.current.current, true);
      },
    });

    const handle = GRID_RENDERER_PORT.mount(
      container,
      createGridRendererSpec({
        // **表示順の見出しを渡す**（幅は表示状態が持ち、並びも表示状態が決める。要件 8.1、8.2）。
        columns: display.renderColumns(summary.columns.map((column) => column.name)),
        rowCount: visibleRows,
        // **マウントの時点で現在位置が 1 つある**（要件 2.1）。
        selection,
        rowMarkers: ROW_MARKERS,
        getCell: (position) => cache.getCell(position),
        // 編集の起動（要件 3.1）。**現在位置と、いま描かれている値**を画面へ上げる。
        onActivateEditor: onEditStarted,
        onSelectionChange,
        // 見えている区間の知らせ（8.2 が移植口へ足した口である）。**追随の判断の材料**であり、
        // 同時に窓の先読みの材料でもある（この行が 8.1 の申し送りの答えである）。
        onVisibleSpanChange: (span) => {
          visibleRef.current = span;
          // **1 画面に見えている行数**（8.6 の閾値。要件 6.5）。移植口が報せた区間だけが源で
          // ある — 開いた直後の見当（上の `openingSpan`）は先読みの幅であり、画面の高さではない。
          viewportRowsRef.current = span.rows.count;
          cache.setVisibleSpan(span.rows);
        },
        // 列幅と列の移動（8.8。要件 8.1、8.2）。**知らせは表示位置で来る**ので、そのまま
        // 画面の遷移へ渡す（写し直さない）。
        onColumnResize,
        onColumnMove,
        // 複製と貼り付け（8.7。要件 7.1、7.2、7.3、7.4）。**3 つの口は `./clipboard` の
        // 面が組む**（材料と行き先を渡すだけであり、判断も往復も本 module には書かない —
        // 画面の関数本体に置くと `node` 環境の検査から組み立てられない。`createClipboardSurface`）。
        copyRange: clipboard.copyRange,
        pasteAt: clipboard.pasteAt,
        sendPaste: clipboard.sendPaste,
        onRefused,
      }),
    );
    handleRef.current = handle;
    cacheRef.current = cache;
    // 開いた直後の先読み（要件 1.4）。**上の見当を渡す** — 本物の区間は実装が知らせてくる。
    cache.setVisibleSpan(openingSpan.rows);

    return () => {
      handle.destroy();
      cache.dispose();
      handleRef.current = null;
      cacheRef.current = null;
    };
    // **組み直しの合図は 3 つである**（列の構成は `summary` の同一性で観測する — 同じ並びなら
    // 8.5 が据え置く）。幅と並びは `layoutKey`、行の集合（並べ替え・絞り込みと行数）は
    // `rowOrderKey` / `visibleRows` / `hiddenRows` である。
  }, [sheet, summary, layoutKey, rowOrderKey, visibleRows, hiddenRows]);

  /**
   * **メニューの活性化による複製を購読する**（要件 7.8 の後者。8.7）。
   *
   * 器（`src-tauri`）が `編集 > 複製` の選択を、活性化の対象ウィンドウへイベントとして送る。
   * 入口は**打鍵と同じ 1 つ**（`RendererHandle.copySelection`）であり、範囲の決め方もテキストの
   * 作り方も打鍵と共通である（`./clipboardRequests` の doc）。
   *
   * **この効果を器の組み立ての効果に混ぜない。** 組み立ては列の構成が変わるたびに走り直すが、
   * 購読は 1 回で足りる — 入口は `handleRef` を通して引くので、器を組み立て直しても新しい取っ手を
   * 指す（`selectionRef` と同じ規律である）。購読の解除は効果の後始末が行う。
   *
   * 複製できないとき（窓が届いていないセルを含む）は、打鍵と同じく `RendererSpec.onCopy` が
   * 理由を告知へ出してから拒否する。ここは**記録だけ**する。
   */
  useEffect(() => installGridCopyRequests(createGridCopyEntry(() => handleRef.current)), []);

  /**
   * **世代を記憶へ下ろす**（8.5）。**組み直さない** — 適用は世代だけを進め、列の構成を変えない
   * （構成が変わったときは、`summary` を依存に持つ組み立ての効果が別に走って器ごと組み直る）。
   * ここで組み直すと、**列の構成が変わらない出来事**（編集の適用）でも移植口を作り直すことになり、
   * 走査の位置（表示範囲）と Glide の部品まで失われる。
   * 下ろすだけで足りるのは、進んだ世代が変えるのが**窓の要求が名乗る数**だけだからである
   * （記憶している窓の中身は、行の並びを変えない指定では正しいままである）。
   *
   * 下ろさないと、以後の要求は古い世代を名乗り、Rust 側が `WindowCodec::is_stale` で**空の窓を
   * 返す** — 取り直した窓は永久に読み込み中のままになる（`store` は空の窓を記憶に入れない）。
   */
  useEffect(() => {
    cacheRef.current?.setGeneration(generation);
  }, [generation]);

  /**
   * 選択を移植口へ下ろし、必要なら表示範囲を追随させる（要件 2.2、2.4）。
   *
   * **下ろす値と数え上げの値は同じ 1 つである**（`selection`）— 画面に出ている数と、描かれて
   * いる選択がずれる余地を作らない（design.md「8.2 が広げた面」の判断）。本体は
   * [`followSelection`] に切り出してある（効果を走らせずに検査できるようにするためである）。
   */
  useEffect(() => {
    // 移植口の知らせと確定の続きが最新の値を読めるようにする（この効果は選択が変わるたびに
    // 走るので、つねに最新である）。
    selectionRef.current = selection;
    const handle = handleRef.current;
    if (handle === null) {
      return;
    }
    followSelection(handle, visibleRef.current, selection);
  }, [selection]);

  /**
   * 現在位置の違反の理由を引く（要件 4.2）。**現在位置が変わるたびに走る。**
   *
   * 窓の印を門番にするので、印の無いセルでは境界へ問い合わせない（移動のたびに往復する経路を
   * 作らない）。未取得のセルでは窓の到着で引き直す（`refreshViolation` の doc）。
   */
  useEffect(() => {
    refreshViolation(selection.current);
  }, [selection.current.row, selection.current.column]);

  const counts = selectionCounts(selection);

  /**
   * 詳細表示に渡すもの（8.5）。**源ごとに分けて組む** — 列の記述は構成から、要約と違反の内側の
   * 位置は窓の記憶から、内側の宣言は構成からである。
   *
   * `cacheRef.current` は**描画の中では組み立ての効果のあとの値**である（表を描いている状態で
   * なければ詳細表示は開けない）。`getCell` は未取得なら要求も始めるので、開いた時点で窓が
   * 無い行でも、到着（`arrivals`）で中身が埋まる。
   */
  const detailColumn = detail === null ? null : (columns[detail.position.column] ?? null);
  const detailCell =
    detail === null ? UNKNOWN_CELL : (cacheRef.current?.getCell(detail.position) ?? UNKNOWN_CELL);
  const detailMarks = detail === null ? null : (cacheRef.current?.nestedMarks(detail.position) ?? null);

  /**
   * 入力手段の 2 つの口を、境界への 1 往復へ渡す（要件 3.3、3.6、5.7）。
   *
   * **画面の状態を変えるのは結果を受け取った側である**（`onEditSettled`）。ここが担うのは
   * 往復そのものであり、窓の記憶を要する（宛先の行の識別子と列、適用のあとの作り直し）。
   * 例外を投げない — `settleCellEdit` が投げないので、この非同期の経路も投げない。
   *
   * **運び手（`carrier`）は呼び出し側（面）が登録簿から引いて渡す**（要件 10.3）。表はそれを
   * そのまま `./cellEdit` へ渡すだけである — ここで列の型を見て経路を選ぶと、要件 10.3 と
   * 衝突する（8.3 の申し送り 4）。
   */
  const settle = (
    position: CellPosition,
    intent: CellEditIntent,
    carrier: EditCarrier,
    /** 結果の反映先（**セルの編集と詳細表示の編集で違う遷移である**）。 */
    onSettled: (settlement: CellEditSettlement) => void,
  ): void => {
    const cache = cacheRef.current;
    if (cache === null) {
      // 器がまだ無い（描かれていない）。編集も開いていないので、ここへは来ない。
      return;
    }
    void settleCellEdit({ client, cache, position, intent, carrier }).then((settlement) => {
      onSettled(settlement);
      if (settlement.status !== "applied") {
        return;
      }
      // **適用のあとは違反を引き直す**（要件 4.6）。解消されたなら理由は出ず（境界の索引は
      // 既に新しい）、残っているなら新しい理由が出る。窓の印は取り直しの途中で古いので、
      // **到着でもう一度**引く（新しく生じた違反を取りこぼさない）。
      refreshViolation(selectionRef.current.current, true);
    });
  };

  /**
   * 詳細表示の中の編集の取消（要件 5.7）。**セルの編集と同じ関数である** — 違うのは、結果を
   * 反映する遷移が詳細表示の面を初期状態へ戻すことだけである。
   *
   * 取消の腕でも運び手を渡すが、**取消はそれを読まない**（`./cellEdit` は運び手を見る前に取消を
   * 返す）。`settleCellEdit` の形を 2 つに割らないために、同じ 1 つの口から引いた値をそのまま
   * 渡す。
   */
  const settleDetailCancel = (position: CellPosition): void => {
    settle(
      position,
      { kind: "cancel" },
      columnEditor(columns[position.column] ?? null).carrier,
      onDetailEditSettled,
    );
  };

  /**
   * 行の操作の 1 往復（8.6。要件 6.1、6.2、6.3）。**宛先は境界が解く** — 画面は可視の序数
   * （または末尾）をそのまま送り、**識別子を引かない**（10.4 が `RowTarget` / `RowAnchor` を
   * 足して、写像の所有者をドメインの `RowOrder` 1 つにした）ので、往復はここから起動する
   * （セルの編集と同じ配置である）。
   *
   * 器がまだ無いときは往復を起こさない。**投げない**（`applyRowOperation` が投げない）。
   */
  const runRowOperation = (intent: RowSendIntent): void => {
    const cache = cacheRef.current;
    if (cache === null) {
      return;
    }
    void applyRowOperation({ client, cache, intent }).then((settlement) => {
      onRowOperationSettled(settlement);
      if (settlement.status !== "applied") {
        return;
      }
      // **適用のあとは違反を引き直す**（要件 4.6。セルの編集と同じ規律である）。消えた行の
      // 違反は消え、残っている行の理由は新しく出る。**窓は捨てられている**（行数が変わった）ので、
      // いまの位置が未取得なら到着でもう一度引く（`refreshViolation` が予約する）。行が消えて
      // 位置が新しい範囲へ寄った場合は、選択の効果が新しい位置で引き直す。
      refreshViolation(selectionRef.current.current, true);
    });
  };

  /**
   * 行の操作の入口（8.6。要件 6.1、6.2、6.3、6.5）。**判断は `./rowOps` が行う** — 表が担うのは
   * 材料（**いまの表示の指定・行数・1 画面に見えている行数**）を揃えることと、答えを 3 つの行き先へ
   * 渡すことだけである（閾値も座標空間の判断もここには書かない。**送れないという答えは無い** —
   * 10.4 が可視の序数をそのまま送れる形にした）。
   *
   * **取り消しもこの入口を通る**（確認の面の 2 つの口が 1 つの経路で終わる）。取り消しの腕は
   * 送る腕へ載らないので、境界へは 1 つも行かない（`./rowOps` の
   * [`runRowOperationPlan`]）。
   */
  const requestRowOperation = (target: RowOperationTarget): void => {
    runRowOperationPlan(
      planRowOperation(target, {
        visibleRows,
        // **移植口が報せた区間だけが源である**（開いた直後の見当は先読みの幅である）。
        viewportRows: viewportRowsRef.current,
      }),
      {
        // **送る対象は可視の序数である**（識別子を引き集める口も、文書の位置へ写す口も
        // 画面には無い — 解くのはドメインである。要件 8.6、tasks.md 10.4）。
        send: runRowOperation,
        confirm: onDeleteRequested,
        cancel: onDeleteCancelled,
      },
    );
  };

  /** 選択の行を対象にする操作（**表の外の行は対象にしない**。対象が無ければ何もしない）。 */
  const requestRows = (kind: "delete" | "duplicate"): void => {
    const targets = rowTargets(selection, visibleRows);
    if (targets === null) {
      return;
    }
    requestRowOperation(
      kind === "delete" ? { kind: "delete", targets } : { kind: "duplicate", targets },
    );
  };

  /**
   * 履歴（取り消し・やり直し）の 1 往復（8.9。要件 9.2、9.3、9.8）。
   *
   * **画面の中の 2 つの操作と、メニューの活性化が、この 1 つの関数へ来る**（`./history` の
   * `HistoryEntry` がその口である）。判断（捨てる前の移動先の解決、行数の作り直し）は
   * `./history` が持ち、ここが担うのは往復の起動と、結果の行き先（画面の遷移と違反の引き直し）
   * だけである（8.6 / 8.7 と同じ分担である）。
   *
   * **器がまだ無いときは送らない。**履歴は文書のものであり表のものではないが、応答を受けた
   * あとに**窓の記憶を作り直す**（`clear`）ので、その先が無ければ往復を起こす意味が無い
   * （`runRowOperation` と同じ判断である。**投げない** — `applyHistory` が投げない）。
   */
  const runHistory = (direction: GridHistoryDirection): void => {
    const cache = cacheRef.current;
    if (cache === null) {
      return;
    }
    void applyHistory({ client, cache, direction }).then((settlement) => {
      onHistorySettled(settlement);
      if (settlement.status !== "applied") {
        // **進める履歴が無い**（要件 9.2、9.3 の正常な結果）か、経路が失敗したかである。
        // どちらも文書も表示も動いていないので、違反を引き直す理由が無い。
        return;
      }
      // **適用のあとは違反を引き直す**（要件 4.6。セルの編集・行の操作・貼り付けと同じ規律で
      // ある）。取り消しは違反を消しも生みもする（保持された値が戻るためである）。
      refreshViolation(selectionRef.current.current, true);
    });
  };

  /**
   * メニューの活性化（`編集 > 元に戻す` / `編集 > やり直し`）を、**画面の中の操作と同じ入口**へ
   * 渡す（要件 9.9）。
   *
   * 購読は**1 回だけ**設置する（器の組み立ての効果に混ぜない — 組み立ては列の構成が変わるたびに
   * 走り直す）。入口は**最新の値を指す参照**を通す: `runHistory` は毎回の描画で作られる関数で
   * あり（`onHistorySettled` と `refreshViolation` を閉じ込めている）、1 回だけ設置した購読が
   * 古い閉包を握ると、**反映先が古い状態のまま**になる（`handleRef` / `selectionRef` と同じ規律
   * である）。下の効果が毎回の描画のあとに参照を最新へ差し替える。
   */
  const historyEntryRef = useRef<(direction: GridHistoryDirection) => void>(runHistory);
  useEffect(() => {
    historyEntryRef.current = runHistory;
  });
  useEffect(
    () => installGridHistoryRequests((direction) => historyEntryRef.current(direction)),
    [],
  );

  return (
    // 窓の到着の回数を属性にも出す（**描き直しを起こした数の観測**であり、`arrivals` を使う
    // 唯一の場所である）。
    <div data-window-arrivals={arrivals} style={SURFACE_STYLE}>
      {/*
        取り消しとやり直し（8.9。要件 9.2、9.3、9.9）。**行の操作の行の隣に出す** — どちらも
        文書を変える操作であり、表示だけを変える 8.8 の行（`ViewBar`）とは置き場所が違う。
        **メニューの活性化と同じ入口**（`runHistory`）を叩く。
      */}
      <HistoryOperations
        onUndo={() => {
          runHistory("undo");
        }}
        onRedo={() => {
          runHistory("redo");
        }}
      />
      {/*
        行の操作と、その対象の数（8.6。要件 6.1、6.2、6.3、6.5）。**表の上に出す** — 消す行も
        足す位置も**いまの選択**であり、その提示（数え上げの行）の隣に在るのが読める位置である。
        行数は要件 6.2 が示す数（`summary.row_count`）であり、行が増減すればここが直ちに変わる。
      */}
      <RowOperations
        rowCount={summary.row_count}
        pendingDelete={pendingDelete}
        onInsert={() => {
          requestRowOperation({ kind: "insert", at: selection.current.row });
        }}
        onDelete={() => {
          requestRows("delete");
        }}
        onDuplicate={() => {
          requestRows("duplicate");
        }}
        onConfirm={() => {
          if (pendingDelete === null) {
            return;
          }
          // **確認の答えである**（閾値を見ない — 尋ねるのは 1 度だけである）。
          requestRowOperation({ kind: "confirmDelete", targets: pendingDelete });
        }}
        onCancel={onDeleteCancelled}
      />
      {/*
        選択の数え上げ（要件 2.5）。**利用者に見える数は 1 起点である**（内部の序数は 0 起点）。
        読み手（検査）のために、生の数を属性にも出しておく。
      */}
      <p
        data-testid="jxcel-grid-selection-counts"
        data-selection-rows={counts.rows}
        data-selection-columns={counts.columns}
        data-selection-cells={counts.cells}
        data-current-row={selection.current.row}
        data-current-column={selection.current.column}
        style={SELECTION_STYLE}
      >
        {visibleRows === 0
          ? // **行が 1 件も無いときは位置を名乗らない**（すべての行を消した後である）。描かれて
            // いる行が 0 件なのに「現在位置 1 行 1 列」と名乗るのは、利用者に見える食い違いで
            // ある（要件 6.5 の「行の位置の提示が直ちに更新される」は、消し切ったときこの形に
            // なる）。**足す操作は残る** — `at == 行数` への追加は妥当である。
            "行がありません"
          : `現在位置 ${String(selection.current.row + 1)} 行 ${String(selection.current.column + 1)} 列 ／ 選択 ${String(counts.rows)} 行 × ${String(counts.columns)} 列 = ${String(counts.cells)} セル`}
      </p>
      {editing === null ? null : (
        <CellEditorPanel
          // **編集の対象が変わったら組み直す。**参照先の行の読み込みは面の状態なので、
          // 別のセルへ移ったときに前のセルの一覧が残らないようにする（`editKey` と同じ規律）。
          key={`${String(editing.position.row)}:${String(editing.position.column)}`}
          edit={editing}
          column={columns[editing.position.column] ?? null}
          client={client}
          onCommit={(text, carrier) => {
            settle(editing.position, { kind: "commit", text }, carrier, onEditSettled);
          }}
          onCancel={(carrier) => {
            settle(editing.position, { kind: "cancel" }, carrier, onEditSettled);
          }}
        />
      )}
      {/*
        入れ子の値の詳細表示（8.5。要件 4.5、5.5、5.7）。**値を読むのはここである** — 窓の記憶
        （`getCell` の要約と `nestedMarks` の内側の位置）を持つのは表だからである。開いている
        位置そのものは画面の状態（`detail`）が持つ。
      */}
      {detail === null ? null : (
        <NestedInspector
          position={detail.position}
          column={detailColumn}
          declared={declaredInnerPositions(detailColumn)}
          summary={detailCell.text}
          loading={detailCell.loading}
          // 未取得は `null` であり、空の並び（違反なし）と区別する（要件 4.5）。
          innerViolations={detailMarks}
          editKey={detail.edit}
          onCommit={(text, carrier) => {
            // **運び手は面が登録簿から引いたものである**（要件 10.3。表は型を見ない）。
            settle(detail.position, { kind: "commit", text }, carrier, onDetailEditSettled);
          }}
          onCancel={() => {
            settleDetailCancel(detail.position);
          }}
          onClose={onDetailClosed}
        />
      )}
      <div
        ref={containerRef}
        onKeyDown={onKeyDown}
        data-testid="jxcel-grid-table"
        style={TABLE_STYLE}
      />
    </div>
  );
}

/**
 * 参照先の行を 1 頁読む（タスク 10.3。要件 3.8、10.3、10.4）。
 *
 * **読むかどうかは型の札ではなく材料が決める** — 参照先のシートを名乗る列（`reference_sheet`）
 * だけが頁を読む。画面に型ごとの分岐は 1 つも無い（要件 10.3）。参照しない列の答えは `null`
 * であり、**「頁を読んでいない」ことを状態として持つ**（空の並びは「参照先に行が無い」、失敗は
 * 「読めなかった」 — 3 つを混同しない）。
 *
 * **頁ごとである**（`./referenceRows`）。参照先が 1 万行でも一度に全部を読まない — 続きは
 * `previous` が繋ぐ（読んだ行の数だけ `start` が進むので、頁は重ならない）。
 */
export async function loadEditorReference(
  client: GridClient,
  column: ColumnDescriptor | null,
  previous: ReferenceRows,
): Promise<ReferenceRows | null> {
  if (column === null || column.reference_sheet === null) {
    return null;
  }
  return loadReferenceRows(client, column.column, previous);
}

/**
 * その列の入力手段が読む制約（**材料は境界から、参照先の行は読んだ頁から**。要件 3.8、10.3）。
 *
 * **結線はここ 1 箇所である**: ① 材料そのものは `./columnConstraints` が `ColumnDescriptor` の
 * 宣言から写し、② 読んだ頁は**参照の列のときだけ** `reference` へ載る。読めていない間も面は
 * 出る（材料が欠けた欄は既定へ落ちる。要件 10.4）。
 */
function editorConstraints(
  column: ColumnDescriptor | null,
  reference: ReferenceRows | null,
): ColumnConstraints {
  const material = constraintsOf(column);
  if (column === null || column.reference_sheet === null) {
    return material;
  }
  if (reference === null || reference.state !== "loaded") {
    return material;
  }
  return withReferenceRows(material, column.reference_sheet, reference.rows);
}

/** 編集中の 1 セルの面へ渡すもの（**状態を持たない**。`CellEditorPanel` が状態と往復を足す）。 */
export interface CellEditorPanelViewProps {
  readonly edit: CellEdit;
  /** その列の宣言（表示位置の列）。使用不能な列は `null`（札が読めない）。 */
  readonly column: ColumnDescriptor | null;
  /**
   * 読んだ参照先の頁。`null` は**まだ読んでいない**であり、読み込み中・失敗・読了は
   * [`ReferenceRows`] が区別する（参照しない列は決して読まないので、この欄は使われない）。
   */
  readonly reference: ReferenceRows | null;
  /** 次の頁を読む（**押下の口**。読むのは呼び出し側である — 状態を持つ側）。 */
  readonly onMore: () => void;
  /** 確定（**運び手つきで上げる**。`./cellEdit` がそれで命令を選ぶ。要件 5.7）。 */
  readonly onCommit: (text: string, carrier: EditCarrier) => void;
  /** 取消（**境界へ何も送らない**。要件 3.6）。 */
  readonly onCancel: (carrier: EditCarrier) => void;
}

/**
 * 編集中の 1 セルの面（要件 3.1）。**入力手段を登録簿から引く唯一の場所である。**
 *
 * 型ごとの分岐はここに 1 つも無い — 解決するのは `CellEditorRegistry.resolve` であり、未登録の
 * 札は登録簿が既定（値をそのまま扱う面）へ落とす（要件 10.3、10.4）。**本 module は入力手段の
 * 成分を名指ししない**（`editorRegistry.test.ts` が源の走査で固定する）。
 *
 * 面は**表の器の外**に出る。移植口に「セルの上へ DOM を重ねる」口は無く（`RendererSpec` に
 * そんな欄は無い）、覆われたセルを探させるより、**どのセルを編集しているかを名乗る**方が読める。
 *
 * **材料の組み立てはここが持つ**（`editorConstraints`）ので、読み込みの状態を持つ側
 * （[`CellEditorPanel`]）は頁を渡すだけでよい — `GridScreenView` と同じ分担である。
 */
export function CellEditorPanelView({
  edit,
  column,
  reference,
  onMore,
  onCommit,
  onCancel,
}: CellEditorPanelViewProps): ReactElement {
  // 入力手段と、その面が確定する文字の**運び手**（8.5。要件 5.5、10.3）。`columnEditor` が登録簿を
  // 通して 1 度に引く（成分と運び手が同じ 1 件の登録から来る）。
  const { component: Editor, carrier } = columnEditor(column);
  // 札が読めない列は `Any`（値をそのまま扱う面）として登録簿へ来る（7.4 の module doc）。
  const kind = column?.kind ?? "Any";
  // 参照先のシートは**宣言の材料**である（要件 3.8）。名乗らない列では頁の面を出さない。
  const sheet = column === null ? null : column.reference_sheet;
  const constraints = editorConstraints(column, reference);

  return (
    <div
      data-testid="jxcel-grid-editor"
      data-editor-row={edit.position.row}
      data-editor-column={edit.position.column}
      data-editor-kind={kind}
      data-editor-carrier={carrier}
      style={EDITOR_STYLE}
    >
      <span style={MESSAGE_STYLE}>
        {`${String(edit.position.row + 1)} 行 ${String(edit.position.column + 1)} 列を編集中`}
      </span>
      {/*
        参照先の行の頁（要件 3.8）。**面の外に出す** — 面（`editors/ref.tsx`）は
        `CellEditorProps` の 4 つしか受け取らないため（design.md が逐語で固定している）、
        続きを読む操作を持てない。材料が無い列では何も出さない（要件 10.4）。
      */}
      {sheet === null ? null : (
        <span
          data-testid="jxcel-grid-reference"
          data-reference-sheet={sheet}
          data-reference-state={reference?.state ?? "loading"}
          data-reference-rows={reference?.state === "loaded" ? reference.rows.length : 0}
          data-reference-total={reference?.state === "loaded" ? reference.total : 0}
          style={MESSAGE_STYLE}
        >
          {reference === null || reference.state === "loading"
            ? `参照先 ${sheet} の行を読んでいます`
            : reference.state === "failed"
              ? `参照先 ${sheet} の行を読めませんでした: ${reference.message}`
              : `参照先 ${sheet}: ${String(reference.rows.length)} 行を表示中（全 ${String(reference.total)} 行）`}
          {reference !== null && reference.state === "loaded" && reference.hasMore ? (
            <button
              type="button"
              data-testid="jxcel-grid-reference-more"
              onClick={onMore}
              style={BUTTON_STYLE}
            >
              {`次の ${String(REFERENCE_PAGE_SIZE)} 行を読む`}
            </button>
          ) : null}
        </span>
      )}
      <Editor
        initialText={edit.initialText}
        constraints={constraints}
        commit={(text: string) => {
          onCommit(text, carrier);
        }}
        cancel={() => {
          onCancel(carrier);
        }}
      />
    </div>
  );
}

/**
 * 編集中の 1 セルの面の**状態を持つ側**（要件 3.8。タスク 10.3）。**頁を読み、結果を面へ渡す。**
 *
 * 持つのは効果（最初の頁）と次の頁の押下だけであり、組み立ては [`CellEditorPanelView`] が
 * 担う。**読むかどうかは `loadEditorReference` が材料から決める**ので、ここに型の分岐は無い。
 * 依存は**文書の列と参照先のシート**である — 記述そのものを依存に取ると、列の構成が変わるたびに
 * 頁を読み直す（記述の同一性は本 module の状態では保証されない）。
 */
function CellEditorPanel({
  edit,
  column,
  client,
  onCommit,
  onCancel,
}: {
  readonly edit: CellEdit;
  /** その列の宣言（表示位置の列）。使用不能な列は `null`（札が読めない）。 */
  readonly column: ColumnDescriptor | null;
  /** 参照先の行を頁ごとに読む口（**参照の列のときだけ使う**。要件 3.8）。 */
  readonly client: GridClient;
  /** 確定（**運び手つきで上げる**。`./cellEdit` がそれで命令を選ぶ。要件 5.7）。 */
  readonly onCommit: (text: string, carrier: EditCarrier) => void;
  /** 取消（**境界へ何も送らない**。要件 3.6）。 */
  readonly onCancel: (carrier: EditCarrier) => void;
}): ReactElement {
  const [reference, setReference] = useState<ReferenceRows | null>(null);
  const referenceColumn = column?.column ?? null;
  const referenceSheet = column?.reference_sheet ?? null;
  useEffect(() => {
    let cancelled = false;
    void loadEditorReference(client, column, { state: "loading" }).then((next) => {
      if (!cancelled) {
        setReference(next);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [client, referenceColumn, referenceSheet]);

  return (
    <CellEditorPanelView
      edit={edit}
      column={column}
      reference={reference}
      onMore={() => {
        // **続きは読んだ行の数だけ進める**（頁は重ならない）。読む口は最初の頁と同じ 1 本である。
        void loadEditorReference(client, column, reference ?? { state: "loading" }).then(setReference);
      }}
      onCommit={onCommit}
      onCancel={onCancel}
    />
  );
}

/**
 * 行の操作と、その対象の数、削除の確認（8.6。要件 6.1、6.2、6.3、6.5）。
 *
 * **表の面が描く**のは、3 つの操作が要するものが**窓の記憶と選択**だからである（行の識別子は
 * 窓にしか無く、対象はいまの選択である）。呼び出し側（`GridSurface`）が判断と往復を持ち、
 * この面は**操作と数を出すだけ**である（数え上げの行や編集の面と同じ分担である）。
 *
 * 操作を**メニューと打鍵の双方から**実行できるようにするのは要件 7.8 / 9.9 であり、それは 8.7 が
 * この面の隣へ足す（本タスクは操作そのものである — 要件 6.1、6.2、6.3 は操作の入口を
 * 指定していない）。
 *
 * 確認（要件 6.5）は**その場に出す**（`window.confirm` のような器の外の対話にしない）: 画面は
 * 器の中の 1 つの面であり、`node` の環境では観測できないものを増やさない。数を示すのは
 * **尋ねた時点の対象の数**であり、選択が動けば画面の側が取り下げる（`GridScreenState` の
 * `pendingDelete`）。
 */
function RowOperations({
  rowCount,
  pendingDelete,
  onInsert,
  onDelete,
  onDuplicate,
  onConfirm,
  onCancel,
}: {
  /** 提示する行数（要件 6.2。シートの行数であり、行の増減で直ちに変わる）。 */
  readonly rowCount: number;
  /** 確認を待っている対象（8.6。`null` なら尋ねていない）。 */
  readonly pendingDelete: DeleteConfirmation | null;
  /** 現在位置の行の位置へ 1 行足す（要件 6.1）。 */
  readonly onInsert: () => void;
  /** 選択した行を消す（**閾値を超えていれば確認を求める**。要件 6.2、6.5）。 */
  readonly onDelete: () => void;
  /** 選択した行と同じ値の行を足す（要件 6.3）。 */
  readonly onDuplicate: () => void;
  /** 確認に答える（**尋ねるのは 1 度だけである**）。 */
  readonly onConfirm: () => void;
  /** 確認を取り消す（**境界へ何も送らない**）。 */
  readonly onCancel: () => void;
}): ReactElement {
  return (
    <div data-testid="jxcel-grid-row-ops" style={ROW_OPS_STYLE}>
      <button
        type="button"
        data-testid="jxcel-grid-insert-row"
        onClick={onInsert}
        style={BUTTON_STYLE}
      >
        行を追加
      </button>
      <button
        type="button"
        data-testid="jxcel-grid-delete-rows"
        onClick={onDelete}
        style={BUTTON_STYLE}
      >
        行を削除
      </button>
      <button
        type="button"
        data-testid="jxcel-grid-duplicate-rows"
        onClick={onDuplicate}
        style={BUTTON_STYLE}
      >
        行を複製
      </button>
      {/*
        行数（要件 6.2）。**行が増減すれば直ちにこの数が変わる**（状態が要約の数を置き換える）。
      */}
      <span data-testid="jxcel-grid-row-count" data-row-count={rowCount} style={MESSAGE_STYLE}>
        {`行数 ${String(rowCount)}`}
      </span>
      {pendingDelete === null ? null : (
        <span data-testid="jxcel-grid-delete-confirm" role="alert" style={CONFIRM_STYLE}>
          <span style={MESSAGE_STYLE}>
            {`${String(pendingDelete.count)} 行を削除します。よろしいですか？`}
          </span>
          <button
            type="button"
            data-testid="jxcel-grid-delete-confirm-yes"
            onClick={onConfirm}
            style={BUTTON_STYLE}
          >
            削除する
          </button>
          <button
            type="button"
            data-testid="jxcel-grid-delete-confirm-cancel"
            onClick={onCancel}
            style={BUTTON_STYLE}
          >
            取り消す
          </button>
        </span>
      )}
    </div>
  );
}

/**
 * 取り消しとやり直しの 2 つの操作（8.9。要件 9.2、9.3、9.9）。
 *
 * **表の面が描く**のは、この 2 つが**窓の記憶**（取り消しの後の作り直しと、移動先の解決）を
 * 要するためである（行の操作の 3 つと同じ理由である。呼び出し側（`GridSurface`）が判断と往復を
 * 持ち、この面は**操作を出すだけ**である）。
 *
 * **表示名はメニューの項目と同じである**（`編集 > 元に戻す` / `編集 > やり直し`）— 同じ操作に
 * 2 つの呼び名を作らない（`src-tauri/src/commands/grid.rs` の `UNDO_LABEL` / `REDO_LABEL` と
 * `scripts/check-menu-shortcut.sh` の `EXPECTED` が同じ綴りを要求する）。
 *
 * 打鍵（`Ctrl+Z` / `Ctrl+Shift+Z`、macOS は `Cmd`）は**器のメニューのアクセラレータ**が担う
 * （この画面は打鍵を聴かない。`./history` の module doc「キーボードの経路はアクセラレータで
 * ある」）。
 */
function HistoryOperations({
  onUndo,
  onRedo,
}: {
  /** 直前の操作の前の状態へ戻す（要件 9.2）。 */
  readonly onUndo: () => void;
  /** 取り消した操作を再び適用する（要件 9.3）。 */
  readonly onRedo: () => void;
}): ReactElement {
  return (
    <div data-testid="jxcel-grid-history" style={ROW_OPS_STYLE}>
      <button type="button" data-testid="jxcel-grid-undo" onClick={onUndo} style={BUTTON_STYLE}>
        元に戻す
      </button>
      <button type="button" data-testid="jxcel-grid-redo" onClick={onRedo} style={BUTTON_STYLE}>
        やり直し
      </button>
    </div>
  );
}

// ===========================================================================
// 5. 見た目（**配色は器のカスタムプロパティだけを参照する**）
// ===========================================================================
/** 画面の枠。**領域いっぱいに広がる**（領域は中央寄せなので、自前で伸ばさないと縦に潰れる）。 */
const ROOT_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.75rem",
  width: "100%",
  height: "100%",
  minHeight: 0,
  boxSizing: "border-box",
  padding: "1rem",
  borderRadius: "0.5rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 見出し。 */
const HEADING_STYLE = { margin: 0, fontSize: "1.125rem" } as const;

/** 説明・失敗の理由・告知の文字（補助的な文字色）。 */
const MESSAGE_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 内容の区画（読み込み中・失敗・空の状態）。 */
const PANEL_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
  alignItems: "flex-start",
} as const;

/** 操作（再試行・告知を閉じる）。**枠線と文字に器の配色を使う。** */
const BUTTON_STYLE = {
  font: "inherit",
  fontSize: "0.875rem",
  padding: "0.25rem 0.75rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.controlActiveText})`,
  backgroundColor: `var(${APPEARANCE_VARS.controlActiveBackground})`,
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 告知の 1 行（**内容の領域の上に出す**）。 */
const NOTICE_STYLE = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: "0.75rem",
  padding: "0.5rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
} as const;

/** 列の構成（要件 1.5 の提示）。**宣言の順に並べる。** */
const COLUMNS_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  gap: "0.5rem",
  margin: 0,
  padding: 0,
  listStyle: "none",
} as const;

/** 列 1 本ぶんの見出し。 */
const COLUMN_STYLE = {
  padding: "0.15rem 0.5rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 編集中のセルの面（**表の器の外に出す**。どのセルを編集しているかを名乗る）。 */
const EDITOR_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  alignItems: "center",
  gap: "0.5rem",
  padding: "0.5rem 0.75rem",
  borderRadius: "0.25rem",
  border: `1px solid var(${APPEARANCE_VARS.controlActiveBackground})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** 確定の報告の中身（型強制の一覧と、残った違反）。 */
const REPORT_BODY_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
} as const;

/** 型強制の一覧（要件 3.4）。**1 件 1 行である。** */
const REPORT_LIST_STYLE = {
  margin: 0,
  padding: 0,
  listStyle: "none",
  fontSize: "0.8125rem",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/**
 * 列の同一性の綴り（`ColumnDescriptor` の doc「列の同一性は（`column`, `path`）の対である」）。
 *
 * 表示名は人が読むためのものであり、同一性ではない（同じ名前の列が 2 本ありうる）。並びの鍵に
 * 名前を使うと、名前が重複したスキーマで鍵が衝突する。
 */
function columnKey(column: ColumnDescriptor): string {
  const path = column.path
    .map((segment) => (segment.segment === "Field" ? segment.name : `[${segment.position}]`))
    .join(".");
  return `${column.column}:${path}`;
}

/** 内容の領域。**状態を網羅的に分岐する**（新しい変種を足すと型検査がここで落ちる）。 */
function GridScreenBody({
  model,
  client,
  onRetry,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onNextViolation,
  onViolationRead,
  onExpansion,
  onDetailOpened,
  onDetailEditSettled,
  onDetailClosed,
  onColumnWidth,
  onColumnMove,
  onView,
  onRowOperationSettled,
  onPasteSettled,
  onHistorySettled,
  onDeleteRequested,
  onDeleteCancelled,
  onRefused,
}: {
  readonly model: GridScreenModel;
  /** 境界の口（表を描く腕が、編集の 1 往復に使う）。 */
  readonly client: GridClient;
  readonly onRetry: () => void;
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
  /** バーの「次の違反へ」（要件 4.4）。 */
  readonly onNextViolation: () => void;
  /** 表が読んだ違反の提示（要件 4.2、4.6）。 */
  readonly onViolationRead: (reading: ViolationReading) => void;
  /** 列の展開の操作（要件 5.1、5.2）。**送るのはいまの指定に足した完全な記述である。** */
  readonly onExpansion: (state: GridExpansionState) => void;
  /** 詳細表示の入口（要件 5.4、5.5）。 */
  readonly onDetailOpened: (position: CellPosition) => void;
  /** 列の幅の変更（8.8。要件 8.1。**表示位置**で指す）。 */
  readonly onColumnWidth: (displayPosition: number, width: number) => void;
  /** 列の表示位置の変更（8.8。要件 8.2。**表示順の位置どうし**で指す）。 */
  readonly onColumnMove: (from: number, to: number) => void;
  /** 並べ替え・絞り込みの操作（8.8。要件 8.3、8.4）。 */
  readonly onView: (operation: ViewOperation) => void;
  /** 詳細表示の中の編集の結果（要件 5.7）。 */
  readonly onDetailEditSettled: (settlement: CellEditSettlement) => void;
  /** 詳細表示を閉じる（**値も文書も動かない**）。 */
  readonly onDetailClosed: () => void;
  /** 行の操作の 1 往復の結果（8.6。要件 6.1、6.2、6.3）。 */
  readonly onRowOperationSettled: (settlement: RowOperationSettlement) => void;
  /** 貼り付けの 1 往復の結果（8.7。要件 7.3、7.4、1.7）。 */
  readonly onPasteSettled: (settlement: PasteSettlement) => void;
  /** 取り消し・やり直しの 1 往復の結果（8.9。要件 9.2、9.3、9.8）。 */
  readonly onHistorySettled: (settlement: HistorySettlement) => void;
  /** 削除の確認を求める（8.6。要件 6.5。**送っていない**）。 */
  readonly onDeleteRequested: (confirmation: DeleteConfirmation) => void;
  /** 確認への取り消し（8.6。**送らない**）。 */
  readonly onDeleteCancelled: () => void;
  /** 送らずに理由を告げる（8.6。挿入の位置を写せない・識別子が届いていない）。 */
  readonly onRefused: (message: string) => void;
}): ReactElement {
  const state = model.state;
  /**
   * **描かれる列の並び**（表示順。要件 8.2）。**1 度だけ組んで 3 つの読み手へ渡す** —
   * 表（窓の記憶の写像と、描く列）、列ごとの操作の行（8.5）、表示の操作の行（8.8）である。
   *
   * 読み手ごとに組むと、**片方だけが並びに追随する**日が来る（`./viewOps` の module doc
   * 「表示の並びは 1 つである」）。依存に `state` を取るのは、[`DisplayState`] が可変の値で
   * あり、変化が**状態の置き換え**としてしか観測できないためである。
   */
  const columns = useMemo(
    () => (state.status === "ready" ? drawnColumns(state.summary.columns, state.display) : []),
    [state],
  );
  switch (state.status) {
    case "loading":
      return (
        <div data-testid="jxcel-grid-loading" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>シートを読み込んでいます</h2>
        </div>
      );
    case "failed":
      return (
        <div data-testid="jxcel-grid-failure" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>シートを開けませんでした</h2>
          <p style={MESSAGE_STYLE}>{state.message}</p>
          {state.canRetry ? (
            <button type="button" data-testid="jxcel-grid-retry" onClick={onRetry} style={BUTTON_STYLE}>
              再試行
            </button>
          ) : null}
        </div>
      );
    case "no-schema":
      // 要件 1.6: **表を描かない。**
      return (
        <div data-testid="jxcel-grid-schema-undefined" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>スキーマが未定義です</h2>
          <p style={MESSAGE_STYLE}>
            シート「{state.sheetName}」には列が 1 本も宣言されていません。表は描きません。
          </p>
        </div>
      );
    case "no-rows":
      // 要件 1.5: **列の構成を示したうえで**行が無いことを示す。
      return (
        <div data-testid="jxcel-grid-no-rows" style={PANEL_STYLE}>
          <h2 style={HEADING_STYLE}>行がありません</h2>
          <p style={MESSAGE_STYLE}>
            シート「{state.sheetName}」には行が 1 件もありません。列の構成は次の {state.columns.length}
            列です。
          </p>
          <ol data-testid="jxcel-grid-columns" style={COLUMNS_STYLE}>
            {state.columns.map((column) => (
              <li key={columnKey(column)} data-grid-column={columnKey(column)} style={COLUMN_STYLE}>
                {column.name}
              </li>
            ))}
          </ol>
        </div>
      );
    case "ready":
      return (
        <>
          {/*
            違反のバー（要件 4.3、4.4）。**表を描く状態にだけ出す** — 総数の源は
            `grid_set_view` の応答であり、他の腕はその呼び出しをしていない（`GridScreenState`
            の「この腕が持つことが総数の源を持つことを表す」）。
          */}
          <ViolationBar
            total={state.violationTotal}
            presentation={state.violation}
            onNext={onNextViolation}
          />
          {/*
            列ごとの操作（8.5。要件 5.1、5.2、5.4、5.6）。**表の上に出す** — 移植口に「見出しの
            操作を受け取る口」は無く（`RendererSpec` の列の知らせは幅と並びだけである）、列の
            名前と操作を並べる方が、どの列の操作かが読める。
          */}
          <NestedColumnControls
            // **表示順の構成を渡す**（8.8）。列ごとの操作は「その位置に描かれている列」に
            // つくものであり、位置（詳細表示の宛先）は表示の位置である — 構成の順で渡すと、
            // 並びを変えた後で**別の列の名前と操作**が並ぶ。
            columns={columns}
            view={state.view}
            // 詳細表示は**現在位置の行**の値について開く。
            currentRow={state.selection.current.row}
            onExpansion={onExpansion}
            onDetail={onDetailOpened}
          />
          {/*
            表示の操作（8.8。要件 8.1、8.2、8.3、8.4、8.7）。**表の上に出す** — 列幅も列順も
            並べ替えも絞り込みも、対象は利用者がいま見ている列であり、その提示（表）の隣に在るのが
            読める位置である（8.6 の行の操作と同じ判断である）。
          */}
          <ViewBar
            columns={columns}
            display={state.display}
            view={state.view}
            visibleRows={state.visibleRows}
            hiddenRows={state.hiddenRows}
            onColumnWidth={onColumnWidth}
            onColumnMove={onColumnMove}
            onView={onView}
          />
          <GridSurface
            sheet={state.sheet}
            summary={state.summary}
            columns={columns}
            display={state.display}
            layoutKey={state.layoutKey}
            rowOrderKey={state.rowOrderKey}
            visibleRows={state.visibleRows}
            hiddenRows={state.hiddenRows}
            generation={state.generation}
            selection={state.selection}
            editing={state.editing}
            detail={state.detail}
            pendingDelete={state.pendingDelete}
            client={client}
            onSelectionChange={onSelectionChange}
            onEditStarted={onEditStarted}
            onEditSettled={onEditSettled}
            onDetailEditSettled={onDetailEditSettled}
            onDetailClosed={onDetailClosed}
            onViolationRead={onViolationRead}
            onColumnResize={onColumnWidth}
            onColumnMove={onColumnMove}
            onRowOperationSettled={onRowOperationSettled}
            onPasteSettled={onPasteSettled}
            onHistorySettled={onHistorySettled}
            onDeleteRequested={onDeleteRequested}
            onDeleteCancelled={onDeleteCancelled}
            onRefused={onRefused}
          />
        </>
      );
    default:
      return assertNever(state);
  }
}

/** 見た目へ渡すもの。**状態と操作だけである**（読み込みは [`GridScreen`] が持つ）。 */
export interface GridScreenViewProps {
  readonly model: GridScreenModel;
  /**
   * 境界の口。**表を描く腕が編集の 1 往復に使う**（入力手段の 2 つの口 → `./cellEdit`）。
   * 差し替えの口である（検査は偽の実装を渡せる。8.1 の `loadGridScreenState` と同じ規律）。
   */
  readonly client: GridClient;
  /** 開く流れをやり直す（失敗の提示の「再試行」）。 */
  readonly onRetry: () => void;
  /** 告知を閉じる。 */
  readonly onDismissNotice: () => void;
  /**
   * 選択が変わった（打鍵・ポインタのどちらでも）。要件 2.1、2.2、2.3。
   *
   * `null` は**実装が選択を解除した**こと（Glide の Escape など）である。画面はそれを
   * 取り下げず、いまの選択を置き直す（[`gridScreenSelectionChanged`] の doc）。
   */
  readonly onSelectionChange: (selection: RendererSelection | null) => void;
  /** 編集が起動された（要件 3.1。位置と、いま描かれている値）。 */
  readonly onEditStarted: (position: CellPosition, initialText: string) => void;
  /** 確定の 1 往復の結果（要件 3.3、3.4、3.5、3.6）。 */
  readonly onEditSettled: (settlement: CellEditSettlement) => void;
  /** 直近の確定の報告を閉じる（**文書も値も動かない**）。 */
  readonly onDismissEditReport: () => void;
  /**
   * バーが指示する「次の違反へ」（要件 4.4）。
   *
   * **表の外の出来事である**（境界への問い合わせと、現在位置の移動）。表を描く腕の外に
   * 置いてあるのは、巡回が要するものが**境界の口と可視行数だけ**であり、窓の記憶も移植口の
   * 取っ手も要さないためである（追随は現在位置の移動から自動的に起きる — 要件 2.4）。
   */
  readonly onNextViolation: () => void;
  /** 表が読んだ違反の提示（要件 4.2、4.6）。 */
  readonly onViolationRead: (reading: ViolationReading) => void;
  /** 列の展開の操作（8.5。要件 5.1、5.2）。 */
  readonly onExpansion: (state: GridExpansionState) => void;
  /** 詳細表示の入口（8.5。要件 5.4、5.5）。 */
  readonly onDetailOpened: (position: CellPosition) => void;
  /** 詳細表示の中の編集の結果（8.5。要件 5.7）。 */
  readonly onDetailEditSettled: (settlement: CellEditSettlement) => void;
  /** 詳細表示を閉じる（8.5）。 */
  readonly onDetailClosed: () => void;
  /**
   * 列の幅の変更（8.8。要件 8.1）。
   *
   * **移植口の知らせ（`RendererSpec.onColumnResize`）と、列ごとの操作の入力の両方がここへ
   * 来る** — 2 つを別の経路にすると、片方だけが幅を動かす日が来る（8.7 が複製の入口を
   * 1 つに寄せたのと同じ判断である）。
   */
  readonly onColumnWidth: (displayPosition: number, width: number) => void;
  /** 列の表示位置の変更（8.8。要件 8.2。**表示順の位置どうし**である）。 */
  readonly onColumnMove: (from: number, to: number) => void;
  /** 並べ替え・絞り込みの操作（8.8。要件 8.3、8.4）。 */
  readonly onView: (operation: ViewOperation) => void;
  /** 行の操作の 1 往復の結果（8.6。要件 6.1、6.2、6.3、6.5）。 */
  readonly onRowOperationSettled: (settlement: RowOperationSettlement) => void;
  /** 貼り付けの 1 往復の結果（8.7。要件 7.3、7.4、1.7）。 */
  readonly onPasteSettled: (settlement: PasteSettlement) => void;
  /** 取り消し・やり直しの 1 往復の結果（8.9。要件 9.2、9.3、9.8）。 */
  readonly onHistorySettled: (settlement: HistorySettlement) => void;
  /** 削除の確認を求める（8.6。要件 6.5。**送っていない**）。 */
  readonly onDeleteRequested: (confirmation: DeleteConfirmation) => void;
  /** 確認への取り消し（8.6。**送らない**）。 */
  readonly onDeleteCancelled: () => void;
  /** 送らずに理由を告げる（8.6）。 */
  readonly onRefused: (message: string) => void;
}

/**
 * 画面の見た目。**状態だけを受け取る純粋な描画である**ので、検査は状態ごとにこれを呼んで
 * 「何が DOM へ出るか」を読める（`GridScreen.test.ts`）。
 */
export function GridScreenView({
  model,
  client,
  onRetry,
  onDismissNotice,
  onDismissEditReport,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onNextViolation,
  onViolationRead,
  onExpansion,
  onDetailOpened,
  onDetailEditSettled,
  onDetailClosed,
  onColumnWidth,
  onColumnMove,
  onView,
  onRowOperationSettled,
  onPasteSettled,
  onHistorySettled,
  onDeleteRequested,
  onDeleteCancelled,
  onRefused,
}: GridScreenViewProps): ReactElement {
  return (
    <section data-testid="jxcel-grid-screen" aria-label="グリッド" style={ROOT_STYLE}>
      {model.notice === null ? null : (
        <div data-testid="jxcel-grid-notice" role="status" style={NOTICE_STYLE}>
          <span style={MESSAGE_STYLE}>{model.notice}</span>
          <button
            type="button"
            data-testid="jxcel-grid-notice-dismiss"
            onClick={onDismissNotice}
            style={BUTTON_STYLE}
          >
            閉じる
          </button>
        </div>
      )}
      {model.editReport === null ? null : (
        <div data-testid="jxcel-grid-edit-report" role="status" style={NOTICE_STYLE}>
          <div style={REPORT_BODY_STYLE}>
            {/*
              型強制（要件 3.4）。**変換が起きたこと**と、**変換前の値**を出す。前後の表示文字列は
              どちらも境界が運んだものである（`GridCoercionNotice` の doc）— 画面は解釈しない。
            */}
            {model.editReport.coercions.length === 0 ? null : (
              <ul data-testid="jxcel-grid-coercions" style={REPORT_LIST_STYLE}>
                {model.editReport.coercions.map((coercion) => (
                  <li
                    key={`${coercion.cell.row}:${String(coercion.cell.column)}`}
                    data-coercion-row={coercion.cell.row}
                    data-coercion-column={coercion.cell.column}
                    data-coercion-before={coercion.before}
                    data-coercion-after={coercion.after}
                  >
                    {`型強制: 行「${coercion.cell.row}」の ${String(coercion.cell.column + 1)} 列目 — 変換前「${coercion.before}」／変換後「${coercion.after}」`}
                  </li>
                ))}
              </ul>
            )}
            {/*
              残った違反（要件 3.5。**提示の本体は 8.4**）。**値は文書に残っている** — 判定する
              側は編集を決して拒否せず、適合しない値も破棄せずに返す（`grid_apply_edit` の doc）。
              総数は**シート全体**の数である（再検証した列に閉じているのは、下に並ぶ位置のほうである）。
            */}
            {model.editReport.violationTotal === 0 ? null : (
              <p
                data-testid="jxcel-grid-violations"
                data-violation-total={model.editReport.violationTotal}
                data-violation-count={model.editReport.violations.length}
                style={MESSAGE_STYLE}
              >
                {`違反 ${String(model.editReport.violationTotal)} 件（シート全体の総数）${
                  model.editReport.violations.length === 0
                    ? ""
                    : `: ${model.editReport.violations
                        .map(
                          (violation) =>
                            `行「${violation.row ?? "（行なし）"}」の ${String(violation.column + 1)} 列目`,
                        )
                        .join("、")}`
                }`}
              </p>
            )}
          </div>
          <button
            type="button"
            data-testid="jxcel-grid-edit-report-dismiss"
            onClick={onDismissEditReport}
            style={BUTTON_STYLE}
          >
            閉じる
          </button>
        </div>
      )}
      <GridScreenBody
        model={model}
        client={client}
        onRetry={onRetry}
        onSelectionChange={onSelectionChange}
        onEditStarted={onEditStarted}
        onEditSettled={onEditSettled}
        onNextViolation={onNextViolation}
        onViolationRead={onViolationRead}
        onExpansion={onExpansion}
        onDetailOpened={onDetailOpened}
        onDetailEditSettled={onDetailEditSettled}
        onDetailClosed={onDetailClosed}
        onColumnWidth={onColumnWidth}
        onColumnMove={onColumnMove}
        onView={onView}
        onRowOperationSettled={onRowOperationSettled}
        onPasteSettled={onPasteSettled}
        onHistorySettled={onHistorySettled}
        onDeleteRequested={onDeleteRequested}
        onDeleteCancelled={onDeleteCancelled}
        onRefused={onRefused}
      />
    </section>
  );
}

// ===========================================================================
// 6. 画面の実体（登録簿へ差し込まれる側）
// ===========================================================================

/**
 * グリッド画面。**`ScreenProps` だけを受け取り、それ以外の props を持たない**（画面の契約 1）。
 *
 * 器が渡す 2 つ（`screenId` と `navigate`）をこの画面は使わないので、引数を 1 つも宣言しない
 * （宣言しなければ、余分な props を要求する経路が生まれない）。
 *
 * 読み込みは**試行の番号を依存に持つ効果** 1 つで行う。後始末で「古い結果を捨てる」印を立てる
 * ので、再試行の途中に届いた前の応答が新しい表示を上書きしない。
 */
export function GridScreen(): ReactElement {
  const [model, setModel] = useState<GridScreenModel>(initialGridScreenModel);
  /**
   * 巡回の世代。**遅れて届いた答えを捨てる**（二度押しの 1 つ目が後から届いても、その間に
   * 動いた現在位置を巻き戻さない）。
   */
  const traversalRef = useRef(0);
  /**
   * 表示の指定を送った世代。**遅れて届いた答えを捨てる**（巡回と同じ規律。二度押しの 1 つ目が
   * 後から届いても、その間に組んだ指定を巻き戻さない）。
   */
  const viewRef = useRef(0);
  /**
   * 送る途中の表示の指定（**まだ応答が返っていない押下を畳む**）。
   *
   * 指定は**完全な記述**である（生成物の `GridViewSpec` の doc）ので、2 つの押下が続くと
   * 2 つ目は 1 つ目の**応答を待たずに**組まれる。そのとき `ready.view` を起点にすると、1 つ目の
   * 押下が指定から消える（ドメインは要求に現れない展開を折りたたみへ戻す）。押された指定を
   * ここへ積んでから送る。
   */
  const pendingViewRef = useRef<GridViewSpec | null>(null);

  useEffect(() => {
    let cancelled = false;
    void loadGridScreenState(DEFAULT_CLIENT).then((state) => {
      if (cancelled) {
        return;
      }
      setModel((current) => gridScreenLoaded(current, state));
    });
    return () => {
      cancelled = true;
    };
  }, [model.attempt]);

  const retry = useCallback(() => {
    setModel(gridScreenRetried);
  }, []);
  const dismissNotice = useCallback(() => {
    setModel(gridScreenNoticeDismissed);
  }, []);
  const select = useCallback((selection: RendererSelection | null) => {
    // **器に届かない失敗と同じ側である**（イベントハンドラ）。ここは状態の遷移だけであり、
    // 表を描いていないときは遷移が自分で何もしない（`gridScreenSelectionChanged`）。
    setModel((current) => gridScreenSelectionChanged(current, selection));
  }, []);
  const startEdit = useCallback((position: CellPosition, initialText: string) => {
    setModel((current) => gridScreenEditStarted(current, position, initialText));
  }, []);
  const settleEdit = useCallback((settlement: CellEditSettlement) => {
    // **非同期の結果である**（`ScreenBoundary` は効果の同期の例外しか捕まえない）。遷移は
    // 全域であり、投げない（`gridScreenEditSettled`）。
    setModel((current) => gridScreenEditSettled(current, settlement));
  }, []);
  const dismissEditReport = useCallback(() => {
    setModel(gridScreenEditReportDismissed);
  }, []);
  const readViolation = useCallback((reading: ViolationReading) => {
    // **表から上がってくる読み取りである**（窓の印と行の識別子を読んだ結果）。遷移は全域で
    // あり、投げない（`gridScreenViolationReason`）。
    setModel((current) => gridScreenViolationReason(current, reading));
  }, []);
  const goToNextViolation = useCallback(() => {
    const state = model.state;
    if (state.status !== "ready") {
      return;
    }
    const token = (traversalRef.current += 1);
    // 起点（いまの行の次）を決めるのは `./violations` である（そこに規則があり、検査もある）。
    // **写像も渡す**（`./columnSpace`）— 着地点は表示の位置でなければならない（境界が運ぶのは
    // 文書の列であり、展開と**表示上の列順**があると一致しない。`./violations` の module doc）。
    // 並びは**描かれる列**（表示順）である — 構成の順で組むと、並びを変えた後で**別の列へ
    // 現在位置が着く**（要件 8.2、8.6 と同じ取り違えである）。
    void nextViolation({
      client: DEFAULT_CLIENT,
      current: state.selection.current,
      rowCount: state.visibleRows,
      space: createColumnSpace(drawnColumns(state.summary.columns, state.display)),
    }).then((reading) => {
      if (token !== traversalRef.current) {
        return;
      }
      setModel((current) => gridScreenNextViolation(current, reading));
    });
  }, [model]);

  /**
   * **表示の指定を 1 つ適用する**（要件 5.1、5.2、5.3、8.3、8.4）。
   *
   * 4 つの入口（展開・並べ替え・絞り込み・数の取り直し）がここへ集まる。**送るのは常に
   * 「完全な記述」である**（ドメインは要求に現れない指定を既定へ戻す）ので、操作は
   * [`applyViewOperation`] で 1 つの指定へ写してから送る — 部分的な指定を送る経路を作らない。
   *
   * 送る途中の指定は [`pendingViewRef`] へ積む — 2 つの押下が続くと、2 つ目は 1 つ目の応答を
   * 待たずに組まれるので、`ready.view` を起点にすると**1 つ目の押下が指定から消える**。
   *
   * `update` は**いま送ろうとしている指定**（まだ応答が返っていない押下を含む）から組み立てる
   * 関数である。状態（`ready.view`）から組むと、押下が続いたときに前の押下を落とす。
   */
  const sendView = useCallback(
    (update: (view: GridViewSpec) => GridViewSpec) => {
      const state = model.state;
      if (state.status !== "ready") {
        return;
      }
      const token = (viewRef.current += 1);
      const next = update(pendingViewRef.current ?? state.view);
      pendingViewRef.current = next;
      void applyGridView(DEFAULT_CLIENT, next).then((settlement) => {
        if (token !== viewRef.current) {
          return;
        }
        // 応答が返った（適用されたか、失敗したか）。**次の押下は状態を起点に組む。**
        pendingViewRef.current = null;
        setModel((current) => gridScreenViewSettled(current, settlement));
      });
    },
    [model],
  );

  /** 列の展開の操作（要件 5.1、5.2、5.3）。**他の 2 つの指定を落とさないためにここを通る。** */
  const expand = useCallback(
    (expansion: GridExpansionState) => {
      sendView((view) => applyViewOperation(view, { kind: "expansion", state: expansion }));
    },
    [sendView],
  );

  /** 並べ替えと絞り込みの操作（8.8。要件 8.3、8.4）。 */
  const updateView = useCallback(
    (operation: ViewOperation) => {
      sendView((view) => applyViewOperation(view, operation));
    },
    [sendView],
  );

  /**
   * 列の幅の変更（8.8。要件 8.1）。**移植口の知らせと列ごとの入力が同じここへ来る**
   * （2 つを別の経路にすると、片方だけが幅を動かす日が来る）。
   */
  const resizeColumn = useCallback((displayPosition: number, width: number) => {
    setModel((current) => gridScreenColumnResized(current, displayPosition, width));
  }, []);

  /** 列の表示位置の変更（8.8。要件 8.2。**幅は動かない** — 7.5 の規則である）。 */
  const moveColumn = useCallback((from: number, to: number) => {
    setModel((current) => gridScreenColumnMoved(current, from, to));
  }, []);
  const openDetail = useCallback((position: CellPosition) => {
    setModel((current) => gridScreenDetailOpened(current, position));
  }, []);
  const closeDetail = useCallback(() => {
    setModel(gridScreenDetailClosed);
  }, []);
  const settleDetailEdit = useCallback((settlement: CellEditSettlement) => {
    // セルの編集と同じ規律である（違うのは、面を初期状態へ戻す鍵が進むことだけである）。
    setModel((current) => gridScreenDetailEditSettled(current, settlement));
  }, []);
  /**
   * 行の操作の 1 往復の結果（8.6。要件 6.1、6.2、6.3、6.5）。
   *
   * **非同期の結果である**（`ScreenBoundary` は効果の同期の例外しか捕まえない）。遷移は全域で
   * あり、投げない（`gridScreenRowOperationSettled`）。
   */
  const settleRowOperation = useCallback(
    (settlement: RowOperationSettlement) => {
      const state = model.state;
      setModel((current) => gridScreenRowOperationSettled(current, settlement));
      // **行の集合が変わったなら数を取り直す**（要件 8.7）。指定が行を絞っている間、適用の
      // 応答が運ぶ行数はシートの行数であり、可視行数でも隠れた行の数でもない — 数を知る
      // 唯一の源は表示の指定の応答である（`needsViewRefresh` の doc）。
      if (
        state.status === "ready" &&
        settlement.status === "applied" &&
        needsViewRefresh({
          view: state.view,
          outcome: settlement.outcome,
          sheetRowsBefore: state.summary.row_count,
        })
      ) {
        sendView((view) => view);
      }
    },
    [model, sendView],
  );
  /** 削除の確認を求める（**送っていない。**数を示して尋ねるだけである。要件 6.5）。 */
  const requestDelete = useCallback((confirmation: DeleteConfirmation) => {
    setModel((current) => gridScreenDeleteRequested(current, confirmation));
  }, []);
  /** 確認への取り消し（**境界へ 1 つも送らない**。文書も表示も動かない）。 */
  const cancelDelete = useCallback(() => {
    setModel(gridScreenDeleteCancelled);
  }, []);
  /** 行の操作を送らなかった理由（識別子が届いていない・位置を写せない）を告知へ出す。 */
  const refuseRowOperation = useCallback((message: string) => {
    setModel((current) => gridScreenFailed(current, message));
  }, []);
  /**
   * 貼り付けの 1 往復の結果（8.7。要件 7.3、7.4、1.7）。
   *
   * **非同期の結果である**（`ScreenBoundary` は効果の同期の例外しか捕まえない）。遷移は全域で
   * あり、投げない（`gridScreenPasteSettled`）。
   */
  const settlePaste = useCallback(
    (settlement: PasteSettlement) => {
      const state = model.state;
      setModel((current) => gridScreenPasteSettled(current, settlement));
      // 貼り付けも**行を補充しうる**ので、行の操作と同じ判断を通る（要件 7.4、8.7）。
      if (
        state.status === "ready" &&
        settlement.status === "applied" &&
        needsViewRefresh({
          view: state.view,
          outcome: settlement.outcome,
          sheetRowsBefore: state.summary.row_count,
        })
      ) {
        sendView((view) => view);
      }
    },
    [model, sendView],
  );

  /**
   * 取り消し・やり直しの 1 往復の結果（8.9。要件 9.2、9.3、9.8）。
   *
   * **非同期の結果である**（`ScreenBoundary` は効果の同期の例外しか捕まえない）。遷移は全域で
   * あり、投げない（`gridScreenHistorySettled`）。
   */
  const settleHistory = useCallback(
    (settlement: HistorySettlement) => {
      const state = model.state;
      setModel((current) => gridScreenHistorySettled(current, settlement));
      // 取り消しは**行数を変えうる**（行の追加・削除・複製・貼り付けの補充の逆命令である）ので、
      // 行の操作・貼り付けと同じ判断を通る（要件 8.7）。指定が行を絞っている間は、応答が運ぶ
      // 行数がシートの行数であって可視行数ではない — 数を知る唯一の源は表示の指定の応答である。
      if (
        state.status === "ready" &&
        settlement.status === "applied" &&
        needsViewRefresh({
          view: state.view,
          outcome: settlement.outcome,
          sheetRowsBefore: state.summary.row_count,
        })
      ) {
        sendView((view) => view);
      }
    },
    [model, sendView],
  );

  return (
    <GridScreenView
      model={model}
      client={DEFAULT_CLIENT}
      onRetry={retry}
      onDismissNotice={dismissNotice}
      onDismissEditReport={dismissEditReport}
      onSelectionChange={select}
      onEditStarted={startEdit}
      onEditSettled={settleEdit}
      onNextViolation={goToNextViolation}
      onViolationRead={readViolation}
      onExpansion={expand}
      onDetailOpened={openDetail}
      onDetailEditSettled={settleDetailEdit}
      onDetailClosed={closeDetail}
      onColumnWidth={resizeColumn}
      onColumnMove={moveColumn}
      onView={updateView}
      onRowOperationSettled={settleRowOperation}
      onPasteSettled={settlePaste}
      onHistorySettled={settleHistory}
      onDeleteRequested={requestDelete}
      onDeleteCancelled={cancelDelete}
      onRefused={refuseRowOperation}
    />
  );
}
