// probe: verus! の中の extern ブロックの関数は、pub でもそうでなくても、Verus が検証の前に拒否する（#169。書き下しが extern ブロックを verus! の外に出す理由）
// expect: rejected
// expect-output: extern function cannot be checked
use vstd::prelude::*;

verus! {

unsafe extern "C" {
    pub fn foo(x: u64) -> u64;
}

} // verus!
