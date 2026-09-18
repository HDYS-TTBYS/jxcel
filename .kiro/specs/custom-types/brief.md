# Brief: custom-types

## Problem
「JS でデータ型を柔軟に定義可能」という要件は、組込型カタログだけでは満たせない。ユーザーが独自の型（郵便番号、通貨、社内コード体系など）を検証ルール・表示・入力方法とセットで定義できて初めて、このアプリは汎用のデータベースになる。

## Current State
schema-engine が組込型と拡張インターフェースを定義し、macro-runtime が JS 実行基盤を提供する。拡張点を実際に埋める実装が存在しない。

## Desired Outcome
ユーザーが JS/TS で型を定義すると、それがスキーマエディタの型リストに現れ、グリッドのセルエディタとして機能し、検証が働き、フォームの入力部品として使える。組込型とユーザー定義型がユーザーから見て区別なく扱える。

## Approach
schema-engine の拡張インターフェースを macro-runtime 上で実装する。型定義は「検証・型強制・表示・編集・シリアライズ」の関数群を持つオブジェクトとして書き、レジストリに登録する。10 万行の検証で JS を行ごとに呼ぶと性能が破綻するため、バッチ検証の経路を必ず用意する。

## Scope
- **In**: JS による型定義の記述形式、型レジストリと登録・解決、検証 / 型強制 / 表示 / 編集の各フック、ユーザー定義型のセルエディタ登録（data-grid のレジストリ経由）、型定義のドキュメントへの保存と可搬性、バッチ検証の高速経路、型定義のエラーハンドリングと縮退動作
- **Out**: 組込型（schema-engine）、ランタイム（macro-runtime）、スキーマ編集 UI そのもの（schema-editor）

## Boundary Candidates
- 型定義の記述形式（ユーザーが書く API）と レジストリ / 解決機構（内部）の分離
- 検証フックと 表示・編集フックを別インターフェースにし、UI を持たない環境（フォームサーバ、エクスポート）でも検証だけが動くようにする

## Out of Boundary
- 組込型カタログの実装（schema-engine が所有）
- グリッドのセルエディタ基盤そのもの（data-grid が所有。本スペックはそこに登録する側）
- 標準ライブラリ関数（macro-stdlib が所有）

## Upstream / Downstream
- **Upstream**: schema-engine, macro-runtime, data-grid（エディタレジストリ）
- **Downstream**: form-builder（ユーザー定義型の入力部品）、export-templates（書き出し時の表示変換）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: schema-engine（拡張インターフェースの所有権は schema-engine 側。本スペックは実装側）

## Constraints
- 10 万行の検証で JS を行ごとに呼び出さないこと。バッチ経路が必須
- 型定義が例外を投げてもアプリが落ちないこと。縮退動作を定義する
- 型定義はドキュメントに保存され、ファイルを別マシンで開いても機能すること

## Incoming Handovers（実装済みスペックからの申し送り。2026-09-18 の棚卸し）

- **一括メソッドは本番経路から呼ばれていることを示す**: `CustomType` トレイトの一括判定（`validate_batch`）の契約は trait の doc が正典。**呼び出し回数を数える観測**で「列ごとに 1 回」を固定する（速度は証拠にならない。出典: `.kiro/steering/structure.md:89`、`.kiro/specs/schema-engine/tasks.md:354`）
- **セルエディタの登録インターフェース**: 登録簿（`CellEditorRegistration` と `carrier: EditCarrier`）は `data-grid` が定義し本スペックが登録する。**確定の文字の運び手**を登録に足す設計改訂は `data-grid` 側で既に入っている（出典: `.kiro/specs/data-grid/design.md:73`）
- **組込型との対照テストが積み残し**: 同一列で既定実装と拡張実装を差し替えた 2 つの台帳の `SheetReport` を突き合わせる対照が無い（出典: `.kiro/specs/schema-engine/tasks.md:513`）
