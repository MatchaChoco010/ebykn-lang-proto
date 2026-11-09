//! 健全性：検査（[`spec_check`]）が受理した関数は、どの経路でも違反（[`Viol`]）を起こさない。
//!
//! 証明の中心は不変条件 [`inv`]：検査の Γ と実行の状態が対応している。
//! - Γ で生きている変数（`Owned` / 借用）は、その種類の生きた札に束縛されている。
//! - Γ にない変数は、札に束縛されていないか、消費された札に束縛されている。
//! - 生きている変数どうしは別の札に束縛されている。
//! - この本体の `Owned` の生きた札は、どれも Γ で `Owned` の変数に束縛されている（迷子がない）。
//!
//! 各文について「検査が通り、入る前に不変条件が成り立てば、どの経路でも違反せず、出た先の Γ
//! （普通に終われば `fall`、`break` ならループの直後の Γ、`continue` なら本体の入口の Γ）と
//! 不変条件が成り立つ」ことを、文の構造とループの回数についての帰納法で示す（[`post`]）。

// Verus なしのビルドでは spec と proof が消えて使われなくなる。
#[allow(unused_imports)]
use crate::check::*;
#[allow(unused_imports)]
use crate::sem::*;
#[allow(unused_imports)]
use crate::syntax::*;
use vstd::prelude::*;

verus! {

// ---- 不変条件 ----

pub open spec fn live(s: Slot) -> bool {
    !(s is Absent || s is Captured)
}

pub open spec fn kind_slot(s: Slot) -> Kind {
    match s {
        Slot::Shared | Slot::BlkShared => Kind::Shared,
        Slot::Mut | Slot::BlkMut => Kind::Mut,
        _ => Kind::Owned,
    }
}

pub open spec fn rel(s: Slot, b: Option<nat>, toks: Seq<Tok>) -> bool {
    if live(s) {
        match b {
            Some(t) => toks[t as int] == Tok::Live(kind_slot(s)),
            None => false,
        }
    } else {
        match b {
            Some(t) => toks[t as int] is Dead,
            None => true,
        }
    }
}

pub open spec fn bound_ok(b: Option<nat>, base: nat, n: nat) -> bool {
    match b {
        Some(t) => base <= t && t < n,
        None => true,
    }
}

pub open spec fn inv(e: Env, st: St, fr: Frame) -> bool {
    &&& len(e) == st.bind.len()
    &&& fr.base <= st.toks.len()
    &&& forall|x: int|
        0 <= x < st.bind.len() ==> #[trigger] bound_ok(st.bind[x], fr.base, st.toks.len())
    &&& forall|x: int| 0 <= x < st.bind.len() ==> #[trigger] rel(get(e, x as nat), st.bind[x], st.toks)
    &&& forall|x: int, y: int|
        0 <= x < st.bind.len() && 0 <= y < st.bind.len() && x != y && live(get(e, x as nat)) && live(
            get(e, y as nat),
        ) ==> #[trigger] st.bind[x] != #[trigger] st.bind[y]
    &&& forall|t: nat|
        fr.base <= t < st.toks.len() && #[trigger] st.toks[t as int] == Tok::Live(Kind::Owned)
            ==> exists|x: int|
            0 <= x < st.bind.len() && get(e, x as nat) == Slot::Owned && #[trigger] st.bind[x] == Some(
                t,
            )
}

/// `st` から `s2` へ：変数の数は変わらず、札は増えるだけで、外の本体の札は変わらず、消費は戻らない。
pub open spec fn ext(st: St, s2: St, fr: Frame) -> bool {
    &&& s2.bind.len() == st.bind.len()
    &&& st.toks.len() <= s2.toks.len()
    &&& forall|t: int| 0 <= t < fr.base ==> #[trigger] s2.toks[t] == st.toks[t]
    &&& forall|t: int| 0 <= t < st.toks.len() && st.toks[t] is Dead ==> #[trigger] s2.toks[t] is Dead
}

pub open spec fn res_fall(r: Res) -> Option<Env> {
    match r {
        Res::Ok { fall, .. } => fall,
        Res::Err { .. } => None,
    }
}

pub open spec fn res_brk(r: Res) -> Option<Env> {
    match r {
        Res::Ok { brk, .. } => brk,
        Res::Err { .. } => None,
    }
}

/// `break` の行き先の Γ：`while` は本体の入口（先頭で抜けた場合と同じ）、`loop` は最初の `break` の Γ。
pub open spec fn brk_target(c: Ctx, r: Res) -> Env {
    match c.lp {
        LoopCtx::While(en) => en,
        LoopCtx::Loop(_) => res_brk(r).unwrap(),
        LoopCtx::NoLoop => Env::Nil,
    }
}

// ---- 抜ける点で終わる `borrow`（#21 の既定 D） ----
//
// `break` / `continue` で抜けるとき、検査は行き先のループに入ってから開いた `borrow` の `p` を Γ から
// 消す（[`release`]）。実行では、抜ける途中の各 `borrow` が `p` の束縛を入る前に戻す（[`restore`]）。
// 抜ける文の点ではまだ戻していないので、`post` は「印の付いた変数の束縛を入る前（`olds`）に戻した
// 状態」（[`restore_mask`]）で不変条件を述べる。

pub open spec fn marked(m: Env, y: nat) -> bool {
    !(get(m, y) is Absent)
}

pub open spec fn blk(s: Slot) -> bool {
    s is BlkShared || s is BlkMut
}

/// 印の付いた変数の束縛を `olds` に戻した状態。
pub open spec fn restore_mask(s: St, m: Env, olds: Seq<Option<nat>>) -> St {
    St {
        bind: Seq::new(
            s.bind.len(),
            |i: int|
                if marked(m, i as nat) {
                    olds[i]
                } else {
                    s.bind[i]
                },
        ),
        toks: s.toks,
    }
}

/// `borrow` に入る前の束縛：札がないか、消費された札。
pub open spec fn old_ok(b: Option<nat>, st: St, fr: Frame) -> bool {
    &&& bound_ok(b, fr.base, st.toks.len())
    &&& b matches Some(t) ==> st.toks[t as int] is Dead
}

/// 印の付いた変数は `borrow` の `p` で、入る前の束縛を `olds` に持っている。
pub open spec fn pend_ok(e: Env, m: Env, olds: Seq<Option<nat>>, st: St, fr: Frame) -> bool {
    &&& olds.len() == st.bind.len()
    &&& forall|y: nat| #[trigger] marked(m, y) ==> blk(get(e, y)) && old_ok(olds[y as int], st, fr)
}

