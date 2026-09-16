/**
 * 窓の記憶と先読み、および生バイト経路の符号化と復号（tasks.md 7.3。data-grid 要件 1.4, 1.7,
 * 11.1, 11.2, 11.6。design.md「WindowCache」と「生バイト経路の要求の頭」）。
 *
 * # 何を担うか
 *
 * 3 つである。
 *
 * 1. **要求の符号化**（[`encodeWindowRequest`]）。`grid_rows_window` の引数を組み立てる。
 *    配置の唯一の源は `src-tauri/src/commands/grid.rs` の「生バイト経路」節であり、
 *    **本 module はそれを写す**（形を変えるときは両側を同時に直すこと）
 * 2. **窓の復号**（[`decodeWindow`]）。配置の唯一の源は
 *    `crates/data-grid/src/transport/mod.rs` のモジュール docs の表である
 * 3. **窓の記憶と先読み**（[`createWindowCache`]）。`RendererSpec.getCell` の同期契約
 *    （`./renderer/port.ts`）を満たす — **未取得のセルは空白ではなく読み込み中である**
 *
 * # 2 つの形式は別の体系である
 *
 * 要求の版（[`WINDOW_REQUEST_VERSION`]）と窓の版（[`WINDOW_FORMAT_VERSION`]）は**別々に進む**
 * （どちらも今は 1）。頭の幅がどちらも 33 バイトであるのは偶然ではない — 数の欄を位置だけで
 * 引ける固定の幅にしてある（両側の module docs が同じ理由を書いている）。
 *
 * # 言語をまたぐ固定（**TS 側だけで往復させないこと**）
 *
 * 本 module の復号が読むのは、**本物の Rust の符号化器が出したバイト列**である —
 * `crates/data-grid/tests/fixtures/window_protocol.txt`（Rust 側は
 * `crates/data-grid/tests/window_protocol_fixture.rs` が「符号化器はいまもそれを書く」ことを
 * 表明し、本 module の検査（`windowCache.test.ts`）が「そのバイト列をそう読む」ことを表明する）。
 * **TS 側の符号化と復号を往復させるだけでは足りない** — 両方が同じ誤り（たとえば
 * リトルエンディアンであるべき欄をビッグエンディアンで書く）を共有していれば緑になる。
 * 要求の側は Rust の復号器が `src-tauri` に閉じており本 module からは呼べないため、
 * **位置ごとの表明**（`windowCache.test.ts` の表）で固定する。
 *
 * # 層の鎖（どこまでが本 module か）
 *
 * 本 module は `renderer/port.ts` の型と `ipc/client.ts` の移送だけを使う。**判定を持たない** —
 * 値の正否も違反の理由も Rust 側の所有であり、窓は表示文字列と札を運ぶだけである
 * （`transport` のモジュール docs「何を運び、何を運ばないか」）。
 */
// **型だけの取り込みである**（`verbatimModuleSyntax` により `import type` が要る）。
import { invokeRaw } from "../../ipc/client";
import type { ColumnSpace } from "./columnSpace";
import type { CellPosition, RenderCell, RowSpan } from "./renderer/port";

// ===========================================================================
// 1. プロトコルの定数（両側の module docs が唯一の源である写し）
// ===========================================================================

/**
 * 要求の頭の版（1 バイト目。`WINDOW_REQUEST_VERSION` ＝ 1）。
 *
 * **窓の版（[`WINDOW_FORMAT_VERSION`]）とは別の体系である。**欄を足すときはこの数を上げる。
 */
export const WINDOW_REQUEST_VERSION = 1;

/** 要求の頭の固定部分の幅（バイト）: 版 1 ＋ 世代 8 ＋ 開始序数 8 ＋ 行数 8 ＋ シート長 8。 */
export const WINDOW_REQUEST_HEADER_LEN = 33;

/**
 * 窓の版（頭の 1 バイト目。`WINDOW_FORMAT_VERSION` ＝ 1）。
 *
 * **この数値は wire ABI であり、動かさない。**知らない版の窓は復号が拒む。
 */
export const WINDOW_FORMAT_VERSION = 1;

/** 窓の頭の幅（バイト）: 版 1 ＋ 世代 8 ＋ 開始序数 8 ＋ 行数 8 ＋ 列数 8。 */
export const WINDOW_HEADER_LEN = 33;

/** 行の識別子の生バイト長（ULID の 128 ビット。**不透明な鍵**として扱う）。 */
export const ROW_KEY_LEN = 16;

/**
 * wire の変種の札の表（`transport` のモジュール docs の表が唯一の源。0..=7）。
 *
 * **並べ替えない。**値は wire ABI であり、7.4 の入力手段の選択に対応する。値を足すときは
 * 末尾へ足す（未知の値は復号が拒む）。
 */
export const CELL_VARIANT_TAGS = [
  "Null",
  "Bool",
  "Int",
  "Float",
  "Decimal",
  "Text",
  "Nested",
  "Attachment",
] as const;

/** wire の変種の札の名前（[`CELL_VARIANT_TAGS`] の要素）。 */
export type CellVariantTag = (typeof CELL_VARIANT_TAGS)[number];

/** 違反の有無のバイト: 違反なし（`VIOLATED_NO`）。 */
const VIOLATED_NO = 0;

/** 違反の有無のバイト: 違反あり（直後に札の塊が続く。`VIOLATED_YES`）。 */
const VIOLATED_YES = 1;

/** 内側の位置の段の種類: オブジェクトのフィールド名（`SEGMENT_FIELD`）。 */
const SEGMENT_FIELD = 0;

/** 内側の位置の段の種類: 配列の 0 起点の添字（`SEGMENT_INDEX`）。 */
const SEGMENT_INDEX = 1;

/**
 * 要求の頭へ載せる値（数の欄はすべて u64 リトルエンディアン）。
 *
 * 数は **`bigint`** である — 欄は u64 であり、TS の数（2^53 まで）では**上位のバイトを
 * 運べない**。境界の値を丸めると、壊れた要求が正常に見える（Rust 側が「収まらない」経路を
 * 作らないのと同じ理由である）。負の値は `DataView` が拒む。
 */
export interface WindowRequestFields {
  /** 要求が名乗る世代。 */
  readonly generation: bigint;
  /** 可視行の序数（文書の位置ではない）。 */
  readonly start: bigint;
  /** 要求する行の数。 */
  readonly count: bigint;
  /** シートの識別子（`grid_open_sheet` に渡した文字列と同一のもの）。 */
  readonly sheet: string;
}

