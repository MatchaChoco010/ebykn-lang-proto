//! §3（プロジェクト）と §6（静的検査）の診断の文言。

use super::{Code, Lang};

/// 権限の状態（仕様 §6.2 の `State` と、Γ に無いこと）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PermState {
    Owned,
    /// `BorrowedShared`。
    Shared,
    /// `BorrowedMut`。
    Mut,
    /// Γ に無い。
    Absent,
}

impl PermState {
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (PermState::Owned, Lang::En) => "owned",
            (PermState::Owned, Lang::Ja) => "所有",
            (PermState::Shared, Lang::En) => "borrowed (shared)",
            (PermState::Shared, Lang::Ja) => "共有の借用",
            (PermState::Mut, Lang::En) => "borrowed (mut)",
            (PermState::Mut, Lang::Ja) => "可変の借用",
            (PermState::Absent, Lang::En) => "not held",
            (PermState::Absent, Lang::Ja) => "なし",
        }
    }
}

/// 分岐・ループの点で食い違った権限。`a` が比べる基準の側、`b` が食い違った側。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mismatch {
    pub name: String,
    pub a: PermState,
    pub b: PermState,
}

fn mismatches(diffs: &[Mismatch], lang: Lang) -> String {
    let sep = if lang == Lang::En { ", " } else { "、" };
    diffs
        .iter()
        .map(|m| format!("`{}@` ({} / {})", m.name, m.a.text(lang), m.b.text(lang)))
        .collect::<Vec<_>>()
        .join(sep)
}

/// 権限を消費・借用する操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `p.free()`。
    Free,
    /// `put r[p];`。
    Put,
    /// `uses p@` の引数に渡す（移動）。
    Move,
    /// `leak p@;`。
    Leak,
    /// `uses &mut p@` の引数に渡す。
    MutBorrow,
    /// `uses &p@` の引数に渡す。
    SharedBorrow,
    /// `gives p@` で返す。
    Give,
}

impl Op {
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Op::Free, Lang::En) => "free it with `.free()`",
            (Op::Free, Lang::Ja) => "`.free()` で解放",
            (Op::Put, Lang::En) => "put it back with `put`",
            (Op::Put, Lang::Ja) => "`put` で戻す",
            (Op::Move, Lang::En) => "move it to a `uses p@` parameter",
            (Op::Move, Lang::Ja) => "`uses p@` の引数に移動",
            (Op::Leak, Lang::En) => "leak it with `leak`",
            (Op::Leak, Lang::Ja) => "`leak` で捨てる",
            (Op::MutBorrow, Lang::En) => "lend it to a `uses &mut p@` parameter",
            (Op::MutBorrow, Lang::Ja) => "`uses &mut p@` の引数に貸す",
            (Op::SharedBorrow, Lang::En) => "lend it to a `uses &p@` parameter",
            (Op::SharedBorrow, Lang::Ja) => "`uses &p@` の引数に貸す",
            (Op::Give, Lang::En) => "return it with `gives`",
            (Op::Give, Lang::Ja) => "`gives` で返すことが",
        }
    }
}

/// ループの中で Γ を比べた点（仕様 §6.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopPoint {
    /// 本体の終端。
    End,
    /// `continue`。
    Continue,
    /// `break`。
    Break,
}

