//! isolate の組み立て（tasks.md 3.2。要件 2.3, 4.1, 5.1, 9.2）。
//!
//! 実行 1 回ぶんの `JsRuntime`（isolate）を作り、**op の登録・`console` の取り込み・
//! ソースマップの結線**を行い、マクロのモジュールを評価する。**isolate の生成は
//! 本モジュールの [`Isolate::build`] 1 箇所だけ**であり（design.md「Implementation Notes」の
//! 「生成は 1 箇所（`isolate.rs`）に閉じる」）、メモリの上限（タスク 1.5）はこのときに
//! `create_params` へ入る。作った isolate は [`crate::engine::actor`] の専用スレッドを離れない。
//!
//! # 3 つの結線
//!
//! | 結線 | 何をするか | 根拠 |
//! |---|---|---|
//! | op の登録 | 宣言表（`surface/declaration.rs` の [`HOST_APIS`]）を走査し、**1 行につき 1 つの op** を登録する。登録した名前は [`check_registration`] で表と両方向に突き合わせる | 決定 5 / 要件 4.1, 10.1 |
//! | `console` の取り込み | `console.log` などの出力を **op 越しにエンジンが集める**（アプリの標準出力へ漏らさない）。op は同期であるため**並びは呼び出し順のまま**残る | 要件 2.3 |
//! | ソースマップ | 変換（3.1）を [`ModuleLoader::load`] の中で走らせ、変換結果が持つ**インラインの `sourceMappingURL`** を V8 に読ませる（`deno_core` の `SourceMapper` が例外のフレームを原位置へ写す） | 要件 3.1, 9.1 |
//!
//! # 実行の形（モジュールとして実行し、戻り値は既定の輸出）
//!
//! マクロは **ES モジュールとして読み込んで評価する**（`load_main_es_module` →
//! `mod_evaluate` → `run_event_loop` → 評価の約束の状態を読む）。`execute_script` を使わない
//! のは、**変換がモジュールの解決の段で走る**ためである（design.md「Transpiler」の
//! Integration。要件 3.5 の取り込みは `import` の解決でしか現れない）。タスク 1.4 の時点は
//! 種別に関わらずソースをそのまま `execute_script` へ渡しており、**TypeScript のマクロは
//! 型注釈が構文の誤りになって走れなかった**（`engine/actor.rs` の doc にその旨が残っている）。
//! 1.4 のテストは完了値（スクリプトの最後の式の値）を固定していたが、モジュールには完了値が
//! 無いため**既定の輸出の形へ追随**した。
//!
//! **戻り値はモジュールの既定の輸出（`export default`）である。** モジュールの評価そのものは
//! 値を持たない（完了値はスクリプトの概念である）ため、値を持つ口は既定の輸出だけである。
//! 既定の輸出が無いマクロの戻り値は `undefined`（提示の面では「何も返さなかった」に見える）。
//! 提示用の表現（オブジェクトは JSON）への写しは [`presentation`] が持つ。
//!
//! # 門は op の本体の先頭にある（要件 8.1）
//!
//! 登録する op の本体は**最初に** [`check_call`] を呼ぶ。宣言表に無い名前（`UnknownApi`）と、
//! マクロが宣言していない能力を要する呼び出し（`MissingCapability`）は、**ホストの縫い目
//! （[`HostPort`]）へ届く前に**理由つきで拒まれる。`Deno.core.ops.op_host_*` を直接叩かれても
//! 門は op の内側にあるため迂回できない。
//!
//! 拒否は 2 段でマクロへ届く:
//!
//! 1. **JS の例外として投げる** — 文言は [`CallRefusal`] / [`HostError`] が持ち、スタックには
//!    **マクロの呼び出し位置**が残る（フレームはソースマップで原位置へ写る）
//! 2. その例外が実行を終わらせたとき、**記録した拒否**（[`HostRefusal`]）から
//!    `FailureKind::HostRejected { api }` と理由を組み立て、
//!    [`RunOutcome::Failed`](crate::engine::outcome::RunOutcome::Failed) としてフレームつきで返す
//!
//! 拒否は**投げた文言**と一緒に記録し、失敗の文言がその文言で終わるときだけその拒否として
//! 扱う（[`apply_refusal`]）。マクロが拒否を `try`/`catch` で握った後に別の失敗を起こしても、
//! 直前の拒否をその失敗へ結び付けないためである。
//!
//! # `console` の取り込み（要件 2.3）
//!
//! 出力は [`OutputLine`] として**実行 1 回ぶんの並び**（[`Ran::output`]）へ入る。op は同期で
//! あるため**並びは呼び出し順のまま**である。アプリの標準出力へは 1 行も漏らさない。
//!
//! # ソースマップの結線（要件 9.1）
//!
//! 変換（3.1）は [`ModuleLoader::load`] の中で 1 回だけ走り、変換結果の末尾に**インラインの
//! `sourceMappingURL`（データ URL）** が付いている。V8 はモジュールをコンパイルしたときに
//! その `sourceMappingURL` を報告し、`deno_core` の `SourceMapper` が写しを復号して
//! **例外のフレームの行・列を原位置へ置き換える**（`deno_core` の `source_map.rs` の
//! `decode_source_map` は、V8 が報告した `data:` の URL を最優先で読む）。写しを読みに行く
//! 経路は要らない（[`crate::engine::transpile`] の doc が同じ形を定めている）。
//!
//! ブートストラップ（`console` と `host` を据える JS。スクリプト名
//! [`BOOTSTRAP_SCRIPT_NAME`]）はマクロのソースではないため、**フレームから落とす**
//! （[`frames_of`]）。落とさないと、宣言に無い名前を呼んだ失敗の最も内側のフレームが
//! ブートストラップの行を指し、要件 9.1 の「マクロのソースの位置」を偽る。
//!
//! # op の本体は panic してはならない（実測。スパイクの記録を訂正する）
//!
//! op は V8 から **`extern "C"` の関数として**呼ばれる。したがって op の本体の panic は
//! 巻き戻せず、`panic_cannot_unwind` で**プロセスが abort する**。実測（本クレートのテスト。
//! デバッグビルド）: JS から `Deno.core.ops.op_panic("…")` を踏むと
//! `thread caused non-unwinding panic. aborting.` と共に **SIGABRT でテストのプロセスが
//! 落ちた**（実行の境界で `catch_unwind` を掛けても捕まらない）。
//! `tech.md`「Known Risks」1 のスパイクが記録した「op の panic を捕捉してもランタイムは
//! 生き続ける」は、この版の `deno_core`（0.412）では成り立たない。
//!
//! したがって**本モジュールの op は失敗を [`Result`] で返す**（[`enter`] / [`from_host`]）。
//! `panic!` / `unwrap` / `expect` / 添字を使わない — 書けば**アプリ全体が落ちる**のであり、
//! 「実行基盤の失敗」として扱える範囲を越える。
//!
//! # 設計から動かした 3 点（理由つき）
//!
//! 1. **宣言に無い名前も呼べる形にする**（`host` は `Proxy`）。設計の門は「表に無い名前は
//!    呼べない」を要求する（要件 10.1 の裏面）が、`host.deleteEverything` を未定義のままに
//!    すると `TypeError` になり、**理由（どの名前が宣言に無いか）が出ない**。`Proxy` の既定の
//!    腕が未登録の名前を [`op_host_unknown_call`] へ送り、そこで門が理由つきに拒む。この op は
//!    **表の外の 1 本**であり、[`check_registration`] へ渡す一覧には入れない（表に無い名前を
//!    拒むための口であり、ホスト API ではない）
//! 2. **`emit` を縫い目に置かない**（[`HostPort`] の doc）— 出力の型が `engine` にあるため
//!    `host` 層から参照できない（層の鎖）。エンジンが集めて `RunOutcome::Ran` でホストへ戻す
//! 3. **縫い目は実行の要求と一緒に渡る** — `RunRequest`（1.3 の型）は縫い目を持たず、変更集合は
//!    実行 1 回のトランザクション境界であるため、アダプタが実行ごとに 1 つ作る。したがって
//!    [`MacroActor::run`](crate::engine::actor::MacroActor::run) の引数として渡る（設計の
//!    `run(&self, request)` に対する追加である）
//!
//! # 観測（tasks.md 3.2 の受け入れ。テストとして固定してある）
//!
//! | 観測 | テスト |
//! |------|--------|
//! | 宣言にある API を呼べ、答えがマクロへ届く | `tests::宣言にあるapiを呼べる` |
//! | 宣言表の**すべての** API が縫い目の正しい口へ届く | `tests::宣言表のすべてのapiを呼べる` |
//! | 呼び出しが変更集合へ届き、件数が結果に載る | `tests::書き込みは変更集合へ届き件数が結果に載る` |
//! | 書いたセルを読むと書いた値が返る（重ね合わせ） | `tests::書いたセルを読むと書いた値が返る` |
//! | 宣言に無い API は**名前つきの理由**で拒まれ、フレームが呼び出し位置を指す | `tests::宣言に無いapiは理由つきで拒まれる` |
//! | 能力の無い API は**能力の名前**つきで拒まれ、縫い目へ届かない | `tests::能力の無いapiは能力の名前つきで拒まれる` |
//! | 縫い目が拒んだ呼び出しは**API の名前と理由**つきの失敗になる | `tests::縫い目が拒んだ呼び出しはapiの名前つきで返る` |
//! | 能力を宣言したマクロは能力を要する口を呼べる | `tests::能力を宣言したマクロは能力を要する口を呼べる` |
//! | 握った拒否を後の失敗へ結び付けない | `tests::拒否を握った後の失敗は拒否として扱わない` |
//! | `console` の出力が**並びを保って**回収される | `tests::consoleの出力は順序を保って回収される` |
//! | 例外のフレームが**TypeScript の原位置**を指す（要件 9.1） | `tests::例外のフレームはtypescriptの原位置を指す` |
//! | JavaScript のマクロは変換されない（要件 3.3） | `tests::javascriptのマクロは変換されずに走る` |
//! | 解決できない取り込みは**名前を挙げて**失敗する（要件 3.5） | `tests::解決できない取り込みは名前を挙げて失敗する` |
//! | 構文の誤りは行・列つきの変換の失敗として返る（要件 3.4） | `tests::構文の誤りは位置つきで返る` |
//! | 宣言表と実装（op）の一致を検査する（タスク 2.1） | `tests::宣言表と実装の一致を検査する` |

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;
use std::str::FromStr;
use std::sync::Arc;