/**
 * `grid_rows_window` の引数を組み立てる（**引数はこのバッファ全体である**）。
 *
 * ```text
 * 引数 = 頭 || シートの識別子
 *
 * 頭（33 バイト。数の欄はすべて u64 リトルエンディアン）:
 *   0       版        u8      = WINDOW_REQUEST_VERSION
 *   1..9    世代      u64
 *   9..17   開始序数  u64
 *   17..25  行数      u64
 *   25..33  シート長  u64     シートの識別子の UTF-8 の**バイト**長
 * 33..     シート    UTF-8   （引数の末尾まで）
 * ```
 *
 * 全体の長さは `33 + シート長` であり、**それより長い入力も短い入力も Rust 側は拒む**
 * （余りを黙って捨てると、壊れた要求が正常に見える）。
 *
 * シートの長さは `TextEncoder` のバイト数で数える — `sheet.length` は UTF-16 の符号単位の数で
 * あり、日本語のシート名では**食い違う**（その食い違いは要求の長さの検査で落ちる）。
 *
 * # 誤り
 *
 * 数が負であれば [`DataView`] が [`RangeError`] を投げる。**記憶の側はこれを握って未取得の
 * まま残す**（`getCell` は決して投げない）ため、この口を直接使う側だけが気にすればよい。
 */
export function encodeWindowRequest(fields: WindowRequestFields): Uint8Array {
  const sheet = new TextEncoder().encode(fields.sheet);
  const bytes = new Uint8Array(WINDOW_REQUEST_HEADER_LEN + sheet.byteLength);
  const view = new DataView(bytes.buffer);
  bytes[0] = WINDOW_REQUEST_VERSION;
  view.setBigUint64(1, fields.generation, true);
  view.setBigUint64(9, fields.start, true);
  view.setBigUint64(17, fields.count, true);
  view.setBigUint64(25, BigInt(sheet.byteLength), true);
  bytes.set(sheet, WINDOW_REQUEST_HEADER_LEN);
  return bytes;
}

// ===========================================================================
// 2. 窓の復号（前方 1 回の走査）
// ===========================================================================

/**
 * 窓の復号が失敗する理由（`WindowDecodeError` の写し）。
 *
 * **例外にしない** — 窓は IPC から届くバイト列であり、壊れた入力は値として返さなければ
 * ならない（`getCell` の「決して投げない」契約の手前で握りつぶさないため）。
 */
export type WindowDecodeFailure =
  | { readonly kind: "empty" }
  | { readonly kind: "truncated" }
  | { readonly kind: "unknownVersion"; readonly version: number }
  | { readonly kind: "unknownTag"; readonly tag: number }
  | { readonly kind: "unknownViolationFlag"; readonly flag: number }
  | { readonly kind: "unknownSegmentKind"; readonly value: number }
  | { readonly kind: "trailingBytes" }
  | { readonly kind: "notUtf8" }
  | { readonly kind: "tooLarge" };

/** 復号の結果（Rust の `Result<DecodedWindow, WindowDecodeError>` の写し）。 */
export type WindowDecodeResult =
  | { readonly ok: true; readonly window: DecodedWindow }
  | { readonly ok: false; readonly failure: WindowDecodeFailure };

/** 失敗の理由を人が読める 1 行にする（`WindowDecodeError` の `Display` の写し）。 */
export function describeWindowDecodeFailure(failure: WindowDecodeFailure): string {
  switch (failure.kind) {
    case "empty":
      return "空の窓（要求が通らなかったか、世代が違う）";
    case "truncated":
      return "頭が宣言する行数を読み終える前に尽きた";
    case "unknownVersion":
      return `知らない窓の版 ${failure.version}`;
    case "unknownTag":
      return `知らない変種の札 ${failure.tag}`;
    case "unknownViolationFlag":
      return `知らない違反の有無のバイト ${failure.flag}`;
    case "unknownSegmentKind":
      return `知らない段の種類 ${failure.value}`;
    case "trailingBytes":
      return "宣言した行を読み終えたあとにバイトが残っている";
    case "notUtf8":
      return "表示文字列が UTF-8 ではない";
    case "tooLarge":
      return "数の欄がこの環境の数で表せない";
  }
}

/** 内側の位置の段（`NestedPathSegment` の写し）。 */
export type NestedSegment =
  | { readonly kind: "field"; readonly name: string }
  | { readonly kind: "index"; readonly index: number };

/** 復号された 1 セル（`DecodedCell` の写し）。 */
export interface DecodedCell {
  /** 変種の札のバイト（[`CELL_VARIANT_TAGS`] の添字）。 */
  readonly variant: number;
  /**
   * 違反している内側の位置（報告の順。**空の並びなら違反なし**であり、セル直下の違反は
   * 空の位置 1 つとして現れる）。
   */
  readonly marks: readonly (readonly NestedSegment[])[];
  /** 表示文字列（UTF-8。値なしは空文字）。 */
  readonly text: string;
}

/** 復号された 1 行（`DecodedRow` の写し）。 */
export interface DecodedRow {
  /** 行の識別子の生 16 バイト（**不透明な鍵**。[`rowKeyText`] が正準の 26 文字へ写す）。 */
  readonly key: Uint8Array;
  /** セル（列順）。 */
  readonly cells: readonly DecodedCell[];
}

/** 復号された窓（`DecodedWindow` の写し）。 */
export interface DecodedWindow {
  /** 版（復号できた窓では常に [`WINDOW_FORMAT_VERSION`]）。 */
  readonly version: number;
  /**
   * 世代（この窓が属する文書・表示の状態）。**10 進の文字列である**（境界の規約そのもの）。
   *
   * 窓の頭の欄は u64 であり、TS の数は 2^53 までしか運べない。文字列のまま復号し、いまの世代
   * （同じく文字列）と**文字列として**比べる — 数へ落とす経路を作らない。
   */
  readonly generation: string;
  /** 開始序数（**可視行の序数**であり、文書の位置ではない）。 */
  readonly start: number;
  /** この窓が運ぶ行の数。 */
  readonly rowCount: number;
  /** 1 行が運ぶセルの数（宣言の列数）。 */
  readonly columnCount: number;
  /** 行（表示の順）。空になりうる（端に接する要求、可視行 0 のシート）。 */
  readonly rows: readonly DecodedRow[];
}

/**
 * 表示文字列の復号器（**窓ごとに作り直さない** — 窓は走査のたびに届くため、セルごとに
 * `TextDecoder` を確保すると 1 窓で数千回の確保になる）。不正な UTF-8 は `notUtf8` へ写す。
 */
const UTF8 = new TextDecoder("utf-8", { fatal: true });

/** そのセルが違反しているか（違反の札が 1 つ以上あるかと同じ）。 */
export function isViolated(cell: DecodedCell): boolean {
  return cell.marks.length > 0;
}

