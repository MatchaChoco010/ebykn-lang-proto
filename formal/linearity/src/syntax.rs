//! 抽象した文の言語（仕様 §6.2 の判定に要る情報だけを持つ）。
//!
//! 式と型は持たない。変数は添字（`usize`）で、領域は関数の引数の一つ（`uses r: &mut Region<..>`）
//! だけを考える。文の `id` は診断の位置に使う（`formal/README.md`）。

use vstd::prelude::*;

verus! {

/// 環境 Γ の一つの欄。仕様の `State` を、検査に要る範囲で細かく分けたもの。
///
/// - `Shared` / `Mut` は引数の `uses &p@` / `uses &mut p@`、`BlkShared` / `BlkMut` は
///   `borrow` / `borrow mut` ブロックの `p`。仕様ではどちらも `BorrowedShared` / `BorrowedMut`。
///   出どころは途中で変わらないので、合流の比較（等しさ）の結果は仕様と同じになる。
/// - `Captured` はクロージャの本体で、外の Γ にある変数（仕様では Γ を空にしたので「Γ にない」）。
///   E0512 を出すために区別する。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Slot {
    Absent,
    Owned,
    Shared,
    Mut,
    BlkShared,
    BlkMut,
    Captured,
}

/// 導入の形。検査と意味ではどれも同じで、`.eyk` の断片を書き出すときだけ使う。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum IntroKind {
    /// `let p = Ptr::new(0);`
    PtrNew,
    /// `let p = Ptr::empty();`
    PtrEmpty,
    /// `let p = Cell::new(0);`
    CellNew,
    /// `let p = make();`（`make` は `gives` を持つ）
    Gives,
}

/// `Owned` を要る消費の形。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum ConsumeKind {
    /// `p.free();`
    Free,
    /// `put r[p];`
    Put,
    /// `consume(p);`（引数が `uses p'@`）
    Move,
    /// `leak p@;`
    Leak,
}

/// 分岐の形。`LetElse` は `a` が空で、`b` が `else` のブロック。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum BranchKind {
    IfElse,
    IfLet,
    Match,
    LetElse,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[verifier::external_derive]
