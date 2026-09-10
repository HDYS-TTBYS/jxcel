//! 完全性検証（タスク 4.1。要件 5.1, 5.2, 5.3。design Components 表「IntegrityVerifier」）。
//!
//! # パート 1 件単位のプリミティブ
//!
//! 本モジュールが提供するのは、パート 1 件分の実バイト列に対する 2 操作だけである:
//!
//! - [`digest_part`]: 実バイト列 → [`Blake3Digest`]（保存時に manifest へ記録する値。要件 5.1）
//! - [`verify_part`]: エントリ名 + 実バイト列 + 記録された期待ダイジェスト → 照合（要件 5.2）
//!
//! design の Requirements Traceability が `IntegrityVerifier` に与える `digest_parts` /
//! `verify_parts`（パート**集合**版）は、集合型 `crate::parts::DocumentParts` を導入する
//! タスク 4.8 の担当である: 4.8 がパートごとに本プリミティブを呼んで集合版を構成する
//! （本モジュールは集合型を先取りしない）。したがって依存も `entry_name` / `error` /
//! `ids` に留まり、`parts` 層に依存しない（design「依存方向」: 各層は左方向にのみ依存）。
//!
//! # BLAKE3 の算出経路は 1 箇所
//!
//! [`digest_part`] は [`Blake3Digest::of`]（`src/ids.rs`、タスク 1.3）へ委譲する。
//! 添付の content-addressed 識別子（要件 7.2）と同一の実装を共有し、`blake3::` を
//! 呼ぶのは本クレートでは `ids.rs` のその 1 箇所（と同ファイルのテスト内の照合 1 箇所）
//! だけである。本関数は IntegrityVerifier の名前付き入口であり、第二の算出規約ではない。
//!
//! # 対象は展開後（復号後）の実バイト列
//!
//! ダイジェストは各エントリの**内容**から算出する。照合は ZIP 復号後に行うため、
//! 圧縮後のバイト列・ZIP ヘッダ・属性は対象に含めない（同じ内容なら、コンテナ側の
//! 圧縮方式や格納順が違っても同じダイジェストになる）。
//!
//! # 照合は読み取り専用（自動修復の禁止）
//!
//! 要件 5.5 と design「自動修復の禁止」: [`verify_part`] は実バイト列を借用で受け取り、
//! 修復・上書き・再計算して書き戻す経路を提供しない。不一致は常に
//! [`DocumentError::IntegrityMismatch`]（design エラー表の応答は中止）として返り、
//! 一致しても入力を一切変更しない。状態を持たないので、同じ入力に対する結果は
//! 何度呼んでも同じである。

use crate::entry_name::EntryName;
use crate::error::DocumentError;
use crate::ids::Blake3Digest;

/// パート 1 件分の実バイト列から完全性ダイジェストを算出する（要件 5.1）。
///
/// これが manifest に記録する値そのものである。算出は [`Blake3Digest::of`] へ委譲し、
/// 添付識別子（要件 7.2）と同じ唯一の BLAKE3 経路を通る（モジュール docs 参照）。
///
/// 空のバイト列も有効なダイジェストを持つ（空であることも内容の一部であり、
/// `panic` しない）。引数は `&[u8]` で、算出は入力を借用したまま処理する
/// （[`Blake3Digest::of`] は `blake3` の一括 API へそのまま渡す）ため、数十 MB の入力でも
/// 中間バッファを確保せず、メモリ使用量はダイジェスト 32 バイト分に留まる。
/// ダイジェストを記録するのは保存経路（要件 5.1 / 8.2）なので、ここで入力の
/// 全量コピーを作らないことが保存時間の予算に効く。
#[inline]
pub fn digest_part(bytes: &[u8]) -> Blake3Digest {
    Blake3Digest::of(bytes)
}

