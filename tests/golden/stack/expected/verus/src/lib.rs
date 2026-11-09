use vstd::prelude::*;
use ebykn_prelude::*;
verus! {

// 付録 B の `stack` を、仕様 §6.4・§7.2〜§7.5 の書き下しの形で手で書いたもの。M2 の目標の出力。
// `// +` の付いた行は証明として足した部分（M3 以降なら `.eykp` に書く部分）。
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
        (forall|p| self.order@.contains(p) ==> self.nodes@.contains(p)) && (self.head
            == self.order@.first_opt()) && (forall|i|
            0 <= i < self.order@.len() ==> self.nodes@.index(self.order@[i]).next
                == self.order@.get(i + 1))
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
        proof {  // +
            let o = old(self).order@;  // +
            assert(!old(self).nodes@.contains(n));  // +
            assert(self.nodes@.contains(n) && self.nodes@.index(n).next == old(self).head);  // +
            assert(forall|q| q != n ==> (self.nodes@.contains(q) == old(self).nodes@.contains(q)));  // +
            assert(forall|q| q != n ==> self.nodes@.index(q) == old(self).nodes@.index(q));  // +
            assert(self.order@.len() == o.len() + 1 && self.order@[0] == n);  // +
            assert forall|i| 0 < i < self.order@.len() implies self.order@[i] == o[i - 1] && o[i - 1] != n by {  // +
                assert(old(self).nodes@.contains(o[i - 1]));  // +
            }  // +
            assert forall|p| self.order@.contains(p) implies self.nodes@.contains(p) by {  // +
                let k = choose|k: int| 0 <= k < self.order@.len() && self.order@[k] == p;  // +
                if k > 0 {  // +
                    assert(o[k - 1] == p);  // +
                }  // +
            }  // +
            assert forall|i| 0 <= i < self.order@.len() implies self.nodes@.index(self.order@[i]).next == self.order@.get(i + 1) by {  // +
                if i > 0 {  // +
                    assert(old(self).nodes@.index(o[i - 1]).next == o.get(i));  // +
                }  // +
            }  // +
        }  // +
    }

    pub fn pop(&mut self) -> Option<T>
        requires
            old(self).invariant(),
        ensures
            final(self).invariant(),
    {
        proof {  // +
            self.lemma_no_duplicates();  // +
        }  // +
        let h = self.head?;
        let tracked __eyk_perm_h = self.nodes.borrow_mut().remove(h);
        let node = h.free(Tracked(__eyk_perm_h));
        self.head = node.next;
        proof {
            self.order = Ghost(self.order@.drop_first());
        }
        proof {  // +
            let o = old(self).order@;  // +
            assert(o[0] == h);  // +
            assert forall|i| 0 <= i < self.order@.len() implies self.order@[i] == o[i + 1] && o[i + 1] != h by {  // +
                assert(o[0] != o[i + 1]);  // +
            }  // +
            assert forall|p| self.order@.contains(p) implies self.nodes@.contains(p) by {  // +
                let k = choose|k: int| 0 <= k < self.order@.len() && self.order@[k] == p;  // +
                assert(o[k + 1] == p);  // +
                assert(old(self).nodes@.contains(p));  // +
            }  // +
            assert forall|i| 0 <= i < self.order@.len() implies self.nodes@.index(self.order@[i]).next == self.order@.get(i + 1) by {  // +
                assert(old(self).nodes@.index(o[i + 1]).next == o.get(i + 2));  // +
            }  // +
        }  // +
        Some(node.val)
    }
}

// + `.eykp` だけの項目：不変条件から順序に重複がないことが出る。
impl<T> Stack<T> {  // +
    proof fn lemma_no_duplicates(&self)  // +
        requires  // +
            self.invariant(),  // +
        ensures  // +
            self.order@.no_duplicates(),  // +
    {  // +
        assert forall|i, j| 0 <= i < j < self.order@.len() implies self.order@[i] != self.order@[j] by {  // +
            if self.order@[i] == self.order@[j] {  // +
                self.lemma_shifted_equal(i, j);  // +
            }  // +
        }  // +
    }  // +

    // + `i < j` で `order[i] == order[j]` なら矛盾する（`next` をたどって末尾の `None` に行き着く）。
    proof fn lemma_shifted_equal(&self, i: int, j: int)  // +
        requires  // +
            self.invariant(),  // +
            0 <= i < j < self.order@.len(),  // +
            self.order@[i] == self.order@[j],  // +
        ensures  // +
            false,  // +
        decreases self.order@.len() - j,  // +
    {  // +
        assert(self.nodes@.index(self.order@[i]).next == self.order@.get(i + 1));  // +
        assert(self.nodes@.index(self.order@[j]).next == self.order@.get(j + 1));  // +
        if j + 1 < self.order@.len() {  // +
            self.lemma_shifted_equal(i + 1, j + 1);  // +
        }  // +
    }  // +
}  // +

} // verus!