#[allow(inconsistent_fields)]
pub enum Op {
    /// `Ptr::new` / `Ptr::empty` / `Cell::new` / `gives` を持つ関数の呼び出しの `let`。
    Intro { kind: IntroKind, x: usize },
    /// `take r[p];`
    Take { x: usize },
    Consume { kind: ConsumeKind, x: usize },
    /// `inspect(p)`（`uses &p'@`）/ `update(p)`（`uses &mut p'@`）。
    BorrowArg { x: usize, mutable: bool },
    /// `*p = 1;`
    Write { x: usize },
    /// `p = other();`
    Assign { x: usize },
    /// `let p = other();`
    Let { x: usize },
    Return,
    /// `fallible()?;`
    Try,
    /// `panic!();`
    Diverge,
    Break,
    Continue,
    Branch { kind: BranchKind, a: Box<Block>, b: Box<Block> },
    /// `while cond() { .. }`（先頭で抜けられる）
    While { body: Box<Block> },
    /// `loop { .. }`（`break` でだけ抜ける）
    Loop { body: Box<Block> },
    Borrow { x: usize, mutable: bool, body: Box<Block> },
    /// `let f = || { .. };`
    Closure { body: Box<Block> },
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[verifier::external_derive]
pub struct Stmt {
    pub id: u32,
    pub op: Op,
}

#[derive(Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[verifier::external_derive]
pub enum Block {
    Nil,
    Cons(Box<Stmt>, Box<Block>),
}

/// 環境 Γ。`i` 番目の要素が変数 `i` の欄。長さは関数の変数の数で、検査の間変わらない。
#[derive(Clone, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
#[verifier::external_derive]
pub enum Env {
    Nil,
    Cons(Slot, Box<Env>),
}

/// 検査する関数。`params` は引数の `uses` から作る最初の Γ（`Absent` / `Owned` / `Shared` / `Mut`）。
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
#[verifier::external_derive]
pub struct Program {
    pub params: Env,
    /// `gives` の戻り値の名前（変数）。
    pub gives: Option<usize>,
    /// `unchecked fn`。本体を検査しない。
    pub unchecked: bool,
    /// 戻り値の型を `Option<..>` / `Result<..>` と書いた（`borrow mut` の中の `?` を書き下しで展開できる。
    /// 書いていなければ E0003。#21）。
    pub try_shape: bool,
    pub body: Block,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Code {
    /// 未実装（ここでは、戻り値の型を `Option` / `Result` と書いていない関数の `borrow mut` の中の `?`）。
    E0003,
    E0503,
    E0504,
    E0505,
    E0506,
    E0507,
    E0508,
    E0509,
    E0510,
    E0511,
    E0512,
}

/// 診断の位置。`At(id)` は文 `id` の先頭、`End(id)` は複合文 `id` の閉じ括弧（`End(0)` は関数本体の閉じ括弧）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord)]
pub enum Pos {
    At(u32),
    End(u32),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Verdict {
    Accept,
    Reject { code: Code, pos: Pos },
}

// ---- 長さ・読み・書き（spec） ----

pub open spec fn len(e: Env) -> nat
    decreases e,
{
    match e {
        Env::Nil => 0,
        Env::Cons(_, t) => 1 + len(*t),
    }
}

/// 範囲の外は `Absent`。
pub open spec fn get(e: Env, x: nat) -> Slot
    decreases e,
{
    match e {
        Env::Nil => Slot::Absent,
        Env::Cons(s, t) => if x == 0 {
            s
        } else {
            get(*t, (x - 1) as nat)
        },
    }
}

/// 範囲の外への書き込みは何もしない。
pub open spec fn set(e: Env, x: nat, v: Slot) -> Env
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => if x == 0 {
            Env::Cons(v, t)
        } else {
            Env::Cons(s, Box::new(set(*t, (x - 1) as nat, v)))
        },
    }
}

/// クロージャの本体の最初の Γ：外の Γ にある変数を `Captured` にする。
pub open spec fn capture(e: Env) -> Env
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => Env::Cons(
            if s is Absent {
                Slot::Absent
            } else {
                Slot::Captured
            },
            Box::new(capture(*t)),
        ),
    }
}

/// 同じ長さで、すべて `Absent` の Γ（ループに入った時点の、開いた `borrow` の印）。
pub open spec fn clear(e: Env) -> Env
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(_, t) => Env::Cons(Slot::Absent, Box::new(clear(*t))),
    }
}

pub open spec fn head(m: Env) -> Slot {
    match m {
        Env::Nil => Slot::Absent,
        Env::Cons(s, _) => s,
    }
}

pub open spec fn tail(m: Env) -> Env {
    match m {
        Env::Nil => Env::Nil,
        Env::Cons(_, t) => *t,
    }
}

/// `break` / `continue` で抜ける点の Γ：印 `m` の付いた変数（行き先のループに入ってから開いた
/// `borrow` の `p`）を消す。ブロックの終わりと同じ扱い（#21 の既定 D）。
pub open spec fn release(e: Env, m: Env) -> Env
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => Env::Cons(
            if head(m) is Absent {
                s
            } else {
                Slot::Absent
            },
            Box::new(release(*t, tail(m))),
        ),
    }
}

pub open spec fn no_owned(e: Env) -> bool
    decreases e,
{
    match e {
        Env::Nil => true,
        Env::Cons(s, t) => !(s is Owned) && no_owned(*t),
    }
}

/// `return` と本体の末尾の規則：`Owned` は `gives` の名前だけ。
pub open spec fn exit_ok(e: Env, gives: Option<usize>) -> bool {
    match gives {
        None => no_owned(e),
        Some(g) => no_owned(set(e, g as nat, Slot::Absent)),
    }
}