use deno_core::error::{CoreError, CoreErrorKind, JsError};
// `deno_core` は `deno_error::JsErrorBox` を根に再輸出していないため、**同じ型の別名**
// （`ModuleLoaderError = JsErrorBox`）をこの名前で使う。名前を変えるのは、op が投げる例外が
// モジュールの読み込みの失敗ではないことを読んで分かるようにするためである
// （`deno_error` を直接依存にはできない — 実行基盤の版に依存を固定する方針
// （`Cargo.toml` の依存方針 4）のもとで、`deno_core` が引く版と二重管理になる）。
use deno_core::error::ModuleLoaderError as JsErrorBox;
use deno_core::futures::FutureExt;
use deno_core::{
    op2, v8, JsRuntime, ModuleId, ModuleLoadOptions, ModuleLoadReferrer, ModuleLoadResponse,
    ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier, ModuleType, OpState,
    ResolutionKind, RuntimeOptions,
};
use document_format::{CellValue, RowId, SheetId};
use schema_engine::{ColumnIndex, TypeKind};

use crate::engine::limits;
use crate::engine::outcome::{
    ChangeSummary, FailureKind, Frame, MacroFailure, OutputLevel, OutputLine, RunRequest,
};
use crate::engine::transpile::{module_name, Transpiler, MODULE_SCHEME};
use crate::host::changes::{CellWrite, Change};
use crate::host::overlay::{ColumnTypeInfo, ReadRow, RowPage, RowSpan, SheetInfo};
use crate::host::value::{to_js, JsScalar, JsView, Mapping};
use crate::host::{HostError, HostPort};
use crate::source::capability::{self, CapabilitySet};
use crate::source::record::{MacroName, MacroRecord};
use crate::surface::declaration;
use crate::surface::gate::{check_call, CallRefusal};

/// ブートストラップ（`console` と `host` を据える JS）のスクリプト名。
///
/// マクロのソースではないため、例外のフレームから落とす（[`frames_of`]）。
pub(crate) const BOOTSTRAP_SCRIPT_NAME: &str = "macro-runtime:bootstrap";

/// ホスト API の op の名前の接頭辞（`op_host_sheets` など）。
const OP_PREFIX: &str = "op_host_";

/// 宣言表の名前 → op の実体（**手で書く唯一の対応づけ**）。
///
/// 表（[`HOST_APIS`]）に足して実装を足し忘れれば [`check_registration`] が落ち、逆に表に
/// 無いホスト API を足しても落ちる（タスク 2.1 の受け入れ）。並びは表と同じにする
/// （登録の順序を人が読む表と一致させ、突き合わせを目で追えるようにする）。
const HOST_OPS: &[(&str, deno_core::OpDecl)] = &[
    ("sheets", op_host_sheets()),
    ("columns", op_host_columns()),
    ("readRange", op_host_read_range()),
    ("setCells", op_host_set_cells()),
    ("insertRows", op_host_insert_rows()),
    ("removeRows", op_host_remove_rows()),
    ("duplicateRows", op_host_duplicate_rows()),
    ("fileRead", op_host_file_read()),
    ("fileWrite", op_host_file_write()),
    ("netFetch", op_host_net_fetch()),
];

/// `console` の呼び出しの種別（**JS 側の添字がこの並びを指す唯一の対応づけ**）。
const CONSOLE_LEVELS: &[(&str, OutputLevel)] = &[
    ("log", OutputLevel::Log),
    ("info", OutputLevel::Info),
    ("warn", OutputLevel::Warn),
    ("error", OutputLevel::Error),
    ("debug", OutputLevel::Debug),
];

/// 実行 1 回ぶんの isolate。
pub(crate) struct Isolate {
    /// この実行の isolate（**専用スレッドを離れない**）。
    js: JsRuntime,
    /// 実行するモジュールの名前（`macro:…`。3.1 の [`module_name`] が唯一の源）。
    module: ModuleSpecifier,
    /// ローダーが変換に失敗した理由（あれば）。
    ///
    /// 変換の失敗（構文の誤り・読み取れない名前。要件 3.4）は [`MacroFailure`] であり、
    /// **種別と位置を持つ**。`ModuleLoader` の口は文字列の誤りしか返せないため、失敗そのものを
    /// ここに預けてエンジンが取り出す（位置を落とさない）。
    transpile_failure: Rc<RefCell<Option<MacroFailure>>>,
}

impl Isolate {
    /// 実行 1 回ぶんの isolate を組み立てる（**生成はこの 1 箇所**）。
    ///
    /// 順序に意味がある:
    ///
    /// 1. **宣言と実装の一致**（タスク 2.1）— ずれていれば実行しない。スレッドを畳まずに
    ///    [`MacroFailure`] として返すのは、actor が以後の実行も受けられるようにするためである
    /// 2. **能力の宣言の解析**（タスク 1.6）— 誤りは [`FailureKind::Source`]（実行しない）
    /// 3. **モジュールの名前**（3.1）— URL として読めない綴りはここで止める
    /// 4. isolate の生成（メモリの上限は `create_params` で入る。タスク 1.5）と **op の登録**
    /// 5. **状態の据え付け**（縫い目・宣言・出力の集め先）
    /// 6. **ブートストラップ**（`console` と `host`）の実行
    pub(crate) fn build(
        request: &RunRequest,
        port: Arc<dyn HostPort>,
    ) -> Result<Self, MacroFailure> {
        // 1. 宣言表と実装（op）の一致。片側だけを持つ状態で走らせない。
        let registered: Vec<&'static str> = HOST_OPS.iter().map(|(api, _)| *api).collect();
        declaration::check_registration(&registered).map_err(|mismatch| {
            MacroFailure::new(
                FailureKind::Execution,
                format!("ホスト API の宣言と実装が一致しない: {mismatch}"),
                Vec::new(),
            )
        })?;

        // 2. 能力の宣言（1.6）。実行の前に読むのは、op の門がこの集合を読むためである。
        let declared = capability::parse(&request.record.source).map_err(|error| {
            MacroFailure::new(FailureKind::Source, error.to_string(), Vec::new())
        })?;

        // 3. モジュールの名前（3.1 が唯一の源）。`Transpiler::transpile` も同じ判定を持ち、
        //    同じ文言で断る（ここは先に止めているだけである）。
        let name = module_name(&request.record);
        let module = ModuleSpecifier::parse(&name).map_err(|error| {
            MacroFailure::new(
                FailureKind::Transpile,
                format!("cannot use {name} as a module name: {error}"),
                Vec::new(),
            )
        })?;

        // 4. isolate。op は拡張として登録され、変換はローダーの中で走る。
        let transpile_failure = Rc::new(RefCell::new(None));
        let loader = Rc::new(MacroModuleLoader {
            record: request.record.clone(),
            transpiler: Transpiler::new(),
            module: module.clone(),
            transpile_failure: Rc::clone(&transpile_failure),
        });
        let options = RuntimeOptions {
            // メモリの上限は isolate を作るときに V8 へ渡す（タスク 1.5）。
            create_params: Some(limits::create_params(request.limits)),
            module_loader: Some(loader),
            extensions: vec![extension::macro_host::init()],
            ..Default::default()
        };
        let mut js = JsRuntime::try_new(options).map_err(|error| {
            MacroFailure::new(
                FailureKind::Execution,
                format!("実行基盤を作れない: {error}"),
                Vec::new(),
            )
        })?;

        // 5. op が読む状態を据え付ける（実行 1 回ぶん。出力の並びもここで持つ）。
        js.op_state().borrow_mut().put(RunState {
            port,
            declared,
            registered,
            output: Vec::new(),
            refusal: None,
        });

        // 6. ブートストラップ（`console` と `host`）。マクロより前に 1 度だけ走る。
        js.execute_script(BOOTSTRAP_SCRIPT_NAME, bootstrap_js())
            .map_err(|error| {
                MacroFailure::new(
                    FailureKind::Execution,
                    format!("実行基盤を据え付けられない: {error}"),
                    Vec::new(),
                )
            })?;

        Ok(Self {
            js,
            module,
            transpile_failure,
        })
    }

    /// isolate を貸す（打ち切りの見張りを張るのは actor。タスク 1.5）。
    pub(crate) fn runtime_mut(&mut self) -> &mut JsRuntime {
        &mut self.js
    }

    /// マクロのモジュールを読んで評価し、戻り値・出力・変更の件数を返す。
    ///
    /// 失敗は [`MacroFailure`] であり、[`FailureKind`] まで決まっている（拒否は
    /// [`HostRefusal`] から、例外は V8 のスタックから組み立てる）。
    pub(crate) async fn evaluate(&mut self, request: &RunRequest) -> Result<Ran, MacroFailure> {
        let name = request.record.name.clone();
        let evaluated = self.run_module(&name).await;
        // **出力と拒否は isolate から取り出す**（op が書いた場所。成功でも失敗でも読む —
        // 失敗のときは拒否を失敗へ写すために要る）。
        let run = self.take_run();

        match evaluated {
            Ok(module) => {
                let value = self.value_of(module, &name)?;
                Ok(Ran {
                    value,
                    output: run.output,
                    changes: changes_of(run.port.as_ref()),
                })
            }
            Err(failure) => Err(apply_refusal(failure, run.refusal)),
        }
    }

    /// モジュールを読み込み、イベントループを回し切って評価を終わらせ、その識別子を返す。
    ///
    /// 変換（3.1）は [`MacroModuleLoader::load_module`] の中で 1 回だけ走る。
    async fn run_module(&mut self, name: &MacroName) -> Result<ModuleId, MacroFailure> {
        let module = self
            .js
            .load_main_es_module(&self.module)
            .await
            .map_err(|error| self.load_failure(name, error))?;
        // **`resolve` を使わない**。評価の約束を取り出してからイベントループを回し、
        // 最後にその状態を読む（`tech.md`「Known Risks」1 の確定形）。
        let evaluation = self.js.mod_evaluate(module);
        self.js
            .run_event_loop(Default::default())
            .await
            .map_err(|error| failure_of_core_error(name, error))?;
        match evaluation.now_or_never() {
            Some(Ok(())) => Ok(module),
            Some(Err(error)) => Err(failure_of_core_error(name, error)),
            // イベントループが空になったのに評価が終わっていない（トップレベルの `await` が
            // 解決しない等の異常。**成功を装わない**）。
            None => Err(MacroFailure::new(
                FailureKind::Execution,
                "イベントループが空になったのに約束が未解決である".to_owned(),
                Vec::new(),
            )),
        }
    }

    /// モジュールの戻り値（既定の輸出）を提示用の表現へ写す。
    fn value_of(&mut self, module: ModuleId, name: &MacroName) -> Result<String, MacroFailure> {
        // 評価済みのモジュールの名前空間を取る（評価の後に読む）。
        let namespace = self
            .js
            .get_module_namespace(module)
            .map_err(|error| failure_of_core_error(name, error))?;
        deno_core::scope!(scope, self.js);
        let namespace = v8::Local::new(scope, &namespace);
        let Some(key) = v8::String::new(scope, "default") else {
            // 短い ASCII の綴りであり、ここで失敗するのは isolate が壊れているときだけである。
            return Err(MacroFailure::new(
                FailureKind::Execution,
                "JS の文字列を作れない".to_owned(),
                Vec::new(),
            ));
        };
        // 既定の輸出が無ければ `undefined`（モジュールの評価そのものは値を持たない。
        // モジュール docs「実行の形」）。
        let value = namespace
            .get(scope, key.into())
            .unwrap_or_else(|| v8::undefined(scope).into());
        Ok(presentation(scope, value))
    }

