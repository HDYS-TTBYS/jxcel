/**
 * 画面単位のエラー隔離 — 個別機能の画面が描画中に失敗してもアプリ全体を停止させない境界。
 *
 * 所有: `ShellLayout` のエラー隔離部分（design.md「Components and Interfaces → Frontend Layer」）。
 * 要件: 9.5（個別機能の画面の描画中にエラーが発生したとき、アプリケーション全体を停止させず、
 * 該当する領域にエラーを提示すること）。
 *
 * # 配置（**この境界が包むのは画面 1 式だけである**）
 *
 * `Layout.tsx` の `ShellRegion` が描く `<Screen … />` の 1 式を包む。**シェルのクローム
 * （ヘッダ帯・外観の操作）も、領域の要素そのものも包まない。** 包む範囲を広げると、画面の
 * 失敗がシェル全体の停止（空白のウィンドウ）に化け、要件 9.5 の「アプリケーション全体を
 * 停止させない」が成立しない。境界は表示中の画面の識別子（`screenId`）を鍵として与えられ、
 * **画面が変わると React が境界を作り直す**（`ShellRegion` の `key`。下の「回復」を参照）。
 *
 * # 捕まえるもの・捕まえないもの（**React のエラー境界の仕様**）
 *
 * 捕まえる — 画面のサブツリーで起きた、
 *
 * - 描画（render）中の例外、
 * - コンストラクタ・`componentDidMount` / `componentDidUpdate` / `componentWillUnmount`
 *   などのライフサイクル中の例外、
 * - 副作用（`useEffect` / `useLayoutEffect`）の実行中の例外（コミット中に投げられ、最寄りの
 *   エラー境界へ届く）。
 *
 * 捕まえない（**React のエラー境界では捕まえられない**）:
 *
 * - イベントハンドラ（`onClick` など）の中で投げられた例外、
 * - 非同期処理（`setTimeout` / Promise / IPC の応答コールバック）の中で投げられた例外、
 * - この境界自身と、その上位（`ShellRegion` / `Layout`）で投げられた例外。
 *
 * 要件 9.5 の対象は「画面の**描画中**のエラー」であるため、上の捕まえる範囲で足りる。
 * イベントハンドラや非同期の失敗は**どの画面に帰属するかを機械的に決められない**（例外の
 * スタックに表示中の画面の情報が残らない）ので、ここで領域へ押し込むと誤った画面にエラーを
 * 出す。各画面が自分の失敗として扱う（`./router` は未知の遷移を例外で拒否するが、それを
 * 呼ぶのは画面のイベントハンドラであり、画面自身が受け止める）。
 *
 * # 回復（**アプリを恒久的に汚染しない**）
 *
 * - **別の画面へ遷移すると境界ごと作り直される。** `ShellRegion` は境界へ `key={screen.id}`
 *   を与えるため、`navigate` で画面が変わると古い境界は破棄され、遷移先の新しい境界が
 *   作られる。したがって失敗した画面から離れればエラー提示は消え、遷移先は通常どおり描画される。
 * - **「再試行」は同じ画面をもう一度描く。** 境界の状態を戻すだけであり、画面の識別子は
 *   変わらないので遷移は起きない。
 * - **同じ画面が再び失敗した場合**も、例外は捕まって同じ提示に戻るだけである（アプリは
 *   停止しない）。再試行のたびに描画をやり直すので、一時的な失敗はその場で解消しうる。
 *   一度失敗した画面へ後で戻ってきた場合も、そのとき新しく作られた境界がもう一度描画を
 *   試みる（失敗を記憶して握り潰すことはしない）。
 *
 * # 診断情報へ送らない理由
 *
 * このエラーを診断の記録へ送る経路は**設けない**。
 *
 * - フロントエンドから記録へ書き込むコマンドは存在せず、`crates/app-shell/src/ipc/command_names.rs`
 *   の集合は閉じている（tasks.md 2.2）。追加すれば Rust 側のハンドラ登録・権限記述・生成物に
 *   触れることになり、本タスクの境界（`src/**` のみ）を越える。
 * - 5.2 は診断の記録先として `Webview` ターゲットを意図的に有効にしていない。したがって
 *   基盤の記録機構へ流す経路も無い。
 *
 * 代わりに保証するのは「**利用者に見える形で領域に提示され、シェルと他の画面は操作可能な
 * まま**」であること。生の例外は開発者向けに `console.error` へ残す（下の `componentDidCatch`）。
 *
 * # 利用者に見せるもの（**生の例外を見せない**）
 *
 * 提示に出すのは固定の見出しと、失敗した画面の**利用者向けの題名**（レジストリの `title`）
 * だけである。例外メッセージ・スタックトレース・コンポーネントスタック・ソースのパスは
 * **出さない**（内部の構造やパスを画面へ漏らさないため）。機械可読な目印は
 * `[data-testid="jxcel-screen-error"]` と、同じ要素の `data-screen-error="<画面の識別子>"`
 * である。
 *
 * # 本ファイルはタスク 9.3 が実装した
 *
 * 1.4 はコメント（適用先の記述）だけを置いた。9.1 は領域の中の `<Screen … />` 1 式を包む、
 * という契約を記録した。9.3 がその契約どおりに実体を置いた。
 */
