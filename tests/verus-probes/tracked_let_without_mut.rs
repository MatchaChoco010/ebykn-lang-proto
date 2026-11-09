// probe: mut のない let tracked で束縛した権限も &mut で渡せる（付録 A.3 の let tracked __eyk_perm_t の形のままでよい）
// expect: verified 1
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

pub struct Node {
    pub next: u64,
}

fn set(t: PPtr<Node>, Tracked(m): Tracked<&mut Map<PPtr<Node>, PointsTo<Node>>>)
    requires
        old(m).dom().contains(t),
        old(m)[t].pptr() == t,
        old(m)[t].is_init(),
{
    let tracked perm_t = m.tracked_remove(t);
    t.borrow_mut(Tracked(&mut perm_t)).next = 1;
    proof {
        m.tracked_insert(t, perm_t);
    }
}

} // verus!
