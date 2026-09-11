# Research & Design Decisions: app-shell

## Summary

- **Feature**: `app-shell`
- **Discovery Scope**: New Feature（グリーンフィールド。フル調査を実施）
- **調査日**: 2026-09-10 / 2026-09-11。バージョンと issue の状態は crates.io / npm registry / GitHub に対して当日実測した値である
- **Key Findings**:
  1. **`externalBin` は Linux で壊れる。** AppImage のバンドル処理は `usr/bin` 配下の ELF に無条件で `patchelf` を掛け、Tauri は `externalBin` をまさにその `usr/bin` に置く。追記型ペイロードを持つ実行ファイル（Node SEA / pkg / Bun compile / PyInstaller / Nuitka）は破壊される。2022 年から未解決であり、除外設定も存在しない
  2. **型の単一定義と 10 万行の一括転送は、`tauri-specta` では両立しない。** JSON を経由しない唯一の経路である `tauri::ipc::Response` を `tauri-specta` が型付けできず、上流でブロックされている。`ts-rs` を採ることで衝突が消える
  3. **Rust から起動したサイドカーはアプリ終了時に kill されない。** `tauri-plugin-shell` の終了時クリーンアップは JS→IPC 経路で起動した子だけを対象とし、`CommandChild` に `Drop` もない。孤児プロセス対策は自前で実装する必要がある
  4. **描画失敗を検出する API は存在しない。** 「空白のウィンドウ」と「アセットの 404」は区別がつかない、というのが Tauri 自身の open issue の主張である。ハートビートによる検出を自前で作る
  5. **起動時間の最難関は Linux ではなく macOS である。** 公式ベンチ（2026-09-10 実測）で hello world が Linux 0.748 秒 / Windows 0.606 秒に対し macOS 1.564 秒

## Research Log

### Tauri v2 のバージョンと安定性

- **Context**: steering は「Tauri v2」とだけ決めており、具体的なバージョンと下限を確定する必要があった
- **Sources Consulted**: `https://crates.io/api/v1/crates/tauri`、`https://registry.npmjs.org/@tauri-apps/api`、`https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri/CHANGELOG.md`
- **Findings**:
  - `tauri` 2.11.5（2026-07-01）、`@tauri-apps/api` 2.11.1、`@tauri-apps/cli` 2.11.4、`tauri-build` 2.6.3。3.x は存在しない
  - 2.x に破壊的変更はなく semver 安定
  - **2.11.1 に security fix が 2 件**: `AppManifest` 未設定時に自前コマンドの ACL が迂回される問題、および Windows における `.localhost` サフィックスの origin 混同。2.11.1 未満へのピン留めは不可
  - 2.11.3 で `tauri://` プロトコルハンドラが非同期読み込みになり読み込み時間が改善
- **Implications**: 下限を `tauri >= 2.11.3` とする（security fix + 起動性能）。`@tauri-apps/api` の `next` dist-tag は 2.0.1 のまま放置されており、参照してはならない

### 単一インスタンス化の実際の挙動

- **Context**: 要件 1.5 は「新しいアプリケーションプロセスを増やさず」と書かれていた
- **Sources Consulted**: `tauri-plugin-single-instance` 2.4.4 のソース（`platform_impl/{windows,linux,macos}.rs`）、`https://v2.tauri.app/plugin/single-instance/`、プラグイン CHANGELOG
- **Findings**:
  - 実装は OS ごとに異なる: Windows は名前付きミューテックス `{identifier}-sim` + `WM_COPYDATA`、Linux は zbus セッションバス名 `{identifier}.SingleInstance`、macOS は `/tmp/{id}_si.sock` の Unix ドメインソケット
  - **2 つ目のプロセスは常に起動する。** 引数を引き渡した直後に `exit(0)` するだけであり、「プロセスを増やさない」は実現不能
  - Windows は argv を `|` で連結した単一文字列として渡す。`|` は Windows のファイル名に使えない文字なので実害は小さいが、引数一般では破損しうる
  - macOS は Launch Services が `.app` の二重起動を既に防いでいる。プラグインは CLI 起動と `open -n` のためにある
  - Linux は AppImage で動作するが、セッションバスがない環境（headless / ssh / コンテナ）では多重起動に落ちる
  - **ドキュメントとコードが矛盾している**: docs は D-Bus 名を `org.{id}.SingleInstance` と書くが、CHANGELOG 2.4.0（2026-02-05）が破壊的変更として `<bundle-id>.SingleInstance` に変更済み。docs の Snap/Flatpak の例は 2.4.0 以降で誤り
  - 2.4.3（2026-07-13）で macOS のスレッドブロック不具合を修正。下限は 2.4.3
- **Implications**: 要件 1.5 の文言を実測に合わせて修正した（「2 つ目のアプリケーションを常駐させず、引数を引き継いだうえで新しく起動した側を終了させる」）。プラグインは **最初に登録**する必要がある

### ウィンドウ管理と複数ウィンドウ

- **Context**: 1 ウィンドウ 1 ドキュメントという決定の実装可能性と、状態の持ち方
- **Sources Consulted**: `https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindowBuilder.html`、`https://v2.tauri.app/concept/process-model/`、`crates/tauri-runtime-wry/src/lib.rs` @ `tauri-v2.11.5`、`tauri-plugin-window-state` 2.4.1 のソース
- **Findings**:
  - **`WebviewWindowBuilder::build()` は Windows において同期コマンドおよびイベントハンドラ内でデッドロックする**（公式 docs に明記）。ウィンドウを開くコマンドは `async fn` でなければならない
  - 全ウィンドウは 1 つの Core プロセス・1 つの tao イベントループ上にある。Tauri は `data_directory` ごとに `WebContext` を共有し、Linux では web process を意図的に再利用する
  - **ウィンドウ単位の状態管理機構は存在しない。** `Manager::manage<T>()` は型ごとに 1 インスタンスのアプリ全体状態である。ウィンドウラベルをキーにしたレジストリを自前で持つ
  - コマンドは `window: tauri::WebviewWindow` を引数に宣言すると呼び出し元ウィンドウを識別できる
  - `tauri-plugin-window-state` はラベルをキーに状態を保存する。**ドキュメントごとに一意なラベルを付けると JSON が無限に増え、かつ何も復元されない。** `map_label` で束ねる必要がある
  - 同プラグインの**ディスク書き込みは `RunEvent::Exit` の時だけ**。クラッシュで位置情報は失われる
