/**
 * 範囲の複製と貼り付けの 1 往復（tasks.md 8.7。data-grid 要件 7.1、7.2、7.3、7.4、8.6、8.9、1.7）。
 *
 * # この module が持つもの
 *
 * 1. **複製のテキストの組み立て**（要件 7.1、7.2）— 選択の範囲の**行と列の配置を保った**
 *    表形式テキストである（[`tableText`]）
 * 2. **表形式テキストを読む側の写し**（[`parseTableText`]）— 往復の検査と、**貼り付けの宛先を
 *    決めるのに要る行数**（[`tableTextRowCount`]）のためのものである
 * 3. **複製の判断**（[`planCopy`]）— 選択の範囲を、窓の記憶から読んでテキストへする
 * 4. **貼り付けの判断**（[`planPaste`]）— 錨と、表示されている行の並びを決める
 * 5. **貼り付けの 1 往復**（[`applyPaste`]）— 生成物の `PasteRange` を 1 つ送り、行数が変わった
 *    なら記憶を作り直す
 *
 * 形は 8.6 の `./rowOps` と同じである（**判断は純粋な module が持ち、往復は狭い依存だけを取り、
 * 画面の状態は遷移が持つ**）。違うのは、移植口の 2 つの口（`onCopy` / `onPaste`）が `Promise` を
 * 返すため、往復が表の面ではなく移植口の仕様を組む側から起動されることである（`GridScreen` の
 * `createGridRendererSpec`）。
 *
 * # 境界の項目（**正直に書く**。8.7 が越えられなかった 1 つ）
 *
 * ① **複製のテキストを書く側はドメインに在るが、画面からは到達できない。**
 * `crates/data-grid/src/edit/paste.rs`（3.4）は読む側（`PasteCodec::parse`）と書く側
 * （`PasteCodec::write`）の**両方**を持ち、規則の正典はその module docs の表である。しかし
 * **境界の 6 つのコマンド（`crates/app-shell/src/ipc/command_names.rs`）に「範囲を読む」口が
 * 無い** — `grid_rows_window` が運ぶのは窓の生バイトであり、`PasteCodec::write` を呼ぶ経路は
 * 1 つも無い（実測: `grep -rn "PasteCodec" crates/data-grid/src/` は `edit` 層の内側と
 * `lib.rs` の再輸出だけを返す）。したがって本 module は**規則の写しを持つ**:
 * [`tableText`] は `PasteCodec::write` の写しであり、[`parseTableText`] は `PasteCodec::parse`
 * の写しである。**正典は `paste.rs` のままであり**、写しが正典から離れると往復が壊れる
 * （`clipboard.test.ts` が往復を固定し、実測の突き合わせを research.md に記録してある）。
 * この写しを消せるのは境界に「範囲の値を表形式テキストで返すコマンド」を足す仕事であり、
 * **所有は 7.1 / 7.2 の実装（境界の側）**である — 8.7（画面）はそれを足せない。
 *
 * ② **メニューからの貼り付けは結線していない**（**複製のほうは 8.7 が結線した**）。障碍は
 * **クリップボードを読む経路が無いこと**である — 本アプリの読み口は DOM の `paste` イベントだけ
 * であり（7.2 の設計）、メニューの活性化には `ClipboardEvent` が無い。`tauri-plugin-clipboard-manager`
 * は依存に無く（`src-tauri/Cargo.toml`）、`navigator.clipboard.readText()` は 7.2 の実起動で
 * **`不可`**（権限の拒否）と記録されている。**読み口が無いまま項目を登録すると、`Ctrl+V` が
 * 基盤のメニューに取られて DOM の `paste` が届かなくなり、いま動いている打鍵の貼り付けを
 * 壊す** — だから登録しない（`design.md` の「貼り付けの項目を今 登録しない理由」）。
 * 複製の側は**打鍵とメニューの双方が同じ入口**（`RendererHandle.copySelection`）へ着く形で
 * 結線した（下の [`createClipboardSurface`] と `clipboardRequests.ts`）。**誰が埋めるか**は
 * research.md「メニューからの実行」に記録してある。
 *
 * # 座標空間（要件 8.6、8.9 の取り違え）
 *
 * | 値 | 空間 | どこから引くか |
 * |---|---|---|
 * | `PasteRange.anchor.row` | **物理の行（`RowId`）** | `WindowCache.rowId`（表示の序数から。引けなければ送らない） |
 * | `PasteRange.anchor.column` | **文書の列** | `WindowCache.documentColumn`（表示の位置から。展開の下では食い違う） |
 * | `PasteRange.rows` | **表示されている行の並び**（`RowId`） | 錨の可視行から下へ、矩形の行数ぶんだけ |
 * | `PasteRange.text` | 表形式テキスト | クリップボードの生の文字列（**1 バイトも変えない**） |
 *
 * **8.6 の「挿入の位置を写せない」制約は貼り付けには掛からない。**挿入が写せないのは
 * `InsertRows.at` が**文書の位置**（行順の添字）であり、可視の序数から文書の位置への写像が
 * 境界に無いためである。貼り付けが渡すのは**行の識別子**（表示の序数から引ける）と**表示の
 * 並び**そのものであり、どちらも並べ替え・絞り込みの下で成り立つ（8.6 が削除・複製の対象を
 * 識別子で決めたのと同じ理由）。**隠れている行には 1 セルも書かれない** — 並びに現れないから
 * である（要件 8.9。`crates/data-grid/src/edit/mod.rs` の `paste_range_with_inverse`）。
 *
 * # 行の補充（要件 7.4）と、行数の写しが要る理由
 *
 * ドメインは「矩形の行数から、表示の並びの錨から先に残る行数を引いたもの」を行の補充として
 * **文書の末尾へ足す**。したがって画面が渡す並びが**短すぎると、ドメインは既存の行へ書かずに
 * 行を足す**（利用者から見れば、貼り付けたはずの行が増えて元の行が空のまま残る）。逆に
 * **長すぎる分は無害である**（矩形の行数しか書かない）。正しい境目は「矩形の行数」と
 * 「錨から先に残る可視行数」の小さい方であり、前者を画面が知るために
 * [`tableTextRowCount`] が要る（表形式テキストの行数は**囲みの中の改行**を数えないので、
 * `split("\n")` では出せない）。
 *
 * # 単体テストが観測しないもの（**正直に書く**）
 *
 * ① **文書へ実際に値が書かれること**と、補充される行の既定値は Rust 側の契約である
 * （`crates/data-grid` の検査）。本 module は「何を送ったか」までしか主張しない。
 * ② **クリップボードとの往復**（要件 7.2 の後半。他の表計算アプリケーションとの間で範囲を
 * 往復できること）は、**実機のクリップボードと他のアプリケーションを要する**ので、
 * `node` の環境（DOM なし）では観測できない。7.2 のレビューは、システムのクリップボードの
 * 読み戻しがこの観測環境では確かめられないことを記録している。観測の場所は実起動（9.2 の
 * 台本と `smoke-port-probe`）と人手の手順である。
 * ③ **メニューからの実行**（要件 7.8）は結線していない（上の「境界の項目」②）。
 */
