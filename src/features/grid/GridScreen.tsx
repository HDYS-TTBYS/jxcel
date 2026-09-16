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
 * 手順 4 と 5 のあいだに**世代の整合**がある。境界の型は世代を運ばないので、画面が
 * `GridSession` と同じ規則で数える（開いた直後が 0、`grid_set_view` の成功ごとに +1）。
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
 * | 2.6 確定した選択を複製・貼り付け・削除・取り消しの対象にする | **`ready.selection` がその口である**（8.6 / 8.7 / 8.9 が読む）。本 module は操作そのものを実装しない |
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
 * | 4.2 指定したセルの理由 | 理由の文言 | `grid_find_violation` の `reason`（組み立てるのは適応層） | `./violations` の `reasonInRow` と、表（窓の印と行の識別子を読む） |
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
 * | 段数の上限の誘導（要件 5.4） | 記述の印（`expandability` が `capped`）から「詳細表示へ」を出し、押すと**現在位置の行のその列**の詳細表示を開く | `./nestedInspector` の `nestedColumnControls` と `gridScreenDetailOpened` |
 * | 詳細表示（要件 4.5、5.5、5.6） | 値を読むのは**表**（窓の記憶を持つ側）であり、開いている位置は状態が持つ。**同じ遷移でセルの編集と同一の規律**（`gridScreenDetailEditSettled` が `gridScreenEditSettled` を呼ぶ） | `GridSurface` の中の `NestedInspector` |
 * | 世代（8.5 が足した是正） | `grid_set_view` の成功と、影響を伴う適用で進める。**組み直さずに**記憶へ下ろす | `ready.generation` と `GridSurface` の効果 |
 *
 * **要件 5.5 の「構造の全体」と、要件 5.1 / 5.2 の見える結果は、境界に読む口が無いため届かない**
 * （`design.md`「8.5 が確定させたもの」の申し送り 1・2）。本 module はその事実を利用者にも示す
 * （詳細表示は「宣言が読めません」と書き、展開は指定を送るが**描かれる列は変わらない**）。
 *
 * # 8.3〜8.9 への申し送り（本 module が足す予定の場所）
 *
 * - **8.5〜8.9**: 移植口の 4 つの**操作**（`onColumnResize` / `onColumnMove` / `onCopy` /
 *   `onPaste`）を実装で置き換える（8.4 はこの 4 つに触っていない）。8.1 はそれらを
 *   `onUnavailable` へ流すだけである
 *   （**黙って何もしない実装にしない** — `onCopy` が空文字を返せばクリップボード
 *   が空になり、`onPaste` が黙って捨てれば貼り付けが消える。無反応より悪い）
 * - **8.4（違反の提示）**: 実装済みである（上の「8.4 が確定させたもの」）。本 module が受け取る
 *   `violation_total` は 6.2 の時点で既に**シート全体**の数であり、広げる作業は無かった
 * - **8.6（行の増減）**: 行数が変わったら `WindowCache.clear(rowCount)`（7.3 の申し送り）。
 *   `SetCells` は行数を変えないので、本 module の経路では要らない
 * - **8.8（列幅・列順）**: 列幅と表示上の列順は `createDisplayState`（7.5）が持つ。変化は
 *   **次の `mount` の仕様**に載せる（`RendererHandle` に幅や順を押し込む口は無い。7.2 の申し送り）。
 * - **列の添字の恒等が崩れるのは 2 つである（8.5 が 1 つ目を閉じた）**: 崩すのは ① 8.8 の列順
 *   ② 入れ子の展開であり、**②は 8.5 が閉じた**（写像は `./columnSpace` の 1 つであり、窓の読み
 *   `WindowCache.getCell` と編集の宛先 `WindowCache.documentColumn` が同じ値を引く）。①（8.8）
 *   は**同じ 1 つへ揃える**こと — 列の並びの変更は窓の中身を変えないので、揃えるのは `getCell`
 *   へ渡す位置の側である（8.8 の担当）
 */
