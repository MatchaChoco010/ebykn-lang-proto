// probe: 付録 A.3 の形どおり（let tracked に mut がない、insert を proof の外で呼ぶ）では通らない
// expect: rejected
// expect-output: cannot call function `vstd::map::impl&%0::tracked_insert` with mode proof
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

pub struct Node {
    pub next: u64,
}

pub struct List {
    pub m: Tracked<Map<PPtr<Node>, PointsTo<Node>>>,
}

impl List {
    pub fn set_next(&mut self, t: PPtr<Node>, v: u64)
        requires
            old(self).m@.dom().contains(t),
            old(self).m@[t].pptr() == t,
            old(self).m@[t].is_init(),
    {
        let tracked mut perm_t = self.m.borrow_mut().tracked_remove(t);
        t.borrow_mut(Tracked(&mut perm_t)).next = v;
        self.m.borrow_mut().tracked_insert(t, perm_t);
    }
}

} // verus!
