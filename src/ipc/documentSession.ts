/**
 * ドキュメントセッションの境界の薄いラッパ — 4 つのコマンドと、状態変化の通知の購読。
 *
 * 所有: `DocumentCommands` のフロントエンド側（design.md「Components and Interfaces →
 * Frontend Layer」の `ipc/documentSession.ts`）。要件: 1.6（名前と未保存の読み出し）、
 * 1.7（シート一覧の読み出し）、5.1（保存の指示）、7.1（新規作成の指示）。タスク 4.1。
 *
 * # 何をここに置くか
 *
 * 1. **4 つのコマンドの薄いラッパ**（[`documentState`] / [`documentSave`] / [`documentNew`] /
 *    [`documentDiscard`]）。実体は Rust 側の `DocumentCommands` であり、本モジュールは
 *    `invokeCommand` へ名前と型を添えて渡すだけである。
 * 2. **状態変化の通知の購読**（[`installDocumentSessionChanged`]）。メニューからの
 *    「開く…」「新規」「保存」は Rust 側で完結するため、フロントエンドは結果を知らない。
 *    適応層は状態を変えた操作のあとに対象ウィンドウへ `DOCUMENT_SESSION_CHANGED_EVENT` を
 *    1 回送るので、購読側は**状態を問い合わせ直す**（design.md「セッション状態の通知」）。
 *    ダウンストリームの 4.2（終了前の問い）と 4.3（状態の表示）は、ここから 4 つの呼び出しと
 *    この購読の両方を取る。
 *
 * # 封筒を包み直さない（なぜ薄いままにするか）
 *
 * 4 つのラッパは [`IpcClientResult`] を**そのまま返す**。ここで `data` だけを取り出したり、
 * 失敗を例外へ写し替えたり、独自の `{ ok, value }` へ詰め直したりしない。包み直すと
 * ドメインの失敗（読めなかった・未保存で拒否した・書き出せなかった）が成功として報告され、
 * 呼び出し側は `status` の 2 つの腕を網羅できなくなる（tasks.md 4.1 の本文）。
 * ドメインの結果と経路の失敗の載せ分けは Rust 側の封筒が既に決めており、本モジュールは
 * その意味を変えない。
 *
 * # 要求の型を持たない（ウィンドウは引数で渡さない）
 *
 * 4 つのコマンドは要求の型を持たない。対象ウィンドウは**基盤が注入する `WebviewWindow`
 * から Rust 側が取る**（design.md「DocumentCommands」の「呼び出し元ウィンドウは注入された
 * `WebviewWindow` から取る（ペイロードで受け取らない）」）。したがってラッパも引数を取らない
 * — ウィンドウの識別子を payload で申告する設計へ静かに戻らないことを、引数の不在が型で
 * 固定する（末尾の負の型検査を参照）。
 *
 * # コマンド名とイベント名は生成物から取る
 *
 * コマンド名は `CommandName`（生成物 `src/ipc/bindings.ts` の `COMMAND_NAMES` から導いた
 * 合併型）で型付けした定数として持つ。`crates/app-shell/src/ipc/command_names.rs` から名前が
 * 消えると**このファイルの型検査が落ちる**（`src/shell/closeVeto.ts` と同じ慣用。要件 4.1、
 * 4.2 の「名前の単一の源」）。イベント名も生成物の定数
 * [`DOCUMENT_SESSION_CHANGED_EVENT`] だけを参照し、綴りを手で書かない。
 *
 * # 状態の源は `document_state` ただ 1 つ
 *
 * 通知は「変わった」ことだけを運び、**状態そのものを運ばない**。したがって購読の効果は
 * 「`document_state` を問い合わせ直す」ことに閉じる。イベントの本文は読まない — 読まないこと
 * が最も強い防御であり（本文がどんな値でも再問い合わせは壊れない）、状態の源が 2 つに
 * 割れる余地も残さない。イベント 1 回につき問い合わせ 1 回であり、間引かない（検証は
 * 連続操作での再問い合わせの回数を**呼び出しの形**として数える。design.md の同節）。
 *
 * # 失敗の扱い
 *
 * ラッパも購読も**例外を外へ出さない**。`invokeCommand` は `invoke` の拒否を封筒の
 * エラー腕へ写すので（`src/ipc/client.ts`）、ここで `try`/`catch` を足す必要はない。購読の
 * 登録だけは例外を投げうる（IPC が無い素のブラウザ等）が、記録して戻る — メニューが無い
 * 環境でも画面は開ける（`src/features/diagnostics/requests.ts` と同じ判断）。
 * **`src/shared/` は本ファイルを参照しない**（要件 9.6。`scripts/check-shared-assets.sh` が
 * 機械検査する）。
 */
