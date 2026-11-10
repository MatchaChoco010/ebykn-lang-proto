// probe: exec static の初期化の式の呼び出しは、要求を検査される（#163。初期化の式は検証の対象）
// expect: errors 1 1
// expect-output: error: precondition not satisfied
use vstd::prelude::*;

verus! {

const fn mk(x: u64) -> (r: u64)
    requires
        x < 10,
{
    x + 1
}

exec static A: u64 = mk(20);

} // verus!
