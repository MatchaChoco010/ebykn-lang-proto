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

### §3.1 証明ファイルの置き場所

- `proof/<p>.eykp` は、クレートのモジュールとして読んだ `src/<p>.eyk` の証明です（`proof/a/mod.eykp` は `src/a/mod.eyk`）。対応する `src/<p>.eyk` がなければ `E0301` です。ファイルがあっても `mod` で宣言していない（クレートのモジュールでない）`src/<p>.eyk` に対応する `.eykp` も `E0301` で、文言で `mod` の宣言を案内します。
- `proof/` の外の `.eykp` は `E0302` です（`props/` の下は性質ファイルで、M5 まで `E0003`）。`proof/` の下の `.eyk` も `E0302` で、拡張子の誤りとして案内します。
- 隠しディレクトリ（`.ebykn/` など）、`build/`、`target/` の下は見ません。

* 元の仕様：§3.1 は「`proof/` に対応する `src/` が無い `.eykp` は E0301。`proof/` 以外の `.eykp`、`src/` 内の `.eykp` は E0302」とだけ書き、モジュールでない `src/<p>.eyk` と、`proof/` の下の `.eyk` を定めていない。
* 根拠：#65。黙って読まないと、書いた証明が効いていないのに検査が通る。
* 状態：基準で決めた。

### §3.2 `Ebykn.toml` の読み方

- `[package]` の `name`・`version` と、`[verus]` の `release`・`commit` は必須です。`edition` を省くと `2024` です。
- §3.2 にない表と鍵は `E0002` です。知らない鍵を黙って捨てると、書いたつもりの設定が効きません。
- `[verus]` の `release` / `commit` が、`ebykn` をビルドしたときの固定値（このリポジトリのルートの `Cargo.toml` の `[workspace.metadata.verus]`）と違えば `E0002` です。
- `Ebykn.toml` はカレントディレクトリから上へ探します（cargo と同じ）。
- `[dependencies]` は値を保ったまま、鍵の名前の順に `build/verus/Cargo.toml` へ写します。
- `[dependencies]` の鍵に `std`・`core`・`alloc`・`vstd`・`ebykn_prelude`・`verus_builtin`・`verus_builtin_macros`（`-` は `_` と同じ）は使えず、`E0002` です。この名前の依存クレートがあると、`::core::clone::Clone` や `use core::..` がそのクレートを指します（#49。`.eyk` の改名の拒否と同じ名前）。
- 生成する `build/verus/Cargo.toml` の `vstd` は、crates.io の `vstd = "=0.0.0-2026-11-01-0251"`（`cargo verus new` が出す版）です。
- 生成する `build/verus/Cargo.toml` は空の `[workspace]` を持ち、それだけで一つのワークスペースになります。プロジェクトを Cargo のワークスペースの下に置いても、上の `Cargo.toml` のメンバーとはみなされません。
- `ebykn` が走らせた外部のコマンドが失敗したときは、コマンド、走らせたディレクトリ、終了コード（シグナルで止まったときはその番号）、コマンドの出力を示します。

* 元の仕様：§3.2 は例だけで、必須の鍵、知らない鍵の扱い、探し方、vstd の取り先、ワークスペースの下に置いたときの扱いを定めていない。
* 根拠：#17（PR #20、#26）、#6（PR #13）、#78。
* 状態：基準で決めた。

## §4 字句

### 消去フィールドの `@ Ident` は、どの式の後にも書ける

`self@x` のほかに、`b@x`、`old(self)@x`、`final(self)@x` と書けます。`@` の直後が句のキーワード（`requires`、`ensures`、`returns`、`decreases`、`no_panic`、`uses`、`gives`、`by`、`invariant`）なら、消去フィールドではなく、権限 `p@` の後に句が始まると読みます。そのため、これらの名前の `spec` フィールドは `x@name` で読めません。

* 元の仕様：§4 の `@` の用法と §5.3 の `At` は `self @ Ident` だけ。
* 根拠：#14（PR #15、#20、#26）。二つの値の抽象状態を比べる契約や補題（`old(self)@order`、補題の `b@order`）に要る。
* 状態：作者が決めた。

### `uses` は予約語

`uses` は名前（変数、引数、関数、フィールド、パスの部分など）に使えません（`E0001`）。名前に使うなら `r#uses` と書きます。契約の句の `uses` と、呼び出しで領域を渡す `f(x, uses &mut r)`（§5.2）だけがこの語の使い道です。

* 元の仕様：§4 は `uses` を、関数シグネチャの後でだけキーワードになる文脈キーワードとする。
* 根拠：#19。呼び出しの引数の `uses &r` は、`uses` が変数なら `uses & r`（ビット積）と区別できない。
* 状態：判断待ちの既定（#19 の P1 に伴う）。

### Rust の予約語の raw の綴りは、局所の名前に使えない

Rust の予約語を `r#` で名前にしたもの（`r#type`、`r#match`、`r#gen` など）は、局所の名前に使えません（`E0103`）。局所の名前とは、`let`・`match`・`if let`・`for`・関数とクロージャの引数のパターンの束縛（構造体のパターンの省略形 `S { r#type }` と消去フィールドの `@r#type` を含む）、量化子の変数、名前付きの戻り値（`-> (r#type: T)`）、`uses` の領域の名前、const generics の引数です。診断は、書き方ごとに書き換え方を案内します。

- 名前を書いて束縛するところは、`type_` のような別の名前にします。
- 構造体のパターンの省略形（`S { r#type }`、`S { r#type, @g }`）は、`S { r#type: type_ }` のように別の名前で束縛します。`ref` / `mut` の付いた省略形は、`S { r#type: ref type_ }` のように付けたまま書きます。
- 消去フィールドのパターン（`@r#type`）には、別の名前で束縛する書き方がありません。`spec` フィールドなら、パターンでは `@..` で外し、構造体の値から `s@r#type` のように読みます。`owns` フィールドなら、フィールドの名前を変えます（`@..` で外すと、権限を `leak` するのと同じになるため）。
- 対象の語は `crates/ebykn_syntax/src/reserved.rs` の `RAW_KEYWORDS` です。厳格な予約語と将来のための予約語（`abstract`、`yield` など）と、版で予約語になった `async`・`await`・`dyn`・`try`（2018）、`gen`（2024）です。版で予約語になった語は、`Ebykn.toml` の `edition` がその版より前なら拒否しません（2021 の `r#gen` は通ります）。`edition` が数として読めなければ拒否します。
- 予約語でない名前の raw の綴り（`r#foo`、弱いキーワードの `r#union`・`r#raw`、Verus の句の `r#requires`）は通ります。
- 関数・型・モジュール・トレイト・型の別名・構造体のフィールド・型の引数の名前は、raw の綴りのまま通します（Verus が通すので、`fn r#gen` や `s.r#type` はそのまま書けます）。`const`・`static`・列挙子・列挙子のフィールドの raw の名前は、Verus が予約語でない `r#foo` でも拒否します。これは別の問題で、#147 で扱います。
- 中を読まないトークン列（マクロの引数など）の中の束縛は見ません。そこに書いたものは、Verus の `function body check` のままです。
- パターンの名前は字句で判定するので、予約語の raw の綴りの unit struct をパターンに書くと（`pub struct r#type;` の後の `let r#type = x;`）、Rust ではパスのパターンでも `E0103` になります。`r#type {}` か `crate::r#type` と書きます。

* 元の仕様：§4 は識別子を Rust のものとし、raw の綴りに触れない。
* 根拠：#119。固定した Verus は、予約語を `r#` で名前にした局所の名前を SMT の記号に `r#type@` のまま書き、z3 が読めないので、検証条件に出ると `function body check` で失敗する（`spec fn` の引数なら定義が効かず、使う側が `postcondition not satisfied` になる）。`ebykn check` は通るので、`verify` で初めて分かり、文言から原因が分からない（`tests/verus-probes/raw_keyword_local.rs`。2021 の `r#gen` が通ることは `raw_keyword_local_2021.rs`）。書き下しで別の名前に変えない理由は `OPEN.md` の 2026-11-10 の行。
* 状態：基準で決めた。

### `borrow` は式の位置にも書ける

`borrow r[p] { .. }` / `borrow mut r[p] { .. }` は、式が始まる位置ならどこにでも書けます。`if` / `match` と同じブロック式で、値を持ちます。match の腕に書いたときは、続く腕の前に `,` が要ります（構文解析器のフォークが、Ebykn の式をブロック式と見なさないため）。

* 元の仕様：§4 の表は「文の先頭」、§5.4 は文。
* 根拠：#14。値を返す `peek_front` のような関数を素直に書ける。
* 状態：作者が決めた。

### Rust の版の予約語は `r#` を付けて名前にする

`Ebykn.toml` の `edition` で予約語になる名前（今は Rust 2024 の `gen`。`crates/ebykn_syntax/src/reserved.rs` の `EDITION_KEYWORDS`）を `r#` なしで名前に使うと、`E0001` です。診断は、関数・型・モジュール・フィールドの名前なら `r#gen` と書き、局所の名前なら `gen_` などの別の名前を使うよう案内します（局所の名前の `r#gen` は `E0103`。§4「Rust の予約語の raw の綴りは、局所の名前に使えない」）。`r#gen` は書き下しでも `r#gen` のままです。

- 生成する `build/verus/` は `Ebykn.toml` の `edition`（既定は `2024`）でビルドするので、`r#` のない `gen` は生成コードの構文エラーになります。rustc の edition 2024 と同じく、書いた所で拒否します。
- 書いた所とは、識別子の位置すべてです（関数・変数・引数・フィールド・型・モジュールの名前、パス、マクロの引数の中）。トークン列で判定するので、`stringify!(gen)` のように Rust 2024 でも書ける形も拒否します。
- `edition` がその版より前（`2021` など）なら、名前に使えます。数として読めない `edition` は、予約語とみなします。
- 構文解析器（syn）が自分で予約語として拒否する名前（`async`、`await`、`dyn`、`try` など）は、表に入れません。

* 元の仕様：§4 は Ebykn のキーワードを挙げるだけで、Rust の版の予約語を定めていない。
* 根拠：#104。
* 状態：基準で決めた。

### `.eyk` の証明構文

`.eyk` では、次を証明構文として `E0303` で拒否します（`.eykp` では §5.7 の証明構文として読む）。文言は、証明ファイル（`proof/*.eykp`）に書くよう案内します。

- 文の先頭の `assert`（`assert!` は除く）、`proof { }`、`spec let`、`reveal(..)`、`hide(..)`。
- 式のどの位置でも、裸のパスの呼び出し `assert(..)`・`reveal(..)`・`hide(..)`。
- §5.7 の書ける位置にある証明の属性、`lemma fn` の本体と `by(mode)`、`lemma group`、`use lemmas`、`=~=`。

`.eykp` でも書けないものは、`.eyk` でも証明ファイルへ案内せず、`.eykp` と同じ `E0001` にします。案内に従って `.eykp` に移しても通らないからです。

- `reveal_with_fuel(..)`：Verus の書き方として、`reveal(f, n)` を案内します。
- `assume(..)`・`admit(..)`：Ebykn にない Verus の関数として拒否します（文言は「Verus の書き方で、書けません」。クレートの中に同じ名前の関数があれば、パスで呼べると案内します）。
- 書けない位置の証明の属性と引数の崩れた属性、`fn` / `spec fn` の `by(..)`、証明器の名前の誤り、`unchecked lemma fn` と trait の `lemma fn` の本体、モジュールの直下でない `lemma group`、空の `lemma group`、`pub use lemmas`（§5.7「証明構文の位置」）。

