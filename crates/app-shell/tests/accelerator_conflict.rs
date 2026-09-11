//! キーボードショートカットの一意性検査（要件 3.4、tasks.md 4.6）。
//!
//! `AcceleratorRegistry::insert` が同じ組み合わせの二重登録を**登録の時点で**競合として返し、
//! 競合が既存の登録と新しい登録の**両方**を含むことを固定する（完了状態）。あわせて、一意性
//! 検査の前提である正規化（等価な綴りは同じ組み合わせ）、同じ登録の再登録の冪等性、列挙順の
//! 決定性、誤った綴りの拒否を検証する。
//!
//! このファイルは次の主張を固定する:
//!
//! - `duplicate_chord_from_another_registration_is_reported_with_both_sides`: 完了状態。
//!   同じ組み合わせを別の登録が要求すると競合が返り、エラーからどちらとどちらが衝突したかが
//!   分かる。既存の登録は残って機能し続ける（片方を黙って捨てない）
//! - `equivalent_spellings_are_the_same_combination` / `modifier_and_key_aliases_are_folded`:
//!   大文字小文字・空白・修飾キーの順序・別名の違いを越えて同じ組み合わせとして衝突する
//! - `distinct_chords_do_not_conflict`: 本当に別の組み合わせは競合しない（誤検出しない）
//! - `reinserting_the_same_registration_is_idempotent`: メニュー再構築のための冪等性
//! - `three_owners_report_the_right_pair_and_lose_no_registration`: 競合が常に正しい組を指し、
//!   どの登録も失われない
//! - `malformed_chords_are_rejected_with_a_reason`: 誤った綴りを「別のキー」として黙って
//!   受け入れない
//!
//! プラットフォーム固有のコードは無く、3 OS で同じ結果になる。開発者の環境にも依存しない
//! （一時ファイルも環境変数も使わない）。

use app_shell::accelerator::{
    Accelerator, AcceleratorOwner, AcceleratorParseError, AcceleratorRegistry, MenuItemId,
};

// ---------------------------------------------------------------------------
// 補助関数
// ---------------------------------------------------------------------------

/// テスト用のショートカット。誤った綴りはテストの誤りなので panic させてよい。
fn chord(input: &str) -> Accelerator {
    Accelerator::parse(input).unwrap_or_else(|error| panic!("{input:?} は正しい: {error}"))
}

fn owner(identity: &str) -> AcceleratorOwner {
    AcceleratorOwner::new(identity)
}

fn item(identity: &str) -> MenuItemId {
    MenuItemId::new(identity)
}

