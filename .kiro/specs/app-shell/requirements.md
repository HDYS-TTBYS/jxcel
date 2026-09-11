# Requirements Document

## Introduction
本機能は jxcel のアプリケーションシェル、すなわち「器」を定義する。jxcel を受け取って使い始めるユーザーに対し、Windows / macOS / Linux のいずれでも単一のファイルを実行するだけで立ち上がり操作できるデスクトップアプリケーションを提供する。同時に、後続の 13 スペックがその上に乗るための共通基盤 — フロントエンドとドメインコアの通信境界、補助プロセスの同梱と起動、個別画面が差し込まれるシェル構造 — を確立する。

現状、ドキュメント形式（`document-format`）は GUI を起動せずに動作するライブラリとして実装済みだが、それを画面に載せる手段が存在しない。また、通信境界を最初に確定しておかなければ、後続スペックがそれぞれ独自の呼び出し口を生やし、型の保証とテスト容易性が失われる。器がなければどの機能もユーザーに届かず、境界がなければ届いた頃には保守できなくなっている。

本機能が乗る技術基盤（Tauri v2、および各 OS が提供する WebView）は steering で確定済みの前提である。本要件はその選定を所与とし、ユーザーおよび保守者が観測できる振る舞いと性質のみを定義する。

## Boundary Context

- **In scope**: 配布物の形と起動、ウィンドウのライフサイクルと複数ウィンドウの管理、メニューとキーボードショートカットの登録口、フロントエンドとドメインコアの通信境界および型の共有、補助プロセスの同梱・配置・起動・終了、3 プラットフォームのビルドと検証パイプライン、アプリケーション設定の永続化、ログとクラッシュ記録、個別画面が差し込まれるシェル構造（レイアウト・遷移・外観）、3 OS で描画が成立することの確認

- **Out of scope**: 個別画面の中身（グリッド、スキーマ編集、マクロエディタ、フォームビルダーなど）、ドキュメントの読み書きと検証、型の意味論、マクロの実行、変更履歴の管理、テンプレートへの書き出し、フォームの LAN 配信、自動更新と署名配布、認証とユーザー管理、権限管理と暗号化

- **Adjacent expectations**:
  - ドキュメントの読み書きを行う機能は、本機能が提供する通信境界を通じて呼び出される。本機能はドキュメントの内容を解釈しない
  - 補助プロセスを必要とする機能は、本機能が提供する同梱・配置・起動の仕組みを利用する。どの実行ファイルを同梱するかは利用側が決め、本機能は個々の補助プロセスの中身を知らない
  - ウィンドウを閉じてよいかの判断は、そのウィンドウのドキュメントを所有する機能が行う。本機能は問い合わせと、結果に従ってウィンドウを閉じる制御だけを担う
  - 外部から配信される画面でも動作する必要がある資産は、本機能の通信境界に依存しない。この制約が破られると、フォームを LAN 配信する機能が成立しなくなる
  - 本機能は 3 OS で描画が成立することを確認するための最小の画面を提供する。その画面の完成度を実用水準へ引き上げるのは、個別画面を所有する各機能である
  - 検証パイプラインは、`document-format` で既に構築された 3 OS の検証マトリクスと脆弱性検査ゲートを拡張する形で構成される。プラットフォーム別に独立した検証系統を新設しない

## Requirements

### Requirement 1: 単一実行ファイルとしての配布と起動
**Objective:** As a jxcel を受け取って使い始めるユーザー, I want 追加のインストール作業なしに 1 つのファイルを実行するだけでアプリケーションが立ち上がること, so that 環境構築の手間をかけずに使い始められる

