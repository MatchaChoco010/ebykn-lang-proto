// probe: Drop に requires は付けられない（型の不変条件を drop の前提にできない）
// expect: rejected
// expect-output: `requires` clauses are not allowed on `Drop` implementations
use vstd::prelude::*;

verus! {

pub struct Guard {
    pub a: u64,
}

impl Drop for Guard {
    fn drop(&mut self)
        requires
            self.a > 0,
        opens_invariants none
        no_unwind
    {
        self.a = 0;
    }
}

} // verus!
