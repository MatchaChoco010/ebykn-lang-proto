// probe: --output-json は失敗した関数の名前を JSON に含むか（OPEN-7）
// verus-args: --output-json
// expect: errors 1 1
// expect-output: "errors": 1
use vstd::prelude::*;

verus! {

fn ok() {
    assert(1 + 1 == 2);
}

fn bad() {
    assert(1 + 1 == 3);
}

} // verus!
