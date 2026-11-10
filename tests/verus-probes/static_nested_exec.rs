// probe: 関数の本体の中の exec static は、Verus の構文解析が文の位置で読めない（#163。E0003 にする理由）
// expect: rejected
// expect-output: unexpected token, expected `;`
use vstd::prelude::*;

verus! {

fn f() -> u64 {
    exec static INNER: u64 = 1;
    INNER
}

} // verus!
