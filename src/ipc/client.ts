/**
 * フロントエンド側の薄い呼び出しラッパ。
 *
 * 所有: `IpcClient`（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 4.1（単一の通信境界）, 4.2（境界を越える型をひとつの定義から得ること）,
 * 4.4（失敗の原因を識別でき、成功と区別できること）。
 *
 * 本ファイルはタスク 2.4 が実装した。担うのは次の 4 点だけで、要求と応答の仲介・再試行・
 * キャッシュ・機能別のラッパは持たない（design.md の「生成された型の上の薄いラッパ」）。
 *
 * 1. コマンド名は `./bindings`（ts-rs の生成物）の `COMMAND_NAMES` から導いた `CommandName`
 *    だけを受け付ける。**本ファイルにも呼び出し側にも、コマンド名の文字列リテラルを
 *    手で書かない**（要件 4.1、4.2）。
 * 2. 戻り値は生成物の `IpcResult` 判別可能な合併型であり、`status` で網羅的に分岐できる。
 *    網羅しない分岐は型検査で落ちる（本ファイル末尾の型検査の節がその証拠である）。
 * 3. ドメインの失敗は Rust が返す封筒（`status: "error"`）がそのまま届く。**再包装しない。**
 * 4. `invoke` 自体の拒否も例外として外へ出さず、エラー腕へ写す（要件 4.4）。フロントエンドが
 *    封筒を新しく作るのはこの経路だけである。
 *
 * ## `invoke` の拒否の表し方（要件 4.4）
 *
 * Rust 側は封筒を返すが、`invoke` の Promise は失敗しうる（未知のコマンド、引数や応答の
 * 直列化失敗、開発中の IPC 不在など）。これを `throw` のまま外へ出すと呼び出し側の分岐は
 * 「例外か成功か」になり、失敗の原因も `try`/`catch` の外へ漏れる。
 *
 * そこで `describeRejection` が拒否を文字列へ落とし、**生成物の `IpcError` に 1 種だけ
 * 足したフロントエンド局所の変種** `FrontendIpcError`（`kind: "Frontend"`）としてエラー腕に
 * 合成する。`IpcResult` は `status` で、`IpcError` は `kind` で判別されるため、種別を 1 つ
 * 足しても両側の絞り込みはそのまま働く。網羅的な分岐は `assertNever` が新しい種別を
 * コンパイルエラーにする。**`bindings.ts` と Rust 側の型は変更しない。**
 *
 * この写像によりラッパの戻り値は常に `IpcClientResult<T>`（成功か失敗かの判別可能な
 * 合併型）となり、呼び出し側は `catch` を書かなくてよい。
 *
 * 生成物はペイロード型を名指しする具体形 `WindowContextResult = IpcResult<WindowContext,
 * IpcError>` も持つが、そのエラー型は生成の `IpcError` に固定されておりフロントエンド局所の
 * 変種を載せられない。よってラッパはペイロードを型引数に取る `IpcClientResult<T>` を同じ形で
 * 具体化する（`T = WindowContext` のとき生成の具体形のエラー腕をそのまま含む）。
 *
 * 依存の向き: `./bindings.ts` はタスク 2.2 が生成する追跡対象の生成物であり、**手で
 * 編集しない**。`src/shared/` は本ファイルを参照しない（要件 9.6）。
 */
import { invoke } from "@tauri-apps/api/core";
import type { InvokeArgs } from "@tauri-apps/api/core";

import type { COMMAND_NAMES, IpcError, IpcResult } from "./bindings";

/**
 * 生成物の `COMMAND_NAMES` から導いたコマンド名の合併型。`src-tauri` のハンドラ登録と
 * 同じ配列に由来するため、**配列に無い手書きの文字列はこの型へ代入できず、その場で
 * 型検査が落ちる**（要件 4.1、4.2）。
 */
export type CommandName = (typeof COMMAND_NAMES)[number];

/**
 * 生バイトで応答するコマンド名（要件 4.5）。生成物の `CommandName` から取り出す。
 *
 * `Extract` を使うのは、名前を 2 つ目の一覧として手書きしないためである。単一の源
 * （`crates/app-shell/src/ipc/command_names.rs` の `COMMAND_NAMES`）から `bulk_echo` が
 * 消えれば、この型は `never` になり、`invokeRaw` の呼び出しが型検査で落ちる。
 */
export type RawCommandName = Extract<CommandName, "bulk_echo">;

/**
 * フロントエンド局所の失敗（要件 4.4）。`invoke` 自体の拒否を表す。
 *
 * 生成物の `IpcError` と同じく `kind` で判別され、`detail.message` に原因を運ぶ。
 * **Rust 側の型に足さず、フロントエンド側でのみ合成する。**
 */
export type FrontendIpcError = {
  kind: "Frontend";
  detail: { message: string };
};

/**
 * ラッパが返しうる失敗の全体。生成物の `IpcError`（設定・補助プロセス・ウィンドウ・
 * 診断情報・ドキュメント）に
 * フロントエンド局所の変種を加えたもので、`kind` で網羅的に分岐できる。
 */
export type IpcClientError = IpcError | FrontendIpcError;

