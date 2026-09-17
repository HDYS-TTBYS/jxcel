# Requirements Document

## Introduction

本機能は、「計算はマクロに統合する」という本製品の中核判断を支える土台を所有する。閉じた式言語を持たないという判断は、数式・標準マクロライブラリ・ユーザー定義型・テンプレートへの書き出しのすべてが**同じ 1 つの実行基盤**の上に乗ることを意味する。したがって本機能は、後続の 4 つの機能が現れる前に「利用者が書いた TypeScript / JavaScript が、アプリの中でドキュメントを型付きで読み書きしながら走る」状態を作る。

現状: ドキュメント形式が値を保持し、型システムが値の正否を決め、画面が人が編集する手段を提供している。しかし**プログラムからドキュメントに触る手段が存在しない** — 集計も、繰り返しの加工も、定型の書き出しも、人が手で打つしかない。

本機能は、マクロを**ドキュメントの一部として保存**し、実行の入口（メニューから選んで走らせ、戻り値と出力を 1 つの面で見る）を提供し、**宣言された能力の範囲でだけ**ファイルとネットワークに触らせる。実行は隔離され、既定の時間とメモリの上限で打ち切られ、**アプリは死なない**。マクロが加えた変更は画面の編集と同じ経路を通り、**実行 1 回を 1 回の取り消しで戻せる**。

本機能は振る舞いだけを定める。実行基盤の構成、型定義の生成方法、ドキュメント内での表現は design が決める。サンドボックスは「敵対的なコードに対する硬い境界」ではなく**事故防止**の水準であり、信頼できないマクロを安全に走らせる保証は本機能の目標ではない（`product.md`「Explicitly Not」）。

## Boundary Context

- **In scope**: マクロのドキュメントへの保存と一覧（名前・種別・ソース）、実行の入口と結果の提示（戻り値・出力・失敗）、TypeScript と JavaScript の実行、型付きのホスト API（シート・列の宣言・行・セルの読みと書き）、変更の適用（画面と同じ経路。違反は保持する）と 1 回の実行の取り消し、実行の隔離と時間・メモリの上限による打ち切り、能力（ファイル・ネットワーク）の宣言と提示と拒否、失敗の提示（理由とソース上の位置）、マクロへ公開するホスト API の型定義の公開、10 万行規模の一括処理の性能

- **Out of scope**: 標準マクロライブラリの実装（`macro-stdlib` が所有）、マクロのエディタ・補完・診断（`macro-editor-lsp` が所有）、セルへの数式の入力と依存グラフによる再計算（`formula-engine` が所有）、ユーザー定義型への応用（`custom-types` が所有）、テンプレートへの書き出し（`export-templates` が所有）、スキーマの宣言・変更（`schema-editor` が所有）、敵対的なコードに対する強固なセキュリティ境界（個人利用を前提とした明示的な非目標）、マクロを書くための編集面そのもの（本機能は**保存**と**実行**を持ち、編集面は持たない）

- **Adjacent expectations**:
  - 値が型に適合するかの判断と型強制の規則は型システムを所有する機能が決める。本機能は判定を求めて結果を提示するだけであり、**独自の判定規則を持たない**
  - ドキュメントへの変更の適用経路はセッションを所有する機能が持つ。本機能はその経路を通して変更する（違反する値も破棄せず保持される）
  - マクロが加えた変更の取り消しは、画面の取り消しと同じ履歴に現れる。履歴の保持と上限の規則は画面側の保持が持つ
  - マクロをドキュメントの一部として持つための**表現**はドキュメント形式を所有する機能が決める。本機能が要求するのは「保存して開き直すと同じ名前と同じソースで現れる」という振る舞いだけである
  - マクロの編集面・補完・診断はエディタを所有する機能が提供する。本機能は実行の入口と、補完の入力になる**型定義**を提供する
  - 標準マクロライブラリ・数式・ユーザー定義型・スクリプト駆動の書き出しは、本機能の実行基盤とホスト API を**同じものとして**使う（本機能はそれらの利用者を知らない）

## Requirements

### Requirement 1: マクロをドキュメントに保存する
**Objective:** As a 手元のファイル 1 つで仕事を完結させたいユーザー, I want マクロがドキュメントと一緒に保存され、渡した相手のところでも同じ状態で開けること, so that 手順とデータが離ればなれにならない

#### Acceptance Criteria
1. When ユーザーがマクロをドキュメントへ保存したとき、the Macro Runtime shall そのマクロをそのドキュメントの一部として保持する。
2. When そのドキュメントを保存して開き直したとき、the Macro Runtime shall 保存したマクロを同じ名前と**同じソース**で提示する。
3. When ドキュメントを開いたとき、the Macro Runtime shall そのドキュメントが持つマクロの一覧（名前と種別）を、**実行せずに**提示する。
4. If ドキュメントが持つマクロのソースがその種別として解釈できないとき、then the Macro Runtime shall そのマクロを一覧に残したまま解釈できなかった理由を提示し、**ドキュメントは開ける**。
5. The Macro Runtime shall マクロのソースを保存と読み込みの間で変えない（整形し直さない）。
6. When ユーザーが既にある名前でマクロを保存したとき、the Macro Runtime shall 同じ名前のマクロを置き換える。
7. When ユーザーがマクロの削除を指示したとき、the Macro Runtime shall そのマクロをドキュメントから取り除く。

