#!/bin/sh
# check-command-acl.sh — 登録されたコマンドが権限記述で許可されていることの機械検査
#                         （tasks.md 7.3 / 要件 4.7）
#
# 目的（要件 4.7 の 4 段構成の (2)(3) の間の抜けを塞ぐ）:
#   要件 4.7 は次の 4 段で成立している。
#     (1) ファイルシステム系・シェル系のプラグインを依存に入れない（タスク 1.3、
#         `scripts/check-forbidden-plugins.sh` が検査する）
#     (2) 自前コマンドの ACL を有効にする（`src-tauri/permissions/app.toml`。タスク 7.3）
#     (3) 未使用コマンドをビルド時に削る（`build.removeUnusedCommands: true`。タスク 7.3）
#     (4) 生成される capability の記述を機械検査する（`scripts/check-capabilities.sh`）
#   **(2) と (3) の間には機械的な結びつきが無かった。** コマンドを
#   `crates/app-shell/src/ipc/command_names.rs` の配列へ足し、`src-tauri/src/commands/mod.rs`
#   の `command_root!` へ登録し、`src/ipc/bindings.ts` を再生成しても、
#   **`src-tauri/permissions/app.toml` への権限追加（および `[[set]]` への追加）を忘れると、
#   `removeUnusedCommands` がそのコマンドを配布物から静かに削る** — ドリフト検査も
#   `tsc --noEmit` も capability 検査も core-deps 検査も緑のままで、失敗は実行時にしか
#   現れない。本スクリプトはこの抜けを機械検査する。
#
# # 判定する不変条件（登録 ⊆ 許可。方向が重要である）
#
#   **`command_root!` が登録したコマンド名は、すべて capability が実際に与える権限の
#   `commands.allow` に含まれていなければならない。**
#
#   逆向き（`COMMAND_NAMES` にあるのに登録していない名前）は要求しない。配列は後続タスクが
#   自分のコマンドを実装する前に名前だけを予約する場所であり（`commands/mod.rs` の doc が
#   明記している）、「配列 ⊆ 登録」を要求すると未実装の名前を置けなくなる。必要なのは
#   「**登録されたコマンドが削られない**」ことだけであり、それは「登録 ⊆ 許可」で表せる。
#
# # 許可の解決（`removeUnusedCommands` と同じ経路を通る）
#
#   `tauri-build` が書き出す `src-tauri/gen/schemas/acl-manifests.json` の `__app-acl__` が
#   権限の定義と集合を持ち、`src-tauri/gen/schemas/capabilities.json` が capability から
#   参照される識別子を持つ。**capability が参照する集合を解決してから** `commands.allow` を
#   集めるので、権限ブロックを足したのに `[[set]]` へ足し忘れた場合（これも実際に削られる）も
#   検出する。`core:*` のようなアプリ外の権限は対象外である。
#
#   **生成物を読むのは `scripts/check-capabilities.sh` と同じ理由である** — 手書きの TOML を
#   正規表現で走査するより、Tauri が実際に使う解決結果を読む方が確かである。CI では `Build`
#   （`cargo build --workspace`）の後に走るので生成物はそのビルドのものである。
#   生成物が古い可能性に備え、`permissions/app.toml` が生成物より新しいときは exit 2 で
#   「再ビルド」を促す（黙って古い許可を検査しない）。
#
# POSIX sh 互換: `bash scripts/check-command-acl.sh` が Linux / macOS /
# Windows (Git Bash) のいずれでも動作すること（3 OS マトリクス共用）。JSON の解釈と Rust
# ソースの走査は node が行う。
#
# 使い方: sh scripts/check-command-acl.sh
# 終了コード:
#   0 = 登録されたコマンドはすべて許可されている / 1 = 逸脱を検出
#   2 = 入力が使えない（生成物の不在・`__app-acl__` の不在・node 不在・生成物が古い）
set -eu

