//! 検査（仕様 §6.2）。`chk_*` が定義（spec）、`check_*` が実行できる版で、
//! 規則をすべて有効にしたとき（[`full`]）に定義と一致することを Verus が確かめる。
//!
//! 検査は文を順に一度だけ走査し、最初の違反で止まる。複合文の合流の検査（E0510、本体の末尾の E0511、
//! クロージャの末尾の E0505）は、中の文をすべて検査した後に行う。止まる文（`return` など）より後ろの
//! 文は到達しないので検査しない。

use crate::syntax::*;
use vstd::prelude::*;

verus! {

/// 最も内側のループ。`While` / `Loop` の中身は本体の入口の Γ。
#[derive(Clone, PartialEq, Eq, Debug)]
#[verifier::external_derive]
pub enum LoopCtx {
    NoLoop,
    While(Env),
    Loop(Env),
}

#[derive(Clone, PartialEq, Eq, Debug)]
#[verifier::external_derive]
pub struct Ctx {
    /// 今の本体（関数かクロージャ）の `gives`。
    pub gives: Option<usize>,
    pub lp: LoopCtx,
    /// 最も内側のループに入ってから開いた `borrow` の `p` の印（`Absent` でない欄）。`break` /
    /// `continue` で抜ける点で Γ から消す（[`release`]。#21 の既定 D）。
    pub opened: Env,
    /// 今の本体で `borrow mut` の中にいる（`?` の E0003 を見る）。
    pub bm: bool,
    /// 関数の戻り値の型を `Option` / `Result` と書いた（[`Program::try_shape`]）。
    pub try_shape: bool,
    /// クロージャの本体の中（R が空）。
    pub closure: bool,
}

/// 検査の結果。`fall` は文が普通に終わったときの Γ（止まるなら `None`）。
/// `brk` は最も内側の `loop` の、最初の `break` の点の Γ（ループの直後の Γ）。
#[derive(Clone, PartialEq, Eq, Debug)]
#[verifier::external_derive]
pub enum Res {
    Ok { fall: Option<Env>, brk: Option<Env> },
    Err { code: Code, pos: Pos },
}

pub open spec fn ok(e: Env, brk: Option<Env>) -> Res {
    Res::Ok { fall: Some(e), brk }
}

pub open spec fn div(brk: Option<Env>) -> Res {
    Res::Ok { fall: None, brk }
}

pub open spec fn err(code: Code, pos: Pos) -> Res {
    Res::Err { code, pos }
}

pub open spec fn entry_of(lp: LoopCtx) -> Env {
    match lp {
        LoopCtx::NoLoop => Env::Nil,
        LoopCtx::While(e) => e,
        LoopCtx::Loop(e) => e,
    }
}

pub open spec fn blk_slot(mutable: bool) -> Slot {
    if mutable {
        Slot::BlkMut
    } else {
        Slot::BlkShared
    }
}

/// `borrow` ブロックの本体の文脈：`x` に印を付ける。
pub open spec fn borrow_ctx(c: Ctx, x: usize, mutable: bool) -> Ctx {
    Ctx { opened: set(c.opened, x as nat, Slot::Owned), bm: c.bm || mutable, ..c }
}

/// クロージャの本体の文脈（`gives`、ループ、開いた `borrow` を持ち込まない）。
pub open spec fn closure_ctx(c: Ctx) -> Ctx {
    Ctx {
        gives: None,
        lp: LoopCtx::NoLoop,
        opened: Env::Nil,
        bm: false,
        try_shape: c.try_shape,
        closure: true,
    }
}

pub open spec fn chk_stmt(s: Stmt, e: Env, brk: Option<Env>, c: Ctx) -> Res
    decreases s,
{
    let at = Pos::At(s.id);
    match s.op {
        Op::Intro { x, .. } => match get(e, x as nat) {
            Slot::Absent => ok(set(e, x as nat, Slot::Owned), brk),
            Slot::Captured => err(Code::E0512, at),
            _ => err(Code::E0507, at),
        },
        Op::Take { x } => if c.closure {
            err(Code::E0512, at)
        } else {
            match get(e, x as nat) {
                Slot::Absent => ok(set(e, x as nat, Slot::Owned), brk),
                Slot::Captured => err(Code::E0512, at),
                _ => err(Code::E0503, at),
            }
        },
        Op::Consume { kind, x } => if c.closure && kind is Put {
            err(Code::E0512, at)
        } else {
            match get(e, x as nat) {
                Slot::Owned => ok(set(e, x as nat, Slot::Absent), brk),
                Slot::Captured => err(Code::E0512, at),
                Slot::BlkShared | Slot::BlkMut => if kind is Leak {
                    err(Code::E0504, at)
                } else {
                    err(Code::E0509, at)
                },
                _ => err(Code::E0504, at),
            }
        },
        Op::BorrowArg { x, mutable } => match get(e, x as nat) {
            Slot::Absent => err(Code::E0504, at),
            Slot::Captured => err(Code::E0512, at),
            Slot::Shared | Slot::BlkShared => if mutable {
                err(Code::E0508, at)
            } else {
                ok(e, brk)
            },
            _ => ok(e, brk),
        },
        Op::Write { x } => match get(e, x as nat) {
            Slot::Captured => err(Code::E0512, at),
            Slot::Shared | Slot::BlkShared => err(Code::E0508, at),
            _ => ok(e, brk),
        },
        Op::Assign { x } => match get(e, x as nat) {
            Slot::Absent => ok(e, brk),
            Slot::Captured => err(Code::E0512, at),
            _ => err(Code::E0506, at),
        },
        Op::Let { x } => match get(e, x as nat) {
            Slot::Absent => ok(e, brk),
            Slot::Captured => err(Code::E0512, at),
            _ => err(Code::E0507, at),
        },
        Op::Return => if exit_ok(e, c.gives) {
            div(brk)
        } else {
            err(Code::E0505, at)
        },
        Op::Try => if c.bm && !c.try_shape {
            err(Code::E0003, at)
        } else if exit_ok(e, c.gives) {
            ok(e, brk)
        } else {
            err(Code::E0505, at)
        },
        Op::Diverge => div(brk),
        // ループの外の `break` / `continue` は Rust が拒む形で、整形式（`sem::wf`）に入らない。
        // 検査は止まる文として扱うだけで、定理はこの形について何も述べない。
        // 抜ける点で、行き先のループに入ってから開いた `borrow` の `p` を消し、消したあとの Γ で比べる。
        Op::Break => match c.lp {
            LoopCtx::NoLoop => div(brk),
            LoopCtx::While(entry) => if release(e, c.opened) != entry {
                err(Code::E0511, at)
            } else {
                div(brk)
            },
            LoopCtx::Loop(_) => match brk {
                None => div(Some(release(e, c.opened))),
                Some(b) => if release(e, c.opened) == b {
                    div(brk)
                } else {
                    err(Code::E0511, at)
                },
            },
        },
        Op::Continue => match c.lp {
            LoopCtx::NoLoop => div(brk),
            _ => if release(e, c.opened) != entry_of(c.lp) {
                err(Code::E0511, at)
            } else {
                div(brk)
            },
        },
        Op::Branch { a, b, .. } => match chk_block(*a, e, brk, c) {
            Res::Err { code, pos } => err(code, pos),
            Res::Ok { fall: fa, brk: b1 } => match chk_block(*b, e, b1, c) {
                Res::Err { code, pos } => err(code, pos),
                Res::Ok { fall: fb, brk: b2 } => match fa {
                    None => Res::Ok { fall: fb, brk: b2 },
                    Some(x) => match fb {
                        None => ok(x, b2),
                        Some(y) => if x == y {
                            ok(x, b2)
                        } else {
                            err(Code::E0510, Pos::End(s.id))
                        },
                    },
                },
            },
        },
        Op::While { body } => match chk_block(
            *body,
            e,
            None,
            Ctx { lp: LoopCtx::While(e), opened: clear(e), ..c },
        ) {
            Res::Err { code, pos } => err(code, pos),
            Res::Ok { fall, .. } => match fall {
                Some(f) => if f != e {
                    err(Code::E0511, Pos::End(s.id))
                } else {
                    ok(e, brk)
                },
                None => ok(e, brk),
            },
        },
        Op::Loop { body } => match chk_block(
            *body,
            e,
            None,
            Ctx { lp: LoopCtx::Loop(e), opened: clear(e), ..c },
        ) {
            Res::Err { code, pos } => err(code, pos),
            Res::Ok { fall, brk: b2 } => match fall {
                Some(f) => if f != e {
                    err(Code::E0511, Pos::End(s.id))
                } else {
                    Res::Ok { fall: b2, brk }
                },
                None => Res::Ok { fall: b2, brk },
            },
        },
        Op::Borrow { x, mutable, body } => if c.closure {
            err(Code::E0512, at)
        } else {
            match get(e, x as nat) {
                Slot::Absent => match chk_block(
                    *body,
                    set(e, x as nat, blk_slot(mutable)),
                    brk,
                    borrow_ctx(c, x, mutable),
                ) {
                    Res::Err { code, pos } => err(code, pos),
                    Res::Ok { fall, brk: b2 } => match fall {
                        None => div(b2),
                        Some(f) => ok(set(f, x as nat, Slot::Absent), b2),
                    },
                },
                Slot::Captured => err(Code::E0512, at),
                _ => err(Code::E0503, at),
            }
        },
        Op::Closure { body } => match chk_block(
            *body,
            capture(e),
            None,
            closure_ctx(c),
        ) {
            Res::Err { code, pos } => err(code, pos),
            Res::Ok { fall, .. } => match fall {
                Some(f) => if exit_ok(f, None) {
                    ok(e, brk)
                } else {
                    err(Code::E0505, Pos::End(s.id))
                },
                None => ok(e, brk),
            },
        },
    }
}

pub open spec fn chk_block(b: Block, e: Env, brk: Option<Env>, c: Ctx) -> Res
    decreases b,
{
    match b {
        Block::Nil => ok(e, brk),
        Block::Cons(s, rest) => match chk_stmt(*s, e, brk, c) {
            Res::Ok { fall: Some(e2), brk: b2 } => chk_block(*rest, e2, b2, c),
            r => r,
        },
    }
}

pub open spec fn ctx0(p: Program) -> Ctx {
    Ctx {
        gives: p.gives,
        lp: LoopCtx::NoLoop,
        opened: Env::Nil,
        bm: false,
        try_shape: p.try_shape,
        closure: false,
    }
}

/// 関数の判定。本体の末尾で普通に終わるなら、末尾の規則（E0505、位置は `End(0)`）も見る。
pub open spec fn spec_check(p: Program) -> Verdict {
    if p.unchecked {
        Verdict::Accept
    } else {
        match chk_block(p.body, p.params, None, ctx0(p)) {
            Res::Err { code, pos } => Verdict::Reject { code, pos },
            Res::Ok { fall, .. } => match fall {
                Some(f) => if exit_ok(f, p.gives) {
                    Verdict::Accept
                } else {
                    Verdict::Reject { code: Code::E0505, pos: Pos::End(0) }
                },
                None => Verdict::Accept,
            },
        }
    }
}

// ---- 実行できる版と、わざと壊した検査（負の証拠） ----

/// 規則の有無。[`Rules::full`] が正しい検査で、一つを外したものが負の証拠（`formal/README.md`）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rules {
    /// 分岐の終端の Γ を比べる（E0510）。
    pub join: bool,
    /// `borrow` ブロックの `p` の消費を拒む（E0509）。外すと `Owned` と同じに消費できる。
    pub borrow_consume: bool,
    /// `continue` の点の Γ を本体の入口と比べる。
    pub cont: bool,
    /// `break` の点の Γ をループの直後の Γ と比べる。
    pub brk: bool,
    /// 本体の終端の Γ を本体の入口と比べる。
    pub loop_end: bool,
    /// クロージャの本体で Γ を空にする。
    pub closure_reset: bool,
    /// `?` を `return` として扱う。
    pub try_return: bool,
    /// `break` / `continue` で抜ける点で、開いた `borrow` の `p` を Γ から消す（#21 の既定 D）。
    pub release: bool,
    /// 戻り値の型を `Option` / `Result` と書いていない関数の `borrow mut` の中の `?` を拒む（E0003）。
    pub try_shape: bool,
    /// ループに入るときに、開いた `borrow` の印を空にする（外のループの中のループで、外の印を引き継がない）。
    pub loop_clear: bool,
}

