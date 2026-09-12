/**
 * ドキュメントを関連付けていないウィンドウの操作導線 — 新規作成と既存ファイルを開く
 * （タスク 9.6。要件 2.2）。
 *
 * 所有: 画面の契約（`src/shell/Layout.tsx` の「画面の契約」）。
 * 要件: 2.2（ドキュメントを指定せずに起動したウィンドウへ両方の操作を提示する）、
 * 2.4（既存ファイルを開く操作を OS 標準のファイル選択の経路につなぐ）、4.6。
 *
 * # 何を提示し、何を提示しないか
 *
 * - **関連付けが無いと判定されたウィンドウ**: 「新規作成」と「既存ファイルを開く…」の 2 つを
 *   提示する。後者は 7.7 の `pick_document_file` を呼び、**選ばれた位置をドキュメント所有者へ
 *   引き渡す経路そのもの**につながる（ファイル選択の実装をこの画面は持たない）。
 * - **関連付けがあると判定されたウィンドウ**: 操作は提示せず、**このスペックがドキュメントの
 *   画面を持たない事実**だけを提示する。ドキュメントの画面（何を作り、どう見せるか）は
 *   ドキュメントを所有する後続のスペックの持ち物であり、この画面はそれを装わない。
 *
 * # 「関連付けが無い」はどう決まるか（**接頭辞ではなく記録から取る**）
 *
 * 判定はコマンド `window_document_state` の答えだけを使う。このコマンドは 6.1 のレジストリが
 * 生成時に記録した関連付け（`document_of`）を読む。**ラベルの接頭辞（`empty-` / `doc-`）から
 * は判定しない** — 接頭辞は割り当て順の規約であって関連付けの事実ではなく、7.7 のファイル
 * 選択が実行時に `DocumentHost::attach` へ位置を引き渡しても**記録された関連付けは書き換わら
 * ない**（6.2 のポート契約にその操作が無い）ため、接頭辞と記録が食い違いうる。この画面が
 * 一手で提示する 2 つの操作のうち「既存ファイルを開く」は、まさにその経路を踏む。
 *
 * **関連付けは生成時に確定する。**したがってこの画面は、表示している間に `attach` が起きた
 * としても判定を引き直さない — 引き直しても記録は同じであり、**見かけの遷移を作る方が嘘に
 * なる**。画面の状態を変えるのは、以後に別のウィンドウが生成され、そのウィンドウがこの画面を
 * 初めて描くときである。
 *
 * # 新規作成が何をするか（**正直に、成功を装わない**）
 *
 * **何も作成しない。** このアプリケーションにはドキュメントを所有する機能がまだ組み込まれて
 * おらず（本スペックはドキュメントの読み書きを所有しない）、6.2 の委譲点
 * （`DocumentHost`）にも作成の操作は無い。したがって操作を選ぶと、**作成できない事実と、
 * 作成機能の持ち主（後続スペック）を利用者へ提示する**。次のいずれもしない:
 *
 * - 成功したように見せること（偽の成功・空のウィンドウ・無言の no-op）。
 * - デタラメな位置のドキュメントを作り、`attach` へ渡すこと。
 * - 6.2 のポート契約へ「作成」を足すこと（契約変更は design.md の Revalidation Trigger で
 *   あり、下流スペックの接続点を本タスクの都合で動かすことになる）。
 *
 * この操作はコマンドを呼ばない（記録に残す先が無い）。**フロントエンドから記録へ書く経路は
 * 存在しない**（9.3 が同じ理由で診断画面を記録へつないでいない）。提示は利用者に見える形で
 * 行う。
 *
 * # 既存ファイルを開くの結果の見せ方（7.7 の契約）
 *
 * 応答は封筒（[`IpcClientResult`]）であり、`outcome` の 3 値と封筒の失敗を**別々に**扱う:
 *
 * | 結果 | 見せ方 |
 * |---|---|
 * | `Attached` | 選ばれた位置を所有者へ引き渡した事実（**パス自体は応答に無い**。7.7 の契約） |
 * | `Cancelled` | **正常な結果**。取り消したこと、何も起きていないこと |
 * | `Rejected` | 所有者が受け取らなかったこと。理由（`reason`）を添える |
 * | 封筒の失敗 | 選択手段を提示できなかったこと（IPC の失敗・親の消失）として区別する |
 *
 * **封筒の失敗を `Rejected` と混ぜない。** 混ぜると「所有者が断った」と「通信が壊れた」が
 * 同じ見え方になり、利用者の次の行動を誤らせる（7.6 が `Deny` を成功側へ置いたのと同じ判断）。
 *
 * # 失敗の見せ方（例外を描画へ出さない）
 *
 * 封筒の失敗はこの画面の状態として提示する。**例外を投げない** — 描画中に投げると 9.3 の
 * 画面単位のエラー隔離が発動し、この画面の内容がまるごとエラーの提示へ置き換わる。ここで
 * 扱える失敗はここで出す。エラー隔離は、この画面の描画そのものが壊れたときの最後の砦である。
 */
