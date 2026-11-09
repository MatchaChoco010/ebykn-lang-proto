// probe: async fn の引数のパターン Tracked(x) を Verus が読めず、内部エラーで止まる（#101）。&mut・&・所有（uses p@ の書き下し）のどれも同じ。uses を持つ async fn を E0003 にする前提
// expect: rejected
// expect-output-count: 3 ill-formed `unwrap_parameter` header
use vstd::prelude::*;

verus! {

async fn f(x: u64, Tracked(t): Tracked<&mut u64>) {
}

async fn g(x: u64, Tracked(t): Tracked<&u64>) {
}

async fn h(x: u64, Tracked(t): Tracked<u64>) {
}

} // verus!
