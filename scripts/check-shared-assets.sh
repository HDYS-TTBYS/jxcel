#!/bin/sh
# check-shared-assets.sh — 配信先中立な資産の置き場が通信境界に依存していないことの検査
#                          （tasks.md 9.4 / 要件 9.6、design.md「SharedAssets」）
#
# 目的:
#   `src/shared/` は、フォームレンダラのように**デスクトップアプリの中と LAN 配信の Web
#   ページの両方**で動く必要がある資産の置き場である（design.md「Directory Structure」）。
#   LAN 配信される画面には Tauri のランタイムが存在しないため、ここに通信境界への依存が
#   入ると `form-web-server` スペックが成立しない。本スクリプトはその混入を機械的に検出する
#   （design.md「SharedAssets」の「`BuildPipeline` が機械検査する」の実体。要件 9.6）。
#
# 検出する依存（要件 9.6 の「通信境界」）:
#   (a) Tauri の API パッケージ `@tauri-apps/*`（サブパス・型のみの import・
#       `require` / 動的 `import()` を含む。指定子を変数や文字列に保持した場合も同じ
#       literal として対象）
#   (b) アプリ自身の IPC モジュール `src/ipc/client.ts` / `src/ipc/bindings.ts`
#       （およびそのディレクトリ形 `src/ipc/`。指定子を変数や文字列に保持した場合も対象）
#   (c) Tauri が注入する実行時グローバル `__TAURI__` / `__TAURI_INTERNALS__` /
#       `__TAURI_IPC__` と、`@tauri-apps/api` が内部で使う `transformCallback`
#
# 照合の単位（重要・規則は 1 本に統一してある）:
#   正規表現で生のテキストを走査せず、**TypeScript の構文木**として解釈したうえで、
#   構文木に現れる**すべての文字列 literal とテンプレート literal（の断片）**、および
#   識別子を照合する。`import` / `export ... from` / `import =` / `require()` /
#   動的 `import()` の**指定子はこの literal の一種にすぎず、位置で規則を分けない**。
#   これは必須である。位置で規則を分けると、`const spec = "../ipc/client"; import(spec);`
#   のように**完全な指定子を literal に保持して渡す**迂回が、指定子位置にだけ適用される
#   規則をすり抜ける（`@tauri-apps` は literal 側の規則で捕まるのに IPC 側は捕まらない、
#   という非対称が実際に生じた。round-1 のレビューで棄却された欠陥がこれである）。
#   指定子を import 文に直接書いても変数に保持しても、同じ StringLiteral ノードとして同じ
#   照合を受けるので、この非対称は構造的に起こりえない。
#   したがって README の説明文やコードのコメントが「@tauri-apps に依存してはならない」と
#   述べていても逸脱にならない（コメントは構文木に現れない）。
#   (b) は literal の値が相対指定子（`./` / `../` で始まる）なら実際に解決し、置き場の兄弟
#   `../ipc/{client,bindings}` か IPC モジュール群のディレクトリ `../ipc` を指す場合だけ
#   逸脱とする（置き場の内側の `ipc/client` や `./ipc/client` は誤検出しない）。相対でない
#   指定子（パスエイリアス・リポジトリ根からのパス）は本スクリプトからは解決できないため、
#   アプリの IPC モジュールの名前を含む場合に**保守的に**逸脱とする。
#
# **検出できない形（正直な限界）**:
#   - 断片から組み立てた指定子のうち、**どの literal にも境界の形が単体で現れない**もの
#     （例: `import(base + "/ipc/" + "client")` の `"/ipc/"` と `"client"`）。
#     本スクリプトは**定数伝播をしない**（literal の値をそのまま照合するだけ）ので、
#     変数を経由した連結の結果・`Array#join` の結果は照合対象にならない。
#     ただし literal が単体で `../ipc/client` や `ipc/client` を含めば検出する
#     （テンプレート literal の tail に完全な形が現れる場合は捕まる）。
#   - `eval` / `new Function` に渡した**文字列の中の**指定子や、その文字列が組み立てる
#     グローバル名。literal の値は「それ自体が境界の形か」でのみ照合するため、プログラム
#     テキストを内包する文字列の中までは解釈しない。
#   - 実行時に組み立てたグローバル名（例: `globalThis["__TAURI" + "_INTERNALS__"]`）。
#     識別子として現れる `__TAURI__` 等と、literal 全体がその名前と一致する場合だけを見る。
#   - 置き場の外にある別名（`tsconfig.json` / `vite.config.ts` にパスエイリアスは無い）を
#     介する指定子のうち、上記 (b) の名前照合に一致しないもの。
#
# 入力と前提:
#   - 検査対象は置き場（既定 `src/shared/`）配下の TypeScript / JavaScript
#     （.ts .tsx .mts .cts .js .jsx .mjs .cjs）。`.md` は対象外なので、README が制約の語を
#     述べても逸脱にならない。
#   - 構文解析にはリポジトリの devDependency `typescript` を使う（`npm ci` が導入する。
#     CI では `npm run typecheck` と同じジョブに載る）。したがって**リポジトリルートで
#     実行する**こと。
#
# POSIX sh 互換: `bash scripts/check-shared-assets.sh` が Linux / macOS / Windows (Git Bash) の
# いずれでも動作すること（3 OS マトリクス共用。`.github/workflows/ci.yml` の既存の
# `shell: bash` の段と同じ）。
#
# 使い方: sh scripts/check-shared-assets.sh [置き場のパス]  (既定: src/shared)
# 終了コード:
#   0 = 通信境界への依存なし
#   1 = 依存を検出（検出したファイル・行・列を標準エラーへ列挙する）
#   2 = 入力が使えない（置き場の不在 / node 不在 / typescript 不在 / ファイル読み込み失敗 /
#       構文解析の失敗）。**この 3 値はどれも「静かに通る」を含まない。**
set -eu

