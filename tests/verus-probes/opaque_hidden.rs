// probe: #[verifier::opaque] の関数は reveal しないと中身が見えない
// expect: errors 0 1
// expect-output: assertion failed
use vstd::prelude::*;

verus! {

#[verifier::opaque]
pub open spec fn secret(x: int) -> int {
    x + 1
}

proof fn without_reveal() {
    assert(secret(1) == 2);
}

} // verus!
