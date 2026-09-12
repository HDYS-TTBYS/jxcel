//! 計画の適用（design.md「File Structure Plan」の `evolution/apply.rs`。tasks.md 7.3。
//! 要件 8.5, 8.6, 8.8）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは同じ層の [`impact`](super::impact) が組み立てた計画を
//! 受け取る側であり、`document-format` の文書モデルのほかは何も参照しない
//! （解釈・強制・判定のいずれも行わないため）。`api`（公開面。タスク 8.1）が本モジュールを
//! 参照する。
//!
//! # 適用の段に可謬な処理を残さない（design.md「Evolution Layer / SchemaEvolution」。要件 8.6）
//!
//! 可謬な処理（値の運搬・型強制・違反の判定）はすべて集計の段
//! （[`plan_change`](super::impact::plan_change)）で済んでいる。本モジュールが行うのは、
//! 計画が持つ新しい列名と全行分の新しい値の**書き戻し**だけである。書き戻しに使う上流の
//! 経路（`Document::set_sheet_columns` / `Document::set_row_values`）が失敗するのは未知の
//! シート・行のときだけであり、それは適用の前の照合が排除する。したがって「途中で失敗して
//! 半端に適用された状態」は構造上作れない。
//!
//! # 適用の前に照合する（要件 8.5）
//!
//! 集計と適用の間に文書が変われば、適用結果は提示（[`ChangeImpact`]）
//! と一致しない。計画は集計の時点のシートの**行識別子の並びと列名**のダイジェストを持ち、
//! 適用はそれを現在のシートと照合する（[`ChangePlan::is_stale`]）。食い違えば
//! [`StalePlan`] を返して**何も変更しない**。呼び出し元は集計からやり直す。
//!
//! # 列名と値を同時に書き戻す（要件 8.8）
//!
//! 列名は行データのキー順そのものである。値を書き戻すだけで列名を古いまま残すと
//! 「1 シート内の全行が同一のキー列」という上流の不変条件が壊れるため、両方を置き換える。
//! 追加された列の値は計画が計算済みである — 宣言された既定値、既定値が宣言されていなければ
//! 値なし（要件 8.8）。計画は列ごとの既定値を
//! [`CompiledSchema::default_value`](crate::compile::CompiledSchema::default_value) から
//! 取るため、行の初期値の供給（タスク 6.3 の `default_row`。要件 4.3）と同じ源である。
//!
//! # ルートスキーマは設置しない（タスク 7.2 の裁定）
//!
//! 計画はルート宣言しか受け取らず、名前付き型定義の集合を持たない。適用が
//! `SchemaPart` のエンベロープを組み立てると、型定義と、上流が保持する未知フィールドを
//! 破壊する（design.md「Out of Boundary」はエンベロープの所有を `document-format` に
//! 置いている）。新しい宣言を持つ呼び出し元が `Document::set_root_schema` で設置する。
//! 設置を忘れた場合は観測可能で回復可能である（次のコンパイルが古い宣言を使う）。

use document_format::{Document, SheetId};
use thiserror::Error;

use super::impact::{ChangeImpact, ChangePlan, PlanDigest};

/// 適用した変更（design.md「Public API Layer / SchemaEngineApi」の `apply_change` の戻り値。
/// 要件 8.5）。
///
/// 計画は適用に**消費される**ため、呼び出し元は適用の後で計画の中身を読めない。適用が
/// 何を書いたかを知る必要のある呼び出し元（列の並び順を永続化する経路。要件 1.2）のために、
/// 適用した列名・行数・集計を返す。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedChange {
    /// 変更したシート。
    sheet: SheetId,
    /// 適用した列名の配列（新しい宣言の並び順）。
    columns: Box<[String]>,
    /// 書き戻した行の数。
    rows: usize,
    /// 適用が実現した集計（提示したものそのものである。要件 8.5）。
    impact: ChangeImpact,
}

impl AppliedChange {
    /// 変更したシート。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 適用した列名の配列（新しい宣言の並び順）。行データを永続化する呼び出し元へ渡す値で
    /// ある（要件 1.2）。
    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    /// 書き戻した行の数。
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// 適用が実現した集計（要件 8.5）。
    ///
    /// 集計の段が提示した [`ChangeImpact`] と同じ内容である — 適用は計画の値を書き戻すだけ
    /// であり、集計はその値を数えたものであるから、両者は同じ 1 つの計画から出る。
    pub fn impact(&self) -> &ChangeImpact {
        &self.impact
    }
}

/// 計画が陳腐化していた（design.md「Error Handling」の 3 分類目。要件 8.5）。
///
/// 集計の時点と適用の時点でシートの状態（行識別子の並びと列名）が食い違っていることを表す。
/// [`SchemaError`](crate::error::SchemaError) とは**別の型**である — 宣言は壊れておらず、
/// 直すべきは計画の取り直しである（design.md「Error Categories and Responses」）。
///
/// 変種は診断に必要な文脈だけを保持し、表示用の文言を持たない
/// （[`SchemaError`](crate::error::SchemaError) と同じ規約。`Display` はログ・デバッグ用の
/// 最小限の技術的診断であり、提示は呼び出し元が `match` で組み立てる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("stale plan for sheet {sheet}")]
pub struct StalePlan {
    /// 計画の対象だったシート。
    sheet: SheetId,
    /// 集計の時点のシートの状態のダイジェスト。
    expected: PlanDigest,
    /// 適用の時点で観測したダイジェスト。シートが消えていた場合は `None`。
    observed: Option<PlanDigest>,
}

impl StalePlan {
    /// 計画の対象だったシート。
    pub fn sheet(&self) -> SheetId {
        self.sheet
    }

    /// 集計の時点のシートの状態のダイジェスト。
    pub fn expected(&self) -> PlanDigest {
        self.expected
    }

    /// 適用の時点で観測したダイジェスト。シートが消えていた場合は `None`。
    pub fn observed(&self) -> Option<PlanDigest> {
        self.observed
    }
}

/// 計画を適用する（design.md「Public API Layer / SchemaEngineApi」の Service Interface。
/// tasks.md 7.3。要件 8.5, 8.6, 8.8）。
///
/// 適用の前に計画のダイジェストを現在のシートと照合し、食い違えば [`StalePlan`] を返して
/// **何も変更しない**（要件 8.5）。一致すれば、計画が持つ新しい列名と全行分の新しい値を
/// 書き戻す（要件 8.8）。可謬な処理は集計の段に済んでいるため、この適用は失敗しない。
///
/// ルートスキーマは設置しない（モジュール docs「ルートスキーマは設置しない」）。
pub fn apply_change(doc: &mut Document, plan: ChangePlan) -> Result<AppliedChange, StalePlan> {
    let sheet = plan.sheet();
    if plan.is_stale(doc) {
        return Err(StalePlan {
            sheet,
            expected: plan.digest(),
            observed: doc.sheet_by_id(sheet).map(PlanDigest::of),
        });
    }

    // ここから先の書き戻しは失敗しない: 照合がシートの存在と、行識別子の並び・列名の一致を
    // 確認済みである（design.md「Evolution Layer / SchemaEvolution」の原子性の作り方）。
    let parts = plan.into_parts();
    doc.set_sheet_columns(sheet, parts.columns.to_vec())
        .expect("照合でシートの存在を確認済み");
    let applied_rows = parts.rows.len();
    for (row, values) in parts.rows {
        doc.set_row_values(sheet, row, values)
            .expect("照合で行識別子の並びの一致を確認済み");
    }

    Ok(AppliedChange {
        sheet,
        columns: parts.columns,
        rows: applied_rows,
        impact: parts.impact,
    })
}
