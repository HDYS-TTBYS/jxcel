//! **能力の門**（tasks.md 2.1。要件 8.1, 8.3, 8.4。design.md 決定 5 / 6）。
//!
//! マクロは、ソース先頭に**宣言した能力**の範囲でだけファイルとネットワークに触れる
//! （決定 6。宣言の解析は `source::capability` = タスク 1.6 が持つ）。**実行時にそれを
//! 強制するのがこの門である** — op の本体は最初に [`check_call`] を呼び、門を通った
//! 呼び出しだけがホストの縫い目（`HostPort`。タスク 2.2–2.4）へ届く（要件 8.1）。
//!
//! # 判定の順序（名前 → 能力 → 実体）
//!
//! 1. 宣言表（[`crate::surface::declaration::HOST_APIS`]）に無い名前は
//!    [`CallRefusal::UnknownApi`]。**公開していない API は呼べない**（要件 10.1 の裏面）
//! 2. その API が要する能力がマクロの宣言に無ければ [`CallRefusal::MissingCapability`]。
//!    **拒んだ能力の名前**を [`CallRefusal::capability`] から取り出せる（要件 8.3 の提示。
//!    提示の面はこれをそのまま使う）
//! 3. 宣言はあるがエンジンがまだ op を登録していなければ [`CallRefusal::Unimplemented`]。
//!    実体の無い API の答えは**明示的な失敗**であり、`todo!()` も嘘の成功も置かない
//!
//! 順序に意味がある: 能力の判定は**呼び出しの前**に終わる（実体が無いことを能力の判定より
//! 先に言うと、宣言の無いファイル読みが「未実装」として通ってしまう）。
//!
//! # 名前は大文字小文字を区別する
//!
//! 能力の**綴り**は 1.6 が大文字小文字を畳むが、**API の名前**は畳まない（`readrange`
//! は 1. で拒まれる）。API の名前は TS の識別子であり、畳むと別の API を呼べてしまう。
//!
//! # 何をしないか
//!
//! 引数の数と型は見ない（op の署名が型で受け取る。`.d.ts` はタスク 3.3 が表から作る）。
//! 実体の呼び出しもしない — 門は「受け付けるか」だけを決め、呼ぶのはエンジンである。

use core::fmt;

use crate::source::capability::{Capability, CapabilitySet};
use crate::surface::declaration::{self, ApiDecl};

/// 呼び出しを受け付けなかった理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallRefusal {
    /// 宣言表に無い名前（公開していない API は呼べない）。
    UnknownApi {
        /// 呼ぼうとした名前。宣言表に引く手掛かりが無いので、生の文字列で運ぶ。
        name: String,
    },
    /// マクロの宣言に、その API が要する能力が無い（要件 8.1, 8.3）。
    MissingCapability {
        /// 拒まれた API。
        api: &'static ApiDecl,
        /// 宣言に無かった能力（**提示するのはこの名前である**）。
        capability: Capability,
    },
    /// 宣言はあるが、実体（op）がまだ登録されていない（タスク 3.2 が結線するまで）。
    Unimplemented {
        /// 実体の無い API。
        api: &'static ApiDecl,
    },
}

impl CallRefusal {
    /// 受け付けなかった呼び出しの名前（宣言上の名前。宣言に無い名前は呼ばれたまま）。
    pub fn api(&self) -> &str {
        match self {
            Self::UnknownApi { name } => name,
            Self::MissingCapability { api, .. } | Self::Unimplemented { api } => api.name,
        }
    }

    /// **拒んだ能力**（要件 8.3 の提示。能力の不在が理由でなければ `None`）。
    pub fn capability(&self) -> Option<Capability> {
        match self {
            Self::UnknownApi { .. } | Self::Unimplemented { .. } => None,
            Self::MissingCapability { capability, .. } => Some(*capability),
        }
    }
}

impl fmt::Display for CallRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownApi { name } => write!(f, "知らないホスト API {name}"),
            Self::MissingCapability { api, capability } => write!(
                f,
                "ホスト API {} には能力 {capability} の宣言が要る",
                api.js_name()
            ),
            Self::Unimplemented { api } => {
                write!(f, "ホスト API {} の実体がまだ無い", api.js_name())
            }
        }
    }
}

impl std::error::Error for CallRefusal {}

