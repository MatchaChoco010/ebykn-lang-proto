// probe: Verus は custom_inner_attributes と stmt_expr_attributes を有効にして動き、安定版の Rust が拒否する位置（式の属性、モジュールの中の内側のツールの属性）の属性も受け入れる（#49、PR #57 のレビューの B1）
// expect: verified 1
use vstd::prelude::*;

verus! {

mod m {
    #![rustfmt::skip]
}

pub fn f() -> (r: u32)
    ensures
        r == 1,
{
    let x: u32 = #[allow(unused_parens)] (1);
    x
}

} // verus!
