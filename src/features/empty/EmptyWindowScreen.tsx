/**
 * ドキュメントのセッションの状態 — 名前・未保存の有無・シートの一覧と、2 つの操作導線
 * （新規作成 / 既存ファイルを開く。タスク 4.3。要件 1.6、1.7、2.1）。
 *
 * 所有: 画面の契約（`src/shell/Layout.tsx` の「画面の契約」）、`DocumentStateView`
 * （design.md「Components and Interfaces → frontend」）。
 * 要件: 1.6（名前と未保存の読み出し）、1.7（シート一覧の読み出し）、2.1（読み込めなかった
 * 理由の報告）。既存ファイルを開く導線は 2.4 の経路をそのまま使う。
 *
 * # 何を提示するか（セッションの状態 3 値 + 経路の失敗）
 *
 * 判定の材料は `document_state` の答え**だけ**である。3 値と、それに足す 1 つを次へ写す:
 *
 * | 状態 | 提示 |
 * |---|---|
 * | `Absent` | 保持していない事実と、「新規作成」「既存ファイルを開く…」の 2 操作（要件 2.2） |
 * | `Open` | ドキュメントの名前・未保存の有無・シートの一覧（名前と行数。要件 1.6、1.7） |
 * | `Unavailable` | 読み込めなかった理由（要件 2.1）。**再試行は出さない** — 失敗はセッションが覚えており、同じ問い合わせは同じ答えを返す（読み込みは 1 回だけ。design.md「起動時に指定されたドキュメントの解決」）。回復の道は 2 操作（別のドキュメントを作る・開く）である |
 * | 封筒の失敗 | 経路そのものの失敗（IPC 不在・親の消失）。理由と再試行を提示する |
 *
 * `Open` は**要約だけ**を提示する。何を作り、どう見せるか（表・編集）はドキュメントを所有する
 * 後続スペックの持ち物であり、この画面はそれを装わない。
 *
 * **2 操作は問い合わせが成功したどの状態でも出す。**保持しているウィンドウで「開く」を選ぶと、
 * 未保存なら所有者が拒否し（`Rejected`）、その理由が結果行に出る。提示を消して選べなくするより、
 * 断られた事実を見せる方が利用者の次の行動を決められる（要件 2.2 が操作を求める相手は
 * 「ドキュメントを関連付けていないウィンドウ」だが、状態の写しが「保持している」でも
 * 操作を隠す理由にはならない）。
 *
 * # 判定はセッションの状態だけを使う（関連付けの記録は使わない）
 *
 * 以前のこの画面は app-shell の `window_document_state`（生成要求の記録）で「関連付けの有無」を
 * 判定していた。**その判定を `document_state` へ置き換えた**（design.md「既存の関連付け
 * （`window_document_state`）との関係」）。2 つは別の問いである:
 *
 * - `window_document_state`: **生成要求**に関連付けがあるか。`attach` でも新規作成でも更新されない
 * - `document_state`: **今**そのウィンドウがドキュメントを保持しているか、未保存か、どのシートがあるか
 *
 * **開いているドキュメントの真実はセッションである。**関連付けの記録で判定すると、ファイル選択の
 * 直後（引き渡しが成立してセッションが保持している）や新規作成の直後に「関連付けなし」と
 * 「保持している」が同居し、どちらが本当か画面から読めなくなる。**app-shell のコマンド自体は
 * 削除しない** — 生成要求の記録は app-shell の成果物であり、この画面が使わなくなっただけである。
 *
 * # この問い合わせが起動時の読み込みの引き金になる
 *
 * 起動引数で指定された位置のドキュメントは、**ウィンドウの最初の `document_state` が遅延解決の
 * 引き金**である（design.md「起動時に指定されたドキュメントの解決」。要件 2.1）。したがってこの
 * 画面はマウント時に必ず 1 回問い合わせる。**この画面を初期画面に残す限り、起動経路の読み込みは
 * 必ず 1 回起きる**（`src/shell/Layout.tsx` のレジストリがこの画面を `initial` にしている）。
 *
 * # 状態が変わったら問い合わせ直す（仕掛けは購読の 1 つだけ）
 *
 * メニューからの「保存」「新規」「開く」は **Rust 側で完結**し、この画面は結果を知らない。
 * そこで適応層が、状態を変えた操作のあとに対象ウィンドウへ `document_session_changed` を
 * 1 回送る。**この画面はそれを購読して問い合わせ直す**（design.md「セッション状態の通知」）。
 * 送る側は 3 箇所に分かれているが、どれも「状態を変えたあとに 1 回」という同じ規則に従う:
 *
 * | 経路 | 送る場所 |
 * |---|---|
 * | メニューの「保存」「新規」 | `src-tauri/src/session/menu.rs` |
 * | メニューの「開く…」 | `src-tauri/src/dialog.rs`〔`hand_off`。引き渡しが成立したときだけ〕 |
 * | コマンドの 4 つ（この画面の「新規作成」「既存ファイルを開く…」を含む） | `src-tauri/src/session/commands.rs` |
 *
 * **この画面は自分が駆動しない経路のために、自前の再問い合わせを持たない。** 以前は
 * 「既存ファイルを開く…」の成功後にここで問い合わせ直していたが、`hand_off` が通知を送る
 * ようになった時点で二重になり、片方（購読）が壊れても気づけない形になる。**状態の源は 1 つ、
 * 問い合わせ直す引き金も 1 つ**に保つ — 画面は購読だけを張り、どの経路が状態を変えたかを
 * 知ろうとしない。
 *
 * **イベントは状態を運ばない** — 状態の唯一の源は `document_state` である。また**通知のたびに
 * 「確認しています…」へ戻さない**（購読は [`applySession`] を直に呼び、読み込みの最中を示すのは
 * 初回と利用者が押した再試行だけである）。解除の後に解決した結果は購読側が捨てる
 * （`src/ipc/documentSession.ts` の契約）。
 *
 * # 新規作成は本物のコマンドを呼ぶ（以前の「未搭載」の提示は消した）
 *
 * この画面は以前「このアプリケーションには、ドキュメントを所有する機能がまだ組み込まれて
 * いません」と提示していた（当時それは事実だった）。**今は事実に反する** — `document_new`
 * が存在し、行も列も無いシートを 1 つ持つドキュメントを用意する（要件 7.1）。したがって
 * コマンドを呼び、`Created` と `Refused{reason}` を**別々に**提示する:
 *
 * | 結果 | 見せ方 |
 * |---|---|
 * | `Created` | 用意した事実。以後の表示は応答の `status`（作成後の状態そのもの）へ差し替える |
 * | `Refused` | **失敗ではない**。未保存の変更があるため作れなかったというドメインの答えであり、理由を添える（要件 7.3） |
 * | 封筒の失敗 | 経路の失敗として理由を添える |
 *
 * **応答の `status` を使い、通知の再問い合わせを待たない。** 応答は作成後の状態を運んでおり、
 * これをそのまま画面の状態にすれば、表示が 1 往復分古いままになる瞬間が無い。
 *
 * # 既存ファイルを開くの結果の見せ方（7.7 の契約）
 *
 * 応答は封筒（[`IpcClientResult`]）であり、`outcome` の 3 値と封筒の失敗を**別々に**扱う:
 *
 * | 結果 | 見せ方 |
 * |---|---|
 * | `Attached` | 選ばれた位置を所有者へ引き渡した事実（**パス自体は応答に無い**。7.7 の契約）。引き渡しの成立は `dialog::hand_off` が通知するので、**画面の状態は購読の再問い合わせで更新される**（この画面は自分で問い合わせ直さない） |
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
 *
 * # 配色と契約
 *
 * 配色は器が与えるカスタムプロパティ（`APPEARANCE_VARS` の `var(--jxcel-*)`）**だけ**を参照し、
 * 自前のレイアウトと遷移を持たない。`ScreenProps` 以外の props を受け取らない
 * （画面の契約。`src/shell/Layout.tsx`）。
 */