### Requirement 2: マクロを実行する（入口と結果）
**Objective:** As a 手で打つ代わりに計算や加工を任せたいユーザー, I want 今開いているドキュメントに対してマクロを走らせ、結果と影響をその場で見られること, so that 作業の流れを止めずに自動化できる

#### Acceptance Criteria
1. When ユーザーがメニューからマクロの実行を指示したとき、the Macro Runtime shall 実行できるマクロの一覧を提示し、そのウィンドウが開いているドキュメントに対して選ばれたマクロを実行する。
2. While マクロが実行されているとき、the Macro Runtime shall 表の表示と操作を止めない（ウィンドウは応答し続ける）。
3. When マクロが終わったとき、the Macro Runtime shall 戻り値と、マクロが出した出力を 1 つの面に提示する。
4. If マクロが実行の途中で失敗したとき、then the Macro Runtime shall 失敗として終わったことを同じ面に提示する（提示する内容は要件 9 が定める）。
5. When マクロがドキュメントを変更したとき、the Macro Runtime shall 変更が起きたことを提示する。
6. When マクロの実行が終わったとき、the Macro Runtime shall 成否・出力・変更の有無を診断の記録に 1 件残す（利用者が書き出して確かめられる）。
7. While 実行できるマクロが 1 つも無いとき、the Macro Runtime shall 実行の指示を提示しない（押しても何も起きない状態を作らない）。

### Requirement 3: TypeScript と JavaScript を書ける
**Objective:** As a 手順を書き残すユーザー, I want 型の付いた TypeScript で書けること, so that 補完と型の助けを得ながら書ける

#### Acceptance Criteria
1. When ユーザーが TypeScript として保存されたマクロを実行したとき、the Macro Runtime shall 型の注釈を取り除いて実行する。
2. If TypeScript のマクロに型の注釈の誤りがあるとき、then the Macro Runtime shall 実行を止めない（型の検査はエディタを所有する機能の役割である）。
3. When ユーザーが JavaScript として保存されたマクロを実行したとき、the Macro Runtime shall そのまま実行する。
4. If マクロのソースに構文の誤りがあるとき、then the Macro Runtime shall その位置（行と列）と理由を提示する。
5. If マクロが解決できない名前を取り込もうとしたとき、then the Macro Runtime shall 解決できなかった名前を提示して実行を止める。

### Requirement 4: 型付きのホスト API でドキュメントを読む
**Objective:** As a データを加工するマクロを書くユーザー, I want ドキュメントの中身を型ごと読めること, so that 列の意味を間違えないコードが書ける

#### Acceptance Criteria
1. The Macro Runtime shall 呼び出し元のウィンドウのドキュメントについて、シートの一覧・列の宣言・行数をマクロから読めるようにする。
2. The Macro Runtime shall 行の値を、その列に宣言された型に対応する JavaScript の値として渡す。
3. When 列の値が入れ子または ANY であるとき、the Macro Runtime shall その構造を JavaScript の値として提示する。
4. When マクロが行の範囲の読みを要求したとき、the Macro Runtime shall その範囲の値を 1 回の呼び出しで返す（利用者に行ごとの呼び出しを強いない）。
5. When マクロが列の宣言を書き換えようとしたとき、the Macro Runtime shall その変更を拒み、理由を提示する（スキーマは読み取り専用である）。
6. The Macro Runtime shall マクロから読める値と、書き込む値の型の対応を、公開する型定義として示す。

### Requirement 5: 変更は画面と同じ経路で適用する
**Objective:** As a マクロでデータを直すユーザー, I want マクロの書き込みが手で打ったときと同じ扱いになること, so that 型の保証と違反の提示がマクロ経由でも同じように効く

#### Acceptance Criteria
1. When マクロがセルの値を書き換えたとき、the Macro Runtime shall その変更を画面の編集と同じ経路でドキュメントへ適用する。
2. If 書き込まれた値が列の型に適合しないとき、then the Macro Runtime shall その値を破棄せずに保持したうえで、画面と同じ形で違反として提示できる状態にする。
3. The Macro Runtime shall 値の書き込みに加えて、行の追加・削除・複製を行えるようにする。
4. If マクロが存在しない行または列を指したとき、then the Macro Runtime shall 変更を適用せず、理由とマクロのソースの位置を提示する。
5. When マクロが書き込みを行ったとき、the Macro Runtime shall 書き込みの合計を提示する（何件を変更したかが分かる）。