// ---- 実行できる版 ----

pub fn ex_copy(e: &Env) -> (r: Env)
    ensures
        r == *e,
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => Env::Cons(*s, Box::new(ex_copy(t))),
    }
}

pub fn ex_copy_opt(e: &Option<Env>) -> (r: Option<Env>)
    ensures
        r == *e,
{
    match e {
        None => None,
        Some(x) => Some(ex_copy(x)),
    }
}

pub fn ex_slot_eq(a: Slot, b: Slot) -> (r: bool)
    ensures
        r == (a == b),
{
    match (a, b) {
        (Slot::Absent, Slot::Absent) => true,
        (Slot::Owned, Slot::Owned) => true,
        (Slot::Shared, Slot::Shared) => true,
        (Slot::Mut, Slot::Mut) => true,
        (Slot::BlkShared, Slot::BlkShared) => true,
        (Slot::BlkMut, Slot::BlkMut) => true,
        (Slot::Captured, Slot::Captured) => true,
        _ => false,
    }
}

pub fn ex_eq(a: &Env, b: &Env) -> (r: bool)
    ensures
        r == (*a == *b),
    decreases a,
{
    match (a, b) {
        (Env::Nil, Env::Nil) => true,
        (Env::Cons(x, t), Env::Cons(y, u)) => ex_slot_eq(*x, *y) && ex_eq(t, u),
        _ => false,
    }
}

pub fn ex_get(e: &Env, x: usize) -> (r: Slot)
    ensures
        r == get(*e, x as nat),
    decreases e,
{
    match e {
        Env::Nil => Slot::Absent,
        Env::Cons(s, t) => if x == 0 {
            *s
        } else {
            ex_get(t, x - 1)
        },
    }
}

pub fn ex_set(e: &Env, x: usize, v: Slot) -> (r: Env)
    ensures
        r == set(*e, x as nat, v),
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => if x == 0 {
            Env::Cons(v, Box::new(ex_copy(t)))
        } else {
            Env::Cons(*s, Box::new(ex_set(t, x - 1, v)))
        },
    }
}

pub fn ex_clear(e: &Env) -> (r: Env)
    ensures
        r == clear(*e),
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(_, t) => Env::Cons(Slot::Absent, Box::new(ex_clear(t))),
    }
}

pub fn ex_release(e: &Env, m: &Env) -> (r: Env)
    ensures
        r == release(*e, *m),
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => {
            let (h, mt) = match m {
                Env::Nil => (Slot::Absent, Env::Nil),
                Env::Cons(x, u) => (*x, ex_copy(u)),
            };
            let v = match h {
                Slot::Absent => *s,
                _ => Slot::Absent,
            };
            Env::Cons(v, Box::new(ex_release(t, &mt)))
        },
    }
}

pub fn ex_capture(e: &Env) -> (r: Env)
    ensures
        r == capture(*e),
    decreases e,
{
    match e {
        Env::Nil => Env::Nil,
        Env::Cons(s, t) => {
            let h = match s {
                Slot::Absent => Slot::Absent,
                _ => Slot::Captured,
            };
            Env::Cons(h, Box::new(ex_capture(t)))
        },
    }
}

pub fn ex_no_owned(e: &Env) -> (r: bool)
    ensures
        r == no_owned(*e),
    decreases e,
{
    match e {
        Env::Nil => true,
        Env::Cons(s, t) => {
            let h = match s {
                Slot::Owned => false,
                _ => true,
            };
            h && ex_no_owned(t)
        },
    }
}

pub fn ex_exit_ok(e: &Env, gives: Option<usize>) -> (r: bool)
    ensures
        r == exit_ok(*e, gives),
{
    match gives {
        None => ex_no_owned(e),
        Some(g) => ex_no_owned(&ex_set(e, g, Slot::Absent)),
    }
}

} // verus!
