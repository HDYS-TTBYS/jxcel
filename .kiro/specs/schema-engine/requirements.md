# Requirements Document

## Introduction
本機能は jxcel の型システムそのものを定義する。「DB のようなデータ型を指定できる」「ANY もあり」「JSON のようにスキーマをネスト可能」という要求は、表示上の書式設定ではなく、値を検証し・必要なら変換し・違反を位置と理由とともに報告する実行時のエンジンを必要とする。これがなければ本製品は「型付きデータベース」ではなく単なる JSON エディタになる。

ドキュメント形式はすでにシートごとのスキーマを保持する構造を持つが、その中身は構造として保たれているだけで意味を持たない。本機能はその中身に意味を与える。すなわち、どの列がどの型を持ち、どの値が適合し、適合しない値をどう扱い、スキーマを変えたとき既存データがどうなるかを決める。

本機能は意味論だけを所有し、画面・永続化・履歴・計算は所有しない。型の拡張インターフェースはここで定義し、その実装は別機能に委ねる。この分離は、拡張点が後付けで歪むのを防ぐための意図的な境界である。

## Boundary Context
- **In scope**: 組込型カタログ（整数 / 小数 / 10 進数 / 文字列 / 真偽 / 日時 / 列挙 / シート間参照 / 添付参照 / ANY）、入れ子（オブジェクトと配列）と名前付き型定義、値なし・既定値・一意・範囲などの制約、値の検証と違反の表現、型強制の規則、書き込み経路ごとの受け入れ方針、スキーマ変更の影響集計と既存データの移行、シート間参照の整合性判定、列の集合と並び順の決定、ユーザー定義型のための拡張インターフェースの定義
- **Out of scope**: スキーマを編集する画面、セルの表示書式とセルエディタの選択、数式と再計算、ユーザー定義型の実装そのもの、スキーマと行データのファイルへの格納と決定的な出力、変更履歴の記録と差分表示
- **Adjacent expectations**:
  - ドキュメント形式を扱う機能は、スキーマを構造として保持・検証するのみで中身を解釈しない。値が型に適合するかの判断は本機能が所有する。行データの列の並び順は本機能が供給し、ドキュメント形式を扱う機能はそれを受け取って出力する
  - 本機能は画面を持たない。違反は呼び出し元へ返し、ユーザーへの提示方法は呼び出し元が決める
  - フォームからの送信を拒否するのは、本機能が返す判定に基づいて配信側の機能が行う。本機能は判定を所有し、送信の成立可否の実行は所有しない
  - ユーザー定義型の実装は別機能が担う。本機能は拡張インターフェースとその上での振る舞いのみを所有する
  - 添付の実在と参照整合性はドキュメント形式を扱う機能が所有する。本機能は列が添付参照であることを宣言できるようにするのみで、添付の実在は判断しない

## Requirements

### Requirement 1: スキーマ宣言と列の構成
**Objective:** As a 型付きの表としてデータを育てたいユーザー, I want シートの列とその型を宣言でき、その宣言がファイルの中で読める形で残ること, so that データの意味が時間が経っても失われず、変更履歴にも意味のある形で現れる

#### Acceptance Criteria
1. The Schema Engine shall シートのルートスキーマから、そのシートの列名の集合と列の並び順を決定する。
2. The Schema Engine shall 決定した列の並び順を、行データを永続化する呼び出し元へ供給する。
3. The Schema Engine shall 各列について、名前・型・説明を宣言できるようにする。
4. The Schema Engine shall 同一内容のスキーマ宣言が常に同一のテキスト表現になるようにする。
5. If スキーマ宣言に同一の列名が複数現れるとき、then the Schema Engine shall 該当する列名を含むエラーとして宣言を拒否する。
6. If スキーマ宣言に空の列名が含まれるとき、then the Schema Engine shall 該当する位置を含むエラーとして宣言を拒否する。
7. If スキーマ宣言が解釈できない構造を含むとき、then the Schema Engine shall 該当箇所の位置と理由を含むエラーとして宣言を拒否する。
8. Where ルートスキーマが列を 1 つも宣言していないとき、the Schema Engine shall そのシートを列を持たないシートとして扱い、エラーとしない。

