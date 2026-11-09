// probe: assert .. by(integer_ring) は使えない（integer_ring は関数単位の by だけ）
// expect: rejected
// expect-output: unknown prover name for assert-by (supported provers: 'compute_only', 'compute', 'bit_vector', and 'nonlinear_arith')
use vstd::prelude::*;

verus! {

proof fn ir(x: int, y: int) {
    assert((x + y) * (x + y) == x * x + 2 * x * y + y * y) by (integer_ring);
}

} // verus!