/**
 * 窓のバイト列を復号する（**前方 1 回の走査**。`transport` のモジュール docs の不変条件）。
 *
 * 空の窓（長さ 0）は `empty` である — 空の窓は**失敗の表現**であり、行 0 の窓（頭だけの
 * 33 バイト）ではない。**この 2 つは別の理由である**（前者は「要求が通らなかった」＝
 * 再試行する、後者は「端に達した」＝再試行しない）。
 *
 * 検査は Rust 側と 1 対 1 である: 知らない版・知らない札・余分なバイト・切り詰めを拒み、
 * **宣言された数を信用しない**（確保は読み進めながら行う）。
 */
export function decodeWindow(bytes: Uint8Array): WindowDecodeResult {
  try {
    return { ok: true, window: readWindow(bytes) };
  } catch (error: unknown) {
    if (error instanceof DecodeStop) {
      return { ok: false, failure: error.failure };
    }
    // `DecodeStop` 以外は実装の誤りである。握りつぶすと、壊れた窓が「空の窓」と同じ扱いに
    // なって静かに隠れる（呼び出し側の分岐が 1 つ減る）ため、そのまま通す。
    throw error;
  }
}

/**
 * 復号の内側でだけ使う停止の合図。
 *
 * 数値の読み取りが 10 箇所以上で失敗しうるため、各所で `Result` を組み立てる代わりに
 * 1 つの合図で抜ける。**公開の口（[`decodeWindow`]）はこれを投げない。**
 */
class DecodeStop extends Error {
  readonly failure: WindowDecodeFailure;

  constructor(failure: WindowDecodeFailure) {
    super(describeWindowDecodeFailure(failure));
    this.failure = failure;
  }
}

/** 窓のバイト列を読む本体（失敗は [`DecodeStop`] で抜ける）。 */
function readWindow(bytes: Uint8Array): DecodedWindow {
  if (bytes.byteLength === 0) {
    throw new DecodeStop({ kind: "empty" });
  }
  const cursor = new Cursor(bytes);
  const version = cursor.u8();
  if (version !== WINDOW_FORMAT_VERSION) {
    throw new DecodeStop({ kind: "unknownVersion", version });
  }
  const generation = cursor.u64Text();
  const start = cursor.u64();
  const rowCount = cursor.u64();
  const columnCount = cursor.u64();

  const rows: DecodedRow[] = [];
  for (let index = 0; index < rowCount; index += 1) {
    const key = cursor.take(ROW_KEY_LEN);
    const cells: DecodedCell[] = [];
    for (let column = 0; column < columnCount; column += 1) {
      cells.push(readCell(cursor));
    }
    rows.push({ key, cells });
  }
  if (!cursor.atEnd()) {
    throw new DecodeStop({ kind: "trailingBytes" });
  }
  return { version, generation, start, rowCount: rows.length, columnCount, rows };
}

/** セル 1 つを読む。 */
function readCell(cursor: Cursor): DecodedCell {
  const variant = cursor.u8();
  if (CELL_VARIANT_TAGS[variant] === undefined) {
    throw new DecodeStop({ kind: "unknownTag", tag: variant });
  }
  const flag = cursor.u8();
  let marks: NestedSegment[][] = [];
  if (flag === VIOLATED_YES) {
    const count = cursor.u64();
    marks = [];
    for (let index = 0; index < count; index += 1) {
      marks.push(readMark(cursor));
    }
  } else if (flag !== VIOLATED_NO) {
    throw new DecodeStop({ kind: "unknownViolationFlag", flag });
  }
  const text = cursor.utf8(cursor.u64());
  return { variant, marks, text };
}

/** 内側の位置の札 1 つを読む。 */
function readMark(cursor: Cursor): NestedSegment[] {
  const segments: NestedSegment[] = [];
  const count = cursor.u64();
  for (let index = 0; index < count; index += 1) {
    const kind = cursor.u8();
    if (kind === SEGMENT_FIELD) {
      segments.push({ kind: "field", name: cursor.utf8(cursor.u64()) });
    } else if (kind === SEGMENT_INDEX) {
      segments.push({ kind: "index", index: cursor.u64() });
    } else {
      throw new DecodeStop({ kind: "unknownSegmentKind", value: kind });
    }
  }
  return segments;
}

/**
 * 前方向のカーソル（**後戻りする経路を持たない**）。
 *
 * 長さの欄は運ばれる本体の**直前**にあるため、各欄はそれより前のバイトだけを読んで解釈できる
 * （`transport` のモジュール docs の不変条件）。その不変条件の帰結が
 * **「完全な窓のすべての真の接頭辞は拒まれる」**であり、`windowCache.test.ts` が全接頭辞を走査する。
 */
class Cursor {
  readonly #bytes: Uint8Array;
  #at = 0;

  constructor(bytes: Uint8Array) {
    this.#bytes = bytes;
  }

  /** 入力を使い切ったか（余りの検査に使う）。 */
  atEnd(): boolean {
    return this.#at === this.#bytes.byteLength;
  }

  /** 1 バイト。 */
  u8(): number {
    return this.take(1)[0] ?? 0;
  }

  /**
   * リトルエンディアンの u64 を**10 進の文字列**として読む（境界が運ぶ世代の形そのもの）。
   *
   * 世代はこの口で読む — [`Cursor.u64`] は `Number.MAX_SAFE_INTEGER` を越える値を `tooLarge`
   * として拒むが、世代にその上限は無い（u64 の全体が値である）。文字列のまま持ち回れば、
   * 丸めの経路が 1 つも生まれない。
   */
  u64Text(): string {
    const raw = this.take(8);
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    return view.getBigUint64(0, true).toString();
  }

  /** リトルエンディアンの u64（`Number.MAX_SAFE_INTEGER` を越える値は `tooLarge`）。 */
  u64(): number {
    const raw = this.take(8);
    const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
    const value = view.getBigUint64(0, true);
    if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
      throw new DecodeStop({ kind: "tooLarge" });
    }
    return Number(value);
  }

  /** `count` バイトを読み進めて返す（尽きていれば `truncated`）。 */
  take(count: number): Uint8Array {
    // 長さの欄が入力を越えていれば `truncated`（**確保は読み進めながら行う**）。
    if (!Number.isSafeInteger(count) || count < 0 || this.#at + count > this.#bytes.byteLength) {
      throw new DecodeStop({ kind: "truncated" });
    }
    const start = this.#at;
    this.#at += count;
    return this.#bytes.subarray(start, this.#at);
  }

  /** `length` バイトの UTF-8 の本体（UTF-8 でなければ `notUtf8`）。 */
  utf8(length: number): string {
    const raw = this.take(length);
    try {
      return UTF8.decode(raw);
    } catch {
      throw new DecodeStop({ kind: "notUtf8" });
    }
  }
}