`.eyk` が `E0303` でまるごと拒否したコード（`proof { }`、`assert ..`、`lemma fn` の本体など）と、読めなかった項目の中は読まないので、中の証明の属性に「ここには書けない」を重ねません。
- マクロの引数など、Ebykn が中を読まないコードの中の証明構文（`proof { }`、`assert`、`spec let`、`assert(..)`・`reveal(..)`・`hide(..)`）。文言は「Ebykn が読まないコードの中には書けない」です。

パス付きの呼び出し（`m::assume(x)`）は、クレートの中の定義に解決するときだけ利用者の関数として読みます（§5「`.eyk` では Verus の構文を拒否する」。`std::hint::assume(true)` は `E0001`）。メソッドは利用者のメソッドとして読みます。

* 元の仕様：§4 は `assert`・`reveal`・`hide` を `.eykp` でだけキーワードとするので、`.eyk` では識別子になる。
* 根拠：#7（PR #15）、#64。Verus に渡すと証明の構文として読まれ、`.eyk` に証明を書けてしまう（設計ノート §4.7）。
* 状態：基準で決めた。

## §5 文法

### `.eyk` では Verus の構文を拒否する

構文解析器（`vendor/verus_syn` のフォーク）は Verus の構文も受理しますが、`.eyk` ではそれを `E0001` で拒否します。`.eykp` でも同じです。`.eykp` の証明は §5.7 の Ebykn の形（`assert forall x in r, P by { }`、`spec let`、`use lemmas` など）で書き、Verus の形（`forall|x|`、`let ghost`、`broadcast use`、`reveal_with_fuel`）、`proof fn`、`assume` / `admit` は拒否します。診断の文言は、そのファイルの拡張子（`.eyk` / `.eykp`）を名指します。Ebykn での書き方があれば、診断で示します（`forall|x|` → `forall x: T, P`、`proof fn` → `lemma fn`、`&&&` → `and { }` など。一覧は `crates/ebykn_syntax/tests/` の拒否の例）。

- 比較演算子の連鎖（`a < b < c`）も、syn と Rust が受理しないので拒否します。
- 属性は、`#[verifier::..]`（`E0102`）に加えて `#[verus::..]` も拒否します。どちらも `r#`、先頭の `::`、`cfg_attr(条件, ..)` の中まで見ます。
- `self@`・`Self@` は、権限ではなく Verus の view として拒否します。権限は `Ident "@"`、view は `.view()` です。
- `broadcast`・`uninterp`・`assume_specification` は、後ろに Verus の形（`broadcast use / group / proof / fn / axiom`、`uninterp spec`、`assume_specification [` / `<`）が続くときだけ Verus の構文とし、それ以外は識別子として読みます。
- Verus の句・証明・モード変換の組み込み関数（`crates/ebykn_syntax/src/reserved.rs` の `VERUS_BUILTIN_CALLS`）、証明の構文の関数、Verus のマクロ、`Invariant` / `View` の名前は、綴りによらずパスの最後の名前で判定します。裸の名前の呼び出しと、先頭が `::` のパスは `E0001` です。そのほかのパス（`crate::` / `self::` / `super::`、モジュール経由、改名する `use`）は、クレートの中の定義（列挙の列挙子 `St::Tracked` を含む）に解決するときだけ通します。診断は、同じ名前の自分の関数を `crate::..::requires(..)` のパスで呼べることを案内します。裸の名前を値として参照する形（`let r = requires;`、`(requires)(..)`）は、ローカル変数でなければ `E0001` です。中を読まないマクロの引数の中は名前解決しないので、どの綴りの呼び出しも拒否します。値を求めるだけの仕様の関数（`equal`、`imply`、`add` など）と、Ebykn が使う `old` は表に入れません。`crates/ebykn_check/tests/reserved.rs` が、表のすべての名前について綴りを総当たりで試します。
- Verus の ghost の包み `Tracked` / `Ghost`（`reserved.rs` の `VERUS_WRAPPERS`）も、同じ判定で拒否します。見るのは、式（`Tracked(e)`）、型（`Tracked<T>`、`Option<Ghost<T>>`）、パターン（`Tracked(x)`）、`use`、中を読まないマクロの引数です。実行コードの `Tracked(e)` / `Ghost(e)` は、`e` を証明のモードで評価するモードの変換で、`.eyk` に証明を書けてしまいます。ghost の状態は `owns` / `spec` フィールドで、領域の引数は `uses r: Region<..>` で書きます（#99）。

* 元の仕様：§5 は「Rust の文法（syn が受理する範囲）を土台」とし、Verus の構文の扱いを定めていない。
* 根拠：#7（PR #15）、#16（PR #20）、#64（`.eykp`）、#99。設計ノート §3.2「同じ概念を複数の形で書けるようにしない」、§9「`admit` / `assume` は文法上書けない」。黙って受理すると、書き下しで Verus の意味になる（glob の `use vstd::prelude::*;` の下では、本体の `requires(x == 0);` が黙って事前条件になる。`self.nodes = Tracked(Region::empty());` は領域の権限をすべて捨てて verified になった）。
* 状態：基準で決めた。

### マクロ

マクロの扱いは、統一案 #53（needs review）の判断待ちです。#16（マクロの範囲）と #27（cfg と環境変数）の扱いは #53 に委ねました。

作者が示した方針は次の 2 つです。

- なるべく多くを許し、Verus で壊れる形だけを拒否リストで止める。壊れる形とは、検証したものと配るものが食い違う形と、証明が紛れ込む形。穴が見つかるたびに拒否リストを足して塞ぐ（#27）。
- マクロを一律に禁止することはとらない。tokio をはじめ多くのクレートはマクロを前提に作られていて、Rust のクレートの資産を使えるという原則に反する（#53）。

#53 の提案（判断待ち）：

1. 関数の本体の中のマクロは、どのクレートのものも使える。呼べるマクロの許可リストをやめ、壊れる形の拒否リストと、`ebykn verify` の出力の照合（§9、#37）で扱う。拒否リストの候補は、`.eyk` に書いた証明の入口（`verus!`、`proof!` などの名前と `verifier` の属性）、Verus が自分で設定する cfg（`verus_keep_ghost`、`verus_keep_ghost_body`、`verus_only`、`verus_verify_core`、`verus_no_vstd`）と `debug_assertions` を条件に含む `cfg!` / `#[cfg]`、`env!` / `option_env!` / `include!`。
2. Verus が対応していない std のマクロ（`println!` 系、`write!`、`writeln!`、`dbg!`）は、prelude の関数の呼び出しに書き下し、その関数を前提として `ebykn status` に出す。
3. derive・属性マクロを使えるようにし、検証されない部分を前提として一覧に出す。std の derive は前提に出さない（仕様がなく、verified なコードがその振る舞いに頼れない）。関数に付く属性マクロ（`#[tokio::main]` など）は、検証する本体と、マクロを付ける薄い外側に分けて書き下す。
4. 利用者の `macro_rules!` を使えるようにし、モジュール直下で項目を作るものは前提として一覧に出す。
5. 関数の中の一部だけを `unchecked { .. }` ブロックにする逃げ道を足すかは、作者の判断（足すなら別の Issue で構文を詰める）。
6. 依存クレートのマクロの中身は、依存クレートと同じく信頼する前提として扱う（`assume` の類は §9 の照合で捕まえる）。

作者のコメント（#27）は、cfg で分岐するなら組み合わせごとに検証を走らせる方向（組み合わせを CLI で指定する）も示しています。形は決まっていません。

#16 の調査では、関数の本体の中の式・文のマクロ（std のマクロと利用者の `macro_rules!`）は、展開した後も Verus が手で書いたコードと同じく検証します（`println!` 系と `write!` は Verus が拒否する）。derive・属性マクロが `verus!` の外に作る項目は検証されず、それを前提に呼び出し側が verified になりえます（#49）。依存クレートの derive と属性マクロは拒否せず、それが付いた項目を `ebykn verify` の前提の一覧に出します（#53 の提案の 3。下の「[依存クレートの derive と属性マクロ](#依存クレートの-derive-と属性マクロ)」）。

今の develop（#16 の既定の許可リストと、#27 の拒否リスト）：

- `.eyk` で呼べるマクロは固定の一覧だけです（`crates/ebykn_syntax/src/lib.rs` の `ALLOWED_MACROS`）。
  - std の `vec`、`println`、`print`、`eprintln`、`eprint`、`format`、`format_args`、`write`、`writeln`、`panic`、`assert`、`assert_eq`、`assert_ne`、`unreachable`、`unimplemented`、`todo`、`matches`、`concat`、`stringify`、`line`、`column`、`module_path`、`compile_error`、`cfg`。裸の名前か、`std` / `core` / `alloc` から始まるパスで呼べます。
  - vstd の `seq`、`set`、`map`。裸の名前だけで呼べます。
