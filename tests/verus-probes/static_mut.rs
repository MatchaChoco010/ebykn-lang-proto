// probe: static mut は exec の有無によらず拒否される（#163。E0003 にする理由）
// expect: rejected
// expect-output-count: 2 Verus does not support 'static mut'
use vstd::prelude::*;

verus! {

static mut A: u64 = 1;

exec static mut B: u64 = 2;

} // verus!