- **Implications**: ウィンドウレジストリを自前で持つ。ラベル規約を設計で固定する。位置サイズはウィンドウを閉じた時点で明示的に保存する（要件 2.7 はクラッシュ時の保証を求めていないが、`Exit` 依存だと通常の終了経路以外で失われる）

### メニューとキーボードショートカット

- **Context**: 要件 3.4 が「競合を検出して報告し、無言で無効化しない」ことを求めている
- **Sources Consulted**: `crates/tauri/src/window/mod.rs` @ 2.11.5、`https://github.com/tauri-apps/muda`（`accelerator/mod.rs`、`platform_impl/windows/mod.rs`）、`https://v2.tauri.app/learn/window-menu/`
- **Findings**:
  - **macOS ではメニューはアプリ全体で 1 つ。** `Window::set_menu` / `remove_menu` / `hide_menu` は macOS で非対応であり、該当分岐が `#[cfg(not(target_os = "macos"))]` で切られている。macOS では `AppHandle::set_menu` を使う
  - macOS ではトップレベル項目はすべて `Submenu` でなければならず、最初の submenu はアプリケーションメニューに畳み込まれる
  - **アクセラレータの競合検出はどこにも存在しない。** muda のエラーは解析失敗（`AcceleratorParseError`）だけである
  - **Windows では競合時の勝者が非決定的である。** アクセラレータテーブルが `HashMap::values()` から構築されるため、`TranslateAcceleratorW` が最初に一致した項目を採り、その順序は実行ごとに変わりうる
  - `tauri-plugin-global-shortcut` はアプリが非フォーカスでも発火する OS 全体のホットキーであり、アプリ内ショートカットには不適切
- **Implications**: **要件 3.4 は「あれば良い」ではない。** 自前の一意性検査を持たないと、Windows で実行ごとに挙動が変わる。メニュー構築時に検査する。macOS はアプリ全体メニュー 1 つ + フォーカス移動時の有効状態更新、Windows / Linux はウィンドウ単位メニューという二経路になる

### ウィンドウを閉じる操作の拒否

- **Context**: 要件 2.6 は「所有する機能へ問い合わせ、拒否された場合は閉じない」ことを求める。問い合わせは非同期になる
- **Sources Consulted**: `crates/tauri-runtime-wry/src/lib.rs`（`on_close_requested`）、`crates/tauri/src/manager/window.rs`、`packages/api/src/window.ts` @ 2.11.5
- **Findings**:
  - `CloseRequestApi::prevent_close()` はチャネルに送るだけであり、**ランタイムは `try_recv()` で非ブロッキングに読む**。したがって `prevent_close()` はリスナ内で同期的に呼ぶ必要があり、待ってから呼ぶことはできない
  - Tauri は非同期往復のための機構を既に持っている: **JS 側に `onCloseRequested` リスナが登録されているだけで自動的に veto される**（`has_js_listener` を見て `prevent_close()` してから `tauri://close-requested` を emit する）。フロント側が待ってから `destroy()` する
  - **`Window::close()` は `CloseRequested` を再発火するが `destroy()` はしない。** 拒否解除後に `close()` を呼ぶと veto に再突入する
- **Implications**: 非同期の可否問い合わせは JS 側リスナ経路に載せる。Rust 側から確定的に閉じるときは `destroy()` のみを使う

### 最後のウィンドウを閉じたときの挙動

- **Sources Consulted**: `crates/tauri-runtime-wry`、`crates/tauri/src/app.rs`、`https://github.com/tauri-apps/tauri/issues/13511`
- **Findings**:
  - **既定では 3 OS すべてでアプリが終了する。** 該当分岐は `cfg` で切られておらず、macOS の常駐慣習には従わない
  - macOS 常駐は `RunEvent::ExitRequested { code: None, api }` で `api.prevent_exit()` を呼ぶ。`RunEvent::Reopen { has_visible_windows }` が Dock アイコンのクリックに対応する
  - #13511（2025-05 から open）: 「最後のウィンドウが閉じた」と「ユーザーが終了を選んだ」を区別する手段が `code: None` の推定以外にない。無条件の `prevent_exit()` はプロセスを通常手段で終了できなくする
- **Implications**: 要件 2.9 は既定任せでは成立しない。macOS のみ `code.is_none()` を条件に `prevent_exit` し、明示的な終了メニューが `app.exit(0)` を呼ぶ構成にする

### 型共有とドリフト検出

