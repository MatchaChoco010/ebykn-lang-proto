// probe: opens_invariants none の drop からは、opens_invariants を書いていない（既定の any の）関数を呼べない（PR #76 のレビューの S1）。no_unwind を付けても同じ。Verus は呼ばれる側の署名・呼び出し元の opens_invariants・呼び出しの位置（at this call-site）の 3 つのラベルをどれも ^ で描き、--> はファイルの中で先に出るもの（ここでは呼ばれる側）を指す
// expect: errors 1 1
// expect-output: callee may open invariants that caller cannot
// expect-output: invariants opened by callee
// expect-output: at this call-site
use vstd::prelude::*;

verus! {

pub fn helper(x: u64) -> u64
    no_unwind
{
    x
}

pub struct C {
    pub a: u64,
}

impl Drop for C {
    fn drop(&mut self)
        opens_invariants none
        no_unwind
    {
        self.a = helper(self.a);
    }
}

} // verus!