import type { GridEditCommand, GridEditOutcome } from "../../ipc/bindings";
import { assertNever, describeIpcError } from "../../ipc/client";
import type { GridClient } from "./gridClient";
import type { CellPosition, CellRange, RenderCell } from "./renderer/port";
import type { WindowCache } from "./windowCache";

// ===========================================================================
// 1. 表形式テキスト（`PasteCodec` の規則の写し。正典は paste.rs）
// ===========================================================================

/**
 * セル値の矩形を表形式テキストへ書く（要件 7.1、7.2。`PasteCodec::write` の写し）。
 *
 * 規則は `crates/data-grid/src/edit/paste.rs` の module docs「規則（正典）」のとおりである:
 *
 * - 行の区切りは `\n`、列の区切りは `\t` である（他の表計算アプリケーションがクリップボードへ
 *   置く綴りそのものである）
 * - 値が `\t` / `\n` / `\r` / `"` のいずれかを含むときは `"` で囲み、囲みの中の `"` を `""` へ
 *   倍にする（`\r` を含む値も囲む — 囲まないと、値の末尾の `\r` と次の行の区切りの `\n` が
 *   繋がって**行が 1 つ増える**）
 * - 空の値は**空の綴り**で書く（`""` と書くと、一般の表では空のセルがすべて `""` になり、
 *   他のアプリケーションへ渡す綴りとして通常と違うものになる）
 * - 空の矩形は空のテキストである（読み直すと行 0 件。既知の限界は `paste.rs` の
 *   module docs「往復」にある）
 *
 * **値は表示文字列である**（画面が窓から読むのは表示文字列だけであり、`PasteCodec::write` も
 * `view` 層の `display_text` を通す）。したがって本関数は値の解釈を 1 つも持たない。
 */