#### Acceptance Criteria
1. The Application Shell shall Windows / macOS / Linux のそれぞれに対して、単一のファイルとして起動できる配布物を提供する。
2. When 配布物が、jxcel のための追加のランタイム・ライブラリ・フレームワークが導入されていない環境で実行されたとき、the Application Shell shall 追加のインストールを要求せずに起動する。
3. When 配布物が起動されたとき、the Application Shell shall SSD を搭載した 4 コア以上の一般的なデスクトップ環境において、操作可能なウィンドウが表示されるまでの時間を 2 秒以内に収める。
4. If 起動に必要な前提が満たされず起動を継続できないとき、then the Application Shell shall 満たされなかった前提を特定できるメッセージを提示し、無言で終了しない。
5. While アプリケーションが既に起動しているとき、when 配布物が再度実行されたとき、the Application Shell shall 2 つ目のアプリケーションを常駐させず、実行時の引数を既に動作しているアプリケーションへ引き継いだうえで新しく起動した側を終了させ、引き継いだ側が要求に対応するウィンドウを提示する。
6. The Application Shell shall 起動および通常の操作において、外部ネットワークへの通信を行わない。

### Requirement 2: ウィンドウのライフサイクルと複数ウィンドウ
**Objective:** As a 複数のデータファイルを並べて扱いたいユーザー, I want ファイルごとに独立したウィンドウを開けること, so that 別々のデータを見比べながら作業できる

#### Acceptance Criteria
1. The Application Shell shall 1 つのウィンドウに対して、ドキュメントを高々 1 つ関連付ける。
2. When ドキュメントを指定せずにアプリケーションが起動されたとき、the Application Shell shall ドキュメントを関連付けていないウィンドウを開き、新規作成と既存ファイルを開く操作を提示する。
3. When ユーザーが別のドキュメントを開く操作を行ったとき、the Application Shell shall 既存のウィンドウを閉じずに新しいウィンドウを開く。
4. When ユーザーがドキュメントを開く操作を行ったとき、the Application Shell shall OS 標準のファイル選択手段を提示し、選択された位置をドキュメントを所有する機能へ渡す。
5. While 複数のウィンドウが開いているとき、the Application Shell shall すべてのウィンドウを 1 つのアプリケーションプロセスの中で動作させる。
6. When ウィンドウが閉じられようとしたとき、the Application Shell shall そのウィンドウのドキュメントを所有する機能へ終了の可否を問い合わせ、拒否された場合はウィンドウを閉じない。
7. The Application Shell shall 直近に閉じられたウィンドウの位置とサイズを記憶し、次に開くウィンドウの初期値として用いる。
8. When 最後のウィンドウが閉じられたとき、the Application Shell shall アプリケーションを終了する。
9. Where プラットフォームが最後のウィンドウを閉じてもアプリケーションを常駐させる慣習を持つとき、the Application Shell shall アプリケーションを終了させずに常駐させる。
10. If ウィンドウの生成に失敗したとき、then the Application Shell shall 失敗を提示し、既に開いている他のウィンドウの動作を中断させない。

### Requirement 3: メニューとキーボードショートカット
**Objective:** As a キーボード中心で操作するユーザー, I want 主要な操作にメニュー項目とショートカットが割り当てられていること, so that マウスに持ち替えずに作業を続けられる

#### Acceptance Criteria
1. The Application Shell shall アプリケーション共通のメニュー構造を提供し、個別機能がメニュー項目を登録できる登録口を備える。
2. When 個別機能がメニュー項目を登録したとき、the Application Shell shall その項目を指定された位置に表示し、選択されたときに登録元へ通知する。
3. The Application Shell shall メニュー項目にキーボードショートカットを割り当てられるようにし、割り当てられたショートカットをメニュー上に表示する。
4. If 同一のキーボードショートカットが複数の項目に割り当てられたとき、then the Application Shell shall 登録の時点で競合を検出して登録元へ報告し、いずれか一方を無言で無効化しない。
5. While 複数のウィンドウが開いているとき、the Application Shell shall キーボードショートカットの操作を、操作対象となっているウィンドウに対してのみ適用する。
6. Where プラットフォームがアプリケーション共通のメニューバーを持つとき、the Application Shell shall そのプラットフォームの配置慣習に従ってメニューを提示する。

### Requirement 4: フロントエンドとドメインコアの通信境界
**Objective:** As a 後続スペックを実装する開発者, I want フロントエンドからドメイン機能を型の保証された単一の経路で呼べること, so that 各機能が独自の呼び出し口を作らずに済み、型の不一致が実行前に見つかる

