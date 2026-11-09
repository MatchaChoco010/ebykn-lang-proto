// probe: Tracked を含む戻り値の async fn は検証できる（#101。gives を持つ async fn は通す前提）
// expect: verified 1
use vstd::prelude::*;

verus! {

async fn f(x: u64) -> (r: (u64, Tracked<u64>))
    ensures
        r.0 == x,
{
    (x, Tracked(0))
}

} // verus!