/// ループの中なら、印の Γ は変数の数と同じ長さ（`borrow` で印を付けられる）。
pub open spec fn mask_ok(c: Ctx, st: St) -> bool {
    !(c.lp is NoLoop) ==> len(c.opened) == st.bind.len()
}

pub open spec fn post(out: Out, r: Res, c: Ctx, st: St, fr: Frame, olds: Seq<Option<nat>>) -> bool {
    match out {
        Out::Bad(_) => false,
        Out::Normal(s2) => res_fall(r) is Some && inv(res_fall(r).unwrap(), s2, fr) && ext(st, s2, fr),
        Out::Brk(s2) => !(c.lp is NoLoop) && (c.lp is Loop ==> res_brk(r) is Some) && inv(
            brk_target(c, r),
            restore_mask(s2, c.opened, olds),
            fr,
        ) && ext(st, s2, fr),
        Out::Cont(s2) => !(c.lp is NoLoop) && inv(
            entry_of(c.lp),
            restore_mask(s2, c.opened, olds),
            fr,
        ) && ext(st, s2, fr),
        Out::Ret(s2) => ext(st, s2, fr) && exit_sem(s2, fr),
        Out::Halt => true,
    }
}

// ---- Γ の補題 ----

pub proof fn lemma_len_set(e: Env, x: nat, v: Slot)
    ensures
        len(set(e, x, v)) == len(e),
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => if x != 0 {
            lemma_len_set(*t, (x - 1) as nat, v);
        },
    }
}

pub proof fn lemma_get_set(e: Env, x: nat, v: Slot, y: nat)
    ensures
        get(set(e, x, v), y) == if x == y && x < len(e) {
            v
        } else {
            get(e, y)
        },
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => if x != 0 && y != 0 {
            lemma_get_set(*t, (x - 1) as nat, v, (y - 1) as nat);
        },
    }
}

pub proof fn lemma_get_out(e: Env, y: nat)
    requires
        y >= len(e),
    ensures
        get(e, y) == Slot::Absent,
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => lemma_get_out(*t, (y - 1) as nat),
    }
}

pub proof fn lemma_capture(e: Env, y: nat)
    ensures
        len(capture(e)) == len(e),
        get(capture(e), y) == if get(e, y) is Absent {
            Slot::Absent
        } else {
            Slot::Captured
        },
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => {
            lemma_capture(*t, if y == 0 {
                0
            } else {
                (y - 1) as nat
            });
        },
    }
}

pub proof fn lemma_get_clear(e: Env, y: nat)
    ensures
        len(clear(e)) == len(e),
        get(clear(e), y) == Slot::Absent,
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => lemma_get_clear(*t, if y == 0 {
            0
        } else {
            (y - 1) as nat
        }),
    }
}

/// ループに入った時点の印には、何も付いていない。
pub proof fn lemma_clear_unmarked(e: Env)
    ensures
        len(clear(e)) == len(e),
        forall|y: nat| !#[trigger] marked(clear(e), y),
{
    lemma_get_clear(e, 0);
    assert forall|y: nat| !#[trigger] marked(clear(e), y) by {
        lemma_get_clear(e, y);
    }
}

pub proof fn lemma_nil_unmarked()
    ensures
        forall|y: nat| !#[trigger] marked(Env::Nil, y),
{
}

pub proof fn lemma_get_release(e: Env, m: Env, y: nat)
    ensures
        len(release(e, m)) == len(e),
        get(release(e, m), y) == if marked(m, y) {
            Slot::Absent
        } else {
            get(e, y)
        },
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => {
            if y == 0 {
                lemma_get_release(*t, tail(m), 0);
            } else {
                lemma_get_release(*t, tail(m), (y - 1) as nat);
                assert(get(tail(m), (y - 1) as nat) == get(m, y));
            }
        },
    }
}

pub proof fn lemma_no_owned(e: Env, y: nat)
    requires
        no_owned(e),
    ensures
        get(e, y) != Slot::Owned,
    decreases e,
{
    match e {
        Env::Nil => {},
        Env::Cons(_, t) => if y != 0 {
            lemma_no_owned(*t, (y - 1) as nat);
        },
    }
}

pub proof fn lemma_exit_ok(e: Env, g: Option<usize>, y: nat)
    requires
        exit_ok(e, g),
        get(e, y) == Slot::Owned,
    ensures
        g matches Some(gi) && gi as nat == y,
{
    match g {
        None => lemma_no_owned(e, y),
        Some(gi) => {
            lemma_no_owned(set(e, gi as nat, Slot::Absent), y);
            lemma_get_set(e, gi as nat, Slot::Absent, y);
        },
    }
}

// ---- 状態の補題 ----

pub proof fn lemma_ext_refl(st: St, fr: Frame)
    ensures
        ext(st, st, fr),
{
}

pub proof fn lemma_ext_trans(a: St, b: St, c: St, fr: Frame)
    requires
        ext(a, b, fr),
        ext(b, c, fr),
    ensures
        ext(a, c, fr),
{
    assert forall|t: int| 0 <= t < a.toks.len() && a.toks[t] is Dead implies #[trigger] c.toks[t] is Dead by {
        assert(b.toks[t] is Dead);
    }
    assert forall|t: int| 0 <= t < fr.base implies #[trigger] c.toks[t] == a.toks[t] by {
        assert(c.toks[t] == b.toks[t]);
    }
}

/// 本体の出口：検査の規則（[`exit_ok`]）が成り立てば、実行の規則（[`exit_sem`]）も成り立つ。
pub proof fn lemma_exit(e: Env, st: St, fr: Frame)
    requires
        inv(e, st, fr),
        exit_ok(e, fr.gives),
    ensures
        exit_sem(st, fr),
{
    assert forall|t: nat|
        fr.base <= t < st.toks.len() && #[trigger] st.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies (match fr.gives {
        Some(g) => (g as int) < st.bind.len() && st.bind[g as int] == Some(t),
        None => false,
    }) by {
        let x = choose|x: int|
            0 <= x < st.bind.len() && get(e, x as nat) == Slot::Owned && #[trigger] st.bind[x]
                == Some(t);
        lemma_exit_ok(e, fr.gives, x as nat);
    }
}