- **Context**: 要件 4.2（型は単一定義から）と 4.3（不一致はビルド失敗）と 4.5（10 万行を 1 回で）を同時に満たす必要がある
- **Sources Consulted**: crates.io API、`https://raw.githubusercontent.com/specta-rs/tauri-specta/main/{src/lib.rs,src/lang.rs,src/builder.rs,examples/app/src/bindings.ts,.github/workflows/ci.yml}`、`https://raw.githubusercontent.com/Aleph-Alpha/ts-rs/main/README.md`、specta-rs/tauri-specta#158 / #170 / #194
- **Findings**:
  - `tauri-specta` 2.0.0-rc.25 はコマンドバインディングとイベントまで生成する唯一の選択肢。ただし **2023 年から RC のまま**であり、docs.rs のビルドが現在失敗中、公式が `=` でのピン留めを要求している
  - **生成される `bindings.ts` に `any` が 3 箇所ある**（`e as any`、`payload: any`、`Event<any>`）。lint 抑制ヘッダは付かない
  - **`tauri::ipc::Response` を `tauri-specta` が扱えない**（#170、open、「次の Specta リリースまでブロック、当面予定なし」）。`Vec<u8>` は `number[]` として出力される（#194）
  - `ts-rs` 12.0.1 は型のみ。ただし **`TS::export_to_string()` を持つ**ため、コミット済み `.ts` とのバイト比較が `#[test]` 内で完結する。`tauri-specta` には render-to-string 相当がなく、一時ファイルへの書き出しを経由するしかない
  - **どのツールもコンパイル時ドリフト検出を持たない。** `tauri-specta` 自身の CI も生成物を検査しておらず、`bindings.ts` は手動で再生成されている。「生成 → diff」は確立されたパターンではなく、自前で組む必要がある
  - `typeshare` は `syn` によるソース解析であり、トレイト解決もジェネリクス実体化も見ない。`taurpc` / `rspc` は採用実績が僅少（それぞれ約 2.2k / 5.8k recent downloads）
- **Implications**: 決定 1 と決定 2 を参照

### IPC の転送形式と大きなペイロード

- **Sources Consulted**: `crates/tauri/scripts/{ipc-protocol.js,process-ipc-message-fn.js,core.js}`、`crates/tauri/src/ipc/mod.rs`、`crates/tauri/src/ipc/channel.rs` @ 2.11.5、`https://v2.tauri.app/blog/tauri-20/`、tauri#12835
- **Findings**:
  - invoke はカスタムプロトコル（`ipc://localhost` / Windows は `http://ipc.localhost`）への `fetch` POST である
  - 応答の Content-Type が `application/json` でも `text/plain` でもなければ `response.arrayBuffer()` になる。すなわち **`tauri::ipc::Response::new(vec_u8)` は両側とも JSON を通らずに `ArrayBuffer` として届く**
  - 引数側で生バイトを送るには、バッファが**引数全体**でなければならない。オブジェクトの中にネストした `Uint8Array` は `Array.from()` で数値配列に変換される
  - **CSP を設定した場合 `connect-src ipc: http://ipc.localhost` が必須。** 欠けると `fetch` が拒否され、`console.warn` 1 行だけを残して恒久的に `postMessage` 文字列経路へ降格する（tauri#12835）
  - **`Channel` は一括転送には向かない。** 閾値（JSON 8192 バイト / raw 1024 バイト）を超えるメッセージは `eval` に加えて IPC 往復を 1 回追加する。進捗通知の意味論のためのものである
  - 公式ベンチは存在しない。第三者計測（64KB で JSON 2.272 ms 対 binary 202 µs）は Rust 側ディスパッチのみを測り、JS 側の解析コストを明示的に除外している
- **Implications**: 10 万行の経路は `tauri::ipc::Response` による生バイト 1 回。CSP の設定漏れは無言で性能を壊すため起動時アサーションを置く

### 権限とフロントエンドの到達範囲

- **Sources Consulted**: `crates/tauri/build.rs`、`crates/tauri/src/webview/mod.rs`、`crates/tauri/src/ipc/authority.rs`、`crates/tauri-utils/src/acl/mod.rs`、`crates/tauri-build/src/acl.rs`
- **Findings**:
  - **`core:default` にファイルシステムもシェルも含まれない。** `fs` と `shell` は別クレートであり、依存に入れず `.plugin()` もしなければフロントからの到達経路が存在しない。これが第一の制御である
  - **自前コマンドは既定では ACL の対象外**である。`src-tauri/permissions/*.toml` を定義して `__app-acl__` マニフェストが登録されて初めてゲートが効く
  - `tauri-build` が `src-tauri/gen/schemas/capabilities.json` と `acl-manifests.json` を出力する。**機械検査が可能である**
  - `build.removeUnusedCommands`（既定 `false`）は、どの capability からも許可されていないコマンドをビルド時に削る
  - `dynamic-acl` は tauri の既定 feature に含まれ、`windows: ["*"]` の実行時付与を可能にする
- **Implications**: 要件 4.7 は「設計上そうする」だけでなく **CI で機械検査できる**。決定 3 を参照

### サイドカーの同梱と AppImage

- **Context**: steering が最高リスクとして挙げた領域。`tauri-apps/tauri#11898` の現況確認が主目的
- **Sources Consulted**: tauri#11898 / #5189 / #5445 / #9981 / #7460 / #8894、linuxdeploy#149、`linuxdeploy/src/core/appdir.cpp`、`crates/tauri-bundler/src/bundle/linux/appimage/linuxdeploy.rs`、`crates/tauri-bundler/src/bundle/{linux/debian.rs,macos/app.rs}`、`https://docs.appimage.org/`
- **Findings**:
  - **#11898 は open だが 2024-12-07 以降 21 か月間まったく動きがない。** 実質的に停止している
  - **正典は #5189（2022-09-14 から open、最終更新 2026-01-06）である。** プラグインなしの素の `linuxdeploy --appdir` が実行ファイルを書き換えることが md5 比較で証明されており、2026-01 に Tauri v2 + Bun compile で再現が報告されている
  - 根本原因を上流ソースで確認: `listExecutables()` が `usr/bin` を**非再帰**で走査し、ELF として解釈できるものすべてに `$ORIGIN/../lib` の rpath を**無条件で**設定する。`usr/lib` は**再帰**走査され、既存 rpath に追記される
  - **`NO_STRIP=1` は効かない。** 「rpath が `$` で始まるならスキップ」というガードは `strip` の周りにしかなく、`setRPath` には掛かっていない
  - **静的リンクされた実行ファイルは明示的にスキップされる**（"Not setting rpath in statically-linked file"）
  - **Tauri は `externalBin` を Linux で `usr/bin` に置く。** まさに patchelf の対象ディレクトリであり、除外設定は存在しない
  - **検証済みの回避路**: `bundle.linux.appimage.files` は任意のパスへコピーでき、権限を保持する。`linuxdeploy` が走査するのは `usr/bin`（非再帰）と `usr/lib`（再帰）だけなので、**`usr/share/<app>/` に置いたものは patchelf を通らない**。一方 `bundle.resources` は Linux で `usr/lib/${exe_name}` に落ちるため**回避路にならない**
  - macOS では bundler が `Contents/MacOS/` に置き、`is_an_executable: true` として **codesign の対象に含める**
  - AppImage は読み取り専用の squashfs を FUSE でマウントする。`$APPDIR` から**その場で実行できる**（`AppRun` 自体がそうである）。ただし**壊れたサイドカーをその場で修復することはできない**
  - #9981 は命名規約の誤りであり Tauri の不具合ではない（設定にはターゲットトリプル接尾辞なしの名前を書く）。#7460 は macOS の**ビルド時** EACCES であり実行時のサイドカー権限問題ではない
  - `include_bytes!` + 展開は Tauri の公認パターンではない（#8894 が open）。**macOS arm64 では署名のない実行ファイルがカーネルに SIGKILL される**ため、展開経路は macOS で新たなリスクを生む
