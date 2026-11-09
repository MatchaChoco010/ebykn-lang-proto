//! 判定の表（オラクル）を作る（検証しない普通の Rust）。
//!
//! 列挙したケース（[`crate::enumerate`]）をモデルの検査（[`check`]）にかけ、次の 3 つを作る。
//!
//! - `linearity.eyk`：各ケースを `.eyk` の関数 `c<番号>` に書き出したもの。先頭に補助の関数を置く。
//! - `linearity.tsv`：各ケースの判定（`ok` か診断コード）と、診断の位置（`linearity.eyk` の行と列）。
//! - `linearity-mutants.tsv`：わざと壊した検査（規則を一つ外したもの）が、表のどのケースで正しい検査と
//!   食い違うか。食い違うケースがない壊れた検査があれば失敗する。

use std::collections::HashMap;
use std::fmt::Write as _;

use crate::check::{Rules, check, check_with};
use crate::enumerate::{A, Case, Header, S, all_cases};
use crate::syntax::{
    Block, BranchKind, Code, ConsumeKind, Env, IntroKind, Op, Pos, Program, Slot, Stmt, Verdict,
};

pub const TSV: &str = "linearity.tsv";
pub const EYK: &str = "linearity.eyk";
pub const MUTANTS: &str = "linearity-mutants.tsv";

const NAMES: &[&str] = &["p", "q"];

fn name(x: usize) -> &'static str {
    NAMES[x]
}

// ---- モデルの項へ ----

fn to_block(b: &[S], next: &mut u32) -> Block {
    let stmts: Vec<Stmt> = b.iter().map(|s| to_stmt(s, next)).collect();
    stmts
        .into_iter()
        .rev()
        .fold(Block::Nil, |acc, s| Block::Cons(Box::new(s), Box::new(acc)))
}

fn to_stmt(s: &S, next: &mut u32) -> Stmt {
    let id = *next;
    *next += 1;
    let mut sub = |b: &[S]| Box::new(to_block(b, next));
    let op = match s {
        S::Atom(a) => match *a {
            A::Intro(kind, x) => Op::Intro { kind, x },
            A::Take(x) => Op::Take { x },
            A::Consume(kind, x) => Op::Consume { kind, x },
            A::BorrowArg(x, mutable) => Op::BorrowArg { x, mutable },
            A::Write(x) => Op::Write { x },
            A::Assign(x) => Op::Assign { x },
            A::Let(x) => Op::Let { x },
            A::Return => Op::Return,
            A::Try => Op::Try,
            A::Diverge => Op::Diverge,
            A::Break => Op::Break,
            A::Continue => Op::Continue,
        },
        S::Branch(kind, a, b) => {
            let a = sub(a);
            let b = sub(b);
            Op::Branch { kind: *kind, a, b }
        }
        S::While(b) => Op::While { body: sub(b) },
        S::Loop(b) => Op::Loop { body: sub(b) },
        S::Borrow(x, mutable, b) => Op::Borrow {
            x: *x,
            mutable: *mutable,
            body: sub(b),
        },
        S::Closure(b) => Op::Closure { body: sub(b) },
    };
    Stmt { id, op }
}

pub fn to_program(c: &Case) -> Program {
    let params = c
        .header
        .params
        .iter()
        .rev()
        .fold(Env::Nil, |acc, s| Env::Cons(*s, Box::new(acc)));
    let mut next = 1;
    Program {
        params,
        gives: c.header.gives,
        unchecked: c.header.unchecked,
        try_shape: c.header.try_shape,
        body: to_block(&c.body, &mut next),
    }
}

// ---- 短い表記（TSV の列） ----

fn uses_name(s: Slot) -> &'static str {
    match s {
        Slot::Owned => "own",
        Slot::Shared => "&",
        Slot::Mut => "&mut",
        _ => "-",
    }
}

pub fn header_text(h: &Header) -> String {
    let mut parts: Vec<String> = h
        .params
        .iter()
        .enumerate()
        .map(|(i, s)| format!("{}={}", name(i), uses_name(*s)))
        .collect();
    if let Some(g) = h.gives {
        parts.push(format!("gives={}", name(g)));
    }
    if h.unchecked {
        parts.push("unchecked".to_string());
    }
    if !h.try_shape {
        parts.push("ret=alias".to_string());
    }
    parts.join(" ")
}

