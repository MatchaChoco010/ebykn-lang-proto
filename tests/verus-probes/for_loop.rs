// probe: for .. in の invariant（OPEN-13）。範囲の for に invariant と decreases なしで書ける
// expect: verified 2
use vstd::prelude::*;

verus! {

fn sum_to(n: u64) -> (r: u64)
    requires
        n <= 1000,
    ensures
        r == n,
{
    let mut s: u64 = 0;
    for i in 0..n
        invariant
            s == i,
            n <= 1000,
    {
        s = s + 1;
    }
    s
}

} // verus!