/// 未実装の機能（`E0003`）。各マイルストーンで実装する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unimpl {
    /// `.eykp`（M3）。
    Eykp,
    /// `props/` の性質ファイル（M5）。
    PropsDir,
    /// `Ebykn.toml` の `[props]`（M5）。
    PropsTable,
    /// `Ebykn.toml` の `[ebykn-dependencies]`（M5）。
    EbyknDeps,
    /// 部分集合型（M4）。
    SubsetType,
    /// 契約・`spec fn`・`lemma fn` を持つ trait（M4）。
    ContractTrait,
    /// `impl spec`（M5）。
    ImplSpec,
    /// `check --base`（M3）。
    Base,
    /// 名前付きの消去される引数（`uses r: Region<..>`）を持つ関数の呼び出し（仕様に呼び出し側の形がない）。
    NamedUsesCall,
    /// `build --verified`（M5）。
    BuildVerified,
    /// クレートの外の型への `impl Invariant` / `impl View`（性質ファイル、M5）。
    ExternalTypeSpec,
    /// 関数の本体の中の Ebykn の項目（索引に入らない）。
    NestedItem,
    /// 中を読まないマクロの引数の中の権限の操作。
    PermInMacro,
    /// 同じモジュールに同じ名前の定義が二つ以上ある（cfg で分けた定義。索引は cfg を評価しない）。
    CfgDuplicate,
    /// 関数の本体の中の `unchecked fn` / `lemma fn`（`verify` が証明なしの前提として一覧に出せない）。
    NestedAssumption,
    /// 論理文脈の、中を読まないマクロの引数の中の `.first()` / `.last()`（#11 の置き換えが届かない）。
    MacroFirstLast,
}

impl Unimpl {
    fn text(self, lang: Lang) -> &'static str {
        match (self, lang) {
            (Unimpl::Eykp, Lang::En) => "proof files (`.eykp`) are implemented in M3",
            (Unimpl::Eykp, Lang::Ja) => "証明ファイル（`.eykp`）は M3 で実装します",
            (Unimpl::PropsDir, Lang::En) => "props files (`props/`) are implemented in M5",
            (Unimpl::PropsDir, Lang::Ja) => "性質ファイル（`props/`）は M5 で実装します",
            (Unimpl::PropsTable, Lang::En) => "`[props]` in `Ebykn.toml` is implemented in M5",
            (Unimpl::PropsTable, Lang::Ja) => "`Ebykn.toml` の `[props]` は M5 で実装します",
            (Unimpl::EbyknDeps, Lang::En) => {
                "`[ebykn-dependencies]` in `Ebykn.toml` is implemented in M5"
            }
            (Unimpl::EbyknDeps, Lang::Ja) => {
                "`Ebykn.toml` の `[ebykn-dependencies]` は M5 で実装します"
            }
            (Unimpl::SubsetType, Lang::En) => {
                "subset types (`type T = B where P;`) are implemented in M4"
            }
            (Unimpl::SubsetType, Lang::Ja) => {
                "部分集合型（`type T = B where P;`）は M4 で実装します"
            }
            (Unimpl::ContractTrait, Lang::En) => {
                "traits with contracts, `spec fn` or `lemma fn` are implemented in M4"
            }
            (Unimpl::ContractTrait, Lang::Ja) => {
                "契約・`spec fn`・`lemma fn` を持つ trait は M4 で実装します"
            }
            (Unimpl::ImplSpec, Lang::En) => "`impl spec` is implemented in M5",
            (Unimpl::ImplSpec, Lang::Ja) => "`impl spec` は M5 で実装します",
            (Unimpl::Base, Lang::En) => "`check --base` is implemented in M3",
            (Unimpl::Base, Lang::Ja) => "`check --base` は M3 で実装します",
            (Unimpl::NamedUsesCall, Lang::En) => {
                "calling a function with a named erased parameter (`uses r: Region<..>`) is not implemented yet; the specification does not say how the caller passes the region"
            }
            (Unimpl::NamedUsesCall, Lang::Ja) => {
                "名前付きの消去される引数（`uses r: Region<..>`）を持つ関数の呼び出しは未実装です。呼び出し側が領域を渡す形が仕様にありません"
            }
            (Unimpl::BuildVerified, Lang::En) => {
                "`build --verified` needs `ebykn status`, which is implemented in M5"
            }
            (Unimpl::BuildVerified, Lang::Ja) => {
                "`build --verified` は `ebykn status` が要るので M5 で実装します"
            }
            (Unimpl::ExternalTypeSpec, Lang::En) => {
                "`impl Invariant` / `impl View` for a type outside this crate belongs in a props file, which is implemented in M5"
            }
            (Unimpl::ExternalTypeSpec, Lang::Ja) => {
                "クレートの外の型への `impl Invariant` / `impl View` は性質ファイルに書くもので、M5 で実装します"
            }
            (Unimpl::NestedItem, Lang::En) => {
                "`impl Invariant`, `impl View`, structs with `owns` / `spec` and modules inside a function body are not supported yet; move them to the module level"
            }
            (Unimpl::NestedItem, Lang::Ja) => {
                "関数の本体の中の `impl Invariant`・`impl View`・`owns` / `spec` を持つ構造体・モジュールはまだ扱えません。モジュールの直下に移します"
            }
            (Unimpl::CfgDuplicate, Lang::En) => {
                "this name is defined more than once in the module (for example under different `#[cfg]`); Ebykn's index does not evaluate `#[cfg]` yet, so it cannot tell which definition remains"
            }
            (Unimpl::CfgDuplicate, Lang::Ja) => {
                "この名前はモジュールの中で二度以上定義されています（`#[cfg]` で分けた定義など）。Ebykn の索引はまだ `#[cfg]` を評価しないので、どちらの定義が残るかを決められません"
            }
            (Unimpl::NestedAssumption, Lang::En) => {
                "`unchecked fn` and `lemma fn` inside a function body are not supported yet, because `verify` could not list them as assumptions; move them to the module level"
            }
            (Unimpl::NestedAssumption, Lang::Ja) => {
                "関数の本体の中の `unchecked fn` と `lemma fn` はまだ扱えません。`verify` が証明なしの前提として一覧に出せないためです。モジュールの直下に移します"
            }
            (Unimpl::MacroFirstLast, Lang::En) => {
                "`.first()` / `.last()` in the arguments of this macro in a logical context are not supported yet (Ebykn does not read them, so they would mean vstd's `first` instead of `first_opt`); compute the value outside the macro"
            }
            (Unimpl::MacroFirstLast, Lang::Ja) => {
                "論理文脈のこのマクロの引数の中の `.first()` / `.last()` はまだ扱えません（Ebykn が読まないので、`first_opt` ではなく vstd の `first` の意味になります）。値はマクロの外で求めます"
            }
            (Unimpl::PermInMacro, Lang::En) => {
                "permissions cannot be used in the arguments of this macro yet, because Ebykn does not read them; compute the value outside the macro"
            }
            (Unimpl::PermInMacro, Lang::Ja) => {
                "このマクロの引数は Ebykn が読まないので、その中ではまだ権限を使えません。値はマクロの外で求めます"
            }
        }
    }
}

