/**
 * 初回描画のハートビート — 描画フレームの中から到達を通知する。
 *
 * 所有: 初回描画の監視の発信側（design.md「Components and Interfaces → Adapter Layer」の
 * `RenderWatchdog`。監視側は受け取るだけであり、発信はフロントエンドの責任である）。
 * 要件: 10.1、10.2。
 *
 * # なぜ「描画フレームの中から」なのか
 *
 * **描画失敗を検出する仕組みは基盤側に存在しない**（research.md 決定 7。空白の画面と資産の
 * 読み込み失敗は外からは区別できない）。したがって描画が成立したことの唯一の証拠は、
 * **実際にフレームが描かれた後に、その文脈から通知が届くこと**である。
 *
 * `requestAnimationFrame` を 2 回重ねるのはこのためである:
 *
 * - 1 回目のコールバックは「このフレームをこれから描く」直前に走る（まだ描画の証拠にならない）。
 * - 2 回目のコールバックは、**1 回目のフレームが描かれた後**に走る。ここから送れば、
 *   「少なくとも 1 フレームが成立した」ことの証拠になる。
 *
 * マウント直後（`src/main.tsx`）に 1 回だけ仕掛ける。**ウィンドウごとに別の JS 実行文脈なので、
 * それぞれの文脈が自分の通知を送る**（送信先のウィンドウは Rust 側が注入された引数から取る。
 * ここからウィンドウを申告しない。要件 4.6）。
 *
 * # ラスタライザの申告（research.md 決定 7）
 *
 * `WEBGL_debug_renderer_info` の `UNMASKED_RENDERER_WEBGL` を読んで送る。Rust 側はこれが
 * 既知のソフトウェア実装に一致するかで `Painted` と `SoftwareRaster` を分ける。**取得できない
 * 環境（拡張が無効・WebGL 文脈を作れない）では `null` を送る** — 描画フレームから通知が届いた
 * こと自体が描画成立の証拠であり、ラスタライザは補助的な情報である。
 *
 * 読むためだけに作った WebGL 文脈は、読み終えたら `WEBGL_lose_context` で手放す（使わない
 * 文脈を保持すると、GPU 資源とコンテキスト数の上限を無駄に消費する）。
 *
 * # 失敗しても何も壊さない
 *
 * 通知が届かなければ監視側は期限超過として不成立を記録し、利用者に提示する（要件 10.2）。
 * したがってこの送信は**例外を外へ出さない**。`invoke` の拒否も、封筒の失敗腕も、
 * コンソールへの 1 行に留める。**アプリの起動や描画を妨げない。**
 *
 * # 9.7 との分担（**送信側を 2 つ作らない**）
 *
 * この送信側（描画フレームの中から到達を通知する機構）は 8.2 が実装した。一方、tasks.md 9.7
 * の文言は「描画フレームの中から到達の通知を送る発信側をここで持つ」として発信側の責務を
 * 9.7 に置いており、**文言上は重なっている**（design.md の `RenderWatchdog` の行は
 * 「フロントエンドが描画フレーム内から通知する」として、この機構自体を監視と一体の責務と
 * している）。重なりは次のように解いた:
 *
 * - **送信側（このモジュールと `src/main.tsx` の `installRenderHeartbeat()`）は 8.2 が
 *   持つ。** 監視の期限（3 秒）はウィンドウの生成から測るため、送信側が無ければ 8.2 の
 *   受け入れ基準（通知を呼べば成立、呼ばなければ不成立）そのものが成立しない。
 * - **9.7 はこの送信側を再利用すること。** 9.7 が持つのは 3 OS 描画確認用の最小画面
 *   （表形式と文字編集の 2 種類）であり、このモジュールは画面を持たない。**9.7 が 2 つ目の
 *   送信側を足してはならない** — 同じウィンドウについて両方が通知すると、先に届いた方だけが
 *   判定を確定し（2 番目は `AlreadyDecided` で無視される）、どちらが先かは環境依存になる。
 *   画面を足しても、この送信はマウント時に 1 回だけ仕掛かるのでそのまま働く。
 */

import type { RenderHeartbeatResponse } from "../ipc/bindings";
import { invokeCommand, type CommandName } from "../ipc/client";

/**
 * 初回描画の通知コマンドの名前。
 *
 * **文字列を直接 `invoke` へ渡さない。** 型注釈（[`CommandName`]）は生成物
 * `src/ipc/bindings.ts` の `COMMAND_NAMES` から導かれた合併型であるため、
 * `crates/app-shell/src/ipc/command_names.rs` からこの名前が消えると**この行で型検査が落ちる**
 * （tasks.md 2.2 / 2.4 の拡張規則。手書きの名前を許さない）。
 */
const RENDER_HEARTBEAT_COMMAND: CommandName = "render_heartbeat";

/**
 * 通知を 1 回送る。**マウントの直後に 1 回だけ呼ぶ。**
 *
 * 通知は 2 回目の `requestAnimationFrame` の中から行う（冒頭を参照）。`requestAnimationFrame`
 * が無い環境では何もしない（配信先中立の画面でも壊れないようにする）。
 *
 * **例外を外へ出さない。** 封筒の失敗腕（監視していないウィンドウ・経路の失敗）と `invoke` の
 * 拒否のどちらも、コンソールへの 1 行に留める。監視側は通知が届かなければ期限超過として
 * 不成立を記録するので、ここで起動を止める必要はない（むしろ止めてはならない）。
 */
export function installRenderHeartbeat(): void {
  if (typeof requestAnimationFrame !== "function") {
    return;
  }
  requestAnimationFrame(() => {
    requestAnimationFrame(() => {
      void (async () => {
        try {
          // **引数の鍵は Rust 側の仮引数名である**（Tauri は仮引数名で引数を対応付ける。
          // コマンドの署名は `render_heartbeat(watch, window, request: RenderHeartbeatRequest)`
          // なので、鍵は `request` になる）。生バイト経路（`bulk_echo`）だけが例外であり、
          // こちらは封筒を返す通常の経路である。
          const result = await invokeCommand<RenderHeartbeatResponse>(
            RENDER_HEARTBEAT_COMMAND,
            { request: { renderer: detectRenderer() } },
          );
          if (result.status === "error") {
            console.warn(
              `初回描画の通知を受け取ってもらえなかった: ${result.error.kind}`,
            );
          }
        } catch (error: unknown) {
          console.warn("初回描画の通知を送れなかった", error);
        }
      })();
    });
  });
}

/**
 * ラスタライザの文字列を読む（research.md 決定 7）。取得できなければ `null`。
 *
 * **失敗しない。** WebGL 文脈を作れない環境・拡張が無効な環境・`getParameter` が文字列以外を
 * 返す環境のいずれでも `null` を返し、呼び出し側はそのまま通知する。
 */
export function detectRenderer(): string | null {
  let gl: WebGLRenderingContext | null = null;
  try {
    const canvas = document.createElement("canvas");
    gl = canvas.getContext("webgl");
    if (gl === null) {
      return null;
    }
    const extension = gl.getExtension("WEBGL_debug_renderer_info");
    if (extension === null) {
      return null;
    }
    const value: unknown = gl.getParameter(extension.UNMASKED_RENDERER_WEBGL);
    return typeof value === "string" && value.length > 0 ? value : null;
  } catch {
    return null;
  } finally {
    // 読むためだけに作った文脈は手放す（使わない文脈を保持しない）。手放せなくても
    // 起動を妨げない。
    try {
      gl?.getExtension("WEBGL_lose_context")?.loseContext();
    } catch {
      // 手放せないだけであり、通知には影響しない。
    }
  }
}
