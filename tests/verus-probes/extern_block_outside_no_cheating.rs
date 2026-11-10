// probe: verus! の外の、verifier::external を付けた extern ブロックは --no-cheating でも拒否されない（#169。verify の --no-cheating の再実行で、許す拒否を足さなくてよい）
// expect: verified 1
// verus-args: --no-cheating
use vstd::prelude::*;

verus! {

} // verus!

// Verus なしのビルドでは消える（書き下しの形）。
#[cfg_attr(verus_keep_ghost, verifier::external)]
unsafe extern "C" {
    pub fn abs(x: i32) -> i32;
}

verus! {

pub fn id(x: u64) -> (r: u64)
    ensures
        r == x,
{
    x
}

} // verus!
