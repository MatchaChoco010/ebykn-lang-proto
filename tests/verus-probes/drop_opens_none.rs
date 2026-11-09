// probe: ユーザー定義の Drop（OPEN-3）。opens_invariants none と no_unwind を付ければ本体を検証できる。drop は requires を持てず、暗黙の drop の呼び出しで事後条件は使えない
// expect: verified 2
use vstd::prelude::*;

verus! {

pub struct Guard {
    pub a: u64,
}

impl Drop for Guard {
    fn drop(&mut self)
        opens_invariants none
        no_unwind
    {
        self.a = 0;
    }
}

fn scope_end() {
    let g = Guard { a: 1 };
    assert(g.a == 1);
}

} // verus!