### Requirement 2: 組込型カタログ
**Objective:** As a Excel では型が守れないと感じているユーザー, I want データベースのような具体的な型を列に指定できること, so that 数値の列に文字列が紛れ込むような事故が起きない

#### Acceptance Criteria
1. The Schema Engine shall 整数・小数・10 進数・文字列・真偽・日時・列挙・シート間参照・添付参照・ANY を組込型として提供する。
2. The Schema Engine shall 各組込型について、その型が受け入れる値の範囲を一意に定める。
3. The Schema Engine shall 10 進数の型について、有効桁数と小数点以下の桁数を宣言できるようにする。
4. The Schema Engine shall 日時の型について、時刻を含むか否かとタイムゾーンの扱いを宣言できるようにする。
5. Where 列の型が ANY であるとき、the Schema Engine shall 保持可能なすべての値を適合として扱う。
6. Where 列の型が ANY であるとき、the Schema Engine shall その列に対する型強制を行わない。
7. When 値が列の型に適合するか判定されたとき、the Schema Engine shall 適合または違反のいずれか一方を結果として返す。

### Requirement 3: 入れ子のスキーマ
**Objective:** As a 表の形に収まらないデータも同じファイルに置きたいユーザー, I want 列の型として入れ子の構造を宣言でき、その内側にも型が効くこと, so that 入れ子のデータが型の保証の外側に落ちない

#### Acceptance Criteria
1. The Schema Engine shall 列の型として、名前付きフィールドの集合と、同一型の並びを宣言できるようにする。
2. The Schema Engine shall 入れ子の内側の各フィールドに対しても、組込型と制約を宣言できるようにする。
3. The Schema Engine shall 名前付きの型定義を宣言し、ルートスキーマおよび他の型定義から識別子で参照できるようにする。
4. When 入れ子の値に違反が見つかったとき、the Schema Engine shall 入れ子の内側の位置を特定できる形で違反を報告する。
5. If 型定義への参照が実在しない識別子を指しているとき、then the Schema Engine shall 参照元と参照先の識別子を含むエラーとして宣言を拒否する。
6. If 型定義の参照が循環しており、その型に適合する値が有限の大きさで存在しえないとき、then the Schema Engine shall 循環に含まれる型定義を含むエラーとして宣言を拒否する。

### Requirement 4: 制約
**Objective:** As a データベースとして運用するユーザー, I want 必須・既定値・一意といった制約を列に付けられること, so that 欠けや重複を後から探し回らずに済む

#### Acceptance Criteria
1. The Schema Engine shall 各列および入れ子の各フィールドについて、値なしを許すか否かを宣言できるようにする。
2. The Schema Engine shall 各列および入れ子の各フィールドについて、既定値を宣言できるようにする。
3. When 値が与えられていない列を含む行が追加されたとき、the Schema Engine shall その列の既定値を適用する。
4. If 値なしを許さない列に値なしが与えられたとき、then the Schema Engine shall 該当する行と列を含む違反として報告する。
5. The Schema Engine shall 数値の型について値の範囲を、文字列の型について長さと書式を、列挙の型について選択肢の集合を、それぞれ制約として宣言できるようにする。
6. The Schema Engine shall 列に一意制約を宣言できるようにする。
7. If 一意制約を持つ列に重複する値が存在するとき、then the Schema Engine shall 重複するすべての行の識別子を含む違反として報告する。
8. If 宣言された既定値がその列の型または制約に適合しないとき、then the Schema Engine shall 該当する列を含むエラーとしてスキーマ宣言を拒否する。

### Requirement 5: 違反の報告
**Objective:** As a 大量の行を扱うユーザー, I want 違反がどの行のどの列で、なぜ起きたのかが分かること, so that 不正なデータを一件ずつ特定して直せる

