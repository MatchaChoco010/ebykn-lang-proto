// probe: 固定したリリース版の Verus は integer_ring を使えない（Singular 付きでビルドした Verus が要る）
// expect: rejected
// expect-output: please cargo build with `--features singular` to use `integer_ring` attribute
use vstd::prelude::*;

verus! {

proof fn fn_integer_ring(a: int, b: int)
    by (integer_ring)
    requires
        b != 0,
    ensures
        (a * b) % b == 0,
{
}

} // verus!
