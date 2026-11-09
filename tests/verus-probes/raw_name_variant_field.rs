// probe: 列挙子のフィールドの名前を r# で書くと、予約語でない r#foo でも、宣言だけで Verus が no field で拒否する（#147）
// verus-args: --edition=2024
// expect: rejected
// expect-output: no field `r#foo` for this variant
use vstd::prelude::*;

verus! {

pub enum E { A { r#foo: u64 }, B }

} // verus!