    /// op が書いた状態を取り出す（**実行の終わりに 1 回**）。
    ///
    /// `OpState` に置いた [`RunState`] を移し取る（`GothamState::take`）。op はこの型を
    /// 借用して読み書きするため、ここへ来る時点で借用は残っていない。
    fn take_run(&mut self) -> RunState {
        self.js.op_state().borrow_mut().take::<RunState>()
    }

    /// モジュールの読み込みの失敗を失敗へ写す（要件 3.4, 3.5）。
    ///
    /// **変換の失敗を先に見る** — ローダーが預けた [`MacroFailure`] は種別と位置を持つ
    /// （構文の誤りは行・列、読み取れない名前はその理由）。預かりが無ければここへ来るのは
    /// 2 つである:
    ///
    /// - **ローダーの解決・読み込みの失敗**（解決できない取り込み）— 文言に名前が入る（要件 3.5）
    /// - **V8 がモジュールをコンパイルしたときの構文の誤り** — 種別が JavaScript のソースに
    ///   TypeScript の綴りを書いた場合などである。**JavaScript のソースは変換しない**
    ///   （要件 3.3）ため、`deno_ast` の解析を通った後でここへ落ちる
    ///
    /// どちらも**モジュールを実行できる形にする段**の失敗であるため、種別は
    /// [`FailureKind::Transpile`] にする。JS の例外として届いたものは**位置を持つ**ので
    /// 落とさない（要件 3.4 の行・列）。
    fn load_failure(&self, macro_name: &MacroName, error: CoreError) -> MacroFailure {
        if let Some(failure) = self.transpile_failure.borrow_mut().take() {
            return failure;
        }
        let mapped = failure_of_core_error(macro_name, error);
        MacroFailure::new(FailureKind::Transpile, mapped.message, mapped.frames)
    }
}

/// 実行が走り切った結果（`RunOutcome::Ran` の中身。所要は actor が測る）。
pub(crate) struct Ran {
    /// 戻り値の提示用の表現（**オブジェクトは JSON**。要件 2.3）。
    pub(crate) value: String,
    /// `console` の出力（**並びを保つ**。要件 2.3）。
    pub(crate) output: Vec<OutputLine>,
    /// 変更の件数（種別ごと。要件 5.5）。
    pub(crate) changes: ChangeSummary,
}

/// 実行 1 回ぶんの状態（op が読み書きする。isolate の `OpState` に 1 つ置く）。
struct RunState {
    /// ホストの縫い目（アダプタが差し込む）。
    port: Arc<dyn HostPort>,
    /// マクロが宣言した能力（1.6 の解析結果）。
    declared: CapabilitySet,
    /// 登録した op の名前（門の `Unimplemented` の判定に渡す）。
    registered: Vec<&'static str>,
    /// `console` の出力（**呼び出し順**。要件 2.3）。
    output: Vec<OutputLine>,
    /// 直前に拒んだ呼び出し（あれば。失敗の種別と理由を組み立てるために読む）。
    refusal: Option<HostRefusal>,
}

/// op の本体が記録する拒否（実行の失敗へ写すために読む）。
enum HostRefusal {
    /// 門（2.1）が呼び出しを受け付けなかった（理由に能力の名前を含む）。
    Gate(CallRefusal),
    /// ホストの縫い目が拒んだ（存在しない行・範囲外の列など）。
    Host {
        /// 拒んだ API の名前（宣言表の名前。要件 9.2）。
        api: &'static str,
        /// 投げた理由（アダプタが組み立てた 1 行）。
        reason: String,
    },
}

impl HostRefusal {
    /// 投げた例外の文言（**同じ文言の失敗のときだけ**この拒否として扱う）。
    fn text(&self) -> String {
        match self {
            Self::Gate(refusal) => refusal.to_string(),
            Self::Host { reason, .. } => reason.clone(),
        }
    }

    /// 拒否を失敗へ写す（フレームは投げた例外のものをそのまま使う。要件 9.1, 9.2）。
    fn failure(&self, frames: Vec<Frame>) -> MacroFailure {
        match self {
            Self::Gate(refusal) => MacroFailure::new(
                FailureKind::HostRejected {
                    api: refusal.api().to_owned(),
                },
                refusal.to_string(),
                frames,
            ),
            Self::Host { api, reason } => MacroFailure::new(
                FailureKind::HostRejected {
                    api: (*api).to_owned(),
                },
                reason.clone(),
                frames,
            ),
        }
    }
}

/// 直前に拒んだ呼び出しを、**同じ文言の失敗のときだけ**その拒否として写す。
fn apply_refusal(failure: MacroFailure, refusal: Option<HostRefusal>) -> MacroFailure {
    match refusal {
        Some(refusal) if failure.message.ends_with(&refusal.text()) => {
            refusal.failure(failure.frames)
        }
        _ => failure,
    }
}

/// 集めた変更の件数を実行の結果の形（[`ChangeSummary`]）へ写す。
///
/// 数えるのは `host/changes.rs` の口であり、**変更集合を複製しない**（要件 11.3）。
fn changes_of(port: &dyn HostPort) -> ChangeSummary {
    let mut summary = ChangeSummary::default();
    port.with_changes(&mut |changes| {
        summary = ChangeSummary {
            set_cells: changes.cell_count(),
            inserted_rows: changes.inserted_row_count(),
            removed_rows: changes.removed_row_count(),
            duplicated_rows: changes.duplicated_row_count(),
        };
    });
    summary
}

/// op を登録した拡張（**`ops` は [`HOST_OPS`] と `console` の口から組む**）。
///
/// 拡張を起こすマクロは `pub struct` を生成するため、**私的なモジュールの中**で起こす
/// （クレートの公開面に実行基盤の内部の名前を出さない）。
mod extension {
    use super::host_ops;

    deno_core::extension!(macro_host, ops_fn = host_ops);
}

/// 登録する op の一式。
///
/// 宣言表の 1 行につき 1 つの op（[`HOST_OPS`]）と、`console` の口、そして**宣言に無い名前を
/// 拒むための** [`op_host_unknown_call`] を登録する。最後の 1 本だけはホスト API ではなく
/// 門への入口であるため、[`check_registration`] へは渡さない（モジュール docs の
/// 「設計から動かした 3 点」1）。
fn host_ops() -> Vec<deno_core::OpDecl> {
    // ホスト API の op は**接頭辞を持つ**（表の外の 1 本 — [`op_host_unknown_call`] — と
    // 区別する）。綴りを手で書く唯一の場所（[`HOST_OPS`]）が崩れていないことを確かめる。
    debug_assert!(HOST_OPS
        .iter()
        .all(|(_, decl)| decl.name.starts_with(OP_PREFIX)));
    HOST_OPS
        .iter()
        .map(|(_, decl)| *decl)
        .chain([op_console_write(), op_host_unknown_call()])
        .collect()
}

/// ブートストラップの JS（`console` と `host` を据える）。
///
/// **表から作る**。`host` の関数は [`HOST_OPS`] の 1 行につき 1 つ（op の名前は
/// `OpDecl::name` をそのまま使う）生成し、`console` の口は [`CONSOLE_LEVELS`] から作る。
/// ここに名前を手で書くと、宣言表と実行時の面がずれる（design.md 決定 5）。
fn bootstrap_js() -> String {
    let mut js = String::from(
        r#""use strict";
(() => {
  const ops = Deno.core.ops;
  // 出力の 1 行を組み立てる。オブジェクトは JSON（提示の面と同じ規則であり、文字列は
  // そのまま）。JSON にできない値は JS の文字列表現へ落とす。
  const format = (value) => {
    if (typeof value === "string") return value;
    try {
      const json = JSON.stringify(value);
      if (json !== undefined) return json;
    } catch (_) {
      // 循環などで JSON にできない値は下の文字列表現へ落とす。
    }
    return String(value);
  };
  const write = (level, args) => ops.op_console_write(level, args.map(format).join(" "));
  globalThis.console = {
"#,
    );
    for (index, (name, _)) in CONSOLE_LEVELS.iter().enumerate() {
        js.push_str(&format!("    {name}: (...args) => write({index}, args),\n"));
    }
    js.push_str(
        r#"  };
  // ホスト API（要件 4.1）。名前は宣言表（HOST_APIS）が唯一の源であり、ここは表から
  // 生成される。値は op の関数そのものであるため、**呼び出しの位置はマクロの行**になる
  // （JS の包みを挟むと、フレームが包みの行を指してしまう）。
  const host = {};
"#,
    );
    for (api, decl) in HOST_OPS {
        js.push_str(&format!("  host[{api:?}] = ops.{};\n", decl.name));
    }
    js.push_str(
        r#"  // 宣言に無い名前も**呼べる形**にする（未定義のままだと `TypeError` になり理由が出ない）。
  // 呼べば門が名前つきで拒む。
  globalThis.host = new Proxy(host, {
    get: (target, name) =>
      typeof name === "string" && Object.prototype.hasOwnProperty.call(target, name)
        ? target[name]
        : (...args) => ops.op_host_unknown_call(String(name)),
  });
})();
"#,
    );
    js
}

/// 門（2.1）を通してから状態を貸す（**op の本体の先頭で呼ぶ**）。
///
/// 通らなければ**理由を JS の例外として投げ**、同時に記録する（[`HostRefusal`]）。宣言に無い
/// 名前（`UnknownApi`）と、宣言に無い能力（`MissingCapability`）の両方がここで止まる
/// （要件 8.1, 8.3）。
fn enter<'a>(state: &'a mut OpState, api: &str) -> Result<&'a mut RunState, JsErrorBox> {
    let run = state.borrow_mut::<RunState>();
    match check_call(api, &run.declared, &run.registered) {
        Ok(_) => Ok(run),
        Err(refusal) => {
            let text = refusal.to_string();
            run.refusal = Some(HostRefusal::Gate(refusal));
            Err(JsErrorBox::generic(text))
        }
    }
}

/// ホストの縫い目を呼び、拒否（[`HostError`]）を記録して JS の例外にする（要件 5.4）。
fn from_host<T>(
    run: &mut RunState,
    api: &'static str,
    call: impl FnOnce(&dyn HostPort) -> Result<T, HostError>,
) -> Result<T, JsErrorBox> {
    match call(run.port.as_ref()) {
        Ok(value) => Ok(value),
        Err(error) => {
            let reason = error.reason().to_owned();
            run.refusal = Some(HostRefusal::Host {
                api,
                reason: reason.clone(),
            });
            Err(JsErrorBox::generic(reason))
        }
    }
}

