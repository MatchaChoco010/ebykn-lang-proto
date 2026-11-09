# Ebykn 実装仕様（段階 1 プロトタイプ）

版：0.7（2026-11-09）
対象読者：プロトタイプの実装者（人間と AI）。思想は `ebykn-thesis.md`、設計の経緯・却下した案・判断基準は `ebykn-design-notes.md`。本書は**実装のための規範**で、代案の比較は含まない。

表記：**MUST** は守らなければならない規則、**SHOULD** は正当な理由がなければ守る規則、**MAY** は任意。`OPEN-n` は未決事項で、各項に実装者が従うべき既定の挙動を書いてある。実装者は OPEN を自分で決めず、既定に従い、気づいた点を `OPEN.md` に記録する。本書と `ebykn-design-notes.md` が矛盾したら本書を優先し、矛盾を `OPEN.md` に記録する。

---

## 1. 段階 1 の範囲

### 1.1 作るもの
`ebykn` コマンド（Rust 製）。入力は `.eyk` / `.eykp` / 性質ファイル、出力は Verus のソース。検証とビルドは Verus（`verus` / `cargo verus`）に任せる。

| 機能 | 段階 1 |
| --- | --- |
| `.eyk` / `.eykp` の構文解析（§4、§5） | ○ |
| 証明の剥がし一致検査（§6.1） | ○ |
| 権限スコープと線形性の検査（§6.2） | ○ |
| 絞り込み（型不変条件・部分集合型）の署名展開（§6.4） | ○ |
| `.eyk` の自己完結検査（§6.6） | ○ |
| Verus ソースへの書き下し（§7） | ○ |
| 性質ファイルの合成（§8） | ○ |
| `check` / `verify` / `build` / `sync` / `status` / `init`（§9） | ○ |
| Verus の診断の写し戻し（§10） | ○ |
| `fmt`、LSP、`prove`（AI 連携）、索引サービス、性質ファイルの自動生成、`Vec<絞り込み型>` 等の入れ子展開 | ×（段階 2 以降） |

### 1.2 作らないもの（段階 1）
型推論、借用検査、§6.5 を超える名前解決、独自の検証、実行時検査の挿入、Rust から呼ぶための API 整形、`as` の置き換え、線形型の一般化。

### 1.3 固定するもの
- Verus：リリースを一つ選び、コミットハッシュを `Ebykn.toml` に記録する（§3.2）。vstd の API 名は prelude（§7.1）で吸収し、書き下しは prelude だけを参照する。
- Rust toolchain：Verus が要求する nightly。
- `panic = abort`：生成する `Cargo.toml` に `[profile.dev] panic = "abort"`、`[profile.release] panic = "abort"` を MUST で書く。
- 出力クレート `build/verus/` は、Verus で検証でき、かつ Verus なしの rustc でもビルドできる（`verus!` マクロが ghost を消去する）。これが「消去済みの Rust クレート」であり、別の emitter は作らない（OPEN-11）。

---

## 2. 用語

| 語 | 意味 |
| --- | --- |
| source / `.eyk` | 実装・契約・資源の操作を書くファイル。証明構文は文法上書けない |
| proof / `.eykp` | 対応する `.eyk` の写しに証明構文を足したファイル |
| 性質ファイル / props | 外部 Rust クレートへの `unchecked` 宣言の集合（§8） |
| 証明構文 | `.eykp` にだけ書ける構文（§5.7）。剥がすと `.eyk` に一致する |
| 権限 | `PointsTo<T>` の値。表層では `p@` |
| 領域 | `Region<L>`。権限の集合。`L` は `Ptr<T>` か `Cell<T>` |
| 仕様の世界 | 実行時に存在しないもの全て（権限、領域、`spec` フィールド、論理式）。印は `@` |
| 絞り込み | 型に付く述語。部分集合型と型不変条件の総称（§6.4） |
| 書き下し | Ebykn → Verus の変換 |
| prelude | Ebykn が提供する Verus クレート `ebykn_prelude`（§7.1）。全 `.eyk` に暗黙に `use` される |
| 索引 | 字句的に集めた署名の表（§6.5） |

---

## 3. プロジェクト構成

### 3.1 ディレクトリとモジュール
```
Ebykn.toml
src/lib.eyk | src/main.eyk        # クレートルート
src/**/*.eyk
proof/**/*.eykp                   # src/<p>.eyk に対応するのは proof/<p>.eykp。無くてよい
props/**/*.eyk | *.eykp           # 自作の性質ファイル（§8）
build/verus/                      # 生成（Cargo プロジェクト）
build/map/                        # 生成（source map、§10.2）
build/interface.json              # 生成（他の Ebykn クレートが読む索引、§3.3）
.ebykn/sync/                      # sync の基準（前回の src の写し）
```
- モジュールは Rust と同じ：`mod foo;` は `src/foo.eyk` か `src/foo/mod.eyk`。インライン `mod foo { }` も可。`.eykp` のモジュール構造は `.eyk` と一致していなければならない（剥がし一致で検査）。
- 全 `.eyk` / `.eykp` に `use ebykn_prelude::*;` が暗黙に付く。
- `proof/` に対応する `src/` が無い `.eykp` は **E0301**。`proof/` 以外の `.eykp`、`src/` 内の `.eykp` は **E0302**。

### 3.2 `Ebykn.toml`
```toml
[package]
name = "dlist"
version = "0.1.0"
edition = "2024"

[verus]
release = "release/0.2026.xx"       # 固定
commit  = "<sha>"

[dependencies]                       # Rust クレート（cargo と同じ記法。そのまま Cargo.toml に写す）
rayon = "1.10"

[props]                              # 性質パッケージ
rayon = { path = "../rayon-props" }  # 段階 1 は path と git のみ（OPEN-9）
std   = "builtin"                    # vstd の std_specs

[ebykn-dependencies]                 # 他の Ebykn クレート（build/interface.json を読む）
mylib = { path = "../mylib" }
```
`ebykn` は `build/verus/Cargo.toml` を生成する。`[dependencies]` を写し、`vstd` と `ebykn_prelude` と `[ebykn-dependencies]` の `build/verus` を足す。

### 3.3 `build/interface.json`
他の Ebykn クレートが §6.5 の索引として読む。
```json
{ "crate": "mylib", "version": "0.1.0",
  "fns":      [ { "path": "mylib::swap", "uses": [{"param":"p","mode":"&mut"},{"param":"q","mode":"&mut"}], "gives": [] } ],
  "structs":  [ { "path": "mylib::DList", "owns": [{"name":"nodes","ty":"Region<Ptr<Node<T>>>"}], "spec": [{"name":"order","ty":"Seq<Ptr<Node<T>>>"}] } ],
  "invariants": [ "mylib::DList" ],
  "subsets":  [ { "path": "mylib::FiniteMesh", "base": "mylib::Mesh", "pred": "finite(self)" } ] }
```