export function tableText(cells: readonly (readonly string[])[]): string {
  let text = "";
  for (let row = 0; row < cells.length; row += 1) {
    if (row > 0) {
      text += "\n";
    }
    const values = cells[row] ?? [];
    for (let column = 0; column < values.length; column += 1) {
      if (column > 0) {
        text += "\t";
      }
      const value = values[column] ?? "";
      if (value.includes("\t") || value.includes("\n") || value.includes("\r") || value.includes('"')) {
        text += `"${value.split('"').join('""')}"`;
      } else {
        text += value;
      }
    }
  }
  return text;
}

/**
 * 表形式テキストを読み、行ごとに `sink` へ渡す（`PasteCodec::parse` の写し）。
 *
 * **規則の正典は `paste.rs` の module docs である**（読む側と書く側の両方がそこに書いてある）。
 * 本関数はその写しであり、分岐の 1 つ 1 つが正典の 1 つ 1 つに対応する:
 *
 * | 入力 | 何が起きるか |
 * |---|---|
 * | `\t` | 次の列へ進む |
 * | `\n` | 次の行へ進む（値の個数は行ごとに違ってよい） |
 * | `\r\n` | 次の行へ進む（**2 文字ぶん進む**。Windows のアプリケーションが置く綴りである） |
 * | 値の先頭の `"` | 囲みを始める（囲みの中では区切りが区切りにならない。`""` は 1 つの `"`） |
 * | それ以外 | 値の文字（`\r` 単独もここへ落ちて値の文字になる） |
 *
 * 空のテキストは行 0 件であり、末尾の行の区切りは行を作らない（`a\n` は 1 行）。連続する行の
 * 区切りは空の行として残る。
 *
 * **1 つの走査を 2 つの用途で共有する**（[`parseTableText`] と [`tableTextRowCount`]）— 規則の
 * 写しを 2 つ持つと、片方だけが正典に追随する日が来る。
 */
function readTableText(text: string, sink: (row: readonly string[]) => void): void {
  if (text === "") {
    return;
  }
  let row: string[] = [];
  let field = "";
  // 値の先頭に居るか（囲みはここでだけ始まる）。
  let atFieldStart = true;
  // 直前の行の区切りの後に何かを読んだか（末尾の区切りが余分な行を作らないための印）。
  let started = false;
  let index = 0;
  while (index < text.length) {
    const character = text[index] ?? "";
    if (character === "\t") {
      row.push(field);
      field = "";
      atFieldStart = true;
      started = true;
      index += 1;
      continue;
    }
    if (character === "\n" || (character === "\r" && text[index + 1] === "\n")) {
      row.push(field);
      field = "";
      sink(row);
      row = [];
      atFieldStart = true;
      started = false;
      index += character === "\r" ? 2 : 1;
      continue;
    }
    if (character === '"' && atFieldStart) {
      const read = readQuoted(text, index + 1);
      field += read.value;
      index = read.next;
      atFieldStart = false;
      started = true;
      continue;
    }
    // 区切りはすべて ASCII であるため、UTF-8 の非 ASCII の並び（サロゲート対を含む）が
    // 区切りと一致することはない（`paste.rs` の同じ理由のコメント）。
    field += character;
    index += 1;
    atFieldStart = false;
    started = true;
  }
  if (started) {
    row.push(field);
    sink(row);
  }
}

/**
 * 囲みの中を読み、値と次の位置を返す（`index` は囲みの開始の `"` の次を指す）。
 *
 * `""` は 1 つの `"` として読み、次が `"` でない `"` で囲みを閉じる。閉じないまま入力が終われば
 * **残り全部が値**になる（解釈は全域であり、失敗する腕を持たない。`paste.rs` の module docs
 * 「囲みの解釈」）。
 */
function readQuoted(text: string, index: number): { readonly value: string; readonly next: number } {
  let value = "";
  let at = index;
  while (at < text.length) {
    if (text[at] === '"') {
      if (text[at + 1] === '"') {
        value += '"';
        at += 2;
        continue;
      }
      // 囲みの終わり。閉じる `"` は値に入れない（その後に続く文字は値へ足される）。
      return { value, next: at + 1 };
    }
    value += text[at] ?? "";
    at += 1;
  }
  return { value, next: at };
}

