/**
 * マクロの実行の面のパネル（tasks.md 4.4。要件 1.3、1.4、2.1、2.3、2.4、2.5、2.7、8.2、9.1、
 * 9.2、9.3）。
 *
 * # どこに載るか（**表と同じ画面の中のバーである**）
 *
 * 本パネルは**文書を映している画面（グリッド画面）の中へ、表と並べて**載る。
 * `src/features/grid/GridScreen.tsx` の `GridScreenView` が [`MacroSurfaceBinding`] を
 * 受け取ったときにだけ描く（検査が状態だけを読むときは省略できる）。
 *
 * **対話の窓も覆いも作らない。** 実行は秒単位かかりうるので（要件 6.1）、実行中に画面を覆う
 * 提示を出すと**表の表示と操作が止まる**（要件 2.2 が禁じている）。本パネルが実行中に出すのは
 * **1 行の「実行中」**（`data-macro-running`）だけであり、表も一覧もそのまま操作できる。
 * 実行そのものは境界の非同期呼び出しであり（`src-tauri` は `spawn_blocking` の上で走らせる）、
 * 画面は結果を待って止まらない。
 *
 * # 4 つの流れ（要件の順に読める形にしてある）
 *
 * | 流れ | どこ | 何が出るか |
 * |---|---|---|
 * | 一覧の提示（1.3、1.4） | 一覧の区画 | 名前・種別・宣言している能力。**解釈できなかった 1 件も残り**、層と理由とフレームが出る |
 * | 能力の提示（8.2） | 選択の区画 | 選ばれた 1 件の**宣言している能力**と、実行の操作 |
 * | 実行（2.1、2.2） | 実行中の 1 行 | 「実行中」と名前（**表は止めない**） |
 * | 結果（2.3、2.4、2.5、6.1、6.2、9.1、9.2、9.3） | 結果の区画 | 成功・失敗・打ち切り・経路の失敗の 4 つが**それぞれ固有の形**で出る |
 *
 * # 配色（画面の契約）
 *
 * 器が `<html>` に与えるカスタムプロパティ（`src/shell/theme.ts` の `APPEARANCE_VARS`）だけを
 * 参照し、自前の配色を持たない。**形は `src/features/grid/GridScreen.tsx` の同じ名の定数に
 * 合わせてある**（2 つ目の流儀を作らない）— あちらは module 私有であり、本 module から取り込むと
 * グリッド画面 → 本パネル → グリッド画面の相互の取り込みになるため、値だけを写している。
 */
import { useEffect, useSyncExternalStore, type ReactElement } from "react";

import { assertNever } from "../../ipc/client";
import { APPEARANCE_VARS } from "../../shell/theme";
import { installMacroListRefresh } from "./requests";
import { createMacroClient } from "./macroClient";
import { createMacroSurfaceStore, type MacroSurfaceStore } from "./store";
import {
  canPresentRun,
  changeTotal,
  chosenSummary,
  describeAbortLimit,
  describeChanges,
  describeFailureLayer,
  describeFrame,
  describeKind,
  describeOutputLine,
  failurePresentation,
  runnableMacros,
  type MacroFailurePresentation,
  type MacroResultPresentation,
  type MacroSurfaceState,
  type SettledMacroRun,
} from "./surface";

/** 実行中であることを外から読むための属性（検査と、実行中かどうかの表明）。 */
const RUNNING_ATTR = "data-macro-running";

/** パネルが受け取る結び付け。**状態の保持と、作り直しの入口だけである。** */
export interface MacroSurfaceBinding {
  /** 面の状態の保持（[`./store`]）。 */
  readonly store: MacroSurfaceStore;
  /**
   * **変更が適用された**ときに表示を作り直す入口（要件 2.5）。
   *
   * 呼ぶのは、実行が `Ran` で終わり、かつ変更の件数が 1 件以上であるときだけである。グリッド
   * 画面はこれを受けて**いま表示しているシート**を `grid_open_sheet` で開き直す（適用のあとの
   * 表示は古い。`src-tauri/src/commands/grid.rs` の `with_displayed` の doc）。
   */
  readonly onApplied: () => void;
}

