// probe: bit_vector の assert の失敗は errors に数えるが、function-breakdown の success を false にしない（Verus の func_fails に入らない）。bv2 は bit_vector と ensures の 2 つで errors を 2 つ数え、success が false の関数は bv2 だけ（#79）。"success": false は verification-results にも出る
// verus-args: --output-json --time
// expect: errors 2 3
// expect-output: "errors": 3
// expect-output-count: 2 error: `bit_vector` assertion not satisfied
// expect-output-count: 2 "success": false
// expect-output-count: 2 "success": true
use vstd::prelude::*;

verus! {

fn ok(x: u32) -> (r: u32)
    requires
        x <= 10,
    ensures
        r == x + 1,
{
    x + 1
}

fn bv(x: u32) {
    assert(x & 1u32 == 0u32) by (bit_vector);
}

fn bv2(x: u32) -> (r: u32)
    ensures
        r == 7,
{
    assert(x & 1u32 == 0u32) by (bit_vector);
    x
}

} // verus!
