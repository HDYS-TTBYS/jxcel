//! 不正アーカイブの拒否（タスク 8.5。要件 2.5, 2.6。design「Container Layer / EntryLayout」、
//! 「Container Layer / ContainerCodec」、「Security Considerations / zip-slip・リソース枯渇」、
//! 「Testing Strategy / Integration Tests（不正アーカイブ）」）。
//!
//! design のテスト項目「許可リスト外のエントリ名、同一パスの重複エントリを持つ ZIP が拒否される
//! こと（2.5, 2.6）」と、タスク記述の「極端に膨張するアーカイブが拒否されること」を実装する。
//! 観測するのは **`zip` クレートで生に組み立てたアーカイブ**を、**`ContainerCodec::decode`
//! （バイト列経路）と `DocumentFormatApi::open`（実ファイル経路）の両方**へ通したときの拒否である。
//! 各シナリオには **正しいアーカイブが受理される対照**を併置する（拒否テストが「何でも拒否して
//! いるだけ」でないことを示す。8.4 と同じ作法）。
//!
//! # 極端に膨張するアーカイブをどう検証するか（**絶対的なサイズ上限は検証しない**）
//!
//! 本実装は**バイト数のしきい値による拒否を持たない**。これは意図的な非実装である:
//! 要件 8.5 が「10 万行を超えても読み込みを拒否せず、性能保証の対象外である旨を通知する」と
//! 定めており、正当に大きなドキュメントを拒否できない。加えて DEFLATE の圧縮比には理論上限が
//! あり（最良でも約 1032:1）、比による判定も実効的な防御にならない（task 5.3 の裁定。
//! `src/container/reader.rs` の「絶対的なサイズ上限を設けない理由」）。
//!
//! したがって「極端に膨張するアーカイブの拒否」は、次の多層として実測する:
//!
//! * **(a) 許可リスト照合と重複検出が展開に先行すること**:
//!   [`the_allow_list_is_decided_before_any_entry_is_expanded`] が、許可リスト違反のエントリと
//!   宣言サイズを偽った（展開すれば失敗する）エントリを同時に持つ入力で、返るエラーが**展開側
//!   ではなく許可リスト側**であることを固定する。
//! * **(b) 宣言サイズを上限とした有界読み（`take(宣言 + 1)`）**: 実際は 64 KiB へ膨張する
//!   エントリに 100 バイトと宣言させ、報告される展開長が**宣言 + 1 の 101** で止まることを
//!   固定する（無制限に `read_to_end` していれば 65536 が報告される。
//!   [`a_declared_size_smaller_than_the_expansion_is_rejected_with_bounded_reading`]）。
//! * **(c) 展開長が宣言と一致することの要求**: 宣言が実際より**小さい**場合（上記）と
//!   **大きい**場合（[`a_declared_size_larger_than_the_expansion_is_rejected`]）の両方向で拒否する。
//!
//! つまり「極端に膨張するアーカイブ」の実体は、**実際の展開長を偽って宣言するアーカイブ**を
//! 実際に組み立てて拒否を観測する形で固定する（絶対上限の不在そのものは検証対象にしない）。
//!
//! # アーカイブの組み立て方（生バイトを壊さない）
//!
//! 許可リスト外の名前・重複パスは本クレートの符号化では作れないため、`zip::write::ZipWriter`
//! で**任意の名前のエントリをそのまま**書く（`tests/corruption.rs` の作り方と同じ）。
//! ただし `ZipWriter` は同一名の重複を拒否するため、重複パスは正しい ZIP を組んだ後に
//! エントリ名を書き換えて作る。宣言サイズの偽装も `ZipWriter` では作れない（正しい値しか
//! 書かない）ため、組み立てた後に**中央ディレクトリの固定ヘッダ内の非圧縮サイズ欄 4 バイト
//! だけ**を書き換える（[`patch_declared_size`]）。いずれもデータと CRC には触れない: ZIP の
//! データを壊すと `zip` の CRC 検証が本層の照合より先に落ち、検証したい経路へ到達しない。
//!
//! # 非ゴール
//!
//! 破損の検出（8.4。重複させない）、中断耐性（8.6）、移行 fixture（8.7）、添付（8.8）、
//! ベンチ（8.9）。絶対的なサイズ上限の導入可否（要件 8.5 との衝突の解消が前提。5.3 の申し送り）。
//!
//! 標本・一時ディレクトリ・生ヘッダ解析は `tests/common/mod.rs` を再利用する
//! （第二の標本・比較規約・ヘッダ解析を作らない）。

