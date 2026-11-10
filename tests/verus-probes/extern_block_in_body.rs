// probe: 検証する関数の本体の中の extern ブロックは、#[verifier::external] を付けても Verus が拒否する（#169。関数の本体の中の extern ブロックを E0003 にする理由の一つ。verus! の外にも出せない）
// expect: rejected
// expect-output: Verus does not currently support internal item statements
use vstd::prelude::*;

verus! {

pub fn f(x: u64) -> u64 {
    #[verifier::external]
    unsafe extern "C" {
        fn foo(x: u64) -> u64;
    }
    x
}

} // verus!
