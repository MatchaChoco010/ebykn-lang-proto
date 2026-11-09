//! `Seq` の拡張（spec）。vstd にあるもの（`index_of`、`no_duplicates`、`remove`）は重複させない。

use vstd::prelude::*;

verus! {

pub trait SeqExt<A>: Sized {
    /// `0 <= i < len` なら `Some(self[i])`、それ以外は `None`。
    spec fn get(self, i: int) -> Option<A>;

    /// 先頭の要素。空なら `None`（vstd の `first()` は空でないことを前提に `A` を返す）。
    spec fn first_opt(self) -> Option<A>;

    /// 末尾の要素。空なら `None`。
    spec fn last_opt(self) -> Option<A>;
}

impl<A> SeqExt<A> for Seq<A> {
    open spec fn get(self, i: int) -> Option<A> {
        if 0 <= i < self.len() {
            Some(self[i])
        } else {
            None
        }
    }

    open spec fn first_opt(self) -> Option<A> {
        self.get(0)
    }

    open spec fn last_opt(self) -> Option<A> {
        self.get(self.len() - 1)
    }
}

} // verus!