import { listen } from "@tauri-apps/api/event";

import { DOCUMENT_SESSION_CHANGED_EVENT } from "./bindings";
import type {
  DocumentDiscardResponse,
  DocumentNewResponse,
  DocumentSaveResponse,
  DocumentSessionStatus,
  DocumentStateResponse,
} from "./bindings";
import { invokeCommand } from "./client";
import type { CommandName, IpcClientResult } from "./client";

/**
 * セッションの状態を問い合わせるコマンドの名前（実体は 3.4 の `session/commands.rs`）。
 *
 * **文字列リテラルを `invoke` へ直接渡さない。** 型注釈（[`CommandName`]）は生成物の
 * `COMMAND_NAMES` から導かれた合併型であるため、`crates/app-shell/src/ipc/command_names.rs`
 * からこの名前が消えるとこの行で型検査が落ちる（tasks.md 2.2 / 2.4 の拡張規則）。
 */
const DOCUMENT_STATE_COMMAND: CommandName = "document_state";

/** 保存を指示するコマンドの名前（5.1。出所が無ければ適応層が保存先を提示する）。 */
const DOCUMENT_SAVE_COMMAND: CommandName = "document_save";

/** 新規作成を指示するコマンドの名前（7.1）。 */
const DOCUMENT_NEW_COMMAND: CommandName = "document_new";

/** 破棄の印を指示するコマンドの名前（6.5。未保存を落とす明示の指示）。 */
const DOCUMENT_DISCARD_COMMAND: CommandName = "document_discard";

/**
 * このウィンドウのセッションの状態を問い合わせる（要件 1.6、1.7）。
 *
 * 応答は名前・未保存・シート一覧と、状態の 3 値（保持していない / 保持している /
 * 読み込めなかった理由）を運ぶ。**未解決のウィンドウでは、この問い合わせが起動時に
 * 指定されたドキュメントの読み込みの引き金になる**（design.md「DocumentStateView」の
 * 遅延解決。要件 2.1）。
 *
 * 戻り値は封筒そのものである。読み込めなかったことは**封筒の失敗ではない** — 要求のない
 * 状態 `Unavailable` として成功の腕に載る（`src/ipc/bindings.ts` の
 * [`DocumentSessionStatus`] の doc）。**この 2 つを混ぜて見せないこと**が表示側の責務である。
 */
export async function documentState(): Promise<
  IpcClientResult<DocumentStateResponse>
> {
  return await invokeCommand<DocumentStateResponse>(DOCUMENT_STATE_COMMAND);
}

/**
 * このウィンドウのドキュメントの保存を指示する（要件 5.1）。
 *
 * 出所を持たないドキュメントでは、適応層が保存先の選択を求める（要件 5.2）。応答は
 * `outcome`（保存した / 取り消した / 失敗した）と、保存のあとのセッションの状態
 * （`status`）の両方を運ぶ。**取り消しは失敗ではない**（要件 5.3）ので、`outcome` の 3 値で
 * 分岐し、封筒の失敗と混ぜないこと。
 */
export async function documentSave(): Promise<
  IpcClientResult<DocumentSaveResponse>
> {
  return await invokeCommand<DocumentSaveResponse>(DOCUMENT_SAVE_COMMAND);
}

