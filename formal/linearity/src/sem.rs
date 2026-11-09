//! 権限の操作だけを追う実行の意味。
//!
//! 権限を「札」（token）として数え、変数は札に束縛される。分岐でどちらを通るか、ループを何回回るか、
//! `?` で戻るか、クロージャを何回呼ぶかは選択の列 `o` で決める（どの選択も取りうる）。違反に出会うと
//! `Bad` になる。検査の Γ は見ない（`*p` を権限付きの書き込みとみなすのは、`p` が生きた札に束縛されて
//! いるときだけ）。

// Verus なしのビルドでは spec と proof が消えて使われなくなる。
#[allow(unused_imports)]
use crate::syntax::*;
use vstd::prelude::*;

verus! {

pub enum Kind {
    Owned,
    Shared,
    Mut,
}

pub enum Tok {
    Live(Kind),
    /// 消費された札。
    Dead,
}

/// 実行の状態。`bind[x]` は変数 `x` が束縛されている札、`toks[t]` は札 `t` の状態。
pub struct St {
    pub bind: Seq<Option<nat>>,
    pub toks: Seq<Tok>,
}

/// 今の本体（関数かクロージャ）。`base` より前の札は外の本体のもの。
pub struct Frame {
    pub base: nat,
    pub gives: Option<usize>,
}

/// 違反。定理は、検査が受理した関数でこれが起きないことを示す。
pub enum Viol {
    /// 権限に束縛されていない変数で、権限が要る操作をした。
    NoPerm,
    /// 消費した権限をもう一度消費した。
    Twice,
    /// 消費した権限を借用の引数に渡した。
    AfterConsume,
    /// 借用の権限（`uses &p@` / `uses &mut p@` / `borrow` ブロックの `p`）を消費した。
    ConsumeBorrowed,
    /// 共有の借用に書き込んだ（`*p = e` か、`uses &mut p'@` の引数に渡した）。
    WriteShared,
    /// 本体を出るとき、`gives` の名前に束縛されていない `Owned` の札が残った。
    OwnedLeft,
}

pub enum Out {
    Normal(St),
    Brk(St),
    Cont(St),
    Ret(St),
    /// 発散した（`panic!` など）か、`loop` を指定の回数回っても抜けなかった（実行の途中）。
    Halt,
    Bad(Viol),
}

pub open spec fn choice(o: Seq<nat>) -> nat {
    if o.len() > 0 {
        o[0]
    } else {
        0
    }
}

pub open spec fn rest(o: Seq<nat>) -> Seq<nat> {
    if o.len() > 0 {
        o.drop_first()
    } else {
        o
    }
}

pub open spec fn fresh(st: St, x: usize, k: Kind) -> St {
    St { bind: st.bind.update(x as int, Some(st.toks.len())), toks: st.toks.push(Tok::Live(k)) }
}

pub open spec fn unbind(st: St, x: usize) -> St {
    St { bind: st.bind.update(x as int, None), toks: st.toks }
}

pub open spec fn consume(st: St, x: usize) -> Out {
    match st.bind[x as int] {
        None => Out::Bad(Viol::NoPerm),
        Some(t) => match st.toks[t as int] {
            Tok::Dead => Out::Bad(Viol::Twice),
            Tok::Live(Kind::Owned) => Out::Normal(
                St { bind: st.bind, toks: st.toks.update(t as int, Tok::Dead) },
            ),
            Tok::Live(_) => Out::Bad(Viol::ConsumeBorrowed),
        },
    }
}

pub open spec fn borrow_arg(st: St, x: usize, mutable: bool) -> Out {
    match st.bind[x as int] {
        None => Out::Bad(Viol::NoPerm),
        Some(t) => match st.toks[t as int] {
            Tok::Dead => Out::Bad(Viol::AfterConsume),
            Tok::Live(Kind::Shared) => if mutable {
                Out::Bad(Viol::WriteShared)
            } else {
                Out::Normal(st)
            },
            Tok::Live(_) => Out::Normal(st),
        },
    }
}

/// `*p = e`。`p` が生きた札に束縛されていなければ Rust の書き込みで、権限は関わらない。
pub open spec fn write(st: St, x: usize) -> Out {
    match st.bind[x as int] {
        Some(t) => if st.toks[t as int] == Tok::Live(Kind::Shared) {
            Out::Bad(Viol::WriteShared)
        } else {
            Out::Normal(st)
        },
        None => Out::Normal(st),
    }
}

/// 本体を出てよいか：この本体の `Owned` の札は、すべて `gives` の名前に束縛されている。
pub open spec fn exit_sem(st: St, fr: Frame) -> bool {
    forall|t: nat|
        #![trigger st.toks[t as int]]
        fr.base <= t < st.toks.len() && st.toks[t as int] == Tok::Live(Kind::Owned) ==> (
        match fr.gives {
            Some(g) => (g as int) < st.bind.len() && st.bind[g as int] == Some(t),
            None => false,
        })
}

pub open spec fn ret(st: St, fr: Frame) -> Out {
    if exit_sem(st, fr) {
        Out::Ret(st)
    } else {
        Out::Bad(Viol::OwnedLeft)
    }
}

/// `borrow` ブロックを出るとき（どの出方でも）、`x` の束縛を入る前に戻す。
pub open spec fn restore(out: Out, x: usize, old: Option<nat>) -> Out {
    match out {
        Out::Normal(s) => Out::Normal(St { bind: s.bind.update(x as int, old), toks: s.toks }),
        Out::Brk(s) => Out::Brk(St { bind: s.bind.update(x as int, old), toks: s.toks }),
        Out::Cont(s) => Out::Cont(St { bind: s.bind.update(x as int, old), toks: s.toks }),
        other => other,
    }
}

pub open spec fn kind_of(mutable: bool) -> Kind {
    if mutable {
        Kind::Mut
    } else {
        Kind::Shared
    }
}

pub open spec fn run_stmt(s: Stmt, st: St, fr: Frame, o: Seq<nat>) -> (Out, Seq<nat>)
    decreases s, 0nat,
{
    match s.op {
        Op::Intro { x, .. } => (Out::Normal(fresh(st, x, Kind::Owned)), o),
        Op::Take { x } => (Out::Normal(fresh(st, x, Kind::Owned)), o),
        Op::Consume { x, .. } => (consume(st, x), o),
        Op::BorrowArg { x, mutable } => (borrow_arg(st, x, mutable), o),
        Op::Write { x } => (write(st, x), o),
        Op::Assign { x } => (Out::Normal(unbind(st, x)), o),
        Op::Let { x } => (Out::Normal(unbind(st, x)), o),
        Op::Return => (ret(st, fr), o),
        Op::Try => if choice(o) == 0 {
            (Out::Normal(st), rest(o))
        } else {
            (ret(st, fr), rest(o))
        },
        Op::Diverge => (Out::Halt, o),
        Op::Break => (Out::Brk(st), o),
        Op::Continue => (Out::Cont(st), o),
        Op::Branch { a, b, .. } => if choice(o) == 0 {
            run_block(*a, st, fr, rest(o))
        } else {
            run_block(*b, st, fr, rest(o))
        },
        Op::While { body } => run_loop(*body, st, fr, rest(o), choice(o), true),
        Op::Loop { body } => run_loop(*body, st, fr, rest(o), choice(o), false),
        Op::Borrow { x, mutable, body } => {
            let (out, o2) = run_block(*body, fresh(st, x, kind_of(mutable)), fr, o);
            (restore(out, x, st.bind[x as int]), o2)
        },
        Op::Closure { body } => run_closure(*body, st, fr, rest(o), choice(o)),
    }
}

pub open spec fn run_block(b: Block, st: St, fr: Frame, o: Seq<nat>) -> (Out, Seq<nat>)
    decreases b, 0nat,
{
    match b {
        Block::Nil => (Out::Normal(st), o),
        Block::Cons(s, r) => {
            let (out, o2) = run_stmt(*s, st, fr, o);
            match out {
                Out::Normal(st2) => run_block(*r, st2, fr, o2),
                other => (other, o2),
            }
        },
    }
}

/// 本体を `k` 回まで回す。`while` は先頭で抜けられ、`loop` は `break` でだけ抜ける。
pub open spec fn run_loop(body: Block, st: St, fr: Frame, o: Seq<nat>, k: nat, exits: bool) -> (
    Out,
    Seq<nat>,
)
    decreases body, k,
{
    if k == 0 {
        if exits {
            (Out::Normal(st), o)
        } else {
            (Out::Halt, o)
        }
    } else {
        let (out, o2) = run_block(body, st, fr, o);
        match out {
            Out::Normal(s) => run_loop(body, s, fr, o2, (k - 1) as nat, exits),
            Out::Cont(s) => run_loop(body, s, fr, o2, (k - 1) as nat, exits),
            Out::Brk(s) => (Out::Normal(s), o2),
            other => (other, o2),
        }
    }
}

pub open spec fn closure_frame(st: St) -> Frame {
    Frame { base: st.toks.len(), gives: None }
}

pub open spec fn closure_start(st: St) -> St {
    St { bind: Seq::new(st.bind.len(), |i: int| None), toks: st.toks }
}

/// クロージャを定義した位置で `k` 回呼ぶ。呼び出しは外の変数の束縛が見えない新しい本体で、
/// 札の表だけを外と共有する。
pub open spec fn run_closure(body: Block, st: St, fr: Frame, o: Seq<nat>, k: nat) -> (
    Out,
    Seq<nat>,
)
    decreases body, k,
{
    if k == 0 {
        (Out::Normal(st), o)
    } else {
        let fr2 = closure_frame(st);
        let (out, o2) = run_block(body, closure_start(st), fr2, o);
        match out {
            Out::Normal(s) => if exit_sem(s, fr2) {
                run_closure(body, St { bind: st.bind, toks: s.toks }, fr, o2, (k - 1) as nat)
            } else {
                (Out::Bad(Viol::OwnedLeft), o2)
            },
            Out::Ret(s) => run_closure(
                body,
                St { bind: st.bind, toks: s.toks },
                fr,
                o2,
                (k - 1) as nat,
            ),
            Out::Bad(v) => (Out::Bad(v), o2),
            _ => (Out::Halt, o2),
        }
    }
}

pub open spec fn tok_of(s: Slot) -> Tok {
    match s {
        Slot::Owned => Tok::Live(Kind::Owned),
        Slot::Shared => Tok::Live(Kind::Shared),
        Slot::Mut => Tok::Live(Kind::Mut),
        _ => Tok::Dead,
    }
}

/// 関数に入った直後：`uses` の引数 `x` は札 `x` に束縛される。
pub open spec fn init(p: Program) -> St {
    St {
        bind: Seq::new(
            len(p.params),
            |i: int|
                if get(p.params, i as nat) is Absent {
                    None
                } else {
                    Some(i as nat)
                },
        ),
        toks: Seq::new(len(p.params), |i: int| tok_of(get(p.params, i as nat))),
    }
}

pub open spec fn frame0(p: Program) -> Frame {
    Frame { base: 0, gives: p.gives }
}

/// 関数を選択の列 `o` に沿って実行する。本体の末尾で普通に終われば、末尾の規則を見る。
pub open spec fn run_fn(p: Program, o: Seq<nat>) -> Out {
    let (out, _) = run_block(p.body, init(p), frame0(p), o);
    match out {
        Out::Normal(s) => ret(s, frame0(p)),
        Out::Brk(_) => Out::Halt,
        Out::Cont(_) => Out::Halt,
        other => other,
    }
}

// ---- 整形式：変数の添字が範囲にあり、引数の欄が `uses` から作れる形 ----

pub open spec fn var_ok(x: usize, n: nat) -> bool {
    (x as nat) < n
}

pub open spec fn wf_stmt(s: Stmt, n: nat) -> bool
    decreases s,
{
    match s.op {
        Op::Intro { x, .. } => var_ok(x, n),
        Op::Take { x } => var_ok(x, n),
        Op::Consume { x, .. } => var_ok(x, n),
        Op::BorrowArg { x, .. } => var_ok(x, n),
        Op::Write { x } => var_ok(x, n),
        Op::Assign { x } => var_ok(x, n),
        Op::Let { x } => var_ok(x, n),
        Op::Branch { a, b, .. } => wf_block(*a, n) && wf_block(*b, n),
        Op::While { body } => wf_block(*body, n),
        Op::Loop { body } => wf_block(*body, n),
        Op::Borrow { x, body, .. } => var_ok(x, n) && wf_block(*body, n),
        Op::Closure { body } => wf_block(*body, n),
        _ => true,
    }
}

pub open spec fn wf_block(b: Block, n: nat) -> bool
    decreases b,
{
    match b {
        Block::Nil => true,
        Block::Cons(s, r) => wf_stmt(*s, n) && wf_block(*r, n),
    }
}

pub open spec fn param_slot(s: Slot) -> bool {
    s is Absent || s is Owned || s is Shared || s is Mut
}

pub open spec fn wf(p: Program) -> bool {
    &&& forall|i: nat| i < len(p.params) ==> #[trigger] param_slot(get(p.params, i))
    &&& wf_block(p.body, len(p.params))
    &&& match p.gives {
        Some(g) => var_ok(g, len(p.params)),
        None => true,
    }
}

} // verus!
