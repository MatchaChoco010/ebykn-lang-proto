// probe: 関数単位の by(m) を ensures の後（仕様 §5.2 の正規順の位置）に置くと構文エラー
// expect: rejected
// expect-output: cannot find function `by` in this scope
use vstd::prelude::*;

verus! {

proof fn fn_bit_vector(x: u64)
    ensures
        x | 0 == x,
    by (bit_vector)
{
}

} // verus!