ACL_MANIFEST="${ACL_MANIFEST:-src-tauri/gen/schemas/acl-manifests.json}"
CAPABILITIES="${CAPABILITIES:-src-tauri/gen/schemas/capabilities.json}"
COMMAND_NAMES_RS="${COMMAND_NAMES_RS:-crates/app-shell/src/ipc/command_names.rs}"
COMMANDS_MOD_RS="${COMMANDS_MOD_RS:-src-tauri/src/commands/mod.rs}"
PERMISSIONS_TOML="${PERMISSIONS_TOML:-src-tauri/permissions/app.toml}"

if [ ! -f "$ACL_MANIFEST" ]; then
  echo "check-command-acl: ACL マニフェストがありません: $ACL_MANIFEST" >&2
  echo "check-command-acl: 先に \`cargo build -p jxcel\`（または \`cargo build --workspace\`）を実行してください" >&2
  exit 2
fi

if [ ! -f "$CAPABILITIES" ]; then
  echo "check-command-acl: capability 記述がありません: $CAPABILITIES" >&2
  echo "check-command-acl: 先に \`cargo build -p jxcel\`（または \`cargo build --workspace\`）を実行してください" >&2
  exit 2
fi

if [ ! -f "$COMMAND_NAMES_RS" ]; then
  echo "check-command-acl: コマンド名の単一の源がありません: $COMMAND_NAMES_RS" >&2
  exit 2
fi

if [ ! -f "$COMMANDS_MOD_RS" ]; then
  echo "check-command-acl: コマンドの根がありません: $COMMANDS_MOD_RS" >&2
  exit 2
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-command-acl: node が見つかりません（JSON と Rust ソースの解釈に必要です）" >&2
  exit 2
fi

# 検査本体。Tauri が実際に使う解決結果（生成物）と、Rust の 2 つの源を突き合わせる。
ACL_MANIFEST_FILE="$ACL_MANIFEST" \
CAPABILITIES_FILE="$CAPABILITIES" \
COMMAND_NAMES_RS_FILE="$COMMAND_NAMES_RS" \
COMMANDS_MOD_RS_FILE="$COMMANDS_MOD_RS" \
PERMISSIONS_TOML_FILE="$PERMISSIONS_TOML" \
node <<'NODE'
'use strict';
const fs = require('fs');

const manifestFile = process.env.ACL_MANIFEST_FILE;
const capabilitiesFile = process.env.CAPABILITIES_FILE;
const commandNamesFile = process.env.COMMAND_NAMES_RS_FILE;
const commandsModFile = process.env.COMMANDS_MOD_RS_FILE;
const permissionsTomlFile = process.env.PERMISSIONS_TOML_FILE;

function unusable(message) {
  console.error('check-command-acl: ' + message);
  process.exit(2);
}

function readJson(file, label) {
  let text;
  try {
    text = fs.readFileSync(file, 'utf8');
  } catch (error) {
    unusable(label + ' を読めません: ' + file + '（' + error.message + '）');
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    unusable(label + ' を JSON として解釈できません: ' + file + '（' + error.message + '）');
  }
}

function readText(file, label) {
  try {
    return fs.readFileSync(file, 'utf8');
  } catch (error) {
    unusable(label + ' を読めません: ' + file + '（' + error.message + '）');
  }
}

// --- 生成物が古くないことの確認（黙って古い許可を検査しない） ---------------------------
if (fs.existsSync(permissionsTomlFile)) {
  const tomlStat = fs.statSync(permissionsTomlFile);
  const manifestStat = fs.statSync(manifestFile);
  // ファイルシステムの mtime の粒度（1〜2 秒）を考慮した猶予である。
  if (tomlStat.mtimeMs > manifestStat.mtimeMs + 2000) {
    unusable(
      permissionsTomlFile + ' が生成物 ' + manifestFile + ' より新しい。' +
      '生成物が古い可能性があるので `cargo build -p jxcel` をやり直してから再実行してください'
    );
  }
}

// --- capability が実際に与える権限から、許可されたコマンドを解決する ---------------------
const manifest = readJson(manifestFile, 'ACL マニフェスト');
const appAcl = manifest['__app-acl__'];
if (appAcl === null || typeof appAcl !== 'object' || Array.isArray(appAcl)) {
  unusable(
    manifestFile + ' に `__app-acl__` がありません。' +
    '`src-tauri/permissions/` に権限ファイルが 1 つも無いと自前コマンドは ACL の対象外になり、' +
    '要件 4.7 の制御が働きません（tasks.md 7.3）'
  );
}
const permissionDefinitions = appAcl.permissions && typeof appAcl.permissions === 'object'
  ? appAcl.permissions
  : {};
