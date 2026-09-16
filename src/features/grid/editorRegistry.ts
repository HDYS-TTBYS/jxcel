/**
 * セル入力手段の登録簿（tasks.md 7.4。design.md「EditorRegistry（拡張点の所有者）」。
 * data-grid 要件 3.1, 3.2, 3.8, 10.1, 10.2, 10.3, 10.4, 10.5, 10.6）。
 *
 * # 何を担うか
 *
 * **型の札から入力手段を引く表**である。グリッドの画面（8.3）は列の札だけを持ち、どの面を
 * 出すかを本 module に聞く。ユーザー定義型を所有する機能（`custom-types`）は、自分の型の面を
 * ここへ登録する — **要件 10.3「入力手段の追加は、登録簿への登録のみで成立する」**。
 *
 * # 拡張点は所有者と実装者を分ける（要件 10.3、`structure.md`）
 *
 * 型ごとの対応を**画面の側に書かない**。「この札ならこの部品」と分岐し始めると、拡張する側が
 * 画面を書き換えることになり、拡張点が拡張点でなくなる（`structure.md`「拡張点は所有者と
 * 実装者を分ける」）。したがって:
 *
 * - 対応表は `editors/index.ts` の **1 箇所**にある（組込の 10 種。要件 10.2）
 * - 画面・窓・描画の側は**入力手段の成分を名指ししない**。`resolve` だけを通る
 *   （`editorRegistry.test.ts` が源の走査で固定する）
 * - 本 module が持つのは**既定**（要件 10.4）と、重複の報告（要件 10.6）だけである
 *
 * 本 module は**判定を持たない**。値が型に適合するか、変換が起きたか、違反かどうかを決めるのは
 * `schema-engine` である（要件 3.3、3.4、3.5）。面が渡すのは**打たれた文字**であり、本 module は
 * その文字を解釈しない。
 *
 * # 型の札は生成物から取り込む（**本 module で定義し直さない**）
 *
 * [`TypeKindTag`] は境界用の型の生成物（`src/ipc/bindings.ts`。`cargo run -p app-shell --bin
 * generate-bindings` が作る）から取り込む。写しを 2 つ持つと、`schema-engine` の `TypeKind` に
 * 変種が 1 つ増えたときに片方だけが古くなり、気づけない（design.md「EditorRegistry」の Risks は
 * 同じ理由を書いている）。取り込みは**型だけ**（`import type`）なので実行時には消える。
 *
 * # 事後条件: `resolve` は必ず成分を返す（要件 10.4）
 *
 * 未登録の札は**既定の文字入力**（[`TextEditor`]）へ落ちる。画面は「面が無い」場合を扱わない —
 * 列の宣言が壊れていて札が読めない場合（生成物の `ColumnDescriptor.kind` が `null`）も、
 * 画面は `"Any"` としてここへ来る。したがって `resolve` は**投げない**。
 *
 * # 不変条件: 同一の鍵への重複登録は報告する（要件 10.6）
 *
 * 鍵は `(kind, customTypeId)` の対である。`customTypeId` は `kind` が `"Custom"` のときだけ
 * 伴う（design.md の前提条件）— 伴わない `Custom` と、`Custom` 以外に付いた `customTypeId` は
 * どちらも登録側の誤りであり、`register` が報告する。**報告は例外である**（登録を行った側は
 * 拡張の実装者であり、黙って上書きされると自分の面がいつ出るのか分からなくなる）。
 *
 * # `ColumnConstraints` — design が名指ししたが定義していない型
 *
 * design.md の `CellEditorProps` は `constraints: ColumnConstraints` を持つが、この型の定義は
 * どこにも無い。**本 module が 7.4 として定める**（下の doc を参照）。
 *
 * ## 境界に足りないもの（**申し送り。2026-09-17 にタスク 10.3 がすべて閉じた**）
 *
 * かつては 2 つの情報が**境界から来なかった**。いまはどちらも `ColumnDescriptor` が運び、
 * `./columnConstraints` が面の読む形へ写す。
 *
 * 1. **選択肢**（`choices`）— 要件 3.2 が求める「選択肢を持つ型の一覧」の材料である。
 *    生成物の `ColumnDescriptor` は `column / path / name / kind / element_count /
 *    expandability` しか運ばず、列の宣言（`schema-engine` の `enum`）の選択肢は**境界に
 *    1 つも現れなかった**。いまは `ColumnDescriptor.choices` として来る（値と名の対）
 * 2. **参照先**（`reference`）— 要件 3.8 が求める「参照先のシートに存在する行からの選択」の
 *    材料である。参照先のシートも、そのシートの行を一覧する経路も**境界に無かった**
 *    （6.1 が定めたコマンドは 6 本である）。いまは参照先のシートの名が
 *    `ColumnDescriptor.reference_sheet` として来て、その行は**コマンド 1 本**
 *    （`grid_reference_rows`。10.3 が足した 7 本目）が**頁ごとに**返す
 *
 * 3 つ目は**ユーザー定義型の同一性**である — `kind` は札しか運ばないため、`resolve` の
 * `customTypeId` に渡す値の出所が境界に無く、`Custom` の列は既定の面へ落ちていた。いまは
 * `ColumnDescriptor.custom_type_id` として来り、`editors/index.ts` の `columnEditor` が
 * 登録簿へ渡す（10.3 が閉じた）。
 *
 * **本 module は境界の型を勝手に増やさない**（`src/ipc/bindings.ts` は生成物であり、
 * 手で編集しない。`crates/app-shell/tests/bindings_drift.rs` のドリフト検査が固定している）。
 * したがって面は `constraints` を**呼び出し元が渡すもの**として扱い、渡されないときは値を
 * そのまま扱う既定へ落ちる（要件 10.4）— 境界が広がった日に**面を書き換えずに一覧になる**という
 * 形はそのままである。変わったのは渡す側（`./columnConstraints` と `./GridScreen`）であり、
 * 材料の無い欄は依然として渡されない。
 *
 * # 既定の登録簿は本 module に無い
 *
 * 本 module が与えるのは**器**（[`createEditorRegistry`]）と、器が必ず返す既定の面
 * （[`TextEditor`]）である。組込を入れた既定の登録簿は `editors/index.ts` が持つ —
 * 所有者が組込の表を知ると、拡張点が「所有者と実装者」に分かれなくなる（`structure.md`）。
 * 画面（8.3）と拡張する側の取り込み口も `editors/index.ts` である。
 */
