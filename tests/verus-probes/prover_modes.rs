// probe: assert .. by(m) は bit_vector / nonlinear_arith / compute / compute_only を受け付ける。関数単位の by(m) は署名の直後（requires の前）に置き、bit_vector / nonlinear_arith を受け付ける。no_unwind
// expect: verified 7
use vstd::prelude::*;

verus! {

proof fn assert_by_modes(x: u32, a: int, b: int)
    requires
        a > 0,
        b > 0,
{
    assert(x & 0 == 0) by (bit_vector);
    assert(a * b > 0) by (nonlinear_arith)
        requires
            a > 0,
            b > 0,
    ;
    assert(7 * 6 == 42int) by (compute);
    assert(2 * 3 == 6int) by (compute_only);
}

proof fn fn_bit_vector(x: u64)
    by (bit_vector)
    ensures
        x | 0 == x,
{
}

proof fn fn_nonlinear(x: int, y: int)
    by (nonlinear_arith)
    requires
        x > 0,
        y > 0,
    ensures
        x * y > 0,
{
}

fn nu(x: u64) -> (r: u64)
    requires
        x < 100,
    ensures
        r == x + 1,
    no_unwind
{
    x + 1
}

fn nu_caller() {
    let _ = nu(1);
}

} // verus!