/**
 * ラッパの戻り値。`IpcResult` のエラー型を `IpcClientError` に具体化したもので、
 * `status` で成功と失敗を網羅的に分岐できる（要件 4.1、4.4）。
 */
export type IpcClientResult<T> = IpcResult<T, IpcClientError>;

/**
 * 網羅性の検査。判別可能な合併型の分岐が尽きていることを型で強制する。
 *
 * 到達しえない値だけを受け取る（引数の型が `never`）。新しいエラー種別や新しい状態を
 * 足したとき、分岐を更新し忘れた箇所では引数が `never` にならず**コンパイルエラー**に
 * なる。呼び出し側も `default:` からこれを呼ぶことで同じ保証を得られる。
 */
export function assertNever(value: never, hint = "未処理の値"): never {
  throw new Error(`${hint}: ${JSON.stringify(value)}`);
}

/**
 * 失敗の原因を人が読める 1 行にする（要件 4.4）。`kind` を網羅的に分岐し、
 * 生成物に新しい種別が増えたら `assertNever` がコンパイルエラーにする。
 */
export function describeIpcError(error: IpcClientError): string {
  switch (error.kind) {
    case "Settings":
      return `設定の失敗: ${error.detail.message}`;
    case "Sidecar":
      return `補助プロセスの失敗: ${error.detail.message}`;
    case "Window":
      return `ウィンドウの失敗: ${error.detail.message}`;
    case "Diagnostics":
      return `診断情報の失敗: ${error.detail.message}`;
    case "Document":
      return `ドキュメントの失敗: ${error.detail.message}`;
    case "Frontend":
      return `通信境界の失敗: ${error.detail.message}`;
    default:
      return assertNever(error, "エラー種別の分岐が網羅されていない");
  }
}

/** `invoke` の拒否（`unknown`）を 1 行の原因へ写す。例外は投げない。 */
function describeRejection(cause: unknown): string {
  if (cause instanceof Error) {
    return cause.message;
  }
  if (typeof cause === "string") {
    return cause;
  }
  try {
    return JSON.stringify(cause) ?? String(cause);
  } catch {
    return "原因を文字列化できない拒否";
  }
}

/**
 * 封筒を返すコマンドを呼ぶ入口（要件 4.1）。Tauri の `invoke` へ委譲し、型付けだけを足す。
 *
 * **封筒を返すすべてのコマンドは `IpcResult` そのものを返す**（design.md「CommandSurface」の
 * 「すべてのコマンドは `IpcResult` を返す。例外に頼らない」、要件 4.2・4.4）。`invoke` は
 * その封筒に解決するので、本関数は**解決値を再包装せずそのまま返す**。
 *
 * **例外は生バイトの経路（要件 4.5）だけである。** `bulk_echo` は JSON を通さない
 * `tauri::ipc::Response` で応答するため、封筒へ解決しない。その呼び出しには
 * [`invokeRaw`] を使うこと（本関数を当てると、`application/octet-stream` のバイト列を
 * 封筒として解釈することになり、`status` の分岐が成立しない。tasks.md 7.2）。
 *
 * 戻り値は常に判別可能な合併型である:
 * - 解決した封筒はそのまま返る。成功なら `data` がコマンドのペイロード、失敗なら `error` が
 *   生成物の `IpcError` である。**ドメインの失敗が成功として扱われることはない。**
 * - `invoke` 自体が拒否した場合（未知のコマンド、直列化失敗、IPC 不在）だけ、新しい封筒を
 *   作って `status: "error"` の `kind: "Frontend"` へ写す（要件 4.4）。
 *
 * したがって本関数は例外を外へ出さず、呼び出し側は `status` の 2 つの腕を網羅すればよい。
 * 生成された封筒を加工しないため、`bindings.ts` の型がそのまま実行時の値の意味になる。
 */
export async function invokeCommand<T>(
  command: CommandName,
  args?: InvokeArgs,
): Promise<IpcClientResult<T>> {
  try {
    return await invoke<IpcResult<T, IpcError>>(command, args);
  } catch (cause: unknown) {
    return {
      status: "error",
      error: { kind: "Frontend", detail: { message: describeRejection(cause) } },
    };
  }
}

