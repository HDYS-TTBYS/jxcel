//! 決定性の検証（タスク 8.2。要件 3.1, 3.2）。
//!
//! design「Testing Strategy / Integration Tests」の「**決定性**: 同一ドキュメントを 2 回保存して
//! バイト一致すること。CI マトリクスで Linux / macOS / Windows の出力が一致すること
//! （3.1, 3.2）」を実装する。タスク 5.2（[`ContainerCodec`] の決定的符号化）と 7.2（`save` の
//! 結線）が既に固定した内容を、**保存経路（[`DocumentFormatApi::save`]）の出力バイト列**という
//! 最終的な観測面から確かめる。符号化の内部パラメータを固定することと、保存された 1 つの
//! ファイルが決定的であることは別の命題であり（`save` は検証・パート構築・符号化・原子的
//! 書き込みを順に繋ぐ）、本ファイルは後者を担う。
//!
//! # 「決定性」の意味（注意: 標本の識別子は内容の一部である）
//!
//! 本ファイルが固定する同一性は、**同じ識別子を持つ文書**に対する同一性である。`Document`
//! の識別子（`DocumentId` / `SheetId` / `RowId`）は `document.json` と行エントリへ永続化される
//! **文書の内容の一部**であり、[`Document::new`] / `add_sheet` / `add_row` は `IdFactory` から
//! 新しい ULID を発行する（ULID は 48 bit ミリ秒時刻 + 80 bit ランダム値）。したがって
//! **新規作成した文書のバイト列がプロセスごとに違うのは決定性の違反ではない**（値が同じでも
//! 識別子が違えば別の内容である）。このため、**プロセスを跨いだ（＝ CI の OS を跨いだ）
//! バイト比較のアンカーには、識別子まで固定された文書**を使う必要がある。本ファイルは
//! [`common::fixture_path`] の `golden_container.zip`（5.2 が固定 ID の集合から生成した
//! コミット済みの実体）を唯一のアンカーにする。
//!
//! # CI マトリクスによる OS 間一致（要件 3.2）の検証方法
//!
//! CI ワークフロー（`.github/workflows/ci.yml`）は `cargo test --workspace` を
//! ubuntu / macos / windows の 3 OS マトリクスで実行する。本ファイルのアンカーテスト
//! （[`the_committed_anchor_bytes_are_reproduced_across_time_and_directories`]）は、
//! コミット済み `golden_container.zip` を `open` して `save` し直した出力が
//! **アンカーのバイト列と一致する**ことを要求する。`save` がビルド環境（`cfg!(windows)` に
//! よる host system バイト、現在時刻、乱数など）に依存する値を出力へ混ぜれば、**どの OS で
//! 走らせても**一致しなくなり、このテストが落ちる。したがってこの比較が、そのまま
//! **3 OS の出力が同一であることの検証**になる（CI 自体は本タスクの実行環境では走らせられない。
//! ローカルでは Linux のみを実測する。タスク完了報告の「検証できなかったこと」を参照）。
//!
//! # 観測する性質
//!
//! 1. **アンカーとの一致**（要件 3.1, 3.2。CI マトリクスの実体）: コミット済み期待バイト列を
//!    `open` → `save` して一致する。同時に、（a）その出力が
//!    [`ContainerCodec::encode`]\([`DocumentFormatApi::to_parts`]\(document)\) のバイト列とも
//!    一致すること（保存経路と符号化経路の一致）、（b）実時間で秒境界を跨いだ 2 度目の保存も
//!    一致すること（要件 3.6: 時刻に依存しない）を確かめる。`tests/api.rs` の
//!    `save_reproduces_the_committed_golden_fixture_bytes` と 1 行を写すだけにせず、
//!    保存経路の決定性・1 経路性・時刻非依存を 1 つのアンカーから束ねて示す。
//! 2. **2 回保存のバイト一致**（要件 3.1）: `common` の 5 標本をループし、同一の文書を
//!    別々のパス・別々のディレクトリへ 2 回保存してバイト単位で一致する（同一プロセス内で
//!    同一の `Document` を使うため、ULID は問題にならない）。
//! 3. **保存経路と符号化経路の一致**（要件 8.2 の結線）: `save(d, p)` の出力が
//!    `ContainerCodec::encode(&to_parts(d)?)` のバイト列と一致することを標本ループで確かめる。
//! 4. **決定的固定係数の確認**: アンカーの出力 ZIP が固定の更新日時・unix permissions・
//!    ホスト OS バイトを持つことを、**ファイルに落ちたバイト列**の生ヘッダから確認する
//!    （5.2 の単体テストは符号化の戻り値を対象にしており、本ファイルは保存されたファイルを
//!    対象にする）。
//!
//! 標本・`Scratch`（一時ディレクトリ）は `tests/common/mod.rs` を再利用する（第二の比較規約・
//! 標本を作らない）。`tests/common/mod.rs` は `container` を import しない方針のため、
//! コンテナ依存の観測（[`ContainerCodec`] と生ヘッダの解析）は本ファイル側に置く。

