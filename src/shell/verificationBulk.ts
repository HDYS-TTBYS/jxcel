/**
 * 検証専用: 大きなペイロードの一括転送の**駆動側**（tasks.md 10.8 / 要件 4.5）。
 *
 * 10 万行規模のデータが **1 回の呼び出し**で受け渡しでき、**呼び出し回数が行数に比例しない**
 * ことを、実アプリの経路（`invokeRaw` → `bulk_echo`。7.2 の生バイト経路）で観測するための
 * 唯一の駆動側である。検査器（`scripts/check-bulk-transfer.sh`）はこのモジュールが
 * 診断の記録へ残した行を読んで判定する。
 *
 * 所有: シェルの起動経路（`src/main.tsx` の結線）。
 * 要件: 4.5（10 万行規模を 1 回の呼び出しで。行ごとに通信境界を越えない）。
 *
 * # 経路（Rust とフロントエンドの対の契約）
 *
 * 1. 検証ビルド（`--features verification-triggers`）を環境変数
 *    `JXCEL_VERIFICATION_BULK_ROWS=<行数の一覧>`（例 `100,100000`）付きで起動する。
 * 2. Rust（`src-tauri/src/window/mod.rs` の `bulk_rows_script`）がその値を**ウィンドウの
 *    初期化スクリプト**として書き、`window.__JXCEL_VERIFICATION_BULK_ROWS__` に載せる。
 *    **Webview はプロセスの環境変数を読めない**ため、この 1 手だけが環境変数を画面へ運ぶ方法で
 *    ある（コマンドを増やさない — 通信境界 `src/ipc/bindings.ts` は閉じている）。
 * 3. 本モジュールがマウント直後にそのグローバルを読み、**行数の一覧として妥当なときだけ**
 *    行数ごとに **1 回ずつ** `invokeRaw("bulk_echo", payload)` を呼ぶ。
 * 4. 各転送の結果（行数・送信バイト数・受信バイト数・バイト一致・**呼び出し回数**）を
 *    `BULK_RESULT_EVENT` で Rust へ通知し、Rust（`src-tauri/src/lifecycle.rs` の
 *    `register_bulk_result_listener`。検証ビルドにだけ存在する）が診断の記録へ 1 行で残す。
 *
 * **グローバルの綴り・イベントの綴り・1 行あたりのバイト数は Rust 側および検査器と対であり、
 * 片方だけ変えてはならない。**
 *
 * # 既定のビルドと既定の起動は変わらない
 *
 * `verification-triggers` は非既定の cargo feature であり、**既定のビルドには環境変数の読み取り
 * も初期化スクリプトも入らない**（配布物に検証専用の入口を残さない。`src-tauri/Cargo.toml`）。
 * したがって既定のビルドの起動では本モジュールは常に何もせずに戻る。**通常の起動の挙動は
 * 変わらない**（10.3 の起動予算・8.2 の初回描画・9.6 の空ウィンドウの画面はいずれも不変である —
 * 転送は初回描画の後に回す）。
 *
 * # 何をもって「1 回の呼び出し」とするか
 *
 * 呼び出し回数は**このモジュールが実際に `invokeRaw` を呼んだ回数**を行数ごとに数えたもので
 * ある（行数に依存しない定数 1 が期待値）。同じ事実は Rust 側の `bulk_echo` の記録行
 * （呼び出しごとに 1 行。7.2）からも独立に数えられる — 検査器は**両方**を突き合わせるので、
 * この申告だけを大きく書いても通らない。
 *
 * # ウィンドウは 1 枚である前提（検査が守る）
 *
 * 本モジュールは**ウィンドウごとに**（`src/main.tsx` がウィンドウの読み込みごとに 1 回呼ぶ）
 * 動く。10.8 の検査はドキュメントを渡さずに起動するので、開くウィンドウは起動時の 1 枚だけ
 * であり、駆動も 1 回だけである。**2 枚目が開けば転送の回数が増える**が、検査器は
 * 「一覧の件数を超える呼び出しが 1 件も無いこと」を要求するので**黙って通らない**。
 */

import { emit } from "@tauri-apps/api/event";

import { invokeRaw } from "../ipc/client";