AREA="${1:-src/shared}"

if [ ! -d "$AREA" ]; then
  echo "check-shared-assets: 置き場が見つかりません: $AREA" >&2
  echo "check-shared-assets: 置き場が無い状態を「依存なし」として通してはならない（要件 9.6）" >&2
  exit 2
fi

if ! command -v node >/dev/null 2>&1; then
  echo "check-shared-assets: node が見つかりません（構文木の解析に必要です）" >&2
  echo "check-shared-assets: フロントエンドのツールチェーン（actions/setup-node → npm ci）を導入してください" >&2
  exit 2
fi

# 検査本体。置き場を再帰的に走査し、構文木に現れるすべての文字列・テンプレート literal と
# 識別子を照合する（指定子も literal の一種として同じ経路を通る）。
# 検出した逸脱は `ファイル:行:列` で列挙して 1、入力が使えなければ 2 で終了する。
# node の終了コードはそのまま本スクリプトの終了コードになる（`set -e` により最終コマンドの
# 失敗がそのまま伝播する）。
SHARED_AREA="$AREA" node <<'NODE'
'use strict';

const fs = require('fs');
const path = require('path');

const label = 'check-shared-assets';
const area = path.resolve(process.env.SHARED_AREA);
const areaLabel = path.relative(process.cwd(), area) || area;

let ts;
try {
  ts = require('typescript');
} catch (error) {
  console.error(label + ': typescript が見つかりません（構文木の解析に必要です）: ' + error.message);
  console.error(label + ': リポジトリルートで `npm ci` を実行してから、同じルートで本スクリプトを実行してください');
  process.exit(2);
}

// 走査対象の拡張子と TypeScript の構文種別。Markdown などを含めないのは、説明文（散文）を
// 検査対象にしないためである。
const KINDS = {
  '.ts': ts.ScriptKind.TS,
  '.tsx': ts.ScriptKind.TSX,
  '.mts': ts.ScriptKind.TS,
  '.cts': ts.ScriptKind.TS,
  '.js': ts.ScriptKind.JS,
  '.jsx': ts.ScriptKind.JSX,
  '.mjs': ts.ScriptKind.JS,
  '.cjs': ts.ScriptKind.JS,
};
const EXTENSIONS = Object.keys(KINDS);

// 通信境界の構成要素（要件 9.6）。`@tauri-apps/api` は `__TAURI_INTERNALS__.invoke` と
// `transformCallback` に依存するため、この 3 つを「境界そのもの」として扱う。
const TAURI_PACKAGE = '@tauri-apps';
const FORBIDDEN_GLOBALS = ['__TAURI__', '__TAURI_INTERNALS__', '__TAURI_IPC__', 'transformCallback'];

// アプリ自身の IPC モジュールは置き場の兄弟 `../ipc/{client,bindings}` にある
// （design.md「Directory Structure」: `src/shared/` の兄弟が `src/ipc/`）。
// ディレクトリ自体（`../ipc` / `../ipc/`）への参照も境界への参照として扱う
// （`src/ipc/index.ts` は今は無いが、置き場を指す形を素通しにしない）。
const IPC_DIRECTORY = path.resolve(area, '..', 'ipc');
const IPC_MODULES = ['client', 'bindings'].map((name) => path.join(IPC_DIRECTORY, name));
const BARE_IPC_SPECIFIER = /(^|[/\\])ipc[/\\](client|bindings)([/\\]|$)/;

const violations = [];
const reported = new Set();

function relative(file) {
  const rel = path.relative(process.cwd(), file);
  return rel === '' ? file : rel;
}

function report(sourceFile, node, message, detail) {
  const position = sourceFile.getLineAndCharacterOfPosition(node.getStart(sourceFile));
  const line = position.line + 1;
  const key = relative(sourceFile.fileName) + ':' + line + ':' + message + ':' + detail;
  if (reported.has(key)) {
    return;
  }
  reported.add(key);
  violations.push({
    file: relative(sourceFile.fileName),
    line,
    column: position.character + 1,
    message,
    detail,
  });
}

function stripExtension(file) {
  const ext = path.extname(file).toLowerCase();
  return EXTENSIONS.indexOf(ext) === -1 ? file : file.slice(0, -ext.length);
}

