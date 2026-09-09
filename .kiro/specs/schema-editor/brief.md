# Brief: schema-editor

## Problem
schema-engine がネスト可能な型システムを提供しても、それを定義する手段が JSON の手書きしかなければ実用に耐えない。一方で、スキーマ編集 UI をグリッドのスペックに混ぜると 1 スペックが「データの編集」と「構造の編集」という 2 つの責務を抱えて肥大する。

## Current State
schema-engine が型カタログ・ネスト・検証を提供する。定義する GUI が存在しない。

## Desired Outcome
GUI 上でシートのスキーマを定義・変更でき、ネストした構造を視覚的に組み立てられる。スキーマ変更が既存データに与える影響（どの行が不正になるか）が適用前にプレビューされ、移行方法を選んでから確定できる。

## Approach
ツリー状のスキーマエディタとして実装する。schema-engine のマイグレーション機構を UI から駆動し、「変更を適用したらどうなるか」をドライランで見せてから確定させる。組込型のみを対象とし、ユーザー定義型は custom-types 側が登録した型として自動的にリストに現れる構造にする。

## Scope
- **In**: スキーマのツリー編集 UI、ネスト構造の追加 / 削除 / 並び替え、型・制約・既定値の設定、シート間参照の設定、スキーマ変更の影響プレビューと移行の確定、スキーマの複製とテンプレート化
- **Out**: 型システムそのもの（schema-engine）、ユーザー定義型の作成 UI（custom-types）、データの編集（data-grid）

## Boundary Candidates
- スキーマ編集 UI と マイグレーション駆動ロジックの分離
- 組込型の設定パネルと 拡張型の設定パネルを同じインターフェースで扱えるようにする

## Out of Boundary
- 検証ルールの実装（schema-engine が所有）
- JS による型定義の記述（custom-types が所有）
- セル値の編集（data-grid が所有）

## Upstream / Downstream
- **Upstream**: app-shell, schema-engine
- **Downstream**: custom-types（登録された型がこのエディタに現れる）、form-builder（スキーマからフォームを導出する）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: data-grid（同じシート画面に同居する）、custom-types（型リストへの登録インターフェースを共有する）

## Constraints
- 破壊的なスキーマ変更（型の縮小、必須化）は必ず影響プレビューを経由すること。個人利用でもデータ損失は許容しない
- ネストの深さに実用上の上限を設け、UI が破綻しないようにする