import { useCallback, useEffect, useState, type ReactElement } from "react";

import type {
  DocumentSessionStatus,
  DocumentSheet,
  DocumentStateResponse,
  PickDocumentFileResponse,
} from "../../ipc/bindings";
import {
  assertNever,
  describeIpcError,
  invokeCommand,
  type CommandName,
  type IpcClientResult,
} from "../../ipc/client";
import {
  documentNew,
  documentState,
  installDocumentSessionChanged,
} from "../../ipc/documentSession";
import { APPEARANCE_VARS } from "../../shell/theme";

/**
 * この画面の識別子。`src/shell/Layout.tsx` のレジストリと画面が同じ綴りを使うための単一の
 * 定義である。
 */
export const EMPTY_WINDOW_SCREEN_ID = "empty-window";

/**
 * 既存ファイルを開くコマンド名（実体は 7.7 の `src-tauri/src/dialog.rs`）。
 *
 * **文字列リテラルを `invoke` へ渡さない。** 型注釈（[`CommandName`]）は生成物の
 * `COMMAND_NAMES` から導かれた合併型であるため、`crates/app-shell/src/ipc/command_names.rs`
 * からこの名前が消えるとこの行で型検査が落ちる（tasks.md 2.2）。
 *
 * **関連付けの問い合わせ（`window_document_state`）の定数はここに無い。** この画面は
 * `document_state` を `src/ipc/documentSession.ts` のラッパ経由で呼ぶ（モジュール doc
 * 「判定はセッションの状態だけを使う」）。
 */