const permissionSets = appAcl.permission_sets && typeof appAcl.permission_sets === 'object'
  ? appAcl.permission_sets
  : {};

const capabilities = readJson(capabilitiesFile, 'capability 記述');
if (capabilities === null || typeof capabilities !== 'object' || Array.isArray(capabilities)) {
  unusable(capabilitiesFile + ' がオブジェクトではありません');
}

const referenced = new Set();
for (const capabilityId of Object.keys(capabilities)) {
  const capability = capabilities[capabilityId];
  if (capability === null || typeof capability !== 'object' || Array.isArray(capability)) {
    unusable(capabilitiesFile + ' の capability "' + capabilityId + '" がオブジェクトではありません');
  }
  const list = Array.isArray(capability.permissions) ? capability.permissions : [];
  for (const entry of list) {
    if (typeof entry === 'string') {
      referenced.add(entry);
    } else if (entry !== null && typeof entry === 'object' && typeof entry.identifier === 'string') {
      referenced.add(entry.identifier);
    } else {
      unusable(
        capabilitiesFile + ' の capability "' + capabilityId + '" の permissions の要素を' +
        '識別子として解釈できません: ' + JSON.stringify(entry)
      );
    }
  }
}

const resolvedPermissions = new Set();
for (const identifier of referenced) {
  if (Object.prototype.hasOwnProperty.call(permissionSets, identifier)) {
    const set = permissionSets[identifier];
    const members = set && Array.isArray(set.permissions) ? set.permissions : [];
    for (const member of members) {
      resolvedPermissions.add(member);
    }
  } else if (Object.prototype.hasOwnProperty.call(permissionDefinitions, identifier)) {
    resolvedPermissions.add(identifier);
  }
  // それ以外は `core:*` のようなアプリ外（プラグイン・中核）の権限であり、`__app-acl__` の
  // 外にあるので対象外である。
}

const granted = new Set();
for (const permissionId of resolvedPermissions) {
  const definition = permissionDefinitions[permissionId];
  if (definition === null || typeof definition !== 'object') {
    // 集合が存在しない権限を参照している（`app.toml` の綴り違い）。
    console.error(
      'check-command-acl: 逸脱を検出しました: capability が参照する集合が、存在しない権限 "' +
      permissionId + '" を含んでいます（' + permissionsTomlFile + ' の綴りを確認すること）'
    );
    process.exit(1);
  }
  const allow = definition.commands && Array.isArray(definition.commands.allow)
    ? definition.commands.allow
    : [];
  for (const command of allow) {
    granted.add(command);
  }
}

// --- 単一の源（コマンド名の定数と配列）と、登録の一覧を読む -----------------------------
const commandNamesSource = readText(commandNamesFile, 'コマンド名の単一の源');
const constantPattern = /pub const ([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*"([^"]*)"\s*;/g;
const constants = new Map();
let match;
while ((match = constantPattern.exec(commandNamesSource)) !== null) {
  constants.set(match[1], match[2]);
}

function bracketBody(source, startMarker, label) {
  const marker = source.indexOf(startMarker);
  if (marker === -1) {
    unusable(label + ' が ' + startMarker + ' を含みません');
  }
  const equals = source.indexOf('=', marker);
  if (equals === -1) {
    unusable(label + ' の ' + startMarker + ' に ' + '`=` がありません');
  }
  const open = source.indexOf('[', equals);
  if (open === -1) {
    unusable(label + ' の ' + startMarker + ' に配列がありません');
  }
  let depth = 0;
  for (let index = open; index < source.length; index += 1) {
    const ch = source[index];
    if (ch === '[') {
      depth += 1;
    } else if (ch === ']') {
      depth -= 1;
      if (depth === 0) {
        return source.slice(open + 1, index);
      }
    }
  }
  unusable(label + ' の ' + startMarker + ' の配列が閉じていません');
}

