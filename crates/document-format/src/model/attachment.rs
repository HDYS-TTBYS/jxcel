//! 添付の保持と参照集計(タスク 2.3。要件 7.1, 7.5, 7.6)。
//!
//! # 不透明なバイト列(要件 7.1, 7.5)
//!
//! [`Attachment`] は識別子と**任意のバイト列**だけを持つ。本クレートは内容を解釈・
//! 変換・再圧縮・検証しない(design「Out of Boundary: 添付の内容の解釈」)。空列、
//! 非 UTF-8、NUL を含む列、既に圧縮された列もそのまま保持され、取り出しで 1 バイトも
//! 変わらない(テスト `attachment_bytes_survive_holding_and_retrieval_unchanged`)。
//! `String` を経由しないため、内容がテキストであることを要求する経路は無い。
//!
//! # content-addressed で冪等(要件 7.2)
//!
//! 識別子は [`AttachmentId::from_bytes`] が内容の BLAKE3 ダイジェストから決めるため、
//! 発行者を必要としない(タスク 1.3)。同一バイト列の再登録は同一識別子を返し、
//! エントリを重複させない。登録順は識別子に影響しない。
//!
//! # 反復順は識別子で決まる(要件 3.6)
//!
//! レジストリの内部表現は [`BTreeMap`](std::collections::BTreeMap) であり、反復順は
//! [`AttachmentId`] の昇順である。`HashMap` は反復順が実行ごとに変わり決定性を壊すため
//! 使わない(value.rs と同じ禁止理由。登録順にも依存しない)。
//!
//! # 参照集計(要件 7.3 の帰結)
//!
//! セル値の列から [`CellValue::Attachment`] を再帰的に集める。走査規則(たどるのは
//! `Nested` の内側だけ、参照として数えるのは `CellValue::Attachment` だけ)の実装は
//! **本モジュールではなく** [`crate::value::visit_attachment_references`] が持つ:
//! `parts` 層の [`crate::parts::PartInventory::declare_attachment_refs`] と同じ規則であり、
//! 規則を 2 箇所に置くと [`crate::value::NestedValue`] に変種が増えたときに片方だけを
//! 直して黙って漏れる(コンパイルで強制されない)。両層が依存する最下層 `crate::value` に
//! 1 つだけ置く。参照が実在するかの検証(要件 7.4 の `DanglingAttachmentRef`)は読み込み
//! 経路 `StructuralValidator`(タスク 4.7)の責務で、本モジュールは検証も報告もしない。
//!
//! # 未参照の添付を削除しない(要件 7.6)
//!
//! [`AttachmentRegistry::unreferenced_attachments`] は未参照の識別子を昇順の [`Vec`] で
//! 返すだけで、レジストリを一切変更しない。**削除の API は存在しない**: 「自動的に
//! 削除しない」は削除経路が無いという構造で示す(列挙した後に
//! [`AttachmentRegistry::get`] で取得できることをテストで確認する)。
//!
//! # Clone を実装しない理由
//!
//! [`Attachment`] / [`AttachmentRegistry`] は `Clone` を実装しない。集約ルート
//! [`Document`](super::Document) は添付を保持して `Clone` を持たず、添付は `Document` の
//! 外で独立に存在しない(design「Domain Model」)。clone 方針が未定の型を公開すると、
//! 集約ルートの保証を黙って壊す経路を作る(model/sheet.rs の「Clone を実装しない理由」
//! と同じ判断)。

use std::collections::{BTreeMap, HashSet};

use crate::ids::AttachmentId;
use crate::value::{visit_attachment_references, CellValue};

/// 添付 1 件: content-addressed 識別子と不透明なバイト列(要件 7.1, 7.5)。
///
/// バイト列は保持したまま取り出すだけの値であり、本クレートは内容を解釈・変換・
/// 再圧縮しない。識別子はバイト列から決まり(要件 7.2)、構築後に変わらない。
#[derive(Debug)]
pub struct Attachment {
    id: AttachmentId,
    bytes: Vec<u8>,
}

impl Attachment {
    /// この添付の content-addressed 識別子(内容の BLAKE3。要件 7.2)。
    #[inline]
    pub fn id(&self) -> AttachmentId {
        self.id
    }

