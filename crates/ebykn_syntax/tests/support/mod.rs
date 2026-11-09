//! テストの道具：入力を読み、AST を S 式で短く書き出す。
#![allow(dead_code)]

use ebykn_syntax::ast::*;
use ebykn_syntax::diag::Lang;
use ebykn_syntax::span::{FileId, SourceFile};
use ebykn_syntax::{Parsed, parse_eyk};

pub fn file(src: &str) -> SourceFile {
    SourceFile::new(FileId(0), "src/lib.eyk", src)
}

pub fn parse(src: &str) -> (SourceFile, Parsed) {
    let f = file(src);
    let p = parse_eyk(&f);
    (f, p)
}

/// 診断なしで読めることを確かめて、AST を返す。
#[track_caller]
pub fn ok(src: &str) -> File {
    let (f, p) = parse(src);
    let d: Vec<String> = p.diags.iter().map(|d| d.render(&f, Lang::En)).collect();
    assert!(d.is_empty(), "診断が出た:\n{}\n---\n{src}", d.join("\n"));
    p.file
}

/// 診断を英語で書き出す。
pub fn diags(src: &str) -> Vec<String> {
    diags_in(src, Lang::En)
}

pub fn diags_in(src: &str, lang: Lang) -> Vec<String> {
    let (f, p) = parse(src);
    p.diags.iter().map(|d| d.render(&f, lang)).collect()
}

/// 少なくとも一つ診断が出て、そのどれかが `needle` を含むことを確かめる。
#[track_caller]
pub fn rejects(src: &str, needle: &str) {
    let d = diags(src);
    assert!(
        d.iter().any(|d| d.contains(needle)),
        "{needle:?} を含む診断がない: {d:?}\n---\n{src}"
    );
}

/// 関数 `f` の本体の最後の式（または文）を S 式で。
#[track_caller]
pub fn body_sexp(src: &str) -> String {
    let file = ok(src);
    for item in &file.items {
        if let ItemKind::Fn(f) = &item.kind {
            return match &f.body {
                Some(FnBody::Block(b)) => b.stmts.iter().map(stmt).collect::<Vec<_>>().join(" "),
                Some(FnBody::Pure(e)) => expr(e),
                None => "none".into(),
            };
        }
    }
    panic!("fn がない");
}

/// 式一つを `fn f() { <式> }` に入れて読み、S 式で返す。
#[track_caller]
pub fn e(src: &str) -> String {
    body_sexp(&format!("fn f() {{ {src} }}"))
}

pub fn ident(i: &Ident) -> String {
    if i.raw {
        format!("r#{}", i.name)
    } else {
        i.name.clone()
    }
}

pub fn path(p: &Path) -> String {
    let segs: Vec<String> = p
        .segments
        .iter()
        .map(|s| {
            let a = match &s.args {
                GenericArgs::None => String::new(),
                GenericArgs::Angle { args, .. } => {
                    format!("<{}>", args.iter().map(garg).collect::<Vec<_>>().join(","))
                }
                GenericArgs::Paren { inputs, output, .. } => format!(
                    "({}){}",
                    inputs.iter().map(ty).collect::<Vec<_>>().join(","),
                    output
                        .as_ref()
                        .map_or(String::new(), |o| format!("->{}", ty(o)))
                ),
            };
            format!("{}{a}", ident(&s.ident))
        })
        .collect();
    format!(
        "{}{}",
        if p.leading_colon { "::" } else { "" },
        segs.join("::")
    )
}

fn garg(a: &GenericArg) -> String {
    match a {
        GenericArg::Lifetime(l) => format!("'{}", l.name),
        GenericArg::Type(t) => ty(t),
        GenericArg::Const(e) => expr(e),
        GenericArg::AssocType { ident: i, ty: t } => format!("{}={}", ident(i), ty(t)),
        GenericArg::Rust(t) => t.tokens.to_string(),
    }
}

