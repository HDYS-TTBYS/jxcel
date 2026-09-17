// このファイルは生成物である。**手で編集しない。**
// ホスト API の関数の宣言は crates/macro-runtime/src/surface/declaration.rs の HOST_APIS が、
// 型の宣言は crates/macro-runtime/src/{types.rs,host/overlay.rs,host/changes.rs} の定義から
// ts-rs が生成する。直すのは生成元である。
//
// 再生成（リポジトリルートで実行する）: cargo run -p macro-runtime --bin generate-macro-types
// 本ファイルは追跡対象である。ドリフト検査（crates/macro-runtime/tests/macro_host_dts_drift.rs）が
// 生成器の出力とバイト比較する。
//
// **本ファイルはモジュールではない**（`import` も `export` も書かない）。書くとファイルが
// モジュールになり、`declare namespace host` がグローバルでなくなるためである。
// マクロはグローバルの `host` の下の API を呼び、型の名前をそのまま書ける必要がある。
// フロントエンドの型検査（tsconfig.json の `include` は `src`）には入らない — 取り込むのは
// 後続の macro-editor-lsp である。

/**
 * マクロから見えるホスト API（要件 4.1–4.6）。
 *
 * マクロのソースでは `host.readRange(sheet, span)` のように呼ぶ。**能力を要する
 * API は、ソースの先頭でその能力を宣言していなければ呼べない**（要件 8.1–8.4）。
 * 宣言は `// @grant file.read, net` の形で書く。
 */
declare namespace host {
  /**
   * `host.sheets` — 能力: 不要（宣言なしで呼べる）。
   */
  function sheets(): SheetInfo[];
  /**
   * `host.columns` — 能力: 不要（宣言なしで呼べる）。
   */
  function columns(sheet: SheetId): ColumnTypeInfo[];
  /**
   * `host.readRange` — 能力: 不要（宣言なしで呼べる）。
   */
  function readRange(sheet: SheetId, span: RowSpan): RowPage;
  /**
   * `host.setCells` — 能力: 不要（宣言なしで呼べる）。
   */
  function setCells(sheet: SheetId, writes: CellWrite[]): void;
  /**
   * `host.insertRows` — 能力: 不要（宣言なしで呼べる）。
   */
  function insertRows(sheet: SheetId, values: CellValue[][]): void;
  /**
   * `host.removeRows` — 能力: 不要（宣言なしで呼べる）。
   */
  function removeRows(sheet: SheetId, rows: RowId[]): void;
  /**
   * `host.duplicateRows` — 能力: 不要（宣言なしで呼べる）。
   */
  function duplicateRows(sheet: SheetId, rows: RowId[]): void;
  /**
   * `host.fileRead` — 能力: `file.read` を宣言したマクロだけが呼べる（`// @grant file.read`）。
   */
  function fileRead(path: string): string;
  /**
   * `host.fileWrite` — 能力: `file.write` を宣言したマクロだけが呼べる（`// @grant file.write`）。
   */
  function fileWrite(path: string, text: string): void;
  /**
   * `host.netFetch` — 能力: `net` を宣言したマクロだけが呼べる（`// @grant net`）。
   */
  function netFetch(url: string): string;
}

// ---------------------------------------------------------------------------
// マクロから見える型（宣言表が要求する型の閉包。出どころは crates/macro-runtime/src/types.rs）

/**
 * セルの値（マクロから見える形）。
 *
 * 素の JS の値と、構造として渡す値（配列・オブジェクト）の合併型である。
 * 整数は 2^53 の内側に限られるため `number` で正確に表せる。
 */
type CellValue = null | boolean | number | string | CellValue[] | { [key: string]: CellValue };

/**
 * 1 セルへの書き込み（マクロ側の `host.setCells(sheet, writes)` の要素）。
 *
 * 列の添字は **0 起点**で、[`ColumnIndex`] が指す位置（`Sheet::columns` の並びに対する
 * 位置）である。行は文書の識別子であり、マクロは**読みで受け取った識別子**をそのまま返す
 * （タスク 2.4 の範囲の読みが `RowId` を渡す）。
 *
 * JS 側の型名 `CellWrite`（要件 4.6 / 10.1）を持つのはこの型であり、タスク 3.3 の生成器
 * （`crate::types`）が宣言表と本型の `ts-rs` の宣言から `.d.ts` へ出す。
 *
 * 上流の型（`RowId` / `ColumnIndex` / `CellValue`）には `ts-rs` の導出を付けない
 * （`ts-rs` の導出は `crates/app-shell/src/ipc/` の内側に限る規約。`ipc-contract.md`）ため、
 * **マクロから見える綴り**はフィールドの `#[ts(type = ...)]` でここに決める。その綴りの
 * 宣言（`type RowId = string;` など）は `crate::types` が 1 箇所で持つ。
 */

type CellWrite = { 
/**
 * 書き込む行（文書の識別子）。マクロから見える形は**文字列**である。
 */
row: RowId, 
/**
 * 書き込む列（0 起点）。マクロから見える形は**数値**である。
 */
column: ColumnIndex, 
/**
 * 書き込む値。マクロから見える形は**セル値の合併型**である。
 */
value: CellValue, };

/**
 * 列の添字（0 起点。`host.columns` が返す並びの位置）。
 */
type ColumnIndex = number;