/// 登録簿の列挙を (登録元, 項目, 正準形) の並びに落とす。
fn listed(registry: &AcceleratorRegistry) -> Vec<(String, String, String)> {
    registry
        .registrations()
        .map(|registration| {
            (
                registration.owner.as_str().to_string(),
                registration.item.as_str().to_string(),
                registration.chord.as_str().to_string(),
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// 完了状態
// ---------------------------------------------------------------------------

/// 同じ組み合わせを別の登録が要求すると競合が返り、**どちらの登録が衝突したか**がエラーから
/// 分かる。既存の登録は競合の後も残って機能し続ける（要件 3.4、tasks.md 4.6 の完了状態）。
#[test]
fn duplicate_chord_from_another_registration_is_reported_with_both_sides() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("document"), item("save"), chord("Ctrl+S"))
        .expect("最初の登録は成功する");

    let conflict = registry
        .insert(owner("macro"), item("save-macro"), chord("ctrl+s"))
        .expect_err("同じ組み合わせの別登録は競合する");

    // 競合は両側を運ぶ。
    assert_eq!(conflict.chord, chord("Ctrl+S"));
    assert_eq!(conflict.existing.owner, owner("document"));
    assert_eq!(conflict.existing.item, item("save"));
    assert_eq!(conflict.existing.chord, chord("Ctrl+S"));
    assert_eq!(conflict.incoming.owner, owner("macro"));
    assert_eq!(conflict.incoming.item, item("save-macro"));
    assert_eq!(conflict.incoming.chord, chord("ctrl+s"));

    // エラーの文言からもどちらとどちらが衝突したか分かる。
    let message = conflict.to_string();
    assert!(
        message.contains("document#save") && message.contains("macro#save-macro"),
        "エラーに両方の登録が現れる: {message}"
    );
    assert!(
        message.contains("ctrl+KeyS"),
        "エラーに衝突した組み合わせが現れる: {message}"
    );

    // 既存の登録は残り、機能し続ける。拒否された登録は加わっていない。
    let existing = registry
        .resolve(&chord("Ctrl+S"))
        .expect("既存の登録が残っている");
    assert_eq!(existing.owner, owner("document"));
    assert_eq!(existing.item, item("save"));
    assert_eq!(registry.len(), 1);
    assert!(registry
        .registration(&owner("macro"), &item("save-macro"))
        .is_none());

    // 別の組み合わせなら、拒否されたのと同じ登録元から登録できる。
    registry
        .insert(owner("macro"), item("save-macro"), chord("Ctrl+Shift+M"))
        .expect("別の組み合わせは競合しない");
    assert_eq!(registry.len(), 2);
}

// ---------------------------------------------------------------------------
// 正規化（等価な綴り）
// ---------------------------------------------------------------------------

/// 大文字小文字・空白・修飾キーの順序・重複する修飾キーの違いを越えて、同じ組み合わせとして
/// 衝突する。
#[test]
fn equivalent_spellings_are_the_same_combination() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("first"), item("save"), chord("Ctrl+Shift+S"))
        .expect("最初の登録は成功する");

    let equivalents = [
        "shift+control+S",
        "shift+ctrl+s",
        "control+shift+KeyS",
        "CTRL+SHIFT+S",
        "  ctrl + shift + s  ",
        "Shift+Control+Shift+S",
    ];
    for (index, spelling) in equivalents.iter().enumerate() {
        let conflict = registry
            .insert(
                owner(&format!("other-{index}")),
                item(&format!("item-{index}")),
                chord(spelling),
            )
            .expect_err("等価な綴りは既存と衝突する");
        assert_eq!(
            conflict.existing.owner,
            owner("first"),
            "{spelling:?} は最初の登録と衝突する"
        );
    }
    assert_eq!(registry.len(), 1, "等価な綴りは増えない");

    // 正準形は入力の綴りに依存しない。
    assert_eq!(
        chord("  CTRL + shift + s ").as_str(),
        "ctrl+shift+KeyS",
        "正準形は修飾キーの順序と大小文字を揃える"
    );
}

/// 修飾キーとキーの別名を同じものへ畳む。
#[test]
fn modifier_and_key_aliases_are_folded() {
    let cases = [
        ("Cmd+S", "Super+S"),
        ("Command+S", "cmd+s"),
        ("Option+F4", "Alt+F4"),
        ("Ctrl+KeyS", "Ctrl+S"),
        ("Escape", "Esc"),
        ("Ctrl+Comma", "Ctrl+,"),
        ("Ctrl+Numpad4", "Ctrl+Num4"),
        ("ArrowUp", "Up"),
        ("Ctrl+Equal", "Ctrl+="),
    ];
    for (first, second) in cases {
        let mut registry = AcceleratorRegistry::new();
        registry
            .insert(owner("first"), item("one"), chord(first))
            .unwrap_or_else(|conflict| panic!("{first:?} の登録は成功する: {conflict}"));
        let conflict = registry
            .insert(owner("second"), item("two"), chord(second))
            .expect_err("別名は同じ組み合わせとして衝突する");
        assert_eq!(
            conflict.existing.chord,
            chord(first),
            "{first:?} と {second:?} は同じ組み合わせ"
        );
    }
}

/// 本当に別の組み合わせは競合しない（一意性検査が誤検出しない）。
#[test]
fn distinct_chords_do_not_conflict() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("base"), item("save"), chord("Ctrl+S"))
        .expect("最初の登録は成功する");

    let distinct = [
        ("alt", "Ctrl+Alt+S"),
        ("shift", "Ctrl+Shift+S"),
        ("shift-only", "Shift+S"),
        ("no-modifier", "S"),
        ("alt-only", "Alt+S"),
        ("function", "Ctrl+F4"),
    ];
    for (identity, spelling) in distinct {
        registry
            .insert(owner(identity), item(identity), chord(spelling))
            .unwrap_or_else(|conflict| panic!("{spelling:?} は別の組み合わせ: {conflict}"));
    }
    assert_eq!(registry.len(), 7);
}