---

## 4. 字句

Rust の字句に次を足す。全て**文脈キーワード**（識別子としても使えるが、下記の位置ではキーワード）。

| 語 | キーワードになる位置 |
| --- | --- |
| `spec`、`lemma`、`unchecked`、`closed` | 項目の先頭（`fn` / `let` / `{` / `group` の直前） |
| `requires`、`ensures`、`returns`、`decreases`、`no_panic`、`uses`、`gives`、`by` | 関数シグネチャ（`where` 句を含む）の後、本体の前 |
| `invariant`、`decreases` | ループヘッダの後、`{` の前 |
| `owns`、`spec` | `struct` 本体の `}` の直後 |
| `take`、`put` | 文の先頭で、直後が領域添字（§5.4）のとき |
| `borrow` | 文の先頭で、直後が `mut`? 領域添字 `{` のとき |
| `leak` | 文の先頭で、直後が `Ident @ ;` のとき |
| `forall`、`exists` | 式の先頭 |
| `and`、`or` | 式の先頭で、直後が `{` のとき |
| `where` | `type Ident Generics? = Type` の直後で、続くトークン列が Rust の where 述語（`Type :`）として読めないとき |
| `props` | 性質ファイルの先頭 |
| `proof`、`assert`、`reveal`、`hide`、`lemmas`（`use` の直後）、`group`（`lemma` の直後） | `.eykp` 内 |

- トークン `@`：Rust の `@`（パターン束縛）に加え、`Ident @`（後置：権限）、`self @ Ident`（消去フィールド）、`@ Ident`（構造体リテラル・パターンの前置）。位置で解消する。
- 演算子：`==>`（右結合、`||` より低い）、`<==>`（`==>` より低い、非結合）、`=~=`（`==` と同じ優先順位、`.eykp` のみ）。
- 識別子の接頭辞 `__eyk_` は生成名のために予約（使えば **E0101**）。
- `.eyk` / `.eykp` に `#[verifier::..]` 属性を書くことはできない（**E0102**。Ebykn が認識する属性は §5.7 と `#[override]`、`#[test]`、Rust の属性）。

---

## 5. 文法

Rust の文法（`syn` が受理する範囲）を土台とし、以下を足す。記法は EBNF 風。`Expr`、`Type`、`Block`、`Pat`、`Path`、`Generics`、`WhereClause`、`Vis`、`Attrs` は Rust のもの。

### 5.1 項目
```
Item          ::= RustItem | Fn | SpecFn | LemmaFn | Unchecked | Struct | Trait | SubsetType | ImplSpec
               |  LemmaGroup | UseLemmas                      (* 後 2 つは .eykp のみ *)

Fn            ::= Attrs Vis "fn" Ident Generics "(" Params ")" Ret? WhereClause? Contract FnBody
FnBody        ::= Block | "=" Expr ";"                          (* "=" 形は純粋関数 *)
Ret           ::= "->" Type | "->" "(" Ident ":" Type ")"       (* 名前付き戻り値 *)

SpecFn        ::= Attrs Vis "closed"? "spec" "fn" Ident Generics "(" Params ")" "->" Type
                  WhereClause? Contract ( "=" Expr ";" | ";" )  (* ";" だけは trait / props の宣言 *)
LemmaFn       ::= Attrs Vis "lemma" "fn" Ident Generics "(" Params ")" WhereClause? Contract
                  ( ";" | Block )                               (* Block は .eykp のみ *)
Unchecked     ::= Attrs Vis "unchecked" ( Fn | LemmaFn | ExternFn )
ExternFn      ::= "fn" Path Generics "(" Params ")" Ret? WhereClause? Contract ";"  (* props のみ *)

Struct        ::= RustStruct Owns? Spec?                         (* 名前付きフィールドの struct のみ *)
Owns          ::= "owns" "{" ErasedField* "}"
Spec          ::= "spec" "{" ErasedField* "}"
ErasedField   ::= Attrs Vis? Ident ":" Type ","?

Trait         ::= Attrs Vis "trait" Ident Generics (":" Bounds)? WhereClause? "{" TraitItem* "}"
TraitItem     ::= RustTraitItem
               |  Attrs "fn" Ident Generics "(" Params ")" Ret? WhereClause? Contract (";" | Block)
               |  Attrs "spec" "fn" Ident Generics "(" Params ")" "->" Type WhereClause? (";" | "=" Expr ";")
               |  Attrs "lemma" "fn" Ident Generics "(" Params ")" WhereClause? Contract ";"

SubsetType    ::= Attrs Vis "type" Ident Generics "=" Type "where" Expr ";"
ImplSpec      ::= "impl" Generics "spec" Path ( "for" Type )? "{" ( SpecFn | LemmaFn )* "}"
LemmaGroup    ::= Vis "lemma" "group" Ident "{" Path ("," Path)* ","? "}"
UseLemmas     ::= "use" "lemmas" Path ";"
```
- `Struct`：tuple struct と enum に `owns` / `spec` は付けられない（**E0401**）。enum にも `impl Invariant` は付けられる。
- 純粋関数 `fn f(..) -> T = e;`：`e` は実行文脈でも論理文脈でも妥当でなければならない（同じ式を `fn` と `spec fn` の両方に書き下す。`int`・量化子・`spec fn` の呼び出しは使えず、`u64` の算術はオーバーフローが義務になる。違反は Verus が報告する）。
- `spec fn` の本体 `e` はブロック式でもよい（`= { let x = ..; .. };`）。
- `impl` の中で `invariant`、`view` という名前の項目を利用者が直接定義することはできない（**E0403**。`impl Invariant` / `impl View` を使う）。
- trait のメソッドの契約は Verus の規則に従う（impl 側は `requires` を弱め `ensures` を強めてよい）。trait の `spec fn` / `lemma fn` は impl が本体を与える（`lemma fn` の本体は `.eykp`）。
- `where` の曖昧さ：`type T<X> = U where X: Bound;`（Rust）と `type T = U where P(self);`（部分集合型）は、`where` の後が `Type :` の形なら Rust、そうでなければ Ebykn と読む。部分集合型の述語は `self` を自由変数として持つ `bool` の式。

