/**
 * 検証専用: 移植口の実装（`../grid/renderer/glideAdapter`）を**実物の上で駆動した結果**の形と、
 * それを**1 行の `[検証]` 行**にする純粋な関数（tasks.md 7.2）。
 *
 * # なぜ 1 行にするのか（経路の制約）
 *
 * アプリの診断記録（`TargetKind::Folder`）へは**フロントエンドから書けない**（実測: wry は
 * console を stdout へ流さず、`tauri-plugin-log` に `TargetKind::Webview` は無い。1.6 の
 * `research.md`「測れなかったこと」が同じ制約を記録している）。したがって観測の行は
 * **アクセシビリティの木**（DOM の `aria-label` → AT-SPI の名前）へ出し、`busctl` で読む
 * （1.6 の `scripts/check-render-traversal.sh` と同じ読み方。**新しい記録の仕組みを作らない**）。
 *
 * 1 行にするのは、読む側（`scripts/check-port-interaction.sh`）が `キー=値` を切り出して
 * 判定するためである。**値に空白を入れない**（読む側は空白で区切って探す）。
 *
 * # 状態を 2 語で持つ理由
 *
 * 観測が終わっていない行（`状態=未測定`）と、途中で失敗した行（`状態=failed`）と、成立した行
 * （`状態=ok`）を区別する。**「まだ終わっていない」を「成立した」と読ませない**のが要点である
 * （1.6 の計測が `measured` と `unmeasurable` を分けているのと同じ判断）。
 */
/**
 * 実物の上で駆動して観測した事実。**判定はしない**（判定は `check-port-interaction.sh` が持つ）。
 *
 * 3 つの主張（10 万行の走査・選択の区別・列幅と列の位置の操作）に、それぞれ対応する欄がある。
 * `ok` のときにだけ「主張を裏付ける値」が埋まる — 途中で失敗した場合は `reason` に理由が入り、
 * 残りは `null` / 0 のままである（**数を捏造しない**）。
 */
export interface PortProbeFacts {
  readonly status: "ok" | "failed";
  /** 失敗の理由（`ok` のときは空文字）。**空白を含んでよい**（読む側はこの欄を解析しない）。 */
  readonly reason: string;

  // ---- シートの形（`RendererSpec` が宣言したもの）----
  readonly rows: number;
  readonly columns: number;

  // ---- 主張 1: 10 万行を走査できる ----
  /** 末尾へ移動したあとに見えている最後の行（可視行の序数）。 */
  readonly reachedRow: number;
  readonly scrollTop: number;
  readonly scrollHeight: number;
  readonly clientHeight: number;
  /** 塗って読み戻す検査（既知の色が返るか）。 */
  readonly paintOk: boolean;
  readonly paintPixel: string;
  /** 標本の canvas の色数（1 は一様＝何も塗られていない）。 */
  readonly colors: number;

  // ---- 主張 2: 選択した範囲が視覚的に区別できる ----
  /** 移植口が受け取った選択の範囲（`start-end`。選択が無ければ `なし`）。 */
  readonly selection: string;
  /** アクセシビリティの木で選択として印されたセルの数。 */
  readonly selectedCells: number;
  /** 選択の前後で同じ画素の色が変わったか。 */
  readonly selectionPixelChanged: boolean;
  readonly selectionPixelBefore: string;
  readonly selectionPixelAfter: string;

  // ---- 主張 3: 列幅と列の位置が操作できる ----
  /** 列幅の変更で移植口が受け取ったもの（`添字:変更前→変更後`）。 */
  readonly resize: string;
  /** 仮想スクロールの内容の幅（全列の幅の合計）。変更の前後で増分が一致するかを見る。 */
  readonly contentWidthBefore: number;
  readonly contentWidthAfter: number;
  /** 列の移動で移植口が受け取ったもの（`from→to`）。 */
  readonly move: string;
  /** 移動の後に**描かれている見出しの並び**（表示順の先頭 4 列）。 */
  readonly headerOrder: string;

  // ---- クリップボードの配管（要件 7.1、7.2）----
  readonly copyRange: string;
  readonly copyChars: number;
  /** 複製の文字列が実際にクリップボードから読み戻せたか（`一致` / `不一致` / `不可`）。 */
  readonly clipboard: string;
  /** 貼り付けで移植口が受け取った錨。 */
  readonly pasteAnchor: string;
  /** 送った文字列と受け取った文字列が 1 バイトも違わないか。 */
  readonly pasteRoundTrip: boolean;

  // ---- 読み込み中の描き方（`loading: true`）----
  /**
   * データ領域を縦に走査した標本の数（読み込み中の帯の縦の刻みの数）。
   * {@link PortProbeFacts.loadingPixels} と比べる分母である。
   */
  readonly loadingStrip: number;
  /**
   * 骨組みの棒の内側の x で、**地色と違う画素**の数。
   *
   * 末尾は取得済みでない行である（値の文字も違反の帯も無い）。したがって地色と違う画素があれば、
   * それは読み込み中の骨組みの棒である。**0 なら何も描かれていない**（空白と区別がつかない）。
   */
  readonly loadingPixels: number;
  /** 値のあるセルの空いている部分の画素（**地色の基準**）。 */
  readonly backgroundPixel: string;

  /** 実行環境の WebKitGTK の版（`(不明)` のこともある）。 */
  readonly webkit: string;
}

/** まだ観測が終わっていないときの行（**成立と読ませない**）。 */
export function pendingPortProbeLine(): string {
  return "[検証] 移植口の操作: 状態=未測定 理由=駆動がまだ終わっていない";
}

/** 数の欄を読む側が扱いやすい形にする（整数はそのまま、小数は 1 桁へ丸める）。 */
function round(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(1);
}

/**
 * 観測した事実を 1 行にする。**入力に対して純粋**であり、描画の判断には使わない。
 *
 * 鍵の綴りは読む側（`scripts/check-port-interaction.sh`）と対である。**片方だけ変えない。**
 */
export function describePortProbe(facts: PortProbeFacts): string {
  return (
    `[検証] 移植口の操作: 状態=${facts.status}` +
    ` 行数=${String(facts.rows)} 列数=${String(facts.columns)}` +
    ` 到達行=${String(facts.reachedRow)}` +
    ` scrollTop=${round(facts.scrollTop)}` +
    ` scrollHeight=${round(facts.scrollHeight)}` +
    ` clientHeight=${round(facts.clientHeight)}` +
    ` 塗り=${facts.paintOk ? "ok" : "ng"} 画素=${facts.paintPixel}` +
    ` 色数=${String(facts.colors)}` +
    ` 選択=${facts.selection} 選択セル数=${String(facts.selectedCells)}` +
    ` 選択画素=${facts.selectionPixelChanged ? "変化" : "同じ"}` +
    ` 選択前=${facts.selectionPixelBefore} 選択後=${facts.selectionPixelAfter}` +
    ` 列幅=${facts.resize}` +
    ` 内容幅=${String(facts.contentWidthBefore)}→${String(facts.contentWidthAfter)}` +
    ` 列の移動=${facts.move} 見出し=${facts.headerOrder}` +
    ` 複製範囲=${facts.copyRange} 複製文字数=${String(facts.copyChars)}` +
    ` クリップボード=${facts.clipboard}` +
    ` 錨=${facts.pasteAnchor} 往復=${facts.pasteRoundTrip ? "一致" : "不一致"}` +
    ` webkit=${facts.webkit}` +
    ` 読み込み画素=${String(facts.loadingPixels)}/${String(facts.loadingStrip)}` +
    ` 地色画素=${facts.backgroundPixel}` +
    ` 理由=${facts.reason}`
  );
}