/// 新しい札を作って `x` に束縛する（導入、`take`、`borrow` の入口）。
pub proof fn lemma_fresh(e: Env, st: St, fr: Frame, x: usize, slot: Slot, k: Kind)
    requires
        inv(e, st, fr),
        (x as int) < st.bind.len(),
        !live(get(e, x as nat)),
        live(slot),
        kind_slot(slot) == k,
    ensures
        inv(set(e, x as nat, slot), fresh(st, x, k), fr),
        ext(st, fresh(st, x, k), fr),
{
    let e2 = set(e, x as nat, slot);
    let st2 = fresh(st, x, k);
    let tn = st.toks.len();
    lemma_len_set(e, x as nat, slot);
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] bound_ok(
        st2.bind[y],
        fr.base,
        st2.toks.len(),
    ) by {
        if y != x {
            assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        }
    }
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] rel(
        get(e2, y as nat),
        st2.bind[y],
        st2.toks,
    ) by {
        lemma_get_set(e, x as nat, slot, y as nat);
        if y != x {
            assert(rel(get(e, y as nat), st.bind[y], st.toks));
            assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        }
    }
    assert forall|y: int, z: int|
        0 <= y < st2.bind.len() && 0 <= z < st2.bind.len() && y != z && live(get(e2, y as nat))
            && live(get(e2, z as nat)) implies #[trigger] st2.bind[y] != #[trigger] st2.bind[z] by {
        lemma_get_set(e, x as nat, slot, y as nat);
        lemma_get_set(e, x as nat, slot, z as nat);
        if y == x {
            assert(rel(get(e, z as nat), st.bind[z], st.toks));
            assert(bound_ok(st.bind[z], fr.base, st.toks.len()));
        } else if z == x {
            assert(rel(get(e, y as nat), st.bind[y], st.toks));
            assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        } else {
            assert(st.bind[y] != st.bind[z]);
        }
    }
    assert forall|t: nat|
        fr.base <= t < st2.toks.len() && #[trigger] st2.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < st2.bind.len() && get(e2, w as nat) == Slot::Owned && #[trigger] st2.bind[w]
            == Some(t) by {
        if t == tn {
            lemma_get_set(e, x as nat, slot, x as nat);
            assert(st2.bind[x as int] == Some(t));
        } else {
            assert(st.toks[t as int] == Tok::Live(Kind::Owned));
            let w = choose|w: int|
                0 <= w < st.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] st.bind[w]
                    == Some(t);
            lemma_get_set(e, x as nat, slot, w as nat);
            assert(st2.bind[w] == Some(t));
        }
    }
    assert forall|t: int| 0 <= t < st.toks.len() && st.toks[t] is Dead implies #[trigger] st2.toks[t] is Dead by {
    }
    assert forall|t: int| 0 <= t < fr.base implies #[trigger] st2.toks[t] == st.toks[t] by {}
}

/// `x`（Γ で `Owned`）の札を消費する。
pub proof fn lemma_consume(e: Env, st: St, fr: Frame, x: usize)
    requires
        inv(e, st, fr),
        (x as int) < st.bind.len(),
        get(e, x as nat) == Slot::Owned,
    ensures
        consume(st, x) is Normal,
        inv(set(e, x as nat, Slot::Absent), consume(st, x)->Normal_0, fr),
        ext(st, consume(st, x)->Normal_0, fr),
{
    assert(rel(get(e, x as nat), st.bind[x as int], st.toks));
    assert(bound_ok(st.bind[x as int], fr.base, st.toks.len()));
    let t0 = st.bind[x as int].unwrap();
    let e2 = set(e, x as nat, Slot::Absent);
    let st2 = St { bind: st.bind, toks: st.toks.update(t0 as int, Tok::Dead) };
    assert(consume(st, x) == Out::Normal(st2));
    lemma_len_set(e, x as nat, Slot::Absent);
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] bound_ok(
        st2.bind[y],
        fr.base,
        st2.toks.len(),
    ) by {
        assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
    }
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] rel(
        get(e2, y as nat),
        st2.bind[y],
        st2.toks,
    ) by {
        lemma_get_set(e, x as nat, Slot::Absent, y as nat);
        assert(rel(get(e, y as nat), st.bind[y], st.toks));
        if y != x && live(get(e, y as nat)) {
            assert(st.bind[y] != st.bind[x as int]);
        }
    }
    assert forall|y: int, z: int|
        0 <= y < st2.bind.len() && 0 <= z < st2.bind.len() && y != z && live(get(e2, y as nat))
            && live(get(e2, z as nat)) implies #[trigger] st2.bind[y] != #[trigger] st2.bind[z] by {
        lemma_get_set(e, x as nat, Slot::Absent, y as nat);
        lemma_get_set(e, x as nat, Slot::Absent, z as nat);
        assert(st.bind[y] != st.bind[z]);
    }
    assert forall|t: nat|
        fr.base <= t < st2.toks.len() && #[trigger] st2.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < st2.bind.len() && get(e2, w as nat) == Slot::Owned && #[trigger] st2.bind[w]
            == Some(t) by {
        assert(t != t0);
        assert(st.toks[t as int] == Tok::Live(Kind::Owned));
        let w = choose|w: int|
            0 <= w < st.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] st.bind[w]
                == Some(t);
        lemma_get_set(e, x as nat, Slot::Absent, w as nat);
        assert(st2.bind[w] == Some(t));
    }
    assert forall|t: int| 0 <= t < st.toks.len() && st.toks[t] is Dead implies #[trigger] st2.toks[t] is Dead by {
    }
    assert forall|t: int| 0 <= t < fr.base implies #[trigger] st2.toks[t] == st.toks[t] by {}
}

/// `x`（Γ で生きていない）の束縛を外す（`x = e;`、`let x = ..;`）。
pub proof fn lemma_unbind(e: Env, st: St, fr: Frame, x: usize)
    requires
        inv(e, st, fr),
        (x as int) < st.bind.len(),
        !live(get(e, x as nat)),
    ensures
        inv(e, unbind(st, x), fr),
        ext(st, unbind(st, x), fr),
{
    let st2 = unbind(st, x);
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] bound_ok(
        st2.bind[y],
        fr.base,
        st2.toks.len(),
    ) by {
        if y != x {
            assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        }
    }
    assert forall|y: int| 0 <= y < st2.bind.len() implies #[trigger] rel(
        get(e, y as nat),
        st2.bind[y],
        st2.toks,
    ) by {
        if y != x {
            assert(rel(get(e, y as nat), st.bind[y], st.toks));
        }
    }
    assert forall|y: int, z: int|
        0 <= y < st2.bind.len() && 0 <= z < st2.bind.len() && y != z && live(get(e, y as nat))
            && live(get(e, z as nat)) implies #[trigger] st2.bind[y] != #[trigger] st2.bind[z] by {
        assert(st.bind[y] != st.bind[z]);
    }
    assert forall|t: nat|
        fr.base <= t < st2.toks.len() && #[trigger] st2.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < st2.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] st2.bind[w]
            == Some(t) by {
        let w = choose|w: int|
            0 <= w < st.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] st.bind[w]
                == Some(t);
        assert(st2.bind[w] == Some(t));
    }
}

