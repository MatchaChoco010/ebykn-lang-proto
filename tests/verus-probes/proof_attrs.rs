// probe: #[verifier::opaque] と reveal / hide、#[verifier::rlimit]、#[verifier::spinoff_prover]、#[verifier::memoize]
// expect: verified 4
use vstd::prelude::*;

verus! {

#[verifier::opaque]
pub open spec fn secret(x: int) -> int {
    x + 1
}

proof fn opaque_needs_reveal() {
    reveal(secret);
    assert(secret(1) == 2);
}

#[verifier::rlimit(20)]
#[verifier::spinoff_prover]
proof fn limited(x: int)
    ensures
        x + 0 == x,
{
}

#[verifier::memoize]
pub open spec fn fib(n: nat) -> nat
    decreases n,
{
    if n < 2 {
        n
    } else {
        fib((n - 1) as nat) + fib((n - 2) as nat)
    }
}

proof fn fib_compute() {
    assert(fib(20) == 6765) by (compute_only);
}

} // verus!