#### Acceptance Criteria
1. The Application Shell shall フロントエンドからドメイン機能を呼び出すための単一の通信境界を提供する。
2. The Application Shell shall 通信境界を越えるすべての入力と出力の型を、フロントエンド側とドメイン側で共通のひとつの定義から得られるようにする。
3. If 通信境界の型定義がドメイン側の定義と一致しないとき、then the Application Shell shall 実行前に不一致を検出し、ビルドを失敗させる。
4. When ドメイン機能の呼び出しが失敗したとき、the Application Shell shall 失敗の原因を識別できる情報をフロントエンドへ返し、成功した場合と区別できるようにする。
5. The Application Shell shall 10 万行規模のデータを 1 回の呼び出しで受け渡せる経路を提供し、行ごとに通信境界を越えることを必要としない。
6. When 呼び出しが行われたとき、the Application Shell shall どのウィンドウからの呼び出しであるかを呼び出し先が識別できるようにする。
7. The Application Shell shall フロントエンドが任意のファイルを直接読み書きする経路、および任意のプロセスを起動する経路を提供しない。

### Requirement 5: 補助プロセスの同梱と起動
**Objective:** As a 単一のファイルを受け取ったユーザー, I want 別プロセスを必要とする機能が追加のインストールなしに動くこと, so that 高機能なエディタなどを最初から使える

#### Acceptance Criteria
1. The Application Shell shall 補助プロセスとして動作する実行ファイルを配布物に同梱し、実行時に利用可能な状態にする。
2. When 補助プロセスが初めて必要になったとき、the Application Shell shall 同梱された実行ファイルを実行可能な状態で利用可能にし、起動する。
3. If 配置された実行ファイルの内容が同梱時のものと一致しないとき、then the Application Shell shall 起動を試みる前に不一致を検出し、その事実を含むエラーとして報告する。
4. If 補助プロセスの起動が失敗したとき、then the Application Shell shall 失敗の原因を区別できる形で報告し、アプリケーション本体を終了させない。
5. While 複数のウィンドウが開いているとき、the Application Shell shall 同一種類の補助プロセスをウィンドウごとに重複して起動せず、ウィンドウ間で共有する。
6. When アプリケーションが終了するとき、the Application Shell shall 起動したすべての補助プロセスを終了させ、孤児プロセスを残さない。
7. If 補助プロセスが予期せず終了したとき、then the Application Shell shall その事実を利用側の機能へ通知し、アプリケーション本体を巻き込んで終了しない。
8. When 予期せず終了した補助プロセスが再び必要になったとき、the Application Shell shall 改めて起動を試みる。
9. The Application Shell shall 補助プロセスの出力を診断情報として取得できるようにする。

### Requirement 6: 3 プラットフォームのビルドと検証パイプライン
**Objective:** As a リリースを行う保守者, I want 3 つの OS 向けの配布物が自動で生成され、配布可能であることが機械的に確認されること, so that 手作業の確認漏れによって壊れた配布物を出さずに済む

#### Acceptance Criteria
1. The Build Pipeline shall Windows / macOS / Linux のそれぞれに対して配布物を生成する。
2. The Build Pipeline shall 既存の 3 OS 検証マトリクスを拡張する形で構成し、プラットフォーム別に独立した検証系統を新設しない。
3. The Build Pipeline shall 依存の脆弱性検査を継続して実行し、検査が失敗したときにパイプライン全体を失敗させる。
4. When 配布物が生成されたとき、the Build Pipeline shall 生成された配布物から補助プロセスの実行ファイルを取り出し、それが起動できることを検証する。
5. If バンドル処理を経た補助プロセスの実行ファイルが同梱前と異なる内容になったとき、then the Build Pipeline shall パイプラインを失敗させる。
6. When 配布物が生成されたとき、the Build Pipeline shall 各プラットフォームの配布物のサイズを出力に記録する。
7. The Build Pipeline shall 生成した 3 プラットフォーム分の配布物を、後から取得できる成果物として保存する。
8. The Build Pipeline shall 起動から操作可能なウィンドウが表示されるまでの時間を計測し、予算を超えたときにパイプラインを失敗させる。

