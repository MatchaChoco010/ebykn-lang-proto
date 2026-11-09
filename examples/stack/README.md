# stack：付録 B の stack

仕様の付録 B の `Stack<T>` です。`owns`（権限の領域）と `spec`（論理の状態）を持つ構造体と、その型不変条件（`impl Invariant`）を書けることを確かめられます。

いまは `push` と `pop` が verified になりません。理由は下の「push と pop が verified にならない理由」に書きました。

## 中身

[`src/lib.eyk`](src/lib.eyk) は `tests/golden/stack/` と同じものです。

- `Node<T>`：値と、次の節点への `Option<Ptr<Node<T>>>`。
- `Stack<T>`：先頭の節点 `head`、節点の権限の領域 `owns { nodes }`、先頭からの並び `spec { order }`。
- 不変条件：`order` のすべての節点の権限が `nodes` にあり、`head` が `order` の先頭で、各節点の `next` が `order` の次を指す。
- `new` / `push` / `pop`：`put` / `take` で領域に権限を出し入れし、`self@order` を更新する。

付録 B との違いは、不変条件の `self@order.first()` を `self@order.get(0)` と書いた 1 か所だけです。vstd の `first()` は `Option` を返さないためです（#11、[現行仕様](../../docs/current-spec.md)の「§7.1 `Seq` の `first()` / `last()` は vstd の意味」）。

## 試せる機能

- `owns` / `spec` のある構造体、`impl Invariant`、`put` / `take`、`Ptr::new` / `free`、`?`。
- `ebykn verify` が、項目ごとに `verified` と `failed` を分けて出し、失敗があれば終了コード 1 で終わること。

## 打つコマンドと期待する出力

```console
$ ebykn check
ebykn: check passed (lowered to `$PWD/build/verus`; the self-contained check §6.6 comes with `.eykp` in M3)
$ ebykn verify 2> /dev/null
verified stack::Stack::invariant
verified stack::Stack::new
failed stack::Stack::push
failed stack::Stack::pop
results 1 verified, 2 errors
$ echo $?
1
```

`2> /dev/null` で、Verus の診断（標準エラー）を隠して、状態の行（標準出力）だけを示しました。`ebykn verify` だけを打つと、`push` と `pop` の「postcondition not satisfied」が `build/verus/src/lib.rs` の位置付きで出ます。

## push と pop が verified にならない理由

どちらも、事後条件の不変条件（`final(self).invariant()`）を Verus が自動では示せません。不変条件が `closed` で、`forall` の展開と帰納法が要るためです。

- `push`：新しい節点 `n` が古い `nodes` にないことと、`order` の添字が 1 つずれることから、`forall` の 2 項を示す必要がある。
- `pop`：`order` に重複がないこと（`next` をたどると末尾の `None` に着くことからの帰納法）から、取り出した `h` が残りの `order` にないことと、添字のずれを示す必要がある。

この証明は `.eyk` には書けません（`.eyk` には証明の構文を書かない）。M3 の `.eykp`（証明ファイル）で足す予定です。経緯は[現行仕様](../../docs/current-spec.md)の「`stack` が verified になるのは M3」にあります。

## 試してみること

- `ebykn verify` だけを打って、Verus の診断を見る。
- `push` の `put self@nodes[n];` を消して、権限の検査（§6.2）が何を報告するかを見る。
- `self@order` を `self.order` と書き換えて、消去フィールドを `.` で書いたときの `E0404` を見る（[現行仕様](../../docs/current-spec.md)の「§5.4 消去フィールドは `@` でだけ書く」）。