/// シートの一覧（要件 4.1）。
#[op2]
fn op_host_sheets<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "sheets")?;
    let sheets = from_host(run, "sheets", |port| port.sheets())?;
    let array = v8::Array::new(scope, count(sheets.len())?);
    for (index, sheet) in sheets.iter().enumerate() {
        let value = sheet_value(scope, sheet)?;
        array.set_index(scope, index as u32, value.into());
    }
    Ok(array.into())
}

/// 列の宣言（要件 4.1, 4.2）。
#[op2]
fn op_host_columns<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "columns")?;
    let sheet = sheet_id(&sheet)?;
    let columns = from_host(run, "columns", |port| port.columns(sheet))?;
    let array = v8::Array::new(scope, count(columns.len())?);
    for (index, column) in columns.iter().enumerate() {
        let value = column_value(scope, column)?;
        array.set_index(scope, index as u32, value.into());
    }
    Ok(array.into())
}

/// 行の範囲の読み（要件 4.4）。**1 回の呼び出しで範囲の全部を返す。**
///
/// セルの写像は列の型が決める（`host/value.rs`）ため、列の宣言を先に読む。
#[op2]
fn op_host_read_range<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
    span: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "readRange")?;
    let sheet = sheet_id(&sheet)?;
    let span = row_span(scope, span)?;
    let columns = from_host(run, "readRange", |port| port.columns(sheet))?;
    let page = from_host(run, "readRange", |port| port.read_rows(sheet, span))?;
    page_value(scope, &page, &columns)
}

/// セルの書き込み（要件 5.1）。集約だけを行い、適用はアダプタが 1 回で行う。
#[op2]
fn op_host_set_cells<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
    writes: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "setCells")?;
    let sheet = sheet_id(&sheet)?;
    let columns = from_host(run, "setCells", |port| port.columns(sheet))?;
    let writes = cell_writes(scope, writes, &columns)?;
    from_host(run, "setCells", |port| {
        port.stage(Change::SetCells { sheet, writes })
    })?;
    Ok(v8::undefined(scope).into())
}

/// 行の追加（要件 5.3）。値の並びは列の宣言の並びと同じ順である。
#[op2]
fn op_host_insert_rows<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
    values: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "insertRows")?;
    let sheet = sheet_id(&sheet)?;
    let columns = from_host(run, "insertRows", |port| port.columns(sheet))?;
    let values = row_values(scope, values, &columns)?;
    from_host(run, "insertRows", |port| {
        port.stage(Change::InsertRows { sheet, values })
    })?;
    Ok(v8::undefined(scope).into())
}

/// 行の削除（要件 5.3）。
#[op2]
fn op_host_remove_rows<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
    rows: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "removeRows")?;
    let sheet = sheet_id(&sheet)?;
    let rows = row_ids(scope, rows)?;
    from_host(run, "removeRows", |port| {
        port.stage(Change::RemoveRows { sheet, rows })
    })?;
    Ok(v8::undefined(scope).into())
}

/// 行の複製（要件 5.3）。
#[op2]
fn op_host_duplicate_rows<'s, 'i>(
    state: &mut OpState,
    scope: &mut v8::PinScope<'s, 'i>,
    #[string] sheet: String,
    rows: v8::Local<'s, v8::Value>,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let run = enter(state, "duplicateRows")?;
    let sheet = sheet_id(&sheet)?;
    let rows = row_ids(scope, rows)?;
    from_host(run, "duplicateRows", |port| {
        port.stage(Change::DuplicateRows { sheet, rows })
    })?;
    Ok(v8::undefined(scope).into())
}

/// ファイルの読み（要件 8.1）。門は能力の宣言を要する（要件 8.3）。
#[op2]
#[string]
fn op_host_file_read(state: &mut OpState, #[string] path: String) -> Result<String, JsErrorBox> {
    let run = enter(state, "fileRead")?;
    from_host(run, "fileRead", |port| port.file_read(&path))
}

/// ファイルの書き（要件 8.1）。門は能力の宣言を要する（要件 8.3）。
#[op2(fast)]
fn op_host_file_write(
    state: &mut OpState,
    #[string] path: String,
    #[string] text: String,
) -> Result<(), JsErrorBox> {
    let run = enter(state, "fileWrite")?;
    from_host(run, "fileWrite", |port| port.file_write(&path, &text))
}

/// ネットワークの取得（要件 8.1）。門は能力の宣言を要する（要件 8.3）。
#[op2]
#[string]
fn op_host_net_fetch(state: &mut OpState, #[string] url: String) -> Result<String, JsErrorBox> {
    let run = enter(state, "netFetch")?;
    from_host(run, "netFetch", |port| port.net_fetch(&url))
}

/// 宣言に無い名前の呼び出し（**門への入口**。表の外の 1 本）。
///
/// `host` の `Proxy` が未登録の名前をここへ送る。門が `UnknownApi` として理由つきに拒む
/// （要件 10.1 の裏面）。引数は使わない — 拒むのに要るのは名前だけである。
#[op2(fast)]
fn op_host_unknown_call(state: &mut OpState, #[string] name: String) -> Result<(), JsErrorBox> {
    let run = state.borrow_mut::<RunState>();
    let Err(refusal) = check_call(&name, &run.declared, &run.registered) else {
        // 宣言にある名前をここから呼んでも**呼び出しは起きない**（この op は名前しか
        // 受け取らない）。表の外の入口であることは変わらない。
        return Ok(());
    };
    let text = refusal.to_string();
    run.refusal = Some(HostRefusal::Gate(refusal));
    Err(JsErrorBox::generic(text))
}

/// `console` の 1 行（要件 2.3）。**呼び出し順のまま**並びへ積む。
#[op2(fast)]
fn op_console_write(
    state: &mut OpState,
    #[smi] level: u32,
    #[string] text: String,
) -> Result<(), JsErrorBox> {
    let Some((_, level)) = CONSOLE_LEVELS.get(level as usize) else {
        // ブートストラップが渡す添字しか来ない（`console` の口は [`CONSOLE_LEVELS`] から
        // 生成される）。範囲外は**黙って別の水準にしない** — 記録の水準が化けるためである。
        return Err(JsErrorBox::generic(format!(
            "unknown console level: {level}"
        )));
    };
    let run = state.borrow_mut::<RunState>();
    run.output.push(OutputLine::new(*level, text));
    Ok(())
}

/// JS のシート識別子を文書の型へ戻す（読めなければ理由つきで拒む）。
fn sheet_id(text: &str) -> Result<SheetId, JsErrorBox> {
    SheetId::from_str(text).map_err(|_| JsErrorBox::generic(format!("invalid sheet id: {text:?}")))
}

/// JS の行識別子を文書の型へ戻す。
fn row_id(text: &str) -> Result<RowId, JsErrorBox> {
    RowId::from_str(text).map_err(|_| JsErrorBox::generic(format!("invalid row id: {text:?}")))
}

/// JS の行識別子の並びを読む。
fn row_ids<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: v8::Local<'s, v8::Value>,
) -> Result<Vec<RowId>, JsErrorBox> {
    let array = array_of(value, "行の並び")?;
    let mut rows = Vec::with_capacity(array.length() as usize);
    for index in 0..array.length() {
        rows.push(row_id(&text_of(scope, array, index, "行の識別子")?)?);
    }
    Ok(rows)
}

/// JS の範囲（`{ from, to }`）を読む（要件 4.4。0 起点・両端を含む）。
fn row_span<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: v8::Local<'s, v8::Value>,
) -> Result<RowSpan, JsErrorBox> {
    let object = v8::Local::<v8::Object>::try_from(value)
        .map_err(|_| JsErrorBox::generic("範囲がオブジェクトでない"))?;
    Ok(RowSpan::new(
        position(scope, object, "from")?,
        position(scope, object, "to")?,
    ))
}

/// JS のセルの書き込みの並び（`CellWrite[]`）を読む。
///
/// 値は `host/value.rs` の規則でセル値へ戻す（列の型が変種を決める位置は型が決め、決めない
/// 位置は**値が自分で語る**自己記述的な形として読む）。
fn cell_writes<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: v8::Local<'s, v8::Value>,
    columns: &[ColumnTypeInfo],
) -> Result<Vec<CellWrite>, JsErrorBox> {
    let array = array_of(value, "書き込みの並び")?;
    let mut writes = Vec::with_capacity(array.length() as usize);
    for index in 0..array.length() {
        let element = array
            .get_index(scope, index)
            .ok_or_else(|| JsErrorBox::generic("書き込みの要素が読めない"))?;
        let object = v8::Local::<v8::Object>::try_from(element)
            .map_err(|_| JsErrorBox::generic("書き込みがオブジェクトでない"))?;
        let row = row_id(&field_text(scope, object, "row")?)?;
        let column = position(scope, object, "column")?;
        let value_key = key(scope, "value")?;
        let value = object
            .get(scope, value_key.into())
            .unwrap_or_else(|| v8::undefined(scope).into());
        writes.push(CellWrite::new(
            row,
            ColumnIndex::new(column),
            value_from_v8(scope, value, kind_at(columns, column))?,
        ));
    }
    Ok(writes)
}

/// JS の行の並び（`CellValue[][]`）を読む（行の追加。要件 5.3）。
fn row_values<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: v8::Local<'s, v8::Value>,
    columns: &[ColumnTypeInfo],
) -> Result<Vec<Vec<CellValue>>, JsErrorBox> {
    let array = array_of(value, "行の並び")?;
    let mut rows = Vec::with_capacity(array.length() as usize);
    for index in 0..array.length() {
        let element = array
            .get_index(scope, index)
            .ok_or_else(|| JsErrorBox::generic("行の要素が読めない"))?;
        let cells = array_of(element, "行のセルの並び")?;
        let mut row = Vec::with_capacity(cells.length() as usize);
        for column in 0..cells.length() {
            let value = cells
                .get_index(scope, column)
                .ok_or_else(|| JsErrorBox::generic("セルが読めない"))?;
            row.push(value_from_v8(
                scope,
                value,
                kind_at(columns, column as usize),
            )?);
        }
        rows.push(row);
    }
    Ok(rows)
}