### 5.2 契約
```
Contract   ::= Clause*
Clause     ::= "uses" UsesItem ("," UsesItem)*
            |  "gives" Ident "@" ("," Ident "@")*
            |  "requires" ExprList | "ensures" ExprList | "returns" Expr
            |  "decreases" ExprList | "no_panic"
            |  "by" "(" ProverMode ")"                          (* LemmaFn、.eykp のみ *)
UsesItem   ::= ("&" "mut"?)? Ident "@"                          (* 引数 Ident の権限 *)
            |  Ident ":" Type                                   (* 名前付きの消去される引数。Type は Region<..> か &/&mut Region<..> *)
ExprList   ::= Expr ("," Expr)* ","?
ProverMode ::= "bit_vector" | "nonlinear_arith" | "integer_ring" | "compute" | "compute_only"
```
- 同じ種類の句が複数回現れてもよい（連結する）。構文上の順序は任意。正規順は `uses`、`gives`、`requires`、`ensures`、`returns`、`decreases`、`no_panic`、`by`。
- `uses Ident @` の `Ident` は引数名で、その引数の型は `Ptr<..>` / `Cell<..>` / `&Ptr<..>` / `&Cell<..>` と**直接**書かれていること（**E0501**）。`gives Ident @` の `Ident` は名前付き戻り値で、型は `Ptr<..>` / `Cell<..>`（**E0501**）。
- `unchecked fn` にも `uses` / `gives` は書ける（FFI の包みなど）。

### 5.3 式（論理）
```
Expr       ::= RustExpr | Quant | AndOr | At | Implies
Quant      ::= ("forall" | "exists") Binder ("," Binder)* "," Expr
Binder     ::= Ident ":" Type | Ident "in" Expr | Ident
AndOr      ::= ("and" | "or") "{" ExprList "}"
At         ::= Ident "@" | "self" "@" Ident | At "[" Expr "]"   (* 後置 *)
Implies    ::= Expr "==>" Expr | Expr "<==>" Expr
```
- `Quant` の束縛子の読み方：`,` の直後が `Ident ":"`、`Ident "in"`、`Ident ","` なら束縛子を続け、そうでなければ本体。
- `in` の右辺は範囲（`a..b`、`a..=b`）、`Seq`、`Set`、`Region`、`Map`（鍵）。
- 論理文脈での `*p`（`p` は権限付き変数）は「`p@` の指す値」（§7.3）。`old(*p)` も可。
- `p@` の論理 API：`p@.value()`、`p@.is_init()`、`p@.is_uninit()`。
- `old(e)`、`final(e)`、`e.view()` は通常の呼び出し構文。`e.view()` には `impl View` が必要（無ければ Verus のエラー）。

### 5.4 文（source にも書けるもの）
```
Stmt        ::= RustStmt | Take | Put | Borrow | Leak | SpecAssign | Loop
Take        ::= "take" RegionIndex ";"
Put         ::= "put"  RegionIndex ";"
Borrow      ::= "borrow" "mut"? RegionIndex Block
Leak        ::= "leak" Ident "@" ";"
SpecAssign  ::= "self" "@" Ident "=" Expr ";"                   (* spec フィールドの代入 *)
RegionIndex ::= RegionExpr "[" Ident "]"
RegionExpr  ::= "self" "@" Ident | Ident
Loop        ::= ("while" Expr | "for" Pat "in" Expr | "loop") LoopClause* Block
LoopClause  ::= "invariant" ExprList | "decreases" ExprList
```
- `RegionIndex` の添字は**識別子**（**E0502**）。
- `RegionExpr` は、`self@Ident` が Self 構造体の `owns` フィールドであるか、`Ident` が領域変数（`uses Ident: Region<..>`、`let Ident: Region<..> = ..`、`@` パターンで束縛した `owns` フィールド）であるときだけ領域。それ以外の `x[i]` は Rust の添字。
- `SpecAssign` の左辺は `spec` フィールドに限る。`owns` フィールドへの代入は **E0516**。

構造体リテラルとパターン：
```
StructLit   ::= Path "{" ( Field | "@" Ident ":" Expr )* ( ".." Expr )? "}"
StructPat   ::= Path "{" ( FieldPat | "@" Ident | "@" ".." )* ".."? "}"
```
- `owns` / `spec` を持つ構造体のリテラルは、全ての消去フィールドを `@name: expr` で与えるか、`..base` で引き継ぐ（**E0402**）。
- パターンで `owns` フィールドを無言で省くことはできない。`@..` は残りの消去フィールドを捨てる（権限が含まれていれば `leak` と同じ）。`spec` フィールドは `..` で省いてよい。

### 5.5 絞り込みの位置
部分集合型 `T` と、`impl Invariant for T` を持つ型 `T` は、次の**署名位置**で展開される（§6.4）：`x: T`、`x: &T`、`x: &mut T`、`-> T`、`-> (r: T)`、`Option<T>`、`Result<T, E>`、およびこれらの `&` / `&mut` 付き。他の入れ子（`Vec<T>` 等）は展開しない（**W0601**）。

### 5.6 クロージャ
Rust のクロージャはそのまま通す。契約付きクロージャ（Verus の `|x| requires .. ensures ..`）は段階 1 では Verus の構文をそのまま通す（OPEN-12）。クロージャ本体で権限は使えない（§6.2）。

### 5.7 証明構文（`.eykp` にだけ書ける。`.eyk` にあれば **E0303**）
```
ProofStmt   ::= "proof" Block
             |  "assert" Expr ( "by" Block | "by" "(" ProverMode ")" ("requires" ExprList)? )? ";"
             |  "assert" "forall" Binder ("," Binder)* "," Expr "by" Block ";"
             |  "spec" "let" Pat "=" Expr ";"
             |  "reveal" "(" Path ("," Expr)? ")" ";" | "hide" "(" Path ")" ";"
             |  "use" "lemmas" Path ";"
             |  Path "(" Args ")" ";"                           (* lemma fn の呼び出し。proof Block の中でのみ *)
ProofAttr   ::= "#[trigger]" | "#![trigger" ExprList "]" | "#![auto]"
             |  "#[opaque]" | "#[rlimit(" Int ")]" | "#[spinoff_prover]" | "#[memoize]"
ProofItem   ::= LemmaFn(本体付き) | SpecFn | LemmaGroup | UseLemmas
```
- 証明構文：上記、`LemmaFn` の本体、`by(..)` 句、`ProofAttr`、`ProofItem`、ループの**追加**の `invariant` / `decreases` 句、`=~=`。
- `.eykp` だけの項目は他の `.eykp` から通常のパスで参照できる。`.eyk` からは参照できない（§6.6 で検査）。

---

## 6. 静的検査（Ebykn 自身が行う）

### 6.1 剥がし一致検査
入力：`src/p.eyk` と `proof/p.eykp`。
1. 両方を構文解析する。
2. `.eykp` の AST から §5.7 の証明構文を取り除く。ループの `invariant` / `decreases` は、`.eyk` 側の句の列が `.eykp` 側の句の列の**部分列**（順序保存、トークン列として等しい）であることを確かめ（違えば **E0305**）、`.eykp` 側だけの句を取り除く。
3. 両 AST を正規化して（空白・コメント・属性の順序・末尾カンマを無視）トークン列として比較する。
4. 不一致なら、最初に異なる項目を特定し、その中の最初に異なる文の位置を **E0304** で報告する。両ファイルの位置を出す。
5. `--base <rev>` が指定されたら、`git show <rev>:src/p.eyk` を `.eyk` の代わりに使う。