/**
 * 行の識別子の生 16 バイトを**正準の 26 文字**（Crockford base32 大文字）へ写す。
 *
 * 窓は行の識別子を生バイトで運ぶ（`ROW_KEY_LEN`）一方、`EditOutcome.affected`（要件 1.7 の
 * 通知）は識別子の**文字列**を運ぶ。両者を突き合わせるにはこの写しが要る（[`createWindowCache`]
 * の `invalidate`）。写しは ULID の規格そのものである — 128 ビットを 5 ビットずつ 26 文字へ
 * 詰め、**先頭に 2 ビットの詰め物**をする（最初の文字は 0..=7 にしかならない）。
 *
 * 正しさは**本物の Rust の符号化器が出した鍵**で固定してある（`windowCache.test.ts` が
 * 固定ファイルの鍵から `01ARZ3NDEKTSV4RRFFQ69G5FAW` を得ることを表明する）。
 */
export function rowKeyText(key: Uint8Array): string {
  let text = "";
  let accumulator = 0;
  let bits = 2; // 先頭の詰め物（130 ビット = 26 文字 × 5 ビットにする）。
  for (const byte of key) {
    accumulator = (accumulator << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      text += CROCKFORD[(accumulator >> bits) & 0x1f];
    }
  }
  return text;
}

/** ULID の Crockford base32 の英字（I・L・O・U を除く。`document-format/src/ids.rs` と同じ）。 */
const CROCKFORD = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";

// ===========================================================================
// 3. 窓の記憶と先読み
// ===========================================================================

/**
 * 1 つの窓が運ぶ行数の既定。
 *
 * 設計は「幅は計測で決める」とし、7.6 がフレーム時間の標本を `RenderProbe` と共用して監視する
 * （design.md「WindowCache」の Implementation Notes と Risks）。**この段階では計測に基づく定数**
 * であり、根拠は次の 4 つである。
 *
 * 1. **可視の数画面ぶん**: 移植口の行の高さの既定は 34 px であり、1080p で見えるのは 30〜40 行
 *    である（`research.md` の 10 万行 × 30 列の観測は「可視 40 行」を前提に置いている）。
 *    256 行はその 6〜8 画面ぶんに当たる
 * 2. **先読みの猶予**: 窓の符号化の費用は**窓の行数に比例**する（`transport` のモジュール docs
 *    「費用の形」）。実測は 10 万行のシートの末尾 200 行で約 139 ミリ秒（design.md
 *    「生バイト経路の失敗の写像」の節）であり、256 行でも同じ桁に収まる。速い走査（毎秒 100 行）
 *    でも 1 窓ぶんの猶予は 2 秒以上あり、**1 窓先を読んでおけば次の窓は可視に入る前に届く**
 * 3. **最初の画面（要件 11.2）**: 最初の窓も同じ幅なので、1 秒の予算に対する符号化の費用は
 *    この桁に収まる（要求は 1 往復である）
 * 4. **記憶の上限（要件 11.6）**: 窓の数は [`MAX_WINDOWS`] で頭打ちであり、行数に比例しない
 *
 * 7.6 と 9.x がフレーム時間の標本を入れたら、この数をそこから決め直すこと。
 */
export const WINDOW_ROWS = 256;

/**
 * 記憶が同時に保つ窓の数の上限（要件 11.6: 表示のための資源を行数に比例させない）。
 *
 * 可視の範囲は高々 2 窓（[`WINDOW_ROWS`] の窓に対して可視は数十行）であり、先読みが前後 1 窓
 * ずつなので**同時に生きているのは 4 窓**である。上限を 12 にするのは、向きが反転したときに
 * 直前に見ていた窓を捨てないためであり、記憶は `12 × 256 = 3072` 行ぶんで頭打ちになる
 * （10 万行のシートでも 3% であり、**行数を 10 倍にしても増えない**）。
 */
export const MAX_WINDOWS = 12;

/**
 * 窓の移送（`invokeRaw` の縫い目）。
 *
 * 本番は `grid_rows_window` を生バイトで呼ぶ（既定は [`createWindowCache`] が入れる）。
 * 検査はここを差し替えて呼び出しを数える（要件 1.4 の「同じ区間への要求を 1 本にまとめる」の
 * 証拠は**呼び出し回数**である）。
 */
export type WindowTransport = (argument: Uint8Array) => Promise<ArrayBuffer>;

/**
 * 既定の移送: `invokeRaw("grid_rows_window", バッファ)`。
 *
 * **入れ子にしない**（`{ argument: buffer }` にすると Tauri が `Uint8Array` を `Array.from()` で
 * 数値の配列へ変換し、JSON として送る — 受け手は生バイトとして読めず、**例外を投げずに**空の窓
 * を返す。`src-tauri/src/commands/bulk.rs` のモジュール docs「経路の性質」）。
 */
const defaultTransport: WindowTransport = (argument) =>
  invokeRaw("grid_rows_window", argument);

/** 記憶を組み立てる指定。 */
export interface WindowCacheOptions {
  /**
   * 表示しているシートの識別子。**`grid_open_sheet` に渡した文字列と同一のものでなければ
   * ならない**（Rust 側は要求のシートを保持しているシートと突き合わせ、一致しなければ空の窓を
   * 返す。`grid.rs` のモジュール docs「シートの照合」）。
   */
  readonly sheet: string;
  /**
   * 列の空間（**表示の位置から文書の列への写像**。8.5 が足した）。
   *
   * 窓が運ぶのは**文書の列**（宣言の列数。`transport` の `WindowCodec::encode`）である一方、
   * 呼び出し側（移植口と選択）が持つのは**表示の位置**である。入れ子の展開が 1 つでもあると
   * 2 つは一致しない（`./columnSpace` の module doc）ので、**この 1 つの値**から両方を引く —
   * 読み（[`WindowCache.getCell`]）も書き（[`WindowCache.documentColumn`]）も同じ写像である。
   *
   * **既定を置かない。**恒等を既定にすると、展開が入った日に古い前提が黙って生き残る
   * （呼び出し側が写像を渡し忘れても気づけない）。
   */
  readonly columns: ColumnSpace;
  /**
   * 可視行の総数（窓の要求をこの数へ切り詰める。絞り込みの適用後の数）。
   *
   * **組み立て時にだけ決まる** — 行数が変わる編集（行の追加・削除・貼り付けの補充・
   * 取り消し）のあとは、[`WindowCache.clear`] へ**新しい数**を渡して作り直す。
   */
  readonly rowCount: number;
  /**
   * いまの世代（**10 進の文字列**。既定は `"0"` — `GridSession` は開いた直後を
   * `Generation::FIRST` とする）。
   *
   * 源は境界の応答（`GridOpenResponse` / `GridViewResponse` / `GridEditResponse` の
   * `generation`）ただ 1 つであり、**画面はそれを採用するだけ**である（数え直さない）。
   */
  readonly generation?: string;
  /** 移送（既定は [`defaultTransport`]）。 */
  readonly transport?: WindowTransport;
  /** 1 つの窓が運ぶ行数（既定は [`WINDOW_ROWS`]）。 */
  readonly windowRows?: number;
  /** 記憶が同時に保つ窓の数（既定は [`MAX_WINDOWS`]）。 */
  readonly maxWindows?: number;
  /**
   * 窓が記憶に入ったときに呼ばれる（区間は**実際に記憶した範囲**）。
   *
   * 画面はこれを受けて `RendererHandle.invalidate(span)` を呼ぶ（移植口は知らせが無ければ
   * 描き直さない）。**到着の順に呼ばれる**（同じ区間の再取得でも呼ばれる）。
   */
  readonly onArrival?: (span: RowSpan) => void;
}