mod common;

use std::fs;
use std::thread::sleep;
use std::time::Duration;

use document_format::container::writer::marker_bytes;
use document_format::container::ContainerCodec;
use document_format::{Document, DocumentFormatApi};

use common::{
    api, central_headers, fixture_path, local_headers, sample, sample_minimal,
    sample_with_unknown_fields, sample_with_unreferenced_attachment, sample_with_values, Scratch,
};

/// 決定性を確かめる標本の一覧。
///
/// 標本ごとに 1 つ以上の性質を担わせ、どれか 1 つが落ちれば経路の欠陥を特定できるようにする
/// （`tests/roundtrip.rs` の `samples()` と同じ流儀）。未知フィールド保持の標本は
/// `from_parts` 経由で組み立てられるため、保持フィールドのバイト列も保存経路へ乗る。
fn samples() -> Vec<(&'static str, Document)> {
    vec![
        ("minimal", sample_minimal()),
        ("multi_sheet", sample()),
        ("values", sample_with_values()),
        ("unreferenced_attachment", sample_with_unreferenced_attachment()),
        ("unknown_fields", sample_with_unknown_fields("A")),
    ]
}

/// 期待バイト列と実測バイト列を比較し、相違があれば最初の位置を示して panic する。
fn assert_same_bytes(expected: &[u8], actual: &[u8], label: &str) {
    if expected != actual {
        let mismatch = expected.iter().zip(actual).position(|(a, b)| a != b);
        panic!(
            "{label}: コミット済み期待バイト列と一致しない（要件 3.1, 3.2）。\n\
             期待 {} バイト / 実際 {} バイト / 最初に相違した位置 {mismatch:?}\n\
             この比較は CI の Linux / macOS / Windows マトリクスで走り、OS を跨いだ\
             バイト同一性を検証する。相違した場合は、保存経路（検証・パート構築・符号化・\
             原子的書き込み）のいずれかが環境依存の値を混ぜたか、決定的固定係数\
             （更新日時・permissions・host system・圧縮レベル・エントリ順）が変わったことを\
             意味する。5.2 のゴールデンと同じく、まず符号化バックエンド（`flate2` / \
             `miniz_oxide`）の更新を疑うこと（design「ContainerCodec / Implementation \
             Notes」）。",
            expected.len(),
            actual.len(),
        );
    }
}