- ほかのマクロの呼び出しは、どこに書いても拒否します。項目・文・式・パターン・型・trait / impl / extern の中、一覧のマクロの引数の中、属性の値（`#[doc = m!()]`）を含み、自作の `macro_rules!` の定義も含みます。診断は一覧と、自作のマクロが要るならそれを使う関数を `unchecked fn` にすれば書けることを示します。
- `verus!` と、Verus・vstd の証明用・内部用のマクロ（`crates/ebykn_syntax/src/reserved.rs` の `VERUS_MACROS`）は、Verus の構文として拒否します。それを取り込む・改名する `use` と、一覧の名前を外のクレートから取り込む・その名前に改名する `use`（`use foo::vec;`）も拒否します。
- 属性は `#[path]`、`#[macro_use]`（`cfg_attr` の中を含む）を拒否し、`#[verus_verify]` / `#[verus_spec]` はパスの最後の名前で拒否します。
- vstd のマクロの引数は Ebykn の式として読みます。std のマクロの引数と、中を読まない Rust の式・型・項目・where 句・境界・属性の値はトークン列のまま持ち、証明と Verus の構文に当たる形（`proof { }`、`assert(`・`reveal(`・`reveal_with_fuel(`・`hide(`・`assume(`・`admit(`・`assume_(`、`forall|`・`exists|`・`choose|`、`let ghost` / `let tracked`、`proof fn` / `exec fn`）と、呼べないマクロの呼び出しを字句的に探して拒否します。`fn` の直後の名前は宣言なので見ません。`&&&` / `|||` は探しません。
- std のマクロの引数の中に権限の操作（Γ・R の名前への `*X`・`X@`・`X.free()`、`self@`、`take` / `put` / `borrow`、`leak X@`、`uses` / `gives` を持つ関数の呼び出し、`Ptr::new` などの権限を作る呼び出し）があれば `E0003` です。引数の中の属性は、`cfg`・`cfg_attr`・lint の属性（`allow`、`warn`、`deny`、`forbid`、`expect`）だけを許し、ほか（`#[inline]`、`#[derive(..)]`、属性マクロ、`#[clippy::..]` などのパスの属性）は `E0003` です。引数はトークンのまま持つので、derive や属性マクロが作るコードを見られないためです。許す属性も、書ける位置を引数のトークンで決めます。Ebykn がトークンから見分けられる、安定版の Rust が属性を許す位置は、`vec!` の要素と繰り返しの数の頭と、`{ .. }` の中の文・match の腕・構造体の欄の頭（先頭、`;`・`,`・`{ .. }` の後）の外側の属性だけで、ここだけに書けます。安定版の Rust はほかにも呼び出しやメソッドの引数などで属性を許しますが、トークンでは見分けないので `E0003` です。安定版の Rust が許さない位置（`assert!(#[allow(unused)] (a > 0))`、`assert_eq!(#[cfg(..)] a, b)`、`let x = #[..] a;` の初期値など）と、内側の属性 `#![..]`（`{ #![..] .. }` はブロック式の属性になる）を通すと、Verus の toolchain では通るので、verified なのに Verus なしのビルドが E0658 で落ちるためです（PR #59 の再確認）。位置は引数のトークンで決めるので、許す位置の式が代入・二項演算・型変換・範囲で始まるとき（`vec![#[allow(unused)] a + 1]`、`assert!({ #[allow(unused)] 0..a; true })`）は見分けられず、通ります（#130）。
- 拒否リスト（`crates/ebykn_syntax/src/denylist.rs` の `DENYLIST`、#27）：検証したコードと配るコードを食い違わせる形を、一つの表で `E0001` にします。表の各項目は、壊す理由（診断の文言）と再現の例を持ち、`crates/ebykn_check/tests/denylist.rs` が、表のすべての項目について、再現の例と書ける位置の総当たりが拒否されることを確かめます。
  - cfg：`verus_keep_ghost`、`verus_keep_ghost_body`、`verus_only`、`verus_verify_core`、`verus_no_vstd`（Verus が検証するときに自分で設定する。#16 の調査）、`debug_assertions`（検証の dev では有効、release のビルドでは無効）、`panic`（パニックの戦略は根になるクレートのプロファイルで決まり、`build/verus/Cargo.toml` の `panic = "abort"` は、そのクレートが根のときだけ効く）を条件に含む `cfg!(..)`（`std::cfg!` / `core::cfg!`）、`#[cfg(..)]` / `#![cfg(..)]`、`#[cfg_attr(条件, ..)]` を拒否します。`cfg_attr` は、中の `cfg(..)` に加えて条件そのものも見ます（`#[cfg_attr(verus_keep_ghost, derive(..))]` は Verus のときだけ impl を作る）。述語は `all` / `any` / `not` の入れ子の中まで識別子をすべて見ます（`feature = "x"` の値は文字列なので見ない。`r#` は外す）。トークン列で見るので、項目・本体・マクロの引数・属性の値・`unchecked fn` の本体のどこでも効きます（cfg には `unchecked fn` の例外を作らない）。
  - マクロ：`env!` / `option_env!`（`ebykn verify` は Verus に `CARGO_TERM_COLOR=never` を渡し、`RUSTUP_TOOLCHAIN` は Verus の toolchain になる）、`file!`（検証では `src/lib.rs`、別のクレートから path で依存されると絶対パス）、`include!`（Ebykn が読まないコードを取り込む。#16 の調査の `assume(false)` の持ち込み）、`include_bytes!` / `include_str!`（読むファイルが Verus の実行とビルドで違いうる。`/proc/self/cmdline`、`/proc/self/environ` など）、`debug_assert!` / `debug_assert_eq!` / `debug_assert_ne!`（std の展開の中で `cfg(debug_assertions)` に届き、引数が検証では実行され、release のビルドでは実行されない）。裸の名前か `std::` / `core::` から始まるパスで、どこに書いても（本体・`const`・`static`・属性の値・マクロの引数の中も）拒否します。`include!` のほかは、`unchecked fn` の本体の中でだけ許します（本体は検査せず、`verify` が証明なしの前提として一覧に出す。入れ子の `fn` は自分の `unchecked` で決め直す）。
  - `line!` / `column!` / `module_path!` は許します（生成コードの位置と、生成するモジュールのパス。Verus の実行と素のビルドで同じ値になる）。
  - 拒否リストにない cfg（`test`、`feature`、`target_os` などの target 系、利用者が `--cfg` で付ける名前）は、関数の中・マクロの引数の中を含めて許します。ただし、Verus の検証と同じ設定（ホストの target、既定の feature、追加の `--cfg` なし、`test` なし）でビルドしたときだけ食い違いません。別の target・別の feature・`RUSTFLAGS` の `--cfg`・`cargo test` でのビルドは、検証したのと別のコードになります。
  - そのため、クレートがこうした cfg に頼っていれば（項目・本体・マクロの引数・`cfg_attr` の条件のどこでも。`all()` / `any()` と `true` / `false` だけの述語は除く。`r#true` は名前として数える）、`ebykn verify` は前提の一覧に必ず 1 行出します。テキストでは `assumed cfg 名前, ..` の行、`--json` では `cfg_assumption`（`names` と `note`）、標準エラーには英日の文言で、「verified は、ホストの target・既定の feature・追加の `--cfg` なし・`test` なしで検証した結果。ほかの組み合わせのビルドと `cargo test` は検証していない」ことと、頼っている名前を出します（`tests/golden/cfg_allowed/`）。前提なので、verify の成否は変えません。
  - これは #87（needs review、P0）の判断が出るまでの既定で、#87 の案 B の前半に当たります。`build` が違う組み合わせを拒否する部分（案 B の後半）は、#87 の判断の後にします。
- 文と式の cfg：§6 の検査は cfg を評価しないので、次のものに付いた `#[cfg]`（`cfg_attr` の中の `cfg` を含む。`r#cfg` / `r#cfg_attr` の綴りも同じ）は `E0003` です（PR #59 のレビュー）。
  - 権限の状態（Γ）を変える文・式・構造体リテラルの欄（`let n = q.free();`、権限を消費する呼び出し、`Ptr::new` で権限を作る `let`、配列とタプルの要素・呼び出しとメソッドの引数・欄の値の `q.free()` など）。消えると、§6 が見ないまま権限を捨てる・作る。前後で Γ を比べて決める。
  - 発散する文・式・欄（`return`、`break`、`continue`。欄の値の `return` も）。消えると、§6 が到達しないと見た後ろのコードが実行される。
- ジェネリクスの引数とパターンの cfg（#95）：Ebykn が中を読む項目（`fn`、`struct`、`enum`、`trait`、`impl`、`type`、`const`。impl の中の `type` を含む）のジェネリクスの引数に付けた `#[cfg(..)]`（`fn f<#[cfg(x)] T>`。項目の中でも本体の中でも）と、パターン（クロージャの引数、構造体パターンの欄と `@x`。`fn` の引数のパターンの中の欄も）に付けた `#[cfg(..)]` は `E0003` です（`fn` の引数そのものの `#[cfg]` は、下の「ほかの位置」のとおり書けます）。検査は cfg を評価せずに引数の並びを読み、パターンの名前（`@x` なら権限）を束縛するためです。`union` や trait の中の `type` / `const` のように、中を読まずに Rust のまま渡す項目のジェネリクスの引数は通します（rustc が `verify` と素のビルドで同じに評価する）。cfg でない属性は書き下しに残します。構造体パターンの `..` と `@..` の前には、rustc と同じく属性を書けません（`E0001`）。書き下しは `..` に属性を付けられないためです。トークンの検査が拒否リストの `E0001` にする属性には、`E0003` を重ねません。`E0001` になるのは、`cfg(述語)` の述語か、`cfg_attr(条件, ..)` の条件か並びの中の `cfg(..)` / `cfg_attr(..)` に拒否リストの名前がある属性です（判定は `crates/ebykn_syntax/src/denylist.rs` の `for_each_cfg_pred` の一つで、トークンの検査と `attr_denied` が共有します）。lint の名前（`#[cfg_attr(all(), allow(verus_keep_ghost))]`）のように述語でない所に名前を書いた属性は `E0001` にならないので、同じ並びの `#[cfg]` は `E0003` です。重ねないかどうかは属性ごとに決め、隣の属性では決めません。
- 代入・二項演算・型変換の文（`x = e`、`x += e`、`*p = v`、`(*p).f = v`、`a == 0;`、`a as u64;`）には、cfg に限らずどの属性も付けられません（`E0003`）。構文解析器が文の属性を左の項に移す形で、安定版の Rust では書けません（`attributes on expressions are experimental`。Verus の toolchain では通るが、Verus なしのビルドで落ちる）。cfg で消える代入は、Verus も「removing an expression is not supported」で拒否します。
- ほかの式の属性も、安定版の Rust が許す位置だけに書けます。許すのは、代入・二項演算・型変換でない文の先頭、ブロックの末尾、配列とタプルの要素、呼び出しとメソッドの引数です。ほかの位置（`let x = #[allow(unused)] a;` の初期値、`if` の条件、`return` の値、`match` の対象と腕の本体、括弧の中、構造体リテラルの欄の値、クロージャの本体など）の式の属性は、cfg に限らず `E0003` です。安定版の Rust では `attributes on expressions are experimental`（E0658）になり、Verus の toolchain では通るので、verified なのに Verus なしのビルドが落ちるためです（PR #59 の再確認）。論理文脈（契約・`spec fn` の本体など）の式は Verus なしのビルドで消えるので対象外です。構造体リテラルの欄そのもの（`S { #[cfg(..)] b: 2 }`）と match の腕に付ける属性は、式の属性ではないので書けます（`crates/ebykn_check/tests/cfg_stmt.rs`）。
- `take` / `put` / `leak` / 仕様の代入（`x@f = e`）には、どの属性も付けられません（属性を書き下さない）。仕様の代入の属性は、取り込んだ `verus_syn` が捨てていたので、トークンに戻して読みます（`vendor/verus_syn/src/stmt.rs` の `EBYKN:`。`verus_syn` の `printing` の feature の下にある）。
- ほかの位置（`let`、項目、構造体の欄の定義、match の腕、引数の定義、呼び出し・ブロック・`if`・`while` の文、マクロの引数の中）の属性と、権限を操作も発散もしない式・欄の cfg は、書き下しにそのまま残します。`crates/ebykn_lower/tests/cfg_attrs.rs` が位置ごとに確かめ、`tests/golden/cfg_allowed/` が Verus で検証します。
- 残る抜け道（#16）：外のクレートが vstd の項目を再公開する経路（`use foo::admit;`）、外のクレートからの glob の `use`（`use foo::*;`）。外のクレートの手続きマクロ（`#[derive(..)]`、属性のマクロ）は、項目に付けたものを前提の一覧に出し、ほかの位置のものを拒否します（下の「依存クレートの derive と属性マクロ」）。項目に付けた手続きマクロが健全な証明（`proof!{ reveal(..) }` など）を持ち込む経路は残ります（#16 の判断の案 X / 案 Y）。

* 元の仕様：マクロの扱いを定めていない。§5 は Rust の文法を土台とする。
* 根拠：#16（PR #15、#20、#26）、#27（PR #26、PR #59、`tests/golden/denylist/`、`tests/golden/cfg_allowed/`）、#53、#49、#87。`assume_specification .. ensures false` や `admit()` が一つ紛れ込むだけで何でも verified になる（P0）。
* 状態：判断待ちの既定（#53、#87）。マクロは上の許可リスト、cfg とビルドで値の変わるマクロは上の拒否リスト（作者が決めた方針。#27）。derive と属性マクロは下の「依存クレートの derive と属性マクロ」（#49）。