import {
  Component,
  type CSSProperties,
  type ErrorInfo,
  type ReactElement,
  type ReactNode,
} from "react";

import type { ScreenDefinition, ScreenProps } from "./router";
import { APPEARANCE_VARS } from "./theme";

/**
 * エラー提示の目印。**外部の検査はこの `data-testid` と、同じ要素の `data-screen-error`
 * 属性（失敗した画面の識別子）を読めば足りる。**
 */
export const SCREEN_ERROR_TESTID = "jxcel-screen-error";

/** 同じ画面をもう一度描く操作の目印。 */
export const SCREEN_ERROR_RETRY_TESTID = "jxcel-screen-error-retry";

/**
 * エラー時に他の画面へ移る操作の目印の接頭辞。実際の値は `<接頭辞>-<画面の識別子>` である。
 */
export const SCREEN_ERROR_GO_PREFIX = "jxcel-screen-error-go";

/** 境界が受け取るもの。 */
export interface ScreenBoundaryProps {
  /** 包んでいる画面の識別子。提示の目印（`data-screen-error`）と記録に使う。 */
  readonly screenId: string;
  /** 包んでいる画面の利用者向けの題名。提示に出す唯一の可変な文言である。 */
  readonly title: string;
  /** エラー時に提示する他の画面（自分自身を除く）。空なら遷移の操作を出さない。 */
  readonly alternatives: readonly ScreenDefinition[];
  /** 遷移の唯一の入口（`ScreenProps.navigate` と同じもの）。 */
  readonly navigate: ScreenProps["navigate"];
  /** 画面の実体（`ShellRegion` の中の `<Screen … />` 1 式）。 */
  readonly children: ReactNode;
}

/** 境界の状態。**失敗したかどうかだけ**を持つ。 */
interface ScreenBoundaryState {
  readonly failed: boolean;
}

/** 提示の見出し。**例外の中身を混ぜない**（モジュール doc「利用者に見せるもの」）。 */
const ERROR_HEADING = "この画面を表示できませんでした";

/**
 * 提示の面の見た目。**シェルと同じ配色（`./theme` のカスタムプロパティ）だけを参照する**ので、
 * 明色・暗色のどちらの外観でも、画面と同じ 1 組の配色に従う（画面の契約 4）。
 */
const panelStyle: CSSProperties = {
  maxWidth: "32rem",
  padding: "1.5rem 2rem",
  borderRadius: "0.5rem",
  // 上端だけアクセント色（両外観で同じ）を残し、シェルの面と区別できるようにする。
  borderTop: `4px solid var(${APPEARANCE_VARS.shellChrome})`,
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
  textAlign: "center",
};

/** 提示の中で押せる操作の見た目。**枠線と文字にシェルの配色を使う。** */
const buttonStyle: CSSProperties = {
  font: "inherit",
  fontSize: "0.875rem",
  lineHeight: 1.4,
  padding: "0.35rem 0.9rem",
  borderRadius: "0.25rem",
  cursor: "pointer",
  color: `var(${APPEARANCE_VARS.screenText})`,
  backgroundColor: "transparent",
  border: `1px solid var(${APPEARANCE_VARS.screenMuted})`,
};

