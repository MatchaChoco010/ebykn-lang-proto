// probe: verus! の中の static は exec static と書けば通る（#163）。pub・pub(crate)・非 pub、参照・配列・構造体・タプルの型、const fn と const を使う初期化の式、&FOO で 'static の参照を返すこと、要求を満たさない呼び出しを含まない初期化の式。exec static の数も verified に数える
// expect: verified 12
use vstd::prelude::*;

verus! {

pub exec static A: u64 = 1;

exec static B: &'static u64 = &A;

pub(crate) exec static S: &'static str = "hi";

exec static ARR: [u8; 3] = [1, 2, 3];

pub struct P {
    pub x: u64,
}

pub exec static D: P = P { x: 3 };

exec static T: (u64, bool) = (1, true);

const C: u64 = 5;

const fn mk(x: u64) -> (r: u64)
    requires
        x < 10,
    ensures
        r == x + 1,
{
    x + 1
}

exec static Q: u64 = mk(C);

fn addr() -> &'static u64 {
    &A
}

fn first() -> u8 {
    ARR[0]
}

fn field() -> u64 {
    D.x
}

} // verus!
