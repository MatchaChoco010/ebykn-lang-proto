//! 判定の表に載せる小さなプログラムの列挙（検証しない普通の Rust）。
//!
//! 列挙の上限は [`TIERS`] に置き、`formal/README.md` に同じものを書く。文の大きさは、原子文が 1、
//! 複合文が 1 ＋ 中のブロックの文の大きさの和。

use crate::syntax::{BranchKind, ConsumeKind, IntroKind, Slot};

/// 原子文。`x` は変数の添字。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum A {
    Intro(IntroKind, usize),
    Take(usize),
    Consume(ConsumeKind, usize),
    BorrowArg(usize, bool),
    Write(usize),
    Assign(usize),
    Let(usize),
    Return,
    Try,
    Diverge,
    Break,
    Continue,
}

/// 文。ブロックは `Vec<S>`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum S {
    Atom(A),
    Branch(BranchKind, Vec<S>, Vec<S>),
    While(Vec<S>),
    Loop(Vec<S>),
    Borrow(usize, bool, Vec<S>),
    Closure(Vec<S>),
}

/// 一つのブロックを中に持つ複合文の形。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum C1 {
    While,
    Loop,
    Borrow(usize, bool),
    Closure,
    LetElse,
}

/// 関数の頭：各変数の `uses`（`Absent` は `uses` なし）と `gives`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub params: Vec<Slot>,
    pub gives: Option<usize>,
    pub unchecked: bool,
}

#[derive(Clone, Debug)]
pub struct Case {
    pub tier: &'static str,
    pub header: Header,
    pub body: Vec<S>,
}

/// 列挙の段。`atoms` と複合文の形で、文の大きさの和が `max_size` 以下のブロックを全部作る。
pub struct Tier {
    pub name: &'static str,
    pub vars: usize,
    pub max_size: usize,
    pub atoms: fn() -> Vec<A>,
    pub c1: fn() -> Vec<C1>,
    pub c2: fn() -> Vec<BranchKind>,
    pub headers: fn() -> Vec<Header>,
    /// この段に載せるか（他の段と重ならないようにする）。
    pub keep: fn(&Header, &[S]) -> bool,
}

const P: usize = 0;
const Q: usize = 1;

fn h(params: &[Slot], gives: Option<usize>) -> Header {
    Header {
        params: params.to_vec(),
        gives,
        unchecked: false,
    }
}

fn core_atoms_for(x: usize) -> Vec<A> {
    vec![
        A::Intro(IntroKind::PtrNew, x),
        A::Take(x),
        A::Consume(ConsumeKind::Free, x),
        A::BorrowArg(x, false),
        A::BorrowArg(x, true),
        A::Write(x),
        A::Assign(x),
    ]
}

fn control() -> Vec<A> {
    vec![A::Return, A::Try, A::Diverge, A::Break, A::Continue]
}

fn core_atoms() -> Vec<A> {
    let mut v = core_atoms_for(P);
    v.extend(control());
    v
}

fn core_c1() -> Vec<C1> {
    vec![
        C1::While,
        C1::Loop,
        C1::Borrow(P, false),
        C1::Borrow(P, true),
        C1::Closure,
        C1::LetElse,
    ]
}

fn if_else() -> Vec<BranchKind> {
    vec![BranchKind::IfElse]
}

fn one_var_headers() -> Vec<Header> {
    vec![
        h(&[Slot::Absent], None),
        h(&[Slot::Owned], None),
        h(&[Slot::Shared], None),
        h(&[Slot::Mut], None),
        h(&[Slot::Absent], Some(P)),
    ]
}

/// 大きさ 3 の段の原子文：Γ を変えるものと、止まるもの。
fn deep_atoms() -> Vec<A> {
    vec![
        A::Intro(IntroKind::PtrNew, P),
        A::Consume(ConsumeKind::Free, P),
        A::BorrowArg(P, true),
        A::Return,
        A::Diverge,
        A::Break,
        A::Continue,
    ]
}

fn deep_c1() -> Vec<C1> {
    vec![
        C1::While,
        C1::Loop,
        C1::Borrow(P, true),
        C1::Closure,
        C1::LetElse,
    ]
}

fn deep_headers() -> Vec<Header> {
    vec![h(&[Slot::Absent], None), h(&[Slot::Owned], None)]
}

/// 大きさがちょうど 3 で、複合文を含む（原子文を三つ並べただけのものは除く）。
fn keep_deep(_: &Header, body: &[S]) -> bool {
    size(body) == 3 && body.iter().any(|s| !matches!(s, S::Atom(_)))
}

fn form_atoms() -> Vec<A> {
    let mut v = core_atoms();
    v.extend([
        A::Intro(IntroKind::PtrEmpty, P),
        A::Intro(IntroKind::CellNew, P),
        A::Intro(IntroKind::Gives, P),
        A::Consume(ConsumeKind::Put, P),
        A::Consume(ConsumeKind::Move, P),
        A::Consume(ConsumeKind::Leak, P),
        A::Let(P),
    ]);
    v
}

