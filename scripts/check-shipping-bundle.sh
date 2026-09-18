#!/bin/sh
# check-shipping-bundle.sh — 配布物のフロントエンド資産に検証専用コードが無いことの検査
#
# 目的（tasks.md 5.4 / 8.2 の決定、要件 4.7 の精神）:
#   配布物（既定のビルド）に検証専用のコードを入れない、という決定は Rust 側では非既定の
#   cargo feature `verification-triggers` が果たしている。**しかし cargo の feature は
#   TypeScript を括れない**ため、フロントエンド側は Vite の `define`
#   （`vite.config.ts` の `__JXCEL_VERIFICATION__`）が果たす。本スクリプトはその結果を
#   機械検査する — **ビルドされた `dist/` に検証専用の識別子・グローバル名・モジュール名が
#   1 つも現れないこと**を要求する。
#
# # なぜ `dist/` を見るのか（重要・以前の錠前が見えなかったもの）
#
# Tauri は `compression` feature（既定で有効）でフロントエンド資産を **brotli 圧縮して**
# 実行ファイルへ埋め込む。したがって `strings -a <配布物の実行ファイル>` に JS の本文は現れず、
# **実行ファイルに対する文字列検査だけでは埋め込み資産の中身を検査できない**（配布物の
# 実行ファイルに残るのは資産のファイル名だけである）。本スクリプトは埋め込みの入力そのもの
# （`npm run build` が書く `dist/`。`npx tauri build` の `beforeBuildCommand` が同じコマンドで
# 生成する）を検査し、第 2 引数に実行ファイルを渡したときは**内容ハッシュ付きのファイル名の
# 一致**で「その実行ファイルが検査した dist を埋め込んでいる」ことを結びつける
# （ファイル名は Vite が内容から決めるので、一致は埋め込み内容の同一性の根拠になる。
# 圧縮された本文を伸長する必要は無い）。
#
# # 要求するもの（3 つ。どれか 1 つでも欠けたら非 0 で終わる）
#
#   1. `dist/` に JavaScript の資産が 1 つ以上ある（ビルドされていない入力で空回りしない）
#   2. 検証専用の識別子が 1 つも現れない:
#      `JXCEL_VERIFICATION`（環境変数名とグローバル名 `__JXCEL_VERIFICATION_*` をまとめて覆う）、
#      `verificationBulk` / `verificationScreen` / `verificationMacroRun`（検証専用モジュールの
#      名前。`verificationMacroRun` は `macro-runtime` スペックの 5.1 が足した起動時の
#      マクロの実行の仕込みであり、`src/main.tsx` の `__JXCEL_VERIFICATION__` の分岐からだけ
#      動的 import される）、
#      `verification-triggers`（cargo feature 名）
#   3. 10.4 の両画面の識別子（`smoke-table` / `smoke-editor`）が**現れる** — 配布物の初回描画を
#      担保するだけでなく、10.4 の方式 A（両画面は**検証用の形でも配布物と共通のスモーク画面の
#      コード**で描かれる）の前提を機械的に固定する（`scripts/check-bundle-forms.sh` が 2 つの
#      形を突き合わせる側である）
#
# 第 2 引数（実行ファイル）を渡したときは、さらに次を要求する:
#
#   4. `dist/` の JavaScript 資産の**ファイル名**がその実行ファイルのバイト列に現れる
#      （上記の「実行ファイルが検査した dist を埋め込んでいる」ことの根拠）。
#
# POSIX sh 互換: `bash scripts/check-shipping-bundle.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。走査は node が行う
# （`scripts/check-capabilities.sh` と同じ理由 — CJK を含むテキストを安全に扱う。CI は
# `npm ci` の後なので node が必ずある）。
#
# 使い方: sh scripts/check-shipping-bundle.sh [dist ディレクトリ] [実行ファイル]
# 終了コード:
#   0 = 適合 / 1 = 逸脱を検出 / 2 = 入力が使えない（dist の不在・資産の不在・node 不在・
#       実行ファイルの不在）
set -eu

DIST="${1:-dist}"
BINARY="${2:-}"

if [ ! -d "$DIST" ]; then
  echo "check-shipping-bundle: dist ディレクトリがありません: $DIST" >&2
  echo "check-shipping-bundle: 先に \`npm run build\`（または \`npx tauri build\`）を実行してください" >&2
  exit 2
fi

# Windows の release 実行ファイルは接尾辞付きである（呼び出し側は接尾辞なしで渡せる）。
#
# **`.exe` を先に見る。** Git Bash（MSYS2 / Cygwin）の `-f` は `jxcel` を `jxcel.exe` へ
# 解決して真を返すため、「`-f $BINARY` が真なら接尾辞なしのまま使う」順序では、**Windows の
# node へ接尾辞なしのパスを渡してしまい** `ENOENT` で落ちる（実測: CI の Windows）。
# `jxcel.exe` が実在すればそれを優先する（Linux / macOS には `.exe` は無いので影響しない）。
if [ -n "$BINARY" ] && [ -f "${BINARY}.exe" ]; then
  BINARY="${BINARY}.exe"
