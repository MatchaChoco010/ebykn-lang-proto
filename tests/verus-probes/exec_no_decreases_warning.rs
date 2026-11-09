// probe: exec_allows_no_decreases_clause の下で関数に decreases を書くと、Verus は停止を保証しないと警告し、.eyk では書けない #[verifier::allow(..)] を案内する（#81）
// expect: verified 1
// expect-output: do not guarantee termination
// expect-output: decreases_when_exec_allows_no_decreases_clause
use vstd::prelude::*;

verus! {

#[verifier::exec_allows_no_decreases_clause]
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

} // verus!