pub fn body_text(b: &[S]) -> String {
    if b.is_empty() {
        return "{}".to_string();
    }
    let inner: Vec<String> = b.iter().map(stmt_text).collect();
    format!("{{ {} }}", inner.join("; "))
}

fn stmt_text(s: &S) -> String {
    match s {
        S::Atom(a) => match *a {
            A::Intro(k, x) => {
                let w = match k {
                    IntroKind::PtrNew => "new",
                    IntroKind::PtrEmpty => "empty",
                    IntroKind::CellNew => "cell",
                    IntroKind::Gives => "make",
                };
                format!("{w} {}", name(x))
            }
            A::Take(x) => format!("take {}", name(x)),
            A::Consume(k, x) => {
                let w = match k {
                    ConsumeKind::Free => "free",
                    ConsumeKind::Put => "put",
                    ConsumeKind::Move => "move",
                    ConsumeKind::Leak => "leak",
                };
                format!("{w} {}", name(x))
            }
            A::BorrowArg(x, false) => format!("inspect {}", name(x)),
            A::BorrowArg(x, true) => format!("update {}", name(x)),
            A::Write(x) => format!("write {}", name(x)),
            A::Assign(x) => format!("assign {}", name(x)),
            A::Let(x) => format!("let {}", name(x)),
            A::Return => "return".to_string(),
            A::Try => "?".to_string(),
            A::Diverge => "panic".to_string(),
            A::Break => "break".to_string(),
            A::Continue => "continue".to_string(),
        },
        S::Branch(BranchKind::IfElse, a, b) => format!("if {} else {}", body_text(a), body_text(b)),
        S::Branch(BranchKind::IfLet, a, b) => {
            format!("iflet {} else {}", body_text(a), body_text(b))
        }
        S::Branch(BranchKind::Match, a, b) => format!("match {} {}", body_text(a), body_text(b)),
        S::Branch(BranchKind::LetElse, _, b) => format!("letelse {}", body_text(b)),
        S::While(b) => format!("while {}", body_text(b)),
        S::Loop(b) => format!("loop {}", body_text(b)),
        S::Borrow(x, false, b) => format!("borrow {} {}", name(*x), body_text(b)),
        S::Borrow(x, true, b) => format!("borrow mut {} {}", name(*x), body_text(b)),
        S::Closure(b) => format!("closure {}", body_text(b)),
    }
}

// ---- `.eyk` への書き出し ----

/// 補助の関数。どれも §6.2 の検査を通る。
const PRELUDE: &str = "\
// formal/linearity が生成した §6.2 の判定の表の入力（formal/README.md）。手で編集しない。
// 各 `fn c<番号>` が formal/oracle/linearity.tsv の 1 行に対応する。

fn cond() -> bool {
    true
}

fn pick() -> u32 {
    0
}

fn maybe() -> Option<u32> {
    None
}

fn fallible() -> Option<u32> {
    None
}

type Alias = Option<()>;

fn other() -> Ptr<u32> {
    let p = Ptr::new(0);
    leak p@;
    p
}

fn make() -> (q: Ptr<u32>) gives q@ {
    let q = Ptr::new(0);
    q
}

fn consume(p: Ptr<u32>) uses p@ {
    p.free();
}

fn inspect(p: Ptr<u32>) uses &p@ {
}

fn update(p: Ptr<u32>) uses &mut p@ {
    *p = 1;
}
";

/// 本体の戻り方（`return` と末尾の式の書き方）。
#[derive(Clone, Copy)]
enum Ret {
    Unit,
    Gives(usize),
    Opt,
}

struct Out {
    text: String,
    line: usize,
    /// 文 `id` の先頭（`At`）と、複合文 `id` の閉じ括弧（`End`）の位置。
    at: HashMap<u32, (usize, usize)>,
    end: HashMap<u32, (usize, usize)>,
}

impl Out {
    fn new(text: &str) -> Self {
        Out {
            text: text.to_string(),
            line: text.lines().count() + 1,
            at: HashMap::new(),
            end: HashMap::new(),
        }
    }