/**
 * 領域の中に出すエラー提示。**境界自身が描く**（領域の外へ出さない。モジュール doc「配置」）。
 *
 * 押せる操作は 2 種類ある:
 *
 * - 「再試行」— 境界の状態を戻し、同じ画面をもう一度描く。
 * - 「<題名> へ」— 他の画面へ移る（`navigate`。失敗した画面から出る唯一の導線。画面自身は
 *   壊れているので、遷移の操作はシェル側のこの提示が持つ）。
 */
function ScreenErrorPresentation({
  screenId,
  title,
  alternatives,
  navigate,
  onRetry,
}: {
  readonly screenId: string;
  readonly title: string;
  readonly alternatives: readonly ScreenDefinition[];
  readonly navigate: ScreenProps["navigate"];
  readonly onRetry: () => void;
}): ReactElement {
  return (
    <div
      data-testid={SCREEN_ERROR_TESTID}
      data-screen-error={screenId}
      role="alert"
      style={panelStyle}
    >
      <h2 style={{ margin: 0, fontSize: "1.25rem" }}>{ERROR_HEADING}</h2>
      <p
        style={{
          margin: "0.75rem 0 0",
          color: `var(${APPEARANCE_VARS.screenMuted})`,
        }}
      >
        「{title}」の描画中にエラーが発生しました。他の画面とシェルはそのまま操作できます。
      </p>
      <div
        style={{
          display: "flex",
          flexWrap: "wrap",
          gap: "0.5rem",
          justifyContent: "center",
          marginTop: "1.25rem",
        }}
      >
        <button
          type="button"
          data-testid={SCREEN_ERROR_RETRY_TESTID}
          onClick={onRetry}
          style={buttonStyle}
        >
          再試行
        </button>
        {alternatives.map((screen) => (
          <button
            key={screen.id}
            type="button"
            data-testid={`${SCREEN_ERROR_GO_PREFIX}-${screen.id}`}
            onClick={() => {
              navigate(screen.id);
            }}
            style={buttonStyle}
          >
            {screen.title} へ
          </button>
        ))}
      </div>
    </div>
  );
}

/**
 * 画面 1 つを隔離するエラー境界。
 *
 * React のエラー境界はクラスコンポーネントでしか作れない（`componentDidCatch` /
 * `getDerivedStateFromError` に対応するフックは存在しない）。ここでクラスを使うのはそのためで、
 * **状態は「失敗したかどうか」の 1 つだけ**である。
 */
export class ScreenBoundary extends Component<
  ScreenBoundaryProps,
  ScreenBoundaryState
> {
  override state: ScreenBoundaryState = { failed: false };

  /** 描画中の例外を受けて失敗状態へ移る（例外そのものは保持しない）。 */
  static getDerivedStateFromError(): ScreenBoundaryState {
    return { failed: true };
  }

  /**
   * 失敗を開発者向けの記録（コンソール）へ残す。**利用者に見せる文言には混ぜない。**
   *
   * 5.2 は `Webview` ターゲットを有効にしていないため、これは診断の記録ファイルには入らない
   * （モジュール doc「診断情報へ送らない理由」）。
   */
  override componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error(
      `画面 "${this.props.screenId}" の描画中にエラーが発生しました。この画面だけを隔離し、シェルは動作を続けます。`,
      error,
      info.componentStack,
    );
  }

  /** 同じ画面をもう一度描く（境界の状態を戻すだけ。画面の識別子は変わらない）。 */
  private readonly retry = (): void => {
    this.setState({ failed: false });
  };

  override render(): ReactNode {
    if (!this.state.failed) {
      return this.props.children;
    }

    return (
      <ScreenErrorPresentation
        screenId={this.props.screenId}
        title={this.props.title}
        alternatives={this.props.alternatives}
        navigate={this.props.navigate}
        onRetry={this.retry}
      />
    );
  }
}