function stripTrailingSeparators(file) {
  return file.replace(/[/\\]+$/, '');
}

// 通信境界の照合はこの 1 関数だけが行う。**呼び出し側は「モジュール指定子の位置か」
// 「ただの文字列か」を区別しない** — 構文木に現れるすべての文字列・テンプレート literal が
// 同じ経路を通る。位置で規則を分けると、`const spec = "../ipc/client"; import(spec);` の
// ように完全な指定子を literal に保持して渡す迂回が片方の規則だけをすり抜ける
// （`@tauri-apps` は検出できて IPC 側だけ検出できない、という非対称が実際に生じた）。
function checkBoundaryText(sourceFile, node) {
  const text = node.text;
  if (text.indexOf(TAURI_PACKAGE) !== -1) {
    report(sourceFile, node, 'Tauri の API パッケージ「@tauri-apps/*」への参照', text);
  }
  if (FORBIDDEN_GLOBALS.indexOf(text) !== -1) {
    report(sourceFile, node, 'Tauri の実行時グローバル「' + text + '」への参照', text);
  }
  if (text.startsWith('./') || text.startsWith('../')) {
    const resolved = stripTrailingSeparators(stripExtension(path.resolve(path.dirname(sourceFile.fileName), text)));
    if (IPC_MODULES.indexOf(resolved) !== -1) {
      report(sourceFile, node, 'アプリ自身の IPC モジュール「src/ipc/' + path.basename(resolved) + '.ts」への参照', text);
    } else if (resolved === IPC_DIRECTORY) {
      report(sourceFile, node, 'アプリ自身の IPC モジュール群のディレクトリ「src/ipc/」への参照', text);
    }
    return;
  }
  if (BARE_IPC_SPECIFIER.test(text)) {
    report(sourceFile, node, 'アプリ自身の IPC モジュール（src/ipc/client.ts / src/ipc/bindings.ts）を指す未解決の指定子', text);
  }
}

function checkFile(file, kind) {
  let text;
  try {
    text = fs.readFileSync(file, 'utf8');
  } catch (error) {
    console.error(label + ': ファイルを読めません: ' + relative(file));
    console.error(label + ': ' + error.message);
    process.exit(2);
  }

  const sourceFile = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, kind);
  const diagnostics = sourceFile.parseDiagnostics || [];
  if (diagnostics.length > 0) {
    const first = diagnostics[0];
    const position =
      typeof first.start === 'number'
        ? sourceFile.getLineAndCharacterOfPosition(first.start)
        : { line: 0, character: 0 };
    console.error(label + ': 構文解析に失敗しました: ' + relative(file) + ':' + (position.line + 1));
    console.error(label + ': ' + ts.flattenDiagnosticMessageText(first.messageText, ' '));
    console.error(label + ': 構文を解釈できないファイルを「依存なし」として通してはならない');
    process.exit(2);
  }

  // 構文木に現れる文字列・テンプレート literal を**すべて**照合する。`import` /
  // `export ... from` / `import =` / `require()` / 動的 `import()` の指定子も、変数や
  // 文字列に保持した literal も、同じ StringLiteral ノードなのでここを通る。テンプレート
  // literal は断片（head / middle / tail）ごとに、その断片に現れた完全な形だけを対象にする。
  function walk(node) {
    if (ts.isStringLiteralLike(node) || ts.isTemplateLiteralToken(node)) {
      checkBoundaryText(sourceFile, node);
    } else if (ts.isIdentifier(node) && FORBIDDEN_GLOBALS.indexOf(node.text) !== -1) {
      report(sourceFile, node, 'Tauri の実行時グローバル「' + node.text + '」への参照', node.text);
    }
    ts.forEachChild(node, walk);
  }

  walk(sourceFile);
}

function collect(dir, files) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  entries.sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  for (const entry of entries) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'node_modules') {
        collect(full, files);
      }
    } else if (entry.isFile()) {
      const kind = KINDS[path.extname(entry.name).toLowerCase()];
      if (kind) {
        files.push({ file: full, kind });
      }
    }
  }
  return files;
}

let files;
try {
  files = collect(area, []);
} catch (error) {
  console.error(label + ': 置き場を走査できません: ' + areaLabel);
  console.error(label + ': ' + error.message);
  process.exit(2);
}

for (const entry of files) {
  checkFile(entry.file, entry.kind);
}

if (violations.length > 0) {
  console.error(label + ': 逸脱を ' + violations.length + ' 件検出しました: ' + areaLabel);
  for (const violation of violations) {
    console.error(
      '  - ' +
        violation.file +
        ':' +
        violation.line +
        ':' +
        violation.column +
        ': ' +
        violation.message +
        '（指定子・名前「' +
        violation.detail +
        '」）',
    );
  }
  console.error(
    label +
      ': 配信先中立な資産は Tauri の通信境界に依存できない（要件 9.6。破ると form-web-server が成立しない）',
  );
  process.exit(1);
}

console.log(label + ': OK 通信境界への依存なし: ' + areaLabel + '（走査 ' + files.length + ' ファイル）');
NODE