import { useCallback, useEffect, useState, type ReactElement } from "react";

import type {
  PickDocumentFileResponse,
  WindowDocumentState,
  WindowDocumentStateResponse,
} from "../../ipc/bindings";
import {
  assertNever,
  describeIpcError,
  invokeCommand,
  type CommandName,
} from "../../ipc/client";
import { APPEARANCE_VARS } from "../../shell/theme";

/**
 * この画面の識別子。`src/shell/Layout.tsx` のレジストリと画面が同じ綴りを使うための単一の
 * 定義である。
 */
export const EMPTY_WINDOW_SCREEN_ID = "empty-window";

/**
 * 関連付けの問い合わせのコマンド名（実体は `src-tauri/src/window/association.rs`）。
 *
 * **文字列リテラルを `invoke` へ渡さない。** 型注釈（[`CommandName`]）は生成物の
 * `COMMAND_NAMES` から導かれた合併型であるため、`crates/app-shell/src/ipc/command_names.rs`
 * からこの名前が消えるとこの行で型検査が落ちる（tasks.md 2.2）。
 */
const WINDOW_DOCUMENT_STATE_COMMAND: CommandName = "window_document_state";

/** 既存ファイルを開くコマンド名（実体は 7.7 の `src-tauri/src/dialog.rs`）。 */
const PICK_DOCUMENT_FILE_COMMAND: CommandName = "pick_document_file";

/**
 * 新規作成を選んだときに提示する文言。**ドキュメント所有機能が未搭載である事実と、その
 * 持ち主を述べる**（成功を装わない。モジュール doc「新規作成が何をするか」）。
 */
const NEW_DOCUMENT_UNAVAILABLE =
  "このアプリケーションには、ドキュメントを所有する機能がまだ組み込まれていません。" +
  "そのため新規作成はできません。ドキュメントを作成する機能は後続のスペックが提供します。";

/**
 * 関連付けがあるウィンドウに提示する文言。**ドキュメントの画面を装わない**（モジュール doc
 * 「何を提示し、何を提示しないか」）。
 */
const ASSOCIATED_NOTE =
  "このウィンドウのドキュメントの画面は、ドキュメントを所有する後続のスペックが提供します。" +
  "このシェルはドキュメントの内容を解釈する画面を持ちません。";

/** 関連付けの問い合わせの状態。 */
type AssociationState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly state: WindowDocumentState }
  | { readonly status: "failed"; readonly message: string };

/** 既存ファイルを開く操作の状態（7.7 の `outcome` と封筒の失敗を区別して持つ）。 */
type OpenState =
  | { readonly status: "idle" }
  | { readonly status: "running" }
  | { readonly status: "attached" }
  | { readonly status: "cancelled" }
  | { readonly status: "rejected"; readonly reason: string }
  | { readonly status: "failed"; readonly message: string };

