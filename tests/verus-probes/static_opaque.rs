// probe: ensures のない exec static の値は、検証からは見えない（#163。初期化の式が 1 でも、読んだ値に契約が立たず、+ 1 のあふれも示せない）
// expect: errors 1 2
// expect-output-count: 1 error: postcondition not satisfied
// expect-output-count: 1 error: possible arithmetic underflow/overflow
use vstd::prelude::*;

verus! {

exec static A: u64 = 1;

fn read() -> (r: u64)
    ensures
        r == 1,
{
    A
}

fn incr() -> u64 {
    A + 1
}

} // verus!