/**
 * 実物の保持（アプリ全体で 1 つ）。**`src/ipc/client.ts` の入口へ委譲する実装である。**
 *
 * module 定数である（差し替える理由が無く、実行が画面の差し替えを越えて続くため、面ごとに
 * 作り直してはならない。[`./store`] の module doc）。検査は
 * [`createMacroSurfaceStore`] に偽の境界を渡して自分専用の保持を作る。
 */
export const MACRO_SURFACE_STORE: MacroSurfaceStore = createMacroSurfaceStore(createMacroClient());

/**
 * 面のパネル。**グリッド画面が載せる実体である。**
 *
 * 状態は `useSyncExternalStore` で読む（保持は module にあり、実行が画面の差し替えを越えて
 * 続くためである。[`./store`] の module doc）。3 つの効果はマウントに 1 回ずつである。
 */
export function MacroPanel({ binding }: { readonly binding: MacroSurfaceBinding }): ReactElement {
  const { store, onApplied } = binding;
  const state = useSyncExternalStore(store.subscribe, store.getState, store.getState);
  useEffect(() => { store.dispatch({ type: "refresh" }); }, [store]);
  useEffect(() => installMacroListRefresh(store), [store]);
  useEffect(() => store.subscribeApplied(onApplied), [store, onApplied]);
  return <MacroPanelView state={state} dispatch={store.dispatch} />;
}

export type MacroPanelEvent =
  | { readonly type: "refresh" }
  | { readonly type: "choose"; readonly name: string }
  | { readonly type: "cancel-choice" }
  | { readonly type: "run" }
  | { readonly type: "dismiss-result" };

/** 見た目へ渡すもの。イベントは機能の仲介役へ 1 つの口で送る。 */
export interface MacroPanelViewProps {
  readonly state: MacroSurfaceState;
  readonly dispatch: (event: MacroPanelEvent) => void;
}

export function MacroPanelView({ state, dispatch }: MacroPanelViewProps): ReactElement {
  const chosen = chosenSummary(state);
  return (
    <section
      data-testid="jxcel-macro-panel"
      data-macro-list={state.list.status}
      {...{ [RUNNING_ATTR]: state.running === null ? "false" : "true" }}
      aria-label="マクロ"
      style={PANEL_STYLE}
    >
      <h2 style={HEADING_STYLE}>マクロ</h2>
      {state.running === null ? null : (
        <p data-testid="jxcel-macro-running" role="status" style={MESSAGE_STYLE}>
          {`実行中: ${state.running.name}`}
        </p>
      )}
      {state.result === null ? null : (
        <MacroResultView run={state.result} dispatch={dispatch} />
      )}
      {chosen === null ? null : (
        <div data-testid="jxcel-macro-capabilities" data-macro-name={chosen.name} style={BLOCK_STYLE}>
          <h3 style={SUBHEADING_STYLE}>実行する前に、宣言している能力を確認してください</h3>
          <p style={MESSAGE_STYLE}>{`マクロ「${chosen.name}」（${describeKind(chosen.kind)}）`}</p>
          {chosen.capabilities.length === 0 ? (
            <p data-testid="jxcel-macro-no-capability" style={MESSAGE_STYLE}>
              宣言している能力はありません（ファイルとネットワークに触れません）。
            </p>
          ) : (
            <ul data-testid="jxcel-macro-capability-list" style={LIST_STYLE}>
              {chosen.capabilities.map((capability) => (
                <li key={capability} data-testid="jxcel-macro-capability" data-macro-capability={capability} style={ITEM_STYLE}>
                  {capability}
                </li>
              ))}
            </ul>
          )}
          {canPresentRun(state) ? (
            <button type="button" data-testid="jxcel-macro-run" onClick={() => dispatch({ type: "run" })} style={BUTTON_STYLE}>
              実行する
            </button>
          ) : (
            <p data-testid="jxcel-macro-run-blocked" style={MESSAGE_STYLE}>
              実行中です。終わってから実行できます。
            </p>
          )}
          <button type="button" data-testid="jxcel-macro-cancel" onClick={() => dispatch({ type: "cancel-choice" })} style={BUTTON_STYLE}>
            取り消す
          </button>
        </div>
      )}
      <MacroListView state={state} dispatch={dispatch} />
    </section>
  );
}

