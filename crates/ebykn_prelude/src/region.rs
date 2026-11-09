//! `Region<L>`：権限の集合（tracked `Map` の包み）。`L` は `Ptr<T>` か `Cell<T>`。

use vstd::prelude::*;

use crate::cell::{Cell, CellId, CellPointsTo};
use crate::ptr::{PointsTo, Ptr};

verus! {

/// 領域の添字になる型。鍵の型、権限の型、権限が指す値の型を結び付ける。
pub trait Loc: Sized {
    /// 領域の鍵。`Ptr<T>` はポインタそのもの（等しさはアドレスの等しさ）、`Cell<T>` は `CellId`。
    type Key;

    type Perm;

    type Value;

    spec fn key(&self) -> Self::Key;

    spec fn perm_key(perm: Self::Perm) -> Self::Key;

    spec fn perm_init(perm: Self::Perm) -> bool;

    spec fn perm_value(perm: Self::Perm) -> Self::Value;

    /// 二つの権限の鍵は違う。
    proof fn perm_distinct(tracked a: &mut Self::Perm, tracked b: &Self::Perm)
        ensures
            *final(a) == *old(a),
            Self::perm_key(*final(a)) != Self::perm_key(*b),
    ;
}

impl<T> Loc for Ptr<T> {
    type Key = Ptr<T>;

    type Perm = PointsTo<T>;

    type Value = T;

    open spec fn key(&self) -> Ptr<T> {
        *self
    }

    open spec fn perm_key(perm: PointsTo<T>) -> Ptr<T> {
        perm.pptr()
    }

    open spec fn perm_init(perm: PointsTo<T>) -> bool {
        perm.is_init()
    }

    open spec fn perm_value(perm: PointsTo<T>) -> T {
        perm.value()
    }

    proof fn perm_distinct(tracked a: &mut PointsTo<T>, tracked b: &PointsTo<T>) {
        a.is_distinct(b);
    }
}

impl<T> Loc for Cell<T> {
    type Key = CellId;

    type Perm = CellPointsTo<T>;

    type Value = T;

    open spec fn key(&self) -> CellId {
        self.id()
    }

    open spec fn perm_key(perm: CellPointsTo<T>) -> CellId {
        perm.id()
    }

    open spec fn perm_init(perm: CellPointsTo<T>) -> bool {
        perm.is_init()
    }

    open spec fn perm_value(perm: CellPointsTo<T>) -> T {
        perm.value()
    }

    proof fn perm_distinct(tracked a: &mut CellPointsTo<T>, tracked b: &CellPointsTo<T>) {
        a.is_distinct(b);
    }
}

/// 権限の集合。各権限は自分の鍵の位置にあり、初期化済み。
pub tracked struct Region<L: Loc> {
    tracked map: Map<L::Key, L::Perm>,
}

impl<L: Loc> Region<L> {
    #[verifier::type_invariant]
    closed spec fn wf(self) -> bool {
        forall|k: L::Key|
            #![trigger self.map.dom().contains(k)]
            self.map.dom().contains(k) ==> L::perm_key(self.map[k]) == k && L::perm_init(
                self.map[k],
            )
    }

    /// 鍵から権限への写像。
    pub closed spec fn perms(&self) -> Map<L::Key, L::Perm> {
        self.map
    }

    pub open spec fn contains(&self, loc: L) -> bool {
        self.perms().dom().contains(loc.key())
    }

    /// `r[p]`：`p` の権限が指す値。
    pub open spec fn index(&self, loc: L) -> L::Value {
        L::perm_value(self.perms()[loc.key()])
    }

    /// 論理文脈の `r[p]`（Verus は spec の `[]` を `spec_index` に読む）。`self@x` / 領域変数でない
    /// 受け手（`b@items[p]`）の書き下し `b.items@[p]` がこれを呼ぶ（#103）。
    pub open spec fn spec_index(&self, loc: L) -> L::Value {
        self.index(loc)
    }

    pub open spec fn dom(&self) -> Set<L::Key> {
        self.perms().dom()
    }

    pub open spec fn len(&self) -> nat {
        self.dom().len()
    }

    pub proof fn empty() -> (tracked r: Region<L>)
        ensures
            r.perms() == Map::<L::Key, L::Perm>::empty(),
    {
        Region { map: Map::tracked_empty() }
    }

    /// 権限を入れる（`put r[p];`）。鍵が新しいことは事後条件で返す（`docs/current-spec.md`）。
    pub proof fn insert(tracked &mut self, tracked perm: L::Perm)
        requires
            L::perm_init(perm),
        ensures
            !old(self).perms().dom().contains(L::perm_key(perm)),
            final(self).perms() == old(self).perms().insert(L::perm_key(perm), perm),
    {
        use_type_invariant(&*self);
        let tracked mut perm = perm;
        let k = L::perm_key(perm);
        if self.map.dom().contains(k) {
            L::perm_distinct(&mut perm, self.map.tracked_borrow(k));
        }
        self.map.tracked_insert(k, perm);
    }

    /// 権限を取り出す（`take r[p];`）。
    pub proof fn remove(tracked &mut self, loc: L) -> (tracked perm: L::Perm)
        requires
            old(self).contains(loc),
        ensures
            perm == old(self).perms()[loc.key()],
            L::perm_key(perm) == loc.key(),
            L::perm_init(perm),
            final(self).perms() == old(self).perms().remove(loc.key()),
    {
        use_type_invariant(&*self);
        self.map.tracked_remove(loc.key())
    }

    /// 権限を共有参照で借りる（`borrow r[p] { }`）。
    pub proof fn borrow(tracked &self, loc: L) -> (tracked perm: &L::Perm)
        requires
            self.contains(loc),
        ensures
            *perm == self.perms()[loc.key()],
            L::perm_key(*perm) == loc.key(),
            L::perm_init(*perm),
    {
        use_type_invariant(self);
        self.map.tracked_borrow(loc.key())
    }
}

} // verus!