fn form_c2() -> Vec<BranchKind> {
    vec![BranchKind::IfElse, BranchKind::IfLet, BranchKind::Match]
}

fn two_atoms() -> Vec<A> {
    let mut v = Vec::new();
    for x in [P, Q] {
        v.extend([
            A::Intro(IntroKind::PtrNew, x),
            A::Take(x),
            A::Consume(ConsumeKind::Free, x),
            A::BorrowArg(x, true),
            A::Write(x),
            A::Assign(x),
        ]);
    }
    v.extend([A::Return, A::Break, A::Continue]);
    v
}

fn two_c1() -> Vec<C1> {
    vec![
        C1::While,
        C1::Loop,
        C1::Borrow(P, true),
        C1::Borrow(Q, false),
        C1::Closure,
    ]
}

fn two_headers() -> Vec<Header> {
    vec![
        h(&[Slot::Absent, Slot::Absent], None),
        h(&[Slot::Owned, Slot::Absent], None),
        h(&[Slot::Absent, Slot::Owned], None),
        h(&[Slot::Owned, Slot::Owned], None),
        h(&[Slot::Shared, Slot::Owned], None),
        h(&[Slot::Owned, Slot::Absent], Some(Q)),
    ]
}

fn keep_all(_: &Header, _: &[S]) -> bool {
    true
}

/// 本体に核の段にない形（導入・消費の別の書き方、`let`、`if let`、`match`）がある。
fn keep_forms(_: &Header, body: &[S]) -> bool {
    fn has(b: &[S]) -> bool {
        b.iter().any(|s| match s {
            S::Atom(a) => matches!(
                a,
                A::Intro(
                    IntroKind::PtrEmpty | IntroKind::CellNew | IntroKind::Gives,
                    _
                ) | A::Consume(ConsumeKind::Put | ConsumeKind::Move | ConsumeKind::Leak, _)
                    | A::Let(_)
            ),
            S::Branch(k, a, b) => {
                matches!(k, BranchKind::IfLet | BranchKind::Match) || has(a) || has(b)
            }
            S::While(b) | S::Loop(b) | S::Borrow(_, _, b) | S::Closure(b) => has(b),
        })
    }
    has(body)
}

/// 本体が二つ目の変数 `q` に触れる。
fn keep_two(_: &Header, body: &[S]) -> bool {
    fn has(b: &[S]) -> bool {
        b.iter().any(|s| match s {
            S::Atom(a) => matches!(
                a,
                A::Intro(_, Q)
                    | A::Take(Q)
                    | A::Consume(_, Q)
                    | A::BorrowArg(Q, _)
                    | A::Write(Q)
                    | A::Assign(Q)
                    | A::Let(Q)
            ),
            S::Branch(_, a, b) => has(a) || has(b),
            S::Borrow(x, _, b) => *x == Q || has(b),
            S::While(b) | S::Loop(b) | S::Closure(b) => has(b),
        })
    }
    has(body)
}

fn unchecked_headers() -> Vec<Header> {
    vec![Header {
        params: vec![Slot::Owned],
        gives: None,
        unchecked: true,
    }]
}

pub const TIERS: &[Tier] = &[
    Tier {
        name: "core",
        vars: 1,
        max_size: 2,
        atoms: core_atoms,
        c1: core_c1,
        c2: if_else,
        headers: one_var_headers,
        keep: keep_all,
    },
    Tier {
        name: "deep",
        vars: 1,
        max_size: 3,
        atoms: deep_atoms,
        c1: deep_c1,
        c2: if_else,
        headers: deep_headers,
        keep: keep_deep,
    },
    Tier {
        name: "forms",
        vars: 1,
        max_size: 2,
        atoms: form_atoms,
        c1: core_c1,
        c2: form_c2,
        headers: deep_headers,
        keep: keep_forms,
    },
    Tier {
        name: "two",
        vars: 2,
        max_size: 2,
        atoms: two_atoms,
        c1: two_c1,
        c2: if_else,
        headers: two_headers,
        keep: keep_two,
    },
    Tier {
        name: "unchecked",
        vars: 1,
        max_size: 1,
        atoms: core_atoms,
        c1: core_c1,
        c2: if_else,
        headers: unchecked_headers,
        keep: keep_all,
    },
];

pub fn size(b: &[S]) -> usize {
    b.iter()
        .map(|s| match s {
            S::Atom(_) => 1,
            S::Branch(_, a, b) => 1 + size(a) + size(b),
            S::While(b) | S::Loop(b) | S::Borrow(_, _, b) | S::Closure(b) => 1 + size(b),
        })
        .sum()
}

