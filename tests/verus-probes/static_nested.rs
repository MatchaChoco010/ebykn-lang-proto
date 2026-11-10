// probe: 関数の本体の中の static は exec と ensures を求められる（#163。E0003 にする理由。exec を付けた形は static_nested_exec.rs）
// expect: rejected
// expect-output: explicitly mark the `static` item as `exec` and use an `ensures` clause
use vstd::prelude::*;

verus! {

fn f() -> u64 {
    static INNER: u64 = 1;
    INNER
}

} // verus!
