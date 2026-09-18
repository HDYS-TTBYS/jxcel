# Brief: formula-engine

## Problem
「関数はマクロに統合する」を成立させるには、セルに書いた式がマクロランタイムで評価され、参照元が変われば自動的に再計算される必要がある。単にマクロを手動実行できるだけでは、スプレッドシートとしての体験にならない。同時に、任意の JS を式として許すと依存関係の静的な把握が難しくなるという固有の難問がある。

## Current State
macro-runtime が JS/TS を実行でき、macro-stdlib が関数群を提供し、data-grid がセルを表示・編集できる。セルと計算を結びつける層が存在しない。

## Desired Outcome
セルに式を書くと、マクロランタイム上で評価されて結果が表示される。参照しているセルや行が変わると、依存する式だけが再計算される。循環参照が検出され、エラーがセル上に表示される。10 万行の計算列が実用的な時間で再計算される。

## Approach
式の評価は macro-runtime に委ね、本スペックは「依存グラフの構築」と「再計算のスケジューリング」を所有する。依存関係は、ホスト API のアクセス記録（実行時に何を読んだかを追跡する）から動的に構築する — 任意の JS を静的解析するのは非現実的なため。列全体に同じ式が適用される「計算列」を基本単位とし、セル単位のばらばらな式は副次的に扱う。

## Scope
- **In**: 式の記述と保存（セル単位 / 計算列単位）、依存グラフの構築（実行時アクセス追跡）、再計算のスケジューリングと差分再計算、循環参照の検出、計算結果のキャッシュと無効化、エラー値の表現とグリッド上での表示、数式バー UI、再計算の中断とプログレス
- **Out**: JS の実行そのもの（macro-runtime）、関数の中身（macro-stdlib）、グリッド描画（data-grid）

## Boundary Candidates
- 依存グラフ（データ構造）と 再計算スケジューラ（実行制御）の分離
- 計算列（列単位・一括評価）と セル単位の式を別の評価経路として扱う
- 数式バー UI と 評価エンジンの分離

## Out of Boundary
- ランタイム・サンドボックス（macro-runtime が所有）
- 標準関数の実装（macro-stdlib が所有）
- セルの描画とエディタ（data-grid が所有）

## Upstream / Downstream
- **Upstream**: macro-runtime, macro-stdlib, data-grid
- **Downstream**: export-templates（計算結果を書き出す）、form-web-server（フォーム送信後の再計算）

## Existing Spec Touchpoints
- **Extends**: なし
- **Adjacent**: data-grid（undo スタックを共有する）、macro-runtime（実行基盤を共有する）

## Constraints
- 任意の JS を式として許すため、依存関係は静的解析ではなく実行時アクセス追跡で得る。この判断の限界（動的アクセスの取りこぼし）を設計で明示する
- 10 万行 × 計算列の再計算が実用時間で終わること。行ごとの JS 呼び出しを避ける一括評価経路が必須
- 再計算中も UI が固まらないこと

## Incoming Handovers（実装済みスペックからの申し送り。2026-09-18 の棚卸し）

- **取り消し履歴の所有者**: 履歴は `GridSession` ではなく**ウィンドウの保持（`SheetEntry`）**が持つ（`data-grid` の 10.2 が降ろした）。`EditCommand` / `UndoStack` の形を変えると `macro-runtime` のアダプタの写しが壊れる（出典: `.kiro/specs/data-grid/design.md:69`、`.kiro/specs/macro-runtime/design.md:61`）
- **外の経路から文書へ書くときの規律**: 再計算の結果は `DocumentSessionsApi::edit` の**閉包の内側**で適用し、取り消しの単位を `UndoLabel` の 1 対で積む（`macro-runtime` の 4.2 が同じ形。閉包の中でセッションを呼び返さない。出典: `.kiro/steering/structure.md` の「共有される継ぎ目」、`.kiro/specs/data-grid/design.md:71`）
- **変更の版**: 境界の `document_state` は `DocumentSummary.revision`（`u32`）を運ぶ（2026-09-18 に `document-session` が追加）。内容だけの変化の検出に使える（出典: `.kiro/specs/document-session/design.md:77`）
