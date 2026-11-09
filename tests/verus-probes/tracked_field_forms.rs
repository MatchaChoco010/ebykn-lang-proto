// probe: 付録 A.3 の Tracked フィールドの借用と Ghost 代入の形。実行コードの中で let tracked mut .. = self.f.borrow_mut().m(..) と let tracked .. = self.f.borrow().m(..)、proof { self.f.borrow_mut().m(..); }、論理では self.f@、Ghost の代入は proof { self.g = Ghost(..); } と実行コードの self.g = Ghost(..) のどちらも通る
// expect: verified 2
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

pub struct Node {
    pub next: u64,
}

pub struct List {
    pub m: Tracked<Map<PPtr<Node>, PointsTo<Node>>>,
    pub g: Ghost<Seq<PPtr<Node>>>,
}

impl List {
    pub fn set_next(&mut self, t: PPtr<Node>, v: u64)
        requires
            old(self).m@.dom().contains(t),
            old(self).m@[t].pptr() == t,
            old(self).m@[t].is_init(),
        ensures
            final(self).m@.dom().contains(t),
            final(self).m@[t].value().next == v,
            final(self).g@ == old(self).g@.push(t),
    {
        let tracked mut perm_t = self.m.borrow_mut().tracked_remove(t);
        t.borrow_mut(Tracked(&mut perm_t)).next = v;
        proof {
            self.m.borrow_mut().tracked_insert(t, perm_t);
        }
        proof {
            self.g = Ghost(self.g@.push(t));
        }
        self.g = Ghost(self.g@);
    }

    pub fn get_next(&self, t: PPtr<Node>) -> (r: u64)
        requires
            self.m@.dom().contains(t),
            self.m@[t].pptr() == t,
            self.m@[t].is_init(),
        ensures
            r == self.m@[t].value().next,
    {
        let tracked perm_t = self.m.borrow().tracked_borrow(t);
        t.borrow(Tracked(perm_t)).next
    }
}

} // verus!