### 依存クレートの derive と属性マクロ

依存クレートの derive と属性マクロは、項目（`fn`、`struct`、`enum`、`union`、`trait`、`impl`、`mod`、`const`、`static`、`type`、`use`、`extern crate`、`extern` ブロックの中の宣言と、`impl`・`trait` の中の項目、関数の本体の中の項目）に付けて書けます。マクロが作る項目は `verus!` の外になり、Verus が検証しないので、`ebykn verify` は付いている項目を前提の一覧に必ず出します（下の「`ebykn verify` の項目の状態と出力」の `macro`）。

- 判定は字句的に、どの項目にどの属性が付いているかで行います（`crates/ebykn_syntax/src/macros.rs` の表）。`cfg_attr(..)` と `unsafe(..)` の中も見ます。
- 出さないもの（std と Rust のもの）：
  - std の derive：`Clone`、`Copy`、`Debug`、`PartialEq`、`Eq`、`Hash`、`Default`、`PartialOrd`、`Ord` を、裸の名前か `std::` / `core::` から始まるパスで書いたもの。`std::..::derive` / `core::..::derive` のパスの derive（`core::prelude::v1::derive`）の中も同じです。std の derive が作るのは std の書いた impl だけです。固定した Verus は derive した `Clone` の impl だけを検証し、ほかの impl は検証しません。それでも、どの impl にも仕様がないので、verified なコードはその振る舞い（`a == b` の結果、`S::default()` の値など）に頼れません。
  - Rust の組み込みの属性：Rust リファレンスの Built-in attributes index のうち、Conditional compilation（`cfg`、`cfg_attr`）、Testing（`test`、`ignore`、`should_panic`）、Derive（`derive`、`automatically_derived`）、Macros（`macro_export`、`macro_use`、`proc_macro`、`proc_macro_derive`、`proc_macro_attribute`）、Diagnostics（`allow`、`expect`、`warn`、`deny`、`forbid`、`deprecated`、`must_use`）、ABI, linking, symbols, and FFI（`link`、`link_name`、`link_ordinal`、`no_link`、`repr`、`crate_type`、`no_main`、`export_name`、`link_section`、`no_mangle`、`used`、`crate_name`）、Code generation（`inline`、`cold`、`naked`、`no_builtins`、`target_feature`、`track_caller`、`instruction_set`）、Documentation（`doc`）、Preludes（`no_std`、`no_implicit_prelude`）、Modules（`path`）、Limits（`recursion_limit`、`type_length_limit`）、Runtime（`panic_handler`、`global_allocator`、`windows_subsystem`）、Type system（`non_exhaustive`）、Debugger（`debugger_visualizer`、`collapse_debuginfo`）。Features（`feature`）は入れません（下の位置の規則）。
  - ツールの属性：Rust リファレンスの Tool attributes が挙げる `clippy`、`rustfmt`、`diagnostic`、`miri`、`rust_analyzer` の名前空間（`diagnostic::on_unimplemented` を含む）。
- 名前を覆い隠す書き方は、std や Rust のものとして数えません。クレートのどこかでその名前を束縛していれば（`use` の葉と改名、`mod`、`extern crate`、依存クレートがコードで使われる名前）、前提として出します。
  - `use evil::Clone;` の後の `#[derive(Clone)]`、`use evil::x as derive;`（`test`、`global_allocator` も）の後の `#[derive(..)]`。
  - `clippy` などと同じ名前の依存クレートがあるときの `#[clippy::x]`。依存クレートの名前は、改名（`package = ..`）なら鍵、そうでなければそのクレートの `[lib] name`（`[lib] name = "clippy"` も）です。
  - `core` という名前を束縛しているときの `#[derive(::core::clone::Clone)]`（先頭の `::` があっても）と、`use core::clone::Clone;` の後の `#[derive(Clone)]`。
  - `inline`・`allow` などの組み込みの属性は、同じ名前を取り込むと rustc が曖昧さのエラー（E0659）にするので、数えてよいものです。glob の `use` で std の derive や `derive` / `test` を取り込んでも曖昧さのエラーになります。
- 依存クレートがコードで使われる名前は、書き下しでは `Ebykn.toml` と path の依存の `Cargo.toml` から決めます。registry や git の依存、読めない `Cargo.toml` があって決められなければ、std のパス（`std::..` / `core::..`）、std から取り込んだ名前、ツールの属性を数えず、前提として出します。`verify` は Verus の実行の後に `cargo metadata --offline` で名前を確定し、一覧を決め直します。`cargo metadata` が失敗すれば、決められないときの扱いのままです。
- 項目の外側の属性のほかの位置には、決まった属性だけを書けます。ほかの属性は `E0001` です。tokio や serde はこれらの位置を使わないので、#53 の方針に反しません。
  - 内側の属性 `#![..]`（ファイルの先頭、`mod` の中）：上の Rust の組み込みの属性のうち、名前を覆い隠せないもの（`test`・`derive`・`global_allocator` を除く）だけです。依存クレートの属性マクロ、ツールの属性、`#![feature(..)]` は書けません。安定版の Rust は内側の属性マクロを許しませんが、Verus は `-Zcrate-attr=feature(custom_inner_attributes, stmt_expr_attributes)` と `RUSTC_BOOTSTRAP=1` で動くので、`verify` では内側の属性マクロが走ります（`mod m { #![evil::x] }` が一覧にない `impl` を足せる）。`#![feature(..)]` を拒否するのは、`#![feature(proc_macro_hygiene)]` で文・式の属性マクロも走るようになるためです。
  - 文・式・`let`・match の腕の外側の属性：内側と同じ組み込みの属性と、ツールの属性（`#[rustfmt::skip] let ..`）です。ここに付く属性マクロは、Verus でも E0658（`proc_macro_hygiene` は有効でない）で大きく失敗します。`E0001` は、`#![feature]` の拒否の後ろの二重の守りです。
- `#[override]` は `override` が予約語なので構文解析が読めず（`E0001`）、`#[r#override]` は利用者が束縛した名前として前提に出します。
- raw の綴り：予約語の raw の綴り（`r#unsafe`、`r#override`）は予約語とは別の名前で、利用者が束縛した名前として扱います（`#[r#unsafe(..)]` は組み込みの `unsafe(..)` の包みではない）。予約語でない名前の raw の綴り（`r#derive`、`r#cfg`、`r#repr`、`r#test`）は、素の名前と同じです（rustc と同じ）。組み込みの属性の名前を比べる検査（パターンとジェネリクスの引数の `#[cfg]` の `E0003`、権限を操作するか発散する文・式の `#[cfg]` の `E0003`、拒否リストの cfg の `E0001`、`#[repr]`、`#[test]` の `external` など）も、すべて raw の綴りを素の名前と同じに比べます（#122）。比べる関数は `Ident::is_name`・`Attr::is_builtin`・`macros::token_is_name` の 3 つで、ゲート `check-attr-names` は、3 つを通さない比較のうち 2 つの形を拒否します。`.is("名前")` と、1 行の中でトークンの識別子（同じ行の `Ident(x)` で束縛した `x` か、`.to_string()` で終わる式）と `"名前"` を `==` / `!=` で比べる形です（#132）。名前を変数で渡す `.is(name)`、名前を変数に入れてから比べる形、`matches!`・`match` の腕・`.eq(..)` での比較、括弧で包んだ `(i.to_string())`、前の行で束縛した `Ident(i)` の比較、改行をまたぐ `==` は捕まえません。これらの形は、raw の綴りのテストが落ちることで見つけます。

* 元の仕様：マクロの扱いを定めていない。
* 根拠：#49（PR #57）、#53。`tests/golden/external_macros/`（属性マクロを付けた `ensures r == 1` の関数が verified と出ても、Verus なしのビルドでは 0 を返す。前提の一覧に `macro` の行が出る）、`crates/ebykn_syntax/tests/macros.rs`（綴りの総当たり）、`crates/ebykn_check/tests/attr_positions.rs`（位置の拒否）、プローブ `external_impl_of_spec_trait.rs`・`std_derive_no_spec.rs`・`automatically_derived_body.rs`（OPEN.md の「M0 の実測」）。
* 状態：判断待ちの既定（#16 の案 X / 案 Y）。案 Y（依存クレートのマクロを `Ebykn.toml` で明示して許可する）になれば、許可したマクロだけを書ける形に変わり、`status` に前提として出します。前提に検証されない項目がある関数も `verified` と出します。依存先の前提の伝播は M5 の `status` で扱います。

### §5.2 名前付きの消去される引数は、呼び出しの `uses` で渡す

名前付きの消去される引数（`uses r: Region<..>` / `uses r: &Region<..>` / `uses r: &mut Region<..>`）を持つ関数に、呼び出しの引数の末尾の `uses` の後に領域を `,` で並べて渡します（#19 の既定 P1）。

```
CallArgs   ::= ( Expr ( "," Expr )* "," )? "uses" RegionArg ( "," RegionArg )* ","?
            |  Expr ( "," Expr )* ","?                         (* Rust の引数 *)
RegionArg  ::= ( "&" "mut"? )? RegionExpr                      (* RegionExpr は §5.4 *)
```

```
let head = build_chain(n, uses &mut r);                          // 構造体を作る前の補助関数
1 + max(depth(l, uses &r), depth(q, uses &r))                    // 再帰
let t = merge(x, y, uses &mut ra, &mut rb);                      // 二つの領域を行き来する
unlink(h, uses &mut self@nodes);                                 // 同じ補助関数を使い回す
into_graph(a, b, uses g)                                         // 移動
```

- 呼び出しと、メソッドの呼び出し（`self.f(x, uses &r)`）に書けます。`uses` の後はすべて領域です。
- 呼び出し側に、`&r`（共有）/ `&mut r`（可変）/ `r`（移動）を書きます。呼ばれる側の宣言の `&Region<..>` / `&mut Region<..>` / `Region<..>` と合わなければ `E0518` です。領域の数が宣言と違うとき（領域を受け取る関数を `uses` なしで呼ぶ、受け取らない関数に渡す）も `E0518` です。領域は書いた順に、宣言の `uses` の領域の順に対応します。
- 渡せるのは §5.4 の `RegionExpr`（領域変数か `self@x`）だけです。それ以外の名前は `E0001`、`spec` フィールドは `E0513` です。
- 移動で渡した領域変数は、その後で使えません（`E0519`）。借用は呼び出しの間だけで、状態を変えません。`uses` で借りて受けた領域と `self@x` は移動で渡せず、`&` で受けた領域は `&mut` で渡せません。同じ呼び出しで同じ領域を二度渡し、一方が `&mut` か移動なら `E0519` です。読み方の細部は `OPEN.md` の「§6.2 の読み方」にあります。
- 呼び出しの引数の末尾の外（タプル、配列、`let` の初期値、文）に書いた `uses` は `E0001` です。中を読まないマクロの引数の中の `uses` は `E0003` です（権限の操作と同じ）。
- 権限 `uses p@` の渡し方（`f(p)`。権限は実行時の引数 `p` に付いて渡る）は変えません。

書き下し（§7.3）：`Tracked(..)` を、呼ばれる側の `uses` の宣言の順（権限と領域を合わせた順）で引数の末尾に足します。