    /// 保持しているバイト列そのもの(登録時に渡されたバイト列と 1 バイトも変わらない)。
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// 添付の保持と参照集計(design Components 表 `AttachmentRegistry`。要件 7.1, 7.5, 7.6)。
///
/// 文書内の添付はこの型が一手に保持し、[`Document`](super::Document) が集約ルートとして
/// 所有する(design ER 図 `Document ||--o{ Attachment : holds`)。添付は `Document` の外で
/// 独立に存在しない。
///
/// 内部表現は [`BTreeMap`](std::collections::BTreeMap) である: 反復順が [`AttachmentId`] の
/// 昇順に決まり、登録順にも実行環境にも依存しない(要件 3.6 の精神)。`HashMap` は
/// 反復順が観測される集合に使わない(value.rs と同じ禁止理由)。
#[derive(Debug, Default)]
pub struct AttachmentRegistry {
    /// 識別子 → 添付。キー順がそのまま決定的な反復順になる。
    entries: BTreeMap<AttachmentId, Attachment>,
}

impl AttachmentRegistry {
    /// 空のレジストリを作る。
    #[inline]
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// バイト列を添付として登録し、その識別子を返す(要件 7.1, 7.2)。
    ///
    /// 識別子は [`AttachmentId::from_bytes`] が内容から決めるため**冪等**である:
    /// 同一バイト列の再登録は同一識別子を返し、エントリを重複させない(既存の
    /// エントリをそのまま残す)。バイト列は解釈・変換・再圧縮せずそのまま保持する。
    pub fn add(&mut self, bytes: Vec<u8>) -> AttachmentId {
        let id = AttachmentId::from_bytes(&bytes);
        self.entries
            .entry(id)
            .or_insert_with(|| Attachment { id, bytes });
        id
    }

    /// 識別子で添付を引く。未登録なら `None`。
    #[inline]
    pub fn get(&self, id: AttachmentId) -> Option<&Attachment> {
        self.entries.get(&id)
    }

    /// [`AttachmentId`] の昇順で全添付を反復する(登録順に依存しない)。
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Attachment> {
        self.entries.values()
    }

