// probe: 関数単位の by(compute) / by(compute_only) は使えない（compute は assert .. by だけ）
// expect: rejected
// expect-output: invalid prover
use vstd::prelude::*;

verus! {

proof fn fn_compute()
    by (compute)
    ensures
        7 * 6 == 42int,
{
}

} // verus!
