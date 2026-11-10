// probe: verus! の中の exec のない pub static は、open を付けられて拒否される（#163。書き下しが exec を付ける理由）
// expect: rejected
// expect-output: only `spec` functions can be marked `open`, `closed`, or `uninterp`
use vstd::prelude::*;

verus! {

pub static B: u64 = 2;

} // verus!