// ---------------------------------------------------------------------------
// 同じ登録の再登録（冪等）と更新
// ---------------------------------------------------------------------------

/// メニューは再構築されるので、同じ登録元 + 同じ項目 + 同じ組み合わせの再登録は成功し、
/// 状態を増やさない。競合は**異なる登録**の間でだけ起きる。
#[test]
fn reinserting_the_same_registration_is_idempotent() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("feature"), item("copy"), chord("Ctrl+C"))
        .expect("最初の登録は成功する");
    registry
        .insert(owner("feature"), item("copy"), chord("ctrl+c"))
        .expect("同じ登録の再登録は成功する");
    registry
        .insert(owner("feature"), item("copy"), chord("Ctrl+C"))
        .expect("3 回目も成功する");

    assert_eq!(registry.len(), 1, "再登録で登録は増えない");
    assert_eq!(registry.registrations().count(), 1);
    let registration = registry
        .resolve(&chord("Ctrl+C"))
        .expect("登録は残っている");
    assert_eq!(registration.owner, owner("feature"));
    assert_eq!(registration.item, item("copy"));
}

/// 同じ登録元でも**別の項目**が同じ組み合わせを要求すれば競合である（要件 3.4 は複数の項目に
/// ついて述べている）。
#[test]
fn a_second_item_of_the_same_owner_conflicts() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("feature"), item("save"), chord("Ctrl+S"))
        .expect("最初の登録は成功する");

    let conflict = registry
        .insert(owner("feature"), item("export"), chord("Ctrl+S"))
        .expect_err("同じ登録元でも別の項目なら競合する");

    assert_eq!(conflict.existing.owner, owner("feature"));
    assert_eq!(conflict.existing.item, item("save"));
    assert_eq!(conflict.incoming.owner, owner("feature"));
    assert_eq!(conflict.incoming.item, item("export"));
    assert_eq!(registry.len(), 1);
}

/// 同じ項目が別の組み合わせで再登録された場合は、その項目の組み合わせの更新である。
#[test]
fn reregistering_an_item_with_a_new_chord_updates_it_in_place() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("feature"), item("save"), chord("Ctrl+S"))
        .expect("最初の登録は成功する");
    registry
        .insert(owner("feature"), item("save"), chord("Ctrl+T"))
        .expect("同じ項目の組み合わせの更新は成功する");

    assert_eq!(registry.len(), 1);
    assert!(
        registry.resolve(&chord("Ctrl+S")).is_none(),
        "旧い組み合わせは外れている"
    );
    let registration = registry
        .resolve(&chord("Ctrl+T"))
        .expect("新しい組み合わせで引ける");
    assert_eq!(registration.owner, owner("feature"));
    assert_eq!(registration.item, item("save"));
}

/// 別の登録が保持している組み合わせへ更新しようとした場合は競合として拒否し、**何も変えない**。
#[test]
fn an_update_that_collides_changes_nothing() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("feature"), item("save"), chord("Ctrl+S"))
        .expect("最初の登録は成功する");
    registry
        .insert(owner("other"), item("other"), chord("Ctrl+T"))
        .expect("2 件目の登録は成功する");

    let conflict = registry
        .insert(owner("feature"), item("save"), chord("Ctrl+T"))
        .expect_err("他の登録が持つ組み合わせへは更新できない");

    assert_eq!(conflict.existing.owner, owner("other"));
    assert_eq!(conflict.incoming.owner, owner("feature"));
    assert_eq!(registry.len(), 2);
    assert_eq!(
        registry.resolve(&chord("Ctrl+S")).map(|r| &r.owner),
        Some(&owner("feature")),
        "既存の組み合わせは元のまま"
    );
    assert_eq!(
        registry.resolve(&chord("Ctrl+T")).map(|r| &r.owner),
        Some(&owner("other")),
        "占有されている組み合わせは元のまま"
    );
}

