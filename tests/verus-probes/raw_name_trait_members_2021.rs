// probe: edition 2021 では gen は予約語でないので、rustc が r#gen の r# を外して Verus に渡し、trait の関連型・関連定数・本体のある関数の r#gen は、impl で与えて使っても通る（関連型は X::r#gen、関数は上書きして呼ぶ。#147）。Ebykn が Ebykn.toml の edition が 2024 より前ならこれらの r#gen を E0104 にしない根拠
// verus-args: --edition=2021
// expect: verified 5
use vstd::prelude::*;

verus! {

pub trait A {
    type r#gen;
}

pub trait B {
    const r#gen: u64;
}

pub trait C {
    fn r#gen(&self) -> u64 {
        1
    }
}

pub struct S {
    pub x: u64,
}

impl A for S {
    type r#gen = u64;
}

impl B for S {
    const r#gen: u64 = 1;
}

impl C for S {
    fn r#gen(&self) -> u64 {
        2
    }
}

pub fn call(s: S) -> u64 {
    s.r#gen()
}

pub fn assoc<X: A>(x: X::r#gen) -> X::r#gen {
    x
}

} // verus!