/// パート 1 件分の実バイト列を、記録された期待ダイジェストと照合する（要件 5.2, 5.3）。
///
/// 一致すれば `Ok(())`。不一致は [`DocumentError::IntegrityMismatch`] で、その `entry`
/// には**照合したエントリ名のテキスト形**（[`EntryName`] の `Display`。例:
/// `sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl`）が入る（要件 5.3）。エントリ名は
/// [`EntryName`] で受けるため、呼び出し元が表示用に文字列化した名前を別途渡す必要はない。
///
/// 読み取り専用（要件 5.5 / design「自動修復の禁止」）: 引数はすべて借用で、修復・
/// 書き戻しの経路は無い。不一致でも入力バイト列と記録値はそのまま残り、呼び出し元が
/// 中止・報告を決める（design エラー戦略: 本クレートは提示方法を持たない）。
///
/// ```
/// use document_format::integrity::{digest_part, verify_part};
/// use document_format::{DocumentError, EntryName};
///
/// let entry = EntryName::parse("sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl").unwrap();
/// let bytes = b"{\"a\":1}\n";
///
/// // 保存側: 算出したダイジェストを記録する（要件 5.1）。
/// let recorded = digest_part(bytes);
///
/// // 読み込み側: 一致すれば Ok（要件 5.2）。
/// assert!(verify_part(&entry, bytes, recorded).is_ok());
///
/// // 1 バイトでも違えば不一致。修復はせず Err を返し、entry にはエントリ名が入る
/// // （要件 5.3, 5.5）。
/// let mut tampered = bytes.to_vec();
/// tampered[1] = b'b';
/// match verify_part(&entry, &tampered, recorded) {
///     Err(DocumentError::IntegrityMismatch { entry }) => {
///         assert_eq!(entry, "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl");
///     }
///     other => panic!("不一致が報告されない: {other:?}"),
/// }
/// ```
#[inline]
pub fn verify_part(
    entry: &EntryName,
    bytes: &[u8],
    expected: Blake3Digest,
) -> Result<(), DocumentError> {
    if digest_part(bytes) == expected {
        Ok(())
    } else {
        Err(DocumentError::IntegrityMismatch {
            entry: entry.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// BLAKE3 公式テストベクタ（`BLAKE3-team/BLAKE3` の `test_vectors/test_vectors.json`）。
    ///
    /// 入力は `byte[i] = (i % 251) as u8` で塗った `input_len` バイト列。公式ファイルの
    /// 各値は 131 バイトの拡張出力であり、「先頭 32 バイトが既定長出力に一致する」ことを
    /// 実装は検査すべきと注記されているため、ここでは先頭 32 バイト（本クレートの
    /// ダイジェスト長）だけを固定する。
    ///
    /// 実測値ではなく外部のゴールデンであることが重要: 本クレートの出力をそのまま
    /// 期待値にすると、算出そのものの誤りを検出できない。長さは 0 / 1 チャンク /
    /// 2 チャンク / 8 チャンク / 100 チャンク（BLAKE3 のチャンク長は 1024 バイト）を
    /// 跨ぎ、木構造の畳み込み経路の誤りも落ちるようにしてある。
    const OFFICIAL_VECTORS: &[(usize, &str)] = &[
        (0, "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"),
        (1024, "42214739f095a406f3fc83deb889744ac00df831c10daa55189b5d121c855af7"),
        (2048, "e776b6028c7cd22a4d0ba182a8bf62205d2ef576467e838ed6f2529b85fba24a"),
        (8192, "aae792484c8efe4f19e2ca7d371d8c467ffb10748d8a5a1ae579948f718a2a63"),
        (102400, "bc3e3d41a1146b069abffad3c0d44860cf664390afce4d9661f7902e7943e085"),
    ];

    /// 公式ベクタの入力パターン `byte[i] = (i % 251) as u8`。
    fn painted(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    /// 照合の標本: 受理 6 形のうち内容を持つ 5 形（マーカー以外）を 1 形 1 個。
    ///
    /// 横断的な性質（どの形でも `entry` に自分の名前が入る・不一致を検出する）は
    /// 対象を 2 つ以上に分散させないと回帰を検出できないため、5 形すべてを回す。
    /// バイト列は長さも内容も形ごとに異なる（定数や先頭数バイトだけを見る実装、
    /// エントリ名を固定する実装を落とす）。
    fn samples() -> Vec<(EntryName, Vec<u8>)> {
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("manifest.json", b"{\"version\":1}".to_vec()),
            (
                "document.json",
                b"{\"document_id\":\"01ARZ3NDEKTSV4RRFFQ69G5FAV\"}".to_vec(),
            ),
            (
                "schemas/01ARZ3NDEKTSV4RRFFQ69G5FAV.json",
                b"{\"root\":{}}".to_vec(),
            ),
            (
                "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.jsonl",
                b"{\"a\":1}\n{\"a\":2}\n".to_vec(),
            ),
            (
                "attachments/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef.bin",
                vec![0xff, 0x00, 0x80, 0x01],
            ),
        ];
        cases
            .into_iter()
            .map(|(text, bytes)| (EntryName::parse(text).expect("標本は許可リスト内"), bytes))
            .collect()
    }

    /// 不一致エラーの `entry`（テキスト形）を取り出す。`DocumentError` は
    /// `PartialEq` ではない（`Io` 変種が `std::io::Error` を持つ）ため、比較は
    /// この取り出しを経由する。
    fn mismatch_entry(err: DocumentError) -> String {
        match err {
            DocumentError::IntegrityMismatch { entry } => entry,
            other => panic!("IntegrityMismatch 以外が返った: {other:?}"),
        }
    }

    /// 要件 5.1: 算出が BLAKE3 そのものであること。公式ベクタと一致しなければ、
    /// アルゴリズムの取り違え・切り詰め・入力の一部だけの算出がすべて露見する。
    #[test]
    fn digest_matches_official_blake3_vectors() {
        for &(len, expected) in OFFICIAL_VECTORS {
            let expected = Blake3Digest::from_hex(expected).expect("公式ベクタは 64 文字 hex");
            assert_eq!(
                expected,
                digest_part(&painted(len)),
                "BLAKE3 公式ベクタと不一致: input_len={len}"
            );
        }
    }

    /// 要件 5.1: 内容が同じなら同じ値、1 バイトでも違えば違う値。空列・NUL 含み・
    /// 非 UTF-8 も内容として扱う（解釈も正規化もしない）。
    #[test]
    fn digest_is_content_addressed_and_sensitive_to_every_byte() {
        let empty: &[u8] = &[];
        let cases: &[&[u8]] = &[
            empty,
            &[0x00],
            &[0x00, 0x00],
            b"jxcel",
            &[0xff, 0x00, 0x80, 0xfe],
            &[0x01, 0x02, 0x03],
        ];

        // 同一バイト列 → 同一ダイジェスト（ダイジェスト長は常に 32 バイト）。
        for bytes in cases {
            let digest = digest_part(bytes);
            assert_eq!(digest, digest_part(bytes), "同一バイト列で結果が変わる");
            assert_eq!(
                Blake3Digest::LEN * 2,
                digest.to_hex().len(),
                "ダイジェストのテキスト形が 64 文字でない"
            );
        }

        // 全標本が相異なる（定数・切り詰め・先頭数バイトだけの算出を落とす）。
        for (i, left) in cases.iter().enumerate() {
            for right in &cases[i + 1..] {
                assert_ne!(
                    digest_part(left),
                    digest_part(right),
                    "異なる内容が同じダイジェストになった: {left:?} / {right:?}"
                );
            }
        }

        // 1 バイト差（末尾・中程・非 UTF-8）をすべて検出する。
        assert_ne!(digest_part(&[0x01, 0x02, 0x03]), digest_part(&[0x01, 0x02, 0x04]));
        assert_ne!(digest_part(&[0x00, 0x00]), digest_part(&[0x00, 0x01]));
        assert_ne!(digest_part(&[0xff]), digest_part(&[0xfe]));
        assert_ne!(digest_part(empty), digest_part(&[0x00]), "空列と 1 バイトが同一");
    }

    /// 要件 5.2: 記録値と実際の内容が一致すれば通る（5 形すべて）。
    #[test]
    fn verify_accepts_matching_digest() {
        for (entry, bytes) in samples() {
            let recorded = digest_part(&bytes);
            let result = verify_part(&entry, &bytes, recorded);
            assert!(result.is_ok(), "一致時に Ok(()) にならない: {entry}: {result:?}");
        }
    }

    /// 要件 5.3: 不一致の `entry` には**照合した対象のエントリ名**（テキスト形）が入る。
    /// 5 形を回し、固定名・空文字・別エントリ名を返す実装を落とす。
    #[test]
    fn verify_reports_the_verified_entry_name_on_mismatch() {
        let samples = samples();
        let mut reported = Vec::new();
        for (entry, bytes) in &samples {
            // 内容に見合わない記録値（別パートのダイジェスト）。
            let wrong = digest_part(b"recorded for another part");
            let text = mismatch_entry(
                verify_part(entry, bytes, wrong).expect_err("不一致は Err でなければならない"),
            );
            assert_eq!(
                entry.to_string(),
                text,
                "不一致エラーの entry が照合したエントリ名のテキスト形でない"
            );
            assert!(!text.is_empty(), "entry が空になっている");
            reported.push(text);
        }

        // 標本のエントリ形は互いに異なる（固定名の検出）。網羅性の回帰を検出するには
        // 対象が 2 つ以上必要なので、それも固定する。
        assert!(reported.len() >= 2, "エントリ形が 2 つ以上ないと回帰を検出できない");
        for (i, left) in reported.iter().enumerate() {
            for right in &reported[i + 1..] {
                assert_ne!(left, right, "異なるエントリ名が同じ entry として報告された");
            }
        }
    }

    /// 要件 5.2: 1 バイトの改変でも不一致になる。長さが同じでも内容が違えば落ちる
    /// （サイズだけを比較する実装を落とす）ことを、先頭・中央・末尾で確かめる。
    #[test]
    fn verify_detects_single_byte_tampering() {
        for (entry, bytes) in samples() {
            let recorded = digest_part(&bytes);
            let last = bytes.len() - 1;
            for index in [0, bytes.len() / 2, last] {
                let mut tampered = bytes.clone();
                tampered[index] ^= 0x01;
                let err = verify_part(&entry, &tampered, recorded)
                    .expect_err("1 バイトの改変が検出されない");
                assert_eq!(entry.to_string(), mismatch_entry(err));
            }
        }
    }

    /// 要件 5.2: 記録値側の 1 バイト違いも不一致として検出する。
    ///
    /// 照合対象は manifest に hex で記録された値であり、転記・解析の誤りは
    /// ダイジェストの任意の 1 バイトに現れる。期待値の先頭だけを見る実装、
    /// 末尾だけを見る実装、長さ（32 バイト固定）だけを見る実装を落とすため、
    /// 先頭・2 番目・中央・末尾の 4 箇所を回す。
    #[test]
    fn verify_detects_tampering_in_the_recorded_digest() {
        for (entry, bytes) in samples() {
            let recorded = digest_part(&bytes);
            for index in [0, 1, Blake3Digest::LEN / 2, Blake3Digest::LEN - 1] {
                let mut wrong = *recorded.as_bytes();
                wrong[index] ^= 0x01;
                let err = verify_part(&entry, &bytes, Blake3Digest::from_bytes(wrong))
                    .expect_err("記録値の 1 バイト違いが検出されない");
                assert_eq!(entry.to_string(), mismatch_entry(err));
            }
        }
    }

    /// 要件 5.5: 照合は読み取り専用である。2 回呼んでも同じ結果、入力バイト列と
    /// 記録値は変化しない（修復・書き戻し・再計算の経路が無い）。
    #[test]
    fn verify_is_read_only_and_repeatable() {
        for (entry, bytes) in samples() {
            let recorded = digest_part(&bytes);
            assert!(verify_part(&entry, &bytes, recorded).is_ok(), "一致する入力で Ok にならない: {entry}");
            assert!(
                verify_part(&entry, &bytes, recorded).is_ok(),
                "同じ一致入力の 2 回目の照合で結果が変わる: {entry}"
            );

            // 改変した入力でも、2 回の照合が同じ entry を報告し、入力は元のまま。
            let before = bytes.clone();
            let mut tampered = bytes.clone();
            tampered[0] ^= 0x01;
            let submitted = tampered.clone();
            let first = mismatch_entry(verify_part(&entry, &tampered, recorded).unwrap_err());
            let second = mismatch_entry(verify_part(&entry, &tampered, recorded).unwrap_err());
            assert_eq!(first, second, "同じ不一致入力で報告内容が変わる");
            assert_eq!(before, bytes, "照合が入力バイト列を書き換えた");
            assert_eq!(submitted, tampered, "照合が照合対象を書き換えた");
            assert_eq!(recorded, digest_part(&bytes), "記録値が再計算で上書きされた");
        }
    }

    /// 要件 5.1 / 8.x: 数十 MB 級でも正しく動く。BLAKE3 は 1024 バイトのチャンク単位で
    /// 処理するため、8 MiB（= 8192 チャンク）で先頭・中央・末尾のどの 1 バイト改変も
    /// 検出できることを確かめる（先頭チャンクだけ／末尾チャンクだけを計算する実装を落とす）。
    #[test]
    fn digest_handles_large_input() {
        const LEN: usize = 8 * 1024 * 1024;
        let bytes = painted(LEN);
        let digest = digest_part(&bytes);

        assert_eq!(digest, digest_part(&painted(LEN)), "同一バイト列で結果が変わる");
        for index in [0, LEN / 2, LEN - 1] {
            let mut changed = painted(LEN);
            changed[index] ^= 0x01;
            assert_ne!(
                digest,
                digest_part(&changed),
                "大入力の改変位置 {index} を検出できない"
            );
        }
    }
}
