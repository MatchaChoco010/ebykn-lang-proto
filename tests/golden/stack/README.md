# golden のケース `stack`

付録 B の `stack` の `.eyk` だけを書き下すケースです。

- `expected/verus/` は、`ebykn` の書き下しの生成物そのものです。書き下しが変わったら `cargo xtask golden-bless stack` で作り直します。期待のファイルにはコメントを書かず、説明はこの README に書きます。
- 証明（M3 の `.eykp`）がないので、`push` と `pop` は検証を通らず failed になります（`expected/status.txt`）。`stack` が verified になるのは M3 です（`docs/current-spec.md` §11）。
- `src/lib.eyk` の不変条件は、付録 B の `self.head == self@order.first()` を、vstd の `first()`（要素を返す）の意味で正しい `self.head == self@order.get(0)` に書き直したものです（#11 の決定：案 B）。