- **Implications**: 決定 4 を参照。要件 5.2 が指定していた「アプリケーションデータ領域へ配置して起動」は 3 OS すべてで不適切であり、要件から機構の指定を外した

### サイドカーのライフサイクル

- **Sources Consulted**: `plugins/shell/src/{lib.rs,commands.rs,process/mod.rs}`、plugins-workspace#1332 / #3062、tauri#11686
- **Findings**:
  - `tauri-plugin-shell` は `RunEvent::Exit` で子を kill する。**ただし対象は `Shell::children` に登録されたもの、すなわち JS→IPC 経路で起動した子だけである**
  - **Rust から `app.shell().sidecar(..).spawn()` した子は登録されず、`CommandChild` に `Drop` もない。アプリ終了時に kill されない**
  - `CommandChild::kill()` は直接の子だけを対象とする（Unix は SIGKILL、Windows は `TerminateProcess`）。プロセスグループも Job Object も使わない。フォークした孫は孤児になる
  - **クラッシュ時（SIGSEGV / SIGKILL）は `RunEvent::Exit` が発火しないため、サイドカーは 3 OS すべてで生き残る。** WebKitGTK の SIGSEGV は現在も open な不具合クラス（#14721）であり、これは絵空事ではない
  - plugins-workspace#1332（プロセスグループ対応）は open のまま未実装。#3062 も open
  - `shell` のスコープ検査は `#[tauri::command]` ハンドラ経由でのみ効く。**Rust から起動する限り capability の付与は一切不要である**
- **Implications**: 決定 5 を参照。Rust 側で起動し、監督機構を自前で持つ。これは要件 4.7（フロントに任意プロセス起動経路を与えない）と要件 5.6（孤児を残さない）を同時に満たす唯一の構成である

### ログと設定の永続化

- **Sources Consulted**: `tauri-plugin-log` 2.9.1 と `tauri-plugin-store` 2.4.4 のソース、`https://v2.tauri.app/plugin/logging/`
- **Findings**:
  - ログの既定保存先は Linux `$XDG_DATA_HOME/{id}/logs`、macOS `~/Library/Logs/{id}`、Windows `{LocalAppData}/{id}/logs`
  - **既定値が罠である**: `DEFAULT_MAX_FILE_SIZE = 40_000`（40 KB）かつ `DEFAULT_ROTATION_STRATEGY = KeepOne`。明示設定しないと直近 40 KB しか残らない
  - `RotationStrategy::KeepSome(u32)` がソースに存在するが**公式ドキュメントには記載がない**（docs は `KeepAll` と `KeepOne` しか挙げていない）
  - プラグインに**子プロセスの出力を取り込む機能はない**
  - `tauri-plugin-store` は 1 プロセス内の複数ウィンドウに対して安全である（解決済みパスで dedup し、同一の `Arc<Store>` を返す）
  - **`Store::save()` は原子的ではない**。`fs::write` による truncate-then-write であり、temp + rename も fsync もない。オートセーブは 100 ms のデバウンス付き
  - プロセス間のロックは存在しない
- **Implications**: 決定 6 を参照。ログはプラグインを採るが既定値を上書きする。設定ストアは自前で持つ

### Linux / WebKitGTK の描画

- **Sources Consulted**: tauri#5761 / #7021 / #13157 / #5143 / #15936 / #10702 / #14924 / #14721 / #9394 / #15665 / #15976 / #15902 / #7155、`https://v2.tauri.app/develop/debug/linux-graphics/`
- **Findings**:
  - **steering の tech.md が挙げている 3 件はいずれも既にクローズしている**: #5761（2022-12-05、distro の webkit2gtk が WebGL 2.0 を欠くため対処不能として close）、#7021（2023-10-21、WebKitGTK 2.40 の回帰。2.42.1 で解消）、#13157（2025-04-07、**NOT_PLANNED で close。修正されていない**。WebKitGTK 2.46.5 では起きず 2.48.0 で起きることが bisect されている）
  - **現在 open で重要なもの**: #5143（白画面、2022 から open、コメント 53 件）、#15936（ソフトウェア GL 下で白いウィンドウ、**診断手段がないこと自体が主題**、2026-08-31）、#14721（NVIDIA 環境で `libwebkit2gtk-4.1.so` が SIGSEGV）、#10702 / #14924（Wayland Error 71）
  - **回避策に公式見解はなく、情報源が矛盾している**。`WEBKIT_DISABLE_DMABUF_RENDERER=1` は高速描画経路を捨てる。`WEBKIT_DISABLE_COMPOSITING_MODE=1` はハードウェア支援を切る。`__NV_DISABLE_EXPLICIT_SYNC=1` は性能を落とさない唯一の対処だという報告が複数あるが Tauri の裁定はない
  - **公式ドキュメントは無条件適用を戒めている**（一部ユーザーの問題を全ユーザーの性能低下と引き換えに直すことになる）
  - **Tauri も wry もこれらを自動設定しない**（両リポジトリに該当文字列が 1 件も存在しない）。設定するなら GTK/WebKit のコードが動く前、`tauri::Builder` の前である
  - **描画失敗を検出する API は存在しない。** #15936 はまさにその診断の要求であり、コメント 0 件で open のまま
  - Monaco に言及した Tauri issue は 1 件も存在しない（未検証。露出は #13157 のリサイズ時ゴーストと #5761 の canvas 劣化を通じた間接的なもの）
