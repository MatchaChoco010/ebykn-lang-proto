// probe: 実行コードから Tracked::assume_new() / Ghost::assume_new() は呼べない（#16 のレビュアーの懸念は当たらない）
// expect: rejected
// expect-output: `verus_builtin::impl&%14::assume_new` is not supported
// expect-output: `verus_builtin::impl&%13::assume_new` is not supported
use vstd::prelude::*;

verus! {

fn f() -> (r: u64)
    ensures
        r == 0,
{
    let t: Tracked<u64> = Tracked::assume_new();
    let g: Ghost<u64> = Ghost::assume_new();
    1
}

} // verus!
