/**
 * 診断の画面 — 記録の保存場所の確認（要件 8.1）、書き出しの要求（要件 8.6）、詳細度の変更
 * （要件 8.7）。タスク 9.5 が 9.1 の領域へ差し込む画面である。
 *
 * 所有: 診断の導線の画面（design.md「Components and Interfaces → Frontend Layer」の画面の契約）。
 * 要件: 8.1, 8.6, 8.7, 4.6。
 *
 * # 契約（`src/shell/Layout.tsx` の「画面の契約」）
 *
 * `ScreenProps` 以外の props を受け取らず、自前のレイアウト・遷移・配色を持たない。配色は
 * `var(--jxcel-…)`（`src/shell/theme.ts` の [`APPEARANCE_VARS`]）を参照するだけなので、
 * 明暗の外観に追随する。**ファイルマネージャやフォルダを開く操作は行わない** — アプリから
 * プロセスを起動しないという要件 4.7 の境界に触れるためである（保存場所は文字列として提示し、
 * 開くかどうかは利用者に委ねる）。
 *
 * # 3 つの導線とコマンド
 *
 * | 区画 | 表示するもの | 使うコマンド（4.5 の申し送り） |
 * |---|---|---|
 * | 保存場所 | 各 OS の規約で解決した記録ディレクトリ | `diagnostics_log_location` |
 * | 書き出し | 書き出したファイルの位置と、記録の有無 | `diagnostics_export` |
 * | 詳細度 | 現在値と、選べる値の全体（`Off` 〜 `Trace`） | `diagnostics_verbosity_get` / `_set` |
 *
 * **コマンド名は生成物（`src/ipc/bindings.ts` の `COMMAND_NAMES`）から導いた型で持つ**ので、
 * 名前が消えればこのファイルの型検査が落ちる（tasks.md 2.2 / 2.4）。イベント名も生成物の定数
 * だけを参照する。
 *
 * # 区画の選択（メニューからの導線）
 *
 * メニューの選択は [`useRequestedSection`] が示す区画として届く。画面はその区画を強調し、
 * 見える位置へ送る（`scrollIntoView`）。**メニューが無い環境（素のブラウザ）でも画面は開ける**
 * ので、各区画の操作は画面の中だけで完結する。
 *
 * # 失敗の見せ方
 *
 * 封筒の失敗（[`IpcClientResult`] の `error` 腕）は `describeIpcError` の 1 行として区画の中に
 * 出す。**例外は外へ出さない** — 描画中に投げると画面単位のエラー隔離（要件 9.5、9.3 の境界）が
 * 発動し、その区画だけでなく画面全体の提示に置き換わってしまう。ここで扱える失敗はここで出す。
 */
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";

import { SETTINGS_CHANGED_EVENT } from "../../ipc/bindings";
import type {
  DiagnosticsExportResponse,
  DiagnosticsLevel,
  DiagnosticsLogLocationResponse,
  DiagnosticsVerbosityResponse,
  SettingsChangedEvent,
} from "../../ipc/bindings";
import {
  describeIpcError,
  invokeCommand,
  type CommandName,
} from "../../ipc/client";
import { APPEARANCE_VARS } from "../../shell/theme";
import { useRequestedSection } from "./requests";

/**
 * 記録の保存場所を提示するコマンドの名前（4.4 が実体を、9.5 が導線を持つ）。
 *
 * **文字列リテラルを `invoke` へ渡さない。** 型注釈（[`CommandName`]）は生成物の
 * `COMMAND_NAMES` から導かれた合併型であるため、`crates/app-shell/src/ipc/command_names.rs`
 * からこの名前が消えるとこの行で型検査が落ちる（tasks.md 2.2）。
 */
const LOG_LOCATION_COMMAND: CommandName = "diagnostics_log_location";

/** 記録を 1 つのファイルへ書き出すコマンドの名前（4.5 / 9.5）。 */
const EXPORT_COMMAND: CommandName = "diagnostics_export";

/** 記録の詳細度を読み取るコマンドの名前（4.5 / 9.5）。 */
const VERBOSITY_GET_COMMAND: CommandName = "diagnostics_verbosity_get";

/** 記録の詳細度を変更するコマンドの名前（4.5 / 9.5）。 */
const VERBOSITY_SET_COMMAND: CommandName = "diagnostics_verbosity_set";

/**
 * 詳細度を保存する設定キーの名前（`crates/app-shell/src/settings/mod.rs` の
 * `SettingsKey::DiagnosticsLevel` の `as_str()` と一致する）。他のウィンドウでの変更を
 * 見分けるために使う。**カタログは閉じており、ここで新しい鍵を作らない。**
 */
export const DIAGNOSTICS_LEVEL_SETTINGS_KEY = "diagnostics.level";

