// probe: verus! の外の、verifier::external を付けた extern ブロックは Verus が見ず、中の関数（pub / 非 pub / safe fn）と static は external_body の本体から呼べる（#169。書き下しの形と、unchecked fn で包む形）
// expect: verified 1
use vstd::prelude::*;

verus! {

} // verus!

// Verus なしのビルドでは消える（書き下しの形）。
#[cfg_attr(verus_keep_ghost, verifier::external)]
unsafe extern "C" {
    pub fn abs(x: i32) -> i32;
    fn labs(x: i64) -> i64;
    pub safe fn llabs(x: i64) -> i64;
    pub static mut errno: i32;
}

verus! {

// `unchecked fn` の書き下し。
#[verifier::external_body]
pub fn wrap(x: i32) -> (r: i32)
    ensures
        r >= 0,
{
    unsafe { abs(x) + labs(x as i64) as i32 + llabs(x as i64) as i32 + errno }
}

// 検証する関数は、包んだ関数の契約に頼る。
pub fn user(x: i32) -> (r: i32)
    ensures
        r >= 0,
{
    wrap(x)
}

} // verus!