/// コミット済みアンカーを `open` → `save` した出力が、時間・ディレクトリに依存せず一致する
/// （要件 3.1, 3.2, 3.6。CI マトリクスの実体）。
///
/// 期待値は `tests/fixtures/bytes/golden_container.zip` のバイト列そのものである（実装の出力から
/// 期待値を組み立てない）。同 fixture は固定 ID の集合から 5.2 が生成した**識別子まで固定された
/// 唯一の入力**であり、`save` が環境依存の値（host system バイト・現在時刻・乱数）を混ぜれば
/// どの OS でもここで落ちる。あわせて次の 2 点を同じアンカーで示す:
///
/// * **保存経路と符号化経路の一致**: 出力が `ContainerCodec::encode(&to_parts(document))` とも
///   一致すること（`save` が符号化の 1 経路を通っている）。
/// * **時刻非依存**（要件 3.6）: 実時間で秒境界を跨いだ 2 度目の保存も一致すること。
#[test]
fn the_committed_anchor_bytes_are_reproduced_across_time_and_directories() {
    let anchor = fixture_path();
    let expected = fs::read(&anchor)
        .unwrap_or_else(|error| panic!("期待バイト列 {} が読めない: {error}", anchor.display()));
    let document = api().open(&anchor).expect("アンカーは開ける").document;

    let encoded = ContainerCodec::encode(&api().to_parts(&document).expect("パート集合"));
    assert_same_bytes(
        &expected,
        &encoded.expect("符号化"),
        "保存経路と符号化経路（ContainerCodec::encode）",
    );

    let early = Scratch::new("determinism_anchor_early");
    let early_path = early.file("anchor.jxcel");
    api().save(&document, &early_path).expect("1 度目は保存できる");
    assert_same_bytes(
        &expected,
        &fs::read(&early_path).expect("1 度目の出力が読める"),
        "1 度目の保存",
    );

    // 1.1 秒待てば実時間で必ず秒境界を 1 回以上跨ぐ（現在時刻を混ぜる実装なら値が変わる）。
    sleep(Duration::from_millis(1_100));

    let late = Scratch::new("determinism_anchor_late");
    let late_path = late.file("anchor-with-a-much-longer-name.jxcel");
    api().save(&document, &late_path).expect("2 度目は保存できる");
    assert_same_bytes(
        &expected,
        &fs::read(&late_path).expect("2 度目の出力が読める"),
        "秒境界を跨いだ 2 度目の保存（別ディレクトリ）",
    );
}

/// 同一の文書を別々のパス・別々のディレクトリへ 2 回保存するとバイト単位で一致する
/// （要件 3.1。design の `save` 不変条件）。
///
/// パス名（長さの違う 2 つの名前）・保存時刻・ホスト環境に由来する値がバイト列へ混入して
/// いないことを、実ファイルのバイト比較で観測する。標本をループするため、1 つの標本だけで
/// 通るテストにはならない。
#[test]
fn saving_the_same_document_twice_gives_identical_bytes() {
    let first = Scratch::new("determinism_first");
    let second = Scratch::new("determinism_second");

    for (name, document) in samples() {
        let left = first.file(&format!("{name}.jxcel"));
        let right = second.file(&format!("{name}-with-a-much-longer-name.jxcel"));

        api().save(&document, &left).unwrap_or_else(|error| {
            panic!("{name}: 1 度目の保存に失敗した: {error:?}");
        });
        api().save(&document, &right).unwrap_or_else(|error| {
            panic!("{name}: 2 度目の保存に失敗した: {error:?}");
        });

        let left_bytes = fs::read(&left).expect("1 度目の出力が読める");
        let right_bytes = fs::read(&right).expect("2 度目の出力が読める");
        assert!(
            !left_bytes.is_empty(),
            "{name}: 保存されたファイルが空である（比較が空振りする）"
        );
        if left_bytes != right_bytes {
            let mismatch = left_bytes.iter().zip(&right_bytes).position(|(a, b)| a != b);
            panic!(
                "{name}: 別パス・別ディレクトリへの 2 回保存でバイト列が違う（要件 3.1）。\n\
                 1 度目 {} バイト / 2 度目 {} バイト / 最初に相違した位置 {mismatch:?}",
                left_bytes.len(),
                right_bytes.len(),
            );
        }
    }
}

/// 保存の出力が符号化経路のバイト列と一致する（保存が符号化の 1 経路を通ることの確認）。
///
/// `save` の出力と `ContainerCodec::encode(&to_parts(&document))` を標本ごとに比較する。
/// 保存経路が独自の ZIP 組み立てや別の圧縮設定を持ち込めば、ここで落ちる（要件 8.2 の
/// 結線を、バイト列という観測面で固定する）。
#[test]
fn save_and_the_container_encoding_path_agree() {
    let scratch = Scratch::new("determinism_single_path");

    for (name, document) in samples() {
        let path = scratch.file(&format!("{name}.jxcel"));
        api().save(&document, &path).unwrap_or_else(|error| {
            panic!("{name}: 保存に失敗した: {error:?}");
        });
        let saved = fs::read(&path).expect("保存されたファイルが読める");

        let parts = api().to_parts(&document).unwrap_or_else(|error| {
            panic!("{name}: パート集合へ取り出せない: {error:?}");
        });
        let encoded = ContainerCodec::encode(&parts).expect("符号化");

        assert_eq!(
            saved, encoded,
            "{name}: 保存経路と符号化経路のバイト列が違う（保存が 1 経路を通っていない）"
        );
    }
}