/// 1 回のホスト API 呼び出しを門に通す（**呼び出しの前に**呼ぶ。要件 8.1, 8.3）。
///
/// `declared` はマクロのソース先頭の宣言（タスク 1.6 が作る集合。宣言が無ければ空であり、
/// **能力を要する API はすべて拒まれる**）。`registered` はエンジンが登録した op の名前であり、
/// 空なら「実体がまだ無い」ことを意味する（[`CallRefusal::Unimplemented`]）。
///
/// 通れば、その API の宣言（[`ApiDecl`]）を返す。呼び出し側はこの宣言に従って本体へ渡す。
pub fn check_call(
    name: &str,
    declared: &CapabilitySet,
    registered: &[&str],
) -> Result<&'static ApiDecl, CallRefusal> {
    let Some(api) = declaration::get(name) else {
        return Err(CallRefusal::UnknownApi {
            name: name.to_owned(),
        });
    };
    if let Some(capability) = api.required_capability() {
        if !declared.contains(capability) {
            return Err(CallRefusal::MissingCapability { api, capability });
        }
    }
    if !registered.iter().any(|entry| *entry == api.name) {
        return Err(CallRefusal::Unimplemented { api });
    }
    Ok(api)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 宣言の集合を組み立てる（テストの前提を 1 行で書けるようにする）。
    fn cap(declared: &[Capability]) -> CapabilitySet {
        let mut set = CapabilitySet::empty();
        for capability in declared {
            set.insert(*capability);
        }
        set
    }

    /// 実体がすべて登録されている状態の登録簿（門の判定だけを見たいときに使う）。
    fn all_registered() -> Vec<&'static str> {
        declaration::HOST_APIS.iter().map(|api| api.name).collect()
    }

    #[test]
    fn a_capability_that_is_not_declared_is_refused_with_its_name() {
        let refusal = check_call("fileRead", &CapabilitySet::empty(), &all_registered())
            .expect_err("宣言が無ければファイルは読めない");
        assert_eq!(
            refusal,
            CallRefusal::MissingCapability {
                api: declaration::get("fileRead").expect("宣言にある"),
                capability: Capability::FileRead,
            }
        );
        // **拒んだ能力の名前**が取り出せる（要件 8.3 の提示はこれを使う）。
        assert_eq!(refusal.capability(), Some(Capability::FileRead));
        let shown = refusal.to_string();
        assert!(
            shown.contains("file.read") && shown.contains("host.fileRead"),
            "提示に能力の名前と API の名前が出ていない: {shown}"
        );
    }

    #[test]
    fn the_three_capabilities_are_each_required_on_their_own() {
        // 読みだけを宣言したマクロは、書きもネットワークも**できない**（要件 8.4）。
        let declared = cap(&[Capability::FileRead]);
        let granted = check_call("fileRead", &declared, &all_registered());
        assert!(
            granted.is_ok(),
            "宣言した能力で拒まれた: {:?}",
            granted.err()
        );
        for (api, capability) in [
            ("fileWrite", Capability::FileWrite),
            ("netFetch", Capability::Net),
        ] {
            let refusal = check_call(api, &declared, &all_registered())
                .expect_err("宣言の無い能力は使えない");
            assert_eq!(refusal.capability(), Some(capability), "{api} の拒みが違う");
        }
    }

    #[test]
    fn an_api_that_needs_no_capability_passes_with_an_empty_declaration() {
        // 宣言が 1 つも無いマクロでも、シートと行の読み書きはできる（要件 4.1）。
        for name in ["sheets", "columns", "readRange", "setCells", "insertRows"] {
            let result = check_call(name, &CapabilitySet::empty(), &all_registered());
            assert!(
                result.is_ok(),
                "能力を要さない API が拒まれた: {name} {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn every_api_in_the_table_can_be_called_when_its_capability_is_declared() {
        // 「宣言にある API はすべて呼べる」（tasks.md 2.1 の受け入れ）。能力を要する口は、
        // **その能力を宣言した**マクロから呼べる（宣言が無ければ拒まれることは別の検査）。
        let registered = all_registered();
        for api in declaration::HOST_APIS {
            let declared = match api.required_capability() {
                Some(capability) => cap(&[capability]),
                None => CapabilitySet::empty(),
            };
            let called = check_call(api.name, &declared, &registered);
            assert!(
                called.is_ok(),
                "{} が呼べない: {:?}",
                api.js_name(),
                called.err()
            );
        }
    }

    #[test]
    fn an_api_outside_the_table_cannot_be_called() {
        let refusal = check_call(
            "deleteEverything",
            &cap(&[Capability::FileWrite]),
            &all_registered(),
        )
        .expect_err("宣言表に無い名前は呼べない");
        assert_eq!(
            refusal,
            CallRefusal::UnknownApi {
                name: "deleteEverything".to_owned(),
            }
        );
        assert_eq!(refusal.capability(), None);

        // 綴りが違うだけの名前も「無い名前」である（表の名前は TS の識別子である）。
        assert_eq!(
            check_call(
                "filewrite",
                &cap(&[Capability::FileWrite]),
                &all_registered()
            ),
            Err(CallRefusal::UnknownApi {
                name: "filewrite".to_owned(),
            })
        );
    }

    #[test]
    fn an_api_without_an_implementation_is_unimplemented_rather_than_silently_accepted() {
        // 宣言はあるがまだ op が無い（タスク 2.1 の時点のすべての API がこれである）。
        // 答えは**明示的な失敗**であり、成功でも `todo!()` でもない。
        let refusal = check_call("setCells", &CapabilitySet::empty(), &[]).expect_err("実体が無い");
        assert_eq!(
            refusal,
            CallRefusal::Unimplemented {
                api: declaration::get("setCells").expect("宣言にある"),
            }
        );

        // 能力の判定は実体の有無より先である（宣言の無いファイル読みが「未実装」で
        // 通ってしまわない。順序が逆だと門が意味を失う）。
        let refusal = check_call("fileWrite", &cap(&[Capability::FileRead]), &[])
            .expect_err("能力の宣言が無い");
        assert_eq!(refusal.capability(), Some(Capability::FileWrite));

        // 登録簿に載れば同じ呼び出しが通る（`Unimplemented` は登録の不在だけを言う）。
        assert!(check_call("setCells", &CapabilitySet::empty(), &["setCells"]).is_ok());
    }
}