    /// 1 行を書き、その行番号と、字下げの後の列（1 始まり）を返す。
    fn put(&mut self, indent: usize, s: &str) -> (usize, usize) {
        let pos = (self.line, indent * 4 + 1);
        let _ = writeln!(self.text, "{}{s}", "    ".repeat(indent));
        self.line += 1;
        pos
    }
}

fn has_try(b: &[S]) -> bool {
    b.iter().any(|s| match s {
        S::Atom(A::Try) => true,
        S::Atom(_) | S::Closure(_) => false,
        S::Branch(_, a, b) => has_try(a) || has_try(b),
        S::While(x) | S::Loop(x) | S::Borrow(_, _, x) => has_try(x),
    })
}

fn write_block(o: &mut Out, b: &[S], indent: usize, next: &mut u32, ret: Ret) {
    for s in b {
        write_stmt(o, s, indent, next, ret);
    }
}

/// 本体（関数かクロージャ）を書き、普通に終わりうるなら末尾の式を足す。
fn write_body(o: &mut Out, b: &[S], indent: usize, next: &mut u32, ret: Ret) {
    write_block(o, b, indent, next, ret);
    if crate::enumerate::completes(b) {
        match ret {
            Ret::Unit => {}
            Ret::Gives(g) => {
                o.put(indent, name(g));
            }
            Ret::Opt => {
                o.put(indent, "Some(())");
            }
        }
    }
}

fn write_stmt(o: &mut Out, s: &S, indent: usize, next: &mut u32, ret: Ret) {
    let id = *next;
    *next += 1;
    let line = match s {
        S::Atom(a) => match *a {
            A::Intro(IntroKind::PtrNew, x) => format!("let {} = Ptr::new(0);", name(x)),
            A::Intro(IntroKind::PtrEmpty, x) => format!("let {} = Ptr::empty();", name(x)),
            A::Intro(IntroKind::CellNew, x) => format!("let {} = Cell::new(0);", name(x)),
            A::Intro(IntroKind::Gives, x) => format!("let {} = make();", name(x)),
            A::Take(x) => format!("take r[{}];", name(x)),
            A::Consume(ConsumeKind::Free, x) => format!("{}.free();", name(x)),
            A::Consume(ConsumeKind::Put, x) => format!("put r[{}];", name(x)),
            A::Consume(ConsumeKind::Move, x) => format!("consume({});", name(x)),
            A::Consume(ConsumeKind::Leak, x) => format!("leak {}@;", name(x)),
            A::BorrowArg(x, false) => format!("inspect({});", name(x)),
            A::BorrowArg(x, true) => format!("update({});", name(x)),
            A::Write(x) => format!("*{} = 1;", name(x)),
            A::Assign(x) => format!("{} = other();", name(x)),
            A::Let(x) => format!("let {} = other();", name(x)),
            A::Return => match ret {
                Ret::Unit => "return;".to_string(),
                Ret::Gives(g) => format!("return {};", name(g)),
                Ret::Opt => "return None;".to_string(),
            },
            A::Try => "fallible()?;".to_string(),
            A::Diverge => "panic!();".to_string(),
            A::Break => "break;".to_string(),
            A::Continue => "continue;".to_string(),
        },
        S::Branch(BranchKind::IfElse, ..) => "if cond() {".to_string(),
        S::Branch(BranchKind::IfLet, ..) => "if let Some(_) = maybe() {".to_string(),
        S::Branch(BranchKind::Match, ..) => "match pick() {".to_string(),
        S::Branch(BranchKind::LetElse, ..) => "let Some(_) = maybe() else {".to_string(),
        S::While(_) => "while cond() {".to_string(),
        S::Loop(_) => "loop {".to_string(),
        S::Borrow(x, false, _) => format!("borrow r[{}] {{", name(*x)),
        S::Borrow(x, true, _) => format!("borrow mut r[{}] {{", name(*x)),
        S::Closure(_) => "let f = || {".to_string(),
    };
    let pos = o.put(indent, &line);
    o.at.insert(id, pos);
    let close = match s {
        S::Atom(_) => return,
        S::Branch(BranchKind::IfElse | BranchKind::IfLet, a, b) => {
            write_block(o, a, indent + 1, next, ret);
            if !b.is_empty() || matches!(s, S::Branch(BranchKind::IfLet, ..)) {
                o.put(indent, "} else {");
                write_block(o, b, indent + 1, next, ret);
            }
            o.put(indent, "}")
        }
        S::Branch(BranchKind::Match, a, b) => {
            o.put(indent + 1, "0 => {");
            write_block(o, a, indent + 2, next, ret);
            o.put(indent + 1, "}");
            o.put(indent + 1, "_ => {");
            write_block(o, b, indent + 2, next, ret);
            o.put(indent + 1, "}");
            o.put(indent, "}")
        }
        S::Branch(BranchKind::LetElse, _, b) => {
            write_block(o, b, indent + 1, next, ret);
            o.put(indent, "};")
        }
        S::While(b) | S::Loop(b) | S::Borrow(_, _, b) => {
            write_block(o, b, indent + 1, next, ret);
            o.put(indent, "}")
        }
        S::Closure(b) => {
            let r = if has_try(b) { Ret::Opt } else { Ret::Unit };
            write_body(o, b, indent + 1, next, r);
            o.put(indent, "};")
        }
    };
    o.end.insert(id, close);
}