fi
if [ -n "$BINARY" ] && [ ! -f "$BINARY" ]; then
  echo "check-shipping-bundle: 実行ファイルがありません: ${BINARY}（または ${BINARY}.exe）" >&2
  exit 2
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-shipping-bundle: node が見つかりません（資産の走査に必要です）" >&2
  exit 2
fi

# 検査本体。`dist/` を再帰的に走査し、すべての JavaScript 資産のテキストを照合する。
DIST_DIR="$DIST" SHIPPING_BINARY="$BINARY" node <<'NODE'
'use strict';
const fs = require('fs');
const path = require('path');

const distDir = process.env.DIST_DIR;
const binaryPath = process.env.SHIPPING_BINARY || '';

function unusable(message) {
  console.error('check-shipping-bundle: ' + message);
  process.exit(2);
}

/** 検証専用の識別子（`dist/` に現れてはならない）。 */
const FORBIDDEN_TOKENS = [
  'JXCEL_VERIFICATION',
  'verificationBulk',
  'verificationScreen',
  'verificationMacroRun',
  'verification-triggers',
];

/** 配布物にも必ず入っていなければならない識別子（10.4 の両画面。上記ヘッダの 3 を参照）。 */
const REQUIRED_TOKENS = ['smoke-table', 'smoke-editor'];

function collect(dir) {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...collect(full));
    } else if (entry.isFile()) {
      out.push(full);
    }
  }
  return out;
}

let files;
try {
  files = collect(distDir);
} catch (error) {
  unusable(distDir + ' を走査できません: ' + error.message);
}

const jsFiles = files.filter((file) => file.endsWith('.js'));
if (jsFiles.length === 0) {
  unusable(distDir + ' に JavaScript の資産がありません（`npm run build` が済んでいません）');
}

const violations = [];
const counts = new Map();

function countToken(text, token) {
  let count = 0;
  let index = text.indexOf(token);
  while (index !== -1) {
    count += 1;
    index = text.indexOf(token, index + token.length);
  }
  return count;
}

const contents = [];
for (const file of jsFiles) {
  try {
    contents.push({ file, text: fs.readFileSync(file, 'utf8') });
  } catch (error) {
    unusable(file + ' を読めません: ' + error.message);
  }
}

for (const { file, text } of contents) {
  for (const token of FORBIDDEN_TOKENS) {
    const count = countToken(text, token);
    counts.set(token, (counts.get(token) || 0) + count);
    if (count > 0) {
      violations.push(
        file + ': 検証専用の識別子 "' + token + '" が ' + count + ' 箇所あります' +
        '（配布物に検証専用のコードを入れない決定に反する。vite.config.ts の ' +
        '__JXCEL_VERIFICATION__ による除外を確認すること）'
      );
    }
  }
}

for (const token of REQUIRED_TOKENS) {
  const total = contents.reduce((sum, { text }) => sum + countToken(text, token), 0);
  if (total === 0) {
    violations.push(
      '10.4 のスモーク画面の識別子 "' + token + '" がどの資産にもありません' +
      '（配布物が 10.4 の両画面のコードを失っている。src/features/smoke/ と ' +
      'SHELL_SCREEN_REGISTRY を確認すること）'
    );
  }
}

// 埋め込み資産の同一性（上記ヘッダの 4）。実行ファイルのバイト列に、検査した dist の
// JavaScript 資産の**ファイル名**が現れることを要求する。
if (binaryPath !== '') {
  let binary;
  try {
    binary = fs.readFileSync(binaryPath);
  } catch (error) {
    unusable(binaryPath + ' を読めません: ' + error.message);
  }
  for (const file of jsFiles) {
    const name = path.basename(file);
    if (binary.indexOf(Buffer.from(name, 'utf8')) === -1) {
      violations.push(
        binaryPath + ': 資産のファイル名 "' + name + '" が実行ファイルに現れません' +
        '（この実行ファイルは検査した dist を埋め込んでいない可能性がある。' +
        'npx tauri build の beforeBuildCommand が npm run build を走らせているか確認すること）'
      );
    }
  }
  // 参考: 実行ファイルの生バイトにおける検証専用の識別子の出現数。**圧縮された資産の本文は
  // 現れない**ので 0 であっても「埋め込み資産に無い」ことの証明にはならない（そのために
  // ファイル名の一致を見ている）。値を出しておくのは、実行ファイル側の文字列検査
  // （ci.yml の既存の段）と突き合わせて読めるようにするためである。
  const binaryText = binary.toString('latin1');
  const rawVerification = countToken(binaryText, 'JXCEL_VERIFICATION');
  console.log('check-shipping-bundle: 参考: 実行ファイルの生バイトの JXCEL_VERIFICATION 出現数 = ' + rawVerification);
}

if (violations.length > 0) {
  console.error('check-shipping-bundle: 逸脱を ' + violations.length + ' 件検出しました: ' + distDir);
  for (const violation of violations) {
    console.error('  - ' + violation);
  }
  process.exit(1);
}

const forbiddenCounts = FORBIDDEN_TOKENS.map((token) => token + '=' + (counts.get(token) || 0)).join(' ');
console.log(
  'check-shipping-bundle: OK 配布物の資産に検証専用の識別子はありません（走査 ' +
  jsFiles.length + ' 資産 / ' + forbiddenCounts + ' / 必須 ' + REQUIRED_TOKENS.join(',') + ' は存在）'
);
NODE
