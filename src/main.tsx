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
 * `shell/` 配下を育てて拡張する。
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { installCloseVeto } from "./shell/closeVeto";
import { Layout } from "./shell/Layout";
import { installRenderHeartbeat } from "./shell/renderHeartbeat";

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

// 初回描画のハートビート（要件 10.1、10.2。タスク 8.2）。**マウントの直後に仕掛け、通知は
// 描画フレームの中から送る** — 描画失敗を検出する仕組みは基盤側に無いので、フレームが実際に
// 描かれたことを示せるのはこの経路だけである（`src/shell/renderHeartbeat.ts` のモジュール doc）。
// 送信は例外を外へ出さず、届かなければ監視側が期限超過として不成立を記録して利用者に提示する。
installRenderHeartbeat();

createRoot(container).render(
  <StrictMode>
    <Layout />
  </StrictMode>,
);