/**
 * このウィンドウへ新しいドキュメントを作る（要件 7.1）。
 *
 * 応答は `outcome`（作成した / 拒否した理由）を運ぶ。**未保存のドキュメントを保持して
 * いる間は拒否され**（要件 7.3）、その理由は失敗ではなく `outcome` の成功の腕に載る。
 */
export async function documentNew(): Promise<
  IpcClientResult<DocumentNewResponse>
> {
  return await invokeCommand<DocumentNewResponse>(DOCUMENT_NEW_COMMAND);
}

/**
 * このウィンドウの未保存の印を落とす（要件 6.5。利用者の明示の指示である）。
 *
 * ドキュメントと内容は変えず、以後の「閉じてよいか」の答えだけを変える。応答は操作の
 * あとの状態を運ぶので、呼び出し側は未保存が実際に落ちたかを `status` から読める。
 */
export async function documentDiscard(): Promise<
  IpcClientResult<DocumentDiscardResponse>
> {
  return await invokeCommand<DocumentDiscardResponse>(DOCUMENT_DISCARD_COMMAND);
}

/**
 * 状態変化の通知を受け取ったときに呼ばれる関数。引数は**問い合わせ直した封筒**であり、
 * イベントの本文ではない（通知は状態を運ばないため）。
 */
export type DocumentSessionChangeListener = (
  result: IpcClientResult<DocumentStateResponse>,
) => void;

/**
 * `DOCUMENT_SESSION_CHANGED_EVENT` を購読し、届くたびに `document_state` を問い合わせ直して
 * その結果を `onChanged` へ渡す。返る関数は購読を解除する。
 *
 * 形状は `installDiagnosticsRequests`（`src/features/diagnostics/requests.ts`）を写した:
 * 登録は非同期なので、解除が先に来た場合は登録完了を待ってから解除する（`cancelled` と
 * `unlisten` の 2 段）。解除後に解決した問い合わせの結果は捨てる（解除の後に画面へ書き戻すと、
 * アンマウント済みの画面を更新することになる）。
 *
 * **イベントの本文は読まない。** 状態の唯一の源は `document_state` であり、本文を解釈しない
 * ことは最も強い防御である — 本文がどんな値でも（オブジェクトでなくても、境界の型と食い違って
 * いても）再問い合わせは壊れない。診断の導線のように本文で分岐する必要がある場合だけ、
 * 解釈できない値を無視する判断が要る（同ファイルの `parseSection`）。
 *
 * **例外を外へ出さない。** 購読の登録に失敗した場合（IPC が無い素のブラウザ等）は記録だけ
 * して戻る。その場合もラッパは直接呼べるので、画面は自力で状態を読める。
 */
export function installDocumentSessionChanged(
  onChanged: DocumentSessionChangeListener,
): () => void {
  let cancelled = false;
  let unlisten: (() => void) | null = null;

  /** イベント 1 回につき 1 回の問い合わせ（間引かない。冒頭のモジュール doc を参照）。 */
  const refresh = async (): Promise<void> => {
    const result = await documentState();
    if (cancelled) {
      return;
    }
    onChanged(result);
  };

  void (async () => {
    try {
      const stop = await listen<unknown>(DOCUMENT_SESSION_CHANGED_EVENT, () => {
        void refresh();
      });
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    } catch (error: unknown) {
      console.warn("セッションの状態変化を購読できなかった", error);
    }
  })();

  return () => {
    cancelled = true;
    unlisten?.();
  };
}