    /// 保持している添付の件数。
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 添付を 1 件も保持していないか。
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 渡されたセル値の列から**参照されていない**添付の識別子を昇順で返す(要件 7.6)。
    ///
    /// 参照は [`CellValue::Attachment`] を [`NestedValue`](crate::value::NestedValue) の
    /// Object の値と Array の要素へ再帰的にたどって集める(規則の実装は
    /// [`visit_attachment_references`] が唯一の場所。モジュール docs「参照集計」)。
    /// **集計は読み取り専用**で、レジストリを削除も書き換えもしない(削除の API は存在
    /// しない)。未登録の識別子を参照していても失敗しない(実在検証はタスク 4.7 の責務)。
    ///
    /// 返り値の順序は [`AttachmentId`] の昇順であり、シート・行の順序や添付の登録順に
    /// 依存しない。参照済みの識別子は登録済みか否かに関わらず結果に影響しない。
    pub fn unreferenced_attachments<'a>(
        &self,
        values: impl IntoIterator<Item = &'a CellValue>,
    ) -> Vec<AttachmentId> {
        // 参照済みの識別子は**所属判定にしか使わない**(反復しない)。結果の順序は
        // レジストリのキー順(BTreeMap)だけで決まるため、ここでの集合の反復順は
        // 観測されない。
        let mut referenced: HashSet<AttachmentId> = HashSet::new();
        for value in values {
            visit_attachment_references(value, &mut |id| {
                referenced.insert(id);
            });
        }
        self.entries
            .keys()
            .copied()
            .filter(|id| !referenced.contains(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{Attachment, AttachmentRegistry};
    use crate::ids::AttachmentId;
    use crate::value::{CellValue, NestedValue};

    /// 任意のバイト列: 空・非 UTF-8・NUL を含む列・既圧縮データ・全バイト値。
    fn opaque_payloads() -> Vec<Vec<u8>> {
        vec![
            Vec::new(),
            vec![0xff, 0xfe, 0x00, 0x80],
            b"\x00\x01binary\x00\x00".to_vec(),
            // gzip のマジックで始まる(deflate ストリームに見える)既圧縮データ。
            // 再圧縮すれば必ず別バイト列になる並び。
            vec![
                0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x9c, 0x3d,
            ],
            (0..=255u8).collect(),
        ]
    }

    #[test]
    fn attachment_bytes_survive_holding_and_retrieval_unchanged() {
        // 要件 7.1 / 7.5: 解釈・変換・再圧縮せずに保持し、取り出しで 1 バイトも変えない。
        let payloads = opaque_payloads();
        let mut registry = AttachmentRegistry::new();
        for payload in &payloads {
            let id = registry.add(payload.clone());
            let stored = registry.get(id).expect("登録直後の添付は取得できる");
            assert_eq!(
                payload,
                stored.bytes(),
                "保持と取り出しで 1 バイトも変わってはならない"
            );
            assert_eq!(payload.len(), stored.bytes().len());
            assert_eq!(
                id,
                stored.id(),
                "取得した添付の識別子は登録時の識別子と同一"
            );
            // 識別子は内容の BLAKE3(要件 7.2)。内容を解釈していないことの帰結である。
            assert_eq!(AttachmentId::from_bytes(payload), id);
        }
        assert_eq!(
            payloads.len(),
            registry.len(),
            "異なる内容は別エントリになる"
        );

        // 内容を注視しないことの証拠: 非 UTF-8 を含む列がそのまま成功している。
        // 文字列として解釈する実装なら、ここで拒否か置換が起きる。
        assert!(
            payloads.iter().any(|p| std::str::from_utf8(p).is_err()),
            "非 UTF-8 の検体が無ければこの検査は意味を持たない"
        );
    }

    #[test]
    fn registering_the_same_bytes_twice_is_idempotent() {
        // 要件 7.2: 同一内容は同一識別子。エントリを重複させない。
        let payload = b"\x00\xffduplicate".to_vec();
        let mut registry = AttachmentRegistry::new();
        let first = registry.add(payload.clone());
        let second = registry.add(payload.clone());
        assert_eq!(first, second, "同一バイト列の再登録は同一識別子を返す");
        assert_eq!(1, registry.len(), "エントリは重複しない");
        assert_eq!(1, registry.iter().count());
        assert_eq!(payload, registry.get(first).unwrap().bytes());

        // 異なるバイト列は異なる識別子・別エントリ。
        let other = registry.add(b"\x00\xffduplicate!".to_vec());
        assert_ne!(first, other);
        assert_eq!(2, registry.len());
    }

    #[test]
    fn registration_order_does_not_change_the_entries() {
        // 識別子は内容から決まる純関数であり、登録順に依存しない。
        let payloads = opaque_payloads();
        let mut forward = AttachmentRegistry::new();
        let mut backward = AttachmentRegistry::new();
        let forward_ids: Vec<AttachmentId> =
            payloads.iter().cloned().map(|p| forward.add(p)).collect();
        let backward_ids: Vec<AttachmentId> = payloads
            .iter()
            .rev()
            .cloned()
            .map(|p| backward.add(p))
            .collect();

        let mut sorted_forward = forward_ids.clone();
        let mut sorted_backward = backward_ids.clone();
        sorted_forward.sort();
        sorted_backward.sort();
        assert_eq!(
            sorted_forward, sorted_backward,
            "同じ内容集合なら登録順に関わらず同じ識別子集合"
        );
        // 内容 → 識別子の写像そのものが登録順に依存しない(逆順登録の逆順と一致する)。
        for (forward_id, backward_id) in forward_ids.iter().zip(backward_ids.iter().rev()) {
            assert_eq!(forward_id, backward_id, "同じバイト列は常に同じ識別子");
        }
        assert_eq!(
            forward.iter().map(Attachment::id).collect::<Vec<_>>(),
            backward.iter().map(Attachment::id).collect::<Vec<_>>(),
            "登録順が違っても反復順は同一"
        );
    }

    #[test]
    fn iteration_is_ascending_by_attachment_id() {
        // 要件 3.6 の精神: 反復順は識別子で決まり、登録順に依存しない。
        let payloads = opaque_payloads();
        let mut registry = AttachmentRegistry::new();
        for payload in &payloads {
            registry.add(payload.clone());
        }
        let observed: Vec<AttachmentId> = registry.iter().map(Attachment::id).collect();
        let mut expected = observed.clone();
        expected.sort();
        assert_eq!(expected, observed, "AttachmentId の昇順で反復する");
        assert_eq!(
            payloads.len(),
            observed.len(),
            "全エントリがちょうど 1 回ずつ現れる"
        );
    }

    #[test]
    fn unreferenced_attachments_are_listed_in_ascending_order_and_kept() {
        // 要件 7.6: 未参照の添付を削除せず、未参照として一覧できる。
        // 「自動的に削除しない」は削除経路の不在(この型に削除の API が無いこと)として
        // 構造で示す。ここでは一覧した後も登録済みのまま取得できることを確かめる。
        let payloads: Vec<Vec<u8>> = vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()];
        let mut registry = AttachmentRegistry::new();
        let ids: Vec<AttachmentId> = payloads.iter().cloned().map(|p| registry.add(p)).collect();

        // どのセル値からも参照されていない。
        let unreferenced = registry.unreferenced_attachments(std::iter::empty::<&CellValue>());
        let mut expected = ids.clone();
        expected.sort();
        assert_eq!(
            expected, unreferenced,
            "未参照の添付を AttachmentId 昇順で一覧する"
        );

        // 一覧した後も登録済みのまま取得できる(削除されない)。
        for (id, payload) in ids.iter().zip(&payloads) {
            let stored = registry.get(*id).expect("未参照でも削除されない");
            assert_eq!(payload, stored.bytes());
        }
        assert_eq!(
            payloads.len(),
            registry.len(),
            "集計はレジストリを変更しない"
        );
        assert_eq!(
            unreferenced,
            registry.unreferenced_attachments(std::iter::empty::<&CellValue>()),
            "繰り返し呼んでも同じ結果(冪等)"
        );

        // 登録順を逆にしたレジストリでも一覧は同一(順序は識別子で決まる)。
        let mut reversed = AttachmentRegistry::new();
        for payload in payloads.iter().rev() {
            reversed.add(payload.clone());
        }
        assert_eq!(
            unreferenced,
            reversed.unreferenced_attachments(std::iter::empty::<&CellValue>()),
            "登録順を変えても一覧は同一"
        );
    }

    #[test]
    fn references_are_deeply_collected_from_nested_values() {
        // 要件 7.3 の帰結: セル値の奥深くの参照も参照済みとして集計される。
        let mut registry = AttachmentRegistry::new();
        let in_array = registry.add(b"array-element".to_vec());
        let in_object = registry.add(b"object-value".to_vec());
        let deep = registry.add(b"deep".to_vec());
        let orphan = registry.add(b"orphan".to_vec());

        let values = [
            CellValue::Nested(NestedValue::Object(vec![
                (
                    "ptr".to_string(),
                    CellValue::Nested(NestedValue::Array(vec![
                        CellValue::Null,
                        CellValue::Bool(true),
                        CellValue::Nested(NestedValue::Object(vec![(
                            "deep".to_string(),
                            CellValue::Attachment(deep),
                        )])),
                        // 同じ添付への 2 回目の参照(重複しても 1 件として扱う)。
                        CellValue::Attachment(in_array),
                    ])),
                ),
                (
                    "list".to_string(),
                    CellValue::Nested(NestedValue::Array(vec![CellValue::Attachment(in_array)])),
                ),
            ])),
            CellValue::Attachment(in_object),
        ];

        assert_eq!(
            vec![orphan],
            registry.unreferenced_attachments(values.iter()),
            "参照済みは現れず、未参照だけが昇順で残る"
        );
        for referenced in [in_array, in_object, deep] {
            assert!(
                registry.get(referenced).is_some(),
                "参照済みの添付も削除されない"
            );
        }
    }

    #[test]
    fn only_attachment_cells_count_as_references() {
        // 参照は `CellValue::Attachment` だけである: 同じ hex を内容に持つ
        // `Text` / `Decimal` や、オブジェクトのキーは参照ではない(値だけをたどる)。
        let mut registry = AttachmentRegistry::new();
        let id = registry.add(b"payload".to_vec());
        let hex = id.to_hex();
        let values = [
            CellValue::Text(hex.clone()),
            CellValue::Decimal(hex.clone()),
            CellValue::Nested(NestedValue::Object(vec![(hex.clone(), CellValue::Null)])),
            CellValue::Nested(NestedValue::Array(vec![CellValue::Text(hex)])),
        ];
        assert_eq!(vec![id], registry.unreferenced_attachments(values.iter()));
    }

    #[test]
    fn dangling_references_are_not_the_registrys_role() {
        // 実在しない参照の報告(要件 7.4)は StructuralValidator(タスク 4.7)の責務で、
        // 本モジュールは検証しない。集計は登録済みの集合だけを見る(panic しない)。
        let mut registry = AttachmentRegistry::new();
        let stored = registry.add(b"stored".to_vec());
        let stranger = AttachmentId::from_bytes(b"never registered");
        let values = [CellValue::Attachment(stranger)];
        assert_eq!(
            vec![stored],
            registry.unreferenced_attachments(values.iter())
        );
    }
}
