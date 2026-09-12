#!/bin/sh
# check-bundle-forms.sh — 配布物の形と検証用の形が共有する資産の同一性検査（tasks.md 10.4 / 要件 10.4）
#
# 目的（10.4 の方式 A の前提を機械検査する）:
#   10.4 は「3 OS の配布物が起動して初回描画が成立し、**両方の最小画面**（`smoke-table` /
#   `smoke-editor`）が描画される」ことを、配布物の検査と**検証用の形**
#   （`JXCEL_VERIFICATION_BUILD=1 npx tauri build --no-bundle --features verification-triggers`
#   が作る `target/release/jxcel[.exe]`）の起動に分けて確かめる。方式 A が成立するのは、
#   **両方の形がスモーク画面のコードとシェルの登録簿を共通に持ち**、違うのが検証専用の入口
#   （`src/shell/verificationScreen.ts` / `src/shell/verificationBulk.ts`）だけだからである。
#
#   **以前の CI のコメントは「埋め込み資産は配布物と同一である」と書いていたが、これは誤りで
#   あった** — 検証用の形は `npx tauri build` のたびに `npm run build` をやり直すので、
#   配布物の `dist/` とは別のビルドである（検証専用コードの除外を入れた後は、意図的に異なる）。
#   本スクリプトは「何が等しく、何が違うか」を機械的に固定する:
#
#   等しくなければならないもの（両方の形に現れる）:
#     - 10.4 のスモーク画面の識別子 `smoke-table` / `smoke-editor`（これが無い形では、その形は
#       10.4 の両画面を描けない）
#     - シェルの領域の識別子 `jxcel-shell-region`（画面の選択と描画の報告が通る 9.1 の構造。
#       両方の形が同じシェルで描く根拠）
#   違ってよいもの（むしろ違わなければならないもの）:
#     - 配布物は検証専用の識別子（`JXCEL_VERIFICATION`）を含まない
#     - 検証用の形はそれを含む（含まなければ、検証用の引き金がフロントエンドへ届かず 10.4 の
#       経路が成立しない — **空回りしていないことの錠前**）
#
# POSIX sh 互換: `bash scripts/check-bundle-forms.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。走査は node が行う
# （`scripts/check-shipping-bundle.sh` と同じ理由）。
#
# 使い方: sh scripts/check-bundle-forms.sh <配布物をビルドした dist> <検証用の形をビルドした dist>
# 終了コード:
#   0 = 適合 / 1 = 逸脱を検出 / 2 = 入力が使えない（dist の不在・資産の不在・node 不在）
set -eu

SHIPPING_DIST="${1:-}"
VERIFICATION_DIST="${2:-}"

if [ -z "$SHIPPING_DIST" ] || [ -z "$VERIFICATION_DIST" ]; then
  echo "usage: $0 <配布物をビルドした dist> <検証用の形をビルドした dist>" >&2
  exit 2
fi

if [ ! -d "$SHIPPING_DIST" ]; then
  echo "check-bundle-forms: 配布物の dist がありません: $SHIPPING_DIST" >&2
  exit 2
fi

if [ ! -d "$VERIFICATION_DIST" ]; then
  echo "check-bundle-forms: 検証用の形の dist がありません: $VERIFICATION_DIST" >&2
  exit 2
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-bundle-forms: node が見つかりません（資産の走査に必要です）" >&2
  exit 2
fi

# 検査本体。両方の dist の JavaScript 資産を読み、共有必須の識別子と形ごとに異なる識別子を
# 照合する。
SHIPPING_DIST_DIR="$SHIPPING_DIST" VERIFICATION_DIST_DIR="$VERIFICATION_DIST" node <<'NODE'
'use strict';
const fs = require('fs');
const path = require('path');

const shippingDir = process.env.SHIPPING_DIST_DIR;
const verificationDir = process.env.VERIFICATION_DIST_DIR;
const VERIFICATION_TOKEN = 'JXCEL_VERIFICATION';

/** 両方の形に必ず現れなければならない識別子（上記ヘッダの「等しくなければならないもの」）。 */
const SHARED_TOKENS = ['smoke-table', 'smoke-editor', 'jxcel-shell-region'];

function unusable(message) {
  console.error('check-bundle-forms: ' + message);
  process.exit(2);
}

function collectJs(dir) {
  const out = [];
  const walk = (current) => {
    for (const entry of fs.readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (entry.isFile() && full.endsWith('.js')) {
        out.push(full);
      }
    }
  };
  walk(dir);
  return out;
}

function combinedText(dir, label) {
  let files;
  try {
    files = collectJs(dir);
  } catch (error) {
    unusable(label + ' を走査できません: ' + error.message);
  }
  if (files.length === 0) {
    unusable(label + ' に JavaScript の資産がありません（`npm run build` が済んでいません）');
  }
  let text = '';
  for (const file of files) {
    try {
      text += fs.readFileSync(file, 'utf8');
    } catch (error) {
      unusable(file + ' を読めません: ' + error.message);
    }
  }
  return text;
}

function countToken(text, token) {
  let count = 0;
  let index = text.indexOf(token);
  while (index !== -1) {
    count += 1;
    index = text.indexOf(token, index + token.length);
  }
  return count;
}

const shippingText = combinedText(shippingDir, '配布物の dist（' + shippingDir + '）');
const verificationText = combinedText(verificationDir, '検証用の形の dist（' + verificationDir + '）');

const violations = [];

for (const token of SHARED_TOKENS) {
  const inShipping = countToken(shippingText, token);
  const inVerification = countToken(verificationText, token);
  if (inShipping === 0) {
    violations.push(
      '配布物の dist に "' + token + '" がありません' +
      '（10.4 の方式 A は両方の形がこのコードを共有することを前提にしている）'
    );
  }
  if (inVerification === 0) {
    violations.push(
      '検証用の形の dist に "' + token + '" がありません' +
      '（検証用の形が 10.4 の両画面・シェルの構造を持っていない）'
    );
  }
}

const verificationInShipping = countToken(shippingText, VERIFICATION_TOKEN);
const verificationInVerification = countToken(verificationText, VERIFICATION_TOKEN);

if (verificationInShipping > 0) {
  violations.push(
    '配布物の dist に検証専用の識別子 "' + VERIFICATION_TOKEN + '" が ' +
    verificationInShipping + ' 箇所あります（配布物に検証専用のコードを入れない決定に反する。' +
    'vite.config.ts の __JXCEL_VERIFICATION__ による除外を確認すること）'
  );
}
if (verificationInVerification === 0) {
  violations.push(
    '検証用の形の dist に検証専用の識別子 "' + VERIFICATION_TOKEN + '" が 1 つもありません' +
    '（検証用の引き金がフロントエンドへ届かない。JXCEL_VERIFICATION_BUILD=1 でビルドしたか、' +
    'vite.config.ts の define を確認すること）'
  );
}

if (violations.length > 0) {
  console.error('check-bundle-forms: 逸脱を ' + violations.length + ' 件検出しました');
  for (const violation of violations) {
    console.error('  - ' + violation);
  }
  process.exit(1);
}

console.log(
  'check-bundle-forms: OK 両方の形が共有コード（' + SHARED_TOKENS.join(',') + '）を持ち、' +
  '検証専用の識別子は配布物に ' + verificationInShipping + ' 箇所・検証用の形に ' +
  verificationInVerification + ' 箇所です'
);
NODE