pub fn ty(t: &Type) -> String {
    match &t.kind {
        TypeKind::Path { path: p, .. } => path(p),
        TypeKind::Ref { mutable, elem, .. } => {
            format!("&{}{}", if *mutable { "mut " } else { "" }, ty(elem))
        }
        TypeKind::Tuple(v) => format!("({})", v.iter().map(ty).collect::<Vec<_>>().join(",")),
        TypeKind::Array { elem, len } => format!("[{};{}]", ty(elem), expr(len)),
        TypeKind::Slice(e) => format!("[{}]", ty(e)),
        TypeKind::Paren(e) => format!("({})", ty(e)),
        TypeKind::Never => "!".into(),
        TypeKind::Infer => "_".into(),
        TypeKind::Rust(t) => format!("rust`{}`", t.tokens),
    }
}

fn list(v: &[Expr]) -> String {
    v.iter().map(expr).collect::<Vec<_>>().join(" ")
}

pub fn binop(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Rem => "%",
        And => "&&",
        Or => "||",
        BitXor => "^",
        BitAnd => "&",
        BitOr => "|",
        Shl => "<<",
        Shr => ">>",
        Eq => "==",
        Lt => "<",
        Le => "<=",
        Ne => "!=",
        Ge => ">=",
        Gt => ">",
        AddAssign => "+=",
        SubAssign => "-=",
        MulAssign => "*=",
        DivAssign => "/=",
        RemAssign => "%=",
        BitXorAssign => "^=",
        BitAndAssign => "&=",
        BitOrAssign => "|=",
        ShlAssign => "<<=",
        ShrAssign => ">>=",
        Implies => "==>",
        Iff => "<==>",
    }
}

fn member(m: &Member) -> String {
    match m {
        Member::Named(i) => ident(i),
        Member::Unnamed { index, .. } => index.to_string(),
    }
}

pub fn region(r: &RegionIndex) -> String {
    let base = match &r.region {
        Region::SelfField { field, .. } => format!("self@{}", ident(field)),
        Region::Var(i) => ident(i),
    };
    format!("{base}[{}]", expr(&r.index))
}

/// 呼び出しの `uses` の領域（#19 の既定 P1）。なければ空。
fn region_args(uses: &[ebykn_syntax::ast::RegionArg]) -> String {
    if uses.is_empty() {
        return String::new();
    }
    let rs: Vec<String> = uses
        .iter()
        .map(|u| {
            let m = match u.mode {
                ebykn_syntax::ast::PermMode::Move => "",
                ebykn_syntax::ast::PermMode::Shared => "&",
                ebykn_syntax::ast::PermMode::Mut => "&mut ",
            };
            let r = match &u.region {
                Region::SelfField { field, .. } => format!("self@{}", ident(field)),
                Region::Var(i) => ident(i),
            };
            format!("{m}{r}")
        })
        .collect();
    format!(" (uses {})", rs.join(", "))
}