const arrayBody = bracketBody(commandNamesSource, 'pub const COMMAND_NAMES', 'コマンド名の単一の源');
const arrayIdentifiers = arrayBody.match(/[A-Z][A-Z0-9_]*/g) || [];
const arrayNames = arrayIdentifiers.map((identifier) => {
  if (!constants.has(identifier)) {
    unusable('`COMMAND_NAMES` が未定義の定数 ' + identifier + ' を参照しています（' + commandNamesFile + '）');
  }
  return constants.get(identifier);
});

// `command_root!` の**呼び出し**（マクロ定義ではない）の本体を取る。
const commandsModuleSource = readText(commandsModFile, 'コマンドの根');
const invocationPattern = /^command_root!\s*\{/m;
const invocation = invocationPattern.exec(commandsModuleSource);
if (invocation === null) {
  unusable(commandsModFile + ' に `command_root! { ... }` の呼び出しがありません');
}
const braceOpen = commandsModuleSource.indexOf('{', invocation.index);
let braceDepth = 0;
let braceClose = -1;
for (let index = braceOpen; index < commandsModuleSource.length; index += 1) {
  const ch = commandsModuleSource[index];
  if (ch === '{') {
    braceDepth += 1;
  } else if (ch === '}') {
    braceDepth -= 1;
    if (braceDepth === 0) {
      braceClose = index;
      break;
    }
  }
}
if (braceClose === -1) {
  unusable(commandsModFile + ' の `command_root!` の本体が閉じていません');
}
const rootBody = commandsModuleSource.slice(braceOpen + 1, braceClose);
const registeredIdentifiers = (rootBody.match(/command_names::([A-Z][A-Z0-9_]*)/g) || []).map(
  (token) => token.replace('command_names::', '')
);
const registeredNames = registeredIdentifiers.map((identifier) => {
  if (!constants.has(identifier)) {
    unusable(
      '`command_root!` が ' + commandNamesFile + ' に無い定数 command_names::' + identifier +
      ' を参照しています（名前は単一の源へ足すこと。tasks.md 2.2）'
    );
  }
  return constants.get(identifier);
});

if (registeredNames.length === 0 || arrayNames.length === 0) {
  unusable(
    '登録されたコマンド名（' + registeredNames.length + ' 件）または `COMMAND_NAMES`（' +
    arrayNames.length + ' 件）が空です。走査が空回りしている可能性があります'
  );
}

// --- 判定 -------------------------------------------------------------------------------
const violations = [];

for (const name of registeredNames) {
  if (!arrayNames.includes(name)) {
    violations.push(
      '登録されたコマンド "' + name + '" が `COMMAND_NAMES`（' + commandNamesFile + '）にありません' +
      '（名前は単一の源へ足すこと。tasks.md 2.2）'
    );
  }
  if (!granted.has(name)) {
    violations.push(
      '登録されたコマンド "' + name + '" が、capability が与える権限の `commands.allow` に' +
      'ありません（' + permissionsTomlFile + ' に対応する [[permission]] を足し、[[set]] へも' +
      '足すこと）。build.removeUnusedCommands がこのコマンドを配布物から削ります（tasks.md 7.1 の申し送り）'
    );
  }
}

for (const name of granted) {
  if (!arrayNames.includes(name)) {
    violations.push(
      '権限が `COMMAND_NAMES` に無いコマンド "' + name + '" を許可しています' +
      '（' + permissionsTomlFile + ' の綴り違いの可能性。単一の源と突き合わせること）'
    );
  }
}

if (violations.length > 0) {
  console.error('check-command-acl: 逸脱を ' + violations.length + ' 件検出しました');
  for (const violation of violations) {
    console.error('  - ' + violation);
  }
  process.exit(1);
}

const setNames = [...referenced].filter((identifier) =>
  Object.prototype.hasOwnProperty.call(permissionSets, identifier)
);
console.log(
  'check-command-acl: OK 登録 ' + registeredNames.length + ' 件のコマンドはすべて許可されています' +
  '（許可 ' + granted.size + ' 件 / 使用した集合 ' + (setNames.join(',') || 'なし') + '）'
);
NODE