/**
 * 表形式テキストをセルの矩形として読む（`PasteCodec::parse` の写し）。
 *
 * **本番の経路は使わない**（貼り付けは生の文字列をそのままドメインへ渡す — 解釈は 1 箇所で
 * あるべきである）。これは**往復の検査**（本 module の書き出しを、正典と同じ規則で読み直す）と、
 * 実測の突き合わせ（research.md「複製のテキストの往復」）のための口である。
 */
export function parseTableText(text: string): string[][] {
  const rows: string[][] = [];
  readTableText(text, (row) => {
    rows.push([...row]);
  });
  return rows;
}

/**
 * 表形式テキストが作る行数（＝ `PasteCodec::parse` が返す矩形の行数）。
 *
 * 貼り付けの宛先を決めるのに要る（上の module doc「行の補充と、行数の写しが要る理由」）。
 * `split("\n")` で代用してはならない — 囲みの中の改行は値の文字であり、行の区切りではない。
 */
export function tableTextRowCount(text: string): number {
  let rows = 0;
  readTableText(text, () => {
    rows += 1;
  });
  return rows;
}

// ===========================================================================
// 2. 複製の計画（要件 7.1、7.2）
// ===========================================================================

/**
 * 複製の判断の結果。
 *
 * | 腕 | 画面は何をするか |
 * |---|---|
 * | `text` | 移植口へ返す（器がクリップボードへ書く。`glideAdapter.tsx` の `copySelection`） |
 * | `refused` | **返さずに**理由を告知へ出す（空文字を返せば、利用者には「複製できた」と見える） |
 */
export type CopyPlan =
  | { readonly kind: "text"; readonly text: string }
  | { readonly kind: "refused"; readonly message: string };

/**
 * 選択の範囲を表形式テキストへする（要件 7.1、7.2）。
 *
 * **配置は表示の位置で決まる**（移植口が渡す `CellRange` は表示の位置である）。値は窓の記憶から
 * 引く（`getCell` は同期であり投げない）。**窓が届いていないセルが 1 つでもあれば複製しない** —
 * `RenderCell.loading` を空文字として書くと、利用者は空のセルを複製したことになり、他の
 * アプリケーションへ渡った内容が**静かに違う**（8.6 が「識別子が引けなければ送らない」のと
 * 同じ規律である。推測で書かない）。
 */
export function planCopy(options: {
  /** 選択の矩形（表示の位置。**正規化は前提にしない** — 本関数が自分で正規化する）。 */
  readonly range: CellRange;
  /** セルを引く口（窓の記憶の `getCell` をそのまま渡せる）。 */
  readonly cell: (position: CellPosition) => RenderCell;
}): CopyPlan {
  const startRow = Math.min(options.range.start.row, options.range.end.row);
  const endRow = Math.max(options.range.start.row, options.range.end.row);
  const startColumn = Math.min(options.range.start.column, options.range.end.column);
  const endColumn = Math.max(options.range.start.column, options.range.end.column);

  const cells: string[][] = [];
  for (let row = startRow; row <= endRow; row += 1) {
    const values: string[] = [];
    for (let column = startColumn; column <= endColumn; column += 1) {
      const cell = options.cell({ row, column });
      if (cell.loading) {
        return {
          kind: "refused",
          message: "選択の範囲のセルがまだ届いていないため、複製できません",
        };
      }
      values.push(cell.text);
    }
    cells.push(values);
  }
  return { kind: "text", text: tableText(cells) };
}

// ===========================================================================
// 3. 貼り付けの計画（要件 7.3、7.4、8.6、8.9）
// ===========================================================================

/**
 * 境界へ送る貼り付けの内容（**生成物の `PasteRange` の欄そのもの**である）。
 *
 * 生成物の `GridEditCommand` の `PasteRange` と同じ 3 つの欄を持ち、`applyPaste` が
 * `command: "PasteRange"` を足して送る — **写しを 2 つ持たない**（欄が増えれば型検査がここで
 * 落ちる）。
 */
