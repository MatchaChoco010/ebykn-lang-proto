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

pub open spec fn post(out: Out, r: Res, c: Ctx, st: St, fr: Frame) -> bool {
    match out {
        Out::Bad(_) => false,
        Out::Normal(s2) => res_fall(r) is Some && inv(res_fall(r).unwrap(), s2, fr) && ext(st, s2, fr),
        Out::Brk(s2) => !(c.lp is NoLoop) && !c.crossed && (c.lp is Loop ==> res_brk(r) is Some)
            && inv(brk_target(c, r), s2, fr) && ext(st, s2, fr),
        Out::Cont(s2) => !(c.lp is NoLoop) && !c.crossed && inv(entry_of(c.lp), s2, fr) && ext(
            st,
            s2,
            fr,
        ),
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
            lemma_mono_block(
                *body,
                set(e, x as nat, blk_slot(mutable)),
                b,
                Ctx { crossed: true, ..c },
            );
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

/// 借用の欄は変わらない（消費も再束縛も拒まれる）。出口の Γ と、新しく決まった `break` の Γ で同じ。
pub proof fn lemma_frozen_stmt(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, y: nat)
    requires
        chk_stmt(s, e, brk, c) is Ok,
        borrowed(get(e, y)),
    ensures
        res_fall(chk_stmt(s, e, brk, c)) matches Some(f) ==> get(f, y) == get(e, y),
        brk is None ==> (res_brk(chk_stmt(s, e, brk, c)) matches Some(b) ==> get(b, y) == get(e, y)),
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
                Ctx { lp: LoopCtx::Loop(e), crossed: false, ..c },
                y,
            );
        },
        Op::Borrow { x, mutable, body } => {
            let e1 = set(e, x as nat, blk_slot(mutable));
            lemma_get_set(e, x as nat, blk_slot(mutable), y);
            lemma_frozen_block(*body, e1, brk, Ctx { crossed: true, ..c }, y);
            match res_fall(chk_block(*body, e1, brk, Ctx { crossed: true, ..c })) {
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
            e,
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

pub open spec fn pre(s_wf: bool, r: Res, e: Env, c: Ctx, st: St, fr: Frame) -> bool {
    &&& s_wf
    &&& r is Ok
    &&& inv(e, st, fr)
    &&& c.gives == fr.gives
}

proof fn lemma_atom(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
        !(s.op is Branch || s.op is While || s.op is Loop || s.op is Borrow || s.op is Closure),
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
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
        _ => {},
    }
}

proof fn lemma_branch(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
        s.op is Branch,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
    decreases s, 0nat,
{
    if let Op::Branch { a, b, .. } = s.op {
        let ra = chk_block(*a, e, brk, c);
        let b1 = res_brk(ra);
        let rb = chk_block(*b, e, b1, c);
        if choice(o) == 0 {
            lemma_block(*a, e, brk, c, st, fr, rest(o));
            if b1 is Some {
                lemma_mono_block(*b, e, b1.unwrap(), c);
            }
        } else {
            lemma_block(*b, e, b1, c, st, fr, rest(o));
        }
    }
}

proof fn lemma_while(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
        s.op is While || s.op is Loop,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
    decreases s, 0nat,
{
    match s.op {
        Op::While { body } => lemma_iter(
            *body,
            e,
            Ctx { lp: LoopCtx::While(e), crossed: false, ..c },
            st,
            fr,
            rest(o),
            choice(o),
            true,
        ),
        Op::Loop { body } => lemma_iter(
            *body,
            e,
            Ctx { lp: LoopCtx::Loop(e), crossed: false, ..c },
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
        !c.crossed,
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
        lemma_block(body, e, None, c, st, fr, o);
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
            Out::Cont(s) => {
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

proof fn lemma_borrow(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
        s.op is Borrow,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
    decreases s, 0nat,
{
    if let Op::Borrow { x, mutable, body } = s.op {
        let k = blk_slot(mutable);
        let e1 = set(e, x as nat, k);
        let c3 = Ctx { crossed: true, ..c };
        let st1 = fresh(st, x, kind_of(mutable));
        let old = st.bind[x as int];
        assert(rel(get(e, x as nat), old, st.toks));
        assert(bound_ok(old, fr.base, st.toks.len()));
        lemma_fresh(e, st, fr, x, k, kind_of(mutable));
        lemma_block(*body, e1, brk, c3, st1, fr, o);
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
            Out::Ret(s2) => lemma_ext_trans(st, st1, s2, fr),
            _ => {},
        }
    }
}

proof fn lemma_closure(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
        s.op is Closure,
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
    decreases s, 0nat,
{
    if let Op::Closure { body } = s.op {
        lemma_calls(*body, e, st, fr, rest(o), choice(o));
    }
}

pub open spec fn closure_ctx() -> Ctx {
    Ctx { gives: None, lp: LoopCtx::NoLoop, crossed: false, closure: true }
}

/// クロージャを `k` 回呼ぶ。外の Γ と不変条件は変わらない。
proof fn lemma_calls(body: Block, e: Env, st: St, fr: Frame, o: Seq<nat>, k: nat)
    requires
        wf_block(body, st.bind.len(), false),
        chk_block(body, capture(e), None, closure_ctx()) is Ok,
        res_fall(chk_block(body, capture(e), None, closure_ctx())) matches Some(f) ==> exit_ok(
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
        lemma_block(body, capture(e), None, closure_ctx(), st2, fr2, o);
        let (out, o2) = run_block(body, st2, fr2, o);
        match out {
            Out::Normal(s) => {
                let f = res_fall(chk_block(body, capture(e), None, closure_ctx())).unwrap();
                lemma_exit(f, s, fr2);
                lemma_closure_back(e, st, fr, s);
                let s3 = St { bind: st.bind, toks: s.toks };
                lemma_calls(body, e, s3, fr, o2, (k - 1) as nat);
                match run_closure(body, s3, fr, o2, (k - 1) as nat).0 {
                    Out::Normal(s4) => lemma_ext_trans(st, s3, s4, fr),
                    _ => {},
                }
            },
            Out::Ret(s) => {
                lemma_closure_back(e, st, fr, s);
                let s3 = St { bind: st.bind, toks: s.toks };
                lemma_calls(body, e, s3, fr, o2, (k - 1) as nat);
                match run_closure(body, s3, fr, o2, (k - 1) as nat).0 {
                    Out::Normal(s4) => lemma_ext_trans(st, s3, s4, fr),
                    _ => {},
                }
            },
            _ => {},
        }
    }
}

pub proof fn lemma_stmt(s: Stmt, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_stmt(s, st.bind.len(), !(c.lp is NoLoop)), chk_stmt(s, e, brk, c), e, c, st, fr),
    ensures
        post(run_stmt(s, st, fr, o).0, chk_stmt(s, e, brk, c), c, st, fr),
    decreases s, 1nat,
{
    match s.op {
        Op::Branch { .. } => lemma_branch(s, e, brk, c, st, fr, o),
        Op::While { .. } => lemma_while(s, e, brk, c, st, fr, o),
        Op::Loop { .. } => lemma_while(s, e, brk, c, st, fr, o),
        Op::Borrow { .. } => lemma_borrow(s, e, brk, c, st, fr, o),
        Op::Closure { .. } => lemma_closure(s, e, brk, c, st, fr, o),
        _ => lemma_atom(s, e, brk, c, st, fr, o),
    }
}

pub proof fn lemma_block(b: Block, e: Env, brk: Option<Env>, c: Ctx, st: St, fr: Frame, o: Seq<nat>)
    requires
        pre(wf_block(b, st.bind.len(), !(c.lp is NoLoop)), chk_block(b, e, brk, c), e, c, st, fr),
    ensures
        post(run_block(b, st, fr, o).0, chk_block(b, e, brk, c), c, st, fr),
    decreases b, 0nat,
{
    lemma_ext_refl(st, fr);
    match b {
        Block::Nil => {},
        Block::Cons(s, r) => {
            lemma_stmt(*s, e, brk, c, st, fr, o);
            let rs = chk_stmt(*s, e, brk, c);
            let (out, o2) = run_stmt(*s, st, fr, o);
            match out {
                Out::Normal(s2) => {
                    let e2 = res_fall(rs).unwrap();
                    let b2 = res_brk(rs);
                    lemma_block(*r, e2, b2, c, s2, fr, o2);
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
    lemma_block(p.body, p.params, None, ctx0(p), init(p), frame0(p), o);
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