/**
 * 一覧の区画（要件 1.3、1.4、2.1、2.7）。
 *
 * `picking` のときだけ「選ぶ」の操作が出る（**実行の入口はメニューの 1 項目**であり、面は
 * その要求を受けて選択を提示する）。
 */
function MacroListView({ state, dispatch }: { readonly state: MacroSurfaceState; readonly dispatch: (event: MacroPanelEvent) => void }): ReactElement {
  const list = state.list;
  switch (list.status) {
    case "loading":
      return (
        <p data-testid="jxcel-macro-list-loading" style={MESSAGE_STYLE}>
          マクロの一覧を読み込んでいます
        </p>
      );
    case "failed":
      return (
        <div data-testid="jxcel-macro-list-failure" style={BLOCK_STYLE}>
          <p style={MESSAGE_STYLE}>{`マクロの一覧を読み込めませんでした: ${list.message}`}</p>
          <button
            type="button"
            data-testid="jxcel-macro-list-retry"
            onClick={() => dispatch({ type: "refresh" })}
            style={BUTTON_STYLE}
          >
            再試行
          </button>
        </div>
      );
    case "ready": {
      if (list.macros.length === 0) {
        // **導線を出さない**（要件 2.7）。メニューの項目は無効化されない（述語を持たない。
        // `src-tauri/src/commands/macro.rs` の `install`）ので、要求が来てもここで何も
        // 提示しないことが 2.7 の満たし方である。
        return (
          <p data-testid="jxcel-macro-empty" style={MESSAGE_STYLE}>
            この文書にはマクロがありません
          </p>
        );
      }
      return (
        <>
          {state.picking && runnableMacros(list.macros).length === 0 ? (
            // 解釈できない 1 件しか無い場合である。**一覧は理由つきで残る**が、実行の導線は出さない。
            <p data-testid="jxcel-macro-none-runnable" role="status" style={MESSAGE_STYLE}>
              実行できるマクロがありません（一覧の理由を参照してください）
            </p>
          ) : null}
          <ol data-testid="jxcel-macro-list" style={LIST_STYLE}>
            {list.macros.map((macro) => (
              <li
                key={macro.name}
                data-testid="jxcel-macro-row"
                data-macro-name={macro.name}
                data-macro-kind={macro.kind}
                data-macro-runnable={macro.failure === null ? "true" : "false"}
                data-macro-capabilities={macro.capabilities.join(" ")}
                style={ITEM_STYLE}
              >
                <span data-testid="jxcel-macro-row-name">{macro.name}</span>
                <span data-testid="jxcel-macro-row-kind">{describeKind(macro.kind)}</span>
                <span data-testid="jxcel-macro-row-capabilities" style={MESSAGE_STYLE}>
                  {macro.capabilities.length === 0
                    ? "能力の宣言なし"
                    : `能力: ${macro.capabilities.join(", ")}`}
                </span>
                {macro.failure === null ? null : (
                  // **解釈できなかった 1 件も一覧に残る**（要件 1.4）。層・理由・フレームを出す。
                  <div
                    data-testid="jxcel-macro-row-uninterpretable"
                    data-macro-failure-layer={macro.failure.kind.kind}
                    style={BLOCK_STYLE}
                  >
                    <MacroFailure failure={failurePresentation(macro.failure)} />
                  </div>
                )}
                {state.picking && macro.failure === null ? (
                  <button
                    type="button"
                    data-testid="jxcel-macro-choose"
                    data-macro-name={macro.name}
                    onClick={() => dispatch({ type: "choose", name: macro.name })}
                    style={BUTTON_STYLE}
                  >
                    選ぶ
                  </button>
                ) : null}
              </li>
            ))}
          </ol>
        </>
      );
    }
    default:
      return assertNever(list, "一覧の状態の分岐が網羅されていない");
  }
}