import type { ComponentType } from "react";

import type { TypeKindTag } from "../../ipc/bindings";
import { TextEditor } from "./editors/text";

/**
 * 選択肢を持つ型（`Enum`）が提示する 1 件。
 *
 * **値と名を分ける。**確定するのは `value` であり（打たれた文字として境界へ渡る。要件 3.3）、
 * 人が読むのは `label` である。同じ文字列を使ってしまうと、表示名を直したときに保存済みの値が
 * 動く — 値は宣言が決める識別子であり、表示名は見せ方である。
 */
export interface EnumChoice {
  readonly value: string;
  readonly label: string;
}

/**
 * シート間参照（`Ref`）の参照先の 1 行。
 *
 * **確定するのは `id`**（行の識別子）であり、人が読むのは `label` である。参照の値は
 * 「どの行か」を指すのであって、その行の表示文字列ではない（表示は並べ替えや絞り込みで変わる）。
 */
export interface ReferenceRow {
  readonly id: string;
  readonly label: string;
}

/** シート間参照の参照先（要件 3.8）: 参照先のシートと、そこに存在する行。 */
export interface ReferenceSource {
  readonly sheet: string;
  readonly rows: readonly ReferenceRow[];
}

/**
 * 入れ子の型（`Object` / `Array`）の 1 つの位置。
 *
 * `name` は構造表現の鍵である（オブジェクトならフィールド名、並びなら 0 起点の添字の文字列）。
 * その位置の型の札は [`ColumnConstraints.kind`] が持つ（2 箇所に書かない）。
 */
export interface ColumnMember {
  readonly name: string;
  /** ユーザー定義型の位置である場合の型の識別子（`resolve` へそのまま渡す）。 */
  readonly customTypeId?: string;
  readonly constraints: ColumnConstraints;
}

/**
 * 列の宣言のうち、**入力手段が読むもの**（design.md の `CellEditorProps.constraints`）。
 *
 * design.md はこの型を名指しするが定義していないため、7.4 が**面が必要とする最小**を定める。
 * 欄は 5 つであり、いずれも「その面が何を提示できるか」を決める。
 *
 * | 欄 | 何のためか | どの要件か |
 * |---|---|---|
 * | `kind` | 葉の型の札。同じ面が札で振る舞いを変える（数値の面は `Int` を 1 刻みに、`Float` を縛らずに扱い、入れ子の面は位置ごとの面をこの札で登録簿から引く）。`CellEditorProps` は札を別に持たないため、**宣言が運ぶ** | 3.1, 3.2 |
 * | `nullable` | 値なしを許すか。キーだけで取り消せない面（暦・一覧・二値・参照）は、これが真のときだけ `値なし` の道を出す（空の文字列が値なしである。`crates/data-grid/src/edit/mod.rs` の `edited_value`） | 3.7 |
 * | `choices` | 選択肢を持つ型の一覧（`Enum`）。`ColumnDescriptor.choices` から写る（10.3 が閉じた申し送り） | 3.2 |
 * | `reference` | 参照先のシートとその行（`Ref`）。シートの名は `ColumnDescriptor.reference_sheet`、行は `grid_reference_rows` の頁から写る（10.3 が閉じた申し送り） | 3.8 |
 * | `members` | 入れ子の位置ごとの宣言（`Object` / `Array`）。`ColumnDescriptor.members` から写り、`nested` の面はこれがあるときだけ位置ごとの面を出す | 5.1, 5.5 |
 *
 * **渡されない欄は「その面が提示できない」であって、誤りではない。**面はそのとき、値をそのまま
 * 扱う既定（[`TextEditor`]）へ落ちる（要件 10.4）— 制約の材料が無いことは、値を打てないこと
 * ではない。材料を渡すのは `./columnConstraints`（境界の材料から写す）と `./GridScreen`
 * （参照先の行は頁ごとに読んでから載せる）である。
 */