/**
 * マクロから見た列の宣言 1 列（要件 4.1 の「列の宣言」、4.2 の「宣言された型」）。
 *
 * 型情報（[`TypeKind`]）を持つのは、値の写像（`host/value.rs` の [`to_js`]）が
 * 「この列の型」を必要とするためである（要件 4.2, 4.3）。
 *
 * **制約**（範囲・長さ・書式・選択肢）は渡さない。値が型に適合するかの判定は
 * `schema-engine` の持ち物であり、マクロは判定規則を持たない（design.md「Out of Boundary」
 * の「値の正否の判定」）。違反は適用のときに画面と同じ形で提示される（要件 5.2）。
 *
 * [`to_js`]: crate::host::value::to_js
 */

type ColumnTypeInfo = { 
/**
 * 列名（`RowPage` のセルの並びはこの並びと同じ順である）。
 */
name: string, 
/**
 * 宣言された型。名前付き型定義への参照（`TypeDecl::Ref`）は、アダプタが解決した先の
 * 種別をここへ渡す（`schema-engine` の compile / resolve 層が解決を持つ）。
 *
 * マクロから見える形は**種別の綴りの合併型**である（`"Int" | "Float" | …`。
 * [`TypeKind`] に `ts-rs` の導出が無いため、綴りは `crate::types` が種別カタログ
 * （`TypeKind::ALL`）を走査して 1 箇所で組み立てる）。
 */
kind: TypeKind, 
/**
 * 値なしを許さないか（上流の宣言の `required`）。
 */
required: boolean, 
/**
 * 一意制約（上流の宣言の `unique`）。
 */
unique: boolean, };

/**
 * 範囲の読みで返る 1 行（要件 4.4）。
 */

type ReadRow = { 
/**
 * 行識別子。書き込み（`host.setCells` の `CellWrite`）へそのまま渡せる。
 *
 * マクロから見える形は**文字列**である（[`SheetInfo::id`] と同じ理由）。
 */
id: RowId, 
/**
 * 列の並び順のセル値（[`ColumnTypeInfo`] の並びと同じ順）。
 *
 * マクロから見える形は**セル値の合併型の並び**である（セル値そのものの綴りは
 * `crate::types` が持つ）。
 */
cells: CellValue[], };

/**
 * 行の識別子。`host.readRange` が返す識別子を、そのまま書き込みへ渡せる。
 */
type RowId = string;

/**
 * 範囲の読みの結果（要件 4.4）。**1 回の呼び出しで範囲の全部を返す。**
 */

type RowPage = { 
/**
 * 要求した範囲の行（要求した順のまま）。
 */
rows: Array<ReadRow>, };

/**
 * 行の範囲（要件 4.4 の「行の範囲の読み」）。**0 起点の位置**で、両端を含む。
 *
 * 位置で指すのは、**行ごとの呼び出しを強いないため**である（要件 4.4 / 11.3）。識別子で
 * 指す形にすると、全行を読むには先に全行の識別子を集める必要があり、それは行ごとの
 * 呼び出しそのものになる。書き込み（`host.setCells` の `CellWrite`）は行の識別子で指す —
 * 読みで受け取った [`ReadRow::id`] をそのまま渡せる。
 *
 * 位置は文書の行の並び（表示の順）であり、行の識別子の順とは一致しない
 * （`document-format` は行の並べ替えを持つ）。[`RowSpan::resolve`] が位置を行数へ当てる。
 */

type RowSpan = { 
/**
 * 始まりの位置（0 起点。含む）。
 */
from: number, 
/**
 * 終わりの位置（0 起点。含む）。
 */
to: number, };

/**
 * シートの識別子。`host.sheets` が返す識別子を、読み書きの要求へ渡す。
 */
type SheetId = string;

/**
 * マクロから見たシート 1 枚（要件 4.1 の「シートの一覧」）。
 *
 * マクロ向けの `.d.ts` へは `ts-rs` の導出で出る（タスク 3.3。モジュール docs の
 * 「`.d.ts` への写し」）。
 */

type SheetInfo = { 
/**
 * シート識別子。読み書きの要求（`columns` / `readRange` / `setCells`）へ渡す。
 *
 * マクロから見える形は**文字列**である（`document-format` の識別子は文字列として
 * 往復し、マクロは読みで受け取った文字列をそのまま書きの要求へ渡す）。上流の型に
 * `ts-rs` の導出が無いため、綴りはここで決める（宣言は `crate::types` が持つ）。
 */
id: SheetId, 
/**
 * シート名（利用者に見えている名前）。
 */
name: string, 
/**
 * 行数。**重ね合わせを見ない**（文書の行数。モジュール docs の規則 3）。
 */
row_count: number, };

/**
 * 列に宣言された型の種別（`host.columns` が返す `kind`）。
 *
 * 綴りは `schema-engine` の種別カタログ（`TypeKind::ALL`）の変種名である。
 * マクロへ種別を渡す口は、この綴りを作る `macro_runtime::types::type_kind_name` を使う
 * （`.d.ts` と実行時の値が同じ綴りになる）。
 */
type TypeKind = "Int" | "Float" | "Decimal" | "Text" | "Bool" | "Date" | "DateTime" | "Enum" | "Ref" | "Attachment" | "Object" | "Array" | "Any" | "Custom";