/// `borrow` ブロックの終わり：`x` を Γ から消し、束縛を入る前（消費済みか、なし）に戻す。
pub proof fn lemma_borrow_end(f: Env, s2: St, fr: Frame, x: usize, old: Option<nat>)
    requires
        inv(f, s2, fr),
        (x as int) < s2.bind.len(),
        get(f, x as nat) is BlkShared || get(f, x as nat) is BlkMut,
        bound_ok(old, fr.base, s2.toks.len()),
        old matches Some(t) ==> s2.toks[t as int] is Dead,
    ensures
        inv(
            set(f, x as nat, Slot::Absent),
            St { bind: s2.bind.update(x as int, old), toks: s2.toks },
            fr,
        ),
{
    let f2 = set(f, x as nat, Slot::Absent);
    let s3 = St { bind: s2.bind.update(x as int, old), toks: s2.toks };
    lemma_len_set(f, x as nat, Slot::Absent);
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] bound_ok(
        s3.bind[y],
        fr.base,
        s3.toks.len(),
    ) by {
        if y != x {
            assert(bound_ok(s2.bind[y], fr.base, s2.toks.len()));
        }
    }
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] rel(
        get(f2, y as nat),
        s3.bind[y],
        s3.toks,
    ) by {
        lemma_get_set(f, x as nat, Slot::Absent, y as nat);
        if y != x {
            assert(rel(get(f, y as nat), s2.bind[y], s2.toks));
        }
    }
    assert forall|y: int, z: int|
        0 <= y < s3.bind.len() && 0 <= z < s3.bind.len() && y != z && live(get(f2, y as nat))
            && live(get(f2, z as nat)) implies #[trigger] s3.bind[y] != #[trigger] s3.bind[z] by {
        lemma_get_set(f, x as nat, Slot::Absent, y as nat);
        lemma_get_set(f, x as nat, Slot::Absent, z as nat);
        assert(s2.bind[y] != s2.bind[z]);
    }
    assert forall|t: nat|
        fr.base <= t < s3.toks.len() && #[trigger] s3.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < s3.bind.len() && get(f2, w as nat) == Slot::Owned && #[trigger] s3.bind[w]
            == Some(t) by {
        let w = choose|w: int|
            0 <= w < s2.bind.len() && get(f, w as nat) == Slot::Owned && #[trigger] s2.bind[w]
                == Some(t);
        lemma_get_set(f, x as nat, Slot::Absent, w as nat);
        assert(w != x);
        assert(s3.bind[w] == Some(t));
    }
}

/// `break` / `continue` の点：印の付いた変数を Γ から消し、束縛を入る前に戻すと、不変条件が成り立つ。
pub proof fn lemma_release_inv(e: Env, st: St, fr: Frame, m: Env, olds: Seq<Option<nat>>)
    requires
        inv(e, st, fr),
        pend_ok(e, m, olds, st, fr),
    ensures
        inv(release(e, m), restore_mask(st, m, olds), fr),
{
    let e2 = release(e, m);
    let s3 = restore_mask(st, m, olds);
    lemma_get_release(e, m, 0);
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] bound_ok(
        s3.bind[y],
        fr.base,
        s3.toks.len(),
    ) by {
        if marked(m, y as nat) {
        } else {
            assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        }
    }
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] rel(
        get(e2, y as nat),
        s3.bind[y],
        s3.toks,
    ) by {
        lemma_get_release(e, m, y as nat);
        if marked(m, y as nat) {
        } else {
            assert(rel(get(e, y as nat), st.bind[y], st.toks));
        }
    }
    assert forall|y: int, z: int|
        0 <= y < s3.bind.len() && 0 <= z < s3.bind.len() && y != z && live(get(e2, y as nat))
            && live(get(e2, z as nat)) implies #[trigger] s3.bind[y] != #[trigger] s3.bind[z] by {
        lemma_get_release(e, m, y as nat);
        lemma_get_release(e, m, z as nat);
        assert(st.bind[y] != st.bind[z]);
    }
    assert forall|t: nat|
        fr.base <= t < s3.toks.len() && #[trigger] s3.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < s3.bind.len() && get(e2, w as nat) == Slot::Owned && #[trigger] s3.bind[w]
            == Some(t) by {
        let w = choose|w: int|
            0 <= w < st.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] st.bind[w]
                == Some(t);
        if marked(m, w as nat) {
            assert(blk(get(e, w as nat)));
        }
        lemma_get_release(e, m, w as nat);
        assert(s3.bind[w] == Some(t));
    }
}

/// 印が何も付いていなければ、戻しても同じ状態。
pub proof fn lemma_restore_none(s: St, m: Env, olds: Seq<Option<nat>>)
    requires
        forall|y: nat| !#[trigger] marked(m, y),
    ensures
        restore_mask(s, m, olds) == s,
{
    let r = restore_mask(s, m, olds);
    assert forall|i: int| 0 <= i < r.bind.len() implies r.bind[i] == s.bind[i] by {
        assert(!marked(m, i as nat));
    }
    assert(r.bind =~= s.bind);
}

/// `borrow` ブロックを `break` / `continue` で抜ける：ブロックの `x` の束縛を戻してから、外の印で戻す。
pub proof fn lemma_restore_step(s2: St, m: Env, olds: Seq<Option<nat>>, x: usize, old: Option<nat>)
    requires
        (x as nat) < len(m),
        !marked(m, x as nat),
        (x as int) < s2.bind.len(),
        olds.len() == s2.bind.len(),
    ensures
        restore_mask(s2, set(m, x as nat, Slot::Owned), olds.update(x as int, old))
            == restore_mask(St { bind: s2.bind.update(x as int, old), toks: s2.toks }, m, olds),
{
    let l = restore_mask(s2, set(m, x as nat, Slot::Owned), olds.update(x as int, old));
    let r = restore_mask(St { bind: s2.bind.update(x as int, old), toks: s2.toks }, m, olds);
    assert forall|i: int| 0 <= i < l.bind.len() implies l.bind[i] == r.bind[i] by {
        lemma_get_set(m, x as nat, Slot::Owned, i as nat);
    }
    assert(l.bind =~= r.bind);
}