const PICK_DOCUMENT_FILE_COMMAND: CommandName = "pick_document_file";

/**
 * 名前を持たないドキュメントの見せ方。新規作成直後のドキュメントは名前が空文字である
 * （`DocumentSummary` の契約。ファイル名のみを運び、新規は空文字）。
 */
const UNNAMED_DOCUMENT = "（無題）";

/** セッションの状態の問い合わせの状態。 */
type SessionState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly state: DocumentSessionStatus }
  | { readonly status: "failed"; readonly message: string };

/** 既存ファイルを開く操作の状態（7.7 の `outcome` と封筒の失敗を区別して持つ）。 */
type OpenState =
  | { readonly status: "idle" }
  | { readonly status: "running" }
  | { readonly status: "attached" }
  | { readonly status: "cancelled" }
  | { readonly status: "rejected"; readonly reason: string }
  | { readonly status: "failed"; readonly message: string };

/** 新規作成の状態（`Created` / `Refused` / 封筒の失敗を区別して持つ）。 */
type CreateState =
  | { readonly status: "idle" }
  | { readonly status: "running" }
  | { readonly status: "created" }
  | { readonly status: "refused"; readonly reason: string }
  | { readonly status: "failed"; readonly message: string };

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
 * セッションの状態を提示へ写す。**3 つの変種を網羅的に分岐する**（`default` の
 * [`assertNever`] が、境界に変種が増えたときここをコンパイルエラーにする。`state` の綴りを
 * 小文字で書くと絞り込みが効かず、同じ場所で落ちる）。
 *
 * 状態ごとの見せ方はモジュール doc の表が唯一の定義である。
 */
function SessionBody({
  state,
}: {
  readonly state: DocumentSessionStatus;
}): ReactElement {
  switch (state.state) {
    case "Absent":
      return (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            ドキュメントを保持していません
          </h2>
          <p style={{ margin: "0 0 1rem" }}>
            新規作成するか、既存のファイルを開いてください。
          </p>
        </>
      );
    case "Open":
      return (
        <DocumentSummaryView
          name={state.name}
          unsaved={state.unsaved}
          sheets={state.sheets}
        />
      );
    case "Unavailable":
      return (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            このウィンドウのドキュメントを読み込めませんでした
          </h2>
          {/* 読み込めなかった理由は**既存の結果行の形**で出す（要件 2.1。design.md
              「DocumentStateView」）。再試行は出さない — 失敗はセッションが覚えており、
              同じ問い合わせは同じ答えを返す（モジュール doc の表）。 */}
          <ResultLine
            testId="jxcel-empty-state-unavailable"
            text={state.reason}
            tone="failure"
          />
        </>
      );
    default:
      return assertNever(state, "セッションの状態の分岐が網羅されていない");
  }
}