/** 新規作成の状態。**成功の状態を持たない**（作成する機能が無いため）。 */
type CreateState =
  | { readonly status: "idle" }
  | { readonly status: "unavailable" };

/** 画面の枠。シェルの配色（`APPEARANCE_VARS`）だけを参照する。 */
const PANEL_STYLE = {
  padding: "1.5rem 2rem",
  borderRadius: "0.5rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
  textAlign: "left",
  width: "min(40rem, 100%)",
} as const;

/** 補助的な説明の見た目。 */
const MUTED_STYLE = {
  color: `var(${APPEARANCE_VARS.screenMuted})`,
  fontSize: "0.8125rem",
} as const;

/** 操作のボタン。 */
function Action({
  testId,
  label,
  disabled,
  onClick,
}: {
  readonly testId: string;
  readonly label: string;
  readonly disabled: boolean;
  readonly onClick: () => void;
}): ReactElement {
  return (
    <button
      type="button"
      data-testid={testId}
      disabled={disabled}
      onClick={onClick}
      style={{
        font: "inherit",
        fontSize: "0.9375rem",
        padding: "0.45rem 1.1rem",
        borderRadius: "0.25rem",
        cursor: disabled ? "default" : "pointer",
        color: `var(${APPEARANCE_VARS.controlActiveText})`,
        backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
        border: `1px solid var(${APPEARANCE_VARS.controlActiveText})`,
      }}
    >
      {label}
    </button>
  );
}

/** 操作の結果・失敗の 1 行。**成功と失敗を同じ見え方にしない**（要件 2.2 の提示）。 */
function ResultLine({
  testId,
  text,
  tone,
}: {
  readonly testId: string;
  readonly text: string;
  readonly tone: "neutral" | "failure";
}): ReactElement {
  return (
    <p
      data-testid={testId}
      role={tone === "failure" ? "alert" : undefined}
      style={{
        margin: "0.75rem 0 0",
        fontSize: "0.875rem",
        color:
          tone === "failure"
            ? `var(${APPEARANCE_VARS.controlActiveText})`
            : `var(${APPEARANCE_VARS.screenText})`,
      }}
    >
      {text}
    </p>
  );
}

/**
 * ドキュメントを関連付けていないウィンドウの操作導線。**`ScreenProps` 以外の props を受け
 * 取らない**（画面の契約。`src/shell/Layout.tsx`）。
 */