/**
 * 窓の記憶。
 *
 * **呼び出し側（画面）が使う口だけを持つ。**`getCell` は `RendererSpec.getCell` へそのまま
 * 渡せる（本型は `this` を使わない — クロージャで組み立ててあるため、束縛を外しても動く）。
 */
export interface WindowCache {
  /**
   * セルを引く。**同期であり、例外を投げない**（移植口の不変条件）。
   *
   * 記憶にあれば直ちに返し、無ければ**読み込み中**（`loading: true`）を返して取得を始める。
   * 空白で代用しないのは、空白が「値なし」と区別がつかないためである（要件 1.4）。
   */
  getCell(position: CellPosition): RenderCell;
  /**
   * その可視行の**文書の行の識別子**（正準の 26 文字）を返す。**記憶に無ければ `null`**。
   *
   * 編集の宛先（生成物の `GridCellAddress.row`）を組むための口である（tasks.md 8.3）。画面は
   * **可視行の序数しか持たない** — 行の識別子は窓（[`DecodedRow.key`]）にしか無く、序数から
   * 行への対応を持つのは本記憶と移植口だけである。
   *
   * 写しは [`rowKeyText`] の 1 つだけである。**`invalidate` が突き合わせる綴りと同じでなければ
   * ならない**（同じ行を別の綴りで名指しすると、影響を受けた行の窓が捨てられない）。
   *
   * `null` は「まだ無い」である（範囲の外・未取得の行・整数でない序数）。**空白や推測で
   * 答えてはならない** — 呼び出し側はそれを文書の位置として使う（要件 8.6 の取り違え）。
   * **[`getCell`] と違い、要求を始めない**（引く口ではなく、既に描かれている行の身元を尋ねる
   * 口である）。
   */
  rowId(position: CellPosition): string | null;
  /**
   * その行の識別子が**いまの表示の順序で何番目か**（表示の序数）。**記憶に無ければ `null`**。
   *
   * **取り消しとやり直しの移動先（要件 9.8）のための口である**（tasks.md 8.9。`./history`）。
   * `rowId` の逆向きであり、**序数と行の対応を持つ唯一の場所**が本記憶であるという事実から
   * 来ている — 境界は行の識別子しか運ばず（`grid_history` の応答もまた然り）、序数を求める
   * 手段はここにしか無い。
   *
   * **捨てる前に引くこと。**`invalidate` / `clear` は影響を受けた行の窓そのものを捨てるので、
   * 順序を誤れば答えは必ず `null` になる（`./history` の module doc「移動先の解決は捨てる前に
   * 済ませる」）。
   *
   * `null` は「まだ無い」である（未取得の行・捨てた行・範囲の外）。**推測で答えてはならない**
   * — 呼び出し側はそれを現在位置として使う（要件 8.6 の取り違え。`rowId` と同じ規律である）。
   * `rowId` と対称に、**要求を始めない**（引く口ではなく、既に描かれている行の位置を尋ねる
   * 口である）— 序数を求めるには「どの行か」が既に分かっている必要があり、文書全体を走査する
   * 経路は本機能に無い（行数に比例する費用を作らない。要件 11.6）。
   */
  ordinalOf(rowId: string): number | null;
  /**
   * その**表示の位置**が指す文書の列の添字（`./columnSpace` の写像そのもの）。答えられなければ
   * `null`。
   *
   * **編集の宛先（生成物の `GridCellAddress.column`）を組むための口である**（tasks.md 8.5。
   * 要件 5.7、8.6）。画面は**表示の位置しか持たない**（選択も移植口の座標も表示の位置である）が、
   * 送る先は文書の列である。写像を呼び出し側で書き直すと、**読みと書きが別の規則で列を決める**
   * 経路ができる（展開が入ると、片方だけが正しいまま残る）— したがって本記憶が持つ 1 つの
   * 写像を、この口を通して**そのまま**使う。
   *
   * `null` は「その位置に列が無い」である（範囲の外・整数でない位置）。**推測で答えては
   * ならない** — 呼び出し側はそれを文書の位置として使う（[`WindowCache.rowId`] と同じ規律）。
   */
  documentColumn(position: CellPosition): number | null;
  /**
   * そのセルが**違反している内側の位置**（窓の違反の札。要件 4.5）。`null` は**未取得**である。
   *
   * 空の並びは「違反していない」であり、`null`（まだ分からない）と区別する — 8.4 が
   * `RenderCell.violated` を門番にしたのと同じ規律である（未取得を「違反なし」と読むと、
   * 窓が届いたときに提示が変わる）。内側の位置そのものを提示するのは入れ子の詳細表示であり、
   * 移植口はこれを運ばない（`RenderCell` は違反の有無の 1 ビットだけである。8.4 の申し送り）。
   *
   * **[`WindowCache.getCell`] と同じく、要求を始める**（未取得なら窓を取りに行く）。詳細表示は
   * 描画の外（利用者の操作）から開かれるため、その行の窓はまだ無いことがある。
   */
  nestedMarks(position: CellPosition): readonly (readonly NestedSegment[])[] | null;
  /**
   * 可視範囲が変わったことを報せる（窓の要求と先読みの起点）。
   *
   * **走査の向きはこの通知の並びから観測する**（前回の開始序数との比較）。向きが変わると、
   * 先読みする側も入れ替わる。
   */
  setVisibleSpan(span: RowSpan): void;
  /**
   * 影響を受けた行の通知（`EditOutcome.affected`。要件 1.7）。
   *
   * **その行を含む窓だけを捨てる**（他の窓は残る）。落ちた窓が可視のものであれば、その場で
   * 取得し直す（次の走査を待たない）。識別子は大文字小文字を問わない。
   *
   * **行の追加・削除のように「何番目の行がどの行か」そのものが変わる編集では、本通知では
   * 足りない** — 影響を受けた行を含む窓を捨てても、その後ろの行がずれたままになる。その場合は
   * 呼び出し側が [`WindowCache.clear`] を**新しい行数**とともに呼ぶこと（行数の変化が合図で
   * あり、その値は `GridEditResponse` の `row_count` がそのまま使える）。
   */
  invalidate(affected: readonly string[]): void;
  /**
   * いまの世代を置き換える（**値は応答が運ぶ 10 進の文字列である**）。
   *
   * 本記憶は**一致するかどうかだけ**を見る（`WindowCodec::is_stale` と同じ規律。大小では
   * 見ない）。置き換えたあとに届いた古い応答は捨てられる。
   */
  setGeneration(generation: string): void;
  /**
   * 記憶をすべて捨てる（**序数と行の対応、または列の構成が変わったとき**）。
   *
   * 並べ替え・絞り込み・入れ子の展開は「何番目の行がどの行か」を変えるため、古い窓は別の行を
   * 指す。**進行中の応答も捨てる**（古い対応のまま記憶へ入る経路を閉じる）。
   *
   * 引数は**新しい可視行の総数**である（画面は `GridEditResponse` の `row_count` をそのまま
   * 渡す）。行数が変わる編集（行の追加・削除・貼り付けの補充・取り消し）では、窓が覆う序数の
   * 範囲そのものが変わる — **本記憶の行数は組み立て時にしか決まらない**ため、渡さなければ
   * [`WindowCacheOptions.rowCount`]（開いたときの数）へ戻る。増えた行は渡された数まで要求の
   * 対象になり（渡さなければ**永久に読み込み中**になる）、減った先は範囲の外として扱い、
   * 要求も配りもしない。
   */
  clear(rowCount?: number): void;
  /** 記憶を手放す（画面の片付け。以後の応答は捨てられる）。 */
  dispose(): void;
  /** いまの世代（**10 進の文字列**）。 */
  readonly generation: string;
  /** 記憶が保つ窓の数（要件 11.6 の観測）。 */
  readonly windowCount: number;
  /** 進行中の要求の数（同じ区間への要求が 1 本にまとまっていることの観測）。 */
  readonly pendingCount: number;
}