// ---------------------------------------------------------------------------
// 完了状態の証明（型検査専用の節）
//
// タスク 4.1 の完了状態のうち、機械で示せるのは 2 点である:
//
// (a) 4 つのコマンド名が生成物の `COMMAND_NAMES` に由来すること。**上の 4 つの定数の型注釈
//     そのものが証拠である**（名前が消えれば定数の行で落ちる）。配列に無い手書きの名前を
//     許さないことは `src/ipc/client.ts` の負例が既に示しているので、ここでは重ねない。
// (b) セッションの状態の判別子 `state` と変種の綴り（`"Absent"` / `"Open"` /
//     `"Unavailable"`）がそのまま効き、網羅しない分岐が型検査で落ちること。
//
// フロントエンドに試験基盤を置かないため、`src/ipc/client.ts` と同じ負の型検査の慣用
// （`// @ts-expect-error`）で示す。**抑止が不要になったときは TS2578 で落ちる**ので、この
// 行が在ること自体が、直後のコードが本当に型検査に落ちることの証拠である。tsconfig.json の
// `noUnusedLocals` があるため、宣言はすべて export する（実行時には使わない。Vite の
// ツリーシェイクで配布物には残らない）。
// ---------------------------------------------------------------------------

/**
 * 正例: 3 つの変種を網羅的に分岐する。`state` で絞り込まれるため、`Open` の腕では
 * `name` / `unsaved` / `sheets`（`DocumentSummary`）が読める。網羅しているので
 * `tsc --noEmit` を通る。
 */
export function describeDocumentSessionStatus(
  status: DocumentSessionStatus,
): string {
  switch (status.state) {
    case "Absent":
      return "ドキュメントを保持していない";
    case "Open":
      return `${status.name}（未保存: ${status.unsaved ? "あり" : "なし"}、シート ${status.sheets.length} 件）`;
    case "Unavailable":
      return `読み込めなかった: ${status.reason}`;
  }
}

/**
 * 負例: `Unavailable` の分岐を欠く。`state` で絞り込んだ残りが `never` にならないため、
 * 到達しえない値だけを受ける戻り値の型（`never`）へは戻せない。`@ts-expect-error` を外すと
 * `tsc --noEmit` が TS2322 で落ちる。**`state` による絞り込みが効き、網羅性が型で強制されて
 * いる**ことの証拠である（`src/ipc/client.ts` の `assertNever` と同じ保証を、境界の合併型
 * そのものについて示す）。
 */
export function missingUnavailableVariantIsNotNever(
  status: DocumentSessionStatus,
): never {
  switch (status.state) {
    case "Absent":
      throw new Error("ドキュメントを保持していない");
    case "Open":
      throw new Error(status.name);
  }
  // @ts-expect-error `Unavailable` の分岐が残るため `never` へは絞られない
  return status;
}

/**
 * 負例: 判別子の値を小文字で書いた利用側。生成物の変種名は `"Absent"` / `"Open"` /
 * `"Unavailable"` であり、`"absent"` はどの腕にも一致しない（TS2322）。
 *
 * この 1 行はタスク 3.1 の Implementation Notes が警告する綴りの罠（`case "absent"` と書くと
 * 絞り込みが静かに効かなくなる）を機械で固定する。**境界が小文字の綴りへ変えられたらこの
 * 代入が正当になり、抑止が不要になって型検査が落ちる** — 綴りが黙って変わることを許さない。
 */
export const absentSpelledInLowercase: DocumentSessionStatus = {
  // @ts-expect-error 変種の綴りは `"Absent"` であり小文字の `"absent"` は代入できない
  state: "absent",
};

/**
 * 負例: 呼び出し元ウィンドウを引数で申告する利用側。4 つのコマンドは要求の型を持たず、対象
 * ウィンドウは基盤が注入する `WebviewWindow` から Rust 側が取る（design.md
 * 「DocumentCommands」）。`Parameters` は `[]` になるため、payload を渡す呼び出し方へ
 * 戻す（＝ウィンドウを引数で申告する設計へ戻す）と型検査が落ちる。
 *
 * この宣言は**呼び出さない**（モジュール直下で呼ぶと、抑止された呼び出しが読み込み時に実際に
 * 走ってしまう。`src/ipc/client.ts` の負例が定数の代入に閉じているのと同じ理由）。
 */
// @ts-expect-error 4 つのコマンドは要求の型を持たない（ウィンドウは注入された値から取る）
export const documentStatePayload: Parameters<typeof documentState> = [
  { window: "main" },
];