/** 保持しているドキュメントの要約（要件 1.6、1.7）。**名前・未保存・シートだけ**を出す。 */
function DocumentSummaryView({
  name,
  unsaved,
  sheets,
}: {
  readonly name: string;
  readonly unsaved: boolean;
  readonly sheets: readonly DocumentSheet[];
}): ReactElement {
  return (
    <>
      <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
        このウィンドウのドキュメント
      </h2>
      <p
        data-testid="jxcel-empty-document-name"
        style={{ margin: "0 0 0.25rem", fontSize: "0.9375rem" }}
      >
        {`名前: ${name === "" ? UNNAMED_DOCUMENT : name}`}
      </p>
      <p
        data-testid="jxcel-empty-document-unsaved"
        style={{ margin: "0 0 0.75rem", ...MUTED_STYLE }}
      >
        {`未保存の変更: ${unsaved ? "あり" : "なし"}`}
      </p>
      <p style={{ margin: "0 0 0.25rem", ...MUTED_STYLE }}>{"シート"}</p>
      <ul
        data-testid="jxcel-empty-sheet-list"
        style={{ margin: 0, paddingLeft: "1.25rem", fontSize: "0.875rem" }}
      >
        {sheets.map((sheet) => (
          // 鍵はシートの識別子である（名前は重複しうる。境界の契約）。
          <li key={sheet.id} data-testid="jxcel-empty-sheet">
            {`${sheet.name}（${sheet.rows} 行）`}
          </li>
        ))}
      </ul>
    </>
  );
}

/**
 * ドキュメントのセッションの状態と、2 つの操作導線。**`ScreenProps` 以外の props を受け
 * 取らない**（画面の契約。`src/shell/Layout.tsx`）。
 */
