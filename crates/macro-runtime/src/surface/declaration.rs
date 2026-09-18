//! ホスト API の**宣言表**（tasks.md 2.1。要件 4.1, 4.6, 8.1, 8.4, 10.1。design.md 決定 5）。
//!
//! マクロへ公開する API を **1 つの表**（[`HOST_APIS`]）に並べる。行が持つのは
//! **名前・引数・戻り値の型・必要とする能力**の 4 つだけである。この表が 3 つの用途の
//! **唯一の源**になる（用途ごとに一覧を持つと、必ずどれかがずれる。design.md 決定 5）:
//!
//! 1. **op の登録** — エンジン（`engine/isolate.rs`。タスク 3.2）が表を読み、1 行につき
//!    1 つの op を登録する。登録された名前は [`check_registration`] で表と突き合わせる
//! 2. **能力の門** — [`crate::surface::gate`] が [`ApiDecl::capability`] を読み、マクロの
//!    ソース先頭の宣言（`source::capability::CapabilitySet`。タスク 1.6）と突き合わせて
//!    **呼び出しの前に**拒む（要件 8.1, 8.3）
//! 3. **型定義** — `types.rs`（タスク 3.3）が [`ApiDecl::params`] と [`ApiDecl::returns`] を
//!    読み、`types/macro-host.d.ts` を組み立てる（要件 10.1–10.3）
//!
//! # 能力の欄は必須である（既定を「能力不要」にしない）
//!
//! [`ApiDecl::capability`] の型は [`ApiCapability`] であり、**「能力不要」も
//! [`ApiCapability::NotRequired`] と明示的に書く**。`Option` の既定値へ逃がさないのは、
//! 欄の書き忘れが「能力不要」に化けると門が静かに素通りするからである
//! （`research.md`「Risks & Mitigations」の「能力の門が『付け忘れ』で素通りする」）。
//! 欄を省いた宣言は**コンパイルできない**（[`ApiDecl`] の doctest がその性質を固定する —
//! `cargo test` が落ちることで検査になる）。
//!
//! # 何を宣言しないか
//!
//! - **`console`**: マクロが出す出力は JS の組み込み（`console.log` 等）であり、エンジンが
//!    isolate へ与える。表が持つのは `host` の API だけである（出力の回収はタスク 3.2）
//! - **列の宣言を書き換える API**: 公開しない。列の宣言は読み取り専用であり（要件 4.5）、
//!   表に無い名前を呼べば門が [`crate::surface::gate::CallRefusal::UnknownApi`] として
//!   **名前つきで**拒む
//!
//! # 型の名前（タスク 3.3 との約束）
//!
//! [`ApiDecl::params`] と [`ApiDecl::returns`] の型は **TypeScript の型名**であり、`ts-rs`
//! が Rust の型から生成する宣言と同名でなければならない。表は「その名前の型が無ければ
//! 生成が失敗する」ことを要求する側であり、**型の無い API を公開できない**（要件 10.1）。
//! 現在の表が要求する名前: `SheetInfo` / `SheetId` / `ColumnTypeInfo` / `RowSpan` /
//! `RowPage` / `CellValue` / `CellWrite` / `RowId`。

use core::fmt;

use crate::source::capability::Capability;

/// マクロから API を見るときの名前空間（`host.readRange(...)`）。
pub const HOST_NAMESPACE: &str = "host";

/// 1 つのホスト API の宣言。
///
/// 4 つの欄はすべて必須である。特に [`ApiDecl::capability`] は**能力不要の場合も明示**する
/// （モジュールの doc を参照）:
///
/// ```compile_fail
/// use macro_runtime::surface::declaration::ApiDecl;
///
/// // 能力の欄が無い → コンパイルできない（既定を「能力不要」にしない）。
/// const BROKEN: ApiDecl = ApiDecl {
///     name: "broken",
///     params: &[],
///     returns: "void",
/// };
/// ```
///
/// 型も同じく必須である（型の無い API を公開しない。要件 10.1）:
///
/// ```compile_fail
/// use macro_runtime::surface::declaration::{ApiCapability, ApiDecl};
///
/// // 戻り値の型の欄が無い → コンパイルできない。
/// const BROKEN: ApiDecl = ApiDecl {
///     name: "broken",
///     params: &[],
///     capability: ApiCapability::NotRequired,
/// };
/// ```
///
/// 能力不要の API も、欄を明示して書く:
///
/// ```
/// use macro_runtime::surface::declaration::{ApiCapability, ApiDecl, ParamDecl};
///
/// const PING: ApiDecl = ApiDecl {
///     name: "ping",
///     params: &[ParamDecl { name: "sheet", ty: "SheetId" }],
///     returns: "void",
///     capability: ApiCapability::NotRequired,
/// };
///
/// assert_eq!(PING.required_capability(), None);
/// assert_eq!(PING.js_name(), "host.ping");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApiDecl {
    /// マクロから見える名前（[`HOST_NAMESPACE`] の後ろ。`readRange` など）。
    ///
    /// **大文字小文字を区別する**（TS の識別子である。畳むと別の API を呼べてしまう）。
    pub name: &'static str,
    /// 引数（並びがそのまま呼び出しの並びである）。
    pub params: &'static [ParamDecl],
    /// 戻り値の型の名前。何も返さない API は `"void"`。
    pub returns: &'static str,
    /// **必要とする能力の欄**。能力不要も [`ApiCapability::NotRequired`] と明示する。
    pub capability: ApiCapability,
}