/// JS の値をセル値へ戻す（`host/value.rs` の写像の逆。要件 4.2, 4.3）。
///
/// 列の型が変種を決める位置（[`Mapping::Plain`]）は、値の**形**から [`JsScalar`] を組んで
/// 型に渡す（数値・真偽・文字列・`null`）。型が変種を決めない位置（自己記述的）と、素の値
/// でない形（object / array）は、**上流の wire の規則で読む** — 後者は型に合わない値も捨てずに
/// 保持し、違反として提示できる状態にするためである（要件 5.2）。
fn value_from_v8<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: v8::Local<'s, v8::Value>,
    kind: TypeKind,
) -> Result<CellValue, JsErrorBox> {
    let scalar = if value.is_null() || value.is_undefined() {
        // 値なし。`undefined` も `null` と同じ扱いにする（`CellValue` に `undefined` は無い）。
        Some(JsScalar::Null)
    } else if value.is_boolean() {
        Some(JsScalar::Bool(value.boolean_value(scope)))
    } else if value.is_number() {
        Some(JsScalar::Number(
            value
                .number_value(scope)
                .ok_or_else(|| JsErrorBox::generic("数値が読めない"))?,
        ))
    } else if value.is_string() {
        Some(JsScalar::Str(Cow::Owned(value.to_rust_string_lossy(scope))))
    } else {
        None
    };

    if Mapping::of(kind) == Mapping::Plain {
        if let Some(scalar) = scalar {
            return scalar.to_value(kind).ok_or_else(|| {
                // `Mapping::Plain` は「型が変種をただ 1 つ決める」ことを意味するため、ここへは
                // 来ない（来たら写像の規則が壊れている — 成功を装わない）。
                JsErrorBox::generic(format!("cannot map a value to {kind:?}"))
            });
        }
    }
    deno_core::serde_v8::from_v8::<CellValue>(scope, value)
        .map_err(|error| JsErrorBox::generic(format!("cannot read a cell value: {error}")))
}

/// 列の添字に対応する型（宣言より後ろのセルは型が決まらないため `Any`）。
///
/// `Any` は [`Mapping::SelfDescribing`] であり、値が自分で変種を語る形で渡る
/// （`host/value.rs` の表）。
fn kind_at(columns: &[ColumnTypeInfo], column: usize) -> TypeKind {
    columns
        .get(column)
        .map_or(TypeKind::Any, |column| column.kind)
}

/// シート 1 枚を JS の値へ写す（綴りは `types/macro-host.d.ts` の `SheetInfo`）。
fn sheet_value<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    sheet: &SheetInfo,
) -> Result<v8::Local<'s, v8::Object>, JsErrorBox> {
    let id = string(scope, &sheet.id.to_string())?;
    let name = string(scope, &sheet.name)?;
    let row_count = number(scope, sheet.row_count);
    object(
        scope,
        &[
            ("id", id.into()),
            ("name", name.into()),
            ("row_count", row_count),
        ],
    )
}

/// 列の宣言 1 列を JS の値へ写す（綴りは `.d.ts` の `ColumnTypeInfo`）。
///
/// 型の綴りは [`crate::host::value::type_kind_name`] が唯一の源である（`host` 層は
/// `engine` より下であり、層の鎖の内側である）。
fn column_value<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    column: &ColumnTypeInfo,
) -> Result<v8::Local<'s, v8::Object>, JsErrorBox> {
    let name = string(scope, &column.name)?;
    let kind = string(scope, &crate::host::value::type_kind_name(column.kind))?;
    let required = v8::Boolean::new(scope, column.required);
    let unique = v8::Boolean::new(scope, column.unique);
    object(
        scope,
        &[
            ("name", name.into()),
            ("kind", kind.into()),
            ("required", required.into()),
            ("unique", unique.into()),
        ],
    )
}

/// 範囲の読みの結果を JS の値へ写す（綴りは `.d.ts` の `RowPage`）。
fn page_value<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    page: &RowPage,
    columns: &[ColumnTypeInfo],
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    let rows = v8::Array::new(scope, count(page.rows.len())?);
    for (index, row) in page.rows.iter().enumerate() {
        let value = row_value(scope, row, columns)?;
        rows.set_index(scope, index as u32, value.into());
    }
    Ok(object(scope, &[("rows", rows.into())])?.into())
}

/// 1 行を JS の値へ写す（綴りは `.d.ts` の `ReadRow`）。
fn row_value<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    row: &ReadRow,
    columns: &[ColumnTypeInfo],
) -> Result<v8::Local<'s, v8::Object>, JsErrorBox> {
    let cells = v8::Array::new(scope, count(row.cells.len())?);
    for (column, value) in row.cells.iter().enumerate() {
        let cell = cell_value(scope, value, kind_at(columns, column))?;
        cells.set_index(scope, column as u32, cell);
    }
    let id = string(scope, &row.id.to_string())?;
    object(scope, &[("id", id.into()), ("cells", cells.into())])
}

/// 1 セルを JS の値へ写す（要件 4.2, 4.3。規則は `host/value.rs` が持つ）。
///
/// 素の写像の位置は JS のスカラー（真偽・数値・文字列・`null`）、自己記述的な位置は上流の
/// wire の規則（`CellValue` の `Serialize`）で渡す。
fn cell_value<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    value: &CellValue,
    kind: TypeKind,
) -> Result<v8::Local<'s, v8::Value>, JsErrorBox> {
    match to_js(value, kind) {
        JsView::Scalar(JsScalar::Null) => Ok(v8::null(scope).into()),
        JsView::Scalar(JsScalar::Bool(value)) => Ok(v8::Boolean::new(scope, value).into()),
        JsView::Scalar(JsScalar::Number(value)) => Ok(v8::Number::new(scope, value).into()),
        JsView::Scalar(JsScalar::Str(text)) => Ok(string(scope, &text)?.into()),
        JsView::SelfDescribing(value) => deno_core::serde_v8::to_v8(scope, value)
            .map_err(|error| JsErrorBox::generic(format!("cannot write a cell value: {error}"))),
    }
}

/// JS のオブジェクトを組む（欄の綴りは `.d.ts` と同じ）。
fn object<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    entries: &[(&str, v8::Local<'s, v8::Value>)],
) -> Result<v8::Local<'s, v8::Object>, JsErrorBox> {
    let object = v8::Object::new(scope);
    for (name, value) in entries {
        let key = string(scope, name)?;
        object.set(scope, key.into(), *value);
    }
    Ok(object)
}

/// JS の配列か確かめて取り出す。
fn array_of<'s>(
    value: v8::Local<'s, v8::Value>,
    what: &str,
) -> Result<v8::Local<'s, v8::Array>, JsErrorBox> {
    v8::Local::<v8::Array>::try_from(value)
        .map_err(|_| JsErrorBox::generic(format!("{what}が配列でない")))
}

/// 配列の要素を文字列として読む。
fn text_of<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    array: v8::Local<'s, v8::Array>,
    index: u32,
    what: &str,
) -> Result<String, JsErrorBox> {
    let value = array
        .get_index(scope, index)
        .ok_or_else(|| JsErrorBox::generic(format!("{what}が読めない")))?;
    if !value.is_string() {
        return Err(JsErrorBox::generic(format!("{what}が文字列でない")));
    }
    Ok(value.to_rust_string_lossy(scope))
}

/// オブジェクトの欄を文字列として読む。
fn field_text<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
) -> Result<String, JsErrorBox> {
    let key = key(scope, name)?;
    let value = object
        .get(scope, key.into())
        .ok_or_else(|| JsErrorBox::generic(format!("{name} が読めない")))?;
    if !value.is_string() {
        return Err(JsErrorBox::generic(format!("{name} が文字列でない")));
    }
    Ok(value.to_rust_string_lossy(scope))
}

/// オブジェクトの欄を 0 起点の位置として読む（非整数・負値は拒む）。
fn position<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    object: v8::Local<'s, v8::Object>,
    name: &str,
) -> Result<usize, JsErrorBox> {
    let key = key(scope, name)?;
    let value = object
        .get(scope, key.into())
        .ok_or_else(|| JsErrorBox::generic(format!("{name} が読めない")))?;
    let Some(number) = value.integer_value(scope) else {
        return Err(JsErrorBox::generic(format!("{name} が整数でない")));
    };
    usize::try_from(number)
        .map_err(|_| JsErrorBox::generic(format!("{name} が 0 起点の位置でない: {number}")))
}

/// JS の文字列を作る（作れなければ理由つきで失敗する。**panic しない**）。
fn string<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    text: &str,
) -> Result<v8::Local<'s, v8::String>, JsErrorBox> {
    v8::String::new(scope, text).ok_or_else(|| JsErrorBox::generic("JS の文字列を作れない"))
}

/// JS の欄の名前を作る。
fn key<'s, 'i>(
    scope: &mut v8::PinScope<'s, 'i>,
    name: &str,
) -> Result<v8::Local<'s, v8::String>, JsErrorBox> {
    string(scope, name)
}

/// JS の数値を作る（件数・行数は 2^53 の内側である）。
fn number<'s, 'i>(scope: &mut v8::PinScope<'s, 'i>, value: usize) -> v8::Local<'s, v8::Value> {
    v8::Number::new(scope, value as f64).into()
}

/// JS の配列の長さ（`v8::Array::new` は `i32` を取る）。
fn count(length: usize) -> Result<i32, JsErrorBox> {
    i32::try_from(length).map_err(|_| JsErrorBox::generic("並びが長すぎる"))
}

/// マクロのモジュールを読むローダー（**変換はここで 1 回だけ走る**。要件 3.1, 3.5）。
///
/// 解決するのは `macro:` の scheme の名前だけであり、**それ以外は解決しない**
/// （design.md「Transpiler」の Integration。標準ライブラリは `macro-stdlib` が後で足す）。
/// 解決できない取り込みは**名前を挙げて**失敗させる（要件 3.5）。
struct MacroModuleLoader {
    /// 実行するマクロの記録（ソースは保存されたバイト列のまま。要件 1.5）。
    record: MacroRecord,
    /// 変換器（状態を持たない。タスク 3.1）。
    transpiler: Transpiler,
    /// 実行するモジュールの名前（[`module_name`] が唯一の源）。
    module: ModuleSpecifier,
    /// 変換の失敗（種別と位置を落とさないため、エンジンが取り出す）。
    transpile_failure: Rc<RefCell<Option<MacroFailure>>>,
}

impl ModuleLoader for MacroModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, JsErrorBox> {
        if specifier.starts_with(MODULE_SCHEME) {
            return ModuleSpecifier::parse(specifier).map_err(|error| {
                JsErrorBox::generic(format!("cannot resolve {specifier}: {error}"))
            });
        }
        // `macro:` 以外は解決しない。**名前を挙げる**（要件 3.5）。
        Err(JsErrorBox::generic(format!(
            "cannot resolve import {specifier:?} in {referrer}"
        )))
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleLoadReferrer>,
        _options: ModuleLoadOptions,
    ) -> ModuleLoadResponse {
        ModuleLoadResponse::Sync(self.load_module(module_specifier))
    }
}

impl MacroModuleLoader {
    /// 1 つのモジュールを読む（**変換はここ**）。
    fn load_module(&self, module_specifier: &ModuleSpecifier) -> Result<ModuleSource, JsErrorBox> {
        // 実行するマクロのモジュールだけを読む。他の名前は解決しない（要件 3.5）。
        if module_specifier != &self.module {
            return Err(JsErrorBox::generic(format!(
                "macro module {module_specifier} cannot be imported"
            )));
        }
        let transpiled = self.transpiler.transpile(&self.record).map_err(|failure| {
            // 失敗そのものを預ける（`ModuleLoader` の口は文字列の誤りしか返せないため、
            // 種別と位置はここで保つ）。
            let text = failure.message.clone();
            *self.transpile_failure.borrow_mut() = Some(failure);
            JsErrorBox::generic(text)
        })?;
        Ok(ModuleSource::new(
            ModuleType::JavaScript,
            ModuleSourceCode::String(transpiled.code.into()),
            module_specifier,
            None,
        ))
    }
}