/**
 * 生バイトで応答するコマンドを呼ぶ入口（要件 4.5。design.md「CommandSurface」の大きな
 * ペイロードの経路）。
 *
 * **封筒を意図的に通らない。** 応答は `tauri::ipc::Response` が `application/octet-stream`
 * として配るため、`invoke` は `ArrayBuffer` へ解決する。`IpcResult` へは解決しないので、
 * [`invokeCommand`] の戻り値（`status` で分岐する合併型）はこの経路には当てはまらない。
 *
 * 代償を明示する:
 * - 封筒の `status: "error"` の腕は存在しない。**拒否（例外）として現れるのは経路そのものの
 *   失敗だけ**（未知のコマンド、IPC の不在など）であり、呼び出し側は `try` / `catch` で扱う。
 *   コマンド自身の誤用は拒否にならない: **中身のあるペイロードを送ったのに 0 バイトが返った
 *   場合、それは空のデータではなく「生バイトでない引数」または「上限超過」の腕である**
 *   （どちらの原因かは Rust 側の記録に警告として残る。`commands/bulk.rs` を参照）。
 * - 引数は**バッファそのもの**を渡す。`{ payload: bytes }` のようにオブジェクトへ入れ子に
 *   すると、Tauri は `Uint8Array` を `Array.from()` で数値の配列へ変換し、JSON として送る
 *   （受け手は生バイトとして扱えず、空の応答と警告になる）。引数全体をバッファにすること。
 * - 送ったバイトと返るバイトは同一である（受け手はエコーする）。100k 行規模のバッチは
 *   **1 回の呼び出し**で渡す。行ごとに呼ぶコマンドは境界に存在しない。
 *
 * @param command 生バイトで応答するコマンド名（`RawCommandName`。生成物の合併型から導く）
 * @param payload 引数全体として送るバッファ（`ArrayBuffer` またはその型付き配列）
 * @returns 応答の生バイト。`application/octet-stream` のため `ArrayBuffer` になる
 */
export async function invokeRaw(
  command: RawCommandName,
  payload: ArrayBuffer | Uint8Array,
): Promise<ArrayBuffer> {
  return await invoke<ArrayBuffer>(command, payload);
}

// ---------------------------------------------------------------------------
// 完了状態の証明（型検査専用の節）
//
// タスク 2.4 の完了状態は「ラッパ経由の呼び出しが型検査を通り、成功と失敗の分岐を網羅
// しないコードが型検査で落ちる」ことである。フロントエンドに試験基盤を置かないため、
// TypeScript 自身の負の型検査の慣用（`// @ts-expect-error`）で示す。
//
// `// @ts-expect-error` は抑止が不要になったとき TS2578 を出す。したがって**この行が
// 在ること自体が、直後のコードが本当に型検査に落ちることの証拠**である。tsconfig.json の
// `noUnusedLocals` があるため、宣言はすべて export する（実行時には使わない。Vite の
// ツリーシェイクで配布物には残らない）。
// ---------------------------------------------------------------------------

/**
 * 正例: ラッパ経由で呼び出し、成功と失敗の両方を扱う。`status` の 2 つの腕を戻り値で
 * 覆っているため `tsc --noEmit` を通る。
 *
 * コマンド名は `CommandName`（生成物から導いた合併型）で受け取る。呼び出し側も名前を
 * 手書きせず、この型で受け渡す。
 */
export async function invokeExample<T>(command: CommandName): Promise<T | string> {
  const result = await invokeCommand<T>(command);
  switch (result.status) {
    case "ok":
      return result.data;
    case "error":
      return describeIpcError(result.error);
  }
}

/**
 * 負例: 成功の腕しか扱わない。失敗の分岐が無いため、`@ts-expect-error` を外すと
 * `tsc --noEmit` が TS2366（関数の終わりに達する経路があり、戻り値の型に `undefined` が
 * 含まれない）で落ちる。
 */
// @ts-expect-error 失敗（`status: "error"`）の分岐を網羅していない
export async function invokeExampleNonExhaustive<T>(command: CommandName): Promise<T> {
  const result = await invokeCommand<T>(command);
  if (result.status === "ok") {
    return result.data;
  }
}

/**
 * 負例: 生成された種別の一部（設定・補助プロセス・ウィンドウ）だけを分岐し、診断・
 * ドキュメントと `Frontend` を放置した利用側。`switch` が 6 種のうち 3 種しか覆わないため、
 * `@ts-expect-error` を外すと `tsc --noEmit` が TS2366 で落ちる。**`kind` の絞り込みが
 * 効いていること**、および `invoke` の拒否の腕が放置できないことの証拠である。
 */
// @ts-expect-error `Frontend`（`invoke` の拒否）の分岐を網羅していない
export function describeGeneratedKindsOnly(error: IpcClientError): string {
  switch (error.kind) {
    case "Settings":
    case "Sidecar":
    case "Window":
      return error.detail.message;
  }
}

/**
 * 負例: 生成された `COMMAND_NAMES` に無い手書きの名前。`CommandName` は配列から導いた
 * 合併型なので、この代入は通りえない。`@ts-expect-error` を外すと TS2322 で落ちる。
 *
 * この定数は型検査専用であり、実行時にも `src-tauri` の登録一覧にも現れない。
 */
// @ts-expect-error 生成された名前定数に無い文字列は `CommandName` に代入できない
export const handWrittenCommandName: CommandName = "not_a_generated_command";

/**
 * 負例: 生バイトで応答しないコマンドを `RawCommandName` へ代入した利用側。
 * `RawCommandName` は生成物の合併型から `bulk_echo` だけを取り出した型なので、
 * 封筒を返す `settings_get` は代入できない。`@ts-expect-error` を外すと TS2322 で落ちる。
 *
 * **生バイトの経路と封筒の経路が型で混ざらない**ことの証拠である（要件 4.5。tasks.md 7.2）。
 */
// @ts-expect-error 封筒を返すコマンドは生バイト経路の入口へ渡せない
export const envelopedCommandIsNotRaw: RawCommandName = "settings_get";