- **Implications**: 決定 7 を参照。steering の tech.md の Known Risks 3 番は事実として古い

### AppImage の自己完結性とサイズ

- **Findings**:
  - Tauri のバンドラは `webkit2gtk-4.1` から `WebKitNetworkProcess` / `WebKitWebProcess` / injected bundle をディレクトリ名ハードコードでコピーしており、ソースに `// TODO: Check if it's the same dir name on all systems` が残っている
  - 実測例（Tabularis 0.16.0）: AppImage 93.7 MB に対し .deb 17 MB / .msi 15.8 MB / .dmg 18.3 MB。**言語サーバとその実行環境を加えると 150〜200 MB を見込む必要がある**
  - **直近四半期に既定の AppImage 経路の回帰が複数 open**: #15665（Mesa 25+ で失敗）、#15976（Fedora 44 で `EGL_BAD_PARAMETER`）、#15902（XWayland なしでハードクラッシュ）、#7155（FUSE2）
  - 失敗の形はいずれも同じで、ホスト側と一致していなければならない低レベルライブラリ（libwayland / Mesa / libxkbcommon）を同梱してしまうことに起因する。**自己完結性そのものが原因である**
  - 一方で自己完結は #13157 のようなホスト側 WebKitGTK の回帰からは守ってくれる（古い動作する WebKitGTK に固定できる）
  - ビルドは最も古いターゲット（Ubuntu 22.04 / Debian 12）で行う必要がある。ARM の AppImage はクロスコンパイルできない
  - Tauri は長期的に CEF（`tauri-apps/cef-rs`）へ向かっているが、リリースも時期もない
- **Implications**: roadmap の「約 76MB」は WebKitGTK 分のみの数字であり、サイドカーを含む実際の配布物では過小である。設計では配布物サイズを CI で記録する（要件 6.6）ことで実測に置き換える

### 起動時間

- **Sources Consulted**: `https://github.com/tauri-apps/benchmark_results`（2026-09-10 実行分）
- **Findings**:
  - hello world の起動から `DOMContentLoaded` まで: **Linux 0.748 秒 / Windows 0.606 秒 / macOS 1.564 秒**
  - 生の wry との差は Linux 約 20 ms、Windows 約 10 ms、macOS 約 350 ms。支配的なのは OS の WebView 初期化であり Tauri でも自前の Rust でもない
  - これらは 3 回のウォームアップを捨てた**ウォーム実行**であり、CI ハードウェア上の些末なページに対する値である
- **Implications**: **2 秒予算に対して最も余裕がないのは macOS である。** 要件を書いた時点では Linux が最難関だと想定していたが逆であった。Windows の初回起動は WebView2 ランタイムがページキャッシュに乗っていないため定常状態より悪い

## Architecture Pattern Evaluation

| Option | 説明 | Strengths | Risks / Limitations | 判定 |
|---|---|---|---|---|
| **Tauri 非依存コア + 薄いアダプタ**（採用） | プロセス監督・整合性検査・設定・ショートカット検査を `crates/app-shell/` に置き、`src-tauri/` は Tauri への接続だけを行う | steering の層規則をそのまま満たす。GUI を起動せずにテストできる。監督機構は Tauri の不具合と独立に検証できる | クレートが 1 つ増える。Tauri の型を跨げない箇所で変換が必要 | 採用 |
| すべてを `src-tauri/` に置く | 単一クレート。変換が不要 | 最短距離 | プロセス監督とショートカット検査のテストに GUI 起動が必要になる。structure.md の「ドメインのテストに GUI 起動が必要なら層が壊れている」に反する | 却下 |
| プラグイン化（`tauri-plugin-*` として実装） | app-shell の機能を Tauri プラグインとして切る | Tauri の作法に沿う。再利用可能 | プラグインコマンドは常に ACL 対象になり capability の管理が増える。再利用の相手が存在しない（アプリは 1 つ） | 却下（投機的抽象） |
| フロント主導（JS からサイドカーを起動） | `tauri-plugin-shell` の JS API を使う | プラグインが終了時 kill を面倒みる | フロントにプロセス起動経路を与えることになり要件 4.7 に反する。capability のスコープ管理が必要になる。命名規約の罠を踏む | 却下 |

## Design Decisions

### Decision 1: 型共有は `ts-rs` を採り、`tauri-specta` を却下する

- **Context**: 要件 4.2（型は単一定義から）・4.3（不一致はビルド失敗）・4.5（10 万行を 1 回で）を同時に満たす必要がある
- **Alternatives Considered**:
  1. `tauri-specta` 2.0.0-rc.25 — 型 + コマンドバインディング + イベントを生成する唯一の選択肢
  2. `ts-rs` 12.0.1 — 型のみ生成。コマンドのラッパは手書き
  3. `tauri-specta` + ローカルプラグインに生バイトコマンドを逃がす二経路構成
  4. `typeshare` / `taurpc` / `rspc`