impl ApiDecl {
    /// この API が要する能力（能力不要なら `None`）。
    pub const fn required_capability(&self) -> Option<Capability> {
        self.capability.required()
    }

    /// マクロから見た名前（`host.readRange`）。拒否の提示と実行の記録に使う（要件 9.2）。
    pub fn js_name(&self) -> String {
        format!("{HOST_NAMESPACE}.{}", self.name)
    }
}

/// 1 つの引数の宣言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParamDecl {
    /// 引数の名前（`.d.ts` の仮引数名になる）。
    pub name: &'static str,
    /// 引数の型の名前（[`ApiDecl`] の doc を参照）。
    pub ty: &'static str,
}

/// 能力の欄（[`ApiDecl::capability`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiCapability {
    /// この能力がマクロの宣言に無ければ、この API は**呼べない**（要件 8.1, 8.4）。
    Required(Capability),
    /// 能力を要さない（**明示的に書く**。欄の省略で得られる既定ではない）。
    NotRequired,
}

impl ApiCapability {
    /// 要する能力（[`Self::NotRequired`] なら `None`）。
    pub const fn required(self) -> Option<Capability> {
        match self {
            Self::Required(capability) => Some(capability),
            Self::NotRequired => None,
        }
    }
}

/// マクロへ公開する API の表（**登録・門・型定義の唯一の源**。design.md 決定 5）。
///
/// 並びは `.d.ts` と登録の順序を決める。**読み → 書き → 能力を要する口**の順に固定する
/// （人が読む表として、能力を要する API が末尾にまとまっている方が確認しやすい）。
///
/// 各行の `capability` は必ず書く（[`ApiCapability::NotRequired`] も明示である）。
pub const HOST_APIS: &[ApiDecl] = &[
    // --- 読み（能力不要。要件 4.1–4.5） --------------------------------------
    ApiDecl {
        name: "sheets",
        params: &[],
        returns: "SheetInfo[]",
        capability: ApiCapability::NotRequired,
    },
    ApiDecl {
        name: "columns",
        params: &[ParamDecl {
            name: "sheet",
            ty: "SheetId",
        }],
        returns: "ColumnTypeInfo[]",
        capability: ApiCapability::NotRequired,
    },
    // 行の範囲は**1 回の呼び出し**で読む（行ごとの呼び出しを強いない。要件 4.4 / 11.3）。
    ApiDecl {
        name: "readRange",
        params: &[
            ParamDecl {
                name: "sheet",
                ty: "SheetId",
            },
            ParamDecl {
                name: "span",
                ty: "RowSpan",
            },
        ],
        returns: "RowPage",
        capability: ApiCapability::NotRequired,
    },
    // --- 書き（能力不要。適用はアダプタが 1 回で行う。要件 5.1–5.5） ---------
    ApiDecl {
        name: "setCells",
        params: &[
            ParamDecl {
                name: "sheet",
                ty: "SheetId",
            },
            ParamDecl {
                name: "writes",
                ty: "CellWrite[]",
            },
        ],
        returns: "void",
        capability: ApiCapability::NotRequired,
    },
    ApiDecl {
        name: "insertRows",
        params: &[
            ParamDecl {
                name: "sheet",
                ty: "SheetId",
            },
            ParamDecl {
                name: "values",
                ty: "CellValue[][]",
            },
        ],
        returns: "void",
        capability: ApiCapability::NotRequired,
    },
    ApiDecl {
        name: "removeRows",
        params: &[
            ParamDecl {
                name: "sheet",
                ty: "SheetId",
            },
            ParamDecl {
                name: "rows",
                ty: "RowId[]",
            },
        ],
        returns: "void",
        capability: ApiCapability::NotRequired,
    },
    ApiDecl {
        name: "duplicateRows",
        params: &[
            ParamDecl {
                name: "sheet",
                ty: "SheetId",
            },
            ParamDecl {
                name: "rows",
                ty: "RowId[]",
            },
        ],
        returns: "void",
        capability: ApiCapability::NotRequired,
    },
    // --- 能力を要する口（宣言が無ければ門が呼び出しの前に拒む。要件 8.1–8.4） --
    ApiDecl {
        name: "fileRead",
        params: &[ParamDecl {
            name: "path",
            ty: "string",
        }],
        returns: "string",
        capability: ApiCapability::Required(Capability::FileRead),
    },
    ApiDecl {
        name: "fileWrite",
        params: &[
            ParamDecl {
                name: "path",
                ty: "string",
            },
            ParamDecl {
                name: "text",
                ty: "string",
            },
        ],
        returns: "void",
        capability: ApiCapability::Required(Capability::FileWrite),
    },
    ApiDecl {
        name: "netFetch",
        params: &[ParamDecl {
            name: "url",
            ty: "string",
        }],
        returns: "string",
        capability: ApiCapability::Required(Capability::Net),
    },
];

