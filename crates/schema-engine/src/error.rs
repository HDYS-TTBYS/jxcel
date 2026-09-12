//! 宣言を拒否する誤り（design.md「Components and Interfaces」の `SchemaError`。
//! tasks.md 1.2。要件 1.5, 1.6, 1.7, 3.5, 3.6, 4.8, 11.4）。
//!
//! # 層の鎖（design.md「内部の依存の向き」）
//!
//! `error / types → declaration → registry → compile → { coerce, validate } → write →
//! evolution → api` の最左である。本モジュールは**どの層にも依存しない**（`std` と
//! design 採択の `thiserror` だけを使う）。したがって本ファイルは、宣言の解析
//! （`declaration`）・型の解決（`compile`）・拡張型の登録（`registry`）のどの段からも
//! 参照できる共有の葉である。
//!
//! # 「誤り」と「違反」を型で分ける（design.md「Components and Interfaces」）
//!
//! [`SchemaError`] は**宣言が壊れている**ことを表し、コンパイルを止める。値が宣言に
//! 合わないことは値の違反（`src/validate/report.rs` の `Violation`。tasks.md 1.3）が
//! 表し、処理を止めない。**この 2 つを 1 つの型にすると、「1 件の不正な値でシート全体が
//! 開けない」という振る舞いが型の上で表現できてしまう**（design.md 同節）。したがって
//! 本型の語彙は宣言の誤りに閉じており、値の違反（型の不一致・範囲外・長さ超過・
//! 書式不一致・必須の欠落・重複・壊れた参照・桁超過・使用不能な列・拡張型の拒否と失敗）
//! を表す変種を持たない。値の違反を表す型は `validate` 層が所有し、本モジュールの
//! 依存の向き（`error` は鎖の最左）からは参照されない（design.md「Error Categories and
//! Responses」の 3 分類）。
//!
//! # 文脈だけを持ち、表示用の文言を持たない
//!
//! 変種は診断に必要な文脈だけを保持する: 列名、`$ref` の参照元と参照先、型定義の識別子、
//! 宣言上の位置、超えた上限。`document-format` の `DocumentError` と同じ規約であり、
//! [`std::fmt::Display`] はログ・デバッグ用の最小限の技術的診断である。利用者向けの提示
//! （ロケール化メッセージを含む）は呼び出し元が `match` で変種と文脈を取り出して
//! 組み立てる（本スペックは UI を持たない。design.md「Error Handling」の Out of Boundary）。
//!
//! # 位置は文字列で持つ（tasks.md 1.2）
//!
//! 位置（`position` / `occurrences` / `from`）と、参照先の識別子（`to`）・拡張型の
//! 識別子（`id`）は、検証済みの新型（`ColumnIndex` / `TypeDefId` など）ではなく
//! **文字列**で持つ。壊れた宣言を拒否するときに問題なのは「その位置を文字どおり記録
//! できること」であり、妥当性の検証を通った新型を要求すると**不正な入力をそのまま
//! 記録できない**（design.md 同節）。

use core::fmt;

use thiserror::Error;

/// 書式のパターンに課す上限の種別（要件 4.5。design.md「Compile Layer / SchemaCompiler」）。
///
/// 利用者が書いたパターンで実行時間が跳ねないよう、生の文字列長・コンパイル後の
/// プログラムの大きさ・入れ子の深さの 3 つに上限を設ける（tasks.md 2.4 が課す）。
/// どれを超えたかによって対処が変わるため、上限の種別を判別可能な列挙体として持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PatternLimit {
    /// パターンの生の文字列長。
    SourceLength,
    /// コンパイル後のプログラムの大きさ。
    CompiledSize,
    /// パターンの入れ子の深さ。
    NestDepth,
}

impl PatternLimit {
    /// 技術的診断用の安定トークン（`Display` と同一。ロケール依存なし）。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SourceLength => "source_length",
            Self::CompiledSize => "compiled_size",
            Self::NestDepth => "nest_depth",
        }
    }
}

