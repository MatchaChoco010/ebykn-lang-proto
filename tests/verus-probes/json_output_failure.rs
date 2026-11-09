// probe: --output-json は失敗の件数を出すが、どの関数が失敗したかは出さない（OPEN-7）。失敗した bad の名前は func-details に一度だけ出て、そこの failed_proof_notes も空
// verus-args: --output-json
// expect: errors 1 1
// expect-output: "errors": 1
// expect-output: "success": false
// expect-output: "json_output_failure::bad": {\n      "obligation_proof_notes": [],\n      "failed_proof_notes": []\n    }
// expect-output-count: 1 json_output_failure::bad
use vstd::prelude::*;

verus! {

fn ok() {
    assert(1 + 1 == 2);
}

fn bad() {
    assert(1 + 1 == 3);
}

} // verus!