/// 印の付いた変数の欄が変わらず、状態が先へ進んでも、`pend_ok` は保たれる。
pub proof fn lemma_pend_ext(
    e: Env,
    e2: Env,
    m: Env,
    olds: Seq<Option<nat>>,
    st: St,
    s2: St,
    fr: Frame,
)
    requires
        pend_ok(e, m, olds, st, fr),
        ext(st, s2, fr),
        forall|y: nat| #[trigger] marked(m, y) ==> get(e2, y) == get(e, y),
    ensures
        pend_ok(e2, m, olds, s2, fr),
{
    assert forall|y: nat| #[trigger] marked(m, y) implies blk(get(e2, y)) && old_ok(
        olds[y as int],
        s2,
        fr,
    ) by {
        assert(old_ok(olds[y as int], st, fr));
        if olds[y as int] is Some {
            let t = olds[y as int].unwrap() as int;
            assert(st.toks[t] is Dead);
            assert(s2.toks[t] is Dead);
        }
    }
}

/// クロージャの呼び出しの入口：外の Γ を `Captured` にした Γ と、束縛のない新しい本体。
pub proof fn lemma_closure_start(e: Env, st: St, fr: Frame)
    requires
        inv(e, st, fr),
    ensures
        inv(capture(e), closure_start(st), closure_frame(st)),
{
    let s2 = closure_start(st);
    let f2 = closure_frame(st);
    lemma_capture(e, 0);
    assert forall|y: int| 0 <= y < s2.bind.len() implies #[trigger] rel(
        get(capture(e), y as nat),
        s2.bind[y],
        s2.toks,
    ) by {
        lemma_capture(e, y as nat);
    }
    assert forall|y: int| 0 <= y < s2.bind.len() implies #[trigger] bound_ok(
        s2.bind[y],
        f2.base,
        s2.toks.len(),
    ) by {}
    assert forall|y: int, z: int|
        0 <= y < s2.bind.len() && 0 <= z < s2.bind.len() && y != z && live(
            get(capture(e), y as nat),
        ) && live(get(capture(e), z as nat)) implies #[trigger] s2.bind[y] != #[trigger] s2.bind[z] by {
        lemma_capture(e, y as nat);
    }
}

/// クロージャの呼び出しから戻る：外の束縛に戻し、札の表は呼び出しの後のものを使う。
pub proof fn lemma_closure_back(e: Env, st: St, fr: Frame, s: St)
    requires
        inv(e, st, fr),
        ext(closure_start(st), s, closure_frame(st)),
        exit_sem(s, closure_frame(st)),
    ensures
        inv(e, St { bind: st.bind, toks: s.toks }, fr),
        ext(st, St { bind: st.bind, toks: s.toks }, fr),
{
    let s3 = St { bind: st.bind, toks: s.toks };
    let n0 = st.toks.len();
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] bound_ok(
        s3.bind[y],
        fr.base,
        s3.toks.len(),
    ) by {
        assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
    }
    assert forall|y: int| 0 <= y < s3.bind.len() implies #[trigger] rel(
        get(e, y as nat),
        s3.bind[y],
        s3.toks,
    ) by {
        assert(rel(get(e, y as nat), st.bind[y], st.toks));
        assert(bound_ok(st.bind[y], fr.base, st.toks.len()));
        if st.bind[y] is Some {
            let t = st.bind[y].unwrap() as int;
            assert(s.toks[t] == closure_start(st).toks[t]);
        }
    }
    assert forall|y: int, z: int|
        0 <= y < s3.bind.len() && 0 <= z < s3.bind.len() && y != z && live(get(e, y as nat))
            && live(get(e, z as nat)) implies #[trigger] s3.bind[y] != #[trigger] s3.bind[z] by {
        assert(st.bind[y] != st.bind[z]);
    }
    assert forall|t: nat|
        fr.base <= t < s3.toks.len() && #[trigger] s3.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < s3.bind.len() && get(e, w as nat) == Slot::Owned && #[trigger] s3.bind[w]
            == Some(t) by {
        if t < n0 {
            assert(s.toks[t as int] == closure_start(st).toks[t as int]);
            assert(st.toks[t as int] == Tok::Live(Kind::Owned));
        } else {
            assert(s.toks[t as int] == Tok::Live(Kind::Owned));
        }
    }
    assert forall|t: int| 0 <= t < fr.base implies #[trigger] s3.toks[t] == st.toks[t] by {
        assert(s.toks[t] == closure_start(st).toks[t]);
    }
    assert forall|t: int| 0 <= t < st.toks.len() && st.toks[t] is Dead implies #[trigger] s3.toks[t] is Dead by {
        assert(s.toks[t] == closure_start(st).toks[t]);
    }
}

// ---- 検査だけの補題 ----

/// 最初の `break` の Γ は、一度決まれば変わらない。
pub proof fn lemma_mono_stmt(s: Stmt, e: Env, b: Env, c: Ctx)
    requires
        chk_stmt(s, e, Some(b), c) is Ok,
    ensures
        res_brk(chk_stmt(s, e, Some(b), c)) == Some(b),
    decreases s,
{
    match s.op {
        Op::Branch { a, b: bb, .. } => {
            lemma_mono_block(*a, e, b, c);
            lemma_mono_block(*bb, e, b, c);
        },
        Op::Borrow { x, mutable, body } => {
            lemma_mono_block(*body, set(e, x as nat, blk_slot(mutable)), b, borrow_ctx(c, x, mutable));
        },
        _ => {},
    }
}

pub proof fn lemma_mono_block(bl: Block, e: Env, b: Env, c: Ctx)
    requires
        chk_block(bl, e, Some(b), c) is Ok,
    ensures
        res_brk(chk_block(bl, e, Some(b), c)) == Some(b),
    decreases bl,
{
    match bl {
        Block::Nil => {},
        Block::Cons(s, r) => {
            lemma_mono_stmt(*s, e, b, c);
            match chk_stmt(*s, e, Some(b), c) {
                Res::Ok { fall: Some(e2), .. } => lemma_mono_block(*r, e2, b, c),
                _ => {},
            }
        },
    }
}

pub open spec fn borrowed(s: Slot) -> bool {
    s is Shared || s is Mut || s is BlkShared || s is BlkMut
}