// ---------------------------------------------------------------------------
// 複数の登録元
// ---------------------------------------------------------------------------

/// 3 つの登録元が競合しても、競合は常に正しい組を指し、成功した登録はどれも失われない。
#[test]
fn three_owners_report_the_right_pair_and_lose_no_registration() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("alpha"), item("one"), chord("Ctrl+S"))
        .expect("alpha の登録は成功する");

    let first = registry
        .insert(owner("beta"), item("two"), chord("Ctrl+S"))
        .expect_err("beta は alpha と競合する");
    assert_eq!(first.existing.owner, owner("alpha"));
    assert_eq!(first.existing.item, item("one"));
    assert_eq!(first.incoming.owner, owner("beta"));
    assert_eq!(first.incoming.item, item("two"));

    let second = registry
        .insert(owner("gamma"), item("three"), chord("Ctrl+S"))
        .expect_err("gamma も先に登録された alpha と競合する");
    assert_eq!(
        second.existing.owner,
        owner("alpha"),
        "競合の相手は常に先に登録された側"
    );
    assert_eq!(second.incoming.owner, owner("gamma"));

    registry
        .insert(owner("beta"), item("two"), chord("Ctrl+T"))
        .expect("beta は別の組み合わせで登録できる");
    let third = registry
        .insert(owner("gamma"), item("three"), chord("Ctrl+T"))
        .expect_err("gamma は beta と競合する");
    assert_eq!(third.existing.owner, owner("beta"));
    assert_eq!(third.existing.item, item("two"));
    assert_eq!(third.incoming.owner, owner("gamma"));

    registry
        .insert(owner("gamma"), item("three"), chord("Ctrl+U"))
        .expect("gamma は別の組み合わせで登録できる");

    assert_eq!(
        listed(&registry),
        vec![
            ("alpha".to_string(), "one".to_string(), "ctrl+KeyS".to_string()),
            ("beta".to_string(), "two".to_string(), "ctrl+KeyT".to_string()),
            ("gamma".to_string(), "three".to_string(), "ctrl+KeyU".to_string()),
        ],
        "成功した登録はどれも失われない"
    );
}

// ---------------------------------------------------------------------------
// 列挙順
// ---------------------------------------------------------------------------

/// 列挙は登録元 → 項目の辞書順であり、挿入順に依存しない（メニュー描画が安定する）。
#[test]
fn enumeration_order_is_deterministic_and_independent_of_insertion_order() {
    fn build(order: &[(&str, &str, &str)]) -> Vec<(String, String, String)> {
        let mut registry = AcceleratorRegistry::new();
        for (owner_id, item_id, spelling) in order {
            registry
                .insert(owner(owner_id), item(item_id), chord(spelling))
                .unwrap_or_else(|conflict| panic!("{spelling:?} の登録は成功する: {conflict}"));
        }
        listed(&registry)
    }

    let forward = [
        ("zeta", "b", "Alt+F1"),
        ("alpha", "z", "Ctrl+S"),
        ("alpha", "a", "Ctrl+A"),
        ("beta", "m", "Ctrl+B"),
    ];
    let mut backward = forward;
    backward.reverse();

    let expected = vec![
        ("alpha".to_string(), "a".to_string(), "ctrl+KeyA".to_string()),
        ("alpha".to_string(), "z".to_string(), "ctrl+KeyS".to_string()),
        ("beta".to_string(), "m".to_string(), "ctrl+KeyB".to_string()),
        ("zeta".to_string(), "b".to_string(), "alt+F1".to_string()),
    ];
    assert_eq!(build(&forward), expected);
    assert_eq!(
        build(&backward),
        expected,
        "挿入順を逆にしても列挙順は同じ"
    );
}

// ---------------------------------------------------------------------------
// 誤った綴り
// ---------------------------------------------------------------------------