impl fmt::Display for PatternLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 宣言を拒否する理由（design.md の宣言の誤り 7 分類 + 上限を超えるパターン）。
///
/// 変種は診断文脈のみを保持し、提示用の文言を持たない（モジュール docs 参照）。値の違反を
/// 表す型とは**別の型**であり、値の違反はここに現れない（design.md「Error Handling」）。
#[derive(Debug, Error)]
pub enum SchemaError {
    /// 宣言に同一の列名が複数現れた（要件 1.5）。宣言ごと拒否する。
    /// `occurrences` は各出現の宣言上の位置（どの並びが重複しているかを示す）。
    #[error("duplicate column name `{name}`; occurrences: {}", occurrences.join(", "))]
    DuplicateColumnName {
        /// 重複した列名。
        name: String,
        /// 各出現の宣言上の位置。
        occurrences: Vec<String>,
    },

    /// 空の列名がある（要件 1.6）。宣言ごと拒否する。
    #[error("empty column name at {position}")]
    EmptyColumnName {
        /// 空の名前があった宣言上の位置。
        position: String,
    },

    /// 宣言の中に解釈できない構造がある（要件 1.7）。宣言ごと拒否する。
    #[error("malformed declaration at {position}: {reason}")]
    MalformedDeclaration {
        /// 解釈できなかった箇所の宣言上の位置。
        position: String,
        /// 解釈できなかった技術的な理由（提示用の文言は呼び出し元が組み立てる）。
        reason: String,
    },

    /// 型定義への参照が実在しない識別子を指している（要件 3.5）。宣言ごと拒否する。
    #[error("dangling type ref: {from} -> {to}")]
    DanglingTypeRef {
        /// 参照元（参照を持つ列・フィールドの宣言上の位置）。
        from: String,
        /// 実在しなかった参照先の型定義の識別子。
        to: String,
    },

    /// 型定義の参照が循環し、その型に適合する値が有限の大きさで存在しえない（要件 3.6）。
    /// 宣言ごと拒否する。
    #[error("type definitions cannot be satisfied by a finite value: {}", type_defs.join(" -> "))]
    ImpossibleCycle {
        /// 循環に含まれる型定義の識別子（循環の順に並ぶ）。
        type_defs: Vec<String>,
    },

    /// 宣言された既定値がその列の型または制約に適合しない（要件 4.8）。宣言ごと拒否する。
    #[error("invalid default for column `{column}`")]
    InvalidDefault {
        /// 適合しない既定値を持つ列（入れ子の内側のフィールドは宣言上の位置で指す）。
        column: String,
    },

    /// 登録された拡張型が既存の型と同一の識別子を使っている（要件 11.4）。登録を拒否する。
    #[error("duplicate custom type id `{id}`")]
    DuplicateCustomTypeId {
        /// 既存の型と衝突した識別子。
        id: String,
    },