/// §3 と §6 の診断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckMsg {
    // E0002：プロジェクト
    /// `Ebykn.toml` が見つからない。
    ManifestNotFound {
        dir: String,
    },
    /// `Ebykn.toml` を TOML として読めない。`detail` は TOML の構文解析器の英語の文言。
    ManifestParse {
        detail: String,
    },
    /// `Ebykn.toml` に必要な鍵がない（`package.name` など）。
    ManifestMissing {
        key: String,
    },
    /// `Ebykn.toml` の知らない鍵。
    ManifestUnknown {
        key: String,
    },
    /// `Ebykn.toml` の値の型が違う。`expected` は英語の型名（`string`、`table`）。
    ManifestType {
        key: String,
        expected: &'static str,
    },
    /// `src/lib.eyk` も `src/main.eyk` もない。
    CrateRootMissing,
    /// `src/lib.eyk` と `src/main.eyk` の両方がある。
    CrateRootAmbiguous,
    /// `mod foo;` のファイルがない。
    ModuleNotFound {
        name: String,
        a: String,
        b: String,
    },
    /// `mod foo;` のファイルが二つある。
    ModuleAmbiguous {
        name: String,
        a: String,
        b: String,
    },
    /// 同じモジュールを二度宣言した。
    ModuleDuplicate {
        name: String,
    },
    /// `Ebykn.toml` の `[verus]` が、この `ebykn` の固定した Verus と違う。
    VerusPinMismatch {
        key: &'static str,
        written: String,
        pinned: String,
    },
    /// ファイルを読めない。
    ReadFailed {
        path: String,
        detail: String,
    },

    // E0003
    Unimplemented(Unimpl),

    // E0001
    /// `take` / `put` / `borrow` の後の `x[..]` の `x` が領域でない。
    NotARegion {
        name: String,
        keyword: &'static str,
    },
    /// `impl Invariant for T` の中身の形が違う。
    InvariantImplShape,
    /// `impl View for T` の中に `type V` と `spec fn` 以外がある。
    ViewImplShape,
    /// `Invariant` / `View` を `impl Invariant for T` 以外の綴り（パス付き、改名、`use`）で書いた。
    EbyknTraitSpelling {
        written: String,
    },

    // E0402
    /// 構造体リテラル・パターンに消去フィールドがない。
    MissingErasedField {
        ty: String,
        field: String,
        pattern: bool,
    },
    /// `@name` の `name` が消去フィールドでない。
    NotErasedField {
        ty: String,
        field: String,
    },

    // E0501
    UsesNotParam {
        name: String,
    },
    UsesType {
        name: String,
    },
    UsesRegionType {
        name: String,
    },
    GivesNotRet {
        name: String,
    },
    GivesType {
        name: String,
    },

    // E0502
    RegionIndexNotIdent,

    // E0503
    PermAlreadyHeld {
        name: String,
    },

    // E0504
    PermNotOwned {
        name: String,
        op: Op,
        state: PermState,
    },
    /// `uses` の引数に、権限を持つ変数でない式を渡した。
    UsesArgNotVar {
        param: String,
    },

    // E0505
    PermLeft {
        names: Vec<String>,
    },
    /// 権限を返す式（`Ptr::new(..)` や `gives` を持つ関数の呼び出し）を `let X = ..;` で受けていない。
    PermDiscarded,

    // E0506
    AssignHeld {
        name: String,
    },

    // E0507
    ShadowHeld {
        name: String,
    },

    // E0508
    WriteShared {
        name: String,
    },

    // E0509
    ConsumeBorrowed {
        name: String,
        op: Op,
    },

    // E0510
    BranchMismatch {
        diffs: Vec<Mismatch>,
    },

    // E0511
    LoopMismatch {
        diffs: Vec<Mismatch>,
        at: LoopPoint,
    },

    /// `borrow` ブロックを `break` / `continue` で抜ける（#21 の既定）。
    JumpOutOfBorrow {
        name: String,
        is_continue: bool,
    },

    // E0512
    PermInClosure {
        name: String,
    },

    // E0513
    SpecFieldAsRegion {
        field: String,
    },

    // E0514
    MethodCallUses {
        method: String,
    },

    // E0515
    SelfErasedWithoutSelf,

    // E0516
    AssignOwnsField {
        field: String,
    },

    // W0601
    RefinementNested {
        ty: String,
    },
}