pub fn expr(e: &Expr) -> String {
    use ExprKind as K;
    let s = match &e.kind {
        K::Array(v) => format!("(array {})", list(v)),
        K::Repeat { expr: x, len } => format!("(repeat {} {})", expr(x), expr(len)),
        K::Assign { lhs, rhs } => format!("(= {} {})", expr(lhs), expr(rhs)),
        K::Binary { op, lhs, rhs, .. } => format!("({} {} {})", binop(*op), expr(lhs), expr(rhs)),
        K::Unary { op, expr: x } => {
            let o = match op {
                UnOp::Deref => "*",
                UnOp::Not => "!",
                UnOp::Neg => "-",
            };
            format!("({o} {})", expr(x))
        }
        K::Ref { mutable, expr: x } => {
            format!("({} {})", if *mutable { "&mut" } else { "&" }, expr(x))
        }
        K::Block { block: b, .. } => block(b),
        K::Unsafe(b) => format!("(unsafe {})", block(b)),
        K::Const(b) => format!("(const {})", block(b)),
        K::Break { expr: x, .. } => match x {
            Some(x) => format!("(break {})", expr(x)),
            None => "(break)".into(),
        },
        K::Continue { .. } => "(continue)".into(),
        K::Return(x) => match x {
            Some(x) => format!("(return {})", expr(x)),
            None => "(return)".into(),
        },
        K::Call { func, args, uses } => {
            format!("(call {} {}{})", expr(func), list(args), region_args(uses))
                .replace("  ", " ")
                .replace(" )", ")")
        }
        K::MethodCall {
            receiver,
            method,
            args,
            uses,
            ..
        } => format!(
            "(.{} {} {}{})",
            ident(method),
            expr(receiver),
            list(args),
            region_args(uses)
        )
        .replace("  ", " ")
        .replace(" )", ")"),
        K::Cast { expr: x, ty: t } => format!("(as {} {})", expr(x), ty(t)),
        K::Closure(c) => {
            let ps: Vec<String> = c.params.iter().map(pat).collect();
            let mut s = format!("(closure [{}]", ps.join(" "));
            for cl in &c.contract.clauses {
                s.push(' ');
                s.push_str(&clause(cl));
            }
            s.push(' ');
            s.push_str(&expr(&c.body));
            s.push(')');
            s
        }
        K::Field { base, member: m } => format!("(. {} {})", expr(base), member(m)),
        K::Index { base, index } => format!("(index {} {})", expr(base), expr(index)),
        K::If { cond, then, else_ } => match else_ {
            Some(x) => format!("(if {} {} {})", expr(cond), block(then), expr(x)),
            None => format!("(if {} {})", expr(cond), block(then)),
        },
        K::Let { pat: p, expr: x } => format!("(let {} {})", pat(p), expr(x)),
        K::Loop(l) => {
            let head = match &l.kind {
                LoopKind::While(c) => format!("while {}", expr(c)),
                LoopKind::For { pat: p, iter } => format!("for {} {}", pat(p), expr(iter)),
                LoopKind::Loop => "loop".into(),
            };
            let cl: Vec<String> = l
                .clauses
                .iter()
                .map(|c| {
                    let k = match c.kind {
                        LoopClauseKind::Invariant => "invariant",
                        LoopClauseKind::Decreases => "decreases",
                    };
                    format!("({k} {})", list(&c.exprs))
                })
                .collect();
            let mid = if cl.is_empty() {
                String::new()
            } else {
                format!(" {}", cl.join(" "))
            };
            format!("({head}{mid} {})", block(&l.body))
        }
        K::Match { expr: x, arms } => {
            let a: Vec<String> = arms
                .iter()
                .map(|a| format!("({} => {})", pat(&a.pat), expr(&a.body)))
                .collect();
            format!("(match {} {})", expr(x), a.join(" "))
        }
        K::Lit(l) => l.text.clone(),
        K::Macro(m) => match &m.args {
            MacroArgs::Tokens => format!("{}!({})", path(&m.path), m.tokens),
            MacroArgs::Exprs(v) => format!(
                "{}![{}]",
                path(&m.path),
                v.iter().map(expr).collect::<Vec<_>>().join(", ")
            ),
            MacroArgs::Map(v) => format!(
                "{}![{}]",
                path(&m.path),
                v.iter()
                    .map(|(k, x)| format!("{} => {}", expr(k), expr(x)))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        },
        K::Paren(x) => format!("(paren {})", expr(x)),
        K::Path { path: p, .. } => path(p),
        K::Range { start, end, closed } => format!(
            "({} {} {})",
            if *closed { "..=" } else { ".." },
            start.as_ref().map_or("_".into(), |x| expr(x)),
            end.as_ref().map_or("_".into(), |x| expr(x))
        ),
        K::Struct(s) => {
            let mut f: Vec<String> = s
                .fields
                .iter()
                .map(|f| {
                    format!(
                        "{}{}:{}",
                        if f.erased { "@" } else { "" },
                        member(&f.member),
                        expr(&f.expr)
                    )
                })
                .collect();
            if let Some(r) = &s.rest {
                f.push(format!(
                    "..{}",
                    r.as_ref().map_or(String::new(), |x| expr(x))
                ));
            }
            format!("(struct {} {})", path(&s.path), f.join(" "))
        }
        K::Try(x) => format!("(? {})", expr(x)),
        K::Tuple(v) => format!("(tuple {})", list(v)),
        K::Infer => "_".into(),
        K::Rust(t) => format!("rust`{}`", t.tokens),
        K::Quant {
            kind,
            binders,
            body,
        } => {
            let k = match kind {
                QuantKind::Forall => "forall",
                QuantKind::Exists => "exists",
            };
            let b: Vec<String> = binders
                .iter()
                .map(|b| match &b.kind {
                    BinderKind::Typed(t) => format!("{}:{}", ident(&b.ident), ty(t)),
                    BinderKind::In(x) => format!("{} in {}", ident(&b.ident), expr(x)),
                    BinderKind::Bare => ident(&b.ident),
                })
                .collect();
            format!("({k} [{}] {})", b.join(", "), expr(body))
        }
        K::AndOr { kind, exprs } => {
            let k = match kind {
                AndOrKind::And => "and",
                AndOrKind::Or => "or",
            };
            format!("({k} {})", list(exprs))
        }
        K::Perm(i) => format!("{}@", ident(i)),
        K::Erased { base, field } => format!("(@ {} {})", expr(base), ident(field)),
        K::Borrow {
            mutable,
            region: r,
            body,
        } => format!(
            "(borrow{} {} {})",
            if *mutable { " mut" } else { "" },
            region(r),
            block(body)
        ),
    };
    if e.attrs.is_empty() {
        s
    } else {
        let a: Vec<String> = e
            .attrs
            .iter()
            .map(|a| format!("#[{}]", path(&a.path)))
            .collect();
        format!("{} {s}", a.join(""))
    }
}

pub fn block(b: &Block) -> String {
    let s: Vec<String> = b.stmts.iter().map(stmt).collect();
    format!("{{{}}}", s.join(" "))
}

pub fn stmt(s: &Stmt) -> String {
    match &s.kind {
        StmtKind::Let(l) => {
            let mut out = format!("(let {}", pat(&l.pat));
            if let Some(t) = &l.ty {
                out.push_str(&format!(": {}", ty(t)));
            }
            if let Some(i) = &l.init {
                out.push_str(&format!(" {}", expr(i)));
            }
            if let Some(b) = &l.else_ {
                out.push_str(&format!(" else {}", block(b)));
            }
            out.push(')');
            out
        }
        StmtKind::Item(i) => format!("(item {})", item_head(i)),
        StmtKind::Expr { expr: x, semi } => {
            if *semi {
                format!("{};", expr(x))
            } else {
                expr(x)
            }
        }
        StmtKind::Empty => ";".into(),
        StmtKind::Take(r) => format!("(take {})", region(r)),
        StmtKind::Put(r) => format!("(put {})", region(r)),
        StmtKind::Leak(i) => format!("(leak {})", ident(i)),
        StmtKind::SpecAssign { target, value } => {
            format!("(spec= {} {})", expr(target), expr(value))
        }
    }
}

fn attr_prefix(a: &[Attr]) -> String {
    a.iter().map(|a| format!("#[{}] ", path(&a.path))).collect()
}

pub fn pat(p: &Pat) -> String {
    match &p.kind {
        PatKind::Ident {
            by_ref,
            mutable,
            ident: i,
            sub,
        } => {
            let mut s = format!(
                "{}{}{}",
                if *by_ref { "ref " } else { "" },
                if *mutable { "mut " } else { "" },
                ident(i)
            );
            if let Some(sub) = sub {
                s.push_str(&format!(" @ {}", pat(sub)));
            }
            s
        }
        PatKind::Wild => "_".into(),
        PatKind::Rest => "..".into(),
        PatKind::Lit(x) => expr(x),
        PatKind::Range { start, end, .. } => format!(
            "{}..{}",
            start.as_ref().map_or(String::new(), |x| expr(x)),
            end.as_ref().map_or(String::new(), |x| expr(x))
        ),
        PatKind::Path { path: p, .. } => path(p),
        PatKind::Tuple(v) => format!("({})", v.iter().map(pat).collect::<Vec<_>>().join(", ")),
        PatKind::TupleStruct { path: p, elems, .. } => format!(
            "{}({})",
            path(p),
            elems.iter().map(pat).collect::<Vec<_>>().join(", ")
        ),
        PatKind::Struct {
            path: p,
            fields,
            rest,
            ..
        } => {
            let mut f: Vec<String> = fields
                .iter()
                .map(|f| match f {
                    FieldPat::Field {
                        member: m,
                        pat: x,
                        shorthand,
                        ..
                    } => {
                        if *shorthand {
                            pat(x)
                        } else {
                            format!("{}: {}", member(m), pat(x))
                        }
                    }
                    FieldPat::Erased { attrs, ident: i } => {
                        format!("{}@{}", attr_prefix(attrs), ident(i))
                    }
                    FieldPat::ErasedRest { attrs, .. } => format!("{}@..", attr_prefix(attrs)),
                })
                .collect();
            if *rest {
                f.push("..".into());
            }
            format!("{} {{ {} }}", path(p), f.join(", "))
        }
        PatKind::Ref { mutable, pat: x } => {
            format!("&{}{}", if *mutable { "mut " } else { "" }, pat(x))
        }
        PatKind::Or(v) => v.iter().map(pat).collect::<Vec<_>>().join(" | "),
        PatKind::Slice(v) => format!("[{}]", v.iter().map(pat).collect::<Vec<_>>().join(", ")),
        PatKind::Paren(x) => format!("({})", pat(x)),
        PatKind::Macro(m) => format!("{}!", path(&m.path)),
        PatKind::Type { pat: x, ty: t } => format!("{}: {}", pat(x), ty(t)),
    }
}

pub fn clause(c: &Clause) -> String {
    match &c.kind {
        ClauseKind::Uses(v) => {
            let s: Vec<String> = v
                .iter()
                .map(|u| match u {
                    UsesItem::Perm { mode, ident: i, .. } => format!(
                        "{}{}@",
                        match mode {
                            PermMode::Move => "",
                            PermMode::Shared => "&",
                            PermMode::Mut => "&mut ",
                        },
                        ident(i)
                    ),
                    UsesItem::Named {
                        ident: i, ty: t, ..
                    } => format!("{}: {}", ident(i), ty(t)),
                })
                .collect();
            format!("(uses {})", s.join(", "))
        }
        ClauseKind::Gives(v) => format!(
            "(gives {})",
            v.iter()
                .map(|i| format!("{}@", ident(i)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ClauseKind::Requires(v) => format!("(requires {})", list(v)),
        ClauseKind::Ensures(v) => format!("(ensures {})", list(v)),
        ClauseKind::Returns(x) => format!("(returns {})", expr(x)),
        ClauseKind::Decreases(v) => format!("(decreases {})", list(v)),
        ClauseKind::NoPanic => "(no_panic)".into(),
    }
}

pub fn contract(c: &Contract) -> String {
    c.clauses.iter().map(clause).collect::<Vec<_>>().join(" ")
}

fn sig(s: &Signature) -> String {
    let ps: Vec<String> = s
        .params
        .iter()
        .map(|p| match p {
            Param::Receiver {
                reference,
                mutable,
                ty: t,
                ..
            } => {
                let mut r = String::new();
                if reference.is_some() {
                    r.push('&');
                }
                if *mutable {
                    r.push_str("mut ");
                }
                r.push_str("self");
                if let Some(t) = t {
                    r.push_str(&format!(": {}", ty(t)));
                }
                r
            }
            Param::Typed { pat: p, ty: t, .. } => format!("{}: {}", pat(p), ty(t)),
        })
        .collect();
    let mut out = format!("{}({})", ident(&s.ident), ps.join(", "));
    if let Some(r) = &s.ret {
        match &r.name {
            Some(n) => out.push_str(&format!(" -> ({}: {})", ident(n), ty(&r.ty))),
            None => out.push_str(&format!(" -> {}", ty(&r.ty))),
        }
    }
    if let Some(w) = &s.where_clause {
        let p: Vec<String> = w.predicates.iter().map(|p| p.tokens.to_string()).collect();
        out.push_str(&format!(" where {}", p.join(", ")));
    }
    out
}

/// 項目の見出し（種類、名前、シグネチャ、契約）。
pub fn item_head(i: &Item) -> String {
    let v = match &i.vis {
        Vis::Inherited => "",
        _ => "pub ",
    };
    let k = match &i.kind {
        ItemKind::Fn(f) => {
            let body = match &f.body {
                Some(FnBody::Block(_)) => " {..}",
                Some(FnBody::Pure(e)) => {
                    return format!(
                        "{v}{}fn {} {}= {}",
                        if f.unchecked.is_some() {
                            "unchecked "
                        } else {
                            ""
                        },
                        sig(&f.sig),
                        with_space(contract(&f.contract)),
                        expr(e)
                    );
                }
                None => ";",
            };
            format!(
                "{}fn {}{}{body}",
                if f.unchecked.is_some() {
                    "unchecked "
                } else {
                    ""
                },
                sig(&f.sig),
                pre_space(contract(&f.contract))
            )
        }
        ItemKind::SpecFn(f) => format!(
            "{}spec fn {}{}{}",
            if f.closed.is_some() { "closed " } else { "" },
            sig(&f.sig),
            pre_space(contract(&f.contract)),
            f.body
                .as_ref()
                .map_or(";".into(), |e| format!(" = {}", expr(e)))
        ),
        ItemKind::LemmaFn(f) => format!(
            "{}lemma fn {}{};",
            if f.unchecked.is_some() {
                "unchecked "
            } else {
                ""
            },
            sig(&f.sig),
            pre_space(contract(&f.contract))
        ),
        ItemKind::Struct(s) => {
            let n = |e: &Option<ErasedFields>| {
                e.as_ref().map(|e| {
                    e.fields
                        .iter()
                        .map(|f| {
                            format!(
                                "{}: {}",
                                f.ident.as_ref().map_or("_".into(), ident),
                                ty(&f.ty)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                })
            };
            let mut s2 = format!("struct {}", ident(&s.ident));
            if let Some(o) = n(&s.owns) {
                s2.push_str(&format!(" owns {{ {o} }}"));
            }
            if let Some(o) = n(&s.spec) {
                s2.push_str(&format!(" spec {{ {o} }}"));
            }
            s2
        }
        ItemKind::Enum(e) => format!("enum {}", ident(&e.ident)),
        ItemKind::Trait(t) => format!("trait {} ({} items)", ident(&t.ident), t.items.len()),
        ItemKind::Impl(im) => format!(
            "impl {}{} ({} items)",
            im.trait_
                .as_ref()
                .map_or(String::new(), |t| format!("{} for ", path(t))),
            ty(&im.self_ty),
            im.items.len()
        ),
        ItemKind::ImplSpec(im) => format!(
            "impl spec {}{} ({} items)",
            path(&im.trait_),
            im.self_ty
                .as_ref()
                .map_or(String::new(), |t| format!(" for {}", ty(t))),
            im.items.len()
        ),
        ItemKind::TypeAlias(t) => format!(
            "type {} = {}{}",
            ident(&t.ident),
            ty(&t.ty),
            t.where_clause.as_ref().map_or(String::new(), |w| format!(
                " where {}",
                w.predicates
                    .iter()
                    .map(|p| p.tokens.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        ),
        ItemKind::Subset(t) => format!(
            "subset {} = {} where {}",
            ident(&t.ident),
            ty(&t.base),
            expr(&t.pred)
        ),
        ItemKind::Mod(m) => format!("mod {}", ident(&m.ident)),
        ItemKind::Const(c) => format!("const {}", ident(&c.ident)),
        ItemKind::Static(s) => format!("static {}", ident(&s.ident)),
        ItemKind::Use(_) => "use".into(),
        ItemKind::Rust(t) => format!("rust`{}`", t.tokens),
    };
    format!("{v}{k}")
}

fn pre_space(s: String) -> String {
    if s.is_empty() { s } else { format!(" {s}") }
}

fn with_space(s: String) -> String {
    if s.is_empty() { s } else { format!("{s} ") }
}

/// ファイルの項目の見出しを並べる。
pub fn heads(src: &str) -> Vec<String> {
    ok(src).items.iter().map(item_head).collect()
}