mod common;

use std::fs;
use std::io::{Cursor, Write};

use document_format::container::writer::marker_bytes;
use document_format::container::ContainerCodec;
use document_format::parts::to_parts;
use document_format::{Document, DocumentError, DocumentFormatApi, EntryName};
use zip::write::{SimpleFileOptions, ZipWriter};
use zip::CompressionMethod;

use common::{
    api, assert_same_document, central_headers, entries_of, local_headers, sample, Scratch,
};

/// 標本の `document.json` が膨張する先（64 KiB。Deflate が桁違いに縮める分量）。
const EXPANDED_SIZE: usize = 1 << 16;

/// ZIP のエントリ 1 件（名前は許可リスト外も含め任意。`ZipWriter` がそのまま書く）。
type RawEntry = (String, Vec<u8>, CompressionMethod);

/// 与えられたエントリを順に書いた ZIP を組み立てる（`tests/corruption.rs` と同じ作法）。
///
/// 名前・圧縮方式・本数のいずれも読み込み側の要求ではないため、テストはここで
/// 検証したい構成だけを与える。`ZipWriter` は名前の文法を検査しないので、許可リスト外の
/// 名前・NUL を含む名前・ディレクトリエントリをそのまま書ける。**同一名の重複だけは
/// `ZipWriter` 自身が拒否する**ため、重複パスのアーカイブは正しい ZIP を組んだ後に名前を
/// 書き換えて作る（[`patch_entry_name`]）。
fn zip_bytes(entries: Vec<RawEntry>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes, method) in entries {
        writer
            .start_file(
                name,
                SimpleFileOptions::default().compression_method(method),
            )
            .expect("エントリを開始できる");
        writer.write_all(&bytes).expect("エントリを書ける");
    }
    writer.finish().expect("ZIP を完成できる").into_inner()
}

/// 標本の正しいエントリ（型マーカー + 全パート）を ZIP 用の列へ写す。
///
/// 型マーカーは書き出し側と同じく無圧縮、パートは Deflate にする（位置も圧縮方式も
/// 読み込み側は要求しない。対照の組み立てを本番の符号化に近づけるためだけである）。
/// `document` を受け取るのは、識別子（ULID）が内容の一部であり、組み立てと比較で
/// **同じ 1 つの文書**を使わないと対照が別の文書を比べてしまうためである。
fn document_entries(document: &Document) -> Vec<RawEntry> {
    let parts = to_parts(document).expect("標本はパート集合へ取り出せる");
    let mut entries: Vec<RawEntry> = vec![(
        EntryName::Marker.to_string(),
        marker_bytes(parts.format_version()),
        CompressionMethod::Stored,
    )];
    entries.extend(
        entries_of(&parts)
            .into_iter()
            .map(|(name, bytes)| (name.to_string(), bytes, CompressionMethod::Deflated)),
    );
    entries
}

/// 同じ組み立て方で作った正しいアーカイブ（対照。`decode` / `open` が受理する）。
fn valid_container(document: &Document) -> Vec<u8> {
    zip_bytes(document_entries(document))
}

/// `len` バイトへ展開する、Deflate で桁違いに縮む反復バイト列。
fn expanding_bytes(len: usize) -> Vec<u8> {
    const PATTERN: &[u8] = b"jxcel malicious archive: extreme expansion probe. ";
    PATTERN.iter().copied().cycle().take(len).collect()
}

/// 標本の正しいエントリ集合の `document.json` を、`expand_to` バイトへ膨張する内容へ差し替える。
///
/// 内容は JSON として不正になり得るが、本ファイルの拒否シナリオはコンテナ層で止まるため
/// パート復号へ到達しない（対照では差し替えない）。
fn with_expanding_document(entries: &mut [RawEntry], expand_to: usize) {
    let slot = entries
        .iter()
        .position(|(name, _, _)| name == "document.json")
        .expect("標本は document.json を持つ");
    entries[slot].1 = expanding_bytes(expand_to);
}

/// 宣言サイズを偽った `document.json` を持つアーカイブを組む。
///
/// `invalid_names` は許可リスト外の名前を先頭側へ足す（判定順の実測用。空なら正しい名前だけ）。
fn forged_size_container(
    document: &Document,
    expand_to: usize,
    declared: u32,
    invalid_names: &[&str],
) -> Vec<u8> {
    let mut entries = document_entries(document);
    with_expanding_document(&mut entries, expand_to);
    for name in invalid_names {
        entries.insert(
            0,
            (name.to_string(), b"{}".to_vec(), CompressionMethod::Stored),
        );
    }
    let mut bytes = zip_bytes(entries);
    patch_declared_size(&mut bytes, "document.json", declared);
    bytes
}

