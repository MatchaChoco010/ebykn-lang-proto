// probe: const・static・const fn・本体のない trait の関数の名前を r# で書くと、予約語でない r#foo でも、Verus の proc macro がその名前から作る記号（VERUS_UNERASED_PROXY__r#foo、VERUS_SPEC__r#foo）で panic する（#147）。verus! を位置ごとに分け、それぞれが panic することを数える
// verus-args: --edition=2024
// expect: rejected
// expect-output-count: 6 error: proc macro panicked
// expect-output-count: 1 `"VERUS_UNERASED_PROXY__r#foo"` is not a valid identifier
// expect-output-count: 1 `"VERUS_UNERASED_PROXY__r#type"` is not a valid identifier
// expect-output-count: 1 `"VERUS_UNERASED_PROXY__r#bar"` is not a valid identifier
// expect-output-count: 1 `"VERUS_UNERASED_PROXY__r#baz"` is not a valid identifier
// expect-output-count: 1 `"VERUS_UNERASED_PROXY__r#loc"` is not a valid identifier
// expect-output-count: 1 `"VERUS_SPEC__r#m"` is not a valid identifier
use vstd::prelude::*;

// モジュールの const
verus! {
pub const r#foo: u64 = 3;
} // verus!

// static（予約語の名前）
verus! {
pub static r#type: u64 = 3;
} // verus!

// const fn
verus! {
pub const fn r#bar() -> u64 { 3 }
} // verus!

// inherent な impl の関連定数
verus! {
pub struct S;
impl S { pub const r#baz: u64 = 1; }
} // verus!

// ブロックの中の const
verus! {
fn f() -> u64 { const r#loc: u64 = 1; r#loc }
} // verus!

// 本体のない trait の関数
verus! {
pub trait T { fn r#m(&self) -> u64; }
} // verus!
