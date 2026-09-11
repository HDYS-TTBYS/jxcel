#!/bin/sh
# check-capabilities.sh — 生成された capability 記述の逸脱検査（tasks.md 7.3 / 要件 4.7）
#
# 目的（design.md「Adapter Layer → CommandSurface」の 4 段構成の第 4 段）:
#   要件 4.7「フロントエンドが任意のファイルを直接読み書きする経路、および任意のプロセスを
#   起動する経路を提供しない」は、次の 4 段で成立している。
#     (1) ファイルシステム系・シェル系のプラグインを依存に入れない（タスク 1.3）
#     (2) 自前コマンドの ACL を有効にする（src-tauri/permissions/app.toml。タスク 7.3）
#     (3) 未使用コマンドをビルド時に削る（build.removeUnusedCommands。タスク 7.3）
#     (4) 生成される capability の記述を機械検査する（**本スクリプト**）
#   本スクリプトは (4) を担う。`tauri-build` が `src-tauri/gen/schemas/capabilities.json` へ
#   書き出す実効の capability 記述を入力に取り、次の逸脱を検出して非 0 で終了する。
#     (a) `fs:` または `shell:` を接頭辞に持つ権限識別子
#     (b) 「全ウィンドウ」を与える指定（`windows` / `webviews` に要素 `"*"`）
#
# **キー不在を `"*"` と同一視しない。** Tauri v2 は `windows` の省略を「全ウィンドウ」ではなく
# 「どのウィンドウにも適用しない」として解決する（タスク 7.1 で実測。省略時は `core:event` の
# `listen` まで拒否された）。したがって `windows` キーの不在は逸脱ではない。逸脱なのは
# **キーが存在して要素に `"*"` がある場合だけ**である（tasks.md 7.1 の訂正）。
#
# JSON の解釈に Node.js を使う理由:
#   capability の `description` は任意の日本語文字列であり、`fs:` や `windows`、`"*"` という
#   字面を含みうる（実際に `capabilities/default.json` の説明文はこれらの語を含む）。生の
#   JSON に対する正規表現の走査は、説明文の中身を権限の逸脱として誤検知する。そこで JSON を
#   構造として解釈し、`permissions` の要素と `windows` / `webviews` の要素だけを対象にする。
#   Node.js は本リポジトリの CI（3 OS マトリクス）が必ず導入しており（`.github/workflows/ci.yml`
#   の `actions/setup-node`）、フロントエンドのビルドと同じジョブで走る。
#
# POSIX sh 互換: `bash scripts/check-capabilities.sh` が Linux / macOS / Windows (Git Bash) の
# いずれでも動作すること（3 OS マトリクス共用）。node は環境変数で入力を受け取るため、
# パスに空白や引用符が含まれても安全である。
#
# 使い方: sh scripts/check-capabilities.sh [capabilities.json のパス]
#         (既定: src-tauri/gen/schemas/capabilities.json)
# 終了コード:
#   0 = 逸脱なし
#   1 = 逸脱を検出（検出した項目を標準エラーへ列挙する）
#   2 = 入力の不在・解釈失敗・node 不在（生成をやり直すか前提を満たす必要がある）
set -eu

FILE="${1:-src-tauri/gen/schemas/capabilities.json}"

if [ ! -f "$FILE" ]; then
  echo "check-capabilities: 生成された capability 記述が見つかりません: $FILE" >&2
  echo "check-capabilities: リポジトリルートで次を実行して生成してください（tauri-build が書き出します）:" >&2
  echo "check-capabilities:   cargo build -p jxcel" >&2
  exit 2
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-capabilities: node が見つかりません（JSON を構造として解釈するために必要です）" >&2
  echo "check-capabilities: フロントエンドのツールチェーン（actions/setup-node）を導入してください" >&2
  exit 2
fi

# 検査本体。`permissions` の各要素を識別子として取り出し（文字列または `{ "identifier": ... }`）、
# 接頭辞が fs / shell のものを逸脱とする。`windows` / `webviews` は**キーが配列として存在する
# 場合に限り**要素 `"*"` を逸脱とする（キー不在は逸脱ではない）。
CAPABILITIES_FILE="$FILE" node -e '
const fs = require("fs");
const file = process.env.CAPABILITIES_FILE;

let document;
try {
  document = JSON.parse(fs.readFileSync(file, "utf8"));
} catch (error) {
  console.error("check-capabilities: capability 記述を JSON として解釈できません: " + file);
  console.error("check-capabilities: " + error.message);
  process.exit(2);
}

if (document === null || typeof document !== "object" || Array.isArray(document)) {
  console.error("check-capabilities: capability 記述がオブジェクトではありません: " + file);
  process.exit(2);
}

const forbiddenPrefixes = ["fs", "shell"];
const windowKeys = ["windows", "webviews"];
const violations = [];

function entryIdentifier(entry) {
  if (typeof entry === "string") {
    return entry;
  }
  if (entry !== null && typeof entry === "object" && typeof entry.identifier === "string") {
    return entry.identifier;
  }
  return null;
}

for (const capabilityId of Object.keys(document)) {
  const capability = document[capabilityId];
  if (capability === null || typeof capability !== "object" || Array.isArray(capability)) {
    console.error("check-capabilities: capability \"" + capabilityId + "\" がオブジェクトではありません: " + file);
    process.exit(2);
  }

  const permissions = Array.isArray(capability.permissions) ? capability.permissions : [];
  for (const entry of permissions) {
    const identifier = entryIdentifier(entry);
    if (identifier === null) {
      console.error("check-capabilities: capability \"" + capabilityId + "\" の permissions の要素を識別子として解釈できません: " + JSON.stringify(entry));
      process.exit(2);
    }
    const separator = identifier.indexOf(":");
    const prefix = separator === -1 ? null : identifier.slice(0, separator);
    if (prefix !== null && forbiddenPrefixes.indexOf(prefix) !== -1) {
      violations.push(
        "capability \"" + capabilityId + "\": 禁止された権限識別子 \"" + identifier +
        "\"（ファイルシステム系・シェル系の権限は与えない。要件 4.7。capabilities/default.json から削除すること）"
      );
    }
  }

  for (const key of windowKeys) {
    const labels = capability[key];
    // キー不在・非配列は逸脱ではない（上記ヘッダの「キー不在を "*" と同一視しない」を参照）。
    if (!Array.isArray(labels)) {
      continue;
    }
    for (const label of labels) {
      if (label === "*") {
        violations.push(
          "capability \"" + capabilityId + "\": " + key + " に全ウィンドウ指定 \"*\" がある" +
          "（対象のウィンドウをラベル規約 doc-<連番> / empty-<連番> で明示すること。要件 4.7）"
        );
      }
    }
  }
}

if (violations.length > 0) {
  console.error("check-capabilities: 逸脱を " + violations.length + " 件検出しました: " + file);
  for (const violation of violations) {
    console.error("  - " + violation);
  }
  process.exit(1);
}

console.log("check-capabilities: OK 逸脱なし: " + file);
'
