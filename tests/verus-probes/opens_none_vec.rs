// probe: vstd の Vec::new / push / len は opens_invariants any で、opens_invariants none の関数からは呼べない（すべての関数に opens_invariants none を出す案の影響。PR #76 のレビューの S1）
// expect: errors 0 1
// expect-output-count: 3 callee may open invariants that caller cannot
use vstd::prelude::*;

verus! {

fn uses_vec() -> (r: usize)
    opens_invariants none
{
    let mut v: Vec<u64> = Vec::new();
    v.push(1);
    v.len()
}

} // verus!
