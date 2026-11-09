// probe: async fn の引数のパターン Tracked(x) を Verus が読めず、内部エラーで止まる（#101）。uses を持つ async fn を E0003 にする前提
// expect: rejected
// expect-output: ill-formed `unwrap_parameter` header
use vstd::prelude::*;

verus! {

async fn f(x: u64, Tracked(t): Tracked<&mut u64>) {
}

} // verus!