/// ケースを関数 `fname` として書き、関数本体の閉じ括弧の位置（`End(0)`）を記録する。
fn write_case(o: &mut Out, fname: &str, c: &Case) {
    let h = &c.header;
    let params: Vec<String> = (0..h.params.len())
        .filter(|&i| h.gives != Some(i))
        .map(|i| format!("{}: Ptr<u32>", name(i)))
        .collect();
    let mut uses: Vec<String> = h
        .params
        .iter()
        .enumerate()
        .filter_map(|(i, s)| match s {
            Slot::Owned => Some(format!("{}@", name(i))),
            Slot::Shared => Some(format!("&{}@", name(i))),
            Slot::Mut => Some(format!("&mut {}@", name(i))),
            _ => None,
        })
        .collect();
    uses.push("r: &mut Region<Ptr<u32>>".to_string());
    let ret = match h.gives {
        Some(g) => Ret::Gives(g),
        None if has_try(&c.body) => Ret::Opt,
        None => Ret::Unit,
    };
    // 文法（仕様 §5.1）：戻り値の型、契約（`uses`、`gives`）の順。
    let sig_ret = match ret {
        Ret::Unit => String::new(),
        Ret::Gives(g) => format!(" -> ({}: Ptr<u32>)", name(g)),
        Ret::Opt if !h.try_shape => " -> Alias".to_string(),
        Ret::Opt => " -> Option<()>".to_string(),
    };
    if let Ret::Gives(g) = ret {
        uses.push(format!("gives {}@", name(g)));
    }
    let kw = if h.unchecked { "unchecked fn" } else { "fn" };
    o.put(0, "");
    o.put(0, &format!("// {} {}", header_text(h), body_text(&c.body)));
    let contract = uses.join(", ").replace(", gives ", " gives ");
    o.put(
        0,
        &format!(
            "{kw} {fname}({}){sig_ret} uses {contract} {{",
            params.join(", ")
        ),
    );
    let mut next = 1;
    write_body(o, &c.body, 1, &mut next, ret);
    let close = o.put(0, "}");
    o.end.insert(0, close);
}

fn code_text(c: Code) -> &'static str {
    match c {
        Code::E0003 => "E0003",
        Code::E0503 => "E0503",
        Code::E0504 => "E0504",
        Code::E0505 => "E0505",
        Code::E0506 => "E0506",
        Code::E0507 => "E0507",
        Code::E0508 => "E0508",
        Code::E0509 => "E0509",
        Code::E0510 => "E0510",
        Code::E0511 => "E0511",
        Code::E0512 => "E0512",
    }
}

pub fn verdict_text(v: &Verdict) -> String {
    match v {
        Verdict::Accept => "ok".to_string(),
        Verdict::Reject { code, pos } => {
            let p = match pos {
                Pos::At(i) => format!("at {i}"),
                Pos::End(i) => format!("end {i}"),
            };
            format!("{} {p}", code_text(*code))
        }
    }
}