- **Selected Approach**: `ts-rs` 12.0.1 で境界の型を生成し、コマンド名は Rust 側の単一配列から定数として生成し、`invoke` の薄いラッパを手書きする
- **Rationale**:
  - **`tauri-specta` は要件 4.5 を構造的に満たせない。** JSON を通さない唯一の経路である `tauri::ipc::Response` を型付けできず（#170、open、上流ブロック・ETA なし）、`Vec<u8>` は `number[]` に落ちる
  - 選択肢 3 は二経路構成になるうえ、プラグイン経路と specta の共存は調査者が「ソースからの推論であり未検証」と明記している。最重要の継ぎ目を未検証の推論に載せない
  - **生成物に `any` が入る**（`e as any` / `payload: any` / `Event<any>`、lint 抑制ヘッダなし）。steering の「TS で `any` 禁止」と正面から衝突する
  - 2023 年から RC のまま、docs.rs のビルドが現在失敗中、`=` でのピン留めを公式が要求。スタックの他の部分（tauri 2.11.5）と成熟度が釣り合わない
  - `ts-rs` は `export_to_string()` を持つため**決定 2 のドリフト検査が `cargo test` 内で完結する**。`tauri-specta` にはこの原始的操作がない
- **Trade-offs**: コマンドごとに 1 行程度のラッパを手書きする。イベントの型付けも手書きになる。代わりに、生バイト経路・JSON 経路・イベントのすべてが同一の型定義の上に乗り、`any` が生成物に混入しない
- **Follow-up**: ラッパのコマンド名がドリフトしないよう、名前定数を生成物に含めて同じバイト比較で検査する

### Decision 2: 「不一致はビルド失敗」は 2 段で構成する

- **Context**: 要件 4.3。調査の結論は「どのツールもコンパイル時ドリフト検出を持たない」である
- **Selected Approach**:
  1. `crates/app-shell/tests/bindings_drift.rs` が `TS::export_to_string()` の結果とコミット済み `src/ipc/bindings.ts` をバイト比較する。差があれば `cargo test` が落ちる
  2. `tsc --noEmit` がフロント側の追随漏れを落とす
- **Rationale**: (1) だけでは「生成物が古い」しか捕まらず、(2) だけでは「Rust 側が変わったのに生成物を更新していない」を捕まえられない。両方でなければ要件 4.3 は成立しない
- **Trade-offs**: 生成物をリポジトリにコミットする必要がある。代わりに、フロントエンドのビルドが Rust ツールチェーンに依存しない
- **Follow-up**: `tauri-specta` 自身の CI が生成物を検査していないことから分かるとおり、これは確立されたパターンではない。自前で保守する対象である

### Decision 3: フロントエンドに fs / shell 経路を与えず、CI で機械検査する

- **Context**: 要件 4.7
- **Selected Approach**:
  1. `tauri-plugin-fs` と `tauri-plugin-shell` を**依存に入れない**。`core:default` にはどちらも含まれないため、到達経路自体が存在しなくなる
  2. `src-tauri/permissions/app.toml` を定義して `__app-acl__` を有効にし、自前コマンドも ACL の対象にする（既定では対象外である）
  3. `scripts/check-capabilities.sh` が `src-tauri/gen/schemas/capabilities.json` を検査し、`^(fs|shell):` の権限 id と `windows: "*"` を検出したら CI を落とす
  4. `build.removeUnusedCommands: true` を設定する
- **Rationale**: 「設計上そうする」だけでは退行を防げない。`tauri-build` が機械可読な成果物を吐くため、要件 4.7 は検査可能な性質にできる
- **Trade-offs**: ファイル選択ダイアログのために `tauri-plugin-dialog` は入れる。ただしダイアログが返すのはパスだけであり、読み書きは Rust 側が行う
- **Follow-up**: `dynamic-acl` は tauri の既定 feature に含まれ `windows: ["*"]` の実行時付与を許す。硬化ビルドでは `default-features = false` を検討する（本スペックでは行わず、Revalidation Trigger として記録する）

### Decision 4: サイドカーの配置はプラットフォームごとに分ける

- **Context**: 要件 5.1・5.2・5.3。`externalBin` は Linux で壊れる
- **Alternatives Considered**:
  1. 3 OS すべてで `externalBin`
  2. 3 OS すべてで `include_bytes!` + アプリケーションデータ領域へ展開
  3. プラットフォーム別（採用）
- **Selected Approach**:
  - **Windows / macOS**: `externalBin`。その場で実行する。macOS では bundler が codesign する
  - **Linux**: `bundle.linux.appimage.files` で `usr/share/jxcel/` に置く。`linuxdeploy` の走査対象（`usr/bin` 非再帰、`usr/lib` 再帰）の**どちらにも該当しない**ため patchelf を通らない。読み取り専用マウントからその場で実行する
  - どのプラットフォームでも、**通常経路では展開しない**
  - **整合性検査で不一致を検出した場合も修復しない。**AppImage は読み取り専用の squashfs でありその場で置き換えられず、複製して起動する経路は「壊れているかもしれない実行ファイルを別の場所で動かす」ことになる。要件 5.3 が求めているのは検出と報告であり、修復ではない（design.md の Error Handling と一致させた）
- **Rationale**:
  - 選択肢 1 は Linux で破壊される。2022 年から未解決で、除外設定がなく、`NO_STRIP=1` も効かない
  - 選択肢 2 は macOS arm64 で**署名のない実行ファイルがカーネルに SIGKILL される**新たなリスクを生む。`externalBin` なら bundler が codesign してくれるものを、わざわざ捨てることになる
  - `bundle.linux.appimage.files` 経路は上流ソースで確認済みである（`copy_custom_files` → `fs::copy` で権限保持、`linuxdeploy.rs` が AppDir へコピーするのは `data_dir/usr/` 配下のみ）
  - **`bundle.resources` は回避路にならない**。Linux では `usr/lib/${exe_name}` に落ち、再帰走査の対象になる
