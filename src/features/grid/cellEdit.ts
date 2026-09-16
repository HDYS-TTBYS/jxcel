/**
 * セル編集の 1 往復（tasks.md 8.3。data-grid 要件 3.1、3.3、3.4、3.5、3.6、3.7、1.7）。
 *
 * 所有: `settleCellEdit`（design.md「Components and Interfaces → Frontend Layer」の
 * GridScreen が使う、入力手段の 2 つの口と境界の間の 1 枚）。
 *
 * # 何を担うか（**入力手段の 2 つの口はここで終わる**）
 *
 * 入力手段（7.4）が出す口は `commit(text)` と `cancel()` の 2 つだけであり
 * （`CellEditorProps`。design.md が逐語で固定している）、どちらも本 module の 1 つの関数
 * （[`settleCellEdit`]）へ来る。**取消が「何もしない」ことを型と経路の両方で保証する**のが
 * ここを分けた理由である: 取消は境界の口を 1 つも持たず、`GridClient` を触らない経路を通る。
 *
 * | 指示 | 何が起きるか | どの要件か |
 * |---|---|---|
 * | 確定（打たれた文字） | `SetCells` の 1 件として適用し、結果（影響範囲・型強制・違反）を返す | 3.3、3.4、3.5 |
 * | 取消 | **何も送らない**。値は画面の側で元へ戻る（文書は 1 バイトも変わらない） | 3.6 |
 *
 * # 打たれた文字は解釈しない（**値を作るのは `schema-engine` である**）
 *
 * 確定が運ぶのは**打たれた文字そのもの**である（`GridCellEdit.text` の doc）。数値へ読むのも
 * 日付として読むのも適合を判定するのも Rust 側であり（`grid_apply_edit` の docs
 * 「適合しない値も破棄せず違反として返す」）、**本 module は適合しないから送らないという判断を
 * しない**（要件 3.5 の「破棄せずに保持」はこの経路で満たす）。
 *
 * **値なしの表現は空の文字列である**（要件 3.7）。`crates/data-grid/src/edit/mod.rs` の
 * `edited_value` がそれを `CellValue::Null` へ写す — **本 module は「値なし」という別の表現を
 * 作らない**（面が空の文字列を確定すれば、そのまま値なしになる）。
 *
 * # 宛先は文書の位置である（**可視行の序数ではない**）
 *
 * `GridCellAddress` が持つのは**行の識別子（文字列）と列の添字**であり、表示の位置ではない
 * （生成物の doc。要件 8.6「取り違えると別の行を編集する」）。行の識別子は窓にしか無いので、
 * 記憶へ引く（[`WindowCache.rowId`]。**本タスクが足した口である**）。引けなければ
 * **送らない** — 推測した識別子で別の行へ書くより、理由を返して利用者に待ってもらう方が正しい。
 *
 * 列は**表示の位置ではなく、記憶が答える文書の列**である（[`WindowCache.documentColumn`]）。
 *
 * **恒等が崩れるのは 2 つの出来事である**（8.3 のレビューが実測）。① **8.8 の列順**（利用者が
 * 列を運ぶ）② **8.5 の入れ子の展開** — 展開すると `ColumnDescriptor` の並びは
 * **文書の列の添字と一致しなくなる**（`crates/data-grid/src/view/mod.rs` の
 * `push_column`。展開した `Object` の内側の位置が文書の同じ列の下へ並ぶ）。**8.5 が②を閉じた**:
 * 写像は `./columnSpace` の 1 つであり、窓の読み（`WindowCache.getCell`）と本 module の宛先が
 * 同じ値を引く（`windowCache.test.ts` / `cellEdit.test.ts` が**離れた構成で**固定する）。①（8.8）
 * は `RendererSpec.onColumnMove` を結線するときに**同じ 1 つへ揃える**こと — 表示の並びの変更は
 * 窓の中身を変えない（列幅・列順は `DisplayState` に閉じる）ので、揃えるのは `getCell` へ渡す
 * 位置の側である。
 *
 * # 適用のあとの表示の作り直し（要件 1.7）
 *
 * 適用が成功したら、**影響を受けた行の窓を捨てる**（[`EditOutcome.affected`] → 7.3 の
 * `invalidate`）。窓を捨てれば記憶が取り直し、到着の通知が `RendererHandle.invalidate` を
 * 呼ぶ（8.1 が組んだ `onArrival` の結線）— すなわち本 module は**捨てるところまで**を担い、
 * 描き直させるのは 7.3 の記憶と移植口の間の既存の結線である。行数を変える命令（6.2 の他の
 * 5 つ）は `WindowCache.clear` を要するが、**本タスクの経路（`SetCells`）は行数を変えない**
 * （8.6 / 8.7 / 8.9 の担当）。
 *
 * # 文書を変えるのは適用の 1 命令だけである（取り消しの作り方）
 *
 * 取り消し（要件 3.6）は**本 module では何もしない**。文書を変えたのは適用だけであり、
 * 取消の時点で文書はまだ変わっていない（適用の前である）— 画面は入力手段を閉じ、窓の記憶を
 * 捨てない。したがって「取消で値が戻る」は**画面の側の 1 つの事実**（編集を開いた時点の値を
 * もう一度描く）から出る。
 */
