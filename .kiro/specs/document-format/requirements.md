# Requirements Document

## Introduction
本機能は jxcel のドキュメント形式そのものを定義する。型付きの表形式データを「持ち運べる 1 ファイル」として扱いたい個人・パワーユーザーに対し、ZIP 圧縮された JSON テキストを実体とするドキュメント形式と、それをメモリ上のドキュメントモデルへ読み書きする手段を提供する。

既存の選択肢はいずれもこの用途を満たしていない。xlsx はバイナリであるため変更履歴から「いつ・どのセルが変わったか」を追跡できず、素の JSON には構造・型・シートという概念の規約がない。本機能は、ドキュメントを「ファイル → シート → データスキーマ → 行」という明確な階層として定義し、同じ内容が常に同じバイト列になる決定的な出力を保証することで、テキスト差分が意味を持つファイル形式を成立させる。

本機能は jxcel の最初期の 2 スペックのひとつであり、型システム・マクロランタイム・バージョン管理・エクスポート・フォーム配信のほぼすべてがこの上に乗る。したがって本機能は構造と永続化のみを所有し、値の意味論・履歴管理・UI は所有しない。

## Boundary Context
- **In scope**: ZIP コンテナ仕様とエントリ構成、ドキュメントモデル（ファイル / シート / スキーマ / 行 / 添付の階層と識別子）、シリアライズとデシリアライズ、決定的出力の保証、構造整合性の検証、破損検出と安全な失敗、フォーマットバージョニングと変換、添付ファイルの格納と参照整合性
- **Out of scope**: 値の型検証・型強制・既定値といった型の意味論、変更履歴の記録とコミット・差分表示、画面表示と操作、10 万行を超える規模のための遅延ロード・ページング・インデックス、暗号化とアクセス制御
- **Adjacent expectations**:
  - 変更履歴を扱う機能は、本機能が保証する決定的出力に依存して差分の品質を得る。本機能は差分の計算・表示・保存を行わない
  - 型システムを扱う機能は、本機能が提供するスキーマ格納構造の上に型の意味論を載せる。本機能はスキーマを構造として保持・検証するのみで、値が型に適合するかは判断しない
  - 本機能は画面を持たない。検出したエラーは呼び出し元に返し、ユーザーへの提示方法は呼び出し元が決める

## Requirements

### Requirement 1: ドキュメント構造モデル
**Objective:** As a データを 1 ファイルで管理するユーザー, I want ファイル・シート・スキーマ・行の階層が一意に定まっていること, so that データの位置づけが曖昧にならず、後続の機能が同じ構造を前提にできる

#### Acceptance Criteria
1. The Document Format Module shall 1 つのドキュメントが 0 個以上のシートを保持できる構造を提供する。
2. The Document Format Module shall 各シートに対してルートデータスキーマを厳密に 1 つ関連付ける。
3. The Document Format Module shall 各シートが 0 個以上の名前付きネスト型定義を保持し、ルートスキーマおよび他のネスト型定義から識別子で参照できる構造を提供する。
4. The Document Format Module shall ドキュメント内の各シート、各行、各ネスト型定義に対して、ドキュメント内で一意な識別子を付与する。
5. When 行の並び順が変更されて保存されたとき、the Document Format Module shall 各行の識別子を変更せずに保持する。
6. When シートの名前が変更されて保存されたとき、the Document Format Module shall そのシートの識別子を変更せずに保持する。
7. If ネスト型定義への参照が実在しない識別子を指しているとき、then the Document Format Module shall 参照元と参照先の識別子を含むエラーとして報告する。

### Requirement 2: ZIP コンテナとエントリ構成
**Objective:** As a ファイルの中身を自分で確認したいユーザー, I want ドキュメントが標準的な ZIP として展開でき、中身が用途ごとに分かれていること, so that 専用ツールなしでも内容を確認でき、変更が意味のある単位で現れる

#### Acceptance Criteria
1. The Document Format Module shall ドキュメントを標準的な ZIP アーカイブとして書き出し、一般的な ZIP 展開ツールで展開可能にする。
2. The Document Format Module shall ドキュメントメタデータ、スキーマ定義、シートデータをそれぞれ別のエントリとして格納する。
3. The Document Format Module shall シートデータをシートごとに独立したエントリとして格納する。
4. The Document Format Module shall すべての JSON エントリを UTF-8 のテキストとして格納する。
5. If 読み込んだコンテナのエントリパスがコンテナのルート外を指しているとき、then the Document Format Module shall そのエントリを展開せず、該当エントリ名を含むエラーとして読み込みを中止する。
6. If 読み込んだコンテナが同一パスのエントリを複数含むとき、then the Document Format Module shall 該当パスを含むエラーとして読み込みを中止する。

### Requirement 3: 決定的シリアライズ
**Objective:** As a 変更履歴を追跡したいユーザー, I want 同じ内容が常に同じバイト列になり、変更した箇所だけが差分に現れること, so that ファイルの変更履歴を行単位で意味のある形で追える

#### Acceptance Criteria
1. When 同一内容のドキュメントが 2 回保存されたとき、the Document Format Module shall バイト単位で同一のファイルを出力する。
2. When 同一内容のドキュメントが異なる OS 上で保存されたとき、the Document Format Module shall バイト単位で同一のファイルを出力する。
3. The Document Format Module shall JSON オブジェクトのキーを常に同一の規則で整列して出力する。
4. The Document Format Module shall 1 行のデータが出力テキスト上で独立した 1 行として現れるように整形する。
5. When ドキュメント内の 1 行のみが変更されて保存されたとき、the Document Format Module shall その行に対応するテキスト行のみが変化した出力を生成する。
6. The Document Format Module shall 保存時刻、実行環境、および内部処理順序に依存する値を出力に含めない。

