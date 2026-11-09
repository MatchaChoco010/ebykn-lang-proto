// probe: 本体のある trait の関数の名前を Rust の予約語の r# で書き（fn r#dyn(&self) { .. }）、impl で上書きして呼ぶと、Verus が AIR の内部エラーで落ちる（#147）。予約語でない r#foo は通る（raw_name_items.rs）
// verus-args: --edition=2024
// expect: rejected
// expect-output: generated ill-typed AIR code
// expect-output: dyn.
use vstd::prelude::*;

verus! {

pub trait T {
    fn r#dyn(&self) -> u64 {
        1
    }
}

pub struct S {
    pub x: u64,
}

impl T for S {
    fn r#dyn(&self) -> u64 {
        2
    }
}

pub fn call(s: S) -> u64 {
    s.r#dyn()
}

} // verus!