| 呼び出し側で書くもの | 書き下し（Verus） |
| --- | --- |
| `&mut r` / `&r`（局所の領域、値で受けた領域） | `Tracked(&mut r)` / `Tracked(&r)` |
| `&mut r` / `&r`（`r` が `uses r: &mut Region<..>` / `&Region<..>` で受けた参照） | `Tracked(&mut *r)` / `Tracked(&*r)` |
| `&mut self@nodes` / `&self@nodes` | `Tracked(self.__eyk_erased_nodes.borrow_mut())` / `Tracked(self.__eyk_erased_nodes.borrow())` |
| `r`（移動） | `Tracked(r)` |

呼ばれる側を引けない呼び出し（クロージャ、外のクレートの関数など）には、書いた領域を書いた順に足します。受け取らなければ Verus が型の誤りとして拒みます。

例は `tests/golden/region_call/`（#19 のコメントの用途 1〜4。構造体を作る前の補助関数、再帰、一つの領域の共有、補助関数の使い回し）です。

* 元の仕様：宣言の書き下し（§7.3）だけがあり、呼び出し側の書き方がない。
* 根拠：#19（PR #20）。構造体を作る前の補助関数、二つの領域を行き来する再帰、領域の共有、補助関数の使い回しに要る。
* 状態：専用の構文を作ることは作者が決めた。構文の形は判断待ちの既定（#19）。案は P1（括弧の中の末尾に `uses`。既定）、P2（`@` を付けて括弧の中の末尾。`build_chain(n, @&mut r)`）、P3（括弧の後ろに `@( )`。`build_chain(n)@(&mut r)`）の 3 つ。呼び出し側に `&` / `&mut` を書くかも判断待ちで、既定は「書く」。違う案に決まっても、作り直すのは呼び出しの構文の読み取り（`crates/ebykn_syntax` の差し込み口）だけで、AST の `RegionArg` から先の検査と書き下しは変わらない。

### §5.4 消去フィールドは `@` でだけ書く

`owns` / `spec` のフィールドは `x@f` でだけ読み書きします。`.` の形は `E0404` です。

- 拒否する形は、フィールドの読み書き `e.f`、構造体リテラルの `f: e`、パターンの `f` / `f: p`（分解代入の左辺を含む）です。受け手によりません（`self`、ほかの値、`*p`、`old(self)` / `final(self)`）。実行コードでも論理文脈でも同じです。
- `.` の前の値の型が分かり、それがクレートの構造体で、`f` がその通常のフィールドなら通します。消去フィールドなら拒否します。
- 型が分からないものは、`f` がクレートのどれかの構造体の消去フィールドと同じ名前で、クレートのどの構造体も `f` を通常のフィールドに持たないときだけ拒否します。診断は、別の型の値なら型を書く（`let x: T = ..`）よう案内します。通常のフィールドに `f` を持つ構造体があれば、その値かもしれないので通します。消去フィールドに届く形なら、書き下しの名前（§7.3「消去フィールドの名前」）のため Verus のビルドで止まります。
- 型が分かるとするのは、`self`、型を書いた引数と変数、`*e`（参照と `Ptr` / `Cell` の指す値）、`old(e)` / `final(e)`、型の引数のない構造体のフィールド `e.g` です。次の型は、分からないものとして扱います。
  - `f` を自分のフィールドに持たないクレートの構造体。`Deref` でほかの型に届きうるからです。
  - 型の引数（`T`、`T::Target`、`<T as Tr>::A`）と、クレートの構造体でない型の `impl` や trait の既定のメソッドの `self`。
  - 型の引数か型の別名の先に、`f` を消去フィールドに持つ構造体か型の引数を書いた型（`Box<S>`、`type A = S;` の `A`、`Box<T>`）。
  - `_` と `dyn Trait` など。
- 中を読まないマクロ（`println!` など）の引数の `.f` も、同じ規則で判定します（メソッドの呼び出し `.f(..)` は除く）。受け手の型は、`.` の直前が裸の名前（`self` か、型を書いた変数）のときだけ見ます。それ以外は型が分からないものとして扱います。

* 元の仕様：§5.4 は `self@x` と `@name` の形を定めるが、`.` で書いたものの扱いを定めていない。`owns` フィールドへの代入の `E0516` は `self@x = e;` の形だけ。
* 根拠：#99。`self.nodes = Tracked(Region::empty());` が、書き下しの `Tracked` のフィールドに届いて領域の権限をすべて捨て、verified になった（P0）。
* 状態：基準で決めた。

### §5.4 match の腕の本体に直接書いたループの句

ループの句（`invariant`、`decreases`）は文法どおり、順序も繰り返しも自由です。ただし match の腕の本体に直接書いたループは、`invariant` → `decreases` の順に一回ずつしか書けません。

* 元の仕様：`LoopClause*`（順序も回数も自由）。
* 根拠：#7（PR #15）。match の腕の本体はフォーク（Verus の規則）がループを先に読む。
* 状態：基準で決めた。

### §5.7 証明構文の位置

- 証明の文（`proof { }`、`assert`、`spec let`、`reveal`、`hide`）は文の位置にだけ書けます。式の位置（`f(proof { .. })`）に書くと `E0001` です。マクロの引数の中（`vec![{ proof { } 1 }]`）は Ebykn が読まないので、文の形でも `E0001` です。証明の文には属性を付けられません（`E0001`）。
- `ProofAttr` は、次の位置にだけ書けます。ほかの位置（ファイルや `mod` の先頭の内部属性、文・構造体・フィールドの属性、`cfg_attr` の中、マクロの引数の中など）は `E0001` です。
  - `#[trigger]`：式に付ける。
  - `#![trigger e, ..]` / `#![auto]`：量化子の束縛子の後の `,` と本体のあいだに書く（`forall x: T, #![trigger f(x)] P`、`assert forall x: T, #![auto] P by { }`）。`#![trigger]` の引数は Ebykn の式として読み（`self@nodes[p]` などを書き下すため）、一つ以上を要します。
  - `#[opaque]`・`#[memoize]`：`spec fn` に付ける。
  - `#[rlimit(n)]`・`#[spinoff_prover]`：`fn` と `lemma fn` に付ける（Verus が検証する関数。`unchecked fn` / `unchecked lemma fn` には付けられない）。
  - 項目の属性は、関数の本体の中の項目（`fn f() { #[opaque] spec fn g() -> int = 1; }`）にも同じ規則で付けられる。
- 引数の形：`#[rlimit(n)]` の `n` は整数のリテラル、ほかの外部属性は引数を持ちません。崩れた形（`#[rlimit(x)]`、`#[opaque(1)]`、`#[trigger(x)]`）は `E0001` で、正しい形を示します。
- `lemma group` はモジュールの直下にだけ書けます（Verus の `broadcast group` と同じ）。ブロックや `impl` の中では `E0001` です。補題を一つ以上要します。
- `spec let` は初期値（`= e`）を要し、`let-else` の形は書けません（§5.7 の `"spec" "let" Pat "=" Expr ";"`）。
- `by(mode)` は一つの `lemma fn` に一つだけです。

* 元の仕様：§5.7 は文法だけで、`ProofAttr` の位置と引数、`lemma group` を置ける場所、式の位置の証明構文を定めていない。§5.1 の `Item` は `LemmaGroup` をどの項目の位置にも置ける。
* 根拠：#64（PR #86 のレビュー）。`ProofAttr` の位置は Verus の属性（`verifier::opaque` など）が付く項目に合わせた。式の位置の証明構文は、Verus に渡すと式として読まれ、`.eyk` の剥がし（§6.1）と書き下しの両方で扱いが一つに決まらない。`#![trigger]` の引数をトークン列のまま渡すと、Ebykn の式（`x@f`、領域添字）が書き下されない。
* 状態：基準で決めた。

## §6 静的検査

### §6.1 剥がし一致検査で剥がすものと比べ方

剥がすもの：

- 証明の文（§5.7 の `ProofStmt`）と、ブロックの中の `use lemmas`。
- `lemma fn` の `by(mode)` と、証明の属性（`ProofAttr`）。
- `.eyk` に宣言のある `lemma fn` の本体。本体を宣言の `;` に置き換えて比べます。
- `.eykp` だけの項目：本体付きの `lemma fn` と `spec fn`（項目ごと）、`lemma group`、`use lemmas`。

`.eykp` だけの項目として剥がすのは、モジュールの直下（インラインの `mod` を含む）と `impl` の中の項目だけです。`.eyk` の同じ場所（同じモジュール、`impl` なら同じ見出し）に同じ名前の `lemma fn` / `spec fn` があれば宣言があるとみなし、比べます。関数の本体の中と trait の中の項目は剥がさずに比べます。本体のない `lemma fn`、`unchecked lemma fn`、そのほかの項目（`fn`、`use` など）を `.eykp` にだけ書くと `E0304` です。証明のない前提を証明ファイルで足せないようにするためです。

`.eykp` だけの項目（剥がす `spec fn`、本体付きの `lemma fn`、`lemma group`）は、クレートのどれかの `.eyk` に識別子として現れる名前（マクロと属性の引数の中も含む）と、Ebykn がソースのために使う名前（`invariant`、`view`、`ebykn_prelude` が公開する名前）を持てません（`E0304`）。同じファイルのほかの `.eykp` だけの項目と同じ名前も `E0304` です。その場の項目は glob の import（暗黙の `use ebykn_prelude::*;` を含む）を覆い、`impl T` の `spec fn` は同じ名前のトレイトのメソッドより先に選ばれるので、`.eykp` だけの項目が `.eyk` の契約とコードの指すものを変えうるためです。`.eyk` に現れない名前なら、`.eyk` の意味は変わりません。

比べ方：

- ループの `invariant` / `decreases` は、句ではなく式を単位に、`.eyk` 側の式の列が `.eykp` 側の式の列の部分列（順序を保ち、句の種類と正規化したトークン列が等しい）であることを確かめます。同じ種類の句は連結して読む（§5.2）ので、`invariant a, b` と `invariant a invariant b` は同じです。
- 正規化では、doc コメント（`///`、`//!`、`/** */`）もコメントとして無視します。
- 空白は、トークンの区切りを変えない範囲で無視します。続けて書いた記号は、rustc と同じく複数文字の演算子（`&&`、`||`、`::`、`->`、`..=`、`==>` など）にまとめてから比べます。`a && b` は短絡し、`a & &b` は両方を評価するので、別のものとして比べます。安全側に倒すので、rustc が同じに読むものでも区切りが違えば `E0304` になります（`Vec<Vec<u64>>` と `Vec<Vec<u64> >`、`&&u64` と `& &u64`、`|| 1` と `| | 1`）。診断は食い違ったトークンを示すので、`.eyk` と同じ書き方に直します。
- 属性の順序は、続けて書いた属性の並びがすべて Rust の組み込みの属性（`ebykn_syntax::macros::BUILTIN_ATTRS`）のときだけ無視します。属性マクロを含む並びは、展開の順が意味を持つので、書いた順に比べます（判断待ちの既定。#144）。
- 末尾のカンマは、項目と波括弧の階層では閉じ括弧・`;`・`{`・契約とループの句のキーワードの前で、角括弧の中では閉じ括弧の前で無視します。丸括弧の中のカンマは、書いたとおりに比べます。`(a,)` は 1 要素のタプルで `(a)` とは別のものなので、呼び出しと引数の並びの末尾のカンマ（`f(a,)`、`fn g(x: u64,)`）とトークンだけでは見分けられません。安全側に倒し、`f(a,)` と `f(a)` も `E0304` にします。`(S, {})` と `(S {})`、`[S, {}]` と `[S {}]` も区別します。
- マクロの引数と属性の引数の中は正規化しません。中の意味を Ebykn が知らないためです。
- 一つの組から出すのは、最初の `E0304` か `E0305` の一つだけです。どちらかのファイルに構文の診断があれば、その組は比べません。

