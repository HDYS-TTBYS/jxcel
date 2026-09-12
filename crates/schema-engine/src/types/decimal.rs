//! 10 進数の文字列上の桁検査と正準化（design.md「コンポーネントとファイルの対応」の
//! `DecimalDigits`。tasks.md 2.2 が実装する。要件 2.3）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api`。本モジュールは [`super`]（`TypeCatalog`）の下に置かれ、
//! 10 進数の**桁勘定と比較のための正準形**だけを所有する。
//!
//! # なぜ 10 進数のライブラリを入れないか（research.md の裁定）
//!
//! `rust_decimal` / `bigdecimal` / `fastnum` はいずれも出力時に何かを正規化する（先頭の 0、
//! `+`、指数形）。`document-format` の `Decimal` は**文字列のまま逐語で往復する**契約であり、
//! 正規化はその契約を壊す。桁数の検査と、宣言された `scale` に揃えた比較用の正準形は
//! ASCII の 1 パス走査で足りる。**保存される文字列は変えない**（正準形は比較のためだけに
//! 作る）。
//!
//! # 現状
//!
//! 本ファイルは tasks.md 2.1（境界 `TypeCatalog`）が親モジュールの宣言のために置いた骨格で
//! ある。実装はタスク 2.2 が入れる。