export interface ColumnConstraints {
  readonly kind: TypeKindTag;
  readonly nullable: boolean;
  readonly choices?: readonly EnumChoice[];
  readonly reference?: ReferenceSource;
  readonly members?: readonly ColumnMember[];
}

/**
 * 入力手段が受け取るもの（design.md の `CellEditorProps`。**逐語で固定されている**）。
 *
 * 面は**打たれた文字**（`commit`）と**取り消し**（`cancel`）だけを外へ出す。判定も履歴も知らない
 * （要件 3.3、3.6 — 確定したあとに何が起きるかは画面と `schema-engine` が決める）。
 */
export interface CellEditorProps {
  /** いまの値の表示文字列（`view` 層の `display_text` が作ったもの。解釈済みの値ではない）。 */
  readonly initialText: string;
  readonly constraints: ColumnConstraints;
  readonly commit: (text: string) => void;
  readonly cancel: () => void;
}

/**
 * **確定の文字をどの命令へ載せるか**（tasks.md 8.5 が足した設計の改訂。design.md の
 * Revalidation Triggers「確定の文字の運び手を登録に足す設計の改訂」）。
 *
 * 入力手段が出す口は `commit(text)` の 1 本である（`CellEditorProps`。design.md が逐語で
 * 固定している）。ところがその文字の**意味**は 2 通りある。
 *
 * | 運び手 | 何を運ぶか | 宛先の命令 |
 * |---|---|---|
 * | `text` | **打たれた文字そのもの**（型の解釈は `schema-engine` が行う。要件 3.3） | `SetCells`（`GridEditCommand`） |
 * | `structure` | セル値の**構造表現**（JSON。要件 5.5、5.7） | `SetNested` |
 *
 * **`structure` が要る理由**（7.4 の申し送り 4 の実測）: `Text` → `object` / `array` の変換の行が
 * `schema-engine` の変換の表に無く（`crates/schema-engine/src/coerce/mod.rs` の `_` の腕）、
 * `object` の列が受理するのは `Object` の値だけである（同 `types/mod.rs` の
 * `accepted_variants`）。したがって入れ子の面が組み立てた構造表現を `SetCells` に載せると
 * **必ず違反になる** — 正しい運び手は `SetNested` である。
 *
 * **画面は列の札で経路を選ばない。**どちらの命令へ載せるかは**登録が宣言する**ので、画面は
 * 登録簿へ問い合わせるだけでよい（要件 10.3「入力手段の追加は登録簿への登録のみで成立する」。
 * 画面が型ごとに分岐すると、ユーザー定義型の面が自分の運び手を宣言できなくなる）。
 */
export type EditCarrier = "text" | "structure";

/** 登録の 1 件（design.md の `CellEditorRegistration`。**8.5 が `carrier` を足した**）。 */
export interface CellEditorRegistration {
  readonly kind: TypeKindTag;
  /** `kind` が `"Custom"` のときだけ伴う（前提条件）。 */
  readonly customTypeId?: string;
  readonly component: ComponentType<CellEditorProps>;
  /**
   * この面が確定する文字を、**どの命令へ載せるか**（上の [`EditCarrier`]）。
   *
   * **必須である。**既定を置くと、運び手を宣言し忘れた登録が黙って `SetCells` へ流れ、
   * その面の確定が**必ず違反になる**（入れ子の場合）— 気づけるのは利用者が違反を見たときで
   * ある。登録を行った側（拡張の実装者）にその場で報告するのが、7.4 の重複の報告と同じ規律で
   * ある（要件 10.6）。
   */
  readonly carrier: EditCarrier;
}

/** 登録簿の公開面（design.md の `CellEditorRegistry`。**8.5 が `resolveCarrier` を足した**）。 */
export interface CellEditorRegistry {
  register(registration: CellEditorRegistration): void;
  resolve(kind: TypeKindTag, customTypeId?: string): ComponentType<CellEditorProps>;
  /**
   * その札の面が確定する文字の**運び手**（上の [`EditCarrier`]）。
   *
   * [`CellEditorRegistry.resolve`] と**同じ 1 件の登録**を引く（成分と運び手が別の登録を指す
   * 経路を作らない）。未登録の札は `"text"` である — 既定の面（[`TextEditor`]）は値をそのまま
   * 扱う面であり、その文字は `SetCells` の「打たれた文字」である（要件 10.4）。
   */
  resolveCarrier(kind: TypeKindTag, customTypeId?: string): EditCarrier;
}

