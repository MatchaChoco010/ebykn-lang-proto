//! `Seq` の拡張（spec）。vstd にあるもの（`index_of`、`no_duplicates`、`remove`、`first`、`last`）は
//! 重複させない。`first()` / `last()` は vstd の意味（空でないことを前提に要素を返す）に従う（#11 の
//! 決定：案 B）。`Option` が要るときは `get(0)` / `get(len - 1)` を使う。

use vstd::prelude::*;

verus! {

pub trait SeqExt<A>: Sized {
    /// `0 <= i < len` なら `Some(self[i])`、それ以外は `None`。
    spec fn get(self, i: int) -> Option<A>;
}

impl<A> SeqExt<A> for Seq<A> {
    open spec fn get(self, i: int) -> Option<A> {
        if 0 <= i < self.len() {
            Some(self[i])
        } else {
            None
        }
    }
}

} // verus!