`.eykp` の本体に §6.2〜§6.5 の検査（線形性など）はまだかけません（#67）。`.eykp` の中のパスのうち、Verus の関数の名前を最後に持つもの（§5「`.eyk` では Verus の構文を拒否する」）は、対応するモジュールの中の名前として判定します。

* 元の仕様：§6.1 は「§5.7 の証明構文を取り除く」とだけ書き、§5.7 の `ProofItem` の剥がし方、ループの句の部分列の単位、正規化の細部を定めていない。
* 根拠：#65（PR #136 のレビュー）。設計ノート §3.2「同じ概念を複数の形で書けるようにしない」。`.eykp` に本体のない補題を足せると、証明なしの前提が増える（§9 の `admit` / `assume` は文法上書けない）。
* 状態：基準で決めた。属性の順序の範囲だけは判断待ちの既定（#144）。

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
- 参照モデル（`formal/linearity`）もこの規則で検査し、健全性の証明と判定の表（`formal/oracle/linearity.tsv`）はこの規則に従います。
- `panic = abort`（§1.3）なので、巻き戻しの経路はありません。

* 元の仕様：この場合を書いていない。文言どおりに読むと `loop { borrow mut r[p] { break; } }` を受理し、ループの後に借用中の `p` が残る（健全でない）。`borrow mut` を `return` で抜けると、権限が領域に戻らない。
* 根拠：#21（PR #20、#22、#51、#60）。
* 状態：作者が決めた（案 D）。

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
- 関数単位の `by(m)`：`.eykp` が入る M3 からの規則です。書き下しは署名の直後（`requires` の前）に出します。`ensures` の後に置くと Verus の構文エラーです。`.eyk` では、`by(m)` は証明の構文として拒否します（`E0303`）。
- 利用者の `impl Drop`：std の `Drop` の impl の `fn drop` に、書き下しが `opens_invariants none` と `no_unwind` を足します。Verus はこの 2 つのない `Drop` の実装を拒否し、利用者は `.eyk` にどちらも書けないためです。std の `Drop` は、裸の `Drop`、`std::ops::Drop` / `core::ops::Drop`、`std::prelude::<版>::Drop` と、`use` の別名・glob の `use`・`pub use` の再公開でそれを指すパスです。クレートで定義した同じ名前のトレイトには、glob の `use` の先で見えるものも含めて何も足しません。
  - `drop` の本体は検証します。`drop` は何も前提にしません（`requires` を持てず、型の不変条件も前提にしない）。`unchecked fn drop` は、ほかの `unchecked` と同じく前提の一覧に出ます。
  - `drop` の本体から呼べるのは、`opens_invariants none` の関数だけです。利用者の `fn` は `.eyk` に `opens_invariants` を書けず、Verus の既定の `opens_invariants any` になるので、`drop` から呼ぶと `callee may open invariants that caller cannot` で失敗します（`no_panic` を付けても同じ）。vstd の `Vec::new` などや、`assert!`、`unwrap()`、添字、`panic!` も同じエラーになります。今の `drop` に書けるのは、フィールドへの単純な書き込みのような、関数を呼ばない本体です。扱いは判断待ちです（#94）。
  - 型の不変条件を持つ型（`impl Invariant for T`）の `impl Drop` は、ほかの trait の impl で §6.4 の展開が契約を足すメソッドと同じく E0003（M4）です。§6.4 のとおりなら `drop` に `requires old(self).invariant()` が付きますが、Verus は `Drop` の `requires` を拒否するので、M4 で扱いを決めます。

* 元の仕様：§7.2 の表（`spec fn` は「`open spec fn f(..)`（`pub` はそのまま）」、純粋関数は同じ式を `fn` と `spec fn` に写す）、§5.2 の正規順（`by` が最後）。
* 根拠：#17（PR #26）、#6（PR #13）、#61。プローブ `when_used_as_spec.rs`、`prover_mode_by_last.rs`、`drop_opens_none.rs`、`drop_requires.rs`、`drop_calls_opens_any.rs`、`opens_none_vec.rs`。golden `tests/golden/drop`。
* 状態：`spec fn` は基準で決めた。ほかは実測で決めた。`drop` から呼べる関数は判断待ち（#94）。

### §7.3 構造体と資源の書き下し

- `put r[p];` は `proof { <r>.insert(__eyk_perm_p); }` です。`borrow mut` の末尾の `insert` も `proof { }` に入れます。実行コードに文として置くと、proof のモードの関数を呼べないエラーになります。
- 論理文脈の `<r>` は `self.__eyk_erased_r@` です（`r[p]` は `self.__eyk_erased_r@.index(p)`。名前は次の「消去フィールドの名前」）。
- `uses &mut p@` には、`requires __eyk_perm_p.pptr() == p` に加えて `ensures final(__eyk_perm_p).pptr() == p` を足します（呼び出しの後も `*p` を使えるように）。`Cell` は `.id() == c.id()` で、`&Ptr<T>` の引数は `*p` と比べます。
- `-> (q: Ptr<T>) gives q@` は `-> (__eyk_ret: (Ptr<T>, Tracked<PointsTo<T>>))` にします。`ensures` の `q` を `__eyk_ret.0`、`q@` を `__eyk_ret.1@` に付け替え、`ensures __eyk_ret.1@.pptr() == __eyk_ret.0` を足します。本体の末尾と `return e` は `(e, Tracked(__eyk_perm_q))` です。§7.3 の `-> (q: Ptr<T>, Tracked<PointsTo<T>>)` は Verus の構文にありません。
- 権限の変数を渡すときは、権限そのものなら `Tracked(&x)` / `Tracked(&mut x)`、借用（`uses &p@`、`uses &mut p@`、`borrow`）なら `Tracked(&*x)` / `Tracked(&mut *x)` です。
- 型を書いた `let`（`let p: Ptr<u8> = Ptr::new(..)`。`Ptr::empty`、`Cell::new`、`gives` の呼び出しも同じ）は、`let (p, Tracked(__eyk_perm_p)): (Ptr<u8>, Tracked<_>) = ..` にして型注釈を残します。型注釈を落とすと値の型が変わります。
- 値で受けた領域 `uses r: Region<L>` と、移動で受けた権限 `uses p@` は、引数を `Tracked(r): Tracked<Region<L>>` / `Tracked(__eyk_perm_p): Tracked<PointsTo<T>>` のまま、本体の先頭で `let tracked mut r = r;` / `let tracked mut __eyk_perm_p = __eyk_perm_p;` と束縛し直します。`take` / `put` / `borrow mut` と、`*p = e` / `(*p).f = e` で変更できるようにするためです。Verus は引数のパターン `Tracked(mut r)` を受け付けません（「ill-formed `unwrap_parameter` header」）。変更しない関数でも、Verus と Verus なしの cargo は `unused_mut` の警告を出しません。

* 元の仕様：§7.3 の表（`put` は `<r>.insert(..);`、論理の `<r>` は `self.r.borrow()`、`gives` は `-> (q: Ptr<T>, Tracked<PointsTo<T>>)`、`uses r: Region<L>` は `Tracked(r): Tracked<Region<L>>` だけ）。
* 根拠：#6（PR #13、プローブ `tracked_field_forms.rs`、`tracked_field_a3_literal.rs`）、#17（PR #26）、#90、#102（`tests/golden/lowering` の `Bag`、`set_owned`、`set_and_put`）。
* 状態：`put`、論理の `<r>`、値で受けた領域と移動で受けた権限の束縛し直しは実測で決めた。ほかは基準で決めた。

### §7.3 消去フィールドの名前

消去フィールド `f`（`owns` / `spec`）は、生成する Verus では `__eyk_erased_f` という名前のフィールドにします。`struct S { a: A } owns { r: R } spec { q: Q }` は `struct S { a: A, __eyk_erased_r: Tracked<R>, __eyk_erased_q: Ghost<Q> }` です。`self@r` は `self.__eyk_erased_r.borrow_mut()` / `self.__eyk_erased_r.borrow()` / `self.__eyk_erased_r@`、リテラルの `@r: e` は `__eyk_erased_r: Tracked(e)`、パターンの `@r` は `__eyk_erased_r: Tracked(r)`、`self@q = e;` は `proof { self.__eyk_erased_q = Ghost(e); }` です。

`.eyk` の `x.r` は、`ebykn check` が見逃しても（§5.4「消去フィールドは `@` でだけ書く」）、生成した Verus にそのフィールドがないのでビルドで止まります。`__eyk_` は利用者が書けないので（`E0101`）、`.eyk` から生成した名前にも届きません。

* 元の仕様：§7.3 の表は、消去フィールドを同じ名前で書き下す（`struct S { a: A, r: Tracked<..>, q: Ghost<..> }`、`self.r.borrow_mut()`、`self.q@`）。
* 根拠：#99。同じ名前だと、`.eyk` の `self.r` が `Tracked` のフィールドに直接届いた。
* 状態：基準で決めた。

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
| `macro` | 依存クレートの derive・属性マクロが付いた項目（§5 の「依存クレートの derive と属性マクロ」）。マクロが作る項目は Verus が検証しない |
| `failed` | Verus のエラーの失敗の場所が、項目の行の範囲にある。失敗の場所は、エラーの診断の中の主な span（`^`）の行と `at this call-site` のラベルの行。ただし、呼ばれる側を示すだけのラベル（`failed precondition`、`invariants opened by callee`、`perhaps you need to mark this function as no_unwind?`）の行は数えない（`crates/ebykn/src/sites.rs`） |
| `unknown` | Verus の `--output-json` の `func-details` に項目の関数がない（Verus が見なかった。`#[cfg]` で消えた項目など）か、エラーの診断の副のラベル（`-`）か、終わりの印の見つからない複数行の span がその項目の行を指すか、どの項目にも割り当てられないエラー（失敗の場所がないか、項目の外にある。診断に `NN \|` の行がないエラーも含む）があるか、Verus が検証の前に止まった |
| `verified` | それ以外。Verus 自身が検証した関数で、エラーがない |

- 前提の状態（`admit` / `unchecked` / `external`）は、エラーより優先します（止まった実行に限りません）。コンパイルエラーを持つ `unchecked fn` も `failed` ではなく `unchecked` と出ます。そのエラーの場所は、標準エラーに出す Verus の診断の生成コードの位置で示します。

