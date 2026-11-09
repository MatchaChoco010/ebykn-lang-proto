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

`cargo xtask verus-verify` が固定した Verus で `--no-cheating` のまま検証する（78 verified, 0 errors）。下の 6 つの定理が検証されたことは、`--output-json` の関数の一覧で名前を確かめる（定理を消すか検証の対象から外すと落ちる）。`admit` / `assume` / `external_body` は使っていない。

再帰を含む型（`syntax.rs` の `Op`、`Stmt`、`Block`、`Env`、`Program`、`check.rs` の `LoopCtx`、`Ctx`、`Res`）には `#[verifier::external_derive]` を付けた。これがないと、Verus は derive した `Clone` などを循環した定義として拒む。付けると derive した `Clone` / `PartialEq` / `Eq` / `Debug` / `Hash` は検証の外に置かれ、仕様は持たない。証明と検証する関数はこれらを使わない。spec の等しさは構造の等しさで、実行できる版は自前の `ex_copy` / `ex_eq` を使う。derive した実装を使うのは、検証しない表の生成器（`enumerate.rs`、`oracle.rs`）だけである。

- `theorem_soundness`：整形式（変数の添字が範囲にあり、`break` / `continue` がループの中にある）で `unchecked` でない関数を `check` が受理すれば、分岐の選択・ループの回数・`?` で戻るか・クロージャを呼ぶ回数をどう選んでも、実行は違反を起こさない。系として、次の 5 つを違反の種類ごとに述べる。
  - `theorem_consumed_at_most_once`：各権限は高々一度だけ消費される。
  - `theorem_no_use_after_consume`：消費した権限を借用の引数に渡さない。権限に束縛されていない変数で、権限が要る操作をしない。
  - `theorem_borrowed_not_consumed`：借用の権限（`borrow` ブロックの `p` と、`uses &p@` / `uses &mut p@`）は消費されない。
  - `theorem_no_write_to_shared`：共有の借用に書き込まない（`*p = e` と、`uses &mut p'@` の引数に渡すこと）。
  - `theorem_only_gives_left`：関数とクロージャの本体を出るとき、残る `Owned` は `gives` の名前に束縛されたものだけ。
- `check`（実行できる版）は定義 `spec_check` と一致する。
- 検査と実行の意味はどれも停止する（各関数の `decreases` を Verus が確かめる）。

## 証明していないこと・抽象化で捨てたもの

- 式と型。変数は添字で、関数ごとに数が決まる。式の中の権限の操作は、文として前後に置いたものとみなす。
- 領域は関数の引数の一つ（`uses r: &mut Region<..>`）だけ。`let r: Region<..> = e;` とパターンの `@r` による R への導入は持たない。R はクロージャの中で空になることにだけ使う。
- `*p` の読み。`p` が Γ にあるときは常に許され、Γ にないときは Rust の参照外しなので、判定に効かない。
- `*p = e` は、`p` が生きた札に束縛されているときだけ権限付きの書き込みとみなす（仕様 §6.2 の「`X ∉ Γ` なら Rust のまま」）。消費した後の `*p = e` は Rust の書き込みで、rustc が拒む。
- クロージャは定義した位置で 0 回以上呼ぶものとし、外の変数の束縛は見えない（§5.6）。外の変数への代入がクロージャの外に及ぶことは扱わない（検査は E0512 で拒む）。
- ラベル付きの `break` / `continue`、`for` の反復子、`match` の 3 本以上の腕（2 本の腕と同じ規則）。
- `gives` の名前が `return` の時点で `Owned` であること（§6.2 は残ってよいものだけを決めている。満たさなければ Verus が拒む）。
- 違反のコードと位置の選び方（`OPEN.md`）。定理が述べるのは受理したときの健全性だけで、どのコードで拒むかは表で固定する。
- 仕様が書いていない場合の決め方は `OPEN.md` に記録した（#18 の行）。`borrow` ブロックを `break` / `continue` で抜ける場合は判断待ち（#21）。

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
| `unchecked` | `p` | 1 以下 | `core` と同じ | `unchecked fn`、`p@` | 17 |

`linearity.tsv` の列：`case`（`linearity.eyk` の関数名）、`tier`、`header`・`body`（抽象のプログラム）、`verdict`（`ok` か E0503〜E0512）、`pos`（モデルの位置。`at <文の番号>` か `end <文の番号>`、番号は前順で 1 から、`end 0` は関数本体）、`line`・`col`（`linearity.eyk` の中の位置）。

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
| `no-cross` | `borrow` ブロックを `break` / `continue` で抜けることを拒まない（§6.2 の文言どおりの読み。#21） |

## 実装との対応

モデルは仕様だけから作り、実装（`crates/ebykn_check`）は見ていない。実装と一致するかは、M2 の §6.2 が入った後の差分テストで確かめる。`linearity.eyk` の各関数を実装の検査にかけ、最初の診断のコードと位置を `linearity.tsv` と比べる。補助の関数（`cond`、`make`、`consume` など）は §6.2 の検査を通る。断片は型を合わせていないので、型を見る段（§6.6 の `verus --no-verify`）には通さない。