/// 借用の欄は変わらない（消費も再束縛も拒まれる）。出口の Γ では同じで、新しく決まった `break` の Γ
/// では、抜ける点で消した後（[`release`]）と同じ。
pub proof fn lemma_frozen_stmt(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, y: nat)
    requires
        chk_stmt(s, e, brk, c) is Ok,
        borrowed(get(e, y)),
    ensures
        res_fall(chk_stmt(s, e, brk, c)) matches Some(f) ==> get(f, y) == get(e, y),
        brk is None ==> (res_brk(chk_stmt(s, e, brk, c)) matches Some(b) ==> get(b, y) == get(
            release(e, c.opened),
            y,
        )),
    decreases s,
{
    let r = chk_stmt(s, e, brk, c);
    match s.op {
        Op::Intro { x, .. } => lemma_get_set(e, x as nat, Slot::Owned, y),
        Op::Take { x } => lemma_get_set(e, x as nat, Slot::Owned, y),
        Op::Consume { x, .. } => lemma_get_set(e, x as nat, Slot::Absent, y),
        Op::Branch { a, b, .. } => {
            lemma_frozen_block(*a, e, brk, c, y);
            let b1 = res_brk(chk_block(*a, e, brk, c));
            lemma_frozen_block(*b, e, b1, c, y);
            if b1 is Some {
                lemma_mono_block(*b, e, b1.unwrap(), c);
            }
        },
        Op::Loop { body } => {
            lemma_frozen_block(
                *body,
                e,
                None,
                Ctx { lp: LoopCtx::Loop(e), opened: clear(e), ..c },
                y,
            );
            lemma_get_release(e, clear(e), y);
            lemma_get_clear(e, y);
        },
        Op::Borrow { x, mutable, body } => {
            let e1 = set(e, x as nat, blk_slot(mutable));
            let c3 = borrow_ctx(c, x, mutable);
            lemma_get_set(e, x as nat, blk_slot(mutable), y);
            lemma_get_set(c.opened, x as nat, Slot::Owned, y);
            lemma_get_release(e1, c3.opened, y);
            lemma_get_release(e, c.opened, y);
            lemma_frozen_block(*body, e1, brk, c3, y);
            match res_fall(chk_block(*body, e1, brk, c3)) {
                Some(f) => lemma_get_set(f, x as nat, Slot::Absent, y),
                None => {},
            }
        },
        _ => {},
    }
}

pub proof fn lemma_frozen_block(bl: Block, e: Env, brk: Option<Env>, c: Ctx, y: nat)
    requires
        chk_block(bl, e, brk, c) is Ok,
        borrowed(get(e, y)),
    ensures
        res_fall(chk_block(bl, e, brk, c)) matches Some(f) ==> get(f, y) == get(e, y),
        brk is None ==> (res_brk(chk_block(bl, e, brk, c)) matches Some(b) ==> get(b, y) == get(
            release(e, c.opened),
            y,
        )),
    decreases bl,
{
    match bl {
        Block::Nil => {},
        Block::Cons(s, r) => {
            lemma_frozen_stmt(*s, e, brk, c, y);
            match chk_stmt(*s, e, brk, c) {
                Res::Ok { fall: Some(e2), brk: b2 } => {
                    lemma_get_release(e, c.opened, y);
                    lemma_get_release(e2, c.opened, y);
                    lemma_frozen_block(*r, e2, b2, c, y);
                    if b2 is Some {
                        lemma_mono_block(*r, e2, b2.unwrap(), c);
                    }
                },
                _ => {},
            }
        },
    }
}

// ---- 健全性 ----

pub open spec fn pre(
    s_wf: bool,
    r: Res,
    e: Env,
    c: Ctx,
    st: St,
    fr: Frame,
    olds: Seq<Option<nat>>,
) -> bool {
    &&& s_wf
    &&& r is Ok
    &&& inv(e, st, fr)
    &&& c.gives == fr.gives
    &&& pend_ok(e, c.opened, olds, st, fr)
    &&& mask_ok(c, st)
}

