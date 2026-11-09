//! `Cell<T>` と `CellPointsTo<T>`：`vstd::cell::pcell_maybe_uninit::PCell` の包み。
//!
//! 仕様 §7.1 の `vstd::cell::PCell` は固定した vstd で非推奨になり、
//! 初期化しない状態を持てる `pcell_maybe_uninit::PCell` が同じ役割を持つ（`docs/current-spec.md`）。

use vstd::cell::pcell_maybe_uninit as pcell;
use vstd::prelude::*;

pub use vstd::cell::CellId;

verus! {

/// 権限で中身を読み書きするセル。`Ptr` と違ってアドレスを持たず、`id()` で識別する。
pub struct Cell<T>(pcell::PCell<T>);

/// `Cell<T>` の中身を読み書きする権限（tracked）。
pub tracked struct CellPointsTo<T> {
    tracked perm: pcell::PointsTo<T>,
}

impl<T> CellPointsTo<T> {
    /// この権限が対応するセルの識別子。
    pub closed spec fn id(&self) -> CellId {
        self.perm.id()
    }

    pub closed spec fn is_init(&self) -> bool {
        self.perm.is_init()
    }

    pub open spec fn is_uninit(&self) -> bool {
        !self.is_init()
    }

    pub closed spec fn value(&self) -> T {
        self.perm.value()
    }

    /// 二つの権限は違うセルに対応する。
    pub proof fn is_distinct(tracked &mut self, tracked other: &CellPointsTo<T>)
        ensures
            *final(self) == *old(self),
            final(self).id() != other.id(),
    {
        self.perm.is_exclusive(&other.perm);
    }
}

impl<T> Cell<T> {
    pub closed spec fn id(&self) -> CellId {
        self.0.id()
    }

    pub fn new(v: T) -> (r: (Cell<T>, Tracked<CellPointsTo<T>>))
        ensures
            r.1@.id() == r.0.id(),
            r.1@.is_init(),
            r.1@.value() == v,
    {
        let (c, Tracked(perm)) = pcell::PCell::new(v);
        (Cell(c), Tracked(CellPointsTo { perm }))
    }

    pub fn empty() -> (r: (Cell<T>, Tracked<CellPointsTo<T>>))
        ensures
            r.1@.id() == r.0.id(),
            r.1@.is_uninit(),
    {
        let (c, Tracked(perm)) = pcell::PCell::empty();
        (Cell(c), Tracked(CellPointsTo { perm }))
    }

    pub fn read<'a>(&'a self, Tracked(perm): Tracked<&'a CellPointsTo<T>>) -> (r: &'a T)
        requires
            perm.id() == self.id(),
            perm.is_init(),
        ensures
            *r == perm.value(),
    {
        self.0.borrow(Tracked(&perm.perm))
    }

    /// 中身に `v` を書く。初期化済みなら前の値は drop せずに捨てる（`docs/current-spec.md`）。
    pub fn write(&self, Tracked(perm): Tracked<&mut CellPointsTo<T>>, v: T)
        requires
            old(perm).id() == self.id(),
        ensures
            final(perm).id() == self.id(),
            final(perm).is_init(),
            final(perm).value() == v,
    {
        self.0.write(Tracked(&mut perm.perm), v);
    }

    pub fn borrow_mut<'a>(&'a self, Tracked(perm): Tracked<&'a mut CellPointsTo<T>>) -> (r:
        &'a mut T)
        requires
            old(perm).id() == self.id(),
            old(perm).is_init(),
        ensures
            final(perm).id() == self.id(),
            final(perm).is_init(),
            *r == old(perm).value(),
            final(perm).value() == *final(r),
    {
        self.0.borrow_mut(Tracked(&mut perm.perm))
    }

    /// 権限を消費してセルを壊し、中の値を返す。
    pub fn free(self, Tracked(perm): Tracked<CellPointsTo<T>>) -> (r: T)
        requires
            perm.id() == self.id(),
            perm.is_init(),
        ensures
            r == perm.value(),
    {
        let tracked CellPointsTo { perm: inner } = perm;
        self.0.into_inner(Tracked(inner))
    }
}

} // verus!