- **Trade-offs**: 3 OS で配置が異なるため、実行時のパス解決に分岐が入る。追記型ペイロードを持つ実行ファイル（Node SEA / pkg / Bun compile）は Linux で採用できない — これは `macro-editor-lsp` への申し送りである
- **Follow-up**: 静的リンクされた実行ファイルは `linuxdeploy` が明示的にスキップする。言語サーバの配布形態を選ぶ段階でこの選択肢も評価する

### Decision 5: サイドカーは Rust から起動し、監督機構を自前で持つ

- **Context**: 要件 5.5・5.6・5.7・5.8 と要件 4.7
- **Selected Approach**: `app.shell()` の JS 経路を使わず Rust 側で起動する。終了保証は Windows が Job Object + `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`、Unix がプロセスグループ + `killpg`。ハードクラッシュに対しては親 PID をサイドカーに渡して自己終了させ、加えて起動時に古い PID を掃除する（PID 再利用を避けるため PID と実行ファイル名の両方で照合する）
- **Rationale**:
  - **`tauri-plugin-shell` の終了時クリーンアップは Rust から起動した子を対象にしない**。`Shell::children` に登録されるのは JS→IPC 経路だけであり、`CommandChild` に `Drop` もない
  - `CommandChild::kill()` は直接の子しか殺さない。フォークした孫は孤児になる
  - **クラッシュ時は `RunEvent::Exit` が発火しない**。WebKitGTK の SIGSEGV は現在も open な不具合であり、これは現実的な経路である。Windows の Job Object はカーネルが強制するため、こちらのクラッシュ後も有効な唯一の機構である
  - Rust から起動する限り `shell` の capability 付与が**一切不要**になり、要件 4.7 と同時に満たせる
  - plugins-workspace#1332（プロセスグループ対応）は open のまま未実装であり、待つ選択肢はない
- **Trade-offs**: プラットフォーム別のプロセス管理コードを持つ。ただしこれは Tauri 非依存であり、GUI なしでテストできる
- **Follow-up**: サードパーティの `tauri-plugin-sidecar` が同種の機能を主張しているが未監査であり、採用しない

### Decision 6: 設定ストアは自前で持ち、ログはプラグインを既定値を上書きして使う

- **Context**: 要件 7.1〜7.7、8.1〜8.7
- **Selected Approach**:
  - **設定**: `crates/app-shell/src/settings/` に実装する。temp + rename + fsync による原子的書き込み、未知キーの保持、読み取り失敗時の既定値起動
  - **ログ**: `tauri-plugin-log` 2.9.1 を採るが `max_file_size` と `RotationStrategy::KeepSome` を明示設定する
- **Rationale**:
  - `tauri-plugin-store` の `Store::save()` は `fs::write` による truncate-then-write であり**原子的でない**。クラッシュで切り詰められた JSON が残る
  - 同プラグインは**未知キーの保持**（要件 7.6）と**破損時の既定値起動**（要件 7.5）の意味論を持たない。要件 7.6 は `document-format` が確立した「理解できないものを壊さない」原則の継承である
  - 自前実装は 1 プロセス内共有（要件 7.3・7.4）を `Arc<RwLock<_>>` で満たす。プラグインが与える多重ウィンドウ安全性は自前でも同じ形で得られる
  - ログの既定値（40 KB / `KeepOne`）は要件 8.5（50 MB 上限）と桁が違う。`KeepSome` は**ソースには存在するが公式ドキュメントに記載がない**
- **Trade-offs**: 設定ストアのコードを持つ（小さい）。代わりに依存が 1 つ減り、GUI なしでテストできる
- **Follow-up**: ログのローテーション設定は公式ドキュメントに載っていない API を使うため、バージョン更新時に確認する

### Decision 7: 描画の健全性は自前のハートビートで検出し、回避策は条件付きで適用する

- **Context**: 要件 10.1〜10.4
- **Selected Approach**:
  1. **初回描画ハートビート**: ウィンドウ生成時に Rust 側が監視を開始し、フロントがマウント後の `requestAnimationFrame` 内からコマンドを呼ぶ。一定時間内に届かなければ診断情報に記録し、回避策付きで再起動する経路を提示する
  2. **ソフトウェアラスタライズの検出**: `WEBGL_debug_renderer_info` の `UNMASKED_RENDERER_WEBGL` を照合する
  3. 環境変数による回避策は**検出された場合にのみ**適用する。無条件適用はしない
- **Rationale**:
  - **描画失敗を検出する API は存在しない**。#15936 はまさにその診断の要求であり、その主張は「白いウィンドウとアセットの 404 は見分けがつかない」である。要件 10.2 を満たすには自前で作るしかない
  - 公式ドキュメントが無条件適用を戒めている。`WEBKIT_DISABLE_DMABUF_RENDERER=1` は高速描画経路を、`WEBKIT_DISABLE_COMPOSITING_MODE=1` はハードウェア支援を捨てる
  - 回避策の優先順位について**情報源が矛盾しており Tauri の裁定がない**。したがって設計は特定の変数に賭けず、検出 → 適用 → 記録という枠組みだけを固定する
- **Trade-offs**: ハートビートのために起動経路にコマンドが 1 つ増える
- **Follow-up**: 環境変数は GTK/WebKit のコードが動く前、`tauri::Builder` の前に設定する

### Decision 8: フロントエンドは React + Vite

- **Context**: steering は「TypeScript」としか決めていない。app-shell がシェル構造を所有する以上ここで確定する必要がある
- **Selected Approach**: React + Vite（SPA 構成）
- **Rationale**:
  - **`monaco-languageclient` の一次ラッパは React のみ**である（`@typefox/monaco-editor-react`）。Svelte / Solid / Vue のラッパは存在しない。`macro-editor-lsp` の中心的な依存がここにある
  - canvas ベースの仮想化グリッドで実績のあるものも React 前提である。**Svelte / Solid を一級で支援する canvas グリッドは見つからなかった**
  - jxcel の最難関 UI である `macro-editor-lsp` と `data-grid` が同じ方向を指している
  - Tauri は SSR を支援しない。SPA / SSG のみである。Vite は Tauri 公式が SPA フレームワーク向けに推奨している