/**
 * 検証専用: 転送する行数の一覧を載せるグローバルの名前。
 *
 * **`src-tauri/src/window/mod.rs` の `VERIFY_BULK_ROWS_GLOBAL` と同じ綴りでなければならない。**
 * 名前の定義を両側に 1 つずつ置くのは、これがビルド時にも実行時にも共有できない
 * **検証専用の対の契約**だからである（既定のビルドには Rust 側の定義が存在しない）。
 */
const VERIFICATION_BULK_ROWS_GLOBAL = "__JXCEL_VERIFICATION_BULK_ROWS__" as const;

/**
 * 転送の結果を Rust へ通知するイベントの名前。
 *
 * **`src-tauri/src/lifecycle.rs` の `VERIFY_BULK_RESULT_EVENT` と同じ綴りでなければならない**
 * （上と同じ理由の対の契約）。`emit` は `core:event:default` の許可（`core:default` に含まれる）
 * の範囲であり、**新しい権限もコマンドも要さない**。
 */
const BULK_RESULT_EVENT = "jxcel-verification-bulk-result";

/**
 * 1 行あたりのバイト数。
 *
 * 検査器（`scripts/check-bulk-transfer.sh`）が期待バイト数を `行数 × これ` で計算するため、
 * **検査器の `bulk_line_bytes` と同じ値でなければならない**。7.2 の実測（10 万行 = 4,700,000 B）
 * と同じ 47 B にする（行番号 10 B + 区切り 1 B + 詰め物 35 B + 改行 1 B）。
 */
export const BULK_LINE_BYTES = 47;

/** 行数の一覧として受け付ける上限（1,000,000 行 × 47 B = 47 MB < 7.2 の 64 MiB の上限）。 */
const MAX_BULK_ROWS = 1_000_000;

declare global {
  interface Window {
    /**
     * 検証専用: 検証ビルドの初期化スクリプトが載せる行数の一覧。**既定のビルドでは決して
     * 設定されない**（`undefined`）。値は配列であることを実行時に検査する。
     */
    readonly __JXCEL_VERIFICATION_BULK_ROWS__?: unknown;
  }
}

/** 1 つの行数についての転送の結果（Rust へ通知する形。Rust 側が記録の 1 行へ写す）。 */
interface BulkTransferOutcome {
  /** 転送した行数。 */
  rows: number;
  /** 送ったバイト数（このモジュールが組み立てたペイロードの長さ）。 */
  sentBytes: number;
  /** 返ってきたバイト数（`invokeRaw` が解決した `ArrayBuffer` の長さ）。 */
  receivedBytes: number;
  /** 送ったバイト列と返ってきたバイト列が**完全に一致**したか（要件 4.5 の往復の同一性）。 */
  byteIdentical: boolean;
  /** この行数の転送で**実際に呼んだ回数**（期待値は 1。行数に比例しないことの証拠）。 */
  invocations: number;
}

/**
 * 一括転送の駆動側を仕掛ける。**`src/main.tsx` から 1 回だけ呼ぶ。**
 *
 * グローバルが無い（既定のビルド・通常の起動）ときは何もしない。あるときは**初回描画の後**に
 * 行数ごとの転送を順に行う（初回描画のフレームと 8.2 のハートビートを妨げないため）。
 * **例外を外へ出さない** — 転送の失敗は結果として記録へ残し（`byteIdentical=false`）、
 * 検査器がそれを見て落ちる。ここで投げると起動そのものが壊れる。
 */
export function installVerificationBulkTransfer(): void {
  const requested = readRequestedRows();
  if (requested === null) {
    return;
  }
  if (typeof requestAnimationFrame !== "function") {
    // 描画フレームを持たない環境（配信先中立の画面など）では駆動しない — 検証は実アプリで行う。
    return;
  }
  // 2 番目の `requestAnimationFrame` で回すのは、初回描画のフレーム（8.2 の通知が送られる
  // フレーム）より後に転送を始めるためである。
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void runAllTransfers(requested);
    });
  });
}

/**
 * グローバルから行数の一覧を読む。**妥当でなければ `null`** を返し、呼び出し側は駆動しない
 * （未知の値・文字列・空の配列・1 件だけの配列を黙って受け付けない）。
 *
 * 「行数の一覧が**行数に比例しないこと**」を示すには**少なくとも 2 つの大きさ**が要るので、
 * 1 件だけの指定は受け付けない（1 件では定数であることを示せない）。
 */