### 6.2 権限スコープと線形性
関数本体ごとに、環境 Γ：`Ident → State`（`State ∈ { Owned, BorrowedShared, BorrowedMut }`）と領域変数集合 R を持って文を順に走査する。

**導入**
| 構文 | 効果 |
| --- | --- |
| `uses p@` | `p ↦ Owned` |
| `uses &p@` / `uses &mut p@` | `p ↦ BorrowedShared` / `BorrowedMut` |
| `uses r: Region<..>`、`let r: Region<..> = e;`、パターンの `@r` | `r ∈ R` |
| `let p = Ptr::new(e);`、`let p = Ptr::empty();`、`let c = Cell::new(e);` | `p ↦ Owned` |
| `let q = f(..);` で `f` が `gives q'@` を持つ（§6.5） | `q ↦ Owned` |
| `take r[p];` | `p ↦ Owned`（既に `p ∈ Γ` なら **E0503**） |
| `borrow r[p] { }` / `borrow mut r[p] { }` | ブロック内で `p ↦ BorrowedShared` / `BorrowedMut`。終了で削除 |

**消費**
| 構文 | 条件と効果 |
| --- | --- |
| `p.free()` | `Owned` 必須（**E0504**）。削除 |
| `put r[p];` | `Owned` 必須。削除 |
| `f(.., p, ..)` で対応引数が `uses p'@`（移動） | `Owned` 必須。削除 |
| `f(.., p, ..)` で対応引数が `uses &mut p'@` / `uses &p'@` | `Owned` か同種以上の借用。変化なし |
| `leak p@;` | `Owned` 必須。削除 |
| `return e` / 本体末尾 | `Owned` は `gives` の戻り値名に一致するものだけ残ってよい。他が残れば **E0505** |

**規則**
- `*X`（`X` は識別子）は `X ∈ Γ` のときだけ権限付き読み書きに書き下す。`(*X).f = e` も同じ。`X ∉ Γ` なら Rust のまま。
- `X ∈ Γ` のとき `X = e;` は **E0506**、`let X = ..;` は **E0507**。
- `BorrowedShared` の `X` への書き込みは **E0508**。
- `borrow` ブロック内での `p` の `free` / `put` / 移動は **E0509**。
- `if` / `match` / `if let` / `let-else`：各分岐の終端で Γ が等しいこと（**E0510**）。`break` / `continue` / `return` / 発散で終わる分岐は比較から除く。
- ループ：本体入口の Γ と、本体終端および各 `continue` 点の Γ が等しいこと。各 `break` 点の Γ はループ直後の Γ と等しいこと（**E0511**）。
- クロージャ本体では Γ と R を空にする（**E0512**）。
- `?` 演算子による早期 return は `return` と同じ扱い。
- `unchecked fn` の本体は検査しない。

### 6.3 領域添字の認識
§5.4 の判定に、索引（Self 構造体の `owns` フィールド名）と R を使う。`self@x[i]` で `x` が `spec` フィールドなら論理式の添字であり、文の位置にあれば **E0513**。

### 6.4 絞り込みの署名展開
索引から集合 F：`type T = B where P(self)` の `T ↦ (B, P)`、`impl Invariant for T` の `T ↦ (T, self.invariant())`。
各 `fn` と `unchecked fn`（`spec fn` / `lemma fn` は除く）の署名について、§5.5 の位置にある型 `T ∈ F` を見つけ、`T` を `B` に置き換えて契約を足す：

| 位置 | 足す契約（`P[x]` は `P` の `self` を `x` に置換したもの） |
| --- | --- |
| `x: T`、`x: &T`、`x: T`（受け手 `self` / `&self` を含む） | `requires P[x]` |
| `x: &mut T`（`&mut self` を含む） | `requires P[old(x)]`、`ensures P[x]` |
| `-> T` / `-> (r: T)`（`Self` を含む） | `ensures P[r]`（無名なら名前 `__eyk_r` を導入） |
| `Option<T>` / `Result<T, E>`（引数） | `requires x is Some ==> P[x.unwrap()]` 相当 |
| 同（戻り値） | `ensures r is Some ==> P[r.unwrap()]` 相当 |

例外：`impl Invariant for T` 由来の `P` は、`impl T { }`（`T` 自身の impl）の中の **非 `pub` の関数には足さない**。`let x: T = e;` で `T` が部分集合型なら、書き下しで `assert(P[e])` を挿入する（証明義務）。

**入れ子**：`T` のフィールド型 `U` が F にあれば、`T.invariant()` の書き下しに `P_U[self.field]` を連言で足す（`Option<U>` フィールドは `is Some ==>`）。

### 6.5 索引と名前解決（限定）
**索引**：クレート内の全項目（パス → `fn` の `uses` / `gives`、`struct` の `owns` / `spec` フィールド、`impl Invariant for T`、部分集合型、`impl T` の Self 型、`impl View for T`）、`[ebykn-dependencies]` の `interface.json`、prelude の署名。

| 呼び出しの形 | 解決 |
| --- | --- |
| `path::f(..)`、`f(..)` | 現在のモジュール → `use` 宣言 → prelude の順 |
| `Self::f(..)`、`self.f(..)`（`impl T` の中） | `T` の impl の索引 |
| `x.f(..)` で `x ∈ Γ` | prelude の `Ptr` / `Cell` メソッド |
| `x.f(..)` その他 | **解決しない**。`uses` / `gives` を持つ関数をこの形で呼ぶのは **E0514** |

解決できない呼び出しはそのまま Verus に渡す。

### 6.6 `.eyk` の自己完結
`.eykp` だけにある項目を `.eyk` が参照していないことを保証するため、`check` は全モジュールの `.eyk` だけを書き下したプロジェクトに対して `verus --no-verify` を一度走らせる（型・名前解決のみ）。失敗は **E0306** として報告する。その後 `.eykp` を合成したプロジェクトを作る。

---

## 7. 書き下し（Ebykn → Verus）

### 7.1 prelude（Verus クレート `ebykn_prelude`）
書き下しの出力は vstd を直接参照せず、prelude の次の API だけを参照する。契約は M0 で vstd に合わせて確定し、Verus の版に結び付ける。

