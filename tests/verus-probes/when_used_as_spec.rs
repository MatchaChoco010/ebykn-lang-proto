// probe: 仕様 §7.2 の純粋関数 `fn f(..) -> T = e;` の書き下し（when_used_as_spec と open spec fn の対）がそのまま通る
// expect: verified 3
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
    (x * 2) as u32
}

// 論理文脈の `double(x)` は __eyk_spec_double(x) に解決される。
proof fn uses_in_spec()
    ensures
        double(3) == 6,
{
}

fn uses_in_exec() {
    let y = double(4);
    assert(y == 8);
}

} // verus!