/// 戻り値を提示用の表現へ写す（要件 2.3）。
///
/// **オブジェクトは JSON** である（`JSON.stringify` と同じ。`to_string` の `[object Object]`
/// では利用者に何も伝わらない）。JSON にできない値（`undefined`・関数・Symbol・BigInt・循環）
/// は JavaScript の文字列表現へ落とす — 提示の面が「何が返ったか」を失わないためである。
fn presentation(scope: &v8::PinScope<'_, '_>, value: v8::Local<'_, v8::Value>) -> String {
    if value.is_undefined() {
        return "undefined".to_owned();
    }
    match v8::json::stringify(scope, value) {
        Some(json) => json.to_rust_string_lossy(scope),
        None => value.to_rust_string_lossy(scope),
    }
}

/// 実行の例外を失敗へ写す（要件 9.1, 9.3）。
///
/// 理由は例外の種別とメッセージ（`TypeError: …`）であり、フレームは V8 のスタックから
/// **内側（投げた位置）から外側へ**の順で取る。**TypeScript の原位置**へ写すのは
/// `deno_core` の `SourceMapper`（変換結果のインラインの写し）である。
fn failure_of_js_error(macro_name: &MacroName, error: &JsError) -> MacroFailure {
    MacroFailure::new(
        FailureKind::Execution,
        message_of(error),
        frames_of(macro_name, error),
    )
}

/// イベントループの誤りを失敗へ写す。
///
/// JS の例外（未処理の拒否など）は [`CoreErrorKind::Js`] として返るので、**例外と同じ形で
/// 理由とフレームを取り出す**（V8 のスタック本文を理由へ混ぜない。提示の組み立ては 4.4 の
/// 面の仕事である）。
fn failure_of_core_error(macro_name: &MacroName, error: CoreError) -> MacroFailure {
    match *error.0 {
        CoreErrorKind::Js(js_error) => failure_of_js_error(macro_name, &js_error),
        other => MacroFailure::new(FailureKind::Execution, other.to_string(), Vec::new()),
    }
}

/// 例外の理由。JavaScript の `Error` は名前と本文を分けて持つため、**両方を残す**
/// （`TypeError: undefined は関数ではありません`。名前を落とすと提示から情報が消える）。
fn message_of(error: &JsError) -> String {
    match (&error.name, &error.message) {
        (Some(name), Some(message)) if !name.is_empty() && !message.is_empty() => {
            format!("{name}: {message}")
        }
        (_, Some(message)) if !message.is_empty() => message.clone(),
        // `Error` でない値が投げられた場合（`throw '…'` 等）は V8 の文言を使う。
        _ => error
            .exception_message
            .trim_start_matches("Uncaught ")
            .to_owned(),
    }
}

/// V8 のスタックからフレームを取る（内側から外側へ）。
///
/// 位置を持たないフレーム（V8 の内部や `eval` の外側）は落とす — 1 起点の行・列を指せない
/// ものを「位置がある」ように見せないためである。**ブートストラップ
/// （[`BOOTSTRAP_SCRIPT_NAME`]）のフレームも落とす** — マクロの位置ではないものを
/// 「マクロのソースの位置」（要件 9.1）として見せないためである。
fn frames_of(macro_name: &MacroName, error: &JsError) -> Vec<Frame> {
    error
        .frames
        .iter()
        .filter(|frame| frame.file_name.as_deref() != Some(BOOTSTRAP_SCRIPT_NAME))
        .filter_map(|frame| {
            let line = u32::try_from(frame.line_number?).ok()?;
            let column = u32::try_from(frame.column_number.unwrap_or(1)).ok()?;
            if line == 0 {
                return None;
            }
            Some(Frame::at(
                macro_name.clone(),
                frame.function_name.clone(),
                line,
                column,
            ))
        })
        .collect()
}