/// わざと壊した検査。`rules` は正しい検査から規則を一つ外したもの。
pub fn mutants() -> Vec<(&'static str, &'static str, Rules)> {
    let f = Rules::full();
    vec![
        (
            "no-join",
            "分岐の終端の Γ を比べない（E0510）",
            Rules { join: false, ..f },
        ),
        (
            "no-borrow-consume",
            "borrow ブロックの p を Owned と同じに消費できる（E0509）",
            Rules {
                borrow_consume: false,
                ..f
            },
        ),
        (
            "no-continue",
            "continue の点の Γ を本体の入口と比べない",
            Rules { cont: false, ..f },
        ),
        (
            "no-break",
            "break の点の Γ をループの直後の Γ と比べない",
            Rules { brk: false, ..f },
        ),
        (
            "no-loop-end",
            "本体の終端の Γ を本体の入口と比べない",
            Rules {
                loop_end: false,
                ..f
            },
        ),
        (
            "no-closure-reset",
            "クロージャの本体で Γ を空にしない（E0512）",
            Rules {
                closure_reset: false,
                ..f
            },
        ),
        (
            "no-try-return",
            "? を return として扱わない（E0505）",
            Rules {
                try_return: false,
                ..f
            },
        ),
        (
            "no-release",
            "break / continue で抜ける点で borrow ブロックの p を Γ から消さない（#21）",
            Rules {
                release: false,
                ..f
            },
        ),
        (
            "no-loop-clear",
            "ループの中のループに入るときに、外のループに入ってから開いた borrow の印を引き継ぐ（#21）",
            Rules {
                loop_clear: false,
                ..f
            },
        ),
        (
            "no-try-shape",
            "戻り値の型を Option / Result と書いていない関数の borrow mut の中の ? を通す（E0003。#21）",
            Rules {
                try_shape: false,
                ..f
            },
        ),
    ]
}

pub struct Tables {
    pub files: Vec<(&'static str, String)>,
    /// 食い違うケースのない壊れた検査。
    pub undetected: Vec<&'static str>,
}

pub fn build() -> Tables {
    let cases = all_cases();
    let mut eyk = Out::new(PRELUDE);
    let mut tsv = String::from("case\ttier\theader\tbody\tverdict\tpos\tline\tcol\n");
    let muts = mutants();
    let mut first: Vec<Option<(String, String, String)>> = vec![None; muts.len()];
    let mut count = vec![0usize; muts.len()];
    for (i, c) in cases.iter().enumerate() {
        let fname = format!("c{:05}", i + 1);
        eyk.at.clear();
        eyk.end.clear();
        write_case(&mut eyk, &fname, c);
        let p = to_program(c);
        let v = check(&p);
        let (line, col) = match v {
            Verdict::Accept => ("-".to_string(), "-".to_string()),
            Verdict::Reject { pos, .. } => {
                let (l, c) = match pos {
                    Pos::At(i) => eyk.at[&i],
                    Pos::End(i) => eyk.end[&i],
                };
                (l.to_string(), c.to_string())
            }
        };
        let (code, mpos) = match v {
            Verdict::Accept => ("ok", "-".to_string()),
            Verdict::Reject { code, pos } => (
                code_text(code),
                match pos {
                    Pos::At(i) => format!("at {i}"),
                    Pos::End(i) => format!("end {i}"),
                },
            ),
        };
        let _ = writeln!(
            tsv,
            "{fname}\t{}\t{}\t{}\t{code}\t{mpos}\t{line}\t{col}",
            c.tier,
            header_text(&c.header),
            body_text(&c.body)
        );
        for (k, (_, _, rules)) in muts.iter().enumerate() {
            let m = check_with(&p, rules);
            if m != v {
                count[k] += 1;
                if first[k].is_none() {
                    first[k] = Some((fname.clone(), verdict_text(&v), verdict_text(&m)));
                }
            }
        }
    }
    let mut mt = String::from("mutant\trule\tdiffering\tfirst_case\texpected\tgot\n");
    let mut undetected = Vec::new();
    for (k, (name, rule, _)) in muts.iter().enumerate() {
        match &first[k] {
            Some((case, want, got)) => {
                let _ = writeln!(mt, "{name}\t{rule}\t{}\t{case}\t{want}\t{got}", count[k]);
            }
            None => {
                undetected.push(*name);
                let _ = writeln!(mt, "{name}\t{rule}\t0\t-\t-\t-");
            }
        }
    }
    Tables {
        files: vec![(TSV, tsv), (EYK, eyk.text), (MUTANTS, mt)],
        undetected,
    }
}
