// probe: Tracked を含む戻り値の async fn は、定義も、呼ぶ側の .await の分解（let (v, Tracked(t)) = f(..).await）も検証できる（#101。gives を持つ async fn を将来通すときの前提。今は .await の位置で §6.2 が権限を追えないので E0003）
// expect: verified 2
use vstd::prelude::*;

verus! {

async fn f(x: u64) -> (r: (u64, Tracked<u64>))
    ensures
        r.0 == x,
        r.1@ == x,
{
    (x, Tracked(x))
}

async fn g() {
    let (v, Tracked(t)) = f(3).await;
    proof {
        assert(t == 3);
    }
    assert(v == 3);
}

} // verus!
