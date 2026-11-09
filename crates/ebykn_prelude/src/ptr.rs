//! `Ptr<T>` と `PointsTo<T>`：`vstd::simple_pptr` の包み。

use core::marker::PhantomData;

use vstd::prelude::*;
use vstd::simple_pptr;

/// `Ptr<T>` の `T` がゼロサイズ型でないことの、コンパイル時の検査（#12、`docs/current-spec.md` §7.1）。
///
/// `Ptr::new` / `Ptr::empty` の本体が [`NonZst::OK`] を参照するので、`T` がゼロサイズ型なら
/// 単相化の時点で定数の評価が失敗し、ビルド（素の cargo、`ebykn build`）が `E0080` で止まる。
/// 検証（Verus が `cfg(verus_keep_ghost_body)` を立てる）では参照しない。`verus!` の外の項目なので、
/// 参照すると Verus が `is not supported` で拒否する。検証は単相化しないので、参照しても効かない。
/// 検証の側では `assert_non_zst` の分岐が「ゼロサイズ型でない」を示す。
pub(crate) struct NonZst<T>(PhantomData<T>);

impl<T> NonZst<T> {
    // rustc の定数の `assert!` は言語を切り替えられないので、英語（`--lang` の既定）と日本語を併記する。
    pub const OK: () = assert!(
        core::mem::size_of::<T>() != 0,
        "Ptr<T> cannot point to a zero-sized type T; use Cell<T> if you need distinct identities / Ptr<T> の T にゼロサイズ型は使えません（識別が要るなら Cell<T> を使う）"
    );
}