export interface PastePayload {
  /** 貼り付けの起点（**物理の行**と**文書の列**。要件 8.6）。 */
  readonly anchor: { readonly row: string; readonly column: number };
  /** **表示されている行の並び**（錨の可視行から下へ。要件 8.9）。 */
  readonly rows: readonly string[];
  /** 貼り付ける表形式テキスト（**1 バイトも変えない**）。 */
  readonly text: string;
}

/**
 * 貼り付けの判断の材料。**境界も窓の記憶も持たない**（引く口だけである。`./rowOps` と同じ規律）。
 */
export interface PasteContext {
  /** 可視行の数（窓が覆う行数。並びを切る上限である）。 */
  readonly visibleRows: number;
  /** 可視行の序数 → **文書の行の識別子**。**引けなければ `null`**（推測で答えてはならない）。 */
  readonly rowId: (position: CellPosition) => string | null;
  /** 表示の位置 → **文書の列の添字**。答えられなければ `null`。 */
  readonly documentColumn: (position: CellPosition) => number | null;
}

/**
 * 貼り付けの判断の結果。
 *
 * | 腕 | 画面は何をするか |
 * |---|---|
 * | `send` | 境界へ `PasteRange` を 1 つ送る（[`applyPaste`]） |
 * | `refused` | **送らずに**理由を告知へ出す（錨が表の外・識別子が届いていない） |
 * | `nothing` | 何もしない（書くものが無い — 空のテキストである） |
 */
export type PastePlan =
  | { readonly kind: "send"; readonly payload: PastePayload }
  | { readonly kind: "refused"; readonly message: string }
  | { readonly kind: "nothing" };

/**
 * クリップボードのテキストを、錨のセルから始まる矩形として貼り付ける計画を立てる
 * （要件 7.3、7.4、8.6、8.9）。**全域であり、投げない。**
 *
 * 決めるのは 3 つである: ① 錨（物理の行の識別子と文書の列）② 歩く順序（表示されている行の
 * 並びのうち、**矩形が覆う行数ぶんだけ**）③ テキストそのもの（生のまま）。
 *
 * **渡す並びは「矩形の行数」と「錨から先に残る可視行数」の小さい方である。**長く渡しても
 * 無害だが（ドメインは矩形の行数しか書かない）、**短く渡すとドメインが行を補充する** —
 * 利用者から見れば、既存の行へ書かずに行が増える（module doc「行の補充」）。
 */
export function planPaste(
  anchor: CellPosition,
  text: string,
  context: PasteContext,
): PastePlan {
  if (text === "") {
    // **境界へ 1 つも送らない**（ドメインも何も書かない。`unchanged` の往復を作らない）。
    return { kind: "nothing" };
  }
  const rowsInText = tableTextRowCount(text);
  if (rowsInText === 0) {
    return { kind: "nothing" };
  }
  if (anchor.row < 0 || anchor.row >= context.visibleRows) {
    // 起点が表の外である（寄せが効いていれば起きないが、全域にしておく）。
    return { kind: "refused", message: "貼り付けの起点が表の外にあるため、貼り付けできません" };
  }
  const anchorRow = context.rowId({ row: anchor.row, column: 0 });
  if (anchorRow === null) {
    return {
      kind: "refused",
      message: "貼り付けの起点の行の識別子がまだ届いていないため、貼り付けできません",
    };
  }
  const anchorColumn = context.documentColumn(anchor);
  if (anchorColumn === null) {
    return {
      kind: "refused",
      message: "貼り付けの起点の列がこの表に無いため、貼り付けできません",
    };
  }

  // 矩形が覆う行は、表示の並びの錨から下へ進む（要件 8.9）。可視行の末尾を越える分は
  // **ドメインが末尾へ足す**（要件 7.4）ので、ここでは作らない。
  const covered = Math.min(rowsInText, context.visibleRows - anchor.row);
  const rows: string[] = [anchorRow];
  for (let offset = 1; offset < covered; offset += 1) {
    // 列は問わない（行の識別子は行そのものの身元である。`WindowCache.rowId` の doc）。
    const id = context.rowId({ row: anchor.row + offset, column: 0 });
    if (id === null) {
      // **1 つでも引けなければ送らない。**部分的な並びを送ると、ドメインは残りを補充として
      // 末尾へ足し、利用者が見ている位置とは別のところへ書く（8.6 と同じ規律である）。
      return {
        kind: "refused",
        message: "貼り付けの宛先の行の識別子がまだ届いていないため、貼り付けできません",
      };
    }
    rows.push(id);
  }
  return { kind: "send", payload: { anchor: { row: anchorRow, column: anchorColumn }, rows, text } };
}