impl CheckMsg {
    pub fn code(&self) -> Code {
        use CheckMsg::*;
        match self {
            ManifestNotFound { .. }
            | ManifestParse { .. }
            | ManifestMissing { .. }
            | ManifestUnknown { .. }
            | ManifestType { .. }
            | CrateRootMissing
            | CrateRootAmbiguous
            | ModuleNotFound { .. }
            | ModuleAmbiguous { .. }
            | ModuleDuplicate { .. }
            | VerusPinMismatch { .. }
            | ReadFailed { .. } => Code::E0002,
            Unimplemented(_) => Code::E0003,
            NotARegion { .. } | InvariantImplShape | ViewImplShape | EbyknTraitSpelling { .. } => {
                Code::E0001
            }
            MissingErasedField { .. } | NotErasedField { .. } => Code::E0402,
            UsesNotParam { .. }
            | UsesType { .. }
            | UsesRegionType { .. }
            | GivesNotRet { .. }
            | GivesType { .. } => Code::E0501,
            RegionIndexNotIdent => Code::E0502,
            PermAlreadyHeld { .. } => Code::E0503,
            PermNotOwned { .. } | UsesArgNotVar { .. } => Code::E0504,
            PermLeft { .. } | PermDiscarded => Code::E0505,
            AssignHeld { .. } => Code::E0506,
            ShadowHeld { .. } => Code::E0507,
            WriteShared { .. } => Code::E0508,
            ConsumeBorrowed { .. } => Code::E0509,
            BranchMismatch { .. } => Code::E0510,
            LoopMismatch { .. } | JumpOutOfBorrow { .. } => Code::E0511,
            PermInClosure { .. } => Code::E0512,
            SpecFieldAsRegion { .. } => Code::E0513,
            MethodCallUses { .. } => Code::E0514,
            SelfErasedWithoutSelf => Code::E0515,
            AssignOwnsField { .. } => Code::E0516,
            RefinementNested { .. } => Code::W0601,
        }
    }