#### Acceptance Criteria
1. When 違反が報告されるとき、the Schema Engine shall シート・行の識別子・列名・入れ子の内側の位置・違反の理由を違反ごとに含める。
2. The Schema Engine shall 違反の理由に、期待した内容と実際の値の双方を含める。
3. When 1 つの行に複数の違反が存在するとき、the Schema Engine shall 最初の違反で打ち切らず、その行のすべての違反を報告する。
4. When シート全体が検証されたとき、the Schema Engine shall 違反を持つ行と持たない行を区別できる形で結果を返す。
5. The Schema Engine shall 同一の入力に対して常に同一の違反集合を同一の順序で報告する。
6. If 違反の件数が呼び出し元の指定した上限を超えるとき、then the Schema Engine shall 上限までの違反と違反の総件数を返す。
7. When ドキュメントが開かれたとき、the Schema Engine shall 全行の検証結果を呼び出し元が取得できる状態にする。

### Requirement 6: 書き込み経路ごとの受け入れ方針
**Objective:** As a 表計算のように自由に打ちたいが、集めたデータは型が正しくあってほしいユーザー, I want 編集中は不正な値も残せるが、外から集める経路では型が守られること, so that 編集の手が止まらず、かつ集まったデータは信用できる

#### Acceptance Criteria
1. When 編集経路から違反する値が与えられたとき、the Schema Engine shall その値を違反として報告したうえで、呼び出し元がその値を保持することを妨げない。
2. When 収集経路から違反する値が与えられたとき、the Schema Engine shall その値を受け入れられないものとして判定し、その判定を呼び出し元に返す。
3. The Schema Engine shall 画面上の入力とマクロからの書き込みを編集経路として扱い、フォームからの送信を収集経路として扱う。
4. While 違反する値がドキュメントに存在するとき、the Schema Engine shall その位置と理由をいつでも取得できるようにする。
5. When 違反する値を含むドキュメントが保存されるとき、the Schema Engine shall 保存を妨げない。
6. The Schema Engine shall 違反する値が保存と再読込を経ても入力されたとおりに保たれるようにする。

### Requirement 7: 型強制
**Objective:** As a セルに直接打ち込むユーザー, I want 打った内容が列の型として素直に解釈されること, so that 変換をいちいち書かなくても型付きのデータが貯まる

#### Acceptance Criteria
1. When 与えられた値が列の型と異なる形を持ち、その値が列の型の表記として一意に解釈できるとき、the Schema Engine shall その値を列の型へ変換する。
2. If 変換によって値の情報が失われるとき、then the Schema Engine shall 変換を行わず、違反として報告する。
3. If 値の解釈が地域設定または実行環境によって変わりうるとき、then the Schema Engine shall その解釈を行わず、違反として報告する。
4. When 変換が行われたとき、the Schema Engine shall 変換が起きたことと変換前の値を呼び出し元が確認できる形で返す。
5. The Schema Engine shall 変換の規則を、与えられた値の形と列の型の組み合わせごとに一意に定める。
6. The Schema Engine shall 同一の入力に対して常に同一の変換結果を返す。

### Requirement 8: スキーマの変更と既存データの移行
**Objective:** As a 使いながらスキーマを育てるユーザー, I want 変更を適用する前に既存データへの影響が分かること, so that 取り返しのつかない変換を知らずに走らせてしまうことがない

#### Acceptance Criteria
1. The Schema Engine shall 列の追加・削除・改名・型の変更・制約の変更をスキーマの変更として扱う。
2. When スキーマの変更が提示されたとき、the Schema Engine shall 適用した場合に変換される行数・違反になる行数・値が失われる行数を集計して返す。
3. The Schema Engine shall 集計結果に、値が失われる行の識別子と列名を含める。
4. While 変更が承認されていないとき、the Schema Engine shall 既存データを変更しない。
5. When 変更が承認されたとき、the Schema Engine shall 集計時に提示した内容と一致する結果になるように変更を適用する。
6. If 変更の適用が途中で失敗したとき、then the Schema Engine shall 適用前の状態を保ち、部分的に適用された状態を残さない。
7. When 列が改名されたとき、the Schema Engine shall その列の既存の値を新しい列名のもとに保持する。
8. When 列が追加されたとき、the Schema Engine shall 既存のすべての行に、その列の既定値を、既定値が宣言されていなければ値なしを与える。

