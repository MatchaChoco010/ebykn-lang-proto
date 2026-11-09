// probe: 仕様 §7.1 が挙げる vstd::cell::PCell は非推奨（pcell / pcell_maybe_uninit を使えという警告が出る）
// expect: verified 1
// expect-output: use `vstd::cell::pcell::PCell` or `vstd::cell::pcell_maybe_uninit::PCell` instead
use vstd::prelude::*;

verus! {

fn old_pcell() {
    let (c, Tracked(perm)) = vstd::cell::PCell::new(1u64);
    let _ = c.into_inner(Tracked(perm));
}

} // verus!