    pub fn text(&self, lang: Lang) -> String {
        use CheckMsg::*;
        use Lang::{En, Ja};
        match (self, lang) {
            (ManifestNotFound { dir }, En) => {
                format!("`Ebykn.toml` was not found in `{dir}` or its parent directories")
            }
            (ManifestNotFound { dir }, Ja) => {
                format!("`{dir}` とその上のディレクトリに `Ebykn.toml` がありません")
            }
            (ManifestParse { detail }, En) => format!("cannot read `Ebykn.toml`: {detail}"),
            (ManifestParse { detail }, Ja) => {
                format!("`Ebykn.toml` を読めません: {detail}")
            }
            (ManifestMissing { key }, En) => format!("`Ebykn.toml` has no `{key}`"),
            (ManifestMissing { key }, Ja) => format!("`Ebykn.toml` に `{key}` がありません"),
            (ManifestUnknown { key }, En) => format!("unknown key `{key}` in `Ebykn.toml`"),
            (ManifestUnknown { key }, Ja) => format!("`Ebykn.toml` の `{key}` は知らない鍵です"),
            (ManifestType { key, expected }, En) => {
                format!("`{key}` in `Ebykn.toml` must be a {expected}")
            }
            (ManifestType { key, expected }, Ja) => {
                let t = match *expected {
                    "string" => "文字列",
                    "table" => "表",
                    _ => expected,
                };
                format!("`Ebykn.toml` の `{key}` は{t}にします")
            }
            (CrateRootMissing, En) => {
                "the crate root is missing; create `src/lib.eyk` or `src/main.eyk`".into()
            }
            (CrateRootMissing, Ja) => {
                "クレートルートがありません。`src/lib.eyk` か `src/main.eyk` を作ります".into()
            }
            (CrateRootAmbiguous, En) => {
                "both `src/lib.eyk` and `src/main.eyk` exist; keep only one crate root".into()
            }
            (CrateRootAmbiguous, Ja) => {
                "`src/lib.eyk` と `src/main.eyk` の両方があります。クレートルートは一つにします"
                    .into()
            }
            (ModuleNotFound { name, a, b }, En) => {
                format!("file not found for module `{name}`; create `{a}` or `{b}`")
            }
            (ModuleNotFound { name, a, b }, Ja) => {
                format!("モジュール `{name}` のファイルがありません。`{a}` か `{b}` を作ります")
            }
            (ModuleAmbiguous { name, a, b }, En) => {
                format!("module `{name}` has two files, `{a}` and `{b}`; remove one")
            }
            (ModuleAmbiguous { name, a, b }, Ja) => format!(
                "モジュール `{name}` のファイルが `{a}` と `{b}` の二つあります。一つにします"
            ),
            (ModuleDuplicate { name }, En) => format!("module `{name}` is declared twice"),
            (ModuleDuplicate { name }, Ja) => format!("モジュール `{name}` を二度宣言しています"),
            (VerusPinMismatch { key, written, pinned }, En) => format!(
                "`verus.{key}` in `Ebykn.toml` is `{written}`, but this `ebykn` uses the Verus pinned at `{pinned}`"
            ),
            (VerusPinMismatch { key, written, pinned }, Ja) => format!(
                "`Ebykn.toml` の `verus.{key}` は `{written}` ですが、この `ebykn` が使う固定した Verus は `{pinned}` です"
            ),
            (ReadFailed { path, detail }, En) => format!("cannot read `{path}`: {detail}"),
            (ReadFailed { path, detail }, Ja) => format!("`{path}` を読めません: {detail}"),

            (Unimplemented(u), En) => format!("not implemented yet: {}", u.text(En)),
            (Unimplemented(u), Ja) => format!("未実装: {}", u.text(Ja)),

            (NotARegion { name, keyword }, En) => format!(
                "syntax error: `{name}` is not a region, so `{keyword} {name}[..]` cannot be written; a region is an `owns` field (`self@{name}`) or a variable of type `Region<..>`"
            ),
            (NotARegion { name, keyword }, Ja) => format!(
                "構文エラー: `{name}` は領域ではないので、`{keyword} {name}[..]` とは書けません。領域は `owns` フィールド（`self@{name}`）か `Region<..>` 型の変数です"
            ),
            (InvariantImplShape, En) => {
                "syntax error: `impl Invariant for T` must contain exactly one item, `spec fn invariant(&self) -> bool = ..;`".into()
            }
            (EbyknTraitSpelling { written }, En) => format!(
                "syntax error: `{written}` cannot be written; Ebykn's `Invariant` and `View` are always in scope and are written only as `impl Invariant for T` / `impl View for T` (no path, rename or generics)"
            ),
            (EbyknTraitSpelling { written }, Ja) => format!(
                "構文エラー: `{written}` とは書けません。Ebykn の `Invariant` と `View` は常に使える名前で、`impl Invariant for T` / `impl View for T` とだけ書きます（パス・改名・ジェネリクスなし）"
            ),
            (ViewImplShape, En) => {
                "syntax error: `impl View for T` can only contain `type V = ..;` and `spec fn view(&self) -> V = ..;`".into()
            }
            (ViewImplShape, Ja) => {
                "構文エラー: `impl View for T` の中には `type V = ..;` と `spec fn view(&self) -> V = ..;` だけを書きます".into()
            }
            (InvariantImplShape, Ja) => {
                "構文エラー: `impl Invariant for T` の中には `spec fn invariant(&self) -> bool = ..;` を一つだけ書きます".into()
            }

            (MissingErasedField { ty, field, pattern }, En) => {
                if *pattern {
                    format!(
                        "the pattern for `{ty}` omits the `owns` field `{field}`; write `@{field}` or `@..` (which leaks the permissions)"
                    )
                } else {
                    format!(
                        "the literal of `{ty}` does not give the erased field `{field}`; write `@{field}: ..` or `..base`"
                    )
                }
            }
            (MissingErasedField { ty, field, pattern }, Ja) => {
                if *pattern {
                    format!(
                        "`{ty}` のパターンが `owns` フィールド `{field}` を省いています。`@{field}` か `@..`（権限を捨てる）を書きます"
                    )
                } else {
                    format!(
                        "`{ty}` のリテラルに消去フィールド `{field}` がありません。`@{field}: ..` か `..base` を書きます"
                    )
                }
            }
            (NotErasedField { ty, field }, En) => {
                format!("`{field}` is not an `owns` or `spec` field of `{ty}`")
            }
            (NotErasedField { ty, field }, Ja) => {
                format!("`{field}` は `{ty}` の `owns` / `spec` フィールドではありません")
            }

            (UsesNotParam { name }, En) => {
                format!("`uses {name}@` must name a parameter, but `{name}` is not a parameter")
            }
            (UsesNotParam { name }, Ja) => {
                format!("`uses {name}@` には引数の名前を書きますが、`{name}` は引数ではありません")
            }
            (UsesType { name }, En) => format!(
                "the type of `{name}` must be written directly as `Ptr<..>`, `Cell<..>`, `&Ptr<..>` or `&Cell<..>` to use `{name}@`"
            ),
            (UsesType { name }, Ja) => format!(
                "`{name}@` を使うには、`{name}` の型を直接 `Ptr<..>`、`Cell<..>`、`&Ptr<..>`、`&Cell<..>` と書きます"
            ),
            (UsesRegionType { name }, En) => format!(
                "the type of the erased parameter `{name}` must be `Region<..>`, `&Region<..>` or `&mut Region<..>`"
            ),
            (UsesRegionType { name }, Ja) => format!(
                "消去される引数 `{name}` の型は `Region<..>`、`&Region<..>`、`&mut Region<..>` にします"
            ),
            (GivesNotRet { name }, En) => format!(
                "`gives {name}@` must name the named return value, like `-> ({name}: Ptr<..>)`"
            ),
            (GivesNotRet { name }, Ja) => format!(
                "`gives {name}@` には名前付き戻り値の名前を書きます（`-> ({name}: Ptr<..>)`）"
            ),
            (GivesType { name }, En) => format!(
                "the type of the return value `{name}` must be written directly as `Ptr<..>` or `Cell<..>` to give `{name}@`"
            ),
            (GivesType { name }, Ja) => format!(
                "`{name}@` を返すには、戻り値 `{name}` の型を直接 `Ptr<..>` か `Cell<..>` と書きます"
            ),

            (RegionIndexNotIdent, En) => {
                "the index of a region must be a variable name, like `r[p]`".into()
            }
            (RegionIndexNotIdent, Ja) => {
                "領域の添字には変数の名前を書きます（`r[p]`）".into()
            }

            (PermAlreadyHeld { name }, En) => format!(
                "the permission `{name}@` is already held here; put it back before taking it again"
            ),
            (PermAlreadyHeld { name }, Ja) => format!(
                "権限 `{name}@` はすでにここで持っています。もう一度取り出す前に戻します"
            ),

            (PermNotOwned { name, op, state }, En) => format!(
                "cannot {} because the permission `{name}@` is {} here",
                op.text(En),
                state.text(En)
            ),
            (PermNotOwned { name, op, state }, Ja) => format!(
                "権限 `{name}@` がここでは{}なので、{}できません",
                state.text(Ja),
                op.text(Ja)
            ),
            (UsesArgNotVar { param }, En) => format!(
                "the argument for `{param}`, which `uses` a permission, must be a variable that holds the permission"
            ),
            (UsesArgNotVar { param }, Ja) => format!(
                "権限を `uses` する引数 `{param}` には、権限を持つ変数を渡します"
            ),

            (PermLeft { names }, En) => format!(
                "the permission(s) {} are still owned here; free, put back, move or `leak` them (only `gives` return values may remain)",
                names
                    .iter()
                    .map(|n| format!("`{n}@`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            (PermLeft { names }, Ja) => format!(
                "権限 {} をここでまだ所有しています。解放・`put`・移動・`leak` のどれかをします（残してよいのは `gives` の戻り値だけです）",
                names
                    .iter()
                    .map(|n| format!("`{n}@`"))
                    .collect::<Vec<_>>()
                    .join("、")
            ),

            (PermDiscarded, En) => {
                "the permission returned here is discarded; bind the result to a variable with `let x = ..;`".into()
            }
            (PermDiscarded, Ja) => {
                "ここで返る権限が捨てられます。結果を `let x = ..;` で変数に束縛します".into()
            }
            (JumpOutOfBorrow { name, is_continue }, En) => format!(
                "cannot `{}` out of the `borrow` block of `{name}`; end the block first",
                if *is_continue { "continue" } else { "break" }
            ),
            (JumpOutOfBorrow { name, is_continue }, Ja) => format!(
                "`{name}` の `borrow` ブロックから `{}` で抜けることはできません。先にブロックを終えます",
                if *is_continue { "continue" } else { "break" }
            ),
            (AssignHeld { name }, En) => format!(
                "cannot assign to `{name}` while its permission `{name}@` is held"
            ),
            (AssignHeld { name }, Ja) => format!(
                "権限 `{name}@` を持っている間は、`{name}` に代入できません"
            ),

            (ShadowHeld { name }, En) => format!(
                "cannot bind `{name}` again while its permission `{name}@` is held"
            ),
            (ShadowHeld { name }, Ja) => format!(
                "権限 `{name}@` を持っている間は、`{name}` を束縛し直せません"
            ),

            (WriteShared { name }, En) => format!(
                "cannot write through `{name}` because the permission `{name}@` is only borrowed (shared)"
            ),
            (WriteShared { name }, Ja) => format!(
                "権限 `{name}@` は共有の借用なので、`{name}` を通して書き込めません"
            ),

            (ConsumeBorrowed { name, op }, En) => format!(
                "cannot {} inside `borrow` of `{name}`; the permission `{name}@` is only borrowed in the block",
                op.text(En)
            ),
            (ConsumeBorrowed { name, op }, Ja) => format!(
                "`{name}` の `borrow` の中では{}できません。権限 `{name}@` はブロックの中で借りているだけです",
                op.text(Ja)
            ),

            (BranchMismatch { diffs }, En) => format!(
                "the branches end holding different permissions: {}",
                mismatches(diffs, En)
            ),
            (BranchMismatch { diffs }, Ja) => format!(
                "分岐の終わりで持っている権限が違います: {}",
                mismatches(diffs, Ja)
            ),

            (LoopMismatch { diffs, at }, En) => {
                let at = match at {
                    LoopPoint::End => "at the end of the loop body than at its start",
                    LoopPoint::Continue => "at this `continue` than at the start of the loop body",
                    LoopPoint::Break => "at this `break` than after the loop",
                };
                format!(
                    "the loop holds different permissions {at}: {}",
                    mismatches(diffs, En)
                )
            }
            (LoopMismatch { diffs, at }, Ja) => {
                let at = match at {
                    LoopPoint::End => "本体の終わりと始め",
                    LoopPoint::Continue => "この `continue` と本体の始め",
                    LoopPoint::Break => "この `break` とループの後",
                };
                format!(
                    "ループの{at}で持っている権限が違います: {}",
                    mismatches(diffs, Ja)
                )
            }

            (PermInClosure { name }, En) => format!(
                "permissions cannot be used inside a closure, but this uses `{name}`"
            ),
            (PermInClosure { name }, Ja) => format!(
                "クロージャの中では権限を使えませんが、ここで `{name}` を使っています"
            ),

            (SpecFieldAsRegion { field }, En) => format!(
                "`self@{field}` is a `spec` field, not a region; only `owns` fields can be indexed here"
            ),
            (SpecFieldAsRegion { field }, Ja) => format!(
                "`self@{field}` は `spec` フィールドで、領域ではありません。ここで添字を付けられるのは `owns` フィールドだけです"
            ),

            (MethodCallUses { method }, En) => format!(
                "`{method}` uses or gives permissions, so it cannot be called as a method `x.{method}(..)`; call it by path (`Type::{method}(x, ..)`) or as `self.{method}(..)`"
            ),
            (MethodCallUses { method }, Ja) => format!(
                "`{method}` は権限を `uses` / `gives` するので、メソッド `x.{method}(..)` の形では呼べません。パス（`Type::{method}(x, ..)`）か `self.{method}(..)` で呼びます"
            ),

            (SelfErasedWithoutSelf, En) => {
                "`self@..` can only be used in a function that takes `self`".into()
            }
            (SelfErasedWithoutSelf, Ja) => {
                "`self@..` は `self` を取る関数の中でだけ使えます".into()
            }

            (AssignOwnsField { field }, En) => format!(
                "cannot assign to the `owns` field `{field}`; change a region with `take`, `put` and `borrow`"
            ),
            (AssignOwnsField { field }, Ja) => format!(
                "`owns` フィールド `{field}` には代入できません。領域は `take`・`put`・`borrow` で変えます"
            ),

            (RefinementNested { ty }, En) => format!(
                "the invariant of `{ty}` is not added to contracts at this position; only `T`, `&T`, `&mut T`, `Option<T>` and `Result<T, E>` are expanded"
            ),
            (RefinementNested { ty }, Ja) => format!(
                "この位置では `{ty}` の不変条件を契約に足しません。展開するのは `T`、`&T`、`&mut T`、`Option<T>`、`Result<T, E>` だけです"
            ),
        }
    }
}