export function EmptyWindowScreen(): ReactElement {
  const [association, setAssociation] = useState<AssociationState>({
    status: "loading",
  });
  const [open, setOpen] = useState<OpenState>({ status: "idle" });
  const [create, setCreate] = useState<CreateState>({ status: "idle" });

  /** 関連付けを問い合わせる（**例外を外へ出さない**。封筒の失敗は画面の状態として持つ）。 */
  const loadAssociation = useCallback(async (): Promise<void> => {
    setAssociation({ status: "loading" });
    const result = await invokeCommand<WindowDocumentStateResponse>(
      WINDOW_DOCUMENT_STATE_COMMAND,
    );
    setAssociation(
      result.status === "ok"
        ? { status: "ready", state: result.data.state }
        : { status: "failed", message: describeIpcError(result.error) },
    );
  }, []);

  // 画面が現れた時点で、このウィンドウの関連付けを提示する（利用者が操作しなくても見える）。
  useEffect(() => {
    void loadAssociation();
  }, [loadAssociation]);

  /**
   * 既存ファイルを開く。**7.7 のコマンドをそのまま呼ぶ**（選択手段の実装をこの画面は持たず、
   * 親ウィンドウも payload で申告しない。Rust 側が注入されたウィンドウから取る）。
   */
  const openDocument = useCallback(async (): Promise<void> => {
    setOpen({ status: "running" });
    const result = await invokeCommand<PickDocumentFileResponse>(
      PICK_DOCUMENT_FILE_COMMAND,
    );
    if (result.status !== "ok") {
      // 封筒の失敗（IPC の失敗・親の消失）。**`Rejected` とは別の見せ方**にする。
      setOpen({ status: "failed", message: describeIpcError(result.error) });
      return;
    }
    switch (result.data.outcome.outcome) {
      case "Attached":
        setOpen({ status: "attached" });
        return;
      case "Cancelled":
        setOpen({ status: "cancelled" });
        return;
      case "Rejected":
        setOpen({ status: "rejected", reason: result.data.outcome.reason });
        return;
      default:
        // 境界の `outcome` に値が増えたらここでコンパイルエラーになる。
        assertNever(result.data.outcome);
    }
  }, []);

  /** 新規作成。**作成する機能が無い事実を提示するだけで、何も作成しない**（モジュール doc）。 */
  const requestNewDocument = useCallback((): void => {
    setCreate({ status: "unavailable" });
  }, []);

  return (
    <section
      data-testid="jxcel-empty-window"
      data-document-state={
        association.status === "ready" ? association.state : association.status
      }
      aria-label="ドキュメント"
      style={PANEL_STYLE}
    >
      {association.status === "loading" ? (
        <p data-testid="jxcel-empty-state" style={{ margin: 0 }}>
          このウィンドウの状態を確認しています…
        </p>
      ) : null}

      {association.status === "failed" ? (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            このウィンドウの状態を確認できません
          </h2>
          <ResultLine
            testId="jxcel-empty-state-error"
            text={association.message}
            tone="failure"
          />
          <div style={{ marginTop: "0.75rem" }}>
            <Action
              testId="jxcel-empty-state-retry"
              label="再試行"
              disabled={false}
              onClick={() => {
                void loadAssociation();
              }}
            />
          </div>
        </>
      ) : null}

      {association.status === "ready" &&
      association.state === "unassociated" ? (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            ドキュメントが関連付けられていません
          </h2>
          <p style={{ margin: "0 0 1rem" }}>
            新規作成するか、既存のファイルを開いてください。
          </p>
          <div style={{ display: "flex", gap: "0.75rem", flexWrap: "wrap" }}>
            <Action
              testId="jxcel-empty-new-document"
              label="新規作成"
              disabled={false}
              onClick={requestNewDocument}
            />
            <Action
              testId="jxcel-empty-open-document"
              label="既存ファイルを開く…"
              disabled={open.status === "running"}
              onClick={() => {
                void openDocument();
              }}
            />
          </div>

          {create.status === "unavailable" ? (
            <ResultLine
              testId="jxcel-empty-new-document-result"
              text={NEW_DOCUMENT_UNAVAILABLE}
              tone="neutral"
            />
          ) : null}

          {open.status === "attached" ? (
            <ResultLine
              testId="jxcel-empty-open-result"
              text="選ばれた位置を、ドキュメントを所有する機能へ引き渡しました。"
              tone="neutral"
            />
          ) : null}
          {open.status === "cancelled" ? (
            <ResultLine
              testId="jxcel-empty-open-result"
              text="選択を取り消しました。引き渡しは行われていません。"
              tone="neutral"
            />
          ) : null}
          {open.status === "rejected" ? (
            <ResultLine
              testId="jxcel-empty-open-result"
              text={`ドキュメントを所有する機能が引き渡しを受け取りませんでした（理由: ${open.reason}）。`}
              tone="failure"
            />
          ) : null}
          {open.status === "failed" ? (
            <ResultLine
              testId="jxcel-empty-open-error"
              text={`ファイル選択を提示できませんでした: ${open.message}`}
              tone="failure"
            />
          ) : null}
        </>
      ) : null}

      {association.status === "ready" &&
      association.state === "associated" ? (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            ドキュメントが関連付けられています
          </h2>
          <p data-testid="jxcel-empty-associated-note" style={MUTED_STYLE}>
            {ASSOCIATED_NOTE}
          </p>
        </>
      ) : null}
    </section>
  );
}