/// テストが使う偽のホスト（**本番の実装ではない**）。
///
/// アダプタの実装（タスク 4.1）は文書とセッションを持つ。ここにあるのは **isolate の結線を
/// 見るための最小の代役**であり、据えたシート 1 枚の上で動く（`engine/actor.rs` のテストも
/// 使う）。据え付けた値をそのまま返すだけであり、文書も履歴も持たない。
#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::host::changes::ChangeSet;
    use crate::host::overlay::Overlay;
    use std::sync::Mutex;

    /// 据えたシート 1 枚の上で動く偽のホスト。
    #[derive(Default)]
    pub(crate) struct StubHost {
        /// 据えたシート（`None` ならシートが無い）。
        sheet: Option<SheetInfo>,
        /// 列の宣言（据えたシートのもの）。
        columns: Vec<ColumnTypeInfo>,
        /// 行（文書の順）。
        rows: Vec<ReadRow>,
        /// ファイル読みの答え。
        file: Option<String>,
        /// ネットワーク取得の答え。
        net: Option<String>,
        /// 集めた変更（適用はしない。`host/changes.rs` の集合そのもの）。
        changes: Mutex<ChangeSet>,
        /// **呼ばれた口の記録**（テストの観測。門が止めたことを確かめるために使う）。
        calls: Mutex<Vec<&'static str>>,
    }

    impl StubHost {
        /// シートもファイルも無い偽のホスト。
        pub(crate) fn empty() -> Self {
            Self::default()
        }

        /// シート 1 枚と、その列の宣言・行を据える。
        pub(crate) fn with_sheet(
            id: SheetId,
            name: &str,
            columns: Vec<ColumnTypeInfo>,
            rows: Vec<ReadRow>,
        ) -> Self {
            Self {
                sheet: Some(SheetInfo {
                    id,
                    name: name.to_owned(),
                    row_count: rows.len(),
                }),
                columns,
                rows,
                ..Self::default()
            }
        }

        /// ファイル読みの答えを据える（能力を宣言したマクロの検査で使う）。
        pub(crate) fn with_file(mut self, text: &str) -> Self {
            self.file = Some(text.to_owned());
            self
        }

        /// ネットワークの答えを据える（実行に渡す前に足す）。
        pub(crate) fn with_net(mut self, text: &str) -> Self {
            self.net = Some(text.to_owned());
            self
        }

        /// **呼ばれた口の記録**（テストの観測）。
        pub(crate) fn calls(&self) -> Vec<&'static str> {
            self.calls.lock().expect("毒されていない").clone()
        }

        /// 集めた変更の件数（セル / 追加行 / 削除行 / 複製行）。
        pub(crate) fn change_counts(&self) -> (usize, usize, usize, usize) {
            let changes = self.changes.lock().expect("毒されていない");
            (
                changes.cell_count(),
                changes.inserted_row_count(),
                changes.removed_row_count(),
                changes.duplicated_row_count(),
            )
        }

        /// 1 つのセルに集まった値（集約の観測）。
        pub(crate) fn cell_value(
            &self,
            sheet: SheetId,
            row: RowId,
            column: usize,
        ) -> Option<CellValue> {
            self.changes
                .lock()
                .expect("毒されていない")
                .cell_value(sheet, row, ColumnIndex::new(column))
                .cloned()
        }

        /// 口の呼び出しを記録する。
        fn record(&self, call: &'static str) {
            self.calls.lock().expect("毒されていない").push(call);
        }

        /// 据えたシート（無ければ理由つきで拒む）。
        fn sheet(&self) -> Result<&SheetInfo, HostError> {
            self.sheet
                .as_ref()
                .ok_or_else(|| HostError::new("this document has no sheet"))
        }
    }

    impl HostPort for StubHost {
        fn sheets(&self) -> Result<Vec<SheetInfo>, HostError> {
            self.record("sheets");
            Ok(self.sheet.iter().cloned().collect())
        }

        fn columns(&self, sheet: SheetId) -> Result<Vec<ColumnTypeInfo>, HostError> {
            self.record("columns");
            let known = self.sheet()?;
            if known.id != sheet {
                return Err(HostError::new(format!("no such sheet: {sheet}")));
            }
            Ok(self.columns.clone())
        }

        fn read_rows(&self, sheet: SheetId, span: RowSpan) -> Result<RowPage, HostError> {
            self.record("read_rows");
            let known = self.sheet()?;
            if known.id != sheet {
                return Err(HostError::new(format!("no such sheet: {sheet}")));
            }
            // **重ね合わせを見る**（自分の書き込みを読める。要件 5.1 の裏面）。アダプタの
            // 実装（タスク 4.1）は文書から読んだ行にこれを重ねる。
            let base = span
                .resolve(self.rows.len())
                .map(|range| self.rows[range].to_vec())
                .unwrap_or_default();
            let changes = self.changes.lock().expect("毒されていない");
            Ok(Overlay::new(&changes).read_range(sheet, base))
        }

        fn stage(&self, change: Change) -> Result<(), HostError> {
            self.record("stage");
            self.changes
                .lock()
                .expect("毒されていない")
                .stage(change)
                .map_err(|error| HostError::new(error.to_string()))
        }

        fn with_changes(&self, read: &mut dyn FnMut(&ChangeSet)) {
            read(&self.changes.lock().expect("毒されていない"));
        }

        fn file_read(&self, path: &str) -> Result<String, HostError> {
            self.record("file_read");
            self.file
                .clone()
                .ok_or_else(|| HostError::new(format!("cannot read {path}")))
        }

        fn file_write(&self, _path: &str, _text: &str) -> Result<(), HostError> {
            // 書ける先を持たない代役であるため、**記録だけして成功**を返す（拒否の検査は
            // 「縫い目へ届いたか」を記録で見る）。
            self.record("file_write");
            Ok(())
        }

        fn net_fetch(&self, url: &str) -> Result<String, HostError> {
            self.record("net_fetch");
            self.net
                .clone()
                .ok_or_else(|| HostError::new(format!("cannot fetch {url}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::StubHost;
    use super::*;
    use crate::engine::outcome::{
        ChangeSummary, FailureKind, Limits, MacroFailure, OutputLevel, RunOutcome, WindowLabel,
    };
    use crate::engine::transpile::Transpiler;
    use crate::source::record::MacroKind;
    use crate::surface::declaration::HOST_APIS;
    use document_format::IdFactory;
    use std::time::Duration;

    /// 記録を組み立てる。
    fn record(name: &str, kind: MacroKind, source: &str) -> MacroRecord {
        MacroRecord::new(MacroName::from(name), kind, source)
    }

    /// 実行の要求を組み立てる（上限は既定）。
    fn request(record: MacroRecord) -> RunRequest {
        RunRequest::new(record, Limits::default(), WindowLabel::from("main"))
    }

    /// 実行して結末を返す（actor を通さず、本モジュールの結線だけを見る）。
    ///
    /// 専用スレッドを立てずに current-thread のランタイムを 1 つ回す（isolate をスレッドの
    /// 外へ出さない性質は actor のテストが持つ）。
    fn evaluate(record: MacroRecord, host: &Arc<StubHost>) -> RunOutcome {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread ランタイムを作れる");
        runtime.block_on(async {
            let port: Arc<dyn HostPort> = Arc::clone(host) as Arc<dyn HostPort>;
            let request = request(record);
            let mut isolate = match Isolate::build(&request, port) {
                Ok(isolate) => isolate,
                Err(failure) => return RunOutcome::Failed { failure },
            };
            let started = std::time::Instant::now();
            match isolate.evaluate(&request).await {
                Ok(ran) => RunOutcome::Ran {
                    value: ran.value,
                    output: ran.output,
                    changes: ran.changes,
                    elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
                },
                Err(failure) => RunOutcome::Failed { failure },
            }
        })
    }

    /// 走り切った結果から戻り値を取り出す。
    fn ran_value(outcome: &RunOutcome) -> &str {
        match outcome {
            RunOutcome::Ran { value, .. } => value,
            other => panic!("最後まで走り切るはずである: {other:?}"),
        }
    }

    /// 失敗を取り出す。
    fn failed(outcome: &RunOutcome) -> &MacroFailure {
        match outcome {
            RunOutcome::Failed { failure } => failure,
            other => panic!("失敗として返るはずである: {other:?}"),
        }
    }

    /// 在庫 2 行の偽のホスト（列は `品名`（text）/ `数量`（int））。
    fn stock() -> (StubHost, SheetId, RowId) {
        let mut ids = IdFactory::default();
        let sheet = ids.new_sheet_id();
        let first = ids.new_row_id();
        let second = ids.new_row_id();
        let host = StubHost::with_sheet(
            sheet,
            "在庫",
            vec![
                ColumnTypeInfo {
                    name: "品名".to_owned(),
                    kind: TypeKind::Text,
                    required: true,
                    unique: false,
                },
                ColumnTypeInfo {
                    name: "数量".to_owned(),
                    kind: TypeKind::Int,
                    required: false,
                    unique: false,
                },
            ],
            vec![
                ReadRow {
                    id: first,
                    cells: vec![CellValue::Text("りんご".to_owned()), CellValue::Int(3)],
                },
                ReadRow {
                    id: second,
                    cells: vec![CellValue::Text("みかん".to_owned()), CellValue::Int(5)],
                },
            ],
        );
        (host, sheet, first)
    }

    /// 在庫 2 行の偽のホスト（実行に渡す形）。
    fn stock_host() -> (Arc<StubHost>, SheetId, RowId) {
        let (host, sheet, first) = stock();
        (Arc::new(host), sheet, first)
    }

    /// 宣言にある API を呼べ、答えがマクロへ届く（要件 4.1）。
    #[test]
    fn 宣言にあるapiを呼べる() {
        let (host, sheet, _) = stock_host();
        let source = format!(
            r#"
const シート = host.sheets();
const 列 = host.columns(シート[0].id);
const 頁 = host.readRange({sheet:?}, {{ from: 0, to: 1 }});
export default [シート[0].name, シート[0].row_count, 列[1].kind, 頁.rows.length, 頁.rows[0].cells[0], 頁.rows[0].cells[1]];
"#,
            sheet = sheet.to_string(),
        );
        let outcome = evaluate(record("在庫を見る", MacroKind::TypeScript, &source), &host);
        assert_eq!(
            ran_value(&outcome),
            r#"["在庫",2,"Int",2,"りんご",3]"#,
            "ホスト API の答えがマクロへ届く"
        );
    }

    /// 書き込みは変更集合へ届き、件数が実行の結果に載る（要件 5.1, 5.5）。
    #[test]
    fn 書き込みは変更集合へ届き件数が結果に載る() {
        let (host, sheet, first) = stock_host();
        let source = format!(
            r#"
const 行 = host.readRange({sheet:?}, {{ from: 0, to: 0 }}).rows[0];
host.setCells({sheet:?}, [{{ row: 行.id, column: 1, value: 20 }}]);
host.insertRows({sheet:?}, [["ぶどう", 1]]);
host.removeRows({sheet:?}, [{first:?}]);
export default 行.cells[0];
"#,
            sheet = sheet.to_string(),
            first = first.to_string(),
        );
        let outcome = evaluate(record("書き換える", MacroKind::TypeScript, &source), &host);
        match &outcome {
            RunOutcome::Ran { changes, .. } => {
                assert_eq!(1, changes.set_cells, "セルの書き込みが届いていない");
                assert_eq!(1, changes.inserted_rows, "行の追加が届いていない");
                assert_eq!(1, changes.removed_rows, "行の削除が届いていない");
            }
            other => panic!("走り切るはずである: {other:?}"),
        }
        // 変更集合そのものへ届いている（適用はアダプタが行う。タスク 4.2）。
        assert_eq!((1, 1, 1, 0), host.change_counts());
        assert_eq!(
            Some(CellValue::Int(20)),
            host.cell_value(sheet, first, 1),
            "書いた値が変更集合に入っていない"
        );
    }

    /// 書いたセルを読むと書いた値が返る（重ね合わせ。要件 5.1 の裏面）。
    #[test]
    fn 書いたセルを読むと書いた値が返る() {
        let (host, sheet, _) = stock_host();
        let source = format!(
            r#"
const 前 = host.readRange({sheet:?}, {{ from: 0, to: 1 }}).rows;
host.setCells({sheet:?}, [{{ row: 前[0].id, column: 0, value: "ぶどう" }}]);
const 後 = host.readRange({sheet:?}, {{ from: 0, to: 1 }}).rows;
export default [前[0].cells[0], 後[0].cells[0], 後[1].cells[0]];
"#,
            sheet = sheet.to_string(),
        );
        let outcome = evaluate(record("読み直す", MacroKind::TypeScript, &source), &host);
        assert_eq!(
            ran_value(&outcome),
            r#"["りんご","ぶどう","みかん"]"#,
            "自分の書き込みが読みに重なっていない"
        );
    }

    /// 宣言表の**すべての** API を呼べ、呼び出しが縫い目へ届く（要件 4.1, 8.1。tasks.md 2.1 の
    /// 「宣言にある API はすべて呼べる」）。
    ///
    /// 表の 1 行ごとに op があること（[`tests::宣言表と実装の一致を検査する`]）だけでなく、
    /// **その op が正しい縫い目の口へ繋がっていること**をここで見る（取り違えれば、記録される
    /// 口の並びが変わる）。
    #[test]
    fn 宣言表のすべてのapiを呼べる() {
        let (host, sheet, _) = stock();
        let source = format!(
            r#"
// @grant file.read, file.write, net
const シート = host.sheets();
const 列 = host.columns(シート[0].id);
const 頁 = host.readRange({sheet:?}, {{ from: 0, to: 0 }});
host.setCells({sheet:?}, [{{ row: 頁.rows[0].id, column: 1, value: 1 }}]);
host.insertRows({sheet:?}, [["ぶどう", 1]]);
host.duplicateRows({sheet:?}, [頁.rows[0].id]);
host.removeRows({sheet:?}, [頁.rows[0].id]);
const 本文 = host.fileRead("/tmp/メモ.txt");
host.fileWrite("/tmp/メモ.txt", 本文);
const 取った = host.netFetch("https://example.invalid/");
export default [シート.length, 列.length, 頁.rows.length, 本文, 取った];
"#,
            sheet = sheet.to_string(),
        );
        let host = Arc::new(host.with_file("ファイルの中身").with_net("取ってきた"));
        let outcome = evaluate(record("全部呼ぶ", MacroKind::TypeScript, &source), &host);
        assert_eq!(
            ran_value(&outcome),
            r#"[1,2,1,"ファイルの中身","取ってきた"]"#,
            "宣言表の API の答えがマクロへ届く"
        );
        assert_eq!(
            host.calls(),
            vec![
                "sheets",
                "columns",
                "columns",
                "read_rows",
                "columns",
                "stage",
                "columns",
                "stage",
                "stage",
                "stage",
                "file_read",
                "file_write",
                "net_fetch",
            ],
            "呼び出しが縫い目の正しい口へ届いていない"
        );
        let (set_cells, inserted, removed, duplicated) = host.change_counts();
        assert_eq!((1, 1, 1, 1), (set_cells, inserted, removed, duplicated));
    }

    /// 宣言に無い API は**名前つきの理由**で拒まれ、フレームが呼び出し位置を指す
    /// （要件 10.1 の裏面, 9.2）。
    #[test]
    fn 宣言に無いapiは理由つきで拒まれる() {
        let (host, _, _) = stock_host();
        let source = "const 前 = 1;\nhost.deleteEverything();";
        let outcome = evaluate(record("消す", MacroKind::TypeScript, source), &host);

        let failure = failed(&outcome);
        assert_eq!(
            failure.kind,
            FailureKind::HostRejected {
                api: "deleteEverything".to_owned()
            },
            "拒否の種別に呼んだ名前が入る: {failure:?}"
        );
        assert!(
            failure.message.contains("deleteEverything"),
            "理由に名前が入る: {}",
            failure.message
        );
        let innermost = failure.innermost().expect("呼び出し位置のフレームがある");
        assert_eq!(innermost.line, 2, "呼んだ行を指す: {innermost:?}");
        assert_eq!(innermost.macro_name.as_str(), "消す");
    }

    /// 宣言に無い能力を要する API は**能力の名前**つきで拒まれ、縫い目へ届かない
    /// （要件 8.3, 8.4）。
    #[test]
    fn 能力の無いapiは能力の名前つきで拒まれる() {
        let (host, _, _) = stock_host();
        let source = "host.fileRead(\"/etc/passwd\");";
        let outcome = evaluate(record("読む", MacroKind::TypeScript, source), &host);

        let failure = failed(&outcome);
        assert_eq!(
            failure.kind,
            FailureKind::HostRejected {
                api: "fileRead".to_owned()
            },
            "拒否の種別に API の名前が入る: {failure:?}"
        );
        assert!(
            failure.message.contains("file.read"),
            "理由に能力の名前が入る: {}",
            failure.message
        );
        assert!(
            failure.innermost().is_some(),
            "呼び出し位置のフレームがある: {failure:?}"
        );
        // **門が呼び出しの前に止める**（ホストの縫い目へ届かない）。
        assert!(
            !host.calls().contains(&"file_read"),
            "拒まれた呼び出しが縫い目へ届いた: {:?}",
            host.calls()
        );
    }

    /// ホストの縫い目が拒んだ呼び出しは、**API の名前と理由**つきの失敗になる
    /// （要件 5.4, 9.2）。理由を組み立てるのは文書を引ける側（アダプタ）である。
    #[test]
    fn 縫い目が拒んだ呼び出しはapiの名前つきで返る() {
        let (host, _, _) = stock_host();
        // 文書に無いシート（ULID の 0。据えたシートとは必ず別である）。
        let missing = "00000000000000000000000000";
        let source = format!("host.columns({missing:?});");
        let outcome = evaluate(record("無いシート", MacroKind::TypeScript, &source), &host);

        let failure = failed(&outcome);
        assert_eq!(
            failure.kind,
            FailureKind::HostRejected {
                api: "columns".to_owned()
            },
            "拒否の種別に API の名前が入る: {failure:?}"
        );
        assert!(
            failure.message.contains("no such sheet"),
            "理由がアダプタの立てたものになっていない: {}",
            failure.message
        );
        // 縫い目へは届いている（拒んだのは縫い目自身である）。
        assert_eq!(vec!["columns"], host.calls());
    }

    /// 拒否を握った後の失敗は、直前の拒否として扱わない（[`apply_refusal`] の照合）。
    ///
    /// マクロが拒否を `try`/`catch` で握って続けた場合、その後の失敗はマクロ自身の失敗で
    /// ある。記録した拒否を無条件に使うと、種別と理由が直前の拒否に化ける。
    #[test]
    fn 拒否を握った後の失敗は拒否として扱わない() {
        let (host, _, _) = stock_host();
        let source = "try {\n  host.fileRead(\"/etc/passwd\");\n} catch (_) {}\nthrow new Error(\"別の失敗\");";
        let outcome = evaluate(record("握る", MacroKind::TypeScript, source), &host);

        let failure = failed(&outcome);
        assert_eq!(
            failure.kind,
            FailureKind::Execution,
            "握った拒否が後の失敗に結び付いた: {failure:?}"
        );
        assert!(
            failure.message.contains("別の失敗"),
            "理由がマクロの失敗のままである: {}",
            failure.message
        );
    }

    /// 能力を宣言したマクロは能力を要する口を呼べる（要件 8.1）。
    #[test]
    fn 能力を宣言したマクロは能力を要する口を呼べる() {
        let host = Arc::new(StubHost::empty().with_file("ファイルの中身"));
        let source =
            "// @grant file.read\nconst 本文 = host.fileRead(\"/tmp/メモ.txt\");\nexport default 本文;";
        let outcome = evaluate(record("読む", MacroKind::TypeScript, source), &host);
        assert_eq!(ran_value(&outcome), r#""ファイルの中身""#);
        assert_eq!(vec!["file_read"], host.calls());
    }

    /// `console` の出力が**並びを保って**回収される（要件 2.3）。
    #[test]
    fn consoleの出力は順序を保って回収される() {
        let host = Arc::new(StubHost::empty());
        let source = r#"
console.log("1 行目");
const 数 = 2;
console.info("2 行目", 数);
console.warn({ 印: "3 行目" });
console.error("4 行目");
console.debug("5 行目");
export default 数;
"#;
        let outcome = evaluate(record("出す", MacroKind::JavaScript, source), &host);
        match &outcome {
            RunOutcome::Ran { output, .. } => {
                let lines: Vec<(OutputLevel, &str)> = output
                    .iter()
                    .map(|line| (line.level, line.text.as_str()))
                    .collect();
                assert_eq!(
                    lines,
                    vec![
                        (OutputLevel::Log, "1 行目"),
                        (OutputLevel::Info, "2 行目 2"),
                        (OutputLevel::Warn, r#"{"印":"3 行目"}"#),
                        (OutputLevel::Error, "4 行目"),
                        (OutputLevel::Debug, "5 行目"),
                    ],
                    "出力の並びと水準が呼び出し順のまま回収されない"
                );
            }
            other => panic!("走り切るはずである: {other:?}"),
        }
    }

    /// 例外のフレームが**TypeScript の原位置**を指す（要件 9.1）。
    ///
    /// 型だけの宣言（`type`）は変換で消えるため、生成位置は原位置から 1 行ずれる。写しが
    /// 結線されていなければ、フレームは**生成位置**（1 行ずれた行）を指す。
    #[test]
    fn 例外のフレームはtypescriptの原位置を指す() {
        let host = Arc::new(StubHost::empty());
        let source =
            "type 準備 = number;\nconst 前置き: 準備 = 1;\nthrow new Error(\"原位置で投げる\");";
        let record = record("投げる", MacroKind::TypeScript, source);

        // 生成位置を実測する（原位置と違うことをテスト自身が確かめる）。
        let transpiled = Transpiler::new().transpile(&record).expect("変換できる");
        let generated_line = transpiled
            .code
            .lines()
            .position(|line| line.contains("throw new Error"))
            .map(|index| index + 1)
            .expect("変換後のコードに throw がある");
        assert_eq!(2, generated_line, "型だけの宣言が消えて行がずれる");

        let outcome = evaluate(record, &host);
        let failure = failed(&outcome);
        let innermost = failure.innermost().expect("投げた位置のフレームがある");
        assert_eq!(
            innermost.line, 3,
            "TypeScript の原位置を指していない（生成位置 {generated_line} を指している）: {innermost:?}"
        );
        assert_eq!(innermost.macro_name.as_str(), "投げる");
    }

    /// JavaScript のマクロは変換されない（要件 3.3）。
    #[test]
    fn javascriptのマクロは変換されずに走る() {
        let host = Arc::new(StubHost::empty());
        // JavaScript として渡すと `: number` は構文の誤りである（変換しないことの裏面）。
        let outcome = evaluate(
            record("型の注釈", MacroKind::JavaScript, "const 数: number = 1;"),
            &host,
        );
        let failure = failed(&outcome);
        assert_eq!(failure.kind, FailureKind::Transpile, "{failure:?}");
        assert_eq!(
            failure.innermost().map(|frame| frame.line),
            Some(1),
            "{failure:?}"
        );

        // 型の注釈が無ければそのまま走る。
        let outcome = evaluate(
            record("そのまま", MacroKind::JavaScript, "export default 1 + 1;"),
            &host,
        );
        assert_eq!(ran_value(&outcome), "2");
    }

    /// 解決できない取り込みは**名前を挙げて**失敗する（要件 3.5）。
    #[test]
    fn 解決できない取り込みは名前を挙げて失敗する() {
        let host = Arc::new(StubHost::empty());
        let source = "import { 何か } from \"macro-stdlib\";\nexport default 何か;";
        let outcome = evaluate(record("取り込む", MacroKind::TypeScript, source), &host);
        let failure = failed(&outcome);
        assert_eq!(failure.kind, FailureKind::Transpile, "{failure:?}");
        assert!(
            failure.message.contains("macro-stdlib"),
            "解決できなかった名前が理由に入る: {}",
            failure.message
        );
    }

    /// 構文の誤りは行・列つきの変換の失敗として返る（要件 3.4。3.1 の失敗を落とさない）。
    #[test]
    fn 構文の誤りは位置つきで返る() {
        let host = Arc::new(StubHost::empty());
        let outcome = evaluate(
            record("壊れている", MacroKind::TypeScript, "const 数: = 1;"),
            &host,
        );
        let failure = failed(&outcome);
        assert_eq!(failure.kind, FailureKind::Transpile, "{failure:?}");
        let innermost = failure.innermost().expect("位置つきである");
        assert_eq!(innermost.line, 1);
        assert!(innermost.column >= 1);
    }

    /// 宣言表と実装（op）の一致を検査する（タスク 2.1 の受け入れ）。
    ///
    /// 表に足して op を足し忘れれば登録の一覧が足りず、表に無いホスト API を足せば余る。
    /// **一覧は [`HOST_OPS`]（手で書く対応づけ）から作る**ため、この検査は素通りしない。
    #[test]
    fn 宣言表と実装の一致を検査する() {
        let registered: Vec<&str> = HOST_OPS.iter().map(|(api, _)| *api).collect();
        declaration::check_registration(&registered).expect("表と実装が一致する");

        // 欠けを検出できることを、欠けを作って確かめる。
        assert!(
            declaration::check_registration(&registered[1..]).is_err(),
            "op を 1 本落としても検査が落ちない"
        );
        // 表に無いホスト API も落ちる。
        let mut extra = registered.clone();
        extra.push("deleteEverything");
        assert!(
            declaration::check_registration(&extra).is_err(),
            "表に無い名前を足しても検査が落ちない"
        );
        // 宣言表の各行に対応する op があり、名前が接頭辞を持つ（取りこぼしの検出）。
        for api in HOST_APIS {
            assert!(
                HOST_OPS
                    .iter()
                    .any(|(name, decl)| *name == api.name && decl.name.starts_with(OP_PREFIX)),
                "{} の op が無い（または名前が接頭辞を持たない）",
                api.js_name()
            );
        }
    }

    /// 出力の並びは実行をまたがない（isolate は実行ごとに作られる。design.md 決定 1）。
    #[test]
    fn 出力の並びは実行をまたがない() {
        let host = Arc::new(StubHost::empty());
        let first = evaluate(
            record(
                "1 回目",
                MacroKind::JavaScript,
                "console.log(\"1 回目\"); export default 1;",
            ),
            &host,
        );
        let second = evaluate(
            record(
                "2 回目",
                MacroKind::JavaScript,
                "console.log(\"2 回目\"); export default 2;",
            ),
            &host,
        );
        let lines = |outcome: &RunOutcome| match outcome {
            RunOutcome::Ran { output, .. } => output
                .iter()
                .map(|line| line.text.clone())
                .collect::<Vec<_>>(),
            other => panic!("走り切るはずである: {other:?}"),
        };
        assert_eq!(lines(&first), vec!["1 回目".to_owned()]);
        assert_eq!(lines(&second), vec!["2 回目".to_owned()]);
    }

    /// 既定の輸出が無いマクロの戻り値は `undefined`（モジュールの評価は値を持たない）。
    #[test]
    fn 既定の輸出が無ければ戻り値はundefined() {
        let host = Arc::new(StubHost::empty());
        let outcome = evaluate(
            record("返さない", MacroKind::JavaScript, "const 数 = 1;"),
            &host,
        );
        assert_eq!(ran_value(&outcome), "undefined");
    }

    /// メモリの上限は isolate の生成に入る（タスク 1.5 との結合点）。
    #[test]
    fn メモリの上限が生成へ入る() {
        let host = Arc::new(StubHost::empty());
        let request = RunRequest::new(
            record("走る", MacroKind::JavaScript, "export default 1;"),
            Limits::new(Duration::from_secs(5), 8 * 1024 * 1024),
            WindowLabel::from("main"),
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread ランタイムを作れる");
        let ran = runtime.block_on(async {
            let port: Arc<dyn HostPort> = Arc::clone(&host) as Arc<dyn HostPort>;
            let mut isolate = Isolate::build(&request, port).expect("isolate を組み立てられる");
            isolate.evaluate(&request).await.expect("走り切る")
        });
        assert_eq!(ran.value, "1");
        assert!(ran.output.is_empty());
        assert_eq!(ran.changes, ChangeSummary::default());
    }
}
