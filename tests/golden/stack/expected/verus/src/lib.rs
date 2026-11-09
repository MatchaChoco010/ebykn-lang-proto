#[allow(unused_imports)]
use vstd::prelude::{Seq, Set, Map, seq, set, map, View, int, nat, Tracked, Ghost, verus, SpecOrd as _, SpecNeg as _, SpecAdd as _, SpecSub as _, SpecMul as _, SpecEuclideanOrRealDiv as _, SpecEuclideanMod as _, SpecBitAnd as _, SpecBitOr as _, SpecBitXor as _, SpecShl as _, SpecShr as _};
#[cfg(verus_keep_ghost)]
#[allow(unused_imports)]
use vstd::prelude::old;
#[allow(unused_imports)]
use ebykn_prelude::*;
verus! {

// 付録 B の `stack` の `.eyk` だけを書き下した、M2 の書き下しの期待（`ebykn` の出力とトークン列で比べる）。
// M0 の手書きの目標（2bca455 の同じファイル）から、証明として足した部分（M3 の `.eykp` で足す部分）を
// 除いたもの。そのため `push` / `pop` は検証を通らない（`expected/status.txt`）。
// ほかに変えたのは、先頭の `use`（#16）、`and { }` の括弧、範囲の束縛子の型（`forall|i: int|`）。
// 表の形どおりでは通らず形を変えた箇所は `OPEN.md` に記録してある。

pub struct Node<T> {
    pub val: T,
    next: Option<Ptr<Node<T>>>,
}

pub struct Stack<T> {
    head: Option<Ptr<Node<T>>>,
    nodes: Tracked<Region<Ptr<Node<T>>>>,
    order: Ghost<Seq<Ptr<Node<T>>>>,
}

impl<T> Stack<T> {
    pub closed spec fn invariant(&self) -> bool {
        (((forall|p| self.order@.contains(p) ==> self.nodes@.contains(p))) && (self.head
            == self.order@.first_opt()) && ((forall|i: int|
            0 <= i < self.order@.len() ==> self.nodes@.index(self.order@[i]).next
                == self.order@.get(i + 1))))
    }
}

impl<T> Stack<T> {
    pub fn new() -> (__eyk_r: Self)
        ensures
            __eyk_r.invariant(),
    {
        Stack { head: None, nodes: Tracked(Region::empty()), order: Ghost(Seq::empty()) }
    }

    pub fn push(&mut self, v: T)
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
    {
        let (n, Tracked(__eyk_perm_n)) = Ptr::new(Node { val: v, next: self.head });
        self.head = Some(n);
        proof {
            self.nodes.borrow_mut().insert(__eyk_perm_n);
        }
        proof {
            self.order = Ghost(seq![n] + self.order@);
        }
    }

    pub fn pop(&mut self) -> Option<T>
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
    {
        let h = self.head?;
        let tracked __eyk_perm_h = self.nodes.borrow_mut().remove(h);
        let node = h.free(Tracked(__eyk_perm_h));
        self.head = node.next;
        proof {
            self.order = Ghost(self.order@.drop_first());
        }
        Some(node.val)
    }
}



} // verus!