/// 保存されたファイルの ZIP が決定的固定係数を持つ（要件 3.6。生ヘッダの実測）。
///
/// 5.2 の単体テスト（[`ContainerCodec::encode`] の戻り値を対象にする）とは別に、ここでは
/// **アンカーを `save` してディスクに落ちたバイト列**を対象にする。観測する固定値は
/// `src/container/writer.rs` の定数と一致しなければならない:
///
/// * 更新日時: 1980-01-01 00:00:00（`FIXED_MODIFIED_TIME` = `DateTime::DEFAULT`）。
/// * unix permissions: 0644（`FIXED_UNIX_PERMISSIONS`。外部属性の上位 16 bit は
///   通常ファイルを表す `0o100644`）。
/// * ホスト OS バイト: Unix = 3（`FIXED_HOST_SYSTEM`。`cfg!(windows)` を反映しない）。
/// * エントリ順: 型マーカー `jxcel` が先頭、続いてパート名の昇順。
/// * 圧縮方式: 型マーカーが `Stored`（0）、他が `Deflate`（8）。
/// * データディスクリプタ: 使わない（ローカルヘッダのフラグ bit 3 が立たない）。
#[test]
fn saved_bytes_fix_every_determinism_relevant_zip_parameter() {
    let scratch = Scratch::new("determinism_headers");
    let document = api().open(&fixture_path()).expect("アンカーは開ける").document;
    let path = scratch.file("anchor.jxcel");
    api().save(&document, &path).expect("アンカーは保存できる");
    let bytes = fs::read(&path).expect("保存されたファイルが読める");

    let parts = api().to_parts(&document).expect("アンカーはパート集合へ取り出せる");
    let expected_names: Vec<String> = std::iter::once("jxcel".to_owned())
        .chain(parts.iter().map(|part| part.name.to_string()))
        .collect();

    let local = local_headers(&bytes);
    let local_names: Vec<String> = local.iter().map(|header| header.name.clone()).collect();
    assert_eq!(
        expected_names, local_names,
        "エントリの書き込み順が固定されていない（型マーカー先頭 + パート名昇順）"
    );
    assert_eq!("jxcel", local_names[0], "型マーカーが先頭エントリでない");

    for header in &local {
        assert_eq!(
            (0x0000u16, 0x0021u16),
            (header.modified_time, header.modified_date),
            "更新日時が 1980-01-01 00:00:00 に固定されていない: {}",
            header.name
        );
        assert_eq!(
            0,
            header.flags & 0x0008,
            "データディスクリプタが使われている（サイズは既知であり使ってはならない）: {}",
            header.name
        );
    }
    assert_eq!(0, local[0].compression_method, "型マーカーが無圧縮（Stored）でない");
    for header in local.iter().skip(1) {
        assert_eq!(8, header.compression_method, "Deflate でない: {}", header.name);
    }

    // 型マーカーの内容は確定形（`jxcel\n<major>.<minor>\n`。`Stored` なので展開せずに読める）。
    let marker = &local[0];
    assert_eq!(
        marker_bytes(parts.format_version()),
        &bytes[marker.data_start..marker.data_start + marker.data_len],
        "型マーカーの内容が確定形と違う"
    );

    let central = central_headers(&bytes);
    let central_names: Vec<String> = central.iter().map(|header| header.name.clone()).collect();
    assert_eq!(local_names, central_names, "中央ディレクトリの順序がローカルヘッダと違う");
    for header in &central {
        assert_eq!(
            (0x0000u16, 0x0021u16),
            (header.modified_time, header.modified_date),
            "中央ディレクトリの更新日時が固定されていない: {}",
            header.name
        );
        assert_eq!(
            3,
            (header.version_made_by >> 8) as u8,
            "host system バイトが Unix 固定でない（ビルド環境を反映している）: {}",
            header.name
        );
        assert_eq!(
            0o100644,
            header.external_attributes >> 16,
            "unix permissions が固定されていない: {}",
            header.name
        );
    }
    assert_eq!(0, central[0].compression_method, "型マーカーが無圧縮（Stored）でない");
    for header in central.iter().skip(1) {
        assert_eq!(8, header.compression_method, "Deflate でない: {}", header.name);
    }
}


