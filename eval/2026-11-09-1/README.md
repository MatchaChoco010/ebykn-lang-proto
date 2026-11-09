# dogfooding 2026-11-09 の 1 回目

- 対象：`develop` の 9cad241（`Ptr<T>` の ZST の検査 #12 まで）。PR を出したあと、848b2c8（#51、#58 まで）でも、同じ結果になることを確かめました
- Verus：`release/0.2026.11.02.8bf68ee`（`cargo xtask verus-install`）
- `.eykp`（M3）はまだないので、証明が要る関数は `verify` で `failed` になります。これは「M3 待ち」として記録し、バグにはしていません。

## 書いたもの

| プロジェクト | 設計ノート §1.2 の用途 | 内容 |
| --- | --- | --- |
| [dlist](dlist/src/main.eyk) | 双方向リスト | 付録 A.1 を現行仕様の形で書いたもの。`push_back`、`peek_front`、O(1) の中間削除 `remove` |
| [tree](tree/src/main.eyk) | 親ポインタ付きの木 | 各ノードが親を指し、子は「最初の子と次の兄弟」でつなぐ。`add_child`、`parent`、親をたどる `depth`、兄弟をたどる `children_sum` |
| [graph](graph/src/main.eyk) | グラフのアリーナ（循環参照） | ノードを `Region` に置き、辺は `Vec<Ptr<GNode>>`。自己ループと循環（a → b → a）を含む。`add_edge`、`neighbors_sum`、`mark_neighbors` |
| [rccell](rccell/src/main.eyk) | `Rc<RefCell<T>>` の置き換え | 口座のハンドルを `Copy` な `Ptr<Account>` にして共有し、権限は `Bank` の `Region` が持つ。二つの口座を同時に可変で借りる `transfer` は、`RefCell` なら実行時の panic になる同じ口座の二重の借用を、`from != to` の静的な義務にする |

どのプロジェクトも `src/main.eyk` の `main` は `unchecked fn` です。検証される関数の本体の `println!` は Verus が拒否するためです（#54）。

## 結果

`ebykn build` が止まったプロジェクトは、`build/verus/` で Verus なしの `cargo build` を打って実行しました（現行仕様 §1.3）。

| プロジェクト | `check` | `verify` | `build` | 実行 |
| --- | --- | --- | --- | --- |
| dlist | 通る | 失敗：`failed` 2、`unknown` 5、`unchecked` 1 | 失敗（#80） | 期待どおり |
| tree | 通る | 失敗：Verus が検証の前に止まる（#81） | 失敗（#80） | 期待どおり |
| graph | 通る | 失敗：`verified` 6、`failed` 3、`unchecked` 1 | 失敗（#80） | 期待どおり |
| rccell | 通る | 通る：`verified` 7、`unchecked` 1 | 通る | 期待どおり |

### dlist

```text
unknown dlist::DList::view
unknown dlist::DList::invariant
unknown dlist::DList::has
unknown dlist::DList::index_of_node
unknown dlist::DList::new
failed dlist::DList::push_back
unknown dlist::DList::peek_front
failed dlist::DList::remove
unchecked dlist::main
results 2 verified, 2 errors
```

- `push_back` は事後条件の 3 項（`final(self)@ == old(self)@.push(v)`、`final(self).has(h)`、不変条件）が成り立たないと報告されます。`remove` は、`prev` と `next` の権限を領域から取り出す前提（`old(self).contains(loc)`）が 2 つ成り立たないと報告されます。どちらも M3 待ちです。
- 失敗した 2 つ以外が `unknown` なのは、一つの関数に Verus のエラーが 2 つ以上あると、Verus の通した関数まで `unknown` になるバグです（#79）。Verus 自身は `new` と `peek_front` を通しています（`2 verified`）。
- 実行（`cargo build` の後の `target/debug/dlist`）：

```text
removed 2
front Some(1)
then 1 3
empty None
```

### tree

```text
error: loop must have a `decreases` clause
failed tree::Tree::depth
results not verified: Verus stopped before verification (1 errors)
```

- 親をたどる `loop` と兄弟をたどる `while` に `decreases` を書いていないので、Verus が検証の前に止まります。設計ノート §4.3 は実行関数の `decreases` を任意と決めているので、バグです（#81）。
- 生成した `build/verus/src/main.rs` の `depth` と `children_sum` に手で `#[verifier::exec_allows_no_decreases_clause]` を付けると、`8 verified, 1 errors` まで進みます。残る 1 つは `add_child` の事後条件の不変条件で、M3 待ちです。初めは `depth` を `d = d + 1` と書いていて、算術のあふれを報告されたので、`wrapping_add` にしました。
- 根を入れた木を作る `new` で、最初は `put t@nodes[r];`（ローカル変数の構造体の領域）と書き、行動につながらない構文エラーになりました（#84）。領域変数を先に作って、構造体のリテラルで渡す形にしました。
- 「`b` の親が根か」を `t.parent(b) == Some(r)` と書くと、`Ptr` が `PartialEq` を持たないので生成コードの型エラーになります。`addr()` と `match` で比べました（#85）。
- 実行：

```text
depth(d) = 3
children_sum(root) = 5
parent(b) is root: true
value(c) = 4
```

### graph

```text
verified graph::Graph::has
verified graph::Graph::invariant
verified graph::Graph::new
failed graph::Graph::add_node
failed graph::Graph::add_edge
verified graph::Graph::out_degree
verified graph::Graph::neighbors_sum
failed graph::Graph::mark_neighbors
verified graph::Graph::is_visited
unchecked graph::main
results 6 verified, 3 errors
```