### Requirement 4: 読み込みと構造整合性の検証
**Objective:** As a データを預けるユーザー, I want 開いた時点で構造的な破綻が検出されること, so that 整合性を失ったデータに気づかないまま編集を続けることがない

#### Acceptance Criteria
1. When ドキュメントファイルが開かれたとき、the Document Format Module shall メモリ上にドキュメントモデルを構築し、シート・スキーマ・行・添付にアクセス可能な状態にする。
2. When ドキュメントが読み込まれたとき、the Document Format Module shall シート、行、ネスト型定義、添付のすべての識別子参照が実在する対象を指していることを検証する。
3. If 同一種別の識別子が重複しているとき、then the Document Format Module shall 重複した識別子とその出現箇所を含むエラーとして報告し、読み込みを中止する。
4. If シートデータのエントリに対応するスキーマ定義が存在しないとき、then the Document Format Module shall 該当シートの識別子を含むエラーとして報告し、読み込みを中止する。
5. If ドキュメントメタデータのエントリが存在しないとき、then the Document Format Module shall 不足しているエントリ名を含むエラーとして報告し、読み込みを中止する。

### Requirement 5: 破損検出と安全な失敗
**Objective:** As a 重要なデータを預けるユーザー, I want 破損したファイルが黙って部分的に読み込まれたり自動的に書き換えられたりしないこと, so that データ損失や不整合に気づかないまま作業を続けることがない

#### Acceptance Criteria
1. When ドキュメントが保存されたとき、the Document Format Module shall 各エントリの内容から算出した完全性検証値をドキュメント内に記録する。
2. When ドキュメントが読み込まれたとき、the Document Format Module shall 記録された完全性検証値と実際のエントリ内容を照合する。
3. If 完全性検証値が実際の内容と一致しないとき、then the Document Format Module shall 不一致のエントリ名を含むエラーとして報告し、読み込みを中止する。
4. If ドキュメントの読み込みが失敗したとき、then the Document Format Module shall 部分的に構築されたドキュメントモデルを利用可能な結果として返さない。
5. The Document Format Module shall 破損を検出したドキュメントを自動的に修復または上書きしない。
6. If 保存処理が完了前に中断されたとき、then the Document Format Module shall 保存対象のファイルを保存前の内容のまま残す。

### Requirement 6: フォーマットバージョニングと変換
**Objective:** As a 長期間データを保持するユーザー, I want 形式が変わっても過去に作成したファイルが開けること, so that アプリケーションの更新によって既存のデータが読めなくなることがない

#### Acceptance Criteria
1. The Document Format Module shall ドキュメントメタデータにフォーマットバージョンを記録する。
2. When 現行より古いフォーマットバージョンのドキュメントが開かれたとき、the Document Format Module shall 現行バージョンの構造へ変換したうえでドキュメントモデルを構築する。
3. When 複数バージョン前のフォーマットのドキュメントが開かれたとき、the Document Format Module shall 現行バージョンへ変換したうえでドキュメントモデルを構築する。
4. When 変換されたドキュメントが初めて保存されるとき、the Document Format Module shall 変換前のファイルを退避として保持する。
5. If 現行より新しいフォーマットバージョンのドキュメントが開かれたとき、then the Document Format Module shall 読み込みを中止し、そのファイルが要求するフォーマットバージョンを含むエラーとして報告する。

### Requirement 7: 添付ファイル
**Objective:** As a データに関連する画像や資料も一緒に持ち運びたいユーザー, I want JSON 以外のファイルを同じドキュメントに同封できること, so that 関連ファイルがドキュメントから散逸しない

#### Acceptance Criteria
1. The Document Format Module shall JSON 以外の任意のバイト列を添付エントリとしてドキュメントに格納できる。
2. The Document Format Module shall 各添付エントリに対して、ドキュメント内で一意な識別子を付与する。
3. When 添付エントリが格納されたとき、the Document Format Module shall その識別子をシートデータから参照できるようにする。
4. If シートデータが実在しない添付識別子を参照しているとき、then the Document Format Module shall 参照元と該当識別子を含むエラーとして報告する。
5. The Document Format Module shall 添付エントリの内容を解釈、変換、または再圧縮しない。
6. If いずれのシートデータからも参照されていない添付エントリが存在するとき、then the Document Format Module shall その添付を自動的に削除せず、未参照の添付として呼び出し元が一覧できるようにする。

### Requirement 8: 性能と規模
**Objective:** As a 10 万行規模のデータを日常的に扱うユーザー, I want 開く操作と保存操作で待たされないこと, so that このファイル形式をデータベースとして日常的に運用できる

#### Acceptance Criteria
1. When 10 万行を含むドキュメントが開かれたとき、the Document Format Module shall 3 秒以内にドキュメントモデルの構築を完了する。
2. When 10 万行を含むドキュメントが保存されたとき、the Document Format Module shall 2 秒以内に書き出しを完了する。
3. The Document Format Module shall 上記の所要時間を、SSD を搭載した 4 コア以上の一般的なデスクトップ環境における計測で満たす。
4. The Document Format Module shall 1 つのドキュメントにつき合計 10 万行を保持できることを保証する。
5. If ドキュメントが 10 万行を超えるとき、then the Document Format Module shall 読み込みを拒否せず、性能保証の対象外である旨を呼び出し元に通知する。