- 項目の Verus の名前は、impl のメソッドなら型の完全なパスの下、trait の既定のメソッドなら trait の下です。クレートの外の型への impl のメソッドは `impl を置いたモジュール::impl&%N::名前` で、形に当たる鍵がちょうど一つのときだけ決まり、決まらなければ `unknown` です（表示は `<u32 as Tr>::名前`）。同じ Verus の名前になる項目が二つ以上あれば、全部 `unknown` です。
- Verus の数えたエラーと読み取ったエラーの見出しの数が違えば、エラーを割り当てられないとみなします。
- `failed` か `unknown` があれば、`verify` は失敗します。
- 証明なしの前提（`admit` / `unchecked` / `external` / `macro`）は、`verify item..` の絞り込みによらず全体から出します（絞り込むのは表示だけで、検証は全体）。
- `macro` の行は、derive なら `macro <パス> derive on <項目>`、属性マクロなら `macro <パス> on <項目>` です（`macro serde::Serialize derive on k::Foo`、`macro tokio::main on k::main`）。ファイルごとに、項目の行の後に並べます。`--json` では `status` が `macro` で、`macro`（パス）と `kind`（`derive` / `attribute`）が付きます。
- 出力は、項目ごとの「状態 パス」の行と、最後の `results N verified, M errors` です（`status`（M5）の形が決まるまでの仮の形）。
- Verus が検証の前に止まったとき（コンパイルエラーなどで、検証のエラーが 0 件か件数がない）は、最後の行を `results not verified: Verus stopped before verification (N errors)` にし、何も検証していないことを英日の文言でも出します（`--output-json` はコンパイルエラーでも `verified: 0, errors: 0` を出します）。このとき（`--json` の `ran: false`）の項目の状態は、前提なら `admit` / `unchecked` / `external` のまま、そうでなくエラーの失敗の場所がその項目の行の範囲にあれば `failed`、ほかはすべて `unknown` です。`func-details` に関数があっても `verified` にしません（Verus は止まる前に関数の一覧を出すことがある）。テキストと `--json` は同じ状態を出します。`tests/golden/stopped/`（Verus が対応しない std の関数 `std::process::id` を呼ぶ）で固定してあります。
- `--json` では、1 行の JSON（`$message_type: ebykn-verify`。`results` は `{"ran": true, "verified": N, "errors": M}` か `{"ran": false, "errors": N}`）を標準出力に出し、Verus の診断は標準エラーに出します。
- Verus の出力は `CARGO_TERM_COLOR=never` で受け取り、制御文字を除いて読みます。Verus の診断は、生成コードの位置を `build/verus/src/..（生成コード）` として出します（§10.2 の変換できない場合の形）。

紛れ込んだ前提の照合：1 回目が検証まで進んだら、`cargo verus focus -- --no-cheating --no-verify` をもう一度走らせ、出たエラーを、Ebykn が自分で出した前提（`unchecked` と本体のない補題）と照らし合わせます（`crates/ebykn/src/no_cheating.rs` の `judge`）。1 回目が検証の前に止まったときは走らせません（`verify` はそれで失敗する）。`--no-verify` でも `--no-cheating` の拒否は全部出るので、2 回目は検証を省きます。

- 許すのは、`unchecked` の項目の行の範囲にある `external_body` の拒否と、本体のない補題の行の範囲にある `assume` / `admit` の拒否を、項目ごとに一つずつだけです。`#[test]` の `#[verifier::external]` は `--no-cheating` でも拒否されないので、許す拒否はありません。
- それ以外のエラーは、拒否の文言か、生成コードの中か（`include!` の位置は `src/../../../x.in` のように生成コードの外になる）を問わず、紛れ込んだ前提として `verify` を失敗にし、その場所を出します。依存クレートのマクロ、`include!`、マクロが出す入れ子の `verus!` から入った `assume(false)` を、これで捕まえます。
- 次のときは、照合できないとして失敗にします：Verus が失敗したのに見出しを読めない、cargo の数えたエラー（`due to N previous errors`）と読んだ見出しの数が違う、cargo がクレートを作り直した行（`Checking 名前 v..`）がない。cargo は同じ位置・同じ文言のエラーを 1 つにまとめて出すが件数はまとめる前の数のままなので、Ebykn の `admit()` と同じ位置に紛れ込んだ拒否も、この件数の食い違いで照合できないことになります。各実行の前の `cargo clean -p` が失敗したら、`verify` を止めます。
- 紛れ込んだ前提があるか照合できないときは、どの項目がそれに頼るか分からないので、`verified` の項目をすべて `unknown` にします。
- 出力は、テキストでは `smuggled 位置 文言` の行（照合できなければ `smuggled unreadable: 理由`。位置は、生成コードならプロジェクトのルートからのパスを字句的に正規化したもの）、`--json` では `no_cheating`（`state` が `clean` / `smuggled` / `unreadable` / `not_run`、`checked`、`smuggled` の位置と文言か `reason`）です。標準エラーには、紛れ込んだものの Verus の診断と英日の文言を出します（照合できないときは、Ebykn 自身の前提への拒否も含むと断って、2 回目の出力の全文）。
- 捕まえるのは、Verus が `--no-cheating` で拒否するもの（`assume` / `admit` / `external_body` / `assume_specification`、`assume_termination` など）が、Ebykn の前提の一覧の外に出たときだけです。`verus!` の外で依存クレートの derive・属性マクロが作る項目は、Verus が検証しないだけで拒否しないので捕まえません。それが付いた項目は `macro` の前提として出します（#49）。
- M5 で性質ファイル（trait の仕様）を足すと、一つの仕様からメソッドごとに拒否が出うるので、「許す拒否は項目ごとに一つ」の規則をそのとき仕様の単位で数え直します。
- `tests/golden/smuggled/` と `crates/ebykn/tests/no_cheating.rs`（Verus が要るので `#[ignore]`。`cargo xtask golden-verify` が走らせる）で固定してあります。

* 元の仕様：§9 は `verify` を「`check` ＋ Verus による検証」とし、項目の状態と出力の形を §9.1（`status`）に置く。`--no-cheating` に触れていない。
* 根拠：#17（PR #26）、#43（PR #45）、#37（PR #42）。表示の根拠を Verus 自身の証拠（`func-details` とエラーの位置）にし、根拠のない verified を出さない。
* 状態：基準で決めた。

### 生成クレートの成果物は `build/verus/target`

`check`・`verify`・`build` が生成クレート（`build/verus/`）で走らせる cargo（`cargo metadata`、`cargo clean -p`、`cargo verus ...`）は、どれも `CARGO_TARGET_DIR` に `build/verus/target` の絶対パスを渡します。最終の成果物（`build` が作る実行ファイルなど）は、利用者の環境の `CARGO_TARGET_DIR` や、親のディレクトリの `.cargo/config.toml` の `build.target-dir` によらず、`build/verus/target` の下にできます（`cargo verus focus` の成果物は `build/verus/target/verus-partial`）。中間の成果物は、`build.build-dir` を設定すればその下に移ります。親の設定で移しても、`verify` と `build` の結果は設定のないときと同じです。

- 置き場を cargo の設定に任せると、各実行の前の `cargo clean -p` が消す成果物と cargo が使う成果物が食い違います。前の成果物が残ると、cargo はクレートを作り直さず Verus を呼ばないので、`verify` の項目がすべて `unknown` になります。
- cargo の呼び出しは `crates/ebykn/src/verus.rs` の `cargo` でだけ作ります（単体テストが数える）。置き場は `target_dir` の 1 か所で決め、`build` が示す置き場もそこから作ります。`build.target-dir` を設定した親の下（`build.build-dir` も設定したものと、しないもの）のプロジェクトで `verify` を 2 回と `build` を走らせ、`build` が示す置き場に成果物があることも確かめるテスト（`crates/ebykn/tests/target_dir.rs`。Verus が要るので `#[ignore]`。`cargo xtask golden-verify` が走らせる）で固定してあります。

* 元の仕様：§9 は `build/verus/` に書き下すとするが、cargo の成果物の置き場を定めていない。
* 根拠：#137（PR #146）。
* 状態：基準で決めた。

### `ebykn sync` の決めごと

`sync` は `proof/` の下の各 `.eykp` を、仕様 §9 のとおり `git merge-file` で更新します（`proof/<p>.eykp` ↔ `src/<p>.eyk` ↔ 基準 `.ebykn/sync/<p>.eyk`）。仕様が定めていない場合は、次のように扱います。

- 基準がない `.eykp`（初めての `sync`、`.eykp` を作ったばかり）は、今の `src/<p>.eyk` を基準として書くだけで、`.eykp` は変えません（`base`）。`.eykp` が古い `src/` の写しなら、取り込まれなかった差は剥がし一致検査（#65）が `E0304` で見つけます。基準を書く前に、剥がした `.eykp` と `src` を比べるようにするのは #97 です。
- 衝突したときも、基準を今の `src/<p>.eyk` に進めます。進めないと、手で解いた hunk が次の `sync` のたびにまた衝突します。印の行の名前は `proof/<p>.eykp`・`.ebykn/sync/<p>.eyk`・`src/<p>.eyk` です。衝突の文言は、書く前に `.eykp` と基準の両方を git が追跡していて作業ツリーに変更がなかったときだけ、その `sync` の取りやめ方（`git -C <ルート> checkout -- proof/<p>.eykp .ebykn/sync/<p>.eyk`）を示します。`git checkout` はインデックスの版に戻すので、コミットしていない変更があれば一緒に消えるからです。そうでないときは、`git checkout` では取りやめられないと伝えます。
- `.eykp` に `sync` 自身の衝突の印の行（`<<<<<<< proof/<p>.eykp` か `>>>>>>> src/<p>.eyk`）が残っていれば、何も変えずに失敗にします（`unresolved-markers`）。基準を進めたあとなので、印を残したまま成功にすると、衝突した変更が `.eykp` に入らないまま次に進み、さらに `src` を変えると印が入れ子になります。
- 対応する `.eykp` のない基準（`.ebykn/sync/**.eyk`）は失敗として報告し、名前を変えたモジュールなら新しい名前に移し、消したモジュールなら削除するよう案内します。残すと、あとで同じ名前を使うモジュールが古い基準から変更を当てます。そうした基準があるあいだは、基準のない `.eykp` にも基準を作りません（`base-held`）。名前を変えたモジュールの新しい名前の `.eykp` に今の `src` を基準として書くと、名前を変えたあとの変更が `.eykp` に届かないからです。
- 対応する `src/<p>.eyk` がない `.eykp` は変えず、失敗として数えます（`E0301` に当たる）。
- `git` が動かない、読み書きに失敗した、`git merge-file` が衝突の数でない終了コードを返した（中身が二進の `.eykp` の 255 など）、衝突を数えたのに出力に印がない、のどれかのときは、そのファイルを変えずに失敗として数えます。`.eykp` には取り込んだのに基準を書けなかったときも失敗にし（衝突の数も出す）、基準を直してから `sync` を実行し直すよう案内します。
- `.eykp` と基準は、同じディレクトリの一時ファイル（`.<名前>.ebykn-sync-tmp`）に書いてから名前を変えて置き換えます。`std::fs::write` のようにファイルを空にしてから書くと、途中で失敗したとき（ディスクの空き、ファイルの大きさの上限など）に途中までの中身が残るためです。置き換えるファイルの mode は引き継ぎ、読み取り専用のファイルは置き換えずに失敗にします。symlink の `.eykp` は、指す先のファイルを置き換えます（symlink はそのまま）。
- `src/` にあって `proof/` にないモジュールには基準を作りません（`.eykp` は無くてよい。§3.1）。
- 終了コードは、すべての `.eykp` を衝突も失敗もなく扱え、`.eykp` のない基準もなければ 0、そうでなければ 1 です。引数を付けると使い方の誤りで 2 です。
- 出力は、`.eykp` ごとに状態（`unchanged` / `merged` / `conflict` / `base` / `error`）の 1 行を標準エラーに出します。理由は `--lang` の言語で出します。`--json` では `{"$message_type":"ebykn-sync","success":..,"files":[..],"orphan_bases":[..],"errors":[..]}` の 1 行を標準出力に出します（`files` の各項目は `proof`・`source`・`base`・`state`。`state` は `unchanged` / `merged` / `conflict` / `base-created` / `base-held` / `unresolved-markers` / `missing-source` / `failed` / `base-not-updated`。`conflict` と `base-not-updated` には衝突の数 `conflicts`、`failed` と `base-not-updated` には理由 `reason`（英語）が付く）。
- `.ebykn/sync/` はリポジトリに入れます（`ebykn init` の `.gitignore` は `/build/` だけを除く）。基準を共有しないと、ほかの人や証明を担当するエージェントの `sync` が別の基準から変更を当てます。`ebykn init` は `.gitattributes` に `/.ebykn/sync/** linguist-generated=true` を書き、GitHub の PR の差分で基準の写しを畳ませます。

