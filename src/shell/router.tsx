/**
 * 画面間の遷移を扱う**単一の仕組み**（画面レジストリと現在画面の保持）。
 *
 * 所有: `ShellLayout` の遷移部分（design.md「Components and Interfaces → Frontend Layer」、
 * および「Traceability」の 9.2 の行にある `router`）。
 * 要件: 9.2（画面間の遷移を単一の仕組みで扱い、現在表示されている画面を識別できること）。
 *
 * # 何がここに集約されるか
 *
 * - **どの画面が表示されるかの唯一の決定者**。`useShellRouter` が現在の `ScreenId` を保持し、
 *   遷移は [`navigate`][ShellRouter.navigate] の 1 経路だけで起きる。
 * - **画面の集合（レジストリ）**。[`ScreenDefinition`] の配列と初期画面の識別子。
 *
 * **2 つ目の遷移機構を置いてはならない。** 個別機能の画面が `history` / `location` /
 * ハッシュ / 独自のルーターを持ったら誤りである（要件 9.2）。画面が遷移を要求する唯一の手段は、
 * 受け取った [`ScreenProps.navigate`] を呼ぶことである。画面自身が現在画面を書き換える経路は
 * 存在しない。
 *
 * # 現在画面の識別（外部から観測できる形）
 *
 * 型としての識別子が [`ScreenId`] であり、**表示中の DOM 上では `ShellRegion`
 * （`src/shell/Layout.tsx`）が `data-shell-screen` 属性に現在の識別子を書き出す**。外部の検査は
 * 次の 1 つの式で現在画面を知る（`ShellRegion` の doc に同じ式がある）:
 *
 * ```js
 * document.querySelector('[data-testid="jxcel-shell-region"]').getAttribute("data-shell-screen")
 * ```
 *
 * # 不正なレジストリ・不正な遷移
 *
 * 画面の識別子の重複、空のレジストリ、初期画面の識別子がレジストリに無い場合は、起動時に
 * 判明する**プログラムの誤り**なので、黙って既定へ落とさず例外にする。未知の識別子への遷移も
 * 同様に例外にする（現在画面が識別できなくなる状態を作らないため）。これらの例外は描画中に
 * 起きうるが、画面単位の隔離（要件 9.5、タスク 9.3）はこのファイルではなく `ShellRegion` に
 * 置かれる境界が担う。
 */
import { useCallback, useMemo, useState } from "react";
import type { ComponentType } from "react";

/**
 * 画面の識別子。**現在表示されている画面をコードと外部の両方から識別する**ための型である。
 *
 * 画面の集合はスペックをまたいで増える（3 OS 描画確認用の画面はタスク 9.7、実用画面は下流の
 * スペック）ため、ここで閉じた合併型にはしない。値の妥当性はレジストリ（[`ShellScreenRegistry`]）
 * が実行時に検査する。
 */
export type ScreenId = string;

/**
 * 個別機能の画面が受け取るもの。**画面の契約はこの型である。**
 *
 * 画面はこれ以外の入力を受け取らない（レイアウトも遷移もシェルが持つ。`src/shell/Layout.tsx`
 * の「画面の契約」を参照）。
 */
export interface ScreenProps {
  /** 自分自身の識別子。表示の切り替えや記録に使う（遷移の判断には使わない）。 */
  readonly screenId: ScreenId;
  /**
   * 遷移を要求する**唯一の入口**。画面はこれ以外の方法で画面を切り替えてはならない。
   *
   * 未知の識別子を渡した場合は例外になる（黙って何も起きない状態を作らない）。
   */
  readonly navigate: (destination: ScreenId) => void;
}

/**
 * 画面の定義。**シェルに差し込まれる側が用意する唯一の記述**である。
 *
 * タスク 9.7 は 3 OS 描画確認用の画面を `src/features/smoke/` に置き、その `component` を
 * 持つ定義をレジストリ（`SHELL_SCREEN_REGISTRY`）へ足す。
 */