/**
 * 10 進の文字列か（**境界が運ぶ世代の形**。`u64` の全体を桁の並びとして運ぶ唯一の形である）。
 *
 * 桁以外（空・記号・小数・先頭の `-`）は世代として扱わない — 扱うと、`BigInt` が投げる値や
 * 丸められた値が要求の頭へ載る経路ができる。**投げずに `false` を返す**（本 module の他の口と
 * 同じ規律。`getCell` は決して投げない）。
 */
function isDecimalGeneration(value: string): boolean {
  return /^[0-9]+$/.test(value);
}

/** 記憶が保つ 1 つの窓。 */
interface StoredWindow {
  /** 要求した区間の鍵（`同じ区間への要求を 1 本にまとめる` の同一性）。 */
  readonly key: string;
  /** **実際に記憶した区間**（窓が要求より短ければ切り落とされている）。 */
  readonly span: RowSpan;
  /** 行（表示の順。`span.start` からの並び）。 */
  readonly rows: readonly DecodedRow[];
  /** 最後に使った順序（追い出しの規則に使う）。 */
  used: number;
}

/**
 * 窓の記憶と先読みを組み立てる。
 *
 * # 状態の模型（design.md「WindowCache」の State Management）
 *
 * - **序数の区間を鍵とする窓の表**（[`StoredWindow`] の並び。上限 [`MAX_WINDOWS`]）
 * - **いまの世代**（`generation`）
 * - **進行中の要求**（区間の鍵の集合。同じ区間への要求を 1 本にまとめる実体）
 * - **可視の区間と走査の向き**（先読みの起点）
 * - **観測した可視行の終端**（窓が要求より短ければ「端に達した」を覚え、再要求しない）
 *
 * # 区間の量子化（なぜ可視の区間をそのまま鍵にしないか）
 *
 * 走査では可視の区間が 1 行ずつ動く。可視の区間をそのまま要求すると、**1 行動くたびに別の
 * 要求**になり、記憶は 1 度も当たらない（先読みが追いつかない）。したがって要求の区間は
 * [`WINDOW_ROWS`] に行を揃えて量子化し、**可視の一部でも覆う窓を単位**にする。
 *
 * # 世代の規律
 *
 * 要求は**その時点の世代**を名乗り、応答は次の 3 つすべてを満たすときだけ記憶へ入る:
 * 要求の世代がいまの世代と一致する・**窓が名乗る世代**がいまの世代と一致する・
 * `clear` を跨いでいない。
 */
