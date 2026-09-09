# Brief: macro-stdlib

## Problem
「関数はマクロに統合する」設計では、Excel が組込関数として提供していたものをすべてライブラリ側で用意しなければならない。SUM や VLOOKUP に相当するものが何もない状態では、ユーザーは毎回ゼロから JS を書くことになり、スプレッドシートとしての体験が成立しない。

## Current State
macro-runtime が JS/TS の実行基盤とホスト API を提供する。その上に載る標準ライブラリが存在しない。

## Desired Outcome
集計・検索・日付処理・文字列処理・型変換・シート間結合といった日常的な操作が、標準ライブラリの呼び出しで完結する。ライブラリは TypeScript の型が付いており、エディタで補完が効く。ユーザーが自分のライブラリを追加する仕組みも同じ形で提供される。

## Approach
Excel 関数の単純な移植ではなく、JS のイディオムに沿った API として再設計する（配列メソッドとの合成が自然に書ける形）。同時に、Excel からの移行を助ける互換レイヤーを別モジュールとして用意する。実装は原則 TypeScript で書き、性能が問題になる部分だけ Rust のホスト関数に落とす。

## Scope
- **In**: 集計（合計・平均・件数・グループ化）、検索と参照（VLOOKUP 相当を含む）、日付 / 時刻、文字列、数値と丸め、型変換、シート間の結合とリレーション解決、統計、正規表現ヘルパ、Excel 互換関数レイヤー、ライブラリの型定義（.d.ts）配布、ユーザー製ライブラリの追加・管理の仕組み
- **Out**: ランタイムとホスト API（macro-runtime）、数式としての評価（formula-engine）、エクスポート処理（export-templates）

## Boundary Candidates
- 「JS イディオム版 API」と「Excel 互換レイヤー」を別モジュールに分離する
- TypeScript 実装部分と 性能上 Rust に落とすホスト関数部分の境界
- 標準ライブラリと ユーザー製ライブラリを同一の登録機構に乗せる

## Out of Boundary
- ランタイム・サンドボックス・実行モデル（macro-runtime が所有）
- 依存グラフと再計算（formula-engine が所有）
- 型定義としての利用（custom-types が所有）

## Upstream / Downstream
- **Upstream**: macro-runtime, schema-engine
- **Downstream**: formula-engine（数式から呼ばれる）、export-templates（書き出し処理から呼ばれる）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: macro-editor-lsp（ライブラリの .d.ts が補完の主要な入力になる）

## Constraints
- API は TypeScript の型が完全に付くこと。型が付かない API は補完体験を壊すため許容しない
- 10 万行の集計が実用時間で終わること。ホットパスは Rust 側に落とす判断基準を設計で示す
- スコープが際限なく膨らむ領域なので、requirements フェーズで「初版に含める関数」を明示的に線引きすること