/** 登録が拒まれた理由（要件 10.6 と、design.md の前提条件）。 */
export type EditorRegistrationFailure =
  | "duplicate"
  | "customTypeIdRequired"
  | "customTypeIdForbidden"
  | "unknownCarrier";

/**
 * 登録が拒まれたこと（要件 10.6「重複を検出し、登録を行った側へ報告する」）。
 *
 * 報告は例外である。**登録を行った側は拡張の実装者**であり、黙って上書きされると自分の面が
 * いつ出るのか分からなくなる（逆に、登録を消す道は用意しない — 面が消えると、その列は既定へ
 * 落ちる。拡張の取り消しが要る日が来たら、そのときに設計する）。
 */
export class EditorRegistrationError extends Error {
  readonly reason: EditorRegistrationFailure;
  readonly kind: TypeKindTag;
  readonly customTypeId: string | undefined;

  constructor(reason: EditorRegistrationFailure, kind: TypeKindTag, customTypeId: string | undefined) {
    super(describeRegistrationFailure(reason, kind, customTypeId));
    this.name = "EditorRegistrationError";
    this.reason = reason;
    this.kind = kind;
    this.customTypeId = customTypeId;
  }
}

/**
 * 登録簿を作る。
 *
 * `resolve` は**必ず成分を返す**（事後条件）。未登録は既定の文字入力へ落ちる（要件 10.4）。
 */
export function createEditorRegistry(): CellEditorRegistry {
  const registrations = new Map<string, CellEditorRegistration>();

  return {
    register(registration: CellEditorRegistration): void {
      const { kind, customTypeId, carrier } = registration;
      // 前提条件（design.md「Preconditions」）。**伴うのは Custom のときだけ**であり、
      // どちらの破り方も登録側の誤りである。
      if (kind === "Custom" && customTypeId === undefined) {
        throw new EditorRegistrationError("customTypeIdRequired", kind, customTypeId);
      }
      if (kind !== "Custom" && customTypeId !== undefined) {
        throw new EditorRegistrationError("customTypeIdForbidden", kind, customTypeId);
      }
      // 運び手も登録側の誤りである（拡張は JS からも来るため、型検査だけでは足りない）。
      if (carrier !== "text" && carrier !== "structure") {
        throw new EditorRegistrationError("unknownCarrier", kind, customTypeId);
      }
      const key = registrationKey(kind, customTypeId);
      if (registrations.has(key)) {
        throw new EditorRegistrationError("duplicate", kind, customTypeId);
      }
      registrations.set(key, registration);
    },

    resolve(kind: TypeKindTag, customTypeId?: string): ComponentType<CellEditorProps> {
      return registrations.get(registrationKey(kind, customTypeId))?.component ?? TextEditor;
    },

    resolveCarrier(kind: TypeKindTag, customTypeId?: string): EditCarrier {
      // 未登録は既定の面（値をそのまま扱う）の運び手である（要件 10.4）。
      return registrations.get(registrationKey(kind, customTypeId))?.carrier ?? "text";
    },
  };
}

/**
 * 鍵の綴り。**`Custom` だけが `customTypeId` を鍵に含める**（前提条件より、`Custom` 以外では
 * `customTypeId` は伴わないので、この関数の 2 番目の引数は読み捨てられる）。
 */
function registrationKey(kind: TypeKindTag, customTypeId: string | undefined): string {
  return kind === "Custom" ? `Custom:${customTypeId ?? ""}` : kind;
}

/** 報告の文（人が読む。鍵の名指しを省かない — どの登録が拒まれたかが分からなくなる）。 */
function describeRegistrationFailure(
  reason: EditorRegistrationFailure,
  kind: TypeKindTag,
  customTypeId: string | undefined,
): string {
  switch (reason) {
    case "duplicate":
      return `同じ鍵への入力手段の登録が 2 度目に来た: ${registrationKey(kind, customTypeId)}`;
    case "customTypeIdRequired":
      return `Custom の入力手段には customTypeId が要る: ${kind}`;
    case "customTypeIdForbidden":
      return `${kind} の入力手段に customTypeId は付けられない: ${customTypeId ?? "未指定"}`;
    case "unknownCarrier":
      // **値そのものは載せない**（載せるには誤り型の欄を増やすことになる。拡張を書いた側は
      // 自分が何を書いたかを知っており、要るのは「どの鍵の登録が拒まれたか」である）。
      return `${kind} の入力手段の運び手が text / structure のどちらでもない`;
  }
}