* 元の仕様：§9 の表の 1 行（`git merge-file proof/p.eykp .ebykn/sync/p.eyk src/p.eyk` で更新し、基準を新しい `src` で置き換え、衝突はファイルに残して 1）だけで、基準がないとき、`src` がないとき、失敗したときを定めていない。
* 根拠：#69（PR #93 のレビュー）。
* 状態：基準で決めた。

### `ebykn build` は検証しない

`--verified` なしの `build` は、`check` のあと `cargo verus build -- --no-verify` でビルドします。検証の結果によらずビルドし、検証に失敗する項目があっても止まりません。止まるのは、`check` の失敗と、ビルドそのものの失敗（生成コードのコンパイルエラーなど）だけです。

- 成功したら、標準エラーに、作った実行ファイルの場所（ライブラリだけのクレートなら `build/verus/target/debug`）と、検証していないことを出します。ビルドできたことを verified と取り違えないためです。
- 検証は `ebykn verify` で行います。全項目が verified / unchecked であることをビルドの条件にする `--verified` は、`status` と一緒に M5 で実装します（今は `E0003`）。
- 生成コードは、Verus なしの cargo ではなく `cargo verus build` でビルドします。§9 の表の経路を保ち、prelude のコンパイル時の検査（#12）を利用者の経路で効かせるためです。
- golden の各ケースで、`ebykn build` が Verus なしの cargo と同じ期待（`expected/build-error.txt` があればその行で失敗、なければ成功）になることを確かめます（`cargo xtask golden-verify`）。`check` で止まるケース（`tests/golden/stopped/`。Verus が対応しない std の関数を呼ぶ）だけは、`expected/ebykn-build-error.txt` で `ebykn build` の失敗を期待します。`failed` の項目を持つ `tests/golden/stack/` も、`ebykn build` が通ります。

* 元の仕様：§9 の表は `build` を「`check` ＋ `cargo verus build`」とし、`--verified` を「全項目が verified / unchecked でなければ 1」とする。`cargo verus build` は既定で検証するので、`--verified` なしでも検証の失敗で止まっていた。
* 根拠：#80。`--verified` の条件を `--verified` なしの `build` には課さないと読み、設計ノート §1.3 の「検証なしでもビルドできること」と現行仕様 §1.3（`build/verus/` は Verus なしの cargo でもビルドできる）に合わせる。
* 状態：基準で決めた。

### 証明ファイルがあるときの `verify` / `build`

証明ファイル（`proof/` の `.eykp`）があると、`ebykn verify` と `ebykn build` は `E0003` で止まります（`.eykp` ごとに一つ、位置はそのファイルの先頭）。書き下しはまだ `.eyk` だけを読むので、そのまま進めると `.eykp` に書いた証明を捨てて、本体のない補題を `admit` の前提として検証することになるためです。`ebykn check` は止めません。剥がし一致を確かめ、`.eyk` を書き下して `verus --no-verify` にかけます。`.eykp` の本体にはまだ §6.2〜§6.5 の検査をかけないので、`check` の成功の文言と `--json` の `skipped` にそのことを出します。

* 元の仕様：§7 は `.eykp` を書き下す。
* 根拠：#65。書き下しが `.eykp` を読むのは #67。
* 状態：基準で決めた。#67 で書き下しが `.eykp` を読むようになったら外す。

### §9.1 Verus の結果の取り方（OPEN-7）

Verus の `--output-json` は、件数（`verification-results` の `verified` / `errors` / `success`）と、検証した関数の名前の一覧（`func-details`）を出します。どの関数が失敗したかは JSON になく、標準エラーの診断（位置つき）にだけ出ます。そのため、失敗した関数は診断の位置から決めます（上の `failed`）。決められないときに `--verify-function` で項目ごとに検証し直すかは、`status` の M5 で決めます。

* 元の仕様：OPEN-7「JSON 出力があればそれ、無ければ標準出力の `verification results::` と関数名を解析」。
* 根拠：#6（PR #13、`json_output.rs`、`json_output_failure.rs`）。
* 状態：実測で決めた。

## §10 診断

### §10.1 仕様にないコード

元の仕様の §10.1 の表にないコードです。元の仕様は、構文エラー、プロジェクトの誤り、未実装の機能、§6.4 の `&mut` の事後条件、消去フィールドの `.`、ZST の `Ptr`、呼び出しで渡す領域、局所の名前の raw の綴りに当たるコードを持ちません。

| コード | 内容 | 元の仕様 | 根拠 | 状態 |
| --- | --- | --- | --- | --- |
| `E0001` | 構文エラー。Verus にあって Ebykn にない書き方、性質ファイルだけの構文、拒否リスト（§5 の「マクロ」。#27）に当たる cfg とマクロも、文言を分けて `E0001`。`.eyk` の証明構文は `E0303`（#64） | コードがない（§10.1 の形式はコードを要する） | #7（PR #15） | 基準で決めた |
| `E0002` | プロジェクトの誤り。`Ebykn.toml` がない・読めない・知らない鍵・型が違う・`[verus]` が `ebykn` の固定値と違う、`mod foo;` のファイルがない・二つある、同じモジュールの二度の宣言、クレートルートがない・二つある | コードがない | #17（PR #20） | 基準で決めた |
| `E0003` | まだ実装しない機能。文言にどのマイルストーンで実装するかを書く（`.eykp` があるときの `verify` / `build`、`props/`、`[props]`、`[ebykn-dependencies]`、部分集合型、契約・`spec fn`・`lemma fn` を持つ trait、`impl spec`、型の不変条件を持つ型の `impl Drop`、`build --verified`、権限の操作・発散を含む文・式・欄に付けた `#[cfg]`、ジェネリクスの引数とパターンに付けた `#[cfg]`、代入・二項演算・型変換の文の属性、安定版の Rust が許さない位置の式の属性、マクロの引数の中の cfg・lint のほかの属性など） | コードがない（§1.1 は段階 1 の機能を挙げるだけ） | #17（PR #20） | 基準で決めた |
| `E0004` | `ensures` / `returns` の裸の `&mut` 引数、`old` / `final` の引数が `&mut` の引数でないもの、マクロの引数の中の `final(..)`（§6.4） | §6.4 は `ensures` の裸の `x` を事後の値と読む | #10（PR #35） | 作者が決めた |
| `E0103` | 局所の名前に Rust の予約語の raw の綴り（`r#type`）（§4） | 識別子の raw の綴りに触れない | #119（PR #148） | 基準で決めた |
| `E0404` | 消去フィールドを `.` で書いた（`e.f`、リテラルの `f: e`、パターンの `f`。§5.4） | `.` の形に触れていない | #99 | 基準で決めた |
| `E0517` | `Ptr<T>` の `T` が確実にゼロサイズ型（§7.1） | ZST に触れていない | #12（PR #47） | 作者が決めた |
| `E0518` | 呼び出しの `uses` の領域が、呼ばれる側の `uses` の宣言と合わない（数、`&` / `&mut` / 移動）（§5.2） | 呼び出しで領域を渡す構文がない | #19 | 判断待ちの既定 |
| `E0519` | 呼び出しで渡す領域の状態の誤り：移動した領域を使う、借りた領域か `self@x` を移動で渡す、`&` で借りた領域を `&mut` で渡す、同じ呼び出しで同じ領域を `&mut` か移動を含めて二度渡す（§5.2） | 同上 | #19 | 判断待ちの既定 |

`E0001`〜`E0004` は、§10.1 の番号帯（E01xx 字句、E03xx 証明ファイル、E04xx 構造体、E05xx 権限、W06xx 絞り込み、E08xx 性質ファイル）とぶつからない E00xx に置きます。`E0103` は字句の番号帯の続きです。`E0404` は構造体の番号帯の、`E0517`〜`E0519` は権限の番号帯の続きです。黙って無視すると、書いた性質や証明が効いていないのに検査が通るので、未実装の機能も `E0003` で止めます。

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
| OPEN-3（ユーザー定義 `Drop`） | M0 で確かめ、対応がなければ `owns` を持つ型の `Drop` を E にする | Verus は対応している（`opens_invariants none` と `no_unwind` を付ければ検証できる）ので、E にしない。書き下しが std の `Drop` の `fn drop` に両方を足す（§7.2）。`drop` は型の不変条件を前提にできず、`requires` も付けられないので、`owns` の権限を使って領域を解放することはできない。消費されずにスコープを抜ける値の警告は段階 2（既定どおり） | #6（PR #13）、#61、`drop.rs`、`drop_opens_none.rs`、`drop_requires.rs`、`drop_tracked_field.rs` | 実測で決めた |
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
| 付録 A.3 の生成 Verus | `self.nodes.borrow_mut().insert(__eyk_perm_n);`（文） | `proof { self.__eyk_erased_nodes.borrow_mut().insert(__eyk_perm_n); }`（§7.3） | #6、#99 |
| 付録 A.3・§7.3 の生成 Verus の消去フィールド | `nodes: Tracked<..>`、`self.nodes.borrow_mut()`、`self.order@` | `__eyk_erased_nodes: Tracked<..>`、`self.__eyk_erased_nodes.borrow_mut()`、`self.__eyk_erased_order@`（§7.3「消去フィールドの名前」） | #99 |
| 付録 B の不変条件 | `self.head == self@order.first()` | `self.head == self@order.get(0)`。`tests/golden/stack/src/lib.eyk` が付録 B と違うのはこの 1 か所だけ（`crates/ebykn_syntax/tests/appendix.rs` が確かめる） | #11 |
| §7.1 の `Seq` の拡張 | `first() -> Option<T>`、`last() -> Option<T>` | prelude に置かない。vstd の `first()` / `last()`（要素を返す）を使う | #11 |
| §7.1 の `Ptr::new` / `empty` | `-> (p: Ptr<T>, Tracked<PointsTo<T>>)` | `-> (r: (Ptr<T>, Tracked<PointsTo<T>>))`（§7.1） | #6 |
| §7.3 の `gives` | `-> (q: Ptr<T>, Tracked<PointsTo<T>>)` | `-> (__eyk_ret: (Ptr<T>, Tracked<PointsTo<T>>))`（§7.3） | #17 |
| §6.4 の表の `x: &mut T` | `ensures P[x]` | `ensures P[final(x)]`（§6.4） | #10 |
| §7.4 の `and { }` | `(a && b && c)` | `((a) && (b) && (c))`（§7.4） | #17 |
