// probe: owns を持つ型（Tracked フィールドを持つ型）にユーザー定義の Drop を付けられるか（OPEN-3）
// expect: verified 1
use vstd::prelude::*;
use vstd::simple_pptr::*;

verus! {

pub struct Owner {
    pub p: PPtr<u64>,
    pub perm: Tracked<PointsTo<u64>>,
}

impl Drop for Owner {
    fn drop(&mut self)
        opens_invariants none
        no_unwind
    {
    }
}

} // verus!