### Requirement 6: 実行を打ち切る（時間とメモリ）
**Objective:** As a 暴走したマクロで作業を失いたくないユーザー, I want 止まらないマクロがアプリを巻き込まないこと, so that 安心して試せる

#### Acceptance Criteria
1. If マクロの実行が既定の時間の上限（30 秒）を超えたとき、then the Macro Runtime shall その実行を打ち切り、時間の上限による打ち切りであることを提示する。
2. If マクロの使用メモリが上限（既定 512 MB）を超えたとき、then the Macro Runtime shall その実行を打ち切り、メモリの上限による打ち切りであることを提示する。
3. When 実行が打ち切られたとき、the Macro Runtime shall ドキュメントを実行前の状態のまま残す。
4. If マクロが終わらない繰り返しに入ったとき、then the Macro Runtime shall アプリを終了させず、打ち切りの後も画面を操作できる状態に戻す。
5. The Macro Runtime shall 時間の上限（既定 30 秒）とメモリの上限（既定 512 MB）を設定で変更できるようにし、変更を次の実行から適用する。

### Requirement 7: 実行 1 回の変更を 1 回の取り消しで戻す
**Objective:** As a マクロの結果を試しているユーザー, I want 実行まるごとを 1 回で戻せること, so that 失敗した試行を後始末せずにやり直せる

#### Acceptance Criteria
1. When マクロがドキュメントを変更したとき、the Macro Runtime shall その実行で加えた変更の全体を、画面の取り消しの入口から 1 回で戻せるようにする。
2. When ユーザーがその取り消しを行ったとき、the Macro Runtime shall 実行前の行と値へ戻し、やり直しで同じ変更をもう一度適用できるようにする。
3. If マクロが失敗または打ち切りで終わったとき、then the Macro Runtime shall ドキュメントを変更しない（取り消すものが生じない）。
4. The Macro Runtime shall 取り消しの保持の上限を、画面の取り消しと同じ規則で扱う。

### Requirement 8: 能力の宣言と拒否（ファイルとネットワーク）
**Objective:** As a 見知らぬマクロを走らせるユーザー, I want 何に触るマクロなのかが分かること, so that 意図しない読み書きを起こさない

#### Acceptance Criteria
1. The Macro Runtime shall マクロに、**宣言されていない**ファイルとネットワークの利用を許さない。
2. When ユーザーがマクロの実行を指示したとき、the Macro Runtime shall そのマクロが宣言している能力を提示する。
3. If マクロが宣言していない能力を使おうとしたとき、then the Macro Runtime shall その利用を拒み、拒んだ能力の名前を提示して実行を失敗として終わらせる。
4. The Macro Runtime shall ファイルの読み込み・ファイルの書き込み・ネットワークの利用を、それぞれ別の能力として扱う。
5. When ユーザーがマクロの宣言を変えたとき、the Macro Runtime shall 次の実行から新しい宣言を適用する。

### Requirement 9: 失敗の提示（理由と位置）
**Objective:** As a マクロを直すユーザー, I want どこで何が起きたかが分かること, so that 手当てができる

#### Acceptance Criteria
1. When マクロが例外で終わったとき、the Macro Runtime shall 例外の理由と、マクロのソースの行と列を提示する。
2. When 失敗がホスト API の呼び出しで起きたとき、the Macro Runtime shall 失敗した API の名前と理由を提示する。
3. The Macro Runtime shall 例外に至るまでの呼び出しの並びを提示する（マクロのソースの位置を含む）。
4. If マクロが失敗しても、then the Macro Runtime shall 保存されているマクロのソースを失わない。

### Requirement 10: ホスト API の型定義を公開する
**Objective:** As a 補完の効くエディタでマクロを書きたいユーザー, I want 公開されている API に型が付いていること, so that 名前を覚えずに書ける

#### Acceptance Criteria
1. The Macro Runtime shall マクロへ公開するすべてのホスト API に型を付ける（型の無い API を公開しない）。
2. The Macro Runtime shall 公開する型定義を、後続の機能（エディタと標準マクロライブラリ）がそのまま取り込める形で提供する。
3. When ホスト API が変わったとき、the Macro Runtime shall 公開する型定義を同じ変更の中で追随させ、乖離を機械的に検出できるようにする。

### Requirement 11: 一括処理の性能
**Objective:** As a 10 万行を相手にするユーザー, I want マクロが実用の時間で終わること, so that 手作業より速いという利点が保たれる

#### Acceptance Criteria
1. When マクロが 10 万行 × 30 列のシートの全行を読み、その全部を使う集計を行ったとき、the Macro Runtime shall 10 秒以内に終わる。
2. When マクロが 1 万行を書き換えたとき、the Macro Runtime shall 5 秒以内に終わる（画面から 1 万行を貼り付ける予算と同じ桁に収める）。
3. The Macro Runtime shall 実行の所要を計測できる形にし、上記の予算を実起動の観測で判定できるようにする。
