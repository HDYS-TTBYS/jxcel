# Brief: schema-engine

## Problem
「DB のようなデータ型を指定できる」「ANY もあり」「JSON のようにスキーマをネスト可能」という要件は、単なる表示上の書式設定ではなく、値を検証し・強制変換し・不正なデータを拒否する実行時エンジンを必要とする。これがないと本アプリは「型付きデータベース」ではなく単なる JSON エディタになる。

## Current State
document-format がドキュメント構造（File / Sheet / Schema / Row）を持つが、Schema はまだ入れ物にすぎず意味論を持たない。

## Desired Outcome
シートに対してネスト構造を持つスキーマを宣言でき、セルへの入力・マクロからの書き込み・フォームからの送信のすべてに対して、同一のルールで検証と型強制が働く。検証エラーは位置と理由を伴って返る。スキーマを変更したとき既存データがどうなるかが定義されている。

## Approach
JSON Schema の思想（ネスト・合成）と RDB の型カタログ（厳密な型・NOT NULL・既定値）を組み合わせた独自スキーマを定義する。スキーマ宣言（データ）と検証エンジン（実行）を分離し、宣言をコンパイル済みバリデータに落とすことで 10 万行の一括検証を実用速度にする。ユーザー定義型のための拡張点をここで切り、実装は custom-types に委ねる。

## Scope
- **In**: 組込型カタログ（数値 / 文字列 / 真偽 / 日時 / 列挙 / 参照 / ANY など）、ネスト（object / array）、null 許容・必須・既定値・一意制約、検証エンジンと検証エラーの表現、型強制ルール（文字列入力からの解釈を含む）、スキーマ変更時のデータ移行、シート間参照の整合性、ユーザー定義型のための拡張インターフェース定義
- **Out**: JS によるユーザー定義型の実装（custom-types）、スキーマ編集 UI（schema-editor）、数式、UI 全般

## Boundary Candidates
- スキーマ宣言（シリアライズ可能なデータ）と 検証エンジン（実行時の振る舞い）の分離
- 組込型と 拡張型の境界 — 拡張インターフェースをここで確定させ、custom-types が後から差し込めるようにする
- 検証（拒否するか否か）と 型強制（入力をどう解釈するか）を別レイヤーとして扱う

## Out of Boundary
- JS ランタイム上でのユーザー定義型の実行（custom-types が所有）
- スキーマを編集する GUI（schema-editor が所有）
- セルの表示書式・エディタ選択（data-grid が所有）
- 数式による計算列（formula-engine が所有）

## Upstream / Downstream
- **Upstream**: document-format
- **Downstream**: data-grid, schema-editor, custom-types, macro-runtime, macro-stdlib, export-templates, form-builder

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: document-format（スキーマの永続化表現は document-format が所有、意味論は本スペックが所有）

## Constraints
- Rust 実装。10 万行の全件検証が実用時間で終わること
- ANY 型が存在するため、型システムは「全てが厳密」を前提にできない。ANY を含む値のマクロ側での扱いを設計で明示する
- スキーマ自体もドキュメントの一部として git 差分に乗るため、宣言表現は人間可読かつ決定的であること