### Requirement 7: アプリケーション設定の永続化
**Objective:** As a 継続的に jxcel を使うユーザー, I want 変更した設定が次回の起動時にも保たれること, so that 起動のたびに設定をやり直さずに済む

#### Acceptance Criteria
1. The Application Shell shall 名前付きの設定値を永続化し、再起動後に同じ値を返す。
2. The Application Shell shall 設定を各 OS が定める標準のアプリケーションデータ領域に保存する。
3. While 複数のウィンドウが開いているとき、the Application Shell shall すべてのウィンドウが同一の設定値を参照するようにする。
4. When あるウィンドウで設定が変更されたとき、the Application Shell shall 他のウィンドウにも変更後の値を反映する。
5. If 保存された設定が読み取れないとき、then the Application Shell shall 既定値で起動し、読み取れなかった事実を診断情報に記録し、起動を中止しない。
6. If 保存された設定に未知の項目が含まれるとき、then the Application Shell shall その項目を破棄せずに保持する。
7. The Application Shell shall 設定にドキュメントの内容を保存しない。

### Requirement 8: 診断情報とクラッシュ記録
**Objective:** As a 不具合に遭遇したユーザー, I want 何が起きたかを示す記録が手元に残ること, so that 原因を報告したり自分で確認したりできる

#### Acceptance Criteria
1. The Application Shell shall 実行中の出来事を各 OS が定める標準のアプリケーションデータ領域にファイルとして記録し、ユーザーがその保存場所を確認できるようにする。
2. When アプリケーションが異常終了したとき、the Application Shell shall 異常終了の記録を残す。
3. The Application Shell shall 記録および異常終了の記録を外部へ送信しない。
4. The Application Shell shall 記録にドキュメントのセル値およびスキーマの内容を出力しない。
5. While 記録が蓄積されるとき、the Application Shell shall 保持する記録の合計サイズを 50 MB 以下に保ち、超過した分を古いものから破棄する。
6. When ユーザーが診断情報の書き出しを要求したとき、the Application Shell shall 記録をひとつのファイルにまとめて出力する。
7. The Application Shell shall 記録の詳細度をユーザーが変更できるようにする。

### Requirement 9: フロントエンドのシェル構造
**Objective:** As a 個別画面を実装する開発者, I want 画面が差し込まれる場所と画面間の遷移が最初から決まっていること, so that 各機能が独自のレイアウトや遷移の仕組みを作らずに済む

#### Acceptance Criteria
1. The Application Shell shall 個別機能の画面が差し込まれる領域を定義し、画面がその領域に配置されるようにする。
2. The Application Shell shall 画面間の遷移を単一の仕組みで扱い、現在表示されている画面を識別できるようにする。
3. The Application Shell shall 明色と暗色の外観を提供し、既定では OS の外観設定に追随する。
4. When ユーザーが外観を明示的に選択したとき、the Application Shell shall OS の設定よりユーザーの選択を優先し、再起動後もその選択を維持する。
5. If 個別機能の画面の描画中にエラーが発生したとき、then the Application Shell shall アプリケーション全体を停止させず、該当する領域にエラーを提示する。
6. The Application Shell shall 外部から配信される画面でも動作する必要がある資産に、本機能の通信境界への依存を持ち込まない。

### Requirement 10: 3 OS における描画の成立
**Objective:** As a Windows / macOS / Linux のいずれかで jxcel を使うユーザー, I want どの OS でも画面が正しく描画されること, so that 環境によって使えないという状態に陥らない

#### Acceptance Criteria
1. The Application Shell shall Windows / macOS / Linux のいずれにおいても、起動後に操作対象となる画面要素を描画する。
2. If 起動後に画面の描画が成立しないとき、then the Application Shell shall 無内容の画面を提示したまま留まらず、描画が成立しなかったことを識別できる情報を提示する。
3. If 既知のプラットフォーム上の制約により描画が成立しない構成が検出されたとき、then the Application Shell shall 描画が成立する代替の経路で起動し、代替経路を用いた事実を診断情報に記録する。
4. The Application Shell shall 多数の要素を持つ表形式の描画と、文字編集を伴う描画のそれぞれについて、3 つの OS で成立することを確認できる最小の画面を提供する。