// ===========================================================================
// 4. 貼り付けの 1 往復（要件 7.3、7.4、1.7）
// ===========================================================================

/** 往復の結果（**画面が状態を決めるのに要るものだけ**）。 */
export type PasteSettlement =
  | {
      readonly status: "applied";
      readonly outcome: GridEditOutcome | null;
      /** **応答を組み立てた時点の世代**（10 進の文字列。`GridEditResponse.generation` そのもの）。 */
      readonly generation: string;
    }
  | { readonly status: "failed"; readonly message: string };

/**
 * 貼り付けの内容を境界へ送り、**行数が変わったなら記憶を作り直す**（要件 7.3、7.4、1.7）。
 *
 * **例外を投げない**（`GridClient` の口は封筒の失敗を値で返し、本 module はそれを 1 行へ写す）。
 * 画面の側は `ScreenBoundary` が捕まえない経路（イベントハンドラと非同期）に居るので、投げない
 * ことがそのまま画面の壊れなさになる。
 *
 * `clear` を呼ぶのは**適用が影響を受けた行を持ったとき**だけである（8.6 の `applyRowOperation`
 * と同じ条件である）。貼り付けは**行を補充しうる**ので、`invalidate`（影響を受けた行の窓を
 * 捨てる）では足りない — 増えた行は永久に読み込み中のままになり、減った先は古い窓のまま配られる
 * （`WindowCache.clear` の doc）。
 */
export async function applyPaste(options: {
  readonly client: GridClient;
  /** 行数の変化のあとに記憶を捨てる口（要件 1.7）。**適用されなかったときは触らない。** */
  readonly cache: Pick<WindowCache, "clear">;
  readonly payload: PastePayload;
}): Promise<PasteSettlement> {
  const command: GridEditCommand = {
    command: "PasteRange",
    anchor: { row: options.payload.anchor.row, column: options.payload.anchor.column },
    rows: [...options.payload.rows],
    text: options.payload.text,
  };
  const answer = await options.client.applyEdit(command);
  if (answer.status === "error") {
    return { status: "failed", message: describeIpcError(answer.error) };
  }
  const outcome = answer.data.outcome;
  if (outcome !== null && outcome.affected.length > 0) {
    // **行数を渡す**（`GridEditResponse.row_count` をそのまま。画面は数え直さない）。
    options.cache.clear(outcome.row_count);
  }
  return { status: "applied", outcome, generation: answer.data.generation };
}

/**
 * 計画の 3 つの腕を、それぞれの行き先へ渡す（**送る腕だけが境界へ行く**）。
 *
 * 移植口の `onPaste` は `Promise<void>` を返す口なので、2 つの受け口も `Promise` を返す —
 * 拒否の腕は告知を上げてから**拒否する**（`createGridRendererSpec` の `refusePromise`）。
 * 8.6 の [`runRowOperationPlan`](./rowOps.ts) と同じ形であり、**腕の取り違えが 1 箇所で
 * 起きる**ようにしてある。
 */
export function runPastePlan(
  plan: PastePlan,
  sinks: {
    readonly send: (payload: PastePayload) => Promise<void>;
    readonly refuse: (message: string) => Promise<never>;
  },
): Promise<void> {
  switch (plan.kind) {
    case "send":
      return sinks.send(plan.payload);
    case "refused":
      return sinks.refuse(plan.message);
    case "nothing":
      // 書くものが無い（空のテキスト）。**往復を起こさない**（起こせば、何も変えない適用の応答が
      // 返るだけである）。移植口から見れば「貼り付けは成功した」である。
      return Promise.resolve();
    default:
      return assertNever(plan, "貼り付けの計画の分岐が網羅されていない");
  }
}

// ===========================================================================
// 5. 画面の面（移植口へ渡す 3 つの口を組む）
// ===========================================================================

