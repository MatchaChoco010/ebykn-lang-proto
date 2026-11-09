// probe: exec_allows_no_decreases_clause は --no-cheating でも拒まれない（#81。verify の照合に許す拒否を足さなくてよい）
// expect: verified 2
// verus-args: --no-cheating
use vstd::prelude::*;

verus! {

#[verifier::exec_allows_no_decreases_clause]
fn halve_to_zero(x: u64) -> (r: u64)
    ensures
        r == 0,
{
    let mut y = x;
    while y > 0 {
        y = y / 2;
    }
    y
}

} // verus!