/// 誤った綴りは「別のキー」として黙って受け入れず、理由付きで拒否する。構文の誤りは登録簿に
/// 入る前に落ちる（型が [`Accelerator`] だけを受け取る）。
#[test]
fn malformed_chords_are_rejected_with_a_reason() {
    assert_eq!(Accelerator::parse(""), Err(AcceleratorParseError::Empty));
    assert_eq!(Accelerator::parse("   \t "), Err(AcceleratorParseError::Empty));
    assert!(
        matches!(Accelerator::parse("Ctrl"), Err(AcceleratorParseError::UnsupportedKey { ref token, .. }) if token == "Ctrl"),
        "修飾キーだけでは組み合わせにならない"
    );
    assert!(
        matches!(Accelerator::parse("Ctrl+"), Err(AcceleratorParseError::UnsupportedKey { ref token, .. }) if token == "+"),
        "キーの無い末尾の区切りは拒否する"
    );
    assert!(
        matches!(Accelerator::parse("Ctrl++S"), Err(AcceleratorParseError::EmptyToken { .. })),
        "空のトークンは拒否する"
    );
    assert!(
        matches!(Accelerator::parse("Ctrl+NotAKey"), Err(AcceleratorParseError::UnsupportedKey { ref token, .. }) if token == "NotAKey"),
        "未知のキーは拒否する"
    );
    assert!(
        matches!(Accelerator::parse("Shift+KeyQ+Alt"), Err(AcceleratorParseError::UnknownModifier { ref token, .. }) if token == "KeyQ"),
        "キーより後ろの修飾キーは拒否する"
    );
    assert!(
        matches!(Accelerator::parse("Ctrl Alt+S"), Err(AcceleratorParseError::UnknownModifier { ref token, .. }) if token == "Ctrl Alt"),
        "トークン内部の空白は拒否する"
    );
}

/// `CmdOrCtrl` 系はプラットフォームによって `Ctrl` にも `Cmd` にもなる。独立した組み合わせとして
/// 黙って登録せず、専用のエラーで拒否する（module doc「構文の契約」）。
#[test]
fn platform_dependent_modifier_is_rejected_rather_than_treated_as_distinct() {
    for spelling in [
        "CmdOrCtrl+S",
        "cmdorctrl+s",
        "CmdOrControl+S",
        "CommandOrCtrl+S",
        "CommandOrControl+S",
    ] {
        match Accelerator::parse(spelling) {
            Err(AcceleratorParseError::PlatformDependentModifier { .. }) => {}
            other => panic!("{spelling:?} はプラットフォーム依存として拒否する: {other:?}"),
        }
    }
    // 解決済みの修飾キー（非 macOS の Ctrl、macOS の Cmd = Super）は受理する。
    assert!(Accelerator::parse("Ctrl+S").is_ok());
    assert!(Accelerator::parse("Cmd+S").is_ok());
}

// ---------------------------------------------------------------------------
// 明示的な解除と引き当て
// ---------------------------------------------------------------------------

/// 解除は明示的な操作であり、解除した登録を返す。空いた組み合わせは別の登録元が使える。
#[test]
fn remove_is_explicit_and_frees_the_chord() {
    let mut registry = AcceleratorRegistry::new();
    registry
        .insert(owner("alpha"), item("one"), chord("Ctrl+S"))
        .expect("alpha の登録は成功する");
    registry
        .insert(owner("beta"), item("two"), chord("Ctrl+A"))
        .expect("beta の登録は成功する");

    let removed = registry
        .remove(&owner("alpha"), &item("one"))
        .expect("登録済みなら返る");
    assert_eq!(removed.owner, owner("alpha"));
    assert_eq!(removed.chord, chord("Ctrl+S"));
    assert!(registry.resolve(&chord("Ctrl+S")).is_none());
    assert_eq!(registry.len(), 1);
    assert_eq!(
        registry.resolve(&chord("Ctrl+A")).map(|r| &r.owner),
        Some(&owner("beta")),
        "他の登録は影響を受けない"
    );
    assert!(registry.remove(&owner("alpha"), &item("one")).is_none());

    registry
        .insert(owner("gamma"), item("three"), chord("Ctrl+S"))
        .expect("空いた組み合わせは使える");
    assert_eq!(registry.len(), 2);
}

/// 登録されていない組み合わせの引き当ては `None`。
#[test]
fn resolve_returns_none_for_an_unregistered_chord() {
    let registry = AcceleratorRegistry::new();
    assert!(registry.is_empty());
    assert!(registry.resolve(&chord("Ctrl+S")).is_none());
}