export interface ScreenDefinition {
  /** 識別子。レジストリ内で一意でなければならない。 */
  readonly id: ScreenId;
  /** シェルのクロームに表示する題名（利用者に見える名前）。 */
  readonly title: string;
  /**
   * 画面の実体。`ScreenProps` だけを受け取る React コンポーネントである。
   *
   * **`ScreenProps` 以外の props を要求してはならない**（シェルが与えられないため）。
   */
  readonly component: ComponentType<ScreenProps>;
}

/**
 * 画面の集合と初期画面。**シェルが画面を差し込むために必要な情報の全体**である。
 */
export interface ShellScreenRegistry {
  /** 起動時に表示する画面の識別子。 */
  readonly initial: ScreenId;
  /** 差し込み可能な画面。空は許さない（無内容のウィンドウを残さないため）。 */
  readonly screens: readonly ScreenDefinition[];
}

/**
 * シェルが画面の差し込みに使う遷移機構の現在の姿。
 */
export interface ShellRouter {
  /** 現在表示されている画面の定義。 */
  readonly current: ScreenDefinition;
  /** 遷移を要求する唯一の入口（[`ScreenProps.navigate`] へそのまま渡される）。 */
  readonly navigate: (destination: ScreenId) => void;
}

/** 例外メッセージに登録済みの識別子を並べる（何が使えるか一目で分かるようにする）。 */
function describeRegisteredIds(index: ReadonlyMap<ScreenId, ScreenDefinition>): string {
  return [...index.keys()].map((id) => `"${id}"`).join(", ");
}

/**
 * レジストリを識別子から定義への写像へ直し、前提を検査する。
 *
 * 純粋な関数として切り出してあるので、画面の集合の妥当性は React を通さずに確かめられる。
 */
function indexScreens(
  registry: ShellScreenRegistry,
): ReadonlyMap<ScreenId, ScreenDefinition> {
  if (registry.screens.length === 0) {
    throw new Error("画面レジストリが空です（差し込む画面が 1 つもありません）");
  }

  const index = new Map<ScreenId, ScreenDefinition>();
  for (const screen of registry.screens) {
    if (index.has(screen.id)) {
      throw new Error(`画面の識別子が重複しています: "${screen.id}"`);
    }
    index.set(screen.id, screen);
  }

  if (!index.has(registry.initial)) {
    throw new Error(
      `初期画面の識別子がレジストリに存在しません: "${registry.initial}"（登録済み: ${describeRegisteredIds(
        index,
      )}）`,
    );
  }

  return index;
}

/**
 * シェルの遷移機構を作る。**画面の差し込みと遷移の全体はこの 1 つのフックが持つ。**
 *
 * 呼ぶのは `src/shell/Layout.tsx` だけであり、画面はこのフックを知らない（受け取るのは
 * [`ScreenProps`] だけである）。
 */
export function useShellRouter(registry: ShellScreenRegistry): ShellRouter {
  // レジストリはモジュール定数であり、識別子から定義への写像はその内容だけで決まる。
  const index = useMemo(() => indexScreens(registry), [registry]);
  const [currentId, setCurrentId] = useState<ScreenId>(registry.initial);

  const navigate = useCallback(
    (destination: ScreenId) => {
      if (!index.has(destination)) {
        throw new Error(
          `未知の画面へ遷移しようとしました: "${destination}"（登録済み: ${describeRegisteredIds(
            index,
          )}）`,
        );
      }
      setCurrentId(destination);
    },
    [index],
  );

  const current = index.get(currentId);
  if (current === undefined) {
    // 上の `navigate` が未知の識別子を拒むため通常は到達しない。到達したなら、初期画面の
    // 検査（`indexScreens`）と保持している状態のどちらかが壊れている。
    throw new Error(`現在の画面がレジストリに存在しません: "${currentId}"`);
  }

  return { current, navigate };
}