```rust
pub struct Ptr<T>;                          // Copy。vstd::simple_pptr::PPtr の包み
pub struct PointsTo<T>;                     // tracked。spec: pptr(), value(), is_init(), is_uninit()
impl<T> Ptr<T> {
    fn new(v: T) -> (p: Ptr<T>, Tracked<PointsTo<T>>)   ensures perm.pptr() == p, perm.is_init(), perm.value() == v
    fn empty()  -> (p: Ptr<T>, Tracked<PointsTo<T>>)    ensures perm.pptr() == p, perm.is_uninit()
    fn read(&self, Tracked(perm: &PointsTo<T>)) -> &T   requires perm.pptr() == *self, perm.is_init()  ensures *r == perm.value()
    fn write(&self, Tracked(perm: &mut PointsTo<T>), v: T)  requires old(perm).pptr() == *self  ensures perm.is_init(), perm.value() == v
    fn borrow_mut(&self, Tracked(perm: &mut PointsTo<T>)) -> &mut T   requires .. ensures ..(final)
    fn free(self, Tracked(perm: PointsTo<T>)) -> T      requires perm.pptr() == self, perm.is_init()  ensures r == perm.value()
    fn addr(&self) -> usize
}
pub struct Cell<T>;                         // vstd::cell::PCell の包み。new / read / write / borrow_mut / id() は Ptr と同形
pub struct Region<L>;                       // tracked。Map<usize, PointsTo<T>> の包み（L = Ptr<T> | Cell<T>、鍵は addr / id）
impl<L> Region<L> {
    proof fn empty() -> (tracked r: Region<L>)
    proof fn insert(tracked &mut self, tracked perm: PointsTo<T>)   requires !old(self).contains(perm.loc())  ensures self == old(self).spec_insert(..)
    proof fn remove(tracked &mut self, loc: L) -> (tracked perm: PointsTo<T>)  requires old(self).contains(loc)  ensures perm.loc() == loc, ..
    proof fn borrow(tracked &self, loc: L) -> (tracked perm: &PointsTo<T>)  requires self.contains(loc)
    spec fn contains(&self, loc: L) -> bool;  spec fn index(&self, loc: L) -> T;   // r[p]
    spec fn dom(&self) -> Set<L>;  spec fn len(&self) -> nat;
    spec fn spec_insert(&self, loc: L, v: T) -> Region<L>;  spec fn spec_remove(&self, loc: L) -> Region<L>;
}
// Seq の拡張（spec）：get(i: int) -> Option<T>、first() -> Option<T>、last() -> Option<T>、index_of(x) -> int、no_duplicates()、remove(i)
// View：vstd::view::View を再公開。old / final は Verus の組み込み
```
- `borrow mut r[p] { }` は `remove` → 本体 → `insert` で書き下す（OPEN-4）。
- `Seq` 拡張は vstd の `Seq` に対する拡張トレイトとして実装する。

### 7.2 項目
| Ebykn | Verus |
| --- | --- |
| `fn f(..) -> T { .. }` | `fn f(..) -> T { .. }` |
| `fn f(..) -> T = e;` | `#[verifier::when_used_as_spec(__eyk_spec_f)] pub fn f(..) -> (r: T) ensures r == __eyk_spec_f(..) { e }` ＋ `pub open spec fn __eyk_spec_f(..) -> T { e }` |
| `spec fn f(..) -> T = e;`（既定 open） | `open spec fn f(..) -> T { e }`（`pub` はそのまま） |
| `closed spec fn` | `closed spec fn` |
| `lemma fn f(..) ..;`（本体なし） | `.eykp` に本体があればそれ。無ければ `proof fn f(..) .. { admit() }` を出し、`status` で unverified と表示 |
| `lemma fn f(..) by(m) { .. }` | `proof fn f(..) by(m) { .. }` |
| `unchecked fn f(..) { body }` | `#[verifier::external_body] fn f(..) { body }`（`uses` があれば Tracked 引数も付く） |
| `unchecked fn path::f(..) ..;`（props） | `pub assume_specification [path::f](..) ..;`（§8 で合成後） |
| `unchecked lemma fn f(..) ..;` | `#[verifier::external_body] proof fn f(..) { }` |
| `trait` のメソッド・`spec fn`・`lemma fn` | Verus の trait（`spec fn` / `proof fn` を trait item に） |
| `impl spec path::Trait { .. }` | `#[verifier::external_trait_specification]` ＋ `obeys_*`（OPEN-10） |
| `lemma group g { a, b }` / `use lemmas g;` | `broadcast group g { a, b }` / `broadcast use g;` |
| `type T = B where P;` | 項目としては消える。`T` の出現は `B` に |
| `impl Invariant for T { closed spec fn invariant(&self) -> bool = e; }` | `impl T { pub closed spec fn invariant(&self) -> bool { e && <入れ子> } }`。`closed` が無ければ `open` |
| `impl View for T { type V = ..; spec fn view(&self) -> V = e; }` | `impl View for T { type V = ..; open spec fn view(&self) -> V { e } }`（`closed` 指定可） |
| `#[test] fn` | `#[verifier::external] #[test] fn` |

### 7.3 構造体と資源
```
struct S { a: A } owns { r: Region<Ptr<N>> } spec { q: Seq<Ptr<N>> }
  ⇒ struct S { a: A, r: Tracked<Region<Ptr<N>>>, q: Ghost<Seq<Ptr<N>>> }
S { a: x, @r: Region::empty(), @q: Seq::empty() }   ⇒ S { a: x, r: Tracked(Region::empty()), q: Ghost(Seq::empty()) }
S { a: x, ..base }                                   ⇒ そのまま（消去フィールドも base から）
let S { a, @r, @q } = s;                             ⇒ let S { a, r: Tracked(r), q: Ghost(q) } = s;
self@r（変更）/（読み）                              ⇒ self.r.borrow_mut() / self.r.borrow()
self@q（論理）                                        ⇒ self.q@
self@q = e;                                          ⇒ proof { self.q = Ghost(e); }
```
| Ebykn | Verus（`__eyk_` は生成名） |
| --- | --- |
| `let p = Ptr::new(e);` | `let (p, Tracked(__eyk_perm_p)) = Ptr::new(e);` |
| `let v = p.free();` | `let v = p.free(Tracked(__eyk_perm_p));` |
| `*p`（実行、値が要る位置） | `*p.read(Tracked(&__eyk_perm_p))`（OPEN-5） |
| `*p`（実行、参照が要る位置：`&(*p).f` 等） | `p.read(Tracked(&__eyk_perm_p))` |
| `*p = e;` | `p.write(Tracked(&mut __eyk_perm_p), e);` |
| `(*p).f = e;`、`(*p).g.h = e;` | `p.borrow_mut(Tracked(&mut __eyk_perm_p)).f = e;` |
| `*p`（論理）/ `old(*p)` | `__eyk_perm_p.value()` / `old(__eyk_perm_p).value()` |
| `p@`（論理） | `__eyk_perm_p` |
| `take r[p];` | `let tracked __eyk_perm_p = <r>.remove(p);` |
| `put r[p];` | `<r>.insert(__eyk_perm_p);` |
| `borrow r[p] { body }` | `{ let tracked __eyk_perm_p = <r>.borrow(p); body }` |
| `borrow mut r[p] { body }` | `{ let tracked __eyk_perm_p = <r>.remove(p); let __eyk_res = { body }; <r>.insert(__eyk_perm_p); __eyk_res }` |
| `leak p@;` | （何も出さない） |
| `r[p]`（論理） | `<r>.index(p)` |
| `fn f(p: Ptr<T>) uses &mut p@` | `fn f(p: Ptr<T>, Tracked(__eyk_perm_p): Tracked<&mut PointsTo<T>>) requires __eyk_perm_p.pptr() == p` |
| `uses p@`（移動） | `Tracked(__eyk_perm_p): Tracked<PointsTo<T>>` ＋ 同じ `requires` |
| `uses r: Region<L>` / `uses r: &mut Region<L>` | `Tracked(r): Tracked<Region<L>>` / `Tracked<&mut Region<L>>` |
| `-> (q: Ptr<T>) gives q@` | `-> (q: Ptr<T>, Tracked<PointsTo<T>>)` ＋ `ensures __eyk_perm_q.pptr() == q`。呼び出し側 `let q = f();` ⇒ `let (q, Tracked(__eyk_perm_q)) = f();` |
| `f(p)`（callee が `uses &mut p'@`） | `f(p, Tracked(&mut __eyk_perm_p))`（追加引数は callee の `uses` 宣言順で末尾） |

