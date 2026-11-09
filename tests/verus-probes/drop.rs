// probe: ユーザー定義の Drop（OPEN-3）。opens_invariants none を付けないと拒まれる
// expect: rejected
// expect-output: a `Drop` implementation must be marked `opens_invariants none`
use vstd::prelude::*;

verus! {

pub struct Guard {
    pub a: u64,
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.a = 0;
    }
}

} // verus!
