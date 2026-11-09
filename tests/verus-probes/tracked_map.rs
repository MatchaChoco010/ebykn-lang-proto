// probe: tracked Map の tracked_empty / tracked_insert / tracked_remove / tracked_borrow
// expect: verified 1
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

proof fn map_ops(tracked a: PointsTo<u64>, tracked b: PointsTo<u64>)
    requires
        a.pptr() != b.pptr(),
{
    let ka = a.pptr();
    let kb = b.pptr();
    let tracked mut m = Map::<PPtr<u64>, PointsTo<u64>>::tracked_empty();
    m.tracked_insert(ka, a);
    m.tracked_insert(kb, b);
    assert(m.dom().contains(ka) && m.dom().contains(kb));
    let tracked x = m.tracked_borrow(ka);
    assert(x.pptr() == ka);
    let tracked y = m.tracked_remove(kb);
    assert(y.pptr() == kb);
    assert(!m.dom().contains(kb));
}

} // verus!