import type { GridEditOutcome } from "../../ipc/bindings";
import { describeIpcError } from "../../ipc/client";
import type { EditCarrier } from "./editorRegistry";
import type { GridClient } from "./gridClient";
import type { CellPosition } from "./renderer/port";
import type { WindowCache } from "./windowCache";

/**
 * 入力手段から上がる指示。**2 つしかない**（`CellEditorProps` の 2 つの口そのもの）。
 *
 * 打たれた文字は**確定の腕だけ**が運ぶ（取消は文字を持たない — 持たせると、取消のときに
 * その文字をどうするかを決める余地が生まれる）。
 */
export type CellEditIntent =
  | { readonly kind: "commit"; readonly text: string }
  | { readonly kind: "cancel" };

/**
 * 1 往復の結果。**画面が状態を決めるのに要るものだけ**を持つ。
 *
 * `applied` の `outcome` が `null` でありうるのは生成物の型がそう定めているからである
 * （`GridEditResponse.outcome` の doc: `None` は「進める履歴が無かった」場合であり、適用では
 * つねに `Some`。`grid_history` のための腕である）。本 module は**その腕でも壊れない**ように
 * 扱う（何も提示せず、何も捨てない）。
 */
export type CellEditSettlement =
  | { readonly status: "cancelled" }
  | {
      readonly status: "applied";
      readonly outcome: GridEditOutcome | null;
      /**
       * **応答を組み立てた時点の世代**（10 進の文字列。`GridEditResponse.generation` そのもの）。
       *
       * 画面はこれを採用するだけである — 進み方を `outcome.affected` の空・非空から推し量る
       * 規則（かつての `generationAfterEdit`）は持たない（規則が 2 つあると、片方だけが正しい
       * まま残る。design.md「世代を進めるのは境界である」）。
       */
      readonly generation: string;
    }
  | { readonly status: "failed"; readonly message: string };