/**
 * 実行の結果の区画（要件 2.3、2.4、2.5、6.1、6.2）。**4 つの値がそれぞれ固有の形で出る。**
 *
 * 打ち切りは失敗と**別の値**であり（生成物の `MacroRunOutcome` の doc）、提示も別である —
 * 失敗は「どこで失敗したか」を、打ち切りは「どちらの上限に当たったか」を言う。
 */
function MacroResultView({ run, dispatch }: { readonly run: SettledMacroRun; readonly dispatch: (event: MacroPanelEvent) => void }): ReactElement {
  const dismiss = (
    <button
      type="button"
      data-testid="jxcel-macro-result-dismiss"
      onClick={() => dispatch({ type: "dismiss-result" })}
      style={BUTTON_STYLE}
    >
      閉じる
    </button>
  );
  const result: MacroResultPresentation = run.result;
  switch (result.kind) {
    case "ran":
      return (
        <div
          data-testid="jxcel-macro-result-ran"
          data-macro-changed={result.changed ? "true" : "false"}
          data-macro-elapsed-ms={result.elapsedMs}
          style={BLOCK_STYLE}
        >
          <h3 style={SUBHEADING_STYLE}>{`実行が終わりました: ${run.name}`}</h3>
          {/* **戻り値は境界が組んだ文字列そのものである**（`MacroRunOutcome` の doc）。 */}
          <p data-testid="jxcel-macro-result-value" style={MESSAGE_STYLE}>
            {`戻り値: ${result.value}`}
          </p>
          <p
            data-testid="jxcel-macro-changes"
            data-macro-changes-total={changeTotal(result.changes)}
            style={MESSAGE_STYLE}
          >
            {describeChanges(result.changes)}
          </p>
          <p data-testid="jxcel-macro-elapsed" style={MESSAGE_STYLE}>
            {`所要 ${String(result.elapsedMs)} ms`}
          </p>
          {result.output.length === 0 ? null : (
            <ol data-testid="jxcel-macro-output" style={LIST_STYLE}>
              {result.output.map((line, index) => (
                <li
                  // 出力は**同じ本文の行が並びうる**ので、位置を鍵にする（順序が意味を持つ）。
                  key={`${String(index)}:${line.level}`}
                  data-testid="jxcel-macro-output-line"
                  data-macro-output-level={line.level}
                  style={ITEM_STYLE}
                >
                  {describeOutputLine(line)}
                </li>
              ))}
            </ol>
          )}
          {dismiss}
        </div>
      );
    case "failed":
      return (
        <div data-testid="jxcel-macro-result-failed" style={BLOCK_STYLE}>
          <h3 style={SUBHEADING_STYLE}>{`実行が失敗しました: ${run.name}`}</h3>
          <MacroFailure failure={result.failure} />
          {dismiss}
        </div>
      );
    case "aborted":
      return (
        <div
          data-testid="jxcel-macro-result-aborted"
          data-macro-abort-limit={result.limit}
          data-macro-elapsed-ms={result.elapsedMs}
          style={BLOCK_STYLE}
        >
          <h3 style={SUBHEADING_STYLE}>
            {`実行を打ち切りました: ${run.name}（${describeAbortLimit(result.limit)}）`}
          </h3>
          <p data-testid="jxcel-macro-abort-limit" style={MESSAGE_STYLE}>
            {`打ち切りの種類: ${describeAbortLimit(result.limit)}`}
          </p>
          <p data-testid="jxcel-macro-elapsed" style={MESSAGE_STYLE}>
            {`打ち切りまでの所要 ${String(result.elapsedMs)} ms`}
          </p>
          <MacroFailure failure={result.failure} />
          {dismiss}
        </div>
      );
    case "rejected":
      return (
        <div data-testid="jxcel-macro-result-rejected" style={BLOCK_STYLE}>
          <h3 style={SUBHEADING_STYLE}>{`実行できませんでした: ${run.name}`}</h3>
          <p data-testid="jxcel-macro-rejected-reason" style={MESSAGE_STYLE}>
            {result.message}
          </p>
          {dismiss}
        </div>
      );
    default:
      return assertNever(result, "実行の結果の分岐が網羅されていない");
  }
}
/** 失敗の提示（要件 2.4、9.1、9.2、9.3）。**失敗と打ち切りと、解釈できなかった理由が使う。** */
function MacroFailure({ failure }: { readonly failure: MacroFailurePresentation }): ReactElement {
  const layer = failure.layer;
  return (
    <>
      <p
        data-testid="jxcel-macro-failure-layer"
        data-macro-failure-layer={layer.kind}
        {...(layer.kind === "host_rejected" ? { "data-macro-failure-api": layer.api } : {})}
        style={MESSAGE_STYLE}
      >
        {describeFailureLayer(layer)}
      </p>
      <p data-testid="jxcel-macro-failure-reason" style={MESSAGE_STYLE}>
        {failure.reason}
      </p>
      <MacroFrames frames={failure.frames} />
    </>
  );
}