function readRequestedRows(): number[] | null {
  const requested = window[VERIFICATION_BULK_ROWS_GLOBAL];
  if (requested === undefined || requested === null) {
    return null;
  }
  if (!Array.isArray(requested)) {
    console.warn(
      `${VERIFICATION_BULK_ROWS_GLOBAL} が行数の一覧ではない。一括転送は行わない`,
      requested,
    );
    return null;
  }
  const list = requested as unknown[];
  if (list.length < 2) {
    console.warn(
      `${VERIFICATION_BULK_ROWS_GLOBAL} の指定が 1 件だけである（呼び出し回数が行数に比例しないことを示せない）。一括転送は行わない`,
      requested,
    );
    return null;
  }
  const rows: number[] = [];
  for (const value of list) {
    if (
      typeof value !== "number" ||
      !Number.isInteger(value) ||
      value <= 0 ||
      value > MAX_BULK_ROWS
    ) {
      console.warn(
        `${VERIFICATION_BULK_ROWS_GLOBAL} の行数が正の整数ではない（1 以上 ${MAX_BULK_ROWS} 以下）。一括転送は行わない`,
        value,
      );
      return null;
    }
    rows.push(value);
  }
  return rows;
}

/** 指定された行数ごとに **1 回ずつ** 転送し、結果を Rust へ通知する（順に行う）。 */
async function runAllTransfers(rows: number[]): Promise<void> {
  for (const count of rows) {
    const outcome = await transferOnce(count);
    try {
      await emit(BULK_RESULT_EVENT, outcome);
    } catch (error: unknown) {
      console.error("検証用の一括転送の結果を通知できなかった", error);
    }
  }
}

/**
 * 1 つの行数のペイロードを組み立て、**ちょうど 1 回** `invokeRaw` を呼び、往復を検める。
 *
 * 呼び出しが失敗（`invoke` の拒否）しても投げない — 受信バイト数 0・バイト一致 `false` の結果を
 * 返し、検査器が「転送が成立していない」として落ちる。
 */
async function transferOnce(rows: number): Promise<BulkTransferOutcome> {
  const payload = buildPayload(rows);
  let receivedBytes = 0;
  let byteIdentical = false;
  let invocations = 0;
  try {
    // **行ごとに呼ばない。**このペイロード全体で 1 回だけ呼ぶ（要件 4.5）。
    invocations += 1;
    const returned = await invokeRaw("bulk_echo", payload);
    receivedBytes = returned.byteLength;
    byteIdentical = bytesEqual(payload, returned);
  } catch (error: unknown) {
    console.error("検証用の一括転送が失敗した", rows, error);
  }
  return {
    rows,
    sentBytes: payload.byteLength,
    receivedBytes,
    byteIdentical,
    invocations,
  };
}

/**
 * 決定的な行指向のペイロードを組み立てる。
 *
 * 1 行 = 0 詰め 10 桁の行番号 + `:` + 行番号から決まる 35 バイト + 改行 = [`BULK_LINE_BYTES`]。
 * **内容は検査に使わない**（検査が見るのはバイト数と往復の同一性だけである）が、全行が位置に
 * よって異なるので、途中で切り詰められた応答はバイト一致の検査で必ず落ちる。
 */
function buildPayload(rows: number): Uint8Array {
  const payload = new Uint8Array(rows * BULK_LINE_BYTES);
  const colon = 0x3a;
  const newline = 0x0a;
  const zero = 0x30;
  const lowerA = 0x61;
  for (let row = 0; row < rows; row += 1) {
    const offset = row * BULK_LINE_BYTES;
    let value = row;
    for (let digit = 9; digit >= 0; digit -= 1) {
      payload[offset + digit] = zero + (value % 10);
      value = Math.floor(value / 10);
    }
    payload[offset + 10] = colon;
    for (let index = 11; index < BULK_LINE_BYTES - 1; index += 1) {
      payload[offset + index] = lowerA + ((row + index) % 26);
    }
    payload[offset + BULK_LINE_BYTES - 1] = newline;
  }
  return payload;
}

/** 送ったバイト列と返ってきた `ArrayBuffer` が**完全に一致**するか。 */
function bytesEqual(sent: Uint8Array, returned: ArrayBuffer): boolean {
  const received = new Uint8Array(returned);
  if (received.byteLength !== sent.byteLength) {
    return false;
  }
  for (let index = 0; index < sent.byteLength; index += 1) {
    if (sent[index] !== received[index]) {
      return false;
    }
  }
  return true;
}
