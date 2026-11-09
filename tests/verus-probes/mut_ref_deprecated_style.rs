// probe: #[verifier::deprecated_postcondition_mut_ref_style(true)] を付けると、ensures の裸の &mut 引数を事後の値として読む旧来の形が通る
// expect: verified 1
use vstd::prelude::*;

verus! {

pub struct S {
    pub a: u64,
}

impl S {
    #[verifier::deprecated_postcondition_mut_ref_style(true)]
    pub fn bump(&mut self)
        requires
            old(self).a < 100,
        ensures
            self.a == old(self).a + 1,
    {
        self.a = self.a + 1;
    }
}

} // verus!