pub open spec fn full(r: Rules) -> bool {
    r.join && r.borrow_consume && r.cont && r.brk && r.loop_end && r.closure_reset && r.try_return
        && r.release && r.try_shape && r.loop_clear
}

impl Rules {
    pub fn full() -> (r: Rules)
        ensures
            full(r),
    {
        Rules {
            join: true,
            borrow_consume: true,
            cont: true,
            brk: true,
            loop_end: true,
            closure_reset: true,
            try_return: true,
            release: true,
            try_shape: true,
            loop_clear: true,
        }
    }
}

fn r_ok(e: Env, brk: Option<Env>) -> (r: Res)
    ensures
        r == ok(e, brk),
{
    Res::Ok { fall: Some(e), brk }
}

fn r_err(code: Code, pos: Pos) -> (r: Res)
    ensures
        r == err(code, pos),
{
    Res::Err { code, pos }
}

fn check_atom(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        !(s.op is Branch || s.op is While || s.op is Loop || s.op is Borrow || s.op is Closure),
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
{
    let at = Pos::At(s.id);
    match &s.op {
        Op::Intro { x, .. } => match ex_get(e, *x) {
            Slot::Absent => r_ok(ex_set(e, *x, Slot::Owned), ex_copy_opt(brk)),
            Slot::Captured => r_err(Code::E0512, at),
            _ => r_err(Code::E0507, at),
        },
        Op::Take { x } => if c.closure {
            r_err(Code::E0512, at)
        } else {
            match ex_get(e, *x) {
                Slot::Absent => r_ok(ex_set(e, *x, Slot::Owned), ex_copy_opt(brk)),
                Slot::Captured => r_err(Code::E0512, at),
                _ => r_err(Code::E0503, at),
            }
        },
        Op::Consume { kind, x } => {
            let is_put = match kind {
                ConsumeKind::Put => true,
                _ => false,
            };
            let is_leak = match kind {
                ConsumeKind::Leak => true,
                _ => false,
            };
            if c.closure && is_put {
                r_err(Code::E0512, at)
            } else {
                match ex_get(e, *x) {
                    Slot::Owned => r_ok(ex_set(e, *x, Slot::Absent), ex_copy_opt(brk)),
                    Slot::Captured => r_err(Code::E0512, at),
                    Slot::BlkShared | Slot::BlkMut => if is_leak {
                        r_err(Code::E0504, at)
                    } else if !rules.borrow_consume {
                        r_ok(ex_set(e, *x, Slot::Absent), ex_copy_opt(brk))
                    } else {
                        r_err(Code::E0509, at)
                    },
                    _ => r_err(Code::E0504, at),
                }
            }
        },
        Op::BorrowArg { x, mutable } => match ex_get(e, *x) {
            Slot::Absent => r_err(Code::E0504, at),
            Slot::Captured => r_err(Code::E0512, at),
            Slot::Shared | Slot::BlkShared => if *mutable {
                r_err(Code::E0508, at)
            } else {
                r_ok(ex_copy(e), ex_copy_opt(brk))
            },
            _ => r_ok(ex_copy(e), ex_copy_opt(brk)),
        },
        Op::Write { x } => match ex_get(e, *x) {
            Slot::Captured => r_err(Code::E0512, at),
            Slot::Shared | Slot::BlkShared => r_err(Code::E0508, at),
            _ => r_ok(ex_copy(e), ex_copy_opt(brk)),
        },
        Op::Assign { x } => match ex_get(e, *x) {
            Slot::Absent => r_ok(ex_copy(e), ex_copy_opt(brk)),
            Slot::Captured => r_err(Code::E0512, at),
            _ => r_err(Code::E0506, at),
        },
        Op::Let { x } => match ex_get(e, *x) {
            Slot::Absent => r_ok(ex_copy(e), ex_copy_opt(brk)),
            Slot::Captured => r_err(Code::E0512, at),
            _ => r_err(Code::E0507, at),
        },
        Op::Return => if ex_exit_ok(e, c.gives) {
            Res::Ok { fall: None, brk: ex_copy_opt(brk) }
        } else {
            r_err(Code::E0505, at)
        },
        Op::Try => if rules.try_shape && c.bm && !c.try_shape {
            r_err(Code::E0003, at)
        } else if !rules.try_return || ex_exit_ok(e, c.gives) {
            r_ok(ex_copy(e), ex_copy_opt(brk))
        } else {
            r_err(Code::E0505, at)
        },
        Op::Diverge => Res::Ok { fall: None, brk: ex_copy_opt(brk) },
        Op::Break => {
            let out = if rules.release {
                ex_release(e, &c.opened)
            } else {
                ex_copy(e)
            };
            match &c.lp {
                LoopCtx::NoLoop => Res::Ok { fall: None, brk: ex_copy_opt(brk) },
                LoopCtx::While(entry) => if rules.brk && !ex_eq(&out, entry) {
                    r_err(Code::E0511, at)
                } else {
                    Res::Ok { fall: None, brk: ex_copy_opt(brk) }
                },
                LoopCtx::Loop(_) => match brk {
                    None => Res::Ok { fall: None, brk: Some(out) },
                    Some(b) => if !rules.brk || ex_eq(&out, b) {
                        Res::Ok { fall: None, brk: ex_copy_opt(brk) }
                    } else {
                        r_err(Code::E0511, at)
                    },
                },
            }
        },
        Op::Continue => {
            let out = if rules.release {
                ex_release(e, &c.opened)
            } else {
                ex_copy(e)
            };
            match &c.lp {
                LoopCtx::NoLoop => Res::Ok { fall: None, brk: ex_copy_opt(brk) },
                LoopCtx::While(entry) | LoopCtx::Loop(entry) => if rules.cont && !ex_eq(
                    &out,
                    entry,
                ) {
                    r_err(Code::E0511, at)
                } else {
                    Res::Ok { fall: None, brk: ex_copy_opt(brk) }
                },
            }
        },
        _ => r_err(Code::E0503, at),
    }
}

pub fn check_stmt(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 1nat,
{
    match &s.op {
        Op::Branch { .. } => check_branch(s, e, brk, c, rules),
        Op::While { .. } => check_while(s, e, brk, c, rules),
        Op::Loop { .. } => check_loop(s, e, brk, c, rules),
        Op::Borrow { .. } => check_borrow(s, e, brk, c, rules),
        Op::Closure { .. } => check_closure(s, e, brk, c, rules),
        _ => check_atom(s, e, brk, c, rules),
    }
}

fn check_branch(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        s.op is Branch,
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 0nat,
{
    match &s.op {
        Op::Branch { a, b, .. } => match check_block(a, e, brk, c, rules) {
            Res::Err { code, pos } => r_err(code, pos),
            Res::Ok { fall: fa, brk: b1 } => match check_block(b, e, &b1, c, rules) {
                Res::Err { code, pos } => r_err(code, pos),
                Res::Ok { fall: fb, brk: b2 } => match fa {
                    None => Res::Ok { fall: fb, brk: b2 },
                    Some(x) => match fb {
                        None => r_ok(x, b2),
                        Some(y) => if !rules.join || ex_eq(&x, &y) {
                            r_ok(x, b2)
                        } else {
                            r_err(Code::E0510, Pos::End(s.id))
                        },
                    },
                },
            },
        },
        _ => r_err(Code::E0503, Pos::At(s.id)),
    }
}

fn check_while(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        s.op is While,
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 0nat,
{
    match &s.op {
        Op::While { body } => {
            let c2 = Ctx {
                gives: c.gives,
                lp: LoopCtx::While(ex_copy(e)),
                // 壊した検査（`no-loop-clear`）は、外のループの中のループで外の印を引き継ぐ。
                opened: match (&c.lp, rules.loop_clear) {
                    (LoopCtx::While(_) | LoopCtx::Loop(_), false) => ex_copy(&c.opened),
                    _ => ex_clear(e),
                },
                bm: c.bm,
                try_shape: c.try_shape,
                closure: c.closure,
            };
            match check_block(body, e, &None, &c2, rules) {
                Res::Err { code, pos } => r_err(code, pos),
                Res::Ok { fall, .. } => match fall {
                    Some(f) => if rules.loop_end && !ex_eq(&f, e) {
                        r_err(Code::E0511, Pos::End(s.id))
                    } else {
                        r_ok(ex_copy(e), ex_copy_opt(brk))
                    },
                    None => r_ok(ex_copy(e), ex_copy_opt(brk)),
                },
            }
        },
        _ => r_err(Code::E0503, Pos::At(s.id)),
    }
}

fn check_loop(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        s.op is Loop,
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 0nat,
{
    match &s.op {
        Op::Loop { body } => {
            let c2 = Ctx {
                gives: c.gives,
                lp: LoopCtx::Loop(ex_copy(e)),
                // 壊した検査（`no-loop-clear`）は、外のループの中のループで外の印を引き継ぐ。
                opened: match (&c.lp, rules.loop_clear) {
                    (LoopCtx::While(_) | LoopCtx::Loop(_), false) => ex_copy(&c.opened),
                    _ => ex_clear(e),
                },
                bm: c.bm,
                try_shape: c.try_shape,
                closure: c.closure,
            };
            match check_block(body, e, &None, &c2, rules) {
                Res::Err { code, pos } => r_err(code, pos),
                Res::Ok { fall, brk: b2 } => match fall {
                    Some(f) => if rules.loop_end && !ex_eq(&f, e) {
                        r_err(Code::E0511, Pos::End(s.id))
                    } else {
                        Res::Ok { fall: b2, brk: ex_copy_opt(brk) }
                    },
                    None => Res::Ok { fall: b2, brk: ex_copy_opt(brk) },
                },
            }
        },
        _ => r_err(Code::E0503, Pos::At(s.id)),
    }
}

fn check_borrow(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        s.op is Borrow,
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 0nat,
{
    match &s.op {
        Op::Borrow { x, mutable, body } => if c.closure {
            r_err(Code::E0512, Pos::At(s.id))
        } else {
            match ex_get(e, *x) {
                Slot::Absent => {
                    let k = if *mutable {
                        Slot::BlkMut
                    } else {
                        Slot::BlkShared
                    };
                    let c2 = Ctx {
                        gives: c.gives,
                        lp: match &c.lp {
                            LoopCtx::NoLoop => LoopCtx::NoLoop,
                            LoopCtx::While(x) => LoopCtx::While(ex_copy(x)),
                            LoopCtx::Loop(x) => LoopCtx::Loop(ex_copy(x)),
                        },
                        opened: ex_set(&c.opened, *x, Slot::Owned),
                        bm: c.bm || *mutable,
                        try_shape: c.try_shape,
                        closure: c.closure,
                    };
                    match check_block(body, &ex_set(e, *x, k), brk, &c2, rules) {
                        Res::Err { code, pos } => r_err(code, pos),
                        Res::Ok { fall, brk: b2 } => match fall {
                            None => Res::Ok { fall: None, brk: b2 },
                            Some(f) => r_ok(ex_set(&f, *x, Slot::Absent), b2),
                        },
                    }
                },
                Slot::Captured => r_err(Code::E0512, Pos::At(s.id)),
                _ => r_err(Code::E0503, Pos::At(s.id)),
            }
        },
        _ => r_err(Code::E0503, Pos::At(s.id)),
    }
}

fn check_closure(s: &Stmt, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    requires
        s.op is Closure,
    ensures
        full(*rules) ==> r == chk_stmt(*s, *e, *brk, *c),
    decreases s, 0nat,
{
    match &s.op {
        Op::Closure { body } => {
            let c2 = Ctx {
                gives: None,
                lp: LoopCtx::NoLoop,
                opened: Env::Nil,
                bm: false,
                try_shape: c.try_shape,
                closure: true,
            };
            let start = if rules.closure_reset {
                ex_capture(e)
            } else {
                ex_copy(e)
            };
            match check_block(body, &start, &None, &c2, rules) {
                Res::Err { code, pos } => r_err(code, pos),
                Res::Ok { fall, .. } => match fall {
                    Some(f) => if ex_exit_ok(&f, None) {
                        r_ok(ex_copy(e), ex_copy_opt(brk))
                    } else {
                        r_err(Code::E0505, Pos::End(s.id))
                    },
                    None => r_ok(ex_copy(e), ex_copy_opt(brk)),
                },
            }
        },
        _ => r_err(Code::E0503, Pos::At(s.id)),
    }
}

pub fn check_block(b: &Block, e: &Env, brk: &Option<Env>, c: &Ctx, rules: &Rules) -> (r: Res)
    ensures
        full(*rules) ==> r == chk_block(*b, *e, *brk, *c),
    decreases b, 1nat,
{
    match b {
        Block::Nil => r_ok(ex_copy(e), ex_copy_opt(brk)),
        Block::Cons(s, rest) => match check_stmt(s, e, brk, c, rules) {
            Res::Ok { fall: Some(e2), brk: b2 } => check_block(rest, &e2, &b2, c, rules),
            r => r,
        },
    }
}

/// 規則 `rules` で関数を判定する。
pub fn check_with(p: &Program, rules: &Rules) -> (r: Verdict)
    ensures
        full(*rules) ==> r == spec_check(*p),
{
    if p.unchecked {
        return Verdict::Accept;
    }
    let c = Ctx {
        gives: p.gives,
        lp: LoopCtx::NoLoop,
        opened: Env::Nil,
        bm: false,
        try_shape: p.try_shape,
        closure: false,
    };
    match check_block(&p.body, &p.params, &None, &c, rules) {
        Res::Err { code, pos } => Verdict::Reject { code, pos },
        Res::Ok { fall, .. } => match fall {
            Some(f) => if ex_exit_ok(&f, p.gives) {
                Verdict::Accept
            } else {
                Verdict::Reject { code: Code::E0505, pos: Pos::End(0) }
            },
            None => Verdict::Accept,
        },
    }
}

/// 正しい検査。
pub fn check(p: &Program) -> (r: Verdict)
    ensures
        r == spec_check(*p),
{
    check_with(p, &Rules::full())
}

} // verus!