proof fn lemma_atom(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
        !(s.op is Branch || s.op is While || s.op is Loop || s.op is Borrow || s.op is Closure),
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
{
    lemma_ext_refl(st, fr);
    match s.op {
        Op::Intro { x, .. } => lemma_fresh(e, st, fr, x, Slot::Owned, Kind::Owned),
        Op::Take { x } => lemma_fresh(e, st, fr, x, Slot::Owned, Kind::Owned),
        Op::Consume { x, .. } => lemma_consume(e, st, fr, x),
        Op::BorrowArg { x, .. } => {
            assert(rel(get(e, x as nat), st.bind[x as int], st.toks));
        },
        Op::Write { x } => {
            assert(rel(get(e, x as nat), st.bind[x as int], st.toks));
        },
        Op::Assign { x } => lemma_unbind(e, st, fr, x),
        Op::Let { x } => lemma_unbind(e, st, fr, x),
        Op::Return => lemma_exit(e, st, fr),
        Op::Try => lemma_exit(e, st, fr),
        Op::Break | Op::Continue => lemma_release_inv(e, st, fr, c.opened, olds),
        _ => {},
    }
}

proof fn lemma_branch(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
        s.op is Branch,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
    decreases s, 0nat,
{
    if let Op::Branch { a, b, .. } = s.op {
        let ra = chk_block(*a, e, brk, c);
        let b1 = res_brk(ra);
        let rb = chk_block(*b, e, b1, c);
        if choice(o) == 0 {
            lemma_block(*a, e, brk, c, st, fr, rest(o), olds);
            if b1 is Some {
                lemma_mono_block(*b, e, b1.unwrap(), c);
            }
        } else {
            lemma_block(*b, e, b1, c, st, fr, rest(o), olds);
        }
    }
}

proof fn lemma_while(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
        s.op is While || s.op is Loop,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
    decreases s, 0nat,
{
    match s.op {
        Op::While { body } => lemma_iter(
            *body,
            e,
            Ctx { lp: LoopCtx::While(e), opened: clear(e), ..c },
            st,
            fr,
            rest(o),
            choice(o),
            true,
        ),
        Op::Loop { body } => lemma_iter(
            *body,
            e,
            Ctx { lp: LoopCtx::Loop(e), opened: clear(e), ..c },
            st,
            fr,
            rest(o),
            choice(o),
            false,
        ),
        _ => {},
    }
}

pub open spec fn iter_target(exits: bool, e: Env, r: Res) -> Env {
    if exits {
        e
    } else {
        res_brk(r).unwrap()
    }
}

/// ループを `k` 回まで回す。普通に抜ければ、ループの直後の Γ と不変条件が成り立つ。
proof fn lemma_iter(
    body: Block,
    e: Env,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    k: nat,
    exits: bool,
)
    requires
        wf_block(body, st.bind.len(), true),
        chk_block(body, e, None, c) is Ok,
        res_fall(chk_block(body, e, None, c)) matches Some(f) ==> f == e,
        inv(e, st, fr),
        c.gives == fr.gives,
        c.lp == (if exits {
            LoopCtx::While(e)
        } else {
            LoopCtx::Loop(e)
        }),
        c.opened == clear(e),
    ensures
        ({
            let out = run_loop(body, st, fr, o, k, exits).0;
            let r = chk_block(body, e, None, c);
            match out {
                Out::Normal(s2) => (!exits ==> res_brk(r) is Some) && inv(
                    iter_target(exits, e, r),
                    s2,
                    fr,
                ) && ext(st, s2, fr),
                Out::Ret(s2) => ext(st, s2, fr) && exit_sem(s2, fr),
                Out::Halt => true,
                _ => false,
            }
        }),
    decreases body, k,
{
    lemma_ext_refl(st, fr);
    if k > 0 {
        lemma_clear_unmarked(e);
        lemma_block(body, e, None, c, st, fr, o, st.bind);
        let (out, o2) = run_block(body, st, fr, o);
        match out {
            Out::Normal(s) => {
                lemma_iter(body, e, c, s, fr, o2, (k - 1) as nat, exits);
                let out2 = run_loop(body, s, fr, o2, (k - 1) as nat, exits).0;
                match out2 {
                    Out::Normal(s3) => lemma_ext_trans(st, s, s3, fr),
                    Out::Ret(s3) => lemma_ext_trans(st, s, s3, fr),
                    _ => {},
                }
            },
            Out::Brk(s) => lemma_restore_none(s, c.opened, st.bind),
            Out::Cont(s) => {
                lemma_restore_none(s, c.opened, st.bind);
                lemma_iter(body, e, c, s, fr, o2, (k - 1) as nat, exits);
                let out2 = run_loop(body, s, fr, o2, (k - 1) as nat, exits).0;
                match out2 {
                    Out::Normal(s3) => lemma_ext_trans(st, s, s3, fr),
                    Out::Ret(s3) => lemma_ext_trans(st, s, s3, fr),
                    _ => {},
                }
            },
            _ => {},
        }
    }
}

proof fn lemma_borrow(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
        s.op is Borrow,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
    decreases s, 0nat,
{
    if let Op::Borrow { x, mutable, body } = s.op {
        let k = blk_slot(mutable);
        let e1 = set(e, x as nat, k);
        let c3 = borrow_ctx(c, x, mutable);
        let st1 = fresh(st, x, kind_of(mutable));
        let old = st.bind[x as int];
        let m = c.opened;
        let m1 = c3.opened;
        let olds1 = olds.update(x as int, old);
        assert(rel(get(e, x as nat), old, st.toks));
        assert(bound_ok(old, fr.base, st.toks.len()));
        lemma_fresh(e, st, fr, x, k, kind_of(mutable));
        lemma_len_set(m, x as nat, Slot::Owned);
        // `x` は Γ になかったので、外の印は付いていない。
        if marked(m, x as nat) {
            assert(blk(get(e, x as nat)));
        }
        // 本体の印：`x` に入る前の束縛を持たせる。
        assert forall|y: nat| #[trigger] marked(m1, y) implies blk(get(e1, y)) && old_ok(
            olds1[y as int],
            st1,
            fr,
        ) by {
            lemma_get_set(m, x as nat, Slot::Owned, y);
            lemma_get_set(e, x as nat, k, y);
            if y == x as nat {
                if old is Some {
                    assert(st1.toks[old.unwrap() as int] == st.toks[old.unwrap() as int]);
                }
            } else {
                assert(marked(m, y));
                assert(old_ok(olds[y as int], st, fr));
                if olds[y as int] is Some {
                    let t = olds[y as int].unwrap() as int;
                    assert(st1.toks[t] == st.toks[t]);
                }
            }
        }
        lemma_block(*body, e1, brk, c3, st1, fr, o, olds1);
        lemma_get_set(e, x as nat, k, x as nat);
        lemma_frozen_block(*body, e1, brk, c3, x as nat);
        let outb = run_block(*body, st1, fr, o).0;
        match outb {
            Out::Normal(s2) => {
                lemma_ext_trans(st, st1, s2, fr);
                let f = res_fall(chk_block(*body, e1, brk, c3)).unwrap();
                lemma_borrow_end(f, s2, fr, x, old);
                let s3 = St { bind: s2.bind.update(x as int, old), toks: s2.toks };
                assert(ext(st, s3, fr));
            },
            // 抜ける：束縛を戻してから外へ（消費ではない）。
            Out::Brk(s2) => {
                lemma_ext_trans(st, st1, s2, fr);
                lemma_restore_step(s2, m, olds, x, old);
                let s3 = St { bind: s2.bind.update(x as int, old), toks: s2.toks };
                assert(ext(st, s3, fr));
            },
            Out::Cont(s2) => {
                lemma_ext_trans(st, st1, s2, fr);
                lemma_restore_step(s2, m, olds, x, old);
                let s3 = St { bind: s2.bind.update(x as int, old), toks: s2.toks };
                assert(ext(st, s3, fr));
            },
            Out::Ret(s2) => lemma_ext_trans(st, st1, s2, fr),
            _ => {},
        }
    }
}

proof fn lemma_closure(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
        s.op is Closure,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
    decreases s, 0nat,
{
    if let Op::Closure { body } = s.op {
        lemma_calls(*body, e, c, st, fr, rest(o), choice(o));
    }
}

/// クロージャを `k` 回呼ぶ。外の Γ と不変条件は変わらない。
proof fn lemma_calls(body: Block, e: Env, c: Ctx, st: St, fr: Frame, o: Seq<nat>, k: nat)
    requires
        wf_block(body, st.bind.len(), false),
        chk_block(body, capture(e), None, closure_ctx(c)) is Ok,
        res_fall(chk_block(body, capture(e), None, closure_ctx(c))) matches Some(f) ==> exit_ok(
            f,
            None,
        ),
        inv(e, st, fr),
    ensures
        ({
            let out = run_closure(body, st, fr, o, k).0;
            match out {
                Out::Normal(s2) => inv(e, s2, fr) && ext(st, s2, fr),
                Out::Halt => true,
                _ => false,
            }
        }),
    decreases body, k,
{
    lemma_ext_refl(st, fr);
    if k > 0 {
        let fr2 = closure_frame(st);
        let st2 = closure_start(st);
        lemma_closure_start(e, st, fr);
        lemma_nil_unmarked();
        lemma_block(body, capture(e), None, closure_ctx(c), st2, fr2, o, st2.bind);
        let (out, o2) = run_block(body, st2, fr2, o);
        match out {
            Out::Normal(s) => {
                let f = res_fall(chk_block(body, capture(e), None, closure_ctx(c))).unwrap();
                lemma_exit(f, s, fr2);
                lemma_closure_back(e, st, fr, s);
                let s3 = St { bind: st.bind, toks: s.toks };
                lemma_calls(body, e, c, s3, fr, o2, (k - 1) as nat);
                match run_closure(body, s3, fr, o2, (k - 1) as nat).0 {
                    Out::Normal(s4) => lemma_ext_trans(st, s3, s4, fr),
                    _ => {},
                }
            },
            Out::Ret(s) => {
                lemma_closure_back(e, st, fr, s);
                let s3 = St { bind: st.bind, toks: s.toks };
                lemma_calls(body, e, c, s3, fr, o2, (k - 1) as nat);
                match run_closure(body, s3, fr, o2, (k - 1) as nat).0 {
                    Out::Normal(s4) => lemma_ext_trans(st, s3, s4, fr),
                    _ => {},
                }
            },
            _ => {},
        }
    }
}

pub proof fn lemma_stmt(
    s: Stmt,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr, olds),
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr, olds),
    decreases s, 1nat,
{
    match s.op {
        Op::Branch { .. } => lemma_branch(s, e, brk, c, st, fr, o, olds),
        Op::While { .. } => lemma_while(s, e, brk, c, st, fr, o, olds),
        Op::Loop { .. } => lemma_while(s, e, brk, c, st, fr, o, olds),
        Op::Borrow { .. } => lemma_borrow(s, e, brk, c, st, fr, o, olds),
        Op::Closure { .. } => lemma_closure(s, e, brk, c, st, fr, o, olds),
        _ => lemma_atom(s, e, brk, c, st, fr, o, olds),
    }
}

pub proof fn lemma_block(
    b: Block,
    e: Env,
    brk: Option<Env>,
    c: Ctx,
    st: St,
    fr: Frame,
    o: Seq<nat>,
    olds: Seq<Option<nat>>,
)
    requires
        pre(wf_block(b, st.bind.len(), !(c.lp is NoLoop)), chk_block(b, e, brk, c), e, c, st, fr, olds),
    ensures
        post(run_block(b, st, fr, o).0, chk_block(b, e, brk, c), c, st, fr, olds),
    decreases b, 0nat,
{
    lemma_ext_refl(st, fr);
    match b {
        Block::Nil => {},
        Block::Cons(s, r) => {
            lemma_stmt(*s, e, brk, c, st, fr, o, olds);
            let rs = chk_stmt(*s, e, brk, c);
            let (out, o2) = run_stmt(*s, st, fr, o);
            match out {
                Out::Normal(s2) => {
                    let e2 = res_fall(rs).unwrap();
                    let b2 = res_brk(rs);
                    // 印の付いた変数（`borrow` の `p`）の欄は変わらない。
                    assert forall|y: nat| #[trigger] marked(c.opened, y) implies get(e2, y) == get(
                        e,
                        y,
                    ) by {
                        lemma_frozen_stmt(*s, e, brk, c, y);
                    }
                    lemma_pend_ext(e, e2, c.opened, olds, st, s2, fr);
                    lemma_block(*r, e2, b2, c, s2, fr, o2, olds);
                    match run_block(*r, s2, fr, o2).0 {
                        Out::Normal(s3) => lemma_ext_trans(st, s2, s3, fr),
                        Out::Brk(s3) => lemma_ext_trans(st, s2, s3, fr),
                        Out::Cont(s3) => lemma_ext_trans(st, s2, s3, fr),
                        Out::Ret(s3) => lemma_ext_trans(st, s2, s3, fr),
                        _ => {},
                    }
                },
                Out::Brk(_) => {
                    if let Res::Ok { fall: Some(e2), brk: b2 } = rs {
                        if b2 is Some {
                            lemma_mono_block(*r, e2, b2.unwrap(), c);
                        }
                    }
                },
                _ => {},
            }
        },
    }
}

/// 関数に入った直後に不変条件が成り立つ。
pub proof fn lemma_init(p: Program)
    requires
        wf(p),
    ensures
        inv(p.params, init(p), frame0(p)),
{
    let st = init(p);
    let n = len(p.params);
    assert forall|y: int| 0 <= y < st.bind.len() implies #[trigger] rel(
        get(p.params, y as nat),
        st.bind[y],
        st.toks,
    ) by {
        assert(param_slot(get(p.params, y as nat)));
    }
    assert forall|y: int| 0 <= y < st.bind.len() implies #[trigger] bound_ok(
        st.bind[y],
        0,
        st.toks.len(),
    ) by {}
    assert forall|t: nat|
        0 <= t < st.toks.len() && #[trigger] st.toks[t as int] == Tok::Live(
            Kind::Owned,
        ) implies exists|w: int|
        0 <= w < st.bind.len() && get(p.params, w as nat) == Slot::Owned && #[trigger] st.bind[w]
            == Some(t) by {
        assert(param_slot(get(p.params, t)));
        assert(st.bind[t as int] == Some(t));
    }
}

