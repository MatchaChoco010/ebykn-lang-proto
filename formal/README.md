# 形式モデル

Ebykn 自身が判定する規則の参照モデルを Verus で書いて検証し、実装との差分テストのオラクルにします（AGENTS.md「形式モデル」、#1 の F）。今あるのは仕様 §6.2（権限スコープと線形性）のモデルです（#18）。

| 置き場 | 内容 |
| --- | --- |
| `linearity/src/syntax.rs` | 抽象した文の言語と、Γ の操作 |
| `linearity/src/check.rs` | 検査。`chk_*` が定義、`check` が実行できる版（定義と一致することを Verus が確かめる）。わざと壊した検査も同じ関数で、規則の有無（`Rules`）を切り替えて作る |
| `linearity/src/sem.rs` | 権限の操作だけを追う実行の意味と、違反（`Viol`） |
| `linearity/src/sound.rs` | 健全性の証明 |
| `linearity/src/enumerate.rs`、`linearity/src/oracle.rs`、`linearity/examples/oracle.rs` | ケースの列挙と、判定の表の生成（検証しない普通の Rust） |
| `oracle/linearity.tsv` | 判定の表。1 行が 1 ケース |
| `oracle/linearity.eyk` | 各ケースを `.eyk` の関数 `c<番号>` に書き出したもの |
| `oracle/linearity-mutants.tsv` | わざと壊した検査が、表のどのケースで正しい検査と食い違うか |

## 証明したこと

`cargo xtask verus-verify` が固定した Verus で `--no-cheating` のまま検証する（91 verified, 0 errors）。下の 6 つの定理が検証されたことは、`--output-json` の関数の一覧で名前を確かめる（定理を消すか検証の対象から外すと落ちる）。`admit` / `assume` / `external_body` は使っていない。

再帰を含む型（`syntax.rs` の `Op`、`Stmt`、`Block`、`Env`、`Program`、`check.rs` の `LoopCtx`、`Ctx`、`Res`）には `#[verifier::external_derive]` を付けた。これがないと、Verus は derive した `Clone` などを循環した定義として拒む。付けると derive した `Clone` / `PartialEq` / `Eq` / `Debug` / `Hash` は検証の外に置かれ、仕様は持たない。証明と検証する関数はこれらを使わない。spec の等しさは構造の等しさで、実行できる版は自前の `ex_copy` / `ex_eq` を使う。derive した実装を使うのは、検証しない表の生成器（`enumerate.rs`、`oracle.rs`）だけである。

- `theorem_soundness`：整形式（変数の添字が範囲にあり、`break` / `continue` がループの中にある）で `unchecked` でない関数を `check` が受理すれば、分岐の選択・ループの回数・`?` で戻るか・クロージャを呼ぶ回数をどう選んでも、実行は違反を起こさない。系として、次の 5 つを違反の種類ごとに述べる。
  - `theorem_consumed_at_most_once`：各権限は高々一度だけ消費される。
  - `theorem_no_use_after_consume`：消費した権限を借用の引数に渡さない。権限に束縛されていない変数で、権限が要る操作をしない。
  - `theorem_borrowed_not_consumed`：借用の権限（`borrow` ブロックの `p` と、`uses &p@` / `uses &mut p@`）は消費されない。
  - `theorem_no_write_to_shared`：共有の借用に書き込まない（`*p = e` と、`uses &mut p'@` の引数に渡すこと）。
  - `theorem_only_gives_left`：関数とクロージャの本体を出るとき、残る `Owned` は `gives` の名前に束縛されたものだけ。
- 検査は、本体を出る点（`return`・`?`・本体の末尾）で `gives` の名前が `Owned` でなければ E0504 で拒み（`syntax.rs` の `exit_err`）、クロージャの本体で権限を導入すること（`Ptr::new` などの導入）を E0512 で拒む（#63。実装の `OPEN.md` の §6.2 の読み方と同じ。`take`・`borrow`・`put` は前から E0512）。拒むだけなので、健全性の証明には効かない。
- `borrow` ブロックを `break` / `continue` / `return` / `?` で抜ける点で、借用は終わる（#21 の案 D）。検査は、行き先のループに入ってから開いた `borrow` の `p` を Γ から消して比べ（`check.rs` の `release`）、実行の意味では、抜ける途中の各 `borrow` が `p` の束縛を入る前に戻す（`sem.rs` の `restore`。権限は領域に戻るので消費ではない）。証明は、抜ける文の点の状態を、まだ戻していない束縛を戻した状態（`sound.rs` の `restore_mask`）で不変条件に結びつける。上の系はこのまま成り立つ。
- 戻り値の型を `Option` / `Result` と書いていない関数（`Program::try_shape`）の `borrow mut` の中の `?` は E0003 で拒む（書き下しが失敗の経路を展開できない。#21）。拒むだけなので、健全性の証明には効かない。
- `check`（実行できる版）は定義 `spec_check` と一致する。
- 検査と実行の意味はどれも停止する（各関数の `decreases` を Verus が確かめる）。

