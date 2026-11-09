// probe: 純粋関数の式をそのまま `fn` と `spec fn` に写すと、spec 側では u32 の算術が int になり型が合わない（`as` が要る）
// expect: rejected
// expect-output: mismatched types
use vstd::prelude::*;

verus! {

#[verifier::when_used_as_spec(__eyk_spec_double)]
pub fn double(x: u32) -> (r: u32)
    requires
        x < 1000,
    ensures
        r == __eyk_spec_double(x),
{
    x * 2
}

pub open spec fn __eyk_spec_double(x: u32) -> u32 {
    x * 2
}

} // verus!