    /// 書式のパターンが上限を超えている（要件 4.5。tasks.md 2.4）。宣言ごと拒否する。
    ///
    /// 利用者が書いたパターンで実行時間が跳ねる余地を残さないため、コンパイルの前に
    /// 生の文字列長・コンパイル後の大きさ・入れ子の深さを検査する（design.md
    /// 「Compile Layer / SchemaCompiler」）。
    #[error("pattern limit `{limit}` exceeded at {position}: {observed} > {max}")]
    PatternLimitExceeded {
        /// パターンを宣言した位置。
        position: String,
        /// 超えた上限の種別。
        limit: PatternLimit,
        /// 実際の値。
        observed: usize,
        /// 許される上限。
        max: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 変種を（変種名, 保持する診断文脈）へ分解する。
    ///
    /// `match` はワイルドカードなしで全変種を網羅する: 変種が増減すればここでコンパイルが
    /// 壊れ、判別可能性を機械的に保証する（`document-format` の `error` と同じ規約）。
    fn discriminate(err: &SchemaError) -> (&'static str, Vec<String>) {
        match err {
            SchemaError::DuplicateColumnName { name, occurrences } => {
                let mut ctx = vec![name.clone()];
                ctx.extend(occurrences.iter().cloned());
                ("DuplicateColumnName", ctx)
            }
            SchemaError::EmptyColumnName { position } => {
                ("EmptyColumnName", vec![position.clone()])
            }
            SchemaError::MalformedDeclaration { position, reason } => (
                "MalformedDeclaration",
                vec![position.clone(), reason.clone()],
            ),
            SchemaError::DanglingTypeRef { from, to } => {
                ("DanglingTypeRef", vec![from.clone(), to.clone()])
            }
            SchemaError::ImpossibleCycle { type_defs } => ("ImpossibleCycle", type_defs.clone()),
            SchemaError::InvalidDefault { column } => ("InvalidDefault", vec![column.clone()]),
            SchemaError::DuplicateCustomTypeId { id } => {
                ("DuplicateCustomTypeId", vec![id.clone()])
            }
            SchemaError::PatternLimitExceeded {
                position,
                limit,
                observed,
                max,
            } => (
                "PatternLimitExceeded",
                vec![
                    position.clone(),
                    limit.to_string(),
                    observed.to_string(),
                    max.to_string(),
                ],
            ),
        }
    }

    /// design.md「Error Categories and Responses」が挙げる宣言の誤り（列名の重複・空、
    /// 解釈できない構造、実在しない `$ref`、値が存在しえない循環、型に合わない既定値、
    /// 識別子が重複する拡張型の登録、上限を超えるパターン）を、診断としてあり得る文脈で
    /// 1 個ずつ構築する。
    fn all_variants() -> Vec<SchemaError> {
        vec![
            SchemaError::DuplicateColumnName {
                name: "数量".into(),
                occurrences: vec!["columns[0].name".into(), "columns[3].name".into()],
            },
            SchemaError::EmptyColumnName {
                position: "columns[2].name".into(),
            },
            SchemaError::MalformedDeclaration {
                position: "columns[1].type".into(),
                reason: "type must be an object carrying `kind` or `$ref`".into(),
            },
            SchemaError::DanglingTypeRef {
                from: "columns[4].type.$ref".into(),
                to: "01K4ANRRG004HMASW9NF6YY099".into(),
            },
            SchemaError::ImpossibleCycle {
                type_defs: vec![
                    "01K4ANRRG004HMASW9NF6YY091".into(),
                    "01K4ANRRG004HMASW9NF6YY092".into(),
                ],
            },
            SchemaError::InvalidDefault {
                column: "数量".into(),
            },
            SchemaError::DuplicateCustomTypeId {
                id: "postal-code".into(),
            },
            SchemaError::PatternLimitExceeded {
                position: "columns[0].type.pattern".into(),
                limit: PatternLimit::SourceLength,
                observed: 4096,
                max: 1024,
            },
        ]
    }

    /// 全変種が生成でき、ワイルドカードなしの `match` で自分自身の変種に照合され、文脈
    /// フィールドを取り出せる（tasks.md 1.2「すべての変種が生成・照合できることをテストで
    /// 示す」）。
    #[test]
    fn every_variant_is_constructible_and_discriminable() {
        let errors = all_variants();
        let labels: Vec<&'static str> = errors.iter().map(|e| discriminate(e).0).collect();
        assert_eq!(
            vec![
                "DuplicateColumnName",
                "EmptyColumnName",
                "MalformedDeclaration",
                "DanglingTypeRef",
                "ImpossibleCycle",
                "InvalidDefault",
                "DuplicateCustomTypeId",
                "PatternLimitExceeded",
            ],
            labels,
            "宣言を拒否する誤りの全変種が判別可能でない"
        );

        // 各変種は最低 1 つの診断文脈を露出する（空文脈の変種は診断不能）。
        for (label, ctx) in errors.iter().map(discriminate) {
            assert!(!ctx.is_empty(), "{label} は診断文脈を保持していない");
        }

        // 多フィールド変種で抽出値・順が正しいこと。
        let (_, ctx) = discriminate(&SchemaError::DanglingTypeRef {
            from: "columns[4].type.$ref".into(),
            to: "01K4ANRRG004HMASW9NF6YY099".into(),
        });
        assert_eq!(
            vec!["columns[4].type.$ref", "01K4ANRRG004HMASW9NF6YY099"],
            ctx
        );

        let (_, ctx) = discriminate(&SchemaError::PatternLimitExceeded {
            position: "columns[0].type.pattern".into(),
            limit: PatternLimit::NestDepth,
            observed: 64,
            max: 16,
        });
        assert_eq!(
            vec!["columns[0].type.pattern", "nest_depth", "64", "16"],
            ctx
        );

        // 上限の 3 種別はそれぞれ別のトークンへ落ちる（判別可能性）。
        assert_eq!("source_length", PatternLimit::SourceLength.as_str());
        assert_eq!("compiled_size", PatternLimit::CompiledSize.as_str());
        assert_eq!("nest_depth", PatternLimit::NestDepth.as_str());
    }

    /// 値の違反を表す型とは別の型であることの、型の上の区別（tasks.md 1.2）。
    ///
    /// 本型は「処理を止める」側であり、`std::error::Error` として `Result` の誤り側にだけ
    /// 現れる（design.md「Public API Layer」の
    /// `compile(...) -> Result<CompiledSchema, SchemaError>`）。値の違反は報告として返り、
    /// `Result` の誤り側に載らない（design.md「Error Handling」）。値の違反を表す型は
    /// `validate` 層が所有し、本モジュールの依存の向き（`error` は鎖の最左）からは
    /// 参照できない。以下は、本型が担う役割のうち型と表示に現れる 2 点を固定する。
    #[test]
    fn schema_error_is_the_result_side_error_not_a_value_violation() {
        // (1) 処理を止める側の型であること。値の違反はデータとして返るため、この境界を
        // 持たない（`Violation` 側に `Error` の実装を求めない）。
        fn assert_std_error<T: std::error::Error>() {}
        assert_std_error::<SchemaError>();

        // (2) 表示は文脈の技術的診断であり、提示用の文言を持たない。文脈フィールドが
        // そのまま現れるため、呼び出し元は表示文言を自前で組み立てられる。
        let err = SchemaError::DanglingTypeRef {
            from: "columns[4].type.$ref".into(),
            to: "01K4ANRRG004HMASW9NF6YY099".into(),
        };
        let diagnostic = err.to_string();
        assert!(
            diagnostic.contains("columns[4].type.$ref")
                && diagnostic.contains("01K4ANRRG004HMASW9NF6YY099"),
            "表示が文脈を運んでいない: {diagnostic}"
        );
    }

    /// 位置と識別子は検証済みの新型ではなく文字列で持ち、不正な入力をそのまま記録できる
    /// （tasks.md 1.2）。新型へ締めれば、この構築はコンパイルできなくなる。
    #[test]
    fn positions_record_invalid_input_verbatim() {
        // 改行・NUL・桁外れの添字を含む、宣言として不正な入力をそのまま運ぶ。
        let raw = "columns[999999].name\n\u{0}";
        let errors = vec![
            SchemaError::DuplicateColumnName {
                name: raw.into(),
                occurrences: vec![raw.into(), raw.into()],
            },
            SchemaError::EmptyColumnName {
                position: raw.into(),
            },
            SchemaError::MalformedDeclaration {
                position: raw.into(),
                reason: raw.into(),
            },
            SchemaError::DanglingTypeRef {
                from: raw.into(),
                to: raw.into(),
            },
            SchemaError::ImpossibleCycle {
                type_defs: vec![raw.into()],
            },
            SchemaError::InvalidDefault { column: raw.into() },
            SchemaError::DuplicateCustomTypeId { id: raw.into() },
        ];
        for err in &errors {
            let (label, ctx) = discriminate(err);
            assert!(
                ctx.iter().all(|entry| entry == raw),
                "{label} の文脈が逐語で保持されていない"
            );
        }

        // 上限つきのパターンも位置だけは同じ規約である。
        let err = SchemaError::PatternLimitExceeded {
            position: raw.into(),
            limit: PatternLimit::CompiledSize,
            observed: 0,
            max: 0,
        };
        match err {
            SchemaError::PatternLimitExceeded { position, .. } => assert_eq!(raw, position),
            other => panic!("変種が取り違えられた: {other:?}"),
        }
    }
}