### Requirement 9: シート間参照
**Objective:** As a 複数のシートに分けて台帳を作るユーザー, I want ある列が別シートの行を指していることを型として表せること, so that 手で転記した識別子が実在しなくなったことに気づける

#### Acceptance Criteria
1. The Schema Engine shall 列の型として、特定のシートの行を指す参照を宣言できるようにする。
2. When 参照を持つ値が検証されたとき、the Schema Engine shall 参照先の行が実在するかを判定する。
3. If 参照先の行が実在しないとき、then the Schema Engine shall 参照元の行と列、および参照先の識別子を含む違反として報告する。
4. When 参照されている行が削除されるとき、the Schema Engine shall 削除を妨げない。
5. When 参照先のシートが削除されたとき、the Schema Engine shall そのシートを指すすべての参照を違反として報告する。
6. The Schema Engine shall シート全体の参照の判定を、行ごとの個別の問い合わせを要さない一括の操作として提供する。

### Requirement 10: 一括検証の規模と応答
**Objective:** As a 10 万行を日常的に扱うユーザー, I want 全件の検証で待たされないこと, so that 型の検査があることで操作が重くならない

**予算の根拠:** ドキュメント形式の要件は「10 万行を 3 秒以内に開く」を定めている。開く操作の内側で全件検証が走るため、その予算を圧迫しない上限として 1 秒を置く。

#### Acceptance Criteria
1. When 10 万行かつ 30 列のシートが全件検証されたとき、the Schema Engine shall 1 秒以内に検証を完了する。
2. When 1 つのセルの値が検証されたとき、the Schema Engine shall 16 ミリ秒以内に結果を返す。
3. The Schema Engine shall 上記の所要時間を、SSD を搭載した 4 コア以上の一般的なデスクトップ環境における計測で満たす。
4. The Schema Engine shall シート全体の検証を、行ごとの個別の呼び出しを要さない一括の操作として提供する。
5. When スキーマの一部が変更されて再検証が必要になったとき、the Schema Engine shall 影響を受ける列だけを対象に再検証できるようにする。
6. The Schema Engine shall 1 シートにつき合計 10 万行を検証できることを保証する。

### Requirement 11: ユーザー定義型の拡張
**Objective:** As a 自分の業務の語彙で表を作りたいユーザー, I want 独自に定義した型を組込型と同じように列に指定できること, so that 郵便番号や商品コードのような自分の型が組込型と同じ扱いを受ける

#### Acceptance Criteria
1. The Schema Engine shall 組込型以外の型を登録できる拡張インターフェースを定義する。
2. Where 拡張インターフェースを通じて型が登録されているとき、the Schema Engine shall その型を列および入れ子のフィールドの型として指定できるようにする。
3. When 拡張された型の値が検証されたとき、the Schema Engine shall 組込型と同一の形式で違反を報告する。
4. If 登録された型が既存の型と同一の識別子を使うとき、then the Schema Engine shall 該当する識別子を含むエラーとして登録を拒否する。
5. If 拡張された型の検証が失敗または応答しないとき、then the Schema Engine shall その値を違反として報告し、シート全体の検証を中断しない。
6. The Schema Engine shall 拡張された型を含むシートの一括検証を、組込型のみの場合と同一の一括の経路で行う。
7. If 拡張された型を使うスキーマが読み込まれ、その型が登録されていないとき、then the Schema Engine shall 該当する列と型の識別子を含むエラーとして報告し、スキーマを破棄しない。