/// 名前から宣言を引く（**門と登録が使う唯一の索引**）。無ければ `None`。
pub fn get(name: &str) -> Option<&'static ApiDecl> {
    HOST_APIS.iter().find(|decl| decl.name == name)
}

/// 宣言と実装（エンジンが登録した op の名前）の一致を検査する（tasks.md 2.1 の受け入れ）。
///
/// 両方向を見る: 宣言にある API がすべて登録されていること（実装漏れ）と、登録された名前が
/// すべて宣言にあること（公開していない実装）。片方だけでは、表に足して実装を忘れた行か、
/// 表に無い op のどちらかが残る。
///
/// 呼ぶのはエンジン（`engine/isolate.rs`。タスク 3.2）であり、**op を登録した直後**に
/// 登録した名前の一覧を渡す。一致しなければ起動を止める（黙って片側だけを持つ状態を
/// 作らない）。
pub fn check_registration(registered: &[&str]) -> Result<(), RegistrationMismatch> {
    let missing = HOST_APIS
        .iter()
        .map(|decl| decl.name)
        .filter(|name| !registered.iter().any(|entry| entry == name))
        .collect::<Vec<_>>();
    let undeclared = registered
        .iter()
        .filter(|name| get(name).is_none())
        .map(|name| (*name).to_owned())
        .collect::<Vec<_>>();
    if missing.is_empty() && undeclared.is_empty() {
        Ok(())
    } else {
        Err(RegistrationMismatch {
            missing,
            undeclared,
        })
    }
}

/// 宣言と実装のずれ（[`check_registration`] の答え）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationMismatch {
    /// 宣言にあるのに登録されていない API（実装が足りない側）。
    pub missing: Vec<&'static str>,
    /// 登録されているのに宣言に無い名前（公開していない側）。
    pub undeclared: Vec<String>,
}

impl fmt::Display for RegistrationMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "宣言と登録が一致しない: 未登録 [{}] / 宣言に無い [{}]",
            self.missing.join(", "),
            self.undeclared.join(", ")
        )
    }
}

impl std::error::Error for RegistrationMismatch {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_so_the_index_has_one_answer_per_name() {
        for (index, decl) in HOST_APIS.iter().enumerate() {
            let later = HOST_APIS[index + 1..]
                .iter()
                .find(|other| other.name == decl.name);
            assert!(
                later.is_none(),
                "名前 {} が 2 度ある（`get` が片方にしか答えられない）",
                decl.name
            );
            assert_eq!(
                get(decl.name).map(|found| found.name),
                Some(decl.name),
                "索引が表の行を引けない"
            );
        }
        assert_eq!(get("deleteEverything"), None);
    }

    #[test]
    fn capability_column_maps_each_api_to_its_capability() {
        // 能力の欄は型で必須であり（`ApiCapability` に既定が無い）、型の欄も同じである。
        // ここでは表が「能力を要する API」と「要さない API」を**取り違えずに**書けているかを
        // 見る（門の検査は `gate` の側が呼び出しの単位で固定する）。
        let requiring = HOST_APIS
            .iter()
            .filter_map(|decl| {
                decl.required_capability()
                    .map(|capability| (decl.name, capability))
            })
            .collect::<Vec<_>>();
        assert_eq!(
            requiring,
            vec![
                ("fileRead", Capability::FileRead),
                ("fileWrite", Capability::FileWrite),
                ("netFetch", Capability::Net),
            ],
            "能力の欄の対応が変わった（要件 8.4: 3 つは別の能力である）"
        );
    }

    #[test]
    fn a_registration_that_misses_an_api_is_reported_with_its_name() {
        let mismatch = check_registration(&["sheets"]).expect_err("実装が足りない");
        assert!(
            mismatch.missing.contains(&"readRange"),
            "足りない API の名前が挙がっていない: {mismatch}"
        );
        assert!(
            mismatch.missing.contains(&"fileWrite"),
            "能力を要する口も同じく登録が要る: {mismatch}"
        );
        assert_eq!(mismatch.missing.len(), HOST_APIS.len() - 1);
        assert!(
            mismatch.undeclared.is_empty(),
            "宣言にある名前を「宣言に無い」と言った: {mismatch}"
        );
    }

    #[test]
    fn a_registration_that_is_not_declared_is_reported_with_its_name() {
        let mismatch = check_registration(&["sheets", "deleteEverything"]).expect_err("余分がある");
        assert_eq!(mismatch.undeclared, vec!["deleteEverything".to_owned()]);
        assert_eq!(mismatch.missing.len(), HOST_APIS.len() - 1);
    }

    #[test]
    fn a_registration_that_covers_the_table_is_accepted() {
        let registered = HOST_APIS.iter().map(|decl| decl.name).collect::<Vec<_>>();
        assert_eq!(check_registration(&registered), Ok(()));
    }
}