- **Trade-offs**: フレームワークの選択肢を早期に固定する。ただし後続 2 スペックの主要依存が実質的に決めている
- **Follow-up**: Glide Data Grid は stable が 2024-02-03 で止まり alpha が 2.5 年続いている。**グリッド実装の選定は `data-grid` スペックで再評価する**。app-shell は React + Vite までしか約束しない

### Decision 9: 機構検証専用の補助プロセスを app-shell が自ら所有する

- **Context**: 要件 5.1〜5.9 と 6.4 / 6.5 は補助プロセスの同梱・整合性検査・起動・共有・終了・出力取得を要求する。しかし**本スペックの実装時点で同梱すべき実行ファイルが存在しない**（言語サーバは `macro-editor-lsp` が選定する）。このままでは実装も検証もできない
- **Alternatives Considered**:
  1. `macro-editor-lsp` まで待ち、app-shell では機構だけ書いて検証しない
  2. 任意の既存バイナリ（`echo` など）を借りて検証する
  3. 検証専用の最小の補助プロセスを app-shell が所有する
- **Selected Approach**: `crates/sidecar-smoke/` として、親 PID を監視して自己終了し標準出力へ応答するだけの bin クレートを持つ
- **Rationale**:
  - 選択肢 1 は、最高リスク領域（決定 4・決定 5）を未検証のまま次スペックへ送ることになる。roadmap が prototype-first risk として挙げた項目そのものであり、先送りは本末転倒である
  - 選択肢 2 では親 PID 監視による自己終了（ハードクラッシュ経路の唯一の対策）を検証できない。またプラットフォームごとにパスが異なり、バンドル検証の対象にならない
  - 選択肢 3 は、バンドル後の取り出しと起動（要件 6.4）およびバイト一致（要件 6.5）を CI で実際に回せる。**Linux のバンドル処理が実行ファイルを書き換える問題に対する恒久的なカナリアになる**
- **Trade-offs**: 検証専用のクレートが 1 つ増える。配布物にごく小さな実行ファイルが 1 つ乗る
- **Follow-up**: `macro-editor-lsp` が実物を登録する際、この smoke を配布物から外すか残すかを判断する

## Risks & Mitigations

- **AppImage の既定バンドル経路に直近四半期の回帰が複数 open**（#15665 / #15976 / #15902 / #7155）。失敗の形は一貫して「ホストと一致すべき低レベルライブラリを同梱してしまう」ことである — 最も古いターゲット（Ubuntu 22.04 / Debian 12）でビルドし、配布物を CI で実際に起動して検証する（要件 6.4）
- **配布物サイズが roadmap の想定を超える**。WebKitGTK だけで約 76 MB、言語サーバとその実行環境を含めると 150〜200 MB — 要件 6.6 のサイズ記録で実測に置き換え、roadmap を更新する
- **`ts-rs` の生成物が古いまま気付かれない** — 決定 2 の 2 段検査。ただしこれは自前で保守するパターンである
- **CSP の設定漏れで IPC が無言で `postMessage` 経路へ降格する** — 起動時アサーションを置く（要件 4.5 の性能が静かに失われる）
- **macOS の起動時間が 2 秒予算に対して最も余裕がない**（hello world で 1.564 秒） — 要件 6.8 の計測を 3 OS で行い、macOS を基準に判断する
- **`tauri-plugin-window-state` は 2025-10-27 以降リリースがない**。不保守の証拠はないが、この依存群で最も更新が停滞している — 位置サイズの保存は自前でも実装できる規模であり、問題が出たら内製に切り替える
- **`prevent_exit` の無条件適用はプロセスを通常手段で終了できなくする**（#13511） — `code.is_none()` を条件とし、明示的な終了メニューを必ず用意する
- **PID 再利用による誤 kill** — 掃除時は PID と実行ファイル名の両方で照合する

## References

- [tauri-apps/tauri#5189](https://github.com/tauri-apps/tauri/issues/5189) — AppImage のサイドカー破壊の正典。2022-09-14 から open、2026-01-06 に Tauri v2 で再現報告
- [tauri-apps/tauri#11898](https://github.com/tauri-apps/tauri/issues/11898) — steering が挙げていた issue。open だが 2024-12-07 以降停止
- [linuxdeploy#149](https://github.com/linuxdeploy/linuxdeploy/issues/149) — rpath 書き換えのスキップ要求。2020 年から open
- [tauri-apps/tauri#15936](https://github.com/tauri-apps/tauri/issues/15936) — ソフトウェア GL 下の白いウィンドウ。診断手段がないこと自体が主題
- [tauri-apps/tauri#13157](https://github.com/tauri-apps/tauri/issues/13157) — WebKitGTK 2.48.0 の描画不良。NOT_PLANNED で close（未修正）
- [tauri-apps/tauri#13511](https://github.com/tauri-apps/tauri/issues/13511) — 最後のウィンドウと明示的終了を区別できない問題
- [specta-rs/tauri-specta#170](https://github.com/specta-rs/tauri-specta/issues/170) — `tauri::ipc::Response` を扱えない。上流ブロック
- [plugins-workspace#1332](https://github.com/tauri-apps/plugins-workspace/issues/1332) — サイドカーのプロセスグループ対応。未実装
- [tauri-apps/benchmark_results](https://github.com/tauri-apps/benchmark_results) — 起動時間の公式継続計測
- [Tauri: Linux graphics debugging](https://v2.tauri.app/develop/debug/linux-graphics/) — 環境変数による回避策と、無条件適用への戒め
- [AppImage architecture](https://docs.appimage.org/reference/architecture.html) — 読み取り専用 squashfs マウント