- 読むだけの操作（`neighbors_sum` は辺をたどって別のノードを借りるループ）は verified です。
- `add_node` と `add_edge` は事後条件の不変条件、`mark_neighbors` はループの終わりの不変条件が成り立たないと報告されます。領域の一つのノードを変えたあとも、ほかのノードの辺が変わらないことを示す証明が要り、M3 待ちです。
- 辺を足す `borrow mut self@nodes[a] { (*a).out.push(b); }` は、書き下しが共有の `read` にするので、生成コードがコンパイルできません（#82）。フィールドを複製して足し、代入で書き戻す形で回避しました（O(1) が O(出次数) になる）。
- 実行：

```text
out_degree(b) = 2
neighbors_sum(b) = 101
neighbors_sum(c) = 100
visited a b c = true false true
```

### rccell

```text
verified rccell::Bank::has
verified rccell::Bank::bal
verified rccell::Bank::new
verified rccell::Bank::open
verified rccell::Bank::balance
verified rccell::Bank::transfer
verified rccell::demo
unchecked rccell::main
results 5 verified, 0 errors
```

- `ebykn build` が通り、`build/verus/target/debug/rccell` が `alice = 70, bob = 35` を出します。`demo` の `ensures r.0 == 70, r.1 == 35` も verified です。
- 誤りが止まることも確かめました。`open` の `put self@accts[a];` を消すと、`check` が `E0505`（権限 `a@` が残っている）で止まります。`transfer` の `requires from != to` を消すと、`verify` で `transfer` が `failed` になります（内側の `borrow mut` で、同じ口座の権限を二度取り出せない）。
- 初めは、仕様 §11 の `bank` と同じく `Rc<Cell<Account>>` と `Region<Cell<Account>>` で書きました。関数が受け取る `&Cell<Account>` で領域を添字にすると、生成コードが型エラーになります（#83）。`Cell` は `Copy` でなく、添字は識別子に限られる（E0502）ので回避できず、ハンドルを `Ptr<Account>` に変えました。

## 見つけた Issue

| Issue | 種類 | 優先度 | 内容 | 再現 |
| --- | --- | --- | --- | --- |
| #78 | bug | P1 | 生成する `build/verus/Cargo.toml` に `[workspace]` がなく、Cargo のワークスペースの下のプロジェクトで `check` が落ちる | 本文の手順 |
| #79 | bug | P1 | 一つの関数に Verus のエラーが 2 つ以上あると、`verify` が Verus の通したほかの関数まで `unknown` と出す | [two_errors](repro/two_errors/src/lib.eyk) |
| #80 | bug | P1 | `ebykn build`（`--verified` なし）が検証の失敗で止まり、ビルドできない | [two_errors](repro/two_errors/src/lib.eyk) |
| #81 | bug | P1 | 実行関数のループと再帰に `decreases` を書かないと `verify` が失敗する（設計ノート §4.3 では任意） | [loop_no_decreases](repro/loop_no_decreases/src/lib.eyk) |
| #82 | bug | P1 | 権限の指す値のフィールドへの `&mut self` のメソッド呼び出しが `read` に書き下され、コンパイルできない | [field_method_mut](repro/field_method_mut/src/lib.eyk) |
| #83 | bug | P1 | 参照の引数（`&Cell<T>` / `&Ptr<T>`）で領域を添字にすると、生成コードが型エラーになる | [ref_index](repro/ref_index/src/lib.eyk) |
| #84 | bug | P1 | `put t@nodes[r]` が「expected `;`」の構文エラーになり、書き方が分からない | [local_region_put](repro/local_region_put/src/lib.eyk) |
| #85 | usability（needs review） | P2 | `Ptr<T>` と `Option<Ptr<T>>` を実行時に `==` で比べられず、`addr()` と `match` で書く | 本文のコード |

## M3 以降の機能待ち

| 待つもの | 該当 | 中身 |
| --- | --- | --- |
| M3（`.eykp` の証明） | dlist の `push_back` / `remove`、tree の `add_child`、graph の `add_node` / `add_edge` / `mark_neighbors` | 領域に権限を足す・ノードを変えるたびに、`forall` の不変条件が保たれることを示す証明（付録 A.2 の `push_keeps_invariant` の類）。dlist の `remove` は、`order` に重複がないことからの添字のずれも要る |
| M4（`Cell` の領域、`bank`） | rccell を `Rc<Cell<Account>>` で書く形 | #83 が直ったあと、M4 の `bank` の golden で確かめる |
| M6（診断の写し戻し） | 全プロジェクト | Verus の診断が生成コードの位置（`build/verus/src/main.rs`）を指す。たとえば `pub` の関数の契約で非公開の消去フィールドを読むと `field expression for an opaque datatype` になり、`.eyk` のどこかが分からない |

§1.2 の残りの用途（glTF の検証と部分集合型 `FiniteMesh`、rayon の性質ファイル）は、部分集合型（M4）と性質ファイル（M5）がないので、この回では書いていません。

## Issue にしなかったこと

- `pub` の関数の契約から非公開の消去フィールドを読めず、`closed spec fn`（rccell の `has` / `bal`）を通す必要がある。Verus と同じ制約で、付録 A.1 の注記と #25 で決まった書き方です。
- `build/verus/` を Verus なしの `cargo build` でビルドすると、消去したフィールドの `dead_code` と、生成コードの `unnecessary braces around block return value` の警告が出る。前者は `OPEN.md` の「M0 の実測」に記録済みで、どちらも動作は変わりません。
- 自分の書き誤り：補助の `spec fn` に `pub` を付け忘れた（`pub` の関数の `ensures` から非公開の関数は呼べない）。中置の `and` を書いた（`and { .. }` はブロックの形だけで、中置は `&&`）。どちらも診断から直せました。

## examples/ に足したもの

rccell は `check` / `verify` / `build` と実行がすべて通るので、[examples/shared-accounts](../../examples/shared-accounts/) として足しました。
