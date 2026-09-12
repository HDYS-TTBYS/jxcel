/**
 * フロントエンドのエントリポイント。React のルートを単一ページとして組み立てる。
 *
 * 所有: アプリ起動の結線（design.md「File Structure Plan」の `src/main.tsx`）。
 * 要件: 1.1（3 OS それぞれで起動できる配布物）, 1.2（追加のインストール無しで起動）。
 *
 * サーバ側描画（SSR）は行わない。Tauri は SSR を支援せず、SPA のみである
 * （design.md「Technology Stack」）。画面の中身は `shell/Layout` が持ち、本ファイルは
 * マウントと起動時の前提確認だけを行う。
 *
 * 本ファイルはタスク 1.4 が実体を置いた。初期画面の領域定義はタスク 9.1 が
 * `shell/` 配下を育てて拡張した。外観の解決（タスク 9.2）は `shell/theme` が持つ。
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { installCloseVeto } from "./shell/closeVeto";
import { Layout } from "./shell/Layout";
import { installRenderHeartbeat } from "./shell/renderHeartbeat";
import { bootstrapAppearance } from "./shell/theme";

// `src/index.html` のマウント先。欠けたまま起動すると無内容のウィンドウが残るため、
// 黙って握り潰さずに失敗させる（要件 10.2 の趣旨に沿う）。
const container = document.getElementById("root");
if (container === null) {
  throw new Error("初期画面のマウント先 #root が src/index.html に存在しません");
}

// 終了拒否の購読（要件 2.6、タスク 7.6）。**この購読が存在すること自体が、基盤に
// ウィンドウの終了を拒否させる**（Tauri は `tauri://close-requested` の JS リスナの登録を
// 検出して自動的に `prevent_close()` する）。拒否された後は、この経路がドキュメント所有者へ
// 可否を問い合わせ、許可されたときだけ `destroy()` で閉じる。
// `installCloseVeto` は例外を投げない（登録に失敗した場合はウィンドウは通常どおり閉じる）。
void installCloseVeto();

// 初回描画のハートビートの送信側（要件 10.1、10.2。タスク 8.2）は、**起動時の await を
// すべて終えてから、マウントの直前に 1 回だけ仕掛ける**（下の `mountShell` の最後）。待つ前に
// 仕掛けると、保存された外観を読んでいる間の空白のフレームで「描画が成立した」と報告して
// しまう。通知は描画フレームの中から送られ、届かなければ監視側が期限超過として不成立を
// 記録して利用者に提示する（`src/shell/renderHeartbeat.ts` のモジュール doc）。

/**
 * 外観を解決してから React をマウントする（要件 9.3、9.4。タスク 9.2）。
 *
 * **シェルの最初の描画を解決済みの外観で行うため、マウントを待たせる。** マウントが先だと、
 * 保存された明示選択が届くまでの間だけ既定（OS 追随）の外観で描かれ、その後に切り替わる。
 * `bootstrapAppearance` は自分で期限を切って必ず戻り、例外も外へ出さないので、**マウントが
 * 行われない経路は無い**（`shell/theme` のモジュール doc を参照）。念のため外側でも受け止め、
 * どのみち描画へ進む。
 */
async function mountShell(root: HTMLElement): Promise<void> {
  try {
    await bootstrapAppearance();
  } catch (error: unknown) {
    console.error("外観を初期化できなかった。OS に追随する外観で起動する", error);
  }

  // 検証専用: 大きなペイロードの一括転送の駆動側（要件 4.5。タスク 10.8）。**既定のビルド
  // （配布物）にはこのモジュールが 1 バイトも入らず、動的 import の塊も生成されない** —
  // `__JXCEL_VERIFICATION__` は Vite の `define` が埋め込むビルド時定数であり、既定のビルド
  // では `false` なので、この分岐ごと定数畳み込みで消える（`vite.config.ts`。
  // `scripts/check-shipping-bundle.sh` が配布物の `dist/` を機械検査する）。
  // 検証用の形（`JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features
  // verification-triggers`）では、初期化スクリプトが載せたグローバルがあるときだけ働く。
  // 転送は初回描画の後に回るので、8.2 のハートビートと起動予算（10.3）は変わらない。
  //
  // **この 1 箇所だけ意図的に動的 import を使う**（静的 import では要件を満たせない）。
  // 静的 import は到達不能な分岐の中にあってもモジュールグラフへ引き込まれ、依存先
  // （`@tauri-apps/api/event` など）が副作用を持つと Rollup が木を落とせない。配布物から
  // 確実に落ちるのは、**到達不能な動的 import の塊**（Rollup が生成自体を取りやめる）だけ
  // である。
  if (__JXCEL_VERIFICATION__) {
    const { installVerificationBulkTransfer } = await import(
      "./shell/verificationBulk"
    );
    installVerificationBulkTransfer();
  }

  // **マウントの直前に仕掛ける。** 通知は入れ子の `requestAnimationFrame`（2 フレーム）から
  // 出るので、間に await を挟むと「シェルがまだマウントされていないフレーム」から通知が出る。
  // 実測（2026-09-12、macOS のランナー）: 上の動的 import を挟んだ位置で仕掛けると、2 フレーム
  // が import の完了より先に来て、領域が無い状態の通知（記録は `画面=(報告なし)`）が確定し、
  // 10.4 の描画確認が落ちた（配布物＝動的 import が無い形では同じ実行が通っていた）。
  // **この行より上に await を足してはならない。**
  //
  // 通知の送信側は 1 つだけである（9.7 はこれを再利用し、2 つ目を足さない）。
  installRenderHeartbeat();

  createRoot(root).render(
    <StrictMode>
      <Layout />
    </StrictMode>,
  );
}

void mountShell(container);