verus! {

/// ヒープ上の `T` への所有しないポインタ。`Copy`。実行時は `usize` 一つ。
///
/// フィールドを公開して、`Ptr` の等しさがアドレスの等しさであることを証明器に見せる。
pub struct Ptr<T>(pub simple_pptr::PPtr<T>);

impl<T> Clone for Ptr<T> {
    fn clone(&self) -> (r: Self)
        ensures
            r == *self,
    {
        Ptr(self.0)
    }
}

impl<T> Copy for Ptr<T> {

}

/// `Ptr<T>` の指す先を読み書き・解放する権限（tracked）。
///
/// 型不変条件として `T` がゼロサイズ型でないことを持つ。`Region` はこれを使って、
/// 挿入する権限の鍵が既存の鍵と重ならないことを示す（`docs/current-spec.md`）。
pub tracked struct PointsTo<T> {
    tracked perm: simple_pptr::PointsTo<T>,
}

impl<T> PointsTo<T> {
    #[verifier::type_invariant]
    closed spec fn non_zst(self) -> bool {
        vstd::layout::size_of::<T>() != 0
    }

    /// この権限が指すポインタ。
    pub closed spec fn pptr(&self) -> Ptr<T> {
        Ptr(self.perm.pptr())
    }

    pub closed spec fn is_init(&self) -> bool {
        self.perm.is_init()
    }

    pub open spec fn is_uninit(&self) -> bool {
        !self.is_init()
    }

    /// 指す先の値。`is_init()` のときだけ意味を持つ。
    pub closed spec fn value(&self) -> T {
        self.perm.value()
    }

    /// 同じ型の二つの権限は違うポインタを指す。
    pub proof fn is_distinct(tracked &mut self, tracked other: &PointsTo<T>)
        ensures
            *final(self) == *old(self),
            final(self).pptr() != other.pptr(),
    {
        use_type_invariant(&*self);
        use_type_invariant(other);
        self.perm.is_distinct(&other.perm);
    }
}

/// `T` がゼロサイズ型でないことを確かめる（#12、`docs/current-spec.md` §7.1）。
///
/// ゼロサイズ型の `T` は、ビルドでは先頭の [`NonZst::OK`] の評価がコンパイルエラーにする。
/// コンパイルされるどの実体でも `size_of::<T>() != 0` なので、後ろの `loop {}` には到達しない。
/// この分岐は、検証器に「`T` はゼロサイズ型でない」を教えるためだけにある（検証は単相化
/// しないので、コンパイル時の検査の結果を知らない）。`size_of::<T>()` は定数なので、
/// ゼロサイズでない型では分岐ごと消える。
#[verifier::exec_allows_no_decreases_clause]
fn assert_non_zst<T>()
    ensures
        vstd::layout::size_of::<T>() != 0,
{
    #[cfg(not(verus_keep_ghost_body))]
    let _ = NonZst::<T>::OK;
    if core::mem::size_of::<T>() == 0 {
        loop {}
    }
}

impl<T> Ptr<T> {
    /// `Ptr` のアドレス（論理）。
    pub open spec fn spec_addr(&self) -> usize {
        self.0.addr()
    }

    /// `v` をヒープに置き、そのポインタと権限を返す。
    pub fn new(v: T) -> (r: (Ptr<T>, Tracked<PointsTo<T>>))
        ensures
            r.1@.pptr() == r.0,
            r.1@.is_init(),
            r.1@.value() == v,
    {
        assert_non_zst::<T>();
        let (p, Tracked(perm)) = simple_pptr::PPtr::new(v);
        (Ptr(p), Tracked(PointsTo { perm }))
    }

    /// 初期化していない領域をヒープに取り、そのポインタと権限を返す。
    pub fn empty() -> (r: (Ptr<T>, Tracked<PointsTo<T>>))
        ensures
            r.1@.pptr() == r.0,
            r.1@.is_uninit(),
    {
        assert_non_zst::<T>();
        let (p, Tracked(perm)) = simple_pptr::PPtr::empty();
        (Ptr(p), Tracked(PointsTo { perm }))
    }

    /// 指す先を共有参照で読む（`*p`）。
    pub fn read<'a>(self, Tracked(perm): Tracked<&'a PointsTo<T>>) -> (r: &'a T)
        requires
            perm.pptr() == self,
            perm.is_init(),
        ensures
            *r == perm.value(),
    {
        self.0.borrow(Tracked(&perm.perm))
    }

    /// 指す先に `v` を書く（`*p = v`）。初期化済みなら前の値は drop せずに捨てる（`docs/current-spec.md`）。
    pub fn write(self, Tracked(perm): Tracked<&mut PointsTo<T>>, v: T)
        requires
            old(perm).pptr() == self,
        ensures
            final(perm).pptr() == self,
            final(perm).is_init(),
            final(perm).value() == v,
    {
        proof {
            use_type_invariant(&*perm);
            perm.perm.leak_contents();
        }
        self.0.put(Tracked(&mut perm.perm), v);
    }

    /// 指す先を可変参照で借りる（`(*p).f = e`）。
    pub fn borrow_mut<'a>(self, Tracked(perm): Tracked<&'a mut PointsTo<T>>) -> (r: &'a mut T)
        requires
            old(perm).pptr() == self,
            old(perm).is_init(),
        ensures
            final(perm).pptr() == self,
            final(perm).is_init(),
            *r == old(perm).value(),
            final(perm).value() == *final(r),
    {
        proof {
            use_type_invariant(&*perm);
        }
        self.0.borrow_mut(Tracked(&mut perm.perm))
    }

    /// 権限を消費して解放し、中の値を返す（`p.free()`）。
    pub fn free(self, Tracked(perm): Tracked<PointsTo<T>>) -> (r: T)
        requires
            perm.pptr() == self,
            perm.is_init(),
        ensures
            r == perm.value(),
    {
        let tracked PointsTo { perm: inner } = perm;
        self.0.into_inner(Tracked(inner))
    }

    /// アドレス。
    #[verifier::when_used_as_spec(spec_addr)]
    pub fn addr(&self) -> (r: usize)
        ensures
            r == self.spec_addr(),
    {
        self.0.addr()
    }
}

} // verus!