`<r>` は `self.r.borrow_mut()`（変更）/ `self.r.borrow()`（読み）/ 領域変数名。`self` を取らない関数で `self@` を使えば **E0515**。

### 7.4 式と文
| Ebykn | Verus |
| --- | --- |
| `forall x: T, P` | `forall\|x: T\| P` |
| `forall x in r, P` | `forall\|x\| <展開> ==> P`。`a..b` は `a <= x < b`、`a..=b` は `a <= x <= b`、`Seq` / `Set` / `Region` は `r.contains(x)`、`Map` は `r.dom().contains(x)` |
| `exists ..` | `exists\|x\| <展開> && P` |
| `and { a, b, c }` / `or { .. }` | `(a && b && c)` / `(a \|\| b \|\| c)` |
| `P ==> Q`、`P <==> Q` | 同じ |
| `v.view()` | `v@` |
| `old(e)`、`final(e)` | 同じ |
| `assert P;`（`.eykp`） | `assert(P);` |
| `assert P by { .. };` / `by(mode) requires Q;` | 同じ形 |
| `assert forall x in r, P by { .. };` | `assert forall\|x\| <展開> ==> P by { .. }` |
| `spec let a = e;` | `let ghost a = e;` |
| `proof { .. }` | 同じ |
| `#[opaque]` 等 | `#[verifier::opaque]` 等。`#[trigger]` / `#![trigger ..]` / `#![auto]` は同じ |
| `=~=` | 同じ |
| ループ句 | 同じ位置 |
| `no_panic` | `no_unwind` |
| `returns e` | `returns e` |
| `let x: T = e;`（`T` が部分集合型） | `let x: B = e; assert(P[x]);` |

### 7.5 出力
- `build/verus/Cargo.toml`、`build/verus/src/**/*.rs`。各モジュールは `.eykp` があればそれを、無ければ `.eyk` を書き下す。
- 各 `.rs` の先頭は `use vstd::prelude::*; use ebykn_prelude::*; verus! {`、末尾は `}`。
- `verusfmt` が利用可能なら SHOULD 適用する。source map は整形後に作る。

---

## 8. 性質ファイル

### 8.1 形式
```
props rayon = "^1.10";                       // 先頭。対象クレートと版範囲

unchecked fn rayon::join<A, B, RA, RB>(a: A, b: B) -> (r: (RA, RB))
    requires a.requires(()), b.requires(())
    ensures  a.ensures((), r.0), b.ensures((), r.1);

impl spec rayon::iter::ParallelIterator for .. { .. }
spec fn par_sum_spec<T: Monoid>(s: Seq<T>) -> T = ..;
lemma fn par_sum_is_fold<T: Monoid>(..) ensures ..;   // 本体は props/rayon.eykp
```
- `props` ヘッダは MUST（**E0801**）。
- 外部の型への `impl View` / `impl Invariant` は props に書ける（Verus の `external_type_specification` に写す。OPEN-10）。

### 8.2 合成
同じ対象（`Path` ＋ ジェネリクス ＋ 引数型の列がトークン列として等しい）への `unchecked fn` 宣言が複数あるとき：
1. `requires` は全宣言でトークン列として等しいこと（**E0802**）。
2. `ensures` は全宣言のものを連結する。
3. `#[override]` が付いた宣言があればそれだけを採用する。複数は **E0803**。使用時は **W0804** で捨てた宣言の出所を列挙する。
4. 一つの `assume_specification` を出力する。
`impl spec` / `impl View` が複数パッケージから来て食い違えば **E0805**。

### 8.3 ロック
`Ebykn.lock` に、各性質パッケージの出所、版、内容ハッシュを記録する。

---

## 9. CLI

| コマンド | 動作 | 終了コード |
| --- | --- | --- |
| `ebykn init <name>` | 雛形生成 | 0 |
| `ebykn check [--base <rev>]` | §6 の全検査 → §7 → `verus --no-verify` | 0 / 1 / 2（使い方） |
| `ebykn verify [item...]` | `check` ＋ Verus による検証。`item` はパス（`dlist::DList::push_back`） | 同上 |
| `ebykn build [--verified]` | `check` ＋ `cargo verus build`。`--verified` は全項目が verified / unchecked でなければ 1 | 同上 |
| `ebykn sync` | 各 `proof/p.eykp` を `git merge-file proof/p.eykp .ebykn/sync/p.eyk src/p.eyk` で更新し、`.ebykn/sync/p.eyk` を新 src で置き換える。衝突はファイルに残し 1 | 0 / 1 |
| `ebykn status [--json]` | §9.1 | 0 |

全コマンドは `--json` で機械可読出力を持つ（rustc 互換の診断形式）。

### 9.1 `status`
1. Verus の結果（項目ごとの verified / failed）を取得する（OPEN-7）。
2. 呼び出しグラフ：§6.5 で解決できた呼び出しは辺、できない呼び出しは「外部」ノードへの辺。
3. 状態：`unchecked` 宣言は `unchecked`。本体が通り、到達可能な全ノードが verified か `unchecked` なら `verified`。それ以外は `unverified`（理由：本体の失敗 / 依存先 X が unverified / 補題に本体なし）。
4. 出力：各項目の状態と、`verified` 項目が依存する `unchecked` の一覧（項目名と出所：自クレート / 性質パッケージ名 / vstd）。