/// 中央ディレクトリとローカルヘッダのエントリ名 `from` を `to` へ書き換える。
///
/// `ZipWriter` は同一名のエントリを拒否する（`InvalidArchive("Duplicate filename: ...")`）
/// ため、重複パスのアーカイブは正しい ZIP を組んだ後に名前だけを書き換えて作る。
/// `from` と `to` は同じバイト長でなければならない（長さ欄を書き換えないため）。
fn patch_entry_name(bytes: &mut [u8], from: &str, to: &str) {
    assert_eq!(
        from.len(),
        to.len(),
        "名前の長さを変えない（長さ欄を書き換えないため）"
    );

    let central = central_headers(bytes)
        .into_iter()
        .find(|header| header.name == from)
        .unwrap_or_else(|| panic!("中央ディレクトリに {from} が無い"));
    // ローカルヘッダは名前の直後（固定長 30 バイト）から本体が始まる。本テストの ZIP は
    // 拡張フィールドを持たないため、本体開始位置から逆算できる。
    let local = local_headers(bytes)
        .into_iter()
        .find(|header| header.name == from)
        .unwrap_or_else(|| panic!("ローカルヘッダに {from} が無い"));
    let local_start = local.data_start - 30 - from.len();
    assert_eq!(b"PK\x03\x04", &bytes[local_start..local_start + 4]);

    bytes[central.header_start + 46..central.header_start + 46 + to.len()]
        .copy_from_slice(to.as_bytes());
    bytes[local_start + 30..local_start + 30 + to.len()].copy_from_slice(to.as_bytes());
}

/// 中央ディレクトリの `name` エントリの非圧縮サイズ（宣言サイズ）欄だけを書き換える。
///
/// `zip` の読み手が `ZipFile::size()` として読むのは中央ディレクトリの値である
/// （`src/container/reader.rs` の「宣言サイズの出所」）。データと CRC には触れない。
fn patch_declared_size(bytes: &mut [u8], name: &str, size: u32) {
    const DECLARED_SIZE_OFFSET: usize = 24;
    let header = central_headers(bytes)
        .into_iter()
        .find(|header| header.name == name)
        .unwrap_or_else(|| panic!("中央ディレクトリに {name} が無い"));
    let at = header.header_start + DECLARED_SIZE_OFFSET;
    bytes[at..at + 4].copy_from_slice(&size.to_le_bytes());
}

