// probe: verus! の中の exec のない pub でない static は、exec と ensures を求められて拒否される（#163。書き下しが exec を付ける理由。pub の形は static_pub_without_exec.rs）
// expect: rejected
// expect-output: explicitly mark the `static` item as `exec` and use an `ensures` clause
use vstd::prelude::*;

verus! {

static A: u64 = 1;

} // verus!