---

## 10. 診断

### 10.1 Ebykn の診断
| コード | 内容 |
| --- | --- |
| E0101 / E0102 | `__eyk_` 接頭辞の使用 ／ `#[verifier::..]` の使用 |
| E0301 / E0302 | 対応する `.eyk` のない `.eykp` ／ 置き場所の誤り |
| E0303 | `.eyk` に証明構文 |
| E0304 / E0305 | 剥がし不一致 ／ ループ句が部分列でない |
| E0306 | `.eyk` が `.eykp` だけの項目に依存 |
| E0401 / E0402 / E0403 | tuple struct・enum に `owns`/`spec` ／ 消去フィールドの欠落 ／ `invariant`・`view` の直接定義 |
| E0501 | `uses` / `gives` の対象の型が直接 `Ptr` / `Cell` でない |
| E0502 | 領域添字が識別子でない |
| E0503〜E0512 | 権限の規則違反（§6.2） |
| E0513 | `spec` フィールドを領域として使用 |
| E0514 | `uses` / `gives` 関数のメソッド呼び出し（解決不能） |
| E0515 / E0516 | `self` のない関数で `self@` ／ `owns` フィールドへの代入 |
| E0801〜E0805 | 性質ファイル |
| W0601 | 絞り込み型を展開しない位置で使用 |
| W0804 | `#[override]` の使用 |

形式：`ファイル:行:列: コード: メッセージ`。文言表は英語と日本語を持ち、既定は英語。

### 10.2 Verus の診断の写し戻し
- source map：`build/map/<生成ファイル>.json`。`[{ "gen": {"line":l,"col":c,"len":n}, "src": {"file":"src/x.eyk","line":l,"col":c,"len":n}, "kind": "token" | "expanded" }]`。展開で生じたトークン（契約の追加など）は `kind: "expanded"` で、展開元（`impl Invariant` や部分集合型の宣言位置）を指す。
- Verus の診断の位置を source map で変換する。変換できなければ生成位置をそのまま示し `（生成コード）` を付ける。
- 文言の置き換え：展開由来の precondition / postcondition の失敗は「型不変条件 `T::invariant` がここで成り立つことを示せません」「部分集合型 `T` の述語を示せません」に置き換える。

---

## 11. 受け入れテスト

`tests/golden/<name>/` に `src/`、`proof/`、`props/`、`expected/verus/`、`expected/diag.txt`、`expected/status.txt`。CI は (1) 生成 Verus がトークン列として一致、(2) 診断が一致、(3) `verify` の結果が `status.txt` と一致、を確かめる。

| 名前 | 内容 | 確かめること |
| --- | --- | --- |
| `stack` | 単方向スタック（付録 B） | §7.3 の基本、`impl Invariant` の展開 |
| `dlist` | 双方向リスト（付録 A） | `borrow mut`、`spec` フィールド、`.eykp` の補題、`closed spec fn` 経由の `requires` |
| `bank` | `Rc<Cell<Account>>` と `Region<Cell<..>>` の振替 | セル、`*c` |
| `finite_mesh` | `type FiniteMesh = Mesh where finite(self)`、`fn finite(m: Mesh) -> FiniteMesh`、`Result` 位置 | 部分集合型の展開 |
| `traits` | 契約付き trait と impl、`lemma fn` の法則 | §5.1 の trait |
| `props_merge` | 2 パッケージの `ensures` 合成、`#[override]` | §8.2 |
| `strip` | 一致／不一致の `.eykp`（契約の変更、ループ句の追加、文の追加、`.eyk` が `.eykp` の項目を参照） | §6.1、§6.6 |
| `linearity` | E0503〜E0512 を一つずつ出す負例 | §6.2 |
| `status` | 本体のない補題に依存する関数 | §9.1 |

`dlist`、`bank`、`finite_mesh` が全項目 verified になることを段階 1 の完了条件とする。

---

## 12. 実装の指針

- 言語：Rust。クレート：`ebykn`（CLI）、`ebykn_syntax`（字句・構文・AST）、`ebykn_check`（§6）、`ebykn_lower`（§7）、`ebykn_props`（§8）、`ebykn_prelude`（Verus クレート）。
- 構文解析：OPEN-1 の既定（`syn_verus` フォーク）。AST は Ebykn 独自の型に写す。
- 書き下しは `proc_macro2::TokenStream` を組み立てて文字列化し、`verusfmt` で整形する。トークンごとの元 Span を保持し、整形後に source map を再計算する。
- 3-way merge は `git merge-file`。
- マイルストーン（完了条件は §11 の対応テスト）：
  - M0：`ebykn_prelude` を Verus で書き、付録 B を Verus で直接書いて検証を通す。vstd の API 名、`?`、`for` ループ、Drop の対応を確認する（OPEN-3、OPEN-13）。
  - M1：構文解析と AST、構文エラー。`stack` の `.eyk` が読める。
  - M2：書き下し（権限・領域・`impl Invariant`）。`stack` が verified。
  - M3：`.eykp`、剥がし一致、自己完結検査、`sync`。`dlist` が verified。
  - M4：部分集合型、trait。`bank`、`finite_mesh`、`traits`。
  - M5：性質ファイルの合成、`status`、`interface.json`。
  - M6：source map と診断の写し戻し、全負例。

---

## 13. OPEN（未決。既定の挙動に従うこと）

| # | 事項 | 既定 |
| --- | --- | --- |
| OPEN-1 | 構文解析器：`syn_verus` フォークか手書きか | フォーク。診断の質で問題が出たら手書きに置き換える |
| OPEN-2 | `Cell<T>` の名前（std と同名） | `Cell<T>`（`ebykn_prelude::Cell`） |
| OPEN-3 | ユーザー定義 `Drop` | M0 で Verus の対応を確認。対応がなければ `owns` を持つ型の `Drop` を **E** にし、消費されずにスコープを抜ける値の警告は段階 2 |
| OPEN-4 | `borrow mut` の書き下し | `remove` ＋ `insert` |
| OPEN-5 | `*p` の読みの形 | 値が要る位置は `*p.read(..)`、参照が要る位置は `p.read(..)`。`Copy` でない型の値読みは Verus のエラーに任せる |
| OPEN-6 | `by(mode)` の一覧 | §5.2 の 5 つ |
| OPEN-7 | Verus の結果取得 | JSON 出力があればそれ、無ければ標準出力の `verification results::` と関数名を解析 |
| OPEN-8 | 型も `in` もない束縛子 `forall x, P` | 許す |
| OPEN-9 | 性質パッケージの解決 | `path` と `git` のみ |
| OPEN-10 | `impl spec` と外部型への `impl View` の詳細 | vstd `std_specs` の `external_trait_specification` ＋ `obeys_*`、`external_type_specification` に一対一で写せる最小形。M5 で確定 |
| OPEN-11 | 消去済み Rust クレートの作り方 | `build/verus` を Verus なしの rustc でビルドして確認（M0）。不可なら別 emitter を検討 |
| OPEN-12 | 契約付きクロージャの構文 | Verus の構文をそのまま通す |
| OPEN-13 | `?`、`for .. in` の `invariant`、`let-else` の Verus 対応 | M0 で確認。未対応なら該当構文を **E** にする |