/**
 * 画面の貼り付け・複製の面。**移植口へ渡す 3 つの口の組み立てをここ 1 箇所に閉じる。**
 *
 * 画面（`GridSurface`）は器を組み立てる効果の中でこれを 1 つ作り、`createGridRendererSpec` へ
 * そのまま渡す。**材料（窓の記憶を引く口・可視行数）と行き先（境界・往復の結果・適用のあとの
 * 後始末）を引数で受け取り、判断そのものは上の節の関数に委ねる。**
 *
 * # なぜ画面の本体から切り出してあるか（**8.7 のレビューの指摘に答えた形**）
 *
 * 切り出す前は、この 3 つの口が `GridSurface` の関数本体の中にあった。`GridSurface` は React の
 * 部品であり、**`node` 環境の検査からは組み立てられない**（`jsdom` を足していない。
 * `vitest.config.ts` の判断）。そのため次の 3 つの誤りが**どの検査でも捕まらなかった**
 * （実測: 442 件すべてが緑のまま通った）:
 *
 *   1. **表示の列を文書の列として送る**（要件 8.6、8.9 が名指しする取り違え。下の
 *      `documentColumn` を通さず `position.column` を使う）
 *   2. `sendPaste` が**何もしない**（境界へ 1 つも送らない）
 *   3. 往復の結果を**画面へ渡さない**（`onSettled` を呼ばない）
 *
 * 本体を module へ出せば、3 つとも検査で落ちる（`clipboard.test.ts` の「画面の面」の節）。
 * 8.6 が `planRowOperation` を `./rowOps` へ出したのと同じ形であり、8.1 が効果の本体
 * （`followSelection`）を出したのと同じ理由である。
 *
 * # 引数の意味（**引く口だけを受け取り、判断はしない**）
 *
 * - `cache`: 窓の記憶を引く口。**器がまだ無ければ `null` を返す**（そのときは拒否の計画になる。
 *   `GridSurface` の ref をそのまま渡せる）。要る能力は 4 つだけである — 値（`getCell`）、
 *   行の識別子（`rowId`）、文書の列（`documentColumn`）、行数が変わったときの作り直し（`clear`）
 * - `visibleRows`: 窓が覆う可視行数（貼り付けの並びを切る上限。要件 7.4）
 * - `onSettled`: 貼り付けの 1 往復の結果（画面の遷移。**適用されなかったときも呼ぶ** — 失敗の
 *   理由を告知へ出すのは画面である）
 * - `onApplied`: 適用のあとの後始末（要件 4.6 の違反の引き直し）。**適用されたときだけ呼ぶ**
 */
export function createClipboardSurface(options: {
  readonly client: GridClient;
  readonly cache: () => Pick<WindowCache, "getCell" | "rowId" | "documentColumn" | "clear"> | null;
  readonly visibleRows: number;
  readonly onSettled: (settlement: PasteSettlement) => void;
  readonly onApplied: () => void;
}): {
  readonly copyRange: (range: CellRange) => CopyPlan;
  readonly pasteAt: (anchor: CellPosition, text: string) => PastePlan;
  readonly sendPaste: (payload: PastePayload) => Promise<void>;
} {
  return {
    copyRange: (range) => {
      const cache = options.cache();
      if (cache === null) {
        return { kind: "refused", message: "表がまだ描かれていないため、複製できません" };
      }
      // **値を読むのは窓の記憶である**（表示の位置のまま引く。複製は表示の並びを保つ）。
      return planCopy({ range, cell: (position) => cache.getCell(position) });
    },
    pasteAt: (anchor, text) => {
      const cache = options.cache();
      if (cache === null) {
        return { kind: "refused", message: "表がまだ描かれていないため、貼り付けできません" };
      }
      // **宛先は表示の位置ではなく文書の位置である**（要件 8.6、8.9）。行は識別子で、
      // 列は `documentColumn` を通した文書の添字で送る — ここを `position.column` にすると、
      // 入れ子の展開（表示の列と文書の列が食い違う唯一の場面。8.5）で別の列へ書く。
      return planPaste(anchor, text, {
        visibleRows: options.visibleRows,
        rowId: (position) => cache.rowId(position),
        documentColumn: (position) => cache.documentColumn(position),
      });
    },
    sendPaste: (payload) => {
      const cache = options.cache();
      if (cache === null) {
        // 器がまだ無い（描かれていない）。**投げない** — 移植口へ届いた要求に応える先が無い
        // だけであり、`applyPaste` の規律（例外を投げない）と同じである。
        return Promise.resolve();
      }
      return applyPaste({ client: options.client, cache, payload }).then((settlement) => {
        options.onSettled(settlement);
        if (settlement.status === "applied") {
          options.onApplied();
        }
      });
    },
  };
}