## 証明していないこと・抽象化で捨てたもの

- 式と型。変数は添字で、関数ごとに数が決まる。式の中の権限の操作は、文として前後に置いたものとみなす。
- 領域は関数の引数の一つ（`uses r: &mut Region<..>`）だけ。`let r: Region<..> = e;` とパターンの `@r` による R への導入は持たない。R はクロージャの中で空になることにだけ使う。
- `*p` の読み。`p` が Γ にあるときは常に許され、Γ にないときは Rust の参照外しなので、判定に効かない。
- `*p = e` は、`p` が生きた札に束縛されているときだけ権限付きの書き込みとみなす（仕様 §6.2 の「`X ∉ Γ` なら Rust のまま」）。消費した後の `*p = e` は Rust の書き込みで、rustc が拒む。
- クロージャは定義した位置で 0 回以上呼ぶものとし、外の変数の束縛は見えない（§5.6）。外の変数への代入がクロージャの外に及ぶことは扱わない（検査は E0512 で拒む）。
- ラベル付きの `break` / `continue`、`for` の反復子、`match` の 3 本以上の腕（2 本の腕と同じ規則）。
- `gives` の名前が本体を出る点で `Owned` であることは、検査が E0504 で拒むだけで、定理は述べない（実行の意味は、戻り値に権限を付けて返すことを持たない）。
- 違反のコードと位置の選び方（`OPEN.md`）。定理が述べるのは受理したときの健全性だけで、どのコードで拒むかは表で固定する。
- 仕様が書いていない場合の決め方は `OPEN.md` に記録した（#18 と #21 の行）。
- `return` / `?` で `borrow` ブロックを抜けたあとの束縛（本体を出るので、それ以上の操作がない）。`unchecked fn` の本体の E0003（実装は `unchecked fn` でも拒む）。マクロの引数の中の脱出（実装は E0003 で拒む）。

Γ の欄は仕様の 3 状態より細かい。`uses &p@` と `borrow` の `p` を分け（E0509 のため）、クロージャの本体で外の変数を `Captured` にする（E0512 のため）。どちらも途中で変わらないので、合流の比較の結果は仕様と同じになる。

## 判定の表

`cargo xtask formal-oracle` がモデルから表を作り直し、`formal/oracle/` と一致することを確かめる（CI の verus ジョブ）。モデルを変えたら `cargo xtask formal-oracle-write` で作り直してコミットする。`cargo test -p xtask` は、`linearity.eyk` を M1 の構文解析器（`ebykn_syntax`）で読めること、表の行と関数が一対一であること、診断の位置が文の先頭か閉じ括弧であることを確かめる。

列挙の上限（`linearity/src/enumerate.rs` の `TIERS`）。文の大きさは、原子文が 1、複合文が 1 ＋ 中の文の大きさの和。どの段も、止まる文の後ろに文がない、`break` / `continue` はループの中、`let-else` の `else` は止まる、`gives` と `?` を同時に使わない、という Rust として書ける形だけを残す。

| 段 | 変数 | 大きさ | 文の形 | 関数の頭 | ケース |
| --- | --- | --- | --- | --- | --- |
| `core` | `p` | 2 以下 | 導入（`Ptr::new`）、`take`、`free`、借用の引数（`&` / `&mut`）、`*p = 1`、代入、`return`、`?`、`panic!`、`break`、`continue`、`while`、`loop`、`borrow` / `borrow mut`、クロージャ、`let-else`、`if` | `uses` なし / `p@` / `&p@` / `&mut p@`、`gives p@` | 1685 |
| `deep` | `p` | ちょうど 3（複合文を含む） | 導入、`free`、`&mut` の引数、`return`、`panic!`、`break`、`continue`、`while`、`loop`、`borrow mut`、クロージャ、`let-else`、`if` | `uses` なし / `p@` | 4890 |
| `forms` | `p` | 2 以下（`core` にない形を含む） | `core` に加え `Ptr::empty`、`Cell::new`、`gives` を持つ関数、`put`、移動の引数、`leak`、`let`、`if let`、`match` | `uses` なし / `p@` | 1028 |
| `two` | `p`、`q` | 2 以下（`q` に触れる） | 両方の変数の導入、`take`、`free`、`&mut` の引数、`*x = 1`、代入、`return`、`break`、`continue`、`while`、`loop`、`borrow mut r[p]`、`borrow r[q]`、クロージャ、`if` | 6 通り | 1626 |
| `exit` | `p` | ちょうど 4（ループの中の `borrow` から、そのループへ `break` / `continue` で抜けるか、ループの中の `borrow` の中のループを抜ける） | `take`、`&mut` の引数、`*p = 1`、`break`、`continue`、`while`、`loop`、`borrow` / `borrow mut` | `uses` なし | 250 |
| `alias` | `p` | 2 以下（`?` を含む） | `core` と同じ | `uses` なし / `p@`。戻り値の型を別名 `Alias` で書く | 70 |
| `unchecked` | `p` | 1 以下 | `core` と同じ | `unchecked fn`、`p@` | 17 |