/// 定理：検査が受理した（`unchecked` でない、整形式の）関数は、どの選択の列でも違反しない。
pub proof fn theorem_soundness(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        !(run_fn(p, o) is Bad),
{
    lemma_init(p);
    lemma_nil_unmarked();
    lemma_block(p.body, p.params, None, ctx0(p), init(p), frame0(p), o, init(p).bind);
    match run_block(p.body, init(p), frame0(p), o).0 {
        Out::Normal(s) => {
            let f = res_fall(chk_block(p.body, p.params, None, ctx0(p))).unwrap();
            lemma_exit(f, s, frame0(p));
        },
        _ => {},
    }
}

// ---- 系：違反の種類ごと（Issue #18 の 5 つの性質） ----

/// 各権限は高々一度だけ消費される。
pub proof fn theorem_consumed_at_most_once(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        run_fn(p, o) != Out::Bad(Viol::Twice),
{
    theorem_soundness(p, o);
}

/// 消費の後には使われない（借用の引数に渡されない。権限のない変数で権限の操作をしない）。
pub proof fn theorem_no_use_after_consume(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        run_fn(p, o) != Out::Bad(Viol::AfterConsume),
        run_fn(p, o) != Out::Bad(Viol::NoPerm),
{
    theorem_soundness(p, o);
}

/// 借用中の権限（`borrow` ブロックの `p` と、`uses &p@` / `uses &mut p@`）は消費されない。
pub proof fn theorem_borrowed_not_consumed(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        run_fn(p, o) != Out::Bad(Viol::ConsumeBorrowed),
{
    theorem_soundness(p, o);
}

/// 共有の借用に書き込まない。
pub proof fn theorem_no_write_to_shared(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        run_fn(p, o) != Out::Bad(Viol::WriteShared),
{
    theorem_soundness(p, o);
}

/// 関数（とクロージャ）の終わりに残る `Owned` は、`gives` の戻り値の名前だけ。
pub proof fn theorem_only_gives_left(p: Program, o: Seq<nat>)
    requires
        wf(p),
        !p.unchecked,
        spec_check(p) == Verdict::Accept,
    ensures
        run_fn(p, o) != Out::Bad(Viol::OwnedLeft),
{
    theorem_soundness(p, o);
}

} // verus!
