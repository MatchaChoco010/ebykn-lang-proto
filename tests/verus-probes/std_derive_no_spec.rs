// probe: std の derive のうち PartialEq と Default が作る impl には仕様がなく、verified なコードはその振る舞いに頼れない（#49 のレビューの S1。func-details に出るのは derive した Clone だけ）
// expect: errors 1 2
// expect-output: postcondition not satisfied
use vstd::prelude::*;

verus! {

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct S {
    pub x: u32,
}

// derive した `==` の結果は、欄の比較と結びつかない。
pub fn eq(a: S, b: S) -> (r: bool)
    ensures
        r == (a.x == b.x),
{
    a == b
}

// derive した `default()` の値も分からない。
pub fn zero() -> (r: S)
    ensures
        r.x == 0,
{
    S::default()
}

} // verus!