export function createWindowCache(options: WindowCacheOptions): WindowCache {
  const transport = options.transport ?? defaultTransport;
  const windowRows = Math.max(1, Math.floor(options.windowRows ?? WINDOW_ROWS));
  const maxWindows = Math.max(1, Math.floor(options.maxWindows ?? MAX_WINDOWS));
  const sheet = options.sheet;
  /** 列の空間（**表示の位置 → 文書の列**。窓の列の添字と編集の宛先の唯一の源である）。 */
  const space = options.columns;
  /** 表示の位置ごとの札（この並びの長さが**表示の列の数**である）。 */
  const variants = space.variants;
  const onArrival = options.onArrival;
  const initialRowCount = Math.max(0, Math.floor(options.rowCount));

  /** いまの世代（**一致を見るだけである**）。10 進の文字列でない指定は `"0"` として扱う。 */
  const initialGeneration = options.generation ?? "0";
  let generation = isDecimalGeneration(initialGeneration) ? initialGeneration : "0";
  /** 記憶（**配列である** — 上限が小さく、引きは毎フレーム走るため、反復の確保を作らない）。 */
  let windows: StoredWindow[] = [];
  /** 進行中の要求の区間の鍵。 */
  const pending = new Set<string>();
  /** `clear` を跨いだ応答を捨てるための世代（記憶の側の世代とは別である）。 */
  let epoch = 0;
  /** 可視の区間（最後に報せられたもの）。 */
  let visible: RowSpan | null = null;
  /** 直前の可視の開始序数（走査の向きを観測する唯一の材料）。 */
  let previousStart: number | null = null;
  /** 観測した走査の向き（1 = 下へ、-1 = 上へ）。最初は下とみなす。 */
  let direction: 1 | -1 = 1;
  /** 可視行の終端（要求を切り詰める上限。窓の切り落としで下がることはあっても上がらない）。 */
  let end = initialRowCount;
  /** 使用の順序（追い出しの規則）。 */
  let clock = 0;
  /** 記憶を手放したか（以後の応答は捨てる）。 */
  let disposed = false;

  /** その序数を覆う窓（**上限が小さいので線形で引く**。触った窓を使った印へ繰り上げる）。 */
  const covering = (row: number): StoredWindow | undefined => {
    for (const entry of windows) {
      if (row >= entry.span.start && row < entry.span.start + entry.span.count) {
        clock += 1;
        entry.used = clock;
        return entry;
      }
    }
    return undefined;
  };

  /** その序数を含む窓の開始序数（量子化）。 */
  const windowStartFor = (row: number): number => Math.floor(row / windowRows) * windowRows;

  /**
   * 1 つの区間を要求する（同じ区間が記憶にあるか進行中なら何もしない）。
   *
   * 符号化と移送の失敗は**どちらも同じ扱い**である: 進行中の印を外し、未取得のまま残す
   * （次の引きが再試行する）。**この関数は決して投げない。**
   */
  const requestAt = (start: number): void => {
    if (disposed) {
      return;
    }
    // 開始序数 `start` から始まる窓の区間（可視行の終端を越えれば要求しない）。
    if (start < 0 || start >= end) {
      return;
    }
    const span: RowSpan = { start, count: Math.min(windowRows, end - start) };
    // 区間の鍵（同じ区間への要求を 1 本にまとめる同一性）。
    const key = `${span.start}:${span.count}`;
    if (windows.some((entry) => entry.key === key) || pending.has(key)) {
      // 同じ区間への要求は 1 本にまとめる（進行中のものも記憶にあるものも要求しない）。
      return;
    }
    const claimedEpoch = epoch;
    pending.add(key);
    let response: Promise<ArrayBuffer>;
    try {
      response = transport(
        encodeWindowRequest({
          // **10 進の文字列から u64 へ戻す唯一の場所**である（`setBigUint64` が欄の幅を
          // 決める）。丸めは起こらない — `BigInt` は桁の文字列をそのまま読む。
          generation: BigInt(generation),
          start: BigInt(span.start),
          count: BigInt(span.count),
          sheet,
        }),
      );
    } catch {
      // 移送が同期的に投げた（実装の誤りか、移送そのものの失敗）。未取得のまま残す。
      pending.delete(key);
      return;
    }
    response.then(
      (buffer) => {
        store(span, key, claimedEpoch, new Uint8Array(buffer));
      },
      () => {
        // 経路そのものの失敗（IPC の不達）。**空の窓と同じ扱いである** — 未取得のまま残し、
        // 読み込み中として描かれる（設計の誤り表「経路の失敗」）。
        pending.delete(key);
      },
    );
  };

  /** 応答を記憶へ入れる（**世代が変わっていれば捨てる**）。 */
  const store = (
    requested: RowSpan,
    key: string,
    claimedEpoch: number,
    bytes: Uint8Array,
  ): void => {
    pending.delete(key);
    if (disposed || claimedEpoch !== epoch) {
      // 記憶そのものを捨てた（`clear`）あとの応答は入れない。
      return;
    }
    const result = decodeWindow(bytes);
    if (!result.ok) {
      // 空の窓（失敗と世代違い）と壊れた窓。**どちらも記憶に入れない** — 未取得のまま
      // 残し、再試行できるようにする（空白として描かない）。
      return;
    }
    const window = result.window;
    if (window.generation !== generation) {
      // **世代が変わった応答は捨てる**（記憶に入れない）。窓が名乗る世代が判断の材料である —
      // 応答は**窓そのものの札**を運ぶため、要求の時に名乗った世代を持ち回る必要は無い
      // （古い世代の要求には、そもそも Rust 側が空の窓を返す）。捨てた区間は未取得のまま
      // 残るので、次の引きが新しい世代で要求し直す。
      return;
    }
    if (window.rows.length < requested.count) {
      // 窓が要求より短い ＝ 可視行の末尾に接した（`RowOrder::span` の切り落とし）。
      // **端を覚える** — 覚えないと、存在しない序数を毎フレーム要求し続ける。
      end = Math.min(end, requested.start + window.rows.length);
    }
    if (window.rows.length === 0) {
      return;
    }
    const entry: StoredWindow = {
      key,
      span: { start: window.start, count: window.rows.length },
      rows: window.rows,
      used: clock,
    };
    clock += 1;
    const existing = windows.findIndex((candidate) => candidate.key === key);
    if (existing >= 0) {
      windows[existing] = entry;
    } else {
      windows.push(entry);
    }
    evict();
    onArrival?.(entry.span);
  };

  /** 上限を越えた分を、**最も使われていない窓**から捨てる。 */
  const evict = (): void => {
    while (windows.length > maxWindows) {
      let oldest = 0;
      for (let index = 1; index < windows.length; index += 1) {
        if ((windows[index]?.used ?? 0) < (windows[oldest]?.used ?? 0)) {
          oldest = index;
        }
      }
      windows.splice(oldest, 1);
    }
  };

  /** 可視の範囲が掛かる窓を要求する（**向きの先から**）。 */
  const requestVisibleWindows = (): void => {
    const span = visible;
    if (span === null) {
      return;
    }
    const stop = Math.min(end, span.start + span.count);
    if (stop <= span.start) {
      return;
    }
    const first = windowStartFor(span.start);
    const last = windowStartFor(stop - 1);
    if (direction > 0) {
      // 下向きなら、向きの先（下）の窓から要求する。
      for (let at = last; at >= first; at -= windowRows) {
        requestAt(at);
      }
    } else {
      for (let at = first; at <= last; at += windowRows) {
        requestAt(at);
      }
    }
  };

  const getCell = (position: CellPosition): RenderCell => {
    const display = position.column;
    // 列が範囲の外であるときは列の札も決まらないので、既定の入力へ落ちる札（`Any`）を使う。
    const variant = variants[display] ?? "Any";
    const row = position.row;
    // **表示の位置 → 文書の列**（窓が運ぶのは文書の列だけである。`./columnSpace` の module doc）。
    const column = space.documentColumn(display);
    if (
      !Number.isInteger(row) ||
      row < 0 ||
      row >= end ||
      column === null
    ) {
      // 範囲の外（負・行数以上・列数以上・整数でない・列が無い）。**取得もしない**（存在しない
      // 序数を要求しない）。
      // 読み込み中として返すのは、範囲の外を描かない描き手にとって観測されない札であり、
      // 空白（値なし）と取り違えられないためである。
      return { text: "", variant, violated: false, loading: true };
    }
    const entry = covering(row);
    if (entry === undefined) {
      // 未取得である。**要求を始めて、読み込み中を返す**（同期の契約）。
      requestAt(windowStartFor(row));
      return { text: "", variant, violated: false, loading: true };
    }
    const cell = entry.rows[row - entry.span.start]?.cells[column];
    if (cell === undefined) {
      // 行は取得済みだが、窓がその列を運んでいない（宣言の列数を越える列）。
      return { text: "", variant, violated: false, loading: false };
    }
    return { text: cell.text, variant, violated: isViolated(cell), loading: false };
  };

  /**
   * その可視行の行の識別子（`WindowCache.rowId` の実装）。
   *
   * **要求を始めない**（引く口ではない）。`covering` は使った印を繰り上げるので、この引きも
   * 「その窓を使った」として数えられる — 身元を尋ねる相手はたいてい直前に描かれた窓であり、
   * 追い出しの規則から見て同じ扱いでよい。
   */
  const rowId = (position: CellPosition): string | null => {
    const row = position.row;
    if (!Number.isInteger(row) || row < 0 || row >= end) {
      return null;
    }
    const entry = covering(row);
    if (entry === undefined) {
      return null;
    }
    const decoded = entry.rows[row - entry.span.start];
    return decoded === undefined ? null : rowKeyText(decoded.key);
  };

  /**
   * その行の識別子が**いまの表示の順序で何番目か**（要件 9.8。`rowId` の逆向きである）。
   *
   * 記憶が保つ窓を順に見る（上限は 12 窓であるので、費用は行数に依らない。要件 11.6）。
   * **要求を始めない** — 文書全体から 1 行を探す経路は本機能に無い（`ordinalOf` の doc）。
   *
   * 綴りの突き合わせは [`invalidate`] と同じ規律である（大小を問わない）— 行の識別子の写しは
   * `rowKeyText` の 1 つであり、別の綴りで名指しされた行を「無い」と答えてはならない。
   */
  const ordinalOf = (wanted: string): number | null => {
    if (wanted.length === 0) {
      return null;
    }
    const upper = wanted.toUpperCase();
    for (const entry of windows) {
      for (let offset = 0; offset < entry.rows.length; offset += 1) {
        const decoded = entry.rows[offset];
        if (decoded !== undefined && rowKeyText(decoded.key).toUpperCase() === upper) {
          return entry.span.start + offset;
        }
      }
    }
    return null;
  };

  /**
   * 表示の位置が指す**文書の列**（`./columnSpace` の写像そのもの。8.5）。
   *
   * 記憶の側で写像を書き直さない（**同じ 1 つの値を、読みと書きの両方が引く**）。行の範囲は
   * 見ない — 列の写像は行に依らないためであり、見ると「窓が未取得だから列も分からない」という
   * 誤った答えになる（宛先を組む呼び出し側は、行の識別子の `null` と列の `null` を別々に扱う）。
   */
  const documentColumn = (position: CellPosition): number | null =>
    space.documentColumn(position.column);

  /**
   * そのセルの違反の札（**内側の位置**。要件 4.5）。
   *
   * `getCell` と同じ引き方をする（窓が無ければ要求を始めて `null` を返す）。**セルの本体を
   * 読むのと同じ写像を使う**ので、表示の位置と文書の列が食い違う構成でも同じセルを指す。
   */
  const nestedMarks = (position: CellPosition): readonly (readonly NestedSegment[])[] | null => {
    const row = position.row;
    const column = space.documentColumn(position.column);
    if (!Number.isInteger(row) || row < 0 || row >= end || column === null) {
      return null;
    }
    const entry = covering(row);
    if (entry === undefined) {
      requestAt(windowStartFor(row));
      return null;
    }
    const cell = entry.rows[row - entry.span.start]?.cells[column];
    return cell === undefined ? null : cell.marks;
  };

  const setVisibleSpan = (span: RowSpan): void => {
    const start = Math.max(0, Math.floor(span.start));
    const count = Math.max(0, Math.floor(span.count));
    // **向きは通知の並びから観測する**（前回の開始序数との比較）。
    if (previousStart !== null && start !== previousStart) {
      direction = start > previousStart ? 1 : -1;
    }
    previousStart = start;
    visible = { start, count };
    requestVisibleWindows();
    // 先読み: **向きの先**の窓を先に、反対側を次に要求する（反対側は向きが反転したときに効く）。
    const stop = Math.min(end, start + count);
    if (stop > start) {
      const first = windowStartFor(start);
      const last = windowStartFor(stop - 1);
      if (direction > 0) {
        requestAt(last + windowRows);
        requestAt(first - windowRows);
      } else {
        requestAt(first - windowRows);
        requestAt(last + windowRows);
      }
    }
  };

  const invalidate = (affected: readonly string[]): void => {
    if (affected.length === 0 || disposed) {
      return;
    }
    // 識別子は**大文字小文字を問わない**（Rust の正準形は大文字である）。
    const wanted = new Set(affected.map((id) => id.toUpperCase()));
    let droppedVisible = false;
    const survivors: StoredWindow[] = [];
    for (const entry of windows) {
      const hit = entry.rows.some((row) => wanted.has(rowKeyText(row.key)));
      if (hit) {
        if (
          visible !== null &&
          entry.span.start < visible.start + visible.count &&
          entry.span.start + entry.span.count > visible.start
        ) {
          droppedVisible = true;
        }
        continue;
      }
      survivors.push(entry);
    }
    windows = survivors;
    if (droppedVisible) {
      // 可視の窓が落ちた。**その場で取得し直す**（次の走査を待たない。要件 11.3 の反映）。
      requestVisibleWindows();
    }
  };

  const setGeneration = (value: string): void => {
    if (!isDecimalGeneration(value) || value === generation) {
      return;
    }
    generation = value;
  };

  const clear = (rowCount?: number): void => {
    windows = [];
    pending.clear();
    epoch += 1;
    visible = null;
    previousStart = null;
    // 行数が渡されたなら、**窓が覆いうる序数の範囲を作り直す**（ここが増える唯一の経路で
    // ある。窓の切り落としは `store` の中で下がる一方である）。渡されなければ開いたときの
    // 数へ戻す（従来の挙動）。
    end = rowCount === undefined ? initialRowCount : Math.max(0, Math.floor(rowCount));
  };

  const dispose = (): void => {
    disposed = true;
    windows = [];
    pending.clear();
    epoch += 1;
  };

  return {
    getCell,
    rowId,
    ordinalOf,
    documentColumn,
    nestedMarks,
    setVisibleSpan,
    invalidate,
    setGeneration,
    clear,
    dispose,
    get generation(): string {
      return generation;
    },
    get windowCount(): number {
      return windows.length;
    },
    get pendingCount(): number {
      return pending.size;
    },
  };
}
