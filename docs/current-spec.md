# 現行仕様

元の仕様（[docs/ebykn-spec.md](ebykn-spec.md)、版 0.7）を、実装しながら決めたことで上書きする文書です。元の仕様は実装を始める前の初期案で、正しい仕様は作りながら決めていきます（#12、#25 の作者のコメント）。

- ここに書いた項目は、元の仕様より優先します。書いていない項目は、元の仕様のままです。
- 項目は元の仕様の節の順に並べます。各項目は現行の仕様を先に書き、元の仕様の内容、根拠（決めた Issue / PR）、状態を添えます。
- 付録の例との食い違いは、最後の「[付録の例との食い違い](#付録の例との食い違い)」にまとめます。付録の例はラフスケッチなので、そこには正しい形を書きます。
- 実装の読み方（仕様の文言から一意に読めない点をどう読んだか）、未決の既定、Verus の実測の記録は [OPEN.md](../OPEN.md) に書きます。同じことを両方には書きません。
- この文書はエージェントが更新します。3 文書（`docs/ebykn-*.md`）はエージェントが変えません（[AGENTS.md](../AGENTS.md)）。

状態は次のどれかです。

| 状態 | 意味 |
| --- | --- |
| 作者が決めた | 作者が Issue で決めた |
| 判断待ちの既定 | `needs review` の Issue の判断が出るまでの既定。判断で変わりうる |
| 基準で決めた | 元の仕様が定めていないか、書いたとおりには作れない点を、設計ノート §3.1・§3.2 の基準で決めた |
| 実測で決めた | 固定した Verus（`release/0.2026.11.02.8bf68ee`）で確かめた事実で決まった。プローブは `OPEN.md` の「M0 の実測」にある |

決まっていて `develop` にまだ入っていない項目には「実装中（PR #N）」を添え、今の `develop` の挙動を「今の develop」として併記します。PR がまだないものは Issue の番号を添えます。`develop` に入ったら、その PR で印と併記を消します。

## §1 段階 1 の範囲

### §1.3 Rust の toolchain は stable

Verus は自分の toolchain で動きます。固定した Verus が要求するのは stable の `1.99.1-x86_64-unknown-linux-gnu` です。`ebykn` 自身は Verus の toolchain でビルドしません（このリポジトリは `rust-toolchain.toml` の 1.96.0）。`ebykn_prelude` と生成クレートは Verus で検証でき、Verus なしの cargo（1.96.0）でもビルドできます。

* 元の仕様：Rust の toolchain は「Verus が要求する nightly」。
* 根拠：#6（PR #13）。`verus --version` が要求する toolchain を出す。
* 状態：実測で決めた。

### §1.3 消去済みのクレートは `build/verus/` のまま（OPEN-11）

`build/verus/` は Verus なしの cargo でビルドでき、別の emitter は作りません（OPEN-11 の既定どおり）。そのために、書き下しは ghost だけの項目を `use` で取り込まず、パスで書きます。Verus なしでは、たとえば `use vstd::layout::size_of;` が「unresolved import」になるからです。`old` の `use` には `#[cfg(verus_keep_ghost)]` を付けます（§7.5）。

* 元の仕様：「`build/verus` を Verus なしの rustc でビルドして確認（M0）。不可なら別 emitter を検討」。
* 根拠：#6（PR #13）。
* 状態：実測で決めた。

## §3 プロジェクト構成

### §3.1 `.eyk` は vstd を直接参照しない

- `.eyk` は `vstd`、`verus_builtin`、`verus_builtin_macros`、`builtin` を直接参照しません。`use`、`extern crate`、パス、属性、マクロの引数のどこに書いても `E0001` です。`builtin` はよくある語なので、パスの先頭と `use` / `extern crate` の直後だけを見ます。
- `.eyk` に要る vstd の名前（`Seq`、`Set`、`Map`、`seq!` / `set!` / `map!`、`View`、`int`、`nat`）は裸の名前で書きます。生成する各 `.rs` の先頭の `use`（§7.5）がこれを解決します。
- `std`、`core`、`alloc`、`vstd`、`ebykn_prelude` という名前への改名と、外のパスからその名前を取り込む `use`（`extern crate foo as std;`、`use foo as vstd;`、`use foo::std;`）も拒否します。
- `ebykn_prelude` は vstd を glob で再公開しません。再公開してよいのは `View` と `CellId` だけです（`check-prelude-exports` が検査する）。glob で再公開すると、`.eyk` から `ebykn_prelude::admit` のようなパスで証明の関数に届きます。
- 制限：自分で定義した `mod builtin { .. }` の `builtin::x()` も拒否します（`crate::builtin::x()` なら通る）。

* 元の仕様：全 `.eyk` / `.eykp` に `use ebykn_prelude::*;` が暗黙に付く。vstd の参照は定めていない。
* 根拠：#16（PR #15、#20、#26）。パスは改名と再公開で綴りを変えられるので、名前ごとに塞いでも閉じない。
* 状態：判断待ちの既定（#16。マクロと合わせて #53 に委ねた）。

### §3.2 `Ebykn.toml` の読み方

- `[package]` の `name`・`version` と、`[verus]` の `release`・`commit` は必須です。`edition` を省くと `2024` です。
- §3.2 にない表と鍵は `E0002` です。知らない鍵を黙って捨てると、書いたつもりの設定が効きません。
- `[verus]` の `release` / `commit` が、`ebykn` をビルドしたときの固定値（このリポジトリのルートの `Cargo.toml` の `[workspace.metadata.verus]`）と違えば `E0002` です。
- `Ebykn.toml` はカレントディレクトリから上へ探します（cargo と同じ）。
- `[dependencies]` は値を保ったまま、鍵の名前の順に `build/verus/Cargo.toml` へ写します。
- 生成する `build/verus/Cargo.toml` の `vstd` は、crates.io の `vstd = "=0.0.0-2026-11-01-0251"`（`cargo verus new` が出す版）です。

* 元の仕様：§3.2 は例だけで、必須の鍵、知らない鍵の扱い、探し方、vstd の取り先を定めていない。
* 根拠：#17（PR #20、#26）、#6（PR #13）。
* 状態：基準で決めた。

## §4 字句

### 消去フィールドの `@ Ident` は、どの式の後にも書ける

`self@x` のほかに、`b@x`、`old(self)@x`、`final(self)@x` と書けます。`@` の直後が句のキーワード（`requires`、`ensures`、`returns`、`decreases`、`no_panic`、`uses`、`gives`、`by`、`invariant`）なら、消去フィールドではなく、権限 `p@` の後に句が始まると読みます。そのため、これらの名前の `spec` フィールドは `x@name` で読めません。

* 元の仕様：§4 の `@` の用法と §5.3 の `At` は `self @ Ident` だけ。
* 根拠：#14（PR #15、#20、#26）。二つの値の抽象状態を比べる契約や補題（`old(self)@order`、補題の `b@order`）に要る。
* 状態：作者が決めた。

### `borrow` は式の位置にも書ける

`borrow r[p] { .. }` / `borrow mut r[p] { .. }` は、式が始まる位置ならどこにでも書けます。`if` / `match` と同じブロック式で、値を持ちます。match の腕に書いたときは、続く腕の前に `,` が要ります（構文解析器のフォークが、Ebykn の式をブロック式と見なさないため）。

* 元の仕様：§4 の表は「文の先頭」、§5.4 は文。
* 根拠：#14。値を返す `peek_front` のような関数を素直に書ける。
* 状態：作者が決めた。

### `.eyk` の証明構文

`.eyk` では、次を証明構文として拒否します。今は `E0001` で、`.eykp` を実装する M3 で `E0303` に置き換えます。

- 文の先頭の `assert`（`assert!` は除く）、`proof { }`、`spec let`。
- 式のどの位置でも、裸のパスの呼び出し `assert(..)`・`reveal(..)`・`reveal_with_fuel(..)`・`hide(..)`。
- `assume(..)`・`admit(..)`。Ebykn にない Verus の関数として拒否し、`unchecked` を案内します。

パス付きの呼び出し（`m::assume(x)`）は、クレートの中の定義に解決するときだけ利用者の関数として読みます（§5「`.eyk` では Verus の構文を拒否する」。`std::hint::assume(true)` は `E0001`）。メソッドは利用者のメソッドとして読みます。

* 元の仕様：§4 は `assert`・`reveal`・`hide` を `.eykp` でだけキーワードとするので、`.eyk` では識別子になる。
* 根拠：#7（PR #15）。Verus に渡すと証明の構文として読まれ、`.eyk` に証明を書けてしまう（設計ノート §4.7）。
* 状態：基準で決めた。

## §5 文法

### `.eyk` では Verus の構文を拒否する

構文解析器（`vendor/verus_syn` のフォーク）は Verus の構文も受理しますが、`.eyk` ではそれを `E0001` で拒否します。Ebykn での書き方があれば、診断で示します（`forall|x|` → `forall x: T, P`、`proof fn` → `lemma fn`、`&&&` → `and { }` など。一覧は `crates/ebykn_syntax/tests/` の拒否の例）。

- 比較演算子の連鎖（`a < b < c`）も、syn と Rust が受理しないので拒否します。
- 属性は、`#[verifier::..]`（`E0102`）に加えて `#[verus::..]` も拒否します。どちらも `r#`、先頭の `::`、`cfg_attr(条件, ..)` の中まで見ます。
- `self@`・`Self@` は、権限ではなく Verus の view として拒否します。権限は `Ident "@"`、view は `.view()` です。
- `broadcast`・`uninterp`・`assume_specification` は、後ろに Verus の形（`broadcast use / group / proof / fn / axiom`、`uninterp spec`、`assume_specification [` / `<`）が続くときだけ Verus の構文とし、それ以外は識別子として読みます。
- Verus の句・証明・モード変換の組み込み関数（`crates/ebykn_syntax/src/reserved.rs` の `VERUS_BUILTIN_CALLS`）、証明の構文の関数、Verus のマクロ、`Invariant` / `View` の名前は、綴りによらずパスの最後の名前で判定します。裸の名前の呼び出しと、先頭が `::` のパスは `E0001` です。そのほかのパス（`crate::` / `self::` / `super::`、モジュール経由、改名する `use`）は、クレートの中の定義に解決するときだけ通します。診断は、同じ名前の自分の関数を `crate::..::requires(..)` のパスで呼べることを案内します。裸の名前を値として参照する形（`let r = requires;`、`(requires)(..)`）は、ローカル変数でなければ `E0001` です。中を読まないマクロの引数の中は名前解決しないので、どの綴りの呼び出しも拒否します。値を求めるだけの仕様の関数（`equal`、`imply`、`add` など）と、Ebykn が使う `old` は表に入れません。`crates/ebykn_check/tests/reserved.rs` が、表のすべての名前について綴りを総当たりで試します。

* 元の仕様：§5 は「Rust の文法（syn が受理する範囲）を土台」とし、Verus の構文の扱いを定めていない。
* 根拠：#7（PR #15）、#16（PR #20）。設計ノート §3.2「同じ概念を複数の形で書けるようにしない」。黙って受理すると、書き下しで Verus の意味になる（glob の `use vstd::prelude::*;` の下では、本体の `requires(x == 0);` が黙って事前条件になる）。
* 状態：基準で決めた。

### マクロ

マクロの扱いは、統一案 #53（needs review）の判断待ちです。#16（マクロの範囲）と #27（cfg と環境変数）の扱いは #53 に委ねました。

作者が示した方針は次の 2 つです。

- なるべく多くを許し、Verus で壊れる形だけを拒否リストで止める。壊れる形とは、検証したものと配るものが食い違う形と、証明が紛れ込む形。穴が見つかるたびに拒否リストを足して塞ぐ（#27）。
- マクロを一律に禁止することはとらない。tokio をはじめ多くのクレートはマクロを前提に作られていて、Rust のクレートの資産を使えるという原則に反する（#53）。

#53 の提案（判断待ち）：

1. 関数の本体の中のマクロは、どのクレートのものも使える。呼べるマクロの許可リストをやめ、壊れる形の拒否リストと、`ebykn verify` の出力の照合（§9、#37）で扱う。拒否リストの候補は、`.eyk` に書いた証明の入口（`verus!`、`proof!` などの名前と `verifier` の属性）、Verus が自分で設定する cfg（`verus_keep_ghost`、`verus_keep_ghost_body`、`verus_only`、`verus_verify_core`、`verus_no_vstd`）と `debug_assertions` を条件に含む `cfg!` / `#[cfg]`、`env!` / `option_env!` / `include!`。
2. Verus が対応していない std のマクロ（`println!` 系、`write!`、`writeln!`、`dbg!`）は、prelude の関数の呼び出しに書き下し、その関数を前提として `ebykn status` に出す。
3. derive・属性マクロを使えるようにし、検証されない部分を前提として一覧に出す。std の derive は Verus が検証する。関数に付く属性マクロ（`#[tokio::main]` など）は、検証する本体と、マクロを付ける薄い外側に分けて書き下す。
4. 利用者の `macro_rules!` を使えるようにし、モジュール直下で項目を作るものは前提として一覧に出す。
5. 関数の中の一部だけを `unchecked { .. }` ブロックにする逃げ道を足すかは、作者の判断（足すなら別の Issue で構文を詰める）。
6. 依存クレートのマクロの中身は、依存クレートと同じく信頼する前提として扱う（`assume` の類は §9 の照合で捕まえる）。

作者のコメント（#27）は、cfg で分岐するなら組み合わせごとに検証を走らせる方向（組み合わせを CLI で指定する）も示しています。形は決まっていません。

#16 の調査では、関数の本体の中の式・文のマクロ（std のマクロと利用者の `macro_rules!`）は、展開した後も Verus が手で書いたコードと同じく検証します（`println!` 系と `write!` は Verus が拒否する）。derive・属性マクロが `verus!` の外に作る項目は検証されず、それを前提に呼び出し側が verified になりえます。この穴は #49（P0）で扱います。依存クレートの derive と属性マクロは拒否せず、それが付いた項目を `ebykn verify` の前提の一覧に `macro ext_macros::One derive on external_macros::S` のような行で出します（#53 の提案の 3）。std の derive（`Clone`、`Copy`、`Debug`、`PartialEq`、`Eq`、`Hash`、`Default`、`PartialOrd`、`Ord`）は Verus が検証するので出しません。実装中（PR #57）。今の develop は、derive・属性マクロが作る項目を一覧に出しません。

今の develop（#16 の既定の許可リストと、#27 の既定 B）：

- `.eyk` で呼べるマクロは固定の一覧だけです（`crates/ebykn_syntax/src/lib.rs` の `ALLOWED_MACROS`）。
  - std の `vec`、`println`、`print`、`eprintln`、`eprint`、`format`、`format_args`、`write`、`writeln`、`panic`、`assert`、`assert_eq`、`assert_ne`、`debug_assert`、`debug_assert_eq`、`debug_assert_ne`、`unreachable`、`unimplemented`、`todo`、`matches`、`concat`、`stringify`、`line`、`column`、`module_path`、`include_str`、`include_bytes`、`compile_error`。裸の名前か、`std` / `core` / `alloc` から始まるパスで呼べます。
  - vstd の `seq`、`set`、`map`。裸の名前だけで呼べます。
- ほかのマクロの呼び出しは、どこに書いても拒否します。項目・文・式・パターン・型・trait / impl / extern の中、一覧のマクロの引数の中、属性の値（`#[doc = m!()]`）を含み、自作の `macro_rules!` の定義と `include!` も含みます。診断は一覧と、自作のマクロが要るならそれを使う関数を `unchecked fn` にすれば書けることを示します。
- `verus!` と、Verus・vstd の証明用・内部用のマクロ（`crates/ebykn_syntax/src/reserved.rs` の `VERUS_MACROS`）は、Verus の構文として拒否します。それを取り込む・改名する `use` と、一覧の名前を外のクレートから取り込む・その名前に改名する `use`（`use foo::vec;`）も拒否します。
- 属性は `#[path]`、`#[macro_use]`（`cfg_attr` の中を含む）を拒否し、`#[verus_verify]` / `#[verus_spec]` はパスの最後の名前で拒否します。
- vstd のマクロの引数は Ebykn の式として読みます。std のマクロの引数と、中を読まない Rust の式・型・項目・where 句・境界・属性の値はトークン列のまま持ち、証明と Verus の構文に当たる形（`proof { }`、`assert(`・`reveal(`・`reveal_with_fuel(`・`hide(`・`assume(`・`admit(`・`assume_(`、`forall|`・`exists|`・`choose|`、`let ghost` / `let tracked`、`proof fn` / `exec fn`）と、呼べないマクロの呼び出しを字句的に探して拒否します。`fn` の直後の名前は宣言なので見ません。`&&&` / `|||` は探しません。
- std のマクロの引数の中に権限の操作（Γ・R の名前への `*X`・`X@`・`X.free()`、`self@`、`take` / `put` / `borrow`、`leak X@`、`uses` / `gives` を持つ関数の呼び出し、`Ptr::new` などの権限を作る呼び出し）があれば `E0003` です。引数の中の属性（`#[..]`、`#![..]`）も、`cfg` に限らず `E0003` です。
- `cfg!`（`std::cfg!` / `core::cfg!` を含む）と、関数の中に付けた `#[cfg(..)]` / `#[cfg_attr(.., cfg(..))]` は `E0003` です。関数の中とは、文・式・`let`・match の腕・構造体リテラルの欄・構造体パターンの欄・引数・本体の中の項目と、本体の中で定義した構造体・列挙の欄と列挙子です。項目に付ける `#[cfg]` は通します（Verus が見なかった項目は `verify` で `unknown`、同じパスの重複は `E0003`）。
- `env!` / `option_env!` / `file!`（`std::` / `core::` から始まるパスを含む）は、`unchecked fn` の本体の外では、どこに書いても `E0003` です。値が要るなら `unchecked fn` で包みます。`unchecked fn` の本体の中の入れ子の `fn` では、その `fn` 自身が `unchecked` かどうかで決め直します。
- `line!` / `column!` / `module_path!` / `include_str!` / `include_bytes!` は、Verus の実行と素のビルドで同じ値になるので通します。ただし生成コードの位置とモジュールを指すので、`include_str!` で読むファイルは `build/verus/src/` から見たパス（`"../../../src/data.txt"` など）で書きます。
- 残る抜け道（#16）：外のクレートが vstd の項目を再公開する経路（`use foo::admit;`）、外のクレートからの glob の `use`（`use foo::*;`）、外のクレートの手続きマクロ（`#[derive(..)]`、属性のマクロ。#49）。

* 元の仕様：マクロの扱いを定めていない。§5 は Rust の文法を土台とする。
* 根拠：#16（PR #15、#20、#26）、#27（PR #26、`tests/golden/cfg_in_body/`）、#53、#49。`assume_specification .. ensures false` や `admit()` が一つ紛れ込むだけで何でも verified になる（P0）。
* 状態：判断待ちの既定（#53）。今の develop は上の許可リスト。derive・属性マクロの穴（#49）は実装中（PR #57）、cfg と `env!` などの拒否リストへの組み替え（#27）は実装中（PR #59）。

### §5.2 名前付きの消去される引数は、専用の構文で渡す

名前付きの消去される引数（`uses r: Region<..>` / `uses r: &mut Region<..>`）を持つ関数に、呼び出し側から領域を渡す専用の構文を作ります。構文の形は判断待ちで、案は次の 3 つです（#19）。判断が出るまでの既定は P1 です。

- P1（推奨）：括弧の中の末尾に `uses` を置く。`build_chain(n, uses &mut r)`。
- P2：`@` を付けて括弧の中の末尾に置く。`build_chain(n, @&mut r)`。
- P3：括弧の後ろに `@( )` を付けて分ける。`build_chain(n)@(&mut r)`。

呼び出し側に `&` / `&mut` を書くかも判断待ちで、推奨は「書く」です。

今の develop：宣言は受理し、それを持つ関数の呼び出しは `E0003` です。黙って何も渡さない書き下しにはしません。`uses r: T` の `T` が `Region<..>` / `&Region<..>` / `&mut Region<..>` でなければ `E0501` です。

* 元の仕様：宣言の書き下し（§7.3）だけがあり、呼び出し側の書き方がない。
* 根拠：#19（PR #20）。構造体を作る前の補助関数、二つの領域を行き来する再帰、領域の共有、補助関数の使い回しに要る。
* 状態：専用の構文を作ることは作者が決めた。構文の形は判断待ちで、既定は P1（#19）。develop ではまだ `E0003`。

### §5.4 match の腕の本体に直接書いたループの句

ループの句（`invariant`、`decreases`）は文法どおり、順序も繰り返しも自由です。ただし match の腕の本体に直接書いたループは、`invariant` → `decreases` の順に一回ずつしか書けません。

* 元の仕様：`LoopClause*`（順序も回数も自由）。
* 根拠：#7（PR #15）。match の腕の本体はフォーク（Verus の規則）がループを先に読む。
* 状態：基準で決めた。

## §6 静的検査

### §6.2 `borrow` ブロックを `break` / `continue` / `return` / `?` で抜けるとき

`borrow` / `borrow mut` ブロックを、普通に終わるときだけでなく、`break` / `continue` / `return` / `?` で抜けるときにも権限を返します（案 D）。Rust のブロックと同じく、ブロックを抜けたら借用が終わります。

- 書き下し：`borrow mut` のブロックの中の `break` / `continue` / `return` のうち、ブロックの外（外のループ、ラベル付きのブロック、関数）へ抜けるものの直前に `proof { <r>.insert(__eyk_perm_p); }` を置きます。ブロックの中のループやラベル付きのブロックで閉じる文には置きません。行き先の決め方は検査と同じです。`borrow mut` が入れ子なら、内側から順に戻します。
- 値を持って抜ける場合（`break 'a e` / `return e`）は、値を先に評価してから権限を戻します（`{ let __eyk_exit = e; proof { .. } return __eyk_exit }`）。値の式が借りている権限を読むことがあるためです（`return (*p).val` など）。
- `borrow mut` の中の `e?` は明示の `match` に展開し、失敗の腕で権限を戻してから返します。失敗の値の形は、関数の戻り値の型の最後の名前で決めます（`Option` なら `None`、`Result` なら `Err(::core::convert::From::from(e))`）。戻り値の型を `Option<..>` / `Result<..>` と書いていない関数では、`borrow mut` の中の `?` は `E0003` です（書き下しは型を知らず、型の別名も読まないので、推測で別の形を書かない）。
- 本体の後の `insert` は、発散の解析に頼らず常に置きます（`#[allow(unreachable_code)] let __eyk_res = { proof { <r>.insert(..); } __eyk_res };`）。本体が必ず抜けるときは届かない文になりますが、`let` に付けた属性で rustc の警告を止めます。解析の誤りが、普通に終わる経路での権限の消失につながらないようにするためです。
- マクロの引数の中の `return` / `break` / `continue` / `?` は、`borrow` の外でも `E0003` です。マクロの引数はトークンのまま Verus に渡るので、抜ける文の前に `insert` を置けず、検査も見ないためです（マクロの扱いが #53 と #28 で決まるまで）。
- Ebykn が Rust のトークンのまま持つ式（`.await` など）の中の `return` / `break` / `continue` / `?` も、関数のどこでも `E0003` です。理由はマクロの引数と同じです。
- クロージャの中の `return` / `break` はクロージャの外へ抜けないので、権限を戻す文は置きません。
- 共有の `borrow` は領域から権限を取り出さないので、戻すものはありません。
- 検査：抜ける点で、行き先より内側で開いた `borrow` の `p` を Γ から消します（ブロックの終わりと同じ）。抜けた先の Γ の比較（`E0510` / `E0511`）は、消したあとの Γ で行います。`return` / `?` で `Owned` の権限が残っていれば `E0505` です。`&&` / `||` の右辺は評価しないことがあるので、`else` のない `if` と同じに扱います（右辺だけが抜けても、式は必ず抜けるとは数えない）。
- 参照モデル（`formal/linearity`）は、この規則で健全性の証明と判定の表を作り直します（PR #60）。
- `panic = abort`（§1.3）なので、巻き戻しの経路はありません。

今の develop：参照モデル（`formal/linearity`）と判定の表（`formal/oracle/linearity.tsv`）は、まだ前の規則です（`borrow` ブロックの中から外の枠へ `break` / `continue` で抜ける文を `E0511` にする）。

* 元の仕様：この場合を書いていない。文言どおりに読むと `loop { borrow mut r[p] { break; } }` を受理し、ループの後に借用中の `p` が残る（健全でない）。`borrow mut` を `return` で抜けると、権限が領域に戻らない。
* 根拠：#21（PR #20、#22、#51）。
* 状態：作者が決めた（案 D）。参照モデルは実装中（PR #60）。

### §6.4 `ensures` の `&mut` 引数は `old` / `final` を明示する

`ensures` と `returns` に `&mut` 引数を `old(..)` も `final(..)` も付けずに書くと、`E0004` です。

- 対象は `&mut self`（`self: &mut Self` を含む）、`x: &mut T`、`uses r: &mut Region<..>` の `r`、`uses &mut p@` の `p@` と `*p` です。`self@f`、`self@f[p]`、`self.view()` の `self` も対象です。
- 量化子・クロージャの引数、`match` の腕、`let` のパターンで束縛し直した同じ名前は、その中では束縛した値を指すので対象外です。
- 文言は、事後の値の `final(x)` と入口の値の `old(x)` の両方の書き方を示します。
- `requires` の裸の `x` は入口の値です（Verus と同じ）。
- `old(e)` / `final(e)` の `e` が、`&mut` の引数、`uses &mut p@` の `p@` / `*p`、それらの消去フィールド（`x@f`）のどれでもなければ、どの句でも `E0004` です。Verus の `old` / `final` は `&mut` の引数そのものにしか付けられず、`old(self.len)` は生成コードの型エラーになります（`old(self).len` と書く）。
- 権限を持つ名前 `p`（`uses` の権限と、事後条件の `gives` の権限）を束縛し直した中の `p@` / `*p` は、どの句でも、`old` / `final` の下でも `E0507` です。書き下しは束縛に従わず外側の引数の権限を指すからです。
- 中を読まないマクロの引数（トークン列）も、`&mut` 引数の名前を探して `E0004` にします（位置はマクロ全体）。`.` / `::` の直後と、直後に `:` が続くパスの先頭・フィールドの名前は見ません（`S { f: x }` の `x` は見る）。`matches!(g(x), 3)` のように参照を丸ごと渡すと、裸のまま Verus に渡って入口の値と読まれ、黙って誤るためです。トークン列の中の束縛は読まないので、そこで束縛し直した名前も報告します。
- マクロの引数の中の `final(..)` も `E0004` です（Verus が受け付けない）。文言は、事後の値なら条件をマクロの外で書く（`*final(x) == 3`）よう案内します。マクロの引数の中の `old(x)` は書けます。
- `final(x)` を参照のまま丸ごと渡す形（`g(final(x))`、`final(x) == old(x)`）は Ebykn では拒まず、Verus に任せます（見分けるには型が要り、どちらも黙っては誤らない）。
- 書き下しは、利用者が書いた `ensures` をそのまま写します。暗黙に `final` で包みません。`old(x@f)` / `final(x@f)` は `old(x).f@` / `final(x).f@` に書き下します。
- Ebykn 自身が足す契約は `final` で書きます。§6.4 の `x: &mut T`（`&mut self` を含む）の展開は `requires P[old(x)]`、`ensures P[final(x)]` です。`uses &mut p@` に足す `ensures final(__eyk_perm_p).pptr() == p`（§7.3）も同じです。
- 論理の領域添字は `final(self)@f[p]`、`old(self)@f[p]`、`old(self@f)[p]`、`final(r)[p]` も領域として読み、`final(self).f@.index(p)`、`final(r).index(p)` などに書き下します。

* 元の仕様：§6.4 の表は `x: &mut T` に `requires P[old(x)]`、`ensures P[x]` を足す。§7.4 は `old(e)`、`final(e)` を「同じ」に写す。付録 A は裸の `self` を事後の値と読む。
* 根拠：#10（PR #35）。固定した Verus は、`ensures` の裸の `&mut` 引数を拒否する（`mut_ref_ensures.rs`）。
* 状態：作者が決めた。

## §7 書き下し

### §7.1 `Ptr<T>` の `T` はゼロサイズ型でない

`Ptr<T>` の `T` にゼロサイズ型（ZST）は使えません。次の 2 段で止めます。

- `ebykn check` が、`.eyk` の上で確実に ZST と分かる型を `Ptr<T>` に使ったら `E0517` にし、識別が要るなら `Cell<T>` を使うよう案内します。見るのは、型の位置の `Ptr<T>`、ターボフィッシュの `Ptr::<T>`、型を書かない `Ptr::new(e)` の `e`（`()`、unit struct の値、ZST の構造体のリテラル、ZST の列挙のフィールドのない列挙子 `M::A`）です。確実に分かる型とは、`()` と ZST だけの tuple、長さ 0 の配列と ZST の配列、`PhantomData`、実行時のフィールドがすべて ZST のクレートの構造体（フィールドのない構造体、unit struct、`owns` / `spec` だけの構造体を含む。型の引数は使う側の型を代入して判定する）、`repr` がなく、列挙子が 0 個か、フィールドがすべて ZST の列挙子が 1 個のクレートの列挙（`enum M { A }`。`#[cfg]` の付いた列挙子があれば判定しない）です。ジェネリックな `T` と、ジェネリックな構造体や型の別名の中の `Ptr<T>` を経由するものは判定しません（ZST ならビルドで止まる）。glob の `use` があれば判定しません。
- prelude の `Ptr::new` / `Ptr::empty` が、`verus!` の外に置いた定数 `assert!(size_of::<T>() != 0, ..)` を参照します。ZST で単相化するとコンパイルエラー（`E0080`）になり、ジェネリックな関数を経由しても止まります。`E0080` の文言は英語と日本語を併記します（rustc の定数の `assert!` は言語を切り替えられない）。検証のときは単相化しないので、この検査は効きません（検証では参照しない。参照すると Verus が拒否する）。

prelude の `PointsTo<T>` は型不変条件として `size_of::<T>() != 0` を持ちます。`Ptr::new` / `Ptr::empty` の `if size_of::<T>() == 0 { loop {} }` は、検証器に「ZST でない」を示すための分岐で、コンパイルされるどの実体でもコンパイル時の検査が先に止めるので到達しません。`Region::insert` は、鍵が新しいことを `requires` ではなく `ensures` で返します（利用者の義務にしない）。識別が要る中身のない型は、大きさによらず識別子を持つ `Cell<()>` で書きます。

* 元の仕様：`Region::insert` は `requires !old(self).contains(perm.loc())`。ZST に触れていない。
* 根拠：#12（PR #47）。vstd の `is_distinct` は `size_of::<T>() != 0` を前提にし、Verus はジェネリックな構造体の `size_of` が 0 でないことを示せない（`size_of_generic.rs`）。そのままでは `stack` の `push` を検証できない。
* 状態：作者が決めた。

### §7.1 `Seq` の `first()` / `last()` は vstd の意味

`first()` / `last()` は vstd のとおり要素を返します（空でないことが前提）。`Option` が要るときは `get(0)` / `get(len - 1)` と書きます。prelude の `SeqExt` は `get(i) -> Option<T>` だけを持ちます。`index_of`、`no_duplicates`、`remove`、`drop_first` は vstd のものを使います。書き下しは `.first()` / `.last()` をそのまま写します。

* 元の仕様：`Seq` の拡張として `first() -> Option<T>`、`last() -> Option<T>` を置く。
* 根拠：#11（PR #38）。vstd の固有メソッド `first()` / `last()` が拡張トレイトより優先されるので、`Option` を返す同名の拡張は作れない（`seq_first.rs`）。書き下しで別名に置き換えるのは、例を通すための暗黙の変換になる。
* 状態：作者が決めた。

### §7.1 prelude の `Ptr`・`Cell`・`Region` の API

`Ptr<T>`：

- `new` / `empty` は名前付きのタプル `(r: (Ptr<T>, Tracked<PointsTo<T>>))` を返します。§7.1 の `-> (p: Ptr<T>, Tracked<PointsTo<T>>)` は Verus の構文にありません。
- `read` / `write` / `borrow_mut` は、受け手を `&self` ではなく値で取ります（`Ptr` は `Copy`）。呼び出しの形 `p.read(..)` は変わりません。
- `write` は `T: Copy` を要求しません。初期化済みの前の値は drop せずに捨てます（`ptr::write` と同じ）。
- `free` は vstd の `into_inner` に当たります。vstd の `free` は未初期化の権限を要求し、値を返しません。

`Cell<T>`：

- vstd の `cell::pcell_maybe_uninit::PCell` の包みです（`vstd::cell::PCell` は非推奨）。
- `new` / `read` / `write` / `borrow_mut` / `id()` に加えて、`Ptr` にそろえて `empty` と `free`（権限を消費して中の値を返す）を持ちます。
- `id()` は spec 関数です（vstd の `PCell::id` が spec 関数だけのため）。`Ptr::addr` のように実行できる関数ではありません。
- 権限の型は `CellPointsTo<T>` で、`PointsTo<T>` とは別の型です。書き下しは、`uses` の対象の型が直接 `Ptr` か `Cell` かで選びます。

`Region<L>`：

- 全権限が初期化済みで、鍵の位置にあることを型不変条件に持ちます。`insert` は `is_init` を要求し、`remove` / `borrow` は `is_init` を返します。
- 契約は `spec_insert` / `spec_remove` ではなく、`perms()`（鍵から権限への `Map`）で書きます。
- 鍵は、`Ptr<T>` ではポインタそのもの（等しさはアドレスの等しさ）、`Cell<T>` では `CellId` です。`dom()` はその集合です。

* 元の仕様：§7.1 の API の一覧。
* 根拠：#6（PR #13）。プローブ `simple_pptr.rs`、`simple_pptr_write_needs_copy.rs`、`pcell.rs`、`pcell_deprecated.rs`、`tracked_map.rs`。
* 状態：実測で決めた。

### §7.2 項目の書き下し

- `spec fn` の可視性：`pub` でない `spec fn` は、`closed` の有無によらず `spec fn` とだけ書きます（`open` を付けない）。純粋関数の spec 側も同じです。Verus は `pub` でない `open spec fn` を拒否します。
- 純粋関数（`fn f(..) -> T = e;`）：戻り値の型が整数の基本型なら、spec 側の本体を `(e) as T` で包みます。spec 側では整数の算術が `int` になり、同じ式を写すと型が合わないからです。それ以外（引数の位置の算術など）は Verus のエラーに任せます。
- 関数単位の `by(m)`：`.eykp` が入る M3 からの規則です。書き下しは署名の直後（`requires` の前）に出します。`ensures` の後に置くと Verus の構文エラーです。今の `.eyk` では、`by(m)` は証明の構文として拒否します。
- 利用者の `impl Drop`：`drop` に `opens_invariants none` と `no_unwind` を足すと決めましたが、develop にはまだありません。今は `impl Drop` が Verus のエラー（`a Drop implementation must be marked opens_invariants none`）になり、利用者は `opens_invariants` を書けません（#61）。

* 元の仕様：§7.2 の表（`spec fn` は「`open spec fn f(..)`（`pub` はそのまま）」、純粋関数は同じ式を `fn` と `spec fn` に写す）、§5.2 の正規順（`by` が最後）。
* 根拠：#17（PR #26）、#6（PR #13）。プローブ `when_used_as_spec.rs`、`prover_mode_by_last.rs`、`drop_opens_none.rs`。
* 状態：`spec fn` は基準で決めた。ほかは実測で決めた。`impl Drop` は develop にまだない（#61）。

### §7.3 構造体と資源の書き下し

- `put r[p];` は `proof { <r>.insert(__eyk_perm_p); }` です。`borrow mut` の末尾の `insert` も `proof { }` に入れます。実行コードに文として置くと、proof のモードの関数を呼べないエラーになります。
- 論理文脈の `<r>` は `self.r@` です（`r[p]` は `self.r@.index(p)`）。
- `uses &mut p@` には、`requires __eyk_perm_p.pptr() == p` に加えて `ensures final(__eyk_perm_p).pptr() == p` を足します（呼び出しの後も `*p` を使えるように）。`Cell` は `.id() == c.id()` で、`&Ptr<T>` の引数は `*p` と比べます。
- `-> (q: Ptr<T>) gives q@` は `-> (__eyk_ret: (Ptr<T>, Tracked<PointsTo<T>>))` にします。`ensures` の `q` を `__eyk_ret.0`、`q@` を `__eyk_ret.1@` に付け替え、`ensures __eyk_ret.1@.pptr() == __eyk_ret.0` を足します。本体の末尾と `return e` は `(e, Tracked(__eyk_perm_q))` です。§7.3 の `-> (q: Ptr<T>, Tracked<PointsTo<T>>)` は Verus の構文にありません。
- 権限の変数を渡すときは、権限そのものなら `Tracked(&x)` / `Tracked(&mut x)`、借用（`uses &p@`、`uses &mut p@`、`borrow`）なら `Tracked(&*x)` / `Tracked(&mut *x)` です。
- 型を書いた `let`（`let p: Ptr<u8> = Ptr::new(..)`。`Ptr::empty`、`Cell::new`、`gives` の呼び出しも同じ）は、`let (p, Tracked(__eyk_perm_p)): (Ptr<u8>, Tracked<_>) = ..` にして型注釈を残します。型注釈を落とすと値の型が変わります。
- 値で受けた領域 `uses r: Region<L>` は、引数を `Tracked(r): Tracked<Region<L>>` のまま、本体の先頭で `#[allow(unused_mut)] let tracked mut r = r;` と束縛し直します。`take` / `put` / `borrow mut` で変更できるようにするためです。Verus は引数のパターン `Tracked(mut r)` を受け付けません（「ill-formed `unwrap_parameter` header」）。`#[allow(unused_mut)]` は、変更しない関数で警告を出さないためです。

* 元の仕様：§7.3 の表（`put` は `<r>.insert(..);`、論理の `<r>` は `self.r.borrow()`、`gives` は `-> (q: Ptr<T>, Tracked<PointsTo<T>>)`、`uses r: Region<L>` は `Tracked(r): Tracked<Region<L>>` だけ）。
* 根拠：#6（PR #13、プローブ `tracked_field_forms.rs`、`tracked_field_a3_literal.rs`）、#17（PR #26）、#90（`tests/golden/lowering` の `Bag`）。
* 状態：`put`、論理の `<r>`、値で受けた領域の束縛し直しは実測で決めた。ほかは基準で決めた。

### §7.4 式の書き下し

- `and { a, b, c }` は `((a) && (b) && (c))` です（各項を括弧で包む）。`or { }` も同じです。Verus の `forall` / `exists` は右へできるだけ伸びるので、§7.4 の `(a && b && c)` のままでは `forall` の項が後ろの項を取り込みます。
- 量化子の本体 `P` は、`forall x in r, P` では `<==>` のとき、`exists` では `||`・`==>`・`<==>` のときに括弧で包みます。範囲の両端は、単純な式（リテラル、パス、呼び出し、フィールドなど）でなければ括弧で包みます。
- 範囲の束縛子に型を付けます。`forall i in a..b` / `exists i in a..b` の `i` の型は、端のリテラルの接尾辞（`3int`、`10u64`）、なければ `int` です。型がないと、本体で型が決まらないとき Verus が E0282 で止まります。

* 元の仕様：§7.4 の表（`(a && b && c)`、`forall|x| <展開> ==> P`）。
* 根拠：#17（PR #26）。
* 状態：基準で決めた。

### §7.5 生成する `.rs` の先頭は、名前を挙げた `use`

各 `.rs` の先頭は、glob の `use vstd::prelude::*;` ではなく、`.eyk` と `verus!` が使う名前だけを挙げた `use` です（`crates/ebykn_lower/src/lib.rs` の `HEADER_USES`）。

- vstd から取り込むのは `Seq`・`Set`・`Map`・`seq`・`set`・`map`・`View`・`int`・`nat`・`Tracked`・`Ghost`・`verus` と、仕様の算術のトレイト（`as _`）です。`old` は `#[cfg(verus_keep_ghost)]` を付けて取り込みます。そのあとに `use ebykn_prelude::*;` を置きます。
- `Tracked` / `Ghost` / `old` は、`verus!` が裸の名前でしか読まないので、裸の名前で取り込みます。実行コードから `Tracked::assume_new()` / `Ghost::assume_new()` は呼べません（`tracked_assume_new_exec.rs`）。
- 書き下しが自分で使う `admit` は、`vstd::prelude::admit()` のパスで書きます。
- インラインの `mod` の中にも同じ `use` を置きます（Rust のモジュールは親の `use` を見ない）。

* 元の仕様：§7.5「各 `.rs` の先頭は `use vstd::prelude::*; use ebykn_prelude::*; verus! {`」。§1.3・§7.1「書き下しの出力は vstd を直接参照せず、prelude の API だけを参照する」。名前を挙げた `use vstd::prelude::{..}` と `vstd::prelude::admit()` は、§1.3・§7.1 の規則とも違う。
* 根拠：#16（PR #26）。glob の下では、本体の文 `requires(x == 0);` が黙って事前条件になる（`prelude_glob_requires_in_body.rs`）。明示の `use` の下では名前が見つからない（`explicit_prelude_uses.rs`）。`Tracked` / `Ghost` / `old` は `verus!` が裸の名前で読むので、prelude を通せない。
* 状態：判断待ちの既定（#16。#53 に委ねた）。

### §7.5 / §12 整形と組み立て

書き下しは文字列で組み立て、`proc_macro2::TokenStream` に読み直してから、自前の整形器（`crates/ebykn_lower/src/print.rs`）で書き出します。`verusfmt` は使いません（§7.5 は SHOULD）。トークンごとの元の位置はまだ持たず、source map（§10.2）を作る M6 でトークン列の組み立てに置き換えます。

* 元の仕様：§12「`proc_macro2::TokenStream` を組み立てて文字列化し、`verusfmt` で整形する。トークンごとの元 Span を保持する」。
* 根拠：#17（PR #26）。golden はトークン列で比べるので、整形の形は比べない。
* 状態：基準で決めた。

## §9 CLI

### 診断の言語と `--json`

- 診断の言語は、全サブコマンド共通の `--lang en|ja` で選びます（既定は `en`）。
- `--json` は、rustc 互換の JSON を 1 行に一つ標準出力に出し、テキストの診断は標準エラーに出します（cargo の `--message-format=json` と同じ）。
- `check` は、警告（`W0601`）だけなら終了コード 0 です。
- `check --json` は、診断の JSON の後に `{"$message_type":"ebykn-summary",..}` を 1 行出し、件数と、まだ実行しない段階を `skipped` に挙げます（今は §6.6 だけ）。テキストの出力だけで飛ばした段階を伝えると、機械で読む側には一部の検査だけで通ったことが分からないためです。`Ebykn.toml` が見つからないときも JSON の診断を出します。
- まだないサブコマンドと `check --base` は、終了コード 2 と、どのマイルストーンで実装するかの案内を出します。

* 元の仕様：§10.1 は文言表に英語と日本語を持つとするが、選び方を定めていない。
* 根拠：#17（PR #20）。
* 状態：基準で決めた。

### `ebykn verify` の項目の状態と出力

`verify` は `cargo verus focus -- --triggers-mode silent --output-json` で検証します。この実行には `--no-cheating` を付けません（本体のない `lemma fn` の `admit()` と、`unchecked` の `external_body` が拒まれるため）。

項目は書き下した関数（`spec fn`、trait の既定のメソッドを含む）です。状態は一つの関数（`crates/ebykn/src/verus.rs` の `decide`）で決めます。

| 状態 | 意味 |
| --- | --- |
| `admit` | 本体のない補題 |
| `unchecked` | `unchecked` の項目 |
| `external` | `#[test]`。Verus は見ない |
| `failed` | Verus のエラーの主な位置が、項目の行の範囲にある |
| `unknown` | Verus の `--output-json` の `func-details` に項目の関数がない（Verus が見なかった。`#[cfg]` で消えた項目など）か、どの項目にも割り当てられないエラーがあるか、Verus が検証の前に止まった |
| `verified` | それ以外。Verus 自身が検証した関数で、エラーがない |

- 前提の状態（`admit` / `unchecked` / `external`）は、エラーより優先します（止まった実行に限りません）。コンパイルエラーを持つ `unchecked fn` も `failed` ではなく `unchecked` と出ます。そのエラーの場所は、標準エラーに出す Verus の診断の生成コードの位置で示します。

- 項目の Verus の名前は、impl のメソッドなら型の完全なパスの下、trait の既定のメソッドなら trait の下です。クレートの外の型への impl のメソッドは `impl を置いたモジュール::impl&%N::名前` で、形に当たる鍵がちょうど一つのときだけ決まり、決まらなければ `unknown` です（表示は `<u32 as Tr>::名前`）。同じ Verus の名前になる項目が二つ以上あれば、全部 `unknown` です。
- Verus の数えたエラーと読み取ったエラーの見出しの数が違えば、エラーを割り当てられないとみなします。
- `failed` か `unknown` があれば、`verify` は失敗します。
- 証明なしの前提（`admit` / `unchecked` / `external`）は、`verify item..` の絞り込みによらず全体から出します（絞り込むのは表示だけで、検証は全体）。
- 出力は、項目ごとの「状態 パス」の行と、最後の `results N verified, M errors` です（`status`（M5）の形が決まるまでの仮の形）。
- Verus が検証の前に止まったとき（コンパイルエラーなどで、検証のエラーが 0 件か件数がない）は、最後の行を `results not verified: Verus stopped before verification (N errors)` にし、何も検証していないことを英日の文言でも出します（`--output-json` はコンパイルエラーでも `verified: 0, errors: 0` を出します）。このとき（`--json` の `ran: false`）の項目の状態は、前提なら `admit` / `unchecked` / `external` のまま、そうでなくエラーの主な位置がその項目の行の範囲にあれば `failed`、ほかはすべて `unknown` です。`func-details` に関数があっても `verified` にしません（Verus は止まる前に関数の一覧を出すことがある）。テキストと `--json` は同じ状態を出します。`tests/golden/stopped/`（Verus が対応しない std の関数 `std::process::id` を呼ぶ）で固定してあります。
- `--json` では、1 行の JSON（`$message_type: ebykn-verify`。`results` は `{"ran": true, "verified": N, "errors": M}` か `{"ran": false, "errors": N}`）を標準出力に出し、Verus の診断は標準エラーに出します。
- Verus の出力は `CARGO_TERM_COLOR=never` で受け取り、制御文字を除いて読みます。Verus の診断は、生成コードの位置を `build/verus/src/..（生成コード）` として出します（§10.2 の変換できない場合の形）。

紛れ込んだ前提の照合：1 回目が検証まで進んだら、`cargo verus focus -- --no-cheating --no-verify` をもう一度走らせ、出たエラーを、Ebykn が自分で出した前提（`unchecked` と本体のない補題）と照らし合わせます（`crates/ebykn/src/no_cheating.rs` の `judge`）。1 回目が検証の前に止まったときは走らせません（`verify` はそれで失敗する）。`--no-verify` でも `--no-cheating` の拒否は全部出るので、2 回目は検証を省きます。

- 許すのは、`unchecked` の項目の行の範囲にある `external_body` の拒否と、本体のない補題の行の範囲にある `assume` / `admit` の拒否を、項目ごとに一つずつだけです。`#[test]` の `#[verifier::external]` は `--no-cheating` でも拒否されないので、許す拒否はありません。
- それ以外のエラーは、拒否の文言か、生成コードの中か（`include!` の位置は `src/../../../x.in` のように生成コードの外になる）を問わず、紛れ込んだ前提として `verify` を失敗にし、その場所を出します。依存クレートのマクロ、`include!`、マクロが出す入れ子の `verus!` から入った `assume(false)` を、これで捕まえます。
- 次のときは、照合できないとして失敗にします：Verus が失敗したのに見出しを読めない、cargo の数えたエラー（`due to N previous errors`）と読んだ見出しの数が違う、cargo がクレートを作り直した行（`Checking 名前 v..`）がない。cargo は同じ位置・同じ文言のエラーを 1 つにまとめて出すが件数はまとめる前の数のままなので、Ebykn の `admit()` と同じ位置に紛れ込んだ拒否も、この件数の食い違いで照合できないことになります。各実行の前の `cargo clean -p` が失敗したら、`verify` を止めます。
- 紛れ込んだ前提があるか照合できないときは、どの項目がそれに頼るか分からないので、`verified` の項目をすべて `unknown` にします。
- 出力は、テキストでは `smuggled 位置 文言` の行（照合できなければ `smuggled unreadable: 理由`。位置は、生成コードならプロジェクトのルートからのパスを字句的に正規化したもの）、`--json` では `no_cheating`（`state` が `clean` / `smuggled` / `unreadable` / `not_run`、`checked`、`smuggled` の位置と文言か `reason`）です。標準エラーには、紛れ込んだものの Verus の診断と英日の文言を出します（照合できないときは、Ebykn 自身の前提への拒否も含むと断って、2 回目の出力の全文）。
- 捕まえるのは、Verus が `--no-cheating` で拒否するもの（`assume` / `admit` / `external_body` / `assume_specification`、`assume_termination` など）が、Ebykn の前提の一覧の外に出たときだけです。`verus!` の外で依存クレートの derive・属性マクロが作る項目は、Verus が検証しないだけで拒否しないので捕まえません（#49）。
- M5 で性質ファイル（trait の仕様）を足すと、一つの仕様からメソッドごとに拒否が出うるので、「許す拒否は項目ごとに一つ」の規則をそのとき仕様の単位で数え直します。
- `tests/golden/smuggled/` と `crates/ebykn/tests/no_cheating.rs`（Verus が要るので `#[ignore]`。`cargo xtask golden-verify` が走らせる）で固定してあります。

* 元の仕様：§9 は `verify` を「`check` ＋ Verus による検証」とし、項目の状態と出力の形を §9.1（`status`）に置く。`--no-cheating` に触れていない。
* 根拠：#17（PR #26）、#43（PR #45）、#37（PR #42）。表示の根拠を Verus 自身の証拠（`func-details` とエラーの位置）にし、根拠のない verified を出さない。
* 状態：基準で決めた。

### §9.1 Verus の結果の取り方（OPEN-7）

Verus の `--output-json` は、件数（`verification-results` の `verified` / `errors` / `success`）と、検証した関数の名前の一覧（`func-details`）を出します。どの関数が失敗したかは JSON になく、標準エラーの診断（位置つき）にだけ出ます。そのため、失敗した関数は診断の位置から決めます（上の `failed`）。決められないときに `--verify-function` で項目ごとに検証し直すかは、`status` の M5 で決めます。

* 元の仕様：OPEN-7「JSON 出力があればそれ、無ければ標準出力の `verification results::` と関数名を解析」。
* 根拠：#6（PR #13、`json_output.rs`、`json_output_failure.rs`）。
* 状態：実測で決めた。

## §10 診断

### §10.1 仕様にないコード

元の仕様の §10.1 の表にないコードです。元の仕様は、構文エラー、プロジェクトの誤り、未実装の機能、§6.4 の `&mut` の事後条件、ZST の `Ptr` に当たるコードを持ちません。

| コード | 内容 | 元の仕様 | 根拠 | 状態 |
| --- | --- | --- | --- | --- |
| `E0001` | 構文エラー。Verus にあって Ebykn にない書き方、`.eyk` の証明構文（M3 で `E0303` に置き換える）、性質ファイルだけの構文も、文言を分けて `E0001` | コードがない（§10.1 の形式はコードを要する） | #7（PR #15） | 基準で決めた |
| `E0002` | プロジェクトの誤り。`Ebykn.toml` がない・読めない・知らない鍵・型が違う・`[verus]` が `ebykn` の固定値と違う、`mod foo;` のファイルがない・二つある、同じモジュールの二度の宣言、クレートルートがない・二つある | コードがない | #17（PR #20） | 基準で決めた |
| `E0003` | まだ実装しない機能。文言にどのマイルストーンで実装するかを書く（`.eykp`、`props/`、`[props]`、`[ebykn-dependencies]`、部分集合型、契約・`spec fn`・`lemma fn` を持つ trait、`impl spec`、`build --verified`、名前付きの消去される引数の呼び出しなど） | コードがない（§1.1 は段階 1 の機能を挙げるだけ） | #17（PR #20） | 基準で決めた |
| `E0004` | `ensures` / `returns` の裸の `&mut` 引数、`old` / `final` の引数が `&mut` の引数でないもの、マクロの引数の中の `final(..)`（§6.4） | §6.4 は `ensures` の裸の `x` を事後の値と読む | #10（PR #35） | 作者が決めた |
| `E0517` | `Ptr<T>` の `T` が確実にゼロサイズ型（§7.1） | ZST に触れていない | #12（PR #47） | 作者が決めた |

`E0001`〜`E0004` は、§10.1 の番号帯（E01xx 字句、E03xx 証明ファイル、E04xx 構造体、E05xx 権限、W06xx 絞り込み、E08xx 性質ファイル）とぶつからない E00xx に置きます。`E0517` は権限の番号帯の続きです。黙って無視すると、書いた性質や証明が効いていないのに検査が通るので、未実装の機能も `E0003` で止めます。

### §10.1 位置の数え方

`ファイル:行:列` の行と列は 1 から数え、列は Unicode のスカラー値（`char`）の数で数えます（rustc の JSON 診断の `column_start` と同じ）。長さとファイル内の位置はバイトで持ちます。

* 元の仕様：形式（`ファイル:行:列: コード: メッセージ`）だけを決める。
* 根拠：#7（PR #15）。M6 で Verus の診断を写し戻すとき、同じ数え方なら変換が要らない。
* 状態：基準で決めた。

## §11 受け入れテスト

### `stack` が verified になるのは M3

`stack` は `.eyk` だけでは verified になりません。`invariant` と `new` は verified、`push` と `pop` は failed（どちらも「postcondition not satisfied：`final(self).invariant()`」）です。不変条件が `closed` で、`forall` の展開と帰納法が要るので、Verus が自動では示せません。証明は M3 の `.eykp` で足し、`stack` の golden が verified になることを M3 の完了条件にします。

M3 の `.eykp` で示す義務は次のとおりです。

- `push`：新しい `n` が古い `nodes` にないこと（`Ptr::new` の権限と `Region::insert` の `ensures`）と、`order` の添字が一つずれることから、不変条件の `forall` の 2 項を示す。
- `pop`：`order` に重複がないこと（`next` をたどると末尾の `None` に着くことからの帰納法。M0 の `lemma_no_duplicates` / `lemma_shifted_equal`）から、`h` が残りの `order` にないことと添字のずれを示す。

`tests/golden/stack/expected/verus/` は、M0 の手書きの stack（2bca455）から `// +` の証明を除いたものです（ほかの違いは先頭の `use`、`and { }` の括弧、範囲の束縛子の型 `: int`）。

* 元の仕様：§12 の M2 の完了条件は「`stack` が verified」。
* 根拠：#17（PR #26）。証明は M0 の手書きの stack（2bca455）にある。
* 状態：基準で決めた。

## §13 OPEN の結果

元の仕様の OPEN のうち、M0 の実測で結果が出たものです。

| OPEN | 元の既定 | 結果 | 出所 | 状態 |
| --- | --- | --- | --- | --- |
| OPEN-3（ユーザー定義 `Drop`） | M0 で確かめ、対応がなければ `owns` を持つ型の `Drop` を E にする | Verus は対応している（`opens_invariants none` と `no_unwind` を付ければ検証できる）ので、E にしない。書き下しで `drop` に両方を足すと決めたが、develop にはまだなく、今は `impl Drop` が Verus のエラーになる（#61、§7.2）。`drop` は型の不変条件を前提にできず、`requires` も付けられないので、`owns` の権限を使って領域を解放することはできない。消費されずにスコープを抜ける値の警告は段階 2（既定どおり） | #6（PR #13）、`drop.rs`、`drop_opens_none.rs`、`drop_requires.rs`、`drop_tracked_field.rs` | 実測で決めた |
| OPEN-4（`borrow mut` の書き下し） | `remove` ＋ `insert` | 既定どおり。`insert` は `proof { }` に入れる（§7.3） | #6（PR #13）、`tracked_field_forms.rs` | 実測で決めた |
| OPEN-6（`by(mode)` の一覧） | §5.2 の 5 つ | 構文は 5 つを受け付け、使えないものは Verus のエラーに任せる。使えるのは場所で違う：`assert .. by(m)` は `bit_vector` / `nonlinear_arith` / `compute` / `compute_only`、関数単位の `by(m)` は `bit_vector` / `nonlinear_arith`。`integer_ring` は、Singular 付きでビルドした Verus でないと使えない | #6（PR #13）、`prover_modes.rs`、`prover_mode_*.rs` | 実測で決めた |
| OPEN-7（Verus の結果の取り方） | JSON 出力があればそれ、なければ標準出力を解析 | §9.1 | #6（PR #13）、`json_output.rs`、`json_output_failure.rs` | 実測で決めた |
| OPEN-11（消去済みのクレート） | Verus なしの rustc でビルドして確かめ、不可なら別の emitter を検討 | §1.3 | #6（PR #13）、`verus-verify` と `golden-verify` | 実測で決めた |
| OPEN-12（契約付きクロージャ） | Verus の構文をそのまま通す | Verus の構文（`\|x\| -> (r: T) requires .. ensures .. { .. }`）が通る。既定どおり | #6（PR #13）、`closure.rs` | 実測で決めた |
| OPEN-13（`?`、`for .. in` の `invariant`、`let-else`） | M0 で確かめ、未対応なら E にする | どれも通るので E にしない。`?` は `Option` と `Result` で通り、早期 return の経路も事後条件で検査される | #6（PR #13）、`question_mark.rs`、`for_loop.rs`、`let_else.rs` | 実測で決めた |

## 付録の例との食い違い

付録と本文の例は、実装を始める前のラフスケッチです。次の箇所は、現行の仕様では通らないか、別の形が正しい形です。golden と Book は正しい形で書きます。

| 箇所 | 付録・本文の形 | 正しい形 | 根拠 |
| --- | --- | --- | --- |
| 付録 A.1 の `view` | `self@order.map(\|p\| self@nodes[p].val)` | `self@order.map_values(\|p\| self@nodes[p].val)`。vstd の `Seq::map` は添字と要素の 2 引数のクロージャを取る | #25 |
| 付録 A.1 の `remove` の `ensures` | `old(self)@order.index_of(h)` | `closed spec fn` を通す（例：`old(self).index_of_node(h)`）。`pub` の関数の契約で非公開のフィールドは読めない | #25 |
| 付録 A.1 の不変条件 | `self.head == self@order.first()`、`self.tail == self@order.last()` | `self.head == self@order.get(0)`、`self.tail == self@order.get(self@order.len() - 1)` | #11 |
| 付録 A.1 の `push_back` の `ensures` | `self.view() == old(self).view().push(v), self.has(h)` | `final(self).view() == old(self).view().push(v), final(self).has(h)` | #10 |
| 付録 A.1 の `remove` の `ensures` の左辺 | `self.view() == ..` | `final(self).view() == ..` | #10 |
| 付録 A.2 の `push_back` | A.1 と同じ `ensures` | A.1 と同じく `final(self)` | #10 |
| 付録 A.3 の生成 Verus | `ensures self.invariant(), self@ == old(self)@.push(v), self.has(h)` | `ensures final(self).invariant(), final(self)@ == old(self)@.push(v), final(self).has(h)` | #10 |
| 付録 A.3 の生成 Verus | `self.nodes.borrow_mut().insert(__eyk_perm_n);`（文） | `proof { self.nodes.borrow_mut().insert(__eyk_perm_n); }`（§7.3） | #6 |
| 付録 B の不変条件 | `self.head == self@order.first()` | `self.head == self@order.get(0)`。`tests/golden/stack/src/lib.eyk` が付録 B と違うのはこの 1 か所だけ（`crates/ebykn_syntax/tests/appendix.rs` が確かめる） | #11 |
| §7.1 の `Seq` の拡張 | `first() -> Option<T>`、`last() -> Option<T>` | prelude に置かない。vstd の `first()` / `last()`（要素を返す）を使う | #11 |
| §7.1 の `Ptr::new` / `empty` | `-> (p: Ptr<T>, Tracked<PointsTo<T>>)` | `-> (r: (Ptr<T>, Tracked<PointsTo<T>>))`（§7.1） | #6 |
| §7.3 の `gives` | `-> (q: Ptr<T>, Tracked<PointsTo<T>>)` | `-> (__eyk_ret: (Ptr<T>, Tracked<PointsTo<T>>))`（§7.3） | #17 |
| §6.4 の表の `x: &mut T` | `ensures P[x]` | `ensures P[final(x)]`（§6.4） | #10 |
| §7.4 の `and { }` | `(a && b && c)` | `((a) && (b) && (c))`（§7.4） | #17 |
