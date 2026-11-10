// probe: --output-json に --time を足すと、times-ms の function-breakdown に関数ごとの success が出る（#79）。ensures の 2 項が成り立たない two_ens は、エラーの見出しが 2 つで errors は 1、success は false。"success": false は verification-results にも出る
// verus-args: --output-json --time
// expect: errors 1 1
// expect-output: "errors": 1
// expect-output: "function-breakdown"
// expect-output-count: 2 error: postcondition not satisfied
// expect-output-count: 2 "success": false
// expect-output-count: 1 "success": true
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

fn two_ens(x: u32) -> (r: u32)
    ensures
        r == 1,
        r == 2,
{
    x
}

} // verus!
