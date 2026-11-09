// probe: 浮動小数点の実行時の述語（f64::is_finite など）には vstd の仕様がなく、呼ぶと拒まれる
// expect: rejected
// expect-output: `core::f64::impl&%0::is_finite` is not supported
use vstd::prelude::*;

verus! {

fn exec_is_finite(x: f64) -> bool {
    x.is_finite()
}

} // verus!