export function EmptyWindowScreen(): ReactElement {
  const [session, setSession] = useState<SessionState>({ status: "loading" });
  const [open, setOpen] = useState<OpenState>({ status: "idle" });
  const [create, setCreate] = useState<CreateState>({ status: "idle" });

  /** 封筒を画面の状態へ写す。**封筒を包み直さない**（`src/ipc/documentSession.ts` の契約）。 */
  const applySession = useCallback(
    (result: IpcClientResult<DocumentStateResponse>): void => {
      setSession(
        result.status === "ok"
          ? { status: "ready", state: result.data.status }
          : { status: "failed", message: describeIpcError(result.error) },
      );
    },
    [],
  );

  /**
   * セッションの状態を問い合わせ、「確認しています…」から始める（**例外を外へ出さない**。
   * 封筒の失敗は画面の状態として持つ）。
   *
   * **通知からの再問い合わせはこれを使わない。** 購読は [`applySession`] を直に呼ぶので、
   * メニュー操作のたびに画面が「確認しています…」へ戻ることはない（読み込みの最中を示すのは
   * 初回と、利用者が明示的に押した再試行だけである。モジュール doc「状態が変わったら
   * 問い合わせ直す」）。
   */
  const loadSession = useCallback(async (): Promise<void> => {
    setSession({ status: "loading" });
    applySession(await documentState());
  }, [applySession]);

  // 画面が現れた時点で 1 回問い合わせる。**この問い合わせが起動時に指定されたドキュメントの
  // 読み込みの引き金である**（遅延解決。モジュール doc「この問い合わせが起動時の読み込みの
  // 引き金になる」）。
  useEffect(() => {
    void loadSession();
  }, [loadSession]);

  // 状態変化の通知を購読して問い合わせ直す。**この 1 つの仕掛けが、この画面が駆動しない経路を
  // すべて覆う** — メニューの「保存」「新規」は `session/menu.rs` が、メニューの「開く…」は
  // `dialog::hand_off` が、コマンド経路の 4 つは `session/commands.rs` が、それぞれ状態を
  // 変えたあとに 1 回送る（design.md「セッション状態の通知」）。解除は購読側が返す関数を
  // そのまま effect の後片付けに使う（`src/ipc/documentSession.ts`）。
  useEffect(() => {
    return installDocumentSessionChanged(applySession);
  }, [applySession]);

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

  /** 新規作成。**本物のコマンドを呼び、`Created` と `Refused` を別々に提示する**（モジュール doc）。 */
  const requestNewDocument = useCallback(async (): Promise<void> => {
    setCreate({ status: "running" });
    const result = await documentNew();
    if (result.status !== "ok") {
      setCreate({ status: "failed", message: describeIpcError(result.error) });
      return;
    }
    // 応答の `status` は**作成後の状態そのもの**である。通知の再問い合わせを待たずに反映する
    // （待つと、表示が 1 往復分古いままになる瞬間ができる）。
    setSession({ status: "ready", state: result.data.status });
    setCreate(
      result.data.outcome.outcome === "Created"
        ? { status: "created" }
        : // **拒否は失敗ではない**（要件 7.3）。理由を添えて正常な結果として出す。
          { status: "refused", reason: result.data.outcome.reason },
    );
  }, []);

  return (
    <section
      data-testid="jxcel-empty-window"
      // 外から読める観測点。**セッションの状態の判別子**（`Absent` / `Open` / `Unavailable`）と、
      // 問い合わせ自体の状態（`loading` / `failed`）を出す。関連付けの記録ではない
      // （モジュール doc「判定はセッションの状態だけを使う」）。
      data-document-state={
        session.status === "ready" ? session.state.state : session.status
      }
      aria-label="ドキュメント"
      style={PANEL_STYLE}
    >
      {session.status === "loading" ? (
        <p data-testid="jxcel-empty-state" style={{ margin: 0 }}>
          このウィンドウの状態を確認しています…
        </p>
      ) : null}

      {session.status === "failed" ? (
        <>
          <h2 style={{ margin: "0 0 0.5rem", fontSize: "1.125rem" }}>
            このウィンドウの状態を確認できません
          </h2>
          <ResultLine
            testId="jxcel-empty-state-error"
            text={session.message}
            tone="failure"
          />
          <div style={{ marginTop: "0.75rem" }}>
            <Action
              testId="jxcel-empty-state-retry"
              label="再試行"
              disabled={false}
              onClick={() => {
                void loadSession();
              }}
            />
          </div>
        </>
      ) : null}

      {session.status === "ready" ? <SessionBody state={session.state} /> : null}

      {/* 2 操作は「保持していない」「保持している」「読み込めなかった」のいずれでも出す。
          保持しているウィンドウでは、未保存なら「開く」が所有者に拒否され（`Rejected`）、
          その理由が結果行に出る — 提示を消して選べなくするより、断られた事実を見せる方が
          利用者の次の行動を決められる。 */}
      {session.status === "ready" ? (
        <>
          <div
            style={{
              display: "flex",
              gap: "0.75rem",
              flexWrap: "wrap",
              marginTop: "1rem",
            }}
          >
            <Action
              testId="jxcel-empty-new-document"
              label="新規作成"
              disabled={create.status === "running"}
              onClick={() => {
                void requestNewDocument();
              }}
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

          {create.status === "created" ? (
            <ResultLine
              testId="jxcel-empty-new-document-result"
              text="新しいドキュメントを用意しました。"
              tone="neutral"
            />
          ) : null}
          {create.status === "refused" ? (
            <ResultLine
              testId="jxcel-empty-new-document-result"
              text={`新しいドキュメントを用意できませんでした（理由: ${create.reason}）。`}
              tone="failure"
            />
          ) : null}
          {create.status === "failed" ? (
            <ResultLine
              testId="jxcel-empty-new-document-error"
              text={`新しいドキュメントを用意できませんでした: ${create.message}`}
              tone="failure"
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
    </section>
  );
}
