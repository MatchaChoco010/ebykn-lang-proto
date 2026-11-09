# golden のケース `stack`

付録 B の `stack` の `.eyk` だけを書き下した、M2 の書き下しの期待です。`expected/verus/` は `ebykn` の出力と
トークン列で比べます（`crates/ebykn/tests/golden.rs`）。書き下しが変わったら `cargo xtask golden-bless stack` で
作り直します。期待のファイルにはコメントを書かず、説明はこの README に書きます。

- `expected/verus/src/lib.rs` は、M0 の手書きの目標（2bca455 の同じファイル）から、証明として足した部分（M3 の
  `.eykp` で足す部分）を除いたものです。そのため `push` / `pop` は検証を通りません（`expected/status.txt`）。
- M0 の手書きの目標から、ほかに変えたのは、先頭の `use`（#16）、`and { }` の括弧、範囲の束縛子の型
  （`forall|i: int|`）です。表の形どおりでは通らず形を変えた箇所は `OPEN.md` に記録してあります。
- `src/lib.eyk` は、付録 B の不変条件の `self.head == self@order.first()` を、vstd の `first()`（要素を返す）の
  意味で正しい `self.head == self@order.get(0)` に書き直したものです（#11 の決定：案 B）。