/**
 * 呼び出しの並び（要件 9.3）。**内側から外側へ**並ぶ（境界の並びをそのまま出す）。
 *
 * 空でありうる（フレームを持たない失敗。例: ソースの解釈の失敗）ので、そのときは何も出さない
 * — 空の枠を出さない。
 */
function MacroFrames({
  frames,
}: {
  readonly frames: MacroFailurePresentation["frames"];
}): ReactElement | null {
  if (frames.length === 0) {
    return null;
  }
  return (
    <ol data-testid="jxcel-macro-frames" style={LIST_STYLE}>
      {frames.map((frame, index) => (
        <li
          // 同じ位置が 2 度現れうる（相互再帰）ので、位置を鍵にする（並びが意味を持つ）。
          key={`${String(index)}:${frame.macro_name}`}
          data-testid="jxcel-macro-frame"
          data-macro-frame-macro={frame.macro_name}
          data-macro-frame-function={frame.function}
          data-macro-frame-line={frame.line}
          data-macro-frame-column={frame.column}
          style={ITEM_STYLE}
        >
          {describeFrame(frame)}
        </li>
      ))}
    </ol>
  );
}

// ---------------------------------------------------------------------------
// 見た目（**値は `GridScreen.tsx` の同じ名の定数に合わせてある**）
// ---------------------------------------------------------------------------

/** パネル全体。**表の上に載るバーである**（表を押し出さないよう、伸びるのは一覧の中だけ）。 */
const PANEL_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.5rem",
  border: `1px solid var(${APPEARANCE_VARS.controlBorder})`,
  borderRadius: "0.25rem",
  padding: "0.5rem 0.75rem",
  color: `var(${APPEARANCE_VARS.screenText})`,
} as const;

/** パネルの見出し（表の状態の見出しと同じ大きさである）。 */
const HEADING_STYLE = { margin: 0, fontSize: "1.125rem" } as const;

/** 区画の中の見出し（「実行する前に…」「実行が終わりました…」）。 */
const SUBHEADING_STYLE = { margin: 0, fontSize: "0.9375rem" } as const;

/** 説明・理由・告知の文字（補助的な文字色）。 */
const MESSAGE_STYLE = { margin: 0, color: `var(${APPEARANCE_VARS.screenMuted})` } as const;

/** 縦に積む区画（能力の提示・結果・解釈できなかった理由）。 */
const BLOCK_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  alignItems: "flex-start",
} as const;

/**
 * 一覧（マクロの一覧・能力の並び・フレーム・出力）。
 *
 * **高さを区切って中でスクロールさせる** — 一覧が長くても表を押し出さない（要件 2.2 の
 * 「表の表示を止めない」は、面が場所を奪わないことでもある）。
 */
const LIST_STYLE = {
  display: "flex",
  flexDirection: "column",
  gap: "0.25rem",
  margin: 0,
  padding: 0,
  listStyle: "none",
  maxHeight: "9rem",
  overflowY: "auto",
  width: "100%",
  boxSizing: "border-box",
} as const;

/** 一覧の 1 件。 */
const ITEM_STYLE = {
  display: "flex",
  flexWrap: "wrap",
  gap: "0.5rem",
  alignItems: "baseline",
} as const;

/** 操作（選ぶ・実行する・取り消す・閉じる・再試行）。 */
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