/**
 * 確定を 1 往復させる（要件 3.3、3.4、3.5、3.6、3.7、5.5、5.7）。
 *
 * **例外を投げない**（`GridClient` の口は封筒の失敗を値で返し、本 module はそれを 1 行へ写す）。
 * 画面の側は `ScreenBoundary` が捕まえない経路（イベントハンドラと非同期）に居るので、
 * 投げないことがそのまま画面の壊れなさになる。
 *
 * 依存を**狭く取る**: 窓の記憶から要るのは**宛先**（行の識別子と、表示の位置が指す文書の列）と、
 * 影響を受けた行を捨てる口の 3 つだけである（`WindowCache` の全体を要求しない — 検査が偽の
 * 実装を置きやすくなる）。
 *
 * # 宛先の列は記憶が答える（8.5）
 *
 * セルの位置は**表示の位置**で届く（移植口の座標であり、選択も同じ空間である）。送る先は
 * **文書の列**であり、入れ子の展開があると 2 つは一致しない。写像を本 module で書くと、
 * 窓の記憶の列の添字と**別の規則**で列を決める経路ができる（片方だけが正しいまま残る）ので、
 * 記憶の 1 つの写像をそのまま使う（[`WindowCache.documentColumn`]）。
 *
 * # 運び手で命令が変わる（8.5。**画面が型で分岐しない**ための口である）
 *
 * `carrier` は**登録**（`./editorRegistry` の `EditCarrier`）が宣言したものであり、本 module は
 * それに従うだけである — `"text"` は打たれた文字を `SetCells` へ、`"structure"` はセル値の
 * 構造表現を `SetNested` へ載せる。**どちらの経路も、取消・判定・違反の保持・取り直しの規律は
 * 同一である**（要件 5.7）— 違うのは運ぶ欄の名前だけである。
 */
export async function settleCellEdit(options: {
  readonly client: GridClient;
  readonly cache: Pick<WindowCache, "rowId" | "documentColumn" | "invalidate">;
  readonly position: CellPosition;
  readonly intent: CellEditIntent;
  readonly carrier: EditCarrier;
}): Promise<CellEditSettlement> {
  if (options.intent.kind === "cancel") {
    // **境界へ 1 つも送らない**（要件 3.6）。取消は「適用しない」ことであり、取り消す操作が
    // 文書へ届いたことは一度も無い。窓の記憶も触らない（表示は変わっていない）。
    return { status: "cancelled" };
  }

  const row = options.cache.rowId(options.position);
  if (row === null) {
    // 窓がまだ届いていない行である（読み込み中）。**推測で書かない。**
    return {
      status: "failed",
      message: "この行の識別子がまだ届いていないため、確定できません",
    };
  }
  const column = options.cache.documentColumn(options.position);
  if (column === null) {
    // 表示の位置が列を指していない（構成の外）。**推測で別の列へ書かない**（要件 8.6）。
    return {
      status: "failed",
      message: "この列の宛先が特定できないため、確定できません",
    };
  }

  const answer = await options.client.applyEdit(
    // **本タスクの経路は 1 セルである。** 範囲へ書く経路は 8.7 の貼り付け（`PasteRange`）で
    // あり、本 module はそれを作らない（作ると、どのセルがどう書かれたかを画面が組み立てる
    // ことになり、表示の並びと文書の位置の写像が 2 箇所に現れる）。
    options.carrier === "structure"
      ? { command: "SetNested", cell: { row, column }, json: options.intent.text }
      : { command: "SetCells", cells: [{ cell: { row, column }, text: options.intent.text }] },
  );
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error) };
  }

  // 影響を受けた行の窓を捨てる（要件 1.7）。`affected` は**重複を畳んだ命令の順**である
  // （生成物の doc）。捨てるだけで足りるのは、値の編集（`SetCells` / `SetNested`）が行の構造
  // （数と並び）を変えないためである — 行数を変える命令は `WindowCache.clear` を要するが、
  // それは 8.6 / 8.7 / 8.9 の経路である。`outcome` が無い（`None`）ときは何も変わっていないので、
  // 捨てるものも無い。
  if (answer.data.outcome !== null) {
    options.cache.invalidate(answer.data.outcome.affected);
  }
  return {
    status: "applied",
    outcome: answer.data.outcome,
    generation: answer.data.generation,
  };
}

// 世代を数える関数は**ここに無い**（タスク 10.1 が消した）。かつての `generationAfterEdit` は
// 「`affected` が空でなければ +1」という `GridSession` の規則の写しだったが、規則が 2 つある
// こと自体が欠陥の温床だった — 展開の適用は 1 つのコマンドの内側で 2 回進むので、数え上げは
// つねにずれる。いまは `GridEditResponse.generation` をそのまま採用する（`GridOpenResponse` /
// `GridViewResponse` も同じ欄を持つ）。
