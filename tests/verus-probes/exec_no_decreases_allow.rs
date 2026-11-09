// probe: #[verifier::allow(decreases_when_exec_allows_no_decreases_clause)] を並べて付けると、関数の decreases の警告が消え、書いた decreases は確かめたまま（#81。書き下しは本体を検証する実行関数に両方を付ける）
// expect: errors 1 1
// expect-output: could not prove termination
// expect-output-count: 0 do not guarantee termination
use vstd::prelude::*;

verus! {

#[verifier::exec_allows_no_decreases_clause]
#[verifier::allow(decreases_when_exec_allows_no_decreases_clause)]
fn count(n: u64) -> (r: u64)
    ensures
        r == n,
    decreases n,
{
    if n == 0 {
        0
    } else {
        count(n - 1) + 1
    }
}

#[verifier::exec_allows_no_decreases_clause]
#[verifier::allow(decreases_when_exec_allows_no_decreases_clause)]
fn wrong_rec(n: u64) -> u64
    decreases n,
{
    if n == 0 {
        0
    } else {
        wrong_rec(n)
    }
}

} // verus!