---

## 付録 A：`dlist`

### A.1 `src/dlist.eyk`
```
pub struct Node<T> { pub val: T, prev: Option<Ptr<Node<T>>>, next: Option<Ptr<Node<T>>> }

pub struct DList<T> {
    head: Option<Ptr<Node<T>>>,
    tail: Option<Ptr<Node<T>>>,
} owns {
    nodes: Region<Ptr<Node<T>>>,
} spec {
    order: Seq<Ptr<Node<T>>>,
}

impl<T> View for DList<T> {
    type V = Seq<T>;
    closed spec fn view(&self) -> Seq<T> = self@order.map(|p| self@nodes[p].val);
}

impl<T> Invariant for DList<T> {
    closed spec fn invariant(&self) -> bool = and {
        self@order.no_duplicates(),
        forall p in self@order, self@nodes.contains(p),
        self.head == self@order.first(),
        self.tail == self@order.last(),
        forall i in 0..self@order.len(), and {
            self@nodes[self@order[i]].prev == self@order.get(i - 1),
            self@nodes[self@order[i]].next == self@order.get(i + 1),
        },
    };
}

impl<T> DList<T> {
    pub closed spec fn has(&self, h: Ptr<Node<T>>) -> bool = self@order.contains(h);

    pub fn new() -> Self {
        DList { head: None, tail: None, @nodes: Region::empty(), @order: Seq::empty() }
    }

    pub fn push_back(&mut self, v: T) -> (h: Ptr<Node<T>>)
        ensures self.view() == old(self).view().push(v), self.has(h)
    {
        let n = Ptr::new(Node { val: v, prev: self.tail, next: None });
        match self.tail {
            Some(t) => borrow mut self@nodes[t] { (*t).next = Some(n); },
            None    => self.head = Some(n),
        }
        self.tail = Some(n);
        put self@nodes[n];
        self@order = self@order.push(n);
        n
    }

    pub fn peek_front(&self) -> Option<&T> {
        let h = self.head?;
        borrow self@nodes[h] { Some(&(*h).val) }
    }

    pub fn remove(&mut self, h: Ptr<Node<T>>) -> (v: T)
        requires self.has(h)
        ensures  self.view() == old(self).view().remove(old(self)@order.index_of(h))
    {
        take self@nodes[h];
        let node = h.free();
        match node.prev {
            Some(p) => borrow mut self@nodes[p] { (*p).next = node.next; },
            None    => self.head = node.next,
        }
        match node.next {
            Some(q) => borrow mut self@nodes[q] { (*q).prev = node.prev; },
            None    => self.tail = node.prev,
        }
        self@order = self@order.remove(self@order.index_of(h));
        node.val
    }
}
```
`view`、`invariant`、`has` は非公開フィールドを参照するので `closed`（open にすると Verus が可視性のエラーを出す）。

### A.2 `proof/dlist.eykp`（A.1 の写し。足した部分に `// +`）
```
    pub fn push_back(&mut self, v: T) -> (h: Ptr<Node<T>>)
        ensures self.view() == old(self).view().push(v), self.has(h)
    {
        spec let before = *self;                                   // +
        let n = Ptr::new(Node { val: v, prev: self.tail, next: None });
        match self.tail {
            Some(t) => borrow mut self@nodes[t] { (*t).next = Some(n); },
            None    => self.head = Some(n),
        }
        self.tail = Some(n);
        put self@nodes[n];
        self@order = self@order.push(n);
        proof { push_keeps_invariant(before, *self, n); }          // +
        n
    }

// + .eykp だけの項目
lemma fn push_keeps_invariant<T>(a: DList<T>, b: DList<T>, n: Ptr<Node<T>>)
    requires a.invariant(), b@order == a@order.push(n), /* b@nodes、head、tail と a の関係 */
    ensures  b.invariant()
{
    assert b@order.no_duplicates();
    assert forall i in 0..b@order.len(), b@nodes[b@order[i]].prev == b@order.get(i - 1) by { .. };
}
```

### A.3 `push_back` の生成 Verus（抜粋）
```rust
pub fn push_back(&mut self, v: T) -> (h: Ptr<Node<T>>)
    requires old(self).invariant()
    ensures  self.invariant(), self@ == old(self)@.push(v), self.has(h)
{
    let ghost before = *self;
    let (n, Tracked(__eyk_perm_n)) = Ptr::new(Node { val: v, prev: self.tail, next: None });
    match self.tail {
        Some(t) => {
            let tracked __eyk_perm_t = self.nodes.borrow_mut().remove(t);
            t.borrow_mut(Tracked(&mut __eyk_perm_t)).next = Some(n);
            self.nodes.borrow_mut().insert(__eyk_perm_t);
        }
        None => { self.head = Some(n); }
    }
    self.tail = Some(n);
    self.nodes.borrow_mut().insert(__eyk_perm_n);
    proof { self.order = Ghost(self.order@.push(n)); }
    proof { push_keeps_invariant(before, *self, n); }
    n
}
```
（`Tracked` フィールドの借用形と `Ghost` 代入の正確な形は M0 で prelude と一緒に確定する。）

---

## 付録 B：`stack`（最小）
```
pub struct Node<T> { pub val: T, next: Option<Ptr<Node<T>>> }
pub struct Stack<T> { head: Option<Ptr<Node<T>>> } owns { nodes: Region<Ptr<Node<T>>> } spec { order: Seq<Ptr<Node<T>>> }

impl<T> Invariant for Stack<T> {
    closed spec fn invariant(&self) -> bool = and {
        forall p in self@order, self@nodes.contains(p),
        self.head == self@order.first(),
        forall i in 0..self@order.len(), self@nodes[self@order[i]].next == self@order.get(i + 1),
    };
}

impl<T> Stack<T> {
    pub fn new() -> Self { Stack { head: None, @nodes: Region::empty(), @order: Seq::empty() } }
    pub fn push(&mut self, v: T) {
        let n = Ptr::new(Node { val: v, next: self.head });
        self.head = Some(n);
        put self@nodes[n];
        self@order = seq![n] + self@order;
    }
    pub fn pop(&mut self) -> Option<T> {
        let h = self.head?;
        take self@nodes[h];
        let node = h.free();
        self.head = node.next;
        self@order = self@order.drop_first();
        Some(node.val)
    }
}
```