/// 大きさがちょうど `n` のブロックを全部作る。
fn blocks(t: &Tier, memo: &mut Vec<Option<Vec<Vec<S>>>>, n: usize) -> Vec<Vec<S>> {
    if let Some(Some(v)) = memo.get(n) {
        return v.clone();
    }
    let mut out = Vec::new();
    if n == 0 {
        out.push(Vec::new());
    } else {
        for k in 1..=n {
            let heads = stmts(t, memo, k);
            let tails = blocks(t, memo, n - k);
            for s in &heads {
                for r in &tails {
                    let mut b = vec![s.clone()];
                    b.extend(r.iter().cloned());
                    out.push(b);
                }
            }
        }
    }
    if memo.len() <= n {
        memo.resize(n + 1, None);
    }
    memo[n] = Some(out.clone());
    out
}

fn stmts(t: &Tier, memo: &mut Vec<Option<Vec<Vec<S>>>>, n: usize) -> Vec<S> {
    let mut out = Vec::new();
    if n == 1 {
        out.extend((t.atoms)().into_iter().map(S::Atom));
    }
    if n >= 1 {
        for b in blocks(t, memo, n - 1) {
            for c in (t.c1)() {
                out.push(match c {
                    C1::While => S::While(b.clone()),
                    C1::Loop => S::Loop(b.clone()),
                    C1::Borrow(x, m) => S::Borrow(x, m, b.clone()),
                    C1::Closure => S::Closure(b.clone()),
                    C1::LetElse => S::Branch(BranchKind::LetElse, Vec::new(), b.clone()),
                });
            }
        }
        for i in 0..n {
            for a in blocks(t, memo, i) {
                for b in blocks(t, memo, n - 1 - i) {
                    for k in (t.c2)() {
                        out.push(S::Branch(k, a.clone(), b.clone()));
                    }
                }
            }
        }
    }
    out
}

/// 普通に終わりうるか（構文だけで決める）。
pub fn completes(b: &[S]) -> bool {
    b.iter().all(stmt_completes)
}

fn stmt_completes(s: &S) -> bool {
    match s {
        S::Atom(A::Return | A::Diverge | A::Break | A::Continue) => false,
        S::Atom(_) => true,
        S::Branch(_, a, b) => completes(a) || completes(b),
        S::While(_) | S::Closure(_) => true,
        S::Loop(b) => breaks_here(b),
        S::Borrow(_, _, b) => completes(b),
    }
}

/// このループを抜ける `break` がある（内側のループとクロージャの中は数えない）。
fn breaks_here(b: &[S]) -> bool {
    b.iter().any(|s| match s {
        S::Atom(A::Break) => true,
        S::Branch(_, a, b) => breaks_here(a) || breaks_here(b),
        S::Borrow(_, _, b) => breaks_here(b),
        _ => false,
    })
}

/// Rust として書ける形か：止まる文の後ろに文がない、`break` / `continue` はループの中
/// （クロージャの境界を越えない）、`let-else` の `else` は止まる、`gives` と `?` は同時に使わない
/// （戻り値の型が合わない）。
fn plausible(h: &Header, b: &[S]) -> bool {
    fn block(b: &[S], in_loop: bool) -> bool {
        for (i, s) in b.iter().enumerate() {
            if i + 1 < b.len() && !stmt_completes(s) {
                return false;
            }
            let ok = match s {
                S::Atom(A::Break | A::Continue) => in_loop,
                S::Atom(_) => true,
                S::Branch(BranchKind::LetElse, a, e) => {
                    a.is_empty() && !completes(e) && block(e, in_loop)
                }
                S::Branch(_, a, e) => block(a, in_loop) && block(e, in_loop),
                S::While(x) | S::Loop(x) => block(x, true),
                S::Borrow(_, _, x) => block(x, in_loop),
                S::Closure(x) => block(x, false),
            };
            if !ok {
                return false;
            }
        }
        true
    }
    fn has_try(b: &[S]) -> bool {
        b.iter().any(|s| match s {
            S::Atom(A::Try) => true,
            S::Atom(_) | S::Closure(_) => false,
            S::Branch(_, a, b) => has_try(a) || has_try(b),
            S::While(x) | S::Loop(x) | S::Borrow(_, _, x) => has_try(x),
        })
    }
    block(b, false) && !(h.gives.is_some() && has_try(b))
}

/// 全段の全ケース。順序は段、頭、本体の大きさ、列挙の順。
pub fn all_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for t in TIERS {
        let mut memo = Vec::new();
        let mut bodies = Vec::new();
        for n in 0..=t.max_size {
            bodies.extend(blocks(t, &mut memo, n));
        }
        for header in (t.headers)() {
            assert_eq!(header.params.len(), t.vars);
            for b in &bodies {
                if plausible(&header, b) && (t.keep)(&header, b) {
                    out.push(Case {
                        tier: t.name,
                        header: header.clone(),
                        body: b.clone(),
                    });
                }
            }
        }
    }
    out
}