/// 本物の中央ディレクトリと EOCD の間に、中央ディレクトリのヘッダに似せたレコードを挿入する。
///
/// EOCD は挿入前と同じ位置の中央ディレクトリ（先頭からの絶対オフセット）を指すため、
/// `zip` の読み手は本物のエントリだけを読む（挿入したレコードは本物のディレクトリの
/// 後ろにあるため読まれない）。`src/container/reader.rs` の生バイト走査は `PK\x01\x02` の
/// 並びを EOCD に当たるまで辿るので、このレコードを**存在しないエントリとして読んでしまう**。
fn inject_central_directory_lookalike(bytes: &[u8], name: &str) -> Vec<u8> {
    let eocd = bytes
        .windows(4)
        .rposition(|window| window == b"PK\x05\x06")
        .expect("EOCD が無い");
    let mut fake = vec![0u8; 46 + name.len()];
    fake[..4].copy_from_slice(b"PK\x01\x02");
    fake[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
    fake[46..].copy_from_slice(name.as_bytes());

    let mut patched = bytes[..eocd].to_vec();
    patched.extend_from_slice(&fake);
    patched.extend_from_slice(&bytes[eocd..]);
    patched
}

/// `InvalidContainer` の該当エントリ名を取り出す（他の変種は即座に落とす）。
fn invalid_entry(error: &DocumentError) -> String {
    match error {
        DocumentError::InvalidContainer { entry } => entry.clone(),
        other => panic!("InvalidContainer 以外が返った: {other:?}"),
    }
}

/// `decode`（バイト列経路）と `open`（実ファイル経路）の両方で拒否理由を観測する。
///
/// 返るのは両経路に共通の `InvalidContainer.entry` である。片方でも受理すれば panic し、
/// 理由が食い違えば assert が落ちる（`open` は `decode` へ素通しする 1 経路であることの実測）。
/// 対象ファイルは呼び出しごとに一意な名前を使う（作業ディレクトリは [`Scratch`] が片付ける）。
fn rejection_of(scratch: &Scratch, tag: &str, container: &[u8]) -> String {
    let decoded = match ContainerCodec::decode(container) {
        Err(error) => invalid_entry(&error),
        Ok(_) => panic!("{tag}: decode が不正なアーカイブを受理した"),
    };

    let path = scratch.file(&format!("{tag}.jxcel"));
    fs::write(&path, container).expect("入力を書き出せる");
    let opened = match api().open(&path) {
        Err(error) => invalid_entry(&error),
        Ok(_) => panic!("{tag}: open が不正なアーカイブを受理した"),
    };
    assert_eq!(decoded, opened, "{tag}: decode と open で拒否理由が違う");
    decoded
}

/// 許可リスト外のエントリ名は、原文の名前を含む `InvalidContainer` で拒否される（要件 2.5）。
///
/// 観測する形は次の 8 種（design「EntryLayout / Responsibilities & Constraints」が列挙する
/// 攻撃面と、許可リストの 7 形に当たらない未知・大文字の名前）。いずれも**サニタイズされず、
/// 原文のまま `entry` に載る**（正規化に依存しないことの担保でもある）:
///
/// * ルート外を指すパス（`../evil.json`）
/// * 絶対パス（`/tmp/x.json`）とドライブレター（`C:/x.json`）
/// * バックスラッシュ区切り（`a\b.json`）
/// * NUL を含む名前（`man\0ifest.json`）
/// * 未知の名前（`unknown.json`）
/// * ディレクトリエントリ（末尾 `/` の `schemas/`）
/// * 大文字の拡張子（`sheets/<ulid>.JSONL`。正準形は小文字）
#[test]
fn names_outside_the_allow_list_are_rejected_with_the_raw_name() {
    let scratch = Scratch::new("malicious_names");
    let cases: &[(&str, &str)] = &[
        ("path_traversal", "../evil.json"),
        ("absolute_unix", "/tmp/x.json"),
        ("drive_letter", "C:/x.json"),
        ("backslash", "a\\b.json"),
        ("nul", "man\u{0}ifest.json"),
        ("unknown", "unknown.json"),
        ("directory", "schemas/"),
        (
            "uppercase_suffix",
            "sheets/01ARZ3NDEKTSV4RRFFQ69G5FAV.JSONL",
        ),
    ];

    for (tag, name) in cases {
        let container = zip_bytes(vec![(
            name.to_string(),
            b"{}".to_vec(),
            CompressionMethod::Stored,
        )]);
        let entry = rejection_of(&scratch, tag, &container);
        assert_eq!(
            *name, entry,
            "{tag}: 該当エントリ名が原文のまま載っていない: {entry:?}"
        );
    }
}

/// 同一パスの重複エントリを持つ ZIP は、そのパスを含む `InvalidContainer` で拒否される
/// （要件 2.6）。
///
/// それ以外は正しい文書（型マーカー + 全パート）である。`zip` の `ZipWriter` は同一名の
/// エントリ自体を拒否するため、正しい ZIP を組んだ後に `document.json` の名前だけを
/// **同じ長さの** `manifest.json` へ書き換えて重複を作る（[`patch_entry_name`]）。
/// `zip` の `ZipArchive` は同一名のエントリを `IndexMap` で**黙って畳む**ため、この検出だけは
/// 中央ディレクトリの生バイト走査で行われる（`src/container/reader.rs` 参照）。重複の判定は
/// 名前だけで完結するため、内容は展開されない。
#[test]
fn a_duplicate_entry_path_is_rejected_with_the_shared_path() {
    let scratch = Scratch::new("malicious_duplicate");
    let document = sample();
    let mut container = zip_bytes(document_entries(&document));
    patch_entry_name(&mut container, "document.json", "manifest.json");

    let entry = rejection_of(&scratch, "duplicate", &container);
    assert_eq!("manifest.json: duplicate entry path", entry);
    assert!(
        entry.contains("manifest.json"),
        "該当パスが載っていない: {entry:?}"
    );
}

/// 許可リスト違反と重複パスを同時に持つ入力は、重複ではなく**許可リスト違反**で拒否される
/// （`layout::admit` の判定順。要件 2.5 は名前の段階で止まることを求める）。
///
/// 正しい文書に `../evil.json` を足し、そのうえで `document.json` の名前を書き換えて
/// `manifest.json` を重複させる。許可リスト側を先に判定する実装は `../evil.json` を報告し、
/// 重複側を先に判定する実装は `manifest.json: duplicate entry path` を報告する。
#[test]
fn an_allow_list_violation_is_reported_before_a_duplicate_path() {
    let scratch = Scratch::new("malicious_precedence");
    let mut entries = document_entries(&sample());
    entries.insert(
        0,
        (
            "../evil.json".to_string(),
            b"{}".to_vec(),
            CompressionMethod::Stored,
        ),
    );
    let mut container = zip_bytes(entries);
    patch_entry_name(&mut container, "document.json", "manifest.json");

    let entry = rejection_of(&scratch, "precedence", &container);
    assert_eq!(
        "../evil.json", entry,
        "許可リスト違反より重複パスが先に報告されている"
    );
}

/// 正規化で同一視され得る別表記（`./manifest.json`）は、**重複ではなく許可リスト違反**として
/// 原文の名前つきで拒否される（畳み込みをしないことの実測。要件 2.5 と 2.6 の境界）。
///
/// 実装は `manifest.json` と `./manifest.json` を別の名前として扱う（バイト単位で異なるため）。
/// したがって返るエラーは重複の報告（`duplicate entry path`）ではなく、許可リストの拒否であり、
/// `entry` は原文の `./manifest.json` である。パス区切りの正規化（`enclosed_name()` の類）に
/// 依存しないという design の決定を、この表記で固定する。
#[test]
fn a_dot_prefixed_spelling_is_rejected_by_the_allow_list_not_folded_into_a_duplicate() {
    let scratch = Scratch::new("malicious_dot_prefix");
    let document = sample();
    let mut entries = document_entries(&document);
    entries.push((
        "./manifest.json".to_string(),
        b"{}".to_vec(),
        CompressionMethod::Deflated,
    ));

    let entry = rejection_of(&scratch, "dot_prefix", &zip_bytes(entries));
    assert_eq!("./manifest.json", entry);
    assert!(
        !entry.contains("duplicate entry path"),
        "正規化で同一視された別表記が重複として畳まれている: {entry:?}"
    );
}

/// 許可リストの判定は、どのエントリの展開よりも先に行われる（要件 2.5。判定順の実測）。
///
/// 実際は 64 KiB へ膨張する `document.json` に 100 バイトと宣言させたアーカイブを 2 つ組む:
///
/// * 許可リスト違反の `../evil.json` を**足さない**もの → 展開側（サイズ照合）のエラーになる
///   （対照。この偽装だけでも拒否されることを先に示す）。
/// * 許可リスト違反の `../evil.json` を**足した**もの → 許可リスト側のエラーになる。
///
/// 後者が許可リストのエラーを返すことは、展開（サイズ照合）が先に走っていないことの実測である
/// （5.3 の裁定「展開は許可リストと重複の判定が終わるまで始まらない」）。
#[test]
fn the_allow_list_is_decided_before_any_entry_is_expanded() {
    let scratch = Scratch::new("malicious_order");
    let document = sample();
    let without_invalid = forged_size_container(&document, EXPANDED_SIZE, 100, &[]);
    let with_invalid = forged_size_container(&document, EXPANDED_SIZE, 100, &["../evil.json"]);

    let size_rejection = rejection_of(&scratch, "order_control", &without_invalid);
    assert_eq!(
        "document.json: declared size 100 does not match expanded size 101", size_rejection,
        "対照（許可リスト外を足さない場合）がサイズ照合で拒否されていない"
    );

    let allow_list_rejection = rejection_of(&scratch, "order", &with_invalid);
    assert_eq!(
        "../evil.json", allow_list_rejection,
        "展開（サイズ照合）が許可リストの判定より先に走っている"
    );
}

/// 実際より小さい非圧縮サイズを宣言するアーカイブは、有界読み（宣言 + 1）で拒否される。
///
/// 実際は 64 KiB へ膨張する Deflate エントリに 100 バイトと宣言させる。読み手は
/// **宣言 + 1 バイトだけ**読み、展開長 101 が宣言 100 と一致しないため中止する。
/// 報告される展開長が 101（= 宣言 + 1）であることが、無制限に展開していないことの実測である
/// （`read_to_end` していれば 65536 が報告される）。アーカイブ全体が 64 KiB よりずっと小さい
/// ことも確かめ、標本が実際に「極端に膨張する」入力であることを固定する。
#[test]
fn a_declared_size_smaller_than_the_expansion_is_rejected_with_bounded_reading() {
    let scratch = Scratch::new("malicious_underdeclared");
    let document = sample();
    let container = forged_size_container(&document, EXPANDED_SIZE, 100, &[]);
    assert!(
        container.len() < EXPANDED_SIZE,
        "標本が膨張していない（アーカイブ {} バイト / 展開 {} バイト）",
        container.len(),
        EXPANDED_SIZE
    );

    let entry = rejection_of(&scratch, "underdeclared", &container);
    assert_eq!(
        "document.json: declared size 100 does not match expanded size 101",
        entry
    );
}

/// 実際より大きい非圧縮サイズを宣言するアーカイブも拒否される（一致要求の逆方向）。
///
/// 実際は 64 KiB で終わるエントリに 1 MiB と宣言させる。読み手はストリームの終端まで読み
/// （データは正しいので `zip` の CRC 照合は通る）、展開長 65536 が宣言と一致しないため中止する。
#[test]
fn a_declared_size_larger_than_the_expansion_is_rejected() {
    let scratch = Scratch::new("malicious_overdeclared");
    let document = sample();
    let container = forged_size_container(&document, EXPANDED_SIZE, (1 << 20) as u32, &[]);

    let entry = rejection_of(&scratch, "overdeclared", &container);
    assert_eq!(
        "document.json: declared size 1048576 does not match expanded size 65536",
        entry
    );
}

/// 同じ組み立て方で作った正しいアーカイブは拒否されない（対照）。
///
/// `decode` が標本と同一の論理エントリ集合を返し、`open` が標本と同一のモデルを返すことを
/// 確かめる。上の拒否テストが「どんな ZIP でも拒否しているだけ」でないことを示す。
#[test]
fn a_correct_archive_built_the_same_way_is_accepted() {
    let scratch = Scratch::new("malicious_control");
    let document = sample();
    let container = valid_container(&document);

    let decoded = ContainerCodec::decode(&container).expect("正しいアーカイブは復号できる");
    let expected = to_parts(&document).expect("標本はパート集合へ取り出せる");
    assert_eq!(
        entries_of(&expected),
        entries_of(&decoded),
        "復号した集合が標本と違う"
    );
    assert_eq!(
        expected.format_version(),
        decoded.format_version(),
        "復号した形式バージョンが標本と違う"
    );

    let path = scratch.file("control.jxcel");
    fs::write(&path, &container).expect("正しいアーカイブを書き出せる");
    let outcome = api().open(&path).expect("正しいアーカイブは開ける");
    assert_same_document(&document, &outcome.document);
}

/// 中央ディレクトリと EOCD の間の `PK\x01\x02` の並びは、存在しないエントリとして
/// **過剰に拒否**され得る（可用性のみの劣化。task 5.3 の既知の制限、tasks.md の 8.5 への申し送り）。
///
/// `src/container/reader.rs` の生バイト走査（`raw_entry_names`）は、`zip` 8.6 が EOCD の
/// `cd_size` / `cd_count` を公開しないため、中央ディレクトリの終端を EOCD の署名でしか
/// 判定できない。したがって本物のディレクトリと EOCD の間に `PK\x01\x02` の並びがあると、
/// それを**存在しないエントリのヘッダ**として読み、その名前を許可リストに掛けて拒否する。
///
/// ここでは正しい ZIP に `evil.json` という偽のレコードを挿入し、`zip` の読み手は本物の
/// エントリだけを読める（[`inject_central_directory_lookalike`]）にもかかわらず、本実装が
/// 過剰に拒否することを固定する。**不正な受理にはならない**（不正な名前・重複は当然拒否され、
/// この過剰拒否は可用性を落とすだけである）。将来この走査が `cd_size` で有界化されれば、
/// この入力は正当に受理され得る。その場合に守るべき不変条件は「挿入した名前が論理エントリ
/// 集合へ入らないこと」であり、このテストはそのとき更新してよい。
#[test]
fn a_central_directory_lookalike_between_the_index_and_the_eocd_is_over_rejected() {
    let scratch = Scratch::new("malicious_lookalike");
    let document = sample();
    let valid = valid_container(&document);
    assert!(
        ContainerCodec::decode(&valid).is_ok(),
        "対照（挿入なし）が読めないと、注入の効果を帰属できない"
    );

    let injected = inject_central_directory_lookalike(&valid, "evil.json");
    let entry = rejection_of(&scratch, "lookalike", &injected);
    assert_eq!(
        "evil.json", entry,
        "既知の制限（EOCD の cd_size で有界化していない）の挙動が変わった"
    );
}