/**
 * 詳細度の表示名。**閉じた列挙を網羅する**（`Record<DiagnosticsLevel, string>` なので、
 * 境界の列挙に値を足すとこの定義がコンパイルエラーになる）。並び順はここに持たず、
 * 応答の `levels`（Rust 側の昇順）に従う。
 */
const LEVEL_LABELS: Record<DiagnosticsLevel, string> = {
  off: "記録しない",
  error: "失敗のみ",
  warn: "警告まで",
  info: "通常（既定）",
  debug: "開発向けの詳細",
  trace: "最も細かい",
};

/** 保存場所の区画の状態。 */
type LocationState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly directory: string }
  | { readonly status: "failed"; readonly message: string };

/** 書き出しの区画の状態。 */
type ExportState =
  | { readonly status: "idle" }
  | { readonly status: "running" }
  | { readonly status: "done"; readonly response: DiagnosticsExportResponse }
  | { readonly status: "failed"; readonly message: string };

/** 詳細度の区画の状態。 */
type VerbosityState =
  | { readonly status: "loading" }
  | {
      readonly status: "ready";
      readonly level: DiagnosticsLevel;
      readonly levels: readonly DiagnosticsLevel[];
      readonly saving: boolean;
      /** 直前の変更の失敗（保存に失敗しても現在値の表示は保つ）。 */
      readonly error: string | null;
    }
  | { readonly status: "failed"; readonly message: string };

/** 区画の共通の枠。シェルの配色（`APPEARANCE_VARS`）だけを参照する。 */
const PANEL_STYLE = {
  padding: "1.25rem 1.5rem",
  borderRadius: "0.5rem",
  backgroundColor: `var(${APPEARANCE_VARS.screenPanel})`,
  color: `var(${APPEARANCE_VARS.screenText})`,
  textAlign: "left",
} as const;

/** 補助的な説明の見た目。 */
const MUTED_STYLE = {
  color: `var(${APPEARANCE_VARS.screenMuted})`,
  fontSize: "0.8125rem",
} as const;

/**
 * 区画の枠。**選ばれた区画は縁取りで強調される**ので、メニューからどの導線で来たかが
 * 画面上で分かる（外部の検査は `data-diagnostics-active` を読める）。
 */
function Panel({
  section,
  active,
  heading,
  children,
}: {
  readonly section: string;
  readonly active: boolean;
  readonly heading: string;
  readonly children: ReactNode;
}): ReactElement {
  return (
    <section
      data-testid={`jxcel-diagnostics-${section}`}
      data-diagnostics-section={section}
      data-diagnostics-active={active ? "true" : "false"}
      aria-label={heading}
      style={{
        ...PANEL_STYLE,
        border: active
          ? `2px solid var(${APPEARANCE_VARS.controlActiveText})`
          : `1px solid var(${APPEARANCE_VARS.controlBorder})`,
      }}
    >
      <h2 style={{ margin: "0 0 0.5rem", fontSize: "1rem" }}>{heading}</h2>
      {children}
    </section>
  );
}

/** 区画の中の操作。 */
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
        fontSize: "0.875rem",
        padding: "0.3rem 0.9rem",
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

/**
 * 設定値（`unknown`）を詳細度へ解釈する。解釈できない値は `null`。
 *
 * 判定は表示名の表（[`LEVEL_LABELS`]）の鍵で行うので、**閉じた列挙の写しを別に持たない**
 * （値が増えれば表がコンパイルエラーになる）。
 */
function asDiagnosticsLevel(value: unknown): DiagnosticsLevel | null {
  if (typeof value !== "string" || !Object.hasOwn(LEVEL_LABELS, value)) {
    return null;
  }
  return value as DiagnosticsLevel;
}

/** 詳細度の区画を、応答から組み立て直す。 */
function readyVerbosity(
  response: DiagnosticsVerbosityResponse,
): VerbosityState {
  return {
    status: "ready",
    level: response.level,
    levels: response.levels,
    saving: false,
    error: null,
  };
}

