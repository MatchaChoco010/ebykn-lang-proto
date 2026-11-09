// probe: --output-json で検証結果を JSON で標準出力に出す（OPEN-7）
// verus-args: --output-json
// expect: verified 2
// expect-output: "verification-results"
// expect-output: "verified": 2
use vstd::prelude::*;

verus! {

fn ok1() {
    assert(1 + 1 == 2);
}

fn ok2() {
    assert(2 + 2 == 4);
}

} // verus!
