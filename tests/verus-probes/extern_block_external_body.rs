// probe: extern ブロックの関数に #[verifier::external_body] を付けると、Verus は空の仕様で信用し、検証する関数から unchecked の印なしに呼べてしまう（#169。書き下しが external_body を付けない理由。設計ノート A4）
// expect: verified 1
use vstd::prelude::*;

verus! {

unsafe extern "C" {
    #[verifier::external_body]
    pub fn abs(x: i32) -> i32;
}

pub fn call(x: i32) -> i32 {
    unsafe { abs(x) }
}

} // verus!