`linearity.tsv` の列：`case`（`linearity.eyk` の関数名）、`tier`、`header`・`body`（抽象のプログラム）、`verdict`（`ok` か E0503〜E0512、E0003）、`pos`（モデルの位置。`at <文の番号>` か `end <文の番号>`、番号は前順で 1 から、`end 0` は関数本体）、`line`・`col`（`linearity.eyk` の中の位置）。

## 負の証拠

`linearity-mutants.tsv` は、規則を一つ外した検査が表のどのケースで正しい検査と食い違うかを示す。食い違うケースがない壊れた検査があれば、表の生成が失敗する（列挙の上限を上げる）。

| 壊れた検査 | 外した規則 |
| --- | --- |
| `no-join` | 分岐の終端の Γ を比べない（E0510） |
| `no-borrow-consume` | `borrow` ブロックの `p` を `Owned` と同じに消費できる（E0509） |
| `no-continue` | `continue` の点の Γ を本体の入口と比べない |
| `no-break` | `break` の点の Γ をループの直後の Γ と比べない |
| `no-loop-end` | 本体の終端の Γ を本体の入口と比べない |
| `no-closure-reset` | クロージャの本体で Γ を空にしない |
| `no-try-return` | `?` を `return` として扱わない |
| `no-release` | `break` / `continue` で抜ける点で、`borrow` ブロックの `p` を Γ から消さない（#21）。`while` と `continue` では正しいプログラムを E0511 で拒み、`loop` を抜けた後に借用中の状態を残して、権限のない変数の借用を通す（`exit` の段の c09300 など） |
| `no-loop-clear` | ループの中のループに入るときに、外のループに入ってから開いた `borrow` の印を引き継ぐ（#21）。内側のループの `break` で外の `borrow` の `p` まで消す |
| `no-try-shape` | 戻り値の型を `Option` / `Result` と書いていない関数の `borrow mut` の中の `?` を通す（E0003。#21） |
| `no-gives-owned` | 本体を出る点で `gives` の名前が `Owned` であることを求めない（E0504。#63） |
| `no-closure-intro` | クロージャの本体で権限を導入できる（E0512。#63） |

## 実装との対応

モデルは仕様と `OPEN.md` の §6.2 の読み方から作る。出口の `gives` の規則（E0504）とクロージャの中の導入の規則（E0512）は、仕様が定めていない所で実装の読み方に従うので、この 2 つの規則ではオラクルは実装から独立していない（#63）。実装と一致するかは、差分テストのゲート `cargo xtask check-formal-diff`（`xtask/src/gates/formal_diff.rs`。`gates` に入り、CI と pre-commit で走る）で確かめる（#63）。

- `linearity.eyk` を一つのプロジェクトの `src/lib.eyk` として `ebykn check` と同じ検査にかけ、診断を位置で関数 `c<番号>` に割り当てる。表の `verdict` が `ok` なら診断がないこと、コードならそのコードが診断にあることを確かめる。
- 位置はまだ比べない。位置の選び方（`OPEN.md` の #18 の行）は、実装とモデルで違う（[#141](https://github.com/MatchaChoco010/ebykn-lang-proto/issues/141)）。
- 食い違いは、ゲートの `EXCEPTIONS` に、ケース・実装の判定・理由・追跡の Issue で持つ。一覧にない食い違い、一覧にあるのに一致するケース、実装の判定が一覧と違うケースは失敗にする。一覧の件数は、空のときも出力に出す。今は一覧が空で、全ケースが一致する。
- 補助の関数（`cond`、`make`、`consume` など）に診断が出れば失敗にする。
- モデルは変数をすべて権限の名前とみなす。実装は、一度も権限を持っていない名前の `free` を、型を `Ptr` / `Cell` と書いたときだけ §6.2 の `free` と読む（`docs/current-spec.md` の §6.2）。そのため、表の `.eyk` は `let` に型を書く（`let p: Ptr<u32> = other();`）。
- 断片は型を合わせていないので、型を見る段（§6.6 の `verus --no-verify`）には通さない。