import {
  useCallback,
  useEffect,
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
  GridSheetSummary,
  GridViolationLocation,
  GridViewSpec,
} from "../../ipc/bindings";
import {
  generationAfterEdit,
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
import { createDisplayState } from "./displayState";
import { columnEditor } from "./editors";
import type { ColumnConstraints, EditCarrier } from "./editorRegistry";
import { createColumnSpace } from "./columnSpace";
import {
  NestedColumnControls,
  NestedInspector,
  declaredInnerPositions,
  withExpansion,
} from "./nestedInspector";
import { WINDOW_ROWS, createWindowCache, type WindowCache } from "./windowCache";
import { createGlideAdapter } from "./renderer/glideAdapter";
import {
  followTarget,
  initialSelection,
  selectionAt,
  selectionCounts,
  selectionForKey,
} from "./selection";
import type {
  CellPosition,
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
       * `GridSession` が持つ**世代**の写し（8.5。**境界は世代を運ばない** — 7.3 の申し送り）。
       *
       * 進むのは 2 つの出来事である: ① `grid_set_view`（つねに +1。`api.rs` の `set_view`）、
       * ② 適用（影響を受けた行があるときだけ +1。同 `apply`。規則は `./cellEdit` の
       * [`generationAfterEdit`]）。**写さないと、以後の窓の要求が古い世代を名乗り、Rust 側が
       * 空の窓を返す**（`WindowCodec::is_stale`）— 取り直した窓は永久に読み込み中のままになる。
       *
       * 開いた直後は 1 である（8.1 が開く流れで `grid_set_view` を 1 度呼ぶ）。
       */
      readonly generation: number;
      /**
       * 開いている詳細表示（8.5。`null` なら開いていない）。**この腕が持つ**ことが、描かれて
       * いる表のセルにしか詳細表示が無いことを型で表している。
       *
       * 状態に載せる理由は、**入れ子の詳細が表の面の中に描かれる**ためである（窓の記憶を持つのは
       * 表であり、値を読むにはそこが要る）。
       */
      readonly detail: CellDetail | null;
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
    state: { ...model.state, selection: next, violation: moved ? null : model.state.violation },
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
        state: stateAfterEdit(closed.state, settlement.outcome),
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
 * 世代もここで進める（8.5）。規則は `./cellEdit` の [`generationAfterEdit`] であり、**適用が
 * 行の値を変えたときだけ**進む（`api.rs` の `apply` と同じ）。進めないと、以後の窓の要求が
 * 古い世代を名乗り、**取り直した窓が永久に読み込み中のまま**になる。
 *
 * 結果が無い（`None`。`grid_history` の腕）ときは**何も動かさない** — 適用していないので、
 * 総数を変える根拠が無い（生成物の doc が「適用では `None` を取り得ない」と定めている）。
 */
function stateAfterEdit(state: GridScreenState, outcome: GridEditOutcome | null): GridScreenState {
  if (state.status !== "ready" || outcome === null) {
    return state;
  }
  return {
    ...state,
    violationTotal: outcome.violation_total,
    violation: null,
    generation: generationAfterEdit(state.generation, outcome),
  };
}

// ===========================================================================
// 2.5 入れ子の展開と詳細表示（8.5。要件 4.5、5.1〜5.7）
// ===========================================================================

/**
 * 表示の指定を送った結果（**画面が状態を決めるのに要るものだけ**）。
 *
 * 成功の腕は**送った指定をそのまま持ち帰る** — 状態へ入れるのは「境界が受け取った指定」で
 * なければならない（組み立て直すと、送ったものと入れたものが食い違いうる）。
 */
export type GridViewSettlement =
  | {
      readonly status: "applied";
      readonly view: GridViewSpec;
      /** 可視行数（絞り込みが効けばシートの行数と違う）。窓が覆う行数である。 */
      readonly visibleRows: number;
      /** **シート全体**の違反の総数（要件 4.3）。 */
      readonly violationTotal: number;
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
    visibleRows: answer.data.visible_rows,
    violationTotal: answer.data.violation_total,
  };
}

/**
 * 表示の指定の適用を画面へ反映する（要件 5.1、5.2、5.3）。
 *
 * | 結果 | 何が起きるか |
 * |---|---|
 * | 適用された | **送った指定をそのまま状態へ入れる**（展開の状態がここに住むので、走査では失われない）。可視行数・違反の総数を置き換え、**世代を 1 つ進める** |
 * | 適用できなかった | **状態を 1 つも動かさず**、理由を告知として出す（世代も進めない — 進めると、窓の要求が存在しない世代を名乗る） |
 *
 * 窓の記憶は捨てない（**組み直しもしない**）。表示の指定のうち展開は**窓が運ぶ列を変えない**
 * ので、記憶している窓の内容はそのまま正しい（変わったのは世代だけである — `GridSurface` が
 * 世代を記憶へ下ろす）。並べ替え・絞り込みのように**行の並びを変える**指定は、8.8 が
 * `WindowCache.clear` を伴って足す。
 */
export function gridScreenViewSettled(
  model: GridScreenModel,
  settlement: GridViewSettlement,
): GridScreenModel {
  if (model.state.status !== "ready") {
    return model;
  }
  switch (settlement.status) {
    case "applied":
      return {
        attempt: model.attempt,
        state: {
          ...model.state,
          view: settlement.view,
          visibleRows: settlement.visibleRows,
          violationTotal: settlement.violationTotal,
          // 世代は**進めることだけが契約である**（`api.rs` の `set_view` はつねに +1 する）。
          generation: model.state.generation + 1,
          // 詳細表示は**開いたままにする**（構成が変わったことを理由に閉じる理由が無い）。
        },
        notice: model.notice,
        editReport: model.editReport,
      };
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
  const derived = await client.setView(EMPTY_GRID_VIEW);
  if (derived.status === "error") {
    return { status: "failed", message: describeIpcError(derived.error), canRetry: true };
  }
  return {
    status: "ready",
    sheet: sheet.id,
    summary,
    // **窓が覆うのは可視行である**（窓の区間は可視行の序数で表される。`RowSpan` の doc）ので、
    // 記憶へ渡す行数はシートの行数ではなく応答の可視行数である。
    visibleRows: derived.data.visible_rows,
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
    // いま `grid_set_view` を 1 度呼んだところである（世代は 0 から 1 へ進んだ）。
    generation: GENERATION_AFTER_OPEN,
    // 開いた直後は詳細表示を開いていない（要件 5.5。開くのは利用者の操作である）。
    detail: null,
  };
}

// ===========================================================================
// 3. 移植口へ渡す仕様（**キャンバスを要さない純粋な部分**）
// ===========================================================================

/**
 * まだ結線していない操作の名前（**利用者に見える語**である）。8.4〜8.9 がそれぞれ実装したら、
 * その名前はここから落ちる（8.3 が「セルの編集の起動」を実装したので、それはもう無い）。
 */
const OPERATION_NAMES = {
  columnResize: "列の幅の変更",
  columnMove: "列の位置の変更",
  copy: "選択の範囲の複製",
  paste: "表形式のテキストの貼り付け",
} as const;

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
 * 残る 4 つは**操作**であり（8.4〜8.9 の担当）、本 module はそれらを [`onUnavailable`] へ流す。
 * **黙って何もしない実装にしない**理由は 2 つある: `onCopy` が空文字を返せばクリップボードが
 * 空になり、`onPaste` が黙って捨てれば貼り付けが消える（どちらも無反応より悪い）。拒否（`Promise`
 * の失敗）にしておくのは、移植口の実装が**クリップボードへ書かず・適用もしない**ためである
 * （`glideAdapter.tsx` の `GlideSurface` は拒否を記録して描画を止めない）。
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
  readonly onUnavailable: (operation: string) => void;
}): RendererSpec {
  const refuse = (operation: string): Error => {
    options.onUnavailable(operation);
    return new Error(operation);
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
    onColumnResize: () => {
      refuse(OPERATION_NAMES.columnResize);
    },
    onColumnMove: () => {
      refuse(OPERATION_NAMES.columnMove);
    },
    onCopy: () => Promise.reject(refuse(OPERATION_NAMES.copy)),
    onPaste: () => Promise.reject(refuse(OPERATION_NAMES.paste)),
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
  /** 可視行の数（窓が覆う行数）。 */
  readonly visibleRows: number;
  /**
   * いまの世代（8.5。`grid_set_view` と適用で進む）。
   *
   * **依存には入れない**（入れると、編集のたびに器を組み直して走査の位置を失う）。組み立ての
   * 時点の値として読み、以後の変化は専用の効果が記憶へ下ろす。
   */
  readonly generation: number;
  /** 選択（現在位置と矩形）。**画面の状態が持つ唯一の値である**（写しをここに作らない）。 */
  readonly selection: RendererSelection;
  /** 編集中のセル（要件 3.1）。`null` なら編集していない。 */
  readonly editing: CellEdit | null;
  /** 開いている詳細表示（8.5。要件 5.5）。`null` なら開いていない。 */
  readonly detail: CellDetail | null;
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
  /** 器が捕まえない失敗を画面内へ流す口。 */
  readonly onUnavailable: (operation: string) => void;
}

/**
 * 開いた直後に 1 度だけ `grid_set_view` を呼んだあとの世代。
 *
 * 画面は `GridSession` と同じ規則で世代を数える（**境界の型は世代を運ばない**ため。design.md
 * 「世代を進めるのは画面である」）: 開いた直後が 0（`Generation::FIRST`）であり、
 * `grid_set_view` の成功ごとに 1 つ進む（8.5 は**適用でも進む**ことを足した。
 * `./cellEdit` の `generationAfterEdit`）。**食い違うと窓はつねに空になる**（Rust 側は一致
 * しない世代へ空の窓を返すため、画面は読み込み中のまま再試行を続ける）。
 *
 * 以後の世代は [`GridScreenState`] の `generation` が持ち、この定数は**開いた直後の値**だけを
 * 表す。
 */
const GENERATION_AFTER_OPEN = 1;

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
 */
export function createGridSurfaceCache(options: {
  readonly sheet: string;
  readonly summary: GridSheetSummary;
  readonly visibleRows: number;
  /** いまの世代（`grid_set_view` と適用で進む）。 */
  readonly generation: number;
  readonly client: GridClient;
  /** 窓が記憶に入ったときの通知（区間は実際に記憶した範囲）。 */
  readonly onArrival?: (span: RowSpan) => void;
}): WindowCache {
  return createWindowCache({
    sheet: options.sheet,
    // 構成の並びがそのまま表示の位置の空間である（入れ子の展開を含む）。
    columns: createColumnSpace(options.summary.columns),
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
 * 応答は捨てられる）。**列の並びは表示状態（7.5）から組む**ので、8.8 が列幅・列順を変えたときは
 * 新しい仕様でマウントし直すことになる（`RendererHandle` に幅や順を押し込む口が無い）。
 *
 * 効果は 4 つである: ① 器の組み立て（依存はシートと列の構成と可視行数だけ — **選択を依存に
 * 入れない**。入れると打鍵のたびに器を組み直し、React の根と Glide の部品を作り直して走査の
 * 位置を失う）、② 選択を移植口へ下ろし、必要なら追随させる（依存は選択だけ）、③ **世代を
 * 記憶へ下ろす**（8.5。組み直さない — 適用も表示の指定の変更も世代を進めるので、組み直すと
 * 走査の位置まで失われる）、④ 現在位置の違反の理由を引く（要件 4.2）。
 *
 * **詳細表示（8.5）もここが描く。**値を読むには窓の記憶が要り、記憶を持つのは表だからである
 * （8.4 が違反の理由を読むのと同じ理由）。開いている位置そのものは画面の状態（`detail`）が持つ
 * — そうしないと、開いていることを検査から観測できない。
 */
function GridSurface({
  sheet,
  summary,
  visibleRows,
  generation,
  selection,
  editing,
  detail,
  client,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onDetailEditSettled,
  onDetailClosed,
  onViolationRead,
  onUnavailable,
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
    void reasonInRow({ client, current: position, rowId: cache.rowId(position) }).then((reading) => {
      if (token !== violationTokenRef.current) {
        return;
      }
      onViolationRead(reading);
    });
  };

  // 表の大きさ（現在位置を寄せる先。要件 2.2 の端の扱いと、行・列の全体の選択に要る）。
  const bounds = { rowCount: visibleRows, columnCount: summary.columns.length };

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

    // 表示状態（7.5）。8.1 は初期の並び（宣言の順・既定の幅）だけを組む。
    const display = createDisplayState({ columnCount: summary.columns.length });
    // 開いた直後に見えている区間の見当。**実装が知らせてくるまでの値である**（Glide は
    // マウントの直後に本当の区間を知らせる）。
    const openingSpan: VisibleSpan = {
      rows: { start: 0, count: Math.min(visibleRows, WINDOW_ROWS) },
      columns: { start: 0, count: summary.columns.length },
    };
    visibleRef.current = openingSpan;

    const cache = createGridSurfaceCache({
      sheet,
      summary,
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

    const handle = GRID_RENDERER_PORT.mount(
      container,
      createGridRendererSpec({
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
          cache.setVisibleSpan(span.rows);
        },
        onUnavailable,
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
  }, [sheet, summary, visibleRows, onUnavailable]);

  /**
   * **世代を記憶へ下ろす**（8.5）。**組み直さない** — `grid_set_view` も適用も世代を進めるので、
   * 組み直すと移植口を作り直すことになり、走査の位置（表示範囲）と Glide の部品まで失われる。
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
  const detailColumn = detail === null ? null : (summary.columns[detail.position.column] ?? null);
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
      columnEditor(summary.columns[position.column] ?? null).carrier,
      onDetailEditSettled,
    );
  };

  return (
    // 窓の到着の回数を属性にも出す（**描き直しを起こした数の観測**であり、`arrivals` を使う
    // 唯一の場所である）。
    <div data-window-arrivals={arrivals} style={SURFACE_STYLE}>
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
        {`現在位置 ${String(selection.current.row + 1)} 行 ${String(selection.current.column + 1)} 列 ／ 選択 ${String(counts.rows)} 行 × ${String(counts.columns)} 列 = ${String(counts.cells)} セル`}
      </p>
      {editing === null ? null : (
        <CellEditorPanel
          edit={editing}
          column={summary.columns[editing.position.column] ?? null}
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
          declared={
            detailColumn === null ? [] : declaredInnerPositions(summary.columns, detailColumn.column)
          }
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
 * 編集中の 1 セルの面（要件 3.1）。**入力手段を登録簿から引く唯一の場所である。**
 *
 * 型ごとの分岐はここに 1 つも無い — 解決するのは `CellEditorRegistry.resolve` であり、未登録の
 * 札は登録簿が既定（値をそのまま扱う面）へ落とす（要件 10.3、10.4）。**本 module は入力手段の
 * 成分を名指ししない**（`editorRegistry.test.ts` が源の走査で固定する）。
 *
 * 面は**表の器の外**に出る。移植口に「セルの上へ DOM を重ねる」口は無く（`RendererSpec` に
 * そんな欄は無い）、覆われたセルを探させるより、**どのセルを編集しているかを名乗る**方が読める。
 */
function CellEditorPanel({
  edit,
  column,
  onCommit,
  onCancel,
}: {
  readonly edit: CellEdit;
  /** その列の宣言（表示位置の列）。使用不能な列は `null`（札が読めない）。 */
  readonly column: ColumnDescriptor | null;
  /** 確定（**運び手つきで上げる**。`./cellEdit` がそれで命令を選ぶ。要件 5.7）。 */
  readonly onCommit: (text: string, carrier: EditCarrier) => void;
  /** 取消（**境界へ何も送らない**。要件 3.6）。 */
  readonly onCancel: (carrier: EditCarrier) => void;
}): ReactElement {
  // 入力手段と、その面が確定する文字の**運び手**（8.5。要件 5.5、10.3）。`columnEditor` が登録簿を
  // 通して 1 度に引く（成分と運び手が同じ 1 件の登録から来る）。
  const { component: Editor, carrier } = columnEditor(column);
  // 札が読めない列は `Any`（値をそのまま扱う面）として登録簿へ来る（7.4 の module doc）。
  const kind = column?.kind ?? "Any";
  /**
   * 入力手段が読む宣言（要件 3.1、3.7）。
   *
   * `kind` だけが境界から来る。`nullable` は**境界に欄が無い**（`ColumnDescriptor` は
   * `column / path / name / kind / element_count / expandability` しか運ばない）ので、
   * **つねに「値なしの道を出す」**を渡す — 道を閉じると、値なしを許す列で値なしへ戻せなくなる
   * （要件 3.7）。値なしを許さない列では、判定がそれを違反として返し、**値は保持される**
   * （要件 3.5）。`choices` / `reference` / `members` も同じく材料が無い（下の申し送り）。
   */
  const constraints: ColumnConstraints = { kind, nullable: true };

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
  onUnavailable,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onNextViolation,
  onViolationRead,
  onExpansion,
  onDetailOpened,
  onDetailEditSettled,
  onDetailClosed,
}: {
  readonly model: GridScreenModel;
  /** 境界の口（表を描く腕が、編集の 1 往復に使う）。 */
  readonly client: GridClient;
  readonly onRetry: () => void;
  readonly onUnavailable: (operation: string) => void;
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
  /** 詳細表示の中の編集の結果（要件 5.7）。 */
  readonly onDetailEditSettled: (settlement: CellEditSettlement) => void;
  /** 詳細表示を閉じる（**値も文書も動かない**）。 */
  readonly onDetailClosed: () => void;
}): ReactElement {
  const state = model.state;
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
            columns={state.summary.columns}
            view={state.view}
            // 詳細表示は**現在位置の行**の値について開く。
            currentRow={state.selection.current.row}
            onExpansion={onExpansion}
            onDetail={onDetailOpened}
          />
          <GridSurface
            sheet={state.sheet}
            summary={state.summary}
            visibleRows={state.visibleRows}
            generation={state.generation}
            selection={state.selection}
            editing={state.editing}
            detail={state.detail}
            client={client}
            onSelectionChange={onSelectionChange}
            onEditStarted={onEditStarted}
            onEditSettled={onEditSettled}
            onDetailEditSettled={onDetailEditSettled}
            onDetailClosed={onDetailClosed}
            onViolationRead={onViolationRead}
            onUnavailable={onUnavailable}
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
  /** 移植口の操作がまだ結線されていないことを知らせる。 */
  readonly onUnavailable: (operation: string) => void;
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
  onUnavailable,
  onSelectionChange,
  onEditStarted,
  onEditSettled,
  onNextViolation,
  onViolationRead,
  onExpansion,
  onDetailOpened,
  onDetailEditSettled,
  onDetailClosed,
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
        onUnavailable={onUnavailable}
        onSelectionChange={onSelectionChange}
        onEditStarted={onEditStarted}
        onEditSettled={onEditSettled}
        onNextViolation={onNextViolation}
        onViolationRead={onViolationRead}
        onExpansion={onExpansion}
        onDetailOpened={onDetailOpened}
        onDetailEditSettled={onDetailEditSettled}
        onDetailClosed={onDetailClosed}
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
  const noteUnavailable = useCallback((operation: string) => {
    // **器に届かない失敗である**（イベントハンドラ。`ScreenBoundary` は捕まえない）。内容の
    // 領域は変えず、告知として 1 行出す。
    setModel((current) => gridScreenFailed(current, `この操作はまだ使えません: ${operation}`));
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
    void nextViolation({
      client: DEFAULT_CLIENT,
      current: state.selection.current,
      rowCount: state.visibleRows,
    }).then((reading) => {
      if (token !== traversalRef.current) {
        return;
      }
      setModel((current) => gridScreenNextViolation(current, reading));
    });
  }, [model]);

  /**
   * 列の展開の操作（要件 5.1、5.2、5.3）。**送るのは「いまの指定に 1 件を足した」完全な記述で
   * ある**（ドメインは要求に現れない展開を折りたたみへ戻す）。
   *
   * 送る途中の指定は [`pendingViewRef`] へ積む — 2 つの押下が続くと、2 つ目は 1 つ目の応答を
   * 待たずに組まれるので、`ready.view` を起点にすると**1 つ目の押下が指定から消える**。
   */
  const expand = useCallback(
    (expansion: GridExpansionState) => {
      const state = model.state;
      if (state.status !== "ready") {
        return;
      }
      const token = (viewRef.current += 1);
      const next = withExpansion(pendingViewRef.current ?? state.view, expansion);
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

  return (
    <GridScreenView
      model={model}
      client={DEFAULT_CLIENT}
      onRetry={retry}
      onDismissNotice={dismissNotice}
      onDismissEditReport={dismissEditReport}
      onUnavailable={noteUnavailable}
      onSelectionChange={select}
      onEditStarted={startEdit}
      onEditSettled={settleEdit}
      onNextViolation={goToNextViolation}
      onViolationRead={readViolation}
      onExpansion={expand}
      onDetailOpened={openDetail}
      onDetailEditSettled={settleDetailEdit}
      onDetailClosed={closeDetail}
    />
  );
}
