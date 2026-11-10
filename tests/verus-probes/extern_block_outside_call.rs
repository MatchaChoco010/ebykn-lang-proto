// probe: verus! の外の、verifier::external を付けた extern ブロックの関数と static は、検証する関数から使えない。Verus が検証の前に拒否するので、黙って信用されることはない（#169。呼べるのは unchecked fn の本体だけ）
// expect: rejected
// expect-output: cannot use function `extern_block_outside_call::abs` which is ignored because it is either declared outside the verus! macro or it is marked as `external`
// expect-output: cannot use function `extern_block_outside_call::llabs` which is ignored
// expect-output: cannot find constant extern_block_outside_call::ERRNO
use vstd::prelude::*;

verus! {

} // verus!

// Verus なしのビルドでは消える（書き下しの形）。
#[cfg_attr(verus_keep_ghost, verifier::external)]
unsafe extern "C" {
    pub fn abs(x: i32) -> i32;
    pub safe fn llabs(x: i64) -> i64;
    pub static ERRNO: i32;
}

verus! {

pub fn call_unsafe(x: i32) -> i32 {
    unsafe { abs(x) }
}

pub fn call_safe(x: i64) -> i64 {
    llabs(x)
}

pub fn read_static() -> i32 {
    unsafe { ERRNO }
}

} // verus!