/** 診断の画面の実体。 */
export function DiagnosticsScreen(): ReactElement {
  const activeSection = useRequestedSection();
  const [location, setLocation] = useState<LocationState>({ status: "loading" });
  const [exported, setExported] = useState<ExportState>({ status: "idle" });
  const [verbosity, setVerbosity] = useState<VerbosityState>({
    status: "loading",
  });

  const locations = useRef<(HTMLElement | null)[]>([]);

  /** 保存場所を読み直す（**例外を外へ出さない**。封筒の失敗は区画の中へ出す）。 */
  const loadLocation = useCallback(async (): Promise<void> => {
    setLocation({ status: "loading" });
    const result = await invokeCommand<DiagnosticsLogLocationResponse>(
      LOG_LOCATION_COMMAND,
    );
    setLocation(
      result.status === "ok"
        ? { status: "ready", directory: result.data.directory }
        : { status: "failed", message: describeIpcError(result.error) },
    );
  }, []);

  /** 現在の詳細度を読み直す。 */
  const loadVerbosity = useCallback(async (): Promise<void> => {
    setVerbosity({ status: "loading" });
    const result = await invokeCommand<DiagnosticsVerbosityResponse>(
      VERBOSITY_GET_COMMAND,
    );
    setVerbosity(
      result.status === "ok"
        ? readyVerbosity(result.data)
        : { status: "failed", message: describeIpcError(result.error) },
    );
  }, []);

  // 画面が現れた時点で、保存場所と現在の詳細度を提示する（利用者が操作しなくても見える）。
  useEffect(() => {
    void loadLocation();
    void loadVerbosity();
  }, [loadLocation, loadVerbosity]);

  // 設定は全ウィンドウで共有される（要件 7.3）ので、他のウィンドウで変わった詳細度を
  // 追随する。**鍵の名前で絞る**（外観など他の鍵の通知は無視する）。
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void (async () => {
      try {
        const stop = await listen<SettingsChangedEvent>(
          SETTINGS_CHANGED_EVENT,
          (event) => {
            if (event.payload.key !== DIAGNOSTICS_LEVEL_SETTINGS_KEY) {
              return;
            }
            const level = asDiagnosticsLevel(event.payload.value);
            if (level === null) {
              console.warn(
                `詳細度の設定値が解釈できない: ${JSON.stringify(event.payload.value)}`,
              );
              return;
            }
            setVerbosity((current) =>
              current.status === "ready" ? { ...current, level } : current,
            );
          },
        );
        if (cancelled) {
          stop();
        } else {
          unlisten = stop;
        }
      } catch (error: unknown) {
        // IPC が無い環境では購読できない。自ウィンドウの操作は影響を受けない。
        console.warn("詳細度の変更の購読を登録できなかった", error);
      }
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // メニューから選ばれた区画を、見える位置へ送る。強調（`data-diagnostics-active`）は
  // 描画側が付けている。
  useEffect(() => {
    const index =
      activeSection === "location" ? 0 : activeSection === "export" ? 1 : 2;
    locations.current[index]?.scrollIntoView({ block: "nearest" });
  }, [activeSection]);

  /** 書き出しを要求する（宛先は Rust 側が決める。この画面はパスを渡さない）。 */
  const exportDiagnostics = useCallback(async (): Promise<void> => {
    setExported({ status: "running" });
    const result = await invokeCommand<DiagnosticsExportResponse>(EXPORT_COMMAND);
    setExported(
      result.status === "ok"
        ? { status: "done", response: result.data }
        : { status: "failed", message: describeIpcError(result.error) },
    );
  }, []);

  /** 詳細度を変更する（保存と適用は Rust 側の 1 つのコマンドが行う。要件 8.7）。 */
  const changeVerbosity = useCallback(
    async (level: DiagnosticsLevel): Promise<void> => {
      setVerbosity((current) =>
        current.status === "ready"
          ? { ...current, saving: true, error: null }
          : current,
      );
      const result = await invokeCommand<DiagnosticsVerbosityResponse>(
        VERBOSITY_SET_COMMAND,
        { request: { level } },
      );
      if (result.status === "ok") {
        setVerbosity(readyVerbosity(result.data));
        return;
      }
      // 保存に失敗した場合、設定は変更前のままである（4.5 の `write_to` の契約）。
      // 現在値の表示を保ったまま、失敗を区画の中に出す。
      const message = describeIpcError(result.error);
      console.warn(`詳細度を変更できなかった: ${message}`);
      setVerbosity((current) =>
        current.status === "ready"
          ? { ...current, saving: false, error: message }
          : { status: "failed", message },
      );
    },
    [],
  );

  return (
    <div
      data-testid="jxcel-diagnostics-screen"
      aria-label="診断"
      style={{
        display: "flex",
        flexDirection: "column",
        gap: "1rem",
        width: "min(48rem, 100%)",
      }}
    >
      <p style={{ margin: 0, ...MUTED_STYLE }}>
        記録の保存場所を確認し、1 つのファイルへ書き出し、詳細度を変更できます。場所を自動で
        開く操作は行いません（表示された文字列は選択してコピーできます）。
      </p>

      <div ref={(element) => { locations.current[0] = element; }}>
        <Panel section="location" active={activeSection === "location"} heading="記録の保存場所">
          <p style={{ margin: "0 0 0.5rem" }}>記録は次の場所に保存されています。</p>
          {location.status === "loading" ? (
            <p data-testid="jxcel-diagnostics-location-value" style={{ margin: 0 }}>
              読み込み中…
            </p>
          ) : null}
          {location.status === "ready" ? (
            <code
              data-testid="jxcel-diagnostics-location-value"
              style={{
                display: "block",
                padding: "0.4rem 0.6rem",
                borderRadius: "0.25rem",
                backgroundColor: `var(${APPEARANCE_VARS.shellSurface})`,
                color: `var(${APPEARANCE_VARS.shellText})`,
                wordBreak: "break-all",
                userSelect: "text",
              }}
            >
              {location.directory}
            </code>
          ) : null}
          {location.status === "failed" ? (
            <p
              data-testid="jxcel-diagnostics-location-error"
              style={{ margin: 0, color: `var(${APPEARANCE_VARS.controlActiveText})` }}
            >
              {location.message}
            </p>
          ) : null}
        </Panel>
      </div>

      <div ref={(element) => { locations.current[1] = element; }}>
        <Panel section="export" active={activeSection === "export"} heading="診断情報の書き出し">
          <p style={{ margin: "0 0 0.5rem" }}>
            保存されている記録を 1 つのファイルにまとめて書き出します。書き出し先は OS の
            ダウンロード領域（無ければホーム領域）で、ファイル名は jxcel-diagnostics-&lt;日時&gt;.log
            です。
          </p>
          <Action
            testId="jxcel-diagnostics-export"
            label={exported.status === "running" ? "書き出し中…" : "書き出す"}
            disabled={exported.status === "running"}
            onClick={() => {
              void exportDiagnostics();
            }}
          />
          {exported.status === "done" ? (
            <p data-testid="jxcel-diagnostics-export-result" style={{ margin: "0.5rem 0 0" }}>
              {exported.response.records === "empty"
                ? "記録は見つかりませんでしたが、1 つのファイルを書き出しました（中身は見出しのみ）: "
                : "記録を 1 つのファイルに書き出しました: "}
              <code style={{ userSelect: "text" }}>{exported.response.destination}</code>
            </p>
          ) : null}
          {exported.status === "failed" ? (
            <p
              data-testid="jxcel-diagnostics-export-error"
              style={{ margin: "0.5rem 0 0" }}
            >
              {exported.message}
            </p>
          ) : null}
        </Panel>
      </div>

      <div ref={(element) => { locations.current[2] = element; }}>
        <Panel
          section="verbosity"
          active={activeSection === "verbosity"}
          heading="記録の詳細度"
        >
          {verbosity.status === "loading" ? (
            <p data-testid="jxcel-diagnostics-verbosity-value" style={{ margin: 0 }}>
              読み込み中…
            </p>
          ) : null}
          {verbosity.status === "ready" ? (
            <>
              <p data-testid="jxcel-diagnostics-verbosity-value" style={{ margin: "0 0 0.5rem" }}>
                現在の詳細度: {LEVEL_LABELS[verbosity.level]}（{verbosity.level}）
              </p>
              <div
                role="group"
                aria-label="記録の詳細度"
                style={{ display: "flex", flexWrap: "wrap", gap: "0.4rem" }}
              >
                {verbosity.levels.map((level) => {
                  const chosen = level === verbosity.level;
                  return (
                    <button
                      key={level}
                      type="button"
                      data-testid={`jxcel-diagnostics-verbosity-option-${level}`}
                      aria-pressed={chosen}
                      disabled={verbosity.saving}
                      onClick={() => {
                        void changeVerbosity(level);
                      }}
                      style={{
                        font: "inherit",
                        fontSize: "0.8125rem",
                        padding: "0.25rem 0.7rem",
                        borderRadius: "0.25rem",
                        cursor: verbosity.saving ? "default" : "pointer",
                        color: chosen
                          ? `var(${APPEARANCE_VARS.controlActiveBackground})`
                          : `var(${APPEARANCE_VARS.screenText})`,
                        backgroundColor: chosen
                          ? `var(${APPEARANCE_VARS.controlActiveText})`
                          : `var(${APPEARANCE_VARS.screenPanel})`,
                        border: `1px solid var(${APPEARANCE_VARS.controlActiveText})`,
                      }}
                    >
                      {LEVEL_LABELS[level]}
                    </button>
                  );
                })}
              </div>
              <p style={{ margin: "0.5rem 0 0", ...MUTED_STYLE }}>
                変更はすぐに効き、次回の起動でも保たれます。{verbosity.saving ? "（保存中…）" : ""}
              </p>
              {verbosity.error !== null ? (
                <p
                  data-testid="jxcel-diagnostics-verbosity-error"
                  style={{ margin: "0.25rem 0 0" }}
                >
                  {verbosity.error}
                </p>
              ) : null}
            </>
          ) : null}
          {verbosity.status === "failed" ? (
            <p
              data-testid="jxcel-diagnostics-verbosity-error"
              style={{ margin: 0 }}
            >
              {verbosity.message}
            </p>
          ) : null}
        </Panel>
      </div>
    </div>
  );
}
